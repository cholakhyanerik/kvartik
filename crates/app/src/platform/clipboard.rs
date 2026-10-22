//! Буфер обмена (задача 24): число для расчёта — в поле формы в системе
//! ՊԵԿ, все числа — текстом бухгалтеру.
//!
//! `navigator.clipboard` есть только в защищённом контексте — по HTTPS и на
//! `localhost` (`[SecureContext]` по спецификации Clipboard API): на
//! `http://` по адресу сети его нет вовсе, как `crypto.subtle`
//! (CLAUDE.md, Правило 6). Писать в буфер Firefox и Safari дают только от
//! свежего нажатия человека, Chromium — от нажатия или с разрешением
//! `clipboard-write`; отказ — `NotAllowedError` (по браузеру: MDN, Clipboard
//! API, «Security considerations», и `Clipboard.writeText`; сверено
//! 2026-10-22). Поэтому `writeText` зовётся прямо в обработчике нажатия, до
//! первого `await`, как «Поделиться» (`platform::share`), а отказ окно
//! говорит словами: число на экране — текст, его выделяют и копируют сами.
//!
//! Привязки — свои, с `catch`: вызов на `undefined` бросил бы исключение
//! сквозь wasm, а привязка `web-sys` его не ловит.

use std::future::Future;

use js_sys::{Promise, Reflect};
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::{JsCast, JsValue};

#[wasm_bindgen]
extern "C" {
    /// `navigator.clipboard`.
    type Board;

    /// `navigator.clipboard.writeText(текст)`.
    #[wasm_bindgen(method, catch, js_name = writeText)]
    fn write_text(this: &Board, text: &str) -> Result<Promise, JsValue>;
}

/// Буфер обмена; `None` — его нет: адрес не защищённый или браузер старый.
fn board() -> Option<Board> {
    let navigator = web_sys::window()?.navigator();
    let board = Reflect::get(&navigator, &"clipboard".into()).ok()?;
    (!board.is_undefined() && !board.is_null()).then(|| board.unchecked_into())
}

/// Положить текст в буфер обмена: `Ok` — лёг, `Err` — буфера нет или
/// браузер отказал. Причину отказа человеку знать не нужно — ему нужно
/// скопировать самому; разработчику она — в консоли (наружу ничего не
/// уходит, Правило 13).
pub fn copy(text: &str) -> impl Future<Output = Result<(), ()>> + 'static {
    let started = board()
        .ok_or_else(|| JsValue::from_str("navigator.clipboard нет"))
        .and_then(|board| board.write_text(text));
    async move {
        let written = match started {
            Ok(promise) => promise.await.map(|_| ()),
            Err(error) => Err(error),
        };
        written.map_err(|error| web_sys::console::error_2(&"Kvartik, буфер обмена:".into(), &error))
    }
}
