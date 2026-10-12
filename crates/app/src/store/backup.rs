//! Файл копии из базы и обратно (задача 7).
//!
//! Что в файле и как записи сливаются — ядро (`kvartik_core::format::backup`),
//! шифр — WebCrypto (`crate::platform::crypto`). Здесь — путь между ними
//! и базой: прочитать все записи одной транзакцией, собрать и зашифровать
//! файл; расшифровать, разобрать и слить с базой — тоже одной транзакцией
//! (одно действие — одна транзакция, Правило 2).

use kvartik_core::format::FormatError;
use kvartik_core::format::backup::{
    self, BackupError, Entry, Header, IV_BYTES, Kind, Lock, Sealed,
};

use super::{Dumped, Store, StoreError, Writes};
use crate::platform::crypto::{self, CryptoError, Key};

/// Почему копия не сделана или не восстановлена.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Failure {
    /// Хранилище отказало.
    Store(StoreError),
    /// Шифр отказал или пароль не подошёл.
    Crypto(CryptoError),
    /// Файл копии не открывается.
    File(BackupError),
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

/// Собрать копию всей базы и зашифровать её ключом `key`. Соль и число
/// итераций, из которых получен ключ, ложатся в замок файла: по ним ключ
/// получат снова при восстановлении.
pub async fn make(
    store: &Store,
    kinds: &[Kind],
    header: &Header,
    key: &Key,
    salt: &[u8],
    iterations: u32,
) -> Result<Made, Failure> {
    let dumped = store.read()?.dump().await?;
    let collected = backup::collect(kinds, dumped.iter().map(Dumped::raw)).map_err(newer)?;
    for why in collected.unreadable() {
        super::log("backup", &format!("не вошла в копию: {why}").into());
    }
    let plaintext = backup::plaintext(header, collected.entries());
    let iv = crypto::random(IV_BYTES)?;
    let lock = Lock::new(iterations, salt.to_vec(), iv).ok_or(CryptoError::Failed)?;
    let ciphertext = crypto::encrypt(key, lock.iv(), plaintext.as_bytes()).await?;
    Ok(Made {
        file: backup::seal(header, &lock, &ciphertext),
        records: collected.entries().len(),
        unreadable: collected.unreadable().len(),
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
    let entries = backup::read(kinds, sealed, &plaintext)?;
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

/// Слить записи копии с базой — одним действием: или легли все, что
/// решило слияние, или ни одной.
///
/// Между чтением базы и записью — только расчёт ядра, без ожидания
/// чужого: транзакция ещё жива (шапка `store/mod.rs`).
pub async fn restore(
    store: &Store,
    kinds: &[Kind],
    entries: &[Entry],
) -> Result<Restored, Failure> {
    let tx = store.write()?;
    let dumped = tx.dump().await?;
    let merge = backup::merge(kinds, entries, dumped.iter().map(Dumped::raw)).map_err(newer)?;
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
fn newer(error: FormatError) -> Failure {
    match error {
        FormatError::Newer { .. } => Failure::Store(StoreError::Newer),
        FormatError::Damaged(why) => {
            super::log("backup", &why.into());
            Failure::Store(StoreError::Damaged)
        }
    }
}
