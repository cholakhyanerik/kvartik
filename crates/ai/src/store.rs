//! Подключённый ИИ — запись `localStorage` рамки (задача 111). На
//! `ai.kvartik.com` её читает только код рамки, а не окна программы: у
//! другого происхождения другое хранилище. Имя записи то же, что у окна в
//! 0.0.158 (`kvartik.ai`), — запись там же и того же вида (`ai::Setup`).
//! В копию она не идёт: копию делает окно, а хранилища рамки оно не видит.

use kvartik_core::ai::Setup;

const KEY: &str = "kvartik.ai";

fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

/// Подключённый ИИ; не читается — не подключён: рамка попросит ключ заново,
/// а не пошлёт поставщику то, что в записи лежит.
pub fn stored() -> Option<Setup> {
    storage()?.get_item(KEY).ok().flatten().as_deref().and_then(Setup::parse)
}

/// Запомнить; `false` — браузер не дал.
pub fn save(setup: &Setup) -> bool {
    storage().is_some_and(|storage| storage.set_item(KEY, &setup.to_text()).is_ok())
}

/// Стереть; `false` — браузер не дал.
pub fn clear() -> bool {
    storage().is_some_and(|storage| storage.remove_item(KEY).is_ok())
}
