//! Браузерные тесты нескольких ИП (задача 38, этапы 2–3): список конторы,
//! добавление и переход к другому ИП, имя открытого над экранами, отказ
//! перейти, когда начато, «убрать» и «вернуть»; сводка конторы (этап 5).
//! Перезагрузку окна тесты считают (`platform::RELOADS`): настоящая оборвала
//! бы прогон.

use kvartik_core::i18n::{self, Key, Lang, fill, t};
use kvartik_core::license;
use kvartik_core::model::activity::ActivityCode;
use kvartik_core::model::date::{Date, Quarter};
use kvartik_core::model::ledger::{Flow, Operation, OperationData};
use kvartik_core::model::money::{Currency, Money};
use kvartik_core::model::office::{FIRST_BASE, Ip};
use kvartik_core::model::profile::{Profile, ProfileData};
use kvartik_core::model::text::Line;
use kvartik_core::tax::closing::Closing;
use kvartik_core::tax::turnover::{self, Books};
use wasm_bindgen_test::wasm_bindgen_test;
use web_sys::HtmlElement;

use super::documents::{wait_selector, wait_until};
use super::{button, mount_at, section_text, sleep, wait_for};
use crate::platform::{RELOADS, clock};
use crate::store::device::Device;
use crate::store::office::{OFFICE_DB, list, tab_base};
use crate::store::{DB_NAME, Store, Writes, delete_database};

/// Забыть всё, что устройство помнит о нескольких ИП.
pub(super) fn forget_office() {
    let window = web_sys::window().expect("окно есть");
    if let Ok(Some(storage)) = window.local_storage() {
        for name in ["kvartik.office", "kvartik.base", "kvartik.lang"] {
            let _ = storage.remove_item(name);
        }
    }
    if let Ok(Some(storage)) = window.session_storage() {
        let _ = storage.remove_item("kvartik.base");
    }
}

fn reloads() -> u32 {
    RELOADS.with(std::cell::Cell::get)
}

/// Профиль ИП с этим именем на всех языках.
pub(super) fn named(name: &str) -> Profile {
    let mut data = ProfileData::default();
    let line = Line::new(name).expect("годное имя");
    data.name.hy = line.clone();
    data.name.ru = line.clone();
    data.name.en = line;
    Profile::save(None, data, clock::now()).expect("годный профиль").expect("новый")
}

/// Записать в базу `name` этот профиль.
pub(super) async fn seed(name: &str, profile: &Profile) {
    let store = Store::open_base(name, |_| {}).await.expect("база открыта");
    let mut writes = Writes::default();
    writes.insert(profile);
    store.write().expect("транзакция").commit(writes).await.expect("записано");
}

/// Список конторы — записать как есть.
pub(super) async fn seed_office(ips: &[&Ip]) {
    let office = Store::open_office(|_| {}).await.expect("контора открыта");
    let mut writes = Writes::default();
    for ip in ips {
        writes.insert(*ip);
    }
    office.write().expect("транзакция").commit(writes).await.expect("записано");
}

/// Список конторы из базы.
pub(super) async fn stored_office() -> Vec<Ip> {
    let office = Store::open_office(|_| {}).await.expect("контора открыта");
    list(&office).await.expect("список читается").records
}

/// Видимая кнопка с подписью `key` в строке ИП с именем `name`.
pub(super) fn row_button(host: &HtmlElement, name: &str, label: &str) -> Option<HtmlElement> {
    let rows = host.query_selector_all("#office li").ok()?;
    (0..rows.length())
        .filter_map(|at| rows.item(at))
        .map(wasm_bindgen::JsCast::unchecked_into::<HtmlElement>)
        .find(|row| row.text_content().is_some_and(|text| text.contains(name)))
        .and_then(|row| button(&row, &[label]))
}

/// Подождать видимую кнопку `label` в строке ИП `name`: список — из части
/// книги, она рисуется после показа экрана.
pub(super) async fn wait_row_button(host: &HtmlElement, name: &str, label: &str) -> Option<HtmlElement> {
    for _ in 0..100 {
        if let Some(found) = row_button(host, name, label) {
            return Some(found);
        }
        sleep(50).await;
    }
    None
}

/// Экран ИП в программе ведёт одного ИП, пока второго нет: открытый — его
/// имя из профиля, «открыт». «Добавить ИП» заводит список — первый ИП с
/// именем из профиля и новый с пустой своей базой — и открывает новый:
/// вкладка и устройство помнят его базу, окно просит перезагрузку.
#[wasm_bindgen_test]
async fn a_second_ie_is_added_and_opened() {
    delete_database(DB_NAME).await;
    delete_database(OFFICE_DB).await;
    forget_office();
    seed(DB_NAME, &named("ИП Арам Петросян")).await;
    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let (host, mounted) = mount_at("#/office");
    let mut wrong = Vec::new();
    if !wait_for(&host, "ИП Арам Петросян").await {
        wrong.push("на экране нет открытого ИП".to_owned());
    }
    if !section_text(&host, "office").contains(t(lang, Key::OfficeOpenNow)) {
        wrong.push("открытый ИП не назван открытым".to_owned());
    }
    let before = reloads();
    match button(&host, &[t(lang, Key::OfficeAdd)]) {
        Some(add) => add.click(),
        None => wrong.push("нет «Добавить ИП»".to_owned()),
    }
    for _ in 0..100 {
        if reloads() > before {
            break;
        }
        sleep(50).await;
    }
    if reloads() != before + 1 {
        wrong.push(format!("перезагрузок {} вместо одной", reloads() - before));
    }
    let ips = stored_office().await;
    let first = ips.iter().find(|ip| ip.base() == FIRST_BASE);
    let new = ips.iter().find(|ip| ip.base() != FIRST_BASE);
    match (first, new) {
        (Some(first), Some(new)) if ips.len() == 2 => {
            if first.name(lang) != "ИП Арам Петросян" {
                wrong.push(format!("у первого не его имя: {:?}", first.name(lang)));
            }
            if tab_base().as_deref() != Some(new.base()) || Device::new().base().as_deref() != Some(new.base()) {
                wrong.push(format!("помнят не новую базу: {:?}, {:?}", tab_base(), Device::new().base()));
            }
            delete_database(new.base()).await;
        }
        _ => wrong.push(format!("в списке не первый и новый: {ips:?}")),
    }
    if !Device::new().office() {
        wrong.push("устройство не помнит, что ИП несколько".to_owned());
    }
    drop(mounted);
    host.remove();
    crate::platform::set_hash("");
    delete_database(OFFICE_DB).await;
    forget_office();
    assert!(wrong.is_empty(), "второй ИП: {wrong:#?}");
}

/// Вкладка открывает ИП, которого помнит, и над экранами называет его.
/// Другого открывает «Открыть» — но не при открытой форме: тогда только
/// слова, и перезагрузки нет. «Убрать из списка» прячет ИП в убранные,
/// «Вернуть в список» возвращает; открытого убрать нельзя.
#[wasm_bindgen_test]
async fn the_tab_opens_its_ie_and_switches_only_when_free() {
    delete_database(DB_NAME).await;
    delete_database(OFFICE_DB).await;
    forget_office();
    let aram = named("ИП Арам Петросян");
    let anna = named("ИП Анна Акопян");
    let today = clock::today().expect("сегодня");
    let first = {
        let mut ip = Ip::first(Some(&aram), today, clock::now());
        ip.copy_profile(&aram, clock::now());
        ip
    };
    let mut second = Ip::new(today, clock::now());
    second.copy_profile(&anna, clock::now());
    seed(DB_NAME, &aram).await;
    seed(second.base(), &anna).await;
    seed_office(&[&first, &second]).await;
    Device::new().set_office();
    crate::store::office::set_tab_base(second.base());
    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let (host, mounted) = mount_at("#/");
    let mut wrong = Vec::new();
    let bar = fill(t(lang, Key::OfficeBar), &["ИП Анна Акопян"]);
    if !wait_for(&host, &bar).await {
        wrong.push("над экраном не назван открытый ИП".to_owned());
    }

    // Открыта форма профиля — переход отказывает словами.
    crate::platform::set_hash("#/profile");
    if !wait_for(&host, t(lang, Key::ProfileEdit)).await {
        wrong.push("профиль не открылся".to_owned());
    }
    if let Some(edit) = button(&host, &[t(lang, Key::ProfileEdit)]) {
        edit.click();
    }
    sleep(100).await;
    crate::platform::set_hash("#/office");
    let before = reloads();
    match wait_row_button(&host, "ИП Арам Петросян", t(lang, Key::OfficeOpen)).await {
        Some(open) => open.click(),
        None => {
            let where_ = host.query_selector_all("section").ok().map(|sections| {
                (0..sections.length())
                    .filter_map(|at| sections.item(at))
                    .filter(|section| section.text_content().is_some_and(|text| text.contains("ИП Арам Петросян")))
                    .filter_map(|section| wasm_bindgen::JsCast::dyn_into::<web_sys::Element>(section).ok().map(|s| s.id()))
                    .collect::<Vec<_>>()
            });
            wrong.push(format!("у первого ИП нет «Открыть»: {} — имя есть в {where_:?}", section_text(&host, "office")));
        }
    }
    if !wait_for(&host, t(lang, Key::OfficeBusy)).await || reloads() != before {
        wrong.push("при открытой форме переход не отказал словами".to_owned());
    }
    if row_button(&host, "ИП Анна Акопян", t(lang, Key::OfficeRemove)).is_some() {
        wrong.push("открытый ИП можно убрать".to_owned());
    }

    // Убрать и вернуть — метка в списке, а не стирание.
    match row_button(&host, "ИП Арам Петросян", t(lang, Key::OfficeRemove)) {
        Some(remove) => remove.click(),
        None => wrong.push("у первого ИП нет «Убрать из списка»".to_owned()),
    }
    if !wait_for(&host, t(lang, Key::OfficeRemovedTitle)).await {
        wrong.push("убранного не видно в убранных".to_owned());
    }
    if !stored_office().await.iter().any(|ip| ip.base() == FIRST_BASE && ip.is_removed()) {
        wrong.push("в списке первый не убран".to_owned());
    }
    match button(&host, &[t(lang, Key::OfficeBringBack)]) {
        Some(back) => back.click(),
        None => wrong.push("нет «Вернуть в список»".to_owned()),
    }
    for _ in 0..100 {
        if stored_office().await.iter().all(|ip| !ip.is_removed()) {
            break;
        }
        sleep(50).await;
    }
    if stored_office().await.iter().any(Ip::is_removed) {
        wrong.push("«Вернуть в список» не вернуло".to_owned());
    }

    // Форма закрыта — переход идёт: вкладка помнит первую базу.
    crate::platform::set_hash("#/profile");
    sleep(100).await;
    if let Some(cancel) = button(&host, &[t(lang, Key::Cancel)]) {
        cancel.click();
    }
    sleep(100).await;
    crate::platform::set_hash("#/office");
    sleep(200).await;
    match row_button(&host, "ИП Арам Петросян", t(lang, Key::OfficeOpen)) {
        Some(open) => open.click(),
        None => wrong.push("после формы у первого ИП нет «Открыть»".to_owned()),
    }
    sleep(100).await;
    if reloads() != before + 1 || tab_base().as_deref() != Some(FIRST_BASE) {
        wrong.push(format!("переход не открыл первого: {} перезагрузок, вкладка — {:?}", reloads() - before, tab_base()));
    }
    drop(mounted);
    host.remove();
    crate::platform::set_hash("");
    delete_database(second.base()).await;
    delete_database(OFFICE_DB).await;
    forget_office();
    assert!(wrong.is_empty(), "переход между ИП: {wrong:#?}");
}

/// Радиокнопка выпуска на экране настроек (задача 124, часть E, вместо
/// переключателя): `here` — «это устройство», иначе — «другое устройство».
fn issue_choice(host: &HtmlElement, here: bool) -> Option<web_sys::HtmlInputElement> {
    let value = if here { "here" } else { "elsewhere" };
    let found = host
        .query_selector(&format!("#settings li.issuing input[type=radio][value={value}]"))
        .ok()
        .flatten()?;
    Some(wasm_bindgen::JsCast::unchecked_into(found))
}

/// Выпуск у каждого ИП свой (задача 116): при двух ИП переключатель
/// настроек выключает выпуск только открытого, слова говорят, что у каждого
/// ИП выбор свой и что сейчас он не у всех одинаков, а «Сделать у всех ИП
/// так же» отдаёт выбор открытого всем; включённый снова — опять только у
/// открытого.
#[wasm_bindgen_test]
async fn issuing_is_switched_per_ie() {
    delete_database(DB_NAME).await;
    delete_database(OFFICE_DB).await;
    forget_office();
    crate::ui::settings::set_issue_elsewhere(None, false);
    let aram = named("ИП Арам Петросян");
    let anna = named("ИП Анна Акопян");
    let today = clock::today().expect("сегодня");
    let first = {
        let mut ip = Ip::first(Some(&aram), today, clock::now());
        ip.copy_profile(&aram, clock::now());
        ip
    };
    let mut second = Ip::new(today, clock::now());
    second.copy_profile(&anna, clock::now());
    seed(DB_NAME, &aram).await;
    seed(second.base(), &anna).await;
    seed_office(&[&first, &second]).await;
    Device::new().set_office();
    crate::store::office::set_tab_base(second.base());
    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let (host, mounted) = mount_at("#/settings");
    let mut wrong = Vec::new();
    let bar = fill(t(lang, Key::OfficeBar), &["ИП Анна Акопян"]);
    if !wait_for(&host, &bar).await {
        wrong.push("над экраном не назван открытый ИП".to_owned());
    }
    if !wait_selector(&host, "#settings li.issuing input[type=radio][value=here]").await {
        wrong.push("на экране настроек нет выбора, кто выпускает".to_owned());
    }
    if !wait_for(&host, t(lang, Key::SettingsIssueEach)).await {
        wrong.push("при двух ИП не сказано, что выбор у каждого свой".to_owned());
    }
    if button(&host, &[t(lang, Key::SettingsIssueAll)]).is_some() {
        wrong.push("«так же у всех», а выпуск и так у всех одинаков".to_owned());
    }
    match issue_choice(&host, false) {
        Some(elsewhere) => elsewhere.click(),
        None => wrong.push("«другое устройство» не нажать".to_owned()),
    }
    let device = Device::new();
    if !wait_until(|| device.issue_elsewhere(second.base())).await {
        wrong.push("выпуск открытого ИП не выключился".to_owned());
    }
    if device.issue_elsewhere(DB_NAME) {
        wrong.push("выпуск выключился и у другого ИП".to_owned());
    }
    if !wait_for(&host, t(lang, Key::SettingsIssueMixed)).await {
        wrong.push("не сказано, что выпуск не у всех одинаков".to_owned());
    }
    match button(&host, &[t(lang, Key::SettingsIssueAll)]) {
        Some(all) => all.click(),
        None => wrong.push(format!("нет «{}»", t(lang, Key::SettingsIssueAll))),
    }
    if !wait_until(|| device.issue_elsewhere(DB_NAME)).await || !device.issue_elsewhere(second.base()) {
        wrong.push("«так же у всех» не выключило выпуск у другого ИП".to_owned());
    }
    if !wait_for(&host, t(lang, Key::SettingsIssueEach)).await {
        wrong.push("выпуск у всех одинаков — а слова прежние".to_owned());
    }
    // Включить снова — только у открытого: у другого он остаётся выключенным.
    match issue_choice(&host, true) {
        Some(here) if !here.checked() => here.click(),
        Some(_) => wrong.push("выпуск выключен, а переключатель включён".to_owned()),
        None => wrong.push("переключатель выпуска не нажать".to_owned()),
    }
    if !wait_until(|| !device.issue_elsewhere(second.base())).await || !device.issue_elsewhere(DB_NAME) {
        wrong.push("включили у открытого — а у другого выпуск включился тоже".to_owned());
    }
    drop(mounted);
    host.remove();
    crate::ui::settings::set_issue_elsewhere(None, false);
    delete_database(second.base()).await;
    delete_database(OFFICE_DB).await;
    forget_office();
    assert!(wrong.is_empty(), "выпуск по ИП: {wrong:#?}");
}

/// Экран списка, открытый вместе со страницей, строится раньше, чем окно
/// выбрало базу: «открыт» — у ИП, которого помнит вкладка, а «Открыть» —
/// у другого (найдено проверкой собранного сайта, 2026-11-06).
#[wasm_bindgen_test]
async fn the_list_shown_with_the_page_names_the_chosen_ie() {
    delete_database(DB_NAME).await;
    delete_database(OFFICE_DB).await;
    forget_office();
    let aram = named("ИП Арам Петросян");
    let anna = named("ИП Анна Акопян");
    let today = clock::today().expect("сегодня");
    let mut first = Ip::first(Some(&aram), today, clock::now());
    first.copy_profile(&aram, clock::now());
    let mut second = Ip::new(today, clock::now());
    second.copy_profile(&anna, clock::now());
    seed(DB_NAME, &aram).await;
    seed(second.base(), &anna).await;
    seed_office(&[&first, &second]).await;
    Device::new().set_office();
    crate::store::office::set_tab_base(second.base());
    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let (host, mounted) = mount_at("#/office");
    let mut wrong = Vec::new();
    if wait_row_button(&host, "ИП Арам Петросян", t(lang, Key::OfficeOpen)).await.is_none() {
        wrong.push(format!("у первого ИП нет «Открыть»: {}", section_text(&host, "office")));
    }
    if row_button(&host, "ИП Анна Акопян", t(lang, Key::OfficeOpen)).is_some() {
        wrong.push("«Открыть» — у открытого ИП".to_owned());
    }
    drop(mounted);
    host.remove();
    crate::platform::set_hash("");
    delete_database(second.base()).await;
    delete_database(OFFICE_DB).await;
    forget_office();
    assert!(wrong.is_empty(), "список вместе со страницей: {wrong:#?}");
}

/// Профиль ИП `name` с кодом услуг — налог его квартала посчитан весь.
fn with_code(name: &str) -> Profile {
    let mut data = named(name).data().clone();
    data.activities = vec![ActivityCode::parse("74.10.0").expect("код")];
    Profile::save(None, data, clock::now()).expect("годный профиль").expect("новый")
}

/// Доход в 1 000 000 ֏ первым днём квартала `quarter`.
fn income_in(quarter: Quarter) -> Operation {
    Operation::create(
        OperationData {
            flow: Flow::Income,
            date: quarter.first_day(),
            paid: None,
            amount: Money::new(1_000_000, Currency::Amd),
            rate: None,
            text: Line::new("Дизайн сайта").expect("текст"),
            client: None,
            supplier: Line::default(),
            proof: None,
            nature: None,
            cost: None,
            activity: None,
            income_kind: None,
            attachments: Vec::new(),
        },
        clock::now(),
    )
    .expect("годная запись")
}

/// Квартал `quarter` ИП с профилем `profile` и доходом `income` — закрыт
/// сегодня, командой ядра, как его закрыл бы экран налога.
fn closed(profile: &Profile, income: &Operation, quarter: Quarter, today: Date) -> Closing {
    let books = Books {
        documents: &[],
        operations: std::slice::from_ref(income),
        profile: Some(profile.data()),
        closings: &[],
        openings: &[],
        unreadable: 0,
    };
    Closing::close(&turnover::quarter(books, quarter), today, clock::now()).expect("квартал закрывается")
}

/// Записать в базу `name` эти записи — в обход окна, как другая вкладка.
async fn put(name: &str, writes: Writes) {
    let store = Store::open_base(name, |_| {}).await.expect("база открыта");
    store.write().expect("транзакция").commit(writes).await.expect("записано");
}

/// Сводка конторы (задача 38, этап 5): у каждого ИП — прошлый квартал словами
/// карточки налога: у открытого — «к уплате», у другого, закрытого, —
/// «расчёт подан». Квартал, закрытый, пока экран спрятан, сводка показывает
/// при следующем показе экрана: базы она перечитывает. Открыта она здесь
/// тестам без ключа (`OPEN_FOR_TESTS`, задача 113), а с настоящим сроком —
/// без пробного периода и ключа «Pro» — сводки нет: вместо неё слова, когда
/// она открыта.
#[wasm_bindgen_test]
async fn the_summary_names_what_each_ie_owes() {
    delete_database(DB_NAME).await;
    delete_database(OFFICE_DB).await;
    forget_office();
    super::license::forget_trial();
    let today = clock::today().expect("сегодня");
    let previous = today.quarter().previous().expect("прошлый квартал есть");
    let aram = with_code("ИП Арам Петросян");
    let anna = with_code("ИП Анна Акопян");
    let mut first = Ip::first(Some(&aram), today, clock::now());
    first.copy_profile(&aram, clock::now());
    let mut second = Ip::new(today, clock::now());
    second.copy_profile(&anna, clock::now());
    let income = income_in(previous);
    let anna_closed = closed(&anna, &income, previous, today);
    let mut writes = Writes::default();
    writes.insert(&aram);
    writes.insert(&income);
    put(DB_NAME, writes).await;
    let mut writes = Writes::default();
    writes.insert(&anna);
    writes.insert(&income);
    writes.insert(&anna_closed);
    put(second.base(), writes).await;
    seed_office(&[&first, &second]).await;
    let device = Device::new();
    device.set_office();
    crate::store::office::set_tab_base(DB_NAME);
    let lang = Lang::Ru;
    device.set_ui_lang(lang);

    let (host, mounted) = mount_at("#/office");
    let mut wrong = Vec::new();
    let summary = || section_text(&host, "office");
    let to_pay = fill(t(lang, Key::TaxToPayBy), &[""]);
    let to_pay = to_pay.trim_end();
    let filed = t(lang, Key::DeadlineDone);
    let quarter = i18n::quarter(lang, previous);
    if !wait_until(|| {
        let text = summary();
        text.contains(to_pay) && text.contains(filed) && text.contains(&quarter)
    })
    .await
    {
        wrong.push(format!("нет «к уплате» у одного и «подан» у другого: {}", summary()));
    }

    // Квартал открытого ИП закрыли, пока экран спрятан: сводка — при
    // следующем показе, а не прежняя.
    crate::platform::set_hash("#/");
    sleep(100).await;
    let aram_closed = closed(&aram, &income, previous, today);
    let mut writes = Writes::default();
    writes.insert(&aram_closed);
    put(DB_NAME, writes).await;
    crate::platform::set_hash("#/office");
    if !wait_until(|| {
        let text = summary();
        !text.contains(to_pay) && text.matches(filed).count() == 2
    })
    .await
    {
        wrong.push(format!("закрытый квартал открытого ИП сводка не перечитала: {}", summary()));
    }
    drop(mounted);
    host.remove();

    // Настоящий срок: пробный период кончился, ключа «Pro» нет — сводки нет.
    let real = crate::ui::license::RealTerms::new();
    device.set_trial_start(license::LAST_TRIAL_START.add_days(-30).expect("дата"));
    let (host, mounted) = mount_at("#/office");
    let locked = t(lang, Key::OfficeSummaryLocked);
    if !wait_until(|| section_text(&host, "office").contains(locked)).await {
        wrong.push(format!("без пробного периода и «Pro» нет слов о сводке: {}", section_text(&host, "office")));
    }
    if section_text(&host, "office").contains(filed) {
        wrong.push("без пробного периода и «Pro» сводка видна".to_owned());
    }
    drop(mounted);
    host.remove();
    drop(real);
    crate::platform::set_hash("");
    delete_database(second.base()).await;
    delete_database(OFFICE_DB).await;
    delete_database(DB_NAME).await;
    forget_office();
    super::license::forget_trial();
    assert!(wrong.is_empty(), "сводка конторы: {wrong:#?}");
}
