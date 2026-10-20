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
//!   коды; граница по году рождения — 1 января; у ставки вычет расходов
//!   и нижний предел — вместе или никак, а предел ниже ставки; порог больше
//!   нуля; платёж банка у расходов — в своём списке, а не среди документов;
//!   проценты — числом закона: `4.5`, а не `4,5` и не `4.50`; у годовых
//!   платежей суммы больше нуля, срок есть в каждом году (не 29 февраля),
//!   границы ступеней гербового сбора растут, а без границы — только
//!   последняя; у медстраховки возраст больше нуля, порог оборота не
//!   отрицательный, а число месяца у помесячного срока есть в каждом
//!   месяце (не позже 28-го); у сроков (задача 21) число месяца — в каждом
//!   месяце, день года — в каждом году, выходные — дни недели от 1 до 7 без
//!   повторов и не вся неделя, праздники — без повторов, а дни решения
//!   правительства — в его периоде и каждый в одном списке;
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

use serde::de::{self, Deserializer, Visitor};
use serde::{Deserialize, Serialize, Serializer};

use crate::model::activity::{self, ActivityCode, IncomeKind};
use crate::model::date::Date;
use crate::model::ledger::{Nature, Proof};

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

/// В файле и в снимке закрытого квартала (`tax::closing`) версия — строка
/// `"2026.1"`.
impl Serialize for Version {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

/// Версия читается только через [`Version::parse`].
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

/// Процент — сотыми долями процента: 4,5 % — 450. В файле правил
/// и в снимке квартала — строкой, как пишет закон: `"4.5"`, `"10"`.
///
/// Целое, а не `f64`: процент умножает драмы, а деньги в `f64` запрещены
/// (Правило 2) — 4,5 % от 1 234 567 ֏ в `f64` легли бы на последний драм
/// по-разному в разных местах.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Percent(u16);

impl Percent {
    /// Сто процентов.
    pub const WHOLE: Self = Self(10_000);

    /// Процент из записи `4.5`, `10`, `1.5`. `None` — не число от 0 до 100
    /// с одним-двумя знаками после точки, ведущий ноль, ноль в конце дробной
    /// части, знак, пробел или запятая: что пишет закон, то и читается,
    /// а прочее угадывать нельзя.
    pub fn parse(text: &str) -> Option<Self> {
        let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
        let digits = |part: &str| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit());
        if !digits(whole) || (whole.len() > 1 && whole.starts_with('0')) || whole.len() > 3 {
            return None;
        }
        if text.contains('.') && (!digits(fraction) || fraction.len() > 2 || fraction.ends_with('0')) {
            return None;
        }
        let whole: u16 = whole.parse().ok()?;
        let fraction: u16 = match fraction.len() {
            0 => 0,
            1 => fraction.parse::<u16>().ok()? * 10,
            _ => fraction.parse().ok()?,
        };
        let hundredths = whole.checked_mul(100)?.checked_add(fraction)?;
        (hundredths <= Self::WHOLE.0).then_some(Self(hundredths))
    }

    /// Сотые доли процента: 4,5 % — 450.
    pub const fn hundredths(self) -> u16 {
        self.0
    }

    /// Доля суммы в драмах — целыми драмами, лумы отброшены: строки формы
    /// расчёта — целые драмы, «без лум» (приказ ՊԵԿ 299-Ն, прил. 1, п. 1;
    /// НК, ст. 318, ч. 1; `model::money::Lumas::drams`). 10 % от
    /// 1 234 567 ֏ — 123 456, а не 123 457 (вопрос 9.1 листа сверки).
    pub fn of(self, drams: i64) -> i64 {
        let share = i128::from(drams) * i128::from(self.0) / i128::from(Self::WHOLE.0);
        // Доля не больше самой суммы: помещается.
        i64::try_from(share).unwrap_or(i64::MAX)
    }
}

/// `4.5` — запись закона: точка, без нулей в конце.
impl fmt::Display for Percent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (whole, fraction) = (self.0 / 100, self.0 % 100);
        match fraction {
            0 => write!(f, "{whole}"),
            _ if fraction % 10 == 0 => write!(f, "{whole}.{}", fraction / 10),
            _ => write!(f, "{whole}.{fraction:02}"),
        }
    }
}

impl Serialize for Percent {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

/// Процент читается только через [`Percent::parse`].
impl<'de> Deserialize<'de> for Percent {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Text;

        impl Visitor<'_> for Text {
            type Value = Percent;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("процент вида 4.5")
            }

            fn visit_str<E: de::Error>(self, text: &str) -> Result<Percent, E> {
                Percent::parse(text).ok_or_else(|| E::invalid_value(de::Unexpected::Str(text), &self))
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

/// Ставка строки таблицы НК, ст. 258, ч. 1 — и, у строк с вычетом расходов
/// (ч. 2–5), процент расходов и нижний предел (`tax::turnover`).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rate {
    /// Строка таблицы — вид дохода.
    pub kind: IncomeKind,
    /// Ставка — процент дохода.
    pub rate: Percent,
    /// Сколько процентов расходов квартала уменьшает налог; `None` —
    /// вычета нет, налог — ставка, умноженная на доход (ст. 260, ч. 1).
    /// Ключ обязателен, как у [`Exclusion::until`].
    #[serde(deserialize_with = "Option::deserialize")]
    pub deduction: Option<Percent>,
    /// Нижний предел налога — процент дохода; есть ровно у строк с вычетом.
    #[serde(deserialize_with = "Option::deserialize")]
    pub floor: Option<Percent>,
    pub from: Date,
    #[serde(deserialize_with = "Option::deserialize")]
    pub until: Option<Date>,
    pub source: Source,
}

impl Rule for Rate {
    fn period(&self) -> Period {
        Period {
            from: self.from,
            until: self.until,
        }
    }

    fn source(&self) -> &Source {
        &self.source
    }

    /// Об одном — если о той же строке.
    fn same_subject(&self, other: &Self) -> bool {
        self.kind == other.kind
    }

    /// Ставка больше нуля; вычет и нижний предел — вместе или никак;
    /// предел ниже ставки — иначе вычет не применялся бы никогда.
    fn usable(&self) -> bool {
        let rate = self.rate.hundredths() > 0;
        match (self.deduction, self.floor) {
            (None, None) => rate,
            (Some(deduction), Some(floor)) => rate && deduction.hundredths() > 0 && floor < self.rate,
            (Some(_), None) | (None, Some(_)) => false,
        }
    }
}

/// Список видов деятельности высоких технологий — строка 8 таблицы ст. 258,
/// ч. 1 (`tax::turnover::kind_of`): по коду закон узнаёт только её.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HighTech {
    /// Коды, как их пишет список: подкласс `62.01.0`, без буквы раздела
    /// (`model::activity::is_law_prefix`).
    pub codes: Vec<String>,
    /// Классификатор, в редакции которого список назвал коды (как
    /// у [`Exclusion::classifier`]).
    pub classifier: String,
    pub from: Date,
    #[serde(deserialize_with = "Option::deserialize")]
    pub until: Option<Date>,
    pub source: Source,
}

impl HighTech {
    /// В списке ли код.
    pub fn contains(&self, code: &ActivityCode) -> bool {
        self.codes.iter().any(|prefix| code.starts_with(prefix))
    }
}

impl Rule for HighTech {
    fn period(&self) -> Period {
        Period {
            from: self.from,
            until: self.until,
        }
    }

    fn source(&self) -> &Source {
        &self.source
    }

    /// Список один на всех: две записи — всегда об одном.
    fn same_subject(&self, _: &Self) -> bool {
        true
    }

    fn usable(&self) -> bool {
        self.classifier == activity::CLASSIFIER
            && !self.codes.is_empty()
            && self.codes.iter().all(|code| activity::is_law_prefix(code))
    }
}

/// Порог оборота всех видов за год (НК, ст. 254, ч. 2; ст. 255, ч. 1, п. 2;
/// ст. 59, ч. 1, п. 5).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Threshold {
    /// Порог в драмах: оборот больше него — не плательщик налога с оборота.
    pub amount: i64,
    pub from: Date,
    #[serde(deserialize_with = "Option::deserialize")]
    pub until: Option<Date>,
    pub source: Source,
}

impl Rule for Threshold {
    fn period(&self) -> Period {
        Period {
            from: self.from,
            until: self.until,
        }
    }

    fn source(&self) -> &Source {
        &self.source
    }

    /// Порог один на всех: две записи — всегда об одном.
    fn same_subject(&self, _: &Self) -> bool {
        true
    }

    fn usable(&self) -> bool {
        self.amount > 0
    }
}

/// Какой расход книги идёт в вычет (НК, ст. 258, ч. 6; ст. 55, ч. 2, 12 и 13;
/// `tax::turnover`): документ продавца (`model::ledger::Proof`) и вид
/// расхода (`Nature`).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Expenses {
    /// Документы, которыми подтверждается любой вычитаемый расход: расчётные
    /// документы ст. 55, ч. 2, п. 1–5, и иные документы ч. 13.
    pub proofs: Vec<Proof>,
    /// Виды расхода, которым хватает одного платёжного документа банка
    /// (`Proof::BankPayment`; ст. 55, ч. 12).
    pub payment_only: Vec<Nature>,
    /// Виды расхода, которые в вычет не идут ни с каким документом
    /// (ст. 258, ч. 6, п. 2).
    pub excluded: Vec<Nature>,
    pub from: Date,
    #[serde(deserialize_with = "Option::deserialize")]
    pub until: Option<Date>,
    pub source: Source,
}

impl Rule for Expenses {
    fn period(&self) -> Period {
        Period {
            from: self.from,
            until: self.until,
        }
    }

    fn source(&self) -> &Source {
        &self.source
    }

    /// Правило одно на всех: две записи — всегда об одном.
    fn same_subject(&self, _: &Self) -> bool {
        true
    }

    /// Документы есть; платёж банка и «документа нет» — не среди них:
    /// платёжу отведён свой список, а без документа вычета нет; вид
    /// расхода не стоит и среди тех, кому хватает платежа, и среди
    /// исключённых; повторов нет.
    fn usable(&self) -> bool {
        let unique = |list: &[Nature]| list.iter().enumerate().all(|(i, a)| !list[..i].contains(a));
        let proofs_unique = self.proofs.iter().enumerate().all(|(i, a)| !self.proofs[..i].contains(a));
        !self.proofs.is_empty()
            && proofs_unique
            && !self.proofs.contains(&Proof::BankPayment)
            && !self.proofs.contains(&Proof::None)
            && unique(&self.payment_only)
            && unique(&self.excluded)
            && !self.payment_only.iter().any(|nature| self.excluded.contains(nature))
    }
}

/// День года, следующего за годом платежа, — срок годового платежа: «до
/// 20 апреля следующего года» — `{"month":4,"day":20}`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NextYearDay {
    pub month: u8,
    pub day: u8,
}

impl NextYearDay {
    /// Этот день в году после `year`; `None` — календарь кончился.
    pub fn after(self, year: u16) -> Option<Date> {
        Date::new(year.checked_add(1)?, self.month, self.day)
    }

    /// Есть ли такой день в каждом году: 29 февраля — не в каждом,
    /// а 2023-й — не високосный.
    fn usable(self) -> bool {
        Date::new(2023, self.month, self.day).is_some()
    }
}

/// Фиксированный платёж ИП на налоге с оборота за каждый месяц года
/// (`tax::annual`): налог на прибыль и соцплатёж. Платится за год одной
/// суммой — до дня `due` следующего года.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Monthly {
    /// Сумма за месяц, драмы.
    pub monthly: i64,
    /// Срок уплаты за год.
    pub due: NextYearDay,
    pub from: Date,
    /// Ключ обязателен, как у [`Exclusion::until`].
    #[serde(deserialize_with = "Option::deserialize")]
    pub until: Option<Date>,
    pub source: Source,
}

impl Rule for Monthly {
    fn period(&self) -> Period {
        Period {
            from: self.from,
            until: self.until,
        }
    }

    fn source(&self) -> &Source {
        &self.source
    }

    /// Сумма одна на всех: две записи — всегда об одном.
    fn same_subject(&self, _: &Self) -> bool {
        true
    }

    fn usable(&self) -> bool {
        self.monthly > 0 && self.due.usable()
    }
}

/// Ступень шкалы гербового сбора: база до `up_to` драмов включительно —
/// `amount` драмов.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    /// Верхняя граница базы, включительно; `None` — без границы: последняя
    /// ступень. Ключ обязателен, как у [`Exclusion::until`].
    #[serde(deserialize_with = "Option::deserialize")]
    pub up_to: Option<i64>,
    /// Сбор, драмы.
    pub amount: i64,
}

/// Гербовый сбор ИП на налоге с оборота за год (`tax::annual`): ступени
/// по обороту года — так, как их пишет закон, — и срок уплаты.
///
/// Запись — об отчётном годе: её период — годы, к обязательствам которых
/// она применяется, а не дни, когда действовала редакция закона. Новая
/// шкала для ИП вступила 2026-01-01, хотя в сводной редакции стоит
/// с 25.12.2025 — с этого дня она действует только для работников (закон
/// ՀՕ-477-Ն, ст. 2).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StampDuty {
    /// Ступени по возрастанию границы; у последней границы нет.
    pub steps: Vec<Step>,
    /// Срок уплаты за год.
    pub due: NextYearDay,
    pub from: Date,
    /// Ключ обязателен, как у [`Exclusion::until`].
    #[serde(deserialize_with = "Option::deserialize")]
    pub until: Option<Date>,
    pub source: Source,
}

impl StampDuty {
    /// Номер ступени для базы `base` драмов.
    pub fn step(&self, base: i64) -> Option<usize> {
        self.steps
            .iter()
            .position(|step| step.up_to.is_none_or(|up_to| base <= up_to))
    }
}

impl Rule for StampDuty {
    fn period(&self) -> Period {
        Period {
            from: self.from,
            until: self.until,
        }
    }

    fn source(&self) -> &Source {
        &self.source
    }

    /// Шкала одна на всех: две записи — всегда об одном.
    fn same_subject(&self, _: &Self) -> bool {
        true
    }

    /// Ступени есть; границы растут, а без границы — только последняя;
    /// сборы больше нуля; срок есть в каждом году.
    fn usable(&self) -> bool {
        let Some((last, rest)) = self.steps.split_last() else {
            return false;
        };
        let bounds: Option<Vec<i64>> = rest.iter().map(|step| step.up_to).collect();
        let rising = bounds.is_some_and(|bounds| {
            bounds.first().is_none_or(|first| *first >= 0) && bounds.windows(2).all(|pair| pair[0] < pair[1])
        });
        rising && last.up_to.is_none() && self.steps.iter().all(|step| step.amount > 0) && self.due.usable()
    }
}

/// Как платится взнос медстраховки за год (`HealthInsurance::by`). В файле —
/// `{"year":{"income_over":2400000,"due":"2026-07-20"}}` или
/// `{"month":{"due_day":20}}`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum HealthBy {
    /// За год одной суммой — годовая ставка, двенадцать месячных, — до дня
    /// `due`; платит ИП, у которого оборот прошлого года по расчётам налога
    /// с оборота больше `income_over` драмов.
    Year { income_over: i64, due: Date },
    /// Помесячно — до числа `due_day` месяца, следующего за месяцем.
    Month { due_day: u8 },
}

/// Взнос всеобщей медстраховки ИП за год (`tax::annual`).
///
/// Закон вступает для ИП по частям (ст. 45, ч. 1, п. 5 и 6): с дня
/// `starts`, а у того, чья деятельность в этот день остановлена, — с месяца
/// после возобновления. Запись, как и у гербового сбора, — об отчётном
/// годе.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HealthInsurance {
    /// Взнос за месяц, драмы.
    pub monthly: i64,
    /// С какого возраста взнос за гражданина-резидента платит государство.
    pub state_pays_from_age: u8,
    /// С какого дня закон действует для ИП этой записи.
    pub starts: Date,
    pub by: HealthBy,
    pub from: Date,
    /// Ключ обязателен, как у [`Exclusion::until`].
    #[serde(deserialize_with = "Option::deserialize")]
    pub until: Option<Date>,
    pub source: Source,
}

impl Rule for HealthInsurance {
    fn period(&self) -> Period {
        Period {
            from: self.from,
            until: self.until,
        }
    }

    fn source(&self) -> &Source {
        &self.source
    }

    /// Взнос один на всех: две записи — всегда об одном.
    fn same_subject(&self, _: &Self) -> bool {
        true
    }

    /// Взнос и возраст больше нуля; порог не отрицательный; число месяца
    /// есть в каждом месяце.
    fn usable(&self) -> bool {
        let by = match self.by {
            HealthBy::Year { income_over, .. } => income_over >= 0,
            HealthBy::Month { due_day } => (1..=28).contains(&due_day),
        };
        self.monthly > 0 && self.state_pays_from_age > 0 && by
    }
}

/// День года — срок, который каждый год приходится на одно число:
/// «до 20 февраля данного года» — `{"month":2,"day":20}`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct YearDay {
    pub month: u8,
    pub day: u8,
}

impl YearDay {
    /// Этот день в году `year`.
    pub fn of(self, year: u16) -> Option<Date> {
        Date::new(year, self.month, self.day)
    }

    /// Есть ли такой день в каждом году: 29 февраля — не в каждом.
    fn usable(self) -> bool {
        Date::new(2023, self.month, self.day).is_some()
    }
}

/// Срок расчёта и налога с оборота за квартал (`tax::deadlines`): число
/// `day` месяца, следующего за кварталом.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TurnoverDue {
    pub day: u8,
    pub from: Date,
    /// Ключ обязателен, как у [`Exclusion::until`].
    #[serde(deserialize_with = "Option::deserialize")]
    pub until: Option<Date>,
    pub source: Source,
}

impl Rule for TurnoverDue {
    fn period(&self) -> Period {
        Period {
            from: self.from,
            until: self.until,
        }
    }

    fn source(&self) -> &Source {
        &self.source
    }

    /// Срок один на всех: две записи — всегда об одном.
    fn same_subject(&self, _: &Self) -> bool {
        true
    }

    /// Число есть в каждом месяце: не позже 28-го.
    fn usable(&self) -> bool {
        (1..=28).contains(&self.day)
    }
}

/// Срок заявления о налоге с оборота на год (`tax::deadlines`): день `due`
/// того же года, а у нового ИП и у того, кто остановил деятельность до
/// этого дня, — через `after_days` дней после дня учёта или возобновления.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TurnoverApplication {
    pub due: YearDay,
    pub after_days: u8,
    pub from: Date,
    /// Ключ обязателен, как у [`Exclusion::until`].
    #[serde(deserialize_with = "Option::deserialize")]
    pub until: Option<Date>,
    pub source: Source,
}

impl Rule for TurnoverApplication {
    fn period(&self) -> Period {
        Period {
            from: self.from,
            until: self.until,
        }
    }

    fn source(&self) -> &Source {
        &self.source
    }

    /// Срок один на всех: две записи — всегда об одном.
    fn same_subject(&self, _: &Self) -> bool {
        true
    }

    fn usable(&self) -> bool {
        self.due.usable() && self.after_days > 0
    }
}

/// Всеобщая декларация о доходах за отчётный год (`tax::deadlines`):
/// последний день — день `due` следующего года. Подают резиденты —
/// граждане Армении на 31 декабря отчётного года, кроме несовершеннолетних
/// (НК, ст. 156, ч. 4.1); доход ИП подоходным налогом не облагается (ст.
/// 141, ч. 1, п. 1), но декларацию гражданин подаёт всё равно. Запись, как
/// у гербового сбора, — об отчётном годе.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IncomeDeclaration {
    pub due: NextYearDay,
    pub from: Date,
    /// Ключ обязателен, как у [`Exclusion::until`].
    #[serde(deserialize_with = "Option::deserialize")]
    pub until: Option<Date>,
    pub source: Source,
}

impl Rule for IncomeDeclaration {
    fn period(&self) -> Period {
        Period {
            from: self.from,
            until: self.until,
        }
    }

    fn source(&self) -> &Source {
        &self.source
    }

    /// Срок один на всех: две записи — всегда об одном.
    fn same_subject(&self, _: &Self) -> bool {
        true
    }

    fn usable(&self) -> bool {
        self.due.usable()
    }
}

/// Перенос срока (`tax::deadlines`): последний день срока пришёлся на
/// нерабочий — срок кончается в следующий рабочий. Нерабочие — дни недели
/// `weekend` (по ISO 8601: понедельник — 1, воскресенье — 7), праздники
/// ([`Holidays`]) и дни, которые правительство объявило нерабочими
/// ([`DaysOff`]).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeadlineShift {
    pub weekend: Vec<u8>,
    pub from: Date,
    /// Ключ обязателен, как у [`Exclusion::until`].
    #[serde(deserialize_with = "Option::deserialize")]
    pub until: Option<Date>,
    pub source: Source,
}

impl Rule for DeadlineShift {
    fn period(&self) -> Period {
        Period {
            from: self.from,
            until: self.until,
        }
    }

    fn source(&self) -> &Source {
        &self.source
    }

    /// Правило одно на всех: две записи — всегда об одном.
    fn same_subject(&self, _: &Self) -> bool {
        true
    }

    /// Дни недели — от 1 до 7, без повторов, и рабочий день в неделе есть:
    /// иначе перенос не кончился бы никогда.
    fn usable(&self) -> bool {
        let unique = self.weekend.iter().enumerate().all(|(i, a)| !self.weekend[..i].contains(a));
        !self.weekend.is_empty()
            && self.weekend.len() < 7
            && unique
            && self.weekend.iter().all(|day| (1..=7).contains(day))
    }
}

/// Нерабочие дни закона о праздниках (`tax::deadlines`) — днями года: закон
/// называет их числом и месяцем, и праздник, выпавший на выходной, никуда не
/// переносится.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Holidays {
    pub days: Vec<YearDay>,
    pub from: Date,
    /// Ключ обязателен, как у [`Exclusion::until`].
    #[serde(deserialize_with = "Option::deserialize")]
    pub until: Option<Date>,
    pub source: Source,
}

impl Holidays {
    /// Праздник ли день `day`.
    pub fn contains(&self, day: Date) -> bool {
        self.days
            .iter()
            .any(|holiday| holiday.month == day.month() && holiday.day == day.day())
    }
}

impl Rule for Holidays {
    fn period(&self) -> Period {
        Period {
            from: self.from,
            until: self.until,
        }
    }

    fn source(&self) -> &Source {
        &self.source
    }

    /// Список один на всех: две записи — всегда об одном.
    fn same_subject(&self, _: &Self) -> bool {
        true
    }

    /// Дни есть, каждый — в каждом году, без повторов.
    fn usable(&self) -> bool {
        let unique = self.days.iter().enumerate().all(|(i, a)| !self.days[..i].contains(a));
        !self.days.is_empty() && unique && self.days.iter().all(|day| day.usable())
    }
}

/// Решение правительства о нерабочих днях года (`tax::deadlines`): дни `off`
/// — нерабочие, дни `work` — рабочие, хотя по календарю выходные.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DaysOff {
    pub off: Vec<Date>,
    pub work: Vec<Date>,
    pub from: Date,
    /// Ключ обязателен, как у [`Exclusion::until`].
    #[serde(deserialize_with = "Option::deserialize")]
    pub until: Option<Date>,
    pub source: Source,
}

impl Rule for DaysOff {
    fn period(&self) -> Period {
        Period {
            from: self.from,
            until: self.until,
        }
    }

    fn source(&self) -> &Source {
        &self.source
    }

    /// Об одном — если называют один и тот же день: два решения о разных
    /// днях действуют вместе.
    fn same_subject(&self, other: &Self) -> bool {
        self.off
            .iter()
            .chain(&self.work)
            .any(|day| other.off.contains(day) || other.work.contains(day))
    }

    /// Дни есть, каждый — в дни действия записи и только в одном списке,
    /// без повторов.
    fn usable(&self) -> bool {
        let days: Vec<Date> = self.off.iter().chain(&self.work).copied().collect();
        let unique = days.iter().enumerate().all(|(i, a)| !days[..i].contains(a));
        let period = self.period();
        !days.is_empty() && unique && days.iter().all(|day| period.contains(*day))
    }
}

/// Правила страны — файл целиком.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    version: Version,
    turnover_exclusions: Vec<Exclusion>,
    funded_pension: Vec<FundedPension>,
    turnover_rates: Vec<Rate>,
    high_tech: Vec<HighTech>,
    turnover_threshold: Vec<Threshold>,
    turnover_expenses: Vec<Expenses>,
    profit_tax: Vec<Monthly>,
    social_payment: Vec<Monthly>,
    stamp_duty: Vec<StampDuty>,
    health_insurance: Vec<HealthInsurance>,
    turnover_due: Vec<TurnoverDue>,
    turnover_application: Vec<TurnoverApplication>,
    income_declaration: Vec<IncomeDeclaration>,
    deadline_shift: Vec<DeadlineShift>,
    holidays: Vec<Holidays>,
    days_off: Vec<DaysOff>,
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
        check("turnover_rates", &rules.turnover_rates)?;
        check("high_tech", &rules.high_tech)?;
        check("turnover_threshold", &rules.turnover_threshold)?;
        check("turnover_expenses", &rules.turnover_expenses)?;
        check("profit_tax", &rules.profit_tax)?;
        check("social_payment", &rules.social_payment)?;
        check("stamp_duty", &rules.stamp_duty)?;
        check("health_insurance", &rules.health_insurance)?;
        check("turnover_due", &rules.turnover_due)?;
        check("turnover_application", &rules.turnover_application)?;
        check("income_declaration", &rules.income_declaration)?;
        check("deadline_shift", &rules.deadline_shift)?;
        check("holidays", &rules.holidays)?;
        check("days_off", &rules.days_off)?;
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

    /// Ставки строк налога с оборота.
    pub fn turnover_rates(&self) -> &[Rate] {
        &self.turnover_rates
    }

    /// Список видов деятельности высоких технологий.
    pub fn high_tech(&self) -> &[HighTech] {
        &self.high_tech
    }

    /// Порог оборота за год.
    pub fn turnover_threshold(&self) -> &[Threshold] {
        &self.turnover_threshold
    }

    /// Какие расходы идут в вычет.
    pub fn turnover_expenses(&self) -> &[Expenses] {
        &self.turnover_expenses
    }

    /// Налог на прибыль ИП на налоге с оборота — за месяц (НК, ст. 125,
    /// ч. 3).
    pub fn profit_tax(&self) -> &[Monthly] {
        &self.profit_tax
    }

    /// Соцплатёж ИП на налоге с оборота — за месяц.
    pub fn social_payment(&self) -> &[Monthly] {
        &self.social_payment
    }

    /// Гербовый сбор ИП — шкала за отчётный год.
    pub fn stamp_duty(&self) -> &[StampDuty] {
        &self.stamp_duty
    }

    /// Взнос медстраховки ИП за отчётный год.
    pub fn health_insurance(&self) -> &[HealthInsurance] {
        &self.health_insurance
    }

    /// Срок расчёта и налога с оборота за квартал.
    pub fn turnover_due(&self) -> &[TurnoverDue] {
        &self.turnover_due
    }

    /// Срок заявления о налоге с оборота на год.
    pub fn turnover_application(&self) -> &[TurnoverApplication] {
        &self.turnover_application
    }

    /// Срок всеобщей декларации о доходах гражданина за отчётный год.
    pub fn income_declaration(&self) -> &[IncomeDeclaration] {
        &self.income_declaration
    }

    /// Перенос срока с нерабочего дня и дни недели, которые нерабочие.
    pub fn deadline_shift(&self) -> &[DeadlineShift] {
        &self.deadline_shift
    }

    /// Нерабочие дни закона о праздниках.
    pub fn holidays(&self) -> &[Holidays] {
        &self.holidays
    }

    /// Решения правительства о нерабочих и рабочих днях.
    pub fn days_off(&self) -> &[DaysOff] {
        &self.days_off
    }
}

/// Запись вида с одним предметом на всех (список высоких технологий, порог,
/// расходы для вычета), которая действует в день `on`: записи одного
/// предмета в один день не действуют вдвоём (это держит разбор), и первая
/// найденная — единственная. У ставок предмет — строка, и запись ищут по ней.
pub fn on<R: Rule>(records: &[R], on: Date) -> Option<&R> {
    records.iter().find(|record| record.period().contains(on))
}

/// Запись вида с одним предметом на всех, которая действует весь год
/// `year` — одна и та же 1 января и 31 декабря. `None` — правила нет или
/// оно сменилось посреди года: годовой платёж считается по одному правилу
/// на год (`tax::annual`), а год со сменой — к бухгалтеру.
pub fn for_year<R: Rule>(records: &[R], year: u16) -> Option<&R> {
    let first = on(records, Date::new(year, 1, 1)?)?;
    let last = on(records, Date::new(year, 12, 31)?)?;
    std::ptr::eq(first, last).then_some(first)
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
