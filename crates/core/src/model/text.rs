//! Текст, который ввёл человек: имена, адреса, реквизиты.
//!
//! В данных — обычная строка JSON. Читается с проверкой (Правило 13): файл
//! копии приходит откуда угодно, а текст из него попадёт в окно и в PDF.
//! Управляющих знаков в тексте нет — табуляция или `\u{0}` в имени в PDF
//! стали бы пустым квадратом, — а перевод строки бывает только
//! в многострочном тексте ([`Lines`]): в адресе.
//!
//! Что программа пишет, то она и читает: в данных лежит текст, уже
//! приведённый конструктором (`new`), и всё прочее при чтении — порча, а не
//! повод «привести» молча.

use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize};

/// Почему введённый текст не принят.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum TextError {
    /// Длиннее предела. Поле ввода сам предел держит (`maxlength`), так что
    /// это — вставка в обход поля или испорченные данные.
    TooLong { max: usize },
}

/// Текст в одну строку: без пробелов по краям, без управляющих знаков, не
/// длиннее [`Line::MAX_CHARS`] знаков. Пустой — поле не заполнено.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Default, Serialize)]
#[serde(transparent)]
pub struct Line(String);

impl Line {
    /// Предел — с запасом на самое длинное в реквизитах: полное название
    /// банка с организационной формой — около сотни знаков.
    pub const MAX_CHARS: usize = 300;

    /// Текст из поля ввода: управляющие знаки (табуляция из вставленного
    /// текста) становятся пробелами, края обрезаются.
    pub fn new(input: &str) -> Result<Self, TextError> {
        let text: String = input
            .chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .collect();
        within(text.trim(), Self::MAX_CHARS).map(Self)
    }

    /// Текст как есть.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Пусто ли поле.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Текст в несколько строк — адрес: переводы строк остаются, `\r\n` и `\r`
/// становятся `\n`, остальные управляющие знаки — пробелами, края текста
/// обрезаются. Не длиннее [`Lines::MAX_CHARS`] знаков.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Default, Serialize)]
#[serde(transparent)]
pub struct Lines(String);

impl Lines {
    /// Адрес в три-четыре строки — пара сотен знаков; предел — с запасом.
    pub const MAX_CHARS: usize = 1_000;

    /// Текст из многострочного поля.
    pub fn new(input: &str) -> Result<Self, TextError> {
        let text: String = input
            .replace("\r\n", "\n")
            .chars()
            .map(|c| match c {
                '\r' => '\n',
                '\n' => '\n',
                c if c.is_control() => ' ',
                c => c,
            })
            .collect();
        within(text.trim(), Self::MAX_CHARS).map(Self)
    }

    /// Текст как есть.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Пусто ли поле.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Обрезанный текст, если он не длиннее `max` знаков. Знак — точка кода
/// Unicode: армянская буква — два байта UTF-8, но один знак.
fn within(text: &str, max: usize) -> Result<String, TextError> {
    if text.chars().count() > max {
        Err(TextError::TooLong { max })
    } else {
        Ok(text.to_owned())
    }
}

/// Читается только текст, который `new` оставил бы как есть.
fn read_exact<'de, D, T>(
    deserializer: D,
    new: fn(&str) -> Result<T, TextError>,
    inner: fn(&T) -> &str,
) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
{
    let text = String::deserialize(deserializer)?;
    match new(&text) {
        Ok(value) if inner(&value) == text => Ok(value),
        _ => Err(de::Error::invalid_value(
            de::Unexpected::Str(&text),
            &"текст без пробелов по краям и управляющих знаков, в пределах длины",
        )),
    }
}

impl<'de> Deserialize<'de> for Line {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        read_exact(deserializer, Self::new, Self::as_str)
    }
}

impl<'de> Deserialize<'de> for Lines {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        read_exact(deserializer, Self::new, Self::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Из поля ввода: края обрезаны, табуляция и прочие управляющие знаки —
    /// пробелы, а армянские и русские буквы считаются знаками, а не байтами.
    #[test]
    fn input_is_cleaned() {
        let cases = [
            ("  ԱՁ Արամ Պետրոսյան  ", "ԱՁ Արամ Պետրոսյան"),
            ("ИП\tИванов", "ИП Иванов"),
            ("\u{0}IE Aram\u{7f}", "IE Aram"),
            ("", ""),
            ("   ", ""),
        ];
        let mut wrong = Vec::new();
        for (input, expected) in cases {
            match Line::new(input) {
                Ok(line) if line.as_str() == expected => {}
                other => wrong.push(format!("{input:?}: {other:?}")),
            }
        }
        let address = Lines::new("  Abovyan 1\r\nYerevan\r0001\tArmenia \n").expect("адрес");
        if address.as_str() != "Abovyan 1\nYerevan\n0001 Armenia" {
            wrong.push(format!("адрес: {address:?}"));
        }
        assert!(wrong.is_empty(), "текст приведён не так: {wrong:#?}");
    }

    /// Предел — в знаках: 300 армянских букв — это 600 байтов, но годятся.
    #[test]
    fn limit_counts_characters() {
        assert!(Line::new(&"ա".repeat(Line::MAX_CHARS)).is_ok());
        assert_eq!(
            Line::new(&"ա".repeat(Line::MAX_CHARS + 1)),
            Err(TextError::TooLong {
                max: Line::MAX_CHARS
            })
        );
        assert_eq!(
            Lines::new(&"a\n".repeat(Lines::MAX_CHARS)),
            Err(TextError::TooLong {
                max: Lines::MAX_CHARS
            })
        );
    }

    /// В данных — строка, и читается только та, что `new` оставил бы как
    /// есть: с пробелом по краю, табуляцией или переводом строки в одну
    /// строку — порча.
    #[test]
    fn data_reads_only_clean_text() {
        let line: Line = serde_json::from_str(r#""ИП Иванов""#).expect("читается");
        assert_eq!(serde_json::to_string(&line).ok().as_deref(), Some(r#""ИП Иванов""#));
        let lines: Lines = serde_json::from_str(r#""Abovyan 1\nYerevan""#).expect("читается");
        assert_eq!(lines.as_str(), "Abovyan 1\nYerevan");
        let mut wrong = Vec::new();
        for json in [
            r#"" ИП""#,
            r#""ИП ""#,
            r#""ИП\tИванов""#,
            r#""ИП\nИванов""#,
            r#""\u0000""#,
            "7",
            "null",
        ] {
            if let Ok(line) = serde_json::from_str::<Line>(json) {
                wrong.push(format!("строка {json} прочитана как {line:?}"));
            }
        }
        for json in [r#""a\r\nb""#, r#""a\tb""#, r#""\na""#] {
            if let Ok(lines) = serde_json::from_str::<Lines>(json) {
                wrong.push(format!("строки {json} прочитаны как {lines:?}"));
            }
        }
        let long = format!("\"{}\"", "a".repeat(Line::MAX_CHARS + 1));
        if serde_json::from_str::<Line>(&long).is_ok() {
            wrong.push("прочитана строка длиннее предела".to_owned());
        }
        assert!(wrong.is_empty(), "прочитана порча: {wrong:#?}");
    }
}
