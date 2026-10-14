//! Файлы шрифтов документа и знаки в каждом.
//!
//! Это файлы окна (`crates/app/public/fonts/`): их пишет `tools/fonts.py`
//! подмножествами Noto по диапазонам знаков Google Fonts, а окно
//! подключает правилами `@font-face` в `style/fonts.css`. Таблица ниже —
//! копия тех правил для текста и заголовков: окно по ней знает, какие файлы
//! взять у сайта для документа, а документ — в каком файле искать знак
//! первым. Разойтись с `fonts.css` она не может: их сверяет тест окна
//! `crates/app/tests/fonts.rs`.
//!
//! Caprasimo окна («Kvartik» и крупные суммы) здесь нет: в нём только знаки
//! сумм, а текст документа — любой.

/// Начертание текста в документе.
///
/// Расширяется, а `match` — без `_` (Правило 2): новое начертание каждое
/// место, где оно что-то решает, должно увидеть само.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Face {
    /// Текст — Noto Sans и Noto Sans Armenian 400.
    Regular,
    /// Выделенный текст — они же, 600.
    Strong,
    /// Заголовок — Noto Serif и Noto Serif Armenian 800, как заголовки окна.
    Heading,
}

impl Face {
    /// Все начертания.
    pub const ALL: [Face; 3] = [Face::Regular, Face::Strong, Face::Heading];

    /// Семейство в `fonts.css` и вес CSS — для окна и для проверки таблицы.
    pub const fn css(self) -> (&'static str, u16) {
        match self {
            Face::Regular => ("Kvartik Sans", 400),
            Face::Strong => ("Kvartik Sans", 600),
            Face::Heading => ("Kvartik Serif", 800),
        }
    }

    /// Где искать знак, которого нет в файлах начертания: у заголовка —
    /// в выделенном тексте (в Noto Serif нет расширенной латиницы, шапка
    /// `tools/fonts.py`). Так же делает окно: `--font-serif` в `tokens.css`
    /// — «Kvartik Serif», потом «Kvartik Sans».
    pub const fn fallback(self) -> Option<Face> {
        match self {
            Face::Regular | Face::Strong => None,
            Face::Heading => Some(Face::Strong),
        }
    }
}

/// Файл шрифта: имя в `fonts/`, начертание, имя шрифта для PDF и знаки
/// (`unicode-range` правила `@font-face`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct FontFile {
    pub name: &'static str,
    pub face: Face,
    /// Имя шрифта PostScript — в PDF шрифт называется им (`/BaseFont`).
    pub postscript: &'static str,
    pub ranges: &'static [(u32, u32)],
}

impl FontFile {
    /// Отправит ли окно знак `c` в этот файл.
    pub fn covers(&self, c: char) -> bool {
        let c = u32::from(c);
        self.ranges.iter().any(|&(lo, hi)| (lo..=hi).contains(&c))
    }
}

/// Диапазоны — как у Google Fonts для Noto Sans (`tools/fonts.py`, `LATIN`,
/// `LATIN_EXT`, `CYRILLIC`, `ARMENIAN`).
const LATIN: &[(u32, u32)] = &[
    (0x0000, 0x00FF),
    (0x0131, 0x0131),
    (0x0152, 0x0153),
    (0x02BB, 0x02BC),
    (0x02C6, 0x02C6),
    (0x02DA, 0x02DA),
    (0x02DC, 0x02DC),
    (0x0304, 0x0304),
    (0x0308, 0x0308),
    (0x0329, 0x0329),
    (0x2000, 0x206F),
    (0x20AC, 0x20AC),
    (0x2122, 0x2122),
    (0x2191, 0x2191),
    (0x2193, 0x2193),
    (0x2212, 0x2212),
    (0x2215, 0x2215),
    (0xFEFF, 0xFEFF),
    (0xFFFD, 0xFFFD),
];
const LATIN_EXT: &[(u32, u32)] = &[
    (0x0100, 0x02BA),
    (0x02BD, 0x02C5),
    (0x02C7, 0x02CC),
    (0x02CE, 0x02D7),
    (0x02DD, 0x02FF),
    (0x0304, 0x0304),
    (0x0308, 0x0308),
    (0x0329, 0x0329),
    (0x1D00, 0x1DBF),
    (0x1E00, 0x1E9F),
    (0x1EF2, 0x1EFF),
    (0x2020, 0x2020),
    (0x20A0, 0x20AB),
    (0x20AD, 0x20C0),
    (0x2113, 0x2113),
    (0x2C60, 0x2C7F),
    (0xA720, 0xA7FF),
];
const CYRILLIC: &[(u32, u32)] = &[
    (0x0301, 0x0301),
    (0x0400, 0x045F),
    (0x0490, 0x0491),
    (0x04B0, 0x04B1),
    (0x2116, 0x2116),
];
const ARMENIAN: &[(u32, u32)] = &[(0x0530, 0x058F), (0xFB13, 0xFB17)];

/// Файлы в порядке правил `fonts.css`: в нём же документ ищет знак.
pub const FILES: &[FontFile] = &[
    FontFile {
        name: "noto-sans-latin-400.ttf",
        face: Face::Regular,
        postscript: "NotoSans-Regular",
        ranges: LATIN,
    },
    FontFile {
        name: "noto-sans-latin-ext-400.ttf",
        face: Face::Regular,
        postscript: "NotoSans-Regular",
        ranges: LATIN_EXT,
    },
    FontFile {
        name: "noto-sans-cyrillic-400.ttf",
        face: Face::Regular,
        postscript: "NotoSans-Regular",
        ranges: CYRILLIC,
    },
    FontFile {
        name: "noto-sans-armenian-400.ttf",
        face: Face::Regular,
        postscript: "NotoSansArmenian-Regular",
        ranges: ARMENIAN,
    },
    FontFile {
        name: "noto-sans-latin-600.ttf",
        face: Face::Strong,
        postscript: "NotoSans-SemiBold",
        ranges: LATIN,
    },
    FontFile {
        name: "noto-sans-latin-ext-600.ttf",
        face: Face::Strong,
        postscript: "NotoSans-SemiBold",
        ranges: LATIN_EXT,
    },
    FontFile {
        name: "noto-sans-cyrillic-600.ttf",
        face: Face::Strong,
        postscript: "NotoSans-SemiBold",
        ranges: CYRILLIC,
    },
    FontFile {
        name: "noto-sans-armenian-600.ttf",
        face: Face::Strong,
        postscript: "NotoSansArmenian-SemiBold",
        ranges: ARMENIAN,
    },
    FontFile {
        name: "noto-serif-latin-800.ttf",
        face: Face::Heading,
        postscript: "NotoSerif-ExtraBold",
        ranges: LATIN,
    },
    FontFile {
        name: "noto-serif-cyrillic-800.ttf",
        face: Face::Heading,
        postscript: "NotoSerif-ExtraBold",
        ranges: CYRILLIC,
    },
    FontFile {
        name: "noto-serif-armenian-800.ttf",
        face: Face::Heading,
        postscript: "NotoSerifArmenian-ExtraBold",
        ranges: ARMENIAN,
    },
];
