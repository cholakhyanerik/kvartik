//! Тесты примера расчёта. Ожидаемые числа посчитаны вручную по тексту закона,
//! а не записаны кодом (Правило 2): IT — 1 % (НК, ст. 258, ч. 1, строка 8);
//! прочая деятельность — 10 % минус 6 % расходов, но не меньше 4,5 % оборота
//! (там же, строка 10, ч. 5 и 7; форма 299-Ն, п. 9–17); налог на прибыль
//! и соцплатёж — 5 000 ֏ за месяц (НК, ст. 125, ч. 3; ՀՕ-244-Ն, ст. 6, ч. 2,
//! п. 3); гербовый сбор — ступень шкалы по обороту года (ՀՕ-245-Ն, ст. 12,
//! ч. 1, п. 2); взнос медстраховки за 2026 год — двенадцать месячных по
//! 10 800 ֏ при обороте прошлого года больше 2 400 000 ֏ (ՀՕ-459-Ն, ст. 13,
//! ч. 1; ст. 45, ч. 1, п. 5; ст. 46, ч. 10). Посчитал агент 2026-11-05.
//! Здесь проверяется прежде всего сборка книги примера: сам расчёт держат
//! тесты `turnover` и `annual`.

use super::*;

fn day(year: u16, month: u8, day: u8) -> Date {
    Date::new(year, month, day).expect("в тесте — существующая дата")
}

/// «Сегодня» тестов: IV квартал 2026 года, годовые платежи — за 2026.
fn today() -> Date {
    day(2026, 11, 5)
}

fn input(kind: IncomeKind, turnover: i64, expenses: i64) -> Input {
    Input {
        kind,
        turnover,
        expenses,
        birth_year: Some(1990),
        residency: Some(Residency::Citizen),
        pension_joined: None,
        resident: None,
    }
}

fn raw(kind: IncomeKind, turnover: &str) -> Raw {
    Raw {
        kind: kind_id(kind),
        turnover: turnover.to_owned(),
        birth: "1990".to_owned(),
        residency: residency_id(Residency::Citizen),
        ..Raw::default()
    }
}

/// IT по списку: 1 % от 1 000 000 ֏ — 10 000 ֏, без вычета.
#[test]
fn high_tech_pays_one_percent() {
    let got = example(&input(IncomeKind::HighTech, 1_000_000, 0), today());
    assert_eq!(got.quarter.known(), Some(10_000));
    assert_eq!(got.quarter.lines.len(), 1);
    let line = &got.quarter.lines[0];
    assert_eq!((line.kind, line.income, line.gross, line.deduction), (IncomeKind::HighTech, 1_000_000, 10_000, None));
}

/// Прочая деятельность: 100 000 − 6 % от 300 000 = 82 000, больше предела
/// 4,5 % (45 000) — переноса нет, и квартал за кварталом налог тот же.
#[test]
fn other_activities_deduct_expenses() {
    let got = example(&input(IncomeKind::Other, 1_000_000, 300_000), today());
    assert_eq!(got.quarter.known(), Some(82_000));
    let deduction = got.quarter.lines[0].deduction.expect("у строки 10 есть вычет");
    assert_eq!(
        (deduction.expenses, deduction.current, deduction.carry_in, deduction.floor),
        (300_000, 18_000, 0, 45_000)
    );
}

/// Расходы большие: 100 000 − 57 000 = 43 000 меньше предела 45 000 —
/// налог 45 000, а каждый квартал переносит дальше 2 000 (ст. 258, ч. 7).
/// IV квартал 2026 — восьмой с I квартала 2025: перенос в него — 7 × 2 000.
#[test]
fn the_floor_holds_and_carries_over() {
    let got = example(&input(IncomeKind::Other, 1_000_000, 950_000), today());
    assert_eq!(got.quarter.known(), Some(45_000));
    let deduction = got.quarter.lines[0].deduction.expect("у строки 10 есть вычет");
    assert_eq!((deduction.current, deduction.carry_in), (57_000, 14_000));
    assert_eq!(got.quarter.carry_unknown, None, "перенос до программы — «не было»");
}

/// Аренда — строка самого дохода: 10 % без вычета (ст. 258, ч. 1, строка 5;
/// ст. 260, ч. 1), и расходов форма не спрашивает.
#[test]
fn rent_is_its_own_line_without_deduction() {
    let got = example(&input(IncomeKind::Rent, 1_000_000, 0), today());
    assert_eq!(got.quarter.known(), Some(100_000));
    assert_eq!(got.quarter.lines[0].kind, IncomeKind::Rent);
    assert!(!has_deduction(IncomeKind::Rent, today()));
    assert!(has_deduction(IncomeKind::Other, today()));
}

/// Торговля, производство и общепит — строка, выбранная коду: у каждой своя.
#[test]
fn chosen_lines_reach_the_quarter() {
    let mut wrong = Vec::new();
    for kind in KINDS {
        let got = example(&input(kind, 1_000_000, 0), today());
        if got.quarter.lines.iter().map(|line| line.kind).collect::<Vec<_>>() != [kind] || got.quarter.known().is_none() {
            wrong.push(format!("{kind:?}: {:?}", got.quarter.lines));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// Гражданин 1990 года рождения, оборот 4 000 000 ֏ в год: налог на прибыль
/// и соцплатёж — 12 × 5 000, гербовый сбор — ступень «до 12 000 000» шкалы
/// 2026 года — 12 000 (ՀՕ-245-Ն в редакции ՀՕ-326-Ն и ՀՕ-477-Ն, ст. 12,
/// ч. 1, п. 2), всего 132 000 до срока; взнос медстраховки 2026 года —
/// годовой ставкой: 12 × 10 800 = 129 600, оборот 2025 года больше
/// 2 400 000.
#[test]
fn annual_payments_of_a_citizen() {
    let got = example(&input(IncomeKind::HighTech, 1_000_000, 0), today()).annual;
    assert_eq!(got.year, 2026);
    assert_eq!(got.profit.amount(), Some(60_000));
    assert_eq!(got.social.amount(), Some(60_000));
    assert_eq!(got.stamp.amount(), Some(12_000));
    assert!(matches!(got.health, Health::Year { amount: 129_600, .. }), "{:?}", got.health);
    let total = Total::of(&got);
    assert_eq!((total.sum, total.complete), (Some(132_000), true));
}

/// Иностранец без статуса проживания: соцплатежа, гербового сбора и взноса
/// медстраховки нет (ՀՕ-244-Ն, ст. 5, ч. 3; ՀՕ-245-Ն, ст. 11, ч. 2, п. 3;
/// ՀՕ-459-Ն, ст. 11, ч. 1).
#[test]
fn a_foreigner_without_status_pays_profit_tax_only() {
    let got = example(
        &Input {
            residency: Some(Residency::ForeignWithoutStatus),
            ..input(IncomeKind::HighTech, 1_000_000, 0)
        },
        today(),
    )
    .annual;
    assert_eq!(got.profit.amount(), Some(60_000));
    assert!(matches!(got.social, Social::Out(_)), "{:?}", got.social);
    assert_eq!(got.stamp, Stamp::Exempt);
    assert!(matches!(got.health, Health::Out(_)), "{:?}", got.health);
}

/// Порог: 40 000 000 ֏ в квартал — 160 000 000 за прошлый год, больше
/// 115 000 000: весь этот год — не налог с оборота (НК, ст. 254, ч. 2), и
/// ответ говорит это вместо чисел.
#[test]
fn over_the_threshold_goes_to_an_accountant() {
    let got = example(&input(IncomeKind::HighTech, 40_000_000, 0), today());
    assert_eq!(got.quarter.known(), None);
    let text = answer(Lang::Ru, &raw(IncomeKind::HighTech, "40 000 000"), today());
    let over = fill(t(Lang::Ru, Key::CalcOverThreshold), &[&amd(Lang::Ru, 115_000_000)]);
    assert!(text.blocks.iter().any(|block| block.text == over), "{:#?}", text.blocks);
    assert!(!text.blocks.iter().any(|block| block.kind == BlockKind::Sum));
}

/// Ответ на трёх языках: заголовок с кварталом, сумма налога, годовые
/// платежи и допущения — без пустых блоков.
#[test]
fn the_answer_speaks_every_language() {
    let mut wrong = Vec::new();
    for lang in Lang::ALL {
        let got = answer(lang, &raw(IncomeKind::HighTech, &i18n::amount(lang, Money::new(1_000_000, Currency::Amd))), today());
        let head = fill(t(lang, Key::CalcQuarterHead), &[&i18n::quarter(lang, today().quarter())]);
        let checks = [
            ("заголовок", got.blocks.first().is_some_and(|block| block.text == head)),
            ("сумма налога", got.blocks.iter().any(|block| block.kind == BlockKind::Sum && block.text == amd(lang, 10_000))),
            ("допущения", got.blocks.last().is_some_and(|block| block.text == t(lang, Key::CalcAssume))),
            ("без пустых", got.blocks.iter().all(|block| !block.text.trim().is_empty())),
            ("без ошибок", got.errors.is_empty()),
        ];
        for (what, ok) in checks {
            if !ok {
                wrong.push(format!("{lang:?}: {what} — {:#?}", got.blocks));
            }
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// Пустой оборот — подсказка без ошибки; не число и меньше нуля — ошибка
/// поля словами, а чисел нет.
#[test]
fn bad_fields_are_named() {
    let prompt = vec![Block::new(BlockKind::Note, t(Lang::Ru, Key::CalcPrompt))];
    let empty = answer(Lang::Ru, &raw(IncomeKind::HighTech, " "), today());
    assert_eq!((empty.errors.len(), &empty.blocks), (0, &prompt));
    let letters = answer(Lang::Ru, &raw(IncomeKind::HighTech, "abc"), today());
    assert!(letters.errors.get("turnover").is_some_and(|text| !text.is_empty()));
    assert_eq!(letters.blocks, prompt);
    let negative = answer(Lang::Ru, &raw(IncomeKind::HighTech, "-5"), today());
    assert_eq!(negative.errors.get("turnover").map(String::as_str), Some(t(Lang::Ru, Key::CalcNotNegative)));
    let year = answer(Lang::Ru, &Raw { birth: "19".to_owned(), ..raw(IncomeKind::HighTech, "1 000") }, today());
    assert_eq!(year.errors.get("birth").map(String::as_str), Some(t(Lang::Ru, Key::BirthYearInvalid)));
}

/// Поля, которые нужны не всегда, — по тем же условиям, что у профиля окна:
/// расходы — у строки с вычетом; пенсия — родившемуся до 1974 года;
/// резидентство — гражданину, которому до конца следующего года исполняется
/// 65 (ՀՕ-459-Ն, ст. 11, ч. 2, п. 2).
#[test]
fn fields_follow_the_rules() {
    let fields = |kind: IncomeKind, birth: &str, residency: Residency| {
        answer(
            Lang::Ru,
            &Raw {
                birth: birth.to_owned(),
                residency: residency_id(residency),
                ..raw(kind, "1 000 000")
            },
            today(),
        )
        .fields
    };
    let citizen = Residency::Citizen;
    assert_eq!(fields(IncomeKind::HighTech, "1990", citizen), Fields::default());
    assert!(fields(IncomeKind::Other, "1990", citizen).expenses);
    assert!(fields(IncomeKind::HighTech, "1970", citizen).pension);
    assert!(!fields(IncomeKind::HighTech, "1974", citizen).pension);
    assert!(fields(IncomeKind::HighTech, "1961", citizen).resident);
    assert!(!fields(IncomeKind::HighTech, "1961", Residency::ForeignWithStatus).resident);
}

/// Без года рождения соцплатёж не посчитан — ответ просит год; не вступивший
/// в накопительную пенсию до 1974 года рождения его не платит.
#[test]
fn missing_answers_are_asked() {
    let no_year = answer(Lang::Ru, &Raw { birth: String::new(), ..raw(IncomeKind::HighTech, "1 000 000") }, today());
    assert!(no_year.blocks.iter().any(|block| block.text == t(Lang::Ru, Key::CalcAskBirthYear)));
    let old = Input {
        birth_year: Some(1970),
        pension_joined: Some(false),
        ..input(IncomeKind::HighTech, 1_000_000, 0)
    };
    assert!(matches!(example(&old, today()).annual.social, Social::Out(_)));
}

/// «Открыть Kvartik» берёт с собой то, что вошло в пример: строку налога,
/// год рождения, гражданство и ответы — только на заданные вопросы (пенсия —
/// родившемуся до 1974 года, резидентство — гражданину с 1961 года рождения,
/// как в `fields_follow_the_rules`). Примера нет — нет и записи, а год
/// с ошибкой в пример не вошёл и не передаётся.
#[test]
fn the_example_hands_off_what_the_profile_has() {
    let old = Raw {
        birth: "1961".to_owned(),
        pension: "no".to_owned(),
        resident: "yes".to_owned(),
        ..raw(IncomeKind::Trade, "1 000 000")
    };
    let want = Handoff {
        kind: Some(IncomeKind::Trade),
        birth_year: Some(1961),
        residency: Some(Residency::Citizen),
        pension_joined: Some(false),
        resident: Some(true),
    };
    let mut wrong = Vec::new();
    let young = Raw {
        pension: "yes".to_owned(),
        resident: "no".to_owned(),
        ..raw(IncomeKind::HighTech, "1 000 000")
    };
    let young_want = Handoff {
        kind: Some(IncomeKind::HighTech),
        birth_year: Some(1990),
        residency: Some(Residency::Citizen),
        ..Handoff::default()
    };
    let bad_year = Raw {
        birth: "19".to_owned(),
        ..raw(IncomeKind::Trade, "1 000")
    };
    for (what, got, expected) in [
        ("с ответами", handoff(Lang::Ru, &old, today()), Some(want)),
        ("без незаданных вопросов", handoff(Lang::Ru, &young, today()), Some(young_want)),
        ("без оборота", handoff(Lang::Ru, &raw(IncomeKind::Trade, " "), today()), None),
        ("оборот с ошибкой", handoff(Lang::Ru, &raw(IncomeKind::Trade, "abc"), today()), None),
        (
            "год с ошибкой",
            handoff(Lang::Ru, &bad_year, today()),
            Some(Handoff {
                kind: Some(IncomeKind::Trade),
                residency: Some(Residency::Citizen),
                ..Handoff::default()
            }),
        ),
    ] {
        if got != expected {
            wrong.push(format!("{what}: {got:?}, а ждали {expected:?}"));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// Запись читается обратно, а запись другой версии — по полям: незнакомое
/// значение — «нет значения», лишнее поле не мешает. Нечитаемая или
/// с полем другого типа — пустая: это только предложение.
#[test]
fn the_handoff_record_is_lenient() {
    let full = Handoff {
        kind: Some(IncomeKind::Catering),
        birth_year: Some(1985),
        residency: Some(Residency::ForeignWithStatus),
        pension_joined: Some(true),
        resident: Some(false),
    };
    assert_eq!(
        full.to_json(),
        r#"{"kind":"catering","birth_year":1985,"residency":"foreign_with_status","pension_joined":true,"resident":false}"#
    );
    assert_eq!(Handoff::parse(&full.to_json()), full);
    assert_eq!(Handoff::default().to_json(), "{}");
    assert_eq!(
        Handoff::parse(r#"{"kind":"space_mining","birth_year":1985,"residency":"martian","extra":[1,2]}"#),
        Handoff {
            birth_year: Some(1985),
            ..Handoff::default()
        }
    );
    for broken in ["", "x", "null", "[1985]", r#"{"birth_year":"1985"}"#] {
        assert_eq!(Handoff::parse(broken), Handoff::default(), "{broken}");
    }
}

/// Окно предлагает только то, чего профиль не знает: заполненное человеком
/// не переписывается, а строку — коду, пока он один и строки у него никто
/// не выбирал. Высокие технологии и прочую деятельность решает сам код.
#[test]
fn only_what_the_profile_lacks_is_offered() {
    let carried = Handoff {
        kind: Some(IncomeKind::Trade),
        birth_year: Some(1961),
        residency: Some(Residency::Citizen),
        pension_joined: Some(false),
        resident: Some(true),
    };
    let code = ActivityCode::parse("47.11.0").expect("годный код");
    let mut wrong = Vec::new();
    let mut check = |what: &str, got: Handoff, expected: Handoff| {
        if got != expected {
            wrong.push(format!("{what}: {got:?}, а ждали {expected:?}"));
        }
    };
    check("профиля нет", carried.new_for(None), carried);
    let mut data = ProfileData::default();
    check("профиль пустой", carried.new_for(Some(&data)), carried);
    data.person.birth_year = Some(1962);
    data.person.residency = Some(Residency::ForeignWithStatus);
    data.activities = vec![code];
    check(
        "год и гражданство заполнены, код один",
        carried.new_for(Some(&data)),
        Handoff {
            kind: Some(IncomeKind::Trade),
            pension_joined: Some(false),
            resident: Some(true),
            ..Handoff::default()
        },
    );
    data.kinds.insert(code, IncomeKind::Production);
    data.person.pension_joined = Some(true);
    data.person.resident = Some(false);
    check("заполнено всё", carried.new_for(Some(&data)), Handoff::default());
    let two = ProfileData {
        activities: vec![code, ActivityCode::parse("62.01.0").expect("годный код")],
        ..ProfileData::default()
    };
    check(
        "два кода",
        carried.new_for(Some(&two)),
        Handoff {
            kind: None,
            ..carried
        },
    );
    for kind in [IncomeKind::HighTech, IncomeKind::Other] {
        let only = Handoff {
            kind: Some(kind),
            ..Handoff::default()
        };
        check(&format!("{kind:?}"), only.new_for(None), Handoff::default());
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
    assert!(Handoff::default().is_empty() && !carried.is_empty());
}

/// Имена строк и гражданства в разметке — те же, что в данных, и читаются
/// обратно; чужое имя — первая строка списка.
#[test]
fn ids_round_trip() {
    for kind in KINDS {
        assert_eq!(kind_from(&kind_id(kind)), Some(kind), "{kind:?}");
    }
    assert_eq!(kind_id(IncomeKind::HighTech), "high_tech");
    assert_eq!(kind_from("assets"), None, "не строка калькулятора");
    assert_eq!(residency_id(Residency::Citizen), "citizen");
    for residency in RESIDENCIES {
        assert!(!residency_id(residency).is_empty());
    }
}

/// Имена полей формы — те, что читает ядро: страница шлёт их же
/// (`crates/sw/src/landing.rs`), а чужое поле не мешает.
#[test]
fn raw_reads_the_form_names() {
    let raw: Raw = serde_json::from_str(
        r#"{"kind":"rent","turnover":"1","expenses":"2","birth":"1990","residency":"citizen","pension":"yes","resident":"no","other":"x"}"#,
    )
    .expect("поля читаются");
    let want = Raw {
        kind: "rent".to_owned(),
        turnover: "1".to_owned(),
        expenses: "2".to_owned(),
        birth: "1990".to_owned(),
        residency: "citizen".to_owned(),
        pension: "yes".to_owned(),
        resident: "no".to_owned(),
    };
    assert_eq!(raw, want);
}

/// Часы, отстающие от дня сверки правил, врут: пример — на день сверки;
/// часов нет — тоже. Часы впереди — их день.
#[test]
fn the_clock_never_runs_behind_the_rules() {
    let checked = rules::armenia().checked().expect("правила сверены");
    assert_eq!(super::today(None), Some(checked));
    assert_eq!(super::today(Some(day(2020, 1, 1))), Some(checked));
    assert_eq!(super::today(Some(day(2031, 6, 1))), Some(day(2031, 6, 1)));
}

/// Код примера вне списка высоких технологий и вне кодов, закрывающих
/// налог с оборота, — на «сегодня» тестов и на весь срок правил.
#[test]
fn the_plain_code_is_plain() {
    let code = ActivityCode::parse(PLAIN_CODE).expect("годный код");
    assert_eq!(turnover::default_kind(&code, today()), IncomeKind::Other);
    assert!(turnover::excluded(&code, today()).is_none());
    assert!(rules::armenia().high_tech().iter().all(|list| !list.contains(&code)));
}
