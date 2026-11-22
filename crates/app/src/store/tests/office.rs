//! Браузерные тесты копии всех ИП (задача 38): настоящая IndexedDB с базой
//! на каждого ИП и базой конторы, настоящий WebCrypto. Маршрут ИП по базам
//! проверяет ядро (`format::office`); здесь — путь от баз до файла и обратно.

use kvartik_core::format::Stored;
use kvartik_core::format::backup::{
    Entry, FILE_KINDS, Header, KDF_ITERATIONS, KINDS, SALT_BYTES, is_full, open as open_file,
};
use kvartik_core::license::{self, License, MAX_KEYS, OfficeLicense, Plan};
use kvartik_core::model::office::{FIRST_BASE, Ip};
use kvartik_core::model::profile::{Profile, ProfileData};
use kvartik_core::model::text::Line;
use kvartik_core::model::tin::Tin;
use wasm_bindgen_test::wasm_bindgen_test;

use crate::platform::{clock, crypto};
use crate::store::backup::{
    Failure, Made, Merged, OtherIe, Sealing, make, make_all, restore, restore_all, restore_new, unlock,
};
use crate::store::StoreError;
use crate::store::office::{OFFICE_DB, keys as office_keys, list};
use crate::store::{DB_NAME, Store, Writes, delete_database};

const PASSWORD: &str = "Երևան կопия всех ИП 2026";

fn named(name: &str) -> Profile {
    let mut data = ProfileData::default();
    data.name.en = Line::new(name).expect("годное имя");
    Profile::save(None, data, clock::now()).expect("годный профиль").expect("новый")
}

async fn write(store: &Store, profile: &Profile) {
    let mut writes = Writes::default();
    writes.insert(profile);
    store.write().expect("транзакция").commit(writes).await.expect("записано");
}

/// Профиль базы `name` по-английски; базы нет или профиля нет — `None`.
async fn profile_in(name: &str) -> Option<String> {
    let store = Store::open_base(name, |_| {}).await.ok()?;
    let profiles = store.read().ok()?.all::<Profile>().await.ok()?;
    Profile::current(profiles).map(|profile| profile.data().name.en.as_str().to_owned())
}

/// Собрать файл копии: всех ИП — с конторой, одного — без.
async fn sealed(salt: &[u8], all: Option<(&Store, &str, &Store)>, single: Option<&Store>) -> Made {
    let key = crypto::derive(&crypto::normalize(PASSWORD), salt, KDF_ITERATIONS).await.expect("ключ");
    let header = Header::new("0.0.87", clock::now());
    let sealing = Sealing {
        header: &header,
        key: &key,
        salt,
        iterations: KDF_ITERATIONS,
    };
    match (all, single) {
        (Some((open, name, office)), _) => make_all(open, name, office, &sealing).await.expect("копия всех"),
        (None, Some(store)) => make(store, KINDS, &sealing).await.expect("копия одного"),
        (None, None) => unreachable!("тест просит одну из двух"),
    }
}

/// Копия всех ИП собирается с каждой базы и с конторы — и на свежем
/// устройстве ложится по базам: ранний ИП — в его пустую `kvartik`, второй —
/// в свою; повтор не меняет ничего. Убранный ИП тоже в копии.
#[wasm_bindgen_test]
async fn a_backup_of_all_ies_lands_in_each_base() {
    delete_database(OFFICE_DB).await;
    delete_database(DB_NAME).await;
    let (today, now) = (clock::today().expect("сегодня"), clock::now());
    let aram = named("IE Aram");
    let anna = named("IE Anna");
    let vahe = named("IE Vahe");
    let first = Ip::first(Some(&aram), today, now);
    let mut second = Ip::new(today, now);
    second.copy_profile(&anna, now);
    let mut removed = Ip::new(today, now);
    removed.copy_profile(&vahe, now);
    removed.remove(now);
    let open = Store::open_base(DB_NAME, |_| {}).await.expect("база открыта");
    write(&open, &aram).await;
    for (ip, profile) in [(&second, &anna), (&removed, &vahe)] {
        let store = Store::open_base(ip.base(), |_| {}).await.expect("база открыта");
        write(&store, profile).await;
    }
    let office = Store::open_office(|_| {}).await.expect("контора открыта");
    let mut writes = Writes::default();
    for ip in [&first, &second, &removed] {
        writes.insert(ip);
    }
    office.write().expect("транзакция").commit(writes).await.expect("записано");
    let salt = crypto::random(SALT_BYTES).expect("соль");
    let made = sealed(&salt, Some((&open, DB_NAME, &office)), None).await;
    assert_eq!(made.records, 6, "три записи списка и три профиля");

    // Свежее устройство: всё стёрто.
    drop((open, office));
    for name in [DB_NAME, OFFICE_DB, second.base(), removed.base()] {
        delete_database(name).await;
    }
    let file = open_file(&made.file).expect("файл открывается");
    let (entries, _) = unlock(FILE_KINDS, &file, &crypto::normalize(PASSWORD)).await.expect("расшифрована");
    assert!(is_full(&entries));
    let open = Store::open_base(DB_NAME, |_| {}).await.expect("база открыта");
    let office = Store::open_office(|_| {}).await.expect("контора открыта");
    let done = restore_all(&open, DB_NAME, &office, entries.clone(), None, Some(DB_NAME))
        .await
        .expect("легла");
    let mut wrong = Vec::new();
    if (done.restored.added, done.covers_all, done.failed.is_none()) != (3, true, true) {
        wrong.push(format!(
            "легло {} записей, все ИП устройства в копии: {}, сбой: {:?}",
            done.restored.added, done.covers_all, done.failed
        ));
    }
    let ips = list(&office).await.expect("список").records;
    if ips.len() != 3 {
        wrong.push(format!("в списке {} ИП вместо трёх", ips.len()));
    }
    let mut bases: Vec<&str> = ips.iter().map(Ip::base).collect();
    bases.dedup();
    if bases.len() != ips.len() || !bases.contains(&FIRST_BASE) {
        wrong.push(format!("базы не свои: {bases:?}"));
    }
    for ip in &ips {
        let got = profile_in(ip.base()).await;
        if got.as_deref() != Some(ip.name(kvartik_core::i18n::Lang::En)) {
            wrong.push(format!("в базе {} профиль {got:?}, а ИП {}", ip.base(), ip.name(kvartik_core::i18n::Lang::En)));
        }
    }
    if !ips.iter().any(|ip| ip.is_removed() && ip.name(kvartik_core::i18n::Lang::En) == "IE Vahe") {
        wrong.push("убранный ИП вернулся не убранным".to_owned());
    }
    // Повтор безвреден: всё уже такое же.
    let again = restore_all(&open, DB_NAME, &office, entries.clone(), None, None).await.expect("повтор");
    if again.restored.added + again.restored.updated != 0 || list(&office).await.expect("список").records.len() != 3 {
        wrong.push(format!("повтор что-то записал: {:?}", again.restored));
    }
    if !again.covers_all {
        wrong.push("копия со всеми ИП не названа копией всех".to_owned());
    }
    // Здесь завели ещё одного ИП — копии без него днём копии не считаться.
    let mut writes = Writes::default();
    writes.insert(&Ip::new(today, now));
    office.write().expect("транзакция").commit(writes).await.expect("записано");
    let partial = restore_all(&open, DB_NAME, &office, entries, None, None).await.expect("ещё повтор");
    if partial.covers_all {
        wrong.push("копия без здешнего ИП названа копией всех".to_owned());
    }
    drop((open, office));
    for ip in &ips {
        delete_database(ip.base()).await;
    }
    delete_database(OFFICE_DB).await;
    assert!(wrong.is_empty(), "копия всех ИП: {wrong:#?}");
}

/// Записи файла копии `made` — расшифрованные.
async fn entries_of(made: &Made) -> Vec<Entry> {
    let file = open_file(&made.file).expect("файл открывается");
    let (entries, _) = unlock(FILE_KINDS, &file, &crypto::normalize(PASSWORD)).await.expect("расшифрована");
    entries
}

/// Ключ «Pro» конторы (задача 38) — в копии всех ИП и только в ней: копия
/// одного ИП его не несёт. На свежем устройстве он ложится в контору, а не
/// в базу ИП, и повтор его не множит.
#[wasm_bindgen_test]
async fn a_pro_key_travels_only_with_the_backup_of_all_ies() {
    delete_database(OFFICE_DB).await;
    delete_database(DB_NAME).await;
    let (today, now) = (clock::today().expect("сегодня"), clock::now());
    let aram = named("IE Aram");
    let first = Ip::first(Some(&aram), today, now);
    let tin = Tin::parse("07654321").expect("годный ՀՎՀՀ");
    let until = today.add_days(365).expect("дата");
    let key = license::Key::signed(tin, Plan::Pro, today, until, Some(30), [0; 64]).expect("ключ Pro");
    let pro = OfficeLicense::enter(key, &[], now).expect("ключ Pro");
    let open = Store::open_base(DB_NAME, |_| {}).await.expect("база открыта");
    write(&open, &aram).await;
    let office = Store::open_office(|_| {}).await.expect("контора открыта");
    let mut writes = Writes::default();
    writes.insert(&first);
    writes.insert(&pro);
    office.write().expect("транзакция").commit(writes).await.expect("записано");
    let salt = crypto::random(SALT_BYTES).expect("соль");
    let all = sealed(&salt, Some((&open, DB_NAME, &office)), None).await;
    let single = sealed(&salt, None, Some(&open)).await;
    let mut wrong = Vec::new();
    if (all.records, single.records) != (3, 1) {
        wrong.push(format!("в копии всех ИП {} записей, в копии одного — {}", all.records, single.records));
    }
    let single_entries = entries_of(&single).await;
    if single_entries.iter().any(|entry| entry.kind() == OfficeLicense::KIND) || is_full(&single_entries) {
        wrong.push("ключ Pro — в копии одного ИП".to_owned());
    }
    let entries = entries_of(&all).await;
    if !entries.iter().any(|entry| entry.kind() == OfficeLicense::KIND && entry.base().is_none()) {
        wrong.push("в копии всех ИП нет ключа Pro без метки базы".to_owned());
    }

    // Свежее устройство.
    drop((open, office));
    delete_database(DB_NAME).await;
    delete_database(OFFICE_DB).await;
    let open = Store::open_base(DB_NAME, |_| {}).await.expect("база открыта");
    let office = Store::open_office(|_| {}).await.expect("контора открыта");
    let done = restore_all(&open, DB_NAME, &office, entries.clone(), None, Some(DB_NAME))
        .await
        .expect("легла");
    if (done.restored.added, done.failed.is_none()) != (2, true) {
        wrong.push(format!("легло {} записей, сбой: {:?}", done.restored.added, done.failed));
    }
    let keys = || async {
        let keys = office_keys(&office).await.expect("контора читается");
        keys.records.into_iter().map(|license| license.id()).collect::<Vec<_>>()
    };
    if keys().await != [pro.id()] {
        wrong.push(format!("в конторе ключи {:?}, а не ключ копии", keys().await));
    }
    // Легший ключ — легшая запись конторы: код всех ИП возьмёт его дальше
    // (ревью задачи 116).
    if !done.landed.iter().any(|(base, at)| base == OFFICE_DB && *at == pro.meta().modified()) {
        wrong.push(format!("ключ не назван легшим: {:?}", done.landed));
    }
    let in_base = open.read().expect("чтение").list::<License>().await.expect("ключи").records.len();
    let others = open.read().expect("чтение").list::<OfficeLicense>().await.expect("записи").records.len();
    if (in_base, others) != (0, 0) {
        wrong.push(format!("в базе ИП ключей {in_base} и записей конторы {others}"));
    }
    let again = restore_all(&open, DB_NAME, &office, entries, None, None).await.expect("повтор");
    if again.restored.added + again.restored.updated != 0 || keys().await.len() != 1 {
        wrong.push(format!("повтор что-то записал: {:?}", again.restored));
    }
    drop((open, office));
    delete_database(OFFICE_DB).await;
    delete_database(DB_NAME).await;
    assert!(wrong.is_empty(), "ключ Pro в копии: {wrong:#?}");
}

/// Запись ключа «Pro» под этим идентификатором — испорченная: не JSON.
async fn spoil_key(office: &Store, id: &str) {
    let key: wasm_bindgen::JsValue = js_sys::Array::of2(&OfficeLicense::KIND.into(), &id.into()).into();
    crate::store::tests::put_raw(office, &key, "не JSON").await;
}

/// Ключ «Pro» с последним днём `until`, подписью из нулей.
fn pro_key(until: kvartik_core::model::date::Date) -> OfficeLicense {
    let today = clock::today().expect("сегодня");
    let tin = Tin::parse("07654321").expect("годный ՀՎՀՀ");
    let key = license::Key::signed(tin, Plan::Pro, today, until, Some(30), [0; 64]).expect("ключ Pro");
    OfficeLicense::enter(key, &[], clock::now()).expect("ключ Pro")
}

/// Нечитаемая запись ключа «Pro» копию всех ИП не останавливает — её только
/// нет в копии, и окно называет число, — а нечитаемая запись ИП
/// останавливает: база этого ИП выпала бы из копии молча (ревью задачи 38).
#[wasm_bindgen_test]
async fn a_broken_pro_key_does_not_stop_the_backup_of_all_ies() {
    delete_database(OFFICE_DB).await;
    delete_database(DB_NAME).await;
    let (today, now) = (clock::today().expect("сегодня"), clock::now());
    let aram = named("IE Aram");
    let open = Store::open_base(DB_NAME, |_| {}).await.expect("база открыта");
    write(&open, &aram).await;
    let office = Store::open_office(|_| {}).await.expect("контора открыта");
    let mut writes = Writes::default();
    writes.insert(&Ip::first(Some(&aram), today, now));
    office.write().expect("транзакция").commit(writes).await.expect("записано");
    spoil_key(&office, "6e1d3f8a-9b52-4f4c-8d7e-3c8b2a1f0e45").await;
    let salt = crypto::random(SALT_BYTES).expect("соль");
    let key = crypto::derive(&crypto::normalize(PASSWORD), &salt, KDF_ITERATIONS).await.expect("ключ");
    let header = Header::new("0.0.134", clock::now());
    let sealing = Sealing {
        header: &header,
        key: &key,
        salt: &salt,
        iterations: KDF_ITERATIONS,
    };
    let mut wrong = Vec::new();
    match make_all(&open, DB_NAME, &office, &sealing).await {
        Ok(made) if (made.records, made.unreadable) == (2, 1) => {}
        other => wrong.push(format!("с испорченным ключом: {:?}", other.map(|made| (made.records, made.unreadable)))),
    }
    let broken_ip: wasm_bindgen::JsValue =
        js_sys::Array::of2(&Ip::KIND.into(), &"5d0c2f7e-8a41-4e3b-9c6d-2b7a1f0e9d34".into()).into();
    crate::store::tests::put_raw(&office, &broken_ip, "не JSON").await;
    if !matches!(make_all(&open, DB_NAME, &office, &sealing).await, Err(Failure::Store(StoreError::Damaged))) {
        wrong.push("испорченная запись ИП не остановила копию".to_owned());
    }
    drop((open, office));
    delete_database(OFFICE_DB).await;
    delete_database(DB_NAME).await;
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// Записи ключей «Стандарт» из копии в базу ИП — тоже не сверх `MAX_KEYS`:
/// копию одного ИП, с которой их стало бы больше, восстановление не кладёт,
/// а ровно до предела — кладёт (ревью задачи 38: эта дыра была с 0.0.60, и
/// тысячи поддельных ключей на ՀՎՀՀ из счетов человека держали бы страницу).
#[wasm_bindgen_test]
async fn base_keys_over_the_limit_are_not_restored() {
    delete_database(DB_NAME).await;
    let today = clock::today().expect("сегодня");
    let standard = |n: usize| {
        let tin = Tin::parse("00054321").expect("годный ՀՎՀՀ");
        let until = today.add_days(i32::try_from(n).expect("число") + 1).expect("дата");
        let key = license::Key::signed(tin, Plan::Standard, today, until, None, [0; 64]).expect("ключ");
        License::enter(key, &[], clock::now()).expect("ключ")
    };
    let open = Store::open_base(DB_NAME, |_| {}).await.expect("база открыта");
    let kept = standard(0);
    let mut writes = Writes::default();
    writes.insert(&kept);
    for n in 1..MAX_KEYS - 1 {
        writes.insert(&standard(n));
    }
    open.write().expect("транзакция").commit(writes).await.expect("записано");
    let entries = |licenses: &[License]| {
        let records: Vec<(String, String)> = licenses
            .iter()
            .map(|license| (license.id().to_string(), kvartik_core::format::encode(license)))
            .collect();
        let raws = records.iter().map(|(id, text)| kvartik_core::format::backup::Raw {
            kind: License::KIND,
            id,
            text: Some(text),
        });
        kvartik_core::format::backup::collect(KINDS, raws).expect("собрано").entries().to_vec()
    };
    let copy = |from: usize, count: usize| entries(&(from..from + count).map(&standard).collect::<Vec<_>>());
    let count = || async { open.read().expect("чтение").list::<License>().await.expect("ключи").records.len() };
    let mut wrong = Vec::new();
    match restore(&open, KINDS, &copy(1_000, 2), OtherIe::Merge).await {
        Err(Failure::TooManyKeys) => {}
        other => wrong.push(format!("сверх предела: {other:?}")),
    }
    if count().await != MAX_KEYS - 1 {
        wrong.push("сверх предела что-то легло".to_owned());
    }
    match restore(&open, KINDS, &copy(2_000, 1), OtherIe::Merge).await {
        Ok(Merged::Done(done)) if done.added == 1 => {}
        other => wrong.push(format!("ровно предел: {other:?}")),
    }
    if count().await != MAX_KEYS {
        wrong.push("ровно до предела ключ не лёг".to_owned());
    }
    // Ключей больше предела, как могло лечь до 0.0.135: копия без новых
    // ключей — своя копия такой базы — кладётся, а с новым ключом — нет.
    let mut writes = Writes::default();
    writes.insert(&standard(3_000));
    open.write().expect("транзакция").commit(writes).await.expect("записано");
    match restore(&open, KINDS, &entries(std::slice::from_ref(&kept)), OtherIe::Merge).await {
        Ok(Merged::Done(_)) => {}
        other => wrong.push(format!("без новых ключей сверх предела: {other:?}")),
    }
    match restore(&open, KINDS, &copy(4_000, 1), OtherIe::Merge).await {
        Err(Failure::TooManyKeys) => {}
        other => wrong.push(format!("новый ключ сверх предела: {other:?}")),
    }
    if count().await != MAX_KEYS + 1 {
        wrong.push("сверх предела ключей стало больше".to_owned());
    }
    drop(open);
    delete_database(DB_NAME).await;
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// Копию, с которой записей ключей «Pro» в конторе стало бы больше
/// `MAX_KEYS`, восстановление не кладёт — ни ключей, ни списка, — а ровно до
/// предела кладёт (ревью задачи 38: тысячи поддельных ключей держали бы
/// страницу).
#[wasm_bindgen_test]
async fn pro_keys_over_the_limit_are_not_restored() {
    delete_database(OFFICE_DB).await;
    delete_database(DB_NAME).await;
    let (today, now) = (clock::today().expect("сегодня"), clock::now());
    let aram = named("IE Aram");
    let first = Ip::first(Some(&aram), today, now);
    let until = |n: usize| today.add_days(i32::try_from(n).expect("число") + 1).expect("дата");
    let open = Store::open_base(DB_NAME, |_| {}).await.expect("база открыта");
    write(&open, &aram).await;
    let office = Store::open_office(|_| {}).await.expect("контора открыта");
    let mut writes = Writes::default();
    writes.insert(&first);
    for n in 0..MAX_KEYS - 1 {
        writes.insert(&pro_key(until(n)));
    }
    office.write().expect("транзакция").commit(writes).await.expect("записано");
    let salt = crypto::random(SALT_BYTES).expect("соль");
    let made = sealed(&salt, Some((&open, DB_NAME, &office)), None).await;
    // Копия без ключей — список и база, — а к ней новые ключи.
    let bare: Vec<Entry> = entries_of(&made)
        .await
        .into_iter()
        .filter(|entry| entry.kind() != OfficeLicense::KIND)
        .collect();
    let with_new = |from: usize, count: usize| {
        let mut entries = bare.clone();
        for n in from..from + count {
            let key = pro_key(until(n));
            let stored = kvartik_core::format::encode(&key);
            let id = key.id().to_string();
            let raw = kvartik_core::format::backup::Raw {
                kind: OfficeLicense::KIND,
                id: &id,
                text: Some(&stored),
            };
            let collected = kvartik_core::format::backup::collect(kvartik_core::format::backup::OFFICE_KINDS, [raw])
                .expect("собрано");
            entries.extend(collected.entries().iter().cloned());
        }
        entries
    };
    let mut wrong = Vec::new();
    let count = || async { office_keys(&office).await.expect("ключи").records.len() };
    // В самом файле ключей больше предела — его не кладёт маршрут; а с двумя
    // новыми к 99 здешним их стало бы 101 — не кладёт слияние.
    for (what, entries) in [("в файле", with_new(1_000, MAX_KEYS + 1)), ("с прежними", with_new(1_000, 2))] {
        match restore_all(&open, DB_NAME, &office, entries, None, None).await {
            Err(Failure::TooManyKeys) => {}
            other => wrong.push(format!("{what} сверх предела: {:?}", other.map(|done| done.restored))),
        }
        if count().await != MAX_KEYS - 1 {
            wrong.push(format!("{what} сверх предела что-то легло"));
        }
    }
    match restore_all(&open, DB_NAME, &office, with_new(2_000, 1), None, None).await {
        Ok(done) if done.restored.added == 1 => {}
        other => wrong.push(format!("ровно предел: {:?}", other.map(|done| done.restored))),
    }
    if count().await != MAX_KEYS {
        wrong.push("ровно до предела ключи не легли".to_owned());
    }
    drop((open, office));
    delete_database(OFFICE_DB).await;
    delete_database(DB_NAME).await;
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// Копия одного ИП ложится новым ИП: своя новая база и запись в списке;
/// открытого ИП, которого в списке не было, список заводит первым.
#[wasm_bindgen_test]
async fn a_backup_of_one_ie_lands_as_a_new_ie() {
    delete_database(OFFICE_DB).await;
    delete_database(DB_NAME).await;
    let (today, now) = (clock::today().expect("сегодня"), clock::now());
    let aram = named("IE Aram");
    let other = Ip::new(today, now);
    let source = Store::open_base(other.base(), |_| {}).await.expect("база открыта");
    write(&source, &named("IE Anna")).await;
    let salt = crypto::random(SALT_BYTES).expect("соль");
    let made = sealed(&salt, None, Some(&source)).await;
    drop(source);
    delete_database(other.base()).await;
    let file = open_file(&made.file).expect("файл открывается");
    let (entries, _) = unlock(FILE_KINDS, &file, &crypto::normalize(PASSWORD)).await.expect("расшифрована");
    assert!(!is_full(&entries));

    let office = Store::open_office(|_| {}).await.expect("контора открыта");
    let first = Ip::first(Some(&aram), today, now);
    let (new, restored) = restore_new(&office, &entries, Some(first.clone()), today, now).await.expect("легла");
    let mut wrong = Vec::new();
    if restored.added != 1 {
        wrong.push(format!("легло {} записей", restored.added));
    }
    let ips = list(&office).await.expect("список").records;
    let bases: Vec<&str> = ips.iter().map(Ip::base).collect();
    if bases.len() != 2 || !bases.contains(&FIRST_BASE) || !bases.contains(&new.base()) {
        wrong.push(format!("в списке не первый и новый: {bases:?}"));
    }
    if profile_in(new.base()).await.as_deref() != Some("IE Anna") {
        wrong.push("в новой базе не профиль копии".to_owned());
    }
    drop(office);
    delete_database(new.base()).await;
    delete_database(OFFICE_DB).await;
    assert!(wrong.is_empty(), "копия одного ИП новым ИП: {wrong:#?}");
}

/// Копию одного ИП с ключами сверх `MAX_KEYS` новым ИП восстановление не
/// кладёт — ни записей, ни ИП в списке (ревью задачи 38).
#[wasm_bindgen_test]
async fn keys_over_the_limit_do_not_land_as_a_new_ie() {
    delete_database(OFFICE_DB).await;
    let (today, now) = (clock::today().expect("сегодня"), clock::now());
    let records: Vec<(String, String)> = (0..=MAX_KEYS)
        .map(|n| {
            let tin = Tin::parse("00054321").expect("годный ՀՎՀՀ");
            let until = today.add_days(i32::try_from(n).expect("число") + 1).expect("дата");
            let key = license::Key::signed(tin, Plan::Standard, today, until, None, [0; 64]).expect("ключ");
            let license = License::enter(key, &[], now).expect("ключ");
            (license.id().to_string(), kvartik_core::format::encode(&license))
        })
        .collect();
    let raws = records.iter().map(|(id, text)| kvartik_core::format::backup::Raw {
        kind: License::KIND,
        id,
        text: Some(text),
    });
    let entries = kvartik_core::format::backup::collect(KINDS, raws).expect("собрано").entries().to_vec();
    let office = Store::open_office(|_| {}).await.expect("контора открыта");
    let mut wrong = Vec::new();
    match restore_new(&office, &entries, None, today, now).await {
        Err(Failure::TooManyKeys) => {}
        other => wrong.push(format!("сверх предела: {other:?}")),
    }
    if !list(&office).await.expect("список").records.is_empty() {
        wrong.push("в списке новый ИП".to_owned());
    }
    drop(office);
    delete_database(OFFICE_DB).await;
    assert!(wrong.is_empty(), "{wrong:#?}");
}
