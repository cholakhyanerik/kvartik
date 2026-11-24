//! Автокопия в папку (задача 119 — файлом из `backup.rs`): выбор и смена
//! папки паролем копий, слова о ней на экране копии и на главном экране,
//! запись файла дня (`write_auto`) и её очередь между вкладками.

use super::*;

/// Автокопия на экране копии (задача 29) — из части книги
/// (`ledger::book_part`), как лицензия: своя часть завела бы общих кусков
/// на тринадцать файлов wasm (замер задачи 29; CLAUDE.md, Правило 6),
/// а главному wasm она не нужна (Правило 1).
#[derive(Clone, Copy)]
pub(in crate::ui) struct AutoView {
    // Поля — модулю раздела: собирает вид экран копии (`mod.rs`), а части
    // книги он идёт целиком.
    pub(super) lang: ReadSignal<Lang>,
    pub(super) backups: Backups,
    pub(super) forms: Forms,
}

/// Блок автокопии — его рисует часть книги. И копия для клиента, когда ИП
/// больше одного (задача 38): нужна она редко, и главному wasm её строки
/// ни к чему (Правило 1).
pub(in crate::ui) fn auto_view(view: AutoView) -> AnyView {
    let AutoView { lang, backups, forms } = view;
    view! {
        <AutoSection lang backups forms />
        {move || backups.office().is_some_and(|office| office.several()).then(|| view! {
            <ClientCopy lang backups forms />
        })}
    }
    .into_any()
}

/// Карточки автокопии на главном экране — из части книги, вместе
/// с карточками налога (`tax::content`): ждущая разрешения автокопия,
/// которой есть что писать, — с кнопкой «Разрешить запись»: это бывает
/// после каждого перезапуска браузера, и одно нажатие её чинит; и раз
/// в месяц — что пора ввести пароль копий (решение человека 2026-11-03).
/// Часть грузится, когда в базе есть данные, — тогда же автокопия и
/// начинает работать: без данных ей писать нечего.
#[component]
pub(in crate::ui) fn AutoCards(lang: ReadSignal<Lang>, backups: Backups) -> impl IntoView {
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
                        <span class="label">{tr(lang, Key::AutoAllow)}</span>
                    </button>
                </div>
            }
            .into_any())
        } else if check.get() {
            Some(view! {
                <div class="notice due" role="status">
                    {icon(Icon::Shield)}
                    <p class="reminder">{tr(lang, Key::AutoCheckDue)}</p>
                    <a class="button" href=Screen::Backup.href()><span class="label">{tr(lang, Key::BackupGoTo)}</span></a>
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
pub(super) fn AutoSection(lang: ReadSignal<Lang>, backups: Backups, forms: Forms) -> impl IntoView {
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
    // «Показать пароль» (задача 124): у формы включения и у проверки — свои.
    let shown = RwSignal::new(false);
    let shown_check = RwSignal::new(false);
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
        conceal(&event, shown);
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
        conceal(&event, shown_check);
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
            // Включилась — форма пропадает (`chosen_name` снят): так менеджер
            // паролей видит, что она отправлена (`ui/password.rs`).
            <form class="backup" on:submit=enable>
                <PasswordLogin />
                {move || if new_password() {
                    view! {
                        <PasswordInput lang label=Key::BackupNewPassword value=password shown fresh=true />
                        <PasswordInput lang label=Key::BackupRepeatPassword value=repeat shown fresh=true />
                    }
                    .into_any()
                } else {
                    view! { <PasswordInput lang label=Key::BackupPassword value=password shown /> }.into_any()
                }}
                <PasswordShow lang shown />
                <p class="hint">{tr(lang, Key::AutoPasswordNote)}</p>
                <div class="actions">
                    <button type="submit" disabled=move || busy.get()><span class="label">{tr(lang, Key::AutoEnable)}</span></button>
                    <button type="button" class="secondary" disabled=move || busy.get() on:click=cancel>
                        <span class="label">{tr(lang, Key::Cancel)}</span>
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
                        <span class="label">{tr(lang, Key::AutoChoose)}</span>
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
                        <button type="button" on:click=allow><span class="label">{tr(lang, Key::AutoAllow)}</span></button>
                    }.into_any()),
                    Permission::Denied => Some(view! {
                        <p role="alert">{tr(lang, Key::AutoDenied)}</p>
                    }.into_any()),
                }}
                {hitch}
                {move || (renewing.get() && !picking()).then(|| view! {
                    <button type="button" on:click=renew><span class="label">{tr(lang, Key::AutoRenew)}</span></button>
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
                        <PasswordLogin />
                        <p>{tr(lang, Key::AutoCheckDue)}</p>
                        <PasswordInput lang label=Key::BackupPassword value=typed_check shown=shown_check />
                        <PasswordShow lang shown=shown_check />
                        <button type="submit" class="wide" disabled=move || busy.get()>
                            <span class="label">{tr(lang, Key::AutoCheck)}</span>
                        </button>
                    </form>
                })}
                {form}
                {move || (!picking()).then(|| view! {
                    <div class="actions auto-actions">
                        <button type="button" class="secondary" disabled=move || busy.get() on:click=pick>
                            <span class="label">{tr(lang, Key::AutoChange)}</span>
                        </button>
                        <button type="button" class="secondary" disabled=move || busy.get() on:click=off>
                            <span class="label">{tr(lang, Key::AutoOff)}</span>
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
pub(super) async fn act(
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

/// Что помнит устройство об автокопии и что браузер разрешает с её папкой.
pub(super) async fn status() -> Made {
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
pub(super) async fn enable(
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
pub(super) async fn write_auto(backups: Backups) -> Made {
    match write_file(backups).await {
        Ok(Some(wrote)) => Made::Ran(Ok(wrote)),
        Ok(None) => Made::Off,
        Err(hitch) => Made::Ran(Err(hitch)),
    }
}

/// Собрать и зашифровать копию ключом из базы устройства, положить файл дня
/// в папку и убрать старые свои. `None` — автокопии нет: её выключили.
pub(super) async fn write_file(backups: Backups) -> Result<Option<Wrote>, Hitch> {
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
    let sealing = Sealing {
        header: &header,
        key: &key,
        salt: saved.lock.salt(),
        iterations: saved.lock.iterations(),
        limit: part_limit(),
    };
    let made = make_file(backups, &store, &sealing).await?;
    let (size, parts) = (made.size(), made.sheets.len());
    // Копия частями (задача 117) — файлы частей рядом, по одному: текст
    // записанного больше не нужен.
    let name = autobackup::file_name(today, tag);
    let mut written = Vec::with_capacity(parts);
    for sheet in made.sheets {
        let file = backup::part_file_name(&name, sheet.part);
        folders::write(&saved.folder, &file, sheet.text.as_bytes()).await?;
        written.push(file);
    }
    // Старые свои файлы и брошенные временные — уже после нового: не
    // убрались — копия всё равно записана, а лишнее уберёт следующая.
    // Сегодняшние, кроме только что записанных, — прежняя копия дня: у неё
    // частей могло быть больше (`autobackup::stale`).
    if let Ok(names) = folders::names(&saved.folder, autobackup::PREFIX).await {
        let written: Vec<&str> = written.iter().map(String::as_str).collect();
        let leftovers = names
            .iter()
            .map(String::as_str)
            .filter(|name| autobackup::leftover(name, tag, today));
        for name in autobackup::stale(names.iter().map(String::as_str), tag, today, &written)
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
    device.set_last_size(size, made.files, parts);
    Ok(Some(Wrote {
        at,
        unreadable: made.unreadable,
    }))
}
