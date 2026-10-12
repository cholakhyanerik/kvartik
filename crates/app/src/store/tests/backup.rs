//! Браузерные тесты копии (задача 7): настоящий WebCrypto и настоящая
//! IndexedDB. Что внутри файла и как сливаются записи, проверяет ядро
//! (`kvartik_core::format::backup`); здесь — шифр браузера и путь от базы
//! к файлу и обратно.

use kvartik_core::format::backup::{
    DeviceKey, Header, KDF_ITERATIONS, KINDS, Kind, SALT_BYTES, open,
};
use wasm_bindgen::JsCast;

use super::*;
use crate::platform::crypto::{self, CryptoError, Key};
use crate::platform::file::{self as files, Download, FileError};
use crate::store::backup::{Failure, Restored, make, restore, unlock};
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

/// Копия всей базы под `PASSWORD` — текстом файла.
async fn backup_of(store: &Store, salt: &[u8]) -> String {
    let key = key_for(PASSWORD, salt).await;
    make(
        store,
        KINDS_UNDER_TEST,
        &header(),
        &key,
        salt,
        KDF_ITERATIONS,
    )
    .await
    .expect("копия сделана")
    .file
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
    let restored = restore(&store, KINDS_UNDER_TEST, &entries)
        .await
        .expect("восстановлено");
    assert_eq!(
        restored,
        Restored {
            added: 3,
            updated: 0,
            unchanged: 0
        }
    );
    let tx = store.read().expect("транзакция чтения");
    assert_eq!(tx.get::<Note>(first.meta.id()).await, Ok(Some(first)));
    assert_eq!(tx.get::<Note>(second.meta.id()).await, Ok(Some(second)));
    assert_eq!(tx.all::<Tag>().await, Ok(vec![deleted]));

    // Та же копия второй раз ничего не меняет.
    let again = restore(&store, KINDS_UNDER_TEST, &entries)
        .await
        .expect("восстановлено снова");
    assert_eq!((again.added, again.updated, again.unchanged), (0, 0, 3));

    drop((tx, store));
    target.delete().await;
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
    let restored = restore(&store, KINDS_UNDER_TEST, &entries)
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
        restore(&store, KINDS_UNDER_TEST, &entries).await,
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
    let made = make(
        &store,
        KINDS_UNDER_TEST,
        &header(),
        &secret,
        &salt,
        KDF_ITERATIONS,
    )
    .await
    .expect("копия сделана");
    assert_eq!((made.records, made.unreadable), (1, 1));

    let newer = format!(
        r#"{{"format_version":{},"record":{{}}}}"#,
        FORMAT_VERSION + 1
    );
    put_raw(&store, &key(Note::KIND, RecordId::random()), &newer).await;
    assert!(matches!(
        make(
            &store,
            KINDS_UNDER_TEST,
            &header(),
            &secret,
            &salt,
            KDF_ITERATIONS
        )
        .await,
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

/// Файл для скачивания — адрес `blob:`, и по нему лежит ровно текст копии.
#[wasm_bindgen_test]
async fn download_holds_the_file() {
    let text = "{\"file\":\"kvartik-backup\",\"x\":\"֏\"}";
    let download = Download::new(text).expect("файл собран");
    assert!(download.url().starts_with("blob:"), "{}", download.url());
    let window = web_sys::window().expect("окно есть");
    let response: web_sys::Response = window
        .fetch_with_str(download.url())
        .await
        .expect("файл читается")
        .unchecked_into();
    let body = response.text().expect("текст").await.expect("текст пришёл");
    assert_eq!(body.as_string().as_deref(), Some(text));
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
        let entries = match unlock(KINDS, &sealed, &crypto::normalize(sample.password)).await {
            Ok((entries, _)) => entries,
            Err(error) => {
                wrong.push(format!("{}: {error:?}", sample.path));
                continue;
            }
        };
        let probe = Probe::new();
        let store = probe.open(DB_VERSION).await.expect("база открыта");
        match restore(&store, KINDS, &entries).await {
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
