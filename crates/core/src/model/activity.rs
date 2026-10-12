//! Код вида экономической деятельности — по классификатору, который
//! применяется в Армении (задача 9).
//!
//! Классификатор — ՀԴ-011-2013, «Տնտեսական գործունեության տեսակների
//! դասակարգիչ», утверждён приказом министра экономики № 874-Ն от
//! 19.09.2013 (по закону: https://www.arlis.am/hy/acts/181173; сверено
//! 2026-10-12, разведка задачи 9). Код — пять цифр: раздел (2), группа (1),
//! класс (1) и национальный подкласс (1); пишется `62.01.0`. Буквы разделов
//! A–U «в код не входят» (§ 3 приложения), но в официальных текстах их
//! ставят впереди: `J62.01.0` — постановление правительства № 142-Ն от
//! 13.02.2025. Поэтому читаются обе записи, а хранится и показывается код
//! без буквы.
//!
//! До класса классификатор совпадает с NACE Rev. 2 (все 615 классов
//! и границы разделов — сверка с публикацией Eurostat KS-RA-07-015-EN,
//! там же), поэтому раздел по разделу NACE ([`SECTIONS`]) — тот же. Пятая
//! цифра — национальная. Проверяется, что раздел (две первые цифры) есть
//! в классификаторе, а буква, если её написали, — его: список всех
//! 992 подклассов в программу не встроен, и опечатку внутри раздела
//! поймает глаз.
//!
//! Риск: armstat выложил проект редакции 2.1 (по NACE 2.1), где разделы 69
//! и 70 уходят в раздел N, а группа 78.3 исчезает. Если его примут, коды
//! в правилах налога (`tax::turnover`) придётся сверить заново.
//!
//! В данных — строка `"62.01.0"`.

use std::fmt;

use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize, Serializer};

/// Разделы классификатора: буква и разделы (по две цифры) от и до —
/// NACE Rev. 2. Разделов без номера (04, 34, 40…) в классификаторе нет.
pub const SECTIONS: [(char, u8, u8); 21] = [
    ('A', 1, 3),
    ('B', 5, 9),
    ('C', 10, 33),
    ('D', 35, 35),
    ('E', 36, 39),
    ('F', 41, 43),
    ('G', 45, 47),
    ('H', 49, 53),
    ('I', 55, 56),
    ('J', 58, 63),
    ('K', 64, 66),
    ('L', 68, 68),
    ('M', 69, 75),
    ('N', 77, 82),
    ('O', 84, 84),
    ('P', 85, 85),
    ('Q', 86, 88),
    ('R', 90, 93),
    ('S', 94, 96),
    ('T', 97, 98),
    ('U', 99, 99),
];

/// Буква раздела, в который входит раздел `division`; `None` — такого
/// раздела в классификаторе нет.
fn section_of(division: u8) -> Option<char> {
    SECTIONS
        .iter()
        .find(|(_, from, to)| (*from..=*to).contains(&division))
        .map(|(letter, _, _)| *letter)
}

/// Код вида деятельности — пять цифр классификатора.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ActivityCode([u8; 5]);

/// Почему код не принят.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum CodeError {
    /// Не пять цифр.
    Malformed,
    /// Таких двух первых цифр в классификаторе нет: `04…`, `34…`.
    NoSuchDivision,
    /// Буква впереди — не того раздела: `K62.01.0`, а 62 — раздел J.
    WrongSection,
}

impl ActivityCode {
    /// Самая длинная запись, которую стоит набирать: `J 62.01.0` и пробелы.
    pub const MAX_INPUT: usize = 12;

    /// Код, как его пишет человек: `62.01.0`, `J62.01.0`, `6201 0` — точки
    /// и пробелы не важны, буква раздела — по желанию, латиницей в любом
    /// регистре.
    pub fn parse(input: &str) -> Result<Self, CodeError> {
        let text: String = input
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '.')
            .collect();
        let (letter, digits) = match text.chars().next() {
            Some(first) if first.is_ascii_alphabetic() => {
                (Some(first.to_ascii_uppercase()), &text[1..])
            }
            _ => (None, text.as_str()),
        };
        let bytes = digits.as_bytes();
        if bytes.len() != 5 || !bytes.iter().all(u8::is_ascii_digit) {
            return Err(CodeError::Malformed);
        }
        let mut code = [0; 5];
        for (place, digit) in code.iter_mut().zip(bytes) {
            *place = digit - b'0';
        }
        let section = section_of(code[0] * 10 + code[1]).ok_or(CodeError::NoSuchDivision)?;
        if letter.is_some_and(|letter| letter != section) {
            return Err(CodeError::WrongSection);
        }
        Ok(Self(code))
    }

    /// Буква раздела: `J` у `62.01.0`.
    pub fn section(self) -> char {
        // Код существует только с разделом из классификатора (`parse`).
        section_of(self.0[0] * 10 + self.0[1]).unwrap_or('?')
    }

    /// Начинается ли код с записи `prefix` — так коды называет закон:
    /// `69` — раздел, `78.2` — группа. Точки в записи не важны.
    pub fn starts_with(self, prefix: &str) -> bool {
        let digits = prefix.bytes().filter(|b| *b != b'.');
        let mut matched = 0;
        for (place, digit) in digits.enumerate() {
            match self.0.get(place) {
                Some(own) if digit.is_ascii_digit() && *own == digit - b'0' => matched += 1,
                _ => return false,
            }
        }
        matched > 0
    }
}

/// Запись классификатора: `62.01.0`. Это код, а не текст для перевода:
/// на всех языках он пишется одинаково.
impl fmt::Display for ActivityCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [a, b, c, d, e] = self.0;
        write!(f, "{a}{b}.{c}{d}.{e}")
    }
}

impl fmt::Debug for ActivityCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{self}", self.section())
    }
}

impl Serialize for ActivityCode {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

/// Читается только запись `62.01.0` — та, что пишет программа.
impl<'de> Deserialize<'de> for ActivityCode {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        match ActivityCode::parse(&text) {
            Ok(code) if code.to_string() == text => Ok(code),
            _ => Err(de::Error::invalid_value(
                de::Unexpected::Str(&text),
                &"код классификатора вида 62.01.0",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Записи, которые встречаются в выписках и в законе, читаются в один
    /// код; ожидаемое выписано человеком.
    #[test]
    fn reads_both_official_spellings() {
        let cases = [
            ("62.01.0", "62.01.0"),
            ("J62.01.0", "62.01.0"),
            ("j62.01.0", "62.01.0"),
            ("J 62.01.0", "62.01.0"),
            ("62010", "62.01.0"),
            ("  69.20.0 ", "69.20.0"),
            ("C26.11.0", "26.11.0"),
            ("01.11.0", "01.11.0"),
            ("99.00.0", "99.00.0"),
        ];
        let mut wrong = Vec::new();
        for (input, expected) in cases {
            match ActivityCode::parse(input) {
                Ok(code) if code.to_string() == expected => {}
                other => wrong.push(format!("«{input}»: {other:?}")),
            }
        }
        assert!(wrong.is_empty(), "коды прочитаны не так: {wrong:#?}");
    }

    /// Не пять цифр, раздела нет в классификаторе, буква чужого раздела —
    /// ошибки, а не догадки.
    #[test]
    fn rejects_what_is_not_a_code() {
        use CodeError::{Malformed, NoSuchDivision, WrongSection};
        let cases = [
            ("", Malformed),
            ("62.01", Malformed),
            ("62", Malformed),
            ("62.01.00", Malformed),
            ("JJ62.01.0", Malformed),
            ("62.0a.0", Malformed),
            ("Ա62.01.0", Malformed),
            ("04.00.0", NoSuchDivision),
            ("34.10.0", NoSuchDivision),
            ("00.00.0", NoSuchDivision),
            ("K62.01.0", WrongSection),
            ("N69.10.0", WrongSection),
        ];
        let mut wrong = Vec::new();
        for (input, expected) in cases {
            let got = ActivityCode::parse(input);
            if got != Err(expected) {
                wrong.push(format!("«{input}»: {got:?}, а не {expected:?}"));
            }
        }
        assert!(wrong.is_empty(), "прочитано не то: {wrong:#?}");
    }

    /// Разделов в классификаторе 88 (там же, сверка разведки задачи 9),
    /// и у каждого — одна буква; разделы букв не пересекаются.
    #[test]
    fn sections_cover_88_divisions() {
        let divisions: Vec<u8> = (0..=99).filter(|d| section_of(*d).is_some()).collect();
        assert_eq!(divisions.len(), 88);
        for (letter, from, to) in SECTIONS {
            for division in from..=to {
                assert_eq!(section_of(division), Some(letter), "раздел {division}");
            }
        }
        assert_eq!(ActivityCode::parse("62.01.0").map(ActivityCode::section), Ok('J'));
        assert_eq!(ActivityCode::parse("78.20.0").map(ActivityCode::section), Ok('N'));
    }

    /// Код начинается с записи закона: раздел, группа, класс.
    #[test]
    fn prefixes_as_the_law_writes_them() {
        let code = |text| ActivityCode::parse(text).expect("в тесте — годный код");
        assert!(code("69.10.0").starts_with("69"));
        assert!(code("78.20.0").starts_with("78.2"));
        assert!(!code("78.10.0").starts_with("78.2"));
        assert!(code("62.02.9").starts_with("62.02.9"));
        assert!(!code("62.02.1").starts_with("62.02.9"));
        assert!(!code("70.22.0").starts_with("7022.01"));
        assert!(!code("70.22.0").starts_with(""));
        assert!(!code("70.22.0").starts_with("x"));
    }

    /// В данных — `"62.01.0"`; запись с буквой или без точек — порча.
    #[test]
    fn data_keeps_the_dotted_code() {
        let code = ActivityCode::parse("J62.01.0").expect("годный код");
        assert_eq!(serde_json::to_string(&code).ok().as_deref(), Some(r#""62.01.0""#));
        assert_eq!(serde_json::from_str::<ActivityCode>(r#""62.01.0""#).ok(), Some(code));
        for json in [r#""J62.01.0""#, r#""62010""#, r#""04.00.0""#, "62010"] {
            assert!(
                serde_json::from_str::<ActivityCode>(json).is_err(),
                "{json} прочитан"
            );
        }
    }
}
