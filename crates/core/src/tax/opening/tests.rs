//! Тесты переноса с кварталов до программы. Образец записи выписан человеком,
//! а не записан кодом (Правило 2).

use super::*;

/// Перенос на начало III квартала 2026 года: прочая деятельность, 12 000 ֏.
const OPENING: &str = concat!(
    r#"{"meta":{"id":"7c1d2e3f-4a5b-4c6d-8e7f-0a1b2c3d4e5f","modified":1760000000000,"deleted":null},"#,
    r#""data":{"year":2026,"quarter":3,"carry":{"other":12000}}}"#
);

fn quarter(year: u16, number: u8) -> Quarter {
    Quarter::new(year, number).expect("в тесте — существующий квартал")
}

fn at(millis: i64) -> Timestamp {
    Timestamp::from_unix_millis(millis)
}

fn other(amount: i64) -> BTreeMap<IncomeKind, i64> {
    BTreeMap::from([(IncomeKind::Other, amount)])
}

/// Образец читается в ожидаемые значения, а его запись — слово в слово
/// образец. Поменялась запись — это новая версия формата (Правило 5).
#[test]
fn data_shape_is_pinned() {
    let opening: Opening = serde_json::from_str(OPENING).expect("запись читается");
    assert_eq!(opening.quarter(), quarter(2026, 3));
    assert_eq!(opening.carry(), &other(12_000));
    assert_eq!(serde_json::to_string(&opening).ok().as_deref(), Some(OPENING));
}

/// Запись, которой команда не пишет, — порча: квартал не из календаря или
/// раньше правил налога, сумма не больше нуля, незнакомые поле и строка.
#[test]
fn data_rejects_what_save_never_writes() {
    let cases = [
        OPENING.replace(r#""quarter":3"#, r#""quarter":5"#),
        OPENING.replace(r#""year":2026"#, r#""year":2024"#),
        OPENING.replace(r#""year":2026"#, r#""year":1"#),
        OPENING.replace(r#""other":12000"#, r#""other":0"#),
        OPENING.replace(r#""other":12000"#, r#""other":-1"#),
        OPENING.replace(r#""other":12000"#, r#""barter":12000"#),
        OPENING.replace(r#"}}}"#, r#"},"note":1}}"#),
    ];
    let mut wrong = Vec::new();
    for text in cases {
        assert_ne!(text, OPENING, "образец поправлен");
        if let Ok(opening) = serde_json::from_str::<Opening>(&text) {
            wrong.push(format!("{text} прочитан как {opening:?}"));
        }
    }
    assert!(wrong.is_empty(), "прочитано запретное: {wrong:#?}");
}

/// Переноса не было — запись без сумм, и нули не хранятся: ответ «ничего»
/// отличается от «неизвестно». Что не изменилось, не записывается.
#[test]
fn saving_drops_zeros_and_skips_the_same() {
    let none = Opening::save(None, quarter(2026, 3), other(0), at(1)).expect("годно").expect("новая");
    assert!(none.carry().is_empty());
    let saved = Opening::save(None, quarter(2026, 3), other(12_000), at(1)).expect("годно").expect("новая");
    assert_eq!(saved.carry(), &other(12_000));
    let same = Opening::save(Some(saved.clone()), quarter(2026, 3), other(12_000), at(2));
    assert_eq!(same, Ok(None));

    let changed = Opening::save(Some(saved.clone()), quarter(2026, 3), other(5_000), at(2)).expect("годно").expect("правка");
    assert_eq!((changed.id(), changed.modified()), (saved.id(), at(2)));
    assert_eq!(changed.carry(), &other(5_000));
}

/// Сумма меньше нуля — порча; строка без вычета переноса не знает, а из
/// расчётов до 2025 года он есть только у торговли (ՀՕ-285-Ն, ст. 16, ч. 3).
#[test]
fn saving_checks_the_rule_of_the_quarter() {
    let q = quarter(2026, 3);
    assert_eq!(Opening::save(None, q, other(-1), at(1)), Err(OpeningError::Negative));
    let high_tech = BTreeMap::from([(IncomeKind::HighTech, 1_000)]);
    assert_eq!(Opening::save(None, q, high_tech, at(1)), Err(OpeningError::Kind(IncomeKind::HighTech)));

    let first = quarter(2025, 1);
    assert_eq!(Opening::save(None, first, other(1_000), at(1)), Err(OpeningError::Kind(IncomeKind::Other)));
    let trade = BTreeMap::from([(IncomeKind::Trade, 1_000)]);
    assert!(Opening::save(None, first, trade.clone(), at(1)).is_ok());
    assert!(Opening::save(None, quarter(2025, 2), other(1_000), at(1)).is_ok());
    assert!(Opening::save(None, quarter(2025, 2), trade, at(1)).is_ok());
}

/// Строки по правилам: на начало 2025 года — только торговля, дальше — все
/// строки с вычетом (НК, ст. 258, ч. 7); до правил налога строк нет.
#[test]
fn the_rule_names_the_kinds() {
    let all = [IncomeKind::Trade, IncomeKind::Production, IncomeKind::Catering, IncomeKind::Other];
    assert_eq!(kinds(quarter(2025, 1)), [IncomeKind::Trade]);
    assert_eq!(kinds(quarter(2025, 2)), all);
    assert_eq!(kinds(quarter(2026, 3)), all);
    assert_eq!(kinds(quarter(2024, 4)), []);
}

/// Запись квартала — его живая, изменённая последней; помеченная не
/// считается, а запись другого квартала — не его.
#[test]
fn the_latest_live_record_of_the_quarter_wins() {
    let q = quarter(2026, 3);
    let older = Opening::save(None, q, other(1_000), at(1)).expect("годно").expect("новая");
    let newer = Opening::save(None, q, other(2_000), at(2)).expect("годно").expect("новая");
    let elsewhere = Opening::save(None, quarter(2026, 2), other(3_000), at(3)).expect("годно").expect("новая");
    let all = [older.clone(), newer.clone(), elsewhere.clone()];
    assert_eq!(Opening::current(&all, q).map(Opening::id), Some(newer.id()));
    assert_eq!(Opening::current(&all, quarter(2026, 2)).map(Opening::id), Some(elsewhere.id()));
    let mut gone = newer.clone();
    gone.meta.delete(at(3));
    let marked = [older.clone(), gone];
    assert_eq!(Opening::current(&marked, q).map(Opening::id), Some(older.id()));
    assert_eq!(Opening::current(&[], q), None);
}
