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
//! о копии через 7 дней (задача 7). Сложение дней и день недели были
//! в пробе и добавятся с задачами, которым они нужны: срок оплаты счёта,
//! перенос срока на рабочий день. Дни считает алгоритм `days_from_civil`
//! Говарда Хиннанта (`day_number` ниже) — на нём строить и их.

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
    /// с источником в файле правил (задача 18), а не константа в коде
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
