//! Книга Excel (XLSX, SpreadsheetML, ECMA-376 часть 1) — своим кодом, без
//! крейта: у `rust_xlsxwriter` и похожих свой ZIP со сжатием и модель
//! всей книги Excel, а выгрузке нужны пять частей пакета и листы из строк
//! (CLAUDE.md, «Общее»: зависимость окна оплачивает каждый пользователь).
//!
//! Что внутри — минимум, который открывают Excel, LibreOffice и Google
//! Таблицы: `[Content_Types].xml`, связи пакета и книги, `xl/workbook.xml`,
//! `xl/styles.xml` и по листу на каждую таблицу. Строки — прямо в ячейках
//! (`t="inlineStr"`, ECMA-376 ч. 1, §18.3.1.4 и §18.18.11), без общей
//! таблицы строк: она нужна для повторов, а повторов у выгрузки мало.
//!
//! Числа — числами, даты — датами: Excel показывает их по настройкам
//! Windows человека, и разделители не нужны вовсе. Дата — номер дня
//! от 1899-12-30, как его считает Excel для дат с марта 1900 года (система
//! дат 1900, ECMA-376 ч. 1, §18.17.4.1), а показывает её встроенный формат 14
//! — короткая дата по настройкам Windows (§18.8.30).
//!
//! Текст, похожий на формулу (`=`, `+`, `-`, `@`, табуляция, возврат
//! каретки в начале), — строкой со стилем `quotePrefix` (§18.8.45): Excel
//! и при правке ячейки оставит его текстом, а не формулой (Правило 13).

use std::fmt::Write as _;

use super::zip::{self, Entry};
use super::{Cell, Sheet, formula_like};
use crate::model::date::Date;

/// Стили ячеек — места в `cellXfs` у [`STYLES`].
const PLAIN: u8 = 0;
const DATE: u8 = 1;
const WHOLE: u8 = 2;
const CENTS: u8 = 3;
const RATE: u8 = 4;
const HEADER: u8 = 5;
const QUOTED: u8 = 6;

/// Стили книги. Формат 3 — «#,##0», 4 — «#,##0.00», 14 — короткая дата
/// (встроенные, §18.8.30); 164 — свой, четыре знака курса. Шрифт — Calibri,
/// как у новой книги Excel: знаков армянского в нём нет, и Excel берёт их
/// у шрифта Windows сам.
const STYLES: &str = concat!(
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
    "\n",
    r#"<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">"#,
    // `##` — потому что в формате есть `"#`, который закрыл бы `r#"…"#`.
    r##"<numFmts count="1"><numFmt numFmtId="164" formatCode="#,##0.0000"/></numFmts>"##,
    r#"<fonts count="2"><font><sz val="11"/><name val="Calibri"/></font>"#,
    r#"<font><b/><sz val="11"/><name val="Calibri"/></font></fonts>"#,
    r#"<fills count="2"><fill><patternFill patternType="none"/></fill>"#,
    r#"<fill><patternFill patternType="gray125"/></fill></fills>"#,
    r#"<borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders>"#,
    r#"<cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs>"#,
    r#"<cellXfs count="7">"#,
    r#"<xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/>"#,
    r#"<xf numFmtId="14" fontId="0" fillId="0" borderId="0" xfId="0" applyNumberFormat="1"/>"#,
    r#"<xf numFmtId="3" fontId="0" fillId="0" borderId="0" xfId="0" applyNumberFormat="1"/>"#,
    r#"<xf numFmtId="4" fontId="0" fillId="0" borderId="0" xfId="0" applyNumberFormat="1"/>"#,
    r#"<xf numFmtId="164" fontId="0" fillId="0" borderId="0" xfId="0" applyNumberFormat="1"/>"#,
    r#"<xf numFmtId="0" fontId="1" fillId="0" borderId="0" xfId="0" applyFont="1"/>"#,
    r#"<xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0" quotePrefix="1"/>"#,
    r#"</cellXfs>"#,
    r#"<cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles>"#,
    r#"</styleSheet>"#,
);

const RELS: &str = concat!(
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
    "\n",
    r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#,
    r#"<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/>"#,
    r#"</Relationships>"#,
);

/// Книга Excel из листов `sheets` — байты файла `.xlsx`.
pub fn xlsx(sheets: &[Sheet]) -> Vec<u8> {
    let count = sheets.len();
    let mut types = String::from(concat!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
        "\n",
        r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">"#,
        r#"<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>"#,
        r#"<Default Extension="xml" ContentType="application/xml"/>"#,
        r#"<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>"#,
        r#"<Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/>"#,
    ));
    let mut workbook = String::from(concat!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
        "\n",
        r#"<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets>"#,
    ));
    let mut workbook_rels = String::from(concat!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
        "\n",
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#,
    ));
    for (at, sheet) in sheets.iter().enumerate() {
        let n = at + 1;
        let _ = write!(
            types,
            r#"<Override PartName="/xl/worksheets/sheet{n}.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>"#
        );
        let _ = write!(
            workbook,
            r#"<sheet name="{}" sheetId="{n}" r:id="rId{n}"/>"#,
            escape(&sheet_name(&sheet.name))
        );
        let _ = write!(
            workbook_rels,
            r#"<Relationship Id="rId{n}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet{n}.xml"/>"#
        );
    }
    types.push_str("</Types>");
    workbook.push_str("</sheets></workbook>");
    let _ = write!(
        workbook_rels,
        r#"<Relationship Id="rId{}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#,
        count + 1
    );

    let worksheets: Vec<String> = sheets.iter().map(worksheet).collect();
    let names: Vec<String> = (1..=count).map(|n| format!("xl/worksheets/sheet{n}.xml")).collect();
    let mut entries = vec![
        Entry { name: "[Content_Types].xml", bytes: types.as_bytes() },
        Entry { name: "_rels/.rels", bytes: RELS.as_bytes() },
        Entry { name: "xl/workbook.xml", bytes: workbook.as_bytes() },
        Entry { name: "xl/_rels/workbook.xml.rels", bytes: workbook_rels.as_bytes() },
        Entry { name: "xl/styles.xml", bytes: STYLES.as_bytes() },
    ];
    for (name, sheet) in names.iter().zip(&worksheets) {
        entries.push(Entry { name, bytes: sheet.as_bytes() });
    }
    zip::stored(&entries)
}

/// Лист: закреплённая строка заголовков, ширина столбцов по их тексту
/// и ячейки по строкам.
fn worksheet(sheet: &Sheet) -> String {
    let mut out = String::from(concat!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
        "\n",
        r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">"#,
        r#"<sheetViews><sheetView workbookViewId="0"><pane ySplit="1" topLeftCell="A2" activePane="bottomLeft" state="frozen"/></sheetView></sheetViews>"#,
    ));
    out.push_str("<cols>");
    for (at, title) in sheet.columns.iter().enumerate() {
        // Заголовок — `max` с числом, а не `chain` с массивом: итератор массива
        // общий у многих частей, и программа разреза положила бы его отдельным
        // куском wasm (CLAUDE.md, Правило 6; замер задачи 30).
        let widest = sheet
            .rows
            .iter()
            .filter_map(|row| row.get(at))
            .map(Cell::width)
            .max()
            .unwrap_or(0)
            .max(title.chars().count());
        // Ширина — в знаках шрифта по умолчанию (§18.3.1.13), с запасом на
        // поля ячейки; слишком длинное Excel переносит сам.
        let width = widest.clamp(8, 60) + 2;
        let _ = write!(out, r#"<col min="{n}" max="{n}" width="{width}" customWidth="1"/>"#, n = at + 1);
    }
    out.push_str("</cols><sheetData>");
    let header: Vec<Cell> = sheet.columns.iter().map(|title| Cell::Text(title.clone())).collect();
    row(&mut out, 1, &header, true);
    for (at, cells) in sheet.rows.iter().enumerate() {
        row(&mut out, at + 2, cells, false);
    }
    out.push_str("</sheetData></worksheet>");
    out
}

/// Строка листа `n` (с единицы).
fn row(out: &mut String, n: usize, cells: &[Cell], header: bool) {
    let _ = write!(out, r#"<row r="{n}">"#);
    for (at, cell) in cells.iter().enumerate() {
        let reference = format!("{}{n}", column(at));
        match cell {
            Cell::Empty => {}
            Cell::Text(text) => {
                let style = if header {
                    HEADER
                } else if formula_like(text) {
                    QUOTED
                } else {
                    PLAIN
                };
                let _ = write!(
                    out,
                    r#"<c r="{reference}"{} t="inlineStr"><is><t xml:space="preserve">{}</t></is></c>"#,
                    style_attribute(style),
                    cell_text(clip(text))
                );
            }
            Cell::Date(day) => {
                let _ = write!(out, r#"<c r="{reference}" s="{DATE}"><v>{}</v></c>"#, serial(*day));
            }
            Cell::Number { minor, places } => {
                let style = match places {
                    0 => WHOLE,
                    1 | 2 => CENTS,
                    _ => RATE,
                };
                let _ = write!(
                    out,
                    r#"<c r="{reference}" s="{style}"><v>{}</v></c>"#,
                    super::decimal(*minor, *places)
                );
            }
        }
    }
    out.push_str("</row>");
}

fn style_attribute(style: u8) -> String {
    if style == PLAIN {
        String::new()
    } else {
        format!(r#" s="{style}""#)
    }
}

/// Буквы столбца: A … Z, AA, AB …
pub(super) fn column(at: usize) -> String {
    let mut n = at + 1;
    let mut letters = Vec::new();
    while n > 0 {
        let rest = (n - 1) % 26;
        letters.push(b'A' + rest as u8);
        n = (n - 1) / 26;
    }
    letters.reverse();
    String::from_utf8(letters).unwrap_or_default()
}

/// Номер дня в системе дат 1900: дни от 1899-12-30 — так Excel считает
/// дни с 1 марта 1900 года (раньше его прибавленный 29 февраля 1900-го
/// сдвигает счёт, но таких дат в книге ИП нет).
pub(super) fn serial(day: Date) -> i32 {
    match Date::new(1899, 12, 30) {
        Some(start) => day.days_since(start),
        None => 0,
    }
}

/// Имя листа по правилам Excel: не длиннее 31 знака, без `[]:*?/\`
/// (§18.2.19); иначе Excel откажется открыть книгу.
pub(super) fn sheet_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if "[]:*?/\\".contains(c) { ' ' } else { c })
        .take(31)
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() { "1".to_owned() } else { trimmed.to_owned() }
}

/// Не длиннее 32 767 знаков — предел ячейки Excel; длинное режется, а не
/// ломает книгу. Знаки Excel считает в UTF-16: знак вне BMP — два (ревью
/// задачи 30), а «за что» дохода из документа — до ста строк по триста знаков.
pub(super) fn clip(text: &str) -> &str {
    let mut units = 0;
    for (at, c) in text.char_indices() {
        units += c.len_utf16();
        if units > 32_767 {
            return &text[..at];
        }
    }
    text
}

/// Текст для XML: служебные знаки — сущностями, а знаки, которых XML 1.0
/// не допускает (управляющие, кроме табуляции и перевода строки), —
/// заменой: с ними Excel книгу не откроет вовсе.
pub(super) fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        push_escaped(&mut out, c);
    }
    out
}

/// Текст ячейки для XML: как [`escape`], и ещё запись `_xHHHH_` — Excel
/// читает её в тексте ячейки знаком U+HHHH (ECMA-376 ч. 1, §22.4.2.4,
/// ST_Xstring): «_x003D_1+1» показал бы `=1+1` без `quotePrefix`, а
/// «_x0041_» — букву A. Поэтому её `_` пишется записью `_x005F_`, как у
/// XlsxWriter (ревью задачи 30).
pub(super) fn cell_text(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    for (at, c) in text.char_indices() {
        if c == '_' && excel_code(&bytes[at..]) {
            out.push_str("_x005F");
        }
        push_escaped(&mut out, c);
    }
    out
}

/// Начинается ли `rest` с записи `_xHHHH_`.
fn excel_code(rest: &[u8]) -> bool {
    rest.len() >= 7 && rest[1] == b'x' && rest[2..6].iter().all(u8::is_ascii_hexdigit) && rest[6] == b'_'
}

fn push_escaped(out: &mut String, c: char) {
    match c {
        '&' => out.push_str("&amp;"),
        '<' => out.push_str("&lt;"),
        '>' => out.push_str("&gt;"),
        '"' => out.push_str("&quot;"),
        '\t' | '\n' | '\r' => out.push(c),
        c if (c as u32) < 0x20 || c == '\u{FFFE}' || c == '\u{FFFF}' => out.push('\u{FFFD}'),
        c => out.push(c),
    }
}
