//! Браузерные тесты первого запуска и первых шагов (задача 28): всё окно
//! в настоящем DOM и настоящей IndexedDB.

use kvartik_core::i18n::{Key, Lang, t};
use kvartik_core::model::client::Client;
use kvartik_core::model::document::Document;
use kvartik_core::model::profile::Profile;
use leptos::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::wasm_bindgen_test;
use web_sys::HtmlElement;

use super::documents::{acme, commit, forget_device, fresh_base_with_client, invoice_data, profile};
use super::{button, choice_card, document, field, mount_at, section_text, sleep, stored_profile, type_into, wait_for};
use crate::platform::pwa::Browser;
use crate::platform::{clock, hash};
use crate::store::Writes;
use crate::store::device::Device;
use crate::ui::lang::WindowLang;
use crate::ui::onboarding::Welcome;
use crate::ui::pwa::Program;
use crate::ui::route::Router;

/// Забыть, что первые шаги скрыты: тесты делят одно хранилище устройства.
fn forget_steps() {
    if let Some(storage) = web_sys::window().and_then(|window| window.local_storage().ok().flatten()) {
        let _ = storage.remove_item("kvartik.steps_hidden");
    }
}

/// Нажать кнопку с подписью `key` внутри экрана `screen`.
fn press(host: &HtmlElement, screen: &str, lang: Lang, key: Key, wrong: &mut Vec<String>) {
    let scope: Option<HtmlElement> = host
        .query_selector(&format!("#{screen}"))
        .ok()
        .flatten()
        .map(JsCast::unchecked_into);
    match scope.and_then(|scope| button(&scope, &[t(lang, key)])) {
        Some(found) => found.click(),
        None => wrong.push(format!("на экране {screen} нет кнопки «{}»", t(lang, key))),
    }
}

/// Какой шаг нынешний — подпись строки, отмеченной `aria-current`.
fn current_step(host: &HtmlElement) -> Option<String> {
    host.query_selector("#home .first-steps li[aria-current=\"step\"]")
        .ok()
        .flatten()
        .and_then(|row| row.text_content())
}

/// Подождать, пока нынешним станет шаг `key`.
async fn wait_step(host: &HtmlElement, lang: Lang, key: Key) -> bool {
    for _ in 0..100 {
        if current_step(host).is_some_and(|step| step.contains(t(lang, key))) {
            return true;
        }
        sleep(50).await;
    }
    false
}

/// С пустой базы — до счёта: шаг за шагом, и каждый шаг открывает готовую
/// форму и возвращает на главный экран. Короткая форма профиля не показывает
/// лишнего, но и не теряет его, а у кода называет ставку и срок. Передумавший
/// в форме клиента тоже возвращается к шагу.
#[wasm_bindgen_test]
async fn first_steps_lead_from_an_empty_base_to_the_invoice() {
    crate::store::delete_database(crate::store::DB_NAME).await;
    forget_steps();
    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let (host, mounted) = mount_at("#/");
    let mut wrong = Vec::new();
    if !wait_step(&host, lang, Key::StepRequisites).await {
        wrong.push(format!("нет первого шага: {:?}", current_step(&host)));
    }
    if section_text(&host, "home").contains(t(lang, Key::HomeProfileMissing)) {
        wrong.push("о профиле говорят и шаги, и отдельная карточка".to_owned());
    }

    press(&host, "home", lang, Key::StepsFillRequisites, &mut wrong);
    if !wait_for(&host, t(lang, Key::ProfileBriefAbout)).await {
        wrong.push("короткая форма не открылась".to_owned());
    }
    let form = section_text(&host, "profile");
    for hidden in [Key::ProfileRegistration, Key::ProfilePhone, Key::ProfilePerson, Key::ProfileLogo] {
        if form.contains(t(lang, hidden)) {
            wrong.push(format!("в короткой форме есть «{}»", t(lang, hidden)));
        }
    }
    match field(&host, t(lang, Key::ProfileNameRu), 0) {
        Some(input) => type_into(&input, "ИП Арам Петросян"),
        None => wrong.push("нет поля имени".to_owned()),
    }
    press(&host, "profile", lang, Key::ProfileSave, &mut wrong);
    if !wait_step(&host, lang, Key::StepActivity).await {
        wrong.push(format!("после имени — не вид деятельности: {:?}", current_step(&host)));
    }
    if hash() != "#/" {
        wrong.push(format!("после формы не на главном: {}", hash()));
    }

    press(&host, "home", lang, Key::StepsFillActivity, &mut wrong);
    if !wait_for(&host, t(lang, Key::ProfileBriefAbout)).await {
        wrong.push("короткая форма не открылась снова".to_owned());
    }
    match field(&host, t(lang, Key::ActivityCode), 0) {
        Some(input) => type_into(&input, "62.01.0"),
        None => wrong.push("в короткой форме нет поля кода".to_owned()),
    }
    let said = t(lang, Key::ActivityRateDue).split("{}").next().unwrap_or_default();
    if !wait_for(&host, said).await {
        wrong.push("код не назвал ставку и срок".to_owned());
    }
    press(&host, "profile", lang, Key::ProfileSave, &mut wrong);
    if !wait_step(&host, lang, Key::StepClient).await {
        wrong.push(format!("после кода — не клиент: {:?}", current_step(&host)));
    }
    match stored_profile().await {
        Ok(Some(saved)) => {
            let data = saved.data();
            if data.name.ru.as_str() != "ИП Арам Петросян" || data.activities.len() != 1 || !data.accounts.is_empty() {
                wrong.push(format!("в базе не то: {data:?}"));
            }
        }
        other => wrong.push(format!("в базе профиля нет: {other:?}")),
    }

    press(&host, "home", lang, Key::ClientAdd, &mut wrong);
    if !wait_for(&host, t(lang, Key::ClientNew)).await {
        wrong.push("форма клиента не открылась".to_owned());
    }
    match host.query_selector("form.client .wizard-head button.round").ok().flatten() {
        Some(back) => back.unchecked_into::<HtmlElement>().click(),
        None => wrong.push("у формы клиента нет «назад»".to_owned()),
    }
    sleep(100).await;
    if hash() != "#/" {
        wrong.push(format!("передумавший в форме клиента не на главном: {}", hash()));
    }
    press(&host, "home", lang, Key::ClientAdd, &mut wrong);
    if !wait_for(&host, t(lang, Key::ClientNew)).await {
        wrong.push("форма клиента не открылась снова".to_owned());
    }
    match field(&host, t(lang, Key::ClientName), 0) {
        Some(input) => type_into(&input, "ООО Ромашка"),
        None => wrong.push("нет поля названия клиента".to_owned()),
    }
    match choice_card(&host, 0) {
        Some(armenia) => armenia.click(),
        None => wrong.push("нет карточки «В Армении»".to_owned()),
    }
    sleep(50).await;
    press(&host, "clients", lang, Key::ClientNextDefaults, &mut wrong);
    sleep(50).await;
    press(&host, "clients", lang, Key::ProfileSave, &mut wrong);
    if !wait_step(&host, lang, Key::StepInvoice).await {
        wrong.push(format!("после клиента — не счёт: {:?}", current_step(&host)));
    }
    sleep(100).await;
    if hash() != "#/" {
        wrong.push(format!("после клиента не на главном: {}", hash()));
    }

    press(&host, "home", lang, Key::DocNewInvoice, &mut wrong);
    if !wait_for(&host, t(lang, Key::DocWhoInvoice)).await {
        wrong.push("форма счёта не открылась".to_owned());
    }
    if hash() != "#/documents" {
        wrong.push(format!("форма счёта не на своём экране: {}", hash()));
    }

    drop(mounted);
    host.remove();
    crate::platform::set_hash("");
    forget_device();
    assert!(wrong.is_empty(), "первые шаги: {wrong:#?}");
}

/// Счёт выпущен — последний шаг ведёт к нему, а «Готово» убирает карточку
/// насовсем: и после нового открытия её нет.
#[wasm_bindgen_test]
async fn the_last_step_opens_the_invoice_and_finishes() {
    let client = fresh_base_with_client().await;
    forget_steps();
    let now = clock::now();
    let issued = Document::create(invoice_data(&client), now)
        .expect("черновик")
        .issue(&[], 0, Some(&profile()), &client, now)
        .expect("выпущен");
    let mut writes = Writes::default();
    writes.insert(&issued);
    commit(writes).await;

    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let (host, mounted) = mount_at("#/");
    let mut wrong = Vec::new();
    if !wait_step(&host, lang, Key::StepPdf).await {
        wrong.push(format!("нет шага PDF: {:?}", current_step(&host)));
    }
    let done = host.query_selector_all("#home .first-steps li.done").map_or(0, |rows| rows.length());
    if done != 4 {
        wrong.push(format!("пройдено шагов: {done}, а не 4"));
    }
    press(&host, "home", lang, Key::StepsOpenInvoice, &mut wrong);
    sleep(100).await;
    if hash() != format!("#/documents/{}", issued.id()) {
        wrong.push(format!("шаг ведёт не к счёту: {}", hash()));
    }
    crate::platform::set_hash("#/");
    sleep(100).await;
    press(&host, "home", lang, Key::StepsFinish, &mut wrong);
    sleep(100).await;
    if host.query_selector("#home .first-steps").ok().flatten().is_some() {
        wrong.push("«Готово» не убрало карточку".to_owned());
    }
    drop(mounted);
    host.remove();

    let (host, mounted) = mount_at("#/");
    if !wait_for(&host, t(lang, Key::DocNewInvoice)).await {
        wrong.push("окно не открылось снова".to_owned());
    }
    sleep(300).await;
    if host.query_selector("#home .first-steps").ok().flatten().is_some() {
        wrong.push("скрытые шаги вернулись после нового открытия".to_owned());
    }
    drop(mounted);
    host.remove();
    crate::platform::set_hash("");
    forget_steps();
    forget_device();
    assert!(wrong.is_empty(), "последний шаг: {wrong:#?}");
}

/// Два выпущенных документа — уже не новичок (и копия с документами тоже):
/// карточки нет, хотя её никто не скрывал.
#[wasm_bindgen_test]
async fn a_settled_entrepreneur_sees_no_first_steps() {
    let client = fresh_base_with_client().await;
    forget_steps();
    let now = clock::now();
    let first = Document::create(invoice_data(&client), now)
        .expect("черновик")
        .issue(&[], 0, Some(&profile()), &client, now)
        .expect("выпущен");
    let second = Document::create(invoice_data(&client), now)
        .expect("черновик")
        .issue(std::slice::from_ref(&first), 0, Some(&profile()), &client, now)
        .expect("выпущен");
    let mut writes = Writes::default();
    writes.insert(&first);
    writes.insert(&second);
    commit(writes).await;

    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let (host, mounted) = mount_at("#/");
    let mut wrong = Vec::new();
    // Весь путь чтения базы: карточка не мелькает и до чтения документов.
    for _ in 0..100 {
        if host.query_selector("#home .first-steps").ok().flatten().is_some() {
            wrong.push("первые шаги у того, у кого есть документы".to_owned());
            break;
        }
        sleep(10).await;
    }
    if !wait_for(&host, t(lang, Key::DocNewInvoice)).await {
        wrong.push("окно не открылось".to_owned());
    }
    drop(mounted);
    host.remove();
    crate::platform::set_hash("");
    forget_device();
    assert!(wrong.is_empty(), "опытный человек: {wrong:#?}");
}

/// Пока стоит экран первого запуска, карточки нет: её часть не грузится
/// и не делит канал со шрифтами (шапка `ui/onboarding.rs`). «Продолжить» — и она
/// на месте.
#[wasm_bindgen_test]
async fn the_card_waits_for_the_first_screen() {
    crate::store::delete_database(crate::store::DB_NAME).await;
    forget_steps();
    forget_device();
    let lang = Lang::Ru;
    let (host, mounted) = mount_at("#/");
    let mut wrong = Vec::new();
    if host.query_selector(".welcome").ok().flatten().is_none() {
        wrong.push("нет экрана первого запуска".to_owned());
    }
    sleep(500).await;
    if host.query_selector("#home .first-steps").ok().flatten().is_some() {
        wrong.push("карточка раньше экрана первого запуска".to_owned());
    }
    let shown = WindowLang::remembered().unwrap_or(lang);
    match button(&host, &[t(shown, Key::WelcomeContinue), t(Lang::Am, Key::WelcomeContinue), t(Lang::En, Key::WelcomeContinue)]) {
        Some(found) => found.click(),
        None => wrong.push("нет кнопки «Продолжить»".to_owned()),
    }
    if !wait_step(&host, Device::new().ui_lang().unwrap_or(lang), Key::StepRequisites).await {
        wrong.push(format!("после экрана нет карточки: {:?}", current_step(&host)));
    }
    drop(mounted);
    host.remove();
    crate::platform::set_hash("");
    forget_device();
    assert!(wrong.is_empty(), "карточка и первый экран: {wrong:#?}");
}

/// Экран первого запуска отдельно от окна, в браузере `browser`.
fn mount_welcome(browser: Browser) -> (HtmlElement, impl Sized) {
    let page = document();
    let host: HtmlElement = page.create_element("div").expect("создаётся").unchecked_into();
    page.body().expect("есть <body>").append_child(&host).expect("вставляется");
    let mounted = leptos::mount::mount_to(host.clone(), move || {
        let window_lang = WindowLang::start();
        let router = Router::start();
        let open = RwSignal::new(true);
        let program = Program::new().in_browser(browser);
        move || open.get().then(|| view! { <Welcome window_lang open router program /> })
    });
    (host, mounted)
}

/// На iPhone, iPad и Mac установленная программа не видит данных браузера:
/// после языка — установка, и «У меня есть копия» тоже ведёт через неё. В Chrome
/// видит: шага нет, экран закрывается сразу.
#[wasm_bindgen_test]
async fn install_comes_before_data_where_the_app_does_not_see_them() {
    let lang = Lang::Ru;
    let mut wrong = Vec::new();
    for browser in [Browser::SafariTouch, Browser::OtherTouch, Browser::SafariMac] {
        for first in [Key::WelcomeContinue, Key::WelcomeHaveBackup] {
            forget_device();
            Device::new().set_ui_lang(lang);
            let (host, mounted) = mount_welcome(browser);
            let case = format!("{browser:?}, «{}»", t(lang, first));
            match button(&host, &[t(lang, first)]) {
                Some(found) => found.click(),
                None => wrong.push(format!("{case}: нет кнопки")),
            }
            sleep(50).await;
            if !host.text_content().unwrap_or_default().contains(t(lang, Key::InstallTitle)) {
                wrong.push(format!("{case}: нет шага установки"));
            }
            if host.query_selector(".welcome-first:not([hidden])").ok().flatten().is_some() {
                wrong.push(format!("{case}: выбор языка остался на шаге установки"));
            }
            match button(&host, &[t(lang, Key::WelcomeInstallLater)]) {
                Some(later) => later.click(),
                None => wrong.push(format!("{case}: нельзя продолжить без установки")),
            }
            sleep(50).await;
            if host.query_selector(".welcome").ok().flatten().is_some() {
                wrong.push(format!("{case}: экран не закрылся"));
            }
            drop(mounted);
            host.remove();
        }
    }
    forget_device();
    Device::new().set_ui_lang(lang);
    let (host, mounted) = mount_welcome(Browser::Chrome);
    match button(&host, &[t(lang, Key::WelcomeContinue)]) {
        Some(found) => found.click(),
        None => wrong.push("Chrome: нет кнопки".to_owned()),
    }
    sleep(50).await;
    if host.query_selector(".welcome").ok().flatten().is_some() {
        wrong.push("в Chrome экран не закрылся сразу".to_owned());
    }
    drop(mounted);
    host.remove();
    crate::platform::set_hash("");
    forget_device();
    assert!(wrong.is_empty(), "установка до данных: {wrong:#?}");
}

/// Живой клиент без выпущенного документа — шаг счёта, а не шаг клиента:
/// шаги считаются по данным, а не по пройденному пути.
#[wasm_bindgen_test]
async fn a_returning_person_continues_from_the_data() {
    crate::store::delete_database(crate::store::DB_NAME).await;
    forget_steps();
    let now = clock::now();
    let mut data = profile();
    data.activities = vec![kvartik_core::model::activity::ActivityCode::parse("62.01.0").expect("код")];
    let saved = Profile::save(None, data, now).expect("годный профиль").expect("новый");
    let client = Client::create(acme(), now).expect("годный клиент");
    let mut writes = Writes::default();
    writes.insert(&saved);
    writes.insert(&client);
    commit(writes).await;

    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let (host, mounted) = mount_at("#/");
    let mut wrong = Vec::new();
    if !wait_step(&host, lang, Key::StepInvoice).await {
        wrong.push(format!("нынешний шаг: {:?}", current_step(&host)));
    }
    drop(mounted);
    host.remove();
    crate::platform::set_hash("");
    forget_device();
    assert!(wrong.is_empty(), "возвращение: {wrong:#?}");
}
