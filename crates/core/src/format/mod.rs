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

pub mod autobackup;
pub mod backup;
pub mod export;
pub mod office;
pub(crate) mod base64;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

use crate::model::record::RecordMeta;

/// Версия формата записей, которую пишет и понимает эта программа.
///
/// Записи прошлых версий читаются переводом при чтении (миграция
/// `migrate_vN_to_vN1`, Правило 2), более новых — не читаются. Что
/// меняла каждая версия:
///
/// - 1 (0.0.7) — конверт записи; видов записей ещё нет.
/// - 2 (0.0.12) — виды `profile` и `logo` (задача 9). Полей у прежних видов
///   не прибавилось — их не было, — и запись версии 1 читается как есть.
/// - 3 (0.0.14) — вид `client` (задача 10). Поля прежних видов те же, и записи
///   версий 1 и 2 читаются как есть.
/// - 4 (0.0.15) — у клиента поле `tax_id` (налоговый номер иностранного
///   клиента, `#[serde(default)]`) и страна Косово, `XK`. Записи версий 1–3
///   читаются как есть: у клиента без поля номер пустой.
/// - 5 (0.0.17) — виды `document` (счёт и акт) и `recurring` (повторяющийся
///   счёт), задача 11. Поля прежних видов те же, и записи версий 1–4
///   читаются как есть.
/// - 6 (0.0.27) — виды `operation` (операция книги) и `attachment`
///   (вложение), у документа — поле `rate` (курс для книги,
///   `#[serde(default)]`), задача 14. Записи версий 1–5 читаются как есть:
///   у документа без поля курса нет. Файл копии с этой версии сжат перед
///   шифром (`backup`).
/// - 7 (0.0.40) — у документа поле `signed` (день подписи акта клиентом,
///   `#[serde(default)]`), задача 62. Записи версий 1–6 читаются как есть:
///   документ без поля не подписан.
/// - 8 (0.0.47) — налог квартала, задача 19: вид `closing` (закрытый
///   квартал), у документа поле `activity` (вид деятельности дохода),
///   у профиля — `kinds` (строка налога, выбранная коду; оба
///   `#[serde(default)]`), у дохода книги — вид деятельности, у расхода —
///   вид `levy` (зарплата, налог, пошлина, услуга банка). Записи версий 1–7
///   читаются как есть: без полей — не указано.
/// - 9 (0.0.49) — годовые платежи, задача 20: у профиля поле
///   `person.registered` (день государственной регистрации ИП,
///   `#[serde(default)]`). Записи версий 1–8 читаются как есть: без поля
///   день не указан.
/// - 10 (0.0.55) — у документа поле `settlement` (расчётный документ,
///   выписанный в системе ՊԵԿ: день и номер; `null` — не отмечен), задача
///   23. Записи версий 1–9 читаются как есть: у документа без поля
///   неизвестно, отмечен ли он, — отмечать было негде, — и ключа нет и при
///   записи, пока документ не отметят.
/// - 11 (0.0.60) — вид `license` (введённый ключ лицензии), задача 25.
///   Поля прежних видов те же, и записи версий 1–10 читаются как есть.
/// - 12 (0.0.66) — вид `opening` (перенос вычета с кварталов до программы),
///   задача 69. Поля прежних видов те же, и записи версий 1–11 читаются как
///   есть.
/// - 13 (0.0.68) — у документа и операции книги поле `income_kind` (строка
///   налога дохода, если она не та, что у кода, `#[serde(default)]`), задача
///   70. Записи версий 1–12 читаются как есть: без поля строка — по коду.
/// - 14 (0.0.69) — у профиля поля `person.pension_applied` (день заявления
///   о выборе пенсионного фонда) и `person.resident` (резидент ли Армении; оба
///   `#[serde(default)]`), задача 73. Записи версий 1–13 читаются как есть:
///   без полей — не указано.
/// - 15 (0.0.87) — вид `ip` (ИП в списке конторы) и у записи копии метка
///   базы `base` — копия всех ИП одним файлом (`backup`), задача 38. Поля
///   прежних видов те же, и записи версий 1–14 читаются как есть.
/// - 16 (0.0.102) — у профиля поле `person.turnover_applied` (день подачи
///   заявления о налоге с оборота нового ИП, `#[serde(default)]`), полный
///   прогон 2026-11-10, п. 7. Записи версий 1–15 читаются как есть: без поля
///   не отмечено.
/// - 17 (0.0.103) — у банковского счёта поле `bank_latin` (банк латиницей для
///   счёта и акта не на армянском, `#[serde(default)]`) — в профиле и в копии
///   реквизитов выпущенного документа, полный прогон 2026-11-10, п. 8.
///   Записи версий 1–16 читаются как есть: без поля — пусто, и документ пишет
///   банк, как он записан.
/// - 18 (0.0.119) — у расхода книги поле `cost` (прямой или сбытовой
///   и административный), а у строки снимка закрытого квартала — `direct`
///   (прямые расходы, x.4.1; оба `#[serde(default)]`), задача 79. Записи
///   версий 1–17 читаются как есть: без полей — не сказано, и x.4.1 и x.4.2
///   делит человек.
/// - 19 (0.0.120) — у расхода книги поле `income_kind` (строка дохода «аренда,
///   проценты, роялти» или «продажа имущества», к которой отнесён расход),
///   задача 95. Поле было и раньше, но у расхода — только `null`: прежняя
///   версия назвала бы такую запись порчей, а не записью новее себя. Записи
///   версий 1–18 читаются как есть.
/// - 20 (0.0.121) — у расхода книги вид `non_culinary` (товар, который общепит
///   продаёт, не готовя), задача 94. Записи версий 1–19 читаются как есть.
/// - 21 (0.0.126) — у профиля поле `micro` (годы, в которые ИП —
///   микропредприятие, `#[serde(default)]`), задача 100. Записи версий 1–20
///   читаются как есть: без поля все годы — налог с оборота.
pub const FORMAT_VERSION: u32 = 21;

/// Первая версия формата: старше не бывает.
pub(crate) const FIRST_VERSION: u32 = 1;

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
    split_raw(text).map(|(version, record)| (version, record.get()))
}

/// [`split`], но запись — нетронутым куском JSON: копия кладёт запись
/// текущей версии тем же текстом (`backup`).
fn split_raw(text: &str) -> Result<(u32, &RawValue), FormatError> {
    let header: Header = serde_json::from_str(text).map_err(damaged)?;
    if header.format_version > FORMAT_VERSION {
        return Err(FormatError::Newer {
            found: header.format_version,
        });
    }
    let envelope: Envelope<'_> = serde_json::from_str(text).map_err(damaged)?;
    Ok((envelope.format_version, envelope.record))
}

/// Запись версии формата `version` из JSON её полей.
///
/// Единственное место, где читаются записи прошлых версий: здесь встанут
/// миграции `migrate_vN_to_vN1` (Правило 2), и через него читают и база,
/// и файл копии.
fn decode_record<T: DeserializeOwned>(version: u32, record: &str) -> Result<T, FormatError> {
    match version {
        // Версии 2–21 прибавили виды записей и поля со значением по
        // умолчанию, а не переименовали и не убрали прежние (шапка
        // `FORMAT_VERSION`): записи прошлых версий читаются как есть.
        FIRST_VERSION..=FORMAT_VERSION => serde_json::from_str(record).map_err(damaged),
        found if found > FORMAT_VERSION => Err(FormatError::Newer { found }),
        // Версий до первой нет.
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

    /// Та же запись в версии 2 — так её писала 0.0.12. Тоже выписана
    /// человеком: версия 2 прибавила виды записей, а не поля, и от версии 1
    /// запись отличается только номером.
    const SAMPLE_V2: &str = concat!(
        r#"{"format_version":2,"record":{"#,
        r#""meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","modified":1760000000000,"deleted":null},"#,
        r#""day":"2026-10-12","#,
        r#""sum":{"minor":150000,"currency":"USD"},"#,
        r#""rate":{"currency":"USD","per_unit_e4":3619300,"date":"2026-10-09","source":"central_bank"}"#,
        r#"}}"#
    );

    /// Та же запись в версии 3 — так её писала 0.0.14. Выписана человеком:
    /// версия 3 прибавила вид записей, а не поля, и запись отличается только
    /// номером.
    const SAMPLE_V3: &str = concat!(
        r#"{"format_version":3,"record":{"#,
        r#""meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","modified":1760000000000,"deleted":null},"#,
        r#""day":"2026-10-12","#,
        r#""sum":{"minor":150000,"currency":"USD"},"#,
        r#""rate":{"currency":"USD","per_unit_e4":3619300,"date":"2026-10-09","source":"central_bank"}"#,
        r#"}}"#
    );

    /// Та же запись в версии 4 — так её писала 0.0.15. Выписана человеком:
    /// версия 4 прибавила поле клиенту, а у пробы его нет, и запись
    /// отличается только номером.
    const SAMPLE_V4: &str = concat!(
        r#"{"format_version":4,"record":{"#,
        r#""meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","modified":1760000000000,"deleted":null},"#,
        r#""day":"2026-10-12","#,
        r#""sum":{"minor":150000,"currency":"USD"},"#,
        r#""rate":{"currency":"USD","per_unit_e4":3619300,"date":"2026-10-09","source":"central_bank"}"#,
        r#"}}"#
    );

    /// Та же запись в версии 5 — так её писала 0.0.17. Выписана человеком:
    /// версия 5 прибавила виды записей, а не поля, и запись отличается
    /// только номером.
    const SAMPLE_V5: &str = concat!(
        r#"{"format_version":5,"record":{"#,
        r#""meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","modified":1760000000000,"deleted":null},"#,
        r#""day":"2026-10-12","#,
        r#""sum":{"minor":150000,"currency":"USD"},"#,
        r#""rate":{"currency":"USD","per_unit_e4":3619300,"date":"2026-10-09","source":"central_bank"}"#,
        r#"}}"#
    );

    /// Та же запись в версии 6 — так её писала 0.0.27. Выписана
    /// человеком: версия 6 прибавила виды записей и поле документу, а у
    /// пробы его нет, и запись отличается только номером.
    const SAMPLE_V6: &str = concat!(
        r#"{"format_version":6,"record":{"#,
        r#""meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","modified":1760000000000,"deleted":null},"#,
        r#""day":"2026-10-12","#,
        r#""sum":{"minor":150000,"currency":"USD"},"#,
        r#""rate":{"currency":"USD","per_unit_e4":3619300,"date":"2026-10-09","source":"central_bank"}"#,
        r#"}}"#
    );

    /// Та же запись в версии 7 — так её писала 0.0.40. Выписана
    /// человеком: версия 7 прибавила поле документу, а у пробы его нет,
    /// и запись отличается только номером.
    const SAMPLE_V7: &str = concat!(
        r#"{"format_version":7,"record":{"#,
        r#""meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","modified":1760000000000,"deleted":null},"#,
        r#""day":"2026-10-12","#,
        r#""sum":{"minor":150000,"currency":"USD"},"#,
        r#""rate":{"currency":"USD","per_unit_e4":3619300,"date":"2026-10-09","source":"central_bank"}"#,
        r#"}}"#
    );

    /// Та же запись в версии 8 — так её писала 0.0.47. Выписана
    /// человеком: версия 8 прибавила вид записей и поля документу,
    /// профилю и книге, а у пробы их нет, и запись отличается только
    /// номером.
    const SAMPLE_V8: &str = concat!(
        r#"{"format_version":8,"record":{"#,
        r#""meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","modified":1760000000000,"deleted":null},"#,
        r#""day":"2026-10-12","#,
        r#""sum":{"minor":150000,"currency":"USD"},"#,
        r#""rate":{"currency":"USD","per_unit_e4":3619300,"date":"2026-10-09","source":"central_bank"}"#,
        r#"}}"#
    );

    /// Та же запись в версии 9 — так её писала 0.0.49. Выписана
    /// человеком: версия 9 прибавила поле профилю, а у пробы его нет,
    /// и запись отличается только номером.
    const SAMPLE_V9: &str = concat!(
        r#"{"format_version":9,"record":{"#,
        r#""meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","modified":1760000000000,"deleted":null},"#,
        r#""day":"2026-10-12","#,
        r#""sum":{"minor":150000,"currency":"USD"},"#,
        r#""rate":{"currency":"USD","per_unit_e4":3619300,"date":"2026-10-09","source":"central_bank"}"#,
        r#"}}"#
    );

    /// Та же запись в версии 10 — так её писала 0.0.55. Выписана
    /// человеком: версия 10 прибавила поле документу, а у пробы его нет,
    /// и запись отличается только номером.
    const SAMPLE_V10: &str = concat!(
        r#"{"format_version":10,"record":{"#,
        r#""meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","modified":1760000000000,"deleted":null},"#,
        r#""day":"2026-10-12","#,
        r#""sum":{"minor":150000,"currency":"USD"},"#,
        r#""rate":{"currency":"USD","per_unit_e4":3619300,"date":"2026-10-09","source":"central_bank"}"#,
        r#"}}"#
    );

    /// Та же запись в версии 11 — так её писала 0.0.60. Выписана
    /// человеком: версия 11 прибавила вид записей, а не поля, и запись
    /// отличается только номером.
    const SAMPLE_V11: &str = concat!(
        r#"{"format_version":11,"record":{"#,
        r#""meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","modified":1760000000000,"deleted":null},"#,
        r#""day":"2026-10-12","#,
        r#""sum":{"minor":150000,"currency":"USD"},"#,
        r#""rate":{"currency":"USD","per_unit_e4":3619300,"date":"2026-10-09","source":"central_bank"}"#,
        r#"}}"#
    );

    /// Та же запись в версии 12 — так её писала 0.0.66. Выписана
    /// человеком: версия 12 прибавила вид записей, а не поля, и запись
    /// отличается только номером.
    const SAMPLE_V12: &str = concat!(
        r#"{"format_version":12,"record":{"#,
        r#""meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","modified":1760000000000,"deleted":null},"#,
        r#""day":"2026-10-12","#,
        r#""sum":{"minor":150000,"currency":"USD"},"#,
        r#""rate":{"currency":"USD","per_unit_e4":3619300,"date":"2026-10-09","source":"central_bank"}"#,
        r#"}}"#
    );

    /// Та же запись в версии 13 — так её писала 0.0.68. Выписана
    /// человеком: версия 13 прибавила поле документу и операции, а не этой
    /// записи, и запись отличается только номером.
    const SAMPLE_V13: &str = concat!(
        r#"{"format_version":13,"record":{"#,
        r#""meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","modified":1760000000000,"deleted":null},"#,
        r#""day":"2026-10-12","#,
        r#""sum":{"minor":150000,"currency":"USD"},"#,
        r#""rate":{"currency":"USD","per_unit_e4":3619300,"date":"2026-10-09","source":"central_bank"}"#,
        r#"}}"#
    );

    /// Та же запись в версии 14 — так её писали 0.0.69–0.0.86.
    const SAMPLE_V14: &str = concat!(
        r#"{"format_version":14,"record":{"#,
        r#""meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","modified":1760000000000,"deleted":null},"#,
        r#""day":"2026-10-12","#,
        r#""sum":{"minor":150000,"currency":"USD"},"#,
        r#""rate":{"currency":"USD","per_unit_e4":3619300,"date":"2026-10-09","source":"central_bank"}"#,
        r#"}}"#
    );

    /// Та же запись в версии 15 — так её писали 0.0.87–0.0.101.
    const SAMPLE_V15: &str = concat!(
        r#"{"format_version":15,"record":{"#,
        r#""meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","modified":1760000000000,"deleted":null},"#,
        r#""day":"2026-10-12","#,
        r#""sum":{"minor":150000,"currency":"USD"},"#,
        r#""rate":{"currency":"USD","per_unit_e4":3619300,"date":"2026-10-09","source":"central_bank"}"#,
        r#"}}"#
    );

    /// Та же запись в версии 16 — так её писала 0.0.102.
    const SAMPLE_V16: &str = concat!(
        r#"{"format_version":16,"record":{"#,
        r#""meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","modified":1760000000000,"deleted":null},"#,
        r#""day":"2026-10-12","#,
        r#""sum":{"minor":150000,"currency":"USD"},"#,
        r#""rate":{"currency":"USD","per_unit_e4":3619300,"date":"2026-10-09","source":"central_bank"}"#,
        r#"}}"#
    );

    /// Та же запись в версии 17 — так её писали 0.0.103–0.0.118.
    const SAMPLE_V17: &str = concat!(
        r#"{"format_version":17,"record":{"#,
        r#""meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","modified":1760000000000,"deleted":null},"#,
        r#""day":"2026-10-12","#,
        r#""sum":{"minor":150000,"currency":"USD"},"#,
        r#""rate":{"currency":"USD","per_unit_e4":3619300,"date":"2026-10-09","source":"central_bank"}"#,
        r#"}}"#
    );

    /// Та же запись в версии 18 — так её писала 0.0.119.
    const SAMPLE_V18: &str = concat!(
        r#"{"format_version":18,"record":{"#,
        r#""meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","modified":1760000000000,"deleted":null},"#,
        r#""day":"2026-10-12","#,
        r#""sum":{"minor":150000,"currency":"USD"},"#,
        r#""rate":{"currency":"USD","per_unit_e4":3619300,"date":"2026-10-09","source":"central_bank"}"#,
        r#"}}"#
    );

    /// Та же запись в версии 19 — так её писала 0.0.120.
    const SAMPLE_V19: &str = concat!(
        r#"{"format_version":19,"record":{"#,
        r#""meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","modified":1760000000000,"deleted":null},"#,
        r#""day":"2026-10-12","#,
        r#""sum":{"minor":150000,"currency":"USD"},"#,
        r#""rate":{"currency":"USD","per_unit_e4":3619300,"date":"2026-10-09","source":"central_bank"}"#,
        r#"}}"#
    );

    /// Та же запись в версии 20 — так её писала 0.0.121.
    const SAMPLE_V20: &str = concat!(
        r#"{"format_version":20,"record":{"#,
        r#""meta":{"id":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","modified":1760000000000,"deleted":null},"#,
        r#""day":"2026-10-12","#,
        r#""sum":{"minor":150000,"currency":"USD"},"#,
        r#""rate":{"currency":"USD","per_unit_e4":3619300,"date":"2026-10-09","source":"central_bank"}"#,
        r#"}}"#
    );

    /// Та же запись в версии 21 — так её пишет эта программа. Выписана
    /// человеком: версия 21 прибавила поле профилю, а не этой записи,
    /// и запись отличается только номером.
    const SAMPLE_V21: &str = concat!(
        r#"{"format_version":21,"record":{"#,
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

    /// Образец текущей версии читается в ожидаемые значения, а запись его
    /// значений — слово в слово образец. Поменялась запись — это новая
    /// версия формата с миграцией (Правило 5), а не правка образца.
    #[test]
    fn sample_v21_is_pinned() {
        let record: Probe = decode(SAMPLE_V21).expect("образец версии 21 читается");
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
        assert_eq!(encode(&record), SAMPLE_V21);
    }

    /// Записи версий 1–20 читаются той же записью, а пишутся уже версией 21:
    /// данные прошлых версий не теряются (Правило 2).
    #[test]
    fn older_samples_still_read() {
        let current: Probe = decode(SAMPLE_V21).expect("образец версии 21 читается");
        let older = [
            (1, SAMPLE_V1),
            (2, SAMPLE_V2),
            (3, SAMPLE_V3),
            (4, SAMPLE_V4),
            (5, SAMPLE_V5),
            (6, SAMPLE_V6),
            (7, SAMPLE_V7),
            (8, SAMPLE_V8),
            (9, SAMPLE_V9),
            (10, SAMPLE_V10),
            (11, SAMPLE_V11),
            (12, SAMPLE_V12),
            (13, SAMPLE_V13),
            (14, SAMPLE_V14),
            (15, SAMPLE_V15),
            (16, SAMPLE_V16),
            (17, SAMPLE_V17),
            (18, SAMPLE_V18),
            (19, SAMPLE_V19),
            (20, SAMPLE_V20),
        ];
        for (version, sample) in older {
            let old: Probe = decode(sample)
                .unwrap_or_else(|error| panic!("образец версии {version}: {error:?}"));
            assert_eq!(old, current, "версия {version}");
            assert_eq!(encode(&old), SAMPLE_V21, "версия {version}");
        }
    }

    /// Запись более новой версии — «новее», даже если её поля незнакомы
    /// или испорчены: версия проверяется раньше полей.
    #[test]
    fn newer_records_are_refused_by_version() {
        let next = FORMAT_VERSION + 1;
        let cases = [
            (
                format!(r#"{{"format_version":{next},"record":{{"meta":"что угодно","wings":2}}}}"#),
                next,
            ),
            (format!(r#"{{"record":null,"format_version":{next}}}"#), next),
            (
                r#"{"format_version":4294967295,"record":{}}"#.to_owned(),
                u32::MAX,
            ),
        ];
        let mut wrong = Vec::new();
        for (text, found) in &cases {
            let got = decode::<Probe>(text);
            if got != Err(FormatError::Newer { found: *found }) {
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
