//! База конторы (задача 38): список ИП этого устройства — записи `ip` ядра
//! (`model::office::Ip`) — и ключи «Pro» (`license::OfficeLicense`, с 0.0.134)
//! в той же раскладке, что база ИП: хранилище `records`, ключ `[вид, id]`,
//! значение — `format::encode`. Своя база, а не записи в базе ИП: в копию
//! одного ИП, отданную клиенту, ни список, ни ключ «Pro» не попадут по
//! построению, а правка списка не трогает ни одной базы ИП.
//!
//! Открывается она, только когда устройство помнит, что ИП больше одного
//! (`Device::office`), или человек открыл экран конторы: `indexedDB.open`
//! создаёт базу, и у каждого, кто ведёт одного ИП, без метки появилась бы
//! пустая база, а первый заход ждал бы лишнее открытие (Правило 1).
//!
//! Какая база открыта во вкладке, помнит сама вкладка (`sessionStorage`):
//! две вкладки могут вести двух ИП, а перезагрузка вкладки оставляет её на
//! своём. Последнюю открытую помнит и устройство (`localStorage`) — для
//! новой вкладки. Имя проверяется по списку до открытия (`office::choose`
//! ядра).

use kvartik_core::license::OfficeLicense;
use kvartik_core::model::office::Ip;

use super::{DB_VERSION, Event, Listed, Store, StoreError, Writes};

/// Имя базы конторы. Как у базы ИП — навсегда: в ней список ИП
/// устройства, и с другим именем программа его бы не нашла.
pub const OFFICE_DB: &str = "kvartik-office";

impl Store {
    /// Открыть базу конторы.
    pub async fn open_office(on_event: impl Fn(Event) + 'static) -> Result<Self, StoreError> {
        Self::open_named(OFFICE_DB, DB_VERSION, Box::new(on_event)).await
    }
}

/// Список конторы — живые и убранные ИП. Нечитаемая запись не прячет
/// остальных: она считается в [`Listed::unreadable`], а окно называет число.
pub async fn list(office: &Store) -> Result<Listed<Ip>, StoreError> {
    office.read()?.list::<Ip>().await
}

/// Ключи «Pro» конторы. Нечитаемая запись не прячет остальных: она
/// считается в [`Listed::unreadable`]. Читают их части, которым нужен срок
/// (`ui/license.rs`), а не часть конторы при запуске: разбор ключа на пути
/// к рабочему экрану стоил бы каждому, кто ведёт контору, 5,4 КБ после
/// brotli (замер задачи 38, 2026-11-16).
pub async fn keys(office: &Store) -> Result<Listed<OfficeLicense>, StoreError> {
    office.read()?.list::<OfficeLicense>().await
}

/// Записать правку списка — одной транзакцией.
pub async fn write(office: &Store, writes: Writes) -> Result<(), StoreError> {
    office.write()?.commit(writes).await
}

/// Какую базу открыла вкладка — её запись в `sessionStorage`; `None` —
/// не помнит или хранилище не даёт.
pub fn tab_base() -> Option<String> {
    session()?.get_item(TAB_BASE).ok().flatten()
}

/// Запомнить базу вкладки. `false` — `sessionStorage` отказал: перезагрузка
/// откроет базу, которую помнит устройство.
pub fn set_tab_base(name: &str) -> bool {
    session().is_some_and(|storage| storage.set_item(TAB_BASE, name).is_ok())
}

/// Имя записи вкладки.
const TAB_BASE: &str = "kvartik.base";

/// `sessionStorage` вкладки; `None` — его нет или браузер не даёт.
pub(super) fn session() -> Option<web_sys::Storage> {
    web_sys::window()?.session_storage().ok().flatten()
}
