//! Код передачи изменений из базы и обратно (задача 110).
//!
//! Что в коде, как он режется на QR-кадры и куски текста и что помнит
//! принимающее устройство — ядро (`kvartik_core::format::sync`), шифр —
//! WebCrypto (`crate::platform::crypto`), пароль — пароль копий устройства,
//! как у файла копии. Ложатся принятые записи тем же путём, что копия:
//! `store::backup::restore` — одной транзакцией, со сверкой ИП.

use kvartik_core::format::backup::{IV_BYTES, Lock};
use kvartik_core::format::sync::{self, Code, CodeError, Header, Sealed, TooLarge};

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
/// транзакцией чтения, — сжать, зашифровать и запечатать.
pub async fn make(store: &Store, sealing: &Sealing<'_>) -> Result<Made, Unmade> {
    let dumped = store.read().map_err(Failure::from)?.dump().await.map_err(Failure::from)?;
    let picked = sync::pick_sliced(dumped.iter().map(Dumped::raw), sealing.header.since(), &mut breathe)
        .await
        .map_err(newer)?;
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
    let plain = crypto::decrypt(&key, lock.iv(), sealed.ciphertext())
        .await
        .map_err(Unread::Crypto)?;
    let code = sync::read_sliced(sealed, &plain, &mut breathe).await.map_err(Unread::Code)?;
    Ok((code, key))
}
