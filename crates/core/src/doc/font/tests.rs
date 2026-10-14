//! Тесты чтения шрифта и подмножества — на файлах окна.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use super::*;
use crate::doc::fonts::FILES;

/// Файл шрифта окна (`crates/app/public/fonts/`).
fn bytes(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../app/public/fonts")
        .join(name);
    fs::read(&path).unwrap_or_else(|_| panic!("{} не читается", path.display()))
}

/// Имя шрифта PostScript (`name`, запись 6, Windows Unicode — UTF-16BE).
fn postscript_name(bytes: &[u8]) -> Option<String> {
    let font = Font::parse(bytes).ok()?;
    let name = font.table(b"name")?;
    let count = usize::from(u16_at(name, 2).ok()?);
    let strings = usize::from(u16_at(name, 4).ok()?);
    (0..count).find_map(|i| {
        let record = 6 + 12 * i;
        let platform = u16_at(name, record).ok()?;
        let id = u16_at(name, record + 6).ok()?;
        if platform != 3 || id != 6 {
            return None;
        }
        let len = usize::from(u16_at(name, record + 8).ok()?);
        let at = strings + usize::from(u16_at(name, record + 10).ok()?);
        let units: Vec<u16> = name
            .get(at..at + len)?
            .chunks(2)
            .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
            .collect();
        String::from_utf16(&units).ok()
    })
}

/// Каждый файл таблицы читается, и имя PostScript в таблице — его
/// собственное: под этим именем шрифт стоит в PDF.
#[test]
fn every_file_parses_with_its_postscript_name() {
    let mut wrong = Vec::new();
    for file in FILES {
        let data = bytes(file.name);
        match Font::parse(&data) {
            Ok(font) => {
                if font.units_per_em() == 0 || font.chars().count() == 0 {
                    wrong.push(format!("{}: пустой шрифт", file.name));
                }
            }
            Err(error) => wrong.push(format!("{}: {error:?}", file.name)),
        }
        let name = postscript_name(&data);
        if name.as_deref() != Some(file.postscript) {
            wrong.push(format!(
                "{}: имя {name:?}, а в таблице {}",
                file.name, file.postscript
            ));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// Знак находится в своём файле, ширина у него есть, а чужого знака нет:
/// ֏ — в армянском, а не в латинском (Правило 4), № — в кириллице.
#[test]
fn glyphs_and_widths_are_where_they_should_be() {
    let latin = bytes("noto-sans-latin-400.ttf");
    let armenian = bytes("noto-sans-armenian-400.ttf");
    let cyrillic = bytes("noto-sans-cyrillic-400.ttf");
    let latin = Font::parse(&latin).expect("латиница читается");
    let armenian = Font::parse(&armenian).expect("армянский читается");
    let cyrillic = Font::parse(&cyrillic).expect("кириллица читается");
    for c in ['A', '0', ' ', '\u{A0}', '$', '€'] {
        let glyph = latin
            .glyph(c)
            .unwrap_or_else(|| panic!("«{c}» нет в латинице"));
        assert!(latin.advance(glyph) > 0, "у «{c}» нет ширины");
    }
    for c in ['Ա', 'և', '֏', '։'] {
        assert!(armenian.glyph(c).is_some(), "«{c}» нет в армянском");
    }
    assert!(cyrillic.glyph('№').is_some());
    assert!(cyrillic.glyph('Ж').is_some());
    assert_eq!(latin.glyph('֏'), None);
    assert_eq!(latin.glyph('Ж'), None);
    // Цифры у Noto — одной ширины: столбец сумм не «пляшет».
    let widths: BTreeSet<u16> = ('0'..='9')
        .filter_map(|c| latin.glyph(c))
        .map(|g| latin.advance(g))
        .collect();
    assert_eq!(widths.len(), 1, "ширины цифр: {widths:?}");
}

/// Испорченный файл — ошибка, а не паника: обрезанный на любом байте
/// и чужой.
#[test]
fn broken_files_are_errors_not_panics() {
    let data = bytes("noto-sans-armenian-400.ttf");
    for len in (0..data.len()).step_by(97).chain([1, 4, 11, 12, 100, 300]) {
        let _ = Font::parse(&data[..len]);
    }
    assert_eq!(Font::parse(b"%PDF-1.7").err(), Some(FontError::Malformed));
    assert_eq!(Font::parse(&[]).err(), Some(FontError::Malformed));
    // Таблица, указывающая за конец файла.
    let mut lying = data.clone();
    let record = 12;
    lying[record + 12..record + 16].copy_from_slice(&u32::MAX.to_be_bytes());
    assert_eq!(Font::parse(&lying).err(), Some(FontError::Malformed));
}

/// Подмножество — снова шрифт: номера и ширины глифов прежние, контуры —
/// только у нужных глифов и у их составных частей, файл меньше.
#[test]
fn subset_keeps_glyph_ids_and_only_used_outlines() {
    let data = bytes("noto-sans-latin-ext-400.ttf");
    let font = Font::parse(&data).expect("шрифт читается");
    // Буквы с надстрочными знаками — у Noto часто составные глифы.
    let text = "ĀāŞşŽžĐđ";
    let used: BTreeSet<u16> = text.chars().filter_map(|c| font.glyph(c)).collect();
    assert_eq!(used.len(), text.chars().count(), "все буквы есть в файле");
    let composite: Vec<u16> = used
        .iter()
        .copied()
        .filter(|&g| !font.components(g).unwrap_or_default().is_empty())
        .collect();
    assert!(
        !composite.is_empty(),
        "среди букв есть составные — иначе тест ничего не проверяет"
    );

    let subset = font.subset(&used).expect("подмножество собирается");
    assert!(
        subset.len() * 4 < data.len(),
        "{} из {} байтов",
        subset.len(),
        data.len()
    );
    let small = Font::parse(&subset).expect("подмножество — снова шрифт");
    for &glyph in &used {
        // Глиф в подмножестве — с границы четырёх байтов: хвост — нули.
        let (before, after) = (
            font.glyph_data(glyph).expect("контур"),
            small.glyph_data(glyph).expect("контур"),
        );
        assert!(
            after.starts_with(before) && after[before.len()..].iter().all(|&b| b == 0),
            "контур {glyph}"
        );
        assert_eq!(small.advance(glyph), font.advance(glyph), "ширина {glyph}");
        for part in font.components(glyph).expect("части читаются") {
            assert!(
                !small.glyph_data(part).expect("часть есть").is_empty(),
                "часть {part} пустая"
            );
        }
    }
    // Глиф, которого нет в тексте и который не часть составного, — пустой.
    let other = font.glyph('Ŵ').expect("Ŵ есть в файле");
    let parts: BTreeSet<u16> = used
        .iter()
        .flat_map(|&g| font.components(g).unwrap_or_default())
        .collect();
    if !parts.contains(&other) {
        assert!(small.glyph_data(other).expect("запись есть").is_empty());
    }
    // Суммы таблиц сходятся: читалки PDF их обычно не проверяют, но
    // испорченный шрифт им лучше не давать.
    let whole = checksum(&subset);
    assert_eq!(whole, 0xB1B0_AFBA, "сумма файла с checkSumAdjustment");
}
