//! Выпуск ключей лицензии на машине автора.
//!
//! Закрытый ключ — вне репозитория и вне сборки, всегда («Общее»
//! в CLAUDE.md): утёкший ключ — бесплатные лицензии навсегда, а отозвать
//! его без сервера нельзя. Агент его не создаёт, не читает и не передаёт
//! в команды (Правило 9): программу запускает человек в своём терминале.
//!
//! Пока здесь только пара ключей (задача 25): открытый встраивается в ядро
//! (`kvartik_core::license::PUBLIC_KEY`), без него сборке нечем проверять
//! ключи. Выпуск ключа на ՀՎՀՀ, журнал выданных и путь закрытого ключа
//! в запретах hook'а — задача 26.
//!
//! ```text
//! kvartik-keygen pair <путь закрытого ключа>
//! ```

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use ed25519_dalek::SigningKey;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.as_slice() {
        [command, path] if command == "pair" => pair(Path::new(path)),
        _ => Err("использование: kvartik-keygen pair <путь закрытого ключа вне репозитория>".to_owned()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

/// Новая пара: закрытый ключ — файлом по `path`, открытый — на экран.
///
/// Файл не перезаписывается никогда: прежний закрытый ключ, затёртый
/// новым, — это все выданные ключи, которые больше нечем продлить. И не
/// ложится в рабочую копию: `.gitignore` закрывает только шаблоны `*.key`,
/// `*.pem` и `keys/`, а файл с другим именем `git add .` унёс бы в историю
/// (Правило 11).
fn pair(path: &Path) -> Result<(), String> {
    let folder = path
        .parent()
        .filter(|folder| !folder.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let folder = fs::canonicalize(folder)
        .map_err(|error| format!("папка {}: {error} (создайте её сами)", folder.display()))?;
    if inside(&folder, &workspace()?) {
        return Err(format!(
            "{} — внутри рабочей копии: закрытый ключ хранится вне репозитория",
            path.display()
        ));
    }

    let mut seed = [0_u8; ed25519_dalek::SECRET_KEY_LENGTH];
    getrandom::fill(&mut seed).map_err(|error| format!("случайность системы: {error}"))?;
    let key = SigningKey::from_bytes(&seed);
    let text = format!("{}\n", hex(&seed));
    seed.fill(0);

    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("{}: {error} (существующий ключ не перезаписывается)", path.display()))?;
    file.write_all(text.as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|error| format!("{}: {error}", path.display()))?;

    println!("Закрытый ключ записан: {}", path.display());
    println!("Сохраните его копию вне этой машины: без него ключей не продлить.");
    println!("Открытый ключ — для сборки:");
    println!("{}", hex(key.verifying_key().as_bytes()));
    Ok(())
}

/// Корень рабочей копии — на два уровня выше этого крейта.
fn workspace() -> Result<PathBuf, String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
    fs::canonicalize(&root).map_err(|error| format!("рабочая копия {}: {error}", root.display()))
}

/// Лежит ли `path` внутри `root`. Без учёта регистра: на Windows
/// `C:\Dev` и `c:\dev` — одна папка.
fn inside(path: &Path, root: &Path) -> bool {
    let lower = |path: &Path| path.to_string_lossy().to_lowercase();
    let (path, root) = (lower(path), lower(root));
    path == root || path.strip_prefix(&root).is_some_and(|rest| rest.starts_with(['\\', '/']))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Папка рабочей копии и вложенные в неё — внутри, соседняя с похожим
    /// именем — нет.
    #[test]
    fn inside_knows_the_working_copy() {
        let root = Path::new(r"C:\dev\kvartik");
        assert!(inside(Path::new(r"C:\dev\kvartik"), root));
        assert!(inside(Path::new(r"c:\DEV\kvartik\crates"), root));
        assert!(!inside(Path::new(r"C:\dev\kvartik-keys"), root));
        assert!(!inside(Path::new(r"C:\Users\author\kvartik"), root));
    }
}
