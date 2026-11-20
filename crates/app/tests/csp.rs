//! CSP сайта (`public/root/_headers`) пускает запросы окна только к своему
//! сайту, а рамку — только рамку ИИ-помощника на её происхождении
//! (задача 111): `ai.kvartik.com` и рамки предпросмотров — те, что ядро
//! ставит окну в пару (`kvartik_core::ai::frame::frame_origin`). Адрес
//! поставщика ИИ в CSP окна — снова дверь наружу для данных человека
//! (CLAUDE.md, Правило 13, «ИИ-помощник»); к поставщикам пускает только CSP
//! рамки (`crates/ai/tests/csp.rs`).
//!
//! Тест машинный: читает файл с диска. Под wasm32 его нет — там нечего
//! читать.
#![cfg(not(target_arch = "wasm32"))]

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use kvartik_core::ai::Provider;
use kvartik_core::ai::frame;

/// Строка CSP у правила `/*` — единственная в файле (шапка `_headers`).
fn policy() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("public/root/_headers");
    let text = fs::read_to_string(&path).expect("_headers читается");
    let lines: Vec<&str> = text
        .lines()
        .filter_map(|line| line.trim().strip_prefix("Content-Security-Policy:"))
        .collect();
    assert_eq!(lines.len(), 1, "CSP в _headers должен быть ровно один");
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
fn requests_go_only_to_the_own_site() {
    let policy = policy();
    // Без `connect-src` действует `default-src` — свой сайт; есть — тоже
    // только он.
    let connect = directive(&policy, "connect-src").unwrap_or_else(|| vec!["'self'".to_owned()]);
    assert_eq!(connect, vec!["'self'".to_owned()], "connect-src окна — только свой сайт");
    let mut failures = Vec::new();
    for provider in Provider::ALL {
        let host = provider.url().trim_start_matches("https://").split('/').next().unwrap_or_default();
        if policy.contains(host) {
            failures.push(format!("в CSP окна адрес поставщика {host}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn the_only_frame_is_the_assistant_of_its_pair() {
    let policy = policy();
    let frames: BTreeSet<String> = directive(&policy, "frame-src")
        .expect("в CSP окна есть frame-src")
        .into_iter()
        .collect();
    let want: BTreeSet<String> = ["https://ai.kvartik.com", "https://*.kvartik-ai.pages.dev"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    assert_eq!(frames, want, "frame-src окна");
    // Те же адреса, что пара ядра: своему домену — рамка своего домена,
    // предпросмотру — рамка той же ветки.
    assert_eq!(frame::frame_origin("kvartik.com").as_deref(), Some("https://ai.kvartik.com"));
    let preview = frame::frame_origin("ci-check.kvartik.pages.dev").unwrap_or_default();
    assert!(
        preview.starts_with("https://") && preview.ends_with(".kvartik-ai.pages.dev"),
        "рамка предпросмотра {preview} — вне frame-src"
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
    assert_eq!(directive(&policy, "frame-ancestors"), Some(vec!["'none'".to_owned()]));
}
