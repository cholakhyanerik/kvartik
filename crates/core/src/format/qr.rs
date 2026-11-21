//! QR-код (задача 110): создать — байты в квадрат модулей, и прочитать —
//! с кадра камеры обратно в байты. По ISO/IEC 18004:2015; таблицы блоков —
//! таблица 9 стандарта, и каждую версию и уровень тест сверяет модуль
//! в модуль с независимым кодировщиком `qrcodegen` (Nayuki, MIT; только
//! в тестах, в сборку он не идёт).
//!
//! Своё, а не крейт: читатель кадров камеры с утверждённой лицензией не
//! нашёлся — `rqrr` и `quircs` несут текст ISC (порт quirc), а новая
//! лицензия — решение человека (`deny.toml`); `bardecoder` тянет `image`
//! и крейты 2016 года (разбор задачи 110, 2026-11-21). Чтение камерой
//! в браузере — `BarcodeDetector` — есть только у Chrome на Android, macOS
//! и ChromeOS: у Chrome на Windows и Linux его нет, у Safari — только за
//! флагом, у Firefox — нет вовсе (MDN browser-compat-data,
//! `api/BarcodeDetector.json`, сверено 2026-11-21). А создавать и читать
//! одним кодом проще проверить: тест рисует код, искажает картинку, как
//! камера, и читает её.
//!
//! Только режим байтов (п. 7.4.5): коды программы — двоичные кадры
//! передачи (`format::sync`), а чужих она не читает.

mod gf;
mod read;
mod scan;
#[cfg(test)]
mod tests;

pub use scan::{Gray, MAX_PIXELS, scan};

/// Уровень исправления ошибок (п. 7.5.1): сколько байтов кода можно
/// потерять и всё равно прочитать — около 7, 15, 25 и 30 %.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ecc {
    Low,
    Medium,
    Quartile,
    High,
}

impl Ecc {
    /// Строка таблиц блоков.
    const fn ordinal(self) -> usize {
        match self {
            Self::Low => 0,
            Self::Medium => 1,
            Self::Quartile => 2,
            Self::High => 3,
        }
    }

    /// Два бита уровня в слове формата (таблица 12).
    const fn format_bits(self) -> u32 {
        match self {
            Self::Low => 1,
            Self::Medium => 0,
            Self::Quartile => 3,
            Self::High => 2,
        }
    }
}

/// Версии QR-кода: от 21×21 модуля до 177×177.
pub const MIN_VERSION: u8 = 1;
pub const MAX_VERSION: u8 = 40;

/// Байтов проверки в блоке — по версии (индекс 0 не бывает) и уровню.
const ECC_PER_BLOCK: [[u8; 41]; 4] = [
    [0, 7, 10, 15, 20, 26, 18, 20, 24, 30, 18, 20, 24, 26, 30, 22, 24, 28, 30, 28, 28, 28, 28, 30, 30, 26, 28, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30],
    [0, 10, 16, 26, 18, 24, 16, 18, 22, 22, 26, 30, 22, 22, 24, 24, 28, 28, 26, 26, 26, 26, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28],
    [0, 13, 22, 18, 26, 18, 24, 18, 22, 20, 24, 28, 26, 24, 20, 30, 24, 28, 28, 26, 30, 28, 30, 30, 30, 30, 28, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30],
    [0, 17, 28, 22, 16, 22, 28, 26, 26, 24, 28, 24, 28, 22, 24, 24, 30, 28, 28, 26, 28, 30, 24, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30],
];

/// Блоков кода — по версии и уровню.
const BLOCKS: [[u8; 41]; 4] = [
    [0, 1, 1, 1, 1, 1, 2, 2, 2, 2, 4, 4, 4, 4, 4, 6, 6, 6, 6, 7, 8, 8, 9, 9, 10, 12, 12, 12, 13, 14, 15, 16, 17, 18, 19, 19, 20, 21, 22, 24, 25],
    [0, 1, 1, 1, 2, 2, 4, 4, 4, 5, 5, 5, 8, 9, 9, 10, 10, 11, 13, 14, 16, 17, 17, 18, 20, 21, 23, 25, 26, 28, 29, 31, 33, 35, 37, 38, 40, 43, 45, 47, 49],
    [0, 1, 1, 2, 2, 4, 4, 6, 6, 8, 8, 8, 10, 12, 16, 12, 17, 16, 18, 21, 20, 23, 23, 25, 27, 29, 34, 34, 35, 38, 40, 43, 45, 48, 51, 53, 56, 59, 62, 65, 68],
    [0, 1, 1, 2, 4, 4, 4, 5, 6, 8, 8, 11, 11, 16, 16, 18, 16, 19, 21, 25, 25, 25, 34, 30, 32, 35, 37, 40, 42, 45, 48, 51, 54, 57, 60, 63, 66, 70, 74, 77, 81],
];

/// Сторона кода версии `version` в модулях.
pub const fn side(version: u8) -> usize {
    17 + 4 * version as usize
}

/// Сколько модулей версии несут данные и проверку (с остаточными битами).
const fn raw_modules(version: u8) -> usize {
    let v = version as usize;
    let mut result = (16 * v + 128) * v + 64;
    if v >= 2 {
        let align = v / 7 + 2;
        result -= (25 * align - 10) * align - 55;
        if v >= 7 {
            result -= 36;
        }
    }
    result
}

/// Блоки версии и уровня: сколько, байтов проверки в каждом, всего байтов.
struct Blocks {
    count: usize,
    ecc: usize,
    total: usize,
}

impl Blocks {
    const fn of(version: u8, ecc: Ecc) -> Self {
        Self {
            count: BLOCKS[ecc.ordinal()][version as usize] as usize,
            ecc: ECC_PER_BLOCK[ecc.ordinal()][version as usize] as usize,
            total: raw_modules(version) / 8,
        }
    }

    const fn data(&self) -> usize {
        self.total - self.ecc * self.count
    }

    /// Короткие блоки — первыми; длинные на байт данных длиннее.
    const fn short(&self) -> usize {
        self.count - self.total % self.count
    }

    const fn short_len(&self) -> usize {
        self.total / self.count
    }
}

/// Битов счётчика длины в режиме байтов (таблица 3).
const fn count_bits(version: u8) -> usize {
    if version <= 9 { 8 } else { 16 }
}

/// Сколько байтов вмещает код версии `version` уровня `ecc`.
pub const fn capacity(version: u8, ecc: Ecc) -> usize {
    (Blocks::of(version, ecc).data() * 8 - 4 - count_bits(version)) / 8
}

/// Центры узоров выравнивания по одной оси (приложение E).
fn alignment(version: u8) -> Vec<usize> {
    if version == 1 {
        return Vec::new();
    }
    let v = usize::from(version);
    let count = v / 7 + 2;
    let step = if version == 32 { 26 } else { (v * 4 + count * 2 + 1) / (count * 2 - 2) * 2 };
    let size = side(version);
    let mut result: Vec<usize> = (0..count - 1).map(|i| size - 7 - i * step).collect();
    result.push(6);
    result.reverse();
    result
}

/// Слово формата: уровень и маска с кодом БЧХ (15, 5) и маской 0x5412 (п. 7.9).
const fn format_word(ecc: Ecc, mask: u8) -> u32 {
    let data = ecc.format_bits() << 3 | mask as u32;
    let mut rem = data;
    let mut i = 0;
    while i < 10 {
        rem = (rem << 1) ^ ((rem >> 9) * 0x537);
        i += 1;
    }
    (data << 10 | rem) ^ 0x5412
}

/// Слово версии (от 7-й): номер с кодом БЧХ (18, 6) (п. 7.10).
const fn version_word(version: u8) -> u32 {
    let data = version as u32;
    let mut rem = data;
    let mut i = 0;
    while i < 12 {
        rem = (rem << 1) ^ ((rem >> 11) * 0x1F25);
        i += 1;
    }
    data << 12 | rem
}

/// Тёмный ли модуль маски `mask` (таблица 10).
const fn masked(mask: u8, x: usize, y: usize) -> bool {
    match mask {
        0 => (x + y).is_multiple_of(2),
        1 => y.is_multiple_of(2),
        2 => x.is_multiple_of(3),
        3 => (x + y).is_multiple_of(3),
        4 => (x / 3 + y / 2).is_multiple_of(2),
        5 => x * y % 2 + x * y % 3 == 0,
        6 => (x * y % 2 + x * y % 3).is_multiple_of(2),
        _ => ((x + y) % 2 + x * y % 3).is_multiple_of(2),
    }
}

/// Квадрат модулей с отметкой служебных: узоры поиска, синхронизации,
/// выравнивания, слова формата и версии. Служебные не несут данных, и маска
/// их не трогает.
#[derive(Clone)]
struct Canvas {
    side: usize,
    dark: Vec<bool>,
    function: Vec<bool>,
}

impl Canvas {
    /// Служебные модули версии; слово формата — пока пустое.
    fn new(version: u8) -> Self {
        let side = side(version);
        let mut canvas = Self {
            side,
            dark: vec![false; side * side],
            function: vec![false; side * side],
        };
        for i in 0..side {
            canvas.set(6, i, i % 2 == 0);
            canvas.set(i, 6, i % 2 == 0);
        }
        canvas.finder(3, 3);
        canvas.finder(side - 4, 3);
        canvas.finder(3, side - 4);
        let centers = alignment(version);
        let last = centers.len().saturating_sub(1);
        for (i, &x) in centers.iter().enumerate() {
            for (j, &y) in centers.iter().enumerate() {
                if !((i == 0 && j == 0) || (i == 0 && j == last) || (i == last && j == 0)) {
                    canvas.align(x, y);
                }
            }
        }
        canvas.format(0);
        if version >= 7 {
            let word = version_word(version);
            for i in 0..18 {
                let bit = word >> i & 1 != 0;
                let a = side - 11 + i % 3;
                let b = i / 3;
                canvas.set(a, b, bit);
                canvas.set(b, a, bit);
            }
        }
        canvas
    }

    fn set(&mut self, x: usize, y: usize, dark: bool) {
        self.dark[y * self.side + x] = dark;
        self.function[y * self.side + x] = true;
    }

    fn finder(&mut self, cx: usize, cy: usize) {
        for dy in -4i32..=4 {
            for dx in -4i32..=4 {
                let (x, y) = (cx as i32 + dx, cy as i32 + dy);
                if (0..self.side as i32).contains(&x) && (0..self.side as i32).contains(&y) {
                    let distance = dx.abs().max(dy.abs());
                    self.set(x as usize, y as usize, distance != 2 && distance != 4);
                }
            }
        }
    }

    fn align(&mut self, cx: usize, cy: usize) {
        for dy in -2i32..=2 {
            for dx in -2i32..=2 {
                let (x, y) = ((cx as i32 + dx) as usize, (cy as i32 + dy) as usize);
                self.set(x, y, dx.abs().max(dy.abs()) != 1);
            }
        }
    }

    /// Обе копии слова формата `word` и всегда тёмный модуль (п. 7.9.1).
    fn format(&mut self, word: u32) {
        let bit = |i: usize| word >> i & 1 != 0;
        let side = self.side;
        for i in 0..6 {
            self.set(8, i, bit(i));
        }
        self.set(8, 7, bit(6));
        self.set(8, 8, bit(7));
        self.set(7, 8, bit(8));
        for i in 9..15 {
            self.set(14 - i, 8, bit(i));
        }
        for i in 0..8 {
            self.set(side - 1 - i, 8, bit(i));
        }
        for i in 8..15 {
            self.set(8, side - 15 + i, bit(i));
        }
        self.set(8, side - 8, true);
    }

    /// Модули данных — в порядке, в котором их пишет и читает код (п. 7.7.3):
    /// столбцами по два справа налево, змейкой вверх и вниз, мимо служебных
    /// и мимо столбца синхронизации.
    fn data_modules(&self) -> Vec<(usize, usize)> {
        let side = self.side;
        let mut order = Vec::with_capacity(side * side);
        let mut right = side as i32 - 1;
        while right >= 1 {
            if right == 6 {
                right = 5;
            }
            for vert in 0..side {
                for j in 0..2 {
                    let x = right as usize - j;
                    let upward = (right + 1) & 2 == 0;
                    let y = if upward { side - 1 - vert } else { vert };
                    if !self.function[y * side + x] {
                        order.push((x, y));
                    }
                }
            }
            right -= 2;
        }
        order
    }
}

/// QR-код: квадрат модулей, тёмный — `true`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Code {
    version: u8,
    side: usize,
    modules: Vec<bool>,
}

impl Code {
    /// Версия кода.
    pub const fn version(&self) -> u8 {
        self.version
    }

    /// Сторона в модулях.
    pub const fn side(&self) -> usize {
        self.side
    }

    /// Тёмный ли модуль; за краем — светлый.
    pub fn dark(&self, x: usize, y: usize) -> bool {
        x < self.side && y < self.side && self.modules[y * self.side + x]
    }
}

/// Байты кода, прочитанные прямо с его модулей, — тестам соседних
/// модулей: снимок камеры проверяют тесты сканера.
#[cfg(test)]
pub(crate) fn read_code(code: &Code) -> Option<Vec<u8>> {
    read::read(&|x, y| code.dark(x, y), code.side()).ok()
}

/// Код байтов `data` версии `version` уровня `ecc`; `None` — не помещаются
/// или такой версии нет. Маска — та, у которой меньше штраф (п. 7.8.3):
/// меньше длинных полос и квадратов одного цвета, ложных узоров поиска
/// и перекоса тёмного и светлого, — так код легче читается камерой.
pub fn encode(data: &[u8], version: u8, ecc: Ecc) -> Option<Code> {
    encode_masked(data, version, ecc, None)
}

/// [`encode`] с заданной маской — тестам: сверка с `qrcodegen` модуль
/// в модуль.
fn encode_masked(data: &[u8], version: u8, ecc: Ecc, mask: Option<u8>) -> Option<Code> {
    if !(MIN_VERSION..=MAX_VERSION).contains(&version) || data.len() > capacity(version, ecc) {
        return None;
    }
    let blocks = Blocks::of(version, ecc);
    let capacity_bits = blocks.data() * 8;
    let mut bits = Bits::default();
    bits.push(0b0100, 4);
    bits.push(data.len() as u32, count_bits(version));
    for &byte in data {
        bits.push(u32::from(byte), 8);
    }
    let terminator = (capacity_bits - bits.len).min(4);
    bits.push(0, terminator);
    let pad = (8 - bits.len % 8) % 8;
    bits.push(0, pad);
    let mut codewords = bits.bytes;
    for pad in [0xEC, 0x11].into_iter().cycle() {
        if codewords.len() >= blocks.data() {
            break;
        }
        codewords.push(pad);
    }
    let all = interleave(&codewords, &blocks);
    let mut canvas = Canvas::new(version);
    let order = canvas.data_modules();
    for (i, &(x, y)) in order.iter().enumerate() {
        let byte = all.get(i / 8).copied().unwrap_or(0);
        canvas.dark[y * canvas.side + x] = byte >> (7 - i % 8) & 1 != 0;
    }
    let apply = |canvas: &Canvas, mask: u8| {
        let mut done = canvas.clone();
        for &(x, y) in &order {
            if masked(mask, x, y) {
                done.dark[y * done.side + x] ^= true;
            }
        }
        done.format(format_word(ecc, mask));
        done
    };
    let best = match mask {
        Some(mask) if mask < 8 => apply(&canvas, mask),
        Some(_) => return None,
        None => (0..8u8)
            .map(|mask| apply(&canvas, mask))
            .min_by_key(|done| penalty(&done.dark, done.side))?,
    };
    Some(Code {
        version,
        side: best.side,
        modules: best.dark,
    })
}

/// Байты данных по блокам с байтами проверки — вперемешку, как их кладёт
/// код (п. 7.6): первые байты всех блоков, вторые… — у коротких блоков
/// последнего байта данных нет.
fn interleave(data: &[u8], blocks: &Blocks) -> Vec<u8> {
    let divisor = gf::divisor(blocks.ecc);
    let short_len = blocks.short_len();
    let mut split = Vec::with_capacity(blocks.count);
    let mut taken = 0;
    for i in 0..blocks.count {
        let len = short_len - blocks.ecc + usize::from(i >= blocks.short());
        let part = &data[taken..taken + len];
        taken += len;
        let mut block = part.to_vec();
        let check = gf::remainder(part, &divisor);
        if i < blocks.short() {
            block.push(0);
        }
        block.extend(check);
        split.push(block);
    }
    let mut result = Vec::with_capacity(blocks.total);
    for i in 0..=short_len {
        for (j, block) in split.iter().enumerate() {
            if i != short_len - blocks.ecc || j >= blocks.short() {
                result.push(block[i]);
            }
        }
    }
    result
}

/// Биты подряд — от старшего.
#[derive(Default)]
struct Bits {
    bytes: Vec<u8>,
    len: usize,
}

impl Bits {
    fn push(&mut self, value: u32, count: usize) {
        for i in (0..count).rev() {
            if self.len.is_multiple_of(8) {
                self.bytes.push(0);
            }
            if value >> i & 1 != 0 {
                let last = self.bytes.len() - 1;
                self.bytes[last] |= 1 << (7 - self.len % 8);
            }
            self.len += 1;
        }
    }
}

/// Штраф маски (п. 7.8.3.1): полосы одного цвета от пяти модулей, квадраты
/// 2×2, ложные узоры поиска 1:1:3:1:1 у светлых четырёх и перекос тёмных.
fn penalty(dark: &[bool], side: usize) -> usize {
    let at = |x: usize, y: usize| dark[y * side + x];
    let mut score = 0;
    for line in 0..side {
        for horizontal in [true, false] {
            let get = |i: usize| if horizontal { at(i, line) } else { at(line, i) };
            let mut run = 1;
            for i in 1..side {
                if get(i) == get(i - 1) {
                    run += 1;
                    if run == 5 {
                        score += 3;
                    } else if run > 5 {
                        score += 1;
                    }
                } else {
                    run = 1;
                }
            }
            // 1011101 с четырьмя светлыми с одной из сторон.
            for i in 0..side.saturating_sub(6) {
                let pattern = [true, false, true, true, true, false, true];
                if (0..7).all(|k| get(i + k) == pattern[k]) {
                    let light = |from: i32| (from..from + 4).all(|k| k < 0 || k >= side as i32 || !get(k as usize));
                    if light(i as i32 - 4) || light(i as i32 + 7) {
                        score += 40;
                    }
                }
            }
        }
    }
    for y in 0..side - 1 {
        for x in 0..side - 1 {
            let color = at(x, y);
            if color == at(x + 1, y) && color == at(x, y + 1) && color == at(x + 1, y + 1) {
                score += 3;
            }
        }
    }
    let total = side * side;
    let darks = dark.iter().filter(|&&d| d).count();
    let k = (darks * 20).abs_diff(total * 10).div_ceil(total).saturating_sub(1);
    score + k * 10
}
