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
    let bitmap = bitmap(bytes, mime).await?;
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
    let encoded = draw(&bitmap, w, h, &paper(), JPEG).await;
    bitmap.close();
    Ok(Shrunk {
        bytes: encoded?,
        reencoded: true,
    })
}

/// Длинная сторона уменьшенного логотипа: в PDF он — значок в углу листа
/// (`doc::sheet`), и 512 точек хватает с запасом, и на печати тоже.
const LOGO_SIDE: u32 = 512;

/// Логотип не больше `max` байтов (полный прогон 2026-11-10, п. 13): картинка,
/// которую браузер умеет открыть, — PNG не больше [`LOGO_SIDE`] точек по
/// длинной стороне, прозрачность сохраняется; не уложился — JPEG на цвете
/// бумаги, как фото книги. `Ok(None)` — не уложился и он.
pub async fn logo(bytes: &[u8], mime: &str, max: usize) -> Result<Option<Vec<u8>>, Unreadable> {
    let bitmap = bitmap(bytes, mime).await?;
    let (width, height) = (bitmap.width(), bitmap.height());
    let longest = width.max(height);
    if longest == 0 {
        bitmap.close();
        return Err(Unreadable);
    }
    let scale = f64::from(LOGO_SIDE.min(longest)) / f64::from(longest);
    let to = |side: u32| ((f64::from(side) * scale).round() as u32).max(1);
    let (w, h) = (to(width), to(height));
    let png = draw(&bitmap, w, h, "", PNG).await;
    let fitted = match png {
        Ok(png) if png.len() <= max => Ok(Some(png)),
        _ => draw(&bitmap, w, h, &paper(), JPEG).await.map(|jpeg| (jpeg.len() <= max).then_some(jpeg)),
    };
    bitmap.close();
    fitted
}

/// Картинка из байтов файла типа `mime` — её разбирает браузер.
async fn bitmap(bytes: &[u8], mime: &str) -> Result<ImageBitmap, Unreadable> {
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
    Ok(promise
        .await
        .map_err(|error| log("createImageBitmap", &error))?
        .unchecked_into())
}

/// Во что перекодировать: тип и качество (у PNG его нет).
type Encoding = (&'static str, Option<f64>);
const JPEG: Encoding = ("image/jpeg", Some(QUALITY));
const PNG: Encoding = ("image/png", None);

/// Нарисовать картинку размером `w`×`h` на фоне `background` (пусто — без
/// фона) и отдать файлом `encoding`.
async fn draw(bitmap: &ImageBitmap, w: u32, h: u32, background: &str, encoding: Encoding) -> Result<Vec<u8>, Unreadable> {
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
    options.set_type(encoding.0);
    if let Some(quality) = encoding.1 {
        options.set_quality(quality);
    }
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


#[cfg(all(test, target_arch = "wasm32"))]
mod tests {
    use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

    use super::LOGO_SIDE;

    wasm_bindgen_test_configure!(run_in_browser);

    /// CRC-32 куска PNG (ISO 3309, как в RFC 2083, §3.4) — побитово.
    fn crc32(bytes: &[u8]) -> u32 {
        let mut crc = 0xFFFF_FFFF_u32;
        for byte in bytes {
            crc ^= u32::from(*byte);
            for _ in 0..8 {
                crc = if crc & 1 == 1 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
            }
        }
        !crc
    }

    /// Кусок PNG: длина, вид, данные, CRC.
    fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
        out.extend_from_slice(&u32::try_from(data.len()).expect("кусок невелик").to_be_bytes());
        let mut crc = kind.to_vec();
        crc.extend_from_slice(data);
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        out.extend_from_slice(&crc32(&crc).to_be_bytes());
    }

    /// PNG `width`×`height`, RGB, точки — шум: каждая строка без фильтра,
    /// а поток zlib — несжатыми блоками (RFC 1950 и 1951). Собран здесь, а
    /// не кодом программы: файл — вход, а не то, что проверяется.
    fn noise_png(width: u32, height: u32) -> Vec<u8> {
        let mut seed = 0x2545_F491_u32;
        // Строка — байт фильтра (0, без фильтра) и точки.
        let row = 1 + width as usize * 3;
        let mut raw = vec![0_u8; row * height as usize];
        for line in raw.chunks_mut(row) {
            for byte in &mut line[1..] {
                seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                *byte = (seed >> 16) as u8;
            }
        }
        let mut zlib = vec![0x78, 0x01];
        let blocks: Vec<&[u8]> = raw.chunks(65_535).collect();
        for (at, block) in blocks.iter().enumerate() {
            zlib.push(u8::from(at + 1 == blocks.len()));
            let len = u16::try_from(block.len()).expect("блок не длиннее 65535");
            zlib.extend_from_slice(&len.to_le_bytes());
            zlib.extend_from_slice(&(!len).to_le_bytes());
            zlib.extend_from_slice(block);
        }
        let (mut a, mut b) = (1_u32, 0_u32);
        for byte in &raw {
            a = (a + u32::from(*byte)) % 65_521;
            b = (b + a) % 65_521;
        }
        zlib.extend_from_slice(&((b << 16) | a).to_be_bytes());
        let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        let mut header = Vec::new();
        header.extend_from_slice(&width.to_be_bytes());
        header.extend_from_slice(&height.to_be_bytes());
        header.extend_from_slice(&[8, 2, 0, 0, 0]);
        chunk(&mut png, b"IHDR", &header);
        chunk(&mut png, b"IDAT", &zlib);
        chunk(&mut png, b"IEND", &[]);
        png
    }

    /// Ширина и высота PNG из его заголовка.
    fn png_size(png: &[u8]) -> Option<(u32, u32)> {
        let width = u32::from_be_bytes(png.get(16..20)?.try_into().ok()?);
        let height = u32::from_be_bytes(png.get(20..24)?.try_into().ok()?);
        Some((width, height))
    }

    /// Логотип больше предела (полный прогон 2026-11-10, п. 13): браузер
    /// уменьшает его до [`LOGO_SIDE`] точек по длинной стороне и отдаёт PNG;
    /// PNG не уложился — JPEG; не уложился и он — `None`. Шум 800×600 весит
    /// 1,4 МБ, и JPEG шума меньше его PNG.
    #[wasm_bindgen_test]
    async fn a_large_logo_is_made_smaller() {
        let big = noise_png(800, 600);
        assert!(big.len() > 1_000_000, "шум весит {}", big.len());
        let mut wrong = Vec::new();
        let png = super::logo(&big, "image/png", usize::MAX).await;
        let png = match png {
            Ok(Some(png)) => png,
            other => panic!("PNG не вышел: {:?}", other.map(|bytes| bytes.map(|b| b.len()))),
        };
        if !png.starts_with(&[0x89, b'P', b'N', b'G']) || png_size(&png) != Some((LOGO_SIDE, LOGO_SIDE * 3 / 4)) {
            wrong.push(format!("PNG: {:?}, {} байтов", png_size(&png), png.len()));
        }
        match super::logo(&big, "image/png", png.len() - 1).await {
            Ok(Some(jpeg)) if jpeg.starts_with(&[0xFF, 0xD8, 0xFF]) && jpeg.len() < png.len() => {}
            other => wrong.push(format!("JPEG вместо большого PNG: {:?}", other.map(|bytes| bytes.map(|b| b.len())))),
        }
        match super::logo(&big, "image/png", 1).await {
            Ok(None) => {}
            other => wrong.push(format!("в один байт: {:?}", other.map(|bytes| bytes.map(|b| b.len())))),
        }
        assert!(wrong.is_empty(), "логотип: {wrong:#?}");
    }
}
