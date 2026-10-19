//! Браузерные тесты налога (задача 19): всё окно в настоящем DOM
//! и настоящей IndexedDB. Профиль и записи книги кладутся в базу командами
//! ядра — их формы проверяют свои тесты; число налога посчитано вручную.

use kvartik_core::i18n::{self, Key, Lang, fill, t};
use kvartik_core::model::activity::ActivityCode;
use kvartik_core::model::ledger::{Flow, Operation, OperationData};
use kvartik_core::model::money::{Currency, Money};
use kvartik_core::model::profile::Profile;
use kvartik_core::model::text::Line;
use kvartik_core::tax::closing::Closing;
use wasm_bindgen_test::wasm_bindgen_test;
use web_sys::HtmlElement;

use super::documents::{commit, forget_device, profile};
use super::{button, mount_at, sleep, wait_for};
use crate::platform::{clock, set_hash};
use crate::store::device::Device;
use crate::store::{Store, Writes};

fn drams(lang: Lang, drams: i64) -> String {
    i18n::money(lang, Money::new(drams, Currency::Amd))
}

/// Снимки закрытых кварталов из базы программы.
async fn closings() -> Vec<Closing> {
    let store = Store::open(|_| {}).await.expect("база открыта");
    let tx = store.read().expect("транзакция чтения");
    tx.list::<Closing>().await.expect("снимки читаются").records
}

/// Текст карточки налога на главном экране; `None` — карточки нет.
fn card(host: &HtmlElement) -> Option<String> {
    host.query_selector("#home .hero-card").ok().flatten().and_then(|card| card.text_content())
}

/// Налог прошлого квартала: дизайнер (74.10.0) с доходом 1 000 000 ֏ — 10 %,
/// 100 000 ֏ (предел 4,5 % — 45 000). Главный экран показывает его «к
/// уплате», экран налога — с доходом и ставкой; квартал закрывается —
/// снимок ложится в базу, а карточка уходит: в текущем квартале сказать
/// нечего; «Открыть снова» снимает закрытие.
#[wasm_bindgen_test]
async fn the_quarter_tax_is_shown_closed_and_reopened() {
    crate::store::delete_database(crate::store::DB_NAME).await;
    let today = clock::today().expect("часы с датой");
    let previous = today.quarter().previous().expect("прошлый квартал есть");
    let now = clock::now();
    let mut data = profile();
    data.activities = vec![ActivityCode::parse("74.10.0").expect("код")];
    let saved = Profile::save(None, data, now).expect("годный профиль").expect("новый");
    let income = Operation::create(
        OperationData {
            flow: Flow::Income,
            date: previous.first_day(),
            paid: None,
            amount: Money::new(1_000_000, Currency::Amd),
            rate: None,
            text: Line::new("Дизайн сайта").expect("текст"),
            client: None,
            supplier: Line::default(),
            proof: None,
            nature: None,
            activity: None,
            attachments: Vec::new(),
        },
        now,
    )
    .expect("годная запись");
    let mut writes = Writes::default();
    writes.insert(&saved);
    writes.insert(&income);
    commit(writes).await;

    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let (host, mounted) = mount_at("#/");
    let mut wrong = Vec::new();
    let tax = drams(lang, 100_000);
    if !wait_for(&host, t(lang, Key::TaxToPay)).await {
        wrong.push("на главном нет карточки налога к уплате".to_owned());
    }
    match card(&host) {
        Some(text) if text.contains(&tax) && text.contains(&i18n::quarter(lang, previous)) => {}
        other => wrong.push(format!("карточка: {other:?}")),
    }

    set_hash("#/tax");
    if !wait_for(&host, t(lang, Key::TaxTurnover)).await {
        wrong.push("экран налога не открылся".to_owned());
    }
    let line = fill(
        t(lang, Key::TaxIncomeTimesRate),
        &[&drams(lang, 1_000_000), &i18n::percent(lang, 1_000)],
    );
    let screen = host
        .query_selector("#tax .hero-card")
        .ok()
        .flatten()
        .and_then(|card| card.text_content())
        .unwrap_or_default();
    if !screen.contains(&tax) || !screen.contains(&line) {
        wrong.push(format!("карточка экрана: «{screen}»"));
    }
    match button(&host, &[t(lang, Key::TaxClose)]) {
        Some(close) => close.click(),
        None => wrong.push("нет «Закрыть квартал»".to_owned()),
    }
    if !wait_for(&host, t(lang, Key::TaxClosedNotice)).await {
        wrong.push("квартал не закрылся".to_owned());
    }
    match closings().await.as_slice() {
        [closed] if !closed.is_deleted() && closed.quarter() == previous && closed.data().tax == 100_000 => {}
        other => wrong.push(format!("снимки после закрытия: {other:?}")),
    }
    set_hash("#/");
    sleep(100).await;
    if let Some(text) = card(&host) {
        wrong.push(format!("после закрытия карточка осталась: «{text}»"));
    }

    set_hash("#/tax");
    sleep(100).await;
    match button(&host, &[t(lang, Key::TaxReopen)]) {
        Some(reopen) => reopen.click(),
        None => wrong.push("нет «Открыть снова»".to_owned()),
    }
    if !wait_for(&host, t(lang, Key::TaxReopenedNotice)).await {
        wrong.push("квартал не открылся снова".to_owned());
    }
    match closings().await.as_slice() {
        [reopened] if reopened.is_deleted() => {}
        other => wrong.push(format!("снимки после «Открыть снова»: {other:?}")),
    }

    drop(mounted);
    host.remove();
    set_hash("");
    forget_device();
    assert!(wrong.is_empty(), "налог квартала: {wrong:#?}");
}
