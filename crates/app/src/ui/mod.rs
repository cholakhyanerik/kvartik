//! Экраны и компоненты окна.
//!
//! Текст — только ключами `Key` из `kvartik_core::i18n`, цвета и размеры —
//! только переменными `crates/app/style/tokens.css` и классами из
//! `style/`, а не атрибутом `style` (Правило 4). Цвет литералом и опечатку
//! в `var(--…)` ловит тест `crates/app/tests/tokens.rs`.

mod storage;

use kvartik_core::i18n::{Key, Lang, t};
use leptos::prelude::*;

use crate::store::{self, Store};
use storage::{Storage, StorageSection};

/// Корень окна. Пока — экран каркаса: название и раздел о хранении данных
/// сразу на трёх языках, по блоку на язык, чтобы армянский и кириллица
/// проходили весь путь от таблицы строк до страницы. Выбора языка окна ещё
/// нет (задача 53).
#[component]
pub fn App() -> impl IntoView {
    let storage = RwSignal::new(Storage::Opening);
    // Открытая база живёт, пока живо окно: брошенное соединение браузер
    // может закрыть, а с ним и обработчик `versionchange` (Правило 6).
    let opened = StoredValue::new_local(None::<Store>);
    leptos::task::spawn_local(async move {
        match Store::open(move |event| storage.update(|state| state.apply(event))).await {
            Ok(store) => {
                opened.set_value(Some(store));
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
                    </section>
                })
                .collect_view()}
        </main>
    }
}
