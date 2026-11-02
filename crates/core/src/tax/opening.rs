//! Перенос вычета с кварталов до программы (задача 69): остаток по строкам
//! налога на начало первого квартала в программе. Его человек берёт из
//! поданного расчёта — поля x.11 прошлого квартала.
//!
//! Перенос идёт цепочкой (`tax::turnover`), а что было до первой записи,
//! программа не знает: вычет вышел бы меньше настоящего (НК, ст. 258, ч. 7).
//! С этой записью цепочка начинает с поданного, и окно больше не спрашивает.
//! У каких строк перенос мог быть, говорит запись правил на этот квартал
//! ([`kinds`], `rules::QuarterKinds`): из расчётов до 2025 года — только у торговли.
//! С уходом с режима перенос уходит в вычет общего режима (ст. 264, ч. 4), и
//! дальше цепочка его не несёт. Запись важнее снимка соседнего квартала
//! (`turnover::chain_start`): что показано на экране квартала, то и действует.
//!
//! Запись — на квартал, как снимок: что было в поданном расчёте, остаётся
//! фактом и когда книга пополнилась кварталами раньше. Внесли записи раньше
//! начала — окно спросит о новом начале отдельной записью, а прежняя цела
//! и по-прежнему действует со своего квартала. Слияние двух копий с разными
//! началами тоже ничего не теряет. «Переноса не было» — тоже ответ, запись
//! без сумм, и он отличается от «неизвестно».
//!
//! Меняется только командой [`Opening::save`].
//!
//! В данных: `{"meta":{…},"data":{"year":2026,"quarter":3,
//! "carry":{"other":12000}}}`.

use std::collections::BTreeMap;

use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize};

use super::{rules, turnover};
use crate::format::Stored;
use crate::model::activity::IncomeKind;
use crate::model::date::Quarter;
use crate::model::record::{RecordId, RecordMeta, Timestamp};

/// Строки налога, у которых на начало квартала `quarter` мог быть перенос.
/// Пусто — правила на этот квартал нет.
pub fn kinds(quarter: Quarter) -> &'static [IncomeKind] {
    rules::on(rules::armenia().turnover_carry(), quarter.first_day()).map_or(&[], |rule| rule.kinds.as_slice())
}

/// Перенос на начало квартала, как его записали.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize)]
pub struct OpeningData {
    /// Год квартала.
    pub year: u16,
    /// Номер квартала: от 1 до 4.
    pub quarter: u8,
    /// Перенос по строкам налога, драмы. Строк без переноса здесь нет.
    pub carry: BTreeMap<IncomeKind, i64>,
}

/// Поля записи, как они лежат в данных, — до проверки.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OpeningDataFields {
    year: u16,
    quarter: u8,
    carry: BTreeMap<IncomeKind, i64>,
}

impl OpeningData {
    /// Годна ли запись: квартал из календаря и не раньше правил налога
    /// с оборота, суммы больше нуля. Иное в данных — порча: запись первого
    /// года из чужой копии растянула бы цепочку на тысячи кварталов, как
    /// снимок закрытого квартала (`ClosingData`).
    fn check(&self) -> bool {
        let quarter = Quarter::new(self.year, self.quarter)
            .is_some_and(|quarter| turnover::first_quarter().is_some_and(|first| quarter >= first));
        quarter && self.carry.values().all(|amount| *amount > 0)
    }
}

/// Читается только то, что пропустил бы [`OpeningData::check`].
impl<'de> Deserialize<'de> for OpeningData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let OpeningDataFields { year, quarter, carry } = OpeningDataFields::deserialize(deserializer)?;
        let data = Self { year, quarter, carry };
        if !data.check() {
            return Err(de::Error::custom("перенос, которого команда не пишет"));
        }
        Ok(data)
    }
}

/// Почему перенос не принят. Форма до этого не допускает: это порча, а не
/// ошибка человека.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum OpeningError {
    /// Сумма меньше нуля.
    Negative,
    /// У этой строки на начало квартала переноса быть не могло.
    Kind(IncomeKind),
}

/// Перенос с кварталов до программы — запись.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Opening {
    meta: RecordMeta,
    data: OpeningData,
}

impl Stored for Opening {
    const KIND: &'static str = "opening";

    fn meta(&self) -> &RecordMeta {
        &self.meta
    }
}

impl Opening {
    /// Команда «сохранить перенос» на начало квартала `quarter`: новая запись
    /// или правка записи этого квартала, прочитанной в той же транзакции
    /// (`existing`, [`Self::current`]).
    /// Нулевые суммы не хранятся: у строки без переноса записи нет.
    /// `Ok(None)` — ничего не изменилось, и записывать нечего.
    pub fn save(
        existing: Option<Opening>,
        quarter: Quarter,
        carry: BTreeMap<IncomeKind, i64>,
        now: Timestamp,
    ) -> Result<Option<Self>, OpeningError> {
        if carry.values().any(|amount| *amount < 0) {
            return Err(OpeningError::Negative);
        }
        let allowed = kinds(quarter);
        if let Some(kind) = carry.keys().find(|kind| !allowed.contains(kind)) {
            return Err(OpeningError::Kind(*kind));
        }
        let data = OpeningData {
            year: quarter.year(),
            quarter: quarter.number(),
            carry: carry.into_iter().filter(|(_, amount)| *amount > 0).collect(),
        };
        Ok(match existing {
            Some(opening) if opening.data == data => None,
            Some(mut opening) => {
                opening.meta.touch(now);
                opening.data = data;
                Some(opening)
            }
            None => Some(Self {
                meta: RecordMeta::new(now),
                data,
            }),
        })
    }

    /// Запись квартала `quarter`: живая, изменённая последней. Живых бывает
    /// больше одной, когда перенос указали на двух устройствах и слили копии
    /// (задача 41).
    pub fn current(openings: &[Opening], quarter: Quarter) -> Option<&Opening> {
        openings
            .iter()
            .filter(|opening| !opening.is_deleted() && opening.quarter() == quarter)
            .max_by_key(|opening| opening.modified())
    }

    /// Постоянный идентификатор.
    pub const fn id(&self) -> RecordId {
        self.meta.id()
    }

    /// Помечена ли запись удалённой.
    pub const fn is_deleted(&self) -> bool {
        self.meta.is_deleted()
    }

    /// Когда запись меняли в последний раз.
    pub const fn modified(&self) -> Timestamp {
        self.meta.modified()
    }

    /// Квартал, на начало которого указан перенос. Он из календаря: это
    /// проверено при чтении и в команде.
    pub fn quarter(&self) -> Quarter {
        Quarter::new(self.data.year, self.data.quarter).expect("квартал переноса проверен при чтении и в команде")
    }

    /// Перенос по строкам налога, драмы.
    pub const fn carry(&self) -> &BTreeMap<IncomeKind, i64> {
        &self.data.carry
    }
}

#[cfg(test)]
mod tests;
