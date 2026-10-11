"""Шрифты окна: подмножества Noto Sans и Noto Sans Armenian.

Скачивает выпуски Noto с GitHub по закреплённым адресам, сверяет их суммы,
режет подмножества по диапазонам знаков и пишет:

- `crates/app/public/fonts/*.ttf` — по файлу на диапазон и начертание;
- `crates/app/public/fonts/OFL.txt` — лицензию шрифтов (OFL требует класть
  её рядом с каждой копией);
- `crates/app/style/fonts.css` — правила `@font-face` с теми же диапазонами,
  что и у файлов: диапазон в CSS и набор знаков в файле берутся отсюда же
  и разойтись не могут.

Запускается руками, когда меняется набор знаков, начертаний или версия Noto.
CI шрифты не пересобирает: он проверяет готовые файлы тестом
`crates/app/tests/fonts.rs`. Python 3 и fontTools ровно этой версии:

    python -m venv .venv-fonts
    .venv-fonts/Scripts/python -m pip install fonttools==4.60.1
    .venv-fonts/Scripts/python tools/fonts.py

Почему так, а не иначе (замер 2026-10-11, fontTools 4.60.1, `brotli -q 11`):

- **Диапазоны — как у Google Fonts.** Браузер скачивает файл, только когда
  на странице есть знак из его `unicode-range`: русскому окну не нужна
  расширенная латиница (51 КБ на начертание), армянскому — кириллица.
- **TTF, а не WOFF2.** WOFF2 легче TTF, сжатого brotli, примерно на 20 %
  (латиница 400: 12,9 КБ против 15,5), но Cloudflare сжимает `font/ttf` сам
  (developers.cloudflare.com, «Content compression», сверено 2026-10-11),
  а PDF (задача 12) встраивает именно TTF: окно и документ берут одни и те же
  файлы (Правило 4), и человек не скачивает шрифт дважды. Тест читает ровно
  те байты, что получит браузер.
- **Два статических начертания, 400 и 600, а не переменный шрифт.**
  Переменный с осью веса 400–700 после brotli легче двух статических
  (латиница 26,9 КБ против 31,3), но переменные шрифты понимает не всякая
  библиотека PDF, а статические — любая.
"""

import hashlib
import io
import pathlib
import sys
import urllib.request
import zipfile

from fontTools import subset
from fontTools.ttLib import TTFont

ROOT = pathlib.Path(__file__).resolve().parent.parent
FONTS = ROOT / "crates" / "app" / "public" / "fonts"
CSS = ROOT / "crates" / "app" / "style" / "fonts.css"

# Выпуски Noto. Сумма архива записана здесь, а не скачивается рядом с ним:
# подменённый выпуск принёс бы и подменённую сумму (как Trunk в ci.yml).
# Суммы посчитаны по скачанным архивам 2026-10-11.
RELEASES = {
    "latin": (
        "https://github.com/notofonts/latin-greek-cyrillic/releases/download/"
        "NotoSans-v2.015/NotoSans-v2.015.zip",
        "0c34df072a3fa7efbb7cbf34950e1f971a4447cffe365d3a359e2d4089b958f5",
        "NotoSans/unhinted/ttf/NotoSans-{style}.ttf",
    ),
    "armenian": (
        "https://github.com/notofonts/armenian/releases/download/"
        "NotoSansArmenian-v2.008/NotoSansArmenian-v2.008.zip",
        "eab89b99e134177ca6a3f9f0412a7cb812aafceb13175d686b4c45cb237f64ac",
        "NotoSansArmenian/unhinted/ttf/NotoSansArmenian-{style}.ttf",
    ),
}

# Начертания: вес CSS и имя файла в выпуске. Другого веса в окне нет —
# браузер нарисовал бы его сам, утолщением, а это выглядит грязно.
WEIGHTS = {400: "Regular", 600: "SemiBold"}

# Диапазоны знаков — как у Google Fonts для Noto Sans (fonts.googleapis.com,
# 2026-10-11). Имя — часть имени файла; второе — из какого выпуска.
RANGES = {
    "latin": (
        "latin",
        "U+0000-00FF, U+0131, U+0152-0153, U+02BB-02BC, U+02C6, U+02DA, "
        "U+02DC, U+0304, U+0308, U+0329, U+2000-206F, U+20AC, U+2122, U+2191, "
        "U+2193, U+2212, U+2215, U+FEFF, U+FFFD",
    ),
    "latin-ext": (
        "latin",
        "U+0100-02BA, U+02BD-02C5, U+02C7-02CC, U+02CE-02D7, U+02DD-02FF, "
        "U+0304, U+0308, U+0329, U+1D00-1DBF, U+1E00-1E9F, U+1EF2-1EFF, "
        "U+2020, U+20A0-20AB, U+20AD-20C0, U+2113, U+2C60-2C7F, U+A720-A7FF",
    ),
    "cyrillic": (
        "latin",
        "U+0301, U+0400-045F, U+0490-0491, U+04B0-04B1, U+2116",
    ),
    "armenian": (
        "armenian",
        "U+0530-058F, U+FB13-FB17",
    ),
}

# Возможности OpenType, которые остаются в подмножестве: кернинг и знаки
# с диакритикой, лигатуры, табличные и пропорциональные цифры (Правило 4:
# `tabular-nums` работает, только если в шрифте есть `tnum`), прописные
# формы знаков.
FEATURES = ["kern", "mark", "mkmk", "liga", "ccmp", "locl", "tnum", "lnum", "pnum", "case"]

FAMILY = "Kvartik Sans"


def codepoints(ranges: str) -> list[int]:
    out = []
    for part in ranges.split(","):
        lo, _, hi = part.strip().removeprefix("U+").partition("-")
        out.extend(range(int(lo, 16), int(hi or lo, 16) + 1))
    return out


def download(url: str, sha256: str) -> zipfile.ZipFile:
    # В памяти, а не во временной папке: архивы — около 120 МБ, но открытый
    # файл Windows удалить не даёт.
    with urllib.request.urlopen(url) as response:
        data = response.read()
    digest = hashlib.sha256(data).hexdigest()
    if digest != sha256:
        sys.exit(f"{url}: сумма {digest}, а ждали {sha256}")
    return zipfile.ZipFile(io.BytesIO(data))


def make_subset(source: bytes, ranges: str) -> bytes:
    # Без пересчёта даты изменения: иначе каждая пересборка давала бы другие
    # байты при тех же знаках, и в git ходили бы пустые изменения.
    font = TTFont(io.BytesIO(source), recalcTimestamp=False)
    options = subset.Options()
    options.layout_features = FEATURES
    # Все записи `name`: в них авторские права и лицензия (OFL, условие 2),
    # а по умолчанию fontTools оставляет только первые семь.
    options.name_IDs = ["*"]
    options.notdef_outline = True
    options.hinting = False
    options.drop_tables += ["DSIG"]
    subsetter = subset.Subsetter(options)
    subsetter.populate(unicodes=codepoints(ranges))
    subsetter.subset(font)
    out = io.BytesIO()
    font.save(out)
    return out.getvalue()


def main() -> None:
    # Консоль Windows по умолчанию пишет в cp1252, и кириллица в выводе
    # роняла бы скрипт.
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")
    FONTS.mkdir(parents=True, exist_ok=True)
    CSS.parent.mkdir(parents=True, exist_ok=True)
    archives = {
        name: (download(url, sha), member) for name, (url, sha, member) in RELEASES.items()
    }
    licenses = [archive.read("OFL.txt").decode("utf-8") for archive, _ in archives.values()]
    rules = []
    for weight, style in WEIGHTS.items():
        for name, (release, ranges) in RANGES.items():
            archive, member = archives[release]
            data = make_subset(archive.read(member.format(style=style)), ranges)
            file = f"noto-sans-{name}-{weight}.ttf"
            (FONTS / file).write_bytes(data)
            print(f"{file:32} {len(data):7} Б")
            rules.append(
                "@font-face {\n"
                f'  font-family: "{FAMILY}";\n'
                "  font-style: normal;\n"
                f"  font-weight: {weight};\n"
                "  font-display: swap;\n"
                f'  src: url("fonts/{file}") format("truetype");\n'
                f"  unicode-range: {ranges};\n"
                "}\n"
            )
    # Тексты OFL у двух выпусков отличаются только строкой авторских прав.
    copyrights = [text.split("\n", 1)[0] for text in licenses]
    body = licenses[0].split("\n", 1)[1]
    if any(text.split("\n", 1)[1] != body for text in licenses):
        sys.exit("тексты OFL у выпусков разошлись — сверить руками")
    (FONTS / "OFL.txt").write_text("\n".join(copyrights) + "\n" + body, encoding="utf-8", newline="\n")
    header = (
        "/* Шрифты окна — Noto Sans и Noto Sans Armenian под одним именем\n"
        f'   «{FAMILY}». Файл пишет tools/fonts.py — руками не править: диапазоны\n'
        "   здесь обязаны совпадать с набором знаков в файлах, и это держит тест\n"
        "   crates/app/tests/fonts.rs. Почему TTF, два начертания и диапазоны\n"
        "   Google Fonts — в шапке tools/fonts.py.\n"
        "\n"
        "   `swap`: текст виден сразу системным шрифтом и подменяется, когда\n"
        "   файл пришёл, — пустой страницы на медленной сети нет (Правило 1). */\n"
    )
    CSS.write_text(header + "\n" + "\n".join(rules), encoding="utf-8", newline="\n")


if __name__ == "__main__":
    main()
