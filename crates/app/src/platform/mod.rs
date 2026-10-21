//! Платформа: то, что даёт браузер, кроме хранилища, — часы и языки
//! устройства, шифр WebCrypto, сохранение и выбор файла, уменьшение фото,
//! пауза в долгой работе, файлы своего сайта (шрифты документа), service
//! worker и установка, «Поделиться» (таблица слоёв CLAUDE.md). Бизнес-логики здесь нет: что шифровать и что
//! сохранять, решают ядро и хранилище.

pub mod clock;
pub mod crypto;
pub mod file;
pub mod image;
pub mod pause;
pub mod pwa;
pub mod share;
pub mod site;

/// Перезагрузить страницу. Не вышло — делать нечего: кнопка браузера
/// остаётся.
pub fn reload() {
    if let Some(window) = web_sys::window() {
        let _ = window.location().reload();
    }
}

/// Адрес сайта — схема, домен и порт (`https://kvartik.com`) — и адрес
/// программы без `#…` (`https://kvartik.com/app/`): общий календарь сроков
/// лежит в корне сайта, а ссылка из личного ведёт в программу (задача 22).
/// `None` — браузер не сказал.
pub fn address() -> Option<(String, String)> {
    let location = web_sys::window()?.location();
    let origin = location.origin().ok()?;
    let page = format!("{origin}{}", location.pathname().ok()?);
    Some((origin, page))
}

/// Часть адреса после `#` вместе с `#` (`#/clients`); пусто — её нет.
pub fn hash() -> String {
    web_sys::window()
        .and_then(|window| window.location().hash().ok())
        .unwrap_or_default()
}

/// Сменить часть адреса после `#`. Браузер запоминает шаг в истории —
/// кнопка «Назад» вернёт прежний адрес — и шлёт `hashchange`.
pub fn set_hash(hash: &str) {
    if let Some(window) = web_sys::window() {
        let _ = window.location().set_hash(hash);
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
