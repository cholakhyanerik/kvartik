//! CLAUDE.md и `.claude/rules/` (с 0.0.179): правила, которые Claude Code
//! грузит в каждую сессию, не разрастаются снова.
//!
//! CLAUDE.md Claude Code грузит целиком в каждую сессию, а к 0.0.178 в нём
//! было 3 402 строки. Подробности легли в `.claude/rules/**/*.md`, и каждый
//! такой файл грузится, только когда Claude читает, пишет или правит файл по
//! его `paths:` (code.claude.com/docs/en/memory, «Path-specific rules»,
//! сверено 2026-11-23 на Claude Code 2.1.330). Молча ломается всё это так:
//! файл без `paths:` или с BOM перед `---` (разбор шапки не узнаёт)
//! грузится в каждую сессию, `@путь` в CLAUDE.md разворачивается при запуске,
//! файл, которого нет в таблице CLAUDE.md, никто не найдёт, а путь, который
//! переименовали, больше не совпадёт ни с одним файлом.
//!
//! Тест машинный: читает файлы с диска. Под wasm32 его нет — там нечего
//! читать.
#![cfg(not(target_arch = "wasm32"))]

use std::fs;
use std::path::{Path, PathBuf};

/// Предел CLAUDE.md в строках: новое правило — коротко, с причиной,
/// а подробности — в файл своей области `.claude/rules/`.
const MAX_LINES: usize = 400;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn claude_md() -> String {
    fs::read_to_string(root().join("CLAUDE.md")).expect("CLAUDE.md читается")
}

/// Файлы `.claude/rules/**/*.md` — путями от `.claude/rules/`, через `/`.
fn rule_files() -> Vec<String> {
    fn walk(dir: &Path, base: &Path, out: &mut Vec<String>) {
        for entry in fs::read_dir(dir).expect("папка правил читается") {
            let path = entry.expect("запись папки читается").path();
            if path.is_dir() {
                walk(&path, base, out);
            } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
                let rel = path.strip_prefix(base).expect("путь внутри папки правил");
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    let base = root().join(".claude/rules");
    let mut out = Vec::new();
    walk(&base, &base, &mut out);
    out.sort();
    out
}

/// Пути из шапки файла правил: `---`, `paths:`, строки `  - "…"`, `---`.
/// `None` — шапки нет или она не такая: Claude Code загрузил бы файл
/// в каждую сессию.
fn paths_of(text: &str) -> Option<Vec<String>> {
    let rest = text.strip_prefix("---\npaths:\n")?;
    let (head, _) = rest.split_once("\n---\n")?;
    let globs: Option<Vec<String>> = head
        .lines()
        .map(|line| {
            let glob = line.strip_prefix("  - \"")?.strip_suffix('"')?;
            (!glob.is_empty()).then(|| glob.to_owned())
        })
        .collect();
    globs.filter(|g| !g.is_empty())
}

/// Строка без кусков `кода`: в коде `@` — не импорт.
fn without_code(line: &str) -> String {
    line.split('`').step_by(2).collect::<Vec<_>>().join(" ")
}

#[test]
fn claude_md_stays_short() {
    let lines = claude_md().lines().count();
    assert!(
        lines <= MAX_LINES,
        "CLAUDE.md — {lines} строк, предел {MAX_LINES}: подробности — в .claude/rules/<область>.md, \
         а в CLAUDE.md — правило и причина в несколько строк"
    );
}

#[test]
fn claude_md_imports_nothing() {
    // Импорт `@путь` Claude Code разворачивает при запуске: он грузился бы
    // в каждую сессию вместе с CLAUDE.md.
    let mut fence = false;
    let imports: Vec<String> = claude_md()
        .lines()
        .filter(|line| {
            if line.starts_with("```") {
                fence = !fence;
            }
            !fence
        })
        .filter(|line| {
            let plain = without_code(line);
            plain.split_whitespace().any(|word| word.starts_with('@') && word.len() > 1)
        })
        .map(str::to_owned)
        .collect();
    assert!(imports.is_empty(), "импорт в CLAUDE.md: {imports:#?}");
}

#[test]
fn every_rule_file_loads_only_by_its_paths() {
    let files = rule_files();
    assert!(files.len() > 1, "в .claude/rules/ не нашлось файлов правил");
    let mut wrong = Vec::new();
    for file in &files {
        let bytes = fs::read(root().join(".claude/rules").join(file)).expect("файл правил читается");
        if bytes.starts_with(b"\xEF\xBB\xBF") {
            wrong.push(format!("{file}: BOM перед шапкой"));
            continue;
        }
        if bytes.contains(&b'\r') {
            wrong.push(format!("{file}: концы строк CRLF"));
        }
        let text = String::from_utf8(bytes).expect("файл правил — UTF-8");
        if paths_of(&text).is_none() {
            wrong.push(format!("{file}: нет шапки `---` / `paths:` / `  - \"…\"` / `---`"));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

#[test]
fn every_rule_file_is_in_the_claude_md_table() {
    let claude = claude_md();
    let listed: Vec<&str> = claude
        .lines()
        .filter_map(|line| line.strip_prefix("| `")?.split_once("` |").map(|(file, _)| file))
        .filter(|file| file.ends_with(".md"))
        .collect();
    let files = rule_files();
    let missing: Vec<&String> = files.iter().filter(|f| !listed.contains(&f.as_str())).collect();
    let stale: Vec<&&str> = listed.iter().filter(|f| !files.iter().any(|g| g == **f)).collect();
    assert!(missing.is_empty(), "файлы .claude/rules/, которых нет в таблице CLAUDE.md: {missing:#?}");
    assert!(stale.is_empty(), "в таблице CLAUDE.md файлы, которых нет в .claude/rules/: {stale:#?}");
}

#[test]
fn rule_paths_still_name_real_files() {
    // Начало пути до первой звёздочки или скобки — файл или папка проекта:
    // переименованная папка иначе молча выключила бы правило.
    let mut wrong = Vec::new();
    for file in rule_files() {
        let text = fs::read_to_string(root().join(".claude/rules").join(&file)).expect("файл правил читается");
        for glob in paths_of(&text).unwrap_or_default() {
            let literal = glob.split(['*', '{', '[']).next().unwrap_or_default();
            if !literal.is_empty() && !root().join(literal).exists() {
                wrong.push(format!("{file}: «{glob}» — нет `{literal}`"));
            }
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// Разбор шапки — на образцах: сломанный разбор пропустил бы любой файл.
#[test]
fn paths_header_is_read_strictly() {
    let good = "---\npaths:\n  - \"crates/**\"\n  - \"Cargo.toml\"\n---\n\n# Текст\n";
    assert_eq!(paths_of(good), Some(vec!["crates/**".to_owned(), "Cargo.toml".to_owned()]));
    assert_eq!(paths_of("# Текст без шапки\n"), None);
    assert_eq!(paths_of("---\npaths:\n---\n"), None, "пустой список");
    assert_eq!(paths_of("---\ndescription: x\n---\n"), None, "без paths");
    assert_eq!(paths_of("---\npaths:\n  - crates/**\n---\n"), None, "без кавычек");
    assert_eq!(without_code("до `@путь` после"), "до   после");
}
