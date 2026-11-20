//! Вопрос поставщику ИИ — запросом прямо из браузера человека, с его ключом,
//! и ответ потоком по кускам (задача 102; с 0.0.160 — из рамки на своём
//! происхождении, задача 111). Что спросить и как прочесть ответ, решает ядро
//! (`kvartik_core::ai`): здесь — только сеть.
//!
//! Адрес — один из четырёх, которые пускает CSP рамки (`connect-src`
//! в `crates/ai/public/_headers`): запрос на любой другой браузер отвергнет
//! сам. Service worker у рамки нет, а кэш HTTP ответов на `POST` не хранит.
//!
//! Поток — `Response.body` и его читатель — через `Reflect`, а не привязками
//! `web-sys`: так было в окне, где каждый импорт JS ложился в главный wasm
//! (CLAUDE.md, Правило 6), и так проверено вживую на всех четырёх
//! поставщиках (2026-11-19).

use js_sys::{Function, Object, Promise, Reflect, Uint8Array};
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{RequestInit, Response};

/// Ответ поставщика: код и тело, которое читается кусками, — и поток
/// ответа, и слова о сбое.
pub struct Body {
    status: u16,
    reader: JsValue,
}

/// Кусок тела.
pub enum Chunk {
    Bytes(Vec<u8>),
    /// Тело кончилось — само или остановкой ([`Body::cancel`]).
    End,
    /// Тело оборвалось: сеть.
    Broken,
}

/// Спросить: `POST` на `url` с заголовками и телом JSON. Куки и адрес
/// страницы поставщику не уходят (`credentials: omit`, `no-referrer`).
/// `None` — ответа нет: нет сети, адрес не пустил CSP или ответ без
/// заголовков CORS (так OpenAI отвечает на неверный ключ — проверено
/// 2026-11-19).
pub async fn open(url: &str, headers: &[(&str, String)], body: &str) -> Option<Body> {
    let window = web_sys::window()?;
    let fields = Object::new();
    for (name, value) in headers {
        set(&fields, name, &JsValue::from_str(value));
    }
    let init = Object::new();
    set(&init, "method", &JsValue::from_str("POST"));
    set(&init, "headers", &fields);
    set(&init, "body", &JsValue::from_str(body));
    set(&init, "credentials", &JsValue::from_str("omit"));
    set(&init, "cache", &JsValue::from_str("no-store"));
    set(&init, "referrerPolicy", &JsValue::from_str("no-referrer"));
    let response: Response = window
        .fetch_with_str_and_init(url, init.unchecked_ref::<RequestInit>())
        .await
        .ok()?
        .dyn_into()
        .ok()?;
    let stream = Reflect::get(&response, &JsValue::from_str("body")).ok()?;
    let reader = call(&stream, "getReader")?;
    Some(Body {
        status: response.status(),
        reader,
    })
}

impl Body {
    /// Код ответа HTTP.
    pub fn status(&self) -> u16 {
        self.status
    }

    /// Следующий кусок тела. `read()` читателя по спецификации Streams
    /// всегда отдаёт обещание, а кусок — `Uint8Array`: проверки `instanceof`
    /// стоили бы главному wasm двух импортов (CLAUDE.md, Правило 6), а кусок
    /// другого вида `Uint8Array::new` прочтёт пустым.
    pub async fn next(&self) -> Chunk {
        let Some(promise) = call(&self.reader, "read") else {
            return Chunk::Broken;
        };
        let Ok(result) = promise.unchecked_into::<Promise>().await else {
            return Chunk::Broken;
        };
        let done = Reflect::get(&result, &JsValue::from_str("done"))
            .ok()
            .and_then(|done| done.as_bool())
            .unwrap_or(true);
        if done {
            return Chunk::End;
        }
        match Reflect::get(&result, &JsValue::from_str("value")) {
            Ok(value) => Chunk::Bytes(Uint8Array::new(&value).to_vec()),
            Err(_) => Chunk::Broken,
        }
    }

    /// Остановить поток: соединение закрывается, и поставщик перестаёт
    /// писать ответ. Ждущий кусок [`Body::next`] получает конец тела. Звать
    /// только у живого потока: у оборвавшегося обещание отмены отвергнуто,
    /// и браузер написал бы в консоль о необработанном отказе.
    pub fn cancel(&self) {
        let _ = call(&self.reader, "cancel");
    }
}

/// Свойство объекта. Не встало — запрос уйдёт без него, а отказ покажет
/// ответ поставщика.
fn set(target: &Object, name: &str, value: &JsValue) {
    let _ = Reflect::set(target, &JsValue::from_str(name), value);
}

/// Метод `name` объекта `target` без аргументов; `None` — его нет или он
/// бросил исключение.
fn call(target: &JsValue, name: &str) -> Option<JsValue> {
    let method: Function = Reflect::get(target, &JsValue::from_str(name)).ok()?.dyn_into().ok()?;
    method.call0(target).ok()
}
