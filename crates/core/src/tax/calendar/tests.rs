//! Тесты календаря сроков. Ожидаемые дни и суммы выписаны руками: дни — по
//! тексту законов (разведка задачи 21; шапка `deadlines/tests.rs`), дни
//! недели сверены стандартной библиотекой Python, суммы — по ставкам файла
//! правил (НК, ст. 258; ՀՕ-245-Ն; ՀՕ-459-Ն) на бумаге, а не кодом
//! программы (Правило 2).

use super::*;
use crate::model::date::Quarter;
use crate::model::document::Document;
use crate::model::ledger::{Flow, Operation, OperationData};
use crate::model::money::{Currency, Money};
use crate::model::profile::{Person, ProfileData, Residency};
use crate::model::record::Timestamp;
use crate::model::text::Line;
use crate::tax::closing::Closing;

fn day(year: u16, month: u8, day: u8) -> Date {
    Date::new(year, month, day).expect("в тесте — существующая дата")
}

fn quarter(year: u16, number: u8) -> Quarter {
    Quarter::new(year, number).expect("в тесте — существующий квартал")
}

const NBSP: char = '\u{A0}';

/// Строки файла без складывания (RFC 5545, 3.1): продолжение — строка,
/// которая начинается с пробела. Строки разделены `CRLF`, и файл им
/// кончается.
fn unfold(text: &str) -> Vec<String> {
    let body = text.strip_suffix("\r\n").expect("файл кончается CRLF");
    let mut lines: Vec<String> = Vec::new();
    for raw in body.split("\r\n") {
        match (raw.strip_prefix(' '), lines.last_mut()) {
            (Some(rest), Some(last)) => last.push_str(rest),
            _ => lines.push(raw.to_owned()),
        }
    }
    lines
}

/// Значение TEXT без экранирования; `None` — в нём голая `,` или `;`,
/// управляющий знак или неизвестное экранирование: такой файл читатель
/// понял бы не так.
fn unescape(value: &str) -> Option<String> {
    let mut out = String::new();
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next()? {
                '\\' => out.push('\\'),
                ';' => out.push(';'),
                ',' => out.push(','),
                'n' | 'N' => out.push('\n'),
                _ => return None,
            },
            ',' | ';' => return None,
            c if c.is_control() && c != '\t' => return None,
            c => out.push(c),
        }
    }
    Some(out)
}

/// Событие файла: свойства по имени (с параметрами, до `:`) и значения,
/// по порядку, без напоминаний; напоминания — отдельно.
#[derive(Debug, Default)]
struct Parsed {
    props: Vec<(String, String)>,
    alarms: Vec<Vec<(String, String)>>,
}

impl Parsed {
    fn all(&self, name: &str) -> Vec<&str> {
        self.props
            .iter()
            .filter(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
            .collect()
    }

    fn one(&self, name: &str) -> &str {
        match self.all(name).as_slice() {
            [value] => value,
            values => panic!("{name}: {values:?} вместо одного"),
        }
    }

    fn text(&self, name: &str) -> String {
        let value = self.one(name);
        unescape(value).unwrap_or_else(|| panic!("{name}: не экранировано: {value}"))
    }
}

/// Шапка календаря и события.
fn parse(text: &str) -> (Vec<(String, String)>, Vec<Parsed>) {
    let mut head = Vec::new();
    let mut events = Vec::new();
    let mut event: Option<Parsed> = None;
    let mut alarm: Option<Vec<(String, String)>> = None;
    for line in unfold(text) {
        let (name, value) = line.split_once(':').unwrap_or_else(|| panic!("строка без «:»: {line}"));
        let pair = (name.to_owned(), value.to_owned());
        match (line.as_str(), &mut event, &mut alarm) {
            ("BEGIN:VEVENT", None, _) => event = Some(Parsed::default()),
            ("END:VEVENT", Some(_), None) => events.extend(event.take()),
            ("BEGIN:VALARM", Some(_), None) => alarm = Some(Vec::new()),
            ("END:VALARM", Some(parsed), Some(_)) => parsed.alarms.extend(alarm.take()),
            (_, Some(_), Some(alarm)) => alarm.push(pair),
            (_, Some(parsed), None) => parsed.props.push(pair),
            (_, None, _) => head.push(pair),
        }
    }
    assert!(event.is_none() && alarm.is_none(), "незакрытое событие или напоминание");
    (head, events)
}

/// Экранирование TEXT: `\`, `;`, `,`, перевод строки — со знаком `\`;
/// `CR` и прочие управляющие знаки — прочь, табуляция остаётся.
#[test]
fn text_is_escaped() {
    let cases = [
        ("Налог, сбор; пошлина", "Налог\\, сбор\\; пошлина"),
        ("C:\\путь", "C:\\\\путь"),
        ("строка\r\nдругая", "строка\\nдругая"),
        ("звонок\u{7}\u{0}", "звонок"),
        ("а\tб", "а\tб"),
        ("Ձեր ժամկետները՝ 20.10.2026։", "Ձեր ժամկետները՝ 20.10.2026։"),
    ];
    let wrong: Vec<String> = cases
        .iter()
        .filter_map(|&(text, expected)| {
            let got = escape(text);
            (got != expected).then(|| format!("{text:?}: {got:?} вместо {expected:?}"))
        })
        .collect();
    assert!(wrong.is_empty(), "экранирование: {wrong:#?}");
}

/// Строка до 75 октетов — как есть; длиннее — продолжения с пробелом,
/// каждое не длиннее 75 октетов вместе с ним, а армянская буква (два
/// октета) не режется: разрезанную Rust и не записал бы, поэтому
/// проверяется, что граница октетов честная и сложенное разворачивается
/// в то же.
#[test]
fn long_lines_fold_between_characters() {
    let exact = format!("SUMMARY:{}", "a".repeat(LINE_OCTETS - "SUMMARY:".len()));
    let mut out = String::new();
    fold(&mut out, &exact);
    assert_eq!(out, format!("{exact}\r\n"), "ровно 75 октетов не складывается");

    let over = format!("{exact}b");
    let mut out = String::new();
    fold(&mut out, &over);
    assert_eq!(out, format!("{exact}\r\n b\r\n"), "76-й октет — на продолжение");

    // Нечётный сдвиг: двухоктетные буквы после «S:» встали бы на границу
    // 75-го октета.
    let armenian = format!("S:{}", "Ժամկետ ".repeat(40));
    let mut out = String::new();
    fold(&mut out, &armenian);
    let mut wrong = Vec::new();
    for line in out.strip_suffix("\r\n").expect("CRLF в конце").split("\r\n") {
        if line.len() > LINE_OCTETS {
            wrong.push(format!("{} октетов: {line}", line.len()));
        }
    }
    let lines = out.split("\r\n").count();
    assert!(lines > 2, "длинная строка не сложена: {out}");
    assert!(wrong.is_empty(), "длинные строки: {wrong:#?}");
    assert_eq!(unfold(&out), [armenian]);
}

/// Сроки всех ИП на налоге с оборота за 2026 год — по одним правилам, без
/// профиля: квартальные 20-го числа, заявление 20 февраля, годовые платежи
/// 20 апреля, медстраховка за 2026 год 20 июля, декларация за 2025 год
/// 1 ноября (воскресенье — 2 ноября); в один день — по порядку `Kind`.
#[test]
fn everyones_deadlines_of_a_year() {
    let got: Vec<(Date, Kind)> = deadlines::general(day(2026, 1, 1), day(2026, 12, 31))
        .iter()
        .map(|deadline| (deadline.day, deadline.kind))
        .collect();
    let expected = vec![
        (day(2026, 1, 20), Kind::Turnover(quarter(2025, 4))),
        (day(2026, 2, 20), Kind::Application(2026)),
        (day(2026, 4, 20), Kind::Turnover(quarter(2026, 1))),
        (day(2026, 4, 20), Kind::Annual(2025)),
        (day(2026, 7, 20), Kind::Turnover(quarter(2026, 2))),
        (day(2026, 7, 20), Kind::HealthYear(2026)),
        (day(2026, 10, 20), Kind::Turnover(quarter(2026, 3))),
        (day(2026, 11, 2), Kind::Declaration(2025)),
    ];
    assert_eq!(got, expected);
}

/// Общий календарь на каждом языке: строки по RFC 5545 — не длиннее
/// 75 октетов, `CRLF`, текст экранирован; у каждого события постоянный
/// `UID` (одни и те же на всех языках, без повторов), день сверки правил
/// в `DTSTAMP`, событие на весь день и два напоминания — за 7 дней и за
/// день в 9 утра; сроки — с 1 января года сверки по 31 декабря
/// следующего; тот же код — те же байты (шапка модуля, «Повторяемость»).
#[test]
fn the_public_calendar_is_sound() {
    let checked = rules::armenia().checked().expect("в правилах есть записи");
    let (first, last) = (day(checked.year(), 1, 1), day(checked.year() + 1, 12, 31));
    let stamp = format!("{}T000000Z", ymd(checked));
    let mut wrong = Vec::new();
    let mut uids_by_lang = Vec::new();
    for lang in Lang::ALL {
        let text = public(lang);
        if text != public(lang) {
            wrong.push(format!("{lang:?}: две сборки — разные байты"));
        }
        if text.replace("\r\n", "").contains(['\r', '\n']) {
            wrong.push(format!("{lang:?}: перевод строки без пары CR LF"));
        }
        for line in text.split("\r\n").filter(|line| line.len() > LINE_OCTETS) {
            wrong.push(format!("{lang:?}: {} октетов: {line}", line.len()));
        }
        let (head, events) = parse(&text);
        let head_value = |name: &str| head.iter().find(|(key, _)| key == name).map(|(_, value)| value.as_str());
        if head.first().map(|(key, value)| (key.as_str(), value.as_str())) != Some(("BEGIN", "VCALENDAR"))
            || head.last().map(|(key, value)| (key.as_str(), value.as_str())) != Some(("END", "VCALENDAR"))
        {
            wrong.push(format!("{lang:?}: не VCALENDAR от начала до конца"));
        }
        if head_value("VERSION") != Some("2.0") || head_value("PRODID").is_none() {
            wrong.push(format!("{lang:?}: нет VERSION 2.0 или PRODID"));
        }
        let source = format!("https://kvartik.com/calendar/{}.ics", lang.code());
        if head_value("SOURCE;VALUE=URI") != Some(source.as_str()) {
            wrong.push(format!("{lang:?}: SOURCE {:?}", head_value("SOURCE;VALUE=URI")));
        }
        if events.len() < 20 {
            wrong.push(format!("{lang:?}: событий {} — меньше двух лет сроков", events.len()));
        }
        let mut uids = Vec::new();
        for event in &events {
            let uid = event.one("UID").to_owned();
            let start = event.one("DTSTART;VALUE=DATE");
            let date = Date::new(
                start[..4].parse().unwrap_or(0),
                start[4..6].parse().unwrap_or(0),
                start[6..].parse().unwrap_or(0),
            );
            match date {
                Some(date) if first <= date && date <= last => {
                    let end = date.add_days(1).map(ymd).unwrap_or_default();
                    if event.one("DTEND;VALUE=DATE") != end {
                        wrong.push(format!("{lang:?} {uid}: DTEND не следующий день"));
                    }
                }
                _ => wrong.push(format!("{lang:?} {uid}: день {start} вне {first:?}–{last:?}")),
            }
            if event.one("DTSTAMP") != stamp {
                wrong.push(format!("{lang:?} {uid}: DTSTAMP {}", event.one("DTSTAMP")));
            }
            let summary = event.text("SUMMARY");
            let description = event.text("DESCRIPTION");
            if summary.is_empty() || !description.contains(APP_URL) {
                wrong.push(format!("{lang:?} {uid}: без названия или ссылки на программу"));
            }
            let triggers: Vec<&str> = event
                .alarms
                .iter()
                .filter_map(|alarm| alarm.iter().find(|(key, _)| key == "TRIGGER").map(|(_, value)| value.as_str()))
                .collect();
            let displays = event
                .alarms
                .iter()
                .filter(|alarm| {
                    alarm.contains(&("ACTION".to_owned(), "DISPLAY".to_owned()))
                        && alarm.iter().any(|(key, value)| key == "DESCRIPTION" && unescape(value) == Some(summary.clone()))
                })
                .count();
            if triggers != ["-P6DT15H", "-PT15H"] || displays != 2 {
                wrong.push(format!("{lang:?} {uid}: напоминания {triggers:?}, показываемых с названием {displays}"));
            }
            uids.push(uid);
        }
        let count = uids.len();
        uids.sort();
        uids.dedup();
        if uids.len() != count {
            wrong.push(format!("{lang:?}: повторы UID"));
        }
        uids_by_lang.push(uids);
    }
    if uids_by_lang.windows(2).any(|pair| pair[0] != pair[1]) {
        wrong.push("UID на разных языках разные".to_owned());
    }
    assert!(wrong.is_empty(), "общий календарь: {wrong:#?}");
}

/// Взнос медстраховки за январь 2027 года (правки человека 2026-11-26:
/// сайт, программа и календарь говорят одно): событие срока — на 20.02.2027
/// по прочтению Kvartik, без переноса с субботы на 22.02.2027, со словами
/// «Надёжнее заплатить в январе, до 20.01.2027», и своё событие на
/// 20.01.2027 — день с безопасной стороны, с теми же двумя напоминаниями:
/// напоминания подписки календарь может снять молча (шапка модуля), а
/// событие на сам день видно и без них. В личном календаре — так же, со
/// своими `UID`, а прошедшего 20 января события в нём нет.
#[test]
fn the_first_monthly_premium_has_its_safe_day() {
    let mut wrong = Vec::new();
    // Строки под взносом (с 0.0.210): заплатившему в январе второй раз не
    // платить, заплатившему годовую ставку за 2026 год — уточнить.
    let paid = "Если вы заплатили взнос за январь в январе, второй раз платить не нужно";
    let annual = "Если вы заплатили годовую ставку за 2026 год, уточните у бухгалтера, нужен ли вам взнос за январь (ст.\u{a0}46, ч.\u{a0}12)";
    let (_, events) = parse(&public(Lang::Ru));
    let find = |uid: &str| events.iter().find(|event| event.one("UID") == uid);
    match find("health-2027-01@kvartik.com") {
        Some(event)
            if event.one("DTSTART;VALUE=DATE") == "20270220"
                && event
                    .text("DESCRIPTION")
                    .contains(&format!("Надёжнее заплатить в январе, до 20.01.2027\n{paid}\n{annual}")) => {}
        other => wrong.push(format!(
            "срок взноса за январь: {:?}",
            other.map(|event| (event.one("DTSTART;VALUE=DATE").to_owned(), event.text("DESCRIPTION")))
        )),
    }
    let about = "По прочтению Kvartik срок взноса\u{a0}— 20.02.2027, но закон не говорит, за какой месяц каждый платёж, \
                 и надёжнее заплатить в январе, до 20.01.2027.";
    match find("health-2027-01-safe@kvartik.com") {
        Some(event)
            if event.one("DTSTART;VALUE=DATE") == "20270120"
                && event.text("SUMMARY") == "Взнос медстраховки за январь 2027\u{a0}г.\u{a0}— надёжнее заплатить до этого дня"
                && event.text("DESCRIPTION").starts_with(&format!("{about}\n{annual}"))
                && event.text("DESCRIPTION").contains(APP_URL) => {}
        other => wrong.push(format!(
            "январь, день с безопасной стороны: {:?}",
            other.map(|event| (event.one("DTSTART;VALUE=DATE").to_owned(), event.text("SUMMARY"), event.text("DESCRIPTION")))
        )),
    }
    let profile = programmer();
    let documents: &[Document] = &[];
    let books = Books {
        documents,
        operations: &[],
        profile: Some(&profile),
        closings: &[],
        openings: &[],
        unreadable: 0,
    };
    let starts = |today: Date| -> Vec<(String, String)> {
        personal_events(books, today)
            .into_iter()
            .filter(|(uid, _, _)| uid.starts_with("mine-health-2027-01"))
            .map(|(uid, start, _)| (uid, start))
            .collect()
    };
    let before = starts(day(2026, 11, 26));
    let expected = [
        ("mine-health-2027-01-safe@kvartik.com".to_owned(), "20270120".to_owned()),
        ("mine-health-2027-01@kvartik.com".to_owned(), "20270220".to_owned()),
    ];
    if before != expected {
        wrong.push(format!("личный 26.11.2026: {before:?}"));
    }
    let after = starts(day(2027, 1, 25));
    if after != expected[1..] {
        wrong.push(format!("личный 25.01.2027: {after:?}"));
    }
    assert!(wrong.is_empty(), "взнос за январь в календаре: {wrong:#?}");
}

/// Гражданин 1990 года рождения, ИП с 2022 года, программист по списку
/// 142-Ն (1 %).
fn programmer() -> ProfileData {
    ProfileData {
        person: Person {
            birth_year: Some(1990),
            pension_joined: None,
            pension_applied: None,
            resident: None,
            residency: Some(Residency::Citizen),
            registered: Some(day(2022, 5, 12)),
            turnover_applied: None,
            stops: Vec::new(),
        },
        activities: vec![crate::model::activity::ActivityCode::parse("62.01.0").expect("код")],
        ..ProfileData::default()
    }
}

/// Доход в драмах в день `date`.
fn income(date: Date, drams: i64) -> Operation {
    income_of(date, Money::new(drams, Currency::Amd))
}

/// Доход в долларах без курса ЦБ в день `date`: в налог он не идёт.
fn unrated_income(date: Date, cents: i64) -> Operation {
    income_of(date, Money::new(cents, Currency::Usd))
}

/// Доход суммой `amount` в день `date`, без курса.
fn income_of(date: Date, amount: Money) -> Operation {
    Operation::create(
        OperationData {
            flow: Flow::Income,
            date,
            paid: None,
            amount,
            rate: None,
            text: Line::new("Услуга").expect("текст"),
            client: None,
            supplier: Line::default(),
            proof: None,
            nature: None,
            cost: None,
            activity: None,
            income_kind: None,
            attachments: Vec::new(),
        },
        Timestamp::from_unix_millis(1_760_000_000_000),
    )
    .expect("в тесте — годная запись")
}

/// События личного календаря по `UID`: день и описание.
fn personal_events(books: Books<'_>, today: Date) -> Vec<(String, String, String)> {
    let text = personal(books, today, Lang::Ru, "https://kvartik.com/app/").expect("сроки есть");
    let (_, events) = parse(&text);
    events
        .iter()
        .map(|event| {
            (
                event.one("UID").to_owned(),
                event.one("DTSTART;VALUE=DATE").to_owned(),
                event.text("DESCRIPTION"),
            )
        })
        .collect()
}

/// Личный календарь 5 октября 2026 года у программиста с доходом
/// 4 321 987 ֏ в августе: III квартал кончился — к 20 октября налог
/// 43 219 ֏ (1 %, лумы отброшены); IV квартал не кончился — суммы нет;
/// годовые платежи за 2026 год — 60 000 налога на прибыль, 60 000
/// соцплатежа и 12 000 гербового сбора (оборот не больше 12 млн) — 132 000;
/// медстраховка за январь 2027 года — 10 800; у декларации — перенос
/// с воскресенья. Закрытый квартал — сделан, и в календаре его нет.
#[test]
fn the_personal_calendar_has_my_amounts() {
    let profile = programmer();
    let operations = [income(day(2026, 8, 15), 4_321_987)];
    let documents: &[Document] = &[];
    let books = Books {
        documents,
        operations: &operations,
        profile: Some(&profile),
        closings: &[],
        openings: &[],
        unreadable: 0,
    };
    let today = day(2026, 10, 5);
    let events = personal_events(books, today);
    let find = |uid: &str| events.iter().find(|(id, _, _)| id == uid);
    let sum = |digits: &str| format!("К уплате: {}{NBSP}֏{NBSP}— по данным Kvartik на 05.10.2026.", digits.replace(' ', &NBSP.to_string()));
    let mut wrong = Vec::new();
    for (uid, start, expected) in [
        ("mine-turnover-2026-q3@kvartik.com", "20261020", Some(sum("43 219"))),
        ("mine-turnover-2026-q4@kvartik.com", "20270120", None),
        ("mine-annual-2026@kvartik.com", "20270420", Some(sum("132 000"))),
        // Взнос за январь — по прочтению Kvartik, без переноса с субботы.
        ("mine-health-2027-01@kvartik.com", "20270220", Some(sum("10 800"))),
        ("mine-application-2027@kvartik.com", "20270222", None),
    ] {
        match find(uid) {
            Some((_, got_start, description)) => {
                let first = description.lines().next().unwrap_or_default();
                let amount = first.starts_with("К уплате").then(|| first.to_owned());
                if got_start != start || amount != expected {
                    wrong.push(format!("{uid}: {got_start}, {amount:?} вместо {start}, {expected:?}"));
                }
                if !description.ends_with("Подробности\u{A0}— в Kvartik: https://kvartik.com/app/") {
                    wrong.push(format!("{uid}: без ссылки на программу: {description}"));
                }
                // День зачисления на единый счёт — у налога, а не у взноса
                // медстраховки и не у заявления.
                let tax = uid.contains("turnover") || uid.contains("annual");
                if description.contains("Налог считается заплаченным") != tax {
                    wrong.push(format!("{uid}: слова о дне зачисления {}", if tax { "пропали" } else { "лишние" }));
                }
            }
            None => wrong.push(format!("{uid}: нет события")),
        }
    }
    match find("mine-declaration-2025@kvartik.com") {
        Some((_, start, description)) if start == "20261102" && description.contains("Перенесено с 01.11.2026") => {}
        other => wrong.push(format!("декларация за 2025 год: {other:?}")),
    }
    if events.iter().any(|(uid, _, _)| !uid.starts_with("mine-")) {
        wrong.push("UID личного календаря совпадают с общим".to_owned());
    }

    let tax = turnover::quarter(books, quarter(2026, 3));
    let closed = [Closing::close(&tax, today, Timestamp::from_unix_millis(1_760_000_000_000)).expect("квартал закрывается")];
    let closed_books = Books {
        closings: &closed,
        openings: &[],
        ..books
    };
    if personal_events(closed_books, today)
        .iter()
        .any(|(uid, _, _)| uid == "mine-turnover-2026-q3@kvartik.com")
    {
        wrong.push("закрытый квартал в календаре".to_owned());
    }
    assert!(wrong.is_empty(), "личный календарь: {wrong:#?}");
}

/// Чего программа не знает, того календарь не выдаёт за расчёт (ревью
/// задачи 22, Правило 6): весь доход III квартала — в долларах без курса,
/// строк налога нет — у срока нет «К уплате: 0 ֏», а только «посчитано не
/// всё»; в профиле нет гражданства — нет ни медстраховки, ни декларации,
/// и описание календаря говорит почему. С курсом тот же квартал — с суммой.
#[test]
fn the_personal_calendar_says_what_it_cannot_count() {
    let mut profile = programmer();
    profile.person.residency = None;
    let operations = [unrated_income(day(2026, 8, 15), 300_000)];
    let books = Books {
        documents: &[],
        operations: &operations,
        profile: Some(&profile),
        closings: &[],
        openings: &[],
        unreadable: 0,
    };
    let text = personal(books, day(2026, 10, 5), Lang::Ru, APP_URL).expect("сроки есть");
    let (head, events) = parse(&text);
    let mut wrong = Vec::new();
    let about = head
        .iter()
        .find(|(key, _)| key == "DESCRIPTION")
        .and_then(|(_, value)| unescape(value))
        .unwrap_or_default();
    if !about.contains(t(Lang::Ru, Key::DeadlinesNeedResidency)) || !about.contains("Сверьте с бухгалтером") {
        wrong.push(format!("описание календаря: {about}"));
    }
    match events.iter().find(|event| event.one("UID") == "mine-turnover-2026-q3@kvartik.com") {
        Some(event) => {
            let description = event.text("DESCRIPTION");
            if description.contains("К уплате") || !description.contains(t(Lang::Ru, Key::TaxPartial)) {
                wrong.push(format!("квартал без строк: {description}"));
            }
        }
        None => wrong.push("нет срока III квартала".to_owned()),
    }
    let unknown: Vec<&str> = events
        .iter()
        .map(|event| event.one("UID"))
        .filter(|uid| uid.contains("health") || uid.contains("declaration"))
        .collect();
    if !unknown.is_empty() {
        wrong.push(format!("сроки без гражданства: {unknown:?}"));
    }

    let rated = [income(day(2026, 8, 15), 1_000_000)];
    let counted = personal(Books { operations: &rated, ..books }, day(2026, 10, 5), Lang::Ru, APP_URL).expect("сроки есть");
    if !counted.replace("\r\n ", "").contains("К уплате: 10\u{A0}000\u{A0}֏") {
        wrong.push("квартал с курсом — без суммы".to_owned());
    }
    assert!(wrong.is_empty(), "чего не знает календарь: {wrong:#?}");
}

/// Год микропредпринимательства (задача 100): к 20 апреля следующего года —
/// гербовый сбор, 12 000 ֏ при обороте 3 600 000 ֏ (закон ՀՕ-245-Ն, ст. 12,
/// ч. 1, п. 2), без налога на прибыль и соцплатежа; с нечитаемой записью
/// база может быть больше, и сумма — с «посчитано не всё», как у годовых
/// платежей.
#[test]
fn the_personal_calendar_has_the_micro_stamp_duty() {
    let mut profile = programmer();
    profile.activities = vec![crate::model::activity::ActivityCode::parse("90.01.0").expect("код")];
    profile.micro.insert(2026);
    let operations = [income(day(2026, 3, 10), 3_600_000)];
    let books = Books {
        documents: &[],
        operations: &operations,
        profile: Some(&profile),
        closings: &[],
        openings: &[],
        unreadable: 0,
    };
    let today = day(2026, 10, 5);
    let mut wrong = Vec::new();
    for (unreadable, partial) in [(0, false), (1, true)] {
        let events = personal_events(Books { unreadable, ..books }, today);
        match events.iter().find(|(uid, _, _)| uid == "mine-micro-annual-2026@kvartik.com") {
            Some((_, start, description)) => {
                let first = description.lines().next().unwrap_or_default();
                if start != "20270420" || !first.contains("12\u{A0}000\u{A0}֏") {
                    wrong.push(format!("нечитаемых {unreadable}: {start}, {first}"));
                }
                if description.contains(t(Lang::Ru, Key::AnnualPartialCard)) != partial {
                    wrong.push(format!("нечитаемых {unreadable}: {description}"));
                }
            }
            None => wrong.push(format!("нечитаемых {unreadable}: нет события сбора: {events:?}")),
        }
        if events.iter().any(|(uid, _, _)| uid == "mine-annual-2026@kvartik.com") {
            wrong.push(format!("нечитаемых {unreadable}: годовые платежи налога с оборота"));
        }
    }
    assert!(wrong.is_empty(), "сбор микропредприятия в календаре: {wrong:#?}");
}

/// Файлы для глаз и для импорта в календарь телефона: общий календарь на
/// трёх языках и личный — программиста из теста выше, на 5 октября 2026
/// года. `cargo test -p kvartik-core look_ics -- --ignored`, файлы —
/// в `target/look/task-22/`.
#[test]
#[ignore = "пишет файлы для глаз"]
fn look_ics() {
    use std::fs;
    use std::path::PathBuf;
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/look/task-22");
    fs::create_dir_all(&dir).expect("папка");
    let profile = programmer();
    let operations = [income(day(2026, 8, 15), 4_321_987)];
    let books = Books {
        documents: &[],
        operations: &operations,
        profile: Some(&profile),
        closings: &[],
        openings: &[],
        unreadable: 0,
    };
    for lang in Lang::ALL {
        fs::write(dir.join(format!("public-{}.ics", lang.code())), public(lang)).expect("запись");
        let mine = personal(books, day(2026, 10, 5), lang, APP_URL).expect("сроки есть");
        fs::write(dir.join(format!("mine-{}.ics", lang.code())), mine).expect("запись");
    }
}
