//! Часы устройства: «сейчас» и «сегодня».
//!
//! Ядро часов не спрашивает — оболочка берёт время здесь и передаёт
//! параметром (Архитектура, «Время — параметр»). `SystemTime::now()` на
//! wasm32 паникует (Правило 6), поэтому время — из `js_sys::Date`.

use kvartik_core::model::date::Date;
use kvartik_core::model::record::Timestamp;
use wasm_bindgen::JsValue;

/// Сейчас — по часам устройства.
pub fn now() -> Timestamp {
    // `Date.now()` — целое число миллисекунд в пределах дат JS, и `as`
    // ничего не теряет.
    Timestamp::from_unix_millis(js_sys::Date::now() as i64)
}

/// День календаря устройства, на который приходится момент `at`, — по
/// местному времени, а не по UTC: в Ереване в 01:00 по UTC ещё вчера
/// (Правило 2, «Даты — календарные»). `None` — день за пределами `Date`.
pub fn local_date(at: Timestamp) -> Option<Date> {
    let date = js_sys::Date::new(&JsValue::from_f64(at.unix_millis() as f64));
    // У `js_sys::Date` январь — 0 (Правило 6).
    Date::new(
        u16::try_from(date.get_full_year()).ok()?,
        u8::try_from(date.get_month() + 1).ok()?,
        u8::try_from(date.get_date()).ok()?,
    )
}

/// Сегодня — по календарю устройства.
pub fn today() -> Option<Date> {
    local_date(now())
}
