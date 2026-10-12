"""Таблица стран: коды ISO 3166-1 alpha-2 и названия на трёх языках из CLDR.

Скачивает файлы CLDR по закреплённым адресам, сверяет их суммы и пишет
`crates/core/src/model/country/names.rs` — таблицу, которую читает
`model/country.rs`. Страна клиента хранится кодом (`"AM"`), а название
берётся из таблицы на языке окна или документа: решение человека
2026-10-12 (задача 10) — своя таблица в ядре, а не `Intl.DisplayNames`
браузера: названия нужны и PDF (задача 12), а у браузеров они разные.

Запускается руками, когда выходит новый CLDR или меняется ISO 3166-1. CI
таблицу не пересобирает: готовый файл лежит в репозитории, а знаки названий
проверяет тест шрифтов `crates/app/tests/fonts.rs`. Только стандартная
библиотека Python 3:

    python -I -X utf8 tools/countries.py

Что берётся (CLDR 48.2.3, сверено 2026-10-13):

- **Коды — назначенные коды ISO 3166-1 alpha-2 и Косово.** Из
  `codeMappings.json` — две латинские буквы с числовым кодом ISO меньше 900
  (900–999 стандарт оставляет пользователям: `EU`, `XK`, `ZZ`), минус
  устаревшие — те, что CLDR переводит в новые (`territoryAlias`
  в `aliases.json`: `SU`, `YU`, `AN`…). Без числового кода остаются
  и исключительно зарезервированные `AC`, `CP`, `DG`, `EA`, `IC`, `TA`.
  Назначенных вышло 249 — столько их в ISO 3166-1 на 2026 год, и скрипт
  останавливается, если число изменилось: новая страна — повод посмотреть
  глазами. Сверх них — Косово, `XK` ([`EXTRA`]). Новый код в таблице —
  новая версия формата данных: прежняя версия программы прочла бы его как
  порчу (шапка `model/country.rs`).
- **Названия — основные**, без вариантов `-alt-short` и `-alt-variant`:
  «Hong Kong SAR China», а не «Hong Kong». Так же их показывает
  `Intl.DisplayNames` в Chrome и Firefox.
- **Порядок по названию на каждом языке** считается здесь, а не в окне:
  в ядре нет таблиц сравнения Unicode, а тащить их в wasm ради одного
  списка дорого (Правило 1). Сравнение — без учёта регистра; в английском
  без диакритики («Åland» стоит у «A»), в русском «ё» — как «е», но «й» —
  своя буква («Йемен» после «Исландии»), в армянском «և» — как «եւ».
  Армянский и русский алфавиты в Unicode идут по порядку, и этого хватает:
  порядок совпадает с `Intl.Collator` (ICU в Node 24) на всех трёх языках
  (проверено 2026-10-13).
"""

import hashlib
import json
import pathlib
import re
import sys
import unicodedata
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / "crates" / "core" / "src" / "model" / "country" / "names.rs"

CLDR = "48.2.3"
BASE = f"https://raw.githubusercontent.com/unicode-org/cldr-json/{CLDR}/cldr-json"

# Файлы CLDR. Сумма записана здесь, а не скачивается рядом с файлом:
# подменённый файл принёс бы и подменённую сумму (как шрифты в fonts.py).
# Суммы посчитаны по скачанным файлам 2026-10-13.
FILES = {
    "en": (
        f"{BASE}/cldr-localenames-full/main/en/territories.json",
        "158c1d575308f7e46912edbeda435c8fe2ef5dad280798231f3a432e406b1807",
    ),
    "ru": (
        f"{BASE}/cldr-localenames-full/main/ru/territories.json",
        "305ba1f9eecb1b0151ab081a111aaa8a5898d38c9d57bfbfa125734e508a2389",
    ),
    "hy": (
        f"{BASE}/cldr-localenames-full/main/hy/territories.json",
        "56d94172f2f07555f25097ec2ef44e0ad38e740a64d2ac59d6658ffc05b3196e",
    ),
    "codes": (
        f"{BASE}/cldr-core/supplemental/codeMappings.json",
        "0d1ef50b92c1140e5847d22d96faf1a9c35543b0dedf8e9a2fcb87e4c51b9ed6",
    ),
    "aliases": (
        f"{BASE}/cldr-core/supplemental/aliases.json",
        "b7011c866f27502ed6f160ca83f2a83fa42bce9fdc2176e60e2c6630961d877c",
    ),
}

# Языки в порядке `Lang::ALL` ядра: армянский, русский, английский.
LANGS = ["hy", "ru", "en"]

# Назначенных кодов ISO 3166-1 alpha-2 на 2026 год.
EXPECTED = 249

# Коды сверх назначенных. Косово ISO кода не назначило, но `XK` пишут
# Еврокомиссия и банки (IBAN Косово начинается с XK), а у CLDR есть его
# названия на всех трёх языках. Клиент из Косово — настоящий клиент, и без
# кода ему не выбрать страну (решение человека 2026-10-13, 0.0.15).
EXTRA = ["XK"]


def download(url: str, sha256: str) -> dict:
    with urllib.request.urlopen(url) as response:
        data = response.read()
    digest = hashlib.sha256(data).hexdigest()
    if digest != sha256:
        sys.exit(f"{url}: сумма {digest}, а ждали {sha256}")
    return json.loads(data.decode("utf-8"))


def collation_key(lang: str, name: str) -> str:
    text = name.casefold()
    if lang == "ru":
        # «й» — своя буква, а «ё» — та же «е»: разложение Unicode сделало бы
        # обе буквы «и» и «е» с точками.
        return text.replace("ё", "е")
    text = unicodedata.normalize("NFKD", text)
    return "".join(c for c in text if not unicodedata.combining(c))


def rust_str(text: str) -> str:
    if any(c in text for c in '"\\') or any(unicodedata.category(c).startswith("C") for c in text):
        sys.exit(f"название с кавычкой, обратной чертой или управляющим знаком: {text!r}")
    return f'"{text}"'


def main() -> None:
    # Консоль Windows по умолчанию пишет в cp1252, и кириллица в выводе
    # роняла бы скрипт.
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")
    data = {name: download(url, sha) for name, (url, sha) in FILES.items()}
    mappings = data["codes"]["supplemental"]["codeMappings"]
    aliases = data["aliases"]["supplemental"]["metadata"]["alias"]["territoryAlias"]
    names = {
        lang: data[lang]["main"][lang]["localeDisplayNames"]["territories"] for lang in LANGS
    }
    assigned = [
        code
        for code, mapping in mappings.items()
        if re.fullmatch("[A-Z]{2}", code)
        and int(mapping.get("_numeric", "999")) < 900
        and code not in aliases
    ]
    if len(assigned) != EXPECTED:
        sys.exit(f"кодов {len(assigned)}, а ждали {EXPECTED}: ISO 3166-1 изменился — сверить глазами")
    codes = sorted(set(assigned) | set(EXTRA))
    missing = [f"{code} ({lang})" for code in codes for lang in LANGS if code not in names[lang]]
    if missing:
        sys.exit(f"нет названий: {missing}")

    rows = []
    for code in codes:
        fields = ", ".join(f"{lang}: {rust_str(names[lang][code])}" for lang in LANGS)
        rows.append(f'    Names {{ code: *b"{code}", {fields} }},\n')
    orders = []
    for lang in LANGS:
        order = sorted(range(len(codes)), key=lambda i: (collation_key(lang, names[lang][codes[i]]), codes[i]))
        numbers = [str(i) for i in order]
        lines, line = [], "   "
        for number in numbers:
            if len(line) + len(number) + 2 > 100:
                lines.append(line.rstrip() + "\n")
                line = "   "
            line += f" {number},"
        lines.append(line.rstrip() + "\n")
        orders.append(
            f"/// Страны по алфавиту названий на языке `{lang}` — номера строк [`COUNTRIES`].\n"
            f"pub(super) const BY_NAME_{lang.upper()}: [u8; {len(codes)}] = [\n"
            + "".join(lines)
            + "];\n"
        )

    header = (
        "// Таблица стран — файл пишет tools/countries.py, руками не править.\n"
        f"// Коды ISO 3166-1 alpha-2 и названия из CLDR {CLDR} (unicode-org/cldr-json);\n"
        "// что берётся и почему — в шапке скрипта. Данные CLDR — под лицензией\n"
        "// Unicode (unicode.org/license.txt).\n"
        "\n"
        "use super::Names;\n"
        "\n"
        "/// Страны по алфавиту кодов.\n"
        f"pub(super) const COUNTRIES: [Names; {len(codes)}] = [\n"
    )
    text = header + "".join(rows) + "];\n\n" + "\n".join(orders)
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(text, encoding="utf-8", newline="\n")
    print(f"{OUT.relative_to(ROOT)}: стран {len(codes)}, {len(text.encode('utf-8'))} Б")


if __name__ == "__main__":
    main()
