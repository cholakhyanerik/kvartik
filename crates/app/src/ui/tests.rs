//! Браузерные тесты окна: всё окно (`App`) в настоящем DOM headless-браузера
//! (`wasm-bindgen-test`). Как запускать — Правило 2 в CLAUDE.md.
//!
//! Окно открывает базу программы (`store::DB_NAME`), а не пробную: в браузере
//! тестов она своя и после прогона пропадает вместе с профилем браузера.
//! Поэтому тесты не рассчитывают на пустую базу — профиль, сохранённый одним
//! тестом, другой может застать.

use js_sys::Promise;
use kvartik_core::format::Stored;
use kvartik_core::i18n::{ALL_KEYS, Key, Lang, fill, t};
use kvartik_core::model::client::{Client, ClientData};
use kvartik_core::model::country::Country;
use kvartik_core::model::money::Currency;
use kvartik_core::model::profile::{Person, Profile, ProfileData, Residency};
use kvartik_core::model::record::Timestamp;
use kvartik_core::model::text::{Line, Lines};
use kvartik_core::model::tin::Tin;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};
use web_sys::{Document, Element, HtmlElement, HtmlInputElement, HtmlSelectElement};

use super::App;
use super::route::Screen;
use crate::store::device::Device;
use crate::store::{Store, StoreError, Writes};

mod annual;
mod documents;
mod install;
mod ledger;
pub(super) mod license;
mod motion;
mod office;
mod onboarding;
mod report;
mod settings;
mod share;
mod tax;
mod update;

wasm_bindgen_test_configure!(run_in_browser);

fn document() -> Document {
    web_sys::window()
        .and_then(|window| window.document())
        .expect("есть документ")
}

/// Подождать `ms` миллисекунд: перерисовка Leptos идёт не сразу после
/// изменения сигнала, а следующей задачей.
async fn sleep(ms: i32) {
    let promise = Promise::new(&mut |resolve, _| {
        web_sys::window()
            .expect("окно есть")
            .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms)
            .expect("таймер поставлен");
    });
    promise.await.expect("таймер срабатывает");
}

/// Окно в своём `<div>`: снять его — бросить ручку (`UnmountHandle`)
/// и убрать `<div>`. Адрес — с начала: экран, открытый прошлым тестом, не
/// переходит в следующий.
fn mount() -> (HtmlElement, impl Sized) {
    mount_at("")
}

/// Окно, открытое по адресу `hash`, — как после перезагрузки вкладки: у тестов
/// вкладка одна на весь прогон, и без этого первое окно в прогоне — какое
/// придётся — открылось бы как запуск, а с кончившимся сроком ушло бы на
/// экран лицензии (`ui/license.rs`, `open_on_license`). Запуск — [`launch_at`].
fn mount_at(hash: &str) -> (HtmlElement, impl Sized) {
    crate::store::device::pretend_launch(false);
    mount_in(hash)
}

/// Окно, открытое по адресу `hash` запуском программы: вкладка открывает её
/// впервые.
fn launch_at(hash: &str) -> (HtmlElement, impl Sized) {
    crate::store::device::pretend_launch(true);
    mount_in(hash)
}

/// Окно по адресу `hash`; как вкладка открывалась раньше — решает тот, кто
/// зовёт.
fn mount_in(hash: &str) -> (HtmlElement, impl Sized) {
    crate::platform::set_hash(hash);
    let document = document();
    let host: HtmlElement = document
        .create_element("div")
        .expect("создаётся")
        .unchecked_into();
    document
        .body()
        .expect("есть <body>")
        .append_child(&host)
        .expect("вставляется");
    let mounted = leptos::mount::mount_to(host.clone(), App);
    (host, mounted)
}

/// Показать экраны по очереди и вернуться на главный. Счета, книга, клиенты
/// и профиль рисуются с первого показа (`split::screen`, задача 61),
/// а показанный однажды остаётся в странице спрятанным — с полями
/// и кнопками, которые тест потом нажимает. В тестах часть не грузится,
/// а зовётся (`ui/split.rs`): экран виден — его содержимое уже нарисовано.
async fn visit(host: &HtmlElement, screens: &[Screen]) {
    for screen in screens.iter().chain([&Screen::Home]) {
        crate::platform::set_hash(&screen.href());
        for _ in 0..100 {
            if shown_screens(host) == [screen.id()] {
                break;
            }
            sleep(10).await;
        }
    }
    sleep(50).await;
}

/// Подождать, пока в окне появится текст; `false` — не дождались за пять
/// секунд.
async fn wait_for(host: &HtmlElement, text: &str) -> bool {
    for _ in 0..100 {
        if host.text_content().is_some_and(|content| content.contains(text)) {
            return true;
        }
        sleep(50).await;
    }
    false
}

/// Кнопка с одной из этих подписей.
fn button(host: &HtmlElement, labels: &[&str]) -> Option<HtmlElement> {
    let buttons = host.query_selector_all("button").ok()?;
    (0..buttons.length())
        .filter_map(|at| buttons.item(at))
        .map(|node| node.unchecked_into::<HtmlElement>())
        .find(|button| {
            let text = button.text_content().unwrap_or_default();
            labels.contains(&text.trim())
        })
}

/// Поле ввода внутри подписи `label` — `nth`-е такое, считая с нуля.
/// Подпись сравнивается целиком, по первому узлу текста: «Банк» — начало
/// и «Банк-корреспондент», а у подписи списка выбора в тексте ещё и все
/// варианты.
fn field(host: &HtmlElement, label: &str, nth: usize) -> Option<HtmlInputElement> {
    let labels = host.query_selector_all("label").ok()?;
    (0..labels.length())
        .filter_map(|at| labels.item(at))
        .map(|node| node.unchecked_into::<Element>())
        .filter(|element| {
            element
                .first_child()
                .and_then(|text| text.text_content())
                .is_some_and(|text| text.trim() == label)
        })
        .nth(nth)?
        .query_selector("input")
        .ok()
        .flatten()
        .map(JsCast::unchecked_into)
}

/// Ввести текст, как человек: значение и событие `input`, которое слушает
/// привязка Leptos.
fn type_into(input: &HtmlInputElement, text: &str) {
    input.set_value(text);
    let event = web_sys::Event::new("input").expect("событие создаётся");
    input.dispatch_event(&event).expect("событие отправлено");
}

/// Список выбора внутри подписи `label` — `nth`-й такой, считая с нуля.
fn select(host: &HtmlElement, label: &str, nth: usize) -> Option<HtmlSelectElement> {
    let labels = host.query_selector_all("label").ok()?;
    (0..labels.length())
        .filter_map(|at| labels.item(at))
        .map(|node| node.unchecked_into::<Element>())
        .filter(|element| {
            element
                .first_child()
                .and_then(|text| text.text_content())
                .is_some_and(|text| text.trim() == label)
        })
        .nth(nth)?
        .query_selector("select")
        .ok()
        .flatten()
        .map(JsCast::unchecked_into)
}

/// Выбрать вариант, как человек: значение и событие `change`.
fn pick(select: &HtmlSelectElement, value: &str) {
    select.set_value(value);
    let event = web_sys::Event::new("change").expect("событие создаётся");
    select.dispatch_event(&event).expect("событие отправлено");
}

/// Карточка выбора формы по шагам — `nth`-я, считая с нуля: «В Армении»,
/// «В другой стране». Нажатие на неё выбирает её переключатель.
fn choice_card(host: &HtmlElement, nth: u32) -> Option<HtmlElement> {
    host.query_selector_all("label.choice-card")
        .ok()?
        .item(nth)
        .map(JsCast::unchecked_into)
}

/// Текст карточки выбора, чей переключатель отмечен.
fn checked_card(host: &HtmlElement) -> Option<String> {
    host.query_selector("label.choice-card input:checked")
        .ok()
        .flatten()?
        .parent_element()?
        .text_content()
}

/// Какой шаг формы `form` (CSS) виден — номер с единицы; `None` — видно не
/// по одному шагу.
fn shown_step(host: &HtmlElement, form: &str) -> Option<u32> {
    let steps = host.query_selector_all(&format!("{form} > fieldset")).ok()?;
    let shown: Vec<u32> = (0..steps.length())
        .filter(|at| {
            steps
                .item(*at)
                .is_some_and(|step| !step.unchecked_ref::<Element>().has_attribute("hidden"))
        })
        .map(|at| at + 1)
        .collect();
    match shown.as_slice() {
        [one] => Some(*one),
        _ => None,
    }
}

/// Выбрать клиента `name` в первом шаге нового документа — строкой.
fn pick_client(host: &HtmlElement, name: &str) -> bool {
    let Ok(rows) = host.query_selector_all("ul.pick button.entry") else {
        return false;
    };
    let row = (0..rows.length())
        .filter_map(|at| rows.item(at))
        .map(|node| node.unchecked_into::<HtmlElement>())
        .find(|row| {
            row.query_selector(".entry-title")
                .ok()
                .flatten()
                .and_then(|title| title.text_content())
                .as_deref()
                == Some(name)
        });
    row.map(|row| row.click()).is_some()
}

/// Строки других языков, оставшиеся в тексте окна на языке `lang`: строка,
/// собранная один раз, при смене языка остаётся прежней (Правило 4).
/// Шаблон сверяется кусками между `{}`, и только длинными: короткий кусок
/// найдётся где угодно.
fn leftovers(text: &str, lang: Lang) -> Vec<String> {
    let mut found = Vec::new();
    for (key, name) in ALL_KEYS {
        for other in Lang::ALL {
            let theirs = t(other, *key);
            if other == lang || theirs == t(lang, *key) {
                continue;
            }
            let leftover = theirs
                .split("{}")
                .filter(|piece| piece.trim().chars().count() >= 6 || !theirs.contains("{}"))
                .any(|piece| text.contains(piece));
            if leftover {
                found.push(format!("{name} ({other:?})"));
            }
        }
    }
    found
}

/// Смена языка переводит всё окно сразу, без перезагрузки: после выбора не
/// остаётся ни строки на прежнем языке, `lang` у страницы — новый,
/// и выбор запомнен на устройстве, а введённое в поле остаётся в нём. Окно
/// открывается языком, выбранным раньше. Форма профиля открыта, и в каждом
/// её списке есть строка: их подписи тоже должны перевестись. Форма
/// клиента открыта тоже, с фирмой другой страны: список стран — на языке
/// окна и по его алфавиту. И форма нового счёта — с выбранным клиентом,
/// строкой услуги и подсказками.
#[wasm_bindgen_test]
async fn choosing_a_language_translates_the_whole_window() {
    let document = document();
    let html = document.document_element().expect("есть <html>");
    let page_lang = html.get_attribute("lang");
    let device = Device::new();
    device.set_ui_lang(Lang::Am);
    // Счёт выписывают живому клиенту: без него кнопка нового счёта
    // выключена. Армянская фирма — чтобы форма показала и слова о чеке кассы.
    let client = Client::create(
        ClientData {
            country: Some(Country::ARMENIA),
            ..ClientData {
                name: Line::new("Proba LLC").expect("годное название"),
                country: None,
                address: Lines::default(),
                tin: None,
                tax_id: Line::default(),
                currency: None,
                lang: None,
                payment_days: None,
            }
        },
        crate::platform::clock::now(),
    )
    .expect("годный клиент");
    // Профиль с данными человека: на экране годовых платежей — карточки
    // платежей со всеми их словами (задача 20).
    let mut data = documents::profile();
    data.person = Person {
        birth_year: Some(1990),
        pension_joined: None,
        pension_applied: None,
        residency: Some(Residency::ForeignWithStatus),
        resident: None,
        registered: None,
        turnover_applied: None,
        stops: Vec::new(),
    };
    let profile = Profile::save(None, data, crate::platform::clock::now())
        .expect("годный профиль")
        .expect("новый");
    let mut writes = Writes::default();
    writes.insert(&client);
    writes.insert(&profile);
    Store::open(|_| {})
        .await
        .expect("база открыта")
        .write()
        .expect("транзакция записи")
        .commit(writes)
        .await
        .expect("клиент записан");
    let (host, mounted) = mount();

    let mut wrong = Vec::new();
    // Разделы о данных появляются, когда открылась база; заголовок экрана
    // копии виден и раньше.
    if !wait_for(&host, t(Lang::Am, Key::BackupChooseFile)).await {
        wrong.push("раздел копии не появился".to_owned());
    }
    visit(
        &host,
        &[
            Screen::Documents,
            Screen::Clients,
            Screen::Tax,
            Screen::Annual,
            Screen::Deadlines,
            Screen::Report,
            Screen::Profile,
            Screen::About,
        ],
    )
    .await;
    if !wait_for(&host, t(Lang::Am, Key::AnnualSumOf)).await {
        wrong.push("экран годовых платежей не показал платежей".to_owned());
    }
    match button(
        &host,
        &[t(Lang::Am, Key::ProfileFill), t(Lang::Am, Key::ProfileEdit)],
    ) {
        Some(open) => open.click(),
        None => wrong.push("нет кнопки профиля".to_owned()),
    }
    sleep(50).await;
    for add in [Key::AccountAdd, Key::ActivityAdd, Key::StopAdd, Key::ClientAdd, Key::DocNewInvoice] {
        match button(&host, &[t(Lang::Am, add)]) {
            Some(add) => add.click(),
            None => wrong.push(format!("нет кнопки {add:?}")),
        }
    }
    sleep(50).await;
    match choice_card(&host, 1) {
        Some(abroad) => abroad.click(),
        None => wrong.push("в форме клиента нет «В другой стране»".to_owned()),
    }
    if !pick_client(&host, client.data().name.as_str()) {
        wrong.push("нет клиента в выборе формы счёта".to_owned());
    }
    sleep(50).await;
    // Экран книги рисуется с первого показа (`ui/ledger.rs`): открыть его
    // и форму расхода в долларах — с подсказками о курсе и документе.
    crate::platform::set_hash("#/ledger");
    if !wait_for(&host, t(Lang::Am, Key::LedgerAddExpense)).await {
        wrong.push("экран книги не открылся".to_owned());
    }
    match button(&host, &[t(Lang::Am, Key::LedgerAddExpense)]) {
        Some(add) => add.click(),
        None => wrong.push("нет кнопки нового расхода".to_owned()),
    }
    sleep(50).await;
    match host.query_selector("form.operation select").ok().flatten() {
        Some(currency) => pick(currency.unchecked_ref(), "USD"),
        None => wrong.push("нет выбора валюты расхода".to_owned()),
    }
    sleep(50).await;

    let typed: Option<HtmlInputElement> = host
        .query_selector("input[type=password]")
        .ok()
        .flatten()
        .map(JsCast::unchecked_into);
    if let Some(input) = &typed {
        input.set_value("пароль на полпути");
    }
    let mut steps = vec![format!("открылось: lang={:?}", html.get_attribute("lang"))];
    if html.get_attribute("lang").as_deref() != Some("hy") {
        wrong.push("запомненный армянский не открылся".to_owned());
    }
    for lang in [Lang::Ru, Lang::En, Lang::Am] {
        let button: Option<Element> = host
            .query_selector(&format!(".langs button[lang=\"{}\"]", lang.code()))
            .ok()
            .flatten();
        let Some(button) = button else {
            wrong.push(format!("{lang:?}: нет кнопки"));
            continue;
        };
        button.unchecked_ref::<HtmlElement>().click();
        sleep(50).await;
        let text = host.text_content().unwrap_or_default();
        steps.push(format!("{lang:?}: {} знаков", text.chars().count()));
        for key in [
            Key::BackupTitle,
            Key::ProfileAccounts,
            Key::StopFrom,
            Key::ClientCountry,
            Key::DocServiceDate,
            Key::DocWhatHint,
            Key::DocCashReceipt,
            Key::ClientInArmeniaHint,
            Key::RateHint,
            Key::ProofHint,
            Key::NatureHint,
            Key::ActivityKindHint,
            Key::TaxTurnover,
            Key::TaxByMonth,
            Key::TaxCheck,
            Key::AnnualSumOf,
            Key::AnnualProfitLaw,
            Key::AnnualHealth,
            Key::DeadlinesShiftRule,
            Key::ReportAbout,
            Key::ReportTurnover,
            Key::AboutDisclaimer,
            Key::AboutLicensesText,
            Key::AboutLicensesOpen,
        ] {
            if !text.contains(t(lang, key)) {
                wrong.push(format!("{lang:?}: нет {key:?} на этом языке"));
            }
        }
        match select(&host, t(lang, Key::ClientCountry), 0)
            .and_then(|countries| countries.query_selector_all("option").ok())
        {
            Some(options) => {
                // Первый вариант — «не выбрано».
                let shown: Vec<String> = (1..options.length())
                    .filter_map(|at| options.item(at))
                    .filter_map(|option| option.text_content())
                    .collect();
                let expected: Vec<&str> = Country::by_name(lang).map(|c| c.name(lang)).collect();
                if shown != expected {
                    wrong.push(format!(
                        "{lang:?}: список стран не тот: {:?}…",
                        shown.iter().take(3).collect::<Vec<_>>()
                    ));
                }
            }
            None => wrong.push(format!("{lang:?}: нет списка стран")),
        }
        let left = leftovers(&text, lang);
        if !left.is_empty() {
            wrong.push(format!("{lang:?}: остались {left:?}"));
        }
        if html.get_attribute("lang").as_deref() != Some(lang.code()) {
            wrong.push(format!("{lang:?}: lang страницы {:?}", html.get_attribute("lang")));
        }
        if button.get_attribute("aria-pressed").as_deref() != Some("true") {
            wrong.push(format!("{lang:?}: кнопка не нажата"));
        }
        if device.ui_lang() != Some(lang) {
            wrong.push(format!("{lang:?}: не запомнен, {:?}", device.ui_lang()));
        }
    }

    match &typed {
        Some(input) if input.is_connected() && input.value() == "пароль на полпути" => {}
        Some(_) => wrong.push("введённое в поле пропало".to_owned()),
        None => wrong.push("нет поля пароля".to_owned()),
    }

    drop(mounted);
    host.remove();
    crate::platform::set_hash("");
    if let Some(storage) = web_sys::window().and_then(|window| window.local_storage().ok().flatten())
    {
        let _ = storage.remove_item("kvartik.lang");
    }
    match page_lang {
        Some(code) => html.set_attribute("lang", &code).expect("ставится"),
        None => html.remove_attribute("lang").expect("снимается"),
    }
    assert!(wrong.is_empty(), "смена языка: {wrong:#?}\nшаги: {steps:#?}");
}

/// Забыть выбранный язык окна: окно откроется первым запуском.
fn forget_lang() {
    if let Some(storage) = web_sys::window().and_then(|window| window.local_storage().ok().flatten())
    {
        let _ = storage.remove_item("kvartik.lang");
    }
}

/// Отмеченный язык на экране первого запуска — код из `lang` его пилюли.
fn welcome_choice(host: &HtmlElement) -> Option<String> {
    host.query_selector(".welcome input:checked")
        .ok()
        .flatten()?
        .parent_element()?
        .get_attribute("lang")
}

/// Первый запуск: пока язык окна не выбран, окно открывается экраном
/// первого запуска на языке браузера, а само спрятано под ним; выбор языка
/// сразу переводит экран — и строку о том, что после пробного периода, —
/// и запоминается; «Продолжить» убирает экран,
/// и при новом открытии его нет. «У меня есть копия» запоминает язык
/// и ведёт к копии, где в пустой базе восстановление стоит первым.
#[wasm_bindgen_test]
async fn first_launch_asks_for_the_language() {
    crate::store::delete_database(crate::store::DB_NAME).await;
    forget_lang();
    let mut wrong = Vec::new();
    let preferred = Lang::preferred(crate::platform::languages().iter().map(String::as_str));
    let (host, mounted) = mount();
    let app_hidden = |host: &HtmlElement| {
        host.query_selector(".app")
            .ok()
            .flatten()
            .is_some_and(|app| app.has_attribute("hidden"))
    };
    let welcome = |host: &HtmlElement| host.query_selector(".welcome").ok().flatten().is_some();
    if !welcome(&host) || !app_hidden(&host) {
        wrong.push(format!(
            "первый запуск: экран — {}, окно спрятано — {}",
            welcome(&host),
            app_hidden(&host)
        ));
    }
    if welcome_choice(&host).as_deref() != Some(preferred.code()) {
        wrong.push(format!("предложен не язык браузера {preferred:?}: {:?}", welcome_choice(&host)));
    }
    match host.query_selector(".welcome label[lang=\"ru\"]").ok().flatten() {
        Some(russian) => russian.unchecked_into::<HtmlElement>().click(),
        None => wrong.push("нет русского среди языков".to_owned()),
    }
    sleep(50).await;
    let text = host.text_content().unwrap_or_default();
    if !text.contains(t(Lang::Ru, Key::WelcomeHeadline)) || welcome_choice(&host).as_deref() != Some("ru") {
        wrong.push(format!("выбор русского не перевёл экран: {:?}", welcome_choice(&host)));
    }
    // За сроком пробного периода — что потом без ключа (задача 98): в той
    // же строке и на выбранном языке.
    let hint = host
        .query_selector(".welcome-first .hint")
        .ok()
        .flatten()
        .and_then(|hint| hint.text_content())
        .unwrap_or_default();
    if !hint.ends_with(t(Lang::Ru, Key::WelcomeAfterTrial)) {
        wrong.push(format!("за сроком пробного периода нет того, что потом: {hint:?}"));
    }
    if Device::new().ui_lang() != Some(Lang::Ru) {
        wrong.push(format!("выбор не запомнен: {:?}", Device::new().ui_lang()));
    }
    match button(&host, &[t(Lang::Ru, Key::WelcomeContinue)]) {
        Some(next) => next.click(),
        None => wrong.push("нет «Продолжить»".to_owned()),
    }
    sleep(50).await;
    if welcome(&host) || app_hidden(&host) {
        wrong.push("«Продолжить» не открыло окно".to_owned());
    }
    drop(mounted);
    host.remove();
    let (host, mounted) = mount();
    if welcome(&host) || app_hidden(&host) {
        wrong.push("язык выбран, а первый запуск снова".to_owned());
    }
    drop(mounted);
    host.remove();

    // «У меня есть копия» — к восстановлению, и язык браузера запомнен.
    forget_lang();
    let (host, mounted) = mount();
    match button(&host, &[t(preferred, Key::WelcomeHaveBackup)]) {
        Some(backup) => backup.click(),
        None => wrong.push("нет «У меня есть копия»".to_owned()),
    }
    sleep(50).await;
    if welcome(&host) || crate::platform::hash() != "#/backup" {
        wrong.push(format!("«У меня есть копия» не повело к копии: {}", crate::platform::hash()));
    }
    if Device::new().ui_lang() != Some(preferred) {
        wrong.push(format!("язык браузера не запомнен: {:?}", Device::new().ui_lang()));
    }
    if !wait_for(&host, t(preferred, Key::BackupChooseFile)).await {
        wrong.push("на экране копии нет восстановления".to_owned());
    }
    let restore = host.query_selector("#backup p.restore").ok().flatten();
    let make = host.query_selector("#backup form.backup").ok().flatten();
    match (restore, make) {
        (Some(restore), Some(make))
            if restore.compare_document_position(&make) & web_sys::Node::DOCUMENT_POSITION_FOLLOWING != 0 => {}
        _ => wrong.push("в пустой базе восстановление не первым".to_owned()),
    }
    drop(mounted);
    host.remove();
    forget_lang();
    assert!(wrong.is_empty(), "первый запуск: {wrong:#?}");
}

/// `id` видимых экранов окна.
fn shown_screens(host: &HtmlElement) -> Vec<String> {
    let Ok(sections) = host.query_selector_all("section.screen") else {
        return Vec::new();
    };
    (0..sections.length())
        .filter_map(|at| sections.item(at))
        .map(|node| node.unchecked_into::<Element>())
        .filter(|section| !section.has_attribute("hidden"))
        .map(|section| section.id())
        .collect()
}

/// Адреса вкладок меню, отмеченных открытыми (`aria-current`).
fn current_tabs(host: &HtmlElement) -> Vec<String> {
    let Ok(tabs) = host.query_selector_all(r#".tabs a[aria-current="page"]"#) else {
        return Vec::new();
    };
    (0..tabs.length())
        .filter_map(|at| tabs.item(at))
        .filter_map(|tab| tab.unchecked_into::<Element>().get_attribute("href"))
        .collect()
}

/// Сколько раз по 20 мс `look` ждёт экрана: две секунды. Переход по адресу
/// и «Назад» браузера приходят событием `hashchange` своей задачей, а не
/// в самом нажатии, и на занятой машине — позже, чем кажется: в CI «Назад»
/// пришёл после 200 мс ожидания и перекрыл следующее нажатие вкладки
/// (Chrome, 2026-11-06). Поэтому тест ждёт состояния, а не времени.
const LOOK_TRIES: usize = 100;

/// Подождать, пока окно покажет один экран `screen` с открытой вкладкой
/// `tab`, но не дольше `LOOK_TRIES` раз по 20 мс, — и записать в `wrong`,
/// что видно, если не дождались.
async fn look(host: &HtmlElement, step: &str, screen: &str, tab: &str, wrong: &mut Vec<String>) {
    let mut seen = (Vec::new(), Vec::new());
    for _ in 0..LOOK_TRIES {
        seen = (shown_screens(host), current_tabs(host));
        if seen.0 == [screen] && seen.1 == [tab] {
            return;
        }
        sleep(20).await;
    }
    let (shown, current) = seen;
    wrong.push(format!("{step}: видны {shown:?}, открыта вкладка {current:?}"));
}

/// Экран — по адресу: окно, открытое с `#/clients`, показывает клиентов,
/// а остальные экраны прячет; вкладка меню открывает свой экран и отмечена
/// открытой; «Назад» браузера возвращает прежний экран, а не уводит из
/// программы; книга — своей вкладкой; копия — экран внутри профиля; чужой
/// адрес — главный экран.
#[wasm_bindgen_test]
async fn screens_follow_the_address() {
    use crate::platform::{hash, set_hash};

    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let (host, mounted) = mount_at("#/clients");
    let mut wrong = Vec::new();
    look(&host, "с #/clients", "clients", "#/clients", &mut wrong).await;

    match host.query_selector(r##".tabs a[href="#/documents"]"##).ok().flatten() {
        Some(tab) => tab.unchecked_into::<HtmlElement>().click(),
        None => wrong.push("нет вкладки счетов".to_owned()),
    }
    look(&host, "вкладка счетов", "documents", "#/documents", &mut wrong).await;
    if hash() != "#/documents" {
        wrong.push(format!("адрес после вкладки — {}", hash()));
    }

    if let Some(window) = web_sys::window() {
        let _ = window.history().map(|history| history.back());
    }
    look(&host, "«Назад»", "clients", "#/clients", &mut wrong).await;

    match host.query_selector(r##".tabs a[href="#/ledger"]"##).ok().flatten() {
        Some(tab) => tab.unchecked_into::<HtmlElement>().click(),
        None => wrong.push("нет вкладки книги".to_owned()),
    }
    look(&host, "вкладка книги", "ledger", "#/ledger", &mut wrong).await;

    set_hash("#/backup");
    look(&host, "копия", "backup", "#/profile", &mut wrong).await;

    set_hash("#/nothing");
    look(&host, "чужой адрес", "home", "#/", &mut wrong).await;

    drop(mounted);
    host.remove();
    set_hash("");
    if let Some(storage) = web_sys::window().and_then(|window| window.local_storage().ok().flatten())
    {
        let _ = storage.remove_item("kvartik.lang");
    }
    assert!(wrong.is_empty(), "экраны: {wrong:#?}");
}

/// Пора сделать копию — метка о ней стоит под заголовком каждого экрана,
/// кроме главного (там карточка) и самого экрана копии: напоминать обязано
/// каждое открытие, на каком бы экране оно ни было (CLAUDE.md, Правило 2).
/// Экран лицензии с 0.0.60 по 0.0.76 её не показывал.
#[wasm_bindgen_test]
async fn backup_label_stands_on_every_screen_when_due() {
    documents::fresh_base_with_client().await;
    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    // Копии на этом устройстве не было: в базе есть данные — пора.
    if let Some(storage) = web_sys::window().and_then(|window| window.local_storage().ok().flatten()) {
        let _ = storage.remove_item("kvartik.last_backup");
    }
    let (host, mounted) = mount();
    let mut wrong = Vec::new();
    if !wait_for(&host, t(lang, Key::BackupChooseFile)).await {
        wrong.push("база не открылась".to_owned());
    }
    for screen in Screen::PLAIN {
        if matches!(screen, Screen::Home | Screen::Backup) {
            continue;
        }
        crate::platform::set_hash(&screen.href());
        // По адресу метки: рядом бывает и метка срока лицензии (задача 92).
        let label = format!("#{} a.chip.nudge[href=\"{}\"]", screen.id(), Screen::Backup.href());
        let mut seen = false;
        for _ in 0..100 {
            if shown_screens(&host) == [screen.id()] && host.query_selector(&label).ok().flatten().is_some() {
                seen = true;
                break;
            }
            sleep(20).await;
        }
        if !seen {
            wrong.push(format!("{screen:?}: нет метки копии под заголовком"));
        }
    }
    drop(mounted);
    host.remove();
    crate::platform::set_hash("");
    documents::forget_device();
    assert!(wrong.is_empty(), "метка копии: {wrong:#?}");
}

/// Текст раздела `#id` окна; нет раздела — пусто.
fn section_text(host: &HtmlElement, id: &str) -> String {
    host.query_selector(&format!("#{id}"))
        .ok()
        .flatten()
        .and_then(|section| section.text_content())
        .unwrap_or_default()
}

/// Счета, клиенты и профиль рисуются с первого показа (`split::screen`,
/// задача 61): до него в странице только заголовок экрана, а показанный
/// однажды экран остаётся в ней спрятанным — с тем, что начато в его поле.
/// Данные разделов прочитаны и без показа: главный экран знает профиль.
#[wasm_bindgen_test]
async fn hidden_screens_are_drawn_on_first_show() {
    let client = documents::fresh_base_with_client().await;
    let name = client.data().name.as_str().to_owned();
    let profile = documents::profile().name.ru.as_str().to_owned();
    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let (host, mounted) = mount();
    let mut wrong = Vec::new();
    if !wait_for(&host, t(lang, Key::BackupChooseFile)).await {
        wrong.push("база не открылась".to_owned());
    }
    if section_text(&host, "home").contains(t(lang, Key::StepsFillRequisites)) {
        wrong.push("главный экран не знает профиля, пока экран профиля не показан".to_owned());
    }
    let drawn = |host: &HtmlElement, wrong: &mut Vec<String>, step: &str, expected: [bool; 3]| {
        let shown = [
            section_text(host, "documents").contains(t(lang, Key::DocsAbout)),
            section_text(host, "clients").contains(&name),
            section_text(host, "profile").contains(&profile),
        ];
        if shown != expected {
            wrong.push(format!("{step}: нарисованы счета, клиенты, профиль — {shown:?}, а не {expected:?}"));
        }
    };
    drawn(&host, &mut wrong, "до показа", [false, false, false]);

    crate::platform::set_hash(&Screen::Clients.href());
    if !wait_for(&host, &name).await {
        wrong.push("экран клиентов не нарисовался при показе".to_owned());
    }
    let search: Option<HtmlInputElement> = host
        .query_selector("#clients input[type=search]")
        .ok()
        .flatten()
        .map(JsCast::unchecked_into);
    match &search {
        Some(input) => type_into(input, "Ac"),
        None => wrong.push("у клиентов нет поиска".to_owned()),
    }
    visit(&host, &[]).await;
    drawn(&host, &mut wrong, "после клиентов", [false, true, false]);
    match &search {
        Some(input) if input.is_connected() && input.value() == "Ac" => {}
        Some(_) => wrong.push("начатое в поиске клиентов пропало при уходе с экрана".to_owned()),
        None => {}
    }

    visit(&host, &[Screen::Documents, Screen::Profile]).await;
    drawn(&host, &mut wrong, "после всех", [true, true, true]);

    drop(mounted);
    host.remove();
    crate::platform::set_hash("");
    documents::forget_device();
    assert!(wrong.is_empty(), "экраны с первого показа: {wrong:#?}");
}

/// Положить файл в поле выбора файла, как будто его выбрал человек.
fn choose_file(input: &HtmlInputElement, name: &str, text: &str) {
    let file = web_sys::File::new_with_str_sequence(&js_sys::Array::of1(&text.into()), name)
        .expect("файл собран");
    let transfer = web_sys::DataTransfer::new().expect("DataTransfer есть");
    transfer.items().add_with_file(&file).expect("файл положен");
    input.set_files(transfer.files().as_ref());
    let event = web_sys::Event::new("change").expect("событие создаётся");
    input.dispatch_event(&event).expect("событие отправлено");
}

/// Восстановление из копии меняет базу в обход разделов окна, и раздел
/// профиля обязан перечитать её: иначе на новом устройстве после
/// восстановления окно просит «заполнить профиль», который уже в базе,
/// и второй профиль появляется рядом с первым. Копия — образец версии 0.0.12
/// с профилем и логотипом.
#[wasm_bindgen_test]
async fn restoring_a_backup_shows_its_profile() {
    crate::store::delete_database(crate::store::DB_NAME).await;
    let lang = Lang::Ru;
    let device = Device::new();
    device.set_ui_lang(lang);
    let (host, mounted) = mount();
    let mut wrong = Vec::new();
    if !wait_for(&host, t(lang, Key::StepsFillRequisites)).await {
        wrong.push("пустая база — а профиль не предлагают заполнить".to_owned());
    }
    // Раздел копии появляется позже профиля: после ответа браузера
    // о постоянном хранении, а он в CI бывает небыстрым.
    if !wait_for(&host, t(lang, Key::BackupChooseFile)).await {
        wrong.push("раздела копии нет".to_owned());
    }
    let input: Option<HtmlInputElement> = host
        .query_selector(r#"input[type="file"][accept=".json,application/json"]"#)
        .ok()
        .flatten()
        .map(JsCast::unchecked_into);
    match &input {
        Some(input) => choose_file(
            input,
            "kvartik-2026-10-12.json",
            include_str!("../../../core/tests/backups/v2/kvartik-2026-10-12.json"),
        ),
        None => wrong.push("нет поля выбора копии".to_owned()),
    }
    if !wait_for(&host, t(lang, Key::BackupFilePassword)).await {
        wrong.push("копия не открылась".to_owned());
    }
    match field(&host, t(lang, Key::BackupFilePassword), 0) {
        Some(password) => type_into(&password, "Պահուստ копия sample 0.0.12"),
        None => wrong.push("нет поля пароля копии".to_owned()),
    }
    match button(&host, &[t(lang, Key::BackupRestore)]) {
        Some(restore) => restore.click(),
        None => wrong.push("нет кнопки «Восстановить»".to_owned()),
    }
    let done = t(lang, Key::BackupRestored).split("{}").next().unwrap_or_default();
    if !wait_for(&host, done).await {
        wrong.push("восстановление не завершилось".to_owned());
    }
    // Экран профиля ещё ни разу не показан (`split::screen`), а главный
    // экран обязан узнать профиль копии и без него.
    let missing = t(lang, Key::StepsFillRequisites);
    for _ in 0..100 {
        if !section_text(&host, "home").contains(missing) {
            break;
        }
        sleep(50).await;
    }
    if section_text(&host, "home").contains(missing) {
        wrong.push("после восстановления главный экран всё просит заполнить профиль".to_owned());
    }
    visit(&host, &[Screen::Profile]).await;
    if !wait_for(&host, "ИП Арам Петросян").await {
        wrong.push("после восстановления раздел профиля не показал профиль копии".to_owned());
    }
    drop(mounted);
    host.remove();
    if let Some(storage) = web_sys::window().and_then(|window| window.local_storage().ok().flatten())
    {
        // И день данных: образец копии приносит прошедшие дни пробного
        // периода, и устройство их помнит (задача 99) — следующим тестам
        // нужен свой пробный период.
        for name in ["kvartik.lang", "kvartik.backup_key", "kvartik.last_backup", "kvartik.trial_data"] {
            let _ = storage.remove_item(name);
        }
    }
    // База с записями образца — тоже: по их моментам пробный период
    // следующих тестов кончился бы (задача 99).
    crate::store::delete_database(crate::store::DB_NAME).await;
    assert!(wrong.is_empty(), "восстановление: {wrong:#?}");
}

/// Выбрать образец копии 0.0.12 и нажать «Восстановить» с его паролем,
/// как человек.
async fn restore_sample_0_0_12(host: &HtmlElement, lang: Lang, wrong: &mut Vec<String>) {
    if !wait_for(host, t(lang, Key::BackupChooseFile)).await {
        wrong.push("раздела копии нет".to_owned());
    }
    let input: Option<HtmlInputElement> = host
        .query_selector(r#"input[type="file"][accept=".json,application/json"]"#)
        .ok()
        .flatten()
        .map(JsCast::unchecked_into);
    match &input {
        Some(input) => choose_file(
            input,
            "kvartik-2026-10-12.json",
            include_str!("../../../core/tests/backups/v2/kvartik-2026-10-12.json"),
        ),
        None => wrong.push("нет поля выбора копии".to_owned()),
    }
    if !wait_for(host, t(lang, Key::BackupFilePassword)).await {
        wrong.push("копия не открылась".to_owned());
    }
    match field(host, t(lang, Key::BackupFilePassword), 0) {
        Some(password) => type_into(&password, "Պահուստ копия sample 0.0.12"),
        None => wrong.push("нет поля пароля копии".to_owned()),
    }
    match button(host, &[t(lang, Key::BackupRestore)]) {
        Some(restore) => restore.click(),
        None => wrong.push("нет кнопки «Восстановить»".to_owned()),
    }
}

/// Кнопка в карточке вопроса о копии другого ИП.
fn other_ie_button(host: &HtmlElement, lang: Lang, label: &str) -> Option<HtmlElement> {
    let notices = host.query_selector_all(".notice").ok()?;
    (0..notices.length())
        .filter_map(|at| notices.item(at))
        .map(|node| node.unchecked_into::<HtmlElement>())
        .find(|notice| {
            notice
                .text_content()
                .is_some_and(|text| text.contains(t(lang, Key::BackupOtherIe)))
        })
        .and_then(|notice| button(&notice, &[label]))
}

/// Записей профиля в базе программы — живых и удалённых.
async fn stored_profiles() -> Result<usize, StoreError> {
    let store = Store::open(|_| {}).await?;
    Ok(store.read()?.all::<Profile>().await?.len())
}

/// Копия другого ИП в программу с профилем молча не ложится (задача 38,
/// этап 1): окно называет обоих — имя и ՀՎՀՀ, — «Отмена» не пишет ничего,
/// а «Всё равно слить» сливает. В программе — «ИП Анна Акопян» с ՀՎՀՀ,
/// которого у образца 0.0.12 («ИП Арам Петросян») нет.
#[wasm_bindgen_test]
async fn a_backup_of_another_ie_asks_before_merging() {
    crate::store::delete_database(crate::store::DB_NAME).await;
    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    // Копий на этом устройстве ещё не было: копия другого ИП, слитая по
    // ответу, копией данных программы не станет.
    if let Some(storage) = web_sys::window().and_then(|window| window.local_storage().ok().flatten())
    {
        let _ = storage.remove_item("kvartik.last_backup");
    }
    let mut data = ProfileData::default();
    data.name.ru = Line::new("ИП Анна Акопян").expect("годное имя");
    data.tin = Some(Tin::parse("99999990").expect("годный ՀՎՀՀ"));
    let anna = Profile::save(None, data, Timestamp::from_unix_millis(1_700_000_000_000))
        .expect("годный профиль")
        .expect("новый профиль");
    let store = Store::open(|_| {}).await.expect("база открыта");
    let mut writes = Writes::default();
    writes.insert(&anna);
    store
        .write()
        .expect("транзакция записи")
        .commit(writes)
        .await
        .expect("профиль сохранён");
    drop(store);
    let (host, mounted) = mount();
    let mut wrong = Vec::new();

    restore_sample_0_0_12(&host, lang, &mut wrong).await;
    if !wait_for(&host, t(lang, Key::BackupOtherIe)).await {
        wrong.push("о копии другого ИП не спросили".to_owned());
    }
    let tin = fill(t(lang, Key::TinShort), &["99999990"]);
    let ours = fill(t(lang, Key::BackupOtherIeOurs), &[&format!("ИП Анна Акопян, {tin}")]);
    let theirs = fill(t(lang, Key::BackupOtherIeTheirs), &["ИП Арам Петросян"]);
    for line in [&ours, &theirs] {
        if !wait_for(&host, line).await {
            wrong.push(format!("в вопросе нет «{line}»"));
        }
    }
    match other_ie_button(&host, lang, t(lang, Key::Cancel)) {
        Some(cancel) => cancel.click(),
        None => wrong.push("у вопроса нет «Отмены»".to_owned()),
    }
    if !wait_for(&host, t(lang, Key::BackupChooseFile)).await
        || host.text_content().unwrap_or_default().contains(t(lang, Key::BackupOtherIe))
    {
        wrong.push("после «Отмены» вопрос остался".to_owned());
    }
    if stored_profiles().await != Ok(1) {
        wrong.push(format!("после «Отмены» профилей в базе {:?}, а не 1", stored_profiles().await));
    }

    restore_sample_0_0_12(&host, lang, &mut wrong).await;
    if !wait_for(&host, t(lang, Key::BackupOtherIe)).await {
        wrong.push("второй раз о копии другого ИП не спросили".to_owned());
    }
    match other_ie_button(&host, lang, t(lang, Key::BackupOtherIeMerge)) {
        Some(merge) => merge.click(),
        None => wrong.push("у вопроса нет «Всё равно слить»".to_owned()),
    }
    let done = t(lang, Key::BackupRestored).split("{}").next().unwrap_or_default();
    if !wait_for(&host, done).await {
        wrong.push("после «Всё равно слить» восстановление не завершилось".to_owned());
    }
    if stored_profiles().await != Ok(2) {
        wrong.push(format!("после «Всё равно слить» профилей {:?}, а не 2", stored_profiles().await));
    }
    if let Some(day) = Device::new().last_backup() {
        wrong.push(format!("копия другого ИП стала днём последней копии: {day:?}"));
    }
    drop(mounted);
    host.remove();
    if let Some(storage) = web_sys::window().and_then(|window| window.local_storage().ok().flatten())
    {
        // И день данных: образец копии приносит прошедшие дни пробного
        // периода, и устройство их помнит (задача 99) — следующим тестам
        // нужен свой пробный период.
        for name in ["kvartik.lang", "kvartik.backup_key", "kvartik.last_backup", "kvartik.trial_data"] {
            let _ = storage.remove_item(name);
        }
    }
    crate::store::delete_database(crate::store::DB_NAME).await;
    assert!(wrong.is_empty(), "копия другого ИП: {wrong:#?}");
}

/// Профиль из базы программы — тот, что окно показывает.
async fn stored_profile() -> Result<Option<Profile>, StoreError> {
    let store = Store::open(|_| {}).await?;
    let profiles = store.read()?.all::<Profile>().await?;
    Ok(Profile::current(profiles))
}

/// Профиль сохраняется одной командой и читается обратно: ошибочный ՀՎՀՀ
/// не даёт сохранить и говорит об этом у поля, код, закрывающий налог
/// с оборота, и опечатка в номере счёта предупреждают сразу при вводе,
/// но сохранить не мешают, о пенсии спрашивают по году рождения,
/// а сохранённое лежит в базе и видно в окне после нового открытия.
#[wasm_bindgen_test]
async fn profile_is_saved_and_read_back() {
    // С пустой базы: профиль, оставленный другим тестом, принёс бы свои
    // строки списков.
    crate::store::delete_database(crate::store::DB_NAME).await;
    let lang = Lang::Ru;
    let device = Device::new();
    device.set_ui_lang(lang);
    let (host, mounted) = mount_at("#/profile");
    let mut wrong = Vec::new();
    // Заголовок экрана профиля виден сразу, а кнопка — когда база прочитана.
    if !wait_for(&host, t(lang, Key::ProfileFill)).await {
        wrong.push("пустая база — а профиль не предлагают заполнить".to_owned());
    }
    match button(&host, &[t(lang, Key::ProfileFill), t(lang, Key::ProfileEdit)]) {
        Some(open) => open.click(),
        None => wrong.push("нет кнопки профиля".to_owned()),
    }
    sleep(50).await;

    let name = format!("ИП Проба {}", js_sys::Date::now());
    match field(&host, t(lang, Key::ProfileNameRu), 0) {
        Some(input) => type_into(&input, &name),
        None => wrong.push("нет поля имени".to_owned()),
    }
    let tin = field(&host, t(lang, Key::ProfileTin), 0);
    if let Some(input) = &tin {
        type_into(input, "0005432");
    } else {
        wrong.push("нет поля ՀՎՀՀ".to_owned());
    }
    // Ровно один код в форме: строк могло остаться от прошлого прогона.
    let codes = (0..10).filter_map(|nth| field(&host, t(lang, Key::ActivityCode), nth)).count();
    if codes == 0 {
        if let Some(add) = button(&host, &[t(lang, Key::ActivityAdd)]) {
            add.click();
        }
        sleep(50).await;
    }
    match field(&host, t(lang, Key::ActivityCode), 0) {
        // Раздел 70 — буква M: буква впереди читается, если она своя.
        Some(input) => type_into(&input, "M70.22.0"),
        None => wrong.push("нет поля кода".to_owned()),
    }
    sleep(50).await;
    let warning = t(lang, Key::ActivityExcluded)
        .split("{}")
        .nth(1)
        .unwrap_or_default()
        .to_owned();
    if !host.text_content().is_some_and(|text| text.contains(warning.trim())) {
        wrong.push("код 70.22.0 не предупредил до сохранения".to_owned());
    }
    // Опечатка в номере счёта — предупреждение сразу, а сохранить не мешает.
    if let Some(add) = button(&host, &[t(lang, Key::AccountAdd)]) {
        add.click();
    }
    sleep(50).await;
    let typo = t(lang, Key::AccountNumberTypo);
    match field(&host, t(lang, Key::AccountNumber), 0) {
        Some(input) => {
            type_into(&input, "1270 2748 2877");
            sleep(50).await;
            if host.text_content().is_some_and(|text| text.contains(typo)) {
                wrong.push("верный номер 127027482877 назван опечаткой".to_owned());
            }
            type_into(&input, "127027482878");
            sleep(50).await;
            if !host.text_content().is_some_and(|text| text.contains(typo)) {
                wrong.push("номер 127027482878 не предупредил до сохранения".to_owned());
            }
        }
        None => wrong.push("нет поля номера счёта".to_owned()),
    }
    // О накопительной пенсии спрашивают только родившихся до 1974 года —
    // по правилу файла правил на сегодня (`tax::annual`).
    let pension = t(lang, Key::PensionJoined);
    match field(&host, t(lang, Key::BirthYear), 0) {
        Some(input) => {
            type_into(&input, "1990");
            sleep(50).await;
            if host.text_content().is_some_and(|text| text.contains(pension)) {
                wrong.push("родившегося в 1990 году спрашивают о пенсии".to_owned());
            }
            type_into(&input, "1960");
            sleep(50).await;
            if !host.text_content().is_some_and(|text| text.contains(pension)) {
                wrong.push("родившегося в 1960 году не спрашивают о пенсии".to_owned());
            }
            type_into(&input, "");
        }
        None => wrong.push("нет поля года рождения".to_owned()),
    }
    // День регистрации ИП — полем даты; с его месяца годовые платежи
    // (задача 20).
    match field(&host, t(lang, Key::RegisteredDay), 0) {
        Some(input) => type_into(&input, "2022-05-12"),
        None => wrong.push("нет поля даты регистрации".to_owned()),
    }

    let save = button(&host, &[t(lang, Key::ProfileSave)]);
    if let Some(save) = &save {
        save.click();
    } else {
        wrong.push("нет кнопки «Сохранить»".to_owned());
    }
    sleep(100).await;
    let text = host.text_content().unwrap_or_default();
    if !text.contains(t(lang, Key::TinInvalid)) || !text.contains(t(lang, Key::ProfileFix)) {
        wrong.push("семь цифр ՀՎՀՀ сохранились без слова".to_owned());
    }
    if let Some(input) = &tin {
        type_into(input, "00054321");
    }
    if let Some(save) = &save {
        save.click();
    }
    if !wait_for(&host, t(lang, Key::ProfileSaved)).await {
        wrong.push("сохранение не подтвердилось".to_owned());
    }
    let text = host.text_content().unwrap_or_default();
    for shown in [name.as_str(), "00054321", "70.22.0"] {
        if !text.contains(shown) {
            wrong.push(format!("в итоге нет «{shown}»"));
        }
    }
    // Строка итога открывает форму — на своей группе полей.
    let row = host
        .query_selector_all("#profile button.entry")
        .ok()
        .and_then(|rows| {
            (0..rows.length())
                .filter_map(|at| rows.item(at))
                .map(JsCast::unchecked_into::<HtmlElement>)
                .find(|row| row.text_content().unwrap_or_default().contains(t(lang, Key::ProfileActivities)))
        });
    match row {
        Some(row) => row.click(),
        None => wrong.push("в итоге нет строки видов деятельности".to_owned()),
    }
    sleep(100).await;
    if host.query_selector("#profile-activities").ok().flatten().is_none()
        || button(&host, &[t(lang, Key::ProfileSave)]).is_none()
    {
        wrong.push("строка итога не открыла форму".to_owned());
    }
    if button(&host, &[t(lang, Key::Cancel)]).is_none() {
        wrong.push("в форме нет «Отмены»".to_owned());
    }
    // Сохранить открытую снова форму: что сохранено раньше — день
    // регистрации тоже, — форма приносит обратно, а не стирает.
    match button(&host, &[t(lang, Key::ProfileSave)]) {
        Some(save) => save.click(),
        None => wrong.push("в открытой снова форме нет «Сохранить»".to_owned()),
    }
    if !wait_for(&host, t(lang, Key::ProfileSaved)).await {
        wrong.push("повторное сохранение не подтвердилось".to_owned());
    }

    match stored_profile().await {
        Ok(Some(profile)) => {
            let data = profile.data();
            if data.name.ru.as_str() != name
                || data.tin.as_ref().map(|tin| tin.as_str()) != Some("00054321")
                || data.activities.iter().map(ToString::to_string).collect::<Vec<_>>()
                    != ["70.22.0"]
                || data.accounts.iter().map(|account| account.number().as_str()).collect::<Vec<_>>()
                    != ["127027482878"]
                || data.person.registered.map(|day| day.iso()).as_deref() != Some("2022-05-12")
            {
                wrong.push(format!("в базе не то: {data:?}"));
            }
            if profile.meta().is_deleted() {
                wrong.push("профиль в базе удалён".to_owned());
            }
        }
        other => wrong.push(format!("в базе профиля нет: {other:?}")),
    }

    drop(mounted);
    host.remove();
    let (host, mounted) = mount_at("#/profile");
    if !wait_for(&host, &name).await {
        wrong.push("после нового открытия профиля не видно".to_owned());
    }
    drop(mounted);
    host.remove();
    if let Some(storage) = web_sys::window().and_then(|window| window.local_storage().ok().flatten())
    {
        let _ = storage.remove_item("kvartik.lang");
    }
    assert!(wrong.is_empty(), "профиль: {wrong:#?}");
}

/// Названия живых клиентов в списке — по порядку.
fn client_names(host: &HtmlElement) -> Vec<String> {
    let Ok(names) = host.query_selector_all("ul.clients:not(.deleted) .client-name") else {
        return Vec::new();
    };
    (0..names.length())
        .filter_map(|at| names.item(at))
        .filter_map(|name| name.text_content())
        .collect()
}

/// Строка клиента `name` в списке `list` (CSS).
fn client_row(host: &HtmlElement, list: &str, name: &str) -> Option<HtmlElement> {
    let items = host.query_selector_all(&format!("{list} li")).ok()?;
    (0..items.length())
        .filter_map(|at| items.item(at))
        .map(|node| node.unchecked_into::<HtmlElement>())
        .find(|item| {
            item.query_selector(".client-name")
                .ok()
                .flatten()
                .and_then(|title| title.text_content())
                .as_deref()
                == Some(name)
        })
}

/// Кнопка `label` у клиента `name` в списке `list` (CSS).
fn row_button(host: &HtmlElement, list: &str, name: &str, label: &str) -> Option<HtmlElement> {
    button(&client_row(host, list, name)?, &[label])
}

/// Строка живого клиента `name` — кнопка, которая открывает его форму.
fn client_entry(host: &HtmlElement, name: &str) -> Option<HtmlElement> {
    client_row(host, "ul.clients:not(.deleted)", name)?
        .query_selector("button.entry")
        .ok()
        .flatten()
        .map(JsCast::unchecked_into)
}

/// Клиенты: новый — по шагам; первый шаг не пускает дальше без названия
/// и с неверным ՀՎՀՀ, второй не сохраняет срок больше года, и форма
/// говорит об этом у поля; «назад» возвращает к первому шагу с введённым;
/// ՀՎՀՀ спрашивается только у армянской фирмы, а страна — только у фирмы
/// другой страны; сохранённое видно в списке и лежит в базе; поиск
/// находит по части названия без учёта «ё»; удалённый уходит из списка
/// и возвращается — из сообщения об удалении и из списка удалённых; после
/// нового открытия клиенты на месте.
#[wasm_bindgen_test]
async fn clients_are_added_found_deleted_and_restored() {
    crate::store::delete_database(crate::store::DB_NAME).await;
    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let (host, mounted) = mount_at("#/clients");
    let mut wrong = Vec::new();
    let press = |label: &str, wrong: &mut Vec<String>| match button(&host, &[label]) {
        Some(button) => button.click(),
        None => wrong.push(format!("нет кнопки «{label}»")),
    };
    if !wait_for(&host, t(lang, Key::ClientsAbout)).await {
        wrong.push("пустая база — а о клиентах ни слова".to_owned());
    }
    press(t(lang, Key::ClientAdd), &mut wrong);
    sleep(50).await;
    if shown_step(&host, "form.client") != Some(1) {
        wrong.push(format!("новый клиент — не с первого шага: {:?}", shown_step(&host, "form.client")));
    }
    press(t(lang, Key::ClientNextDefaults), &mut wrong);
    sleep(50).await;
    let text = host.text_content().unwrap_or_default();
    // Шаг ничего не сохраняет — и «не сохранено» не говорит.
    if !text.contains(t(lang, Key::ClientNameMissing))
        || !text.contains(t(lang, Key::StepFix))
        || text.contains(t(lang, Key::ProfileFix))
    {
        wrong.push("клиент без названия прошёл дальше без слова".to_owned());
    }
    if shown_step(&host, "form.client") != Some(1) {
        wrong.push("клиент без названия прошёл ко второму шагу".to_owned());
    }
    if field(&host, t(lang, Key::ProfileTin), 0).is_some() || select(&host, t(lang, Key::ClientCountry), 0).is_some() {
        wrong.push("ՀՎՀՀ или страна спрошены, когда не выбрано, где фирма".to_owned());
    }

    let name = "ООО Семён и партнёры";
    match field(&host, t(lang, Key::ClientName), 0) {
        Some(input) => type_into(&input, name),
        None => wrong.push("нет поля названия".to_owned()),
    }
    match choice_card(&host, 0) {
        Some(armenia) => armenia.click(),
        None => wrong.push("нет карточки «В Армении»".to_owned()),
    }
    sleep(50).await;
    // Выбор — настоящим переключателем: отмечен тот, что в нажатой карточке.
    if !checked_card(&host).is_some_and(|text| text.contains(t(lang, Key::ClientInArmenia))) {
        wrong.push(format!("отмечена не «В Армении»: {:?}", checked_card(&host)));
    }
    if select(&host, t(lang, Key::ClientCountry), 0).is_some() {
        wrong.push("у армянской фирмы спрошена страна".to_owned());
    }
    let tin = field(&host, t(lang, Key::ProfileTin), 0);
    match &tin {
        Some(input) => type_into(input, "0005432"),
        None => wrong.push("у армянской фирмы не спрошен ՀՎՀՀ".to_owned()),
    }
    press(t(lang, Key::ClientNextDefaults), &mut wrong);
    sleep(50).await;
    if !host.text_content().unwrap_or_default().contains(t(lang, Key::TinInvalid))
        || shown_step(&host, "form.client") != Some(1)
    {
        wrong.push("семь цифр ՀՎՀՀ прошли ко второму шагу без слова".to_owned());
    }
    if let Some(input) = &tin {
        type_into(input, "00054321");
    }
    press(t(lang, Key::ClientNextDefaults), &mut wrong);
    sleep(50).await;
    if shown_step(&host, "form.client") != Some(2) {
        wrong.push("верный первый шаг не пустил ко второму".to_owned());
    }
    // Фирма из Армении: на втором шаге драмы и армянский уже выбраны
    // (полный прогон 2026-11-10, п. 18), а сменить их можно ниже.
    let chosen = |label: Key| select(&host, t(lang, label), 0).map(|select| select.value());
    if (chosen(Key::ClientCurrency), chosen(Key::ClientLang)) != (Some("AMD".to_owned()), Some("hy".to_owned())) {
        wrong.push(format!(
            "у армянской фирмы не выбраны драмы и армянский: {:?} {:?}",
            chosen(Key::ClientCurrency),
            chosen(Key::ClientLang)
        ));
    }
    // «Назад» — к первому шагу, и введённое на месте.
    match host.query_selector("form.client .wizard-head button.round").ok().flatten() {
        Some(back) => back.unchecked_into::<HtmlElement>().click(),
        None => wrong.push("у формы клиента нет «назад»".to_owned()),
    }
    sleep(50).await;
    match field(&host, t(lang, Key::ClientName), 0) {
        Some(input) if shown_step(&host, "form.client") == Some(1) && input.value() == name => {}
        other => wrong.push(format!(
            "«назад» не вернул первый шаг с названием: шаг {:?}, {:?}",
            shown_step(&host, "form.client"),
            other.map(|input| input.value())
        )),
    }
    press(t(lang, Key::ClientNextDefaults), &mut wrong);
    sleep(50).await;
    match select(&host, t(lang, Key::ClientCurrency), 0) {
        Some(currencies) => pick(&currencies, "USD"),
        None => wrong.push("нет списка валют".to_owned()),
    }
    match select(&host, t(lang, Key::ClientLang), 0) {
        Some(langs) => pick(&langs, "en"),
        None => wrong.push("нет списка языков".to_owned()),
    }
    let days = field(&host, t(lang, Key::ClientPaymentDays), 0);
    match &days {
        Some(input) => type_into(input, "400"),
        None => wrong.push("нет поля срока оплаты".to_owned()),
    }
    press(t(lang, Key::ProfileSave), &mut wrong);
    sleep(50).await;
    let text = host.text_content().unwrap_or_default();
    let days_problem = t(lang, Key::PaymentDaysInvalid).split("{}").next().unwrap_or_default();
    if !text.contains(days_problem) {
        wrong.push("срок 400 дней сохранился без слова".to_owned());
    }
    if let Some(input) = &days {
        type_into(input, "15");
    }
    press(t(lang, Key::ProfileSave), &mut wrong);
    if !wait_for(&host, t(lang, Key::ClientSaved)).await {
        wrong.push("сохранение не подтвердилось".to_owned());
    }
    let text = host.text_content().unwrap_or_default();
    for shown in [
        name.to_owned(),
        "Армения".to_owned(),
        fill(t(lang, Key::TinShort), &["00054321"]),
        "USD".to_owned(),
        "English".to_owned(),
        fill(t(lang, Key::ClientPaymentTerm), &["15\u{A0}дней"]),
    ] {
        if !text.contains(&shown) {
            wrong.push(format!("в списке нет «{shown}»"));
        }
    }

    press(t(lang, Key::ClientAdd), &mut wrong);
    sleep(50).await;
    if let Some(input) = field(&host, t(lang, Key::ClientName), 0) {
        type_into(&input, "Acme Inc.");
    }
    match choice_card(&host, 1) {
        Some(abroad) => abroad.click(),
        None => wrong.push("нет карточки «В другой стране»".to_owned()),
    }
    sleep(50).await;
    // Армения из списка стран — армянская фирма: отмечается «В Армении».
    if let Some(countries) = select(&host, t(lang, Key::ClientCountry), 0) {
        pick(&countries, "AM");
    }
    sleep(50).await;
    if !checked_card(&host).is_some_and(|text| text.contains(t(lang, Key::ClientInArmenia))) {
        wrong.push(format!("Армения из списка не отметила «В Армении»: {:?}", checked_card(&host)));
    }
    if let Some(abroad) = choice_card(&host, 1) {
        abroad.click();
    }
    sleep(50).await;
    match select(&host, t(lang, Key::ClientCountry), 0) {
        Some(countries) => pick(&countries, "US"),
        None => wrong.push("у фирмы другой страны не спрошена страна".to_owned()),
    }
    sleep(50).await;
    if field(&host, t(lang, Key::ProfileTin), 0).is_some() {
        wrong.push("у американской фирмы спрошен ՀՎՀՀ".to_owned());
    }
    match field(&host, t(lang, Key::ClientTaxId), 0) {
        Some(input) => type_into(&input, "EIN 12-3456789"),
        None => wrong.push("у американской фирмы не спрошен налоговый номер".to_owned()),
    }
    press(t(lang, Key::ClientNextDefaults), &mut wrong);
    sleep(50).await;
    press(t(lang, Key::ProfileSave), &mut wrong);
    if !wait_for(&host, "Acme Inc.").await {
        wrong.push("второй клиент не появился".to_owned());
    }
    if !wait_for(&host, &fill(t(lang, Key::TaxIdShort), &["EIN 12-3456789"])).await {
        wrong.push("налогового номера нет в списке".to_owned());
    }
    let both = vec!["Acme Inc.".to_owned(), name.to_owned()];
    if client_names(&host) != both {
        wrong.push(format!("порядок списка: {:?}", client_names(&host)));
    }

    let search = field(&host, t(lang, Key::ClientSearch), 0);
    for (query, expected) in [("семен", vec![name.to_owned()]), ("zzz", Vec::new()), ("", both.clone())] {
        match &search {
            Some(input) => type_into(input, query),
            None => wrong.push("нет поиска".to_owned()),
        }
        sleep(50).await;
        if client_names(&host) != expected {
            wrong.push(format!("поиск «{query}»: {:?}", client_names(&host)));
        }
        let none = host.text_content().unwrap_or_default().contains(t(lang, Key::ClientsNotFound));
        if none != expected.is_empty() {
            wrong.push(format!("поиск «{query}»: «никого не нашлось» — {none}"));
        }
    }

    // Удаляют из формы клиента: строка списка открывает её.
    match client_entry(&host, "Acme Inc.") {
        Some(entry) => entry.click(),
        None => wrong.push("нет строки Acme Inc.".to_owned()),
    }
    sleep(50).await;
    press(t(lang, Key::ClientDelete), &mut wrong);
    if !wait_for(&host, &fill(t(lang, Key::ClientDeleted), &["Acme Inc."])).await {
        wrong.push("удаление не подтвердилось".to_owned());
    }
    if client_names(&host) != [name] {
        wrong.push(format!("удалённый в списке: {:?}", client_names(&host)));
    }
    press(t(lang, Key::ClientRestore), &mut wrong);
    if !wait_for(&host, &fill(t(lang, Key::ClientRestored), &["Acme Inc."])).await {
        wrong.push("восстановление из сообщения не подтвердилось".to_owned());
    }
    if client_names(&host) != both {
        wrong.push(format!("после восстановления: {:?}", client_names(&host)));
    }
    if let Some(entry) = client_entry(&host, "Acme Inc.") {
        entry.click();
    }
    sleep(50).await;
    press(t(lang, Key::ClientDelete), &mut wrong);
    let toggle = fill(t(lang, Key::ClientsDeleted), &["1"]);
    if !wait_for(&host, &toggle).await {
        wrong.push("нет кнопки удалённых".to_owned());
    }
    press(&toggle, &mut wrong);
    sleep(50).await;
    match row_button(&host, "ul.clients.deleted", "Acme Inc.", t(lang, Key::ClientRestore)) {
        Some(restore) => restore.click(),
        None => wrong.push("удалённого нет в списке удалённых".to_owned()),
    }
    // Список меняется, когда запись легла на диск (`durability: "strict"`):
    // ждать слова о восстановлении, а не паузы — на диске CI 100 мс не
    // хватило (2026-10-15).
    if !wait_for(&host, &fill(t(lang, Key::ClientRestored), &["Acme Inc."])).await {
        wrong.push("восстановление из списка удалённых не подтвердилось".to_owned());
    }
    if client_names(&host) != both {
        wrong.push(format!("после восстановления из списка: {:?}", client_names(&host)));
    }

    match client_entry(&host, name) {
        Some(entry) => entry.click(),
        None => wrong.push(format!("нет строки «{name}»")),
    }
    sleep(50).await;
    match field(&host, t(lang, Key::ClientName), 0) {
        Some(input) if input.value() == name => {}
        other => wrong.push(format!("форма правки без названия: {:?}", other.map(|i| i.value()))),
    }
    if let Some(input) = field(&host, t(lang, Key::ClientPaymentDays), 0) {
        type_into(&input, "30");
    }
    press(t(lang, Key::ProfileSave), &mut wrong);
    if !wait_for(&host, &fill(t(lang, Key::ClientPaymentTerm), &["30\u{A0}дней"])).await {
        wrong.push("правка не видна в списке".to_owned());
    }

    match stored_clients().await {
        Ok(clients) => {
            let alive: Vec<&Client> = clients.iter().filter(|c| !c.is_deleted()).collect();
            let semyon = alive.iter().find(|c| c.data().name.as_str() == name);
            match semyon.map(|c| c.data()) {
                Some(data)
                    if alive.len() == 2
                        && data.country == Some(Country::ARMENIA)
                        && data.tin.as_ref().map(|tin| tin.as_str()) == Some("00054321")
                        && data.currency == Some(Currency::Usd)
                        && data.lang == Some(Lang::En)
                        && data.payment_days == Some(30)
                        && data.tax_id.is_empty() => {}
                other => wrong.push(format!("в базе не то: живых {}, {other:?}", alive.len())),
            }
            let acme = alive.iter().find(|c| c.data().name.as_str() == "Acme Inc.");
            match acme.map(|c| (c.data().tin.is_none(), c.data().tax_id.as_str())) {
                Some((true, "EIN 12-3456789")) => {}
                other => wrong.push(format!("Acme в базе: {other:?}")),
            }
        }
        Err(error) => wrong.push(format!("клиенты не читаются: {error:?}")),
    }

    drop(mounted);
    host.remove();
    let (host, mounted) = mount_at("#/clients");
    if !wait_for(&host, name).await || !wait_for(&host, "Acme Inc.").await {
        wrong.push("после нового открытия клиентов не видно".to_owned());
    }
    drop(mounted);
    host.remove();
    if let Some(storage) = web_sys::window().and_then(|window| window.local_storage().ok().flatten())
    {
        let _ = storage.remove_item("kvartik.lang");
    }
    assert!(wrong.is_empty(), "клиенты: {wrong:#?}");
}

/// Восстановление из копии меняет базу в обход разделов окна, и раздел
/// клиентов обязан перечитать её, как профиль: иначе после восстановления
/// на новом устройстве окно показывает пустой список. Копия — образец версии
/// 0.0.14: два живых клиента и один удалённый.
#[wasm_bindgen_test]
async fn restoring_a_backup_shows_its_clients() {
    crate::store::delete_database(crate::store::DB_NAME).await;
    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let (host, mounted) = mount_at("#/clients");
    let mut wrong = Vec::new();
    if !wait_for(&host, t(lang, Key::ClientsAbout)).await {
        wrong.push("пустая база — а о клиентах ни слова".to_owned());
    }
    if !wait_for(&host, t(lang, Key::BackupChooseFile)).await {
        wrong.push("раздела копии нет".to_owned());
    }
    match host
        .query_selector(r#"input[type="file"][accept=".json,application/json"]"#)
        .ok()
        .flatten()
    {
        Some(input) => choose_file(
            input.unchecked_ref(),
            "kvartik-2026-10-13.json",
            include_str!("../../../core/tests/backups/v3/kvartik-2026-10-13.json"),
        ),
        None => wrong.push("нет поля выбора копии".to_owned()),
    }
    if !wait_for(&host, t(lang, Key::BackupFilePassword)).await {
        wrong.push("копия не открылась".to_owned());
    }
    match field(&host, t(lang, Key::BackupFilePassword), 0) {
        Some(password) => type_into(&password, "Պահուստ копия sample 0.0.14"),
        None => wrong.push("нет поля пароля копии".to_owned()),
    }
    match button(&host, &[t(lang, Key::BackupRestore)]) {
        Some(restore) => restore.click(),
        None => wrong.push("нет кнопки «Восстановить»".to_owned()),
    }
    let done = t(lang, Key::BackupRestored).split("{}").next().unwrap_or_default();
    if !wait_for(&host, done).await {
        wrong.push("восстановление не завершилось".to_owned());
    }
    if !wait_for(&host, "Acme Inc.").await {
        wrong.push("после восстановления раздел клиентов не показал клиентов копии".to_owned());
    }
    let alive = vec!["Acme Inc.".to_owned(), "«Արարատ» ՍՊԸ".to_owned()];
    if client_names(&host) != alive {
        wrong.push(format!("живые клиенты копии: {:?}", client_names(&host)));
    }
    if !host
        .text_content()
        .is_some_and(|text| text.contains(&fill(t(lang, Key::ClientsDeleted), &["1"])))
    {
        wrong.push("удалённый клиент копии не в удалённых".to_owned());
    }
    drop(mounted);
    host.remove();
    if let Some(storage) = web_sys::window().and_then(|window| window.local_storage().ok().flatten())
    {
        // И день данных: образец копии приносит прошедшие дни пробного
        // периода, и устройство их помнит (задача 99) — следующим тестам
        // нужен свой пробный период.
        for name in ["kvartik.lang", "kvartik.backup_key", "kvartik.last_backup", "kvartik.trial_data"] {
            let _ = storage.remove_item(name);
        }
    }
    // База с записями образца — тоже: по их моментам пробный период
    // следующих тестов кончился бы (задача 99).
    crate::store::delete_database(crate::store::DB_NAME).await;
    assert!(wrong.is_empty(), "восстановление клиентов: {wrong:#?}");
}

/// Клиенты из базы программы, включая удалённых.
async fn stored_clients() -> Result<Vec<Client>, StoreError> {
    let store = Store::open(|_| {}).await?;
    Ok(store.read()?.list::<Client>().await?.records)
}

/// Видна ли кнопка с подписью `label`.
fn shown_button(host: &HtmlElement, label: &str) -> bool {
    button(host, &[label]).is_some_and(|found| found.offset_width() > 0)
}

/// Восстановление не путается с новой копией (полный прогон 2026-11-10,
/// п. 12): в пустой базе на экране копии — только восстановление, без
/// «Сделать копию»; с данными она есть, а пока выбран файл копии и спрошен
/// его пароль — снова нет: два набора паролей подряд путали пароль копии
/// с паролем копий. «Отмена» её возвращает.
#[wasm_bindgen_test]
async fn a_chosen_backup_hides_making_one() {
    crate::store::delete_database(crate::store::DB_NAME).await;
    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let make = t(lang, Key::BackupMake);
    let mut wrong = Vec::new();

    let (host, mounted) = mount_at("#/backup");
    if !wait_for(&host, t(lang, Key::BackupChooseFile)).await {
        wrong.push("раздела копии нет".to_owned());
    }
    sleep(300).await;
    if shown_button(&host, make) {
        wrong.push("в пустой базе — «Сделать копию»".to_owned());
    }
    drop(mounted);
    host.remove();

    documents::fresh_base_with_client().await;
    let (host, mounted) = mount_at("#/backup");
    for _ in 0..100 {
        if shown_button(&host, make) {
            break;
        }
        sleep(50).await;
    }
    if !shown_button(&host, make) {
        wrong.push("с данными нет «Сделать копию»".to_owned());
    }
    let input: Option<HtmlInputElement> = host
        .query_selector(r#"input[type="file"][accept=".json,application/json"]"#)
        .ok()
        .flatten()
        .map(JsCast::unchecked_into);
    match &input {
        Some(input) => choose_file(
            input,
            "kvartik-2026-10-12.json",
            include_str!("../../../core/tests/backups/v2/kvartik-2026-10-12.json"),
        ),
        None => wrong.push("нет поля выбора копии".to_owned()),
    }
    if !wait_for(&host, t(lang, Key::BackupRestore)).await {
        wrong.push("копия не открылась".to_owned());
    }
    sleep(100).await;
    if shown_button(&host, make) {
        wrong.push("пока выбран файл копии — «Сделать копию»".to_owned());
    }
    // Видимый текст: спрятанная форма остаётся в разметке.
    if host.inner_text().contains(t(lang, Key::BackupNewPassword)) {
        wrong.push("пока выбран файл копии — «Придумайте пароль копий»".to_owned());
    }
    match button(&host, &[t(lang, Key::Cancel)]) {
        Some(cancel) => cancel.click(),
        None => wrong.push("нет «Отмены» у пароля копии".to_owned()),
    }
    sleep(200).await;
    if !shown_button(&host, make) {
        wrong.push("после «Отмены» «Сделать копию» не вернулась".to_owned());
    }
    drop(mounted);
    host.remove();
    crate::platform::set_hash("");
    documents::forget_device();
    assert!(wrong.is_empty(), "восстановление и новая копия: {wrong:#?}");
}

/// Номер клиента в его строке — отдельным элементом `.tax-number`, который
/// стили не переносят (полный прогон 2026-11-10, п. 19): налоговый номер
/// «12-3456789» на телефоне рвался после дефиса. Подпись — рядом, по шаблону
/// языка.
#[wasm_bindgen_test]
async fn a_client_number_is_kept_whole() {
    crate::store::delete_database(crate::store::DB_NAME).await;
    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let mut data = documents::acme();
    data.tax_id = documents::line("12-3456789");
    let client = Client::create(data, crate::platform::clock::now()).expect("годный клиент");
    let mut writes = Writes::default();
    writes.insert(&client);
    documents::commit(writes).await;
    let (host, mounted) = mount_at("#/clients");
    let mut wrong = Vec::new();
    if !wait_for(&host, "Acme Inc.").await {
        wrong.push("клиента нет в списке".to_owned());
    }
    match host.query_selector("#clients .tax-number").ok().flatten() {
        Some(number) if number.text_content().as_deref() == Some("12-3456789") => {
            let fact = number.parent_element().and_then(|fact| fact.text_content()).unwrap_or_default();
            if fact != fill(t(lang, Key::TaxIdShort), &["12-3456789"]) {
                wrong.push(format!("сведение о номере: «{fact}»"));
            }
        }
        other => wrong.push(format!("номер не отдельным элементом: {:?}", other.and_then(|e| e.text_content()))),
    }
    drop(mounted);
    host.remove();
    crate::platform::set_hash("");
    documents::forget_device();
    assert!(wrong.is_empty(), "номер клиента: {wrong:#?}");
}

/// «К содержимому» (полный прогон 2026-11-10, п. 23): первая нажимаемая
/// ссылка окна — раньше десяти пунктов меню; нажатая, она ставит фокус на
/// заголовок открытого экрана, а адрес — экран — не меняет.
#[wasm_bindgen_test]
async fn the_skip_link_leads_past_the_menu() {
    crate::store::delete_database(crate::store::DB_NAME).await;
    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let (host, mounted) = mount_at("#/clients");
    let mut wrong = Vec::new();
    if !wait_for(&host, t(lang, Key::ClientsAbout)).await {
        wrong.push("экран клиентов не открылся".to_owned());
    }
    match host.query_selector(".app a[href], .app button").ok().flatten() {
        Some(first) if first.text_content().as_deref().map(str::trim) == Some(t(lang, Key::SkipToContent)) => {
            first.unchecked_into::<HtmlElement>().click();
        }
        other => wrong.push(format!(
            "первое нажимаемое — не «К содержимому»: {:?}",
            other.and_then(|element| element.text_content())
        )),
    }
    let mut focused = false;
    for _ in 0..40 {
        focused = document()
            .active_element()
            .is_some_and(|element| element.tag_name() == "H1" && element.closest("#clients").ok().flatten().is_some());
        if focused {
            break;
        }
        sleep(50).await;
    }
    if !focused {
        wrong.push("фокус не на заголовке экрана клиентов".to_owned());
    }
    if crate::platform::hash() != "#/clients" {
        wrong.push(format!("адрес сменился: {}", crate::platform::hash()));
    }
    drop(mounted);
    host.remove();
    crate::platform::set_hash("");
    documents::forget_device();
    assert!(wrong.is_empty(), "«К содержимому»: {wrong:#?}");
}
