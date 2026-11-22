//! Хранилище на устройстве: IndexedDB и просьба о постоянном хранении.
//!
//! Сервера нет, и база в браузере — единственное место, где живут данные
//! человека («Архитектура» в CLAUDE.md). Здесь — только хранение: что
//! записать, решают команды ядра, как записать — `kvartik_core::format`
//! (таблица слоёв). Обёртка своя, на `web-sys`: она легче `idb`
//! и `indexed_db_futures`, отдаёт причину отката, которую те теряют, и держит
//! правило записи ниже (почему не они — «Общее» в CLAUDE.md, замер —
//! docs/measurements.md, 0.0.7).
//!
//! # Одно действие — одна транзакция
//!
//! Действие человека — одна транзакция `readwrite`: прочитать
//! ([`Tx::get`], [`Tx::all`], [`Tx::list`]), посчитать (ядро, без ожидания), записать
//! ([`Tx::commit`]). Транзакция IndexedDB завершается сама, как только ей
//! нечего ждать (Правило 6), поэтому внутри неё можно ждать только её же
//! запросов: продолжение после их `await` выполняется в микрозадаче сразу
//! за событием запроса, пока транзакция ещё активна (`js-sys` будит задачу
//! через `queueMicrotask`). `await` чужого будущего — таймера, сети, другой
//! транзакции — и транзакция уже зафиксирована.
//!
//! От этой ошибки защищает устройство [`Tx::commit`]: все записи действия
//! ставятся разом, без ожидания между ними. Если транзакция успела
//! завершиться, первый же запрос `commit` получает
//! `TransactionInactiveError`, и не записано ничего — не половина действия.
//!
//! Об успехе говорит только `complete` транзакции, а не ответ на запись:
//! Chrome сообщает о нехватке места (`QuotaExceededError`) лишь при
//! фиксации, когда каждая запись уже ответила «успешно» (разведка задачи 6,
//! 2026-10-12, по исходникам Chromium `transaction.cc`).
//!
//! # Запись — до диска
//!
//! Транзакция записи открывается с `durability: "strict"`: `complete`
//! приходит, только когда данные легли на диск. По умолчанию Chrome
//! с 121–124-й версии фиксирует без сброса на диск, и правки последних
//! секунд пропадают при отключении питания, а «relaxed» в Firefox — это
//! `synchronous=OFF` с риском испортить базу (разведка задачи 6,
//! 2026-10-12). Сброс стоит миллисекунды на действие человека, а данные
//! важнее: с сохранностью Правило 1 не спорит. Браузер, который параметра
//! не знает, его пропускает: незнакомое поле словаря WebIDL не ошибка.
//!
//! # База
//!
//! База записей — одна на ИП: первого — [`DB_NAME`], следующих — свои
//! (задача 38, [`Store::open_base`]); в каждой одно хранилище объектов —
//! `records`. Список ИП — в базе конторы ([`office::OFFICE_DB`]) того же
//! устройства: она устроена так же и идёт только в копию всех ИП.
//! Рядом — база устройства для автокопии ([`autobackup::DEVICE_DB`],
//! задача 29): в ней не записи, а объекты браузера — папка и ключ, — и в
//! копию она не идёт.
//! Ключ записи — массив `[вид, идентификатор]`, значение — строка JSON из
//! `format::encode` с версией формата внутри. Новый вид записи базу не
//! меняет: вид — часть ключа. Версия базы ([`DB_VERSION`]) растёт, только
//! когда меняются хранилища объектов или индексы, — вместе с шагом в
//! `upgrade` и тестом (Правило 5). Это не версия формата и не версия
//! программы.
//!
//! Старая вкладка рядом с новой (Правило 6): при новой версии базы старая
//! получает `versionchange`, закрывает соединение и просит перезагрузиться
//! ([`StoreError::Replaced`]); при новой версии формата — отказывается
//! читать и переписывать записи новее себя ([`StoreError::Newer`]).

pub mod autobackup;
pub mod backup;
pub mod device;
pub mod office;
mod settle;
pub mod sync;

use std::cell::Cell;
use std::marker::PhantomData;
use std::rc::Rc;

use js_sys::{Array, Object, Promise, Reflect};
use kvartik_core::format::backup::{Entry, Raw};
use kvartik_core::format::{self, FORMAT_VERSION, FormatError, Stored};
use kvartik_core::model::record::RecordId;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{
    DomException, IdbDatabase, IdbKeyRange, IdbObjectStore, IdbOpenDbRequest, IdbRequest,
    IdbTransaction, IdbTransactionMode, IdbVersionChangeEvent, StorageManager,
};

use settle::Settle;

/// Имя базы — первого ИП (задача 38: у следующих свои, `model::office`).
/// Данные привязаны к нему и к адресу сайта навсегда («Происхождение
/// сайта» в CLAUDE.md): сменить его — значит оставить данные всех
/// пользователей в старой базе.
pub const DB_NAME: &str = kvartik_core::model::office::FIRST_BASE;

/// Версия базы: хранилища объектов и индексы. Растёт вместе с шагом
/// в [`upgrade`] и тестом (Правило 5).
pub const DB_VERSION: u32 = 1;

/// Хранилище объектов со всеми записями.
const RECORDS: &str = "records";

/// Что пошло не так — так, как это нужно сказать человеку. Слова — у окна,
/// подробность для разработчика — в консоли браузера.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StoreError {
    /// Хранилища нет или браузер не дал его открыть: сайтам запрещено
    /// хранить данные, приватное окно у старых браузеров (Firefox до 115).
    Unavailable,
    /// База или запись — от более новой версии программы (Правило 2).
    Newer,
    /// Другая вкладка обновила базу, и эта закрыла соединение
    /// (`versionchange`).
    Replaced,
    /// Браузер сам закрыл базу (`close`): данные сайта стёрли или у Safari
    /// упал процесс хранилища — тогда данные целы (Правило 6).
    Lost,
    /// Не хватило места (`QuotaExceededError`).
    Full,
    /// Запись в базе не читается.
    Damaged,
    /// Прочее.
    Failed,
}

/// Что хранилище сообщает окну само, без вопроса.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Event {
    /// Новой версии базы мешает вкладка со старой (`blocked`): открытие ждёт,
    /// пока та закроется, и завершится само.
    Blocked,
    /// Соединение закрыто — [`StoreError::Replaced`] или
    /// [`StoreError::Lost`]. Дальше хранилище только отказывает: нужна
    /// перезагрузка страницы.
    Closed(StoreError),
    /// Браузер ответил на просьбу о постоянном хранении.
    Persistence(Persistence),
    /// Действие записано в базу (`complete` транзакции): данные изменились,
    /// и автокопии (задача 29) есть что писать. Одна точка на все действия —
    /// [`Tx::commit`], поэтому ни одно не забыто.
    Written,
}

/// Сотрёт ли браузер данные сам (Правило 6).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Persistence {
    /// Постоянное хранение: сам браузер данные не сотрёт.
    Persistent,
    /// Браузер может стереть данные сам — частый ответ, а не ошибка.
    BestEffort,
    /// Браузер не говорит: `navigator.storage` нет (страница не по HTTPS)
    /// или он не ответил.
    Unknown,
}

/// Открытая база.
///
/// Держать, пока окно живо: брошенное соединение браузер может закрыть,
/// а вместе с ним пропадут и обработчики `versionchange` и `close`.
pub struct Store {
    db: IdbDatabase,
    /// Имя базы: по нему выпуск знает, у какого ИП он выключен (задача 116).
    name: String,
    shared: Rc<Shared>,
    _on_versionchange: Closure<dyn FnMut()>,
    _on_close: Closure<dyn FnMut()>,
}

/// Общее у базы и её транзакций.
struct Shared {
    on_event: Box<dyn Fn(Event)>,
    /// Почему соединение закрыто; `None` — открыто.
    closed: Cell<Option<StoreError>>,
    /// Просили ли в этой сессии постоянное хранение.
    asked_persistence: Cell<bool>,
    /// Сколько действий сейчас записывается ([`Tx::commit`]): переход
    /// к другому ИП перезагружает страницу (задача 38), а перезагрузка
    /// посреди записи отменила бы действие.
    writing: Cell<usize>,
}

/// Отметка «действие записывается» на время [`Tx::commit`]: снимается
/// и на любом раннем выходе.
struct Writing(Rc<Shared>);

impl Writing {
    fn start(shared: &Rc<Shared>) -> Self {
        shared.writing.set(shared.writing.get() + 1);
        Self(Rc::clone(shared))
    }
}

impl Drop for Writing {
    fn drop(&mut self) {
        self.0.writing.set(self.0.writing.get().saturating_sub(1));
    }
}

impl Shared {
    fn close(&self, why: StoreError) {
        if self.closed.get().is_none() {
            self.closed.set(Some(why));
            (self.on_event)(Event::Closed(why));
        }
    }

    /// Первое сохранение в сессии — повод попросить постоянное хранение
    /// (задача 6). Не раньше: Firefox на эту просьбу спрашивает человека,
    /// а вопрос до первых данных ему непонятен. Ответ приходит событием:
    /// запрос у человека может висеть сколько угодно, и запись его не ждёт.
    fn after_write(self: &Rc<Self>) {
        (self.on_event)(Event::Written);
        if self.asked_persistence.replace(true) {
            return;
        }
        let shared = Rc::clone(self);
        js_sys::futures::spawn_local(async move {
            let answer = ask(StorageManager::persist).await;
            (shared.on_event)(Event::Persistence(answer));
        });
    }
}

impl Store {
    /// Открыть базу первого ИП — для браузерных тестов: окно открывает
    /// базу, которую выбрала контора ([`Store::open_base`]).
    #[cfg(all(test, target_arch = "wasm32"))]
    pub async fn open(on_event: impl Fn(Event) + 'static) -> Result<Self, StoreError> {
        Self::open_base(DB_NAME, on_event).await
    }

    /// Открыть базу ИП по имени (задача 38): первого — [`DB_NAME`] —
    /// или следующего (`model::office`). `on_event` получает то, о чём
    /// хранилище сообщает само: ожидание старой вкладки, закрытие
    /// соединения, ответ на просьбу о постоянном хранении. Имя проверяется
    /// до открытия:
    /// `indexedDB.open` молча создаёт пустую базу, и мусор в имени стал бы
    /// пустой программой (Правило 6).
    pub async fn open_base(name: &str, on_event: impl Fn(Event) + 'static) -> Result<Self, StoreError> {
        if !kvartik_core::model::office::is_base_name(name) {
            log("open", &format!("не имя базы ИП: {name}").into());
            return Err(StoreError::Failed);
        }
        Self::open_named(name, DB_VERSION, Box::new(on_event)).await
    }

    /// Открыть базу с этим именем и версией: программа открывает свою,
    /// а браузерные тесты — свои, отдельные для каждого теста.
    async fn open_named(
        name: &str,
        version: u32,
        on_event: Box<dyn Fn(Event)>,
    ) -> Result<Self, StoreError> {
        let shared = Rc::new(Shared {
            on_event,
            closed: Cell::new(None),
            asked_persistence: Cell::new(false),
            writing: Cell::new(0),
        });
        let factory = web_sys::window()
            .and_then(|window| window.indexed_db().ok().flatten())
            .ok_or(StoreError::Unavailable)?;
        let request = factory
            .open_with_u32(name, version)
            .map_err(|error| open_error("open", Some(error)))?;

        let upgrading = request.clone();
        let on_upgrade = Closure::<dyn FnMut(IdbVersionChangeEvent)>::new(
            move |event: IdbVersionChangeEvent| {
                upgrade(&upgrading, event.old_version());
            },
        );
        request.set_onupgradeneeded(Some(on_upgrade.as_ref().unchecked_ref()));
        let waiting = Rc::clone(&shared);
        let on_blocked = Closure::<dyn FnMut()>::new(move || (waiting.on_event)(Event::Blocked));
        request.set_onblocked(Some(on_blocked.as_ref().unchecked_ref()));

        let opened = Settle::new(IdbRequest::from(request.clone())).await;
        request.set_onupgradeneeded(None);
        request.set_onblocked(None);
        drop((on_upgrade, on_blocked));
        if opened.is_err() {
            return Err(open_error(
                "open",
                request.error().ok().flatten().map(JsValue::from),
            ));
        }
        let db: IdbDatabase = request
            .result()
            .map_err(|error| open_error("open", Some(error)))?
            .unchecked_into();

        let closing = db.clone();
        let replaced = Rc::clone(&shared);
        let on_versionchange = Closure::<dyn FnMut()>::new(move || {
            closing.close();
            replaced.close(StoreError::Replaced);
        });
        db.set_onversionchange(Some(on_versionchange.as_ref().unchecked_ref()));
        let lost = Rc::clone(&shared);
        let on_close = Closure::<dyn FnMut()>::new(move || lost.close(StoreError::Lost));
        db.set_onclose(Some(on_close.as_ref().unchecked_ref()));

        Ok(Self {
            db,
            name: name.to_owned(),
            shared,
            _on_versionchange: on_versionchange,
            _on_close: on_close,
        })
    }

    /// Имя базы — без `IDBDatabase.name`: свой импорт JS лёг бы в главный
    /// wasm (CLAUDE.md, Правило 6).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Записывается ли сейчас какое-нибудь действие.
    pub fn writing(&self) -> bool {
        self.shared.writing.get() > 0
    }

    /// Транзакция только на чтение.
    pub fn read(&self) -> Result<Tx<Read>, StoreError> {
        self.begin(|db| db.transaction_with_str_and_mode(RECORDS, IdbTransactionMode::Readonly))
    }

    /// Транзакция одного действия: прочитать, посчитать, записать. Данные —
    /// до диска (`durability: "strict"`, шапка модуля).
    pub fn write(&self) -> Result<Tx<Write>, StoreError> {
        self.begin(|db| {
            let options = Object::new();
            Reflect::set(&options, &"durability".into(), &"strict".into())?;
            db.unchecked_ref::<DatabaseOptions>().transaction(
                RECORDS,
                IdbTransactionMode::Readwrite,
                &options,
            )
        })
    }

    fn begin<M>(
        &self,
        open: impl FnOnce(&IdbDatabase) -> Result<IdbTransaction, JsValue>,
    ) -> Result<Tx<M>, StoreError> {
        if let Some(why) = self.shared.closed.get() {
            return Err(why);
        }
        let tx = open(&self.db).map_err(|error| failure("transaction", Some(error)))?;
        let records = tx
            .object_store(RECORDS)
            .map_err(|error| failure("objectStore", Some(error)))?;
        Ok(Tx {
            tx,
            records,
            shared: Rc::clone(&self.shared),
            mode: PhantomData,
        })
    }
}

#[wasm_bindgen]
extern "C" {
    /// Та же `IDBDatabase` — ради одного вызова, которого нет в стабильном
    /// `web-sys`: `transaction(имена, режим, параметры)` там закрыт флагом
    /// `web_sys_unstable_apis` (по исходникам web-sys 0.3.107,
    /// `gen_IdbDatabase.rs`), а нужен он ради `durability`.
    #[wasm_bindgen(js_name = IDBDatabase)]
    type DatabaseOptions;

    #[wasm_bindgen(method, catch, js_name = transaction)]
    fn transaction(
        this: &DatabaseOptions,
        store_names: &str,
        mode: IdbTransactionMode,
        options: &Object,
    ) -> Result<IdbTransaction, JsValue>;
}

/// Закрывает соединение и снимает обработчики: замыкания уходят вместе
/// с `Store`, и браузер не должен звать их после этого.
impl Drop for Store {
    fn drop(&mut self) {
        self.db.set_onversionchange(None);
        self.db.set_onclose(None);
        self.db.close();
    }
}

/// Обновление базы до [`DB_VERSION`]: по шагу на каждую версию, начиная
/// с той, что была. Идёт в транзакции обновления, пока у остальных вкладок
/// база закрыта. Шаг не удался — транзакция обновления отменяется, и база
/// остаётся прежней.
fn upgrade(request: &IdbOpenDbRequest, old_version: f64) {
    let Ok(result) = request.result() else {
        return;
    };
    let db: IdbDatabase = result.unchecked_into();
    if old_version < 1.0
        && let Err(error) = db.create_object_store(RECORDS)
    {
        log("upgrade", &error);
        if let Some(tx) = request.transaction() {
            let _ = tx.abort();
        }
    }
}

/// Режим транзакции: только чтение.
pub struct Read;

/// Режим транзакции: действие с записью.
pub struct Write;

/// Транзакция над записями.
pub struct Tx<M> {
    tx: IdbTransaction,
    records: IdbObjectStore,
    shared: Rc<Shared>,
    mode: PhantomData<M>,
}

impl<M> Tx<M> {
    /// Запись вида `T` с этим идентификатором; `None` — такой нет.
    pub async fn get<T: Stored>(&self, id: RecordId) -> Result<Option<T>, StoreError> {
        let request = self
            .records
            .get(&key(T::KIND, id))
            .map_err(|error| failure("get", Some(error)))?;
        let value = wait(request).await?;
        if value.is_undefined() {
            return Ok(None);
        }
        decode(&value).map(Some)
    }

    /// Все записи вида `T`, включая помеченные удалёнными. Хоть одна не
    /// читается — отказ целиком ([`StoreError::Damaged`]): так читается то,
    /// что окно показывает одной записью, — профиль.
    pub async fn all<T: Stored>(&self) -> Result<Vec<T>, StoreError> {
        self.values(T::KIND)
            .await?
            .iter()
            .map(|value| decode(&value))
            .collect()
    }

    /// Все записи вида `T` для списка — клиенты, документы, — включая
    /// помеченные удалёнными. Нечитаемая запись не прячет остальные:
    /// пропускается и считается в [`Listed::unreadable`] (задача 10). Иначе
    /// одна испорченная запись оставила бы человека без всех клиентов, хотя
    /// остальные целы. Запись новее программы — отказ целиком
    /// ([`StoreError::Newer`]): список без неё был бы неполным молча, а нужна
    /// перезагрузка — новой версией (Правило 2).
    pub async fn list<T: Stored>(&self) -> Result<Listed<T>, StoreError> {
        let mut listed = Listed {
            records: Vec::new(),
            unreadable: 0,
        };
        for value in self.values(T::KIND).await?.iter() {
            match decode(&value) {
                Ok(record) => listed.records.push(record),
                Err(StoreError::Damaged) => listed.unreadable += 1,
                Err(error) => return Err(error),
            }
        }
        Ok(listed)
    }

    /// Значения всех записей вида `kind` — строки JSON, как они лежат.
    async fn values(&self, kind: &str) -> Result<Array, StoreError> {
        let range = IdbKeyRange::bound(
            &Array::of1(&kind.into()),
            &Array::of2(&kind.into(), &Array::new()),
        )
        .map_err(|error| failure("bound", Some(error)))?;
        let request = self
            .records
            .get_all_with_key(&range)
            .map_err(|error| failure("getAll", Some(error)))?;
        wait(request)
            .await?
            .dyn_into()
            .map_err(|_| StoreError::Damaged)
    }

    /// Все записи базы как они лежат, любого вида, — для файла копии
    /// (`backup`). Разбирает их ядро: хранилище не знает, какие виды бывают.
    pub async fn dump(&self) -> Result<Vec<Dumped>, StoreError> {
        let keys = self
            .records
            .get_all_keys()
            .map_err(|error| failure("getAllKeys", Some(error)))?;
        let values = self
            .records
            .get_all()
            .map_err(|error| failure("getAll", Some(error)))?;
        // Оба запроса ждём с самого начала: второй может ответить, пока ждут
        // первый (`Settle`). Ключи и значения приходят в одном порядке —
        // по ключу, — а между ними база не меняется: оба в одной транзакции.
        let (keys, values) = (Settle::new(keys), Settle::new(values));
        let keys = settled(keys, "getAllKeys").await?;
        let values = settled(values, "getAll").await?;
        let (Ok(keys), Ok(values)) = (keys.dyn_into::<Array>(), values.dyn_into::<Array>()) else {
            return Err(StoreError::Damaged);
        };
        if keys.length() != values.length() {
            return Err(StoreError::Damaged);
        }
        Ok(keys
            .iter()
            .zip(values.iter())
            .map(|(key, value)| {
                let part = |at| Reflect::get_u32(&key, at).ok().and_then(|v| v.as_string());
                Dumped {
                    kind: part(0).unwrap_or_default(),
                    id: part(1).unwrap_or_default(),
                    text: value.as_string(),
                }
            })
            .collect())
    }

    /// Есть ли в базе хоть одна запись, включая удалённые.
    pub async fn is_empty(&self) -> Result<bool, StoreError> {
        let request = self
            .records
            .count()
            .map_err(|error| failure("count", Some(error)))?;
        Ok(wait(request)
            .await?
            .as_f64()
            .is_none_or(|count| count < 1.0))
    }
}

/// Записи одного вида для списка: прочитанные и число нечитаемых.
#[derive(Debug)]
pub struct Listed<T> {
    /// Прочитанные записи, включая помеченные удалёнными.
    pub records: Vec<T>,
    /// Сколько записей вида не читается. Человеку окно говорит число,
    /// подробность — в консоли браузера.
    pub unreadable: usize,
}

/// Запись базы как она лежит: вид и идентификатор из ключа, строка JSON
/// из значения. Ключ не той формы — пустые вид и идентификатор, значение
/// не строка — `None`: такую запись ядро назовёт нечитаемой.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Dumped {
    kind: String,
    id: String,
    text: Option<String>,
}

impl Dumped {
    /// Вид для ядра.
    pub fn raw(&self) -> Raw<'_> {
        Raw {
            kind: &self.kind,
            id: &self.id,
            text: self.text.as_deref(),
        }
    }
}

impl Tx<Write> {
    /// Записать действие целиком и дождаться, пока браузер его сохранит.
    ///
    /// Сначала проверяется, что ни одна правка не ложится поверх записи
    /// более новой версии формата: старая вкладка не перепишет данные новой
    /// (Правило 2), даже если команда забыла прочитать запись. Потом все
    /// записи ставятся разом, без ожидания между ними. Ошибка любой из них —
    /// и не сохраняется ни одна: транзакция отменяется целиком.
    pub async fn commit(self, writes: Writes) -> Result<(), StoreError> {
        // Записывать нечего — и ждать нечего: транзакция без запросов могла
        // уже завершиться, и `complete` не пришёл бы никогда.
        if writes.items.is_empty() {
            return Ok(());
        }
        let _writing = Writing::start(&self.shared);
        // Обработчики `complete` и `abort` — до первой записи, иначе ответ
        // браузера может прийти раньше, чем его начнут ждать.
        let done = Settle::new(self.tx.clone());

        let mut stored = Vec::new();
        for write in writes.items.iter().filter(|write| !write.insert) {
            let request = self
                .records
                .get(&write.key)
                .map_err(|error| failure("get", Some(error)))?;
            stored.push(Settle::new(request));
        }
        for check in stored {
            let request = check.target().clone();
            if check.await.is_err() {
                return Err(failure(
                    "get",
                    request.error().ok().flatten().map(JsValue::from),
                ));
            }
            let newer = request
                .result()
                .ok()
                .and_then(|value| value.as_string())
                .is_some_and(|text| format::version(&text).is_some_and(|v| v > FORMAT_VERSION));
            if newer {
                let _ = self.tx.abort();
                return Err(StoreError::Newer);
            }
        }

        // Между первой и последней записью — только вызовы браузера: паника
        // здесь оставила бы в базе половину действия. `drop` при панике
        // в wasm не зовётся, и транзакция сама фиксирует уже поставленное
        // (проверено вживую в разведке задачи 6). Поэтому строки и ключи
        // готовы заранее, в `Writes`.
        for write in &writes.items {
            let queued = if write.insert {
                self.records.add_with_key(&write.value, &write.key)
            } else {
                self.records.put_with_key(&write.value, &write.key)
            };
            if let Err(error) = queued {
                let _ = self.tx.abort();
                return Err(failure("put", Some(error)));
            }
        }
        // Транзакция фиксируется сама, когда поставленные записи выполнены
        // и новых запросов нет. `IDBTransaction.commit()` ускорил бы это
        // на долю задачи, но в web-sys 0.3.107 он помечен устаревшим.
        if done.await.is_err() {
            return Err(failure("commit", self.tx.error().map(JsValue::from)));
        }
        self.shared.after_write();
        Ok(())
    }
}

/// Записи одного действия: собираются заранее, а ставятся в
/// [`Tx::commit`] разом.
#[derive(Default)]
pub struct Writes {
    items: Vec<Item>,
}

/// Запись, готовая встать в базу: ключ и строка JSON уже в JS.
struct Item {
    insert: bool,
    key: JsValue,
    value: JsValue,
}

impl Writes {
    /// Новая запись. Если запись с таким ключом уже есть, действие
    /// отменяется целиком, а не перезаписывает её: два одинаковых
    /// идентификатора — беда, и чужую запись из-за неё терять нельзя.
    pub fn insert<T: Stored>(&mut self, record: &T) {
        self.push(true, record);
    }

    /// Правка записи `before`, прочитанной в этой же транзакции: `after` —
    /// она же после команды. Ложится с моментами своих полей
    /// (`format::fields::stamp`, задача 116): по ним слияние с другим
    /// устройством берёт каждое поле с той стороны, где его правили позже.
    /// Моменты ставит этот вызов, а не [`Tx::commit`]: он — в части wasm той
    /// команды, а `commit` — в главном, и разбор моментов стоил бы первому
    /// заходу 4,6 КБ после brotli (замер задачи 116; Правило 1).
    pub fn update<T: Stored>(&mut self, before: &T, after: &T) {
        let next = format::encode(after);
        let stamped = format::fields::stamp(T::KIND, &format::encode(before), &next);
        self.items.push(Item {
            insert: false,
            key: key(T::KIND, after.meta().id()),
            value: JsValue::from_str(stamped.as_deref().unwrap_or(&next)),
        });
    }

    /// Запись поверх той, что в базе, как есть — без моментов полей: её
    /// решило слияние копии (`format::office::route`), она сливается только
    /// целиком (логотип, `format::fields::WHOLE`) или прежней записи нет —
    /// она не читается.
    pub fn put<T: Stored>(&mut self, record: &T) {
        self.push(false, record);
    }

    /// Запись из файла копии — поверх той, что в базе, если она есть.
    /// Какие записи класть, решает слияние ядра (`backup::merge`).
    pub fn restore(&mut self, entry: &Entry) {
        self.items.push(Item {
            insert: false,
            key: key(entry.kind(), entry.meta().id()),
            value: JsValue::from_str(&entry.stored()),
        });
    }

    fn push<T: Stored>(&mut self, insert: bool, record: &T) {
        self.items.push(Item {
            insert,
            key: key(T::KIND, record.meta().id()),
            value: JsValue::from_str(&format::encode(record)),
        });
    }
}

/// Ключ записи: `[вид, идентификатор]`.
fn key(kind: &str, id: RecordId) -> JsValue {
    Array::of2(&kind.into(), &id.to_string().into()).into()
}

/// Значение запроса, когда он ответил.
async fn wait(request: IdbRequest) -> Result<JsValue, StoreError> {
    settled(Settle::new(request), "request").await
}

/// Значение запроса, которого уже ждут.
async fn settled(settle: Settle<IdbRequest>, context: &str) -> Result<JsValue, StoreError> {
    let request = settle.target().clone();
    if settle.await.is_err() {
        return Err(failure(
            context,
            request.error().ok().flatten().map(JsValue::from),
        ));
    }
    request
        .result()
        .map_err(|error| failure("result", Some(error)))
}

fn decode<T: Stored>(value: &JsValue) -> Result<T, StoreError> {
    let Some(text) = value.as_string() else {
        log("decode", &"значение не строка".into());
        return Err(StoreError::Damaged);
    };
    format::decode(&text).map_err(|error| match error {
        FormatError::Newer { .. } => StoreError::Newer,
        FormatError::Damaged(detail) => {
            log("decode", &detail.into());
            StoreError::Damaged
        }
    })
}

/// Ошибка открытия базы — словами для человека.
///
/// `VersionError`: база на диске новее программы — её открыла более новая
/// версия. Остальное при открытии почти всегда значит, что хранить данные
/// браузер не даёт (по исходникам движков; разведка задачи 6, 2026-10-12):
/// Chrome при запрете данных сайта отвечает `UnknownError` («The user
/// denied permission…»), Firefox при запрете бросает `SecurityError` сразу,
/// а до 115-й версии в приватном окне отвечал `InvalidStateError`.
/// `UnknownError` у Chrome бывает и от порчи базы на диске — совет «разрешите
/// хранить данные» тогда не поможет, но перезагрузка в нём есть.
fn open_error(context: &str, error: Option<JsValue>) -> StoreError {
    match exception_name(context, error).as_deref() {
        Some("VersionError") => StoreError::Newer,
        Some("QuotaExceededError") => StoreError::Full,
        _ => StoreError::Unavailable,
    }
}

/// Ошибка запроса или транзакции — словами для человека.
fn failure(context: &str, error: Option<JsValue>) -> StoreError {
    match exception_name(context, error).as_deref() {
        Some("QuotaExceededError") => StoreError::Full,
        _ => StoreError::Failed,
    }
}

/// Имя `DOMException` и запись в консоль: человеку окно скажет словами,
/// а разработчику нужна причина. Наружу ничего не уходит (Правило 13).
fn exception_name(context: &str, error: Option<JsValue>) -> Option<String> {
    let error = error?;
    log(context, &error);
    error.dyn_ref::<DomException>().map(DomException::name)
}

fn log(context: &str, error: &JsValue) {
    web_sys::console::error_2(&format!("Kvartik, хранилище ({context}):").into(), error);
}

/// Постоянное ли хранение — без вопроса человеку.
pub async fn persistence() -> Persistence {
    ask(StorageManager::persisted).await
}

/// Спросить у `navigator.storage` про постоянное хранение.
async fn ask(call: fn(&StorageManager) -> Result<Promise, JsValue>) -> Persistence {
    let Some(window) = web_sys::window() else {
        return Persistence::Unknown;
    };
    let storage = window.navigator().storage();
    // Без HTTPS `navigator.storage` нет вовсе (по браузеру: MDN,
    // «Navigator: storage property»).
    if storage.is_undefined() {
        return Persistence::Unknown;
    }
    let Ok(promise) = call(&storage) else {
        return Persistence::Unknown;
    };
    match promise.await.map(|answer| answer.as_bool()) {
        Ok(Some(true)) => Persistence::Persistent,
        Ok(Some(false)) => Persistence::BestEffort,
        Ok(None) | Err(_) => Persistence::Unknown,
    }
}

/// Удалить базу целиком — для браузерных тестов окна, которым нужна пустая
/// база программы. Её соединения к этому времени закрыты: окна прошлых
/// тестов сняты.
#[cfg(all(test, target_arch = "wasm32"))]
pub(crate) async fn delete_database(name: &str) {
    let factory = web_sys::window()
        .and_then(|window| window.indexed_db().ok().flatten())
        .expect("IndexedDB есть");
    let request = factory
        .delete_database(name)
        .expect("удаление базы поставлено");
    let _ = Settle::new(IdbRequest::from(request)).await;
}

/// Тесты хранилища; их помощники с папкой автокопии берут и тесты окна.
#[cfg(all(test, target_arch = "wasm32"))]
pub(crate) mod tests;
