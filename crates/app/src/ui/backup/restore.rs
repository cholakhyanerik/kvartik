//! Восстановление из копии (задача 119 — файлом из `backup.rs`): выбор
//! файла и пароль, проверка копии и ответ человека, запись одного ИП
//! одной транзакцией, а куда кладётся копия другого ИП и копия всех ИП —
//! в `merge.rs`.

use super::*;

/// Восстановить из копии: выбрать файл, ввести его пароль, слить.
/// Пароль файла и само восстановление — открытая форма ([`Forms`]): новая
/// версия их ждёт, а её перезагрузка оборвала бы восстановление — записано
/// не было бы ничего, а человек мог бы решить, что было (задача 67).
#[component]
pub(super) fn RestoreBackup(
    lang: ReadSignal<Lang>,
    backups: Backups,
    forms: Forms,
    /// Выбран ли файл копии: от выбора до конца восстановления или отказа
    /// ([`BackupSection`] прячет на это время «Сделать копию»).
    picked: RwSignal<bool>,
) -> impl IntoView {
    let restoring = RwSignal::new_local(Restoring::Idle);
    // Копия всех ИП легла в пустую программу частью (ревью задачи 41,
    // задача 116): спросить, где выпускать, — и сразу, под словами о сбое,
    // и после повтора: к повтору список ИП уже записан, и программа для
    // него не пустая.
    let fresh_partly = RwSignal::new(false);
    // Переход к ответу: частичная копия запоминает, что программа была
    // пуста, а легшая после неё — спрашивает и тогда.
    let landed = move |outcome: Recovered| -> Option<Restoring> {
        Some(match outcome {
            Recovered::Done(restored, after) => Restoring::Done(
                restored,
                After {
                    fresh: after.fresh || fresh_partly.get_untracked(),
                    ..after
                },
            ),
            Recovered::AsNew => Restoring::AsNew,
            Recovered::Elsewhere(ip) => Restoring::Elsewhere(ip),
            Recovered::Partial(why, fresh) => {
                if fresh {
                    fresh_partly.set(true);
                }
                Restoring::Failed(Problem::Partly(why))
            }
            Recovered::Other(_) => return None,
        })
    };
    Effect::new(move |_| {
        picked.set(restoring.with(|state| {
            matches!(state, Restoring::Busy | Restoring::Password(..) | Restoring::Other(..))
        }));
    });
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
            let job = RestoreJob::Open(Rc::clone(&sealed), typed);
            let outcome = match restoring_task(backups, job).await {
                Ok(task) => task.await,
                Err(_) => Err(Problem::Say(Key::PartFailed)),
            };
            if outcome.is_ok() {
                password.set(String::new());
            }
            restoring.set(match outcome {
                Ok(Recovered::Other(asked)) => Restoring::Other(asked),
                Ok(outcome) => landed(outcome).unwrap_or(Restoring::Failed(Problem::Say(Key::BackupFailed))),
                // Пароль не подошёл — спросить снова, а не выбирать файл
                // заново.
                Err(problem @ Problem::Say(Key::BackupWrongFilePassword)) => {
                    Restoring::Password(sealed, Some(problem))
                }
                Err(problem) => Restoring::Failed(problem),
            });
        });
    };

    // Ответ на вопрос о копии другого ИП — «Всё равно слить» или «Добавить
    // как нового ИП»: та же часть wasm, без пароля.
    let answer = move |job: fn(Rc<Asked>) -> RestoreJob| {
        let Restoring::Other(asked) = restoring.get_untracked() else {
            return;
        };
        restoring.set(Restoring::Busy);
        leptos::task::spawn_local(async move {
            let outcome = match restoring_task(backups, job(asked)).await {
                Ok(task) => task.await,
                Err(_) => Err(Problem::Say(Key::PartFailed)),
            };
            restoring.set(match outcome {
                // С ответом хранилище второй раз не спрашивает.
                Ok(outcome) => landed(outcome).unwrap_or(Restoring::Failed(Problem::Say(Key::BackupFailed))),
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
                            <button type="submit"><span class="label">{tr(lang, Key::BackupRestore)}</span></button>
                            <button
                                type="button"
                                class="secondary"
                                on:click=move |_| {
                                    password.set(String::new());
                                    restoring.set(Restoring::Idle);
                                }
                            >
                                <span class="label">{tr(lang, Key::Cancel)}</span>
                            </button>
                        </div>
                    </form>
                }
                .into_any()
            }
            Restoring::Other(asked) => {
                // Ответа ещё нет, а копия расшифрована и ждёт: новая версия
                // тоже ждёт, как у пароля файла.
                forms.hold();
                let ours = asked.ours.clone();
                let theirs = asked.theirs.clone();
                view! {
                    <div class="notice">
                        <p role="alert">{tr(lang, Key::BackupOtherIe)}</p>
                        <p>{move || {
                            let lang = lang.get();
                            let ours = ours.as_ref().map_or_else(
                                || t(lang, Key::BackupOtherIeUnreadable).to_owned(),
                                |ours| who(lang, ours),
                            );
                            fill(t(lang, Key::BackupOtherIeOurs), &[&ours])
                        }}</p>
                        <p>{move || fill(t(lang.get(), Key::BackupOtherIeTheirs), &[&who(lang.get(), &theirs)])}</p>
                        <p>{tr(lang, Key::BackupOtherIeWhy)}</p>
                        // Безопасный ответ — первым и главным: смешанные
                        // ИП потом не разделить одной кнопкой.
                        <div class="actions">
                            <button type="button" on:click=move |_| restoring.set(Restoring::Idle)>
                                <span class="label">{tr(lang, Key::Cancel)}</span>
                            </button>
                            <button type="button" class="secondary" on:click=move |_| answer(RestoreJob::AsNew)>
                                <span class="label">{tr(lang, Key::BackupOtherIeAsNew)}</span>
                            </button>
                            <button type="button" class="secondary" on:click=move |_| answer(RestoreJob::Merge)>
                                <span class="label">{tr(lang, Key::BackupOtherIeMerge)}</span>
                            </button>
                        </div>
                    </div>
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
                    Restoring::Done(restored, after) => Some(view! {
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
                        {after.ahead.then(|| view! { <p class="warning">{tr(lang, Key::BackupAhead)}</p> })}
                        // Выпуск уже выключен — спрашивать не о чем.
                        {(after.fresh && restored.added > 0 && !issued_elsewhere(backups))
                            .then(|| view! { <IssuerAsk lang /> })}
                    }.into_any()),
                    // Другой ИП открывается на экране списка: переход
                    // перезагружает страницу, и его место там (`ui/office.rs`).
                    Restoring::AsNew => Some(view! {
                        <p role="status">{tr(lang, Key::BackupAsNewDone)}</p>
                        <a class="button" href=Screen::Office.href()><span class="label">{tr(lang, Key::OfficeTitle)}</span></a>
                    }.into_any()),
                    Restoring::Elsewhere(ip) => Some(view! {
                        <p role="alert">{move || {
                            let lang = lang.get();
                            let name = match ip.name(lang) {
                                "" => t(lang, Key::OfficeNameless),
                                name => name,
                            };
                            fill(t(lang, Key::BackupElsewhere), &[name])
                        }}</p>
                        <a class="button" href=Screen::Office.href()><span class="label">{tr(lang, Key::OfficeTitle)}</span></a>
                    }.into_any()),
                    Restoring::Failed(problem) => Some(view! {
                        <p role="alert">{move || problem.text(lang.get())}</p>
                        {(matches!(problem, Problem::Partly(_)) && fresh_partly.get_untracked() && !issued_elsewhere(backups))
                            .then(|| view! { <IssuerAsk lang /> })}
                    }.into_any()),
                    _ => None,
                }}
            }
            .into_any(),
        }}
    }
}

/// Расшифровать копию и слить с базой — или остановиться на вопросе: копия
/// другого ИП ([`backup::relation`]) без ответа человека не пишется.
pub(super) async fn restore(backups: Backups, job: RestoreJob) -> Result<Recovered, Problem> {
    let store = backups.base.get().ok_or(Problem::Say(Key::BackupFailed))?;
    match job {
        RestoreJob::Open(sealed, typed) => {
            let password = crypto::normalize(&typed);
            let (entries, key) = stored::unlock(FILE_KINDS, &sealed, &password).await?;
            let ahead = backup::ahead(&entries, clock::now());
            if backup::is_full(&entries) {
                return restore_full(backups, &store, (&sealed, &key), entries, ahead).await;
            }
            // ИП несколько, а копия — ИП из списка, не открытого: в открытую
            // базу её не класть, а отдельным ИП — тем более (ревью 2026-11-06).
            let several = Device::new().office();
            if several && let Some(office) = backups.office() {
                let office_store = office.store().await.map_err(Failure::from)?;
                let listed = office_list(&office_store).await.map_err(Failure::from)?.records;
                if let Some(ip) = office_route::listed(&entries, &listed).map_err(stored::newer)?
                    && ip.base() != office.open_name()
                {
                    return Ok(Recovered::Elsewhere(Arc::new(ip.clone())));
                }
            }
            // Копия одного ИП — копия данных программы, только когда ИП один:
            // тогда и день копии сдвигается, и пароль её — свой.
            let own = if several { SameIe::No } else { SameIe::Yes };
            // Пустая ли программа — до записи: после неё не узнать. Пустая
            // при нескольких ИП — ещё не значит новое устройство.
            let fresh = !several && empty(&store).await?;
            Ok(match stored::restore(&store, KINDS, &entries, OtherIe::Ask).await? {
                Merged::Done(restored) => Recovered::Done(
                    restored_from(backups, (&sealed, &key), restored, Landed::single(&restored, &entries, own)),
                    After { fresh, ahead },
                ),
                Merged::Other { ours, theirs } => Recovered::Other(Rc::new(Asked {
                    sealed,
                    entries,
                    key,
                    ours,
                    theirs,
                })),
            })
        }
        RestoreJob::Merge(asked) => {
            // Копия другого ИП: программа не пуста — в ней свой профиль.
            let ahead = backup::ahead(&asked.entries, clock::now());
            match stored::restore(&store, KINDS, &asked.entries, OtherIe::Merge).await? {
                Merged::Done(restored) => Ok(Recovered::Done(
                    restored_from(
                        backups,
                        (&asked.sealed, &asked.key),
                        restored,
                        Landed::single(&restored, &asked.entries, SameIe::No),
                    ),
                    After { fresh: false, ahead },
                )),
                // С ответом «слить» хранилище не спрашивает.
                Merged::Other { .. } => Err(Problem::Say(Key::BackupFailed)),
            }
        }
        RestoreJob::AsNew(asked) => restore_as_new(backups, &store, &asked).await,
    }
}

/// Нет ли в базе ни одной записи.
pub(super) async fn empty(store: &Store) -> Result<bool, Failure> {
    Ok(store.read()?.is_empty().await?)
}
