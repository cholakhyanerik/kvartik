//! Браузерные тесты окна: всё окно (`App`) в настоящем DOM headless-браузера
//! (`wasm-bindgen-test`). Как запускать — Правило 2 в CLAUDE.md.
//!
//! Окно открывает базу программы (`store::DB_NAME`), а не пробную: в браузере
//! тестов она своя и после прогона пропадает вместе с профилем браузера.
//! Поэтому тесты не рассчитывают на пустую базу — профиль, сохранённый одним
//! тестом, другой может застать.

use js_sys::Promise;
use kvartik_core::format::Stored;
use kvartik_core::i18n::{ALL_KEYS, Key, Lang, t};
use kvartik_core::model::profile::Profile;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};
use web_sys::{Document, Element, HtmlElement, HtmlInputElement};

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
/// её списке есть строка: их подписи тоже должны перевестись.
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
    for add in [Key::AccountAdd, Key::ActivityAdd, Key::StopAdd] {
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
        for key in [Key::BackupTitle, Key::ProfileAccounts, Key::StopFrom] {
            if !text.contains(t(lang, key)) {
                wrong.push(format!("{lang:?}: нет {key:?} на этом языке"));
            }
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
