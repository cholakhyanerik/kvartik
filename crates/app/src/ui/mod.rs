//! Экраны и компоненты окна.
//!
//! Текст — только ключами `Key` из `kvartik_core::i18n`, цвета и размеры —
//! только переменными `crates/app/style/tokens.css` и классами из
//! `style/`, а не атрибутом `style` (Правило 4). Цвет литералом и опечатку
//! в `var(--…)` ловит тест `crates/app/tests/tokens.rs`.

mod backup;
mod lang;
pub mod panic;
mod profile;
mod pwa;
mod storage;

use std::rc::Rc;

use kvartik_core::i18n::{Key, Lang, t};
use leptos::prelude::*;

use crate::store::{self, Store};
use backup::{BackupReminder, BackupSection, Backups};
use lang::{LangSwitch, WindowLang};
use profile::{ProfileSection, Profiles};
use pwa::{InstallSection, Program, UpdateNotice};
use storage::{Storage, StorageSection};

/// Строка окна на его языке — производное от сигнала языка, а не значение,
/// вычисленное один раз: при смене языка она пересобирается сама
/// (Правило 4).
fn tr(lang: ReadSignal<Lang>, key: Key) -> impl Fn() -> &'static str + Copy + Send + 'static {
    move || t(lang.get(), key)
}

/// Открытая база — одна на всё окно, общая у разделов, которые читают
/// и пишут данные. Живёт, пока живо окно: брошенное соединение браузер
/// может закрыть, а с ним и обработчик `versionchange` (Правило 6).
#[derive(Clone, Copy)]
pub struct Base(StoredValue<Option<Rc<Store>>, LocalStorage>);

impl Base {
    fn new() -> Self {
        Self(StoredValue::new_local(None))
    }

    fn opened(&self, store: Rc<Store>) {
        self.0.set_value(Some(store));
    }

    /// Открытая база; `None` — ещё открывается или не открылась.
    pub fn get(&self) -> Option<Rc<Store>> {
        self.0.get_value()
    }
}

/// Корень окна. Пока — экран каркаса: название и выбор языка, новая версия,
/// напоминание о копии, установка, профиль ИП, раздел о хранении данных
/// и раздел о резервной копии.
///
/// Установка — выше профиля: на iPhone программу ставят до того, как
/// вводить данные (Правило 6). Напоминание о копии — над всем, что растёт:
/// с профилем раздел копии при открытии уже не виден, а напоминать обязано
/// каждое открытие (Правило 2).
#[component]
pub fn App() -> impl IntoView {
    // Язык — первым: до первой отрисовки (задача 53).
    let window_lang = WindowLang::start();
    let lang = window_lang.signal();
    // Service worker — до базы: работа без сети от неё не зависит.
    let program = Program::new();
    let storage = RwSignal::new(Storage::Opening);
    // Открытая база живёт, пока живо окно (её держит `Base`): брошенное
    // соединение браузер может закрыть, а с ним и обработчик
    // `versionchange` (Правило 6).
    let base = Base::new();
    let backups = Backups::new(base);
    let profiles = Profiles::new();
    leptos::task::spawn_local(async move {
        match Store::open(move |event| storage.update(|state| state.apply(event))).await {
            Ok(store) => {
                let store = Rc::new(store);
                base.opened(Rc::clone(&store));
                // Не прочиталось — напоминания о копии не будет, а об ошибке
                // хранилище скажет на первом же действии.
                let has_data = match store.read() {
                    Ok(tx) => tx.is_empty().await.is_ok_and(|empty| !empty),
                    Err(_) => false,
                };
                backups.opened(has_data);
                profiles.load(&store).await;
                let persistence = store::persistence().await;
                storage.update(|state| state.opened(persistence));
            }
            // Профиль остаётся пустым: почему, скажет раздел о хранении.
            Err(error) => storage.set(Storage::Failed(error)),
        }
    });

    view! {
        <main>
            <section>
                <header class="top">
                    <h1>{tr(lang, Key::AppName)}</h1>
                    <LangSwitch lang=window_lang />
                </header>
                <UpdateNotice lang program />
                <BackupReminder lang backups />
                <InstallSection lang program />
                <ProfileSection lang profiles base backups />
                <StorageSection lang storage />
                <BackupSection lang backups storage />
            </section>
            {panic::debug_trigger()}
        </main>
    }
}

#[cfg(all(test, target_arch = "wasm32"))]
mod tests;
