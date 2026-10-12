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
use kvartik_core::model::client::Client;
use kvartik_core::model::country::Country;
use kvartik_core::model::money::Currency;
use kvartik_core::model::profile::Profile;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};
use web_sys::{Document, Element, HtmlElement, HtmlInputElement, HtmlSelectElement};

use super::App;
use crate::store::device::Device;
use crate::store::{Store, StoreError};

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
/// и убрать `<div>`.
fn mount() -> (HtmlElement, impl Sized) {
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
/// клиента открыта тоже: список стран — на языке окна и по его алфавиту.
#[wasm_bindgen_test]
async fn choosing_a_language_translates_the_whole_window() {
    let document = document();
    let html = document.document_element().expect("есть <html>");
    let page_lang = html.get_attribute("lang");
    let device = Device::new();
    device.set_ui_lang(Lang::Am);
    let (host, mounted) = mount();

    let mut wrong = Vec::new();
    // Разделы о данных появляются, когда открылась база.
    if !wait_for(&host, t(Lang::Am, Key::BackupTitle)).await {
        wrong.push("раздел копии не появился".to_owned());
    }
    match button(
        &host,
        &[t(Lang::Am, Key::ProfileFill), t(Lang::Am, Key::ProfileEdit)],
    ) {
        Some(open) => open.click(),
        None => wrong.push("нет кнопки профиля".to_owned()),
    }
    sleep(50).await;
    for add in [Key::AccountAdd, Key::ActivityAdd, Key::StopAdd, Key::ClientAdd] {
        match button(&host, &[t(Lang::Am, add)]) {
            Some(add) => add.click(),
            None => wrong.push(format!("нет кнопки {add:?}")),
        }
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
        for key in [Key::BackupTitle, Key::ProfileAccounts, Key::StopFrom, Key::ClientCountry] {
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
    if !wait_for(&host, t(lang, Key::ProfileFill)).await {
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
    if !wait_for(&host, "ИП Арам Петросян").await {
        wrong.push("после восстановления раздел профиля не показал профиль копии".to_owned());
    }
    drop(mounted);
    host.remove();
    if let Some(storage) = web_sys::window().and_then(|window| window.local_storage().ok().flatten())
    {
        for name in ["kvartik.lang", "kvartik.backup_key", "kvartik.last_backup"] {
            let _ = storage.remove_item(name);
        }
    }
    assert!(wrong.is_empty(), "восстановление: {wrong:#?}");
}

/// Профиль из базы программы — тот, что окно показывает.
async fn stored_profile() -> Result<Option<Profile>, StoreError> {
    let store = Store::open(|_| {}).await?;
    let profiles = store.read()?.all::<Profile>().await?;
    Ok(Profile::current(profiles))
}

/// Профиль сохраняется одной командой и читается обратно: ошибочный ՀՎՀՀ
/// не даёт сохранить и говорит об этом у поля, код, закрывающий налог
/// с оборота, предупреждает сразу при вводе, а сохранённое лежит в базе
/// и видно в окне после нового открытия.
#[wasm_bindgen_test]
async fn profile_is_saved_and_read_back() {
    // С пустой базы: профиль, оставленный другим тестом, принёс бы свои
    // строки списков.
    crate::store::delete_database(crate::store::DB_NAME).await;
    let lang = Lang::Ru;
    let device = Device::new();
    device.set_ui_lang(lang);
    let (host, mounted) = mount();
    let mut wrong = Vec::new();
    if !wait_for(&host, t(lang, Key::ProfileTitle)).await {
        wrong.push("раздела профиля нет".to_owned());
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

    match stored_profile().await {
        Ok(Some(profile)) => {
            let data = profile.data();
            if data.name.ru.as_str() != name
                || data.tin.as_ref().map(|tin| tin.as_str()) != Some("00054321")
                || data.activities.iter().map(ToString::to_string).collect::<Vec<_>>()
                    != ["70.22.0"]
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
    let (host, mounted) = mount();
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

/// Кнопка `label` у клиента `name` в списке `list` (CSS).
fn row_button(host: &HtmlElement, list: &str, name: &str, label: &str) -> Option<HtmlElement> {
    let items = host.query_selector_all(&format!("{list} li")).ok()?;
    let item = (0..items.length())
        .filter_map(|at| items.item(at))
        .map(|node| node.unchecked_into::<HtmlElement>())
        .find(|item| {
            item.query_selector(".client-name")
                .ok()
                .flatten()
                .and_then(|title| title.text_content())
                .as_deref()
                == Some(name)
        })?;
    button(&item, &[label])
}

/// Клиенты: форма не сохраняет без названия, с неверным ՀՎՀՀ и сроком
/// больше года и говорит об этом у поля; ՀՎՀՀ спрашивается только
/// у армянской фирмы; сохранённое видно в списке и лежит в базе; поиск
/// находит по части названия без учёта «ё»; удалённый уходит из списка
/// и возвращается — из сообщения об удалении и из списка удалённых; после
/// нового открытия клиенты на месте.
#[wasm_bindgen_test]
async fn clients_are_added_found_deleted_and_restored() {
    crate::store::delete_database(crate::store::DB_NAME).await;
    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let (host, mounted) = mount();
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
    press(t(lang, Key::ProfileSave), &mut wrong);
    sleep(50).await;
    let text = host.text_content().unwrap_or_default();
    if !text.contains(t(lang, Key::ClientNameMissing)) || !text.contains(t(lang, Key::ProfileFix)) {
        wrong.push("клиент без названия сохранился без слова".to_owned());
    }
    if field(&host, t(lang, Key::ProfileTin), 0).is_some() {
        wrong.push("ՀՎՀՀ спрошен, когда страна не выбрана".to_owned());
    }

    let name = "ООО Семён и партнёры";
    match field(&host, t(lang, Key::ClientName), 0) {
        Some(input) => type_into(&input, name),
        None => wrong.push("нет поля названия".to_owned()),
    }
    match select(&host, t(lang, Key::ClientCountry), 0) {
        Some(countries) => pick(&countries, "AM"),
        None => wrong.push("нет списка стран".to_owned()),
    }
    sleep(50).await;
    let tin = field(&host, t(lang, Key::ProfileTin), 0);
    match &tin {
        Some(input) => type_into(input, "0005432"),
        None => wrong.push("у армянской фирмы не спрошен ՀՎՀՀ".to_owned()),
    }
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
    if !text.contains(t(lang, Key::TinInvalid)) || !text.contains(days_problem) {
        wrong.push("семь цифр ՀՎՀՀ и срок 400 дней сохранились без слова".to_owned());
    }
    if let Some(input) = &tin {
        type_into(input, "00054321");
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
    if let Some(countries) = select(&host, t(lang, Key::ClientCountry), 0) {
        pick(&countries, "US");
    }
    sleep(50).await;
    if field(&host, t(lang, Key::ProfileTin), 0).is_some() {
        wrong.push("у американской фирмы спрошен ՀՎՀՀ".to_owned());
    }
    match field(&host, t(lang, Key::ClientTaxId), 0) {
        Some(input) => type_into(&input, "EIN 12-3456789"),
        None => wrong.push("у американской фирмы не спрошен налоговый номер".to_owned()),
    }
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

    let gone = "ul.clients:not(.deleted)";
    match row_button(&host, gone, "Acme Inc.", t(lang, Key::ClientDelete)) {
        Some(delete) => delete.click(),
        None => wrong.push("нет кнопки удаления".to_owned()),
    }
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
    if let Some(delete) = row_button(&host, gone, "Acme Inc.", t(lang, Key::ClientDelete)) {
        delete.click();
    }
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
    sleep(100).await;
    if client_names(&host) != both {
        wrong.push(format!("после восстановления из списка: {:?}", client_names(&host)));
    }

    match row_button(&host, gone, name, t(lang, Key::ClientEdit)) {
        Some(edit) => edit.click(),
        None => wrong.push("нет кнопки «Изменить»".to_owned()),
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
    let (host, mounted) = mount();
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
    let (host, mounted) = mount();
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
        for name in ["kvartik.lang", "kvartik.backup_key", "kvartik.last_backup"] {
            let _ = storage.remove_item(name);
        }
    }
    assert!(wrong.is_empty(), "восстановление клиентов: {wrong:#?}");
}

/// Клиенты из базы программы, включая удалённых.
async fn stored_clients() -> Result<Vec<Client>, StoreError> {
    let store = Store::open(|_| {}).await?;
    Ok(store.read()?.list::<Client>().await?.records)
}
