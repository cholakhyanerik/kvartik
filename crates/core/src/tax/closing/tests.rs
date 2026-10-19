//! Тесты снимка закрытого квартала. Образец записи выписан человеком, а не
//! записан кодом (Правило 2).

use super::*;
use crate::tax::rules::Percent;
use crate::tax::turnover::Deduction;

/// Снимок III квартала 2026 года: прочая деятельность, доход 1 000 000 ֏,
/// расходы 950 000 ֏ — налог упёрся в предел 45 000, перенос 2 000 ֏.
const CLOSING: &str = concat!(
    r#"{"meta":{"id":"0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d","modified":1760000000000,"deleted":null},"#,
    r#""data":{"year":2026,"quarter":3,"rules":"2026.2","lines":[{"kind":"other","income":1000000,"#,
    r#""rate":"10","gross":100000,"deduction":{"percent":"6","floor_percent":"4.5","attributed":0,"#,
    r#""shared":950000,"expenses":950000,"current":57000,"carry_in":0,"total":57000,"floor":45000,"#,
    r#""carry_out":2000},"tax":45000}],"tax":45000}}"#
);

fn percent(text: &str) -> Percent {
    Percent::parse(text).expect("в тесте — годный процент")
}

/// Образец читается в ожидаемые значения, а его запись — слово в слово
/// образец. Поменялась запись — это новая версия формата (Правило 5).
#[test]
fn data_shape_is_pinned() {
    let closing: Closing = serde_json::from_str(CLOSING).expect("снимок читается");
    assert_eq!(closing.quarter(), Quarter::new(2026, 3).expect("квартал"));
    assert_eq!(closing.data().rules.to_string(), "2026.2");
    assert_eq!(closing.data().tax, 45_000);
    let line = &closing.data().lines[0];
    assert_eq!((line.kind, line.income, line.rate), (IncomeKind::Other, 1_000_000, percent("10")));
    assert_eq!(
        line.deduction,
        Some(Deduction {
            percent: percent("6"),
            floor_percent: percent("4.5"),
            attributed: 0,
            shared: 950_000,
            expenses: 950_000,
            current: 57_000,
            carry_in: 0,
            total: 57_000,
            floor: 45_000,
            carry_out: 2_000,
        })
    );
    assert_eq!(closing.carry_out(), BTreeMap::from([(IncomeKind::Other, 2_000)]));
    assert_eq!(serde_json::to_string(&closing).ok().as_deref(), Some(CLOSING));
}

/// Снимок, которого команда не пишет, — порча: квартал не из календаря,
/// налог не равен сумме строк, строки не по порядку или дважды, сумма
/// меньше нуля, незнакомое поле.
#[test]
fn data_rejects_what_close_never_writes() {
    let cases = [
        CLOSING.replace(r#""quarter":3"#, r#""quarter":5"#),
        // До правил налога с оборота (2025 год) квартал не закрыть, а снимок
        // 1-го года растянул бы цепочку переноса на восемь тысяч кварталов.
        CLOSING.replace(r#""year":2026"#, r#""year":2024"#),
        CLOSING.replace(r#""year":2026"#, r#""year":1"#),
        CLOSING.replace(r#""tax":45000}}"#, r#""tax":45001}}"#),
        CLOSING.replace(r#""carry_out":2000"#, r#""carry_out":-2000"#),
        CLOSING.replace(r#""rules":"2026.2""#, r#""rules":"2026""#),
        CLOSING.replace(r#""rate":"10""#, r#""rate":"10,0""#),
        CLOSING.replace(r#""tax":45000}}"#, r#""tax":45000,"note":1}}"#),
        CLOSING.replace(
            r#"],"tax":45000}}"#,
            r#",{"kind":"other","income":0,"rate":"10","gross":0,"deduction":null,"tax":0}],"tax":45000}}"#,
        ),
    ];
    let mut wrong = Vec::new();
    for text in cases {
        assert_ne!(text, CLOSING, "образец поправлен");
        if let Ok(closing) = serde_json::from_str::<Closing>(&text) {
            wrong.push(format!("{text} прочитан как {closing:?}"));
        }
    }
    assert!(wrong.is_empty(), "прочитано запретное: {wrong:#?}");
}

/// «Открыть снова» — метка, а не стирание; второй раз — нечего открывать.
/// Живой снимок квартала — изменённый последним.
#[test]
fn reopening_marks_and_the_latest_wins() {
    let closing: Closing = serde_json::from_str(CLOSING).expect("снимок читается");
    let q = closing.quarter();
    let reopened = closing.reopen(Timestamp::from_unix_millis(1_760_000_000_001)).expect("был закрыт");
    assert!(reopened.is_deleted());
    assert!(reopened.reopen(Timestamp::from_unix_millis(1_760_000_000_002)).is_none());
    assert!(Closing::current(std::slice::from_ref(&reopened), q).is_none());
    let mut newer: Closing = serde_json::from_str(&CLOSING.replace("1760000000000", "1760000000500")).expect("снимок");
    newer.meta = RecordMeta::new(Timestamp::from_unix_millis(1_760_000_000_500));
    let both = [closing.clone(), newer.clone()];
    assert_eq!(Closing::current(&both, q).map(Closing::id), Some(newer.id()));
}
