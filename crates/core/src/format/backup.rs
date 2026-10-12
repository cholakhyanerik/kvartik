//! Файл резервной копии: все записи в одном файле под паролем (задача 7).
//!
//! Браузер может стереть данные сам (Правило 6), а сервера с копией у нас
//! нет: файл копии — единственная страховка и способ перенести данные на
//! другое устройство. Пароль на файле — решение человека 2026-10-12: файл
//! лежит в «Загрузках» и в мессенджерах, а в нём все деньги ИП.
//!
//! # Что в файле
//!
//! Снаружи — JSON: открытая шапка, замок и шифротекст.
//!
//! ```json
//! {"file":"kvartik-backup","format_version":1,"app_version":"0.0.8",
//!  "created":1760270400000,
//!  "kdf":{"name":"PBKDF2-SHA256","iterations":600000,"salt":"…"},
//!  "cipher":{"name":"AES-256-GCM","iv":"…"},
//!  "data":"…"}
//! ```
//!
//! Шапка открыта, чтобы копию новее программы отвергнуть целиком, до
//! вопроса о пароле и до разбора записей (Правило 2): человек услышит
//! «откройте новую версию», а не «пароль не подходит». В `data` — тот же
//! JSON, только с записями вместо замка:
//!
//! ```json
//! {"file":"kvartik-backup","format_version":1,"app_version":"0.0.8",
//!  "created":1760270400000,"records":[{"kind":"client","record":{…}},…]}
//! ```
//!
//! Шапка повторена внутри, и после расшифровки обе сверяются ([`read`]):
//! подлинность того, что внутри, подтверждает шифр, а через неё — и шапки
//! снаружи. Подменить в ней версию или дату незаметно нельзя.
//!
//! Записи внутри — без своего конверта: все они той версии формата, что
//! у файла. Копия собирается из базы разбором каждой записи ([`collect`]),
//! поэтому записи прошлых версий в ней уже переведены в текущую.
//!
//! Что меняет этот файл — меняет версию формата ([`FORMAT_VERSION`]):
//! шапку, замок, имена шифра, вид записи в списке. Прошлые версии читаются
//! по своему номеру; образец копии каждой выпущенной версии лежит
//! в `crates/core/tests/backups/` (Правило 2).
//!
//! # Шифр
//!
//! Шифрует окно — WebCrypto браузера (`crates/app/src/platform/crypto.rs`):
//! он работает вне главного потока и не тянет в wasm своей криптографии.
//! Ядро собирает и читает всё вокруг шифра и задаёт его параметры.
//!
//! # Слияние
//!
//! Копия не заменяет базу, а сливается с ней по идентификатору и моменту
//! изменения записи ([`merge`]): из двух записей с одним идентификатором
//! остаётся изменённая позже, а записи, которых в копии нет, остаются как
//! были. Так восстановление из старой копии не стирает того, что сделано
//! после неё, а метка удаления переезжает вместе с записью (CLAUDE.md,
//! «Записи не стираются») — то же понадобится второму устройству
//! (задача 41).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

use super::{
    FIRST_VERSION, FORMAT_VERSION, FormatError, Stored, base64, decode_record, split, wrap,
};
use crate::model::client::Client;
use crate::model::date::Date;
use crate::model::logo::Logo;
use crate::model::profile::Profile;
use crate::model::record::{RecordMeta, Timestamp};

/// Метка файла копии: по ней свой файл отличается от чужого JSON. Часть
/// формата — не меняется и при смене имени программы (задача 16).
pub const FILE_MARK: &str = "kvartik-backup";

/// Больше этого файл копии не читается — проверяется до чтения
/// (Правило 13): файл в сотни мегабайт повесил бы страницу. Запас большой:
/// год работы занятого ИП — около 2 МБ (600 документов по 1–2 КБ и тысяча
/// операций книги по полкилобайта), 32 МБ — полтора десятка лет. Вложения
/// (задача 14) и Pro (задача 38) этот предел пересмотрят.
pub const MAX_FILE_BYTES: u64 = 32 << 20;

/// Как из пароля получается ключ: PBKDF2 с HMAC-SHA-256.
pub const KDF_NAME: &str = "PBKDF2-SHA256";

/// Итераций PBKDF2 в каждой новой копии — столько советует OWASP для
/// PBKDF2-HMAC-SHA256: «600,000 iterations (recommended)» (Password Storage
/// Cheat Sheet, правка 2023-01-23; сверено по тексту от 2026-10-04).
/// Перебор пароля к украденному файлу идёт без нас и без счёта попыток,
/// и цену одной попытки задаёт только это число. Прочитанный файл несёт
/// своё число, и поднять его можно без новой версии формата.
pub const KDF_ITERATIONS: u32 = 600_000;

/// Больше итераций файл нести не может: иначе чужой файл с миллиардом
/// итераций занял бы устройство на часы. Предел — 10 000 000, верх, который
/// NIST SP 800-132 (§5.2) называет «для особо важных ключей».
const MAX_KDF_ITERATIONS: u32 = 10_000_000;

/// Соль PBKDF2: 16 случайных байтов у каждого пароля — «не меньше 128 бит»
/// (NIST SP 800-132, §5.1).
pub const SALT_BYTES: usize = 16;

/// Соль длиннее читается (её длину выбирает писавший), но не без предела.
const MAX_SALT_BYTES: usize = 64;

/// Шифр: AES-256 в режиме GCM, метка подлинности — 128 бит.
pub const CIPHER_NAME: &str = "AES-256-GCM";

/// Вектор GCM — 12 случайных байтов у каждого файла. Соль у пароля
/// устройства одна, и ключ у его копий общий, поэтому вектор обязан быть
/// новым у каждого файла: со случайным 96-битным вектором один ключ годится
/// на 2³² шифрований (NIST SP 800-38D, §8.3) — это копия каждую минуту
/// восемь тысяч лет.
pub const IV_BYTES: usize = 12;

/// Метка подлинности GCM в конце шифротекста.
const TAG_BYTES: usize = 16;

/// Проверочное значение пароля на устройстве ([`DeviceKey`]) — SHA-256.
pub const CHECK_BYTES: usize = 32;

/// Пароль копии — не короче стольких знаков: столько требует NIST
/// SP 800-63B-4 (§3.1.1.2, 2025-07-31) от пароля, который один защищает
/// вход. Файл копии защищён хуже входа: он лежит где угодно, и перебор
/// идёт без счёта попыток, — а SP 800-132 (приложение A.1) называет слабыми
/// фразы короче 20 знаков. Поэтому окно советует фразу из нескольких слов,
/// а не сложные знаки. Знак — точка кода Unicode, как там же.
pub const MIN_PASSWORD_CHARS: usize = 15;

/// Через столько дней без копии окно напоминает о ней при каждом открытии
/// (Правило 2).
pub const REMIND_AFTER_DAYS: i32 = 7;

/// Вид записи, который копия знает: имя и разбор.
#[derive(Clone, Copy)]
pub struct Kind {
    name: &'static str,
    check: fn(u32, &str) -> Result<Checked, FormatError>,
}

impl Kind {
    /// Вид записей типа `T`.
    pub const fn of<T: Stored>() -> Self {
        Self {
            name: T::KIND,
            check: check::<T>,
        }
    }

    /// Имя вида — `Stored::KIND`.
    pub const fn name(self) -> &'static str {
        self.name
    }
}

/// Все виды записей программы — то, что копия забирает из базы и кладёт
/// обратно.
///
/// **Новый тип записи (`impl Stored`) — новой строкой здесь**, в той же
/// задаче. Забытый вид не пропадёт молча: копия посчитает его записи
/// нечитаемыми и скажет об этом словами, а браузерный тест копии с ним
/// упадёт. Новый вид — это ещё и новая версия формата (шапка
/// `format/mod.rs`), и образец копии с его записью (Правило 2).
pub const KINDS: &[Kind] = &[
    Kind::of::<Profile>(),
    Kind::of::<Logo>(),
    Kind::of::<Client>(),
];

fn find(kinds: &[Kind], name: &str) -> Option<Kind> {
    kinds.iter().copied().find(|kind| kind.name == name)
}

/// Запись после разбора своим видом.
struct Checked {
    meta: RecordMeta,
    record: Box<RawValue>,
}

/// Разбор записи вида `T` той версии формата, которой она сделана, и запись
/// её обратно — уже в текущей.
fn check<T: Stored>(version: u32, record: &str) -> Result<Checked, FormatError> {
    let record: T = decode_record(version, record)?;
    let json = serde_json::value::to_raw_value(&record).expect(
        "запись в JSON не падает: в типах записей нет ни словарей с ключами-структурами, ни своих ошибок",
    );
    Ok(Checked {
        meta: record.meta().clone(),
        record: json,
    })
}

/// Запись копии: разобрана своим видом и записана в текущей версии формата.
#[derive(Clone, Debug)]
pub struct Entry {
    kind: &'static str,
    meta: RecordMeta,
    record: Box<RawValue>,
}

impl Entry {
    /// Вид записи.
    pub const fn kind(&self) -> &'static str {
        self.kind
    }

    /// Общие поля записи.
    pub const fn meta(&self) -> &RecordMeta {
        &self.meta
    }

    /// Строка для базы — та же, что дал бы `format::encode`.
    pub fn stored(&self) -> String {
        wrap(self.record.get())
    }
}

/// Запись базы как она лежит: вид и идентификатор — из ключа, текст — из
/// значения; `None` — значение не строка.
#[derive(Clone, Copy, Debug)]
pub struct Raw<'a> {
    pub kind: &'a str,
    pub id: &'a str,
    pub text: Option<&'a str>,
}

/// Запись базы, разобранная своим видом и проверенная по ключу.
fn read_stored(kinds: &[Kind], raw: Raw<'_>) -> Result<Entry, FormatError> {
    let text = raw
        .text
        .ok_or_else(|| FormatError::Damaged("значение не строка".to_owned()))?;
    // Версия — раньше вида: незнакомый вид новее программы — это запись
    // новой версии, а не порча.
    let (version, record) = split(text)?;
    let kind = find(kinds, raw.kind)
        .ok_or_else(|| FormatError::Damaged(format!("вид «{}» программе незнаком", raw.kind)))?;
    let checked = (kind.check)(version, record)?;
    if checked.meta.id().to_string() != raw.id {
        return Err(FormatError::Damaged(
            "запись лежит не под своим идентификатором".to_owned(),
        ));
    }
    Ok(Entry {
        kind: kind.name,
        meta: checked.meta,
        record: checked.record,
    })
}

/// Записи базы, собранные для копии.
#[derive(Debug)]
pub struct Collected {
    entries: Vec<Entry>,
    unreadable: Vec<String>,
}

impl Collected {
    /// Записи, которые войдут в копию.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Что не вошло: по строке на нечитаемую запись — вид, идентификатор
    /// и причина, для консоли разработчика. Человеку окно говорит число.
    pub fn unreadable(&self) -> &[String] {
        &self.unreadable
    }
}

/// Собрать копию из всех записей базы.
///
/// Каждая запись разбирается своим видом, как при чтении, и ложится в копию
/// в текущей версии формата: копия, которую нельзя прочесть, хуже никакой —
/// она обещает то, чего не вернёт.
///
/// Запись новее программы — отказ целиком ([`FormatError::Newer`]): её
/// пишет более новая версия в другой вкладке, и копию сделает она. Нечитаемая
/// запись копию не останавливает — без копии остальные остались бы без
/// страховки, — а пропускается и называется в [`Collected::unreadable`].
pub fn collect<'a>(
    kinds: &[Kind],
    raw: impl IntoIterator<Item = Raw<'a>>,
) -> Result<Collected, FormatError> {
    let mut entries = Vec::new();
    let mut unreadable = Vec::new();
    for item in raw {
        match read_stored(kinds, item) {
            Ok(entry) => entries.push(entry),
            Err(FormatError::Newer { found }) => return Err(FormatError::Newer { found }),
            Err(FormatError::Damaged(why)) => {
                unreadable.push(format!("{} {}: {why}", item.kind, item.id));
            }
        }
    }
    Ok(Collected {
        entries,
        unreadable,
    })
}

/// Шапка копии: какой версией программы и когда сделана.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Header {
    format_version: u32,
    app_version: String,
    created: Timestamp,
}

impl Header {
    /// Шапка новой копии: версия формата — этой программы.
    pub fn new(app_version: &str, created: Timestamp) -> Self {
        Self {
            format_version: FORMAT_VERSION,
            app_version: app_version.to_owned(),
            created,
        }
    }

    /// Версия формата копии.
    pub const fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Версия программы, которая сделала копию: `0.0.8`.
    pub fn app_version(&self) -> &str {
        &self.app_version
    }

    /// Когда копия сделана — по часам устройства, которое её сделало.
    pub const fn created(&self) -> Timestamp {
        self.created
    }
}

/// Открытый текст копии — то, что окно шифрует.
#[derive(Serialize)]
struct PlainOut<'a> {
    file: &'static str,
    format_version: u32,
    app_version: &'a str,
    created: Timestamp,
    records: Vec<RecordOut<'a>>,
}

#[derive(Serialize)]
struct RecordOut<'a> {
    kind: &'static str,
    record: &'a RawValue,
}

/// Открытый текст копии: шапка и записи.
pub fn plaintext(header: &Header, entries: &[Entry]) -> String {
    serde_json::to_string(&PlainOut {
        file: FILE_MARK,
        format_version: header.format_version,
        app_version: &header.app_version,
        created: header.created,
        records: entries
            .iter()
            .map(|entry| RecordOut {
                kind: entry.kind,
                record: &entry.record,
            })
            .collect(),
    })
    .expect("копия в JSON не падает: записи уже в JSON, остальное — строки и числа")
}

/// Замок одного файла: из чего ключ получается из пароля и вектор шифра.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Lock {
    iterations: u32,
    salt: Vec<u8>,
    iv: Vec<u8>,
}

impl Lock {
    /// Замок, если параметры годятся: итераций от одной до предела, соль —
    /// от [`SALT_BYTES`] до 64 байтов, вектор — ровно [`IV_BYTES`].
    pub fn new(iterations: u32, salt: Vec<u8>, iv: Vec<u8>) -> Option<Self> {
        let fits = (1..=MAX_KDF_ITERATIONS).contains(&iterations)
            && (SALT_BYTES..=MAX_SALT_BYTES).contains(&salt.len())
            && iv.len() == IV_BYTES;
        fits.then_some(Self {
            iterations,
            salt,
            iv,
        })
    }

    /// Итераций PBKDF2.
    pub const fn iterations(&self) -> u32 {
        self.iterations
    }

    /// Соль PBKDF2.
    pub fn salt(&self) -> &[u8] {
        &self.salt
    }

    /// Вектор AES-GCM.
    pub fn iv(&self) -> &[u8] {
        &self.iv
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct KdfFields {
    name: String,
    iterations: u32,
    salt: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CipherFields {
    name: String,
    iv: String,
}

#[derive(Serialize)]
struct SealedOut<'a> {
    file: &'static str,
    format_version: u32,
    app_version: &'a str,
    created: Timestamp,
    kdf: KdfFields,
    cipher: CipherFields,
    data: String,
}

/// Файл копии: шапка, замок и шифротекст.
pub fn seal(header: &Header, lock: &Lock, ciphertext: &[u8]) -> String {
    serde_json::to_string(&SealedOut {
        file: FILE_MARK,
        format_version: header.format_version,
        app_version: &header.app_version,
        created: header.created,
        kdf: KdfFields {
            name: KDF_NAME.to_owned(),
            iterations: lock.iterations,
            salt: base64::encode(&lock.salt),
        },
        cipher: CipherFields {
            name: CIPHER_NAME.to_owned(),
            iv: base64::encode(&lock.iv),
        },
        data: base64::encode(ciphertext),
    })
    .expect("файл копии в JSON не падает: в нём только строки и числа")
}

/// Почему файл копии не открыт.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum BackupError {
    /// Это не файл копии программы: другой файл или не JSON вовсе.
    NotBackup,
    /// Копия сделана более новой версией программы: её формат — `found`.
    /// Читать её нельзя — незнакомое старая версия выбросила бы молча
    /// (Правило 2).
    Newer { found: u32 },
    /// Файл копии наш, но испорчен: обрезан при скачивании, правлен руками.
    /// В строке — подробность для консоли разработчика.
    Damaged(String),
}

impl From<FormatError> for BackupError {
    fn from(error: FormatError) -> Self {
        match error {
            FormatError::Newer { found } => Self::Newer { found },
            FormatError::Damaged(why) => Self::Damaged(why),
        }
    }
}

fn damaged(why: impl ToString) -> BackupError {
    BackupError::Damaged(why.to_string())
}

/// Файл копии до расшифровки: шапка и замок проверены.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Sealed {
    header: Header,
    lock: Lock,
    ciphertext: Vec<u8>,
}

impl Sealed {
    /// Шапка копии.
    pub const fn header(&self) -> &Header {
        &self.header
    }

    /// Замок: из чего получить ключ.
    pub const fn lock(&self) -> &Lock {
        &self.lock
    }

    /// Шифротекст вместе с меткой подлинности.
    pub fn ciphertext(&self) -> &[u8] {
        &self.ciphertext
    }
}

/// Только метка и версия: остальное разбор пропускает.
#[derive(Deserialize)]
struct Mark {
    file: String,
    format_version: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SealedIn {
    #[serde(rename = "file")]
    _file: String,
    format_version: u32,
    app_version: String,
    created: Timestamp,
    kdf: KdfFields,
    cipher: CipherFields,
    data: String,
}

/// Открыть файл копии: проверить шапку и замок, достать шифротекст.
///
/// Сначала — чей файл и какой версии, и только потом остальные поля: копия
/// новее программы с незнакомыми полями иначе выглядела бы испорченной.
/// Свой файл программа узнаёт и обрезанным: он всегда начинается с метки.
pub fn open(text: &str) -> Result<Sealed, BackupError> {
    let mark = match serde_json::from_str::<Mark>(text) {
        Ok(mark) => mark,
        Err(_) if text.starts_with(MARK_PREFIX) => {
            return Err(damaged("шапка файла копии не читается"));
        }
        Err(_) => return Err(BackupError::NotBackup),
    };
    if mark.file != FILE_MARK {
        return Err(BackupError::NotBackup);
    }
    match mark.format_version {
        // Шапка, замок и список записей у версий 1 и 2 одинаковы: версия 2
        // прибавила виды записей, а их разбирает `decode_record` по версии
        // файла.
        FIRST_VERSION..=FORMAT_VERSION => {}
        found if found > FORMAT_VERSION => return Err(BackupError::Newer { found }),
        // Версий до первой нет.
        found => return Err(damaged(format!("версия формата {found}: такой не бывает"))),
    }
    let fields: SealedIn = serde_json::from_str(text).map_err(damaged)?;
    if fields.kdf.name != KDF_NAME || fields.cipher.name != CIPHER_NAME {
        return Err(damaged(format!(
            "шифр «{}» и «{}» программе незнаком",
            fields.kdf.name, fields.cipher.name
        )));
    }
    if !is_app_version(&fields.app_version) {
        return Err(damaged("версия программы в шапке — не номер версии"));
    }
    let salt = base64::decode(&fields.kdf.salt).ok_or_else(|| damaged("соль — не base64"))?;
    let iv = base64::decode(&fields.cipher.iv).ok_or_else(|| damaged("вектор — не base64"))?;
    let lock = Lock::new(fields.kdf.iterations, salt, iv)
        .ok_or_else(|| damaged("параметры шифра за пределами допустимого"))?;
    let ciphertext = base64::decode(&fields.data).ok_or_else(|| damaged("данные — не base64"))?;
    if ciphertext.len() < TAG_BYTES {
        return Err(damaged("данные короче метки подлинности"));
    }
    Ok(Sealed {
        header: Header {
            format_version: fields.format_version,
            app_version: fields.app_version,
            created: fields.created,
        },
        lock,
        ciphertext,
    })
}

/// Начало каждого файла копии: так его пишет [`seal`].
const MARK_PREFIX: &str = r#"{"file":"kvartik-backup""#;

/// Похоже ли на номер версии программы: `0.0.8`, `1.2.0-beta.1`. Шапку
/// окно показывает человеку, и в ней не должно быть чего угодно.
fn is_app_version(text: &str) -> bool {
    (1..=32).contains(&text.len())
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'+'))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PlainIn<'a> {
    file: String,
    format_version: u32,
    app_version: String,
    created: Timestamp,
    #[serde(borrow)]
    records: Vec<RecordIn<'a>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RecordIn<'a> {
    kind: String,
    #[serde(borrow)]
    record: &'a RawValue,
}

/// Записи расшифрованной копии — каждая разобрана своим видом.
///
/// Всё или ничего: одна испорченная запись, вид, которого программа не
/// знает, или две записи с одним идентификатором — и не читается вся копия.
/// Половина копии в базе выглядела бы восстановленной, а не была бы ею.
pub fn read(kinds: &[Kind], sealed: &Sealed, plaintext: &[u8]) -> Result<Vec<Entry>, BackupError> {
    let text = std::str::from_utf8(plaintext).map_err(damaged)?;
    let plain: PlainIn<'_> = serde_json::from_str(text).map_err(damaged)?;
    let inner = Header {
        format_version: plain.format_version,
        app_version: plain.app_version,
        created: plain.created,
    };
    if plain.file != FILE_MARK || inner != sealed.header {
        return Err(damaged("шапка снаружи не совпадает с шапкой внутри"));
    }
    let mut seen = BTreeSet::new();
    let mut entries = Vec::with_capacity(plain.records.len());
    for item in plain.records {
        let kind = find(kinds, &item.kind)
            .ok_or_else(|| damaged(format!("вид «{}» программе незнаком", item.kind)))?;
        let checked = (kind.check)(inner.format_version, item.record.get())?;
        if !seen.insert((kind.name, checked.meta.id())) {
            return Err(damaged(format!(
                "запись {} {} в копии дважды",
                kind.name,
                checked.meta.id()
            )));
        }
        entries.push(Entry {
            kind: kind.name,
            meta: checked.meta,
            record: checked.record,
        });
    }
    Ok(entries)
}

/// Что слияние копии с базой запишет.
#[derive(Debug)]
pub struct Merge<'a> {
    writes: Vec<&'a Entry>,
    added: usize,
    updated: usize,
    unchanged: usize,
}

impl<'a> Merge<'a> {
    /// Записи, которые лягут в базу.
    pub fn writes(&self) -> &[&'a Entry] {
        &self.writes
    }

    /// Сколько записей копии в базе не было.
    pub const fn added(&self) -> usize {
        self.added
    }

    /// Сколько записей копия заменит: в копии они изменены позже, чем
    /// в базе, или в базе не читаются.
    pub const fn updated(&self) -> usize {
        self.updated
    }

    /// Сколько записей копии в базе уже есть такими же или изменены позже.
    pub const fn unchanged(&self) -> usize {
        self.unchanged
    }
}

/// Слить записи копии с тем, что в базе.
///
/// Из двух записей с одним видом и идентификатором остаётся изменённая
/// позже (`RecordMeta::modified`); при равенстве — та, что в базе.
/// Нечитаемую запись базы копия чинит: «восстановите из резервной копии» —
/// ровно то, что окно советует при `StoreError::Damaged`. Запись базы новее
/// программы — отказ целиком: её переписала бы старая версия (Правило 2).
pub fn merge<'a, 'b>(
    kinds: &[Kind],
    incoming: &'a [Entry],
    existing: impl IntoIterator<Item = Raw<'b>>,
) -> Result<Merge<'a>, FormatError> {
    // Вставкой, а не `collect`: `collect` в `BTreeMap` сначала сортирует,
    // и сортировка прибавляла wasm 4 КБ кода (замер задачи 7), а база и так
    // отдаёт записи по порядку ключей.
    let mut by_key = BTreeMap::new();
    for raw in existing {
        by_key.insert((raw.kind, raw.id), raw);
    }
    let existing = by_key;
    let mut merge = Merge {
        writes: Vec::new(),
        added: 0,
        updated: 0,
        unchanged: 0,
    };
    for entry in incoming {
        let id = entry.meta.id().to_string();
        let Some(&raw) = existing.get(&(entry.kind, id.as_str())) else {
            merge.added += 1;
            merge.writes.push(entry);
            continue;
        };
        let newer = match read_stored(kinds, raw) {
            Ok(stored) => entry.meta.modified() > stored.meta.modified(),
            Err(FormatError::Newer { found }) => return Err(FormatError::Newer { found }),
            Err(FormatError::Damaged(_)) => true,
        };
        if newer {
            merge.updated += 1;
            merge.writes.push(entry);
        } else {
            merge.unchanged += 1;
        }
    }
    Ok(merge)
}

/// Имя файла копии: `kvartik-2026-10-12.json`. Дата — сегодняшняя по
/// календарю устройства: по ней человек находит нужную копию в папке.
pub fn file_name(today: Date) -> String {
    format!(
        "kvartik-{:04}-{:02}-{:02}.json",
        today.year(),
        today.month(),
        today.day()
    )
}

/// Пора ли напомнить о копии: данные есть, а копии нет или она сделана
/// [`REMIND_AFTER_DAYS`] дней назад и раньше.
///
/// Без данных не напоминает: терять нечего, а новичку напоминание о копии
/// пустой программы непонятно. Копия «из будущего» (часы устройства
/// переводили) — не повод напоминать: это часы, а не копия (Правило 6).
pub fn reminder_due(last_backup: Option<Date>, today: Date, has_data: bool) -> bool {
    has_data && last_backup.is_none_or(|last| today.days_since(last) >= REMIND_AFTER_DAYS)
}

/// Что запомнить устройству как момент последней копии после
/// восстановления из копии, сделанной в `created`; `None` — оставить как
/// есть.
///
/// Восстановленные данные лежат в файле, из которого их взяли: копия на
/// момент `created` у человека есть. Старая копия напоминание и вызовет —
/// и правильно: всё сделанное после неё не застраховано. Позже прежней
/// отметки момент не уменьшает, а момент «из будущего» не берёт: иначе
/// чужой файл с датой через сто лет выключил бы напоминание навсегда
/// (Правило 13).
pub fn last_after_restore(
    current: Option<Timestamp>,
    created: Timestamp,
    now: Timestamp,
) -> Option<Timestamp> {
    (created <= now && current.is_none_or(|last| last < created)).then_some(created)
}

/// Что не так с новым паролем копий.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PasswordProblem {
    /// Короче [`MIN_PASSWORD_CHARS`] знаков.
    TooShort,
    /// Повтор не совпал с паролем: с опечаткой в пароле копия не откроется
    /// никогда.
    Mismatch,
}

/// Годится ли новый пароль копий; `None` — годится. Длина — в знаках,
/// а не в байтах: армянская буква — два байта UTF-8, но один знак.
pub fn new_password_problem(password: &str, repeat: &str) -> Option<PasswordProblem> {
    if password.chars().count() < MIN_PASSWORD_CHARS {
        Some(PasswordProblem::TooShort)
    } else if password != repeat {
        Some(PasswordProblem::Mismatch)
    } else {
        None
    }
}

/// Пароль копий на этом устройстве: соль, итерации и проверочное значение —
/// SHA-256 ключа, полученного из пароля.
///
/// Пароль спрашивается при каждой копии — так его не забывают, — а по
/// проверочному значению окно ловит опечатку, не спрашивая пароль дважды:
/// копия под паролем с опечаткой не открылась бы никогда. Сам пароль нигде
/// не хранится. Проверочное значение ничего не открывает тому, у кого есть
/// устройство: данные на нём и так лежат открыто, а перебор пароля по нему
/// стоит столько же, сколько по файлу копии.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DeviceKey {
    iterations: u32,
    salt: Vec<u8>,
    check: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeviceKeyFields {
    iterations: u32,
    salt: String,
    check: String,
}

impl DeviceKey {
    /// Пароль устройства, если параметры годятся — те же пределы, что
    /// у [`Lock`], и проверочное значение длиной [`CHECK_BYTES`].
    pub fn new(iterations: u32, salt: Vec<u8>, check: Vec<u8>) -> Option<Self> {
        let fits = (1..=MAX_KDF_ITERATIONS).contains(&iterations)
            && (SALT_BYTES..=MAX_SALT_BYTES).contains(&salt.len())
            && check.len() == CHECK_BYTES;
        fits.then_some(Self {
            iterations,
            salt,
            check,
        })
    }

    /// Итераций PBKDF2.
    pub const fn iterations(&self) -> u32 {
        self.iterations
    }

    /// Соль PBKDF2.
    pub fn salt(&self) -> &[u8] {
        &self.salt
    }

    /// Подходит ли пароль: ключ из него дал то же проверочное значение.
    pub fn accepts(&self, check: &[u8]) -> bool {
        self.check == check
    }

    /// Запись для хранилища устройства.
    pub fn to_text(&self) -> String {
        serde_json::to_string(&DeviceKeyFields {
            iterations: self.iterations,
            salt: base64::encode(&self.salt),
            check: base64::encode(&self.check),
        })
        .expect("в записи только строки и числа")
    }

    /// Из записи хранилища устройства; `None` — запись не читается, и пароль
    /// придётся задать заново.
    pub fn from_text(text: &str) -> Option<Self> {
        let fields: DeviceKeyFields = serde_json::from_str(text).ok()?;
        Self::new(
            fields.iterations,
            base64::decode(&fields.salt)?,
            base64::decode(&fields.check)?,
        )
    }
}

#[cfg(test)]
mod tests;
