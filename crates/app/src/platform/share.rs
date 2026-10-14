//! «Поделиться» (Web Share, задача 13): отдать файл системному меню
//! телефона или компьютера — PDF счёта в Telegram, WhatsApp или почту без
//! сохранения, а на iPhone ещё и «Сохранить в Файлы».
//!
//! Где меню есть (по исходникам Chromium `main` d6b17880fcaf, WebKit
//! `main` 1cc0fbfcd0fe и Firefox `main` 976279105b2e; MDN
//! browser-compat-data; всё — 2026-10-15): Chrome и Edge на Android,
//! Windows, macOS и ChromeOS, но не на Linux; Safari на iPhone и iPad
//! с iOS 15 и на Mac. Firefox файлов не отправляет нигде (`canShare`
//! с файлом — `false`). Только по HTTPS и на `localhost` (`[SecureContext]`
//! у всех трёх), и в режиме блокировки iOS меню нет вовсе. Где его нет,
//! остаётся ссылка «Скачать».
//!
//! Привязки — свои, а не `web-sys`: `share` и `canShare` там только
//! с `--cfg=web_sys_unstable_apis` (по исходникам web-sys 0.3.106,
//! `gen_Navigator.rs`), а флаг через RUSTFLAGS перебил бы флаги разреза
//! wasm в `.cargo/config.toml` (Правило 3).

use std::future::Future;

use js_sys::{Array, Object, Promise, Reflect};
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{DomException, File};

use super::pwa;

#[wasm_bindgen]
extern "C" {
    /// `navigator` с Web Share.
    #[wasm_bindgen(extends = web_sys::Navigator)]
    type Sharing;

    /// `navigator.canShare(данные)`. Нет метода — исключение `TypeError`,
    /// и привязка возвращает его ошибкой.
    #[wasm_bindgen(method, catch, js_name = canShare)]
    fn can_share(this: &Sharing, data: &Object) -> Result<bool, JsValue>;

    /// `navigator.share(данные)`.
    #[wasm_bindgen(method, catch)]
    fn share(this: &Sharing, data: &Object) -> Result<Promise, JsValue>;
}

/// Почему файл не отправлен.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ShareError {
    /// Меню закрыто, ничего не отправив: человек передумал. Говорить не
    /// о чем.
    Cancelled,
    /// Меню уже открыто — второе нажатие, пока ждём первое.
    Busy,
    /// Браузер отказал или отправка не удалась.
    Failed,
}

fn navigator() -> Option<Sharing> {
    Some(web_sys::window()?.navigator().unchecked_into())
}

/// Данные для `share` и `canShare`: только файл. Текст и заголовок рядом
/// с файлом WebKit кладёт в меню отдельными записями, и часть получателей
/// берёт текст вместо файла (WebKit bugs 251500 и 316518, открыты; по
/// исходникам WebKit, `WKShareSheet.mm`, 2026-10-15). Что это за файл,
/// говорит его имя.
fn data(file: &File) -> Object {
    let data = Object::new();
    let _ = Reflect::set(&data, &"files".into(), &Array::of1(file));
    data
}

/// Отправит ли браузер этот файл.
///
/// Тип файла `canShare` не проверяет ни в одном движке. Chromium пускает
/// только известные ему типы — по имени и по типу сразу: PDF —
/// `application/pdf` с именем `.pdf` — да, `.json` — нет, — но узнаёт
/// об этом уже `share`, отказом `NotAllowedError` (по исходникам Chromium,
/// `share_service_impl.cc`, 2026-10-15). Поэтому для файла не из этого
/// списка — [`can_share_any`].
pub fn can_share(file: &File) -> bool {
    navigator()
        .and_then(|navigator| navigator.can_share(&data(file)).ok())
        .unwrap_or(false)
}

/// Отправит ли браузер файл любого типа — и файл копии `.json`.
///
/// Верить `canShare` для такого файла можно только у WebKit: списка типов
/// у него нет (по исходникам WebKit, `Navigator::canShare`, 2026-10-15),
/// а Chromium на `.json` отвечает «можно» и отказывает уже в `share`,
/// потратив нажатие. Поэтому — только iPhone и iPad: там «Поделиться →
/// Сохранить в Файлы» ещё и обходит скачивание, которое в программе
/// с экрана «Домой» может завести в тупик без кнопки «назад» (WebKit bug
/// 290847, открыт).
pub fn can_share_any(file: &File) -> bool {
    pwa::apple_touch() && can_share(file)
}

/// Открыть меню «Поделиться» с файлом.
///
/// Зовётся прямо в обработчике нажатия, до первого `await`: меню
/// открывается только от свежего нажатия — не старше 5 секунд в Chromium
/// и WebKit — и тратит его, даже если отказало (по исходникам Chromium,
/// `user_activation_state.h`, `navigator_share.cc`, и WebKit,
/// `LocalDOMWindow.cpp`, `Navigator.cpp`; 2026-10-15). Поэтому
/// `navigator.share` вызывается сразу, а ждать — уже ответа меню.
pub fn share(file: &File) -> impl Future<Output = Result<(), ShareError>> + 'static {
    let started = navigator().ok_or(ShareError::Failed).and_then(|navigator| {
        navigator
            .share(&data(file))
            .map_err(|error| failure(&error))
    });
    async move {
        match started?.await {
            Ok(_) => Ok(()),
            Err(error) => Err(failure(&error)),
        }
    }
}

/// Ответ меню — словом; непонятный — ещё и в консоль для разработчика
/// (наружу ничего не уходит, Правило 13).
///
/// `AbortError` — закрытое меню во всех движках, но Chromium отвечает им
/// и на свой сбой («Share failed»; по исходникам Chromium,
/// `navigator_share.cc`, 2026-10-15): различить их можно только по тексту,
/// а он не обещан. Человек в обоих случаях видит, что ничего не ушло,
/// и рядом — «Скачать». `InvalidStateError` — меню уже открыто.
fn failure(error: &JsValue) -> ShareError {
    match error
        .dyn_ref::<DomException>()
        .map(DomException::name)
        .as_deref()
    {
        Some("AbortError") => ShareError::Cancelled,
        Some("InvalidStateError") => ShareError::Busy,
        _ => {
            web_sys::console::error_2(&"Kvartik, «Поделиться»:".into(), error);
            ShareError::Failed
        }
    }
}
