//! CSP сайта (`public/root/_headers`) пускает запросы окна только к своему
//! сайту и к четырём адресам ИИ-помощника (задача 102): ровно тем, куда
//! ядро шлёт вопрос (`kvartik_core::ai::Provider::url`), — с путём, не весь
//! сервер и без звёздочек. Новый адрес поставщика без правки CSP браузер
//! отверг бы молча, а лишний адрес в CSP — дверь наружу мимо ИИ-помощника
//! (CLAUDE.md, Правило 13).
//!
//! Тест машинный: читает файл с диска. Под wasm32 его нет — там нечего
//! читать.
#![cfg(not(target_arch = "wasm32"))]

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use kvartik_core::ai::Provider;

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
fn connect_src_is_own_site_and_the_four_ai_addresses() {
    let policy = policy();
    let connect: BTreeSet<String> = directive(&policy, "connect-src")
        .expect("в CSP есть connect-src")
        .into_iter()
        .collect();
    let mut want: BTreeSet<String> = Provider::ALL.into_iter().map(|provider| provider.url().to_owned()).collect();
    want.insert("'self'".to_owned());
    assert_eq!(connect, want, "connect-src в _headers и адреса ai::Provider::url");
    let loose: Vec<&String> = connect
        .iter()
        .filter(|source| {
            source.contains('*') || (source.starts_with("https://") && source.trim_start_matches("https://").split_once('/').is_none_or(|(_, path)| path.is_empty() || path.ends_with('/')))
        })
        .collect();
    assert!(loose.is_empty(), "адрес без точного пути пускает весь сервер: {loose:?}");
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
