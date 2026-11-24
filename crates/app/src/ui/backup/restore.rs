//! Восстановление из копии (задача 119 — файлом из `backup.rs`): выбор
//! файла и пароль, проверка копии и ответ человека, запись одного ИП
//! одной транзакцией, а куда кладётся копия другого ИП и копия всех ИП —
//! в `merge.rs`.
//!
//! Копия частями (задача 117) восстанавливается по файлу: каждая часть —
//! копия сама по себе и ложится тем же путём. Окно помнит ключ прошлой части
//! ([`Known`]): следующая часть той же копии ложится сразу, без пароля. Ключ
//! окно забывает, как только легли все части или человек ушёл с экрана
//! копии (ревью задачи 117): экраны спрятаны, а не убраны, и иначе ключ из
//! пароля чужой копии жил бы до перезагрузки страницы.
//!
//! Какие части уже легли и была ли программа пуста до первой, окно помнит
//! записью устройства по идентификатору копии (`backup::Laid`, задача 123),
//! а не памятью экрана: части ложатся в разные заходы и после перезагрузки,
//! и слова «осталась часть 1», а потом «восстановлена целиком» и вопрос, где
//! выпускать, — по всем частям. До 0.0.183 номера забывались вместе с ключом,
//! слова называли номер легшей части голым числом («восстановлены: 2» —
//! человек прочёл «две из двух»), а вопрос вставал после первой же части,
//! хотя записи без файлов книги — и документы с их номерами — копия кладёт
//! первыми (`backup::split`), а человек мог начать со второй (найдено
//! человеком 2026-11-24).

use super::*;

/// Ключ прошлой части копии (задача 117): у частей одной копии один замок.
/// Живёт, пока открыт экран копии и легли не все части: ключа не сохраняет
/// ничто, кроме памяти страницы.
pub(super) struct Known {
    copy: CopyId,
    iterations: u32,
    salt: Vec<u8>,
    key: Rc<crypto::Key>,
}

impl Known {
    /// Ключ для файла `sealed`, если это часть той же копии с тем же замком.
    fn key_for(&self, sealed: &Sealed) -> Option<Rc<crypto::Key>> {
        let part = sealed.header().part()?;
        let lock = sealed.lock();
        let same = part.copy() == self.copy && lock.iterations() == self.iterations && lock.salt() == self.salt;
        same.then(|| Rc::clone(&self.key))
    }

    /// Легла часть `part` файла `sealed`, открытая ключом `key`: им откроется
    /// следующая — или ничего, если легли все.
    fn after(laid: &Laid, sealed: &Sealed, part: Part, key: Rc<crypto::Key>) -> Option<Self> {
        (!laid.whole()).then(|| Self {
            copy: part.copy(),
            iterations: sealed.lock().iterations(),
            salt: sealed.lock().salt().to_vec(),
            key,
        })
    }
}

/// Слова о частях копии (задача 123): какие ещё не легли — номером со словом
/// «часть» и меткой файла, — или что легли все.
fn parts_words(lang: Lang, laid: &Laid) -> String {
    let of = laid.of();
    let integer = |number: u16| i18n::integer(lang, i64::from(number));
    let mark = |number: u16| fill(t(lang, Key::BackupPartMark), &[&backup::part_mark(number, of)]);
    match laid.missing().as_slice() {
        [] => fill(t(lang, Key::BackupPartsAll), &[&integer(of), &integer(of)]),
        &[number] => fill(t(lang, Key::BackupPartsLeftOne), &[&integer(number), &mark(number)]),
        missing => {
            let marks: Vec<String> = missing.iter().map(|&number| mark(number)).collect();
            let count = u16::try_from(missing.len()).unwrap_or(of);
            fill(t(lang, Key::BackupPartsLeft), &[&integer(count), &integer(of), &marks.join(", ")])
        }
    }
}

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
    router: Router,
) -> impl IntoView {
    let restoring = RwSignal::new_local(Restoring::Idle);
    // Ключ прошлой части копии (задача 117) — пока открыт экран копии: ушли
    // с него — забыть.
    let known = StoredValue::new_local(None::<Known>);
    Effect::new(move |_| {
        if !router.shows(Screen::Backup) {
            known.set_value(None);
        }
    });
    // Легшие части — запасом к записи устройства (задача 123): `localStorage`
    // может отказать, и тогда слова — по памяти экрана, до перезагрузки.
    // Ключа в ней нет, и с уходом с экрана она не забывается.
    let laid_here = StoredValue::new_local(None::<Laid>);
    // Копия всех ИП одним файлом легла в пустую программу частью (ревью
    // задачи 41, задача 116): спросить, где выпускать, — и сразу, под словами
    // о сбое, и после повтора: к повтору список ИП уже записан, и программа
    // для него не пустая. У копии частями то же помнит запись устройства
    // (`Laid::failed`): её вопрос — когда лягут все части, и после
    // перезагрузки тоже.
    let fresh_partly = RwSignal::new(false);
    let failed_part = StoredValue::new(false);
    // Что помнилось о частях копий: запись устройства и память экрана.
    let remembered = move || -> Vec<Laid> {
        [Device::new().parts_laid(), laid_here.get_value()].into_iter().flatten().collect()
    };
    let remember = move |laid: &Laid| {
        Device::new().set_parts_laid(laid);
        laid_here.set_value(Some(laid.clone()));
    };
    // Переход к ответу: частичная копия запоминает, что программа была
    // пуста, а легшая после неё — спрашивает и тогда.
    let landed = move |outcome: Recovered, sealed: &Sealed| -> Option<Restoring> {
        Some(match outcome {
            Recovered::Done(restored, after, key) => {
                // Пустой программа была до этой копии — или до её прошлой
                // попытки, легшей частью: тогда прежних частей здесь нет.
                // Взятое один раз: следующая часть ложится уже не в пустую.
                let empty = after.fresh || fresh_partly.get_untracked();
                if fresh_partly.get_untracked() {
                    fresh_partly.set(false);
                }
                let mut after = After { fresh: empty, ..after };
                let laid = after.part.map(|part| {
                    let before = remembered();
                    let laid = Laid::after(before.iter().cloned(), part, empty);
                    remember(&laid);
                    known.set_value(key.and_then(|key| Known::after(&laid, sealed, part, key)));
                    // У копии частями — когда эта часть сделала её целой
                    // (задача 123).
                    after.fresh = laid.asks(&before);
                    laid
                });
                Restoring::Done(restored, after, laid)
            }
            Recovered::AsNew => Restoring::AsNew,
            Recovered::Elsewhere(ip) => Restoring::Elsewhere(ip),
            Recovered::Partial(why, fresh) => {
                let part = sealed.header().part();
                match part {
                    Some(part) if fresh => remember(&Laid::failed(part)),
                    Some(_) => {}
                    None if fresh => fresh_partly.set(true),
                    None => {}
                }
                failed_part.set_value(part.is_some());
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

    // Расшифровать и слить: паролем из поля или ключом прошлой части той же
    // копии (задача 117).
    let run = move |job: RestoreJob, sealed: Rc<Sealed>| {
        restoring.set(Restoring::Busy);
        leptos::task::spawn_local(async move {
            let outcome = match restoring_task(backups, job).await {
                Ok(task) => task.await,
                Err(_) => Err(Problem::Say(Key::PartFailed)),
            };
            if outcome.is_ok() {
                password.set(String::new());
            }
            restoring.set(match outcome {
                Ok(Recovered::Other(asked)) => Restoring::Other(asked),
                Ok(outcome) => landed(outcome, &sealed).unwrap_or(Restoring::Failed(Problem::Say(Key::BackupFailed))),
                // Пароль не подошёл — спросить снова, а не выбирать файл
                // заново.
                Err(problem @ Problem::Say(Key::BackupWrongFilePassword)) => {
                    Restoring::Password(sealed, Some(problem))
                }
                Err(problem) => Restoring::Failed(problem),
            });
        });
    };

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
            match opened {
                Ok(sealed) => {
                    let sealed = Rc::new(sealed);
                    // Следующая часть той же копии — ключом прошлой.
                    match known.with_value(|known| known.as_ref().and_then(|known| known.key_for(&sealed))) {
                        Some(key) => run(RestoreJob::Known(Rc::clone(&sealed), key), sealed),
                        None => restoring.set(Restoring::Password(sealed, None)),
                    }
                }
                Err(problem) => restoring.set(Restoring::Failed(problem)),
            }
        });
    };

    let submit = move |event: SubmitEvent| {
        event.prevent_default();
        let Restoring::Password(sealed, _) = restoring.get_untracked() else {
            return;
        };
        let typed = password.get_untracked();
        run(RestoreJob::Open(Rc::clone(&sealed), typed), sealed);
    };

    // Ответ на вопрос о копии другого ИП — «Всё равно слить» или «Добавить
    // как нового ИП»: та же часть wasm, без пароля.
    let answer = move |job: fn(Rc<Asked>) -> RestoreJob| {
        let Restoring::Other(asked) = restoring.get_untracked() else {
            return;
        };
        let sealed = Rc::clone(&asked.sealed);
        restoring.set(Restoring::Busy);
        leptos::task::spawn_local(async move {
            let outcome = match restoring_task(backups, job(asked)).await {
                Ok(task) => task.await,
                Err(_) => Err(Problem::Say(Key::PartFailed)),
            };
            restoring.set(match outcome {
                // С ответом хранилище второй раз не спрашивает.
                Ok(outcome) => landed(outcome, &sealed).unwrap_or(Restoring::Failed(Problem::Say(Key::BackupFailed))),
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
                // Часть копии (задача 117): какая и что остальные — так же.
                let part = header.part().map(|part| move || {
                    let lang = lang.get();
                    fill(
                        t(lang, Key::BackupFilePart),
                        &[&i18n::integer(lang, i64::from(part.number())), &i18n::integer(lang, i64::from(part.of()))],
                    )
                });
                view! {
                    {info.map(|info| view! { <p>{info}</p> })}
                    {part.map(|part| view! { <p class="hint">{part}</p> })}
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
                    Restoring::Done(restored, after, laid) => {
                        // Легла часть, а не вся копия (задача 123): «Готово» над
                        // ней человек принимал за всю копию.
                        let part = after.part.filter(|_| laid.as_ref().is_some_and(|laid| !laid.whole()));
                        // Где выпускать — когда программа была пуста до копии, а у
                        // копии частями — до первой её части, и эта часть сделала
                        // копию целой: документы и номера могли быть в ещё не
                        // легшей (задача 123). Выпуск уже выключен — спрашивать
                        // не о чем.
                        let ask = (after.fresh && (laid.is_some() || restored.added > 0)) && !issued_elsewhere(backups);
                        let whole = laid.as_ref().is_none_or(Laid::whole);
                        Some(view! {
                            <p role="status">
                                {move || {
                                    let lang = lang.get();
                                    let counts = [
                                        i18n::integer(lang, restored.added as i64),
                                        i18n::integer(lang, restored.updated as i64),
                                        i18n::integer(lang, restored.unchanged as i64),
                                    ];
                                    match part {
                                        Some(part) => fill(
                                            t(lang, Key::BackupPartRestored),
                                            &[
                                                &i18n::integer(lang, i64::from(part.number())),
                                                &i18n::integer(lang, i64::from(part.of())),
                                                &counts[0],
                                                &counts[1],
                                                &counts[2],
                                            ],
                                        ),
                                        None => fill(t(lang, Key::BackupRestored), &[&counts[0], &counts[1], &counts[2]]),
                                    }
                                }}
                            </p>
                            // Части копии (задача 117): какие остались — или что легли все.
                            {laid.map(|laid| view! {
                                <p role=whole.then_some("status") class=(!whole).then_some("warning")>
                                    {move || parts_words(lang.get(), &laid)}
                                </p>
                            })}
                            {after.ahead.then(|| view! { <p class="warning">{tr(lang, Key::BackupAhead)}</p> })}
                            {ask.then(|| view! { <IssuerAsk lang /> })}
                        }.into_any())
                    }
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
                        {(matches!(problem, Problem::Partly(_))
                            && fresh_partly.get_untracked()
                            && !failed_part.get_value()
                            && !issued_elsewhere(backups))
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
            opened(backups, &store, sealed, entries, Rc::new(key)).await
        }
        RestoreJob::Known(sealed, key) => {
            let entries = stored::unlock_with(FILE_KINDS, &sealed, &key).await?;
            opened(backups, &store, sealed, entries, key).await
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
                    After {
                        fresh: false,
                        ahead,
                        part: asked.sealed.header().part(),
                    },
                    Some(Rc::clone(&asked.key)),
                )),
                // С ответом «слить» хранилище не спрашивает.
                Merged::Other { .. } => Err(Problem::Say(Key::BackupFailed)),
            }
        }
        RestoreJob::AsNew(asked) => restore_as_new(backups, &store, &asked).await,
    }
}

/// Расшифрованную копию — слить с базой или остановиться на вопросе.
/// Копия одного ИП ложится в открытую базу, копия всех — по ИП.
async fn opened(
    backups: Backups,
    store: &Store,
    sealed: Rc<Sealed>,
    entries: Vec<Entry>,
    key: Rc<crypto::Key>,
) -> Result<Recovered, Problem> {
    let ahead = backup::ahead(&entries, clock::now());
    if backup::is_full(&entries) {
        return restore_full(backups, store, (&sealed, &key), entries, ahead).await;
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
    let fresh = !several && empty(store).await?;
    Ok(match stored::restore(store, KINDS, &entries, OtherIe::Ask).await? {
        Merged::Done(restored) => Recovered::Done(
            restored_from(backups, (&sealed, &key), restored, Landed::single(&restored, &entries, own)),
            After {
                fresh,
                ahead,
                part: sealed.header().part(),
            },
            Some(key),
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

/// Нет ли в базе ни одной записи.
pub(super) async fn empty(store: &Store) -> Result<bool, Failure> {
    Ok(store.read()?.is_empty().await?)
}
