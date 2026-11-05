//! Браузерные тесты копии всех ИП (задача 38): настоящая IndexedDB с базой
//! на каждого ИП и базой конторы, настоящий WebCrypto. Маршрут ИП по базам
//! проверяет ядро (`format::office`); здесь — путь от баз до файла и обратно.

use kvartik_core::format::backup::{FILE_KINDS, Header, KDF_ITERATIONS, KINDS, SALT_BYTES, is_full, open as open_file};
use kvartik_core::model::office::{FIRST_BASE, Ip};
use kvartik_core::model::profile::{Profile, ProfileData};
use kvartik_core::model::text::Line;
use wasm_bindgen_test::wasm_bindgen_test;

use crate::platform::{clock, crypto};
use crate::store::backup::{Made, Sealing, make, make_all, restore_all, restore_new, unlock};
use crate::store::office::{OFFICE_DB, list};
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
