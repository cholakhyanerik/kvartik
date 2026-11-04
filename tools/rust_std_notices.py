"""Уведомление стандартной библиотеки Rust для файла лицензий сайта (задача 51).

Пишет `crates/sw/licenses/rust-std.txt`, а программа сборки сайта
(`crates/sw/src/licenses.rs`) кладёт его в `licenses.txt` рядом
с уведомлениями крейтов. Стандартная библиотека — `std`, `core`, `alloc`,
`compiler_builtins` и крейты, на которых она стоит, — скомпилирована в каждый
wasm, а под MIT это копия, к которой прилагается уведомление. Cargo её не
видит: она приходит готовой из `rust-std`, а не из `Cargo.lock` проекта
(решение человека 2026-11-04 — уведомление писать отдельно).

Запускается руками, когда поднимается `rust-version` (CLAUDE.md, Правило 3):
программа сборки сверяет версию в первой строке файла с `rust-version`
и без свежего файла сайт не собирает. CI его не пересобирает: готовый файл
лежит в репозитории. Только стандартная библиотека Python 3:

    py -3 -I -X utf8 tools/rust_std_notices.py

Что берётся — всё из установленного Rust, той версии, что в системе:

- **Состав** — крейты, на которых по `library/Cargo.lock` (компонент
  `rust-src`) стоит `std`, и только те, чьи `rlib` лежат среди готовой
  библиотеки цели `wasm32-unknown-unknown` (`lib/rustlib/<цель>/lib`):
  у других целей свои крейты (wasi, uefi). Сколько их кода пережило
  компоновку, не важно: лишнее уведомление условий не нарушает,
  недостающее — нарушает.
- **Слова проекта Rust о его файлах** — из `COPYRIGHT-library.html`
  (компонент `rust-docs`, `share/doc/rust/`): короткая и длинная версии
  и список «In-tree files», кроме файлов других целей ([`OTHER_TARGETS`]).
  Тексты лицензий из этого списка — из `share/doc/rust/licenses/`, а из
  «A OR MIT» — MIT, как у крейтов. Другая лицензия в списке — остановка:
  её смотрят глазами.
- **Тексты крейтов** — из их пакетов на crates.io: сумма пакета сверяется
  с `checksum` того же `library/Cargo.lock`, иначе скрипт останавливается.
  Из пакета берутся файлы лицензии MIT — у всех этих крейтов она среди
  вариантов, и так же выбирает программа сборки, — а нет их — все файлы
  лицензии пакета.
"""

import hashlib
import html
import io
import pathlib
import re
import subprocess
import sys
import tarfile
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / "crates" / "sw" / "licenses" / "rust-std.txt"
TARGET = "wasm32-unknown-unknown"
# Корни, от которых считается состав: что попадает в программу. `test`
# и `proc_macro` в программу не попадают.
ROOTS = ["std"]
LICENSE_NAME = re.compile(r"^(licen[cs]e|copying|copyright|notice|unlicense)", re.I)
# Файлы списка «In-tree files», которые в std этой цели не компилируются:
# мьютекс Fuchsia — только `target_os = "fuchsia"` (library/std/src/sys/sync/
# mutex/mod.rs, Rust 1.99.1).
OTHER_TARGETS = {"library/std/src/sys/sync/mutex/fuchsia.rs"}
# Лицензии, тексты которых могут понадобиться файлам списка после выбора MIT.
KNOWN = {"MIT", "Unicode-3.0"}


def run(*args: str) -> str:
    return subprocess.run(args, capture_output=True, check=True).stdout.decode("utf-8").strip()


def lock_packages(lock: str) -> list[dict]:
    """Пакеты `Cargo.lock`: имя, версия, источник, сумма и зависимости."""
    packages = []
    for block in lock.split("[[package]]")[1:]:
        fields = dict(re.findall(r'^(\w+) = "([^"]*)"$', block, re.M))
        deps = re.search(r"^dependencies = \[(.*?)\]", block, re.M | re.S)
        fields["deps"] = re.findall(r'"([^"]+)"', deps.group(1)) if deps else []
        packages.append(fields)
    return packages


def closure(packages: list[dict]) -> list[dict]:
    """Пакеты, до которых доходят зависимости от [`ROOTS`]."""
    by_name: dict[str, list[dict]] = {}
    for package in packages:
        by_name.setdefault(package["name"], []).append(package)

    def find(spec: str) -> dict:
        name, _, version = spec.partition(" ")
        found = [p for p in by_name[name] if not version or p["version"] == version.split(" ")[0]]
        if len(found) != 1:
            sys.exit(f"в library/Cargo.lock не один пакет «{spec}»")
        return found[0]

    seen: dict[tuple[str, str], dict] = {}
    todo = [find(root) for root in ROOTS]
    while todo:
        package = todo.pop()
        key = (package["name"], package["version"])
        if key in seen:
            continue
        seen[key] = package
        todo.extend(find(dep) for dep in package["deps"])
    return sorted(seen.values(), key=lambda p: (p["name"], p["version"]))


def built_for_target(sysroot: pathlib.Path) -> set[str]:
    """Имена крейтов (через `_`), чьи `rlib` лежат в готовой библиотеке цели."""
    folder = sysroot / "lib" / "rustlib" / TARGET / "lib"
    names = set()
    for path in folder.glob("lib*.rlib"):
        names.add(re.sub(r"-[0-9a-f]+$", "", path.stem[3:]))
    if not names:
        sys.exit(f"в {folder} нет rlib: rustup target add {TARGET}")
    return names


def crate_files(name: str, version: str, checksum: str) -> list[tuple[str, str]]:
    """Файлы лицензии пакета с crates.io: сначала сверка суммы с замком."""
    url = f"https://static.crates.io/crates/{name}/{name}-{version}.crate"
    data = urllib.request.urlopen(url, timeout=60).read()
    if hashlib.sha256(data).hexdigest() != checksum:
        sys.exit(f"{url}: сумма не та, что в library/Cargo.lock")
    files = []
    with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as archive:
        for member in archive.getmembers():
            parts = member.name.split("/")
            if member.isfile() and len(parts) == 2 and LICENSE_NAME.match(parts[1]):
                text = archive.extractfile(member).read().decode("utf-8")
                files.append((parts[1], text.replace("\r\n", "\n").strip()))
    files.sort()
    mit = [f for f in files if "MIT" in f[0].upper()]
    chosen = mit or files
    if not chosen:
        sys.exit(f"в пакете {name} {version} нет файла лицензии")
    return chosen


def text(fragment: str) -> str:
    """Текст HTML без разметки, абзацы — через пустую строку."""
    fragment = re.sub(r"</p>\s*", "\n\n", fragment)
    fragment = re.sub(r"<[^>]+>", "", fragment)
    lines = [re.sub(r"\s+", " ", html.unescape(line)).strip() for line in fragment.split("\n\n")]
    return "\n\n".join(line for line in lines if line)


def chosen(expression: str) -> list[str]:
    """Лицензии выражения SPDX после выбора: из «A OR MIT» — MIT, «AND»
    — все части. Скобок глубже одного уровня в списке нет."""
    result = []
    for part in re.split(r"\s+AND\s+", expression.strip("() ")):
        options = [o.strip("() ") for o in re.split(r"\s+OR\s+", part)]
        result.append("MIT" if "MIT" in options else options[0])
    return result


def in_tree(page: str) -> tuple[list[str], set[str]]:
    """Список «In-tree files»: путь, лицензия, авторские права — и лицензии,
    чьи тексты нужны. Исключение внутри папки — путём от неё: оно вложено
    в её блок."""
    start = page.index('<h2 id="in-tree-files">')
    end = page.index('<h2 id="out-of-tree-dependencies">')
    section = page[start:end]
    entries = []
    depth = 0
    parents: list[str] = []
    for token in re.finditer(r"<div\b|</div>|<b>File/Directory:</b>\s*<code>([^<]*)</code>|"
                             r"<b>License:</b>\s*([^<]*)</p>|<b>Copyright:</b>\s*([^<]*)</p>", section):
        if token.group(0).startswith("<div"):
            depth += 1
        elif token.group(0) == "</div>":
            depth -= 1
            del parents[depth:]
        elif token.group(1) is not None:
            path = html.unescape(token.group(1))
            parent = parents[depth - 2] if depth > 1 and len(parents) >= depth - 1 else "."
            if parent != ".":
                path = f"{parent}/{path}"
            parents[depth - 1:] = [path]
            entries.append([path, None, []])
        elif token.group(2) is not None:
            entries[-1][1] = html.unescape(token.group(2)).strip()
        else:
            entries[-1][2].append(html.unescape(token.group(3)).strip())
    lines = []
    ids = set()
    for path, license, copyrights in entries:
        if path in OTHER_TARGETS:
            continue
        used = chosen(license)
        unknown = set(used) - KNOWN
        if unknown:
            sys.exit(f"в «In-tree files» у {path} — {license}: тексты {unknown} скрипт не берёт, посмотреть глазами")
        ids.update(used)
        lines.append(f"{path}: {license}" + ("" if used == [license] else f", used under {' AND '.join(used)}"))
        lines.extend(f"    Copyright: {c}" for c in copyrights)
    return lines, ids


def license_text(path: pathlib.Path) -> str:
    """Текст лицензии из `share/doc/rust/licenses/` без строки-заготовки
    авторских прав («<year> <copyright holders>»): авторы названы в списке
    выше."""
    body = path.read_text(encoding="utf-8").replace("\r\n", "\n")
    body = "\n".join(line for line in body.split("\n") if not re.search(r"<year>|\[yyyy\]", line))
    return re.sub(r"\n{3,}", "\n\n", body).strip()


def main() -> None:
    sysroot = pathlib.Path(run("rustc", "--print", "sysroot"))
    version = run("rustc", "--version")
    release = re.match(r"rustc (\d+\.\d+\.\d+)", version).group(1)
    docs = sysroot / "share" / "doc" / "rust"
    page_path = docs / "COPYRIGHT-library.html"
    if not page_path.exists():
        sys.exit(f"нет {page_path}: rustup component add rust-docs")
    page = page_path.read_text(encoding="utf-8")
    lock = (sysroot / "lib" / "rustlib" / "src" / "rust" / "library" / "Cargo.lock").read_text(encoding="utf-8")

    built = built_for_target(sysroot)
    crates = [p for p in closure(lock_packages(lock))
              if p.get("source", "").startswith("registry+") and p["name"].replace("-", "_") in built]

    short = text(page[page.index('<h2 id="short-version">'):page.index('<h2 id="longer-version">')].split("</h2>", 1)[1])
    longer = text(page[page.index('<h2 id="longer-version">'):page.index('<h2 id="in-tree-files">')].split("</h2>", 1)[1])
    entries, ids = in_tree(page)

    out = [
        f"The Rust Standard Library (Rust {release}, {TARGET})",
        "",
        "The program is compiled with the Rust standard library (std, core, alloc and",
        "compiler_builtins) and the crates it is built on for this target. The words",
        "of the Rust project below are from COPYRIGHT-library.html of this release.",
        "",
        short,
        "",
        longer,
        "",
        "In-tree files:",
        "",
        *entries,
        "",
    ]
    for license_id in sorted(ids):
        file = docs / "licenses" / f"{license_id}.txt"
        if not file.exists():
            sys.exit(f"нет текста {license_id}: {file}")
        out += [f"--- {license_id} ---", "", license_text(file), ""]
    out += ["Crates of the standard library:", ""]
    for package in crates:
        name, ver = package["name"], package["version"]
        out += [f"--- {name} {ver} ---", ""]
        for file_name, body in crate_files(name, ver, package["checksum"]):
            out += [f"{file_name}:", "", body, ""]
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_bytes(("\n".join(out).rstrip() + "\n").encode("utf-8"))
    print(f"{OUT.relative_to(ROOT)}: Rust {release}, крейтов {len(crates)}: "
          + ", ".join(f"{p['name']} {p['version']}" for p in crates))


if __name__ == "__main__":
    main()
