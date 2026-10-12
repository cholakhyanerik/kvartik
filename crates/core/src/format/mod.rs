//! Формат данных: версия формата, запись в JSON и чтение обратно.
//!
//! Байты на входе и на выходе: ни IndexedDB, ни файлов браузера здесь нет
//! (таблица слоёв CLAUDE.md). Хранилище окна (`crates/app/src/store/`)
//! кладёт в базу то, что дал [`encode`], и отдаёт [`decode`] то, что
//! прочитало; файл копии ([`backup`]) собирается из тех же записей.
//!
//! # Версия — у каждой записи
//!
//! Запись лежит вместе с версией формата, по которой её записали:
//! `{"format_version":1,"record":{…}}`. Не у базы целиком — потому что
//! в одну базу пишут сразу две вкладки разных версий программы (Правило 6,
//! «Старая вкладка молча держит базу»), а базу без смены её собственной
//! версии ни одна из них не запирает. Новая вкладка пишет записи своей
//! версии; старая, прочитав такую, отказывается ([`FormatError::Newer`]),
//! а не выбрасывает незнакомые поля и не стирает их при правке (Правило 2).
//! Каждая правка в хранилище — «прочитать — посчитать — записать» одной
//! транзакцией, поэтому записи новее себя старая вкладка не перепишет.
//!
//! Номер [`FORMAT_VERSION`] растёт, когда меняется то, что лежит в записи:
//! новое поле (с `#[serde(default)]`), новый вид записи, переименование или
//! удаление поля — последние два только миграцией с тестом на образце
//! прошлой версии (Правило 2). Поле, добавленное без нового номера, старая
//! вкладка прочла бы как своё. Версия формата — не версия программы и не
//! версия базы IndexedDB (Правило 5).
//!
//! Незнакомое поле в записи своей версии — ошибка, а не молчание: у типов
//! записей `#[serde(deny_unknown_fields)]`. Так забытый подъём номера
//! ломается громко, а не стирает поле у того, кто откроет данные старой
//! версией.

pub mod backup;
mod base64;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

use crate::model::record::RecordMeta;

/// Версия формата записей, которую пишет и понимает эта программа.
///
/// Записи прошлых версий читаются переводом при чтении (миграция
/// `migrate_vN_to_vN1`, Правило 2), более новых — не читаются. Пока версия
/// одна, переводить нечего.
pub const FORMAT_VERSION: u32 = 1;

/// Запись, которая хранится: у неё есть вид и общие поля.
pub trait Stored: Serialize + DeserializeOwned {
    /// Вид записи — имя её раздела в хранилище и в файле копии: `"client"`,
    /// `"invoice"`. Латиница в нижнем регистре и `_`. Часть формата:
    /// переименовать вид — значит перевести данные всех пользователей
    /// миграцией. Новый вид — строкой в [`backup::KINDS`], иначе копия его
    /// не возьмёт.
    const KIND: &'static str;

    /// Общие поля: идентификатор, момент изменения, метка удаления.
    fn meta(&self) -> &RecordMeta;
}

/// Почему запись не прочитана.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum FormatError {
    /// Запись сделана более новой версией программы: её формат — `found`,
    /// а эта понимает только до [`FORMAT_VERSION`]. Читать её нельзя:
    /// незнакомое старая версия выбросила бы молча (Правило 2). Человеку —
    /// «откройте новую версию», то есть перезагрузите страницу.
    Newer { found: u32 },
    /// Запись не читается: испорчена, обрезана или не наша. В строке —
    /// подробность разбора для консоли разработчика; человеку окно говорит
    /// словами.
    Damaged(String),
}

/// Запись при чтении — после того, как версия проверена. Сама запись ещё не
/// разобрана: её разбирает [`decode_record`] по версии.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope<'a> {
    format_version: u32,
    #[serde(borrow)]
    record: &'a RawValue,
}

/// Только версия: остальное разбор пропускает, не заглядывая в поля.
#[derive(Deserialize)]
struct Header {
    format_version: u32,
}

/// Запись в JSON вместе с версией формата.
///
/// Не падает на типах записей: `serde_json` отказывает только ключам
/// словаря, которые не строки и не числа, и ошибкам собственных `Serialize`,
/// а в записях нет ни того, ни другого. Новый тип записи держит это своим
/// тестом «записано — прочитано».
pub fn encode<T: Serialize>(record: &T) -> String {
    wrap(&serde_json::to_string(record).expect(
        "запись в JSON не падает: в типах записей нет ни словарей с ключами-структурами, ни своих ошибок",
    ))
}

/// Запись, уже в JSON, — в конверт с версией формата. Поля в том же
/// порядке, что всегда: сначала версия, потом запись.
fn wrap(record: &str) -> String {
    format!(r#"{{"format_version":{FORMAT_VERSION},"record":{record}}}"#)
}

/// Запись из JSON — с проверкой версии и каждого поля.
///
/// Версия проверяется раньше полей: запись новой версии с незнакомым полем
/// иначе выглядела бы испорченной, и человек услышал бы «данные испорчены»
/// вместо «откройте новую версию».
pub fn decode<T: DeserializeOwned>(text: &str) -> Result<T, FormatError> {
    let (version, record) = split(text)?;
    decode_record(version, record)
}

/// Версия и ещё не разобранная запись из конверта. Версия — раньше
/// остального: конверт новой версии может быть и другим.
fn split(text: &str) -> Result<(u32, &str), FormatError> {
    let header: Header = serde_json::from_str(text).map_err(damaged)?;
    if header.format_version > FORMAT_VERSION {
        return Err(FormatError::Newer {
            found: header.format_version,
        });
    }
    let envelope: Envelope<'_> = serde_json::from_str(text).map_err(damaged)?;
    Ok((envelope.format_version, envelope.record.get()))
}

/// Запись версии формата `version` из JSON её полей.
///
/// Единственное место, где читаются записи прошлых версий: здесь встанут
/// миграции `migrate_vN_to_vN1` (Правило 2), и через него читают и база,
/// и файл копии.
fn decode_record<T: DeserializeOwned>(version: u32, record: &str) -> Result<T, FormatError> {
    match version {
        FORMAT_VERSION => serde_json::from_str(record).map_err(damaged),
        found if found > FORMAT_VERSION => Err(FormatError::Newer { found }),
        // Версий до первой нет; прошлые версии появятся здесь вместе со своими
        // миграциями.
        found => Err(FormatError::Damaged(format!(
            "версия формата {found}: такой не бывает"
        ))),
    }
}

/// Версия формата, которой сделана запись; `None` — запись не читается.
///
/// Хранилищу этого хватает, чтобы не переписать запись новее себя, не
/// разбирая её полей (Правило 2): правка старой вкладки поверх записи новой
/// стёрла бы всё, чего старая не знает.
pub fn version(text: &str) -> Option<u32> {
    serde_json::from_str::<Header>(text)
        .ok()
        .map(|header| header.format_version)
}

fn damaged(error: serde_json::Error) -> FormatError {
    FormatError::Damaged(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::date::Date;
    use crate::model::money::{Currency, ExchangeRate, Money, RateSource};
    use crate::model::record::Timestamp;

    /// Проба — запись из всех кирпичей, которые уже есть в домене.
    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    #[serde(deny_unknown_fields)]
    struct Probe {
        meta: RecordMeta,
        day: Date,
        sum: Money,
        rate: Option<ExchangeRate>,
    }

    impl Stored for Probe {
        const KIND: &'static str = "probe";

        fn meta(&self) -> &RecordMeta {
            &self.meta
        }
    }

    fn day(year: u16, month: u8, day: u8) -> Date {
        Date::new(year, month, day).expect("в тесте — существующая дата")
    }

    fn probe() -> Probe {
        let rate = ExchangeRate::new(
            Currency::Usd,
            3_619_300,
            day(2026, 10, 9),
            RateSource::CentralBank,
        )
        .expect("настоящий курс");
        Probe {
            meta: RecordMeta::new(Timestamp::from_unix_millis(1_760_000_000_000)),
            day: day(2026, 10, 12),
            sum: Money::new(150_000, Currency::Usd),
            rate: Some(rate),
        }
    }

    /// Образец версии 1, выписанный человеком, а не записанный кодом: код,
    /// записавший себе эталон, проверял бы сам себя (Правило 2).
    const SAMPLE_V1: &str = concat!(
        r#"{"format_version":1,"record":{"#,
        r#""meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","modified":1760000000000,"deleted":null},"#,
        r#""day":"2026-10-12","#,
        r#""sum":{"minor":150000,"currency":"USD"},"#,
        r#""rate":{"currency":"USD","per_unit_e4":3619300,"date":"2026-10-09","source":"central_bank"}"#,
        r#"}}"#
    );

    /// Записанное читается обратно в ту же запись.
    #[test]
    fn what_is_encoded_decodes_back() {
        let record = probe();
        assert_eq!(decode::<Probe>(&encode(&record)), Ok(record));
        let without_rate = Probe {
            rate: None,
            ..probe()
        };
        assert_eq!(decode::<Probe>(&encode(&without_rate)), Ok(without_rate));
    }

    /// Образец версии 1 читается в ожидаемые значения, а запись его
    /// значений — слово в слово образец. Поменялась запись — это новая
    /// версия формата с миграцией (Правило 5), а не правка образца.
    #[test]
    fn sample_v1_is_pinned() {
        let record: Probe = decode(SAMPLE_V1).expect("образец версии 1 читается");
        assert_eq!(
            record.meta.id().to_string(),
            "0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a"
        );
        assert_eq!(
            record.meta.modified(),
            Timestamp::from_unix_millis(1_760_000_000_000)
        );
        assert_eq!(record.meta.deleted(), None);
        assert_eq!(record.day, day(2026, 10, 12));
        assert_eq!(record.sum, Money::new(150_000, Currency::Usd));
        assert_eq!(record.rate, probe().rate);
        assert_eq!(encode(&record), SAMPLE_V1);
    }

    /// Запись более новой версии — «новее», даже если её поля незнакомы
    /// или испорчены: версия проверяется раньше полей.
    #[test]
    fn newer_records_are_refused_by_version() {
        let cases = [
            (
                r#"{"format_version":2,"record":{"meta":"что угодно","wings":2}}"#,
                2,
            ),
            (r#"{"record":null,"format_version":7}"#, 7),
            (r#"{"format_version":4294967295,"record":{}}"#, u32::MAX),
        ];
        let mut wrong = Vec::new();
        for (text, found) in cases {
            let got = decode::<Probe>(text);
            if got != Err(FormatError::Newer { found }) {
                wrong.push(format!("{text}: {got:?}"));
            }
        }
        assert!(wrong.is_empty(), "новая версия не узнана: {wrong:#?}");
    }

    /// Версию записи хранилище узнаёт, не разбирая полей.
    #[test]
    fn version_is_read_without_fields() {
        assert_eq!(version(SAMPLE_V1), Some(1));
        assert_eq!(
            version(r#"{"format_version":2,"record":{"wings":2}}"#),
            Some(2)
        );
        assert_eq!(version(r#"{"record":{}}"#), None);
        assert_eq!(version("не JSON"), None);
    }

    /// Испорченное не читается и не роняет программу: ни обрезанная запись,
    /// ни чужая, ни запись без версии, ни запись своей версии с незнакомым
    /// или недостающим полем, ни запретное значение внутри.
    #[test]
    fn damaged_records_are_refused() {
        let v1 = |record: &str| format!(r#"{{"format_version":1,"record":{record}}}"#);
        let meta = r#""meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","modified":1760000000000,"deleted":null}"#;
        let rest = r#""sum":{"minor":150000,"currency":"USD"},"rate":null"#;
        let mut cases = vec![
            String::new(),
            "не JSON".to_owned(),
            SAMPLE_V1[..SAMPLE_V1.len() - 5].to_owned(),
            format!("{SAMPLE_V1} хвост"),
            r#"{"record":{}}"#.to_owned(),
            r#"{"format_version":"1","record":{}}"#.to_owned(),
            r#"{"format_version":1.0,"record":{}}"#.to_owned(),
            r#"{"format_version":-1,"record":{}}"#.to_owned(),
            r#"{"format_version":0,"record":{}}"#.to_owned(),
            r#"{"format_version":1}"#.to_owned(),
            r#"[1,{}]"#.to_owned(),
            SAMPLE_V1.replace(r#""record""#, r#""extra":1,"record""#),
            v1(&format!(
                r#"{{{meta},"day":"2026-10-12",{rest},"wings":2}}"#
            )),
            v1(&format!(r#"{{{meta},{rest}}}"#)),
            v1(&format!(r#"{{{meta},"day":"2026-00-12",{rest}}}"#)),
            v1(&format!(r#"{{{meta},"day":"2026-02-30",{rest}}}"#)),
            SAMPLE_V1.replace("3619300", "0"),
            SAMPLE_V1.replace(
                r#""currency":"USD","per_unit_e4""#,
                r#""currency":"AMD","per_unit_e4""#,
            ),
            SAMPLE_V1.replace("1760000000000", "8640000000000001"),
            SAMPLE_V1.replace(r#""deleted":null"#, r#""deleted":1760000000001"#),
            // Вложенность глубже предела `serde_json` (128) — ошибка, а не
            // переполнение стека.
            format!(
                r#"{{"format_version":1,"record":{}{}}}"#,
                "[".repeat(10_000),
                "]".repeat(10_000)
            ),
        ];
        cases.push(SAMPLE_V1.replace("2026-10-12", "2026-10-12\u{0}"));
        let mut wrong = Vec::new();
        for text in &cases {
            match decode::<Probe>(text) {
                Err(FormatError::Damaged(_)) => {}
                other => wrong.push(format!("{text:.120}: {other:?}")),
            }
        }
        assert!(wrong.is_empty(), "испорченное не распознано: {wrong:#?}");
    }
}
