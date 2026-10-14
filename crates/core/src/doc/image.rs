//! Логотип для PDF: картинка PNG или JPEG из профиля (`model::logo`) —
//! в виде, который PDF берёт.
//!
//! JPEG ложится в PDF как есть (`/DCTDecode`): из файла читаются только
//! размеры и число цветов. PNG разбирается: сжатие у PNG и PDF одно
//! (deflate), но у PNG каждая строка ещё и закодирована своим фильтром,
//! а прозрачность лежит в тех же байтах, что цвет, — PDF же хочет цвет
//! и прозрачность раздельно, прозрачность — маской (`/SMask`). Поэтому
//! картинка разжимается, строки раскодируются, а цвет и прозрачность
//! пишутся двумя потоками.
//!
//! Файл — от человека, а в копии — от кого угодно (Правило 13): разбор не
//! паникует, а размер распакованной картинки ограничен до распаковки.

use miniz_oxide::inflate::decompress_to_vec_zlib_with_limit;

use crate::model::logo::{Image, ImageFormat};

/// Почему логотип не взять в PDF.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum PictureError {
    /// Файл испорчен или в нём то, чего PNG и JPEG не бывают.
    Malformed,
    /// Картинка больше [`MAX_PIXELS`]: распакованная, она заняла бы
    /// десятки мегабайт памяти телефона.
    TooLarge,
}

/// Самая большая картинка — 4 мегапикселя (2048 × 2048): распакованная
/// с прозрачностью, это 16 МБ. Логотип в шапке документа — несколько
/// сантиметров, и для печати ему хватает в разы меньше.
pub const MAX_PIXELS: u64 = 2048 * 2048;

/// Логотип, готовый для PDF.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Picture<'a> {
    /// JPEG — байты файла как есть.
    Jpeg {
        bytes: &'a [u8],
        width: u32,
        height: u32,
        /// Цветов: 1 — серый, 3 — RGB, 4 — CMYK.
        components: u8,
        /// CMYK от Adobe записан наоборот (маркер APP14), и PDF должен его
        /// перевернуть (`/Decode`).
        inverted: bool,
    },
    /// Распакованный PNG: цвет по строкам без фильтров — серый (байт на
    /// точку) или RGB (три), — и прозрачность, если она где-то есть.
    Raster {
        width: u32,
        height: u32,
        gray: bool,
        color: Vec<u8>,
        alpha: Option<Vec<u8>>,
    },
}

impl<'a> Picture<'a> {
    /// Логотип из картинки профиля.
    pub fn new(image: &'a Image) -> Result<Self, PictureError> {
        match image.format() {
            ImageFormat::Jpeg => jpeg(image.bytes()),
            ImageFormat::Png => png(image.bytes()),
        }
    }

    /// Ширина и высота в точках.
    pub const fn size(&self) -> (u32, u32) {
        match self {
            Picture::Jpeg { width, height, .. } | Picture::Raster { width, height, .. } => {
                (*width, *height)
            }
        }
    }
}

fn u16_at(bytes: &[u8], at: usize) -> Result<u16, PictureError> {
    match bytes.get(at..at + 2) {
        Some(&[a, b]) => Ok(u16::from_be_bytes([a, b])),
        _ => Err(PictureError::Malformed),
    }
}

fn u32_at(bytes: &[u8], at: usize) -> Result<u32, PictureError> {
    match bytes.get(at..at + 4) {
        Some(&[a, b, c, d]) => Ok(u32::from_be_bytes([a, b, c, d])),
        _ => Err(PictureError::Malformed),
    }
}

fn checked_size(width: u32, height: u32) -> Result<(), PictureError> {
    if width == 0 || height == 0 {
        return Err(PictureError::Malformed);
    }
    if u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err(PictureError::TooLarge);
    }
    Ok(())
}

/// Размеры и цвета JPEG — из маркера начала кадра (SOF, ITU-T T.81,
/// приложение B): маркеры идут друг за другом, у каждого — длина.
fn jpeg(bytes: &[u8]) -> Result<Picture<'_>, PictureError> {
    let mut at = 2;
    let mut adobe = false;
    loop {
        // Маркер — 0xFF и его код; лишние 0xFF перед кодом — заполнитель.
        if bytes.get(at) != Some(&0xFF) {
            return Err(PictureError::Malformed);
        }
        while bytes.get(at) == Some(&0xFF) {
            at += 1;
        }
        let code = *bytes.get(at).ok_or(PictureError::Malformed)?;
        at += 1;
        // Маркеры без длины: TEM и RST0–RST7.
        if code == 0x01 || (0xD0..=0xD7).contains(&code) {
            continue;
        }
        let len = usize::from(u16_at(bytes, at)?);
        if len < 2 {
            return Err(PictureError::Malformed);
        }
        match code {
            // APP14 «Adobe»: CMYK записан наоборот.
            0xEE => adobe = bytes.get(at + 2..at + 7) == Some(b"Adobe"),
            // SOF0–SOF15, кроме DHT (C4), JPG (C8) и DAC (CC).
            0xC0..=0xCF if !matches!(code, 0xC4 | 0xC8 | 0xCC) => {
                let height = u32::from(u16_at(bytes, at + 3)?);
                let width = u32::from(u16_at(bytes, at + 5)?);
                let components = *bytes.get(at + 7).ok_or(PictureError::Malformed)?;
                if bytes.get(at + 2) != Some(&8) || !matches!(components, 1 | 3 | 4) {
                    return Err(PictureError::Malformed);
                }
                checked_size(width, height)?;
                return Ok(Picture::Jpeg {
                    bytes,
                    width,
                    height,
                    components,
                    inverted: components == 4 && adobe,
                });
            }
            // Начало данных (SOS) или конец (EOI) до кадра — файл без размеров.
            0xDA | 0xD9 => return Err(PictureError::Malformed),
            _ => {}
        }
        at += len;
    }
}

/// Заголовок PNG (`IHDR`, RFC 2083, §4.1.1).
struct Header {
    width: u32,
    height: u32,
    depth: u8,
    color: u8,
    interlaced: bool,
}

impl Header {
    /// Значений на точку: серый — 1, RGB — 3, палитра — 1, серый
    /// с прозрачностью — 2, RGBA — 4.
    fn channels(&self) -> usize {
        match self.color {
            2 => 3,
            4 => 2,
            6 => 4,
            _ => 1,
        }
    }

    /// Бит на точку.
    fn bits(&self) -> usize {
        self.channels() * usize::from(self.depth)
    }

    /// Байтов в строке шириной `width` без байта фильтра.
    fn row_bytes(&self, width: u32) -> usize {
        (width as usize * self.bits()).div_ceil(8)
    }
}

/// Разбор PNG: заголовок, палитра, прозрачность и данные (RFC 2083).
fn png(bytes: &[u8]) -> Result<Picture<'static>, PictureError> {
    let mut at = 8;
    let mut header = None;
    let mut palette: &[u8] = &[];
    let mut transparency: &[u8] = &[];
    let mut data = Vec::new();
    while at + 8 <= bytes.len() {
        let len = u32_at(bytes, at)? as usize;
        let kind = bytes.get(at + 4..at + 8).ok_or(PictureError::Malformed)?;
        // Длина — из файла: на wasm32 сумма могла бы переполниться.
        let end = (at + 8).checked_add(len).ok_or(PictureError::Malformed)?;
        let body = bytes.get(at + 8..end).ok_or(PictureError::Malformed)?;
        match kind {
            b"IHDR" => {
                let header_of = Header {
                    width: u32_at(body, 0)?,
                    height: u32_at(body, 4)?,
                    depth: *body.get(8).ok_or(PictureError::Malformed)?,
                    color: *body.get(9).ok_or(PictureError::Malformed)?,
                    interlaced: *body.get(12).ok_or(PictureError::Malformed)? == 1,
                };
                let fits = match header_of.color {
                    0 => matches!(header_of.depth, 1 | 2 | 4 | 8 | 16),
                    3 => matches!(header_of.depth, 1 | 2 | 4 | 8),
                    2 | 4 | 6 => matches!(header_of.depth, 8 | 16),
                    _ => false,
                };
                if !fits {
                    return Err(PictureError::Malformed);
                }
                checked_size(header_of.width, header_of.height)?;
                header = Some(header_of);
            }
            b"PLTE" => palette = body,
            b"tRNS" => transparency = body,
            b"IDAT" => data.extend_from_slice(body),
            b"IEND" => break,
            _ => {}
        }
        // Длина, вид, данные и сумма CRC.
        at = end + 4;
    }
    let header = header.ok_or(PictureError::Malformed)?;
    // Сколько байтов даст распаковка — известно до неё: строки всех проходов
    // с байтом фильтра. Больше — испорченный файл, и дальше него не читаем.
    let expected: usize = passes(&header)
        .iter()
        .map(|&(w, h, ..)| {
            if w == 0 {
                0
            } else {
                h as usize * (1 + header.row_bytes(w))
            }
        })
        .sum();
    let raw =
        decompress_to_vec_zlib_with_limit(&data, expected).map_err(|_| PictureError::Malformed)?;
    if raw.len() != expected {
        return Err(PictureError::Malformed);
    }
    let (width, height) = (header.width as usize, header.height as usize);
    let gray = matches!(header.color, 0 | 4);
    let step = if gray { 1 } else { 3 };
    let mut color = vec![0u8; width * height * step];
    let mut alpha = vec![255u8; width * height];
    let mut offset = 0;
    for (pass_w, pass_h, x0, y0, dx, dy) in passes(&header) {
        if pass_w == 0 {
            continue;
        }
        let row = header.row_bytes(pass_w);
        let size = pass_h as usize * (1 + row);
        let rows = unfilter(&raw[offset..offset + size], row, header.bits().div_ceil(8))?;
        offset += size;
        for py in 0..pass_h as usize {
            let line = &rows[py * row..(py + 1) * row];
            for px in 0..pass_w as usize {
                let (rgb, a) = pixel(&header, line, px, palette, transparency)?;
                let at = (y0 + py * dy) * width + x0 + px * dx;
                if gray {
                    color[at] = rgb[0];
                } else {
                    color[3 * at..3 * at + 3].copy_from_slice(&rgb);
                }
                alpha[at] = a;
            }
        }
    }
    Ok(Picture::Raster {
        width: header.width,
        height: header.height,
        gray,
        color,
        alpha: alpha.iter().any(|&a| a < 255).then_some(alpha),
    })
}

/// Проходы картинки: ширина и высота прохода, начало и шаг по x и y. Без
/// чередования проход один — вся картинка; с чередованием Adam7 — семь
/// (RFC 2083, §2.6).
fn passes(header: &Header) -> Vec<(u32, u32, usize, usize, usize, usize)> {
    if !header.interlaced {
        return vec![(header.width, header.height, 0, 0, 1, 1)];
    }
    const ADAM7: [(u32, u32, u32, u32); 7] = [
        (0, 0, 8, 8),
        (4, 0, 8, 8),
        (0, 4, 4, 8),
        (2, 0, 4, 4),
        (0, 2, 2, 4),
        (1, 0, 2, 2),
        (0, 1, 1, 2),
    ];
    ADAM7
        .iter()
        .map(|&(x0, y0, dx, dy)| {
            let w = header.width.saturating_sub(x0).div_ceil(dx);
            let h = header.height.saturating_sub(y0).div_ceil(dy);
            let w = if h == 0 { 0 } else { w };
            (w, h, x0 as usize, y0 as usize, dx as usize, dy as usize)
        })
        .collect()
}

/// Строки без фильтров (RFC 2083, §6): перед каждой — байт фильтра,
/// фильтр считает байт от соседа слева (на `bpp` байтов раньше), сверху
/// и слева сверху.
fn unfilter(data: &[u8], row: usize, bpp: usize) -> Result<Vec<u8>, PictureError> {
    let mut out = vec![0u8; data.len() / (row + 1) * row];
    for (y, line) in data.chunks_exact(row + 1).enumerate() {
        let (filter, line) = (line[0], &line[1..]);
        let (done, rest) = out.split_at_mut(y * row);
        let up = if y == 0 {
            None
        } else {
            Some(&done[(y - 1) * row..])
        };
        let current = &mut rest[..row];
        for x in 0..row {
            let a = if x >= bpp { current[x - bpp] } else { 0 };
            let b = up.map_or(0, |up| up[x]);
            let c = if x >= bpp {
                up.map_or(0, |up| up[x - bpp])
            } else {
                0
            };
            let predicted = match filter {
                0 => 0,
                1 => a,
                2 => b,
                3 => ((u16::from(a) + u16::from(b)) / 2) as u8,
                4 => paeth(a, b, c),
                _ => return Err(PictureError::Malformed),
            };
            current[x] = line[x].wrapping_add(predicted);
        }
    }
    Ok(out)
}

/// Предсказатель Паэта (RFC 2083, §6.6).
fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let p = i16::from(a) + i16::from(b) - i16::from(c);
    let (pa, pb, pc) = (
        (p - i16::from(a)).abs(),
        (p - i16::from(b)).abs(),
        (p - i16::from(c)).abs(),
    );
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

/// Значение `i`-го отсчёта строки глубиной `depth` бит: у 16-битных — старший
/// байт, у 1-, 2- и 4-битных — растянутое на 0–255 (у палитры — номер цвета).
fn sample(line: &[u8], i: usize, depth: u8, scale: bool) -> Result<u8, PictureError> {
    let get = |at: usize| line.get(at).copied().ok_or(PictureError::Malformed);
    Ok(match depth {
        8 => get(i)?,
        16 => get(2 * i)?,
        bits => {
            let bits = usize::from(bits);
            let per_byte = 8 / bits;
            let byte = get(i / per_byte)?;
            let shift = 8 - bits * (i % per_byte + 1);
            let value = (byte >> shift) & ((1u8 << bits) - 1);
            if scale {
                (u16::from(value) * 255 / ((1u16 << bits) - 1)) as u8
            } else {
                value
            }
        }
    })
}

/// Точка `x` строки: цвет и прозрачность.
fn pixel(
    header: &Header,
    line: &[u8],
    x: usize,
    palette: &[u8],
    transparency: &[u8],
) -> Result<([u8; 3], u8), PictureError> {
    let channels = header.channels();
    let depth = header.depth;
    let s = |c: usize| sample(line, x * channels + c, depth, true);
    Ok(match header.color {
        0 => {
            let g = s(0)?;
            // Прозрачный серый — один, записанный в tRNS (своей глубиной).
            let key = u16_at(transparency, 0).ok();
            let raw = if depth == 16 {
                u16_at(line, 2 * x).ok()
            } else {
                Some(u16::from(sample(line, x, depth, false)?))
            };
            let a = if key.is_some() && key == raw { 0 } else { 255 };
            ([g, g, g], a)
        }
        2 => {
            let rgb = [s(0)?, s(1)?, s(2)?];
            let a = if transparency.len() >= 6 {
                let raw = |c: usize| -> Result<u16, PictureError> {
                    if depth == 16 {
                        u16_at(line, 6 * x + 2 * c)
                    } else {
                        Ok(u16::from(
                            line.get(3 * x + c)
                                .copied()
                                .ok_or(PictureError::Malformed)?,
                        ))
                    }
                };
                let key = [
                    u16_at(transparency, 0)?,
                    u16_at(transparency, 2)?,
                    u16_at(transparency, 4)?,
                ];
                if [raw(0)?, raw(1)?, raw(2)?] == key {
                    0
                } else {
                    255
                }
            } else {
                255
            };
            (rgb, a)
        }
        3 => {
            let i = usize::from(sample(line, x, depth, false)?);
            let rgb = palette
                .get(3 * i..3 * i + 3)
                .ok_or(PictureError::Malformed)?;
            let a = transparency.get(i).copied().unwrap_or(255);
            ([rgb[0], rgb[1], rgb[2]], a)
        }
        4 => {
            let g = s(0)?;
            ([g, g, g], s(1)?)
        }
        _ => ([s(0)?, s(1)?, s(2)?], s(3)?),
    })
}

#[cfg(test)]
mod tests;
