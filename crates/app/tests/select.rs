//! Выделение одним касанием (`user-select`): номер счёта, слова согласия,
//! ключ лицензии, адрес календаря и число для расчёта человек выделяет
//! целиком, чтобы скопировать сам, когда буфер отказал или его нет. Safari на
//! iPhone знает только `-webkit-user-select` (MDN browser-compat-data,
//! `css.properties.user-select`, сверено 2026-11-13: без префикса — Safari 27.3
//! на компьютере, на iPhone — нет), и правило без него на iPhone молча не
//! работает: у реквизитов и слов согласия страницы «Купить» и у адреса
//! календаря лендинга так было до 0.0.124.
//!
//! Тест машинный: читает файлы с диска. Под wasm32 его нет — там нечего
//! читать.
#![cfg(not(target_arch = "wasm32"))]

use std::fs;
use std::path::PathBuf;

/// Стили окна и лендинга.
fn stylesheets() -> Vec<PathBuf> {
    let app = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut out = Vec::new();
    for dir in [app.join("style"), app.join("site")] {
        for entry in fs::read_dir(&dir).expect("папка читается") {
            let path = entry.expect("запись папки читается").path();
            if path.extension().and_then(|e| e.to_str()) == Some("css") {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

/// Правила файла — тексты до `}`, без комментариев.
fn blocks(text: &str) -> Vec<String> {
    let mut plain = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("/*") {
        plain.push_str(&rest[..at]);
        rest = rest[at..].split_once("*/").map_or("", |(_, after)| after);
    }
    plain.push_str(rest);
    plain.split('}').map(str::to_owned).collect()
}

/// Значения свойства `name` в теле правила — после его `{`, чтобы двоеточие
/// селектора (`a:hover`) не сошло за свойство. Префиксное свойство — своё
/// имя: `user-select` не ловит `-webkit-user-select`.
fn values<'a>(block: &'a str, name: &str) -> Vec<&'a str> {
    let body = block.rsplit_once('{').map_or(block, |(_, body)| body);
    body.split(';')
        .filter_map(|declaration| {
            let (property, value) = declaration.split_once(':')?;
            (property.trim() == name).then(|| value.trim())
        })
        .collect()
}

/// Каждое `user-select` — в паре с `-webkit-user-select` того же значения
/// в том же правиле.
#[test]
fn every_user_select_has_its_webkit_twin() {
    let mut wrong = Vec::new();
    let mut seen = 0;
    for path in stylesheets() {
        let text = fs::read_to_string(&path).expect("стиль читается");
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_owned();
        for block in blocks(&text) {
            let plain = values(&block, "user-select");
            let webkit = values(&block, "-webkit-user-select");
            seen += plain.len();
            if plain != webkit {
                let rule = block.rsplit_once('{').map_or("", |(selector, _)| selector).trim().to_owned();
                wrong.push(format!("{name}: «{rule}» — user-select {plain:?}, -webkit-user-select {webkit:?}"));
            }
        }
    }
    assert!(seen > 0, "в стилях не нашлось ни одного user-select");
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// Разбор — на образцах: без них сломанный разбор прошёл бы молча.
#[test]
fn values_are_read_per_rule() {
    let rules = blocks(
        ".a {\n  user-select: all;\n  -webkit-user-select: all;\n}\n/* .c { user-select: none; } */\n\
         a:hover { color: red; user-select: all }\n@media (x) {\n  .d { -webkit-user-select: none; }\n}\n",
    );
    assert_eq!(values(&rules[0], "user-select"), ["all"]);
    assert_eq!(values(&rules[0], "-webkit-user-select"), ["all"]);
    assert_eq!(values(&rules[1], "user-select"), ["all"]);
    assert!(values(&rules[1], "-webkit-user-select").is_empty());
    assert!(values(&rules[2], "user-select").is_empty());
    assert_eq!(values(&rules[2], "-webkit-user-select"), ["none"]);
    assert!(rules.iter().all(|rule| !rule.contains(".c")), "комментарий — не правило");
}
