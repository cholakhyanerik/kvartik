//! Профиль ИП (задача 9): реквизиты для счёта и акта, виды деятельности
//! и режим для налога, данные самого человека для годовых платежей.
//!
//! ИП в программе один (несколько — «Pro», задача 38), и профиль — одна
//! запись. Записи две, если у профиля есть логотип: картинка лежит отдельно
//! (`model/logo.rs`), чтобы не тащить её с каждым чтением профиля.
//!
//! Меняется профиль одной командой — [`Profile::save`] (CLAUDE.md, «Данные
//! меняются одним путём»): окно собирает из полей формы [`ProfileData`],
//! а момент изменения ставит команда.
//!
//! В данных: `{"meta":{…},"data":{…}}`, поля `data` — как у
//! [`ProfileData`]; образец — в тесте `data_shape_is_pinned`. Данные не
//! проверяются налоговыми правилами: что из них следует, решают правила на
//! нужную дату (`tax`), а правило, поменявшееся завтра, не должно сделать
//! вчерашний профиль «испорченным».

use std::collections::BTreeMap;

use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize};

use super::activity::{ActivityCode, IncomeKind};
use super::bank::BankAccount;
use super::date::Date;
use super::record::{RecordId, RecordMeta, Timestamp};
use super::text::{Line, Lines};
use super::tin::Tin;
use crate::format::Stored;
use crate::i18n::Lang;

/// Текст на трёх языках документов: имя ИП и адрес. В счёте по-английски
/// имя и адрес пишутся латиницей, в акте местной фирме — по-армянски,
/// и перевести их за человека программа не может.
///
/// В данных — `{"hy":…,"ru":…,"en":…}`: ключи — коды языков (`Lang::code`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Localized<T> {
    pub hy: T,
    pub ru: T,
    pub en: T,
}

impl<T> Localized<T> {
    /// Текст на языке `lang`.
    pub const fn get(&self, lang: Lang) -> &T {
        match lang {
            Lang::Am => &self.hy,
            Lang::Ru => &self.ru,
            Lang::En => &self.en,
        }
    }
}

/// Адрес почты: одна `@`, до неё и после — не пусто, после — домен с точкой
/// не по краям, без пробелов. Строже не проверяется: настоящий адрес бывает
/// причудливым, а опечатку в нём видно и так.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize)]
#[serde(transparent)]
pub struct Email(String);

/// Почему адрес почты не принят.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum EmailError {
    /// Не похож на адрес.
    Malformed,
}

impl Email {
    /// Самый длинный адрес — 254 знака (RFC 5321, §4.5.3.1.3, путь без
    /// угловых скобок).
    pub const MAX_CHARS: usize = 254;

    /// Адрес из поля ввода: края обрезаются.
    pub fn parse(input: &str) -> Result<Self, EmailError> {
        let text = input.trim();
        let fits = text.chars().count() <= Self::MAX_CHARS
            && !text.chars().any(|c| c.is_whitespace() || c.is_control())
            && text.split_once('@').is_some_and(|(local, domain)| {
                !local.is_empty()
                    && !domain.contains('@')
                    && domain.contains('.')
                    && !domain.starts_with('.')
                    && !domain.ends_with('.')
            });
        if fits {
            Ok(Self(text.to_owned()))
        } else {
            Err(EmailError::Malformed)
        }
    }

    /// Адрес как есть.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for Email {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        match Email::parse(&text) {
            Ok(email) if email.0 == text => Ok(email),
            _ => Err(de::Error::invalid_value(
                de::Unexpected::Str(&text),
                &"адрес почты",
            )),
        }
    }
}

/// Режим налогообложения ИП.
///
/// Расширяется (микропредприятие, общий режим — когда программа научится
/// их считать), а смысл существующих вариантов не меняется (Правило 2).
/// `match` — без `_`. В данных — `"turnover"`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Regime {
    /// Налог с оборота.
    Turnover,
}

/// Гражданство и право проживания — от них зависят соцплатёж, гербовый
/// сбор и медстраховка (задача 20).
///
/// Вид статуса проживания (временный, постоянный, специальный) не
/// спрашивается: все три закона говорят о праве проживания
/// («կացության կարգավիճակ») любого вида — закон о накопительных пенсиях
/// ՀՕ-244-Ն, ст. 5, ч. 3 (соцплатёж); закон о возмещении вреда
/// военнослужащим ՀՕ-245-Ն, ст. 11, ч. 2, п. 3 (гербовый сбор ИП); закон
/// о всеобщем медицинском страховании ՀՕ-459-Ն, ст. 11, ч. 1, п. 2 (по
/// закону; сверено на arlis.am 2026-10-12, разведка задачи 9). Лицо без
/// гражданства законы ставят рядом с иностранцем.
///
/// В данных — `"citizen"`, `"foreign_with_status"`, `"foreign_without_status"`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Residency {
    /// Гражданин Армении, в том числе с другим гражданством.
    Citizen,
    /// Иностранец или лицо без гражданства со статусом проживания в Армении
    /// (կացության կարգավիճակ) любого вида.
    ForeignWithStatus,
    /// Иностранец или лицо без гражданства без статуса проживания.
    ForeignWithoutStatus,
}

/// Период, когда деятельность остановлена заявлением в налоговый орган.
///
/// ИП подаёт заявление об остановке деятельности с какого-то дня — на срок
/// или бессрочно, — а потом о возобновлении (Налоговый кодекс, ст. 53,
/// ч. 8). Налог на прибыль 5 000 ֏ в месяц не платится за **целые месяцы**
/// внутри периода от дня остановки до дня возобновления, и ни тот, ни
/// другой день не раньше дня подачи своего заявления (ст. 125, ч. 3, п. 1;
/// по закону, сверено на arlis.am 2026-10-12). Поэтому период — даты, а не
/// месяцы: какие месяцы целые, считает задача 20, а человек вписывает
/// даты из своих заявлений.
///
/// В данных — `{"from":"2026-03-01","until":"2026-06-01"}`; `until` —
/// `null`, пока деятельность не возобновлена.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize)]
pub struct Stop {
    from: Date,
    until: Option<Date>,
}

/// Поля периода, как они лежат в данных, — до проверки.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StopFields {
    from: Date,
    until: Option<Date>,
}

/// Почему период не принят.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum StopError {
    /// День возобновления не позже дня остановки.
    Backwards,
}

impl Stop {
    /// Период с дня остановки `from` до дня возобновления `until` — первого
    /// дня работы после остановки; `None` — не возобновлена.
    pub fn new(from: Date, until: Option<Date>) -> Result<Self, StopError> {
        if until.is_some_and(|until| until <= from) {
            return Err(StopError::Backwards);
        }
        Ok(Self { from, until })
    }

    /// День остановки.
    pub const fn from(self) -> Date {
        self.from
    }

    /// День возобновления; `None` — деятельность ещё остановлена.
    pub const fn until(self) -> Option<Date> {
        self.until
    }
}

impl<'de> Deserialize<'de> for Stop {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let StopFields { from, until } = StopFields::deserialize(deserializer)?;
        Self::new(from, until)
            .map_err(|_| de::Error::custom("день возобновления не позже дня остановки"))
    }
}

/// Данные самого человека для годовых платежей (задача 20). «Нет
/// значения» — `None`, а не ноль и не ответ по умолчанию (Правило 6).
#[derive(Clone, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Person {
    /// Год рождения.
    pub birth_year: Option<u16>,
    /// Вступил ли в накопительную пенсию по своему заявлению — спрашивается
    /// у родившихся до 1974 года (`tax::annual::pension_is_a_choice`).
    /// Ответ хранится как дан: что из него следует, решает правило на
    /// нужную дату, а не профиль.
    pub pension_joined: Option<bool>,
    /// Гражданство и право проживания.
    pub residency: Option<Residency>,
    /// Периоды остановки деятельности.
    pub stops: Vec<Stop>,
}

/// Профиль, как его заполнил человек, — то, что меняет форма.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Default, Serialize)]
pub struct ProfileData {
    /// Имя ИП на трёх языках: «ԱՁ Արամ Պետրոսյան», «ИП Арам Петросян»,
    /// «IE Aram Petrosyan».
    pub name: Localized<Line>,
    /// ՀՎՀՀ; `None` — не указан.
    pub tin: Option<Tin>,
    /// Номер государственной регистрации ИП — как в выписке из реестра.
    pub registration: Line,
    /// Адрес на трёх языках.
    pub address: Localized<Lines>,
    pub phone: Line,
    /// Адрес почты; `None` — не указан.
    pub email: Option<Email>,
    /// Банковские счета — не больше [`ProfileData::MAX_ACCOUNTS`].
    pub accounts: Vec<BankAccount>,
    /// Коды видов деятельности — не больше [`ProfileData::MAX_ACTIVITIES`].
    pub activities: Vec<ActivityCode>,
    /// Строка налога, которую человек выбрал коду сам (решение человека
    /// 2026-10-19, задача 19): торговля, производство, аренда… Кода здесь
    /// нет — строку решают правила на нужный день (`tax::turnover`): по
    /// списку высоких технологий — 1 %, иначе — прочая деятельность. Ключи —
    /// только коды из [`Self::activities`]. С версии формата 8.
    pub kinds: BTreeMap<ActivityCode, IncomeKind>,
    /// Режим налогообложения; `None` — не выбран.
    pub regime: Option<Regime>,
    pub person: Person,
}

/// Поля профиля, как они лежат в данных, — до проверки.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProfileDataFields {
    name: Localized<Line>,
    tin: Option<Tin>,
    registration: Line,
    address: Localized<Lines>,
    phone: Line,
    email: Option<Email>,
    accounts: Vec<BankAccount>,
    activities: Vec<ActivityCode>,
    /// С версии формата 8 (задача 19): у профиля прежних версий строки
    /// выбирают правила.
    #[serde(default)]
    kinds: BTreeMap<ActivityCode, IncomeKind>,
    regime: Option<Regime>,
    person: Person,
}

/// Почему данные профиля не приняты.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ProfileError {
    /// Список длиннее предела. Форма до него не даёт дойти — значит, это
    /// порча.
    TooMany,
    /// Строка налога выбрана коду, которого в профиле нет. Форма такого не
    /// пишет — это порча.
    StrayKind,
}

impl ProfileData {
    /// Счетов — с запасом: у ИП их обычно два-три, по одному на валюту.
    pub const MAX_ACCOUNTS: usize = 20;
    /// Кодов деятельности — с запасом: в выписке их обычно один-пять.
    pub const MAX_ACTIVITIES: usize = 30;
    /// Периодов остановки — с запасом на годы работы.
    pub const MAX_STOPS: usize = 100;

    /// Не длиннее ли списки пределов. Списки ограничены, чтобы испорченная
    /// копия не положила в окно миллион строк (Правило 13). И строки налога
    /// выбраны только кодам профиля.
    pub fn check(&self) -> Result<(), ProfileError> {
        let fits = self.accounts.len() <= Self::MAX_ACCOUNTS
            && self.activities.len() <= Self::MAX_ACTIVITIES
            && self.person.stops.len() <= Self::MAX_STOPS;
        if !fits {
            return Err(ProfileError::TooMany);
        }
        if self.kinds.keys().any(|code| !self.activities.contains(code)) {
            return Err(ProfileError::StrayKind);
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for ProfileData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let ProfileDataFields {
            name,
            tin,
            registration,
            address,
            phone,
            email,
            accounts,
            activities,
            kinds,
            regime,
            person,
        } = ProfileDataFields::deserialize(deserializer)?;
        let data = Self {
            name,
            tin,
            registration,
            address,
            phone,
            email,
            accounts,
            activities,
            kinds,
            regime,
            person,
        };
        data.check().map_err(|error| match error {
            ProfileError::TooMany => de::Error::custom("список в профиле длиннее предела"),
            ProfileError::StrayKind => de::Error::custom("строка налога у кода, которого в профиле нет"),
        })?;
        Ok(data)
    }
}

/// Профиль ИП — запись.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    meta: RecordMeta,
    data: ProfileData,
}

impl Stored for Profile {
    const KIND: &'static str = "profile";

    fn meta(&self) -> &RecordMeta {
        &self.meta
    }
}

impl Profile {
    /// Команда «сохранить профиль»: новый профиль или правка прежнего,
    /// прочитанного в той же транзакции (`existing`). `Ok(None)` — данные не
    /// изменились, и записывать нечего: запись не становится «новее» без
    /// настоящей правки.
    pub fn save(
        existing: Option<Profile>,
        data: ProfileData,
        now: Timestamp,
    ) -> Result<Option<Profile>, ProfileError> {
        data.check()?;
        Ok(match existing {
            Some(profile) if profile.data == data => None,
            Some(mut profile) => {
                profile.meta.touch(now);
                profile.data = data;
                Some(profile)
            }
            None => Some(Self {
                meta: RecordMeta::new(now),
                data,
            }),
        })
    }

    /// Постоянный идентификатор профиля — под ним же лежит его логотип.
    pub const fn id(&self) -> RecordId {
        self.meta.id()
    }

    /// Данные профиля.
    pub const fn data(&self) -> &ProfileData {
        &self.data
    }

    /// Профиль, с которым работает окно, из всех записей вида: живой,
    /// изменённый последним. Живых бывает больше одного, когда профиль
    /// заполнили на двух устройствах и слили копии (задача 41); тогда
    /// остальные лежат нетронутыми.
    pub fn current(profiles: Vec<Profile>) -> Option<Profile> {
        profiles
            .into_iter()
            .filter(|profile| !profile.meta.is_deleted())
            .max_by_key(|profile| profile.meta.modified())
    }
}

#[cfg(test)]
mod tests;
