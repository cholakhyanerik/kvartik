//! Рамка ИИ-помощника — свой сайт на своём происхождении `ai.kvartik.com`
//! (задача 111): своя папка рядом с папкой сайта (`dist-ai/`) и свой проект
//! Cloudflare Pages `kvartik-ai`. Не папка сайта программы: путь
//! `kvartik.com/ai/` был бы тем же происхождением, что у программы, и CSP
//! рамки пускал бы к поставщикам ИИ код, у которого под рукой все данные
//! человека (CLAUDE.md, Правило 13, «ИИ-помощник»).
//!
//! Что в папке: страница (`index.html`), клей и wasm рамки с хэшем в имени
//! (`wasm::page`), оформление окна одним файлом с хэшем — `tokens.css`,
//! `fonts.css`, `base.css` окна и `style/frame.css` рамки, — шрифты окна,
//! её заголовки (`public/_headers` крейта рамки), `404.html`, `robots.txt`
//! и тексты лицензий её кода и шрифтов (`licenses::write_part`): рамка
//! раздаёт копию чужого кода со своего домена, и тексты идут с ней.
//! Встроенных скриптов и стилей нет (CSP), а слова — из таблицы строк ядра.
//! Service worker у рамки нет: без сети помощник не работает и так.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use kvartik_core::ai::frame;
use kvartik_core::i18n::{Key, Lang, t};

use super::{digest, licenses, wasm};

/// Файлы оформления окна в папке `--style`: на них стоит и рамка.
const STYLE: [&str; 3] = ["tokens.css", "fonts.css", "base.css"];
/// Оформление самой рамки в папке её крейта.
const FRAME_STYLE: &str = "style/frame.css";
/// Заголовки рамки в папке её крейта.
const HEADERS: &str = "public/_headers";

/// Что собралось — для строки отчёта программы сборки.
pub struct Built {
    /// Клей рамки с хэшем в имени.
    pub module: String,
    /// Крейтов в её файле лицензий.
    pub crates: usize,
}

/// Собрать сайт рамки из пакета `package` (крейт в папке `crate_dir`) в
/// пустую папку `out`: оформление — из `style`, шрифты — из `fonts`.
pub fn write(out: &Path, package: &str, crate_dir: &Path, style: &Path, fonts: &Path) -> Result<Built, String> {
    let read = |path: &Path| fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()));
    // Папка рамки — только её: прежняя сборка в ней — стереть, иначе
    // выкладка увезла бы и прежние файлы.
    if out.exists() {
        fs::remove_dir_all(out).map_err(|error| format!("{}: {error}", out.display()))?;
    }
    fs::create_dir_all(out).map_err(|error| format!("{}: {error}", out.display()))?;
    let built = wasm::page(package, out, wasm::FRAME, true)?;
    let mut css = String::new();
    for name in STYLE {
        css.push_str(&read(&style.join(name))?);
        css.push('\n');
    }
    css.push_str(&read(&crate_dir.join(FRAME_STYLE))?);
    let css_name = format!("{}-{}.css", wasm::FRAME, digest([(FRAME_STYLE, css.as_bytes())]));
    let script = read(&out.join(&built.module))?;
    let lang = Lang::Am;
    let files = [
        ("index.html".to_owned(), page(lang, &css_name, &css, &built.module, &script)),
        ("404.html".to_owned(), missing(&css_name, &css)),
        ("robots.txt".to_owned(), "User-agent: *\nDisallow: /\n".to_owned()),
        ("_headers".to_owned(), read(&crate_dir.join(HEADERS))?),
        (css_name, css),
    ];
    for (name, text) in &files {
        let path = out.join(name);
        if path.exists() {
            return Err(format!("{}: файл рамки лёг бы на другой", path.display()));
        }
        fs::write(&path, text).map_err(|error| format!("{}: {error}", path.display()))?;
    }
    copy_fonts(fonts, &out.join("fonts"))?;
    let crates = licenses::write_part(out, &built.crates)?;
    Ok(Built {
        module: built.module,
        crates,
    })
}

/// Шрифты окна — в папку рамки: `fonts.css` зовёт их от папки CSS
/// (`fonts/…`), а CSS рамки лежит в её корне.
fn copy_fonts(from: &Path, to: &Path) -> Result<(), String> {
    fs::create_dir_all(to).map_err(|error| format!("{}: {error}", to.display()))?;
    let names: BTreeSet<_> = fs::read_dir(from)
        .and_then(|entries| entries.map(|entry| entry.map(|entry| entry.file_name())).collect())
        .map_err(|error| format!("{}: {error}", from.display()))?;
    for name in names {
        fs::copy(from.join(&name), to.join(&name)).map_err(|error| format!("{}: {error}", from.join(&name).display()))?;
    }
    Ok(())
}

/// Страница рамки: только оформление и модуль — встроенного скрипта нет
/// (CSP), а слова рисует wasm на языке из адреса (`?lang=`). Поисковикам её
/// не показывать: она — часть экрана программы.
fn page(lang: Lang, css_name: &str, css: &str, module: &str, script: &str) -> String {
    format!(
        "<!doctype html>\n<html lang=\"{code}\">\n<head>\n<meta charset=\"utf-8\">\n\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
<meta name=\"robots\" content=\"noindex\">\n\
<title>{title}</title>\n\
<link rel=\"stylesheet\" href=\"/{css_name}\" integrity=\"{css_sri}\">\n\
<script type=\"module\" src=\"/{module}\" integrity=\"{script_sri}\"></script>\n\
</head>\n<body></body>\n</html>\n",
        code = lang.code(),
        title = t(lang, Key::AiTitle),
        css_sri = wasm::sri(css.as_bytes()),
        script_sri = wasm::sri(script.as_bytes()),
    )
}

/// Неизвестный адрес рамки: без своего `404.html` Pages отдал бы страницу с
/// кодом 200 (CLAUDE.md, Правило 6). Слова — на всех трёх языках: языка
/// адрес не знает.
fn missing(css_name: &str, css: &str) -> String {
    let mut body = String::new();
    for lang in Lang::ALL {
        body.push_str(&format!(
            "<p lang=\"{}\">{}</p>\n",
            lang.code(),
            escape(t(lang, Key::AiFrameAlone))
        ));
    }
    let open = Lang::ALL
        .iter()
        .map(|&lang| format!("<span lang=\"{}\">{}</span>", lang.code(), escape(t(lang, Key::LandingOpen))))
        .collect::<String>();
    format!(
        "<!doctype html>\n<html lang=\"hy\">\n<head>\n<meta charset=\"utf-8\">\n\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
<meta name=\"robots\" content=\"noindex\">\n\
<title>{brand}</title>\n\
<link rel=\"stylesheet\" href=\"/{css_name}\" integrity=\"{sri}\">\n\
</head>\n<body>\n<main class=\"ai-page ai-alone\">\n{body}<p><a class=\"button\" href=\"https://{site}/app/#/ask\">{open}</a></p>\n</main>\n</body>\n</html>\n",
        brand = t(Lang::Am, Key::Brand),
        sri = wasm::sri(css.as_bytes()),
        site = frame::SITE,
    )
}

/// Текст для HTML: строки таблицы — не разметка.
fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}
