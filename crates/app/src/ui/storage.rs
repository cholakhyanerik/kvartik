//! Раздел «Хранение данных»: сотрёт ли браузер данные сам и что мешает
//! хранилищу — словами (задача 6, Правило 6).
//!
//! Экрана настроек ещё нет, и раздел стоит на единственном экране каркаса.
//! Слова для ошибок хранилища ([`error_key`]) — общие: ими же заговорят
//! действия, когда сохранение не удалось.

use kvartik_core::i18n::{Key, Lang};
use leptos::prelude::*;

use super::tr;
use crate::platform;
use crate::store::{Event, Persistence, StoreError};

/// Что окно знает о хранилище.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Storage {
    /// База ещё открывается.
    Opening,
    /// Открытие ждёт, пока закроется вкладка со старой версией.
    Blocked,
    /// База открыта; сотрёт ли браузер данные сам.
    Ready(Persistence),
    /// Хранилищем пользоваться нельзя — до перезагрузки страницы.
    Failed(StoreError),
}

impl Storage {
    /// Хранилище сообщило о себе само.
    pub fn apply(&mut self, event: Event) {
        match event {
            Event::Blocked => {
                if *self == Self::Opening {
                    *self = Self::Blocked;
                }
            }
            Event::Closed(why) => *self = Self::Failed(why),
            Event::Persistence(answer) => {
                if let Self::Ready(_) = self {
                    *self = Self::Ready(answer);
                }
            }
        }
    }

    /// База открылась, и браузер сказал, постоянное ли хранение. Закрытое
    /// за это время соединение остаётся закрытым: ответ про хранение мог
    /// прийти позже `versionchange`.
    pub fn opened(&mut self, persistence: Persistence) {
        if let Self::Opening | Self::Blocked = self {
            *self = Self::Ready(persistence);
        }
    }
}

/// Слова для ошибки хранилища.
pub const fn error_key(error: StoreError) -> Key {
    match error {
        StoreError::Unavailable => Key::StoreUnavailable,
        StoreError::Newer => Key::StoreNewer,
        StoreError::Replaced => Key::StoreReplaced,
        StoreError::Lost => Key::StoreLost,
        StoreError::Full => Key::StoreFull,
        StoreError::Damaged => Key::StoreDamaged,
        StoreError::Failed => Key::StoreFailed,
    }
}

/// Что сказать про хранилище; `None` — пока нечего: база открывается
/// долю секунды, и «открываем…» только мигнуло бы.
const fn message(storage: Storage) -> Option<Key> {
    match storage {
        Storage::Opening => None,
        Storage::Blocked => Some(Key::StorageBlocked),
        Storage::Ready(Persistence::Persistent) => Some(Key::StoragePersistent),
        Storage::Ready(Persistence::BestEffort) => Some(Key::StorageBestEffort),
        Storage::Ready(Persistence::Unknown) => Some(Key::StorageUnknown),
        Storage::Failed(error) => Some(error_key(error)),
    }
}

/// Раздел на языке окна.
#[component]
pub fn StorageSection(lang: ReadSignal<Lang>, storage: RwSignal<Storage>) -> impl IntoView {
    move || {
        let state = storage.get();
        let failed = matches!(state, Storage::Failed(_));
        // Ошибка — сразу диктору (`alert`), ожидание — когда тот договорит
        // (`status`); спокойное состояние не объявляется.
        let role = match state {
            Storage::Failed(_) => Some("alert"),
            Storage::Blocked => Some("status"),
            Storage::Opening | Storage::Ready(_) => None,
        };
        message(state).map(|key| {
            view! {
                <h2>{tr(lang, Key::StorageTitle)}</h2>
                <p role=role>{tr(lang, key)}</p>
                {failed.then(|| view! {
                    <button type="button" on:click=|_| platform::reload()>{tr(lang, Key::Reload)}</button>
                })}
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Закрытое соединение остаётся закрытым, даже если ответ про хранение
    /// пришёл после `versionchange`, а ожидание старой вкладки кончается
    /// открытием.
    #[test]
    fn closing_wins_over_late_answers() {
        let mut storage = Storage::Opening;
        storage.apply(Event::Blocked);
        assert_eq!(storage, Storage::Blocked);
        storage.opened(Persistence::BestEffort);
        assert_eq!(storage, Storage::Ready(Persistence::BestEffort));
        storage.apply(Event::Persistence(Persistence::Persistent));
        assert_eq!(storage, Storage::Ready(Persistence::Persistent));

        storage.apply(Event::Closed(StoreError::Replaced));
        storage.opened(Persistence::Persistent);
        storage.apply(Event::Persistence(Persistence::Persistent));
        storage.apply(Event::Blocked);
        assert_eq!(storage, Storage::Failed(StoreError::Replaced));
    }

    /// Каждое состояние, кроме открытия, говорит словами, и каждая ошибка —
    /// своими: две ошибки с одними словами человек не различит.
    #[test]
    fn every_state_has_its_own_words() {
        let errors = [
            StoreError::Unavailable,
            StoreError::Newer,
            StoreError::Replaced,
            StoreError::Lost,
            StoreError::Full,
            StoreError::Damaged,
            StoreError::Failed,
        ];
        let mut states = vec![
            Storage::Blocked,
            Storage::Ready(Persistence::Persistent),
            Storage::Ready(Persistence::BestEffort),
            Storage::Ready(Persistence::Unknown),
        ];
        states.extend(errors.map(Storage::Failed));
        let mut keys: Vec<Key> = states.iter().filter_map(|state| message(*state)).collect();
        assert_eq!(keys.len(), states.len(), "состояние без слов");
        keys.sort_by_key(|key| format!("{key:?}"));
        keys.dedup();
        assert_eq!(keys.len(), states.len(), "у двух состояний одни слова");
        assert_eq!(message(Storage::Opening), None);
    }
}
