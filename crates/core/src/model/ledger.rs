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
//! в драмах — сумма переведённых строк с лумами (`Lumas::total`), а не
//! перевод итога в валюте: у каждой строки свой курс, а до драма итог
//! доводит строка расчёта налога (`model::money`).
//!
//! **Расход** помнит факты, из которых расчёт налога (`tax::turnover`,
//! задача 19) считает вычет: каким документом продавца он подтверждён
//! ([`Proof`]), что это за расход ([`Nature`]) и к какому виду деятельности
//! ИП относится. Какие из них дают вычет, решают правила с источником
//! в файле правил (`tax::rules`), а не этот модуль. **Доход** без документа
//! в программе тоже помнит вид деятельности — у ИП, чьи коды облагаются по
//! разным строкам (решение человека 2026-10-19), — и строку налога, если она
//! не та, что у кода: аренда, проценты, роялти или продажа имущества
//! (задача 70, `tax::turnover`).
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

use super::activity::{ActivityCode, IncomeKind};
use super::attachment::{Attachment, AttachmentRef, FileBytes};
use super::date::{Date, Quarter};
use super::document::{DocKind, Document, Number};
use super::money::{Currency, ExchangeRate, Lumas, Money, MoneyError, RateError, RateSource};
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
/// `match` — без `_`. В данных — `"ordinary"`, `"levy"`, `"asset"`,
/// `"travel"`, `"hospitality"`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Nature {
    /// Обычный расход: товар, материалы, услуга.
    Ordinary,
    /// Не покупка: зарплата, налог, пошлина, сбор — или услуга банка.
    /// Такому расходу хватает платёжного документа (НК, ст. 55, ч. 12; по
    /// закону, сверено по arlis 230455 2026-10-19). С версии формата 8
    /// (задача 19, решение человека 2026-10-19): прежде книга не отличала
    /// его от обычного, а обычному одного платежа мало. С 0.0.64 окно зовёт
    /// этим видом только зарплату и комиссию банка: налог и пошлина в вычет
    /// не идут (`Key::NatureHint`); записи прежних версий считаются как были.
    Levy,
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
    pub const ALL: [Nature; 5] = [
        Nature::Ordinary,
        Nature::Levy,
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
    /// Вид деятельности ИП, к которому относится операция. У расхода
    /// `None` — общий: административные и сбытовые расходы делятся по долям
    /// оборота (НК, ст. 260, ч. 3), а у ИП с одним видом это он и есть, пока
    /// в квартале нет дохода со своей строкой (`tax::turnover`).
    /// У дохода `None` — по кодам профиля, если они облагаются по одной
    /// строке (`tax::turnover`); код у дохода — с версии формата 8.
    pub activity: Option<ActivityCode>,
    /// Строка налога дохода, если она не та, что у кода
    /// ([`IncomeKind::OF_INCOME`]); `None` — по коду. У расхода — `None`.
    pub income_kind: Option<IncomeKind>,
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
    /// С версии формата 13 (задача 70): у записи прошлых версий строка — по
    /// коду.
    #[serde(default)]
    income_kind: Option<IncomeKind>,
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
    /// Поле не того вида операции: клиент или строка налога дохода у расхода,
    /// документ продавца или вид расхода у дохода, нет документа или вида
    /// у расхода, строка, которую у дохода не выбирают. Форма такого не пишет —
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
                    && self.income_kind.is_none_or(|kind| IncomeKind::OF_INCOME.contains(&kind))
            }
            Flow::Expense => {
                self.client.is_none() && self.proof.is_some() && self.nature.is_some() && self.income_kind.is_none()
            }
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
            income_kind,
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
            income_kind,
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
    /// Вид деятельности ИП, к которому относится строка; `None` — не указан
    /// (у расхода — общий).
    pub activity: Option<ActivityCode>,
    /// Строка налога, выбранная у самого дохода; `None` — по коду.
    pub income_kind: Option<IncomeKind>,
}

impl Row<'_> {
    /// Сумма в драмах с лумами. Валюта без курса — `RateUnknown`, а не ноль.
    pub fn lumas(&self) -> Result<Lumas, MoneyError> {
        self.amount.to_lumas(self.rate)
    }

    /// Документ продавца и вид расхода — у расхода; у дохода — `None`.
    pub fn expense(&self) -> Option<(Proof, Nature)> {
        match self.source {
            Source::Document(_) => None,
            Source::Operation(operation) => {
                let data = operation.data();
                data.proof.zip(data.nature)
            }
        }
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

    /// Место строки среди строк того же дня и вида: документы — по виду
    /// и номеру, за ними записи книги. Идентификатор — случайный UUID — решает
    /// только между записями без номера: по нему одному акт 2026-003 вставал
    /// раньше 2026-002 (полный прогон 0.0.96, выгрузка).
    pub fn in_day(&self) -> (u8, Option<(DocKind, Option<Number>)>, RecordId) {
        match self.source {
            Source::Document(document) => (0, Some((document.data().kind, document.number())), document.id()),
            Source::Operation(operation) => (1, None, operation.id()),
        }
    }
}

/// Строки книги за квартал: доход из документов и живые операции, свежие
/// сверху; в один день — доходы выше расходов, а среди них — тоже свежие
/// сверху: документ с большим номером выше ([`Row::in_day`]).
pub fn rows<'a>(documents: &'a [Document], operations: &'a [Operation], quarter: Quarter) -> Vec<Row<'a>> {
    let mut rows = between(documents, operations, quarter.first_day(), quarter.last_day());
    rows.sort_by(|a, b| (b.date, a.flow, b.in_day()).cmp(&(a.date, b.flow, a.in_day())));
    rows
}

/// Строки книги с дня `from` по день `until` включительно — в порядке
/// записей: расчёт налога (`tax::turnover`) раскладывает их сам.
pub fn between<'a>(
    documents: &'a [Document],
    operations: &'a [Operation],
    from: Date,
    until: Date,
) -> Vec<Row<'a>> {
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
            activity: data.activity,
            income_kind: data.income_kind,
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
                activity: data.activity,
                income_kind: data.income_kind,
            }
        });
    from_documents
        .chain(from_operations)
        .filter(|row| from <= row.date && row.date <= until)
        .collect()
}

/// Итог квартала в драмах с лумами.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Totals {
    /// Доход — сумма переведённых строк дохода.
    pub income: Lumas,
    /// Расходы — сумма переведённых строк расхода.
    pub expense: Lumas,
    /// Строк в валюте без курса: в итог они не вошли.
    pub unrated: usize,
}

/// Итог строк в драмах: каждая строка переводится своим курсом до лумы,
/// и итог — сумма переведённых (`Lumas::total`). Строки без курса в итог не
/// идут, а считаются ([`Totals::unrated`]): итог без них занижен, и окно
/// говорит это словами. Ошибка — только переполнение от испорченных данных.
pub fn totals(rows: &[Row<'_>]) -> Result<Totals, MoneyError> {
    let mut unrated = 0;
    let (mut income, mut expense) = (Vec::new(), Vec::new());
    for row in rows {
        match row.lumas() {
            Ok(lumas) => match row.flow {
                Flow::Income => income.push(lumas),
                Flow::Expense => expense.push(lumas),
            },
            Err(MoneyError::RateUnknown) => unrated += 1,
            Err(error) => return Err(error),
        }
    }
    Ok(Totals {
        income: Lumas::total(income)?,
        expense: Lumas::total(expense)?,
        unrated,
    })
}

#[cfg(test)]
mod tests;
