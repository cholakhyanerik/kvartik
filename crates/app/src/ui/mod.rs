//! Экраны и компоненты окна.
//!
//! Текст — только ключами `Key` из `kvartik_core::i18n`, цвета и размеры —
//! только переменными `crates/app/style/tokens.css` и классами из
//! `style/`, а не атрибутом `style` (Правило 4). Цвет литералом и опечатку
//! в `var(--…)` ловит тест `crates/app/tests/tokens.rs`.

mod backup;
mod storage;

use std::rc::Rc;

use kvartik_core::i18n::{Key, Lang, t};
use leptos::prelude::*;

use crate::store::{self, Store};
use backup::{BackupSection, Backups};
use storage::{Storage, StorageSection};

/// Корень окна. Пока — экран каркаса: название, раздел о хранении данных
/// и раздел о резервной копии сразу на трёх языках, по блоку на язык, чтобы
/// армянский и кириллица проходили весь путь от таблицы строк до страницы.
/// Выбора языка окна ещё нет (задача 53).
#[component]
pub fn App() -> impl IntoView {
    let storage = RwSignal::new(Storage::Opening);
    // Открытая база живёт, пока живо окно (её держит `Backups`): брошенное
    // соединение браузер может закрыть, а с ним и обработчик
    // `versionchange` (Правило 6).
    let backups = Backups::new();
    leptos::task::spawn_local(async move {
        match Store::open(move |event| storage.update(|state| state.apply(event))).await {
            Ok(store) => {
                // Не прочиталось — напоминания о копии не будет, а об ошибке
                // хранилище скажет на первом же действии.
                let has_data = match store.read() {
                    Ok(tx) => tx.is_empty().await.is_ok_and(|empty| !empty),
                    Err(_) => false,
                };
                backups.opened(Rc::new(store), has_data);
                let persistence = store::persistence().await;
                storage.update(|state| state.opened(persistence));
            }
            Err(error) => storage.set(Storage::Failed(error)),
        }
    });

    view! {
        <main>
            {Lang::ALL
                .into_iter()
                // `lang` у каждого блока: по нему браузер выбирает шрифт
                // и правила переноса, а экранный диктор — произношение.
                .map(|lang| view! {
                    <section lang=lang.code()>
                        <h1>{t(lang, Key::AppName)}</h1>
                        <StorageSection lang storage />
                        <BackupSection lang backups storage />
                    </section>
                })
                .collect_view()}
        </main>
    }
}
