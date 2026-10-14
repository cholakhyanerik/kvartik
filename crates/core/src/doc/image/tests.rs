//! Тесты разбора логотипа: PNG собираются здесь же, по RFC 2083, а не
//! берутся готовыми — так видно, что в каждом лежит.

use miniz_oxide::deflate::compress_to_vec_zlib;

use super::*;

/// Кусок PNG: длина, вид, данные и сумма (разбор её не проверяет — нули).
fn chunk(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut out = (body.len() as u32).to_be_bytes().to_vec();
    out.extend_from_slice(kind);
    out.extend_from_slice(body);
    out.extend_from_slice(&[0; 4]);
    out
}

/// PNG из строк, уже с байтом фильтра впереди каждой.
fn png_file(
    width: u32,
    height: u32,
    depth: u8,
    color: u8,
    interlaced: bool,
    rows: &[u8],
    extra: &[Vec<u8>],
) -> Vec<u8> {
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut header = width.to_be_bytes().to_vec();
    header.extend_from_slice(&height.to_be_bytes());
    header.extend_from_slice(&[depth, color, 0, 0, u8::from(interlaced)]);
    out.extend(chunk(b"IHDR", &header));
    for piece in extra {
        out.extend_from_slice(piece);
    }
    out.extend(chunk(b"IDAT", &compress_to_vec_zlib(rows, 6)));
    out.extend(chunk(b"IEND", &[]));
    out
}

fn raster(picture: Picture<'_>) -> (u32, u32, bool, Vec<u8>, Option<Vec<u8>>) {
    match picture {
        Picture::Raster {
            width,
            height,
            gray,
            color,
            alpha,
        } => (width, height, gray, color, alpha),
        Picture::Jpeg { .. } => panic!("ждали PNG"),
    }
}

/// RGBA с фильтрами «нет», «слева» и «сверху»: цвет и прозрачность
/// разделяются, фильтры снимаются.
#[test]
fn rgba_splits_color_and_alpha_through_filters() {
    // Две строки по две точки: (10,20,30,255) (11,22,33,128) / (10,20,30,0) (1,2,3,255).
    let row0 = [1u8, 10, 20, 30, 255, 1, 2, 3, 129]; // «слева»: вторая точка — разница.
    let row1 = [2u8, 0, 0, 0, 1, 246, 236, 226, 127]; // «сверху»: разница со строкой выше.
    let rows: Vec<u8> = row0.iter().chain(&row1).copied().collect();
    let file = png_file(2, 2, 8, 6, false, &rows, &[]);
    let (w, h, gray, color, alpha) = raster(png(&file).expect("PNG читается"));
    assert_eq!((w, h, gray), (2, 2, false));
    assert_eq!(color, [10, 20, 30, 11, 22, 33, 10, 20, 30, 1, 2, 3]);
    assert_eq!(alpha, Some(vec![255, 128, 0, 255]));
}

/// Непрозрачная картинка — без маски: маска из одних 255 — лишние байты.
#[test]
fn opaque_picture_has_no_mask() {
    let rows = [0u8, 1, 2, 3, 4, 5, 6];
    let file = png_file(2, 1, 8, 2, false, &rows, &[]);
    let (.., color, alpha) = raster(png(&file).expect("PNG читается"));
    assert_eq!(color, [1, 2, 3, 4, 5, 6]);
    assert_eq!(alpha, None);
}

/// Палитра в 2 бита с прозрачностью из `tRNS` и фильтр Паэта.
#[test]
fn palette_with_transparency_and_paeth() {
    let palette = chunk(b"PLTE", &[255, 0, 0, 0, 255, 0, 0, 0, 255, 9, 9, 9]);
    let trns = chunk(b"tRNS", &[0, 255]);
    // 4 точки по 2 бита в одном байте: 0, 1, 2, 3 → 0b00_01_10_11. Паэт на
    // первой строке без соседей сверху — то же, что «слева» на байтах.
    let rows = [4u8, 0b0001_1011];
    let file = png_file(4, 1, 2, 3, false, &rows, &[palette, trns]);
    let (.., color, alpha) = raster(png(&file).expect("PNG читается"));
    assert_eq!(color, [255, 0, 0, 0, 255, 0, 0, 0, 255, 9, 9, 9]);
    assert_eq!(alpha, Some(vec![0, 255, 255, 255]));
}

/// Серый в 1 бит растягивается на 0–255; 16 бит — старший байт.
#[test]
fn gray_bits_and_sixteen_bit_samples() {
    let file = png_file(3, 1, 1, 0, false, &[0, 0b1010_0000], &[]);
    let (.., gray, color, alpha) = raster(png(&file).expect("PNG читается"));
    assert!(gray);
    assert_eq!(color, [255, 0, 255]);
    assert_eq!(alpha, None);

    let rows = [0u8, 0x12, 0x34, 0xAB, 0xCD, 0x00, 0x01];
    let file = png_file(1, 1, 16, 2, false, &rows, &[]);
    let (.., color, _) = raster(png(&file).expect("PNG читается"));
    assert_eq!(color, [0x12, 0xAB, 0x00]);
}

/// Чередование Adam7: картинка 3 × 3 из четырёх проходов собирается на
/// свои места.
#[test]
fn interlaced_picture_lands_in_place() {
    // Серый 8 бит, значение точки — 10·y + x. Проходы 3 × 3: 1 — (0,0);
    // 4 — (2,0); 5 — (0,2) и (2,2); 6 — (1,0), (1,2); 7 — (0,1), (1,1), (2,1).
    let value = |x: u8, y: u8| 10 * y + x;
    let mut rows = Vec::new();
    rows.extend([0, value(0, 0)]); // проход 1: 1 × 1
    rows.extend([0, value(2, 0)]); // проход 4: 1 × 1
    rows.extend([0, value(0, 2), value(2, 2)]); // проход 5: 2 × 1
    rows.extend([0, value(1, 0)]); // проход 6: 1 × 2
    rows.extend([0, value(1, 2)]);
    rows.extend([0, value(0, 1), value(1, 1), value(2, 1)]); // проход 7: 3 × 1
    let file = png_file(3, 3, 8, 0, true, &rows, &[]);
    let (.., color, _) = raster(png(&file).expect("PNG читается"));
    assert_eq!(color, [0, 1, 2, 10, 11, 12, 20, 21, 22]);
}

/// Испорченный и чрезмерный файл — ошибка, а не паника и не гигабайты
/// памяти.
#[test]
fn broken_and_huge_files_are_errors() {
    let rows = [0u8, 1, 2, 3];
    let good = png_file(1, 1, 8, 2, false, &rows, &[]);
    for len in 0..good.len() {
        let _ = png(&good[..len]);
    }
    // Заявлено 100 000 × 100 000 точек.
    let huge = png_file(100_000, 100_000, 8, 2, false, &rows, &[]);
    assert_eq!(png(&huge).err(), Some(PictureError::TooLarge));
    // Данных меньше, чем строк.
    let short = png_file(2, 2, 8, 2, false, &rows, &[]);
    assert_eq!(png(&short).err(), Some(PictureError::Malformed));
    // Длина куска — за концом файла.
    let mut lying = good.clone();
    lying[8..12].copy_from_slice(&u32::MAX.to_be_bytes());
    assert_eq!(png(&lying).err(), Some(PictureError::Malformed));
    // Неизвестный фильтр.
    let bad_filter = png_file(1, 1, 8, 2, false, &[7, 1, 2, 3], &[]);
    assert_eq!(png(&bad_filter).err(), Some(PictureError::Malformed));
}

/// JPEG: размеры и цвета — из маркера кадра, после маркеров приложений;
/// байты — те же, что в файле.
#[test]
fn jpeg_size_comes_from_the_frame_marker() {
    let mut file = vec![0xFF, 0xD8];
    // APP0 JFIF длиной 16.
    file.extend([0xFF, 0xE0, 0x00, 0x10]);
    file.extend(b"JFIF\0");
    file.extend([1, 1, 0, 0, 1, 0, 1, 0, 0]);
    // SOF2 (прогрессивный): точность 8, высота 300, ширина 640, 3 цвета.
    file.extend([0xFF, 0xC2, 0x00, 0x11, 8, 0x01, 0x2C, 0x02, 0x80, 3]);
    file.extend([1, 0x22, 0, 2, 0x11, 1, 3, 0x11, 1]);
    file.extend([0xFF, 0xD9]);
    match jpeg(&file).expect("JPEG читается") {
        Picture::Jpeg {
            bytes,
            width,
            height,
            components,
            inverted,
        } => {
            assert_eq!((width, height, components, inverted), (640, 300, 3, false));
            assert_eq!(bytes, file.as_slice());
        }
        Picture::Raster { .. } => panic!("ждали JPEG"),
    }
    for len in 0..file.len() {
        let _ = jpeg(&file[..len]);
    }
    assert_eq!(
        jpeg(&[0xFF, 0xD8, 0xFF, 0xD9]).err(),
        Some(PictureError::Malformed)
    );
}
