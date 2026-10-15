//! Программа как приложение (задача 8): манифест, значки и ссылки на них
//! в странице.
//!
//! Всё это браузер проверяет молча: манифест с ошибкой или значок не того
//! размера — и кнопки «Установить» просто нет, а на экране «Домой» — снимок
//! страницы вместо значка (Правило 6). Цвета манифеста сверяет
//! `tests/tokens.rs`.
//!
//! Тест машинный: читает файлы с диска. Под wasm32 (браузерные тесты,
//! клиппи окна) его нет — там нечего читать.
#![cfg(not(target_arch = "wasm32"))]

use std::fs;
use std::path::PathBuf;

use kvartik_core::i18n::{Key, Lang, t};
use serde_json::Value;

fn app_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn manifest() -> Value {
    let text = fs::read_to_string(app_dir().join("public/manifest.webmanifest"))
        .expect("манифест читается");
    serde_json::from_str(&text).expect("манифест — JSON")
}

/// Значок PNG: размер и палитра — то, что читается без распаковки.
struct Png {
    width: u32,
    height: u32,
    /// Цвета палитры с прозрачностью; пусто — значок не с палитрой.
    palette: Vec<[u8; 4]>,
}

impl Png {
    /// Значок из `public/` по пути из манифеста или страницы.
    fn open(path: &str) -> Self {
        let file = app_dir().join("public").join(path);
        let bytes = fs::read(&file).unwrap_or_else(|_| panic!("{path}: файла нет"));
        assert!(
            bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
            "{path}: не PNG"
        );
        let be = |at: usize| u32::from_be_bytes(bytes[at..at + 4].try_into().expect("4 байта"));
        let (mut width, mut height) = (0, 0);
        let (mut colors, mut alpha) = (Vec::new(), Vec::new());
        let mut at = 8;
        while at + 8 <= bytes.len() {
            let len = be(at) as usize;
            let kind = &bytes[at + 4..at + 8];
            let data = &bytes[at + 8..at + 8 + len];
            match kind {
                b"IHDR" => {
                    width = be(at + 8);
                    height = be(at + 12);
                }
                b"PLTE" => colors = data.chunks(3).map(|c| [c[0], c[1], c[2]]).collect(),
                b"tRNS" => alpha = data.to_vec(),
                _ => {}
            }
            at += 12 + len;
        }
        let palette = colors
            .iter()
            .enumerate()
            .map(|(i, [r, g, b])| [*r, *g, *b, alpha.get(i).copied().unwrap_or(255)])
            .collect();
        Self {
            width,
            height,
            palette,
        }
    }
}

/// Имя программы в манифесте — из таблицы строк, а не своё: на экране
/// «Домой» и в списке программ человек должен увидеть то же имя, что
/// в окне. Без перевода — армянское, как `lang` манифеста; переводы —
/// в `name_localized` (Chrome и Edge 148 на компьютере; остальные берут
/// `name` — по данным browser-compat-data, 2026-10-12).
#[test]
fn names_come_from_the_string_table() {
    let manifest = manifest();
    let mut wrong = Vec::new();
    for field in ["name", "short_name"] {
        let expected = t(Lang::Am, Key::AppName);
        if manifest[field].as_str() != Some(expected) {
            wrong.push(format!("{field}: {}, а не {expected}", manifest[field]));
        }
        for lang in Lang::ALL {
            let localized = &manifest[format!("{field}_localized")][lang.code()];
            let expected = t(lang, Key::AppName);
            if localized.as_str() != Some(expected) {
                wrong.push(format!("{field}_localized.{}: {localized}, а не {expected}", lang.code()));
            }
        }
    }
    assert_eq!(manifest["lang"], Lang::Am.code());
    assert!(wrong.is_empty(), "имена манифеста разошлись с таблицей строк: {wrong:#?}");
}

/// Что Chrome требует для установки (по исходникам Chromium,
/// `installable_evaluator.cc`; сверено 2026-10-12): имя, явный
/// `start_url`, `display` и значок «any» не меньше 144 точек — а web.dev
/// советует 192 и 512. `start_url`, `scope` и значки — относительные: они
/// считаются от адреса манифеста, а программа живёт в папке сайта `/app/`
/// (задача 65). А `id` — нет: его браузер разбирает от начала адреса
/// `start_url`, от домена (по спецификации Web App Manifest, «process the
/// `id` member», сверено 2026-10-16; Chrome 155 — так же), и `"./"` назвал
/// бы программу корнем сайта, где её нет. Поэтому `id` — папка программы,
/// как `public_url` в `Trunk.toml`, и навсегда: другой `id` — другая
/// программа, и установленную пришлось бы ставить заново. Каждый значок
/// есть и того размера, что написан: значок не того размера Chrome
/// отбрасывает молча.
#[test]
fn manifest_makes_the_app_installable() {
    let manifest = manifest();
    let trunk = fs::read_to_string(app_dir().join("Trunk.toml")).expect("Trunk.toml читается");
    let public_url = trunk
        .lines()
        .find_map(|line| line.strip_prefix("public_url = \""))
        .and_then(|rest| rest.strip_suffix('"'))
        .expect("в Trunk.toml есть public_url");
    for (field, expected) in [
        ("id", public_url),
        ("start_url", "./"),
        ("scope", "./"),
        ("display", "standalone"),
    ] {
        assert_eq!(manifest[field], expected, "{field}");
    }

    let icons = manifest["icons"].as_array().expect("icons — список");
    let mut any = Vec::new();
    let mut maskable = false;
    let mut wrong = Vec::new();
    for icon in icons {
        let src = icon["src"].as_str().expect("у значка есть src");
        let sizes = icon["sizes"].as_str().expect("у значка есть sizes");
        let png = Png::open(src);
        if format!("{}x{}", png.width, png.height) != sizes {
            wrong.push(format!("{src}: {}×{}, а написано {sizes}", png.width, png.height));
        }
        if icon["type"] != "image/png" {
            wrong.push(format!("{src}: type {}", icon["type"]));
        }
        match icon["purpose"].as_str() {
            Some("any") => any.push(png.width),
            Some("maskable") => maskable = true,
            other => wrong.push(format!("{src}: purpose {other:?}")),
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
    any.sort_unstable();
    assert_eq!(any, [192, 512], "значки «any» — 192 и 512");
    assert!(maskable, "нет значка maskable: Android обрежет «any» кругом");
}

/// Значки нарисованы цветами `tokens.css` (`tools/icons.py`): каждый цвет
/// палитры — смесь `--accent` и `--on-accent` светлой темы, а края
/// подложки — `--accent` с прозрачностью. Поменяли цвет в `tokens.css`
/// и не перерисовали значки — красный.
#[test]
fn icons_are_painted_with_tokens() {
    let tokens = fs::read_to_string(app_dir().join("style/tokens.css")).expect("tokens.css читается");
    let light = tokens.split("@media").next().expect("светлая тема");
    let color = |name: &str| -> [u8; 3] {
        let line = light
            .lines()
            .find(|line| line.trim_start().starts_with(&format!("{name}:")))
            .unwrap_or_else(|| panic!("{name} нет в tokens.css"));
        let hex = line.split('#').nth(1).expect("цвет #rrggbb");
        let channel = |at: usize| u8::from_str_radix(&hex[at..at + 2], 16).expect("шестнадцатеричное");
        [channel(0), channel(2), channel(4)]
    };
    let (back, front) = (color("--accent"), color("--on-accent"));

    let dir = app_dir().join("public/icons");
    let mut files: Vec<String> = fs::read_dir(&dir)
        .expect("папка значков читается")
        .map(|entry| entry.expect("запись читается").file_name().into_string().expect("имя UTF-8"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "значков нет");

    let mut wrong = Vec::new();
    for name in files {
        let png = Png::open(&format!("icons/{name}"));
        if png.palette.is_empty() {
            wrong.push(format!("{name}: не с палитрой — перерисовать tools/icons.py"));
            continue;
        }
        let opaque = |rgb: [u8; 3]| png.palette.contains(&[rgb[0], rgb[1], rgb[2], 255]);
        if !opaque(back) || !opaque(front) {
            wrong.push(format!("{name}: нет чистых --accent и --on-accent"));
        }
        for &[r, g, b, a] in &png.palette {
            if a == 0 {
                continue;
            }
            // Доля знака — по каналу, где цвета расходятся сильнее всего.
            let widest = (0..3)
                .max_by_key(|&i| back[i].abs_diff(front[i]))
                .expect("три канала");
            let share = (f64::from([r, g, b][widest]) - f64::from(back[widest]))
                / (f64::from(front[widest]) - f64::from(back[widest]));
            let mixed = (0..3).all(|i| {
                let expected =
                    f64::from(back[i]) + (f64::from(front[i]) - f64::from(back[i])) * share;
                (f64::from([r, g, b][i]) - expected).abs() <= 1.5
            });
            if !mixed || !(-0.01..=1.01).contains(&share) {
                wrong.push(format!("{name}: цвет #{r:02x}{g:02x}{b:02x} — не из tokens.css"));
            }
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// Страница ссылается на манифест и значки, и Trunk кладёт их в сайт.
/// Значок вкладки объявлен — без него браузер на каждом заходе просил бы
/// `/favicon.ico` и получал 404 (по исходникам Blink и Firefox, сверено
/// 2026-10-12). Значок iPhone — 180 точек: его iOS берёт вместо значков
/// манифеста (webkit.org/blog/12445).
#[test]
fn page_links_the_manifest_and_icons() {
    let page = fs::read_to_string(app_dir().join("index.html")).expect("index.html читается");
    for needed in [
        r#"<link data-trunk rel="copy-file" href="public/manifest.webmanifest" />"#,
        r#"<link data-trunk rel="copy-dir" href="public/icons" />"#,
        r#"<link rel="manifest" href="manifest.webmanifest" />"#,
    ] {
        assert!(page.contains(needed), "в index.html нет {needed}");
    }
    let mut linked = Vec::new();
    for tag in page.split('<').filter(|tag| tag.starts_with("link ")) {
        let attr = |name: &str| {
            tag.split(&format!(" {name}=\"")).nth(1).and_then(|rest| rest.split('"').next())
        };
        let (Some(rel), Some(href)) = (attr("rel"), attr("href")) else {
            continue;
        };
        if !matches!(rel, "icon" | "apple-touch-icon") {
            continue;
        }
        let png = Png::open(href);
        if let Some(sizes) = attr("sizes") {
            assert_eq!(format!("{}x{}", png.width, png.height), sizes, "{href}");
        }
        linked.push((rel, png.width));
    }
    assert!(linked.contains(&("icon", 32)), "нет значка вкладки 32×32: {linked:?}");
    assert!(
        linked.contains(&("apple-touch-icon", 180)),
        "нет значка iPhone 180×180: {linked:?}"
    );
}

/// Раскладку сайта, wasm окна и `sw.js` собирает хук сборки (`Trunk.toml`),
/// а не Trunk. Шаблон service worker лежит там, откуда его берёт хук,
/// и сам в сайт не копируется: в сайте должен оказаться только `sw.js`,
/// написанный `kvartik-sw`, — шаблон с `"dev"` выключил бы обновления
/// у всех. Файлы корня сайта Trunk не копирует тоже: он положил бы их
/// в папку программы, а не в корень (задача 65). А wasm Trunk не собирает
/// (`data-trunk-skip`): без этого он собрал бы свой, неразрезанный, со
/// своим запуском (задача 58).
#[test]
fn wasm_and_service_worker_go_through_the_hook() {
    let trunk = fs::read_to_string(app_dir().join("Trunk.toml")).expect("Trunk.toml читается");
    assert!(
        trunk.contains(
            r#"command_arguments = ["run", "--quiet", "--package", "kvartik-sw", "--", "--root", "public/root", "--wasm", "kvartik-app", "public/sw.js"]"#
        ),
        "хук kvartik-sw в Trunk.toml не найден"
    );
    assert!(app_dir().join("public/sw.js").is_file());
    let page = fs::read_to_string(app_dir().join("index.html")).expect("index.html читается");
    assert!(
        !page.contains("public/sw.js") && !page.contains(r#"href="public""#),
        "шаблон sw.js копируется в сайт как есть"
    );
    assert!(
        !page.contains(r#"href="public/root"#),
        "файлы корня сайта копирует Trunk — в папку программы"
    );
    let rust: Vec<_> = page
        .split('<')
        .filter(|tag| tag.starts_with("link ") && tag.contains(r#"rel="rust""#))
        .collect();
    assert!(
        rust.len() == 1 && rust[0].contains("data-trunk-skip"),
        "wasm окна собирает Trunk, а не хук: {rust:?}"
    );
}

/// Программа живёт в папке сайта, а корень — лендингу (задача 65). Папку
/// называет одна строка — `public_url` в `Trunk.toml`, а файлы корня
/// (`public/root/`) знают её сами, и каждый — молча: корень, который ведёт
/// не туда, `404.html`, который ведёт в пустоту, правило `_headers`, которое
/// не подходит ни к одному файлу программы, — ни ошибки сборки, ни ошибки
/// в браузере. Поэтому тест сверяет их с `public_url`.
#[test]
fn the_site_root_knows_the_app_folder() {
    let trunk = fs::read_to_string(app_dir().join("Trunk.toml")).expect("Trunk.toml читается");
    let setting = |name: &str| {
        trunk
            .lines()
            .find_map(|line| line.strip_prefix(&format!("{name} = \"")))
            .and_then(|rest| rest.strip_suffix('"'))
            .unwrap_or_else(|| panic!("в Trunk.toml нет {name}"))
            .to_owned()
    };
    let app = setting("public_url");
    assert!(
        app.len() > 2 && app.starts_with('/') && app.ends_with('/'),
        "public_url «{app}» — не папка сайта"
    );
    // `trunk serve` отдаёт сайт от корня, как хостинг: иначе программа
    // оказалась бы в /app/app/.
    assert_eq!(setting("serve_base"), "/");

    let root = |name: &str| {
        fs::read_to_string(app_dir().join("public/root").join(name))
            .unwrap_or_else(|_| panic!("в public/root нет {name}"))
    };
    let mut wrong = Vec::new();

    // Корень — в программу, временной переадресацией: постоянную браузер
    // запомнил бы навсегда, и лендинга не увидел бы никто.
    let rules: Vec<_> = root("_redirects")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_owned)
        .collect();
    if rules != [format!("/ {app} 302")] {
        wrong.push(format!("_redirects: {rules:?}, а ждали «/ {app} 302»"));
    }

    // Неизвестный адрес — в программу.
    let missing = root("404.html");
    for needed in [format!("content=\"0; url={app}\""), format!("<a href=\"{app}\">")] {
        if !missing.contains(&needed) {
            wrong.push(format!("404.html: нет {needed}"));
        }
    }

    // Долгий кэш — только файлам программы с хэшем в имени: правило без её
    // папки к ним не подойдёт.
    let headers = root("_headers");
    let mut path = "";
    for line in headers.lines() {
        if line.starts_with('/') {
            path = line.trim();
        } else if line.contains("immutable") && !path.starts_with(app.as_str()) {
            wrong.push(format!("_headers: immutable у «{path}» — не в {app}"));
        }
    }

    // `sw.js` корня — прежний service worker корня снимает себя, по
    // согласию, как любая новая версия, и запросов не перехватывает.
    let sw = root("sw.js");
    for needed in ["\"skip-waiting\"", "self.skipWaiting()", "self.registration.unregister()"] {
        if !sw.contains(needed) {
            wrong.push(format!("sw.js корня: нет {needed}"));
        }
    }
    if sw.contains("\"fetch\"") || sw.contains("\"install\"") {
        wrong.push("sw.js корня перехватывает запросы или включается без согласия".into());
    }

    assert!(wrong.is_empty(), "{wrong:#?}");
}
