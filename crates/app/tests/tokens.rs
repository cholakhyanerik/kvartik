//! Оформление (Правило 4): цвета живут только в `style/tokens.css`, каждая
//! пара «текст на фоне» держит порог WCAG 2.1 в обеих темах, коэффициенты
//! в комментариях правдивы, а каждая переменная, которой пользуются стили
//! и окно, объявлена.
//!
//! Соглашение об именах, по которому тест знает, что с чем сравнивать, —
//! в шапке `tokens.css`.
//!
//! Тест машинный: читает файлы с диска. Под wasm32 (браузерные тесты,
//! клиппи окна) его нет — там нечего читать.
#![cfg(not(target_arch = "wasm32"))]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Объявление переменной: значение и комментарий в той же строке.
struct Token {
    value: String,
    comment: String,
}

type Theme = BTreeMap<String, Token>;

fn app_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Темы `tokens.css`: светлая — `:root` верхнего уровня, тёмная — `:root`
/// внутри `@media (prefers-color-scheme: dark)`. Разбор построчный: файл
/// пишется по образцу, и шаг в сторону тест замечает, а не угадывает.
fn read_themes() -> (Theme, Theme) {
    let path = app_dir().join("style/tokens.css");
    let text = fs::read_to_string(&path).expect("tokens.css читается");
    let (mut light, mut dark) = (Theme::new(), Theme::new());
    let mut in_comment = false;
    let mut depth = 0_usize;
    let mut dark_depth = None;
    for raw in text.lines() {
        let line = raw.trim();
        if in_comment {
            in_comment = !line.contains("*/");
            continue;
        }
        if line.starts_with("/*") {
            in_comment = !line.contains("*/");
            continue;
        }
        if line.starts_with("@media") {
            assert_eq!(
                line, "@media (prefers-color-scheme: dark) {",
                "в tokens.css одна @media — тёмная тема"
            );
            dark_depth = Some(depth);
        }
        if let Some(rest) = line.strip_prefix("--") {
            let (name, rest) = rest
                .split_once(':')
                .unwrap_or_else(|| panic!("«{line}»: ждали «--имя: значение;»"));
            let (value, after) = rest
                .split_once(';')
                .unwrap_or_else(|| panic!("«{line}»: объявление кончается «;»"));
            let after = after.trim();
            let comment = match after.strip_prefix("/*") {
                Some(open) => open
                    .strip_suffix("*/")
                    .unwrap_or_else(|| panic!("«{line}»: комментарий — в одну строку"))
                    .trim(),
                None if after.is_empty() => "",
                None => panic!("«{line}»: после «;» — только комментарий"),
            };
            let theme = if dark_depth.is_some() {
                &mut dark
            } else {
                &mut light
            };
            let token = Token {
                value: value.trim().to_owned(),
                comment: comment.to_owned(),
            };
            assert!(
                theme.insert(format!("--{name}"), token).is_none(),
                "--{name} объявлена дважды в одной теме"
            );
        }
        depth += line.matches('{').count();
        depth -= line.matches('}').count();
        if dark_depth == Some(depth) {
            dark_depth = None;
        }
    }
    (light, dark)
}

/// Цвет `#rrggbb`. `None` — значение не цвет (длина, шрифт, число).
fn hex(value: &str) -> Option<Result<[u8; 3], String>> {
    let digits = value.strip_prefix('#')?;
    let channel = |at: usize| {
        digits
            .get(at..at + 2)
            .and_then(|pair| u8::from_str_radix(pair, 16).ok())
    };
    Some(match (digits.len(), channel(0), channel(2), channel(4)) {
        (6, Some(r), Some(g), Some(b)) => Ok([r, g, b]),
        _ => Err(format!(
            "«{value}»: цвет — только #rrggbb, без прозрачности"
        )),
    })
}

/// Цветовая функция в тексте: `rgb(`, `oklch(` и т. п. — если перед именем
/// не буква, цифра, `_` или `-` (иначе это метод `text_color(` в Rust или
/// переменная `--accent-color`).
fn color_function(text: &str) -> Option<&'static str> {
    const FUNCTIONS: [&str; 11] = [
        "rgb(",
        "rgba(",
        "hsl(",
        "hsla(",
        "hwb(",
        "lab(",
        "lch(",
        "oklab(",
        "oklch(",
        "color(",
        "color-mix(",
    ];
    let lower = text.to_ascii_lowercase();
    FUNCTIONS.into_iter().find(|function| {
        lower.match_indices(function).any(|(at, _)| {
            let before = lower[..at].chars().next_back().unwrap_or(' ');
            !(before.is_ascii_alphanumeric() || before == '_' || before == '-')
        })
    })
}

/// Цвет словом. `transparent` и `currentColor` — не цвета палитры, им можно.
const COLOR_NAMES: [&str; 7] = ["white", "black", "red", "green", "blue", "gray", "grey"];

/// Цвет, записанный не через `#`: функцией или словом. Такой цвет тест
/// не распознал бы и не проверил бы его контраст.
fn looks_like_other_color(value: &str) -> bool {
    color_function(value).is_some() || COLOR_NAMES.contains(&value.to_ascii_lowercase().as_str())
}

/// Относительная яркость по WCAG 2.1. `f64` здесь законен: это не деньги.
fn luminance([r, g, b]: [u8; 3]) -> f64 {
    let linear = |c: u8| {
        let c = f64::from(c) / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
}

fn contrast(a: [u8; 3], b: [u8; 3]) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// С чем цвет обязан контрастировать — по имени (шапка `tokens.css`).
enum Need {
    /// Фон: с ним сравнивают остальных.
    Surface,
    /// Заливка плашки или карточки: с ней сравнивают только её текст
    /// `--on-…`, а сама она слов не несёт.
    Fill,
    /// Только украшает, порога нет.
    Decorative,
    /// Порог на каждом фоне `--surface…`.
    OnSurfaces(f64),
    /// Порог на одной заливке.
    On(String, f64),
}

fn need(name: &str) -> Option<Need> {
    if name.starts_with("--surface") {
        Some(Need::Surface)
    } else if let Some(fill) = name.strip_prefix("--on-") {
        Some(Need::On(format!("--{fill}"), 4.5))
    } else if name.starts_with("--text") || name.ends_with("-text") {
        Some(Need::OnSurfaces(4.5))
    } else if matches!(name, "--border-strong" | "--focus" | "--accent") {
        Some(Need::OnSurfaces(3.0))
    } else if name.starts_with("--fill-") {
        Some(Need::Fill)
    } else if name == "--border" || name.starts_with("--decor-") {
        Some(Need::Decorative)
    } else {
        None
    }
}

/// Заливка второго текста `--on-X-…`: самое длинное начало имени, которое
/// объявлено, — `--on-fill-paper-muted` лежит на `--fill-paper`. Не нашлось
/// — имя как есть, и тест скажет, что такого фона нет.
fn on_fill(fill: &str, tokens: &Theme) -> String {
    let mut name = fill;
    while !tokens.contains_key(name) {
        match name.rsplit_once('-') {
            Some((shorter, _)) if shorter.len() > 2 => name = shorter,
            _ => return fill.to_owned(),
        }
    }
    name.to_owned()
}

/// Коэффициент, как его пишет комментарий: округлённый вниз до десятых,
/// чтобы 4.49 не выглядело как 4.5.
fn shown(ratio: f64) -> String {
    format!("{:.1}", (ratio * 10.0).floor() / 10.0)
}

/// Записи «N на имя» из комментария: имя фона без `--` и число.
fn claimed(comment: &str) -> BTreeMap<String, String> {
    comment
        .split(';')
        .filter_map(|part| {
            let (number, background) = part.trim().split_once(" на ")?;
            Some((format!("--{}", background.trim()), number.trim().to_owned()))
        })
        .collect()
}

/// Каждый цвет — `#rrggbb` и назван по соглашению: иначе тест не знал бы,
/// с чем его сравнивать, и новый цвет прошёл бы без проверки.
#[test]
fn colors_are_hex_and_named_by_role() {
    let (light, dark) = read_themes();
    let mut wrong = Vec::new();
    for (theme, tokens) in [("светлая", &light), ("тёмная", &dark)] {
        for (name, token) in tokens {
            if looks_like_other_color(&token.value) {
                wrong.push(format!(
                    "{theme} {name}: «{}» — цвет только #rrggbb",
                    token.value
                ));
            }
            match hex(&token.value) {
                None => {}
                Some(Err(error)) => wrong.push(format!("{theme} {name}: {error}")),
                Some(Ok(_)) if need(name).is_none() => wrong.push(format!(
                    "{theme} {name}: роль по имени не ясна — --surface…, --text…, …-text, \
                     --fill-…, --on-…, --border, --border-strong, --focus, --accent \
                     или --decor-…"
                )),
                Some(Ok(_)) => {}
            }
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// Тёмная тема переопределяет каждый цвет светлой и ничего, кроме цветов:
/// забытый цвет остался бы светлым на тёмном фоне.
#[test]
fn dark_theme_redefines_every_color() {
    let (light, dark) = read_themes();
    let colors: Vec<&String> = light
        .iter()
        .filter(|(_, token)| hex(&token.value).is_some())
        .map(|(name, _)| name)
        .collect();
    let redefined: Vec<&String> = dark.keys().collect();
    assert_eq!(redefined, colors, "тёмная тема — ровно цвета светлой");
}

/// Каждая пара «текст на фоне» держит порог в обеих темах, а комментарий
/// у цвета называет те же коэффициенты, что посчитал тест.
#[test]
fn contrast_meets_wcag_and_comments_tell_the_truth() {
    let (light, dark) = read_themes();
    let mut wrong = Vec::new();
    for (theme, tokens) in [("светлая", &light), ("тёмная", &dark)] {
        let color = |name: &str| match tokens.get(name).map(|t| hex(&t.value)) {
            Some(Some(Ok(rgb))) => Some(rgb),
            _ => None,
        };
        let surfaces: Vec<&String> = tokens
            .keys()
            .filter(|name| matches!(need(name), Some(Need::Surface)))
            .collect();
        assert!(!surfaces.is_empty(), "{theme}: ни одного фона --surface…");
        for (name, token) in tokens {
            let Some(rgb) = color(name) else {
                continue;
            };
            let pairs: Vec<(String, f64)> = match need(name) {
                Some(Need::OnSurfaces(min)) => {
                    surfaces.iter().map(|s| ((*s).clone(), min)).collect()
                }
                Some(Need::On(fill, min)) => vec![(on_fill(&fill, tokens), min)],
                Some(Need::Surface | Need::Fill | Need::Decorative) | None => Vec::new(),
            };
            let mut claims = claimed(&token.comment);
            for (background, min) in pairs {
                let Some(back) = color(&background) else {
                    wrong.push(format!("{theme} {name}: фона {background} нет"));
                    continue;
                };
                let ratio = contrast(rgb, back);
                if ratio < min {
                    wrong.push(format!(
                        "{theme} {name} на {background}: {ratio:.2}, порог {min} — менять цвет, а не порог"
                    ));
                }
                match claims.remove(&background) {
                    Some(number) if number == shown(ratio) => {}
                    Some(number) => wrong.push(format!(
                        "{theme} {name}: в комментарии {number} на {background}, а на деле {}",
                        shown(ratio)
                    )),
                    None => wrong.push(format!(
                        "{theme} {name}: в комментарии нет «{} на {}»",
                        shown(ratio),
                        background.trim_start_matches("--")
                    )),
                }
            }
            for background in claims.keys() {
                wrong.push(format!("{theme} {name}: лишняя запись о {background}"));
            }
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// Все файлы, где может оказаться цвет: стили, страница, код окна,
/// service worker, манифест программы и стили и скрипт лендинга (задача 34,
/// `site/`).
fn sources() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(dir).expect("папка читается") {
            let path = entry.expect("запись папки читается").path();
            if path.is_dir() {
                walk(&path, out);
            } else if matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("css" | "rs" | "html" | "js" | "webmanifest")
            ) {
                out.push(path);
            }
        }
    }
    let mut out = vec![app_dir().join("index.html")];
    walk(&app_dir().join("style"), &mut out);
    walk(&app_dir().join("src"), &mut out);
    walk(&app_dir().join("public"), &mut out);
    walk(&app_dir().join("site"), &mut out);
    out
}

/// Манифест программы: цвета в нём — литералом, потому что JSON не читает
/// `var(--…)`. Каждый — копия переменной светлой темы: окно установленной
/// программы и заставка при запуске (задача 8). Цвет заставки — фон
/// страницы, чтобы между заставкой и первым кадром не мигало.
fn manifest() -> PathBuf {
    app_dir().join("public/manifest.webmanifest")
}

const MANIFEST_COLORS: [(&str, &str); 2] =
    [("theme_color", "--accent"), ("background_color", "--surface")];

/// Текст файла без комментариев: `/* … */` везде и `// …` в Rust. Цвет или
/// `var(--…)` в комментарии — пример, а не использование. `//` в Rust и JS
/// режется до конца строки, даже внутри строкового литерала: адреса
/// `https://` в коде окна и service worker не живут.
fn code_only(path: &Path) -> String {
    let text = fs::read_to_string(path).expect("файл читается");
    let mut out = String::with_capacity(text.len());
    let mut rest = text.as_str();
    while let Some(open) = rest.find("/*") {
        out.push_str(&rest[..open]);
        rest = rest[open..].split_once("*/").map_or("", |(_, after)| after);
    }
    out.push_str(rest);
    if matches!(path.extension().and_then(|e| e.to_str()), Some("rs" | "js")) {
        out = out
            .lines()
            .map(|line| line.split_once("//").map_or(line, |(code, _)| code))
            .collect::<Vec<_>>()
            .join("\n");
    }
    out
}

/// Цвет литералом — `#` и 3, 4, 6 или 8 шестнадцатеричных цифр, за которыми
/// не идёт буква или цифра (иначе это `#[derive]` или `#app`), или цветовая
/// функция.
fn color_literals(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let bytes = text.as_bytes();
    for (at, _) in text.match_indices('#') {
        let run = bytes[at + 1..]
            .iter()
            .take_while(|b| b.is_ascii_hexdigit())
            .count();
        let next = bytes.get(at + 1 + run).copied().unwrap_or(b' ');
        let word = next.is_ascii_alphanumeric() || next == b'_' || next == b'-';
        if matches!(run, 3 | 4 | 6 | 8) && !word {
            found.push(text[at..at + 1 + run].to_owned());
        }
    }
    for line in text.lines() {
        let named = line.rsplit_once(':').is_some_and(|(_, value)| {
            let value = value.trim().trim_end_matches(';').trim();
            COLOR_NAMES.contains(&value.to_ascii_lowercase().as_str())
        });
        if color_function(line).is_some() || named {
            found.push(line.trim().to_owned());
        }
    }
    found
}

/// Ни одного цвета литералом вне `tokens.css` — ни в стилях, ни в разметке,
/// ни в `style=` из Rust, ни в service worker (Правило 4). Исключение —
/// копии переменных в манифесте ([`MANIFEST_COLORS`]): их сверяет
/// [`manifest_colors_copy_tokens`].
#[test]
fn no_color_literal_outside_tokens() {
    let tokens = app_dir().join("style/tokens.css");
    let (light, _) = read_themes();
    let copies: Vec<&str> = MANIFEST_COLORS
        .iter()
        .map(|(_, token)| light[*token].value.as_str())
        .collect();
    let mut wrong = Vec::new();
    for path in sources() {
        if path == tokens {
            continue;
        }
        for literal in color_literals(&code_only(&path)) {
            if path == manifest() && copies.contains(&literal.as_str()) {
                continue;
            }
            wrong.push(format!("{}: {literal}", path.display()));
        }
    }
    assert!(
        wrong.is_empty(),
        "цвет литералом вне tokens.css: {wrong:#?}"
    );
}

/// Цвета манифеста — те же, что у переменных: поменяли цвет в `tokens.css`
/// и забыли манифест — окно установленной программы осталось бы прежнего
/// цвета, и никто бы не заметил.
#[test]
fn manifest_colors_copy_tokens() {
    let (light, _) = read_themes();
    let text = fs::read_to_string(manifest()).expect("манифест читается");
    let json: serde_json::Value = serde_json::from_str(&text).expect("манифест — JSON");
    let mut wrong = Vec::new();
    for (field, token) in MANIFEST_COLORS {
        let expected = &light[token].value;
        let got = json[field].as_str();
        if got != Some(expected.as_str()) {
            wrong.push(format!("{field}: {got:?}, а {token} — {expected}"));
        }
    }
    assert!(wrong.is_empty(), "цвета манифеста разошлись с tokens.css: {wrong:#?}");
}

/// Опечатка в имени переменной не даёт ошибки: свойство с `var(--surfce)`
/// браузер молча выбрасывает, и элемент остаётся без фона. Поэтому каждое
/// `var(--…)` в стилях и в коде окна должно быть объявлено.
#[test]
fn every_used_variable_is_declared() {
    let (light, _) = read_themes();
    let mut wrong = Vec::new();
    for path in sources() {
        let text = code_only(&path);
        for (at, _) in text.match_indices("var(--") {
            let name: String = text[at + 4..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
                .collect();
            if !light.contains_key(&name) {
                wrong.push(format!("{}: {name}", path.display()));
            }
        }
    }
    assert!(wrong.is_empty(), "переменной нет в tokens.css: {wrong:#?}");
}

/// Цвета PDF (`kvartik_core::doc::pdf::rgb`) — копии переменных бумаги
/// светлой темы: превью в окне рисует ими же, и в превью тот же цвет, что
/// в PDF (Правило 2). Поменяли цвет в `tokens.css` и забыли ядро — PDF
/// остался бы прежнего цвета, и никто бы не заметил.
#[test]
fn pdf_colors_copy_tokens() {
    use kvartik_core::doc::page::Ink;
    use kvartik_core::doc::pdf::rgb;
    let (light, _) = read_themes();
    let mut wrong = Vec::new();
    for (ink, token) in [
        (Ink::Text, "--on-fill-paper"),
        (Ink::Muted, "--on-fill-paper-muted"),
        (Ink::Accent, "--on-fill-paper-accent"),
        (Ink::Rule, "--decor-paper-rule"),
    ] {
        let [r, g, b] = rgb(ink);
        let pdf = format!("#{r:02x}{g:02x}{b:02x}");
        match light.get(token) {
            Some(declared) if declared.value == pdf => {}
            Some(declared) => wrong.push(format!("{ink:?}: в PDF {pdf}, а {token} — {}", declared.value)),
            None => wrong.push(format!("{ink:?}: {token} нет в tokens.css")),
        }
    }
    assert!(wrong.is_empty(), "цвета PDF разошлись с tokens.css: {wrong:#?}");
}
