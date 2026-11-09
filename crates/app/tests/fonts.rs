//! Шрифты окна (Правило 4): каждый знак, который окно может показать, есть
//! в том файле, куда его отправит браузер.
//!
//! Отсутствующий знак не даёт ошибки: браузер молча берёт его из системного
//! шрифта, и буква выглядит чужой (Правило 6). Поэтому знак проверяется
//! дважды: его `unicode-range` в `style/fonts.css` должен вести хоть в один
//! файл каждого начертания, и каждый такой файл должен этот знак содержать
//! (по таблице `cmap`).
//!
//! Тест машинный: читает файлы с диска. Под wasm32 (браузерные тесты,
//! клиппи окна) его нет — там нечего читать.
#![cfg(not(target_arch = "wasm32"))]

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

use kvartik_core::i18n::{self, ALL_KEYS, Key, Lang, t};
use kvartik_core::model::country::Country;
use kvartik_core::model::date::Date;
use kvartik_core::model::money::{Currency, Money};
use kvartik_core::release::{Legal, Notes};
use kvartik_core::tax::rules;

fn app_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Что семейство обязано рисовать само.
#[derive(Clone, Copy)]
enum Coverage {
    /// Всё, что окно может показать ([`needed`]): текст и заголовки.
    All,
    /// Только крупные суммы и «Kvartik» ([`display_needed`]): остальное
    /// берёт следующее семейство стека `--font-display` (tokens.css).
    Display,
}

/// Семейства окна (`tools/fonts.py`): имя в `fonts.css`, переменные весов
/// его начертаний в `tokens.css` и что оно обязано рисовать.
const FAMILIES: [(&str, &[&str], Coverage); 3] = [
    ("Kvartik Sans", &["--weight-regular", "--weight-strong"], Coverage::All),
    ("Kvartik Serif", &["--weight-heading"], Coverage::All),
    ("Kvartik Display", &["--weight-regular"], Coverage::Display),
];

/// Правило `@font-face`: семейство, вес, файл и диапазоны знаков.
struct Face {
    family: String,
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
        let family = property("font-family").trim_matches('"').to_owned();
        faces.push(Face {
            family,
            weight: property("font-weight").parse().expect("вес — число"),
            file,
            ranges,
        });
    }
    faces
}

/// Начертания, которые объявляет `tokens.css`: `--weight-…: 400;` — по
/// имени переменной.
fn token_weights() -> BTreeMap<String, u16> {
    let text = fs::read_to_string(app_dir().join("style/tokens.css")).expect("tokens.css читается");
    text.lines()
        .filter_map(|line| line.trim().strip_prefix("--weight-"))
        .map(|rest| {
            let (name, value) = rest.split_once(':').expect("--weight-…: число;");
            let weight = value
                .trim()
                .trim_end_matches(';')
                .parse()
                .expect("вес — число");
            (format!("--weight-{name}"), weight)
        })
        .collect()
}

/// Веса начертаний семейства — по его переменным в `tokens.css`.
fn family_weights(names: &[&str]) -> BTreeSet<u16> {
    let weights = token_weights();
    names
        .iter()
        .map(|name| *weights.get(*name).unwrap_or_else(|| panic!("{name} нет в tokens.css")))
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

    /// Знаки, у которых есть глиф.
    fn chars(&self) -> BTreeSet<u32> {
        self.glyphs().into_keys().collect()
    }

    /// Знак — номер его глифа: подтаблица `cmap` формата 4 (BMP) или 12.
    fn glyphs(&self) -> BTreeMap<u32, usize> {
        let cmap = self.table(b"cmap").expect("в шрифте есть cmap");
        let mut out = BTreeMap::new();
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

    fn format_4(&self, sub: usize, out: &mut BTreeMap<u32, usize>) {
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
                    out.insert(code as u32, glyph);
                }
            }
        }
    }

    fn format_12(&self, sub: usize, out: &mut BTreeMap<u32, usize>) {
        for g in 0..self.u32_at(sub + 12) {
            let group = sub + 16 + 12 * g;
            let (start, end) = (self.u32_at(group), self.u32_at(group + 4));
            let first = self.u32_at(group + 8);
            out.extend((start..=end).map(|c| (c as u32, first + (c - start))));
        }
    }

    /// Ширина глифа (`hmtx`): у глифов после последней записи — ширина
    /// последней (`hhea.numberOfHMetrics`).
    fn advance(&self, glyph: usize) -> usize {
        let hhea = self.table(b"hhea").expect("в шрифте есть hhea");
        let hmtx = self.table(b"hmtx").expect("в шрифте есть hmtx");
        let metrics = self.u16_at(hhea + 34);
        self.u16_at(hmtx + 4 * glyph.min(metrics - 1))
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
/// заметки «Что нового» (`public/notes.json`, задача 67), названия стран
/// (`model::country`, задача 10), акты файла правил (задача 31), записи
/// сумм, курсов и дат, знаки, которые
/// Правило 4 называет поимённо, и целые алфавиты трёх языков — их человек
/// вводит сам: имена, адреса, названия услуг.
fn needed() -> BTreeSet<char> {
    let mut text = String::new();
    for (key, _) in ALL_KEYS {
        for lang in Lang::ALL {
            text += t(lang, *key);
        }
    }
    let notes = fs::read_to_string(app_dir().join("public/notes.json")).expect("notes.json читается");
    for note in Notes::parse(&notes).expect("notes.json разбирается").all() {
        for lang in Lang::ALL {
            for item in note.items(lang) {
                text += item;
            }
        }
    }
    for lang in Lang::ALL {
        for currency in [Currency::Amd, Currency::Usd, Currency::Eur] {
            text += &i18n::money(lang, Money::new(-1_234_567_890, currency));
        }
        for month in 1..=12 {
            let day = Date::new(2026, month, 1).expect("первое число есть всегда");
            text += &i18n::date(lang, day);
            text += &i18n::month_heading(lang, 2025, month, 2026);
            text += &i18n::month_heading(lang, 2026, month, 2026);
        }
        // Дни с месяцем словом — все месяцы и все дни недели.
        for day in 1..=7 {
            let date = Date::new(2026, 10, 11 + day).expect("день есть");
            text += &i18n::day_long(lang, date);
        }
        for month in 1..=12 {
            text += &i18n::day_long(lang, Date::new(2026, month, 5).expect("день есть"));
        }
        text += &i18n::rate(lang, 3_619_349);
        for country in Country::by_name(lang) {
            text += country.name(lang);
        }
    }
    // Акты, на которых стоят правила, — их названия на экране «О программе»
    // (задача 31), на любом языке окна — армянскими, как на arlis.am.
    for act in rules::armenia().acts() {
        text += act.name;
    }
    // Юридические тексты сайта (задача 32): их страницы — теми же шрифтами,
    // что окно. Цитатные блоки — заметки для человека — на страницу не идут.
    for doc in Legal::ALL {
        for lang in Lang::ALL {
            let path = app_dir().join(format!("../../docs/legal/{}.{}.md", doc.name(), lang.code()));
            let source = fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            for line in source.lines().filter(|line| !line.starts_with('>')) {
                text += line;
            }
        }
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

/// Знаки, которые окно пишет крупным шрифтом «Kvartik Display»: суммы на
/// трёх языках во всех валютах и «Kvartik». ֏ в Caprasimo нет вовсе — его
/// рисует следующее семейство стека, как и в дизайне.
fn display_needed() -> BTreeSet<char> {
    let mut text = String::new();
    for lang in Lang::ALL {
        text += t(lang, Key::Brand);
        for currency in [Currency::Amd, Currency::Usd, Currency::Eur] {
            text += &i18n::money(lang, Money::new(-1_234_567_890, currency));
        }
    }
    text.chars().filter(|c| *c != '֏').collect()
}

/// Каждый нужный знак есть в каждом начертании каждого семейства: диапазон
/// ведёт в файл, и файл знак содержит.
#[test]
fn every_needed_char_is_in_the_fonts() {
    let faces = read_faces();
    let fonts: BTreeMap<&str, BTreeSet<u32>> = faces
        .iter()
        .map(|face| (face.file.as_str(), Font::open(&face.file).chars()))
        .collect();
    let mut wrong = Vec::new();
    for (family, weights, coverage) in FAMILIES {
        let chars = match coverage {
            Coverage::All => needed(),
            Coverage::Display => display_needed(),
        };
        for weight in family_weights(weights) {
            let faces: Vec<&Face> = faces
                .iter()
                .filter(|f| f.family == family && f.weight == weight)
                .collect();
            for &c in &chars {
                let covering: Vec<&&Face> = faces.iter().filter(|f| f.covers(c)).collect();
                if covering.is_empty() {
                    wrong.push(format!(
                        "{family} {weight}: «{c}» U+{:04X} — ни в одном диапазоне, браузер возьмёт другой шрифт",
                        u32::from(c)
                    ));
                }
                for face in covering {
                    if !fonts[face.file.as_str()].contains(&u32::from(c)) {
                        wrong.push(format!(
                            "{family} {weight}: «{c}» U+{:04X} — диапазон ведёт в {}, а знака там нет",
                            u32::from(c),
                            face.file
                        ));
                    }
                }
            }
        }
    }
    assert!(wrong.is_empty(), "знаков нет в шрифтах: {wrong:#?}");
}

/// У семейства — ровно начертания его весов из `tokens.css`, и семейств
/// других нет: вес без файла браузер нарисовал бы утолщением сам, а файл
/// без веса никто бы не скачал.
#[test]
fn faces_match_the_weights_of_tokens() {
    let faces = read_faces();
    let mut wrong = Vec::new();
    for (family, weights, _) in FAMILIES {
        let declared: BTreeSet<u16> = faces
            .iter()
            .filter(|face| face.family == family)
            .map(|face| face.weight)
            .collect();
        if declared != family_weights(weights) {
            wrong.push(format!("{family}: начертания {declared:?}, а в tokens.css {weights:?}"));
        }
    }
    for face in &faces {
        if !FAMILIES.iter().any(|(family, _, _)| *family == face.family) {
            wrong.push(format!("{}: семейства «{}» нет в списке", face.file, face.family));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
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
/// цифры. У Caprasimo `tnum` нет, но цифры одной ширины и так: тогда
/// столбец не «пляшет» и без него.
#[test]
fn digits_can_be_tabular() {
    let mut wrong = Vec::new();
    for face in read_faces() {
        let font = Font::open(&face.file);
        let glyphs = font.glyphs();
        let Some(widths) = ('0'..='9')
            .map(|digit| glyphs.get(&u32::from(digit)).map(|&glyph| font.advance(glyph)))
            .collect::<Option<BTreeSet<usize>>>()
        else {
            continue;
        };
        if widths.len() > 1 && !font.substitutions().contains("tnum") {
            wrong.push(face.file);
        }
    }
    assert!(wrong.is_empty(), "цифры разной ширины, а tnum нет: {wrong:?}");
}

/// Шрифты документа (задача 12) — те же файлы, что у окна: таблица ядра
/// `kvartik_core::doc::fonts::FILES` — копия правил `fonts.css` для текста
/// и заголовков: файл, семейство, вес и диапазон, в том же порядке. По ней
/// окно берёт файлы для PDF, а PDF ищет знак, — разойдись она с `fonts.css`,
/// превью и PDF взяли бы знак из разных файлов. Знаки самих строк документа
/// проверяет [`every_needed_char_is_in_the_fonts`]: таблица строк одна
/// на окно и документ.
#[test]
fn document_fonts_copy_the_font_faces() {
    let faces: Vec<Face> = read_faces()
        .into_iter()
        .filter(|face| face.family != "Kvartik Display")
        .collect();
    let files = kvartik_core::doc::fonts::FILES;
    let mut wrong = Vec::new();
    if faces.len() != files.len() {
        wrong.push(format!("в fonts.css {} правил текста и заголовков, в ядре {}", faces.len(), files.len()));
    }
    for (face, file) in faces.iter().zip(files) {
        let (family, weight) = file.face.css();
        if face.file != file.name || face.family != family || face.weight != weight {
            wrong.push(format!(
                "{}: в fonts.css «{}» {} {}, в ядре «{family}» {weight} {}",
                face.file, face.family, face.weight, face.file, file.name
            ));
        }
        if face.ranges != file.ranges {
            wrong.push(format!("{}: диапазоны разошлись", face.file));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}
