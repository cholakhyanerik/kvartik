//! Файлы: отдать файл человеку и прочитать выбранный им (копия, задача 7).

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
/// Размер проверяется до чтения (Правило 13): `File.size` известен сразу,
/// а чтение файла в сотни мегабайт повесило бы страницу. Читается он
/// байтами (`arrayBuffer`), а не строкой (`text`): строка JS прошла бы
/// через UTF-16 и границу wasm лишний раз, а байты копируются один раз
/// (Правило 1), и не UTF-8 видно сразу.
///
/// Читать — сразу при выборе: на iPhone выбранный файл через минуту может
/// оказаться пустым (WebKit bug 197142, открыт; разведка задачи 7,
/// 2026-10-12).
pub async fn read(file: &File, limit: u64) -> Result<String, FileError> {
    // Размер в JS — число; дробным или отрицательным он не бывает.
    if file.size() > limit as f64 {
        return Err(FileError::TooLarge);
    }
    let buffer = file.array_buffer().await.map_err(|error| {
        log("arrayBuffer", &error);
        FileError::Unreadable
    })?;
    String::from_utf8(Uint8Array::new(&buffer).to_vec()).map_err(|_| FileError::NotText)
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
        let options = BlobPropertyBag::new();
        options.set_type("application/octet-stream");
        let blob = Blob::new_with_str_sequence_and_options(&Array::of1(&text.into()), &options)
            .map_err(|error| log("Blob", &error))
            .ok()?;
        let url = Url::create_object_url_with_blob(&blob)
            .map_err(|error| log("createObjectURL", &error))
            .ok()?;
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
