"""Шрифты окна: Noto Sans для текста, Noto Serif для заголовков, Caprasimo
для крупных сумм.

Скачивает выпуски Noto с GitHub и Caprasimo из google/fonts по закреплённым
адресам, сверяет их суммы, режет подмножества по диапазонам знаков и пишет:

- `crates/app/public/fonts/*.ttf` — по файлу на диапазон и начертание;
- `crates/app/public/fonts/OFL.txt` — лицензию шрифтов (OFL требует класть
  её рядом с каждой копией);
- `crates/app/style/fonts.css` — правила `@font-face` с теми же диапазонами,
  что и у файлов: диапазон в CSS и набор знаков в файле берутся отсюда же
  и разойтись не могут.

Запускается руками, когда меняется набор знаков, начертаний или версия
шрифтов. CI шрифты не пересобирает: он проверяет готовые файлы тестом
`crates/app/tests/fonts.rs`. Python 3 и fontTools ровно этой версии:

    python -m venv .venv-fonts
    .venv-fonts/Scripts/python -m pip install fonttools==4.60.1
    .venv-fonts/Scripts/python tools/fonts.py

Семейства — по дизайну «Kvartik Design» (решение человека 2026-10-14):

- **«Kvartik Sans»** — Noto Sans и Noto Sans Armenian, 400 и 600: весь текст
  окна (решение человека 2026-10-11).
- **«Kvartik Serif»** — Noto Serif и Noto Serif Armenian, 800: заголовки.
  В дизайне заголовки — Noto Serif Armenian, а в нём нет кириллицы; Noto Serif
  — та же рисовка для латиницы и кириллицы. Расширенной латиницы нет: в
  заголовках её почти не бывает, а файл service worker скачал бы каждому
  (Правило 1) — редкий знак возьмётся из следующего семейства стека.
- **«Kvartik Display»** — Caprasimo, 400: «Kvartik» и крупные суммы. В файле
  — только их знаки (`DISPLAY`), а остальное, и ֏ тоже (его в Caprasimo нет
  вовсе), берёт следующее семейство стека (`--font-display` в tokens.css).
  Поэтому диапазон в CSS у него — ровно знаки файла, а не диапазон Google
  Fonts: знак, которого в файле нет, браузер сразу ищет дальше по стеку.

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
- **Статические начертания, а не переменный шрифт.** Переменный с осью веса
  400–700 после brotli легче двух статических (латиница 26,9 КБ против
  31,3), но переменные шрифты понимает не всякая библиотека PDF, а
  статические — любая.
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

# Выпуски Noto: архив, его сумма и путь к файлу начертания внутри. Сумма
# записана здесь, а не скачивается рядом с архивом: подменённый выпуск принёс
# бы и подменённую сумму (как Trunk в ci.yml). Суммы посчитаны по скачанным
# архивам: Noto Sans — 2026-10-11, Noto Serif — 2026-10-14.
ARCHIVES = {
    "noto-sans": (
        "https://github.com/notofonts/latin-greek-cyrillic/releases/download/"
        "NotoSans-v2.015/NotoSans-v2.015.zip",
        "0c34df072a3fa7efbb7cbf34950e1f971a4447cffe365d3a359e2d4089b958f5",
        "NotoSans/unhinted/ttf/NotoSans-{style}.ttf",
    ),
    "noto-sans-armenian": (
        "https://github.com/notofonts/armenian/releases/download/"
        "NotoSansArmenian-v2.008/NotoSansArmenian-v2.008.zip",
        "eab89b99e134177ca6a3f9f0412a7cb812aafceb13175d686b4c45cb237f64ac",
        "NotoSansArmenian/unhinted/ttf/NotoSansArmenian-{style}.ttf",
    ),
    "noto-serif": (
        "https://github.com/notofonts/latin-greek-cyrillic/releases/download/"
        "NotoSerif-v2.015/NotoSerif-v2.015.zip",
        "0e9a43c8a4b94ac76f55069ed1d7385bbcaf6b99527a94deb5619e032b7e76c1",
        "NotoSerif/unhinted/ttf/NotoSerif-{style}.ttf",
    ),
    "noto-serif-armenian": (
        "https://github.com/notofonts/armenian/releases/download/"
        "NotoSerifArmenian-v2.008/NotoSerifArmenian-v2.008.zip",
        "b21c198a4c76ae598a304decefb3b5c2a4c2d4c3ae226728eff359185f291c6f",
        "NotoSerifArmenian/unhinted/ttf/NotoSerifArmenian-{style}.ttf",
    ),
}

# Шрифты из google/fonts: архива выпуска у них нет, и каждый файл закреплён
# коммитом репозитория и своей суммой (посчитаны 2026-10-14). Caprasimo —
# The DocRepair Project, Phaedra Charles, Flavia Zimbardi; OFL 1.1
# (METADATA.pb того же коммита).
GOOGLE = (
    "https://raw.githubusercontent.com/google/fonts/"
    "3dd78844021e948ceb633d1dcee3f7885561b5d9/ofl/"
)
SINGLES = {
    "caprasimo": (
        {
            "Regular": (
                GOOGLE + "caprasimo/Caprasimo-Regular.ttf",
                "786ab84ee787d40df55be78cb4361e485a1c6687195cbcccf3538cb5e46ced90",
            ),
        },
        (
            GOOGLE + "caprasimo/OFL.txt",
            "755d165efdbad1ad4a7840aa1f2484b0111627993e162bdcfb25fb2c277db955",
        ),
    ),
}

# Диапазоны знаков — как у Google Fonts для Noto Sans (fonts.googleapis.com,
# 2026-10-11).
LATIN = (
    "U+0000-00FF, U+0131, U+0152-0153, U+02BB-02BC, U+02C6, U+02DA, "
    "U+02DC, U+0304, U+0308, U+0329, U+2000-206F, U+20AC, U+2122, U+2191, "
    "U+2193, U+2212, U+2215, U+FEFF, U+FFFD"
)
LATIN_EXT = (
    "U+0100-02BA, U+02BD-02C5, U+02C7-02CC, U+02CE-02D7, U+02DD-02FF, "
    "U+0304, U+0308, U+0329, U+1D00-1DBF, U+1E00-1E9F, U+1EF2-1EFF, "
    "U+2020, U+20A0-20AB, U+20AD-20C0, U+2113, U+2C60-2C7F, U+A720-A7FF"
)
CYRILLIC = "U+0301, U+0400-045F, U+0490-0491, U+04B0-04B1, U+2116"
ARMENIAN = "U+0530-058F, U+FB13-FB17"

# Знаки, которые окно пишет шрифтом «Kvartik Display»: суммы — цифры, знак
# между разрядами и перед дробной частью, минус, `$`, `€` и `AMD`
# (`i18n::money`), — и «Kvartik» (`Key::Brand`). Больше файлу ничего не
# нужно: подмножество весит 4,8 КБ после brotli вместо 24 у всей латиницы
# (замер 2026-10-14). Новое слово крупным шрифтом — сначала сюда, иначе
# красным станет тест crates/app/tests/fonts.rs.
DISPLAY = "0123456789  ,.-+%$€AMDKvartik"

# Семейства: имя в CSS, начало имён файлов, начертания (вес CSS — стиль
# в выпуске), подмножества (имя — источник и диапазон; без диапазона — знаки
# `DISPLAY`) и диапазон в CSS — ровно знаки файла (`True`) или диапазон
# Google Fonts. Другого веса у
# семейства нет — браузер нарисовал бы его сам, утолщением, а это выглядит
# грязно.
FAMILIES = [
    (
        "Kvartik Sans",
        "noto-sans",
        {400: "Regular", 600: "SemiBold"},
        [
            ("latin", "noto-sans", LATIN),
            ("latin-ext", "noto-sans", LATIN_EXT),
            ("cyrillic", "noto-sans", CYRILLIC),
            ("armenian", "noto-sans-armenian", ARMENIAN),
        ],
        False,
    ),
    (
        "Kvartik Serif",
        "noto-serif",
        {800: "ExtraBold"},
        [
            ("latin", "noto-serif", LATIN),
            ("cyrillic", "noto-serif", CYRILLIC),
            ("armenian", "noto-serif-armenian", ARMENIAN),
        ],
        False,
    ),
    (
        "Kvartik Display",
        "caprasimo",
        {400: "Regular"},
        [("latin", "caprasimo", None)],
        True,
    ),
]

# Возможности OpenType, которые остаются в подмножестве: кернинг и знаки
# с диакритикой, лигатуры, табличные и пропорциональные цифры (Правило 4:
# `tabular-nums` работает, только если в шрифте есть `tnum`), прописные
# формы знаков.
FEATURES = ["kern", "mark", "mkmk", "liga", "ccmp", "locl", "tnum", "lnum", "pnum", "case"]


def codepoints(ranges: str) -> list[int]:
    out = []
    for part in ranges.split(","):
        lo, _, hi = part.strip().removeprefix("U+").partition("-")
        out.extend(range(int(lo, 16), int(hi or lo, 16) + 1))
    return out


def ranges_of(chars: list[int]) -> str:
    """Знаки — записью `unicode-range`: подряд идущие сливаются в диапазон."""
    parts = []
    start = previous = None
    for code in sorted(chars):
        if previous is not None and code == previous + 1:
            previous = code
            continue
        if start is not None:
            parts.append((start, previous))
        start = previous = code
    if start is not None:
        parts.append((start, previous))
    return ", ".join(
        f"U+{lo:04X}" if lo == hi else f"U+{lo:04X}-{hi:04X}" for lo, hi in parts
    )


def download(url: str, sha256: str) -> bytes:
    # В памяти, а не во временной папке: архивы — около 120 МБ, но открытый
    # файл Windows удалить не даёт.
    with urllib.request.urlopen(url) as response:
        data = response.read()
    digest = hashlib.sha256(data).hexdigest()
    if digest != sha256:
        sys.exit(f"{url}: сумма {digest}, а ждали {sha256}")
    return data


class Archive:
    """Выпуск Noto: zip со шрифтами и OFL.txt."""

    def __init__(self, url: str, sha256: str, member: str) -> None:
        self.zip = zipfile.ZipFile(io.BytesIO(download(url, sha256)))
        self.member = member

    def font(self, style: str) -> bytes:
        return self.zip.read(self.member.format(style=style))

    def license(self) -> str:
        return self.zip.read("OFL.txt").decode("utf-8")


class Singles:
    """Шрифт из google/fonts: файлы по одному, каждый со своей суммой."""

    def __init__(self, fonts: dict, license: tuple) -> None:
        self.fonts = {style: download(*source) for style, source in fonts.items()}
        self.license_text = download(*license).decode("utf-8")

    def font(self, style: str) -> bytes:
        return self.fonts[style]

    def license(self) -> str:
        return self.license_text


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


def chars_of(data: bytes) -> list[int]:
    """Знаки, у которых в файле есть глиф."""
    return sorted(TTFont(io.BytesIO(data)).getBestCmap())


def main() -> None:
    # Консоль Windows по умолчанию пишет в cp1252, и кириллица в выводе
    # роняла бы скрипт.
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")
    FONTS.mkdir(parents=True, exist_ok=True)
    CSS.parent.mkdir(parents=True, exist_ok=True)
    sources = {name: Archive(*source) for name, source in ARCHIVES.items()}
    sources.update({name: Singles(*source) for name, source in SINGLES.items()})
    rules = []
    for family, prefix, weights, subsets, exact in FAMILIES:
        for weight, style in weights.items():
            for name, source, ranges in subsets:
                # Без диапазона — знаки `DISPLAY`.
                ranges = ranges or ranges_of(sorted({ord(c) for c in DISPLAY}))
                data = make_subset(sources[source].font(style), ranges)
                file = f"{prefix}-{name}-{weight}.ttf"
                (FONTS / file).write_bytes(data)
                print(f"{file:32} {len(data):7} Б")
                shown = ranges_of(chars_of(data)) if exact else ranges
                rules.append(
                    "@font-face {\n"
                    f'  font-family: "{family}";\n'
                    "  font-style: normal;\n"
                    f"  font-weight: {weight};\n"
                    "  font-display: swap;\n"
                    f'  src: url("fonts/{file}") format("truetype");\n'
                    f"  unicode-range: {shown};\n"
                    "}\n"
                )
    # Тексты OFL у выпусков отличаются только строкой авторских прав.
    licenses = [source.license() for source in sources.values()]
    copyrights = list(dict.fromkeys(text.split("\n", 1)[0] for text in licenses))
    body = licenses[0].split("\n", 1)[1]
    if any(text.split("\n", 1)[1] != body for text in licenses):
        sys.exit("тексты OFL у выпусков разошлись — сверить руками")
    (FONTS / "OFL.txt").write_text("\n".join(copyrights) + "\n" + body, encoding="utf-8", newline="\n")
    header = (
        "/* Шрифты окна: «Kvartik Sans» — Noto Sans и Noto Sans Armenian, текст;\n"
        "   «Kvartik Serif» — Noto Serif и Noto Serif Armenian, заголовки;\n"
        "   «Kvartik Display» — Caprasimo, «Kvartik» и крупные суммы. Файл пишет\n"
        "   tools/fonts.py — руками не править: диапазоны здесь обязаны совпадать\n"
        "   с набором знаков в файлах, и это держит тест crates/app/tests/fonts.rs.\n"
        "   Почему TTF, статические начертания и диапазоны Google Fonts — в шапке\n"
        "   tools/fonts.py.\n"
        "\n"
        "   `swap`: текст виден сразу системным шрифтом и подменяется, когда\n"
        "   файл пришёл, — пустой страницы на медленной сети нет (Правило 1). */\n"
    )
    CSS.write_text(header + "\n" + "\n".join(rules), encoding="utf-8", newline="\n")


if __name__ == "__main__":
    main()
