//! Файл PDF из страниц документа (ISO 32000-1, PDF 1.7).
//!
//! Свой писатель, а не крейт: страницам нужны текст, линии и картинка,
//! а крейты PDF тащат в wasm разбор шрифтов всех форматов и раскладку
//! текста, которые нам ни к чему (замер — `crates/core/Cargo.toml`, у
//! `miniz_oxide`).
//!
//! **Текст** — шрифтами Type 0 с CID-шрифтом TrueType внутри (`/Identity-H`,
//! `/CIDToGIDMap /Identity`): в строке лежат номера глифов, по два байта
//! на знак. Так пишется любой знак любого письма, а ширины берутся из
//! `/W` — те же, что у раскладки. Таблица `/ToUnicode` возвращает глифам
//! их знаки: текст PDF ищется и копируется. Шрифт — подмножество
//! (`Font::subset`): только глифы документа.
//!
//! **Один и тот же документ — одни и те же байты:** ни даты создания, ни
//! случайного `/ID` в файле нет, числа — целые. Тест сравнивает файлы
//! байт в байт, а человек, скачавший PDF дважды, получает один файл.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use miniz_oxide::deflate::compress_to_vec_zlib;

use super::font::FontError;
use super::image::Picture;
use super::page::{Fonts, Ink, Mark, PAGE_HEIGHT, PAGE_WIDTH, Page, Pt};
use crate::i18n::Lang;

/// Цвет роли — копия переменной светлой темы `tokens.css` (Правило 4):
/// бумага белая, как и у превью в окне. Сверяет их тест окна
/// `crates/app/tests/tokens.rs`.
pub const fn rgb(ink: Ink) -> [u8; 3] {
    match ink {
        // --on-fill-paper
        Ink::Text => [0x20, 0x1E, 0x1D],
        // --on-fill-paper-muted
        Ink::Muted => [0x64, 0x5C, 0x50],
        // --on-fill-paper-accent
        Ink::Accent => [0x8C, 0x49, 0x1A],
        // --decor-paper-rule
        Ink::Rule => [0xDC, 0xD3, 0xC4],
    }
}

/// Уровень сжатия deflate: 6 — обычный у zlib. Документ — десятки
/// килобайт, и сжатие сильнее выиграло бы байты, а не время.
const LEVEL: u8 = 6;

/// Сведения о файле: название — во вкладке и в списке файлов читалки,
/// язык — для экранного диктора.
#[derive(Clone, Copy, Debug)]
pub struct Info<'a> {
    pub title: &'a str,
    pub lang: Lang,
}

/// Число PDF из сотых долей пункта: `595.28`, `12`, `-0.5`.
fn num(value: Pt) -> String {
    let sign = if value < 0 { "-" } else { "" };
    let abs = value.unsigned_abs();
    let (whole, part) = (abs / 100, abs % 100);
    match part {
        0 => format!("{sign}{whole}"),
        p if p % 10 == 0 => format!("{sign}{whole}.{}", p / 10),
        p => format!("{sign}{whole}.{p:02}"),
    }
}

/// Цвет числами PDF: доли единицы с тремя знаками.
fn color(rgb: [u8; 3]) -> String {
    let part = |c: u8| {
        let thousandths = u32::from(c) * 1000 / 255;
        let mut text = format!("{}.{:03}", thousandths / 1000, thousandths % 1000);
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
        text
    };
    format!("{} {} {}", part(rgb[0]), part(rgb[1]), part(rgb[2]))
}

/// Строка PDF в UTF-16BE с меткой порядка байтов — шестнадцатеричной
/// записью: так в ней не надо ничего экранировать.
fn text_string(text: &str) -> String {
    let mut out = String::from("<FEFF");
    for unit in text.encode_utf16() {
        let _ = write!(out, "{unit:04X}");
    }
    out.push('>');
    out
}

/// Файл PDF по объектам: смещение каждого — для таблицы `xref`.
struct Writer {
    out: Vec<u8>,
    /// Смещение объекта по номеру; номер 0 — пустой, как в `xref`.
    offsets: Vec<usize>,
}

impl Writer {
    fn new() -> Self {
        // Заголовок и строка с байтами старше 127: по ней программы
        // передачи файлов понимают, что файл двоичный (ISO 32000-1, 7.5.2).
        let mut out = b"%PDF-1.7\n%".to_vec();
        out.extend_from_slice(&[0xE2, 0xE3, 0xCF, 0xD3, b'\n']);
        Self {
            out,
            offsets: vec![0],
        }
    }

    /// Номер для объекта, который будет записан позже.
    fn reserve(&mut self) -> usize {
        self.offsets.push(0);
        self.offsets.len() - 1
    }

    fn object(&mut self, id: usize, body: &str) {
        self.offsets[id] = self.out.len();
        self.out
            .extend_from_slice(format!("{id} 0 obj\n{body}\nendobj\n").as_bytes());
    }

    /// Поток: словарь `dict` (без `/Length`) и байты.
    fn stream(&mut self, id: usize, dict: &str, data: &[u8]) {
        self.offsets[id] = self.out.len();
        self.out.extend_from_slice(
            format!("{id} 0 obj\n<< /Length {} {dict} >>\nstream\n", data.len()).as_bytes(),
        );
        self.out.extend_from_slice(data);
        self.out.extend_from_slice(b"\nendstream\nendobj\n");
    }

    /// Сжатый поток.
    fn deflated(&mut self, id: usize, dict: &str, data: &[u8]) {
        let packed = compress_to_vec_zlib(data, LEVEL);
        self.stream(id, &format!("/Filter /FlateDecode {dict}"), &packed);
    }

    /// Таблица `xref` и хвост файла.
    fn finish(mut self, root: usize, info: usize) -> Vec<u8> {
        let start = self.out.len();
        let mut table = format!("xref\n0 {}\n0000000000 65535 f \n", self.offsets.len());
        for offset in &self.offsets[1..] {
            let _ = writeln!(table, "{offset:010} 00000 n ");
        }
        let _ = write!(
            table,
            "trailer\n<< /Size {} /Root {root} 0 R /Info {info} 0 R >>\nstartxref\n{start}\n%%EOF\n",
            self.offsets.len()
        );
        self.out.extend_from_slice(table.as_bytes());
        self.out
    }
}

/// Шрифт в файле: глифы, которые нужны страницам, и их знаки.
#[derive(Default)]
struct Used {
    /// Глиф → знаки, которые он рисует (для `/ToUnicode`).
    glyphs: BTreeMap<u16, char>,
}

/// Метка подмножества перед именем шрифта — шесть заглавных латинских букв
/// (ISO 32000-1, 9.6.4): по номеру шрифта в документе.
fn subset_tag(index: usize) -> String {
    let mut n = index;
    let mut tag = [b'A'; 6];
    for letter in tag.iter_mut().rev() {
        *letter = b'A' + (n % 26) as u8;
        n /= 26;
    }
    String::from_utf8_lossy(&tag).into_owned()
}

/// PDF из страниц. `logo` — картинка для места логотипа на страницах.
pub fn write(
    pages: &[Page],
    fonts: &Fonts<'_>,
    logo: Option<&Picture<'_>>,
    info: &Info<'_>,
) -> Result<Vec<u8>, FontError> {
    let mut w = Writer::new();
    let catalog = w.reserve();
    let tree = w.reserve();
    let meta = w.reserve();

    // Страницы: содержимое и шрифты, которые оно взяло.
    let mut used: BTreeMap<usize, Used> = BTreeMap::new();
    let contents: Vec<String> = pages
        .iter()
        .map(|page| content(page, fonts, &mut used))
        .collect();

    // Номера объектов шрифтов — по номеру файла.
    let font_ids: BTreeMap<usize, usize> = used.keys().map(|&font| (font, w.reserve())).collect();
    let logo_id = logo.map(|_| w.reserve());
    let mut resources = String::from("<< /Font <<");
    for (font, id) in &font_ids {
        let _ = write!(resources, " /F{font} {id} 0 R");
    }
    resources.push_str(" >>");
    if let Some(id) = logo_id {
        let _ = write!(resources, " /XObject << /Logo {id} 0 R >>");
    }
    resources.push_str(" >>");

    let mut kids = Vec::new();
    for content in &contents {
        let page_id = w.reserve();
        let content_id = w.reserve();
        w.deflated(content_id, "", content.as_bytes());
        w.object(
            page_id,
            &format!(
                "<< /Type /Page /Parent {tree} 0 R /MediaBox [0 0 {} {}] /Resources {resources} /Contents {content_id} 0 R >>",
                num(PAGE_WIDTH),
                num(PAGE_HEIGHT)
            ),
        );
        kids.push(format!("{page_id} 0 R"));
    }
    w.object(
        tree,
        &format!(
            "<< /Type /Pages /Kids [{}] /Count {} >>",
            kids.join(" "),
            kids.len()
        ),
    );

    for (&font, &id) in &font_ids {
        embed_font(&mut w, id, font, fonts, &used[&font])?;
    }
    if let (Some(id), Some(picture)) = (logo_id, logo) {
        embed_picture(&mut w, id, picture);
    }

    w.object(
        meta,
        &format!(
            "<< /Title {} /Producer {} >>",
            text_string(info.title),
            text_string("Kvartik")
        ),
    );
    w.object(
        catalog,
        &format!(
            "<< /Type /Catalog /Pages {tree} 0 R /Lang ({}) /ViewerPreferences << /DisplayDocTitle true >> >>",
            info.lang.code()
        ),
    );
    Ok(w.finish(catalog, meta))
}

/// Поток содержимого страницы. Глифы, которые он берёт, записываются
/// в `used`.
fn content(page: &Page, fonts: &Fonts<'_>, used: &mut BTreeMap<usize, Used>) -> String {
    let mut out = String::new();
    let y = |top: Pt| num(PAGE_HEIGHT - top);
    for mark in &page.marks {
        match mark {
            Mark::Rule(rule) => {
                let _ = writeln!(
                    out,
                    "{} RG {} w {} {} m {} {} l S",
                    color(rgb(Ink::Rule)),
                    num(rule.width),
                    num(rule.x1),
                    y(rule.y1),
                    num(rule.x2),
                    y(rule.y2)
                );
            }
            Mark::Logo(frame) => {
                let _ = writeln!(
                    out,
                    "q {} 0 0 {} {} {} cm /Logo Do Q",
                    num(frame.width),
                    num(frame.height),
                    num(frame.x),
                    y(frame.y + frame.height)
                );
            }
            Mark::Text(text) => {
                let _ = write!(
                    out,
                    "BT {} rg {} {} Td",
                    color(rgb(text.ink)),
                    num(text.x),
                    y(text.baseline)
                );
                // Куски одного шрифта подряд: шрифт меняется только там, где
                // знак лежит в другом файле. Позиция после куска сдвигается
                // сама — на ширины глифов из `/W`.
                let mut current = None;
                let mut run = String::new();
                for c in text.text.chars() {
                    let Some(glyph) = fonts.glyph(text.face, c) else {
                        continue;
                    };
                    used.entry(glyph.font)
                        .or_default()
                        .glyphs
                        .entry(glyph.id)
                        .or_insert(c);
                    if current != Some(glyph.font) {
                        if !run.is_empty() {
                            let _ = write!(out, " <{run}> Tj");
                            run.clear();
                        }
                        let _ = write!(out, " /F{} {} Tf", glyph.font, num(text.size));
                        current = Some(glyph.font);
                    }
                    let _ = write!(run, "{:04X}", glyph.id);
                }
                if !run.is_empty() {
                    let _ = write!(out, " <{run}> Tj");
                }
                out.push_str(" ET\n");
            }
        }
    }
    out
}

/// Шрифт Type 0: сам шрифт, CID-шрифт внутри, его описание, подмножество
/// файла и таблица знаков (ISO 32000-1, 9.7).
fn embed_font(
    w: &mut Writer,
    id: usize,
    index: usize,
    fonts: &Fonts<'_>,
    used: &Used,
) -> Result<(), FontError> {
    let (file, font) = &fonts.loaded()[index];
    let name = format!("{}+{}", subset_tag(index), file.postscript);
    let scale = |v: i32| v * 1000 / i32::from(font.units_per_em());
    let cid = w.reserve();
    let descriptor = w.reserve();
    let program = w.reserve();
    let unicode = w.reserve();

    let mut widths = String::new();
    for &glyph in used.glyphs.keys() {
        let _ = write!(
            widths,
            "{glyph} [{}] ",
            scale(i32::from(font.advance(glyph)))
        );
    }
    let glyphs = used.glyphs.keys().copied().collect();
    w.deflated(program, "", &font.subset(&glyphs)?);
    w.deflated(unicode, "", to_unicode(&used.glyphs).as_bytes());
    let [x0, y0, x1, y1] = font.bbox().map(i32::from);
    w.object(
        descriptor,
        &format!(
            "<< /Type /FontDescriptor /FontName /{name} /Flags 32 /FontBBox [{} {} {} {}] /ItalicAngle 0 /Ascent {} /Descent {} /CapHeight {} /StemV 80 /FontFile2 {program} 0 R >>",
            scale(x0),
            scale(y0),
            scale(x1),
            scale(y1),
            scale(i32::from(font.ascender())),
            scale(i32::from(font.descender())),
            scale(i32::from(font.cap_height()))
        ),
    );
    w.object(
        cid,
        &format!(
            "<< /Type /Font /Subtype /CIDFontType2 /BaseFont /{name} /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> /FontDescriptor {descriptor} 0 R /W [{}] /CIDToGIDMap /Identity >>",
            widths.trim_end()
        ),
    );
    w.object(
        id,
        &format!(
            "<< /Type /Font /Subtype /Type0 /BaseFont /{name} /Encoding /Identity-H /DescendantFonts [{cid} 0 R] /ToUnicode {unicode} 0 R >>"
        ),
    );
    Ok(())
}

/// Таблица «глиф → знак» (`/ToUnicode`, ISO 32000-1, 9.10.3): по ней
/// читалка находит и копирует текст.
fn to_unicode(glyphs: &BTreeMap<u16, char>) -> String {
    let mut out = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
         /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
         /CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n\
         1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
    );
    let pairs: Vec<(&u16, &char)> = glyphs.iter().collect();
    // Не больше ста записей в блоке (ISO 32000-1, 9.10.3, примечание).
    for chunk in pairs.chunks(100) {
        let _ = writeln!(out, "{} beginbfchar", chunk.len());
        for (glyph, c) in chunk {
            let mut units = [0u16; 2];
            let code: String = c
                .encode_utf16(&mut units)
                .iter()
                .map(|unit| format!("{unit:04X}"))
                .collect();
            let _ = writeln!(out, "<{glyph:04X}> <{code}>");
        }
        out.push_str("endbfchar\n");
    }
    out.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    out
}

/// Картинка логотипа (ISO 32000-1, 8.9.5): JPEG — как есть, PNG — цвет
/// и маска прозрачности, оба сжатые.
fn embed_picture(w: &mut Writer, id: usize, picture: &Picture<'_>) {
    match picture {
        Picture::Jpeg {
            bytes,
            width,
            height,
            components,
            inverted,
        } => {
            let space = match components {
                1 => "/DeviceGray",
                4 => "/DeviceCMYK",
                _ => "/DeviceRGB",
            };
            let decode = if *inverted {
                " /Decode [1 0 1 0 1 0 1 0]"
            } else {
                ""
            };
            w.stream(
                id,
                &format!(
                    "/Type /XObject /Subtype /Image /Width {width} /Height {height} /ColorSpace {space} /BitsPerComponent 8 /Filter /DCTDecode{decode}"
                ),
                bytes,
            );
        }
        Picture::Raster {
            width,
            height,
            gray,
            color,
            alpha,
        } => {
            let space = if *gray { "/DeviceGray" } else { "/DeviceRGB" };
            let mask = alpha.as_ref().map(|alpha| {
                let mask = w.reserve();
                w.deflated(
                    mask,
                    &format!(
                        "/Type /XObject /Subtype /Image /Width {width} /Height {height} /ColorSpace /DeviceGray /BitsPerComponent 8"
                    ),
                    alpha,
                );
                format!(" /SMask {mask} 0 R")
            });
            w.deflated(
                id,
                &format!(
                    "/Type /XObject /Subtype /Image /Width {width} /Height {height} /ColorSpace {space} /BitsPerComponent 8{}",
                    mask.unwrap_or_default()
                ),
                color,
            );
        }
    }
}

#[cfg(test)]
mod tests;
