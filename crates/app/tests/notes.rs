//! «Что нового» (задача 67): заметки о версиях в `public/notes.json` — их
//! показывает окно новой версии перед обновлением.
//!
//! Ошибиться в файле легко, и молча: забытая заметка — окно без списка,
//! опечатка в версии или дне — заметка, которую прежняя программа пропустит
//! (`kvartik_core::release`), пункт на одном языке из трёх — пустое место
//! в окне, а длинный текст — окно, которое на телефоне не помещается. Тест
//! ловит это до выкладки. Знаки заметок в шрифтах сверяет `tests/fonts.rs`.
//!
//! Тест машинный: читает файлы с диска. Под wasm32 (браузерные тесты,
//! клиппи окна) его нет — там нечего читать.
#![cfg(not(target_arch = "wasm32"))]

use std::fs;
use std::path::PathBuf;

use kvartik_core::i18n::{self, Lang};
use kvartik_core::release::{MAX_BYTES, MAX_NOTES, Notes, Version};

/// Пунктов у версии — не больше: окно новой версии на телефоне — несколько
/// строк о главном, а не история изменений (она — в `CHANGELOG.md`).
const MAX_ITEMS: usize = 4;

/// Знаков в пункте — не больше: длинный пункт на экране 360 точек — уже
/// страница текста, а кнопка «Обновить» уезжает под список.
const MAX_CHARS: usize = 360;

fn text() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("public/notes.json");
    fs::read_to_string(&path).expect("public/notes.json читается")
}

fn notes() -> Notes {
    Notes::parse(&text()).expect("public/notes.json разбирается")
}

/// Каждая заметка читается: пропущенную прежняя программа молча не
/// покажет. И файл не больше предела разбора.
#[test]
fn every_note_reads() {
    let text = text();
    assert!(text.len() <= MAX_BYTES, "файл {} Б, предел разбора — {MAX_BYTES}", text.len());
    let notes = notes();
    assert_eq!(notes.skipped(), 0, "заметки, которые разбор пропускает: версия — три числа через точку, день — ГГГГ-ММ-ДД");
    assert!(!notes.all().is_empty(), "в файле нет ни одной заметки");
}

/// Самая свежая заметка — об этой версии: задача, поднявшая версию в
/// `Cargo.toml`, пишет и её (skill `finish-task`). Иначе окно, которое
/// покажет эту версию тем, у кого прежняя, останется без списка.
#[test]
fn the_newest_note_is_this_version() {
    let notes = notes();
    assert_eq!(
        notes.all().first().map(|note| note.version()),
        Some(Version::CURRENT),
        "версия в Cargo.toml — {}, а заметки о ней в начале public/notes.json нет",
        Version::CURRENT
    );
}

/// Свежие — сверху: версии убывают, дни не растут, и заметок не больше,
/// чем покажет окно ([`MAX_NOTES`]) — лишние только качались бы зря.
#[test]
fn notes_go_newest_first() {
    let notes = notes();
    let mut wrong = Vec::new();
    for pair in notes.all().windows(2) {
        if pair[0].version() <= pair[1].version() {
            wrong.push(format!("{} стоит выше {}, а не новее", pair[0].version(), pair[1].version()));
        }
        if pair[0].date() < pair[1].date() {
            wrong.push(format!("у {} день раньше, чем у {}", pair[0].version(), pair[1].version()));
        }
    }
    if notes.all().len() > MAX_NOTES {
        wrong.push(format!("заметок {}, а окно покажет не больше {MAX_NOTES}: старые — убрать", notes.all().len()));
    }
    assert!(wrong.is_empty(), "порядок заметок: {wrong:#?}");
}

/// У каждой версии — пункты на всех трёх языках, их поровну (это переводы
/// одного и того же), они короткие и записаны по правилам языка — тем же,
/// что строки окна (`i18n::flaw`).
#[test]
fn every_note_speaks_three_languages() {
    let mut wrong = Vec::new();
    for note in notes().all() {
        let version = note.version();
        let ru = note.items(Lang::Ru).len();
        for lang in Lang::ALL {
            let items = note.items(lang);
            if items.is_empty() || items.len() > MAX_ITEMS {
                wrong.push(format!("{version} ({lang:?}): пунктов {}, а нужно от 1 до {MAX_ITEMS}", items.len()));
            }
            if items.len() != ru {
                wrong.push(format!("{version} ({lang:?}): пунктов {}, а по-русски {ru}", items.len()));
            }
            for item in items {
                let chars = item.chars().count();
                if chars > MAX_CHARS {
                    wrong.push(format!("{version} ({lang:?}): пункт в {chars} знаков, предел {MAX_CHARS}"));
                }
                if let Some(flaw) = i18n::flaw(lang, item) {
                    wrong.push(format!("{version} ({lang:?}): {flaw} — «{item}»"));
                }
            }
        }
    }
    assert!(wrong.is_empty(), "заметки: {wrong:#?}");
}
