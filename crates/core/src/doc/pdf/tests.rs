//! Тесты записи чисел, строк и таблицы знаков PDF. Документ целиком
//! проверяет `doc/tests.rs`.

use super::*;

#[test]
fn numbers_are_written_without_floats() {
    assert_eq!(num(59_528), "595.28");
    assert_eq!(num(84_189), "841.89");
    assert_eq!(num(950), "9.5");
    assert_eq!(num(1_200), "12");
    assert_eq!(num(5), "0.05");
    assert_eq!(num(-50), "-0.5");
    assert_eq!(num(0), "0");
}

#[test]
fn colors_are_fractions_of_one() {
    assert_eq!(color([0, 0, 0]), "0 0 0");
    assert_eq!(color([255, 255, 255]), "1 1 1");
    assert_eq!(color([0x20, 0x1E, 0x1D]), "0.125 0.117 0.113");
}

/// Строка сведений — UTF-16BE шестнадцатерично, с меткой порядка байтов:
/// армянское название и знаки вне BMP не ломают файл.
#[test]
fn info_strings_are_utf16() {
    assert_eq!(text_string("Ա"), "<FEFF0531>");
    assert_eq!(text_string("a(b)"), "<FEFF0061002800620029>");
    assert_eq!(text_string("😀"), "<FEFFD83DDE00>");
}

/// В блоке таблицы знаков — не больше ста записей, знак вне BMP —
/// суррогатной парой.
#[test]
fn to_unicode_blocks_hold_at_most_a_hundred() {
    let glyphs: BTreeMap<u16, char> = (1..=150u16)
        .map(|g| (g, char::from_u32(0x530 + u32::from(g)).unwrap_or('?')))
        .chain([(200, '😀')])
        .collect();
    let cmap = to_unicode(&glyphs);
    assert!(cmap.contains("100 beginbfchar"));
    assert!(cmap.contains("51 beginbfchar"));
    assert!(cmap.contains("<00C8> <D83DDE00>"));
    assert!(cmap.contains("<0001> <0531>"));
}

#[test]
fn subset_tags_are_six_capital_letters() {
    assert_eq!(subset_tag(0), "AAAAAA");
    assert_eq!(subset_tag(1), "AAAAAB");
    assert_eq!(subset_tag(27), "AAAABB");
}
