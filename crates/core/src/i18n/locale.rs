//! Запись и разбор чисел, сумм, курсов и дат по правилам языка.
//!
//! Человеку сумму и дату показывает только эта запись: у `Money` и `Date`
//! нарочно нет `Display`. Окно и PDF зовут одни и те же функции, поэтому
//! в превью и в документе сумма записана одинаково (Правило 2, «Что в превью,
//! то и в PDF»).
//!
//! # Откуда правила записи
//!
//! Из CLDR — той же базы, по которой пишет числа `Intl` в браузере (по
//! браузеру: CLDR 48 через ICU 78.3 в Node 24.21, сверено 2026-10-11). Своя
//! запись, а не `Intl.NumberFormat`: ядро работает без браузера, и PDF
//! собирается тем же кодом. Где запись отступает от CLDR или выбрана
//! человеком, сказано у функции.
//!
//! | | армянский | русский | английский |
//! |---|---|---|---|
//! | число | `1 234 567,5`, но `1234` | `1 234 567,5` | `1,234,567.5` |
//! | драмы | `1 234 567 ֏` | `1 234 567 ֏` | `AMD 1,234,567` |
//! | доллары | `1234,50 $` | `1 234,50 $` | `$1,234.50` |
//! | дата | `11.10.2026` | `11.10.2026` | `Oct 11, 2026` |
//!
//! Пробел между разрядами и перед знаком валюты — неразрывный (U+00A0), как
//! в CLDR: перенос строки не разорвёт сумму. Минус — дефис-минус (U+002D),
//! как в CLDR у всех трёх языков.

use super::Lang;
use crate::model::date::Date;
use crate::model::money::{Currency, Money, RATE_DIGITS};

/// Неразрывный пробел: между разрядами в армянском и русском и перед знаком
/// валюты.
const NBSP: char = '\u{A0}';

/// Знак между разрядами.
const fn group_mark(lang: Lang) -> char {
    match lang {
        Lang::Am | Lang::Ru => NBSP,
        Lang::En => ',',
    }
}

/// Знак перед дробной частью.
const fn decimal_mark(lang: Lang) -> char {
    match lang {
        Lang::Am | Lang::Ru => ',',
        Lang::En => '.',
    }
}

/// С какой длины целую часть делят на разряды. В армянском — с пяти цифр:
/// `5000 ֏`, но `12 345 ֏` (CLDR, `minimumGroupingDigits` = 2); в русском
/// и английском — с четырёх: `5 000 ֏`, `$5,000.00`.
const fn min_grouped_len(lang: Lang) -> usize {
    match lang {
        Lang::Am => 5,
        Lang::Ru | Lang::En => 4,
    }
}

/// Целое число: `1 234 567`, `1,234,567`; по-армянски `1234`.
pub fn integer(lang: Lang, n: i64) -> String {
    let mut out = String::new();
    push_signed(&mut out, lang, n, 0);
    out
}

/// Сумма со знаком валюты: `1 234 567 ֏`, `1 234,50 $`, `$1,234.50`,
/// `AMD 1,234,567`. Знаков после запятой — сколько у валюты: у драма ни
/// одного, у доллара и евро два.
///
/// В армянском и русском знак валюты — узкий из CLDR (`֏`, `$`, `€`) после
/// числа. В английском — обычный из CLDR перед числом: `$`, `€` и `AMD`,
/// а не `֏`: знак драма иностранный клиент может не узнать (решение человека
/// 2026-10-11).
pub fn money(lang: Lang, sum: Money) -> String {
    let digits = sum.currency().minor_digits();
    let mut out = String::new();
    match lang {
        Lang::Am | Lang::Ru => {
            push_signed(&mut out, lang, sum.minor(), digits);
            out.push(NBSP);
            out.push_str(match sum.currency() {
                Currency::Amd => "֏",
                Currency::Usd => "$",
                Currency::Eur => "€",
            });
        }
        Lang::En => {
            // Минус — перед знаком валюты: `-$5.00`, как в CLDR.
            if sum.minor() < 0 {
                out.push('-');
            }
            out.push_str(match sum.currency() {
                Currency::Amd => "AMD\u{A0}",
                Currency::Usd => "$",
                Currency::Eur => "€",
            });
            push_unsigned(&mut out, lang, sum.minor().unsigned_abs(), digits);
        }
    }
    out
}

/// Сумма без знака валюты — для поля ввода и для столбца, у которого валюта
/// в заголовке: `1 234,50`, `1,234.50`. Что записала она, то `parse_money`
/// и прочтёт.
pub fn amount(lang: Lang, sum: Money) -> String {
    let mut out = String::new();
    push_signed(&mut out, lang, sum.minor(), sum.currency().minor_digits());
    out
}

/// Курс — драмы за единицу валюты в десятитысячных (`RATE_SCALE`): `361,93`,
/// `361.93`. Нули справа после второго знака не пишутся: ЦБ публикует доллар
/// и евро с двумя знаками, и `361,9300` выглядел бы другим курсом. Что
/// записала она, то `parse_rate` и прочтёт.
pub fn rate(lang: Lang, per_unit_e4: i64) -> String {
    let mut scaled = per_unit_e4;
    let mut digits = RATE_DIGITS;
    while digits > 2 && scaled % 10 == 0 {
        scaled /= 10;
        digits -= 1;
    }
    let mut out = String::new();
    push_signed(&mut out, lang, scaled, digits);
    out
}

/// Десятичное число без нулей в конце дробной части: количество в строке
/// счёта — `37,5`, `1`, `0,125`. `scaled` — значение, умноженное на
/// `10^digits`. Что записала она, то `parse_decimal` с теми же `digits`
/// и прочтёт.
pub fn decimal(lang: Lang, scaled: i64, digits: u32) -> String {
    let (mut scaled, mut digits) = (scaled, digits);
    while digits > 0 && scaled % 10 == 0 {
        scaled /= 10;
        digits -= 1;
    }
    let mut out = String::new();
    push_signed(&mut out, lang, scaled, digits);
    out
}

/// Дата: `11.10.2026` в армянском и русском, `Oct 11, 2026` в английском.
///
/// Русская запись — короткая из CLDR (`dd.MM.y`). Армянская — тоже, но год
/// в четыре цифры: короткая армянская в CLDR — `11.10.26`, а двузначный год
/// в счёте и акте неоднозначен. Английская — средняя из CLDR (`MMM d, y`):
/// короткая `10/11/26` ставит месяц первым, и европеец прочтёт её как
/// 10 ноября (решение человека 2026-10-11).
pub fn date(lang: Lang, day: Date) -> String {
    match lang {
        Lang::Am | Lang::Ru => format!("{:02}.{:02}.{}", day.day(), day.month(), day.year()),
        Lang::En => format!(
            "{} {}, {}",
            EN_MONTHS[usize::from(day.month() - 1)],
            day.day(),
            day.year()
        ),
    }
}

/// Месяцы английской записи даты — сокращения CLDR `en`: `Sep`, а не `Sept`.
/// Только английские: армянская и русская даты пишутся цифрами.
const EN_MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Число со знаком: `scaled` — значение, умноженное на `10^digits`.
fn push_signed(out: &mut String, lang: Lang, scaled: i64, digits: u32) {
    if scaled < 0 {
        out.push('-');
    }
    push_unsigned(out, lang, scaled.unsigned_abs(), digits);
}

/// Число без знака: целая часть по разрядам, дробная — ровно `digits` цифр.
fn push_unsigned(out: &mut String, lang: Lang, scaled: u64, digits: u32) {
    let scale = 10_u64.pow(digits);
    let whole = (scaled / scale).to_string();
    let grouped = whole.len() >= min_grouped_len(lang);
    // Цифры ASCII: номер знака совпадает с номером байта.
    for (i, digit) in whole.chars().enumerate() {
        if grouped && i > 0 && (whole.len() - i).is_multiple_of(3) {
            out.push(group_mark(lang));
        }
        out.push(digit);
    }
    if digits > 0 {
        out.push(decimal_mark(lang));
        let fraction = (scaled % scale).to_string();
        for _ in fraction.len()..digits as usize {
            out.push('0');
        }
        out.push_str(&fraction);
    }
}

/// Почему введённое число не прочитано. Окно говорит об этом словами
/// (задача 11), а пример верной записи берёт у `amount` того же языка.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ParseError {
    /// Ни одной цифры: пустое поле, один минус или одна запятая.
    Empty,
    /// Знак, которого в числе быть не может: буква, знак валюты, второй
    /// минус, плюс.
    Invalid,
    /// Разделители не на месте: разряды не по три (`12 34`, `1,23,456`) или
    /// два знака дробной части (`1.234,56` в русском окне).
    Separators,
    /// Знаков после запятой больше, чем бывает: у доллара — два, у драма —
    /// ни одного, у курса — четыре.
    TooPrecise { max: u32 },
    /// Число не помещается в сумму — больше девяти квинтиллионов младших
    /// единиц.
    TooLarge,
}

/// Сумма, введённая человеком, — по правилам языка окна (Правило 6, «Ввод
/// суммы не угадывается»): `1,234` в английском окне — тысяча двести
/// тридцать четыре, в русском — одна целая и двести тридцать четыре
/// тысячных, и для доллара это уже ошибка `TooPrecise`. Прочитанное окно
/// сразу показывает записью `money`: человек видит, что понято.
///
/// Знака валюты в записи быть не должно (`Invalid`): валюту выбирают рядом
/// с полем, а `€100` в поле долларов — ошибка, которую молча не исправить.
pub fn parse_money(lang: Lang, input: &str, currency: Currency) -> Result<Money, ParseError> {
    parse_scaled(lang, input, currency.minor_digits()).map(|minor| Money::new(minor, currency))
}

/// Курс, введённый человеком, — в десятитысячных драма (`RATE_SCALE`):
/// `361,93` → 3 619 300. Правила — те же, что у `parse_money`. Положителен ли
/// курс, решает `ExchangeRate::new`.
pub fn parse_rate(lang: Lang, input: &str) -> Result<i64, ParseError> {
    parse_scaled(lang, input, RATE_DIGITS)
}

/// Десятичное число, введённое человеком, — количество в строке счёта:
/// `37,5` при трёх знаках → 37 500. Правила — те же, что у `parse_money`.
pub fn parse_decimal(lang: Lang, input: &str, digits: u32) -> Result<i64, ParseError> {
    parse_scaled(lang, input, digits)
}

/// Разбирает число с `digits` знаками после запятой в целое, умноженное на
/// `10^digits`.
///
/// - Вокруг числа — любые пробелы; впереди может стоять минус: `-` или `−`
///   (U+2212).
/// - Разряды разделяют пробелы (обычный, неразрывные U+00A0 и U+202F, тонкий
///   U+2009), а в английском ещё и запятая — строго по три цифры: `12 34`
///   и `1,23,456` — ошибка, а не 1234 и 123456. Без разделителей тоже можно:
///   `1234567`.
/// - Дробную часть в армянском и русском отделяет запятая **или точка**,
///   в английском — точка, а запятая — только если она одна и за ней не три
///   цифры: `12,50`. Причина — телефон: десятичная клавиатура iPhone берёт
///   разделитель у региона и раскладки устройства, а не у `lang` страницы
///   (по браузеру: WebKit bug 247242, сверено 2026-10-11), и в русском окне
///   на телефоне с американским регионом запятой просто нет. Двусмысленного
///   это не делает: точка в русской записи ничего другого не значит, а
///   английская запятая с тремя цифрами после неё — по-прежнему разряды.
/// - Знаков после запятой — не больше `digits`, даже нулей: `1,000` в русском
///   окне для драма — ошибка, а не один драм. Иначе тысяча, написанная
///   по-английски, стала бы драмом, и никто бы этого не заметил.
fn parse_scaled(lang: Lang, input: &str, digits: u32) -> Result<i64, ParseError> {
    let text = input.trim();
    let (negative, body) = match text.strip_prefix(['-', '\u{2212}']) {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let allowed = |c: char| c.is_ascii_digit() || c == ',' || c == '.' || is_space_mark(c);
    if !body.chars().all(allowed) {
        return Err(ParseError::Invalid);
    }
    let (whole, fraction) = split_fraction(lang, body)?;
    if !fraction.chars().all(|c| c.is_ascii_digit()) {
        return Err(ParseError::Separators);
    }
    if fraction.is_empty() && !whole.chars().any(|c| c.is_ascii_digit()) {
        return Err(ParseError::Empty);
    }
    check_groups(lang, whole)?;
    // В дробной части только цифры ASCII: байтов в ней столько же, сколько
    // знаков. Длиннее `u32::MAX` строка из поля ввода не бывает.
    let fraction_len = u32::try_from(fraction.len()).unwrap_or(u32::MAX);
    if fraction_len > digits {
        return Err(ParseError::TooPrecise { max: digits });
    }
    // В `i128` и с проверкой: длинная строка цифр — ошибка, а не перенос
    // через ноль. Знак — после: модуль `i64::MIN` в `i64` не помещается.
    let mut value: i128 = 0;
    for digit in whole
        .bytes()
        .chain(fraction.bytes())
        .filter(u8::is_ascii_digit)
    {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add(i128::from(digit - b'0')))
            .ok_or(ParseError::TooLarge)?;
    }
    value = value
        .checked_mul(10_i128.pow(digits - fraction_len))
        .ok_or(ParseError::TooLarge)?;
    if negative {
        value = -value;
    }
    i64::try_from(value).map_err(|_| ParseError::TooLarge)
}

/// Пробелы, которыми разделяют разряды: обычный, неразрывный (так пишет
/// CLDR), узкий неразрывный и тонкий (так пишут типографы и копируется
/// из PDF).
const fn is_space_mark(c: char) -> bool {
    matches!(c, ' ' | '\u{A0}' | '\u{202F}' | '\u{2009}')
}

/// Делит запись на целую и дробную части по знаку дробной части языка. Два
/// таких знака — `Separators`.
fn split_fraction(lang: Lang, body: &str) -> Result<(&str, &str), ParseError> {
    let is_mark = |c: char| match lang {
        Lang::Am | Lang::Ru => c == ',' || c == '.',
        Lang::En => c == '.',
    };
    let mut marks = body.match_indices(is_mark).map(|(at, _)| at);
    let first = marks.next();
    if marks.next().is_some() {
        return Err(ParseError::Separators);
    }
    let mark = match lang {
        Lang::Am | Lang::Ru => first,
        Lang::En => first.or_else(|| lone_decimal_comma(body)),
    };
    // Запятая и точка — по байту, и срез по ним не режет знак пополам.
    Ok(match mark {
        Some(at) => (&body[..at], &body[at + 1..]),
        None => (body, ""),
    })
}

/// Запятая в английской записи отделяет дробную часть, только если она одна
/// и за ней не ровно три цифры: `12,50` разрядами быть не может, а `1,234` —
/// разряды, как в английском и положено.
fn lone_decimal_comma(body: &str) -> Option<usize> {
    let at = body.find(',')?;
    let rest = &body[at + 1..];
    let digits_only = rest.bytes().all(|b| b.is_ascii_digit());
    (digits_only && rest.len() != 3).then_some(at)
}

/// Разряды целой части: первая группа — от одной до трёх цифр, остальные —
/// ровно по три, без пустых групп. `12 34`, `1,23,456`, `1  000`, `,500`
/// и `- 5` — ошибки.
fn check_groups(lang: Lang, whole: &str) -> Result<(), ParseError> {
    let is_group_mark = |c: char| is_space_mark(c) || (comma_groups(lang) && c == ',');
    let mut groups = whole.split(is_group_mark);
    let first = groups.next().unwrap_or_default();
    let mut separated = false;
    for group in groups {
        separated = true;
        if group.len() != 3 {
            return Err(ParseError::Separators);
        }
    }
    if separated && !(1..=3).contains(&first.len()) {
        return Err(ParseError::Separators);
    }
    Ok(())
}

/// Делит ли запятая разряды. В армянском и русском запятая — знак дробной
/// части, и до `check_groups` она не доходит.
const fn comma_groups(lang: Lang) -> bool {
    match lang {
        Lang::Am | Lang::Ru => false,
        Lang::En => true,
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

    fn day(year: u16, month: u8, day: u8) -> Date {
        Date::new(year, month, day).expect("в тесте — существующая дата")
    }

    /// Ожидаемое пишется обычными пробелами, а сравнивается с неразрывными:
    /// иначе каждое `\u{a0}` в таблице случаев мешало бы её читать. Что
    /// пробел и правда неразрывный, держит `spaces_are_non_breaking`.
    fn nb(text: &str) -> String {
        text.replace(' ', "\u{A0}")
    }

    /// Неразрывный пробел — между разрядами и перед знаком валюты: обычный
    /// разорвал бы сумму переносом строки — `1` в конце одной строки, `000 ֏`
    /// в начале другой.
    #[test]
    fn spaces_are_non_breaking() {
        assert_eq!(integer(Lang::Ru, 1_000), "1\u{A0}000");
        assert_eq!(money(Lang::Am, amd(5)), "5\u{A0}֏");
        assert_eq!(money(Lang::En, amd(5)), "AMD\u{A0}5");
    }

    /// Целые числа — по разрядам языка. Армянский делит только с пяти цифр
    /// (CLDR): `1234`, но `12 345`.
    #[test]
    fn integers_follow_each_language() {
        let cases = [
            (Lang::Ru, 0, "0"),
            (Lang::Ru, 999, "999"),
            (Lang::Ru, 1_000, "1 000"),
            (Lang::Ru, 1_234_567, "1 234 567"),
            (Lang::Ru, -1_234_567, "-1 234 567"),
            (Lang::Ru, i64::MIN, "-9 223 372 036 854 775 808"),
            (Lang::Am, 1_234, "1234"),
            (Lang::Am, -5_000, "-5000"),
            (Lang::Am, 12_345, "12 345"),
            (Lang::Am, 1_234_567, "1 234 567"),
            (Lang::En, 999, "999"),
            (Lang::En, 1_234, "1,234"),
            (Lang::En, -1_234_567, "-1,234,567"),
            (Lang::En, i64::MAX, "9,223,372,036,854,775,807"),
        ];
        let mut wrong = Vec::new();
        for (lang, n, expected) in cases {
            let got = integer(lang, n);
            if got != nb(expected) {
                wrong.push(format!("{lang:?} {n}: «{got}», а не «{expected}»"));
            }
        }
        assert!(wrong.is_empty(), "числа записаны не так: {wrong:#?}");
    }

    /// Суммы — по языку и валюте; ожидаемое сверено с `Intl.NumberFormat`
    /// (CLDR 48, 2026-10-11), английские драмы — решение человека.
    #[test]
    fn money_follows_each_language() {
        let cases = [
            (Lang::Ru, amd(1_234_567), "1 234 567 ֏"),
            (Lang::Ru, amd(0), "0 ֏"),
            (Lang::Ru, usd(123_450), "1 234,50 $"),
            (Lang::Ru, usd(-500), "-5,00 $"),
            (Lang::Ru, eur(5), "0,05 €"),
            (Lang::Am, amd(5_000), "5000 ֏"),
            (Lang::Am, amd(12_345), "12 345 ֏"),
            (Lang::Am, usd(123_450), "1234,50 $"),
            (Lang::Am, eur(-500), "-5,00 €"),
            (Lang::En, amd(1_234_567), "AMD 1,234,567"),
            (Lang::En, amd(-5), "-AMD 5"),
            (Lang::En, usd(123_450), "$1,234.50"),
            (Lang::En, usd(-500), "-$5.00"),
            (Lang::En, usd(0), "$0.00"),
            (Lang::En, eur(123_456_789), "€1,234,567.89"),
        ];
        let mut wrong = Vec::new();
        for (lang, sum, expected) in cases {
            let got = money(lang, sum);
            if got != nb(expected) {
                wrong.push(format!("{lang:?} {sum:?}: «{got}», а не «{expected}»"));
            }
        }
        assert!(wrong.is_empty(), "суммы записаны не так: {wrong:#?}");
    }

    /// Без знака валюты — то же число, что в `money`.
    #[test]
    fn amounts_have_no_currency_sign() {
        let cases = [
            (Lang::Ru, usd(123_450), "1 234,50"),
            (Lang::Am, amd(1_234), "1234"),
            (Lang::En, usd(123_450), "1,234.50"),
            (Lang::En, amd(-1_234), "-1,234"),
        ];
        let mut wrong = Vec::new();
        for (lang, sum, expected) in cases {
            let got = amount(lang, sum);
            if got != nb(expected) {
                wrong.push(format!("{lang:?} {sum:?}: «{got}», а не «{expected}»"));
            }
        }
        assert!(
            wrong.is_empty(),
            "суммы без знака записаны не так: {wrong:#?}"
        );
    }

    /// Курс — от двух до четырёх знаков: нули справа после второго не
    /// пишутся.
    #[test]
    fn rates_keep_two_to_four_digits() {
        let cases = [
            (Lang::Ru, 3_619_300, "361,93"),
            (Lang::Ru, 3_619_340, "361,934"),
            (Lang::Ru, 3_619_349, "361,9349"),
            (Lang::Ru, 4_000_000, "400,00"),
            (Lang::Ru, 1, "0,0001"),
            (Lang::Ru, 0, "0,00"),
            (Lang::En, 3_619_300, "361.93"),
            (Lang::Am, 12_345_678_900, "1 234 567,89"),
        ];
        let mut wrong = Vec::new();
        for (lang, per_unit_e4, expected) in cases {
            let got = rate(lang, per_unit_e4);
            if got != nb(expected) {
                wrong.push(format!(
                    "{lang:?} {per_unit_e4}: «{got}», а не «{expected}»"
                ));
            }
        }
        assert!(wrong.is_empty(), "курсы записаны не так: {wrong:#?}");
    }

    /// Количество — без нулей в конце дробной части, разряды — по языку.
    #[test]
    fn decimals_drop_trailing_zeros() {
        let cases = [
            (Lang::Ru, 37_500, "37,5"),
            (Lang::Ru, 1_000, "1"),
            (Lang::Ru, 125, "0,125"),
            (Lang::Ru, 0, "0"),
            (Lang::Ru, -1_500, "-1,5"),
            (Lang::Ru, 1_234_000, "1 234"),
            (Lang::Am, 1_234_000, "1234"),
            (Lang::Am, 12_345_250, "12 345,25"),
            (Lang::En, 1_234_500, "1,234.5"),
            (Lang::En, 1_234, "1.234"),
        ];
        let mut wrong = Vec::new();
        for (lang, scaled, expected) in cases {
            let got = decimal(lang, scaled, 3);
            if got != nb(expected) {
                wrong.push(format!("{lang:?} {scaled}: «{got}», а не «{expected}»"));
            }
        }
        assert!(wrong.is_empty(), "количество записано не так: {wrong:#?}");
    }

    /// Дата — цифрами в армянском и русском, месяцем словом в английском.
    /// Все двенадцать английских месяцев выписаны по `Intl.DateTimeFormat`
    /// (CLDR 48, 2026-10-11).
    #[test]
    fn dates_follow_each_language() {
        let mut cases = vec![
            (Lang::Ru, day(2026, 10, 11), "11.10.2026"),
            (Lang::Ru, day(2026, 1, 5), "05.01.2026"),
            (Lang::Am, day(2026, 10, 11), "11.10.2026"),
            (Lang::Am, day(2026, 1, 5), "05.01.2026"),
            (Lang::En, day(2026, 10, 11), "Oct 11, 2026"),
        ];
        let english = [
            "Jan 5, 2026",
            "Feb 5, 2026",
            "Mar 5, 2026",
            "Apr 5, 2026",
            "May 5, 2026",
            "Jun 5, 2026",
            "Jul 5, 2026",
            "Aug 5, 2026",
            "Sep 5, 2026",
            "Oct 5, 2026",
            "Nov 5, 2026",
            "Dec 5, 2026",
        ];
        for (month, expected) in (1..=12).zip(english) {
            cases.push((Lang::En, day(2026, month, 5), expected));
        }
        let mut wrong = Vec::new();
        for (lang, d, expected) in cases {
            let got = date(lang, d);
            if got != expected {
                wrong.push(format!("{lang:?} {d:?}: «{got}», а не «{expected}»"));
            }
        }
        assert!(wrong.is_empty(), "даты записаны не так: {wrong:#?}");
    }

    /// Одна и та же запись `1,234` читается по языку окна (Правило 6):
    /// в английском — тысяча двести тридцать четыре, в русском и армянском —
    /// одна целая с тысячными, и ни драмам, ни долларам это не годится.
    #[test]
    fn one_comma_means_what_the_window_language_says() {
        assert_eq!(
            parse_money(Lang::En, "1,234", Currency::Amd),
            Ok(amd(1_234))
        );
        assert_eq!(
            parse_money(Lang::En, "1,234", Currency::Usd),
            Ok(usd(123_400))
        );
        for lang in [Lang::Ru, Lang::Am] {
            assert_eq!(
                parse_money(lang, "1,234", Currency::Amd),
                Err(ParseError::TooPrecise { max: 0 })
            );
            assert_eq!(
                parse_money(lang, "1,234", Currency::Usd),
                Err(ParseError::TooPrecise { max: 2 })
            );
        }
    }

    /// Что читается: разряды любыми пробелами, запятая и точка там, где они
    /// не двусмысленны, минус обоих видов.
    #[test]
    fn parse_reads_each_language() {
        let cases = [
            (Lang::Ru, "1 234,56", 2, 123_456),
            (Lang::Ru, "1234,56", 2, 123_456),
            (Lang::Ru, "1234.56", 2, 123_456),
            (Lang::Ru, "1\u{A0}234,5", 2, 123_450),
            (Lang::Ru, "1\u{202F}234", 2, 123_400),
            (Lang::Ru, "1\u{2009}234", 2, 123_400),
            (Lang::Ru, "  12  ", 2, 1_200),
            (Lang::Ru, "-5,5", 2, -550),
            (Lang::Ru, "\u{2212}5", 2, -500),
            (Lang::Ru, ",5", 2, 50),
            (Lang::Ru, "5,", 2, 500),
            (Lang::Ru, "-0", 2, 0),
            (Lang::Am, "1 234 567", 0, 1_234_567),
            (Lang::Am, "5000", 0, 5_000),
            (Lang::Am, "5 000", 0, 5_000),
            (Lang::Am, "361.93", 4, 3_619_300),
            (Lang::En, "1,234.56", 2, 123_456),
            (Lang::En, "1,234", 2, 123_400),
            (Lang::En, "1 234.56", 2, 123_456),
            (Lang::En, "1,234,567.89", 2, 123_456_789),
            (Lang::En, "12,50", 2, 1_250),
            (Lang::En, "12,5", 2, 1_250),
            (Lang::En, ".5", 2, 50),
            (Lang::En, "361,9349", 4, 3_619_349),
            (Lang::En, "361.93", 4, 3_619_300),
            (Lang::En, "9,223,372,036,854,775,807", 0, i64::MAX),
            (Lang::Ru, "-9 223 372 036 854 775 808", 0, i64::MIN),
        ];
        let mut wrong = Vec::new();
        for (lang, input, digits, expected) in cases {
            let got = parse_scaled(lang, input, digits);
            if got != Ok(expected) {
                wrong.push(format!(
                    "{lang:?} «{input}» ({digits}): {got:?}, а не {expected}"
                ));
            }
        }
        assert!(wrong.is_empty(), "не прочитано: {wrong:#?}");
    }

    /// Что не читается — и почему. Каждая ошибка здесь — запись, которую
    /// можно было бы «угадать» и угадать неверно.
    #[test]
    fn parse_rejects_what_it_cannot_read_for_sure() {
        use ParseError::{Empty, Invalid, Separators, TooLarge, TooPrecise};
        let cases = [
            (Lang::Ru, "", 2, Empty),
            (Lang::Ru, "   ", 2, Empty),
            (Lang::Ru, "-", 2, Empty),
            (Lang::Ru, ",", 2, Empty),
            (Lang::En, ".", 2, Empty),
            (Lang::En, ",", 2, Empty),
            (Lang::Ru, "abc", 2, Invalid),
            (Lang::Ru, "$5", 2, Invalid),
            (Lang::Ru, "5 ֏", 0, Invalid),
            (Lang::Ru, "--5", 2, Invalid),
            (Lang::Ru, "5-", 2, Invalid),
            (Lang::Ru, "+5", 2, Invalid),
            (Lang::En, "1e3", 2, Invalid),
            (Lang::Ru, "1.234,56", 2, Separators),
            (Lang::Ru, "1,2,3", 2, Separators),
            (Lang::Ru, "12 34", 2, Separators),
            (Lang::Ru, "1 2345", 2, Separators),
            (Lang::Ru, "1234 567", 2, Separators),
            (Lang::Ru, "1  000", 2, Separators),
            (Lang::Ru, "1 000,50 0", 2, Separators),
            (Lang::Ru, "- 5", 2, Separators),
            (Lang::En, "1,23,456", 2, Separators),
            (Lang::En, "1,234,5", 2, Separators),
            (Lang::En, "1.234,56", 2, Separators),
            (Lang::En, "1.2.3", 2, Separators),
            (Lang::En, ",500", 2, Separators),
            (Lang::Ru, "1000,00", 0, TooPrecise { max: 0 }),
            (Lang::Ru, "361,93491", 4, TooPrecise { max: 4 }),
            (Lang::En, "0.001", 2, TooPrecise { max: 2 }),
            (Lang::En, "1.5", 0, TooPrecise { max: 0 }),
            (Lang::Ru, "92 233 720 368 547 758,08", 2, TooLarge),
            (Lang::En, "9,223,372,036,854,775,808", 0, TooLarge),
            (Lang::Ru, "-9 223 372 036 854 775 809", 0, TooLarge),
        ];
        let mut wrong = Vec::new();
        for (lang, input, digits, expected) in cases {
            let got = parse_scaled(lang, input, digits);
            if got != Err(expected) {
                wrong.push(format!(
                    "{lang:?} «{input}» ({digits}): {got:?}, а не {expected:?}"
                ));
            }
        }
        let endless = format!("1{}", "0".repeat(50));
        if parse_scaled(Lang::Ru, &endless, 0) != Err(TooLarge) {
            wrong.push("пятьдесят нулей прочитаны".to_owned());
        }
        assert!(
            wrong.is_empty(),
            "прочитано то, что читать нельзя: {wrong:#?}"
        );
    }

    /// Что записано, то и читается: сумма, показанная в поле ввода, после
    /// правки человеком читается обратно в ту же сумму — на каждом языке,
    /// в каждой валюте, на краях `i64`. Так же и курс.
    #[test]
    fn what_is_written_reads_back() {
        let values = [
            0,
            1,
            5,
            999,
            1_000,
            1_234,
            12_345,
            123_456_789,
            -1_234_567,
            i64::MAX,
            i64::MIN,
        ];
        let rates = [
            1,
            3_619_300,
            3_619_340,
            3_619_349,
            4_000_000,
            12_345_678_900,
        ];
        let mut lost = Vec::new();
        for lang in Lang::ALL {
            for currency in [Currency::Amd, Currency::Usd, Currency::Eur] {
                for minor in values {
                    let sum = Money::new(minor, currency);
                    let text = amount(lang, sum);
                    if parse_money(lang, &text, currency) != Ok(sum) {
                        lost.push(format!("{lang:?} {sum:?} → «{text}»"));
                    }
                }
            }
            for per_unit_e4 in rates {
                let text = rate(lang, per_unit_e4);
                if parse_rate(lang, &text) != Ok(per_unit_e4) {
                    lost.push(format!("{lang:?} курс {per_unit_e4} → «{text}»"));
                }
            }
            for scaled in values.into_iter().chain(rates) {
                let text = decimal(lang, scaled, 3);
                if parse_decimal(lang, &text, 3) != Ok(scaled) {
                    lost.push(format!("{lang:?} количество {scaled} → «{text}»"));
                }
            }
        }
        assert!(lost.is_empty(), "записанное не читается обратно: {lost:#?}");
    }
}
