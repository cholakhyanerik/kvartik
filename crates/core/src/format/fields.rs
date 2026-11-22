//! Слияние записи по полям (задача 116).
//!
//! До 0.0.171 из двух правок одной записи на двух устройствах между
//! передачами слияние ([`super::backup::merge`]) оставляло целиком позднюю:
//! оплата, внесённая на телефоне, и отметка ՊԵԿ, поставленная на компьютере,
//! — и одна из них пропадала. Теперь у записи есть моменты правки её полей
//! (`model::record`, «Моменты полей»), и слияние берёт каждое поле с той
//! стороны, где его правили позже.
//!
//! # Что такое поле
//!
//! Поле — поле записи верхнего уровня (`payments`, `settlement`, `signed`,
//! `annulled` документа), а `data` — одно поле: в ней то, что правит форма,
//! и правится оно вместе. Отдельно сливаются только поля `data` из таблицы
//! [`APART`] — те, что правят сами по себе: у документа — вид деятельности
//! и строка налога, которые выбирают у выпущенного, у клиента — всё, кроме
//! страны с ИНН и налоговым номером, у профиля — всё, кроме кодов с их
//! строками налога. Валюта и цены черновика, страна и ИНН клиента правятся
//! вместе, и валюта одного устройства с ценами другого была бы суммой,
//! которой не вводил никто (ревью 0.0.171). Глубже полей `data` слияние не
//! смотрит вовсе. Новое поле `data` без строки в таблице сливается вместе
//! с остальными — так осторожнее. Метка удаления — тоже поле, [`DELETED`]:
//! удалённое на одном устройстве и правленное на другом остаётся удалённым —
//! с правкой, и восстановить его можно (CLAUDE.md, «Записи не стираются»).
//!
//! # Когда поле правили
//!
//! Моменты ставит запись правки ([`stamp`], её зовёт `Writes::update`
//! хранилища окна): поля, которые в записи стали другими против записи до
//! правки, получают момент этой правки. Команда поле забыть могла бы,
//! а сравнение — нет. У записи без моментов — прежних версий и новой до
//! первой правки — все поля правлены в момент её изменения, и слияние такой
//! записи — прежнее: целиком поздняя.
//!
//! В общих полях записи моменты лежат нетронутым куском JSON
//! (`model::record::Edits`): их разбирает и проверяет только этот модуль
//! ([`Moments`]), а он живёт в частях wasm — правки и слияния, — не в главном
//! (CLAUDE.md, Правило 1). Негодные моменты чужой записи — порча при чтении
//! копии и кода (`backup::check`), а записи базы — как их нет.
//!
//! # Чего слияние по полям не делает
//!
//! - Две правки одного поля — та же беда, что прежде с записью: остаётся
//!   поздняя. Две оплаты одного счёта на двух устройствах — одно поле
//!   `payments`, и одна пропадёт: у оплат нет своих идентификаторов.
//! - Выпущенный документ против черновика — по-прежнему целиком выпущенный,
//!   а два разных выпуска одного черновика — целиком поздний
//!   (`backup::issued_wins`): строки черновика с реквизитами выпуска дали бы
//!   выпущенный документ, которого никто не выпускал.
//! - Файлы книги и картинки логотипов ([`WHOLE`]) — целиком: их поля — одна
//!   картинка, а разбирать сотни килобайт base64 внутри транзакции записи,
//!   где паузы быть не может (Правило 6), незачем.
//! - Слитая запись, которую её вид не читает — поля двух сторон, которые
//!   вместе не бывают, — не пишется: тогда слияние прежнее, целиком
//!   (`backup::merge`).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::de::{Deserializer, MapAccess, Visitor};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

use super::{FORMAT_VERSION, Stored, split_raw, wrap};
use crate::model::attachment::Attachment;
use crate::model::client::Client;
use crate::model::document::Document;
use crate::model::ledger::Operation;
use crate::model::logo::{IssuedLogo, Logo};
use crate::model::profile::Profile;
use crate::model::record::{Edits, RecordMeta, Timestamp};

/// Метка удаления среди полей. В типах записей поля с таким именем нет:
/// точки в именах полей Rust не бывает.
pub const DELETED: &str = "meta.deleted";

/// Виды записей, которые сливаются целиком: файлы книги и картинки
/// логотипов (шапка модуля).
pub const WHOLE: [&str; 3] = [Attachment::KIND, Logo::KIND, IssuedLogo::KIND];

/// Общие поля записи.
const META: &str = "meta";

/// Поля `data`, которые сливаются каждое само по себе, — по видам записей;
/// остальные поля `data` правятся вместе и сливаются одним полем `data`
/// (шапка модуля, «Что такое поле»). У операции книги отдельно — день оплаты
/// и файлы: сумма с курсом, вид расхода с видом деятельности и «прямой ли»
/// решаются вместе (`turnover::cost_needed`).
const APART: &[(&str, &[&str])] = &[
    (Document::KIND, &["activity", "income_kind"]),
    (Operation::KIND, &["paid", "attachments"]),
    (Client::KIND, &["name", "address", "currency", "lang", "payment_days"]),
    (
        Profile::KIND,
        &["name", "tin", "registration", "address", "phone", "email", "accounts", "regime", "micro", "person"],
    ),
];

/// Поле слияния, в котором лежит путь `path` записи вида `kind`: поле
/// верхнего уровня — само, поле `data` — своё, если сливается само по себе
/// ([`APART`]), иначе — `data`.
fn unit_of<'p>(kind: &str, path: &'p str) -> &'p str {
    let Some(inner) = path.strip_prefix("data.") else {
        return path;
    };
    let apart = APART
        .iter()
        .find(|(name, _)| *name == kind)
        .is_some_and(|(_, fields)| fields.contains(&inner));
    if apart { path } else { DATA }
}

/// Поля слияния записи вида `kind` из её путей (`Record::units`): значение
/// поля `data` — его пути и значения подряд, по порядку путей.
fn grouped(kind: &str, paths: &BTreeMap<String, &str>) -> BTreeMap<String, String> {
    let mut units: BTreeMap<String, String> = BTreeMap::new();
    for (path, value) in paths {
        let text = units.entry(unit_of(kind, path).to_owned()).or_default();
        text.push_str(path);
        text.push('=');
        text.push_str(value);
        text.push('\n');
    }
    units
}

/// Больше полей с моментами у записи не бывает: у самой большой, профиля,
/// их меньше сорока (тест `every_field_fits_under_the_limit`), а чужой файл
/// с тысячами имён иначе разбирался бы каждой правкой (Правило 13).
pub const MAX_FIELDS: usize = 64;

/// Имя поля с моментом — не длиннее этого: `data.` и имя поля типа.
const MAX_FIELD_NAME: usize = 64;

/// Имя поля с моментом — как его пишет этот модуль: латиница в нижнем
/// регистре, цифры, `_` и точка между `data` или `meta` и полем.
fn is_field_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_FIELD_NAME
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_' || byte == b'.')
}

/// Моменты правки полей записи, как они лежат в данных: `{"since":…,
/// "fields":{"payments":…}}`.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EditsFields {
    since: Timestamp,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    fields: BTreeMap<String, Timestamp>,
}

/// Моменты правки полей записи — разобранные и проверенные.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Moments {
    /// До какого момента правлены поля, которых нет в `fields`.
    since: Timestamp,
    /// Поля, правленные позже `since`, — с моментом последней правки.
    fields: BTreeMap<String, Timestamp>,
}

impl Moments {
    /// Моменты из общих полей записи; без моментов — все поля правлены
    /// в момент изменения. `None` — моменты негодные: позже момента
    /// изменения, раньше `since`, не тех имён или больше предела. Поле,
    /// правленное «в будущем», перебивало бы при слиянии все правки другого
    /// устройства (Правило 13).
    pub fn of(meta: &RecordMeta) -> Option<Self> {
        let Some(edits) = meta.edits() else {
            return Some(Self {
                since: meta.modified(),
                fields: BTreeMap::new(),
            });
        };
        // Только объект: serde прочёл бы структуру и из массива `[since]`.
        if !edits.get().starts_with('{') {
            return None;
        }
        let EditsFields { since, fields } = serde_json::from_str(edits.get()).ok()?;
        let moments = Self { since, fields };
        moments.fit(meta.modified()).then_some(moments)
    }

    /// Годятся ли моменты записи, изменённой в `modified`.
    fn fit(&self, modified: Timestamp) -> bool {
        self.since <= modified
            && self.fields.len() <= MAX_FIELDS
            && self
                .fields
                .iter()
                .all(|(name, at)| is_field_name(name) && self.since <= *at && *at <= modified)
    }

    /// До какого момента правлены поля, у которых своего момента нет.
    pub const fn since(&self) -> Timestamp {
        self.since
    }

    /// Когда в последний раз правили поле `name`.
    pub fn changed(&self, name: &str) -> Timestamp {
        self.fields.get(name).copied().unwrap_or(self.since)
    }

    /// Поля, правленные позже [`Self::since`], с моментами.
    pub const fn fields(&self) -> &BTreeMap<String, Timestamp> {
        &self.fields
    }

    /// Общие поля `meta` с этими моментами. Без полей и с `since` в момент
    /// изменения — без моментов вовсе: так и пишется, слово в слово как у
    /// записи прежней версии. `None` — моменты не годятся для `meta`.
    fn put(self, meta: &RecordMeta) -> Option<RecordMeta> {
        if !self.fit(meta.modified()) {
            return None;
        }
        if self.fields.is_empty() && self.since == meta.modified() {
            return Some(meta.with_edits(None));
        }
        let text = serde_json::to_string(&EditsFields {
            since: self.since,
            fields: self.fields,
        })
        .expect("моменты в JSON не падают");
        let raw = RawValue::from_string(text).expect("моменты — JSON");
        Some(meta.with_edits(Some(Edits::new(raw))))
    }
}

/// Годны ли моменты правки полей в общих полях `meta` (`backup::check`).
pub(super) fn valid(meta: &RecordMeta) -> bool {
    Moments::of(meta).is_some()
}

/// Поле, в которое слияние заглядывает на уровень глубже (шапка модуля).
const DATA: &str = "data";

/// Поля объекта JSON по порядку — нетронутыми кусками.
struct Object<'a>(Vec<(String, &'a RawValue)>);

impl<'de> Deserialize<'de> for Object<'de> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Fields;
        impl<'de> Visitor<'de> for Fields {
            type Value = Object<'de>;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("объект JSON")
            }

            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                let mut fields = Vec::new();
                while let Some((name, value)) = map.next_entry::<String, &'de RawValue>()? {
                    fields.push((name, value));
                }
                Ok(Object(fields))
            }
        }
        deserializer.deserialize_map(Fields)
    }
}

/// Поле записи верхнего уровня.
enum Slot<'a> {
    /// Общие поля.
    Meta(&'a RawValue),
    /// `data` — со своими полями по порядку.
    Data(Vec<(String, &'a RawValue)>),
    /// Любое другое — целиком.
    Value(&'a RawValue),
}

/// Запись, разложенная на поля: по порядку, как её записал serde.
struct Record<'a> {
    slots: Vec<(String, Slot<'a>)>,
}

impl<'a> Record<'a> {
    /// Поля записи из её JSON; `None` — не объект, общих полей нет или они
    /// дважды.
    fn parse(text: &'a str) -> Option<Self> {
        let Object(fields) = serde_json::from_str(text).ok()?;
        let mut slots = Vec::with_capacity(fields.len());
        let mut metas = 0;
        for (name, value) in fields {
            let slot = if name == META {
                metas += 1;
                Slot::Meta(value)
            } else if name == DATA && value.get().starts_with('{') {
                Slot::Data(serde_json::from_str::<Object<'a>>(value.get()).ok()?.0)
            } else {
                Slot::Value(value)
            };
            slots.push((name, slot));
        }
        (metas == 1).then_some(Self { slots })
    }

    /// Общие поля записи — разобранные.
    fn meta(&self) -> Option<RecordMeta> {
        self.slots.iter().find_map(|(_, slot)| match slot {
            Slot::Meta(raw) => serde_json::from_str(raw.get()).ok(),
            Slot::Data(_) | Slot::Value(_) => None,
        })
    }

    /// Поля для слияния: имя — значение текстом JSON.
    fn units(&self) -> BTreeMap<String, &'a str> {
        let mut units = BTreeMap::new();
        for (name, slot) in &self.slots {
            match slot {
                Slot::Meta(_) => {}
                Slot::Value(raw) => {
                    units.insert(name.clone(), raw.get());
                }
                Slot::Data(fields) => {
                    for (inner, raw) in fields {
                        units.insert(format!("{DATA}.{inner}"), raw.get());
                    }
                }
            }
        }
        units
    }

    /// `data` — объект ли.
    fn data_shape(&self) -> Option<bool> {
        self.slots.iter().find_map(|(name, slot)| (name == DATA).then_some(matches!(slot, Slot::Data(_))))
    }
}

/// Имя поля текстом JSON — в кавычках, экранированное.
fn quoted(name: &str) -> String {
    serde_json::to_string(name).expect("строка в JSON не падает")
}

/// Объект JSON из полей по порядку.
fn object<'s>(fields: impl IntoIterator<Item = (&'s str, String)>) -> String {
    let mut text = String::from("{");
    for (i, (name, value)) in fields.into_iter().enumerate() {
        if i > 0 {
            text.push(',');
        }
        text.push_str(&quoted(name));
        text.push(':');
        text.push_str(&value);
    }
    text.push('}');
    text
}

/// Записать правку записи вида `kind` с моментами полей: `next` — строка для
/// базы, которую записала команда, `previous` — запись до правки. Поля,
/// которые стали другими, получают момент правки — момент изменения `next`;
/// остальные моменты — из записи до правки, а её негодные моменты — как их
/// нет. `None` — записывать `next` как есть: запись до правки не читается,
/// это не та же запись, или правка не новее её (такого команды не делают —
/// `RecordMeta::touch`).
pub fn stamp(kind: &str, previous: &str, next: &str) -> Option<String> {
    let (_, before) = split_raw(previous).ok()?;
    let (version, after) = split_raw(next).ok()?;
    if version != FORMAT_VERSION {
        return None;
    }
    let before = Record::parse(before.get())?;
    let after = Record::parse(after.get())?;
    let (old, new) = (before.meta()?, after.meta()?);
    if old.id() != new.id() || new.modified() <= old.modified() {
        return None;
    }
    let (was, is) = (grouped(kind, &before.units()), grouped(kind, &after.units()));
    let Moments { since, mut fields } = Moments::of(&old).unwrap_or(Moments {
        since: old.modified(),
        fields: BTreeMap::new(),
    });
    // Без `collect` в множество: он сортирует, и сортировка — лишний код
    // части (как у `backup::merge`).
    for (name, value) in &was {
        if is.get(name) != Some(value) {
            fields.insert(name.clone(), new.modified());
        }
    }
    for name in is.keys() {
        if !was.contains_key(name) {
            fields.insert(name.clone(), new.modified());
        }
    }
    if old.deleted() != new.deleted() {
        fields.insert(DELETED.to_owned(), new.modified());
    }
    let meta = Moments { since, fields }.put(&new)?;
    Some(wrap(&rebuild(&after, &meta, |_, _| None)))
}

/// Запись из полей `record` с общими полями `meta`; `pick` подменяет значение
/// поля (по имени для слияния): `Some(Some(…))` — другим, `Some(None)` —
/// убрать поле, `None` — оставить.
fn rebuild<'a>(record: &Record<'a>, meta: &RecordMeta, pick: impl Fn(&str, &'a str) -> Option<Option<&'a str>>) -> String {
    let meta = serde_json::to_string(meta).expect("общие поля в JSON не падают");
    let mut top = Vec::with_capacity(record.slots.len());
    for (name, slot) in &record.slots {
        let value = match slot {
            Slot::Meta(_) => Some(meta.clone()),
            Slot::Value(raw) => pick(name, raw.get()).unwrap_or(Some(raw.get())).map(str::to_owned),
            Slot::Data(fields) => Some(object(fields.iter().filter_map(|(inner, raw)| {
                let unit = format!("{DATA}.{inner}");
                pick(&unit, raw.get())
                    .unwrap_or(Some(raw.get()))
                    .map(|value| (inner.as_str(), value.to_owned()))
            }))),
        };
        if let Some(value) = value {
            top.push((name.as_str(), value));
        }
    }
    object(top)
}

/// Запись базы `stored` и пришедшая `incoming` вида `kind` (тексты записей
/// без конверта, текущей версии, с общими полями `stored_meta`
/// и `incoming_meta`), слитые по полям: каждое поле — с той стороны, где его
/// правили позже, при равенстве — базы; моменты — поздний каждого поля. Текст
/// ещё не разобран своим видом: проверяет его `backup::merge`. `None` — по
/// полям не слить: у сторон разный вид объекта или негодные моменты.
pub(super) fn merge<'a>(
    kind: &str,
    stored: &'a str,
    stored_meta: &RecordMeta,
    incoming: &'a str,
    incoming_meta: &RecordMeta,
) -> Option<(RecordMeta, String)> {
    let ours = Record::parse(stored)?;
    let theirs = Record::parse(incoming)?;
    if ours.data_shape() != theirs.data_shape() {
        return None;
    }
    let (ours_units, theirs_units) = (ours.units(), theirs.units());
    let (mine, other) = (Moments::of(stored_meta)?, Moments::of(incoming_meta)?);
    let since = mine.since().max(other.since());
    let mut fields = BTreeMap::new();
    let mut take = BTreeSet::new();
    let mut names = BTreeSet::new();
    for path in ours_units.keys().chain(theirs_units.keys()) {
        names.insert(unit_of(kind, path));
    }
    names.insert(DELETED);
    for name in names {
        let (ours_at, theirs_at) = (mine.changed(name), other.changed(name));
        if theirs_at > ours_at {
            take.insert(name.to_owned());
        }
        let at = ours_at.max(theirs_at);
        if at > since {
            fields.insert(name.to_owned(), at);
        }
    }
    let deleted = if take.contains(DELETED) { incoming_meta.deleted() } else { stored_meta.deleted() };
    let modified: Timestamp = stored_meta.modified().max(incoming_meta.modified());
    let meta = Moments { since, fields }.put(&stored_meta.merged(modified, deleted))?;
    // Значение пути — у той стороны, чьё его поле слияния; у неё пути нет —
    // нет его и в слитой записи (`skip_serializing_if` у типа).
    let value = |path: &str| -> Option<&str> {
        if take.contains(unit_of(kind, path)) {
            theirs_units.get(path).copied()
        } else {
            ours_units.get(path).copied()
        }
    };
    let mut text = rebuild(&ours, &meta, |name, _| Some(value(name)));
    // Поля, которых у базы нет вовсе, — в конец: порядок наведёт разбор
    // своим видом (`backup::merge`).
    let missing: Vec<(String, String)> = theirs
        .slots
        .iter()
        .filter(|(name, _)| !ours.slots.iter().any(|(own, _)| own == name))
        .filter_map(|(name, slot)| match slot {
            Slot::Value(_) => value(name).map(|raw| (name.clone(), raw.to_owned())),
            Slot::Meta(_) | Slot::Data(_) => None,
        })
        .collect();
    let data_missing: Vec<(String, String)> = theirs_units
        .keys()
        .filter_map(|name| name.strip_prefix("data.").map(|inner| (name, inner)))
        .filter(|(name, _)| !ours_units.contains_key(*name))
        .filter_map(|(name, inner)| value(name).map(|raw| (inner.to_owned(), raw.to_owned())))
        .collect();
    if !missing.is_empty() || !data_missing.is_empty() {
        text = extend(&text, &missing, &data_missing)?;
    }
    Some((meta, text))
}

/// Дописать к записи `text` поля верхнего уровня `top` и поля `data` —
/// `data`.
fn extend(text: &str, top: &[(String, String)], data: &[(String, String)]) -> Option<String> {
    let record = Record::parse(text)?;
    let mut slots: Vec<(String, String)> = Vec::new();
    for (name, slot) in &record.slots {
        let value = match slot {
            Slot::Meta(raw) | Slot::Value(raw) => raw.get().to_owned(),
            Slot::Data(fields) => object(
                fields
                    .iter()
                    .map(|(inner, raw)| (inner.as_str(), raw.get().to_owned()))
                    .chain(data.iter().map(|(inner, raw)| (inner.as_str(), raw.clone()))),
            ),
        };
        slots.push((name.clone(), value));
    }
    slots.extend(top.iter().cloned());
    Some(object(slots.iter().map(|(name, value)| (name.as_str(), value.clone()))))
}

#[cfg(test)]
mod tests;
