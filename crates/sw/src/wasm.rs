//! Wasm окна с частями (задача 58): сборка, разрез, wasm-bindgen, wasm-opt,
//! имена с хэшем и запуск в `index.html`.
//!
//! Почему не Trunk: он wasm не режет (trunk-rs/trunk#1022, открыт),
//! а неразрезанный wasm с точками разреза не запускается вовсе — страница
//! пустая, без плашки сбоя (CLAUDE.md, Правило 6). Резать надо до
//! wasm-bindgen: программе разреза нужны связи компоновщика
//! (`--emit-relocs`, `.cargo/config.toml`), а wasm-bindgen и wasm-opt их
//! выбрасывают. Поэтому весь путь wasm — здесь, а Trunk собирает остальное:
//! стили, шрифты, значки и саму страницу.
//!
//! Порядок — как у cargo-leptos 0.3.11 (`compile/front.rs`):
//!
//! 1. `cargo build` окна под wasm32;
//! 2. разрез (`wasm_split_cli_support`): главный модуль, части `split_<имя>`
//!    (точки разреза — `crates/app/src/ui/split.rs`), общие куски
//!    `chunk_<n>` и загрузчик частей на JS;
//! 3. wasm-bindgen главного модуля — с `--keep-lld-exports`: без него он
//!    выбросил бы `__indirect_function_table` и другие экспорты, которые
//!    части берут у главного;
//! 4. wasm-opt каждого файла — только в выпускной сборке, как было у Trunk;
//! 5. проверка: заглушек разреза не осталось, каждой части хватает
//!    экспортов главного модуля, а главному — загрузчиков. Иначе сборка
//!    прошла бы, а программа не запустилась бы или упала бы на первом
//!    нажатии (Правило 6);
//! 6. имена с хэшем — одним на все файлы: главный JS ссылается на
//!    загрузчик, загрузчик — на главный JS и на части, и хэш каждого
//!    по отдельности ходил бы по кругу (у cargo-leptos на этом ошибки #690
//!    и #658). Хэш в имени — то, по чему `sw.js` узнаёт файлы своей сборки;
//! 7. запуск, предзагрузки и `integrity` в `index.html`. Запуск — в конце
//!    клея wasm-bindgen, а клей — модуль страницы, а не встроенный скрипт,
//!    как у Trunk: встроенный скрипт CSP без `'unsafe-inline'` не пустит
//!    (задача 15, Правило 13). И не отдельным файлом: седьмой запрос
//!    первого захода ждал бы свободного соединения — HTTP/1.1 даёт
//!    браузеру шесть на сайт, — и wasm приходил бы на круг сети позже
//!    (+0,55 с на телефонном профиле, замер 2026-10-13).
//!
//! wasm-bindgen и wasm-opt — программы из PATH той версии, что ждёт сборка:
//! wasm-bindgen — та же, что крейт в `Cargo.lock` (Правило 3), wasm-opt —
//! [`WASM_OPT_VERSION`]. Где их взять — CLAUDE.md, Правило 3; в CI их
//! ставит `ci.yml`, сверяя сумму.

use std::collections::BTreeSet;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use sha2::{Digest, Sha384};
use wasmparser::{Parser, Payload};

use super::digest;

/// Цель сборки окна.
const TARGET: &str = "wasm32-unknown-unknown";
/// Имя клея wasm-bindgen и главного модуля без хэша: `kvartik-app.js`
/// и `kvartik-app_bg.wasm`, как называл их Trunk.
const MAIN: &str = "kvartik-app";
/// Загрузчик частей без хэша. Это имя зашито в главный модуль: из модуля
/// с таким именем он берёт функции загрузки частей, и клей wasm-bindgen
/// под этим ключом их и передаёт. Файл получает имя с хэшем, а ключ
/// остаётся.
const LOADER: &str = "kvartik-split";
/// Последняя строка клея wasm-bindgen 0.2.129: после неё встаёт запуск.
/// Нет её — сменился вывод wasm-bindgen, и запуск надо писать заново.
const GLUE_END: &str = "export { initSync, __wbg_init as default };";
/// Запуск окна, который встаёт после [`GLUE_END`]; `{wasm}` — имя главного
/// модуля. Входит в хэш имён: другой запуск — другой клей, и под прежним
/// именем его не должно быть ни в одном кэше.
const START: &str = "\n\n// Запуск окна (crates/sw/src/wasm.rs): этот модуль подключает страница.\n\
                     await __wbg_init({ module_or_path: new URL(\"{wasm}\", import.meta.url) });";
/// Модуль, из которого точка разреза импортирует свою часть, пока её не
/// заменила программа разреза (`wasm_split_macros` 0.2.2).
const PLACEHOLDER: &str = "__wasm_split_placeholder__";
/// Модуль, из которого части берут память, таблицу функций и прочее
/// у главного (загрузчик собирает его из экспортов главного).
const SHARED: &str = "__wasm_split";
/// Версия binaryen, которой wasm-opt жмёт wasm: та, что брал Trunk
/// 0.22.0-rc.2 по умолчанию (`version_131`). Меняется — меняется и в
/// `ci.yml`, и замер размера (Правило 1).
const WASM_OPT_VERSION: &str = "131";
/// Уровень wasm-opt: `s`, а не `z` — после brotli `z` не меньше
/// (замер 2026-10-13, задача 57; CLAUDE.md, Правило 1).
const WASM_OPT_LEVEL: &str = "-Os";

/// Что сборка wasm кладёт в папку сайта.
pub struct Built {
    /// Хэш в именах файлов.
    pub hash: String,
    /// Сколько частей.
    pub parts: usize,
}

/// Собрать wasm пакета `package` в папку сборки `staging` и вписать запуск
/// в её `index.html`.
pub fn build(package: &str, staging: &Path) -> Result<Built, String> {
    let release = match env::var("TRUNK_PROFILE").as_deref() {
        Ok("release") => true,
        Ok("debug") => false,
        _ => {
            return Err(
                "нет TRUNK_PROFILE: программу зовёт хук Trunk (crates/app/Trunk.toml)".into(),
            );
        }
    };
    let public_url = env::var("TRUNK_PUBLIC_URL").unwrap_or_else(|_| "/".to_owned());
    let bindgen = Tool::wasm_bindgen()?;
    let opt = if release {
        Some(Tool::wasm_opt()?)
    } else {
        None
    };

    let raw = cargo_build(package, release)?;
    let work = raw
        .parent()
        .ok_or("у wasm нет папки")?
        .join("kvartik-split");
    let _ = fs::remove_dir_all(&work);
    fs::create_dir_all(&work).map_err(|error| format!("{}: {error}", work.display()))?;

    // Разрез.
    let input = read(&raw)?;
    one_point_per_part(&Module::parse(&input)?)?;
    let main_path = work.join(format!("{MAIN}.wasm"));
    let link_name = format!("./{LOADER}.js");
    let main_module = format!("./{MAIN}.js");
    let split = wasm_split_cli_support::transform({
        let mut options = wasm_split_cli_support::Options::new(&input);
        options.output_dir = &work;
        options.main_out_path = &main_path;
        options.link_name = &link_name;
        options.main_module = &main_module;
        options.target.web();
        options
    })
    .map_err(|error| {
        format!(
            "разрез wasm: {error:#}. «No linking section found» — в wasm нет связей \
             компоновщика: `-Clink-args=--emit-relocs` в .cargo/config.toml, а переменная \
             RUSTFLAGS его перебивает"
        )
    })?;

    // wasm-bindgen главного модуля.
    let glue_dir = work.join("bindgen");
    bindgen.run(
        Command::new(&bindgen.path)
            .args([
                "--target",
                "web",
                "--keep-lld-exports",
                "--no-typescript",
                "--out-name",
                MAIN,
            ])
            .arg("--out-dir")
            .arg(&glue_dir)
            .arg(&main_path),
    )?;
    let mut main_wasm = glue_dir.join(format!("{MAIN}_bg.wasm"));
    let glue = text(&glue_dir.join(format!("{MAIN}.js")))?;
    let loader = sorted_shares(&text(&work.join(format!("{LOADER}.js")))?);

    // wasm-opt — главный модуль и каждая часть, на которую ссылается
    // загрузчик. Пустую часть загрузчик не грузит, и в сайт она не идёт.
    let mut parts = Vec::new();
    for path in &split.split_modules {
        let name = file_name(path)?;
        if loader.contains(&format!("\"./{name}\"")) {
            parts.push((name, path.clone()));
        }
    }
    if let Some(opt) = &opt {
        let optimized = work.join("opt");
        fs::create_dir_all(&optimized)
            .map_err(|error| format!("{}: {error}", optimized.display()))?;
        let out = optimized.join(format!("{MAIN}_bg.wasm"));
        opt.optimize(&main_wasm, &out)?;
        main_wasm = out;
        for (name, path) in &mut parts {
            let out = optimized.join(&*name);
            opt.optimize(path, &out)?;
            *path = out;
        }
    }

    // Проверка.
    let main_bytes = read(&main_wasm)?;
    let mut part_bytes = Vec::with_capacity(parts.len());
    for (name, path) in &parts {
        part_bytes.push((name.clone(), read(path)?));
    }
    let mut part_info = Vec::with_capacity(parts.len());
    for (name, bytes) in &part_bytes {
        part_info.push((
            name.clone(),
            Module::parse(bytes).map_err(|error| format!("{name}: {error}"))?,
        ));
    }
    check(&Module::parse(&main_bytes)?, &part_info, &loader)?;

    // Имена с хэшем: один хэш всех файлов, ещё без хэшей внутри, и запуска,
    // который встанет в клей.
    let (glue_plain, wasm_plain, loader_plain) = (
        format!("{MAIN}.js"),
        format!("{MAIN}_bg.wasm"),
        format!("{LOADER}.js"),
    );
    let mut entries: Vec<(&str, &[u8])> = vec![
        (&glue_plain, glue.as_bytes()),
        (&wasm_plain, &main_bytes),
        (&loader_plain, loader.as_bytes()),
        ("start", START.as_bytes()),
    ];
    entries.extend(
        part_bytes
            .iter()
            .map(|(name, bytes)| (name.as_str(), bytes.as_slice())),
    );
    let hash = digest(entries);
    let named = |stem: &str, tail: &str| format!("{stem}-{hash}{tail}");
    let glue_name = named(MAIN, ".js");
    let wasm_name = named(MAIN, "_bg.wasm");
    let loader_name = named(LOADER, ".js");

    let glue = replace(
        &glue,
        &format!("from \"./{LOADER}.js\""),
        &format!("from \"./{loader_name}\""),
        !parts.is_empty(),
    )?;
    let glue = replace(
        &glue,
        &format!("'{MAIN}_bg.wasm'"),
        &format!("'{wasm_name}'"),
        false,
    )?;
    let start = format!("{GLUE_END}{}", START.replace("{wasm}", &wasm_name));
    let glue = replace(&glue, GLUE_END, &start, true)?;
    let mut loader = replace(
        &loader,
        &format!("from \"./{MAIN}.js\""),
        &format!("from \"./{glue_name}\""),
        true,
    )?;
    let mut outputs = vec![
        (glue_name.clone(), glue.into_bytes()),
        (wasm_name.clone(), main_bytes),
    ];
    for (name, bytes) in part_bytes {
        let stem = name.strip_suffix(".wasm").ok_or("часть не .wasm")?;
        let hashed = named(stem, ".wasm");
        loader = replace(
            &loader,
            &format!("\"./{name}\""),
            &format!("\"./{hashed}\""),
            true,
        )?;
        outputs.push((hashed, bytes));
    }
    outputs.push((loader_name.clone(), loader.into_bytes()));

    let page_path = staging.join("index.html");
    let page = text(&page_path)?;
    let integrity = |name: &str| {
        let bytes = &outputs
            .iter()
            .find(|(file, _)| file == name)
            .expect("файл среди своих")
            .1;
        sri(bytes)
    };
    let page = with_start(
        &page,
        &Start {
            public_url: &public_url,
            glue: (&glue_name, &integrity(&glue_name)),
            loader: (!parts.is_empty()).then(|| (loader_name.as_str(), integrity(&loader_name))),
            wasm: (&wasm_name, &integrity(&wasm_name)),
        },
    )?;
    for (name, bytes) in &outputs {
        let path = staging.join(name);
        fs::write(&path, bytes).map_err(|error| format!("{}: {error}", path.display()))?;
    }
    fs::write(&page_path, page).map_err(|error| format!("{}: {error}", page_path.display()))?;
    Ok(Built {
        hash,
        parts: parts.len(),
    })
}

/// `cargo build` окна под wasm32; путь к wasm — из сообщений cargo.
fn cargo_build(package: &str, release: bool) -> Result<PathBuf, String> {
    let cargo = env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    let mut command = Command::new(cargo);
    command.args([
        "build",
        "--package",
        package,
        "--target",
        TARGET,
        "--message-format=json-render-diagnostics",
    ]);
    if release {
        command.arg("--release");
    }
    let output = command
        .stderr(Stdio::inherit())
        .output()
        .map_err(|error| format!("cargo: {error}"))?;
    if !output.status.success() {
        return Err(format!("cargo build {package} не собрался"));
    }
    artifact(&String::from_utf8_lossy(&output.stdout), package)
        .ok_or_else(|| format!("cargo не назвал wasm пакета {package}"))
}

/// Wasm пакета из сообщений `cargo build --message-format=json`.
fn artifact(messages: &str, package: &str) -> Option<PathBuf> {
    messages
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|message| {
            message["reason"] == "compiler-artifact" && message["target"]["name"] == package
        })
        .flat_map(|message| {
            message["filenames"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|name| name.as_str().map(PathBuf::from))
                .collect::<Vec<_>>()
        })
        .find(|path| path.extension().is_some_and(|ext| ext == "wasm"))
}

/// Программа из PATH, сверенная по версии.
struct Tool {
    path: PathBuf,
    name: &'static str,
}

impl Tool {
    /// wasm-bindgen той же версии, что крейт `wasm-bindgen` в `Cargo.lock`:
    /// соседняя версия ломает клей молча или громко (Правило 3).
    fn wasm_bindgen() -> Result<Self, String> {
        let lock = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.lock");
        let lock = text(&lock)?;
        let wanted =
            locked_version(&lock, "wasm-bindgen").ok_or("в Cargo.lock нет wasm-bindgen")?;
        Self::find("wasm-bindgen", &format!("wasm-bindgen {wanted}"))
    }

    /// wasm-opt из binaryen [`WASM_OPT_VERSION`].
    fn wasm_opt() -> Result<Self, String> {
        Self::find("wasm-opt", &format!("version {WASM_OPT_VERSION} "))
    }

    /// `name` из PATH, если `--version` содержит `wanted`.
    fn find(name: &'static str, wanted: &str) -> Result<Self, String> {
        let missing = || {
            format!(
                "нужна программа {name} («{}») в PATH — где её взять, сказано в CLAUDE.md, Правило 3",
                wanted.trim()
            )
        };
        let output = Command::new(name)
            .arg("--version")
            .output()
            .map_err(|_| missing())?;
        let version = String::from_utf8_lossy(&output.stdout);
        if !output.status.success() || !version.contains(wanted) {
            return Err(format!("{}; в PATH — «{}»", missing(), version.trim()));
        }
        Ok(Self {
            path: PathBuf::from(name),
            name,
        })
    }

    fn run(&self, command: &mut Command) -> Result<(), String> {
        let status = command
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .status()
            .map_err(|error| format!("{}: {error}", self.name))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("{} завершился с ошибкой ({status})", self.name))
        }
    }

    fn optimize(&self, input: &Path, output: &Path) -> Result<(), String> {
        let mut out = OsString::from("--output=");
        out.push(output);
        self.run(
            Command::new(&self.path)
                .arg(WASM_OPT_LEVEL)
                .arg(out)
                .arg(input),
        )
    }
}

/// Версия пакета `name` в `Cargo.lock`.
fn locked_version<'a>(lock: &'a str, name: &str) -> Option<&'a str> {
    let mut lines = lock.lines().map(str::trim);
    while let Some(line) = lines.next() {
        if line == format!("name = \"{name}\"") {
            return lines
                .next()?
                .strip_prefix("version = \"")?
                .strip_suffix('"');
        }
    }
    None
}

/// Импорты и экспорты модуля wasm — то, чем части и главный модуль
/// связаны друг с другом.
#[derive(Debug, Default)]
struct Module {
    /// Модуль и имя каждого импорта.
    imports: Vec<(String, String)>,
    exports: BTreeSet<String>,
}

impl Module {
    fn parse(bytes: &[u8]) -> Result<Self, String> {
        let mut module = Self::default();
        for payload in Parser::new(0).parse_all(bytes) {
            match payload.map_err(|error| format!("wasm не читается: {error}"))? {
                Payload::ImportSection(section) => {
                    for import in section.into_imports() {
                        let import =
                            import.map_err(|error| format!("импорт не читается: {error}"))?;
                        module
                            .imports
                            .push((import.module.to_owned(), import.name.to_owned()));
                    }
                }
                Payload::ExportSection(section) => {
                    for export in section {
                        let export =
                            export.map_err(|error| format!("экспорт не читается: {error}"))?;
                        module.exports.insert(export.name.to_owned());
                    }
                }
                _ => {}
            }
        }
        Ok(module)
    }
}

/// Части и главный модуль сходятся: заглушек разреза нет нигде, каждая
/// часть берёт у главного только то, что он экспортирует, и ничего
/// больше, а каждую функцию загрузки, которую ждёт главный, загрузчик
/// экспортирует. Кроме экспортов главного части берёт «канарейку»
/// отладочной сборки: её загрузчик кладёт в общие импорты сам
/// (`__canary_…: ~0xdead`, `js.rs` в `wasm_split_cli_support` 0.2.3).
fn check(main: &Module, parts: &[(String, Module)], loader: &str) -> Result<(), String> {
    let link = format!("./{LOADER}.js");
    let mut wrong = Vec::new();
    for (module, name) in &main.imports {
        if module == PLACEHOLDER {
            wrong.push(format!(
                "главный модуль: заглушка разреза {name} — разрез не сделан"
            ));
        } else if *module == link && !loader.contains(&format!("export const {name} ")) {
            wrong.push(format!(
                "главный модуль ждёт от загрузчика {name}, а его там нет"
            ));
        }
    }
    for (part, module) in parts {
        for (from, name) in &module.imports {
            if from == PLACEHOLDER {
                wrong.push(format!("{part}: заглушка разреза {name}"));
            } else if from != SHARED {
                wrong.push(format!(
                    "{part}: импорт {from}.{name} — части берут только у главного модуля"
                ));
            } else if !main.exports.contains(name) && !loader.contains(&format!("{name}: ")) {
                wrong.push(format!("{part}: у главного модуля нет экспорта {name}"));
            }
        }
    }
    if wrong.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "части и главный модуль не сходятся:\n  {}",
            wrong.join("\n  ")
        ))
    }
}

/// У каждой части ровно одна точка разреза. Точка разреза импортирует
/// свою функцию как `__wasm_split_00<часть>00_import_<хэш>_<функция>`
/// (`wasm_split_macros` 0.2.2), а часть называется по имени функции
/// (`crates/app/src/ui/split.rs`): две функции с одним именем в разных
/// модулях программа разреза молча кладёт в одну часть, и та грузится
/// целиком, когда нужна любая из них.
fn one_point_per_part(raw: &Module) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    let mut twice = BTreeSet::new();
    for (module, name) in &raw.imports {
        let part = (module == PLACEHOLDER)
            .then(|| {
                name.strip_prefix("__wasm_split_00")?
                    .split_once("00_import_")
            })
            .flatten();
        if let Some((part, _)) = part
            && !seen.insert(part)
        {
            twice.insert(part);
        }
    }
    if twice.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "у частей {} больше одной точки разреза: имя функции в split! — имя части,              и у каждой оно своё",
            twice.into_iter().collect::<Vec<_>>().join(", ")
        ))
    }
}

/// Загрузчик с общими экспортами по алфавиту. Программа разреза
/// перечисляет их из `HashSet`, а его порядок свой в каждом запуске
/// (`reexported_shared_symbols` в `js.rs`, `wasm_split_cli_support` 0.2.3):
/// тот же код давал бы другой загрузчик, другой хэш и другой `sw.js`,
/// и после пересборки без единой правки все увидели бы «Есть новая
/// версия» (найдено 2026-10-13: из пяти сборок подряд — два разных
/// загрузчика). Список стоит в строках `const { … } = initSync(…)`
/// и `Object.assign(sharedImports.__wasm_split, { … })`.
fn sorted_shares(loader: &str) -> String {
    loader
        .split_inclusive('\n')
        .map(|line| {
            let listed = line.contains("= initSync(")
                || line.contains("Object.assign(sharedImports.__wasm_split, {");
            let (Some(open), Some(close)) = (line.find("{ "), line.find(" }")) else {
                return line.to_owned();
            };
            if !listed || close < open {
                return line.to_owned();
            }
            let mut names: Vec<&str> = line[open + 2..close]
                .split(',')
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .collect();
            names.sort_unstable();
            let names: String = names.iter().map(|name| format!("{name}, ")).collect();
            format!("{}{{ {names} }}{}", &line[..open], &line[close + 2..])
        })
        .collect()
}

/// `text` с `from` вместо `to`. `required` — `from` обязан встретиться:
/// иначе формат вывода wasm-bindgen или разреза поменялся, и файл
/// сослался бы на имя без хэша.
fn replace(text: &str, from: &str, to: &str, required: bool) -> Result<String, String> {
    if required && !text.contains(from) {
        return Err(format!(
            "в JS нет «{from}»: сменился вывод wasm-bindgen или разреза"
        ));
    }
    Ok(text.replace(from, to))
}

/// Ссылки на файлы запуска: имя и `integrity`.
struct Start<'a> {
    public_url: &'a str,
    /// Клей wasm-bindgen с запуском в конце — модуль страницы.
    glue: (&'a str, &'a str),
    /// Нет частей — нет и загрузчика.
    loader: Option<(&'a str, String)>,
    wasm: (&'a str, &'a str),
}

/// Страница с запуском и предзагрузками перед `</head>`. Предзагрузки —
/// как у Trunk: загрузчик частей нужен клею, а wasm качается параллельно
/// с ними, иначе каждый — лишний круг сети.
fn with_start(page: &str, links: &Start) -> Result<String, String> {
    if page.matches("</head>").count() != 1 {
        return Err("в index.html ожидался ровно один </head>".into());
    }
    let url = links.public_url;
    let mut tags = format!(
        "<script type=module src=\"{url}{}\" integrity=\"{}\" crossorigin></script>",
        links.glue.0, links.glue.1
    );
    if let Some((name, integrity)) = &links.loader {
        tags.push_str(&format!(
            "<link rel=modulepreload href=\"{url}{name}\" integrity=\"{integrity}\" crossorigin>"
        ));
    }
    tags.push_str(&format!(
        "<link rel=preload as=fetch type=application/wasm href=\"{url}{}\" integrity=\"{}\" crossorigin>",
        links.wasm.0, links.wasm.1
    ));
    Ok(page.replacen("</head>", &format!("{tags}</head>"), 1))
}

/// `integrity` по SRI: SHA-384 в base64, как у Trunk.
fn sri(bytes: &[u8]) -> String {
    format!("sha384-{}", STANDARD.encode(Sha384::digest(bytes)))
}

fn file_name(path: &Path) -> Result<String, String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
        .ok_or_else(|| format!("{}: имя не UTF-8", path.display()))
}

fn read(path: &Path) -> Result<Vec<u8>, String> {
    fs::read(path).map_err(|error| format!("{}: {error}", path.display()))
}

fn text(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn module(imports: &[(&str, &str)], exports: &[&str]) -> Module {
        Module {
            imports: imports
                .iter()
                .map(|(m, n)| ((*m).to_owned(), (*n).to_owned()))
                .collect(),
            exports: exports.iter().map(|n| (*n).to_owned()).collect(),
        }
    }

    const LOADER_JS: &str = "export const __wasm_split_load_form = wrapAsyncCb(makeLoad(x, []));\n";

    fn main_ok() -> Module {
        module(
            &[
                ("./kvartik-app_bg.js", "__wbg_log"),
                ("./kvartik-split.js", "__wasm_split_load_form"),
            ],
            &[
                "memory",
                "__indirect_function_table",
                "__wasm_split_shared0",
            ],
        )
    }

    fn part_ok() -> (String, Module) {
        (
            "split_form.wasm".to_owned(),
            module(
                &[(SHARED, "memory"), (SHARED, "__indirect_function_table")],
                &[],
            ),
        )
    }

    #[test]
    fn matching_parts_pass() {
        assert_eq!(check(&main_ok(), &[part_ok()], LOADER_JS), Ok(()));
    }

    /// Канарейку отладочной сборки части берут у загрузчика, а не
    /// у главного модуля.
    #[test]
    fn debug_canary_comes_from_the_loader() {
        let (name, mut part) = part_ok();
        part.imports
            .push((SHARED.to_owned(), "__canary_f35f".to_owned()));
        let loader = format!(
            "{LOADER_JS}sharedImports = {{ __wasm_split: {{ __canary_f35f: ~0xdead, }} }};\n"
        );
        assert_eq!(check(&main_ok(), &[(name.clone(), part)], &loader), Ok(()));
        let (_, mut part) = part_ok();
        part.imports
            .push((SHARED.to_owned(), "__canary_f35f".to_owned()));
        assert!(check(&main_ok(), &[(name, part)], LOADER_JS).is_err());
    }

    /// Каждое расхождение — своя строка: проверка, которая видит только
    /// первое, прячет остальные (Правило 2).
    #[test]
    fn every_mismatch_is_named() {
        let main = module(
            &[
                (PLACEHOLDER, "__wasm_split_00form00_import"),
                ("./kvartik-split.js", "__wasm_split_load_view"),
            ],
            &["memory"],
        );
        let parts = [(
            "split_form.wasm".to_owned(),
            module(
                &[
                    (SHARED, "__indirect_function_table"),
                    ("./kvartik-app_bg.js", "__wbg_log"),
                    (PLACEHOLDER, "x"),
                ],
                &[],
            ),
        )];
        let error = check(&main, &parts, LOADER_JS).expect_err("расхождения есть");
        for expected in [
            "разрез не сделан",
            "ждёт от загрузчика __wasm_split_load_view",
            "нет экспорта __indirect_function_table",
            "импорт ./kvartik-app_bg.js.__wbg_log",
            "split_form.wasm: заглушка разреза x",
        ] {
            assert!(error.contains(expected), "нет «{expected}» в {error}");
        }
    }

    /// Импорты и экспорты читаются из настоящего wasm.
    #[test]
    fn parses_imports_and_exports() {
        use wasm_encoder::{
            EntityType, ExportKind, ExportSection, ImportSection, MemorySection, MemoryType,
            Module as Encoder,
        };
        let mut wasm = Encoder::new();
        let mut imports = ImportSection::new();
        let memory = MemoryType {
            minimum: 1,
            maximum: None,
            memory64: false,
            shared: false,
            page_size_log2: None,
        };
        imports.import(SHARED, "memory", EntityType::Memory(memory));
        wasm.section(&imports);
        let mut memories = MemorySection::new();
        memories.memory(memory);
        wasm.section(&memories);
        let mut exports = ExportSection::new();
        exports.export("own", ExportKind::Memory, 1);
        wasm.section(&exports);
        let parsed = Module::parse(&wasm.finish()).expect("модуль читается");
        assert_eq!(parsed.imports, [(SHARED.to_owned(), "memory".to_owned())]);
        assert_eq!(parsed.exports.iter().collect::<Vec<_>>(), ["own"]);
    }

    #[test]
    fn one_point_per_part_only() {
        let point = |part: &str, function: &str| {
            (
                PLACEHOLDER,
                format!("__wasm_split_00{part}00_import_0f_{function}"),
            )
        };
        let raw = |points: &[(&str, String)]| Module {
            imports: points
                .iter()
                .map(|(m, n)| ((*m).to_owned(), n.clone()))
                .collect(),
            exports: BTreeSet::new(),
        };
        let fine = [
            point("profile_form", "profile_form"),
            point("client_form", "client_form"),
            (PLACEHOLDER, "__wasm_split_load_client_form".to_owned()),
        ];
        assert_eq!(one_point_per_part(&raw(&fine)), Ok(()));
        let merged = [
            point("form", "form"),
            point("form", "form"),
            point("view", "view"),
        ];
        let error = one_point_per_part(&raw(&merged)).expect_err("две точки в одной части");
        assert!(error.contains("частей form больше одной"), "{error}");
    }

    #[test]
    fn start_goes_before_head_end() {
        let links = Start {
            public_url: "/",
            glue: ("kvartik-app-1.js", "sha384-b"),
            loader: Some(("kvartik-split-1.js", "sha384-c".to_owned())),
            wasm: ("kvartik-app-1_bg.wasm", "sha384-d"),
        };
        let page = with_start(
            "<html><head><title>K</title></head><body></body></html>",
            &links,
        )
        .expect("страница с </head>");
        assert_eq!(
            page,
            "<html><head><title>K</title>\
             <script type=module src=\"/kvartik-app-1.js\" integrity=\"sha384-b\" crossorigin></script>\
             <link rel=modulepreload href=\"/kvartik-split-1.js\" integrity=\"sha384-c\" crossorigin>\
             <link rel=preload as=fetch type=application/wasm href=\"/kvartik-app-1_bg.wasm\" \
             integrity=\"sha384-d\" crossorigin></head><body></body></html>"
        );
        assert!(with_start("<html><body></body></html>", &links).is_err());
        assert!(with_start("</head></head>", &links).is_err());
    }

    /// Тот же код — тот же загрузчик, в каком бы порядке программа разреза
    /// ни перечислила общие экспорты.
    #[test]
    fn shares_are_sorted() {
        let loader = |names: &str| {
            format!(
                "let sharedImports = undefined;\n\
                 \x20       const {{ {names} }} = initSync(undefined, undefined);\n\
                 \x20       Object.assign(sharedImports.__wasm_split, {{ {names} }});\n\
                 const {{ x, a }} = other;\n"
            )
        };
        let one = sorted_shares(&loader(
            "memory, __indirect_function_table, __wasm_split_shared0, ",
        ));
        let two = sorted_shares(&loader(
            "__wasm_split_shared0, memory, __indirect_function_table, ",
        ));
        assert_eq!(one, two);
        assert_eq!(
            one,
            loader("__indirect_function_table, __wasm_split_shared0, memory, ")
        );
        assert!(
            one.ends_with("const { x, a } = other;\n"),
            "чужая строка не тронута: {one}"
        );
    }

    #[test]
    fn a_required_name_must_be_there() {
        assert_eq!(
            replace("from \"./a.js\";", "\"./a.js\"", "\"./a-1.js\"", true),
            Ok("from \"./a-1.js\";".to_owned())
        );
        assert!(replace("from \"./b.js\";", "\"./a.js\"", "\"./a-1.js\"", true).is_err());
        assert_eq!(replace("x", "y", "z", false), Ok("x".to_owned()));
    }

    #[test]
    fn version_comes_from_the_lock() {
        let lock = "[[package]]\r\nname = \"wasm-bindgen\"\r\nversion = \"0.2.129\"\r\n\r\n\
                    [[package]]\nname = \"wasm-bindgen-futures\"\nversion = \"0.4.79\"\n";
        assert_eq!(locked_version(lock, "wasm-bindgen"), Some("0.2.129"));
        assert_eq!(locked_version(lock, "wasm-bindgen-futures"), Some("0.4.79"));
        assert_eq!(locked_version(lock, "leptos"), None);
    }

    #[test]
    fn cargo_names_the_wasm() {
        let messages = concat!(
            r#"{"reason":"compiler-artifact","target":{"name":"leptos"},"filenames":["/t/libleptos.rlib"]}"#,
            "\n",
            r#"{"reason":"compiler-artifact","target":{"name":"kvartik-app"},"filenames":["/t/kvartik-app.wasm"]}"#,
            "\n",
            r#"{"reason":"build-finished","success":true}"#,
        );
        assert_eq!(
            artifact(messages, "kvartik-app"),
            Some(PathBuf::from("/t/kvartik-app.wasm"))
        );
        assert_eq!(artifact(messages, "kvartik-core"), None);
    }

    /// SRI — как у браузера: SHA-384 пустой строки известен.
    #[test]
    fn integrity_is_sha384_base64() {
        assert_eq!(
            sri(b""),
            "sha384-OLBgp1GsljhM2TJ+sbHjaiH9txEUvgdDTAzHv2P24donTt6/529l+9Ua0vFImLlb"
        );
    }
}
