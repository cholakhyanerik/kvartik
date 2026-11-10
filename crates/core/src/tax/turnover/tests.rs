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
        income_kind: None,
        attachments: Vec::new(),
    })
}

/// Доход в драмах со своей строкой налога и без кода.
fn income_as(date: Date, drams: i64, kind: IncomeKind) -> Operation {
    operation(OperationData {
        income_kind: Some(kind),
        ..income(date, drams, None).data().clone()
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
        income_kind: None,
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
            openings: &[],
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
    // Квартал — первый с записями: перенос из расчётов до программы неизвестен,
    // и неизвестным он тянется дальше по цепочке.
    assert_eq!(third.carry_unknown, Some(quarter(2026, 3)));
    let fourth = tax(Some(&profile), &books, &[], quarter(2026, 4));
    assert_eq!(fourth.lines, [other(1_000_000, 100_000, 0, 0, 2_000, 45_000, 98_000, 0)]);
    assert_eq!(fourth.carry_unknown, Some(quarter(2026, 3)));
}

/// Запись переноса на начало квартала `q`: прочая деятельность.
fn carried(q: Quarter, drams: i64) -> Opening {
    let carry = BTreeMap::from([(IncomeKind::Other, drams)]);
    Opening::save(None, q, carry, at(1_770_000_000_000)).expect("годный перенос").expect("новая запись")
}

/// Налог квартала с указанным переносом.
fn tax_from(profile: &ProfileData, operations: &[Operation], closings: &[Closing], opening: &Opening, q: Quarter) -> QuarterTax {
    super::quarter(
        Books {
            openings: std::slice::from_ref(opening),
            ..drift_books(profile, operations, closings)
        },
        q,
    )
}

/// Цепочка начинает с поданного: 12 000 уменьшают налог III квартала, а окно
/// больше не спрашивает. 60 000 упираются в предел: 100 000 − 60 000 = 40 000 —
/// меньше 45 000, налог 45 000, дальше идут 60 000 − 55 000 = 5 000.
#[test]
fn the_chain_starts_from_the_carry_given() {
    let profile = profile(&["74.10.0"]);
    let (third, fourth) = (quarter(2026, 3), quarter(2026, 4));
    let books = [income(day(2026, 7, 15), 1_000_000, None), income(day(2026, 10, 15), 1_000_000, None)];

    let got = tax_from(&profile, &books, &[], &carried(third, 12_000), third);
    assert_eq!(got.lines, [other(1_000_000, 100_000, 0, 0, 12_000, 45_000, 88_000, 0)]);
    assert_eq!(got.carry_unknown, None);

    let opening = carried(third, 60_000);
    let got = tax_from(&profile, &books, &[], &opening, third);
    assert_eq!(got.lines, [other(1_000_000, 100_000, 0, 0, 60_000, 45_000, 45_000, 5_000)]);
    let got = tax_from(&profile, &books, &[], &opening, fourth);
    assert_eq!(got.lines, [other(1_000_000, 100_000, 0, 0, 5_000, 45_000, 95_000, 0)]);
    assert_eq!(got.carry_unknown, None);
}

/// «Переноса не было» — ответ: числа те же, а окно больше не спрашивает.
#[test]
fn no_carry_is_an_answer() {
    let profile = profile(&["74.10.0"]);
    let q = quarter(2026, 3);
    let books = [income(day(2026, 7, 15), 1_000_000, None)];
    let asked = tax(Some(&profile), &books, &[], q);
    assert_eq!(asked.carry_unknown, Some(q));
    let answered = tax_from(&profile, &books, &[], &carried(q, 0), q);
    assert_eq!((answered.carry_unknown, &answered.lines), (None, &asked.lines));
}

/// Перенос указан раньше первой записи — он доходит через квартал без дохода
/// (НК, ст. 258, ч. 7). Записи раньше него — своя цепочка, а квартал переноса
/// берёт поданное: 12 000, а не 57 000, посчитанные из расхода I квартала.
#[test]
fn the_opening_passes_empty_quarters_and_ends_earlier_chains() {
    let profile = profile(&["74.10.0"]);
    let (first, second, third) = (quarter(2026, 1), quarter(2026, 2), quarter(2026, 3));
    let books = [income(day(2026, 7, 15), 1_000_000, None)];
    let got = tax_from(&profile, &books, &[], &carried(second, 12_000), third);
    assert_eq!(got.lines, [other(1_000_000, 100_000, 0, 0, 12_000, 45_000, 88_000, 0)]);

    let earlier = [ordinary(day(2026, 2, 10), 950_000), income(day(2026, 7, 15), 1_000_000, None)];
    let opening = carried(third, 12_000);
    let before = tax_from(&profile, &earlier, &[], &opening, second);
    assert_eq!(before.carry_unknown, Some(first), "до указанного — своя цепочка");
    let got = tax_from(&profile, &earlier, &[], &opening, third);
    assert_eq!(got.lines, [other(1_000_000, 100_000, 0, 0, 12_000, 45_000, 88_000, 0)]);
}

/// Снимок и указанный перенос: цепочка идёт от более позднего, а на одном
/// квартале — от указанного: снимок мог закрыть квартал, ничего о переносе
/// не зная, а запись на экране квартала обязана действовать.
#[test]
fn the_later_of_snapshot_and_opening_starts_the_chain() {
    let profile = profile(&["74.10.0"]);
    let (first, second, third) = (quarter(2026, 1), quarter(2026, 2), quarter(2026, 3));
    let books = [
        income(day(2026, 1, 15), 1_000_000, None),
        ordinary(day(2026, 2, 10), 950_000),
        income(day(2026, 4, 15), 1_000_000, None),
        income(day(2026, 7, 15), 1_000_000, None),
    ];
    // I квартал: 100 000 − 57 000 = 43 000 — меньше предела, налог 45 000,
    // перенос 2 000.
    let closings = [closed(&profile, &books, &[], first, 1_770_000_000_000)];
    assert_eq!(closings[0].carry_out(), BTreeMap::from([(IncomeKind::Other, 2_000)]));

    let on_the_same = tax_from(&profile, &books, &closings, &carried(second, 30_000), second);
    assert_eq!(on_the_same.lines, [other(1_000_000, 100_000, 0, 0, 30_000, 45_000, 70_000, 0)]);
    let older = tax_from(&profile, &books, &closings, &carried(first, 30_000), second);
    assert_eq!(older.lines, [other(1_000_000, 100_000, 0, 0, 2_000, 45_000, 98_000, 0)]);
    let later = tax_from(&profile, &books, &closings, &carried(third, 30_000), third);
    assert_eq!(later.lines, [other(1_000_000, 100_000, 0, 0, 30_000, 45_000, 70_000, 0)]);
}

/// Снимок без строк с вычетом о переносе не знает ничего: человек закрыл
/// пустой II квартал, как просит напоминание о сроке, а книгу начал с III.
/// Вопрос остаётся, ответ на него действует и на закрытии заранее, и когда
/// квартал закрыт уже после ответа: 100 000 − 12 000 = 88 000.
#[test]
fn an_empty_snapshot_does_not_answer_the_question() {
    let profile = profile(&["74.10.0"]);
    let (second, third) = (quarter(2026, 2), quarter(2026, 3));
    let books = [income(day(2026, 7, 15), 1_000_000, None)];
    let closings = [closed(&profile, &[], &[], second, 1_770_000_000_000)];
    assert_eq!(closings[0].data().lines, []);
    assert_eq!(tax(Some(&profile), &books, &closings, third).carry_unknown, Some(third));
    let got = tax_from(&profile, &books, &closings, &carried(third, 12_000), third);
    assert_eq!(got.lines, [other(1_000_000, 100_000, 0, 0, 12_000, 45_000, 88_000, 0)]);
    assert_eq!(got.carry_unknown, None);

    // Снимок со строкой с вычетом — поданный расчёт: дальше цепочка от него.
    let fourth = quarter(2026, 4);
    let filed = [closed(&profile, &books, &[], third, 1_770_000_000_000)];
    let books = [income(day(2026, 7, 15), 1_000_000, None), income(day(2026, 10, 15), 1_000_000, None)];
    assert_eq!(tax(Some(&profile), &books, &filed, fourth).carry_unknown, None);
}

/// Тихий квартал закрыт пустым, а I квартал уже закрыт со строкой с вычетом:
/// перенос известен из того снимка, и спрашивать не о чем.
#[test]
fn a_quiet_closed_quarter_keeps_what_was_filed() {
    let profile = profile(&["74.10.0"]);
    let (first, second, third) = (quarter(2026, 1), quarter(2026, 2), quarter(2026, 3));
    let books = [income(day(2026, 1, 15), 1_000_000, None), income(day(2026, 7, 15), 1_000_000, None)];
    let mut closings = vec![closed(&profile, &books, &[], first, 1_770_000_000_000)];
    closings.push(closed(&profile, &books, &closings, second, 1_770_000_000_001));
    assert_eq!(closings[1].data().lines, []);
    let got = tax(Some(&profile), &books, &closings, third);
    assert_eq!(got.carry_unknown, None);
    assert_eq!(got.lines, [other(1_000_000, 100_000, 0, 0, 0, 45_000, 100_000, 0)]);
}

/// Пустой снимок задолго до книги: вопрос — о первом квартале с записями,
/// а не о том, что за снимком. Иначе человек вписал бы x.11 расчёта на
/// квартал раньше и взял больше вычета, чем осталось к началу книги.
#[test]
fn the_question_is_about_the_first_quarter_with_records() {
    let profile = profile(&["74.10.0"]);
    let (first, third) = (quarter(2026, 1), quarter(2026, 3));
    let books = [income(day(2026, 7, 15), 1_000_000, None)];
    let closings = [closed(&profile, &[], &[], first, 1_770_000_000_000)];
    assert_eq!(tax(Some(&profile), &books, &closings, third).carry_unknown, Some(third));
}

/// После ухода с режима спрашивать о переносе не о чем: в новом году от него
/// ничего нет (НК, ст. 264, ч. 4). Квартал ухода ещё может о нём говорить.
#[test]
fn leaving_ends_the_question() {
    let profile = profile(&["74.10.0", "70.22.0"]);
    let books = [
        income(day(2026, 7, 15), 1_000_000, Some("74.10.0")),
        income(day(2026, 11, 20), 300_000, Some("70.22.0")),
        income(day(2027, 1, 15), 1_000_000, Some("74.10.0")),
    ];
    assert_eq!(tax(Some(&profile), &books, &[], quarter(2026, 3)).carry_unknown, Some(quarter(2026, 3)));
    assert_eq!(tax(Some(&profile), &books, &[], quarter(2027, 1)).carry_unknown, None);
}

/// Зарегистрированный в первом квартале книги или позже расчётов до него не
/// подавал, и спрашивать незачем; раньше — вопрос остаётся.
#[test]
fn a_new_entrepreneur_is_not_asked() {
    let q = quarter(2026, 3);
    let books = [income(day(2026, 7, 15), 1_000_000, None)];
    let mut fresh = profile(&["74.10.0"]);
    fresh.person.registered = Some(day(2026, 7, 10));
    assert_eq!(tax(Some(&fresh), &books, &[], q).carry_unknown, None);
    fresh.person.registered = Some(day(2026, 6, 30));
    assert_eq!(tax(Some(&fresh), &books, &[], q).carry_unknown, Some(q));
}

/// Записи двух кварталов не мешают друг другу: ответ о квартале раньше не
/// стирает поданное позже, а после слияния копий с разными началами
/// работают обе. II квартал: 30 000 из его записи, III: 12 000 из своей.
#[test]
fn records_of_two_quarters_both_stand() {
    let profile = profile(&["74.10.0"]);
    let (second, third) = (quarter(2026, 2), quarter(2026, 3));
    let books = [income(day(2026, 5, 15), 1_000_000, None), income(day(2026, 7, 15), 1_000_000, None)];
    let openings = [carried(third, 12_000), carried(second, 30_000)];
    let of = |q| {
        super::quarter(
            Books {
                openings: &openings,
                ..drift_books(&profile, &books, &[])
            },
            q,
        )
    };
    assert_eq!(of(second).lines, [other(1_000_000, 100_000, 0, 0, 30_000, 45_000, 70_000, 0)]);
    assert_eq!(of(third).lines, [other(1_000_000, 100_000, 0, 0, 12_000, 45_000, 88_000, 0)]);
}

/// Уход с режима обрывает перенос и указанный (НК, ст. 264, ч. 4): в новом
/// году от него ничего не остаётся.
#[test]
fn leaving_drops_the_carry_given() {
    let profile = profile(&["74.10.0", "70.22.0"]);
    let books = [
        income(day(2026, 7, 15), 1_000_000, Some("74.10.0")),
        income(day(2026, 11, 20), 300_000, Some("70.22.0")),
        income(day(2027, 1, 15), 1_000_000, Some("74.10.0")),
    ];
    let opening = carried(quarter(2026, 3), 60_000);
    let got = tax_from(&profile, &books, &[], &opening, quarter(2026, 3));
    assert_eq!(got.lines, [other(1_000_000, 100_000, 0, 0, 60_000, 45_000, 45_000, 5_000)]);
    let left = tax_from(&profile, &books, &[], &opening, quarter(2026, 4));
    assert!(matches!(left.regime, Regime::Left { .. }));
    let next = tax_from(&profile, &books, &[], &opening, quarter(2027, 1));
    assert_eq!(next.lines, [other(1_000_000, 100_000, 0, 0, 0, 45_000, 100_000, 0)]);
}

/// На начало 2025 года перенос мог быть только у торговли (ՀՕ-285-Ն, ст. 16,
/// ч. 3), поэтому спрашивать о нём других незачем. Позже — у всех строк
/// с вычетом, а у строки без вычета — никогда.
#[test]
fn the_carry_is_asked_only_where_it_could_be() {
    let design = profile(&["74.10.0"]);
    let mut trade = design.clone();
    trade.kinds.insert(code("74.10.0"), IncomeKind::Trade);
    let first = quarter(2025, 1);
    let books = [income(day(2025, 1, 15), 1_000_000, None)];
    assert_eq!(tax(Some(&design), &books, &[], first).carry_unknown, None);
    assert_eq!(tax(Some(&trade), &books, &[], first).carry_unknown, Some(first));

    let later = quarter(2025, 2);
    let books = [income(day(2025, 4, 15), 1_000_000, None)];
    assert_eq!(tax(Some(&design), &books, &[], later).carry_unknown, Some(later));
    let it = profile(&["62.01.0"]);
    assert_eq!(tax(Some(&it), &books, &[], later).carry_unknown, None);
}

/// Перенос указали после закрытия — расчёт закрытого квартала разошёлся со
/// снимком: 100 000 − 12 000 = 88 000 вместо поданных 100 000.
#[test]
fn a_carry_given_after_closing_is_a_drift() {
    let profile = profile(&["74.10.0"]);
    let q = quarter(2026, 3);
    let books = [income(day(2026, 7, 15), 1_000_000, None)];
    let closings = [closed(&profile, &books, &[], q, 1_770_000_000_000)];
    assert_eq!(scan(drift_books(&profile, &books, &closings)), None);
    let opening = carried(q, 12_000);
    let with_carry = Books {
        openings: std::slice::from_ref(&opening),
        ..drift_books(&profile, &books, &closings)
    };
    let (found, closing) = scan(with_carry).expect("квартал разошёлся");
    assert_eq!((found.tax, closing.data().tax), (88_000, 100_000));
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
            no_base: 0,
            split: 0,
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

/// Книга для поиска расхождения: только эти записи и снимки.
fn drift_books<'a>(profile: &'a ProfileData, operations: &'a [Operation], closings: &'a [Closing]) -> Books<'a> {
    Books {
        documents: &[],
        operations,
        profile: Some(profile),
        closings,
        openings: &[],
        unreadable: 0,
    }
}

/// Поиск расхождения в день, когда все кварталы тестов кончились.
fn scan(books: Books<'_>) -> Option<(QuarterTax, &Closing)> {
    drifted(books, day(2030, 1, 2))
}

/// Снимок квартала по этим записям и снимкам, как его кладёт «Закрыть».
fn closed(profile: &ProfileData, operations: &[Operation], closings: &[Closing], q: Quarter, now: i64) -> Closing {
    let today = day(2030, 1, 2);
    Closing::close(&tax(Some(profile), operations, closings, q), today, at(now)).expect("квартал кончился и посчитан")
}

/// Забытый доход в закрытом квартале находится, и расчёт — тот же, что
/// у `quarter`. Нашлось два — первый по календарю: его уточнённый расчёт
/// поправит перенос следующим. Открытый снова в поиск не идёт.
///
/// I квартал: 1 000 000 × 10 % = 100 000. II: 500 000 × 10 % = 50 000,
/// расход 200 000 × 6 % = 12 000, налог 38 000. Забытые 300 000 во II:
/// 800 000 × 10 % = 80 000, предел 36 000, налог 80 000 − 12 000 = 68 000.
/// Забытые 100 000 в I: 1 100 000 × 10 % = 110 000.
#[test]
fn the_first_drifted_quarter_is_found() {
    let profile = profile(&["74.10.0"]);
    let (first, second) = (quarter(2026, 1), quarter(2026, 2));
    let mut books = vec![
        income(day(2026, 1, 15), 1_000_000, None),
        income(day(2026, 4, 15), 500_000, None),
        ordinary(day(2026, 5, 10), 200_000),
    ];
    let mut closings = vec![closed(&profile, &books, &[], first, 1_770_000_000_000)];
    closings.push(closed(&profile, &books, &closings, second, 1_770_000_000_001));
    assert_eq!(scan(drift_books(&profile, &books, &[])), None, "снимков нет");
    assert_eq!(scan(drift_books(&profile, &books, &closings)), None, "данные те же");

    books.push(income(day(2026, 6, 1), 300_000, None));
    let (found, closing) = scan(drift_books(&profile, &books, &closings)).expect("II квартал разошёлся");
    assert_eq!(found, tax(Some(&profile), &books, &closings, second));
    assert_eq!(found.lines, [other(800_000, 80_000, 200_000, 12_000, 0, 36_000, 68_000, 0)]);
    assert_eq!((found.tax, closing.data().tax, closing.quarter()), (68_000, 38_000, second));

    books.push(income(day(2026, 3, 1), 100_000, None));
    let (found, closing) = scan(drift_books(&profile, &books, &closings)).expect("I квартал разошёлся");
    assert_eq!(found, tax(Some(&profile), &books, &closings, first));
    assert_eq!((found.quarter, found.tax, closing.data().tax), (first, 110_000, 100_000));

    let reopened = [closings[0].reopen(at(1_770_000_000_002)).expect("был закрыт"), closings[1].clone()];
    let (found, _) = scan(drift_books(&profile, &books, &reopened)).expect("II квартал разошёлся");
    assert_eq!(found, tax(Some(&profile), &books, &reopened, second));
    assert_eq!(found.quarter, second);
}

/// Выгрузка за период (задача 83) видит каждый закрытый квартал периода,
/// а не только первый разошедшийся: II совпадает, I и III разошлись. Расчёт
/// каждого — тот же, что у `quarter`; кварталы вне периода и не кончившиеся на
/// сегодня не идут.
///
/// I: 1 000 000 × 10 % = 100 000. II: 500 000 × 10 % = 50 000. III: 700 000 ×
/// 10 % = 70 000. Забытые 100 000 в I: 110 000; забытые 200 000 в III:
/// 900 000 × 10 % = 90 000. Расходов нет — переносу неоткуда взяться.
#[test]
fn every_closed_quarter_of_a_period_is_listed() {
    let profile = profile(&["74.10.0"]);
    let (first, second, third) = (quarter(2026, 1), quarter(2026, 2), quarter(2026, 3));
    let mut books = vec![
        income(day(2026, 1, 15), 1_000_000, None),
        income(day(2026, 4, 15), 500_000, None),
        income(day(2026, 7, 15), 700_000, None),
    ];
    let mut closings = vec![closed(&profile, &books, &[], first, 1_770_000_000_000)];
    closings.push(closed(&profile, &books, &closings, second, 1_770_000_000_001));
    closings.push(closed(&profile, &books, &closings, third, 1_770_000_000_002));
    books.push(income(day(2026, 3, 1), 100_000, None));
    books.push(income(day(2026, 9, 1), 200_000, None));
    let books = drift_books(&profile, &books, &closings);
    let today = day(2030, 1, 2);

    let year = closed_in(books, today, first..=quarter(2026, 4));
    let got: Vec<(Quarter, i64, i64, bool)> = year
        .iter()
        .map(|(now, closing)| (now.quarter, now.tax, closing.data().tax, now.drifted_from(closing)))
        .collect();
    assert_eq!(
        got,
        [(first, 110_000, 100_000, true), (second, 50_000, 50_000, false), (third, 90_000, 70_000, true)]
    );
    for (now, _) in &year {
        assert_eq!(*now, super::quarter(books, now.quarter), "расчёт {:?} — как у quarter", now.quarter);
    }
    let quarters = |list: &[(QuarterTax, &Closing)]| list.iter().map(|(now, _)| now.quarter).collect::<Vec<_>>();
    assert_eq!(quarters(&closed_in(books, today, second..=third)), [second, third], "период — II и III");
    assert!(quarters(&closed_in(books, today, quarter(2026, 4)..=quarter(2026, 4))).is_empty(), "IV не закрыт");
    assert_eq!(quarters(&closed_in(books, day(2026, 9, 30), ..)), [first, second], "III ещё идёт");
    assert_eq!(scan(books).map(|(now, _)| now.quarter), Some(first), "первый разошедшийся — I");
}

/// Цепочка идёт от предыдущего снимка и сквозь открытые кварталы между
/// снимками, а правка раньше снимка до следующего закрытого не доходит.
/// Расчёт может разойтись и при том же налоге: у снимка важен и перенос.
///
/// I квартал: доход 100 000, расход 2 000 000: вычет 120 000, налог —
/// предел 4 500, перенос 120 000 − 5 500 = 114 500. II (открыт): доход
/// 1 000 000, налог — предел 45 000, перенос 114 500 − 55 000 = 59 500.
/// III: то же, перенос 59 500 − 55 000 = 4 500. Забытый расход 1 000 000
/// во II: вычет 60 000, перенос 174 500 − 55 000 = 119 500, в III налог тот
/// же, а перенос 119 500 − 55 000 = 64 500.
#[test]
fn the_drift_chain_starts_at_the_previous_snapshot() {
    let profile = profile(&["74.10.0"]);
    let (first, third) = (quarter(2026, 1), quarter(2026, 3));
    let mut books = vec![
        income(day(2026, 1, 15), 100_000, None),
        ordinary(day(2026, 2, 10), 2_000_000),
        income(day(2026, 4, 15), 1_000_000, None),
        income(day(2026, 7, 15), 1_000_000, None),
    ];
    let mut closings = vec![closed(&profile, &books, &[], first, 1_770_000_000_000)];
    closings.push(closed(&profile, &books, &closings, third, 1_770_000_000_001));
    assert_eq!(closings[1].data().lines, [other(1_000_000, 100_000, 0, 0, 59_500, 45_000, 45_000, 4_500)]);
    assert_eq!(scan(drift_books(&profile, &books, &closings)), None, "данные те же");

    books.push(ordinary(day(2026, 5, 10), 1_000_000));
    let (found, closing) = scan(drift_books(&profile, &books, &closings)).expect("III квартал разошёлся");
    assert_eq!(found, tax(Some(&profile), &books, &closings, third));
    assert_eq!(found.lines, [other(1_000_000, 100_000, 0, 0, 119_500, 45_000, 45_000, 64_500)]);
    assert_eq!((found.tax, closing.data().tax), (45_000, 45_000));
    books.pop();

    // Расход I квартала расходится с его снимком, а III берёт перенос из снимка.
    books.push(ordinary(day(2026, 3, 1), 500_000));
    let (found, _) = scan(drift_books(&profile, &books, &closings)).expect("I квартал разошёлся");
    assert_eq!(found.quarter, first);
    assert!(!tax(Some(&profile), &books, &closings, third).differs_from(&closings[1]));
}

/// Неполный расчёт с поданным не сравнивается. Второй код другой строки
/// налога в профиле: доход без вида стал ничьим, строки пропали, хотя
/// поданное верно. Нечитаемая запись — то же. Забытый доход рядом с полным
/// расчётом находится, а его сравнение держит этот случай двусторонним.
#[test]
fn an_incomplete_calculation_is_no_drift() {
    let one = profile(&["74.10.0"]);
    let q = quarter(2026, 1);
    let mut books = vec![income(day(2026, 1, 15), 1_000_000, None)];
    let closings = [closed(&one, &books, &[], q, 1_770_000_000_000)];

    let two = profile(&["74.10.0", "62.01.0"]);
    let live = tax(Some(&two), &books, &closings, q);
    assert!(live.differs_from(&closings[0]) && live.known().is_none());
    assert_eq!(scan(drift_books(&two, &books, &closings)), None, "второй код в профиле");

    books.push(income(day(2026, 3, 1), 100_000, None));
    assert!(scan(drift_books(&one, &books, &closings)).is_some(), "забытый доход");
    let unreadable = Books {
        unreadable: 1,
        ..drift_books(&one, &books, &closings)
    };
    assert_eq!(scan(unreadable), None, "нечитаемая запись");
}

/// Сумму квартала можно назвать, только когда расчёт полон и весь квартал на
/// налоге с оборота: дохода нет — это ноль, доход без курса рядом с посчитанным
/// — ни ноль, ни часть (Правило 6).
#[test]
fn only_a_whole_calculation_has_a_sum() {
    let one = profile(&["74.10.0"]);
    let q = quarter(2026, 1);
    let amd = income(day(2026, 1, 15), 1_000_000, None);
    let usd = usd_income(day(2026, 1, 20), 10_000, None);
    assert_eq!(tax(Some(&one), &[], &[], q).known(), Some(0), "дохода нет");
    assert_eq!(tax(Some(&one), std::slice::from_ref(&amd), &[], q).known(), Some(100_000), "доход посчитан");
    let both = tax(Some(&one), &[amd.clone(), usd.clone()], &[], q);
    assert_eq!((both.tax, both.known()), (100_000, None), "часть дохода без курса");
    assert_eq!(tax(Some(&one), std::slice::from_ref(&usd), &[], q).known(), None, "весь доход без курса");
    assert_eq!(tax(None, &[amd], &[], q).known(), None, "кодов нет");
}

/// Сверяется то, что идёт в форму: тот же расход отнесли к виду деятельности,
/// расходы строки разделились иначе, а поля x.1–x.11 те же — нового расчёта
/// подавать незачем.
#[test]
fn a_new_split_of_expenses_is_no_drift() {
    let profile = profile(&["74.10.0"]);
    let q = quarter(2026, 1);
    let sale = income(day(2026, 1, 15), 1_000_000, None);
    let shared = ordinary(day(2026, 2, 10), 200_000);
    let closings = [closed(&profile, &[sale.clone(), shared], &[], q, 1_770_000_000_000)];
    let attributed = expense(day(2026, 2, 10), 200_000, Proof::SettlementInvoice, Nature::Ordinary, Some("74.10.0"));
    let books = [sale, attributed];
    let live = tax(Some(&profile), &books, &closings, q);
    assert_ne!(live.lines, closings[0].data().lines, "расходы разделились иначе");
    assert!(!live.differs_from(&closings[0]));
    assert_eq!(scan(drift_books(&profile, &books, &closings)), None);
}

/// Уход с режима — расхождение и при тех же строках: новый доход по коду,
/// закрывающему налог с оборота, в строки не идёт, а суммы у квартала больше
/// нет — его считает бухгалтер.
#[test]
fn leaving_after_closing_is_a_drift() {
    let profile = profile(&["74.10.0", "70.22.0"]);
    let q = quarter(2026, 4);
    let mut books = vec![income(day(2026, 10, 5), 1_000_000, Some("74.10.0"))];
    let closings = [closed(&profile, &books, &[], q, 1_770_000_000_000)];
    assert_eq!(scan(drift_books(&profile, &books, &closings)), None, "данные те же");
    books.push(income(day(2026, 12, 20), 300_000, Some("70.22.0")));
    let (got, closing) = scan(drift_books(&profile, &books, &closings)).expect("квартал разошёлся");
    assert!(matches!(got.regime, Regime::Left { .. }));
    assert_eq!(got.lines, closing.data().lines);
    assert_eq!(got.known(), None);
}

/// Квартал, не кончившийся на сегодня, не сверяется: такой не закрыть, а снимок
/// далёкого года из чужой копии растянул бы цепочку. Кончился — сверяется.
#[test]
fn a_quarter_still_going_is_not_compared() {
    let profile = profile(&["74.10.0"]);
    let q = quarter(2026, 1);
    let books = [income(day(2026, 1, 15), 1_000_000, None)];
    let closings = [closed(&profile, &books, &[], q, 1_770_000_000_000)];
    assert_eq!(drifted(drift_books(&profile, &[], &closings), day(2026, 3, 31)), None, "последний день квартала");
    assert!(drifted(drift_books(&profile, &[], &closings), day(2026, 4, 1)).is_some(), "квартал кончился");
}

/// Живых снимков квартала два (слияние копий, задача 41): сверяется
/// изменённый последним, как и в расчёте.
#[test]
fn the_latest_snapshot_is_the_one_compared() {
    let profile = profile(&["74.10.0"]);
    let q = quarter(2026, 1);
    let mut books = vec![income(day(2026, 1, 15), 1_000_000, None)];
    let older = closed(&profile, &books, &[], q, 1_770_000_000_000);
    books.push(income(day(2026, 3, 1), 100_000, None));
    let newer = closed(&profile, &books, &[], q, 1_770_000_000_001);
    let closings = [newer.clone(), older];
    assert_eq!(scan(drift_books(&profile, &books, &closings)), None);
    books.pop();
    let (found, closing) = scan(drift_books(&profile, &books, &closings)).expect("разошёлся с новым");
    assert_eq!((found.tax, closing.id()), (100_000, newer.id()));
}

/// Квартал раньше правил налога не закрывается: снимок такого квартала чтение
/// считает порчей (`ClosingData::check`), и окно вечно говорило бы о нечитаемом
/// закрытом квартале. Кончившийся пустой квартал рядом с правилами закрывается.
#[test]
fn a_quarter_before_the_rules_does_not_close() {
    let profile = profile(&["74.10.0"]);
    let today = day(2026, 11, 1);
    let before = tax(Some(&profile), &[], &[], quarter(2024, 4));
    assert!(!before.closable(today));
    assert!(Closing::close(&before, today, at(1)).is_err());
    let first = tax(Some(&profile), &[], &[], quarter(2025, 1));
    assert!(first.closable(today));
    let closing = Closing::close(&first, today, at(1)).expect("квартал правил закрывается");
    let text = serde_json::to_string(&closing).expect("снимок пишется");
    assert_eq!(serde_json::from_str::<Closing>(&text).ok(), Some(closing), "снимок читается обратно");
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

/// Квартал, кончившийся до дня регистрации ИП, — не его (`NotYet`, по образцу
/// года до регистрации): закрыть нельзя, чисел для формы нет, а записи книги
/// в нём названы числом. Квартал регистрации — обычный, и когда она в его
/// последний день; без дня регистрации — тоже: неизвестное — не «ещё не был»
/// (Правило 6). Снимок, закрытый раньше, остаётся поданным расчётом. ИП —
/// как в полном прогоне 2026-11-10: зарегистрирован 6 ноября 2026 года.
#[test]
fn a_quarter_before_registration_is_not_the_entrepreneurs() {
    use super::super::closing::CloseError;
    use super::super::report::{self, Missing};
    let registered = |on: Option<Date>| {
        let mut data = profile(&["62.01.0"]);
        data.person.registered = on;
        data
    };
    let (third, fourth) = (quarter(2026, 3), quarter(2026, 4));
    let november = registered(Some(day(2026, 11, 6)));
    let today = day(2026, 11, 10);
    let mut failures = Vec::new();

    let before = tax(Some(&november), &[], &[], third);
    if before.not_yet != Some(NotYet { registered: day(2026, 11, 6), records: 0 }) {
        failures.push(format!("III квартал до регистрации: {:?}", before.not_yet));
    }
    if before.closable(today) {
        failures.push("III квартал до регистрации закрывается".to_owned());
    }
    match Closing::close(&before, today, at(1)) {
        Err(CloseError::NotYet) => {}
        other => failures.push(format!("закрытие III квартала: {:?}", other.map(|_| ()))),
    }
    match report::report(&before, None) {
        Err(Missing::NotYet { registered }) if registered == day(2026, 11, 6) => {}
        other => failures.push(format!("числа III квартала: {:?}", other.map(|_| ()))),
    }

    // Квартал регистрации — обычный: закрывается, когда кончился.
    let own = tax(Some(&november), &[], &[], fourth);
    if own.not_yet.is_some() || !own.closable(day(2027, 1, 10)) || report::report(&own, None).is_err() {
        failures.push(format!("IV квартал регистрации: {:?}", own.not_yet));
    }

    // Граница — последний день квартала: регистрация в нём делает квартал
    // своим, а 1 октября — уже следующим.
    for (on, expected) in [(day(2026, 9, 30), false), (day(2026, 10, 1), true)] {
        let got = tax(Some(&registered(Some(on))), &[], &[], third).not_yet.is_some();
        if got != expected {
            failures.push(format!("регистрация {on:?}: «ещё не ИП» — {got}, а ждали {expected}"));
        }
    }

    // Без дня регистрации — обычный кончившийся квартал.
    let unknown = registered(None);
    let plain = tax(Some(&unknown), &[], &[], third);
    if plain.not_yet.is_some() || !plain.closable(today) {
        failures.push(format!("без дня регистрации: {:?}", plain.not_yet));
    }

    // Доход и расход до регистрации — названы числом.
    let books = [income(day(2026, 8, 15), 100_000, None), ordinary(day(2026, 9, 1), 20_000)];
    let with_records = tax(Some(&november), &books, &[], third);
    if with_records.not_yet.map(|not_yet| not_yet.records) != Some(2) {
        failures.push(format!("записи до регистрации: {:?}", with_records.not_yet));
    }

    // Закрытый раньше, когда дня регистрации в профиле не было, — его снимок.
    let closing = Closing::close(&plain, today, at(1)).expect("без дня регистрации квартал закрывается");
    let closings = [closing.clone()];
    let after = tax(Some(&november), &[], &closings, third);
    if !matches!(report::report(&after, Some(&closing)), Ok(report::Ready { .. })) {
        failures.push("снимок, закрытый до дня регистрации, не отдаёт числа".to_owned());
    }

    assert!(failures.is_empty(), "{failures:#?}");
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
            openings: &[],
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

/// Выпущенный акт на 1 500 000 ֏ за дизайн с кодом 74.10.0.
const ISSUED_ACT: &str = concat!(
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
);

/// Доход из выпущенного акта — со своим видом деятельности: дизайн по акту
/// у ИП с IT и дизайном идёт в прочую деятельность.
#[test]
fn documents_bring_their_activity() {
    let act: Document = serde_json::from_str(ISSUED_ACT).expect("акт читается");
    let profile = profile(&["62.01.0", "74.10.0"]);
    let got = quarter_of(&[act], Some(&profile), &[], &[], quarter(2026, 4));
    assert_eq!(got.lines, [other(1_500_000, 150_000, 0, 0, 0, 67_500, 150_000, 0)]);
}

/// Строка налога дохода важнее строки кода (ст. 258, ч. 1): у IT 1 % на
/// услуги, а лицензия как роялти и проданный ноутбук — по 10 % без вычета.
///
/// 1 000 000 × 1 % = 10 000; 300 000 × 10 % = 30 000; 200 000 × 10 % = 20 000;
/// итого 60 000. Строкой кода было бы 15 000.
#[test]
fn the_income_row_beats_the_row_of_the_code() {
    let profile = profile(&["62.01.0"]);
    let books = [
        income(day(2026, 4, 10), 1_000_000, None),
        income_as(day(2026, 5, 10), 300_000, IncomeKind::Rent),
        income_as(day(2026, 6, 10), 200_000, IncomeKind::Assets),
    ];
    let got = tax(Some(&profile), &books, &[], quarter(2026, 2));
    assert_eq!(
        got.lines,
        [
            plain(IncomeKind::Rent, 300_000, "10", 30_000),
            plain(IncomeKind::HighTech, 1_000_000, "1", 10_000),
            plain(IncomeKind::Assets, 200_000, "10", 20_000),
        ]
    );
    assert_eq!(got.tax, 60_000);
    assert!(got.complete());
    assert_eq!(got.year_turnover, 1_500_000);
}

/// Строка дохода есть и в документе: тот же акт — продажа имущества, хотя код
/// у него есть.
#[test]
fn documents_bring_their_income_row() {
    let text = ISSUED_ACT.replacen(r#""activity":"74.10.0""#, r#""activity":"74.10.0","income_kind":"assets""#, 1);
    let act: Document = serde_json::from_str(&text).expect("акт читается");
    let profile = profile(&["62.01.0", "74.10.0"]);
    let got = quarter_of(&[act], Some(&profile), &[], &[], quarter(2026, 4));
    assert_eq!(got.lines, [plain(IncomeKind::Assets, 1_500_000, "10", 150_000)]);
}

/// Продажа имущества — в обороте всех видов: её вес уменьшает долю общих
/// расходов у строки с вычетом (ст. 260, ч. 3), хотя у самой строки 9 вычета
/// нет. IT и дизайн по 300 000, продажа 400 000, общие 500 000: доля дизайна
/// 500 000 × 300 000 / 1 000 000 = 150 000, вычет 9 000, налог 30 000 − 9 000
/// = 21 000; IT 3 000, строка 9 — 40 000. Без продажи доля была бы 250 000.
#[test]
fn an_income_row_weighs_in_the_split_of_shared_expenses() {
    let profile = profile(&["62.01.0", "74.10.0"]);
    let books = [
        income(day(2026, 7, 10), 300_000, Some("62.01.0")),
        income(day(2026, 7, 20), 300_000, Some("74.10.0")),
        income_as(day(2026, 8, 10), 400_000, IncomeKind::Assets),
        ordinary(day(2026, 9, 1), 500_000),
    ];
    let got = tax(Some(&profile), &books, &[], quarter(2026, 3));
    assert_eq!(
        got.lines,
        [
            plain(IncomeKind::HighTech, 300_000, "1", 3_000),
            plain(IncomeKind::Assets, 400_000, "10", 40_000),
            other(300_000, 30_000, 150_000, 9_000, 0, 13_500, 21_000, 0),
        ]
    );
    assert_eq!(got.tax, 64_000);
    assert_eq!((got.refused.no_base, got.refused.split), (0, 1));
}

/// Торговец продал кассовый аппарат: прямой расход и товар ст. 260, ч. 3 не
/// делит (ст. 258, ч. 6), но книга этого не знает. Без вида расход общий, его доля
/// уходит на строку 9, и окно это называет; с кодом идёт в свою строку целиком.
///
/// Выручка 1 000 000 ֏, продажа 1 000 000 ֏, расход 900 000 ֏. Без вида: доля
/// 450 000, вычет 42 750, налог 57 250, строка 9 — 100 000, итого 157 250.
/// С кодом: 100 000 − 85 500 = 14 500 (предел 10 000), итого 114 500.
#[test]
fn an_expense_with_its_code_keeps_out_of_the_split() {
    let mut profile = profile(&["47.91.0"]);
    profile.kinds.insert(code("47.91.0"), IncomeKind::Trade);
    let sales = [income(day(2026, 7, 10), 1_000_000, None), income_as(day(2026, 8, 10), 1_000_000, IncomeKind::Assets)];
    let shared = [sales[0].clone(), sales[1].clone(), ordinary(day(2026, 9, 1), 900_000)];
    let got = tax(Some(&profile), &shared, &[], quarter(2026, 3));
    assert_eq!(got.tax, 157_250);
    assert_eq!(got.refused.split, 1);
    assert!(got.complete());

    let own = [
        sales[0].clone(),
        sales[1].clone(),
        expense(day(2026, 9, 1), 900_000, Proof::SettlementInvoice, Nature::Ordinary, Some("47.91.0")),
    ];
    let got = tax(Some(&profile), &own, &[], quarter(2026, 3));
    assert_eq!(got.tax, 114_500);
    assert_eq!(got.refused.split, 0);
}

/// Весь доход квартала — со своей строкой: вес строки с вычетом — ноль, общий
/// расход пропадает, и окно это говорит, как у IT (ст. 260, ч. 3). Расход
/// с кодом переносится (ст. 258, ч. 7, «в том числе при отсутствии базы»).
#[test]
fn only_an_income_row_leaves_shared_expenses_without_a_base() {
    let design = profile(&["74.10.0"]);
    let sale = income_as(day(2026, 8, 10), 400_000, IncomeKind::Rent);
    let shared = [sale.clone(), ordinary(day(2026, 9, 1), 500_000)];
    let got = tax(Some(&design), &shared, &[], quarter(2026, 3));
    assert_eq!(got.lines, [plain(IncomeKind::Rent, 400_000, "10", 40_000)]);
    assert_eq!((got.refused.no_base, got.refused.split), (1, 0));
    assert!(got.complete());

    let own = [sale, expense(day(2026, 9, 1), 500_000, Proof::SettlementInvoice, Nature::Ordinary, Some("74.10.0"))];
    let got = tax(Some(&design), &own, &[], quarter(2026, 3));
    let line = got.lines.iter().find(|line| line.kind == IncomeKind::Other).expect("строка прочей деятельности");
    assert_eq!(line.deduction.map(|deduction| (deduction.current, deduction.carry_out)), Some((30_000, 30_000)));
    assert_eq!(got.refused.no_base, 0);
}

/// У IT расходы не вычитаются вовсе: о доле, ушедшей на строку без вычета,
/// говорить незачем, и совет указать код был бы пустым.
#[test]
fn without_a_deduction_there_is_no_share_to_name() {
    let it = profile(&["62.01.0"]);
    let books = [
        income(day(2026, 7, 10), 300_000, None),
        income_as(day(2026, 8, 10), 200_000, IncomeKind::Rent),
        ordinary(day(2026, 9, 1), 500_000),
    ];
    let got = tax(Some(&it), &books, &[], quarter(2026, 3));
    assert_eq!(got.refused, Refused::default());
    assert!(got.complete());
}

/// Вид у расхода спрашивается при одном коде, только когда это что-то меняет:
/// в квартале есть доход со своей строкой, а у кода есть вычет. Другой квартал,
/// IT без вычета и книга без такого дохода — не спрашивается.
#[test]
fn the_expense_form_asks_the_code_only_beside_an_income_row() {
    let day_of = day(2026, 8, 20);
    let sold = [income_as(day(2026, 8, 10), 400_000, IncomeKind::Assets)];
    let plain_income = [income(day(2026, 8, 10), 400_000, None)];
    let design = profile(&["74.10.0"]);
    let it = profile(&["62.01.0"]);
    assert!(expense_activity_needed(&[], &sold, Some(&design), day_of));
    assert!(!expense_activity_needed(&[], &sold, Some(&design), day(2026, 10, 1)));
    assert!(!expense_activity_needed(&[], &sold, Some(&it), day_of));
    assert!(!expense_activity_needed(&[], &plain_income, Some(&design), day_of));
    assert!(!expense_activity_needed(&[], &sold, None, day_of));
}

/// Без кодов в профиле доход со своей строкой посчитан, а общему расходу
/// некуда идти: какая строка с вычетом у ИП, неизвестно. Окно называет это,
/// и квартал не закрывается (Правило 2), а не проглатывает расход молча.
#[test]
fn an_income_row_without_codes_leaves_expenses_unknown() {
    let none = profile(&[]);
    let books = [income_as(day(2026, 8, 10), 400_000, IncomeKind::Rent), ordinary(day(2026, 9, 1), 500_000)];
    let got = tax(Some(&none), &books, &[], quarter(2026, 3));
    assert!(got.gaps.no_codes);
    assert!(!got.complete());
    let got = tax(Some(&none), &books[..1], &[], quarter(2026, 3));
    assert!(got.complete());
}

/// Продажа имущества идёт в оборот для порога (ст. 255, ч. 1, п. 2): 114 млн
/// за квартиру в феврале и 2 млн услуг в августе — порог перейдён на услуге,
/// и под налог с оборота идёт только 1 млн из неё.
#[test]
fn an_income_row_counts_towards_the_threshold() {
    let profile = profile(&["62.01.0"]);
    let books = [
        income_as(day(2026, 2, 10), 114_000_000, IncomeKind::Assets),
        income(day(2026, 8, 10), 2_000_000, None),
    ];
    let first = tax(Some(&profile), &books, &[], quarter(2026, 1));
    assert_eq!(first.lines, [plain(IncomeKind::Assets, 114_000_000, "10", 11_400_000)]);
    let got = tax(Some(&profile), &books, &[], quarter(2026, 3));
    assert_eq!(got.lines, [plain(IncomeKind::HighTech, 1_000_000, "1", 10_000)]);
    assert_eq!(
        got.regime,
        Regime::Left {
            from: day(2026, 8, 10),
            why: Leaving::Threshold,
        }
    );
    assert_eq!(got.year_turnover, 116_000_000);
}

/// Доходу со своей строкой код не нужен: строка известна, хотя коды профиля
/// облагаются по-разному, а среди них есть закрывающий налог с оборота. Так же
/// и без кодов в профиле вовсе.
#[test]
fn an_income_row_needs_no_code() {
    let books = [income_as(day(2026, 8, 1), 1_000_000, IncomeKind::Rent)];
    let mut wrong = Vec::new();
    for codes in [&["62.01.0", "74.10.0"][..], &["69.20.0", "73.11.0"], &[]] {
        let profile = profile(codes);
        let got = tax(Some(&profile), &books, &[], quarter(2026, 3));
        if !got.complete() || got.lines != [plain(IncomeKind::Rent, 1_000_000, "10", 100_000)] {
            wrong.push(format!("{codes:?}: {:?} {:?}", got.gaps, got.lines));
        }
        if got.regime != Regime::Turnover {
            wrong.push(format!("{codes:?}: {:?}", got.regime));
        }
    }
    assert!(wrong.is_empty(), "доход со своей строкой: {wrong:#?}");
}

/// Режим решает деятельность, а не проданный ноутбук: доход со своей строкой
/// не становится доходом по единственному коду, закрывающему налог с оборота.
/// А код, указанный у самого дохода, решает и при своей строке: окно о нём
/// предупредило.
#[test]
fn an_income_row_does_not_decide_the_regime() {
    let only = profile(&["70.22.0"]);
    let sold = [income_as(day(2026, 8, 1), 1_000_000, IncomeKind::Assets)];
    let got = tax(Some(&only), &sold, &[], quarter(2026, 3));
    assert_eq!(got.regime, Regime::Turnover);
    assert_eq!(got.lines, [plain(IncomeKind::Assets, 1_000_000, "10", 100_000)]);

    let coded = [operation(OperationData {
        activity: Some(code("70.22.0")),
        ..sold[0].data().clone()
    })];
    let got = tax(Some(&only), &coded, &[], quarter(2026, 3));
    assert_eq!(
        got.regime,
        Regime::Left {
            from: day(2026, 8, 1),
            why: Leaving::Code(code("70.22.0")),
        }
    );
}

/// Кто подал заявление об общепите, считает только по строкам 6 и 7 (ст. 258,
/// ч. 8): его проданный актив и аренда — строка 7, 20 %. Заявление программа
/// видит по строке 7 у кода профиля; без неё общепит — обычный плательщик,
/// и строка 9 остаётся.
#[test]
fn a_catering_declaration_leaves_only_its_two_rows() {
    let mut declared = profile(&["56.10.1", "47.91.0"]);
    declared.kinds.insert(code("56.10.1"), IncomeKind::Catering);
    declared.kinds.insert(code("47.91.0"), IncomeKind::CateringOther);
    let books = [
        income_as(day(2026, 8, 1), 1_000_000, IncomeKind::Assets),
        income_as(day(2026, 8, 2), 500_000, IncomeKind::Rent),
    ];
    let got = tax(Some(&declared), &books, &[], quarter(2026, 3));
    assert_eq!(got.lines, [plain(IncomeKind::CateringOther, 1_500_000, "20", 300_000)]);

    let mut plain_catering = profile(&["56.10.1"]);
    plain_catering.kinds.insert(code("56.10.1"), IncomeKind::Catering);
    let got = tax(Some(&plain_catering), &books[..1], &[], quarter(2026, 3));
    assert_eq!(got.lines, [plain(IncomeKind::Assets, 1_000_000, "10", 100_000)]);
}

/// Общепит в 2025 году: только вне Еревана считали по прежним правилам
/// (ՀՕ-285-Ն, ст. 16, ч. 6), и места программа не спрашивает — окно говорит
/// это, а квартал закрывается, потому что у ресторана в Ереване расчёт верен.
/// С 2026 года и у других строк слов нет.
#[test]
fn catering_before_2026_says_the_place_matters() {
    let mut catering = profile(&["56.10.1"]);
    catering.kinds.insert(code("56.10.1"), IncomeKind::Catering);
    let cases = [
        ("IV квартал 2025 года", &catering, day(2025, 12, 31), quarter(2025, 4), true),
        ("I квартал 2026 года", &catering, day(2026, 1, 1), quarter(2026, 1), false),
    ];
    let other = profile(&["74.10.0"]);
    let mut wrong = Vec::new();
    for (case, data, date, q, expected) in cases {
        let got = tax(Some(data), &[income(date, 1_000_000, None)], &[], q);
        if got.place_unknown() != expected {
            wrong.push(format!("{case}: {:?}", got.lines));
        }
        if !got.closable(day(2026, 11, 2)) {
            wrong.push(format!("{case}: квартал не закрывается: {:?}", got.gaps));
        }
    }
    if tax(Some(&other), &[income(day(2025, 6, 1), 1_000_000, None)], &[], quarter(2025, 2)).place_unknown() {
        wrong.push("прочая деятельность в 2025 году".to_owned());
    }
    if tax(Some(&catering), &[], &[], quarter(2025, 2)).place_unknown() {
        wrong.push("квартал без дохода".to_owned());
    }
    assert!(wrong.is_empty(), "место общепита: {wrong:#?}");
}

/// Заявление об общепите программа видит только по строке 7 у кода (ст. 258,
/// ч. 8): рядом с общепитом другая строка — окно спрашивает о заявлении, и у
/// заявителя с кодом другой строки тоже, а без общепита, с одним общепитом или
/// когда весь прочий доход уже в строке 7 — не спрашивает.
#[test]
fn a_second_row_next_to_catering_asks_about_the_declaration() {
    let mut catering = profile(&["56.10.1"]);
    catering.kinds.insert(code("56.10.1"), IncomeKind::Catering);
    let mut declared = catering.clone();
    declared.activities.push(code("47.91.0"));
    declared.kinds.insert(code("47.91.0"), IncomeKind::CateringOther);
    let books = [
        income(day(2026, 8, 1), 1_000_000, Some("56.10.1")),
        income_as(day(2026, 8, 2), 500_000, IncomeKind::Rent),
        income(day(2026, 8, 3), 200_000, Some("47.91.0")),
    ];
    let mut wrong = Vec::new();
    let q = quarter(2026, 3);
    if !tax(Some(&catering), &books[..2], &[], q).declaration_unknown() {
        wrong.push("общепит и аренда: слов нет");
    }
    if tax(Some(&catering), &books[..1], &[], q).declaration_unknown() {
        wrong.push("один общепит: слова есть");
    }
    if tax(Some(&declared), &books, &[], q).declaration_unknown() {
        wrong.push("строка 7 уже есть: слова есть");
    }
    let mut third = declared.clone();
    third.activities.push(code("74.10.0"));
    let design = [income(day(2026, 8, 4), 300_000, Some("74.10.0"))];
    let all: Vec<Operation> = books.iter().chain(&design).cloned().collect();
    if !tax(Some(&third), &all, &[], q).declaration_unknown() {
        wrong.push("заявитель с кодом другой строки: слов нет");
    }
    if tax(Some(&profile(&["74.10.0"])), &books[1..2], &[], q).declaration_unknown() {
        wrong.push("аренда без общепита: слова есть");
    }
    assert!(wrong.is_empty(), "заявление об общепите: {wrong:#?}");
}

/// Строку дохода выбрали после закрытия квартала — расчёт разошёлся со снимком:
/// 1 000 000 × 1 % = 10 000 поданных против 100 000 по строке 5.
#[test]
fn choosing_the_row_after_closing_is_a_drift() {
    let profile = profile(&["62.01.0"]);
    let before = [income(day(2026, 8, 1), 1_000_000, None)];
    let closings = [closed(&profile, &before, &[], quarter(2026, 3), 1_760_000_001_000)];
    assert_eq!(scan(drift_books(&profile, &before, &closings)), None);
    let after = [operation(OperationData {
        income_kind: Some(IncomeKind::Rent),
        ..before[0].data().clone()
    })];
    let (found, closing) = scan(drift_books(&profile, &after, &closings)).expect("расхождение найдено");
    assert_eq!(found.tax, 100_000);
    assert_eq!(closing.data().tax, 10_000);
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
    let (documents, operations) = five_years();
    let profile = profile(&["74.10.0"]);
    let started = std::time::Instant::now();
    let got = quarter_of(&documents, Some(&profile), &operations, &[], quarter(2029, 4));
    let elapsed = started.elapsed();
    println!("цепочка 20 кварталов, {} записей: {elapsed:?}, налог {}", documents.len() + operations.len(), got.tax);
}

/// Замер: все 20 кварталов закрыты, и поиск расхождения проходит их все —
/// так на каждое изменение данных. Рядом — те же кварталы отдельными
/// `quarter`, чего `drifted` избегает. Запуск:
/// `cargo test -p kvartik-core --release --lib drift_timing -- --ignored --nocapture`.
#[test]
#[ignore = "замер, а не проверка"]
fn drift_timing() {
    let (documents, operations) = five_years();
    let profile = profile(&["74.10.0"]);
    let quarters: Vec<Quarter> = (2025..=2029_u16)
        .flat_map(|year| (1..=4_u8).map(move |number| quarter(year, number)))
        .collect();
    let mut closings = Vec::new();
    for q in &quarters {
        let tax = quarter_of(&documents, Some(&profile), &operations, &closings, *q);
        closings.push(Closing::close(&tax, day(2030, 1, 2), at(1_770_000_000_000)).expect("квартал кончился и посчитан"));
    }
    let books = Books {
        documents: &documents,
        operations: &operations,
        profile: Some(&profile),
        closings: &closings,
        openings: &[],
        unreadable: 0,
    };
    let started = std::time::Instant::now();
    let got = drifted(books, day(2030, 1, 2));
    let once = started.elapsed();
    let started = std::time::Instant::now();
    let empty = quarters.iter().filter(|q| super::quarter(books, **q).lines.is_empty()).count();
    let apart = started.elapsed();
    assert!(got.is_none(), "данные те же, а расчёт разошёлся со снимком");
    println!(
        "20 закрытых кварталов, {} записей: {once:?}, отдельными quarter {apart:?} (пустых {empty})",
        documents.len() + operations.len()
    );
}

/// Книга на пять лет: по 600 актов и 1000 записей в год.
fn five_years() -> (Vec<Document>, Vec<Operation>) {
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
    (documents, operations)
}
