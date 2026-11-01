//! Браузерные тесты новой версии (задача 67). Service worker есть только
//! в выпускной сборке, и о новой версии браузер здесь не сообщит: за него
//! говорит тест (`Program::pretend`), а окно рисуется со своей плашкой,
//! с формой-заглушкой и со стилями сайта. Настоящую новую версию — service
//! worker, его кэш и включение само — проверяет `tools/browser/check.mjs` на
//! собранном сайте: перезагрузка здесь перезагрузила бы страницу тестов.
//! Когда включать, решает чистая функция — её тест рядом с ней
//! (`ui/pwa.rs`).

use kvartik_core::i18n::{Key, Lang, t};
use kvartik_core::release::{Notes, Version};
use leptos::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::wasm_bindgen_test;
use web_sys::{Element, HtmlElement, KeyboardEvent, KeyboardEventInit};

use super::{button, document, leftovers, sleep, wait_for};
use crate::platform::pwa::Event;
use crate::store::device::Device;
use crate::ui::form::Forms;
use crate::ui::pwa::{Program, UpdateDialog, UpdateNotice};

/// Человек видел версию 0.0.0: заметка о ней окну не нужна.
const SEEN: Version = Version::new(0, 0, 0);

/// Заметки: версия новее работающей — её окно показывать не должно, она ещё
/// не работает, — сама работающая с коротким пунктом на каждом языке, четыре
/// версии старше — по два длинных пункта, чтобы список не поместился на
/// экран, — и `SEEN`, о которой человек уже знает.
fn notes() -> Notes {
    let long = |text: &str| {
        let item = format!("{text} ").repeat(8);
        format!(r#"["{0}", "{0}"]"#, item.trim_end())
    };
    let mut entries = vec![
        format!(
            r#"{{"version": "{}.0.0", "date": "2026-10-20", "hy": ["Ապագա կետ"], "ru": ["Будущий пункт"], "en": ["A future item"]}}"#,
            u32::MAX
        ),
        format!(
            r#"{{"version": "{}", "date": "2026-10-19", "hy": ["Նոր կետ"], "ru": ["Новый пункт"], "en": ["A new item"]}}"#,
            Version::CURRENT
        ),
    ];
    for patch in (1..=4).rev() {
        entries.push(format!(
            r#"{{"version": "0.0.{patch}", "date": "2026-10-19", "hy": {}, "ru": {}, "en": {}}}"#,
            long("Երկար կետ, որ ցանկը չտեղավորվի էկրանին։"),
            long("Длинный пункт, чтобы список не поместился на экран."),
            long("A long item so that the list does not fit on the screen."),
        ));
    }
    entries.push(format!(
        r#"{{"version": "{SEEN}", "date": "2026-10-18", "hy": ["Հին կետ"], "ru": ["Старый пункт"], "en": ["An old item"]}}"#
    ));
    Notes::parse(&format!(r#"{{"notes": [{}]}}"#, entries.join(", "))).expect("заметки читаются")
}

/// Заметки пришедшей сборки с версией `version` на самом верху.
fn arrived(version: &str) -> Notes {
    let text = format!(r#"{{"notes": [{{"version": "{version}", "date": "2026-10-18", "ru": ["Пункт"]}}]}}"#);
    Notes::parse(&text).expect("заметки читаются")
}

/// Версия новее работающей.
fn newer() -> Notes {
    arrived(&format!("{}.0.0", u32::MAX))
}

/// Форма-заглушка: отмечается в открытых формах, как настоящая.
#[component]
fn FormStub(forms: Forms) -> impl IntoView {
    forms.hold();
    view! {
        <form class="stub">
            <input type="text" />
        </form>
    }
}

/// Поле вне формы: считается открытой формой, пока в нём что-то начато.
#[component]
fn FieldStub(forms: Forms, started: RwSignal<bool>) -> impl IntoView {
    forms.hold_while(move || started.get());
}

fn find(host: &HtmlElement, selector: &str) -> Option<Element> {
    host.query_selector(selector).ok().flatten()
}

fn mount_style() -> Element {
    let document = document();
    let style = document.create_element("style").expect("создаётся");
    style.set_text_content(Some(concat!(
        include_str!("../../../style/tokens.css"),
        include_str!("../../../style/base.css"),
    )));
    document.body().expect("есть <body>").append_child(&style).expect("вставляется");
    style
}

fn mount_host() -> HtmlElement {
    let document = document();
    let host: HtmlElement = document.create_element("div").expect("создаётся").unchecked_into();
    document.body().expect("есть <body>").append_child(&host).expect("вставляется");
    host
}

/// Пришедшая версия новее работающей ждёт открытой формы и поля, где что-то
/// начато: над экраном слова без кнопки, окна нет, программа работает.
/// Закрыли — слов больше нет: дальше версия включается сама, и это видно
/// только на собранном сайте. Включается — слова «Обновляем…». Сборка не
/// новее работающей и заметки, которые не прочитались, не говорят ничего:
/// что пришло, неизвестно.
#[wasm_bindgen_test]
async fn a_new_version_waits_for_forms_and_says_so() {
    let host = mount_host();
    let (lang, _) = signal(Lang::Ru);
    let program = Program::new();
    let forms = Forms::new();
    let open = RwSignal::new(true);
    let started = RwSignal::new(false);
    let mounted = leptos::mount::mount_to(host.clone(), move || {
        view! {
            <div class="under" inert=move || program.blocks()>
                <UpdateNotice lang program forms />
                {move || open.get().then(|| view! { <FormStub forms /> })}
                <FieldStub forms started />
            </div>
        }
    });
    let under = find(&host, ".under").expect("окно");
    let ru = |key| t(Lang::Ru, key);
    let waits = ru(Key::UpdateAfterForm);
    let says = |text: &str| host.text_content().unwrap_or_default().contains(text);
    let mut wrong = Vec::new();

    sleep(50).await;
    if says(waits) {
        wrong.push("новой версии нет, а слова о ней есть".to_owned());
    }

    // Пришла, пока открыта форма: только слова над экраном, без кнопки.
    program.pretend(Event::UpdateReady, Some(newer()));
    if !wait_for(&host, waits).await {
        wrong.push("с открытой формой нет слов «откроется, когда закроете форму»".to_owned());
    }
    if under.has_attribute("inert") || host.query_selector("[role=status] button").ok().flatten().is_some() {
        wrong.push("с открытой формой программа неактивна или у слов есть кнопка".to_owned());
    }

    // Форму закрыли, а начатое поле ещё держит версию.
    started.set(true);
    sleep(50).await;
    open.set(false);
    sleep(50).await;
    if !says(waits) {
        wrong.push("поле с начатым не держит новую версию".to_owned());
    }
    started.set(false);
    sleep(50).await;
    if says(waits) {
        wrong.push("всё закрыто, а слова остались".to_owned());
    }

    // Снова форма — и то, что слова говорят, зависит от пришедшей версии.
    open.set(true);
    for (what, event, notes, shown) in [
        ("включённая в другой вкладке", Event::UpdatedElsewhere, Some(newer()), true),
        ("сборка не новее работающей", Event::UpdateReady, Some(arrived(&Version::CURRENT.to_string())), false),
        ("заметки не прочитались", Event::UpdateReady, None, false),
    ] {
        program.pretend(event, notes);
        sleep(50).await;
        if says(waits) != shown {
            wrong.push(format!("{what}: слова {}", if shown { "пропали" } else { "есть" }));
        }
    }

    program.pretend_applying();
    if !wait_for(&host, ru(Key::UpdateApplying)).await {
        wrong.push("версия включается, а слов «Обновляем…» нет".to_owned());
    }
    if says(waits) || under.has_attribute("inert") {
        wrong.push("версия включается, а над экраном не то".to_owned());
    }

    drop(mounted);
    host.remove();
    assert!(wrong.is_empty(), "новая версия: {wrong:#?}");
}

/// После обновления — окно «Что нового» поверх всего: программа под ним
/// неактивна, фокус на его заголовке, в списке — только версии новее той,
/// что человек видел, и не новее работающей, на языке окна, и всё окно — на
/// экране: длинный список прокручивается внутри, а «Понятно» видна.
/// Посторонняя весть окно не пересобирает. Закрывают его «Понятно» и Escape,
/// и о работающей версии устройство запоминает, что человек знает.
#[wasm_bindgen_test]
async fn the_whats_new_window_comes_after_the_update() {
    let style = mount_style();
    let host = mount_host();
    let (lang, set_lang) = signal(Lang::Ru);
    let program = Program::new();
    let forms = Forms::new();
    let mounted = leptos::mount::mount_to(host.clone(), move || {
        view! {
            <div class="under" inert=move || program.blocks()>
                <UpdateNotice lang program forms />
            </div>
            <UpdateDialog lang program />
        }
    });
    let under = find(&host, ".under").expect("окно");
    let ru = |key| t(Lang::Ru, key);
    let mut wrong = Vec::new();

    sleep(50).await;
    if find(&host, "[role=dialog]").is_some() || under.has_attribute("inert") {
        wrong.push("обновления не было, а окно стоит".to_owned());
    }

    program.pretend_updated(SEEN, notes());
    if !wait_for(&host, ru(Key::UpdateTitle)).await {
        wrong.push("обновление было, а окна нет".to_owned());
    }
    sleep(50).await;
    let dialog = find(&host, "[role=dialog]").and_then(|d| d.text_content()).unwrap_or_default();
    for (needed, what) in [
        (ru(Key::UpdateWhatsNew), "«Что нового»"),
        ("Новый пункт", "пункта работающей версии"),
        ("Длинный пункт", "пункта версии старше неё"),
        (ru(Key::UpdateGotIt), "«Понятно»"),
        (ru(Key::UpdateKeepsData), "слов о данных"),
    ] {
        if !dialog.contains(needed) {
            wrong.push(format!("в окне нет {what}: {dialog}"));
        }
    }
    for (needless, what) in [("Старый пункт", "версии, которую человек видел"), ("Будущий пункт", "версии новее работающей")] {
        if dialog.contains(needless) {
            wrong.push(format!("в окне — заметка {what}"));
        }
    }
    if !under.has_attribute("inert") {
        wrong.push("программа под окном не неактивна".to_owned());
    }
    let document = document();
    let focused = document.active_element().map(|element| element.id());
    if focused.as_deref() != Some("update-title") {
        wrong.push(format!("фокус не на заголовке окна, а на {focused:?}"));
    }
    let window = web_sys::window().expect("есть окно");
    let (width, height) = (
        window.inner_width().ok().and_then(|w| w.as_f64()).unwrap_or(0.0),
        window.inner_height().ok().and_then(|h| h.as_f64()).unwrap_or(0.0),
    );
    // Карточка и кнопка меряются от слоя окна: он закреплён во весь экран,
    // а сама карточка не позиционирована.
    if let (Some(card), Some(close)) = (find(&host, ".update-card"), button(&host, &[ru(Key::UpdateGotIt)])) {
        let card: HtmlElement = card.unchecked_into();
        let (left, right) = (card.offset_left(), card.offset_left() + card.offset_width());
        let bottom = close.offset_top() + close.offset_height();
        if left < 0 || f64::from(right) > width || f64::from(bottom) > height || close.offset_height() < 40 {
            wrong.push(format!(
                "окно не на экране {width}×{height}: карточка {left}–{right}, кнопка снизу {bottom} высотой {}",
                close.offset_height()
            ));
        }
    } else {
        wrong.push("нет карточки окна или кнопки «Понятно»".to_owned());
    }

    // Весть браузера окна не пересобирает: фокус, ушедший на кнопку, на ней
    // и остаётся.
    if let Some(close) = button(&host, &[ru(Key::UpdateGotIt)]) {
        let _ = close.focus();
        program.pretend(Event::UpdateReady, Some(notes()));
        sleep(50).await;
        let focused = document.active_element().and_then(|element| element.text_content());
        if focused.as_deref().map(str::trim) != Some(ru(Key::UpdateGotIt)) {
            wrong.push(format!("весть браузера — фокус ушёл с «Понятно» на {focused:?}"));
        }
    }

    // Язык окна сменили — окно на новом языке целиком.
    for (next, item) in [(Lang::En, "A new item"), (Lang::Am, "Նոր կետ")] {
        set_lang.set(next);
        sleep(50).await;
        let text = find(&host, "[role=dialog]").and_then(|d| d.text_content()).unwrap_or_default();
        if !text.contains(t(next, Key::UpdateTitle)) || !text.contains(item) {
            wrong.push(format!("{next:?}: окно не перевелось — {text}"));
        }
        let left = leftovers(&text, next);
        if !left.is_empty() {
            wrong.push(format!("{next:?}: строки других языков в окне — {left:?}"));
        }
    }
    set_lang.set(Lang::Ru);
    sleep(50).await;

    // «Понятно» закрывает окно, и устройство запоминает работающую версию.
    Device::new().set_seen_version(SEEN);
    match button(&host, &[ru(Key::UpdateGotIt)]) {
        Some(close) => close.click(),
        None => wrong.push("нет кнопки «Понятно»".to_owned()),
    }
    sleep(50).await;
    if find(&host, "[role=dialog]").is_some() || under.has_attribute("inert") {
        wrong.push("«Понятно» не закрыла окно".to_owned());
    }
    if Device::new().seen_version() != Some(Version::CURRENT) {
        wrong.push(format!("после «Понятно» на устройстве {:?}", Device::new().seen_version()));
    }

    // Escape — то же самое, с фокусом на заголовке окна.
    program.pretend_updated(SEEN, notes());
    if !wait_for(&host, ru(Key::UpdateTitle)).await {
        wrong.push("окно не встало второй раз".to_owned());
    }
    sleep(50).await;
    let init = KeyboardEventInit::new();
    init.set_key("Escape");
    init.set_bubbles(true);
    let escape = KeyboardEvent::new_with_keyboard_event_init_dict("keydown", &init).expect("создаётся");
    if let Some(focused) = document.active_element() {
        focused.dispatch_event(&escape).expect("доходит");
    }
    sleep(50).await;
    if find(&host, "[role=dialog]").is_some() {
        wrong.push("Escape не закрыл окно".to_owned());
    }

    drop(mounted);
    host.remove();
    style.remove();
    assert!(wrong.is_empty(), "окно «Что нового»: {wrong:#?}");
}
