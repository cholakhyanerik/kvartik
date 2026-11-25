//! Где автокопия (задача 29): выбранная папка и ключ шифра — в своей базе
//! IndexedDB этого устройства, [`DEVICE_DB`].
//!
//! Это объекты браузера — `FileSystemDirectoryHandle` и неизвлекаемый
//! `CryptoKey`, — а их держит только IndexedDB: структурным клонированием
//! (по спецификациям File System Access и WebCrypto оба `[Serializable]`).
//! `localStorage` (`store/device.rs`) хранит одни строки.
//!
//! Неизвлекаемый ключ не выгрузит ни программа, ни чужой скрипт
//! (`exportKey` откажет), но на диске Chromium держит его байты так же
//! открыто, как сами данные базы (по исходникам Chromium,
//! `v8_script_value_serializer_for_modules.cc`, 2026-11-03): у кого есть
//! файлы профиля браузера, у того и данные, и ключ. Соль у этого ключа своя
//! ([`Saved::lock`]), поэтому он открывает только файлы автокопий — те же
//! данные, что лежат на устройстве открыто, и их следующие копии тому, кто
//! однажды скопировал профиль и потом добрался до облака. Ручные копии того
//! же пароля — у бухгалтера, в почте — ключ из профиля не открывает: их
//! соль — соль устройства (ревью задачи 29).
//!
//! Не в базе записей (`store/mod.rs`): там каждое значение — строка JSON
//! ядра, и копия разбирает их все, а это состояние устройства, как
//! запомненный язык. В копию оно не идёт: перенесённое на другое устройство,
//! оно указывало бы на папку, которой там нет. Своя база, а не второе
//! хранилище объектов в базе записей, — чтобы не поднимать версию базы
//! записей (Правило 5): у каждого, кто держит открытой вкладку прежней
//! версии, та закрыла бы соединение и попросила перезагрузиться, а база
//! устройства нужна только там, где есть автокопия.
//!
//! Там же — ключ ручной копии в одно нажатие (задача 124, часть C, решение
//! человека 2026-11-25; [`CopyKey`]): тоже неизвлекаемый и со своей солью —
//! открывает он только копии, сделанные этим ключом, а копии, сделанные
//! паролем с солью устройства, — нет. Запись — под своим ключом в том же
//! хранилище объектов: версия базы устройства прежняя.
//!
//! Соединение — на одно действие: открыть, прочитать или записать, закрыть.
//! Действия редкие — включить автокопию, начать её, — а соединение, которое
//! держат, мешало бы новой версии этой базы
//! (`.claude/rules/pitfalls/indexeddb.md`, «Старая вкладка молча держит базу»).
//! Запись — `durability: "strict"`, как у записей (шапка `store/mod.rs`).

use js_sys::{Object, Reflect};
use kvartik_core::format::backup::DeviceKey;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{
    CryptoKey, FileSystemDirectoryHandle, IdbDatabase, IdbObjectStore, IdbOpenDbRequest,
    IdbRequest, IdbTransaction, IdbTransactionMode, IdbVersionChangeEvent,
};

use super::settle::Settle;
use super::{DatabaseOptions, StoreError, failure, log, open_error, wait};

/// Имя базы устройства. Как и имя базы записей, оно навсегда: под другим
/// именем прежние настройки потерялись бы, и автокопия молча встала бы.
pub const DEVICE_DB: &str = "kvartik-device";

/// Версия базы устройства: хранилища объектов. Растёт вместе с шагом
/// в [`upgrade`] (Правило 5).
const DEVICE_DB_VERSION: u32 = 1;

/// Хранилище объектов базы устройства.
const THINGS: &str = "things";

/// Ключ записи автокопии в нём.
const AUTOBACKUP: &str = "autobackup";

/// Ключ записи ключа ручной копии (задача 124, часть C).
const COPY: &str = "copy";

/// Версия вида записи автокопии: прочитав запись новее себя, старая вкладка
/// не переписывает её, а говорит, что нужна новая версия (Правило 2).
const SHAPE: u32 = 1;

/// Что помнит устройство об автокопии.
pub struct Saved {
    /// Папка автокопии — своя, «Kvartik» внутри выбранной
    /// (`platform::autobackup::subfolder`).
    pub folder: FileSystemDirectoryHandle,
    /// Как её назвать человеку: «OneDrive/Kvartik». Пути браузер не даёт,
    /// а выбранную папку автокопия не хранит — только её имя.
    pub place: String,
    /// Ключ шифра автокопий — неизвлекаемый, из пароля копий и своей соли.
    pub key: CryptoKey,
    /// Соль, итерации и проверочное значение этого ключа: соль и итерации
    /// ложатся в замок каждого файла автокопии. Соль своя, а не устройства:
    /// ключ, который браузер держит на диске, открывает тогда только
    /// автокопии, а ручные копии того же пароля — нет (ревью задачи 29).
    pub lock: DeviceKey,
    /// Пароль копий устройства, под которым автокопию включили: сменили
    /// его — автокопия не пишет под прежним, пока пароль не введут.
    pub device: DeviceKey,
}

/// Ключ ручной копии в одно нажатие (задача 124, часть C): после копии
/// с паролем «Сделать копию» — одна кнопка, а пароль спрашивается раз в 30
/// дней, как у автокопии (`format::autobackup::check_due`).
pub struct CopyKey {
    /// Ключ шифра — неизвлекаемый, из пароля копий и своей соли.
    pub key: CryptoKey,
    /// Соль, итерации и проверочное значение этого ключа — в замок копии.
    pub lock: DeviceKey,
    /// Пароль копий устройства, под которым ключ запомнен: сменили его —
    /// ключ не годится, и снова форма.
    pub device: DeviceKey,
}

/// Ключ ручной копии; `None` — его нет: копий с паролем ещё не было, его
/// забыли или браузер его не сохранил.
pub async fn load_copy() -> Result<Option<CopyKey>, StoreError> {
    let Some(value) = get(DEVICE_DB, COPY).await? else {
        return Ok(None);
    };
    if newer(&value) {
        return Err(StoreError::Newer);
    }
    let field = |name: &str| Reflect::get(&value, &name.into()).ok();
    let password = |name: &str| {
        field(name)
            .and_then(|text| text.as_string())
            .and_then(|text| DeviceKey::from_text(&text))
    };
    // Ключ, который браузер не вернул (null), — как его нет: тогда снова форма
    // с паролем, а не сбой (WebKit отдаёт `null` вместо неизвлекаемого ключа
    // некоторых видов, bug 312279).
    let key = field("key").and_then(|key| key.dyn_into::<CryptoKey>().ok());
    match (key, password("lock"), password("device")) {
        (Some(key), Some(lock), Some(device)) => Ok(Some(CopyKey { key, lock, device })),
        _ => {
            log("copy key", &"не читается".into());
            Ok(None)
        }
    }
}

/// Запомнить ключ ручной копии.
pub async fn save_copy(copy: &CopyKey) -> Result<(), StoreError> {
    let value = Object::new();
    let set = |field: &str, part: &JsValue| {
        Reflect::set(&value, &field.into(), part).map_err(|error| failure("put", Some(error)))
    };
    set("shape", &SHAPE.into())?;
    set("key", &copy.key)?;
    set("lock", &copy.lock.to_text().into())?;
    set("device", &copy.device.to_text().into())?;
    put(DEVICE_DB, COPY, &value).await
}

/// Забыть ключ ручной копии: сменили пароль или «Забыли пароль?».
pub async fn forget_copy() -> Result<(), StoreError> {
    let db = Open::new(DEVICE_DB).await?;
    let tx = db.begin(IdbTransactionMode::Readwrite)?;
    let done = Settle::new(tx.clone());
    if let Err(error) = store(&tx)?.delete(&COPY.into()) {
        let _ = tx.abort();
        return Err(failure("delete", Some(error)));
    }
    finish(&tx, done).await
}

/// Значение под ключом `key` базы `name`; `None` — его нет.
async fn get(name: &str, key: &str) -> Result<Option<JsValue>, StoreError> {
    let db = Open::new(name).await?;
    let tx = db.begin(IdbTransactionMode::Readonly)?;
    let request = store(&tx)?.get(&key.into()).map_err(|error| failure("get", Some(error)))?;
    let value = wait(request).await?;
    Ok((!value.is_undefined() && !value.is_null()).then_some(value))
}

/// Записать значение под ключом `key` базы `name`, если там не запись новее
/// программы.
async fn put(name: &str, key: &str, value: &JsValue) -> Result<(), StoreError> {
    let db = Open::new(name).await?;
    let tx = db.begin(IdbTransactionMode::Readwrite)?;
    let done = Settle::new(tx.clone());
    let request = store(&tx)?.get(&key.into()).map_err(|error| failure("get", Some(error)))?;
    let current = wait(request).await?;
    if newer(&current) {
        let _ = tx.abort();
        return Err(StoreError::Newer);
    }
    if let Err(error) = store(&tx)?.put_with_key(value, &key.into()) {
        let _ = tx.abort();
        return Err(failure("put", Some(error)));
    }
    finish(&tx, done).await
}

/// Что помнит устройство об автокопии; `None` — автокопия не включена.
pub async fn load() -> Result<Option<Saved>, StoreError> {
    load_from(DEVICE_DB).await
}

/// Запомнить автокопию.
pub async fn save(saved: &Saved) -> Result<(), StoreError> {
    save_to(DEVICE_DB, saved).await
}

/// Забыть автокопию: папку и ключ. Файлы в папке остаются.
pub async fn forget() -> Result<(), StoreError> {
    forget_in(DEVICE_DB).await
}

pub(super) async fn load_from(name: &str) -> Result<Option<Saved>, StoreError> {
    let db = Open::new(name).await?;
    let tx = db.begin(IdbTransactionMode::Readonly)?;
    let request = store(&tx)?
        .get(&AUTOBACKUP.into())
        .map_err(|error| failure("get", Some(error)))?;
    let value = wait(request).await?;
    if value.is_undefined() {
        return Ok(None);
    }
    read(&value).map(Some)
}

pub(super) async fn save_to(name: &str, saved: &Saved) -> Result<(), StoreError> {
    let value = Object::new();
    let set = |field: &str, part: &JsValue| {
        Reflect::set(&value, &field.into(), part).map_err(|error| failure("put", Some(error)))
    };
    set("shape", &SHAPE.into())?;
    set("folder", &saved.folder)?;
    set("place", &saved.place.as_str().into())?;
    set("key", &saved.key)?;
    set("lock", &saved.lock.to_text().into())?;
    set("device", &saved.device.to_text().into())?;
    let db = Open::new(name).await?;
    let tx = db.begin(IdbTransactionMode::Readwrite)?;
    let done = Settle::new(tx.clone());
    // Запись новее программы не переписывается.
    let request = store(&tx)?
        .get(&AUTOBACKUP.into())
        .map_err(|error| failure("get", Some(error)))?;
    let current = wait(request).await?;
    if newer(&current) {
        let _ = tx.abort();
        return Err(StoreError::Newer);
    }
    if let Err(error) = store(&tx)?.put_with_key(&value, &AUTOBACKUP.into()) {
        let _ = tx.abort();
        return Err(failure("put", Some(error)));
    }
    finish(&tx, done).await
}

pub(super) async fn forget_in(name: &str) -> Result<(), StoreError> {
    let db = Open::new(name).await?;
    let tx = db.begin(IdbTransactionMode::Readwrite)?;
    let done = Settle::new(tx.clone());
    if let Err(error) = store(&tx)?.delete(&AUTOBACKUP.into()) {
        let _ = tx.abort();
        return Err(failure("delete", Some(error)));
    }
    finish(&tx, done).await
}

/// Запись автокопии из базы. Чего-то нет или оно не того вида — порча:
/// окно скажет, что папку надо выбрать снова.
fn read(value: &JsValue) -> Result<Saved, StoreError> {
    if newer(value) {
        return Err(StoreError::Newer);
    }
    let field = |name: &str| Reflect::get(value, &name.into()).ok();
    let damaged = |why: &str| {
        log("autobackup", &why.into());
        StoreError::Damaged
    };
    if field("shape").and_then(|shape| shape.as_f64()) != Some(f64::from(SHAPE)) {
        return Err(damaged("не та версия записи"));
    }
    let folder = field("folder")
        .and_then(|folder| folder.dyn_into::<FileSystemDirectoryHandle>().ok())
        .ok_or_else(|| damaged("нет папки"))?;
    let place = field("place")
        .and_then(|place| place.as_string())
        .ok_or_else(|| damaged("нет имени папки"))?;
    let key = field("key")
        .and_then(|key| key.dyn_into::<CryptoKey>().ok())
        .ok_or_else(|| damaged("нет ключа"))?;
    let password = |name: &str| {
        field(name)
            .and_then(|text| text.as_string())
            .and_then(|text| DeviceKey::from_text(&text))
    };
    let lock = password("lock").ok_or_else(|| damaged("нет замка"))?;
    let device = password("device").ok_or_else(|| damaged("нет пароля устройства"))?;
    Ok(Saved {
        folder,
        place,
        key,
        lock,
        device,
    })
}

/// Запись новее этой программы: её версия вида больше [`SHAPE`].
fn newer(value: &JsValue) -> bool {
    Reflect::get(value, &"shape".into())
        .ok()
        .and_then(|shape| shape.as_f64())
        .is_some_and(|shape| shape > f64::from(SHAPE))
}

/// Открытая база устройства — на одно действие; закрывается при сбросе.
struct Open {
    db: IdbDatabase,
    _on_versionchange: Closure<dyn FnMut()>,
}

impl Open {
    async fn new(name: &str) -> Result<Self, StoreError> {
        let factory = web_sys::window()
            .and_then(|window| window.indexed_db().ok().flatten())
            .ok_or(StoreError::Unavailable)?;
        let request = factory
            .open_with_u32(name, DEVICE_DB_VERSION)
            .map_err(|error| open_error("open device", Some(error)))?;
        let upgrading = request.clone();
        let on_upgrade = Closure::<dyn FnMut(IdbVersionChangeEvent)>::new(
            move |event: IdbVersionChangeEvent| {
                upgrade(&upgrading, event.old_version());
            },
        );
        request.set_onupgradeneeded(Some(on_upgrade.as_ref().unchecked_ref()));
        let opened = Settle::new(IdbRequest::from(request.clone())).await;
        request.set_onupgradeneeded(None);
        drop(on_upgrade);
        if opened.is_err() {
            return Err(open_error(
                "open device",
                request.error().ok().flatten().map(JsValue::from),
            ));
        }
        let db: IdbDatabase = request
            .result()
            .map_err(|error| open_error("open device", Some(error)))?
            .unchecked_into();
        // Новая версия этой базы в другой вкладке: соединение отпускается
        // сразу, а не по концу действия.
        let closing = db.clone();
        let on_versionchange = Closure::<dyn FnMut()>::new(move || closing.close());
        db.set_onversionchange(Some(on_versionchange.as_ref().unchecked_ref()));
        Ok(Self {
            db,
            _on_versionchange: on_versionchange,
        })
    }

    fn begin(&self, mode: IdbTransactionMode) -> Result<IdbTransaction, StoreError> {
        let opened = match mode {
            IdbTransactionMode::Readwrite => {
                let options = Object::new();
                Reflect::set(&options, &"durability".into(), &"strict".into())
                    .map_err(|error| failure("transaction", Some(error)))?;
                self.db
                    .unchecked_ref::<DatabaseOptions>()
                    .transaction(THINGS, mode, &options)
            }
            _ => self.db.transaction_with_str_and_mode(THINGS, mode),
        };
        opened.map_err(|error| failure("transaction", Some(error)))
    }
}

impl Drop for Open {
    fn drop(&mut self) {
        self.db.set_onversionchange(None);
        self.db.close();
    }
}

/// Обновление базы устройства до [`DEVICE_DB_VERSION`] — по шагу на версию.
fn upgrade(request: &IdbOpenDbRequest, old_version: f64) {
    let Ok(result) = request.result() else {
        return;
    };
    let db: IdbDatabase = result.unchecked_into();
    if old_version < 1.0
        && let Err(error) = db.create_object_store(THINGS)
    {
        log("upgrade", &error);
        if let Some(tx) = request.transaction() {
            let _ = tx.abort();
        }
    }
}

fn store(tx: &IdbTransaction) -> Result<IdbObjectStore, StoreError> {
    tx.object_store(THINGS)
        .map_err(|error| failure("objectStore", Some(error)))
}

/// Дождаться, пока браузер сохранит записанное: об успехе говорит только
/// `complete` транзакции (шапка `store/mod.rs`).
async fn finish(tx: &IdbTransaction, done: Settle<IdbTransaction>) -> Result<(), StoreError> {
    if done.await.is_err() {
        return Err(failure("commit", tx.error().map(JsValue::from)));
    }
    Ok(())
}
