//! Браузерные тесты окна: всё окно (`App`) в настоящем DOM headless-браузера
//! (`wasm-bindgen-test`). Как запускать — Правило 2 в CLAUDE.md.

use js_sys::Promise;
use kvartik_core::i18n::{ALL_KEYS, Key, Lang, t};
use wasm_bindgen::JsCast;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};
use web_sys::{Document, Element, HtmlElement, HtmlInputElement};

use super::App;
use crate::store::device::Device;

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
/// открывается языком, выбранным раньше.
#[wasm_bindgen_test]
async fn choosing_a_language_translates_the_whole_window() {
    let document = document();
    let html = document.document_element().expect("есть <html>");
    let page_lang = html.get_attribute("lang");
    let device = Device::new();
    device.set_ui_lang(Lang::Am);
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

    // Разделы о данных появляются, когда открылась база.
    let opened = t(Lang::Am, Key::BackupTitle);
    for _ in 0..100 {
        if host.text_content().is_some_and(|text| text.contains(opened)) {
            break;
        }
        sleep(50).await;
    }
    let typed: Option<HtmlInputElement> = host
        .query_selector("input[type=password]")
        .ok()
        .flatten()
        .map(JsCast::unchecked_into);
    if let Some(input) = &typed {
        input.set_value("пароль на полпути");
    }
    let mut wrong = Vec::new();
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
        if !text.contains(t(lang, Key::BackupTitle)) {
            wrong.push(format!("{lang:?}: окно не на этом языке"));
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
