//! Экраны и компоненты окна.
//!
//! Текст — только ключами `Key` из `kvartik_core::i18n`, цвета и размеры —
//! только переменными `crates/app/style/tokens.css` и классами из
//! `style/`, а не атрибутом `style` (Правило 4). Цвет литералом и опечатку
//! в `var(--…)` ловит тест `crates/app/tests/tokens.rs`.

mod backup;
mod clients;
mod documents;
mod form;
mod home;
mod icon;
mod lang;
mod nav;
pub mod panic;
mod profile;
mod pwa;
mod route;
mod split;
mod storage;

use std::rc::Rc;

use kvartik_core::i18n::{Key, Lang, t};
use leptos::prelude::*;

use crate::platform::clock;
use crate::store::{self, Store};
use backup::{BackupChip, BackupLink, BackupSection, Backups};
use clients::{Clients, ClientsSection};
use documents::{Documents, DocumentsScreen};
use home::Home;
use icon::{Icon, icon};
use lang::{LangSwitch, WindowLang};
use nav::Nav;
use profile::{ProfileSection, Profiles};
use pwa::{Program, UpdateNotice};
use route::{Router, Screen};
use storage::{Storage, StorageAlert, StorageSection};

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
pub struct Base {
    store: StoredValue<Option<Rc<Store>>, LocalStorage>,
    /// Растёт, когда данные в базе заменило не действие раздела, а
    /// восстановление из копии: разделы с данными перечитывают своё.
    replaced: RwSignal<u32>,
}

impl Base {
    fn new() -> Self {
        Self {
            store: StoredValue::new_local(None),
            replaced: RwSignal::new(0),
        }
    }

    fn opened(&self, store: Rc<Store>) {
        self.store.set_value(Some(store));
    }

    /// Открытая база; `None` — ещё открывается или не открылась.
    pub fn get(&self) -> Option<Rc<Store>> {
        self.store.get_value()
    }

    /// Восстановление из копии записало в базу: разделам с данными пора
    /// перечитать своё, иначе окно покажет то, чего в базе уже нет.
    pub fn replaced(&self) {
        self.replaced.update(|count| *count = count.wrapping_add(1));
    }

    /// Сколько раз данные заменяли — отслеживаемое: по нему разделы узнают,
    /// что пора перечитать.
    pub fn replacements(&self) -> u32 {
        self.replaced.get()
    }
}

/// Корень окна (дизайн «Kvartik Design»): меню разделов и экраны —
/// главный, счета и акты, клиенты, профиль и копия внутри него
/// (`ui/route.rs`). Все экраны — в странице, открытый виден, остальные
/// спрятаны: начатое в форме не пропадает при переходе.
///
/// Над экранами — то, о чём окно обязано сказать, где бы человек ни был:
/// новая версия и беда с хранилищем. Напоминание о копии — карточкой на
/// главном экране и меткой под заголовком остальных: напоминать обязано
/// каждое открытие (Правило 2).
/// Установка — на главном экране, выше всего о данных: на iPhone программу
/// ставят до того, как вводить данные (Правило 6).
#[component]
pub fn App() -> impl IntoView {
    // Язык — первым: до первой отрисовки (задача 53).
    let window_lang = WindowLang::start();
    let lang = window_lang.signal();
    let router = Router::start();
    // Service worker — до базы: работа без сети от неё не зависит.
    let program = Program::new();
    let storage = RwSignal::new(Storage::Opening);
    // Открытая база живёт, пока живо окно (её держит `Base`): брошенное
    // соединение браузер может закрыть, а с ним и обработчик
    // `versionchange` (Правило 6).
    let base = Base::new();
    let backups = Backups::new(base);
    let profiles = Profiles::new();
    let clients = Clients::new();
    let documents = Documents::new();
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
                clients.load(&store).await;
                documents.load(&store, clock::today()).await;
                let persistence = store::persistence().await;
                storage.update(|state| state.opened(persistence));
            }
            // Профиль, клиенты и документы остаются пустыми: почему, скажет
            // раздел о хранении.
            Err(error) => storage.set(Storage::Failed(error)),
        }
    });

    // Части wasm — в память, когда рабочий экран готов и страницей правит
    // service worker: тогда они приходят из его кэша, а не делят канал
    // с первым заходом (`ui/split.rs`). Один раз.
    Effect::new(move |done: Option<bool>| {
        if done == Some(true) {
            return true;
        }
        let ready = matches!(storage.get(), Storage::Ready(_)) && program.serving();
        if ready {
            leptos::task::spawn_local(split::preload());
        }
        ready
    });

    // Экран сменился — страница с его начала, фокус на его заголовке.
    Effect::watch(move || router.screen(), |screen, _, _| route::shown(*screen), false);

    let hidden = move |screen: Screen| move || !router.shows(screen);
    view! {
        <div class="app">
            <Nav lang router documents clients backups />
            <main>
                <UpdateNotice lang program />
                <StorageAlert lang storage />
                <section id=Screen::Home.id() class="screen" hidden=hidden(Screen::Home)>
                    <Home lang router program backups profiles documents clients />
                </section>
                <section id=Screen::Documents.id() class="screen" hidden=hidden(Screen::Documents)>
                    <DocumentsScreen lang documents clients base backups router />
                </section>
                <section id=Screen::Clients.id() class="screen" hidden=hidden(Screen::Clients)>
                    <h1 tabindex="-1">{tr(lang, Key::ClientsTitle)}</h1>
                    <BackupChip lang backups nudge=true />
                    <ClientsSection lang clients documents base backups />
                </section>
                <section id=Screen::Profile.id() class="screen" hidden=hidden(Screen::Profile)>
                    <h1 tabindex="-1">{tr(lang, Key::ProfileTitle)}</h1>
                    <BackupChip lang backups nudge=true />
                    <ProfileSection lang profiles base backups />
                    <h2>{tr(lang, Key::Language)}</h2>
                    <LangSwitch lang=window_lang />
                    <BackupLink lang backups />
                </section>
                <section id=Screen::Backup.id() class="screen" hidden=hidden(Screen::Backup)>
                    <a class="back" href=Screen::Profile.href()>
                        {icon(Icon::Back)}
                        <span>{tr(lang, Key::NavProfile)}</span>
                    </a>
                    <h1 tabindex="-1">{tr(lang, Key::BackupTitle)}</h1>
                    <BackupSection lang backups storage />
                    <StorageSection lang storage />
                </section>
            </main>
            {panic::debug_trigger()}
        </div>
    }
}

#[cfg(all(test, target_arch = "wasm32"))]
mod tests;
