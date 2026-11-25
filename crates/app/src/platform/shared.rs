//! «Поделиться → Kvartik» на Android (задача 124, часть D; решение человека
//! 2026-11-25): код передачи текстом или файл копии из мессенджера —
//! прямо в программу, без «Сохранить» и «Вставить».
//!
//! Программа объявляет себя целью «Поделиться» (`share_target` в
//! `manifest.webmanifest`), и Android шлёт поделённое запросом POST на
//! `/app/share` — его принимает service worker (`public/sw.js`): кладёт текст
//! и файлы в свой кэш [`CACHE`] и отвечает переходом на «Другое устройство»
//! или экран копии с меткой `?share=` в адресе. Окно по метке забирает
//! поделённое отсюда и стирает его: в сеть ничего не уходит, CSP прежний.
//! Цель «Поделиться» — только у установленной программы и только в Chrome на
//! Android (W3C Web Share Target; у Safari на iPhone её нет вовсе).
//!
//! Привязок `web-sys` к кэшу здесь новых нет — `caches.match` с `cacheName`,
//! `caches.delete` и `Response.text` главный wasm уже знает
//! (`platform::pwa`, `platform::look`), а `Response.blob` и
//! `history.replaceState` — через `Reflect`: новая привязка легла бы импортом
//! в главный wasm (CLAUDE.md, Правило 6).

use js_sys::{Array, Function, Promise, Reflect};
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{CacheQueryOptions, File, FilePropertyBag, Response};

/// Кэш поделённого — его же пишет `sw.js`. Не начинается с `kvartik-`:
/// новая версия программы его не удаляет, а убирает окно, забрав своё.
const CACHE: &str = "kvartik.share";

/// Адреса в нём, от папки программы, как у `sw.js`: текст, а файлы и их
/// имена — с номером с 1 (`shared-file-1`, `shared-name-1`…): частей копии
/// делятся разом.
const TEXT: &str = "shared-text";
const FILE: &str = "shared-file-";
const NAME: &str = "shared-name-";

/// Адреса файла у `sw.js` 0.0.196–0.0.198: файл один и без номера. Окно
/// читает и их: поделённое, принятое прежним `sw.js`, может дождаться окна
/// уже новой версии — её включило прежнее окно, пока не дошло до экрана копии
/// (ревью 0.0.199), — и без них пропало бы молча.
const OLD_FILE: &str = "shared-file";
const OLD_NAME: &str = "shared-name";

/// Больше файлов `sw.js` не кладёт (`MAX_SHARED_FILES`): частей у копии не
/// больше этого.
const MAX_FILES: u16 = kvartik_core::format::backup::MAX_PARTS;

/// Что поделили.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Shared {
    /// Текст — код передачи из мессенджера.
    Text,
    /// Файл копии.
    File,
}

/// Метка в адресе, которую ставит `sw.js`: `?share=text` или `?share=file`.
/// `None` — ничего не поделено.
pub fn marked() -> Option<Shared> {
    let search = web_sys::window()?.location().search().ok()?;
    search.trim_start_matches('?').split('&').find_map(|pair| match pair {
        "share=text" => Some(Shared::Text),
        "share=file" => Some(Shared::File),
        _ => None,
    })
}

/// Снять метку из адреса: перезагрузка не возьмёт поделённое снова.
fn unmark() {
    let Some(window) = web_sys::window() else {
        return;
    };
    let location = window.location();
    let (Ok(path), Ok(hash)) = (location.pathname(), location.hash()) else {
        return;
    };
    let Ok(history) = Reflect::get(&window, &"history".into()) else {
        return;
    };
    if let Ok(Ok(replace)) = Reflect::get(&history, &"replaceState".into()).map(JsCast::dyn_into::<Function>) {
        let _ = replace.call3(&history, &JsValue::NULL, &"".into(), &format!("{path}{hash}").into());
    }
}

/// Ответ из кэша поделённого по адресу `key`; `None` — его там нет. Кэш ищется
/// по имени, а не открывается: `caches.open` завёл бы пустой у каждого.
async fn found(key: &str) -> Option<Response> {
    let caches = web_sys::window()?.caches().ok()?;
    let options = CacheQueryOptions::new();
    options.set_cache_name(CACHE);
    // Нет в кэше — `undefined`, а не ответ.
    caches.match_with_str_and_options(key, &options).await.ok()?.dyn_into().ok()
}

/// Стереть кэш поделённого — всё, что в нём, разом.
async fn forget() {
    if let Some(caches) = web_sys::window().and_then(|window| window.caches().ok()) {
        let _ = caches.delete(CACHE).await;
    }
}

/// Текст ответа.
async fn text_of(response: &Response) -> Option<String> {
    response.text().ok()?.await.ok()?.as_string()
}

/// Забрать поделённый текст — и стереть поделённое, и метку.
pub async fn take_text() -> Option<String> {
    let text = match found(TEXT).await {
        Some(response) => text_of(&response).await,
        None => None,
    };
    forget().await;
    unmark();
    text
}

/// Забрать поделённые файлы копии — все, по номерам, — и стереть поделённое,
/// и метку. Тело — как есть, `Blob`, без чтения в память wasm: часть копии
/// бывает до 128 МБ, и читает её цепочка восстановления, когда до неё дошла
/// очередь (задача 122). До 0.0.199 окно брало только первый файл, и вторая
/// часть копии, поделённая с первой, пропадала молча.
pub async fn take_files() -> Vec<File> {
    let mut files = Vec::new();
    for number in 1..=MAX_FILES {
        match file(&format!("{FILE}{number}"), &format!("{NAME}{number}")).await {
            Some(file) => files.push(file),
            None => break,
        }
    }
    if files.is_empty()
        && let Some(file) = file(OLD_FILE, OLD_NAME).await
    {
        files.push(file);
    }
    forget().await;
    unmark();
    files
}

/// Файл по адресу тела `body` и имени `name` в кэше поделённого.
async fn file(body: &str, name: &str) -> Option<File> {
    let body = found(body).await?;
    let name = match found(name).await {
        Some(response) => text_of(&response).await,
        None => None,
    }
    .filter(|name| !name.is_empty())
    .unwrap_or_else(|| "kvartik.json".to_owned());
    // `Response.blob()` — через `Reflect`, обещание — по его `then`, а не
    // `instanceof Promise`: та привязка легла бы импортом в главный wasm.
    let blob = Reflect::get(&body, &"blob".into()).ok()?.dyn_into::<Function>().ok()?.call0(&body).ok()?;
    if !Reflect::get(&blob, &"then".into()).ok()?.is_function() {
        return None;
    }
    let blob = blob.unchecked_into::<Promise>().await.ok()?;
    let options = FilePropertyBag::new();
    // Тот же конструктор `File`, что у файла для «Поделиться»
    // (`platform::file`): частью он берёт и `Blob`.
    File::new_with_buffer_source_sequence_and_options(&Array::of1(&blob), &name, &options).ok()
}
