//! Сборка сайта после Trunk: раскладка сайта (задача 65), wasm окна
//! с частями (задача 58, `wasm.rs`) и `sw.js` со списком файлов для кэша
//! и хэшем сборки (задача 8).
//!
//! Зовёт её Trunk хуком `post_build` (`crates/app/Trunk.toml`): к этому
//! времени остальные файлы сайта уже лежат в папке сборки, а `index.html`
//! дописан (по исходникам trunk 0.22.0-rc.2, `src/pipelines/html.rs`).
//! Папку она берёт из переменной `TRUNK_STAGING_DIR`, адрес программы —
//! из `TRUNK_PUBLIC_URL`, а остальное — из аргументов:
//! `kvartik-sw [--root <папка>] [--wasm <пакет>] <шаблон sw.js>`.
//!
//! Программа живёт в папке сайта — `/app/`, `public_url` в `Trunk.toml`, —
//! а корень отдан лендингу (задача 65). Trunk кладёт всё собранное в корень
//! папки сборки, а `public_url` меняет только адреса в странице (по
//! исходникам trunk 0.22.0-rc.2, `pipelines/html.rs`, `pipelines/css.rs`).
//! Поэтому с `--root` программа перекладывает собранное Trunk в папку
//! программы, а в корень кладёт файлы корня сайта из `<папка>`. Разложил
//! бы и сам Trunk (`html_output` и `data-target-path`), но имя папки стояло
//! бы тогда в восьми местах, а не в одном `public_url`.
//!
//! С `--wasm` она собирает wasm пакета и вписывает его запуск в страницу,
//! а потом пишет `sw.js` программы по всем файлам, wasm в том числе. Одна
//! программа, а не три хука: хуки одной стадии Trunk идут одновременно (по
//! исходникам trunk 0.22.0-rc.2, `src/hooks.rs`), а `sw.js` должен видеть
//! готовый wasm. Без `--root` и `--wasm` — только `sw.js`: так переписывают
//! `sw.js` у копии `dist/` для прогона новой версии (CLAUDE.md, Правило 2).
//!
//! Зачем: браузер узнаёт о новой версии сайта, только сравнивая `sw.js`
//! с прошлым байт в байт (по спецификации Service Worker, «Update»). Имена
//! wasm, клея JS и CSS Trunk 0.22 считает от их байтов, а шрифты, значки
//! и манифест лежат под постоянными именами. Хэш всех файлов в `sw.js`
//! меняет его с любым из них — и новые шрифты доходят до тех, у кого
//! в кэше старые.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

mod wasm;

/// Строка шаблона, вместо которой встаёт хэш сборки.
const BUILD_LINE: &str = r#"const BUILD = "dev";"#;
/// Строка шаблона, вместо которой встаёт список файлов.
const FILES_LINE: &str = "const FILES = [];";
/// Строка шаблона, вместо которой встают файлы, на которые ссылается
/// страница: по ним service worker видит, что страница той же сборки.
const LINKED_LINE: &str = "const LINKED = [];";
/// Что программа пишет в папку программы.
const OUTPUT: &str = "sw.js";
/// Страница программы: в кэше она лежит под адресом папки (`SHELL`
/// в шаблоне), а в списке её нет — шапка шаблона объясняет почему.
const PAGE: &str = "index.html";

fn main() -> ExitCode {
    match run() {
        Ok(summary) => {
            println!("{summary}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("kvartik-sw: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<String, String> {
    let staging = env::var_os("TRUNK_STAGING_DIR")
        .map(PathBuf::from)
        .ok_or("нет TRUNK_STAGING_DIR: программу зовёт хук Trunk (crates/app/Trunk.toml)")?;
    let public_url = env::var("TRUNK_PUBLIC_URL")
        .map_err(|_| "нет TRUNK_PUBLIC_URL: программу зовёт хук Trunk (crates/app/Trunk.toml)")?;
    let app = app_folder(&public_url)?;
    let (mut root, mut package, mut template_path) = (None, None, None);
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--root" => root = Some(PathBuf::from(args.next().ok_or("после --root ожидалась папка")?)),
            "--wasm" => package = Some(args.next().ok_or("после --wasm ожидалось имя пакета")?),
            _ if template_path.is_none() => template_path = Some(PathBuf::from(arg)),
            _ => return Err(format!("лишний аргумент «{arg}»")),
        }
    }
    let template_path = template_path.ok_or("ожидался путь к шаблону sw.js")?;
    let template = fs::read_to_string(&template_path)
        .map_err(|error| format!("{}: {error}", template_path.display()))?;
    if let Some(root) = &root {
        lay_out(&staging, app, root)?;
    }
    let app_dir = staging.join(app);
    let wasm = match &package {
        Some(package) => {
            let built = wasm::build(package, &app_dir, &public_url)?;
            format!(", wasm {} с частями: {}", built.hash, built.parts)
        }
        None => String::new(),
    };
    let files = collect(&staging, app)?;
    let build = build_id(&template, &files);
    let sw = render(&template, &build, &files, app)?;
    let output = app_dir.join(OUTPUT);
    fs::write(&output, sw).map_err(|error| format!("{}: {error}", output.display()))?;
    Ok(format!(
        "kvartik-sw: сборка {build}, программа в {public_url}, файлов в кэше {}{wasm}",
        precached(&files, app).count() + 1
    ))
}

/// Папка программы в сайте — `public_url` без косых черт по краям:
/// `/app/` — `app`. Корень (`/`) не годится: там лендинг и файлы корня
/// (задача 65), и `sw.js` программы лёг бы на место `sw.js` корня. Адрес
/// с доменом — тоже: папку берут из пути, а хостинг отдаёт файлы по нему.
fn app_folder(public_url: &str) -> Result<&str, String> {
    public_url
        .strip_prefix('/')
        .and_then(|path| path.strip_suffix('/'))
        .filter(|path| {
            path.split('/')
                .all(|part| !part.is_empty() && part != "." && part != ".." && !part.contains('\\'))
        })
        .ok_or_else(|| {
            format!(
                "public_url «{public_url}»: программа живёт в папке сайта, ожидался путь \
                 вида /app/ (crates/app/Trunk.toml)"
            )
        })
}

/// Раскладка сайта (задача 65): всё, что собрал Trunk, — в папку программы
/// `app`, а в корень — файлы корня сайта из папки `root`. До раскладки
/// в сборке нет ничего с именем папки программы, а после — ни один файл
/// корня не лёг на чужое место: иначе одно молча затёрло бы другое.
fn lay_out(staging: &Path, app: &str, root: &Path) -> Result<(), String> {
    let top = app.split('/').next().unwrap_or(app);
    if staging.join(top).exists() {
        return Err(format!("в сборке Trunk уже есть «{top}» — имя папки программы"));
    }
    let built: Vec<_> = fs::read_dir(staging)
        .and_then(|entries| entries.map(|entry| entry.map(|entry| entry.file_name())).collect())
        .map_err(|error| format!("{}: {error}", staging.display()))?;
    let app_dir = staging.join(app);
    fs::create_dir_all(&app_dir).map_err(|error| format!("{}: {error}", app_dir.display()))?;
    for name in built {
        let (from, to) = (staging.join(&name), app_dir.join(&name));
        fs::rename(&from, &to).map_err(|error| format!("{}: {error}", from.display()))?;
    }
    copy_new(root, staging)
}

/// Скопировать содержимое папки `from` в `to`, со вложенными папками.
/// Файл, который уже есть, — ошибка, а не замена. Скрытые файлы не идут,
/// как и в [`collect`].
fn copy_new(from: &Path, to: &Path) -> Result<(), String> {
    let entries = fs::read_dir(from).map_err(|error| format!("{}: {error}", from.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("{}: {error}", from.display()))?;
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        let target = to.join(entry.file_name());
        let kind = entry
            .file_type()
            .map_err(|error| format!("{}: {error}", entry.path().display()))?;
        if kind.is_dir() {
            fs::create_dir_all(&target).map_err(|error| format!("{}: {error}", target.display()))?;
            copy_new(&entry.path(), &target)?;
        } else if target.exists() {
            return Err(format!("{}: файл корня лёг бы на файл программы", target.display()));
        } else {
            fs::copy(entry.path(), &target)
                .map_err(|error| format!("{}: {error}", entry.path().display()))?;
        }
    }
    Ok(())
}

/// Файл сайта: путь от корня сайта через `/` и содержимое.
#[derive(Debug)]
struct File {
    path: String,
    bytes: Vec<u8>,
}

/// Все файлы сайта — и корня, и программы в папке `app`, — по пути. Кроме
/// `sw.js` программы — его пишет эта программа — и скрытых файлов: Trunk
/// держит в папках служебные (`.trunk-lock`). Настройки хостинга
/// (`_headers`) здесь есть: в кэш они не идут ([`precached`]), а в хэш
/// сборки идут.
fn collect(root: &Path, app: &str) -> Result<Vec<File>, String> {
    fn walk(dir: &Path, prefix: &str, skip: &str, out: &mut Vec<File>) -> Result<(), String> {
        let entries = fs::read_dir(dir).map_err(|error| format!("{}: {error}", dir.display()))?;
        for entry in entries {
            let entry = entry.map_err(|error| format!("{}: {error}", dir.display()))?;
            let name = entry.file_name();
            let name = name
                .to_str()
                .ok_or_else(|| format!("{}: имя файла не UTF-8", entry.path().display()))?;
            if name.starts_with('.') {
                continue;
            }
            let path = format!("{prefix}{name}");
            let kind = entry
                .file_type()
                .map_err(|error| format!("{path}: {error}"))?;
            if kind.is_dir() {
                walk(&entry.path(), &format!("{path}/"), skip, out)?;
            } else if path != skip {
                let bytes = fs::read(entry.path()).map_err(|error| format!("{path}: {error}"))?;
                out.push(File { path, bytes });
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    walk(root, "", &format!("{app}/{OUTPUT}"), &mut files)?;
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

/// Хэш сборки: FNV-1a, 64 бита, по шаблону `sw.js` и путям и содержимому
/// всех файлов. Шаблон — тоже: новый `sw.js` с теми же файлами иначе писал
/// бы в кэш, которым открывается работающая версия. И настройки хостинга:
/// страница лежит в кэше вместе со своими заголовками, CSP в том числе,
/// и открывается с ними же. Новый `_headers` без нового `sw.js` не дошёл
/// бы до тех, у кого программа уже в кэше, никогда.
///
/// Свой, а не `DefaultHasher`: тот может смениться с версией Rust, и та же
/// сборка получила бы другой хэш — у всех пользователей разом «новая
/// версия», в которой ничего нового. Стойкость к подбору здесь не нужна:
/// хэш только отличает одну сборку от другой. Длина перед содержимым — чтобы
/// граница между путём и байтами не могла сдвинуться.
fn build_id(template: &str, files: &[File]) -> String {
    digest(
        std::iter::once((OUTPUT, template.as_bytes()))
            .chain(files.iter().map(|file| (file.path.as_str(), file.bytes.as_slice()))),
    )
}

/// Хэш путей и содержимого: FNV-1a, 64 бита, 16 знаков. Почему свой —
/// у [`build_id`]; им же `wasm.rs` называет файлы wasm.
fn digest<'a>(entries: impl IntoIterator<Item = (&'a str, &'a [u8])>) -> String {
    fn feed(hash: u64, bytes: &[u8]) -> u64 {
        bytes.iter().fold(hash, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
        })
    }
    let hash = entries.into_iter().fold(0xcbf2_9ce4_8422_2325, |hash, (path, bytes)| {
        let hash = feed(hash, path.as_bytes());
        let hash = feed(hash, &[0]);
        let hash = feed(hash, &(bytes.len() as u64).to_le_bytes());
        feed(hash, bytes)
    });
    format!("{hash:016x}")
}

/// Файлы, которые service worker программы кладёт в кэш, — пути от его
/// папки `app`: все файлы программы, кроме страниц HTML и настроек
/// хостинга (задача 15) — установка, которая их ждёт, не удалась бы ни
/// у кого никогда. Настройки, имена которых начинаются с `_` (`_headers`),
/// Cloudflare Pages не отдаёт вовсе. На `/404.html` он отвечает
/// переадресацией на `/404` (308; по исходникам workers-sdk,
/// pages-shared/asset-server, 2026-10-15), а ответ с переадресацией
/// установка не принимает. Страница программы лежит в кэше под адресом
/// папки — шапка шаблона. Файлы корня сайта — не программы: их service
/// worker не трогает (задача 65).
fn precached<'a>(files: &'a [File], app: &str) -> impl Iterator<Item = &'a str> {
    let prefix = format!("{app}/");
    files
        .iter()
        .filter_map(move |file| file.path.strip_prefix(prefix.as_str()))
        .filter(|path| !path.ends_with(".html") && !hosting(path))
}

/// Настройка хостинга: имя файла начинается с `_`.
fn hosting(path: &str) -> bool {
    path.rsplit('/').next().unwrap_or(path).starts_with('_')
}

/// Файлы из списка, на которые ссылается страница программы: wasm, клей
/// JS, CSS, манифест, значки. Страница не своей сборки их не назовёт.
fn linked<'a>(files: &'a [File], app: &str) -> Result<Vec<&'a str>, String> {
    let path = format!("{app}/{PAGE}");
    let page = files
        .iter()
        .find(|file| file.path == path)
        .ok_or(format!("в сборке нет {path}"))?;
    let page = String::from_utf8_lossy(&page.bytes);
    Ok(precached(files, app).filter(|file| page.contains(file)).collect())
}

/// `sw.js` программы из шаблона: каждая из трёх строк на месте ровно по
/// разу, иначе ошибка — молча оставленное `"dev"` выключило бы обновления
/// у всех.
fn render(template: &str, build: &str, files: &[File], app: &str) -> Result<String, String> {
    fn list<'a>(name: &str, paths: impl Iterator<Item = &'a str>) -> String {
        let mut out = format!("const {name} = [\n");
        for path in paths {
            out.push_str(&format!("  {},\n", js_string(path)));
        }
        out.push_str("];\n");
        out
    }
    let linked = linked(files, app)?;
    let mut out = String::with_capacity(template.len() + files.len() * 96);
    let mut seen = [0; 3];
    for line in template.split_inclusive('\n') {
        match line.trim_end() {
            BUILD_LINE => {
                seen[0] += 1;
                out.push_str(&format!("const BUILD = {};\n", js_string(build)));
            }
            FILES_LINE => {
                seen[1] += 1;
                out.push_str(&list("FILES", precached(files, app)));
            }
            LINKED_LINE => {
                seen[2] += 1;
                out.push_str(&list("LINKED", linked.iter().copied()));
            }
            _ => out.push_str(line),
        }
    }
    if seen != [1, 1, 1] {
        return Err(format!(
            "строки шаблона «{BUILD_LINE}», «{FILES_LINE}» и «{LINKED_LINE}» встретились \
             {seen:?} раз; нужно ровно по разу"
        ));
    }
    Ok(out)
}

/// Строка JS в кавычках. Пути сайта — ASCII, но кавычка или обратная
/// черта в имени файла не должны ломать `sw.js`.
fn js_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if u32::from(c) < 0x20 || c == '\u{2028}' || c == '\u{2029}' => {
                out.push_str(&format!("\\u{:04x}", u32::from(c)));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Настоящий шаблон из окна: проверка ловит правку, после которой
    /// программа его не узнала бы.
    const TEMPLATE: &str = include_str!("../../app/public/sw.js");

    fn file(path: &str, bytes: &[u8]) -> File {
        File {
            path: path.to_owned(),
            bytes: bytes.to_vec(),
        }
    }

    /// Папка во временной папке системы — своя у каждого теста.
    struct Temp(PathBuf);

    impl Temp {
        fn new(name: &str) -> Self {
            let dir = env::temp_dir().join(format!("kvartik-sw-{}-{name}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).expect("временная папка создаётся");
            Self(dir)
        }

        fn put(&self, path: &str, bytes: &[u8]) {
            let full = self.0.join(path);
            fs::create_dir_all(full.parent().expect("у файла есть папка"))
                .expect("папка создаётся");
            fs::write(full, bytes).expect("файл пишется");
        }
    }

    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// Вложенные папки — через `/` и на Windows: путь с `\` service worker
    /// попросил бы у сервера буквально, и установка упала бы на 404.
    /// Служебные файлы Trunk и прошлый `sw.js` программы в сборку не
    /// попадают, а `sw.js` корня попадает: это другой файл. В кэш — только
    /// файлы программы, путями от её папки, без страниц HTML и настроек
    /// хостинга.
    #[test]
    fn collects_site_files_with_forward_slashes() {
        let temp = Temp::new("collect");
        temp.put("404.html", b"<html>");
        temp.put("_headers", b"/*\n  X-Frame-Options: DENY");
        temp.put("_redirects", b"/ /app/ 302");
        temp.put("sw.js", b"root sw");
        temp.put("app/index.html", b"<html>");
        temp.put("app/kvartik-app-1a2b.js", b"js");
        temp.put("app/fonts/noto-sans-latin-400.ttf", b"ttf");
        temp.put("app/icons/icon-192.png", b"png");
        temp.put("app/_headers", b"");
        temp.put("app/sw.js", b"old sw");
        temp.put(".trunk-lock", b"");
        temp.put(".stage/old.js", b"old");

        let files = collect(&temp.0, "app").expect("папка читается");
        let paths: Vec<&str> = files.iter().map(|file| file.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                "404.html",
                "_headers",
                "_redirects",
                "app/_headers",
                "app/fonts/noto-sans-latin-400.ttf",
                "app/icons/icon-192.png",
                "app/index.html",
                "app/kvartik-app-1a2b.js",
                "sw.js",
            ]
        );
        assert_eq!(
            precached(&files, "app").collect::<Vec<_>>(),
            ["fonts/noto-sans-latin-400.ttf", "icons/icon-192.png", "kvartik-app-1a2b.js"]
        );
    }

    /// Папка программы — путь `public_url`; корень, адрес с доменом и путь
    /// с переходом вверх — ошибка сборки, а не программа не на своём месте.
    #[test]
    fn app_folder_comes_from_the_public_url() {
        assert_eq!(app_folder("/app/"), Ok("app"));
        assert_eq!(app_folder("/a/b/"), Ok("a/b"));
        let mut wrong = Vec::new();
        for url in ["/", "", "app/", "/app", "https://kvartik.com/app/", "/../", "/a//b/", "/a\\b/"] {
            if app_folder(url).is_ok() {
                wrong.push(url);
            }
        }
        assert!(wrong.is_empty(), "приняты: {wrong:?}");
    }

    /// Собранное Trunk уходит в папку программы целиком, со вложенными
    /// папками, а файлы корня ложатся в корень. Имя папки программы в
    /// сборке Trunk и файл корня на месте файла программы — ошибка, а не
    /// молча затёртый файл.
    #[test]
    fn lays_the_site_out() {
        let temp = Temp::new("layout");
        temp.put("stage/index.html", b"page");
        temp.put("stage/fonts/a.ttf", b"ttf");
        temp.put("root/_headers", b"headers");
        temp.put("root/sw.js", b"root sw");
        temp.put("root/.hidden", b"");
        let stage = temp.0.join("stage");
        lay_out(&stage, "app", &temp.0.join("root")).expect("раскладка");
        let files = collect(&stage, "app").expect("папка читается");
        let paths: Vec<&str> = files.iter().map(|file| file.path.as_str()).collect();
        assert_eq!(paths, ["_headers", "app/fonts/a.ttf", "app/index.html", "sw.js"]);

        let clash = Temp::new("clash");
        clash.put("stage/app", b"file");
        clash.put("root/_headers", b"");
        let error = lay_out(&clash.0.join("stage"), "app", &clash.0.join("root"))
            .expect_err("в сборке Trunk есть «app»");
        assert!(error.contains("уже есть «app»"), "{error}");

        let over = Temp::new("over");
        over.put("stage/index.html", b"page");
        over.put("root/app/index.html", b"other page");
        let error = lay_out(&over.0.join("stage"), "app", &over.0.join("root"))
            .expect_err("файл корня на месте файла программы");
        assert!(error.contains("лёг бы на файл программы"), "{error}");
    }

    /// Новый `_headers` — новая сборка: страница в кэше service worker
    /// открывается со старыми заголовками, пока не сменится `sw.js`.
    #[test]
    fn hosting_settings_change_the_build() {
        let temp = Temp::new("headers");
        temp.put("app/index.html", b"<html>");
        temp.put("_headers", b"/*\n  Content-Security-Policy: default-src 'self'");
        let before = build_id(TEMPLATE, &collect(&temp.0, "app").expect("папка читается"));
        temp.put("_headers", b"/*\n  Content-Security-Policy: default-src 'self'; img-src 'self' blob:");
        let after = build_id(TEMPLATE, &collect(&temp.0, "app").expect("папка читается"));
        assert_ne!(before, after);
    }

    /// Хэш меняется с любым байтом, с именем файла и с шаблоном `sw.js`,
    /// а одна и та же сборка даёт один и тот же хэш: иначе все получили бы
    /// «новую версию», в которой ничего нового.
    #[test]
    fn build_id_follows_every_byte_and_name() {
        let base = [file("a.js", b"one"), file("fonts/f.ttf", b"font")];
        let id = build_id("sw", &base);
        assert_eq!(id.len(), 16);
        assert_eq!(id, build_id("sw", &[file("a.js", b"one"), file("fonts/f.ttf", b"font")]));

        let mut changed = Vec::new();
        for (what, template, files) in [
            ("байт шрифта", "sw", [file("a.js", b"one"), file("fonts/f.ttf", b"fonT")]),
            ("имя", "sw", [file("b.js", b"one"), file("fonts/f.ttf", b"font")]),
            ("граница", "sw", [file("a.jso", b"ne"), file("fonts/f.ttf", b"font")]),
            ("шаблон", "sw2", [file("a.js", b"one"), file("fonts/f.ttf", b"font")]),
        ] {
            if build_id(template, &files) == id {
                changed.push(what);
            }
        }
        assert!(changed.is_empty(), "хэш не заметил: {changed:?}");
    }

    /// Настоящий шаблон: все три строки заменены, страница в список не
    /// попала (она в кэше под адресом папки), в `LINKED` — ровно то, на что
    /// она ссылается, и следа от «dev» не осталось.
    #[test]
    fn renders_the_real_template() {
        let files = [
            file("404.html", b"<html>"),
            file(
                "app/index.html",
                br#"<link href=/app/kvartik-app-1a2b.js><link href="kvartik-app-1a2b_bg.wasm">"#,
            ),
            file("app/fonts/noto-sans-armenian-400.ttf", b"ttf"),
            file("app/kvartik-app-1a2b.js", b"js"),
            file("app/kvartik-app-1a2b_bg.wasm", b"wasm"),
            file("sw.js", b"root sw"),
        ];
        let sw = render(TEMPLATE, "0123456789abcdef", &files, "app").expect("шаблон узнан");
        assert!(sw.contains("const BUILD = \"0123456789abcdef\";\n"));
        assert!(sw.contains(
            "const FILES = [\n  \"fonts/noto-sans-armenian-400.ttf\",\n  \"kvartik-app-1a2b.js\",\n  \"kvartik-app-1a2b_bg.wasm\",\n];\n"
        ));
        assert!(sw.contains(
            "const LINKED = [\n  \"kvartik-app-1a2b.js\",\n  \"kvartik-app-1a2b_bg.wasm\",\n];\n"
        ));
        assert!(!sw.contains(BUILD_LINE) && !sw.contains(FILES_LINE) && !sw.contains(LINKED_LINE));
        assert!(!sw.contains("index.html\"") && !sw.contains("\"sw.js\""));
    }

    /// Сборка без страницы программы — ошибка: проверять сборку страницы
    /// было бы не по чему, а программа без неё не откроется. Страница
    /// в корне сайта — не её страница.
    #[test]
    fn a_build_without_the_page_is_an_error() {
        assert!(render(TEMPLATE, "id", &[file("app/a.js", b"")], "app").is_err());
        assert!(render(TEMPLATE, "id", &[file("index.html", b""), file("app/a.js", b"")], "app").is_err());
    }

    /// Шаблон без строки или с ней дважды — ошибка сборки, а не `sw.js`,
    /// который никогда не обновится. Концы строк Windows (`\r\n`) — не
    /// повод не узнать строку.
    #[test]
    fn template_needs_each_line_exactly_once() {
        let files = [file("app/index.html", b""), file("app/a.js", b"")];
        let all = format!("{BUILD_LINE}\r\n{FILES_LINE}\r\n{LINKED_LINE}\r\n");
        assert!(render(&all, "id", &files, "app").is_ok());

        let mut wrong = Vec::new();
        for template in [
            format!("{FILES_LINE}\n{LINKED_LINE}\n"),
            format!("{BUILD_LINE}\n{LINKED_LINE}\n"),
            format!("{BUILD_LINE}\n{FILES_LINE}\n"),
            format!("{BUILD_LINE}\n{BUILD_LINE}\n{FILES_LINE}\n{LINKED_LINE}\n"),
            format!("{BUILD_LINE}\n{FILES_LINE}\n{FILES_LINE}\n{LINKED_LINE}\n"),
            format!("{BUILD_LINE}\n{FILES_LINE}\n{LINKED_LINE}\n{LINKED_LINE}\n"),
        ] {
            if render(&template, "id", &files, "app").is_ok() {
                wrong.push(template);
            }
        }
        assert!(wrong.is_empty(), "шаблон принят: {wrong:#?}");
    }

    #[test]
    fn js_string_escapes_what_breaks_a_literal() {
        assert_eq!(js_string("fonts/a.ttf"), r#""fonts/a.ttf""#);
        assert_eq!(js_string(r#"a"b\c"#), r#""a\"b\\c""#);
        assert_eq!(js_string("a\nb\u{2028}"), "\"a\\u000ab\\u2028\"");
    }
}
