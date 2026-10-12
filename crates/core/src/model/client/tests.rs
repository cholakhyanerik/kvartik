//! Тесты клиента. Образцы записи выписаны человеком, а не записаны кодом:
//! код, записавший себе эталон, проверял бы сам себя (Правило 2).

use super::*;

fn at(millis: i64) -> Timestamp {
    Timestamp::from_unix_millis(millis)
}

fn line(text: &str) -> Line {
    Line::new(text).expect("в тесте — годный текст")
}

/// Данные клиента с этим названием; остальное не заполнено.
fn named(name: &str) -> ClientData {
    ClientData {
        name: line(name),
        country: None,
        address: Lines::default(),
        tin: None,
        currency: None,
        lang: None,
        payment_days: None,
    }
}

/// Армянская фирма, у которой заполнено всё.
const SAMPLE: &str = concat!(
    r#"{"meta":{"id":"5b0e8f9a-3c2d-4e1f-8a7b-9c6d5e4f3a2b","modified":1760000000000,"deleted":null},"#,
    r#""data":{"name":"«Արարատ» ՍՊԸ","country":"AM","#,
    r#""address":"ք. Երևան, Տիգրան Մեծի 10\n0010","tin":"07654321","#,
    r#""currency":"AMD","lang":"hy","payment_days":10}}"#
);

/// Иностранный клиент, у которого заполнено только название.
const BARE: &str = concat!(
    r#"{"meta":{"id":"5b0e8f9a-3c2d-4e1f-8a7b-9c6d5e4f3a2b","modified":1760000000000,"deleted":null},"#,
    r#""data":{"name":"Acme Inc.","country":null,"address":"","tin":null,"#,
    r#""currency":null,"lang":null,"payment_days":null}}"#
);

/// Образцы читаются в ожидаемые значения, а их запись — слово в слово
/// образцы. Поменялась запись — это новая версия формата (Правило 5),
/// а не правка образца.
#[test]
fn data_shape_is_pinned() {
    let client: Client = serde_json::from_str(SAMPLE).expect("образец читается");
    let data = client.data();
    assert_eq!(data.name.as_str(), "«Արարատ» ՍՊԸ");
    assert_eq!(data.country, Some(Country::ARMENIA));
    assert_eq!(data.address.as_str(), "ք. Երևան, Տիգրան Մեծի 10\n0010");
    assert_eq!(data.tin.as_ref().map(Tin::as_str), Some("07654321"));
    assert_eq!(data.currency, Some(Currency::Amd));
    assert_eq!(data.lang, Some(Lang::Am));
    assert_eq!(data.payment_days, Some(10));
    assert!(!client.is_deleted());
    assert_eq!(serde_json::to_string(&client).ok().as_deref(), Some(SAMPLE));

    let bare: Client = serde_json::from_str(BARE).expect("голый читается");
    assert_eq!(bare.data(), &named("Acme Inc."));
    assert_eq!(serde_json::to_string(&bare).ok().as_deref(), Some(BARE));
}

/// Испорченное не читается: незнакомое поле, запретное значение в любом
/// поле, пустое название, срок оплаты больше года.
#[test]
fn data_rejects_what_the_types_forbid() {
    let cases = [
        SAMPLE.replace(r#""payment_days":10"#, r#""payment_days":10,"email":"a@b.am""#),
        SAMPLE.replace(r#""name":"«Արարատ» ՍՊԸ""#, r#""name":"""#),
        SAMPLE.replace(r#""name":"«Արարատ» ՍՊԸ""#, r#""name":" Արարատ""#),
        SAMPLE.replace(r#""country":"AM""#, r#""country":"am""#),
        SAMPLE.replace(r#""country":"AM""#, r#""country":"ARM""#),
        SAMPLE.replace(r#""country":"AM""#, r#""country":"XK""#),
        SAMPLE.replace(r#""tin":"07654321""#, r#""tin":"7654321""#),
        SAMPLE.replace(r#""currency":"AMD""#, r#""currency":"RUB""#),
        SAMPLE.replace(r#""lang":"hy""#, r#""lang":"am""#),
        SAMPLE.replace(r#""lang":"hy""#, r#""lang":"Am""#),
        SAMPLE.replace(r#""payment_days":10"#, r#""payment_days":366"#),
        SAMPLE.replace(r#""payment_days":10"#, r#""payment_days":-1"#),
        SAMPLE.replace(r#""payment_days":10"#, r#""payment_days":"10""#),
        SAMPLE.replace(r#""deleted":null},"data""#, r#""deleted":null},"extra":1,"data""#),
        SAMPLE.replace(r#""name":"«Արարատ» ՍՊԸ","#, ""),
    ];
    let mut wrong = Vec::new();
    for (index, text) in cases.iter().enumerate() {
        if text == SAMPLE {
            wrong.push(format!("случай {index} не изменил образец"));
        } else if serde_json::from_str::<Client>(text).is_ok() {
            wrong.push(format!("случай {index} прочитан"));
        }
    }
    assert!(wrong.is_empty(), "прочитано испорченное: {wrong:#?}");
}

/// Команды: новый клиент — новая запись; правка — тот же идентификатор
/// и новый момент; без изменений — записывать нечего; пустое название
/// и срок больше года не принимаются.
#[test]
fn create_and_edit() {
    let created = Client::create(named("Acme Inc."), at(1_000)).expect("годно");
    assert_eq!(created.meta().modified(), at(1_000));
    assert!(!created.is_deleted());

    assert_eq!(created.edit(named("Acme Inc."), at(2_000)), Ok(None));

    let mut data = named("Acme Inc.");
    data.payment_days = Some(15);
    let edited = created.edit(data.clone(), at(3_000)).expect("годно").expect("правка");
    assert_eq!(edited.id(), created.id());
    assert_eq!(edited.meta().modified(), at(3_000));
    assert_eq!(edited.data(), &data);

    assert_eq!(Client::create(named(""), at(1_000)), Err(ClientError::NoName));
    data.payment_days = Some(ClientData::MAX_PAYMENT_DAYS + 1);
    assert_eq!(edited.edit(data, at(4_000)), Err(ClientError::PaymentDays));
}

/// Удаление — метка, восстановление её снимает; повторять нечего.
#[test]
fn delete_and_restore() {
    let client = Client::create(named("Acme Inc."), at(1_000)).expect("годно");
    assert_eq!(client.restore(at(1_500)), None, "живого не восстанавливают");
    let deleted = client.delete(at(2_000)).expect("удаляется");
    assert!(deleted.is_deleted());
    assert_eq!(deleted.meta().deleted(), Some(at(2_000)));
    assert_eq!(deleted.id(), client.id());
    assert_eq!(deleted.data(), client.data());
    assert_eq!(deleted.delete(at(3_000)), None, "удалённого не удаляют второй раз");
    let restored = deleted.restore(at(4_000)).expect("восстанавливается");
    assert!(!restored.is_deleted());
    assert_eq!(restored.meta().modified(), at(4_000));
}

/// Срок оплаты из поля: целые дни цифрами от 0 до 365, пусто — не указан.
#[test]
fn payment_days_are_parsed() {
    let read = [
        ("", None),
        ("  ", None),
        ("0", Some(0)),
        (" 15 ", Some(15)),
        ("365", Some(365)),
        ("030", Some(30)),
    ];
    let mut wrong = Vec::new();
    for (text, expected) in read {
        if ClientData::parse_payment_days(text) != Ok(expected) {
            wrong.push(format!("«{text}»: {:?}", ClientData::parse_payment_days(text)));
        }
    }
    for text in ["366", "-1", "1.5", "15 дней", "١٥", "0365", "1e2", "+5"] {
        if ClientData::parse_payment_days(text) != Err(ClientError::PaymentDays) {
            wrong.push(format!("«{text}» принят"));
        }
    }
    assert!(wrong.is_empty(), "срок оплаты: {wrong:#?}");
}

/// Поиск: только живые, по части названия без учёта регистра, «ё»
/// и армянской «և», — по алфавиту; пустой запрос — все живые.
#[test]
fn find_matches_names_alphabetically() {
    let make = |name: &str| Client::create(named(name), at(1_000)).expect("годно");
    let gone = make("Альфа удалённая").delete(at(2_000)).expect("удаляется");
    let clients = vec![
        make("ООО Семён и партнёры"),
        make("Acme Inc."),
        make("Երևանի համակարգչային կենտրոն"),
        gone,
        make("Альфа"),
    ];
    let names = |query: &str| -> Vec<&str> {
        Client::find(&clients, query)
            .iter()
            .map(|client| client.data().name.as_str())
            .collect()
    };
    assert_eq!(
        names(""),
        ["Acme Inc.", "Альфа", "ООО Семён и партнёры", "Երևանի համակարգչային կենտրոն"]
    );
    assert_eq!(names("семен"), ["ООО Семён и партнёры"]);
    assert_eq!(names("  ACME "), ["Acme Inc."]);
    assert_eq!(names("երեւան"), ["Երևանի համակարգչային կենտրոն"]);
    assert_eq!(names("альфа"), ["Альфа"], "удалённый не находится");
    assert!(names("Ararat").is_empty());

    let deleted: Vec<&str> =
        Client::deleted(&clients).iter().map(|c| c.data().name.as_str()).collect();
    assert_eq!(deleted, ["Альфа удалённая"]);
}

/// Удалённые — последний удалённый первым.
#[test]
fn deleted_latest_first() {
    let make = |name: &str, when: i64| {
        Client::create(named(name), at(1_000))
            .expect("годно")
            .delete(at(when))
            .expect("удаляется")
    };
    let clients = vec![make("Первый", 2_000), make("Третий", 4_000), make("Второй", 3_000)];
    let order: Vec<&str> =
        Client::deleted(&clients).iter().map(|c| c.data().name.as_str()).collect();
    assert_eq!(order, ["Третий", "Второй", "Первый"]);
}
