//! Папка автокопии (задача 29): выбрать её, спросить разрешение на запись,
//! записать файл, перечислить свои файлы и убрать старые — File System
//! Access браузера. Что писать и что убирать, решает ядро
//! (`kvartik_core::format::autobackup`), где помнить папку — хранилище
//! (`store::autobackup`).
//!
//! Где это работает и как (разведка задачи 29, 2026-11-03: исходники
//! Chromium main bd4bc72f и тег 157.0.8103.42, browser-compat-data,
//! developer.chrome.com «Persistent permissions for the File System Access
//! API», спецификация WHATWG File System):
//!
//! - Выбор папки есть в Chrome и Edge на компьютере. В Firefox и Safari его
//!   нет, в Brave он выключен флагом. В Chrome на Android он есть с 132-й
//!   версии, но там разрешение снимается через 5 секунд после ухода с сайта,
//!   а запись не атомарна — там автокопии нет ([`available`]).
//! - Разрешение на запись живёт, пока открыта хоть одна вкладка сайта
//!   (и не дольше 16 часов в фоне). В новом сеансе [`permission`] отвечает
//!   «спросить», и писать можно только после нажатия человека ([`ask`]):
//!   тогда Chrome спрашивает с тремя ответами, и «при каждом посещении»
//!   оставляет разрешение и после перезапуска. Установленная программа
//!   этого сама не даёт: Chrome ищет её по адресу сайта, а она живёт
//!   в `/app/` (по исходникам, вживую не проверено).
//! - Сами «Документы», «Рабочий стол», «Загрузки», домашняя папка, корень
//!   системного диска и системные папки не выбираются — Chrome говорит это
//!   своим окном и даёт выбрать другую; папки внутри них — можно. Список
//!   меняется с версиями Chrome, и запись в папку, которую новая версия
//!   запретила, отвечает `SecurityError` ([`FolderError::Blocked`]).
//! - Запись идёт во временный `<имя>.crswap` рядом и подменяет файл только
//!   на `close` (`MoveFileEx` на Windows), но `getFileHandle` с `create`
//!   сразу создаёт пустой файл. Упал браузер — `.crswap` остаётся в папке
//!   навсегда: свои такие убирает окно (`format::autobackup::leftover`).
//!   Сброса на диск после `close` нет.
//!
//! Где автокопии нет, остаётся ручная копия.
//!
//! Привязки к выбору папки и к разрешениям — свои: в web-sys 0.3.107
//! `showDirectoryPicker`, `queryPermission` и `requestPermission` закрыты
//! флагом `--cfg=web_sys_unstable_apis` (`gen_Window.rs`,
//! `gen_FileSystemHandle.rs`, сверено 2026-11-03), а флаг открыл бы и всё
//! остальное нестабильное. Запись, перечисление и удаление — стабильные
//! привязки web-sys.

use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use js_sys::{AsyncIterator, IteratorNext, Object, Promise, Reflect, Uint8Array};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{
    DomException, FileSystemDirectoryHandle, FileSystemFileHandle, FileSystemGetDirectoryOptions,
    FileSystemGetFileOptions, FileSystemHandle, FileSystemHandleKind, FileSystemWritableFileStream,
};

#[wasm_bindgen]
extern "C" {
    /// `window` с выбором папки.
    #[wasm_bindgen(extends = web_sys::Window)]
    type Picking;

    /// `window.showDirectoryPicker(параметры)`.
    #[wasm_bindgen(method, catch, js_name = showDirectoryPicker)]
    fn show_directory_picker(this: &Picking, options: &Object) -> Result<Promise, JsValue>;

    /// Папка или файл с вопросом о разрешении.
    #[wasm_bindgen(extends = FileSystemHandle)]
    type Permitting;

    /// `handle.queryPermission({mode})`.
    #[wasm_bindgen(method, catch, js_name = queryPermission)]
    fn query_permission(this: &Permitting, descriptor: &Object) -> Result<Promise, JsValue>;

    /// `handle.requestPermission({mode})`.
    #[wasm_bindgen(method, catch, js_name = requestPermission)]
    fn request_permission(this: &Permitting, descriptor: &Object) -> Result<Promise, JsValue>;

    /// `navigator` с блокировками Web Locks.
    #[wasm_bindgen(extends = web_sys::Navigator)]
    type Locking;

    /// `navigator.locks`; нет их — `undefined`.
    #[wasm_bindgen(method, getter, js_name = locks)]
    fn lock_manager(this: &Locking) -> JsValue;

    /// `LockManager`.
    type Locks;

    /// `locks.request(имя, обработчик)`: обработчик зовётся, когда
    /// блокировка его, и держит её, пока не исполнится его обещание.
    #[wasm_bindgen(method, catch)]
    fn request(this: &Locks, name: &str, callback: &js_sys::Function) -> Result<Promise, JsValue>;
}

/// Сделать `work` одной вкладкой за раз — под блокировкой Web Locks `name`,
/// общей у всех вкладок сайта. Автокопию две вкладки пишут в один файл дня,
/// и без блокировки в файле мог остаться снимок той, что читала базу
/// раньше, а моментом последней автокопии — того, что читала позже: правка
/// между ними не попала бы никуда (ревью задачи 29). Блокировки есть
/// в Chromium с 69-й версии (по браузеру: MDN, «Web Locks API», сверено
/// 2026-11-03); нет их — работа идёт без неё. `None` — работа взята
/// браузером, но не закончилась: так не бывает, а паниковать ради этого
/// нельзя.
pub async fn alone<T: 'static>(name: &str, work: impl Future<Output = T> + 'static) -> Option<T> {
    type Work<T> = Pin<Box<dyn Future<Output = T>>>;
    let work: Rc<RefCell<Option<Work<T>>>> = Rc::new(RefCell::new(Some(Box::pin(work))));
    let done: Rc<RefCell<Option<T>>> = Rc::new(RefCell::new(None));
    let locks = web_sys::window()
        .map(|window| window.navigator().unchecked_into::<Locking>().lock_manager())
        .filter(|locks| !locks.is_undefined() && !locks.is_null());
    if let Some(locks) = locks {
        let (taken, filled) = (Rc::clone(&work), Rc::clone(&done));
        let callback = Closure::once_into_js(move |_lock: JsValue| {
            let work = taken.borrow_mut().take();
            js_sys::futures::future_to_promise(async move {
                if let Some(work) = work {
                    let value = work.await;
                    *filled.borrow_mut() = Some(value);
                }
                Ok(JsValue::UNDEFINED)
            })
        });
        match locks.unchecked_into::<Locks>().request(name, callback.unchecked_ref()) {
            Ok(promise) => {
                if let Err(error) = promise.await {
                    log("locks.request", &error);
                }
            }
            Err(error) => log("locks.request", &error),
        }
    }
    if let Some(value) = done.borrow_mut().take() {
        return Some(value);
    }
    let left = work.borrow_mut().take();
    match left {
        Some(work) => Some(work.await),
        None => None,
    }
}

/// Звать `handler`, когда страница уходит с глаз: вкладку сменили, окно
/// свернули или закрывают. Слушатель — на всё время страницы. Свойство
/// `onvisibilitychange` занято проверкой новой версии (`platform::pwa`),
/// поэтому — `addEventListener`.
pub fn when_hidden(handler: impl Fn() + 'static) {
    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        return;
    };
    let watched = document.clone();
    let listener = Closure::<dyn Fn()>::new(move || {
        if watched.visibility_state() == web_sys::VisibilityState::Hidden {
            handler();
        }
    });
    if document
        .add_event_listener_with_callback("visibilitychange", listener.as_ref().unchecked_ref())
        .is_ok()
    {
        listener.forget();
    }
}

/// Почему папка не выбрана.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PickError {
    /// Человек закрыл выбор, ничего не выбрав: говорить не о чем.
    Cancelled,
    /// Браузер отказал. Подробность — в консоли.
    Failed,
}

/// Разрешение на запись в папку.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Permission {
    /// Писать можно.
    Granted,
    /// Браузер спросит человека: писать можно только после его ответа,
    /// а спросить — только от его нажатия.
    Prompt,
    /// Человек запретил.
    Denied,
}

/// Почему файл не записан или не убран.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FolderError {
    /// Разрешения на запись больше нет: спросить человека снова.
    NotAllowed,
    /// Браузер не даёт писать в эту папку вовсе (`SecurityError`): её
    /// запретила новая версия Chrome или защита папок Windows. Спрашивать
    /// бесполезно — нужна другая папка.
    Blocked,
    /// Папки больше нет: её удалили, переименовали или диск отключён.
    Gone,
    /// На диске нет места.
    Full,
    /// Прочее. Подробность — в консоли.
    Failed,
}

/// Есть ли здесь автокопия: выбор папки есть, и это не Android (шапка
/// модуля).
pub fn available() -> bool {
    let picker = web_sys::window()
        .is_some_and(|window| Reflect::has(&window, &"showDirectoryPicker".into()).unwrap_or(false));
    picker && !super::pwa::android()
}

/// Описание разрешения: на чтение и запись — без записи копии не сделать,
/// а без чтения не увидеть старых файлов.
fn readwrite() -> Object {
    let descriptor = Object::new();
    let _ = Reflect::set(&descriptor, &"mode".into(), &"readwrite".into());
    descriptor
}

/// Открыть выбор папки.
///
/// Зовётся прямо в обработчике нажатия, до первого `await`: выбор
/// открывается только от свежего нажатия человека. `id` — браузер помнит
/// для него последнюю выбранную папку и открывает выбор на ней;
/// `startIn` — «Документы», если такой памяти ещё нет.
pub fn pick() -> impl Future<Output = Result<FileSystemDirectoryHandle, PickError>> + 'static {
    pick_with(readwrite())
}

/// Открыть выбор папки, чтобы восстановить из неё автокопию (задача 124,
/// п. 20): только чтение — браузер спрашивает «просмотреть файлы», а не
/// «править». Выбор открывается на папке автокопии: `id` тот же. Зовётся,
/// как [`pick`], в самом обработчике нажатия.
pub fn pick_to_read() -> impl Future<Output = Result<FileSystemDirectoryHandle, PickError>> + 'static {
    let options = Object::new();
    let _ = Reflect::set(&options, &"mode".into(), &"read".into());
    pick_with(options)
}

fn pick_with(options: Object) -> impl Future<Output = Result<FileSystemDirectoryHandle, PickError>> + 'static {
    let _ = Reflect::set(&options, &"id".into(), &"kvartik-autobackup".into());
    let _ = Reflect::set(&options, &"startIn".into(), &"documents".into());
    let started = web_sys::window()
        .ok_or(PickError::Failed)
        .and_then(|window| {
            window
                .unchecked_into::<Picking>()
                .show_directory_picker(&options)
                .map_err(|error| pick_failure(&error))
        });
    async move {
        let chosen = started?.await.map_err(|error| pick_failure(&error))?;
        chosen.dyn_into().map_err(|_| PickError::Failed)
    }
}

fn pick_failure(error: &JsValue) -> PickError {
    if name(error).as_deref() == Some("AbortError") {
        PickError::Cancelled
    } else {
        log("showDirectoryPicker", error);
        PickError::Failed
    }
}

/// Что разрешено с папкой сейчас — без вопроса человеку.
pub async fn permission(folder: &FileSystemDirectoryHandle) -> Permission {
    let started = folder.unchecked_ref::<Permitting>().query_permission(&readwrite());
    answer(started.map_err(|error| log("queryPermission", &error)).ok()).await
}

/// Спросить у человека разрешение на запись в папку.
///
/// Зовётся прямо в обработчике нажатия, до первого `await`: вопрос
/// задаётся только от свежего нажатия человека.
pub fn ask(folder: &FileSystemDirectoryHandle) -> impl Future<Output = Permission> + use<> {
    let started = folder.unchecked_ref::<Permitting>().request_permission(&readwrite());
    answer(started.map_err(|error| log("requestPermission", &error)).ok())
}

async fn answer(started: Option<Promise>) -> Permission {
    let Some(promise) = started else {
        return Permission::Denied;
    };
    match promise.await.map(|state| state.as_string()) {
        Ok(Some(state)) if state == "granted" => Permission::Granted,
        Ok(Some(state)) if state == "prompt" => Permission::Prompt,
        Ok(_) => Permission::Denied,
        Err(error) => {
            log("permission", &error);
            Permission::Denied
        }
    }
}

/// Папка `name` внутри `folder`; нет — создаётся. Автокопия держит только её,
/// а не выбранную: та может быть корнем облака человека, а ручка, лежащая
/// в базе устройства, открыла бы любому коду сайта всё, что под ней
/// (ревью задачи 29, Правило 13).
pub async fn subfolder(folder: &FileSystemDirectoryHandle, name: &str) -> Result<FileSystemDirectoryHandle, FolderError> {
    let options = FileSystemGetDirectoryOptions::new();
    options.set_create(true);
    settle("getDirectoryHandle", folder.get_directory_handle_with_options(name, &options)).await
}

/// Записать файл `name` в папку целиком.
///
/// Браузер пишет во временный файл рядом и подменяет им прежний только на
/// `close`: оборванная запись оставляет прежний файл целым. Не удалось —
/// временный файл снимается (`abort`). Пишется одним `write` и сразу
/// `close`: Chrome закрывает временный файл после каждого `write`, и между
/// ними его успевает занять синхронизатор облака или антивирус (Chromium
/// issue 40729876, открыт).
pub async fn write(folder: &FileSystemDirectoryHandle, name: &str, bytes: &[u8]) -> Result<(), FolderError> {
    let options = FileSystemGetFileOptions::new();
    options.set_create(true);
    let file: FileSystemFileHandle = settle("getFileHandle", folder.get_file_handle_with_options(name, &options)).await?;
    let stream: FileSystemWritableFileStream = settle("createWritable", file.create_writable()).await?;
    let written = match stream.write_with_js_u8_array(&Uint8Array::from(bytes)) {
        Ok(promise) => settle::<JsValue>("write", promise).await.map(drop),
        Err(error) => Err(failure("write", &error)),
    };
    match written {
        Ok(()) => settle::<JsValue>("close", stream.close()).await.map(drop),
        Err(problem) => {
            let _ = stream.abort().await;
            Err(problem)
        }
    }
}

/// Имена файлов папки, которые начинаются с `prefix`. Остальные не
/// собираются: папкой может быть и корень облака с тысячами файлов.
pub async fn names(folder: &FileSystemDirectoryHandle, prefix: &str) -> Result<Vec<String>, FolderError> {
    let entries: AsyncIterator = folder.values();
    let mut names = Vec::new();
    loop {
        let next = entries.next().map_err(|error| failure("values", &error))?;
        // Шаг итератора — простой объект `{done, value}`, а не экземпляр
        // класса: `dyn_into` с его `instanceof` его не узнал бы.
        let step: IteratorNext = next
            .await
            .map_err(|error| failure("values", &error))?
            .unchecked_into();
        if step.done() {
            return Ok(names);
        }
        let Ok(entry) = step.value().dyn_into::<FileSystemHandle>() else {
            continue;
        };
        if entry.kind() == FileSystemHandleKind::File {
            let name = entry.name();
            if name.starts_with(prefix) {
                names.push(name);
            }
        }
    }
}

/// Папка `name` внутри `folder`, если она есть: не создаётся — восстановление
/// только читает (задача 124, п. 20). Нет её или так назван файл — `None`.
///
/// Вызовы — те же, что у записи, с параметрами без `create`, а `getFile`
/// и `lastModified` ([`modified`]) — через `Reflect`: своя привязка `web-sys`
/// прибавила бы импорт главному wasm, хотя зовёт её только часть копии
/// (Правило 6, «Импорт JS — в главном wasm»).
pub async fn existing_subfolder(folder: &FileSystemDirectoryHandle, child: &str) -> Result<Option<FileSystemDirectoryHandle>, FolderError> {
    match folder.get_directory_handle_with_options(child, &FileSystemGetDirectoryOptions::new()).await {
        Ok(found) => found.dyn_into().map(Some).map_err(|value| failure("getDirectoryHandle", &value)),
        Err(error) if matches!(name(&error).as_deref(), Some("NotFoundError" | "TypeMismatchError")) => Ok(None),
        Err(error) => Err(failure("getDirectoryHandle", &error)),
    }
}

/// Файлы папки, чьё имя начинается с `prefix`, — `File` браузера: имя
/// и момент изменения сразу, а байты — когда их прочитают, по одному
/// (`platform::file::read`).
pub async fn files(folder: &FileSystemDirectoryHandle, prefix: &str) -> Result<Vec<web_sys::File>, FolderError> {
    let options = FileSystemGetFileOptions::new();
    let mut found = Vec::new();
    for file in names(folder, prefix).await? {
        let handle: FileSystemFileHandle = settle("getFileHandle", folder.get_file_handle_with_options(&file, &options)).await?;
        let get = Reflect::get(&handle, &"getFile".into()).map_err(|error| failure("getFile", &error))?;
        let started = get
            .unchecked_into::<js_sys::Function>()
            .call0(&handle)
            .map_err(|error| failure("getFile", &error))?;
        let file = started
            .unchecked_into::<Promise>()
            .await
            .map_err(|error| failure("getFile", &error))?;
        // `getFile` отдаёт `File`: проверка `instanceof` стоила бы импорта.
        found.push(file.unchecked_into());
    }
    Ok(found)
}

/// Момент изменения файла в миллисекундах; не прочитан — ноль.
pub fn modified(file: &web_sys::File) -> i64 {
    Reflect::get(file, &"lastModified".into())
        .ok()
        .and_then(|value| value.as_f64())
        .map_or(0, |millis| millis as i64)
}

/// Убрать файл `file` из папки. Его уже нет — тоже хорошо, и не повод для
/// записи в консоль.
pub async fn remove(folder: &FileSystemDirectoryHandle, file: &str) -> Result<(), FolderError> {
    match folder.remove_entry(file).await {
        Ok(_) => Ok(()),
        Err(error) if name(&error).as_deref() == Some("NotFoundError") => Ok(()),
        Err(error) => Err(failure("removeEntry", &error)),
    }
}

/// Дождаться обещания и взять его значение нужного типа.
async fn settle<T: JsCast>(context: &str, promise: Promise) -> Result<T, FolderError> {
    let value = promise.await.map_err(|error| failure(context, &error))?;
    value.dyn_into().map_err(|value| failure(context, &value))
}

/// Ответ браузера — словом; подробность — в консоль для разработчика
/// (наружу ничего не уходит, Правило 13).
fn failure(context: &str, error: &JsValue) -> FolderError {
    log(context, error);
    match name(error).as_deref() {
        Some("NotAllowedError") => FolderError::NotAllowed,
        Some("SecurityError") => FolderError::Blocked,
        Some("NotFoundError") => FolderError::Gone,
        Some("QuotaExceededError") => FolderError::Full,
        _ => FolderError::Failed,
    }
}

fn name(error: &JsValue) -> Option<String> {
    error.dyn_ref::<DomException>().map(DomException::name)
}

fn log(context: &str, error: &JsValue) {
    web_sys::console::error_2(&format!("Kvartik, папка автокопии ({context}):").into(), error);
}
