//! Движение окна (задача 86): переходы экранов один за другим кончаются
//! последним экраном по адресу, а кнопка под мышью получает точку входа
//! и тягу магнита — касание пальцем их не трогает.

use kvartik_core::i18n::Lang;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::wasm_bindgen_test;
use web_sys::{EventTarget, HtmlElement, PointerEvent, PointerEventInit};

use super::{current_tabs, mount, shown_screens, sleep};
use crate::platform::set_hash;
use crate::store::device::Device;

/// Язык окна, запомненный тестом, — прочь: следующий тест открывается
/// экраном первого запуска, как у нового человека.
fn forget_lang() {
    if let Some(storage) = web_sys::window().and_then(|window| window.local_storage().ok().flatten()) {
        let _ = storage.remove_item("kvartik.lang");
    }
}

/// Два перехода подряд, второй — пока первый не кончился (браузер прерывает
/// первый, а его смену разметки всё равно зовёт): окно встаёт на последний
/// экран по адресу и на нём остаётся, и вкладка меню — его.
#[wasm_bindgen_test]
async fn quick_steps_end_on_the_last_screen() {
    Device::new().set_ui_lang(Lang::Ru);
    let (host, mounted) = mount();
    for _ in 0..100 {
        if shown_screens(&host) == ["home"] {
            break;
        }
        sleep(10).await;
    }
    set_hash("#/clients");
    set_hash("#/ledger");
    let mut seen = Vec::new();
    for _ in 0..100 {
        seen = shown_screens(&host);
        if seen == ["ledger"] {
            break;
        }
        sleep(20).await;
    }
    // Переход кончился — экран тот же.
    sleep(700).await;
    let after = (shown_screens(&host), current_tabs(&host));
    drop(mounted);
    host.remove();
    set_hash("");
    forget_lang();
    assert_eq!(seen, ["ledger"], "два перехода подряд: видны {seen:?}");
    assert_eq!(
        after,
        (vec!["ledger".to_owned()], vec!["#/ledger".to_owned()]),
        "после перехода"
    );
}

/// Событие указателя `kind` с местом `x`, `y`: мышь или палец (`pointer`).
fn fire(target: &HtmlElement, kind: &str, pointer: &str, (x, y): (i32, i32), next: Option<&EventTarget>) {
    let init = PointerEventInit::new();
    init.set_bubbles(true);
    init.set_pointer_type(pointer);
    init.set_client_x(x);
    init.set_client_y(y);
    init.set_related_target(next);
    let event = PointerEvent::new_with_event_init_dict(kind, &init).expect("событие создаётся");
    target.dispatch_event(&event).expect("событие отправляется");
}

fn property(element: &HtmlElement, name: &str) -> String {
    element.style().get_property_value(name).unwrap_or_default()
}

/// Кнопка под мышью: где в неё вошли — переменными CSS на ней (оттуда
/// растекается заливка), при движении — тяга магнита к мыши, ушла мышь —
/// тяга в покой, а кружок у курсора над кнопкой — «нажимается». Палец
/// ничего этого не трогает: наклон и магнит под ним только мешали бы.
#[wasm_bindgen_test]
async fn the_mouse_leaves_its_point_on_buttons() {
    // Язык выбран — окно без экрана первого запуска, главный экран виден.
    Device::new().set_ui_lang(Lang::Ru);
    let (host, mounted) = mount();
    let mut found = None;
    for _ in 0..100 {
        found = host
            .query_selector_all("#home :is(button, .button)")
            .ok()
            .and_then(|all| {
                (0..all.length())
                    .filter_map(|at| all.item(at))
                    .map(|node| node.unchecked_into::<HtmlElement>())
                    .find(|button| button.get_bounding_client_rect().width() > 0.0)
            });
        if found.is_some() {
            break;
        }
        sleep(50).await;
    }
    let button = found.expect("на главном экране есть кнопка");
    if let Some(window) = web_sys::window() {
        window.scroll_to_with_x_and_y(0.0, 0.0);
    }
    let rect = button.get_bounding_client_rect();
    // Вход у левого края, по середине высоты.
    let at = ((rect.left() + 2.0) as i32, (rect.top() + rect.height() / 2.0) as i32);

    fire(&button, "pointerover", "touch", at, None);
    let touched = property(&button, "--pointer-x");
    fire(&button, "pointerover", "mouse", at, None);
    let entered = property(&button, "--pointer-x");
    fire(&button, "pointermove", "mouse", at, None);
    let pulled = property(&button, "--pull-x");
    let over = host
        .query_selector(".cursor")
        .ok()
        .flatten()
        .and_then(|cursor| cursor.get_attribute("data-over"));
    let body = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.body())
        .expect("есть <body>");
    fire(&button, "pointerout", "mouse", at, Some(&body));
    let rested = property(&button, "--pull-x");

    drop(mounted);
    host.remove();
    forget_lang();
    let mut wrong = Vec::new();
    if !touched.is_empty() {
        wrong.push(format!("палец оставил точку {touched}"));
    }
    if !entered.starts_with("0.0") {
        wrong.push(format!("вход у левого края — точка «{entered}»"));
    }
    if !pulled.starts_with("-0.9") && pulled != "-1.000" {
        wrong.push(format!("мышь у левого края — тяга «{pulled}», а не к ней"));
    }
    if over.as_deref() != Some("press") {
        wrong.push(format!("кружок над кнопкой — {over:?}"));
    }
    if rested != "0.000" {
        wrong.push(format!("мышь ушла — тяга «{rested}»"));
    }
    assert!(wrong.is_empty(), "кнопка под мышью: {wrong:#?}");
}
