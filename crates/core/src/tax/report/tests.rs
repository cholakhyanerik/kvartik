//! Тесты чисел для расчёта. Номера полей выписаны из текста приказа ՊԵԿ
//! 299-Ն в редакции 1430-Ն и 420-Ն (arlis 205434 и 201683, сверено
//! 2026-10-22), а числа расчёта посчитаны вручную, как в тестах налога
//! квартала (`turnover/tests.rs`): код, записавший себе эталон, проверял бы
//! сам себя (Правило 2).

use super::*;
use crate::model::document::Document;
use crate::model::ledger::{Nature, Operation, OperationData, Proof};
use crate::model::money::{Currency, ExchangeRate, Money, RateSource};
use crate::model::profile::ProfileData;
use crate::model::record::Timestamp;
use crate::model::text::Line as Text;

fn day(year: u16, month: u8, day: u8) -> Date {
    Date::new(year, month, day).expect("в тесте — существующая дата")
}

fn quarter(year: u16, number: u8) -> Quarter {
    Quarter::new(year, number).expect("в тесте — существующий квартал")
}

fn code(text: &str) -> ActivityCode {
    ActivityCode::parse(text).expect("в тесте — годный код")
}

fn percent(text: &str) -> Percent {
    Percent::parse(text).expect("в тесте — годный процент")
}

fn at(millis: i64) -> Timestamp {
    Timestamp::from_unix_millis(millis)
}

/// Профиль с этими кодами.
fn profile(codes: &[&str]) -> ProfileData {
    ProfileData {
        activities: codes.iter().map(|text| code(text)).collect(),
        ..ProfileData::default()
    }
}

/// Запись книги — доход или расход.
fn operation(data: OperationData) -> Operation {
    Operation::create(data, at(1_760_000_000_000)).expect("в тесте — годная запись")
}

/// Доход в драмах без документа в программе.
fn income(date: Date, drams: i64, activity: Option<&str>) -> Operation {
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
        activity: activity.map(code),
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
        ..income(date, 1, None).data().clone()
    })
}

/// Расход по «հաշիվ վավերագիր»: с видом деятельности или общий.
fn expense(date: Date, drams: i64, activity: Option<&str>) -> Operation {
    operation(OperationData {
        flow: Flow::Expense,
        date,
        paid: None,
        amount: Money::new(drams, Currency::Amd),
        rate: None,
        text: Text::new("Расход").expect("текст"),
        client: None,
        supplier: Text::default(),
        proof: Some(Proof::SettlementInvoice),
        nature: Some(Nature::Ordinary),
        activity: activity.map(code),
        attachments: Vec::new(),
    })
}

fn books<'a>(profile: &'a ProfileData, operations: &'a [Operation], closings: &'a [Closing]) -> Books<'a> {
    let documents: &[Document] = &[];
    Books {
        documents,
        operations,
        profile: Some(profile),
        closings,
        openings: &[],
        unreadable: 0,
    }
}

/// Налог квартала по этим записям.
fn tax(profile: &ProfileData, operations: &[Operation], closings: &[Closing], q: Quarter) -> QuarterTax {
    turnover::quarter(books(profile, operations, closings), q)
}

/// Числа — или почему их нет.
fn ready(tax: &QuarterTax, closing: Option<&Closing>) -> Ready {
    report(tax, closing).unwrap_or_else(|missing| panic!("чисел нет: {missing:?}"))
}

fn drams(value: i64) -> Value {
    Value::Drams(value)
}

fn rate(text: &str) -> Value {
    Value::Percent(percent(text))
}

/// Поля раздела: номер и число.
fn fields(section: &Section) -> Vec<(&str, Value)> {
    section.fields.iter().map(|field| (field.number.as_str(), field.value)).collect()
}

/// Дизайнер (74.10.0) в III квартале 2026 года: доход 1 000 000 ֏, расход
/// с видом деятельности 100 000 ֏ и общий 950 000 ֏. Налог 10 % —
/// 100 000; вычет 6 % от 1 050 000 — 63 000; 100 000 − 63 000 = 37 000
/// меньше предела 4,5 % — 45 000: налог 45 000, перенос 63 000 − (100 000 −
/// 45 000) = 8 000 (приказ 299-Ն, прил. 1, п. 49–60).
fn designer() -> (ProfileData, Vec<Operation>) {
    let operations = vec![
        income(day(2026, 7, 15), 1_000_000, None),
        expense(day(2026, 8, 1), 100_000, Some("74.10.0")),
        expense(day(2026, 8, 2), 950_000, None),
    ];
    (profile(&["74.10.0"]), operations)
}

/// Строка с вычетом заполняет свой раздел полями 9.1–9.11 — номерами
/// приказа, — строку сводки 21 со ставкой «X», итог 22 и оборот 27.
/// Прямые расходы и сбытовые с административными книга не различает: при
/// расходах у 9.4.1 и 9.4.2 числа нет, а 9.4 — их сумма.
#[test]
fn a_line_with_deduction_fills_its_section() {
    let (profile, operations) = designer();
    let got = ready(&tax(&profile, &operations, &[], quarter(2026, 3)), None);
    let numbers = &got.numbers;
    let [section] = numbers.sections.as_slice() else {
        panic!("разделы: {:?}", numbers.sections);
    };
    assert_eq!((section.number.as_str(), section.kind), ("9", IncomeKind::Other));
    assert_eq!(
        fields(section),
        [
            ("9.1", drams(1_000_000)),
            ("9.2", rate("10")),
            ("9.3", drams(100_000)),
            ("9.4.1", Value::Unknown),
            ("9.4.2", Value::Unknown),
            ("9.4", drams(1_050_000)),
            ("9.5", rate("6")),
            ("9.6", drams(63_000)),
            ("9.7", drams(0)),
            ("9.8", drams(63_000)),
            ("9.9", drams(45_000)),
            ("9.10", drams(45_000)),
            ("9.11", drams(8_000)),
        ]
    );
    assert_eq!(
        numbers.rows,
        [Row {
            number: "21".to_owned(),
            kind: IncomeKind::Other,
            income: 1_000_000,
            rate: None,
            tax: 45_000,
        }]
    );
    assert_eq!(
        numbers.total,
        Total {
            number: "22".to_owned(),
            income: 1_000_000,
            tax: 45_000,
        }
    );
    assert_eq!(
        numbers.turnover,
        Turnover {
            number: "27".to_owned(),
            income: 1_000_000,
        }
    );
    // Квартал — первый с записями: перенос из расчётов до программы
    // неизвестен, и окно скажет это у 9.7.
    assert!(matches!(got.origin, Origin::Calculated { carry_unknown: Some(_) }));
    assert_eq!(got.rules, rules::armenia().version());
}

/// Строки без вычета — только в сводке, со своей ставкой в [Բ]: IT — 19,
/// прочая деятельность — 21 со своим разделом 9; строки — по номеру, итог —
/// их сумма. Общие расходы 400 000 ֏ делятся по доле дохода: дизайну —
/// 100 000, вычет 6 000, налог 94 000; IT — 1 % от 3 000 000.
#[test]
fn lines_without_deduction_have_only_a_row() {
    let profile = profile(&["62.01.0", "74.10.0"]);
    let operations = [
        income(day(2026, 8, 10), 1_000_000, Some("74.10.0")),
        income(day(2026, 7, 10), 3_000_000, Some("62.01.0")),
        expense(day(2026, 9, 1), 400_000, None),
    ];
    let numbers = ready(&tax(&profile, &operations, &[], quarter(2026, 3)), None).numbers;
    let sections: Vec<&str> = numbers.sections.iter().map(|section| section.number.as_str()).collect();
    assert_eq!(sections, ["9"]);
    let rows: Vec<(&str, i64, Option<Percent>, i64)> = numbers
        .rows
        .iter()
        .map(|row| (row.number.as_str(), row.income, row.rate, row.tax))
        .collect();
    assert_eq!(
        rows,
        [("19", 3_000_000, Some(percent("1")), 30_000), ("21", 1_000_000, None, 94_000)]
    );
    assert_eq!((numbers.total.income, numbers.total.tax), (4_000_000, 124_000));
    assert_eq!(numbers.turnover.income, 4_000_000);
}

/// Место каждой строки налога в форме — по тексту приказа: разделы 5, 6, 7
/// и 9 — у торговли, производства, общепита и прочей деятельности, строки
/// сводки 11–21 без 17 (общепит вне Еревана до 2026 года — его программа
/// не знает), итог — 22, оборот — 27. Раздел есть ровно у строк с вычетом
/// расходов по файлу правил: иначе расчёт и форма разошлись бы.
#[test]
fn every_kind_has_its_place_in_the_form() {
    let expected = [
        (IncomeKind::Trade, Some("5"), "11"),
        (IncomeKind::Recyclables, None, "12"),
        (IncomeKind::Newspapers, None, "13"),
        (IncomeKind::Production, Some("6"), "14"),
        (IncomeKind::Rent, None, "15"),
        (IncomeKind::Catering, Some("7"), "16"),
        (IncomeKind::CateringOther, None, "18"),
        (IncomeKind::HighTech, None, "19"),
        (IncomeKind::Assets, None, "20"),
        (IncomeKind::Other, Some("9"), "21"),
    ];
    let mut wrong = Vec::new();
    for q in [quarter(2025, 1), quarter(2026, 4), quarter(2031, 4)] {
        let Some(form) = form(q) else {
            wrong.push(format!("{q:?}: формы нет"));
            continue;
        };
        for (kind, section, row) in expected {
            let place = form.line(kind).map(|line| (line.section.as_deref(), line.row.as_str()));
            if place != Some((section, row)) {
                wrong.push(format!("{q:?} {kind:?}: {place:?}"));
            }
            let deduction = turnover::rate(kind, q.first_day()).map(|rate| rate.deduction.is_some());
            if deduction.is_some_and(|deduction| deduction != section.is_some()) {
                wrong.push(format!("{q:?} {kind:?}: вычет {deduction:?}, раздел {section:?}"));
            }
        }
        if (form.total.as_str(), form.turnover.as_str()) != ("22", "27") {
            wrong.push(format!("{q:?}: итог {} и оборот {}", form.total, form.turnover));
        }
    }
    if form(quarter(2024, 4)).is_some() {
        wrong.push("форма 2024 года: номера по редакции 1430-Ն — для периодов с 2025 года".to_owned());
    }
    assert!(wrong.is_empty(), "места строк в форме: {wrong:#?}");
}

/// Числа полей сходятся по формулам самого приказа (299-Ն, прил. 1, п. 9,
/// 11, 12, 14, 16, 17, 63, 64): x.3 = x.1 × x.2, x.4 = x.4.1 + x.4.2 (при
/// расходах у слагаемых числа нет, без расходов оба — ноль), x.6 = x.4 ×
/// x.5, x.8 = x.6 + x.7, x.10 и x.11 — по пределу x.9, [Գ] строки
/// с разделом — её x.10, итог — сумма строк. Поменяй местами два числа
/// расчёта в разделе — и формула не сойдётся.
#[test]
fn the_order_formulas_hold() {
    let (designer, design_books) = designer();
    let carry = [
        income(day(2026, 7, 15), 1_000_000, None),
        expense(day(2026, 8, 1), 950_000, None),
        income(day(2026, 10, 15), 1_000_000, None),
    ];
    let idle = [expense(day(2026, 2, 10), 200_000, None), income(day(2026, 4, 20), 500_000, None)];
    let design = profile(&["74.10.0"]);
    let cases = [
        ("дизайн, предел", tax(&designer, &design_books, &[], quarter(2026, 3))),
        ("перенос в IV квартал", tax(&design, &carry, &[], quarter(2026, 4))),
        ("квартал без дохода", tax(&design, &idle, &[], quarter(2026, 1))),
        ("квартал после него", tax(&design, &idle, &[], quarter(2026, 2))),
    ];
    let mut wrong = Vec::new();
    for (case, tax) in cases {
        let numbers = ready(&tax, None).numbers;
        assert!(!numbers.sections.is_empty(), "{case}: разделов нет");
        for section in &numbers.sections {
            let value = |suffix: &str| {
                let number = format!("{}.{suffix}", section.number);
                section.fields.iter().find(|field| field.number == number).map(|field| field.value)
            };
            let amount = |suffix: &str| match value(suffix) {
                Some(Value::Drams(drams)) => drams,
                other => panic!("{case}: поле {suffix} — {other:?}"),
            };
            let share = |suffix: &str| match value(suffix) {
                Some(Value::Percent(percent)) => percent,
                other => panic!("{case}: поле {suffix} — {other:?}"),
            };
            let (gross, total, floor) = (amount("3"), amount("8"), amount("9"));
            let split = match (value("4.1"), value("4.2")) {
                (Some(Value::Unknown), Some(Value::Unknown)) => amount("4") > 0,
                (Some(Value::Drams(direct)), Some(Value::Drams(overhead))) => {
                    amount("4") == 0 && direct == 0 && overhead == 0
                }
                _ => false,
            };
            let formulas = [
                ("x.3 = x.1 × x.2", gross == share("2").of(amount("1"))),
                ("x.4 = x.4.1 + x.4.2", split),
                ("x.6 = x.4 × x.5", amount("6") == share("5").of(amount("4"))),
                ("x.8 = x.6 + x.7", total == amount("6") + amount("7")),
                ("x.10", amount("10") == if gross - total < floor { floor } else { gross - total }),
                ("x.11", amount("11") == if gross - total < floor { total - (gross - floor) } else { 0 }),
            ];
            for (formula, holds) in formulas {
                if !holds {
                    wrong.push(format!("{case}, раздел {}: {formula}", section.number));
                }
            }
            let row = numbers.rows.iter().find(|row| row.kind == section.kind);
            if row.map(|row| row.tax) != Some(amount("10")) {
                wrong.push(format!("{case}: [Գ] строки раздела {} — не его x.10", section.number));
            }
        }
        let (income, sum): (i64, i64) = numbers.rows.iter().map(|row| (row.income, row.tax)).fold((0, 0), |(a, b), (c, d)| (a + c, b + d));
        if (numbers.total.income, numbers.total.tax, numbers.turnover.income) != (income, sum, income) {
            wrong.push(format!("{case}: итог {:?}, оборот {:?}", numbers.total, numbers.turnover));
        }
    }
    assert!(wrong.is_empty(), "формулы приказа: {wrong:#?}");
}

/// Закрытый квартал отдаёт числа своего снимка и версию его правил, даже
/// если данные потом поменялись, — и говорит, что расчёт с ним разошёлся.
#[test]
fn a_closed_quarter_reports_its_snapshot() {
    let (profile, mut operations) = designer();
    let q = quarter(2026, 3);
    let before = tax(&profile, &operations, &[], q);
    let closing = Closing::close(&before, day(2026, 10, 5), at(1_760_000_000_000)).expect("квартал закрывается");
    let closings = [closing.clone()];
    let same = ready(&tax(&profile, &operations, &closings, q), Some(&closing));
    assert_eq!(same.origin, Origin::Closed { changed: false });
    assert_eq!(same.numbers.total.tax, 45_000);
    assert_eq!(same.notes(Some(day(2026, 10, 5))), [Key::ReportClosed]);
    // Доли по кодам снимок не хранит: у закрытого квартала их нет и в тексте.
    assert!(!same.has_activities());
    let shares = activities(books(&profile, &operations, &closings), q);
    assert!(!shares.shares.is_empty());
    let closed_text = text(Lang::Ru, &same, Some(&shares), None);
    assert!(!closed_text.contains(t(Lang::Ru, Key::ReportActivities)), "{closed_text}");
    operations.push(income(day(2026, 9, 30), 500_000, None));
    let changed = ready(&tax(&profile, &operations, &closings, q), Some(&closing));
    assert_eq!(changed.origin, Origin::Closed { changed: true });
    assert_eq!((changed.numbers.total.income, changed.numbers.total.tax), (1_000_000, 45_000));
    assert_eq!(changed.notes(None), [Key::ReportClosed, Key::ReportClosedChanged]);
    // Снимок по прежним правилам — его версия, а не нынешняя.
    let old: Closing = serde_json::from_str(&serde_json::to_string(&closing).expect("снимок пишется").replace(
        &format!(r#""rules":"{}""#, rules::armenia().version()),
        r#""rules":"2026.2""#,
    ))
    .expect("снимок читается");
    assert_eq!(ready(&before, Some(&old)).rules.to_string(), "2026.2");
}

/// Чисел нет, пока квартал посчитан не целиком, у квартала с уходом
/// с режима и у квартала без формы — и это разные ответы.
#[test]
fn no_numbers_without_a_whole_calculation() {
    let design = profile(&["74.10.0"]);
    let unrated = [usd_income(day(2026, 7, 1), 5_000, None)];
    assert_eq!(report(&tax(&design, &unrated, &[], quarter(2026, 3)), None), Err(Missing::Incomplete));
    let threshold = [
        income(day(2026, 2, 10), 60_000_000, None),
        income(day(2026, 5, 10), 50_000_000, None),
        income(day(2026, 8, 10), 10_000_000, None),
    ];
    assert_eq!(report(&tax(&design, &threshold, &[], quarter(2026, 3)), None), Err(Missing::Left));
    assert_eq!(report(&tax(&design, &[], &[], quarter(2024, 4)), None), Err(Missing::NoForm));
    // Квартал без дохода и расходов — нули в итоге, а не «чисел нет».
    let empty = ready(&tax(&design, &[], &[], quarter(2026, 3)), None);
    assert!(empty.numbers.sections.is_empty() && empty.numbers.rows.is_empty());
    assert_eq!((empty.numbers.total.income, empty.numbers.total.tax, empty.numbers.turnover.income), (0, 0, 0));
}

/// Квартал ещё идёт — так и сказано; закончился — нет.
#[test]
fn an_ongoing_quarter_says_so() {
    let (profile, operations) = designer();
    let got = ready(&tax(&profile, &operations, &[], quarter(2026, 3)), None);
    assert_eq!(got.notes(Some(day(2026, 9, 30))), [Key::ReportOngoing, Key::TaxCarryUnknown]);
    assert_eq!(got.notes(Some(day(2026, 10, 1))), [Key::TaxCarryUnknown]);
}

/// Доли по кодам — сотыми процента, по убыванию и ровно 100 % в сумме:
/// 2 : 1 — 66,67 и 33,33; три равных — 33,34 первому по коду и по 33,33
/// остальным. Код без буквы раздела — с ней. Доход без кода при нескольких
/// кодах профиля — долей нет вовсе.
#[test]
fn shares_add_up_to_a_hundred() {
    let q = quarter(2026, 3);
    let single = profile(&["74.10.0"]);
    let operations = [income(day(2026, 7, 1), 1_234_567, None)];
    let got = activities(books(&single, &operations, &[]), q);
    assert_eq!(got.shares, [Share { code: code("74.10.0"), percent: percent("100") }]);
    assert_eq!(lettered(code("74.10.0")), "M74.10.0");

    let two = profile(&["62.01.0", "74.10.0"]);
    let operations = [
        income(day(2026, 7, 1), 1_000, Some("74.10.0")),
        income(day(2026, 8, 1), 2_000, Some("62.01.0")),
        expense(day(2026, 8, 2), 5_000, Some("62.01.0")),
    ];
    let got = activities(books(&two, &operations, &[]), q);
    assert_eq!(
        got.shares,
        [
            Share { code: code("62.01.0"), percent: percent("66.67") },
            Share { code: code("74.10.0"), percent: percent("33.33") },
        ]
    );

    let three = profile(&["62.09.0", "62.01.0", "62.02.1"]);
    let operations = [
        income(day(2026, 7, 1), 1_000, Some("62.09.0")),
        income(day(2026, 7, 2), 1_000, Some("62.02.1")),
        income(day(2026, 7, 3), 1_000, Some("62.01.0")),
    ];
    let got = activities(books(&three, &operations, &[]), q);
    assert_eq!(
        got.shares,
        [
            Share { code: code("62.01.0"), percent: percent("33.34") },
            Share { code: code("62.02.1"), percent: percent("33.33") },
            Share { code: code("62.09.0"), percent: percent("33.33") },
        ]
    );

    let same_row = profile(&["62.01.0", "62.02.1"]);
    let operations = [income(day(2026, 7, 1), 1_000, Some("62.01.0")), income(day(2026, 7, 2), 1_000, None)];
    let got = activities(books(&same_row, &operations, &[]), q);
    assert_eq!(got, Activities { shares: Vec::new(), unknown: 1 });
    // Налогу код не нужен: оба кода — строка 8.
    assert!(ready(&tax(&same_row, &operations, &[], q), None).numbers.rows.len() == 1);
    assert_eq!(activities(books(&single, &[], &[]), q), Activities::default());
}

/// Текст «Скопировать всё»: каждое поле — номер, слова и число цифрами,
/// как его вписывают в форму; ставки — как пишет приказ; таблица по видам
/// деятельности и версия правил — в конце. И на каждом языке строки
/// записаны по его правилам (`i18n::flaw`).
#[test]
fn the_text_lists_every_field() {
    let (profile, operations) = designer();
    let q = quarter(2026, 3);
    let got = ready(&tax(&profile, &operations, &[], q), None);
    let shares = activities(books(&profile, &operations, &[]), q);
    let text = text(Lang::Ru, &got, Some(&shares), Some(day(2026, 10, 2)));
    let lines: Vec<&str> = text.lines().collect();
    let version = rules::armenia().version();
    let source = format!("Посчитано в Kvartik по правилам {version}\u{A0}— сверьте с бухгалтером.");
    let mut missing = Vec::new();
    for expected in [
        "9. Прочая деятельность",
        "9.1 Доход за квартал: 1000000",
        "9.2 Ставка налога: 10%",
        "9.4.1 Прямые расходы: разделите сами",
        "9.4.2 Сбытовые и административные расходы: разделите сами",
        "9.4 Расходы всего: 1050000",
        "9.5 Процент от расходов в вычет: 6%",
        "9.10 Налог к уплате: 45000",
        "9.11 Перенос на следующие кварталы: 8000",
        "21. Прочая деятельность",
        "21 Доход [Ա]: 1000000",
        "21 Налог [Գ]: 45000",
        "22. Итого",
        "22 Налог [Գ]: 45000",
        "27 Оборот по всем видам деятельности: 1000000",
        "Доход по видам деятельности",
        "M74.10.0 доля дохода: 100%",
        source.as_str(),
    ] {
        if !lines.contains(&expected) {
            missing.push(expected);
        }
    }
    assert!(missing.is_empty(), "нет в тексте: {missing:#?}\n{text}");
    assert!(!text.contains("21 Ставка"), "у строки с разделом ставки нет — «X»:\n{text}");
    let mut wrong = Vec::new();
    for lang in Lang::ALL {
        let text = super::text(lang, &got, Some(&shares), None);
        for line in text.lines().filter(|line| !line.is_empty()) {
            if let Some(flaw) = i18n::flaw(lang, line) {
                wrong.push(format!("{lang:?}: {flaw} — «{line}»"));
            }
        }
    }
    assert!(wrong.is_empty(), "текст не по правилам языка: {wrong:#?}");
}

/// Число в форму — цифрами, без пробелов и знака драма; процент — с точкой;
/// числа, которого программа не знает, нет вовсе.
#[test]
fn plain_numbers_are_what_the_form_takes() {
    assert_eq!(Value::Drams(1_234_567).plain().as_deref(), Some("1234567"));
    assert_eq!(Value::Drams(0).plain().as_deref(), Some("0"));
    assert_eq!(Value::Percent(percent("4.5")).plain().as_deref(), Some("4.5%"));
    assert_eq!(Value::Percent(percent("66.67")).plain().as_deref(), Some("66.67%"));
    assert_eq!(Value::Unknown.plain(), None);
}

/// Торговец с одним кодом (47.91.0, строка «торговля» выбрана в профиле):
/// форма расхода вид деятельности не спрашивает, и покупка товара на
/// 6 000 000 ֏ — общий расход. Книга не знает, товар это (5.4.1) или
/// сбытовой расход (5.4.2), — у обоих полей числа нет, а 5.4 — вся сумма.
/// Налог: 10 % от 10 000 000 — 1 000 000; вычет 9,5 % от 6 000 000 —
/// 570 000; 430 000 не меньше предела 1 % — 100 000 (приказ, п. 6–17).
/// В квартале без расходов оба поля — ноль: делить нечего.
#[test]
fn expenses_are_not_split_into_direct_and_overhead() {
    let trader = ProfileData {
        kinds: BTreeMap::from([(code("47.91.0"), IncomeKind::Trade)]),
        ..profile(&["47.91.0"])
    };
    let operations = [
        income(day(2026, 7, 10), 10_000_000, None),
        expense(day(2026, 7, 20), 6_000_000, None),
        income(day(2026, 10, 10), 1_000_000, None),
    ];
    let third = ready(&tax(&trader, &operations, &[], quarter(2026, 3)), None).numbers;
    let [section] = third.sections.as_slice() else {
        panic!("разделы: {:?}", third.sections);
    };
    assert_eq!(
        fields(section),
        [
            ("5.1", drams(10_000_000)),
            ("5.2", rate("10")),
            ("5.3", drams(1_000_000)),
            ("5.4.1", Value::Unknown),
            ("5.4.2", Value::Unknown),
            ("5.4", drams(6_000_000)),
            ("5.5", rate("9.5")),
            ("5.6", drams(570_000)),
            ("5.7", drams(0)),
            ("5.8", drams(570_000)),
            ("5.9", drams(100_000)),
            ("5.10", drams(430_000)),
            ("5.11", drams(0)),
        ]
    );
    assert_eq!(third.rows.iter().map(|row| row.number.as_str()).collect::<Vec<_>>(), ["11"]);
    let fourth = ready(&tax(&trader, &operations, &[], quarter(2026, 4)), None).numbers;
    let direct: Vec<(&str, Value)> = fourth.sections[0]
        .fields
        .iter()
        .filter(|field| matches!(field.what, What::Direct | What::Overhead))
        .map(|field| (field.number.as_str(), field.value))
        .collect();
    assert_eq!(direct, [("5.4.1", drams(0)), ("5.4.2", drams(0))]);
}
