//! Тесты слияния по полям (задача 116): правки двух устройств — настоящими
//! командами документа, моменты — тем путём, что и у хранилища ([`stamp`]),
//! слияние — тем, что у копии и кода передачи (`backup::merge`).

use serde::{Deserialize, Serialize};

use super::*;
use crate::format::backup::{self, Entry, KINDS, Kind, Raw};
use crate::format::{Stored, decode, encode};
use crate::i18n::Lang;
use crate::model::client::{Client, ClientData};
use crate::model::country::Country;
use crate::model::date::Date;
use crate::model::document::{DocKind, Document, DocumentData, Item, Payment, Settlement, SettlementKind};
use crate::model::money::Currency;
use crate::model::profile::ProfileData;
use crate::model::record::RecordMeta;
use crate::model::text::{Line, Lines};

fn at(millis: i64) -> Timestamp {
    Timestamp::from_unix_millis(millis)
}

fn line(text: &str) -> Line {
    Line::new(text).expect("годный текст")
}

fn day() -> Date {
    Date::new(2026, 11, 2).expect("дата")
}

fn client() -> Client {
    Client::create(
        ClientData {
            name: line("Acme GmbH"),
            country: Some(Country::ARMENIA),
            address: Lines::default(),
            tin: None,
            tax_id: Line::default(),
            currency: Some(Currency::Usd),
            lang: Some(Lang::En),
            payment_days: None,
        },
        at(500),
    )
    .expect("годный клиент")
}

fn data(client: &Client, text: &str) -> DocumentData {
    DocumentData {
        kind: DocKind::Invoice,
        client: client.id(),
        lang: Lang::En,
        currency: Currency::Usd,
        date: day(),
        service_date: day(),
        due: None,
        items: vec![Item {
            text: line(text),
            quantity_e3: 1_000,
            price: 100_000,
        }],
        basis: None,
        recurring: None,
        rate: None,
        activity: None,
        income_kind: None,
    }
}

/// Счёт в долларах, выпущенный в 2 000 — с него начинают оба устройства.
fn issued() -> Document {
    let client = client();
    let mut profile = ProfileData::default();
    profile.name.hy = line("ԱՁ Արամ Պետրոսյան");
    profile.name.en = line("IE Aram Petrosyan");
    Document::create(data(&client, "Consulting"), at(1_000))
        .expect("черновик")
        .issue(&[], 0, Some(&profile), Some(None), &client, at(2_000))
        .expect("выпущен")
}

/// Правка `after` поверх `before`, записанная тем путём, что у хранилища:
/// с моментами полей.
fn stamped<T: Stored>(before: &T, after: &T) -> T {
    let text = stamp(T::KIND, &encode(before), &encode(after)).expect("моменты поставлены");
    decode(&text).expect("запись с моментами читается")
}

/// Моменты правки полей записи — проверенные.
fn moments<T: Stored>(record: &T) -> Moments {
    Moments::of(record.meta()).expect("моменты годятся")
}

/// Слить запись `incoming` с записью базы `stored` (тем же, что у копии):
/// что легло бы в базу — и сколько добавлено, заменено, оставлено.
fn merged<T: Stored>(kinds: &[Kind], stored: &T, incoming: &T) -> (Option<T>, (usize, usize, usize)) {
    let id = stored.meta().id().to_string();
    let incoming_text = encode(incoming);
    let entries: Vec<Entry> = backup::collect(
        kinds,
        [Raw {
            kind: T::KIND,
            id: &id,
            text: Some(&incoming_text),
        }],
    )
    .expect("собрано")
    .entries()
    .to_vec();
    let stored_text = encode(stored);
    let merge = backup::merge(
        kinds,
        &entries,
        [Raw {
            kind: T::KIND,
            id: &id,
            text: Some(&stored_text),
        }],
    )
    .expect("слито");
    let written = merge.writes().next().map(|entry| decode(&entry.stored()).expect("слитое читается"));
    (written, (merge.added(), merge.updated(), merge.unchanged()))
}

fn paid(document: &Document, now: i64) -> Document {
    document
        .add_payment(
            Payment {
                date: day(),
                amount: 40_000,
            },
            at(now),
        )
        .expect("оплата")
}

fn settled(document: &Document, now: i64) -> Document {
    document
        .set_settlement(
            Settlement {
                day: day(),
                number: line("0012345"),
                kind: SettlementKind::System,
            },
            at(now),
        )
        .expect("отметка")
        .expect("изменён")
}

fn rated(document: &Document, per_unit_e4: i64, now: i64) -> Document {
    document
        .set_rate(Some(per_unit_e4), at(now))
        .expect("курс")
        .expect("изменён")
}

/// Хранилище ставит момент правки тем полям, что стали другими, — и только
/// им; прежние моменты остаются, а остальные поля правлены до прежней правки
/// записи. Сама запись при этом та же, что дала команда.
#[test]
fn a_change_marks_only_its_fields() {
    let start = issued();
    let paid = stamped(&start, &paid(&start, 3_000));
    assert_eq!(moments(&paid).since(), at(2_000));
    assert_eq!(moments(&paid).changed("payments"), at(3_000));
    assert_eq!(moments(&paid).changed("data"), at(2_000));
    assert_eq!(moments(&paid).changed("issued"), at(2_000));
    assert_eq!(paid.payments().len(), 1, "оплата на месте");

    let rated = stamped(&paid, &rated(&paid, 3_850_000, 4_000));
    let names: Vec<String> = moments(&rated).fields().keys().cloned().collect();
    assert_eq!(names, ["data", "payments"]);
    assert_eq!(moments(&rated).changed("payments"), at(3_000), "прежний момент остался");
    assert_eq!(moments(&rated).changed("data"), at(4_000), "курс — в поле `data` с валютой и строками");

    let draft = Document::create(data(&client(), "Draft"), at(1_000)).expect("черновик");
    let deleted = stamped(&draft, &draft.delete(at(5_000)).expect("удаляется").expect("удалён"));
    let names: Vec<String> = moments(&deleted).fields().keys().cloned().collect();
    assert_eq!(names, [DELETED]);

    // Правка не новее записи базы, другая запись, запись не текущей версии —
    // моментов нет, запись пишется как есть.
    assert_eq!(stamp(Document::KIND, &encode(&paid), &encode(&start)), None);
    assert_eq!(stamp(Document::KIND, &encode(&draft), &encode(&paid)), None);
    assert_eq!(stamp(Document::KIND, &encode(&start), "не JSON"), None);
}

/// Оплата на телефоне, отметка ՊԵԿ на компьютере (задача 116): после
/// слияния в любую сторону у обоих — и то, и другое, одной и той же записью.
#[test]
fn two_devices_keep_both_changes() {
    let start = issued();
    let phone = stamped(&start, &paid(&start, 3_000));
    let computer = stamped(&start, &settled(&start, 4_000));

    let (on_computer, counts) = merged(KINDS, &computer, &phone);
    let on_computer = on_computer.expect("слитое легло");
    assert_eq!(counts, (0, 1, 0));
    assert_eq!(on_computer.payments().len(), 1, "оплата телефона пропала");
    assert!(on_computer.settlement().is_some(), "отметка компьютера пропала");

    let (on_phone, _) = merged(KINDS, &phone, &computer);
    assert_eq!(on_phone.as_ref(), Some(&on_computer), "устройства разошлись");
    assert_eq!(on_computer.meta().modified(), at(4_000));
    assert_eq!(moments(&on_computer).changed("payments"), at(3_000));
    assert_eq!(moments(&on_computer).changed("settlement"), at(4_000));

    // Слитое с любой стороны — уже то же: второй раз писать нечего.
    let (again, counts) = merged(KINDS, &on_computer, &phone);
    assert_eq!((again, counts), (None, (0, 0, 1)));
    let (again, counts) = merged(KINDS, &on_computer, &on_phone.expect("слито"));
    assert_eq!((again, counts), (None, (0, 0, 1)));
}

/// Одно поле правили на обоих — остаётся поздняя правка, в какую бы сторону
/// ни шло слияние.
#[test]
fn the_later_change_of_one_field_wins() {
    let start = issued();
    let phone = stamped(&start, &rated(&start, 3_850_000, 3_000));
    let computer = stamped(&start, &rated(&start, 3_870_000, 4_000));
    let mut wrong = Vec::new();
    for (name, stored, incoming) in [("на компьютер", &computer, &phone), ("на телефон", &phone, &computer)] {
        let (written, _) = merged(KINDS, stored, incoming);
        let result = written.unwrap_or_else(|| stored.clone());
        if result.data().rate.map(|rate| rate.per_unit_e4()) != Some(3_870_000) {
            wrong.push(name);
        }
    }
    assert!(wrong.is_empty(), "поздний курс не остался: {wrong:?}");
}

/// Удалённое на одном и правленное позже на другом — удалено, с правкой:
/// восстановить его можно, и правка не пропала.
#[test]
fn deleted_on_one_and_changed_on_the_other_stays_deleted_with_the_change() {
    let client = client();
    let draft = Document::create(data(&client, "Draft"), at(1_000)).expect("черновик");
    let phone = stamped(&draft, &draft.delete(at(3_000)).expect("удаляется").expect("удалён"));
    let edited = draft.edit(data(&client, "Edited"), at(4_000)).expect("правится").expect("изменён");
    let computer = stamped(&draft, &edited);
    for (stored, incoming) in [(&computer, &phone), (&phone, &computer)] {
        let (written, _) = merged(KINDS, stored, incoming);
        let result = written.unwrap_or_else(|| stored.clone());
        assert_eq!(result.meta().deleted(), Some(at(3_000)), "удаление пропало");
        assert_eq!(result.data().items[0].text, line("Edited"), "правка пропала");
    }
}

/// Записи без моментов полей — прежних версий — сливаются, как прежде:
/// целиком поздняя.
#[test]
fn records_without_moments_merge_whole() {
    let start = issued();
    let phone = paid(&start, 3_000);
    let computer = settled(&start, 4_000);
    assert!(phone.meta().edits().is_none() && computer.meta().edits().is_none());

    let (written, counts) = merged(KINDS, &phone, &computer);
    assert_eq!(counts, (0, 1, 0));
    assert_eq!(written.as_ref(), Some(&computer), "легла не поздняя целиком");
    let (written, counts) = merged(KINDS, &computer, &phone);
    assert_eq!((written, counts), (None, (0, 0, 1)));
}

/// Проба: запись, которую её вид не читает, если оба поля `data` — единицы.
#[derive(Serialize, Clone, PartialEq, Debug)]
struct Pair {
    meta: RecordMeta,
    data: PairData,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
#[serde(deny_unknown_fields)]
struct PairData {
    a: i32,
    b: i32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PairFields {
    meta: RecordMeta,
    data: PairData,
}

impl<'de> Deserialize<'de> for Pair {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let PairFields { meta, data } = PairFields::deserialize(deserializer)?;
        if data.a == 1 && data.b == 1 {
            return Err(serde::de::Error::custom("оба поля сразу не бывают"));
        }
        Ok(Self { meta, data })
    }
}

impl Stored for Pair {
    const KIND: &'static str = "pair";

    fn meta(&self) -> &RecordMeta {
        &self.meta
    }
}

/// Слитая по полям запись, которую её вид не читает, не пишется: решают
/// моменты записей целиком, как до слияния по полям.
#[test]
fn a_merge_the_kind_refuses_falls_back_to_the_whole_record() {
    const PAIRS: &[Kind] = &[Kind::of::<Pair>()];
    let start = Pair {
        meta: RecordMeta::new(at(1_000)),
        data: PairData { a: 0, b: 0 },
    };
    let change = |data: PairData, now: i64| {
        let mut after = Pair {
            data,
            ..start.clone()
        };
        after.meta.touch(at(now));
        let text = stamp(Pair::KIND, &encode(&start), &encode(&after)).expect("моменты поставлены");
        decode::<Pair>(&text).expect("читается")
    };
    let phone = change(PairData { a: 1, b: 0 }, 3_000);
    let computer = change(PairData { a: 0, b: 1 }, 4_000);
    let (written, _) = merged(PAIRS, &phone, &computer);
    assert_eq!(written.map(|pair| pair.data), Some(PairData { a: 0, b: 1 }), "легла не поздняя целиком");
    let (written, counts) = merged(PAIRS, &computer, &phone);
    assert_eq!((written, counts), (None, (0, 0, 1)));
}

/// Полей с моментами у самых больших записей — профиля и документа — меньше
/// предела `MAX_FIELDS`: иначе моменты правки не встали бы, и правка
/// терялась бы при слиянии.
#[test]
fn every_field_fits_under_the_limit() {
    let mut wrong = Vec::new();
    let profile = serde_json::to_value(ProfileData::default()).expect("профиль в JSON");
    let profile = profile.as_object().map_or(0, serde_json::Map::len) + 2;
    let document = encode(&paid(&issued(), 3_000));
    let (_, record) = split_raw(&document).expect("конверт");
    let units = Record::parse(record.get()).expect("поля").units().len() + 1;
    for (name, count) in [("профиль", profile), ("документ", units)] {
        if count > MAX_FIELDS {
            wrong.push(format!("{name}: {count}"));
        }
    }
    assert!(wrong.is_empty(), "полей больше предела {MAX_FIELDS}: {wrong:?}");
}

/// Поля слияния — поля верхнего уровня, `data` одним полем и отдельно — поля
/// `data` из таблицы `APART`; глубже слияние не смотрит: сумма с валютой, курс
/// с днём, валюта с ценами строк — одно поле. У вида без строки в таблице
/// `data` — одно поле целиком.
#[test]
fn units_are_top_fields_data_and_its_fields_apart() {
    let document = encode(&rated(&issued(), 3_850_000, 3_000));
    let (_, record) = split_raw(&document).expect("конверт");
    let units = grouped(Document::KIND, &Record::parse(record.get()).expect("поля").units());
    let names: Vec<&str> = units.keys().map(String::as_str).collect();
    assert_eq!(
        names,
        ["annulled", "data", "data.activity", "data.income_kind", "issued", "payments", "settlement", "signed"]
    );
    let other = grouped("closing", &Record::parse(record.get()).expect("поля").units());
    assert!(other.contains_key("data") && !other.keys().any(|name| name.starts_with("data.")), "вид без строки в таблице");
    let mut unknown = Vec::new();
    for (kind, fields) in APART {
        if !KINDS.iter().any(|known| known.name() == *kind) {
            unknown.push(format!("вид {kind}"));
        }
        for field in *fields {
            if !is_field_name(&format!("data.{field}")) {
                unknown.push(format!("{kind}: {field}"));
            }
        }
    }
    assert!(unknown.is_empty(), "таблица полей: {unknown:?}");
}

/// Моменты полей проверяет тот, кто их разбирает: общие поля записи берут
/// их нетронутыми, а запись копии и кода с негодными моментами — порча
/// (Правило 13): позже правки, раньше `since`, без `since`, `since` позже
/// правки, не того имени, больше предела, лишнее поле.
#[test]
fn broken_moments_are_refused_by_the_backup() {
    let id = "0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a";
    let pair = |edits: &str| {
        format!(
            r#"{{"format_version":{FORMAT_VERSION},"record":{{"meta":{{"id":"{id}","modified":1000,"deleted":null,"edits":{edits}}},"data":{{"a":0,"b":0}}}}}}"#
        )
    };
    let many = (0..=MAX_FIELDS).map(|i| format!(r#""f{i}":900"#)).collect::<Vec<_>>().join(",");
    let rejected = [
        r#"{"since":500,"fields":{"payments":1001}}"#.to_owned(),
        r#"{"since":500,"fields":{"payments":499}}"#.to_owned(),
        r#"{"fields":{"payments":900}}"#.to_owned(),
        r#"{"since":1001}"#.to_owned(),
        r#"{"since":500,"fields":{"Payments":900}}"#.to_owned(),
        r#"{"since":500,"fields":{"":900}}"#.to_owned(),
        r#"{"since":500,"fields":{"data rate":900}}"#.to_owned(),
        format!(r#"{{"since":500,"fields":{{"{}":900}}}}"#, "a".repeat(MAX_FIELD_NAME + 1)),
        format!(r#"{{"since":500,"fields":{{{many}}}}}"#),
        r#"{"since":500,"extra":1}"#.to_owned(),
        r#"[500]"#.to_owned(),
    ];
    const PAIRS: &[Kind] = &[Kind::of::<Pair>()];
    let mut wrong = Vec::new();
    for edits in &rejected {
        let text = pair(edits);
        if decode::<Pair>(&text).is_err() {
            wrong.push(format!("общие поля не прочли {edits}"));
        }
        let collected = backup::collect(PAIRS, [Raw { kind: Pair::KIND, id, text: Some(&text) }]).expect("собрано");
        if collected.unreadable().is_empty() {
            wrong.push(format!("копия взяла {edits}"));
        }
    }
    let good = pair(r#"{"since":500,"fields":{"data.a":900}}"#);
    let collected = backup::collect(PAIRS, [Raw { kind: Pair::KIND, id, text: Some(&good) }]).expect("собрано");
    if collected.entries().len() != 1 {
        wrong.push("годные моменты не взяты".to_owned());
    }
    assert!(wrong.is_empty(), "моменты: {wrong:#?}");
}

/// Валюта и цены черновика правятся вместе (ревью 0.0.171): на компьютере
/// черновик перевели в драмы с новой ценой, а на телефоне позже поправили
/// текст строки — слитый черновик не берёт валюту одного устройства и цены
/// другого: сумма, которой не вводил никто. Поля `data`, которые правятся
/// вместе, — одно поле.
#[test]
fn currency_and_prices_of_a_draft_merge_together() {
    let client = client();
    let draft = Document::create(data(&client, "Consulting"), at(1_000)).expect("черновик");
    let mut drams = data(&client, "Consulting");
    drams.currency = Currency::Amd;
    drams.items[0].price = 39_000_000;
    let computer = stamped(&draft, &draft.edit(drams, at(2_000)).expect("правится").expect("изменён"));
    let retext = data(&client, "Consulting, November");
    let phone = stamped(&draft, &draft.edit(retext, at(2_500)).expect("правится").expect("изменён"));
    let mut wrong = Vec::new();
    for (name, stored, incoming) in [("на компьютер", &computer, &phone), ("на телефон", &phone, &computer)] {
        let (written, _) = merged(KINDS, stored, incoming);
        let result = written.unwrap_or_else(|| stored.clone());
        let pair = (result.data().currency, result.data().items[0].price);
        if pair != (Currency::Usd, 100_000) && pair != (Currency::Amd, 39_000_000) {
            wrong.push(format!("{name}: {pair:?}"));
        }
    }
    assert!(wrong.is_empty(), "валюта одного устройства с ценой другого: {wrong:?}");
}

/// Два выпуска одного черновика на двух устройствах (ревью 0.0.171) — целиком
/// поздний: реквизиты и номер одного выпуска со строками другого — выпущенный
/// документ, которого никто не выпускал.
#[test]
fn two_issues_of_one_draft_are_not_mixed() {
    let client = client();
    let mut profile = ProfileData::default();
    profile.name.en = line("IE Aram Petrosyan");
    let draft = Document::create(data(&client, "Consulting"), at(1_000)).expect("черновик");
    let edited = stamped(&draft, &draft.edit(data(&client, "Edited"), at(1_500)).expect("правится").expect("изменён"));
    let phone = stamped(
        &edited,
        &edited.issue(&[], 0, Some(&profile), Some(None), &client, at(1_800)).expect("выпущен"),
    );
    profile.name.en = line("IE Aram Petrosyan, Yerevan");
    let computer = stamped(
        &draft,
        &draft.issue(&[], 0, Some(&profile), Some(None), &client, at(2_000)).expect("выпущен"),
    );
    for (stored, incoming) in [(&computer, &phone), (&phone, &computer)] {
        let (written, _) = merged(KINDS, stored, incoming);
        let result = written.unwrap_or_else(|| stored.clone());
        assert_eq!(result, computer, "выпуски смешались");
    }
}

/// Повтор, сдвинутый при открытии программы, пишется без моментов полей
/// (`Writes::put` в главном wasm), — и сдвиг снимает прежние моменты (ревью
/// 0.0.171): иначе счётчик с прежним моментом проиграл бы ничью, и телефон
/// навсегда держал бы прежний.
#[test]
fn a_moved_repeat_merges_whole() {
    use crate::model::recurring::Recurring;
    let client = client();
    let mut profile = ProfileData::default();
    profile.name.en = line("IE Aram Petrosyan");
    let day = |month: u8, d: u8| Date::new(2026, month, d).expect("дата");
    let mut source_data = data(&client, "Subscription");
    source_data.date = day(8, 31);
    source_data.service_date = day(8, 31);
    let source = Document::create(source_data, at(1_000))
        .expect("черновик")
        .issue(&[], 0, Some(&profile), Some(None), &client, at(1_100))
        .expect("выпущен");
    let started = Recurring::start(None, &source, day(9, 1), at(1_200)).expect("годится").expect("новый");
    let stopped = stamped(&started, &started.stop(at(1_300)).expect("остановлен"));
    let restarted = Recurring::start(Some(&stopped), &source, day(9, 1), at(1_400)).expect("годится").expect("снова");
    let restarted = stamped(&stopped, &restarted);
    assert!(restarted.meta().edits().is_some(), "у повтора есть моменты");
    let (moved, _) = restarted.generate(&source, true, day(10, 13), at(2_000)).expect("пора");
    assert!(moved.meta().edits().is_none(), "сдвиг оставил прежние моменты");
    let (on_phone, _) = merged(KINDS, &restarted, &moved);
    assert_eq!(on_phone.map(|recurring| recurring.data().next), Some(moved.data().next), "телефон не взял сдвиг");
}
