//! Тесты файла правил: встроенный файл читается и верен, а разбор не
//! принимает записи без источника, с пересекающимися периодами, со
//! значениями, которых программа применить не умеет, и с опечатками.
//!
//! Отказ тест ждёт тем вариантом ошибки, который даёт только проверка
//! [`Rules::parse`] — `Unsourced`, `Backwards`, `Unusable`, `Overlap`, — а
//! serde даёт лишь `Malformed`: пропади проверка, тест покраснеет, а не
//! пройдёт и со сломанным кодом (Правило 2).

use serde_json::{Value, json};

use super::*;

fn day(year: u16, month: u8, day: u8) -> Date {
    Date::new(year, month, day).expect("в тесте — существующая дата")
}

fn version(text: &str) -> Version {
    Version::parse(text).unwrap_or_else(|| panic!("«{text}» — версия правил"))
}

/// Источник, который проходит проверку.
fn source() -> Value {
    json!({
        "law": "ՀՕ-165-Ն ՀՀ հարկային օրենսգիրք (ՀՕ-72-Ն)",
        "article": "հոդված 254, մաս 3, կետ 3.1",
        "url": "https://www.arlis.am/hy/acts/230455",
        "checked": "2026-10-19"
    })
}

/// Запись о кодах `codes`, действующая с `from` до `until`.
fn exclusion(codes: &[&str], from: &str, until: Option<&str>) -> Value {
    json!({
        "codes": codes,
        "classifier": "ՀԴ-011-2013",
        "from": from,
        "until": until,
        "source": source()
    })
}

/// Граница накопительной пенсии, действующая с `from` до `until`.
fn pension(from: &str, until: Option<&str>) -> Value {
    json!({
        "mandatory_born_from": "1974-01-01",
        "from": from,
        "until": until,
        "source": source()
    })
}

/// Файл правил с этими записями; записей остальных видов нет.
fn file(exclusions: &[Value], pensions: &[Value]) -> String {
    json!({
        "version": "2026.1",
        "turnover_exclusions": exclusions,
        "funded_pension": pensions,
        "turnover_rates": [],
        "high_tech": [],
        "turnover_threshold": [],
        "turnover_expenses": [],
        "turnover_carry": [],
        "turnover_form": [],
        "profit_tax": [],
        "social_payment": [],
        "stamp_duty": [],
        "health_insurance": [],
        "turnover_due": [],
        "turnover_application": [],
        "income_declaration": [],
        "settlement_due": [],
        "deadline_shift": [],
        "holidays": [],
        "days_off": []
    })
    .to_string()
}

/// Файл правил с записями одного из видов налога с оборота: `kind` — имя
/// списка, `records` — его записи.
fn file_of(kind: &str, records: &[Value]) -> String {
    let mut file: Value = serde_json::from_str(&file(&[], &[])).expect("JSON");
    file[kind] = json!(records);
    file.to_string()
}

/// Ставка строки `kind` с вычетом и пределом или без них.
fn rate(kind: &str, rate: &str, deduction: Option<&str>, floor: Option<&str>, from: &str, until: Option<&str>) -> Value {
    json!({
        "kind": kind,
        "rate": rate,
        "deduction": deduction,
        "floor": floor,
        "from": from,
        "until": until,
        "source": source()
    })
}

/// Список высоких технологий.
fn high_tech(codes: &[&str], from: &str, until: Option<&str>) -> Value {
    json!({
        "codes": codes,
        "classifier": "ՀԴ-011-2013",
        "from": from,
        "until": until,
        "source": source()
    })
}

/// Порог оборота.
fn threshold(amount: i64, from: &str, until: Option<&str>) -> Value {
    json!({"amount": amount, "from": from, "until": until, "source": source()})
}

/// Какие расходы идут в вычет.
fn expenses(proofs: &[&str], payment_only: &[&str], excluded: &[&str]) -> Value {
    json!({
        "proofs": proofs,
        "payment_only": payment_only,
        "excluded": excluded,
        "from": "2025-01-01",
        "until": null,
        "source": source()
    })
}

const RATES: &str = "turnover_rates";
const HIGH_TECH: &str = "high_tech";
const THRESHOLD: &str = "turnover_threshold";
const EXPENSES: &str = "turnover_expenses";

/// Запись `record`, у которой поле `key` — `value`.
fn with(mut record: Value, key: &str, value: Value) -> Value {
    record[key] = value;
    record
}

/// Запись `record` с полем источника `key` — `value`.
fn with_source(mut record: Value, key: &str, value: &str) -> Value {
    record["source"][key] = json!(value);
    record
}

const EXCLUSIONS: &str = "turnover_exclusions";
const PENSION: &str = "funded_pension";

fn at(kind: &'static str, index: usize) -> At {
    At { kind, index }
}

/// Встроенный файл читается, проходит все проверки разбора, и в нём есть
/// записи каждого вида; [`armenia`] отдаёт его же.
#[test]
fn the_built_in_file_is_sound() {
    let rules = Rules::parse(ARMENIA).unwrap_or_else(|error| panic!("rules/am.json: {error:?}"));
    assert!(!rules.turnover_exclusions().is_empty());
    assert!(!rules.funded_pension().is_empty());
    assert_eq!(rules.turnover_rates().len(), IncomeKind::ALL.len());
    assert!(!rules.high_tech().is_empty());
    assert!(!rules.turnover_threshold().is_empty());
    assert!(!rules.turnover_expenses().is_empty());
    assert!(!rules.turnover_form().is_empty());
    assert!(!rules.profit_tax().is_empty());
    assert!(!rules.social_payment().is_empty());
    assert!(!rules.stamp_duty().is_empty());
    assert!(!rules.health_insurance().is_empty());
    assert!(!rules.turnover_due().is_empty());
    assert!(!rules.turnover_application().is_empty());
    assert!(!rules.income_declaration().is_empty());
    assert!(!rules.settlement_due().is_empty());
    assert!(!rules.deadline_shift().is_empty());
    assert!(!rules.holidays().is_empty());
    assert!(!rules.days_off().is_empty());
    assert_eq!(armenia(), &rules);
}

/// Годный файл разбирается — без этого отказы ниже ничего бы не значили.
#[test]
fn a_sound_file_reads() {
    let text = file(&[exclusion(&["69", "78.2"], "2025-07-01", None)], &[pension("2014-01-01", None)]);
    let rules = Rules::parse(&text).expect("годный файл");
    assert_eq!(rules.version(), version("2026.1"));
    let rule = &rules.turnover_exclusions()[0];
    assert_eq!(rule.codes, ["69", "78.2"]);
    assert_eq!(
        rule.period(),
        Period {
            from: day(2025, 7, 1),
            until: None
        }
    );
    assert_eq!(rule.source.checked, day(2026, 10, 19));
    assert_eq!(rules.funded_pension()[0].mandatory_born_from, day(1974, 1, 1));
}

/// Запись без источника или с источником, который правило не подтверждает,
/// не разбирается: акт без номера `…-Ն`, пустая статья, ссылка не на
/// arlis.am или src.am, дата сверки не из календаря.
#[test]
fn every_record_needs_a_source() {
    let sound = exclusion(&["69"], "2025-07-01", None);
    let mut no_source = sound.clone();
    no_source.as_object_mut().expect("запись — объект").remove("source");
    let mut wrong = Vec::new();
    let cases = [
        (no_source, None),
        (with_source(sound.clone(), "law", "Налоговый кодекс"), Some(at(EXCLUSIONS, 0))),
        (with_source(sound.clone(), "law", ""), Some(at(EXCLUSIONS, 0))),
        (with_source(sound.clone(), "article", " "), Some(at(EXCLUSIONS, 0))),
        (with_source(sound.clone(), "url", "http://www.arlis.am/hy/acts/230455"), Some(at(EXCLUSIONS, 0))),
        (with_source(sound.clone(), "url", "https://www.arlis.am.example.com/hy/acts/1"), Some(at(EXCLUSIONS, 0))),
        (with_source(sound.clone(), "url", "https://meduza.io/feature/2026/10/05/ip"), Some(at(EXCLUSIONS, 0))),
        (with_source(sound.clone(), "url", "www.arlis.am/hy/acts/230455"), Some(at(EXCLUSIONS, 0))),
        (with_source(sound.clone(), "url", ""), Some(at(EXCLUSIONS, 0))),
        (with_source(sound.clone(), "checked", "2026-02-30"), None),
    ];
    for (record, unsourced) in cases {
        let got = Rules::parse(&file(std::slice::from_ref(&record), &[]));
        let expected = match unsourced {
            Some(at) => got == Err(RulesError::Unsourced(at)),
            None => matches!(got, Err(RulesError::Malformed(_))),
        };
        if !expected {
            wrong.push(format!("{}: {got:?}", record["source"]));
        }
    }
    // Источник проверяется у записей каждого вида.
    let got = Rules::parse(&file(&[], &[with_source(pension("2014-01-01", None), "url", "https://t.me/armenia")]));
    if got != Err(RulesError::Unsourced(at(PENSION, 0))) {
        wrong.push(format!("пенсия с источником из Telegram: {got:?}"));
    }
    // Годные адреса — arlis.am и src.am, с «www» и без.
    for url in [
        "https://www.arlis.am/hy/acts/224823/latest",
        "https://arlis.am/hy/acts/230455",
        "https://src.am/am/getNews/1647",
        "https://www.src.am/am/calculator",
    ] {
        let got = Rules::parse(&file(&[with_source(sound.clone(), "url", url)], &[]));
        if got.is_err() {
            wrong.push(format!("{url}: {got:?}"));
        }
    }
    assert!(wrong.is_empty(), "источник: {wrong:#?}");
}

/// Проверяется каждая запись, а не только первая: ошибка называет номер
/// испорченной.
#[test]
fn every_record_is_checked_not_only_the_first() {
    let sound = exclusion(&["69"], "2025-07-01", None);
    let cases = [
        (
            with_source(exclusion(&["70"], "2025-07-01", None), "url", "https://example.com/"),
            RulesError::Unsourced(at(EXCLUSIONS, 1)),
        ),
        (exclusion(&["70"], "2026-01-01", Some("2025-12-31")), RulesError::Backwards(at(EXCLUSIONS, 1))),
        (exclusion(&["7O"], "2025-07-01", None), RulesError::Unusable(at(EXCLUSIONS, 1))),
    ];
    let mut wrong = Vec::new();
    for (second, expected) in cases {
        let got = Rules::parse(&file(&[sound.clone(), second], &[]));
        if got != Err(expected.clone()) {
            wrong.push(format!("{got:?}, а не {expected:?}"));
        }
    }
    let pensions = [
        pension("2014-01-01", Some("2025-12-31")),
        with(pension("2026-01-01", None), "mandatory_born_from", json!("1974-07-01")),
    ];
    let got = Rules::parse(&file(&[], &pensions));
    if got != Err(RulesError::Unusable(at(PENSION, 1))) {
        wrong.push(format!("вторая граница пенсии 1 июля: {got:?}"));
    }
    assert!(wrong.is_empty(), "проверена не каждая запись: {wrong:#?}");
}

/// День конца — не раньше дня начала; правило на один день — годное.
#[test]
fn a_period_does_not_end_before_it_starts() {
    let backwards = file(&[exclusion(&["69"], "2026-01-01", Some("2025-12-31"))], &[]);
    assert_eq!(Rules::parse(&backwards), Err(RulesError::Backwards(at(EXCLUSIONS, 0))));
    let one_day = exclusion(&["69"], "2026-01-01", Some("2026-01-01"));
    assert!(Rules::parse(&file(&[one_day], &[])).is_ok());
}

/// День начала и день конца — включительно.
#[test]
fn period_bounds_are_inclusive() {
    let period = Period {
        from: day(2025, 7, 1),
        until: Some(day(2026, 12, 31)),
    };
    assert!(!period.contains(day(2025, 6, 30)));
    assert!(period.contains(day(2025, 7, 1)));
    assert!(period.contains(day(2026, 12, 31)));
    assert!(!period.contains(day(2027, 1, 1)));
    let open = Period {
        from: day(2025, 7, 1),
        until: None,
    };
    assert!(open.contains(day(9999, 12, 31)));
}

/// Две записи об одном не действуют в один день: смена правила — конец
/// прежнего и начало нового на следующий день. Записи о разных кодах
/// действуют вместе; о вложенных (`78` и `78.2`) — нет.
#[test]
fn records_about_one_subject_do_not_overlap() {
    let overlap = |first, second| {
        Err(RulesError::Overlap {
            kind: EXCLUSIONS,
            first,
            second,
        })
    };
    let cases = [
        (
            "смена правила день в день",
            vec![
                exclusion(&["69"], "2025-07-01", Some("2026-12-31")),
                exclusion(&["69"], "2027-01-01", None),
            ],
            Ok(()),
        ),
        (
            "общий последний день",
            vec![
                exclusion(&["69"], "2025-07-01", Some("2026-12-31")),
                exclusion(&["69"], "2026-12-31", None),
            ],
            overlap(0, 1),
        ),
        (
            "прежнее без конца",
            vec![exclusion(&["69"], "2025-07-01", None), exclusion(&["69"], "2027-01-01", None)],
            overlap(0, 1),
        ),
        (
            "новое внутри прежнего",
            vec![
                exclusion(&["70"], "2025-07-01", Some("2027-12-31")),
                exclusion(&["70"], "2026-03-01", Some("2026-03-31")),
            ],
            overlap(0, 1),
        ),
        (
            "раздел и его группа",
            vec![exclusion(&["78"], "2025-07-01", None), exclusion(&["78.2"], "2025-07-01", None)],
            overlap(0, 1),
        ),
        (
            "класс и его подкласс",
            vec![exclusion(&["62.01"], "2025-07-01", None), exclusion(&["62.01.0"], "2026-01-01", None)],
            overlap(0, 1),
        ),
        (
            "общий код среди прочих",
            vec![
                exclusion(&["69", "70"], "2025-07-01", None),
                exclusion(&["78.2", "70"], "2026-01-01", None),
            ],
            overlap(0, 1),
        ),
        (
            "разные коды",
            vec![exclusion(&["69"], "2025-07-01", None), exclusion(&["70"], "2025-07-01", None)],
            Ok(()),
        ),
        (
            "соседние группы",
            vec![exclusion(&["78.2"], "2025-07-01", None), exclusion(&["78.3"], "2025-07-01", None)],
            Ok(()),
        ),
        (
            "первая и третья",
            vec![
                exclusion(&["69"], "2025-07-01", Some("2025-12-31")),
                exclusion(&["70"], "2025-07-01", None),
                exclusion(&["69"], "2025-12-01", None),
            ],
            overlap(0, 2),
        ),
    ];
    let mut wrong = Vec::new();
    for (case, records, expected) in cases {
        let got = Rules::parse(&file(&records, &[])).map(|_| ());
        if got != expected {
            wrong.push(format!("{case}: {got:?}, а не {expected:?}"));
        }
    }
    // Граница пенсии одна на всех: любые две записи — об одном.
    let pensions = [pension("2014-01-01", Some("2025-12-31")), pension("2026-01-01", None)];
    let got = Rules::parse(&file(&[], &pensions)).map(|_| ());
    if got.is_err() {
        wrong.push(format!("пенсия день в день: {got:?}"));
    }
    let pensions = [pension("2014-01-01", None), pension("2026-01-01", None)];
    let got = Rules::parse(&file(&[], &pensions)).map(|_| ());
    let expected = Err(RulesError::Overlap {
        kind: PENSION,
        first: 0,
        second: 1,
    });
    if got != expected {
        wrong.push(format!("пенсия внахлёст: {got:?}"));
    }
    assert!(wrong.is_empty(), "пересечения: {wrong:#?}");
}

/// Значение, которого программа применить не умеет, — ошибка разбора, а не
/// правило, которое молча не действует: код не записью закона, раздела нет
/// в классификаторе, коды другой редакции классификатора, пустой список
/// кодов, граница пенсии не 1 января.
#[test]
fn values_the_program_cannot_apply_are_rejected() {
    let sound = exclusion(&["69"], "2025-07-01", None);
    let mut wrong = Vec::new();
    let cases = [
        with(sound.clone(), "codes", json!([])),
        with(sound.clone(), "codes", json!(["7O"])),
        with(sound.clone(), "codes", json!(["69", "78."])),
        with(sound.clone(), "codes", json!(["J62"])),
        with(sound.clone(), "codes", json!(["04"])),
        with(sound.clone(), "classifier", json!("ՀԴ-011-2026")),
        with(sound.clone(), "classifier", json!("")),
    ];
    for record in cases {
        let got = Rules::parse(&file(std::slice::from_ref(&record), &[]));
        if got != Err(RulesError::Unusable(at(EXCLUSIONS, 0))) {
            wrong.push(format!("{} {}: {got:?}", record["codes"], record["classifier"]));
        }
    }
    let midyear = with(pension("2014-01-01", None), "mandatory_born_from", json!("1974-07-01"));
    let got = Rules::parse(&file(&[], &[midyear]));
    if got != Err(RulesError::Unusable(at(PENSION, 0))) {
        wrong.push(format!("граница 1 июля: {got:?}"));
    }
    assert!(wrong.is_empty(), "принято то, что программа не применит: {wrong:#?}");
}

/// Опечатка в имени поля или не та форма файла — ошибка, а не молча
/// пропущенное правило.
#[test]
fn typos_are_errors_not_silence() {
    let without = |mut record: Value, key: &str| {
        record.as_object_mut().expect("запись — объект").remove(key);
        record
    };
    let sound = exclusion(&["69"], "2025-07-01", None);
    let no_end = without(sound.clone(), "until");
    let typo = with(no_end.clone(), "untill", json!("2026-12-31"));
    let texts = [
        file(&[typo], &[]),
        // Забытый ключ конца — не «действует бессрочно».
        file(&[no_end], &[]),
        file(&[], &[without(pension("2014-01-01", None), "until")]),
        file(&[with(sound.clone(), "note", json!("лишнее поле"))], &[]),
        file(&[with_source(sound.clone(), "checked", "19.10.2026")], &[]),
        file(&[with(sound.clone(), "from", json!("2025-7-1"))], &[]),
        file(&[with(sound.clone(), "codes", json!("69"))], &[]),
        json!({"version": "2026.1", "turnover_exclusion": [], "funded_pension": []}).to_string(),
        json!({"version": "2026.1", "turnover_exclusions": []}).to_string(),
        json!({"version": "2026.01", "turnover_exclusions": [], "funded_pension": []}).to_string(),
        json!({"turnover_exclusions": [], "funded_pension": []}).to_string(),
        "{".to_owned(),
        String::new(),
        "<!doctype html>".to_owned(),
    ];
    let mut wrong = Vec::new();
    for text in texts {
        let got = Rules::parse(&text);
        if !matches!(got, Err(RulesError::Malformed(_))) {
            wrong.push(format!("{text}: {got:?}"));
        }
    }
    assert!(wrong.is_empty(), "принято с опечаткой: {wrong:#?}");
}

/// Версии правил сравниваются по числам, а не строками: 2026.10 новее
/// 2026.9, 2027.1 новее любой 2026-го. Запись читается обратно той же.
#[test]
fn versions_compare_by_numbers() {
    let order = ["2026.1", "2026.2", "2026.9", "2026.10", "2027.1", "9999.65535"];
    for pair in order.windows(2) {
        assert!(version(pair[0]) < version(pair[1]), "{} не раньше {}", pair[0], pair[1]);
    }
    let mut wrong = Vec::new();
    for text in order {
        if version(text).to_string() != text {
            wrong.push(text);
        }
    }
    assert!(wrong.is_empty(), "запись не сошлась: {wrong:?}");
    assert_eq!((version("2026.3").year(), version("2026.3").revision()), (2026, 3));
}

/// Разбор версии строгий: год из четырёх цифр, точка и номер правки от
/// единицы, без ведущих нулей, знаков, пробелов и хвоста.
#[test]
fn version_parse_is_strict() {
    let mut wrong = Vec::new();
    for text in [
        "", "2026", "2026.", ".1", "2026.0", "2026.01", "0026.1", "26.1", "20260.1", "2026.1.0", "2026..1",
        "2026,1", " 2026.1", "2026.1 ", "+026.1", "2026.+1", "2026.-1", "2026.65536", "v2026.1", "2026.٣",
    ] {
        if let Some(found) = Version::parse(text) {
            wrong.push(format!("«{text}» — {found}"));
        }
    }
    assert!(wrong.is_empty(), "приняты: {wrong:#?}");
}

/// Отпечаток значений текущей версии правил: её запись и FNV-1a значений
/// файла ([`values`]).
const PINNED: (&str, u64) = ("2026.7", 0x7ed2_1698_ad5b_779a);

/// Значения файла правил одной строкой: всё, кроме версии и источников, —
/// JSON с ключами по алфавиту. Источник — когда сверили и где лежит
/// текст — правила не меняет; ставка, срок, порог, коды, даты действия,
/// новая запись и порядок записей — меняют.
fn values(text: &str) -> String {
    fn write(value: &Value, out: &mut String) {
        match value {
            Value::Object(fields) => {
                let mut keys: Vec<&String> = fields.keys().filter(|key| *key != "source").collect();
                keys.sort();
                out.push('{');
                for (index, key) in keys.into_iter().enumerate() {
                    if index > 0 {
                        out.push(',');
                    }
                    out.push_str(&json!(key).to_string());
                    out.push(':');
                    write(&fields[key], out);
                }
                out.push('}');
            }
            Value::Array(items) => {
                out.push('[');
                for (index, item) in items.iter().enumerate() {
                    if index > 0 {
                        out.push(',');
                    }
                    write(item, out);
                }
                out.push(']');
            }
            scalar => out.push_str(&scalar.to_string()),
        }
    }

    let mut file: Value = serde_json::from_str(text).expect("файл правил — JSON");
    file.as_object_mut().expect("файл правил — объект").remove("version");
    let mut out = String::new();
    write(&file, &mut out);
    out
}

/// FNV-1a, 64 бита: отпечаток, одинаковый на любой машине.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// Значения поменялись — версия правил обязана вырасти (шапка модуля):
/// закрытый квартал хранит версию, по которой его посчитали. Тест помнит
/// отпечаток значений своей версии: правка значения при той же версии его
/// ломает, а новая сверка источника — нет. Подняли версию — запишите
/// в `PINNED` её и отпечаток из сообщения теста.
#[test]
fn values_belong_to_their_version() {
    let current = armenia().version();
    let print = fnv1a(values(ARMENIA).as_bytes());
    let (pinned, pinned_print) = PINNED;
    assert!(
        current >= version(pinned),
        "версия правил {current} старше записанной {pinned}: версия только растёт"
    );
    assert!(
        current != version(pinned) || print == pinned_print,
        "значения правил изменились, а версия та же — {current}: поднимите version в \
         rules/am.json (Правило 5) и запишите в PINNED новую версию с отпечатком {print:#018x}"
    );
    assert!(
        current == version(pinned),
        "версия правил {current} новая: запишите в PINNED её и отпечаток {print:#018x}"
    );
}

/// Отпечаток видит правку значения и не видит сверку источника, пробелы
/// и порядок ключей.
#[test]
fn values_see_rules_not_sources() {
    let base = file(&[exclusion(&["69"], "2025-07-01", None)], &[pension("2014-01-01", None)]);
    let print = |text: &str| fnv1a(values(text).as_bytes());
    let rechecked = file(
        &[with_source(exclusion(&["69"], "2025-07-01", None), "checked", "2027-03-01")],
        &[with_source(pension("2014-01-01", None), "article", "հոդված 5")],
    );
    assert_eq!(print(&base), print(&rechecked), "сверка источника — не новое правило");
    let mut bumped: Value = serde_json::from_str(&base).expect("JSON");
    bumped["version"] = json!("2026.2");
    assert_eq!(print(&base), print(&bumped.to_string()), "версия — не значение");
    let spaced = serde_json::to_string_pretty(&bumped).expect("JSON пишется");
    assert_eq!(print(&base), print(&spaced), "пробелы — не новое правило");
    let reordered = format!(
        r#"{{"days_off": [], "deadline_shift": [], "funded_pension": [{}], "health_insurance": [], "high_tech": [],
            "holidays": [], "income_declaration": [], "profit_tax": [], "settlement_due": [], "social_payment": [], "stamp_duty": [], "turnover_application": [],
            "turnover_carry": [], "turnover_due": [], "turnover_exclusions": [{}], "turnover_expenses": [], "turnover_form": [], "turnover_rates": [],
            "turnover_threshold": [], "version": "2026.1"}}"#,
        pension("2014-01-01", None),
        exclusion(&["69"], "2025-07-01", None),
    );
    assert_eq!(print(&base), print(&reordered), "порядок ключей — не новое правило");
    let changed = [
        file(&[exclusion(&["69", "70"], "2025-07-01", None)], &[pension("2014-01-01", None)]),
        file(&[exclusion(&["69"], "2025-07-02", None)], &[pension("2014-01-01", None)]),
        file(&[exclusion(&["69"], "2025-07-01", Some("2026-12-31"))], &[pension("2014-01-01", None)]),
        file(
            &[exclusion(&["69"], "2025-07-01", None)],
            &[with(pension("2014-01-01", None), "mandatory_born_from", json!("1975-01-01"))],
        ),
        file(&[exclusion(&["69"], "2025-07-01", None), exclusion(&["70"], "2025-07-01", None)], &[pension("2014-01-01", None)]),
    ];
    for text in changed {
        assert_ne!(print(&base), print(&text), "не видна правка: {text}");
    }
}

/// Ставка: вычет и предел — вместе или никак, предел ниже ставки, ставка
/// больше нуля; две записи об одной строке не действуют в один день, а о
/// разных — действуют.
#[test]
fn rates_are_checked() {
    let mut wrong = Vec::new();
    let sound = [
        rate("other", "10", Some("6"), Some("4.5"), "2025-01-01", None),
        rate("high_tech", "1", None, None, "2025-01-01", Some("2031-12-31")),
    ];
    if let Err(error) = Rules::parse(&file_of(RATES, &sound)) {
        wrong.push(format!("годные ставки: {error:?}"));
    }
    for (case, record) in [
        ("вычет без предела", rate("other", "10", Some("6"), None, "2025-01-01", None)),
        ("предел без вычета", rate("other", "10", None, Some("4.5"), "2025-01-01", None)),
        ("предел не ниже ставки", rate("trade", "10", Some("9.5"), Some("10"), "2025-01-01", None)),
        ("нулевая ставка", rate("assets", "0", None, None, "2025-01-01", None)),
        ("нулевой вычет", rate("other", "10", Some("0"), Some("4.5"), "2025-01-01", None)),
    ] {
        let got = Rules::parse(&file_of(RATES, &[record]));
        if got != Err(RulesError::Unusable(at(RATES, 0))) {
            wrong.push(format!("{case}: {got:?}"));
        }
    }
    let twice = [
        rate("other", "10", Some("6"), Some("4.5"), "2025-01-01", None),
        rate("other", "12", Some("6"), Some("4.5"), "2026-01-01", None),
    ];
    let got = Rules::parse(&file_of(RATES, &twice));
    let expected = Err(RulesError::Overlap {
        kind: RATES,
        first: 0,
        second: 1,
    });
    if got != expected {
        wrong.push(format!("одна строка внахлёст: {got:?}"));
    }
    for (case, text) in [
        ("процент с запятой", file_of(RATES, &[rate("other", "4,5", None, None, "2025-01-01", None)])),
        ("незнакомая строка", file_of(RATES, &[rate("luxury", "10", None, None, "2025-01-01", None)])),
    ] {
        if !matches!(Rules::parse(&text), Err(RulesError::Malformed(_))) {
            wrong.push(format!("{case}: принято"));
        }
    }
    assert!(wrong.is_empty(), "ставки: {wrong:#?}");
}

/// Список высоких технологий, порог и расходы: значения, которые программа
/// применит, и один список на всех.
#[test]
fn turnover_lists_are_checked() {
    let mut wrong = Vec::new();
    let unusable = [
        (HIGH_TECH, high_tech(&[], "2025-01-01", None)),
        (HIGH_TECH, high_tech(&["J62.01.0"], "2025-01-01", None)),
        (HIGH_TECH, with(high_tech(&["62.01.0"], "2025-01-01", None), "classifier", json!("ՀԴ-011-2026"))),
        (THRESHOLD, threshold(0, "2025-01-01", None)),
        (EXPENSES, expenses(&[], &["levy"], &["asset"])),
        (EXPENSES, expenses(&["tax_invoice", "bank_payment"], &["levy"], &["asset"])),
        (EXPENSES, expenses(&["tax_invoice", "none"], &["levy"], &["asset"])),
        (EXPENSES, expenses(&["tax_invoice", "tax_invoice"], &["levy"], &["asset"])),
        (EXPENSES, expenses(&["tax_invoice"], &["levy"], &["levy"])),
        (EXPENSES, expenses(&["tax_invoice"], &["levy", "levy"], &[])),
    ];
    for (kind, record) in unusable {
        let got = Rules::parse(&file_of(kind, std::slice::from_ref(&record)));
        if got != Err(RulesError::Unusable(at(kind, 0))) {
            wrong.push(format!("{kind} {record}: {got:?}"));
        }
    }
    for (kind, first, second) in [
        (HIGH_TECH, high_tech(&["62.01.0"], "2025-01-01", None), high_tech(&["72.20.0"], "2026-01-01", None)),
        (THRESHOLD, threshold(115_000_000, "2025-01-01", None), threshold(120_000_000, "2026-01-01", None)),
    ] {
        let got = Rules::parse(&file_of(kind, &[first, second]));
        let expected = Err(RulesError::Overlap {
            kind,
            first: 0,
            second: 1,
        });
        if got != expected {
            wrong.push(format!("{kind} внахлёст: {got:?}"));
        }
    }
    let sound = expenses(&["tax_invoice", "settlement_invoice"], &["levy"], &["asset", "travel"]);
    if let Err(error) = Rules::parse(&file_of(EXPENSES, &[sound])) {
        wrong.push(format!("годные расходы: {error:?}"));
    }
    assert!(wrong.is_empty(), "списки налога с оборота: {wrong:#?}");
}

const PROFIT: &str = "profit_tax";
const SOCIAL: &str = "social_payment";

/// Фиксированный платёж за месяц: сумма и срок «в следующем году».
fn monthly(amount: i64, due: (u8, u8), from: &str, until: Option<&str>) -> Value {
    json!({
        "monthly": amount,
        "due": {"month": due.0, "day": due.1},
        "from": from,
        "until": until,
        "source": source()
    })
}

/// Фиксированный платёж за месяц: сумма больше нуля и срок, который есть
/// в каждом году; одна сумма на всех — две записи не действуют в один
/// день; срок — день следующего года.
#[test]
fn monthly_payments_are_checked() {
    let mut wrong = Vec::new();
    for kind in [PROFIT, SOCIAL] {
        let sound = [monthly(5000, (4, 20), "2025-01-01", Some("2026-12-31")), monthly(6000, (4, 20), "2027-01-01", None)];
        match Rules::parse(&file_of(kind, &sound)) {
            Ok(rules) => {
                let records = if kind == PROFIT { rules.profit_tax() } else { rules.social_payment() };
                if records[0].due.after(2026) != Some(day(2027, 4, 20)) {
                    wrong.push(format!("{kind}: срок за 2026 год — {:?}", records[0].due.after(2026)));
                }
            }
            Err(error) => wrong.push(format!("{kind} годные: {error:?}")),
        }
        for (case, record) in [
            ("нулевая сумма", monthly(0, (4, 20), "2025-01-01", None)),
            ("отрицательная сумма", monthly(-5000, (4, 20), "2025-01-01", None)),
            ("29 февраля", monthly(5000, (2, 29), "2025-01-01", None)),
            ("31 апреля", monthly(5000, (4, 31), "2025-01-01", None)),
            ("тринадцатый месяц", monthly(5000, (13, 1), "2025-01-01", None)),
        ] {
            let got = Rules::parse(&file_of(kind, &[record]));
            if got != Err(RulesError::Unusable(at(kind, 0))) {
                wrong.push(format!("{kind} {case}: {got:?}"));
            }
        }
        let twice = [monthly(5000, (4, 20), "2025-01-01", None), monthly(6000, (4, 20), "2027-01-01", None)];
        let expected = Err(RulesError::Overlap {
            kind,
            first: 0,
            second: 1,
        });
        if Rules::parse(&file_of(kind, &twice)) != expected {
            wrong.push(format!("{kind} внахлёст принят"));
        }
        let typo = with(monthly(5000, (4, 20), "2025-01-01", None), "due", json!({"month": 4, "dya": 20}));
        if !matches!(Rules::parse(&file_of(kind, &[typo])), Err(RulesError::Malformed(_))) {
            wrong.push(format!("{kind} с опечаткой в сроке принят"));
        }
    }
    assert!(wrong.is_empty(), "фиксированные платежи: {wrong:#?}");
}

/// Запись на весь год — та же 1 января и 31 декабря; правило, сменившееся
/// посреди года, на этот год не годится, а года без правила нет.
#[test]
fn a_rule_for_the_whole_year() {
    let records = [
        monthly(5000, (4, 20), "2025-01-01", Some("2026-06-30")),
        monthly(6000, (4, 20), "2026-07-01", Some("2027-12-31")),
    ];
    let rules = Rules::parse(&file_of(PROFIT, &records)).expect("годные записи");
    let list = rules.profit_tax();
    let mut wrong = Vec::new();
    if for_year(list, 2025).map(|rule| rule.monthly) != Some(5000) {
        wrong.push("2025 — первая запись".to_owned());
    }
    if for_year(list, 2026).is_some() {
        wrong.push("2026 — смена посреди года, а запись нашлась".to_owned());
    }
    if for_year(list, 2027).map(|rule| rule.monthly) != Some(6000) {
        wrong.push("2027 — вторая запись".to_owned());
    }
    if for_year(list, 2024).is_some() || for_year(list, 2028).is_some() {
        wrong.push("год без правила нашёл запись".to_owned());
    }
    assert!(wrong.is_empty(), "запись на год: {wrong:#?}");
}

const STAMP: &str = "stamp_duty";
const HEALTH: &str = "health_insurance";

/// Шкала гербового сбора: ступени `(граница, сбор)`.
fn stamp(steps: &[(Option<i64>, i64)], from: &str, until: Option<&str>) -> Value {
    let steps: Vec<Value> = steps
        .iter()
        .map(|(up_to, amount)| json!({"up_to": up_to, "amount": amount}))
        .collect();
    json!({
        "steps": steps,
        "due": {"month": 4, "day": 20},
        "from": from,
        "until": until,
        "source": source()
    })
}

/// Взнос медстраховки: как платится — `by`.
fn health(by: Value, from: &str, until: Option<&str>) -> Value {
    json!({
        "monthly": 10800,
        "state_pays_from_age": 65,
        "starts": from,
        "by": by,
        "from": from,
        "until": until,
        "source": source()
    })
}

/// Шкала гербового сбора: границы растут, без границы — только последняя,
/// сборы больше нуля; ступень по базе — первая, чья граница не меньше её,
/// включительно.
#[test]
fn stamp_duty_scales_are_checked() {
    let mut wrong = Vec::new();
    let sound = stamp(&[(Some(12_000_000), 12_000), (None, 120_000)], "2026-01-01", None);
    match Rules::parse(&file_of(STAMP, std::slice::from_ref(&sound))) {
        Ok(rules) => {
            let scale = &rules.stamp_duty()[0];
            for (base, step) in [(0, 0), (12_000_000, 0), (12_000_001, 1), (i64::MAX, 1)] {
                if scale.step(base) != Some(step) {
                    wrong.push(format!("база {base}: ступень {:?}, а не {step}", scale.step(base)));
                }
            }
        }
        Err(error) => wrong.push(format!("годная шкала: {error:?}")),
    }
    for (case, record) in [
        ("без ступеней", stamp(&[], "2026-01-01", None)),
        ("последняя с границей", stamp(&[(Some(12_000_000), 12_000)], "2026-01-01", None)),
        ("без границы не последняя", stamp(&[(None, 12_000), (None, 120_000)], "2026-01-01", None)),
        (
            "границы не растут",
            stamp(&[(Some(6_000_000), 12_000), (Some(6_000_000), 24_000), (None, 120_000)], "2026-01-01", None),
        ),
        ("нулевой сбор", stamp(&[(Some(12_000_000), 0), (None, 120_000)], "2026-01-01", None)),
        ("отрицательная граница", stamp(&[(Some(-1), 12_000), (None, 120_000)], "2026-01-01", None)),
        (
            "срок 30 февраля",
            with(sound.clone(), "due", json!({"month": 2, "day": 30})),
        ),
    ] {
        let got = Rules::parse(&file_of(STAMP, &[record]));
        if got != Err(RulesError::Unusable(at(STAMP, 0))) {
            wrong.push(format!("{case}: {got:?}"));
        }
    }
    let twice = [sound.clone(), stamp(&[(None, 12_000)], "2025-01-01", None)];
    let expected = Err(RulesError::Overlap {
        kind: STAMP,
        first: 0,
        second: 1,
    });
    if Rules::parse(&file_of(STAMP, &twice)) != expected {
        wrong.push("две шкалы внахлёст приняты".to_owned());
    }
    let no_bound_key = json!({"steps": [{"amount": 12_000}], "due": {"month": 4, "day": 20},
        "from": "2026-01-01", "until": null, "source": source()});
    if !matches!(Rules::parse(&file_of(STAMP, &[no_bound_key])), Err(RulesError::Malformed(_))) {
        wrong.push("ступень без ключа границы принята".to_owned());
    }
    assert!(wrong.is_empty(), "гербовый сбор: {wrong:#?}");
}

/// Взнос медстраховки: за год — с порогом и днём срока, помесячно — с числом
/// месяца, которое есть в каждом месяце; взнос и возраст больше нуля.
#[test]
fn health_insurance_is_checked() {
    let mut wrong = Vec::new();
    let year = json!({"year": {"income_over": 2_400_000, "due": "2026-07-20"}});
    let month = json!({"month": {"due_day": 20}});
    let sound = [health(year.clone(), "2026-01-01", Some("2026-12-31")), health(month.clone(), "2027-01-01", None)];
    match Rules::parse(&file_of(HEALTH, &sound)) {
        Ok(rules) => {
            let records = rules.health_insurance();
            let expected = [
                HealthBy::Year {
                    income_over: 2_400_000,
                    due: day(2026, 7, 20),
                },
                HealthBy::Month { due_day: 20 },
            ];
            if records.iter().map(|record| record.by).collect::<Vec<_>>() != expected {
                wrong.push(format!("прочитано не то: {records:?}"));
            }
        }
        Err(error) => wrong.push(format!("годные записи: {error:?}")),
    }
    for (case, record) in [
        ("нулевой взнос", with(health(month.clone(), "2027-01-01", None), "monthly", json!(0))),
        ("нулевой возраст", with(health(month.clone(), "2027-01-01", None), "state_pays_from_age", json!(0))),
        ("29-е число", health(json!({"month": {"due_day": 29}}), "2027-01-01", None)),
        ("нулевое число", health(json!({"month": {"due_day": 0}}), "2027-01-01", None)),
        (
            "отрицательный порог",
            health(json!({"year": {"income_over": -1, "due": "2026-07-20"}}), "2026-01-01", None),
        ),
    ] {
        let got = Rules::parse(&file_of(HEALTH, &[record]));
        if got != Err(RulesError::Unusable(at(HEALTH, 0))) {
            wrong.push(format!("{case}: {got:?}"));
        }
    }
    for (case, by) in [
        ("незнакомый способ", json!({"quarter": {"due_day": 20}})),
        ("опечатка в поле", json!({"month": {"due_dya": 20}})),
        ("два способа", json!({"year": {"income_over": 1, "due": "2026-07-20"}, "month": {"due_day": 20}})),
    ] {
        let got = Rules::parse(&file_of(HEALTH, &[health(by, "2027-01-01", None)]));
        if !matches!(got, Err(RulesError::Malformed(_))) {
            wrong.push(format!("{case}: {got:?}"));
        }
    }
    assert!(wrong.is_empty(), "медстраховка: {wrong:#?}");
}

/// Запись сроков вида `kind`: поля `fields`, действует с `from` до `until`.
fn timed(fields: Value, from: &str, until: Option<&str>) -> Value {
    let mut record = fields;
    record["from"] = json!(from);
    record["until"] = json!(until);
    record["source"] = source();
    record
}

/// Записи сроков (задача 21): число месяца после квартала — в каждом
/// месяце, день заявления — в каждом году; выходные — дни недели от 1 до 7
/// без повторов, и рабочий день в неделе есть; праздники — дни каждого
/// года без повторов; решение правительства — дни в его периоде, каждый
/// в одном списке. Два решения о разных днях действуют вместе, об одном
/// дне — нет.
#[test]
fn deadline_records_are_checked() {
    let mut wrong = Vec::new();
    let decision = |off: &[&str], work: &[&str]| {
        timed(json!({"off": off, "work": work}), "2026-01-01", Some("2026-12-31"))
    };
    let sound = [
        ("turnover_due", vec![timed(json!({"day": 20}), "2025-01-01", None)]),
        (
            "turnover_application",
            vec![timed(json!({"due": {"month": 2, "day": 20}, "after_days": 20}), "2025-01-01", None)],
        ),
        ("deadline_shift", vec![timed(json!({"weekend": [6, 7]}), "2025-01-01", None)]),
        (
            "holidays",
            vec![timed(json!({"days": [{"month": 1, "day": 1}, {"month": 12, "day": 31}]}), "2025-01-01", None)],
        ),
        ("days_off", vec![decision(&["2026-05-04"], &["2026-04-18"]), decision(&["2026-12-30"], &[])]),
        (
            "settlement_due",
            vec![
                timed(json!({"after_days": 0}), "2025-01-01", Some("2026-08-31")),
                timed(json!({"after_days": 0}), "2026-09-01", None),
            ],
        ),
    ];
    for (kind, records) in &sound {
        if let Err(error) = Rules::parse(&file_of(kind, records)) {
            wrong.push(format!("{kind}: годные записи: {error:?}"));
        }
    }
    for (kind, case, record) in [
        ("turnover_due", "29-е число", timed(json!({"day": 29}), "2025-01-01", None)),
        ("turnover_due", "нулевое число", timed(json!({"day": 0}), "2025-01-01", None)),
        (
            "turnover_application",
            "29 февраля",
            timed(json!({"due": {"month": 2, "day": 29}, "after_days": 20}), "2025-01-01", None),
        ),
        (
            "turnover_application",
            "ноль дней",
            timed(json!({"due": {"month": 2, "day": 20}, "after_days": 0}), "2025-01-01", None),
        ),
        ("deadline_shift", "нет выходных", timed(json!({"weekend": []}), "2025-01-01", None)),
        ("deadline_shift", "нулевой день", timed(json!({"weekend": [0, 7]}), "2025-01-01", None)),
        ("deadline_shift", "восьмой день", timed(json!({"weekend": [6, 8]}), "2025-01-01", None)),
        ("deadline_shift", "повтор", timed(json!({"weekend": [6, 6]}), "2025-01-01", None)),
        (
            "deadline_shift",
            "вся неделя",
            timed(json!({"weekend": [1, 2, 3, 4, 5, 6, 7]}), "2025-01-01", None),
        ),
        ("holidays", "нет дней", timed(json!({"days": []}), "2025-01-01", None)),
        (
            "holidays",
            "29 февраля",
            timed(json!({"days": [{"month": 2, "day": 29}]}), "2025-01-01", None),
        ),
        (
            "holidays",
            "повтор",
            timed(json!({"days": [{"month": 1, "day": 1}, {"month": 1, "day": 1}]}), "2025-01-01", None),
        ),
        ("days_off", "нет дней", decision(&[], &[])),
        ("days_off", "день вне периода", decision(&["2027-01-04"], &[])),
        ("days_off", "день в обоих списках", decision(&["2026-05-04"], &["2026-05-04"])),
        ("days_off", "повтор", decision(&["2026-05-04", "2026-05-04"], &[])),
    ] {
        let got = Rules::parse(&file_of(kind, &[record]));
        if got != Err(RulesError::Unusable(at(kind, 0))) {
            wrong.push(format!("{kind}: {case}: {got:?}"));
        }
    }
    let same_day = [decision(&["2026-05-04"], &[]), decision(&[], &["2026-05-04"])];
    let got = Rules::parse(&file_of("days_off", &same_day));
    if got
        != Err(RulesError::Overlap {
            kind: "days_off",
            first: 0,
            second: 1,
        })
    {
        wrong.push(format!("два решения об одном дне: {got:?}"));
    }
    let two_lists = [
        timed(json!({"day": 20}), "2025-01-01", None),
        timed(json!({"day": 25}), "2026-01-01", None),
    ];
    let got = Rules::parse(&file_of("turnover_due", &two_lists));
    if got
        != Err(RulesError::Overlap {
            kind: "turnover_due",
            first: 0,
            second: 1,
        })
    {
        wrong.push(format!("два срока квартала в один день: {got:?}"));
    }
    assert!(wrong.is_empty(), "записи сроков: {wrong:#?}");
}

const FORM: &str = "turnover_form";

/// Запись формы расчёта (задача 24) с тремя строками налога и номерами,
/// как в приказе 299-Ն; действует с `from` до `until`.
fn form(from: &str, until: Option<&str>) -> Value {
    timed(
        json!({
            "lines": [
                {"kind": "trade", "section": "5", "row": "11"},
                {"kind": "high_tech", "section": null, "row": "19"},
                {"kind": "other", "section": "9", "row": "21"}
            ],
            "fields": {
                "income": "1", "rate": "2", "gross": "3", "direct": "4.1", "overhead": "4.2", "expenses": "4",
                "percent": "5", "current": "6", "carry_in": "7", "total": "8", "floor": "9", "tax": "10",
                "carry_out": "11"
            },
            "total": "22",
            "turnover": "27"
        }),
        from,
        until,
    )
}

/// Форма расчёта (задача 24): номера — числа без нулей впереди через точку,
/// у пунктов формы — разделов, строк сводки, итога и оборота — без точки
/// и все разные, поля раздела — тоже разные, строка налога — по разу, дни —
/// целые кварталы: форму подают за отчётный период. Две формы в один день
/// не действуют, а смена формы с квартала — годна. Забытое поле раздела —
/// ошибка разбора, а не поле без номера.
#[test]
fn the_form_is_checked() {
    let mut wrong = Vec::new();
    let sound = [
        vec![form("2025-01-01", None)],
        vec![form("2025-01-01", Some("2026-06-30")), form("2026-07-01", None)],
    ];
    for records in &sound {
        if let Err(error) = Rules::parse(&file_of(FORM, records)) {
            wrong.push(format!("годная форма: {error:?}"));
        }
    }
    let line = |kind: &str, section: Option<&str>, row: &str| json!({"kind": kind, "section": section, "row": row});
    let lines = |list: &[Value]| with(form("2025-01-01", None), "lines", json!(list));
    let point = |key: &str, number: &str| with(form("2025-01-01", None), key, json!(number));
    let field = |key: &str, number: &str| {
        let mut record = form("2025-01-01", None);
        record["fields"][key] = json!(number);
        record
    };
    for (case, record) in [
        ("нет строк", lines(&[])),
        ("строка дважды", lines(&[line("other", Some("9"), "21"), line("other", None, "23")])),
        ("раздел с точкой", lines(&[line("trade", Some("5.1"), "11")])),
        ("строка с нулём впереди", lines(&[line("trade", Some("5"), "011")])),
        ("пустой номер строки", lines(&[line("trade", Some("5"), "")])),
        ("раздел с точкой в конце", lines(&[line("trade", Some("5."), "11")])),
        ("раздел — буква", lines(&[line("trade", Some("V"), "11")])),
        ("раздел совпал со строкой", lines(&[line("trade", Some("11"), "11")])),
        ("две строки с одним номером", lines(&[line("trade", Some("5"), "11"), line("rent", None, "11")])),
        ("итог совпал со строкой", point("total", "21")),
        ("оборот совпал с итогом", point("turnover", "22")),
        ("итог с точкой", point("total", "22.1")),
        ("поле ноль", field("income", "0")),
        ("поле с точкой в конце", field("direct", "4.")),
        ("поле с двумя точками подряд", field("direct", "4..1")),
        ("два поля с одним номером", field("overhead", "4.1")),
        ("поле с буквой", field("carry_out", "11a")),
        ("с середины квартала", form("2025-02-01", None)),
        ("конец не в конце квартала", form("2025-01-01", Some("2026-06-29"))),
    ] {
        let got = Rules::parse(&file_of(FORM, &[record]));
        if got != Err(RulesError::Unusable(at(FORM, 0))) {
            wrong.push(format!("{case}: {got:?}"));
        }
    }
    let got = Rules::parse(&file_of(FORM, &[form("2025-01-01", None), form("2026-01-01", None)]));
    if got
        != Err(RulesError::Overlap {
            kind: FORM,
            first: 0,
            second: 1,
        })
    {
        wrong.push(format!("две формы в один день: {got:?}"));
    }
    let without_field = {
        let mut record = form("2025-01-01", None);
        record["fields"].as_object_mut().expect("поля — объект").remove("carry_out");
        record
    };
    let without_section = lines(&[json!({"kind": "trade", "row": "11"})]);
    for (case, record) in [
        ("нет поля раздела", without_field),
        ("нет ключа раздела", without_section),
        ("незнакомое поле", field("income_total", "12")),
        ("незнакомая строка налога", lines(&[line("services", Some("9"), "21")])),
    ] {
        let got = Rules::parse(&file_of(FORM, &[record]));
        if !matches!(got, Err(RulesError::Malformed(_))) {
            wrong.push(format!("{case}: {got:?}"));
        }
    }
    assert!(wrong.is_empty(), "форма расчёта: {wrong:#?}");
}

const CARRY: &str = "turnover_carry";

/// Запись о переносе с кварталов до программы (задача 69).
fn carry(kinds: &[&str], from: &str, until: Option<&str>) -> Value {
    timed(json!({ "kinds": kinds }), from, until)
}

/// Перенос (задача 69): запись ищут по первому дню квартала, поэтому её дни —
/// целые кварталы, строки есть и не повторяются; две записи на один день не
/// действуют. Незнакомая строка налога — ошибка разбора.
#[test]
fn the_carry_rule_is_checked() {
    let mut wrong = Vec::new();
    let sound = [carry(&["trade"], "2025-01-01", Some("2025-03-31")), carry(&["trade", "other"], "2025-04-01", None)];
    if let Err(error) = Rules::parse(&file_of(CARRY, &sound)) {
        wrong.push(format!("годные записи: {error:?}"));
    }
    for (case, record) in [
        ("нет строк", carry(&[], "2025-01-01", None)),
        ("строка дважды", carry(&["trade", "trade"], "2025-01-01", None)),
        ("с середины квартала", carry(&["trade"], "2025-02-01", None)),
        ("конец не в конце квартала", carry(&["trade"], "2025-01-01", Some("2025-03-30"))),
    ] {
        let got = Rules::parse(&file_of(CARRY, &[record]));
        if got != Err(RulesError::Unusable(at(CARRY, 0))) {
            wrong.push(format!("{case}: {got:?}"));
        }
    }
    let both = [carry(&["trade"], "2025-01-01", None), carry(&["other"], "2025-04-01", None)];
    let got = Rules::parse(&file_of(CARRY, &both));
    if got != Err(RulesError::Overlap { kind: CARRY, first: 0, second: 1 }) {
        wrong.push(format!("две записи на один день: {got:?}"));
    }
    let got = Rules::parse(&file_of(CARRY, &[carry(&["barter"], "2025-01-01", None)]));
    if !matches!(got, Err(RulesError::Malformed(_))) {
        wrong.push(format!("незнакомая строка: {got:?}"));
    }
    assert!(wrong.is_empty(), "перенос: {wrong:#?}");
}
