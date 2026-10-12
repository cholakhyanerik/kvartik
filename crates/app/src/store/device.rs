//! Что знает это устройство: когда здесь в последний раз сделали копию,
//! проверочное значение пароля копий (`DeviceKey`) и выбранный язык окна.
//!
//! Это не данные человека, а состояние устройства, поэтому оно лежит не
//! в базе записей и не попадает в копию: перенесённая на новое устройство
//! дата копии обещала бы копию, которой там не делали, а копия, открытая
//! бухгалтером, сменила бы ему язык окна. Место — `localStorage`: он
//! синхронный — язык окна читается до первой отрисовки, и окно не мигает
//! чужим языком, — и не требует новой версии базы, а стирает его браузер
//! вместе с базой (по браузеру: данные сайта, записанные скриптом,
//! стираются целиком — WebKit, «Updates to Storage Policy»; сверено
//! 2026-10-10, Правило 6). Транзакций ему не нужно: записи независимы,
//! и потерянная запись стоит одного лишнего напоминания, вопроса о пароле
//! или выбора языка.
//!
//! `localStorage` может отказать — сайтам запрещено хранить данные, место
//! кончилось: тогда чтение даёт «нет», а запись — `false`, и окно живёт
//! дальше без них.

use kvartik_core::format::backup::DeviceKey;
use kvartik_core::i18n::Lang;
use kvartik_core::model::record::Timestamp;

/// Записи устройства под своим префиксом: программа — под [`Device::new`],
/// браузерные тесты — каждый под своим, чтобы не мешать друг другу.
pub struct Device {
    prefix: String,
}

impl Device {
    /// Записи программы.
    pub fn new() -> Self {
        Self::with_prefix("kvartik.")
    }

    pub(super) fn with_prefix(prefix: &str) -> Self {
        Self {
            prefix: prefix.to_owned(),
        }
    }

    /// Когда на этом устройстве сделана последняя копия; `None` — не
    /// делалась или запись не читается.
    pub fn last_backup(&self) -> Option<Timestamp> {
        let millis: i64 = self.get("last_backup")?.parse().ok()?;
        (millis.unsigned_abs() <= Timestamp::MAX_MILLIS.unsigned_abs())
            .then(|| Timestamp::from_unix_millis(millis))
    }

    /// Запомнить момент копии. `false` — `localStorage` отказал.
    pub fn set_last_backup(&self, at: Timestamp) -> bool {
        self.set("last_backup", &at.unix_millis().to_string())
    }

    /// Пароль копий этого устройства; `None` — не задан или запись не
    /// читается, и пароль зададут заново.
    pub fn backup_key(&self) -> Option<DeviceKey> {
        DeviceKey::from_text(&self.get("backup_key")?)
    }

    /// Запомнить пароль копий. `false` — `localStorage` отказал.
    pub fn set_backup_key(&self, key: &DeviceKey) -> bool {
        self.set("backup_key", &key.to_text())
    }

    /// Язык окна, который выбрал человек; `None` — не выбирал или запись
    /// не читается, и язык подскажет браузер.
    pub fn ui_lang(&self) -> Option<Lang> {
        Lang::from_code(&self.get("lang")?)
    }

    /// Запомнить язык окна. `false` — `localStorage` отказал.
    pub fn set_ui_lang(&self, lang: Lang) -> bool {
        self.set("lang", lang.code())
    }

    fn get(&self, name: &str) -> Option<String> {
        storage()?
            .get_item(&format!("{}{name}", self.prefix))
            .ok()
            .flatten()
    }

    fn set(&self, name: &str, value: &str) -> bool {
        storage().is_some_and(|storage| {
            storage
                .set_item(&format!("{}{name}", self.prefix), value)
                .is_ok()
        })
    }

    /// Стереть записи этого префикса — для тестов.
    #[cfg(all(test, target_arch = "wasm32"))]
    pub(super) fn clear(&self) {
        if let Some(storage) = storage() {
            for name in ["last_backup", "backup_key", "lang"] {
                let _ = storage.remove_item(&format!("{}{name}", self.prefix));
            }
        }
    }
}

/// `localStorage`; `None` — его нет или браузер не даёт.
fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}
