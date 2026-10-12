//! Платформа: то, что даёт браузер, кроме хранилища, — часы устройства,
//! шифр WebCrypto, сохранение и выбор файла, service worker и установка
//! (таблица слоёв CLAUDE.md). Бизнес-логики здесь нет: что шифровать
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
