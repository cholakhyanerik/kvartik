//! Тесты расчётного документа: какие документы его ждут и до какого дня.
//! Ожидаемые дни выписаны человеком по записи файла правил, а не посчитаны
//! кодом (Правило 2).

use super::*;
use crate::i18n::Lang;
use crate::model::activity::ActivityCode;
use crate::model::client::{Client, ClientData};
use crate::model::country::Country;
use crate::model::document::{DocKind, DocumentData, Item, Settlement, SettlementKind};
use crate::model::money::Currency;
use crate::model::profile::ProfileData;
use crate::model::record::{RecordId, Timestamp};
use crate::model::text::{Line, Lines};

fn at(millis: i64) -> Timestamp {
    Timestamp::from_unix_millis(millis)
}

fn day(year: u16, month: u8, day: u8) -> Date {
    Date::new(year, month, day).expect("в тесте — существующая дата")
}

fn line(text: &str) -> Line {
    Line::new(text).expect("в тесте — годный текст")
}

/// Армянская фирма — покупатель.
fn client() -> Client {
    client_from(Some(Country::ARMENIA))
}

/// Покупатель из страны `country`; `None` — страна не выбрана.
fn client_from(country: Option<Country>) -> Client {
    let data = ClientData {
        name: line("«Արարատ» ՍՊԸ"),
        country,
        address: Lines::default(),
        tin: None,
        tax_id: Line::default(),
        currency: Some(Currency::Amd),
        lang: Some(Lang::Am),
        payment_days: None,
    };
    Client::create(data, at(1_000)).expect("годный клиент")
}

fn profile() -> ProfileData {
    let mut data = ProfileData::default();
    data.name.hy = line("ԱՁ Արամ Պետրոսյան");
    data
}

/// Счёт в драмах на 100 000 ֏ от 1 октября за услугу, оказанную в день
/// `service`.
fn invoice_data(client: &Client, service: Date) -> DocumentData {
    DocumentData {
        kind: DocKind::Invoice,
        client: client.id(),
        lang: Lang::Am,
        currency: Currency::Amd,
        date: day(2026, 10, 1),
        service_date: service,
        due: None,
        items: vec![Item {
            text: line("Ծրագրային ապահովման մշակում"),
            quantity_e3: 1_000,
            price: 100_000,
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
        .issue(documents, 0, Some(&profile()), Some(None), client, at(3_000))
        .expect("выпускается")
}

/// Отметить расчётный документ днём `day`.
fn settle(document: &Document, day: Date) -> Document {
    document
        .set_settlement(
            Settlement {
                day,
                number: Line::default(),
                kind: SettlementKind::System,
            },
            at(4_000),
        )
        .expect("годно")
        .expect("отмечен")
}

/// По записи файла правил расчётный документ по услуге выписывается в день
/// её оказания — и до 1 сентября 2026 года, и после; до 2025 года правила
/// в файле нет, и срока программа не знает.
#[test]
fn due_is_the_day_of_service() {
    let mut wrong = Vec::new();
    for (service, expected) in [
        (day(2026, 9, 30), Some(day(2026, 9, 30))),
        // Последний день прежней редакции ст. 56 и первый — новой.
        (day(2026, 8, 31), Some(day(2026, 8, 31))),
        (day(2026, 9, 1), Some(day(2026, 9, 1))),
        (day(2025, 1, 1), Some(day(2025, 1, 1))),
        // Суббота: момент поставки на рабочий день не переносится.
        (day(2026, 10, 31), Some(day(2026, 10, 31))),
        (day(2024, 12, 31), None),
    ] {
        if due(service) != expected {
            wrong.push(format!("{service:?}: {:?}, а не {expected:?}", due(service)));
        }
    }
    assert!(wrong.is_empty(), "срок расчётного документа: {wrong:#?}");
}

/// Тот же документ, каким его записала версия до 0.0.55: без ключа отметки
/// — выписан ли его документ, программа не знает.
fn from_before(document: &Document) -> Document {
    let text = serde_json::to_string(document).expect("записывается");
    serde_json::from_str(&text.replacen(r#""settlement":null,"#, "", 1)).expect("читается")
}

/// Ждут расчётного документа документы дохода без отметки — как в книге:
/// акт, а не его счёт; счёт без акта. Отмеченный, черновик, аннулированный,
/// удалённый и выпущенный до 0.0.55 — не ждут: о последнем программа не
/// знает (Правило 6). Порядок — по сроку, ранние сверху.
#[test]
fn pending_are_income_documents_without_a_mark() {
    let client = client();
    let mut documents = Vec::new();
    // Счёт с актом: ждёт акт, а не счёт.
    let invoiced = issue(invoice_data(&client, day(2026, 9, 30)), &client, &documents);
    documents.push(invoiced.clone());
    let act = issue(invoiced.act_data(day(2026, 10, 2)).expect("акт по счёту"), &client, &documents);
    documents.push(act.clone());
    // Счёт без акта — ждёт сам; услуга раньше — выше.
    let early = issue(invoice_data(&client, day(2026, 9, 15)), &client, &documents);
    documents.push(early.clone());
    // Отмеченный не ждёт — и отмеченный чеком ККМ (задача 76).
    let marked = settle(&issue(invoice_data(&client, day(2026, 9, 1)), &client, &documents), day(2026, 9, 1));
    documents.push(marked);
    let receipt = issue(invoice_data(&client, day(2026, 9, 4)), &client, &documents)
        .set_settlement(
            Settlement {
                day: day(2026, 9, 4),
                number: Line::default(),
                kind: SettlementKind::Receipt,
            },
            at(4_000),
        )
        .expect("годно")
        .expect("отмечен");
    documents.push(receipt);
    // Аннулированный, черновик и удалённый черновик не ждут.
    let annulled = issue(invoice_data(&client, day(2026, 9, 2)), &client, &documents)
        .annul(day(2026, 10, 5), at(5_000))
        .expect("выпущен")
        .expect("аннулирован");
    documents.push(annulled);
    let draft = Document::create(invoice_data(&client, day(2026, 9, 3)), at(2_000)).expect("черновик");
    documents.push(draft.delete(at(6_000)).expect("черновик").expect("удалён"));
    documents.push(draft);
    // Выпущенный до 0.0.55 — «неизвестно», а не «не выписан».
    let old = from_before(&issue(invoice_data(&client, day(2026, 8, 1)), &client, &documents));
    assert!(!old.settlement_known());
    documents.push(old);

    let got: Vec<(RecordId, Option<Date>)> =
        pending(&documents).iter().map(|item| (item.document.id(), item.due)).collect();
    assert_eq!(
        got,
        [(early.id(), Some(day(2026, 9, 15))), (act.id(), Some(day(2026, 9, 30)))]
    );
}

/// Пора выписать — когда последний день наступил или прошёл: 30 сентября
/// услуга оказана — 30 сентября пора, 29-го — ещё нет. Выпущенный до 0.0.55
/// в счёт не идёт, хоть его день и прошёл.
#[test]
fn due_count_counts_what_has_come() {
    let client = client();
    let mut documents = Vec::new();
    for service in [day(2026, 9, 15), day(2026, 9, 30), day(2026, 10, 31)] {
        let next = issue(invoice_data(&client, service), &client, &documents);
        documents.push(next);
    }
    let old = from_before(&issue(invoice_data(&client, day(2026, 9, 1)), &client, &documents));
    documents.push(old);
    let mut wrong = Vec::new();
    for (today, expected) in [
        (day(2026, 9, 14), 0),
        (day(2026, 9, 15), 1),
        (day(2026, 9, 29), 1),
        (day(2026, 9, 30), 2),
        (day(2026, 11, 1), 3),
    ] {
        if due_count(&documents, today) != expected {
            wrong.push(format!("{today:?}: {}, а не {expected}", due_count(&documents, today)));
        }
    }
    assert!(wrong.is_empty(), "пора выписать: {wrong:#?}");
}

/// Расчётный документ, выписанный в квартале раньше услуги, — окно
/// предупреждает (задача 77): подтвердил его покупатель до окончания услуги
/// — доход в квартале подтверждения (НК, ст. 40, ч. 4). Ожидаемые кварталы
/// выписаны по этому правилу руками: тот же квартал, день после услуги,
/// покупатель из другой страны, товар (ст. 38), аренда и продажа имущества
/// — без предупреждения; страна не выбрана и строку не узнать (кодов
/// в профиле нет) — с ним.
#[test]
fn early_settlement_is_the_earlier_quarter() {
    let quarter = |year, number| Quarter::new(year, number).expect("в тесте — существующий квартал");
    let code = |text| ActivityCode::parse(text).expect("в тесте — годный код");
    let armenian = client();
    let foreign = client_from(Country::from_code("US"));
    let unknown = client_from(None);
    let mut it = profile();
    it.activities = vec![code("62.01.0")];
    let mut shop = profile();
    shop.activities = vec![code("47.91.0")];
    shop.kinds.insert(code("47.91.0"), IncomeKind::Trade);
    let mut wrong = Vec::new();
    for (name, buyer, own, service, settled, expected) in [
        ("III квартал при услуге IV", &armenian, None, day(2026, 10, 15), day(2026, 9, 30), Some(quarter(2026, 3))),
        ("тот же квартал, раньше", &armenian, None, day(2026, 10, 15), day(2026, 10, 1), None),
        ("в день услуги", &armenian, None, day(2026, 10, 15), day(2026, 10, 15), None),
        ("после услуги, в другом квартале", &armenian, None, day(2026, 9, 30), day(2026, 10, 1), None),
        ("через год", &armenian, None, day(2026, 1, 5), day(2025, 12, 31), Some(quarter(2025, 4))),
        ("покупатель из США", &foreign, None, day(2026, 10, 15), day(2026, 9, 30), None),
        ("страна не выбрана", &unknown, None, day(2026, 10, 15), day(2026, 9, 30), Some(quarter(2026, 3))),
        ("аренда у самого дохода", &armenian, Some(IncomeKind::Rent), day(2026, 10, 15), day(2026, 9, 30), None),
        ("продажа имущества", &armenian, Some(IncomeKind::Assets), day(2026, 10, 15), day(2026, 9, 30), None),
    ] {
        let mut data = invoice_data(buyer, service);
        data.income_kind = own;
        let document = issue(data, buyer, &[]);
        let got = early(&document, Some(&profile()), settled, SettlementKind::System);
        if got != expected {
            wrong.push(format!("{name}: {got:?}, а не {expected:?}"));
        }
    }
    // Строка — по коду профиля, как в налоге: услуги IT — с предупреждением,
    // торговля — без него.
    let document = issue(invoice_data(&armenian, day(2026, 10, 15)), &armenian, &[]);
    for (name, profile, expected) in [
        ("IT по коду", &it, Some(quarter(2026, 3))),
        ("торговля по коду", &shop, None),
    ] {
        let got = early(&document, Some(profile), day(2026, 9, 30), SettlementKind::System);
        if got != expected {
            wrong.push(format!("{name}: {got:?}, а не {expected:?}"));
        }
    }
    // Чек ККМ (задача 76) покупатель в системе ՊԵԿ не подтверждает.
    if let Some(quarter) = early(&document, Some(&it), day(2026, 9, 30), SettlementKind::Receipt) {
        wrong.push(format!("чек ККМ: {quarter:?}, а не None"));
    }
    // Аннулированный документ дохода не даёт — и слов о квартале нет (ревью
    // задачи 77).
    let annulled = document.annul(day(2026, 10, 20), at(5_000)).expect("выпущен").expect("аннулирован");
    if let Some(quarter) = early(&annulled, Some(&it), day(2026, 9, 30), SettlementKind::System) {
        wrong.push(format!("аннулированный: {quarter:?}, а не None"));
    }
    assert!(wrong.is_empty(), "расчётный документ раньше квартала услуги: {wrong:#?}");
}
