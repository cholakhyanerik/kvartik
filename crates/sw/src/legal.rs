//! Юридические тексты сайта (задача 32): условия лицензии и политика
//! конфиденциальности на трёх языках — страницами из утверждённых файлов
//! `docs/legal/<имя>.<код языка>.md` (`release::Legal`), а не копией в таблице
//! строк. На сайте ровно утверждённый текст: правка текста — правка файла,
//! и следующая сборка её повторит, а второй копии, которая разошлась бы
//! с первой молча, нет.
//!
//! Разметка — малое подмножество Markdown, ровно то, что в файлах есть.
//! Блоки — через пустую строку: `# ` — название текста, `## ` — раздел,
//! блок целиком в `**…**` — строка редакции, `1. ` — нумерованный список
//! (продолжение пункта — с отступом), остальное — абзац, строки которого
//! сливаются через пробел. Цитатный блок (`> `) — шапка файла и «Для
//! проверки», для человека и юриста, — в страницу не идёт. Знак разметки,
//! которой разбор не знает (`*`, `_`, `` ` ``, `[`…), — ошибка сборки,
//! а не звёздочки на странице.
//!
//! Переводы держатся друг друга: у трёх языков одного текста — те же блоки
//! в том же порядке и те же числа пунктов. Пропавший в переводе абзац
//! останавливает сборку, а не уходит на сайт. Текст в HTML — только текстом
//! (`landing::escape`, Правило 13).

use kvartik_core::i18n::Lang;

/// Знаки разметки Markdown, которой разбор не знает. В утверждённых текстах
/// их нет, а появись — страница показала бы их как есть.
const UNKNOWN_MARKUP: [char; 8] = ['*', '_', '`', '[', ']', '|', '~', '\\'];

/// Текст, разобранный из файла: название и блоки по порядку.
#[derive(Debug, PartialEq)]
pub struct Doc {
    pub title: String,
    pub blocks: Vec<Block>,
}

#[derive(Debug, PartialEq)]
pub enum Block {
    /// Строка редакции: «Редакция от 09.11.2026».
    Edition(String),
    /// Заголовок раздела вместе с его номером: «1. Стороны и предмет».
    Section(String),
    Paragraph(String),
    /// Пункты нумерованного списка: номера — по порядку с единицы.
    List(Vec<String>),
}

impl Block {
    /// Что за блок — без текста: так сравниваются переводы.
    fn shape(&self) -> (u8, usize) {
        match self {
            Block::Edition(_) => (0, 0),
            Block::Section(_) => (1, 0),
            Block::Paragraph(_) => (2, 0),
            Block::List(items) => (3, items.len()),
        }
    }
}

/// Разбор файла `name`: имя — только для слов об ошибке.
pub fn parse(name: &str, text: &str) -> Result<Doc, String> {
    let fail = |line: usize, what: &str| Err(format!("{name}, строка {line}: {what}"));
    let mut title = None;
    let mut blocks = Vec::new();
    for (start, lines) in split_blocks(text) {
        let first = lines[0];
        if first.starts_with('>') {
            if let Some(at) = lines.iter().position(|line| !line.starts_with('>')) {
                return fail(start + at, "цитатный блок, в котором не каждая строка начинается с «>»");
            }
            continue;
        }
        if let Some((at, c)) = lines
            .iter()
            .enumerate()
            .find_map(|(at, line)| line.chars().find(|c| UNKNOWN_MARKUP.contains(c)).map(|c| (at, c)))
        {
            // Строка редакции — единственная со звёздочками, и только по краям.
            let edition = lines.len() == 1 && first.starts_with("**") && first.ends_with("**");
            let inner = first.trim_start_matches("**").trim_end_matches("**");
            if !edition || inner.chars().any(|c| UNKNOWN_MARKUP.contains(&c)) {
                return fail(start + at, &format!("знак разметки «{c}», которой разбор не знает"));
            }
            blocks.push(Block::Edition(typeset(inner)));
            continue;
        }
        if let Some(rest) = first.strip_prefix("# ") {
            if title.is_some() || lines.len() > 1 {
                return fail(start, "название текста — одна строка «# …» и одно на файл");
            }
            title = Some(typeset(rest.trim()));
        } else if let Some(rest) = first.strip_prefix("## ") {
            if lines.len() > 1 {
                return fail(start, "заголовок раздела — одна строка");
            }
            blocks.push(Block::Section(typeset(rest.trim())));
        } else if first.starts_with('#') {
            return fail(start, "заголовок глубже «##»");
        } else if item_number(first).is_some() {
            blocks.push(Block::List(list(name, start, &lines)?));
        } else {
            blocks.push(Block::Paragraph(join(&lines)));
        }
    }
    let Some(title) = title else {
        return Err(format!("{name}: нет названия «# …»"));
    };
    if blocks.is_empty() {
        return Err(format!("{name}: нет текста"));
    }
    Ok(Doc { title, blocks })
}

/// Блоки текста — строки между пустыми, с номером первой строки (с единицы).
fn split_blocks(text: &str) -> Vec<(usize, Vec<&str>)> {
    let mut out = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    let mut start = 0;
    for (at, line) in text.lines().enumerate() {
        let line = line.trim_end();
        if line.is_empty() {
            if !current.is_empty() {
                out.push((start, std::mem::take(&mut current)));
            }
        } else {
            if current.is_empty() {
                start = at + 1;
            }
            current.push(line);
        }
    }
    if !current.is_empty() {
        out.push((start, current));
    }
    out
}

/// Номер пункта, если строка начинается с «N. », и текст после него.
fn item_number(line: &str) -> Option<(usize, &str)> {
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    let rest = line[digits..].strip_prefix(". ")?;
    Some((line[..digits].parse().ok()?, rest))
}

/// Пункты списка: каждый — с «N. » в начале строки, продолжение — с
/// отступом; номера — 1, 2, 3…: их ставит сам список HTML, и номер,
/// пропущенный в файле, разошёлся бы с номером на странице.
fn list(name: &str, start: usize, lines: &[&str]) -> Result<Vec<String>, String> {
    let mut items: Vec<Vec<&str>> = Vec::new();
    for (at, line) in lines.iter().enumerate() {
        if let Some((number, rest)) = item_number(line) {
            if number != items.len() + 1 {
                return Err(format!("{name}, строка {}: пункт {number} вместо {}", start + at, items.len() + 1));
            }
            items.push(vec![rest]);
        } else if line.starts_with(' ') {
            match items.last_mut() {
                Some(item) => item.push(line.trim_start()),
                None => return Err(format!("{name}, строка {}: продолжение без пункта", start + at)),
            }
        } else {
            return Err(format!("{name}, строка {}: в списке строка без номера и отступа", start + at));
        }
    }
    Ok(items.iter().map(|item| join(item)).collect())
}

/// Строки абзаца — одной строкой через пробел, а тире не начинает строку
/// на экране: перед ним неразрывный пробел, как в таблице строк окна.
fn join(lines: &[&str]) -> String {
    typeset(&lines.join(" "))
}

/// Тире — после неразрывного пробела: на экране строка с него не начнётся.
fn typeset(text: &str) -> String {
    text.replace(" —", "\u{A0}—")
}

/// Переводы одного текста — те же блоки в том же порядке. `docs` — пары
/// «язык, текст».
pub fn same_shape(name: &str, docs: &[(Lang, &Doc)]) -> Result<(), String> {
    let Some((first_lang, first)) = docs.first() else {
        return Ok(());
    };
    let shape = |doc: &Doc| doc.blocks.iter().map(Block::shape).collect::<Vec<_>>();
    for (lang, doc) in &docs[1..] {
        if shape(doc) != shape(first) {
            return Err(format!(
                "{name}: блоки перевода {} не те же, что у {} — переводы разошлись",
                lang.code(),
                first_lang.code()
            ));
        }
    }
    Ok(())
}

/// Текст страницей: название, редакция, разделы — разметкой, текст
/// экранирован `escape`.
pub fn html(doc: &Doc, escape: impl Fn(&str) -> String) -> String {
    let mut out = format!("        <h1>{}</h1>\n", escape(&doc.title));
    for block in &doc.blocks {
        match block {
            Block::Edition(text) => out.push_str(&format!("        <p class=\"edition\">{}</p>\n", escape(text))),
            Block::Section(text) => out.push_str(&format!("        <h2>{}</h2>\n", escape(text))),
            Block::Paragraph(text) => out.push_str(&format!("        <p>{}</p>\n", escape(text))),
            Block::List(items) => {
                out.push_str("        <ol>\n");
                for item in items {
                    out.push_str(&format!("          <li>{}</li>\n", escape(item)));
                }
                out.push_str("        </ol>\n");
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    use kvartik_core::release::{EMAIL, Legal};

    /// Утверждённые тексты — настоящие файлы: проверка ловит правку, после
    /// которой сборка их не разобрала бы.
    fn real(doc: Legal, lang: Lang) -> (String, String) {
        let name = format!("{}.{}.md", doc.name(), lang.code());
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/legal").join(&name);
        let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        (name, text)
    }

    /// Все шесть текстов разбираются, переводы держатся друг друга,
    /// квадратных скобок и заметок «Для проверки» на странице нет, а почта
    /// продавца — та, на которую ведут подвал и «О программе».
    #[test]
    fn approved_texts_parse_and_match_their_translations() {
        let mut wrong = Vec::new();
        for doc in Legal::ALL {
            let mut parsed = Vec::new();
            for lang in Lang::ALL {
                let (name, text) = real(doc, lang);
                match parse(&name, &text) {
                    Ok(parsed_doc) => parsed.push((lang, parsed_doc)),
                    Err(error) => wrong.push(error),
                }
            }
            let pairs: Vec<(Lang, &Doc)> = parsed.iter().map(|(lang, doc)| (*lang, doc)).collect();
            if let Err(error) = same_shape(doc.name(), &pairs) {
                wrong.push(error);
            }
            for (lang, parsed_doc) in &parsed {
                let page = html(parsed_doc, str::to_owned);
                for bad in ["[", "]", "Для проверки", "**", "> "] {
                    if page.contains(bad) {
                        wrong.push(format!("{} {}: на странице «{bad}»", doc.name(), lang.code()));
                    }
                }
                if !page.contains(EMAIL) {
                    wrong.push(format!("{} {}: нет почты {EMAIL}", doc.name(), lang.code()));
                }
                if !matches!(parsed_doc.blocks.first(), Some(Block::Edition(text)) if text.contains("2026")) {
                    wrong.push(format!("{} {}: первым идёт не строка редакции", doc.name(), lang.code()));
                }
                if page.contains(" —") {
                    wrong.push(format!("{} {}: тире после обычного пробела", doc.name(), lang.code()));
                }
            }
        }
        assert!(wrong.is_empty(), "{wrong:#?}");
    }

    /// Блоки, список и редакция; цитатный блок в страницу не идёт, а строки
    /// абзаца и пункта сливаются через пробел.
    #[test]
    fn parses_the_known_markup() {
        let text = "# Название\n\n> Для проверки:\n> заметка\n\n**Редакция от 09.11.2026.**\n\n## 1. Раздел\n\n\
                    Первая строка\nвторая — с тире.\n\n1. пункт\n   с продолжением;\n2. второй.\n";
        let doc = parse("t.md", text).expect("разбирается");
        assert_eq!(doc.title, "Название");
        assert_eq!(
            doc.blocks,
            vec![
                Block::Edition("Редакция от 09.11.2026.".to_owned()),
                Block::Section("1. Раздел".to_owned()),
                Block::Paragraph("Первая строка вторая\u{A0}— с тире.".to_owned()),
                Block::List(vec!["пункт с продолжением;".to_owned(), "второй.".to_owned()]),
            ]
        );
        let page = html(&doc, |text| text.replace('<', "&lt;"));
        assert!(page.contains("<h2>1. Раздел</h2>") && page.contains("<li>второй.</li>") && !page.contains("заметка"));
    }

    /// Чего разбор не знает, — ошибка с именем файла и строкой, а не
    /// разметка, показанная как есть.
    #[test]
    fn unknown_markup_stops_the_build() {
        for (text, what) in [
            ("# Т\n\nтекст *курсивом*\n", "знак разметки «*»"),
            ("# Т\n\nссылка [туда](x)\n", "знак разметки «[»"),
            ("# Т\n\n### глубже\n", "глубже"),
            ("# Т\n\n1. первый\n3. третий\n", "пункт 3 вместо 2"),
            ("# Т\n\n> заметка\nне заметка\n", "не каждая строка"),
            ("текст без названия\n", "нет названия"),
            ("# Т\n\n# Второе\n", "одно на файл"),
            ("# Т\n\n**Редакция *с* звёздочкой**\n", "знак разметки"),
        ] {
            let error = parse("t.md", text).expect_err(text);
            assert!(error.contains(what) && error.starts_with("t.md"), "{text:?}: {error}");
        }
    }

    /// Перевод без абзаца или с другим числом пунктов — ошибка сборки.
    #[test]
    fn translations_must_keep_the_shape() {
        let one = parse("a", "# A\n\n## 1. Р\n\nабзац\n\n1. п\n2. п\n").expect("разбирается");
        let same = parse("b", "# B\n\n## 1. S\n\npara\n\n1. i\n2. i\n").expect("разбирается");
        let fewer = parse("c", "# C\n\n## 1. S\n\n1. i\n2. i\n").expect("разбирается");
        let shorter = parse("d", "# D\n\n## 1. S\n\npara\n\n1. i\n").expect("разбирается");
        assert!(same_shape("x", &[(Lang::Ru, &one), (Lang::En, &same)]).is_ok());
        assert!(same_shape("x", &[(Lang::Ru, &one), (Lang::En, &fewer)]).is_err());
        assert!(same_shape("x", &[(Lang::Ru, &one), (Lang::Am, &shorter)]).is_err());
    }
}
