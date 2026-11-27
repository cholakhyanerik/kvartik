//! Файл копии из базы и обратно (задача 7).
//!
//! Долгие шаги — разбор записей, сжатие, base64 — идут кусками с паузой
//! (`platform::pause`): с файлами книги копия — десятки мегабайт, и одним
//! куском она держала страницу секундами (0.0.28).
//!
//! Что в файле и как записи сливаются — ядро (`kvartik_core::format::backup`),
//! шифр — WebCrypto (`crate::platform::crypto`). Здесь — путь между ними
//! и базой: прочитать все записи одной транзакцией, собрать и зашифровать
//! файл; расшифровать, разобрать и слить с базой — тоже одной транзакцией
//! (одно действие — одна транзакция, Правило 2).
//!
//! Копия всех ИП (задача 38) — тот же файл: записи конторы и записи каждой
//! базы с меткой своего ИП ([`make_all`]). Восстанавливается она по ИП:
//! куда ляжет каждый, решает маршрут ядра до первой записи, потом список
//! конторы и каждая база — своей транзакцией, и повтор безвреден
//! ([`restore_all`], решение человека 2026-11-05).
//!
//! Копия больше файла (задача 117) — несколько файлов, частей: как
//! разложить записи, решает ядро (`backup::split`), а здесь каждая часть
//! сжимается и шифруется по очереди, своим вектором. Ложится часть тем же
//! путём, что целая копия: каждая — копия сама по себе.
//!
//! Память (задача 122): выгрузка базы разбирается по своей — текст записи
//! уходит сразу после разбора, — часть сжимается по записи, без открытого
//! текста целиком, а её файл складывается в браузере кусками и уходит
//! приёмнику ([`Sink`]), как только готов. Копия всех ИП — по базе за раз
//! ([`make_all`]). Память wasm не убывает, поэтому всё это — о пике.

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;

use kvartik_core::format::FormatError;
use kvartik_core::format::backup::{
    self, BackupError, Entry, Files, Grew, Header, IV_BYTES, KINDS, Kind, Lock, MAX_FILE_BYTES, MAX_PARTS, Merge,
    OFFICE_KINDS, Owner, Packed, Part, Relation, Sealed, Stream, Tally, sealed_bound,
};
use kvartik_core::format::Stored;
use kvartik_core::format::office::{self as route, RouteError};
use kvartik_core::license::{License, MAX_KEYS, OfficeLicense};
use kvartik_core::model::date::Date;
use kvartik_core::model::office::{Ip, MAX_IPS};
use kvartik_core::model::record::{RecordId, Timestamp};
use web_sys::File;

use super::office;
use super::{Dumped, Store, StoreError, Writes};
use crate::platform::crypto::{self, CryptoError, Key};
use crate::platform::file::Pieces;
use crate::platform::pause::breathe;

/// Почему копия не сделана или не восстановлена.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Failure {
    /// Хранилище отказало.
    Store(StoreError),
    /// Шифр отказал или пароль не подошёл.
    Crypto(CryptoError),
    /// Файл копии не открывается.
    File(BackupError),
    /// Часть копии вышла больше `MAX_FILE_BYTES` или частей — больше
    /// `MAX_PARTS`: такую программа не откроет, и отдавать её человеку —
    /// обещать то, чего она не вернёт. У программы так не бывает: часть
    /// больше предела — только у записи больше части (`backup::split`).
    TooLarge,
    /// С копией ИП в программе стало бы больше `MAX_IPS`: столько.
    TooManyIps(usize),
    /// С копией записей ключей — «Pro» в конторе или в базе ИП — стало бы
    /// больше `license::MAX_KEYS` (ревью задачи 38): такую программа не кладёт.
    TooManyKeys,
    /// Файл копии не отдан (задача 122): браузер не собрал его или приёмник
    /// ([`Sink`]) отказал — почему, знает он сам.
    Handed,
}

impl From<StoreError> for Failure {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

impl From<CryptoError> for Failure {
    fn from(error: CryptoError) -> Self {
        Self::Crypto(error)
    }
}

impl From<BackupError> for Failure {
    fn from(error: BackupError) -> Self {
        Self::File(error)
    }
}

/// Файл копии — или файл её части (задача 117) — уже в браузере: шапка,
/// данные кусками и хвост ушли туда, как только были готовы (задача 122).
/// Текста файла в памяти wasm нет: часть копии — до сотни мегабайт.
pub struct Sheet {
    /// Файл — без имени: имя ему даёт тот, кто отдаёт его человеку.
    pub file: File,
    /// Какая это часть; `None` — копия одним файлом.
    pub part: Option<Part>,
}

/// Сделанная копия; её файлы уже у приёмника ([`Sink`]).
pub struct Made {
    /// Сколько записей в копии.
    pub records: usize,
    /// Сколько записей не читается и в копию не вошло.
    pub unreadable: usize,
    /// Сколько в ней весят файлы книги и убранные из записей (задача 64).
    pub files: Files,
    /// Сколько весят все файлы копии вместе.
    pub size: u64,
    /// В скольких она файлах.
    pub sheets: usize,
}

/// Отдача файла приёмнику.
pub type Handing<'a> = Pin<Box<dyn Future<Output = Result<(), Failure>> + 'a>>;

/// Куда уходят файлы копии (задача 122): по одному, как только файл готов, —
/// ссылкой человеку или в папку автокопии, — а не все в конце. Копия
/// частями держала все свои файлы в памяти разом, а с ними и записи: замер
/// 0.0.181 — 1,4 ГиБ у вкладки с копией в 135 МБ, а Safari на iPhone
/// закрывает вкладку, которой мало памяти. Приёмник — объект, а не тип
/// параметра: у копии их два, и код сборки в wasm один.
pub trait Sink {
    /// Файл копии готов. Отказ — [`Failure::Handed`]: почему, знает приёмник.
    fn put(&mut self, sheet: Sheet) -> Handing<'_>;
    /// Копию собирают заново ([`make_all`]): отданное раньше — не её.
    fn restart(&mut self);
}

/// Ключ копии и откуда он: соль и число итераций, из которых его получили,
/// ложатся в замок файла — по ним ключ получат снова при восстановлении.
/// `limit` — сколько открытого текста кладётся в один файл: у окна
/// `backup::PART_PLAIN_BYTES`, а тесты делят копию мелко (задача 117).
pub struct Sealing<'a> {
    pub header: &'a Header,
    pub key: &'a Key,
    pub salt: &'a [u8],
    pub iterations: u32,
    pub limit: usize,
}

/// Собрать копию всей базы, зашифровать её и отдать файлы приёмнику — одним
/// файлом или частями (`backup::split`, задача 117). Части — по одной:
/// сжатый текст, шифротекст и файл части уходят, как только она готова
/// (задача 122), и в памяти рядом с записями — одна часть в работе.
pub async fn make(store: &Store, kinds: &[Kind], sealing: &Sealing<'_>, sink: &mut dyn Sink) -> Result<Made, Failure> {
    let (entries, unreadable) = collect(store, kinds, true).await?;
    let parts = backup::split(sealing.header, &entries, sealing.limit);
    // Не уложится — сказать до первой части: автокопия уже положила бы её
    // в папку поверх прежней копии дня.
    if parts.len() > usize::from(MAX_PARTS) || parts.iter().any(|(_, part)| too_large(backup::plain_len(part))) {
        return Err(Failure::TooLarge);
    }
    let mut made = Made {
        records: entries.len(),
        unreadable,
        files: backup::files(&entries),
        size: 0,
        sheets: 0,
    };
    for (header, part) in &parts {
        let packed = backup::pack_sliced(header, part.iter().copied(), &mut breathe).await;
        hand(
            Packed {
                header: header.clone(),
                packed,
            },
            sealing,
            sink,
            &mut made,
        )
        .await?;
    }
    Ok(made)
}

/// Сколько раз копия всех ИП собирается заново, когда база выросла между
/// проходами ([`Grew`]): рост — это правка в другой вкладке в те секунды,
/// что идёт копия, и трижды подряд его не бывает.
const ATTEMPTS: usize = 3;

/// Собрать копию всех ИП (задача 38), зашифровать её и отдать файлы
/// приёмнику: записи конторы и записи базы каждого ИП списка — и убранного:
/// он не стёрт и вернуться может только с копией — с меткой своего ИП. База
/// открытого ИП (`open_name`) читается его соединением, остальные
/// открываются по одной.
///
/// По базе за раз (задача 122): у конторы на сто ИП с фото чеков копия —
/// сотни мегабайт, и все записи всех баз в памяти разом закрыли бы вкладку
/// телефона. Поэтому проходов два ([`make_all_once`]), а изменилась база
/// между ними так, что копия вышла бы не той, — копия собирается заново,
/// и приёмник забывает отданное ([`Sink::restart`]).
///
/// Список ИП — из той же выгрузки конторы, что её записи в файле: иначе ИП,
/// добавленный между двумя чтениями, оставил бы в файле метки без своего ИП,
/// и файл не открылся бы (ревью 2026-11-06). Запись ИП не читается — отказ:
/// база этого ИП молча выпала бы из копии, а день копии сдвинулся бы.
/// А нечитаемая запись ключа «Pro» копию не останавливает — её только нет
/// в копии, как и любой нечитаемой записи: убрать её в окне нечем, и иначе
/// у конторы не осталось бы ни одной копии (ревью задачи 38).
pub async fn make_all(
    open: &Store,
    open_name: &str,
    office_store: &Store,
    sealing: &Sealing<'_>,
    sink: &mut dyn Sink,
) -> Result<Made, Failure> {
    for _ in 0..ATTEMPTS {
        match make_all_once(open, open_name, office_store, sealing, sink).await? {
            Ok(made) => return Ok(made),
            Err(Grew) => sink.restart(),
        }
    }
    Err(Failure::Store(StoreError::Failed))
}

/// Копия всех ИП двумя проходами по базам. Первый держит в памяти записи
/// конторы и общие записи частей — ИП конторы и профили, — а от остальных
/// только размеры: по ним `backup::plan` знает число частей, а оно нужно
/// уже в шапке первой. Второй кладёт записи в части потоком
/// (`backup::Stream`) в том же порядке, что у копии одного ИП
/// (`backup::split`): записи конторы, потом записи без файлов книги всех
/// ИП — база за базой, — потом файлы книги всех ИП. Для этого он обходит
/// базы дважды и каждый раз читает из базы только нужное
/// (`Tx::dump_kinds`): в памяти — записи одной базы, а у части, легшей без
/// остальных, — счета и книга всех ИП, а не одного (ревью задачи 122).
///
/// Каждое чтение — своя транзакция, и база в копии — не один её снимок:
/// записи без файлов и файлы книги — из разных чтений, а профили — из
/// первого прохода. Профиль, правленный между проходами, делает копию
/// заново ([`Grew`]): в частях был бы прежний, а рядом — уже новый логотип
/// той же правки (ревью задачи 122). Тот же профиль вторым проходом — уже
/// в каждой части, и вторым разом он сделал бы часть порчей (`read`,
/// «запись в копии дважды»). Новый профиль, которого первый проход не
/// видел, идёт в поток, как любая запись.
async fn make_all_once(
    open: &Store,
    open_name: &str,
    office_store: &Store,
    sealing: &Sealing<'_>,
    sink: &mut dyn Sink,
) -> Result<Result<Made, Grew>, Failure> {
    let dumped = office_store.read()?.dump().await?;
    let stored_ips = dumped.iter().filter(|record| record.raw().kind == Ip::KIND).count();
    // Контора читается одним разом, первым проходом: её нечитаемые — здесь.
    let (office, office_unreadable) = collect_dumped(dumped, OFFICE_KINDS, true).await?;
    if office.iter().filter(|entry| entry.kind() == Ip::KIND).count() != stored_ips {
        return Err(Failure::Store(StoreError::Damaged));
    }
    let ips = route::ips(&office).map_err(newer)?;
    let (mut shared, keys): (Vec<Entry>, Vec<Entry>) = office.into_iter().partition(Entry::in_every_part);
    // Записи конторы, кроме ИП, — ключи «Pro»: файлов книги среди них нет.
    let mut sizes: Vec<usize> = keys.iter().map(Entry::plain_bytes).collect();
    let mut files = Vec::new();
    for ip in &ips {
        let other;
        let store = if ip.base() == open_name {
            open
        } else {
            other = Store::open_base(ip.base(), |_| {}).await?;
            &other
        };
        let dumped = store.read()?.dump().await?;
        let measured = backup::measure_sliced(KINDS, dumped, &mut breathe).await.map_err(newer)?;
        sizes.extend(measured.records);
        files.extend(measured.files);
        shared.extend(measured.shared.into_iter().map(|entry| entry.in_base(ip.id())));
    }
    sizes.extend(files);
    let shared_bytes = shared.iter().map(Entry::plain_bytes).fold(0, usize::saturating_add);
    let planned = backup::plan(shared_bytes, sizes, sealing.limit);
    if planned.len() > usize::from(MAX_PARTS) || planned.iter().any(|&plain| too_large(plain)) {
        return Err(Failure::TooLarge);
    }
    let known: BTreeMap<(Option<RecordId>, RecordId), Timestamp> = shared
        .iter()
        .map(|entry| ((entry.base(), entry.meta().id()), entry.meta().modified()))
        .collect();
    let mut made = Made {
        records: shared.len() + keys.len(),
        unreadable: office_unreadable,
        files: Files::default(),
        size: 0,
        sheets: 0,
    };
    let mut stream = Stream::new(sealing.header, shared, planned, sealing.limit);
    for entry in &keys {
        if let Err(grew) = feed(&mut stream, entry, sealing, sink, &mut made).await? {
            return Ok(Err(grew));
        }
    }
    drop(keys);
    for of_files in [false, true] {
        for ip in &ips {
            let other;
            let store = if ip.base() == open_name {
                open
            } else {
                other = Store::open_base(ip.base(), |_| {}).await?;
                &other
            };
            let dumped = store.read()?.dump_kinds(|kind| backup::is_file_kind(kind) == of_files).await?;
            let (entries, skipped) = collect_dumped(dumped, KINDS, true).await?;
            made.unreadable += skipped;
            if of_files {
                made.files = made.files.plus(backup::files(&entries));
            }
            let mut tagged = Vec::with_capacity(entries.len());
            for entry in entries {
                if entry.in_every_part() {
                    match known.get(&(Some(ip.id()), entry.meta().id())) {
                        Some(&modified) if modified == entry.meta().modified() => continue,
                        Some(_) => return Ok(Err(Grew)),
                        None => {}
                    }
                }
                tagged.push(entry.in_base(ip.id()));
            }
            made.records += tagged.len();
            for entry in &tagged {
                if let Err(grew) = feed(&mut stream, entry, sealing, sink, &mut made).await? {
                    return Ok(Err(grew));
                }
            }
        }
    }
    for full in stream.finish() {
        hand(full, sealing, sink, &mut made).await?;
    }
    Ok(Ok(made))
}

/// Запись — в поток частей; начала она новую часть — готовая уходит
/// приёмнику.
async fn feed(
    stream: &mut Stream,
    entry: &Entry,
    sealing: &Sealing<'_>,
    sink: &mut dyn Sink,
    made: &mut Made,
) -> Result<Result<(), Grew>, Failure> {
    match stream.add(entry, &mut breathe).await {
        Ok(Some(full)) => hand(full, sealing, sink, made).await.map(Ok),
        Ok(None) => Ok(Ok(())),
        Err(grew) => Ok(Err(grew)),
    }
}

/// Не уложится ли файл части с `plain` байтами открытого текста в
/// `MAX_FILE_BYTES` — такой программа не откроет. У программы так не
/// бывает: часть больше предела — только у записи больше части.
fn too_large(plain: usize) -> bool {
    sealed_bound(plain) as u64 > MAX_FILE_BYTES
}

/// Записи базы для копии — и сколько не читается: их видно в консоли, если
/// `log` (первый проход копии всех ИП молчит — второй скажет то же).
async fn collect(store: &Store, kinds: &[Kind], log: bool) -> Result<(Vec<Entry>, usize), Failure> {
    let dumped = store.read()?.dump().await?;
    collect_dumped(dumped, kinds, log).await
}

/// Записи выгрузки базы для копии — и сколько не читается. Выгрузка —
/// по своей: текст записи уходит из памяти сразу после её разбора
/// (задача 122), и выгрузка с записями не лежат в памяти разом.
async fn collect_dumped(dumped: Vec<Dumped>, kinds: &[Kind], log: bool) -> Result<(Vec<Entry>, usize), Failure> {
    let collected = backup::collect_sliced(kinds, dumped, &mut breathe).await.map_err(newer)?;
    if log {
        for why in collected.unreadable() {
            super::log("backup", &format!("не вошла в копию: {why}").into());
        }
    }
    let (entries, unreadable) = collected.into_parts();
    Ok((entries, unreadable.len()))
}

/// Зашифровать сжатую часть, сложить её файл в браузере кусками и отдать
/// приёмнику. Вектор шифра — новый у каждой части: ключ у них один (шапка
/// `format/backup.rs`, «Шифр»). Сжатое уходит из памяти до того, как придёт
/// шифротекст, а шифротекст — когда файл сложен: в памяти wasm одна из них.
async fn hand(part: Packed, sealing: &Sealing<'_>, sink: &mut dyn Sink, made: &mut Made) -> Result<(), Failure> {
    let Packed { header, packed } = part;
    #[cfg(all(test, target_arch = "wasm32"))]
    if probe::FAIL_AT_PART.with(std::cell::Cell::get) == made.sheets + 1 {
        return Err(Failure::Crypto(CryptoError::Failed));
    }
    let iv = crypto::random(IV_BYTES)?;
    let lock = Lock::new(sealing.iterations, sealing.salt.to_vec(), iv).ok_or(CryptoError::Failed)?;
    let ciphertext = crypto::encrypt(sealing.key, lock.iv(), packed).await?;
    let mut pieces = Pieces::default();
    let size = backup::seal_pieces(
        &header,
        &lock,
        &ciphertext,
        &mut |piece: &str| pieces.push(piece.as_bytes()),
        &mut breathe,
    )
    .await as u64;
    drop(ciphertext);
    if size > MAX_FILE_BYTES {
        return Err(Failure::TooLarge);
    }
    let file = pieces.finish().ok_or(Failure::Handed)?;
    made.size += size;
    made.sheets += 1;
    sink.put(Sheet {
        file,
        part: header.part(),
    })
    .await
}

/// Копия для проб (задача 122): файлы копии — текстом, как их отдал бы
/// браузер. Тесты хранилища и окна сравнивают и открывают сами файлы.
#[cfg(all(test, target_arch = "wasm32"))]
pub mod probe {
    use kvartik_core::format::backup::{Files, Part};

    use super::{Failure, Handing, Kind, Sealing, Sheet, Sink, Store};

    thread_local! {
        /// Номер части, на которой копия откажет шифром, — сбой посреди копии
        /// частями для проб; 0 — не откажет.
        pub static FAIL_AT_PART: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    }

    /// Файл копии текстом.
    pub struct Text {
        pub text: String,
        pub part: Option<Part>,
    }

    /// Сделанная копия с файлами текстом.
    pub struct Made {
        pub sheets: Vec<Text>,
        pub records: usize,
        pub unreadable: usize,
        pub files: Files,
        /// Сколько раз копию собирали заново (`make_all`).
        pub restarts: usize,
        size: u64,
    }

    impl Made {
        /// Сколько весят все файлы копии вместе — как насчитала копия.
        pub const fn size(&self) -> u64 {
            self.size
        }
    }

    /// Приёмник проб: читает файл текстом.
    #[derive(Default)]
    pub struct Texts {
        pub sheets: Vec<Text>,
        pub restarts: usize,
    }

    impl Sink for Texts {
        fn put(&mut self, sheet: Sheet) -> Handing<'_> {
            Box::pin(async move {
                let text = sheet.file.text().await.map_err(|_| Failure::Handed)?;
                self.sheets.push(Text {
                    text: text.as_string().ok_or(Failure::Handed)?,
                    part: sheet.part,
                });
                Ok(())
            })
        }

        fn restart(&mut self) {
            self.sheets.clear();
            self.restarts += 1;
        }
    }

    fn made(made: super::Made, texts: Texts) -> Made {
        Made {
            sheets: texts.sheets,
            records: made.records,
            unreadable: made.unreadable,
            files: made.files,
            restarts: texts.restarts,
            size: made.size,
        }
    }

    /// [`super::make`] с файлами текстом.
    pub async fn make(store: &Store, kinds: &[Kind], sealing: &Sealing<'_>) -> Result<Made, Failure> {
        let mut texts = Texts::default();
        let done = super::make(store, kinds, sealing, &mut texts).await?;
        Ok(made(done, texts))
    }

    /// [`super::make_all`] с файлами текстом.
    pub async fn make_all(open: &Store, open_name: &str, office: &Store, sealing: &Sealing<'_>) -> Result<Made, Failure> {
        let mut texts = Texts::default();
        let done = super::make_all(open, open_name, office, sealing, &mut texts).await?;
        Ok(made(done, texts))
    }
}

/// Записи копии: ключ из пароля и замка файла, расшифровка, разбор.
/// Пароль — уже приведённый (`crypto::normalize`). Вместе с записями —
/// ключ: по нему устройство запомнит пароль, если своего ещё нет, а окно
/// откроет следующую часть той же копии (задача 117).
pub async fn unlock(
    kinds: &[Kind],
    sealed: &Sealed,
    password: &str,
) -> Result<(Vec<Entry>, Key), Failure> {
    let lock = sealed.lock();
    let key = crypto::derive(password, lock.salt(), lock.iterations()).await?;
    let entries = unlock_with(kinds, sealed, &key).await?;
    Ok((entries, key))
}

/// Записи копии ключом, уже полученным из пароля: у частей одной копии
/// замок с одной солью и одним числом итераций (задача 117), и ключ
/// прошлой части открывает следующую без пароля.
pub async fn unlock_with(kinds: &[Kind], sealed: &Sealed, key: &Key) -> Result<Vec<Entry>, Failure> {
    let plaintext = crypto::decrypt(key, sealed.lock().iv(), sealed.ciphertext()).await?;
    Ok(backup::read_sliced(kinds, sealed, &plaintext, &mut breathe).await?)
}

/// Что сделало восстановление.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Restored {
    /// Записей копии, которых в базе не было.
    pub added: usize,
    /// Записей, которые копия заменила: в ней они позже или в базе не
    /// читались.
    pub updated: usize,
    /// Записей копии, которые в базе уже такие же или новее.
    pub unchanged: usize,
    /// Самый ранний момент изменения среди записей, которые легли; `None` —
    /// не легло ничего. Момент у записи — её устройства, и легшая сейчас
    /// запись бывает старше прошлого кода передачи с этого устройства: по
    /// этому моменту следующий код возьмёт и её (`Device::sync_arrived`,
    /// ревью задачи 110).
    pub oldest: Option<Timestamp>,
    /// Что легло — по видам (задача 124, п. 21): слова итога.
    pub landed: Tally,
}

/// Самый ранний момент изменения записей, которые слияние запишет.
fn oldest(merge: &Merge<'_>) -> Option<Timestamp> {
    merge.writes().map(|entry| entry.meta().modified()).min()
}

/// Копия другого ИП ([`backup::relation`]): спросить человека или сливать.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OtherIe {
    /// Не записывать ничего и вернуть обоих ИП: окно спросит.
    Ask,
    /// Человек ответил «слить».
    Merge,
}

/// Чем кончилось восстановление.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Merged {
    /// Записи легли.
    Done(Restored),
    /// Копия другого ИП, а спросить велели: не записано ничего.
    Other {
        /// ИП программы; `None` — профиль в базе не читается.
        ours: Option<Owner>,
        /// ИП копии.
        theirs: Owner,
    },
}

/// Слить записи копии с базой — одним действием: или легли все, что
/// решило слияние, или ни одной. Копию другого ИП с [`OtherIe::Ask`] —
/// не сливать, а вернуть обоих ИП; сверка — в той же транзакции, что
/// запись, а профиль базы новее программы — отказ целиком, как у слияния.
/// С [`OtherIe::Merge`] сверки нет: человек только что видел обоих.
///
/// Между чтением базы и записью — только расчёт ядра, без ожидания
/// чужого: транзакция ещё жива (шапка `store/mod.rs`). Транзакция, брошенная
/// без записи, кончается сама и ничего не пишет.
pub async fn restore(
    store: &Store,
    kinds: &[Kind],
    entries: &[Entry],
    other: OtherIe,
) -> Result<Merged, Failure> {
    let tx = store.write()?;
    let dumped = tx.dump().await?;
    if other == OtherIe::Ask
        && let Relation::Other { ours, theirs } =
            backup::relation(entries, dumped.iter().map(Dumped::raw)).map_err(newer)?
    {
        return Ok(Merged::Other { ours, theirs });
    }
    let merge = backup::merge(kinds, entries, dumped.iter().map(Dumped::raw)).map_err(newer)?;
    keys_fit(&dumped, &merge, License::KIND)?;
    let mut writes = Writes::default();
    for entry in merge.writes() {
        writes.restore(entry);
    }
    tx.commit(writes).await?;
    Ok(Merged::Done(Restored {
        added: merge.added(),
        updated: merge.updated(),
        unchanged: merge.unchanged(),
        oldest: oldest(&merge),
        landed: Tally::of(merge.writes()),
    }))
}

/// Записей ключей вида `kind` после слияния `merge` с базой `dumped` — не
/// больше `license::MAX_KEYS` (ревью задачи 38): иначе отказ, и не пишется
/// ничего — транзакция без записи кончается сама. Копия без новых записей
/// ключей проходит и там, где их уже больше: столько могло лечь до 0.0.135,
/// и своя копия такой базы не открылась бы никогда, а хуже от неё не станет.
fn keys_fit(dumped: &[Dumped], merge: &Merge<'_>, kind: &str) -> Result<(), Failure> {
    let before = dumped.iter().filter(|record| record.raw().kind == kind).count();
    let after = backup::count_after(kind, dumped.iter().map(Dumped::raw), merge);
    if after > MAX_KEYS && after > before {
        return Err(Failure::TooManyKeys);
    }
    Ok(())
}

/// Что сделало восстановление копии всех ИП — и когда легло не всё.
pub struct RestoredAll {
    /// Записи всех баз вместе.
    pub restored: Restored,
    /// Легло ли что-нибудь в базу открытого ИП: тогда её разделы
    /// перечитывают своё (`Base::replaced`).
    pub open_touched: bool,
    /// Были ли в копии все ИП этого устройства — тогда она и копия его
    /// данных: день последней копии сдвигает только такая (ревью
    /// 2026-11-06).
    pub covers_all: bool,
    /// Список легшего уже записан, а база ИП не легла: почему. Повтор
    /// безвреден, и окно так и говорит.
    pub failed: Option<Failure>,
    /// Базы, в которые легли записи, — с самым ранним моментом легших
    /// ([`Restored::oldest`]).
    pub landed: Vec<(String, Timestamp)>,
}

/// Восстановить копию всех ИП (задача 38). Куда ложится каждый ИП копии,
/// решает маршрут ядра (`format::office::route`) — до первой записи; потом
/// список конторы — своей транзакцией, и каждая база — своей. Повтор
/// безвреден: слияние — по идентификатору записи (решение человека
/// 2026-11-05). `first` — открытый ИП, которого в списке ещё нет: конторы
/// не было. Он ложится в список вместе с маршрутом, и копия того же ИП
/// сливается с его базой, а не встаёт рядом вторым. `spare` — пустая база
/// открытого ИП без списка: её займёт первый ИП копии.
///
/// Список — первым: запись списка без данных — пустой ИП, которого видно
/// и можно убрать, а база без записи в списке потерялась бы. Не легла база —
/// дальше не идёт, а говорит, что уже легло ([`RestoredAll::failed`]): окно
/// перечитывает тронутое и просит повторить. Ключи «Pro» копии (с 0.0.134)
/// ложатся той же транзакцией, что список, — слиянием с ключами конторы по
/// идентификатору, как любые записи.
pub async fn restore_all(
    open: &Store,
    open_name: &str,
    office_store: &Store,
    entries: Vec<Entry>,
    first: Option<Ip>,
    spare: Option<&str>,
) -> Result<RestoredAll, Failure> {
    let mut local = office::list(office_store).await?.records;
    local.extend(first.clone());
    let route = route::route(entries, &local, spare).map_err(|error| match error {
        RouteError::TooMany(count) => Failure::TooManyIps(count),
        RouteError::TooManyKeys(_) => Failure::TooManyKeys,
        RouteError::Format(error) => newer(error),
    })?;
    let tx = office_store.write()?;
    let dumped = tx.dump().await?;
    let keys = backup::merge(OFFICE_KINDS, &route.keys, dumped.iter().map(Dumped::raw)).map_err(newer)?;
    // Записей ключей с копией стало бы больше предела — и вместе с прежними:
    // не писать ничего.
    keys_fit(&dumped, &keys, OfficeLicense::KIND)?;
    let mut writes = Writes::default();
    if let Some(first) = &first {
        writes.insert(first);
    }
    for ip in &route.office {
        writes.put(ip);
    }
    for entry in keys.writes() {
        writes.restore(entry);
    }
    tx.commit(writes).await?;
    let keys_oldest = oldest(&keys);
    let covers_all = local
        .iter()
        .all(|ip| route.bases.iter().any(|landing| landing.base == ip.base()));
    let mut done = RestoredAll {
        restored: Restored {
            added: keys.added(),
            updated: keys.updated(),
            unchanged: keys.unchanged(),
            oldest: None,
            landed: Tally::of(keys.writes()),
        },
        open_touched: false,
        covers_all: covers_all && route.bases.iter().any(|landing| landing.base == open_name),
        failed: None,
        // Ключи «Pro», легшие в контору, — тоже легли: код всех ИП с этого
        // устройства возьмёт их и тогда, когда они старше его прошлого кода
        // (ревью задачи 116).
        landed: keys_oldest.map(|at| (office::OFFICE_DB.to_owned(), at)).into_iter().collect(),
    };
    for landing in &route.bases {
        let landed = if landing.base == open_name {
            merge_into(open, &landing.entries).await
        } else {
            match Store::open_base(&landing.base, |_| {}).await {
                Ok(other) => merge_into(&other, &landing.entries).await,
                Err(error) => Err(error.into()),
            }
        };
        match landed {
            Ok(restored) => {
                done.open_touched |= landing.base == open_name && restored.added + restored.updated > 0;
                done.restored.added += restored.added;
                done.restored.updated += restored.updated;
                done.restored.unchanged += restored.unchanged;
                done.restored.landed = done.restored.landed.plus(restored.landed);
                if let Some(at) = restored.oldest {
                    done.landed.push((landing.base.clone(), at));
                }
            }
            Err(failure) => {
                done.failed = Some(failure);
                break;
            }
        }
    }
    Ok(done)
}

/// Восстановить копию одного ИП как нового ИП (задача 38): его записи —
/// в его новую базу, одной транзакцией, а потом — запись в списке (`first` —
/// открытый ИП, которого в списке ещё нет). База — первой: не легла — нет
/// ничего, и повтор не заведёт второго пустого ИП (ревью 2026-11-06); не лёг
/// список — база не видна, но файл копии у человека, и повтор положит всё.
pub async fn restore_new(
    office_store: &Store,
    entries: &[Entry],
    first: Option<Ip>,
    today: Date,
    now: Timestamp,
) -> Result<(Ip, Restored), Failure> {
    let listed = office::list(office_store).await?.records;
    let count = listed.len() + usize::from(first.is_some()) + 1;
    if count > MAX_IPS {
        return Err(Failure::TooManyIps(count));
    }
    // Имя, ՀՎՀՀ и запись профиля — сразу из копии (`route::profile`).
    let mut new = Ip::new(today, now);
    if let Some(profile) = route::profile(entries).map_err(newer)? {
        new.copy_profile(&profile, now);
    }
    let store = Store::open_base(new.base(), |_| {}).await?;
    let restored = merge_into(&store, entries).await?;
    let mut writes = Writes::default();
    if let Some(first) = &first {
        writes.insert(first);
    }
    writes.insert(&new);
    office::write(office_store, writes).await?;
    Ok((new, restored))
}

/// Слить записи с базой одной транзакцией — без сверки ИП: куда они идут,
/// уже решено.
async fn merge_into(store: &Store, entries: &[Entry]) -> Result<Restored, Failure> {
    let tx = store.write()?;
    let dumped = tx.dump().await?;
    let merge = backup::merge(KINDS, entries, dumped.iter().map(Dumped::raw)).map_err(newer)?;
    keys_fit(&dumped, &merge, License::KIND)?;
    let mut writes = Writes::default();
    for entry in merge.writes() {
        writes.restore(entry);
    }
    tx.commit(writes).await?;
    Ok(Restored {
        added: merge.added(),
        updated: merge.updated(),
        unchanged: merge.unchanged(),
        oldest: oldest(&merge),
        landed: Tally::of(merge.writes()),
    })
}

/// Запись базы новее программы — то же, что у остального хранилища:
/// «откройте новую версию».
pub fn newer(error: FormatError) -> Failure {
    match error {
        FormatError::Newer { .. } => Failure::Store(StoreError::Newer),
        FormatError::Damaged(why) => {
            super::log("backup", &why.into());
            Failure::Store(StoreError::Damaged)
        }
    }
}
