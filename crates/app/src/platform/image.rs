//! Фото для вложения книги (задача 14): уменьшить до сохранения.
//!
//! Фото чека с телефона — 3–5 МБ и 4000 точек по длинной стороне. Такое
//! легло бы в базу, в каждую копию и в каждое её восстановление, а копия
//! больше своего предела уже не открывается (`format::backup`). Чек
//! читается и на 1600 точках, а JPEG такого размера весит сотни килобайт.
//!
//! Перекодирует браузер: `createImageBitmap` разбирает файл (и поворачивает
//! фото по EXIF — так по умолчанию, `imageOrientation: "from-image"`),
//! `OffscreenCanvas` рисует меньше и отдаёт JPEG (`convertToBlob`). Своего
//! кодека JPEG в wasm нет: он прибавил бы к частям окна сотни килобайт,
//! а браузер умеет это сам и не на главном потоке Rust. OffscreenCanvas
//! с 2D — Chrome 69, Firefox 105, Safari 16.4 (по данным MDN, сверено
//! 2026-10-15).

use js_sys::{Array, Uint8Array};
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{
    Blob, BlobPropertyBag, ImageBitmap, ImageEncodeOptions, OffscreenCanvas,
    OffscreenCanvasRenderingContext2d,
};

/// Длинная сторона фото после уменьшения. Чек, снятый на телефон, на ней
/// читается целиком, а текст договора на листе A4 — с увеличением.
const MAX_SIDE: u32 = 1600;

/// Фото не больше [`MAX_SIDE`] и не тяжелее этого остаётся как есть:
/// перекодирование ухудшило бы его, а места почти не сберегло бы.
const KEEP_BYTES: usize = 400 * 1024;

/// Качество JPEG при перекодировании: на 0,8 мелкий шрифт чека без
/// заметных следов сжатия, а весит фото 1600 точек 200–400 КБ.
const QUALITY: f64 = 0.8;

/// Почему фото не уменьшилось: браузер не разобрал файл или не нарисовал.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Unreadable;

/// Фото для вложения.
pub struct Shrunk {
    /// Байты файла.
    pub bytes: Vec<u8>,
    /// Перекодировано ли в JPEG: имя файла тогда меняет расширение.
    pub reencoded: bool,
}

/// Фото для вложения: как есть, если оно и так невелико и `keep` — его вид
/// годится во вложение (JPEG или PNG), иначе — JPEG не больше [`MAX_SIDE`]
/// точек по длинной стороне. Так и фото HEIC или WebP, которое браузер
/// умеет открыть, ложится JPEG. `mime` — тип файла. PNG с прозрачностью
/// ложится на цвет бумаги (`--fill-paper` из `tokens.css`, Правило 4):
/// в JPEG прозрачности нет, и без фона она стала бы чёрной — вместе
/// с чёрным текстом чека.
pub async fn shrink(bytes: &[u8], mime: &str, keep: bool) -> Result<Shrunk, Unreadable> {
    let window = web_sys::window().ok_or(Unreadable)?;
    let options = BlobPropertyBag::new();
    options.set_type(mime);
    let blob = Blob::new_with_buffer_source_sequence_and_options(
        &Array::of1(&Uint8Array::from(bytes)),
        &options,
    )
    .map_err(|error| log("Blob", &error))?;
    let promise = window
        .create_image_bitmap_with_blob(&blob)
        .map_err(|error| log("createImageBitmap", &error))?;
    let bitmap: ImageBitmap = promise
        .await
        .map_err(|error| log("createImageBitmap", &error))?
        .unchecked_into();
    let (width, height) = (bitmap.width(), bitmap.height());
    let longest = width.max(height);
    if longest == 0 {
        bitmap.close();
        return Err(Unreadable);
    }
    if keep && longest <= MAX_SIDE && bytes.len() <= KEEP_BYTES {
        bitmap.close();
        return Ok(Shrunk {
            bytes: bytes.to_vec(),
            reencoded: false,
        });
    }
    let scale = f64::from(MAX_SIDE.min(longest)) / f64::from(longest);
    // Округление вверх: сторона в одну точку не исчезает.
    let to = |side: u32| ((f64::from(side) * scale).round() as u32).max(1);
    let (w, h) = (to(width), to(height));
    let encoded = draw(&bitmap, w, h, &paper()).await;
    bitmap.close();
    Ok(Shrunk {
        bytes: encoded?,
        reencoded: true,
    })
}

/// Нарисовать картинку размером `w`×`h` на фоне `background` и отдать JPEG.
async fn draw(bitmap: &ImageBitmap, w: u32, h: u32, background: &str) -> Result<Vec<u8>, Unreadable> {
    let canvas = OffscreenCanvas::new(w, h).map_err(|error| log("OffscreenCanvas", &error))?;
    let context: OffscreenCanvasRenderingContext2d = canvas
        .get_context("2d")
        .map_err(|error| log("getContext", &error))?
        .ok_or(Unreadable)?
        .unchecked_into();
    if !background.is_empty() {
        context.set_fill_style_str(background);
        context.fill_rect(0.0, 0.0, f64::from(w), f64::from(h));
    }
    context
        .draw_image_with_image_bitmap_and_dw_and_dh(bitmap, 0.0, 0.0, f64::from(w), f64::from(h))
        .map_err(|error| log("drawImage", &error))?;
    let options = ImageEncodeOptions::new();
    options.set_type("image/jpeg");
    options.set_quality(QUALITY);
    let promise = canvas
        .convert_to_blob_with_options(&options)
        .map_err(|error| log("convertToBlob", &error))?;
    let blob: Blob = promise
        .await
        .map_err(|error| log("convertToBlob", &error))?
        .unchecked_into();
    let buffer = blob
        .array_buffer()
        .await
        .map_err(|error| log("arrayBuffer", &error))?;
    Ok(Uint8Array::new(&buffer).to_vec())
}

/// Цвет бумаги из `tokens.css` — переменная `--fill-paper` корня страницы.
/// Нет её (страница без стилей) — пусто, и фон не рисуется: прозрачное
/// станет чёрным, но это лучше второго, молчаливого источника цвета
/// литералом здесь (Правило 4).
fn paper() -> String {
    web_sys::window()
        .and_then(|window| {
            let root = window.document()?.document_element()?;
            let style = window.get_computed_style(&root).ok()??;
            style.get_property_value("--fill-paper").ok()
        })
        .map(|value| value.trim().to_owned())
        .unwrap_or_default()
}

fn log(context: &str, error: &JsValue) -> Unreadable {
    web_sys::console::error_2(&format!("Kvartik, фото ({context}):").into(), error);
    Unreadable
}
