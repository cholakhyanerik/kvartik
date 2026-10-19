//! Книга доходов и расходов (задача 14): операции, которые человек вносит
//! сам, и доход из выпущенных счетов и актов — по кварталам, в валюте
//! и в драмах.
//!
//! **Доход из документов книга берёт сама** ([`income_documents`]): каждый
//! выпущенный действующий акт и каждый такой же счёт, по которому нет
//! выпущенного действующего акта. Счёт и акт по нему — одна услуга
//! (`DocumentData::basis`), и доход — один раз. Сумма — итог документа,
//! дата — день оказания услуги, курс — тот, что указан у документа.
//! Аннулированный, удалённый и черновик дохода не дают. Оплаты документа —
//! деньги на счёте, а не доход (`model::document`).
//!
//! **Операция** ([`Operation`]) — доход без документа в программе или
//! расход. Доход попадает в квартал по дню оказания услуги, расход — по дню,
//! когда товар или услуга получены, а не по дню оплаты: налог с оборота
//! считается по начислению (НК, ст. 15, ч. 1; ст. 40, ч. 4; ст. 258, ч. 6,
//! п. 1, «ա»; по закону: https://www.arlis.am/hy/acts/230455, сверено
//! 2026-10-15). Поэтому у операции две даты: та, что для налога, и день
//! оплаты — деньги.
//!
//! **Курс** вводит человек; неизвестный курс — `None`, а не ноль
//! (Правило 6). Операция в валюте без курса в итог квартала не идёт молча:
//! итог её не считает и называет их число ([`Totals::unrated`]). Итог
//! в драмах — `Money::total` переведённых строк, а не перевод итога
//! в валюте: у каждой строки свой курс.
//!
//! **Расход** помнит факты, из которых задача 19 посчитает вычет: каким
//! документом продавца он подтверждён ([`Proof`]), что это за расход
//! ([`Nature`]) и к какому виду деятельности ИП относится. Сам вычет здесь
//! не считается: его правила — проценты, нижний предел, перенос — данные
//! с источником (задачи 18 и 19; ответы бухгалтера 2026-10-19 —
//! FEATURE_TASKS.md, «PDF против текста закона»).
//!
//! Меняется операция только командами ядра (CLAUDE.md, «Данные меняются
//! одним путём»): [`Operation::create`], `edit`, `save` — с новыми файлами,
//! `delete`, `restore`.
//!
//! В данных: `{"meta":{…},"data":{…}}`; образец — в тесте
//! `data_shape_is_pinned`.

use std::collections::BTreeSet;

use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize};

use super::activity::ActivityCode;
use super::attachment::{Attachment, AttachmentRef, FileBytes};
use super::date::{Date, Quarter};
use super::document::{DocKind, Document};
use super::money::{Currency, ExchangeRate, Money, MoneyError, RateError, RateSource};
use super::record::{RecordId, RecordMeta, Timestamp};
use super::text::Line;
use crate::format::Stored;

/// Доход или расход. В данных — `"income"`, `"expense"`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Flow {
    Income,
    Expense,
}

/// Чем подтверждён расход — документ продавца.
///
/// Виды — по Налоговому кодексу: расчётные документы ст. 55, ч. 2, п. 1–5
/// (налоговый счёт, «հաշիվ վավերագիր», чек ККМ — с ՀՎՀՀ покупателя, ч. 3,
/// п. 2), документ о покупке у физического лица (постановление
/// правительства № 392-Ն), другие документы с обязательными данными по
/// договору с физическим лицом (ч. 13), документы иностранного поставщика
/// (постановление № 547-Ն) и платёжный документ (ч. 12). Какие из них
/// и для каких расходов дают вычет, решает расчёт налога (задача 19); по
/// закону: https://www.arlis.am/hy/acts/230455, сверено 2026-10-15.
///
/// Расширяется, а смысл существующих вариантов не меняется (Правило 2);
/// `match` — без `_`. В данных — имя варианта в `snake_case`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Proof {
    /// Налоговый счёт (հարկային հաշիվ) — от плательщика НДС.
    TaxInvoice,
    /// «Հաշիվ վավերագիր» — расчётный документ продавца без НДС.
    SettlementInvoice,
    /// Чек ККМ с ՀՎՀՀ покупателя.
    CashReceipt,
    /// Документ о покупке у физического лица, не ИП.
    IndividualPurchase,
    /// Договор и акт с физическим лицом — работы, услуги, аренда.
    Contract,
    /// Счёт иностранного поставщика и платёж по нему.
    ForeignSupplier,
    /// Только платёжный документ банка.
    BankPayment,
    /// Документа нет.
    None,
}

impl Proof {
    /// Все виды — в порядке списка выбора.
    pub const ALL: [Proof; 8] = [
        Proof::TaxInvoice,
        Proof::SettlementInvoice,
        Proof::CashReceipt,
        Proof::IndividualPurchase,
        Proof::Contract,
        Proof::ForeignSupplier,
        Proof::BankPayment,
        Proof::None,
    ];
}

/// Что это за расход — то, что вычет делит по закону. Основные средства
/// и нематериальные активы (их покупка и расходы на них), командировки
/// и представительские расходы в вычет не идут (НК, ст. 258, ч. 6, п. 2;
/// по закону: https://www.arlis.am/hy/acts/230455, сверено 2026-10-15).
///
/// Расширяется, а смысл существующих вариантов не меняется (Правило 2);
/// `match` — без `_`. В данных — `"ordinary"`, `"asset"`, `"travel"`,
/// `"hospitality"`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Nature {
    /// Обычный расход: товар, материалы, услуга.
    Ordinary,
    /// Основное средство или нематериальный актив — покупка и расходы на
    /// него.
    Asset,
    /// Командировка.
    Travel,
    /// Представительские расходы.
    Hospitality,
}

impl Nature {
    /// Все виды — в порядке списка выбора.
    pub const ALL: [Nature; 4] = [
        Nature::Ordinary,
        Nature::Asset,
        Nature::Travel,
        Nature::Hospitality,
    ];
}

/// Операция, как её заполнил человек, — то, что меняет форма.
///
/// У дохода нет полей расхода, у расхода — клиента: что годится,
/// проверяет [`OperationData::check`].
#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize)]
pub struct OperationData {
    pub flow: Flow,
    /// День, по которому операция попадает в квартал: у дохода — день
    /// оказания услуги, у расхода — день, когда товар или услуга получены.
    pub date: Date,
    /// День оплаты — деньги, а не налог; `None` — не указан.
    pub paid: Option<Date>,
    /// Сумма в валюте операции — больше нуля.
    pub amount: Money,
    /// Курс валюты к драму; у суммы в драмах и пока курс не указан — `None`.
    /// Дата курса — [`Self::date`] операции ([`Self::rate_for`]).
    pub rate: Option<ExchangeRate>,
    /// За что: «Разработка сайта, сентябрь», «Аренда офиса». Не пустой.
    pub text: Line,
    /// Клиент — у дохода, если он из списка клиентов; у расхода — `None`.
    pub client: Option<RecordId>,
    /// Продавец — у расхода, как он назван в документе; может быть пустым.
    /// У дохода — пусто.
    pub supplier: Line,
    /// Документ продавца — у расхода; у дохода — `None`.
    pub proof: Option<Proof>,
    /// Что это за расход — у расхода; у дохода — `None`.
    pub nature: Option<Nature>,
    /// Вид деятельности ИП, к которому относится расход; `None` — общий:
    /// административные и сбытовые расходы делятся по долям оборота
    /// (НК, ст. 260, ч. 3), а у ИП с одним видом это он и есть. У дохода —
    /// `None`.
    pub activity: Option<ActivityCode>,
    /// Вложения — не больше [`OperationData::MAX_ATTACHMENTS`], без
    /// повторов.
    pub attachments: Vec<AttachmentRef>,
}

/// Поля операции, как они лежат в данных, — до проверки.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OperationDataFields {
    flow: Flow,
    date: Date,
    paid: Option<Date>,
    amount: Money,
    rate: Option<ExchangeRate>,
    text: Line,
    client: Option<RecordId>,
    supplier: Line,
    proof: Option<Proof>,
    nature: Option<Nature>,
    activity: Option<ActivityCode>,
    attachments: Vec<AttachmentRef>,
}

/// Почему операция не принята.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum OperationError {
    /// Не сказано, за что.
    NoText,
    /// Сумма не больше нуля.
    Amount,
    /// Курс не годится: не больше нуля, у суммы в драмах или другой валюты.
    Rate,
    /// Поле не того вида операции: клиент у расхода, документ продавца
    /// у дохода, нет документа или вида у расхода. Форма такого не пишет —
    /// это порча.
    WrongFlow,
    /// Вложений больше [`OperationData::MAX_ATTACHMENTS`] или одно дважды.
    Attachments,
}

impl OperationData {
    /// Вложений у операции — с запасом: чек, счёт и договор.
    pub const MAX_ATTACHMENTS: usize = 10;

    /// Годны ли данные.
    pub fn check(&self) -> Result<(), OperationError> {
        if self.text.is_empty() {
            return Err(OperationError::NoText);
        }
        if self.amount.minor() <= 0 {
            return Err(OperationError::Amount);
        }
        if self
            .rate
            .is_some_and(|rate| rate.currency() != self.amount.currency())
        {
            return Err(OperationError::Rate);
        }
        let fits = match self.flow {
            Flow::Income => {
                self.supplier.is_empty()
                    && self.proof.is_none()
                    && self.nature.is_none()
                    && self.activity.is_none()
            }
            Flow::Expense => self.client.is_none() && self.proof.is_some() && self.nature.is_some(),
        };
        if !fits {
            return Err(OperationError::WrongFlow);
        }
        let unique: BTreeSet<RecordId> = self.attachments.iter().map(|file| file.id).collect();
        if self.attachments.len() > Self::MAX_ATTACHMENTS || unique.len() != self.attachments.len() {
            return Err(OperationError::Attachments);
        }
        Ok(())
    }

    /// Курс валюты операции для её дня ([`Self::date`]): `per_unit_e4` —
    /// драмы за единицу валюты в десятитысячных, как их прочитал
    /// `i18n::parse_rate`. Курс ввёл человек (`RateSource::Manual`). Какой
    /// курс ЦБ брать, говорит закон (НК, ст. 16), а окно — подсказкой.
    pub fn rate_for(&self, per_unit_e4: i64) -> Result<ExchangeRate, OperationError> {
        ExchangeRate::new(self.amount.currency(), per_unit_e4, self.date, RateSource::Manual)
            .map_err(|error| match error {
                RateError::NotPositive | RateError::Amd => OperationError::Rate,
            })
    }

    /// Вложения, которые были в `before` и которых нет здесь: их записи
    /// помечаются убранными в той же транзакции, что и правка операции.
    pub fn dropped_attachments(&self, before: &OperationData) -> Vec<RecordId> {
        before
            .attachments
            .iter()
            .map(|file| file.id)
            .filter(|id| !self.attachments.iter().any(|kept| kept.id == *id))
            .collect()
    }
}

/// Читается только то, что пропустил бы [`OperationData::check`].
impl<'de> Deserialize<'de> for OperationData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let OperationDataFields {
            flow,
            date,
            paid,
            amount,
            rate,
            text,
            client,
            supplier,
            proof,
            nature,
            activity,
            attachments,
        } = OperationDataFields::deserialize(deserializer)?;
        let data = Self {
            flow,
            date,
            paid,
            amount,
            rate,
            text,
            client,
            supplier,
            proof,
            nature,
            activity,
            attachments,
        };
        data.check()
            .map_err(|error| de::Error::custom(format!("операция не годится: {error:?}")))?;
        Ok(data)
    }
}

/// Что записать, чтобы сохранить операцию вместе с файлами, — одной
/// транзакцией ([`Operation::save`]).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Saved {
    /// Операция; `None` — она не изменилась, и записывать её нечего.
    pub operation: Option<Operation>,
    /// Новые вложения — записями.
    pub added: Vec<Attachment>,
    /// Вложения, убранные из операции: их записи окно читает в той же
    /// транзакции и помечает убранными (`Attachment::remove`).
    pub dropped: Vec<RecordId>,
}

/// Операция книги — запись.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Operation {
    meta: RecordMeta,
    data: OperationData,
}

impl Stored for Operation {
    const KIND: &'static str = "operation";

    fn meta(&self) -> &RecordMeta {
        &self.meta
    }
}

impl Operation {
    /// Команда «новая операция».
    pub fn create(data: OperationData, now: Timestamp) -> Result<Self, OperationError> {
        data.check()?;
        Ok(Self {
            meta: RecordMeta::new(now),
            data,
        })
    }

    /// Команда «изменить операцию» — прочитанную в той же транзакции.
    /// `Ok(None)` — данные не изменились, и записывать нечего.
    pub fn edit(&self, data: OperationData, now: Timestamp) -> Result<Option<Self>, OperationError> {
        data.check()?;
        if self.data == data {
            return Ok(None);
        }
        let mut operation = self.clone();
        operation.meta.touch(now);
        operation.data = data;
        Ok(Some(operation))
    }

    /// Команда «сохранить операцию с файлами» — новую (`existing` — `None`)
    /// или прочитанную в той же транзакции. `data.attachments` — те прежние
    /// вложения, что человек оставил; `files` — новые файлы с именами: они
    /// становятся записями-вложениями этой операции, а их ссылки ложатся
    /// в её данные. Убранные прежние вложения — в [`Saved::dropped`].
    pub fn save(
        existing: Option<&Operation>,
        mut data: OperationData,
        files: Vec<(String, FileBytes)>,
        now: Timestamp,
    ) -> Result<Saved, OperationError> {
        let meta = existing.map_or_else(|| RecordMeta::new(now), |known| known.meta.clone());
        let added: Vec<Attachment> = files
            .into_iter()
            .map(|(name, file)| Attachment::create(meta.id(), &name, file, now))
            .collect();
        data.attachments.extend(added.iter().map(Attachment::reference));
        let dropped = existing.map_or_else(Vec::new, |known| data.dropped_attachments(&known.data));
        let operation = match existing {
            None => {
                data.check()?;
                Some(Self { meta, data })
            }
            Some(known) => known.edit(data, now)?,
        };
        Ok(Saved {
            operation,
            added,
            dropped,
        })
    }

    /// Команда «удалить операцию»: метка, а не стирание; вложения остаются
    /// при ней и вернутся вместе с ней. `None` — уже удалена.
    pub fn delete(&self, now: Timestamp) -> Option<Self> {
        if self.meta.is_deleted() {
            return None;
        }
        let mut operation = self.clone();
        operation.meta.delete(now);
        Some(operation)
    }

    /// Команда «восстановить операцию». `None` — не удалена.
    pub fn restore(&self, now: Timestamp) -> Option<Self> {
        if !self.meta.is_deleted() {
            return None;
        }
        let mut operation = self.clone();
        operation.meta.restore(now);
        Some(operation)
    }

    /// Постоянный идентификатор.
    pub const fn id(&self) -> RecordId {
        self.meta.id()
    }

    /// Данные операции.
    pub const fn data(&self) -> &OperationData {
        &self.data
    }

    /// Удалена ли.
    pub const fn is_deleted(&self) -> bool {
        self.meta.is_deleted()
    }

    /// Удалённые операции — последняя удалённая первой.
    pub fn deleted(operations: &[Operation]) -> Vec<&Operation> {
        let mut deleted: Vec<&Operation> = operations.iter().filter(|o| o.is_deleted()).collect();
        deleted.sort_by_key(|operation| std::cmp::Reverse((operation.meta.deleted(), operation.id())));
        deleted
    }
}

/// Документы, которые дают доход: выпущенные, действующие и не удалённые
/// акты — и такие же счета, по которым нет выпущенного действующего акта
/// (шапка модуля). Счёт без акта — доход по своему дню оказания услуги:
/// ИП с иностранным клиентом часто выписывает только счёт.
pub fn income_documents(documents: &[Document]) -> Vec<&Document> {
    let live = |document: &&Document| {
        document.issued().is_some() && document.annulled().is_none() && !document.is_deleted()
    };
    let covered: BTreeSet<RecordId> = documents
        .iter()
        .filter(live)
        .filter(|document| document.data().kind == DocKind::Act)
        .filter_map(|act| act.data().basis)
        .collect();
    documents
        .iter()
        .filter(live)
        .filter(|document| match document.data().kind {
            DocKind::Act => true,
            DocKind::Invoice => !covered.contains(&document.id()),
        })
        .collect()
}

/// Откуда строка книги.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Source<'a> {
    /// Доход из выпущенного документа.
    Document(&'a Document),
    /// Операция, которую внёс человек.
    Operation(&'a Operation),
}

/// Строка книги — документ или операция, приведённые к одному виду.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Row<'a> {
    pub source: Source<'a>,
    pub flow: Flow,
    /// День для налога: оказания услуги или получения.
    pub date: Date,
    /// Сумма в валюте строки.
    pub amount: Money,
    /// Курс к драму; `None` — у драмов или курс не указан.
    pub rate: Option<ExchangeRate>,
}

impl Row<'_> {
    /// Сумма в драмах. Валюта без курса — `RateUnknown`, а не ноль.
    pub fn amd(&self) -> Result<Money, MoneyError> {
        self.amount.to_amd(self.rate)
    }

    /// Не хватает ли курса: валюта без курса в драмы не переводится.
    pub fn unrated(&self) -> bool {
        self.amount.currency() != Currency::Amd && self.rate.is_none()
    }

    /// Идентификатор записи, из которой строка.
    pub const fn id(&self) -> RecordId {
        match self.source {
            Source::Document(document) => document.id(),
            Source::Operation(operation) => operation.id(),
        }
    }
}

/// Строки книги за квартал: доход из документов и живые операции, свежие
/// сверху; в один день — доходы выше расходов.
pub fn rows<'a>(documents: &'a [Document], operations: &'a [Operation], quarter: Quarter) -> Vec<Row<'a>> {
    let from_documents = income_documents(documents).into_iter().filter_map(|document| {
        let data = document.data();
        // Итог помещается: это проверено при записи и при чтении.
        let amount = document.total().ok()?;
        Some(Row {
            source: Source::Document(document),
            flow: Flow::Income,
            date: data.service_date,
            amount,
            rate: data.rate,
        })
    });
    let from_operations = operations
        .iter()
        .filter(|operation| !operation.is_deleted())
        .map(|operation| {
            let data = operation.data();
            Row {
                source: Source::Operation(operation),
                flow: data.flow,
                date: data.date,
                amount: data.amount,
                rate: data.rate,
            }
        });
    let mut rows: Vec<Row<'a>> = from_documents
        .chain(from_operations)
        .filter(|row| row.date.quarter() == quarter)
        .collect();
    rows.sort_by(|a, b| (b.date, a.flow, b.id()).cmp(&(a.date, b.flow, a.id())));
    rows
}

/// Итог квартала в драмах.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Totals {
    /// Доход — сумма переведённых строк дохода.
    pub income: Money,
    /// Расходы — сумма переведённых строк расхода.
    pub expense: Money,
    /// Строк в валюте без курса: в итог они не вошли.
    pub unrated: usize,
}

/// Итог строк в драмах: каждая строка переводится своим курсом, и итог —
/// сумма переведённых (`Money::total`). Строки без курса в итог не идут,
/// а считаются ([`Totals::unrated`]): итог без них занижен, и окно говорит
/// это словами. Ошибка — только переполнение от испорченных данных.
pub fn totals(rows: &[Row<'_>]) -> Result<Totals, MoneyError> {
    let mut unrated = 0;
    let (mut income, mut expense) = (Vec::new(), Vec::new());
    for row in rows {
        match row.amd() {
            Ok(amd) => match row.flow {
                Flow::Income => income.push(amd),
                Flow::Expense => expense.push(amd),
            },
            Err(MoneyError::RateUnknown) => unrated += 1,
            Err(error) => return Err(error),
        }
    }
    Ok(Totals {
        income: Money::total(Currency::Amd, income)?,
        expense: Money::total(Currency::Amd, expense)?,
        unrated,
    })
}

#[cfg(test)]
mod tests;
