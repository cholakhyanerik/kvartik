//! Банковский счёт ИП — реквизиты для счёта и акта (задача 9).
//!
//! Счетов несколько: в драмах — для местных клиентов, в долларах и евро —
//! для иностранных. У валютного счёта в реквизитах ещё и банк-корреспондент:
//! через него идёт перевод, и без него иностранный клиент платёж не отправит.
//!
//! В данных: `{"currency":"USD","number":"…","bank":"…","swift":"ARMIAM22",
//! "correspondent":"…"}`; `swift` — `null`, если не указан.

use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize};

use super::money::Currency;
use super::text::{Line, Lines};

/// Счёт в банке.
///
/// Номер обязателен: счёт без номера в реквизиты не годится. Остальное —
/// как в реквизитах банка: формат номера не проверяется, потому что счёт
/// ИП может быть и не в армянском банке.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize)]
pub struct BankAccount {
    currency: Currency,
    number: Line,
    bank: Line,
    swift: Option<Swift>,
    correspondent: Lines,
}

/// Поля счёта, как они лежат в данных, — до проверки.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BankAccountFields {
    currency: Currency,
    number: Line,
    bank: Line,
    swift: Option<Swift>,
    correspondent: Lines,
}

/// Почему счёт не принят.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum AccountError {
    /// Номера нет.
    NoNumber,
}

impl BankAccount {
    /// Счёт, если у него есть номер.
    pub fn new(
        currency: Currency,
        number: Line,
        bank: Line,
        swift: Option<Swift>,
        correspondent: Lines,
    ) -> Result<Self, AccountError> {
        if number.is_empty() {
            return Err(AccountError::NoNumber);
        }
        Ok(Self {
            currency,
            number,
            bank,
            swift,
            correspondent,
        })
    }

    /// Валюта счёта.
    pub const fn currency(&self) -> Currency {
        self.currency
    }

    /// Номер счёта, как в реквизитах банка.
    pub const fn number(&self) -> &Line {
        &self.number
    }

    /// Банк.
    pub const fn bank(&self) -> &Line {
        &self.bank
    }

    /// SWIFT-код банка; `None` — не указан.
    pub const fn swift(&self) -> Option<&Swift> {
        self.swift.as_ref()
    }

    /// Банк-корреспондент: название, SWIFT, номер счёта — как их даёт банк.
    pub const fn correspondent(&self) -> &Lines {
        &self.correspondent
    }
}

/// Счёт читается через [`BankAccount::new`]: счёт без номера — порча.
impl<'de> Deserialize<'de> for BankAccount {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let BankAccountFields {
            currency,
            number,
            bank,
            swift,
            correspondent,
        } = BankAccountFields::deserialize(deserializer)?;
        Self::new(currency, number, bank, swift, correspondent)
            .map_err(|_| de::Error::custom("счёт без номера"))
    }
}

/// SWIFT-код (BIC) банка по ISO 9362: префикс банка — 4 латинские буквы
/// или цифры, страна — 2 буквы, место — 2 буквы или цифры, отделение —
/// ещё 3 или ничего. Всего 8 или 11 знаков; хранится заглавными: `ARMIAM22`.
///
/// Префикс по ISO 9362:2014 может содержать и цифры, хотя выданные коды пока
/// буквенные: строже стандарта проверка не бывает, иначе настоящий код
/// однажды не пройдёт.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize)]
#[serde(transparent)]
pub struct Swift(String);

/// Почему SWIFT-код не принят.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum SwiftError {
    /// Не 8 и не 11 знаков или знаки не на своих местах.
    Malformed,
}

impl Swift {
    /// Код из поля ввода: пробелы убираются, буквы — заглавные.
    pub fn parse(input: &str) -> Result<Self, SwiftError> {
        let code: String = input
            .chars()
            .filter(|c| !c.is_whitespace())
            .map(|c| c.to_ascii_uppercase())
            .collect();
        let bytes = code.as_bytes();
        let alnum = |range: std::ops::Range<usize>| {
            bytes[range]
                .iter()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        };
        let fits = matches!(bytes.len(), 8 | 11)
            && alnum(0..4)
            && bytes[4..6].iter().all(u8::is_ascii_uppercase)
            && alnum(6..bytes.len());
        if fits {
            Ok(Self(code))
        } else {
            Err(SwiftError::Malformed)
        }
    }

    /// Код, как его пишут в реквизитах: `ARMIAM22`.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Код читается только в той записи, что пишет [`Swift::parse`].
impl<'de> Deserialize<'de> for Swift {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        match Swift::parse(&text) {
            Ok(code) if code.0 == text => Ok(code),
            _ => Err(de::Error::invalid_value(
                de::Unexpected::Str(&text),
                &"SWIFT-код ISO 9362 заглавными, 8 или 11 знаков",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(text: &str) -> Line {
        Line::new(text).expect("в тесте — годный текст")
    }

    /// Коды известных банков (Америабанк, Citibank, Commerzbank, Deutsche
    /// Bank) и запись с пробелами и строчными буквами читаются; чужое — нет.
    #[test]
    fn swift_follows_iso_9362() {
        let read = [
            ("ARMIAM22", "ARMIAM22"),
            ("armiam22", "ARMIAM22"),
            ("ARMI AM 22", "ARMIAM22"),
            ("CITIUS33", "CITIUS33"),
            ("COBADEFFXXX", "COBADEFFXXX"),
            ("DEUTDEFF500", "DEUTDEFF500"),
        ];
        let rejected = [
            "", "ARMIAM2", "ARMIAM222", "ARMIAM22XX", "ARMI1M22", "ARMIA222", "ԱՐՄԻԱՄ22", "ARMIAM2_",
            "ARMIAM22XXXX",
        ];
        let mut wrong = Vec::new();
        for (input, expected) in read {
            match Swift::parse(input) {
                Ok(code) if code.as_str() == expected => {}
                other => wrong.push(format!("«{input}»: {other:?}")),
            }
        }
        for input in rejected {
            if let Ok(code) = Swift::parse(input) {
                wrong.push(format!("«{input}» принят как {code:?}"));
            }
        }
        assert!(wrong.is_empty(), "SWIFT: {wrong:#?}");
    }

    /// Без номера счёта нет — ни из формы, ни из данных.
    #[test]
    fn account_needs_a_number() {
        assert_eq!(
            BankAccount::new(
                Currency::Usd,
                Line::default(),
                line("Ameriabank"),
                None,
                Lines::default()
            ),
            Err(AccountError::NoNumber)
        );
        let json = r#"{"currency":"USD","number":"","bank":"Ameriabank","swift":null,"correspondent":""}"#;
        assert!(serde_json::from_str::<BankAccount>(json).is_err());
    }

    /// Запись в данных выписана человеком: что прочитано, то и записывается
    /// обратно, слово в слово. Строчный SWIFT в данных — порча.
    #[test]
    fn data_shape_is_pinned() {
        let sample = r#"{"currency":"USD","number":"1570012345678900","bank":"Ameriabank CJSC","swift":"ARMIAM22","correspondent":"Citibank N.A., New York\nSWIFT: CITIUS33"}"#;
        let account: BankAccount = serde_json::from_str(sample).expect("образец читается");
        assert_eq!(account.currency(), Currency::Usd);
        assert_eq!(account.swift().map(Swift::as_str), Some("ARMIAM22"));
        assert_eq!(
            serde_json::to_string(&account).ok().as_deref(),
            Some(sample)
        );
        let lower = sample.replace("ARMIAM22", "armiam22");
        assert!(serde_json::from_str::<BankAccount>(&lower).is_err());
        let extra = sample.replace(r#""swift""#, r#""iban":"x","swift""#);
        assert!(serde_json::from_str::<BankAccount>(&extra).is_err());
    }
}
