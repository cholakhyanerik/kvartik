//! Тесты файла копии. Образцы выписаны человеком, а не записаны кодом:
//! код, записавший себе эталон, проверял бы сам себя (Правило 2). Шифра
//! здесь нет — его делает WebCrypto окна, и проверяют его браузерные тесты
//! (`crates/app/src/store/tests.rs`); вместо шифротекста — любые байты.

use serde::{Deserialize, Serialize};

use super::*;
use crate::format::encode;

/// Запись для проб.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(deny_unknown_fields)]
struct Note {
    meta: RecordMeta,
    text: String,
}

impl Stored for Note {
    const KIND: &'static str = "note";

    fn meta(&self) -> &RecordMeta {
        &self.meta
    }
}

/// Запись другого вида.
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

const KINDS_UNDER_TEST: &[Kind] = &[Kind::of::<Note>(), Kind::of::<Tag>()];

fn at(millis: i64) -> Timestamp {
    Timestamp::from_unix_millis(millis)
}

fn day(year: u16, month: u8, day: u8) -> Date {
    Date::new(year, month, day).expect("в тесте — существующая дата")
}

const NOTE_ID: &str = "0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a";
const TAG_ID: &str = "92d87e64-1ccb-46d0-93d7-917b1e773af5";

/// Запись `note` в базе — в конверте текущей версии 3, как её кладёт
/// хранилище.
const STORED_NOTE: &str = concat!(
    r#"{"format_version":3,"record":{"meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","#,
    r#""modified":1760000000000,"deleted":null},"text":"Ереван ֏"}}"#
);

/// Запись `tag` в базе: удалена.
const STORED_TAG: &str = concat!(
    r#"{"format_version":3,"record":{"meta":{"id":"92d87e64-1ccb-46d0-93d7-917b1e773af5","#,
    r#""modified":1760000000500,"deleted":1760000000500}}}"#
);

/// Те же записи, какими их клала программа версии формата 1.
const STORED_NOTE_V1: &str = concat!(
    r#"{"format_version":1,"record":{"meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","#,
    r#""modified":1760000000000,"deleted":null},"text":"Ереван ֏"}}"#
);

const STORED_TAG_V1: &str = concat!(
    r#"{"format_version":1,"record":{"meta":{"id":"92d87e64-1ccb-46d0-93d7-917b1e773af5","#,
    r#""modified":1760000000500,"deleted":1760000000500}}}"#
);

/// Открытый текст копии версии 1 с этими двумя записями — история:
/// так копию писала 0.0.8, и такой файл обязан читаться (Правило 2).
const SAMPLE_PLAIN_V1: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":1,"app_version":"0.0.8","created":1760270400000,"#,
    r#""records":["#,
    r#"{"kind":"note","record":{"meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","#,
    r#""modified":1760000000000,"deleted":null},"text":"Ереван ֏"}},"#,
    r#"{"kind":"tag","record":{"meta":{"id":"92d87e64-1ccb-46d0-93d7-917b1e773af5","#,
    r#""modified":1760000000500,"deleted":1760000000500}}}"#,
    r#"]}"#
);

/// Файл копии версии 1. Соль — байты 0–15, вектор — 100–111, «шифротекст» —
/// 200–219; base64 посчитан не этим кодом, а модулем `base64` Python.
const SAMPLE_SEALED_V1: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":1,"app_version":"0.0.8","created":1760270400000,"#,
    r#""kdf":{"name":"PBKDF2-SHA256","iterations":600000,"salt":"AAECAwQFBgcICQoLDA0ODw=="},"#,
    r#""cipher":{"name":"AES-256-GCM","iv":"ZGVmZ2hpamtsbW5v"},"#,
    r#""data":"yMnKy8zNzs/Q0dLT1NXW19jZ2ts="}"#
);

/// Открытый текст копии версии 2 — история: так копию писала 0.0.12.
/// Выписан человеком: от версии 1 отличается номером версии и версией
/// программы в шапке.
const SAMPLE_PLAIN_V2: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":2,"app_version":"0.0.12","created":1760270400000,"#,
    r#""records":["#,
    r#"{"kind":"note","record":{"meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","#,
    r#""modified":1760000000000,"deleted":null},"text":"Ереван ֏"}},"#,
    r#"{"kind":"tag","record":{"meta":{"id":"92d87e64-1ccb-46d0-93d7-917b1e773af5","#,
    r#""modified":1760000000500,"deleted":1760000000500}}}"#,
    r#"]}"#
);

/// Файл копии версии 2 — с теми же солью, вектором и «шифротекстом», что
/// у образца версии 1.
const SAMPLE_SEALED_V2: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":2,"app_version":"0.0.12","created":1760270400000,"#,
    r#""kdf":{"name":"PBKDF2-SHA256","iterations":600000,"salt":"AAECAwQFBgcICQoLDA0ODw=="},"#,
    r#""cipher":{"name":"AES-256-GCM","iv":"ZGVmZ2hpamtsbW5v"},"#,
    r#""data":"yMnKy8zNzs/Q0dLT1NXW19jZ2ts="}"#
);

/// Открытый текст копии текущей версии 3 — так её пишет эта программа.
/// Выписан человеком: от версии 2 отличается номером версии и версией
/// программы в шапке.
const SAMPLE_PLAIN_V3: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":3,"app_version":"0.0.14","created":1760270400000,"#,
    r#""records":["#,
    r#"{"kind":"note","record":{"meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","#,
    r#""modified":1760000000000,"deleted":null},"text":"Ереван ֏"}},"#,
    r#"{"kind":"tag","record":{"meta":{"id":"92d87e64-1ccb-46d0-93d7-917b1e773af5","#,
    r#""modified":1760000000500,"deleted":1760000000500}}}"#,
    r#"]}"#
);

/// Файл копии версии 3 — с теми же солью, вектором и «шифротекстом», что
/// у образцов прошлых версий.
const SAMPLE_SEALED_V3: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":3,"app_version":"0.0.14","created":1760270400000,"#,
    r#""kdf":{"name":"PBKDF2-SHA256","iterations":600000,"salt":"AAECAwQFBgcICQoLDA0ODw=="},"#,
    r#""cipher":{"name":"AES-256-GCM","iv":"ZGVmZ2hpamtsbW5v"},"#,
    r#""data":"yMnKy8zNzs/Q0dLT1NXW19jZ2ts="}"#
);

/// Шапка копии, которую пишет эта программа.
fn sample_header() -> Header {
    Header::new("0.0.14", at(1_760_270_400_000))
}

/// Шапка образца версии 2: так её писала 0.0.12.
fn header_v2() -> Header {
    Header {
        format_version: 2,
        app_version: "0.0.12".to_owned(),
        created: at(1_760_270_400_000),
    }
}

/// Шапка образца версии 1: так её писала 0.0.8.
fn header_v1() -> Header {
    Header {
        format_version: 1,
        app_version: "0.0.8".to_owned(),
        created: at(1_760_270_400_000),
    }
}

fn sample_lock() -> Lock {
    Lock::new(600_000, (0..16).collect(), (100..112).collect()).expect("замок образца годится")
}

fn sealed_sample() -> Sealed {
    open(SAMPLE_SEALED_V3).expect("образец файла копии открывается")
}

fn sealed_sample_v1() -> Sealed {
    open(SAMPLE_SEALED_V1).expect("образец файла копии версии 1 открывается")
}

fn sealed_sample_v2() -> Sealed {
    open(SAMPLE_SEALED_V2).expect("образец файла копии версии 2 открывается")
}

fn raw<'a>(kind: &'a str, id: &'a str, text: &'a str) -> Raw<'a> {
    Raw {
        kind,
        id,
        text: Some(text),
    }
}

/// Записи базы из двух образцов.
fn stored_pair() -> [Raw<'static>; 2] {
    [
        raw("note", NOTE_ID, STORED_NOTE),
        raw("tag", TAG_ID, STORED_TAG),
    ]
}

/// Образец файла текущей версии читается в ожидаемые значения, а запись тех
/// же значений — слово в слово образец. Поменялась запись — это новая
/// версия формата (Правило 5), а не правка образца.
#[test]
fn sealed_sample_v3_is_pinned() {
    let sealed = sealed_sample();
    assert_eq!(sealed.header(), &sample_header());
    assert_eq!(sealed.header().format_version(), 3);
    assert_eq!(sealed.header().app_version(), "0.0.14");
    assert_eq!(sealed.header().created(), at(1_760_270_400_000));
    assert_eq!(sealed.lock(), &sample_lock());
    assert_eq!(sealed.ciphertext(), (200..220).collect::<Vec<u8>>());
    assert_eq!(
        seal(
            &sample_header(),
            &sample_lock(),
            &(200..220).collect::<Vec<u8>>()
        ),
        SAMPLE_SEALED_V3
    );
}

/// Файлы версий 1 и 2 открываются этой программой с той же шапкой
/// и замком.
#[test]
fn older_sealed_samples_still_open() {
    for (sealed, header) in [(sealed_sample_v1(), header_v1()), (sealed_sample_v2(), header_v2())] {
        assert_eq!(sealed.header(), &header);
        assert_eq!(sealed.lock(), &sample_lock());
        assert_eq!(sealed.ciphertext(), (200..220).collect::<Vec<u8>>());
    }
}

/// Открытый текст текущей версии читается в записи с ожидаемыми полями,
/// а запись тех же записей — слово в слово образец.
#[test]
fn plain_sample_v3_is_pinned() {
    let entries = read(
        KINDS_UNDER_TEST,
        &sealed_sample(),
        SAMPLE_PLAIN_V3.as_bytes(),
    )
    .expect("образец читается");
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].kind(), "note");
    assert_eq!(entries[0].meta().id().to_string(), NOTE_ID);
    assert_eq!(entries[0].meta().modified(), at(1_760_000_000_000));
    assert_eq!(entries[0].meta().deleted(), None);
    assert_eq!(entries[0].stored(), STORED_NOTE);
    assert_eq!(entries[1].kind(), "tag");
    assert_eq!(entries[1].meta().deleted(), Some(at(1_760_000_000_500)));
    assert_eq!(entries[1].stored(), STORED_TAG);
    assert_eq!(plaintext(&sample_header(), &entries), SAMPLE_PLAIN_V3);
}

/// Открытый текст версий 1 и 2 читается в те же записи, и в базу они
/// ложатся уже в текущей версии.
#[test]
fn older_plain_samples_still_read() {
    for (version, sealed, plain) in [
        (1, sealed_sample_v1(), SAMPLE_PLAIN_V1),
        (2, sealed_sample_v2(), SAMPLE_PLAIN_V2),
    ] {
        let entries = read(KINDS_UNDER_TEST, &sealed, plain.as_bytes())
            .unwrap_or_else(|error| panic!("образец версии {version}: {error:?}"));
        let stored: Vec<String> = entries.iter().map(Entry::stored).collect();
        assert_eq!(stored, [STORED_NOTE, STORED_TAG], "версия {version}");
    }
}

/// Записи базы версии 1 собираются в копию текущей версии.
#[test]
fn records_of_version_1_go_into_a_current_backup() {
    let collected = collect(
        KINDS_UNDER_TEST,
        [
            raw("note", NOTE_ID, STORED_NOTE_V1),
            raw("tag", TAG_ID, STORED_TAG_V1),
        ],
    )
    .expect("собрано");
    assert_eq!(collected.unreadable(), [] as [String; 0]);
    assert_eq!(
        plaintext(&sample_header(), collected.entries()),
        SAMPLE_PLAIN_V3
    );
}

/// Запись копии ложится в базу той же строкой, что дал бы `format::encode`:
/// база не различает записи из копии и записанные командой.
#[test]
fn entry_is_stored_as_encode_writes() {
    let note = Note {
        meta: RecordMeta::new(at(1_760_000_000_000)),
        text: "Գյումրի «кавычки» \"quotes\"".to_owned(),
    };
    let text = encode(&note);
    let id = note.meta.id().to_string();
    let collected = collect(KINDS_UNDER_TEST, [raw("note", &id, &text)]).expect("собрано");
    assert_eq!(collected.unreadable(), [] as [String; 0]);
    assert_eq!(collected.entries()[0].stored(), text);
}

/// База → копия → файл → обратно: те же записи.
#[test]
fn backup_round_trips() {
    let collected = collect(KINDS_UNDER_TEST, stored_pair()).expect("собрано");
    let header = sample_header();
    let text = plaintext(&header, collected.entries());
    // Шифра в ядре нет: «шифротекст» здесь — сам открытый текст.
    let file = seal(&header, &sample_lock(), text.as_bytes());
    let sealed = open(&file).expect("файл открывается");
    let entries = read(KINDS_UNDER_TEST, &sealed, sealed.ciphertext()).expect("записи читаются");
    let stored: Vec<String> = entries.iter().map(Entry::stored).collect();
    assert_eq!(stored, [STORED_NOTE, STORED_TAG]);
}

/// Пустая база — тоже копия: с пустым списком записей.
#[test]
fn empty_base_makes_an_empty_backup() {
    let collected = collect(KINDS_UNDER_TEST, []).expect("собрано");
    let text = plaintext(&sample_header(), collected.entries());
    assert_eq!(
        text,
        r#"{"file":"kvartik-backup","format_version":3,"app_version":"0.0.14","created":1760270400000,"records":[]}"#
    );
    let entries = read(&[], &sealed_sample(), text.as_bytes()).expect("пустая копия читается");
    assert!(entries.is_empty());
}

/// Копия берёт всё, что читается, а нечитаемое пропускает и называет:
/// испорченное, не строку, незнакомый вид и запись не под своим ключом.
#[test]
fn collect_skips_and_names_unreadable_records() {
    let ghost = STORED_TAG.replace(TAG_ID, NOTE_ID);
    let items = [
        raw("note", NOTE_ID, STORED_NOTE),
        raw("note", "1e2d3c4b-0000-4000-8000-000000000001", "не JSON"),
        Raw {
            kind: "note",
            id: "1e2d3c4b-0000-4000-8000-000000000002",
            text: None,
        },
        raw("ghost", NOTE_ID, &ghost),
        raw("tag", "1e2d3c4b-0000-4000-8000-000000000003", STORED_TAG),
        raw("tag", TAG_ID, STORED_TAG),
    ];
    let collected = collect(KINDS_UNDER_TEST, items).expect("собрано");
    let ids: Vec<String> = collected
        .entries()
        .iter()
        .map(|entry| format!("{} {}", entry.kind(), entry.meta().id()))
        .collect();
    assert_eq!(ids, [format!("note {NOTE_ID}"), format!("tag {TAG_ID}")]);
    assert_eq!(
        collected.unreadable().len(),
        4,
        "{:#?}",
        collected.unreadable()
    );
}

/// Запись новее программы — отказ всей копии, даже среди читаемых и даже
/// незнакомого вида: её пишет новая версия, и копию сделает она.
#[test]
fn collect_refuses_when_a_record_is_newer() {
    let next = FORMAT_VERSION + 1;
    let newer = format!(r#"{{"format_version":{next},"record":{{"wings":2}}}}"#);
    for kind in ["note", "unicorn"] {
        let items = [raw("note", NOTE_ID, STORED_NOTE), raw(kind, TAG_ID, &newer)];
        assert_eq!(
            collect(KINDS_UNDER_TEST, items).map(|c| c.entries().len()),
            Err(FormatError::Newer { found: next }),
            "вид {kind}"
        );
    }
}

/// Чужой файл, копия новее программы и испорченная копия — три разных
/// ответа: человеку о них говорят разными словами.
#[test]
fn open_tells_foreign_newer_and_damaged_apart() {
    let mut wrong = Vec::new();
    let mut expect = |text: &str, expected: &dyn Fn(&Result<Sealed, BackupError>) -> bool| {
        let got = open(text);
        if !expected(&got) {
            wrong.push(format!("{text:.140}: {got:?}"));
        }
    };
    let not_backup = |got: &Result<Sealed, BackupError>| got == &Err(BackupError::NotBackup);
    for text in [
        "",
        "не JSON",
        "[]",
        "{}",
        r#"{"name":"чужой файл"}"#,
        r#"{"file":"other-backup","format_version":1}"#,
        "\u{feff}{\"file\":\"kvartik-backup\",\"format_version\":1}",
    ] {
        expect(text, &not_backup);
    }

    let next = FORMAT_VERSION + 1;
    let newer =
        |got: &Result<Sealed, BackupError>| got == &Err(BackupError::Newer { found: next });
    for text in [
        format!(r#"{{"file":"kvartik-backup","format_version":{next}}}"#),
        format!(r#"{{"format_version":{next},"file":"kvartik-backup","wings":{{"left":1}}}}"#),
        SAMPLE_SEALED_V3.replace(
            r#""format_version":3"#,
            &format!(r#""format_version":{next}"#),
        ),
    ] {
        expect(&text, &newer);
    }

    let damaged = |got: &Result<Sealed, BackupError>| matches!(got, Err(BackupError::Damaged(_)));
    let mut cases = vec![
        // Метка своя, а версии нет — своя копия, но испорченная.
        r#"{"file":"kvartik-backup"}"#.to_owned(),
        SAMPLE_SEALED_V3[..SAMPLE_SEALED_V3.len() / 2].to_owned(),
        SAMPLE_SEALED_V3[..SAMPLE_SEALED_V3.len() - 1].to_owned(),
        SAMPLE_SEALED_V1[..SAMPLE_SEALED_V1.len() / 2].to_owned(),
        format!("{SAMPLE_SEALED_V3} хвост"),
        SAMPLE_SEALED_V3.replace(r#""format_version":3"#, r#""format_version":0"#),
        SAMPLE_SEALED_V3.replace(r#""format_version":3"#, r#""format_version":"2""#),
        SAMPLE_SEALED_V3.replace(r#""data""#, r#""extra":1,"data""#),
        SAMPLE_SEALED_V3.replace("PBKDF2-SHA256", "PBKDF2-SHA1"),
        SAMPLE_SEALED_V3.replace("AES-256-GCM", "AES-256-CBC"),
        SAMPLE_SEALED_V3.replace("600000", "0"),
        SAMPLE_SEALED_V3.replace("600000", "10000001"),
        SAMPLE_SEALED_V3.replace("AAECAwQFBgcICQoLDA0ODw==", "AAECAwQFBgcICQoLDA0O"),
        SAMPLE_SEALED_V3.replace("AAECAwQFBgcICQoLDA0ODw==", "не base64"),
        SAMPLE_SEALED_V3.replace("ZGVmZ2hpamtsbW5v", "ZGVmZ2hpamtsbW5vcA=="),
        SAMPLE_SEALED_V3.replace("yMnKy8zNzs/Q0dLT1NXW19jZ2ts=", "yMnKy8zNzs/Q0dLT1NXW"),
        SAMPLE_SEALED_V3.replace("yMnKy8zNzs/Q0dLT1NXW19jZ2ts=", "yMnK\ny8zN"),
        SAMPLE_SEALED_V3.replace(r#""0.0.14""#, r#""<b>0.0.12</b>""#),
        SAMPLE_SEALED_V3.replace(r#""0.0.14""#, r#""""#),
        SAMPLE_SEALED_V3.replace("1760270400000", "8640000000000001"),
        SAMPLE_SEALED_V3.replace(
            r#","cipher":{"name":"AES-256-GCM","iv":"ZGVmZ2hpamtsbW5v"}"#,
            "",
        ),
    ];
    // Вложенность глубже предела `serde_json` (128) — ошибка, а не
    // переполнение стека.
    cases.push(format!(
        r#"{{"file":"kvartik-backup","format_version":1,"data":{}{}}}"#,
        "[".repeat(10_000),
        "]".repeat(10_000)
    ));
    for text in &cases {
        expect(text, &damaged);
    }
    assert!(wrong.is_empty(), "не тот ответ: {wrong:#?}");
}

/// Открытый текст не читается, если он не совпадает с шапкой снаружи, не
/// наш, испорчен или в нём незнакомый вид, повтор или лишнее поле — и не
/// читается целиком: половины копии не бывает.
#[test]
fn read_is_all_or_nothing() {
    let sealed = sealed_sample();
    let second_note = SAMPLE_PLAIN_V3.replace(
        r#"]}"#,
        &format!(
            r#",{{"kind":"note","record":{{"meta":{{"id":"{NOTE_ID}","modified":1,"deleted":null}},"text":""}}}}]}}"#
        ),
    );
    let mut cases = vec![
        SAMPLE_PLAIN_V3.replace(r#""0.0.14""#, r#""0.0.15""#),
        SAMPLE_PLAIN_V3.replace("1760270400000", "1760270400001"),
        SAMPLE_PLAIN_V3.replace(r#""file":"kvartik-backup""#, r#""file":"other""#),
        SAMPLE_PLAIN_V3.replace(r#""format_version":3"#, r#""format_version":1"#),
        // Шапка внутри — версии 1, а снаружи — 3: подмена версии.
        SAMPLE_PLAIN_V1.replace(r#""0.0.8""#, r#""0.0.14""#),
        SAMPLE_PLAIN_V3.replace(r#""kind":"tag""#, r#""kind":"unicorn""#),
        SAMPLE_PLAIN_V3.replace(r#""text":"Ереван ֏""#, r#""text":7"#),
        SAMPLE_PLAIN_V3.replace(r#""text":"Ереван ֏""#, r#""text":"Ереван ֏","wings":2"#),
        SAMPLE_PLAIN_V3.replace(r#""records""#, r#""extra":1,"records""#),
        SAMPLE_PLAIN_V3.replace(r#"{"kind":"tag","#, r#"{"extra":1,"kind":"tag","#),
        SAMPLE_PLAIN_V3.replace(r#""deleted":1760000000500"#, r#""deleted":1760000000501"#),
        second_note,
        SAMPLE_PLAIN_V3[..SAMPLE_PLAIN_V3.len() - 2].to_owned(),
        "не JSON".to_owned(),
    ];
    cases.push(String::new());
    let mut wrong = Vec::new();
    for text in &cases {
        match read(KINDS_UNDER_TEST, &sealed, text.as_bytes()) {
            Err(BackupError::Damaged(_)) => {}
            other => wrong.push(format!("{text:.140}: {:?}", other.map(|e| e.len()))),
        }
    }
    let not_utf8 = [0xff, 0xfe, b'{', b'}'];
    if !matches!(
        read(KINDS_UNDER_TEST, &sealed, &not_utf8),
        Err(BackupError::Damaged(_))
    ) {
        wrong.push("не UTF-8 прочитано".to_owned());
    }
    assert!(wrong.is_empty(), "прочитано испорченное: {wrong:#?}");
}

/// Запись `note` с этим идентификатором, моментом изменения и текстом —
/// в конверте, как в базе.
fn stored_note(id: &str, modified: i64, text: &str) -> String {
    format!(
        r#"{{"format_version":1,"record":{{"meta":{{"id":"{id}","modified":{modified},"deleted":null}},"text":"{text}"}}}}"#
    )
}

/// Копия с одной записью `note`.
fn incoming_note(id: &str, modified: i64, text: &str) -> Vec<Entry> {
    let stored = stored_note(id, modified, text);
    collect(KINDS_UNDER_TEST, [raw("note", id, &stored)])
        .expect("собрано")
        .entries
}

/// Слияние: нового в базе нет — добавить; в копии позже — заменить; в базе
/// позже или так же — оставить; в базе испорчено — починить. Записи базы,
/// которых нет в копии, слияние не трогает вовсе.
#[test]
fn merge_keeps_the_later_record() {
    let id = NOTE_ID;
    let cases = [
        ("в базе нет", None, (1, 0, 0)),
        (
            "в копии позже",
            Some(stored_note(id, 1_000, "база")),
            (0, 1, 0),
        ),
        (
            "в базе позже",
            Some(stored_note(id, 3_000, "база")),
            (0, 0, 1),
        ),
        (
            "одновременно",
            Some(stored_note(id, 2_000, "база")),
            (0, 0, 1),
        ),
        ("в базе испорчено", Some("не JSON".to_owned()), (0, 1, 0)),
    ];
    let incoming = incoming_note(id, 2_000, "копия");
    let other = stored_note(TAG_ID, 5, "чужая");
    let mut wrong = Vec::new();
    for (name, existing, (added, updated, unchanged)) in cases {
        let mut base = vec![raw("note", TAG_ID, &other)];
        if let Some(text) = &existing {
            base.push(raw("note", id, text));
        }
        let merge = merge(KINDS_UNDER_TEST, &incoming, base).expect("слито");
        let got = (merge.added(), merge.updated(), merge.unchanged());
        let writes = merge.writes().len();
        if got != (added, updated, unchanged) || writes != added + updated {
            wrong.push(format!("{name}: {got:?}, записей {writes}"));
        }
    }
    assert!(wrong.is_empty(), "слияние ошиблось: {wrong:#?}");
}

/// Удаление в копии — правка: позднее удаление переезжает в базу, а запись,
/// восстановленная в базе после удаления, остаётся живой.
#[test]
fn merge_carries_deletion_marks() {
    let deleted = STORED_TAG; // удалена в 1760000000500
    let alive_before = r#"{"format_version":1,"record":{"meta":{"id":"92d87e64-1ccb-46d0-93d7-917b1e773af5","modified":1760000000000,"deleted":null}}}"#;
    let alive_after = r#"{"format_version":1,"record":{"meta":{"id":"92d87e64-1ccb-46d0-93d7-917b1e773af5","modified":1760000000900,"deleted":null}}}"#;
    let incoming = collect(KINDS_UNDER_TEST, [raw("tag", TAG_ID, deleted)])
        .expect("собрано")
        .entries;

    let merge_before = merge(
        KINDS_UNDER_TEST,
        &incoming,
        [raw("tag", TAG_ID, alive_before)],
    )
    .expect("слито");
    assert_eq!(merge_before.updated(), 1, "удаление не переехало");
    assert_eq!(
        merge_before.writes()[0].meta().deleted(),
        Some(at(1_760_000_000_500))
    );

    let merge_after = merge(
        KINDS_UNDER_TEST,
        &incoming,
        [raw("tag", TAG_ID, alive_after)],
    )
    .expect("слито");
    assert_eq!(
        merge_after.unchanged(),
        1,
        "восстановленную запись удалили снова"
    );
}

/// Запись базы новее программы — отказ всего слияния: старая версия её
/// не переписывает (Правило 2).
#[test]
fn merge_refuses_over_newer_records() {
    let incoming = incoming_note(NOTE_ID, 2_000, "копия");
    let next = FORMAT_VERSION + 1;
    let newer = format!(r#"{{"format_version":{next},"record":{{"wings":2}}}}"#);
    assert_eq!(
        merge(KINDS_UNDER_TEST, &incoming, [raw("note", NOTE_ID, &newer)]).map(|m| m.writes().len()),
        Err(FormatError::Newer { found: next })
    );
}

/// Две записи с одним идентификатором, но разного вида — разные записи.
#[test]
fn merge_tells_kinds_apart() {
    let incoming = incoming_note(TAG_ID, 2_000, "копия");
    let merge = merge(
        KINDS_UNDER_TEST,
        &incoming,
        [raw("tag", TAG_ID, STORED_TAG)],
    )
    .expect("слито");
    assert_eq!(merge.added(), 1);
}

/// Напоминание: только при данных, и через семь дней по календарю.
#[test]
fn reminder_comes_after_seven_days_with_data() {
    let today = day(2026, 10, 12);
    let cases = [
        ("данных нет, копий нет", None, false, false),
        (
            "данных нет, копия давно",
            Some(day(2026, 1, 1)),
            false,
            false,
        ),
        ("данные есть, копий нет", None, true, true),
        ("копия сегодня", Some(today), true, false),
        ("копия 6 дней назад", Some(day(2026, 10, 6)), true, false),
        ("копия 7 дней назад", Some(day(2026, 10, 5)), true, true),
        ("копия в прошлом году", Some(day(2025, 12, 31)), true, true),
        ("копия «из будущего»", Some(day(2026, 12, 1)), true, false),
    ];
    let mut wrong = Vec::new();
    for (name, last, has_data, expected) in cases {
        if reminder_due(last, today, has_data) != expected {
            wrong.push(name);
        }
    }
    assert!(wrong.is_empty(), "напоминание ошиблось: {wrong:?}");
}

/// После восстановления копия на момент файла у человека есть, но ни
/// прежняя отметка не уменьшается, ни дата «из будущего» не берётся.
#[test]
fn restore_moves_the_last_backup_only_forward() {
    let now = at(1_760_270_400_000);
    let created = at(1_760_000_000_000);
    let cases = [
        ("копий не было", None, created, Some(created)),
        ("прежняя раньше", Some(at(1_000)), created, Some(created)),
        ("прежняя позже", Some(at(1_760_100_000_000)), created, None),
        ("та же", Some(created), created, None),
        ("файл «из будущего»", None, at(1_860_000_000_000), None),
        ("файл сделан сейчас", None, now, Some(now)),
    ];
    let mut wrong = Vec::new();
    for (name, current, created, expected) in cases {
        let got = last_after_restore(current, created, now);
        if got != expected {
            wrong.push(format!("{name}: {got:?}, а не {expected:?}"));
        }
    }
    assert!(wrong.is_empty(), "отметка копии: {wrong:#?}");
}

#[test]
fn file_name_carries_the_date() {
    assert_eq!(file_name(day(2026, 10, 12)), "kvartik-2026-10-12.json");
    assert_eq!(file_name(day(2027, 1, 5)), "kvartik-2027-01-05.json");
}

/// Длина пароля — в знаках, а не в байтах UTF-8.
#[test]
fn password_rules() {
    let cases = [
        (
            "12345678901234",
            "12345678901234",
            Some(PasswordProblem::TooShort),
        ),
        ("123456789012345", "123456789012345", None),
        (
            "123456789012345",
            "123456789012346",
            Some(PasswordProblem::Mismatch),
        ),
        // 14 армянских букв — 28 байтов UTF-8, но 14 знаков.
        (
            "Երևանըհինքաղաք",
            "Երևանըհինքաղաք",
            Some(PasswordProblem::TooShort),
        ),
        ("Երևանը հին է շատ", "Երևանը հին է շատ", None),
        (
            "кот ест рыбу",
            "кот ест рыбу",
            Some(PasswordProblem::TooShort),
        ),
        ("кот ест свою рыбу", "кот ест свою рыбу", None),
        ("", "", Some(PasswordProblem::TooShort)),
    ];
    let mut wrong = Vec::new();
    for (password, repeat, expected) in cases {
        let got = new_password_problem(password, repeat);
        if got != expected {
            wrong.push(format!("«{password}»: {got:?}, а не {expected:?}"));
        }
    }
    assert!(wrong.is_empty(), "правила пароля: {wrong:#?}");
}

/// Пароль устройства записывается и читается обратно, а испорченная запись
/// не читается — тогда пароль задают заново.
#[test]
fn device_key_round_trips_and_rejects_damage() {
    let key = DeviceKey::new(600_000, (0..16).collect(), (0..32).collect()).expect("годится");
    let text = key.to_text();
    assert_eq!(
        text,
        r#"{"iterations":600000,"salt":"AAECAwQFBgcICQoLDA0ODw==","check":"AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8="}"#
    );
    assert_eq!(DeviceKey::from_text(&text).as_ref(), Some(&key));
    assert!(key.accepts(&(0..32).collect::<Vec<u8>>()));
    assert!(!key.accepts(&(1..33).collect::<Vec<u8>>()));
    assert!(!key.accepts(&[]));
    let rejected = [
        String::new(),
        "не JSON".to_owned(),
        text.replace("600000", "0"),
        text.replace("600000", "10000001"),
        text.replace("AAECAwQFBgcICQoLDA0ODw==", "AAECAwQFBgcICQoLDA0O"),
        text.replace(
            "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=",
            "AAECAwQFBgcICQoLDA0ODw==",
        ),
        text.replace(r#""check""#, r#""extra":1,"check""#),
        text.replace("ODw==", "ODw="),
    ];
    let read: Vec<&String> = rejected
        .iter()
        .filter(|text| DeviceKey::from_text(text).is_some())
        .collect();
    assert!(read.is_empty(), "прочитана испорченная запись: {read:#?}");
}

/// Замок годится только в пределах: итераций от одной до предела, соль от
/// 16 до 64 байтов, вектор — ровно 12.
#[test]
fn lock_limits() {
    let salt = |n: usize| vec![7_u8; n];
    let iv = |n: usize| vec![9_u8; n];
    assert!(Lock::new(KDF_ITERATIONS, salt(SALT_BYTES), iv(IV_BYTES)).is_some());
    assert!(Lock::new(1, salt(64), iv(12)).is_some());
    assert!(Lock::new(MAX_KDF_ITERATIONS, salt(16), iv(12)).is_some());
    let rejected = [
        Lock::new(0, salt(16), iv(12)),
        Lock::new(MAX_KDF_ITERATIONS + 1, salt(16), iv(12)),
        Lock::new(KDF_ITERATIONS, salt(15), iv(12)),
        Lock::new(KDF_ITERATIONS, salt(65), iv(12)),
        Lock::new(KDF_ITERATIONS, salt(16), iv(11)),
        Lock::new(KDF_ITERATIONS, salt(16), iv(16)),
    ];
    assert!(rejected.iter().all(Option::is_none), "{rejected:#?}");
}
