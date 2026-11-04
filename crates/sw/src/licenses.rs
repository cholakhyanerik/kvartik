//! Тексты лицензий чужого кода и шрифтов (задача 51): файл
//! `release::LICENSES` в папке программы.
//!
//! Сайт раздаёт каждому пользователю копию чужого кода — wasm собран из
//! крейтов и стандартной библиотеки Rust — и чужих шрифтов. Их лицензии это
//! разрешают с условием (сверено по текстам лицензий 2026-10-10): MIT —
//! «the above copyright notice and this permission notice shall be included
//! in all copies or substantial portions of the Software»; BSD-3-Clause —
//! уведомление «in the documentation and/or other materials provided with
//! the distribution»; Unicode-3.0 — «with all copies» или «in associated
//! Documentation»; Apache-2.0 — копия самой лицензии; SIL OFL 1.1 —
//! уведомление и лицензия с каждой копией шрифта. `cargo deny check`
//! проверяет только, что лицензия разрешена (`deny.toml`), а условия
//! исполняет этот файл. Открывает его экран «О программе», а лежит он в папке
//! программы: service worker кладёт его в кэш со всем остальным, и без сети
//! он открывается тоже.
//!
//! **Список крейтов — из сборки, а не руками** (ручной отстал бы от
//! `Cargo.lock` в первом же `cargo update`): все крейты, которые cargo
//! скомпилировал под wasm32 для wasm окна, — сообщения той же
//! `cargo build` (`wasm.rs`). Proc-macro и скрипты сборки исполняются только
//! на машине сборки, и их среди них нет. Сколько кода крейта пережило
//! компоновку, не важно: лишнее уведомление условий не нарушает, недостающее
//! — нарушает. Сверх них — загрузчик частей: его пишет из своего шаблона
//! `wasm_split_cli_support` ([`EXTRA`]); клей JS пишет wasm-bindgen, а он
//! среди крейтов. Свой разбор, а не `cargo about`: он брал бы и proc-macro,
//! а запустить его на машине автора не дал автоматический режим (решение
//! человека 2026-11-04).
//!
//! **Тексты — из пакета каждого крейта**, как их положили авторы. Где крейт
//! даёт выбор («MIT OR Apache-2.0»), берётся первая лицензия из
//! [`PREFERENCE`], у которой в пакете есть свой файл: MIT вдесятеро короче
//! Apache-2.0, а файл скачивает каждый пользователь (Правило 1). Одинаковые
//! тексты идут один раз, со списком крейтов. У 23 крейтов файла лицензии
//! в пакете нет, и их текст лежит в `crates/sw/licenses/` ([`NO_FILES`]);
//! новый такой крейт останавливает сборку словами, что делать.
//! Стандартная библиотека — `licenses/rust-std.txt` (пишет
//! `tools/rust_std_notices.py`; сборка сверяет его с `rust-version`), данные
//! CLDR в ядре — `licenses/cldr.txt`, шрифты — `fonts/OFL.txt` (пишет
//! `tools/fonts.py`).
//!
//! **Тот же код — те же байты** (Правило 6): порядок — по имени и версии,
//! концы строк — LF, путей машины в файле нет. Иначе пересборка без правок
//! меняла бы `sw.js`, и у всех появлялась бы «новая версия».

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use kvartik_core::i18n::{Key, Lang, t};
use kvartik_core::release::LICENSES;

/// Лицензии, из которых выбирается, в порядке предпочтения. Набор — ровно
/// `allow` в `deny.toml` (их сверяет тест): другую лицензию `cargo deny`
/// не пропустит, а здесь её текст выбирать не из чего. MIT первой: её
/// уведомление с именем автора короче всех.
const PREFERENCE: [&str; 8] = [
    "MIT",
    "BSD-3-Clause",
    "Zlib",
    "BSL-1.0",
    "Unicode-3.0",
    "Apache-2.0",
    "Unlicense",
    "CC0-1.0",
];

/// Крейты не из wasm, чей код всё же лежит в сайте: загрузчик частей —
/// шаблон `wasm_split_cli_support` (`src/snippets/split_wasm.js` в 0.2.4),
/// который программа разреза дописывает в `kvartik-split-*.js`.
const EXTRA: &[&str] = &["wasm_split_cli_support"];

/// Текст лицензии крейта, который не кладёт его в свой пакет.
struct Stored {
    /// Лицензия в `Cargo.toml` крейта на день сверки: сменилась — текст
    /// сверить заново, и сборка остановится.
    license: &'static str,
    /// Какую из лицензий выражения исполняет текст.
    used: &'static str,
    text: &'static str,
    /// Откуда текст; `None` — его нет и в репозитории, и это общий текст
    /// лицензии без строки авторских прав.
    from: Option<&'static str>,
}

/// Leptos: `LICENSE` в корне github.com/leptos-rs/leptos — один и тот же
/// на коммитах, с которых опубликованы пакеты (`.cargo_vcs_info.json`;
/// двух из них в репозитории нет, у остальных двенадцати он тот же).
const LEPTOS: Stored = Stored {
    license: "MIT",
    used: "MIT",
    text: include_str!("../licenses/leptos.txt"),
    from: Some("https://github.com/leptos-rs/leptos (LICENSE)"),
};
/// gloo: `LICENSE-MIT` в корне github.com/rustwasm/gloo на коммитах 40c6daa
/// (gloo-net) и f3bb1d1 (gloo-utils), одинаковый.
const GLOO: Stored = Stored {
    license: "MIT OR Apache-2.0",
    used: "MIT",
    text: include_str!("../licenses/gloo.txt"),
    from: Some("https://github.com/rustwasm/gloo (LICENSE-MIT)"),
};
/// rstml: `LICENSE` в корне github.com/rs-tml/rstml на коммите 9fe0f46.
const RSTML: Stored = Stored {
    license: "MIT",
    used: "MIT",
    text: include_str!("../licenses/rstml.txt"),
    from: Some("https://github.com/rs-tml/rstml (LICENSE)"),
};
/// Разрез wasm: в корне github.com/WorldSEnder/wasm-split-prototype на
/// коммитах 8b5f836 и d8263b2 — один `LICENSE`, и это Apache-2.0, а файла
/// MIT нет: исполняется она.
const WASM_SPLIT: Stored = Stored {
    license: "MIT OR Apache-2.0",
    used: "Apache-2.0",
    text: include_str!("../licenses/wasm-split.txt"),
    from: Some("https://github.com/WorldSEnder/wasm-split-prototype (LICENSE)"),
};
/// Ни в пакете, ни в репозитории файла лицензии нет: общий текст MIT
/// (SPDX license-list-data 3.29.0, `text/MIT.txt`) без строки-заготовки
/// «Copyright (c) <year> <copyright holders>», а авторы — из `Cargo.toml`.
const MIT_ONLY: Stored = Stored {
    license: "MIT",
    used: "MIT",
    text: include_str!("../licenses/mit.txt"),
    from: None,
};
const MIT_OR_APACHE: Stored = Stored {
    license: "MIT OR Apache-2.0",
    ..MIT_ONLY
};

/// Крейты без файла лицензии в пакете (сверено 2026-11-04): текст — из
/// корня их репозитория на коммите, с которого опубликован пакет.
const NO_FILES: &[(&str, &Stored)] = &[
    ("any_spawner", &LEPTOS),
    ("const_str_slice_concat", &LEPTOS),
    ("drain_filter_polyfill", &MIT_OR_APACHE),
    ("either_of", &LEPTOS),
    ("erased", &MIT_ONLY),
    ("gloo-net", &GLOO),
    ("gloo-utils", &GLOO),
    ("hydration_context", &LEPTOS),
    ("leptos", &LEPTOS),
    ("leptos_config", &LEPTOS),
    ("leptos_dom", &LEPTOS),
    ("leptos_hot_reload", &LEPTOS),
    ("leptos_server", &LEPTOS),
    ("next_tuple", &LEPTOS),
    ("oco_ref", &LEPTOS),
    ("or_poisoned", &LEPTOS),
    ("reactive_graph", &LEPTOS),
    ("reactive_stores", &LEPTOS),
    ("rstml", &RSTML),
    ("server_fn", &LEPTOS),
    ("tachys", &LEPTOS),
    ("throw_error", &LEPTOS),
    ("wasm_split_cli_support", &WASM_SPLIT),
    ("wasm_split_helpers", &WASM_SPLIT),
];

/// Уведомление стандартной библиотеки Rust (`tools/rust_std_notices.py`).
const RUST_STD: &str = include_str!("../licenses/rust-std.txt");
/// Unicode License V3 данных CLDR: `LICENSE` github.com/unicode-org/cldr-json
/// на теге 48.2.3 — той версии, из которой `tools/countries.py` пишет
/// таблицу стран.
const CLDR: &str = include_str!("../licenses/cldr.txt");
/// Лицензия шрифтов в папке программы: её пишет `tools/fonts.py`, а Trunk
/// копирует со шрифтами.
const FONTS_LICENSE: &str = "fonts/OFL.txt";

/// Пакет из `cargo metadata`: что о нём нужно файлу.
#[derive(Debug)]
struct Package {
    name: String,
    version: String,
    license: Option<String>,
    authors: Vec<String>,
    dir: PathBuf,
}

/// Крейт в файле: его строка и тексты, которые к нему относятся.
#[derive(Debug, PartialEq)]
struct Notice {
    /// «имя версия — выражение», с выбранной лицензией и авторами, если
    /// без них не понять, чьё уведомление.
    line: String,
    texts: Vec<Text>,
}

/// Текст лицензии и слова о том, откуда он, — группа в файле.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Text {
    note: Option<String>,
    body: String,
}

/// Записать файл лицензий в папку программы `app`. `compiled` — `package_id`
/// крейтов, скомпилированных в wasm. Возвращает число крейтов в файле.
pub fn write(app: &Path, compiled: &BTreeSet<String>) -> Result<usize, String> {
    let packages = metadata()?;
    let fonts_path = app.join(FONTS_LICENSE);
    let fonts = fs::read_to_string(&fonts_path).map_err(|error| format!("{}: {error}", fonts_path.display()))?;
    let mut notices = Vec::new();
    for package in crates(compiled, &packages)? {
        notices.push(notice(package, &files(&package.dir)?)?);
    }
    unused_stored(&notices)?;
    let text = render(env!("CARGO_PKG_VERSION"), &fonts, &notices)?;
    let path = app.join(LICENSES);
    fs::write(&path, text).map_err(|error| format!("{}: {error}", path.display()))?;
    Ok(notices.len())
}

/// Пакеты всего дерева по `package_id` — из `cargo metadata`: лицензию
/// в `Cargo.toml` руками не разобрать. Платформа — та, под которую собран
/// wasm: пакеты других платформ cargo не скачивает.
fn metadata() -> Result<HashMap<String, Package>, String> {
    let cargo = env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.toml");
    let output = Command::new(cargo)
        .args(["metadata", "--format-version", "1", "--filter-platform", "wasm32-unknown-unknown"])
        .arg("--manifest-path")
        .arg(&manifest)
        .stderr(Stdio::inherit())
        .output()
        .map_err(|error| format!("cargo metadata: {error}"))?;
    if !output.status.success() {
        return Err("cargo metadata завершился с ошибкой".into());
    }
    packages(&String::from_utf8_lossy(&output.stdout))
}

/// Пакеты из ответа `cargo metadata`.
fn packages(json: &str) -> Result<HashMap<String, Package>, String> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|error| format!("cargo metadata: {error}"))?;
    let mut out = HashMap::new();
    for package in value["packages"].as_array().ok_or("cargo metadata: нет packages")? {
        let field = |name: &str| package[name].as_str().map(str::to_owned);
        let id = field("id").ok_or("cargo metadata: пакет без id")?;
        let manifest = field("manifest_path").ok_or_else(|| format!("{id}: нет manifest_path"))?;
        let dir = Path::new(&manifest).parent().ok_or_else(|| format!("{id}: manifest_path без папки"))?;
        out.insert(
            id.clone(),
            Package {
                name: field("name").ok_or_else(|| format!("{id}: нет name"))?,
                version: field("version").ok_or_else(|| format!("{id}: нет version"))?,
                license: field("license"),
                authors: package["authors"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|author| author.as_str().map(str::to_owned))
                    .collect(),
                dir: dir.to_path_buf(),
            },
        );
    }
    Ok(out)
}

/// Крейты файла: скомпилированные в wasm, кроме своих (путь, а не реестр:
/// чужие источники `deny.toml` запрещает), и [`EXTRA`] — по имени и версии.
fn crates<'a>(
    compiled: &BTreeSet<String>,
    packages: &'a HashMap<String, Package>,
) -> Result<Vec<&'a Package>, String> {
    let mut chosen = BTreeMap::new();
    for id in compiled {
        if id.starts_with("path+") {
            continue;
        }
        let package = packages.get(id).ok_or_else(|| format!("{id}: нет в cargo metadata"))?;
        chosen.insert((package.name.as_str(), package.version.as_str()), package);
    }
    if chosen.is_empty() {
        return Err("cargo не назвал ни одного крейта wasm".into());
    }
    for name in EXTRA {
        let mut found = packages.values().filter(|package| package.name == *name);
        let package = found.next().ok_or_else(|| format!("{name}: нет в cargo metadata"))?;
        if found.next().is_some() {
            return Err(format!("{name}: в дереве две версии, а файл ждёт одну"));
        }
        chosen.insert((package.name.as_str(), package.version.as_str()), package);
    }
    Ok(chosen.into_values().collect())
}

/// Файл лицензии в корне пакета.
#[derive(Debug)]
struct LicenseFile {
    name: String,
    /// Чей: `None` у общего — `LICENSE`, `COPYING`, `COPYRIGHT` ([`kind`]).
    license: Option<&'static str>,
    text: String,
}

impl LicenseFile {
    /// Свой файл лицензии `id`.
    fn is_of(&self, id: &str) -> bool {
        self.license == Some(id)
    }
}

/// Файлы лицензии в корне пакета, по имени.
fn files(dir: &Path) -> Result<Vec<LicenseFile>, String> {
    let entries = fs::read_dir(dir).map_err(|error| format!("{}: {error}", dir.display()))?;
    let mut out = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| format!("{}: {error}", dir.display()))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(license) = kind(&name) else { continue };
        if !entry.file_type().map_err(|error| format!("{name}: {error}"))?.is_file() {
            continue;
        }
        let bytes = fs::read(entry.path()).map_err(|error| format!("{}: {error}", entry.path().display()))?;
        let text = String::from_utf8(bytes).map_err(|_| format!("{}: не UTF-8", entry.path().display()))?;
        out.push(LicenseFile { name, license, text });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// Чей файл лицензии, по имени: `Some(Some(лицензия))` — своей лицензии
/// («LICENSE-MIT», «LICENSE-APACHE.md», «UNLICENSE»), `Some(None)` — общий
/// («LICENSE», «COPYING», «COPYRIGHT», «NOTICE» и «LICENSE-…» с незнакомым
/// хвостом), `None` — не файл лицензии.
fn kind(name: &str) -> Option<Option<&'static str>> {
    let upper = name.to_ascii_uppercase();
    let stem = upper.split('.').next().unwrap_or(&upper);
    if stem == "UNLICENSE" {
        return Some(Some("Unlicense"));
    }
    let base = ["LICENSE", "LICENCE", "COPYING", "COPYRIGHT", "NOTICE"]
        .into_iter()
        .find(|base| stem.starts_with(base))?;
    let tail = &stem[base.len()..];
    if tail.is_empty() {
        return Some(None);
    }
    if !tail.starts_with(['-', '_']) {
        return None;
    }
    // Порядок: «0BSD» прежде «BSD», «APACHE» прежде «MIT» не нужен — их
    // вместе в одном имени не бывает.
    let id = [
        ("APACHE", "Apache-2.0"),
        ("MIT", "MIT"),
        ("ZLIB", "Zlib"),
        ("0BSD", "0BSD"),
        ("UNICODE", "Unicode-3.0"),
        ("BOOST", "BSL-1.0"),
        ("BSL", "BSL-1.0"),
        ("CC0", "CC0-1.0"),
    ]
    .into_iter()
    .find(|(mark, _)| tail.contains(mark))
    .map(|(_, id)| id);
    Some(id)
}

/// Варианты выражения SPDX: каждый — лицензии, которые надо исполнить
/// вместе. «A OR B» — два варианта, «A AND B» — один из двух лицензий;
/// AND связывает сильнее OR (SPDX 3.0, приложение D), скобки — как обычно.
/// «/» — старая запись OR, её ещё пишут в `Cargo.toml` («MIT/Apache-2.0»).
/// «A WITH B» — одна лицензия с исключением, а не две.
fn alternatives(expression: &str) -> Result<Vec<BTreeSet<String>>, String> {
    let spaced = expression.replace('/', " OR ").replace('(', " ( ").replace(')', " ) ");
    let tokens: Vec<&str> = spaced.split_whitespace().collect();
    let mut at = 0;
    let result = or_expr(&tokens, &mut at)?;
    if at != tokens.len() {
        return Err(format!("лишнее в выражении «{expression}»"));
    }
    Ok(result)
}

fn or_expr(tokens: &[&str], at: &mut usize) -> Result<Vec<BTreeSet<String>>, String> {
    let mut result = and_expr(tokens, at)?;
    while tokens.get(*at) == Some(&"OR") {
        *at += 1;
        result.extend(and_expr(tokens, at)?);
    }
    Ok(result)
}

fn and_expr(tokens: &[&str], at: &mut usize) -> Result<Vec<BTreeSet<String>>, String> {
    let mut result = atom(tokens, at)?;
    while tokens.get(*at) == Some(&"AND") {
        *at += 1;
        let right = atom(tokens, at)?;
        result = result
            .iter()
            .flat_map(|left| right.iter().map(move |right| left.union(right).cloned().collect()))
            .collect();
    }
    Ok(result)
}

fn atom(tokens: &[&str], at: &mut usize) -> Result<Vec<BTreeSet<String>>, String> {
    match tokens.get(*at) {
        Some(&"(") => {
            *at += 1;
            let inner = or_expr(tokens, at)?;
            if tokens.get(*at) != Some(&")") {
                return Err("нет закрывающей скобки".into());
            }
            *at += 1;
            Ok(inner)
        }
        Some(id) if !matches!(*id, ")" | "AND" | "OR" | "WITH") => {
            *at += 1;
            let mut license = (*id).to_owned();
            if tokens.get(*at) == Some(&"WITH") {
                let exception = tokens.get(*at + 1).ok_or("WITH без исключения")?;
                license = format!("{license} WITH {exception}");
                *at += 2;
            }
            Ok(vec![BTreeSet::from([license])])
        }
        _ => Err("ожидалась лицензия".into()),
    }
}

/// Строка крейта и его тексты.
fn notice(package: &Package, files: &[LicenseFile]) -> Result<Notice, String> {
    let name = &package.name;
    let declared = package
        .license
        .as_deref()
        .ok_or_else(|| format!("{name}: в Cargo.toml нет выражения license — посмотреть его лицензию глазами"))?;
    let stored = NO_FILES.iter().find(|(crate_name, _)| crate_name == name).map(|(_, stored)| *stored);
    if files.is_empty() {
        let stored = stored.ok_or_else(|| {
            format!(
                "{name} {}: в пакете нет файла лицензии. Положите текст из корня его репозитория \
                 на коммите из .cargo_vcs_info.json в crates/sw/licenses/ и строку в NO_FILES \
                 (crates/sw/src/licenses.rs)",
                package.version
            )
        })?;
        if stored.license != declared {
            return Err(format!(
                "{name}: лицензия в Cargo.toml — «{declared}», а текст в crates/sw/licenses/ сверен \
                 для «{}»: сверить заново",
                stored.license
            ));
        }
        let note = match stored.from {
            Some(from) => format!("Text from the repository {from}: the package has no license file."),
            None => "Standard text: neither the package nor its repository has a license file.".to_owned(),
        };
        let authors = (stored.from.is_none() && !package.authors.is_empty())
            .then(|| format!(" (authors in Cargo.toml: {})", package.authors.join(", ")));
        let used = BTreeSet::from([stored.used.to_owned()]);
        return Ok(Notice {
            line: format!("{}{}", line(package, declared, &used), authors.unwrap_or_default()),
            texts: vec![Text {
                note: Some(note),
                body: normalized(stored.text),
            }],
        });
    }
    if stored.is_some() {
        return Err(format!(
            "{name}: в пакете появился файл лицензии — убрать его из NO_FILES (crates/sw/src/licenses.rs)"
        ));
    }
    let used = choose(&alternatives(declared).map_err(|error| format!("{name}: {error}"))?, files)
        .ok_or_else(|| format!("{name}: «{declared}» — ни одной лицензии из allow в deny.toml"))?;
    let general = files.iter().any(|file| file.license.is_none());
    if let Some(missing) = used.iter().find(|id| !general && !files.iter().any(|file| file.is_of(id))) {
        return Err(format!("{name}: в пакете нет текста {missing}"));
    }
    let texts = files
        .iter()
        .filter(|file| file.license.is_none_or(|id| used.contains(id)))
        .map(|file| Text {
            note: None,
            body: normalized(&file.text),
        })
        .collect();
    Ok(Notice {
        line: line(package, declared, &used),
        texts,
    })
}

/// Вариант, который исполняется: все его лицензии разрешены, своих файлов
/// у него больше всех, а среди равных — раньше по [`PREFERENCE`].
fn choose(alternatives: &[BTreeSet<String>], files: &[LicenseFile]) -> Option<BTreeSet<String>> {
    let rank = |id: &str| PREFERENCE.iter().position(|known| *known == id);
    alternatives
        .iter()
        .filter(|alternative| alternative.iter().all(|id| rank(id).is_some()))
        .min_by_key(|alternative| {
            let without_file = alternative.iter().filter(|id| !files.iter().any(|file| file.is_of(id))).count();
            let ranks: Vec<usize> = alternative.iter().filter_map(|id| rank(id)).collect();
            (without_file, ranks.iter().sum::<usize>(), ranks)
        })
        .cloned()
}

/// «имя версия — выражение», и какую лицензию исполняет файл, если выбор был.
fn line(package: &Package, declared: &str, used: &BTreeSet<String>) -> String {
    let used: Vec<&str> = used.iter().map(String::as_str).collect();
    let used = used.join(" AND ");
    if used == declared {
        format!("{} {} — {declared}", package.name, package.version)
    } else {
        format!("{} {} — {declared}, used under {used}", package.name, package.version)
    }
}

/// Текст с концами строк LF и без пустых строк по краям: у одного и того же
/// текста в разных пакетах они бывают разные, а повторяться ему незачем.
fn normalized(text: &str) -> String {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text).replace("\r\n", "\n");
    let lines: Vec<&str> = text.lines().map(str::trim_end).collect();
    lines.join("\n").trim_matches('\n').to_owned()
}

/// Запись в [`NO_FILES`] о крейте, которого в программе нет, — устаревшая:
/// её убирают, а не копят.
fn unused_stored(notices: &[Notice]) -> Result<(), String> {
    for (name, _) in NO_FILES {
        if !notices.iter().any(|notice| notice.line.split(' ').next() == Some(*name)) {
            return Err(format!(
                "{name}: в NO_FILES (crates/sw/src/licenses.rs), а в программе его нет — убрать запись"
            ));
        }
    }
    Ok(())
}

/// Версия Rust в первой строке уведомления стандартной библиотеки — та же,
/// что `rust-version` проекта: проект собирается тем Rust, что стоит
/// в системе (Правило 3), и с ним меняется и состав библиотеки.
fn rust_std_matches(rust_version: &str) -> Result<(), String> {
    let first = RUST_STD.lines().next().unwrap_or_default();
    if first.contains(&format!("(Rust {rust_version}.")) {
        Ok(())
    } else {
        Err(format!(
            "crates/sw/licenses/rust-std.txt — «{first}», а rust-version — {rust_version}: \
             py -3 -I -X utf8 tools/rust_std_notices.py"
        ))
    }
}

/// Раздел файла с заголовком.
fn section(out: &mut String, title: &str) {
    let rule = "=".repeat(80);
    out.push_str(&format!("\n{rule}\n{title}\n{rule}\n\n"));
}

/// Весь файл: шапка на трёх языках, шрифты, стандартная библиотека, CLDR
/// и крейты — по тексту на группу крейтов с одинаковым текстом.
fn render(version: &str, fonts: &str, notices: &[Notice]) -> Result<String, String> {
    rust_std_matches(env!("CARGO_PKG_RUST_VERSION"))?;
    let mut out = format!("Kvartik {version}\n\n");
    for lang in Lang::ALL {
        out.push_str(t(lang, Key::AboutLicensesText));
        out.push_str("\n\n");
    }
    out.push_str("1. Fonts\n2. The Rust standard library\n3. Unicode CLDR data\n4. Rust crates\n");

    section(&mut out, "1. Fonts");
    out.push_str("The fonts in fonts/ are subsets of the original files.\n\n");
    out.push_str(&normalized(fonts));
    out.push('\n');

    section(&mut out, "2. The Rust standard library");
    out.push_str(&normalized(RUST_STD));
    out.push('\n');

    section(&mut out, "3. Unicode CLDR data");
    out.push_str(
        "The names of countries and the rules for writing numbers, dates and amounts come from\n\
         the Unicode CLDR data (https://github.com/unicode-org/cldr-json).\n\n",
    );
    out.push_str(&normalized(CLDR));
    out.push('\n');

    section(&mut out, "4. Rust crates");
    out.push_str(&format!(
        "The program is compiled from these {} crates from crates.io. Its JavaScript glue is\n\
         written by wasm-bindgen, and the loader of its parts by wasm_split_cli_support. Where\n\
         a crate offers a choice of licenses, Kvartik uses the one after \"used under\".\n",
        notices.len()
    ));
    let mut groups: BTreeMap<&Text, Vec<&str>> = BTreeMap::new();
    for notice in notices {
        for text in &notice.texts {
            groups.entry(text).or_default().push(&notice.line);
        }
    }
    // Порядок групп — по первому крейту, а не по тексту: так файл читается
    // по алфавиту.
    let mut groups: Vec<(&Text, Vec<&str>)> = groups.into_iter().collect();
    groups.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(b.0)));
    let rule = "-".repeat(80);
    for (text, lines) in groups {
        out.push_str(&format!("\n{rule}\n"));
        for line in lines {
            out.push_str(line);
            out.push('\n');
        }
        if let Some(note) = &text.note {
            out.push_str(note);
            out.push('\n');
        }
        out.push_str(&format!("{rule}\n\n{}\n", text.body));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package(name: &str, license: &str) -> Package {
        Package {
            name: name.to_owned(),
            version: "1.0.0".to_owned(),
            license: Some(license.to_owned()),
            authors: vec!["A. Author".to_owned()],
            dir: PathBuf::new(),
        }
    }

    fn file(name: &str, text: &str) -> LicenseFile {
        LicenseFile {
            name: name.to_owned(),
            license: kind(name).expect("файл лицензии"),
            text: text.to_owned(),
        }
    }

    fn set(ids: &[&str]) -> BTreeSet<String> {
        ids.iter().map(|id| (*id).to_owned()).collect()
    }

    /// Имена файлов лицензии из пакетов дерева (разведка задачи 51,
    /// 2026-11-04): свои у лицензии, общие и чужие.
    #[test]
    fn kind_reads_license_file_names() {
        let mut wrong = Vec::new();
        for (name, expected) in [
            ("LICENSE-MIT", Some(Some("MIT"))),
            ("LICENSE-MIT.md", Some(Some("MIT"))),
            ("LICENSE-MIT.txt", Some(Some("MIT"))),
            ("LICENSE-APACHE", Some(Some("Apache-2.0"))),
            ("LICENSE-APACHE.md", Some(Some("Apache-2.0"))),
            ("LICENSE-ZLIB.md", Some(Some("Zlib"))),
            ("LICENSE-0BSD", Some(Some("0BSD"))),
            ("LICENSE-UNICODE", Some(Some("Unicode-3.0"))),
            ("UNLICENSE", Some(Some("Unlicense"))),
            ("LICENSE", Some(None)),
            ("LICENSE.md", Some(None)),
            ("COPYING", Some(None)),
            ("COPYRIGHT", Some(None)),
            ("NOTICE", Some(None)),
            ("LICENSE-THIRD-PARTY", Some(None)),
            ("license-mit", Some(Some("MIT"))),
            ("LICENSES", None),
            ("README.md", None),
            ("Cargo.toml", None),
        ] {
            if kind(name) != expected {
                wrong.push((name, kind(name)));
            }
        }
        assert!(wrong.is_empty(), "не так: {wrong:?}");
    }

    /// Выражения из пакетов дерева: «/» — тоже OR, AND сильнее OR, скобки
    /// и WITH.
    #[test]
    fn alternatives_follow_spdx() {
        let mut wrong = Vec::new();
        for (expression, expected) in [
            ("MIT", vec![set(&["MIT"])]),
            ("MIT OR Apache-2.0", vec![set(&["MIT"]), set(&["Apache-2.0"])]),
            ("Apache-2.0/MIT", vec![set(&["Apache-2.0"]), set(&["MIT"])]),
            ("(MIT OR Apache-2.0) AND Unicode-3.0", vec![set(&["MIT", "Unicode-3.0"]), set(&["Apache-2.0", "Unicode-3.0"])]),
            ("MIT OR Apache-2.0 AND Unicode-3.0", vec![set(&["MIT"]), set(&["Apache-2.0", "Unicode-3.0"])]),
            ("Apache-2.0 WITH LLVM-exception OR MIT", vec![set(&["Apache-2.0 WITH LLVM-exception"]), set(&["MIT"])]),
        ] {
            match alternatives(expression) {
                Ok(found) if found == expected => {}
                other => wrong.push((expression, other)),
            }
        }
        for broken in ["", "MIT OR", "(MIT", "MIT)", "AND MIT", "MIT WITH"] {
            if alternatives(broken).is_ok() {
                wrong.push((broken, Ok(Vec::new())));
            }
        }
        assert!(wrong.is_empty(), "не так: {wrong:?}");
    }

    /// Из выбора берётся лицензия со своим файлом, а из равных — раньше по
    /// списку; неразрешённая не берётся никогда.
    #[test]
    fn choose_prefers_a_license_with_its_own_file() {
        let dual = [file("LICENSE-APACHE", "a"), file("LICENSE-MIT", "m")];
        let both = alternatives("MIT OR Apache-2.0").expect("выражение");
        assert_eq!(choose(&both, &dual), Some(set(&["MIT"])));
        let only_apache = [file("LICENSE-APACHE", "a")];
        assert_eq!(choose(&both, &only_apache), Some(set(&["Apache-2.0"])));
        let unicode = alternatives("(MIT OR Apache-2.0) AND Unicode-3.0").expect("выражение");
        let files = [file("LICENSE-APACHE", "a"), file("LICENSE-MIT", "m"), file("LICENSE-UNICODE", "u")];
        assert_eq!(choose(&unicode, &files), Some(set(&["MIT", "Unicode-3.0"])));
        let zero_bsd = alternatives("0BSD OR MIT OR Apache-2.0").expect("выражение");
        let files = [file("LICENSE-0BSD", "0"), file("LICENSE-APACHE", "a"), file("LICENSE-MIT", "m")];
        assert_eq!(choose(&zero_bsd, &files), Some(set(&["MIT"])));
        assert_eq!(choose(&alternatives("GPL-3.0").expect("выражение"), &[file("LICENSE", "g")]), None);
    }

    /// В файл идут текст выбранной лицензии и общие файлы, а текст
    /// невыбранной — нет; концы строк — LF.
    #[test]
    fn notice_takes_the_chosen_text_and_general_files() {
        let files = [
            file("COPYRIGHT", "Copyright 2020 A.\r\n"),
            file("LICENSE-APACHE", "Apache text"),
            file("LICENSE-MIT", "\r\nMIT text\r\n\r\n"),
        ];
        let found = notice(&package("dual", "MIT OR Apache-2.0"), &files).expect("уведомление");
        assert_eq!(found.line, "dual 1.0.0 — MIT OR Apache-2.0, used under MIT");
        let bodies: Vec<&str> = found.texts.iter().map(|text| text.body.as_str()).collect();
        assert_eq!(bodies, ["Copyright 2020 A.", "MIT text"]);

        let single = notice(&package("single", "MIT"), &[file("LICENSE", "MIT text")]).expect("уведомление");
        assert_eq!(single.line, "single 1.0.0 — MIT");

        let error = notice(&package("lost", "MIT OR Apache-2.0"), &[file("LICENSE-ZLIB", "z")]).expect_err("нет текста");
        assert!(error.contains("нет текста MIT"), "{error}");
    }

    /// Крейт без файла лицензии: текст из [`NO_FILES`], а нет записи или
    /// сменилась лицензия — остановка словами. У общего текста — авторы.
    #[test]
    fn crates_without_license_files_need_a_stored_text() {
        let leptos = notice(&package("leptos", "MIT"), &[]).expect("текст Leptos");
        assert_eq!(leptos.texts.len(), 1);
        assert!(leptos.texts[0].body.contains("Copyright (c) 2022 Greg Johnston"));
        assert!(leptos.texts[0].note.as_deref().is_some_and(|note| note.contains("leptos-rs/leptos")));

        let erased = notice(&package("erased", "MIT"), &[]).expect("общий текст");
        assert!(erased.line.ends_with("(authors in Cargo.toml: A. Author)"), "{}", erased.line);
        assert!(!erased.texts[0].body.contains("<year>"));

        let split = notice(&package("wasm_split_helpers", "MIT OR Apache-2.0"), &[]).expect("Apache");
        assert_eq!(split.line, "wasm_split_helpers 1.0.0 — MIT OR Apache-2.0, used under Apache-2.0");

        let mut wrong = Vec::new();
        for (name, license, files, words) in [
            ("unknown", "MIT", vec![], "нет файла лицензии"),
            ("leptos", "Apache-2.0", vec![], "сверить заново"),
            ("leptos", "MIT", vec![file("LICENSE", "x")], "убрать его из NO_FILES"),
        ] {
            match notice(&package(name, license), &files) {
                Err(error) if error.contains(words) => {}
                other => wrong.push((name, license, format!("{other:?}"))),
            }
        }
        assert!(wrong.is_empty(), "не так: {wrong:?}");
    }

    /// Набор [`PREFERENCE`] — ровно `allow` в `deny.toml`: разрешённая там
    /// лицензия, которой здесь нет, остановила бы сборку на первом же
    /// крейте с ней, а лишняя здесь выбиралась бы мимо решения человека.
    #[test]
    fn preference_is_the_deny_allow_list() {
        let deny = include_str!("../../../deny.toml");
        let allow = deny.split("allow = [").nth(1).and_then(|rest| rest.split(']').next()).expect("allow в deny.toml");
        let mut listed: Vec<&str> = allow
            .lines()
            .map(str::trim)
            .filter(|line| line.starts_with('"'))
            .map(|line| line.trim_end_matches(',').trim_matches('"'))
            .collect();
        listed.sort_unstable();
        let mut known = PREFERENCE.to_vec();
        known.sort_unstable();
        assert_eq!(listed, known);
    }

    /// Уведомление стандартной библиотеки — для `rust-version` проекта; не
    /// для неё — остановка со словами, что запустить.
    #[test]
    fn rust_std_notice_follows_the_rust_version() {
        assert_eq!(rust_std_matches(env!("CARGO_PKG_RUST_VERSION")), Ok(()));
        let error = rust_std_matches("1.0").expect_err("другая версия");
        assert!(error.contains("tools/rust_std_notices.py"), "{error}");
    }

    /// Устаревшая запись в [`NO_FILES`] — ошибка, как неиспользованное
    /// исключение в `deny.toml`.
    #[test]
    fn stored_texts_of_crates_not_in_the_program_are_an_error() {
        let all: Vec<Notice> = NO_FILES
            .iter()
            .map(|(name, _)| Notice {
                line: format!("{name} 1.0.0 — MIT"),
                texts: Vec::new(),
            })
            .collect();
        assert_eq!(unused_stored(&all), Ok(()));
        let error = unused_stored(&all[1..]).expect_err("одной нет");
        assert!(error.contains(NO_FILES[0].0), "{error}");
    }

    /// Файл: шапка на трёх языках, четыре раздела, одинаковый текст — один
    /// раз со всеми своими крейтами; тот же вход — те же байты.
    #[test]
    fn render_groups_equal_texts() {
        let mit = Text {
            note: None,
            body: "MIT text".to_owned(),
        };
        let notices = [
            Notice {
                line: "a 1.0.0 — MIT".to_owned(),
                texts: vec![mit.clone()],
            },
            Notice {
                line: "b 1.0.0 — MIT OR Apache-2.0, used under MIT".to_owned(),
                texts: vec![mit.clone()],
            },
            Notice {
                line: "c 1.0.0 — Zlib".to_owned(),
                texts: vec![Text {
                    note: None,
                    body: "Zlib text".to_owned(),
                }],
            },
        ];
        let text = render("9.9.9", "OFL text\r\n", &notices).expect("файл");
        assert!(text.starts_with("Kvartik 9.9.9\n\n"));
        for lang in Lang::ALL {
            assert!(text.contains(t(lang, Key::AboutLicensesText)), "{lang:?}");
        }
        for part in ["1. Fonts", "2. The Rust standard library", "3. Unicode CLDR data", "4. Rust crates"] {
            assert_eq!(text.matches(&format!("\n{part}\n")).count(), 2, "{part}");
        }
        assert!(text.contains("compiled from these 3 crates"));
        assert_eq!(text.matches("MIT text").count(), 1);
        assert!(text.contains("a 1.0.0 — MIT\nb 1.0.0 — MIT OR Apache-2.0, used under MIT\n"));
        assert!(text.contains("OFL text\n") && !text.contains('\r'));
        assert!(text.contains("UNICODE LICENSE V3") && text.contains("The Rust Standard Library (Rust "));
        assert_eq!(text, render("9.9.9", "OFL text\r\n", &notices).expect("файл"));
    }
}
