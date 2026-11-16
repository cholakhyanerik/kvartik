use super::*;

fn day(year: u16, month: u8, day: u8) -> Date {
    Date::new(year, month, day).expect("день календаря")
}

/// Файл курсов из пар доллара; евро — те же дни с курсом на 10 ֏ больше.
fn file(usd: &[(&str, i64)]) -> String {
    let pairs = |plus: i64| {
        usd.iter()
            .map(|(day, e4)| format!("[\"{day}\",{}]", e4 + plus))
            .collect::<Vec<_>>()
            .join(",")
    };
    format!("{{\"currencies\":{{\"USD\":[{}],\"EUR\":[{}]}}}}", pairs(0), pairs(100_000))
}

fn rates(usd: &[(&str, i64)]) -> Rates {
    Rates::parse(&file(usd)).expect("файл курсов годится")
}

/// Файл, который пишет выкладка (`tools/deploy/rates.mjs`) из ответа ЦБ
/// с 2026-11-05 по 2026-11-16 (`tests/rates/cba.xml`; тот же ли он, что
/// пишет программа выкладки, сверяет `tools/browser/check.mjs`), читается,
/// и курсы в нём — те, что в ответе ЦБ: доллар 13 ноября — 360,54 ֏, евро
/// — 398,4 ֏, а рубля, которого программа не знает, нет.
#[test]
fn deploy_file_is_read() {
    let text = include_str!("../../../tests/rates/rates.json");
    let rates = Rates::parse(text).expect("файл выкладки читается");
    assert_eq!(rates.published(Currency::Usd, day(2026, 11, 13)), Some(3_605_400));
    assert_eq!(rates.published(Currency::Eur, day(2026, 11, 13)), Some(3_984_000));
    assert_eq!(rates.published(Currency::Usd, day(2026, 11, 16)), Some(3_604_400));
    // Суббота: ЦБ курса не публиковал.
    assert_eq!(rates.published(Currency::Usd, day(2026, 11, 14)), None);
    assert_eq!(rates.published(Currency::Amd, day(2026, 11, 13)), None);
    // Поставка в понедельник 16 ноября — курс пятницы 13-го.
    assert_eq!(
        hint(Some(&rates), Currency::Usd, day(2026, 11, 16)),
        Some(Hint::Rate {
            published: day(2026, 11, 13),
            per_unit_e4: 3_605_400
        })
    );
}

/// Незнакомые поля и валюты прежняя версия пропускает: файл пишет новая
/// выкладка (шапка модуля).
#[test]
fn unknown_fields_and_currencies_are_skipped() {
    let text = r#"{"source":"x","made":"2026-11-16","currencies":{"GBP":[["2026-11-13",4753500]],
        "USD":[["2026-11-13",3605400]],"XAU":"не пары"},"next":{"any":1}}"#;
    let rates = Rates::parse(text).expect("годится");
    assert_eq!(rates.published(Currency::Usd, day(2026, 11, 13)), Some(3_605_400));
    assert_eq!(rates.published(Currency::Eur, day(2026, 11, 13)), None);
}

/// Что-то не так — файла нет целиком: ни половины курсов, ни угаданного
/// дня. Каждый случай — рядом с годным файлом, который отличается от него
/// одним местом: без проверки разбор принял бы и его.
#[test]
fn broken_files_are_refused_whole() {
    let good = file(&[("2026-11-12", 3_605_100), ("2026-11-13", 3_605_400)]);
    assert!(Rates::parse(&good).is_ok(), "годный файл");
    let broken = [
        ("не JSON", "{".to_owned()),
        ("без currencies", r#"{"source":"x"}"#.to_owned()),
        ("валюта — не список", r#"{"currencies":{"USD":"360.54"}}"#.to_owned()),
        ("день не из календаря", file(&[("2026-11-12", 3_605_100), ("2026-02-30", 3_605_400)])),
        ("день не той записью", file(&[("2026-11-12", 3_605_100), ("13.11.2026", 3_605_400)])),
        ("дни не по возрастанию", file(&[("2026-11-13", 3_605_400), ("2026-11-12", 3_605_100)])),
        ("день дважды", file(&[("2026-11-13", 3_605_100), ("2026-11-13", 3_605_400)])),
        ("курс ноль", file(&[("2026-11-12", 3_605_100), ("2026-11-13", 0)])),
        ("курс меньше нуля", file(&[("2026-11-12", 3_605_100), ("2026-11-13", -1)])),
        ("курс больше предела", file(&[("2026-11-12", 3_605_100), ("2026-11-13", MAX_RATE_E4 + 1)])),
        ("курс дробью", r#"{"currencies":{"USD":[["2026-11-13",360.54]]}}"#.to_owned()),
        ("пара из трёх", r#"{"currencies":{"USD":[["2026-11-13",3605400,1]]}}"#.to_owned()),
    ];
    let mut wrong = Vec::new();
    for (what, text) in &broken {
        if Rates::parse(text) != Err(RatesError::Malformed) {
            wrong.push(*what);
        }
    }
    let big = format!("{}{}", " ".repeat(MAX_BYTES), good);
    if Rates::parse(&big) != Err(RatesError::TooLarge) {
        wrong.push("больше предела");
    }
    assert!(wrong.is_empty(), "приняты: {wrong:?}");
    // Курс на пределе — ещё курс.
    let edge = format!(r#"{{"currencies":{{"USD":[["2026-11-13",{MAX_RATE_E4}]]}}}}"#);
    assert!(Rates::parse(&edge).is_ok());
}

/// День курса — рабочий день, предыдущий дню поставки, по календарю файла
/// правил (НК, ст. 16, ч. 2, п. 1): выходные, праздник, конец года. Случаи
/// посчитаны вручную по календарю 2026 года и закону о праздниках.
#[test]
fn published_day_is_the_previous_working_day() {
    let all = rates(&[
        ("2026-09-18", 3_800_100),
        ("2026-09-22", 3_800_200),
        ("2026-12-30", 3_800_300),
        ("2027-01-04", 3_800_400),
        ("2027-01-05", 3_800_500),
    ]);
    let cases = [
        // Вторник после праздника 21 сентября (понедельник) и выходных —
        // пятница 18-го.
        (day(2026, 9, 22), day(2026, 9, 18), 3_800_100),
        // Праздник сам — тоже поставка: курс пятницы 18-го.
        (day(2026, 9, 21), day(2026, 9, 18), 3_800_100),
        // Среда 23-го — вторник 22-го.
        (day(2026, 9, 23), day(2026, 9, 22), 3_800_200),
        // 1 января 2027 года: 31 декабря — праздник, курс 30-го.
        (day(2027, 1, 1), day(2026, 12, 30), 3_800_300),
        // 5 января (вторник) — после 1, 2 января и выходных — понедельник 4-го.
        (day(2027, 1, 5), day(2027, 1, 4), 3_800_400),
    ];
    let mut wrong = Vec::new();
    for (supply, published, per_unit_e4) in cases {
        let got = hint(Some(&all), Currency::Usd, supply);
        let want = Some(Hint::Rate {
            published,
            per_unit_e4,
        });
        if got != want {
            wrong.push(format!("{}: {got:?}, а ждали {want:?}", supply.iso()));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
    // Евро — свой курс того же дня.
    assert_eq!(
        hint(Some(&all), Currency::Eur, day(2026, 9, 22)),
        Some(Hint::Rate {
            published: day(2026, 9, 18),
            per_unit_e4: 3_900_100
        })
    );
}

/// Правило — с 2026-07-01: поставка 1 июля берёт курс 30 июня, а поставке
/// 30 июня подсказки нет — тогда закон брал курс, опубликованный до 16:00
/// дня расчётного документа (`tax::rules::RateDay`). У драма курса нет.
#[test]
fn hint_starts_with_the_rule() {
    let all = rates(&[("2026-06-29", 3_870_000), ("2026-06-30", 3_871_500)]);
    assert_eq!(
        hint(Some(&all), Currency::Usd, day(2026, 7, 1)),
        Some(Hint::Rate {
            published: day(2026, 6, 30),
            per_unit_e4: 3_871_500
        })
    );
    assert_eq!(hint(Some(&all), Currency::Usd, day(2026, 6, 30)), None);
    assert_eq!(hint(Some(&all), Currency::Amd, day(2026, 11, 16)), None);
}

/// Курса нужного дня нет: файл кончается раньше — «пока нет» (не
/// опубликован или не дошёл), позже есть — пропуск, и курс сверяют на
/// cba.am. Без файла — «пока нет».
#[test]
fn missing_rate_says_why() {
    let some = rates(&[("2026-11-10", 3_604_300), ("2026-11-12", 3_605_100)]);
    // Поставка 17 ноября — курс 16-го, а файл кончается 12-м.
    assert_eq!(
        hint(Some(&some), Currency::Usd, day(2026, 11, 17)),
        Some(Hint::NotYet {
            published: day(2026, 11, 16)
        })
    );
    // Поставка 12 ноября — курс 11-го, которого в файле нет, а 12-е есть.
    assert_eq!(
        hint(Some(&some), Currency::Usd, day(2026, 11, 12)),
        Some(Hint::Missing {
            published: day(2026, 11, 11)
        })
    );
    // Поставка 13 ноября — курс 12-го, последний в файле.
    assert_eq!(
        hint(Some(&some), Currency::Usd, day(2026, 11, 13)),
        Some(Hint::Rate {
            published: day(2026, 11, 12),
            per_unit_e4: 3_605_100
        })
    );
    assert_eq!(
        hint(None, Currency::Usd, day(2026, 11, 13)),
        Some(Hint::NotYet {
            published: day(2026, 11, 12)
        })
    );
    // ЦБ публиковал курс в субботу 21 ноября, которую календарь считает
    // выходной (решение правительства, которого в файле правил нет):
    // поставке в понедельник 23-го курс пятницы 20-го не выдаётся, а без
    // субботней публикации — выдаётся.
    let saturday = rates(&[("2026-11-20", 3_605_000), ("2026-11-21", 3_606_000)]);
    assert_eq!(
        hint(Some(&saturday), Currency::Usd, day(2026, 11, 23)),
        Some(Hint::Missing {
            published: day(2026, 11, 20)
        })
    );
    let friday = rates(&[("2026-11-20", 3_605_000), ("2026-11-23", 3_607_000)]);
    assert_eq!(
        hint(Some(&friday), Currency::Usd, day(2026, 11, 23)),
        Some(Hint::Rate {
            published: day(2026, 11, 20),
            per_unit_e4: 3_605_000
        })
    );
    // Пустой список валюты — тоже «пока нет».
    let empty = Rates::parse(r#"{"currencies":{"USD":[]}}"#).expect("годится");
    assert_eq!(
        hint(Some(&empty), Currency::Usd, day(2026, 11, 13)),
        Some(Hint::NotYet {
            published: day(2026, 11, 12)
        })
    );
}
