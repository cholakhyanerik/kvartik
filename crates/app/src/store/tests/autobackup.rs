//! Браузерные тесты автокопии (задача 29): запись в папку и база
//! устройства — папка и ключ. Папка — из OPFS (`navigator.storage
//! .getDirectory()`): окно выбора папки тест не нажмёт, а OPFS отдаёт такую
//! же `FileSystemDirectoryHandle`, с той же записью через временный файл.

use js_sys::{Object, Reflect};
use kvartik_core::format::backup::{DeviceKey, KDF_ITERATIONS, SALT_BYTES};
use kvartik_core::model::record::RecordId;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{
    Blob, FileSystemDirectoryHandle, FileSystemFileHandle, FileSystemGetDirectoryOptions,
    FileSystemRemoveOptions, IdbDatabase, IdbRequest, IdbTransactionMode,
};

use super::*;
use crate::platform::autobackup::{self as folders};
use crate::platform::crypto;
use crate::store::autobackup::{Saved, forget_in, load_from, save_to};

/// Своя папка теста внутри OPFS: тесты не видят файлов друг друга.
pub(crate) async fn scratch() -> (FileSystemDirectoryHandle, String) {
    let window = web_sys::window().expect("окно есть");
    let root: FileSystemDirectoryHandle = window
        .navigator()
        .storage()
        .get_directory()
        .await
        .expect("OPFS открылась")
        .unchecked_into();
    let name = format!("k29-{}", RecordId::random());
    let options = FileSystemGetDirectoryOptions::new();
    options.set_create(true);
    let folder = root
        .get_directory_handle_with_options(&name, &options)
        .await
        .expect("папка теста создана")
        .unchecked_into();
    (folder, name)
}

/// Убрать папку теста.
pub(crate) async fn drop_scratch(name: &str) {
    let window = web_sys::window().expect("окно есть");
    let root: FileSystemDirectoryHandle = window
        .navigator()
        .storage()
        .get_directory()
        .await
        .expect("OPFS открылась")
        .unchecked_into();
    let options = FileSystemRemoveOptions::new();
    options.set_recursive(true);
    let _ = root.remove_entry_with_options(name, &options).await;
}

/// Текст файла папки.
pub(crate) async fn read_file(folder: &FileSystemDirectoryHandle, name: &str) -> Option<String> {
    let file: FileSystemFileHandle = folder.get_file_handle(name).await.ok()?.unchecked_into();
    let blob: Blob = file.get_file().await.ok()?.unchecked_into();
    blob.text().await.ok()?.as_string()
}

/// Файл пишется целиком и заменяет прежний, а не дописывается к нему;
/// перечисление отдаёт только имена с началом `prefix`; убранного файла
/// нет, а убрать уже убранный — не ошибка.
#[wasm_bindgen_test]
async fn folder_writes_lists_and_removes() {
    let (folder, name) = scratch().await;
    let ours = "kvartik-auto-2026-11-01-1a2b.json";
    let mut wrong = Vec::new();
    for text in ["первая копия, длинная", "вторая"] {
        if let Err(error) = folders::write(&folder, ours, text.as_bytes()).await {
            wrong.push(format!("запись «{text}»: {error:?}"));
        }
    }
    if read_file(&folder, ours).await.as_deref() != Some("вторая") {
        wrong.push("второй файл не заменил первый целиком".to_owned());
    }
    if let Err(error) = folders::write(&folder, "отчёт.txt", b"x").await {
        wrong.push(format!("чужой файл: {error:?}"));
    }
    match folders::names(&folder, "kvartik-auto-").await {
        Ok(names) if names == [ours] => {}
        other => wrong.push(format!("перечисление: {other:?}")),
    }
    if let Err(error) = folders::remove(&folder, ours).await {
        wrong.push(format!("удаление: {error:?}"));
    }
    if read_file(&folder, ours).await.is_some() {
        wrong.push("убранный файл остался".to_owned());
    }
    if let Err(error) = folders::remove(&folder, ours).await {
        wrong.push(format!("повторное удаление: {error:?}"));
    }
    match folders::names(&folder, "kvartik-auto-").await {
        Ok(names) if names.is_empty() => {}
        other => wrong.push(format!("после удаления: {other:?}")),
    }
    if read_file(&folder, "отчёт.txt").await.as_deref() != Some("x") {
        wrong.push("чужой файл пропал".to_owned());
    }
    drop_scratch(&name).await;
    assert!(wrong.is_empty(), "папка: {wrong:#?}");
}

/// Ключ проб и пароль устройства, из которого он.
async fn sample_key() -> (crypto::Key, DeviceKey) {
    let salt = crypto::random(SALT_BYTES).expect("соль");
    let key = crypto::derive(&crypto::normalize("Երևան автокопия 2026"), &salt, KDF_ITERATIONS)
        .await
        .expect("ключ получен");
    let password = DeviceKey::new(KDF_ITERATIONS, salt, key.check().to_vec()).expect("пароль устройства");
    (key, password)
}

/// Удалить базу устройства теста.
async fn drop_base(name: &str) {
    let factory = web_sys::window()
        .and_then(|window| window.indexed_db().ok().flatten())
        .expect("IndexedDB есть");
    let request = factory.delete_database(name).expect("удаление базы поставлено");
    let _ = Settle::new(IdbRequest::from(request)).await;
}

/// Положить в базу устройства значение как есть — так, как его оставила бы
/// другая версия программы или порча.
async fn put_raw_auto(name: &str, value: &JsValue) {
    // Пустое чтение создаёт базу и её хранилище.
    let _ = load_from(name).await;
    let factory = web_sys::window()
        .and_then(|window| window.indexed_db().ok().flatten())
        .expect("IndexedDB есть");
    let request = factory.open(name).expect("открытие поставлено");
    let db: IdbDatabase = wait(IdbRequest::from(request))
        .await
        .expect("база открыта")
        .unchecked_into();
    let tx = db
        .transaction_with_str_and_mode("things", IdbTransactionMode::Readwrite)
        .expect("транзакция");
    let done = Settle::new(tx.clone());
    tx.object_store("things")
        .expect("хранилище")
        .put_with_key(value, &"autobackup".into())
        .expect("запись поставлена");
    done.await.expect("записано");
    db.close();
}

/// База устройства помнит папку и ключ: та же папка, тот же ключ — им
/// расшифровывается зашифрованное прежним, — и ключ остаётся
/// неизвлекаемым. Забытая автокопия не читается вовсе. Папки как объекты
/// в базе кладёт только Chromium — там же, где и выбор папки.
#[wasm_bindgen_test]
async fn device_base_keeps_the_folder_and_the_key() {
    if !folders::available() {
        web_sys::console::log_1(&"автокопии в этом браузере нет — тест базы устройства пропущен".into());
        return;
    }
    let base = format!("kvartik-device-test-{}", RecordId::random());
    let (folder, name) = scratch().await;
    let (key, password) = sample_key().await;
    let mut wrong = Vec::new();
    if !matches!(load_from(&base).await, Ok(None)) {
        wrong.push("в новой базе устройства уже что-то есть".to_owned());
    }
    let device = sample_key().await.1;
    let saved = Saved {
        folder: folder.clone(),
        place: "OneDrive/Kvartik".to_owned(),
        key: key.aes().clone(),
        lock: password.clone(),
        device: device.clone(),
    };
    if let Err(error) = save_to(&base, &saved).await {
        wrong.push(format!("не запомнилось: {error:?}"));
    }
    match load_from(&base).await {
        Ok(Some(loaded)) => {
            let same = loaded.folder.is_same_entry(&folder).await.ok().and_then(|same| same.as_bool());
            if same != Some(true) {
                wrong.push("папка не та".to_owned());
            }
            if loaded.key.extractable() {
                wrong.push("ключ стал извлекаемым".to_owned());
            }
            if loaded.lock != password || loaded.device != device || loaded.place != "OneDrive/Kvartik" {
                wrong.push("замок, пароль устройства или имя папки не те".to_owned());
            }
            let iv = crypto::random(12).expect("вектор");
            let sealed = crypto::encrypt(&key, &iv, "копия ֏".as_bytes()).await.expect("зашифровано");
            let reopened = crypto::Key::stored(loaded.key, password.check().to_vec());
            if crypto::decrypt(&reopened, &iv, &sealed).await.as_deref() != Ok("копия ֏".as_bytes()) {
                wrong.push("ключ из базы не расшифровал".to_owned());
            }
        }
        other => wrong.push(format!("не прочиталось: {:?}", other.map(|saved| saved.is_some()))),
    }
    if let Err(error) = forget_in(&base).await {
        wrong.push(format!("не забылось: {error:?}"));
    }
    if !matches!(load_from(&base).await, Ok(None)) {
        wrong.push("забытая автокопия читается".to_owned());
    }
    drop_base(&base).await;
    drop_scratch(&name).await;
    assert!(wrong.is_empty(), "база устройства: {wrong:#?}");
}

/// Запись новее программы не читается и не переписывается (Правило 2),
/// а без папки или ключа — испорчена: окно попросит выбрать папку снова.
#[wasm_bindgen_test]
async fn device_base_refuses_newer_and_damaged() {
    if !folders::available() {
        web_sys::console::log_1(&"автокопии в этом браузере нет — тест базы устройства пропущен".into());
        return;
    }
    let base = format!("kvartik-device-test-{}", RecordId::random());
    let (folder, name) = scratch().await;
    let (key, password) = sample_key().await;
    let mut wrong = Vec::new();

    let newer = Object::new();
    let _ = Reflect::set(&newer, &"shape".into(), &2.into());
    put_raw_auto(&base, &newer).await;
    if !matches!(load_from(&base).await, Err(StoreError::Newer)) {
        wrong.push("запись новее прочиталась".to_owned());
    }
    let saved = Saved {
        folder,
        place: "Kvartik".to_owned(),
        key: key.aes().clone(),
        lock: password.clone(),
        device: password,
    };
    if !matches!(save_to(&base, &saved).await, Err(StoreError::Newer)) {
        wrong.push("запись новее переписана".to_owned());
    }
    if !matches!(load_from(&base).await, Err(StoreError::Newer)) {
        wrong.push("после отказа записи запись новее пропала".to_owned());
    }

    let damaged = Object::new();
    let _ = Reflect::set(&damaged, &"shape".into(), &1.into());
    let _ = Reflect::set(&damaged, &"folder".into(), &"не папка".into());
    let _ = Reflect::set(&damaged, &"key".into(), key.aes());
    put_raw_auto(&base, &damaged).await;
    if !matches!(load_from(&base).await, Err(StoreError::Damaged)) {
        wrong.push("запись без папки не названа порчей".to_owned());
    }

    drop_base(&base).await;
    drop_scratch(&name).await;
    assert!(wrong.is_empty(), "база устройства: {wrong:#?}");
}
