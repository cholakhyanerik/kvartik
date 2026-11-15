//! Браузерные тесты первого запуска и первых шагов (задача 28): всё окно
//! в настоящем DOM и настоящей IndexedDB.

use kvartik_core::i18n::{Key, Lang, fill, t};
use kvartik_core::model::activity::{ActivityCode, IncomeKind};
use kvartik_core::model::client::Client;
use kvartik_core::model::document::Document;
use kvartik_core::model::profile::{Profile, Residency};
use kvartik_core::tax::example::{HANDOFF_KEY, Handoff};
use kvartik_core::tax::turnover;
use leptos::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::wasm_bindgen_test;
use web_sys::HtmlElement;

use super::documents::{acme, commit, forget_device, fresh_base_with_client, invoice_data, profile, wait_until};
use super::{
    button, choice_card, document, field, mount_at, section_text, select, sleep, stored_profile, type_into, wait_for,
};
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

/// `localStorage` страницы тестов.
fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window().and_then(|window| window.local_storage().ok().flatten())
}

/// Оставить запись калькулятора лендинга — так, как её пишет wasm
/// калькулятора (`crates/calc`): тем же ядром и под тем же именем.
fn hand_off(handed: Handoff) {
    if let Some(storage) = local_storage() {
        let _ = storage.set_item(HANDOFF_KEY, &handed.to_json());
    }
}

/// Осталась ли запись калькулятора.
fn handoff_left() -> bool {
    local_storage().and_then(|storage| storage.get_item(HANDOFF_KEY).ok().flatten()).is_some()
}

fn forget_handoff() {
    if let Some(storage) = local_storage() {
        let _ = storage.remove_item(HANDOFF_KEY);
    }
}

/// Текст предложения калькулятора на главном экране; нет его — `None`.
fn handoff_card(host: &HtmlElement) -> Option<String> {
    host.query_selector("#home .handoff").ok().flatten().and_then(|card| card.text_content())
}

/// Строка предложения «что: значение» — по шаблону строки калькулятора.
fn handoff_row(lang: Lang, label: Key, value: &str) -> String {
    fill(t(lang, Key::CalcRow), &[t(lang, label), value])
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
        .issue(&[], 0, Some(&profile()), Some(None), &client, now)
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
        .issue(&[], 0, Some(&profile()), Some(None), &client, now)
        .expect("выпущен");
    let second = Document::create(invoice_data(&client), now)
        .expect("черновик")
        .issue(std::slice::from_ref(&first), 0, Some(&profile()), Some(None), &client, now)
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

/// Введённое в калькуляторе лендинга (задача 34): окно предлагает его
/// карточкой, «Взять в профиль» открывает полную форму с подставленным —
/// и пока она открыта, карточки нет, — а сохраняет её человек. Строка налога
/// встаёт у строки кода и просит сам код. Сохранённое лежит в базе, а форма
/// вернула на главный экран. Год рождения человек в форме стёр — и его
/// больше не предлагают: запись калькулятора забыта с сохранением формы.
#[wasm_bindgen_test]
async fn the_calculator_handoff_is_taken_through_the_form() {
    crate::store::delete_database(crate::store::DB_NAME).await;
    forget_steps();
    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    hand_off(Handoff {
        kind: Some(IncomeKind::Trade),
        birth_year: Some(1961),
        residency: Some(Residency::Citizen),
        pension_joined: Some(false),
        resident: Some(true),
    });
    let (host, mounted) = mount_at("#/");
    let mut wrong = Vec::new();
    if !wait_for(&host, t(lang, Key::HandoffTitle)).await {
        wrong.push("нет предложения калькулятора".to_owned());
    }
    let card = handoff_card(&host).unwrap_or_default();
    for line in [
        handoff_row(lang, Key::ActivityKind, t(lang, turnover::kind_key(IncomeKind::Trade))),
        handoff_row(lang, Key::BirthYear, "1961"),
        handoff_row(lang, Key::ProfileResidency, t(lang, Key::ResidencyCitizen)),
        t(lang, Key::HandoffPensionNo).to_owned(),
        t(lang, Key::HandoffResidentYes).to_owned(),
    ] {
        if !card.contains(&line) {
            wrong.push(format!("в предложении нет «{line}»: {card}"));
        }
    }

    press(&host, "home", lang, Key::HandoffTake, &mut wrong);
    if !wait_for(&host, t(lang, Key::HandoffApplied)).await {
        wrong.push("форма не сказала, что подставлено".to_owned());
    }
    if !section_text(&host, "profile").contains(t(lang, Key::HandoffAppliedCode)) {
        wrong.push("форма не попросила код к строке налога".to_owned());
    }
    if handoff_card(&host).is_some() {
        wrong.push("предложение видно при открытой форме".to_owned());
    }
    match field(&host, t(lang, Key::BirthYear), 0) {
        Some(input) if input.value() == "1961" => type_into(&input, ""),
        Some(input) => wrong.push(format!("год рождения в форме: «{}»", input.value())),
        None => wrong.push("в форме нет года рождения".to_owned()),
    }
    match select(&host, t(lang, Key::ActivityKind), 0) {
        Some(kind) if kind.value() == "trade" => {}
        Some(kind) => wrong.push(format!("строка налога у кода: «{}»", kind.value())),
        None => wrong.push("в форме нет строки налога у кода".to_owned()),
    }
    match field(&host, t(lang, Key::ActivityCode), 0) {
        Some(input) => type_into(&input, "47.11.0"),
        None => wrong.push("в форме нет поля кода".to_owned()),
    }
    press(&host, "profile", lang, Key::ProfileSave, &mut wrong);
    for _ in 0..100 {
        if !handoff_left() && hash() == "#/" {
            break;
        }
        sleep(50).await;
    }
    if handoff_left() {
        wrong.push("запись калькулятора осталась после сохранения".to_owned());
    }
    if hash() != "#/" {
        wrong.push(format!("после формы не на главном: {}", hash()));
    }
    let code = ActivityCode::parse("47.11.0").expect("годный код");
    match stored_profile().await {
        Ok(Some(saved)) => {
            let person = &saved.data().person;
            let got = (
                person.birth_year,
                person.residency,
                person.pension_joined,
                person.resident,
                saved.data().kinds.get(&code).copied(),
            );
            let want = (None, Some(Residency::Citizen), Some(false), Some(true), Some(IncomeKind::Trade));
            if got != want {
                wrong.push(format!("в базе {got:?}, а ждали {want:?}"));
            }
        }
        other => wrong.push(format!("профиля в базе нет: {other:?}")),
    }
    sleep(200).await;
    if handoff_card(&host).is_some() {
        wrong.push("стёртый в форме год рождения предложен снова".to_owned());
    }
    drop(mounted);
    host.remove();
    crate::platform::set_hash("");
    forget_handoff();
    forget_steps();
    forget_device();
    assert!(wrong.is_empty(), "калькулятор — в профиль: {wrong:#?}");
}

/// Окно предлагает только то, чего профиль не знает: заполненный год рождения
/// и строку при двух кодах — нет. «Не надо» забывает запись и ничего не
/// пишет; запись, из которой профилю нечего взять, забывается без карточки.
#[wasm_bindgen_test]
async fn a_declined_handoff_writes_nothing() {
    crate::store::delete_database(crate::store::DB_NAME).await;
    forget_steps();
    let mut data = profile();
    data.person.birth_year = Some(1980);
    data.activities = vec![
        ActivityCode::parse("47.11.0").expect("годный код"),
        ActivityCode::parse("62.01.0").expect("годный код"),
    ];
    let saved = Profile::save(None, data, clock::now()).expect("годный профиль").expect("новый");
    let mut writes = Writes::default();
    writes.insert(&saved);
    commit(writes).await;
    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    hand_off(Handoff {
        kind: Some(IncomeKind::Trade),
        birth_year: Some(1961),
        residency: Some(Residency::Citizen),
        ..Handoff::default()
    });
    let (host, mounted) = mount_at("#/");
    let mut wrong = Vec::new();
    if !wait_for(&host, t(lang, Key::HandoffTitle)).await {
        wrong.push("нет предложения калькулятора".to_owned());
    }
    let card = handoff_card(&host).unwrap_or_default();
    if !card.contains(&handoff_row(lang, Key::ProfileResidency, t(lang, Key::ResidencyCitizen))) {
        wrong.push(format!("в предложении нет гражданства: {card}"));
    }
    for absent in [
        handoff_row(lang, Key::BirthYear, "1961"),
        handoff_row(lang, Key::ActivityKind, t(lang, turnover::kind_key(IncomeKind::Trade))),
    ] {
        if card.contains(&absent) {
            wrong.push(format!("предложено то, что профиль уже решил: «{absent}»"));
        }
    }
    press(&host, "home", lang, Key::HandoffDismiss, &mut wrong);
    sleep(100).await;
    if handoff_card(&host).is_some() || handoff_left() {
        wrong.push("«Не надо» не забыло предложение".to_owned());
    }
    match stored_profile().await {
        Ok(Some(stored)) if stored == saved => {}
        other => wrong.push(format!("«Не надо» тронуло профиль: {other:?}")),
    }
    drop(mounted);
    host.remove();

    hand_off(Handoff {
        birth_year: Some(1961),
        ..Handoff::default()
    });
    let (host, mounted) = mount_at("#/");
    if !wait_for(&host, t(lang, Key::DocNewInvoice)).await {
        wrong.push("окно не открылось снова".to_owned());
    }
    for _ in 0..40 {
        if !handoff_left() {
            break;
        }
        sleep(25).await;
    }
    if handoff_card(&host).is_some() || handoff_left() {
        wrong.push("запись, из которой нечего взять, не забыта".to_owned());
    }
    drop(mounted);
    host.remove();
    crate::platform::set_hash("");
    forget_handoff();
    forget_device();
    assert!(wrong.is_empty(), "отказ от предложения: {wrong:#?}");
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

/// Язык страницы лендинга приходит строкой запроса (`/app/?lang=ru`, полный
/// прогон 2026-11-10, п. 11): экран первого запуска — на нём, хоть языки
/// браузера и другие, а запоминает его «Продолжить», не само открытие.
/// Незнакомый язык в адресе — как будто его нет.
#[wasm_bindgen_test]
async fn the_landing_language_is_offered_and_kept_on_continue() {
    let window = web_sys::window().expect("окно есть");
    let history = window.history().expect("history есть");
    let location = window.location();
    let original = format!(
        "{}{}{}",
        location.pathname().unwrap_or_default(),
        location.search().unwrap_or_default(),
        location.hash().unwrap_or_default()
    );
    let address = |query: &str| {
        let _ = history.replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(query));
    };
    let mut wrong = Vec::new();
    let browser = Lang::preferred(crate::platform::languages().iter().map(String::as_str));
    let site = if browser == Lang::Ru { Lang::En } else { Lang::Ru };

    forget_device();
    address(&format!("?lang={}", site.code()));
    let (host, mounted) = mount_welcome(Browser::Chrome);
    sleep(100).await;
    if !host.text_content().unwrap_or_default().contains(t(site, Key::WelcomeContinue)) {
        wrong.push(format!("экран первого запуска не на языке сайта {site:?}"));
    }
    if Device::new().ui_lang().is_some() {
        wrong.push("язык сайта запомнен без «Продолжить»".to_owned());
    }
    match button(&host, &[t(site, Key::WelcomeContinue)]) {
        Some(proceed) => proceed.click(),
        None => wrong.push("нет «Продолжить» на языке сайта".to_owned()),
    }
    sleep(100).await;
    if Device::new().ui_lang() != Some(site) {
        wrong.push(format!("«Продолжить» запомнило {:?}, а не {site:?}", Device::new().ui_lang()));
    }
    drop(mounted);
    host.remove();

    forget_device();
    address("?lang=xx");
    let (host, mounted) = mount_welcome(Browser::Chrome);
    sleep(100).await;
    if !host.text_content().unwrap_or_default().contains(t(browser, Key::WelcomeContinue)) {
        wrong.push(format!("с незнакомым языком в адресе — не язык браузера {browser:?}"));
    }
    drop(mounted);
    host.remove();
    address(&original);
    forget_device();
    assert!(wrong.is_empty(), "язык сайта: {wrong:#?}");
}

/// Язык, выбранный переключателем на страницах сайта (запись лендинга
/// `kvartik.site_lang`, 0.0.125), сильнее выбранного в окне раньше: окно
/// открывается на нём, запоминает его и снимает запись — выбрал на лендинге
/// English, а программа открывалась по-русски. Язык страницы в адресе без
/// выбора на сайте выбранного в окне не меняет: корень сайта армянский
/// у всех. При первом запуске выбранный на сайте только предлагается —
/// сильнее языка адреса, — а снимает его выбор в окне.
#[wasm_bindgen_test]
async fn a_language_chosen_on_the_site_wins_over_the_remembered_one() {
    let window = web_sys::window().expect("окно есть");
    let history = window.history().expect("history есть");
    let location = window.location();
    let original = format!(
        "{}{}{}",
        location.pathname().unwrap_or_default(),
        location.search().unwrap_or_default(),
        location.hash().unwrap_or_default()
    );
    let address = |query: &str| {
        let _ = history.replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(query));
    };
    let html = document().document_element().expect("есть <html>");
    let page_lang = html.get_attribute("lang");
    let storage = local_storage().expect("localStorage есть");
    let site_record = || storage.get_item("kvartik.site_lang").ok().flatten();
    let mut wrong = Vec::new();

    // Выбрал в окне русский, потом на сайте — English, а пришёл со страницы
    // армянской: окно — на English и помнит его.
    forget_device();
    Device::new().set_ui_lang(Lang::Ru);
    let _ = storage.set_item("kvartik.site_lang", Lang::En.code());
    address(&format!("?lang={}", Lang::Am.code()));
    let (host, mounted) = mount_welcome(Browser::Chrome);
    if html.get_attribute("lang").as_deref() != Some(Lang::En.code()) {
        wrong.push(format!("выбранный на сайте не взят: окно на {:?}", html.get_attribute("lang")));
    }
    if Device::new().ui_lang() != Some(Lang::En) || site_record().is_some() {
        wrong.push(format!(
            "выбранный на сайте не запомнен окном: {:?}, запись сайта — {:?}",
            Device::new().ui_lang(),
            site_record()
        ));
    }
    drop(mounted);
    host.remove();

    // Выбора на сайте нет — язык страницы в адресе выбранного в окне не меняет.
    address(&format!("?lang={}", Lang::Am.code()));
    let (host, mounted) = mount_welcome(Browser::Chrome);
    if html.get_attribute("lang").as_deref() != Some(Lang::En.code()) || Device::new().ui_lang() != Some(Lang::En) {
        wrong.push(format!(
            "язык страницы без выбора на сайте сменил выбранный: окно на {:?}, помнит {:?}",
            html.get_attribute("lang"),
            Device::new().ui_lang()
        ));
    }
    drop(mounted);
    host.remove();

    // Первый запуск: выбранный на сайте предлагается — сильнее языка адреса, —
    // не запоминается сам, а выбор в окне его снимает.
    forget_device();
    let _ = storage.set_item("kvartik.site_lang", Lang::En.code());
    address(&format!("?lang={}", Lang::Ru.code()));
    let (host, mounted) = mount_welcome(Browser::Chrome);
    if !host.text_content().unwrap_or_default().contains(t(Lang::En, Key::WelcomeContinue)) {
        wrong.push("первый запуск предложил не язык, выбранный на сайте".to_owned());
    }
    if Device::new().ui_lang().is_some() || site_record().is_none() {
        wrong.push(format!(
            "первый запуск без выбора: окно помнит {:?}, запись сайта — {:?}",
            Device::new().ui_lang(),
            site_record()
        ));
    }
    match host.query_selector(".welcome label[lang=\"hy\"]").ok().flatten() {
        Some(armenian) => armenian.unchecked_into::<HtmlElement>().click(),
        None => wrong.push("нет армянского среди языков".to_owned()),
    }
    if !wait_until(|| Device::new().ui_lang() == Some(Lang::Am)).await || site_record().is_some() {
        wrong.push(format!(
            "выбор в окне: помнит {:?}, запись сайта — {:?}",
            Device::new().ui_lang(),
            site_record()
        ));
    }
    drop(mounted);
    host.remove();

    address(&original);
    forget_device();
    match page_lang {
        Some(code) => html.set_attribute("lang", &code).expect("ставится"),
        None => html.remove_attribute("lang").expect("снимается"),
    }
    assert!(wrong.is_empty(), "язык, выбранный на сайте: {wrong:#?}");
}
