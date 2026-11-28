use super::*;
use crate::format::backup::{KINDS, OFFICE_KINDS, Raw, collect};
use crate::format::encode;
use crate::i18n::Lang;
use crate::model::date::Date;
use crate::model::office::{FIRST_BASE, Ip};
use crate::model::profile::{Profile, ProfileData};
use crate::model::record::Timestamp;
use crate::model::text::Line;
use crate::model::tin::Tin;

fn at(millis: i64) -> Timestamp {
    Timestamp::from_unix_millis(millis)
}

fn today() -> Date {
    Date::new(2026, 11, 5).expect("в тесте — существующая дата")
}

fn profile(name: &str, tin: Option<&str>, millis: i64) -> Profile {
    let mut data = ProfileData::default();
    data.name.en = Line::new(name).expect("в тесте — годное имя");
    data.tin = tin.map(|tin| Tin::parse(tin).expect("в тесте — годный ՀՎՀՀ"));
    Profile::save(None, data, at(millis))
        .expect("годный профиль")
        .expect("новый профиль")
}

/// ИП с копией этого профиля в своей базе.
fn ip_of(profile: Option<&Profile>, millis: i64) -> Ip {
    let mut ip = Ip::new(today(), at(millis));
    if let Some(profile) = profile {
        ip.copy_profile(profile, at(millis));
    }
    ip
}

/// Запись копии — как её разобрал бы файл.
fn entry<T: Stored + serde::Serialize>(kinds: &[crate::format::backup::Kind], record: &T) -> Entry {
    let id = record.meta().id().to_string();
    let stored = encode(record);
    collect(
        kinds,
        [Raw {
            kind: T::KIND,
            id: &id,
            text: Some(&stored),
        }],
    )
    .expect("собрано")
    .entries()[0]
    .clone()
}

/// Копия всех ИП: записи конторы и профиль каждого ИП с меткой его базы.
fn full_backup(ips: &[(&Ip, &Profile)]) -> Vec<Entry> {
    let office = ips.iter().map(|(ip, _)| entry(OFFICE_KINDS, *ip));
    let bases = ips
        .iter()
        .map(|(ip, profile)| entry(KINDS, *profile).in_base(ip.id()));
    office.chain(bases).collect()
}

/// Где лёг профиль `profile` и новая ли это база.
fn landed(route: &Route, profile: &Profile) -> Option<(String, bool)> {
    route
        .bases
        .iter()
        .find(|landing| {
            landing
                .entries
                .iter()
                .any(|entry| entry.meta().id() == profile.id())
        })
        .map(|landing| (landing.base.clone(), landing.new))
}

/// ИП копии ложится в свою базу: по той же записи ИП, по тому же профилю,
/// по тому же ՀՎՀՀ, — иначе в новую базу по идентификатору записи, а не
/// в `kvartik`: она здесь своя. В список идут только новые ИП и записи,
/// изменённые в копии позже, — с местным именем базы.
#[test]
fn incoming_ips_find_their_bases() {
    let aram = profile("IE Aram Petrosyan", Some("12345678"), 1_000);
    let anna = profile("IE Anna Hakobyan", Some("87654321"), 1_000);
    let vahe = profile("IE Vahe Sargsyan", Some("11223344"), 1_000);
    let lilit = profile("IE Lilit Avagyan", None, 1_000);

    // Этот ИП есть здесь той же записью; в копии она изменена позже.
    let aram_here = ip_of(Some(&aram), 2_000);
    let mut aram_there = Ip::arrived(&aram_here);
    let mut renamed = aram.data().clone();
    renamed.name.en = Line::new("IE Aram P.").expect("годное имя");
    let aram_renamed = Profile::save(Some(aram.clone()), renamed, at(3_000))
        .expect("годный")
        .expect("правка");
    aram_there.copy_profile(&aram_renamed, at(4_000));
    // Тот же ИП, заведённый здесь и там независимо: тот же профиль.
    let anna_here = Ip::first(Some(&anna), today(), at(2_000));
    let anna_there = ip_of(Some(&anna), 2_500);
    // Тот же ՀՎՀՀ, а профиль здесь ещё не прочитан.
    let mut vahe_here = Ip::new(today(), at(2_000));
    let mut vahe_copy = vahe.data().clone();
    vahe_copy.name.en = Line::new("IE Vahe").expect("годное имя");
    let vahe_other = Profile::save(None, vahe_copy, at(1_500)).expect("годный").expect("новый");
    vahe_here.copy_profile(&vahe_other, at(2_000));
    let vahe_there = ip_of(Some(&vahe), 2_500);
    // Первый ИП другого устройства — там он в `kvartik`.
    let lilit_there = Ip::first(Some(&lilit), today(), at(2_500));

    let local = [aram_here.clone(), anna_here.clone(), vahe_here.clone()];
    let incoming = full_backup(&[
        (&aram_there, &aram_renamed),
        (&anna_there, &anna),
        (&vahe_there, &vahe),
        (&lilit_there, &lilit),
    ]);
    let route = route(incoming, &local, None).expect("маршрут есть");
    let mut wrong = Vec::new();
    for (who, profile, expected) in [
        ("та же запись", &aram_renamed, (aram_here.base().to_owned(), false)),
        ("тот же профиль", &anna, (FIRST_BASE.to_owned(), false)),
        ("тот же ՀՎՀՀ", &vahe, (vahe_here.base().to_owned(), false)),
        (
            "новый",
            &lilit,
            (format!("kvartik-ip-{}", lilit_there.id()), true),
        ),
    ] {
        let got = landed(&route, profile);
        if got.as_ref() != Some(&expected) {
            wrong.push(format!("{who}: {got:?}, а ждали {expected:?}"));
        }
    }
    let written: Vec<(String, String)> = route
        .office
        .iter()
        .map(|ip| (ip.base().to_owned(), ip.name(Lang::En).to_owned()))
        .collect();
    let expected = vec![
        (aram_here.base().to_owned(), "IE Aram P.".to_owned()),
        (format!("kvartik-ip-{}", lilit_there.id()), "IE Lilit Avagyan".to_owned()),
    ];
    if written != expected {
        wrong.push(format!("в список: {written:?}, а ждали {expected:?}"));
    }
    if route.bases.iter().any(|landing| landing.entries.iter().any(|entry| entry.kind() == Ip::KIND)) {
        wrong.push("запись конторы легла в базу ИП".to_owned());
    }
    if route.bases.iter().any(|landing| landing.entries.iter().any(|entry| entry.base().is_some())) {
        wrong.push("запись легла в базу с меткой копии".to_owned());
    }
    if route.bases.len() != 4 {
        wrong.push(format!("баз {} вместо 4", route.bases.len()));
    }
    assert!(wrong.is_empty(), "маршрут копии всех ИП: {wrong:#?}");
}

/// Больше `MAX_IPS` ИП копия не заводит — отказ до первой записи, — а ИП,
/// которые уже здесь, к пределу не прибавляются.
#[test]
fn too_many_ips_are_refused() {
    let local: Vec<Ip> = (0..MAX_IPS as i64).map(|n| ip_of(None, 1_000 + n)).collect();
    let anna = profile("IE Anna Hakobyan", Some("87654321"), 1_000);
    let known = Ip::arrived(&local[3]);
    let known_profile = profile("IE Known", None, 1_000);
    assert!(
        route(full_backup(&[(&known, &known_profile)]), &local, None).is_ok(),
        "тот же ИП места не берёт"
    );
    let stranger = ip_of(Some(&anna), 5_000);
    assert_eq!(
        route(full_backup(&[(&stranger, &anna)]), &local, None).map(|route| route.office.len()),
        Err(RouteError::TooMany(MAX_IPS + 1))
    );
}

/// Контора с ключом «Pro» на все `license::MAX_SEATS` мест, убравшая из
/// списка хоть одного ИП, принимает и сотого живого (задача 90): убранные
/// в пределе считаются, а до 0.0.216 предел был равен числу мест, и
/// бухгалтер беты со ста местами упирался в него, убрав первого же клиента.
#[test]
fn a_full_pro_office_with_a_removed_ip_takes_one_more() {
    let seats = usize::from(crate::license::MAX_SEATS);
    let mut local: Vec<Ip> = (0..seats as i64).map(|n| ip_of(None, 1_000 + n)).collect();
    local[0].remove(at(2_000));
    let anna = profile("IE Anna Hakobyan", Some("87654321"), 1_000);
    let stranger = ip_of(Some(&anna), 5_000);
    assert_eq!(
        route(full_backup(&[(&stranger, &anna)]), &local, None).map(|route| route.office.len()),
        Ok(1),
        "живых ИП стало бы {seats}"
    );
}

/// Свежее устройство: списка нет, своя `kvartik` пуста — в неё ложится ИП
/// копии, добавленный раньше всех из тех, у кого здесь нет пары, а не встаёт
/// рядом с пустой базой. Остальные — в новые базы, убранный — тоже.
#[test]
fn a_spare_empty_base_takes_the_earliest_newcomer() {
    let aram = profile("IE Aram Petrosyan", Some("12345678"), 1_000);
    let anna = profile("IE Anna Hakobyan", Some("87654321"), 1_000);
    let mut later = Ip::new(Date::new(2026, 11, 3).expect("дата"), at(2_000));
    later.copy_profile(&aram, at(2_000));
    let mut earlier = Ip::new(Date::new(2026, 11, 1).expect("дата"), at(3_000));
    earlier.copy_profile(&anna, at(3_000));
    let spared = route(full_backup(&[(&later, &aram), (&earlier, &anna)]), &[], Some(FIRST_BASE)).expect("маршрут");
    let mut wrong = Vec::new();
    if landed(&spared, &anna) != Some((FIRST_BASE.to_owned(), false)) {
        wrong.push(format!("ранний — не в свободную базу: {:?}", landed(&spared, &anna)));
    }
    if landed(&spared, &aram) != Some((format!("kvartik-ip-{}", later.id()), true)) {
        wrong.push(format!("поздний — не в свою новую: {:?}", landed(&spared, &aram)));
    }
    let written: Vec<(RecordId, String)> = spared.office.iter().map(|ip| (ip.id(), ip.base().to_owned())).collect();
    let expected = vec![
        (later.id(), format!("kvartik-ip-{}", later.id())),
        (earlier.id(), FIRST_BASE.to_owned()),
    ];
    if written != expected {
        wrong.push(format!("в список: {written:?}, а ждали {expected:?}"));
    }
    let without = route(full_backup(&[(&earlier, &anna)]), &[], None).expect("маршрут");
    if landed(&without, &anna) != Some((format!("kvartik-ip-{}", earlier.id()), true)) {
        wrong.push("без свободной базы — не в новую".to_owned());
    }
    assert!(wrong.is_empty(), "свободная база: {wrong:#?}");
}

/// Свободная база, которая по имени — база одного из ИП копии, достаётся
/// ему, а не раннему: два ИП в одной базе смешали бы книги.
#[test]
fn a_spare_base_named_after_an_ip_goes_to_that_ip() {
    let aram = profile("IE Aram Petrosyan", Some("12345678"), 1_000);
    let anna = profile("IE Anna Hakobyan", Some("87654321"), 1_000);
    let mut own = Ip::new(Date::new(2026, 11, 3).expect("дата"), at(2_000));
    own.copy_profile(&aram, at(2_000));
    let mut earlier = Ip::new(Date::new(2026, 11, 1).expect("дата"), at(3_000));
    earlier.copy_profile(&anna, at(3_000));
    let spare = format!("kvartik-ip-{}", own.id());
    let spared = route(full_backup(&[(&own, &aram), (&earlier, &anna)]), &[], Some(&spare)).expect("маршрут");
    assert_eq!(landed(&spared, &aram), Some((spare.clone(), false)));
    assert_eq!(landed(&spared, &anna), Some((format!("kvartik-ip-{}", earlier.id()), true)));
}

/// Свободная `kvartik` достаётся ИП, у которого на устройстве копии база
/// так и называлась, — первому ИП того устройства, — даже если другой ИП
/// добавлен раньше: в один день их различал бы только случайный
/// идентификатор.
#[test]
fn a_spare_first_base_goes_to_the_first_ip_of_the_source() {
    let aram = profile("IE Aram Petrosyan", Some("12345678"), 1_000);
    let anna = profile("IE Anna Hakobyan", Some("87654321"), 1_000);
    let first = Ip::first(Some(&aram), Date::new(2026, 11, 6).expect("дата"), at(2_000));
    let mut earlier = Ip::new(Date::new(2026, 11, 1).expect("дата"), at(3_000));
    earlier.copy_profile(&anna, at(3_000));
    let spared = route(full_backup(&[(&earlier, &anna), (&first, &aram)]), &[], Some(FIRST_BASE)).expect("маршрут");
    assert_eq!(landed(&spared, &aram), Some((FIRST_BASE.to_owned(), false)));
    assert_eq!(landed(&spared, &anna), Some((format!("kvartik-ip-{}", earlier.id()), true)));
}

/// Тот же ИП дважды в источнике (по профилю — в одну базу здесь) с одной
/// записью в двух версиях: в базу идёт одна — изменённая позже, на месте
/// первой, — а не последняя в файле.
#[test]
fn one_record_of_two_ips_in_one_base_is_the_later_one() {
    let aram = profile("IE Aram Petrosyan", Some("12345678"), 1_000);
    let mut here = Ip::new(Date::new(2026, 11, 1).expect("дата"), at(2_000));
    here.copy_profile(&aram, at(2_000));
    let mut twin_a = Ip::new(Date::new(2026, 11, 2).expect("дата"), at(3_000));
    twin_a.copy_profile(&aram, at(3_000));
    let mut twin_b = Ip::new(Date::new(2026, 11, 3).expect("дата"), at(3_000));
    twin_b.copy_profile(&aram, at(3_000));
    let mut edited = aram.data().clone();
    edited.name.en = Line::new("IE Aram P.").expect("годное имя");
    let later = Profile::save(Some(aram.clone()), edited, at(5_000)).expect("годный").expect("правка");
    // В файле сначала поздняя версия, потом ранняя: последней в файле лежит
    // ранняя.
    let mut incoming = full_backup(&[(&twin_a, &later)]);
    incoming.extend(full_backup(&[(&twin_b, &aram)]));
    let routed = route(incoming, std::slice::from_ref(&here), None).expect("маршрут");
    assert_eq!(routed.bases.len(), 1, "оба — в базу того же ИП");
    let profiles: Vec<&Entry> = routed.bases[0]
        .entries
        .iter()
        .filter(|entry| entry.kind() == Profile::KIND)
        .collect();
    assert_eq!(profiles.len(), 1, "одна запись — одна");
    assert_eq!(profiles[0].meta().modified(), at(5_000), "изменённая позже");
}

/// Ключи «Pro» копии идут в контору, а не в базу ИП — и тот, что пришёл
/// с меткой базы: она ему ничего не значит, и в контору он ложится без неё.
/// Из двух версий одной записи — изменённая позже.
#[test]
fn pro_keys_go_to_the_office() {
    let aram = profile("IE Aram Petrosyan", Some("12345678"), 1_000);
    let ip = ip_of(Some(&aram), 2_000);
    let day = |month, day| Date::new(2026, month, day).expect("дата");
    let key = |until| {
        let tin = Tin::parse("07654321").expect("годный ՀՎՀՀ");
        crate::license::Key::signed(tin, crate::license::Plan::Pro, day(11, 1), until, Some(30), [0; 64])
            .expect("ключ Pro")
    };
    let first = OfficeLicense::enter(key(day(12, 31)), &[], at(3_000)).expect("ключ Pro");
    let second = OfficeLicense::enter(key(day(12, 30)), &[], at(3_000)).expect("ключ Pro");
    // Та же запись, изменённая позже, — в файле раньше прежней.
    let later: OfficeLicense =
        crate::format::decode(&encode(&first).replace(r#""modified":3000"#, r#""modified":4000"#)).expect("читается");
    let mut incoming = full_backup(&[(&ip, &aram)]);
    incoming.push(entry(OFFICE_KINDS, &later));
    incoming.push(entry(OFFICE_KINDS, &first));
    incoming.push(entry(OFFICE_KINDS, &second).in_base(ip.id()));
    let routed = route(incoming, &[], None).expect("маршрут");
    let mut wrong = Vec::new();
    let keys: Vec<(RecordId, Timestamp, Option<RecordId>)> = routed
        .keys
        .iter()
        .map(|entry| (entry.meta().id(), entry.meta().modified(), entry.base()))
        .collect();
    if keys != [(first.id(), at(4_000), None), (second.id(), at(3_000), None)] {
        wrong.push(format!("ключи конторы: {keys:?}"));
    }
    let in_bases: Vec<&str> = routed
        .bases
        .iter()
        .flat_map(|landing| landing.entries.iter().map(Entry::kind))
        .filter(|kind| *kind == OfficeLicense::KIND)
        .collect();
    if !in_bases.is_empty() {
        wrong.push(format!("ключ Pro в базе ИП: {in_bases:?}"));
    }
    if landed(&routed, &aram).is_none() {
        wrong.push("профиль ИП не лёг".to_owned());
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// Копию с ключами «Pro» сверх `MAX_KEYS` маршрут не кладёт — их проверка
/// держала бы страницу (ревью задачи 38), — а ровно `MAX_KEYS` кладёт.
#[test]
fn too_many_pro_keys_are_refused() {
    let aram = profile("IE Aram Petrosyan", Some("12345678"), 1_000);
    let ip = ip_of(Some(&aram), 2_000);
    let issued = Date::new(2026, 11, 1).expect("дата");
    let backup = |count: usize| {
        let mut incoming = full_backup(&[(&ip, &aram)]);
        for n in 0..count {
            let until = issued.add_days(i32::try_from(n).expect("число")).expect("дата");
            let tin = Tin::parse("07654321").expect("годный ՀՎՀՀ");
            let key = crate::license::Key::signed(tin, crate::license::Plan::Pro, issued, until, Some(30), [0; 64])
                .expect("ключ Pro");
            let license = OfficeLicense::enter(key, &[], at(3_000)).expect("ключ Pro");
            incoming.push(entry(OFFICE_KINDS, &license));
        }
        incoming
    };
    assert!(matches!(
        route(backup(crate::license::MAX_KEYS + 1), &[], None),
        Err(RouteError::TooManyKeys(count)) if count == crate::license::MAX_KEYS + 1
    ));
    let fits = route(backup(crate::license::MAX_KEYS), &[], None).expect("ровно предел ложится");
    assert_eq!(fits.keys.len(), crate::license::MAX_KEYS);
}

/// Ключей «Стандарт» одного ИП сверх `MAX_KEYS` маршрут тоже не кладёт — до
/// первой записи, а не когда список и прежние базы уже легли: повтор упал бы
/// так же (ревью задачи 38), — а ровно `MAX_KEYS` кладёт.
#[test]
fn too_many_keys_of_one_ie_are_refused() {
    let aram = profile("IE Aram Petrosyan", Some("12345678"), 1_000);
    let ip = ip_of(Some(&aram), 2_000);
    let issued = Date::new(2026, 11, 1).expect("дата");
    let backup = |count: usize| {
        let mut incoming = full_backup(&[(&ip, &aram)]);
        for n in 0..count {
            let until = issued.add_days(i32::try_from(n).expect("число")).expect("дата");
            let tin = Tin::parse("12345678").expect("годный ՀՎՀՀ");
            let key = crate::license::Key::signed(tin, crate::license::Plan::Standard, issued, until, None, [0; 64])
                .expect("ключ");
            let license = License::enter(key, &[], at(3_000)).expect("ключ");
            incoming.push(entry(KINDS, &license).in_base(ip.id()));
        }
        incoming
    };
    assert!(matches!(
        route(backup(crate::license::MAX_KEYS + 1), &[], None),
        Err(RouteError::TooManyKeys(count)) if count == crate::license::MAX_KEYS + 1
    ));
    let fits = route(backup(crate::license::MAX_KEYS), &[], None).expect("ровно предел ложится");
    let keys = fits.bases[0]
        .entries
        .iter()
        .filter(|entry| entry.kind() == License::KIND)
        .count();
    assert_eq!(keys, crate::license::MAX_KEYS);
}

/// Копию одного ИП узнают в списке по профилю и по ՀՎՀՀ; чужой ИП — `None`.
/// Записи ИП из записей конторы — все, по порядку.
#[test]
fn a_single_backup_is_found_on_the_list() {
    let aram = profile("IE Aram Petrosyan", Some("12345678"), 1_000);
    let anna = profile("IE Anna Hakobyan", Some("87654321"), 1_000);
    let vahe = profile("IE Vahe Sargsyan", None, 1_000);
    let mut by_profile = Ip::new(Date::new(2026, 11, 1).expect("дата"), at(2_000));
    by_profile.copy_profile(&aram, at(2_000));
    // Тот же ՀՎՀՀ, профиль заведён заново.
    let mut anna_again = anna.data().clone();
    anna_again.name.en = Line::new("IE Anna").expect("годное имя");
    let anna_other = Profile::save(None, anna_again, at(1_500)).expect("годный").expect("новый");
    let mut by_tin = Ip::new(Date::new(2026, 11, 2).expect("дата"), at(2_000));
    by_tin.copy_profile(&anna_other, at(2_000));
    let local = [by_profile.clone(), by_tin.clone()];
    let single = |profile: &Profile| vec![entry(KINDS, profile)];
    let mut wrong = Vec::new();
    for (what, copy, expected) in [
        ("по профилю", single(&aram), Some(by_profile.id())),
        ("по ՀՎՀՀ", single(&anna), Some(by_tin.id())),
        ("чужой", single(&vahe), None),
        ("без профиля", Vec::new(), None),
    ] {
        let got = listed(&copy, &local).expect("читается").map(Ip::id);
        if got != expected {
            wrong.push(format!("{what}: {got:?}"));
        }
    }
    let office: Vec<Entry> = local.iter().map(|ip| entry(OFFICE_KINDS, ip)).collect();
    let read: Vec<RecordId> = ips(&office).expect("читаются").iter().map(Ip::id).collect();
    if read != [by_profile.id(), by_tin.id()] {
        wrong.push(format!("записи ИП из конторы: {read:?}"));
    }
    assert!(wrong.is_empty(), "копия одного ИП в списке: {wrong:#?}");
}

/// Копию одного ИП кладут новым ИП — в список идёт профиль копии, с которым
/// окно будет работать: живой, изменённый последним. Без профиля — `None`.
#[test]
fn a_single_backup_names_its_new_ip() {
    let older = profile("IE Vahe", Some("12345678"), 1_000);
    let later = profile("IE Vahe Sargsyan", Some("12345678"), 2_000);
    let copy = vec![entry(KINDS, &later), entry(KINDS, &older)];
    let mut wrong = Vec::new();
    match super::profile(&copy).expect("читается") {
        Some(found) if found.id() == later.id() => {}
        found => wrong.push(format!("не изменённый последним: {:?}", found.map(|found| found.id()))),
    }
    if super::profile(&[]).expect("читается").is_some() {
        wrong.push("профиль у копии без профиля".to_owned());
    }
    let mut new = Ip::new(today(), at(3_000));
    if let Some(found) = super::profile(&copy).expect("читается") {
        new.copy_profile(&found, at(3_000));
    }
    if new.name(Lang::En) != "IE Vahe Sargsyan" || listed(&copy, std::slice::from_ref(&new)).expect("читается").is_none() {
        wrong.push(format!("новый ИП без имени или не узнан: {:?}", new.name(Lang::En)));
    }
    assert!(wrong.is_empty(), "профиль копии одного ИП: {wrong:#?}");
}
