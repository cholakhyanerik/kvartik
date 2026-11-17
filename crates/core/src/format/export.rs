//! Выгрузка книги и документов за период (задача 30) — для бухгалтера
//! и для Excel, к которому человек привык: XLSX ([`xlsx`]) и CSV ([`csv`]).
//!
//! Листы собирает ядро из тех же строк, что показывает окно: книга —
//! `model::ledger::between` (доход из документов и записи, с курсом и суммой
//! в драмах), документы — выпущенные счета и акты периода со статусом на
//! день выгрузки, закрытые кварталы периода — снимок рядом с расчётом по
//! книге сейчас ([`closings_sheet`], только в XLSX). Слова — на языке окна,
//! а суммы и даты — числами и датами: XLSX хранит их типами, и Excel
//! показывает их по настройкам Windows человека.
//!
//! # CSV — по RFC 4180
//!
//! Поля — через запятую, дробная часть — через точку, даты — ISO 8601
//! (`2026-11-03`), строки — через CRLF, текст — в кавычках (решение
//! человека 2026-11-03): так CSV читают программы учёта. Excel с русскими
//! или армянскими настройками Windows такой файл двойным щелчком откроет
//! одной колонкой, а таблица с русскими настройками может прочесть `12.10`
//! датой (ревью задачи 30, вживую не проверено) — для Excel и Google Таблиц
//! рядом лежит XLSX. В начале — BOM UTF-8: без него Excel показал бы
//! армянский и русский кракозябрами.
//!
//! # Текст, похожий на формулу
//!
//! Ячейка, которая начинается с `=`, `+`, `-`, `@`, табуляции или возврата
//! каретки, открывается в Excel формулой (Правило 13): в имени клиента или
//! в «за что» её мог оставить кто угодно. В CSV такой текст получает
//! апостроф впереди (OWASP, «CSV Injection», сверено 2026-11-03) — и не
//! только в начале поля: Excel с русскими настройками делит строку по `;`
//! ([`csv_text`]). В XLSX — стиль `quotePrefix` (`xlsx`). Числа — не текст:
//! минус суммы остаётся минусом.

mod xlsx;
mod zip;

use std::collections::BTreeSet;

use crate::i18n::{self, Key, Lang, fill, t};
use crate::model::client::Client;
use crate::model::date::Date;
use crate::model::document::{DocKind, Document, Number, SettlementKind, Status};
use crate::model::record::RecordId;
use crate::model::ledger::{self, Cost, Flow, Nature, Operation, Proof, Source};
use crate::model::money::{Currency, Money};
use crate::tax::closing::Closing;
use crate::tax::turnover::{self, Line, QuarterTax, Regime};

pub use xlsx::xlsx;

/// Ячейка выгрузки.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Cell {
    /// Пусто.
    Empty,
    /// Текст.
    Text(String),
    /// Календарная дата.
    Date(Date),
    /// Число `minor` с `places` знаками после запятой: драмы — 0, центы
    /// и лумы — 2, курс — 4 (десятитысячные, `model::money`).
    Number { minor: i64, places: u8 },
}

impl Cell {
    /// Ширина ячейки в знаках — для ширины столбца XLSX.
    fn width(&self) -> usize {
        match self {
            Cell::Empty => 0,
            Cell::Text(text) => text.chars().count(),
            Cell::Date(_) => 10,
            Cell::Number { minor, places } => decimal(*minor, *places).len() + 2,
        }
    }

    fn text(text: &str) -> Self {
        if text.is_empty() { Cell::Empty } else { Cell::Text(text.to_owned()) }
    }

    fn money(money: Money) -> Self {
        Cell::Number {
            minor: money.minor(),
            places: places(money.currency()),
        }
    }

    fn date(day: Option<Date>) -> Self {
        day.map_or(Cell::Empty, Cell::Date)
    }
}

/// Таблица: имя листа, заголовки столбцов и строки.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Sheet {
    pub name: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Cell>>,
}

/// Начинается ли текст так, что Excel прочтёт его формулой (шапка модуля).
fn formula_like(text: &str) -> bool {
    text.starts_with(['=', '+', '-', '@', '\t', '\r'])
}

/// Число `minor` с `places` знаками после точки, без разделителей разрядов:
/// `-1234.56`.
fn decimal(minor: i64, places: u8) -> String {
    if places == 0 {
        return minor.to_string();
    }
    let digits = minor.unsigned_abs().to_string();
    let places = usize::from(places);
    let padded = format!("{digits:0>width$}", width = places + 1);
    let (whole, part) = padded.split_at(padded.len() - places);
    let sign = if minor < 0 { "-" } else { "" };
    format!("{sign}{whole}.{part}")
}

/// Знаков после запятой у суммы в этой валюте.
fn places(currency: Currency) -> u8 {
    u8::try_from(currency.minor_digits()).unwrap_or(2)
}

/// Код валюты ISO 4217 — как в данных.
const fn currency_code(currency: Currency) -> &'static str {
    match currency {
        Currency::Amd => "AMD",
        Currency::Usd => "USD",
        Currency::Eur => "EUR",
    }
}

/// Файл CSV одной таблицы — по RFC 4180, с BOM UTF-8 (шапка модуля).
pub fn csv(sheet: &Sheet) -> Vec<u8> {
    let mut out = String::from("\u{FEFF}");
    let header: Vec<Cell> = sheet.columns.iter().map(|title| Cell::Text(title.clone())).collect();
    for cells in std::iter::once(&header).chain(&sheet.rows) {
        for (at, cell) in cells.iter().enumerate() {
            if at > 0 {
                out.push(',');
            }
            out.push_str(&csv_field(cell));
        }
        out.push_str("\r\n");
    }
    out.into_bytes()
}

/// Строки через `separator` — циклом, а не `join`: `join` у PDF и превью
/// документа, и общий с ними код программа разреза положила бы отдельным
/// куском wasm (CLAUDE.md, Правило 6; замер задачи 30).
fn joined<'a>(parts: impl IntoIterator<Item = &'a str>, separator: &str) -> String {
    let mut out = String::new();
    for (at, part) in parts.into_iter().enumerate() {
        if at > 0 {
            out.push_str(separator);
        }
        out.push_str(part);
    }
    out
}

/// Поле CSV: текст — в кавычках, кавычка внутри — удвоенной, а знак формулы
/// там, где Excel начал бы ячейку, — с апострофом ([`csv_text`]).
fn csv_field(cell: &Cell) -> String {
    match cell {
        Cell::Empty => String::new(),
        Cell::Text(text) => format!("\"{}\"", csv_text(text).replace('"', "\"\"")),
        Cell::Date(day) => day.iso(),
        Cell::Number { minor, places } => decimal(*minor, *places),
    }
}

/// Текст поля CSV, который ни одна программа не прочтёт формулой: апостроф —
/// перед знаком формулы ([`formula_like`]) в начале поля и после каждой `;`.
/// Excel с русскими или армянскими настройками Windows делит строку по `;`,
/// а кавычку замечает только первым знаком ячейки, и «ООО Ромашка;=1+1» дало
/// бы там ячейку `=1+1` (ревью задачи 30; OWASP, «CSV Injection»: ячейку
/// начинают и разделители внутри поля). Пробел и кавычка перед знаком —
/// не защита, их Excel пропустил бы.
fn csv_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 1);
    let mut start = true;
    for c in text.chars() {
        if start && formula_like(c.encode_utf8(&mut [0; 4])) {
            out.push('\'');
            start = false;
        } else if c == ';' {
            start = true;
        } else if c != ' ' && c != '"' {
            start = false;
        }
        out.push(c);
    }
    out
}

/// Имя файла выгрузки: `kvartik-2026-Q3.xlsx`, `kvartik-2026-Q3-ledger.csv`.
/// `period` — `2026-Q3` или `2026`, `part` — что в файле, если это не вся
/// выгрузка. Латиницей: имя файла ходит по почте и облакам, где армянское
/// имя не везде доживает.
pub fn file_name(period: &str, part: Option<&str>, extension: &str) -> String {
    match part {
        Some(part) => format!("kvartik-{period}-{part}.{extension}"),
        None => format!("kvartik-{period}.{extension}"),
    }
}

const fn flow_key(flow: Flow) -> Key {
    match flow {
        Flow::Income => Key::LedgerIncome,
        Flow::Expense => Key::LedgerExpense,
    }
}

const fn kind_key(kind: DocKind) -> Key {
    match kind {
        DocKind::Invoice => Key::KindInvoice,
        DocKind::Act => Key::KindAct,
    }
}

/// Вид расчётного документа словом: в системе ՊԵԿ или чек ККМ (задача 76).
const fn settlement_kind_key(kind: SettlementKind) -> Key {
    match kind {
        SettlementKind::System => Key::ExportSettlementSystem,
        SettlementKind::Receipt => Key::ExportSettlementReceipt,
    }
}

const fn proof_key(proof: Proof) -> Key {
    match proof {
        Proof::TaxInvoice => Key::ProofTaxInvoice,
        Proof::SettlementInvoice => Key::ProofSettlementInvoice,
        Proof::CashReceipt => Key::ProofCashReceipt,
        Proof::IndividualPurchase => Key::ProofIndividualPurchase,
        Proof::Contract => Key::ProofContract,
        Proof::ForeignSupplier => Key::ProofForeignSupplier,
        Proof::ForeignDocument => Key::ProofForeignDocument,
        Proof::BankPayment => Key::ProofBankPayment,
        Proof::None => Key::ProofNone,
    }
}

const fn nature_key(nature: Nature) -> Key {
    match nature {
        Nature::Ordinary => Key::NatureOrdinary,
        Nature::Levy => Key::NatureLevy,
        Nature::Asset => Key::NatureAsset,
        Nature::Travel => Key::NatureTravel,
        Nature::Hospitality => Key::NatureHospitality,
        Nature::NonCulinary => Key::NatureNonCulinary,
    }
}

const fn cost_key(cost: Cost) -> Key {
    match cost {
        Cost::Direct => Key::CostDirect,
        Cost::Overhead => Key::CostOverhead,
    }
}

const fn status_key(status: Status) -> Key {
    match status {
        Status::Draft => Key::StatusDraft,
        Status::Issued => Key::StatusIssued,
        Status::Signed => Key::StatusSigned,
        Status::Unpaid => Key::StatusUnpaid,
        Status::PartlyPaid => Key::StatusPartlyPaid,
        Status::Paid => Key::StatusPaid,
        Status::Overdue => Key::StatusOverdue,
        Status::Annulled => Key::StatusAnnulled,
    }
}

/// Номер документа, как его пишет окно: `2026-001`. Своей записью, а не
/// `to_string` номера: тот зовут PDF и экран документов, и общий с ними код
/// программа разреза положила бы отдельным куском wasm (CLAUDE.md,
/// Правило 6; замер задачи 30). Что запись та же, держит тест.
fn number_text(number: Number) -> String {
    format!("{}-{:03}", number.year(), number.seq())
}

/// «Счёт № 2026-001» — вид и номер выпущенного документа, как в окне.
fn document_title(lang: Lang, document: &Document) -> String {
    let kind = t(lang, kind_key(document.data().kind));
    match document.number() {
        Some(number) => fill(t(lang, Key::DocNumbered), &[kind, &number_text(number)]),
        None => kind.to_owned(),
    }
}

/// Лист книги с дня `from` по день `until` включительно: строки — по дню
/// для налога, в один день доходы выше расходов, а документы — по виду
/// и номеру (`Row::in_day`).
pub fn ledger_sheet(
    lang: Lang,
    documents: &[Document],
    operations: &[Operation],
    clients: &[Client],
    from: Date,
    until: Date,
) -> Sheet {
    let mut rows = ledger::between(documents, operations, from, until);
    rows.sort_by_key(|row| (row.date, row.flow, row.in_day()));
    let client_name = |id| {
        clients
            .iter()
            .find(|client| client.id() == id)
            .map(|client| client.data().name.as_str().to_owned())
            .unwrap_or_default()
    };
    let rows = rows
        .iter()
        .map(|row| {
            let (document, counterparty, what, proof, nature, cost, paid) = match row.source {
                Source::Document(document) => {
                    let buyer = document
                        .issued()
                        .map(|issued| issued.buyer().name.as_str().to_owned())
                        .unwrap_or_default();
                    let items = joined(document.data().items.iter().map(|item| item.text.as_str()), "; ");
                    // Оплаты отмечаются у счёта, а доход по счёту с актом —
                    // акт: его день оплаты — у счёта-основания. Актов по
                    // счёту несколько (этапы) — чьи деньги пришли, не узнать,
                    // и дня нет (Правило 6; ревью задачи 30).
                    let invoice = match (document.data().kind, document.data().basis) {
                        (DocKind::Act, Some(basis)) if live_acts(documents, basis) == 1 => {
                            documents.iter().find(|invoice| invoice.id() == basis)
                        }
                        (DocKind::Act, _) => None,
                        (DocKind::Invoice, _) => Some(document),
                    };
                    // День оплаты — последнего поступления, когда счёт
                    // оплачен целиком; у части — пусто: денег ещё ждут.
                    let paid = invoice
                        .filter(|invoice| invoice.left().is_ok_and(|left| left.minor() == 0))
                        .and_then(|invoice| invoice.payments().iter().map(|payment| payment.date).max());
                    (document_title(lang, document), buyer, items, None, None, None, paid)
                }
                Source::Operation(operation) => {
                    let data = operation.data();
                    let counterparty = match data.client {
                        Some(id) => client_name(id),
                        None => data.supplier.as_str().to_owned(),
                    };
                    let what = data.text.as_str().to_owned();
                    (String::new(), counterparty, what, data.proof, data.nature, data.cost, data.paid)
                }
            };
            vec![
                Cell::Date(row.date),
                Cell::Text(t(lang, flow_key(row.flow)).to_owned()),
                Cell::text(&document),
                Cell::text(&counterparty),
                Cell::text(&what),
                proof.map_or(Cell::Empty, |proof| Cell::Text(t(lang, proof_key(proof)).to_owned())),
                nature.map_or(Cell::Empty, |nature| Cell::Text(t(lang, nature_key(nature)).to_owned())),
                cost.map_or(Cell::Empty, |cost| Cell::Text(t(lang, cost_key(cost)).to_owned())),
                row.activity.map_or(Cell::Empty, |code| Cell::Text(code.to_string())),
                row.income_kind
                    .map_or(Cell::Empty, |kind| Cell::Text(t(lang, turnover::kind_key(kind)).to_owned())),
                Cell::money(row.amount),
                Cell::Text(currency_code(row.amount.currency()).to_owned()),
                row.rate.map_or(Cell::Empty, |rate| Cell::Number {
                    minor: rate.per_unit_e4(),
                    places: 4,
                }),
                row.lumas().map_or(Cell::Empty, |lumas| Cell::Number {
                    minor: lumas.get(),
                    places: 2,
                }),
                Cell::date(paid),
            ]
        })
        .collect();
    let columns = [
        Key::ExportDate,
        Key::ExportFlow,
        Key::ExportDocument,
        Key::ExportCounterparty,
        Key::OpText,
        Key::OpProof,
        Key::OpNature,
        Key::OpCost,
        Key::ActivityCode,
        Key::IncomeKindLabel,
        Key::OpAmount,
        Key::DocCurrency,
        Key::ExportRate,
        Key::ExportAmd,
        Key::ExportPaid,
    ];
    Sheet {
        name: t(lang, Key::LedgerTitle).to_owned(),
        columns: columns.iter().map(|key| t(lang, *key).to_owned()).collect(),
        rows,
    }
}

/// Сколько выпущенных действующих актов по счёту `invoice`.
fn live_acts(documents: &[Document], invoice: RecordId) -> usize {
    documents
        .iter()
        .filter(|act| {
            act.data().kind == DocKind::Act
                && act.data().basis == Some(invoice)
                && act.issued().is_some()
                && act.annulled().is_none()
                && !act.is_deleted()
        })
        .count()
}

/// Сколько строк книги с дня `from` по день `until` в валюте без курса: у них
/// пустая сумма в драмах, и окно называет их число (Правило 2).
pub fn unrated(documents: &[Document], operations: &[Operation], from: Date, until: Date) -> usize {
    ledger::between(documents, operations, from, until)
        .iter()
        .filter(|row| row.unrated())
        .count()
}

/// Лист документов: выпущенные счета и акты с датой документа с дня `from`
/// по день `until` — и аннулированные, чтобы в нумерации не было дыр; статус
/// и остаток — на день `today`. Часы без даты (`None`) — статус пустой:
/// «просрочен» без сегодняшнего дня не узнать (Правило 6).
///
/// Курс и доход в драмах — только у документа дохода (`ledger::
/// income_documents`), как в карточке документа: у аннулированного дохода
/// нет, а у счёта с действующим актом доход — акт, и курс правят у акта.
/// Иначе столбец драм в сумме дал бы оборот вдвое и втрое (ревью задачи 30).
/// По той же причине у такого счёта нет и своей отметки о расчётном
/// документе ՊԵԿ: она у акта, а своя могла устареть (ревью задачи 23).
pub fn documents_sheet(lang: Lang, documents: &[Document], from: Date, until: Date, today: Option<Date>) -> Sheet {
    let income: BTreeSet<RecordId> = ledger::income_documents(documents).iter().map(|document| document.id()).collect();
    let mut issued: Vec<&Document> = documents
        .iter()
        .filter(|document| {
            document.issued().is_some()
                && !document.is_deleted()
                && from <= document.data().date
                && document.data().date <= until
        })
        .collect();
    issued.sort_by_key(|document| (document.data().date, document.data().kind, document.number()));
    let basis = |id| {
        documents
            .iter()
            .find(|document| document.id() == id)
            .map(|document| document_title(lang, document))
            .unwrap_or_default()
    };
    let rows = issued
        .iter()
        .map(|document| {
            let data = document.data();
            let buyer = document.issued().map(|issued| issued.buyer());
            let tin = buyer
                .map(|buyer| match &buyer.tin {
                    Some(tin) => tin.as_str().to_owned(),
                    None => buyer.tax_id.as_str().to_owned(),
                })
                .unwrap_or_default();
            let counted = income.contains(&document.id());
            let covered = data.kind == DocKind::Invoice && document.annulled().is_none() && !counted;
            let settlement = document.settlement().filter(|_| !covered);
            // Оплаты бывают только у счёта, а остаток — у действующего:
            // у акта и аннулированного счёта «осталось» читалось бы долгом.
            let invoice = data.kind == DocKind::Invoice;
            let paid = if invoice { document.paid().map_or(Cell::Empty, Cell::money) } else { Cell::Empty };
            let left = if invoice && document.annulled().is_none() {
                document.left().map_or(Cell::Empty, Cell::money)
            } else {
                Cell::Empty
            };
            vec![
                Cell::Text(t(lang, kind_key(data.kind)).to_owned()),
                document.number().map_or(Cell::Empty, |number| Cell::Text(number_text(number))),
                Cell::Date(data.date),
                Cell::Date(data.service_date),
                Cell::text(buyer.map(|buyer| buyer.name.as_str()).unwrap_or_default()),
                Cell::text(&tin),
                document.total().map_or(Cell::Empty, Cell::money),
                Cell::Text(currency_code(data.currency).to_owned()),
                data.rate.filter(|_| counted).map_or(Cell::Empty, |rate| Cell::Number {
                    minor: rate.per_unit_e4(),
                    places: 4,
                }),
                data.total_lumas().ok().filter(|_| counted).map_or(Cell::Empty, |lumas| Cell::Number {
                    minor: lumas.get(),
                    places: 2,
                }),
                paid,
                left,
                Cell::date(data.due),
                today.map_or(Cell::Empty, |today| Cell::Text(t(lang, status_key(document.status(today))).to_owned())),
                Cell::date(document.signed()),
                Cell::date(document.annulled()),
                data.basis.map_or(Cell::Empty, |id| Cell::text(&basis(id))),
                Cell::date(settlement.map(|settlement| settlement.day)),
                settlement.map_or(Cell::Empty, |settlement| Cell::text(settlement.number.as_str())),
                // Вид — последним столбцом (задача 76): программы учёта,
                // которые читают столбцы по месту, прежние не потеряют.
                settlement.map_or(Cell::Empty, |settlement| {
                    Cell::Text(t(lang, settlement_kind_key(settlement.kind)).to_owned())
                }),
            ]
        })
        .collect();
    let columns = [
        Key::ExportKind,
        Key::ExportNumber,
        Key::DocDate,
        Key::DocServiceDate,
        Key::DocClient,
        Key::ExportClientTin,
        Key::OpAmount,
        Key::DocCurrency,
        Key::ExportRate,
        Key::ExportIncomeAmd,
        Key::ExportPaidSum,
        Key::ExportLeft,
        Key::DocDue,
        Key::ExportStatus,
        Key::HistorySigned,
        Key::StatusAnnulled,
        Key::ExportBasis,
        Key::ExportSettlementDay,
        Key::ExportSettlementNumber,
        Key::ExportSettlementKind,
    ];
    Sheet {
        name: t(lang, Key::ExportDocumentsSheet).to_owned(),
        columns: columns.iter().map(|key| t(lang, *key).to_owned()).collect(),
        rows,
    }
}

/// Лист закрытых кварталов периода (задача 83): у каждого — версия правил
/// снимка и нынешняя, доход и налог в закрытом расчёте и по книге сейчас
/// и сверка их. Выгрузка отдаёт книгу такой, какая она сейчас, а бухгалтер
/// видит файл, а не окно: что квартал поправили после закрытия, он узнаёт
/// отсюда (решение человека 2026-11-03). `closed` — кварталы и их расчёт,
/// как их даёт `turnover::closed_in`.
///
/// Числа сейчас — только у полного расчёта на налоге с оборота
/// (`QuarterTax::known`): часть суммы или ноль выглядели бы расчётом
/// (Правило 6). Неполный расчёт с поданным не сравнивается
/// (`turnover::drifted`) — сверка так и говорит, а не «совпадает». Чем
/// расхождение не видно из двух сумм, сверка называет, как экран налога:
/// налог тот же, а строки другие (перенос, расходы), или с какого дня —
/// общий режим; другие правила видны по двум версиям рядом.
pub fn closings_sheet<'a>(lang: Lang, closed: impl IntoIterator<Item = (&'a QuarterTax, &'a Closing)>) -> Sheet {
    let drams = |value: i64| Cell::Number { minor: value, places: 0 };
    // Доход расчёта — x.1 всех строк. Сложить не вышло — только от
    // испорченных данных, и ячейка пустая, а не неверное число.
    let income = |lines: &[Line]| lines.iter().try_fold(0_i64, |sum, line| sum.checked_add(line.income));
    let rows = closed
        .into_iter()
        .map(|(now, closing)| {
            let filed = closing.data();
            let known = now.known();
            let check = match now.regime {
                _ if !now.complete() => t(lang, Key::ExportClosingUnknown).to_owned(),
                // Год закрытого квартала теперь отмечен в профиле
                // микропредпринимательством (задача 100): расчёт налога
                // с оборота за него подан, а программа налога не считает.
                Regime::Micro => t(lang, Key::ExportClosingMicro).to_owned(),
                Regime::Left { why, .. } if why.micro() => t(lang, Key::ExportClosingMicro).to_owned(),
                Regime::Left { from, .. } => fill(t(lang, Key::ExportClosingLeft), &[&i18n::date(lang, from)]),
                Regime::Turnover if !now.differs_from(closing) => t(lang, Key::ExportClosingSame).to_owned(),
                Regime::Turnover if now.tax == filed.tax => t(lang, Key::ExportClosingSameTax).to_owned(),
                Regime::Turnover => t(lang, Key::ExportClosingDrift).to_owned(),
            };
            vec![
                Cell::Text(i18n::quarter(lang, closing.quarter())),
                Cell::Text(filed.rules.to_string()),
                Cell::Text(now.rules.to_string()),
                income(&filed.lines).map_or(Cell::Empty, drams),
                drams(filed.tax),
                known.and_then(|_| income(&now.lines)).map_or(Cell::Empty, drams),
                known.map_or(Cell::Empty, drams),
                Cell::Text(check),
            ]
        })
        .collect();
    let columns = [
        Key::ExportQuarter,
        Key::ExportClosedRules,
        Key::ExportNowRules,
        Key::ExportClosedIncome,
        Key::ExportClosedTax,
        Key::ExportNowIncome,
        Key::ExportNowTax,
        Key::ExportClosingCheck,
    ];
    Sheet {
        name: t(lang, Key::ExportClosingsSheet).to_owned(),
        columns: columns.iter().map(|key| t(lang, *key).to_owned()).collect(),
        rows,
    }
}

#[cfg(test)]
mod tests;
