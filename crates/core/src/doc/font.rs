//! Шрифт TrueType для документа: какие знаки он рисует, их ширины для
//! раскладки и подмножество для PDF.
//!
//! Файлы — те же, что у окна (`crates/app/public/fonts/`, их пишет
//! `tools/fonts.py`): документ и окно рисуют одними байтами (Правило 4),
//! а человек не скачивает шрифт дважды. Какие файлы есть и какие знаки
//! в каком — [`super::fonts`].
//!
//! Читается только нужное: каталог таблиц, `head` (единиц на кегль, рамка
//! глифов, вид `loca`), `hhea` и `hmtx` (ширины), `maxp` (число глифов),
//! `cmap` (знак → глиф, подтаблицы формата 4 и 12), `OS/2` (высота
//! прописных), `loca` и `glyf` (подмножество). Разбор не паникует: файл,
//! который не читается, — `Err`, а не обрыв программы (Правило 13).

use std::collections::{BTreeMap, BTreeSet};

/// Почему файл шрифта не прочитан.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum FontError {
    /// Не TrueType: нет нужной таблицы, таблица выходит за файл или числа
    /// в ней невозможные.
    Malformed,
}

/// Таблица в файле: где начинается и сколько байтов.
#[derive(Clone, Copy, Debug)]
struct Table {
    tag: [u8; 4],
    at: usize,
    len: usize,
}

/// Прочитанный шрифт. Байты — чужие: файл, который окно взяло у сайта.
#[derive(Clone, Debug)]
pub struct Font<'a> {
    bytes: &'a [u8],
    tables: Vec<Table>,
    units_per_em: u16,
    bbox: [i16; 4],
    long_loca: bool,
    glyphs: u16,
    metrics: u16,
    hmtx: Table,
    ascender: i16,
    descender: i16,
    cap_height: i16,
    cmap: BTreeMap<u32, u16>,
}

/// Число из байтов `bytes[at..]` — или `Malformed`, если байтов не хватает.
fn u16_at(bytes: &[u8], at: usize) -> Result<u16, FontError> {
    match bytes.get(at..at.checked_add(2).ok_or(FontError::Malformed)?) {
        Some(&[a, b]) => Ok(u16::from_be_bytes([a, b])),
        _ => Err(FontError::Malformed),
    }
}

fn i16_at(bytes: &[u8], at: usize) -> Result<i16, FontError> {
    u16_at(bytes, at).map(|n| n as i16)
}

fn u32_at(bytes: &[u8], at: usize) -> Result<u32, FontError> {
    match bytes.get(at..at.checked_add(4).ok_or(FontError::Malformed)?) {
        Some(&[a, b, c, d]) => Ok(u32::from_be_bytes([a, b, c, d])),
        _ => Err(FontError::Malformed),
    }
}

/// `u32` как индекс: на wasm32 `usize` — те же 32 бита.
fn index(n: u32) -> usize {
    n as usize
}

impl<'a> Font<'a> {
    /// Шрифт из байтов файла TrueType.
    pub fn parse(bytes: &'a [u8]) -> Result<Self, FontError> {
        // Каталог (OpenType, «Table Directory»): версия 0x00010000 у глифов
        // TrueType, число таблиц, записи по 16 байт с 12-го.
        if u32_at(bytes, 0)? != 0x0001_0000 {
            return Err(FontError::Malformed);
        }
        let count = usize::from(u16_at(bytes, 4)?);
        let mut tables = Vec::with_capacity(count);
        for i in 0..count {
            let record = 12 + 16 * i;
            let tag = bytes
                .get(record..record + 4)
                .and_then(|tag| <[u8; 4]>::try_from(tag).ok())
                .ok_or(FontError::Malformed)?;
            let at = index(u32_at(bytes, record + 8)?);
            let len = index(u32_at(bytes, record + 12)?);
            if at.checked_add(len).is_none_or(|end| end > bytes.len()) {
                return Err(FontError::Malformed);
            }
            tables.push(Table { tag, at, len });
        }
        let find = |tag: &[u8; 4]| {
            tables
                .iter()
                .find(|table| &table.tag == tag)
                .copied()
                .ok_or(FontError::Malformed)
        };
        let head = find(b"head")?;
        let hhea = find(b"hhea")?;
        let maxp = find(b"maxp")?;
        let hmtx = find(b"hmtx")?;
        let cmap = find(b"cmap")?;
        find(b"loca")?;
        find(b"glyf")?;

        let units_per_em = u16_at(bytes, head.at + 18)?;
        let bbox = [
            i16_at(bytes, head.at + 36)?,
            i16_at(bytes, head.at + 38)?,
            i16_at(bytes, head.at + 40)?,
            i16_at(bytes, head.at + 42)?,
        ];
        let long_loca = match i16_at(bytes, head.at + 50)? {
            0 => false,
            1 => true,
            _ => return Err(FontError::Malformed),
        };
        let glyphs = u16_at(bytes, maxp.at + 4)?;
        let metrics = u16_at(bytes, hhea.at + 34)?;
        if units_per_em == 0 || glyphs == 0 || metrics == 0 || metrics > glyphs {
            return Err(FontError::Malformed);
        }
        if hmtx.len < 4 * usize::from(metrics) + 2 * usize::from(glyphs - metrics) {
            return Err(FontError::Malformed);
        }
        let ascender = i16_at(bytes, hhea.at + 4)?;
        let descender = i16_at(bytes, hhea.at + 6)?;
        // Высота прописных — в `OS/2` с версии 2 (смещение 88); нет её —
        // высота подъёма: для рамки PDF это лишь подсказка.
        let cap_height = match find(b"OS/2") {
            Ok(os2) if u16_at(bytes, os2.at)? >= 2 => i16_at(bytes, os2.at + 88)?,
            _ => ascender,
        };

        let mut font = Self {
            bytes,
            tables,
            units_per_em,
            bbox,
            long_loca,
            glyphs,
            metrics,
            hmtx,
            ascender,
            descender,
            cap_height,
            cmap: BTreeMap::new(),
        };
        font.cmap = font.read_cmap(cmap)?;
        Ok(font)
    }

    /// Знак → глиф по подтаблицам Юникода: платформа 0 или Windows
    /// с кодировкой 1 (BMP) и 10 (весь Юникод); форматы 4 и 12 — других
    /// в Noto нет. Глифы за пределами шрифта не берутся.
    fn read_cmap(&self, cmap: Table) -> Result<BTreeMap<u32, u16>, FontError> {
        let bytes = self.bytes;
        let mut out = BTreeMap::new();
        for i in 0..usize::from(u16_at(bytes, cmap.at + 2)?) {
            let record = cmap.at + 4 + 8 * i;
            let (platform, encoding) = (u16_at(bytes, record)?, u16_at(bytes, record + 2)?);
            if !(platform == 0 || (platform == 3 && matches!(encoding, 1 | 10))) {
                continue;
            }
            let sub = cmap.at + index(u32_at(bytes, record + 4)?);
            match u16_at(bytes, sub)? {
                4 => self.format_4(sub, &mut out)?,
                12 => self.format_12(sub, &mut out)?,
                _ => {}
            }
        }
        out.retain(|_, glyph| *glyph != 0 && *glyph < self.glyphs);
        Ok(out)
    }

    fn format_4(&self, sub: usize, out: &mut BTreeMap<u32, u16>) -> Result<(), FontError> {
        let bytes = self.bytes;
        let segments = usize::from(u16_at(bytes, sub + 6)? / 2);
        let ends = sub + 14;
        let starts = ends + 2 * segments + 2;
        let deltas = starts + 2 * segments;
        let offsets = deltas + 2 * segments;
        for s in 0..segments {
            let start = u16_at(bytes, starts + 2 * s)?;
            let end = u16_at(bytes, ends + 2 * s)?;
            let delta = u16_at(bytes, deltas + 2 * s)?;
            let offset = usize::from(u16_at(bytes, offsets + 2 * s)?);
            // Последний отрезок — 0xFFFF → глиф 0, знака нет.
            for code in start..=end.min(0xFFFE) {
                let glyph = if offset == 0 {
                    code.wrapping_add(delta)
                } else {
                    let at = offsets + 2 * s + offset + 2 * usize::from(code - start);
                    match u16_at(bytes, at)? {
                        0 => 0,
                        raw => raw.wrapping_add(delta),
                    }
                };
                if glyph != 0 {
                    out.insert(u32::from(code), glyph);
                }
            }
        }
        Ok(())
    }

    fn format_12(&self, sub: usize, out: &mut BTreeMap<u32, u16>) -> Result<(), FontError> {
        let bytes = self.bytes;
        for g in 0..index(u32_at(bytes, sub + 12)?) {
            let group = sub + 16 + 12 * g;
            let (start, end) = (u32_at(bytes, group)?, u32_at(bytes, group + 4)?);
            let first = u32_at(bytes, group + 8)?;
            if end < start || end > 0x10_FFFF {
                return Err(FontError::Malformed);
            }
            for code in start..=end {
                let glyph = first.saturating_add(code - start);
                out.insert(
                    code,
                    u16::try_from(glyph).map_err(|_| FontError::Malformed)?,
                );
            }
        }
        Ok(())
    }

    /// Глиф знака; `None` — знака в шрифте нет.
    pub fn glyph(&self, c: char) -> Option<u16> {
        self.cmap.get(&u32::from(c)).copied()
    }

    /// Ширина глифа в единицах шрифта (`hmtx`): у глифов после последней
    /// записи — ширина последней (`hhea.numberOfHMetrics`).
    pub fn advance(&self, glyph: u16) -> u16 {
        let i = usize::from(glyph.min(self.metrics - 1));
        // Размер `hmtx` проверен при разборе.
        u16_at(self.bytes, self.hmtx.at + 4 * i).unwrap_or(0)
    }

    /// Единиц шрифта на кегль.
    pub const fn units_per_em(&self) -> u16 {
        self.units_per_em
    }

    /// Подъём над базовой линией — в единицах шрифта.
    pub const fn ascender(&self) -> i16 {
        self.ascender
    }

    /// Спуск под базовую линию — отрицательный, в единицах шрифта.
    pub const fn descender(&self) -> i16 {
        self.descender
    }

    /// Высота прописных — в единицах шрифта.
    pub const fn cap_height(&self) -> i16 {
        self.cap_height
    }

    /// Рамка всех глифов: x и y нижнего левого угла, x и y верхнего правого.
    pub const fn bbox(&self) -> [i16; 4] {
        self.bbox
    }

    /// Знаки, у которых есть глиф.
    pub fn chars(&self) -> impl Iterator<Item = u32> + '_ {
        self.cmap.keys().copied()
    }

    fn table(&self, tag: &[u8; 4]) -> Option<&'a [u8]> {
        let table = self.tables.iter().find(|table| &table.tag == tag)?;
        self.bytes.get(table.at..table.at + table.len)
    }

    /// Байты глифа в `glyf`; пустой срез — глиф без контуров (пробел).
    fn glyph_data(&self, glyph: u16) -> Result<&'a [u8], FontError> {
        let loca = self.table(b"loca").ok_or(FontError::Malformed)?;
        let glyf = self.table(b"glyf").ok_or(FontError::Malformed)?;
        let i = usize::from(glyph);
        let (start, end) = if self.long_loca {
            (index(u32_at(loca, 4 * i)?), index(u32_at(loca, 4 * i + 4)?))
        } else {
            (
                2 * usize::from(u16_at(loca, 2 * i)?),
                2 * usize::from(u16_at(loca, 2 * i + 2)?),
            )
        };
        glyf.get(start..end).ok_or(FontError::Malformed)
    }

    /// Глифы, из которых собран составной глиф (`glyf`, «Composite Glyph
    /// Description»): буква с надстрочным знаком часто — ссылка на букву
    /// и знак, и без них она в подмножестве была бы пустой.
    fn components(&self, glyph: u16) -> Result<Vec<u16>, FontError> {
        let data = self.glyph_data(glyph)?;
        if data.len() < 10 || i16_at(data, 0)? >= 0 {
            return Ok(Vec::new());
        }
        const ARGS_ARE_WORDS: u16 = 0x0001;
        const HAVE_SCALE: u16 = 0x0008;
        const MORE_COMPONENTS: u16 = 0x0020;
        const HAVE_XY_SCALE: u16 = 0x0040;
        const HAVE_TWO_BY_TWO: u16 = 0x0080;
        let mut out = Vec::new();
        let mut at = 10;
        loop {
            let flags = u16_at(data, at)?;
            out.push(u16_at(data, at + 2)?);
            at += 4 + if flags & ARGS_ARE_WORDS != 0 { 4 } else { 2 };
            at += if flags & HAVE_SCALE != 0 {
                2
            } else if flags & HAVE_XY_SCALE != 0 {
                4
            } else if flags & HAVE_TWO_BY_TWO != 0 {
                8
            } else {
                0
            };
            if flags & MORE_COMPONENTS == 0 {
                return Ok(out);
            }
        }
    }

    /// Подмножество для PDF: файл TrueType, где контуры есть только
    /// у глифов `used` (и у глифа 0 — «нет знака»), а номера глифов прежние.
    ///
    /// Номера не меняются нарочно: PDF ссылается на глифы номерами
    /// (`/CIDToGIDMap /Identity`), и с прежними номерами ширины, текст
    /// и таблица `ToUnicode` пишутся теми же числами, что раскладка. Глифы,
    /// которых нет в `used`, остаются пустыми записями `loca` — по четыре
    /// байта на глиф.
    ///
    /// Таблиц — сколько нужно TrueType-шрифту внутри PDF (ISO 32000-1, 9.9,
    /// и «TrueType Reference Manual», «Required tables»): `head`, `hhea`,
    /// `maxp`, `hmtx`, `loca`, `glyf`, `cmap`, `post` и, если есть, `cvt `,
    /// `fpgm`, `prep`, `OS/2`. Подстановки и сдвиги (`GSUB`, `GPOS`, `GDEF`)
    /// PDF не читает: раскладка уже сделана. `post` — версии 3, без имён
    /// глифов.
    pub fn subset(&self, used: &BTreeSet<u16>) -> Result<Vec<u8>, FontError> {
        // Глиф 0 и составные части — замыкание по ссылкам.
        let mut keep: BTreeSet<u16> = BTreeSet::from([0]);
        let mut queue: Vec<u16> = used.iter().copied().filter(|&g| g < self.glyphs).collect();
        while let Some(glyph) = queue.pop() {
            if keep.insert(glyph) {
                queue.extend(
                    self.components(glyph)?
                        .into_iter()
                        .filter(|&g| g < self.glyphs && !keep.contains(&g)),
                );
            }
        }

        let mut glyf = Vec::new();
        let mut loca = Vec::with_capacity(4 * (usize::from(self.glyphs) + 1));
        for glyph in 0..self.glyphs {
            loca.extend_from_slice(&offset32(glyf.len())?);
            if keep.contains(&glyph) {
                glyf.extend_from_slice(self.glyph_data(glyph)?);
                // Глиф — с границы четырёх байтов: так `loca` длинного вида
                // не требует, но так советует спецификация.
                glyf.resize(glyf.len().next_multiple_of(4), 0);
            }
        }
        loca.extend_from_slice(&offset32(glyf.len())?);

        let mut head = self.table(b"head").ok_or(FontError::Malformed)?.to_vec();
        if head.len() < 54 {
            return Err(FontError::Malformed);
        }
        // `checkSumAdjustment` — посчитается заново, `indexToLocFormat` —
        // длинный вид `loca`.
        head[8..12].fill(0);
        head[50..52].copy_from_slice(&1u16.to_be_bytes());
        // `post` версии 3.0: только общие поля, имён глифов нет.
        let mut post = self.table(b"post").ok_or(FontError::Malformed)?.to_vec();
        post.truncate(32);
        if post.len() < 32 {
            return Err(FontError::Malformed);
        }
        post[0..4].copy_from_slice(&0x0003_0000u32.to_be_bytes());

        let mut tables: Vec<([u8; 4], Vec<u8>)> = vec![
            (
                *b"cmap",
                self.table(b"cmap").ok_or(FontError::Malformed)?.to_vec(),
            ),
            (*b"glyf", glyf),
            (*b"head", head),
            (
                *b"hhea",
                self.table(b"hhea").ok_or(FontError::Malformed)?.to_vec(),
            ),
            (
                *b"hmtx",
                self.table(b"hmtx").ok_or(FontError::Malformed)?.to_vec(),
            ),
            (*b"loca", loca),
            (
                *b"maxp",
                self.table(b"maxp").ok_or(FontError::Malformed)?.to_vec(),
            ),
            (*b"post", post),
        ];
        for tag in [b"OS/2", b"cvt ", b"fpgm", b"prep"] {
            if let Some(table) = self.table(tag) {
                tables.push((*tag, table.to_vec()));
            }
        }
        tables.sort_by_key(|(tag, _)| *tag);
        write_font(&tables)
    }
}

fn offset32(n: usize) -> Result<[u8; 4], FontError> {
    u32::try_from(n)
        .map(u32::to_be_bytes)
        .map_err(|_| FontError::Malformed)
}

/// Сумма таблицы TrueType — сумма слов по 4 байта, хвост добит нулями.
fn checksum(data: &[u8]) -> u32 {
    data.chunks(4).fold(0u32, |sum, chunk| {
        let mut word = [0u8; 4];
        word[..chunk.len()].copy_from_slice(chunk);
        sum.wrapping_add(u32::from_be_bytes(word))
    })
}

/// Файл TrueType из таблиц, уже упорядоченных по тегу: каталог, таблицы
/// с границы четырёх байтов и `head.checkSumAdjustment`.
fn write_font(tables: &[([u8; 4], Vec<u8>)]) -> Result<Vec<u8>, FontError> {
    let count = u16::try_from(tables.len()).map_err(|_| FontError::Malformed)?;
    // Поля поиска по каталогу: наибольшая степень двойки не больше числа
    // таблиц.
    let power = 1u16 << (15 - count.leading_zeros().min(15));
    let search_range = power * 16;
    let entry_selector = power.trailing_zeros() as u16;
    let range_shift = count * 16 - search_range;
    let mut out = Vec::new();
    out.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    for n in [count, search_range, entry_selector, range_shift] {
        out.extend_from_slice(&n.to_be_bytes());
    }
    let mut at = 12 + 16 * tables.len();
    let mut head_at = None;
    for (tag, data) in tables {
        out.extend_from_slice(tag);
        out.extend_from_slice(&checksum(data).to_be_bytes());
        out.extend_from_slice(&offset32(at)?);
        out.extend_from_slice(&offset32(data.len())?);
        if tag == b"head" {
            head_at = Some(at);
        }
        at += data.len().next_multiple_of(4);
    }
    for (_, data) in tables {
        out.extend_from_slice(data);
        out.resize(out.len().next_multiple_of(4), 0);
    }
    if let Some(head) = head_at {
        let adjustment = 0xB1B0_AFBAu32.wrapping_sub(checksum(&out));
        out[head + 8..head + 12].copy_from_slice(&adjustment.to_be_bytes());
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
