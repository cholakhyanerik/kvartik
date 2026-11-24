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
//! С версии 6 (0.0.27) открытый текст перед шифром сжат ([`pack`]) —
//! DEFLATE без обёртки (RFC 1951): во вложениях книги (задача 14) байты
//! файлов лежат base64, а он сжимается на четверть, текст записей — в разы.
//! Сжимать после шифра бесполезно: шифротекст неотличим от случайных
//! байтов. Сжат ли текст, говорит версия в шапке, а не отдельное поле:
//! версии 1–5 не сжимали, с 6-й сжимают всегда.
//!
//! # Кусками, с паузой
//!
//! С файлами книги копия — десятки мегабайт, и разбор, сжатие и base64
//! одним куском держали страницу секундами (замер 0.0.27: 20 МБ —
//! кадр в 5,7 с на телефоне). Поэтому долгие шаги — `*_sliced` — идут
//! кусками по [`SLICE_BYTES`] и между ними ждут паузу, которую даёт окно:
//! отдать управление браузеру (Правило 1). Ядро паузу не выбирает и часов
//! не знает — меряет работу байтами. Те же шаги без пауз (`collect`,
//! `pack`, `seal`, `open`, `read`) — та же работа одним куском: так их
//! зовут тесты и ядро.
//!
//! # Частями
//!
//! Файл копии не бывает больше [`MAX_FILE_BYTES`]: такой программа не
//! откроет. Копия, которая в него не влезает, с версии 27 (0.0.181,
//! задача 117) — несколько файлов, частей ([`split`]): в каждом не больше
//! [`PART_PLAIN_BYTES`] открытого текста, записи без файлов книги — первыми
//! (у копии одного ИП они почти всегда влезают в первую часть, а у конторы на
//! десятки ИП могут уйти и во вторую), файлы книги — по порядку после них. В шапке части — её номер, сколько их
//! и общий идентификатор копии ([`Part`]): так окно говорит, что это часть
//! и какие ещё остались. Восстановление сливает и ничего не стирает, поэтому
//! каждая часть — копия сама по себе и ложится одна, в любом порядке. Для
//! этого в каждой части — записи, по которым её узнаёт восстановление:
//! профили ИП (ни один ли это ИП, [`relation`]) и записи ИП конторы (куда
//! ложится запись с меткой базы, [`whole`]). Убрать что-то из файла,
//! а не из базы, нельзя: обещание «удалённое восстанавливается» держится
//! и на убранных файлах книги (выбор задачи 64), — поэтому части, а не копия
//! без файлов и не стёртые байты убранных (разбор задачи 117).
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
//! «Записи не стираются»). На этом держится и второе устройство (задача 41):
//! копия с одного, восстановленная на другом, и обратно — и у обоих все
//! записи обоих. Из двух правок одной записи каждое поле берётся с той
//! стороны, где его правили позже по часам её устройства (`format::fields`,
//! задача 116), а у документа — выпущенный целиком, а не черновик
//! ([`issued_wins`]), — поэтому о часах, которые спешат, окно говорит
//! ([`ahead`]), а номера документов выдаёт одно устройство
//! (`model::document`).
//!
//! Копию другого ИП — ни общей записи профиля, ни общего ՀՎՀՀ — окно до
//! записи показывает человеку вместе с ИП программы и сливает только по его
//! ответу ([`relation`], с 0.0.85): по идентификаторам записи двух ИП не
//! совпадут, и слияние молча положило бы их рядом.

mod laid;

pub use laid::Laid;

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::pin::pin;
use std::task::{Context, Poll, Waker};

use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

use super::{
    FIRST_VERSION, FORMAT_VERSION, FormatError, Stored, base64, decode_record, fields, split_raw, wrap,
};
use crate::i18n::Lang;
use crate::license::{License, OfficeLicense};
use crate::model::attachment::Attachment;
use crate::model::client::Client;
use crate::model::date::Date;
use crate::model::document::Document;
use crate::model::ledger::Operation;
use crate::model::logo::{IssuedLogo, Logo};
use crate::model::office::Ip;
use crate::model::profile::{Localized, Profile};
use crate::model::record::{RecordId, RecordMeta, Timestamp};
use crate::model::recurring::Recurring;
use crate::model::text::Line;
use crate::model::tin::Tin;
use crate::tax::closing::Closing;
use crate::tax::opening::Opening;

/// Метка файла копии: по ней свой файл отличается от чужого JSON. Часть
/// формата — не меняется и при смене имени программы (задача 16).
pub const FILE_MARK: &str = "kvartik-backup";

/// Больше этого файл копии не читается — проверяется до чтения
/// (Правило 13): файл в гигабайт повесил бы страницу, а то и закрыл бы её
/// на телефоне. Год работы занятого ИП без вложений — около 2 МБ текста
/// (600 документов по 1–2 КБ и тысяча операций книги по полкилобайта),
/// а сжатым — сотни килобайт. Место занимают вложения (задача 14): фото
/// чека — сотни килобайт, и 128 МБ — это около трёхсот фото по
/// 300 КБ (base64 и шифр — ещё треть). Предел был 32 МБ до вложений.
/// Копию больше предела окно с 0.0.181 делит на части ([`split`],
/// задача 117), а до того не делало вовсе: такая не открылась бы.
pub const MAX_FILE_BYTES: u64 = 128 << 20;

/// С этой версии формата открытый текст копии сжат перед шифром ([`pack`]).
const PACKED_SINCE: u32 = 6;

/// С этой версии формата копия бывает частями ([`Part`]): шапки прежних
/// версий части не несут, и такая шапка у них — порча.
const PARTS_SINCE: u32 = 27;

/// Больше стольких частей у копии не бывает: 999 файлов по
/// [`PART_PLAIN_BYTES`] — около 90 ГБ, больше, чем браузер даёт сайту на
/// телефоне. Предел проверяется и при чтении шапки (Правило 13): номер части
/// окно показывает человеку.
pub const MAX_PARTS: u16 = 999;

/// Сколько байтов открытого текста — не больше — кладётся в один файл копии
/// ([`split`]). Файл из такого текста не больше [`MAX_FILE_BYTES`] при
/// любых данных ([`sealed_bound`]): файлы книги — уже сжатые JPEG и PDF,
/// и сжатие их почти не уменьшает, а base64 шифротекста — ещё треть.
pub const PART_PLAIN_BYTES: usize = 94 << 20;

/// Шапка файла копии с замком и кавычками `data` — не длиннее: метка,
/// версии, момент, часть, соль до 64 байтов в base64, вектор — около
/// 400 знаков.
const SEALED_HEAD_BYTES: usize = 1 << 10;

/// Сколько байтов — не больше — займёт файл копии из `plain` байтов
/// открытого текста. Сжатие DEFLATE, которому нечего сжать, пишет блоки
/// «как есть» и прибавляет к ним по 5 байтов на 64 КиБ (RFC 1951, §3.2.4) —
/// здесь с запасом, по килобайту на мегабайт; потом метка подлинности шифра
/// и base64: три байта — четыре знака.
pub const fn sealed_bound(plain: usize) -> usize {
    let packed = plain + plain / 1024 + 1024;
    SEALED_HEAD_BYTES + (packed + TAG_BYTES).div_ceil(3) * 4
}

const _: () = assert!(sealed_bound(PART_PLAIN_BYTES) <= MAX_FILE_BYTES as usize);

/// Больше этого открытый текст копии не разжимается: разжатие
/// останавливается на пределе, а не после (Правило 13), — иначе
/// подложенный файл в килобайты разжался бы в гигабайты нулей и закрыл бы
/// страницу. Настоящий текст больше файла не намного: файлы вложений
/// в base64 сжимаются на четверть, а файл копии — сам base64 шифротекста.
/// Вдвое больше предела файла — с запасом на текст записей, который
/// сжимается в разы.
pub const MAX_PLAIN_BYTES: usize = 2 * MAX_FILE_BYTES as usize;

/// Сколько байтов работы между паузами у шагов `*_sliced`: полмегабайта —
/// около 12 мс сжатия в wasm на машине автора и до 80 мс с замедлением
/// процессора ×4 (замер 0.0.28; мегабайт там давал кадры по 160 мс):
/// страница стоит не дольше нескольких кадров, а пауз на копию в 20 МБ —
/// сотня.
pub const SLICE_BYTES: usize = 512 << 10;

/// Сколько весит в счётчике пауз запись короче этого: разбор мелкой записи
/// с её полями дороже, чем столько же байтов base64 файла, — 2 000 записей
/// книги по полкилобайта без этого шли одним куском почти в полсекунды
/// на телефоне (замер 0.0.28).
pub(super) const RECORD_WEIGHT: usize = 4 << 10;

/// Работа целиком, без пауз: будущее, которое ни разу не ждёт (`no_pause`),
/// готово с первого опроса.
pub(super) fn at_once<F: Future>(work: F) -> F::Output {
    let mut work = pin!(work);
    let mut context = Context::from_waker(Waker::noop());
    loop {
        if let Poll::Ready(output) = work.as_mut().poll(&mut context) {
            return output;
        }
    }
}

/// Пауза, которая не ждёт.
pub(super) fn no_pause() -> std::future::Ready<()> {
    std::future::ready(())
}

/// Счётчик работы: набрался кусок в [`SLICE_BYTES`] — пауза.
pub(super) struct Meter<'p, P> {
    pause: &'p mut P,
    done: usize,
}

impl<'p, P, F> Meter<'p, P>
where
    P: FnMut() -> F,
    F: Future<Output = ()>,
{
    pub(super) fn new(pause: &'p mut P) -> Self {
        Self { pause, done: 0 }
    }

    /// Сделано ещё `bytes` байтов работы.
    pub(super) async fn add(&mut self, bytes: usize) {
        self.done += bytes;
        if self.done >= SLICE_BYTES {
            self.done = 0;
            (self.pause)().await;
        }
    }
}

/// Уровень сжатия `miniz_oxide`: первый, самый быстрый. Замер 2026-10-15
/// (задача 14, машина автора, сборка `release`): копия в 22 МБ — 2 МБ
/// текста и 60 вложений по 250 КБ — на уровне 1 сжимается до 68,8 % за
/// 86 мс, на уровнях 3 и 6 — до 69,1 % за 465–478 мс; 2 МБ текста — до
/// 4,8 % за 2 мс и до 2,8–2,9 % за 7–9 мс. Файлы вложений уже сжаты
/// (JPEG, PDF), и более высокий уровень лишь дольше держит страницу.
const PACK_LEVEL: u8 = 1;

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
pub(super) const MAX_KDF_ITERATIONS: u32 = 10_000_000;

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
pub(super) const TAG_BYTES: usize = 16;

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
    check: fn(u32, &RawValue) -> Result<Checked, FormatError>,
    canon: fn(&str) -> Result<Checked, FormatError>,
}

impl Kind {
    /// Вид записей типа `T`.
    pub const fn of<T: Stored>() -> Self {
        Self {
            name: T::KIND,
            check: check::<T>,
            canon: canon::<T>,
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
    Kind::of::<Document>(),
    Kind::of::<Recurring>(),
    Kind::of::<Operation>(),
    Kind::of::<Attachment>(),
    Kind::of::<Closing>(),
    Kind::of::<License>(),
    Kind::of::<Opening>(),
    Kind::of::<IssuedLogo>(),
];

/// Виды записей конторы (задача 38): то, что копия всех ИП берёт из базы
/// конторы и кладёт обратно, — список ИП и ключи «Pro». В базу ИП они не идут
/// никогда.
pub const OFFICE_KINDS: &[Kind] = &[Kind::of::<Ip>(), Kind::of::<OfficeLicense>()];

/// Все виды, какие бывают в файле копии: записи баз ИП и записи конторы.
/// Файл разбирается этим списком, а где чему лежать, решает метка базы
/// ([`Entry::base`]). Ровно [`KINDS`] и [`OFFICE_KINDS`] вместе — это
/// держит тест: забытый здесь вид сделал бы файл всех ИП нечитаемым.
pub const FILE_KINDS: &[Kind] = &[
    Kind::of::<Profile>(),
    Kind::of::<Logo>(),
    Kind::of::<Client>(),
    Kind::of::<Document>(),
    Kind::of::<Recurring>(),
    Kind::of::<Operation>(),
    Kind::of::<Attachment>(),
    Kind::of::<Closing>(),
    Kind::of::<License>(),
    Kind::of::<Opening>(),
    Kind::of::<IssuedLogo>(),
    Kind::of::<Ip>(),
    Kind::of::<OfficeLicense>(),
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
///
/// Запись текущей версии ложится тем же текстом: разобрать её нужно —
/// это проверка, — а записывать заново незачем: что программа пишет, то
/// она и читает, слово в слово. Так файлы книги не перекодируются в base64
/// второй раз — у копии с файлами это была треть разбора (замер 0.0.28).
fn check<T: Stored>(version: u32, record: &RawValue) -> Result<Checked, FormatError> {
    let parsed: T = decode_record(version, record.get())?;
    moments_fit(parsed.meta())?;
    let json = if version == FORMAT_VERSION {
        record.to_owned()
    } else {
        serde_json::value::to_raw_value(&parsed).expect(
            "запись в JSON не падает: в типах записей нет ни словарей с ключами-структурами, ни своих ошибок",
        )
    };
    Ok(Checked {
        meta: parsed.meta().clone(),
        record: json,
    })
}

/// Моменты правки полей записи годятся (`format::fields`): общие поля
/// записи их не проверяют, а запись копии и кода — чужой ввод (Правило 13).
fn moments_fit(meta: &RecordMeta) -> Result<(), FormatError> {
    if fields::valid(meta) {
        Ok(())
    } else {
        Err(FormatError::Damaged("моменты правки полей негодные".to_owned()))
    }
}

/// Запись вида `T` текущей версии, собранная не программой, — слитая по
/// полям (`format::fields`): разобрать — проверка, что такая запись бывает,
/// а записать заново — тем же текстом, что дала бы команда: поля по порядку
/// типа. Иначе одна запись на двух устройствах лежала бы разными строками,
/// и слияние считало бы её изменённой.
fn canon<T: Stored>(record: &str) -> Result<Checked, FormatError> {
    let parsed: T = decode_record(FORMAT_VERSION, record)?;
    moments_fit(parsed.meta())?;
    Ok(Checked {
        meta: parsed.meta().clone(),
        record: serde_json::value::to_raw_value(&parsed).expect(
            "запись в JSON не падает: в типах записей нет ни словарей с ключами-структурами, ни своих ошибок",
        ),
    })
}

/// Сколько записей каких видов — для слов итога (задача 124, п. 21): «счета
/// и акты — 14, клиенты — 3, записи книги — 40» вместо «новых записей — 57».
/// Человек узнаёт четыре вида; остальное — профиль, логотипы, повторы,
/// закрытые кварталы, ключи, начальные остатки, ИП конторы — «прочее».
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Tally {
    /// Счета и акты.
    pub documents: usize,
    pub clients: usize,
    /// Записи книги доходов и расходов.
    pub ledger: usize,
    /// Файлы книги.
    pub files: usize,
    pub other: usize,
}

impl Tally {
    /// Записи по видам.
    pub fn of<'a>(entries: impl IntoIterator<Item = &'a Entry>) -> Self {
        let mut tally = Self::default();
        for entry in entries {
            let slot = match entry.kind() {
                kind if kind == Document::KIND => &mut tally.documents,
                kind if kind == Client::KIND => &mut tally.clients,
                kind if kind == Operation::KIND => &mut tally.ledger,
                kind if kind == Attachment::KIND => &mut tally.files,
                _ => &mut tally.other,
            };
            *slot += 1;
        }
        tally
    }

    /// Всего записей.
    pub const fn total(&self) -> usize {
        self.documents + self.clients + self.ledger + self.files + self.other
    }

    /// Сумма двух — у копии всех ИП и у цепочки частей.
    #[must_use]
    pub const fn plus(self, other: Self) -> Self {
        Self {
            documents: self.documents + other.documents,
            clients: self.clients + other.clients,
            ledger: self.ledger + other.ledger,
            files: self.files + other.files,
            other: self.other + other.other,
        }
    }
}

/// Запись копии: разобрана своим видом и записана в текущей версии формата.
#[derive(Clone, Debug)]
pub struct Entry {
    kind: &'static str,
    meta: RecordMeta,
    record: Box<RawValue>,
    base: Option<RecordId>,
}

impl Entry {
    /// Вид записи.
    pub const fn kind(&self) -> &'static str {
        self.kind
    }

    /// Чья база — у копии всех ИП: идентификатор записи ИП конторы
    /// (`model::office::Ip`). `None` — копия одного ИП или сама запись
    /// конторы.
    pub const fn base(&self) -> Option<RecordId> {
        self.base
    }

    /// Та же запись с меткой базы ИП `ip` — для копии всех ИП.
    pub fn in_base(self, ip: RecordId) -> Self {
        Self {
            base: Some(ip),
            ..self
        }
    }

    /// Та же запись без метки — для своей базы.
    pub(crate) fn untagged(self) -> Self {
        Self { base: None, ..self }
    }

    /// Общие поля записи.
    pub const fn meta(&self) -> &RecordMeta {
        &self.meta
    }

    /// Строка для базы — та же, что дал бы `format::encode`.
    pub fn stored(&self) -> String {
        wrap(self.record.get())
    }

    /// Поля записи в JSON текущей версии — без конверта.
    pub(crate) fn record(&self) -> &str {
        self.record.get()
    }

    /// То же — куском JSON: так запись ложится в код передачи
    /// (`format::sync`).
    pub(super) fn raw(&self) -> &RawValue {
        &self.record
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
pub(super) fn read_stored(kinds: &[Kind], raw: Raw<'_>) -> Result<Entry, FormatError> {
    let text = raw
        .text
        .ok_or_else(|| FormatError::Damaged("значение не строка".to_owned()))?;
    // Версия — раньше вида: незнакомый вид новее программы — это запись
    // новой версии, а не порча.
    let (version, record) = split_raw(text)?;
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
        base: None,
    })
}

/// Запись вида `kind` версии формата `version` из чужого текста — разобранная
/// своим видом и в текущей версии, без метки базы: так её читает код
/// передачи (`format::sync`). Вид, которого нет в `kinds`, — порча.
pub(super) fn entry_of(kinds: &[Kind], kind: &str, version: u32, record: &RawValue) -> Result<Entry, FormatError> {
    let found = find(kinds, kind).ok_or_else(|| FormatError::Damaged(format!("вид «{kind}» программе незнаком")))?;
    let checked = (found.check)(version, record)?;
    Ok(Entry {
        kind: found.name,
        meta: checked.meta,
        record: checked.record,
        base: None,
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

    /// Записи и что не вошло — без копирования: копия всех ИП складывает
    /// записи всех баз в один файл (задача 38), и с файлами книги это
    /// десятки мегабайт.
    pub fn into_parts(self) -> (Vec<Entry>, Vec<String>) {
        (self.entries, self.unreadable)
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
    at_once(collect_sliced(kinds, raw, &mut no_pause))
}

/// [`collect`] кусками: пауза — после каждого мегабайта разобранных записей.
pub async fn collect_sliced<'a, P, F>(
    kinds: &[Kind],
    raw: impl IntoIterator<Item = Raw<'a>>,
    pause: &mut P,
) -> Result<Collected, FormatError>
where
    P: FnMut() -> F,
    F: Future<Output = ()>,
{
    let mut meter = Meter::new(pause);
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
        meter.add(item.text.map_or(0, str::len).max(RECORD_WEIGHT)).await;
    }
    Ok(Collected {
        entries,
        unreadable,
    })
}

/// Идентификатор одной копии, сделанной частями: общий у всех её частей
/// (задача 117). По нему окно узнаёт, что две части — одной копии.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CopyId(uuid::Uuid);

impl CopyId {
    /// Новый: та же случайность, что у идентификаторов записей
    /// (`RecordId::random`).
    fn random() -> Self {
        Self(uuid::Uuid::new_v4())
    }
}

/// Часть копии (задача 117): какая из скольких и какой копии. В шапке файла
/// — `"part":{"copy":"…","number":2,"of":3}`; у копии одним файлом поля нет.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Part {
    copy: CopyId,
    number: u16,
    of: u16,
}

impl Part {
    /// Копия, чья это часть.
    pub const fn copy(&self) -> CopyId {
        self.copy
    }

    /// Номер части — с единицы.
    pub const fn number(&self) -> u16 {
        self.number
    }

    /// Сколько частей у копии — две и больше.
    pub const fn of(&self) -> u16 {
        self.of
    }

    /// Годится ли часть: номер от единицы до числа частей, частей — от двух
    /// до [`MAX_PARTS`]. Шапка — чужой ввод (Правило 13).
    const fn fits(&self) -> bool {
        self.number >= 1 && self.number <= self.of && self.of >= 2 && self.of <= MAX_PARTS
    }
}

/// Имя файла части: `kvartik-2026-11-23.json` — `kvartik-2026-11-23-1of3.json`;
/// у копии одним файлом — само имя. Номер — в конце, перед расширением: по
/// имени человек видит, сколько файлов у копии и какой из них какой.
pub fn part_file_name(name: &str, part: Option<Part>) -> String {
    match (part, name.strip_suffix(".json")) {
        (Some(part), Some(stem)) => format!("{stem}-{}.json", part_mark(part.number, part.of)),
        _ => name.to_owned(),
    }
}

/// Метка части в имени файла — `1of3`: по ней окно называет человеку части,
/// которых не хватает (задача 123), той же записью, что [`part_file_name`].
pub fn part_mark(number: u16, of: u16) -> String {
    format!("{number}of{of}")
}

/// Номер части и их число из конца имени, как его пишет [`part_file_name`]:
/// `1of3`. `None` — не по образцу: с нулём впереди, номер больше числа,
/// частей меньше двух или больше [`MAX_PARTS`].
pub(super) fn part_of_name(text: &str) -> Option<(u16, u16)> {
    let (number, of) = text.split_once("of")?;
    let read = |digits: &str| {
        let plain = !digits.is_empty() && !digits.starts_with('0') && digits.bytes().all(|byte| byte.is_ascii_digit());
        plain.then(|| digits.parse::<u16>().ok()).flatten()
    };
    let (number, of) = (read(number)?, read(of)?);
    (number <= of && (2..=MAX_PARTS).contains(&of)).then_some((number, of))
}

/// Шапка копии: какой версией программы и когда сделана, а у копии
/// частями — какая это часть.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Header {
    format_version: u32,
    app_version: String,
    created: Timestamp,
    part: Option<Part>,
}

impl Header {
    /// Шапка новой копии: версия формата — этой программы.
    pub fn new(app_version: &str, created: Timestamp) -> Self {
        Self {
            format_version: FORMAT_VERSION,
            app_version: app_version.to_owned(),
            created,
            part: None,
        }
    }

    /// Та же шапка — у части `part`.
    fn with_part(&self, part: Part) -> Self {
        Self {
            part: Some(part),
            ..self.clone()
        }
    }

    /// Какая это часть копии; `None` — копия одним файлом.
    pub const fn part(&self) -> Option<Part> {
        self.part
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
    #[serde(skip_serializing_if = "Option::is_none")]
    part: Option<Part>,
    records: Vec<RecordOut<'a>>,
}

#[derive(Serialize)]
struct RecordOut<'a> {
    kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    base: Option<RecordId>,
    record: &'a RawValue,
}

/// Открытый текст копии: шапка и записи — JSON, ещё не сжатый ([`pack`]).
/// Записи — все записи копии или записи одной её части ([`split`]).
pub fn plaintext<'e>(header: &Header, entries: impl IntoIterator<Item = &'e Entry>) -> String {
    serde_json::to_string(&PlainOut {
        file: FILE_MARK,
        format_version: header.format_version,
        app_version: &header.app_version,
        created: header.created,
        part: header.part,
        records: entries
            .into_iter()
            .map(|entry| RecordOut {
                kind: entry.kind,
                base: entry.base,
                record: &entry.record,
            })
            .collect(),
    })
    .expect("копия в JSON не падает: записи уже в JSON, остальное — строки и числа")
}

/// То, что окно шифрует: открытый текст копии ([`plaintext`]), сжатый
/// DEFLATE (RFC 1951) — так пишет версия формата 6 и новее.
pub fn pack<'e>(header: &Header, entries: impl IntoIterator<Item = &'e Entry>) -> Vec<u8> {
    at_once(pack_sliced(header, entries, &mut no_pause))
}

/// [`pack`] кусками: пауза — после каждого мегабайта сжатого текста.
pub async fn pack_sliced<'e, P, F>(header: &Header, entries: impl IntoIterator<Item = &'e Entry>, pause: &mut P) -> Vec<u8>
where
    P: FnMut() -> F,
    F: Future<Output = ()>,
{
    deflate_sliced(plaintext(header, entries).as_bytes(), PACK_LEVEL, pause).await
}

/// Сколько байтов открытого текста — не больше — занимает запись вокруг
/// своих полей: `{"kind":"…","base":"…","record":…},` — вид, метка базы
/// и кавычки.
const RECORD_FRAME_BYTES: usize = 128;

/// Шапка открытого текста — не длиннее: метка, версии, момент и часть.
const PLAIN_HEAD_BYTES: usize = 512;

/// Сколько байтов — не больше — запись займёт в открытом тексте копии.
fn plain_bytes(entry: &Entry) -> usize {
    entry.record.get().len() + RECORD_FRAME_BYTES
}

/// Запись, которая идёт в каждую часть копии: по ней восстановление узнаёт
/// часть (шапка модуля, «Частями»). Профиль — ни один ли это ИП
/// ([`relation`]), ИП конторы — куда лечь записям с меткой его базы
/// ([`whole`]). Их мало и они маленькие.
fn in_every_part(entry: &Entry) -> bool {
    entry.kind == Profile::KIND || entry.kind == Ip::KIND
}

/// Копия, разложенная по частям (задача 117): шапка и записи каждой. Влезает
/// вся в `limit` байтов открытого текста — одна часть с шапкой как есть,
/// без номера, и файл тот же, что до частей. Не влезает — частей несколько:
/// в каждой записи [`in_every_part`], потом записи без файлов книги, потом
/// файлы книги, всё — в порядке копии, пока часть не наберёт `limit`. Записей
/// без файлов больше `limit` — у конторы на десятки ИП — и граница части
/// пройдёт и между ними: восстановивший одну часть до остальных выпускать
/// не должен, и окно это говорит (ревью задачи 117).
/// Запись, которая и одна больше `limit`, ложится частью сама — и даёт файл
/// больше [`MAX_FILE_BYTES`], который окно не отдаст; у программы таких нет:
/// файл книги — не больше 2 МБ (`model::attachment::MAX_BYTES`).
///
/// `limit` окна — [`PART_PLAIN_BYTES`]; тесты делят мелко.
pub fn split<'e>(header: &Header, entries: &'e [Entry], limit: usize) -> Vec<(Header, Vec<&'e Entry>)> {
    let total = entries.iter().map(plain_bytes).fold(PLAIN_HEAD_BYTES, usize::saturating_add);
    if total <= limit {
        return vec![(header.clone(), entries.iter().collect())];
    }
    let shared: Vec<&Entry> = entries.iter().filter(|entry| in_every_part(entry)).collect();
    let shared_bytes = shared.iter().map(|entry| plain_bytes(entry)).fold(PLAIN_HEAD_BYTES, usize::saturating_add);
    let file = |entry: &&Entry| entry.kind == Attachment::KIND;
    let rest = entries
        .iter()
        .filter(|entry| !in_every_part(entry) && !file(entry))
        .chain(entries.iter().filter(file));
    let mut parts: Vec<Vec<&Entry>> = Vec::new();
    let mut part = shared.clone();
    let mut used = shared_bytes;
    for entry in rest {
        let bytes = plain_bytes(entry);
        // В части уже есть своё, кроме общего, — новая часть.
        if used.saturating_add(bytes) > limit && part.len() > shared.len() {
            parts.push(std::mem::replace(&mut part, shared.clone()));
            used = shared_bytes;
        }
        part.push(entry);
        used = used.saturating_add(bytes);
    }
    parts.push(part);
    let of = u16::try_from(parts.len()).unwrap_or(u16::MAX);
    if of < 2 {
        return vec![(header.clone(), entries.iter().collect())];
    }
    let copy = CopyId::random();
    (1..=of)
        .zip(parts)
        .map(|(number, entries)| (header.with_part(Part { copy, number, of }), entries))
        .collect()
}

/// Сжатие DEFLATE без обёртки (RFC 1951) уровнем `level` кусками по
/// [`SLICE_BYTES`] — поток один, и разжимается он целиком, как сжатый одним
/// куском.
pub(super) async fn deflate_sliced<P, F>(input: &[u8], level: u8, pause: &mut P) -> Vec<u8>
where
    P: FnMut() -> F,
    F: Future<Output = ()>,
{
    use miniz_oxide::deflate::core::{
        CompressorOxide, TDEFLFlush, TDEFLStatus, compress, create_comp_flags_from_zip_params,
    };
    // Окно 0 — без обёртки zlib, как у `compress_to_vec`.
    let flags = create_comp_flags_from_zip_params(i32::from(level), 0, 0);
    let mut compressor = CompressorOxide::new(flags);
    let mut output = vec![0; input.len() / 2 + 1024];
    let mut written = 0;
    let mut meter = Meter::new(pause);
    let mut rest = input;
    loop {
        let (mut chunk, after) = rest.split_at(rest.len().min(SLICE_BYTES));
        let last = after.is_empty();
        let flush = if last { TDEFLFlush::Finish } else { TDEFLFlush::None };
        let taken = chunk.len();
        loop {
            if output.len() - written < 1024 {
                output.resize(output.len() * 2 + 1024, 0);
            }
            let (status, used, made) = compress(&mut compressor, chunk, &mut output[written..], flush);
            chunk = &chunk[used..];
            written += made;
            match status {
                TDEFLStatus::Done => {
                    output.truncate(written);
                    return output;
                }
                TDEFLStatus::Okay if chunk.is_empty() && !last => break,
                // Нужно больше места — или конец ещё не записан.
                TDEFLStatus::Okay => {}
                // С постоянными параметрами и без своего обработчика вывода
                // этого не бывает — так считает и `compress_to_vec`.
                TDEFLStatus::BadParam | TDEFLStatus::PutBufFailed => {
                    unreachable!("сжатие с постоянными параметрами не отказывает")
                }
            }
        }
        rest = after;
        meter.add(taken).await;
    }
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
    #[serde(skip_serializing_if = "Option::is_none")]
    part: Option<Part>,
    kdf: KdfFields,
    cipher: CipherFields,
    data: String,
}

/// Файл копии: шапка, замок и шифротекст.
pub fn seal(header: &Header, lock: &Lock, ciphertext: &[u8]) -> String {
    at_once(seal_sliced(header, lock, ciphertext, &mut no_pause))
}

/// [`seal`] кусками: base64 шифротекста — по мегабайту с паузой.
pub async fn seal_sliced<P, F>(header: &Header, lock: &Lock, ciphertext: &[u8], pause: &mut P) -> String
where
    P: FnMut() -> F,
    F: Future<Output = ()>,
{
    let mut file = serde_json::to_string(&SealedOut {
        file: FILE_MARK,
        format_version: header.format_version,
        app_version: &header.app_version,
        created: header.created,
        part: header.part,
        kdf: KdfFields {
            name: KDF_NAME.to_owned(),
            iterations: lock.iterations,
            salt: base64::encode(&lock.salt),
        },
        cipher: CipherFields {
            name: CIPHER_NAME.to_owned(),
            iv: base64::encode(&lock.iv),
        },
        data: String::new(),
    })
    .expect("файл копии в JSON не падает: в нём только строки и числа");
    // Данные встают в конец, между кавычками `"data":""}`, а не идут через
    // serde: тот прошёл бы по десяткам мегабайт ещё раз, ища, что
    // экранировать, а в base64 нет ни кавычек, ни обратной косой черты.
    let tail = file.split_off(file.len() - 2);
    debug_assert_eq!(tail, "\"}");
    file.reserve(ciphertext.len().div_ceil(3) * 4 + tail.len());
    // Куски — по три байта целиком: base64 кусков подряд — base64 целого.
    let piece = SLICE_BYTES / 3 * 3;
    let mut meter = Meter::new(pause);
    for chunk in ciphertext.chunks(piece) {
        file.push_str(&base64::encode(chunk));
        meter.add(chunk.len()).await;
    }
    file.push_str(&tail);
    file
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
    #[serde(default)]
    part: Option<Part>,
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
    at_once(open_sliced(text, &mut no_pause))
}

/// [`open`] кусками: шифротекст из base64 — по мегабайту с паузой.
pub async fn open_sliced<P, F>(text: &str, pause: &mut P) -> Result<Sealed, BackupError>
where
    P: FnMut() -> F,
    F: Future<Output = ()>,
{
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
        // Шапка, замок и список записей у версий 1–10 одинаковы: версии 2–10
        // прибавили виды записей и поля, а их разбирает `decode_record` по
        // версии файла; сжат ли текст, `read` узнаёт по ней же.
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
    part_fits(fields.format_version, fields.part)?;
    let salt = base64::decode(&fields.kdf.salt).ok_or_else(|| damaged("соль — не base64"))?;
    let iv = base64::decode(&fields.cipher.iv).ok_or_else(|| damaged("вектор — не base64"))?;
    let lock = Lock::new(fields.kdf.iterations, salt, iv)
        .ok_or_else(|| damaged("параметры шифра за пределами допустимого"))?;
    let ciphertext = decode_sliced(&fields.data, pause)
        .await
        .ok_or_else(|| damaged("данные — не base64"))?;
    if ciphertext.len() < TAG_BYTES {
        return Err(damaged("данные короче метки подлинности"));
    }
    Ok(Sealed {
        header: Header {
            format_version: fields.format_version,
            app_version: fields.app_version,
            created: fields.created,
            part: fields.part,
        },
        lock,
        ciphertext,
    })
}

/// Байты из base64 кусками по четыре знака целиком, с паузой после
/// каждого мегабайта. Выравнивание `=` бывает только в конце всего текста,
/// а не в конце куска: иначе подправленный файл читался бы «как-нибудь»
/// (Правило 13). `None` — не base64.
async fn decode_sliced<P, F>(text: &str, pause: &mut P) -> Option<Vec<u8>>
where
    P: FnMut() -> F,
    F: Future<Output = ()>,
{
    if !text.len().is_multiple_of(4) || !text.is_ascii() {
        return None;
    }
    let piece = SLICE_BYTES / 4 * 4;
    let mut bytes = Vec::with_capacity(text.len() / 4 * 3);
    let mut meter = Meter::new(pause);
    let mut rest = text;
    while !rest.is_empty() {
        let (chunk, after) = rest.split_at(rest.len().min(piece));
        if !after.is_empty() && chunk.ends_with('=') {
            return None;
        }
        bytes.extend(base64::decode(chunk)?);
        meter.add(chunk.len()).await;
        rest = after;
    }
    Some(bytes)
}

/// Часть в шапке годится: есть только с версии 27 и номером не больше
/// числа частей ([`Part::fits`]).
fn part_fits(format_version: u32, part: Option<Part>) -> Result<(), BackupError> {
    match part {
        None => Ok(()),
        Some(_) if format_version < PARTS_SINCE => Err(damaged("часть копии в шапке версии до 27")),
        Some(part) if part.fits() => Ok(()),
        Some(part) => Err(damaged(format!("часть {} из {}: так не бывает", part.number, part.of))),
    }
}

/// Начало каждого файла копии: так его пишет [`seal`].
const MARK_PREFIX: &str = r#"{"file":"kvartik-backup""#;

/// Похоже ли на номер версии программы: `0.0.8`, `1.2.0-beta.1`. Шапку
/// окно показывает человеку, и в ней не должно быть чего угодно.
pub(super) fn is_app_version(text: &str) -> bool {
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
    #[serde(default)]
    part: Option<Part>,
    #[serde(borrow)]
    records: Vec<RecordIn<'a>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RecordIn<'a> {
    kind: String,
    #[serde(default)]
    base: Option<RecordId>,
    #[serde(borrow)]
    record: &'a RawValue,
}

/// Записи расшифрованной копии — каждая разобрана своим видом. `opened` —
/// то, что дала расшифровка: у версии 6 и новее — сжатый текст ([`pack`]),
/// у прежних — сам текст.
///
/// Всё или ничего: одна испорченная запись, вид, которого программа не
/// знает, или две записи с одним идентификатором — и не читается вся копия.
/// Половина копии в базе выглядела бы восстановленной, а не была бы ею.
pub fn read(kinds: &[Kind], sealed: &Sealed, opened: &[u8]) -> Result<Vec<Entry>, BackupError> {
    at_once(read_sliced(kinds, sealed, opened, &mut no_pause))
}

/// [`read`] кусками: разжатие и разбор записей — по мегабайту с паузой.
pub async fn read_sliced<P, F>(
    kinds: &[Kind],
    sealed: &Sealed,
    opened: &[u8],
    pause: &mut P,
) -> Result<Vec<Entry>, BackupError>
where
    P: FnMut() -> F,
    F: Future<Output = ()>,
{
    let unpacked;
    let plaintext = if sealed.header.format_version >= PACKED_SINCE {
        unpacked = unpack_sliced(opened, MAX_PLAIN_BYTES, pause).await?;
        unpacked.as_slice()
    } else {
        opened
    };
    let text = std::str::from_utf8(plaintext).map_err(damaged)?;
    let plain: PlainIn<'_> = serde_json::from_str(text).map_err(damaged)?;
    let inner = Header {
        format_version: plain.format_version,
        app_version: plain.app_version,
        created: plain.created,
        part: plain.part,
    };
    if plain.file != FILE_MARK || inner != sealed.header {
        return Err(damaged("шапка снаружи не совпадает с шапкой внутри"));
    }
    let mut seen = BTreeSet::new();
    let mut entries = Vec::with_capacity(plain.records.len());
    let mut meter = Meter::new(pause);
    for item in plain.records {
        meter.add(item.record.get().len().max(RECORD_WEIGHT)).await;
        let kind = find(kinds, &item.kind)
            .ok_or_else(|| damaged(format!("вид «{}» программе незнаком", item.kind)))?;
        let checked = (kind.check)(inner.format_version, item.record)?;
        // Дважды — в одной базе: одна запись в двух базах ИП бывает — копию
        // одного ИП слили «всё равно» в другой.
        if !seen.insert((item.base, kind.name, checked.meta.id())) {
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
            base: item.base,
        });
    }
    whole(&entries)?;
    Ok(entries)
}

/// Копия устроена как одна из двух ([`Entry::base`]): копия одного ИП — без
/// меток базы и без записей конторы, а копия всех ИП — записи конторы
/// ([`OFFICE_KINDS`]: ИП списка и ключи «Pro») без метки и все остальные
/// с меткой своего ИП из этой же копии. Иначе — порча: запись без своего ИП
/// некуда положить, а запись конторы в базе ИП — чужая.
pub(super) fn whole(entries: &[Entry]) -> Result<(), BackupError> {
    let ips: BTreeSet<RecordId> = entries
        .iter()
        .filter(|entry| entry.kind == Ip::KIND)
        .map(|entry| entry.meta.id())
        .collect();
    let office = |entry: &Entry| OFFICE_KINDS.iter().any(|kind| kind.name == entry.kind);
    let full = entries.iter().any(|entry| office(entry) || entry.base.is_some());
    for entry in entries {
        let fits = match (office(entry), entry.base) {
            (true, base) => base.is_none(),
            (false, Some(base)) => ips.contains(&base),
            (false, None) => !full,
        };
        if !fits {
            return Err(damaged(format!(
                "запись {} {} не на своём месте в копии",
                entry.kind,
                entry.meta.id()
            )));
        }
    }
    Ok(())
}

/// Сжатый текст копии ([`pack`]), разжатый не больше чем до `limit`
/// байтов: дальше разжатие останавливается, и текст — порча.
#[cfg(test)]
fn unpack(packed: &[u8], limit: usize) -> Result<Vec<u8>, BackupError> {
    at_once(unpack_sliced(packed, limit, &mut no_pause))
}

/// Разжатие потоком, с паузой после каждого мегабайта разжатого текста
/// и с пределом: перешагнул — порча, а не гигабайты в памяти (Правило 13).
/// Лишние байты после конца сжатого потока — тоже порча: программа их не
/// пишет.
pub(super) async fn unpack_sliced<P, F>(packed: &[u8], limit: usize, pause: &mut P) -> Result<Vec<u8>, BackupError>
where
    P: FnMut() -> F,
    F: Future<Output = ()>,
{
    use miniz_oxide::inflate::stream::{InflateState, inflate};
    use miniz_oxide::{DataFormat, MZFlush, MZStatus};
    let mut state = InflateState::new_boxed(DataFormat::Raw);
    let mut output = Vec::new();
    let mut window = vec![0; 1 << 16];
    let mut input = packed;
    let mut meter = Meter::new(pause);
    loop {
        let result = inflate(&mut state, input, &mut window, MZFlush::None);
        input = &input[result.bytes_consumed..];
        if output.len() + result.bytes_written > limit {
            return Err(damaged("сжатый текст больше предела"));
        }
        output.extend_from_slice(&window[..result.bytes_written]);
        match result.status {
            Ok(MZStatus::StreamEnd) => break,
            Ok(MZStatus::Ok) => {}
            Ok(MZStatus::NeedDict) | Err(_) => {
                return Err(damaged(format!("сжатый текст не разжимается: {:?}", result.status)));
            }
        }
        meter.add(result.bytes_written).await;
    }
    if !input.is_empty() {
        return Err(damaged("после сжатого текста — лишние байты"));
    }
    Ok(output)
}

/// Что слияние копии с базой запишет.
#[derive(Debug)]
pub struct Merge<'a> {
    /// Записи копии как есть — или слитые по полям с записями базы
    /// (`format::fields`): таких копия не несёт, они новые.
    writes: Vec<Cow<'a, Entry>>,
    added: usize,
    updated: usize,
    unchanged: usize,
}

impl Merge<'_> {
    /// Записи, которые лягут в базу.
    pub fn writes(&self) -> impl Iterator<Item = &Entry> {
        self.writes.iter().map(|entry| &**entry)
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
/// Из двух записей с одним видом и идентификатором каждое поле берётся
/// с той стороны, где его правили позже (`format::fields`, задача 116); при
/// равенстве — у той, что в базе. У записей без моментов полей все поля
/// правлены в момент изменения записи, и остаётся целиком изменённая позже
/// (`RecordMeta::modified`), как до 0.0.171; так же — у файлов книги
/// и логотипов (`fields::WHOLE`) и когда слитую запись её вид не читает.
/// Выпущенный документ остаётся и против черновика, изменённого позже
/// ([`issued_wins`]). Нечитаемую запись базы копия чинит: «восстановите из резервной копии» —
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
    // Начало строки базы у записи текущей версии: `{"format_version":10,"record":`.
    let empty = wrap("");
    let prefix = &empty[..empty.len() - 1];
    for entry in incoming {
        let id = entry.meta.id().to_string();
        let Some(&raw) = existing.get(&(entry.kind, id.as_str())) else {
            merge.added += 1;
            merge.writes.push(Cow::Borrowed(entry));
            continue;
        };
        // Слово в слово та же запись — без изменений, без разбора: разбор
        // идёт внутри транзакции записи, а там паузы быть не может
        // (Правило 6), и копия поверх той же базы разбирала бы все её
        // файлы одним куском.
        let same = raw
            .text
            .and_then(|text| text.strip_prefix(prefix))
            .and_then(|text| text.strip_suffix('}'))
            == Some(entry.record.get());
        if same {
            merge.unchanged += 1;
            continue;
        }
        let chosen = match read_stored(kinds, raw) {
            Ok(stored) => match issued_wins(entry, &stored) {
                Some(true) => Some(Cow::Borrowed(entry)),
                Some(false) => None,
                None => by_fields(kinds, &stored, entry).unwrap_or_else(|| {
                    (entry.meta.modified() > stored.meta.modified()).then_some(Cow::Borrowed(entry))
                }),
            },
            Err(FormatError::Newer { found }) => return Err(FormatError::Newer { found }),
            Err(FormatError::Damaged(_)) => Some(Cow::Borrowed(entry)),
        };
        match chosen {
            Some(write) => {
                merge.updated += 1;
                merge.writes.push(write);
            }
            None => merge.unchanged += 1,
        }
    }
    Ok(merge)
}

/// Запись копии `incoming`, слитая по полям с записью базы `stored`
/// (`format::fields`): `Some(None)` — в базе уже то же, `Some(Some(…))` —
/// что записать: запись копии как есть или слитую; `None` — по полям не
/// слить, и решают моменты записей целиком (шапка `fields`, «Чего слияние по
/// полям не делает»).
fn by_fields<'a>(kinds: &[Kind], stored: &Entry, incoming: &'a Entry) -> Option<Option<Cow<'a, Entry>>> {
    if fields::WHOLE.contains(&incoming.kind) {
        return None;
    }
    let kind = find(kinds, incoming.kind)?;
    let (_, text) = fields::merge(incoming.kind, stored.record(), &stored.meta, incoming.record(), &incoming.meta)?;
    let merged = (kind.canon)(&text).ok()?;
    Some(if merged.record.get() == stored.record() {
        None
    } else if merged.record.get() == incoming.record() {
        Some(Cow::Borrowed(incoming))
    } else {
        Some(Cow::Owned(Entry {
            kind: kind.name,
            meta: merged.meta,
            record: merged.record,
            base: incoming.base,
        }))
    })
}

/// Выпущенный документ против того же документа черновиком (задача 41):
/// `Some(true)` — остаётся запись копии, `Some(false)` — базы, `None` —
/// решают моменты. Выпуск у черновика не отнять: черновик, ушедший копией на
/// устройство, которое выпускает, и выпущенный там, на прежнем устройстве
/// правят или удаляют дальше, и его поздний момент вернул бы выпущенный счёт
/// в черновики — без номера, копии реквизитов и дохода, а номер выдался бы
/// снова (ревью задачи 41). Правки черновика после выпуска тогда пропадают:
/// выпущенный не правится (Правило 2). Два разных выпуска одного черновика —
/// на двух устройствах, где выпуск не выключили, — решают моменты записи
/// целиком (ревью 0.0.171): номер и реквизиты одного выпуска со строками
/// другого были бы документом, которого никто не выпускал. Один и тот же
/// выпуск с правками после него — по полям (`None`).
fn issued_wins(incoming: &Entry, stored: &Entry) -> Option<bool> {
    if incoming.kind != Document::KIND {
        return None;
    }
    let issued = |entry: &Entry| {
        decode_record::<Document>(FORMAT_VERSION, entry.record.get())
            .ok()
            .map(|document| document.issued().cloned())
    };
    match (issued(incoming)?, issued(stored)?) {
        (Some(_), None) => Some(true),
        (None, Some(_)) => Some(false),
        (Some(theirs), Some(ours)) if theirs != ours => Some(incoming.meta.modified() > stored.meta.modified()),
        (Some(_), Some(_)) | (None, None) => None,
    }
}

/// Сколько записей вида `kind` станет в базе после слияния `merge` с её
/// записями `existing` — и удалённых, и нечитаемых: для пределов накопителей,
/// которые копия могла бы переполнить (`license::MAX_KEYS`, Правило 1).
pub fn count_after<'b>(kind: &str, existing: impl IntoIterator<Item = Raw<'b>>, merge: &Merge<'_>) -> usize {
    let ids: BTreeSet<&str> = existing
        .into_iter()
        .filter(|raw| raw.kind == kind)
        .map(|raw| raw.id)
        .collect();
    let added = merge
        .writes()
        .filter(|entry| entry.kind == kind && !ids.contains(entry.meta.id().to_string().as_str()))
        .count();
    ids.len() + added
}

/// Насколько правка в копии может быть позже «сейчас» этого устройства,
/// прежде чем окно скажет о часах (задача 41). Копию делают раньше, чем
/// открывают, и правок позже «сейчас» в ней не бывает, когда часы двух
/// устройств сходятся; часы, сверенные по сети, расходятся на секунды,
/// а поставленные вручную — на минуты, и их окно не тревожит.
pub const CLOCK_SLACK_MILLIS: i64 = 10 * 60 * 1000;

/// Есть ли в копии правки позже `now` больше чем на [`CLOCK_SLACK_MILLIS`]:
/// часы одного из устройств идут неверно (задача 41; CLAUDE.md, Правило 6,
/// «Часы устройства могут врать»). Слиянию это не мешает, но из двух правок
/// одной записи остаётся изменённая позже по часам её устройства
/// ([`merge`]), и правка с часов, которые спешат, перебьёт сделанную после
/// неё на другом устройстве, — поэтому окно говорит это словами.
pub fn ahead(incoming: &[Entry], now: Timestamp) -> bool {
    let limit = now.unix_millis().saturating_add(CLOCK_SLACK_MILLIS);
    incoming.iter().any(|entry| entry.meta.modified().unix_millis() > limit)
}

/// Сколько весят в копии файлы книги (задача 64): байты всех вложений
/// (`model::attachment`) и среди них — убранных из записей. Убранный файл
/// помечен удалённым, а не стёрт (CLAUDE.md, «Записи не стираются»), и копия
/// несёт его тоже: убрать файл из записи её не уменьшит. Больше всего места
/// в копии — у них, и предел файла копии ([`MAX_FILE_BYTES`]) ближе всего
/// тому, у кого их много.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Files {
    /// Байты всех файлов книги.
    pub bytes: u64,
    /// Из них — убранных из записей.
    pub removed: u64,
}

/// Файлы книги в записях копии ([`Files`]). Байты — по длине base64 в записи,
/// без разбора самих файлов: они уже проверены при сборке
/// (`model::attachment::FileBytes`), а разбирать десятки мегабайт ради
/// числа незачем (Правило 1).
pub fn files(entries: &[Entry]) -> Files {
    /// Из записи вложения — только base64 файла, без копии.
    #[derive(Deserialize)]
    struct Only<'a> {
        #[serde(borrow)]
        file: std::borrow::Cow<'a, str>,
    }
    let mut files = Files::default();
    for entry in entries.iter().filter(|entry| entry.kind == Attachment::KIND) {
        let Ok(Only { file }) = serde_json::from_str::<Only<'_>>(entry.record.get()) else {
            continue;
        };
        let bytes = base64_bytes(&file);
        files.bytes = files.bytes.saturating_add(bytes);
        if entry.meta.is_deleted() {
            files.removed = files.removed.saturating_add(bytes);
        }
    }
    files
}

/// Сколько байтов в base64 `text` с дополнением `=` (RFC 4648, §4): из
/// четырёх знаков — три байта, без знаков дополнения.
fn base64_bytes(text: &str) -> u64 {
    let padding = text.bytes().rev().take(2).take_while(|&byte| byte == b'=').count();
    ((text.len() / 4 * 3).saturating_sub(padding)) as u64
}

/// Чей ИП в копии или в базе — чтобы показать человеку, что с чем он
/// сливает: имя и ՀՎՀՀ живого профиля, изменённого последним, — того, с
/// которым работает окно (`Profile::current`).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Owner {
    name: Localized<Line>,
    tin: Option<Tin>,
}

impl Owner {
    fn of(profile: &Profile) -> Self {
        Self {
            name: profile.data().name.clone(),
            tin: profile.data().tin.clone(),
        }
    }

    /// Имя на языке `lang`, а без него — на первом языке, где оно есть;
    /// пусто — имени нет ни на одном.
    pub fn name(&self, lang: Lang) -> &str {
        std::iter::once(lang)
            .chain(Lang::ALL)
            .map(|lang| self.name.get(lang).as_str())
            .find(|name| !name.is_empty())
            .unwrap_or_default()
    }

    /// ՀՎՀՀ; `None` — в профиле не указан.
    pub const fn tin(&self) -> Option<&Tin> {
        self.tin.as_ref()
    }
}

/// Об одном ли ИП копия и база.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Relation {
    /// Об одном — или сравнивать не с чем: профиля нет у базы или живого
    /// профиля — у копии.
    Same,
    /// О разных: профили есть у обеих сторон, а общего нет — ни записи
    /// профиля, ни ՀՎՀՀ.
    Other {
        /// ИП базы; `None` — профиль в базе есть, но не читается.
        ours: Option<Owner>,
        /// ИП копии.
        theirs: Owner,
    },
}

/// Об одном ли ИП копия `incoming` и база `existing` (задача 38, этап 1).
///
/// Слияние ([`merge`]) сводит записи по идентификатору, и копия другого ИП
/// легла бы рядом с записями этого: два живых профиля, из которых окно
/// молча взяло бы изменённый последним, а счета и книга двух ИП — в одном
/// налоге. Поэтому окно спрашивает до записи, а здесь решается, по чему
/// узнать тот же ИП:
///
/// - общая запись профиля — тот же, даже если ՀՎՀՀ в ней с тех пор
///   поправили;
/// - общий ՀՎՀՀ — тот же, даже если профиль на другом устройстве завели
///   заново;
/// - профиля нет у базы или живого профиля у копии — сравнивать не с чем:
///   пустая программа принимает копию, как раньше, и копия без профиля —
///   тоже.
///
/// Профиль базы, который не читается, сравнивается одним идентификатором —
/// он есть в ключе: окно при испорченной записи само советует восстановить
/// из копии, и своя копия запись починит, а чужая без вопроса легла бы
/// рядом. Профиль новее программы — отказ целиком, как у остального
/// хранилища: кто он, эта версия не знает (Правило 2).
pub fn relation<'b>(
    incoming: &[Entry],
    existing: impl IntoIterator<Item = Raw<'b>>,
) -> Result<Relation, FormatError> {
    let theirs: Vec<Profile> = incoming
        .iter()
        .filter(|entry| entry.kind == Profile::KIND)
        .filter_map(|entry| decode_record(FORMAT_VERSION, entry.record.get()).ok())
        .collect();
    let mut ids = Vec::new();
    let mut ours = Vec::new();
    let mut unreadable = false;
    for raw in existing.into_iter().filter(|raw| raw.kind == Profile::KIND) {
        ids.push(raw.id);
        match raw.text.map(super::decode::<Profile>) {
            Some(Ok(profile)) => ours.push(profile),
            Some(Err(FormatError::Newer { found })) => return Err(FormatError::Newer { found }),
            Some(Err(FormatError::Damaged(_))) | None => unreadable = true,
        }
    }
    let shared_record = theirs
        .iter()
        .any(|their| ids.contains(&their.id().to_string().as_str()));
    let shared_tin = theirs
        .iter()
        .filter_map(|their| their.data().tin.as_ref())
        .any(|tin| ours.iter().any(|our| our.data().tin.as_ref() == Some(tin)));
    let unshared = !shared_record && !shared_tin;
    Ok(match (Profile::current(ours), Profile::current(theirs)) {
        (Some(ours), Some(theirs)) if unshared => Relation::Other {
            ours: Some(Owner::of(&ours)),
            theirs: Owner::of(&theirs),
        },
        (None, Some(theirs)) if unreadable && unshared => Relation::Other {
            ours: None,
            theirs: Owner::of(&theirs),
        },
        _ => Relation::Same,
    })
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

/// Имя копии одного ИП для клиента (задача 38): `kvartik-12345678-2026-11-06.json`
/// — по ՀՎՀՀ, а не по имени ИП: имя на чужом письме и с кавычками в имени
/// файла ломалось бы, а ՀՎՀՀ — восемь цифр. Без ՀՎՀՀ — `kvartik-ip-…`.
pub fn client_file_name(tin: Option<&Tin>, today: Date) -> String {
    let who = tin.map_or("ip", Tin::as_str);
    format!(
        "kvartik-{who}-{:04}-{:02}-{:02}.json",
        today.year(),
        today.month(),
        today.day()
    )
}

/// Похоже ли имя выбранного файла на копию для клиента
/// ([`client_file_name`]): `kvartik-<ՀՎՀՀ или ip>-<день>.json`, и частями —
/// `…-1of3.json`. Окно по нему не ставит в форму её пароля логин для менеджера
/// паролей (задача 124): пароль такой копии — разовый, и менеджер предложил бы
/// записать его поверх пароля копий. Имя файла назначает не программа, а
/// человек и браузер: прощаются пробелы по краям, расширение любым регистром
/// и пометка копии при повторном скачивании — `(1)`, ` (2)` у Chrome
/// и Firefox, ` 2` у «Файлов» iPhone. Ошибка в обе стороны безвредна: окно
/// спросит пароль так же, только с логином или без.
pub fn is_client_file_name(name: &str) -> bool {
    let name = name.trim();
    let Some(stem) = name
        .len()
        .checked_sub(".json".len())
        .filter(|&at| name.is_char_boundary(at) && name[at..].eq_ignore_ascii_case(".json"))
        .map(|at| &name[..at])
    else {
        return false;
    };
    let stem = without_copy_mark(stem.trim_end());
    // Часть копии — `-1of3` в конце.
    let stem = match stem.rsplit_once('-') {
        Some((head, tail)) if part_of_name(tail).is_some() => head,
        _ => stem,
    };
    let Some(rest) = stem.strip_prefix("kvartik-") else {
        return false;
    };
    let pieces: Vec<&str> = rest.split('-').collect();
    let digits = |text: &str, len: usize| text.len() == len && text.bytes().all(|byte| byte.is_ascii_digit());
    match pieces.as_slice() {
        [who, year, month, day] => {
            (*who == "ip" || digits(who, Tin::DIGITS)) && digits(year, 4) && digits(month, 2) && digits(day, 2)
        }
        _ => false,
    }
}

/// Имя без пометки повторного скачивания в конце: `(1)`, ` (2)`, ` 2`.
fn without_copy_mark(stem: &str) -> &str {
    let numbered = |text: &str| !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit());
    if let Some(inner) = stem.strip_suffix(')')
        && let Some((head, number)) = inner.rsplit_once('(')
        && numbered(number)
    {
        return head.trim_end();
    }
    match stem.rsplit_once(' ') {
        Some((head, number)) if numbered(number) => head.trim_end(),
        _ => stem,
    }
}

/// Копия всех ИП (задача 38): в ней записи конторы — ИП списка или ключ
/// «Pro» — или записи с меткой базы. Такую окно кладёт по ИП
/// (`format::office::route`), а не в открытую базу: запись конторы в базе ИП
/// ушла бы с копией одного ИП к клиенту.
pub fn is_full(entries: &[Entry]) -> bool {
    entries.iter().any(|entry| {
        OFFICE_KINDS.iter().any(|kind| kind.name() == entry.kind()) || entry.base().is_some()
    })
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

    /// Проверочное значение — SHA-256 ключа.
    pub fn check(&self) -> &[u8] {
        &self.check
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
