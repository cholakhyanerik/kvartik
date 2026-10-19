//! Закрытый квартал (задача 19): снимок расчёта налога с оборота, переноса
//! вычета и версии правил, по которой он посчитан.
//!
//! Квартал закрывает человек, когда расчёт подан: дальше закрытый квартал
//! отдаёт перенос следующему из своего снимка, а не пересчитанный, и новый
//! файл правил его чисел не меняет (Правило 2). Поправили данные закрытого
//! квартала — расчёт разойдётся со снимком (`QuarterTax::differs_from`),
//! и окно скажет, что нужен уточнённый расчёт (приказ ՊԵԿ 299-Ն, прил. 1,
//! п. 1). «Открыть снова» помечает снимок удалённым — записи не стираются
//! (CLAUDE.md, «Записи не стираются»), — а новое закрытие кладёт новый.
//!
//! Меняется только командами: [`Closing::close`], `reopen`.
//!
//! В данных: `{"meta":{…},"data":{"year":2026,"quarter":3,"rules":"2026.2",
//! "lines":[…],"tax":…}}`, строки — как у расчёта (`turnover::Line`).

use std::collections::BTreeMap;

use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize};

use super::rules::Version;
use super::turnover::{self, Line, QuarterTax, Regime};
use crate::format::Stored;
use crate::model::activity::IncomeKind;
use crate::model::date::{Date, Quarter};
use crate::model::record::{RecordId, RecordMeta, Timestamp};

/// Снимок квартала, как его закрыли.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize)]
pub struct ClosingData {
    /// Год квартала.
    pub year: u16,
    /// Номер квартала: от 1 до 4.
    pub quarter: u8,
    /// Версия правил, по которой квартал посчитан.
    pub rules: Version,
    /// Строки расчёта — в порядке таблицы закона.
    pub lines: Vec<Line>,
    /// Налог квартала — сумма строк, драмы.
    pub tax: i64,
}

/// Поля снимка, как они лежат в данных, — до проверки.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ClosingDataFields {
    year: u16,
    quarter: u8,
    rules: Version,
    lines: Vec<Line>,
    tax: i64,
}

impl ClosingData {
    /// Годен ли снимок: квартал из календаря и не раньше правил налога
    /// с оборота, строки — разных видов и по порядку таблицы, суммы не
    /// меньше нуля, налог — сумма строк. Команда пишет только такой; иное
    /// в данных — порча. Квартал раньше правил закрыть нельзя, а снимок
    /// 1-го года из чужой копии растянул бы цепочку переноса на восемь
    /// тысяч кварталов (`turnover::quarter`).
    fn check(&self) -> bool {
        let quarter = Quarter::new(self.year, self.quarter)
            .is_some_and(|quarter| turnover::first_quarter().is_some_and(|first| quarter >= first));
        let ordered = self.lines.windows(2).all(|pair| pair[0].kind < pair[1].kind);
        let amounts = self.lines.iter().all(|line| {
            let deduction = line.deduction.is_none_or(|d| {
                [d.attributed, d.shared, d.expenses, d.current, d.carry_in, d.total, d.floor, d.carry_out]
                    .iter()
                    .all(|amount| *amount >= 0)
            });
            line.income >= 0 && line.gross >= 0 && line.tax >= 0 && deduction
        });
        let total = self
            .lines
            .iter()
            .try_fold(0_i64, |sum, line| sum.checked_add(line.tax));
        quarter && ordered && amounts && total == Some(self.tax)
    }
}

/// Читается только то, что пропустил бы [`ClosingData::check`].
impl<'de> Deserialize<'de> for ClosingData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let ClosingDataFields {
            year,
            quarter,
            rules,
            lines,
            tax,
        } = ClosingDataFields::deserialize(deserializer)?;
        let data = Self {
            year,
            quarter,
            rules,
            lines,
            tax,
        };
        if !data.check() {
            return Err(de::Error::custom("снимок квартала, которого команда не пишет"));
        }
        Ok(data)
    }
}

/// Почему квартал не закрыт.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum CloseError {
    /// Квартал ещё идёт: закрывают поданный расчёт.
    NotOver,
    /// Налог посчитан не целиком — нет курса, вида деятельности или
    /// правила: такой снимок закрепил бы неверные числа.
    Incomplete,
    /// В квартале ИП ушёл с налога с оборота: его расчёт — у бухгалтера.
    Left,
}

/// Закрытый квартал — запись.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Closing {
    meta: RecordMeta,
    data: ClosingData,
}

impl Stored for Closing {
    const KIND: &'static str = "closing";

    fn meta(&self) -> &RecordMeta {
        &self.meta
    }
}

impl Closing {
    /// Команда «закрыть квартал» — по расчёту `tax`, который видит человек,
    /// в день `today`.
    pub fn close(tax: &QuarterTax, today: Date, now: Timestamp) -> Result<Self, CloseError> {
        if tax.quarter.last_day() >= today {
            return Err(CloseError::NotOver);
        }
        if tax.regime != Regime::Turnover {
            return Err(CloseError::Left);
        }
        if !tax.closable(today) {
            return Err(CloseError::Incomplete);
        }
        Ok(Self {
            meta: RecordMeta::new(now),
            data: ClosingData {
                year: tax.quarter.year(),
                quarter: tax.quarter.number(),
                rules: tax.rules,
                lines: tax.lines.clone(),
                tax: tax.tax,
            },
        })
    }

    /// Команда «открыть квартал снова»: метка, а не стирание. `None` — уже
    /// открыт.
    pub fn reopen(&self, now: Timestamp) -> Option<Self> {
        if self.meta.is_deleted() {
            return None;
        }
        let mut closing = self.clone();
        closing.meta.delete(now);
        Some(closing)
    }

    /// Живой снимок квартала `quarter`: изменённый последним. Живых бывает
    /// больше одного, когда квартал закрыли на двух устройствах и слили
    /// копии (задача 41).
    pub fn current(closings: &[Closing], quarter: Quarter) -> Option<&Closing> {
        closings
            .iter()
            .filter(|closing| !closing.is_deleted() && closing.quarter() == quarter)
            .max_by_key(|closing| closing.modified())
    }

    /// Постоянный идентификатор.
    pub const fn id(&self) -> RecordId {
        self.meta.id()
    }

    /// Квартал снимка. Он из календаря: это проверено при чтении.
    pub fn quarter(&self) -> Quarter {
        Quarter::new(self.data.year, self.data.quarter)
            .expect("квартал снимка проверен при чтении и при закрытии")
    }

    /// Снимок.
    pub const fn data(&self) -> &ClosingData {
        &self.data
    }

    /// Открыт ли квартал снова.
    pub const fn is_deleted(&self) -> bool {
        self.meta.is_deleted()
    }

    /// Когда снимок меняли в последний раз.
    pub const fn modified(&self) -> Timestamp {
        self.meta.modified()
    }

    /// Перенос на следующий квартал — из снимка.
    pub fn carry_out(&self) -> BTreeMap<IncomeKind, i64> {
        turnover::carry_of(&self.data.lines)
    }
}

#[cfg(test)]
mod tests;
