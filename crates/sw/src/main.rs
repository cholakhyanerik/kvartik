//! Сборка сайта: `sw.js` со списком файлов для кэша и хэшем сборки
//! (задача 8).
//!
//! Зовёт её Trunk хуком `post_build` (`crates/app/Trunk.toml`): к этому
//! времени все файлы сайта уже лежат в папке сборки, а `index.html` дописан
//! (по исходникам trunk 0.22.0-rc.2, `src/pipelines/html.rs`). Папку она
//! берёт из переменной `TRUNK_STAGING_DIR`, а шаблон — путём из аргумента.
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

/// Строка шаблона, вместо которой встаёт хэш сборки.
const BUILD_LINE: &str = r#"const BUILD = "dev";"#;
/// Строка шаблона, вместо которой встаёт список файлов.
const FILES_LINE: &str = "const FILES = [];";
/// Строка шаблона, вместо которой встают файлы, на которые ссылается
/// страница: по ним service worker видит, что страница той же сборки.
const LINKED_LINE: &str = "const LINKED = [];";
/// Что программа пишет в папку сборки.
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
    let template_path = env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("ожидался путь к шаблону sw.js")?;
    let template = fs::read_to_string(&template_path)
        .map_err(|error| format!("{}: {error}", template_path.display()))?;
    let files = collect(&staging)?;
    let build = build_id(&template, &files);
    let sw = render(&template, &build, &files)?;
    let output = staging.join(OUTPUT);
    fs::write(&output, sw).map_err(|error| format!("{}: {error}", output.display()))?;
    Ok(format!(
        "kvartik-sw: сборка {build}, файлов в кэше {}",
        precached(&files).count() + 1
    ))
}

/// Файл сайта: путь от корня сайта через `/` и содержимое.
#[derive(Debug)]
struct File {
    path: String,
    bytes: Vec<u8>,
}

/// Все файлы сайта, по пути. Кроме самого `sw.js` — его пишет эта
/// программа, — скрытых файлов: Trunk держит в папках служебные
/// (`.trunk-lock`), — и настроек хостинга, имена которых начинаются с `_`
/// (`_headers`, `_redirects` у Cloudflare Pages, задача 15): хостинг их не
/// отдаёт, и установка, которая их ждёт, не удалась бы ни у кого никогда.
fn collect(root: &Path) -> Result<Vec<File>, String> {
    fn walk(dir: &Path, prefix: &str, out: &mut Vec<File>) -> Result<(), String> {
        let entries = fs::read_dir(dir).map_err(|error| format!("{}: {error}", dir.display()))?;
        for entry in entries {
            let entry = entry.map_err(|error| format!("{}: {error}", dir.display()))?;
            let name = entry.file_name();
            let name = name
                .to_str()
                .ok_or_else(|| format!("{}: имя файла не UTF-8", entry.path().display()))?;
            if name.starts_with(['.', '_']) {
                continue;
            }
            let path = format!("{prefix}{name}");
            let kind = entry
                .file_type()
                .map_err(|error| format!("{path}: {error}"))?;
            if kind.is_dir() {
                walk(&entry.path(), &format!("{path}/"), out)?;
            } else if path != OUTPUT {
                let bytes = fs::read(entry.path()).map_err(|error| format!("{path}: {error}"))?;
                out.push(File { path, bytes });
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    walk(root, "", &mut files)?;
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

/// Хэш сборки: FNV-1a, 64 бита, по шаблону `sw.js` и путям и содержимому
/// всех файлов. Шаблон — тоже: новый `sw.js` с теми же файлами иначе писал
/// бы в кэш, которым открывается работающая версия.
///
/// Свой, а не `DefaultHasher`: тот может смениться с версией Rust, и та же
/// сборка получила бы другой хэш — у всех пользователей разом «новая
/// версия», в которой ничего нового. Стойкость к подбору здесь не нужна:
/// хэш только отличает одну сборку от другой. Длина перед содержимым — чтобы
/// граница между путём и байтами не могла сдвинуться.
fn build_id(template: &str, files: &[File]) -> String {
    fn feed(hash: u64, bytes: &[u8]) -> u64 {
        bytes.iter().fold(hash, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
        })
    }
    let entries = std::iter::once((OUTPUT, template.as_bytes()))
        .chain(files.iter().map(|file| (file.path.as_str(), file.bytes.as_slice())));
    let hash = entries.fold(0xcbf2_9ce4_8422_2325, |hash, (path, bytes)| {
        let hash = feed(hash, path.as_bytes());
        let hash = feed(hash, &[0]);
        let hash = feed(hash, &(bytes.len() as u64).to_le_bytes());
        feed(hash, bytes)
    });
    format!("{hash:016x}")
}

/// Файлы, которые service worker кладёт в кэш по их путям: все, кроме
/// страницы.
fn precached(files: &[File]) -> impl Iterator<Item = &str> {
    files
        .iter()
        .map(|file| file.path.as_str())
        .filter(|path| *path != PAGE)
}

/// Файлы из списка, на которые ссылается страница: wasm, клей JS, CSS,
/// манифест, значки. Страница не своей сборки их не назовёт.
fn linked(files: &[File]) -> Result<Vec<&str>, String> {
    let page = files
        .iter()
        .find(|file| file.path == PAGE)
        .ok_or(format!("в сборке нет {PAGE}"))?;
    let page = String::from_utf8_lossy(&page.bytes);
    Ok(precached(files).filter(|path| page.contains(path)).collect())
}

/// `sw.js` из шаблона: каждая из трёх строк на месте ровно по разу, иначе
/// ошибка — молча оставленное `"dev"` выключило бы обновления у всех.
fn render(template: &str, build: &str, files: &[File]) -> Result<String, String> {
    fn list<'a>(name: &str, paths: impl Iterator<Item = &'a str>) -> String {
        let mut out = format!("const {name} = [\n");
        for path in paths {
            out.push_str(&format!("  {},\n", js_string(path)));
        }
        out.push_str("];\n");
        out
    }
    let linked = linked(files)?;
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
                out.push_str(&list("FILES", precached(files)));
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
    /// Служебные файлы Trunk, настройки хостинга и прошлый `sw.js` в список
    /// не попадают.
    #[test]
    fn collects_site_files_with_forward_slashes() {
        let temp = Temp::new("collect");
        temp.put("index.html", b"<html>");
        temp.put("kvartik-app-1a2b.js", b"js");
        temp.put("fonts/noto-sans-latin-400.ttf", b"ttf");
        temp.put("icons/icon-192.png", b"png");
        temp.put(".trunk-lock", b"");
        temp.put(".stage/old.js", b"old");
        temp.put("_headers", b"/*\n  X-Frame-Options: DENY");
        temp.put("_redirects", b"");
        temp.put("sw.js", b"old sw");

        let files = collect(&temp.0).expect("папка читается");
        let paths: Vec<&str> = files.iter().map(|file| file.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                "fonts/noto-sans-latin-400.ttf",
                "icons/icon-192.png",
                "index.html",
                "kvartik-app-1a2b.js",
            ]
        );
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
            file("index.html", br#"<link href=/kvartik-app-1a2b.js><link href="kvartik-app-1a2b_bg.wasm">"#),
            file("fonts/noto-sans-armenian-400.ttf", b"ttf"),
            file("kvartik-app-1a2b.js", b"js"),
            file("kvartik-app-1a2b_bg.wasm", b"wasm"),
        ];
        let sw = render(TEMPLATE, "0123456789abcdef", &files).expect("шаблон узнан");
        assert!(sw.contains("const BUILD = \"0123456789abcdef\";\n"));
        assert!(sw.contains(
            "const FILES = [\n  \"fonts/noto-sans-armenian-400.ttf\",\n  \"kvartik-app-1a2b.js\",\n  \"kvartik-app-1a2b_bg.wasm\",\n];\n"
        ));
        assert!(sw.contains(
            "const LINKED = [\n  \"kvartik-app-1a2b.js\",\n  \"kvartik-app-1a2b_bg.wasm\",\n];\n"
        ));
        assert!(!sw.contains(BUILD_LINE) && !sw.contains(FILES_LINE) && !sw.contains(LINKED_LINE));
        assert!(!sw.contains("\"index.html\""));
    }

    /// Сборка без страницы — ошибка: проверять сборку страницы было бы не
    /// по чему, а сайт без неё не откроется.
    #[test]
    fn a_build_without_the_page_is_an_error() {
        assert!(render(TEMPLATE, "id", &[file("a.js", b"")]).is_err());
    }

    /// Шаблон без строки или с ней дважды — ошибка сборки, а не `sw.js`,
    /// который никогда не обновится. Концы строк Windows (`\r\n`) — не
    /// повод не узнать строку.
    #[test]
    fn template_needs_each_line_exactly_once() {
        let files = [file("index.html", b""), file("a.js", b"")];
        let all = format!("{BUILD_LINE}\r\n{FILES_LINE}\r\n{LINKED_LINE}\r\n");
        assert!(render(&all, "id", &files).is_ok());

        let mut wrong = Vec::new();
        for template in [
            format!("{FILES_LINE}\n{LINKED_LINE}\n"),
            format!("{BUILD_LINE}\n{LINKED_LINE}\n"),
            format!("{BUILD_LINE}\n{FILES_LINE}\n"),
            format!("{BUILD_LINE}\n{BUILD_LINE}\n{FILES_LINE}\n{LINKED_LINE}\n"),
            format!("{BUILD_LINE}\n{FILES_LINE}\n{FILES_LINE}\n{LINKED_LINE}\n"),
            format!("{BUILD_LINE}\n{FILES_LINE}\n{LINKED_LINE}\n{LINKED_LINE}\n"),
        ] {
            if render(&template, "id", &files).is_ok() {
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
