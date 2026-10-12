//! Образцы копии выпущенных версий открываются этой версией (Правило 2):
//! шапка и замок каждого образца читаются в ожидаемые значения, и ни один
//! файл папки `backups/` не забыт в списке. Расшифровку и восстановление
//! проверяет браузерный тест окна по тому же списку (`backups/samples.rs`).

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use kvartik_core::format::backup::{self, BackupError};
use kvartik_core::model::record::Timestamp;

include!("backups/samples.rs");

/// Шапка каждого образца — та, что выписана в списке.
#[test]
fn every_sample_opens() {
    let mut wrong = Vec::new();
    for sample in SAMPLES {
        match backup::open(sample.text) {
            Ok(sealed) => {
                let header = sealed.header();
                let got = (
                    header.format_version(),
                    header.app_version(),
                    header.created(),
                );
                let expected = (
                    sample.format_version,
                    sample.app_version,
                    Timestamp::from_unix_millis(sample.created),
                );
                if got != expected {
                    wrong.push(format!("{}: {got:?}, а не {expected:?}", sample.path));
                }
            }
            Err(error) => wrong.push(format!("{}: {error:?}", sample.path)),
        }
    }
    assert!(wrong.is_empty(), "образцы не открываются: {wrong:#?}");
}

/// Образец, обрезанный при скачивании, — испорченный, а не чужой файл.
#[test]
fn truncated_samples_are_damaged() {
    let mut wrong = Vec::new();
    for sample in SAMPLES {
        let half = &sample.text[..sample.text.len() / 2];
        if !matches!(backup::open(half), Err(BackupError::Damaged(_))) {
            wrong.push(sample.path);
        }
    }
    assert!(wrong.is_empty(), "обрезанный образец не узнан: {wrong:?}");
}

/// Каждый файл `backups/vN/` есть в списке, и наоборот: забытый образец не
/// проверялся бы вовсе, а образец под чужой версией — не той.
#[test]
fn every_sample_file_is_listed() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/backups");
    let mut on_disk = BTreeSet::new();
    for version in fs::read_dir(&root).expect("папка образцов читается") {
        let version = version.expect("запись папки");
        if !version.file_type().expect("тип записи").is_dir() {
            continue;
        }
        let name = version
            .file_name()
            .into_string()
            .expect("имя папки — UTF-8");
        for file in fs::read_dir(version.path()).expect("папка версии читается")
        {
            let file = file.expect("запись папки").file_name();
            on_disk.insert(format!("{name}/{}", file.to_string_lossy()));
        }
    }
    let listed: BTreeSet<String> = SAMPLES.iter().map(|s| s.path.to_owned()).collect();
    assert_eq!(on_disk, listed, "файлы образцов и список разошлись");
    let mut wrong = Vec::new();
    for sample in SAMPLES {
        if !sample
            .path
            .starts_with(&format!("v{}/", sample.format_version))
        {
            wrong.push(sample.path);
        }
    }
    assert!(
        wrong.is_empty(),
        "образец не в папке своей версии: {wrong:?}"
    );
}
