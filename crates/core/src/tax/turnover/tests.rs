//! Тесты налога квартала. Ожидаемые числа посчитаны вручную по тексту
//! закона — НК, ст. 258 и 260 — и формулам формы ՊԵԿ 299-Ն (прил. 1,
//! п. 9–17), а не записаны кодом: код, записавший себе эталон, проверял бы
//! сам себя (Правило 2). Посчитал агент 2026-10-19; чужие примеры ՊԵԿ
//! и бухгалтеров — в `crates/core/tests/cases/`.

use super::*;
use crate::model::ledger::OperationData;
use crate::model::money::{Currency, ExchangeRate, Money, RateSource};
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

/// Расход в драмах.
fn expense(date: Date, drams: i64, proof: Proof, nature: Nature, activity: Option<&str>) -> Operation {
    operation(OperationData {
        flow: Flow::Expense,
        date,
        paid: None,
        amount: Money::new(drams, Currency::Amd),
        rate: None,
        text: Text::new("Расход").expect("текст"),
        client: None,
        supplier: Text::default(),
        proof: Some(proof),
        nature: Some(nature),
        activity: activity.map(code),
        attachments: Vec::new(),
    })
}

/// Обычный расход по «հաշիվ վավերագիր», общий для всех видов.
fn ordinary(date: Date, drams: i64) -> Operation {
    expense(date, drams, Proof::SettlementInvoice, Nature::Ordinary, None)
}

/// Налог квартала по этим записям.
fn tax(profile: Option<&ProfileData>, operations: &[Operation], closings: &[Closing], q: Quarter) -> QuarterTax {
    quarter_of(&[], profile, operations, closings, q)
}

fn quarter_of(
    documents: &[Document],
    profile: Option<&ProfileData>,
    operations: &[Operation],
    closings: &[Closing],
    q: Quarter,
) -> QuarterTax {
    super::quarter(
        Books {
            documents,
            operations,
            profile,
            closings,
            unreadable: 0,
        },
        q,
    )
}

/// Строка без вычета.
fn plain(kind: IncomeKind, income: i64, rate: &str, tax: i64) -> Line {
    Line {
        kind,
        income,
        rate: percent(rate),
        gross: tax,
        deduction: None,
        tax,
    }
}

/// Строка прочей деятельности (10 %, вычет 6 %, предел 4,5 %).
#[allow(clippy::too_many_arguments, reason = "строка формы — столько чисел")]
fn other(
    income: i64,
    gross: i64,
    shared: i64,
    current: i64,
    carry_in: i64,
    floor: i64,
    tax: i64,
    carry_out: i64,
) -> Line {
    Line {
        kind: IncomeKind::Other,
        income,
        rate: percent("10"),
        gross,
        deduction: Some(Deduction {
            percent: percent("6"),
            floor_percent: percent("4.5"),
            attributed: 0,
            shared,
            expenses: shared,
            current,
            carry_in,
            total: current + carry_in,
            floor,
            carry_out,
        }),
        tax,
    }
}

/// IT по списку 142-Ն — 1 % без вычета; 4 321 987 ֏ × 1 % = 43 219,87 —
/// 43 219: лумы отбрасываются (вопрос 9.2 листа сверки).
#[test]
fn high_tech_is_one_percent_without_deduction() {
    let profile = profile(&["62.01.0"]);
    let books = [income(day(2026, 8, 15), 4_321_987, None)];
    let got = tax(Some(&profile), &books, &[], quarter(2026, 3));
    assert_eq!(got.lines, [plain(IncomeKind::HighTech, 4_321_987, "1", 43_219)]);
    assert_eq!(got.tax, 43_219);
    assert!(got.complete());
    assert_eq!(got.regime, Regime::Turnover);
}

/// Прочая деятельность без расходов: 1 234 567 ֏ × 10 % = 123 456,7 —
/// 123 456 (вопрос 9.1); предел 4,5 % = 55 555,515 — 55 555.
#[test]
fn services_without_expenses_pay_the_rate() {
    let profile = profile(&["74.10.0"]);
    let books = [income(day(2026, 7, 1), 1_234_567, None)];
    let got = tax(Some(&profile), &books, &[], quarter(2026, 3));
    assert_eq!(got.lines, [other(1_234_567, 123_456, 0, 0, 0, 55_555, 123_456, 0)]);
    assert_eq!(got.tax, 123_456);
}

/// Расходы больше 91,7 % дохода: налог упирается в предел, а остаток
/// вычета переносится и уменьшает налог следующего квартала (ст. 258, ч. 7;
/// 299-Ն, п. 16–17).
///
/// III квартал: 100 000 − 57 000 (6 % от 950 000) = 43 000 < 45 000 —
/// налог 45 000, перенос 57 000 − (100 000 − 45 000) = 2 000. IV: 100 000 −
/// 2 000 = 98 000.
#[test]
fn the_floor_holds_and_the_rest_carries_over() {
    let profile = profile(&["74.10.0"]);
    let books = [
        income(day(2026, 7, 15), 1_000_000, None),
        ordinary(day(2026, 8, 1), 950_000),
        income(day(2026, 10, 15), 1_000_000, None),
    ];
    let third = tax(Some(&profile), &books, &[], quarter(2026, 3));
    assert_eq!(third.lines, [other(1_000_000, 100_000, 950_000, 57_000, 0, 45_000, 45_000, 2_000)]);
    assert_eq!(third.carry_out(), BTreeMap::from([(IncomeKind::Other, 2_000)]));
    // Квартал — первый с записями: перенос из расчётов до программы неизвестен.
    assert!(third.carry_unknown);
    let fourth = tax(Some(&profile), &books, &[], quarter(2026, 4));
    assert_eq!(fourth.lines, [other(1_000_000, 100_000, 0, 0, 2_000, 45_000, 98_000, 0)]);
    assert!(!fourth.carry_unknown);
}

/// Квартал без дохода: вычет за расходы целиком переносится (ст. 258,
/// ч. 7: «в том числе при отсутствии базы»), налог — ноль.
///
/// I квартал: 6 % от 200 000 = 12 000 — перенос 12 000. II: 50 000 − 12 000
/// = 38 000 при пределе 22 500.
#[test]
fn a_quarter_without_income_carries_the_whole_deduction() {
    let profile = profile(&["74.10.0"]);
    let books = [ordinary(day(2026, 2, 10), 200_000), income(day(2026, 4, 20), 500_000, None)];
    let first = tax(Some(&profile), &books, &[], quarter(2026, 1));
    assert_eq!(first.lines, [other(0, 0, 200_000, 12_000, 0, 0, 0, 12_000)]);
    assert_eq!(first.tax, 0);
    let second = tax(Some(&profile), &books, &[], quarter(2026, 2));
    assert_eq!(second.lines, [other(500_000, 50_000, 0, 0, 12_000, 22_500, 38_000, 0)]);
}

/// IT и дизайн: две строки, а общие расходы делятся по весу дохода
/// в обороте всех видов, 1 % IT тоже (ст. 260, ч. 3; вопрос 1.4): дизайну —
/// 400 000 × 1 000 000 / 4 000 000 = 100 000, IT — остальное, без вычета.
#[test]
fn shared_expenses_split_by_turnover_of_all_kinds() {
    let profile = profile(&["62.01.0", "74.10.0"]);
    let books = [
        income(day(2026, 7, 10), 3_000_000, Some("62.01.0")),
        income(day(2026, 8, 10), 1_000_000, Some("74.10.0")),
        ordinary(day(2026, 9, 1), 400_000),
    ];
    let got = tax(Some(&profile), &books, &[], quarter(2026, 3));
    assert_eq!(
        got.lines,
        [
            plain(IncomeKind::HighTech, 3_000_000, "1", 30_000),
            other(1_000_000, 100_000, 100_000, 6_000, 0, 45_000, 94_000, 0),
        ]
    );
    assert_eq!(got.tax, 124_000);
    assert!(got.complete());
}

/// Расход с видом деятельности идёт в свою строку целиком.
#[test]
fn attributed_expenses_go_to_their_line() {
    let profile = profile(&["62.01.0", "74.10.0"]);
    let books = [
        income(day(2026, 7, 10), 3_000_000, Some("62.01.0")),
        income(day(2026, 8, 10), 1_000_000, Some("74.10.0")),
        expense(day(2026, 9, 1), 400_000, Proof::SettlementInvoice, Nature::Ordinary, Some("74.10.0")),
    ];
    let got = tax(Some(&profile), &books, &[], quarter(2026, 3));
    let design = got.lines.iter().find(|line| line.kind == IncomeKind::Other).expect("строка дизайна");
    let deduction = design.deduction.expect("вычет есть");
    // 6 % от 400 000 = 24 000; 100 000 − 24 000 = 76 000.
    assert_eq!((deduction.attributed, deduction.shared, deduction.current), (400_000, 0, 24_000));
    assert_eq!(design.tax, 76_000);
}

/// Драмы с лумами: три услуги по 50,00 $ по 361,93 — 54 289,50 ֏, в строке
/// — 54 289 (вопрос 9.5); 10 % — 5 428,9 — 5 428.
#[test]
fn foreign_income_is_summed_in_lumas() {
    let profile = profile(&["74.10.0"]);
    let books = [
        usd_income(day(2026, 7, 1), 5_000, Some(3_619_300)),
        usd_income(day(2026, 8, 1), 5_000, Some(3_619_300)),
        usd_income(day(2026, 9, 1), 5_000, Some(3_619_300)),
    ];
    let got = tax(Some(&profile), &books, &[], quarter(2026, 3));
    // Предел: 54 289 × 4,5 % = 2 443,005 — 2 443.
    assert_eq!(got.lines, [other(54_289, 5_428, 0, 0, 0, 2_443, 5_428, 0)]);
    assert_eq!(got.months, [Lumas::new(1_809_650); 3]);
}

/// Превысил порог: сделка, на которой превышен, делится — до 115 млн
/// налог с оборота, сверх — общий режим; доход и расходы после — не здесь
/// (ст. 255, ч. 1, п. 2; ст. 59, ч. 1, п. 5; вопрос 7.3).
///
/// До 10 августа — 110 млн; из 10 млн этого дня под налог с оборота идут
/// 5 млн: 500 000 ֏.
#[test]
fn the_threshold_splits_the_crossing_sale() {
    let profile = profile(&["74.10.0"]);
    let books = [
        income(day(2026, 2, 10), 60_000_000, None),
        income(day(2026, 5, 10), 50_000_000, None),
        income(day(2026, 8, 10), 10_000_000, None),
        ordinary(day(2026, 9, 1), 100_000),
        income(day(2026, 9, 5), 3_000_000, None),
    ];
    let first = tax(Some(&profile), &books, &[], quarter(2026, 1));
    assert_eq!(first.regime, Regime::Turnover);
    assert_eq!(first.tax, 6_000_000);
    let third = tax(Some(&profile), &books, &[], quarter(2026, 3));
    assert_eq!(third.lines, [other(5_000_000, 500_000, 0, 0, 0, 225_000, 500_000, 0)]);
    assert_eq!(
        third.regime,
        Regime::Left {
            from: day(2026, 8, 10),
            why: Leaving::Threshold
        }
    );
    assert_eq!((third.year_turnover, third.threshold), (123_000_000, Some(115_000_000)));
    let mut later = books.to_vec();
    later.push(income(day(2026, 11, 1), 1_000, None));
    let fourth = tax(Some(&profile), &later, &[], quarter(2026, 4));
    assert!(fourth.lines.is_empty());
    assert_eq!(fourth.tax, 0);
}

/// Ровно 115 000 000 — ещё не превышение: «не превысил» (ст. 254, ч. 2).
#[test]
fn exactly_the_threshold_is_not_over_it() {
    let profile = profile(&["74.10.0"]);
    let books = [income(day(2026, 3, 1), 115_000_000, None)];
    assert_eq!(tax(Some(&profile), &books, &[], quarter(2026, 1)).regime, Regime::Turnover);
}

/// Доход по коду 70.22 — со дня этой услуги до конца года налога с оборота
/// нет (ст. 255, ч. 1, п. 3): IT до него — 1 %, после — к бухгалтеру.
#[test]
fn income_under_an_excluded_code_ends_the_regime() {
    let profile = profile(&["62.01.0", "70.22.0"]);
    let books = [
        income(day(2026, 7, 5), 1_000_000, Some("62.01.0")),
        income(day(2026, 8, 1), 200_000, Some("70.22.0")),
        income(day(2026, 9, 1), 500_000, Some("62.01.0")),
    ];
    let got = tax(Some(&profile), &books, &[], quarter(2026, 3));
    assert_eq!(got.lines, [plain(IncomeKind::HighTech, 1_000_000, "1", 10_000)]);
    assert_eq!(
        got.regime,
        Regime::Left {
            from: day(2026, 8, 1),
            why: Leaving::Code(code("70.22.0"))
        }
    );
    assert_eq!(got.excluded_codes, [(code("70.22.0"), day(2025, 7, 1))]);
}

/// Единственный код профиля — и есть код дохода без вида: если он закрывает
/// налог с оборота, первый же доход выводит из режима.
#[test]
fn the_only_profile_code_is_the_income_code() {
    let profile = profile(&["70.22.0"]);
    let books = [income(day(2026, 8, 1), 200_000, None)];
    let got = tax(Some(&profile), &books, &[], quarter(2026, 3));
    assert!(got.lines.is_empty());
    assert_eq!(
        got.regime,
        Regime::Left {
            from: day(2026, 8, 1),
            why: Leaving::Code(code("70.22.0"))
        }
    );
}

/// Оборот прошлого года больше порога — в этом году налога с оборота нет
/// (ст. 254, ч. 2).
#[test]
fn last_year_over_the_threshold_closes_this_year() {
    let profile = profile(&["74.10.0"]);
    let books = [income(day(2025, 6, 10), 120_000_000, None), income(day(2026, 2, 1), 1_000_000, None)];
    let got = tax(Some(&profile), &books, &[], quarter(2026, 1));
    assert!(got.lines.is_empty());
    assert_eq!(
        got.regime,
        Regime::Left {
            from: day(2026, 1, 1),
            why: Leaving::PreviousYear
        }
    );
}

/// Какой расход идёт в вычет: платёжки хватает зарплате и услугам банка,
/// а покупке — нет; без документа — нет;
/// командировки — нет ни с каким документом (ст. 55, ч. 12; ст. 258, ч. 6).
///
/// В вычет — только 50 000: 6 % — 3 000; 100 000 − 3 000 = 97 000.
#[test]
fn only_documented_expenses_are_deducted() {
    let profile = profile(&["74.10.0"]);
    let books = [
        income(day(2026, 7, 1), 1_000_000, None),
        expense(day(2026, 7, 2), 100_000, Proof::BankPayment, Nature::Ordinary, None),
        expense(day(2026, 7, 3), 50_000, Proof::BankPayment, Nature::Levy, None),
        expense(day(2026, 7, 4), 30_000, Proof::None, Nature::Ordinary, None),
        expense(day(2026, 7, 5), 20_000, Proof::SettlementInvoice, Nature::Travel, None),
    ];
    let got = tax(Some(&profile), &books, &[], quarter(2026, 3));
    assert_eq!(got.lines, [other(1_000_000, 100_000, 50_000, 3_000, 0, 45_000, 97_000, 0)]);
    assert_eq!(
        got.refused,
        Refused {
            no_document: 1,
            payment_only: 1,
            excluded: 1,
            no_base: 0
        }
    );
}

/// Общий расход при доходе только IT: вес строки 10 — ноль, вычета нет,
/// и окно это называет (ст. 260, ч. 3). С доходом дизайна в том же квартале
/// расход делится по долям, и отказа нет. Расход, отнесённый к дизайну,
/// переносится и без дохода (ст. 258, ч. 7) — случай `it-and-design-no-base`.
#[test]
fn shared_expense_without_a_deduction_row_is_named() {
    let profile = profile(&["62.01.0", "74.10.0"]);
    let accountant = expense(day(2026, 8, 1), 400_000, Proof::SettlementInvoice, Nature::Ordinary, None);
    let it = income(day(2026, 7, 1), 3_000_000, Some("62.01.0"));
    let design = income(day(2026, 7, 2), 1_000_000, Some("74.10.0"));
    let only_it = tax(Some(&profile), &[it.clone(), accountant.clone()], &[], quarter(2026, 3));
    assert_eq!(only_it.refused.no_base, 1);
    assert_eq!(only_it.tax, 30_000);
    assert_eq!(only_it.carry_out(), BTreeMap::new());
    let both = tax(Some(&profile), &[it.clone(), design, accountant.clone()], &[], quarter(2026, 3));
    assert_eq!(both.refused.no_base, 0);
    // Только IT в профиле: вычета нет вовсе, и говорить не о чем.
    let it_alone = self::profile(&["62.01.0"]);
    let pure = tax(Some(&it_alone), &[income(day(2026, 7, 1), 3_000_000, None), accountant], &[], quarter(2026, 3));
    assert_eq!(pure.refused.no_base, 0);
    assert_eq!(pure.tax, 30_000);
}

/// Правило вычета — по каждому документу и виду расхода.
#[test]
fn deductible_follows_the_rules_file() {
    let on = day(2026, 10, 19);
    let mut wrong = Vec::new();
    for proof in Proof::ALL {
        for nature in Nature::ALL {
            let expected = match (proof, nature) {
                (_, Nature::Asset | Nature::Travel | Nature::Hospitality) => Err(Refusal::Excluded),
                (Proof::None, _) => Err(Refusal::NoDocument),
                (Proof::BankPayment, Nature::Levy) => Ok(()),
                (Proof::BankPayment, _) => Err(Refusal::PaymentOnly),
                _ => Ok(()),
            };
            let got = deductible(proof, nature, on);
            if got != expected {
                wrong.push(format!("{proof:?} {nature:?}: {got:?}, а не {expected:?}"));
            }
        }
    }
    assert!(wrong.is_empty(), "вычет: {wrong:#?}");
    // До 2025 года правила вычета нет.
    assert_eq!(
        deductible(Proof::SettlementInvoice, Nature::Ordinary, day(2024, 12, 31)),
        Err(Refusal::NoRule)
    );
}

/// Чего не хватает — называется: доход без курса, без вида деятельности
/// при кодах разных строк, без кодов вовсе.
#[test]
fn gaps_are_named_not_hidden() {
    let profile = profile(&["62.01.0", "74.10.0"]);
    let books = [income(day(2026, 7, 1), 1_000_000, None), usd_income(day(2026, 7, 2), 10_000, None)];
    let got = tax(Some(&profile), &books, &[], quarter(2026, 3));
    assert!(got.lines.is_empty());
    assert_eq!((got.gaps.unassigned, got.gaps.unrated), (1, 1));
    assert!(!got.complete());
    let none = tax(None, &books[..1], &[], quarter(2026, 3));
    assert!(none.gaps.no_codes);
    assert_eq!(none.gaps.unassigned, 1);
}

/// Нет правила — нет строки, и это сказано: до 2025 года ставки были
/// другими, а 1 % IT — по 2031 год (вопрос 1.7: «правила нет», а не 10 %).
#[test]
fn no_rule_is_a_gap_not_a_guess() {
    let services = profile(&["74.10.0"]);
    let before = tax(Some(&services), &[income(day(2024, 11, 10), 1_000_000, None)], &[], quarter(2024, 4));
    assert_eq!(before.gaps.no_rule, [IncomeKind::Other]);
    assert!(before.lines.is_empty());
    let it = profile(&["62.01.0"]);
    let last = tax(Some(&it), &[income(day(2031, 12, 31), 1_000_000, None)], &[], quarter(2031, 4));
    assert_eq!(last.lines, [plain(IncomeKind::HighTech, 1_000_000, "1", 10_000)]);
    let after = tax(Some(&it), &[income(day(2032, 1, 1), 1_000_000, None)], &[], quarter(2032, 1));
    assert_eq!(after.gaps.no_rule, [IncomeKind::HighTech]);
}

/// Строка, выбранная коду в профиле, важнее правил: магазин —
/// торговля, 10 % − 9,5 % расходов, не меньше 1 %.
///
/// 100 000 − 91 200 (9,5 % от 960 000) = 8 800 < 10 000 — налог 10 000,
/// перенос 91 200 − (100 000 − 10 000) = 1 200.
#[test]
fn the_kind_chosen_in_the_profile_wins() {
    let mut shop = profile(&["47.91.0"]);
    shop.kinds.insert(code("47.91.0"), IncomeKind::Trade);
    let books = [income(day(2026, 7, 1), 1_000_000, None), ordinary(day(2026, 7, 2), 960_000)];
    let got = tax(Some(&shop), &books, &[], quarter(2026, 3));
    let trade = Line {
        kind: IncomeKind::Trade,
        income: 1_000_000,
        rate: percent("10"),
        gross: 100_000,
        deduction: Some(Deduction {
            percent: percent("9.5"),
            floor_percent: percent("1"),
            attributed: 0,
            shared: 960_000,
            expenses: 960_000,
            current: 91_200,
            carry_in: 0,
            total: 91_200,
            floor: 10_000,
            carry_out: 1_200,
        }),
        tax: 10_000,
    };
    assert_eq!(got.lines, [trade]);
}

/// Строка по правилам: коды списка 142-Ն — высокие технологии с 2025 года,
/// остальное — прочая деятельность.
#[test]
fn default_kinds_follow_the_list() {
    let today = day(2026, 10, 19);
    assert_eq!(default_kind(&code("62.01.0"), today), IncomeKind::HighTech);
    assert_eq!(default_kind(&code("26.11.0"), today), IncomeKind::HighTech);
    assert_eq!(default_kind(&code("72.20.0"), today), IncomeKind::HighTech);
    assert_eq!(default_kind(&code("62.02.9"), today), IncomeKind::HighTech);
    assert_eq!(default_kind(&code("74.10.0"), today), IncomeKind::Other);
    assert_eq!(default_kind(&code("47.91.0"), today), IncomeKind::Other);
    assert_eq!(default_kind(&code("62.01.0"), day(2024, 12, 31)), IncomeKind::Other);
}

/// Закрытый квартал отдаёт перенос из снимка: правка его данных после
/// закрытия видна расхождением, а следующий квартал её не замечает
/// (Правило 2).
#[test]
fn a_closed_quarter_keeps_its_carry() {
    let profile = profile(&["74.10.0"]);
    let mut books = vec![ordinary(day(2026, 2, 10), 200_000), income(day(2026, 4, 20), 500_000, None)];
    let first = tax(Some(&profile), &books, &[], quarter(2026, 1));
    let closing = Closing::close(&first, day(2026, 4, 15), at(1_770_000_000_000)).expect("квартал кончился");
    let closings = [closing];
    // Расход I квартала внесли после закрытия.
    books.push(ordinary(day(2026, 3, 1), 100_000));
    let live = tax(Some(&profile), &books, &closings, quarter(2026, 1));
    assert!(live.differs_from(&closings[0]));
    let second = tax(Some(&profile), &books, &closings, quarter(2026, 2));
    // Перенос — 12 000 из снимка, а не 18 000 пересчитанных.
    assert_eq!(second.lines, [other(500_000, 50_000, 0, 0, 12_000, 22_500, 38_000, 0)]);
    // Открыли снова — перенос пересчитан: 50 000 − 18 000 = 32 000.
    let reopened = [closings[0].reopen(at(1_770_000_000_001)).expect("был закрыт")];
    let again = tax(Some(&profile), &books, &reopened, quarter(2026, 2));
    assert_eq!(again.tax, 32_000);
}

/// Закрыть можно только кончившийся и посчитанный целиком квартал.
#[test]
fn only_a_finished_complete_quarter_closes() {
    let profile = profile(&["74.10.0"]);
    let books = [income(day(2026, 7, 1), 1_000_000, None)];
    let third = tax(Some(&profile), &books, &[], quarter(2026, 3));
    assert_eq!(
        Closing::close(&third, day(2026, 9, 30), at(1)).map(|_| ()),
        Err(super::super::closing::CloseError::NotOver)
    );
    assert!(Closing::close(&third, day(2026, 10, 1), at(1)).is_ok());
    let unrated = [usd_income(day(2026, 7, 1), 10_000, None)];
    let incomplete = tax(Some(&profile), &unrated, &[], quarter(2026, 3));
    assert_eq!(
        Closing::close(&incomplete, day(2026, 10, 1), at(1)).map(|_| ()),
        Err(super::super::closing::CloseError::Incomplete)
    );
}

/// Квартал, в котором ИП ушёл с налога с оборота, не закрывается: перенос
/// уходит вместе с режимом (ст. 264, ч. 4), а снимок отдал бы его
/// следующему году. Расчёт такого квартала — у бухгалтера.
#[test]
fn a_quarter_with_leaving_does_not_close() {
    let profile = profile(&["74.10.0", "70.22.0"]);
    let books = [
        income(day(2026, 10, 5), 1_000_000, Some("74.10.0")),
        ordinary(day(2026, 10, 6), 950_000),
        income(day(2026, 11, 2), 200_000, Some("70.22.0")),
    ];
    let fourth = tax(Some(&profile), &books, &[], quarter(2026, 4));
    assert!(matches!(fourth.regime, Regime::Left { .. }));
    assert!(fourth.complete());
    assert!(!fourth.closable(day(2027, 1, 10)));
    assert_eq!(
        Closing::close(&fourth, day(2027, 1, 10), at(1)).map(|_| ()),
        Err(super::super::closing::CloseError::Left)
    );
}

/// Доход без курса и в обороте для порога — «нет значения», а не ноль:
/// оборот прошлого года и начала этого без него неизвестен, и квартал
/// не посчитан целиком (Правило 6). 50 000 $ по 361,93 — 18 096 500 ֏,
/// и прошлый год перешёл бы порог.
#[test]
fn turnover_for_the_threshold_misses_unrated_income() {
    let profile = profile(&["74.10.0"]);
    let last_year = [
        income(day(2025, 6, 10), 100_000_000, None),
        usd_income(day(2025, 12, 15), 5_000_000, None),
        income(day(2026, 2, 1), 1_000_000, None),
    ];
    let first = tax(Some(&profile), &last_year, &[], quarter(2026, 1));
    assert_eq!(first.gaps.year_unrated, 1);
    assert!(!first.complete());
    let this_year = [usd_income(day(2026, 3, 10), 30_000_000, None), income(day(2026, 5, 10), 10_000_000, None)];
    let second = tax(Some(&profile), &this_year, &[], quarter(2026, 2));
    assert_eq!(second.gaps.year_unrated, 1);
    assert!(!second.complete());
    // Прошлый год и без строки без курса выше порога — она ничего не решает.
    let over = [
        income(day(2025, 6, 10), 120_000_000, None),
        usd_income(day(2025, 12, 15), 5_000_000, None),
        income(day(2026, 2, 1), 1_000_000, None),
    ];
    assert_eq!(tax(Some(&profile), &over, &[], quarter(2026, 1)).gaps.year_unrated, 0);
}

/// Нечитаемая запись — неизвестный доход или расход: квартал не посчитан
/// целиком, хоть число без неё и есть.
#[test]
fn unreadable_records_leave_the_quarter_incomplete() {
    let profile = profile(&["74.10.0"]);
    let books = [income(day(2026, 7, 1), 1_000_000, None)];
    let got = super::quarter(
        Books {
            documents: &[],
            operations: &books,
            profile: Some(&profile),
            closings: &[],
            unreadable: 1,
        },
        quarter(2026, 3),
    );
    assert_eq!(got.gaps.unreadable, 1);
    assert!(!got.complete());
    assert_eq!(got.tax, 100_000);
}

/// Перенос из неполного квартала — тоже неполный: следующий не посчитан
/// целиком, пока прошлый не досчитан (ст. 258, ч. 7). В III квартале
/// доход в долларах без курса — с ним перенос был бы другим.
#[test]
fn an_incomplete_quarter_leaves_the_next_incomplete() {
    let profile = profile(&["74.10.0"]);
    let books = [
        income(day(2026, 7, 1), 1_000_000, None),
        usd_income(day(2026, 7, 2), 10_000, None),
        ordinary(day(2026, 7, 3), 950_000),
        income(day(2026, 10, 1), 1_000_000, None),
    ];
    let fourth = tax(Some(&profile), &books, &[], quarter(2026, 4));
    assert_eq!(fourth.gaps.previous, Some(quarter(2026, 3)));
    assert!(!fourth.complete());
    // Без строки без курса — полный.
    let full = [books[0].clone(), books[2].clone(), books[3].clone()];
    assert!(tax(Some(&profile), &full, &[], quarter(2026, 4)).complete());
}

/// Та же запись с идентификатором `id`: строки одного дня идут по нему.
fn with_id(operation: Operation, id: &str) -> Operation {
    let mut value = serde_json::to_value(&operation).expect("запись пишется");
    value["meta"]["id"] = serde_json::Value::from(id);
    serde_json::from_value(value).expect("запись читается")
}

/// День дохода по коду, закрывающему налог с оборота, — весь вне режима
/// («со дня», ст. 255, ч. 1, п. 3): порядок строк одного дня программе не
/// известен, и IT-доход того же дня не облагается 1 %, как бы ни лёг.
#[test]
fn the_day_of_an_excluded_code_is_wholly_outside() {
    let profile = profile(&["62.01.0", "70.22.0"]);
    let books = [
        with_id(income(day(2026, 8, 1), 1_000_000, Some("62.01.0")), "00000000-0000-4000-8000-000000000000"),
        with_id(income(day(2026, 8, 1), 200_000, Some("70.22.0")), "ffffffff-ffff-4fff-bfff-ffffffffffff"),
    ];
    let got = tax(Some(&profile), &books, &[], quarter(2026, 3));
    assert!(got.lines.is_empty(), "строки: {:?}", got.lines);
    assert_eq!(
        got.regime,
        Regime::Left {
            from: day(2026, 8, 1),
            why: Leaving::Code(code("70.22.0"))
        }
    );
}

/// Общие расходы квартала без дохода у ИП без кодов в профиле не пропадают
/// молча: строку налога не узнать, и это сказано. У ИП только с IT им
/// некуда идти по закону — это не пропуск.
#[test]
fn shared_expenses_without_codes_are_a_gap() {
    let books = [ordinary(day(2026, 2, 10), 200_000)];
    let empty = profile(&[]);
    let got = tax(Some(&empty), &books, &[], quarter(2026, 1));
    assert!(got.gaps.no_codes);
    assert!(!got.complete());
    assert!(tax(None, &books, &[], quarter(2026, 1)).gaps.no_codes);
    assert!(tax(Some(&profile(&["62.01.0"])), &books, &[], quarter(2026, 1)).complete());
}

/// Код, закрывающий налог с оборота, среди кодов одной строки: у дохода
/// без вида код не узнать, а с ним и режим (ст. 255, ч. 1, п. 3), — это
/// пропуск, и окно спрашивает вид деятельности.
#[test]
fn an_excluded_code_needs_the_income_code() {
    let profile = profile(&["69.20.0", "73.11.0"]);
    let books = [income(day(2026, 8, 1), 1_000_000, None)];
    let got = tax(Some(&profile), &books, &[], quarter(2026, 3));
    assert_eq!(got.gaps.unassigned, 1);
    assert!(!got.complete());
    assert!(activity_needed(Some(&profile), day(2026, 8, 1)));
    assert!(!activity_needed(Some(&profile), day(2025, 6, 30)));
    assert!(!activity_needed(Some(&self::profile(&["74.10.0", "73.11.0"])), day(2026, 8, 1)));
    assert!(activity_needed(Some(&self::profile(&["62.01.0", "73.11.0"])), day(2026, 8, 1)));
}

/// Доход из выпущенного акта — со своим видом деятельности: дизайн по акту
/// у ИП с IT и дизайном идёт в прочую деятельность.
#[test]
fn documents_bring_their_activity() {
    let act: Document = serde_json::from_str(concat!(
        r#"{"meta":{"id":"7c9e6679-7425-40de-944b-e07fc1f90ae8","modified":1760000000500,"deleted":null},"#,
        r#""data":{"kind":"act","client":"5b0e8f9a-3c2d-4e1f-8a7b-9c6d5e4f3a2b","lang":"hy","currency":"AMD","#,
        r#""date":"2026-10-13","service_date":"2026-10-13","due":null,"#,
        r#""items":[{"text":"Դիզայն","quantity_e3":1000,"price":1500000}],"#,
        r#""basis":null,"recurring":null,"rate":null,"activity":"74.10.0"},"#,
        r#""issued":{"number":{"year":2026,"seq":1},"#,
        r#""seller":{"name":{"hy":"ԱՁ Արամ Պետրոսյան","ru":"ИП Арам Петросян","en":"IE Aram Petrosyan"},"#,
        r#""tin":null,"registration":"","address":{"hy":"","ru":"","en":""},"phone":"","email":null,"accounts":[]},"#,
        r#""buyer":{"name":"«Արարատ» ՍՊԸ","country":"AM","address":"","tin":null,"tax_id":"","currency":"AMD","lang":"hy","payment_days":null}},"#,
        r#""annulled":null,"signed":null,"payments":[]}"#
    ))
    .expect("акт читается");
    let profile = profile(&["62.01.0", "74.10.0"]);
    let got = quarter_of(&[act], Some(&profile), &[], &[], quarter(2026, 4));
    assert_eq!(got.lines, [other(1_500_000, 150_000, 0, 0, 0, 67_500, 150_000, 0)]);
}

/// Процент — строкой закона: `4.5`, `10`; запятая, ведущий ноль, ноль
/// в конце, больше ста — нет.
#[test]
fn percents_are_written_as_the_law_writes_them() {
    let mut wrong = Vec::new();
    for (text, hundredths) in [("10", 1000), ("4.5", 450), ("1.5", 150), ("0.5", 50), ("100", 10_000), ("9.25", 925)] {
        if Percent::parse(text).map(Percent::hundredths) != Some(hundredths) {
            wrong.push(format!("«{text}» не прочитан"));
        }
        if Percent::parse(text).map(|p| p.to_string()).as_deref() != Some(text) {
            wrong.push(format!("«{text}» записан не так"));
        }
    }
    for text in ["", "4,5", "04.5", "4.50", "10.0", "100.5", "1000", "-1", "4.", ".5", " 10", "4.555"] {
        if let Some(found) = Percent::parse(text) {
            wrong.push(format!("«{text}» принят как {found}"));
        }
    }
    assert!(wrong.is_empty(), "проценты: {wrong:#?}");
    // Доля — с отбросом лум: 4,5 % от 1 234 567 = 55 555,515 — 55 555.
    assert_eq!(percent("4.5").of(1_234_567), 55_555);
}

/// Замер (Правило 1), а не проверка: налог последнего квартала пятилетней
/// книги без закрытых кварталов — 600 актов и 1 000 записей книги в год.
/// `cargo test -p kvartik-core --release --lib chain_timing -- --ignored --nocapture`.
#[test]
#[ignore = "замер, а не проверка"]
fn chain_timing() {
    let mut documents = Vec::new();
    let mut operations = Vec::new();
    for year in 2025..=2029_u16 {
        for n in 0..600_u16 {
            let date = day(year, (n % 12 + 1) as u8, (n % 28 + 1) as u8);
            let text = format!(
                concat!(
                    r#"{{"meta":{{"id":"{:08x}-0000-4000-8000-{:012x}","modified":1760000000500,"deleted":null}},"#,
                    r#""data":{{"kind":"act","client":"5b0e8f9a-3c2d-4e1f-8a7b-9c6d5e4f3a2b","lang":"hy","currency":"AMD","#,
                    r#""date":"{}","service_date":"{}","due":null,"#,
                    r#""items":[{{"text":"Դիզայն","quantity_e3":1000,"price":100000}},{{"text":"Ծառայություն","quantity_e3":2500,"price":20000}}],"#,
                    r#""basis":null,"recurring":null,"rate":null,"activity":null}},"#,
                    r#""issued":{{"number":{{"year":{},"seq":{}}},"#,
                    r#""seller":{{"name":{{"hy":"ԱՁ","ru":"ИП","en":"IE"}},"#,
                    r#""tin":null,"registration":"","address":{{"hy":"","ru":"","en":""}},"phone":"","email":null,"accounts":[]}},"#,
                    r#""buyer":{{"name":"«Արարատ» ՍՊԸ","country":"AM","address":"","tin":null,"tax_id":"","currency":"AMD","lang":"hy","payment_days":null}}}},"#,
                    r#""annulled":null,"signed":null,"payments":[]}}"#
                ),
                year,
                n,
                date.iso(),
                date.iso(),
                year,
                n + 1
            );
            documents.push(serde_json::from_str::<Document>(&text).expect("акт читается"));
        }
        for n in 0..1000_u16 {
            let date = day(year, (n % 12 + 1) as u8, (n % 28 + 1) as u8);
            operations.push(if n % 2 == 0 {
                income(date, 10_000 + i64::from(n), None)
            } else {
                ordinary(date, 1_000 + i64::from(n))
            });
        }
    }
    let profile = profile(&["74.10.0"]);
    let started = std::time::Instant::now();
    let got = quarter_of(&documents, Some(&profile), &operations, &[], quarter(2029, 4));
    let elapsed = started.elapsed();
    println!("цепочка 20 кварталов, {} записей: {elapsed:?}, налог {}", documents.len() + operations.len(), got.tax);
}
