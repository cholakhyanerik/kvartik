//! Страница документа — то, что рисуют и PDF, и превью в окне.
//!
//! Раскладка ([`super::sheet`]) превращает документ в страницы: тексты
//! с местом, кеглем и начертанием, линии и логотип. PDF ([`super::pdf`])
//! пишет их в файл, а окно рисует те же страницы картинкой SVG. Одна
//! раскладка на двоих — поэтому в превью ровно те поля, порядок и суммы,
//! что в PDF (Правило 2, «Что в превью, то и в PDF»).
//!
//! Места — целые сотые доли пункта (1/7200 дюйма) от левого верхнего угла
//! страницы, как в SVG: целые числа дают те же байты PDF на любой машине,
//! а сотой пункта глазу хватает.

use std::collections::BTreeSet;

use super::font::{Font, FontError};
use super::fonts::{FILES, Face, FontFile};

/// Длина в сотых долях пункта.
pub type Pt = i32;

/// Сотых пункта в пункте.
pub const PT: Pt = 100;

/// Лист A4 — 210 × 297 мм: 595,28 × 841,89 пункта (ISO 216).
pub const PAGE_WIDTH: Pt = 59_528;
pub const PAGE_HEIGHT: Pt = 84_189;

/// Цвет текста и линии — ролью, а не числом: PDF берёт числа у ядра
/// ([`super::pdf::rgb`]), окно — у переменных `tokens.css`, а тест окна
/// сверяет, что это одни и те же цвета (Правило 4).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Ink {
    /// Основной текст.
    Text,
    /// Подписи полей и второстепенное.
    Muted,
    /// Заголовок и итог.
    Accent,
    /// Линии таблицы.
    Rule,
}

/// Текст в одну строку: левый край и базовая линия.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Text {
    pub x: Pt,
    pub baseline: Pt,
    pub size: Pt,
    pub face: Face,
    pub ink: Ink,
    pub text: String,
}

/// Отрезок линии.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Rule {
    pub x1: Pt,
    pub y1: Pt,
    pub x2: Pt,
    pub y2: Pt,
    pub width: Pt,
}

/// Место логотипа: левый верхний угол, ширина и высота.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Frame {
    pub x: Pt,
    pub y: Pt,
    pub width: Pt,
    pub height: Pt,
}

/// Что нарисовано на странице.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Mark {
    Text(Text),
    Rule(Rule),
    Logo(Frame),
}

/// Страница: что на ней, по порядку.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Page {
    pub marks: Vec<Mark>,
}

/// Шрифты документа: прочитанные файлы из [`FILES`].
pub struct Fonts<'a> {
    /// Файл таблицы и его шрифт — в порядке [`FILES`].
    loaded: Vec<(&'static FontFile, Font<'a>)>,
}

/// Где взять знак: номер файла в [`Fonts`] и глиф в нём.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Glyph {
    pub font: usize,
    pub id: u16,
}

impl<'a> Fonts<'a> {
    /// Шрифты из байтов файлов: имя файла из [`FILES`] и его байты. Файлы
    /// не из таблицы не берутся; какие нужны документу, говорит
    /// [`needed_files`].
    pub fn new<'b>(
        files: impl IntoIterator<Item = (&'b str, &'a [u8])>,
    ) -> Result<Self, FontError> {
        let mut loaded = Vec::new();
        for (name, bytes) in files {
            if let Some(file) = FILES.iter().find(|file| file.name == name) {
                loaded.push((file, Font::parse(bytes)?));
            }
        }
        loaded.sort_by_key(|(file, _)| FILES.iter().position(|f| f == *file));
        Ok(Self { loaded })
    }

    /// Файлы и их шрифты — в порядке [`FILES`].
    pub fn loaded(&self) -> &[(&'static FontFile, Font<'a>)] {
        &self.loaded
    }

    /// Где взять знак `c` начертания `face`: первый файл, в чей диапазон он
    /// входит и в котором есть его глиф, — так же ищет браузер по
    /// `unicode-range`. Нет в начертании — в запасном ([`Face::fallback`]).
    /// `None` — знака нет ни в одном шрифте документа.
    pub fn glyph(&self, face: Face, c: char) -> Option<Glyph> {
        let found = self
            .loaded
            .iter()
            .enumerate()
            .filter(|(_, (file, _))| file.face == face && file.covers(c))
            .find_map(|(font, (_, f))| f.glyph(c).map(|id| Glyph { font, id }));
        found.or_else(|| face.fallback().and_then(|face| self.glyph(face, c)))
    }

    /// Ширина текста кеглем `size` — сумма ширин глифов. Знак, которого нет
    /// ни в одном шрифте, ширины не занимает (его называет [`Fonts::missing`]).
    pub fn width(&self, face: Face, size: Pt, text: &str) -> Pt {
        let units: i64 = text
            .chars()
            .filter_map(|c| self.glyph(face, c))
            .map(|glyph| {
                let font = &self.loaded[glyph.font].1;
                // Сотые пункта: ширина × кегль / единиц на кегль.
                i64::from(font.advance(glyph.id)) * i64::from(size) * 1000
                    / i64::from(font.units_per_em())
            })
            .sum();
        Pt::try_from((units + 500) / 1000).unwrap_or(Pt::MAX)
    }

    /// Знаки страниц, которых нет ни в одном шрифте документа: PDF нарисует
    /// на их месте пустоту, и окно говорит о них словами (Правило 6).
    pub fn missing(&self, pages: &[Page]) -> BTreeSet<char> {
        texts(pages)
            .flat_map(|text| {
                text.text
                    .chars()
                    .filter(|&c| !c.is_whitespace() && self.glyph(text.face, c).is_none())
            })
            .collect()
    }
}

/// Все тексты страниц.
pub fn texts(pages: &[Page]) -> impl Iterator<Item = &Text> {
    pages.iter().flat_map(|page| {
        page.marks.iter().filter_map(|mark| match mark {
            Mark::Text(text) => Some(text),
            Mark::Rule(_) | Mark::Logo(_) => None,
        })
    })
}

/// Файлы шрифтов, которые нужны текстам `strings` начертаний `faces`: те,
/// в чей диапазон входит хоть один знак. Окно берёт у сайта только их:
/// русскому документу армянский шрифт не нужен, если в нём нет ни армянской
/// буквы, ни знака ֏.
pub fn needed_files<'s>(
    faces: &[Face],
    strings: impl IntoIterator<Item = &'s str>,
) -> Vec<&'static FontFile> {
    let chars: BTreeSet<char> = strings
        .into_iter()
        .flat_map(str::chars)
        .filter(|c| !c.is_control())
        .collect();
    let mut wanted: BTreeSet<Face> = faces.iter().copied().collect();
    // Запасное начертание заголовка — на случай знака не из его файлов.
    wanted.extend(faces.iter().filter_map(|face| face.fallback()));
    FILES
        .iter()
        .filter(|file| wanted.contains(&file.face))
        .filter(|file| chars.iter().any(|&c| file.covers(c)))
        .collect()
}

/// Строки текста шириной не больше `width`: слова — по пробелам, перевод
/// строки — новая строка, а слово длиннее строки режется по знакам.
pub fn wrap(fonts: &Fonts<'_>, face: Face, size: Pt, text: &str, width: Pt) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for word in paragraph.split(' ').filter(|word| !word.is_empty()) {
            let joined = if line.is_empty() {
                word.to_owned()
            } else {
                format!("{line} {word}")
            };
            if fonts.width(face, size, &joined) <= width {
                line = joined;
                continue;
            }
            if !line.is_empty() {
                lines.push(std::mem::take(&mut line));
            }
            // Слово целиком не влезает и в пустую строку — режется по знакам.
            for c in word.chars() {
                let mut longer = line.clone();
                longer.push(c);
                if !line.is_empty() && fonts.width(face, size, &longer) > width {
                    lines.push(std::mem::take(&mut line));
                    line.push(c);
                } else {
                    line = longer;
                }
            }
        }
        lines.push(line);
    }
    // Пустые строки в конце (адрес, кончившийся переводом строки) не нужны.
    while lines.len() > 1 && lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    lines
}
