//! Код передачи изменений на другое устройство (задача 110): без сервера
//! и без файла — одно устройство показывает QR-коды или текст, другое
//! снимает их камерой или вставляет текстом, и записи ложатся тем же
//! слиянием, что копия ([`backup::merge`]), с теми же правилами: позже по
//! часам своего устройства, выпущенный документ — против черновика.
//!
//! # Что передаётся
//!
//! Не вся база, а записи, изменённые с выбранного дня ([`pick`]): по моменту
//! изменения (`RecordMeta::modified`), с метками удаления — удалённое на
//! одном устройстве удаляется и на другом. День — с начала его в самом
//! раннем поясе Земли, UTC+14 ([`since_day`]): правка этого дня по часам
//! любого пояса в код попадёт. По умолчанию — день прошлого кода с этого
//! устройства или день записей постарше, легших сюда с тех пор копией или
//! кодом, а первый код — все записи ([`default_day`]). «Все записи» — без
//! дня.
//!
//! Файлы книги, логотип профиля и логотипы выпущенных документов в код не
//! идут ([`SKIPPED`]): это сотни килобайт картинок, сотни QR-кодов, а их
//! переносит копия файлом. Сколько таких изменилось, код говорит числом,
//! и окно называет его словами. Запись книги без своего файла на другом
//! устройстве показывает файл без ссылки — так же, как нечитаемый.
//!
//! Живой профиль идёт в код всегда, даже не изменённый: по нему принимающее
//! устройство узнаёт, того ли ИП изменения ([`backup::relation`]), — у кода
//! без профиля сверять было бы не с чем, и изменения другого ИП легли бы
//! рядом молча.
//!
//! # Под паролем
//!
//! Как файл копии — паролем копий, PBKDF2 и AES-GCM WebCrypto (CLAUDE.md,
//! Правило 13): текст кода идёт через мессенджер, а QR-код снимет любая
//! камера рядом. Снаружи — только метка, версия формата и замок ([`seal`]):
//! код новее программы узнаётся до пароля. Внутри — шапка и записи, сжатые
//! DEFLATE уровнем 9: код мал, а каждый байт — лишний QR-кадр.
//!
//! # Кадры и текст
//!
//! Код режется на кадры ([`frames`]) с меткой передачи, номером и числом
//! кадров: QR-код версии [`QR_VERSION`] вмещает [`QR_CHUNK`] байтов, текст —
//! кусок в [`TEXT_CHUNK`] байтов, записанный буквами и цифрами ([`base62`])
//! с меткой [`TEXT_PREFIX`]: мессенджеры не трогают такой текст разметкой
//! (`__курсив__` Telegram съел бы подчёркивания base64), а двойное нажатие
//! выделяет его целиком. Кусок — меньше 4 096 знаков сообщения Telegram
//! (Bot API, `sendMessage`: «1-4096 characters after entities parsing»,
//! сверено 2026-11-21). Собирает кадры [`Collector`] — в любом порядке и
//! с повторами: QR-коды идут по кругу, и пропущенный придёт на следующем.
//!
//! # Помнится на устройствах
//!
//! Каждое устройство — со своим идентификатором. Принимающее помнит, до
//! какого момента часов отправителя от него принято ([`Received`]), и код,
//! который начинается позже, называет: изменения между ними не пришли —
//! прошлый код не сняли ([`continuity`]). Сервера, который знал бы, кто что
//! принял, нет, и обратного пути у QR-кода нет: отправитель не знает,
//! снят ли его код, — поэтому дыру видит принимающий.

use std::future::Future;

use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

use super::backup::{self, BackupError, Entry, KINDS, Lock, Meter, Raw, at_once, no_pause};
use super::qr::{self, Ecc};
use super::{FIRST_VERSION, FORMAT_VERSION, FormatError, Stored, decode_record, split_raw};
use crate::model::attachment::Attachment;
use crate::model::date::Date;
use crate::model::logo::{IssuedLogo, Logo};
use crate::model::profile::Profile;
use crate::model::record::{RecordId, RecordMeta, Timestamp};

/// Виды записей, которые в код не идут: файлы книги и картинки логотипов.
pub const SKIPPED: [&str; 3] = [Attachment::KIND, Logo::KIND, IssuedLogo::KIND];

/// Метка кода: первые три байта и версия конверта.
const MARK: &[u8; 3] = b"KVS";

/// Версия конверта кода — не версия формата записей: растёт, когда меняется
/// сам конверт или шапка внутри. Код новее программа не читает и говорит
/// «обновите программу», а не «код испорчен».
const ENVELOPE: u8 = 1;

/// Метка открытого текста кода — как `kvartik-backup` у файла копии.
const PLAIN_MARK: &str = "kvartik-sync";

/// Больше этого код не делается: 32 КиБ — около 120 QR-кодов по кругу
/// (минута с лишним на круг) и 12 кусков текста. Больше изменений — копией
/// файлом: она и быстрее, и без предела.
pub const MAX_CODE_BYTES: usize = 32 << 10;

/// Больше этого открытый текст кода не собирается и не разжимается: текст
/// записей сжимается в 5–10 раз, и мегабайт — заведомо больше
/// [`MAX_CODE_BYTES`] сжатым, а разжатие с пределом не даст подложенному
/// коду в килобайты закрыть страницу гигабайтами нулей (Правило 13).
const MAX_PLAIN_BYTES: usize = 1 << 20;

/// Уровень сжатия кода: самый плотный. Текст — сотни килобайт самое большее
/// ([`MAX_PLAIN_BYTES`]), и сжимать его дольше дешевле, чем показывать лишние
/// QR-коды.
const CODE_LEVEL: u8 = 9;

/// Версия QR-кода передачи: 65×65 модулей. На экране телефона в 360 точек
/// это 4 точки CSS на модуль, а в кадре камеры ноутбука с полуметра — три-
/// четыре пикселя: меньше версия — больше кодов в круге, больше — их не
/// разберёт камера. Читает сканер ядра любую версию (`format::qr`).
pub const QR_VERSION: u8 = 12;

/// Уровень исправления QR-кода передачи: M, 15 % — блик и муар экрана под
/// камерой портят модули, а L (7 %) дал бы лишь на четверть больше байтов.
pub const QR_ECC: Ecc = Ecc::Medium;

/// Байтов шапки кадра: метка, версия, метка передачи, номер и число кадров.
const FRAME_HEADER: usize = 12;

/// Байтов кода в одном QR-кадре.
pub const QR_CHUNK: usize = qr::capacity(QR_VERSION, QR_ECC) - FRAME_HEADER;

/// Байтов кода в одном куске текста: с меткой и шапкой — 3 874 знака,
/// и ещё строка подписи помещаются в сообщение Telegram в 4 096.
pub const TEXT_CHUNK: usize = 2_800;

/// Метка куска текста — по ней кусок находится в любом вставленном тексте.
pub const TEXT_PREFIX: &str = "KVARTIK";

/// Метка кадра и версия кадра. Метка — три байта, а не один: посторонний
/// QR-код с текстом «Kvartik» иначе читался бы кадром новой версии, и приём
/// обрывался бы словами «обновите программу» (ревью задачи 110).
const FRAME_MARK: &[u8; 3] = b"KVF";
const FRAME_VERSION: u8 = 1;

/// Самое длинное слово куска текста: метка и кадр с самым большим куском,
/// буквами и цифрами. Длиннее — не кусок, и разбирать его незачем:
/// вставленный мегабайт иначе декодировался бы целиком на каждом вводе.
const MAX_TEXT_WORD: usize = TEXT_PREFIX.len() + (FRAME_HEADER + TEXT_CHUNK).div_ceil(8) * 11;

/// Больше кадров у кода не бывает: [`MAX_CODE_BYTES`] самыми мелкими кадрами.
const MAX_FRAMES: usize = MAX_CODE_BYTES.div_ceil(QR_CHUNK);

/// Сколько отправителей помнит принимающее устройство: у человека их
/// два-три, а запись не должна расти без предела (Правило 1).
pub const MAX_PEERS: usize = 8;

/// Сдвиг пояса UTC+14 — самого раннего на Земле (острова Лайн, Кирибати).
const EARLIEST_ZONE_MILLIS: i64 = 14 * 60 * 60 * 1000;

/// Сдвиг пояса UTC−12 — самого позднего на Земле (острова Бейкер и Хауленд).
const LATEST_ZONE_MILLIS: i64 = 12 * 60 * 60 * 1000;

/// Миллисекунд в сутках.
const DAY_MILLIS: i64 = 24 * 60 * 60 * 1000;

/// Начало дня `day` в самом раннем поясе — момент, раньше которого этот
/// день не начинается нигде: правка этого дня по часам любого пояса не
/// раньше него.
pub fn since_day(day: Date) -> Timestamp {
    let start = i64::from(day.day_number()) * DAY_MILLIS - EARLIEST_ZONE_MILLIS;
    Timestamp::from_unix_millis(start)
}

/// День, начало которого — `at` в самом раннем поясе: обратное
/// [`since_day`], для слов «изменения с …» на принимающем устройстве.
pub fn day_of_since(at: Timestamp) -> Option<Date> {
    let days = (at.unix_millis() + EARLIEST_ZONE_MILLIS).div_euclid(DAY_MILLIS);
    Date::from_day_number(i32::try_from(days).ok()?)
}

/// День, код с которого заведомо покрывает момент `got` часов отправителя:
/// день `got` в самом позднем поясе, UTC−12. Начало такого дня в самом
/// раннем поясе ([`since_day`]) — не позже `got` в любом поясе, где живёт
/// отправитель, — его и советуют слова о дыре.
pub fn day_covering(got: Timestamp) -> Option<Date> {
    let days = (got.unix_millis() - LATEST_ZONE_MILLIS).div_euclid(DAY_MILLIS);
    Date::from_day_number(i32::try_from(days).ok()?)
}

/// День, с которого передавать по умолчанию: прошлого кода с этого
/// устройства, а если с тех пор сюда легли копией или кодом записи старше
/// него, — их день (`arrived`, ревью задачи 110: момент у записи — её
/// устройства, и без этого запись с третьего устройства не дошла бы до
/// другого); не позже сегодня — часы, убежавшие вперёд, иначе спрятали бы
/// сегодняшние правки. Прошлого кода нет — `None`: «все записи». День
/// последней копии здесь не годится: автокопия сдвигает его каждый день,
/// а второе устройство начато копией, может быть, неделю назад.
pub fn default_day(last_code: Option<Date>, arrived: Option<Date>, today: Date) -> Option<Date> {
    let day = last_code?;
    Some(arrived.map_or(day, |arrived| arrived.min(day)).min(today))
}

/// Какие записи пойдут в код.
#[derive(Debug)]
pub struct Picked {
    entries: Vec<Entry>,
    skipped: usize,
    unreadable: usize,
}

impl Picked {
    /// Записи кода.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Сколько изменённых файлов книги и логотипов в код не вошло.
    pub const fn skipped(&self) -> usize {
        self.skipped
    }

    /// Сколько записей не читается и в код не вошло.
    pub const fn unreadable(&self) -> usize {
        self.unreadable
    }

    /// Изменённых записей в коде — без живого профиля, который идёт всегда.
    pub fn changed(&self, since: Option<Timestamp>) -> usize {
        self.entries
            .iter()
            .filter(|entry| since.is_none_or(|since| entry.meta().modified() >= since))
            .count()
    }
}

/// Только общие поля — без разбора остального: файл книги — сотни
/// килобайт base64, и перекодировать его, чтобы узнать момент изменения,
/// незачем.
#[derive(Deserialize)]
struct MetaOnly {
    meta: RecordMeta,
}

/// Момент изменения записи из её текста, не разбирая остального; `None` —
/// не читается так.
fn peek(text: &str) -> Result<Option<RecordMeta>, FormatError> {
    let (_, record) = split_raw(text)?;
    Ok(serde_json::from_str::<MetaOnly>(record.get()).ok().map(|only| only.meta))
}

/// Записи базы для кода: изменённые не раньше `since` (`None` — все), кроме
/// [`SKIPPED`], и живой профиль — всегда.
pub fn pick<'a>(raw: impl IntoIterator<Item = Raw<'a>>, since: Option<Timestamp>) -> Result<Picked, FormatError> {
    at_once(pick_sliced(raw, since, &mut no_pause))
}

/// [`pick`] кусками, с паузой: в базе за год — тысячи записей.
pub async fn pick_sliced<'a, P, F>(
    raw: impl IntoIterator<Item = Raw<'a>>,
    since: Option<Timestamp>,
    pause: &mut P,
) -> Result<Picked, FormatError>
where
    P: FnMut() -> F,
    F: Future<Output = ()>,
{
    let changed = |meta: &RecordMeta| since.is_none_or(|since| meta.modified() >= since);
    let mut meter = Meter::new(pause);
    let mut picked = Picked {
        entries: Vec::new(),
        skipped: 0,
        unreadable: 0,
    };
    let mut profiles = Vec::new();
    for item in raw {
        meter.add(item.text.map_or(0, str::len).max(backup::RECORD_WEIGHT)).await;
        let peeked = match item.text.map(peek) {
            Some(Ok(meta)) => meta,
            Some(Err(FormatError::Newer { found })) => return Err(FormatError::Newer { found }),
            Some(Err(FormatError::Damaged(_))) | None => None,
        };
        if SKIPPED.contains(&item.kind) {
            // Файл, у которого и общие поля не читаются, — тоже не вошёл.
            if peeked.as_ref().is_none_or(changed) {
                picked.skipped += 1;
            }
            continue;
        }
        let wanted = item.kind == Profile::KIND || peeked.as_ref().is_none_or(changed);
        if !wanted {
            continue;
        }
        match backup::read_stored(KINDS, item) {
            Ok(entry) => {
                if entry.kind() == Profile::KIND {
                    profiles.push(entry.clone());
                }
                if changed(entry.meta()) {
                    picked.entries.push(entry);
                }
            }
            Err(FormatError::Newer { found }) => return Err(FormatError::Newer { found }),
            Err(FormatError::Damaged(_)) => picked.unreadable += 1,
        }
    }
    let parsed: Vec<Profile> = profiles
        .iter()
        .filter_map(|entry| decode_record(FORMAT_VERSION, entry.record()).ok())
        .collect();
    if let Some(current) = Profile::current(parsed)
        && !picked
            .entries
            .iter()
            .any(|entry| entry.kind() == Profile::KIND && entry.meta().id() == current.id())
        && let Some(entry) = profiles.into_iter().find(|entry| entry.meta().id() == current.id())
    {
        picked.entries.push(entry);
    }
    Ok(picked)
}

/// Шапка кода: кто, когда и с какого момента.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Header {
    app_version: String,
    created: Timestamp,
    device: RecordId,
    since: Option<Timestamp>,
}

impl Header {
    /// Шапка нового кода: версия программы, момент по часам этого
    /// устройства, его идентификатор и с какого момента изменения.
    pub fn new(app_version: &str, created: Timestamp, device: RecordId, since: Option<Timestamp>) -> Self {
        Self {
            app_version: app_version.to_owned(),
            created,
            device,
            since,
        }
    }

    /// Версия программы, которая сделала код.
    pub fn app_version(&self) -> &str {
        &self.app_version
    }

    /// Когда сделан — по часам устройства, которое его сделало: до этого
    /// момента его изменения в коде все.
    pub const fn created(&self) -> Timestamp {
        self.created
    }

    /// Какое устройство его сделало.
    pub const fn device(&self) -> RecordId {
        self.device
    }

    /// С какого момента изменения; `None` — все записи.
    pub const fn since(&self) -> Option<Timestamp> {
        self.since
    }
}

#[derive(Serialize)]
struct PlainOut<'a> {
    file: &'static str,
    format_version: u32,
    app_version: &'a str,
    created: Timestamp,
    device: RecordId,
    since: Option<Timestamp>,
    records: Vec<RecordOut<'a>>,
}

#[derive(Serialize)]
struct RecordOut<'a> {
    kind: &'static str,
    record: &'a RawValue,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PlainIn<'a> {
    file: String,
    format_version: u32,
    app_version: String,
    created: Timestamp,
    device: RecordId,
    since: Option<Timestamp>,
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

/// Почему код не сделан.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TooLarge {
    /// Сколько байтов вышло бы — или больше: текст длиннее предела не
    /// сжимается вовсе.
    pub bytes: usize,
}

/// Байтов конверта сверх сжатого текста: метка, версии, итерации, соль,
/// вектор и метка подлинности шифра.
const ENVELOPE_BYTES: usize = 3 + 1 + 4 + 4 + 1 + backup::SALT_BYTES + backup::IV_BYTES + backup::TAG_BYTES;

/// То, что окно шифрует: шапка и записи, сжатые; [`TooLarge`] — изменений
/// больше, чем вмещает код.
pub fn pack(header: &Header, entries: &[Entry]) -> Result<Vec<u8>, TooLarge> {
    at_once(pack_sliced(header, entries, &mut no_pause))
}

/// [`pack`] кусками, с паузой.
pub async fn pack_sliced<P, F>(header: &Header, entries: &[Entry], pause: &mut P) -> Result<Vec<u8>, TooLarge>
where
    P: FnMut() -> F,
    F: Future<Output = ()>,
{
    let text = serde_json::to_string(&PlainOut {
        file: PLAIN_MARK,
        format_version: FORMAT_VERSION,
        app_version: &header.app_version,
        created: header.created,
        device: header.device,
        since: header.since,
        records: entries
            .iter()
            .map(|entry| RecordOut {
                kind: entry.kind(),
                record: entry.raw(),
            })
            .collect(),
    })
    .expect("код в JSON не падает: записи уже в JSON, остальное — строки и числа");
    if text.len() > MAX_PLAIN_BYTES {
        return Err(TooLarge { bytes: text.len() });
    }
    let packed = backup::deflate_sliced(text.as_bytes(), CODE_LEVEL, pause).await;
    let bytes = packed.len() + ENVELOPE_BYTES;
    if bytes > MAX_CODE_BYTES {
        return Err(TooLarge { bytes });
    }
    Ok(packed)
}

/// Код целиком: метка, версии, замок и шифротекст — байтами.
pub fn seal(lock: &Lock, ciphertext: &[u8]) -> Vec<u8> {
    let mut code = Vec::with_capacity(ENVELOPE_BYTES + ciphertext.len());
    code.extend_from_slice(MARK);
    code.push(ENVELOPE);
    code.extend_from_slice(&FORMAT_VERSION.to_be_bytes());
    code.extend_from_slice(&lock.iterations().to_be_bytes());
    let salt = lock.salt();
    code.push(u8::try_from(salt.len()).expect("соль замка — не длиннее 64 байтов"));
    code.extend_from_slice(salt);
    code.extend_from_slice(lock.iv());
    code.extend_from_slice(ciphertext);
    code
}

/// Почему код не открыт.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum CodeError {
    /// Это не код программы.
    NotCode,
    /// Код сделан более новой версией программы.
    Newer,
    /// Код испорчен: подробность — для консоли разработчика.
    Damaged(String),
}

impl From<BackupError> for CodeError {
    fn from(error: BackupError) -> Self {
        match error {
            BackupError::NotBackup => Self::NotCode,
            BackupError::Newer { .. } => Self::Newer,
            BackupError::Damaged(why) => Self::Damaged(why),
        }
    }
}

impl From<FormatError> for CodeError {
    fn from(error: FormatError) -> Self {
        match error {
            FormatError::Newer { .. } => Self::Newer,
            FormatError::Damaged(why) => Self::Damaged(why),
        }
    }
}

fn damaged(why: impl ToString) -> CodeError {
    CodeError::Damaged(why.to_string())
}

/// Код до расшифровки: версия формата и замок проверены.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Sealed {
    format_version: u32,
    lock: Lock,
    ciphertext: Vec<u8>,
}

impl Sealed {
    /// Замок: из чего получить ключ.
    pub const fn lock(&self) -> &Lock {
        &self.lock
    }

    /// Шифротекст вместе с меткой подлинности.
    pub fn ciphertext(&self) -> &[u8] {
        &self.ciphertext
    }
}

/// Открыть код: метка, версия — новее программы ли, — замок в пределах
/// и шифротекст не короче метки подлинности.
pub fn open(code: &[u8]) -> Result<Sealed, CodeError> {
    if code.len() < 4 || &code[..3] != MARK {
        return Err(CodeError::NotCode);
    }
    if code[3] > ENVELOPE {
        return Err(CodeError::Newer);
    }
    if code[3] == 0 || code.len() > MAX_CODE_BYTES {
        return Err(damaged("конверт кода не той версии или длины"));
    }
    let mut rest = &code[4..];
    let mut take = |n: usize| -> Result<&[u8], CodeError> {
        if rest.len() < n {
            return Err(damaged("код обрезан"));
        }
        let (head, tail) = rest.split_at(n);
        rest = tail;
        Ok(head)
    };
    let format_version = u32::from_be_bytes(take(4)?.try_into().map_err(damaged)?);
    if format_version > FORMAT_VERSION {
        return Err(CodeError::Newer);
    }
    if format_version < FIRST_VERSION {
        return Err(damaged(format!("версия формата {format_version}: такой не бывает")));
    }
    let iterations = u32::from_be_bytes(take(4)?.try_into().map_err(damaged)?);
    let salt_len = usize::from(take(1)?[0]);
    let salt = take(salt_len)?.to_vec();
    let iv = take(backup::IV_BYTES)?.to_vec();
    let lock = Lock::new(iterations, salt, iv).ok_or_else(|| damaged("параметры шифра за пределами допустимого"))?;
    let ciphertext = rest.to_vec();
    if ciphertext.len() < backup::TAG_BYTES {
        return Err(damaged("данные короче метки подлинности"));
    }
    Ok(Sealed {
        format_version,
        lock,
        ciphertext,
    })
}

/// Код, прочитанный целиком.
#[derive(Debug)]
pub struct Code {
    header: Header,
    entries: Vec<Entry>,
}

impl Code {
    /// Шапка кода.
    pub const fn header(&self) -> &Header {
        &self.header
    }

    /// Записи кода — разобраны своим видом и в текущей версии формата.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Записи — без копирования.
    pub fn into_entries(self) -> Vec<Entry> {
        self.entries
    }
}

/// Записи расшифрованного кода. `opened` — то, что дала расшифровка: сжатый
/// текст. Всё или ничего, как у копии: одна испорченная запись, незнакомый
/// вид, запись конторы или две записи с одним идентификатором — и не
/// читается весь код.
pub fn read(sealed: &Sealed, opened: &[u8]) -> Result<Code, CodeError> {
    at_once(read_sliced(sealed, opened, &mut no_pause))
}

/// [`read`] кусками, с паузой.
pub async fn read_sliced<P, F>(sealed: &Sealed, opened: &[u8], pause: &mut P) -> Result<Code, CodeError>
where
    P: FnMut() -> F,
    F: Future<Output = ()>,
{
    let plain = backup::unpack_sliced(opened, MAX_PLAIN_BYTES, pause).await?;
    let text = std::str::from_utf8(&plain).map_err(damaged)?;
    let fields: PlainIn<'_> = serde_json::from_str(text).map_err(damaged)?;
    if fields.file != PLAIN_MARK || fields.format_version != sealed.format_version {
        return Err(damaged("шапка внутри не совпадает с конвертом"));
    }
    if !backup::is_app_version(&fields.app_version) {
        return Err(damaged("версия программы в шапке — не номер версии"));
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut entries = Vec::with_capacity(fields.records.len());
    let mut meter = Meter::new(pause);
    for item in fields.records {
        meter.add(item.record.get().len().max(backup::RECORD_WEIGHT)).await;
        let entry = backup::entry_of(KINDS, &item.kind, fields.format_version, item.record)?;
        if !seen.insert((entry.kind(), entry.meta().id())) {
            return Err(damaged(format!("запись {} {} в коде дважды", entry.kind(), entry.meta().id())));
        }
        entries.push(entry);
    }
    Ok(Code {
        header: Header {
            app_version: fields.app_version,
            created: fields.created,
            device: fields.device,
            since: fields.since,
        },
        entries,
    })
}

/// Кадр кода: чья передача, какой по счёту и сколько их, и кусок кода.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Frame {
    id: u32,
    index: u16,
    count: u16,
    chunk: Vec<u8>,
}

/// Почему байты — не кадр.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FrameError {
    /// Это не кадр программы: чужой QR-код или текст.
    NotFrame,
    /// Кадр новой версии программы.
    Newer,
}

/// Новая метка передачи: кадры двух передач не смешиваются. Случайность —
/// ядра, как у идентификаторов записей (`RecordId::random`).
pub fn transfer_id() -> u32 {
    let bytes = uuid::Uuid::new_v4().into_bytes();
    u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

/// Кадры кода `code` по `chunk` байтов, с меткой передачи `id`.
pub fn frames(code: &[u8], chunk: usize, id: u32) -> Vec<Vec<u8>> {
    let pieces: Vec<&[u8]> = code.chunks(chunk.max(1)).collect();
    let count = u16::try_from(pieces.len()).unwrap_or(u16::MAX);
    pieces
        .iter()
        .enumerate()
        .map(|(index, piece)| {
            let mut frame = Vec::with_capacity(FRAME_HEADER + piece.len());
            frame.extend_from_slice(FRAME_MARK);
            frame.push(FRAME_VERSION);
            frame.extend_from_slice(&id.to_be_bytes());
            frame.extend_from_slice(&u16::try_from(index).unwrap_or(u16::MAX).to_be_bytes());
            frame.extend_from_slice(&count.to_be_bytes());
            frame.extend_from_slice(piece);
            frame
        })
        .collect()
}

/// Кадр из байтов QR-кода или куска текста.
pub fn frame(bytes: &[u8]) -> Result<Frame, FrameError> {
    if bytes.len() < 4 || &bytes[..3] != FRAME_MARK {
        return Err(FrameError::NotFrame);
    }
    if bytes[3] > FRAME_VERSION {
        return Err(FrameError::Newer);
    }
    if bytes[3] != FRAME_VERSION || bytes.len() <= FRAME_HEADER || bytes.len() > FRAME_HEADER + TEXT_CHUNK {
        return Err(FrameError::NotFrame);
    }
    let id = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    let index = u16::from_be_bytes([bytes[8], bytes[9]]);
    let count = u16::from_be_bytes([bytes[10], bytes[11]]);
    if count == 0 || index >= count || usize::from(count) > MAX_FRAMES {
        return Err(FrameError::NotFrame);
    }
    Ok(Frame {
        id,
        index,
        count,
        chunk: bytes[FRAME_HEADER..].to_vec(),
    })
}

/// QR-коды кода `code`: по кадру в каждом, одной версии — круг не прыгает
/// размером.
pub fn qr_codes(code: &[u8], id: u32) -> Vec<qr::Code> {
    frames(code, QR_CHUNK, id)
        .iter()
        .filter_map(|frame| qr::encode(frame, QR_VERSION, QR_ECC))
        .collect()
}

/// Куски текста кода `code`: [`TEXT_PREFIX`] и кадр буквами и цифрами.
pub fn text_parts(code: &[u8], id: u32) -> Vec<String> {
    frames(code, TEXT_CHUNK, id)
        .iter()
        .map(|frame| format!("{TEXT_PREFIX}{}", base62::encode(frame)))
        .collect()
}

/// Кадры, найденные во вставленном тексте: куски с меткой [`TEXT_PREFIX`]
/// где угодно в нём — с подписью, пробелами и переносами строк вокруг,
/// несколько сообщений подряд.
pub fn frames_in_text(text: &str) -> Vec<Result<Frame, FrameError>> {
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| word.len() <= MAX_TEXT_WORD)
        .filter_map(|word| word.strip_prefix(TEXT_PREFIX))
        .map(|rest| base62::decode(rest).ok_or(FrameError::NotFrame).and_then(|bytes| frame(&bytes)))
        .collect()
}

/// Сборка кода из кадров — в любом порядке и с повторами. Кадр другой
/// передачи начинает сборку заново: значит, на том устройстве сделали
/// новый код, и он новее.
#[derive(Default, Debug)]
pub struct Collector {
    id: Option<u32>,
    parts: Vec<Option<Vec<u8>>>,
}

/// Что сборка сделала с кадром.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Took {
    /// Новый кадр этой передачи.
    New,
    /// Такой уже был.
    Again,
    /// Кадр другой передачи — сборка начата заново с него.
    Restarted,
}

impl Collector {
    /// Взять кадр.
    pub fn take(&mut self, frame: Frame) -> Took {
        let same = self.id == Some(frame.id) && self.parts.len() == usize::from(frame.count);
        let took = if same {
            Took::New
        } else {
            let restarted = self.id.is_some();
            self.id = Some(frame.id);
            self.parts = vec![None; usize::from(frame.count)];
            if restarted { Took::Restarted } else { Took::New }
        };
        let slot = &mut self.parts[usize::from(frame.index)];
        if slot.is_some() {
            return Took::Again;
        }
        *slot = Some(frame.chunk);
        took
    }

    /// Сколько кадров собрано.
    pub fn got(&self) -> usize {
        self.parts.iter().filter(|part| part.is_some()).count()
    }

    /// Сколько кадров у кода; 0 — ни одного ещё не было.
    pub fn count(&self) -> usize {
        self.parts.len()
    }

    /// Код целиком, когда собраны все кадры; `None` — не все или длиннее
    /// предела.
    pub fn whole(&self) -> Option<Vec<u8>> {
        if self.parts.is_empty() || self.parts.iter().any(Option::is_none) {
            return None;
        }
        let code: Vec<u8> = self.parts.iter().flatten().flatten().copied().collect();
        (code.len() <= MAX_CODE_BYTES).then_some(code)
    }
}

/// Что принимающее устройство помнит о каждом отправителе: до какого
/// момента его часов от него принято. Запись устройства — текстом
/// `id=момент;id=момент`, не больше [`MAX_PEERS`] отправителей.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Received {
    peers: Vec<(RecordId, Timestamp)>,
}

impl Received {
    /// Из записи устройства; что не читается — пропускается.
    pub fn parse(text: &str) -> Self {
        let peers = text
            .split(';')
            .filter_map(|pair| {
                let (id, millis) = pair.split_once('=')?;
                let millis: i64 = millis.parse().ok()?;
                (millis.unsigned_abs() <= Timestamp::MAX_MILLIS.unsigned_abs())
                    .then(|| (RecordId::parse(id), Timestamp::from_unix_millis(millis)))
                    .and_then(|(id, at)| Some((id?, at)))
            })
            .take(MAX_PEERS)
            .collect();
        Self { peers }
    }

    /// Запись для устройства.
    pub fn to_text(&self) -> String {
        self.peers
            .iter()
            .map(|(id, at)| format!("{id}={}", at.unix_millis()))
            .collect::<Vec<_>>()
            .join(";")
    }

    /// До какого момента принято от устройства `device`; `None` — от него
    /// ещё не принимали.
    pub fn until(&self, device: RecordId) -> Option<Timestamp> {
        self.peers.iter().find(|(id, _)| *id == device).map(|&(_, at)| at)
    }

    /// Принято от `device` до момента `until`: позже прежнего — запомнить.
    /// Отправитель — первым: самый давний забывается, когда их больше
    /// предела.
    pub fn note(&mut self, device: RecordId, until: Timestamp) {
        let known = self.until(device);
        self.peers.retain(|(id, _)| *id != device);
        self.peers.insert(0, (device, known.map_or(until, |known| known.max(until))));
        self.peers.truncate(MAX_PEERS);
    }
}

/// Цел ли ряд изменений от отправителя.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Continuity {
    /// Код начинается не позже принятого прежде — или это все записи.
    Whole,
    /// От этого устройства здесь ещё не принимали: что было до начала кода,
    /// пришло ли копией — неизвестно.
    First,
    /// Код начинается позже, чем принято прежде: изменения между `got`
    /// и началом кода не пришли — прошлый код не сняли.
    Gap { got: Timestamp },
}

/// Цел ли ряд: код с изменениями с `since` (`None` — все записи) против
/// принятого от того же устройства до `got`.
pub fn continuity(since: Option<Timestamp>, got: Option<Timestamp>) -> Continuity {
    match (since, got) {
        (None, _) => Continuity::Whole,
        (Some(since), Some(got)) if since <= got => Continuity::Whole,
        (Some(_), Some(got)) => Continuity::Gap { got },
        (Some(_), None) => Continuity::First,
    }
}

/// Байты буквами и цифрами: по 8 байтов — 11 знаков из 62 (62¹¹ > 2⁶⁴),
/// хвост из `k` байтов — наименьшим числом знаков, что его вмещает. Против
/// base64 — на 3 % длиннее, зато без `+`, `/`, `-` и `_`.
pub mod base62 {
    const ALPHABET: &[u8; 62] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

    /// Знаков на `k` байтов, `k` от 0 до 8.
    const CHARS: [usize; 9] = [0, 2, 3, 5, 6, 7, 9, 10, 11];

    fn digit(c: u8) -> Option<u64> {
        ALPHABET.iter().position(|&a| a == c).map(|i| i as u64)
    }

    /// Байты — знаками.
    pub fn encode(bytes: &[u8]) -> String {
        let mut out = String::with_capacity(bytes.len().div_ceil(8) * 11);
        for group in bytes.chunks(8) {
            let mut value = group.iter().fold(0u64, |value, &byte| value << 8 | u64::from(byte));
            let mut digits = [0u8; 11];
            let n = CHARS[group.len()];
            for slot in digits[..n].iter_mut().rev() {
                *slot = ALPHABET[(value % 62) as usize];
                value /= 62;
            }
            out.extend(digits[..n].iter().map(|&d| char::from(d)));
        }
        out
    }

    /// Знаки — байтами; `None` — не знаки [`encode`].
    pub fn decode(text: &str) -> Option<Vec<u8>> {
        let text = text.as_bytes();
        let mut out = Vec::with_capacity(text.len() / 11 * 8 + 8);
        for group in text.chunks(11) {
            let k = CHARS.iter().position(|&n| n == group.len())?;
            if k == 0 {
                return None;
            }
            let mut value: u128 = 0;
            for &c in group {
                value = value * 62 + u128::from(digit(c)?);
            }
            if value >> (8 * k) != 0 {
                return None;
            }
            for i in (0..k).rev() {
                out.push((value >> (8 * i)) as u8);
            }
        }
        Some(out)
    }
}

#[cfg(test)]
mod tests;
