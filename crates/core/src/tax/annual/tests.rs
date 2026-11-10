//! Тесты годовых платежей. Ожидаемые числа посчитаны вручную по тексту
//! законов (разведка задачи 20, 2026-10-20) и ответам бухгалтера-агента
//! 2026-10-19 (FEATURE_TASKS.md, «PDF против текста закона»), а не записаны
//! кодом: код, записавший себе эталон, проверял бы сам себя (Правило 2).
//! Посчитал агент 2026-10-20; сверка с бухгалтерами — задача 82.

use super::*;
use crate::model::activity::ActivityCode;
use crate::model::document::Document;
use crate::model::ledger::{Operation, OperationData};
use crate::model::money::{Currency, ExchangeRate, Money, RateSource};
use crate::model::profile::ProfileData;
use crate::model::record::Timestamp;
use crate::model::text::Line as Text;

fn day(year: u16, month: u8, day: u8) -> Date {
    Date::new(year, month, day).expect("в тесте — существующая дата")
}

fn stop(from: Date, until: Option<Date>) -> Stop {
    Stop::new(from, until).expect("в тесте — годный период")
}

fn at(millis: i64) -> Timestamp {
    Timestamp::from_unix_millis(millis)
}

/// Человек из профиля: гражданин, родился в 1990-м, зарегистрирован давно.
fn person() -> Person {
    Person {
        birth_year: Some(1990),
        pension_joined: None,
        pension_applied: None,
        residency: Some(Residency::Citizen),
        resident: None,
        registered: Some(day(2022, 5, 12)),
        turnover_applied: None,
        stops: Vec::new(),
    }
}

/// Профиль с этим человеком.
fn profile(person: Person) -> ProfileData {
    ProfileData {
        person,
        ..ProfileData::default()
    }
}

fn operation(data: OperationData) -> Operation {
    Operation::create(data, at(1_760_000_000_000)).expect("в тесте — годная запись")
}

/// Доход в драмах.
fn income(date: Date, drams: i64) -> Operation {
    operation(OperationData {
        flow: Flow::Income,
        date,
        paid: None,
        amount: Money::new(drams, Currency::Amd),
        rate: None,
        text: Text::new("Услуга").expect("текст"),
        client: None,
        supplier: Text::default(),
        proof: None,
        nature: None,
        activity: None,
        income_kind: None,
        attachments: Vec::new(),
    })
}

/// Доход в долларах; `rate_e4` — курс ЦБ в десятитысячных, `None` — нет.
fn usd_income(date: Date, cents: i64, rate_e4: Option<i64>) -> Operation {
    let rate = rate_e4.map(|e4| {
        ExchangeRate::new(Currency::Usd, e4, date, RateSource::Manual).expect("в тесте — годный курс")
    });
    operation(OperationData {
        amount: Money::new(cents, Currency::Usd),
        rate,
        ..income(date, 1).data().clone()
    })
}

fn books<'a>(profile: Option<&'a ProfileData>, operations: &'a [Operation], closings: &'a [Closing]) -> Books<'a> {
    let documents: &[Document] = &[];
    Books {
        documents,
        operations,
        profile,
        closings,
        openings: &[],
        unreadable: 0,
    }
}

/// Годовые платежи года по этим записям и профилю.
fn annual(profile: Option<&ProfileData>, operations: &[Operation], year: u16) -> Annual {
    super::year(books(profile, operations, &[]), year)
}

/// Граница — 1 января 1974 года: родившийся в 1974-м — участник
/// обязательно, в 1973-м — по выбору. Граница — первый день года, иначе
/// одного года рождения не хватило бы.
#[test]
fn born_in_1974_is_mandatory() {
    let today = day(2026, 10, 19);
    let rule = funded_pension(today).expect("правило на сегодня есть");
    assert_eq!(rule.mandatory_born_from, day(1974, 1, 1));
    assert!(pension_is_a_choice(1973, today));
    assert!(!pension_is_a_choice(1974, today));
    assert!(!pension_is_a_choice(1990, today));
}

/// О резидентстве спрашивают с года до 65-летия: в 2026 году — родившегося
/// в 1962-м (65 в 2027-м), и не родившегося в 1963-м.
#[test]
fn residence_is_asked_a_year_before_the_age() {
    let today = day(2026, 11, 2);
    assert!(residence_matters(1950, today));
    assert!(residence_matters(1961, today));
    assert!(residence_matters(1962, today));
    assert!(!residence_matters(1963, today));
    assert!(!residence_matters(1990, today));
}

/// Без исключений правило действует с 1 июля 2018 года: до того тот, кто
/// уже был ИП, мог отказаться от платежа (ст. 81, ч. 1 и 3). Правила на
/// день нет — и спрашивать не о чем.
#[test]
fn pension_rule_since_july_2018() {
    assert!(funded_pension(day(2018, 6, 30)).is_none());
    assert!(funded_pension(day(2018, 7, 1)).is_some());
    assert!(!pension_is_a_choice(1960, day(2018, 6, 30)));
    assert!(pension_is_a_choice(1960, day(2018, 7, 1)));
}

/// Месяцы года: с месяца регистрации или января по месяц ухода с налога
/// с оборота или декабрь — оба крайних целиком (ответы 3.1 и 3.8), но не
/// месяц, где нет ни дня режима, — без целых месяцев остановки (НК,
/// ст. 125, ч. 3, п. 1). Числа — вручную.
#[test]
fn months_count_from_registration_to_leaving_without_whole_stops() {
    let base = person();
    let stopped = |stops: Vec<Stop>| Person { stops, ..person() };
    let registered = |on: Date| Person {
        registered: Some(on),
        ..person()
    };
    let unknown = Person {
        registered: None,
        ..person()
    };
    let left = |from: Date, why: Leaving| Regime::Left { from, why };
    let code = ActivityCode::parse("70.22.0").expect("код");
    // Случай: что, чей профиль и режим года — и сколько месяцев, с какого
    // по какой и без каких.
    type Case = (&'static str, Option<Person>, Regime, u8, Option<(u8, u8)>, Vec<u8>);
    let cases: Vec<Case> = vec![
        ("зарегистрирован давно — весь год", Some(base.clone()), Regime::Turnover, 12, Some((1, 12)), vec![]),
        // Ответ 3.1: учёт 12 марта — март целиком, 10 месяцев.
        ("учёт 12 марта", Some(registered(day(2026, 3, 12))), Regime::Turnover, 10, Some((3, 12)), vec![]),
        ("учёт 31 декабря", Some(registered(day(2026, 12, 31))), Regime::Turnover, 1, Some((12, 12)), vec![]),
        ("учёт в следующем году", Some(registered(day(2027, 1, 1))), Regime::Turnover, 0, None, vec![]),
        ("дня регистрации нет — с января", Some(unknown.clone()), Regime::Turnover, 12, Some((1, 12)), vec![]),
        ("профиля нет — с января", None, Regime::Turnover, 12, Some((1, 12)), vec![]),
        // Ответ 3.8: порог превышен 5 августа — август целиком, 8 месяцев.
        (
            "ушёл по порогу 5 августа",
            Some(base.clone()),
            left(day(2026, 8, 5), Leaving::Threshold),
            8,
            Some((1, 8)),
            vec![],
        ),
        // Порог превышен 1 марта: часть сделки этого дня — ещё налог
        // с оборота, март целиком.
        ("ушёл по порогу 1 марта", Some(base.clone()), left(day(2026, 3, 1), Leaving::Threshold), 3, Some((1, 3)), vec![]),
        // Доход по закрывающему коду 10 марта: март неполный — целиком.
        ("ушёл по коду 10 марта", Some(base.clone()), left(day(2026, 3, 10), Leaving::Code(code)), 3, Some((1, 3)), vec![]),
        // Доход по коду 1 марта: весь март вне режима — январь и февраль.
        ("ушёл по коду 1 марта", Some(base.clone()), left(day(2026, 3, 1), Leaving::Code(code)), 2, Some((1, 2)), vec![]),
        // Доход по коду 1 января: ни дня режима за весь год.
        ("ушёл по коду 1 января", Some(base.clone()), left(day(2026, 1, 1), Leaving::Code(code)), 0, None, vec![]),
        ("прошлый год сверх порога", Some(base.clone()), left(day(2026, 1, 1), Leaving::PreviousYear), 0, None, vec![]),
        // Остановлена 1 марта, возобновлена 1 июня: март, апрель, май — целые.
        (
            "остановка с 1 марта по 1 июня",
            Some(stopped(vec![stop(day(2026, 3, 1), Some(day(2026, 6, 1)))])),
            Regime::Turnover,
            9,
            Some((1, 12)),
            vec![3, 4, 5],
        ),
        // Пример разведки задачи 20: с 10 марта по 1 июля — апрель, май,
        // июнь; 9 × 5 000 = 45 000.
        (
            "остановка с 10 марта по 1 июля",
            Some(stopped(vec![stop(day(2026, 3, 10), Some(day(2026, 7, 1)))])),
            Regime::Turnover,
            9,
            Some((1, 12)),
            vec![4, 5, 6],
        ),
        // По 30 июня: 30 июня уже рабочий — июнь не целый, 10 месяцев.
        (
            "остановка с 10 марта по 30 июня",
            Some(stopped(vec![stop(day(2026, 3, 10), Some(day(2026, 6, 30)))])),
            Regime::Turnover,
            10,
            Some((1, 12)),
            vec![4, 5],
        ),
        (
            "остановка без возобновления с 2 сентября",
            Some(stopped(vec![stop(day(2026, 9, 2), None)])),
            Regime::Turnover,
            9,
            Some((1, 12)),
            vec![10, 11, 12],
        ),
        (
            "остановка с прошлого года по 1 февраля",
            Some(stopped(vec![stop(day(2025, 11, 20), Some(day(2026, 2, 1)))])),
            Regime::Turnover,
            11,
            Some((1, 12)),
            vec![1],
        ),
        (
            "две остановки и учёт в апреле",
            Some(Person {
                registered: Some(day(2026, 4, 3)),
                stops: vec![
                    stop(day(2026, 4, 3), Some(day(2026, 6, 1))),
                    stop(day(2026, 11, 1), None),
                ],
                ..person()
            }),
            Regime::Turnover,
            6,
            Some((4, 12)),
            vec![5, 11, 12],
        ),
        (
            "остановка после ухода с режима не считается",
            Some(stopped(vec![stop(day(2026, 10, 1), None)])),
            left(day(2026, 9, 10), Leaving::Threshold),
            9,
            Some((1, 9)),
            vec![],
        ),
    ];
    let mut wrong = Vec::new();
    for (case, person, regime, count, span, stopped) in cases {
        let got = months(person.as_ref(), 2026, regime);
        if got.count() != count || got.span != span || got.stopped != stopped {
            wrong.push(format!("{case}: {got:?}, а не {count} месяцев {span:?} без {stopped:?}"));
        }
    }
    let got = months(Some(&unknown), 2026, Regime::Turnover);
    if !got.registration_unknown || got.registered.is_some() {
        wrong.push(format!("день регистрации не указан: {got:?}"));
    }
    let got = months(Some(&registered(day(2026, 3, 12))), 2026, Regime::Turnover);
    if got.registration_unknown || got.registered != Some(day(2026, 3, 12)) {
        wrong.push(format!("учёт в этом году: {got:?}"));
    }
    let got = months(Some(&base), 2026, Regime::Turnover);
    if got.registered.is_some() {
        wrong.push(format!("учёт в прошлые годы назван учётом этого: {got:?}"));
    }
    assert!(wrong.is_empty(), "месяцы года: {wrong:#?}");
}

/// Обычный год IT-ИП: гражданин 1990 года, весь 2026 год на налоге
/// с оборота, оборот 3 840 000 ֏, а за 2025 год — 2 500 000 ֏. Налог на
/// прибыль 12 × 5 000 = 60 000, соцплатёж — столько же, гербовый сбор
/// 12 000 (оборот до 12 000 000 включительно), к 20.04.2027 — 132 000;
/// медстраховка за 2026 — 129 600 до 20.07.2026: оборот 2025 года больше
/// 2 400 000. С 20.10.2026 по апрель 2027 — 7 месяцев: 132 000 / 7 =
/// 18 857,14…, вверх до драма — 18 858.
#[test]
fn an_ordinary_year() {
    let data = profile(person());
    let operations = [
        income(day(2025, 6, 30), 2_500_000),
        income(day(2026, 2, 10), 1_210_000),
        income(day(2026, 5, 10), 1_340_000),
        income(day(2026, 9, 10), 1_290_000),
    ];
    let got = annual(Some(&data), &operations, 2026);
    let mut wrong = Vec::new();
    let due = day(2027, 4, 20);
    let fixed = Fixed::Due {
        monthly: 5_000,
        months: 12,
        amount: 60_000,
        due,
    };
    if got.status != Status::Turnover {
        wrong.push(format!("положение: {:?}", got.status));
    }
    if got.profit != fixed {
        wrong.push(format!("налог на прибыль: {:?}", got.profit));
    }
    let social = Social::Due {
        payment: fixed,
        voluntary: false,
        applied: None,
    };
    if got.social != social {
        wrong.push(format!("соцплатёж: {:?}", got.social));
    }
    let stamp = Stamp::Due {
        amount: 12_000,
        base: 3_840_000,
        up_to: Some(12_000_000),
        next: Some((12_000_001, 120_000)),
        due,
        unrated: 0,
    };
    if got.stamp != stamp {
        wrong.push(format!("гербовый сбор: {:?}", got.stamp));
    }
    let health = Health::Year {
        amount: 129_600,
        due: day(2026, 7, 20),
        over: 2_400_000,
        income: 2_500_000,
    };
    if got.health != health {
        wrong.push(format!("медстраховка: {:?}", got.health));
    }
    let total = Total::of(&got);
    if total
        != (Total {
            due: Some(due),
            sum: Some(132_000),
            complete: true,
        })
    {
        wrong.push(format!("итог: {total:?}"));
    }
    if saving(&got, day(2026, 10, 20)) != Some(Saving { months: 7, per_month: 18_858 }) {
        wrong.push(format!("откладывать с 20.10.2026: {:?}", saving(&got, day(2026, 10, 20))));
    }
    assert!(wrong.is_empty(), "обычный год: {wrong:#?}");
}

/// Итог не полон, когда без бухгалтера его не узнать: ушёл с налога
/// с оборота посреди года (месяцы общего режима — свой налог на прибыль
/// и соцплатёж, НК 125(3.1), ՀՕ-244-Ն 6(2)(2)); дня регистрации нет; доход
/// без курса у порога налога с оборота; доход в целом месяце остановки.
/// Сумма — то, что известно.
#[test]
fn the_total_is_partial_when_something_is_unknown() {
    let mut wrong = Vec::new();
    // Порог 115 млн превышен 10 августа: 60 + 50 + 10 млн.
    let crossed = [
        income(day(2026, 2, 1), 60_000_000),
        income(day(2026, 5, 1), 50_000_000),
        income(day(2026, 8, 10), 10_000_000),
    ];
    let data = profile(person());
    let got = annual(Some(&data), &crossed, 2026);
    let total = Total::of(&got);
    // 8 × 5 000 + 8 × 5 000 + 120 000.
    if !got.months.left_midyear() || total.complete || total.sum != Some(200_000) {
        wrong.push(format!("уход по порогу: {:?}, {total:?}", got.months));
    }
    let unknown = profile(Person {
        registered: None,
        ..person()
    });
    let got = annual(Some(&unknown), &[income(day(2026, 3, 1), 1_000_000)], 2026);
    if Total::of(&got).complete {
        wrong.push("без дня регистрации итог полный".to_owned());
    }
    // 110 млн ֏ за 2025 год и 20 000 $ без курса: с курсом оборот мог быть
    // больше 115 млн, и весь 2026 год — общий режим.
    let unrated = [
        income(day(2025, 3, 1), 110_000_000),
        usd_income(day(2025, 6, 1), 2_000_000, None),
        income(day(2026, 3, 1), 1_000_000),
    ];
    let got = annual(Some(&data), &unrated, 2026);
    if got.regime_unrated != 1 || Total::of(&got).complete {
        wrong.push(format!("без курса у порога: {}, {:?}", got.regime_unrated, Total::of(&got)));
    }
    // Остановка с 1 марта без возобновления, а доход 15 июня: месяцы
    // остановки под вопросом.
    let stopped = profile(Person {
        stops: vec![stop(day(2026, 3, 1), None)],
        ..person()
    });
    let got = annual(Some(&stopped), &[income(day(2026, 6, 15), 2_000_000)], 2026);
    if got.stop_income != Some(day(2026, 6, 15)) || Total::of(&got).complete {
        wrong.push(format!("доход в остановке: {:?}, {:?}", got.stop_income, Total::of(&got)));
    }
    // Ничего не известно — суммы нет, а не ноль.
    let old = annual(Some(&data), &[], 2024);
    if Total::of(&old).sum.is_some() {
        wrong.push(format!("2024 год без правил: {:?}", Total::of(&old)));
    }
    assert!(wrong.is_empty(), "неполный итог: {wrong:#?}");
}

/// База гербового сбора — сумма строк квартальных расчётов, каждая целыми
/// драмами: четыре квартала по 2 999 614 ֏ и 1 $ по 386,40 ֏ — строка
/// квартала 3 000 000,40 ֏, в расчёте 3 000 000, за год 12 000 000, и сбор
/// 12 000. Сумма лум за год, доведённая до драма один раз, дала бы
/// 12 000 001 и 120 000.
#[test]
fn stamp_base_is_the_sum_of_quarter_lines_in_whole_drams() {
    let data = profile(person());
    let mut operations = Vec::new();
    for month in [2, 5, 8, 11] {
        operations.push(income(day(2026, month, 10), 2_999_614));
        operations.push(usd_income(day(2026, month, 11), 100, Some(3_864_000)));
    }
    // Случай различает способы: сумма лум за год, доведённая до драма один
    // раз, — 12 000 001.
    let lumas = operations.iter().map(|operation| {
        let data = operation.data();
        data.amount.to_lumas(data.rate).expect("переводится")
    });
    assert_eq!(Lumas::total(lumas).expect("помещается").drams(), 12_000_001);
    let got = annual(Some(&data), &operations, 2026);
    match got.stamp {
        Stamp::Due {
            amount: 12_000,
            base: 12_000_000,
            ..
        } => {}
        other => panic!("гербовый сбор у границы: {other:?}"),
    }
}

/// Строка, выбранная у дохода, — своя строка и для базы гербового сбора:
/// услуги 11 999 999,60 ֏ и аренда 1,50 ֏ — в расчёте 11 999 999 и 1, база
/// 12 000 000 и сбор 12 000. Одной строкой вышло бы 12 000 001 и 120 000.
#[test]
fn stamp_base_keeps_the_row_chosen_at_the_income() {
    let data = profile(person());
    let date = day(2026, 5, 10);
    let operations = [
        income(date, 11_999_999),
        usd_income(date, 1, Some(600_000)),
        operation(OperationData {
            income_kind: Some(IncomeKind::Rent),
            ..usd_income(date, 1, Some(1_500_000)).data().clone()
        }),
    ];
    match annual(Some(&data), &operations, 2026).stamp {
        Stamp::Due {
            amount: 12_000,
            base: 12_000_000,
            ..
        } => {}
        other => panic!("гербовый сбор у границы: {other:?}"),
    }
}

/// Шкала гербового сбора — по отчётному году: за 2025 — старая, четыре
/// ступени, граница включительно (ст. 12, ч. 1, п. 2 в ред. до ՀՕ-477-Ն);
/// с 2026 — 12 000 до 12 000 000 включительно и 120 000 выше.
#[test]
fn stamp_steps_follow_the_year() {
    let data = profile(person());
    let cases = [
        (2025, 2_400_000, 18_000, Some(2_400_000)),
        (2025, 2_400_001, 24_000, Some(6_000_000)),
        (2025, 5_000_000, 24_000, Some(6_000_000)),
        (2025, 12_000_000, 48_000, Some(12_000_000)),
        (2025, 12_000_001, 120_000, None),
        (2026, 1, 12_000, Some(12_000_000)),
        (2026, 12_000_000, 12_000, Some(12_000_000)),
        (2026, 12_000_001, 120_000, None),
        (2027, 50_000_000, 120_000, None),
    ];
    let mut wrong = Vec::new();
    for (year, base, amount, up_to) in cases {
        let got = annual(Some(&data), &[income(day(year, 6, 1), base)], year).stamp;
        let ok = matches!(got, Stamp::Due { amount: a, base: b, up_to: u, due, .. }
            if a == amount && b == base && u == up_to && due == day(year + 1, 4, 20));
        if !ok {
            wrong.push(format!("{year}, оборот {base}: {got:?}"));
        }
    }
    assert!(wrong.is_empty(), "ступени: {wrong:#?}");
}

/// Без дохода за год сбор двояк: 0 или нижняя ступень — осторожно нижняя,
/// 12 000 за 2026 и 18 000 за 2025 (вопрос 4.2); весь год остановки без
/// дохода — сбора нет, а с доходом — по ступени; иностранец без статуса
/// проживания освобождён; без статуса в профиле — не известно.
#[test]
fn stamp_without_turnover_and_exemptions() {
    let mut wrong = Vec::new();
    let data = profile(person());
    for (year, cautious) in [(2025, 18_000), (2026, 12_000)] {
        let got = annual(Some(&data), &[], year);
        if got.stamp != (Stamp::NoTurnover { cautious, due: day(year + 1, 4, 20) }) {
            wrong.push(format!("{year} без дохода: {:?}", got.stamp));
        }
        let total = Total::of(&got);
        if total.sum != Some(60_000 + 60_000 + cautious) || !total.complete {
            wrong.push(format!("{year} без дохода, итог: {total:?}"));
        }
    }
    let stopped = profile(Person {
        stops: vec![stop(day(2025, 12, 1), None)],
        ..person()
    });
    let got = annual(Some(&stopped), &[], 2026);
    if got.stamp != Stamp::Stopped || got.profit.amount() != Some(0) {
        wrong.push(format!("весь год остановки: {:?}, {:?}", got.stamp, got.profit));
    }
    let got = annual(Some(&stopped), &[income(day(2026, 3, 1), 1_000_000)], 2026);
    if !matches!(got.stamp, Stamp::Due { amount: 12_000, .. }) || got.stop_income != Some(day(2026, 3, 1)) {
        wrong.push(format!("остановка на весь год, а доход есть: {:?}, {:?}", got.stamp, got.stop_income));
    }
    let foreign = profile(Person {
        residency: Some(Residency::ForeignWithoutStatus),
        ..person()
    });
    let got = annual(Some(&foreign), &[income(day(2026, 3, 1), 1_000_000)], 2026);
    let total = Total::of(&got);
    if got.stamp != Stamp::Exempt
        || got.social != Social::Out(SocialOut::NoStatus)
        || got.health != Health::Out(HealthOut::NoStatus)
        || total.sum != Some(60_000)
        || !total.complete
    {
        wrong.push(format!("без статуса проживания: {got:?}"));
    }
    let unknown = profile(Person {
        residency: None,
        ..person()
    });
    let got = annual(Some(&unknown), &[income(day(2026, 3, 1), 1_000_000)], 2026);
    let missing = Missing {
        residency: true,
        ..Missing::default()
    };
    let total = Total::of(&got);
    if got.stamp != Stamp::Unknown(missing)
        || got.social != Social::Unknown(missing)
        || total.complete
        || total.sum != Some(60_000)
    {
        wrong.push(format!("статус не указан: {got:?}, итог {total:?}"));
    }
    assert!(wrong.is_empty(), "гербовый сбор без оборота: {wrong:#?}");
}

/// Строки без курса: оборот без них не выше границы — сбор может быть
/// выше; оборот уже выше — сбор известен. Итог в обоих случаях не полон:
/// без курса не узнать, не перешёл ли оборот порог налога с оборота.
#[test]
fn unrated_income_leaves_the_stamp_open_below_the_top() {
    let data = profile(person());
    let below = [income(day(2026, 3, 1), 11_000_000), usd_income(day(2026, 4, 1), 500_000, None)];
    let got = annual(Some(&data), &below, 2026);
    let mut wrong = Vec::new();
    if !matches!(got.stamp, Stamp::Due { amount: 12_000, unrated: 1, next: Some(_), .. }) || Total::of(&got).complete {
        wrong.push(format!("ниже границы: {:?}, {:?}", got.stamp, Total::of(&got)));
    }
    let above = [income(day(2026, 3, 1), 13_000_000), usd_income(day(2026, 4, 1), 500_000, None)];
    let got = annual(Some(&data), &above, 2026);
    if !matches!(got.stamp, Stamp::Due { amount: 120_000, unrated: 1, next: None, .. })
        || got.regime_unrated != 1
        || Total::of(&got).complete
    {
        wrong.push(format!("выше границы: {:?}, {:?}", got.stamp, Total::of(&got)));
    }
    assert!(wrong.is_empty(), "строки без курса: {wrong:#?}");
}

/// Соцплатёж: родившийся до 1974 года — только если вступил сам; без ответа
/// или без года рождения — не известно.
#[test]
fn social_payment_by_birth_year_and_choice() {
    let with = |birth_year: Option<u16>, pension_joined: Option<bool>| {
        profile(Person {
            birth_year,
            pension_joined,
            ..person()
        })
    };
    let due = Fixed::Due {
        monthly: 5_000,
        months: 12,
        amount: 60_000,
        due: day(2027, 4, 20),
    };
    let paying = |voluntary: bool| Social::Due {
        payment: due,
        voluntary,
        applied: None,
    };
    let cases = [
        ("1974 — обязательно", with(Some(1974), None), paying(false)),
        ("1970, вступил", with(Some(1970), Some(true)), paying(true)),
        ("1970, не вступал", with(Some(1970), Some(false)), Social::Out(SocialOut::NotJoined)),
        (
            "1970, без ответа",
            with(Some(1970), None),
            Social::Unknown(Missing {
                pension_joined: true,
                ..Missing::default()
            }),
        ),
        (
            "без года рождения",
            with(None, Some(true)),
            Social::Unknown(Missing {
                birth_year: true,
                ..Missing::default()
            }),
        ),
    ];
    let mut wrong = Vec::new();
    for (case, data, expected) in cases {
        let got = annual(Some(&data), &[], 2026).social;
        if got != expected {
            wrong.push(format!("{case}: {got:?}"));
        }
    }
    assert!(wrong.is_empty(), "соцплатёж: {wrong:#?}");
}

/// Вступивший сам платит с месяца после дня заявления (ՀՕ-244-Ն, ст. 6, ч. 4,
/// п. 4). Числа — вручную по закону: заявление 10 марта — с апреля, 9 месяцев
/// по 5 000 = 45 000; 31 декабря — со следующего января; в декабре — за этот
/// год платежа нет. Целые месяцы остановки и месяцы до регистрации считаются
/// как у всех, а день заявления у того, кто участвует по закону, ничего не
/// значит.
#[test]
fn social_payment_starts_the_month_after_the_application() {
    let with = |birth_year: u16, joined: bool, applied: Date| Person {
        birth_year: Some(birth_year),
        pension_joined: Some(joined),
        pension_applied: Some(applied),
        ..person()
    };
    let march = day(2026, 3, 10);
    let due = |months: u8, applied: Option<Date>| Social::Due {
        payment: Fixed::Due {
            monthly: 5_000,
            months,
            amount: 5_000 * i64::from(months),
            due: day(2027, 4, 20),
        },
        voluntary: true,
        applied,
    };
    let december = day(2026, 12, 5);
    let cases = [
        ("март этого года", with(1970, true, march), due(9, Some(march))),
        ("прошлый год", with(1970, true, day(2024, 2, 1)), due(12, Some(day(2024, 2, 1)))),
        ("31 декабря прошлого года", with(1970, true, day(2025, 12, 31)), due(12, Some(day(2025, 12, 31)))),
        ("ноябрь: остался декабрь", with(1970, true, day(2026, 11, 30)), due(1, Some(day(2026, 11, 30)))),
        ("декабрь: за год нет", with(1970, true, december), Social::Out(SocialOut::Later(december))),
        ("следующий год", with(1970, true, day(2027, 2, 1)), Social::Out(SocialOut::Later(day(2027, 2, 1)))),
        (
            "с остановкой мая и июня",
            Person {
                stops: vec![stop(day(2026, 5, 1), Some(day(2026, 7, 1)))],
                ..with(1970, true, march)
            },
            due(7, Some(march)),
        ),
        (
            "остановка раньше месяца начала не вычитается дважды",
            Person {
                stops: vec![stop(day(2026, 1, 1), Some(day(2026, 3, 1)))],
                ..with(1970, true, march)
            },
            due(9, Some(march)),
        ),
        (
            "зарегистрирован позже заявления",
            Person {
                registered: Some(day(2026, 6, 15)),
                ..with(1970, true, march)
            },
            due(7, Some(march)),
        ),
        ("не вступал", with(1970, false, march), Social::Out(SocialOut::NotJoined)),
        (
            "по закону: день не значит ничего",
            with(1974, true, march),
            Social::Due {
                payment: Fixed::Due {
                    monthly: 5_000,
                    months: 12,
                    amount: 60_000,
                    due: day(2027, 4, 20),
                },
                voluntary: false,
                applied: None,
            },
        ),
    ];
    let mut wrong = Vec::new();
    for (case, data, expected) in cases {
        let got = annual(Some(&profile(data)), &[], 2026).social;
        if got != expected {
            wrong.push(format!("{case}: {got:?}"));
        }
    }
    let later = annual(Some(&profile(with(1970, true, december))), &[], 2026);
    let total = Total::of(&later);
    if total.sum != Some(72_000) || !total.complete {
        wrong.push(format!("итог без соцплатежа: {total:?}"));
    }
    assert!(wrong.is_empty(), "соцплатёж с месяца после заявления: {wrong:#?}");
}

/// Закрытые кварталы 2025 года — по профилю с кодом (иначе квартал не
/// посчитан целиком и не закрывается): доход каждого квартала — снимком.
fn closed_2025(data: &ProfileData, operations: &[Operation]) -> Vec<Closing> {
    (1..=4)
        .map(|number| {
            let quarter = Quarter::new(2025, number).expect("квартал");
            let tax = turnover::quarter(books(Some(data), operations, &[]), quarter);
            Closing::close(&tax, day(2026, 10, 20), at(1_760_000_000_000)).expect("квартал закрывается")
        })
        .collect()
}

/// Медстраховка за 2026 год: годовая ставка 129 600 до 20.07.2026 — у ИП
/// с оборотом 2025 года больше 2 400 000 (ровно 2 400 000 — нет); без
/// записей за 2025 год — не известно; новый ИП 2026 года — «нет значения»
/// (вопрос 5.2); остановленный на 1 января 2026 года — с месяца после
/// возобновления; уменьшение оборота после поданного расчёта — закрытого
/// квартала — не учитывается (ст. 45, ч. 1, п. 5).
#[test]
fn health_insurance_for_2026() {
    let data = profile(person());
    let mut wrong = Vec::new();
    for (income_2025, expected) in [
        (
            2_400_001,
            Health::Year {
                amount: 129_600,
                due: day(2026, 7, 20),
                over: 2_400_000,
                income: 2_400_001,
            },
        ),
        (
            2_400_000,
            Health::Out(HealthOut::BelowThreshold {
                over: 2_400_000,
                income: 2_400_000,
            }),
        ),
    ] {
        let got = annual(Some(&data), &[income(day(2025, 11, 1), income_2025)], 2026).health;
        if got != expected {
            wrong.push(format!("оборот 2025 года {income_2025}: {got:?}"));
        }
    }
    let got = annual(Some(&data), &[], 2026).health;
    let expected = HealthUnclear::NoPreviousYear {
        over: 2_400_000,
        amount: 129_600,
        due: day(2026, 7, 20),
    };
    if got != Health::Unclear(expected) {
        wrong.push(format!("без записей за 2025 год: {got:?}"));
    }
    let unrated = [income(day(2025, 3, 1), 1_000_000), usd_income(day(2025, 4, 1), 100_000, None)];
    let got = annual(Some(&data), &unrated, 2026).health;
    if got
        != Health::Unclear(HealthUnclear::PreviousUnrated {
            over: 2_400_000,
            income: 1_000_000,
            unrated: 1,
            amount: 129_600,
            due: day(2026, 7, 20),
        })
    {
        wrong.push(format!("2025 год без курса: {got:?}"));
    }
    let new = profile(Person {
        registered: Some(day(2026, 3, 12)),
        ..person()
    });
    let got = annual(Some(&new), &[], 2026);
    if got.health != Health::Unclear(HealthUnclear::NewIe) || got.profit.amount() != Some(50_000) {
        wrong.push(format!("новый ИП 2026 года: {:?}, {:?}", got.health, got.profit));
    }
    let stopped = profile(Person {
        stops: vec![stop(day(2025, 11, 1), None)],
        ..person()
    });
    let got = annual(Some(&stopped), &[income(day(2025, 6, 1), 9_000_000)], 2026).health;
    if got != Health::Out(HealthOut::Stopped(day(2026, 1, 1))) {
        wrong.push(format!("остановлен на 1 января: {got:?}"));
    }
    let resumed = profile(Person {
        stops: vec![stop(day(2025, 11, 1), Some(day(2026, 6, 1)))],
        ..person()
    });
    let got = annual(Some(&resumed), &[income(day(2025, 6, 1), 9_000_000)], 2026).health;
    if got != Health::Unclear(HealthUnclear::Resumed(day(2026, 6, 1))) {
        wrong.push(format!("возобновил в 2026 году: {got:?}"));
    }
    // Поданы четыре квартала 2025 года на 2 500 000; потом доход уменьшили
    // до 2 300 000 — в книге меньше порога, а поданное — больше.
    let coded = ProfileData {
        activities: vec![ActivityCode::parse("62.01.0").expect("код")],
        ..profile(person())
    };
    let filed = [income(day(2025, 2, 1), 2_500_000)];
    let closings = closed_2025(&coded, &filed);
    let reduced = [income(day(2025, 2, 1), 2_300_000)];
    let got = super::year(books(Some(&coded), &reduced, &closings), 2026).health;
    if !matches!(got, Health::Year { income: 2_500_000, .. }) {
        wrong.push(format!("уменьшено после поданного расчёта: {got:?}"));
    }
    assert!(wrong.is_empty(), "медстраховка 2026: {wrong:#?}");
}

/// Медстраховка с 2027 года — помесячно, 10 800 ֏ до 20-го числа
/// следующего месяца: весь год — 12 месяцев и 129 600; учёт 5 марта —
/// с марта, 10 месяцев и 108 000. Остановлен на 1 января 2027 года тот, для
/// кого закон вступает только тогда (оборот 2025 года не выше порога), —
/// с месяца после возобновления: возобновил 15 апреля — с мая, 8 месяцев
/// и 86 400; возобновил 10 декабря или не возобновил — за 2027 год не
/// платит. Для кого закон действует с 2026 года, срез 2027 года, по
/// прочтению, не писан — «нет значения» со словами; остановка, отложившая
/// и срез 2026 года, откладывает и этот.
#[test]
fn health_insurance_from_2027_is_monthly() {
    let month = |months: u8, after: Option<Date>| Health::Month {
        monthly: 10_800,
        due_day: 20,
        months,
        amount: 10_800 * i64::from(months),
        after,
    };
    let with = |registered: Date, stops: Vec<Stop>| {
        profile(Person {
            registered: Some(registered),
            stops,
            ..person()
        })
    };
    let low = [income(day(2025, 6, 1), 1_000_000)];
    let high = [income(day(2025, 6, 1), 9_000_000)];
    let since = day(2022, 5, 12);
    let cases: Vec<(&str, ProfileData, &[Operation], Health)> = vec![
        ("весь год", with(since, vec![]), &low, month(12, None)),
        ("учёт 5 марта", with(day(2027, 3, 5), vec![]), &[], month(10, None)),
        (
            "группа 2027 года, возобновил 15 апреля",
            with(since, vec![stop(day(2026, 11, 1), Some(day(2027, 4, 15)))]),
            &low,
            month(8, Some(day(2027, 4, 15))),
        ),
        (
            "группа 2027 года, возобновил 10 декабря",
            with(since, vec![stop(day(2026, 11, 1), Some(day(2027, 12, 10)))]),
            &low,
            Health::Out(HealthOut::Stopped(day(2027, 1, 1))),
        ),
        (
            "группа 2027 года, не возобновил",
            with(since, vec![stop(day(2026, 11, 1), None)]),
            &low,
            Health::Out(HealthOut::Stopped(day(2027, 1, 1))),
        ),
        (
            "закон с 2026 года, остановка на 1 января 2027",
            with(since, vec![stop(day(2026, 11, 1), Some(day(2027, 4, 15)))]),
            &high,
            Health::Unclear(HealthUnclear::StoppedInForce {
                starts: day(2027, 1, 1),
                known: true,
            }),
        ),
        (
            "оборот 2025 года не известен, остановка на 1 января 2027",
            with(since, vec![stop(day(2026, 11, 1), None)]),
            &[],
            Health::Unclear(HealthUnclear::StoppedInForce {
                starts: day(2027, 1, 1),
                known: false,
            }),
        ),
        (
            "остановка с 2025 года на оба среза",
            with(since, vec![stop(day(2025, 11, 1), None)]),
            &high,
            Health::Out(HealthOut::Stopped(day(2027, 1, 1))),
        ),
        (
            "остановка посреди года не освобождает",
            with(since, vec![stop(day(2027, 3, 1), Some(day(2027, 9, 1)))]),
            &low,
            month(12, None),
        ),
    ];
    let mut wrong = Vec::new();
    for (case, data, operations, expected) in cases {
        let got = annual(Some(&data), operations, 2027).health;
        if got != expected {
            wrong.push(format!("{case}: {got:?}"));
        }
    }
    let resumed = with(since, vec![stop(day(2026, 11, 1), Some(day(2027, 4, 15)))]);
    let later = annual(Some(&resumed), &low, 2028);
    if later.health != month(12, None) {
        wrong.push(format!("2028 год после возобновления: {:?}", later.health));
    }
    assert!(wrong.is_empty(), "медстраховка с 2027 года: {wrong:#?}");
}

/// Гражданин, которому уже исполнилось 65, без ответа о резидентстве —
/// «нет значения», с ответом — точно: за резидента платит государство,
/// нерезидент платит сам; в год, когда исполняется 65, не известно, только
/// если он не сказал, что нерезидент; иностранец со статусом платит в любом
/// возрасте; без года рождения у гражданина — не известно.
#[test]
fn health_insurance_and_age() {
    let with = |birth_year: Option<u16>, residency: Residency| {
        profile(Person {
            birth_year,
            residency: Some(residency),
            ..person()
        })
    };
    let answered = |birth_year: u16, resident: bool| {
        profile(Person {
            birth_year: Some(birth_year),
            resident: Some(resident),
            ..person()
        })
    };
    let month = Health::Month {
        monthly: 10_800,
        due_day: 20,
        months: 12,
        amount: 129_600,
        after: None,
    };
    let cases = [
        ("гражданин 1960 года", with(Some(1960), Residency::Citizen), Health::Unclear(HealthUnclear::Elderly(65))),
        ("гражданин 1961 года", with(Some(1961), Residency::Citizen), Health::Unclear(HealthUnclear::TurnsAge(65))),
        ("резидент 1960 года", answered(1960, true), Health::Out(HealthOut::StatePays(65))),
        ("резидент 1961 года", answered(1961, true), Health::Unclear(HealthUnclear::TurnsAge(65))),
        (
            "иностранец с ответом «резидент»",
            profile(Person {
                resident: Some(true),
                ..with(Some(1950), Residency::ForeignWithStatus).person
            }),
            month,
        ),
        ("иностранец 1950 года", with(Some(1950), Residency::ForeignWithStatus), month),
        (
            "гражданин без года рождения",
            with(None, Residency::Citizen),
            Health::Unknown(Missing {
                birth_year: true,
                ..Missing::default()
            }),
        ),
    ];
    let mut wrong = Vec::new();
    for (case, data, expected) in cases {
        let year = if matches!(expected, Health::Month { .. }) { 2027 } else { 2026 };
        let got = annual(Some(&data), &[], year).health;
        if got != expected {
            wrong.push(format!("{case}: {got:?}"));
        }
    }
    // Нерезидент платит сам в любом возрасте: и уже в 65, и в год 65-летия.
    for (case, data) in [("1960", answered(1960, false)), ("1962", answered(1962, false))] {
        let got = annual(Some(&data), &[], 2027).health;
        if got != month {
            wrong.push(format!("нерезидент {case}: {got:?}"));
        }
    }
    // Взнос, который платит государство, ни в какой итог и срок не идёт.
    let data = answered(1960, true);
    let got = annual(Some(&data), &[], 2027).health;
    if got != Health::Out(HealthOut::StatePays(65)) {
        wrong.push(format!("резидент в 2027 году: {got:?}"));
    }
    let due = deadlines::between(books(Some(&data), &[], &[]), day(2026, 11, 2), day(2027, 12, 31));
    if due.iter().any(|deadline| matches!(deadline.kind, deadlines::Kind::HealthMonth { .. })) {
        wrong.push(format!("сроки взноса у того, за кого платит государство: {due:?}"));
    }
    assert!(wrong.is_empty(), "медстраховка и возраст: {wrong:#?}");
}

/// Год не на налоге с оборота: весь год общий режим — оборот прошлого года
/// больше 115 млн или доход по закрывающему коду 1 января, — итог не
/// посчитан, к бухгалтеру; ИП ещё не был — платить нечего; правил на год
/// нет — не посчитано.
#[test]
fn years_outside_the_turnover_regime_and_the_rules() {
    let mut wrong = Vec::new();
    let data = profile(person());
    let general = annual(Some(&data), &[income(day(2025, 6, 1), 116_000_000)], 2026);
    let total = Total::of(&general);
    if general.status != Status::General || total.complete || total.sum.is_some() {
        wrong.push(format!("общий режим: {:?}, {total:?}", general.status));
    }
    let coded = ProfileData {
        activities: vec![ActivityCode::parse("70.22.0").expect("код")],
        ..profile(person())
    };
    let by_code = annual(Some(&coded), &[income(day(2026, 1, 1), 1_000_000)], 2026);
    if by_code.status != Status::General || by_code.profit.amount() != Some(0) {
        wrong.push(format!("код 1 января: {:?}, {:?}", by_code.status, by_code.profit));
    }
    let later = profile(Person {
        registered: Some(day(2027, 2, 1)),
        ..person()
    });
    let not_yet = annual(Some(&later), &[], 2026);
    let total = Total::of(&not_yet);
    if not_yet.status != Status::NotYet || !total.complete || total.sum != Some(0) {
        wrong.push(format!("ещё не ИП: {:?}, {total:?}", not_yet.status));
    }
    let old = annual(Some(&data), &[], 2024);
    if old.profit != Fixed::NoRule || old.social != Social::NoRule || old.stamp != Stamp::NoRule || old.health != Health::NoRule {
        wrong.push(format!("2024 год: {old:?}"));
    }
    if annual(Some(&data), &[], 2025).health != Health::NoRule {
        wrong.push("медстраховка за 2025 год нашла правило".to_owned());
    }
    assert!(wrong.is_empty(), "вне режима и правил: {wrong:#?}");
}

/// Откладывать — с этого месяца по месяц срока: в день срока — всё сразу,
/// на следующий день — уже нечего; с 31 декабря — пять месяцев. Год
/// ближайшего срока — прошлый, пока его срок не прошёл и человек был ИП
/// уже в нём.
#[test]
fn saving_and_the_year_due_next() {
    let data = profile(person());
    let got = annual(Some(&data), &[income(day(2026, 3, 1), 1_000_000)], 2026);
    let mut wrong = Vec::new();
    for (today, expected) in [
        (day(2027, 4, 20), Some(Saving { months: 1, per_month: 132_000 })),
        (day(2027, 4, 21), None),
        (day(2026, 12, 31), Some(Saving { months: 5, per_month: 26_400 })),
        (day(2026, 1, 1), Some(Saving { months: 16, per_month: 8_250 })),
    ] {
        if saving(&got, today) != expected {
            wrong.push(format!("{today:?}: {:?}", saving(&got, today)));
        }
    }
    let new = Person {
        registered: Some(day(2027, 2, 1)),
        ..person()
    };
    for (today, person, expected) in [
        (day(2027, 4, 20), Some(person()), 2026),
        (day(2027, 4, 21), Some(person()), 2027),
        (day(2026, 10, 20), Some(person()), 2026),
        (day(2027, 3, 1), Some(new), 2027),
        (day(2027, 3, 1), None, 2026),
    ] {
        if due_year(today, person.as_ref()) != expected {
            wrong.push(format!("год на {today:?}: {}", due_year(today, person.as_ref())));
        }
    }
    assert!(wrong.is_empty(), "откладывать: {wrong:#?}");
}

/// Замер (Правило 1), а не проверка: годовые платежи последнего года
/// пятилетней книги — 1 600 записей дохода в год. Окно считает год на
/// каждое изменение данных — карточкой на главном и экраном.
/// `cargo test -p kvartik-core --release --lib annual_timing -- --ignored --nocapture`.
#[test]
#[ignore = "замер, а не проверка"]
fn annual_timing() {
    let mut operations = Vec::new();
    for year in 2025..=2029_u16 {
        for n in 0..1600_u16 {
            let date = day(year, (n % 12 + 1) as u8, (n % 28 + 1) as u8);
            operations.push(income(date, 10_000 + i64::from(n)));
        }
    }
    let data = profile(person());
    let started = std::time::Instant::now();
    let got = annual(Some(&data), &operations, 2029);
    let elapsed = started.elapsed();
    println!("год 2029, {} записей: {elapsed:?}, к сроку {:?}", operations.len(), Total::of(&got).sum);
}
