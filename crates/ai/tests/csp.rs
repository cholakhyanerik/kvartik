//! CSP рамки ИИ-помощника (`public/_headers`, задача 111) пускает запросы
//! только к своему сайту и к четырём адресам поставщиков ИИ — ровно тем,
//! куда ядро шлёт вопрос (`kvartik_core::ai::Provider::url`), с путём, не
//! весь сервер и без звёздочек; а встроить рамку может только окно её пары
//! (`kvartik_core::ai::frame::parent_origin`). Новый адрес поставщика без
//! правки CSP браузер отверг бы молча, а лишний — дверь наружу (CLAUDE.md,
//! Правило 13).
//!
//! Тест машинный: читает файл с диска. Под wasm32 его нет — там нечего
//! читать.
#![cfg(not(target_arch = "wasm32"))]

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use kvartik_core::ai::Provider;
use kvartik_core::ai::frame;

/// Строка CSP у правила `/*` — единственная в файле.
fn policy() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("public/_headers");
    let text = fs::read_to_string(&path).expect("_headers рамки читается");
    let lines: Vec<&str> = text
        .lines()
        .filter_map(|line| line.trim().strip_prefix("Content-Security-Policy:"))
        .collect();
    assert_eq!(lines.len(), 1, "CSP в _headers рамки должен быть ровно один");
    lines[0].trim().to_owned()
}

/// Источники директивы `name`; `None` — директивы нет.
fn directive(policy: &str, name: &str) -> Option<Vec<String>> {
    policy.split(';').map(str::trim).find_map(|part| {
        let mut words = part.split_whitespace();
        (words.next() == Some(name)).then(|| words.map(str::to_owned).collect())
    })
}

#[test]
fn connect_src_is_own_site_and_the_four_ai_addresses() {
    let policy = policy();
    let connect: BTreeSet<String> = directive(&policy, "connect-src")
        .expect("в CSP рамки есть connect-src")
        .into_iter()
        .collect();
    let mut want: BTreeSet<String> = Provider::ALL.into_iter().map(|provider| provider.url().to_owned()).collect();
    want.insert("'self'".to_owned());
    assert_eq!(connect, want, "connect-src рамки и адреса ai::Provider::url");
    let loose: Vec<&String> = connect
        .iter()
        .filter(|source| {
            source.contains('*')
                || (source.starts_with("https://")
                    && source
                        .trim_start_matches("https://")
                        .split_once('/')
                        .is_none_or(|(_, path)| path.is_empty() || path.ends_with('/')))
        })
        .collect();
    assert!(loose.is_empty(), "адрес без точного пути пускает весь сервер: {loose:?}");
}

#[test]
fn only_the_window_of_its_pair_frames_the_assistant() {
    let policy = policy();
    let ancestors: BTreeSet<String> = directive(&policy, "frame-ancestors")
        .expect("в CSP рамки есть frame-ancestors")
        .into_iter()
        .collect();
    let want: BTreeSet<String> = ["https://kvartik.com", "https://*.kvartik.pages.dev"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    assert_eq!(ancestors, want, "frame-ancestors рамки");
    assert_eq!(frame::parent_origin("ai.kvartik.com").as_deref(), Some("https://kvartik.com"));
    let preview = frame::parent_origin("ci-check.kvartik-ai.pages.dev").unwrap_or_default();
    assert!(
        preview.starts_with("https://") && preview.ends_with(".kvartik.pages.dev"),
        "окно предпросмотра {preview} — вне frame-ancestors"
    );
}

#[test]
fn everything_else_stays_own_site_only() {
    let policy = policy();
    assert_eq!(directive(&policy, "default-src"), Some(vec!["'self'".to_owned()]));
    assert_eq!(
        directive(&policy, "script-src"),
        Some(vec!["'self'".to_owned(), "'wasm-unsafe-eval'".to_owned()])
    );
    assert!(!policy.contains("unsafe-inline"), "{policy}");
    assert_eq!(directive(&policy, "form-action"), Some(vec!["'none'".to_owned()]));
    assert!(directive(&policy, "frame-src").is_none(), "рамке своих рамок не нужно");
}
