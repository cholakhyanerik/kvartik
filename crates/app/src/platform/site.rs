//! Файлы своего сайта по адресу: шрифты для документа (задача 12), тексты
//! лицензий (задача 51) и курсы ЦБ (задача 43).
//!
//! Шрифты PDF — те же файлы, что у окна (`public/fonts/`, `doc::fonts`):
//! service worker кладёт их в кэш с первого захода (Правило 2), и без сети
//! они приходят оттуда же — как и всё в папке программы. Запрос — только
//! к своему сайту (Архитектура, «Сеть — только к своему сайту»), адрес —
//! относительный, как у `fonts.css`.

use js_sys::Uint8Array;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::Response;

/// Файл не пришёл: сайта нет, а в кэше файла ещё нет, или сайт ответил
/// не файлом.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Unreachable;

/// Байты файлов шрифтов `fonts/<имя>` — в том же порядке. Запросы уходят
/// все сразу, а ответы ждутся по очереди: файлы идут параллельно.
///
/// Ответ не с кодом 2xx — ошибка. А ответ 2xx не тем (Cloudflare отдаёт на
/// неизвестный адрес страницу с кодом 200, Правило 6) ловит разбор шрифта
/// в ядре: страница — не TrueType.
pub async fn fonts(names: &[&'static str]) -> Result<Vec<(&'static str, Vec<u8>)>, Unreachable> {
    let window = web_sys::window().ok_or(Unreachable)?;
    let requests: Vec<_> = names
        .iter()
        .map(|name| (*name, window.fetch_with_str(&format!("fonts/{name}"))))
        .collect();
    let mut out = Vec::with_capacity(requests.len());
    for (name, request) in requests {
        let response: Response = request
            .await
            .and_then(JsValue::dyn_into)
            .map_err(|error| log(name, &error))?;
        if !response.ok() {
            return Err(log(name, &JsValue::from(response.status())));
        }
        let buffer = response
            .array_buffer()
            .map_err(|error| log(name, &error))?
            .await
            .map_err(|error| log(name, &error))?;
        out.push((name, Uint8Array::new(&buffer).to_vec()));
    }
    Ok(out)
}

/// Текст файла `name` папки программы. Ответ не с кодом 2xx — ошибка, как
/// у шрифтов: на неизвестный адрес сайт отвечает `404.html` с кодом 404
/// (с 0.0.29, Правило 6).
pub async fn text(name: &str) -> Result<String, Unreachable> {
    let window = web_sys::window().ok_or(Unreachable)?;
    let response: Response = window
        .fetch_with_str(name)
        .await
        .and_then(JsValue::dyn_into)
        .map_err(|error| log(name, &error))?;
    if !response.ok() {
        return Err(log(name, &JsValue::from(response.status())));
    }
    response
        .text()
        .map_err(|error| log(name, &error))?
        .await
        .map_err(|error| log(name, &error))?
        .as_string()
        .ok_or(Unreachable)
}

/// Текст необязательного файла `name` папки программы — курсов ЦБ (задача
/// 43): без него окно просто не подсказывает курс, поэтому ответ не 2xx —
/// `None` без записи в консоль. Обрыв сети браузер пишет в консоль сам.
pub async fn optional_text(name: &str) -> Option<String> {
    let window = web_sys::window()?;
    let response: Response = window.fetch_with_str(name).await.ok()?.dyn_into().ok()?;
    if !response.ok() {
        return None;
    }
    response.text().ok()?.await.ok()?.as_string()
}

fn log(name: &str, error: &JsValue) -> Unreachable {
    web_sys::console::error_2(&format!("Kvartik, файл {name}:").into(), error);
    Unreachable
}
