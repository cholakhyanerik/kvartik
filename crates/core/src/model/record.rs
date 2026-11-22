//! Общие поля каждой записи: постоянный идентификатор, момент изменения
//! и метка удаления (CLAUDE.md, «Записи не стираются, а помечаются
//! удалёнными»).
//!
//! Сервера синхронизации нет (задача 110), а второе устройство работает
//! слиянием копий (задача 41, `format::backup::merge`), и без этих полей
//! невозможно ни то, ни другое: два устройства не
//! договорятся, что новее, а удалённое на одном воскреснет копией с другого.
//! Добавить поля потом — значит мигрировать данные всех пользователей.
//!
//! Меняет их только путь команд ядра (CLAUDE.md, «Данные меняются одним
//! путём»): создание — `RecordMeta::new`, правка — `touch`, удаление
//! и восстановление — `delete` и `restore`. Правка, удаление и запись под
//! заданным идентификатором открыты только ядру (`pub(crate)`): окно меняет
//! записи командами — `Profile::save`, `Logo::set` и т. д., — и поставить
//! метку в обход команды не может.
//!
//! В данных поля записываются так: `{"id":"…","modified":1760000000000,
//! "deleted":null}`. Читаются — с проверкой каждого поля (Правило 13):
//! момент — только в пределах дат JS, удаление — не позже последней правки.
//!
//! # Моменты полей (задача 116)
//!
//! Из двух правок одной записи на двух устройствах слияние брало целиком
//! позднюю, и одна правка пропадала: оплата на телефоне и отметка ՊԵԿ на
//! компьютере. Поэтому у записи с версии формата 26 есть и моменты правки
//! её полей — `edits` ([`Edits`]): `since` — до какого момента правлены все
//! поля, о которых ничего не записано, и `fields` — поля, правленные позже,
//! с моментом последней правки каждого: `{"id":"…","modified":…,
//! "deleted":null,"edits":{"since":1760000000000,"fields":{"payments":…}}}`.
//! Ставит их не команда, а запись правки сравнением с записью до неё
//! (`format::fields::stamp`): команда поле забыть могла бы, а сравнение —
//! нет. Без них — у записей прежних версий и у новой записи до первой
//! правки — все поля правлены в `modified`, и слияние такой записи —
//! прежнее, целиком.
//!
//! Здесь моменты — нетронутым куском JSON: разбирают и проверяют их только
//! правка и слияние (`format::fields`), а общие поля читает каждая запись
//! главного wasm, и словарь моментов в каждой из них стоил первому заходу
//! 3 КБ после brotli (замер задачи 116; CLAUDE.md, Правило 1).

use std::fmt;
use std::hash::{Hash, Hasher};

use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use uuid::Uuid;

/// Постоянный идентификатор записи — случайный UUID версии 4.
///
/// Случайный, а не порядковый номер: две копии и два устройства (задача 41)
/// создают записи независимо, и номера по порядку у них совпали бы.
///
/// В данных — строка `xxxxxxxx-xxxx-…` в нижнем регистре. Читается любой
/// UUID, не только версии 4: другой версии он стать не может, а запись
/// с ним — всё равно запись, и терять её нельзя.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RecordId(Uuid);

impl RecordId {
    /// Новый идентификатор.
    ///
    /// Случайность ядро берёт само, а не параметром, как «сегодня»: на wasm32
    /// она работает (источник — WebCrypto, его включает фича `js` ядра,
    /// Правило 3), тестам конкретное значение не нужно, а свежий
    /// идентификатор у каждой новой записи так гарантирует ядро — окно не
    /// выдаст один и тот же дважды.
    ///
    /// Паникует, если браузер не дал случайных чисел. Это не сбой посреди
    /// работы, а неподдерживаемая платформа: `crypto.getRandomValues` есть во
    /// всех браузерах с июля 2015 года, в воркерах и на страницах без HTTPS,
    /// а отказывает только на просьбе больше 65 536 байт — у нас их 16
    /// (по браузеру: MDN, «Crypto: getRandomValues()», сверено 2026-10-10).
    pub fn random() -> Self {
        Self(Uuid::new_v4())
    }

    /// Идентификатор из отпечатка содержимого — UUID версии 8 (RFC 9562,
    /// п. 5.8) из 16 байтов `print`: шесть битов версии и варианта затирает
    /// `uuid`, остальные — как есть. Только для записи, чей идентификатор
    /// и есть её содержимое, — логотипа выпущенного документа
    /// (`logo::IssuedLogo`, задача 63): одна картинка у всех устройств и
    /// копий ложится под одним идентификатором.
    pub(crate) const fn from_print(print: [u8; 16]) -> Self {
        Self(uuid::Builder::from_custom_bytes(print).into_uuid())
    }

    /// Идентификатор из записи `Display` — например, из адреса экрана
    /// документа (`#/documents/…`). Только запись с дефисами в нижнем
    /// регистре, как её пишет `Display`: адрес, набранный иначе, — чужой.
    pub fn parse(text: &str) -> Option<Self> {
        let id = Uuid::try_parse(text).ok()?;
        (id.hyphenated().to_string() == text).then_some(Self(id))
    }
}

/// Строка UUID в нижнем регистре, как в данных: `0f8c…-…`. Это не текст для
/// человека, а ключ — например, записи в хранилище окна.
impl fmt::Display for RecordId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0.hyphenated(), f)
    }
}

/// Момент времени: миллисекунды от 1970-01-01 00:00 UTC.
///
/// Только для служебных отметок — когда запись изменили и когда удалили.
/// Дата счёта и операции — календарная `Date`, а не момент (Правило 2):
/// момент по UTC уезжает на сутки. Здесь нужен именно момент: за день правок
/// бывает много, и при слиянии копий (задача 41) важно, какая из них позже.
/// Берёт его оболочка у часов устройства и передаёт параметром — ядро часов
/// не спрашивает (Архитектура, «Время — параметр»).
///
/// Только в пределах дат JS — `±MAX_MILLIS`. Других моментов часы браузера
/// не дают, а за краем `Date` в JS их и не прочесть; в данных такой момент —
/// признак порчи, и разбор его отвергает (`Deserialize` ниже).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize)]
#[serde(transparent)]
pub struct Timestamp(i64);

impl Timestamp {
    /// Край дат JS: ровно 100 000 000 суток в обе стороны от 1970-01-01, это
    /// ±8,64·10¹⁵ мс — до 13 сентября 275 760 года (по спецификации
    /// ECMAScript, «Time Values and Time Range»).
    pub const MAX_MILLIS: i64 = 8_640_000_000_000_000;

    /// Момент по числу миллисекунд от 1970-01-01 00:00 UTC — в тех же
    /// единицах, что `js_sys::Date::now()`. За краем дат JS — край
    /// (`MAX_MILLIS`): часы браузера туда не заходят, а значение, которое
    /// нельзя ни записать, ни прочесть обратно, типу не нужно.
    pub const fn from_unix_millis(millis: i64) -> Self {
        Self(if millis > Self::MAX_MILLIS {
            Self::MAX_MILLIS
        } else if millis < -Self::MAX_MILLIS {
            -Self::MAX_MILLIS
        } else {
            millis
        })
    }

    /// Миллисекунды от 1970-01-01 00:00 UTC.
    pub const fn unix_millis(self) -> i64 {
        self.0
    }
}

/// Момент читается только в пределах дат JS. Не прижимается к краю, как
/// у `from_unix_millis`, а отвергается: часы такого не дают, и в данных это
/// порча, а не время.
impl<'de> Deserialize<'de> for Timestamp {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let millis = i64::deserialize(deserializer)?;
        if millis.unsigned_abs() > Self::MAX_MILLIS.unsigned_abs() {
            return Err(de::Error::invalid_value(
                de::Unexpected::Signed(millis),
                &"момент в пределах дат JS: ±8 640 000 000 000 000 мс",
            ));
        }
        Ok(Self(millis))
    }
}

/// Общие поля записи.
///
/// Без `Copy`, хотя все поля копируются: у типа есть изменяющие методы,
/// и с `Copy` `запись.meta().delete(now)` молча удалил бы копию, а запись
/// осталась бы живой. Без него такой код не соберётся.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize)]
pub struct RecordMeta {
    id: RecordId,
    modified: Timestamp,
    deleted: Option<Timestamp>,
    /// Моменты правки полей; `None` — все поля правлены в `modified`
    /// (шапка модуля, «Моменты полей»).
    #[serde(skip_serializing_if = "Option::is_none")]
    edits: Option<Edits>,
}

/// Моменты правки полей записи — нетронутым куском JSON (шапка модуля,
/// «Моменты полей»): разбирает и проверяет их `format::fields`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Edits(Box<RawValue>);

impl Edits {
    /// Моменты из куска JSON — его пишет `format::fields`.
    pub(crate) const fn new(raw: Box<RawValue>) -> Self {
        Self(raw)
    }

    /// Текст JSON моментов.
    pub fn get(&self) -> &str {
        self.0.get()
    }
}

impl PartialEq for Edits {
    fn eq(&self, other: &Self) -> bool {
        self.get() == other.get()
    }
}

impl Eq for Edits {}

impl Hash for Edits {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.get().hash(state);
    }
}

/// Поля `RecordMeta`, как они лежат в данных, — до проверки.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RecordMetaFields {
    id: RecordId,
    modified: Timestamp,
    deleted: Option<Timestamp>,
    /// С версии формата 26 (задача 116); у записей прежних — нет.
    #[serde(default)]
    edits: Option<Edits>,
}

/// Читается с проверкой, которую иначе держат методы: удаление — тоже
/// правка, поэтому метка удаления не бывает позже момента изменения
/// (`delete`). Запись, удалённая «в будущем», при слиянии копий (задача 41)
/// спорила бы с правками, сделанными после её удаления. Моменты полей
/// проверяет тот, кто их разбирает (`format::fields`): чужая запись с
/// негодными моментами — порча при чтении копии и кода, а запись базы
/// с ними слияние считает записью без моментов.
impl<'de> Deserialize<'de> for RecordMeta {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let RecordMetaFields {
            id,
            modified,
            deleted,
            edits,
        } = RecordMetaFields::deserialize(deserializer)?;
        if deleted.is_some_and(|deleted| deleted > modified) {
            return Err(de::Error::custom(
                "метка удаления позже момента изменения, а удаление — тоже правка",
            ));
        }
        Ok(Self {
            id,
            modified,
            deleted,
            edits,
        })
    }
}

impl RecordMeta {
    /// Поля новой записи: свежий идентификатор, изменена в `now`, не удалена.
    pub fn new(now: Timestamp) -> Self {
        Self::with_id(RecordId::random(), now)
    }

    /// Поля новой записи под заданным идентификатором. Только для записи,
    /// которая продолжает другую под её же идентификатором, — логотип
    /// профиля (`Logo`): вид — часть ключа, и записи не столкнутся, — и для
    /// записи под отпечатком своего содержимого — логотипа выпуска
    /// (`IssuedLogo`, [`RecordId::from_print`]).
    pub(crate) const fn with_id(id: RecordId, now: Timestamp) -> Self {
        Self {
            id,
            modified: now,
            deleted: None,
            edits: None,
        }
    }

    /// Моменты правки полей; `None` — все поля правлены в момент изменения.
    pub const fn edits(&self) -> Option<&Edits> {
        self.edits.as_ref()
    }

    /// Те же поля с моментами правки полей `edits` — их ставит только
    /// `format::fields`: при записи правки и при слиянии.
    pub(crate) fn with_edits(&self, edits: Option<Edits>) -> Self {
        Self {
            edits,
            ..self.clone()
        }
    }

    /// Поля той же записи с моментом изменения `modified` и меткой удаления
    /// `deleted`, без моментов полей — для записи, слитой по полям
    /// (`format::fields::merge`).
    pub(crate) const fn merged(&self, modified: Timestamp, deleted: Option<Timestamp>) -> Self {
        Self {
            id: self.id,
            modified,
            deleted,
            edits: None,
        }
    }

    /// Постоянный идентификатор.
    pub const fn id(&self) -> RecordId {
        self.id
    }

    /// Когда запись меняли в последний раз.
    pub const fn modified(&self) -> Timestamp {
        self.modified
    }

    /// Когда запись удалили; `None` — запись жива.
    pub const fn deleted(&self) -> Option<Timestamp> {
        self.deleted
    }

    /// Удалена ли запись.
    pub const fn is_deleted(&self) -> bool {
        self.deleted.is_some()
    }

    /// Запись изменилась в `now`.
    ///
    /// Момент изменения только растёт: если часы устройства перевели назад
    /// (Правило 6, «Часы устройства могут врать») или две правки пришлись на
    /// одну миллисекунду, новая всё равно позже прежней — иначе при слиянии
    /// копий (задача 41) победила бы старая. Расти некуда только у края дат
    /// JS (`Timestamp::MAX_MILLIS`, 275 760 год): там момент останавливается,
    /// и две правки получают один момент. Дальше края тип не заходит, иначе
    /// запись не прочиталась бы обратно.
    pub(crate) fn touch(&mut self, now: Timestamp) {
        let after_previous = self.modified.0.saturating_add(1);
        self.modified = Timestamp::from_unix_millis(now.0.max(after_previous));
    }

    /// Пометить запись удалённой. Запись не стирается: её можно восстановить,
    /// а слияние копий узнаёт об удалении по метке.
    ///
    /// Удалённую запись не трогает: время удаления остаётся первым, а запись
    /// не становится «новее» без настоящей правки.
    pub(crate) fn delete(&mut self, now: Timestamp) {
        if self.deleted.is_none() {
            self.touch(now);
            self.deleted = Some(self.modified);
        }
    }

    /// Снять метку удаления. Живую запись не трогает.
    pub(crate) fn restore(&mut self, now: Timestamp) {
        if self.deleted.is_some() {
            self.touch(now);
            self.deleted = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn at(millis: i64) -> Timestamp {
        Timestamp::from_unix_millis(millis)
    }

    /// Идентификатор читается из своей записи обратно, а другая запись того
    /// же UUID — заглавными, без дефисов, в фигурных скобках — и не UUID
    /// вовсе — нет: адрес экрана документа должен быть один.
    #[test]
    fn id_reads_back_only_from_its_own_writing() {
        let id = RecordId::random();
        assert_eq!(RecordId::parse(&id.to_string()), Some(id));
        let written = id.to_string();
        let mut wrong = Vec::new();
        for other in [
            written.to_uppercase(),
            written.replace('-', ""),
            format!("{{{written}}}"),
            format!("urn:uuid:{written}"),
            format!("{written} "),
            String::new(),
            "documents".to_owned(),
        ] {
            if RecordId::parse(&other).is_some() {
                wrong.push(other);
            }
        }
        assert!(wrong.is_empty(), "прочитано чужое: {wrong:?}");
    }

    /// Новая запись жива, и момент изменения — момент создания.
    #[test]
    fn new_record_is_alive() {
        let meta = RecordMeta::new(at(1_000));
        assert_eq!(meta.modified(), at(1_000));
        assert_eq!(meta.deleted(), None);
        assert!(!meta.is_deleted());
    }

    /// Идентификатор — случайный UUID версии 4 по RFC 9562, и у каждой записи
    /// свой.
    #[test]
    fn ids_are_random_v4() {
        let ids: Vec<RecordId> = (0..1_000).map(|_| RecordId::random()).collect();
        let distinct: HashSet<RecordId> = ids.iter().copied().collect();
        assert_eq!(distinct.len(), ids.len(), "идентификаторы повторились");
        let mut wrong = Vec::new();
        for id in &ids {
            if id.0.get_version_num() != 4 || id.0.get_variant() != uuid::Variant::RFC4122 {
                wrong.push(format!("{id:?}"));
            }
        }
        assert!(wrong.is_empty(), "не UUID версии 4: {wrong:#?}");
    }

    /// Удаление ставит метку, восстановление снимает; оба — правки,
    /// а идентификатор остаётся прежним.
    #[test]
    fn delete_marks_and_restore_unmarks() {
        let mut meta = RecordMeta::new(at(1_000));
        let id = meta.id();

        meta.delete(at(2_000));
        assert_eq!(meta.deleted(), Some(at(2_000)));
        assert_eq!(meta.modified(), at(2_000));
        assert!(meta.is_deleted());

        meta.restore(at(3_000));
        assert_eq!(meta.deleted(), None);
        assert_eq!(meta.modified(), at(3_000));
        assert_eq!(meta.id(), id);
    }

    /// Повторное удаление и восстановление живой записи ничего не меняют:
    /// время удаления — первое, и запись не «новеет» без правки.
    #[test]
    fn delete_and_restore_are_idempotent() {
        let mut meta = RecordMeta::new(at(1_000));
        meta.restore(at(1_500));
        assert_eq!(meta.modified(), at(1_000), "живую запись восстановили");

        meta.delete(at(2_000));
        let deleted = meta.clone();
        meta.delete(at(9_000));
        assert_eq!(meta, deleted, "удалённую запись удалили второй раз");
    }

    /// Часы перевели назад или правки в одну миллисекунду — момент изменения
    /// всё равно растёт.
    #[test]
    fn modified_only_grows() {
        let mut meta = RecordMeta::new(at(5_000));
        meta.touch(at(1_000));
        assert_eq!(meta.modified(), at(5_001), "часы назад");
        meta.touch(at(5_001));
        assert_eq!(meta.modified(), at(5_002), "та же миллисекунда");
        meta.touch(at(9_000));
        assert_eq!(meta.modified(), at(9_000), "обычная правка");

        // Удаление после перевода часов назад тоже позже прежней правки.
        meta.delete(at(100));
        assert_eq!(meta.deleted(), Some(at(9_001)));
    }

    /// За краем дат JS момента нет: значение прижимается к краю, и `touch`
    /// у края останавливается, а не уходит туда, откуда запись не прочесть.
    #[test]
    fn moments_stay_within_js_dates() {
        let edge = Timestamp::MAX_MILLIS;
        assert_eq!(Timestamp::from_unix_millis(i64::MAX).unix_millis(), edge);
        assert_eq!(Timestamp::from_unix_millis(i64::MIN).unix_millis(), -edge);
        assert_eq!(Timestamp::from_unix_millis(edge).unix_millis(), edge);

        let mut meta = RecordMeta::new(at(edge - 1));
        meta.touch(at(0));
        assert_eq!(meta.modified(), at(edge));
        meta.touch(at(0));
        assert_eq!(meta.modified(), at(edge), "у края момент стоит");
        let text = serde_json::to_string(&meta).expect("поля записываются");
        assert_eq!(
            serde_json::from_str::<RecordMeta>(&text).ok(),
            Some(meta),
            "запись у края читается обратно"
        );
    }

    /// Строка UUID — та же, что в данных.
    #[test]
    fn id_is_shown_as_in_data() {
        let id = RecordId::random();
        assert_eq!(
            serde_json::to_string(&id).expect("идентификатор записывается"),
            format!("\"{id}\"")
        );
        let id: RecordId = serde_json::from_str("\"0F8C3A52-5B1E-4C7D-9A2B-6E4F1D3C2B1A\"")
            .expect("UUID читается");
        assert_eq!(id.to_string(), "0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a");
    }

    /// Поля в данных выписаны человеком: что прочитано, то и записывается
    /// обратно, слово в слово. Поменялась запись — это новая версия формата
    /// (Правило 5), а не правка теста.
    #[test]
    fn data_shape_is_pinned() {
        let samples = [
            r#"{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","modified":1760000000000,"deleted":null}"#,
            r#"{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","modified":1760000000500,"deleted":1760000000000}"#,
            r#"{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","modified":8640000000000000,"deleted":8640000000000000}"#,
            r#"{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","modified":-8640000000000000,"deleted":null}"#,
            // Моменты полей (задача 116): правлены оплаты и курс в `data`.
            concat!(
                r#"{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","modified":1760000005000,"deleted":null,"#,
                r#""edits":{"since":1760000000000,"fields":{"data.rate":1760000005000,"payments":1760000003000}}}"#
            ),
        ];
        let mut wrong = Vec::new();
        for sample in samples {
            match serde_json::from_str::<RecordMeta>(sample)
                .map(|meta| serde_json::to_string(&meta))
            {
                Ok(Ok(back)) if back == sample => {}
                other => wrong.push(format!("{sample} → {other:?}")),
            }
        }
        let meta: RecordMeta = serde_json::from_str(samples[1]).expect("образец читается");
        assert_eq!(
            (meta.modified(), meta.deleted()),
            (at(1_760_000_000_500), Some(at(1_760_000_000_000)))
        );
        assert!(wrong.is_empty(), "запись полей изменилась: {wrong:#?}");
    }

    /// Каждое поле проверяется при чтении: момент — в пределах дат JS,
    /// удаление — не позже правки, идентификатор — UUID, лишних полей нет.
    #[test]
    fn data_rejects_broken_fields() {
        let id = r#""id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a""#;
        let rejected = [
            format!(r#"{{{id},"modified":1000,"deleted":1001}}"#),
            format!(r#"{{{id},"modified":8640000000000001,"deleted":null}}"#),
            format!(r#"{{{id},"modified":-8640000000000001,"deleted":null}}"#),
            format!(r#"{{{id},"modified":9223372036854775807,"deleted":null}}"#),
            format!(r#"{{{id},"modified":1000.5,"deleted":null}}"#),
            format!(r#"{{{id},"modified":"1000","deleted":null}}"#),
            format!(r#"{{{id},"deleted":null}}"#),
            format!(r#"{{{id},"modified":1000,"deleted":null,"color":"red"}}"#),
            r#"{"id":"не-uuid","modified":1000,"deleted":null}"#.to_owned(),
            r#"{"id":[15,140,58,82,91,30,76,125,154,43,110,79,29,60,43,26],"modified":1000,"deleted":null}"#.to_owned(),
            r#"{"modified":1000,"deleted":null}"#.to_owned(),
        ];
        let mut wrong = Vec::new();
        for text in &rejected {
            if let Ok(meta) = serde_json::from_str::<RecordMeta>(text) {
                wrong.push(format!("{text} прочитано как {meta:?}"));
            }
        }
        assert!(wrong.is_empty(), "прочитаны испорченные поля: {wrong:#?}");
    }
}
