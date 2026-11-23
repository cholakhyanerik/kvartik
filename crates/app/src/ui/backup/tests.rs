//! Автокопия от базы до файла (задача 29): настоящая база программы,
//! настоящий WebCrypto и папка из OPFS — выбор папки человеком тест не
//! нажмёт, а OPFS отдаёт такую же `FileSystemDirectoryHandle`.

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

/// Профиль ИП с английским именем.
fn named(name: &str) -> Profile {
    let mut data = kvartik_core::model::profile::ProfileData::default();
    data.name.en = Line::new(name).expect("годное имя");
    Profile::save(None, data, clock::now()).expect("годный профиль").expect("новый")
}

/// Записать профиль в базу `name`.
async fn seed(name: &str, profile: &Profile) {
    let store = Store::open_base(name, |_| {}).await.expect("база открыта");
    let mut writes = Writes::default();
    writes.insert(profile);
    store.write().expect("транзакция").commit(writes).await.expect("записано");
}

/// Файл копии одной базы `name` под `PASSWORD`.
async fn single_file(name: &str) -> Rc<Sealed> {
    let store = Store::open_base(name, |_| {}).await.expect("база открыта");
    let salt = crypto::random(SALT_BYTES).expect("соль");
    let key = crypto::derive(&crypto::normalize(PASSWORD), &salt, KDF_ITERATIONS).await.expect("ключ");
    let header = Header::new("0.0.87", clock::now());
    let sealing = Sealing {
        header: &header,
        key: &key,
        salt: &salt,
        iterations: KDF_ITERATIONS,
        limit: kvartik_core::format::backup::PART_PLAIN_BYTES,
    };
    let made = stored::make(&store, KINDS, &sealing).await.expect("копия");
    Rc::new(open(&made.sheets[0].text).expect("файл открывается"))
}

fn forget_office() {
    let window = web_sys::window().expect("окно есть");
    if let Ok(Some(storage)) = window.local_storage() {
        for name in ["kvartik.office", "kvartik.base"] {
            let _ = storage.remove_item(name);
        }
    }
    if let Ok(Some(storage)) = window.session_storage() {
        let _ = storage.remove_item("kvartik.base");
    }
}

/// Копия одного ИП при нескольких ИП (задача 38, ревью 2026-11-06).
/// Копию ИП из списка, когда открыт другой, окно никуда не кладёт,
/// а называет его. Своя копия ложится, но день последней копии не
/// сдвигает — в ней не все ИП — и пароль её устройству не отдаёт. Копия
/// чужого ИП новым ИП — тоже без пароля: он клиента, а не устройства.
#[wasm_bindgen_test]
async fn single_backups_with_several_ies() {
    use crate::store::office::OFFICE_DB;
    crate::store::delete_database(crate::store::DB_NAME).await;
    crate::store::delete_database(OFFICE_DB).await;
    forget_device();
    forget_office();
    let (today, now) = (clock::today().expect("сегодня"), clock::now());
    let aram = named("IE Aram");
    let anna = named("IE Anna");
    let first = Ip::first(Some(&aram), today, now);
    let mut second = Ip::new(today, now);
    second.copy_profile(&anna, now);
    let stranger = Ip::new(today, now);
    seed(crate::store::DB_NAME, &aram).await;
    seed(second.base(), &anna).await;
    seed(stranger.base(), &named("IE Vahe")).await;
    let (anna_file, aram_file, vahe_file) = (
        single_file(second.base()).await,
        single_file(crate::store::DB_NAME).await,
        single_file(stranger.base()).await,
    );
    crate::store::delete_database(stranger.base()).await;
    let office_store = Store::open_office(|_| {}).await.expect("контора открыта");
    let mut writes = Writes::default();
    writes.insert(&first);
    writes.insert(&second);
    office_store.write().expect("транзакция").commit(writes).await.expect("записано");
    drop(office_store);
    Device::new().set_office();
    let open = Rc::new(Store::open_base(crate::store::DB_NAME, |_| {}).await.expect("база открыта"));
    let backups = backups_over(Rc::clone(&open));
    backups.attach(Office::new(backups));
    let mut wrong = Vec::new();

    match restore(backups, RestoreJob::Open(anna_file, PASSWORD.to_owned())).await {
        Ok(Recovered::Elsewhere(ip)) if ip.id() == second.id() => {}
        Ok(_) => wrong.push("копию ИП из списка окно не назвало".to_owned()),
        Err(problem) => wrong.push(format!("копия ИП из списка: {}", problem.text(Lang::Ru))),
    }
    let profiles = open.read().expect("чтение").all::<Profile>().await.expect("профили");
    if profiles.len() != 1 {
        wrong.push(format!("в открытой базе профилей: {}", profiles.len()));
    }

    match restore(backups, RestoreJob::Open(aram_file, PASSWORD.to_owned())).await {
        Ok(Recovered::Done(..)) => {}
        Ok(_) => wrong.push("своя копия не легла".to_owned()),
        Err(problem) => wrong.push(format!("своя копия: {}", problem.text(Lang::Ru))),
    }
    if Device::new().last_backup().is_some() || Device::new().backup_key().is_some() {
        wrong.push("копия одного ИП сдвинула день копии или отдала пароль".to_owned());
    }

    match restore(backups, RestoreJob::Open(Rc::clone(&vahe_file), PASSWORD.to_owned())).await {
        Ok(Recovered::Other(asked)) => match restore(backups, RestoreJob::AsNew(asked)).await {
            Ok(Recovered::AsNew) => {}
            Ok(_) => wrong.push("чужой ИП не лёг новым".to_owned()),
            Err(problem) => wrong.push(format!("новым ИП: {}", problem.text(Lang::Ru))),
        },
        Ok(_) => wrong.push("о копии чужого ИП не спросили".to_owned()),
        Err(problem) => wrong.push(format!("копия чужого ИП: {}", problem.text(Lang::Ru))),
    }
    if Device::new().backup_key().is_some() {
        wrong.push("копия чужого ИП отдала пароль устройству".to_owned());
    }
    // Та же копия ещё раз — ИП уже в списке, под своим именем: окно его
    // называет, а не заводит второй раз (глазной прогон, 2026-11-06).
    match restore(backups, RestoreJob::Open(vahe_file, PASSWORD.to_owned())).await {
        Ok(Recovered::Elsewhere(ip)) if ip.name(Lang::En) == "IE Vahe" => {}
        Ok(Recovered::Elsewhere(ip)) => wrong.push(format!("новый ИП в списке без имени: {:?}", ip.name(Lang::En))),
        Ok(_) => wrong.push("копию ИП, добавленного новым, окно не узнало".to_owned()),
        Err(problem) => wrong.push(format!("та же копия ещё раз: {}", problem.text(Lang::Ru))),
    }
    let office_store = Store::open_office(|_| {}).await.expect("контора открыта");
    let ips = office_list(&office_store).await.expect("список").records;
    if ips.len() != 3 {
        wrong.push(format!("в списке {} ИП вместо трёх", ips.len()));
    }
    drop((open, office_store));
    for ip in &ips {
        crate::store::delete_database(ip.base()).await;
    }
    crate::store::delete_database(OFFICE_DB).await;
    forget_device();
    forget_office();
    assert!(wrong.is_empty(), "копии одного ИП при нескольких: {wrong:#?}");
}

/// Копия всех ИП на свежем устройстве (задача 38): легла — и открытую
/// базу помнят вкладка и устройство. Иначе следующая перезагрузка — без
/// сети, новой версией — выбрала бы ИП по дню добавления, а у ИП копии он
/// бывает один на всех (найдено проверкой собранного сайта, 2026-11-06).
#[wasm_bindgen_test]
async fn a_full_backup_on_a_fresh_device_keeps_the_open_ie() {
    use crate::store::DB_NAME;
    use crate::store::office::{OFFICE_DB, tab_base};
    crate::store::delete_database(DB_NAME).await;
    crate::store::delete_database(OFFICE_DB).await;
    forget_device();
    forget_office();
    let (today, now) = (clock::today().expect("сегодня"), clock::now());
    let aram = named("IE Aram");
    let anna = named("IE Anna");
    let first = Ip::first(Some(&aram), today, now);
    let mut second = Ip::new(today, now);
    second.copy_profile(&anna, now);
    seed(DB_NAME, &aram).await;
    seed(second.base(), &anna).await;
    let office_store = Store::open_office(|_| {}).await.expect("контора открыта");
    let mut writes = Writes::default();
    writes.insert(&first);
    writes.insert(&second);
    office_store.write().expect("транзакция").commit(writes).await.expect("записано");
    let file = {
        let base = Store::open_base(DB_NAME, |_| {}).await.expect("база открыта");
        let salt = crypto::random(SALT_BYTES).expect("соль");
        let key = crypto::derive(&crypto::normalize(PASSWORD), &salt, KDF_ITERATIONS).await.expect("ключ");
        let header = Header::new("0.0.87", clock::now());
        let sealing = Sealing {
            header: &header,
            key: &key,
            salt: &salt,
            iterations: KDF_ITERATIONS,
            limit: kvartik_core::format::backup::PART_PLAIN_BYTES,
        };
        let made = stored::make_all(&base, DB_NAME, &office_store, &sealing).await.expect("копия всех ИП");
        Rc::new(open(&made.sheets[0].text).expect("файл открывается"))
    };
    drop(office_store);
    for name in [DB_NAME, second.base(), OFFICE_DB] {
        crate::store::delete_database(name).await;
    }
    forget_office();
    let open = Rc::new(Store::open_base(DB_NAME, |_| {}).await.expect("база открыта"));
    let backups = backups_over(Rc::clone(&open));
    backups.attach(Office::new(backups));
    let mut wrong = Vec::new();
    match restore(backups, RestoreJob::Open(file, PASSWORD.to_owned())).await {
        Ok(Recovered::Done(..)) => {}
        Ok(_) => wrong.push("копия всех ИП легла не вся".to_owned()),
        Err(problem) => wrong.push(format!("копия всех ИП: {}", problem.text(Lang::Ru))),
    }
    let remembered = (tab_base(), Device::new().base());
    if remembered != (Some(DB_NAME.to_owned()), Some(DB_NAME.to_owned())) {
        wrong.push(format!("открытую базу не помнят: {remembered:?}"));
    }
    if !Device::new().office() {
        wrong.push("устройство не помнит, что ИП несколько".to_owned());
    }
    let office_store = Store::open_office(|_| {}).await.expect("контора открыта");
    let ips = office_list(&office_store).await.expect("список").records;
    if ips.len() != 2 {
        wrong.push(format!("в списке {} ИП вместо двух", ips.len()));
    }
    drop((open, office_store));
    for ip in &ips {
        crate::store::delete_database(ip.base()).await;
    }
    crate::store::delete_database(OFFICE_DB).await;
    forget_device();
    forget_office();
    assert!(wrong.is_empty(), "копия всех ИП на свежем устройстве: {wrong:#?}");
}

/// Завести базу `name` версии новее программы: открыть её программа не
/// сможет (`VersionError`), и копия всех ИП ляжет частью.
async fn newer_base(name: &str) {
    use wasm_bindgen::JsCast;
    use wasm_bindgen::closure::Closure;
    let factory = web_sys::window()
        .and_then(|window| window.indexed_db().ok().flatten())
        .expect("IndexedDB есть");
    let request = factory
        .open_with_u32(name, crate::store::DB_VERSION + 1)
        .expect("база открывается");
    let opened = js_sys::Promise::new(&mut |resolve, _| {
        let request = request.clone();
        let done = request.clone();
        let on_success = Closure::once_into_js(move || {
            if let Ok(db) = done.result() {
                db.unchecked_into::<web_sys::IdbDatabase>().close();
            }
            let _ = resolve.call0(&wasm_bindgen::JsValue::NULL);
        });
        request.set_onsuccess(Some(on_success.unchecked_ref()));
    });
    let _ = opened.await;
}

/// Копия всех ИП легла в пустую программу частью (задача 116): база
/// второго ИП не открылась. Восстановление говорит, что программа была
/// пуста, — где выпускать, окно спросит и тут, — а повтор, легший весь,
/// пустой её уже не видит: список ИП записан первой попыткой. Поэтому
/// вопрос после повтора помнит экран (`fresh_partly`).
#[wasm_bindgen_test]
async fn a_full_backup_landing_in_part_still_knows_the_app_was_empty() {
    use crate::store::DB_NAME;
    use crate::store::office::OFFICE_DB;
    crate::store::delete_database(DB_NAME).await;
    crate::store::delete_database(OFFICE_DB).await;
    forget_device();
    forget_office();
    let (today, now) = (clock::today().expect("сегодня"), clock::now());
    let aram = named("IE Aram");
    let anna = named("IE Anna");
    let first = Ip::first(Some(&aram), today, now);
    let mut second = Ip::new(today, now);
    second.copy_profile(&anna, now);
    seed(DB_NAME, &aram).await;
    seed(second.base(), &anna).await;
    let office_store = Store::open_office(|_| {}).await.expect("контора открыта");
    let mut writes = Writes::default();
    writes.insert(&first);
    writes.insert(&second);
    office_store.write().expect("транзакция").commit(writes).await.expect("записано");
    let file = {
        let base = Store::open_base(DB_NAME, |_| {}).await.expect("база открыта");
        let salt = crypto::random(SALT_BYTES).expect("соль");
        let key = crypto::derive(&crypto::normalize(PASSWORD), &salt, KDF_ITERATIONS).await.expect("ключ");
        let header = Header::new("0.0.166", clock::now());
        let sealing = Sealing {
            header: &header,
            key: &key,
            salt: &salt,
            iterations: KDF_ITERATIONS,
            limit: kvartik_core::format::backup::PART_PLAIN_BYTES,
        };
        let made = stored::make_all(&base, DB_NAME, &office_store, &sealing).await.expect("копия всех ИП");
        Rc::new(open(&made.sheets[0].text).expect("файл открывается"))
    };
    drop(office_store);
    for name in [DB_NAME, second.base(), OFFICE_DB] {
        crate::store::delete_database(name).await;
    }
    forget_office();
    newer_base(second.base()).await;
    let open = Rc::new(Store::open_base(DB_NAME, |_| {}).await.expect("база открыта"));
    let backups = backups_over(Rc::clone(&open));
    backups.attach(Office::new(backups));
    let mut wrong = Vec::new();
    match restore(backups, RestoreJob::Open(Rc::clone(&file), PASSWORD.to_owned())).await {
        Ok(Recovered::Partial(_, true)) => {}
        Ok(Recovered::Partial(_, false)) => wrong.push("легла частью — а что программа была пуста, не сказано".to_owned()),
        Ok(_) => wrong.push("копия всех ИП легла вся, хотя база второго ИП не открывается".to_owned()),
        Err(problem) => wrong.push(format!("копия всех ИП: {}", problem.text(Lang::Ru))),
    }
    crate::store::delete_database(second.base()).await;
    match restore(backups, RestoreJob::Open(file, PASSWORD.to_owned())).await {
        Ok(Recovered::Done(_, after, _)) if !after.fresh => {}
        Ok(Recovered::Done(..)) => wrong.push("повтор видит программу пустой — тогда экрану помнить незачем".to_owned()),
        Ok(_) => wrong.push("повтор лёг не весь".to_owned()),
        Err(problem) => wrong.push(format!("повтор: {}", problem.text(Lang::Ru))),
    }
    let office_store = Store::open_office(|_| {}).await.expect("контора открыта");
    let ips = office_list(&office_store).await.expect("список").records;
    drop((open, office_store));
    for ip in &ips {
        crate::store::delete_database(ip.base()).await;
    }
    crate::store::delete_database(second.base()).await;
    crate::store::delete_database(OFFICE_DB).await;
    forget_device();
    forget_office();
    assert!(wrong.is_empty(), "копия всех ИП частью: {wrong:#?}");
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

/// Автокопия больше файла (задача 117) кладёт части дня рядом —
/// `…-1of3.json` и дальше, — а прежнюю копию дня одним файлом убирает:
/// новая её сменила. Прошлые дни остаются. Части ложатся по одной в любом
/// порядке: первая — паролем, следующие той же копии — ключом прошлой, без
/// пароля, и окно знает, какая это часть; после всех в пустой базе — все
/// записи.
#[wasm_bindgen_test]
async fn auto_backup_in_parts_replaces_the_copy_of_the_day() {
    if !folders::available() {
        web_sys::console::log_1(&"автокопии в этом браузере нет — тест пропущен".into());
        return;
    }
    crate::store::delete_database(crate::store::DB_NAME).await;
    let _ = remembered::forget().await;
    forget_device();
    let device = Device::new();
    let tag = DeviceTag::parse(TAG).expect("метка");
    device.set_auto_tag(tag);
    let store = Rc::new(Store::open(|_| {}).await.expect("база открыта"));
    let mut writes = Writes::default();
    let mut clients = Vec::new();
    for name in ["Acme ֏", "Beispiel GmbH", "Example LLC"] {
        let client = Client::create(
            ClientData {
                name: Line::new(name).expect("имя"),
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
        writes.insert(&client);
        clients.push(client);
    }
    store.write().expect("транзакция").commit(writes).await.expect("клиенты записаны");
    let (chosen, scratch_name) = scratch().await;
    let backups = backups_over(Rc::clone(&store));
    device.set_backup_key(&device_password(PASSWORD).await);
    let folder = match run(backups, enabling(&chosen, PASSWORD, None)).await {
        Ok(Made::Status(Some(place))) => place.folder,
        _ => panic!("автокопия не включилась"),
    };
    let today = clock::today().expect("часы с датой");
    let day = autobackup::file_name(today, tag);
    let past = format!("kvartik-auto-2020-01-01-{TAG}.json");
    for name in [day.as_str(), past.as_str()] {
        folders::write(&folder, name, b"{}").await.expect("файл проб записан");
    }
    let mut wrong = Vec::new();
    // Каждая запись — своей частью: три клиента — три файла.
    PART_LIMIT.with(|limit| limit.set(1));
    match run(backups, Job::Auto).await {
        Ok(Made::Ran(Ok(_))) => {}
        Ok(Made::Ran(Err(hitch))) => wrong.push(format!("автокопия: {hitch:?}")),
        Ok(_) => wrong.push("автокопия не записалась".to_owned()),
        Err(problem) => wrong.push(format!("автокопия: {problem:?}")),
    }
    PART_LIMIT.with(|limit| limit.set(backup::PART_PLAIN_BYTES));
    let stem = day.strip_suffix(".json").expect("имя файла дня");
    let parts: Vec<String> = (1..=3).map(|n| format!("{stem}-{n}of3.json")).collect();
    let mut names = folders::names(&folder, "").await.expect("папка читается");
    names.sort_unstable();
    let mut expected: Vec<String> = parts.iter().cloned().chain([past.clone()]).collect();
    expected.sort_unstable();
    if names != expected {
        wrong.push(format!("в папке {names:?}, а не {expected:?}"));
    }
    if !matches!(device.last_size(), Some((_, _, 3))) {
        wrong.push(format!("размер последней копии: {:?}", device.last_size()));
    }

    // Части — в пустую программу, с середины.
    drop(store);
    crate::store::delete_database(crate::store::DB_NAME).await;
    let empty = Rc::new(Store::open(|_| {}).await.expect("база открыта"));
    let backups = backups_over(Rc::clone(&empty));
    let mut key = None;
    for number in [2_u16, 1, 3] {
        let Some(text) = read_file(&folder, &parts[usize::from(number) - 1]).await else {
            wrong.push(format!("части {number} нет"));
            continue;
        };
        let sealed = Rc::new(open(&text).expect("часть открывается"));
        let job = match key.take() {
            None => RestoreJob::Open(Rc::clone(&sealed), PASSWORD.to_owned()),
            Some(known) => RestoreJob::Known(Rc::clone(&sealed), known),
        };
        match restore(backups, job).await {
            Ok(Recovered::Done(_, after, opened)) => {
                if after.part.map(|part| (part.number(), part.of())) != Some((number, 3)) {
                    wrong.push(format!("часть {number}: окно знает {:?}", after.part));
                }
                key = opened;
            }
            Ok(_) => wrong.push(format!("часть {number} не легла")),
            Err(problem) => wrong.push(format!("часть {number}: {}", problem.text(Lang::Ru))),
        }
    }
    let landed = empty.read().expect("чтение").list::<Client>().await.expect("клиенты").records;
    let mut ids: Vec<_> = landed.iter().map(Client::id).collect();
    let mut all: Vec<_> = clients.iter().map(Client::id).collect();
    ids.sort_unstable();
    all.sort_unstable();
    if ids != all {
        wrong.push(format!("легло клиентов {} из {}", ids.len(), all.len()));
    }

    drop(empty);
    let _ = remembered::forget().await;
    drop_scratch(&scratch_name).await;
    forget_device();
    crate::store::delete_database(crate::store::DB_NAME).await;
    assert!(wrong.is_empty(), "автокопия частями: {wrong:#?}");
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
