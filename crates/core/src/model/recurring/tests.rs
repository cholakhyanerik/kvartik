//! Тесты повторяющегося счёта. Даты черновиков выписаны человеком по
//! календарю, а не посчитаны той же формулой (Правило 2).

use super::*;
use crate::i18n::Lang;
use crate::model::client::{Client, ClientData};
use crate::model::document::{DocKind, DocumentData, Item};
use crate::model::money::Currency;
use crate::model::profile::ProfileData;
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

/// Повтор, у которого следующим появится второй черновик.
const SAMPLE: &str = concat!(
    r#"{"meta":{"id":"3f2a1b4c-5d6e-4f70-8a9b-0c1d2e3f4a5b","modified":1760000000000,"deleted":null},"#,
    r#""data":{"source":"7c9e6679-7425-40de-944b-e07fc1f90ae7","next":2}}"#
);

/// Образец читается и пишется слово в слово; черновик номер 0 и лишнее поле
/// — порча.
#[test]
fn data_shape_is_pinned() {
    let recurring: Recurring = serde_json::from_str(SAMPLE).expect("образец читается");
    assert_eq!(recurring.data().next, 2);
    assert_eq!(
        recurring.data().source.to_string(),
        "7c9e6679-7425-40de-944b-e07fc1f90ae7"
    );
    assert!(!recurring.is_stopped());
    assert_eq!(serde_json::to_string(&recurring).ok().as_deref(), Some(SAMPLE));
    for broken in [
        SAMPLE.replace(r#""next":2"#, r#""next":0"#),
        SAMPLE.replace(r#""next":2"#, r#""next":-1"#),
        SAMPLE.replace(r#""next":2"#, r#""next":2,"day":1"#),
        SAMPLE.replace(r#""source":"7c9e6679-7425-40de-944b-e07fc1f90ae7","#, ""),
    ] {
        assert!(serde_json::from_str::<Recurring>(&broken).is_err(), "{broken} прочитан");
    }
}

fn client() -> Client {
    let data = ClientData {
        name: line("Acme Inc."),
        country: None,
        address: Lines::default(),
        tin: None,
        tax_id: Line::default(),
        currency: Some(Currency::Usd),
        lang: Some(Lang::En),
        payment_days: Some(15),
    };
    Client::create(data, at(1_000)).expect("годный клиент")
}

fn profile() -> ProfileData {
    let mut data = ProfileData::default();
    data.name.en = line("IE Aram Petrosyan");
    data
}

/// Выпущенный счёт от `date` за услугу `service`, со сроком оплаты через
/// 15 дней.
fn invoice(client: &Client, date: Date, service: Date) -> Document {
    let data = DocumentData {
        kind: DocKind::Invoice,
        client: client.id(),
        lang: Lang::En,
        currency: Currency::Usd,
        date,
        service_date: service,
        due: date.add_days(15),
        items: vec![Item {
            text: line("Software development"),
            quantity_e3: 1_000,
            price: 500_000,
        }],
        basis: None,
        recurring: None,
        rate: None,
        activity: None,
    };
    Document::create(data, at(2_000))
        .expect("черновик")
        .issue(&[], 0, Some(&profile()), client, at(3_000))
        .expect("выпущен")
}

/// Повтор запускается только у выпущенного действующего счёта; у счёта,
/// который уже повторяется, — ничего не пишет; остановленный — запускается
/// снова, с ближайшего месяца, без пропущенных.
#[test]
fn start_only_from_a_live_invoice() {
    let client = client();
    let source = invoice(&client, day(2026, 8, 31), day(2026, 8, 31));
    let today = day(2026, 10, 13);
    let started = Recurring::start(None, &source, today, at(4_000))
        .expect("счёт годится")
        .expect("новый повтор");
    assert_eq!(started.data().source, source.id());
    assert_eq!(Recurring::start(Some(&started), &source, today, at(5_000)), Ok(None));

    let stopped = started.stop(at(5_000)).expect("остановлен");
    assert!(stopped.is_stopped());
    assert_eq!(stopped.stop(at(6_000)), None);
    let later = day(2027, 3, 1);
    let again = Recurring::start(Some(&stopped), &source, later, at(6_000))
        .expect("счёт годится")
        .expect("запущен снова");
    assert_eq!(again.id(), started.id());
    assert!(!again.is_stopped());
    // 28 февраля 2027 — последний наступивший месяц: шестой после августа.
    assert_eq!(again.data().next, 6);

    let draft = Document::create(source.data().clone(), at(2_000)).expect("черновик");
    assert_eq!(Recurring::start(None, &draft, today, at(4_000)), Err(DocError::NotIssued));
    let act_data = source.act_data(today).expect("акт");
    let act = Document::create(act_data, at(2_000))
        .expect("черновик")
        .issue(&[], 0, Some(&profile()), &client, at(3_000))
        .expect("выпущен");
    assert_eq!(Recurring::start(None, &act, today, at(4_000)), Err(DocError::NotInvoice));
    let annulled = source.annul(today, at(4_000)).expect("выпущен").expect("аннулирован");
    assert_eq!(Recurring::start(None, &annulled, today, at(5_000)), Err(DocError::Annulled));
}

/// Первый черновик повтора — за последний наступивший месяц, а если ни
/// один не наступил — через месяц после счёта.
#[test]
fn first_period_is_the_latest_due_month() {
    let cases = [
        // 1 октября наступило — черновик за него появится сразу.
        (day(2026, 9, 1), day(2026, 10, 13), 1),
        // 31 октября ещё нет — первый черновик в его день.
        (day(2026, 9, 30), day(2026, 10, 13), 1),
        // 31 октября ещё нет, а 30 сентября уже было.
        (day(2026, 8, 31), day(2026, 10, 13), 1),
        // Счёт этого месяца — первый черновик через месяц.
        (day(2026, 10, 1), day(2026, 10, 13), 1),
        // Ровно год назад — двенадцатый, сегодняшний.
        (day(2025, 10, 13), day(2026, 10, 13), 12),
        // Год без одного дня — одиннадцатый: двенадцатый будет завтра.
        (day(2025, 10, 14), day(2026, 10, 13), 11),
        // Счёт из будущего (часы переводили) — первый.
        (day(2026, 11, 1), day(2026, 10, 13), 1),
    ];
    let mut wrong = Vec::new();
    for (source, today, expected) in cases {
        let got = first_period(source, today);
        if got != expected {
            wrong.push(format!("{source:?} → {today:?}: {got}, а не {expected}"));
        }
    }
    assert!(wrong.is_empty(), "первый черновик: {wrong:#?}");
}

/// Черновики появляются, когда наступил их день: копия счёта на месяц,
/// два… позже — даты сдвинуты, последний день месяца остаётся последним,
/// срок оплаты — те же 15 дней. Номера у черновиков нет, повтор — сдвинут.
#[test]
fn drafts_appear_when_their_day_comes() {
    let client = client();
    let source = invoice(&client, day(2026, 8, 31), day(2026, 8, 31));
    let recurring = Recurring::start(None, &source, day(2026, 9, 1), at(4_000))
        .expect("годится")
        .expect("новый");
    assert_eq!(recurring.data().next, 1);
    assert_eq!(recurring.next_date(&source), Some(day(2026, 9, 30)));
    assert!(!recurring.due(&source, day(2026, 9, 29)));
    assert_eq!(recurring.generate(&source, true, day(2026, 9, 29), at(5_000)), None);
    assert!(recurring.due(&source, day(2026, 9, 30)));
    // В свой день черновик уже появляется.
    let on_the_day = recurring.generate(&source, true, day(2026, 9, 30), at(5_000));
    assert_eq!(on_the_day.map(|(_, drafts)| drafts.len()), Some(1));

    let (moved, drafts) = recurring
        .generate(&source, true, day(2026, 10, 13), at(5_000))
        .expect("пора");
    assert_eq!(drafts.len(), 1);
    let draft = &drafts[0];
    assert_eq!(
        (draft.data().date, draft.data().service_date, draft.data().due),
        (day(2026, 9, 30), day(2026, 9, 30), Some(day(2026, 10, 15)))
    );
    assert_eq!(draft.data().recurring, Some(recurring.id()));
    assert_eq!(draft.data().items, source.data().items);
    assert_eq!(draft.data().client, client.id());
    assert_eq!(draft.number(), None);
    assert_ne!(draft.id(), source.id());
    assert_eq!(moved.id(), recurring.id());
    assert_eq!(moved.data().next, 2);
    assert_eq!(moved.next_date(&source), Some(day(2026, 10, 31)));
    assert_eq!(moved.generate(&source, true, day(2026, 10, 13), at(6_000)), None);
}

/// Кто не открывал программу полгода, получит черновик за каждый месяц —
/// но не больше `MAX_CATCH_UP` за одно открытие.
#[test]
fn missed_months_catch_up_within_a_limit() {
    let client = client();
    let source = invoice(&client, day(2026, 8, 31), day(2026, 8, 31));
    let recurring = Recurring::start(None, &source, day(2026, 9, 1), at(4_000))
        .expect("годится")
        .expect("новый");
    let (moved, drafts) = recurring
        .generate(&source, true, day(2027, 3, 1), at(5_000))
        .expect("пора");
    let dates: Vec<Date> = drafts.iter().map(|draft| draft.data().date).collect();
    assert_eq!(
        dates,
        [
            day(2026, 9, 30),
            day(2026, 10, 31),
            day(2026, 11, 30),
            day(2026, 12, 31),
            day(2027, 1, 31),
            day(2027, 2, 28),
        ]
    );
    assert_eq!(moved.data().next, 7);

    let (moved, drafts) = recurring
        .generate(&source, true, day(2030, 1, 1), at(5_000))
        .expect("пора");
    assert_eq!(drafts.len(), MAX_CATCH_UP);
    assert_eq!(moved.data().next, 13);
}

/// Остановленный повтор, удалённый клиент и аннулированный счёт черновиков
/// не дают.
#[test]
fn no_drafts_when_there_is_nobody_to_bill() {
    let client = client();
    let source = invoice(&client, day(2026, 8, 31), day(2026, 8, 31));
    let recurring = Recurring::start(None, &source, day(2026, 9, 1), at(4_000))
        .expect("годится")
        .expect("новый");
    let today = day(2026, 10, 13);
    assert!(recurring.generate(&source, true, today, at(5_000)).is_some());
    assert_eq!(recurring.generate(&source, false, today, at(5_000)), None);
    let stopped = recurring.stop(at(5_000)).expect("остановлен");
    assert_eq!(stopped.generate(&source, true, today, at(6_000)), None);
    assert!(!stopped.due(&source, today));
    let annulled = source.annul(today, at(5_000)).expect("выпущен").expect("аннулирован");
    assert_eq!(recurring.generate(&annulled, true, today, at(6_000)), None);
}

/// Повтор счёта — живой, если такой есть.
#[test]
fn the_live_recurring_wins() {
    let client = client();
    let source = invoice(&client, day(2026, 8, 31), day(2026, 8, 31));
    let other = invoice(&client, day(2026, 8, 1), day(2026, 8, 1));
    let today = day(2026, 10, 13);
    let live = Recurring::start(None, &source, today, at(4_000))
        .expect("годится")
        .expect("новый");
    let stopped = Recurring::start(None, &source, today, at(3_000))
        .expect("годится")
        .expect("новый")
        .stop(at(9_000))
        .expect("остановлен");
    let recurrings = vec![stopped.clone(), live.clone()];
    assert_eq!(Recurring::of(&recurrings, source.id()).map(Recurring::id), Some(live.id()));
    assert_eq!(Recurring::of(&recurrings, other.id()), None);
    let only_stopped = vec![stopped.clone()];
    assert_eq!(
        Recurring::of(&only_stopped, source.id()).map(Recurring::id),
        Some(stopped.id())
    );
}
