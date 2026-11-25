//! Платформа: то, что даёт браузер, кроме хранилища, — часы и языки
//! устройства, шифр WebCrypto, сохранение и выбор файла, уменьшение фото,
//! пауза в долгой работе, файлы своего сайта (шрифты документа), service
//! worker и установка, «Поделиться», буфер обмена, камера для QR-кода, папка для автокопий,
//! переход между экранами и что система говорит о движении, свои цвета на
//! странице и тема устройства (таблица слоёв CLAUDE.md). Бизнес-логики здесь
//! нет: что шифровать и что сохранять, решают ядро и хранилище.

pub mod autobackup;
pub mod awake;
pub mod camera;
pub mod clipboard;
pub mod clock;
pub mod crypto;
pub mod file;
pub mod image;
pub mod look;
pub mod motion;
pub mod pause;
pub mod pwa;
pub mod share;
pub mod shared;
pub mod site;

/// Перезагрузить страницу. Не вышло — делать нечего: кнопка браузера
/// остаётся.
#[cfg(not(all(test, target_arch = "wasm32")))]
pub fn reload() {
    if let Some(window) = web_sys::window() {
        let _ = window.location().reload();
    }
}

/// В браузерных тестах страница — раннер тестов, и перезагрузка оборвала бы
/// прогон: вместо неё — счёт просьб, которые видит тест (переход к другому
/// ИП, `ui/office.rs`).
#[cfg(all(test, target_arch = "wasm32"))]
pub fn reload() {
    RELOADS.with(|count| count.set(count.get() + 1));
}

#[cfg(all(test, target_arch = "wasm32"))]
thread_local! {
    /// Сколько раз окно просило перезагрузить страницу.
    pub static RELOADS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
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

/// Звать `handler`, когда страница снова на глазах: человек вернулся
/// к вкладке или окну. Слушатель — на всё время страницы. Свойство
/// `onvisibilitychange` занято проверкой новой версии (`platform::pwa`),
/// поэтому — `addEventListener`, как у `autobackup::when_hidden`.
pub fn when_shown(handler: impl Fn() + 'static) {
    use wasm_bindgen::JsCast;
    use wasm_bindgen::closure::Closure;

    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        return;
    };
    let watched = document.clone();
    let listener = Closure::<dyn Fn()>::new(move || {
        if watched.visibility_state() == web_sys::VisibilityState::Visible {
            handler();
        }
    });
    if document
        .add_event_listener_with_callback("visibilitychange", listener.as_ref().unchecked_ref())
        .is_ok()
    {
        listener.forget();
    }
}

/// Сменить часть адреса после `#`. Браузер запоминает шаг в истории —
/// кнопка «Назад» вернёт прежний адрес — и шлёт `hashchange`.
pub fn set_hash(hash: &str) {
    if let Some(window) = web_sys::window() {
        let _ = window.location().set_hash(hash);
    }
}

/// Язык страницы сайта, с которой пришли, — `lang` строки запроса адреса:
/// «Открыть Kvartik» лендинга ведёт на `/app/?lang=ru` (полный прогон
/// 2026-11-10, п. 11). `None` — его нет. Что это за язык, решает окно.
pub fn address_lang() -> Option<String> {
    let search = web_sys::window()?.location().search().ok()?;
    search
        .trim_start_matches('?')
        .split('&')
        .find_map(|pair| pair.strip_prefix("lang="))
        .map(str::to_owned)
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
