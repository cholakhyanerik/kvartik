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
//! Пока здесь только то, что нужно сейчас: проверка даты, квартал и день
//! месяца после квартала. Сложение дней, день недели и запись ISO
//! (`2026-10-10`) были в пробе и добавятся с задачами, которым они нужны:
//! срок оплаты счёта, перенос срока на рабочий день, файл копии. В пробе дни
//! считал алгоритм `days_from_civil` Говарда Хиннанта — его и брать.

use std::fmt;

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

    /// Квартал, в который попадает дата: январь — март — первый, октябрь —
    /// декабрь — четвёртый.
    pub const fn quarter(self) -> Quarter {
        Quarter {
            year: self.year,
            number: (self.month - 1) / 3 + 1,
        }
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

    /// Отладочная запись — ISO 8601 с ведущими нулями.
    #[test]
    fn debug_is_iso() {
        assert_eq!(format!("{:?}", date(2026, 3, 1)), "2026-03-01");
        assert_eq!(format!("{:?}", date(1, 1, 1)), "0001-01-01");
    }
}
