//! Код передачи изменений из базы и обратно (задача 110).
//!
//! Что в коде, как он режется на QR-кадры и куски текста и что помнит
//! принимающее устройство — ядро (`kvartik_core::format::sync`), шифр —
//! WebCrypto (`crate::platform::crypto`), пароль — пароль копий устройства,
//! как у файла копии. Ложатся принятые записи тем же путём, что копия:
//! `store::backup::restore` — одной транзакцией, со сверкой ИП.

use kvartik_core::format::backup::{IV_BYTES, Lock};
use kvartik_core::format::office as route;
use kvartik_core::format::sync::{self, Code, CodeError, Header, Picked, Sealed, TooLarge};

use super::backup::{Failure, newer};
use super::{Dumped, Store};
use crate::platform::crypto::{self, CryptoError, Key};
use crate::platform::pause::breathe;

/// Сделанный код.
pub struct Made {
    /// Код целиком — байтами: его режут на QR-кадры и куски текста.
    pub code: Vec<u8>,
    /// Изменённых записей в нём — без живого профиля, который идёт всегда.
    pub changed: usize,
    /// Изменённых файлов книги и логотипов, которые в код не идут.
    pub skipped: usize,
    /// Нечитаемых записей, которые в код не вошли.
    pub unreadable: usize,
}

/// Почему код не сделан.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Unmade {
    /// Хранилище или шифр отказали.
    Failure(Failure),
    /// Изменений больше, чем вмещает код.
    TooLarge(TooLarge),
}

impl From<Failure> for Unmade {
    fn from(failure: Failure) -> Self {
        Self::Failure(failure)
    }
}

impl From<CryptoError> for Unmade {
    fn from(error: CryptoError) -> Self {
        Self::Failure(Failure::Crypto(error))
    }
}

/// Ключ кода и откуда он: соль и итерации пароля копий устройства ложатся
/// в замок кода, как у файла копии.
pub struct Sealing<'a> {
    pub header: &'a Header,
    pub key: &'a Key,
    pub salt: &'a [u8],
    pub iterations: u32,
}

/// Собрать код из базы: изменённые с `header.since` записи — одной
/// транзакцией чтения, — сжать, зашифровать и запечатать. У кода файлов
/// (`Header::files`) — файлы книги и логотипы (задача 116).
pub async fn make(store: &Store, sealing: &Sealing<'_>) -> Result<Made, Unmade> {
    let dumped = store.read().map_err(Failure::from)?.dump().await.map_err(Failure::from)?;
    let picked = pick(&dumped, sealing.header.since(), sealing.header.files()).await?;
    seal(picked, sealing).await
}

/// Что из выгрузки базы идёт в код: изменения или файлы.
async fn pick(dumped: &[Dumped], since: Option<kvartik_core::model::record::Timestamp>, files: bool) -> Result<Picked, Unmade> {
    let raw = dumped.iter().map(Dumped::raw);
    let picked = if files {
        sync::pick_files_sliced(raw, since, &mut breathe).await
    } else {
        sync::pick_sliced(raw, since, &mut breathe).await
    };
    Ok(picked.map_err(newer)?)
}

/// Собрать код всех ИП (задача 116): записи конторы и изменения базы каждого
/// ИП списка — и убранного, как у копии всех ИП, — с меткой своего ИП. База
/// открытого ИП (`open_name`) читается его соединением, остальные
/// открываются по одной; каждая — своей транзакцией. ИП, добавленный после
/// начала кода, — всеми записями (шапка `format::sync`, «Код конторы»).
pub async fn make_all(open: &Store, open_name: &str, office_store: &Store, sealing: &Sealing<'_>) -> Result<Made, Unmade> {
    let since = sealing.header.since();
    let dumped = office_store.read().map_err(Failure::from)?.dump().await.map_err(Failure::from)?;
    let mut picked = sync::pick_office(dumped.iter().map(Dumped::raw), since).map_err(newer)?;
    for ip in route::ips(picked.entries()).map_err(newer)? {
        let new = since.is_some_and(|since| ip.added_at().is_some_and(|at| at >= since));
        let other;
        let store = if ip.base() == open_name {
            open
        } else {
            other = Store::open_base(ip.base(), |_| {}).await.map_err(Failure::from)?;
            &other
        };
        let dumped = store.read().map_err(Failure::from)?.dump().await.map_err(Failure::from)?;
        let base = pick(&dumped, if new { None } else { since }, sealing.header.files()).await?;
        picked.absorb(base.in_base(ip.id()));
    }
    seal(picked, sealing).await
}

/// Сжать, зашифровать и запечатать записи кода.
async fn seal(picked: Picked, sealing: &Sealing<'_>) -> Result<Made, Unmade> {
    let packed = sync::pack_sliced(sealing.header, picked.entries(), &mut breathe)
        .await
        .map_err(Unmade::TooLarge)?;
    let iv = crypto::random(IV_BYTES)?;
    let lock = Lock::new(sealing.iterations, sealing.salt.to_vec(), iv).ok_or(CryptoError::Failed)?;
    let ciphertext = crypto::encrypt(sealing.key, lock.iv(), &packed).await?;
    Ok(Made {
        code: sync::seal(&lock, &ciphertext),
        changed: picked.changed(sealing.header.since()),
        skipped: picked.skipped(),
        unreadable: picked.unreadable(),
    })
}

/// Почему код не прочитан.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Unread {
    /// Пароль не подошёл или шифра нет.
    Crypto(CryptoError),
    /// Код испорчен или новее программы.
    Code(CodeError),
}

/// Расшифровать и прочитать код. Пароль — уже приведённый
/// (`crypto::normalize`). Вместе с записями — ключ: по нему устройство
/// запомнит пароль, если своего ещё нет.
pub async fn unlock(sealed: &Sealed, password: &str) -> Result<(Code, Key), Unread> {
    let lock = sealed.lock();
    let key = crypto::derive(password, lock.salt(), lock.iterations())
        .await
        .map_err(Unread::Crypto)?;
    let code = unlock_with(sealed, &key).await?;
    Ok((code, key))
}

/// Расшифровать и прочитать код уже готовым ключом — ключом прошлого кода
/// того же пароля (код файлов вслед за кодом изменений, задача 116).
pub async fn unlock_with(sealed: &Sealed, key: &Key) -> Result<Code, Unread> {
    let lock = sealed.lock();
    let plain = crypto::decrypt(key, lock.iv(), sealed.ciphertext())
        .await
        .map_err(Unread::Crypto)?;
    sync::read_sliced(sealed, &plain, &mut breathe).await.map_err(Unread::Code)
}
