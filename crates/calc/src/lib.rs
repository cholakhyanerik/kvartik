//! Калькулятор лендинга (задача 34) в браузере: поля формы страницы — в ответ
//! словами. Считает и пишет всё ядро (`kvartik_core::tax::example`): этот
//! крейт только переводит вызов скрипта страницы (`crates/app/site/landing.js`)
//! в вызов ядра и берёт «сегодня» у часов браузера. Сам он ничего не считает
//! и не пишет: второй расчёт «для сайта» однажды разошёлся бы с программой
//! (CLAUDE.md, Архитектура).

use kvartik_core::i18n::Lang;
use kvartik_core::model::date::Date;
use kvartik_core::tax::example::{self, Raw};
use wasm_bindgen::prelude::*;

/// Ответ на поля формы `input` — JSON `tax::example::Raw` — словами языка
/// `lang` (`hy`, `ru`, `en`): JSON `tax::example::Answer`. Чужой язык —
/// армянский, язык страницы по умолчанию; испорченный JSON — пустая форма,
/// а пустая строка ответа — дня не узнать ни у часов, ни у правил.
#[wasm_bindgen]
pub fn calculate(lang: &str, input: &str) -> String {
    let lang = Lang::from_code(lang).unwrap_or(Lang::Am);
    let raw: Raw = serde_json::from_str(input).unwrap_or_default();
    let Some(today) = example::today(clock()) else {
        return String::new();
    };
    serde_json::to_string(&example::answer(lang, &raw, today)).unwrap_or_default()
}

/// День календаря устройства по местному времени, а не по UTC: в Ереване
/// в 01:00 по UTC ещё вчера (CLAUDE.md, Правило 2, «Даты — календарные»).
/// У `js_sys::Date` январь — 0 (Правило 6). Так же берёт день окно
/// (`crates/app/src/platform/clock.rs`).
fn clock() -> Option<Date> {
    let now = js_sys::Date::new_0();
    Date::new(
        u16::try_from(now.get_full_year()).ok()?,
        u8::try_from(now.get_month() + 1).ok()?,
        u8::try_from(now.get_date()).ok()?,
    )
}
