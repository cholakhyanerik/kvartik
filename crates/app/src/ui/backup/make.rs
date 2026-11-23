//! Сделать копию (задача 119 — файлом из `backup.rs`): копия под паролем
//! и её файл, копия только этого ИП, проверка пароля копий, а снаружи —
//! напоминание о копии на главном экране, метка под заголовком и строка
//! профиля, слова о размере копии и файлах книги в ней.

use super::*;

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
                    <a class="button" href=Screen::Backup.href()><span class="label">{tr(lang, Key::BackupGoTo)}</span></a>
                </div>
            }
        })
    }
}

/// Когда была последняя копия — словами: «сегодня», «3 дня назад», «копии
/// ещё нет». Часы без даты — `None`.
pub(super) fn ago(lang: Lang, last: Option<Date>, today: Option<Date>) -> Option<String> {
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
        let due = backups.due();
        if nudge && !due {
            return None;
        }
        let text = ago(lang.get(), backups.last.get(), backups.today)?;
        Some(chip(Screen::Backup.href(), Icon::Shield, text, due, nudge))
    }
}

/// Метка — ссылка со значком и словами (дизайн «Kvartik Design»): у копии
/// и у срока лицензии (задача 92) одна разметка — и одна её копия в главном
/// wasm (Правило 1). `due` — пора: и заливкой, и словами; `nudge` — она же
/// на телефоне, на экране, а не в меню. Сменилось что-то — метку рисуют
/// заново, её слова короткие.
pub(in crate::ui) fn chip(href: String, glyph: Icon, words: String, due: bool, nudge: bool) -> impl IntoView {
    view! {
        <a class="chip" class:due=due class:nudge=nudge href=href>
            {icon(glyph)}
            <span>{words}</span>
        </a>
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

/// Мегабайты целыми — вверх: файл в 30 КБ — «1 МБ», а не ноль. Целыми,
/// а не с десятыми: запись дробного (`i18n::decimal`) легла бы общим куском
/// и в части копии и кода передачи — новый файл wasm в 16 КБ (замер
/// задачи 64), — а целое у них уже есть.
pub(super) fn megabytes(lang: Lang, bytes: u64) -> String {
    i18n::integer(lang, i64::try_from(bytes.div_ceil(1 << 20)).unwrap_or(i64::MAX))
}

/// Копия заняла три четверти предела и больше — слова о файлах книги
/// предупреждением: до отказа недалеко (задача 64).
pub(super) fn near_limit(size: u64) -> bool {
    size.saturating_mul(4) >= MAX_FILE_BYTES.saturating_mul(3)
}

/// Слова о файлах книги в последней копии: сколько весят, вся копия
/// и предел, а убранные из записей — если они есть.
pub(super) fn files_words(lang: Lang, size: u64, files: Files) -> String {
    let limit = i18n::integer(lang, (MAX_FILE_BYTES >> 20) as i64);
    let mut words = fill(
        t(lang, Key::BackupFiles),
        &[&megabytes(lang, files.bytes), &megabytes(lang, size), &limit],
    );
    if files.removed > 0 {
        words.push(' ');
        words.push_str(&fill(t(lang, Key::BackupFilesRemoved), &[&megabytes(lang, files.removed)]));
    }
    words
}

/// Копия открытого ИП для клиента (задача 38, решение человека 2026-11-05):
/// только его база — без других ИП и без списка, — под новым паролем: его
/// вводят дважды и не запоминают. Ни пароль копий устройства, ни день
/// последней копии она не трогает: копией программы она не считается.
/// Имя файла — по ՀՎՀՀ (`backup::client_file_name`).
#[component]
pub(super) fn ClientCopy(lang: ReadSignal<Lang>, backups: Backups, forms: Forms) -> impl IntoView {
    let password = RwSignal::new(String::new());
    let repeat = RwSignal::new(String::new());
    let making = RwSignal::new_local(Making::Idle);
    let busy = move || matches!(making.get(), Making::Busy);
    forms.hold_while(move || {
        password.with(|text| !text.is_empty())
            || repeat.with(|text| !text.is_empty())
            || matches!(making.get(), Making::Busy)
    });
    let submit = move |event: SubmitEvent| {
        event.prevent_default();
        if busy() {
            return;
        }
        let job = Job::Client {
            typed: password.get_untracked(),
            repeat: repeat.get_untracked(),
        };
        making.set(Making::Busy);
        leptos::task::spawn_local(async move {
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
            }
            making.set(match outcome {
                Ok(ready) => Making::Ready(Rc::new(ready)),
                Err(problem) => Making::Failed(problem),
            });
        });
    };
    view! {
        <h2>{tr(lang, Key::BackupClientTitle)}</h2>
        <p class="hint">{tr(lang, Key::BackupClientAbout)}</p>
        <form class="backup" on:submit=submit>
            <label>
                {tr(lang, Key::BackupFilePassword)}
                <input type="password" autocomplete="new-password" bind:value=password />
            </label>
            <label>
                {tr(lang, Key::BackupRepeatPassword)}
                <input type="password" autocomplete="new-password" bind:value=repeat />
            </label>
            <p class="hint">{counted(lang, Key::BackupClientPasswordHint, MIN_PASSWORD_CHARS as i64)}</p>
            <button type="submit" class="secondary" disabled=busy>
                {icon(Icon::Download)}
                <span class="label">{tr(lang, Key::BackupClientMake)}</span>
            </button>
        </form>
        {move || match making.get() {
            Making::Idle => None,
            Making::Busy => Some(view! { <p role="status">{tr(lang, Key::BackupMaking)}</p> }.into_any()),
            Making::Failed(problem) => Some(view! {
                <p role="alert">{move || problem.text(lang.get())}</p>
            }.into_any()),
            Making::Ready(ready) => {
                let name = ready.download.name();
                Some(view! {
                    <p role="status">{counted(lang, Key::BackupReady, ready.records as i64)}</p>
                    {(ready.unreadable > 0).then(|| view! {
                        <p role="alert">{counted(lang, Key::BackupUnreadable, ready.unreadable as i64)}</p>
                    })}
                    <div class="actions backup-file">
                        <a class="button" href=ready.download.url().to_owned() download=ready.download.name()>
                            <span class="label">{move || fill(t(lang.get(), Key::BackupSaveFile), &[&name])}</span>
                        </a>
                    </div>
                }.into_any())
            }
        }}
    }
}

/// Сделать копию: пароль, сборка и шифр, потом ссылка на файл. Введённый
/// пароль, копию в работе и готовый файл, пока открыт экран копии, новая
/// версия ждёт ([`Forms`]): ссылка на файл живёт, пока жива страница, а нажатие
/// на неё копию ещё не заканчивает — Safari на iPhone сначала спрашивает
/// о загрузке, и перезагрузка в этот миг оставила бы дату копии без файла.
#[component]
pub(super) fn MakeBackup(
    lang: ReadSignal<Lang>,
    backups: Backups,
    forms: Forms,
    router: Router,
    /// Спрятать: выбран файл копии или база пуста ([`BackupSection`]).
    quiet: Signal<bool>,
) -> impl IntoView {
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
        let device = Device::new();
        device.set_last_backup(now);
        // Сколько весит отданная копия (задача 64): экран говорит это под
        // меткой последней копии и перечитывает по её сигналу.
        if let Making::Ready(ready) = making.get_untracked() {
            device.set_last_size(ready.size, ready.files);
        }
        backups.last.set(clock::local_date(now));
        saved.set(Some(said));
    };
    let on_save = move |_: MouseEvent| handed(Key::BackupSaved);

    view! {
        <form class="backup" hidden=move || quiet.get() on:submit=submit>
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
                <span class="label">{tr(lang, Key::BackupMake)}</span>
            </button>
            {move || (!new_password()).then(|| view! {
                <button
                    type="button"
                    class="link"
                    disabled=busy
                    on:click=move |_| resetting.set(true)
                >
                    <span class="label">{tr(lang, Key::BackupForgot)}</span>
                </button>
            })}
        </form>
        {move || match making.get() {
            _ if quiet.get() => None,
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
                            <span class="label">{move || fill(t(lang.get(), Key::BackupSaveFile), &[&name])}</span>
                        </a>
                        {shares.then(|| view! {
                            <button type="button" class="secondary" on:click=send>
                                {icon(Icon::Share)}
                                <span class="label">{tr(lang, Key::BackupShare)}</span>
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

/// Собрать и зашифровать копию. `repeat` — повтор нового пароля; `None` —
/// пароль на устройстве уже задан, и введённый сверяется с ним.
pub(super) async fn make(backups: Backups, typed: &str, repeat: Option<&str>) -> Result<Ready, Problem> {
    let (key, device_key, fresh) = key_for(backups, typed, repeat).await?;
    let store = backups.base.get().ok_or(Problem::Say(Key::BackupFailed))?;
    let now = clock::now();
    let today = clock::local_date(now).ok_or(Problem::Say(Key::BackupFailed))?;
    let header = Header::new(env!("CARGO_PKG_VERSION"), now);
    let sealing = Sealing {
        header: &header,
        key: &key,
        salt: device_key.salt(),
        iterations: device_key.iterations(),
    };
    let made = make_file(backups, &store, &sealing).await?;
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
        size: made.file.len() as u64,
        files: made.files,
    })
}

/// Копия открытого ИП для клиента (задача 38): своя соль и свой ключ из
/// нового пароля — пароль копий устройства тут ни при чём и не меняется.
pub(super) async fn make_client(backups: Backups, typed: &str, repeat: &str) -> Result<Ready, Problem> {
    let password = crypto::normalize(typed);
    if let Some(problem) = backup::new_password_problem(&password, &crypto::normalize(repeat)) {
        return Err(problem.into());
    }
    let salt = crypto::random(SALT_BYTES)?;
    let key = crypto::derive(&password, &salt, KDF_ITERATIONS).await?;
    let store = backups.base.get().ok_or(Problem::Say(Key::BackupFailed))?;
    let now = clock::now();
    let today = clock::local_date(now).ok_or(Problem::Say(Key::BackupFailed))?;
    let header = Header::new(env!("CARGO_PKG_VERSION"), now);
    let sealing = Sealing {
        header: &header,
        key: &key,
        salt: &salt,
        iterations: KDF_ITERATIONS,
    };
    let made = stored::make(&store, KINDS, &sealing).await?;
    // ՀՎՀՀ — из профиля этой базы; не читается — имя без него.
    let tin = match store.read() {
        Ok(tx) => tx.all::<Profile>().await.ok().and_then(Profile::current).and_then(|profile| profile.data().tin.clone()),
        Err(_) => None,
    };
    let download = Download::text(&made.file, &backup::client_file_name(tin.as_ref(), today))
        .ok_or(Problem::Say(Key::BackupFailed))?;
    Ok(Ready {
        download,
        records: made.records,
        unreadable: made.unreadable,
        size: made.file.len() as u64,
        files: made.files,
    })
}

/// Копия программы — ручная и автокопия: у кого список ИП (задача 38) —
/// всех ИП одним файлом ([`stored::make_all`]), у кого его нет — базы
/// открытого ИП, как всегда.
pub(super) async fn make_file(backups: Backups, store: &Store, sealing: &Sealing<'_>) -> Result<stored::Made, Failure> {
    if let Some(office) = backups.office()
        && let Some((office, open_name)) = office.full().await?
    {
        return stored::make_all(store, &open_name, &office, sealing).await;
    }
    stored::make(store, KINDS, sealing).await
}

/// Проверить пароль копий: тот ли, что на устройстве.
pub(super) async fn check_password(typed: &str) -> Result<Made, Problem> {
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
