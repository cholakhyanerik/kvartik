//! ՀՎՀՀ — учётный номер налогоплательщика (задача 9): у ИП в профиле,
//! у армянской фирмы-клиента (задача 10).
//!
//! Налоговый кодекс, ст. 287, ч. 4: «ՀՎՀՀ-ն կազմված է ութանիշ թվից:
//! ՀՎՀՀ-ի առաջին յոթ նիշը հանդիսանում է հերթական համար, ութերորդ նիշը
//! ստուգիչ է … ՀՎՀՀ-ի ստուգիչ նիշի հաշվարկման բանաձևը սահմանում է հարկային
//! մարմինը» — восемь цифр: семь — порядковый номер, восьмая — контрольная,
//! а формулу контрольной цифры устанавливает налоговый орган (по закону:
//! https://www.arlis.am/hy/acts/201478; сверено 2026-10-12, разведка
//! задачи 9). Номер бывает с нулями впереди — такие есть в списках ՊԵԿ, —
//! поэтому он строка, а не число.
//!
//! **Контрольная цифра не проверяется.** Формулу ՊԵԿ не публикует: её нет
//! ни на arlis.am, ни на src.am, ни в записке ՊԵԿ для ОЭСР (2023-10-06).
//! Формула, выведенная разведкой из 15 248 номеров списков ՊԵԿ, не сходится
//! на шести настоящих номерах ИП, зарегистрированных в конце 2025 года
//! (там же). Догадка, отвергающая настоящий номер, хуже, чем никакой
//! проверки: человек не смог бы сохранить свой профиль (Правило 6).
//!
//! В данных — строка `"07654321"`.
//!
//! **В тестах — только выдуманные номера** (с 0.0.178): репозиторий
//! публичный, а ՀՎՀՀ ИП — номер человека. Настоящие номера разведки задачи 9
//! здесь больше не лежат; прежний остался только в зашифрованных образцах
//! копий (`tests/backups/`, история — Правило 2): решение человека 2026-11-23.

use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize};

/// ՀՎՀՀ — восемь цифр.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize)]
#[serde(transparent)]
pub struct Tin(String);

/// Почему ՀՎՀՀ не принят.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum TinError {
    /// Не восемь цифр.
    Malformed,
}

impl Tin {
    /// Цифр в номере.
    pub const DIGITS: usize = 8;

    /// Номер из поля ввода: пробелы не важны.
    pub fn parse(input: &str) -> Result<Self, TinError> {
        let digits: String = input.chars().filter(|c| !c.is_whitespace()).collect();
        if digits.len() == Self::DIGITS && digits.bytes().all(|b| b.is_ascii_digit()) {
            Ok(Self(digits))
        } else {
            Err(TinError::Malformed)
        }
    }

    /// Номер как есть: `07654321`.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Читается только номер, который `parse` оставил бы как есть.
impl<'de> Deserialize<'de> for Tin {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        match Tin::parse(&text) {
            Ok(tin) if tin.0 == text => Ok(tin),
            _ => Err(de::Error::invalid_value(
                de::Unexpected::Str(&text),
                &"ՀՎՀՀ — восемь цифр",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Восемь цифр, с нулями впереди тоже, а контрольная цифра — любая:
    /// на настоящих номерах ИП выведенная формула не сходилась (разведка
    /// задачи 9). Номера выдуманные (шапка модуля).
    #[test]
    fn eight_digits_with_leading_zeros() {
        let numbers = ["00054321", "07654321", "12345678", "20000001", "20000002", "99999990"];
        let mut wrong = Vec::new();
        for number in numbers {
            match Tin::parse(number) {
                Ok(tin) if tin.as_str() == number => {}
                other => wrong.push(format!("{number}: {other:?}")),
            }
        }
        if Tin::parse(" 0765 4321 ").map(|tin| tin.0) != Ok("07654321".to_owned()) {
            wrong.push("пробелы не убраны".to_owned());
        }
        for text in ["", "0765432", "076543210", "0765432a", "07654321/1", "٠٧٦٥٤٣٢١"] {
            if let Ok(tin) = Tin::parse(text) {
                wrong.push(format!("«{text}» принят как {tin:?}"));
            }
        }
        assert!(wrong.is_empty(), "ՀՎՀՀ: {wrong:#?}");
    }

    /// В данных — строка из восьми цифр; число или запись с пробелом — порча:
    /// у числа пропали бы нули впереди.
    #[test]
    fn data_keeps_a_string() {
        let tin = Tin::parse("00054321").expect("годный номер");
        assert_eq!(serde_json::to_string(&tin).ok().as_deref(), Some(r#""00054321""#));
        assert_eq!(serde_json::from_str::<Tin>(r#""00054321""#).ok(), Some(tin));
        for json in ["54321", r#""0005 4321""#, r#""5432""#] {
            assert!(serde_json::from_str::<Tin>(json).is_err(), "{json} прочитан");
        }
    }
}
