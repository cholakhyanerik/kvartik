//! Страна — код ISO 3166-1 alpha-2 и название на трёх языках (задача 10).
//!
//! Решение человека 2026-10-12: в данных — код (`"AM"`), а названия —
//! своя таблица в ядре, из CLDR. Не названия `Intl.DisplayNames` браузера:
//! они нужны и PDF (задача 12), а у браузеров разные. И не «Армения /
//! другая страна»: в счёте иностранному клиенту страна — часть адреса.
//!
//! Таблицу пишет `tools/countries.py` в `country/names.rs`: 249 назначенных
//! кодов ISO 3166-1, названия CLDR 48.2.3 и порядок по названию на каждом
//! языке — что берётся и почему, в шапке скрипта. Знаки названий есть
//! в шрифтах окна — это держит тест `crates/app/tests/fonts.rs`.
//!
//! **Новый код в таблице — новая версия формата** (Правило 5): код
//! читается, только если он есть в таблице, и прежняя версия программы
//! назвала бы запись с новым кодом испорченной, а не новее себя.
//! Переименование страны в CLDR формата не меняет: в данных лежит код.
//!
//! В данных — строка `"AM"`.

mod names;

use std::fmt;

use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize, Serializer};

use crate::i18n::Lang;
use names::{BY_NAME_EN, BY_NAME_HY, BY_NAME_RU, COUNTRIES};

/// Строка таблицы: код и названия на трёх языках.
struct Names {
    code: [u8; 2],
    hy: &'static str,
    ru: &'static str,
    en: &'static str,
}

/// Страна из таблицы — номер её строки. Номер живёт только в памяти:
/// в данные идёт код, и новая строка в середине таблицы ничего не ломает.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Country(u8);

impl Country {
    /// Армения — страна, у фирм которой есть ՀՎՀՀ.
    pub const ARMENIA: Self = match Self::find(*b"AM") {
        Some(country) => country,
        None => panic!("в таблице стран нет Армении"),
    };

    /// Страна по коду ISO 3166-1 alpha-2: две заглавные латинские буквы,
    /// как в данных. `None` — такой страны в таблице нет.
    pub const fn from_code(code: &str) -> Option<Self> {
        match code.as_bytes() {
            &[a, b] => Self::find([a, b]),
            _ => None,
        }
    }

    /// Двоичный поиск по таблице: она идёт по алфавиту кодов.
    const fn find(code: [u8; 2]) -> Option<Self> {
        let (mut low, mut high) = (0, COUNTRIES.len());
        while low < high {
            let middle = (low + high) / 2;
            let at = COUNTRIES[middle].code;
            if at[0] == code[0] && at[1] == code[1] {
                // Строк в таблице 249 — номер помещается в `u8` (это
                // проверяет `BY_NAME_*`: их тип — `[u8; 249]`).
                return Some(Self(middle as u8));
            }
            if at[0] < code[0] || (at[0] == code[0] && at[1] < code[1]) {
                low = middle + 1;
            } else {
                high = middle;
            }
        }
        None
    }

    const fn names(self) -> &'static Names {
        &COUNTRIES[self.0 as usize]
    }

    /// Код ISO 3166-1 alpha-2: `"AM"`.
    pub fn code(self) -> &'static str {
        // Коды в таблице — две заглавные латинские буквы: их пишет
        // `tools/countries.py`, и это проверяет тест ниже.
        std::str::from_utf8(&self.names().code).unwrap_or("??")
    }

    /// Название страны на языке `lang`: «Հայաստան», «Армения», «Armenia».
    pub const fn name(self, lang: Lang) -> &'static str {
        let names = self.names();
        match lang {
            Lang::Am => names.hy,
            Lang::Ru => names.ru,
            Lang::En => names.en,
        }
    }

    /// Все страны по алфавиту названий на языке `lang` — для списка выбора.
    /// Порядок посчитан заранее, скриптом (шапка модуля): на смену языка
    /// окно ничего не сортирует.
    pub fn by_name(lang: Lang) -> impl Iterator<Item = Self> {
        let order: &'static [u8] = match lang {
            Lang::Am => &BY_NAME_HY,
            Lang::Ru => &BY_NAME_RU,
            Lang::En => &BY_NAME_EN,
        };
        order.iter().map(|&row| Self(row))
    }
}

/// Код, как в данных: `AM`.
impl fmt::Display for Country {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

impl Serialize for Country {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.code())
    }
}

/// Читается только код из таблицы — в том виде, в каком его пишет
/// программа: `"am"` или `"ARM"` — порча.
impl<'de> Deserialize<'de> for Country {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::from_code(&text).ok_or_else(|| {
            de::Error::invalid_value(de::Unexpected::Str(&text), &"код страны ISO 3166-1 alpha-2")
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// Таблица годна: коды — две заглавные буквы по алфавиту без повторов,
    /// у каждой страны есть название на каждом языке, без пробелов по краям.
    #[test]
    fn table_is_well_formed() {
        let mut wrong = Vec::new();
        for pair in COUNTRIES.windows(2) {
            if pair[0].code >= pair[1].code {
                wrong.push(format!("порядок кодов: {:?} {:?}", pair[0].code, pair[1].code));
            }
        }
        for row in &COUNTRIES {
            if !row.code.iter().all(u8::is_ascii_uppercase) {
                wrong.push(format!("код {:?}", row.code));
            }
            for name in [row.hy, row.ru, row.en] {
                if name.is_empty() || name.trim() != name {
                    wrong.push(format!("{:?}: название «{name}»", row.code));
                }
            }
        }
        assert!(wrong.is_empty(), "таблица стран: {wrong:#?}");
    }

    /// Каждый код читается обратно в свою страну, а чужое — нет.
    #[test]
    fn codes_round_trip() {
        let mut wrong = Vec::new();
        for row in 0..COUNTRIES.len() {
            let country = Country(row as u8);
            if Country::from_code(country.code()) != Some(country) {
                wrong.push(country.code());
            }
        }
        assert!(wrong.is_empty(), "код не читается обратно: {wrong:?}");
        for code in ["", "A", "am", "Am", "ARM", "ZZ", "XK", "EU", "SU", "AC", "А М", "Ա"] {
            assert_eq!(Country::from_code(code), None, "«{code}» прочитан");
        }
    }

    /// Названия выписаны человеком из CLDR 48.2.3 (unicode-org/cldr-json,
    /// `territories.json`), а не взяты из той же таблицы.
    #[test]
    fn names_are_cldr() {
        let cases = [
            ("AM", "Հայաստան", "Армения", "Armenia"),
            ("RU", "Ռուսաստան", "Россия", "Russia"),
            ("US", "Միացյալ Նահանգներ", "Соединенные Штаты", "United States"),
            ("CI", "Կոտ դ՚Իվուար", "Кот-д’Ивуар", "Côte d’Ivoire"),
            ("TR", "Թուրքիա", "Турция", "Türkiye"),
        ];
        let mut wrong = Vec::new();
        for (code, hy, ru, en) in cases {
            let Some(country) = Country::from_code(code) else {
                wrong.push(format!("{code}: нет в таблице"));
                continue;
            };
            let got = (country.name(Lang::Am), country.name(Lang::Ru), country.name(Lang::En));
            if got != (hy, ru, en) {
                wrong.push(format!("{code}: {got:?}"));
            }
        }
        assert_eq!(Country::ARMENIA.code(), "AM");
        assert!(wrong.is_empty(), "названия: {wrong:#?}");
    }

    /// Список выбора на каждом языке — все страны ровно по разу, по
    /// алфавиту. Соседи выписаны человеком по алфавиту языка: «Йемен» —
    /// после «Исландии» (у «й» своё место), «Åland» — среди «A».
    #[test]
    fn lists_are_complete_and_alphabetical() {
        let mut wrong = Vec::new();
        for lang in Lang::ALL {
            let listed: Vec<Country> = Country::by_name(lang).collect();
            let distinct: BTreeSet<u8> = listed.iter().map(|country| country.0).collect();
            if listed.len() != COUNTRIES.len() || distinct.len() != COUNTRIES.len() {
                wrong.push(format!("{lang:?}: стран {}, разных {}", listed.len(), distinct.len()));
            }
        }
        let neighbours = [
            (Lang::Ru, "Исландия", "Испания"),
            (Lang::Ru, "Италия", "Йемен"),
            (Lang::Ru, "Йемен", "Кабо-Верде"),
            (Lang::En, "Afghanistan", "Åland Islands"),
            (Lang::En, "Åland Islands", "Albania"),
            (Lang::Am, "Աֆղանստան", "Բահամյան կղզիներ"),
        ];
        for (lang, first, second) in neighbours {
            let names: Vec<&str> = Country::by_name(lang).map(|c| c.name(lang)).collect();
            let at = |name: &str| names.iter().position(|n| *n == name);
            match (at(first), at(second)) {
                (Some(a), Some(b)) if b == a + 1 => {}
                other => wrong.push(format!("{lang:?}: «{first}», «{second}» — {other:?}")),
            }
        }
        assert!(wrong.is_empty(), "списки стран: {wrong:#?}");
    }

    /// В данных — строка кода; другой регистр, трёхбуквенный код и код вне
    /// таблицы — порча.
    #[test]
    fn data_keeps_the_code() {
        let armenia = Country::ARMENIA;
        assert_eq!(serde_json::to_string(&armenia).ok().as_deref(), Some(r#""AM""#));
        assert_eq!(serde_json::from_str::<Country>(r#""AM""#).ok(), Some(armenia));
        for json in [r#""am""#, r#""ARM""#, r#""XK""#, r#""""#, "51", "null"] {
            assert!(serde_json::from_str::<Country>(json).is_err(), "{json} прочитан");
        }
    }
}
