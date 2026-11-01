//! Браузерные тесты налога (задача 19): всё окно в настоящем DOM
//! и настоящей IndexedDB. Профиль и записи книги кладутся в базу командами
//! ядра — их формы проверяют свои тесты; число налога посчитано вручную.

use kvartik_core::i18n::{self, Key, Lang, fill, t};
use kvartik_core::model::activity::ActivityCode;
use kvartik_core::model::date::{Date, Quarter};
use kvartik_core::model::ledger::{Flow, Nature, Operation, OperationData, Proof};
use kvartik_core::model::money::{Currency, Money};
use kvartik_core::model::profile::{Profile, ProfileData};
use kvartik_core::model::text::Line;
use kvartik_core::tax::closing::Closing;
use kvartik_core::tax::opening::Opening;
use kvartik_core::tax::turnover::{self, Books};
use wasm_bindgen::JsCast;
use wasm_bindgen_test::wasm_bindgen_test;
use web_sys::{HtmlElement, HtmlInputElement};

use super::documents::{commit, forget_device, profile};
use super::{button, mount_at, sleep, type_into, wait_for};
use crate::platform::{clock, set_hash};
use crate::store::device::Device;
use crate::store::{Store, Writes};

fn drams(lang: Lang, drams: i64) -> String {
    i18n::money(lang, Money::new(drams, Currency::Amd))
}

/// Запись книги в драмах — как её кладёт форма.
fn operation(flow: Flow, date: Date, amount: i64) -> Operation {
    let expense = flow == Flow::Expense;
    Operation::create(
        OperationData {
            flow,
            date,
            paid: None,
            amount: Money::new(amount, Currency::Amd),
            rate: None,
            text: Line::new("Дизайн сайта").expect("текст"),
            client: None,
            supplier: Line::default(),
            proof: expense.then_some(Proof::SettlementInvoice),
            nature: expense.then_some(Nature::Ordinary),
            activity: None,
            attachments: Vec::new(),
        },
        clock::now(),
    )
    .expect("годная запись")
}

/// В базе — дизайнер (74.10.0), записи `before`, квартал `closed`, закрытый
/// по ним, записи `after`, внесённые уже после закрытия, и коды `codes`
/// в профиле — такие, какими они стали после него.
async fn closed_then_changed(closed: Quarter, before: &[Operation], after: &[Operation], codes: &[&str]) {
    let today = clock::today().expect("часы с датой");
    let now = clock::now();
    let code = |text: &str| ActivityCode::parse(text).expect("код");
    let mut data: ProfileData = profile();
    data.activities = vec![code("74.10.0")];
    let books = Books {
        documents: &[],
        operations: before,
        profile: Some(&data),
        closings: &[],
        openings: &[],
        unreadable: 0,
    };
    let closing = Closing::close(&turnover::quarter(books, closed), today, now).expect("квартал кончился");
    data.activities = codes.iter().map(|text| code(text)).collect();
    let saved = Profile::save(None, data, now).expect("годный профиль").expect("новый");
    let mut writes = Writes::default();
    writes.insert(&saved);
    writes.insert(&closing);
    for operation in before.iter().chain(after) {
        writes.insert(operation);
    }
    commit(writes).await;
}

/// Текст элемента окна; нет его — пустой.
fn text(host: &HtmlElement, selector: &str) -> String {
    host.query_selector(selector)
        .ok()
        .flatten()
        .and_then(|found| found.text_content())
        .unwrap_or_default()
}

/// Карточки налога на главном экране.
fn cards(host: &HtmlElement) -> Vec<HtmlElement> {
    host.query_selector_all("#home .hero-card")
        .map(|found| {
            (0..found.length())
                .filter_map(|at| found.item(at))
                .map(|node| node.unchecked_into::<HtmlElement>())
                .collect()
        })
        .unwrap_or_default()
}

/// Снимки закрытых кварталов из базы программы.
async fn closings() -> Vec<Closing> {
    let store = Store::open(|_| {}).await.expect("база открыта");
    let tx = store.read().expect("транзакция чтения");
    tx.list::<Closing>().await.expect("снимки читаются").records
}

/// Записи переноса из базы программы.
async fn openings() -> Vec<Opening> {
    let store = Store::open(|_| {}).await.expect("база открыта");
    let tx = store.read().expect("транзакция чтения");
    tx.list::<Opening>().await.expect("перенос читается").records
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

/// Забытый доход в закрытом квартале. Позапрошлый закрыт с налогом
/// 1 000 000 × 10 % = 100 000, потом внесли ещё 200 000: сейчас 120 000.
/// Прошлый не закрыт: 500 000 × 10 % = 50 000 к уплате. Экран налога сразу на
/// закрытом — с числами снимка и обеими суммами; на главном экране — карточка
/// расхождения над обычной, и каждая ведёт на свой квартал; «Открыть снова»
/// показывает пересчёт, а «Вернуть» — снова снимок.
#[wasm_bindgen_test]
async fn a_closed_quarter_that_changed_comes_first() {
    crate::store::delete_database(crate::store::DB_NAME).await;
    let today = clock::today().expect("часы с датой");
    let previous = today.quarter().previous().expect("прошлый квартал есть");
    let closed = previous.previous().expect("позапрошлый квартал есть");
    closed_then_changed(
        closed,
        &[operation(Flow::Income, closed.first_day(), 1_000_000)],
        &[
            operation(Flow::Income, closed.last_day(), 200_000),
            operation(Flow::Income, previous.first_day(), 500_000),
        ],
        &["74.10.0"],
    )
    .await;

    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let (host, mounted) = mount_at("#/tax");
    let mut wrong = Vec::new();
    let (now, before) = (drams(lang, 120_000), drams(lang, 100_000));
    let changed = fill(t(lang, Key::TaxChangedAfterClose), &[&now, &before]);
    let snapshot = fill(
        t(lang, Key::TaxIncomeTimesRate),
        &[&drams(lang, 1_000_000), &i18n::percent(lang, 1_000)],
    );
    if !wait_for(&host, &changed).await {
        wrong.push("экран налога не открылся на закрытом квартале, чей расчёт разошёлся".to_owned());
    }
    let name = text(&host, "#tax .quarter-name");
    if name != i18n::quarter(lang, closed) {
        wrong.push(format!("квартал экрана: «{name}»"));
    }
    let hero = text(&host, "#tax .hero-card");
    if !hero.contains(&before) || !hero.contains(&snapshot) || hero.contains(&now) {
        wrong.push(format!("у закрытого квартала не числа снимка: «{hero}»"));
    }
    let screen = text(&host, "#tax");
    if screen.contains(t(lang, Key::TaxByMonth)) {
        wrong.push("месяцы пересчёта рядом с числами снимка".to_owned());
    }
    if !screen.contains(t(lang, Key::TaxClosedNow)) {
        wrong.push("данные сейчас не отделены от числа снимка".to_owned());
    }

    set_hash("#/");
    if !wait_for(&host, t(lang, Key::TaxDrift)).await {
        wrong.push("на главном нет карточки расхождения".to_owned());
    }
    match cards(&host).as_slice() {
        [drift, due] => {
            let (first, second) = (drift.text_content().unwrap_or_default(), due.text_content().unwrap_or_default());
            let was = fill(t(lang, Key::TaxDriftBefore), &[&before]);
            if !first.contains(&i18n::quarter(lang, closed)) || !first.contains(&now) || !first.contains(&was) {
                wrong.push(format!("карточка расхождения: «{first}»"));
            }
            if !second.contains(&i18n::quarter(lang, previous)) || !second.contains(&drams(lang, 50_000)) {
                wrong.push(format!("карточка к уплате: «{second}»"));
            }
            due.click();
            sleep(100).await;
            let name = text(&host, "#tax .quarter-name");
            if name != i18n::quarter(lang, previous) {
                wrong.push(format!("карточка к уплате открыла квартал «{name}»"));
            }
            set_hash("#/");
            sleep(100).await;
            drift.click();
            sleep(100).await;
            let name = text(&host, "#tax .quarter-name");
            if name != i18n::quarter(lang, closed) {
                wrong.push(format!("карточка расхождения открыла квартал «{name}»"));
            }
        }
        other => wrong.push(format!("карточек на главном: {}", other.len())),
    }

    match button(&host, &[t(lang, Key::TaxReopen)]) {
        Some(reopen) => reopen.click(),
        None => wrong.push("нет «Открыть снова»".to_owned()),
    }
    if !wait_for(&host, t(lang, Key::TaxReopenedNotice)).await {
        wrong.push("квартал не открылся снова".to_owned());
    }
    let hero = text(&host, "#tax .hero-card");
    if !hero.contains(&now) {
        wrong.push(format!("у открытого квартала не пересчёт: «{hero}»"));
    }
    match button(&host, &[t(lang, Key::DocUnannul)]) {
        Some(undo) => undo.click(),
        None => wrong.push("нет «Вернуть»".to_owned()),
    }
    if !wait_for(&host, t(lang, Key::TaxRestoredNotice)).await {
        wrong.push("снимок не вернулся".to_owned());
    }
    match closings().await.as_slice() {
        [restored] if !restored.is_deleted() && restored.data().tax == 100_000 => {}
        other => wrong.push(format!("снимки после «Вернуть»: {other:?}")),
    }
    let hero = text(&host, "#tax .hero-card");
    if !hero.contains(&before) || !text(&host, "#tax").contains(&changed) {
        wrong.push(format!("после «Вернуть» экран не на снимке: «{hero}»"));
    }

    drop(mounted);
    host.remove();
    set_hash("");
    forget_device();
    assert!(wrong.is_empty(), "расхождение закрытого квартала: {wrong:#?}");
}

/// Налог упёрся в нижний предел: доход 100 000 — предел 4 500, — а забытый
/// расход поменял только перенос. Две одинаковые суммы ничего бы не сказали,
/// и окно говорит, что налог тот же.
#[wasm_bindgen_test]
async fn a_change_of_lines_alone_says_the_tax_is_the_same() {
    crate::store::delete_database(crate::store::DB_NAME).await;
    let today = clock::today().expect("часы с датой");
    let closed = today.quarter().previous().expect("прошлый квартал есть");
    closed_then_changed(
        closed,
        &[
            operation(Flow::Income, closed.first_day(), 100_000),
            operation(Flow::Expense, closed.first_day(), 2_000_000),
        ],
        &[operation(Flow::Expense, closed.last_day(), 100_000)],
        &["74.10.0"],
    )
    .await;

    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let (host, mounted) = mount_at("#/");
    let mut wrong = Vec::new();
    if !wait_for(&host, t(lang, Key::TaxDriftSameTax)).await {
        wrong.push("карточка не сказала, что налог тот же".to_owned());
    }
    set_hash("#/tax");
    if !wait_for(&host, &fill(t(lang, Key::TaxChangedSameTax), &[&drams(lang, 4_500)])).await {
        wrong.push("экран не сказал, что налог тот же".to_owned());
    }

    drop(mounted);
    host.remove();
    set_hash("");
    forget_device();
    assert!(wrong.is_empty(), "расчёт с тем же налогом: {wrong:#?}");
}

/// Квартал закрыт пустым, а потом внесли забытый доход: 300 000 × 10 % =
/// 30 000. Подпись — о закрытом расчёте: «в этом квартале дохода нет» было
/// бы неправдой.
#[wasm_bindgen_test]
async fn a_quarter_closed_empty_speaks_of_the_return() {
    crate::store::delete_database(crate::store::DB_NAME).await;
    let today = clock::today().expect("часы с датой");
    let closed = today.quarter().previous().expect("прошлый квартал есть");
    closed_then_changed(closed, &[], &[operation(Flow::Income, closed.first_day(), 300_000)], &["74.10.0"]).await;

    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let (host, mounted) = mount_at("#/tax");
    let mut wrong = Vec::new();
    let changed = fill(t(lang, Key::TaxChangedAfterClose), &[&drams(lang, 30_000), &drams(lang, 0)]);
    if !wait_for(&host, &changed).await {
        wrong.push("экран не открылся на квартале с расхождением".to_owned());
    }
    let hero = text(&host, "#tax .hero-card");
    if !hero.contains(t(lang, Key::TaxFiledNoIncome)) || hero.contains(t(lang, Key::TaxNoIncome)) {
        wrong.push(format!("подпись карточки: «{hero}»"));
    }

    drop(mounted);
    host.remove();
    set_hash("");
    forget_device();
    assert!(wrong.is_empty(), "квартал, закрытый пустым: {wrong:#?}");
}

/// Перенос с кварталов до программы. Прошлый квартал — первый с записями:
/// 1 000 000 × 10 % = 100 000, и окно спрашивает о переносе. Указали 12 000 —
/// налог 88 000, вопроса больше нет, запись в базе, и экран показывает её.
/// Убрали сумму — налог снова 100 000: «переноса не было» — тоже ответ.
#[wasm_bindgen_test]
async fn the_carry_from_before_the_program_is_entered() {
    crate::store::delete_database(crate::store::DB_NAME).await;
    let today = clock::today().expect("часы с датой");
    let quarter = today.quarter().previous().expect("прошлый квартал есть");
    let mut data = profile();
    data.activities = vec![ActivityCode::parse("74.10.0").expect("код")];
    let saved = Profile::save(None, data, clock::now()).expect("годный профиль").expect("новый");
    let mut writes = Writes::default();
    writes.insert(&saved);
    writes.insert(&operation(Flow::Income, quarter.first_day(), 1_000_000));
    commit(writes).await;

    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let (host, mounted) = mount_at("#/tax");
    let mut wrong = Vec::new();
    if !wait_for(&host, t(lang, Key::TaxOpeningAsk)).await {
        wrong.push("окно не спросило о переносе".to_owned());
    }
    if !text(&host, "#tax").contains(t(lang, Key::TaxCarryUnknown)) {
        wrong.push("нет слов о неизвестном переносе".to_owned());
    }
    match button(&host, &[t(lang, Key::TaxOpeningAsk)]) {
        Some(ask) => ask.click(),
        None => wrong.push("нет «Указать перенос»".to_owned()),
    }
    if !wait_for(&host, t(lang, Key::TaxOpeningSave)).await {
        wrong.push("форма не открылась".to_owned());
    }
    let field = |host: &HtmlElement| -> Option<HtmlInputElement> {
        let inputs = host.query_selector_all("#tax .tax-opening label input").ok()?;
        (0..inputs.length()).filter_map(|at| inputs.item(at)).map(|node| node.unchecked_into::<HtmlInputElement>()).next_back()
    };
    match field(&host) {
        Some(input) => type_into(&input, &i18n::amount(lang, Money::new(12_000, Currency::Amd))),
        None => wrong.push("нет поля переноса".to_owned()),
    }
    match button(&host, &[t(lang, Key::TaxOpeningSave)]) {
        Some(save) => save.click(),
        None => wrong.push("нет «Сохранить перенос»".to_owned()),
    }
    if !wait_for(&host, t(lang, Key::TaxOpeningSaved)).await {
        wrong.push("перенос не сохранился".to_owned());
    }
    sleep(100).await;
    let hero = text(&host, "#tax .hero-card");
    if !hero.contains(&drams(lang, 88_000)) {
        wrong.push(format!("налог с переносом: «{hero}»"));
    }
    let screen = text(&host, "#tax");
    if screen.contains(t(lang, Key::TaxCarryUnknown)) || !screen.contains(t(lang, Key::TaxOpeningShown)) {
        wrong.push(format!("экран с переносом: «{screen}»"));
    }
    match openings().await.as_slice() {
        [opening] if opening.quarter() == quarter && opening.carry().values().eq([&12_000]) => {}
        other => wrong.push(format!("записи переноса: {other:?}")),
    }

    match button(&host, &[t(lang, Key::TaxOpeningChange)]) {
        Some(change) => change.click(),
        None => wrong.push("нет «Изменить»".to_owned()),
    }
    if !wait_for(&host, t(lang, Key::TaxOpeningSave)).await {
        wrong.push("форма не открылась снова".to_owned());
    }
    match field(&host) {
        Some(input) if input.value() == i18n::amount(lang, Money::new(12_000, Currency::Amd)) => type_into(&input, ""),
        other => wrong.push(format!("поле не заполнено сохранённым: {:?}", other.map(|input| input.value()))),
    }
    match button(&host, &[t(lang, Key::TaxOpeningSave)]) {
        Some(save) => save.click(),
        None => wrong.push("нет «Сохранить перенос»".to_owned()),
    }
    if !wait_for(&host, t(lang, Key::TaxOpeningNone)).await {
        wrong.push("окно не сказало, что переноса не было".to_owned());
    }
    sleep(100).await;
    if !text(&host, "#tax .hero-card").contains(&drams(lang, 100_000)) {
        wrong.push(format!("налог без переноса: «{}»", text(&host, "#tax .hero-card")));
    }
    match openings().await.as_slice() {
        [opening] if opening.carry().is_empty() => {}
        other => wrong.push(format!("записи после правки: {other:?}")),
    }

    drop(mounted);
    host.remove();
    set_hash("");
    forget_device();
    assert!(wrong.is_empty(), "перенос до программы: {wrong:#?}");
}

/// Второй код другой строки налога в профиле: доход без вида стал ничьим,
/// и расчёт не посчитан, хотя поданное верно. Карточки расхождения нет,
/// а экран квартала говорит словами, без нуля, и показывает снимок.
#[wasm_bindgen_test]
async fn an_uncounted_quarter_is_no_alarm_and_no_zero() {
    crate::store::delete_database(crate::store::DB_NAME).await;
    let today = clock::today().expect("часы с датой");
    let closed = today.quarter().previous().expect("прошлый квартал есть");
    closed_then_changed(
        closed,
        &[operation(Flow::Income, closed.first_day(), 1_000_000)],
        &[],
        &["74.10.0", "62.01.0"],
    )
    .await;

    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let (host, mounted) = mount_at("#/");
    let mut wrong = Vec::new();
    // Карточки главного загрузились: годовые платежи всегда есть с профилем.
    sleep(500).await;
    if text(&host, "#home").contains(t(lang, Key::TaxDrift)) {
        wrong.push("тревога там, где поданное верно".to_owned());
    }
    set_hash("#/tax");
    if !wait_for(&host, t(lang, Key::TaxTurnover)).await {
        wrong.push("экран налога не открылся".to_owned());
    }
    // Расхождения нет — экран на текущем квартале, а закрытый перед ним.
    match host.query_selector("#tax .quarter-bar button").ok().flatten() {
        Some(back) => back.unchecked_into::<HtmlElement>().click(),
        None => wrong.push("нет стрелки квартала".to_owned()),
    }
    let before = drams(lang, 100_000);
    let said = fill(t(lang, Key::TaxChangedNotCounted), &[&before]);
    if !wait_for(&host, &said).await {
        wrong.push(format!("экран не сказал: «{said}»"));
    }
    let hero = text(&host, "#tax .hero-card");
    if !hero.contains(&before) {
        wrong.push(format!("нет числа снимка: «{hero}»"));
    }

    drop(mounted);
    host.remove();
    set_hash("");
    forget_device();
    assert!(wrong.is_empty(), "не посчитанный квартал: {wrong:#?}");
}
