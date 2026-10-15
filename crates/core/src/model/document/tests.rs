//! Тесты счёта и акта. Образцы записи и ожидаемые числа выписаны человеком,
//! а не записаны кодом: код, записавший себе эталон, проверял бы сам себя
//! (Правило 2).

use super::*;
use crate::model::country::Country;
use crate::model::profile::Profile;

fn at(millis: i64) -> Timestamp {
    Timestamp::from_unix_millis(millis)
}

fn day(year: u16, month: u8, day: u8) -> Date {
    Date::new(year, month, day).expect("в тесте — существующая дата")
}

fn line(text: &str) -> Line {
    Line::new(text).expect("в тесте — годный текст")
}

/// Выпущенный счёт в долларах: две строки — 5 000,00 $ и 12,5 часа по
/// 40,00 $, итого 5 500,00 $, — оплачен двумя платежами, 2 000,00 $
/// и 3 500,00 $; курс для книги — 388,00 ֏ на день оказания услуги.
const INVOICE: &str = concat!(
    r#"{"meta":{"id":"7c9e6679-7425-40de-944b-e07fc1f90ae7","modified":1760000000000,"deleted":null},"#,
    r#""data":{"kind":"invoice","client":"5b0e8f9a-3c2d-4e1f-8a7b-9c6d5e4f3a2b","lang":"en","currency":"USD","#,
    r#""date":"2026-10-01","service_date":"2026-09-30","due":"2026-10-15","#,
    r#""items":[{"text":"Software development services, September 2026","quantity_e3":1000,"price":500000},"#,
    r#"{"text":"Code review","quantity_e3":12500,"price":4000}],"#,
    r#""basis":null,"recurring":null,"#,
    r#""rate":{"currency":"USD","per_unit_e4":3880000,"date":"2026-09-30","source":"manual"}},"#,
    r#""issued":{"number":{"year":2026,"seq":7},"#,
    r#""seller":{"name":{"hy":"ԱՁ Արամ Պետրոսյան","ru":"ИП Арам Петросян","en":"IE Aram Petrosyan"},"#,
    r#""tin":"07654321","registration":"271.1234567","#,
    r#""address":{"hy":"ք. Երևան, Աբովյան 1","ru":"г. Ереван, ул. Абовяна 1","en":"Abovyan St 1, Yerevan"},"#,
    r#""phone":"+374 91 123456","email":"aram@example.com","#,
    r#""accounts":[{"currency":"USD","number":"127027482877","bank":"Ameriabank CJSC","swift":"ARMIAM22","#,
    r#""correspondent":"Citibank N.A., New York"}]},"#,
    r#""buyer":{"name":"Beispiel GmbH","country":"DE","address":"Hauptstraße 1\n10115 Berlin","tin":null,"#,
    r#""tax_id":"DE123456789","currency":"EUR","lang":"en","payment_days":14}},"#,
    r#""annulled":null,"#,
    r#""payments":[{"date":"2026-10-10","amount":200000},{"date":"2026-10-20","amount":350000}]}"#
);

/// Черновик акта в драмах по этому счёту — по-армянски, одна строка.
const ACT: &str = concat!(
    r#"{"meta":{"id":"7c9e6679-7425-40de-944b-e07fc1f90ae8","modified":1760000000500,"deleted":null},"#,
    r#""data":{"kind":"act","client":"5b0e8f9a-3c2d-4e1f-8a7b-9c6d5e4f3a2b","lang":"hy","currency":"AMD","#,
    r#""date":"2026-10-13","service_date":"2026-10-13","due":null,"#,
    r#""items":[{"text":"Ծրագրային ապահովման մշակում","quantity_e3":1000,"price":1500000}],"#,
    r#""basis":"7c9e6679-7425-40de-944b-e07fc1f90ae7","recurring":null,"rate":null},"#,
    r#""issued":null,"annulled":null,"payments":[]}"#
);

/// Образцы читаются в ожидаемые значения, а их запись — слово в слово
/// образцы. Поменялась запись — это новая версия формата (Правило 5), а не
/// правка образца.
#[test]
fn data_shape_is_pinned() {
    let invoice: Document = serde_json::from_str(INVOICE).expect("счёт читается");
    let data = invoice.data();
    assert_eq!(data.kind, DocKind::Invoice);
    assert_eq!(data.lang, Lang::En);
    assert_eq!(data.currency, Currency::Usd);
    assert_eq!(
        (data.date, data.service_date, data.due),
        (day(2026, 10, 1), day(2026, 9, 30), Some(day(2026, 10, 15)))
    );
    assert_eq!(data.items.len(), 2);
    assert_eq!(invoice.number().map(|n| n.to_string()).as_deref(), Some("2026-007"));
    let issued = invoice.issued().expect("выпущен");
    assert_eq!(issued.seller().name.get(Lang::En).as_str(), "IE Aram Petrosyan");
    assert_eq!(issued.seller().accounts.len(), 1);
    assert_eq!(issued.buyer().name.as_str(), "Beispiel GmbH");
    assert_eq!(issued.buyer().country, Country::from_code("DE"));
    assert_eq!(invoice.total(), Ok(Money::new(550_000, Currency::Usd)));
    assert_eq!(invoice.paid(), Ok(Money::new(550_000, Currency::Usd)));
    // 5 500,00 $ × 388,00 = 2 134 000 ֏.
    assert_eq!(invoice.data().total_amd(), Ok(Money::new(2_134_000, Currency::Amd)));
    assert_eq!(invoice.status(day(2026, 10, 30)), Status::Paid);
    assert_eq!(serde_json::to_string(&invoice).ok().as_deref(), Some(INVOICE));

    let act: Document = serde_json::from_str(ACT).expect("акт читается");
    assert_eq!(act.data().kind, DocKind::Act);
    assert_eq!(act.data().basis, Some(invoice.id()));
    assert_eq!(act.number(), None);
    assert_eq!(act.total(), Ok(Money::new(1_500_000, Currency::Amd)));
    assert_eq!(act.status(day(2026, 10, 13)), Status::Draft);
    assert_eq!(act.data().rate, None);
    assert_eq!(act.data().total_amd(), Ok(Money::new(1_500_000, Currency::Amd)));
    assert_eq!(serde_json::to_string(&act).ok().as_deref(), Some(ACT));
}

/// Документ версии формата 5 — без поля курса — читается с неизвестным
/// курсом, а пишется уже с полем (Правило 2): данные прошлых версий не
/// теряются.
#[test]
fn documents_without_rate_still_read() {
    let old = INVOICE.replacen(
        r#","rate":{"currency":"USD","per_unit_e4":3880000,"date":"2026-09-30","source":"manual"}"#,
        "",
        1,
    );
    assert_ne!(old, INVOICE);
    let invoice: Document = serde_json::from_str(&old).expect("счёт без курса читается");
    assert_eq!(invoice.data().rate, None);
    assert_eq!(invoice.data().total_amd(), Err(MoneyError::RateUnknown));
    let written = serde_json::to_string(&invoice).expect("записывается");
    assert!(written.contains(r#""recurring":null,"rate":null}"#), "{written}");
}

/// Испорченное не читается: незнакомое поле, запретное значение, состояние,
/// которого команды не пишут.
#[test]
fn data_rejects_what_commands_never_write() {
    let invoice = |from: &str, to: &str| INVOICE.replacen(from, to, 1);
    let act = |from: &str, to: &str| ACT.replacen(from, to, 1);
    let cases = [
        invoice(r#""recurring":null,"#, r#""recurring":null,"note":"x","#),
        invoice(r#""payments":["#, r#""extra":1,"payments":["#),
        invoice(r#""kind":"invoice""#, r#""kind":"receipt""#),
        invoice(r#""quantity_e3":1000"#, r#""quantity_e3":0"#),
        invoice(r#""quantity_e3":12500"#, r#""quantity_e3":-12500"#),
        invoice(r#""price":4000"#, r#""price":-4000"#),
        invoice(r#""price":4000"#, r#""price":40.5"#),
        invoice(r#""text":"Code review""#, r#""text":"""#),
        act(r#""items":[{"text":"Ծրագրային ապահովման մշակում","quantity_e3":1000,"price":1500000}]"#, r#""items":[]"#),
        invoice(r#""due":"2026-10-15""#, r#""due":"2026-09-30""#),
        act(r#""due":null"#, r#""due":"2026-10-20""#),
        invoice(r#""basis":null"#, r#""basis":"7c9e6679-7425-40de-944b-e07fc1f90ae8""#),
        act(r#""recurring":null"#, r#""recurring":"7c9e6679-7425-40de-944b-e07fc1f90ae7""#),
        invoice(r#""lang":"en""#, r#""lang":"am""#),
        invoice(r#""currency":"USD""#, r#""currency":"RUB""#),
        invoice(r#""number":{"year":2026,"seq":7}"#, r#""number":{"year":2025,"seq":7}"#),
        invoice(r#""number":{"year":2026,"seq":7}"#, r#""number":{"year":2026,"seq":0}"#),
        invoice(r#""en":"IE Aram Petrosyan""#, r#""en":"""#),
        invoice(r#""deleted":null"#, r#""deleted":1760000000000"#),
        invoice(r#""amount":200000"#, r#""amount":0"#),
        invoice(r#""date":"2026-10-20","amount""#, r#""date":"2026-10-05","amount""#),
        invoice(r#""name":"Beispiel GmbH""#, r#""name":"""#),
        act(r#""annulled":null"#, r#""annulled":"2026-10-13""#),
        act(r#""payments":[]"#, r#""payments":[{"date":"2026-10-13","amount":1}]"#),
        act(r#","payments":[]"#, ""),
        act(r#""rate":null"#, r#""rate":{"currency":"USD","per_unit_e4":3880000,"date":"2026-10-13","source":"manual"}"#),
        invoice(r#""currency":"USD","per_unit_e4""#, r#""currency":"EUR","per_unit_e4""#),
        invoice(r#""per_unit_e4":3880000"#, r#""per_unit_e4":0"#),
    ];
    let mut wrong = Vec::new();
    for (index, text) in cases.iter().enumerate() {
        if text == INVOICE || text == ACT {
            wrong.push(format!("случай {index} не изменил образец"));
        } else if serde_json::from_str::<Document>(text).is_ok() {
            wrong.push(format!("случай {index} прочитан"));
        }
    }
    // Выпущенный акт с оплатой — оплата только у счёта.
    let paid_act = INVOICE.replacen(r#""kind":"invoice""#, r#""kind":"act""#, 1).replacen(
        r#""due":"2026-10-15""#,
        r#""due":null"#,
        1,
    );
    if serde_json::from_str::<Document>(&paid_act).is_ok() {
        wrong.push("акт с оплатами прочитан".to_owned());
    }
    assert!(wrong.is_empty(), "прочитано испорченное: {wrong:#?}");
}

/// Сумма строки — цена на количество, до младшей единицы, половина — от
/// нуля. Числа посчитаны человеком.
#[test]
fn item_totals_round_half_away_from_zero() {
    let item = |quantity_e3, price| Item {
        text: line("Услуга"),
        quantity_e3,
        price,
    };
    let cases = [
        // 37,5 часа по 50,00 $ = 1 875,00 $.
        (item(37_500, 5_000), Currency::Usd, Ok(187_500)),
        // 0,333 часа по 100,00 $ = 33,30 $.
        (item(333, 10_000), Currency::Usd, Ok(3_330)),
        // 0,5 по 0,01 $ = 0,005 $ → 0,01 $: половина — от нуля.
        (item(500, 1), Currency::Usd, Ok(1)),
        // 0,4 по 0,01 $ = 0,004 $ → 0,00 $.
        (item(400, 1), Currency::Usd, Ok(0)),
        // 2,5 месяца по 15 000 ֏ = 37 500 ֏.
        (item(2_500, 15_000), Currency::Amd, Ok(37_500)),
        // Не помещается — ошибка, а не перенос через ноль.
        (item(2_000, i64::MAX), Currency::Amd, Err(MoneyError::Overflow)),
    ];
    let mut wrong = Vec::new();
    for (item, currency, expected) in cases {
        let got = item.total(currency).map(Money::minor);
        if got != expected {
            wrong.push(format!("{item:?}: {got:?}, а не {expected:?}"));
        }
    }
    assert!(wrong.is_empty(), "сумма строки: {wrong:#?}");
}

/// Клиент для проб: армянская фирма.
fn client() -> Client {
    let data = ClientData {
        name: line("«Արարատ» ՍՊԸ"),
        country: Some(Country::ARMENIA),
        address: Lines::default(),
        tin: Tin::parse("07654321").ok(),
        tax_id: Line::default(),
        currency: Some(Currency::Amd),
        lang: Some(Lang::Am),
        payment_days: Some(10),
    };
    Client::create(data, at(1_000)).expect("годный клиент")
}

/// Профиль для проб: имя заполнено по-армянски и по-русски, но не
/// по-английски.
fn profile() -> ProfileData {
    let mut data = ProfileData::default();
    data.name.hy = line("ԱՁ Արամ Պետրոսյան");
    data.name.ru = line("ИП Арам Петросян");
    data.tin = Tin::parse("00054321").ok();
    data
}

/// Данные черновика: одна строка в 100 000 ֏.
fn data(kind: DocKind, client: &Client, date: Date) -> DocumentData {
    DocumentData {
        kind,
        client: client.id(),
        lang: Lang::Am,
        currency: Currency::Amd,
        date,
        service_date: date,
        due: None,
        items: vec![Item {
            text: line("Խորհրդատվություն"),
            quantity_e3: 1_000,
            price: 100_000,
        }],
        basis: None,
        recurring: None,
        rate: None,
    }
}

/// Выпустить черновик среди `documents`.
fn issued(kind: DocKind, client: &Client, date: Date, documents: &[Document]) -> Document {
    Document::create(data(kind, client, date), at(2_000))
        .expect("годный черновик")
        .issue(documents, 0, Some(&profile()), client, at(3_000))
        .expect("выпускается")
}

/// Номера растут в своей серии — вид и год, — и номер аннулированного
/// второй раз не выдаётся; черновики номеров не занимают.
#[test]
fn numbers_grow_within_their_series() {
    let client = client();
    let mut documents = Vec::new();
    for _ in 0..2 {
        let next = issued(DocKind::Invoice, &client, day(2026, 10, 1), &documents);
        documents.push(next);
    }
    let numbers: Vec<String> = documents.iter().filter_map(Document::number).map(|n| n.to_string()).collect();
    assert_eq!(numbers, ["2026-001", "2026-002"]);

    // Второй аннулирован — третий всё равно 003.
    let annulled = documents[1].annul(day(2026, 10, 2), at(4_000)).expect("выпущен").expect("аннулирован");
    documents[1] = annulled;
    documents.push(Document::create(data(DocKind::Invoice, &client, day(2026, 10, 3)), at(5_000)).expect("черновик"));
    assert_eq!(
        Document::next_number(&documents, DocKind::Invoice, 2026).map(|n| n.to_string()).as_deref(),
        Some("2026-003")
    );
    // Акты — своя серия, следующий год — своя.
    assert_eq!(
        Document::next_number(&documents, DocKind::Act, 2026).map(|n| n.to_string()).as_deref(),
        Some("2026-001")
    );
    let next_year = issued(DocKind::Invoice, &client, day(2027, 1, 2), &documents);
    assert_eq!(next_year.number().map(|n| n.to_string()).as_deref(), Some("2027-001"));
    assert_eq!(Document::next_number(&[], DocKind::Invoice, 2026).map(Number::seq), Some(1));
}

/// Номер — год и номер не короче трёх цифр.
#[test]
fn number_is_written_with_its_year() {
    let cases = [(2026, 1, "2026-001"), (2026, 42, "2026-042"), (2027, 1_234, "2027-1234")];
    for (year, seq, expected) in cases {
        assert_eq!(Number { year, seq }.to_string(), expected);
    }
}

/// Выпуск копирует реквизиты ИП и клиента на этот день, и правка клиента
/// после выпуска документа не меняет.
#[test]
fn issue_copies_both_sides() {
    let client = client();
    let draft = Document::create(data(DocKind::Invoice, &client, day(2026, 10, 1)), at(2_000))
        .expect("черновик");
    let document = draft
        .issue(&[], 0, Some(&profile()), &client, at(3_000))
        .expect("выпускается");
    assert_eq!(document.id(), draft.id());
    assert_eq!(document.meta().modified(), at(3_000));
    let issued = document.issued().expect("выпущен");
    assert_eq!(issued.seller().name.get(Lang::Am).as_str(), "ԱՁ Արամ Պետրոսյան");
    assert_eq!(issued.seller().tin, Tin::parse("00054321").ok());
    assert_eq!(issued.buyer(), client.data());

    let mut renamed = client.data().clone();
    renamed.name = line("«Արարատ» ԲԲԸ");
    let client = client.edit(renamed, at(4_000)).expect("годно").expect("правка");
    assert_ne!(document.issued().map(Issued::buyer), Some(client.data()));
}

/// Выпуск отказывает словами: уже выпущен, черновик удалён, клиент не тот
/// или удалён, профиля нет или нет имени на языке документа, записи
/// документов не читаются.
#[test]
fn issue_refuses_what_it_cannot_do_safely() {
    let client = client();
    let draft = Document::create(data(DocKind::Invoice, &client, day(2026, 10, 1)), at(2_000))
        .expect("черновик");
    let profile = profile();
    let ok = |document: &Document| document.issue(&[], 0, Some(&profile), &client, at(3_000));
    let issued = ok(&draft).expect("выпускается");
    assert_eq!(ok(&issued), Err(DocError::Issued));
    let deleted = draft.delete(at(2_500)).expect("черновик").expect("удалён");
    assert_eq!(ok(&deleted), Err(DocError::Deleted));

    let other = Client::create(client.data().clone(), at(1_000)).expect("годный");
    assert_eq!(draft.issue(&[], 0, Some(&profile), &other, at(3_000)), Err(DocError::WrongClient));
    let gone = client.delete(at(1_500)).expect("удаляется");
    assert_eq!(draft.issue(&[], 0, Some(&profile), &gone, at(3_000)), Err(DocError::ClientDeleted));

    assert_eq!(draft.issue(&[], 0, None, &client, at(3_000)), Err(DocError::NoSellerName));
    let mut english = data(DocKind::Invoice, &client, day(2026, 10, 1));
    english.lang = Lang::En;
    let english = Document::create(english, at(2_000)).expect("черновик");
    assert_eq!(
        english.issue(&[], 0, Some(&profile), &client, at(3_000)),
        Err(DocError::NoSellerName),
        "имени по-английски в профиле нет"
    );
    assert_eq!(
        draft.issue(&[], 1, Some(&profile), &client, at(3_000)),
        Err(DocError::NumbersUnknown)
    );
}

/// Выпущенный документ не правится и не удаляется, а аннулируется;
/// аннулирование возвращается — сразу после ошибки.
#[test]
fn issued_documents_are_annulled_not_edited() {
    let client = client();
    let document = issued(DocKind::Invoice, &client, day(2026, 10, 1), &[]);
    let today = day(2026, 10, 5);
    assert_eq!(document.status(today), Status::Unpaid);
    assert_eq!(
        document.edit(data(DocKind::Invoice, &client, day(2026, 10, 2)), at(4_000)),
        Err(DocError::Issued)
    );
    assert_eq!(document.delete(at(4_000)), Err(DocError::Issued));

    let annulled = document.annul(today, at(4_000)).expect("выпущен").expect("аннулирован");
    assert_eq!(annulled.annulled(), Some(today));
    assert_eq!(annulled.status(today), Status::Annulled);
    assert_eq!(annulled.number(), document.number(), "номер остаётся");
    assert_eq!(annulled.annul(today, at(5_000)), Ok(None));
    let back = annulled.unannul(at(6_000)).expect("возвращён");
    assert_eq!(back.status(today), Status::Unpaid);
    assert_eq!(back.unannul(at(7_000)), None);

    let draft = Document::create(data(DocKind::Invoice, &client, today), at(2_000)).expect("черновик");
    assert_eq!(draft.annul(today, at(3_000)), Err(DocError::NotIssued));
}

/// Черновик правится, удаляется меткой и восстанавливается.
#[test]
fn drafts_are_edited_deleted_and_restored() {
    let client = client();
    let draft = Document::create(data(DocKind::Act, &client, day(2026, 10, 1)), at(2_000))
        .expect("черновик");
    assert_eq!(draft.edit(draft.data().clone(), at(3_000)), Ok(None));
    let mut changed = draft.data().clone();
    changed.items[0].quantity_e3 = 2_000;
    let edited = draft.edit(changed.clone(), at(3_000)).expect("годно").expect("правка");
    assert_eq!(edited.data(), &changed);
    assert_eq!(edited.meta().modified(), at(3_000));
    changed.items.clear();
    assert_eq!(edited.edit(changed, at(4_000)), Err(DocError::NoItems));

    let deleted = edited.delete(at(5_000)).expect("черновик").expect("удалён");
    assert!(deleted.is_deleted());
    assert_eq!(deleted.delete(at(6_000)), Ok(None));
    let restored = deleted.restore(at(7_000)).expect("восстановлен");
    assert!(!restored.is_deleted());
    assert_eq!(restored.restore(at(8_000)), None);
}

/// Оплаты: частичная, полная, просрочка, ошибочная — убирается; оплата —
/// только у выпущенного действующего счёта и больше нуля. Счёт —
/// 100 000 ֏, срок оплаты — 11 октября.
#[test]
fn payments_move_the_status() {
    let client = client();
    let mut with_due = data(DocKind::Invoice, &client, day(2026, 10, 1));
    with_due.due = Some(day(2026, 10, 11));
    let invoice = Document::create(with_due, at(2_000))
        .expect("черновик")
        .issue(&[], 0, Some(&profile()), &client, at(3_000))
        .expect("выпущен");
    let (before, after) = (day(2026, 10, 11), day(2026, 10, 12));
    assert_eq!(invoice.status(before), Status::Unpaid);
    assert_eq!(invoice.status(after), Status::Overdue);

    let part = Payment { date: day(2026, 10, 5), amount: 40_000 };
    let partly = invoice.add_payment(part, at(4_000)).expect("отмечена");
    assert_eq!(partly.status(before), Status::PartlyPaid);
    assert_eq!(partly.status(after), Status::Overdue);
    // Оплата раньше по дате встаёт раньше.
    let early = Payment { date: day(2026, 10, 2), amount: 60_000 };
    let paid = partly.add_payment(early, at(5_000)).expect("отмечена");
    assert_eq!(paid.payments(), [early, part]);
    assert_eq!(paid.paid(), Ok(Money::new(100_000, Currency::Amd)));
    assert_eq!(paid.status(after), Status::Paid);
    let back = paid.remove_payment(early, at(6_000)).expect("убрана");
    assert_eq!(back.payments(), [part]);
    assert_eq!(back.remove_payment(early, at(7_000)), None);

    let zero = Payment { date: day(2026, 10, 5), amount: 0 };
    assert_eq!(invoice.add_payment(zero, at(4_000)), Err(DocError::PaymentAmount));
    let annulled = invoice.annul(before, at(4_000)).expect("выпущен").expect("аннулирован");
    assert_eq!(annulled.add_payment(part, at(5_000)), Err(DocError::Annulled));
    let draft = Document::create(data(DocKind::Invoice, &client, before), at(2_000)).expect("черновик");
    assert_eq!(draft.add_payment(part, at(3_000)), Err(DocError::NotIssued));
    let act = issued(DocKind::Act, &client, before, &[]);
    assert_eq!(act.add_payment(part, at(4_000)), Err(DocError::NotInvoice));
    assert_eq!(act.status(after), Status::Issued);
}

/// Акт из счёта: тот же клиент, язык, валюта, строки и дата услуги, дата
/// акта — сегодня, основание — счёт. Из черновика и из аннулированного —
/// нельзя.
#[test]
fn act_from_an_invoice() {
    let client = client();
    let invoice = issued(DocKind::Invoice, &client, day(2026, 10, 1), &[]);
    let today = day(2026, 10, 13);
    let act = invoice.act_data(today).expect("акт");
    assert_eq!(act.kind, DocKind::Act);
    assert_eq!(act.date, today);
    assert_eq!(act.service_date, invoice.data().service_date);
    assert_eq!(act.basis, Some(invoice.id()));
    assert_eq!((act.client, act.lang, act.currency), (client.id(), Lang::Am, Currency::Amd));
    assert_eq!(act.items, invoice.data().items);
    assert!(act.check().is_ok());

    let draft = Document::create(data(DocKind::Invoice, &client, today), at(2_000)).expect("черновик");
    assert_eq!(draft.act_data(today), Err(DocError::NotIssued));
    let annulled = invoice.annul(today, at(4_000)).expect("выпущен").expect("аннулирован");
    assert_eq!(annulled.act_data(today), Err(DocError::Annulled));
}

/// Счёт в долларах для проб курса: 1 000,00 $, услуга оказана 30 сентября.
fn dollar_invoice(client: &Client) -> Document {
    let mut dollars = data(DocKind::Invoice, client, day(2026, 10, 1));
    dollars.currency = Currency::Usd;
    dollars.service_date = day(2026, 9, 30);
    Document::create(dollars, at(2_000))
        .expect("черновик")
        .issue(&[], 0, Some(&profile()), client, at(3_000))
        .expect("выпускается")
}

/// Курс выпущенного документа указывают и правят: он валюты документа и на
/// день оказания услуги, а текст документа не меняется. Тот же курс — нечего
/// писать; у аннулированного, у черновика и у документа в драмах — нельзя.
#[test]
fn rate_is_set_on_an_issued_document() {
    let client = client();
    let invoice = dollar_invoice(&client);
    assert_eq!(invoice.data().total_amd(), Err(MoneyError::RateUnknown));
    let rated = invoice.set_rate(Some(3_880_000), at(4_000)).expect("годен").expect("указан");
    let rate = rated.data().rate.expect("курс есть");
    assert_eq!(
        (rate.currency(), rate.per_unit_e4(), rate.date(), rate.source()),
        (Currency::Usd, 3_880_000, day(2026, 9, 30), RateSource::Manual)
    );
    // Строка — 100 000 центов: 1 000,00 $ × 388,00 = 388 000 ֏.
    assert_eq!(rated.data().total_amd(), Ok(Money::new(388_000, Currency::Amd)));
    assert_eq!(rated.meta().modified(), at(4_000));
    assert_eq!((rated.number(), rated.issued()), (invoice.number(), invoice.issued()));
    assert_eq!(rated.data().items, invoice.data().items);
    assert_eq!(rated.set_rate(Some(3_880_000), at(5_000)), Ok(None));
    let cleared = rated.set_rate(None, at(5_000)).expect("годен").expect("убран");
    assert_eq!(cleared.data().rate, None);
    assert_eq!(invoice.set_rate(Some(0), at(4_000)), Err(DocError::Rate));

    let annulled = invoice.annul(day(2026, 10, 2), at(4_000)).expect("выпущен").expect("аннулирован");
    assert_eq!(annulled.set_rate(Some(3_880_000), at(5_000)), Err(DocError::Annulled));
    let draft = Document::create(data(DocKind::Invoice, &client, day(2026, 10, 1)), at(2_000)).expect("черновик");
    assert_eq!(draft.set_rate(Some(3_880_000), at(5_000)), Err(DocError::NotIssued));
    let drams = issued(DocKind::Invoice, &client, day(2026, 10, 1), &[]);
    assert_eq!(drams.set_rate(Some(3_880_000), at(5_000)), Err(DocError::Rate));
}

/// Акт по счёту берёт курс счёта — день услуги тот же; повтор счёта — нет:
/// у следующего месяца свой курс.
#[test]
fn rate_follows_the_act_but_not_the_repeat() {
    let client = client();
    let rated = dollar_invoice(&client)
        .set_rate(Some(3_880_000), at(4_000))
        .expect("годен")
        .expect("указан");
    let act = rated.act_data(day(2026, 10, 13)).expect("акт");
    assert_eq!(act.rate, rated.data().rate);
    let next = rated.repeat_data(1, RecordId::random()).expect("календарь не кончился");
    assert_eq!(next.service_date, day(2026, 10, 31));
    assert_eq!(next.rate, None);
}

/// Список: черновики первыми, потом выпущенные — свежие сверху; удалённые —
/// отдельно, последний удалённый первым. И сколько документов у клиента.
#[test]
fn lists_and_counts() {
    let client = client();
    let other = Client::create(client.data().clone(), at(1_000)).expect("годный");
    let older = issued(DocKind::Invoice, &client, day(2026, 9, 1), &[]);
    let newer = issued(DocKind::Invoice, &client, day(2026, 10, 1), std::slice::from_ref(&older));
    let draft = Document::create(data(DocKind::Act, &other, day(2026, 8, 1)), at(2_000)).expect("черновик");
    let gone = Document::create(data(DocKind::Act, &client, day(2026, 8, 2)), at(2_000))
        .expect("черновик")
        .delete(at(3_000))
        .expect("черновик")
        .expect("удалён");
    let documents = vec![older.clone(), gone.clone(), newer.clone(), draft.clone()];
    let order: Vec<RecordId> = Document::listed(&documents).iter().map(|d| d.id()).collect();
    assert_eq!(order, [draft.id(), newer.id(), older.id()]);
    let deleted: Vec<RecordId> = Document::deleted(&documents).iter().map(|d| d.id()).collect();
    assert_eq!(deleted, [gone.id()]);
    assert_eq!(Document::count_for_client(&documents, client.id()), 2, "удалённый не в счёт");
    assert_eq!(Document::count_for_client(&documents, other.id()), 1);
}

/// По дате: свежие сверху, в один день черновик выше выпущенных,
/// выпущенные — больший номер выше; удалённых нет. Поиск — по части
/// номера и названия клиента, без учёта регистра и «ё»; пустой — всё.
#[test]
fn by_date_and_search() {
    let client = client();
    let first = issued(DocKind::Invoice, &client, day(2026, 10, 9), &[]);
    let second = issued(DocKind::Invoice, &client, day(2026, 10, 9), std::slice::from_ref(&first));
    let september = issued(DocKind::Act, &client, day(2026, 9, 30), &[]);
    let draft = Document::create(data(DocKind::Invoice, &client, day(2026, 10, 9)), at(2_000)).expect("черновик");
    let newest = Document::create(data(DocKind::Act, &client, day(2026, 10, 12)), at(2_000)).expect("черновик");
    let gone = Document::create(data(DocKind::Act, &client, day(2026, 10, 20)), at(2_000))
        .expect("черновик")
        .delete(at(3_000))
        .expect("черновик")
        .expect("удалён");
    let documents = vec![september.clone(), first.clone(), gone, draft.clone(), second.clone(), newest.clone()];
    let order: Vec<RecordId> = Document::by_date(&documents).iter().map(|d| d.id()).collect();
    assert_eq!(order, [newest.id(), draft.id(), second.id(), first.id(), september.id()]);

    let number = format!("{}", second.number().expect("выпущен"));
    let mut wrong = Vec::new();
    for (query, name, document, expected) in [
        ("", "Семён", &draft, true),
        ("семен", "ООО Семён", &draft, true),
        ("СЕМЁН", "ООО Семён", &draft, true),
        ("acme", "ООО Семён", &draft, false),
        (number.as_str(), "", &second, true),
        ("-002", "", &second, true),
        ("-002", "", &first, false),
        ("2026", "", &draft, false),
        ("  ararat ", "«Ararat» LLC", &first, true),
    ] {
        if document.matches(query, name) != expected {
            wrong.push(format!("«{query}» в «{name}» {:?}: не {expected}", document.number()));
        }
    }
    assert!(wrong.is_empty(), "поиск: {wrong:#?}");
}

/// Ждут оплаты только выпущенные действующие счета, оплаченные не целиком:
/// просроченный — первым, потом по сроку, без срока — последним. Черновик,
/// акт, аннулированный, оплаченный и удалённый — не ждут. Сколько осталось
/// оплатить — итог минус поступления. Сегодня — 20 октября.
#[test]
fn invoices_awaiting_payment() {
    let client = client();
    let today = day(2026, 10, 20);
    let with_due = |date: Date, due: Option<Date>, documents: &[Document]| {
        let mut data = data(DocKind::Invoice, &client, date);
        data.due = due;
        Document::create(data, at(2_000))
            .expect("черновик")
            .issue(documents, 0, Some(&profile()), &client, at(3_000))
            .expect("выпущен")
    };
    let late = with_due(day(2026, 9, 1), Some(day(2026, 10, 1)), &[]);
    let soon = with_due(day(2026, 10, 10), Some(day(2026, 10, 25)), std::slice::from_ref(&late));
    let later = with_due(day(2026, 10, 5), Some(day(2026, 11, 5)), &[late.clone(), soon.clone()]);
    let open = with_due(day(2026, 10, 2), None, &[late.clone(), soon.clone(), later.clone()]);
    let part = Payment { date: day(2026, 10, 12), amount: 30_000 };
    let partly = soon.add_payment(part, at(4_000)).expect("отмечена");
    let full = Payment { date: day(2026, 10, 12), amount: 100_000 };
    let paid = with_due(day(2026, 10, 3), None, &[late.clone(), soon.clone(), later.clone(), open.clone()])
        .add_payment(full, at(4_000))
        .expect("отмечена");
    let annulled = with_due(day(2026, 10, 4), None, &[late.clone(), soon.clone(), later.clone(), open.clone(), paid.clone()])
        .annul(today, at(4_000))
        .expect("выпущен")
        .expect("аннулирован");
    let act = issued(DocKind::Act, &client, day(2026, 10, 6), &[]);
    let draft = Document::create(data(DocKind::Invoice, &client, today), at(2_000)).expect("черновик");
    let documents = vec![
        open.clone(),
        draft,
        later.clone(),
        act,
        paid.clone(),
        partly.clone(),
        annulled,
        late.clone(),
    ];
    let order: Vec<RecordId> = Document::awaiting(&documents, today).iter().map(|d| d.id()).collect();
    assert_eq!(order, [late.id(), partly.id(), later.id(), open.id()]);
    assert_eq!(partly.left(), Ok(Money::new(70_000, Currency::Amd)));
    assert_eq!(late.left(), Ok(Money::new(100_000, Currency::Amd)));
    assert_eq!(paid.left(), Ok(Money::new(0, Currency::Amd)));
}

/// Профиль, сохранённый командой, годится для выпуска так же, как данные
/// профиля: окно передаёт `Profile::data`.
#[test]
fn profile_record_is_enough_to_issue() {
    let client = client();
    let saved = Profile::save(None, profile(), at(1_000)).expect("годно").expect("новый");
    let draft = Document::create(data(DocKind::Invoice, &client, day(2026, 10, 1)), at(2_000))
        .expect("черновик");
    assert!(draft.issue(&[], 0, Some(saved.data()), &client, at(3_000)).is_ok());
}
