//! Календарная дата и квартал.
//!
//! Дата счёта, акта или операции — день в календаре, а не момент времени:
//! год, месяц и день, без часового пояса (Правило 2, «Даты — календарные»).
//! Через UTC дата уезжает на сутки: в Ереване UTC+4, и счёт, выписанный
//! в 01:00, по UTC оказывается вчерашним, а первого числа квартала — уезжает
//! в прошлый квартал. «Сегодня» оболочка берёт у устройства по местному
//! времени и передаёт сюда готовой датой (Архитектура, «Время — параметр»).
//!
//! **Тип свой, а не `jiff::civil::Date`** — по замеру wasm 2026-10-10
//! (`trunk build --release` 0.22.0-rc.2, wasm-opt `s`, `brotli -q 11`).
//! Проба — поле ввода даты на экране каркаса: разбор ISO, квартал, срок после
//! квартала, дата через 30 дней, день недели, дни до конца года. С заглушкой
//! вместо календаря wasm после brotli весил 26 402 Б, со своим типом —
//! 27 194 (+0,8 КБ), с `jiff` 0.2.38 без фич по умолчанию — 66 407 (+40 КБ,
//! больше всего каркаса 0.0.1). Свой календарь на стандартной библиотеке —
//! сотня строк под тестами.
//!
//! Пока здесь только то, что нужно сейчас: проверка даты, квартал, день
//! месяца после квартала, запись ISO (`2026-10-10`) — так дата лежит
//! в данных и в файле копии (`format`) — и разность дней: напоминание
//! о копии через 7 дней (задача 7). Сложение дней — срок оплаты счёта,
//! сложение месяцев — повторяющийся счёт (задача 11). День недели —
//! заголовок дня на главном экране и перенос срока на рабочий день
//! (`tax::deadlines`, задача 21). Дни считают алгоритмы `days_from_civil` и `civil_from_days`
//! Говарда Хиннанта (`day_number` и `from_day_number` ниже) — на них строить
//! и его.

use std::fmt;

use serde::de::{self, Deserializer, Visitor};
use serde::{Deserialize, Serialize, Serializer};

/// Календарная дата: год, месяц и день, без часового пояса.
///
/// Поля закрыты: дата бывает только такой, какая есть в календаре
/// (`Date::new`). Поля идут в порядке «год, месяц, день», и выведенное
/// сравнение поэтому хронологическое.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date {
    year: u16,
    month: u8,
    day: u8,
}

impl Date {
    /// Первый год, который принимает тип.
    pub const MIN_YEAR: u16 = 1;
    /// Последний год: четыре цифры, как в записи ISO 8601.
    pub const MAX_YEAR: u16 = 9999;

    /// Дата, если такой день есть в календаре. `None` — такого дня нет:
    /// 31 апреля, 29 февраля невисокосного года, тринадцатый месяц, год за
    /// пределами `MIN_YEAR..=MAX_YEAR`.
    ///
    /// Месяц считается с единицы: январь — 1. У `js_sys::Date` январь — 0
    /// (Правило 6).
    pub const fn new(year: u16, month: u8, day: u8) -> Option<Self> {
        if year < Self::MIN_YEAR || year > Self::MAX_YEAR || month < 1 || month > 12 {
            return None;
        }
        if day < 1 || day > days_in_month(year, month) {
            return None;
        }
        Some(Self { year, month, day })
    }

    /// Год.
    pub const fn year(self) -> u16 {
        self.year
    }

    /// Месяц: январь — 1.
    pub const fn month(self) -> u8 {
        self.month
    }

    /// День месяца.
    pub const fn day(self) -> u8 {
        self.day
    }

    /// Сколько дней прошло от `earlier` до этой даты: от понедельника до
    /// следующего понедельника — 7. Если `earlier` позже — число
    /// отрицательное.
    pub const fn days_since(self, earlier: Date) -> i32 {
        self.day_number() - earlier.day_number()
    }

    /// День недели по ISO 8601: понедельник — 1, воскресенье — 7.
    /// 1970-01-01, нулевой день `day_number`, — четверг.
    pub const fn weekday(self) -> u8 {
        (self.day_number() + 3).rem_euclid(7) as u8 + 1
    }

    /// Номер дня: сколько дней от 1970-01-01 (алгоритм `days_from_civil`
    /// Говарда Хиннанта, «chrono-Compatible Low-Level Date Algorithms»).
    /// Год считается с марта, чтобы 29 февраля было последним днём года,
    /// а 400 лет — ровно 146 097 дней. Для лет 1–9999 всё в `i32`: по модулю
    /// не больше 2,93 млн.
    const fn day_number(self) -> i32 {
        let month = self.month as i32;
        let year = self.year as i32 - if month <= 2 { 1 } else { 0 };
        // Год с марта неотрицателен (год 1 января — это нулевой), поэтому
        // деление округляет к нулю, как и надо.
        let era = year / 400;
        let year_of_era = year - era * 400;
        let shifted_month = if month > 2 { month - 3 } else { month + 9 };
        let day_of_year = (153 * shifted_month + 2) / 5 + self.day as i32 - 1;
        let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
        era * 146_097 + day_of_era - 719_468
    }

    /// Дата по номеру дня от 1970-01-01 — обратный `day_number` (алгоритм
    /// `civil_from_days` там же). `None` — день за пределами
    /// `MIN_YEAR..=MAX_YEAR`. Крейту — для дня момента записи
    /// (`license::data_since`).
    pub(crate) const fn from_day_number(number: i32) -> Option<Self> {
        // Сдвиг к 0000-03-01: для лет от первого число неотрицательно,
        // и деление на эры округляет к нулю, как и надо.
        let shifted = number as i64 + 719_468;
        if shifted < 0 {
            return None;
        }
        let era = shifted / 146_097;
        let day_of_era = shifted - era * 146_097;
        let year_of_era =
            (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
        let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
        let shifted_month = (5 * day_of_year + 2) / 153;
        let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
        let month = if shifted_month < 10 {
            shifted_month + 3
        } else {
            shifted_month - 9
        };
        let year = year_of_era + era * 400 + if month <= 2 { 1 } else { 0 };
        if year < Self::MIN_YEAR as i64 || year > Self::MAX_YEAR as i64 {
            return None;
        }
        // Месяц — от 1 до 12, день — от 1 до 31: в `u8` они помещаются,
        // и такой день в календаре есть по построению.
        Self::new(year as u16, month as u8, day as u8)
    }

    /// Дата через `days` дней — или раньше на столько, если число
    /// отрицательное: срок оплаты — дата счёта плюс дни из договора.
    /// `None` — календарь `Date` кончился.
    pub const fn add_days(self, days: i32) -> Option<Self> {
        match self.day_number().checked_add(days) {
            Some(number) => Self::from_day_number(number),
            None => None,
        }
    }

    /// Дата через `months` месяцев — для повторяющегося счёта (задача 11).
    ///
    /// Последний день месяца переходит в последний день: счёт за подписку,
    /// выписанный 30 сентября, через месяц выписывается 31 октября, а через
    /// пять — 28 февраля. Остальные дни остаются тем же числом, а где его
    /// нет — последним днём: 30 января через месяц — 28 февраля, через
    /// два — 30 марта. Считать каждый месяц от исходной даты, а не от
    /// прошлой: иначе 31-е, однажды ставшее 28-м, так бы им и осталось.
    /// `None` — календарь `Date` кончился.
    pub const fn add_months(self, months: u32) -> Option<Self> {
        let index = self.year as u64 * 12 + (self.month - 1) as u64 + months as u64;
        let year = index / 12;
        if year > Self::MAX_YEAR as u64 {
            return None;
        }
        // Год не больше 9999, месяц — от 1 до 12.
        let (year, month) = (year as u16, (index % 12) as u8 + 1);
        let last = days_in_month(year, month);
        let day = if self.is_last_of_month() || self.day > last {
            last
        } else {
            self.day
        };
        Some(Self { year, month, day })
    }

    /// Последний ли это день месяца.
    pub const fn is_last_of_month(self) -> bool {
        self.day == days_in_month(self.year, self.month)
    }

    /// Последний день того же месяца: у подписки на программу услуга
    /// оказана в последний день месяца (задача 11).
    pub const fn last_of_month(self) -> Self {
        Self {
            day: days_in_month(self.year, self.month),
            ..self
        }
    }

    /// Квартал, в который попадает дата: январь — март — первый, октябрь —
    /// декабрь — четвёртый.
    pub const fn quarter(self) -> Quarter {
        Quarter {
            year: self.year,
            number: (self.month - 1) / 3 + 1,
        }
    }

    /// Запись ISO 8601 `ГГГГ-ММ-ДД` — как в данных и как её берёт и отдаёт
    /// поле ввода даты (`<input type="date">`). Человеку дату показывает
    /// `i18n::date`, а не эта запись.
    pub fn iso(self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }

    /// Дата из записи ISO 8601 `ГГГГ-ММ-ДД` (`2026-03-31`) — так дата лежит
    /// в данных и в файле копии. `None` — запись не такая или такого дня нет
    /// в календаре.
    ///
    /// Разбор строгий: ровно десять знаков, цифры ASCII и дефисы на своих
    /// местах. `2026-3-31`, `31.03.2026`, `2026-03-31T00:00`, пробел по краям —
    /// не даты: что записывает программа, то она и читает, а всё прочее —
    /// испорченные данные, и угадывать их нельзя (Правило 13). Человеку дату
    /// показывает и читает у него `i18n`, а не эта функция.
    pub fn from_iso(text: &str) -> Option<Self> {
        let bytes = text.as_bytes();
        if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
            return None;
        }
        // Не больше четырёх цифр: 9999 помещается в `u16`.
        let number = |digits: &[u8]| {
            digits.iter().try_fold(0_u16, |value, &digit| {
                digit
                    .is_ascii_digit()
                    .then(|| value * 10 + u16::from(digit - b'0'))
            })
        };
        let year = number(&bytes[..4])?;
        let month = u8::try_from(number(&bytes[5..7])?).ok()?;
        let day = u8::try_from(number(&bytes[8..])?).ok()?;
        Self::new(year, month, day)
    }
}

/// Запись ISO 8601 (`2026-03-31`) — для тестов и отладки. Человеку дату
/// показывает `i18n` по правилам языка (задача 4), поэтому `Display` у даты
/// нет: `{date}` в тексте окна не соберётся.
impl fmt::Debug for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

/// В данных дата — строка ISO 8601: `"2026-03-31"`.
impl Serialize for Date {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&format_args!(
            "{:04}-{:02}-{:02}",
            self.year, self.month, self.day
        ))
    }
}

/// Читает дату только через `Date::from_iso`, а значит, через `Date::new`.
/// Выведенный `Deserialize` принял бы поля как есть, и месяц 0 из
/// испорченного файла копии уронил бы `quarter` переполнением (Правило 13).
impl<'de> Deserialize<'de> for Date {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Iso;

        impl Visitor<'_> for Iso {
            type Value = Date;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("дату ISO 8601 вида ГГГГ-ММ-ДД, которая есть в календаре")
            }

            fn visit_str<E: de::Error>(self, text: &str) -> Result<Date, E> {
                Date::from_iso(text)
                    .ok_or_else(|| E::invalid_value(de::Unexpected::Str(text), &self))
            }
        }

        deserializer.deserialize_str(Iso)
    }
}

/// Квартал года: номер от 1 до 4.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Quarter {
    year: u16,
    number: u8,
}

impl Quarter {
    /// Квартал по году и номеру. `None` — номер не от 1 до 4 или год за
    /// пределами `Date`.
    pub const fn new(year: u16, number: u8) -> Option<Self> {
        if year < Date::MIN_YEAR || year > Date::MAX_YEAR || number < 1 || number > 4 {
            return None;
        }
        Some(Self { year, number })
    }

    /// Год.
    pub const fn year(self) -> u16 {
        self.year
    }

    /// Номер квартала: от 1 до 4.
    pub const fn number(self) -> u8 {
        self.number
    }

    /// Первый день: 1 января, 1 апреля, 1 июля или 1 октября.
    pub const fn first_day(self) -> Date {
        Date {
            year: self.year,
            month: self.first_month(),
            day: 1,
        }
    }

    /// Последний день: 31 марта, 30 июня, 30 сентября или 31 декабря.
    pub const fn last_day(self) -> Date {
        let month = self.first_month() + 2;
        Date {
            year: self.year,
            month,
            day: days_in_month(self.year, month),
        }
    }

    /// `day`-е число месяца, который идёт сразу после квартала: апреля для
    /// первого квартала, января следующего года — для четвёртого. `None` —
    /// такого дня в том месяце нет или следующего года нет у `Date`.
    ///
    /// Так задаются сроки квартальных расчётов, но само число — правило
    /// с источником в файле правил (`tax::rules`), а не константа в коде
    /// (таблица слоёв CLAUDE.md, «Налоги»).
    pub const fn day_in_following_month(self, day: u8) -> Option<Date> {
        if self.number == 4 {
            // Год не больше 9999, и `+ 1` в `u16` не переполнится; 10000-й
            // отвергнет `Date::new`.
            Date::new(self.year + 1, 1, day)
        } else {
            Date::new(self.year, self.first_month() + 3, day)
        }
    }

    /// Прошлый квартал: перед первым — четвёртый прошлого года. `None` —
    /// раньше календаря `Date`.
    pub const fn previous(self) -> Option<Self> {
        if self.number > 1 {
            Some(Self {
                year: self.year,
                number: self.number - 1,
            })
        } else if self.year > Date::MIN_YEAR {
            Some(Self {
                year: self.year - 1,
                number: 4,
            })
        } else {
            None
        }
    }

    /// Следующий квартал: после четвёртого — первый следующего года.
    /// `None` — позже календаря `Date`.
    pub const fn next(self) -> Option<Self> {
        if self.number < 4 {
            Some(Self {
                year: self.year,
                number: self.number + 1,
            })
        } else if self.year < Date::MAX_YEAR {
            Some(Self {
                year: self.year + 1,
                number: 1,
            })
        } else {
            None
        }
    }

    /// Первый месяц квартала: 1, 4, 7 или 10.
    const fn first_month(self) -> u8 {
        (self.number - 1) * 3 + 1
    }
}

/// Високосный год по григорианскому календарю: каждый четвёртый, кроме
/// вековых, но каждый четырёхсотый — снова високосный (2000 — да, 2100 — нет).
const fn is_leap_year(year: u16) -> bool {
    year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400))
}

/// Сколько дней в месяце. Месяц — от 1 до 12.
const fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        2 if is_leap_year(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(year: u16, month: u8, day: u8) -> Date {
        Date::new(year, month, day).expect("в тесте — существующая дата")
    }

    fn quarter(year: u16, number: u8) -> Quarter {
        Quarter::new(year, number).expect("в тесте — существующий квартал")
    }

    /// Граница квартала: 31 марта — ещё первый квартал, 1 апреля — уже
    /// второй. Ошибка здесь — доход в чужом квартале и налог не за тот
    /// квартал, без единого сообщения.
    #[test]
    fn quarter_boundaries() {
        let cases = [
            (date(2026, 1, 1), quarter(2026, 1)),
            (date(2026, 3, 31), quarter(2026, 1)),
            (date(2026, 4, 1), quarter(2026, 2)),
            (date(2026, 6, 30), quarter(2026, 2)),
            (date(2026, 7, 1), quarter(2026, 3)),
            (date(2026, 9, 30), quarter(2026, 3)),
            (date(2026, 10, 1), quarter(2026, 4)),
            (date(2026, 12, 31), quarter(2026, 4)),
            (date(2027, 1, 1), quarter(2027, 1)),
        ];
        let mut wrong = Vec::new();
        for (day, expected) in cases {
            if day.quarter() != expected {
                wrong.push(format!("{day:?}: {:?}, а не {expected:?}", day.quarter()));
            }
        }
        assert!(wrong.is_empty(), "не тот квартал: {wrong:#?}");
    }

    /// Соседний квартал — через границу года и до краёв календаря: перед
    /// первым кварталом 2026 года — четвёртый 2025-го, после четвёртого —
    /// первый 2027-го.
    #[test]
    fn neighbour_quarters_cross_the_year() {
        assert_eq!(quarter(2026, 1).previous(), Some(quarter(2025, 4)));
        assert_eq!(quarter(2026, 3).previous(), Some(quarter(2026, 2)));
        assert_eq!(quarter(2026, 4).next(), Some(quarter(2027, 1)));
        assert_eq!(quarter(2026, 2).next(), Some(quarter(2026, 3)));
        assert_eq!(quarter(Date::MIN_YEAR, 1).previous(), None);
        assert_eq!(quarter(Date::MAX_YEAR, 4).next(), None);
    }

    /// День недели — по календарю, а не той же формулой: 1970-01-01 —
    /// четверг, 2000-02-29 — вторник, 2026-10-14 — среда, 2026-10-18 —
    /// воскресенье, 2027-01-01 — пятница, 9999-12-31 — пятница. И каждый
    /// следующий день — следующий день недели, через воскресенье на
    /// понедельник.
    #[test]
    fn weekday_follows_the_calendar() {
        let cases = [
            (date(1970, 1, 1), 4),
            (date(2000, 2, 29), 2),
            (date(2026, 10, 12), 1),
            (date(2026, 10, 14), 3),
            (date(2026, 10, 18), 7),
            (date(2027, 1, 1), 5),
            (date(1, 1, 1), 1),
            (date(9999, 12, 31), 5),
        ];
        let mut wrong = Vec::new();
        for (day, expected) in cases {
            if day.weekday() != expected {
                wrong.push(format!("{day:?}: {}, а не {expected}", day.weekday()));
            }
        }
        let mut day = date(2026, 1, 1);
        while let Some(next) = day.add_days(1).filter(|next| next.year() == 2026) {
            if next.weekday() != day.weekday() % 7 + 1 {
                wrong.push(format!("{next:?} после {day:?}: {}", next.weekday()));
            }
            day = next;
        }
        assert!(wrong.is_empty(), "не тот день недели: {wrong:#?}");
    }

    /// Первый и последний день каждого квартала — выписаны человеком по
    /// календарю, а не посчитаны той же формулой.
    #[test]
    fn quarter_first_and_last_days() {
        let cases = [
            (quarter(2026, 1), date(2026, 1, 1), date(2026, 3, 31)),
            (quarter(2026, 2), date(2026, 4, 1), date(2026, 6, 30)),
            (quarter(2026, 3), date(2026, 7, 1), date(2026, 9, 30)),
            (quarter(2026, 4), date(2026, 10, 1), date(2026, 12, 31)),
        ];
        let mut wrong = Vec::new();
        for (q, first, last) in cases {
            if (q.first_day(), q.last_day()) != (first, last) {
                wrong.push(format!(
                    "{q:?}: {:?} — {:?}, а не {first:?} — {last:?}",
                    q.first_day(),
                    q.last_day()
                ));
            }
        }
        assert!(wrong.is_empty(), "не те границы: {wrong:#?}");
    }

    /// Каждый день 2000–2100 годов лежит в своём квартале — между его первым
    /// и последним днём, — а первым днём квартала бывают только 1 января,
    /// 1 апреля, 1 июля и 1 октября. Дней в этих годах 101 × 365 и ещё 25
    /// високосных (2000, 2004, …, 2096; 2100 — не високосный): 36 890.
    #[test]
    fn every_day_lies_in_its_quarter() {
        let mut days = 0;
        let mut wrong = Vec::new();
        for year in 2000..=2100 {
            for month in 1..=12 {
                for day in 1..=31 {
                    let Some(d) = Date::new(year, month, day) else {
                        continue;
                    };
                    days += 1;
                    let q = d.quarter();
                    if d < q.first_day() || d > q.last_day() {
                        wrong.push(format!("{d:?} вне {q:?}"));
                    }
                    let starts_quarter = day == 1 && matches!(month, 1 | 4 | 7 | 10);
                    if (d == q.first_day()) != starts_quarter {
                        wrong.push(format!("{d:?}: первый день {q:?} — {:?}", q.first_day()));
                    }
                }
            }
        }
        assert!(wrong.is_empty(), "дни не в своём квартале: {wrong:#?}");
        assert_eq!(days, 36_890, "в календаре 2000–2100 не то число дней");
    }

    /// N-е число месяца после квартала; после четвёртого — январь
    /// следующего года.
    #[test]
    fn day_in_following_month() {
        let cases = [
            (quarter(2026, 1), 20, Some(date(2026, 4, 20))),
            (quarter(2026, 2), 20, Some(date(2026, 7, 20))),
            (quarter(2026, 3), 20, Some(date(2026, 10, 20))),
            (quarter(2026, 4), 20, Some(date(2027, 1, 20))),
            // 31 апреля нет, а 31 января — есть.
            (quarter(2026, 1), 31, None),
            (quarter(2027, 4), 31, Some(date(2028, 1, 31))),
            // После 9999 года календарь кончается.
            (quarter(9999, 4), 20, None),
        ];
        let mut wrong = Vec::new();
        for (q, day, expected) in cases {
            let got = q.day_in_following_month(day);
            if got != expected {
                wrong.push(format!("{q:?}, {day}-е: {got:?}, а не {expected:?}"));
            }
        }
        assert!(wrong.is_empty(), "не тот день после квартала: {wrong:#?}");
    }

    /// Дата существует, только если такой день есть в календаре: високосный —
    /// каждый четвёртый год, кроме вековых, но 2000 — високосный.
    #[test]
    fn only_real_calendar_days() {
        let real = [
            (2028, 2, 29),
            (2000, 2, 29),
            (2026, 4, 30),
            (2026, 12, 31),
            (1, 1, 1),
            (9999, 12, 31),
        ];
        let unreal = [
            (2026, 2, 29),
            (2100, 2, 29),
            (1900, 2, 29),
            (2026, 4, 31),
            (2026, 6, 31),
            (2026, 13, 1),
            (2026, 0, 10),
            (2026, 1, 0),
            (2026, 1, 32),
            (0, 1, 1),
            (10_000, 1, 1),
        ];
        let mut wrong = Vec::new();
        for (year, month, day) in real {
            if Date::new(year, month, day).is_none() {
                wrong.push(format!("{year}-{month}-{day} отвергнута"));
            }
        }
        for (year, month, day) in unreal {
            if let Some(d) = Date::new(year, month, day) {
                wrong.push(format!("{year}-{month}-{day} принята как {d:?}"));
            }
        }
        assert!(wrong.is_empty(), "календарь ошибся: {wrong:#?}");
    }

    /// Номер квартала — только от 1 до 4.
    #[test]
    fn quarter_number_is_one_to_four() {
        assert_eq!(Quarter::new(2026, 0), None);
        assert_eq!(Quarter::new(2026, 5), None);
        assert_eq!(Quarter::new(0, 1), None);
        assert_eq!(quarter(2026, 4).number(), 4);
    }

    /// Сравнение — по календарю: год, потом месяц, потом день.
    #[test]
    fn ordering_is_chronological() {
        assert!(date(2026, 3, 31) < date(2026, 4, 1));
        assert!(date(2025, 12, 31) < date(2026, 1, 1));
        assert!(date(2026, 1, 31) < date(2026, 2, 1));
        assert!(quarter(2026, 4) < quarter(2027, 1));
    }

    /// Разность дней сверена не этой же формулой, а внешними числами:
    /// от 1970-01-01 до 2000-01-01 — 10 957 дней (946 684 800 секунд Unix),
    /// от 0001-01-01 до 9999-12-31 — 3 652 058 (порядковые номера дней
    /// `datetime.date` в Python). Остальное — границы, где ошибается
    /// календарь: февраль високосного года, вековой год, 400-й год, Новый год.
    #[test]
    fn days_since_counts_calendar_days() {
        let cases = [
            (date(2026, 10, 12), date(2026, 10, 5), 7),
            (date(2026, 10, 5), date(2026, 10, 12), -7),
            (date(2026, 10, 12), date(2026, 10, 12), 0),
            (date(2027, 1, 1), date(2026, 12, 31), 1),
            (date(2024, 3, 1), date(2024, 2, 28), 2),
            (date(2023, 3, 1), date(2023, 2, 28), 1),
            (date(2000, 3, 1), date(2000, 2, 28), 2),
            (date(2100, 3, 1), date(2100, 2, 28), 1),
            (date(2000, 1, 1), date(1970, 1, 1), 10_957),
            (date(2026, 10, 12), date(1970, 1, 1), 20_738),
            (date(9999, 12, 31), date(1, 1, 1), 3_652_058),
        ];
        let mut wrong = Vec::new();
        for (later, earlier, expected) in cases {
            let got = later.days_since(earlier);
            if got != expected {
                wrong.push(format!("{later:?} − {earlier:?}: {got}, а не {expected}"));
            }
        }
        assert!(wrong.is_empty(), "не та разность дней: {wrong:#?}");
    }

    /// Соседние дни отстоят ровно на день на всём отрезке 2000–2100 годов:
    /// дыра или повтор номера дня на стыке месяцев дали бы напоминание
    /// на день раньше или позже.
    #[test]
    fn neighbouring_days_are_one_day_apart() {
        let mut days = Vec::new();
        for year in 2000..=2100 {
            for month in 1..=12 {
                days.extend((1..=31).filter_map(|day| Date::new(year, month, day)));
            }
        }
        let wrong: Vec<String> = days
            .windows(2)
            .filter(|pair| pair[1].days_since(pair[0]) != 1)
            .map(|pair| format!("{:?} → {:?}", pair[0], pair[1]))
            .collect();
        assert!(wrong.is_empty(), "соседние дни не через день: {wrong:#?}");
    }

    /// Сложение дней — числами, выписанными по календарю, а не той же
    /// формулой: стыки месяцев и лет, 29 февраля, отрицательные дни и края
    /// календаря.
    #[test]
    fn add_days_follows_the_calendar() {
        let cases = [
            (date(2026, 10, 13), 15, Some(date(2026, 10, 28))),
            (date(2026, 10, 13), 0, Some(date(2026, 10, 13))),
            (date(2026, 12, 25), 10, Some(date(2027, 1, 4))),
            (date(2028, 2, 28), 1, Some(date(2028, 2, 29))),
            (date(2026, 2, 28), 1, Some(date(2026, 3, 1))),
            (date(2026, 1, 31), 30, Some(date(2026, 3, 2))),
            (date(2026, 3, 1), -1, Some(date(2026, 2, 28))),
            (date(2026, 10, 13), 365, Some(date(2027, 10, 13))),
            (date(2027, 10, 13), 366, Some(date(2028, 10, 13))),
            (date(1970, 1, 1), 10_957, Some(date(2000, 1, 1))),
            (date(1, 1, 1), 3_652_058, Some(date(9999, 12, 31))),
            (date(9999, 12, 31), 1, None),
            (date(1, 1, 1), -1, None),
            (date(2026, 10, 13), i32::MAX, None),
            (date(2026, 10, 13), i32::MIN, None),
        ];
        let mut wrong = Vec::new();
        for (from, days, expected) in cases {
            let got = from.add_days(days);
            if got != expected {
                wrong.push(format!("{from:?} + {days}: {got:?}, а не {expected:?}"));
            }
        }
        assert!(wrong.is_empty(), "не тот день: {wrong:#?}");
    }

    /// Номер дня читается обратно в тот же день — на каждом дне 2000–2100
    /// годов и на краях календаря.
    #[test]
    fn day_numbers_round_trip() {
        let mut days = vec![date(1, 1, 1), date(9999, 12, 31)];
        for year in 2000..=2100 {
            for month in 1..=12 {
                days.extend((1..=31).filter_map(|day| Date::new(year, month, day)));
            }
        }
        let lost: Vec<String> = days
            .iter()
            .filter(|day| Date::from_day_number(day.day_number()) != Some(**day))
            .map(|day| format!("{day:?}"))
            .collect();
        assert!(lost.is_empty(), "номер дня не читается обратно: {lost:#?}");
    }

    /// Сложение месяцев: последний день месяца остаётся последним, другое
    /// число — тем же числом, а где его нет — последним днём. Ожидаемое
    /// выписано по календарю.
    #[test]
    fn add_months_keeps_the_end_of_month() {
        let cases = [
            (date(2026, 9, 30), 1, Some(date(2026, 10, 31))),
            (date(2026, 9, 30), 5, Some(date(2027, 2, 28))),
            (date(2026, 1, 31), 1, Some(date(2026, 2, 28))),
            (date(2026, 1, 31), 2, Some(date(2026, 3, 31))),
            (date(2026, 1, 30), 1, Some(date(2026, 2, 28))),
            (date(2026, 1, 30), 2, Some(date(2026, 3, 30))),
            (date(2028, 1, 29), 1, Some(date(2028, 2, 29))),
            (date(2027, 2, 28), 12, Some(date(2028, 2, 29))),
            (date(2028, 2, 29), 12, Some(date(2029, 2, 28))),
            (date(2026, 10, 15), 3, Some(date(2027, 1, 15))),
            (date(2026, 10, 15), 0, Some(date(2026, 10, 15))),
            (date(2026, 12, 1), 1, Some(date(2027, 1, 1))),
            (date(9999, 11, 15), 1, Some(date(9999, 12, 15))),
            (date(9999, 12, 15), 1, None),
            (date(2026, 10, 15), u32::MAX, None),
        ];
        let mut wrong = Vec::new();
        for (from, months, expected) in cases {
            let got = from.add_months(months);
            if got != expected {
                wrong.push(format!("{from:?} + {months} мес.: {got:?}, а не {expected:?}"));
            }
        }
        assert!(wrong.is_empty(), "не тот месяц: {wrong:#?}");
    }

    /// Последний день месяца — и в високосном феврале.
    #[test]
    fn last_day_of_the_month() {
        let cases = [
            (date(2026, 2, 10), date(2026, 2, 28)),
            (date(2028, 2, 10), date(2028, 2, 29)),
            (date(2026, 12, 1), date(2026, 12, 31)),
            (date(2026, 4, 30), date(2026, 4, 30)),
        ];
        let mut wrong = Vec::new();
        for (day, last) in cases {
            if day.last_of_month() != last || !last.is_last_of_month() {
                wrong.push(format!("{day:?}: {:?}, а не {last:?}", day.last_of_month()));
            }
        }
        if date(2026, 4, 29).is_last_of_month() {
            wrong.push("29 апреля — последний день".to_owned());
        }
        assert!(wrong.is_empty(), "последний день месяца: {wrong:#?}");
    }

    /// Отладочная запись — ISO 8601 с ведущими нулями.
    #[test]
    fn debug_is_iso() {
        assert_eq!(format!("{:?}", date(2026, 3, 1)), "2026-03-01");
        assert_eq!(format!("{:?}", date(1, 1, 1)), "0001-01-01");
    }

    /// В данных дата — строка ISO, и каждый день 2000–2100 годов и края типа
    /// читаются обратно в тот же день.
    #[test]
    fn data_keeps_dates_as_iso() {
        assert_eq!(
            serde_json::to_string(&date(2026, 3, 1)).expect("дата записывается"),
            "\"2026-03-01\""
        );
        let mut days = vec![date(1, 1, 1), date(9999, 12, 31)];
        for year in 2000..=2100 {
            for month in 1..=12 {
                days.extend((1..=31).filter_map(|day| Date::new(year, month, day)));
            }
        }
        let mut lost = Vec::new();
        for day in days {
            let text = serde_json::to_string(&day).expect("дата записывается");
            match serde_json::from_str::<Date>(&text) {
                Ok(back) if back == day => {}
                other => lost.push(format!("{day:?} → {text} → {other:?}")),
            }
        }
        assert!(lost.is_empty(), "дата не читается обратно: {lost:#?}");
    }

    /// Читается только то, что программа пишет сама, и только дни календаря.
    /// Месяц 0 — тот самый случай, ради которого разбор идёт через
    /// `Date::new`: выведенный `Deserialize` пропустил бы его, и `quarter`
    /// упал бы на `0 - 1`.
    #[test]
    fn data_rejects_what_is_not_an_iso_calendar_day() {
        let rejected = [
            "2026-00-10",
            "2026-13-01",
            "2026-02-29",
            "2026-04-31",
            "2026-01-00",
            "0000-01-01",
            "2026-3-31",
            "2026-03-1",
            "26-03-31",
            "+2026-03-31",
            "-2026-03-31",
            "2026-03-31T00:00:00Z",
            "2026-03-31 ",
            " 2026-03-31",
            "2026/03/31",
            "31.03.2026",
            "20260331",
            "2026-03-3a",
            "２０２６-03-31",
            "",
        ];
        let mut wrong = Vec::new();
        for text in rejected {
            if let Some(day) = Date::from_iso(text) {
                wrong.push(format!("«{text}» принята как {day:?}"));
            }
            let json = serde_json::to_string(text).expect("строка записывается");
            if let Ok(day) = serde_json::from_str::<Date>(&json) {
                wrong.push(format!("{json} прочитана как {day:?}"));
            }
        }
        // Не строка — тоже не дата.
        for json in ["20260331", "null", "{\"year\":2026,\"month\":0,\"day\":10}"] {
            if let Ok(day) = serde_json::from_str::<Date>(json) {
                wrong.push(format!("{json} прочитана как {day:?}"));
            }
        }
        assert!(
            wrong.is_empty(),
            "прочитано то, что датой не является: {wrong:#?}"
        );
    }
}
