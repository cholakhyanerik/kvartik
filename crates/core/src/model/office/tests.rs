use super::*;
use crate::model::profile::ProfileData;

fn at(millis: i64) -> Timestamp {
    Timestamp::from_unix_millis(millis)
}

fn day(year: u16, month: u8, day: u8) -> Date {
    Date::new(year, month, day).expect("в тесте — существующая дата")
}

fn profile(name: &str, tin: Option<&str>, millis: i64) -> Profile {
    let mut data = ProfileData::default();
    data.name.en = Line::new(name).expect("в тесте — годное имя");
    data.tin = tin.map(|tin| Tin::parse(tin).expect("в тесте — годный ՀՎՀՀ"));
    Profile::save(None, data, at(millis))
        .expect("годный профиль")
        .expect("новый профиль")
}

/// Запись ИП в данных — слово в слово. Поменялась запись — это новая
/// версия формата (Правило 5), а не правка образца.
const SAMPLE: &str = concat!(
    r#"{"meta":{"id":"5d0c2f7e-8a41-4e3b-9c6d-2b7a1f0e9d34","modified":1762300000000,"deleted":null},"#,
    r#""base":"kvartik-ip-5d0c2f7e-8a41-4e3b-9c6d-2b7a1f0e9d34","#,
    r#""profile":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","#,
    r#""name":{"hy":"ԱՁ Աննա Հակոբյան","ru":"ИП Анна Акопян","en":"IE Anna Hakobyan"},"#,
    r#""tin":"99999990","added":"2026-11-05","added_at":null}"#
);

#[test]
fn data_shape_is_pinned() {
    let ip: Ip = serde_json::from_str(SAMPLE).expect("образец читается");
    assert_eq!(ip.id().to_string(), "5d0c2f7e-8a41-4e3b-9c6d-2b7a1f0e9d34");
    assert_eq!(ip.base(), "kvartik-ip-5d0c2f7e-8a41-4e3b-9c6d-2b7a1f0e9d34");
    assert_eq!(
        ip.profile().map(|id| id.to_string()).as_deref(),
        Some("0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a")
    );
    assert_eq!(ip.name(Lang::Ru), "ИП Анна Акопян");
    assert_eq!(ip.tin().map(Tin::as_str), Some("99999990"));
    assert_eq!(ip.added(), day(2026, 11, 5));
    assert!(!ip.is_removed());
    assert_eq!(serde_json::to_string(&ip).ok().as_deref(), Some(SAMPLE));
    // С 0.0.134 — момент добавления; запись до него — без поля, и читается
    // без момента.
    let with_moment = SAMPLE.replace(r#""added_at":null"#, r#""added_at":1762300000000"#);
    let moment: Ip = serde_json::from_str(&with_moment).expect("образец с моментом читается");
    assert_eq!(serde_json::to_string(&moment).ok().as_deref(), Some(with_moment.as_str()));
    let before: Ip = serde_json::from_str(&SAMPLE.replace(r#","added_at":null"#, "")).expect("запись до 0.0.134 читается");
    assert_eq!(before, ip);
}

/// В один день места идут по моменту добавления: тридцать ИП, добавленных
/// подряд, встают в том же порядке, какими бы ни вышли их случайные
/// идентификаторы, а первый ИП программы — впереди добавленных с ним в один
/// день. Добавленный позже места у прежних не отнимает (ревью задачи 38).
#[test]
fn the_same_day_keeps_the_order_of_adding() {
    let today = day(2026, 11, 16);
    let first = Ip::first(None, today, at(1_000));
    let added: Vec<Ip> = (0..30).map(|n| Ip::new(today, at(2_000 + n))).collect();
    let mut ips = added.clone();
    ips.reverse();
    ips.push(first.clone());
    let order: Vec<&str> = alive(&ips).iter().map(|ip| ip.base()).collect();
    let expected: Vec<&str> = std::iter::once(first.base()).chain(added.iter().map(Ip::base)).collect();
    assert_eq!(order, expected, "порядок одного дня");
    let places: Vec<Option<usize>> = added.iter().map(|ip| place(&ips, ip.base(), &[])).collect();
    let expected: Vec<Option<usize>> = (1..=30).map(Some).collect();
    assert_eq!(places, expected, "места одного дня");
}

/// Имя базы — только своё: первая или `kvartik-ip-<id>` в той записи
/// идентификатора, какой его пишет программа. Запись с чужим именем базы —
/// порча: открыть её окно не должно.
#[test]
fn base_names_are_checked() {
    let id = "5d0c2f7e-8a41-4e3b-9c6d-2b7a1f0e9d34";
    let mut wrong = Vec::new();
    for (name, good) in [
        ("kvartik", true),
        (&*format!("kvartik-ip-{id}"), true),
        (&*format!("kvartik-ip-{}", id.to_uppercase()), false),
        (&*format!("kvartik-ip-{}", id.replace('-', "")), false),
        ("kvartik-ip-", false),
        ("kvartik-office", false),
        ("kvartik-device", false),
        ("", false),
        (" kvartik", false),
    ] {
        if is_base_name(name) != good {
            wrong.push(format!("«{name}»: ждали {good}"));
        }
    }
    let foreign = SAMPLE.replace(&format!("\"base\":\"kvartik-ip-{id}\""), "\"base\":\"kvartik-device\"");
    assert_ne!(foreign, SAMPLE, "подмена сработала");
    if serde_json::from_str::<Ip>(&foreign).is_ok() {
        wrong.push("запись с базой kvartik-device читается".to_owned());
    }
    assert!(wrong.is_empty(), "имена баз: {wrong:#?}");
}

/// Первый ИП — в базе `kvartik` с именем и ՀՎՀՀ своего профиля, новый —
/// в своей базе по идентификатору записи, пустой.
#[test]
fn first_and_new_ips_get_their_bases() {
    let aram = profile("IE Aram Petrosyan", Some("12345678"), 1_000);
    let first = Ip::first(Some(&aram), day(2026, 11, 5), at(2_000));
    assert_eq!(first.base(), FIRST_BASE);
    assert_eq!(first.profile(), Some(aram.id()));
    assert_eq!(first.name(Lang::Am), "IE Aram Petrosyan");
    assert_eq!(first.tin().map(Tin::as_str), Some("12345678"));
    assert_eq!(Ip::first(None, day(2026, 11, 5), at(2_000)).name(Lang::En), "");

    let second = Ip::new(day(2026, 11, 6), at(3_000));
    assert_eq!(second.base(), format!("kvartik-ip-{}", second.id()));
    assert!(is_base_name(second.base()));
    assert_eq!(second.profile(), None);
    assert_eq!(second.added(), day(2026, 11, 6));
    assert_ne!(first.id(), second.id());
}

/// ИП из копии всех ИП ложится в базу по своему идентификатору: у другого
/// устройства своя `kvartik`. Своя запись сливается с записью копии как
/// любая — остаётся изменённая позже, — но имя базы — своё.
#[test]
fn incoming_ips_keep_local_bases() {
    let aram = profile("IE Aram Petrosyan", Some("12345678"), 1_000);
    let theirs = Ip::first(Some(&aram), day(2026, 11, 5), at(2_000));
    let arrived = Ip::arrived(&theirs);
    assert_eq!(arrived.id(), theirs.id());
    assert_eq!(arrived.base(), format!("kvartik-ip-{}", theirs.id()));
    assert_eq!(arrived.name(Lang::En), "IE Aram Petrosyan");

    let mut renamed = profile("IE Aram Petrosyan", Some("12345678"), 1_000);
    let mut newer = theirs.clone();
    let mut data = renamed.data().clone();
    data.name.en = Line::new("IE Aram P.").expect("годное имя");
    renamed = Profile::save(Some(renamed), data, at(4_000)).expect("годный").expect("правка");
    assert!(newer.copy_profile(&renamed, at(5_000)));
    let merged = arrived.merged(&newer).expect("копия новее — сливается");
    assert_eq!(merged.base(), arrived.base(), "имя базы — своё");
    assert_eq!(merged.name(Lang::En), "IE Aram P.");
    assert_eq!(newer.merged(&arrived), None, "своя новее — писать нечего");
    assert_eq!(arrived.merged(&arrived), None, "та же — писать нечего");
    let other = Ip::new(day(2026, 11, 6), at(9_000));
    assert_eq!(arrived.merged(&other), None, "другая запись не сливается");
}

/// Копия профиля обновляется только настоящей правкой: та же — без
/// записи и без нового момента; убрать и вернуть — метка, а не стирание.
#[test]
fn profile_copy_and_removal() {
    let aram = profile("IE Aram Petrosyan", Some("12345678"), 1_000);
    let mut ip = Ip::new(day(2026, 11, 5), at(2_000));
    assert!(ip.copy_profile(&aram, at(3_000)));
    let modified = ip.meta().modified();
    assert!(!ip.copy_profile(&aram, at(4_000)), "тот же профиль — записывать нечего");
    assert_eq!(ip.meta().modified(), modified);
    ip.remove(at(5_000));
    assert!(ip.is_removed());
    ip.bring_back(at(6_000));
    assert!(!ip.is_removed());
    assert_eq!(ip.name(Lang::Ru), "IE Aram Petrosyan");
}

/// Открывается запомненная вкладкой база, потом устройством — если она
/// у живого ИП списка; иначе — ИП, добавленного раньше всех. Убранный,
/// чужой и негодный — не открываются; пустой список — первая база.
#[test]
fn the_tab_opens_a_live_remembered_base() {
    let first = Ip::first(None, day(2026, 11, 1), at(1_000));
    let second = Ip::new(day(2026, 11, 3), at(2_000));
    let mut removed = Ip::new(day(2026, 11, 2), at(3_000));
    removed.remove(at(4_000));
    let ips = [second.clone(), removed.clone(), first.clone()];
    let mut wrong = Vec::new();
    for (what, remembered, expected) in [
        ("вкладка", vec![Some(second.base()), Some(FIRST_BASE)], second.base()),
        ("устройство", vec![None, Some(second.base())], second.base()),
        ("вкладка — убранный", vec![Some(removed.base()), Some(second.base())], second.base()),
        ("ничего не помнит", vec![None, None], FIRST_BASE),
        ("чужое имя", vec![Some("kvartik-ip-x"), Some("other")], FIRST_BASE),
    ] {
        let got = choose(&ips, &remembered);
        if got != expected {
            wrong.push(format!("{what}: {got}, а ждали {expected}"));
        }
    }
    let only_second = [second.clone()];
    if choose(&only_second, &[None]) != second.base() {
        wrong.push("первый живой — не тот".to_owned());
    }
    if choose(&[], &[Some(second.base())]) != FIRST_BASE {
        wrong.push("пустой список — не первая база".to_owned());
    }
    let order: Vec<&str> = alive(&ips).iter().map(|ip| ip.base()).collect();
    if order != [first.base(), second.base()] {
        wrong.push(format!("живые не по дню добавления: {order:?}"));
    }
    assert!(wrong.is_empty(), "выбор базы: {wrong:#?}");
}

/// Место ИП в ключе «Pro» — его номер среди живых по дню добавления, с нуля,
/// как бы список ни лежал в базе: убранный места не занимает, а вернувшийся —
/// снова на своём, впереди добавленных позже. Убранного и чужого среди мест нет.
#[test]
fn places_follow_the_day_added() {
    let first = Ip::first(None, day(2026, 11, 1), at(1_000));
    let second = Ip::new(day(2026, 11, 3), at(2_000));
    let mut early = Ip::new(day(2026, 11, 2), at(3_000));
    early.remove(at(4_000));
    let ips = [second.clone(), early.clone(), first.clone()];
    let mut wrong = Vec::new();
    for (what, base, expected) in [
        ("первый", first.base(), Some(0)),
        ("добавленный позже", second.base(), Some(1)),
        ("убранный", early.base(), None),
        ("чужой", "kvartik-ip-x", None),
    ] {
        if place(&ips, base, &[]) != expected {
            wrong.push(format!("{what}: {:?}", place(&ips, base, &[])));
        }
    }
    early.bring_back(at(5_000));
    let back = [second.clone(), early.clone(), first.clone()];
    if (place(&back, early.base(), &[]), place(&back, second.base(), &[])) != (Some(1), Some(2)) {
        wrong.push(format!(
            "вернувшийся: {:?}, добавленный позже: {:?}",
            place(&back, early.base(), &[]),
            place(&back, second.base(), &[])
        ));
    }
    assert!(wrong.is_empty(), "места: {wrong:#?}");
}

/// ИП со своим действующим ключом места в ключе «Pro» не занимает (решение
/// человека 2026-11-17, задача 103): сначала места получают ИП без своего
/// ключа в порядке добавления, потом — с ключом, тоже по порядку. Убранного
/// среди мест нет и тогда, когда у него свой ключ, а ключ, о котором список
/// не знает, ничего не сдвигает.
#[test]
fn an_own_key_gives_its_seat_to_the_next() {
    let first = Ip::first(None, day(2026, 11, 1), at(1_000));
    let keyed = Ip::new(day(2026, 11, 2), at(2_000));
    let third = Ip::new(day(2026, 11, 3), at(3_000));
    let fourth = Ip::new(day(2026, 11, 4), at(4_000));
    let mut gone = Ip::new(day(2026, 11, 5), at(5_000));
    gone.remove(at(6_000));
    let ips = [fourth.clone(), gone.clone(), third.clone(), keyed.clone(), first.clone()];
    let own = [keyed.base().to_owned(), gone.base().to_owned(), "kvartik-ip-x".to_owned()];
    let mut wrong = Vec::new();
    for (what, base, without, with) in [
        ("первый", first.base(), Some(0), Some(0)),
        ("со своим ключом", keyed.base(), Some(1), Some(3)),
        ("третий", third.base(), Some(2), Some(1)),
        ("четвёртый", fourth.base(), Some(3), Some(2)),
        ("убранный", gone.base(), None, None),
    ] {
        let got = (place(&ips, base, &[]), place(&ips, base, &own));
        if got != (without, with) {
            wrong.push(format!("{what}: без ключей {:?}, с ключами {:?}", got.0, got.1));
        }
    }
    assert!(wrong.is_empty(), "места со своими ключами: {wrong:#?}");
}
