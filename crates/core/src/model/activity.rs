//! Код вида экономической деятельности — по классификатору, который
//! применяется в Армении (задача 9).
//!
//! Классификатор — ՀԴ-011-2013, «Տնտեսական գործունեության տեսակների
//! դասակարգիչ», утверждён приказом министра экономики № 874-Ն от
//! 19.09.2013 (по закону: https://www.arlis.am/hy/acts/181173; сверено
//! 2026-10-12, разведка задачи 9). Код — пять цифр: раздел (2), группа (1),
//! класс (1) и национальный подкласс (1); пишется `62.01.0`. Буквы разделов
//! A–U «в код не входят» (§ 3 приложения), но в официальных текстах их
//! ставят впереди: `J62.01.0` — постановление правительства № 142-Ն от
//! 13.02.2025. Поэтому читаются обе записи, а хранится и показывается код
//! без буквы.
//!
//! До класса классификатор совпадает с NACE Rev. 2 (все 615 классов
//! и границы разделов — сверка с публикацией Eurostat KS-RA-07-015-EN,
//! там же), поэтому раздел по разделу NACE ([`SECTIONS`]) — тот же. Пятая
//! цифра — национальная. Проверяется, что раздел (две первые цифры) есть
//! в классификаторе, а буква, если её написали, — его: список всех
//! 992 подклассов в программу не встроен, и опечатку внутри раздела
//! поймает глаз.
//!
//! Риск: armstat выложил проект редакции 2.1 (по NACE 2.1), где разделы 69
//! и 70 уходят в раздел N, а группа 78.3 исчезает. Если его примут, коды
//! в правилах налога придётся сверить заново: у каждой записи с кодами
//! в файле правил записан классификатор ([`CLASSIFIER`]), и разбор файла
//! (`tax::rules`) не примет запись другой редакции.
//!
//! В данных — строка `"62.01.0"`.

use std::fmt;

use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize, Serializer};

/// Классификатор, по которому программа читает коды, — номером его
/// редакции. Им же подписаны коды в файле правил (`tax::rules`).
pub const CLASSIFIER: &str = "ՀԴ-011-2013";

/// Разделы классификатора: буква и разделы (по две цифры) от и до —
/// NACE Rev. 2. Разделов без номера (04, 34, 40…) в классификаторе нет.
pub const SECTIONS: [(char, u8, u8); 21] = [
    ('A', 1, 3),
    ('B', 5, 9),
    ('C', 10, 33),
    ('D', 35, 35),
    ('E', 36, 39),
    ('F', 41, 43),
    ('G', 45, 47),
    ('H', 49, 53),
    ('I', 55, 56),
    ('J', 58, 63),
    ('K', 64, 66),
    ('L', 68, 68),
    ('M', 69, 75),
    ('N', 77, 82),
    ('O', 84, 84),
    ('P', 85, 85),
    ('Q', 86, 88),
    ('R', 90, 93),
    ('S', 94, 96),
    ('T', 97, 98),
    ('U', 99, 99),
];

/// Буква раздела, в который входит раздел `division`; `None` — такого
/// раздела в классификаторе нет.
fn section_of(division: u8) -> Option<char> {
    SECTIONS
        .iter()
        .find(|(_, from, to)| (*from..=*to).contains(&division))
        .map(|(letter, _, _)| *letter)
}

/// Годна ли запись кода, как её пишет закон: раздел `69`, группа `78.2`,
/// класс `62.01` или подкласс `62.01.0` — цифры и точки на своих местах,
/// без буквы раздела, а раздел есть в классификаторе. Так записаны коды
/// в файле правил, и опечатка там молча исказила бы правило: `7O` не
/// совпадёт ни с одним кодом ([`ActivityCode::starts_with`]), а `78.` —
/// со всем разделом 78: точки сравнение пропускает.
pub fn is_law_prefix(text: &str) -> bool {
    // Запись подкласса; запись закона — её начало, кончающееся цифрой.
    const SHAPE: &[u8; 7] = b"00.00.0";
    let bytes = text.as_bytes();
    matches!(bytes.len(), 2 | 4 | 5 | 7)
        && bytes.iter().zip(SHAPE).all(|(byte, place)| match place {
            b'.' => *byte == b'.',
            _ => byte.is_ascii_digit(),
        })
        && section_of((bytes[0] - b'0') * 10 + (bytes[1] - b'0')).is_some()
}

/// Код вида деятельности — пять цифр классификатора.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ActivityCode([u8; 5]);

/// Почему код не принят.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum CodeError {
    /// Не пять цифр.
    Malformed,
    /// Таких двух первых цифр в классификаторе нет: `04…`, `34…`.
    NoSuchDivision,
    /// Буква впереди — не того раздела: `K62.01.0`, а 62 — раздел J.
    WrongSection,
}

impl ActivityCode {
    /// Самая длинная запись, которую стоит набирать: `J 62.01.0` и пробелы.
    pub const MAX_INPUT: usize = 12;

    /// Код, как его пишет человек: `62.01.0`, `J62.01.0`, `6201 0` — точки
    /// и пробелы не важны, буква раздела — по желанию, латиницей в любом
    /// регистре.
    pub fn parse(input: &str) -> Result<Self, CodeError> {
        let text: String = input
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '.')
            .collect();
        let (letter, digits) = match text.chars().next() {
            Some(first) if first.is_ascii_alphabetic() => {
                (Some(first.to_ascii_uppercase()), &text[1..])
            }
            _ => (None, text.as_str()),
        };
        let bytes = digits.as_bytes();
        if bytes.len() != 5 || !bytes.iter().all(u8::is_ascii_digit) {
            return Err(CodeError::Malformed);
        }
        let mut code = [0; 5];
        for (place, digit) in code.iter_mut().zip(bytes) {
            *place = digit - b'0';
        }
        let section = section_of(code[0] * 10 + code[1]).ok_or(CodeError::NoSuchDivision)?;
        if letter.is_some_and(|letter| letter != section) {
            return Err(CodeError::WrongSection);
        }
        Ok(Self(code))
    }

    /// Буква раздела: `J` у `62.01.0`.
    pub fn section(self) -> char {
        // Код существует только с разделом из классификатора (`parse`).
        section_of(self.0[0] * 10 + self.0[1]).unwrap_or('?')
    }

    /// Начинается ли код с записи `prefix` — так коды называет закон:
    /// `69` — раздел, `78.2` — группа. Точки в записи не важны.
    pub fn starts_with(self, prefix: &str) -> bool {
        let digits = prefix.bytes().filter(|b| *b != b'.');
        let mut matched = 0;
        for (place, digit) in digits.enumerate() {
            match self.0.get(place) {
                Some(own) if digit.is_ascii_digit() && *own == digit - b'0' => matched += 1,
                _ => return false,
            }
        }
        matched > 0
    }
}

/// Вид дохода для налога с оборота — строка таблицы Налогового кодекса,
/// ст. 258, ч. 1 (задача 19). Ставку, вычет расходов и нижний предел строки
/// берёт файл правил (`tax::rules`) на нужный день; здесь — только какая
/// это строка.
///
/// Строку определяет вид дохода, а не записанный код (ответ бухгалтера
/// 2026-10-19, вопросы 1.3 и 1.6 листа сверки): по коду закон узнаёт только
/// высокие технологии — список правительства 142-Ն. Поэтому у кода профиля
/// строка по умолчанию — по правилам, а человек может её сменить (решение
/// человека 2026-10-19; `model::profile::ProfileData::kinds`).
///
/// Расширяется, а смысл существующих вариантов не меняется (Правило 2);
/// `match` — без `_`. В данных — `"trade"`, `"recyclables"`,
/// `"newspapers"`, `"production"`, `"rent"`, `"catering"`,
/// `"catering_other"`, `"high_tech"`, `"assets"`, `"other"`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IncomeKind {
    /// 1 — торговля (купля-продажа), кроме строк 2 и 9.
    Trade,
    /// 2 — торговля вторсырьём из списка правительства.
    Recyclables,
    /// 3 — продажа газет их редакциями.
    Newspapers,
    /// 4 — производство.
    Production,
    /// 5 — арендная плата, процент, роялти.
    Rent,
    /// 6 — общепит.
    Catering,
    /// 7 — прочие доходы того, кто подал заявление о налоге с оборота для
    /// общепита (ст. 254, ч. 6 и 7; ст. 258, ч. 8).
    CateringOther,
    /// 8 — высокие технологии из списка правительства.
    HighTech,
    /// 9 — отчуждение прочих активов, в том числе недвижимости.
    Assets,
    /// 10 — прочая деятельность: всё, что не названо строками выше.
    Other,
}

impl IncomeKind {
    /// Все строки — в порядке таблицы закона.
    pub const ALL: [IncomeKind; 10] = [
        IncomeKind::Trade,
        IncomeKind::Recyclables,
        IncomeKind::Newspapers,
        IncomeKind::Production,
        IncomeKind::Rent,
        IncomeKind::Catering,
        IncomeKind::CateringOther,
        IncomeKind::HighTech,
        IncomeKind::Assets,
        IncomeKind::Other,
    ];

    /// Номер строки в таблице ст. 258, ч. 1: так её называет закон и бухгалтер.
    pub const fn row(self) -> u8 {
        match self {
            IncomeKind::Trade => 1,
            IncomeKind::Recyclables => 2,
            IncomeKind::Newspapers => 3,
            IncomeKind::Production => 4,
            IncomeKind::Rent => 5,
            IncomeKind::Catering => 6,
            IncomeKind::CateringOther => 7,
            IncomeKind::HighTech => 8,
            IncomeKind::Assets => 9,
            IncomeKind::Other => 10,
        }
    }
}

/// Запись классификатора: `62.01.0`. Это код, а не текст для перевода:
/// на всех языках он пишется одинаково.
impl fmt::Display for ActivityCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [a, b, c, d, e] = self.0;
        write!(f, "{a}{b}.{c}{d}.{e}")
    }
}

impl fmt::Debug for ActivityCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{self}", self.section())
    }
}

impl Serialize for ActivityCode {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

/// Читается только запись `62.01.0` — та, что пишет программа.
impl<'de> Deserialize<'de> for ActivityCode {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        match ActivityCode::parse(&text) {
            Ok(code) if code.to_string() == text => Ok(code),
            _ => Err(de::Error::invalid_value(
                de::Unexpected::Str(&text),
                &"код классификатора вида 62.01.0",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Записи, которые встречаются в выписках и в законе, читаются в один
    /// код; ожидаемое выписано человеком.
    #[test]
    fn reads_both_official_spellings() {
        let cases = [
            ("62.01.0", "62.01.0"),
            ("J62.01.0", "62.01.0"),
            ("j62.01.0", "62.01.0"),
            ("J 62.01.0", "62.01.0"),
            ("62010", "62.01.0"),
            ("  69.20.0 ", "69.20.0"),
            ("C26.11.0", "26.11.0"),
            ("01.11.0", "01.11.0"),
            ("99.00.0", "99.00.0"),
        ];
        let mut wrong = Vec::new();
        for (input, expected) in cases {
            match ActivityCode::parse(input) {
                Ok(code) if code.to_string() == expected => {}
                other => wrong.push(format!("«{input}»: {other:?}")),
            }
        }
        assert!(wrong.is_empty(), "коды прочитаны не так: {wrong:#?}");
    }

    /// Не пять цифр, раздела нет в классификаторе, буква чужого раздела —
    /// ошибки, а не догадки.
    #[test]
    fn rejects_what_is_not_a_code() {
        use CodeError::{Malformed, NoSuchDivision, WrongSection};
        let cases = [
            ("", Malformed),
            ("62.01", Malformed),
            ("62", Malformed),
            ("62.01.00", Malformed),
            ("JJ62.01.0", Malformed),
            ("62.0a.0", Malformed),
            ("Ա62.01.0", Malformed),
            ("04.00.0", NoSuchDivision),
            ("34.10.0", NoSuchDivision),
            ("00.00.0", NoSuchDivision),
            ("K62.01.0", WrongSection),
            ("N69.10.0", WrongSection),
        ];
        let mut wrong = Vec::new();
        for (input, expected) in cases {
            let got = ActivityCode::parse(input);
            if got != Err(expected) {
                wrong.push(format!("«{input}»: {got:?}, а не {expected:?}"));
            }
        }
        assert!(wrong.is_empty(), "прочитано не то: {wrong:#?}");
    }

    /// Разделов в классификаторе 88 (там же, сверка разведки задачи 9),
    /// и у каждого — одна буква; разделы букв не пересекаются.
    #[test]
    fn sections_cover_88_divisions() {
        let divisions: Vec<u8> = (0..=99).filter(|d| section_of(*d).is_some()).collect();
        assert_eq!(divisions.len(), 88);
        for (letter, from, to) in SECTIONS {
            for division in from..=to {
                assert_eq!(section_of(division), Some(letter), "раздел {division}");
            }
        }
        assert_eq!(ActivityCode::parse("62.01.0").map(ActivityCode::section), Ok('J'));
        assert_eq!(ActivityCode::parse("78.20.0").map(ActivityCode::section), Ok('N'));
    }

    /// Код начинается с записи закона: раздел, группа, класс.
    #[test]
    fn prefixes_as_the_law_writes_them() {
        let code = |text| ActivityCode::parse(text).expect("в тесте — годный код");
        assert!(code("69.10.0").starts_with("69"));
        assert!(code("78.20.0").starts_with("78.2"));
        assert!(!code("78.10.0").starts_with("78.2"));
        assert!(code("62.02.9").starts_with("62.02.9"));
        assert!(!code("62.02.1").starts_with("62.02.9"));
        assert!(!code("70.22.0").starts_with("7022.01"));
        assert!(!code("70.22.0").starts_with(""));
        assert!(!code("70.22.0").starts_with("x"));
    }

    /// Запись закона — раздел, группа, класс или подкласс в форме
    /// `62.01.0`, раздел из классификатора; опечатки и чужие записи — нет.
    #[test]
    fn law_prefixes_have_the_classifier_shape() {
        let mut wrong = Vec::new();
        for text in ["69", "70", "78.2", "78.3", "62.01", "62.01.0", "01", "99.00.0"] {
            if !is_law_prefix(text) {
                wrong.push(format!("«{text}» отвергнута"));
            }
        }
        for text in [
            "", "6", "7O", "78.", "62.01.", "6201", "62.010", "62.01.00", "62.01.0.0", "J62", "J62.01.0",
            " 69", "69 ", "04", "34.1", "00", "٦٩", "６９",
        ] {
            if is_law_prefix(text) {
                wrong.push(format!("«{text}» принята"));
            }
        }
        assert!(wrong.is_empty(), "записи закона: {wrong:#?}");
    }

    /// Строки вида дохода — по порядку таблицы ст. 258, ч. 1, с первой по
    /// десятую, а в данных — имена, выписанные человеком: имя — часть
    /// формата, и поменять его можно только миграцией (Правило 2).
    #[test]
    fn income_kinds_follow_the_table() {
        let rows: Vec<u8> = IncomeKind::ALL.iter().map(|kind| kind.row()).collect();
        assert_eq!(rows, (1..=10).collect::<Vec<u8>>());
        let names = [
            "trade",
            "recyclables",
            "newspapers",
            "production",
            "rent",
            "catering",
            "catering_other",
            "high_tech",
            "assets",
            "other",
        ];
        let mut wrong = Vec::new();
        for (kind, name) in IncomeKind::ALL.into_iter().zip(names) {
            let json = format!("\"{name}\"");
            if serde_json::to_string(&kind).ok().as_deref() != Some(json.as_str())
                || serde_json::from_str::<IncomeKind>(&json).ok() != Some(kind)
            {
                wrong.push(name);
            }
        }
        assert!(wrong.is_empty(), "имена строк в данных: {wrong:?}");
    }

    /// В данных — `"62.01.0"`; запись с буквой или без точек — порча.
    #[test]
    fn data_keeps_the_dotted_code() {
        let code = ActivityCode::parse("J62.01.0").expect("годный код");
        assert_eq!(serde_json::to_string(&code).ok().as_deref(), Some(r#""62.01.0""#));
        assert_eq!(serde_json::from_str::<ActivityCode>(r#""62.01.0""#).ok(), Some(code));
        for json in [r#""J62.01.0""#, r#""62010""#, r#""04.00.0""#, "62010"] {
            assert!(
                serde_json::from_str::<ActivityCode>(json).is_err(),
                "{json} прочитан"
            );
        }
    }
}
