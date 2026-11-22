//! ИП в программе (задача 38, этапы 2–4): у каждого ИП своя база IndexedDB
//! (`model::office`), открыт один, а список ИП — в базе конторы
//! (`store/office.rs`). Там же — ключи «Pro» (с 0.0.134): ключ открывает первых
//! N живых ИП списка по дню добавления (`license::seat`), и экран списка
//! называет тех, кому места не хватило. Введённый ключ «Pro» заводит контору,
//! если её ещё не было ([`Office::enter_pro`]). На том же экране — сводка
//! конторы (этап 5, [`Summary`]): у каждого ИП ближайший срок, прошлый квартал
//! и годовые платежи.
//!
//! Другой ИП открывается перезагрузкой страницы, а не заменой данных на
//! месте: `Base::replacements` не сбрасывает состояние экранов и запущенные
//! задачи, и счёт одного ИП лёг бы в список другого (разбор 2026-11-05).
//! Пока открыта форма, начат ввод или идёт запись, перезагрузка стёрла бы
//! начатое — переход отказывает словами, а не «переключит потом»:
//! отложенное сработало бы посреди работы.
//!
//! Главному wasm — только метка устройства, состояние и обёртки
//! (Правило 1). Пока всё жило в главном wasm, он вырос на 11,9 КБ после
//! brotli, а рабочий экран на телефонном профиле — на 0,1 с у всех (замер
//! задачи 38, 2026-11-06). Поэтому чтение списка при открытии и копия имени
//! ИП в список — своей маленькой частью ([`office_part`]): её грузят только
//! те, кто ведёт несколько ИП. Полоса над экранами и экран списка — частью
//! книги ([`view`]): разметка у них общая с экранами, и своя часть с ней
//! завела 24 новых общих куска (там же; CLAUDE.md, Правило 6), а часть книги
//! у кого есть данные, грузится при открытии и так — ради карточек налога.

use std::rc::Rc;
use std::sync::Arc;

use kvartik_core::i18n::{self, Key, Lang, fill, t};
use kvartik_core::license::{self, Access, License, MAX_KEYS, OfficeLicense, PUBLIC_KEY_BYTES};
use kvartik_core::model::date::Date;
use kvartik_core::model::document::Document;
use kvartik_core::model::ledger::Operation;
use kvartik_core::model::office::{self, Ip, MAX_IPS};
use kvartik_core::model::profile::Profile;
use kvartik_core::model::record::{RecordId, Timestamp};
use kvartik_core::model::tin::Tin;
use kvartik_core::release;
use kvartik_core::tax::closing::Closing;
use kvartik_core::tax::deadlines::{self, When};
use kvartik_core::tax::opening::Opening;
use kvartik_core::tax::summary::{self, Next, Previous, Yearly};
use kvartik_core::tax::turnover::Books;
use leptos::ev;
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::platform::{self, clock};
use crate::store::device::Device;
use crate::store::{DB_NAME, Event, Listed, Store, StoreError, Writes, office as office_store};

use super::backup::{BackupChip, Backups};
use super::form::Forms;
use super::icon::{Icon, icon};
use super::ledger::office_view;
use super::profile::{Profiles, name_for};
use super::route::{Router, Screen};
use super::split::{Task, screen, split};
use super::storage::error_key;
use super::tax::drams;
use super::{Base, tr};

/// Что окно знает о конторе.
#[derive(Clone, Copy)]
pub struct Office {
    /// База конторы — открыта, когда ИП больше одного или открыт её экран.
    store: StoredValue<Option<Rc<Store>>, LocalStorage>,
    /// Список: живые и убранные ИП; пусто — список не читали или ИП один.
    ips: RwSignal<Vec<Ip>>,
    /// Сколько записей списка не читается.
    unreadable: RwSignal<usize>,
    /// Имя открытой базы — одно на жизнь страницы, а сигналом — потому что
    /// решается при открытии, уже после первой отрисовки: по нему первые шаги
    /// узнают, скрыты ли они у этого ИП.
    current: RwSignal<String>,
    /// База выбрана — до этого `current` ещё не решён. Экран списка и полоса
    /// его ждут: экран, открытый вместе со страницей, строится раньше
    /// выбора, и «Открыть» встало бы у открытого ИП, а «открыт» — у другого
    /// (найдено проверкой собранного сайта, 2026-11-06).
    chosen: RwSignal<bool>,
    /// Что не вышло — словами на экране конторы.
    problem: RwSignal<Option<Problem>>,
    /// Идёт запись списка.
    busy: RwSignal<bool>,
    /// Список прочитан — тогда над экранами полоса, если ИП больше одного.
    listed: RwSignal<bool>,
    /// Запись списка — изменение данных для автокопии (задача 29).
    backups: Backups,
}

/// Чем кончился ввод ключа «Pro» ([`Office::enter_pro`]).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum ProEntry {
    /// Записан.
    Entered,
    /// Этот ключ уже введён.
    Known,
    /// Записей ключей уже `license::MAX_KEYS`: больше не бывает.
    Full,
}

/// Почему действие со списком не вышло.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Problem {
    /// Открыта форма, начат ввод или идёт запись.
    Busy,
    /// Больше [`MAX_IPS`] не бывает.
    TooMany,
    Store(StoreError),
}

impl Office {
    pub fn new(backups: Backups) -> Self {
        Self {
            store: StoredValue::new_local(None),
            ips: RwSignal::new(Vec::new()),
            unreadable: RwSignal::new(0),
            current: RwSignal::new(DB_NAME.to_owned()),
            chosen: RwSignal::new(false),
            problem: RwSignal::new(None),
            busy: RwSignal::new(false),
            listed: RwSignal::new(false),
            backups,
        }
    }

    /// Какую базу ИП открыть этой вкладке. Список читается, только когда
    /// устройство помнит, что ИП больше одного: иначе это база первого ИП,
    /// и ни часть конторы, ни её база первому заходу не нужны. Часть не
    /// загрузилась — тоже первая база: она есть всегда.
    pub async fn start(self) -> String {
        let name = if Device::new().office() {
            match office_part(OfficeJob::Start(self)).await {
                Ok(OfficeDone::Started(task)) => task.await,
                _ => DB_NAME.to_owned(),
            }
        } else {
            DB_NAME.to_owned()
        };
        self.chosen.set(true);
        name
    }

    /// Выбрать базу по списку (`office::choose` ядра) — уже в части.
    /// Выбранная запоминается вкладкой и устройством.
    async fn choose(self) -> String {
        let device = Device::new();
        let name = match self.open().await {
            Ok(store) => {
                // Список не прочитался — первая база: она есть всегда.
                let _ = self.read(&store).await;
                let tab = office_store::tab_base();
                let last = device.base();
                let name = self
                    .ips
                    .with_untracked(|ips| office::choose(ips, &[tab.as_deref(), last.as_deref()]).to_owned());
                self.list_open(&store, &name).await;
                name
            }
            Err(_) => DB_NAME.to_owned(),
        };
        office_store::set_tab_base(&name);
        device.set_base(&name);
        self.current.set(name.clone());
        name
    }

    /// Открытая база — в списке, если список есть, а её в нём нет: так
    /// бывает, когда все ИП убраны, и открывается первая база (`choose`
    /// ядра). Не в списке — её не покажут и не возьмут в копию всех ИП,
    /// а вписанное в неё пропало бы для всех копий (ревью 2026-11-06). Не
    /// записалось — попробует следующее открытие.
    async fn list_open(self, store: &Store, name: &str) {
        let missing = self
            .ips
            .with_untracked(|ips| !ips.is_empty() && !ips.iter().any(|ip| ip.base() == name));
        let Some(today) = clock::today().filter(|_| missing && name == DB_NAME) else {
            return;
        };
        let first = Ip::first(None, today, clock::now());
        let mut writes = Writes::default();
        writes.insert(&first);
        if office_store::write(store, writes).await.is_ok() {
            self.ips.update(|ips| ips.push(first));
        }
    }

    /// База конторы — открыть, если ещё не открыта.
    async fn open(self) -> Result<Rc<Store>, StoreError> {
        if let Some(store) = self.store.get_value() {
            return Ok(store);
        }
        let backups = self.backups;
        let store = Rc::new(
            Store::open_office(move |event| {
                if event == Event::Written {
                    backups.written();
                }
            })
            .await?,
        );
        self.store.set_value(Some(Rc::clone(&store)));
        Ok(store)
    }

    /// Прочитать список.
    async fn read(self, store: &Store) -> Result<(), StoreError> {
        let listed = office_store::list(store).await?;
        self.unreadable.set(listed.unreadable);
        self.ips.set(listed.records);
        self.listed.set(true);
        Ok(())
    }

    /// Следить за списком: кто зовёт это в эффекте, перезапустится, когда его
    /// перечитали или поправили, — так срок перечитывает ключи «Pro» после
    /// восстановления копии и ввода ключа в другой вкладке (`ui/license.rs`).
    pub(super) fn track(self) {
        self.ips.track();
    }

    /// Место открытого ИП среди живых ИП списка (`office::place`): `own` —
    /// базы ИП со своим действующим ключом, они стоят после остальных
    /// (задача 103). Отслеживаемое; `None` — списка нет или ИП в нём нет.
    pub(super) fn place(self, own: &[String]) -> Option<usize> {
        let current = self.current.get();
        self.ips.with(|ips| office::place(ips, &current, own))
    }

    /// Имя открытой базы — отслеживаемое.
    pub(super) fn current(self) -> String {
        self.current.get()
    }

    /// Базы живых ИП списка, у которых свой действующий в день `today` ключ —
    /// с верной подписью `public` и на ՀՎՀՀ ИП из списка (задача 103): места
    /// в ключе «Pro» они не занимают (`office::place`). Базы — по одной, как
    /// у сводки, и каждая открыта на время чтения, а открытая окном — `open`.
    /// База, которая не открылась или не прочиталась, — без своего ключа: место
    /// она займёт, но лишнего не откроет никому.
    pub(super) async fn own_keys(self, open: &Store, public: &[u8; PUBLIC_KEY_BYTES], today: Date) -> Vec<String> {
        let current = self.current.get_untracked();
        let alive: Vec<(String, Option<Tin>)> = self.ips.with_untracked(|ips| {
            ips.iter()
                .filter(|ip| !ip.is_removed())
                .map(|ip| (ip.base().to_owned(), ip.tin().cloned()))
                .collect()
        });
        let mut own = Vec::new();
        for (name, tin) in alive {
            let other;
            let store = if name == current {
                open
            } else {
                match Store::open_base(&name, |_| {}).await {
                    Ok(store) => {
                        other = store;
                        &other
                    }
                    Err(_) => continue,
                }
            };
            let read = async { Ok::<_, StoreError>(store.read()?.list::<License>().await?.records) };
            let Ok(licenses) = read.await else {
                continue;
            };
            let key = Access::new(public, License::keys(&licenses), tin.as_ref(), None).key();
            if key.is_some_and(|(_, last)| today <= last) {
                own.push(name);
            }
        }
        own
    }

    /// Ключи «Pro» конторы — для срока (`Licenses::load`). Конторы нет — ИП
    /// один — и ключей нет: базу конторы без метки устройства не открывают
    /// (`indexedDB.open` завёл бы её пустой).
    ///
    /// Соединение — открытое конторой, а нет его — своё, на это чтение, но не
    /// через [`Office::open`]: тот же код открытия делили бы часть конторы
    /// и все части, которые считают срок, и разрез переложил его — главный
    /// wasm вырос на 1 082 байта после brotli, а кусок, нужный части конторы при
    /// запуске, — на 5,4 КБ (замер задачи 38, 2026-11-16).
    pub(super) async fn pro_keys(self) -> Result<Listed<OfficeLicense>, StoreError> {
        if !Device::new().office() {
            return Ok(Listed {
                records: Vec::new(),
                unreadable: 0,
            });
        }
        let store = match self.store.get_value() {
            Some(store) => store,
            None => Rc::new(Store::open_office(|_| {}).await?),
        };
        office_store::keys(&store).await
    }

    /// Ввести ключ «Pro» (задача 38) — в базу конторы, одной транзакцией:
    /// прочитать список и введённые ключи, записать новый (команда ядра
    /// `OfficeLicense::enter`). Конторы ещё нет — ИП один, — и она заводится,
    /// как при «Добавить ИП»: в список ложится открытый ИП с именем из
    /// профиля, а устройство запоминает, что ведёт контору (`several_now`).
    /// Список потом перечитан, и срок перечитывает ключи сам ([`Office::track`]):
    /// и тогда, когда этот ключ уже ввели — в другой вкладке, — а окно его не
    /// знало (ревью задачи 38).
    pub(super) async fn enter_pro(self, key: license::Key, profile: Option<Arc<Profile>>) -> Result<ProEntry, StoreError> {
        let store = self.open().await?;
        let tx = store.write()?;
        let listed = tx.list::<Ip>().await?.records;
        let entered = tx.list::<OfficeLicense>().await?.records;
        let (today, now) = (clock::today(), clock::now());
        // Транзакция без записи кончается сама и ничего не пишет.
        let entry = match OfficeLicense::enter(key, &entered, now) {
            _ if entered.len() >= MAX_KEYS => {
                drop(tx);
                ProEntry::Full
            }
            Some(license) => {
                let mut writes = Writes::default();
                if listed.is_empty() {
                    writes.insert(&Ip::first(profile.as_deref(), today.ok_or(StoreError::Failed)?, now));
                }
                writes.insert(&license);
                tx.commit(writes).await?;
                ProEntry::Entered
            }
            None => {
                drop(tx);
                ProEntry::Known
            }
        };
        if entry != ProEntry::Full {
            self.several_now();
            self.read(&store).await?;
        }
        Ok(entry)
    }

    /// Копия всех ИП — когда устройство их ведёт несколько (`Device::office`):
    /// база конторы и имя открытой базы. Список не открылся — отказ, а не
    /// копия одного открытого ИП молча: человек считал бы, что в ней все.
    /// `None` — ИП один, и копия — его базы, как всегда.
    pub(super) async fn full(self) -> Result<Option<(Rc<Store>, String)>, StoreError> {
        if !Device::new().office() {
            return Ok(None);
        }
        let store = self.open().await?;
        if office_store::list(&store).await?.records.is_empty() {
            return Ok(None);
        }
        Ok(Some((store, self.current.get_untracked())))
    }

    /// База конторы для восстановления копии всех ИП: открыть или завести.
    pub(super) async fn store(self) -> Result<Rc<Store>, StoreError> {
        self.open().await
    }

    /// Имя открытой базы.
    pub(super) fn open_name(self) -> String {
        self.current.get_untracked()
    }

    /// Имя открытой базы — отслеживаемое.
    pub(super) fn base(self) -> String {
        self.current.get()
    }

    /// Перечитать список — после восстановления копии. Не прочитался —
    /// экран конторы скажет при открытии.
    pub(super) async fn refresh(self, store: &Store) {
        let _ = self.read(store).await;
    }

    /// Устройство теперь ведёт несколько ИП, а страница — прежнюю базу:
    /// метка устройства — и открытую базу помнят вкладка и устройство. Иначе
    /// следующая перезагрузка — без сети, новой версией — выбрала бы ИП по
    /// дню добавления, а у ИП копии он бывает один на всех, и открылся бы
    /// случайный (найдено проверкой собранного сайта, 2026-11-06).
    pub(super) fn several_now(self) {
        let device = Device::new();
        device.set_office();
        let name = self.current.get_untracked();
        office_store::set_tab_base(&name);
        device.set_base(&name);
    }

    /// Больше ли одного живого ИП — отслеживаемое: тогда окно называет
    /// открытого над каждым экраном.
    pub fn several(self) -> bool {
        // Счёт, а не `office::alive`: порядок здесь не нужен, а сортировка
        // списка в главном wasm — лишние байты каждого первого захода
        // (её зовёт экран копии; замер задачи 38).
        self.ips.with(|ips| ips.iter().filter(|ip| !ip.is_removed()).count() > 1)
    }

    /// Запись открытого ИП в списке; `None` — список не читали.
    fn current_ip(self) -> Option<Ip> {
        let current = self.current.get();
        self.ips.with(|ips| ips.iter().find(|ip| ip.base() == current).cloned())
    }

    /// Профиль открытой базы прочитан или сохранён — обновить его копию
    /// в списке: имена списка — оттуда, а открывать каждую базу ради них
    /// незачем (шапка `model::office`). Список не открыт — один ИП, и копии
    /// нет: часть конторы не грузится.
    pub fn profile_changed(self, profile: Arc<Profile>) {
        if self.store.get_value().is_none() {
            return;
        }
        spawn_local(async move {
            if let Ok(OfficeDone::Done(task)) = office_part(OfficeJob::Profile(self, profile)).await {
                task.await;
            }
        });
    }

    /// Копия имени ИП в список — уже в части. Не записалось — список покажет
    /// прежнее имя до следующего раза.
    async fn copy_profile(self, profile: Arc<Profile>) {
        let Some(store) = self.store.get_value() else {
            return;
        };
        let current = self.current.get_untracked();
        let Some(mut ip) = self
            .ips
            .with_untracked(|ips| ips.iter().find(|ip| ip.base() == current).cloned())
        else {
            return;
        };
        let before = ip.clone();
        if !ip.copy_profile(&profile, clock::now()) {
            return;
        }
        let mut writes = Writes::default();
        writes.update(&before, &ip);
        if office_store::write(&store, writes).await.is_ok() {
            self.ips.update(|ips| {
                if let Some(listed) = ips.iter_mut().find(|listed| listed.id() == ip.id()) {
                    *listed = ip;
                }
            });
        }
    }

    /// Можно ли сейчас перезагрузить страницу: не открыта форма, не начат
    /// ввод и ничего не записывается. Иначе — слова о том, почему нет.
    fn free(self, forms: Forms, base: Base) -> bool {
        let writing = base.get().is_some_and(|store| store.writing())
            || self.store.get_value().is_some_and(|store| store.writing());
        let free = !forms.busy() && !writing;
        if !free {
            self.problem.set(Some(Problem::Busy));
        }
        free
    }

    /// Открыть базу другого ИП: запомнить её вкладкой и устройством
    /// и перезагрузить страницу на том же экране — кроме карточки документа:
    /// документ — запись открытой базы, у другого ИП его нет.
    fn switch(self, to: &str) {
        office_store::set_tab_base(to);
        Device::new().set_base(to);
        if matches!(Screen::from_hash(&platform::hash()), Screen::Document(_)) {
            platform::set_hash(&Screen::Documents.href());
        }
        platform::reload();
    }

    /// Добавить ИП и открыть его. Первый раз в список ложится и тот, что был
    /// в программе до конторы, — с именем из его профиля: одной транзакцией
    /// с новым.
    async fn add(self, profile: Option<Arc<Profile>>) -> Result<Ip, Problem> {
        let store = self.open().await.map_err(Problem::Store)?;
        let tx = store.write().map_err(Problem::Store)?;
        let listed = tx.list::<Ip>().await.map_err(Problem::Store)?.records;
        let (today, now) = (clock::today(), clock::now());
        let Some(today) = today else {
            return Err(Problem::Store(StoreError::Failed));
        };
        let mut writes = Writes::default();
        let mut added = Vec::new();
        if listed.is_empty() {
            let first = Ip::first(profile.as_deref(), today, now);
            writes.insert(&first);
            added.push(first);
        }
        let new = Ip::new(today, now);
        writes.insert(&new);
        added.push(new.clone());
        if listed.len() + added.len() > MAX_IPS {
            return Err(Problem::TooMany);
        }
        tx.commit(writes).await.map_err(Problem::Store)?;
        self.several_now();
        self.ips.update(|ips| {
            *ips = listed;
            ips.extend(added);
        });
        Ok(new)
    }

    /// Убрать ИП из списка или вернуть — меткой, одной транзакцией.
    async fn mark(self, id: RecordId, change: fn(&mut Ip, Timestamp)) -> Result<(), StoreError> {
        let store = self.open().await?;
        let tx = store.write()?;
        let ips = tx.list::<Ip>().await?.records;
        if let Some(mut ip) = ips.into_iter().find(|ip| ip.id() == id) {
            let before = ip.clone();
            change(&mut ip, clock::now());
            let mut writes = Writes::default();
            writes.update(&before, &ip);
            tx.commit(writes).await?;
        }
        self.read(&store).await
    }
}

/// Имя ИП на языке окна, а нет его — «ИП без имени».
fn name(ip: &Ip, lang: Lang) -> String {
    match ip.name(lang) {
        "" => t(lang, Key::OfficeNameless).to_owned(),
        name => name.to_owned(),
    }
}

/// Работа части конторы — без разметки (шапка модуля).
pub(super) enum OfficeJob {
    /// При открытии окна: прочитать список и выбрать базу.
    Start(Office),
    /// Копию имени открытого ИП — в список.
    Profile(Office, Arc<Profile>),
}

/// Что дала часть конторы.
pub(super) enum OfficeDone {
    /// Имя базы — когда список прочитан.
    Started(Task<String>),
    /// Работа без ответа.
    Done(Task<()>),
}

split! {
    /// Контора — своей частью wasm (шапка модуля): у кого ИП один, её код
    /// не нужен ни главному wasm, ни при открытии.
    pub(super) fn office_part(job: OfficeJob) -> OfficeDone, preload preload_office {
        match job {
            OfficeJob::Start(office) => OfficeDone::Started(Box::pin(office.choose())),
            OfficeJob::Profile(office, profile) => OfficeDone::Done(Box::pin(office.copy_profile(profile))),
        }
    }
}

/// Что рисует часть книги для конторы.
#[derive(Clone, Copy)]
pub(super) enum OfficeView {
    /// Полоса над экранами.
    Bar(Ctx),
    /// Содержимое экрана «ИП в программе».
    Page(Ctx),
}

/// Полоса или экран списка — уже в части книги (`ledger::book_part`).
pub(super) fn view(view: OfficeView) -> AnyView {
    match view {
        OfficeView::Bar(ctx) => view! { <Bar lang=ctx.lang office=ctx.office /> }.into_any(),
        OfficeView::Page(ctx) => view! { <OfficePage ctx /> }.into_any(),
    }
}

/// Полоса над экранами (решение 2026-11-05): открытый ИП назван на каждом
/// экране, пока их больше одного, — счёт не выпишут не тому ИП. Главному wasm
/// — только ожидание части, когда список прочитан.
#[component]
pub fn OfficeBar(ctx: Ctx) -> impl IntoView {
    move || {
        (ctx.office.listed.get() && ctx.office.chosen.get())
            .then(|| Suspend::new(async move { office_view(OfficeView::Bar(ctx)).await.ok() }))
    }
}

/// Полоса — уже в части: только когда живых ИП больше одного.
#[component]
fn Bar(lang: ReadSignal<Lang>, office: Office) -> impl IntoView {
    move || {
        office.several().then(|| {
            view! {
                <p class="office-bar">
                    <span>
                        {move || {
                            let lang = lang.get();
                            let current = office.current_ip().map(|ip| name(&ip, lang)).unwrap_or_default();
                            fill(t(lang, Key::OfficeBar), &[&current])
                        }}
                    </span>
                    <a href=Screen::Office.href()>{tr(lang, Key::OfficeSwitch)}</a>
                </p>
            }
        })
    }
}

/// Строка «ИП в программе» в группе «Программа» на экране профиля — ссылка
/// на экран списка: сам список — в части конторы.
#[component]
pub fn OfficeEntry(lang: ReadSignal<Lang>) -> impl IntoView {
    view! {
        <a class="entry" href=Screen::Office.href()>
            <span class="entry-main">
                <span class="entry-title">{tr(lang, Key::OfficeTitle)}</span>
            </span>
            {icon(Icon::Forward)}
        </a>
    }
}

/// Что нужно экрану конторы: через границу части — одним значением.
#[derive(Clone, Copy)]
pub struct Ctx {
    pub lang: ReadSignal<Lang>,
    pub office: Office,
    pub profiles: Profiles,
    pub forms: Forms,
    pub base: Base,
    pub backups: Backups,
}

/// Экран «ИП в программе»: «назад» к профилю и заголовок — сразу, список —
/// частью книги с первого показа (`split::screen`).
#[component]
pub fn OfficeScreen(ctx: Ctx, router: Router) -> impl IntoView {
    let lang = ctx.lang;
    view! {
        <a class="back" href=Screen::Profile.href()>
            {icon(Icon::Back)}
            <span>{tr(lang, Key::NavProfile)}</span>
        </a>
        <h1 tabindex="-1">{tr(lang, Key::OfficeTitle)}</h1>
        <BackupChip lang backups=ctx.backups nudge=true />
        {screen(lang, router, Screen::Office, move || office_view(OfficeView::Page(ctx)))}
    }
}

/// Список ИП: живые — с открытым и «Открыть» у остальных, «Добавить ИП»,
/// убранные — с «Вернуть в список». Экран открыт — список нужен, даже если
/// устройство ведёт одного ИП: его читают и тогда.
#[component]
fn OfficePage(ctx: Ctx) -> impl IntoView {
    let Ctx {
        lang,
        office,
        profiles,
        forms,
        base,
        ..
    } = ctx;
    // Места ключей «Pro» конторы (задача 38) — свои, с подписью, проверенной
    // здесь, в части книги: `Licenses` в `Ctx` экрана стоили главному wasm
    // байтов (замер задачи 38). Перечитываются, когда перечитан список: ввод
    // ключа и восстановление копии перечитывают его.
    // И базы ИП со своим действующим ключом (задача 103): места они не
    // занимают — только когда ключ «Pro» есть, иначе базы не открываются.
    let seats = RwSignal::new(Vec::<(u8, Date)>::new());
    let own = RwSignal::new(Vec::<String>::new());
    let today = clock::today();
    Effect::new(move |_| {
        office.track();
        spawn_local(async move {
            if let Ok(keys) = office.pro_keys().await {
                let author = super::license::author();
                let found = license::pro_seats(&author, OfficeLicense::keys(&keys.records));
                if let (false, Some(today), Some(store)) = (found.is_empty(), today, base.get()) {
                    own.set(office.own_keys(&store, &author, today).await);
                }
                seats.set(found);
            }
        });
    });
    spawn_local(async move {
        let read = match office.open().await {
            Ok(store) => office.read(&store).await,
            Err(error) => Err(error),
        };
        if let Err(error) = read {
            office.problem.set(Some(Problem::Store(error)));
        }
    });
    let open = move |to: String| {
        office.problem.set(None);
        if office.free(forms, base) {
            office.switch(&to);
        }
    };
    let add = move |_| {
        office.problem.set(None);
        if office.busy.get_untracked() || !office.free(forms, base) {
            return;
        }
        office.busy.set(true);
        let profile = profiles.profile();
        spawn_local(async move {
            let added = office.add(profile).await;
            office.busy.set(false);
            match added {
                Ok(new) => office.switch(new.base()),
                Err(problem) => office.problem.set(Some(problem)),
            }
        });
    };
    let mark = move |id: RecordId, change: fn(&mut Ip, Timestamp)| {
        office.problem.set(None);
        if office.busy.get_untracked() {
            return;
        }
        office.busy.set(true);
        spawn_local(async move {
            let marked = office.mark(id, change).await;
            office.busy.set(false);
            if let Err(error) = marked {
                office.problem.set(Some(Problem::Store(error)));
            }
        });
    };
    // Ключ «Pro» конторы (задача 38): места — с подписью, проверенной один
    // раз на перерисовку (`Licenses::pro_seats`).
    let rows = move || {
        // База не выбрана — какой ИП открыт, ещё не решено (`chosen`).
        if !office.chosen.get() {
            return ().into_any();
        }
        let current = office.current.get();
        let lang = lang.get();
        let ips = office.ips.get();
        let alive = office::alive(&ips);
        // Идёт ключ «Pro» — у ИП без места в нём это сказано словами, а у ИП
        // со своим ключом — что ключ свой: места ему не нужно (задача 103).
        let seats = seats.get();
        let own = own.get();
        let pro = today.filter(|&today| seats.iter().any(|&(_, last)| today <= last));
        // Списка ещё нет — ИП один, тот, что открыт: его имя — из профиля.
        if alive.is_empty() {
            let named = profiles.with(|data| data.map(|data| name_for(&data.name, lang)).unwrap_or_default());
            let named = if named.is_empty() { t(lang, Key::OfficeNameless).to_owned() } else { named };
            return view! {
                <li class="entry">
                    <span class="entry-main">
                        <span class="entry-title">{named}</span>
                        <span class="entry-meta">{t(lang, Key::OfficeOpenNow)}</span>
                    </span>
                </li>
            }
            .into_any();
        }
        alive
            .into_iter()
            .map(|ip| {
                let is_current = ip.base() == current;
                let tin = ip.tin().map(|tin| fill(t(lang, Key::TinShort), &[tin.as_str()]));
                let to = ip.base().to_owned();
                let id = ip.id();
                let keyed = pro.is_some() && own.iter().any(|name| name == ip.base());
                let no_seat = !keyed
                    && pro.is_some_and(|today| {
                        license::seat(&seats, office::place(&ips, ip.base(), &own)).is_none_or(|(_, last)| last < today)
                    });
                view! {
                    <li class="entry office-ip" class:current=is_current>
                        <span class="entry-main">
                            <span class="entry-title">{name(ip, lang)}</span>
                            {tin.map(|tin| view! { <span class="entry-meta">{tin}</span> })}
                            {is_current.then(|| view! { <span class="entry-meta">{t(lang, Key::OfficeOpenNow)}</span> })}
                            {keyed.then(|| view! { <span class="entry-meta">{t(lang, Key::OfficeOwnKey)}</span> })}
                            {no_seat.then(|| view! { <span class="entry-meta">{t(lang, Key::OfficeNoSeat)}</span> })}
                        </span>
                        {(!is_current).then(|| {
                            let to = to.clone();
                            view! {
                                <span class="office-actions">
                                    <button type="button" on:click=move |_| open(to.clone())>
                                        <span class="label">{t(lang, Key::OfficeOpen)}</span>
                                    </button>
                                    <button
                                        type="button"
                                        class="link"
                                        disabled=move || office.busy.get()
                                        on:click=move |_| mark(id, Ip::remove)
                                    >
                                        <span class="label">{t(lang, Key::OfficeRemove)}</span>
                                    </button>
                                </span>
                            }
                        })}
                    </li>
                }
            })
            .collect_view()
            .into_any()
    };
    let removed = move || {
        let lang = lang.get();
        let ips = office.ips.get();
        let removed: Vec<&Ip> = ips.iter().filter(|ip| ip.is_removed()).collect();
        (!removed.is_empty()).then(|| {
            let rows = removed
                .into_iter()
                .map(|ip| {
                    let id = ip.id();
                    view! {
                        <li class="entry">
                            <span class="entry-main">
                                <span class="entry-title">{name(ip, lang)}</span>
                            </span>
                            <button
                                type="button"
                                class="secondary"
                                disabled=move || office.busy.get()
                                on:click=move |_| mark(id, Ip::bring_back)
                            >
                                <span class="label">{t(lang, Key::OfficeBringBack)}</span>
                            </button>
                        </li>
                    }
                })
                .collect_view();
            view! {
                <h2>{t(lang, Key::OfficeRemovedTitle)}</h2>
                <p class="hint">{t(lang, Key::OfficeRemovedAbout)}</p>
                <ul class="list">{rows}</ul>
            }
        })
    };
    let problem = move || {
        let lang = lang.get();
        office.problem.get().map(|problem| {
            let text = match problem {
                Problem::Busy => t(lang, Key::OfficeBusy).to_owned(),
                Problem::TooMany => fill(t(lang, Key::OfficeTooMany), &[&i18n::integer(lang, MAX_IPS as i64)]),
                Problem::Store(error) => t(lang, error_key(error)).to_owned(),
            };
            view! { <p role="alert">{text}</p> }
        })
    };
    let unreadable = move || {
        let count = office.unreadable.get();
        (count > 0).then(|| {
            view! { <p role="alert">{move || fill(t(lang.get(), Key::OfficeUnreadable), &[&count.to_string()])}</p> }
        })
    };
    // Идущий ключ «Pro» (задача 38): до какого дня и сколько в нём мест.
    let pro_term = move || {
        let today = today?;
        seats.with(|seats| seats.iter().copied().filter(|&(_, last)| today <= last).max_by_key(|&(_, last)| last))
    };
    let pro = move || {
        let (seats, last) = pro_term()?;
        let lang = lang.get();
        let words = fill(
            t(lang, Key::OfficePro),
            &[&i18n::date(lang, last), &i18n::integer(lang, i64::from(seats))],
        );
        Some(view! { <p class="hint">{words}</p> })
    };
    // Сводка (этап 5) — пока идёт ключ «Pro», пробный тоже (решение человека
    // 2026-11-05), и в пробный период без ключа, начатый до задачи 113: с
    // первого запуска на устройстве, первого дня данных или ответа сервера
    // о ՀՎՀՀ, которые оно помнит. Новых таких периодов нет
    // (`license::trial_last`).
    let trial_last = {
        let device = Device::new();
        license::trial_start(license::trial_start(device.trial_start(), device.trial_data()), device.trial_tin())
            .and_then(license::trial_last)
    };
    let summary_open = Signal::derive(move || {
        today.is_some_and(|today| trial_last.is_some_and(|last| today <= last))
            || pro_term().is_some()
            || super::license::open_for_tests()
    });
    view! {
        <p>{tr(lang, Key::OfficeAbout)}</p>
        {unreadable}
        <ul class="list">{rows}</ul>
        {pro}
        {problem}
        <button type="button" disabled=move || office.busy.get() on:click=add>
            <span class="label">{tr(lang, Key::OfficeAdd)}</span>
        </button>
        // Ключ — на ИНН одного ИП (задача 93) или один «Pro» на всех (задача
        // 103). Пока идёт ключ «Pro», «ключ у каждого свой» неправда: о нём —
        // строка выше (ревью задачи 38).
        <p class="hint">{move || {
            let lang = lang.get();
            let mut words = vec![t(lang, Key::OfficeAddAbout).to_owned()];
            if pro_term().is_none() {
                words.push(t(lang, Key::OfficeKeyEach).to_owned());
                words.push(fill(
                    t(lang, Key::OfficeKeyPro),
                    &[t(lang, Key::PlanPro), &release::PRO_SEATS.to_string(), t(lang, Key::OfficeTitle)],
                ));
            }
            words.join(" ")
        }}</p>
        <Summary lang office base today open=summary_open />
        {removed}
    }
}

/// Сводка конторы (этап 5): у каждого живого ИП — ближайший срок, прошлый
/// квартал и годовые платежи года ближайшего срока строкой ядра
/// (`tax::summary`), то есть теми же расчётами, что карточки главного экрана,
/// а итога по всем нет: каждый ИП платит своё. Только когда ИП больше одного
/// (одному всё это говорят карточки), в пробный период устройства и пока идёт
/// ключ «Pro» (`open`; решение человека 2026-11-05).
///
/// Базы читаются при каждом показе экрана и когда изменился список живых ИП:
/// пока экран был спрятан, в открытом ИП могли закрыть квартал, а в другой
/// вкладке — внести запись другому ИП. О показе часть узнаёт сама, по `hashchange`,
/// а не роутером: он в `Ctx` экрана стоил бы байтов главному wasm (Правило 1).
/// Базы — по одной, и каждое чтение IndexedDB отдаёт управление странице:
/// долгой работы без паузы нет и у тридцати ИП.
#[component]
fn Summary(lang: ReadSignal<Lang>, office: Office, base: Base, today: Option<Date>, open: Signal<bool>) -> impl IntoView {
    let shown = RwSignal::new(0_u32);
    let listener = window_event_listener(ev::hashchange, move |_| {
        if platform::hash() == Screen::Office.href() {
            shown.update(|count| *count = count.wrapping_add(1));
        }
    });
    on_cleanup(move || listener.remove());
    // ИП и его строка; `None` — сводка ещё читается.
    let rows = RwSignal::new(None::<Vec<(RecordId, Summed)>>);
    // Живые ИП и их базы — по значению: список перечитывают и тогда, когда
    // в нём ничего не изменилось (экран при показе, ввод ключа «Pro»), и
    // сводка читала бы все базы заново.
    let alive = Memo::new(move |_| {
        office.ips.with(|ips| {
            office::alive(ips)
                .into_iter()
                .map(|ip| (ip.id(), ip.base().to_owned()))
                .collect::<Vec<_>>()
        })
    });
    // Чтение, начатое раньше, бросает работу, когда началось новое.
    let run = StoredValue::new(0_u32);
    Effect::new(move |_| {
        shown.track();
        let alive = alive.get();
        let Some(today) = today.filter(|_| open.get() && office.chosen.get() && alive.len() > 1) else {
            return;
        };
        run.update_value(|count| *count = count.wrapping_add(1));
        let mine = run.get_value();
        let current = office.current.get_untracked();
        // В первый раз строки встают по одной, по мере чтения: у тридцати
        // ИП ждать всех — секунды пустого экрана. Потом прежние строки стоят,
        // пока не прочитаны все новые: без мигания.
        let growing = rows.with_untracked(Option::is_none);
        spawn_local(async move {
            let mut found = Vec::with_capacity(alive.len());
            for (id, name) in alive {
                let row = row_of(&name, &current, base, today).await;
                if run.get_value() != mine {
                    return;
                }
                found.push((id, row));
                if growing {
                    rows.set(Some(found.clone()));
                }
            }
            if !growing {
                rows.set(Some(found));
            }
        });
    });
    move || {
        office.several().then(|| {
            let lang = lang.get();
            let body = if !open.get() {
                view! { <p class="hint">{t(lang, Key::OfficeSummaryLocked)}</p> }.into_any()
            } else if let Some(found) = rows.get() {
                let items = found
                    .into_iter()
                    .map(|(id, row)| {
                        let named = office
                            .ips
                            .with(|ips| ips.iter().find(|ip| ip.id() == id).map(|ip| name(ip, lang)))
                            .unwrap_or_default();
                        let row = match row {
                            Ok(Some(row)) => lines(lang, row).into_any(),
                            Ok(None) => view! { <span class="entry-meta">{t(lang, Key::OfficeSummaryEmpty)}</span> }.into_any(),
                            Err(_) => view! { <span class="entry-meta">{t(lang, Key::OfficeSummaryFailed)}</span> }.into_any(),
                        };
                        view! {
                            <li class="entry">
                                <span class="entry-main">
                                    <span class="entry-title">{named}</span>
                                    {row}
                                </span>
                            </li>
                        }
                    })
                    .collect_view();
                view! { <ul class="list office-summary">{items}</ul> }.into_any()
            } else {
                view! { <p class="hint">{t(lang, Key::PartLoading)}</p> }.into_any()
            };
            view! {
                <h2>{t(lang, Key::OfficeSummaryTitle)}</h2>
                <p class="hint">{t(lang, Key::OfficeSummaryAbout)}</p>
                {body}
            }
        })
    }
}

/// Что сводка знает об ИП: строку ядра; `Ok(None)` — в его базе нет ни одной
/// записи: ИП только что завели, и без профиля и книги сроков и налога не
/// узнать — «к уплате 0 ֏» и «срок прошёл» были бы неправдой, как и на главном
/// экране, где у пустой базы карточек нет; `Err` — база не открылась или не
/// прочиталась.
type Summed = Result<Option<summary::Row>, StoreError>;

/// Строка сводки ИП с базой `name` в день `today`: открытого — по базе окна,
/// остальных — открыв их базу на время чтения ([`Summed`]). Книга — как её
/// собирает окно для налога (`Sources` в `ui/tax.rs`): нечитаемые документы
/// и записи книги делают расчёт неполным.
async fn row_of(name: &str, current: &str, base: Base, today: Date) -> Summed {
    let store = match base.get().filter(|_| name == current) {
        Some(store) => store,
        None => Rc::new(Store::open_base(name, |_| {}).await?),
    };
    let tx = store.read()?;
    if tx.is_empty().await? {
        return Ok(None);
    }
    let documents = tx.list::<Document>().await?;
    let operations = tx.list::<Operation>().await?;
    let profile = Profile::current(tx.list::<Profile>().await?.records);
    let closings = tx.list::<Closing>().await?.records;
    let openings = tx.list::<Opening>().await?.records;
    let books = Books {
        documents: &documents.records,
        operations: &operations.records,
        profile: profile.as_ref().map(Profile::data),
        closings: &closings,
        openings: &openings,
        unreadable: documents.unreadable + operations.unreadable,
    };
    Ok(Some(summary::row(books, today)))
}

/// Строки сводки одного ИП — словами карточек главного экрана: ближайший срок
/// — как напоминание о сроках, прошлый квартал — как карточка налога, годовые
/// платежи — как их карточка. Чего нет, той строки нет.
fn lines(lang: Lang, row: summary::Row) -> impl IntoView {
    let next = row.next.map(|Next { deadline, when }| {
        // Срок прошёл — словами и цветом напоминания: цвет один ничего не
        // говорит (Правило 4).
        let overdue = when == When::Overdue;
        view! {
            <span class="entry-meta" class:due=overdue>
                <span>{super::deadlines::when(lang, when, deadline.day)}</span>
                <span>{deadlines::what(lang, deadline.kind)}</span>
            </span>
        }
    });
    let previous = row.previous.map(|previous| {
        let (quarter, words) = match previous {
            Previous::Filed { quarter, tax } => (quarter, vec![t(lang, Key::DeadlineDone).to_owned(), drams(lang, tax)]),
            Previous::ToPay {
                quarter,
                tax,
                complete,
                due,
            } => {
                let mut words = vec![fill(t(lang, Key::TaxToPayBy), &[&i18n::date(lang, due.day)]), drams(lang, tax)];
                if !complete {
                    words.push(t(lang, Key::AnnualPartialShort).to_owned());
                }
                (quarter, words)
            }
            Previous::Left { quarter, from, micro } => {
                let key = if micro { Key::TaxLeftMicroShort } else { Key::TaxLeftShort };
                (quarter, vec![fill(t(lang, key), &[&i18n::date(lang, from)])])
            }
        };
        view! {
            <span class="entry-meta">
                <span>{i18n::quarter(lang, quarter)}</span>
                {spans(words)}
            </span>
        }
    });
    let annual = row.annual.map(|Yearly { year, total }| {
        let mut words = vec![fill(t(lang, Key::AnnualFor), &[&year.to_string()])];
        words.extend(total.sum.map(|sum| drams(lang, sum)));
        words.extend(
            total
                .due
                .map(|due| fill(t(lang, Key::AnnualDue), &[&i18n::date(lang, deadlines::last_day(due))])),
        );
        if !total.complete {
            words.push(t(lang, Key::AnnualPartialShort).to_owned());
        }
        view! { <span class="entry-meta">{spans(words)}</span> }
    });
    (next, previous, annual)
}

/// Слова — частями строки, через точку (`.entry-meta` в `base.css`).
fn spans(words: Vec<String>) -> impl IntoView {
    words.into_iter().map(|words| view! { <span>{words}</span> }).collect_view()
}
