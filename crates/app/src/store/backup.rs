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

use kvartik_core::format::FormatError;
use kvartik_core::format::backup::{
    self, BackupError, Entry, Header, IV_BYTES, KINDS, Kind, Lock, MAX_FILE_BYTES, OFFICE_KINDS, Owner,
    Relation, Sealed,
};
use kvartik_core::format::office::{self as route, RouteError};
use kvartik_core::model::date::Date;
use kvartik_core::model::office::{Ip, MAX_IPS};
use kvartik_core::model::record::Timestamp;

use super::office;
use super::{Dumped, Store, StoreError, Writes};
use crate::platform::crypto::{self, CryptoError, Key};
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
    /// Копия вышла больше `MAX_FILE_BYTES`: такую программа не откроет,
    /// и отдавать её человеку — обещать то, чего она не вернёт.
    TooLarge,
    /// С копией ИП в программе стало бы больше `MAX_IPS`: столько.
    TooManyIps(usize),
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

/// Сделанная копия.
pub struct Made {
    /// Текст файла копии.
    pub file: String,
    /// Сколько записей в копии.
    pub records: usize,
    /// Сколько записей не читается и в копию не вошло.
    pub unreadable: usize,
}

/// Ключ копии и откуда он: соль и число итераций, из которых его получили,
/// ложатся в замок файла — по ним ключ получат снова при восстановлении.
pub struct Sealing<'a> {
    pub header: &'a Header,
    pub key: &'a Key,
    pub salt: &'a [u8],
    pub iterations: u32,
}

/// Собрать копию всей базы и зашифровать её.
pub async fn make(store: &Store, kinds: &[Kind], sealing: &Sealing<'_>) -> Result<Made, Failure> {
    let (entries, unreadable) = collect(store, kinds).await?;
    seal(entries, unreadable, sealing).await
}

/// Собрать копию всех ИП (задача 38) и зашифровать её: записи конторы
/// и записи базы каждого ИП списка — и убранного: он не стёрт и вернуться
/// может только с копией — с меткой своего ИП. База открытого ИП
/// (`open_name`) читается его соединением, остальные открываются по одной.
/// Каждая база — своей транзакцией: копия — не снимок всех баз одним
/// моментом, но каждая база в ней целая.
///
/// Список ИП — из той же выгрузки конторы, что её записи в файле: иначе ИП,
/// добавленный между двумя чтениями, оставил бы в файле метки без своего ИП,
/// и файл не открылся бы (ревью 2026-11-06). Запись ИП не читается — отказ:
/// база этого ИП молча выпала бы из копии, а день копии сдвинулся бы.
pub async fn make_all(
    open: &Store,
    open_name: &str,
    office_store: &Store,
    sealing: &Sealing<'_>,
) -> Result<Made, Failure> {
    let (mut entries, unreadable) = collect(office_store, OFFICE_KINDS).await?;
    if unreadable > 0 {
        return Err(Failure::Store(StoreError::Damaged));
    }
    let mut unreadable = 0;
    for ip in route::ips(&entries).map_err(newer)? {
        let (base, skipped) = if ip.base() == open_name {
            collect(open, KINDS).await?
        } else {
            let other = Store::open_base(ip.base(), |_| {}).await?;
            collect(&other, KINDS).await?
        };
        entries.extend(base.into_iter().map(|entry| entry.in_base(ip.id())));
        unreadable += skipped;
    }
    seal(entries, unreadable, sealing).await
}

/// Записи базы для копии — и сколько не читается: их видно в консоли.
async fn collect(store: &Store, kinds: &[Kind]) -> Result<(Vec<Entry>, usize), Failure> {
    let dumped = store.read()?.dump().await?;
    let collected = backup::collect_sliced(kinds, dumped.iter().map(Dumped::raw), &mut breathe)
        .await
        .map_err(newer)?;
    for why in collected.unreadable() {
        super::log("backup", &format!("не вошла в копию: {why}").into());
    }
    let (entries, unreadable) = collected.into_parts();
    Ok((entries, unreadable.len()))
}

/// Сжать, зашифровать и запечатать записи копии.
async fn seal(entries: Vec<Entry>, unreadable: usize, sealing: &Sealing<'_>) -> Result<Made, Failure> {
    let packed = backup::pack_sliced(sealing.header, &entries, &mut breathe).await;
    let iv = crypto::random(IV_BYTES)?;
    let lock = Lock::new(sealing.iterations, sealing.salt.to_vec(), iv).ok_or(CryptoError::Failed)?;
    let ciphertext = crypto::encrypt(sealing.key, lock.iv(), &packed).await?;
    let file = backup::seal_sliced(sealing.header, &lock, &ciphertext, &mut breathe).await;
    if file.len() as u64 > MAX_FILE_BYTES {
        return Err(Failure::TooLarge);
    }
    Ok(Made {
        file,
        records: entries.len(),
        unreadable,
    })
}

/// Записи копии: ключ из пароля и замка файла, расшифровка, разбор.
/// Пароль — уже приведённый (`crypto::normalize`). Вместе с записями —
/// ключ: по нему устройство запомнит пароль, если своего ещё нет.
pub async fn unlock(
    kinds: &[Kind],
    sealed: &Sealed,
    password: &str,
) -> Result<(Vec<Entry>, Key), Failure> {
    let lock = sealed.lock();
    let key = crypto::derive(password, lock.salt(), lock.iterations()).await?;
    let plaintext = crypto::decrypt(&key, lock.iv(), sealed.ciphertext()).await?;
    let entries = backup::read_sliced(kinds, sealed, &plaintext, &mut breathe).await?;
    Ok((entries, key))
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
    let mut writes = Writes::default();
    for entry in merge.writes() {
        writes.restore(entry);
    }
    tx.commit(writes).await?;
    Ok(Merged::Done(Restored {
        added: merge.added(),
        updated: merge.updated(),
        unchanged: merge.unchanged(),
    }))
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
/// перечитывает тронутое и просит повторить.
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
        RouteError::Format(error) => newer(error),
    })?;
    let mut writes = Writes::default();
    if let Some(first) = &first {
        writes.insert(first);
    }
    for ip in &route.office {
        writes.update(ip);
    }
    office::write(office_store, writes).await?;
    let covers_all = local
        .iter()
        .all(|ip| route.bases.iter().any(|landing| landing.base == ip.base()));
    let mut done = RestoredAll {
        restored: Restored {
            added: 0,
            updated: 0,
            unchanged: 0,
        },
        open_touched: false,
        covers_all: covers_all && route.bases.iter().any(|landing| landing.base == open_name),
        failed: None,
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
    let mut writes = Writes::default();
    for entry in merge.writes() {
        writes.restore(entry);
    }
    tx.commit(writes).await?;
    Ok(Restored {
        added: merge.added(),
        updated: merge.updated(),
        unchanged: merge.unchanged(),
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
