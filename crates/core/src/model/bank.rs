//! Банковский счёт ИП — реквизиты для счёта и акта (задача 9).
//!
//! Счетов несколько: в драмах — для местных клиентов, в долларах и евро —
//! для иностранных. У валютного счёта в реквизитах ещё и банк-корреспондент:
//! через него идёт перевод, и без него иностранный клиент платёж не отправит.
//!
//! В данных: `{"currency":"USD","number":"…","bank":"…","swift":"ARMIAM22",
//! "correspondent":"…"}`; `swift` — `null`, если не указан.
//!
//! У номера счёта в банке Армении есть контрольная цифра (задача 56), и
//! опечатку в нём окно называет сразу при вводе (`BankAccount::mistyped`):
//! платёж иностранного клиента на чужой номер возвращается через неделю,
//! за вычетом комиссий SWIFT. Решение совета ЦБ РА № 178-Ն от 23.07.2013,
//! п. 28–40 (по закону: https://www.arlis.am/hy/acts/216785, редакция
//! с 29.11.2025; сверено 2026-10-16, разведка задачи 56; то же правило —
//! с 06.02.2003, в решении № 394-Ն, отменённом этим):
//! - п. 28 — номера счетов клиентов ЦБ, коммерческих банков
//!   и Казначейства, о валюте ни слова: валютные счета там же;
//! - п. 29–30 — «Հաշվեհամարը թվերի հաջորդականություն է»: 12 обязательных
//!   цифр (банк — 3, подразделение — 2, клиент — 6, «ստուգիչ նիշը» — 12-я)
//!   и до 4 необязательных после неё;
//! - п. 33–35 — необязательные в контрольную не входят, а на бумаге
//!   отделяются дефисом: `220185546730-21`;
//! - п. 39–40 — проверка: первые 12 цифр с весами 2 и 1 слева направо,
//!   у произведения складываются цифры, и сумма делится на 10. Это алгоритм
//!   Луна ровно по первым 12 цифрам; по всей строке он дал бы ложную тревогу
//!   у настоящих 16-значных номеров. Пример акта — `127027482877`.

use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize};

use super::money::Currency;
use super::text::{Line, Lines};

/// Счёт в банке.
///
/// Номер обязателен: счёт без номера в реквизиты не годится. Остальное —
/// как в реквизитах банка: формат номера не запрещается, потому что счёт
/// ИП может быть и не в армянском банке, а о похожем на опечатку окно
/// только предупреждает ([`BankAccount::mistyped`]).
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

    /// Похоже на опечатку: номер — счёт в банке Армении, а контрольная
    /// цифра не сходится (решение ЦБ № 178-Ն, шапка модуля).
    ///
    /// Номером банка Армении считается номер только из цифр — пробелы
    /// и дефисы между ними не в счёт, — 12–16 цифр, и SWIFT банка не
    /// указан или он армянский. Номер с буквами (IBAN) и SWIFT другой
    /// страны — не тот случай: правило ЦБ о них ничего не говорит. Это
    /// предупреждение, а не запрет: чисто цифровой номер бывает и у
    /// иностранного банка, а сверить, что номер верен, можно только
    /// с реквизитами банка. Перестановку соседних 0 и 9 и ошибку в 13–16-й
    /// цифре проверка не ловит.
    pub fn mistyped(number: &str, swift: Option<&Swift>) -> bool {
        if swift.is_some_and(|swift| swift.country() != "AM") {
            return false;
        }
        let mut digits = [0; ARMENIAN_DIGITS_MAX];
        let mut count = 0;
        for c in number.chars() {
            if c.is_whitespace() || c == '-' {
                continue;
            }
            let Some(digit) = c.to_digit(10) else {
                return false;
            };
            let Some(slot) = digits.get_mut(count) else {
                return false;
            };
            *slot = digit;
            count += 1;
        }
        count >= ARMENIAN_DIGITS_CHECKED
            && !check_sum(&digits[..ARMENIAN_DIGITS_CHECKED]).is_multiple_of(10)
    }
}

/// Обязательных цифр в номере счёта банка Армении; последняя из них —
/// контрольная (решение ЦБ № 178-Ն, п. 29–30).
const ARMENIAN_DIGITS_CHECKED: usize = 12;

/// Цифр в номере вместе с необязательными (там же).
const ARMENIAN_DIGITS_MAX: usize = 16;

/// Сумма проверки по п. 39: цифры с весами 2, 1, 2, 1… слева направо,
/// у произведения складываются цифры — 14 даёт 1 + 4. У верного номера
/// она делится на 10.
fn check_sum(digits: &[u32]) -> u32 {
    digits
        .iter()
        .enumerate()
        .map(|(at, digit)| {
            let product = if at.is_multiple_of(2) { digit * 2 } else { *digit };
            product / 10 + product % 10
        })
        .sum()
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

    /// Страна банка — 5-й и 6-й знаки, ISO 3166-1: `AM`.
    pub fn country(&self) -> &str {
        &self.0[4..6]
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

    /// Номера решения ЦБ № 178-Ն и публичные реквизиты — счета ЦБ
    /// в драмах и валюте, Казначейства и 16-значный Америабанка (разведка
    /// задачи 56) — сходятся в любой записи; любая неверная цифра из
    /// первых 12 и перестановка соседних — опечатка. Чужое не проверяется.
    #[test]
    fn armenian_number_has_a_check_digit() {
        let armenian = Swift::parse("ARMIAM22").ok();
        let foreign = Swift::parse("CITIUS33").ok();
        let mut wrong = Vec::new();
        let fits = [
            // п. 35 и 37: оба примера, на бумаге и в платеже.
            "127027482877",
            "220185546730",
            "220185546730-21",
            "22018554673021",
            // ЦБ РА: AMD, USD, EUR, RUB (zinapah.am).
            "103008660005",
            "103008661011",
            "103008661490",
            "103008661581",
            // Казначейство (e-payments.am).
            "900005003315",
            // Америабанк, 16 цифр (ameriabank.am): Лун по всей строке
            // назвал бы его опечаткой.
            "1570036910764400",
            "15700 369107 64400",
            " 1570 0369 1076 4400 ",
            "1570036910764400\u{A0}",
        ];
        for number in fits {
            for swift in [None, armenian.as_ref()] {
                if BankAccount::mistyped(number, swift) {
                    wrong.push(format!("«{number}» ({swift:?}) назван опечаткой"));
                }
            }
        }
        for good in ["127027482877", "220185546730"] {
            let digits: Vec<char> = good.chars().collect();
            for at in 0..digits.len() {
                for other in ('0'..='9').filter(|&other| other != digits[at]) {
                    let mut typo = digits.clone();
                    typo[at] = other;
                    let typo: String = typo.into_iter().collect();
                    for tail in ["", "21", "0001"] {
                        let number = format!("{typo}{tail}");
                        if !BankAccount::mistyped(&number, None) {
                            wrong.push(format!("«{number}»: цифра {} не замечена", at + 1));
                        }
                    }
                }
                // 0 и 9 местами Лун не различает (шапка `mistyped`).
                let pair = digits.get(at..at + 2);
                if let Some(&[a, b]) = pair
                    && a != b
                    && !matches!((a, b), ('0', '9') | ('9', '0'))
                {
                    let mut typo = digits.clone();
                    typo.swap(at, at + 1);
                    let number: String = typo.into_iter().collect();
                    if !BankAccount::mistyped(&number, armenian.as_ref()) {
                        wrong.push(format!("«{number}»: перестановка {} и {} не замечена", at + 1, at + 2));
                    }
                }
            }
        }
        let skipped = [
            ("", None),
            // Короче 12 и длиннее 16 цифр.
            ("12702748287", None),
            ("12702748287812345", None),
            ("40702810900000012345", None),
            // Буквы и чужие цифры.
            ("DE89370400440532013000", None),
            ("AM12702748287800", None),
            ("12702748287A", None),
            ("127027482878/1", None),
            ("١٢٧٠٢٧٤٨٢٨٧٨", None),
            // Банк другой страны.
            ("127027482878", foreign.as_ref()),
            // Необязательные цифры в контрольную не входят (п. 33).
            ("1570036910769999", None),
        ];
        for (number, swift) in skipped {
            if BankAccount::mistyped(number, swift) {
                wrong.push(format!("«{number}» ({swift:?}) проверен, а не должен"));
            }
        }
        if !BankAccount::mistyped("127027482878", armenian.as_ref()) {
            wrong.push("армянский SWIFT выключил проверку".to_owned());
        }
        let countries: Vec<_> = [armenian, foreign, Swift::parse("COBADEFFXXX").ok()]
            .iter()
            .map(|swift| swift.as_ref().map(Swift::country).unwrap_or_default().to_owned())
            .collect();
        if countries != ["AM", "US", "DE"] {
            wrong.push(format!("страны SWIFT: {countries:?}"));
        }
        assert!(wrong.is_empty(), "контрольная цифра: {wrong:#?}");
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
