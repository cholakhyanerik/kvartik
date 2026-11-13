//! Тесты книги. Образцы записи и ожидаемые числа выписаны человеком, а не
//! записаны кодом: код, записавший себе эталон, проверял бы сам себя
//! (Правило 2).

use super::*;
use crate::i18n::Lang;
use crate::model::attachment::FileKind;
use crate::model::client::{Client, ClientData};
use crate::model::country::Country;
use crate::model::document::{DocumentData, Item};
use crate::model::profile::ProfileData;
use crate::model::text::Lines;

fn at(millis: i64) -> Timestamp {
    Timestamp::from_unix_millis(millis)
}

fn day(year: u16, month: u8, day: u8) -> Date {
    Date::new(year, month, day).expect("в тесте — существующая дата")
}

fn line(text: &str) -> Line {
    Line::new(text).expect("в тесте — годный текст")
}

fn quarter(year: u16, number: u8) -> Quarter {
    Quarter::new(year, number).expect("в тесте — существующий квартал")
}

fn usd(minor: i64) -> Money {
    Money::new(minor, Currency::Usd)
}

fn amd(minor: i64) -> Money {
    Money::new(minor, Currency::Amd)
}

/// Доход 1 500,00 $ за сентябрь: услуга оказана 30 сентября, оплачена
/// 5 октября, курс 388,00 ֏, клиент из списка, одно вложение.
const INCOME: &str = concat!(
    r#"{"meta":{"id":"5d1c2b3a-4f5e-4d6c-8b7a-9f8e7d6c5b4a","modified":1760000000000,"deleted":null},"#,
    r#""data":{"flow":"income","date":"2026-09-30","paid":"2026-10-05","#,
    r#""amount":{"minor":150000,"currency":"USD"},"#,
    r#""rate":{"currency":"USD","per_unit_e4":3880000,"date":"2026-09-30","source":"manual"},"#,
    r#""text":"Web development, September","client":"5b0e8f9a-3c2d-4e1f-8a7b-9c6d5e4f3a2b","#,
    r#""supplier":"","proof":null,"nature":null,"cost":null,"activity":null,"income_kind":null,"#,
    r#""attachments":[{"id":"3f2b8c1d-6a4e-4f7b-9c2d-1e0f5a6b7c8d","name":"act.pdf","kind":"pdf","size":183402}]}}"#
);

/// Расход 60 000 ֏ — аренда офиса за октябрь: получена 31 октября, документ
/// — «հաշիվ վավերագիր», сбытовой и административный, вид деятельности
/// 62.01.0, оплаты ещё не было.
const EXPENSE: &str = concat!(
    r#"{"meta":{"id":"6e2d3c4b-5a6f-4e7d-9c8b-0a9f8e7d6c5b","modified":1760000000500,"deleted":null},"#,
    r#""data":{"flow":"expense","date":"2026-10-31","paid":null,"#,
    r#""amount":{"minor":60000,"currency":"AMD"},"rate":null,"#,
    r#""text":"Аренда офиса, октябрь","client":null,"#,
    r#""supplier":"«Արարատ» ՍՊԸ","proof":"settlement_invoice","nature":"ordinary","cost":"overhead","activity":"62.01.0","income_kind":null,"#,
    r#""attachments":[]}}"#
);

/// Образцы читаются в ожидаемые значения, а их запись — слово в слово
/// образцы. Поменялась запись — это новая версия формата (Правило 5).
#[test]
fn data_shape_is_pinned() {
    let income: Operation = serde_json::from_str(INCOME).expect("доход читается");
    let data = income.data();
    assert_eq!(data.flow, Flow::Income);
    assert_eq!((data.date, data.paid), (day(2026, 9, 30), Some(day(2026, 10, 5))));
    assert_eq!(data.amount, usd(150_000));
    assert_eq!(data.rate.map(ExchangeRate::per_unit_e4), Some(3_880_000));
    assert_eq!(data.attachments.len(), 1);
    assert_eq!(data.attachments[0].kind, FileKind::Pdf);
    assert_eq!(serde_json::to_string(&income).ok().as_deref(), Some(INCOME));

    let expense: Operation = serde_json::from_str(EXPENSE).expect("расход читается");
    let data = expense.data();
    assert_eq!(data.flow, Flow::Expense);
    assert_eq!(data.amount, amd(60_000));
    assert_eq!(data.supplier.as_str(), "«Արարատ» ՍՊԸ");
    assert_eq!(data.proof, Some(Proof::SettlementInvoice));
    assert_eq!(data.nature, Some(Nature::Ordinary));
    assert_eq!(data.cost, Some(Cost::Overhead));
    assert_eq!(data.activity.map(|code| code.to_string()).as_deref(), Some("62.01.0"));
    assert_eq!(serde_json::to_string(&expense).ok().as_deref(), Some(EXPENSE));
}

/// Испорченное не читается: незнакомое поле, запретное значение, поле не
/// того вида операции.
#[test]
fn data_rejects_what_commands_never_write() {
    let income = |from: &str, to: &str| INCOME.replacen(from, to, 1);
    let expense = |from: &str, to: &str| EXPENSE.replacen(from, to, 1);
    let attachment = r#"{"id":"3f2b8c1d-6a4e-4f7b-9c2d-1e0f5a6b7c8d","name":"act.pdf","kind":"pdf","size":183402}"#;
    let cases = [
        income(r#""attachments":"#, r#""note":"x","attachments":"#),
        income(r#""flow":"income""#, r#""flow":"refund""#),
        income(r#""minor":150000"#, r#""minor":0"#),
        income(r#""minor":150000"#, r#""minor":-150000"#),
        income(r#""text":"Web development, September""#, r#""text":"""#),
        income(r#""currency":"USD","per_unit_e4""#, r#""currency":"EUR","per_unit_e4""#),
        income(r#""per_unit_e4":3880000"#, r#""per_unit_e4":0"#),
        income(r#""proof":null"#, r#""proof":"tax_invoice""#),
        income(r#""supplier":"""#, r#""supplier":"Кто-то""#),
        income(r#""nature":null"#, r#""nature":"ordinary""#),
        income(r#""income_kind":null"#, r#""income_kind":"salary""#),
        income(r#""income_kind":null"#, r#""income_kind":"high_tech""#),
        income(
            attachment,
            &format!("{attachment},{attachment}"),
        ),
        expense(r#""client":null"#, r#""client":"5b0e8f9a-3c2d-4e1f-8a7b-9c6d5e4f3a2b""#),
        expense(r#""proof":"settlement_invoice""#, r#""proof":null"#),
        expense(r#""nature":"ordinary""#, r#""nature":null"#),
        expense(r#""nature":"ordinary""#, r#""nature":"luxury""#),
        income(r#""cost":null"#, r#""cost":"direct""#),
        expense(r#""cost":"overhead""#, r#""cost":"indirect""#),
        expense(r#""income_kind":null"#, r#""income_kind":"rent""#),
        expense(r#""rate":null"#, r#""rate":{"currency":"USD","per_unit_e4":3880000,"date":"2026-10-31","source":"manual"}"#),
        expense(r#""deleted":null"#, r#""deleted":1760000000600"#),
    ];
    let mut wrong = Vec::new();
    for (index, text) in cases.iter().enumerate() {
        if text == INCOME || text == EXPENSE {
            wrong.push(format!("случай {index} не изменил образец"));
        } else if serde_json::from_str::<Operation>(text).is_ok() {
            wrong.push(format!("случай {index} прочитан"));
        }
    }
    assert!(wrong.is_empty(), "прочитано испорченное: {wrong:#?}");
}

/// С версии формата 8 у дохода — вид деятельности, а у расхода — вид
/// «зарплата, комиссия банка» (`levy`).
#[test]
fn income_names_its_activity_and_levy_is_a_nature() {
    let income = INCOME.replacen(r#""activity":null"#, r#""activity":"62.01.0""#, 1);
    let read: Operation = serde_json::from_str(&income).expect("доход с видом читается");
    assert_eq!(read.data().activity, ActivityCode::parse("62.01.0").ok());
    assert_eq!(serde_json::to_string(&read).ok().as_deref(), Some(income.as_str()));
    let levy = EXPENSE.replacen(r#""nature":"ordinary""#, r#""nature":"levy""#, 1);
    let read: Operation = serde_json::from_str(&levy).expect("расход levy читается");
    assert_eq!(read.data().nature, Some(Nature::Levy));
    assert_eq!(serde_json::to_string(&read).ok().as_deref(), Some(levy.as_str()));
}

/// С версии формата 20 у расхода — вид «товар, который общепит продаёт, не
/// готовя» (задача 94): в данных — `non_culinary`, записывается и читается.
#[test]
fn a_catering_expense_names_goods_sold_uncooked() {
    let sample = EXPENSE.replacen(r#""nature":"ordinary""#, r#""nature":"non_culinary""#, 1);
    assert_ne!(sample, EXPENSE, "подмена не сработала");
    let read: Operation = serde_json::from_str(&sample).expect("расход non_culinary читается");
    assert_eq!(read.data().nature, Some(Nature::NonCulinary));
    assert_eq!(serde_json::to_string(&read).ok().as_deref(), Some(sample.as_str()));
}

/// С версии формата 13 у дохода — своя строка налога, если она не та, что
/// у кода, а запись версии 12 — без поля — читается со строкой по коду
/// и пишется уже с полем (Правило 2).
#[test]
fn income_names_its_tax_row() {
    let mut wrong = Vec::new();
    for (name, kind, expected) in [("rent", IncomeKind::Rent, "rent"), ("assets", IncomeKind::Assets, "assets")] {
        let income = INCOME.replacen(r#""income_kind":null"#, &format!(r#""income_kind":"{expected}""#), 1);
        match serde_json::from_str::<Operation>(&income) {
            Ok(read) if read.data().income_kind == Some(kind) => {
                if serde_json::to_string(&read).ok().as_deref() != Some(income.as_str()) {
                    wrong.push(format!("{name}: записан не тем, чем прочитан"));
                }
            }
            other => wrong.push(format!("{name}: {other:?}")),
        }
    }
    for (name, sample) in [("доход", INCOME), ("расход", EXPENSE)] {
        let old = sample.replacen(r#""income_kind":null,"#, "", 1);
        if old == sample {
            wrong.push(format!("{name}: образец без поля не получился"));
            continue;
        }
        match serde_json::from_str::<Operation>(&old) {
            Ok(read) if read.data().income_kind.is_none() => {
                if serde_json::to_string(&read).ok().as_deref() != Some(sample) {
                    wrong.push(format!("{name}: записан не с полем"));
                }
            }
            other => wrong.push(format!("{name}: {other:?}")),
        }
    }
    assert!(wrong.is_empty(), "строка налога у дохода: {wrong:#?}");
}

/// С версии формата 19 расход называет строку своего дохода (задача 95):
/// аренду или продажу имущества — записывается и читается, — а другую строку,
/// строку вместе с кодом или с «прямой ли» команда не пропускает, и чтение
/// тоже: такой записи форма не пишет.
#[test]
fn an_expense_names_the_row_of_its_income() {
    let mut wrong = Vec::new();
    let own = EXPENSE.replacen(r#""cost":"overhead","activity":"62.01.0","#, r#""cost":null,"activity":null,"#, 1);
    if own == EXPENSE {
        wrong.push("образец без кода и ответа не получился".to_owned());
    }
    for (kind, name) in [(IncomeKind::Rent, "rent"), (IncomeKind::Assets, "assets")] {
        let sample = own.replacen(r#""income_kind":null"#, &format!(r#""income_kind":"{name}""#), 1);
        match serde_json::from_str::<Operation>(&sample) {
            Ok(read) if read.data().income_kind == Some(kind) => {
                if serde_json::to_string(&read).ok().as_deref() != Some(sample.as_str()) {
                    wrong.push(format!("{name}: записан не тем, чем прочитан"));
                }
            }
            other => wrong.push(format!("{name}: {other:?}")),
        }
        let data = OperationData {
            income_kind: Some(kind),
            ..expense_data(day(2026, 10, 3), amd(12_000))
        };
        if Operation::create(data, at(1_000)).is_err() {
            wrong.push(format!("{name}: команда не пропустила строку дохода"));
        }
    }
    let rent = own.replacen(r#""income_kind":null"#, r#""income_kind":"rent""#, 1);
    for (name, text) in [
        ("строка торговли", own.replacen(r#""income_kind":null"#, r#""income_kind":"trade""#, 1)),
        ("строка с кодом", rent.replacen(r#""activity":null"#, r#""activity":"62.01.0""#, 1)),
        ("строка с ответом", rent.replacen(r#""cost":null"#, r#""cost":"direct""#, 1)),
    ] {
        if text == own || text == rent {
            wrong.push(format!("{name}: подмена не сработала"));
        } else if serde_json::from_str::<Operation>(&text).is_ok() {
            wrong.push(format!("{name}: прочитано"));
        }
    }
    let base = || OperationData {
        income_kind: Some(IncomeKind::Rent),
        ..expense_data(day(2026, 10, 3), amd(12_000))
    };
    for (name, data) in [
        ("строка торговли", OperationData { income_kind: Some(IncomeKind::Trade), ..base() }),
        ("строка с кодом", OperationData { activity: ActivityCode::parse("62.01.0").ok(), ..base() }),
        ("строка с ответом", OperationData { cost: Some(Cost::Overhead), ..base() }),
    ] {
        if Operation::create(data, at(1_000)) != Err(OperationError::WrongFlow) {
            wrong.push(format!("{name}: команда пропустила"));
        }
    }
    assert!(wrong.is_empty(), "строка дохода у расхода: {wrong:#?}");
}

/// С версии формата 18 расход говорит, прямой ли он (задача 79): оба ответа
/// записываются и читаются, запись версии 17 — без поля — читается как «не
/// сказано», а не «прямой» (Правило 6), и пишется уже с полем; у дохода
/// команда поля не пропускает.
#[test]
fn an_expense_says_whether_it_is_direct() {
    let mut wrong = Vec::new();
    for (cost, name) in [(Cost::Direct, "direct"), (Cost::Overhead, "overhead")] {
        let sample = EXPENSE.replacen(r#""cost":"overhead""#, &format!(r#""cost":"{name}""#), 1);
        match serde_json::from_str::<Operation>(&sample) {
            Ok(read) if read.data().cost == Some(cost) => {
                if serde_json::to_string(&read).ok().as_deref() != Some(sample.as_str()) {
                    wrong.push(format!("{name}: записан не тем, чем прочитан"));
                }
            }
            other => wrong.push(format!("{name}: {other:?}")),
        }
    }
    for (name, sample) in [("доход", INCOME.to_owned()), ("расход", EXPENSE.replacen(r#""cost":"overhead""#, r#""cost":null"#, 1))] {
        let old = sample.replacen(r#""cost":null,"#, "", 1);
        if old == sample {
            wrong.push(format!("{name}: образец без поля не получился"));
            continue;
        }
        match serde_json::from_str::<Operation>(&old) {
            Ok(read) if read.data().cost.is_none() => {
                if serde_json::to_string(&read).ok().as_deref() != Some(sample.as_str()) {
                    wrong.push(format!("{name}: записан не с полем"));
                }
            }
            other => wrong.push(format!("{name}: {other:?}")),
        }
    }
    let income = OperationData {
        cost: Some(Cost::Direct),
        ..income_data(day(2026, 10, 3), amd(12_000))
    };
    if Operation::create(income, at(1_000)) != Err(OperationError::WrongFlow) {
        wrong.push("у дохода команда пропустила «прямой»".to_owned());
    }
    assert!(wrong.is_empty(), "прямой ли расход: {wrong:#?}");
}

fn income_data(date: Date, amount: Money) -> OperationData {
    OperationData {
        flow: Flow::Income,
        date,
        paid: None,
        amount,
        rate: None,
        text: line("Консультация"),
        client: None,
        supplier: Line::default(),
        proof: None,
        nature: None,
        cost: None,
        activity: None,
        income_kind: None,
        attachments: Vec::new(),
    }
}

fn expense_data(date: Date, amount: Money) -> OperationData {
    OperationData {
        flow: Flow::Expense,
        supplier: line("«Վեբ» ՍՊԸ"),
        proof: Some(Proof::TaxInvoice),
        nature: Some(Nature::Ordinary),
        text: line("Хостинг"),
        ..income_data(date, amount)
    }
}

/// Операция правится, удаляется меткой и восстанавливается; правка без
/// изменений ничего не пишет.
#[test]
fn operations_are_edited_deleted_and_restored() {
    let data = expense_data(day(2026, 10, 3), amd(12_000));
    let operation = Operation::create(data.clone(), at(1_000)).expect("годна");
    assert_eq!(operation.edit(data.clone(), at(2_000)), Ok(None));
    let mut more = data.clone();
    more.amount = amd(15_000);
    let edited = operation.edit(more, at(2_000)).expect("годна").expect("правка");
    assert_eq!(edited.id(), operation.id());
    assert_eq!(edited.meta().modified(), at(2_000));
    let deleted = edited.delete(at(3_000)).expect("удалена");
    assert!(deleted.is_deleted());
    assert_eq!(deleted.delete(at(4_000)), None);
    let restored = deleted.restore(at(4_000)).expect("восстановлена");
    assert!(!restored.is_deleted());
    assert_eq!(restored.data().amount, amd(15_000));
    assert_eq!(Operation::deleted(&[restored, deleted.clone()]), [&deleted]);

    let mut empty = data;
    empty.text = Line::default();
    assert_eq!(Operation::create(empty, at(1_000)), Err(OperationError::NoText));
}

/// Курс — валюты операции и на её день; у драмов курса нет.
#[test]
fn rate_belongs_to_the_operation() {
    let data = income_data(day(2026, 9, 30), usd(150_000));
    let rate = data.rate_for(3_880_000).expect("курс годится");
    assert_eq!((rate.currency(), rate.date(), rate.source()), (Currency::Usd, day(2026, 9, 30), RateSource::Manual));
    assert_eq!(data.rate_for(0), Err(OperationError::Rate));
    let drams = income_data(day(2026, 9, 30), amd(100_000));
    assert_eq!(drams.rate_for(3_880_000), Err(OperationError::Rate));
}

/// Убранные вложения — те, что были и пропали; оставленные и новые — нет.
#[test]
fn dropped_attachments_are_found() {
    let file = |name: &str| AttachmentRef {
        id: RecordId::random(),
        name: line(name),
        kind: FileKind::Jpeg,
        size: 1_000,
    };
    let (kept, gone, new) = (file("1.jpg"), file("2.jpg"), file("3.jpg"));
    let mut before = expense_data(day(2026, 10, 3), amd(12_000));
    before.attachments = vec![kept.clone(), gone.clone()];
    let mut after = before.clone();
    after.attachments = vec![kept, new];
    assert_eq!(after.dropped_attachments(&before), [gone.id]);
    assert_eq!(before.dropped_attachments(&before), [] as [RecordId; 0]);
}

fn pdf(len: usize) -> FileBytes {
    let mut bytes = b"%PDF-1.7\n".to_vec();
    bytes.resize(len, b' ');
    FileBytes::new(bytes).expect("годный PDF")
}

/// Новая операция с файлами: вложения — записи этой операции, их ссылки —
/// в её данных. Правка: оставленные остаются, убранные названы, новые
/// прибавлены; без изменений и без файлов писать нечего.
#[test]
fn save_ties_files_to_the_operation() {
    let data = expense_data(day(2026, 10, 3), amd(12_000));
    let first = Operation::save(None, data, vec![("чек.pdf".to_owned(), pdf(10)), ("акт.pdf".to_owned(), pdf(20))], at(1_000))
        .expect("годна");
    let operation = first.operation.expect("новая операция");
    assert_eq!(first.added.len(), 2);
    assert!(first.added.iter().all(|file| file.owner() == operation.id()));
    let refs: Vec<RecordId> = operation.data().attachments.iter().map(|file| file.id).collect();
    assert_eq!(refs, first.added.iter().map(Attachment::id).collect::<Vec<_>>());
    assert_eq!(operation.data().attachments[1].size, 20);
    assert!(first.dropped.is_empty());

    // Убрать первый файл и приложить третий.
    let mut kept = operation.data().clone();
    let gone = kept.attachments.remove(0);
    let second = Operation::save(Some(&operation), kept, vec![("фото.pdf".to_owned(), pdf(30))], at(2_000))
        .expect("годна");
    let edited = second.operation.expect("правка");
    assert_eq!(edited.id(), operation.id());
    assert_eq!(edited.meta().modified(), at(2_000));
    assert_eq!(second.dropped, [gone.id]);
    assert_eq!(second.added[0].owner(), operation.id());
    let names: Vec<&str> = edited.data().attachments.iter().map(|file| file.name.as_str()).collect();
    assert_eq!(names, ["акт.pdf", "фото.pdf"]);

    let same = Operation::save(Some(&edited), edited.data().clone(), Vec::new(), at(3_000)).expect("годна");
    assert_eq!(same, Saved { operation: None, added: Vec::new(), dropped: Vec::new() });

    let eleven: Vec<(String, FileBytes)> = (0..11).map(|n| (format!("{n}.pdf"), pdf(10))).collect();
    assert_eq!(
        Operation::save(None, expense_data(day(2026, 10, 3), amd(1)), eleven, at(1_000)),
        Err(OperationError::Attachments)
    );
}

/// Клиент для проб.
fn client() -> Client {
    let data = ClientData {
        name: line("Beispiel GmbH"),
        country: Country::from_code("DE"),
        address: Lines::default(),
        tin: None,
        tax_id: Line::default(),
        currency: Some(Currency::Usd),
        lang: Some(Lang::En),
        payment_days: None,
    };
    Client::create(data, at(1_000)).expect("годный клиент")
}

fn profile() -> ProfileData {
    let mut data = ProfileData::default();
    data.name.en = line("IE Aram Petrosyan");
    data
}

/// Данные документа: одна строка в `price` центов, день оказания услуги —
/// `service`.
fn document_data(kind: DocKind, client: &Client, date: Date, service: Date, price: i64) -> DocumentData {
    DocumentData {
        kind,
        client: client.id(),
        lang: Lang::En,
        currency: Currency::Usd,
        date,
        service_date: service,
        due: None,
        items: vec![Item {
            text: line("Development"),
            quantity_e3: 1_000,
            price,
        }],
        basis: None,
        recurring: None,
        rate: None,
        activity: None,
        income_kind: None,
    }
}

fn issue(data: DocumentData, client: &Client, documents: &[Document]) -> Document {
    Document::create(data, at(2_000))
        .expect("годный черновик")
        .issue(documents, 0, Some(&profile()), client, at(3_000))
        .expect("выпускается")
}

/// Доход даёт выпущенный действующий акт и такой же счёт без акта; счёт,
/// по которому выпущен акт, — нет: услуга одна. Черновик, удалённый
/// черновик и аннулированный документ дохода не дают, а аннулированный акт
/// возвращает доход его счёту.
#[test]
fn income_comes_once_per_service() {
    let client = client();
    let october = day(2026, 10, 1);
    let mut documents = Vec::new();
    // Счёт 1 с выпущенным актом: доход — акт.
    let invoiced = issue(document_data(DocKind::Invoice, &client, october, october, 100_000), &client, &documents);
    documents.push(invoiced.clone());
    let act = issue(invoiced.act_data(october).expect("акт по счёту"), &client, &documents);
    documents.push(act.clone());
    // Счёт 2 только с черновиком акта: доход — счёт.
    let lone = issue(document_data(DocKind::Invoice, &client, october, october, 200_000), &client, &documents);
    documents.push(lone.clone());
    documents.push(Document::create(lone.act_data(october).expect("акт"), at(4_000)).expect("черновик акта"));
    // Счёт 3 с аннулированным актом: доход — снова счёт.
    let third = issue(document_data(DocKind::Invoice, &client, october, october, 300_000), &client, &documents);
    documents.push(third.clone());
    let annulled_act = issue(third.act_data(october).expect("акт"), &client, &documents)
        .annul(october, at(5_000))
        .expect("выпущен")
        .expect("аннулирован");
    documents.push(annulled_act);
    // Аннулированный счёт, черновик и удалённый черновик — не доход.
    let annulled = issue(document_data(DocKind::Invoice, &client, october, october, 400_000), &client, &documents)
        .annul(october, at(5_000))
        .expect("выпущен")
        .expect("аннулирован");
    documents.push(annulled);
    let draft = Document::create(document_data(DocKind::Invoice, &client, october, october, 500_000), at(2_000))
        .expect("черновик");
    documents.push(draft.delete(at(6_000)).expect("черновик").expect("удалён"));
    documents.push(draft);

    let mut income: Vec<RecordId> = income_documents(&documents).into_iter().map(Document::id).collect();
    income.sort();
    let mut expected = vec![act.id(), lone.id(), third.id()];
    expected.sort();
    assert_eq!(income, expected);
}

/// Квартал — по дню оказания услуги, а не по дате документа и не по дню
/// оплаты: счёт от 1 октября за услугу 30 сентября — доход третьего
/// квартала; расход, полученный 30 июня и оплаченный 5 июля, — второго.
#[test]
fn quarter_follows_service_not_payment() {
    let client = client();
    let invoice = issue(
        document_data(DocKind::Invoice, &client, day(2026, 10, 1), day(2026, 9, 30), 100_000),
        &client,
        &[],
    );
    let mut bought = expense_data(day(2026, 6, 30), amd(50_000));
    bought.paid = Some(day(2026, 7, 5));
    let expense = Operation::create(bought, at(1_000)).expect("годна");
    let documents = [invoice.clone()];
    let operations = [expense.clone()];
    let third: Vec<RecordId> = rows(&documents, &operations, quarter(2026, 3)).iter().map(Row::id).collect();
    let second: Vec<RecordId> = rows(&documents, &operations, quarter(2026, 2)).iter().map(Row::id).collect();
    assert_eq!(third, [invoice.id()]);
    assert_eq!(second, [expense.id()]);
    assert!(rows(&documents, &operations, quarter(2026, 4)).is_empty());
}

/// Подпись акта клиентом дохода не двигает (задача 62): акт от 1 октября
/// за услугу 30 сентября, подписанный 5 января, — доход третьего квартала
/// тем же днём и той же суммой, что и до подписи.
#[test]
fn signature_does_not_move_income() {
    let client = client();
    let act = issue(
        document_data(DocKind::Act, &client, day(2026, 10, 1), day(2026, 9, 30), 100_000),
        &client,
        &[],
    );
    let signed = act
        .sign(day(2027, 1, 5), at(4_000))
        .expect("годно")
        .expect("подписан");
    let line = |document: &Document| {
        let documents = [document.clone()];
        rows(&documents, &[], quarter(2026, 3))
            .iter()
            .map(|row| (row.id(), row.date, row.amount))
            .collect::<Vec<_>>()
    };
    assert_eq!(line(&act), [(act.id(), day(2026, 9, 30), usd(100_000))]);
    assert_eq!(line(&signed), line(&act));
    let documents = [signed];
    assert!(rows(&documents, &[], quarter(2027, 1)).is_empty());
}

/// Удалённая операция в книгу не идёт; строки — свежие сверху, в один день
/// доход выше расхода.
#[test]
fn rows_are_live_and_newest_first() {
    let first = Operation::create(income_data(day(2026, 10, 2), amd(10_000)), at(1_000)).expect("годна");
    let spent = Operation::create(expense_data(day(2026, 10, 9), amd(3_000)), at(1_000)).expect("годна");
    let earned = Operation::create(income_data(day(2026, 10, 9), amd(20_000)), at(1_000)).expect("годна");
    let gone = Operation::create(income_data(day(2026, 10, 5), amd(1_000)), at(1_000))
        .expect("годна")
        .delete(at(2_000))
        .expect("удалена");
    let operations = [first.clone(), spent.clone(), earned.clone(), gone];
    let ids: Vec<RecordId> = rows(&[], &operations, quarter(2026, 4)).iter().map(Row::id).collect();
    assert_eq!(ids, [earned.id(), spent.id(), first.id()]);
}

/// В один день документы — тоже свежие сверху: по виду и номеру, а не по
/// случайному идентификатору, по которому акт 2026-003 вставал в выгрузке
/// раньше 2026-002 (полный прогон 0.0.96). Восемь документов одного дня:
/// случайный порядок совпал бы с номерами раз на десятки тысяч.
#[test]
fn documents_of_a_day_follow_their_numbers() {
    let client = client();
    let october = day(2026, 10, 9);
    let mut documents = Vec::new();
    for kind in [DocKind::Invoice, DocKind::Act] {
        for _ in 0..4 {
            let document = issue(document_data(kind, &client, october, october, 100_000), &client, &documents);
            documents.push(document);
        }
    }
    let got: Vec<String> = rows(&documents, &[], quarter(2026, 4))
        .iter()
        .filter_map(|row| match row.source {
            Source::Document(document) => Some(format!("{:?} {}", document.data().kind, document.number()?)),
            Source::Operation(_) => None,
        })
        .collect();
    let mut expected: Vec<String> = (1..=4).rev().map(|seq| format!("Act 2026-00{seq}")).collect();
    expected.extend((1..=4).rev().map(|seq| format!("Invoice 2026-00{seq}")));
    assert_eq!(got, expected);
}

/// Итог в драмах — сумма переведённых строк с лумами. Три поступления по
/// 10,00 $ по 361,93: каждое — 3 619,30 ֏, доход — 10 857,90 ֏; до драма
/// его доводит строка расчёта налога, а не книга. Строка в евро без курса
/// в итог не идёт, а считается; расход в драмах — как есть.
#[test]
fn totals_add_converted_lines_and_count_unrated() {
    let ten = |date| {
        let mut data = income_data(date, usd(1_000));
        data.rate = Some(data.rate_for(3_619_300).expect("курс"));
        Operation::create(data, at(1_000)).expect("годна")
    };
    let no_rate = Operation::create(income_data(day(2026, 10, 4), Money::new(5_000, Currency::Eur)), at(1_000))
        .expect("годна");
    let spent = Operation::create(expense_data(day(2026, 10, 5), amd(4_500)), at(1_000)).expect("годна");
    let operations = [
        ten(day(2026, 10, 1)),
        ten(day(2026, 10, 2)),
        ten(day(2026, 10, 3)),
        no_rate,
        spent,
    ];
    let rows = rows(&[], &operations, quarter(2026, 4));
    assert_eq!(rows.iter().filter(|row| row.unrated()).count(), 1);
    assert_eq!(
        totals(&rows),
        Ok(Totals {
            income: Lumas::new(1_085_790),
            expense: Lumas::new(450_000),
            unrated: 1,
        })
    );
    assert_eq!(
        totals(&[]),
        Ok(Totals {
            income: Lumas::ZERO,
            expense: Lumas::ZERO,
            unrated: 0,
        })
    );
}

/// Доход документа — его итог в валюте по курсу документа, а без курса —
/// строка без курса, а не ноль.
#[test]
fn document_income_uses_the_document_rate() {
    let client = client();
    let service = day(2026, 9, 30);
    let mut data = document_data(DocKind::Invoice, &client, day(2026, 10, 1), service, 240_000);
    data.rate = Some(data.rate_for(3_880_000).expect("курс"));
    let rated = issue(data, &client, &[]);
    let unrated = issue(document_data(DocKind::Invoice, &client, day(2026, 10, 2), service, 100_000), &client, &[]);
    let documents = [rated, unrated];
    let rows = rows(&documents, &[], quarter(2026, 3));
    // 2 400,00 $ × 388,00 = 931 200,00 ֏ — как в дизайне «Kvartik Design».
    assert_eq!(
        totals(&rows),
        Ok(Totals {
            income: Lumas::new(93_120_000),
            expense: Lumas::ZERO,
            unrated: 1,
        })
    );
}
