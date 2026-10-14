//! Файлы: отдать файл человеку и прочитать выбранный им (копия, задача 7;
//! логотип, задача 9).

use js_sys::{Array, Uint8Array};
use wasm_bindgen::JsValue;
use web_sys::{Blob, BlobPropertyBag, File, Url};

/// Почему выбранный файл не прочитан.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FileError {
    /// Файл больше предела — его не читали вовсе.
    TooLarge,
    /// Файл не текст UTF-8.
    NotText,
    /// Браузер не отдал содержимое.
    Unreadable,
}

/// Текст выбранного файла, если он не больше `limit` байтов.
///
/// Читается он байтами ([`read_bytes`]), а не строкой (`text`): строка JS
/// прошла бы через UTF-16 и границу wasm лишний раз, а байты копируются
/// один раз (Правило 1), и не UTF-8 видно сразу.
pub async fn read(file: &File, limit: u64) -> Result<String, FileError> {
    String::from_utf8(read_bytes(file, limit).await?).map_err(|_| FileError::NotText)
}

/// Байты выбранного файла, если он не больше `limit` байтов.
///
/// Размер проверяется до чтения (Правило 13): `File.size` известен сразу,
/// а чтение файла в сотни мегабайт повесило бы страницу.
///
/// Читать — сразу при выборе: на iPhone выбранный файл через минуту может
/// оказаться пустым (WebKit bug 197142, открыт; разведка задачи 7,
/// 2026-10-12).
pub async fn read_bytes(file: &File, limit: u64) -> Result<Vec<u8>, FileError> {
    // Размер в JS — число; дробным или отрицательным он не бывает.
    if file.size() > limit as f64 {
        return Err(FileError::TooLarge);
    }
    let buffer = file.array_buffer().await.map_err(|error| {
        log("arrayBuffer", &error);
        FileError::Unreadable
    })?;
    Ok(Uint8Array::new(&buffer).to_vec())
}

/// Адрес `blob:` для файла из этих частей — строк или байтов — или `None`,
/// если браузер не дал собрать файл.
fn object_url(parts: &Array, mime: &str) -> Option<String> {
    let options = BlobPropertyBag::new();
    options.set_type(mime);
    // У всех `new_with_…_sequence_and_options` в web-sys один вызов —
    // `new Blob(части, параметры)`: имена различают только тип частей
    // в Rust, а части здесь — уже массив JS.
    let blob = Blob::new_with_buffer_source_sequence_and_options(parts, &options)
        .map_err(|error| log("Blob", &error))
        .ok()?;
    Url::create_object_url_with_blob(&blob)
        .map_err(|error| log("createObjectURL", &error))
        .ok()
}

/// Картинка для показа в окне (`<img src>`): адрес `blob:` на её байты.
///
/// Адрес, а не `data:` со строкой base64: строку пришлось бы собрать
/// в wasm и передать через границу целиком, а байты уходят в JS один раз
/// (Правило 1). Будущий заголовок CSP (задача 15) должен пускать картинки
/// `blob:` — `img-src 'self' blob:`: `'self'` адресов `blob:` не покрывает
/// (по спецификации CSP3).
///
/// Адрес держит байты в памяти, пока его не отпустят: [`Picture`] отпускает
/// его при сбросе.
pub struct Picture {
    url: String,
}

impl Picture {
    /// Картинка из байтов файла с типом `mime`; `None` — браузер не дал
    /// собрать файл.
    pub fn new(bytes: &[u8], mime: &str) -> Option<Self> {
        let url = object_url(&Array::of1(&Uint8Array::from(bytes)), mime)?;
        Some(Self { url })
    }

    /// Адрес для `src`.
    pub fn url(&self) -> &str {
        &self.url
    }
}

impl Drop for Picture {
    fn drop(&mut self) {
        let _ = Url::revoke_object_url(&self.url);
    }
}

/// Файл для скачивания: адрес `blob:`, который окно ставит настоящей
/// ссылке с атрибутом `download`, а человек нажимает сам.
///
/// Ссылка, а не `click()` из кода после сборки файла: так скачивание всегда
/// идёт от свежего нажатия, сколько бы ни шифровалась копия, а окно знает,
/// что файл отдан. Ссылке не нужно быть в документе, и `download` с адресом
/// `blob:` понимают все движки, на iPhone — с iOS 13 (по исходникам WebKit
/// и Firefox, данные MDN; разведка задачи 7, 2026-10-12).
///
/// Тип — `application/octet-stream`, а не `application/json`: iOS Safari
/// открывает текстовые файлы во вкладке вместо загрузки (WebKit bug 263608,
/// открыт; обход из него же). Тип файлу при выборе даёт расширение `.json`.
/// В приложении с экрана «Домой» скачивание может завести в тупик без
/// кнопки «назад» (WebKit bug 290847, открыт, iOS 16.7–18.4; про `blob:` —
/// только отзывы, что работает): проверить вживую, запасной путь — Web
/// Share (задача 13).
///
/// Адрес держит файл в памяти, пока его не отпустят: [`Download`] отпускает
/// его при сбросе — когда готова следующая копия. Начатое скачивание это не
/// прерывает: адрес разобран при нажатии (по спецификации URL; старые
/// ошибки движков исправлены к 2020 году).
pub struct Download {
    url: String,
}

impl Download {
    /// Файл из текста. `None` — браузер не дал собрать файл.
    pub fn new(text: &str) -> Option<Self> {
        let url = object_url(&Array::of1(&text.into()), "application/octet-stream")?;
        Some(Self { url })
    }

    /// Файл из байтов с типом `mime` — PDF документа (задача 12). Тип —
    /// настоящий, `application/pdf`: обход WebKit bug 263608 нужен текстовым
    /// файлам, а PDF телефон и так сохраняет или показывает своей читалкой.
    pub fn bytes(bytes: &[u8], mime: &str) -> Option<Self> {
        let url = object_url(&Array::of1(&Uint8Array::from(bytes)), mime)?;
        Some(Self { url })
    }

    /// Адрес для `href`.
    pub fn url(&self) -> &str {
        &self.url
    }
}

impl Drop for Download {
    fn drop(&mut self) {
        let _ = Url::revoke_object_url(&self.url);
    }
}

fn log(context: &str, error: &JsValue) {
    web_sys::console::error_2(&format!("Kvartik, файл ({context}):").into(), error);
}

/// Выбранный файл поля `<input type="file">`; `None` — не выбран.
pub fn chosen(input: &web_sys::HtmlInputElement) -> Option<File> {
    input.files()?.get(0)
}
