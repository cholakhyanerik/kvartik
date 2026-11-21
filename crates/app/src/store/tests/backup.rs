//! Браузерные тесты копии (задача 7): настоящий WebCrypto и настоящая
//! IndexedDB. Что внутри файла и как сливаются записи, проверяет ядро
//! (`kvartik_core::format::backup`); здесь — шифр браузера и путь от базы
//! к файлу и обратно.

use kvartik_core::format::backup::{
    DeviceKey, Entry, FILE_KINDS, Files, Header, KDF_ITERATIONS, KINDS, Kind, SALT_BYTES, is_full, open,
};
use kvartik_core::model::office::Ip;
use kvartik_core::i18n::Lang;
use kvartik_core::model::profile::{Profile, ProfileData};
use kvartik_core::model::text::Line;
use kvartik_core::model::tin::Tin;
use kvartik_core::release::Version;
use wasm_bindgen::JsCast;

use super::*;
use crate::platform::crypto::{self, CryptoError, Key};
use crate::platform::file::{self as files, Download, FileError};
use crate::store::backup::{
    Failure, Made, Merged, OtherIe, Restored, Sealing, make, restore, restore_all as restore_by_ip, unlock,
};
use crate::store::office::{OFFICE_DB, list as office_list};
use crate::store::device::Device;

/// Виды записей проб — вместо `KINDS` программы, в которой их пока нет.
const KINDS_UNDER_TEST: &[Kind] = &[Kind::of::<Note>(), Kind::of::<Tag>()];

/// Пароль с армянскими и русскими буквами: ключ получается из байтов
/// UTF-8, и латиницы для проверки мало.
const PASSWORD: &str = "Երևան կопия 2026 года";

/// Ключ из пароля проб.
async fn key_for(password: &str, salt: &[u8]) -> Key {
    crypto::derive(&crypto::normalize(password), salt, KDF_ITERATIONS)
        .await
        .expect("ключ получен")
}

fn header() -> Header {
    Header::new("0.0.8", at(1_760_270_400_000))
}

/// Записать записи одним действием.
async fn insert_all(store: &Store, notes: &[Note], tags: &[Tag]) {
    let mut writes = Writes::default();
    for note in notes {
        writes.insert(note);
    }
    for tag in tags {
        writes.insert(tag);
    }
    store
        .write()
        .expect("транзакция записи")
        .commit(writes)
        .await
        .expect("записи сохранены");
}

/// Копия базы видов `kinds` ключом `key` из соли `salt`.
async fn make_with(store: &Store, kinds: &[Kind], key: &Key, salt: &[u8]) -> Result<Made, Failure> {
    let header = header();
    let sealing = Sealing {
        header: &header,
        key,
        salt,
        iterations: KDF_ITERATIONS,
    };
    make(store, kinds, &sealing).await
}

/// Копия всей базы под `PASSWORD` — текстом файла.
async fn backup_of(store: &Store, salt: &[u8]) -> String {
    let key = key_for(PASSWORD, salt).await;
    make_with(store, KINDS_UNDER_TEST, &key, salt)
        .await
        .expect("копия сделана")
        .file
}

/// Восстановить, как окно до ответа человека: в пробах без профилей
/// и в образцах в пустую базу сверка ИП вопроса не задаёт.
async fn restore_all(store: &Store, kinds: &[Kind], entries: &[Entry]) -> Result<Restored, Failure> {
    match restore(store, kinds, entries, OtherIe::Ask).await? {
        Merged::Done(restored) => Ok(restored),
        other => panic!("вопрос о другом ИП там, где сравнивать не с чем: {other:?}"),
    }
}

/// Записи копии базы, где только этот профиль, — как их получит окно из
/// файла: копия, шифр и расшифровка настоящие.
async fn entries_with(profile: &Profile, salt: &[u8]) -> Vec<Entry> {
    let probe = Probe::new();
    let store = probe.open(DB_VERSION).await.expect("база открыта");
    let mut writes = Writes::default();
    writes.insert(profile);
    store
        .write()
        .expect("транзакция записи")
        .commit(writes)
        .await
        .expect("профиль сохранён");
    let key = key_for(PASSWORD, salt).await;
    let made = make_with(&store, KINDS, &key, salt)
        .await
        .expect("копия сделана");
    drop(store);
    probe.delete().await;
    let sealed = open(&made.file).expect("файл открывается");
    unlock(KINDS, &sealed, &crypto::normalize(PASSWORD))
        .await
        .expect("копия расшифрована")
        .0
}

/// Ключ из одного пароля и одной соли — один и тот же, а из другого
/// пароля или соли — другой. Пароль, набранный составными буквами,
/// после приведения к NFC — тот же пароль.
#[wasm_bindgen_test]
async fn key_depends_on_password_and_salt() {
    let salt = crypto::random(SALT_BYTES).expect("соль");
    let other_salt = crypto::random(SALT_BYTES).expect("соль");
    assert_ne!(salt, other_salt, "случайные байты повторились");
    let first = key_for(PASSWORD, &salt).await;
    assert_eq!(first.check(), key_for(PASSWORD, &salt).await.check());
    assert_ne!(
        first.check(),
        key_for("другой пароль копии", &salt).await.check()
    );
    assert_ne!(first.check(), key_for(PASSWORD, &other_salt).await.check());
    // «й» одной точкой кода и «и» с кратким.
    let composed = key_for("копия мой пароль", &salt).await;
    let decomposed = key_for("копия мои\u{306} пароль", &salt).await;
    assert_eq!(composed.check(), decomposed.check(), "NFC не сработал");
}

/// Зашифрованное расшифровывается тем же ключом; чужим ключом или после
/// правки шифротекста — «пароль не подошёл», а не мусор.
#[wasm_bindgen_test]
async fn encryption_round_trips_and_detects_tampering() {
    let salt = crypto::random(SALT_BYTES).expect("соль");
    let iv = crypto::random(12).expect("вектор");
    let key = key_for(PASSWORD, &salt).await;
    let plain = "Ереван ֏ «копия»".as_bytes();
    let data = crypto::encrypt(&key, &iv, plain)
        .await
        .expect("зашифровано");
    assert_ne!(&data[..plain.len()], plain);
    assert_eq!(
        data.len(),
        plain.len() + 16,
        "метка подлинности — 16 байтов"
    );
    assert_eq!(
        crypto::decrypt(&key, &iv, &data).await.as_deref(),
        Ok(plain)
    );

    let wrong = key_for("не тот пароль копии", &salt).await;
    assert_eq!(
        crypto::decrypt(&wrong, &iv, &data).await,
        Err(CryptoError::WrongPassword)
    );
    let mut tampered = data.clone();
    tampered[0] ^= 1;
    assert_eq!(
        crypto::decrypt(&key, &iv, &tampered).await,
        Err(CryptoError::WrongPassword)
    );
}

/// Всё, что в базе, — живое и удалённое, двух видов — проходит через файл
/// под паролем и ложится в пустую базу таким же.
#[wasm_bindgen_test]
async fn backup_carries_the_whole_base() {
    let source = Probe::new();
    let store = source.open(DB_VERSION).await.expect("база открыта");
    let (first, second) = (note("первая ֏"), note("вторая"));
    let deleted = Tag {
        meta: meta(RecordId::random(), 1_760_000_000_500, Some(1_760_000_000_500)),
    };
    insert_all(&store, &[first.clone(), second.clone()], std::slice::from_ref(&deleted)).await;
    let salt = crypto::random(SALT_BYTES).expect("соль");
    let file = backup_of(&store, &salt).await;
    drop(store);
    source.delete().await;

    let sealed = open(&file).expect("файл открывается");
    assert_eq!(sealed.header(), &header());
    let (entries, key) = unlock(KINDS_UNDER_TEST, &sealed, &crypto::normalize(PASSWORD))
        .await
        .expect("копия расшифрована");
    assert_eq!(entries.len(), 3);
    assert_eq!(key.check(), key_for(PASSWORD, &salt).await.check());

    let target = Probe::new();
    let store = target.open(DB_VERSION).await.expect("база открыта");
    let restored = restore_all(&store, KINDS_UNDER_TEST, &entries)
        .await
        .expect("восстановлено");
    assert_eq!(
        restored,
        Restored {
            added: 3,
            updated: 0,
            unchanged: 0,
            // Самый ранний момент легших — у заметок (задача 110).
            oldest: Some(Timestamp::from_unix_millis(1_760_000_000_000)),
        }
    );
    let tx = store.read().expect("транзакция чтения");
    assert_eq!(tx.get::<Note>(first.meta.id()).await, Ok(Some(first)));
    assert_eq!(tx.get::<Note>(second.meta.id()).await, Ok(Some(second)));
    assert_eq!(tx.all::<Tag>().await, Ok(vec![deleted]));

    // Та же копия второй раз ничего не меняет.
    let again = restore_all(&store, KINDS_UNDER_TEST, &entries)
        .await
        .expect("восстановлено снова");
    assert_eq!((again.added, again.updated, again.unchanged, again.oldest), (0, 0, 3, None));

    drop((tx, store));
    target.delete().await;
}

/// Копия знает, сколько в ней весят файлы книги и убранные из записей
/// (задача 64): экран копии говорит это под днём последней копии, а отказ
/// «больше предела» — с этими числами.
#[wasm_bindgen_test]
async fn a_backup_counts_its_ledger_files() {
    use kvartik_core::model::attachment::{Attachment, FileBytes};
    let probe = Probe::new();
    let store = probe.open(DB_VERSION).await.expect("база открыта");
    let pdf = |len: usize| {
        let mut bytes = b"%PDF-1.7\n".to_vec();
        bytes.resize(len, b' ');
        FileBytes::new(bytes).expect("годный PDF")
    };
    let owner = RecordId::random();
    let kept = Attachment::create(owner, "чек.pdf", pdf(3_000), at(1_000));
    let removed = Attachment::create(owner, "лишний.pdf", pdf(2_000), at(1_000))
        .remove(at(2_000))
        .expect("убран");
    let mut writes = Writes::default();
    writes.insert(&kept);
    writes.insert(&removed);
    store.write().expect("транзакция").commit(writes).await.expect("записано");
    let salt = crypto::random(SALT_BYTES).expect("соль");
    let key = key_for(PASSWORD, &salt).await;
    let made = make_with(&store, KINDS, &key, &salt).await.expect("копия собрана");
    assert_eq!(made.files, Files { bytes: 5_000, removed: 2_000 }, "файлы книги в копии");
    drop(store);
    probe.delete().await;
}

/// Чужой пароль — копия не открывается, и в базу ничего не ложится.
#[wasm_bindgen_test]
async fn wrong_password_restores_nothing() {
    let probe = Probe::new();
    let store = probe.open(DB_VERSION).await.expect("база открыта");
    insert_all(&store, &[note("тайная")], &[]).await;
    let salt = crypto::random(SALT_BYTES).expect("соль");
    let file = backup_of(&store, &salt).await;
    let sealed = open(&file).expect("файл открывается");
    assert!(matches!(
        unlock(KINDS_UNDER_TEST, &sealed, "не тот пароль копии").await,
        Err(Failure::Crypto(CryptoError::WrongPassword))
    ));
    drop(store);
    probe.delete().await;
}

/// Слияние в базе: из копии берётся только то, что в ней позже; своё,
/// чего в копии нет, и своё, изменённое позже копии, остаётся.
#[wasm_bindgen_test]
async fn restore_keeps_the_later_record() {
    let probe = Probe::new();
    let store = probe.open(DB_VERSION).await.expect("база открыта");
    let older = note("было");
    let mut changed_later = note("в базе позже");
    let local_only = note("только здесь");
    insert_all(&store, &[older.clone(), changed_later.clone()], &[]).await;
    let salt = crypto::random(SALT_BYTES).expect("соль");
    let file = backup_of(&store, &salt).await;

    // После копии: одну запись поправили, одну добавили.
    let tx = store.write().expect("транзакция записи");
    let mut writes = Writes::default();
    changed_later.text = "правка после копии".to_owned();
    changed_later.meta = meta(changed_later.meta.id(), 1_760_000_009_000, None);
    writes.update(&changed_later);
    writes.insert(&local_only);
    tx.commit(writes).await.expect("правка сохранена");

    // А `older` в базе — версия раньше той, что в копии: копия её заменит.
    let stale = format!(
        r#"{{"format_version":1,"record":{{"meta":{{"id":"{}","modified":1,"deleted":null}},"text":"раньше копии","sum":{{"minor":150000,"currency":"USD"}}}}}}"#,
        older.meta.id()
    );
    put_raw(&store, &key(Note::KIND, older.meta.id()), &stale).await;

    let sealed = open(&file).expect("файл открывается");
    let (entries, _) = unlock(KINDS_UNDER_TEST, &sealed, &crypto::normalize(PASSWORD))
        .await
        .expect("копия расшифрована");
    let restored = restore_all(&store, KINDS_UNDER_TEST, &entries)
        .await
        .expect("восстановлено");
    assert_eq!(
        (restored.added, restored.updated, restored.unchanged),
        (0, 1, 1)
    );

    let tx = store.read().expect("транзакция чтения");
    assert_eq!(tx.get::<Note>(older.meta.id()).await, Ok(Some(older)));
    assert_eq!(
        tx.get::<Note>(changed_later.meta.id()).await,
        Ok(Some(changed_later))
    );
    assert_eq!(
        tx.get::<Note>(local_only.meta.id()).await,
        Ok(Some(local_only))
    );
    drop((tx, store));
    probe.delete().await;
}

/// Запись базы новее программы: восстановление отказывает целиком и не
/// пишет ничего — ни поверх неё, ни рядом.
#[wasm_bindgen_test]
async fn restore_over_newer_records_writes_nothing() {
    let probe = Probe::new();
    let store = probe.open(DB_VERSION).await.expect("база открыта");
    let (kept, fresh) = (note("в копии и в базе"), note("только в копии"));
    insert_all(&store, &[kept.clone(), fresh.clone()], &[]).await;
    let salt = crypto::random(SALT_BYTES).expect("соль");
    let file = backup_of(&store, &salt).await;
    let sealed = open(&file).expect("файл открывается");
    let (entries, _) = unlock(KINDS_UNDER_TEST, &sealed, &crypto::normalize(PASSWORD))
        .await
        .expect("копия расшифрована");

    let tx = store.write().expect("транзакция записи");
    let mut writes = Writes::default();
    let mut gone = fresh.clone();
    gone.meta = meta(fresh.meta.id(), 1_760_000_009_000, Some(1_760_000_009_000));
    writes.update(&gone);
    tx.commit(writes).await.expect("удаление сохранено");
    // «Удалённая» запись — под ключом свежей; копия её бы вернула. Рядом —
    // запись новой версии формата.
    let newer = format!(
        r#"{{"format_version":{},"record":{{"wings":2}}}}"#,
        FORMAT_VERSION + 1
    );
    let newer_key = key(Note::KIND, kept.meta.id());
    put_raw(&store, &newer_key, &newer).await;
    let raw_fresh = get_raw(&store, &key(Note::KIND, fresh.meta.id())).await;

    assert_eq!(
        restore_all(&store, KINDS_UNDER_TEST, &entries).await,
        Err(Failure::Store(StoreError::Newer))
    );
    assert_eq!(get_raw(&store, &newer_key).await, Some(newer));
    assert_eq!(
        get_raw(&store, &key(Note::KIND, fresh.meta.id())).await,
        raw_fresh
    );
    drop(store);
    probe.delete().await;
}

/// Копия берёт всё, что читается, а нечитаемое называет; запись новее
/// программы — отказ: копию сделает новая версия.
#[wasm_bindgen_test]
async fn make_skips_unreadable_and_refuses_newer() {
    let probe = Probe::new();
    let store = probe.open(DB_VERSION).await.expect("база открыта");
    insert_all(&store, &[note("читается")], &[]).await;
    put_raw(&store, &key(Note::KIND, RecordId::random()), "не JSON").await;
    let salt = crypto::random(SALT_BYTES).expect("соль");
    let secret = key_for(PASSWORD, &salt).await;
    let made = make_with(&store, KINDS_UNDER_TEST, &secret, &salt)
        .await
        .expect("копия сделана");
    assert_eq!((made.records, made.unreadable), (1, 1));

    let newer = format!(
        r#"{{"format_version":{},"record":{{}}}}"#,
        FORMAT_VERSION + 1
    );
    put_raw(&store, &key(Note::KIND, RecordId::random()), &newer).await;
    assert!(matches!(
        make_with(&store, KINDS_UNDER_TEST, &secret, &salt).await,
        Err(Failure::Store(StoreError::Newer))
    ));
    drop(store);
    probe.delete().await;
}

/// Пустая база — пустая, и после первой записи — уже нет; выгрузка берёт
/// записи всех видов с их ключами.
#[wasm_bindgen_test]
async fn dump_and_emptiness() {
    let probe = Probe::new();
    let store = probe.open(DB_VERSION).await.expect("база открыта");
    let tx = store.read().expect("транзакция чтения");
    assert_eq!(tx.is_empty().await, Ok(true));
    assert_eq!(tx.dump().await, Ok(Vec::new()));
    drop(tx);

    let first = note("одна");
    let tag = Tag {
        meta: RecordMeta::new(at(1)),
    };
    insert_all(
        &store,
        std::slice::from_ref(&first),
        std::slice::from_ref(&tag),
    )
    .await;
    let tx = store.read().expect("транзакция чтения");
    assert_eq!(tx.is_empty().await, Ok(false));
    let dumped = tx.dump().await.expect("выгружено");
    let mut kinds: Vec<(String, String)> = dumped
        .iter()
        .map(|item| {
            let raw = item.raw();
            (raw.kind.to_owned(), raw.id.to_owned())
        })
        .collect();
    kinds.sort();
    let mut expected = vec![
        ("note".to_owned(), first.meta.id().to_string()),
        ("tag".to_owned(), tag.meta.id().to_string()),
    ];
    expected.sort();
    assert_eq!(kinds, expected);
    assert!(dumped.iter().all(|item| item.raw().text.is_some()));
    drop((tx, store));
    probe.delete().await;
}

/// Устройство помнит момент копии и пароль копий, а испорченную запись
/// не читает.
#[wasm_bindgen_test]
fn device_remembers_backups() {
    let prefix = format!("kvartik-test-{}.", RecordId::random());
    let device = Device::with_prefix(&prefix);
    assert_eq!(device.last_backup(), None);
    assert_eq!(device.backup_key(), None);
    assert!(device.set_last_backup(at(1_760_270_400_000)));
    assert_eq!(device.last_backup(), Some(at(1_760_270_400_000)));
    let key = DeviceKey::new(KDF_ITERATIONS, vec![7; 16], vec![9; 32]).expect("годится");
    assert!(device.set_backup_key(&key));
    assert_eq!(device.backup_key(), Some(key));

    let storage = web_sys::window()
        .and_then(|window| window.local_storage().ok().flatten())
        .expect("localStorage есть");
    for (name, value) in [("last_backup", "вчера"), ("backup_key", "{}")] {
        storage
            .set_item(&format!("{prefix}{name}"), value)
            .expect("записано");
    }
    assert_eq!(device.last_backup(), None);
    assert_eq!(device.backup_key(), None);
    // Размер последней копии и файлов книги в ней (задача 64): что записано,
    // то и читается; испорченная запись — «нет», а не нули.
    let files = Files { bytes: 7_000_000, removed: 1_500 };
    assert!(device.set_last_size(9_000_000, files));
    assert_eq!(device.last_size(), Some((9_000_000, files)));
    for spoiled in ["9000000 7000000", "9000000 7000000 1500 4", "много 7000000 1500", "9000000 1500 7000000"] {
        storage.set_item(&format!("{prefix}last_size"), spoiled).expect("записано");
        assert_eq!(device.last_size(), None, "прочитано испорченное: {spoiled}");
    }
    assert!(device.forget_last_size());
    assert_eq!(device.last_size(), None);
    device.clear();
}

/// Устройство помнит версии: о какой человек знает и какую пробовали
/// включить. Запись, которая не читается, — «нет», а не паника и не ноль:
/// по ней окно решает, перезагружать ли страницу.
#[wasm_bindgen_test]
fn device_remembers_versions() {
    let prefix = format!("kvartik-test-{}.", RecordId::random());
    let device = Device::with_prefix(&prefix);
    assert_eq!(device.seen_version(), None);
    assert_eq!(device.update_tried(), None);
    let version = Version::new(0, 0, 63);
    assert!(device.set_seen_version(version));
    assert!(device.set_update_tried(version));
    assert_eq!(device.seen_version(), Some(version));
    assert_eq!(device.update_tried(), Some(version));

    let storage = web_sys::window()
        .and_then(|window| window.local_storage().ok().flatten())
        .expect("localStorage есть");
    for name in ["seen_version", "update_tried"] {
        storage
            .set_item(&format!("{prefix}{name}"), "вчера")
            .expect("записано");
    }
    assert_eq!(device.seen_version(), None);
    assert_eq!(device.update_tried(), None);
    device.clear();
}

/// Выбранный файл: больше предела — не читается вовсе, не UTF-8 — не
/// текст, остальное — текстом.
#[wasm_bindgen_test]
async fn chosen_file_is_read_within_limit() {
    let file = |bytes: &[u8]| {
        web_sys::File::new_with_u8_array_sequence(
            &js_sys::Array::of1(&js_sys::Uint8Array::from(bytes)),
            "kvartik-2026-10-12.json",
        )
        .expect("файл собран")
    };
    let text = "{\"file\":\"kvartik-backup\"} ֏";
    assert_eq!(
        files::read(&file(text.as_bytes()), 1024).await,
        Ok(text.to_owned())
    );
    assert_eq!(
        files::read(&file(text.as_bytes()), 8).await,
        Err(FileError::TooLarge)
    );
    assert_eq!(
        files::read(&file(&[0xff, 0xfe, b'{']), 1024).await,
        Err(FileError::NotText)
    );
    assert_eq!(files::read(&file(b""), 1024).await, Ok(String::new()));
}

/// Файл для скачивания — адрес `blob:`, и по нему лежит ровно текст копии;
/// у самого файла — имя копии.
#[wasm_bindgen_test]
async fn download_holds_the_file() {
    let text = "{\"file\":\"kvartik-backup\",\"x\":\"֏\"}";
    let download = Download::text(text, "kvartik-2026-10-15.json").expect("файл собран");
    assert!(download.url().starts_with("blob:"), "{}", download.url());
    assert_eq!(download.name(), "kvartik-2026-10-15.json");
    assert_eq!(download.file().type_(), "application/octet-stream");
    let window = web_sys::window().expect("окно есть");
    let response: web_sys::Response = window
        .fetch_with_str(download.url())
        .await
        .expect("файл читается")
        .unchecked_into();
    let body = response.text().expect("текст").await.expect("текст пришёл");
    assert_eq!(body.as_string().as_deref(), Some(text));
}

/// Копия другого ИП в непустую программу молча не ложится (задача 38,
/// этап 1): сверка возвращает обоих и не пишет ничего, а ответ «слить» —
/// пишет. Копия того же ИП — с тем же ՀՎՀՀ, хоть профиль и заведён заново, —
/// ложится без вопроса и тогда, когда в базе уже двое.
#[wasm_bindgen_test]
async fn backup_of_another_ie_asks_first() {
    let profile = |name: &str, tin: &str, millis: i64| {
        let mut data = ProfileData::default();
        data.name.en = Line::new(name).expect("годное имя");
        data.tin = Some(Tin::parse(tin).expect("годный ՀՎՀՀ"));
        Profile::save(None, data, at(millis))
            .expect("годный профиль")
            .expect("новый профиль")
    };
    let aram = profile("IE Aram Petrosyan", "12345678", 1_000);
    let aram_again = profile("IE Aram P.", "12345678", 2_000);
    let anna = profile("IE Anna Hakobyan", "87654321", 3_000);
    let salt = crypto::random(SALT_BYTES).expect("соль");
    let annas = entries_with(&anna, &salt).await;
    let arams = entries_with(&aram_again, &salt).await;

    let probe = Probe::new();
    let store = probe.open(DB_VERSION).await.expect("база открыта");
    let mut writes = Writes::default();
    writes.insert(&aram);
    store
        .write()
        .expect("транзакция записи")
        .commit(writes)
        .await
        .expect("профиль сохранён");
    let profiles = |store: &Store| {
        let tx = store.read().expect("транзакция чтения");
        async move { tx.all::<Profile>().await.map(|all| all.len()) }
    };
    let mut wrong = Vec::new();
    match restore(&store, KINDS, &annas, OtherIe::Ask).await {
        Ok(Merged::Other { ours: Some(ours), theirs })
            if ours.name(Lang::En) == "IE Aram Petrosyan"
                && theirs.name(Lang::En) == "IE Anna Hakobyan"
                && theirs.tin().map(Tin::as_str) == Some("87654321") => {}
        other => wrong.push(format!("копия другого ИП: {other:?}")),
    }
    if profiles(&store).await != Ok(1) {
        wrong.push(format!("после вопроса профилей в базе {:?}, а не 1", profiles(&store).await));
    }
    match restore(&store, KINDS, &annas, OtherIe::Merge).await {
        Ok(Merged::Done(restored)) if restored.added == 1 => {}
        other => wrong.push(format!("ответ «слить»: {other:?}")),
    }
    if profiles(&store).await != Ok(2) {
        wrong.push(format!("после «слить» профилей {:?}, а не 2", profiles(&store).await));
    }
    match restore(&store, KINDS, &arams, OtherIe::Ask).await {
        Ok(Merged::Done(restored)) if restored.added == 1 => {}
        other => wrong.push(format!("копия того же ИП с тем же ՀՎՀՀ: {other:?}")),
    }
    drop(store);
    probe.delete().await;

    // Профиль базы не читается: чужая копия — вопрос без имени программы,
    // и не записано ничего. Профиль новее программы — отказ целиком: кто
    // он, эта версия не знает.
    let damaged = Probe::new();
    let store = damaged.open(DB_VERSION).await.expect("база открыта");
    let profile_key = key(Profile::KIND, aram.id());
    put_raw(&store, &profile_key, "не запись").await;
    match restore(&store, KINDS, &annas, OtherIe::Ask).await {
        Ok(Merged::Other { ours: None, theirs }) if theirs.name(Lang::En) == "IE Anna Hakobyan" => {}
        other => wrong.push(format!("профиль базы не читается: {other:?}")),
    }
    if get_raw(&store, &key(Profile::KIND, anna.id())).await.is_some() {
        wrong.push("после вопроса при нечитаемом профиле копия записана".to_owned());
    }
    let newer = format!(r#"{{"format_version":{},"record":{{"wings":2}}}}"#, FORMAT_VERSION + 1);
    put_raw(&store, &profile_key, &newer).await;
    match restore(&store, KINDS, &annas, OtherIe::Ask).await {
        Err(Failure::Store(StoreError::Newer)) => {}
        other => wrong.push(format!("профиль базы новее программы: {other:?}")),
    }
    if get_raw(&store, &key(Profile::KIND, anna.id())).await.is_some() {
        wrong.push("при профиле новее программы копия записана".to_owned());
    }
    drop(store);
    damaged.delete().await;
    assert!(wrong.is_empty(), "копия другого ИП: {wrong:#?}");
}

include!("../../../../core/tests/backups/samples.rs");

/// Копия каждой выпущенной версии открывается этой (Правило 2):
/// расшифровывается своим паролем, читается видами записей программы
/// (`KINDS`) и ложится в пустую базу целиком. Список образцов — общий
/// с тестом ядра.
#[wasm_bindgen_test]
async fn every_released_backup_restores() {
    let mut wrong = Vec::new();
    for sample in SAMPLES {
        let sealed = match open(sample.text) {
            Ok(sealed) => sealed,
            Err(error) => {
                wrong.push(format!("{}: {error:?}", sample.path));
                continue;
            }
        };
        let entries = match unlock(FILE_KINDS, &sealed, &crypto::normalize(sample.password)).await {
            Ok((entries, _)) => entries,
            Err(error) => {
                wrong.push(format!("{}: {error:?}", sample.path));
                continue;
            }
        };
        // Копия всех ИП (формат 15 и новее) — по ИП, в базы свежего
        // устройства: записи конторы — в список, остальные — в базы.
        if is_full(&entries) {
            if let Err(error) = by_ip(&entries, sample.records).await {
                wrong.push(format!("{}: {error}", sample.path));
            }
            continue;
        }
        let probe = Probe::new();
        let store = probe.open(DB_VERSION).await.expect("база открыта");
        match restore_all(&store, KINDS, &entries).await {
            Ok(restored) if entries.len() == sample.records && restored.added == sample.records => {
            }
            other => wrong.push(format!(
                "{}: записей {}, восстановление {other:?}",
                sample.path,
                entries.len()
            )),
        }
        drop(store);
        probe.delete().await;
    }
    assert!(wrong.is_empty(), "образцы не восстанавливаются: {wrong:#?}");
}

/// Копия всех ИП — на свежее устройство: своя пустая `kvartik` и пустая
/// контора. Ложится всё: записи конторы — в список, остальные — в базы;
/// потом базы стираются.
async fn by_ip(entries: &[Entry], records: usize) -> Result<(), String> {
    delete_database(DB_NAME).await;
    delete_database(OFFICE_DB).await;
    let open = Store::open_base(DB_NAME, |_| {}).await.map_err(|e| format!("{e:?}"))?;
    let office = Store::open_office(|_| {}).await.map_err(|e| format!("{e:?}"))?;
    let listed = entries.iter().filter(|entry| entry.kind() == Ip::KIND).count();
    let outcome = restore_by_ip(&open, DB_NAME, &office, entries.to_vec(), None, Some(DB_NAME)).await;
    let ips = office_list(&office).await.map(|listed| listed.records).unwrap_or_default();
    drop((open, office));
    for ip in &ips {
        delete_database(ip.base()).await;
    }
    delete_database(OFFICE_DB).await;
    delete_database(DB_NAME).await;
    match outcome {
        Ok(done) if done.restored.added + listed == records && ips.len() == listed => Ok(()),
        other => Err(format!("ИП {}, восстановление {:?}", ips.len(), other.map(|done| done.restored))),
    }
}
