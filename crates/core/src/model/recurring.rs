//! Повторяющийся счёт (задача 11): «каждый месяц, этому клиенту».
//!
//! Сервера нет, и по расписанию программа ничего не делает: черновик
//! следующего счёта появляется, когда человек открывает программу в его
//! день или позже. Черновик — копия счёта-образца на `n` месяцев позже:
//! клиент, язык, валюта и строки те же, даты сдвинуты (`Date::add_months`:
//! последний день месяца остаётся последним), срок оплаты — тот же
//! промежуток от даты счёта. Номер черновик получит при выпуске, как любой.
//!
//! Повтор — своя запись, а не поле счёта: остановить его или сдвинуть
//! счётчик — правка повтора, а не выпущенного документа, и правка повтора
//! на одном устройстве не спорит с оплатой, отмеченной на другом
//! (задача 41). Остановить — пометить удалённым; запустить снова — снять
//! метку.
//!
//! В данных: `{"meta":{…},"data":{"source":"…","next":1}}`.

use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize};

use super::date::Date;
use super::document::{DocError, Document};
use super::record::{RecordId, RecordMeta, Timestamp};
use crate::format::Stored;

/// Сколько черновиков одного повтора создаёт одно открытие программы.
/// Кто не открывал её полгода, получит черновик за каждый месяц, а не за
/// последний: счёт нужен за каждый. Предел — чтобы испорченный счётчик или
/// часы, ушедшие на годы вперёд (Правило 6), не завалили окно черновиками;
/// остальные появятся при следующих открытиях.
pub const MAX_CATCH_UP: usize = 12;

/// Повтор, как он лежит в данных.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize)]
pub struct RecurringData {
    /// Счёт-образец.
    pub source: RecordId,
    /// Какой по счёту черновик появится следующим: 1 — через месяц после
    /// образца.
    pub next: u32,
}

/// Поля повтора, как они лежат в данных, — до проверки.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RecurringFields {
    source: RecordId,
    next: u32,
}

/// Черновик номер 0 — сам образец: такого повтор не пишет, и в данных это
/// порча, которая сделала бы копию счёта с теми же датами.
impl<'de> Deserialize<'de> for RecurringData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let RecurringFields { source, next } = RecurringFields::deserialize(deserializer)?;
        if next == 0 {
            return Err(de::Error::custom("черновик повтора с нулевым номером"));
        }
        Ok(Self { source, next })
    }
}

/// Повтор — запись.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recurring {
    meta: RecordMeta,
    data: RecurringData,
}

impl Stored for Recurring {
    const KIND: &'static str = "recurring";

    fn meta(&self) -> &RecordMeta {
        &self.meta
    }
}

impl Recurring {
    /// Команда «повторять этот счёт каждый месяц» в день `today`.
    /// `existing` — повтор этого счёта, если он когда-то был (в том числе
    /// остановленный), прочитанный в той же транзакции. `Ok(None)` — счёт
    /// уже повторяется.
    ///
    /// Первым появится черновик за последний наступивший месяц, а не за все
    /// пропущенные: повтор, запущенный от счёта годичной давности, — это
    /// «дальше каждый месяц», а не двенадцать черновиков разом. Если месяц
    /// после образца ещё не наступил — черновик появится в его день.
    pub fn start(
        existing: Option<&Recurring>,
        source: &Document,
        today: Date,
        now: Timestamp,
    ) -> Result<Option<Self>, DocError> {
        if !source.repeatable() {
            return Err(match (source.issued(), source.annulled()) {
                (None, _) => DocError::NotIssued,
                (Some(_), Some(_)) => DocError::Annulled,
                (Some(_), None) => DocError::NotInvoice,
            });
        }
        let next = first_period(source.data().date, today);
        Ok(match existing {
            Some(recurring) if !recurring.meta.is_deleted() => None,
            Some(recurring) => {
                let mut recurring = recurring.clone();
                recurring.meta.restore(now);
                recurring.data.next = next;
                Some(recurring)
            }
            None => Some(Self {
                meta: RecordMeta::new(now),
                data: RecurringData {
                    source: source.id(),
                    next,
                },
            }),
        })
    }

    /// Команда «остановить повтор»: метка, а не стирание. `None` — уже
    /// остановлен.
    pub fn stop(&self, now: Timestamp) -> Option<Self> {
        if self.meta.is_deleted() {
            return None;
        }
        let mut recurring = self.clone();
        recurring.meta.delete(now);
        Some(recurring)
    }

    /// Черновики, которым к дню `today` пора появиться, и повтор со
    /// сдвинутым счётчиком — их пишут вместе, одной транзакцией. `None` —
    /// писать нечего: рано, повтор остановлен, образец аннулирован или
    /// клиент удалён (`client_alive`).
    pub fn generate(
        &self,
        source: &Document,
        client_alive: bool,
        today: Date,
        now: Timestamp,
    ) -> Option<(Self, Vec<Document>)> {
        if self.meta.is_deleted() || !client_alive || !source.repeatable() {
            return None;
        }
        let mut next = self.data.next;
        let mut drafts = Vec::new();
        while drafts.len() < MAX_CATCH_UP {
            // Календарь кончился — черновиков дальше нет. Данные образца
            // прошли проверку, а сдвиг дат её не ломает: срок оплаты — тот
            // же промежуток от даты счёта.
            let Some(data) = source.repeat_data(next, self.id()) else {
                break;
            };
            if data.date > today {
                break;
            }
            let (Ok(draft), Some(after)) = (Document::create(data, now), next.checked_add(1)) else {
                break;
            };
            drafts.push(draft);
            next = after;
        }
        if drafts.is_empty() {
            return None;
        }
        let mut recurring = self.clone();
        recurring.meta.touch(now);
        recurring.data.next = next;
        Some((recurring, drafts))
    }

    /// День, когда появится следующий черновик; `None` — календарь
    /// кончился.
    pub fn next_date(&self, source: &Document) -> Option<Date> {
        source.data().date.add_months(self.data.next)
    }

    /// Пора ли повтору создать черновик к дню `today` — без самих
    /// черновиков: окно открывает транзакцию записи, только когда пора.
    pub fn due(&self, source: &Document, today: Date) -> bool {
        !self.meta.is_deleted()
            && source.repeatable()
            && self.next_date(source).is_some_and(|date| date <= today)
    }

    /// Повтор счёта `source` — живой или остановленный.
    pub fn of(recurrings: &[Recurring], source: RecordId) -> Option<&Recurring> {
        recurrings
            .iter()
            .filter(|recurring| recurring.data.source == source)
            // Повтор у счёта один; два бывают, только если его запустили
            // на двух устройствах и слили копии (задача 41): тогда — живой,
            // а из живых — изменённый последним.
            .max_by_key(|recurring| (!recurring.meta.is_deleted(), recurring.meta.modified()))
    }

    /// Постоянный идентификатор.
    pub const fn id(&self) -> RecordId {
        self.meta.id()
    }

    /// Данные повтора.
    pub const fn data(&self) -> &RecurringData {
        &self.data
    }

    /// Остановлен ли повтор.
    pub const fn is_stopped(&self) -> bool {
        self.meta.is_deleted()
    }
}

/// Номер первого черновика повтора, запущенного в день `today` от счёта
/// с датой `source`: последний месяц, день которого уже наступил, а если
/// ни один не наступил — первый.
fn first_period(source: Date, today: Date) -> u32 {
    let months = (i64::from(today.year()) * 12 + i64::from(today.month()))
        - (i64::from(source.year()) * 12 + i64::from(source.month()));
    // Месяцев — не больше 120 000: в `u32` они помещаются.
    let mut n = u32::try_from(months.max(1)).unwrap_or(1);
    // В месяце `today` день образца мог ещё не наступить.
    if n > 1 && source.add_months(n).is_none_or(|date| date > today) {
        n -= 1;
    }
    n
}

#[cfg(test)]
mod tests;
