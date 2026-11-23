//! Тесты файла копии. Образцы выписаны человеком, а не записаны кодом:
//! код, записавший себе эталон, проверял бы сам себя (Правило 2). Шифра
//! здесь нет — его делает WebCrypto окна, и проверяют его браузерные тесты
//! (`crates/app/src/store/tests.rs`); вместо шифротекста — любые байты.

use serde::{Deserialize, Serialize};

use super::*;
use crate::format::encode;
use crate::model::profile::ProfileData;

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

/// Запись `note` в базе — в конверте текущей версии 27, как её кладёт
/// хранилище.
const STORED_NOTE: &str = concat!(
    r#"{"format_version":27,"record":{"meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","#,
    r#""modified":1760000000000,"deleted":null},"text":"Ереван ֏"}}"#
);

/// Запись `tag` в базе: удалена.
const STORED_TAG: &str = concat!(
    r#"{"format_version":27,"record":{"meta":{"id":"92d87e64-1ccb-46d0-93d7-917b1e773af5","#,
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

/// Открытый текст копии версии 3 — история: так копию писала 0.0.14.
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

/// Открытый текст копии версии 4 — история: так копию писала 0.0.15.
/// Выписан человеком: от версии 3 отличается номером версии и версией
/// программы в шапке.
const SAMPLE_PLAIN_V4: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":4,"app_version":"0.0.15","created":1760270400000,"#,
    r#""records":["#,
    r#"{"kind":"note","record":{"meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","#,
    r#""modified":1760000000000,"deleted":null},"text":"Ереван ֏"}},"#,
    r#"{"kind":"tag","record":{"meta":{"id":"92d87e64-1ccb-46d0-93d7-917b1e773af5","#,
    r#""modified":1760000000500,"deleted":1760000000500}}}"#,
    r#"]}"#
);

/// Файл копии версии 4 — с теми же солью, вектором и «шифротекстом», что
/// у образцов прошлых версий.
const SAMPLE_SEALED_V4: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":4,"app_version":"0.0.15","created":1760270400000,"#,
    r#""kdf":{"name":"PBKDF2-SHA256","iterations":600000,"salt":"AAECAwQFBgcICQoLDA0ODw=="},"#,
    r#""cipher":{"name":"AES-256-GCM","iv":"ZGVmZ2hpamtsbW5v"},"#,
    r#""data":"yMnKy8zNzs/Q0dLT1NXW19jZ2ts="}"#
);

/// Открытый текст копии версии 5 — история: так копию писала 0.0.17.
/// Выписан человеком: от версии 4 отличается номером версии и версией
/// программы в шапке.
const SAMPLE_PLAIN_V5: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":5,"app_version":"0.0.17","created":1760270400000,"#,
    r#""records":["#,
    r#"{"kind":"note","record":{"meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","#,
    r#""modified":1760000000000,"deleted":null},"text":"Ереван ֏"}},"#,
    r#"{"kind":"tag","record":{"meta":{"id":"92d87e64-1ccb-46d0-93d7-917b1e773af5","#,
    r#""modified":1760000000500,"deleted":1760000000500}}}"#,
    r#"]}"#
);

/// Файл копии версии 5 — с теми же солью, вектором и «шифротекстом», что
/// у образцов прошлых версий.
const SAMPLE_SEALED_V5: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":5,"app_version":"0.0.17","created":1760270400000,"#,
    r#""kdf":{"name":"PBKDF2-SHA256","iterations":600000,"salt":"AAECAwQFBgcICQoLDA0ODw=="},"#,
    r#""cipher":{"name":"AES-256-GCM","iv":"ZGVmZ2hpamtsbW5v"},"#,
    r#""data":"yMnKy8zNzs/Q0dLT1NXW19jZ2ts="}"#
);

/// Открытый текст копии версии 6 — история: так копию писала 0.0.27, до
/// сжатия. Выписан человеком: от версии 5 отличается номером версии
/// и версией программы в шапке.
const SAMPLE_PLAIN_V6: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":6,"app_version":"0.0.27","created":1760270400000,"#,
    r#""records":["#,
    r#"{"kind":"note","record":{"meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","#,
    r#""modified":1760000000000,"deleted":null},"text":"Ереван ֏"}},"#,
    r#"{"kind":"tag","record":{"meta":{"id":"92d87e64-1ccb-46d0-93d7-917b1e773af5","#,
    r#""modified":1760000000500,"deleted":1760000000500}}}"#,
    r#"]}"#
);

/// Тот же текст, сжатый DEFLATE без обёртки (RFC 1951), в base64 — так
/// версия 6 кладёт его под шифр. Сжат не этим кодом и не `miniz_oxide`,
/// а модулем `zlib` Python (`compressobj(9, DEFLATED, -15)`): тест держит
/// сам формат, а не то, как его пишет наша библиотека.
const SAMPLE_PACKED_V6: &str = concat!(
    "dY5NasQwDIWvUrS2B9v58SRXKaUotlxMfvF4hkIw9AZd9R6l0GXvkG56niaFmTKLeRshvSfpm8H5jqCG9oQh+pY3",
    "aNrjBAzcGHqMjycKBz8OUJcMcJr+exA7sVN6TZpAGMlCLXUplBa52MQgkBmDPUB9P0PrhzUAwxgJzg7UM/QUcat+",
    "c4XbmwwLxYtGEs+NtrxC1fCScidtZlQjcd3uR+udPz+8iIGljv5AhmPXJQaRnuN6dnn7flk+l4/lffm6+3mFlNgF",
    "KOLTbZ5K2b2mMufSmIbnpRW8yqzmldQroNYZuuIGT3HFczVPKT2kXw=="
);

/// Файл копии версии 6 — с теми же солью, вектором и «шифротекстом», что
/// у образцов прошлых версий.
const SAMPLE_SEALED_V6: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":6,"app_version":"0.0.27","created":1760270400000,"#,
    r#""kdf":{"name":"PBKDF2-SHA256","iterations":600000,"salt":"AAECAwQFBgcICQoLDA0ODw=="},"#,
    r#""cipher":{"name":"AES-256-GCM","iv":"ZGVmZ2hpamtsbW5v"},"#,
    r#""data":"yMnKy8zNzs/Q0dLT1NXW19jZ2ts="}"#
);

/// Открытый текст копии версии 7 — история: так её писала 0.0.40, до
/// сжатия. Выписан человеком: от версии 6 отличается номером версии
/// и версией программы в шапке.
const SAMPLE_PLAIN_V7: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":7,"app_version":"0.0.40","created":1760270400000,"#,
    r#""records":["#,
    r#"{"kind":"note","record":{"meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","#,
    r#""modified":1760000000000,"deleted":null},"text":"Ереван ֏"}},"#,
    r#"{"kind":"tag","record":{"meta":{"id":"92d87e64-1ccb-46d0-93d7-917b1e773af5","#,
    r#""modified":1760000000500,"deleted":1760000000500}}}"#,
    r#"]}"#
);

/// Тот же текст, сжатый, как образец версии 6, модулем `zlib` Python
/// (`compressobj(9, DEFLATED, -15)`), а не этим кодом. Тот же способ дал
/// образец версии 6 слово в слово (2026-10-16).
const SAMPLE_PACKED_V7: &str = concat!(
    "dY5NasQwDIWvUrS2Bztx4kmuUkpRbLmY/OLxDIVg6A266j1KocveId30PE0KM2UW8zZCek/SN4PzHUEN7QlD9C1v",
    "0LTHCRi4MfQYH08UDn4coNYMcJr+exA7sVNiTZpAGMlCLXUpMi2U2MQgkBmDPUB9P0PrhzUAwxgJzg7UM/QUcat+",
    "c4XbmxyLjBeNJK6MtrzCrOElKSdtbrJG4rrdj9Y7f354EQNLHf2BDMeuSwwiPcf17PL2/bJ8Lh/L+/J19/MKKbEL",
    "UMSn2zxVZveaSsWlMQ1XpRW8yq3mldQroNY5uuIGT3HFczVPKT2kXw=="
);

/// Файл копии версии 7 — с теми же солью, вектором и «шифротекстом», что
/// у образцов прошлых версий.
const SAMPLE_SEALED_V7: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":7,"app_version":"0.0.40","created":1760270400000,"#,
    r#""kdf":{"name":"PBKDF2-SHA256","iterations":600000,"salt":"AAECAwQFBgcICQoLDA0ODw=="},"#,
    r#""cipher":{"name":"AES-256-GCM","iv":"ZGVmZ2hpamtsbW5v"},"#,
    r#""data":"yMnKy8zNzs/Q0dLT1NXW19jZ2ts="}"#
);

/// Открытый текст копии версии 8 — история: так её писала 0.0.47, до
/// сжатия. Выписан человеком: от версии 7 отличается номером версии
/// и версией программы в шапке.
const SAMPLE_PLAIN_V8: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":8,"app_version":"0.0.47","created":1760270400000,"#,
    r#""records":["#,
    r#"{"kind":"note","record":{"meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","#,
    r#""modified":1760000000000,"deleted":null},"text":"Ереван ֏"}},"#,
    r#"{"kind":"tag","record":{"meta":{"id":"92d87e64-1ccb-46d0-93d7-917b1e773af5","#,
    r#""modified":1760000000500,"deleted":1760000000500}}}"#,
    r#"]}"#
);

/// Тот же текст, сжатый, как образцы версий 6 и 7, модулем `zlib` Python
/// (`compressobj(9, DEFLATED, -15)`), а не этим кодом (2026-10-19).
const SAMPLE_PACKED_V8: &str = concat!(
    "dY5BasQwDEWvUrS2Bztx4iRXKaUotlxMMknweIZCMPQGXfUepdBl75Buep4mhZkyi/kbIf0v6c3gfE/QQHfCEH3H",
    "WzTdcQIGbgx7jI8nCgc/DtBUDHCa/nsQO7FTek2aQBjJQiN1KTItlNjEIJAZgz1Acz9D54c1AMMYCc4ONDPsKeJW",
    "/eYKV5kci4wXrSSujLa8xqzlJSknbW6yVuK6vR+td/788CIGlnr6AxmOfZ8YRHqO69nl7ftl+Vw+lvfl6+7nFVJi",
    "F6CIT7d56sxWmkrFpTEtV6UVvM6t5rXUK6DWObriBk9xxXM1Tyk9pF8="
);

/// Файл копии версии 8 — с теми же солью, вектором и «шифротекстом», что
/// у образцов прошлых версий.
const SAMPLE_SEALED_V8: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":8,"app_version":"0.0.47","created":1760270400000,"#,
    r#""kdf":{"name":"PBKDF2-SHA256","iterations":600000,"salt":"AAECAwQFBgcICQoLDA0ODw=="},"#,
    r#""cipher":{"name":"AES-256-GCM","iv":"ZGVmZ2hpamtsbW5v"},"#,
    r#""data":"yMnKy8zNzs/Q0dLT1NXW19jZ2ts="}"#
);

/// Открытый текст копии версии 9 — история: так её писала 0.0.49, до
/// сжатия. Выписан человеком: от версии 8 отличается номером версии
/// и версией программы в шапке.
const SAMPLE_PLAIN_V9: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":9,"app_version":"0.0.49","created":1760270400000,"#,
    r#""records":["#,
    r#"{"kind":"note","record":{"meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","#,
    r#""modified":1760000000000,"deleted":null},"text":"Ереван ֏"}},"#,
    r#"{"kind":"tag","record":{"meta":{"id":"92d87e64-1ccb-46d0-93d7-917b1e773af5","#,
    r#""modified":1760000000500,"deleted":1760000000500}}}"#,
    r#"]}"#
);

/// Тот же текст, сжатый, как образцы версий 6–8, модулем `zlib` Python
/// (`compressobj(9, DEFLATED, -15)`), а не этим кодом (2026-10-20).
const SAMPLE_PACKED_V9: &str = concat!(
    "dY5NasMwEIWvUmYtBcmWrchXKaXI0qgI/8goSigYQW/QVe9RCl32Du6m56ldSEoWeZth5r2Z+WZwvkdooDvpmHxH",
    "W2264wQEXIiDTo8njAcfRmgUAT1N/z2wHdsJtSZNRJ3QQsNlzQrJBNtEIKIJ0R6guZ+h8+MagDEkhLMDzQwDJr1V",
    "v7nM7U2pq4JWLUcqjLRU6aKlNQrHbWmKlut1ewjWO39+eBEBiz3+gYzHvs8EEj6n9ezy9v2yfC4fy/vydffzCjmT",
    "C1DST7d5VGH3EmtBuTEtFbVlVJVWUsXlCihlqV11g6e64rma55wf8i8="
);

/// Файл копии версии 9 — с теми же солью, вектором и «шифротекстом», что
/// у образцов прошлых версий.
const SAMPLE_SEALED_V9: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":9,"app_version":"0.0.49","created":1760270400000,"#,
    r#""kdf":{"name":"PBKDF2-SHA256","iterations":600000,"salt":"AAECAwQFBgcICQoLDA0ODw=="},"#,
    r#""cipher":{"name":"AES-256-GCM","iv":"ZGVmZ2hpamtsbW5v"},"#,
    r#""data":"yMnKy8zNzs/Q0dLT1NXW19jZ2ts="}"#
);

/// Открытый текст копии версии 10 — история: так её писала 0.0.55, до
/// сжатия. Выписан человеком: от версии 9 отличается номером версии
/// и версией программы в шапке.
const SAMPLE_PLAIN_V10: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":10,"app_version":"0.0.55","created":1760270400000,"#,
    r#""records":["#,
    r#"{"kind":"note","record":{"meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","#,
    r#""modified":1760000000000,"deleted":null},"text":"Ереван ֏"}},"#,
    r#"{"kind":"tag","record":{"meta":{"id":"92d87e64-1ccb-46d0-93d7-917b1e773af5","#,
    r#""modified":1760000000500,"deleted":1760000000500}}}"#,
    r#"]}"#
);

/// Тот же текст, сжатый, как образцы версий 6–9, модулем `zlib` Python
/// (`compressobj(9, DEFLATED, -15)`), а не этим кодом (2026-10-21).
const SAMPLE_PACKED_V10: &str = concat!(
    "dY5NasMwEIWvUmYtBcm2rNhXKaXI0qgI/8goSigYQW/QVe9RCl32Du6m56lcSEoWeZuB997MfAtYNyC00J9UiK6n",
    "ndL9cQYC1odRxccThoPzE7ScEVDz/G8A27GdELmqA6qIJndkzQrJKraJQEDtgzlAe79A76ZcgMlHhHMC7QIjRrVN",
    "t6XM7nWpREFFx5FWWhraqKKjNVaWm1IXHVd5e/TGWXd+eBEBgwP+gUzHYUgEIj7HfHZ9+35ZP9eP9X39uvt5hZTI",
    "BSiqp9s8TWH2EuuKcq07WtWG0aY0kjZcZkApS2XFDR5xxXPlp5Qe0i8="
);

/// Файл копии версии 10 — с теми же солью, вектором и «шифротекстом», что
/// у образцов прошлых версий.
const SAMPLE_SEALED_V10: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":10,"app_version":"0.0.55","created":1760270400000,"#,
    r#""kdf":{"name":"PBKDF2-SHA256","iterations":600000,"salt":"AAECAwQFBgcICQoLDA0ODw=="},"#,
    r#""cipher":{"name":"AES-256-GCM","iv":"ZGVmZ2hpamtsbW5v"},"#,
    r#""data":"yMnKy8zNzs/Q0dLT1NXW19jZ2ts="}"#
);

/// Открытый текст копии версии 11 — история: так её писала 0.0.60, до
/// сжатия. Выписан человеком: от версии 10 отличается номером версии
/// и версией программы в шапке.
const SAMPLE_PLAIN_V11: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":11,"app_version":"0.0.60","created":1760270400000,"#,
    r#""records":["#,
    r#"{"kind":"note","record":{"meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","#,
    r#""modified":1760000000000,"deleted":null},"text":"Ереван ֏"}},"#,
    r#"{"kind":"tag","record":{"meta":{"id":"92d87e64-1ccb-46d0-93d7-917b1e773af5","#,
    r#""modified":1760000000500,"deleted":1760000000500}}}"#,
    r#"]}"#
);

/// Тот же текст, сжатый, как образцы версий 6–10, модулем `zlib` Python
/// (`compressobj(9, DEFLATED, -15)`), а не этим кодом (2026-10-27).
const SAMPLE_PACKED_V11: &str = concat!(
    "dY5NSgQxEIWvIrVOhqT/Mt1XEZHqpCKhf8lkBqEJeANX3kMEl96h3Xge08KMzGLepuC9V1XfAtb1BA10J/TBdbxF",
    "3R1nYGAnP2B4PJE/uGmERkoGOM//Boid2FUiVbUnDGRSR1UiU6IQmxh40pM3B2juF+jcmAowToHgnECzwEABt+m2",
    "VNi9zrHMeNlK4oVWhteYtbyiwkqT66yVmLaHyTjrzg8vYmCopz+Q8dj3kUGg55DOrm/fL+vn+rG+r193P68QI7sA",
    "BXy6zVNnZq+oKrjUuuVFZQSvc6N4LVUCVCpHW97gKa94rvwY40P8BQ=="
);

/// Файл копии версии 11 — с теми же солью, вектором и «шифротекстом», что
/// у образцов прошлых версий.
const SAMPLE_SEALED_V11: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":11,"app_version":"0.0.60","created":1760270400000,"#,
    r#""kdf":{"name":"PBKDF2-SHA256","iterations":600000,"salt":"AAECAwQFBgcICQoLDA0ODw=="},"#,
    r#""cipher":{"name":"AES-256-GCM","iv":"ZGVmZ2hpamtsbW5v"},"#,
    r#""data":"yMnKy8zNzs/Q0dLT1NXW19jZ2ts="}"#
);

/// Открытый текст копии версии 12 — история: так её писала 0.0.66, до
/// сжатия. Выписан человеком: от версии 11 отличается номером версии
/// и версией программы в шапке.
const SAMPLE_PLAIN_V12: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":12,"app_version":"0.0.66","created":1760270400000,"#,
    r#""records":["#,
    r#"{"kind":"note","record":{"meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","#,
    r#""modified":1760000000000,"deleted":null},"text":"Ереван ֏"}},"#,
    r#"{"kind":"tag","record":{"meta":{"id":"92d87e64-1ccb-46d0-93d7-917b1e773af5","#,
    r#""modified":1760000000500,"deleted":1760000000500}}}"#,
    r#"]}"#
);

/// Тот же текст, сжатый, как образцы версий 6–11, модулем `zlib` Python
/// (`compressobj(9, DEFLATED, -15)`), а не этим кодом (2026-11-01).
const SAMPLE_PACKED_V12: &str = concat!(
    "dY5NasMwEIWvUmYtBUm2pdhXKaXI0qgI/6IooWAEvUFXvUcpdNk7uJuep3IhKVnkbQbeezPzLeB8j9BAd9Ih+o62",
    "2nTHGQi4KQw6Pp4wHPw0QsMFAT3P/wawHdtJmasmoI5oc0dJJhQr2SYCAc0U7AGa+wU6P+YCjFNEOCfQLDBg1Nv0",
    "W8rc3hS6ErRqOdLSKEtrLVoqsXTcFka0XOftYbLe+fPDiwhY7PEPZDz2fSIQ8Tnms+vb98v6uX6s7+vX3c8rpEQu",
    "QFE/3eaphd0rlCXlxrS0lJbRurCK1lxlQKUK7aobPNUVz5WfUnpIvw=="
);

/// Файл копии версии 12 — с теми же солью, вектором и «шифротекстом», что
/// у образцов прошлых версий.
const SAMPLE_SEALED_V12: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":12,"app_version":"0.0.66","created":1760270400000,"#,
    r#""kdf":{"name":"PBKDF2-SHA256","iterations":600000,"salt":"AAECAwQFBgcICQoLDA0ODw=="},"#,
    r#""cipher":{"name":"AES-256-GCM","iv":"ZGVmZ2hpamtsbW5v"},"#,
    r#""data":"yMnKy8zNzs/Q0dLT1NXW19jZ2ts="}"#
);

/// Открытый текст копии версии 13 — история: так её писала 0.0.68, до
/// сжатия. Выписан человеком: от версии 12 отличается номером версии
/// и версией программы в шапке.
const SAMPLE_PLAIN_V13: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":13,"app_version":"0.0.68","created":1760270400000,"#,
    r#""records":["#,
    r#"{"kind":"note","record":{"meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","#,
    r#""modified":1760000000000,"deleted":null},"text":"Ереван ֏"}},"#,
    r#"{"kind":"tag","record":{"meta":{"id":"92d87e64-1ccb-46d0-93d7-917b1e773af5","#,
    r#""modified":1760000000500,"deleted":1760000000500}}}"#,
    r#"]}"#
);

/// Тот же текст, сжатый, как образцы версий 6–12, модулем `zlib` Python
/// (`compressobj(9, DEFLATED, -15)`), а не этим кодом (2026-11-01).
const SAMPLE_PACKED_V13: &str = concat!(
    "dY5NasMwEIWvUmYtBcl/sn2VUspYGhVhxzaKEgpG0Bt01XuUQpe9g7vpeSoXkpJF3mbgvTcz3wLWDQQt9Cf0wfW8Q9",
    "0fZ2BgJ7/H8Hgif3DTCK3MGeA8/xsgdmJX1amqPWEgkzqqEpkShdjEwJOevDlAe79A78ZUgHEKBOcE2gX2FHCbbkuF",
    "rXWOZcbLThIvtDK8wazjFRVWmlxnncS0vZ+Ms+788CIGhgb6AxmPwxAZBHoO6ez69v2yfq4f6/v6dffzCjGyC1DAp9",
    "s8TWZqRVXBpdYdLyojeJMbxRupEqBSOdryBk95xXPlxxgf4i8="
);

/// Файл копии версии 13 — с теми же солью, вектором и «шифротекстом», что
/// у образцов прошлых версий.
const SAMPLE_SEALED_V13: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":13,"app_version":"0.0.68","created":1760270400000,"#,
    r#""kdf":{"name":"PBKDF2-SHA256","iterations":600000,"salt":"AAECAwQFBgcICQoLDA0ODw=="},"#,
    r#""cipher":{"name":"AES-256-GCM","iv":"ZGVmZ2hpamtsbW5v"},"#,
    r#""data":"yMnKy8zNzs/Q0dLT1NXW19jZ2ts="}"#
);

/// Открытый текст копии версии 14 — так её писали 0.0.69–0.0.86.
const SAMPLE_PLAIN_V14: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":14,"app_version":"0.0.69","created":1760270400000,"#,
    r#""records":["#,
    r#"{"kind":"note","record":{"meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","#,
    r#""modified":1760000000000,"deleted":null},"text":"Ереван ֏"}},"#,
    r#"{"kind":"tag","record":{"meta":{"id":"92d87e64-1ccb-46d0-93d7-917b1e773af5","#,
    r#""modified":1760000000500,"deleted":1760000000500}}}"#,
    r#"]}"#
);

/// Тот же текст, сжатый модулем `zlib` Python (2026-11-02).
const SAMPLE_PACKED_V14: &str = concat!(
    "dY5NasMwEIWvUmYtBcmWrdhXKaXI0qgI/8goSigYQW/QVe9RCl32Du6m56lcSEoWeZuB997MfAtYNyC00J9UiK6nndL9",
    "cQYC1odRxccThoPzE7RcEFDz/G8A27Fd3eSqDqgimtyRNSskE2wTgYDaB3OA9n6B3k25AJOPCOcE2gVGjGqbbkuZ3etS",
    "VQWtOo5UaGloo4qO1igsN6UuOq7y9uiNs+788CICBgf8A5mOw5AIRHyO+ez69v2yfq4f6/v6dffzCimRC1BUT7d5msLs",
    "JdaCcq07KmrDaFMaSRsuM6CUpbLVDZ7qiufKTyk9pF8="
);

/// Файл копии версии 14.
const SAMPLE_SEALED_V14: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":14,"app_version":"0.0.69","created":1760270400000,"#,
    r#""kdf":{"name":"PBKDF2-SHA256","iterations":600000,"salt":"AAECAwQFBgcICQoLDA0ODw=="},"#,
    r#""cipher":{"name":"AES-256-GCM","iv":"ZGVmZ2hpamtsbW5v"},"#,
    r#""data":"yMnKy8zNzs/Q0dLT1NXW19jZ2ts="}"#
);

/// Открытый текст копии версии 15 — так её писали 0.0.87–0.0.101.
const SAMPLE_PLAIN_V15: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":15,"app_version":"0.0.87","created":1760270400000,"#,
    r#""records":["#,
    r#"{"kind":"note","record":{"meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","#,
    r#""modified":1760000000000,"deleted":null},"text":"Ереван ֏"}},"#,
    r#"{"kind":"tag","record":{"meta":{"id":"92d87e64-1ccb-46d0-93d7-917b1e773af5","#,
    r#""modified":1760000000500,"deleted":1760000000500}}}"#,
    r#"]}"#
);

/// Тот же текст, сжатый модулем `zlib` Python (2026-11-06).
const SAMPLE_PACKED_V15: &str = concat!(
    "dY5NasMwEIWvUmYtBcl/sn2VUspYGhVhxzaKEgpG0Bt01XuUQpe9g7vpeSoXkpJF3mbgvTcz3wLWDQQt9Cf0wfW8Q90f",
    "Z2BgJ7/H8Hgif3DTCK0sGeA8/xsgdmJXq1TVnjCQSR1ViUyJQmxi4ElP3hygvV+gd2MqwDgFgnMC7QJ7CrhNt6XC1jrH",
    "MuNlJ4kXWhneYNbxigorTa6zTmLa3k/GWXd+eBEDQwP9gYzHYYgMAj2HdHZ9+35ZP9eP9X39uvt5hRjZBSjg022eJjO1",
    "oqrgUuuOF5URvMmN4o1UCVCpHG15g6e84rnyY4wP8Rc="
);

/// Файл копии версии 15.
const SAMPLE_SEALED_V15: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":15,"app_version":"0.0.87","created":1760270400000,"#,
    r#""kdf":{"name":"PBKDF2-SHA256","iterations":600000,"salt":"AAECAwQFBgcICQoLDA0ODw=="},"#,
    r#""cipher":{"name":"AES-256-GCM","iv":"ZGVmZ2hpamtsbW5v"},"#,
    r#""data":"yMnKy8zNzs/Q0dLT1NXW19jZ2ts="}"#
);

/// Открытый текст копии версии 16 — так её писала 0.0.102.
const SAMPLE_PLAIN_V16: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":16,"app_version":"0.0.102","created":1760270400000,"#,
    r#""records":["#,
    r#"{"kind":"note","record":{"meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","#,
    r#""modified":1760000000000,"deleted":null},"text":"Ереван ֏"}},"#,
    r#"{"kind":"tag","record":{"meta":{"id":"92d87e64-1ccb-46d0-93d7-917b1e773af5","#,
    r#""modified":1760000000500,"deleted":1760000000500}}}"#,
    r#"]}"#
);

/// Тот же текст, сжатый модулем `zlib` Python (2026-11-10).
const SAMPLE_PACKED_V16: &str = concat!(
    "dY5NasMwEIWvUmYtBUn+UeyrlFJkaVSEf2QUJRSMoDfoqvcohS57B3fT81QuJCWLvM3Ae29mvgWsGxBa6E8qRNfTTun+OAMB",
    "68Oo4uMJw8H5CVpeE1Dz/G8A27EdZyJ3dUAV0eSSrJmQrGSbCATUPpgDtPcL9G7KBZh8RDgn0C4wYlTbdFvK7F4XqhK06jjS",
    "UktDGyU6WmNpuSm06LjK26M3zrrzw4sIGBzwD2Q6DkMiEPE55rPr2/fL+rl+rO/r193PK6RELkBRPd3maYTZS6xLyrXuaFkb",
    "RpvCSNpwmQGlLJStbvBUVzxXfkrpIf0C"
);

/// Файл копии версии 16.
const SAMPLE_SEALED_V16: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":16,"app_version":"0.0.102","created":1760270400000,"#,
    r#""kdf":{"name":"PBKDF2-SHA256","iterations":600000,"salt":"AAECAwQFBgcICQoLDA0ODw=="},"#,
    r#""cipher":{"name":"AES-256-GCM","iv":"ZGVmZ2hpamtsbW5v"},"#,
    r#""data":"yMnKy8zNzs/Q0dLT1NXW19jZ2ts="}"#
);

/// Открытый текст копии версии 17 — так её писали 0.0.103–0.0.118.
const SAMPLE_PLAIN_V17: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":17,"app_version":"0.0.103","created":1760270400000,"#,
    r#""records":["#,
    r#"{"kind":"note","record":{"meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","#,
    r#""modified":1760000000000,"deleted":null},"text":"Ереван ֏"}},"#,
    r#"{"kind":"tag","record":{"meta":{"id":"92d87e64-1ccb-46d0-93d7-917b1e773af5","#,
    r#""modified":1760000000500,"deleted":1760000000500}}}"#,
    r#"]}"#
);

/// Тот же текст, сжатый модулем `zlib` Python (2026-11-10).
const SAMPLE_PACKED_V17: &str = concat!(
    "dY5NasMwEIWvUmYtBcmWrdhXKaWMpVER/kVRQsEIeoOueo9S6LJ3cDc9T+1CUrLI2wy892bmm8H5jqCG9oQh+pY3aNrjBAzc",
    "GHqMjycKBz8OUEvNAKfp3wCxEzsp8rVrAmEku5VKkWmhxCYGgcwY7AHq+xlaP6wFGMZIcE6gnqGniNv0Wyrc3uRYZLxoJHFl",
    "tOUVZg0vSTlpc5M1EtftfrTe+fPDixhY6ugPZDh2XWIQ6TmuZ5e375flc/lY3pevu59XSIldgCI+3eapMrvXVCoujWm4Kq3g",
    "VW41r6ReAbXO0RU3eIornis/pfSQfgE="
);

/// Файл копии версии 17.
const SAMPLE_SEALED_V17: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":17,"app_version":"0.0.103","created":1760270400000,"#,
    r#""kdf":{"name":"PBKDF2-SHA256","iterations":600000,"salt":"AAECAwQFBgcICQoLDA0ODw=="},"#,
    r#""cipher":{"name":"AES-256-GCM","iv":"ZGVmZ2hpamtsbW5v"},"#,
    r#""data":"yMnKy8zNzs/Q0dLT1NXW19jZ2ts="}"#
);

/// Открытый текст копии версии 18 — так её писала 0.0.119.
const SAMPLE_PLAIN_V18: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":18,"app_version":"0.0.119","created":1760270400000,"#,
    r#""records":["#,
    r#"{"kind":"note","record":{"meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","#,
    r#""modified":1760000000000,"deleted":null},"text":"Ереван ֏"}},"#,
    r#"{"kind":"tag","record":{"meta":{"id":"92d87e64-1ccb-46d0-93d7-917b1e773af5","#,
    r#""modified":1760000000500,"deleted":1760000000500}}}"#,
    r#"]}"#
);

/// Тот же текст, сжатый модулем `zlib` Python (2026-11-12).
const SAMPLE_PACKED_V18: &str = concat!(
    "dY5NasMwEIWvUmYtBcl/sn2VUspYGhVhxzaKEgpG0Bt01XuUQpe9g7vpeSoXkpJF3mbgvTcz3wLWDQQt9Cf0wfW8Q90fZ2Bg",
    "J7/H8Hgif3DTCK2sGeA8/xsgdmInZZO62hMGMqmkKpEpUYhNDDzpyZsDtPcL9G5MBRinQHBOoF1gTwG36bZU2FrnWGa87CTx",
    "QivDG8w6XlFhpcl11klM2/vJOOvODy9iYGigP5DxOAyRQaDnkM6ub98v6+f6sb6vX3c/rxAjuwAFfLrN02SmVlQVXGrd8aIy",
    "gje5UbyRKgEqlaMtb/CUVzxXfozxIf4C"
);

/// Файл копии версии 18.
const SAMPLE_SEALED_V18: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":18,"app_version":"0.0.119","created":1760270400000,"#,
    r#""kdf":{"name":"PBKDF2-SHA256","iterations":600000,"salt":"AAECAwQFBgcICQoLDA0ODw=="},"#,
    r#""cipher":{"name":"AES-256-GCM","iv":"ZGVmZ2hpamtsbW5v"},"#,
    r#""data":"yMnKy8zNzs/Q0dLT1NXW19jZ2ts="}"#
);

/// Открытый текст копии версии 19 — так её писала 0.0.120.
const SAMPLE_PLAIN_V19: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":19,"app_version":"0.0.120","created":1760270400000,"#,
    r#""records":["#,
    r#"{"kind":"note","record":{"meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","#,
    r#""modified":1760000000000,"deleted":null},"text":"Ереван ֏"}},"#,
    r#"{"kind":"tag","record":{"meta":{"id":"92d87e64-1ccb-46d0-93d7-917b1e773af5","#,
    r#""modified":1760000000500,"deleted":1760000000500}}}"#,
    r#"]}"#
);

/// Тот же текст, сжатый модулем `zlib` Python (2026-11-13).
const SAMPLE_PACKED_V19: &str = concat!(
    "dY5NasMwEIWvUmYtBUn+UeyrlFJkaVSEf2QUJRSMoDfoqvcohS57B3fT81QuJCWLvM3Ae29mvgWsGxBa6E8qRNfTTun+OAMB",
    "68Oo4uMJw8H5CVreEFDz/G8A27EdFyx3dUAV0eSSrJmQrGSbCATUPpgDtPcL9G7KBZh8RDgn0C4wYlTbdFvK7F4XqhK06jjS",
    "UktDGyU6WmNpuSm06LjK26M3zrrzw4sIGBzwD2Q6DkMiEPE55rPr2/fL+rl+rO/r193PK6RELkBRPd3maYTZS6xLyrXuaFkb",
    "RpvCSNpwmQGlLJStbvBUVzxXfkrpIf0C"
);

/// Файл копии версии 19.
const SAMPLE_SEALED_V19: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":19,"app_version":"0.0.120","created":1760270400000,"#,
    r#""kdf":{"name":"PBKDF2-SHA256","iterations":600000,"salt":"AAECAwQFBgcICQoLDA0ODw=="},"#,
    r#""cipher":{"name":"AES-256-GCM","iv":"ZGVmZ2hpamtsbW5v"},"#,
    r#""data":"yMnKy8zNzs/Q0dLT1NXW19jZ2ts="}"#
);

/// Открытый текст копии версии 20 — так её писала 0.0.121.
const SAMPLE_PLAIN_V20: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":20,"app_version":"0.0.121","created":1760270400000,"#,
    r#""records":["#,
    r#"{"kind":"note","record":{"meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","#,
    r#""modified":1760000000000,"deleted":null},"text":"Ереван ֏"}},"#,
    r#"{"kind":"tag","record":{"meta":{"id":"92d87e64-1ccb-46d0-93d7-917b1e773af5","#,
    r#""modified":1760000000500,"deleted":1760000000500}}}"#,
    r#"]}"#
);

/// Тот же текст, сжатый модулем `zlib` Python (2026-11-13).
const SAMPLE_PACKED_V20: &str = concat!(
    "dY5NasMwEIWvUmYtBUn+UeyrlFLG0qgI/6IooWAEvUFXvUcpdNk7uJuep3YhKVnkbQbeezPzzeB8R1BDe8IQfcsbNO1xAgZu",
    "DD3GxxOFgx8HqJVggNP0b4DYiZ1Ucu2aQBjJQi11KZQWudjEIJAZgz1AfT9D64e1AMMYCc4J1DP0FHGbfkuF25sMC8WLRhLP",
    "jba8QtXwknInbWZUI3Hd7kfrnT8/vIiBpY7+QIZj1yUGkZ7jenZ5+35ZPpeP5X35uvt5hZTYBSji022eStm9pjLn0piG56UV",
    "vMqs5pXUK6DWGbriBk9xxXPlp5Qe0i8="
);

/// Файл копии версии 20.
const SAMPLE_SEALED_V20: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":20,"app_version":"0.0.121","created":1760270400000,"#,
    r#""kdf":{"name":"PBKDF2-SHA256","iterations":600000,"salt":"AAECAwQFBgcICQoLDA0ODw=="},"#,
    r#""cipher":{"name":"AES-256-GCM","iv":"ZGVmZ2hpamtsbW5v"},"#,
    r#""data":"yMnKy8zNzs/Q0dLT1NXW19jZ2ts="}"#
);

/// Открытый текст копии версии 21 — так её писала 0.0.126.
const SAMPLE_PLAIN_V21: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":21,"app_version":"0.0.126","created":1760270400000,"#,
    r#""records":["#,
    r#"{"kind":"note","record":{"meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","#,
    r#""modified":1760000000000,"deleted":null},"text":"Ереван ֏"}},"#,
    r#"{"kind":"tag","record":{"meta":{"id":"92d87e64-1ccb-46d0-93d7-917b1e773af5","#,
    r#""modified":1760000000500,"deleted":1760000000500}}}"#,
    r#"]}"#
);

/// Тот же текст, сжатый модулем `zlib` Python (2026-11-14).
const SAMPLE_PACKED_V21: &str = concat!(
    "dY5NasMwEIWvUmYtBUn+UeyrlFLG0qgI/6IooWAEvUFXvUcpdNk7uJuep3YhKVnkbQbeezPzzeB8R1BDe8IQfcsbNO1xAgZuDD3G",
    "xxOFgx8HqJVkgNP0b4DYiZ1U5do1gTCShVrqUigtcrGJQSAzBnuA+n6G1g9rAYYxEpwTqGfoKeI2/ZYKtzcZFooXjSSeG215harh",
    "JeVO2syoRuK63Y/WO39+eBEDSx39gQzHrksMIj3H9ezy9v2yfC4fy/vydffzCimxC1DEp9s8lbJ7TWXOpTENz0sreJVZzSupV0Ct",
    "M3TFDZ7iiufKTyk9pF8=",
);

/// Файл копии версии 21.
const SAMPLE_SEALED_V21: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":21,"app_version":"0.0.126","created":1760270400000,"#,
    r#""kdf":{"name":"PBKDF2-SHA256","iterations":600000,"salt":"AAECAwQFBgcICQoLDA0ODw=="},"#,
    r#""cipher":{"name":"AES-256-GCM","iv":"ZGVmZ2hpamtsbW5v"},"#,
    r#""data":"yMnKy8zNzs/Q0dLT1NXW19jZ2ts="}"#
);

/// Открытый текст копии версии 22 — так её писала 0.0.130.
const SAMPLE_PLAIN_V22: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":22,"app_version":"0.0.130","created":1760270400000,"#,
    r#""records":["#,
    r#"{"kind":"note","record":{"meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","#,
    r#""modified":1760000000000,"deleted":null},"text":"Ереван ֏"}},"#,
    r#"{"kind":"tag","record":{"meta":{"id":"92d87e64-1ccb-46d0-93d7-917b1e773af5","#,
    r#""modified":1760000000500,"deleted":1760000000500}}}"#,
    r#"]}"#
);

/// Тот же текст, сжатый модулем `zlib` Python (2026-11-15).
const SAMPLE_PACKED_V22: &str = concat!(
    "dY5NasMwEIWvUmYtBUn+UeyrlFLG0qgI/6IooWAEvUFXvUcpdNk7uJuep3YhKVnkbQbeezPzzeB8R1BDe8IQfcsbNO1xAgZuDD3G",
    "xxOFgx8HqJVigNP0b4DYiZ3MxNo1gTCShVrqUigtcrGJQSAzBnuA+n6G1g9rAYYxEpwTqGfoKeI2/ZYKtzcZFooXjSSeG215harh",
    "JeVO2syoRuK63Y/WO39+eBEDSx39gQzHrksMIj3H9ezy9v2yfC4fy/vydffzCimxC1DEp9s8lbJ7TWXOpTENz0sreJVZzSupV0Ct",
    "M3TFDZ7iiufKTyk9pF8=",
);

/// Файл копии версии 22.
const SAMPLE_SEALED_V22: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":22,"app_version":"0.0.130","created":1760270400000,"#,
    r#""kdf":{"name":"PBKDF2-SHA256","iterations":600000,"salt":"AAECAwQFBgcICQoLDA0ODw=="},"#,
    r#""cipher":{"name":"AES-256-GCM","iv":"ZGVmZ2hpamtsbW5v"},"#,
    r#""data":"yMnKy8zNzs/Q0dLT1NXW19jZ2ts="}"#
);

/// Открытый текст копии версии 23 — так её писала 0.0.134.
const SAMPLE_PLAIN_V23: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":23,"app_version":"0.0.134","created":1760270400000,"#,
    r#""records":["#,
    r#"{"kind":"note","record":{"meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","#,
    r#""modified":1760000000000,"deleted":null},"text":"Ереван ֏"}},"#,
    r#"{"kind":"tag","record":{"meta":{"id":"92d87e64-1ccb-46d0-93d7-917b1e773af5","#,
    r#""modified":1760000000500,"deleted":1760000000500}}}"#,
    r#"]}"#
);

/// Тот же текст, сжатый модулем `zlib` Python (2026-11-16).
const SAMPLE_PACKED_V23: &str = concat!(
    "dY5NasMwEIWvUmYtBf3Zin2VUoosjYrwL4oSCkbQG3TVe5RCl72Du+l5aheSkkXeZuC9NzPfDD50CDW0JxNTaGljbHucgIAfY2/S",
    "4wnjIYwD1EISMNP0bwDbsR2Xau3aiCahg5rrkgnNFNtEIKIdoztAfT9DG4a1AMOYEM4J1DP0mMw2w5Yyv7fSFIIWDUeqrHa0MqKh",
    "JSrPnbSi4Wbd7kcXfDg/vIiAww7/QIZj12UCCZ/TenZ5+35ZPpeP5X35uvt5hZzJBSiZp9s8lXB7jaWi3NqGqtIxWkmnacX1Cqi1",
    "NL64wVNc8Vz5OeeH/As=",
);

/// Файл копии версии 23.
const SAMPLE_SEALED_V23: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":23,"app_version":"0.0.134","created":1760270400000,"#,
    r#""kdf":{"name":"PBKDF2-SHA256","iterations":600000,"salt":"AAECAwQFBgcICQoLDA0ODw=="},"#,
    r#""cipher":{"name":"AES-256-GCM","iv":"ZGVmZ2hpamtsbW5v"},"#,
    r#""data":"yMnKy8zNzs/Q0dLT1NXW19jZ2ts="}"#
);

/// Открытый текст копии версии 24 — так её писала 0.0.141.
const SAMPLE_PLAIN_V24: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":24,"app_version":"0.0.141","created":1760270400000,"#,
    r#""records":["#,
    r#"{"kind":"note","record":{"meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","#,
    r#""modified":1760000000000,"deleted":null},"text":"Ереван ֏"}},"#,
    r#"{"kind":"tag","record":{"meta":{"id":"92d87e64-1ccb-46d0-93d7-917b1e773af5","#,
    r#""modified":1760000000500,"deleted":1760000000500}}}"#,
    r#"]}"#
);

/// Тот же текст, сжатый модулем `zlib` Python (2026-11-17).
const SAMPLE_PACKED_V24: &str = concat!(
    "dY5NasMwEIWvUmYtBUmWrdhXKaXI0qgI/8goSigYQW/QVe9RCl32Du6m56ldSEoWeZuB997MfDM43yM00J10TL6jrTbdcQICLsRB",
    "p8cTxoMPIzRCEtDT9G8A27Edl3ztmog6oYWGq4oJxSTbRCCiCdEeoLmfofPjWoAxJIRzAs0MAya9Tb+lzO1NoUtBy5YjlUZZWmvR",
    "0gql47YwouV63R6C9c6fH15EwGKPfyDjse8zgYTPaT27vH2/LJ/Lx/K+fN39vELO5AKU9NNtnlrYvcJKUm5MS2VlGa0Lq2jN1Qqo",
    "VKFdeYOnvOK58nPOD/kX",
);

/// Файл копии версии 24.
const SAMPLE_SEALED_V24: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":24,"app_version":"0.0.141","created":1760270400000,"#,
    r#""kdf":{"name":"PBKDF2-SHA256","iterations":600000,"salt":"AAECAwQFBgcICQoLDA0ODw=="},"#,
    r#""cipher":{"name":"AES-256-GCM","iv":"ZGVmZ2hpamtsbW5v"},"#,
    r#""data":"yMnKy8zNzs/Q0dLT1NXW19jZ2ts="}"#
);

/// Открытый текст копии версии 25 — так её писала 0.0.148.
const SAMPLE_PLAIN_V25: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":25,"app_version":"0.0.148","created":1760270400000,"#,
    r#""records":["#,
    r#"{"kind":"note","record":{"meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","#,
    r#""modified":1760000000000,"deleted":null},"text":"Ереван ֏"}},"#,
    r#"{"kind":"tag","record":{"meta":{"id":"92d87e64-1ccb-46d0-93d7-917b1e773af5","#,
    r#""modified":1760000000500,"deleted":1760000000500}}}"#,
    r#"]}"#
);

/// Тот же текст, сжатый модулем `zlib` Python (2026-11-17).
const SAMPLE_PACKED_V25: &str = concat!(
    "dY5NasMwEIWvUmYtBUmWLdtXKSXI0qgI/6IoIWAEvUFXvUcpdNk7uJuep3YhKVnkbQbeezPzzeB8h1BDe9Ih+pY22rTHCQi4MfQ6",
    "7k8YDn4coBY5AT1N/wawHdtxWa5dE1BHtFBzVTChmGSbCAQ0Y7AHqB9naP2wFmAYI8IlgXqGHqPept9S5kqT6VzQvOFIpVGWVlo0",
    "tEDpuM2MaLhet/vReucvD68iYLHDP5Dh2HWJQMRzXM8ub98vy+fysbwvXw8/r5ASuQJF/XyfpxK2VFhIyo1pqCwso1VmFa24WgGV",
    "yrTL7/DkNzw3fkrpKf0C",
);


/// Файл копии версии 25.
const SAMPLE_SEALED_V25: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":25,"app_version":"0.0.148","created":1760270400000,"#,
    r#""kdf":{"name":"PBKDF2-SHA256","iterations":600000,"salt":"AAECAwQFBgcICQoLDA0ODw=="},"#,
    r#""cipher":{"name":"AES-256-GCM","iv":"ZGVmZ2hpamtsbW5v"},"#,
    r#""data":"yMnKy8zNzs/Q0dLT1NXW19jZ2ts="}"#
);

/// Открытый текст копии версии 26 — так её писала 0.0.171.
const SAMPLE_PLAIN_V26: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":26,"app_version":"0.0.171","created":1760270400000,"#,
    r#""records":["#,
    r#"{"kind":"note","record":{"meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","#,
    r#""modified":1760000000000,"deleted":null},"text":"Ереван ֏"}},"#,
    r#"{"kind":"tag","record":{"meta":{"id":"92d87e64-1ccb-46d0-93d7-917b1e773af5","#,
    r#""modified":1760000000500,"deleted":1760000000500}}}"#,
    r#"]}"#
);

/// Тот же текст, сжатый модулем `zlib` Python (2026-11-22).
const SAMPLE_PACKED_V26: &str = concat!(
    "dY5NasMwEIWvUmYtBUn+UeyrlFLG0qgI/6IooWAEvUFXvUcpdNk7uJuep3YhKVnkbQbeezPzzeB8R1BDe8IQfcsbNO1xAgZuDD3G",
    "xxOFgx8HqFXJAKfp3wCxEzup5do1gTCShVrqUigtcrGJQSAzBnuA+n6G1g9rAYYxEpwTqGfoKeI2/ZYKtzcZFooXjSSeG215harh",
    "JeVO2syoRuK63Y/WO39+eBEDSx39gQzHrksMIj3H9ezy9v2yfC4fy/vydffzCimxC1DEp9s8lbJ7TWXOpTENz0sreJVZzSupV0Ct",
    "M3TFDZ7iiufKTyk9pF8=",
);

/// Файл копии версии 26.
const SAMPLE_SEALED_V26: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":26,"app_version":"0.0.171","created":1760270400000,"#,
    r#""kdf":{"name":"PBKDF2-SHA256","iterations":600000,"salt":"AAECAwQFBgcICQoLDA0ODw=="},"#,
    r#""cipher":{"name":"AES-256-GCM","iv":"ZGVmZ2hpamtsbW5v"},"#,
    r#""data":"yMnKy8zNzs/Q0dLT1NXW19jZ2ts="}"#
);

/// Открытый текст копии текущей версии 27 — так её пишет эта программа,
/// до сжатия. Выписан человеком: от версии 26 отличается номером версии
/// и версией программы в шапке — копия одним файлом части не несёт.
const SAMPLE_PLAIN_V27: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":27,"app_version":"0.0.181","created":1760270400000,"#,
    r#""records":["#,
    r#"{"kind":"note","record":{"meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","#,
    r#""modified":1760000000000,"deleted":null},"text":"Ереван ֏"}},"#,
    r#"{"kind":"tag","record":{"meta":{"id":"92d87e64-1ccb-46d0-93d7-917b1e773af5","#,
    r#""modified":1760000000500,"deleted":1760000000500}}}"#,
    r#"]}"#
);

/// Тот же текст, сжатый, как образцы версий 6–26, модулем `zlib` Python
/// (`compressobj(9, DEFLATED, -15)`), а не этим кодом (2026-11-23).
const SAMPLE_PACKED_V27: &str = concat!(
    "dY5NasMwEIWvUmYtBUn+ke2rlFLG0qgIO7ZRlFAwgt6gq96jFLrsHdxNz1O7kJQs8jYD772Z+WZwvidooDthiL7jLZruOAEDN4Y9",
    "xscThYMfB2iUZoDT9G+A2ImdrOTaNYEwkoVG6lIoLXKxiUEgMwZ7gOZ+hs4PawGGMRKcE2hm2FPEbfotFa4yGRaKF60knhtteY2q",
    "5SXlTtrMqFbiur0frXf+/PAiBpZ6+gMZjn2fGER6juvZ5e37ZflcPpb35evu5xVSYhegiE+3eWplK01lzqUxLc9LK3idWc1rqVdA",
    "rTN0xQ2e4ornyk8pPaRf",
);

/// Файл копии версии 27 — с теми же солью, вектором и «шифротекстом», что
/// у образцов прошлых версий.
const SAMPLE_SEALED_V27: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":27,"app_version":"0.0.181","created":1760270400000,"#,
    r#""kdf":{"name":"PBKDF2-SHA256","iterations":600000,"salt":"AAECAwQFBgcICQoLDA0ODw=="},"#,
    r#""cipher":{"name":"AES-256-GCM","iv":"ZGVmZ2hpamtsbW5v"},"#,
    r#""data":"yMnKy8zNzs/Q0dLT1NXW19jZ2ts="}"#
);

/// Шапка копии, которую пишет эта программа.
fn sample_header() -> Header {
    Header::new("0.0.181", at(1_760_270_400_000))
}

/// Шапка образца версии 26: так её писала 0.0.171.
fn header_v26() -> Header {
    Header {
        format_version: 26,
        app_version: "0.0.171".to_owned(),
        created: at(1_760_270_400_000),
        part: None,
    }
}

/// Шапка образца версии 25: так её писала 0.0.148.
fn header_v25() -> Header {
    Header {
        format_version: 25,
        app_version: "0.0.148".to_owned(),
        created: at(1_760_270_400_000),
        part: None,
    }
}

/// Шапка образца версии 24: так её писала 0.0.141.
fn header_v24() -> Header {
    Header {
        format_version: 24,
        app_version: "0.0.141".to_owned(),
        created: at(1_760_270_400_000),
        part: None,
    }
}

/// Шапка образца версии 23: так её писала 0.0.134.
fn header_v23() -> Header {
    Header {
        format_version: 23,
        app_version: "0.0.134".to_owned(),
        created: at(1_760_270_400_000),
        part: None,
    }
}

/// Шапка образца версии 22: так её писала 0.0.130.
fn header_v22() -> Header {
    Header {
        format_version: 22,
        app_version: "0.0.130".to_owned(),
        created: at(1_760_270_400_000),
        part: None,
    }
}

/// Шапка образца версии 21: так её писала 0.0.126.
fn header_v21() -> Header {
    Header {
        format_version: 21,
        app_version: "0.0.126".to_owned(),
        created: at(1_760_270_400_000),
        part: None,
    }
}

/// Шапка образца версии 20: так её писала 0.0.121.
fn header_v20() -> Header {
    Header {
        format_version: 20,
        app_version: "0.0.121".to_owned(),
        created: at(1_760_270_400_000),
        part: None,
    }
}

/// Шапка образца версии 19: так её писала 0.0.120.
fn header_v19() -> Header {
    Header {
        format_version: 19,
        app_version: "0.0.120".to_owned(),
        created: at(1_760_270_400_000),
        part: None,
    }
}

/// Шапка образца версии 18: так её писала 0.0.119.
fn header_v18() -> Header {
    Header {
        format_version: 18,
        app_version: "0.0.119".to_owned(),
        created: at(1_760_270_400_000),
        part: None,
    }
}

/// Шапка образца версии 17: так её писали 0.0.103–0.0.118.
fn header_v17() -> Header {
    Header {
        format_version: 17,
        app_version: "0.0.103".to_owned(),
        created: at(1_760_270_400_000),
        part: None,
    }
}

/// Шапка образца версии 16: так её писала 0.0.102.
fn header_v16() -> Header {
    Header {
        format_version: 16,
        app_version: "0.0.102".to_owned(),
        created: at(1_760_270_400_000),
        part: None,
    }
}

/// Шапка образца версии 15: так её писали 0.0.87–0.0.101.
fn header_v15() -> Header {
    Header {
        format_version: 15,
        app_version: "0.0.87".to_owned(),
        created: at(1_760_270_400_000),
        part: None,
    }
}

/// Шапка образца версии 14: так её писали 0.0.69–0.0.86.
fn header_v14() -> Header {
    Header {
        format_version: 14,
        app_version: "0.0.69".to_owned(),
        created: at(1_760_270_400_000),
        part: None,
    }
}

/// Шапка образца версии 13: так её писала 0.0.68.
fn header_v13() -> Header {
    Header {
        format_version: 13,
        app_version: "0.0.68".to_owned(),
        created: at(1_760_270_400_000),
        part: None,
    }
}

/// Шапка образца версии 12: так её писала 0.0.66.
fn header_v12() -> Header {
    Header {
        format_version: 12,
        app_version: "0.0.66".to_owned(),
        created: at(1_760_270_400_000),
        part: None,
    }
}

/// Шапка образца версии 11: так её писала 0.0.60.
fn header_v11() -> Header {
    Header {
        format_version: 11,
        app_version: "0.0.60".to_owned(),
        created: at(1_760_270_400_000),
        part: None,
    }
}

/// Шапка образца версии 10: так её писала 0.0.55.
fn header_v10() -> Header {
    Header {
        format_version: 10,
        app_version: "0.0.55".to_owned(),
        created: at(1_760_270_400_000),
        part: None,
    }
}

/// Шапка образца версии 9: так её писала 0.0.49.
fn header_v9() -> Header {
    Header {
        format_version: 9,
        app_version: "0.0.49".to_owned(),
        created: at(1_760_270_400_000),
        part: None,
    }
}

/// Шапка образца версии 8: так её писала 0.0.47.
fn header_v8() -> Header {
    Header {
        format_version: 8,
        app_version: "0.0.47".to_owned(),
        created: at(1_760_270_400_000),
        part: None,
    }
}

/// Шапка образца версии 7: так её писала 0.0.40.
fn header_v7() -> Header {
    Header {
        format_version: 7,
        app_version: "0.0.40".to_owned(),
        created: at(1_760_270_400_000),
        part: None,
    }
}

/// Шапка образца версии 6: так её писала 0.0.27.
fn header_v6() -> Header {
    Header {
        format_version: 6,
        app_version: "0.0.27".to_owned(),
        created: at(1_760_270_400_000),
        part: None,
    }
}

/// Шапка образца версии 5: так её писала 0.0.17.
fn header_v5() -> Header {
    Header {
        format_version: 5,
        app_version: "0.0.17".to_owned(),
        created: at(1_760_270_400_000),
        part: None,
    }
}

/// Шапка образца версии 4: так её писала 0.0.15.
fn header_v4() -> Header {
    Header {
        format_version: 4,
        app_version: "0.0.15".to_owned(),
        created: at(1_760_270_400_000),
        part: None,
    }
}

/// Шапка образца версии 3: так её писала 0.0.14.
fn header_v3() -> Header {
    Header {
        format_version: 3,
        app_version: "0.0.14".to_owned(),
        created: at(1_760_270_400_000),
        part: None,
    }
}

/// Шапка образца версии 2: так её писала 0.0.12.
fn header_v2() -> Header {
    Header {
        format_version: 2,
        app_version: "0.0.12".to_owned(),
        created: at(1_760_270_400_000),
        part: None,
    }
}

/// Шапка образца версии 1: так её писала 0.0.8.
fn header_v1() -> Header {
    Header {
        format_version: 1,
        app_version: "0.0.8".to_owned(),
        created: at(1_760_270_400_000),
        part: None,
    }
}

fn sample_lock() -> Lock {
    Lock::new(600_000, (0..16).collect(), (100..112).collect()).expect("замок образца годится")
}

fn sealed_sample() -> Sealed {
    open(SAMPLE_SEALED_V27).expect("образец файла копии открывается")
}

fn sealed_sample_v26() -> Sealed {
    open(SAMPLE_SEALED_V26).expect("образец файла копии версии 26 открывается")
}

fn sealed_sample_v25() -> Sealed {
    open(SAMPLE_SEALED_V25).expect("образец файла копии версии 25 открывается")
}

fn sealed_sample_v24() -> Sealed {
    open(SAMPLE_SEALED_V24).expect("образец файла копии версии 24 открывается")
}

fn sealed_sample_v23() -> Sealed {
    open(SAMPLE_SEALED_V23).expect("образец файла копии версии 23 открывается")
}

fn sealed_sample_v22() -> Sealed {
    open(SAMPLE_SEALED_V22).expect("образец файла копии версии 22 открывается")
}

fn sealed_sample_v21() -> Sealed {
    open(SAMPLE_SEALED_V21).expect("образец файла копии версии 21 открывается")
}

fn sealed_sample_v20() -> Sealed {
    open(SAMPLE_SEALED_V20).expect("образец файла копии версии 20 открывается")
}

fn sealed_sample_v19() -> Sealed {
    open(SAMPLE_SEALED_V19).expect("образец файла копии версии 19 открывается")
}

fn sealed_sample_v18() -> Sealed {
    open(SAMPLE_SEALED_V18).expect("образец файла копии версии 18 открывается")
}

fn sealed_sample_v17() -> Sealed {
    open(SAMPLE_SEALED_V17).expect("образец файла копии версии 17 открывается")
}

fn sealed_sample_v16() -> Sealed {
    open(SAMPLE_SEALED_V16).expect("образец файла копии версии 16 открывается")
}

fn sealed_sample_v15() -> Sealed {
    open(SAMPLE_SEALED_V15).expect("образец файла копии версии 15 открывается")
}

fn sealed_sample_v14() -> Sealed {
    open(SAMPLE_SEALED_V14).expect("образец файла копии версии 14 открывается")
}

fn sealed_sample_v13() -> Sealed {
    open(SAMPLE_SEALED_V13).expect("образец файла копии версии 13 открывается")
}

fn sealed_sample_v12() -> Sealed {
    open(SAMPLE_SEALED_V12).expect("образец файла копии версии 12 открывается")
}

fn sealed_sample_v11() -> Sealed {
    open(SAMPLE_SEALED_V11).expect("образец файла копии версии 11 открывается")
}

fn sealed_sample_v10() -> Sealed {
    open(SAMPLE_SEALED_V10).expect("образец файла копии версии 10 открывается")
}

fn sealed_sample_v9() -> Sealed {
    open(SAMPLE_SEALED_V9).expect("образец файла копии версии 9 открывается")
}

fn sealed_sample_v8() -> Sealed {
    open(SAMPLE_SEALED_V8).expect("образец файла копии версии 8 открывается")
}

fn sealed_sample_v7() -> Sealed {
    open(SAMPLE_SEALED_V7).expect("образец файла копии версии 7 открывается")
}

fn sealed_sample_v6() -> Sealed {
    open(SAMPLE_SEALED_V6).expect("образец файла копии версии 6 открывается")
}

fn sealed_sample_v5() -> Sealed {
    open(SAMPLE_SEALED_V5).expect("образец файла копии версии 5 открывается")
}

/// Сжатый текст образца версии 27 — так его отдаёт расшифровка.
fn packed_sample_v27() -> Vec<u8> {
    base64::decode(SAMPLE_PACKED_V27).expect("образец — base64")
}

/// Сжатый текст образца версии 26 — так его отдавала расшифровка 0.0.171.
fn packed_sample_v26() -> Vec<u8> {
    base64::decode(SAMPLE_PACKED_V26).expect("образец — base64")
}

/// Сжатый текст образца версии 25 — так его отдавала расшифровка 0.0.148.
fn packed_sample_v25() -> Vec<u8> {
    base64::decode(SAMPLE_PACKED_V25).expect("образец — base64")
}

/// Сжатый текст образца версии 24 — так его отдавала расшифровка 0.0.141.
fn packed_sample_v24() -> Vec<u8> {
    base64::decode(SAMPLE_PACKED_V24).expect("образец — base64")
}

/// Сжатый текст образца версии 23 — так его отдавала расшифровка 0.0.134.
fn packed_sample_v23() -> Vec<u8> {
    base64::decode(SAMPLE_PACKED_V23).expect("образец — base64")
}

/// Сжатый текст образца версии 22 — так его отдавала расшифровка 0.0.130.
fn packed_sample_v22() -> Vec<u8> {
    base64::decode(SAMPLE_PACKED_V22).expect("образец — base64")
}

/// Сжатый текст образца версии 21 — так его отдавала расшифровка 0.0.126.
fn packed_sample_v21() -> Vec<u8> {
    base64::decode(SAMPLE_PACKED_V21).expect("образец — base64")
}

/// Сжатый текст образца версии 20 — так его отдавала расшифровка 0.0.121.
fn packed_sample_v20() -> Vec<u8> {
    base64::decode(SAMPLE_PACKED_V20).expect("образец — base64")
}

/// Сжатый текст образца версии 19 — так его отдавала расшифровка 0.0.120.
fn packed_sample_v19() -> Vec<u8> {
    base64::decode(SAMPLE_PACKED_V19).expect("образец — base64")
}

/// Сжатый текст образца версии 18 — так его отдавала расшифровка 0.0.119.
fn packed_sample_v18() -> Vec<u8> {
    base64::decode(SAMPLE_PACKED_V18).expect("образец — base64")
}

/// Сжатый текст образца версии 17 — так его отдавала расшифровка 0.0.103.
fn packed_sample_v17() -> Vec<u8> {
    base64::decode(SAMPLE_PACKED_V17).expect("образец — base64")
}

/// Сжатый текст образца версии 16 — так его отдавала расшифровка 0.0.102.
fn packed_sample_v16() -> Vec<u8> {
    base64::decode(SAMPLE_PACKED_V16).expect("образец — base64")
}

/// Сжатый текст образца версии 15 — так его отдавала расшифровка 0.0.87–0.0.101.
fn packed_sample_v15() -> Vec<u8> {
    base64::decode(SAMPLE_PACKED_V15).expect("образец — base64")
}

/// Сжатый текст образца версии 14 — так его отдавала расшифровка 0.0.69–0.0.86.
fn packed_sample_v14() -> Vec<u8> {
    base64::decode(SAMPLE_PACKED_V14).expect("образец — base64")
}

/// Сжатый текст образца версии 13 — так его отдавала расшифровка 0.0.68.
fn packed_sample_v13() -> Vec<u8> {
    base64::decode(SAMPLE_PACKED_V13).expect("образец — base64")
}

/// Сжатый текст образца версии 12 — так его отдавала расшифровка 0.0.66.
fn packed_sample_v12() -> Vec<u8> {
    base64::decode(SAMPLE_PACKED_V12).expect("образец — base64")
}

/// Сжатый текст образца версии 11 — так его отдавала расшифровка 0.0.60.
fn packed_sample_v11() -> Vec<u8> {
    base64::decode(SAMPLE_PACKED_V11).expect("образец — base64")
}

/// Сжатый текст образца версии 10 — так его отдавала расшифровка 0.0.55.
fn packed_sample_v10() -> Vec<u8> {
    base64::decode(SAMPLE_PACKED_V10).expect("образец — base64")
}

/// Сжатый текст образца версии 9 — так его отдавала расшифровка 0.0.49.
fn packed_sample_v9() -> Vec<u8> {
    base64::decode(SAMPLE_PACKED_V9).expect("образец — base64")
}

/// Сжатый текст образца версии 8 — так его отдавала расшифровка 0.0.47.
fn packed_sample_v8() -> Vec<u8> {
    base64::decode(SAMPLE_PACKED_V8).expect("образец — base64")
}

/// Сжатый текст образца версии 7 — так его отдавала расшифровка 0.0.40.
fn packed_sample_v7() -> Vec<u8> {
    base64::decode(SAMPLE_PACKED_V7).expect("образец — base64")
}

/// Сжатый текст образца версии 6 — так его отдавала расшифровка 0.0.27.
fn packed_sample_v6() -> Vec<u8> {
    base64::decode(SAMPLE_PACKED_V6).expect("образец — base64")
}

/// Текст, сжатый, как его сжимает [`pack`]: так его отдаёт расшифровка
/// копии версий 6–14.
fn packed(text: &str) -> Vec<u8> {
    miniz_oxide::deflate::compress_to_vec(text.as_bytes(), PACK_LEVEL)
}

fn sealed_sample_v1() -> Sealed {
    open(SAMPLE_SEALED_V1).expect("образец файла копии версии 1 открывается")
}

fn sealed_sample_v2() -> Sealed {
    open(SAMPLE_SEALED_V2).expect("образец файла копии версии 2 открывается")
}

fn sealed_sample_v3() -> Sealed {
    open(SAMPLE_SEALED_V3).expect("образец файла копии версии 3 открывается")
}

fn sealed_sample_v4() -> Sealed {
    open(SAMPLE_SEALED_V4).expect("образец файла копии версии 4 открывается")
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
fn sealed_sample_v27_is_pinned() {
    let sealed = sealed_sample();
    assert_eq!(sealed.header(), &sample_header());
    assert_eq!(sealed.header().format_version(), 27);
    assert_eq!(sealed.header().app_version(), "0.0.181");
    assert_eq!(sealed.header().created(), at(1_760_270_400_000));
    assert_eq!(sealed.lock(), &sample_lock());
    assert_eq!(sealed.ciphertext(), (200..220).collect::<Vec<u8>>());
    assert_eq!(
        seal(
            &sample_header(),
            &sample_lock(),
            &(200..220).collect::<Vec<u8>>()
        ),
        SAMPLE_SEALED_V27
    );
}

/// Файлы версий 1–22 открываются этой программой с той же шапкой
/// и замком.
#[test]
fn older_sealed_samples_still_open() {
    for (sealed, header) in [
        (sealed_sample_v1(), header_v1()),
        (sealed_sample_v2(), header_v2()),
        (sealed_sample_v3(), header_v3()),
        (sealed_sample_v4(), header_v4()),
        (sealed_sample_v5(), header_v5()),
        (sealed_sample_v6(), header_v6()),
        (sealed_sample_v7(), header_v7()),
        (sealed_sample_v8(), header_v8()),
        (sealed_sample_v9(), header_v9()),
        (sealed_sample_v10(), header_v10()),
        (sealed_sample_v11(), header_v11()),
        (sealed_sample_v12(), header_v12()),
        (sealed_sample_v13(), header_v13()),
        (sealed_sample_v14(), header_v14()),
        (sealed_sample_v15(), header_v15()),
        (sealed_sample_v16(), header_v16()),
        (sealed_sample_v17(), header_v17()),
        (sealed_sample_v18(), header_v18()),
        (sealed_sample_v19(), header_v19()),
        (sealed_sample_v20(), header_v20()),
        (sealed_sample_v21(), header_v21()),
        (sealed_sample_v22(), header_v22()),
        (sealed_sample_v23(), header_v23()),
        (sealed_sample_v24(), header_v24()),
        (sealed_sample_v25(), header_v25()),
        (sealed_sample_v26(), header_v26()),
    ] {
        assert_eq!(sealed.header(), &header);
        assert_eq!(sealed.lock(), &sample_lock());
        assert_eq!(sealed.ciphertext(), (200..220).collect::<Vec<u8>>());
    }
}

/// Сжатый текст текущей версии читается в записи с ожидаемыми полями,
/// а запись тех же записей — слово в слово образец; сжатое [`pack`]
/// разжимается в тот же текст.
#[test]
fn plain_sample_v27_is_pinned() {
    let entries = read(KINDS_UNDER_TEST, &sealed_sample(), &packed_sample_v27())
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
    assert_eq!(plaintext(&sample_header(), &entries), SAMPLE_PLAIN_V27);
    let packed = pack(&sample_header(), &entries);
    assert_eq!(
        unpack(&packed, MAX_PLAIN_BYTES).ok().as_deref(),
        Some(SAMPLE_PLAIN_V27.as_bytes())
    );
}

/// Открытый текст версий 1–5 — несжатый — и сжатый версий 6–22 читается в те
/// же записи, и в базу они ложатся уже в текущей версии.
#[test]
fn older_plain_samples_still_read() {
    for (version, sealed, plain) in [
        (1, sealed_sample_v1(), SAMPLE_PLAIN_V1),
        (2, sealed_sample_v2(), SAMPLE_PLAIN_V2),
        (3, sealed_sample_v3(), SAMPLE_PLAIN_V3),
        (4, sealed_sample_v4(), SAMPLE_PLAIN_V4),
        (5, sealed_sample_v5(), SAMPLE_PLAIN_V5),
    ] {
        let entries = read(KINDS_UNDER_TEST, &sealed, plain.as_bytes())
            .unwrap_or_else(|error| panic!("образец версии {version}: {error:?}"));
        let stored: Vec<String> = entries.iter().map(Entry::stored).collect();
        assert_eq!(stored, [STORED_NOTE, STORED_TAG], "версия {version}");
    }
    // Версия 6 — уже сжатая: разжимается в свой открытый текст.
    assert_eq!(
        unpack(&packed_sample_v6(), MAX_PLAIN_BYTES).ok().as_deref(),
        Some(SAMPLE_PLAIN_V6.as_bytes())
    );
    let entries = read(KINDS_UNDER_TEST, &sealed_sample_v6(), &packed_sample_v6())
        .expect("образец версии 6 читается");
    let stored: Vec<String> = entries.iter().map(Entry::stored).collect();
    assert_eq!(stored, [STORED_NOTE, STORED_TAG], "версия 6");
    assert_eq!(
        unpack(&packed_sample_v7(), MAX_PLAIN_BYTES).ok().as_deref(),
        Some(SAMPLE_PLAIN_V7.as_bytes())
    );
    let entries = read(KINDS_UNDER_TEST, &sealed_sample_v7(), &packed_sample_v7())
        .expect("образец версии 7 читается");
    let stored: Vec<String> = entries.iter().map(Entry::stored).collect();
    assert_eq!(stored, [STORED_NOTE, STORED_TAG], "версия 7");
    assert_eq!(
        unpack(&packed_sample_v8(), MAX_PLAIN_BYTES).ok().as_deref(),
        Some(SAMPLE_PLAIN_V8.as_bytes())
    );
    let entries = read(KINDS_UNDER_TEST, &sealed_sample_v8(), &packed_sample_v8())
        .expect("образец версии 8 читается");
    let stored: Vec<String> = entries.iter().map(Entry::stored).collect();
    assert_eq!(stored, [STORED_NOTE, STORED_TAG], "версия 8");
    assert_eq!(
        unpack(&packed_sample_v9(), MAX_PLAIN_BYTES).ok().as_deref(),
        Some(SAMPLE_PLAIN_V9.as_bytes())
    );
    let entries = read(KINDS_UNDER_TEST, &sealed_sample_v9(), &packed_sample_v9())
        .expect("образец версии 9 читается");
    let stored: Vec<String> = entries.iter().map(Entry::stored).collect();
    assert_eq!(stored, [STORED_NOTE, STORED_TAG], "версия 9");
    assert_eq!(
        unpack(&packed_sample_v10(), MAX_PLAIN_BYTES).ok().as_deref(),
        Some(SAMPLE_PLAIN_V10.as_bytes())
    );
    let entries = read(KINDS_UNDER_TEST, &sealed_sample_v10(), &packed_sample_v10())
        .expect("образец версии 10 читается");
    let stored: Vec<String> = entries.iter().map(Entry::stored).collect();
    assert_eq!(stored, [STORED_NOTE, STORED_TAG], "версия 10");
    assert_eq!(
        unpack(&packed_sample_v11(), MAX_PLAIN_BYTES).ok().as_deref(),
        Some(SAMPLE_PLAIN_V11.as_bytes())
    );
    let entries = read(KINDS_UNDER_TEST, &sealed_sample_v11(), &packed_sample_v11())
        .expect("образец версии 11 читается");
    let stored: Vec<String> = entries.iter().map(Entry::stored).collect();
    assert_eq!(stored, [STORED_NOTE, STORED_TAG], "версия 11");
    assert_eq!(
        unpack(&packed_sample_v12(), MAX_PLAIN_BYTES).ok().as_deref(),
        Some(SAMPLE_PLAIN_V12.as_bytes())
    );
    let entries = read(KINDS_UNDER_TEST, &sealed_sample_v12(), &packed_sample_v12())
        .expect("образец версии 12 читается");
    let stored: Vec<String> = entries.iter().map(Entry::stored).collect();
    assert_eq!(stored, [STORED_NOTE, STORED_TAG], "версия 12");
    assert_eq!(
        unpack(&packed_sample_v13(), MAX_PLAIN_BYTES).ok().as_deref(),
        Some(SAMPLE_PLAIN_V13.as_bytes())
    );
    let entries = read(KINDS_UNDER_TEST, &sealed_sample_v13(), &packed_sample_v13())
        .expect("образец версии 13 читается");
    let stored: Vec<String> = entries.iter().map(Entry::stored).collect();
    assert_eq!(stored, [STORED_NOTE, STORED_TAG], "версия 13");
    assert_eq!(
        unpack(&packed_sample_v14(), MAX_PLAIN_BYTES).ok().as_deref(),
        Some(SAMPLE_PLAIN_V14.as_bytes())
    );
    let entries = read(KINDS_UNDER_TEST, &sealed_sample_v14(), &packed_sample_v14())
        .expect("образец версии 14 читается");
    let stored: Vec<String> = entries.iter().map(Entry::stored).collect();
    assert_eq!(stored, [STORED_NOTE, STORED_TAG], "версия 14");
    assert_eq!(
        unpack(&packed_sample_v15(), MAX_PLAIN_BYTES).ok().as_deref(),
        Some(SAMPLE_PLAIN_V15.as_bytes())
    );
    let entries = read(KINDS_UNDER_TEST, &sealed_sample_v15(), &packed_sample_v15())
        .expect("образец версии 15 читается");
    let stored: Vec<String> = entries.iter().map(Entry::stored).collect();
    assert_eq!(stored, [STORED_NOTE, STORED_TAG], "версия 15");
    assert_eq!(
        unpack(&packed_sample_v16(), MAX_PLAIN_BYTES).ok().as_deref(),
        Some(SAMPLE_PLAIN_V16.as_bytes())
    );
    let entries = read(KINDS_UNDER_TEST, &sealed_sample_v16(), &packed_sample_v16())
        .expect("образец версии 16 читается");
    let stored: Vec<String> = entries.iter().map(Entry::stored).collect();
    assert_eq!(stored, [STORED_NOTE, STORED_TAG], "версия 16");
    assert_eq!(
        unpack(&packed_sample_v17(), MAX_PLAIN_BYTES).ok().as_deref(),
        Some(SAMPLE_PLAIN_V17.as_bytes())
    );
    let entries = read(KINDS_UNDER_TEST, &sealed_sample_v17(), &packed_sample_v17())
        .expect("образец версии 17 читается");
    let stored: Vec<String> = entries.iter().map(Entry::stored).collect();
    assert_eq!(stored, [STORED_NOTE, STORED_TAG], "версия 17");
    assert_eq!(
        unpack(&packed_sample_v18(), MAX_PLAIN_BYTES).ok().as_deref(),
        Some(SAMPLE_PLAIN_V18.as_bytes())
    );
    let entries = read(KINDS_UNDER_TEST, &sealed_sample_v18(), &packed_sample_v18())
        .expect("образец версии 18 читается");
    let stored: Vec<String> = entries.iter().map(Entry::stored).collect();
    assert_eq!(stored, [STORED_NOTE, STORED_TAG], "версия 18");
    assert_eq!(
        unpack(&packed_sample_v19(), MAX_PLAIN_BYTES).ok().as_deref(),
        Some(SAMPLE_PLAIN_V19.as_bytes())
    );
    let entries = read(KINDS_UNDER_TEST, &sealed_sample_v19(), &packed_sample_v19())
        .expect("образец версии 19 читается");
    let stored: Vec<String> = entries.iter().map(Entry::stored).collect();
    assert_eq!(stored, [STORED_NOTE, STORED_TAG], "версия 19");
    assert_eq!(
        unpack(&packed_sample_v20(), MAX_PLAIN_BYTES).ok().as_deref(),
        Some(SAMPLE_PLAIN_V20.as_bytes())
    );
    let entries = read(KINDS_UNDER_TEST, &sealed_sample_v20(), &packed_sample_v20())
        .expect("образец версии 20 читается");
    let stored: Vec<String> = entries.iter().map(Entry::stored).collect();
    assert_eq!(stored, [STORED_NOTE, STORED_TAG], "версия 20");
    assert_eq!(
        unpack(&packed_sample_v21(), MAX_PLAIN_BYTES).ok().as_deref(),
        Some(SAMPLE_PLAIN_V21.as_bytes())
    );
    let entries = read(KINDS_UNDER_TEST, &sealed_sample_v21(), &packed_sample_v21())
        .expect("образец версии 21 читается");
    let stored: Vec<String> = entries.iter().map(Entry::stored).collect();
    assert_eq!(stored, [STORED_NOTE, STORED_TAG], "версия 21");
    assert_eq!(
        unpack(&packed_sample_v22(), MAX_PLAIN_BYTES).ok().as_deref(),
        Some(SAMPLE_PLAIN_V22.as_bytes())
    );
    let entries = read(KINDS_UNDER_TEST, &sealed_sample_v22(), &packed_sample_v22())
        .expect("образец версии 22 читается");
    let stored: Vec<String> = entries.iter().map(Entry::stored).collect();
    assert_eq!(stored, [STORED_NOTE, STORED_TAG], "версия 22");
    assert_eq!(
        unpack(&packed_sample_v23(), MAX_PLAIN_BYTES).ok().as_deref(),
        Some(SAMPLE_PLAIN_V23.as_bytes())
    );
    let entries = read(KINDS_UNDER_TEST, &sealed_sample_v23(), &packed_sample_v23())
        .expect("образец версии 23 читается");
    let stored: Vec<String> = entries.iter().map(Entry::stored).collect();
    assert_eq!(stored, [STORED_NOTE, STORED_TAG], "версия 23");
    assert_eq!(
        unpack(&packed_sample_v24(), MAX_PLAIN_BYTES).ok().as_deref(),
        Some(SAMPLE_PLAIN_V24.as_bytes())
    );
    let entries = read(KINDS_UNDER_TEST, &sealed_sample_v24(), &packed_sample_v24())
        .expect("образец версии 24 читается");
    let stored: Vec<String> = entries.iter().map(Entry::stored).collect();
    assert_eq!(stored, [STORED_NOTE, STORED_TAG], "версия 24");
    assert_eq!(
        unpack(&packed_sample_v25(), MAX_PLAIN_BYTES).ok().as_deref(),
        Some(SAMPLE_PLAIN_V25.as_bytes())
    );
    let entries = read(KINDS_UNDER_TEST, &sealed_sample_v25(), &packed_sample_v25())
        .expect("образец версии 25 читается");
    let stored: Vec<String> = entries.iter().map(Entry::stored).collect();
    assert_eq!(stored, [STORED_NOTE, STORED_TAG], "версия 25");
    assert_eq!(
        unpack(&packed_sample_v26(), MAX_PLAIN_BYTES).ok().as_deref(),
        Some(SAMPLE_PLAIN_V26.as_bytes())
    );
    let entries = read(KINDS_UNDER_TEST, &sealed_sample_v26(), &packed_sample_v26())
        .expect("образец версии 26 читается");
    let stored: Vec<String> = entries.iter().map(Entry::stored).collect();
    assert_eq!(stored, [STORED_NOTE, STORED_TAG], "версия 26");
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
        SAMPLE_PLAIN_V27
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
    // Шифра в ядре нет: «шифротекст» здесь — сам сжатый текст.
    let file = seal(&header, &sample_lock(), &pack(&header, collected.entries()));
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
        r#"{"file":"kvartik-backup","format_version":27,"app_version":"0.0.181","created":1760270400000,"records":[]}"#
    );
    let packed = pack(&sample_header(), collected.entries());
    let entries = read(&[], &sealed_sample(), &packed).expect("пустая копия читается");
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
        SAMPLE_SEALED_V27.replace(
            r#""format_version":27"#,
            &format!(r#""format_version":{next}"#),
        ),
    ] {
        expect(&text, &newer);
    }

    let damaged = |got: &Result<Sealed, BackupError>| matches!(got, Err(BackupError::Damaged(_)));
    let mut cases = vec![
        // Метка своя, а версии нет — своя копия, но испорченная.
        r#"{"file":"kvartik-backup"}"#.to_owned(),
        SAMPLE_SEALED_V27[..SAMPLE_SEALED_V27.len() / 2].to_owned(),
        SAMPLE_SEALED_V27[..SAMPLE_SEALED_V27.len() - 1].to_owned(),
        SAMPLE_SEALED_V1[..SAMPLE_SEALED_V1.len() / 2].to_owned(),
        format!("{SAMPLE_SEALED_V27} хвост"),
        SAMPLE_SEALED_V27.replace(r#""format_version":27"#, r#""format_version":0"#),
        SAMPLE_SEALED_V27.replace(r#""format_version":27"#, r#""format_version":"2""#),
        SAMPLE_SEALED_V27.replace(r#""data""#, r#""extra":1,"data""#),
        SAMPLE_SEALED_V27.replace("PBKDF2-SHA256", "PBKDF2-SHA1"),
        SAMPLE_SEALED_V27.replace("AES-256-GCM", "AES-256-CBC"),
        SAMPLE_SEALED_V27.replace("600000", "0"),
        SAMPLE_SEALED_V27.replace("600000", "10000001"),
        SAMPLE_SEALED_V27.replace("AAECAwQFBgcICQoLDA0ODw==", "AAECAwQFBgcICQoLDA0O"),
        SAMPLE_SEALED_V27.replace("AAECAwQFBgcICQoLDA0ODw==", "не base64"),
        SAMPLE_SEALED_V27.replace("ZGVmZ2hpamtsbW5v", "ZGVmZ2hpamtsbW5vcA=="),
        SAMPLE_SEALED_V27.replace("yMnKy8zNzs/Q0dLT1NXW19jZ2ts=", "yMnKy8zNzs/Q0dLT1NXW"),
        SAMPLE_SEALED_V27.replace("yMnKy8zNzs/Q0dLT1NXW19jZ2ts=", "yMnK\ny8zN"),
        SAMPLE_SEALED_V27.replace(r#""0.0.181""#, r#""<b>0.0.12</b>""#),
        SAMPLE_SEALED_V27.replace(r#""0.0.181""#, r#""""#),
        SAMPLE_SEALED_V27.replace("1760270400000", "8640000000000001"),
        SAMPLE_SEALED_V27.replace(
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
    let second_note = SAMPLE_PLAIN_V27.replace(
        r#"]}"#,
        &format!(
            r#",{{"kind":"note","record":{{"meta":{{"id":"{NOTE_ID}","modified":1,"deleted":null}},"text":""}}}}]}}"#
        ),
    );
    let mut cases = vec![
        SAMPLE_PLAIN_V27.replace(r#""0.0.181""#, r#""0.0.26""#),
        SAMPLE_PLAIN_V27.replace("1760270400000", "1760270400001"),
        SAMPLE_PLAIN_V27.replace(r#""file":"kvartik-backup""#, r#""file":"other""#),
        SAMPLE_PLAIN_V27.replace(r#""format_version":27"#, r#""format_version":1"#),
        // Шапка внутри — версии 1, а снаружи — текущей: подмена версии.
        SAMPLE_PLAIN_V1.replace(r#""0.0.8""#, r#""0.0.181""#),
        SAMPLE_PLAIN_V27.replace(r#""kind":"tag""#, r#""kind":"unicorn""#),
        SAMPLE_PLAIN_V27.replace(r#""text":"Ереван ֏""#, r#""text":7"#),
        SAMPLE_PLAIN_V27.replace(r#""text":"Ереван ֏""#, r#""text":"Ереван ֏","wings":2"#),
        SAMPLE_PLAIN_V27.replace(r#""records""#, r#""extra":1,"records""#),
        SAMPLE_PLAIN_V27.replace(r#"{"kind":"tag","#, r#"{"extra":1,"kind":"tag","#),
        SAMPLE_PLAIN_V27.replace(r#""deleted":1760000000500"#, r#""deleted":1760000000501"#),
        second_note,
        SAMPLE_PLAIN_V27[..SAMPLE_PLAIN_V27.len() - 2].to_owned(),
        "не JSON".to_owned(),
    ];
    cases.push(String::new());
    let mut wrong = Vec::new();
    for text in &cases {
        match read(KINDS_UNDER_TEST, &sealed, &packed(text)) {
            Err(BackupError::Damaged(_)) => {}
            other => wrong.push(format!("{text:.140}: {:?}", other.map(|e| e.len()))),
        }
    }
    let not_utf8 = miniz_oxide::deflate::compress_to_vec(&[0xff, 0xfe, b'{', b'}'], PACK_LEVEL);
    if !matches!(
        read(KINDS_UNDER_TEST, &sealed, &not_utf8),
        Err(BackupError::Damaged(_))
    ) {
        wrong.push("не UTF-8 прочитано".to_owned());
    }
    // Версия 6 сжимает всегда: несжатый текст под её шапкой — порча, как
    // и сжатый обрезанный.
    let unpacked = SAMPLE_PLAIN_V27.as_bytes();
    let cut = packed_sample_v12();
    for opened in [unpacked, &cut[..cut.len() / 2], &[]] {
        if !matches!(read(KINDS_UNDER_TEST, &sealed, opened), Err(BackupError::Damaged(_))) {
            wrong.push(format!("{} Б не сжатого текста прочитаны", opened.len()));
        }
    }
    assert!(wrong.is_empty(), "прочитано испорченное: {wrong:#?}");
}

/// Разжатие останавливается на пределе, а не после него (Правило 13):
/// килобайт, разжимающийся в гигабайт нулей, не закрывает страницу.
#[test]
fn unpacking_stops_at_the_limit() {
    let zeros = miniz_oxide::deflate::compress_to_vec(&[0; 10_000], PACK_LEVEL);
    assert!(zeros.len() < 100, "нули сжимаются в разы: {} Б", zeros.len());
    assert_eq!(unpack(&zeros, 10_000).map(|text| text.len()), Ok(10_000));
    assert!(matches!(unpack(&zeros, 9_999), Err(BackupError::Damaged(_))));
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
        let writes = merge.writes().count();
        if got != (added, updated, unchanged) || writes != added + updated {
            wrong.push(format!("{name}: {got:?}, записей {writes}"));
        }
    }
    assert!(wrong.is_empty(), "слияние ошиблось: {wrong:#?}");
}

/// Правка в копии позже «сейчас» больше чем на десять минут — часы одного из
/// устройств спешат (задача 41); раньше, ровно на пределе и без записей —
/// нет: окно не тревожит часами, поставленными вручную.
#[test]
fn copy_from_a_clock_ahead_is_told() {
    let now = Timestamp::from_unix_millis(1_000_000_000);
    let cases = [
        ("раньше сейчас", 999_000_000, false),
        ("на пределе", 1_000_000_000 + CLOCK_SLACK_MILLIS, false),
        ("позже предела", 1_000_000_001 + CLOCK_SLACK_MILLIS, true),
        ("на сутки вперёд", 1_086_400_000, true),
    ];
    let mut wrong = Vec::new();
    for (name, modified, told) in cases {
        if ahead(&incoming_note(NOTE_ID, modified, "копия"), now) != told {
            wrong.push(name);
        }
    }
    if ahead(&[], now) {
        wrong.push("пустая копия");
    }
    assert!(wrong.is_empty(), "часы копии: {wrong:#?}");
}

/// Черновик ушёл копией на устройство, которое выпускает, и выпущен там,
/// а на прежнем его правят или удаляют позже (задача 41): выпущенный
/// остаётся выпущенным, в какую бы сторону ни шла копия, а две версии
/// выпущенного — аннулирование после выпуска — решают моменты, как всё.
#[test]
fn an_issued_document_outlives_a_later_draft() {
    use crate::model::client::{Client, ClientData};
    use crate::model::country::Country;
    use crate::model::document::{DocKind, DocumentData, Item};
    use crate::model::money::Currency;
    use crate::model::text::Lines;

    let at = Timestamp::from_unix_millis;
    let line = |text: &str| Line::new(text).expect("годный текст");
    let client = Client::create(
        ClientData {
            name: line("«Արարատ» ՍՊԸ"),
            country: Some(Country::ARMENIA),
            address: Lines::default(),
            tin: None,
            tax_id: Line::default(),
            currency: Some(Currency::Amd),
            lang: Some(Lang::Am),
            payment_days: None,
        },
        at(500),
    )
    .expect("годный клиент");
    let mut profile = ProfileData::default();
    profile.name.hy = line("ԱՁ Արամ Պետրոսյան");
    let day = Date::new(2026, 11, 2).expect("дата");
    let data = |text: &str| DocumentData {
        kind: DocKind::Invoice,
        client: client.id(),
        lang: Lang::Am,
        currency: Currency::Amd,
        date: day,
        service_date: day,
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
    };
    let draft = Document::create(data("Խորհրդատվություն"), at(1_000)).expect("черновик");
    let issued = draft
        .issue(&[], 0, Some(&profile), Some(None), &client, at(2_000))
        .expect("выпущен");
    let edited = draft.edit(data("Խմբագրված"), at(3_000)).expect("правится").expect("изменён");
    let deleted = draft.delete(at(3_000)).expect("удаляется").expect("удалён");
    let annulled = issued.annul(day, at(4_000)).expect("выпущен").expect("аннулирован");
    let id = draft.id().to_string();
    let cases = [
        ("черновик правили позже — в базе выпущенный", &edited, &issued, false),
        ("черновик удалили позже — в базе выпущенный", &deleted, &issued, false),
        ("выпущенный — в базе черновик, правленный позже", &issued, &edited, true),
        ("аннулирован позже выпуска", &annulled, &issued, true),
        ("в базе аннулирован позже", &issued, &annulled, false),
    ];
    let mut wrong = Vec::new();
    for (name, incoming, existing, written) in cases {
        let incoming = collect(KINDS, [raw(Document::KIND, &id, &encode(incoming))])
            .expect("собрано")
            .entries;
        let stored = encode(existing);
        let merge = merge(KINDS, &incoming, [raw(Document::KIND, &id, &stored)]).expect("слито");
        if merge.writes().next().is_some() != written {
            wrong.push(name);
        }
    }
    assert!(wrong.is_empty(), "выпущенный против черновика: {wrong:#?}");
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
        merge_before.writes().next().map(|entry| entry.meta().deleted()),
        Some(Some(at(1_760_000_000_500)))
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
        merge(KINDS_UNDER_TEST, &incoming, [raw("note", NOTE_ID, &newer)]).map(|m| m.writes().count()),
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

/// Копия для клиента названа ՀՎՀՀ и днём, без ՀՎՀՀ — `ip`; у файла всех ИП
/// и у копии одного — прежнее имя.
#[test]
fn client_file_name_carries_the_tin() {
    let tin = Tin::parse("12345678").expect("годный ՀՎՀՀ");
    assert_eq!(client_file_name(Some(&tin), day(2026, 11, 6)), "kvartik-12345678-2026-11-06.json");
    assert_eq!(client_file_name(None, day(2027, 1, 5)), "kvartik-ip-2027-01-05.json");
}

/// Копию всех ИП узнают по записям конторы или по меткам базы; копия одного
/// ИП — без них.
#[test]
fn a_full_backup_is_told_apart() {
    assert!(is_full(&full_entries()));
    let single = collect(KINDS_UNDER_TEST, stored_pair()).expect("собрано").entries;
    assert!(!is_full(&single));
    let ip = RecordId::parse(IP_ID).expect("идентификатор образца");
    let tagged: Vec<Entry> = single.iter().cloned().map(|entry| entry.in_base(ip)).collect();
    assert!(is_full(&tagged), "метка базы без конторы — тоже копия всех ИП");
    // Ключ «Pro» без ИП списка и без меток — тоже запись конторы: в открытую
    // базу ИП он не ляжет.
    let tin = Tin::parse("07654321").expect("годный ՀՎՀՀ");
    let pro = crate::license::Key::signed(tin, crate::license::Plan::Pro, day(2026, 11, 1), day(2027, 10, 31), Some(30), [0; 64])
        .expect("ключ Pro");
    let license = OfficeLicense::enter(pro, &[], Timestamp::from_unix_millis(1_790_000_000_000)).expect("ключ Pro");
    let (id, stored) = (license.id().to_string(), crate::format::encode(&license));
    let keyed = collect(OFFICE_KINDS, [raw(OfficeLicense::KIND, &id, &stored)]).expect("собрано").entries;
    let with_key: Vec<Entry> = single.into_iter().chain(keyed).collect();
    assert!(is_full(&with_key), "ключ Pro без конторы — копия одного ИП");
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

/// Записи `note` длиной около `len` байтов каждая — `count` штук, в конвертах
/// текущей версии, как в базе: идентификатор и текст.
fn big_notes(count: usize, len: usize) -> Vec<(String, String)> {
    (0..count)
        .map(|n| {
            let note = Note {
                meta: RecordMeta::new(at(1_760_000_000_000 + n as i64)),
                text: format!("{n} ").repeat(len / 2),
            };
            (note.meta.id().to_string(), encode(&note))
        })
        .collect()
}

/// Долгие шаги кусками (0.0.28): между мегабайтами работы — пауза, которую
/// даёт окно, а итог — тот же, что одним куском. Сжатое разжимает
/// и стандартный DEFLATE: поток один, как у сжатого целиком.
#[test]
fn sliced_steps_pause_and_match_the_whole() {
    let notes = big_notes(30, 100_000);
    let raws: Vec<Raw<'_>> = notes.iter().map(|(id, text)| raw("note", id, text)).collect();
    let mut pauses = 0;
    let mut pause = || {
        pauses += 1;
        std::future::ready(())
    };

    let collected = at_once(collect_sliced(KINDS_UNDER_TEST, raws.iter().copied(), &mut pause))
        .expect("собрано");
    let whole = collect(KINDS_UNDER_TEST, raws.iter().copied()).expect("собрано");
    let stored = |c: &Collected| c.entries().iter().map(Entry::stored).collect::<Vec<_>>();
    assert_eq!(stored(&collected), stored(&whole));

    let header = sample_header();
    let packed = at_once(pack_sliced(&header, collected.entries(), &mut pause));
    let text = plaintext(&header, collected.entries());
    assert!(text.len() > 2 * SLICE_BYTES, "проба больше двух кусков");
    assert_eq!(
        miniz_oxide::inflate::decompress_to_vec(&packed).ok().as_deref(),
        Some(text.as_bytes()),
        "сжатое кусками разжимается целиком"
    );
    assert_eq!(unpack(&pack(&header, collected.entries()), MAX_PLAIN_BYTES).ok().as_deref(), Some(text.as_bytes()));

    // «Шифротекст» — сам сжатый текст: шифра в ядре нет.
    let file = at_once(seal_sliced(&header, &sample_lock(), &packed, &mut pause));
    assert_eq!(file, seal(&header, &sample_lock(), &packed));
    let sealed = at_once(open_sliced(&file, &mut pause)).expect("файл открывается");
    assert_eq!(sealed, open(&file).expect("файл открывается"));
    let entries = at_once(read_sliced(KINDS_UNDER_TEST, &sealed, sealed.ciphertext(), &mut pause))
        .expect("записи читаются");
    let read_whole = read(KINDS_UNDER_TEST, &sealed, sealed.ciphertext()).expect("записи читаются");
    assert_eq!(
        entries.iter().map(Entry::stored).collect::<Vec<_>>(),
        read_whole.iter().map(Entry::stored).collect::<Vec<_>>()
    );
    assert_eq!(entries.len(), 30);
    // Каждый шаг перешагнул два мегабайта: пауз — не меньше двух на шаг.
    drop(pause);
    assert!(pauses >= 10, "пауз — {pauses}");
}

/// Выравнивание `=` бывает только в конце всего base64, а не в конце куска:
/// файл, подправленный так, не читается и кусками.
#[test]
fn padding_inside_the_data_is_damage() {
    let piece = SLICE_BYTES / 4 * 4;
    let data = format!("{}QQ==QUFB", "A".repeat(piece - 4));
    let text = format!(
        r#"{{"file":"kvartik-backup","format_version":27,"app_version":"0.0.181","created":1760270400000,"kdf":{{"name":"PBKDF2-SHA256","iterations":600000,"salt":"AAECAwQFBgcICQoLDA0ODw=="}},"cipher":{{"name":"AES-256-GCM","iv":"ZGVmZ2hpamtsbW5v"}},"data":"{data}"}}"#
    );
    assert!(matches!(open(&text), Err(BackupError::Damaged(_))));
    let fine = text.replace("QQ==QUFB", "QUFBQUFB");
    assert!(open(&fine).is_ok(), "без «=» посередине файл читается");
}

/// Профиль ИП для проб опознания: имя по-английски и ՀՎՀՀ, если он дан.
fn owner_profile(name: &str, tin: Option<&str>, modified: i64) -> Profile {
    let mut data = ProfileData::default();
    data.name.en = Line::new(name).expect("в тесте — годное имя");
    data.tin = tin.map(|tin| Tin::parse(tin).expect("в тесте — годный ՀՎՀՀ"));
    Profile::save(None, data, at(modified))
        .expect("годный профиль")
        .expect("новый профиль")
}

/// Профили так, как их кладёт хранилище: идентификатор и текст записи.
fn stored_profiles(profiles: &[&Profile]) -> Vec<(String, String)> {
    profiles
        .iter()
        .map(|profile| (profile.id().to_string(), encode(*profile)))
        .collect()
}

fn profile_raws(rows: &[(String, String)]) -> Vec<Raw<'_>> {
    rows.iter().map(|(id, text)| raw(Profile::KIND, id, text)).collect()
}

/// Копия с этими профилями — собранная, как её собирает окно.
fn backup_with(profiles: &[&Profile]) -> Vec<Entry> {
    let rows = stored_profiles(profiles);
    collect(KINDS, profile_raws(&rows)).expect("собрано").entries
}

/// Случай опознания: название, профили базы (как их кладёт хранилище),
/// записи копии и чего ждать.
type RelationCase = (&'static str, Vec<(String, String)>, Vec<Entry>, Result<Relation, FormatError>);

/// Копия и база об одном ИП, если у них общая запись профиля или общий
/// ՀՎՀՀ, иначе — о разных, и тогда в ответе оба ИП так, как их покажет окно:
/// живой профиль, изменённый последним. Пустая база и копия без профиля —
/// «тот же»: сравнивать не с чем. Нечитаемый профиль базы сравнивается
/// идентификатором: чужая копия — вопрос без имени, своя — без вопроса,
/// а профиль новее программы — отказ целиком. Случаи с обеих сторон:
/// постоянный ответ любой из них не пройдёт.
#[test]
fn relation_tells_another_ie_apart() {
    let aram = owner_profile("IE Aram Petrosyan", Some("12345678"), 1_000);
    let mut fixed = aram.data().clone();
    fixed.tin = Some(Tin::parse("12345679").expect("годный ՀՎՀՀ"));
    let aram_fixed = Profile::save(Some(aram.clone()), fixed, at(2_000))
        .expect("годный профиль")
        .expect("правка");
    let aram_again = owner_profile("IE Aram P.", Some("12345678"), 3_000);
    let anna = owner_profile("IE Anna Hakobyan", Some("87654321"), 4_000);
    let vahe = owner_profile("IE Vahe Sargsyan", Some("11223344"), 5_000);
    let blank_here = owner_profile("", None, 6_000);
    let blank_there = owner_profile("", None, 7_000);
    let garbage = vec![(anna.id().to_string(), "не запись".to_owned())];
    let newer = vec![(
        anna.id().to_string(),
        format!(r#"{{"format_version":{},"record":{{"wings":2}}}}"#, FORMAT_VERSION + 1),
    )];
    let other = |ours: &Profile, theirs: &Profile| {
        Ok(Relation::Other {
            ours: Some(Owner::of(ours)),
            theirs: Owner::of(theirs),
        })
    };
    let same = Ok(Relation::Same);
    let cases: Vec<RelationCase> = vec![
        ("пустая база", Vec::new(), backup_with(&[&aram]), same.clone()),
        ("копия без профиля", stored_profiles(&[&aram]), Vec::new(), same.clone()),
        ("та же запись, ՀՎՀՀ поправлен", stored_profiles(&[&aram]), backup_with(&[&aram_fixed]), same.clone()),
        ("новая запись, тот же ՀՎՀՀ", stored_profiles(&[&aram]), backup_with(&[&aram_again]), same.clone()),
        ("другой ИП", stored_profiles(&[&aram]), backup_with(&[&anna]), other(&aram, &anna)),
        ("оба без ՀՎՀՀ", stored_profiles(&[&blank_here]), backup_with(&[&blank_there]), other(&blank_here, &blank_there)),
        ("в базе двое, копия второго", stored_profiles(&[&aram, &anna]), backup_with(&[&anna]), same.clone()),
        ("в базе двое, копия третьего", stored_profiles(&[&aram, &anna]), backup_with(&[&vahe]), other(&anna, &vahe)),
        (
            "профиль базы не читается, копия чужая",
            garbage.clone(),
            backup_with(&[&vahe]),
            Ok(Relation::Other { ours: None, theirs: Owner::of(&vahe) }),
        ),
        ("профиль базы не читается, копия его же", garbage, backup_with(&[&anna]), same),
        (
            "профиль базы новее программы",
            newer,
            backup_with(&[&vahe]),
            Err(FormatError::Newer { found: FORMAT_VERSION + 1 }),
        ),
    ];
    let mut wrong = Vec::new();
    for (case, base, backup, expected) in cases {
        let found = relation(&backup, profile_raws(&base));
        if found != expected {
            wrong.push(format!("{case}: {found:?}, а ждали {expected:?}"));
        }
    }
    assert!(wrong.is_empty(), "опознание ИП: {wrong:#?}");
}

/// Имя ИП — на языке окна, а без него — на первом языке, где оно есть;
/// ՀՎՀՀ — как в профиле.
#[test]
fn owner_names_the_ie_in_any_language() {
    let mut data = ProfileData::default();
    data.name.ru = Line::new("ИП Анна Акопян").expect("годное имя");
    data.name.en = Line::new("IE Anna Hakobyan").expect("годное имя");
    let anna = Profile::save(None, data, at(1_000)).expect("годный").expect("новый");
    let owner = Owner::of(&anna);
    assert_eq!(owner.name(Lang::En), "IE Anna Hakobyan");
    assert_eq!(owner.name(Lang::Ru), "ИП Анна Акопян");
    assert_eq!(owner.name(Lang::Am), "ИП Анна Акопян", "армянского нет — первый по порядку языков");
    assert_eq!(owner.tin(), None);
    let nameless = Owner::of(&owner_profile("", Some("87654321"), 2_000));
    assert_eq!(nameless.name(Lang::Am), "");
    assert_eq!(nameless.tin().map(Tin::as_str), Some("87654321"));
}

/// Виды записей проб копии всех ИП: записи баз и запись конторы.
const FULL_KINDS_UNDER_TEST: &[Kind] = &[Kind::of::<Note>(), Kind::of::<Tag>(), Kind::of::<Ip>()];

const IP_ID: &str = "5d0c2f7e-8a41-4e3b-9c6d-2b7a1f0e9d34";

/// Запись ИП конторы в базе — в конверте текущей версии.
const STORED_IP: &str = concat!(
    r#"{"format_version":27,"record":{"meta":{"id":"5d0c2f7e-8a41-4e3b-9c6d-2b7a1f0e9d34","#,
    r#""modified":1762300000000,"deleted":null},"#,
    r#""base":"kvartik","profile":null,"name":{"hy":"","ru":"ИП Анна Акопян","en":""},"#,
    r#""tin":"99999990","added":"2026-11-05","added_at":null}}"#
);

/// Открытый текст копии всех ИП версии 15 — так её писали 0.0.87–0.0.101.
const SAMPLE_PLAIN_FULL_V15: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":15,"app_version":"0.0.87","created":1760270400000,"#,
    r#""records":["#,
    r#"{"kind":"ip","record":{"meta":{"id":"5d0c2f7e-8a41-4e3b-9c6d-2b7a1f0e9d34","#,
    r#""modified":1762300000000,"deleted":null},"#,
    r#""base":"kvartik","profile":null,"name":{"hy":"","ru":"ИП Анна Акопян","en":""},"#,
    r#""tin":"99999990","added":"2026-11-05"}},"#,
    r#"{"kind":"note","base":"5d0c2f7e-8a41-4e3b-9c6d-2b7a1f0e9d34","#,
    r#""record":{"meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","#,
    r#""modified":1760000000000,"deleted":null},"text":"Ереван ֏"}},"#,
    r#"{"kind":"tag","base":"5d0c2f7e-8a41-4e3b-9c6d-2b7a1f0e9d34","#,
    r#""record":{"meta":{"id":"92d87e64-1ccb-46d0-93d7-917b1e773af5","#,
    r#""modified":1760000000500,"deleted":1760000000500}}}"#,
    r#"]}"#
);

/// Открытый текст копии всех ИП версии 16 — так её писала 0.0.102.
const SAMPLE_PLAIN_FULL_V16: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":16,"app_version":"0.0.102","created":1760270400000,"#,
    r#""records":["#,
    r#"{"kind":"ip","record":{"meta":{"id":"5d0c2f7e-8a41-4e3b-9c6d-2b7a1f0e9d34","#,
    r#""modified":1762300000000,"deleted":null},"#,
    r#""base":"kvartik","profile":null,"name":{"hy":"","ru":"ИП Анна Акопян","en":""},"#,
    r#""tin":"99999990","added":"2026-11-05"}},"#,
    r#"{"kind":"note","base":"5d0c2f7e-8a41-4e3b-9c6d-2b7a1f0e9d34","#,
    r#""record":{"meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","#,
    r#""modified":1760000000000,"deleted":null},"text":"Ереван ֏"}},"#,
    r#"{"kind":"tag","base":"5d0c2f7e-8a41-4e3b-9c6d-2b7a1f0e9d34","#,
    r#""record":{"meta":{"id":"92d87e64-1ccb-46d0-93d7-917b1e773af5","#,
    r#""modified":1760000000500,"deleted":1760000000500}}}"#,
    r#"]}"#
);

/// Открытый текст копии всех ИП версии 17 — так её писали 0.0.103–0.0.118.
const SAMPLE_PLAIN_FULL_V17: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":17,"app_version":"0.0.103","created":1760270400000,"#,
    r#""records":["#,
    r#"{"kind":"ip","record":{"meta":{"id":"5d0c2f7e-8a41-4e3b-9c6d-2b7a1f0e9d34","#,
    r#""modified":1762300000000,"deleted":null},"#,
    r#""base":"kvartik","profile":null,"name":{"hy":"","ru":"ИП Анна Акопян","en":""},"#,
    r#""tin":"99999990","added":"2026-11-05"}},"#,
    r#"{"kind":"note","base":"5d0c2f7e-8a41-4e3b-9c6d-2b7a1f0e9d34","#,
    r#""record":{"meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","#,
    r#""modified":1760000000000,"deleted":null},"text":"Ереван ֏"}},"#,
    r#"{"kind":"tag","base":"5d0c2f7e-8a41-4e3b-9c6d-2b7a1f0e9d34","#,
    r#""record":{"meta":{"id":"92d87e64-1ccb-46d0-93d7-917b1e773af5","#,
    r#""modified":1760000000500,"deleted":1760000000500}}}"#,
    r#"]}"#
);

/// Открытый текст копии всех ИП версии 18 — так её писала 0.0.119.
const SAMPLE_PLAIN_FULL_V18: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":18,"app_version":"0.0.119","created":1760270400000,"#,
    r#""records":["#,
    r#"{"kind":"ip","record":{"meta":{"id":"5d0c2f7e-8a41-4e3b-9c6d-2b7a1f0e9d34","#,
    r#""modified":1762300000000,"deleted":null},"#,
    r#""base":"kvartik","profile":null,"name":{"hy":"","ru":"ИП Анна Акопян","en":""},"#,
    r#""tin":"99999990","added":"2026-11-05"}},"#,
    r#"{"kind":"note","base":"5d0c2f7e-8a41-4e3b-9c6d-2b7a1f0e9d34","#,
    r#""record":{"meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","#,
    r#""modified":1760000000000,"deleted":null},"text":"Ереван ֏"}},"#,
    r#"{"kind":"tag","base":"5d0c2f7e-8a41-4e3b-9c6d-2b7a1f0e9d34","#,
    r#""record":{"meta":{"id":"92d87e64-1ccb-46d0-93d7-917b1e773af5","#,
    r#""modified":1760000000500,"deleted":1760000000500}}}"#,
    r#"]}"#
);

/// Открытый текст копии всех ИП версии 19 — так её писала 0.0.120.
const SAMPLE_PLAIN_FULL_V19: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":19,"app_version":"0.0.120","created":1760270400000,"#,
    r#""records":["#,
    r#"{"kind":"ip","record":{"meta":{"id":"5d0c2f7e-8a41-4e3b-9c6d-2b7a1f0e9d34","#,
    r#""modified":1762300000000,"deleted":null},"#,
    r#""base":"kvartik","profile":null,"name":{"hy":"","ru":"ИП Анна Акопян","en":""},"#,
    r#""tin":"99999990","added":"2026-11-05"}},"#,
    r#"{"kind":"note","base":"5d0c2f7e-8a41-4e3b-9c6d-2b7a1f0e9d34","#,
    r#""record":{"meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","#,
    r#""modified":1760000000000,"deleted":null},"text":"Ереван ֏"}},"#,
    r#"{"kind":"tag","base":"5d0c2f7e-8a41-4e3b-9c6d-2b7a1f0e9d34","#,
    r#""record":{"meta":{"id":"92d87e64-1ccb-46d0-93d7-917b1e773af5","#,
    r#""modified":1760000000500,"deleted":1760000000500}}}"#,
    r#"]}"#
);

/// Открытый текст копии всех ИП версии 20 — так её писала 0.0.121.
const SAMPLE_PLAIN_FULL_V20: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":20,"app_version":"0.0.121","created":1760270400000,"#,
    r#""records":["#,
    r#"{"kind":"ip","record":{"meta":{"id":"5d0c2f7e-8a41-4e3b-9c6d-2b7a1f0e9d34","#,
    r#""modified":1762300000000,"deleted":null},"#,
    r#""base":"kvartik","profile":null,"name":{"hy":"","ru":"ИП Анна Акопян","en":""},"#,
    r#""tin":"99999990","added":"2026-11-05"}},"#,
    r#"{"kind":"note","base":"5d0c2f7e-8a41-4e3b-9c6d-2b7a1f0e9d34","#,
    r#""record":{"meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","#,
    r#""modified":1760000000000,"deleted":null},"text":"Ереван ֏"}},"#,
    r#"{"kind":"tag","base":"5d0c2f7e-8a41-4e3b-9c6d-2b7a1f0e9d34","#,
    r#""record":{"meta":{"id":"92d87e64-1ccb-46d0-93d7-917b1e773af5","#,
    r#""modified":1760000000500,"deleted":1760000000500}}}"#,
    r#"]}"#
);

/// Открытый текст копии всех ИП версии 21 — так её писала 0.0.126.
const SAMPLE_PLAIN_FULL_V21: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":21,"app_version":"0.0.126","created":1760270400000,"#,
    r#""records":["#,
    r#"{"kind":"ip","record":{"meta":{"id":"5d0c2f7e-8a41-4e3b-9c6d-2b7a1f0e9d34","#,
    r#""modified":1762300000000,"deleted":null},"#,
    r#""base":"kvartik","profile":null,"name":{"hy":"","ru":"ИП Анна Акопян","en":""},"#,
    r#""tin":"99999990","added":"2026-11-05"}},"#,
    r#"{"kind":"note","base":"5d0c2f7e-8a41-4e3b-9c6d-2b7a1f0e9d34","#,
    r#""record":{"meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","#,
    r#""modified":1760000000000,"deleted":null},"text":"Ереван ֏"}},"#,
    r#"{"kind":"tag","base":"5d0c2f7e-8a41-4e3b-9c6d-2b7a1f0e9d34","#,
    r#""record":{"meta":{"id":"92d87e64-1ccb-46d0-93d7-917b1e773af5","#,
    r#""modified":1760000000500,"deleted":1760000000500}}}"#,
    r#"]}"#
);

/// Открытый текст копии всех ИП версии 22 — так её писала 0.0.130.
const SAMPLE_PLAIN_FULL_V22: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":22,"app_version":"0.0.130","created":1760270400000,"#,
    r#""records":["#,
    r#"{"kind":"ip","record":{"meta":{"id":"5d0c2f7e-8a41-4e3b-9c6d-2b7a1f0e9d34","#,
    r#""modified":1762300000000,"deleted":null},"#,
    r#""base":"kvartik","profile":null,"name":{"hy":"","ru":"ИП Анна Акопян","en":""},"#,
    r#""tin":"99999990","added":"2026-11-05"}},"#,
    r#"{"kind":"note","base":"5d0c2f7e-8a41-4e3b-9c6d-2b7a1f0e9d34","#,
    r#""record":{"meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","#,
    r#""modified":1760000000000,"deleted":null},"text":"Ереван ֏"}},"#,
    r#"{"kind":"tag","base":"5d0c2f7e-8a41-4e3b-9c6d-2b7a1f0e9d34","#,
    r#""record":{"meta":{"id":"92d87e64-1ccb-46d0-93d7-917b1e773af5","#,
    r#""modified":1760000000500,"deleted":1760000000500}}}"#,
    r#"]}"#
);

/// Открытый текст копии всех ИП версии 23 — так её писала 0.0.134.
const SAMPLE_PLAIN_FULL_V23: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":23,"app_version":"0.0.134","created":1760270400000,"#,
    r#""records":["#,
    r#"{"kind":"ip","record":{"meta":{"id":"5d0c2f7e-8a41-4e3b-9c6d-2b7a1f0e9d34","#,
    r#""modified":1762300000000,"deleted":null},"#,
    r#""base":"kvartik","profile":null,"name":{"hy":"","ru":"ИП Анна Акопян","en":""},"#,
    r#""tin":"99999990","added":"2026-11-05","added_at":null}},"#,
    r#"{"kind":"note","base":"5d0c2f7e-8a41-4e3b-9c6d-2b7a1f0e9d34","#,
    r#""record":{"meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","#,
    r#""modified":1760000000000,"deleted":null},"text":"Ереван ֏"}},"#,
    r#"{"kind":"tag","base":"5d0c2f7e-8a41-4e3b-9c6d-2b7a1f0e9d34","#,
    r#""record":{"meta":{"id":"92d87e64-1ccb-46d0-93d7-917b1e773af5","#,
    r#""modified":1760000000500,"deleted":1760000000500}}}"#,
    r#"]}"#
);

/// Открытый текст копии всех ИП версии 24 — так её писала 0.0.141.
const SAMPLE_PLAIN_FULL_V24: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":24,"app_version":"0.0.141","created":1760270400000,"#,
    r#""records":["#,
    r#"{"kind":"ip","record":{"meta":{"id":"5d0c2f7e-8a41-4e3b-9c6d-2b7a1f0e9d34","#,
    r#""modified":1762300000000,"deleted":null},"#,
    r#""base":"kvartik","profile":null,"name":{"hy":"","ru":"ИП Анна Акопян","en":""},"#,
    r#""tin":"99999990","added":"2026-11-05","added_at":null}},"#,
    r#"{"kind":"note","base":"5d0c2f7e-8a41-4e3b-9c6d-2b7a1f0e9d34","#,
    r#""record":{"meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","#,
    r#""modified":1760000000000,"deleted":null},"text":"Ереван ֏"}},"#,
    r#"{"kind":"tag","base":"5d0c2f7e-8a41-4e3b-9c6d-2b7a1f0e9d34","#,
    r#""record":{"meta":{"id":"92d87e64-1ccb-46d0-93d7-917b1e773af5","#,
    r#""modified":1760000000500,"deleted":1760000000500}}}"#,
    r#"]}"#
);

/// Открытый текст копии всех ИП версии 25 — так её пишет эта программа:
/// записи конторы без метки, записи баз — с меткой своего ИП. Выписан
/// человеком.
const SAMPLE_PLAIN_FULL_V25: &str = concat!(
    r#"{"file":"kvartik-backup","format_version":27,"app_version":"0.0.181","created":1760270400000,"#,
    r#""records":["#,
    r#"{"kind":"ip","record":{"meta":{"id":"5d0c2f7e-8a41-4e3b-9c6d-2b7a1f0e9d34","#,
    r#""modified":1762300000000,"deleted":null},"#,
    r#""base":"kvartik","profile":null,"name":{"hy":"","ru":"ИП Анна Акопян","en":""},"#,
    r#""tin":"99999990","added":"2026-11-05","added_at":null}},"#,
    r#"{"kind":"note","base":"5d0c2f7e-8a41-4e3b-9c6d-2b7a1f0e9d34","#,
    r#""record":{"meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","#,
    r#""modified":1760000000000,"deleted":null},"text":"Ереван ֏"}},"#,
    r#"{"kind":"tag","base":"5d0c2f7e-8a41-4e3b-9c6d-2b7a1f0e9d34","#,
    r#""record":{"meta":{"id":"92d87e64-1ccb-46d0-93d7-917b1e773af5","#,
    r#""modified":1760000000500,"deleted":1760000000500}}}"#,
    r#"]}"#
);

/// Записи копии всех ИП: контора с одним ИП и его база.
fn full_entries() -> Vec<Entry> {
    let ip = RecordId::parse(IP_ID).expect("идентификатор образца");
    let office = collect(OFFICE_KINDS, [raw("ip", IP_ID, STORED_IP)])
        .expect("контора собрана")
        .entries;
    let base = collect(KINDS_UNDER_TEST, stored_pair())
        .expect("база собрана")
        .entries;
    office
        .into_iter()
        .chain(base.into_iter().map(|entry| entry.in_base(ip)))
        .collect()
}

/// Файл копии разбирается видами баз и конторы вместе — ровно ими:
/// забытый вид сделал бы файл всех ИП нечитаемым.
#[test]
fn file_kinds_are_base_and_office_kinds() {
    let names = |kinds: &[Kind]| kinds.iter().map(|kind| kind.name()).collect::<Vec<_>>();
    let expected: Vec<_> = names(KINDS).into_iter().chain(names(OFFICE_KINDS)).collect();
    assert_eq!(names(FILE_KINDS), expected);
}

/// Копия всех ИП пишется слово в слово образцом и читается обратно теми же
/// записями с метками своей базы.
#[test]
fn full_backup_is_pinned_and_round_trips() {
    let entries = full_entries();
    assert_eq!(plaintext(&sample_header(), &entries), SAMPLE_PLAIN_FULL_V25);
    let back = read(
        FULL_KINDS_UNDER_TEST,
        &sealed_sample(),
        &packed(SAMPLE_PLAIN_FULL_V25),
    )
    .expect("копия всех ИП читается");
    let got: Vec<(&str, Option<String>, String)> = back
        .iter()
        .map(|entry| (entry.kind(), entry.base().map(|id| id.to_string()), entry.stored()))
        .collect();
    assert_eq!(
        got,
        [
            ("ip", None, STORED_IP.to_owned()),
            ("note", Some(IP_ID.to_owned()), STORED_NOTE.to_owned()),
            ("tag", Some(IP_ID.to_owned()), STORED_TAG.to_owned()),
        ]
    );
    // Копии всех ИП версий 15–22 читаются в те же записи, уже версии 23.
    let older = read(
        FULL_KINDS_UNDER_TEST,
        &sealed_sample_v15(),
        &packed(SAMPLE_PLAIN_FULL_V15),
    )
    .expect("копия всех ИП версии 15 читается");
    let older: Vec<(&str, Option<String>, String)> = older
        .iter()
        .map(|entry| (entry.kind(), entry.base().map(|id| id.to_string()), entry.stored()))
        .collect();
    assert_eq!(older, got, "версия 15");
    let older = read(
        FULL_KINDS_UNDER_TEST,
        &sealed_sample_v16(),
        &packed(SAMPLE_PLAIN_FULL_V16),
    )
    .expect("копия всех ИП версии 16 читается");
    let older: Vec<(&str, Option<String>, String)> = older
        .iter()
        .map(|entry| (entry.kind(), entry.base().map(|id| id.to_string()), entry.stored()))
        .collect();
    assert_eq!(older, got, "версия 16");
    let older = read(
        FULL_KINDS_UNDER_TEST,
        &sealed_sample_v17(),
        &packed(SAMPLE_PLAIN_FULL_V17),
    )
    .expect("копия всех ИП версии 17 читается");
    let older: Vec<(&str, Option<String>, String)> = older
        .iter()
        .map(|entry| (entry.kind(), entry.base().map(|id| id.to_string()), entry.stored()))
        .collect();
    assert_eq!(older, got, "версия 17");
    let older = read(
        FULL_KINDS_UNDER_TEST,
        &sealed_sample_v18(),
        &packed(SAMPLE_PLAIN_FULL_V18),
    )
    .expect("копия всех ИП версии 18 читается");
    let older: Vec<(&str, Option<String>, String)> = older
        .iter()
        .map(|entry| (entry.kind(), entry.base().map(|id| id.to_string()), entry.stored()))
        .collect();
    assert_eq!(older, got, "версия 18");
    let older = read(
        FULL_KINDS_UNDER_TEST,
        &sealed_sample_v19(),
        &packed(SAMPLE_PLAIN_FULL_V19),
    )
    .expect("копия всех ИП версии 19 читается");
    let older: Vec<(&str, Option<String>, String)> = older
        .iter()
        .map(|entry| (entry.kind(), entry.base().map(|id| id.to_string()), entry.stored()))
        .collect();
    assert_eq!(older, got, "версия 19");
    let older = read(
        FULL_KINDS_UNDER_TEST,
        &sealed_sample_v20(),
        &packed(SAMPLE_PLAIN_FULL_V20),
    )
    .expect("копия всех ИП версии 20 читается");
    let older: Vec<(&str, Option<String>, String)> = older
        .iter()
        .map(|entry| (entry.kind(), entry.base().map(|id| id.to_string()), entry.stored()))
        .collect();
    assert_eq!(older, got, "версия 20");
    let older = read(
        FULL_KINDS_UNDER_TEST,
        &sealed_sample_v21(),
        &packed(SAMPLE_PLAIN_FULL_V21),
    )
    .expect("копия всех ИП версии 21 читается");
    let older: Vec<(&str, Option<String>, String)> = older
        .iter()
        .map(|entry| (entry.kind(), entry.base().map(|id| id.to_string()), entry.stored()))
        .collect();
    assert_eq!(older, got, "версия 21");
    let older = read(
        FULL_KINDS_UNDER_TEST,
        &sealed_sample_v22(),
        &packed(SAMPLE_PLAIN_FULL_V22),
    )
    .expect("копия всех ИП версии 22 читается");
    let older: Vec<(&str, Option<String>, String)> = older
        .iter()
        .map(|entry| (entry.kind(), entry.base().map(|id| id.to_string()), entry.stored()))
        .collect();
    assert_eq!(older, got, "версия 22");
    let older = read(
        FULL_KINDS_UNDER_TEST,
        &sealed_sample_v23(),
        &packed(SAMPLE_PLAIN_FULL_V23),
    )
    .expect("копия всех ИП версии 23 читается");
    let older: Vec<(&str, Option<String>, String)> = older
        .iter()
        .map(|entry| (entry.kind(), entry.base().map(|id| id.to_string()), entry.stored()))
        .collect();
    assert_eq!(older, got, "версия 23");
    let older = read(
        FULL_KINDS_UNDER_TEST,
        &sealed_sample_v24(),
        &packed(SAMPLE_PLAIN_FULL_V24),
    )
    .expect("копия всех ИП версии 24 читается");
    let older: Vec<(&str, Option<String>, String)> = older
        .iter()
        .map(|entry| (entry.kind(), entry.base().map(|id| id.to_string()), entry.stored()))
        .collect();
    assert_eq!(older, got, "версия 24");
}

/// Копия всех ИП устроена строго: запись базы — с меткой ИП из этой же
/// копии, запись конторы — без метки, и рядом с конторой нет записей без
/// метки. Иначе — порча целиком. А одна запись в двух базах — не порча:
/// копию одного ИП могли слить «всё равно» в другой.
#[test]
fn full_backup_structure_is_checked() {
    let tagged = r#""kind":"tag","base":"5d0c2f7e-8a41-4e3b-9c6d-2b7a1f0e9d34""#;
    let mut wrong = Vec::new();
    for (case, text) in [
        (
            "метка без своего ИП",
            SAMPLE_PLAIN_FULL_V25.replace(
                tagged,
                r#""kind":"tag","base":"11111111-2222-4333-8444-555555555555""#,
            ),
        ),
        (
            "запись конторы с меткой",
            SAMPLE_PLAIN_FULL_V25.replace(
                r#"{"kind":"ip","record""#,
                r#"{"kind":"ip","base":"5d0c2f7e-8a41-4e3b-9c6d-2b7a1f0e9d34","record""#,
            ),
        ),
        (
            "запись без метки рядом с конторой",
            SAMPLE_PLAIN_FULL_V25.replace(tagged, r#""kind":"tag""#),
        ),
        (
            "метка — не идентификатор",
            SAMPLE_PLAIN_FULL_V25.replace(tagged, r#""kind":"tag","base":"kvartik""#),
        ),
        (
            "метка в копии одного ИП",
            SAMPLE_PLAIN_V27.replace(
                r#"{"kind":"tag","#,
                r#"{"kind":"tag","base":"5d0c2f7e-8a41-4e3b-9c6d-2b7a1f0e9d34","#,
            ),
        ),
    ] {
        assert_ne!(text, SAMPLE_PLAIN_FULL_V25, "{case}: подмена не сработала");
        if !matches!(
            read(FULL_KINDS_UNDER_TEST, &sealed_sample(), &packed(&text)),
            Err(BackupError::Damaged(_))
        ) {
            wrong.push(format!("{case}: читается"));
        }
    }
    let second = "6e1d3f8a-9b52-4f4c-8d7e-3c8b2a1f0e45";
    let twice = SAMPLE_PLAIN_FULL_V25.replace(
        "]}",
        &format!(
            concat!(
                r#",{{"kind":"ip","record":{{"meta":{{"id":"{0}","modified":1762300000001,"deleted":null}},"#,
                r#""base":"kvartik-ip-{0}","profile":null,"name":{{"hy":"","ru":"","en":""}},"#,
                r#""tin":null,"added":"2026-11-05"}}}},"#,
                r#"{{"kind":"note","base":"{0}","record":{{"meta":{{"id":"{1}","#,
                r#""modified":1760000000000,"deleted":null}},"text":"Ереван ֏"}}}}]}}"#
            ),
            second, NOTE_ID
        ),
    );
    match read(FULL_KINDS_UNDER_TEST, &sealed_sample(), &packed(&twice)) {
        Ok(entries) if entries.len() == 5 => {}
        other => wrong.push(format!("одна запись в двух базах: {other:?}")),
    }
    assert!(wrong.is_empty(), "устройство копии всех ИП: {wrong:#?}");
}

/// Сколько записей вида станет после слияния: прежние — и удалённые,
/// и нечитаемые — и новые из копии; запись, которую копия заменит, — одна,
/// а не две (ревью задачи 38: по этому числу окно держит предел ключей).
#[test]
fn count_after_counts_old_and_new_once() {
    let other = "6e1d3f8a-9b52-4f4c-8d7e-3c8b2a1f0e45";
    let third = "7f2e4a9b-0c63-4a5d-9e8f-4d9c3b2a1f56";
    let newer = STORED_NOTE.replace("1760000000000", "1760000009999");
    let fresh = STORED_NOTE.replace(NOTE_ID, third);
    let existing = [
        raw("note", NOTE_ID, STORED_NOTE),
        Raw {
            kind: "note",
            id: other,
            text: None,
        },
        raw("tag", TAG_ID, STORED_TAG),
    ];
    let incoming_text = [
        raw("note", NOTE_ID, &newer),
        raw("note", third, &fresh),
    ];
    let incoming = collect(KINDS_UNDER_TEST, incoming_text).expect("собрано").entries;
    let merged = merge(KINDS_UNDER_TEST, &incoming, existing).expect("слито");
    assert_eq!(merged.writes().count(), 2, "заменённая и новая");
    assert_eq!(count_after("note", existing, &merged), 3);
    assert_eq!(count_after("tag", existing, &merged), 1);
}

/// Ключ «Pro» в копии всех ИП (задача 38) — запись конторы: без метки базы
/// он читается, с меткой — порча, как и ИП списка с меткой; а рядом с ним
/// записи баз — только с меткой: копия с ключом — копия всех ИП.
#[test]
fn a_pro_key_sits_in_the_office_of_a_full_backup() {
    let kinds = &[Kind::of::<Note>(), Kind::of::<Tag>(), Kind::of::<Ip>(), Kind::of::<OfficeLicense>()];
    let tin = Tin::parse("07654321").expect("годный ՀՎՀՀ");
    let pro = crate::license::Key::signed(tin, crate::license::Plan::Pro, day(2026, 11, 1), day(2027, 10, 31), Some(30), [0; 64])
        .expect("ключ Pro");
    let license = OfficeLicense::enter(pro, &[], at(1_762_300_000_000)).expect("ключ Pro");
    let record = serde_json::to_string(&license).expect("запись");
    let with_key = |text: &str, base: &str| {
        text.replace("]}", &format!(r#",{{"kind":"office_license"{base},"record":{record}}}]}}"#))
    };
    let mut wrong = Vec::new();
    match read(kinds, &sealed_sample(), &packed(&with_key(SAMPLE_PLAIN_FULL_V25, ""))) {
        Ok(entries)
            if entries.len() == 4
                && entries
                    .iter()
                    .any(|entry| entry.kind() == OfficeLicense::KIND && entry.base().is_none()) => {}
        other => wrong.push(format!("ключ без метки: {other:?}")),
    }
    for (case, text) in [
        ("ключ с меткой базы", with_key(SAMPLE_PLAIN_FULL_V25, &format!(r#","base":"{IP_ID}""#))),
        ("ключ рядом с записями без меток", with_key(SAMPLE_PLAIN_V27, "")),
    ] {
        if !matches!(read(kinds, &sealed_sample(), &packed(&text)), Err(BackupError::Damaged(_))) {
            wrong.push(format!("{case}: читается"));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// Файлы книги в копии (задача 64): байты всех вложений — по base64 в записи,
/// с любым дополнением, — и среди них убранных из записей; записи других
/// видов не считаются.
#[test]
fn files_count_the_ledger_attachments_and_the_removed_ones() {
    use crate::model::attachment::{Attachment, FileBytes};
    let owner = RecordId::random();
    let pdf = |len: usize| {
        let mut bytes = b"%PDF-1.7\n".to_vec();
        bytes.resize(len, b' ');
        FileBytes::new(bytes).expect("годный PDF")
    };
    let kept = Attachment::create(owner, "чек.pdf", pdf(10), at(1_000));
    let other = Attachment::create(owner, "договор.pdf", pdf(11), at(1_000));
    let removed = Attachment::create(owner, "лишний.pdf", pdf(12), at(1_000))
        .remove(at(2_000))
        .expect("убран");
    let texts: Vec<(String, String)> = [&kept, &other, &removed]
        .iter()
        .map(|attachment| (attachment.id().to_string(), encode(*attachment)))
        .collect();
    let profile = Profile::save(None, ProfileData::default(), at(1_000))
        .expect("годный")
        .expect("новый");
    let profile_id = profile.id().to_string();
    let profile_text = encode(&profile);
    let raws: Vec<Raw<'_>> = texts
        .iter()
        .map(|(id, text)| raw(Attachment::KIND, id, text))
        .chain([raw(Profile::KIND, &profile_id, &profile_text)])
        .collect();
    let entries = collect(KINDS, raws).expect("собрано").entries;
    assert_eq!(files(&entries), Files { bytes: 33, removed: 12 });
    assert_eq!(files(&[]), Files::default());
    let mut wrong = Vec::new();
    for len in 1..=9 {
        let text = crate::format::base64::encode(&vec![7; len]);
        if base64_bytes(&text) != len as u64 {
            wrong.push(format!("{len} байтов — {text}: {}", base64_bytes(&text)));
        }
    }
    assert!(wrong.is_empty(), "длина base64: {wrong:#?}");
}

/// Запись вида `kind` с текстом в `bytes` байтов — для проб деления копии на
/// части (задача 117): делению поля записи не нужны, только её вид и длина.
fn sized(kind: &'static str, n: u32, bytes: usize, base: Option<RecordId>) -> Entry {
    let id = format!("00000000-0000-4000-8000-{n:012}");
    let stored = wrap(&format!(
        r#"{{"meta":{{"id":"{id}","modified":1760000000000,"deleted":null}},"text":"{}"}}"#,
        "ы".repeat(bytes / 2)
    ));
    let entry = collect(KINDS_UNDER_TEST, [raw("note", &id, &stored)])
        .expect("собрано")
        .entries
        .remove(0);
    Entry { kind, base, ..entry }
}

/// Часть для проб шапки.
fn part(number: u16, of: u16) -> Part {
    Part {
        copy: CopyId(uuid::Uuid::from_u128(0x0f8c_3a52_5b1e_4c7d_9a2b_6e4f_1d3c_2b1a)),
        number,
        of,
    }
}

/// Копия, которая влезает в предел, — одна, без номера части, со всеми
/// записями по порядку: файл тот же, что до частей.
#[test]
fn a_copy_within_the_limit_stays_one_file() {
    let entries: Vec<Entry> = (1..=5).map(|n| sized(Note::KIND, n, 1000, None)).collect();
    let parts = split(&sample_header(), &entries, 1 << 20);
    assert_eq!(parts.len(), 1);
    assert_eq!(parts[0].0, sample_header());
    assert_eq!(parts[0].0.part(), None);
    let ids: Vec<RecordId> = parts[0].1.iter().map(|entry| entry.meta().id()).collect();
    let all: Vec<RecordId> = entries.iter().map(|entry| entry.meta().id()).collect();
    assert_eq!(ids, all);
}

/// Копия больше предела — частями (задача 117): в каждой — профили и ИП
/// конторы, в первой — записи без файлов, дальше — файлы по порядку; каждая
/// запись, кроме общих, — ровно в одной части, и текст каждой не больше
/// предела. Номера — по порядку, копия у всех одна, а у другой копии — своя.
#[test]
fn a_copy_over_the_limit_goes_in_parts() {
    let ip = RecordId::random();
    let mut entries = vec![sized(Ip::KIND, 1, 200, None)];
    // Файлы книги — вперемешку с записями, как их отдаёт база.
    for n in 2..=13 {
        let kind = if n % 3 == 0 { Attachment::KIND } else { Note::KIND };
        let bytes = if kind == Attachment::KIND { 40_000 } else { 3_000 };
        entries.push(sized(kind, n, bytes, Some(ip)));
    }
    entries.push(sized(Profile::KIND, 20, 500, Some(ip)));
    entries.push(sized(Profile::KIND, 21, 500, Some(ip)));
    let limit = 100_000;
    let parts = split(&sample_header(), &entries, limit);
    assert!(parts.len() >= 2, "частей — {}", parts.len());
    let of = u16::try_from(parts.len()).expect("частей мало");
    let copy = parts[0].0.part().expect("часть").copy();
    let mut seen: Vec<RecordId> = Vec::new();
    let mut wrong = Vec::new();
    for (index, (header, records)) in parts.iter().enumerate() {
        let number = u16::try_from(index + 1).expect("частей мало");
        if header.part() != Some(Part { copy, number, of }) {
            wrong.push(format!("часть {number}: шапка {:?}", header.part()));
        }
        if header.app_version() != "0.0.181" || header.created() != sample_header().created() {
            wrong.push(format!("часть {number}: шапка не той копии"));
        }
        let shared: Vec<&str> = records
            .iter()
            .filter(|entry| entry.kind() == Ip::KIND || entry.kind() == Profile::KIND)
            .map(|entry| entry.kind())
            .collect();
        if shared != [Ip::KIND, Profile::KIND, Profile::KIND] {
            wrong.push(format!("часть {number}: общие — {shared:?}"));
        }
        let text = plaintext(header, records.iter().copied());
        if text.len() > limit {
            wrong.push(format!("часть {number}: текст {} байтов", text.len()));
        }
        for entry in records
            .iter()
            .filter(|entry| entry.kind() == Note::KIND || entry.kind() == Attachment::KIND)
        {
            if number > 1 && entry.kind() == Note::KIND {
                wrong.push(format!("часть {number}: запись без файла не в первой"));
            }
            seen.push(entry.meta().id());
        }
    }
    // Каждая — один раз, записи без файлов — впереди, файлы — в порядке копии.
    let notes = entries.iter().filter(|entry| entry.kind() == Note::KIND);
    let files = entries.iter().filter(|entry| entry.kind() == Attachment::KIND);
    let expected: Vec<RecordId> = notes.chain(files).map(|entry| entry.meta().id()).collect();
    if seen != expected {
        wrong.push(format!("записи частей: {seen:?}"));
    }
    let again = split(&sample_header(), &entries, limit);
    if again[0].0.part().map(|part| part.copy()) == Some(copy) {
        wrong.push("у двух копий один идентификатор".to_owned());
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// Запись, которая и одна больше предела, ложится частью сама, а не
/// пропадает: файл выйдет больше предела, и окно его не отдаст.
#[test]
fn a_record_over_the_limit_is_a_part_of_its_own() {
    let entries = vec![sized(Note::KIND, 1, 500, None), sized(Attachment::KIND, 2, 50_000, None)];
    let parts = split(&sample_header(), &entries, 10_000);
    let sizes: Vec<usize> = parts.iter().map(|(_, records)| records.len()).collect();
    assert_eq!(sizes, [1, 1]);
    assert_eq!(parts[1].1[0].kind(), Attachment::KIND);
}

/// Каждая часть — копия сама по себе: файл открывается, его текст читается
/// с той же частью в шапке, а части копии всех ИП — каждая с ИП конторы
/// (`whole`): записям с меткой базы есть куда лечь.
#[test]
fn every_part_opens_and_reads_alone() {
    let ip = collect(OFFICE_KINDS, [raw("ip", IP_ID, STORED_IP)]).expect("собрано").entries;
    let base = ip[0].meta().id();
    let mut entries = ip;
    entries.extend((1..=6).map(|n| sized(Note::KIND, n, 20_000, Some(base))));
    let parts = split(&sample_header(), &entries, 50_000);
    assert!(parts.len() >= 3, "частей — {}", parts.len());
    let mut read_back = 0;
    for (header, records) in &parts {
        let file = seal(header, &sample_lock(), &(200..220).collect::<Vec<u8>>());
        let sealed = open(&file).expect("файл части открывается");
        assert_eq!(sealed.header(), header);
        let packed = pack(header, records.iter().copied());
        let got = read(FULL_KINDS_UNDER_TEST, &sealed, &packed).expect("часть читается");
        assert!(is_full(&got));
        assert_eq!(got.iter().filter(|entry| entry.kind() == Ip::KIND).count(), 1);
        read_back += got.iter().filter(|entry| entry.kind() == Note::KIND).count();
    }
    assert_eq!(read_back, 6);
}

/// Часть в шапке — снаружи и внутри: записывается полем `part`, читается
/// обратно, а снаружи и внутри обязана совпасть.
#[test]
fn part_is_in_both_headers() {
    let header = sample_header().with_part(part(2, 3));
    let file = seal(&header, &sample_lock(), &(200..220).collect::<Vec<u8>>());
    assert_eq!(
        file,
        concat!(
            r#"{"file":"kvartik-backup","format_version":27,"app_version":"0.0.181","created":1760270400000,"#,
            r#""part":{"copy":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","number":2,"of":3},"#,
            r#""kdf":{"name":"PBKDF2-SHA256","iterations":600000,"salt":"AAECAwQFBgcICQoLDA0ODw=="},"#,
            r#""cipher":{"name":"AES-256-GCM","iv":"ZGVmZ2hpamtsbW5v"},"#,
            r#""data":"yMnKy8zNzs/Q0dLT1NXW19jZ2ts="}"#
        )
    );
    let sealed = open(&file).expect("открывается");
    assert_eq!(sealed.header().part(), Some(part(2, 3)));
    let entries = collect(KINDS_UNDER_TEST, stored_pair()).expect("собрано").entries;
    let text = plaintext(&header, &entries);
    assert!(text.starts_with(concat!(
        r#"{"file":"kvartik-backup","format_version":27,"app_version":"0.0.181","created":1760270400000,"#,
        r#""part":{"copy":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","number":2,"of":3},"records":["#
    )));
    let read_back = read(KINDS_UNDER_TEST, &sealed, &pack(&header, &entries)).expect("читается");
    assert_eq!(read_back.len(), 2);
    // Внутри — другая часть, без части или другая копия: подмена шапки.
    let mut wrong = Vec::new();
    let other_copy = Part {
        copy: CopyId(uuid::Uuid::from_u128(7)),
        ..part(2, 3)
    };
    for inner in [
        sample_header(),
        sample_header().with_part(part(1, 3)),
        sample_header().with_part(other_copy),
    ] {
        if read(KINDS_UNDER_TEST, &sealed, &pack(&inner, &entries)).is_ok() {
            wrong.push(format!("прочитана с частью {:?} внутри", inner.part()));
        }
    }
    // Снаружи без части, внутри — с частью.
    let whole = open(&seal(&sample_header(), &sample_lock(), &(200..220).collect::<Vec<u8>>()))
        .expect("открывается");
    if read(KINDS_UNDER_TEST, &whole, &pack(&header, &entries)).is_ok() {
        wrong.push("прочитана без части снаружи".to_owned());
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// Часть в шапке — чужой ввод (Правило 13): не по образцу — порча, а не
/// копия, и в шапке версии до 27 её не бывает.
#[test]
fn a_wrong_part_is_damage() {
    let good = seal(
        &sample_header().with_part(part(2, 3)),
        &sample_lock(),
        &(200..220).collect::<Vec<u8>>(),
    );
    assert!(open(&good).is_ok());
    let spoil = |from: &str, to: &str| good.replace(from, to);
    let cases = [
        spoil(r#""number":2"#, r#""number":0"#),
        spoil(r#""number":2"#, r#""number":4"#),
        spoil(r#""number":2,"of":3"#, r#""number":1,"of":1"#),
        spoil(r#""of":3"#, r#""of":1000"#),
        spoil(r#""of":3"#, r#""of":70000"#),
        spoil(r#""of":3"#, r#""of":-3"#),
        spoil(r#""of":3"#, r#""of":3,"wings":1"#),
        spoil(r#","of":3"#, ""),
        spoil("0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a", "не идентификатор"),
        spoil(
            r#""part":{"copy":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","number":2,"of":3}"#,
            r#""part":7"#,
        ),
        spoil(r#""format_version":27"#, r#""format_version":26"#),
        spoil(r#""format_version":27"#, r#""format_version":1"#),
    ];
    let wrong: Vec<&String> = cases
        .iter()
        .filter(|text| !matches!(open(text), Err(BackupError::Damaged(_))))
        .collect();
    assert!(wrong.is_empty(), "открыты: {wrong:#?}");
}

/// Файл копии не больше [`sealed_bound`] своего открытого текста — и у текста,
/// который не сжимается: файлы книги — уже сжатые JPEG и PDF. На нём держится
/// [`PART_PLAIN_BYTES`]: часть не выходит больше [`MAX_FILE_BYTES`].
#[test]
fn a_file_stays_within_the_bound_of_its_text() {
    // Случайные байты — xorshift: сжимать в них нечего.
    let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
    let noise: Vec<u8> = (0..3 << 20)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state.to_le_bytes()[0]
        })
        .collect();
    let mut wrong = Vec::new();
    for plain in [noise.clone(), vec![b'a'; 3 << 20], noise[..100].to_vec(), Vec::new()] {
        let packed = at_once(deflate_sliced(&plain, PACK_LEVEL, &mut no_pause));
        let ciphertext = vec![0; packed.len() + TAG_BYTES];
        let file = seal(&sample_header().with_part(part(999, 999)), &sample_lock(), &ciphertext);
        if file.len() > sealed_bound(plain.len()) {
            wrong.push(format!(
                "{} байтов текста — файл {} больше {}",
                plain.len(),
                file.len(),
                sealed_bound(plain.len())
            ));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
    assert!(sealed_bound(PART_PLAIN_BYTES) <= MAX_FILE_BYTES as usize);
}

/// Имя части — номер и число частей перед расширением; у копии одним файлом
/// имя то же (задача 117).
#[test]
fn part_file_names_carry_the_number() {
    let today = day(2026, 11, 23);
    assert_eq!(part_file_name(&file_name(today), None), "kvartik-2026-11-23.json");
    assert_eq!(
        part_file_name(&file_name(today), Some(part(2, 3))),
        "kvartik-2026-11-23-2of3.json"
    );
    let tin = Tin::parse("99999990").expect("ՀՎՀՀ");
    assert_eq!(
        part_file_name(&client_file_name(Some(&tin), today), Some(part(12, 999))),
        "kvartik-99999990-2026-11-23-12of999.json"
    );
    assert_eq!(part_of_name("2of3"), Some((2, 3)));
    assert_eq!(part_of_name("1of1"), None);
}
