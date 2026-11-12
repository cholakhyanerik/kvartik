//! Браузерные тесты чисел для расчёта (задача 24): всё окно в настоящем
//! DOM и настоящей IndexedDB. Профиль и записи книги кладутся в базу
//! командами ядра; числа посчитаны вручную, номера полей — из приказа
//! 299-Ն (раскладку проверяют тесты ядра, `tax/report/tests.rs`).
//!
//! Буфер обмена — заглушкой: от `click()` из теста браузер нажатия не
//! видит и в настоящий буфер писать не даёт (`platform::clipboard`), а
//! заглушка запоминает, что окно в него положило.

use js_sys::{Function, Object, Reflect};
use kvartik_core::i18n::{self, Key, Lang, fill, t};
use kvartik_core::model::activity::ActivityCode;
use kvartik_core::model::date::Date;
use kvartik_core::model::ledger::{Flow, Nature, Operation, OperationData, Proof};
use kvartik_core::model::money::{Currency, Money};
use kvartik_core::model::profile::Profile;
use kvartik_core::model::text::Line;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::wasm_bindgen_test;
use web_sys::HtmlElement;

use super::documents::{commit, forget_device, profile};
use super::{mount_at, sleep, wait_for};
use crate::platform::{clock, set_hash};
use crate::store::Writes;
use crate::store::device::Device;

fn drams(lang: Lang, drams: i64) -> String {
    i18n::money(lang, Money::new(drams, Currency::Amd))
}

/// Запись книги дня `date` на `drams` драмов: доход или общий расход по
/// «հաշիվ վավերագիր».
fn entry(flow: Flow, date: Date, drams: i64) -> Operation {
    let expense = flow == Flow::Expense;
    Operation::create(
        OperationData {
            flow,
            date,
            paid: None,
            amount: Money::new(drams, Currency::Amd),
            rate: None,
            text: Line::new(if expense { "Аренда" } else { "Дизайн сайта" }).expect("текст"),
            client: None,
            supplier: Line::default(),
            proof: expense.then_some(Proof::SettlementInvoice),
            nature: expense.then_some(Nature::Ordinary),
            cost: None,
            activity: None,
            income_kind: None,
            attachments: Vec::new(),
        },
        clock::now(),
    )
    .expect("годная запись")
}

/// Куда заглушка буфера кладёт скопированное.
const COPIED: &str = "__kvartikCopied";

/// Подменить `navigator.clipboard`: `writeText` запоминает текст и отвечает
/// успехом, а при `refuse` — отказом `NotAllowedError`, как браузер без
/// нажатия. Своё свойство у `navigator` закрывает настоящее, а
/// [`unstub_clipboard`] его снимает.
fn stub_clipboard(refuse: bool) {
    let answer = if refuse {
        "return Promise.reject(new DOMException('отказ', 'NotAllowedError'));"
    } else {
        "return Promise.resolve();"
    };
    let write = Function::new_with_args("text", &format!("window.{COPIED} = text; {answer}"));
    let board = Object::new();
    Reflect::set(&board, &"writeText".into(), &write).expect("ставится");
    let descriptor = Object::new();
    Reflect::set(&descriptor, &"value".into(), &board).expect("ставится");
    Reflect::set(&descriptor, &"configurable".into(), &true.into()).expect("ставится");
    let navigator = web_sys::window().expect("окно есть").navigator();
    Object::define_property(navigator.as_ref(), &"clipboard".into(), &descriptor);
}

/// Вернуть настоящий `navigator.clipboard` и забыть скопированное.
fn unstub_clipboard() {
    let window = web_sys::window().expect("окно есть");
    let _ = Reflect::delete_property(window.navigator().as_ref(), &"clipboard".into());
    let _ = Reflect::delete_property(window.as_ref(), &COPIED.into());
}

/// Что окно положило в буфер последним; `None` — ничего.
fn copied() -> Option<String> {
    let window = web_sys::window()?;
    Reflect::get(window.as_ref(), &COPIED.into()).ok()?.as_string()
}

/// Текст того, что нашёл `selector`; не нашёл — пусто.
fn text_of(host: &HtmlElement, selector: &str) -> String {
    host.query_selector(selector)
        .ok()
        .flatten()
        .and_then(|node| node.text_content())
        .unwrap_or_default()
}

/// Тексты полей экрана чисел: номер поля и весь текст строки.
fn fields(host: &HtmlElement) -> Vec<(String, String)> {
    let Ok(items) = host.query_selector_all("#report .report-field") else {
        return Vec::new();
    };
    (0..items.length())
        .filter_map(|at| items.item(at))
        .filter_map(|item| item.dyn_into::<HtmlElement>().ok())
        .map(|item| {
            let number = item
                .query_selector(".report-no")
                .ok()
                .flatten()
                .and_then(|node| node.text_content())
                .unwrap_or_default();
            (number, item.text_content().unwrap_or_default())
        })
        .collect()
}

/// Текст поля с этим номером; `None` — такого поля нет или их больше
/// одного.
fn field(host: &HtmlElement, number: &str) -> Option<String> {
    let mut found = fields(host).into_iter().filter(|(own, _)| own == number);
    match (found.next(), found.next()) {
        (Some((_, text)), None) => Some(text),
        _ => None,
    }
}

/// Кнопка экрана чисел с этой подписью для диктора: такие же стрелки
/// квартала есть и на спрятанном экране налога.
fn labelled(host: &HtmlElement, label: &str) -> Option<HtmlElement> {
    host.query_selector(&format!("#report button[aria-label=\"{label}\"]"))
        .ok()
        .flatten()
        .map(JsCast::unchecked_into)
}

/// Подождать, пока у поля `number` появится слово `key`; `false` — не
/// дождались за пять секунд.
async fn said_at(host: &HtmlElement, number: &str, key: Key, lang: Lang) -> bool {
    for _ in 0..50 {
        if field(host, number).is_some_and(|text| text.contains(t(lang, key))) {
            return true;
        }
        sleep(100).await;
    }
    false
}

/// Дизайнер (74.10.0) в прошлом квартале: доход 1 000 000 ֏ и общий
/// расход 950 000 ֏ — налог 10 % упёрся в предел 45 000, вычет 57 000,
/// перенос 2 000. На экране налога — кнопка к числам; на экране чисел —
/// поля раздела 9 и строк 21, 22, 27 с числами, а 9.4.1 и 9.4.2 не заполнены:
/// о расходе не сказано, прямой ли он (задача 79). «Скопировать» у поля
/// кладёт в буфер число цифрами и говорит
/// «скопировано» у этого поля и ни у какого другого, отказ буфера — тоже
/// у него; «Скопировать всё» кладёт все поля текстом. Квартал — общий
/// с экраном налога.
#[wasm_bindgen_test]
async fn the_numbers_follow_the_form() {
    crate::store::delete_database(crate::store::DB_NAME).await;
    let today = clock::today().expect("часы с датой");
    let previous = today.quarter().previous().expect("прошлый квартал есть");
    let mut data = profile();
    data.activities = vec![ActivityCode::parse("74.10.0").expect("код")];
    let saved = Profile::save(None, data, clock::now()).expect("годный профиль").expect("новый");
    let mut writes = Writes::default();
    writes.insert(&saved);
    writes.insert(&entry(Flow::Income, previous.first_day(), 1_000_000));
    writes.insert(&entry(Flow::Expense, previous.first_day(), 950_000));
    commit(writes).await;

    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let (host, mounted) = mount_at("#/tax");
    let mut wrong = Vec::new();
    if !wait_for(&host, t(lang, Key::ReportOpen)).await {
        wrong.push("на экране налога нет кнопки к числам для расчёта".to_owned());
    }
    match host.query_selector("#tax a.report-open").ok().flatten() {
        Some(link) => link.unchecked_ref::<HtmlElement>().click(),
        None => wrong.push("кнопка к числам — не ссылка".to_owned()),
    }
    if !wait_for(&host, t(lang, Key::ReportCopyAll)).await {
        wrong.push("экран чисел не открылся".to_owned());
    }
    let screen = text_of(&host, "#report");
    if !screen.contains(&i18n::quarter(lang, previous)) {
        wrong.push("экран чисел — не на квартале экрана налога".to_owned());
    }
    for (number, amount) in [
        ("9.1", 1_000_000),
        ("9.3", 100_000),
        ("9.4", 950_000),
        ("9.6", 57_000),
        ("9.9", 45_000),
        ("9.10", 45_000),
        ("9.11", 2_000),
        ("27", 1_000_000),
    ] {
        if !field(&host, number).is_some_and(|text| text.contains(&drams(lang, amount))) {
            wrong.push(format!("нет поля {number} с {}", drams(lang, amount)));
        }
    }
    for number in ["9.4.1", "9.4.2"] {
        if !field(&host, number).is_some_and(|text| text.contains(t(lang, Key::ReportSplitYourself))) {
            wrong.push(format!("у {number} не сказано «разделите сами»"));
        }
    }
    for heading in [
        fill(t(lang, Key::ReportHeading), &["9", t(lang, Key::KindOther)]),
        fill(t(lang, Key::ReportHeading), &["21", t(lang, Key::KindOther)]),
        fill(t(lang, Key::ReportHeading), &["22", t(lang, Key::ReportTotal)]),
        t(lang, Key::ReportCrossed).to_owned(),
        t(lang, Key::ReportAuto).to_owned(),
    ] {
        if !screen.contains(&heading) {
            wrong.push(format!("нет «{heading}»"));
        }
    }

    let copy = fill(
        t(lang, Key::ReportCopyField),
        &[&fill(t(lang, Key::ReportHeading), &["9.10", t(lang, Key::ReportTax)])],
    );
    stub_clipboard(false);
    match labelled(&host, &copy) {
        Some(button) => button.click(),
        None => wrong.push(format!("нет кнопки «{copy}»")),
    }
    if !said_at(&host, "9.10", Key::ReportCopied, lang).await {
        wrong.push("у 9.10 не сказано «скопировано»".to_owned());
    }
    if copied().as_deref() != Some("45000") {
        wrong.push(format!("в буфере не 45000 цифрами, а {:?}", copied()));
    }
    let elsewhere: Vec<String> = fields(&host)
        .into_iter()
        .filter(|(number, text)| number != "9.10" && text.contains(t(lang, Key::ReportCopied)))
        .map(|(number, _)| number)
        .collect();
    if !elsewhere.is_empty() {
        wrong.push(format!("«скопировано» и у других полей: {elsewhere:?}"));
    }

    stub_clipboard(true);
    match labelled(&host, &copy) {
        Some(button) => button.click(),
        None => wrong.push(format!("нет кнопки «{copy}» во второй раз")),
    }
    if !said_at(&host, "9.10", Key::ReportCopyFailed, lang).await {
        wrong.push("отказ буфера у 9.10 не сказан словами".to_owned());
    }

    stub_clipboard(false);
    match host
        .query_selector("#report .report-all button")
        .ok()
        .flatten()
    {
        Some(button) => button.unchecked_ref::<HtmlElement>().click(),
        None => wrong.push("нет «Скопировать всё»".to_owned()),
    }
    sleep(200).await;
    let all = copied().unwrap_or_default();
    for line in ["9.10 Налог к уплате: 45000", "9.4.1 Прямые расходы: разделите сами", "22 Налог [Գ]: 45000"] {
        if !all.lines().any(|own| own == line) {
            wrong.push(format!("в тексте «Скопировать всё» нет «{line}»"));
        }
    }
    if !text_of(&host, "#report .report-all").contains(t(lang, Key::ReportCopied)) {
        wrong.push("у «Скопировать всё» не сказано «скопировано»".to_owned());
    }
    unstub_clipboard();

    // Квартал — один на оба экрана.
    match labelled(&host, t(lang, Key::LedgerNextQuarter)) {
        Some(next) => next.click(),
        None => wrong.push("нет стрелки следующего квартала".to_owned()),
    }
    sleep(100).await;
    let next = i18n::quarter(lang, previous.next().expect("следующий квартал есть"));
    if !text_of(&host, "#tax .quarter-name").contains(&next) {
        wrong.push(format!("экран налога не перешёл на {next}"));
    }

    drop(mounted);
    host.remove();
    set_hash("");
    forget_device();
    assert!(wrong.is_empty(), "числа для расчёта: {wrong:#?}");
}
