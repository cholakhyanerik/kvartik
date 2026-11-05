//! Раздел «Резервная копия» (задача 7): сделать копию под паролем,
//! восстановить из неё, напомнить, если копии давно не было.
//!
//! Раздел — содержимое своего экрана внутри профиля (`ui/route.rs`), а
//! напоминание о копии — вверху остальных экранов.
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

use std::rc::Rc;
use std::time::Duration;

use kvartik_core::format::autobackup::{self, DeviceTag};
use kvartik_core::format::backup::{
    self, BackupError, DeviceKey, Header, KDF_ITERATIONS, KINDS, MAX_FILE_BYTES,
    MIN_PASSWORD_CHARS, PasswordProblem, SALT_BYTES, Sealed,
};
use kvartik_core::i18n::{self, Key, Lang, fill, t};
use kvartik_core::model::date::Date;
use kvartik_core::model::record::Timestamp;
use leptos::ev::{Event, MouseEvent, SubmitEvent};
use leptos::prelude::*;
use leptos::wasm_split::SplitLoaderError;
use web_sys::{FileSystemDirectoryHandle, HtmlInputElement};

use super::form::Forms;
use super::icon::{Icon, icon};
use super::route::{Router, Screen};
use super::split::{Task, split};
use super::storage::{Storage, error_key};
use super::{Base, tr};
use crate::platform::autobackup::{self as folders, FolderError, Permission, PickError};
use crate::platform::clock;
use crate::platform::crypto::{self, CryptoError};
use crate::platform::file::{self, Download, FileError};
use crate::platform::pause::breathe;
use crate::platform::share::{self, ShareError};
use crate::store::StoreError;
use crate::store::autobackup::{self as remembered, Saved};
use crate::store::backup::{self as stored, Failure, Restored};
use crate::store::device::Device;

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
    /// Автокопия в папку (задача 29) — одной ручкой: `Backups` лежит
    /// в каждом разделе, и его размер держит `clippy` (`large_enum_variant`
    /// у частей профиля).
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
    /// Копия вышла больше `MAX_FILE_BYTES`.
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
            Failure::Crypto(_) | Failure::File(_) => Self::Failed,
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

/// Всё об автокопии — за одной ручкой [`Backups`].
struct AutoParts {
    /// Где автокопия.
    state: RwSignal<Auto>,
    /// Её объекты браузера.
    handles: Handles,
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
            }),
        }
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
    fn password_entered(&self) {
        let now = clock::now();
        Device::new().set_password_entered(now);
        let day = clock::local_date(now);
        self.auto().update(|auto| auto.entered = day);
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

/// Напоминание о копии — карточкой на главном экране, под приветствием, со
/// словами, зачем копия; на остальных экранах телефона о том же говорит
/// короткая метка ([`BackupChip`] с `nudge`), а на компьютере метка всегда
/// видна в меню: напоминать обязано каждое открытие, на каком бы экране оно
/// ни было (Правило 2). Ссылка ведёт на экран копии.
#[component]
pub fn BackupReminder(lang: ReadSignal<Lang>, backups: Backups) -> impl IntoView {
    move || {
        backups.due().then(|| {
            let key = if backups.last.get().is_some() {
                Key::BackupDueOld
            } else {
                Key::BackupDueNone
            };
            view! {
                <div class="notice due" role="status">
                    {icon(Icon::Shield)}
                    <p class="reminder">{tr(lang, key)}</p>
                    <a class="button" href=Screen::Backup.href()>{tr(lang, Key::BackupGoTo)}</a>
                </div>
            }
        })
    }
}

/// Когда была последняя копия — словами: «сегодня», «3 дня назад», «копии
/// ещё нет». Часы без даты — `None`.
fn ago(lang: Lang, last: Option<Date>, today: Option<Date>) -> Option<String> {
    let Some(last) = last else {
        return Some(t(lang, Key::BackupChipNone).to_owned());
    };
    let days = today?.days_since(last).max(0);
    Some(if days == 0 {
        t(lang, Key::BackupChipToday).to_owned()
    } else {
        let n = days.unsigned_abs().into();
        let count = fill(
            i18n::plural(lang, n, Key::DaysOne, Key::DaysFew, Key::DaysMany),
            &[&i18n::integer(lang, i64::from(days))],
        );
        fill(t(lang, Key::BackupChipAgo), &[&count])
    })
}

/// Метка копии (дизайн «Kvartik Design»): когда была последняя — словами,
/// а пора ли сделать новую, говорит и заливка, и слова. В меню на
/// компьютере она видна всегда; `nudge` — она же под заголовком экрана на
/// телефоне, и только когда пора. Ссылка ведёт на экран копии.
#[component]
pub fn BackupChip(
    lang: ReadSignal<Lang>,
    backups: Backups,
    #[prop(optional)] nudge: bool,
) -> impl IntoView {
    move || {
        if nudge && !backups.due() {
            return None;
        }
        let text = ago(lang.get(), backups.last.get(), backups.today)?;
        Some(view! {
            <a
                class="chip"
                class:due=move || backups.due()
                class:nudge=nudge
                href=Screen::Backup.href()
            >
                {icon(Icon::Shield)}
                <span>{text}</span>
            </a>
        })
    }
}

/// Строка «Резервная копия» в группе «Программа» на экране профиля —
/// ссылка на экран копии с тем, когда была последняя; пора сделать новую —
/// и словами метки, и точкой цвета напоминания.
#[component]
pub fn BackupEntry(lang: ReadSignal<Lang>, backups: Backups) -> impl IntoView {
    view! {
        <a class="entry" href=Screen::Backup.href()>
            <span class="entry-main">
                <span class="entry-title">{tr(lang, Key::BackupTitle)}</span>
                <span class="entry-meta" class:due=move || backups.due()>
                    {move || ago(lang.get(), backups.last.get(), backups.today)}
                </span>
            </span>
            {icon(Icon::Forward)}
        </a>
    }
}

/// Что сказать человеку, кроме простых строк: шаблоны с числами.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Problem {
    Say(Key),
    TooShort,
    TooLarge,
    /// Сделанная копия вышла больше предела: такую программа не откроет.
    Oversized,
}

impl Problem {
    fn text(self, lang: Lang) -> String {
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
            Self::Oversized => fill(
                t(lang, Key::BackupOversized),
                &[&i18n::integer(lang, (MAX_FILE_BYTES >> 20) as i64)],
            ),
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
        }
    }
}

/// Шаблон с одним числом на языке окна — производное от сигнала языка.
fn counted(lang: ReadSignal<Lang>, key: Key, n: i64) -> impl Fn() -> String + Send + 'static {
    move || {
        let lang = lang.get();
        fill(t(lang, key), &[&i18n::integer(lang, n)])
    }
}

/// Готовая копия: файл и что в нём.
struct Ready {
    download: Download,
    records: usize,
    unreadable: usize,
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
    Done(Restored),
    Failed(Problem),
}

/// Раздел на языке окна — под заголовком экрана копии. Показан, только пока
/// хранилище открыто: без базы копии не из чего делать и некуда
/// восстанавливать.
#[component]
pub fn BackupSection(
    lang: ReadSignal<Lang>,
    backups: Backups,
    storage: RwSignal<Storage>,
    forms: Forms,
    router: Router,
) -> impl IntoView {
    let ready = move || matches!(storage.get(), Storage::Ready(_));
    // Автокопия — из части книги ([`AutoView`]), как карточки налога на
    // главном: когда экран копии впервые показан, — первому заходу она не
    // нужна. Пока часть грузится или если не загрузилась, блока нет, а ручная
    // копия работает и без него.
    let seen = Memo::new(move |seen: Option<&bool>| {
        seen.copied().unwrap_or(false) || router.shows(Screen::Backup)
    });
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
            {if backups.has_data.get_untracked() {
                view! {
                    <MakeBackup lang backups forms router />
                    <RestoreBackup lang backups forms />
                }
                .into_any()
            } else {
                view! {
                    <RestoreBackup lang backups forms />
                    <MakeBackup lang backups forms router />
                }
                .into_any()
            }}
            {move || seen.get().then(|| Suspend::new(async move {
                super::ledger::auto_part(AutoView { lang, backups, forms }).await.ok()
            }))}
        </Show>
    }
}

/// Автокопия на экране копии (задача 29) — из части книги
/// (`ledger::book_part`), как лицензия: своя часть завела бы общих кусков
/// на тринадцать файлов wasm (замер задачи 29; CLAUDE.md, Правило 6),
/// а главному wasm она не нужна (Правило 1).
#[derive(Clone, Copy)]
pub(super) struct AutoView {
    lang: ReadSignal<Lang>,
    backups: Backups,
    forms: Forms,
}

/// Блок автокопии — его рисует часть книги.
pub(super) fn auto_view(view: AutoView) -> AnyView {
    let AutoView { lang, backups, forms } = view;
    view! { <AutoSection lang backups forms /> }.into_any()
}

/// Карточки автокопии на главном экране — из части книги, вместе
/// с карточками налога (`tax::content`): ждущая разрешения автокопия,
/// которой есть что писать, — с кнопкой «Разрешить запись»: это бывает
/// после каждого перезапуска браузера, и одно нажатие её чинит; и раз
/// в месяц — что пора ввести пароль копий (решение человека 2026-11-03).
/// Часть грузится, когда в базе есть данные, — тогда же автокопия и
/// начинает работать: без данных ей писать нечего.
#[component]
pub(super) fn AutoCards(lang: ReadSignal<Lang>, backups: Backups) -> impl IntoView {
    backups.start_auto();
    let ask = Memo::new(move |_| backups.waits_for_permission());
    let check = Memo::new(move |_| backups.check_due());
    move || {
        if ask.get() {
            Some(view! {
                <div class="notice due" role="status">
                    {icon(Icon::Shield)}
                    <p class="reminder">{tr(lang, Key::AutoAsk)}</p>
                    <button type="button" on:click=move |_| backups.allow()>
                        {tr(lang, Key::AutoAllow)}
                    </button>
                </div>
            }
            .into_any())
        } else if check.get() {
            Some(view! {
                <div class="notice due" role="status">
                    {icon(Icon::Shield)}
                    <p class="reminder">{tr(lang, Key::AutoCheckDue)}</p>
                    <a class="button" href=Screen::Backup.href()>{tr(lang, Key::BackupGoTo)}</a>
                </div>
            }
            .into_any())
        } else {
            None
        }
    }
}

/// Автокопия на экране копии: включить в выбранную папку, что с ней сейчас,
/// разрешить запись, сменить папку, выключить, проверить пароль. Введённое
/// здесь и выбранную, но ещё не включённую папку новая версия ждёт
/// ([`Forms`]).
#[component]
fn AutoSection(lang: ReadSignal<Lang>, backups: Backups, forms: Forms) -> impl IntoView {
    backups.start_auto();
    let auto = backups.auto();
    // Выбранная, но ещё не включённая папка.
    // Выбранная, но ещё не включённая папка — и её прежнее имя, если это
    // папка включённой автокопии, которую включают снова после смены
    // пароля.
    let chosen = StoredValue::new_local(None::<(FileSystemDirectoryHandle, Option<String>)>);
    let chosen_name = RwSignal::new(None::<String>);
    let password = RwSignal::new(String::new());
    let repeat = RwSignal::new(String::new());
    let typed_check = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    // Чем кончилось последнее действие здесь: слова успеха или беда.
    let said = RwSignal::new(None::<Result<Key, Problem>>);
    forms.hold_while(move || {
        chosen_name.with(Option::is_some)
            || password.with(|text| !text.is_empty())
            || repeat.with(|text| !text.is_empty())
            || typed_check.with(|text| !text.is_empty())
            || busy.get()
    });
    let new_password = move || !backups.key_set.get();
    // Состояние — отдельно от остального: пока пишется автокопия, блок не
    // перерисовывается целиком, и начатое в поле пароля не теряет фокуса.
    let state = Memo::new(move |_| auto.with(|auto| auto.state));
    let check_due = Memo::new(move |_| backups.check_due());

    // Выбор папки — в самом нажатии, до первого `await`
    // (`platform::autobackup::pick`).
    // И новую папку, и другую вместо прежней включают паролем копий: иначе
    // минута у открытой программы позволила бы молча увести все следующие
    // копии в чужую папку (ревью задачи 29).
    let pick = move |_: MouseEvent| {
        said.set(None);
        let picking = folders::pick();
        leptos::task::spawn_local(async move {
            match picking.await {
                Ok(folder) => {
                    chosen_name.set(Some(autobackup::place(&folder.name())));
                    chosen.set_value(Some((folder, None)));
                }
                Err(PickError::Cancelled) => {}
                Err(PickError::Failed) => said.set(Some(Err(Problem::Say(Key::AutoPickFailed)))),
            }
        });
    };
    let cancel = move |_: MouseEvent| {
        chosen.set_value(None);
        chosen_name.set(None);
        password.set(String::new());
        repeat.set(String::new());
    };
    let enable = move |event: SubmitEvent| {
        event.prevent_default();
        let Some((folder, place)) = chosen.get_value() else {
            return;
        };
        if busy.get_untracked() {
            return;
        }
        said.set(None);
        let job = Job::Enable {
            typed: password.get_untracked(),
            repeat: new_password().then(|| repeat.get_untracked()),
            folder,
            place,
        };
        leptos::task::spawn_local(async move {
            if act(backups, busy, said, job).await {
                password.set(String::new());
                repeat.set(String::new());
                chosen.set_value(None);
                chosen_name.set(None);
                said.set(Some(Ok(Key::AutoTurnedOn)));
            }
        });
    };
    // Вопрос о разрешении — тоже в самом нажатии.
    let allow = move |_: MouseEvent| {
        said.set(None);
        backups.allow();
    };
    let off = move |_: MouseEvent| {
        said.set(None);
        leptos::task::spawn_local(async move {
            if act(backups, busy, said, Job::Off).await {
                said.set(Some(Ok(Key::AutoTurnedOff)));
            }
        });
    };
    let check = move |event: SubmitEvent| {
        event.prevent_default();
        if busy.get_untracked() {
            return;
        }
        said.set(None);
        let typed = typed_check.get_untracked();
        leptos::task::spawn_local(async move {
            if act(backups, busy, said, Job::Check(typed)).await {
                typed_check.set(String::new());
                said.set(Some(Ok(Key::AutoCheckOk)));
            }
        });
    };
    let folder_words = move |name: String| move || fill(t(lang.get(), Key::AutoFolder), &[&name]);
    let hitch = move || {
        auto.with(|auto| auto.hitch).map(|hitch| {
            let problem = hitch.problem();
            view! { <p role="alert">{move || problem.text(lang.get())}</p> }
        })
    };
    // Пароль сменили — автокопию включают снова в ту же папку новым
    // паролем.
    let renewing = Memo::new(move |_| auto.with(|auto| auto.hitch == Some(Hitch::PasswordChanged)));
    let renew = move |_: MouseEvent| {
        said.set(None);
        let Some(folder) = backups.handles(|handles| handles.folder.clone()) else {
            return;
        };
        let name = auto.with_untracked(|auto| auto.folder.clone()).unwrap_or_else(|| folder.name());
        chosen_name.set(Some(name.clone()));
        chosen.set_value(Some((folder, Some(name))));
    };
    // Пароль для новой папки, для другой вместо прежней и для прежней после
    // смены пароля — одна форма.
    let form = move || {
        chosen_name.get().map(|name| view! {
            <p>{folder_words(name)}</p>
            <form class="backup" on:submit=enable>
                {move || if new_password() {
                    view! {
                        <label>
                            {tr(lang, Key::BackupNewPassword)}
                            <input type="password" autocomplete="new-password" bind:value=password />
                        </label>
                        <label>
                            {tr(lang, Key::BackupRepeatPassword)}
                            <input type="password" autocomplete="new-password" bind:value=repeat />
                        </label>
                    }
                    .into_any()
                } else {
                    view! {
                        <label>
                            {tr(lang, Key::BackupPassword)}
                            <input type="password" autocomplete="current-password" bind:value=password />
                        </label>
                    }
                    .into_any()
                }}
                <p class="hint">{tr(lang, Key::AutoPasswordNote)}</p>
                <div class="actions">
                    <button type="submit" disabled=move || busy.get()>{tr(lang, Key::AutoEnable)}</button>
                    <button type="button" class="secondary" disabled=move || busy.get() on:click=cancel>
                        {tr(lang, Key::Cancel)}
                    </button>
                </div>
            </form>
            {move || busy.get().then(|| view! {
                <p role="status">{tr(lang, Key::AutoEnabling)}</p>
            })}
        })
    };
    let picking = move || chosen_name.with(Option::is_some);

    view! {
        <h2>{tr(lang, Key::AutoTitle)}</h2>
        {move || match state.get() {
            AutoState::Unavailable => Some(view! {
                <p class="hint">{tr(lang, Key::AutoNotHere)}</p>
            }.into_any()),
            AutoState::Loading => None,
            AutoState::Off => Some(view! {
                <p>{tr(lang, Key::AutoAbout)}</p>
                {hitch}
                // Одна кнопка — без ряда `.actions`: ряд снимает у кнопки
                // отступ сверху, и она легла бы вплотную к словам (как у слов
                // о несгруженной части, `ui/split.rs`).
                {move || (!picking()).then(|| view! {
                    <p class="hint">{tr(lang, Key::AutoFolderHint)}</p>
                    <button type="button" class="secondary" disabled=move || busy.get() on:click=pick>
                        {tr(lang, Key::AutoChoose)}
                    </button>
                })}
                {form}
            }.into_any()),
            AutoState::On(permission) => Some(view! {
                {move || auto.with(|auto| auto.folder.clone()).map(|name| view! { <p>{folder_words(name)}</p> })}
                <p class="hint">{move || {
                    let lang = lang.get();
                    match auto.with(|auto| auto.last).and_then(clock::local_date) {
                        Some(day) => fill(t(lang, Key::AutoLast), &[&i18n::date(lang, day)]),
                        None => t(lang, Key::AutoNone).to_owned(),
                    }
                }}</p>
                {move || auto.with(|auto| auto.running).then(|| view! {
                    <p role="status">{tr(lang, Key::AutoWriting)}</p>
                })}
                {match permission {
                    Permission::Granted => None,
                    Permission::Prompt => Some(view! {
                        <p role="alert">{tr(lang, Key::AutoAsk)}</p>
                        <button type="button" on:click=allow>{tr(lang, Key::AutoAllow)}</button>
                    }.into_any()),
                    Permission::Denied => Some(view! {
                        <p role="alert">{tr(lang, Key::AutoDenied)}</p>
                    }.into_any()),
                }}
                {hitch}
                {move || (renewing.get() && !picking()).then(|| view! {
                    <button type="button" on:click=renew>{tr(lang, Key::AutoRenew)}</button>
                })}
                {move || {
                    let unreadable = auto.with(|auto| auto.unreadable) as i64;
                    (unreadable > 0).then(|| view! {
                        <p role="alert">{counted(lang, Key::BackupUnreadable, unreadable)}</p>
                    })
                }}
                // Пароль сменился — новый просит кнопка выше, и вторая просьба
                // ввести пароль рядом с ней только путала бы.
                {move || (check_due.get() && !picking() && !renewing.get()).then(|| view! {
                    <form class="backup" on:submit=check>
                        <p>{tr(lang, Key::AutoCheckDue)}</p>
                        <label>
                            {tr(lang, Key::BackupPassword)}
                            <input type="password" autocomplete="current-password" bind:value=typed_check />
                        </label>
                        <button type="submit" class="wide" disabled=move || busy.get()>
                            {tr(lang, Key::AutoCheck)}
                        </button>
                    </form>
                })}
                {form}
                {move || (!picking()).then(|| view! {
                    <div class="actions auto-actions">
                        <button type="button" class="secondary" disabled=move || busy.get() on:click=pick>
                            {tr(lang, Key::AutoChange)}
                        </button>
                        <button type="button" class="secondary" disabled=move || busy.get() on:click=off>
                            {tr(lang, Key::AutoOff)}
                        </button>
                    </div>
                })}
            }.into_any()),
        }}
        {move || said.get().map(|said| match said {
            Ok(key) => view! { <p role="status">{tr(lang, key)}</p> }.into_any(),
            Err(problem) => view! { <p role="alert">{move || problem.text(lang.get())}</p> }.into_any(),
        })}
    }
}

/// Работа части копии по нажатию на экране автокопии и что с ней сделать
/// окну. `true` — получилось.
async fn act(
    backups: Backups,
    busy: RwSignal<bool>,
    said: RwSignal<Option<Result<Key, Problem>>>,
    job: Job,
) -> bool {
    busy.set(true);
    let outcome = match making_task(backups, job).await {
        Ok(task) => task.await,
        Err(_) => Err(Problem::Say(Key::PartFailed)),
    };
    busy.set(false);
    match outcome {
        Ok(Made::Status(Some(place))) => {
            backups.found(place);
            backups.edit(|auto| auto.hitch = None);
            // Первая автокопия в новую папку — сразу: человек видит, что она
            // легла, а не ждёт изменения.
            backups.run_auto();
            true
        }
        Ok(Made::Checked) => {
            backups.password_entered();
            true
        }
        Ok(Made::Off | Made::Status(None)) => {
            backups.forgotten();
            true
        }
        Ok(Made::Refused(hitch)) => {
            said.set(Some(Err(hitch.problem())));
            false
        }
        Ok(Made::File(_) | Made::Ran(_)) => true,
        Err(problem) => {
            said.set(Some(Err(problem)));
            false
        }
    }
}

/// Сделать копию: пароль, сборка и шифр, потом ссылка на файл. Введённый
/// пароль, копию в работе и готовый файл, пока открыт экран копии, новая
/// версия ждёт ([`Forms`]): ссылка на файл живёт, пока жива страница, а нажатие
/// на неё копию ещё не заканчивает — Safari на iPhone сначала спрашивает
/// о загрузке, и перезагрузка в этот миг оставила бы дату копии без файла.
#[component]
fn MakeBackup(lang: ReadSignal<Lang>, backups: Backups, forms: Forms, router: Router) -> impl IntoView {
    let password = RwSignal::new(String::new());
    let repeat = RwSignal::new(String::new());
    // «Забыли пароль?» — новый пароль поверх заданного.
    let resetting = RwSignal::new(false);
    let making = RwSignal::new_local(Making::Idle);
    // Как файл отдан: что сказать о нём.
    let saved = RwSignal::new(None::<Key>);
    let share_failed = RwSignal::new(false);
    let new_password = move || !backups.key_set.get() || resetting.get();
    let busy = move || matches!(making.get(), Making::Busy);
    forms.hold_while(move || {
        let typed = password.with(|text| !text.is_empty()) || repeat.with(|text| !text.is_empty());
        typed
            || making.with(|state| match state {
                Making::Busy => true,
                Making::Ready(_) => router.shows(Screen::Backup),
                Making::Idle | Making::Failed(_) => false,
            })
    });

    let submit = move |event: SubmitEvent| {
        event.prevent_default();
        if busy() {
            return;
        }
        let fresh = new_password();
        let (typed, again) = (password.get_untracked(), repeat.get_untracked());
        making.set(Making::Busy);
        saved.set(None);
        share_failed.set(false);
        leptos::task::spawn_local(async move {
            let job = Job::Manual {
                typed,
                repeat: fresh.then_some(again),
            };
            let outcome = match making_task(backups, job).await {
                Ok(task) => task.await.and_then(|made| match made {
                    Made::File(ready) => Ok(ready),
                    _ => Err(Problem::Say(Key::BackupFailed)),
                }),
                Err(_) => Err(Problem::Say(Key::PartFailed)),
            };
            if outcome.is_ok() {
                password.set(String::new());
                repeat.set(String::new());
                resetting.set(false);
                backups.password_entered();
            }
            making.set(match outcome {
                Ok(ready) => Making::Ready(Rc::new(ready)),
                Err(problem) => Making::Failed(problem),
            });
        });
    };

    // Файл отдан браузеру или меню «Поделиться»: это и есть копия. Сохранили
    // ли они файл, они не говорят.
    let handed = move |said: Key| {
        let now = clock::now();
        Device::new().set_last_backup(now);
        backups.last.set(clock::local_date(now));
        saved.set(Some(said));
    };
    let on_save = move |_: MouseEvent| handed(Key::BackupSaved);

    view! {
        <form class="backup" on:submit=submit>
            {move || if new_password() {
                view! {
                    <label>
                        {tr(lang, Key::BackupNewPassword)}
                        <input type="password" autocomplete="new-password" bind:value=password />
                    </label>
                    <label>
                        {tr(lang, Key::BackupRepeatPassword)}
                        <input type="password" autocomplete="new-password" bind:value=repeat />
                    </label>
                    <p class="hint">
                        {counted(lang, Key::BackupPasswordHint, MIN_PASSWORD_CHARS as i64)}
                    </p>
                    {move || resetting.get().then(|| view! {
                        <p class="hint">{tr(lang, Key::BackupForgotNote)}</p>
                    })}
                }
                .into_any()
            } else {
                view! {
                    <label>
                        {tr(lang, Key::BackupPassword)}
                        <input type="password" autocomplete="current-password" bind:value=password />
                    </label>
                }
                .into_any()
            }}
            <button type="submit" class="wide" disabled=busy>
                {icon(Icon::Download)}
                <span>{tr(lang, Key::BackupMake)}</span>
            </button>
            {move || (!new_password()).then(|| view! {
                <button
                    type="button"
                    class="link"
                    disabled=busy
                    on:click=move |_| resetting.set(true)
                >
                    {tr(lang, Key::BackupForgot)}
                </button>
            })}
        </form>
        {move || match making.get() {
            Making::Idle => None,
            Making::Busy => Some(view! {
                <p role="status">{tr(lang, Key::BackupMaking)}</p>
            }.into_any()),
            Making::Failed(problem) => Some(view! {
                <p role="alert">{move || problem.text(lang.get())}</p>
            }.into_any()),
            Making::Ready(ready) => {
                let (records, unreadable) = (ready.records as i64, ready.unreadable as i64);
                let name = ready.download.name();
                // На iPhone и iPad — ещё и меню «Поделиться» (задача 13):
                // `platform::share::can_share_any`. Меню открывается в самом
                // нажатии, до первого `await` (Правило 6).
                let file = ready.download.file().clone();
                let shares = share::can_share_any(&file);
                let send = move |_: MouseEvent| {
                    share_failed.set(false);
                    let sent = share::share(&file);
                    leptos::task::spawn_local(async move {
                        match sent.await {
                            Ok(()) => handed(Key::BackupShared),
                            Err(error) => share_failed.set(error == ShareError::Failed),
                        }
                    });
                };
                Some(view! {
                    <p role="status">{counted(lang, Key::BackupReady, records)}</p>
                    {(unreadable > 0).then(|| view! {
                        <p role="alert">{counted(lang, Key::BackupUnreadable, unreadable)}</p>
                    })}
                    <div class="actions backup-file">
                        <a
                            class="button"
                            href=ready.download.url().to_owned()
                            download=ready.download.name()
                            on:click=on_save
                        >
                            {move || fill(t(lang.get(), Key::BackupSaveFile), &[&name])}
                        </a>
                        {shares.then(|| view! {
                            <button type="button" class="secondary" on:click=send>
                                {icon(Icon::Share)}
                                <span>{tr(lang, Key::BackupShare)}</span>
                            </button>
                        })}
                    </div>
                    {move || saved.get().map(|said| view! {
                        <p role="status">{tr(lang, said)}</p>
                    })}
                    {move || share_failed.get().then(|| view! {
                        <p role="alert">{tr(lang, Key::ShareFailed)}</p>
                    })}
                }.into_any())
            }
        }}
    }
}

/// Работа части копии.
enum Job {
    /// Ручная копия: введённый пароль и повтор нового; `None` — пароль на
    /// устройстве уже задан, и введённый сверяется с ним.
    Manual { typed: String, repeat: Option<String> },
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
        sealed: Rc<Sealed>,
        typed: String,
    ) -> Task<Result<Restored, Problem>>, preload preload_restore
    {
        Box::pin(async move { restore(backups, &sealed, &typed).await })
    }
}

/// Ключ из введённого пароля копий. `repeat` — повтор нового пароля; `None` —
/// пароль на устройстве уже задан, и введённый сверяется с ним. Вместе
/// с ключом — пароль устройства (соль и итерации — в замок файла) и новый
/// ли он: новый запоминает тот, кто его попросил, когда сделал своё.
async fn key_for(
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

/// Собрать и зашифровать копию. `repeat` — повтор нового пароля; `None` —
/// пароль на устройстве уже задан, и введённый сверяется с ним.
async fn make(backups: Backups, typed: &str, repeat: Option<&str>) -> Result<Ready, Problem> {
    let (key, device_key, fresh) = key_for(backups, typed, repeat).await?;
    let store = backups.base.get().ok_or(Problem::Say(Key::BackupFailed))?;
    let now = clock::now();
    let today = clock::local_date(now).ok_or(Problem::Say(Key::BackupFailed))?;
    let header = Header::new(env!("CARGO_PKG_VERSION"), now);
    let made = stored::make(
        &store,
        KINDS,
        &header,
        &key,
        device_key.salt(),
        device_key.iterations(),
    )
    .await?;
    let download = Download::text(&made.file, &backup::file_name(today))
        .ok_or(Problem::Say(Key::BackupFailed))?;
    // Новый пароль запоминается, когда копия под ним уже собрана.
    // Включённая автокопия на него сама не переходит: её следующая попытка
    // скажет, что пароль сменился, и попросит его ввести (`write_file`), —
    // иначе «Забыли пароль?» у открытой программы молча отдавало бы все
    // следующие автокопии под чужим паролем (ревью задачи 29).
    if fresh {
        Device::new().set_backup_key(&device_key);
        backups.key_set.set(true);
    }
    Ok(Ready {
        download,
        records: made.records,
        unreadable: made.unreadable,
    })
}

/// Что помнит устройство об автокопии и что браузер разрешает с её папкой.
async fn status() -> Made {
    match remembered::load().await {
        Ok(Some(saved)) => {
            let permission = folders::permission(&saved.folder).await;
            Made::Status(Some(Place {
                folder: saved.folder,
                name: saved.place,
                permission,
            }))
        }
        Ok(None) => Made::Status(None),
        Err(error) => Made::Refused(error.into()),
    }
}

/// Включить автокопию: своя папка «Kvartik» внутри выбранной (`place` —
/// папка уже своя, включают снова), ключ из пароля копий со своей солью —
/// в базу устройства. Первую автокопию пишет окно сразу после этого,
/// обычным путём ([`Backups::run_auto`]).
async fn enable(
    backups: Backups,
    typed: &str,
    repeat: Option<&str>,
    chosen: FileSystemDirectoryHandle,
    place: Option<String>,
) -> Result<Made, Problem> {
    let (_, device_key, fresh) = key_for(backups, typed, repeat).await?;
    let (folder, place) = match place {
        Some(place) => (chosen, place),
        None => {
            let place = autobackup::place(&chosen.name());
            if autobackup::own_folder(&chosen.name()) {
                (chosen, place)
            } else {
                match folders::subfolder(&chosen, autobackup::FOLDER).await {
                    Ok(folder) => (folder, place),
                    Err(error) => return Ok(Made::Refused(error.into())),
                }
            }
        }
    };
    // Своя соль: ключ, который останется на устройстве, откроет только
    // автокопии, а не ручные копии того же пароля (шапка
    // `store/autobackup.rs`).
    let salt = crypto::random(SALT_BYTES)?;
    let key = crypto::derive(&crypto::normalize(typed), &salt, KDF_ITERATIONS).await?;
    let lock = DeviceKey::new(KDF_ITERATIONS, salt, key.check().to_vec())
        .ok_or(Problem::Say(Key::BackupFailed))?;
    let saved = Saved {
        folder,
        place,
        key: key.aes().clone(),
        lock,
        device: device_key.clone(),
    };
    if let Err(error) = remembered::save(&saved).await {
        return Ok(Made::Refused(error.into()));
    }
    if fresh {
        Device::new().set_backup_key(&device_key);
        backups.key_set.set(true);
    }
    backups.password_entered();
    let permission = folders::permission(&saved.folder).await;
    Ok(Made::Status(Some(Place {
        folder: saved.folder,
        name: saved.place,
        permission,
    })))
}

/// Записать автокопию — или сказать, что помешало.
async fn write_auto(backups: Backups) -> Made {
    match write_file(backups).await {
        Ok(Some(wrote)) => Made::Ran(Ok(wrote)),
        Ok(None) => Made::Off,
        Err(hitch) => Made::Ran(Err(hitch)),
    }
}

/// Собрать и зашифровать копию ключом из базы устройства, положить файл дня
/// в папку и убрать старые свои. `None` — автокопии нет: её выключили.
async fn write_file(backups: Backups) -> Result<Option<Wrote>, Hitch> {
    let Some(saved) = remembered::load().await? else {
        return Ok(None);
    };
    let device = Device::new();
    // Пароль копий сменили, а автокопию включали под прежним: под ним она
    // больше не пишет, пока новый не введут.
    if device.backup_key().is_some_and(|current| current != saved.device) {
        return Err(Hitch::PasswordChanged);
    }
    match folders::permission(&saved.folder).await {
        Permission::Granted => {}
        Permission::Prompt => return Err(Hitch::Ask),
        Permission::Denied => return Err(Hitch::Denied),
    }
    let store = backups.base.get().ok_or(Hitch::Failed)?;
    // Момент — до чтения базы: что запишут, пока копия собирается, в неё
    // не войдёт и останется на следующую (`format::autobackup::pending`).
    let at = clock::now();
    let today = clock::local_date(at).ok_or(Hitch::Failed)?;
    let tag = device.auto_tag().unwrap_or_else(|| {
        let tag = DeviceTag::random();
        device.set_auto_tag(tag);
        tag
    });
    let key = crypto::Key::stored(saved.key, saved.lock.check().to_vec());
    let header = Header::new(env!("CARGO_PKG_VERSION"), at);
    let made = stored::make(
        &store,
        KINDS,
        &header,
        &key,
        saved.lock.salt(),
        saved.lock.iterations(),
    )
    .await?;
    folders::write(&saved.folder, &autobackup::file_name(today, tag), made.file.as_bytes()).await?;
    // Старые свои файлы и брошенные временные — уже после нового: не
    // убрались — копия всё равно записана, а лишнее уберёт следующая.
    if let Ok(names) = folders::names(&saved.folder, autobackup::PREFIX).await {
        let leftovers = names
            .iter()
            .map(String::as_str)
            .filter(|name| autobackup::leftover(name, tag, today));
        for name in autobackup::stale(names.iter().map(String::as_str), tag, today)
            .into_iter()
            .chain(leftovers)
        {
            let _ = folders::remove(&saved.folder, name).await;
        }
    }
    // Момент — здесь, под той же блокировкой, что и сама запись
    // (`platform::autobackup::alone`): у двух вкладок файл и момент
    // последней автокопии не разойдутся. Автокопия — это копия (решение
    // человека 2026-11-03).
    device.set_last_auto(at);
    device.set_last_backup(at);
    Ok(Some(Wrote {
        at,
        unreadable: made.unreadable,
    }))
}

/// Проверить пароль копий: тот ли, что на устройстве.
async fn check_password(typed: &str) -> Result<Made, Problem> {
    let current = Device::new()
        .backup_key()
        .ok_or(Problem::Say(Key::BackupPasswordLost))?;
    let key = crypto::derive(&crypto::normalize(typed), current.salt(), current.iterations()).await?;
    if current.accepts(key.check()) {
        Ok(Made::Checked)
    } else {
        Err(Problem::Say(Key::PasswordWrong))
    }
}

/// Восстановить из копии: выбрать файл, ввести его пароль, слить.
/// Пароль файла и само восстановление — открытая форма ([`Forms`]): новая
/// версия их ждёт, а её перезагрузка оборвала бы восстановление — записано
/// не было бы ничего, а человек мог бы решить, что было (задача 67).
#[component]
fn RestoreBackup(lang: ReadSignal<Lang>, backups: Backups, forms: Forms) -> impl IntoView {
    let restoring = RwSignal::new_local(Restoring::Idle);
    let password = RwSignal::new(String::new());

    // Файл читается сразу при выборе (`platform::file::read`).
    let on_file = move |event: Event| {
        let input: HtmlInputElement = event_target(&event);
        let Some(chosen) = file::chosen(&input) else {
            return;
        };
        restoring.set(Restoring::Busy);
        leptos::task::spawn_local(async move {
            let opened = match file::read(&chosen, MAX_FILE_BYTES).await {
                Ok(text) => match opening(text).await {
                    Ok(task) => task.await,
                    Err(_) => Err(Problem::Say(Key::PartFailed)),
                },
                Err(error) => Err(Problem::from(error)),
            };
            // Тот же файл можно будет выбрать снова: без сброса браузер
            // не сообщит о повторном выборе.
            input.set_value("");
            restoring.set(match opened {
                Ok(sealed) => Restoring::Password(Rc::new(sealed), None),
                Err(problem) => Restoring::Failed(problem),
            });
        });
    };

    let submit = move |event: SubmitEvent| {
        event.prevent_default();
        let Restoring::Password(sealed, _) = restoring.get_untracked() else {
            return;
        };
        let typed = password.get_untracked();
        restoring.set(Restoring::Busy);
        leptos::task::spawn_local(async move {
            let outcome = match restoring_task(backups, Rc::clone(&sealed), typed).await {
                Ok(task) => task.await,
                Err(_) => Err(Problem::Say(Key::PartFailed)),
            };
            if outcome.is_ok() {
                password.set(String::new());
            }
            restoring.set(match outcome {
                Ok(restored) => Restoring::Done(restored),
                // Пароль не подошёл — спросить снова, а не выбирать файл
                // заново.
                Err(problem @ Problem::Say(Key::BackupWrongFilePassword)) => {
                    Restoring::Password(sealed, Some(problem))
                }
                Err(problem) => Restoring::Failed(problem),
            });
        });
    };

    view! {
        <p class="restore">{tr(lang, Key::BackupRestoreAbout)}</p>
        {move || match restoring.get() {
            Restoring::Password(sealed, problem) => {
                forms.hold();
                let header = sealed.header();
                let version = header.app_version().to_owned();
                let info = clock::local_date(header.created()).map(|day| move || {
                    let lang = lang.get();
                    fill(t(lang, Key::BackupFileInfo), &[&i18n::date(lang, day), &version])
                });
                view! {
                    {info.map(|info| view! { <p>{info}</p> })}
                    <form class="backup" on:submit=submit>
                        <label>
                            {tr(lang, Key::BackupFilePassword)}
                            <input type="password" autocomplete="current-password" bind:value=password />
                        </label>
                        {problem.map(|problem| view! {
                            <p role="alert">{move || problem.text(lang.get())}</p>
                        })}
                        <div class="actions">
                            <button type="submit">{tr(lang, Key::BackupRestore)}</button>
                            <button
                                type="button"
                                class="secondary"
                                on:click=move |_| {
                                    password.set(String::new());
                                    restoring.set(Restoring::Idle);
                                }
                            >
                                {tr(lang, Key::Cancel)}
                            </button>
                        </div>
                    </form>
                }
                .into_any()
            }
            Restoring::Busy => {
                forms.hold();
                view! {
                    <p role="status">{tr(lang, Key::BackupRestoring)}</p>
                }
                .into_any()
            }
            state => view! {
                // Поле выбора файла спрятано под подписью-кнопкой: нажатие на
                // подпись открывает выбор без кода, и на iPhone тоже
                // (разведка задачи 7, 2026-10-12).
                <label class="button link file">
                    {tr(lang, Key::BackupChooseFile)}
                    <input
                        type="file"
                        class="visually-hidden"
                        accept=".json,application/json"
                        on:change=on_file
                    />
                </label>
                {match state {
                    Restoring::Done(restored) => Some(view! {
                        <p role="status">
                            {move || {
                                let lang = lang.get();
                                fill(
                                    t(lang, Key::BackupRestored),
                                    &[
                                        &i18n::integer(lang, restored.added as i64),
                                        &i18n::integer(lang, restored.updated as i64),
                                        &i18n::integer(lang, restored.unchanged as i64),
                                    ],
                                )
                            }}
                        </p>
                    }.into_any()),
                    Restoring::Failed(problem) => Some(view! {
                        <p role="alert">{move || problem.text(lang.get())}</p>
                    }.into_any()),
                    _ => None,
                }}
            }
            .into_any(),
        }}
    }
}

/// Расшифровать копию и слить с базой.
async fn restore(backups: Backups, sealed: &Sealed, typed: &str) -> Result<Restored, Problem> {
    let password = crypto::normalize(typed);
    let (entries, key) = stored::unlock(KINDS, sealed, &password).await?;
    let store = backups.base.get().ok_or(Problem::Say(Key::BackupFailed))?;
    let restored = stored::restore(&store, KINDS, &entries).await?;
    if restored.added + restored.updated > 0 {
        backups.base.replaced();
    }
    let device = Device::new();
    let lock = sealed.lock();
    let opened_with = DeviceKey::new(lock.iterations(), lock.salt().to_vec(), key.check().to_vec());
    // Своего пароля у устройства нет — берётся пароль этой копии: человек
    // только что показал, что знает его, и следующая копия будет под ним же.
    match (device.backup_key(), opened_with) {
        (None, Some(opened_with)) => {
            device.set_backup_key(&opened_with);
            backups.key_set.set(true);
            backups.password_entered();
        }
        // Копия этого же пароля — пароль, значит, помнится.
        (Some(current), Some(opened_with)) if current == opened_with => backups.password_entered(),
        _ => {}
    }
    if let Some(at) = backup::last_after_restore(
        device.last_backup(),
        sealed.header().created(),
        clock::now(),
    ) {
        device.set_last_backup(at);
        backups.last.set(clock::local_date(at));
    }
    if !entries.is_empty() {
        backups.has_data.set(true);
    }
    Ok(restored)
}

/// Автокопия от базы до файла (задача 29): настоящая база программы,
/// настоящий WebCrypto и папка из OPFS — выбор папки человеком тест не
/// нажмёт, а OPFS отдаёт такую же `FileSystemDirectoryHandle`.
#[cfg(all(test, target_arch = "wasm32"))]
mod tests {
    use std::cell::RefCell;
    use std::future::Future;

    use kvartik_core::format::backup::open;
    use kvartik_core::model::client::{Client, ClientData};
    use kvartik_core::model::country::Country;
    use kvartik_core::model::money::Currency;
    use kvartik_core::model::text::{Line, Lines};
    use wasm_bindgen_test::wasm_bindgen_test;

    use super::*;
    use crate::store::tests::autobackup::{drop_scratch, read_file, scratch};
    use crate::store::{Store, Writes};

    /// Пароль с армянскими и русскими буквами, как у тестов копии.
    const PASSWORD: &str = "Գաղտնաբառ автокопии 2026";

    /// Метка устройства теста.
    const TAG: &str = "1a2b";

    /// Записи устройства, которые трогает автокопия.
    const DEVICE: [&str; 6] = [
        "kvartik.last_backup",
        "kvartik.backup_key",
        "kvartik.last_auto",
        "kvartik.changed",
        "kvartik.auto_tag",
        "kvartik.password_entered",
    ];

    fn forget_device() {
        if let Some(storage) = web_sys::window().and_then(|window| window.local_storage().ok().flatten()) {
            for name in DEVICE {
                let _ = storage.remove_item(name);
            }
        }
    }

    /// Пароль устройства из `password`.
    async fn device_password(password: &str) -> DeviceKey {
        let salt = crypto::random(SALT_BYTES).expect("соль");
        let key = crypto::derive(&crypto::normalize(password), &salt, KDF_ITERATIONS)
            .await
            .expect("ключ получен");
        DeviceKey::new(KDF_ITERATIONS, salt, key.check().to_vec()).expect("пароль устройства")
    }

    /// Окно копий над базой программы.
    fn backups_over(store: Rc<Store>) -> Backups {
        let base = Base::new();
        base.opened(store);
        Backups::new(base)
    }

    async fn run(backups: Backups, job: Job) -> Result<Made, Problem> {
        match making_task(backups, job).await {
            Ok(task) => task.await,
            Err(_) => Err(Problem::Say(Key::PartFailed)),
        }
    }

    fn enabling(folder: &FileSystemDirectoryHandle, typed: &str, place: Option<String>) -> Job {
        Job::Enable {
            typed: typed.to_owned(),
            repeat: None,
            folder: folder.clone(),
            place,
        }
    }

    /// Автокопию включают паролем копий в выбранную папку: программа
    /// заводит в ней свою «Kvartik» и хранит только её, а ключ — со своей
    /// солью, не устройства. Автокопия кладёт туда файл дня с меткой
    /// устройства, и он открывается паролем копий со всеми записями базы. Из
    /// своих файлов остаются семь самых свежих, брошенный временный файл
    /// прошлого дня убирается, а чужие не трогаются. Сделанная автокопия —
    /// это копия: день последней копии сдвигается. Пароль сменили —
    /// автокопия ждёт, а включённая снова тем же паролем в ту же папку не
    /// заводит в ней ещё одну; выключили — не пишет вовсе.
    #[wasm_bindgen_test]
    async fn auto_backup_lands_in_its_folder_and_opens_with_the_password() {
        if !folders::available() {
            web_sys::console::log_1(&"автокопии в этом браузере нет — тест пропущен".into());
            return;
        }
        crate::store::delete_database(crate::store::DB_NAME).await;
        let _ = remembered::forget().await;
        forget_device();
        let device = Device::new();
        device.set_auto_tag(DeviceTag::parse(TAG).expect("метка"));
        let store = Rc::new(Store::open(|_| {}).await.expect("база открыта"));
        let client = Client::create(
            ClientData {
                name: Line::new("Ереван ֏ Acme").expect("имя"),
                country: Country::from_code("US"),
                address: Lines::default(),
                tin: None,
                tax_id: Line::default(),
                currency: Some(Currency::Usd),
                lang: Some(Lang::En),
                payment_days: Some(15),
            },
            clock::now(),
        )
        .expect("годный клиент");
        let mut writes = Writes::default();
        writes.insert(&client);
        store
            .write()
            .expect("транзакция записи")
            .commit(writes)
            .await
            .expect("клиент записан");

        let (chosen, scratch_name) = scratch().await;
        let backups = backups_over(Rc::clone(&store));
        let mut wrong = Vec::new();
        if !matches!(run(backups, Job::Status).await, Ok(Made::Status(None))) {
            wrong.push("выключенная автокопия названа включённой".to_owned());
        }
        let device_key = device_password(PASSWORD).await;
        device.set_backup_key(&device_key);
        if !matches!(
            run(backups, enabling(&chosen, "не тот пароль копий", None)).await,
            Err(Problem::Say(Key::PasswordWrong))
        ) {
            wrong.push("включилась не тем паролем".to_owned());
        }
        let folder = match run(backups, enabling(&chosen, PASSWORD, None)).await {
            Ok(Made::Status(Some(place))) => {
                if place.name != format!("{scratch_name}/Kvartik") || place.folder.name() != "Kvartik" {
                    wrong.push(format!("папка «{}» — «{}»", place.name, place.folder.name()));
                }
                if place.permission != Permission::Granted {
                    wrong.push(format!("разрешение: {:?}", place.permission));
                }
                place.folder
            }
            _ => panic!("автокопия не включилась"),
        };
        match remembered::load().await {
            Ok(Some(saved)) => {
                if saved.lock.salt() == device_key.salt() {
                    wrong.push("у ключа автокопии соль устройства".to_owned());
                }
                if saved.device != device_key {
                    wrong.push("запомнен не тот пароль устройства".to_owned());
                }
            }
            _ => wrong.push("включённая автокопия не запомнилась".to_owned()),
        }

        let old: Vec<String> = (1..=8).map(|day| format!("kvartik-auto-2020-01-0{day}-{TAG}.json")).collect();
        let swap = format!("kvartik-auto-2020-01-08-{TAG}.json.crswap");
        let foreign = [
            "kvartik-auto-2020-01-01-9f9f.json",
            "kvartik-2020-01-01.json",
            "Счёт 2020-001.pdf",
        ];
        for name in old.iter().map(String::as_str).chain([swap.as_str()]).chain(foreign) {
            folders::write(&folder, name, b"{}").await.expect("файл проб записан");
        }
        let today = clock::today().expect("часы с датой");
        let fresh = autobackup::file_name(today, DeviceTag::parse(TAG).expect("метка"));
        match run(backups, Job::Auto).await {
            Ok(Made::Ran(Ok(Wrote { at, unreadable }))) => {
                if unreadable != 0 {
                    wrong.push(format!("не вошло записей: {unreadable}"));
                }
                if device.last_auto() != Some(at) || device.last_backup() != Some(at) {
                    wrong.push("день копии не сдвинулся".to_owned());
                }
                backups.ran(Made::Ran(Ok(Wrote { at, unreadable })));
                if backups.auto().with_untracked(|auto| auto.last) != Some(at) {
                    wrong.push("окно не знает о сделанной автокопии".to_owned());
                }
            }
            Ok(Made::Ran(Err(hitch))) => wrong.push(format!("автокопия: {hitch:?}")),
            Ok(_) => wrong.push("автокопия не записалась".to_owned()),
            Err(problem) => wrong.push(format!("автокопия: {problem:?}")),
        }
        let mut names = folders::names(&folder, "").await.expect("папка читается");
        names.sort_unstable();
        let mut expected: Vec<String> = old[2..]
            .iter()
            .cloned()
            .chain(foreign.iter().map(|name| (*name).to_owned()))
            .chain([fresh.clone()])
            .collect();
        expected.sort_unstable();
        if names != expected {
            wrong.push(format!("в папке {names:?}, а не {expected:?}"));
        }
        if !folders::names(&chosen, "").await.is_ok_and(|names| names.is_empty()) {
            wrong.push("в выбранную папку легли файлы, а не в её «Kvartik»".to_owned());
        }
        match read_file(&folder, &fresh).await.map(|text| open(&text)) {
            Some(Ok(sealed)) => match stored::unlock(KINDS, &sealed, &crypto::normalize(PASSWORD)).await {
                Ok((entries, _)) => {
                    if !entries.iter().any(|entry| entry.meta().id() == client.id()) {
                        wrong.push("в файле нет клиента".to_owned());
                    }
                }
                Err(failure) => wrong.push(format!("файл не открылся паролем: {failure:?}")),
            },
            Some(Err(error)) => wrong.push(format!("файл — не копия: {error:?}")),
            None => wrong.push("файла дня нет".to_owned()),
        }

        if !matches!(run(backups, Job::Check(PASSWORD.to_owned())).await, Ok(Made::Checked)) {
            wrong.push("верный пароль не подошёл".to_owned());
        }
        if !matches!(
            run(backups, Job::Check("не тот пароль копий".to_owned())).await,
            Err(Problem::Say(Key::PasswordWrong))
        ) {
            wrong.push("неверный пароль подошёл".to_owned());
        }

        // «Забыли пароль?» — новый пароль устройства: автокопия под прежним
        // не пишет и ждёт, а включённая снова в ту же папку новым паролем
        // продолжает в ней же.
        let renewed = "новый пароль копий 2026 года";
        device.set_backup_key(&device_password(renewed).await);
        if !matches!(run(backups, Job::Auto).await, Ok(Made::Ran(Err(Hitch::PasswordChanged)))) {
            wrong.push("автокопия пишет под прежним паролем".to_owned());
        }
        match run(backups, enabling(&folder, renewed, Some(format!("{scratch_name}/Kvartik")))).await {
            Ok(Made::Status(Some(place))) => {
                if place.folder.is_same_entry(&folder).await.ok().and_then(|same| same.as_bool()) != Some(true) {
                    wrong.push("включённая снова автокопия завела новую папку".to_owned());
                }
            }
            _ => wrong.push("автокопия не включилась снова".to_owned()),
        }
        if !matches!(run(backups, Job::Auto).await, Ok(Made::Ran(Ok(_)))) {
            wrong.push("включённая снова автокопия не пишет".to_owned());
        }

        if !matches!(run(backups, Job::Off).await, Ok(Made::Off)) {
            wrong.push("автокопия не выключилась".to_owned());
        }
        if !matches!(run(backups, Job::Auto).await, Ok(Made::Off)) {
            wrong.push("выключенная автокопия пишет".to_owned());
        }
        if read_file(&folder, &fresh).await.is_none() {
            wrong.push("выключенная автокопия убрала свой файл".to_owned());
        }

        drop(store);
        drop_scratch(&scratch_name).await;
        forget_device();
        crate::store::delete_database(crate::store::DB_NAME).await;
        assert!(wrong.is_empty(), "автокопия: {wrong:#?}");
    }

    /// Две работы под одной блокировкой не идут разом: вторая начинается,
    /// когда кончилась первая (`platform::autobackup::alone`).
    #[wasm_bindgen_test]
    async fn auto_backups_take_turns() {
        let order = Rc::new(RefCell::new(Vec::new()));
        let step = |name: &'static str, ms: i32| {
            let order = Rc::clone(&order);
            async move {
                order.borrow_mut().push(format!("{name}: начало"));
                let promise = js_sys::Promise::new(&mut |resolve, _| {
                    let _ = web_sys::window()
                        .expect("окно есть")
                        .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms);
                });
                let _ = promise.await;
                order.borrow_mut().push(format!("{name}: конец"));
                name
            }
        };
        let first = folders::alone("kvartik-test-lock", step("первая", 80));
        let second = folders::alone("kvartik-test-lock", step("вторая", 10));
        let (first, second) = futures_join(first, second).await;
        assert_eq!((first, second), (Some("первая"), Some("вторая")));
        assert_eq!(
            *order.borrow(),
            ["первая: начало", "первая: конец", "вторая: начало", "вторая: конец"]
        );
    }

    /// Дождаться двух будущих разом — без крейта `futures` и без исполнителя
    /// Leptos: обе опрашиваются здесь же, первая — первой, и ждут, пока
    /// не кончатся обе. Задачами (`leptos::task::spawn_local`) их запускать
    /// нельзя: исполнитель заводит монтирование окна в соседнем тесте, и тест,
    /// запущенный раньше всех, падал «before a global executor was
    /// initialized» — порядок тестов меняется с раскладкой wasm (CI 0.0.83,
    /// 2026-11-05).
    async fn futures_join<A, B>(a: impl Future<Output = A>, b: impl Future<Output = B>) -> (A, B) {
        use std::task::Poll;
        let (mut a, mut b) = (std::pin::pin!(a), std::pin::pin!(b));
        let (mut left, mut right) = (None, None);
        std::future::poll_fn(|cx| {
            if left.is_none()
                && let Poll::Ready(value) = a.as_mut().poll(cx)
            {
                left = Some(value);
            }
            if right.is_none()
                && let Poll::Ready(value) = b.as_mut().poll(cx)
            {
                right = Some(value);
            }
            if left.is_some() && right.is_some() { Poll::Ready(()) } else { Poll::Pending }
        })
        .await;
        (
            left.expect("первая кончилась"),
            right.expect("вторая кончилась"),
        )
    }

    /// Напоминание о копии считается только по дню сделанной копии —
    /// автокопия его не глушит, а сдвигает (решение человека 2026-11-03).
    /// Ждущая разрешения автокопия, которой есть что писать, просит его на
    /// главном экране. Раз в месяц при включённой автокопии окно просит
    /// пароль; без автокопии — нет.
    #[wasm_bindgen_test]
    async fn reminder_follows_the_day_of_the_last_copy() {
        forget_device();
        let backups = backups_over(Rc::new(Store::open(|_| {}).await.expect("база открыта")));
        backups.opened(true);
        let mut wrong = Vec::new();
        let mut expect = |what: &str, due: bool, check: bool, ask: bool| {
            let seen = (backups.due(), backups.check_due(), backups.waits_for_permission());
            if seen != (due, check, ask) {
                wrong.push(format!("{what}: напоминание, пароль, разрешение — {seen:?}"));
            }
        };
        backups.edit(|auto| auto.state = AutoState::Off);
        expect("без автокопии и без копии", true, false, false);
        backups.edit(|auto| auto.state = AutoState::On(Permission::Granted));
        expect("автокопия может писать, а копии нет", true, true, false);
        backups.password_entered();
        expect("пароль ввели сейчас", true, false, false);
        backups.last.set(clock::today());
        expect("сделана копия сегодня", false, false, false);
        backups.edit(|auto| {
            auto.state = AutoState::On(Permission::Prompt);
            auto.pending = false;
        });
        expect("ждёт разрешения, но писать нечего", false, false, false);
        backups.edit(|auto| auto.pending = true);
        expect("ждёт разрешения, а писать есть что", false, false, true);
        backups.last.set(clock::today().and_then(|today| today.add_days(-backup::REMIND_AFTER_DAYS)));
        expect("последней копии неделя", true, false, true);
        backups.edit(|auto| {
            auto.state = AutoState::On(Permission::Granted);
            auto.entered = clock::today().and_then(|today| today.add_days(-autobackup::CHECK_AFTER_DAYS));
        });
        expect("пароль вводили месяц назад", true, true, false);
        forget_device();
        assert!(wrong.is_empty(), "напоминания: {wrong:#?}");
    }
}
