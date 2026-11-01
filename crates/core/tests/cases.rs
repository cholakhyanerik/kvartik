//! Налог квартала на случаях (Правило 2, «Налог проверяется случаями»):
//! каждый файл `cases/*.json` — ИП с записями книги и налог его кварталов,
//! выписанный не кодом. Кто посчитал ожидаемые числа — в самом файле (поле
//! `checked`): примеры ՊԵԿ из руководства и ролика, примеры бухгалтеров с их
//! сайтов, случаи агента вручную (`it-and-design-no-base`,
//! `local-services-rounding`, `trade-seasonal` держат прочтения агента).
//! `accountant` заполняет только живая сверка: чужой пример — не сверка
//! нашего расчёта.
//!
//! Проверка идёт по всем случаям и кварталам и выдаёт провалы в конце,
//! а не падает на первом: иначе красным видно только первое значение.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::Deserialize;

use kvartik_core::model::activity::{ActivityCode, IncomeKind};
use kvartik_core::model::date::{Date, Quarter};
use kvartik_core::model::ledger::{Flow, Nature, Operation, OperationData, Proof};
use kvartik_core::model::money::{Currency, ExchangeRate, Money, RateSource};
use kvartik_core::model::profile::ProfileData;
use kvartik_core::model::record::Timestamp;
use kvartik_core::model::text::Line;
use kvartik_core::tax::turnover::{self, Books};

/// Случай — как он лежит в файле.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    /// Кто этот ИП — словами.
    about: String,
    /// Кто и как посчитал ожидаемые числа.
    #[allow(dead_code, reason = "для человека, читающего файл")]
    checked: Checked,
    profile: Profile,
    entries: Vec<Entry>,
    expected: Vec<Expected>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code, reason = "для человека, читающего файл")]
struct Checked {
    by: String,
    on: String,
    accountant: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Profile {
    activities: Vec<String>,
    #[serde(default)]
    kinds: BTreeMap<String, IncomeKind>,
}

/// Запись книги: доход или расход.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    flow: Flow,
    date: Date,
    /// Сумма в младших единицах валюты: драмы, центы.
    minor: i64,
    currency: Currency,
    /// Курс ЦБ в десятитысячных драма; `None` — у драмов.
    rate_e4: Option<i64>,
    activity: Option<String>,
    proof: Option<Proof>,
    nature: Option<Nature>,
}

/// Налог квартала, выписанный человеком.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Expected {
    /// `2026-3` — год и номер квартала.
    quarter: String,
    tax: i64,
    lines: Vec<ExpectedLine>,
}

#[derive(Deserialize, PartialEq, Debug)]
#[serde(deny_unknown_fields)]
struct ExpectedLine {
    kind: IncomeKind,
    income: i64,
    tax: i64,
    /// Перенос на следующий квартал — у строк с вычетом.
    #[serde(default)]
    carry_out: i64,
}

fn code(text: &str) -> ActivityCode {
    ActivityCode::parse(text).unwrap_or_else(|error| panic!("код «{text}»: {error:?}"))
}

fn operation(entry: &Entry) -> Operation {
    let rate = entry.rate_e4.map(|e4| {
        ExchangeRate::new(entry.currency, e4, entry.date, RateSource::Manual).expect("курс случая годен")
    });
    let data = OperationData {
        flow: entry.flow,
        date: entry.date,
        paid: None,
        amount: Money::new(entry.minor, entry.currency),
        rate,
        text: Line::new("Запись случая").expect("текст"),
        client: None,
        supplier: Line::default(),
        proof: entry.proof,
        nature: entry.nature,
        activity: entry.activity.as_deref().map(code),
        attachments: Vec::new(),
    };
    Operation::create(data, Timestamp::from_unix_millis(1_760_000_000_000)).expect("запись случая годна")
}

fn quarter(text: &str) -> Quarter {
    let (year, number) = text.split_once('-').expect("квартал — ГГГГ-N");
    Quarter::new(year.parse().expect("год"), number.parse().expect("номер")).expect("квартал из календаря")
}

#[test]
fn every_case_holds() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/cases");
    let mut files: Vec<_> = fs::read_dir(&root)
        .expect("папка случаев читается")
        .map(|entry| entry.expect("запись папки").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "случаев нет");
    let mut wrong = Vec::new();
    for path in files {
        let name = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
        let text = fs::read_to_string(&path).expect("случай читается");
        let case: Case = serde_json::from_str(&text).unwrap_or_else(|error| panic!("{name}: {error}"));
        let profile = ProfileData {
            activities: case.profile.activities.iter().map(|text| code(text)).collect(),
            kinds: case.profile.kinds.iter().map(|(text, kind)| (code(text), *kind)).collect(),
            ..ProfileData::default()
        };
        let operations: Vec<Operation> = case.entries.iter().map(operation).collect();
        let books = Books {
            unreadable: 0,
            documents: &[],
            operations: &operations,
            profile: Some(&profile),
            closings: &[],
        };
        for expected in &case.expected {
            let got = turnover::quarter(books, quarter(&expected.quarter));
            let lines: Vec<ExpectedLine> = got
                .lines
                .iter()
                .map(|line| ExpectedLine {
                    kind: line.kind,
                    income: line.income,
                    tax: line.tax,
                    carry_out: line.deduction.map_or(0, |deduction| deduction.carry_out),
                })
                .collect();
            if got.tax != expected.tax || lines != expected.lines {
                wrong.push(format!(
                    "{name} ({}), квартал {}: налог {}, а не {}; строки {lines:?}, а не {:?}",
                    case.about, expected.quarter, got.tax, expected.tax, expected.lines
                ));
            }
        }
    }
    assert!(wrong.is_empty(), "случаи не сошлись: {wrong:#?}");
}
