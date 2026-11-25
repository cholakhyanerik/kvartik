//! Слияние копии с тем, что уже есть (задача 119 — файлом из `backup.rs`):
//! копия другого ИП — только по ответу человека и отдельным ИП, копия всех
//! ИП — списком ИП и базами по одной, что легло частью, и вопрос, где
//! выпускать дальше; самое слияние записей — ядро (`format::backup`).

use super::*;

/// Где выпускать счета и акты дальше — после копии, легшей в пустую
/// программу (задача 41): так заводят второе устройство, а номера документов
/// выдаёт одно — `Document::next_number` знает только свою базу. Ответ —
/// запись устройства, и его меняют в настройках; без ответа выпуск здесь
/// остаётся, как был. «Выпускать здесь» — главным: так и после переезда на
/// новое устройство, и после стёртых данных браузера, а у кого их два,
/// слова после ответа просят выключить выпуск на прежнем.
#[component]
pub(in crate::ui) fn IssuerAsk(lang: ReadSignal<Lang>) -> impl IntoView {
    let answered = RwSignal::new(None::<bool>);
    // Ответ — у всех ИП устройства: копия всех ИП легла целиком сюда.
    // Запись не легла — вопрос остаётся: ответа устройство не помнит.
    let answer = move |elsewhere: bool| {
        if crate::ui::settings::set_issue_elsewhere(None, elsewhere) {
            answered.set(Some(elsewhere));
        }
    };
    view! {
        <div class="notice">
            {move || match answered.get() {
                None => view! {
                    <p>{tr(lang, Key::BackupIssuerAsk)}</p>
                    <div class="actions">
                        <button type="button" on:click=move |_| answer(false)>
                            <span class="label">{tr(lang, Key::BackupIssuerHere)}</span>
                        </button>
                        <button type="button" class="secondary" on:click=move |_| answer(true)>
                            <span class="label">{tr(lang, Key::BackupIssuerThere)}</span>
                        </button>
                    </div>
                }
                .into_any(),
                Some(elsewhere) => view! {
                    <p role="status">
                        {tr(lang, if elsewhere { Key::BackupIssuerThereDone } else { Key::BackupIssuerHereDone })}
                    </p>
                    <a class="button secondary" href=Screen::Settings.href()>
                        <span class="label">{tr(lang, Key::SettingsTitle)}</span>
                    </a>
                }
                .into_any(),
            }}
        </div>
    }
}

/// Выключен ли здесь выпуск у открытого ИП (задачи 41 и 116): тогда
/// спрашивать, где выпускать, не о чем.
pub(super) fn issued_elsewhere(backups: Backups) -> bool {
    backups
        .base
        .get()
        .is_some_and(|store| crate::ui::settings::issue_elsewhere_untracked(store.name()))
}

/// ИП словами: имя и ՀՎՀՀ через запятую — или что их нет.
pub(in crate::ui) fn who(lang: Lang, owner: &Owner) -> String {
    let name = match owner.name(lang) {
        "" => t(lang, Key::BackupOtherIeNameless),
        name => name,
    };
    let tin = match owner.tin() {
        Some(tin) => fill(t(lang, Key::TinShort), &[tin.as_str()]),
        None => t(lang, Key::BackupOtherIeNoTin).to_owned(),
    };
    format!("{name}, {tin}")
}

/// Открытый ИП и список конторы, который заводит восстановление (задача 38):
/// списка ещё нет — открытый ИП ложится в него записью `Ip::first`, чтобы
/// копия того же ИП слилась с его базой, а пустая база открытого ИП
/// свободна: её займёт первый ИП копии (`format::office::route`). Список
/// уже есть — ни того ни другого.
pub(in crate::ui) async fn unlisted(office_store: &Store, open: &Store, open_name: &str) -> Result<(Option<Ip>, Option<String>), Failure> {
    if !office_list(office_store).await?.records.is_empty() {
        return Ok((None, None));
    }
    let tx = open.read()?;
    if tx.is_empty().await? {
        return Ok((None, Some(open_name.to_owned())));
    }
    let profile = match tx.all::<Profile>().await {
        Ok(profiles) => Profile::current(profiles),
        // Нечитаемый профиль — ИП без имени в списке; его данные целы.
        Err(StoreError::Damaged) => None,
        Err(error) => return Err(error.into()),
    };
    let today = clock::today().ok_or(Failure::Store(StoreError::Failed))?;
    Ok((Some(Ip::first(profile.as_ref(), today, clock::now())), None))
}

/// Копия всех ИП (задача 38): по ИП, маршрутом ядра, в базы этого
/// устройства; списка конторы не было — он заводится.
pub(super) async fn restore_full(
    backups: Backups,
    store: &Store,
    (sealed, key): (&Sealed, &Rc<crypto::Key>),
    entries: Vec<Entry>,
    ahead: bool,
    typed: Option<&str>,
) -> Result<Recovered, Problem> {
    let office = backups.office().ok_or(Problem::Say(Key::BackupFailed))?;
    // Конторы не было — устройство чьё-то своё, и пароль его копии — его.
    let had_office = Device::new().office();
    let office_store = office.store().await.map_err(Failure::from)?;
    let open_name = office.open_name();
    let (first, spare) = unlisted(&office_store, store, &open_name).await?;
    let done = stored::restore_all(store, &open_name, &office_store, entries, first, spare.as_deref()).await?;
    for (base, oldest) in &done.landed {
        arrived(base, Some(*oldest));
    }
    // Список уже записан — и тогда, когда база не легла: перечитать его
    // и тронутое (ревью 2026-11-06).
    office.several_now();
    office.refresh(&office_store).await;
    let landed = Landed {
        touched: done.open_touched,
        data: done.open_touched,
        same: if done.covers_all { SameIe::Yes } else { SameIe::No },
        adopt: !had_office,
    };
    let restored = restored_from(backups, (sealed, key), done.restored, landed, typed).await;
    // Программа была пуста — ни списка ИП, ни записей открытого: так копию
    // всех ИП кладут на новое устройство.
    let fresh = spare.is_some();
    Ok(match done.failed {
        None => Recovered::Done(
            restored,
            After {
                fresh,
                ahead,
                part: sealed.header().part(),
            },
            Some(Rc::clone(key)),
        ),
        Some(failure) => Recovered::Partial(
            match Problem::from(failure) {
                Problem::Say(why) => why,
                _ => Key::BackupFailed,
            },
            // Пуста была — и тогда, когда сбой на первой же базе ничего
            // в базы не положил: список уже записан, и повтор пустой
            // программы не увидит.
            fresh,
        ),
    })
}

/// Копия другого ИП — новым ИП (задача 38): своя база и запись в списке.
/// Открытая база не меняется, и копией её данных этот файл не считается.
pub(super) async fn restore_as_new(backups: Backups, store: &Store, asked: &Asked) -> Result<Recovered, Problem> {
    let office = backups.office().ok_or(Problem::Say(Key::BackupFailed))?;
    let office_store = office.store().await.map_err(Failure::from)?;
    let (first, _) = unlisted(&office_store, store, &office.open_name()).await?;
    let today = clock::today().ok_or(Problem::Say(Key::BackupFailed))?;
    let (new, restored) = stored::restore_new(&office_store, &asked.entries, first, today, clock::now()).await?;
    arrived(new.base(), restored.oldest);
    office.several_now();
    office.refresh(&office_store).await;
    let landed = Landed {
        touched: false,
        data: false,
        same: SameIe::No,
        adopt: false,
    };
    restored_from(backups, (&asked.sealed, &asked.key), restored, landed, None).await;
    let _ = new;
    Ok(Recovered::AsNew)
}

/// Записи копии легли: разделы перечитывают базу, а устройство запоминает
/// пароль копий и день последней копии. День сдвигает только копия, в которой
/// данные открытой базы (`backup::last_after_restore`: они лежат в этом
/// файле); в копии другого ИП их нет, и напоминание о копии не пропадает.
/// `typed` — пароль, которым открыли копию (уже приведённый,
/// `crypto::normalize`); у части, открытой ключом прошлой, его нет.
pub(super) async fn restored_from(
    backups: Backups,
    (sealed, key): (&Sealed, &crypto::Key),
    restored: Restored,
    landed: Landed,
    typed: Option<&str>,
) -> Restored {
    if landed.touched {
        backups.base.replaced();
        let base = backups
            .office()
            .map_or_else(|| crate::store::DB_NAME.to_owned(), |office| office.open_name());
        arrived(&base, restored.oldest);
    }
    let device = Device::new();
    let lock = sealed.lock();
    let opened_with = DeviceKey::new(lock.iterations(), lock.salt().to_vec(), key.check().to_vec());
    // Своего пароля у устройства нет — берётся пароль этой копии, если она
    // своя: человек только что показал, что знает его, и следующая копия
    // будет под ним же. Но со своей солью, а не солью файла (ревью задачи
    // 124): файл бывает автокопией другого компьютера, и его соль — соль
    // ключа автокопий, который тот держит на диске открыто; ручные копии
    // этого устройства под ней открыл бы ключ из профиля того компьютера,
    // а обещано обратное (`.claude/rules/security.md`, «Файл копии»). Ключ —
    // заново из введённого пароля; пароля нет (часть открыта ключом прошлой) —
    // устройство его не берёт, и следующая копия спросит новый.
    match (device.backup_key(), opened_with) {
        (None, Some(_)) if landed.adopt => {
            if let Some(typed) = typed
                && let Some(own) = own_key(typed).await
                && device.set_backup_key(&own)
            {
                backups.key_set.set(true);
                backups.password_entered();
            }
        }
        // Копия этого же пароля — пароль, значит, помнится.
        (Some(current), Some(opened_with)) if current == opened_with => backups.password_entered(),
        _ => {}
    }
    if landed.same == SameIe::Yes
        && let Some(at) = backup::last_after_restore(
            device.last_backup(),
            sealed.header().created(),
            clock::now(),
        )
    {
        device.set_last_backup(at);
        // Последней стала восстановленная копия, а её файлов окно не считало:
        // размер прежней о ней говорил бы неправду (задача 64).
        device.forget_last_size();
        backups.last.set(clock::local_date(at));
    }
    if landed.data {
        backups.has_data.set(true);
    }
    restored
}

/// Пароль копий устройства из пароля `typed` — со своей случайной солью
/// и полным числом итераций; `None` — шифр отказал.
async fn own_key(typed: &str) -> Option<DeviceKey> {
    let salt = crypto::random(backup::SALT_BYTES).ok()?;
    let key = crypto::derive(typed, &salt, backup::KDF_ITERATIONS).await.ok()?;
    DeviceKey::new(backup::KDF_ITERATIONS, salt, key.check().to_vec())
}

/// В базу `base` легли записи, самая ранняя — с моментом `oldest`: код
/// передачи с этого устройства возьмёт их и тогда, когда они старше его
/// прошлого кода (`Device::sync_arrived`, задача 110), — и код этого ИП,
/// и код всех ИП (задача 116): он помнит своё под именем базы конторы.
pub(in crate::ui) fn arrived(base: &str, oldest: Option<Timestamp>) {
    if let Some(at) = oldest {
        let device = Device::new();
        device.note_sync_arrived(base, at);
        device.note_sync_arrived(crate::store::office::OFFICE_DB, at);
    }
}
