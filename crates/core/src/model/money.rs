//! Деньги и курсы валют.
//!
//! Сумма — целое число младших единиц валюты (Правило 2, «Деньги — в целых
//! числах»): драм для AMD, цент для USD и EUR. `f64` здесь запрещён: 0,1
//! в нём не представимо, сумма за год расходится с суммой по строкам на
//! драмы, а сравнение с порогом может перевернуться на последнем драме.
//!
//! Курс — тоже целое, с датой и источником. Неизвестный курс — `None`, а не 0
//! и не 1 (Правило 6): с нулём доход в драмах стал бы нулём, и налог
//! занизился бы молча. Перевод в драмы с округлением написан один раз —
//! `Money::to_amd`.
//!
//! В данных: сумма — `{"minor":150000,"currency":"USD"}`, курс —
//! `{"currency":"USD","per_unit_e4":3619300,"date":"2026-10-09",
//! "source":"central_bank"}`. Числа — целые, как в типах: дробная сумма или
//! курс в данных — порча, а не значение.

use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize};

use super::date::Date;

/// Валюта суммы.
///
/// Варианты добавляются (у Грузии будет `GEL`, задача 39), а смысл
/// существующих не меняется (Правило 2). `match` по валюте — без `_`: новую
/// валюту каждое место, где валюта что-то решает, должно увидеть само.
///
/// В данных — код ISO 4217: `"AMD"`, `"USD"`, `"EUR"`. Код — часть формата:
/// имя варианта менять можно, а его — только миграцией (Правило 2).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Currency {
    /// Армянский драм.
    Amd,
    /// Доллар США.
    Usd,
    /// Евро.
    Eur,
}

impl Currency {
    /// Сколько знаков после запятой даёт младшая единица: у доллара и евро —
    /// два (цент), у драма — ноль: младшая единица суммы в драмах — сам драм
    /// (Правило 2).
    pub const fn minor_digits(self) -> u32 {
        match self {
            Currency::Amd => 0,
            Currency::Usd | Currency::Eur => 2,
        }
    }
}

/// Сумма в валюте — целое число младших единиц.
///
/// Сравнения «больше — меньше» у типа нет: суммы разных валют несравнимы,
/// а суммы одной валюты сравниваются по `minor`.
///
/// Чтение выведено, а не написано через конструктор: запретных значений
/// у суммы нет — любое целое в любой валюте — сумма, и `Money::new` ничего
/// не проверяет.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Money {
    minor: i64,
    currency: Currency,
}

impl Money {
    /// Сумма в младших единицах: `Money::new(150_000, Currency::Usd)` —
    /// 1 500,00 $, `Money::new(150_000, Currency::Amd)` — 150 000 ֏.
    pub const fn new(minor: i64, currency: Currency) -> Self {
        Self { minor, currency }
    }

    /// Ноль в валюте.
    pub const fn zero(currency: Currency) -> Self {
        Self::new(0, currency)
    }

    /// Сумма в младших единицах валюты.
    pub const fn minor(self) -> i64 {
        self.minor
    }

    /// Валюта суммы.
    pub const fn currency(self) -> Currency {
        self.currency
    }

    /// Сумма двух сумм одной валюты. Называется `try_`, а не `checked_`:
    /// `checked_*` в стандартной библиотеке возвращают `Option`, а здесь
    /// ошибка говорит, что именно не так.
    pub fn try_add(self, other: Self) -> Result<Self, MoneyError> {
        if self.currency != other.currency {
            return Err(MoneyError::CurrencyMismatch);
        }
        match self.minor.checked_add(other.minor) {
            Some(minor) => Ok(Self::new(minor, self.currency)),
            None => Err(MoneyError::Overflow),
        }
    }

    /// Итог строк одной валюты — ровно до младшей единицы: что человек
    /// сложит по строкам на калькуляторе, то и будет в итоге. Валюта нужна
    /// для пустого списка: его итог — ноль в этой валюте.
    ///
    /// Складывает в `i128` и проверяет только итог: иначе ответ зависел бы
    /// от порядка строк — `MAX + 1 − 1` переполнялся бы, а `MAX − 1 + 1` нет.
    pub fn total(
        currency: Currency,
        items: impl IntoIterator<Item = Self>,
    ) -> Result<Self, MoneyError> {
        let mut sum: i128 = 0;
        for item in items {
            if item.currency != currency {
                return Err(MoneyError::CurrencyMismatch);
            }
            sum = sum
                .checked_add(i128::from(item.minor))
                .ok_or(MoneyError::Overflow)?;
        }
        i64::try_from(sum)
            .map(|minor| Self::new(minor, currency))
            .map_err(|_| MoneyError::Overflow)
    }

    /// Сумма в драмах по курсу — единственное место, где валюта переводится
    /// в драмы и округляется.
    ///
    /// Драмы переводить не нужно, и курс к ним — ошибка `CurrencyMismatch`,
    /// как и курс чужой валюты. Валюта без курса — `RateUnknown`: такую
    /// операцию окно помечает, и в налог она молча не идёт (задача 14).
    ///
    /// Итог в драмах — `Money::total` переведённых строк, а не перевод итога
    /// в валюте: у каждой операции свой курс, а строки, которые человек видит
    /// в книге, обязаны сойтись с итогом.
    pub fn to_amd(self, rate: Option<ExchangeRate>) -> Result<Self, MoneyError> {
        let rate = match (self.currency, rate) {
            (Currency::Amd, None) => return Ok(self),
            (Currency::Amd, Some(_)) => return Err(MoneyError::CurrencyMismatch),
            (Currency::Usd | Currency::Eur, None) => return Err(MoneyError::RateUnknown),
            (Currency::Usd | Currency::Eur, Some(rate)) => rate,
        };
        if rate.currency != self.currency {
            return Err(MoneyError::CurrencyMismatch);
        }
        // В i128: центы, умноженные на курс в десятитысячных, переполняют i64
        // уже на сумме около 25 миллиардов долларов, а частное — нет.
        // Переполнение самих драмов — ошибка, а не тихий перенос через ноль.
        let numerator = i128::from(self.minor) * i128::from(rate.per_unit_e4);
        let denominator = 10_i128.pow(self.currency.minor_digits()) * i128::from(RATE_SCALE);
        let drams = div_round_half_away_from_zero(numerator, denominator);
        i64::try_from(drams)
            .map(|drams| Self::new(drams, Currency::Amd))
            .map_err(|_| MoneyError::Overflow)
    }
}

/// Знаков после запятой в курсе: курс хранится в десятитысячных долях драма
/// за единицу валюты.
///
/// Четыре знака — не с запасом, а впритык: доллар и евро ЦБ публикует
/// с двумя знаками, а рубль — с четырьмя (cba.am, курсы на 2026-10-09;
/// сверено 2026-10-10). Курс ложится в тип без округления: округлённый
/// курс — вторая, незаметная ошибка в драмах.
pub const RATE_DIGITS: u32 = 4;

/// Знаменатель курса: 361,93 ֏ за доллар — это 3 619 300.
pub const RATE_SCALE: i64 = 10_i64.pow(RATE_DIGITS);

/// Откуда взят курс. В данных — `"manual"` и `"central_bank"`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RateSource {
    /// Курс ввёл человек.
    Manual,
    /// Курс Центрального банка Армении на дату курса. Курс какого дня берётся
    /// для операции, решает закон (задачи 14 и 17), а не этот тип.
    CentralBank,
}

/// Курс валюты к драму на дату.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize)]
pub struct ExchangeRate {
    currency: Currency,
    per_unit_e4: i64,
    date: Date,
    source: RateSource,
}

/// Поля `ExchangeRate`, как они лежат в данных, — до проверки.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExchangeRateFields {
    currency: Currency,
    per_unit_e4: i64,
    date: Date,
    source: RateSource,
}

/// Курс читается только через `ExchangeRate::new`: нулевой курс из
/// испорченных данных обнулил бы доход в драмах, и налог занизился бы молча
/// (Правило 6).
impl<'de> Deserialize<'de> for ExchangeRate {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let ExchangeRateFields {
            currency,
            per_unit_e4,
            date,
            source,
        } = ExchangeRateFields::deserialize(deserializer)?;
        Self::new(currency, per_unit_e4, date, source).map_err(|error| {
            de::Error::custom(match error {
                RateError::NotPositive => "курс не больше нуля",
                RateError::Amd => "курс драма к драму",
            })
        })
    }
}

impl ExchangeRate {
    /// Курс `currency` к драму на дату `date`: `per_unit_e4` — драмы за
    /// единицу валюты, умноженные на `RATE_SCALE` (361,93 ֏ — 3 619 300).
    pub const fn new(
        currency: Currency,
        per_unit_e4: i64,
        date: Date,
        source: RateSource,
    ) -> Result<Self, RateError> {
        match currency {
            Currency::Amd => Err(RateError::Amd),
            Currency::Usd | Currency::Eur if per_unit_e4 <= 0 => Err(RateError::NotPositive),
            Currency::Usd | Currency::Eur => Ok(Self {
                currency,
                per_unit_e4,
                date,
                source,
            }),
        }
    }

    /// Валюта, к которой относится курс.
    pub const fn currency(self) -> Currency {
        self.currency
    }

    /// Драмы за единицу валюты, умноженные на `RATE_SCALE`.
    pub const fn per_unit_e4(self) -> i64 {
        self.per_unit_e4
    }

    /// Дата курса.
    pub const fn date(self) -> Date {
        self.date
    }

    /// Откуда курс.
    pub const fn source(self) -> RateSource {
        self.source
    }
}

/// Почему сумму не удалось посчитать.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum MoneyError {
    /// Курса нет — перевести в драмы нечем. Это не ноль (Правило 6).
    RateUnknown,
    /// Валюты не совпали: суммы разных валют не складываются, курс годится
    /// только для своей валюты, а драмам курс не нужен.
    CurrencyMismatch,
    /// Сумма не помещается в `i64` — больше девяти квинтиллионов младших
    /// единиц. Такое бывает только от испорченных данных.
    Overflow,
}

/// Почему курс не принят.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum RateError {
    /// Курс не больше нуля. Такого курса не бывает, а ноль обнулил бы доход
    /// в драмах (Правило 6).
    NotPositive,
    /// Курс драма к драму: драмы переводить не нужно.
    Amd,
}

/// Деление с округлением половины от нуля: 18 096,5 → 18 097,
/// −18 096,5 → −18 097, 3 619,3 → 3 619. Им же округляется сумма строки
/// счёта — цена, умноженная на дробное количество (`model::document`).
///
/// TODO(задача 17): правило округления по закону. До сверки — обычное
/// арифметическое, и тест `rounding_is_half_away_from_zero_until_task_17`
/// держит его: смена правила должна быть решением, а не случайностью.
pub(crate) fn div_round_half_away_from_zero(numerator: i128, denominator: i128) -> i128 {
    debug_assert!(denominator > 0, "знаменатель курса всегда положителен");
    let quotient = numerator / denominator;
    let remainder = numerator % denominator;
    if remainder.unsigned_abs() * 2 >= denominator.unsigned_abs() {
        quotient + numerator.signum()
    } else {
        quotient
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn amd(minor: i64) -> Money {
        Money::new(minor, Currency::Amd)
    }

    fn usd(minor: i64) -> Money {
        Money::new(minor, Currency::Usd)
    }

    fn eur(minor: i64) -> Money {
        Money::new(minor, Currency::Eur)
    }

    fn rate(currency: Currency, per_unit_e4: i64) -> ExchangeRate {
        let date = Date::new(2026, 10, 9).expect("дата курса существует");
        ExchangeRate::new(currency, per_unit_e4, date, RateSource::CentralBank)
            .expect("в тесте — настоящий курс")
    }

    /// Курсы ЦБ на 2026-10-09 (cba.am): 361,93 ֏ за доллар, 405,58 ֏ за евро.
    fn usd_rate() -> ExchangeRate {
        rate(Currency::Usd, 3_619_300)
    }

    fn eur_rate() -> ExchangeRate {
        rate(Currency::Eur, 4_055_800)
    }

    /// Перевод в драмы — числами, которые посчитал человек, а не код.
    #[test]
    fn converts_to_amd() {
        let cases = [
            // 1 000,00 $ × 361,93 = 361 930 ֏ ровно.
            (usd(100_000), usd_rate(), amd(361_930)),
            // 2 500,50 € × 405,58 = 1 014 152,79 ֏ → 1 014 153.
            (eur(250_050), eur_rate(), amd(1_014_153)),
            // 0,01 $ × 361,93 = 3,6193 ֏ → 4.
            (usd(1), usd_rate(), amd(4)),
            // Курс с четырьмя знаками: 1 000,00 $ × 361,9349 = 361 934,9 → 361 935.
            (usd(100_000), rate(Currency::Usd, 3_619_349), amd(361_935)),
        ];
        let mut wrong = Vec::new();
        for (money, rate, expected) in cases {
            let got = money.to_amd(Some(rate));
            if got != Ok(expected) {
                wrong.push(format!("{money:?} по {rate:?}: {got:?}, а не {expected:?}"));
            }
        }
        assert!(wrong.is_empty(), "перевод в драмы ошибся: {wrong:#?}");
    }

    /// TODO(задача 17): правило округления по закону. Пока — половина от нуля.
    /// 50,00 $ × 361,93 = 18 096,5 ֏: здесь 18 097, а банковское округление
    /// (половина к чётному) дало бы 18 096. Тест держит текущее правило,
    /// чтобы его смена была решением, а не случайностью.
    #[test]
    fn rounding_is_half_away_from_zero_until_task_17() {
        let cases = [
            // Ровно половина — от нуля.
            (usd(5_000), amd(18_097)),
            (usd(-5_000), amd(-18_097)),
            // Меньше половины — вниз по модулю: 10,00 $ → 3 619,3 ֏.
            (usd(1_000), amd(3_619)),
            (usd(-1_000), amd(-3_619)),
            // Больше половины — вверх по модулю: 0,01 $ → 3,6193 ֏.
            (usd(1), amd(4)),
            (usd(-1), amd(-4)),
        ];
        let mut wrong = Vec::new();
        for (money, expected) in cases {
            let got = money.to_amd(Some(usd_rate()));
            if got != Ok(expected) {
                wrong.push(format!("{money:?}: {got:?}, а не {expected:?}"));
            }
        }
        assert!(wrong.is_empty(), "округление изменилось: {wrong:#?}");
    }

    /// Неизвестный курс — отдельное состояние, а не ноль и не единица.
    #[test]
    fn unknown_rate_is_not_zero() {
        assert_eq!(usd(10_000).to_amd(None), Err(MoneyError::RateUnknown));
        assert_eq!(eur(10_000).to_amd(None), Err(MoneyError::RateUnknown));
    }

    /// Драмам курс не нужен, а курс годится только для своей валюты.
    #[test]
    fn rate_must_match_currency() {
        assert_eq!(amd(5_000).to_amd(None), Ok(amd(5_000)));
        assert_eq!(
            amd(5_000).to_amd(Some(usd_rate())),
            Err(MoneyError::CurrencyMismatch)
        );
        assert_eq!(
            eur(5_000).to_amd(Some(usd_rate())),
            Err(MoneyError::CurrencyMismatch)
        );
    }

    /// Курс — только положительный и только для валюты, не для драма.
    #[test]
    fn rate_is_positive_and_foreign() {
        let date = Date::new(2026, 10, 9).expect("дата курса существует");
        let manual = RateSource::Manual;
        assert_eq!(
            ExchangeRate::new(Currency::Usd, 0, date, manual),
            Err(RateError::NotPositive)
        );
        assert_eq!(
            ExchangeRate::new(Currency::Eur, -1, date, manual),
            Err(RateError::NotPositive)
        );
        assert_eq!(
            ExchangeRate::new(Currency::Amd, RATE_SCALE, date, manual),
            Err(RateError::Amd)
        );
        let rate = ExchangeRate::new(Currency::Usd, 3_619_300, date, manual)
            .expect("настоящий курс принят");
        assert_eq!(
            (
                rate.currency(),
                rate.per_unit_e4(),
                rate.date(),
                rate.source()
            ),
            (Currency::Usd, 3_619_300, date, manual)
        );
    }

    /// Сумма строк = сумма итога. Десять раз по 0,10 $ — ровно 1,00 $
    /// (в `f64` десять сложений 0,1 дают 0,9999999999999999), а 100,00 $
    /// тремя строками 33,33 + 33,33 + 33,34 — ровно 100,00 $.
    #[test]
    fn lines_add_up_to_the_total_exactly() {
        assert_eq!(Money::total(Currency::Usd, [usd(10); 10]), Ok(usd(100)));
        assert_eq!(
            Money::total(Currency::Usd, [usd(3_333), usd(3_333), usd(3_334)]),
            Ok(usd(10_000))
        );
        assert_eq!(Money::total(Currency::Amd, []), Ok(amd(0)));
        assert_eq!(
            Money::total(Currency::Usd, [usd(100), eur(100)]),
            Err(MoneyError::CurrencyMismatch)
        );
    }

    /// Итог книги в драмах — сумма переведённых строк. Три поступления по
    /// 10,00 $ по 361,93: каждое — 3 619,3 ֏ → 3 619, итог — 10 857 ֏.
    /// Перевод итога в долларах (30,00 $ → 10 857,9 → 10 858) разошёлся бы
    /// со строками, которые человек сложит сам.
    #[test]
    fn amd_total_is_the_sum_of_converted_lines() {
        let lines: Vec<Money> = [usd(1_000); 3]
            .into_iter()
            .map(|line| line.to_amd(Some(usd_rate())).expect("курс есть"))
            .collect();
        assert_eq!(lines, [amd(3_619); 3]);
        assert_eq!(Money::total(Currency::Amd, lines), Ok(amd(10_857)));
        assert_eq!(usd(3_000).to_amd(Some(usd_rate())), Ok(amd(10_858)));
    }

    /// Переполнение — ошибка, а не тихий перенос через ноль.
    #[test]
    fn overflow_is_an_error() {
        assert_eq!(
            usd(i64::MAX).to_amd(Some(usd_rate())),
            Err(MoneyError::Overflow)
        );
        assert_eq!(
            usd(i64::MIN).to_amd(Some(usd_rate())),
            Err(MoneyError::Overflow)
        );
        assert_eq!(
            Money::total(Currency::Amd, [amd(i64::MAX), amd(1)]),
            Err(MoneyError::Overflow)
        );
        assert_eq!(amd(i64::MAX).try_add(amd(1)), Err(MoneyError::Overflow));
    }

    /// Ошибкой бывает только то, что не помещается в ответ. 30 млрд $
    /// × 361,93 = 10 857 900 000 000 ֏: центы на курс в десятитысячных
    /// переполняют `i64`, а драмы помещаются. И итог не зависит от порядка
    /// строк: `MAX + 1 − 1` — это `MAX`, как и `MAX − 1 + 1`.
    #[test]
    fn overflow_only_when_the_answer_does_not_fit() {
        assert_eq!(
            usd(3_000_000_000_000).to_amd(Some(usd_rate())),
            Ok(amd(10_857_900_000_000))
        );
        assert_eq!(
            Money::total(Currency::Amd, [amd(i64::MAX), amd(1), amd(-1)]),
            Ok(amd(i64::MAX))
        );
        assert_eq!(
            Money::total(Currency::Amd, [amd(i64::MAX), amd(-1), amd(1)]),
            Ok(amd(i64::MAX))
        );
    }

    /// Суммы разных валют не складываются и попарно.
    #[test]
    fn try_add_needs_one_currency() {
        assert_eq!(usd(100).try_add(usd(250)), Ok(usd(350)));
        assert_eq!(
            usd(100).try_add(eur(100)),
            Err(MoneyError::CurrencyMismatch)
        );
    }

    /// Запись в данных выписана человеком: что прочитано, то и записывается
    /// обратно, слово в слово. Поменялась запись — это новая версия формата
    /// (Правило 5), а не правка теста.
    #[test]
    fn data_shape_is_pinned() {
        let mut wrong = Vec::new();
        let mut check = |sample: &str, back: Result<String, serde_json::Error>| match back {
            Ok(back) if back == sample => {}
            other => wrong.push(format!("{sample} → {other:?}")),
        };
        for sample in [
            r#"{"minor":150000,"currency":"USD"}"#,
            r#"{"minor":-5,"currency":"EUR"}"#,
            r#"{"minor":9223372036854775807,"currency":"AMD"}"#,
        ] {
            let back = serde_json::from_str::<Money>(sample).map(|sum| serde_json::to_string(&sum));
            check(sample, back.and_then(|text| text));
        }
        for sample in [
            r#"{"currency":"USD","per_unit_e4":3619300,"date":"2026-10-09","source":"central_bank"}"#,
            r#"{"currency":"EUR","per_unit_e4":1,"date":"2026-01-01","source":"manual"}"#,
        ] {
            let back = serde_json::from_str::<ExchangeRate>(sample)
                .map(|rate| serde_json::to_string(&rate));
            check(sample, back.and_then(|text| text));
        }
        assert!(
            wrong.is_empty(),
            "запись сумм и курсов изменилась: {wrong:#?}"
        );
        assert_eq!(
            serde_json::from_str::<Money>(r#"{"minor":150000,"currency":"USD"}"#).ok(),
            Some(usd(150_000))
        );
        assert_eq!(
            serde_json::from_str::<ExchangeRate>(
                r#"{"currency":"USD","per_unit_e4":3619300,"date":"2026-10-09","source":"central_bank"}"#
            )
            .ok(),
            Some(usd_rate())
        );
    }

    /// Курс из данных проходит те же проверки, что у конструктора: нулевой
    /// курс обнулил бы доход в драмах (Правило 6). И нигде — дробей, чужих
    /// валют и лишних полей.
    #[test]
    fn data_rejects_what_the_types_forbid() {
        let money = [
            r#"{"minor":1.5,"currency":"USD"}"#,
            r#"{"minor":"150","currency":"USD"}"#,
            r#"{"minor":9223372036854775808,"currency":"USD"}"#,
            r#"{"minor":150,"currency":"usd"}"#,
            r#"{"minor":150,"currency":"RUB"}"#,
            r#"{"minor":150}"#,
            r#"{"minor":150,"currency":"USD","note":"x"}"#,
        ];
        let rates = [
            r#"{"currency":"USD","per_unit_e4":0,"date":"2026-10-09","source":"central_bank"}"#,
            r#"{"currency":"USD","per_unit_e4":-3619300,"date":"2026-10-09","source":"manual"}"#,
            r#"{"currency":"AMD","per_unit_e4":10000,"date":"2026-10-09","source":"manual"}"#,
            r#"{"currency":"USD","per_unit_e4":361.93,"date":"2026-10-09","source":"manual"}"#,
            r#"{"currency":"USD","per_unit_e4":3619300,"date":"2026-00-09","source":"manual"}"#,
            r#"{"currency":"USD","per_unit_e4":3619300,"date":"2026-10-09","source":"bank"}"#,
            r#"{"currency":"USD","per_unit_e4":3619300,"date":"2026-10-09"}"#,
            r#"{"currency":"USD","per_unit_e4":3619300,"date":"2026-10-09","source":"manual","x":1}"#,
        ];
        let mut wrong = Vec::new();
        for text in money {
            if let Ok(sum) = serde_json::from_str::<Money>(text) {
                wrong.push(format!("{text} прочитано как {sum:?}"));
            }
        }
        for text in rates {
            if let Ok(rate) = serde_json::from_str::<ExchangeRate>(text) {
                wrong.push(format!("{text} прочитано как {rate:?}"));
            }
        }
        assert!(wrong.is_empty(), "прочитано запретное: {wrong:#?}");
    }
}
