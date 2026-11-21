//! Квадрат модулей — обратно в байты (п. 7.4–7.10 наоборот): слово формата,
//! слово версии, снять маску, собрать блоки, исправить ошибки и прочитать
//! данные режима байтов.

use super::{Blocks, Canvas, Ecc, MAX_VERSION, count_bits, format_word, gf, masked, version_word};

/// Почему квадрат не прочитан.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Unread {
    /// Слово версии говорит о другой версии: снять квадрат заново с её
    /// стороной.
    Version(u8),
    /// Не читается: не QR-код, слишком много ошибок или не режим байтов.
    No,
}

/// Сколько неверных битов слова формата или версии ещё узнаются: у кода БЧХ
/// формата расстояние 7, у версии — 8 (п. 7.9 и 7.10), и три бита не дают
/// спутать одно слово с другим.
const MAX_WORD_ERRORS: u32 = 3;

/// Ближайшее слово из списка `(слово, значение)` — не дальше трёх битов.
fn nearest<T: Copy>(read: u32, words: impl IntoIterator<Item = (u32, T)>) -> Option<(u32, T)> {
    words
        .into_iter()
        .map(|(word, value)| ((word ^ read).count_ones(), value))
        .filter(|&(distance, _)| distance <= MAX_WORD_ERRORS)
        .min_by_key(|&(distance, _)| distance)
}

/// Прочитать квадрат стороны `side`: `dark(x, y)` — тёмный ли модуль.
pub(super) fn read(dark: &dyn Fn(usize, usize) -> bool, side_len: usize) -> Result<Vec<u8>, Unread> {
    if side_len < 21 || !(side_len - 17).is_multiple_of(4) {
        return Err(Unread::No);
    }
    let version = u8::try_from((side_len - 17) / 4).map_err(|_| Unread::No)?;
    if version > MAX_VERSION {
        return Err(Unread::No);
    }
    let (ecc, mask) = format(dark, side_len).ok_or(Unread::No)?;
    if version >= 7 {
        let found = version_of(dark, side_len).ok_or(Unread::No)?;
        if found != version {
            return Err(Unread::Version(found));
        }
    }
    let canvas = Canvas::new(version);
    let order = canvas.data_modules();
    let blocks = Blocks::of(version, ecc);
    let mut raw = vec![0u8; blocks.total];
    for (i, &(x, y)) in order.iter().take(blocks.total * 8).enumerate() {
        if dark(x, y) ^ masked(mask, x, y) {
            raw[i / 8] |= 1 << (7 - i % 8);
        }
    }
    let data = deinterleave(&raw, &blocks).ok_or(Unread::No)?;
    payload(&data, version).ok_or(Unread::No)
}

/// Уровень и маска из слова формата: из двух копий — та, что ближе
/// к настоящему слову.
fn format(dark: &dyn Fn(usize, usize) -> bool, side_len: usize) -> Option<(Ecc, u8)> {
    let mut first = 0u32;
    let mut second = 0u32;
    let put = |word: &mut u32, i: usize, x: usize, y: usize| {
        if dark(x, y) {
            *word |= 1 << i;
        }
    };
    for i in 0..6 {
        put(&mut first, i, 8, i);
    }
    put(&mut first, 6, 8, 7);
    put(&mut first, 7, 8, 8);
    put(&mut first, 8, 7, 8);
    for i in 9..15 {
        put(&mut first, i, 14 - i, 8);
    }
    for i in 0..8 {
        put(&mut second, i, side_len - 1 - i, 8);
    }
    for i in 8..15 {
        put(&mut second, i, 8, side_len - 15 + i);
    }
    let words = || {
        [Ecc::Low, Ecc::Medium, Ecc::Quartile, Ecc::High]
            .into_iter()
            .flat_map(|ecc| (0..8u8).map(move |mask| (format_word(ecc, mask), (ecc, mask))))
    };
    let a = nearest(first, words());
    let b = nearest(second, words());
    match (a, b) {
        (Some(a), Some(b)) => Some(if a.0 <= b.0 { a.1 } else { b.1 }),
        (Some(one), None) | (None, Some(one)) => Some(one.1),
        (None, None) => None,
    }
}

/// Версия из слова версии (от 7-й): из двух копий — ближе к настоящему.
fn version_of(dark: &dyn Fn(usize, usize) -> bool, side_len: usize) -> Option<u8> {
    let mut first = 0u32;
    let mut second = 0u32;
    for i in 0..18 {
        let a = side_len - 11 + i % 3;
        let b = i / 3;
        if dark(a, b) {
            first |= 1 << i;
        }
        if dark(b, a) {
            second |= 1 << i;
        }
    }
    let words = || (7..=MAX_VERSION).map(|version| (version_word(version), version));
    let a = nearest(first, words());
    let b = nearest(second, words());
    match (a, b) {
        (Some(a), Some(b)) => Some(if a.0 <= b.0 { a.1 } else { b.1 }),
        (Some(one), None) | (None, Some(one)) => Some(one.1),
        (None, None) => None,
    }
}

/// Блоки из перемешанных байтов, каждый — с исправленными ошибками; вместе —
/// байты данных. `None` — какой-то блок не исправился.
fn deinterleave(raw: &[u8], blocks: &Blocks) -> Option<Vec<u8>> {
    let short_len = blocks.short_len();
    let mut split = vec![vec![0u8; short_len + 1]; blocks.count];
    let mut next = raw.iter();
    for i in 0..=short_len {
        for (j, block) in split.iter_mut().enumerate() {
            if i != short_len - blocks.ecc || j >= blocks.short() {
                block[i] = *next.next()?;
            }
        }
    }
    let mut data = Vec::with_capacity(blocks.data());
    for (j, mut block) in split.into_iter().enumerate() {
        if j < blocks.short() {
            block.remove(short_len - blocks.ecc);
        }
        gf::correct(&mut block, blocks.ecc)?;
        data.extend_from_slice(&block[..block.len() - blocks.ecc]);
    }
    Some(data)
}

/// Данные режима байтов из байтов данных: указатель режима 0100, длина,
/// байты; указатель 0000 или конец — конец данных. Указатель ECI (0111)
/// пропускается вместе с номером: текст он не меняет, а байты — те же.
/// Другой режим — не наш код.
fn payload(data: &[u8], version: u8) -> Option<Vec<u8>> {
    let mut bits = Reader { data, at: 0 };
    let mut out = Vec::new();
    loop {
        let Some(mode) = bits.take(4) else {
            return Some(out);
        };
        match mode {
            0b0000 => return Some(out),
            0b0100 => {
                let count = bits.take(count_bits(version))?;
                for _ in 0..count {
                    out.push(u8::try_from(bits.take(8)?).ok()?);
                }
            }
            0b0111 => {
                let first = bits.take(8)?;
                let extra = if first & 0x80 == 0 {
                    0
                } else if first & 0x40 == 0 {
                    8
                } else {
                    16
                };
                bits.take(extra)?;
            }
            _ => return None,
        }
    }
}

/// Биты подряд — от старшего.
struct Reader<'a> {
    data: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn take(&mut self, count: usize) -> Option<u32> {
        if self.at + count > self.data.len() * 8 {
            return None;
        }
        let mut value = 0u32;
        for _ in 0..count {
            let bit = self.data[self.at / 8] >> (7 - self.at % 8) & 1;
            value = value << 1 | u32::from(bit);
            self.at += 1;
        }
        Some(value)
    }
}
