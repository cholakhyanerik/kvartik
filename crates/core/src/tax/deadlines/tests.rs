//! Тесты сроков. Ожидаемые дни посчитаны вручную по тексту законов
//! (разведка задачи 21, 2026-10-20: НК 254(1), 259(1), 261(1), 262(1),
//! 53(8), 156(4.1); ՀՕ-324-Ն 10(3); ՀՕ-180-Ն 32(4); закон о праздниках;
//! решение 109-Ն) и по календарю: дни недели сверены отдельно,
//! стандартной библиотекой Python, а не кодом программы — код, записавший
//! себе эталон, проверял бы сам себя (Правило 2).

use serde_json::{Value, json};

use super::*;
use crate::model::document::Document;
use crate::model::ledger::Operation;
use crate::model::profile::{ProfileData, Stop};
use crate::model::record::Timestamp;
use crate::tax::turnover;

fn day(year: u16, month: u8, day: u8) -> Date {
    Date::new(year, month, day).expect("в тесте — существующая дата")
}

fn quarter(year: u16, number: u8) -> Quarter {
    Quarter::new(year, number).expect("в тесте — существующий квартал")
}

/// Источник, который проходит проверку разбора.
fn source() -> Value {
    json!({
        "law": "ՀՕ-180-Ն «Նորմատիվ իրավական ակտերի մասին»",
        "article": "հոդված 32, մաս 4",
        "url": "https://www.arlis.am/hy/acts/218697",
        "checked": "2026-10-20"
    })
}

/// Правила переноса для теста: выходные — суббота и воскресенье;
/// праздники — 1, 2 и 6 января и 31 декабря; решение о 2026 годе —
/// 4 мая нерабочий, а суббота 18 апреля — рабочая.
fn shift_rules() -> Rules {
    let text = json!({
        "version": "2026.1",
        "turnover_exclusions": [],
        "funded_pension": [],
        "turnover_rates": [],
        "high_tech": [],
        "turnover_threshold": [],
        "turnover_expenses": [],
        "turnover_form": [],
        "profit_tax": [],
        "social_payment": [],
        "stamp_duty": [],
        "health_insurance": [],
        "turnover_due": [],
        "turnover_application": [],
        "income_declaration": [],
        "settlement_due": [],
        "deadline_shift": [
            {"weekend": [6, 7], "from": "2025-01-01", "until": null, "source": source()}
        ],
        "holidays": [{
            "days": [{"month": 1, "day": 1}, {"month": 1, "day": 2}, {"month": 1, "day": 6}, {"month": 12, "day": 31}],
            "from": "2025-01-01",
            "until": null,
            "source": source()
        }],
        "days_off": [{
            "off": ["2026-05-04"],
            "work": ["2026-04-18"],
            "from": "2026-01-01",
            "until": "2026-12-31",
            "source": source()
        }]
    });
    Rules::parse(&text.to_string()).unwrap_or_else(|error| panic!("правила теста: {error:?}"))
}

/// Последний день — рабочий: он и остаётся; выходной, праздник или день,
/// объявленный нерабочим, — следующий рабочий, через сколько бы нерабочих
/// подряд; суббота, объявленная рабочей, — рабочий день. Раньше правил
/// переноса — без переноса: переносить не по чему.
#[test]
fn the_last_day_moves_to_the_next_working_day() {
    let rules = shift_rules();
    let cases = [
        // Вторник.
        (day(2026, 10, 20), day(2026, 10, 20)),
        // Воскресенье — понедельник.
        (day(2025, 4, 20), day(2025, 4, 21)),
        // Суббота — понедельник.
        (day(2026, 6, 20), day(2026, 6, 22)),
        // 31 декабря, 1 и 2 января — праздники, 2 и 3 января 2027 года —
        // суббота и воскресенье: понедельник, 4 января.
        (day(2026, 12, 31), day(2027, 1, 4)),
        // Суббота, объявленная рабочей.
        (day(2026, 4, 18), day(2026, 4, 18)),
        // Понедельник, объявленный нерабочим, — вторник.
        (day(2026, 5, 4), day(2026, 5, 5)),
        // Раньше правил переноса — суббота 2024 года остаётся собой.
        (day(2024, 4, 20), day(2024, 4, 20)),
    ];
    let wrong: Vec<String> = cases
        .iter()
        .filter_map(|&(by_law, expected)| {
            let got = shifted(&rules, by_law);
            (got != expected).then(|| format!("{by_law:?}: {got:?} вместо {expected:?}"))
        })
        .collect();
    assert!(wrong.is_empty(), "перенос: {wrong:#?}");
}

/// Встроенный файл правил: праздники 2025 года — без 27 января, с 21 января
/// 2026 года — с ним (ՀՕ-1-Ն); решение 109-Ն — 18 апреля 2026 рабочий,
/// 4 мая нерабочий; праздник на выходном не переносится.
#[test]
fn the_built_in_calendar_shifts_by_the_law() {
    let cases = [
        // 31 декабря 2025 — 2 января 2026 — праздники, 3–4 января —
        // выходные: понедельник, 5 января.
        (day(2025, 12, 31), day(2026, 1, 5)),
        // 27 января 2025 года — рабочий понедельник.
        (day(2025, 1, 27), day(2025, 1, 27)),
        // 27 и 28 января 2026 года — нерабочие: четверг, 29-е.
        (day(2026, 1, 27), day(2026, 1, 29)),
        (day(2026, 4, 18), day(2026, 4, 18)),
        (day(2026, 5, 4), day(2026, 5, 5)),
        // 1 мая 2027 года — суббота и праздник, 2-е — воскресенье.
        (day(2027, 5, 1), day(2027, 5, 3)),
        // Всеобщая декларация за 2025 год: 1 ноября 2026 — воскресенье.
        (day(2026, 11, 1), day(2026, 11, 2)),
    ];
    let wrong: Vec<String> = cases
        .iter()
        .filter_map(|&(by_law, expected)| {
            let got = last_day(by_law);
            (got != expected).then(|| format!("{by_law:?}: {got:?} вместо {expected:?}"))
        })
        .collect();
    assert!(wrong.is_empty(), "встроенный календарь: {wrong:#?}");
}

/// Срок квартала — 20-е число месяца после него (НК 261(1), 262(1)),
/// а выходной — понедельник; до правила — срока нет.
#[test]
fn quarters_are_due_on_the_twentieth() {
    let cases = [
        (quarter(2024, 4), Some((day(2025, 1, 20), day(2025, 1, 20)))),
        (quarter(2025, 1), Some((day(2025, 4, 20), day(2025, 4, 21)))),
        (quarter(2025, 2), Some((day(2025, 7, 20), day(2025, 7, 21)))),
        (quarter(2025, 3), Some((day(2025, 10, 20), day(2025, 10, 20)))),
        (quarter(2025, 4), Some((day(2026, 1, 20), day(2026, 1, 20)))),
        (quarter(2026, 3), Some((day(2026, 10, 20), day(2026, 10, 20)))),
        (quarter(2026, 4), Some((day(2027, 1, 20), day(2027, 1, 20)))),
        (quarter(2024, 3), None),
    ];
    let wrong: Vec<String> = cases
        .iter()
        .filter_map(|&(quarter, expected)| {
            let got = turnover(quarter).map(|deadline| (deadline.by_law, deadline.day));
            (got != expected).then(|| format!("{quarter:?}: {got:?} вместо {expected:?}"))
        })
        .collect();
    assert!(wrong.is_empty(), "кварталы: {wrong:#?}");
}

/// Гражданин 1990 года рождения, ИП с 2022 года.
fn citizen() -> ProfileData {
    ProfileData {
        person: Person {
            birth_year: Some(1990),
            pension_joined: None,
            residency: Some(Residency::Citizen),
            registered: Some(day(2022, 5, 12)),
            stops: Vec::new(),
        },
        ..ProfileData::default()
    }
}

fn books<'a>(profile: Option<&'a ProfileData>, closings: &'a [Closing]) -> Books<'a> {
    let documents: &[Document] = &[];
    let operations: &[Operation] = &[];
    Books {
        documents,
        operations,
        profile,
        closings,
        unreadable: 0,
    }
}

/// Сроки как пары «день, что» — для сравнения со списком, выписанным
/// руками.
fn listed(deadlines: &[Deadline]) -> Vec<(Date, Kind)> {
    deadlines.iter().map(|deadline| (deadline.day, deadline.kind)).collect()
}

/// Год сроков гражданина-ИП на налоге с оборота: квартальные, декларация
/// за 2025 год 1 ноября (воскресенье — 2 ноября), заявление 20 февраля
/// 2027 (суббота — 22-е), годовые платежи 20 апреля, медстраховка
/// помесячно с 2027 года — 20-го следующего месяца, а выходной —
/// понедельник; в один день — квартал, заявление, годовые, медстраховка,
/// декларация.
#[test]
fn a_year_of_deadlines() {
    let profile = citizen();
    let got = listed(&between(books(Some(&profile), &[]), day(2026, 10, 20), day(2027, 10, 20)));
    let month = |month| Kind::HealthMonth { year: 2027, month };
    let expected = vec![
        (day(2026, 10, 20), Kind::Turnover(quarter(2026, 3))),
        (day(2026, 11, 2), Kind::Declaration(2025)),
        (day(2027, 1, 20), Kind::Turnover(quarter(2026, 4))),
        (day(2027, 2, 22), Kind::Application(2027)),
        (day(2027, 2, 22), month(1)),
        (day(2027, 3, 22), month(2)),
        (day(2027, 4, 20), Kind::Turnover(quarter(2027, 1))),
        (day(2027, 4, 20), Kind::Annual(2026)),
        (day(2027, 4, 20), month(3)),
        (day(2027, 5, 20), month(4)),
        (day(2027, 6, 21), month(5)),
        (day(2027, 7, 1), Kind::Declaration(2026)),
        (day(2027, 7, 20), Kind::Turnover(quarter(2027, 2))),
        (day(2027, 7, 20), month(6)),
        (day(2027, 8, 20), month(7)),
        (day(2027, 9, 20), month(8)),
        (day(2027, 10, 20), Kind::Turnover(quarter(2027, 3))),
        (day(2027, 10, 20), month(9)),
    ];
    assert_eq!(got, expected);
}

/// Декларация — только у гражданина: у иностранца со статусом её нет.
#[test]
fn the_declaration_is_for_citizens() {
    let mut profile = citizen();
    profile.person.residency = Some(Residency::ForeignWithStatus);
    let got = between(books(Some(&profile), &[]), day(2026, 10, 20), day(2027, 10, 20));
    let declarations: Vec<&Deadline> = got
        .iter()
        .filter(|deadline| matches!(deadline.kind, Kind::Declaration(_)))
        .collect();
    assert!(declarations.is_empty(), "декларация у иностранца: {declarations:?}");
    assert!(
        got.iter().any(|deadline| deadline.kind == Kind::Turnover(quarter(2026, 3))),
        "без декларации пропали и кварталы: {got:?}"
    );
}

/// Квартал целиком внутри остановки — расчёта нет (НК 53(8)), неполный —
/// есть; остановлен по 20 февраля и не возобновил — заявления пока нет,
/// возобновил 10 марта — заявление через 20 дней, 30 марта (254(1)(5)).
#[test]
fn a_stop_moves_or_removes_deadlines() {
    let kinds = |profile: &ProfileData| -> Vec<Kind> {
        between(books(Some(profile), &[]), day(2026, 10, 20), day(2027, 6, 30))
            .into_iter()
            .map(|deadline| deadline.kind)
            .filter(|kind| matches!(kind, Kind::Turnover(_) | Kind::Application(_)))
            .collect()
    };
    let mut stopped = citizen();
    stopped.person.stops = vec![Stop::new(day(2026, 12, 15), None).expect("годный период")];
    assert_eq!(
        kinds(&stopped),
        [Kind::Turnover(quarter(2026, 3)), Kind::Turnover(quarter(2026, 4))],
        "остановлен с 15 декабря и не возобновил"
    );
    let mut resumed = citizen();
    resumed.person.stops = vec![Stop::new(day(2026, 12, 15), Some(day(2027, 3, 10))).expect("годный период")];
    let got = between(books(Some(&resumed), &[]), day(2026, 10, 20), day(2027, 6, 30));
    let application: Vec<(Date, Date)> = got
        .iter()
        .filter(|deadline| deadline.kind == Kind::Application(2027))
        .map(|deadline| (deadline.by_law, deadline.day))
        .collect();
    assert_eq!(application, [(day(2027, 3, 30), day(2027, 3, 30))], "возобновил 10 марта");
    assert!(
        got.iter().any(|deadline| deadline.kind == Kind::Turnover(quarter(2027, 1))),
        "I квартал 2027 года остановлен не целиком: {got:?}"
    );
}

/// Новый ИП — заявление через 20 дней после дня учёта (254(1)(2)), а
/// кварталов до регистрации нет.
#[test]
fn a_new_ie_applies_within_twenty_days() {
    let mut profile = citizen();
    profile.person.registered = Some(day(2027, 2, 5));
    let got = listed(&between(books(Some(&profile), &[]), day(2026, 10, 20), day(2027, 4, 30)));
    assert!(
        got.contains(&(day(2027, 2, 25), Kind::Application(2027))),
        "заявление нового ИП: {got:?}"
    );
    assert!(
        !got.iter().any(|(_, kind)| *kind == Kind::Turnover(quarter(2026, 4))),
        "квартал до регистрации: {got:?}"
    );
    assert!(
        got.contains(&(day(2027, 4, 20), Kind::Turnover(quarter(2027, 1)))),
        "квартал регистрации: {got:?}"
    );
}

/// Прошлый квартал, срок которого прошёл, а квартал не закрыт, —
/// просрочен; в день срока — ещё нет; закрытый — нет.
#[test]
fn an_unclosed_quarter_is_overdue_after_its_day() {
    let profile = citizen();
    assert_eq!(
        overdue(books(Some(&profile), &[]), day(2026, 10, 25)).map(|deadline| deadline.kind),
        Some(Kind::Turnover(quarter(2026, 3)))
    );
    assert_eq!(overdue(books(Some(&profile), &[]), day(2026, 10, 20)), None);
    let tax = turnover::quarter(books(Some(&profile), &[]), quarter(2026, 3));
    let closing = Closing::close(&tax, day(2026, 10, 15), Timestamp::from_unix_millis(1_760_000_000_000))
        .expect("пустой квартал закрывается");
    let closed = books(Some(&profile), std::slice::from_ref(&closing));
    assert_eq!(overdue(closed, day(2026, 10, 25)), None);
    // Закрытый квартал в списке — сделанным, а в напоминаниях его нет.
    let listed: Vec<(Kind, bool)> = between(closed, day(2026, 10, 14), day(2026, 10, 20))
        .into_iter()
        .map(|deadline| (deadline.kind, deadline.done))
        .collect();
    assert_eq!(listed, [(Kind::Turnover(quarter(2026, 3)), true)]);
    assert_eq!(reminders(closed, day(2026, 10, 19)), []);
}

/// Напоминание — за 7 дней и ближе, и о просроченном квартале.
#[test]
fn reminders_come_a_week_ahead() {
    let profile = citizen();
    let when = |today| -> Vec<(Kind, When)> {
        reminders(books(Some(&profile), &[]), today)
            .into_iter()
            .map(|reminder| (reminder.deadline.kind, reminder.when))
            .collect()
    };
    let q3 = Kind::Turnover(quarter(2026, 3));
    assert_eq!(when(day(2026, 10, 12)), []);
    assert_eq!(when(day(2026, 10, 14)), [(q3, When::In(6))]);
    assert_eq!(when(day(2026, 10, 19)), [(q3, When::Tomorrow)]);
    assert_eq!(when(day(2026, 10, 20)), [(q3, When::Today)]);
    // Квартал не закрыт, а до декларации за 2025 год — 7 дней.
    assert_eq!(
        when(day(2026, 10, 26)),
        [(q3, When::Overdue), (Kind::Declaration(2025), When::In(7))]
    );
}

/// Доход в драмах в день `date`.
fn income(date: Date, drams: i64) -> Operation {
    use crate::model::ledger::{Flow, OperationData};
    use crate::model::money::{Currency, Money};
    use crate::model::text::Line;
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
            activity: None,
            attachments: Vec::new(),
        },
        Timestamp::from_unix_millis(1_760_000_000_000),
    )
    .expect("в тесте — годная запись")
}

/// Ревью задачи 21: остановка с 1 февраля — январь работал, и заявление —
/// к 20 февраля (п. 1), а не после возобновления; медстраховки за год до
/// регистрации нет; квартал ухода с режима не просрочен; срок квартала для
/// окна — только если расчёт за квартал подают; год годовых платежей и
/// «откладывать» — по дню с переносом. Чего не хватает профилю, список
/// называет.
#[test]
fn review_cases() {
    let mut wrong = Vec::new();

    let mut later_stop = citizen();
    later_stop.person.stops = vec![Stop::new(day(2027, 2, 1), Some(day(2027, 3, 10))).expect("годный период")];
    let application: Vec<(Date, Date)> = between(books(Some(&later_stop), &[]), day(2027, 1, 1), day(2027, 6, 30))
        .into_iter()
        .filter(|deadline| deadline.kind == Kind::Application(2027))
        .map(|deadline| (deadline.by_law, deadline.day))
        .collect();
    if application != [(day(2027, 2, 20), day(2027, 2, 22))] {
        wrong.push(format!("остановка с 1 февраля: {application:?}"));
    }

    let mut new_ie = citizen();
    new_ie.person.registered = Some(day(2028, 1, 10));
    let health: Vec<Kind> = between(books(Some(&new_ie), &[]), day(2027, 12, 1), day(2028, 2, 28))
        .into_iter()
        .map(|deadline| deadline.kind)
        .filter(|kind| matches!(kind, Kind::HealthMonth { year: 2027, .. } | Kind::HealthYear(2027)))
        .collect();
    if !health.is_empty() {
        wrong.push(format!("медстраховка до регистрации: {health:?}"));
    }

    // Доход по коду 70 — с 5 августа 2026 года не налог с оборота.
    let coded = ProfileData {
        activities: vec![crate::model::activity::ActivityCode::parse("70.22.0").expect("код")],
        ..citizen()
    };
    let operations = [income(day(2026, 8, 5), 1_000_000)];
    let left = Books {
        documents: &[],
        operations: &operations,
        profile: Some(&coded),
        closings: &[],
        unreadable: 0,
    };
    if let Some(deadline) = overdue(left, day(2026, 10, 25)) {
        wrong.push(format!("квартал ухода с режима просрочен: {deadline:?}"));
    }
    if overdue(books(Some(&citizen()), &[]), day(2026, 10, 25)).is_none() {
        wrong.push("обычный незакрытый квартал не просрочен".to_owned());
    }

    if let Some(deadline) = quarter_deadline(books(Some(&new_ie), &[]), quarter(2027, 4)) {
        wrong.push(format!("срок квартала до регистрации: {deadline:?}"));
    }
    let mut stopped = citizen();
    stopped.person.stops = vec![Stop::new(day(2026, 12, 15), None).expect("годный период")];
    if let Some(deadline) = quarter_deadline(books(Some(&stopped), &[]), quarter(2027, 1)) {
        wrong.push(format!("срок квартала в остановке: {deadline:?}"));
    }
    if quarter_deadline(books(Some(&citizen()), &[]), quarter(2027, 1)).map(|deadline| deadline.day) != Some(day(2027, 4, 20)) {
        wrong.push("нет срока обычного квартала".to_owned());
    }

    // 20 апреля 2030 года — суббота: платежи за 2029 год — до 22 апреля.
    let person = citizen().person;
    for (today, year) in [(day(2030, 4, 21), 2029), (day(2030, 4, 22), 2029), (day(2030, 4, 23), 2030)] {
        let got = annual::due_year(today, Some(&person));
        if got != year {
            wrong.push(format!("год годовых платежей {today:?}: {got} вместо {year}"));
        }
    }

    let mut bare = citizen();
    bare.person.registered = None;
    bare.person.residency = None;
    let got = gaps(books(Some(&bare), &[]), day(2026, 10, 20), day(2027, 10, 20));
    let expected = Gaps {
        registered: true,
        residency: true,
        health: false,
    };
    if got != expected {
        wrong.push(format!("пробелы профиля: {got:?}"));
    }
    let mut unborn = citizen();
    unborn.person.birth_year = None;
    let got = gaps(books(Some(&unborn), &[]), day(2026, 10, 20), day(2027, 10, 20));
    if got != (Gaps { health: true, ..Gaps::default() }) {
        wrong.push(format!("без года рождения: {got:?}"));
    }
    if gaps(books(Some(&citizen()), &[]), day(2026, 10, 20), day(2027, 10, 20)) != Gaps::default() {
        wrong.push("у полного профиля пробелы".to_owned());
    }
    assert!(wrong.is_empty(), "ревью сроков: {wrong:#?}");
}
