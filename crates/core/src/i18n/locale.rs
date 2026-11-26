//! Запись и разбор чисел, сумм, курсов и дат по правилам языка.
//!
//! Человеку сумму и дату показывает только эта запись: у `Money` и `Date`
//! нарочно нет `Display`. Окно и PDF зовут одни и те же функции, поэтому
//! в превью и в документе сумма записана одинаково
//! (`.claude/rules/features/documents.md`, «Что в превью, то и в PDF»).
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
use crate::model::date::{Date, Quarter};
use crate::model::money::{Currency, Lumas, Money, RATE_DIGITS};

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

/// Драмы с лумами (`model::money::Lumas`) — сумма в драмах по курсу и итоги
/// книги: с двумя знаками, только когда лумы есть, — `18 096,50 ֏`,
/// `AMD 18,096.50`, — а целые — как драмы, `361 930 ֏`: лумы в суммах
/// человек видит редко, и `,00` у каждой суммы в драмах только мешало бы.
pub fn lumas(lang: Lang, sum: Lumas) -> String {
    if sum.get() % Lumas::PER_DRAM == 0 {
        return money(lang, Money::new(sum.drams(), Currency::Amd));
    }
    let mut out = String::new();
    match lang {
        Lang::Am | Lang::Ru => {
            push_signed(&mut out, lang, sum.get(), 2);
            out.push(NBSP);
            out.push('֏');
        }
        Lang::En => {
            if sum.get() < 0 {
                out.push('-');
            }
            out.push_str("AMD\u{A0}");
            push_unsigned(&mut out, lang, sum.get().unsigned_abs(), 2);
        }
    }
    out
}

/// Процент — сотыми долями, `hundredths`: ставка налога `10 %`, `4,5 %`
/// по-русски — с неразрывным пробелом, как в CLDR (`#,##0 %`), — а по-армянски
/// и по-английски без пробела: `4,5%`, `4.5%` (CLDR `#,##0%`). Число, а не
/// тип налога: язык знает из домена только суммы и даты (таблица слоёв).
pub fn percent(lang: Lang, hundredths: i64) -> String {
    let mut out = decimal(lang, hundredths, 2);
    match lang {
        Lang::Ru => {
            out.push(NBSP);
            out.push('%');
        }
        Lang::Am | Lang::En => out.push('%'),
    }
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

/// День с месяцем словом и днём недели, без года — заголовок дня на главном
/// экране: `հոկտեմբերի 14, չորեքշաբթի`, `среда, 14 октября`, `Wednesday,
/// October 14`. Шаблон — CLDR `MMMMEEEEd` (по браузеру: CLDR 48 через ICU 78.3
/// в Node 24.21, сверено 2026-10-14); месяц в армянском и русском — в
/// родительном падеже, как его ставит CLDR (`format`, а не `stand-alone`).
/// С заглавной буквы его пишет окно, когда строка стоит первой.
pub fn day_long(lang: Lang, day: Date) -> String {
    let month = usize::from(day.month() - 1);
    let weekday = usize::from(day.weekday() - 1);
    match lang {
        Lang::Am => format!("{} {}, {}", HY_MONTHS[month], day.day(), HY_WEEKDAYS[weekday]),
        Lang::Ru => format!("{}, {} {}", RU_WEEKDAYS[weekday], day.day(), RU_MONTHS[month]),
        Lang::En => format!("{}, {} {}", EN_WEEKDAYS[weekday], EN_MONTHS_LONG[month], day.day()),
    }
}

/// Месяц — заголовком группы в списке документов: в этом году — одним
/// словом, иначе — с годом. Шаблоны CLDR `LLLL` и `yMMMM` (по браузеру:
/// CLDR 48 через ICU 78.3 в Node 24.21, сверено 2026-10-14): `октябрь`,
/// `октябрь 2025 г.`, `2025 թ․ հոկտեմբեր`, `October 2025`; месяц —
/// в именительном падеже (`stand-alone`). В армянском после «թ» — U+2024,
/// как в CLDR. С заглавной буквы его пишет окно, когда строка стоит первой.
pub fn month_heading(lang: Lang, year: u16, month: u8, this_year: u16) -> String {
    let index = usize::from(month.clamp(1, 12) - 1);
    let name = match lang {
        Lang::Am => HY_MONTHS_ALONE[index],
        Lang::Ru => RU_MONTHS_ALONE[index],
        Lang::En => EN_MONTHS_LONG[index],
    };
    if year == this_year {
        return name.to_owned();
    }
    match lang {
        Lang::Am => format!("{year} թ\u{2024} {name}"),
        Lang::Ru => format!("{name} {year} г."),
        Lang::En => format!("{name} {year}"),
    }
}

/// Квартал — заголовком книги (задача 14): «III квартал 2026», по-армянски
/// «III եռամսյակ 2026» — так в дизайне «Kvartik Design» (экран 10),
/// по-английски «Q3 2026». Римская цифра квартала — как пишут в русских
/// и армянских отчётах; CLDR квартала с годом не задаёт.
pub fn quarter(lang: Lang, quarter: Quarter) -> String {
    const ROMAN: [&str; 4] = ["I", "II", "III", "IV"];
    let number = usize::from(quarter.number().clamp(1, 4) - 1);
    let year = quarter.year();
    match lang {
        Lang::Am => format!("{}{NBSP}եռամսյակ {year}", ROMAN[number]),
        Lang::Ru => format!("{}{NBSP}квартал {year}", ROMAN[number]),
        Lang::En => format!("Q{}{NBSP}{year}", number + 1),
    }
}

/// День срока без года, словом (задача 126): `20 января`, `January 20`,
/// а по-армянски с определённым артиклем, как пишет кодекс о сроке
/// («մինչև … ամսվա 20-ը ներառյալ»): `հունվարի 20-ը`. Месяц — в том падеже,
/// что у даты с днём (CLDR `format`, как у [`day_long`]).
pub fn deadline_day(lang: Lang, month: u8, day: u8) -> String {
    let month = usize::from(month.clamp(1, 12) - 1);
    match lang {
        Lang::Am => format!("{} {day}-ը", HY_MONTHS[month]),
        Lang::Ru => format!("{day} {}", RU_MONTHS[month]),
        Lang::En => format!("{} {day}", EN_MONTHS_LONG[month]),
    }
}

/// Число месяца, к которому срок, — словом в строке «до 20-го числа»
/// (задачи 126 и 128): по-армянски `20-ը` («ամսվա 20-ը», как пишет кодекс),
/// по-русски `20-го`, по-английски порядковым — `20th`, `21st`, `22nd`,
/// `23rd`, а 11–13 — `th` (CLDR, порядковые формы `en`: one — 1, 21, 31…,
/// two — 2, 22…, few — 3, 23…, other — остальные, и 11, 12, 13 тоже).
/// Суффикс считает число: срок сменится на 21-е — и строка не скажет «21th».
pub fn month_day(lang: Lang, day: u8) -> String {
    match lang {
        Lang::Am => format!("{day}-ը"),
        Lang::Ru => format!("{day}-го"),
        Lang::En => {
            let suffix = match (day % 10, day % 100) {
                (_, 11..=13) => "th",
                (1, _) => "st",
                (2, _) => "nd",
                (3, _) => "rd",
                _ => "th",
            };
            format!("{day}{suffix}")
        }
    }
}

/// Курс парой — «единица валюты = драмы» (задача 14): `1 $ = 388,00 ֏`,
/// по-английски `$1 = AMD 388.00`. Знаки валют — те же, что у [`money`],
/// а число курса — [`rate`]. Пробелы — неразрывные, и вокруг «=» тоже:
/// пара — одно значение, и перенос посреди неё оставлял «1 $» без курса
/// (глазной прогон 2026-10-15).
pub fn rate_pair(lang: Lang, currency: Currency, per_unit_e4: i64) -> String {
    let value = rate(lang, per_unit_e4);
    let sign = match currency {
        Currency::Usd => "$",
        Currency::Eur => "€",
        Currency::Amd => match lang {
            Lang::Am | Lang::Ru => "֏",
            Lang::En => "AMD\u{A0}",
        },
    };
    match lang {
        Lang::Am | Lang::Ru => format!("1{NBSP}{sign}{NBSP}={NBSP}{value}{NBSP}֏"),
        Lang::En => format!("{sign}1{NBSP}={NBSP}AMD{NBSP}{value}"),
    }
}

/// Месяцы CLDR `hy`, `stand-alone` `wide` — в именительном падеже.
const HY_MONTHS_ALONE: [&str; 12] = [
    "հունվար",
    "փետրվար",
    "մարտ",
    "ապրիլ",
    "մայիս",
    "հունիս",
    "հուլիս",
    "օգոստոս",
    "սեպտեմբեր",
    "հոկտեմբեր",
    "նոյեմբեր",
    "դեկտեմբեր",
];

/// Месяцы CLDR `ru`, `stand-alone` `wide` — в именительном падеже.
const RU_MONTHS_ALONE: [&str; 12] = [
    "январь", "февраль", "март", "апрель", "май", "июнь", "июль", "август", "сентябрь",
    "октябрь", "ноябрь", "декабрь",
];

/// Месяцы CLDR `hy`, `format` `wide` — в родительном падеже: «հոկտեմբերի
/// 14».
const HY_MONTHS: [&str; 12] = [
    "հունվարի",
    "փետրվարի",
    "մարտի",
    "ապրիլի",
    "մայիսի",
    "հունիսի",
    "հուլիսի",
    "օգոստոսի",
    "սեպտեմբերի",
    "հոկտեմբերի",
    "նոյեմբերի",
    "դեկտեմբերի",
];

/// Месяцы CLDR `ru`, `format` `wide` — в родительном падеже: «14 октября».
const RU_MONTHS: [&str; 12] = [
    "января", "февраля", "марта", "апреля", "мая", "июня", "июля", "августа", "сентября",
    "октября", "ноября", "декабря",
];

/// Месяцы CLDR `en`, `wide`.
const EN_MONTHS_LONG: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// Дни недели CLDR, `wide`, с понедельника — как `Date::weekday` (ISO 8601).
const HY_WEEKDAYS: [&str; 7] = [
    "երկուշաբթի",
    "երեքշաբթի",
    "չորեքշաբթի",
    "հինգշաբթի",
    "ուրբաթ",
    "շաբաթ",
    "կիրակի",
];
const RU_WEEKDAYS: [&str; 7] = [
    "понедельник",
    "вторник",
    "среда",
    "четверг",
    "пятница",
    "суббота",
    "воскресенье",
];
const EN_WEEKDAYS: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
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

/// Сумма, введённая человеком, — по правилам языка окна
/// (`.claude/rules/pitfalls/core.md`, «Ввод суммы не угадывается»): `1,234` в
/// английском окне — тысяча двести тридцать четыре, в русском — одна целая и
/// двести тридцать четыре тысячных, и для доллара это уже ошибка `TooPrecise`.
/// Прочитанное окно сразу показывает записью `money`: человек видит, что
/// понято.
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

    /// День срока без года словом (задача 126): месяц в падеже даты с днём,
    /// по-армянски — с артиклем «-ը».
    #[test]
    fn a_deadline_day_is_written_by_the_language() {
        let mut wrong = Vec::new();
        let cases: [(Lang, u8, u8, &str); 6] = [
            (Lang::Am, 1, 20, "հունվարի 20-ը"),
            (Lang::Am, 10, 20, "հոկտեմբերի 20-ը"),
            (Lang::Ru, 1, 20, "20 января"),
            (Lang::Ru, 7, 20, "20 июля"),
            (Lang::En, 4, 20, "April 20"),
            (Lang::En, 10, 1, "October 1"),
        ];
        for (lang, month, day, expected) in cases {
            let got = deadline_day(lang, month, day);
            if got != expected {
                wrong.push(format!("{lang:?} {month}/{day}: {got} вместо {expected}"));
            }
        }
        assert!(wrong.is_empty(), "{wrong:#?}");
    }

    /// Число месяца срока словом (задачи 126 и 128): армянское и русское
    /// окончание и английский порядковый суффикс, в том числе 11–13 и 21–23.
    #[test]
    fn a_month_day_is_written_by_the_language() {
        let mut wrong = Vec::new();
        let cases: [(Lang, u8, &str); 12] = [
            (Lang::Am, 20, "20-ը"),
            (Lang::Ru, 20, "20-го"),
            (Lang::Ru, 1, "1-го"),
            (Lang::En, 20, "20th"),
            (Lang::En, 1, "1st"),
            (Lang::En, 2, "2nd"),
            (Lang::En, 3, "3rd"),
            (Lang::En, 11, "11th"),
            (Lang::En, 12, "12th"),
            (Lang::En, 13, "13th"),
            (Lang::En, 21, "21st"),
            (Lang::En, 23, "23rd"),
        ];
        for (lang, day, expected) in cases {
            let got = month_day(lang, day);
            if got != expected {
                wrong.push(format!("{lang:?} {day}: {got} вместо {expected}"));
            }
        }
        assert!(wrong.is_empty(), "{wrong:#?}");
    }

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

    /// Курс парой: единица валюты и драмы — знаками своего языка.
    #[test]
    fn rate_pairs_follow_each_language() {
        let cases = [
            (Lang::Am, Currency::Usd, 3_880_000, "1 $ = 388,00 ֏"),
            (Lang::Ru, Currency::Eur, 4_055_800, "1 € = 405,58 ֏"),
            (Lang::En, Currency::Usd, 3_880_000, "$1 = AMD 388.00"),
            (Lang::En, Currency::Eur, 4_055_849, "€1 = AMD 405.5849"),
        ];
        let mut wrong = Vec::new();
        for (lang, currency, per_unit_e4, expected) in cases {
            let got = rate_pair(lang, currency, per_unit_e4);
            let expected = nb(expected);
            if got != expected {
                wrong.push(format!("{lang:?} {currency:?}: «{got}», а не «{expected}»"));
            }
        }
        assert!(wrong.is_empty(), "курс парой записан не так: {wrong:#?}");
    }

    /// Квартал: римской цифрой по-армянски и по-русски, «Q3» по-английски.
    #[test]
    fn quarters_follow_each_language() {
        let third = Quarter::new(2026, 3).expect("квартал есть");
        let fourth = Quarter::new(2025, 4).expect("квартал есть");
        let cases = [
            (Lang::Am, third, "III\u{A0}եռամսյակ 2026"),
            (Lang::Ru, fourth, "IV\u{A0}квартал 2025"),
            (Lang::En, third, "Q3\u{A0}2026"),
        ];
        let mut wrong = Vec::new();
        for (lang, shown, expected) in cases {
            let got = quarter(lang, shown);
            if got != expected {
                wrong.push(format!("{lang:?}: «{got}», а не «{expected}»"));
            }
        }
        assert!(wrong.is_empty(), "кварталы записаны не так: {wrong:#?}");
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

    /// День с месяцем словом и днём недели — все двенадцать месяцев
    /// и семь дней недели на трёх языках. Ожидаемое выписано по
    /// `Intl.DateTimeFormat` с `weekday: "long", month: "long", day:
    /// "numeric"` (CLDR 48, Node 24.21, 2026-10-14), а не посчитано этим кодом.
    #[test]
    fn long_days_follow_each_language() {
        let expected: [(Lang, [&str; 19]); 3] = [
            (
                Lang::Am,
                [
                    "հունվարի 5, երկուշաբթի",
                    "փետրվարի 5, հինգշաբթի",
                    "մարտի 5, հինգշաբթի",
                    "ապրիլի 5, կիրակի",
                    "մայիսի 5, երեքշաբթի",
                    "հունիսի 5, ուրբաթ",
                    "հուլիսի 5, կիրակի",
                    "օգոստոսի 5, չորեքշաբթի",
                    "սեպտեմբերի 5, շաբաթ",
                    "հոկտեմբերի 5, երկուշաբթի",
                    "նոյեմբերի 5, հինգշաբթի",
                    "դեկտեմբերի 5, շաբաթ",
                    "հոկտեմբերի 12, երկուշաբթի",
                    "հոկտեմբերի 13, երեքշաբթի",
                    "հոկտեմբերի 14, չորեքշաբթի",
                    "հոկտեմբերի 15, հինգշաբթի",
                    "հոկտեմբերի 16, ուրբաթ",
                    "հոկտեմբերի 17, շաբաթ",
                    "հոկտեմբերի 18, կիրակի",
                ],
            ),
            (
                Lang::Ru,
                [
                    "понедельник, 5 января",
                    "четверг, 5 февраля",
                    "четверг, 5 марта",
                    "воскресенье, 5 апреля",
                    "вторник, 5 мая",
                    "пятница, 5 июня",
                    "воскресенье, 5 июля",
                    "среда, 5 августа",
                    "суббота, 5 сентября",
                    "понедельник, 5 октября",
                    "четверг, 5 ноября",
                    "суббота, 5 декабря",
                    "понедельник, 12 октября",
                    "вторник, 13 октября",
                    "среда, 14 октября",
                    "четверг, 15 октября",
                    "пятница, 16 октября",
                    "суббота, 17 октября",
                    "воскресенье, 18 октября",
                ],
            ),
            (
                Lang::En,
                [
                    "Monday, January 5",
                    "Thursday, February 5",
                    "Thursday, March 5",
                    "Sunday, April 5",
                    "Tuesday, May 5",
                    "Friday, June 5",
                    "Sunday, July 5",
                    "Wednesday, August 5",
                    "Saturday, September 5",
                    "Monday, October 5",
                    "Thursday, November 5",
                    "Saturday, December 5",
                    "Monday, October 12",
                    "Tuesday, October 13",
                    "Wednesday, October 14",
                    "Thursday, October 15",
                    "Friday, October 16",
                    "Saturday, October 17",
                    "Sunday, October 18",
                ],
            ),
        ];
        let days = (1..=12)
            .map(|month| day(2026, month, 5))
            .chain((12..=18).map(|d| day(2026, 10, d)));
        let mut wrong = Vec::new();
        for (lang, rows) in expected {
            for (d, want) in days.clone().zip(rows) {
                let got = day_long(lang, d);
                if got != want {
                    wrong.push(format!("{lang:?} {d:?}: «{got}», а не «{want}»"));
                }
            }
        }
        assert!(wrong.is_empty(), "день записан не так: {wrong:#?}");
    }

    /// Месяц заголовком группы: в этом году — одним словом, в другом —
    /// с годом. Ожидаемое выписано по `Intl.DateTimeFormat` с `month:
    /// "long"` и с `year: "numeric", month: "long"` (CLDR 48, Node 24.21,
    /// 2026-10-14), а не посчитано этим кодом.
    #[test]
    fn month_headings_follow_each_language() {
        let alone: [(Lang, [&str; 12]); 3] = [
            (
                Lang::Am,
                [
                    "հունվար", "փետրվար", "մարտ", "ապրիլ", "մայիս", "հունիս", "հուլիս",
                    "օգոստոս", "սեպտեմբեր", "հոկտեմբեր", "նոյեմբեր", "դեկտեմբեր",
                ],
            ),
            (
                Lang::Ru,
                [
                    "январь", "февраль", "март", "апрель", "май", "июнь", "июль", "август",
                    "сентябрь", "октябрь", "ноябрь", "декабрь",
                ],
            ),
            (
                Lang::En,
                [
                    "January", "February", "March", "April", "May", "June", "July", "August",
                    "September", "October", "November", "December",
                ],
            ),
        ];
        let with_year: [(Lang, [&str; 12]); 3] = [
            (
                Lang::Am,
                [
                    "2025 թ\u{2024} հունվար",
                    "2025 թ\u{2024} փետրվար",
                    "2025 թ\u{2024} մարտ",
                    "2025 թ\u{2024} ապրիլ",
                    "2025 թ\u{2024} մայիս",
                    "2025 թ\u{2024} հունիս",
                    "2025 թ\u{2024} հուլիս",
                    "2025 թ\u{2024} օգոստոս",
                    "2025 թ\u{2024} սեպտեմբեր",
                    "2025 թ\u{2024} հոկտեմբեր",
                    "2025 թ\u{2024} նոյեմբեր",
                    "2025 թ\u{2024} դեկտեմբեր",
                ],
            ),
            (
                Lang::Ru,
                [
                    "январь 2025 г.",
                    "февраль 2025 г.",
                    "март 2025 г.",
                    "апрель 2025 г.",
                    "май 2025 г.",
                    "июнь 2025 г.",
                    "июль 2025 г.",
                    "август 2025 г.",
                    "сентябрь 2025 г.",
                    "октябрь 2025 г.",
                    "ноябрь 2025 г.",
                    "декабрь 2025 г.",
                ],
            ),
            (
                Lang::En,
                [
                    "January 2025",
                    "February 2025",
                    "March 2025",
                    "April 2025",
                    "May 2025",
                    "June 2025",
                    "July 2025",
                    "August 2025",
                    "September 2025",
                    "October 2025",
                    "November 2025",
                    "December 2025",
                ],
            ),
        ];
        let mut wrong = Vec::new();
        for ((lang, names), (_, dated)) in alone.into_iter().zip(with_year) {
            for (month, (name, full)) in (1..=12).zip(names.into_iter().zip(dated)) {
                let this = month_heading(lang, 2026, month, 2026);
                let other = month_heading(lang, 2025, month, 2026);
                if this != name || other != full {
                    wrong.push(format!("{lang:?} {month}: «{this}», «{other}»"));
                }
            }
        }
        assert!(wrong.is_empty(), "месяц записан не так: {wrong:#?}");
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

    /// Драмы с лумами — с двумя знаками, только когда лумы есть; процент —
    /// по CLDR своего языка. Ожидаемое выписано человеком.
    #[test]
    fn lumas_and_percents_follow_each_language() {
        let cases = [
            (Lang::Ru, lumas(Lang::Ru, Lumas::new(1_809_650)), "18\u{A0}096,50\u{A0}֏"),
            (Lang::Am, lumas(Lang::Am, Lumas::new(1_809_650)), "18\u{A0}096,50\u{A0}֏"),
            (Lang::En, lumas(Lang::En, Lumas::new(1_809_650)), "AMD\u{A0}18,096.50"),
            (Lang::Ru, lumas(Lang::Ru, Lumas::new(36_193_000)), "361\u{A0}930\u{A0}֏"),
            (Lang::En, lumas(Lang::En, Lumas::new(36_193_000)), "AMD\u{A0}361,930"),
            (Lang::Am, lumas(Lang::Am, Lumas::new(362)), "3,62\u{A0}֏"),
            (Lang::En, lumas(Lang::En, Lumas::new(-362)), "-AMD\u{A0}3.62"),
            (Lang::Ru, percent(Lang::Ru, 1_000), "10\u{A0}%"),
            (Lang::Ru, percent(Lang::Ru, 450), "4,5\u{A0}%"),
            (Lang::Am, percent(Lang::Am, 450), "4,5%"),
            (Lang::En, percent(Lang::En, 450), "4.5%"),
            (Lang::En, percent(Lang::En, 100), "1%"),
        ];
        let mut wrong = Vec::new();
        for (lang, got, expected) in cases {
            if got != expected {
                wrong.push(format!("{lang:?}: «{got}», а не «{expected}»"));
            }
        }
        assert!(wrong.is_empty(), "запись: {wrong:#?}");
    }
}
