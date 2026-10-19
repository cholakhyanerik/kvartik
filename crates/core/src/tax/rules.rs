//! Файл правил: налоговые правила Армении данными — с датами действия
//! и источником каждой записи (задача 18).
//!
//! Правила лежат в `crates/core/rules/am.json`, а не константами в коде
//! (таблица слоёв CLAUDE.md, «Налоги»): правило — данные с источником
//! (Правило 2), его сверяют с законом, не читая кода, а поменялся закон —
//! меняется файл. У каждой страны свой файл: `am` — Армения (ISO 3166-1).
//!
//! # Файл встроен в сборку
//!
//! Через `include_str!`: правила приходят вместе с программой, атомарно
//! с кодом, который их читает, и есть без сети. Разбирается файл один раз,
//! при первом обращении ([`armenia`]). Что он читается и верен, держит тест
//! ядра, а без зелёных тестов сайт не выкладывается (Правило 13): сломанный
//! файл до людей не дойдёт.
//!
//! # Что держит разбор
//!
//! [`Rules::parse`] принимает файл, только если:
//!
//! - у каждой записи есть источник: акт с номером (`ՀՕ-…-Ն`, у решений
//!   правительства и приказов — `N …-Ն`), статья, ссылка на arlis.am или
//!   src.am и дата сверки. Вторичный источник налоговое правило не
//!   подтверждает (Правило 6);
//! - у каждой есть день начала, а день конца — `until`, включительно, —
//!   не раньше него; `null` — действует, конец не назначен. Ключ `until`
//!   пишется всегда: serde прочёл бы пропавший как `null`, и запись, у
//!   которой конец забыли, молча стала бы бессрочной;
//! - две записи об одном и том же не действуют в один день: иначе верную
//!   выбирал бы порядок в файле. Смену правила пишут двумя записями: у
//!   прежней — день конца, у новой — следующий день начала;
//! - значение программа умеет применить: коды записаны так, как их пишет
//!   закон, и в той редакции классификатора, по которой программа читает
//!   коды; граница по году рождения — 1 января;
//! - незнакомых полей нет: опечатка в имени поля — ошибка, а не молча
//!   пропущенное правило.
//!
//! # Версия правил
//!
//! `version` — `2026.1`, `2026.2`…: год и номер правки. Это не версия
//! программы и не версия формата данных (Правило 5). Растёт, когда меняется
//! любое значение — ставка, срок, порог, новая запись или её даты, — но не
//! когда заново сверили источник: правило от этого не меняется. Закрытый
//! квартал хранит версию правил, по которой его посчитали (Правило 2),
//! и забытый подъём ловит тест — он помнит отпечаток значений своей
//! версии.
//!
//! # Новый вид правила
//!
//! Список в файле; здесь — тип записи (поля значения, `from`, `until`,
//! `source`) с реализацией [`Rule`] — о каком предмете запись, — поле
//! в [`Rules`] и строка проверки в [`Rules::parse`]. Новая запись — новая
//! версия правил. Файл читают на любой машине: концы строк и порядок ключей
//! ни разбору, ни отпечатку значений не важны.

use std::fmt;
use std::sync::LazyLock;

use serde::Deserialize;
use serde::de::{self, Deserializer, Visitor};

use crate::model::activity;
use crate::model::date::Date;

/// Файл правил Армении — в сборке.
const ARMENIA: &str = include_str!("../../rules/am.json");

/// Правила Армении, разобранные при первом обращении.
static RULES: LazyLock<Rules> = LazyLock::new(|| match Rules::parse(ARMENIA) {
    Ok(rules) => rules,
    // Не бывает: тот же файл разбирает тест ядра, а без зелёных тестов
    // сайт не выкладывается (шапка модуля).
    Err(error) => panic!("файл правил rules/am.json не прочитан: {error:?}"),
});

/// Правила Армении из файла, встроенного в сборку.
pub fn armenia() -> &'static Rules {
    &RULES
}

/// Версия правил: год и номер правки в нём — `2026.1`. Сравнивается по
/// числам: 2026.10 новее 2026.9.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Version {
    year: u16,
    revision: u16,
}

impl Version {
    /// Версия из записи `2026.1`. `None` — не год из четырёх цифр и номер
    /// от единицы через точку: `2026`, `2026.0`, `2026.01`, `26.1`,
    /// `2026.1.0`. Разбор строгий, как у даты: что пишет файл, то и
    /// читается, а прочее угадывать нельзя.
    pub fn parse(text: &str) -> Option<Self> {
        // Число без знака и без ведущего нуля: «0» тоже не годится — номер
        // правки начинается с единицы, а год из четырёх цифр нулём не
        // начинается.
        fn number(digits: &str) -> Option<u16> {
            let plain = !digits.is_empty()
                && digits.bytes().all(|byte| byte.is_ascii_digit())
                && !digits.starts_with('0');
            plain.then(|| digits.parse().ok()).flatten()
        }

        let (year, revision) = text.split_once('.')?;
        if year.len() != 4 {
            return None;
        }
        Some(Self {
            year: number(year)?,
            revision: number(revision)?,
        })
    }

    /// Год.
    pub const fn year(self) -> u16 {
        self.year
    }

    /// Номер правки в году: с единицы.
    pub const fn revision(self) -> u16 {
        self.revision
    }
}

/// `2026.1` — одна запись на всех языках: цифры и точка.
impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.year, self.revision)
    }
}

/// В файле версия — строка `"2026.1"`, и читается она только через
/// [`Version::parse`].
impl<'de> Deserialize<'de> for Version {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Text;

        impl Visitor<'_> for Text {
            type Value = Version;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("версию правил вида 2026.1")
            }

            fn visit_str<E: de::Error>(self, text: &str) -> Result<Version, E> {
                Version::parse(text).ok_or_else(|| E::invalid_value(de::Unexpected::Str(text), &self))
            }
        }

        deserializer.deserialize_str(Text)
    }
}

/// Откуда правило: акт, статья, ссылка на текст и когда его сверили.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    /// Акт: номер и название, как на arlis.am, — `ՀՕ-165-Ն ՀՀ հարկային
    /// օրենսգիրք`; закон, который ввёл норму, — в скобках.
    pub law: String,
    /// Статья, часть, пункт.
    pub article: String,
    /// Ссылка на текст: arlis.am или src.am.
    pub url: String,
    /// Когда текст сверен.
    pub checked: Date,
}

/// Где лежат тексты законов (arlis.am) и разъяснения ՊԵԿ (src.am). Другой
/// адрес — вторичный источник, а он правило не подтверждает (Правило 6).
const OFFICIAL_HOSTS: [&str; 4] = ["www.arlis.am", "arlis.am", "www.src.am", "src.am"];

impl Source {
    /// Годится ли источником (шапка модуля). Номер нормативного акта
    /// в Армении кончается на `-Ն`: `ՀՕ-165-Ն`, `N 142-Ն`.
    fn is_sound(&self) -> bool {
        let host = self
            .url
            .strip_prefix("https://")
            .and_then(|rest| rest.split('/').next())
            .unwrap_or_default();
        self.law.contains("-Ն") && !self.article.trim().is_empty() && OFFICIAL_HOSTS.contains(&host)
    }
}

/// Когда правило действует: с дня `from` по день `until` включительно.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Period {
    pub from: Date,
    /// `None` — действует, конец не назначен.
    pub until: Option<Date>,
}

impl Period {
    /// Действует ли в день `day`.
    pub fn contains(self, day: Date) -> bool {
        self.from <= day && self.until.is_none_or(|until| day <= until)
    }

    /// Есть ли день, когда действуют оба.
    fn overlaps(self, other: Self) -> bool {
        let ends_before = |until: Option<Date>, from: Date| until.is_some_and(|until| until < from);
        !ends_before(self.until, other.from) && !ends_before(other.until, self.from)
    }
}

/// Запись файла правил — любого вида.
pub trait Rule {
    /// Когда действует.
    fn period(&self) -> Period;

    /// Откуда взята.
    fn source(&self) -> &Source;

    /// Об одном ли она с `other`: такие две не действуют в один день.
    fn same_subject(&self, other: &Self) -> bool;

    /// Умеет ли программа применить её значение (шапка модуля).
    fn usable(&self) -> bool;
}

/// Кто ведёт деятельность с такими кодами, не может быть плательщиком
/// налога с оборота — ни по какой своей деятельности (`tax::turnover`).
///
/// Основание окно называет словами (`Key::ActivityExcluded`: ст. 254,
/// ч. 3, п. 3.1), а из записи берёт только день начала: запись с другим
/// основанием — повод поправить и строку окна.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Exclusion {
    /// Коды, как их пишет закон: раздел (`69`) или группа (`78.2`)
    /// (`model::activity::is_law_prefix`).
    pub codes: Vec<String>,
    /// Классификатор, в редакции которого закон назвал коды: в другой у тех
    /// же цифр бывает другой смысл (`model::activity`, «Риск»).
    pub classifier: String,
    pub from: Date,
    /// `deserialize_with` делает ключ обязательным (шапка модуля): без него
    /// serde подставил бы `None` за пропавший ключ.
    #[serde(deserialize_with = "Option::deserialize")]
    pub until: Option<Date>,
    pub source: Source,
}

impl Rule for Exclusion {
    fn period(&self) -> Period {
        Period {
            from: self.from,
            until: self.until,
        }
    }

    fn source(&self) -> &Source {
        &self.source
    }

    /// Об одном — если коды пересекаются: `69` и `69`, `78` и `78.2`.
    /// Записи кодов закона — одной формы (`usable`), и начало строки
    /// здесь — начало кода.
    fn same_subject(&self, other: &Self) -> bool {
        self.codes.iter().any(|code| {
            other
                .codes
                .iter()
                .any(|another| code.starts_with(another.as_str()) || another.starts_with(code.as_str()))
        })
    }

    fn usable(&self) -> bool {
        self.classifier == activity::CLASSIFIER
            && !self.codes.is_empty()
            && self.codes.iter().all(|code| activity::is_law_prefix(code))
    }
}

/// Кто платит соцплатёж накопительной пенсии по закону, а кто — по своему
/// выбору (`tax::annual`).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FundedPension {
    /// Родившиеся в этот день и позже платят обязательно; раньше — только
    /// если подали заявление о выборе пенсионного фонда.
    pub mandatory_born_from: Date,
    pub from: Date,
    /// Ключ обязателен, как у [`Exclusion::until`].
    #[serde(deserialize_with = "Option::deserialize")]
    pub until: Option<Date>,
    pub source: Source,
}

impl Rule for FundedPension {
    fn period(&self) -> Period {
        Period {
            from: self.from,
            until: self.until,
        }
    }

    fn source(&self) -> &Source {
        &self.source
    }

    /// Граница одна на всех: две записи — всегда об одном.
    fn same_subject(&self, _: &Self) -> bool {
        true
    }

    /// Граница — 1 января: профиль хранит только год рождения
    /// (`model::profile::Person`).
    fn usable(&self) -> bool {
        self.mandatory_born_from.month() == 1 && self.mandatory_born_from.day() == 1
    }
}

/// Правила страны — файл целиком.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    version: Version,
    turnover_exclusions: Vec<Exclusion>,
    funded_pension: Vec<FundedPension>,
}

/// Какая запись: список файла и номер в нём, с нуля.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct At {
    pub kind: &'static str,
    pub index: usize,
}

/// Почему файл правил не принят. Человек этих слов не видит: файл встроен
/// в сборку, и раньше людей их видит тест ядра.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum RulesError {
    /// Не JSON, не те поля или типы, дата не из календаря, версия не
    /// `ГГГГ.N`, запись без источника. В строке — подробность разбора.
    Malformed(String),
    /// Источник не годится: акт без номера `…-Ն`, пустая статья, ссылка не
    /// на arlis.am или src.am.
    Unsourced(At),
    /// День конца раньше дня начала.
    Backwards(At),
    /// Значение программа применить не умеет (шапка модуля).
    Unusable(At),
    /// Две записи об одном действуют в один день.
    Overlap {
        kind: &'static str,
        first: usize,
        second: usize,
    },
}

impl Rules {
    /// Разобрать файл правил и проверить его (шапка модуля).
    pub fn parse(text: &str) -> Result<Self, RulesError> {
        let rules: Self =
            serde_json::from_str(text).map_err(|error| RulesError::Malformed(error.to_string()))?;
        check("turnover_exclusions", &rules.turnover_exclusions)?;
        check("funded_pension", &rules.funded_pension)?;
        Ok(rules)
    }

    /// Версия правил.
    pub const fn version(&self) -> Version {
        self.version
    }

    /// Кто не может быть плательщиком налога с оборота по виду
    /// деятельности.
    pub fn turnover_exclusions(&self) -> &[Exclusion] {
        &self.turnover_exclusions
    }

    /// Граница обязательной накопительной пенсии.
    pub fn funded_pension(&self) -> &[FundedPension] {
        &self.funded_pension
    }
}

/// Записи одного вида: каждая — с источником, днём конца не раньше начала
/// и значением, которое программа умеет применить, а записи об одном не
/// действуют в один день.
fn check<R: Rule>(kind: &'static str, records: &[R]) -> Result<(), RulesError> {
    for (index, record) in records.iter().enumerate() {
        let at = At { kind, index };
        if !record.source().is_sound() {
            return Err(RulesError::Unsourced(at));
        }
        let period = record.period();
        if period.until.is_some_and(|until| until < period.from) {
            return Err(RulesError::Backwards(at));
        }
        if !record.usable() {
            return Err(RulesError::Unusable(at));
        }
    }
    for (first, record) in records.iter().enumerate() {
        for (second, other) in records.iter().enumerate().skip(first + 1) {
            if record.same_subject(other) && record.period().overlaps(other.period()) {
                return Err(RulesError::Overlap {
                    kind,
                    first,
                    second,
                });
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
