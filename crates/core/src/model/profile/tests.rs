//! Тесты профиля. Образец записи выписан человеком, а не записан кодом:
//! код, записавший себе эталон, проверял бы сам себя (Правило 2).

use super::*;
use crate::model::money::Currency;

fn at(millis: i64) -> Timestamp {
    Timestamp::from_unix_millis(millis)
}

fn day(year: u16, month: u8, day: u8) -> Date {
    Date::new(year, month, day).expect("в тесте — существующая дата")
}

fn line(text: &str) -> Line {
    Line::new(text).expect("в тесте — годный текст")
}

fn lines(text: &str) -> Lines {
    Lines::new(text).expect("в тесте — годный текст")
}

/// Профиль, в котором заполнено всё.
const SAMPLE: &str = concat!(
    r#"{"meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","modified":1760000000000,"deleted":null},"#,
    r#""data":{"#,
    r#""name":{"hy":"ԱՁ Արամ Պետրոսյան","ru":"ИП Арам Петросян","en":"IE Aram Petrosyan"},"#,
    r#""tin":"07654321","registration":"271.1234567","#,
    r#""address":{"hy":"ք. Երևան, Աբովյան 1, բն. 5","ru":"г. Ереван, ул. Абовяна 1, кв. 5","#,
    r#""en":"Abovyan St 1, apt 5\nYerevan 0001, Armenia"},"#,
    r#""phone":"+374 91 123456","email":"aram@example.com","#,
    r#""accounts":[{"currency":"USD","number":"127027482877","bank":"Ameriabank CJSC","#,
    r#""swift":"ARMIAM22","correspondent":"Citibank N.A., New York\nSWIFT: CITIUS33"}],"#,
    r#""activities":["62.01.0","70.22.0"],"kinds":{"70.22.0":"other"},"regime":"turnover","#,
    r#""person":{"birth_year":1970,"pension_joined":true,"pension_applied":"2019-03-14","#,
    r#""residency":"foreign_with_status","resident":false,"registered":"2022-05-12","#,
    r#""stops":[{"from":"2026-03-01","until":"2026-06-01"},{"from":"2026-09-15","until":null}]}"#,
    r#"}}"#
);

/// Пустой профиль — ничего не заполнено.
const EMPTY: &str = concat!(
    r#"{"meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","modified":1760000000000,"deleted":null},"#,
    r#""data":{"name":{"hy":"","ru":"","en":""},"tin":null,"registration":"","#,
    r#""address":{"hy":"","ru":"","en":""},"phone":"","email":null,"accounts":[],"#,
    r#""activities":[],"kinds":{},"regime":null,"#,
    r#""person":{"birth_year":null,"pension_joined":null,"pension_applied":null,"residency":null,"#,
    r#""resident":null,"registered":null,"stops":[]}}}"#
);

/// Образцы читаются в ожидаемые значения, а их запись — слово в слово
/// образцы. Поменялась запись — это новая версия формата (Правило 5),
/// а не правка образца.
#[test]
fn data_shape_is_pinned() {
    let profile: Profile = serde_json::from_str(SAMPLE).expect("образец читается");
    let data = profile.data();
    assert_eq!(data.name.get(Lang::Am).as_str(), "ԱՁ Արամ Պետրոսյան");
    assert_eq!(data.name.get(Lang::En).as_str(), "IE Aram Petrosyan");
    assert_eq!(data.tin.as_ref().map(Tin::as_str), Some("07654321"));
    assert_eq!(data.address.get(Lang::En).as_str(), "Abovyan St 1, apt 5\nYerevan 0001, Armenia");
    assert_eq!(data.email.as_ref().map(Email::as_str), Some("aram@example.com"));
    assert_eq!(data.accounts.len(), 1);
    assert_eq!(data.accounts[0].currency(), Currency::Usd);
    let codes: Vec<String> = data.activities.iter().map(ToString::to_string).collect();
    assert_eq!(codes, ["62.01.0", "70.22.0"]);
    assert_eq!(
        data.kinds,
        BTreeMap::from([(data.activities[1], IncomeKind::Other)])
    );
    assert_eq!(data.regime, Some(Regime::Turnover));
    assert_eq!(data.person.birth_year, Some(1970));
    assert_eq!(data.person.pension_joined, Some(true));
    assert_eq!(data.person.pension_applied, Some(day(2019, 3, 14)));
    assert_eq!(data.person.residency, Some(Residency::ForeignWithStatus));
    assert_eq!(data.person.resident, Some(false));
    assert_eq!(data.person.registered, Some(day(2022, 5, 12)));
    assert_eq!(
        data.person.stops,
        [
            Stop::new(day(2026, 3, 1), Some(day(2026, 6, 1))).expect("период годен"),
            Stop::new(day(2026, 9, 15), None).expect("период годен"),
        ]
    );
    assert_eq!(serde_json::to_string(&profile).ok().as_deref(), Some(SAMPLE));

    let empty: Profile = serde_json::from_str(EMPTY).expect("пустой читается");
    assert_eq!(empty.data(), &ProfileData::default());
    assert_eq!(serde_json::to_string(&empty).ok().as_deref(), Some(EMPTY));
}

/// Профиль версии формата 7 — без строк налога у кодов — читается с пустым
/// выбором: строки решают правила (Правило 2).
#[test]
fn profiles_without_kinds_still_read() {
    let old = SAMPLE.replacen(r#""kinds":{"70.22.0":"other"},"#, "", 1);
    assert!(!old.contains("kinds"), "{old}");
    let profile: Profile = serde_json::from_str(&old).expect("профиль версии 7 читается");
    assert!(profile.data().kinds.is_empty());
}

/// Профиль версии формата 8 — без дня регистрации ИП — читается с днём «не
/// указан» (Правило 2): годовые платежи тогда считаются с января и окно
/// говорит это словами (`tax::annual`).
#[test]
fn profiles_without_registration_day_still_read() {
    let old = SAMPLE.replacen(r#""registered":"2022-05-12","#, "", 1);
    assert!(!old.contains("registered"), "{old}");
    let profile: Profile = serde_json::from_str(&old).expect("профиль версии 8 читается");
    assert_eq!(profile.data().person.registered, None);
}

/// Профиль версии формата 13 — без дня заявления о фонде и ответа
/// о резидентстве — читается с «не указано» (Правило 2): `None`, а не «нет»
/// (Правило 6).
#[test]
fn profiles_without_pension_day_and_residence_still_read() {
    let old = SAMPLE
        .replacen(r#""pension_applied":"2019-03-14","#, "", 1)
        .replacen(r#""resident":false,"#, "", 1);
    assert!(!old.contains("pension_applied") && !old.contains("resident"), "{old}");
    let profile: Profile = serde_json::from_str(&old).expect("профиль версии 13 читается");
    assert_eq!(profile.data().person.pension_applied, None);
    assert_eq!(profile.data().person.resident, None);
}

/// Строка налога выбирается только коду из профиля: у кода, которого нет,
/// — порча, и команда её не сохранит.
#[test]
fn kinds_belong_to_listed_codes() {
    let stray = SAMPLE.replacen(r#""kinds":{"70.22.0":"other"}"#, r#""kinds":{"74.10.0":"other"}"#, 1);
    assert!(serde_json::from_str::<Profile>(&stray).is_err());
    let unknown = SAMPLE.replacen(r#""70.22.0":"other""#, r#""70.22.0":"luxury""#, 1);
    assert!(serde_json::from_str::<Profile>(&unknown).is_err());
    let mut data = ProfileData::default();
    data.kinds.insert(ActivityCode::parse("47.91.0").expect("код"), IncomeKind::Trade);
    assert_eq!(Profile::save(None, data.clone(), at(1_000)), Err(ProfileError::StrayKind));
    data.activities.push(ActivityCode::parse("47.91.0").expect("код"));
    assert!(Profile::save(None, data, at(1_000)).is_ok());
}

/// Испорченное не читается: незнакомое поле, запретное значение в любом
/// поле, список длиннее предела.
#[test]
fn data_rejects_what_the_types_forbid() {
    let accounts = format!(
        r#""accounts":[{}]"#,
        vec![
            r#"{"currency":"AMD","number":"1","bank":"","swift":null,"correspondent":""}"#;
            ProfileData::MAX_ACCOUNTS + 1
        ]
        .join(",")
    );
    let cases = [
        SAMPLE.replace(r#""regime""#, r#""vat":true,"regime""#),
        SAMPLE.replace(r#""tin":"07654321""#, r#""tin":"7654321""#),
        SAMPLE.replace(r#""tin":"07654321""#, r#""tin":7654321"#),
        SAMPLE.replace(r#""62.01.0""#, r#""J62.01.0""#),
        SAMPLE.replace(r#""62.01.0""#, r#""04.00.0""#),
        SAMPLE.replace(r#""until":"2026-06-01""#, r#""until":"2026-03-01""#),
        SAMPLE.replace("aram@example.com", "aram"),
        SAMPLE.replace("ИП Арам Петросян", "ИП\tАрам"),
        SAMPLE.replace(r#""turnover""#, r#""micro""#),
        SAMPLE.replace(r#""foreign_with_status""#, r#""resident""#),
        SAMPLE.replace(r#""birth_year":1970"#, r#""birth_year":"1970""#),
        SAMPLE.replace(r#""registered":"2022-05-12""#, r#""registered":"2022-02-30""#),
        SAMPLE.replace(r#""registered":"2022-05-12""#, r#""registered":"12.05.2022""#),
        SAMPLE.replace(r#""swift":"ARMIAM22""#, r#""swift":"armiam22""#),
        SAMPLE.replace(r#""accounts":[{"currency":"USD","number":"127027482877","bank":"Ameriabank CJSC","swift":"ARMIAM22","correspondent":"Citibank N.A., New York\nSWIFT: CITIUS33"}]"#, &accounts),
        SAMPLE.replace(r#""deleted":null},"data""#, r#""deleted":null},"extra":1,"data""#),
        SAMPLE.replace(r#""en":"IE Aram Petrosyan"},"#, r#""en":"IE Aram Petrosyan","fr":""},"#),
    ];
    let mut wrong = Vec::new();
    for (index, text) in cases.iter().enumerate() {
        if text == SAMPLE {
            wrong.push(format!("случай {index} не изменил образец"));
        } else if serde_json::from_str::<Profile>(text).is_ok() {
            wrong.push(format!("случай {index} прочитан"));
        }
    }
    assert!(wrong.is_empty(), "прочитано испорченное: {wrong:#?}");
}

/// Команда «сохранить»: новый профиль — новая запись; правка — тот же
/// идентификатор и новый момент; без изменений — записывать нечего.
#[test]
fn save_creates_edits_and_skips_no_op() {
    let mut data = ProfileData {
        regime: Some(Regime::Turnover),
        ..ProfileData::default()
    };
    let created = Profile::save(None, data.clone(), at(1_000))
        .expect("годно")
        .expect("новый профиль записывается");
    assert_eq!(created.meta().modified(), at(1_000));
    assert!(!created.meta().is_deleted());

    assert_eq!(
        Profile::save(Some(created.clone()), data.clone(), at(2_000)),
        Ok(None),
        "без изменений записывать нечего"
    );

    data.phone = line("+374 91 123456");
    let edited = Profile::save(Some(created.clone()), data.clone(), at(3_000))
        .expect("годно")
        .expect("правка записывается");
    assert_eq!(edited.id(), created.id());
    assert_eq!(edited.meta().modified(), at(3_000));
    assert_eq!(edited.data(), &data);

    data.accounts = vec![
        BankAccount::new(Currency::Amd, line("1"), Line::default(), None, Lines::default())
            .expect("счёт годен");
        ProfileData::MAX_ACCOUNTS + 1
    ];
    assert_eq!(
        Profile::save(Some(edited), data, at(4_000)),
        Err(ProfileError::TooMany)
    );
}

/// Профиль окна — живой и изменённый последним.
#[test]
fn current_is_the_latest_alive() {
    let make = |modified: i64, deleted: Option<i64>| -> Profile {
        let deleted = deleted.map_or_else(|| "null".to_owned(), |d| d.to_string());
        serde_json::from_str(&EMPTY.replace(
            r#""modified":1760000000000,"deleted":null"#,
            &format!(r#""modified":{modified},"deleted":{deleted}"#),
        ))
        .expect("профиль пробы читается")
    };
    assert_eq!(Profile::current(Vec::new()), None);
    let older = make(1_000, None);
    let newer = make(2_000, None);
    let deleted = make(3_000, Some(3_000));
    let current = Profile::current(vec![older, deleted, newer.clone()]);
    assert_eq!(current, Some(newer));
    assert_eq!(Profile::current(vec![make(5, Some(5))]), None);
}

/// Адрес почты: что читается и что нет.
#[test]
fn email_is_checked_loosely() {
    let read = ["aram@example.com", " aram.p+inv@mail.example.am ", "a@b.co"];
    let rejected = [
        "", "aram", "@example.com", "aram@", "aram@example", "aram@.com", "aram@example.",
        "ar am@example.com", "aram@@example.com", "a@b@c.com",
    ];
    let mut wrong = Vec::new();
    for text in read {
        match Email::parse(text) {
            Ok(email) if email.as_str() == text.trim() => {}
            other => wrong.push(format!("«{text}»: {other:?}")),
        }
    }
    for text in rejected {
        if let Ok(email) = Email::parse(text) {
            wrong.push(format!("«{text}» принят как {email:?}"));
        }
    }
    assert!(wrong.is_empty(), "почта: {wrong:#?}");
}

/// День возобновления — позже дня остановки, или его нет.
#[test]
fn stop_resumes_after_it_starts() {
    assert!(Stop::new(day(2026, 3, 1), None).is_ok());
    assert!(Stop::new(day(2026, 3, 1), Some(day(2026, 3, 2))).is_ok());
    assert_eq!(
        Stop::new(day(2026, 3, 1), Some(day(2026, 3, 1))),
        Err(StopError::Backwards)
    );
    assert_eq!(
        Stop::new(day(2026, 3, 1), Some(day(2026, 2, 1))),
        Err(StopError::Backwards)
    );
}

/// Текст на языке — по коду языка, а не по месту в списке.
#[test]
fn localized_follows_the_language() {
    let name = Localized {
        hy: line("ԱՁ"),
        ru: line("ИП"),
        en: line("IE"),
    };
    let got: Vec<&str> = Lang::ALL.iter().map(|lang| name.get(*lang).as_str()).collect();
    assert_eq!(got, ["ԱՁ", "ИП", "IE"]);
    let address = Localized {
        hy: Lines::default(),
        ru: Lines::default(),
        en: lines("Yerevan\nArmenia"),
    };
    assert_eq!(address.get(Lang::En).as_str(), "Yerevan\nArmenia");
}
