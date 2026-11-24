//! Буфер обмена (задача 24): число для расчёта — в поле формы в системе
//! ՊԵԿ, все числа — текстом бухгалтеру; код передачи — туда и обратно
//! (задачи 110 и 124).
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

    /// `navigator.clipboard.readText()`.
    #[wasm_bindgen(method, catch, js_name = readText)]
    fn read_text(this: &Board) -> Result<Promise, JsValue>;
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

/// Взять текст из буфера обмена (задача 124): код передачи, пересланный себе
/// мессенджером, — одним нажатием «Вставить», а не жестом в поле. Читать
/// буфер браузеры дают строже, чем писать (по браузеру: MDN,
/// `Clipboard.readText`, сверено 2026-11-24): Chromium спрашивает разрешение
/// `clipboard-read`, Firefox и Safari — только от свежего нажатия и ещё
/// кнопкой «Вставить» у самого места, — поэтому `readText` зовётся в самом
/// обработчике нажатия, до первого `await`, как `writeText`. `Err` — буфера
/// нет, человек отказал или в нём не текст: поле остаётся, и вставить в него
/// можно жестом.
pub fn paste() -> impl Future<Output = Result<String, ()>> + 'static {
    let started = board()
        .ok_or_else(|| JsValue::from_str("navigator.clipboard нет"))
        .and_then(|board| board.read_text());
    async move {
        let read = match started {
            Ok(promise) => promise.await,
            Err(error) => Err(error),
        };
        match read.map(|text| text.as_string()) {
            Ok(Some(text)) => Ok(text),
            Ok(None) => Err(()),
            Err(error) => {
                web_sys::console::error_2(&"Kvartik, буфер обмена:".into(), &error);
                Err(())
            }
        }
    }
}
