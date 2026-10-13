//! Раздел «Резервная копия» (задача 7): сделать копию под паролем,
//! восстановить из неё, напомнить, если копии давно не было.
//!
//! Экрана настроек ещё нет, и раздел стоит на единственном экране каркаса.
//!
//! Копия делается в два шага: собрать и зашифровать — потом сохранить
//! ссылкой, которую человек нажимает сам (`platform::file::Download`).
//! Пароль спрашивается при каждой копии, чтобы не забылся, а опечатку ловит
//! проверочное значение устройства (`DeviceKey`).

use std::rc::Rc;

use kvartik_core::format::backup::{
    self, BackupError, DeviceKey, Header, KDF_ITERATIONS, KINDS, MAX_FILE_BYTES,
    MIN_PASSWORD_CHARS, PasswordProblem, SALT_BYTES, Sealed,
};
use kvartik_core::i18n::{self, Key, Lang, fill, t};
use kvartik_core::model::date::Date;
use leptos::ev::{Event, MouseEvent, SubmitEvent};
use leptos::prelude::*;
use web_sys::HtmlInputElement;

use super::split::{Task, split};
use super::storage::{Storage, error_key};
use super::{Base, tr};
use crate::platform::clock;
use crate::platform::crypto::{self, CryptoError};
use crate::platform::file::{self, Download, FileError};
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
}

impl Backups {
    /// Что известно до открытия базы: последняя копия и пароль — из
    /// хранилища устройства, сегодня — из часов.
    pub fn new(base: Base) -> Self {
        let device = Device::new();
        Self {
            base,
            last: RwSignal::new(device.last_backup().and_then(clock::local_date)),
            key_set: RwSignal::new(device.backup_key().is_some()),
            has_data: RwSignal::new(false),
            today: clock::today(),
        }
    }

    /// База открылась, и в ней есть данные — или нет.
    pub fn opened(&self, has_data: bool) {
        self.has_data.set(has_data);
    }

    /// Действие сохранило данные: теперь есть что терять, и без копии
    /// напоминание о ней появляется сразу, а не со следующим открытием.
    pub fn data_saved(&self) {
        self.has_data.set(true);
    }

    /// Пора ли напомнить о копии (Правило 2). Часы без даты — не повод.
    fn due(&self) -> bool {
        self.today
            .is_some_and(|today| backup::reminder_due(self.last.get(), today, self.has_data.get()))
    }
}

/// `id` раздела копии: на него ведёт ссылка напоминания.
const SECTION_ID: &str = "backup";

/// Напоминание о копии — вверху окна, над разделами с данными: окно растёт,
/// и раздел копии внизу при открытии уже не виден, а напоминать обязано
/// каждое открытие (Правило 2). Ссылка ведёт к разделу копии.
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
                <div class="notice" role="status">
                    <p class="reminder">{tr(lang, key)}</p>
                    <a class="button" href=format!("#{SECTION_ID}")>{tr(lang, Key::BackupGoTo)}</a>
                </div>
            }
        })
    }
}

/// Что сказать человеку, кроме простых строк: шаблоны с числами.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Problem {
    Say(Key),
    TooShort,
    TooLarge,
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
    name: String,
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

/// Раздел на языке окна. Показан, только пока хранилище открыто: без базы
/// копии не из чего делать и некуда восстанавливать.
#[component]
pub fn BackupSection(
    lang: ReadSignal<Lang>,
    backups: Backups,
    storage: RwSignal<Storage>,
) -> impl IntoView {
    let ready = move || matches!(storage.get(), Storage::Ready(_));
    view! {
        <Show when=ready>
            <h2 id=SECTION_ID>{tr(lang, Key::BackupTitle)}</h2>
            <p>{tr(lang, Key::BackupAbout)}</p>
            <p>
                {move || {
                    let lang = lang.get();
                    match backups.last.get() {
                        Some(day) => fill(t(lang, Key::BackupLast), &[&i18n::date(lang, day)]),
                        None => t(lang, Key::BackupNone).to_owned(),
                    }
                }}
            </p>
            <MakeBackup lang backups />
            <RestoreBackup lang backups />
        </Show>
    }
}

/// Сделать копию: пароль, сборка и шифр, потом ссылка на файл.
#[component]
fn MakeBackup(lang: ReadSignal<Lang>, backups: Backups) -> impl IntoView {
    let password = RwSignal::new(String::new());
    let repeat = RwSignal::new(String::new());
    // «Забыли пароль?» — новый пароль поверх заданного.
    let resetting = RwSignal::new(false);
    let making = RwSignal::new_local(Making::Idle);
    let saved = RwSignal::new(false);
    let new_password = move || !backups.key_set.get() || resetting.get();
    let busy = move || matches!(making.get(), Making::Busy);

    let submit = move |event: SubmitEvent| {
        event.prevent_default();
        if busy() {
            return;
        }
        let fresh = new_password();
        let (typed, again) = (password.get_untracked(), repeat.get_untracked());
        making.set(Making::Busy);
        saved.set(false);
        leptos::task::spawn_local(async move {
            let outcome = match making_task(backups, typed, fresh.then_some(again)).await {
                Ok(task) => task.await,
                Err(_) => Err(Problem::Say(Key::PartFailed)),
            };
            if outcome.is_ok() {
                password.set(String::new());
                repeat.set(String::new());
                resetting.set(false);
            }
            making.set(match outcome {
                Ok(ready) => Making::Ready(Rc::new(ready)),
                Err(problem) => Making::Failed(problem),
            });
        });
    };

    // Файл отдан браузеру: это и есть копия на устройстве. Сохранил ли
    // браузер файл, он не говорит.
    let on_save = move |_: MouseEvent| {
        let now = clock::now();
        Device::new().set_last_backup(now);
        backups.last.set(clock::local_date(now));
        saved.set(true);
    };

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
            <div class="actions">
                <button type="submit" disabled=busy>{tr(lang, Key::BackupMake)}</button>
                {move || (!new_password()).then(|| view! {
                    <button
                        type="button"
                        class="secondary"
                        disabled=busy
                        on:click=move |_| resetting.set(true)
                    >
                        {tr(lang, Key::BackupForgot)}
                    </button>
                })}
            </div>
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
                let name = ready.name.clone();
                Some(view! {
                    <p role="status">{counted(lang, Key::BackupReady, records)}</p>
                    {(unreadable > 0).then(|| view! {
                        <p role="alert">{counted(lang, Key::BackupUnreadable, unreadable)}</p>
                    })}
                    <p>
                        <a
                            class="button"
                            href=ready.download.url().to_owned()
                            download=ready.name.clone()
                            on:click=on_save
                        >
                            {move || fill(t(lang.get(), Key::BackupSaveFile), &[&name])}
                        </a>
                    </p>
                    {move || saved.get().then(|| view! {
                        <p role="status">{tr(lang, Key::BackupSaved)}</p>
                    })}
                }.into_any())
            }
        }}
    }
}

split! {
    /// Сборка и шифр копии — своей частью wasm (`ui/split.rs`): нужны только
    /// по «Сделать копию».
    pub(super) fn making_task(
        backups: Backups,
        typed: String,
        repeat: Option<String>,
    ) -> Task<Result<Ready, Problem>>, preload preload_make
    {
        Box::pin(async move { make(backups, &typed, repeat.as_deref()).await })
    }
}

split! {
    /// Разбор выбранного файла копии — своей частью wasm (`ui/split.rs`).
    pub(super) fn opening(text: String) -> Result<Sealed, Problem>, preload preload_open {
        backup::open(&text).map_err(Problem::from)
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

/// Собрать и зашифровать копию. `repeat` — повтор нового пароля; `None` —
/// пароль на устройстве уже задан, и введённый сверяется с ним.
async fn make(backups: Backups, typed: &str, repeat: Option<&str>) -> Result<Ready, Problem> {
    let password = crypto::normalize(typed);
    let device = Device::new();
    let current = match repeat {
        Some(_) => None,
        None => device.backup_key(),
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
    let store = backups.base.get().ok_or(Problem::Say(Key::BackupFailed))?;
    let now = clock::now();
    let today = clock::local_date(now).ok_or(Problem::Say(Key::BackupFailed))?;
    let header = Header::new(env!("CARGO_PKG_VERSION"), now);
    let made = stored::make(&store, KINDS, &header, &key, &salt, iterations).await?;
    let download = Download::new(&made.file).ok_or(Problem::Say(Key::BackupFailed))?;
    if current.is_none() {
        // Новый пароль запоминается, когда копия под ним уже собрана.
        if let Some(key) = DeviceKey::new(iterations, salt, key.check().to_vec()) {
            device.set_backup_key(&key);
        }
        backups.key_set.set(true);
    }
    Ok(Ready {
        download,
        name: backup::file_name(today),
        records: made.records,
        unreadable: made.unreadable,
    })
}

/// Восстановить из копии: выбрать файл, ввести его пароль, слить.
#[component]
fn RestoreBackup(lang: ReadSignal<Lang>, backups: Backups) -> impl IntoView {
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
                Ok(text) => opening(text)
                    .await
                    .unwrap_or(Err(Problem::Say(Key::PartFailed))),
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
            Restoring::Busy => view! {
                <p role="status">{tr(lang, Key::BackupRestoring)}</p>
            }
            .into_any(),
            state => view! {
                // Поле выбора файла спрятано под подписью-кнопкой: нажатие на
                // подпись открывает выбор без кода, и на iPhone тоже
                // (разведка задачи 7, 2026-10-12).
                <label class="button secondary file">
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
    // Своего пароля у устройства нет — берётся пароль этой копии: человек
    // только что показал, что знает его, и следующая копия будет под ним же.
    if device.backup_key().is_none() {
        let lock = sealed.lock();
        if let Some(key) = DeviceKey::new(
            lock.iterations(),
            lock.salt().to_vec(),
            key.check().to_vec(),
        ) {
            device.set_backup_key(&key);
            backups.key_set.set(true);
        }
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
