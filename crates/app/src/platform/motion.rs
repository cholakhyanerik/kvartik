//! Движение (задача 86): что говорит система о движении и переход View
//! Transitions между экранами.
//!
//! Привязки — свои: `Document::start_view_transition` и `ViewTransition`
//! в `web-sys` 0.3.107 только под флагом `--cfg=web_sys_unstable_apis`
//! (`gen_Document.rs`, `gen_ViewTransition.rs`, сверено 2026-11-06).

use std::cell::Cell;
use std::rc::Rc;

use js_sys::Promise;
use wasm_bindgen::prelude::{Closure, wasm_bindgen};
use wasm_bindgen::{JsCast, JsValue};

#[wasm_bindgen]
extern "C" {
    /// `document` с переходом View Transitions.
    #[wasm_bindgen(extends = web_sys::Document)]
    type Transitions;

    /// `document.startViewTransition(обновление)`. Нет метода — исключение
    /// `TypeError`, и привязка возвращает его ошибкой.
    #[wasm_bindgen(method, catch, js_name = startViewTransition)]
    fn start_view_transition(this: &Transitions, update: &JsValue) -> Result<ViewTransition, JsValue>;

    /// Начатый переход.
    type ViewTransition;

    /// Обещание «анимация началась»: прерванный переход его отвергает.
    #[wasm_bindgen(method, getter)]
    fn ready(this: &ViewTransition) -> Promise;
}

thread_local! {
    /// Обработчик, который глотает отказ `ready`: один на все переходы.
    static IGNORE: Closure<dyn FnMut(JsValue)> = Closure::new(|_| {});
}

/// Движение разрешено: человек не просил систему его убрать
/// (`prefers-reduced-motion`). Не сказала — тоже нет: так у стилей
/// (`style/base.css`).
pub fn allowed() -> bool {
    web_sys::window()
        .and_then(|window| window.match_media("(prefers-reduced-motion: no-preference)").ok().flatten())
        .is_some_and(|query| query.matches())
}

/// Сменить страницу переходом: браузер снимает прежнюю, зовёт `update`,
/// ждёт его обещания и ведёт прежний снимок в новый — как, решает
/// оформление (`::view-transition-…`). Браузер переходов не знает — `update`
/// зовётся сразу. Переход, прерванный следующим, свой `update` всё равно
/// зовёт (по спецификации CSS View Transitions, «skip the view transition»),
/// а отказ его `ready` здесь глотается: иначе быстрый второй переход был бы
/// ошибкой в консоли.
pub fn transition(update: impl FnOnce() -> Promise + 'static) {
    let pending = Rc::new(Cell::new(Some(update)));
    let callback = {
        let pending = Rc::clone(&pending);
        Closure::once_into_js(move || match pending.take() {
            Some(update) => update(),
            None => Promise::resolve(&JsValue::UNDEFINED),
        })
    };
    let started = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| {
            document
                .unchecked_into::<Transitions>()
                .start_view_transition(&callback)
                .ok()
        });
    match started {
        Some(transition) => IGNORE.with(|ignore| {
            let _ = transition.ready().catch(ignore);
        }),
        None => {
            if let Some(update) = pending.take() {
                let _ = update();
            }
        }
    }
}
