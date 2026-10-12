//! Платформа: то, что даёт браузер, кроме хранилища, — часы и языки
//! устройства, шифр WebCrypto, сохранение и выбор файла, service worker
//! и установка (таблица слоёв CLAUDE.md). Бизнес-логики здесь нет: что шифровать
//! и что сохранять, решают ядро и хранилище.

pub mod clock;
pub mod crypto;
pub mod file;
pub mod pwa;

/// Перезагрузить страницу. Не вышло — делать нечего: кнопка браузера
/// остаётся.
pub fn reload() {
    if let Some(window) = web_sys::window() {
        let _ = window.location().reload();
    }
}

/// Языки браузера по убыванию предпочтения (`navigator.languages`, а нет
/// его — `navigator.language`); пусто — браузер не сказал.
pub fn languages() -> Vec<String> {
    let Some(window) = web_sys::window() else {
        return Vec::new();
    };
    let navigator = window.navigator();
    let languages: Vec<String> = navigator
        .languages()
        .iter()
        .filter_map(|tag| tag.as_string())
        .collect();
    if languages.is_empty() {
        navigator.language().into_iter().collect()
    } else {
        languages
    }
}
