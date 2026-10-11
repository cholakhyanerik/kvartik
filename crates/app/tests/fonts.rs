//! Шрифты окна (Правило 4): каждый знак, который окно может показать, есть
//! в том файле, куда его отправит браузер.
//!
//! Отсутствующий знак не даёт ошибки: браузер молча берёт его из системного
//! шрифта, и буква выглядит чужой (Правило 6). Поэтому знак проверяется
//! дважды: его `unicode-range` в `style/fonts.css` должен вести хоть в один
//! файл каждого начертания, и каждый такой файл должен этот знак содержать
//! (по таблице `cmap`).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

use kvartik_core::i18n::{self, ALL_KEYS, Lang, t};
use kvartik_core::model::date::Date;
use kvartik_core::model::money::{Currency, Money};

fn app_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Правило `@font-face`: вес, файл и диапазоны знаков.
struct Face {
    weight: u16,
    file: String,
    ranges: Vec<(u32, u32)>,
}

impl Face {
    fn covers(&self, c: char) -> bool {
        let c = u32::from(c);
        self.ranges.iter().any(|&(lo, hi)| (lo..=hi).contains(&c))
    }
}

/// Правила `@font-face` из `fonts.css` в порядке объявления.
fn read_faces() -> Vec<Face> {
    let text = fs::read_to_string(app_dir().join("style/fonts.css")).expect("fonts.css читается");
    let mut faces = Vec::new();
    for block in text.split("@font-face").skip(1) {
        let body = block
            .split_once('{')
            .and_then(|(_, rest)| rest.split_once('}'))
            .map(|(body, _)| body)
            .expect("правило @font-face в фигурных скобках");
        let mut properties = BTreeMap::new();
        for declaration in body.split(';') {
            if let Some((name, value)) = declaration.split_once(':') {
                properties.insert(name.trim().to_owned(), value.trim().to_owned());
            }
        }
        let property = |name: &str| {
            properties
                .get(name)
                .unwrap_or_else(|| panic!("в @font-face нет {name}"))
                .as_str()
        };
        let file = property("src")
            .split_once("url(\"fonts/")
            .and_then(|(_, rest)| rest.split_once('"'))
            .map(|(file, _)| file.to_owned())
            .expect("src: url(\"fonts/…\")");
        let ranges = property("unicode-range")
            .split(',')
            .map(|range| {
                let range = range
                    .trim()
                    .strip_prefix("U+")
                    .expect("диапазон начинается с U+");
                let (lo, hi) = range.split_once('-').unwrap_or((range, range));
                let parse = |hex: &str| u32::from_str_radix(hex, 16).expect("диапазон в hex");
                (parse(lo), parse(hi))
            })
            .collect();
        faces.push(Face {
            weight: property("font-weight").parse().expect("вес — число"),
            file,
            ranges,
        });
    }
    faces
}

/// Начертания, которые объявляет `tokens.css`: `--weight-…: 400;`.
fn token_weights() -> BTreeSet<u16> {
    let text = fs::read_to_string(app_dir().join("style/tokens.css")).expect("tokens.css читается");
    text.lines()
        .filter_map(|line| line.trim().strip_prefix("--weight-"))
        .map(|rest| {
            let (_, value) = rest.split_once(':').expect("--weight-…: число;");
            value
                .trim()
                .trim_end_matches(';')
                .parse()
                .expect("вес — число")
        })
        .collect()
}

/// Файл шрифта: что в нём есть.
struct Font {
    bytes: Vec<u8>,
}

impl Font {
    fn open(file: &str) -> Self {
        let path = app_dir().join("public/fonts").join(file);
        let bytes = fs::read(&path).unwrap_or_else(|_| panic!("{} не читается", path.display()));
        Self { bytes }
    }

    fn u16_at(&self, at: usize) -> usize {
        usize::from(u16::from_be_bytes([self.bytes[at], self.bytes[at + 1]]))
    }

    fn u32_at(&self, at: usize) -> usize {
        let b = &self.bytes[at..at + 4];
        u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize
    }

    /// Начало таблицы по тегу (каталог таблиц OpenType: число таблиц на 4,
    /// записи по 16 байт с 12-го).
    fn table(&self, tag: &[u8; 4]) -> Option<usize> {
        (0..self.u16_at(4))
            .map(|i| 12 + 16 * i)
            .find(|&record| &self.bytes[record..record + 4] == tag)
            .map(|record| self.u32_at(record + 8))
    }

    /// Знаки, у которых есть глиф: подтаблица `cmap` формата 4 (BMP) или 12.
    fn chars(&self) -> BTreeSet<u32> {
        let cmap = self.table(b"cmap").expect("в шрифте есть cmap");
        let mut out = BTreeSet::new();
        for i in 0..self.u16_at(cmap + 2) {
            let record = cmap + 4 + 8 * i;
            let (platform, encoding) = (self.u16_at(record), self.u16_at(record + 2));
            // Юникод: платформа 0 или Windows с кодировкой 1 (BMP) и 10 (весь).
            if !(platform == 0 || (platform == 3 && matches!(encoding, 1 | 10))) {
                continue;
            }
            let sub = cmap + self.u32_at(record + 4);
            match self.u16_at(sub) {
                4 => self.format_4(sub, &mut out),
                12 => self.format_12(sub, &mut out),
                _ => {}
            }
        }
        out
    }

    fn format_4(&self, sub: usize, out: &mut BTreeSet<u32>) {
        let segments = self.u16_at(sub + 6) / 2;
        let ends = sub + 14;
        let starts = ends + 2 * segments + 2;
        let deltas = starts + 2 * segments;
        let offsets = deltas + 2 * segments;
        for s in 0..segments {
            let (start, end) = (self.u16_at(starts + 2 * s), self.u16_at(ends + 2 * s));
            let delta = self.u16_at(deltas + 2 * s);
            let offset = self.u16_at(offsets + 2 * s);
            for code in start..=end.min(0xFFFE) {
                let glyph = if offset == 0 {
                    (code + delta) & 0xFFFF
                } else {
                    let at = offsets + 2 * s + offset + 2 * (code - start);
                    match self.u16_at(at) {
                        0 => 0,
                        raw => (raw + delta) & 0xFFFF,
                    }
                };
                if glyph != 0 {
                    out.insert(code as u32);
                }
            }
        }
    }

    fn format_12(&self, sub: usize, out: &mut BTreeSet<u32>) {
        for g in 0..self.u32_at(sub + 12) {
            let group = sub + 16 + 12 * g;
            let (start, end) = (self.u32_at(group), self.u32_at(group + 4));
            out.extend((start..=end).map(|c| c as u32));
        }
    }

    /// Теги возможностей из `GSUB`: `tnum`, `liga`…
    fn substitutions(&self) -> BTreeSet<String> {
        let Some(gsub) = self.table(b"GSUB") else {
            return BTreeSet::new();
        };
        let list = gsub + self.u16_at(gsub + 6);
        (0..self.u16_at(list))
            .map(|i| {
                let at = list + 2 + 6 * i;
                String::from_utf8_lossy(&self.bytes[at..at + 4]).into_owned()
            })
            .collect()
    }
}

/// Знаки, которые окно может показать: вся таблица строк на трёх языках,
/// записи сумм, курсов и дат, знаки, которые Правило 4 называет поимённо,
/// и целые алфавиты трёх языков — их человек вводит сам: имена, адреса,
/// названия услуг.
fn needed() -> BTreeSet<char> {
    let mut text = String::new();
    for (key, _) in ALL_KEYS {
        for lang in Lang::ALL {
            text += t(lang, *key);
        }
    }
    for lang in Lang::ALL {
        for currency in [Currency::Amd, Currency::Usd, Currency::Eur] {
            text += &i18n::money(lang, Money::new(-1_234_567_890, currency));
        }
        for month in 1..=12 {
            let day = Date::new(2026, month, 1).expect("первое число есть всегда");
            text += &i18n::date(lang, day);
        }
        text += &i18n::rate(lang, 3_619_349);
    }
    text += "№«»“”‘’—–…֏€$";
    text.extend((' '..='~').chain('А'..='я').chain(['Ё', 'ё']));
    // Армянские буквы и знаки: заглавные Ա–Ֆ, строчные ա–և, ударение,
    // восклицание, вопрос, запятая-пауза, точка ։ и дефис ֊.
    text.extend(('\u{531}'..='\u{556}').chain('\u{561}'..='\u{587}'));
    text.extend([
        '\u{55B}', '\u{55C}', '\u{55D}', '\u{55E}', '\u{589}', '\u{58A}',
    ]);
    text.chars().filter(|c| !c.is_control()).collect()
}

/// Каждый нужный знак есть в каждом начертании: диапазон ведёт в файл,
/// и файл знак содержит.
#[test]
fn every_needed_char_is_in_the_fonts() {
    let faces = read_faces();
    let fonts: BTreeMap<&str, BTreeSet<u32>> = faces
        .iter()
        .map(|face| (face.file.as_str(), Font::open(&face.file).chars()))
        .collect();
    let mut wrong = Vec::new();
    for weight in token_weights() {
        let faces: Vec<&Face> = faces.iter().filter(|f| f.weight == weight).collect();
        for c in needed() {
            let covering: Vec<&&Face> = faces.iter().filter(|f| f.covers(c)).collect();
            if covering.is_empty() {
                wrong.push(format!(
                    "{weight}: «{c}» U+{:04X} — ни в одном диапазоне, браузер возьмёт системный шрифт",
                    u32::from(c)
                ));
            }
            for face in covering {
                if !fonts[face.file.as_str()].contains(&u32::from(c)) {
                    wrong.push(format!(
                        "{weight}: «{c}» U+{:04X} — диапазон ведёт в {}, а знака там нет",
                        u32::from(c),
                        face.file
                    ));
                }
            }
        }
    }
    assert!(wrong.is_empty(), "знаков нет в шрифтах: {wrong:#?}");
}

/// Каждому весу из `tokens.css` — свои файлы, и других начертаний нет:
/// вес без файла браузер нарисовал бы утолщением сам.
#[test]
fn faces_match_the_weights_of_tokens() {
    let weights: BTreeSet<u16> = read_faces().iter().map(|face| face.weight).collect();
    assert_eq!(weights, token_weights());
}

/// Каждый файл в `public/fonts` подключён в `fonts.css`, и каждый
/// подключённый лежит на месте: лишний файл — байты в выкладке, которые
/// никто не скачает, пропавший — запрос в никуда. Лицензия лежит рядом
/// (OFL, условие 2).
#[test]
fn font_files_and_rules_match() {
    let declared: BTreeSet<String> = read_faces().into_iter().map(|face| face.file).collect();
    let mut present = BTreeSet::new();
    for entry in fs::read_dir(app_dir().join("public/fonts")).expect("папка шрифтов читается")
    {
        let name = entry
            .expect("запись читается")
            .file_name()
            .into_string()
            .expect("имя в UTF-8");
        if name != "OFL.txt" {
            present.insert(name);
        }
    }
    assert_eq!(present, declared);
    assert!(
        app_dir().join("public/fonts/OFL.txt").is_file(),
        "рядом со шрифтами нет OFL.txt"
    );
}

/// Табличные цифры: `font-variant-numeric: tabular-nums` работает, только
/// если в шрифте есть `tnum` (Правило 4). Проверяется у файлов, где есть
/// цифры.
#[test]
fn digits_can_be_tabular() {
    let mut wrong = Vec::new();
    for face in read_faces() {
        let font = Font::open(&face.file);
        if font.chars().contains(&u32::from('0')) && !font.substitutions().contains("tnum") {
            wrong.push(face.file);
        }
    }
    assert!(wrong.is_empty(), "цифры есть, а tnum нет: {wrong:?}");
}
