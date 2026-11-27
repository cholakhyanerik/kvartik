//! Файлы: отдать файл человеку и прочитать выбранный им (копия, задача 7;
//! логотип, задача 9).

use js_sys::{Array, Uint8Array};
use wasm_bindgen::JsValue;
use web_sys::{Blob, BlobPropertyBag, File, FilePropertyBag, Url};

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
/// только отзывы, что работает): проверить вживую. Запасной путь —
/// «Поделиться» ([`Download::file`], `platform::share`, задача 13).
///
/// Адрес держит файл в памяти, пока его не отпустят: [`Download`] отпускает
/// его при сбросе — когда готова следующая копия. Начатое скачивание это не
/// прерывает: адрес разобран при нажатии (по спецификации URL; старые
/// ошибки движков исправлены к 2020 году).
///
/// Файл — `File` с именем, а не безымянный `Blob`: тот же объект уходит
/// и в «Поделиться» (`platform::share`, задача 13), а там имя файла — то,
/// что увидит получатель. Байты переходят границу wasm один раз, при
/// сборке, а не на каждое нажатие (Правило 1).
pub struct Download {
    url: String,
    file: File,
}

impl Download {
    /// Файл `name` из текста. `None` — браузер не дал собрать файл.
    ///
    /// Байтами UTF-8, а не строкой JS: строка прошла бы через UTF-16 туда
    /// и обратно, и копия в 20 МБ стояла на этом четверть секунды на
    /// телефоне (замер 0.0.28). Байты файла — те же.
    pub fn text(text: &str, name: &str) -> Option<Self> {
        Self::bytes(text.as_bytes(), name, "application/octet-stream")
    }

    /// Ссылка на уже собранный файл — часть копии из кусков ([`Pieces`]) —
    /// под именем `name`. Байты не копируются: новый `File` ссылается на
    /// прежний.
    pub fn of(file: &File, name: &str) -> Option<Self> {
        Self::new(&Array::of1(file), name, "application/octet-stream")
    }

    /// Файл `name` из байтов с типом `mime` — PDF документа (задача 12).
    /// Тип — настоящий, `application/pdf`: обход WebKit bug 263608 нужен
    /// текстовым файлам, а PDF телефон и так сохраняет или показывает своей
    /// читалкой. И Chromium отправляет через «Поделиться» только файлы
    /// известных ему типов (`platform::share`).
    pub fn bytes(bytes: &[u8], name: &str, mime: &str) -> Option<Self> {
        Self::new(&Array::of1(&Uint8Array::from(bytes)), name, mime)
    }

    /// Файл из частей — строк или байтов. Как и у [`object_url`], у всех
    /// `new_with_…_sequence_and_options` один вызов — `new File(части,
    /// имя, параметры)`.
    fn new(parts: &Array, name: &str, mime: &str) -> Option<Self> {
        let options = FilePropertyBag::new();
        options.set_type(mime);
        let file = File::new_with_buffer_source_sequence_and_options(parts, name, &options)
            .map_err(|error| log("File", &error))
            .ok()?;
        let url = Url::create_object_url_with_blob(&file)
            .map_err(|error| log("createObjectURL", &error))
            .ok()?;
        Some(Self { url, file })
    }

    /// Адрес для `href`.
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Имя файла — для `download` у ссылки.
    pub fn name(&self) -> String {
        self.file.name()
    }

    /// Сам файл — для «Поделиться».
    pub fn file(&self) -> &File {
        &self.file
    }
}

impl Drop for Download {
    fn drop(&mut self) {
        let _ = Url::revoke_object_url(&self.url);
    }
}

/// Файл, который складывается в браузере кусками (задача 122): каждый кусок
/// уходит из памяти wasm, как только готов, и файл части копии в сотню
/// мегабайт целиком в ней не лежит. Куски — байтами UTF-8, как у
/// [`Download::text`]: строка JS прошла бы через UTF-16.
///
/// И в JS они не копятся: набралось [`BATCH_BYTES`] — пачка ложится в файл
/// браузера вместе с прежним (`new File([прежний, куски…])`), и её куски
/// свободны. Иначе рядом с файлом части лежали бы все её куски — ещё один
/// размер файла в памяти вкладки до сборки мусора.
#[derive(Default)]
pub struct Pieces {
    /// Пачка: прежний файл — первым, потом куски.
    batch: Array,
    bytes: usize,
    file: Option<File>,
    failed: bool,
}

/// Сколько байтов кусков копится до того, как лечь в файл: у части копии
/// в сотню мегабайт — меньше десятка складываний. Как браузер держит файл
/// из файла и кусков — ссылкой на прежний или его копией, — не сверено;
/// и копией это меньше десятка переписываний части за копию, а не по
/// куску в 700 КБ на каждый из полутора сотен.
const BATCH_BYTES: usize = 16 << 20;

impl Pieces {
    /// Ещё кусок.
    pub fn push(&mut self, bytes: &[u8]) {
        self.batch.push(&Uint8Array::from(bytes));
        self.bytes += bytes.len();
        if self.bytes >= BATCH_BYTES {
            self.fold();
        }
    }

    /// Пачку — в файл.
    fn fold(&mut self) {
        if self.failed {
            return;
        }
        let options = FilePropertyBag::new();
        options.set_type("application/octet-stream");
        match File::new_with_buffer_source_sequence_and_options(&self.batch, "", &options) {
            Ok(file) => {
                self.batch = Array::of1(&file);
                self.file = Some(file);
                self.bytes = 0;
            }
            Err(error) => {
                log("File", &error);
                self.failed = true;
                self.batch = Array::new();
            }
        }
    }

    /// Файл из кусков, без имени — его даёт тот, кто отдаёт файл человеку
    /// ([`Download::of`]). `None` — браузер не дал собрать файл.
    pub fn finish(mut self) -> Option<File> {
        if self.bytes > 0 || self.file.is_none() {
            self.fold();
        }
        if self.failed { None } else { self.file }
    }
}

fn log(context: &str, error: &JsValue) {
    web_sys::console::error_2(&format!("Kvartik, файл ({context}):").into(), error);
}

/// Выбранный файл поля `<input type="file">`; `None` — не выбран.
pub fn chosen(input: &web_sys::HtmlInputElement) -> Option<File> {
    input.files()?.get(0)
}

/// Все выбранные файлы поля `<input type="file" multiple>` — по порядку.
pub fn chosen_all(input: &web_sys::HtmlInputElement) -> Vec<File> {
    let Some(files) = input.files() else {
        return Vec::new();
    };
    (0..files.length()).filter_map(|at| files.get(at)).collect()
}
