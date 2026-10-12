//! Браузерные тесты хранилища: настоящая IndexedDB в headless-браузере
//! (`wasm-bindgen-test`). Как запускать — Правило 2 в CLAUDE.md.
//!
//! У каждого теста своя база со случайным именем, и после теста она
//! удаляется: тесты не мешают друг другу, а данные программы на том же
//! адресе не задеты.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;

use js_sys::Promise;
use kvartik_core::model::money::{Currency, Money};
use kvartik_core::model::record::{RecordMeta, Timestamp};
use serde::{Deserialize, Serialize};
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

use super::*;

mod backup;

wasm_bindgen_test_configure!(run_in_browser);

/// Запись для проб.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(deny_unknown_fields)]
struct Note {
    meta: RecordMeta,
    text: String,
    sum: Money,
}

impl Stored for Note {
    const KIND: &'static str = "note";

    fn meta(&self) -> &RecordMeta {
        &self.meta
    }
}

/// Запись другого вида — проверить, что виды не смешиваются.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(deny_unknown_fields)]
struct Tag {
    meta: RecordMeta,
}

impl Stored for Tag {
    const KIND: &'static str = "tag";

    fn meta(&self) -> &RecordMeta {
        &self.meta
    }
}

fn at(millis: i64) -> Timestamp {
    Timestamp::from_unix_millis(millis)
}

/// Общие поля в заданном состоянии. Метки ставят только команды ядра
/// (`pub(crate)`), а пробам нужны записи в любом состоянии: поля собираются
/// из данных — так, как их читает хранилище.
fn meta(id: RecordId, modified: i64, deleted: Option<i64>) -> RecordMeta {
    let deleted = deleted.map_or_else(|| "null".to_owned(), |at| at.to_string());
    format::decode(&format!(
        r#"{{"format_version":{FORMAT_VERSION},"record":{{"id":"{id}","modified":{modified},"deleted":{deleted}}}}}"#
    ))
    .expect("поля пробы читаются")
}

/// Новая запись: идентификатор — из WebCrypto (`RecordId::random`).
fn note(text: &str) -> Note {
    Note {
        meta: RecordMeta::new(at(1_760_000_000_000)),
        text: text.to_owned(),
        sum: Money::new(150_000, Currency::Usd),
    }
}

/// База теста: своё имя, события копятся в списке.
struct Probe {
    name: String,
    events: Rc<RefCell<Vec<Event>>>,
}

impl Probe {
    fn new() -> Self {
        Self {
            name: format!("kvartik-test-{}", RecordId::random()),
            events: Rc::default(),
        }
    }

    async fn open(&self, version: u32) -> Result<Store, StoreError> {
        let events = Rc::clone(&self.events);
        Store::open_named(
            &self.name,
            version,
            Box::new(move |event| events.borrow_mut().push(event)),
        )
        .await
    }

    fn events(&self) -> Vec<Event> {
        self.events.borrow().clone()
    }

    /// Удалить базу. Соединения к этому времени закрыты (`Store` брошен).
    async fn delete(self) {
        let factory = web_sys::window()
            .and_then(|window| window.indexed_db().ok().flatten())
            .expect("IndexedDB есть");
        let request = factory
            .delete_database(&self.name)
            .expect("удаление базы поставлено");
        let _ = Settle::new(IdbRequest::from(request)).await;
    }
}

/// Подождать `ms` миллисекунд — чужое будущее, не запрос IndexedDB.
async fn sleep(ms: i32) {
    let promise = Promise::new(&mut |resolve, _| {
        web_sys::window()
            .expect("окно есть")
            .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms)
            .expect("таймер поставлен");
    });
    promise.await.expect("таймер срабатывает");
}

/// Записать в базу строку как есть, в обход `format`.
async fn put_raw(store: &Store, key: &JsValue, value: &str) {
    let tx = store
        .db
        .transaction_with_str_and_mode(RECORDS, IdbTransactionMode::Readwrite)
        .expect("транзакция открыта");
    let done = Settle::new(tx.clone());
    tx.object_store(RECORDS)
        .expect("хранилище есть")
        .put_with_key(&value.into(), key)
        .expect("запись поставлена");
    done.await.expect("запись сохранена");
}

/// Прочитать из базы строку как есть.
async fn get_raw(store: &Store, key: &JsValue) -> Option<String> {
    let tx = store
        .db
        .transaction_with_str_and_mode(RECORDS, IdbTransactionMode::Readonly)
        .expect("транзакция открыта");
    let request = tx
        .object_store(RECORDS)
        .expect("хранилище есть")
        .get(key)
        .expect("чтение поставлено");
    wait(request).await.expect("чтение прошло").as_string()
}

/// Записанное действием читается обратно: по идентификатору и списком
/// своего вида, а чужой вид в список не попадает. Идентификаторы записей —
/// из WebCrypto: этот путь `RecordId::random` здесь исполняется впервые.
#[wasm_bindgen_test]
async fn writes_and_reads_back() {
    let probe = Probe::new();
    let store = probe.open(DB_VERSION).await.expect("база открыта");
    let (first, second) = (note("первая"), note("вторая"));
    let tag = Tag {
        meta: RecordMeta::new(at(1)),
    };

    let tx = store.write().expect("транзакция записи");
    assert_eq!(tx.get::<Note>(first.meta.id()).await, Ok(None));
    let mut writes = Writes::default();
    writes.insert(&first);
    writes.insert(&second);
    writes.insert(&tag);
    tx.commit(writes).await.expect("действие сохранено");

    let tx = store.read().expect("транзакция чтения");
    assert_eq!(
        tx.get::<Note>(first.meta.id()).await,
        Ok(Some(first.clone()))
    );
    assert_eq!(tx.get::<Note>(RecordId::random()).await, Ok(None));
    let ids = |notes: Vec<Note>| notes.iter().map(|n| n.meta.id()).collect::<BTreeSet<_>>();
    assert_eq!(
        tx.all::<Note>().await.map(ids),
        Ok(BTreeSet::from([first.meta.id(), second.meta.id()]))
    );
    assert_eq!(tx.all::<Tag>().await, Ok(vec![tag]));

    drop(store);
    probe.delete().await;
}

/// Запись — до диска: транзакция действия открыта с `durability:
/// "strict"`, а чтение — как решит браузер (шапка `store/mod.rs`).
#[wasm_bindgen_test]
async fn writes_wait_for_the_disk() {
    let probe = Probe::new();
    let store = probe.open(DB_VERSION).await.expect("база открыта");
    let durability = |tx: &IdbTransaction| {
        Reflect::get(tx, &"durability".into())
            .ok()
            .and_then(|value| value.as_string())
    };
    let write = store.write().expect("транзакция записи");
    assert_eq!(durability(&write.tx).as_deref(), Some("strict"));
    let read = store.read().expect("транзакция чтения");
    assert_eq!(durability(&read.tx).as_deref(), Some("default"));

    drop((write, read, store));
    probe.delete().await;
}

/// Сбой посреди действия оставляет прежнее состояние: правка первой
/// записи и новая вторая не сохраняются, если третья запись не легла
/// (ключ уже занят — `ConstraintError`).
#[wasm_bindgen_test]
async fn failed_action_leaves_previous_state() {
    let probe = Probe::new();
    let store = probe.open(DB_VERSION).await.expect("база открыта");
    let first = note("было");
    let mut writes = Writes::default();
    writes.insert(&first);
    store
        .write()
        .expect("транзакция записи")
        .commit(writes)
        .await
        .expect("первая запись сохранена");

    let tx = store.write().expect("транзакция записи");
    let mut changed = tx
        .get::<Note>(first.meta.id())
        .await
        .expect("чтение прошло")
        .expect("запись есть");
    changed.text = "стало".to_owned();
    changed.meta = meta(first.meta.id(), 1_760_000_001_000, None);
    let second = note("лишняя");
    let mut writes = Writes::default();
    writes.update(&changed);
    writes.insert(&second);
    writes.insert(&first);
    assert_eq!(tx.commit(writes).await, Err(StoreError::Failed));

    let tx = store.read().expect("транзакция чтения");
    assert_eq!(tx.get::<Note>(first.meta.id()).await, Ok(Some(first)));
    assert_eq!(tx.get::<Note>(second.meta.id()).await, Ok(None));

    drop(store);
    probe.delete().await;
}

/// Ожидание чужого будущего посреди действия (Правило 6): транзакция за
/// это время зафиксировалась, и запись не ложится вовсе — ни целиком, ни
/// наполовину, а действие отвечает ошибкой, а не молчит.
#[wasm_bindgen_test]
async fn waiting_on_something_else_writes_nothing() {
    let probe = Probe::new();
    let store = probe.open(DB_VERSION).await.expect("база открыта");
    let first = note("было");
    let mut writes = Writes::default();
    writes.insert(&first);
    store
        .write()
        .expect("транзакция записи")
        .commit(writes)
        .await
        .expect("первая запись сохранена");

    let tx = store.write().expect("транзакция записи");
    let mut changed = tx
        .get::<Note>(first.meta.id())
        .await
        .expect("чтение прошло")
        .expect("запись есть");
    sleep(10).await;
    changed.text = "стало".to_owned();
    let second = note("вторая");
    let mut writes = Writes::default();
    writes.insert(&second);
    writes.update(&changed);
    assert!(
        tx.commit(writes).await.is_err(),
        "запись после чужого ожидания прошла"
    );

    let tx = store.read().expect("транзакция чтения");
    assert_eq!(tx.get::<Note>(first.meta.id()).await, Ok(Some(first)));
    assert_eq!(tx.get::<Note>(second.meta.id()).await, Ok(None));

    drop(store);
    probe.delete().await;
}

/// Пустому действию нечего ждать — даже когда транзакция уже завершилась
/// за чужим ожиданием: `complete` к тому времени прошёл, и ждать его
/// значило бы висеть вечно. И сохранением оно не считается: постоянное
/// хранение после него не просится.
#[wasm_bindgen_test]
async fn empty_action_does_not_wait() {
    let probe = Probe::new();
    let store = probe.open(DB_VERSION).await.expect("база открыта");
    let tx = store.write().expect("транзакция записи");
    sleep(10).await;
    assert_eq!(tx.commit(Writes::default()).await, Ok(()));
    sleep(50).await;
    assert_eq!(probe.events(), []);

    drop(store);
    probe.delete().await;
}

/// Запись более новой версии формата старая программа не читает и не
/// переписывает (Правило 2), а испорченную — называет испорченной.
#[wasm_bindgen_test]
async fn newer_and_damaged_records_are_refused() {
    let probe = Probe::new();
    let store = probe.open(DB_VERSION).await.expect("база открыта");
    let first = note("старая");
    let key = key(Note::KIND, first.meta.id());
    let newer = format!(
        r#"{{"format_version":{},"record":{{"wings":2}}}}"#,
        FORMAT_VERSION + 1
    );
    put_raw(&store, &key, &newer).await;

    let tx = store.write().expect("транзакция записи");
    assert_eq!(
        tx.get::<Note>(first.meta.id()).await,
        Err(StoreError::Newer)
    );
    let mut writes = Writes::default();
    writes.update(&first);
    assert_eq!(
        store
            .write()
            .expect("транзакция записи")
            .commit(writes)
            .await,
        Err(StoreError::Newer)
    );
    drop(tx);
    assert_eq!(get_raw(&store, &key).await, Some(newer));

    put_raw(&store, &key, "не JSON").await;
    let tx = store.read().expect("транзакция чтения");
    assert_eq!(
        tx.get::<Note>(first.meta.id()).await,
        Err(StoreError::Damaged)
    );
    assert_eq!(tx.all::<Note>().await, Err(StoreError::Damaged));

    drop(store);
    probe.delete().await;
}

/// Новая версия базы в другой вкладке: старое соединение закрывается само,
/// новое открывается без ожидания, а старое дальше только отказывает
/// словами «программа обновилась».
#[wasm_bindgen_test]
async fn old_connection_gives_way_to_a_new_version() {
    let probe = Probe::new();
    let old = probe.open(DB_VERSION).await.expect("старая открыта");
    let new = probe.open(DB_VERSION + 1).await.expect("новая открыта");
    assert_eq!(probe.events(), [Event::Closed(StoreError::Replaced)]);
    assert!(matches!(old.read(), Err(StoreError::Replaced)));
    assert!(new.read().is_ok());

    drop((old, new));
    probe.delete().await;
}

/// Соединение, которое не уступает (вкладка без обработчика
/// `versionchange`), задерживает открытие: хранилище говорит об этом
/// событием, а открытие завершается само, когда соединение закрылось.
#[wasm_bindgen_test]
async fn blocked_open_waits_and_then_opens() {
    let probe = Probe::new();
    drop(probe.open(DB_VERSION).await.expect("база создана"));
    let factory = web_sys::window()
        .and_then(|window| window.indexed_db().ok().flatten())
        .expect("IndexedDB есть");
    let request = factory
        .open_with_u32(&probe.name, DB_VERSION)
        .expect("открытие поставлено");
    let stubborn: IdbDatabase = wait(IdbRequest::from(request))
        .await
        .expect("упрямое соединение открыто")
        .unchecked_into();

    let opened = Rc::new(RefCell::new(None));
    let result = Rc::clone(&opened);
    let events = Rc::clone(&probe.events);
    let name = probe.name.clone();
    js_sys::futures::spawn_local(async move {
        let store = Store::open_named(
            &name,
            DB_VERSION + 1,
            Box::new(move |event| events.borrow_mut().push(event)),
        )
        .await;
        *result.borrow_mut() = Some(store);
    });
    for _ in 0..100 {
        if !probe.events().is_empty() {
            break;
        }
        sleep(20).await;
    }
    assert_eq!(probe.events(), [Event::Blocked]);
    assert!(opened.borrow().is_none(), "открылось, не дождавшись");

    stubborn.close();
    for _ in 0..100 {
        if opened.borrow().is_some() {
            break;
        }
        sleep(20).await;
    }
    let store = opened.borrow_mut().take().expect("открытие завершилось");
    assert!(store.expect("база открыта").read().is_ok());

    probe.delete().await;
}

/// Первая запись в сессии просит постоянное хранение, и ответ браузера
/// приходит событием; вторая — уже не просит.
#[wasm_bindgen_test]
async fn first_write_asks_for_persistence_once() {
    let probe = Probe::new();
    let store = probe.open(DB_VERSION).await.expect("база открыта");
    for text in ["первая", "вторая"] {
        let mut writes = Writes::default();
        writes.insert(&note(text));
        store
            .write()
            .expect("транзакция записи")
            .commit(writes)
            .await
            .expect("запись сохранена");
    }
    for _ in 0..100 {
        if !probe.events().is_empty() {
            break;
        }
        sleep(20).await;
    }
    sleep(100).await;
    let events = probe.events();
    assert!(
        matches!(events.as_slice(), [Event::Persistence(_)]),
        "события: {events:?}"
    );

    drop(store);
    probe.delete().await;
}

/// Идентификаторы из WebCrypto — UUID версии 4, и у каждой записи свой.
#[wasm_bindgen_test]
fn ids_come_from_webcrypto() {
    let ids: Vec<String> = (0..1_000).map(|_| RecordId::random().to_string()).collect();
    let distinct: BTreeSet<&String> = ids.iter().collect();
    assert_eq!(distinct.len(), ids.len(), "идентификаторы повторились");
    let wrong: Vec<&String> = ids
        .iter()
        .filter(|id| {
            let bytes = id.as_bytes();
            bytes[14] != b'4' || !matches!(bytes[19], b'8' | b'9' | b'a' | b'b')
        })
        .collect();
    assert!(wrong.is_empty(), "не UUID версии 4: {wrong:?}");
}
