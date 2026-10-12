//! Шифр файла копии — WebCrypto браузера: ключ из пароля (PBKDF2 с
//! SHA-256) и AES-256-GCM. Параметры и формат файла задаёт ядро
//! (`kvartik_core::format::backup`), здесь — только вызовы браузера.
//!
//! WebCrypto, а не крейт на Rust: PBKDF2 на 600 000 итераций в wasm шёл бы
//! в главном потоке и заморозил бы страницу на телефоне (Правило 1),
//! а браузер считает его в своём потоке — Chrome, Firefox и Safari
//! (по исходникам движков; разведка задачи 7, 2026-10-12). AES-GCM Chrome
//! выполняет в потоке страницы, но это один проход по файлу в пару
//! мегабайт — миллисекунды. И своей криптографии в wasm не прибавляется.
//!
//! `crypto.subtle` есть только в защищённом контексте — HTTPS или
//! `localhost` и `127.0.0.1`; без него свойство просто не определено
//! (по спецификации WebCrypto, `[SecureContext]`; сверено 2026-10-12).
//! Тогда шифра нет — [`CryptoError::Unavailable`], и окно говорит, почему.

use js_sys::{Array, ArrayBuffer, JsString, Promise, Reflect, Uint8Array};
use kvartik_core::format::backup::{CHECK_BYTES, IV_BYTES};
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{AesGcmParams, Crypto, CryptoKey, DomException, Pbkdf2Params, SubtleCrypto};

/// Что не так с шифром.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CryptoError {
    /// Шифра нет: страница открыта не по HTTPS.
    Unavailable,
    /// Расшифровка не прошла проверку подлинности: пароль не тот или файл
    /// изменён. Различить их GCM не может.
    WrongPassword,
    /// Прочее. Подробность — в консоли.
    Failed,
}

/// Пароль в том виде, из которого получается ключ: в форме NFC.
///
/// Одну и ту же букву можно набрать разными точками кода — «й» одной или
/// «и» с кратким; клавиатуры разных устройств делают это по-разному, и без
/// приведения пароль, набранный на телефоне, не подошёл бы к копии
/// с компьютера. NFC советует NIST SP 800-63B-4 (§3.1.1.2); длину пароля
/// ядро считает уже после приведения. `normalize` есть во всех браузерах
/// с 2016 года (MDN, сверено 2026-10-12).
pub fn normalize(password: &str) -> String {
    JsString::from(password).normalize("NFC").into()
}

/// `len` случайных байтов — соль или вектор. Источник — `crypto.getRandomValues`,
/// он есть и без HTTPS.
pub fn random(len: usize) -> Result<Vec<u8>, CryptoError> {
    let crypto = crypto().ok_or(CryptoError::Failed)?;
    let length = u32::try_from(len).map_err(|_| CryptoError::Failed)?;
    let bytes = Uint8Array::new_with_length(length);
    crypto
        .get_random_values_with_js_u8_array(&bytes)
        .map_err(|error| failure("getRandomValues", &error))?;
    Ok(bytes.to_vec())
}

/// Ключ из пароля и его проверочное значение — SHA-256 ключа
/// (`DeviceKey` в ядре).
pub struct Key {
    aes: CryptoKey,
    check: Vec<u8>,
}

impl Key {
    /// Проверочное значение — то, что запоминает устройство.
    pub fn check(&self) -> &[u8] {
        &self.check
    }
}

/// Ключ из пароля (уже приведённого — [`normalize`]), соли и числа итераций.
pub async fn derive(password: &str, salt: &[u8], iterations: u32) -> Result<Key, CryptoError> {
    let subtle = subtle()?;
    let secret = Uint8Array::from(password.as_bytes());
    let base: CryptoKey = call(
        "importKey",
        subtle.import_key_with_str(
            "raw",
            &secret,
            "PBKDF2",
            false,
            &Array::of1(&"deriveBits".into()),
        ),
    )
    .await?;
    let params =
        Pbkdf2Params::new_with_str("PBKDF2", "SHA-256", iterations, &Uint8Array::from(salt));
    let bits: ArrayBuffer = call(
        "deriveBits",
        subtle.derive_bits_with_object(&params, &base, 256),
    )
    .await?;
    let digest: ArrayBuffer = call(
        "digest",
        subtle.digest_with_str_and_js_u8_array("SHA-256", &Uint8Array::new(&bits)),
    )
    .await?;
    // Ключ неизвлекаемый: из браузера его не достать даже коду окна.
    let aes: CryptoKey = call(
        "importKey",
        subtle.import_key_with_str(
            "raw",
            &bits,
            "AES-GCM",
            false,
            &Array::of2(&"encrypt".into(), &"decrypt".into()),
        ),
    )
    .await?;
    let check = Uint8Array::new(&digest).to_vec();
    if check.len() != CHECK_BYTES {
        return Err(CryptoError::Failed);
    }
    Ok(Key { aes, check })
}

/// Зашифровать: шифротекст вместе с меткой подлинности.
pub async fn encrypt(key: &Key, iv: &[u8], plain: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let subtle = subtle()?;
    let params = gcm(iv)?;
    let data: ArrayBuffer = call(
        "encrypt",
        subtle.encrypt_with_object_and_js_u8_array(&params, &key.aes, &Uint8Array::from(plain)),
    )
    .await?;
    Ok(Uint8Array::new(&data).to_vec())
}

/// Расшифровать. Метка подлинности не сошлась — [`CryptoError::WrongPassword`].
pub async fn decrypt(key: &Key, iv: &[u8], data: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let subtle = subtle()?;
    let params = gcm(iv)?;
    let promise = subtle
        .decrypt_with_object_and_js_u8_array(&params, &key.aes, &Uint8Array::from(data))
        .map_err(|error| failure("decrypt", &error))?;
    match promise.await {
        Ok(plain) => Ok(Uint8Array::new(&plain).to_vec()),
        // `OperationError` — ответ WebCrypto на несошедшуюся метку
        // (по спецификации WebCrypto, AES-GCM, «decrypt»).
        Err(error) if exception_name(&error).as_deref() == Some("OperationError") => {
            Err(CryptoError::WrongPassword)
        }
        Err(error) => Err(failure("decrypt", &error)),
    }
}

/// Параметры AES-GCM: вектор и метка в 128 бит (по умолчанию).
fn gcm(iv: &[u8]) -> Result<AesGcmParams, CryptoError> {
    if iv.len() != IV_BYTES {
        return Err(CryptoError::Failed);
    }
    Ok(AesGcmParams::new_with_u8_array(
        "AES-GCM",
        &Uint8Array::from(iv),
    ))
}

fn crypto() -> Option<Crypto> {
    web_sys::window()?.crypto().ok()
}

/// `crypto.subtle`; без защищённого контекста его нет.
fn subtle() -> Result<SubtleCrypto, CryptoError> {
    let crypto = crypto().ok_or(CryptoError::Unavailable)?;
    let subtle = Reflect::get(&crypto, &"subtle".into()).map_err(|_| CryptoError::Unavailable)?;
    if subtle.is_undefined() || subtle.is_null() {
        return Err(CryptoError::Unavailable);
    }
    Ok(subtle.unchecked_into())
}

/// Дождаться обещания WebCrypto и взять его значение нужного типа.
async fn call<T: JsCast>(
    context: &str,
    started: Result<Promise, JsValue>,
) -> Result<T, CryptoError> {
    let promise = started.map_err(|error| failure(context, &error))?;
    let value = promise.await.map_err(|error| failure(context, &error))?;
    value.dyn_into().map_err(|value| failure(context, &value))
}

fn exception_name(error: &JsValue) -> Option<String> {
    error.dyn_ref::<DomException>().map(DomException::name)
}

/// Ошибка шифра — в консоль для разработчика; наружу ничего не уходит
/// (Правило 13).
fn failure(context: &str, error: &JsValue) -> CryptoError {
    web_sys::console::error_2(&format!("Kvartik, шифр ({context}):").into(), error);
    match exception_name(error).as_deref() {
        Some("NotSupportedError") => CryptoError::Unavailable,
        _ => CryptoError::Failed,
    }
}
