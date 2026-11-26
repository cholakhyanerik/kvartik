//! Тесты сводки. Свои числа сводка не считает — она собирает то, что
//! считают карточки одного ИП, — поэтому строку тесты сверяют с теми же
//! функциями, которые зовёт главный экран (`turnover::quarter`,
//! `deadlines::overdue` и `between`, `annual::year`): разойтись с ними —
//! и есть дефект сводки. Дни сроков — из тестов сроков, где они посчитаны
//! руками по закону и календарю (`deadlines/tests.rs`).

use super::*;
use crate::model::activity::ActivityCode;
use crate::model::document::Document;
use crate::model::ledger::{Flow, Operation, OperationData};
use crate::model::money::{Currency, Money};
use crate::model::profile::{Person, ProfileData, Residency};
use crate::tax::annual::Status;
use crate::model::record::Timestamp;
use crate::model::text::Line;
use crate::tax::deadlines::Kind;
use crate::tax::opening::Opening;

fn day(year: u16, month: u8, day: u8) -> Date {
    Date::new(year, month, day).expect("в тесте — существующая дата")
}

fn quarter(year: u16, number: u8) -> Quarter {
    Quarter::new(year, number).expect("в тесте — существующий квартал")
}

/// Гражданин, зарегистрированный ИП 12 мая 2022 года, с кодом услуг —
/// у дохода без кода налог был бы посчитан не весь.
fn citizen() -> ProfileData {
    ProfileData {
        person: Person {
            birth_year: Some(1990),
            pension_joined: None,
            pension_applied: None,
            resident: None,
            residency: Some(Residency::Citizen),
            registered: Some(day(2022, 5, 12)),
            turnover_applied: None,
            stops: Vec::new(),
        },
        activities: vec![ActivityCode::parse("74.10.0").expect("код")],
        ..ProfileData::default()
    }
}

/// Доход в драмах в день `date`.
fn income(date: Date, drams: i64) -> Operation {
    Operation::create(
        OperationData {
            flow: Flow::Income,
            date,
            paid: None,
            amount: Money::new(drams, Currency::Amd),
            rate: None,
            text: Line::new("Услуга").expect("текст"),
            client: None,
            supplier: Line::default(),
            proof: None,
            nature: None,
            cost: None,
            activity: None,
            income_kind: None,
            attachments: Vec::new(),
        },
        Timestamp::from_unix_millis(1_760_000_000_000),
    )
    .expect("в тесте — годная запись")
}

fn books<'a>(profile: &'a ProfileData, operations: &'a [Operation], closings: &'a [Closing]) -> Books<'a> {
    let documents: &[Document] = &[];
    let openings: &[Opening] = &[];
    Books {
        documents,
        operations,
        profile: Some(profile),
        closings,
        openings,
        unreadable: 0,
    }
}

/// Прошлый квартал не закрыт: к уплате — тот же налог, что считает карточка
/// налога (`turnover::quarter`), полный, со сроком 20 октября; пока срок не
/// прошёл, он и есть ближайший — через шесть дней, — а прошёл — ближайший он
/// же, просроченным. Годовые — года ближайшего срока, как у карточки годовых
/// платежей: до 20 апреля 2027 года — 2026-й.
#[test]
fn an_open_quarter_is_to_pay() {
    let profile = citizen();
    let operations = [income(day(2026, 8, 5), 1_000_000)];
    let open = books(&profile, &operations, &[]);
    let expected = turnover::quarter(open, quarter(2026, 3));
    assert!(expected.complete() && expected.tax > 0, "в примере налог посчитан не весь: {expected:?}");
    let mut wrong = Vec::new();
    for (today, when) in [(day(2026, 10, 14), When::In(6)), (day(2026, 10, 25), When::Overdue)] {
        let row = row(open, today);
        match row.previous {
            Some(Previous::ToPay { quarter: q, tax, complete: true, due })
                if q == quarter(2026, 3) && tax == expected.tax && due.day == day(2026, 10, 20) && !due.done => {}
            other => wrong.push(format!("{today:?}: прошлый квартал {other:?}")),
        }
        match row.next {
            Some(Next { deadline, when: got, .. })
                if deadline.kind == Kind::Turnover(quarter(2026, 3)) && got == when => {}
            other => wrong.push(format!("{today:?}: ближайший срок {other:?}")),
        }
        let total = Total::of(&annual::year(open, 2026));
        if row.annual != Some(Yearly { year: 2026, total }) {
            wrong.push(format!("{today:?}: годовые {:?}, а не 2026 год {total:?}", row.annual));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// Закрытый прошлый квартал — «расчёт подан» с налогом снимка, а не книги:
/// запись, добавленная после закрытия, его не меняет. Ближайший срок —
/// следующий несделанный: декларация гражданина за 2025 год, 2 ноября
/// (1-е — воскресенье), через восемь дней.
#[test]
fn a_closed_quarter_is_filed() {
    let profile = citizen();
    let operations = [income(day(2026, 8, 5), 1_000_000)];
    let before = turnover::quarter(books(&profile, &operations, &[]), quarter(2026, 3));
    let closing = Closing::close(&before, day(2026, 10, 15), Timestamp::from_unix_millis(1_760_000_000_000))
        .expect("квартал закрывается");
    let later = [income(day(2026, 8, 5), 1_000_000), income(day(2026, 9, 1), 500_000)];
    let row = row(books(&profile, &later, std::slice::from_ref(&closing)), day(2026, 10, 25));
    let mut wrong = Vec::new();
    if row.previous != Some(Previous::Filed { quarter: quarter(2026, 3), tax: before.tax }) {
        wrong.push(format!("прошлый квартал {:?}, а не поданный с налогом {}", row.previous, before.tax));
    }
    match row.next {
        Some(Next { deadline, when: When::In(8), .. })
            if deadline.kind == Kind::Declaration(2025) && deadline.day == day(2026, 11, 2) => {}
        other => wrong.push(format!("ближайший срок {other:?}")),
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// Взнос медстраховки за январь 2027 года — в сводке тем же расчётом, что
/// напоминание главного экрана (`deadlines::remind_day`): 15.01.2027, IV
/// квартал 2026 года закрыт — ближайший срок этот взнос (20.02.2027), а
/// близость — к 20.01.2027, дню с безопасной стороны: «через 5 дней», а не
/// «через 36» (ревью версии 0.0.209). После 20 января — к самому сроку.
#[test]
fn the_first_monthly_premium_is_near_as_on_the_home_screen() {
    let profile = citizen();
    let empty = books(&profile, &[], &[]);
    let tax = turnover::quarter(empty, quarter(2026, 4));
    let closing = Closing::close(&tax, day(2027, 1, 10), Timestamp::from_unix_millis(1_760_000_000_000))
        .expect("пустой квартал закрывается");
    let closed = books(&profile, &[], std::slice::from_ref(&closing));
    let january = Kind::HealthMonth { year: 2027, month: 1 };
    let mut wrong = Vec::new();
    for (today, expected) in [
        (day(2027, 1, 15), (When::In(5), day(2027, 1, 20))),
        (day(2027, 1, 25), (When::In(26), day(2027, 2, 20))),
    ] {
        match row(closed, today).next {
            Some(Next { deadline, when, day }) if deadline.kind == january && (when, day) == expected => {}
            other => wrong.push(format!("{today:?}: ближайший срок {other:?}, а не взнос за январь {expected:?}")),
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// Налог посчитан не весь — так и сказано: доход без кода деятельности, а в
/// профиле кодов нет. Сумма — посчитанной части, как у карточки.
#[test]
fn an_incomplete_quarter_says_so() {
    let profile = ProfileData {
        activities: Vec::new(),
        ..citizen()
    };
    let operations = [income(day(2026, 8, 5), 1_000_000)];
    let open = books(&profile, &operations, &[]);
    let expected = turnover::quarter(open, quarter(2026, 3));
    assert!(!expected.complete(), "в примере налог посчитан весь: {expected:?}");
    match row(open, day(2026, 10, 14)).previous {
        Some(Previous::ToPay { tax, complete: false, .. }) if tax == expected.tax => {}
        other => panic!("неполный квартал: {other:?}"),
    }
}

/// ИП, зарегистрированный после прошлого квартала, расчёта за него не
/// подаёт: строки квартала нет, а ближайший срок — его первый.
#[test]
fn a_new_ie_has_no_previous_quarter() {
    let profile = ProfileData {
        person: Person {
            registered: Some(day(2026, 10, 5)),
            ..citizen().person
        },
        ..citizen()
    };
    let row = row(books(&profile, &[], &[]), day(2026, 10, 14));
    assert_eq!(row.previous, None);
    let first = deadlines::between(books(&profile, &[], &[]), day(2026, 10, 14), day(2027, 10, 14))
        .into_iter()
        .find(|deadline| !deadline.done);
    assert_eq!(row.next.map(|next| next.deadline), first);
    assert!(row.next.is_some_and(|next| next.when != When::Overdue));
}

/// Весь год — общий режим: оборот 2025 года больше порога (НК 254(2)).
/// Прошлый квартал — «ушёл с налога с оборота» с 1 января, как говорит
/// карточка налога (`turnover::quarter`), а не молчание; годовые платежи
/// 2026-го считает бухгалтер, и строки годовых нет, как нет и карточки на
/// главном экране, а не «0 ֏».
#[test]
fn a_general_year_says_so_and_has_no_annual_row() {
    let profile = citizen();
    let operations = [income(day(2025, 6, 10), 120_000_000)];
    let general = books(&profile, &operations, &[]);
    let today = day(2026, 10, 14);
    assert_eq!(annual::due_year(today, Some(&profile.person)), 2026, "в примере год ближайшего срока другой");
    assert_eq!(annual::year(general, 2026).status, Status::General, "в примере 2026 год не общий режим");
    let row = row(general, today);
    let mut wrong = Vec::new();
    match (turnover::quarter(general, quarter(2026, 3)).regime, row.previous) {
        (Regime::Left { from, .. }, Some(Previous::Left { quarter: q, from: got, micro: false }))
            if q == quarter(2026, 3) && got == from && from == day(2026, 1, 1) => {}
        (regime, previous) => wrong.push(format!("прошлый квартал {previous:?} при режиме {regime:?}")),
    }
    if row.annual.is_some() {
        wrong.push(format!("годовые {:?}", row.annual));
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// Замер (Правило 1), а не проверка: строка сводки занятого ИП — пять лет
/// книги по 1 600 записей дохода в год, как у замера годовых платежей
/// (`annual_timing`). Сводка 30 ИП — 30 таких строк, по одной между паузами.
/// `cargo test -p kvartik-core --release --lib summary_timing -- --ignored --nocapture`.
#[test]
#[ignore = "замер, а не проверка"]
fn summary_timing() {
    let mut operations = Vec::new();
    for year in 2022..=2026_u16 {
        for n in 0..1600_u16 {
            let date = day(year, (n % 12 + 1) as u8, (n % 28 + 1) as u8);
            operations.push(income(date, 10_000 + i64::from(n)));
        }
    }
    let profile = citizen();
    let started = std::time::Instant::now();
    let got = row(books(&profile, &operations, &[]), day(2026, 11, 16));
    let elapsed = started.elapsed();
    println!("строка сводки, {} записей: {elapsed:?}, к уплате {:?}", operations.len(), got.previous);
}
