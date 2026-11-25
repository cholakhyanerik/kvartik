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
//! Файлы книги, логотип профиля и логотипы выпущенных документов в код
//! изменений не идут ([`SKIPPED`]): это сотни килобайт картинок, и код вырос
//! бы в сотни QR-кодов. Сколько таких изменилось, код говорит числом, и окно
//! называет его словами. Передаёт их отдельный код файлов (с 0.0.174, задача
//! 116; [`pick_files`]) — тот же код, только с файлами и меткой `files`,
//! больше одного кода и поэтому серией: он режется на коды-части не больше
//! [`MAX_CODE_BYTES`] ([`segments`]), у каждой — свой круг QR-кодов и свой
//! текст, а принимающее собирает части в любом порядке ([`Series`]) и кладёт
//! код, когда собраны все. Запись книги без своего файла на другом
//! устройстве показывает файл без ссылки — так же, как нечитаемый.
//!
//! Живой профиль идёт в код всегда, даже не изменённый: по нему принимающее
//! устройство узнаёт, того ли ИП изменения ([`backup::relation`]), — у кода
//! без профиля сверять было бы не с чем, и изменения другого ИП легли бы
//! рядом молча.
//!
//! # Код конторы
//!
//! У кого ИП несколько, код бывает и всех ИП (с 0.0.173, задача 116): записи
//! конторы и изменения каждой базы с меткой своего ИП — как в копии всех ИП
//! (`backup`, `office::route`), и ложится он её маршрутом. Записи ИП списка
//! идут в него всегда, изменённые и нет: по ним принимающее устройство
//! раскладывает базы, а метка без своего ИП в коде — порча
//! ([`pick_office`]). Ключи «Pro» — только изменённые. ИП, добавленный после
//! начала кода, идёт всеми своими записями: изменения «с дня» у нового ИП —
//! это все его записи, а принесённые копией старше дня иначе не дошли бы
//! никогда. А ИП кода, которого у принимающего нет и который не новый, код «с
//! дня» не кладёт ([`office::missing`]): остального его не придёт.
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
//! # Имя устройства и кто выпускает
//!
//! С конверта 4 (задача 124, часть E) шапка кода и квитанция несут имя
//! устройства, которое их сделало, и выпускает ли оно счета и акты
//! ([`Header::from_device`], [`Receipt::from_device`]): так список «Мои
//! устройства» видят оба, и о двух выпускающих окно говорит словами. Имя —
//! чужой ввод: не длиннее [`MAX_NAME_BYTES`] байтов и без управляющих знаков
//! ([`device_name`]), иначе код не читается. Код и квитанция прежних версий —
//! без имени: устройство в списке без имени. Устройства, о которых здесь
//! знают, — запись устройства [`Peers`], не больше [`MAX_PEERS`].
//!
//! # Помнится на устройствах
//!
//! Каждое устройство — со своим идентификатором. Принимающее помнит, до
//! какого момента часов отправителя от него принято ([`Received`]), и код,
//! который начинается позже, называет: изменения между ними не пришли —
//! прошлый код не сняли ([`continuity`]). Сервера, который знал бы, кто что
//! принял, нет, и обратного пути у QR-кода нет — поэтому принявший
//! показывает в ответ квитанцию ([`Receipt`], с 0.0.172): принят ли код и цел
//! ли ряд. Отправитель, снявший её, узнаёт о дыре сам и делает код с её дня;
//! не снял — дыру по-прежнему видит принимающий.

use std::future::Future;

use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

use super::backup::{self, BackupError, Entry, FILE_KINDS, KINDS, Lock, Meter, OFFICE_KINDS, Raw, Tally, at_once, no_pause};
use super::qr::{self, Ecc};
use super::{FIRST_VERSION, FORMAT_VERSION, FormatError, Stored, decode_record, split_raw};
use crate::model::attachment::Attachment;
use crate::model::date::Date;
use crate::model::logo::{IssuedLogo, Logo};
use crate::model::office::Ip;
use crate::model::profile::Profile;
use crate::model::record::{RecordId, RecordMeta, Timestamp};

/// Виды записей, которые в код не идут: файлы книги и картинки логотипов.
pub const SKIPPED: [&str; 3] = [Attachment::KIND, Logo::KIND, IssuedLogo::KIND];

/// Метка кода: первые три байта и версия конверта.
const MARK: &[u8; 3] = b"KVS";

/// Версия конверта кода — не версия формата записей: растёт, когда меняется
/// сам конверт или шапка внутри. Код новее программа не читает и говорит
/// «обновите программу», а не «код испорчен». 2 (0.0.173) — у записи кода
/// метка базы `base`: код всех ИП (шапка модуля, «Код конторы»); 3 (0.0.174) —
/// метка `files` у кода файлов (шапка модуля, «Что передаётся»); 4 (0.0.197) —
/// имя устройства и выпускает ли оно (шапка модуля, «Имя устройства»):
/// прежние версии такой код не примут и скажут «обновите программу».
const ENVELOPE: u8 = 4;

/// Самое длинное имя устройства в байтах UTF-8: в списке оно — строка,
/// а квитанция с ним и в самом тяжёлом случае — дыра, моменты в 13 цифр,
/// имя из кавычек, которые JSON удваивает, — один QR-код версии не больше
/// [`RECEIPT_QR_VERSION`] (тест `a_named_receipt_is_still_one_qr_code`; ревью
/// задачи 124).
pub const MAX_NAME_BYTES: usize = 48;

/// Знак, которого в имени нет: управляющие и знаки направления письма
/// (U+200E, U+200F, U+202A–U+202E, U+2066–U+2069, U+061C), разделители
/// строк и абзацев, BOM. Имя встаёт внутрь фразы окна, и незакрытый RLO
/// перевернул бы её хвост до конца абзаца (UAX #9, X8; ревью задачи 124).
/// ZWJ и ZWNJ остаются: без них не пишутся некоторые имена.
const fn hidden(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{061C}' | '\u{2028}' | '\u{2029}' | '\u{FEFF}'
        )
}

/// Имя устройства, как его ввёл человек или прислал код: без пробелов по
/// краям, без управляющих знаков и знаков направления письма, не длиннее
/// [`MAX_NAME_BYTES`] — длиннее обрезается по границе знака. `None` — пусто.
/// Имя из кода, квитанции и записи устройства проходит через неё же, а не
/// отвергает код: ужесточённое правило имени иначе сделало бы испорченными
/// коды прежних версий вместе с их записями (ревью задачи 124).
pub fn device_name(text: &str) -> Option<String> {
    let clean: String = text.chars().filter(|&c| !hidden(c)).collect();
    let clean = clean.trim();
    let mut cut = clean.len().min(MAX_NAME_BYTES);
    while !clean.is_char_boundary(cut) {
        cut -= 1;
    }
    let name = clean[..cut].trim_end();
    (!name.is_empty()).then(|| name.to_owned())
}

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

/// Больше стольких частей у кода файлов не бывает: каждая — круг QR-кодов
/// в минуту с лишним или дюжина кусков текста, и 32 части — уже больше
/// получаса у камеры; больше файлов — копией файлом.
pub const MAX_SEGMENTS: usize = 32;

/// Байтов кода в одной части серии: часть с шапкой — не больше
/// [`MAX_CODE_BYTES`], как обычный код.
pub const SEGMENT_BYTES: usize = MAX_CODE_BYTES - SEGMENT_HEADER;

/// Байтов шапки части: метка, версия, метка серии, номер и число частей.
const SEGMENT_HEADER: usize = 12;

/// Больше этого код файлов не бывает.
pub const MAX_SERIES_BYTES: usize = MAX_SEGMENTS * SEGMENT_BYTES;

/// Больше этого открытый текст кода файлов не собирается и не разжимается:
/// файлы в base64 сжимаются примерно на четверть — вдвое больше кода с
/// запасом на текст записей (как у копии, `backup::MAX_PLAIN_BYTES`).
const MAX_FILES_PLAIN_BYTES: usize = 2 * MAX_SERIES_BYTES;

/// Уровень сжатия кода файлов: первый, как у копии (`backup`): файлы уже
/// сжаты, и уровень выше лишь дольше держит страницу.
const FILES_LEVEL: u8 = 1;

/// Метка части серии и её версия.
const SEGMENT_MARK: &[u8; 3] = b"KVP";
const SEGMENT_VERSION: u8 = 1;

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

/// Кадр версии 1 — свой кусок кода: номер и число кадров. Им идут куски
/// текста и квитанция, а до 0.0.198 — и круг QR-кодов.
const PLAIN_FRAME: u8 = 1;

/// Кадр версии 2 (с 0.0.198, задача 124, часть F) — символ fountain-кода
/// круга QR-кодов ([`fountain`]): длина кода и номер символа вместо номера
/// и числа кадров, шапка той же длины. Прежние версии такой кадр называют
/// кадром новой версии и просят обновить программу.
const FOUNTAIN_FRAME: u8 = 2;

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
    /// Длина текста изменённых файлов, которые в код не вошли, — для оценки
    /// кода файлов ([`Picked::skipped_bytes`]).
    skipped_text: usize,
    unreadable: usize,
    /// Записи, которые идут в код всегда ([`ALWAYS`]), а пришли сюда тем же
    /// кодом, что открыл встречный ([`Picked::without_echo`]): в коде они
    /// есть, но изменёнными не считаются.
    quiet: Vec<EchoKey>,
}

/// Записи, которые идут в код всегда: живой профиль — по нему принимающее
/// сверяет ИП, — и записи ИП списка у кода всех ИП (шапка модуля).
const ALWAYS: [&str; 2] = [Profile::KIND, Ip::KIND];

impl Picked {
    /// Ничего ещё не выбрано.
    const fn empty() -> Self {
        Self {
            entries: Vec::new(),
            skipped: 0,
            skipped_text: 0,
            unreadable: 0,
            quiet: Vec::new(),
        }
    }

    /// Записи кода.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Те же записи с меткой базы ИП `ip` — для кода всех ИП.
    pub fn in_base(self, ip: RecordId) -> Self {
        Self {
            entries: self.entries.into_iter().map(|entry| entry.in_base(ip)).collect(),
            ..self
        }
    }

    /// Добавить записи другой базы — для кода всех ИП.
    pub fn absorb(&mut self, other: Self) {
        self.entries.extend(other.entries);
        self.skipped += other.skipped;
        self.skipped_text += other.skipped_text;
        self.unreadable += other.unreadable;
        self.quiet.extend(other.quiet);
    }

    /// Без записей, которые здесь те же, что пришли принятым кодом `echo`
    /// (задача 124, п. 14): встречный код вернул бы их тому, кто их прислал.
    /// Записи [`ALWAYS`] остаются — без них принимающее не узнает ИП, — но
    /// изменёнными не считаются. Зовётся, когда метки баз уже стоят.
    #[must_use]
    pub fn without_echo(mut self, echo: &Echo) -> Self {
        if echo.records.is_empty() {
            return self;
        }
        let mut quiet = Vec::new();
        self.entries.retain(|entry| {
            if !echo.has(entry) {
                return true;
            }
            let always = ALWAYS.contains(&entry.kind());
            if always {
                quiet.push(EchoKey::of(entry));
            }
            always
        });
        self.quiet.extend(quiet);
        self
    }

    /// Сколько изменённых файлов книги и логотипов в код не вошло.
    pub const fn skipped(&self) -> usize {
        self.skipped
    }

    /// Сколько байтов вышло бы у кода этих файлов — оценкой, до него
    /// (задача 124): файл лежит в записи base64-ом, на треть длиннее своих
    /// байтов, а сжатие уровнем 1 ([`FILES_LEVEL`]) оставляет от base64 уже
    /// сжатых картинок и PDF 0,755 длины (тест
    /// `the_files_estimate_is_close_and_not_below`) — оценка берёт 0,8,
    /// с запасом. Файлы, которые сжимаются ещё, дадут код меньше оценки —
    /// тогда окно предложит копию файлом зря, но не пообещает серию кодов,
    /// которой не выйдет.
    pub const fn skipped_bytes(&self) -> usize {
        self.skipped_text / 5 * 4
    }

    /// Сколько записей не читается и в код не вошло.
    pub const fn unreadable(&self) -> usize {
        self.unreadable
    }

    /// Изменённых записей в коде — без живого профиля, который идёт всегда.
    pub fn changed(&self, since: Option<Timestamp>) -> usize {
        self.tally(since).total()
    }

    /// Те же записи по видам — для слов «код готов» (задача 124, п. 21).
    pub fn tally(&self, since: Option<Timestamp>) -> Tally {
        Tally::of(self.entries.iter().filter(|entry| {
            since.is_none_or(|since| entry.meta().modified() >= since) && !self.quiet.contains(&EchoKey::of(entry))
        }))
    }
}

/// Поместится ли код файлов в `bytes` байтов (оценка [`Picked::skipped_bytes`])
/// в серию кодов ([`MAX_SERIES_BYTES`]): нет — окно сразу ведёт к копии
/// файлом, а не к серии, которая откажет (задача 124).
pub const fn files_fit(bytes: usize) -> bool {
    bytes.saturating_add(ENVELOPE_BYTES) <= MAX_SERIES_BYTES
}

/// Какая запись: база (у кода всех ИП), вид и идентификатор.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
struct EchoKey {
    base: Option<RecordId>,
    kind: &'static str,
    id: RecordId,
}

impl EchoKey {
    fn of(entry: &Entry) -> Self {
        Self {
            base: entry.base(),
            kind: entry.kind(),
            id: entry.meta().id(),
        }
    }
}

/// Записи принятого кода (задача 124, п. 14) — какими они пришли: момент
/// изменения и отпечаток текста. Встречный код берёт записи с дня, а не по
/// тому, что пришло, и правки отправителя после того дня вернул бы ему же
/// ([`Picked::without_echo`]). Эхо — запись, которая здесь и сейчас та же, что
/// пришла: тот же момент и тот же текст. Одного момента мало: слияние по
/// полям (`format::fields`) берёт поздний момент двух сторон, и запись с
/// правкой этого устройства в другом поле момент пришедшей сохраняет, а
/// текст — нет; такая идёт обратно. Отпечаток — 64 бита FNV-1a: совпасть
/// у двух разных текстов одной записи с одним моментом ему почти негде.
#[derive(Clone, Default, Debug)]
pub struct Echo {
    records: std::collections::BTreeMap<EchoKey, (Timestamp, u64)>,
}

impl Echo {
    /// Записи кода `entries`, какими они пришли.
    pub fn of(entries: &[Entry]) -> Self {
        Self {
            records: entries
                .iter()
                .map(|entry| (EchoKey::of(entry), (entry.meta().modified(), print(entry.record()))))
                .collect(),
        }
    }

    /// Та же ли это запись, что пришла.
    fn has(&self, entry: &Entry) -> bool {
        self.records
            .get(&EchoKey::of(entry))
            .is_some_and(|&(modified, text)| modified == entry.meta().modified() && text == print(entry.record()))
    }
}

/// Отпечаток текста записи — FNV-1a, 64 бита.
fn print(text: &str) -> u64 {
    text.bytes()
        .fold(0xcbf2_9ce4_8422_2325, |hash, byte| (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3))
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
    let mut picked = Picked::empty();
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
                picked.skipped_text += item.text.map_or(0, str::len);
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

/// Файлы для кода файлов (шапка модуля): файлы книги и логотипы
/// ([`SKIPPED`]), изменённые не раньше `since` (`None` — все), и живой
/// профиль — по нему принимающее сверяет ИП, как у кода изменений.
pub fn pick_files<'a>(raw: impl IntoIterator<Item = Raw<'a>>, since: Option<Timestamp>) -> Result<Picked, FormatError> {
    at_once(pick_files_sliced(raw, since, &mut no_pause))
}

/// [`pick_files`] кусками, с паузой: файлов — мегабайты, и неизменённый файл
/// не разбирается вовсе — момент изменения читается без него ([`peek`]).
pub async fn pick_files_sliced<'a, P, F>(
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
    let mut picked = Picked::empty();
    let mut profiles = Vec::new();
    for item in raw {
        let file = SKIPPED.contains(&item.kind);
        if !file && item.kind != Profile::KIND {
            continue;
        }
        if file {
            let peeked = match item.text.map(peek) {
                Some(Ok(meta)) => meta,
                Some(Err(FormatError::Newer { found })) => return Err(FormatError::Newer { found }),
                Some(Err(FormatError::Damaged(_))) | None => None,
            };
            if peeked.as_ref().is_some_and(|meta| !changed(meta)) {
                continue;
            }
            meter.add(item.text.map_or(0, str::len).max(backup::RECORD_WEIGHT)).await;
        }
        match backup::read_stored(KINDS, item) {
            Ok(entry) if entry.kind() == Profile::KIND => profiles.push(entry),
            Ok(entry) => picked.entries.push(entry),
            Err(FormatError::Newer { found }) => return Err(FormatError::Newer { found }),
            Err(FormatError::Damaged(_)) => picked.unreadable += 1,
        }
    }
    let parsed: Vec<Profile> = profiles
        .iter()
        .filter_map(|entry| decode_record(FORMAT_VERSION, entry.record()).ok())
        .collect();
    if let Some(current) = Profile::current(parsed)
        && let Some(entry) = profiles.into_iter().find(|entry| entry.meta().id() == current.id())
    {
        picked.entries.push(entry);
    }
    Ok(picked)
}

/// Записи базы конторы для кода всех ИП (шапка модуля, «Код конторы»):
/// записи ИП списка — все, изменённые и нет, ключи «Pro» — изменённые не
/// раньше `since` (`None` — все). Запись ИП, которая не читается, — отказ:
/// база этого ИП молча выпала бы из кода, как из копии всех ИП.
pub fn pick_office<'a>(raw: impl IntoIterator<Item = Raw<'a>>, since: Option<Timestamp>) -> Result<Picked, FormatError> {
    let mut picked = Picked::empty();
    for item in raw {
        match backup::read_stored(OFFICE_KINDS, item) {
            Ok(entry) => {
                if entry.kind() == Ip::KIND || since.is_none_or(|since| entry.meta().modified() >= since) {
                    picked.entries.push(entry);
                }
            }
            Err(FormatError::Newer { found }) => return Err(FormatError::Newer { found }),
            Err(FormatError::Damaged(why)) if item.kind == Ip::KIND => return Err(FormatError::Damaged(why)),
            Err(FormatError::Damaged(_)) => picked.unreadable += 1,
        }
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
    /// Код файлов (шапка модуля): ряд изменений он не продолжает и не рвёт.
    files: bool,
    /// Имя устройства, которое код сделало; `None` — его нет (код до
    /// конверта 4).
    name: Option<String>,
    /// Выпускает ли оно счета и акты этого кода; `None` — неизвестно.
    issues: Option<bool>,
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
            files: false,
            name: None,
            issues: None,
        }
    }

    /// Та же шапка с именем устройства и тем, выпускает ли оно (задача 124,
    /// часть E). Имя — через [`device_name`].
    #[must_use]
    pub fn from_device(self, name: &str, issues: bool) -> Self {
        Self {
            name: device_name(name),
            issues: Some(issues),
            ..self
        }
    }

    /// Имя устройства, которое код сделало.
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Выпускает ли оно счета и акты; `None` — неизвестно.
    pub const fn issues(&self) -> Option<bool> {
        self.issues
    }

    /// Та же шапка у кода файлов.
    pub fn for_files(self) -> Self {
        Self { files: true, ..self }
    }

    /// Код ли это файлов.
    pub const fn files(&self) -> bool {
        self.files
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
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    files: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    issues: Option<bool>,
    records: Vec<RecordOut<'a>>,
}

#[derive(Serialize)]
struct RecordOut<'a> {
    kind: &'static str,
    /// Чья база — у кода всех ИП (`Entry::base`).
    #[serde(skip_serializing_if = "Option::is_none")]
    base: Option<RecordId>,
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
    /// С конверта 3 (0.0.174); у прежних — нет.
    #[serde(default)]
    files: bool,
    /// С конверта 4 (0.0.197); у прежних — нет.
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    issues: Option<bool>,
    #[serde(borrow)]
    records: Vec<RecordIn<'a>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RecordIn<'a> {
    kind: String,
    /// С конверта 2 (0.0.173); у прежних — нет.
    #[serde(default)]
    base: Option<RecordId>,
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
        files: header.files,
        name: header.name.as_deref(),
        issues: header.issues,
        records: entries
            .iter()
            .map(|entry| RecordOut {
                kind: entry.kind(),
                base: entry.base(),
                record: entry.raw(),
            })
            .collect(),
    })
    .expect("код в JSON не падает: записи уже в JSON, остальное — строки и числа");
    let (plain_limit, level, limit) = if header.files {
        (MAX_FILES_PLAIN_BYTES, FILES_LEVEL, MAX_SERIES_BYTES)
    } else {
        (MAX_PLAIN_BYTES, CODE_LEVEL, MAX_CODE_BYTES)
    };
    if text.len() > plain_limit {
        return Err(TooLarge { bytes: text.len() });
    }
    let packed = backup::deflate_sliced(text.as_bytes(), level, pause).await;
    let bytes = packed.len() + ENVELOPE_BYTES;
    if bytes > limit {
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
    open_up_to(code, MAX_CODE_BYTES)
}

/// Открыть код, собранный из частей серии ([`Series`]): он больше обычного.
pub fn open_series(code: &[u8]) -> Result<Sealed, CodeError> {
    open_up_to(code, MAX_SERIES_BYTES)
}

fn open_up_to(code: &[u8], limit: usize) -> Result<Sealed, CodeError> {
    if code.len() < 4 || &code[..3] != MARK {
        return Err(CodeError::NotCode);
    }
    if code[3] > ENVELOPE {
        return Err(CodeError::Newer);
    }
    if code[3] == 0 || code.len() > limit {
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
/// вид, запись не на своём месте (запись конторы в коде одного ИП, метка без
/// своего ИП — `backup::whole`) или две записи с одним идентификатором
/// в одной базе — и не читается весь код.
pub fn read(sealed: &Sealed, opened: &[u8]) -> Result<Code, CodeError> {
    at_once(read_sliced(sealed, opened, &mut no_pause))
}

/// [`read`] кусками, с паузой.
pub async fn read_sliced<P, F>(sealed: &Sealed, opened: &[u8], pause: &mut P) -> Result<Code, CodeError>
where
    P: FnMut() -> F,
    F: Future<Output = ()>,
{
    // Код больше обычного — код файлов серией: и текст у него больше.
    let limit = if sealed.ciphertext.len() > MAX_CODE_BYTES { MAX_FILES_PLAIN_BYTES } else { MAX_PLAIN_BYTES };
    let plain = backup::unpack_sliced(opened, limit, pause).await?;
    let text = std::str::from_utf8(&plain).map_err(damaged)?;
    let fields: PlainIn<'_> = serde_json::from_str(text).map_err(damaged)?;
    if fields.file != PLAIN_MARK || fields.format_version != sealed.format_version {
        return Err(damaged("шапка внутри не совпадает с конвертом"));
    }
    if !backup::is_app_version(&fields.app_version) {
        return Err(damaged("версия программы в шапке — не номер версии"));
    }
    let name = fields.name.as_deref().and_then(device_name);
    let mut seen = std::collections::BTreeSet::new();
    let mut entries = Vec::with_capacity(fields.records.len());
    let mut meter = Meter::new(pause);
    for item in fields.records {
        meter.add(item.record.get().len().max(backup::RECORD_WEIGHT)).await;
        let entry = backup::entry_of(FILE_KINDS, &item.kind, fields.format_version, item.record)?;
        let entry = match item.base {
            Some(base) => entry.in_base(base),
            None => entry,
        };
        if !seen.insert((entry.base(), entry.kind(), entry.meta().id())) {
            return Err(damaged(format!("запись {} {} в коде дважды", entry.kind(), entry.meta().id())));
        }
        entries.push(entry);
    }
    backup::whole(&entries)?;
    Ok(Code {
        header: Header {
            app_version: fields.app_version,
            created: fields.created,
            device: fields.device,
            since: fields.since,
            files: fields.files,
            name,
            issues: fields.issues,
        },
        entries,
    })
}

/// Кадр кода: чья передача, какой по счёту и сколько их, и кусок кода.
/// У кадра версии 2 — номер символа fountain-кода, число кусков кода и его
/// длина (`length`).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Frame {
    id: u32,
    index: u16,
    count: u16,
    chunk: Vec<u8>,
    length: Option<u16>,
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
            frame.push(PLAIN_FRAME);
            frame.extend_from_slice(&id.to_be_bytes());
            frame.extend_from_slice(&u16::try_from(index).unwrap_or(u16::MAX).to_be_bytes());
            frame.extend_from_slice(&count.to_be_bytes());
            frame.extend_from_slice(piece);
            frame
        })
        .collect()
}

/// Кадр из байтов QR-кода или куска текста: версии 1 или 2.
pub fn frame(bytes: &[u8]) -> Result<Frame, FrameError> {
    if bytes.len() < 4 || &bytes[..3] != FRAME_MARK {
        return Err(FrameError::NotFrame);
    }
    if bytes[3] > FOUNTAIN_FRAME {
        return Err(FrameError::Newer);
    }
    if bytes[3] == 0 || bytes.len() <= FRAME_HEADER || bytes.len() > FRAME_HEADER + TEXT_CHUNK {
        return Err(FrameError::NotFrame);
    }
    let id = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    let first = u16::from_be_bytes([bytes[8], bytes[9]]);
    let second = u16::from_be_bytes([bytes[10], bytes[11]]);
    let chunk = bytes[FRAME_HEADER..].to_vec();
    if bytes[3] == FOUNTAIN_FRAME {
        // Длина кода и номер символа; кусков — по длине куска в кадре, и не
        // больше, чем у самого длинного кода самыми мелкими кадрами.
        let length = first;
        let pieces = fountain::pieces(usize::from(length), chunk.len());
        if length == 0 || usize::from(length) > MAX_CODE_BYTES || pieces > MAX_FRAMES {
            return Err(FrameError::NotFrame);
        }
        return Ok(Frame {
            id,
            index: second,
            count: u16::try_from(pieces).map_err(|_| FrameError::NotFrame)?,
            chunk,
            length: Some(length),
        });
    }
    let (index, count) = (first, second);
    if count == 0 || index >= count || usize::from(count) > MAX_FRAMES {
        return Err(FrameError::NotFrame);
    }
    Ok(Frame {
        id,
        index,
        count,
        chunk,
        length: None,
    })
}

/// Сколько кадров в первом круге QR-кодов кода `code`: его кусков. Дальше
/// круг идёт новыми символами без конца ([`qr_frame`]).
pub fn qr_count(code: &[u8]) -> usize {
    fountain::pieces(code.len(), QR_CHUNK)
}

/// Кадр `esi` круга QR-кодов кода `code` передачи `id` — версии 2: символ
/// fountain-кода. Первые [`qr_count`] — куски кода, дальше — их смеси, и
/// пропущенный кадр не ждёт следующего круга (задача 124, часть F). Длина
/// кода — двумя байтами: код больше [`MAX_CODE_BYTES`] не делается.
pub fn qr_frame(code: &[u8], id: u32, esi: u16) -> Vec<u8> {
    let length = u16::try_from(code.len()).unwrap_or(u16::MAX);
    let mut frame = Vec::with_capacity(FRAME_HEADER + QR_CHUNK);
    frame.extend_from_slice(FRAME_MARK);
    frame.push(FOUNTAIN_FRAME);
    frame.extend_from_slice(&id.to_be_bytes());
    frame.extend_from_slice(&length.to_be_bytes());
    frame.extend_from_slice(&esi.to_be_bytes());
    frame.extend_from_slice(&fountain::symbol(code, QR_CHUNK, id, esi));
    frame
}

/// QR-код кадра `esi` ([`qr_frame`]): одной версии у всех — круг не прыгает
/// размером.
pub fn qr_code(code: &[u8], id: u32, esi: u16) -> Option<qr::Code> {
    qr::encode(&qr_frame(code, id, esi), QR_VERSION, QR_ECC)
}

/// QR-коды первого круга кода `code` — куски кода кадрами версии 2.
pub fn qr_codes(code: &[u8], id: u32) -> Vec<qr::Code> {
    (0..qr_count(code))
        .filter_map(|esi| qr_code(code, id, u16::try_from(esi).ok()?))
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
/// новый код, и он новее. Кадры версии 2 собирает fountain-код: хватает
/// любых кусков числом, а не каждого своего (задача 124, часть F); кадр
/// другой версии той же передачи — тоже заново.
#[derive(Default, Debug)]
pub struct Collector {
    id: Option<u32>,
    parts: Vec<Option<Vec<u8>>>,
    /// Сборка кадров версии 2: длина кода и решатель.
    fountain: Option<(u16, fountain::Solver)>,
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
        let same = self.id == Some(frame.id)
            && match (&self.fountain, frame.length) {
                (Some((length, solver)), Some(of)) => {
                    *length == of && solver.block() == frame.chunk.len() && solver.pieces() == usize::from(frame.count)
                }
                (None, None) => self.parts.len() == usize::from(frame.count),
                _ => false,
            };
        let took = if same {
            Took::New
        } else {
            let restarted = self.id.is_some();
            self.id = Some(frame.id);
            match frame.length {
                Some(length) => {
                    self.parts = Vec::new();
                    let solver = fountain::Solver::new(frame.id, usize::from(frame.count), frame.chunk.len());
                    self.fountain = Some((length, solver));
                }
                None => {
                    self.fountain = None;
                    self.parts = vec![None; usize::from(frame.count)];
                }
            }
            if restarted { Took::Restarted } else { Took::New }
        };
        if let Some((_, solver)) = &mut self.fountain {
            return if solver.take(frame.index, &frame.chunk) { took } else { Took::Again };
        }
        let slot = &mut self.parts[usize::from(frame.index)];
        if slot.is_some() {
            return Took::Again;
        }
        *slot = Some(frame.chunk);
        took
    }

    /// Сколько кадров собрано: у кадров версии 2 — сколько кусков кода уже
    /// определено.
    pub fn got(&self) -> usize {
        match &self.fountain {
            Some((_, solver)) => solver.rank(),
            None => self.parts.iter().filter(|part| part.is_some()).count(),
        }
    }

    /// Сколько кадров у кода — у кадров версии 2 сколько кусков; 0 — ни
    /// одного ещё не было.
    pub fn count(&self) -> usize {
        match &self.fountain {
            Some((_, solver)) => solver.pieces(),
            None => self.parts.len(),
        }
    }

    /// Код целиком, когда собраны все кадры; `None` — не все или длиннее
    /// предела.
    pub fn whole(&self) -> Option<Vec<u8>> {
        if let Some((length, solver)) = &self.fountain {
            return solver.whole(usize::from(*length)).filter(|code| code.len() <= MAX_CODE_BYTES);
        }
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

/// Устройство, о котором это знает (задача 124, часть E): имя, когда был
/// последний обмен с ним по часам этого устройства и выпускало ли оно тогда
/// счета и акты.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Peer {
    pub id: RecordId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub last: Timestamp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issues: Option<bool>,
}

/// Устройства, о которых это знает, — запись устройства, текстом JSON, свежие
/// первыми, не больше [`MAX_PEERS`]. Нечитаемая запись — пустой список:
/// это память о соседях, а не данные.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Peers {
    list: Vec<Peer>,
}

impl Peers {
    /// Из записи устройства.
    pub fn parse(text: &str) -> Self {
        let list: Vec<Peer> = serde_json::from_str(text).unwrap_or_default();
        let list = list
            .into_iter()
            .map(|peer| Peer {
                name: peer.name.as_deref().and_then(device_name),
                ..peer
            })
            .take(MAX_PEERS)
            .collect();
        Self { list }
    }

    /// Запись для устройства.
    pub fn to_text(&self) -> String {
        serde_json::to_string(&self.list).unwrap_or_default()
    }

    /// Устройства — свежие первыми.
    pub fn list(&self) -> &[Peer] {
        &self.list
    }

    /// Был обмен с устройством `id` в момент `at`: его имя и выпускает ли оно —
    /// пришедшие новее прежних, а нет их (код прежней версии) — прежние.
    pub fn note(&mut self, id: RecordId, name: Option<&str>, at: Timestamp, issues: Option<bool>) {
        let known = self.list.iter().position(|peer| peer.id == id).map(|at| self.list.remove(at));
        let name = name.and_then(device_name).or_else(|| known.as_ref().and_then(|peer| peer.name.clone()));
        let issues = issues.or_else(|| known.as_ref().and_then(|peer| peer.issues));
        self.list.insert(0, Peer { id, name, last: at, issues });
        self.list.truncate(MAX_PEERS);
    }
}

/// Цел ли ряд изменений от отправителя. В квитанции — `"whole"`, `"first"`
/// или `{"gap":{"got":…}}`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
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

/// Метка квитанции: первые три байта и версия её конверта (задача 116).
const RECEIPT_MARK: &[u8; 3] = b"KVR";

/// Версия конверта квитанции: 2 (0.0.197) — имя принявшего устройства и
/// выпускает ли оно (шапка модуля, «Имя устройства»).
const RECEIPT_ENVELOPE: u8 = 2;

/// Метка открытого текста квитанции.
const RECEIPT_PLAIN_MARK: &str = "kvartik-receipt";

/// Больше этого квитанция не бывает: шапка, вектор, текст в пару сотен
/// байтов и метка подлинности — один QR-кадр (тест
/// `a_receipt_is_one_qr_code`), а длиннее — чужое.
const MAX_RECEIPT_BYTES: usize = 512;

/// Квитанция (задача 116): что устройство, принявшее код, отвечает тому,
/// которое его сделало. Обратного пути у QR-кода нет, и без неё пропущенный
/// код — сделанный, но не снятый, — видел только принимающий ([`continuity`]),
/// а отправитель не знал, что дошло. Квитанция — под тем же ключом, что код
/// ([`seal_receipt`]): сделать её может только тот, кто знает пароль, — иначе
/// подложенная квитанция «всё дошло» спрятала бы от отправителя дыру.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    file: String,
    /// Устройство, сделавшее код.
    sender: RecordId,
    /// Устройство, принявшее его.
    receiver: RecordId,
    /// Момент принятого кода — по часам отправителя ([`Header::created`]):
    /// по нему отправитель узнаёт свой код.
    code: Timestamp,
    /// Цел ли ряд ([`Continuity`]) — так, как его видит принявший.
    taken: Continuity,
    /// Имя принявшего устройства (с конверта 2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    /// Выпускает ли оно счета и акты (с конверта 2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    issues: Option<bool>,
}

impl Receipt {
    /// Квитанция на код с шапкой `header`, принятый устройством `receiver`
    /// с целостью ряда `continuity`.
    pub fn new(header: &Header, receiver: RecordId, continuity: Continuity) -> Self {
        Self {
            file: RECEIPT_PLAIN_MARK.to_owned(),
            sender: header.device,
            receiver,
            code: header.created,
            taken: continuity,
            name: None,
            issues: None,
        }
    }

    /// Та же квитанция с именем принявшего устройства и тем, выпускает ли оно
    /// (задача 124, часть E).
    #[must_use]
    pub fn from_device(self, name: &str, issues: bool) -> Self {
        Self {
            name: device_name(name),
            issues: Some(issues),
            ..self
        }
    }

    /// Устройство, принявшее код.
    pub const fn receiver(&self) -> RecordId {
        self.receiver
    }

    /// Его имя.
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Выпускает ли оно счета и акты; `None` — неизвестно.
    pub const fn issues(&self) -> Option<bool> {
        self.issues
    }

    /// Открытый текст — то, что окно шифрует ключом кода.
    pub fn plain(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("квитанция в JSON не падает")
    }

    /// Квитанция из открытого текста.
    pub fn read(plain: &[u8]) -> Result<Self, CodeError> {
        // Из строки, а не `from_slice`: разбор байтов — свой вариант разбора
        // идентификатора записи, и программа разреза клала его в главный wasm
        // (+1,7 КБ кода, замер задачи 116).
        let text = std::str::from_utf8(plain).map_err(damaged)?;
        let receipt: Self = serde_json::from_str(text).map_err(damaged)?;
        if receipt.file != RECEIPT_PLAIN_MARK {
            return Err(damaged("квитанция не та"));
        }
        let name = receipt.name.as_deref().and_then(device_name);
        Ok(Self { name, ..receipt })
    }
}

/// Что квитанция говорит отправителю.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Acked {
    /// Код принят, и ряд цел: на том устройстве всё, что было здесь до кода.
    Whole,
    /// Код принят, но от этого устройства там принимают впервые: было ли
    /// там то, что здесь до начала кода, неизвестно (ревью задачи 116:
    /// «цел» обещал бы больше, чем знает принявший).
    First,
    /// Код принят, но между принятым прежде (`got`, по часам этого
    /// устройства) и началом кода изменений там нет: прошлый код не сняли.
    Gap { got: Timestamp },
    /// Квитанция не на этот код или не этому устройству.
    Other,
}

/// Что квитанция `receipt` значит для устройства `me`, показавшего код
/// с моментом `code`.
pub fn acked(receipt: &Receipt, me: RecordId, code: Timestamp) -> Acked {
    if receipt.sender != me || receipt.code != code {
        return Acked::Other;
    }
    match receipt.taken {
        Continuity::Whole => Acked::Whole,
        Continuity::First => Acked::First,
        Continuity::Gap { got } => Acked::Gap { got },
    }
}

/// Квитанция целиком: метка, версия, вектор и шифротекст. Соли и итераций
/// нет: читает её отправитель, у которого ключ кода ещё в руках.
pub fn seal_receipt(iv: &[u8], ciphertext: &[u8]) -> Vec<u8> {
    let mut sealed = Vec::with_capacity(4 + iv.len() + ciphertext.len());
    sealed.extend_from_slice(RECEIPT_MARK);
    sealed.push(RECEIPT_ENVELOPE);
    sealed.extend_from_slice(iv);
    sealed.extend_from_slice(ciphertext);
    sealed
}

/// Квитанция до расшифровки.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SealedReceipt {
    iv: Vec<u8>,
    ciphertext: Vec<u8>,
}

impl SealedReceipt {
    /// Вектор шифра.
    pub fn iv(&self) -> &[u8] {
        &self.iv
    }

    /// Шифротекст вместе с меткой подлинности.
    pub fn ciphertext(&self) -> &[u8] {
        &self.ciphertext
    }
}

/// Открыть квитанцию: метка, версия, длина.
pub fn open_receipt(bytes: &[u8]) -> Result<SealedReceipt, CodeError> {
    if bytes.len() < 4 || &bytes[..3] != RECEIPT_MARK {
        return Err(CodeError::NotCode);
    }
    if bytes[3] > RECEIPT_ENVELOPE {
        return Err(CodeError::Newer);
    }
    let rest = &bytes[4..];
    if bytes[3] == 0 || bytes.len() > MAX_RECEIPT_BYTES || rest.len() < backup::IV_BYTES + backup::TAG_BYTES {
        return Err(damaged("квитанция не той версии или длины"));
    }
    let (iv, ciphertext) = rest.split_at(backup::IV_BYTES);
    Ok(SealedReceipt {
        iv: iv.to_vec(),
        ciphertext: ciphertext.to_vec(),
    })
}

/// Самая большая версия QR-кода квитанции — на две больше кода
/// [`QR_VERSION`]: квитанция с дырой, моментами в 13 цифр и именем в
/// [`MAX_NAME_BYTES`] — 309 байтов кадра, а с именем из кавычек — 357, когда
/// версия 12 уровня M вмещает 287, а 14 — 362 (ревью задачи 124: до него
/// такой квитанции не было вовсе, ни QR-кодом, ни текстом). Квитанция стоит
/// на экране одна и не кругом, и снять версию 13 или 14 камере не труднее;
/// обычно она меньше версии кода.
pub const RECEIPT_QR_VERSION: u8 = QR_VERSION + 2;

/// QR-код квитанции `sealed` — одним кадром, самой малой версии, что её
/// вмещает, а не версией кода [`QR_VERSION`]: квитанция в пару сотен байтов,
/// и с меньшим числом модулей каждый крупнее — камере её снять проще. `None`
/// — квитанция длиннее одного кадра: такой программа не делает.
pub fn receipt_qr(sealed: &[u8], id: u32) -> Option<qr::Code> {
    let frame = frames(sealed, sealed.len(), id).into_iter().next()?;
    (1..=RECEIPT_QR_VERSION)
        .find(|&version| qr::capacity(version, QR_ECC) >= frame.len())
        .and_then(|version| qr::encode(&frame, version, QR_ECC))
}

/// Части серии кода `code` — каждая со своей шапкой: метка, версия, метка
/// серии `id`, номер и число частей. Каждая — не больше [`MAX_CODE_BYTES`]
/// и показывается, как обычный код: своим кругом QR-кодов и своим текстом.
pub fn segments(code: &[u8], id: u32) -> Vec<Vec<u8>> {
    let pieces: Vec<&[u8]> = code.chunks(SEGMENT_BYTES).collect();
    let count = u16::try_from(pieces.len()).unwrap_or(u16::MAX);
    pieces
        .iter()
        .enumerate()
        .map(|(index, piece)| {
            let mut segment = Vec::with_capacity(SEGMENT_HEADER + piece.len());
            segment.extend_from_slice(SEGMENT_MARK);
            segment.push(SEGMENT_VERSION);
            segment.extend_from_slice(&id.to_be_bytes());
            segment.extend_from_slice(&u16::try_from(index).unwrap_or(u16::MAX).to_be_bytes());
            segment.extend_from_slice(&count.to_be_bytes());
            segment.extend_from_slice(piece);
            segment
        })
        .collect()
}

/// Сборка кода из частей серии — в любом порядке и с повторами; часть
/// другой серии начинает сборку заново, как кадр другой передачи у
/// [`Collector`].
#[derive(Default, Debug)]
pub struct Series {
    id: Option<u32>,
    parts: Vec<Option<Vec<u8>>>,
}

impl Series {
    /// Взять часть серии — байты, собранные из кадров ([`Collector::whole`]).
    /// `Err` — это не часть или часть новой версии.
    pub fn take(&mut self, bytes: &[u8]) -> Result<Took, FrameError> {
        if bytes.len() < 4 || &bytes[..3] != SEGMENT_MARK {
            return Err(FrameError::NotFrame);
        }
        if bytes[3] > SEGMENT_VERSION {
            return Err(FrameError::Newer);
        }
        if bytes[3] != SEGMENT_VERSION || bytes.len() <= SEGMENT_HEADER {
            return Err(FrameError::NotFrame);
        }
        let id = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        let index = usize::from(u16::from_be_bytes([bytes[8], bytes[9]]));
        let count = usize::from(u16::from_be_bytes([bytes[10], bytes[11]]));
        if count == 0 || index >= count || count > MAX_SEGMENTS {
            return Err(FrameError::NotFrame);
        }
        let took = if self.id == Some(id) && self.parts.len() == count {
            Took::New
        } else {
            let restarted = self.id.is_some();
            self.id = Some(id);
            self.parts = vec![None; count];
            if restarted { Took::Restarted } else { Took::New }
        };
        let slot = &mut self.parts[index];
        if slot.is_some() {
            return Ok(Took::Again);
        }
        *slot = Some(bytes[SEGMENT_HEADER..].to_vec());
        Ok(took)
    }

    /// Сколько частей собрано.
    pub fn got(&self) -> usize {
        self.parts.iter().filter(|part| part.is_some()).count()
    }

    /// Сколько частей у серии; 0 — ни одной ещё не было.
    pub fn count(&self) -> usize {
        self.parts.len()
    }

    /// Код целиком, когда собраны все части; `None` — не все или длиннее
    /// предела.
    pub fn whole(&self) -> Option<Vec<u8>> {
        if self.parts.is_empty() || self.parts.iter().any(Option::is_none) {
            return None;
        }
        let code: Vec<u8> = self.parts.iter().flatten().flatten().copied().collect();
        (code.len() <= MAX_SERIES_BYTES).then_some(code)
    }
}

/// Что собрано из кадров.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Payload {
    /// Код передачи ([`open`]).
    Code,
    /// Часть серии — кода файлов ([`Series`]).
    Segment,
    /// Квитанция ([`open_receipt`]).
    Receipt,
    /// Ни то, ни другое.
    Other,
}

/// Код это или квитанция — по метке: QR-коды и текст у обоих одни, и снятое
/// не там окно называет словами, а не «это не код программы».
pub fn payload(bytes: &[u8]) -> Payload {
    match bytes.get(..3) {
        Some(mark) if mark == MARK => Payload::Code,
        Some(mark) if mark == SEGMENT_MARK => Payload::Segment,
        Some(mark) if mark == RECEIPT_MARK => Payload::Receipt,
        _ => Payload::Other,
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

mod fountain;

#[cfg(test)]
mod tests;
