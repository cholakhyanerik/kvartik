//! Экраны и компоненты окна.
//!
//! Текст — только ключами `Key` из `kvartik_core::i18n`, цвета и размеры —
//! только переменными `crates/app/style/tokens.css` и классами из
//! `style/`, а не атрибутом `style` (Правило 4). Цвет литералом и опечатку
//! в `var(--…)` ловит тест `crates/app/tests/tokens.rs`.

mod about;
mod annual;
mod avatar;
mod backup;
mod clients;
mod deadlines;
mod documents;
mod form;
mod home;
mod icon;
mod lang;
mod ledger;
mod license;
mod nav;
mod office;
mod onboarding;
pub mod panic;
mod paper;
mod profile;
mod pwa;
mod opening;
mod report;
mod route;
mod split;
mod storage;
mod tax;

use std::rc::Rc;

use kvartik_core::i18n::{Key, Lang, t};
use leptos::prelude::*;

use crate::platform::clock;
use crate::store::{self, Store};
use about::{AboutEntry, AboutScreen};
use annual::AnnualScreen;
use backup::{BackupChip, BackupEntry, BackupSection, Backups};
use clients::{Clients, ClientsSection};
use deadlines::DeadlinesScreen;
use documents::{Documents, DocumentsScreen};
use form::Forms;
use home::Home;
use icon::{Icon, icon};
use lang::{LangSwitch, WindowLang};
use ledger::{LedgerScreen, Operations};
use license::{LicenseEntry, LicenseScreen, Licenses};
use nav::Nav;
use office::{Office, OfficeBar, OfficeEntry, OfficeScreen};
use onboarding::Welcome;
use profile::{ProfileSection, Profiles};
use pwa::{Program, UpdateDialog, UpdateNotice};
use report::ReportScreen;
use route::{Router, Screen};
use storage::{Storage, StorageAlert, StorageSection};
use tax::{Closings, Sources, TaxScreen};

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
    /// Открылась ли база — отслеживаемое: части, которые читают своё сами
    /// (книга и налог, `Operations::watch`, `Closings::watch`), читают,
    /// когда она открыта.
    open: RwSignal<bool>,
    /// Растёт, когда данные в базе заменило не действие раздела, а
    /// восстановление из копии: разделы с данными перечитывают своё.
    replaced: RwSignal<u32>,
}

impl Base {
    fn new() -> Self {
        Self {
            store: StoredValue::new_local(None),
            open: RwSignal::new(false),
            replaced: RwSignal::new(0),
        }
    }

    fn opened(&self, store: Rc<Store>) {
        self.store.set_value(Some(store));
        self.open.set(true);
    }

    /// Открыта ли база — отслеживаемое.
    pub fn is_open(&self) -> bool {
        self.open.get()
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
/// главный, счета и акты, книга, клиенты, налог и внутри него годовые
/// платежи, сроки и числа для расчёта, профиль и внутри него копия,
/// лицензия и «О программе» (`ui/route.rs`). Все экраны —
/// в странице, открытый виден, остальные спрятаны: начатое в форме не
/// пропадает при переходе. Содержимое счетов, книги, клиентов, налога,
/// годовых платежей и профиля — частями wasm с первого показа
/// (`split::screen`, задача 61); документы, клиентов и профиль окно читает
/// всё равно при открытии базы: они нужны главному экрану. Книгу и закрытые
/// кварталы читают части, которым они нужны, — книга и налог с годовыми
/// платежами (их карточки на главном экране — тоже из части, задачи 19
/// и 20).
/// «Программа» на экране профиля — копия и язык окна — не в части: язык
/// меняют и тогда, когда часть не загрузилась.
///
/// Над экранами — то, о чём окно обязано сказать, где бы человек ни был:
/// беда с хранилищем и новая версия, пришедшая, пока открыта форма. Новая
/// версия включается сама, а что в ней нового — окном поверх всего
/// (задача 67): пока оно стоит, окно под ним — `inert`, ни нажать, ни встать
/// фокусом. Формы окно считает ([`Forms`]): с открытой новая версия ждёт.
/// Напоминание о копии — карточкой на главном экране и меткой под
/// заголовком остальных: напоминать обязано каждое открытие (Правило 2).
/// Установка — на главном экране, выше всего о данных: на iPhone программу
/// ставят до того, как вводить данные (Правило 6).
#[component]
pub fn App() -> impl IntoView {
    // Язык — первым: до первой отрисовки (задача 53).
    let window_lang = WindowLang::start();
    let lang = window_lang.signal();
    let router = Router::start();
    // Первый запуск — пока язык окна не выбран (`ui::onboarding`). Окно под
    // ним открывает базу и готовит экраны, спрятанным.
    let welcome = RwSignal::new(onboarding::needed());
    // Service worker — до базы: работа без сети от неё не зависит.
    let program = Program::new();
    let forms = Forms::new();
    program.update_when_free(forms);
    let blocked = Memo::new(move |_| program.blocks());
    let storage = RwSignal::new(Storage::Opening);
    // Открытая база живёт, пока живо окно (её держит `Base`): брошенное
    // соединение браузер может закрыть, а с ним и обработчик
    // `versionchange` (Правило 6).
    let base = Base::new();
    let backups = Backups::new(base);
    // ИП в программе (задача 38): какую базу открыть — решает контора.
    let office = Office::new(backups);
    backups.attach(office);
    let profiles = Profiles::new();
    let clients = Clients::new();
    let documents = Documents::new();
    let operations = Operations::new();
    let closings = Closings::new();
    let sources = Sources {
        documents,
        operations,
        profiles,
        closings,
    };
    // Пробный период — с первого запуска на устройстве (задача 25); ключи
    // читают части, которым они нужны (`ui/license.rs`).
    let licenses = Licenses::start(clock::today());
    let tax = tax::Ctx::new(lang, sources, base, backups, licenses, forms);
    leptos::task::spawn_local(async move {
        // Запись в базу — дело автокопии, а не состояния хранилища: сигнал
        // хранилища на каждое сохранение будил бы всех, кто его читает.
        let on_event = move |event| match event {
            store::Event::Written => backups.written(),
            event => storage.update(|state| state.apply(event)),
        };
        let name = office.start().await;
        match Store::open_base(&name, on_event).await {
            Ok(store) => {
                let store = Rc::new(store);
                base.opened(Rc::clone(&store));
                // Не прочиталось — напоминания о копии не будет, а об ошибке
                // хранилище скажет на первом же действии.
                let has_data = match store.read() {
                    Ok(tx) => tx.is_empty().await.is_ok_and(|empty| !empty),
                    Err(_) => false,
                };
                // ИП несколько — данные у программы есть, даже если открыт
                // пустой новый ИП: напоминание о копии и автокопия — за всех
                // (ревью 2026-11-06).
                backups.opened(has_data || crate::store::device::Device::new().office());
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

    // Профиль открытой базы — копией в список ИП (задача 38): прочитан,
    // сохранён или восстановлен из копии.
    Effect::new(move |_| {
        if let Some(profile) = profiles.profile() {
            office.profile_changed(profile);
        }
    });
    let office_ctx = office::Ctx {
        lang,
        office,
        profiles,
        forms,
        base,
        backups,
    };

    // Экран сменился — страница с его начала, фокус на его заголовке.
    Effect::watch(move || router.screen(), |screen, _, _| route::shown(*screen), false);

    let hidden = move |screen: Screen| move || !router.shows(screen);
    // Первый запуск и окно под окном «Что нового» неактивны: ни нажать,
    // ни встать фокусом.
    let inert = move || blocked.get();
    view! {
        {move || welcome.get().then(|| view! {
            <div inert=inert>
                <Welcome window_lang open=welcome router program />
            </div>
        })}
        <div class="app" hidden=move || welcome.get() inert=inert>
            <Nav lang router documents clients backups />
            <main>
                <UpdateNotice lang program forms />
                <StorageAlert lang storage />
                <OfficeBar ctx=office_ctx />
                <section id=Screen::Home.id() class="screen" hidden=hidden(Screen::Home)>
                    <Home lang router program backups profiles documents clients tax welcome office />
                </section>
                <section id=Screen::Documents.id() class="screen" hidden=hidden(Screen::Documents)>
                    <DocumentsScreen lang documents clients profiles licenses base backups router forms />
                </section>
                <section id=Screen::Ledger.id() class="screen" hidden=hidden(Screen::Ledger)>
                    <LedgerScreen lang documents clients profiles operations base backups router forms tax />
                </section>
                <section id=Screen::Clients.id() class="screen" hidden=hidden(Screen::Clients)>
                    <h1 tabindex="-1">{tr(lang, Key::ClientsTitle)}</h1>
                    <BackupChip lang backups nudge=true />
                    <ClientsSection lang clients documents base backups router forms />
                </section>
                <section id=Screen::Tax.id() class="screen" hidden=hidden(Screen::Tax)>
                    <TaxScreen ctx=tax router />
                </section>
                <section id=Screen::Annual.id() class="screen" hidden=hidden(Screen::Annual)>
                    <AnnualScreen ctx=tax router />
                </section>
                <section id=Screen::Deadlines.id() class="screen" hidden=hidden(Screen::Deadlines)>
                    <DeadlinesScreen ctx=tax router />
                </section>
                <section id=Screen::Report.id() class="screen" hidden=hidden(Screen::Report)>
                    <ReportScreen ctx=tax router />
                </section>
                <section id=Screen::Profile.id() class="screen" hidden=hidden(Screen::Profile)>
                    <h1 tabindex="-1">{tr(lang, Key::ProfileTitle)}</h1>
                    <BackupChip lang backups nudge=true />
                    <ProfileSection lang profiles licenses base backups router forms />
                    <h2>{tr(lang, Key::ProfileApp)}</h2>
                    <ul class="list">
                        <li>
                            <BackupEntry lang backups />
                        </li>
                        <li>
                            <OfficeEntry lang />
                        </li>
                        <li>
                            <LicenseEntry lang />
                        </li>
                        <li class="setting">
                            <span class="entry-title">{tr(lang, Key::Language)}</span>
                            <LangSwitch lang=window_lang />
                        </li>
                        <li>
                            <AboutEntry lang />
                        </li>
                    </ul>
                    <p class="hint privacy">{tr(lang, Key::ProfilePrivacy)}</p>
                </section>
                <section id=Screen::Backup.id() class="screen" hidden=hidden(Screen::Backup)>
                    <a class="back" href=Screen::Profile.href()>
                        {icon(Icon::Back)}
                        <span>{tr(lang, Key::NavProfile)}</span>
                    </a>
                    <h1 tabindex="-1">{tr(lang, Key::BackupTitle)}</h1>
                    <BackupSection lang backups storage forms router />
                    <StorageSection lang storage />
                </section>
                <section id=Screen::License.id() class="screen" hidden=hidden(Screen::License)>
                    <LicenseScreen ctx=tax router />
                </section>
                <section id=Screen::About.id() class="screen" hidden=hidden(Screen::About)>
                    <AboutScreen ctx=tax router />
                </section>
                <section id=Screen::Office.id() class="screen" hidden=hidden(Screen::Office)>
                    <OfficeScreen ctx=office_ctx router />
                </section>
            </main>
            {panic::debug_trigger()}
        </div>
        <UpdateDialog lang program />
    }
}

#[cfg(all(test, target_arch = "wasm32"))]
mod tests;
