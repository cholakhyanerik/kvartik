//! Раздел «Резервная копия» (задача 7): сделать копию под паролем,
//! восстановить из неё, напомнить, если копии давно не было.
//!
//! Раздел — содержимое своего экрана внутри профиля (`ui/route.rs`), а
//! напоминание о копии — вверху остальных экранов. Экран — своей частью wasm
//! с первого показа (задача 91): открывают его редко, а в главном wasm он
//! вместе с общим кодом, который держал там только он, стоил 28 КБ после
//! brotli — 0,19 с рабочего экрана на телефоне (замер 0.0.142,
//! docs/measurements.md). Главному wasm остались дни копий, напоминание,
//! метки и строка профиля ([`Backups`], [`BackupChip`], [`BackupEntry`]).
//!
//! Копия делается в два шага: собрать и зашифровать — потом сохранить
//! ссылкой, которую человек нажимает сам (`platform::file::Download`), а на
//! iPhone и iPad ещё и отдать меню «Поделиться» (`platform::share`).
//! Пароль спрашивается при каждой копии, чтобы не забылся, а опечатку ловит
//! проверочное значение устройства (`DeviceKey`).
//!
//! Автокопия (задача 29) — тот же файл, который окно само пишет в свою папку
//! «Kvartik» внутри выбранной человеком: после изменений, хотя бы раз в день
//! открытой программы и когда страница уходит с глаз (`format::autobackup`).
//! Ключ из пароля копий со своей солью браузер держит неизвлекаемым в базе
//! устройства (`store::autobackup`), а раз в месяц окно просит пароль
//! ввести, чтобы он не забылся (решение человека 2026-11-03). Сделанная
//! автокопия — это копия: она сдвигает день последней. Встала — день
//! перестанет сдвигаться, и напоминание вернётся через неделю, как без неё
//! (решение человека 2026-11-03).
//!
//! Работает автокопия в частях wasm: экран и карточки — в части книги,
//! база устройства, папка и шифр — в части копии; главному wasm остались
//! ручка её состояния и событие записи в базу (замер задачи 29, Правило 1).
//!
//! Файлы раздела — по делу (задача 119): здесь состояние копий
//! ([`Backups`]), `Problem`, `Ctx`, задания частей и все точки разреза wasm
//! (`split!`); сделать копию и напоминания о ней — `make`, автокопия —
//! `auto`, восстановление — `restore`, куда ложится копия другого ИП и копия
//! всех ИП — `merge`, браузерные тесты — `tests`. Соседи видят друг друга
//! через `use super::*`, а типы, поля которых читают несколько файлов, лежат
//! здесь: личные поля родителя дочерний модуль видит, а соседа — нет.

use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use kvartik_core::format::autobackup::{self, DeviceTag};
use kvartik_core::format::backup::{
    self, BackupError, CopyId, DeviceKey, Entry, FILE_KINDS, Files, Header, KDF_ITERATIONS, KINDS, MAX_FILE_BYTES,
    Laid, MIN_PASSWORD_CHARS, Owner, Part, PasswordProblem, SALT_BYTES, Sealed, Tally,
};
use kvartik_core::i18n::{self, Key, Lang, fill, t};
use kvartik_core::release::EMAIL;
use kvartik_core::model::date::Date;
use kvartik_core::model::office::{Ip, MAX_IPS};
use kvartik_core::model::profile::Profile;
use kvartik_core::model::record::Timestamp;
use leptos::ev::{Event, MouseEvent, SubmitEvent};
use leptos::prelude::*;
use leptos::wasm_split::SplitLoaderError;
use web_sys::{FileSystemDirectoryHandle, HtmlInputElement};

use super::form::Forms;
use super::icon::{Icon, icon};
use super::office::Office;
use super::password::{PasswordInput, PasswordLogin, PasswordShow, conceal};
use super::route::{Router, Screen};
use super::split::{Task, screen, split};
use super::storage::{Storage, error_key};
use super::{Base, tr};
use crate::platform::autobackup::{self as folders, FolderError, Permission, PickError};
use crate::platform::clock;
use crate::platform::crypto::{self, CryptoError};
use crate::platform::file::{self, Download, FileError};
use crate::platform::pause::breathe;
use crate::platform::share::{self, ShareError};
use crate::store::{Store, StoreError};
use crate::store::autobackup::{self as remembered, CopyKey, Saved};
use crate::store::backup::{self as stored, Failure, Merged, OtherIe, Restored, Sealing};
use crate::store::device::Device;
use crate::store::office::list as office_list;

mod auto;
mod make;
mod merge;
mod restore;

use auto::*;
use make::*;
use merge::*;
use restore::*;

pub use make::{BackupChip, BackupEntry, BackupReminder};
pub(super) use auto::{AutoCards, AutoView, auto_view};
pub(super) use make::chip;
pub(super) use merge::{IssuerAsk, arrived, unlisted, who};
use kvartik_core::format::office as office_route;

/// Что окно знает о копиях.
#[derive(Clone, Copy)]
pub struct Backups {
    /// Открытая база.
    base: Base,
    /// День последней копии на этом устройстве — по его календарю.
    last: RwSignal<Option<Date>>,
    /// Задан ли на устройстве пароль копий.
    key_set: RwSignal<bool>,
    /// Есть ли в базе хоть одна запись.
    has_data: RwSignal<bool>,
    /// Сегодня — один раз при открытии (Архитектура, «Время — параметр»).
    today: Option<Date>,
    /// Автокопия в папку (задача 29) и контора (задача 38) — одной
    /// ручкой: `Backups` лежит в каждом разделе, и его размер держит `clippy`
    /// (`large_enum_variant` у частей профиля).
    auto: StoredValue<AutoParts, LocalStorage>,
}

/// Где автокопия.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
enum AutoState {
    /// Выбора папки здесь нет — только ручная копия.
    Unavailable,
    /// Ещё не прочитано, включена ли.
    #[default]
    Loading,
    /// Выключена.
    Off,
    /// Включена, и вот что браузер разрешает с папкой.
    On(Permission),
}

/// Что мешает автокопии. Без слов: слова — у экрана копии ([`Hitch::problem`]),
/// а главному wasm строки автокопии на трёх языках ни к чему (Правило 1).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Hitch {
    /// Браузер ждёт разрешения на запись: спросить человека.
    Ask,
    /// Писать в эту папку браузер не даёт — нужна другая.
    Denied,
    /// Папки больше нет.
    Gone,
    /// На диске нет места.
    Full,
    /// Пароль копий сменился, а ключ автокопии — от прежнего.
    PasswordChanged,
    /// Настройки автокопии на устройстве не читаются.
    Damaged,
    /// Хранилище отказало.
    Store(StoreError),
    /// Копия не уложилась и частями (`store::backup::Failure::TooLarge`).
    TooLarge,
    /// Шифра нет: страница открыта не по HTTPS.
    NoCrypto,
    /// Прочее; подробность — в консоли.
    Failed,
}

impl Hitch {
    /// Ждёт ли беда человека: папку выбрать заново, автокопию включить
    /// снова. Тогда автокопия не пробует сама после каждого изменения,
    /// а прочее — нет места, сбой — пробует.
    const fn stops(self) -> bool {
        matches!(self, Self::Denied | Self::Gone | Self::PasswordChanged | Self::Damaged)
    }

    /// Что сказать человеку — у экрана копии.
    fn problem(self) -> Problem {
        Problem::Say(match self {
            Self::Ask => Key::AutoAsk,
            Self::Denied => Key::AutoDenied,
            Self::Gone => Key::AutoGone,
            Self::Full => Key::AutoFull,
            Self::PasswordChanged => Key::AutoPasswordChanged,
            Self::Damaged => Key::AutoDamaged,
            Self::Store(error) => error_key(error),
            Self::TooLarge => return Problem::Oversized,
            Self::NoCrypto => Key::BackupNoCrypto,
            Self::Failed => Key::AutoFailed,
        })
    }
}

impl From<FolderError> for Hitch {
    fn from(error: FolderError) -> Self {
        match error {
            FolderError::NotAllowed => Self::Ask,
            FolderError::Blocked => Self::Denied,
            FolderError::Gone => Self::Gone,
            FolderError::Full => Self::Full,
            FolderError::Failed => Self::Failed,
        }
    }
}

impl From<StoreError> for Hitch {
    fn from(error: StoreError) -> Self {
        match error {
            StoreError::Damaged => Self::Damaged,
            error => Self::Store(error),
        }
    }
}

impl From<Failure> for Hitch {
    fn from(failure: Failure) -> Self {
        match failure {
            Failure::Store(error) => Self::Store(error),
            Failure::Crypto(CryptoError::Unavailable) => Self::NoCrypto,
            Failure::Crypto(_) | Failure::File(_) | Failure::TooManyIps(_) | Failure::TooManyKeys | Failure::Handed => {
                Self::Failed
            }
            Failure::TooLarge => Self::TooLarge,
        }
    }
}

/// Что окно знает об автокопии — одним значением, а не сигналом на каждое
/// поле: каждый тип сигнала Leptos несёт свою копию общего кода (замер
/// задачи 29, Правило 1).
#[derive(Clone, PartialEq, Debug, Default)]
struct Auto {
    state: AutoState,
    /// Имя папки — как его даёт браузер, без пути.
    folder: Option<String>,
    /// Когда последняя удавшаяся автокопия начала читать базу.
    last: Option<Timestamp>,
    /// Пишется ли автокопия сейчас.
    running: bool,
    /// Что помешало последней попытке; `None` — всё хорошо.
    hitch: Option<Hitch>,
    /// Сколько записей не читается и не вошло в последнюю автокопию.
    unreadable: usize,
    /// Есть ли изменения, которых в последней автокопии нет
    /// (`format::autobackup::pending`): ждущая разрешения автокопия говорит
    /// о себе на главном экране, только когда ей есть что писать.
    pending: bool,
    /// День, когда в последний раз ввели пароль копий и тот подошёл.
    entered: Option<Date>,
}

/// Всё об автокопии — за одной ручкой [`Backups`]. И контора: копия всех ИП,
/// когда их несколько (задача 38); ставит её окно ([`Backups::attach`]).
struct AutoParts {
    /// Где автокопия.
    state: RwSignal<Auto>,
    /// Её объекты браузера.
    handles: Handles,
    office: Option<Office>,
    /// Ключ пароля копий, только что введённого для кода передачи (задача
    /// 124, п. 18): экран копии соберёт им ручную копию без второго ввода.
    /// Берётся один раз ([`Backups::take_handoff`]).
    handoff: Option<Rc<crypto::Key>>,
}

/// Объекты браузера автокопии и её будильник.
#[derive(Default)]
struct Handles {
    /// Работает ли автокопия в этом окне: её запускает часть книги, и не
    /// дважды ([`Backups::start_auto`]).
    started: bool,
    /// Папка включённой автокопии — под рукой, а не в базе устройства:
    /// разрешение на запись спрашивают в самом нажатии, до первого `await`
    /// (`platform::autobackup::ask`).
    folder: Option<FileSystemDirectoryHandle>,
    /// Таймер следующей автокопии: он один и только когда ей есть что
    /// писать, в покое его нет (Правило 1).
    timer: Option<TimeoutHandle>,
    /// Что позвать после записи в базу — работающая автокопия; пока она не
    /// запущена, запись только помнится на устройстве.
    on_write: Option<Rc<dyn Fn()>>,
}

impl Backups {
    /// Что известно до открытия базы: последняя копия и пароль — из
    /// хранилища устройства, сегодня — из часов. Автокопия здесь ещё ничего
    /// не знает: её прочитает часть книги, когда запустит.
    pub fn new(base: Base) -> Self {
        let device = Device::new();
        Self {
            base,
            last: RwSignal::new(device.last_backup().and_then(clock::local_date)),
            key_set: RwSignal::new(device.backup_key().is_some()),
            has_data: RwSignal::new(false),
            today: clock::today(),
            auto: StoredValue::new_local(AutoParts {
                state: RwSignal::new(Auto::default()),
                handles: Handles::default(),
                office: None,
                handoff: None,
            }),
        }
    }

    /// Файлов больше, чем вмещает серия кодов (задача 124, п. 18): экран
    /// копии соберёт ручную копию этим ключом — пароль копий человек только
    /// что ввёл для кода, и второй раз его не спрашивают. Тот же ключ: код
    /// и копию шифрует пароль копий с солью устройства (`key_for`). Ключ
    /// встречного кода (задача 124, п. 14) — пароля того устройства: не тот,
    /// что здесь, — не берётся, и экран копии спросит пароль, как всегда,
    /// а не скажет «пароль не тот» о пароле, которого человек не вводил.
    pub(super) fn hand_off(&self, key: Rc<crypto::Key>) {
        if Device::new().backup_key().is_some_and(|device_key| device_key.accepts(key.check())) {
            self.auto.update_value(|parts| parts.handoff = Some(key));
        }
    }

    /// Ключ, переданный кодом передачи, — один раз.
    fn take_handoff(&self) -> Option<Rc<crypto::Key>> {
        self.auto.try_update_value(|parts| parts.handoff.take()).flatten()
    }

    /// Контора заведена — копия теперь знает о ней.
    pub fn attach(&self, office: Office) {
        self.auto.update_value(|parts| parts.office = Some(office));
    }

    /// Контора; `None` — ещё не заведена.
    pub(super) fn office(&self) -> Option<Office> {
        self.auto.with_value(|parts| parts.office)
    }

    /// Состояние автокопии.
    fn auto(&self) -> RwSignal<Auto> {
        self.auto.with_value(|parts| parts.state)
    }

    /// Прочитать объекты браузера автокопии.
    fn handles<T>(&self, read: impl FnOnce(&Handles) -> T) -> T {
        self.auto.with_value(|parts| read(&parts.handles))
    }

    /// Изменить их.
    fn handles_mut(&self, change: impl FnOnce(&mut Handles)) {
        self.auto.update_value(|parts| change(&mut parts.handles));
    }

    /// Действие записано в базу (`store::Event::Written`): момент помнится
    /// на устройстве — по нему автокопия знает, что ей есть что писать, хоть
    /// бы и при следующем открытии, — а работающая автокопия просыпается.
    pub fn written(&self) {
        Device::new().set_changed(clock::now());
        if let Some(wake) = self.handles(|handles| handles.on_write.clone()) {
            wake();
        }
    }

    /// Пароль копий только что ввели, и он подошёл: следующая просьба его
    /// ввести — через месяц (`format::autobackup::check_due`).
    pub(super) fn password_entered(&self) {
        let now = clock::now();
        Device::new().set_password_entered(now);
        let day = clock::local_date(now);
        self.auto().update(|auto| auto.entered = day);
    }

    /// Задан ли на устройстве пароль копий — отслеживаемое: без него форма
    /// просит новый с повтором.
    pub(super) fn key_set(&self) -> bool {
        self.key_set.get()
    }

    /// Запомнить пароль копий устройства — новый, заданный для кода
    /// передачи, или пароль принятого кода, когда своего не было
    /// (`ui/sync.rs`). Запись не легла — пароль остаётся незаданным.
    pub(super) fn password_set(&self, key: &DeviceKey) {
        if Device::new().set_backup_key(key) {
            self.key_set.set(true);
        }
    }

    /// База открылась, и в ней есть данные — или нет.
    pub fn opened(&self, has_data: bool) {
        self.has_data.set(has_data);
    }

    /// Есть ли в базе хоть одна запись — отслеживаемое: по нему главный
    /// экран грузит часть налога (`ui/tax.rs`).
    pub fn has_data(&self) -> bool {
        self.has_data.get()
    }

    /// Действие сохранило данные: теперь есть что терять, и без копии
    /// напоминание о ней появляется сразу, а не со следующим открытием.
    pub fn data_saved(&self) {
        self.has_data.set(true);
    }

    /// Пора ли напомнить о копии (Правило 2). Часы без даты — не повод.
    /// Сделанная автокопия — копия: она сдвигает день последней, а встанет —
    /// день перестанет сдвигаться, и напоминание вернётся (решение человека
    /// 2026-11-03).
    fn due(&self) -> bool {
        self.today
            .is_some_and(|today| backup::reminder_due(self.last.get(), today, self.has_data.get()))
    }
}

/// Автокопия за работой — в части книги: её зовут только экран копии
/// и карточки главного экрана оттуда (`AutoSection`, `AutoCards`).
impl Backups {
    /// Изменить состояние автокопии.
    fn edit(&self, change: impl FnOnce(&mut Auto)) {
        self.auto().update(change);
    }

    /// Запустить автокопию в этом окне: прочитать, что помнит устройство,
    /// включена ли она и что браузер разрешает с её папкой, — и проснуться
    /// после каждой записи в базу и когда страница уходит с глаз. Один раз.
    fn start_auto(&self) {
        if self.handles(|handles| handles.started) {
            return;
        }
        self.handles_mut(|handles| handles.started = true);
        if !folders::available() {
            self.edit(|auto| auto.state = AutoState::Unavailable);
            return;
        }
        let device = Device::new();
        let last = device.last_auto();
        let pending = autobackup::pending(device.changed(), last);
        let entered = device.password_entered().and_then(clock::local_date);
        self.edit(|auto| {
            auto.last = last;
            auto.pending = pending;
            auto.entered = entered;
        });
        let backups = *self;
        let wake: Rc<dyn Fn()> = Rc::new(move || backups.woken());
        self.handles_mut(|handles| handles.on_write = Some(wake));
        // Ушли с вкладки или закрывают окно — записать, что накопилось, не
        // дожидаясь таймера: заход ради одной оплаты короче минуты
        // (ревью задачи 29). Закрытие страницы запись может оборвать —
        // тогда прежний файл цел, а накопленное запишет следующее открытие.
        folders::when_hidden(move || backups.flush());
        leptos::task::spawn_local(async move {
            match finished(making_task(backups, Job::Status).await).await {
                Made::Status(Some(place)) => backups.found(place),
                Made::Refused(hitch) => backups.edit(|auto| {
                    auto.state = AutoState::Off;
                    auto.hitch = Some(hitch);
                }),
                _ => backups.edit(|auto| auto.state = AutoState::Off),
            }
            backups.schedule();
        });
    }

    /// Папка включённой автокопии — и что браузер с ней разрешает.
    fn found(&self, place: Place) {
        let Place { folder, name, permission } = place;
        self.handles_mut(|handles| handles.folder = Some(folder));
        self.edit(|auto| {
            auto.folder = Some(name);
            auto.state = AutoState::On(permission);
        });
    }

    /// В базу записали: автокопии есть что писать.
    fn woken(&self) {
        if !self.auto().with_untracked(|auto| auto.pending) {
            self.edit(|auto| auto.pending = true);
        }
        self.schedule();
    }

    /// Спросить у человека разрешение писать в папку автокопии. Зовётся
    /// прямо в обработчике нажатия: вопрос задаётся до первого `await`
    /// (`platform::autobackup::ask`).
    fn allow(&self) {
        let Some(asking) = self.handles(|handles| handles.folder.as_ref().map(folders::ask)) else {
            return;
        };
        let backups = *self;
        leptos::task::spawn_local(async move {
            let permission = asking.await;
            backups.edit(|auto| {
                auto.state = AutoState::On(permission);
                if permission == Permission::Granted {
                    auto.hitch = None;
                }
            });
            backups.schedule();
        });
    }

    /// Ждёт ли автокопия разрешения, когда ей есть что писать: об этом
    /// главный экран говорит сразу, а не через неделю — разрешение Chrome
    /// снимает, как закрыта последняя вкладка (`platform::autobackup`).
    fn waits_for_permission(&self) -> bool {
        self.auto()
            .with(|auto| auto.state == AutoState::On(Permission::Prompt) && auto.pending)
    }

    /// Может ли автокопия писать сама: включена, запись разрешена, она не
    /// пишется сейчас и не ждёт человека.
    fn can_run(&self) -> bool {
        self.auto().with_untracked(|auto| {
            auto.state == AutoState::On(Permission::Granted)
                && !auto.running
                && !auto.hitch.is_some_and(Hitch::stops)
        })
    }

    /// Нужна ли автокопия сейчас: есть изменения, которых в ней нет, — или
    /// сегодня её ещё не было (`format::autobackup::refresh_due`). Первое —
    /// `true` во втором.
    fn needed(&self) -> Option<bool> {
        let device = Device::new();
        let done = device.last_auto();
        let now = clock::now();
        if autobackup::pending(device.changed(), done) {
            return Some(true);
        }
        let today = clock::local_date(now)?;
        autobackup::refresh_due(done, now, done.and_then(clock::local_date), today).then_some(false)
    }

    /// Поставить автокопию в очередь, если она нужна и может писать: после
    /// изменения — через минуту и не чаще раза в десять
    /// (`format::autobackup::delay`), а раз в день без изменений — вскоре.
    fn schedule(&self) {
        if !self.can_run() || self.handles(|handles| handles.timer.is_some()) {
            return;
        }
        let Some(changed) = self.needed() else {
            return;
        };
        let wait = if changed {
            autobackup::delay(Device::new().last_auto(), clock::now())
        } else {
            autobackup::REFRESH_WAIT_MILLIS
        };
        let backups = *self;
        let handle = set_timeout_with_handle(
            move || {
                backups.handles_mut(|handles| handles.timer = None);
                // Пока таймер ждал, нужное могла сделать первая копия
                // в новую папку (`act`) или уход со вкладки.
                if backups.needed().is_some() {
                    backups.run_auto();
                }
            },
            Duration::from_millis(wait.unsigned_abs()),
        );
        self.handles_mut(|handles| handles.timer = handle.ok());
    }

    /// Страница ушла с глаз: записать автокопию сейчас, если она нужна.
    fn flush(&self) {
        if !self.can_run() || self.needed().is_none() {
            return;
        }
        self.handles_mut(|handles| {
            if let Some(timer) = handles.timer.take() {
                timer.clear();
            }
        });
        self.run_auto();
    }

    /// Записать автокопию сейчас — одной вкладкой за раз
    /// (`platform::autobackup::alone`). После — снова в очередь: пока она
    /// писалась, могли прийти новые изменения.
    fn run_auto(&self) {
        if self.auto().with_untracked(|auto| auto.running) {
            return;
        }
        self.edit(|auto| auto.running = true);
        let backups = *self;
        leptos::task::spawn_local(async move {
            let work = async move { finished(making_task(backups, Job::Auto).await).await };
            let made = folders::alone(AUTO_LOCK, work)
                .await
                .unwrap_or(Made::Ran(Err(Hitch::Failed)));
            backups.edit(|auto| auto.running = false);
            backups.ran(made);
            backups.schedule();
        });
    }

    /// Чем кончилась автокопия. Её момент устройство уже запомнило — сама
    /// автокопия, под той же блокировкой (`write_file`).
    fn ran(&self, made: Made) {
        match made {
            Made::Ran(Ok(Wrote { at, unreadable })) => {
                self.last.set(clock::local_date(at));
                let pending = autobackup::pending(Device::new().changed(), Some(at));
                self.edit(|auto| {
                    auto.last = Some(at);
                    auto.unreadable = unreadable;
                    auto.hitch = None;
                    auto.pending = pending;
                });
            }
            Made::Ran(Err(Hitch::Ask)) => self.edit(|auto| {
                auto.state = AutoState::On(Permission::Prompt);
                auto.hitch = None;
            }),
            Made::Ran(Err(Hitch::Denied)) => self.edit(|auto| {
                auto.state = AutoState::On(Permission::Denied);
                auto.hitch = None;
            }),
            Made::Ran(Err(hitch)) | Made::Refused(hitch) => self.edit(|auto| auto.hitch = Some(hitch)),
            // Выключили — может быть, в другой вкладке.
            Made::Off => self.forgotten(),
            Made::File(_) | Made::Status(_) | Made::Checked => {}
        }
    }

    /// Автокопия выключена: окно забывает папку и таймер.
    fn forgotten(&self) {
        self.handles_mut(|handles| {
            if let Some(timer) = handles.timer.take() {
                timer.clear();
            }
            handles.folder = None;
        });
        self.edit(|auto| {
            auto.state = AutoState::Off;
            auto.folder = None;
            auto.hitch = None;
        });
    }

    /// Пора ли попросить пароль копий: автокопия включена, а пароль не
    /// вводили месяц (`format::autobackup::check_due`).
    fn check_due(&self) -> bool {
        self.auto().with(|auto| {
            matches!(auto.state, AutoState::On(_))
                && self
                    .today
                    .is_some_and(|today| autobackup::check_due(auto.entered, today))
        })
    }
}

/// Имя блокировки Web Locks автокопии — одно у всех вкладок сайта.
const AUTO_LOCK: &str = "kvartik-autobackup";

/// Чем кончилась работа части копии: не загрузилась она или её работа
/// отказала — автокопия считает это сбоем и попробует снова.
async fn finished(task: Result<Task<Result<Made, Problem>>, SplitLoaderError>) -> Made {
    match task {
        Ok(task) => task.await.unwrap_or(Made::Ran(Err(Hitch::Failed))),
        Err(_) => Made::Ran(Err(Hitch::Failed)),
    }
}

/// Что сказать человеку, кроме простых строк: шаблоны с числами. Слова
/// пароля, шифра и хранилища — и у экрана «Другое устройство» (`ui/sync.rs`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Problem {
    Say(Key),
    TooShort,
    TooLarge,
    /// Копия не уложилась и частями: такую программа не откроет (задача 117).
    Oversized,
    /// С копией ИП в программе стало бы больше `MAX_IPS` (задача 38).
    TooManyIps,
    /// Копия всех ИП легла не вся — вот почему (задача 38): повтор
    /// безвреден.
    Partly(Key),
}

impl Problem {
    pub(super) fn text(self, lang: Lang) -> String {
        match self {
            Self::Say(key) => t(lang, key).to_owned(),
            Self::TooShort => fill(
                t(lang, Key::PasswordTooShort),
                &[&i18n::integer(lang, MIN_PASSWORD_CHARS as i64)],
            ),
            Self::TooLarge => fill(
                t(lang, Key::BackupTooLarge),
                &[&i18n::integer(lang, (MAX_FILE_BYTES >> 20) as i64)],
            ),
            Self::Oversized => fill(t(lang, Key::BackupOversized), &[EMAIL]),
            Self::TooManyIps => fill(t(lang, Key::OfficeTooMany), &[&i18n::integer(lang, MAX_IPS as i64)]),
            Self::Partly(why) => fill(t(lang, Key::BackupPartly), &[t(lang, why)]),
        }
    }
}

impl From<PasswordProblem> for Problem {
    fn from(problem: PasswordProblem) -> Self {
        match problem {
            PasswordProblem::TooShort => Self::TooShort,
            PasswordProblem::Mismatch => Self::Say(Key::PasswordMismatch),
        }
    }
}

impl From<CryptoError> for Problem {
    fn from(error: CryptoError) -> Self {
        Self::Say(match error {
            CryptoError::Unavailable => Key::BackupNoCrypto,
            CryptoError::WrongPassword => Key::BackupWrongFilePassword,
            CryptoError::Failed => Key::BackupFailed,
        })
    }
}

impl From<BackupError> for Problem {
    fn from(error: BackupError) -> Self {
        Self::Say(match error {
            BackupError::NotBackup => Key::BackupNotOurs,
            BackupError::Newer { .. } => Key::BackupNewer,
            BackupError::Damaged(why) => {
                web_sys::console::error_1(&format!("Kvartik, файл копии: {why}").into());
                Key::BackupDamaged
            }
        })
    }
}

impl From<FileError> for Problem {
    fn from(error: FileError) -> Self {
        match error {
            FileError::TooLarge => Self::TooLarge,
            FileError::NotText => Self::Say(Key::BackupNotOurs),
            FileError::Unreadable => Self::Say(Key::BackupFileUnreadable),
        }
    }
}

impl From<Failure> for Problem {
    fn from(failure: Failure) -> Self {
        match failure {
            Failure::Store(error) => Self::Say(error_key(error)),
            Failure::Crypto(error) => error.into(),
            Failure::File(error) => error.into(),
            Failure::TooLarge => Self::Oversized,
            Failure::TooManyIps(_) => Self::TooManyIps,
            // Больше `license::MAX_KEYS` записей ключей — «Pro» в конторе или
            // ключей одного ИП — программа не пишет (с 0.0.135), значит, и файл
            // не её (ревью задачи 38): «не копия программы» о нём верно.
            Failure::TooManyKeys => Self::Say(Key::BackupNotOurs),
            Failure::Handed => Self::Say(Key::BackupFailed),
        }
    }
}

/// Сколько чего — словами по видам (задача 124, п. 21): «счета и акты — 14,
/// клиенты — 3, записи книги — 40». Пустые виды молчат, а совсем пусто —
/// «записей нет».
pub(in crate::ui) fn tally_words(lang: Lang, tally: Tally) -> String {
    let words: Vec<String> = [
        (tally.documents, Key::TallyDocuments),
        (tally.clients, Key::TallyClients),
        (tally.ledger, Key::TallyLedger),
        (tally.files, Key::TallyFiles),
        (tally.other, Key::TallyOther),
    ]
    .into_iter()
    .filter(|(count, _)| *count > 0)
    .map(|(count, key)| fill(t(lang, key), &[&i18n::integer(lang, count as i64)]))
    .collect();
    if words.is_empty() {
        t(lang, Key::TallyNone).to_owned()
    } else {
        words.join(", ")
    }
}

/// Итог восстановления или принятого кода (задача 124, п. 21): что легло —
/// по видам, а что уже было здесь — числом; «новых — 14, обновлено — 3»
/// человек сопоставить со своими данными не мог. `part` — легла часть копии:
/// её номер и сколько их (задача 123).
pub(in crate::ui) fn restored_words(lang: Lang, restored: &Restored, part: Option<(u16, u16)>) -> String {
    let landed = restored.landed;
    let number = |count: usize| i18n::integer(lang, count as i64);
    let mut words = match (part, landed.total()) {
        (Some((at, of)), 0) => fill(t(lang, Key::BackupPartNothing), &[&number(at.into()), &number(of.into())]),
        (Some((at, of)), _) => fill(
            t(lang, Key::BackupPartRestored),
            &[&number(at.into()), &number(of.into()), &tally_words(lang, landed)],
        ),
        (None, 0) => fill(t(lang, Key::BackupRestoredNothing), &[&number(restored.unchanged)]),
        (None, _) => fill(t(lang, Key::BackupRestored), &[&tally_words(lang, landed)]),
    };
    if landed.total() > 0 && restored.unchanged > 0 {
        words.push(' ');
        words.push_str(&fill(t(lang, Key::BackupRestoredSame), &[&number(restored.unchanged)]));
    }
    words
}

/// Шаблон с одним числом на языке окна — производное от сигнала языка.
pub(super) fn counted(lang: ReadSignal<Lang>, key: Key, n: i64) -> impl Fn() -> String + Send + 'static {
    move || {
        let lang = lang.get();
        fill(t(lang, key), &[&i18n::integer(lang, n)])
    }
}

/// Готовая копия: файл — или файлы частей по порядку (задача 117) — и что
/// в ней.
struct Ready {
    downloads: Vec<Download>,
    records: usize,
    unreadable: usize,
    /// Сколько весят файлы копии вместе и файлы книги в ней (задача 64).
    size: u64,
    files: Files,
}

/// Файлы копии — ссылками для человека, по одному, как только файл готов
/// (задача 122): имя части — с её номером (`backup::part_file_name`). Файл
/// уже в браузере, и ссылка его не копирует.
pub(super) struct Downloads<'a> {
    name: &'a str,
    files: Vec<Download>,
}

impl<'a> Downloads<'a> {
    /// Ссылки на файлы копии с именем `name`.
    pub(super) const fn new(name: &'a str) -> Self {
        Self { name, files: Vec::new() }
    }

    /// Ссылки по порядку частей.
    pub(super) fn into_files(self) -> Vec<Download> {
        self.files
    }
}

impl stored::Sink for Downloads<'_> {
    fn put(&mut self, sheet: stored::Sheet) -> stored::Handing<'_> {
        let download = Download::of(&sheet.file, &backup::part_file_name(self.name, sheet.part));
        Box::pin(async move {
            self.files.push(download.ok_or(Failure::Handed)?);
            Ok(())
        })
    }

    fn restart(&mut self) {
        self.files.clear();
    }
}

/// Сколько открытого текста кладётся в один файл копии
/// (`backup::PART_PLAIN_BYTES`, задача 117). Браузерные тесты окна делят
/// копию мелко, чтобы видеть части без сотни мегабайт.
#[cfg(not(test))]
pub(super) fn part_limit() -> usize {
    backup::PART_PLAIN_BYTES
}

#[cfg(test)]
thread_local! {
    pub(super) static PART_LIMIT: std::cell::Cell<usize> = const { std::cell::Cell::new(backup::PART_PLAIN_BYTES) };
}

#[cfg(test)]
pub(super) fn part_limit() -> usize {
    PART_LIMIT.with(std::cell::Cell::get)
}

/// Где копия.
#[derive(Clone)]
enum Making {
    Idle,
    Busy,
    Ready(Rc<Ready>),
    Failed(Problem),
}

/// Где восстановление.
#[derive(Clone)]
enum Restoring {
    Idle,
    Busy,
    /// Файл открыт — нужен его пароль; с проблемой — прошлый не подошёл.
    Password(Rc<Sealed>, Option<Problem>),
    /// Копия другого ИП расшифрована: слить ли её (задача 38, этап 1).
    Other(Rc<Asked>),
    /// Записи легли; у копии частями — и какие её части легли здесь (задача 123).
    Done(Restored, After, Option<Laid>),
    /// Копия другого ИП легла новым ИП (задача 38).
    AsNew,
    /// Копия ИП из списка, а открыт другой: ничего не записано (задача 38).
    Elsewhere(Arc<Ip>),
    Failed(Problem),
}

/// Что нужно экрану копии из окна.
#[derive(Clone, Copy)]
pub(super) struct Ctx {
    lang: ReadSignal<Lang>,
    backups: Backups,
    storage: RwSignal<Storage>,
    forms: Forms,
    router: Router,
}

/// Раздел на языке окна — под заголовком экрана копии, частью wasm с первого
/// показа (`split::screen`, задача 91). Слова о хранении под ним — в главном
/// wasm (`ui/storage.rs`): они говорят, сотрёт ли браузер данные сам, и тогда,
/// когда часть не загрузилась.
#[component]
pub fn BackupSection(
    lang: ReadSignal<Lang>,
    backups: Backups,
    storage: RwSignal<Storage>,
    forms: Forms,
    router: Router,
) -> impl IntoView {
    let ctx = Ctx {
        lang,
        backups,
        storage,
        forms,
        router,
    };
    screen(lang, router, Screen::Backup, move || backup_screen(ctx))
}

split! {
    /// Экран копии — своей частью wasm (`ui/split.rs`, задача 91): первому
    /// заходу он не нужен. Сборка, разбор и восстановление копии — свои части,
    /// как и были: сборку зовёт и автокопия из части книги, когда экрана
    /// никто не открывал, а разбор и восстановление нужны только по выбору
    /// файла.
    pub(super) fn backup_screen(ctx: Ctx) -> AnyView, preload preload_screen {
        view! { <BackupBody ctx /> }.into_any()
    }
}

/// Содержимое экрана копии. Показано, только пока хранилище открыто: без базы
/// копии не из чего делать и некуда восстанавливать.
#[component]
fn BackupBody(ctx: Ctx) -> impl IntoView {
    let Ctx {
        lang,
        backups,
        storage,
        forms,
        router,
    } = ctx;
    let ready = move || matches!(storage.get(), Storage::Ready(_));
    // Пока выбран файл копии — и в пустой базе, где копию делать не из чего, —
    // только восстановление: пароль файла под ним и сразу «Придумайте пароль
    // копий», «Повторите пароль» — два набора паролей подряд, и пароль копии
    // легко ввести не туда (полный прогон 2026-11-10, п. 12). «Сделать копию»
    // прячется атрибутом — начатое в ней не пропадает, — автокопия
    // снимается: её состояние — у `Backups`.
    let picked = RwSignal::new(false);
    let quiet = Signal::derive(move || picked.get() || !backups.has_data());
    // Дизайн «Kvartik Design»: щит в круге, зачем копия, когда была
    // последняя — меткой (пора сделать новую — тёплой), пароль и большая
    // кнопка; восстановление — ссылкой ниже. В пустой базе копию делать не
    // из чего, а пришли, скорее всего, восстановить её (первый запуск,
    // «У меня есть копия»): восстановление — первым. Порядок решается, когда
    // база открылась, и после восстановления раздел не перестраивается —
    // иначе пропало бы сообщение о восстановленном.
    view! {
        <Show when=ready>
            <span class="hero-icon">{icon(Icon::Shield)}</span>
            <h2 class="headline">{tr(lang, Key::BackupHeadline)}</h2>
            <p>{tr(lang, Key::BackupAbout)}</p>
            <p class="chip backup-last" class:due=move || backups.due()>
                {move || {
                    let lang = lang.get();
                    match backups.last.get() {
                        Some(day) => fill(t(lang, Key::BackupLast), &[&i18n::date(lang, day)]),
                        None => t(lang, Key::BackupNone).to_owned(),
                    }
                }}
            </p>
            // Сколько весят файлы книги в последней копии (задача 64): больше
            // всего места в копии — у них, а убранный из записи файл тоже
            // хранится и идёт в копию; копия больше файла — частями
            // (задача 117), и сколько их, сказано тут же. Запись устройства
            // перечитывается вслед за днём последней копии.
            {move || {
                backups.last.track();
                let (size, files, parts) = Device::new().last_size().filter(|(_, files, _)| files.bytes > 0)?;
                Some(view! { <p class="hint backup-files">{move || files_words(lang.get(), size, files, parts)}</p> })
            }}
            {if backups.has_data.get_untracked() {
                view! {
                    <MakeBackup lang backups forms router quiet />
                    <RestoreBackup lang backups forms picked router />
                }
                .into_any()
            } else {
                // Пустой программе — и путь без файла (задача 124): новое
                // устройство чаще заводят рядом со старым, а не из копии.
                view! {
                    <RestoreBackup lang backups forms picked router />
                    <a class="button link sync-instead" href=Screen::Sync.href() hidden=move || picked.get()>
                        <span class="label">{tr(lang, Key::BackupSyncInstead)}</span>
                        {icon(Icon::Forward)}
                    </a>
                    <MakeBackup lang backups forms router quiet />
                }
                .into_any()
            }}
            // Автокопия — из части книги ([`AutoView`]), как карточки налога на
            // главном. Пока часть грузится или если не загрузилась, блока нет,
            // а ручная копия работает и без него.
            {move || (!quiet.get()).then(|| Suspend::new(async move {
                super::ledger::auto_part(AutoView { lang, backups, forms }).await.ok()
            }))}
        </Show>
    }
}

/// Работа части копии.
enum Job {
    /// Ручная копия: введённый пароль и повтор нового; `None` — пароль на
    /// устройстве уже задан, и введённый сверяется с ним.
    Manual { typed: String, repeat: Option<String> },
    /// Ручная копия ключом, который передал код передачи (задача 124,
    /// п. 18, [`Backups::hand_off`]).
    Prepared(Rc<crypto::Key>),
    /// Ручная копия в одно нажатие — ключом, который помнит устройство
    /// (задача 124, часть C).
    Remembered,
    /// Копия открытого ИП для клиента — своим новым паролем с повтором
    /// (задача 38).
    Client { typed: String, repeat: String },
    /// Что помнит устройство об автокопии и что браузер разрешает с её
    /// папкой.
    Status,
    /// Включить автокопию — в выбранную папку или снова в прежнюю, под
    /// паролем копий. `place` — прежнее имя для человека: папка уже своя,
    /// и новой в ней не заводится; `None` — папку только что выбрали.
    Enable {
        typed: String,
        repeat: Option<String>,
        folder: FileSystemDirectoryHandle,
        place: Option<String>,
    },
    /// Записать автокопию.
    Auto,
    /// Проверить пароль копий.
    Check(String),
    /// Выключить автокопию: забыть папку и ключ. Файлы в папке остаются.
    Off,
}

/// Папка включённой автокопии: она сама, её имя для человека и что
/// браузер с ней разрешает.
struct Place {
    folder: FileSystemDirectoryHandle,
    name: String,
    permission: Permission,
}

/// Записанная автокопия: когда она начала читать базу и сколько записей
/// не читается и в неё не вошло.
struct Wrote {
    at: Timestamp,
    unreadable: usize,
}

/// Что сделала часть копии.
enum Made {
    /// Ручная копия собрана.
    File(Ready),
    /// Автокопия включена — вот её папка; `None` — не включена.
    Status(Option<Place>),
    /// Автокопия записана — или вот что ей помешало.
    Ran(Result<Wrote, Hitch>),
    /// База устройства не отдала или не взяла автокопию.
    Refused(Hitch),
    /// Автокопии нет: её выключили, может быть, в другой вкладке.
    Off,
    /// Пароль подошёл.
    Checked,
}

split! {
    /// Сборка и шифр копии — своей частью wasm (`ui/split.rs`): ручная копия
    /// по «Сделать копию» и всё, что делает автокопия с базой устройства
    /// и папкой (задача 29), — одной частью. Своя часть у автокопии делила
    /// бы с этой сборку и шифр копии, и программа разреза положила бы их
    /// общим куском (CLAUDE.md, Правило 6).
    pub(super) fn making_task(backups: Backups, job: Job) -> Task<Result<Made, Problem>>, preload preload_make
    {
        Box::pin(async move {
            match job {
                Job::Manual { typed, repeat } => make(backups, &typed, repeat.as_deref()).await.map(Made::File),
                Job::Prepared(key) => make_prepared(backups, &key).await.map(Made::File),
                Job::Remembered => make_remembered(backups).await.map(Made::File),
                Job::Client { typed, repeat } => make_client(backups, &typed, &repeat).await.map(Made::File),
                Job::Status => Ok(status().await),
                Job::Enable { typed, repeat, folder, place } => {
                    enable(backups, &typed, repeat.as_deref(), folder, place).await
                }
                Job::Auto => Ok(write_auto(backups).await),
                Job::Check(typed) => check_password(&typed).await,
                Job::Off => Ok(match remembered::forget().await {
                    Ok(()) => Made::Off,
                    Err(error) => Made::Refused(error.into()),
                }),
            }
        })
    }
}

split! {
    /// Разбор выбранного файла копии — своей частью wasm (`ui/split.rs`);
    /// шифротекст из base64 — кусками с паузой (`platform::pause`).
    pub(super) fn opening(text: String) -> Task<Result<Sealed, Problem>>, preload preload_open {
        Box::pin(async move {
            backup::open_sliced(&text, &mut breathe).await.map_err(Problem::from)
        })
    }
}

split! {
    /// Расшифровка и слияние — своей частью wasm (`ui/split.rs`): нужны
    /// только по «Восстановить».
    pub(super) fn restoring_task(
        backups: Backups,
        job: RestoreJob,
    ) -> Task<Result<Recovered, Problem>>, preload preload_restore
    {
        Box::pin(async move { restore(backups, job).await })
    }
}

/// Что сделать части восстановления.
pub(super) enum RestoreJob {
    /// Расшифровать файл этим паролем и слить — или остановиться на
    /// вопросе о другом ИП.
    Open(Rc<Sealed>, String),
    /// То же ключом прошлой части той же копии (задача 117): пароль второй
    /// раз не спрашивается.
    Known(Rc<Sealed>, Rc<crypto::Key>),
    /// Человек ответил «слить» на вопрос о копии другого ИП.
    Merge(Rc<Asked>),
    /// Человек ответил «добавить как нового ИП» (задача 38).
    AsNew(Rc<Asked>),
}

/// Чем кончилась часть восстановления.
pub(super) enum Recovered {
    /// Записи легли. Ключ — у части копии (задача 117): им окно откроет
    /// следующую часть.
    Done(Restored, After, Option<Rc<crypto::Key>>),
    /// Копия другого ИП: окно спрашивает, а записано ничего.
    Other(Rc<Asked>),
    /// Копия легла новым ИП.
    AsNew,
    /// Копия ИП из списка, а открыт другой: ничего не записано.
    Elsewhere(Arc<Ip>),
    /// Легла не вся — вот почему; повтор безвреден. `true` — копия всех ИП
    /// легла в пустую программу: где выпускать, спросить и так, и после
    /// повтора (задача 116).
    Partial(Key, bool),
}

/// Что сказать после восстановления, кроме итога (задача 41).
#[derive(Clone, Copy, Default)]
pub(super) struct After {
    /// Копия легла в пустую программу — так заводят второе устройство,
    /// а номера документов выдаёт одно: окно спрашивает, где выпускать. У
    /// копии частями окно ставит его заново (`backup::Laid::asks`, задача
    /// 123): эта часть сделала копию целой, а пуста программа была до первой.
    fresh: bool,
    /// В копии — правки позже «сейчас» этого устройства (`backup::ahead`):
    /// часы одного из устройств идут неверно.
    ahead: bool,
    /// Легла часть копии (задача 117): окно говорит, какие ещё остались.
    part: Option<Part>,
}

/// Расшифрованная копия другого ИП — ждёт ответа человека (задача 38,
/// этап 1). Пароль второй раз не спрашивается: ключ из него считается
/// секунды (PBKDF2 на 600 000 итераций), а записи уже разобраны.
pub(super) struct Asked {
    sealed: Rc<Sealed>,
    entries: Vec<Entry>,
    key: Rc<crypto::Key>,
    ours: Option<Owner>,
    theirs: Owner,
}

/// Ключ из введённого пароля копий. `repeat` — повтор нового пароля; `None` —
/// пароль на устройстве уже задан, и введённый сверяется с ним. Вместе
/// с ключом — пароль устройства (соль и итерации — в замок файла) и новый
/// ли он: новый запоминает тот, кто его попросил, когда сделал своё.
pub(super) async fn key_for(
    backups: Backups,
    typed: &str,
    repeat: Option<&str>,
) -> Result<(crypto::Key, DeviceKey, bool), Problem> {
    let password = crypto::normalize(typed);
    let current = match repeat {
        Some(_) => None,
        None => Device::new().backup_key(),
    };
    let (salt, iterations) = match (&current, repeat) {
        (Some(key), _) => (key.salt().to_vec(), key.iterations()),
        (None, Some(repeat)) => {
            let repeat = crypto::normalize(repeat);
            if let Some(problem) = backup::new_password_problem(&password, &repeat) {
                return Err(problem.into());
            }
            (crypto::random(SALT_BYTES)?, KDF_ITERATIONS)
        }
        // Пароль пропал с устройства, пока страница была открыта
        // (`localStorage` стёрли): задать его заново — с повтором.
        (None, None) => {
            backups.key_set.set(false);
            return Err(Problem::Say(Key::BackupPasswordLost));
        }
    };
    let key = crypto::derive(&password, &salt, iterations).await?;
    if let Some(current) = &current
        && !current.accepts(key.check())
    {
        return Err(Problem::Say(Key::PasswordWrong));
    }
    let device_key = DeviceKey::new(iterations, salt, key.check().to_vec())
        .ok_or(Problem::Say(Key::BackupFailed))?;
    Ok((key, device_key, current.is_none()))
}

/// Слитая копия — того же ИП, что в программе, или другого.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SameIe {
    Yes,
    /// Другого — по ответу «Всё равно слить».
    No,
}

/// Что восстановление сделало с открытой базой.
#[derive(Clone, Copy)]
struct Landed {
    /// Что-то в ней изменилось: разделы перечитывают своё.
    touched: bool,
    /// В ней теперь есть данные.
    data: bool,
    /// В копии — данные программы: этой базы, а при нескольких ИП — всех.
    same: SameIe,
    /// Пароль копии может стать паролем копий устройства, если своего нет:
    /// только у своей копии — не у копии клиента, другого ИП или копии
    /// одного ИП при нескольких (реестр, задача 38; ревью 2026-11-06).
    adopt: bool,
}

impl Landed {
    /// Копия одного ИП — в открытую базу.
    fn single(restored: &Restored, entries: &[Entry], same: SameIe) -> Self {
        Self {
            touched: restored.added + restored.updated > 0,
            data: !entries.is_empty(),
            same,
            adopt: same == SameIe::Yes,
        }
    }
}


/// Автокопия от базы до файла (задача 29): настоящая база программы,
/// настоящий WebCrypto и папка из OPFS — выбор папки человеком тест не
/// нажмёт, а OPFS отдаёт такую же `FileSystemDirectoryHandle`.
#[cfg(all(test, target_arch = "wasm32"))]
mod tests;
