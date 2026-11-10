//! Счёт и акт целиком: строки на трёх языках, раскладка и файл PDF.
//!
//! Файл читается обратно маленьким читателем PDF здесь же: таблица `xref`
//! ведёт на свои объекты, потоки разжимаются, а текст страниц собирается
//! из номеров глифов через `/ToUnicode` — то же, что делает читалка, когда
//! человек ищет или копирует текст. Так видно, что в PDF стоит именно то,
//! что разложено, а не только что файл «собрался».

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use miniz_oxide::inflate::decompress_to_vec_zlib;

use super::fonts::FILES;
use super::image::Picture;
use super::page::{Fonts, Page, texts};
use super::pdf::{self, Info};
use super::sheet::{Sheet, Source};
use super::*;
use crate::i18n::{self, Key, Lang, t};
use crate::model::document::{DocKind, Document};
use crate::model::logo::Image;
use crate::model::money::{Currency, Money};

/// Номер документа — из записи: поля его закрыты.
fn number(year: u16, seq: u32) -> Number {
    serde_json::from_str(&format!(r#"{{"year":{year},"seq":{seq}}}"#)).expect("номер")
}

fn app_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../app")
}

/// Байты всех файлов шрифтов документа.
fn font_files() -> Vec<(&'static str, Vec<u8>)> {
    FILES
        .iter()
        .map(|file| {
            let path = app_dir().join("public/fonts").join(file.name);
            let bytes =
                fs::read(&path).unwrap_or_else(|_| panic!("{} не читается", path.display()));
            (file.name, bytes)
        })
        .collect()
}

fn fonts<'a>(files: &'a [(&'static str, Vec<u8>)]) -> Fonts<'a> {
    Fonts::new(files.iter().map(|(name, bytes)| (*name, bytes.as_slice())))
        .expect("шрифты читаются")
}

/// Реквизиты ИП — на трёх языках, два счёта: в долларах и в драмах.
const SELLER: &str = concat!(
    r#"{"name":{"hy":"ԱՁ Արամ Պետրոսյան","ru":"ИП Арам Петросян","en":"IE Aram Petrosyan"},"#,
    r#""tin":"07654321","registration":"271.1234567","#,
    r#""address":{"hy":"ք. Երևան, Աբովյան 1","ru":"г. Ереван, ул. Абовяна 1","en":"Abovyan St 1, Yerevan"},"#,
    r#""phone":"+374 91 123456","email":"aram@example.com","#,
    r#""accounts":[{"currency":"USD","number":"1570012345678901","bank":"Ameriabank CJSC","swift":"ARMIAM22","#,
    r#""correspondent":"Citibank N.A., New York\nSWIFT CITIUS33"},"#,
    r#"{"currency":"AMD","number":"1570012345678902","bank":"Ameriabank CJSC","swift":null,"correspondent":""}]}"#
);

/// Выпущенный счёт на языке `lang`: иностранный клиент, две строки,
/// итого 5 500,00 $.
fn invoice(lang: Lang) -> Document {
    invoice_with(lang, SELLER)
}

/// Реквизиты ИП, у долларового счёта которого банк записан по-армянски,
/// а латиницей — отдельно (формат 17, полный прогон 2026-11-10, п. 8).
fn latin_seller() -> String {
    SELLER.replacen(
        r#""bank":"Ameriabank CJSC","swift":"ARMIAM22","#,
        r#""bank":"«Ամերիաբանկ» ՓԲԸ","bank_latin":"Ameriabank CJSC","swift":"ARMIAM22","#,
        1,
    )
}

/// Тот же счёт с реквизитами ИП `seller`.
fn invoice_with(lang: Lang, seller: &str) -> Document {
    let json = format!(
        concat!(
            r#"{{"meta":{{"id":"7c9e6679-7425-40de-944b-e07fc1f90ae7","modified":1760000000000,"deleted":null}},"#,
            r#""data":{{"kind":"invoice","client":"5b0e8f9a-3c2d-4e1f-8a7b-9c6d5e4f3a2b","lang":"{lang}","currency":"USD","#,
            r#""date":"2026-10-01","service_date":"2026-09-30","due":"2026-10-15","#,
            r#""items":[{{"text":"Software development services, September 2026","quantity_e3":1000,"price":500000}},"#,
            r#"{{"text":"Code review","quantity_e3":12500,"price":4000}}],"#,
            r#""basis":null,"recurring":null}},"#,
            r#""issued":{{"number":{{"year":2026,"seq":7}},"seller":{seller},"#,
            r#""buyer":{{"name":"Beispiel GmbH","country":"DE","address":"Hauptstraße 1\n10115 Berlin","tin":null,"#,
            r#""tax_id":"DE123456789","currency":"USD","lang":"en","payment_days":14}}}},"#,
            r#""annulled":null,"payments":[]}}"#
        ),
        lang = lang.code(),
        seller = seller,
    );
    serde_json::from_str(&json).expect("счёт читается")
}

/// Выпущенный акт в драмах армянской фирме — по счёту `2026-007`.
fn act(lang: Lang) -> Document {
    let json = format!(
        concat!(
            r#"{{"meta":{{"id":"7c9e6679-7425-40de-944b-e07fc1f90ae8","modified":1760000000500,"deleted":null}},"#,
            r#""data":{{"kind":"act","client":"5b0e8f9a-3c2d-4e1f-8a7b-9c6d5e4f3a2c","lang":"{lang}","currency":"AMD","#,
            r#""date":"2026-10-13","service_date":"2026-10-13","due":null,"#,
            r#""items":[{{"text":"Ծրագրային ապահովման մշակում","quantity_e3":1000,"price":1500000}}],"#,
            r#""basis":"7c9e6679-7425-40de-944b-e07fc1f90ae7","recurring":null}},"#,
            r#""issued":{{"number":{{"year":2026,"seq":3}},"seller":{seller},"#,
            r#""buyer":{{"name":"«Արարատ» ՍՊԸ","country":"AM","address":"ք. Երևան, Մաշտոցի 10","tin":"00012345","#,
            r#""tax_id":"","currency":"AMD","lang":"hy","payment_days":null}}}},"#,
            r#""annulled":null,"payments":[]}}"#
        ),
        lang = lang.code(),
        seller = SELLER
    );
    serde_json::from_str(&json).expect("акт читается")
}

/// Логотип — значок программы (PNG с палитрой).
fn logo() -> Image {
    let bytes = fs::read(app_dir().join("public/icons/icon-192.png")).expect("значок читается");
    Image::new(bytes).expect("значок — годная картинка")
}

/// Документ — страницами и файлом.
fn make(
    document: &Document,
    basis: Option<Number>,
    logo: Option<&Image>,
) -> (Vec<Page>, Vec<u8>, Sheet) {
    let files = font_files();
    let fonts = fonts(&files);
    let source = Source::issued(document, basis).expect("документ выпущен");
    let sheet = Sheet::new(&source);
    let picture = logo.map(|image| Picture::new(image).expect("логотип читается"));
    let pages = sheet.pages(&fonts, picture.as_ref().map(Picture::size));
    let title = format!("{}", document.number().expect("номер есть"));
    let bytes = pdf::write(
        &pages,
        &fonts,
        picture.as_ref(),
        &Info {
            title: &title,
            lang: document.data().lang,
        },
    )
    .expect("PDF пишется");
    (pages, bytes, sheet)
}

// ——— Маленький читатель PDF ———

struct Object {
    dict: String,
    data: Option<Vec<u8>>,
}

fn find(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    haystack[from..]
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|at| at + from)
}

/// Число после ключа словаря: `/Length 123`, `/Contents 5 0 R`.
fn after(dict: &str, key: &str) -> Option<usize> {
    let rest = &dict[dict.find(&format!("{key} "))? + key.len() + 1..];
    rest.split(|c: char| !c.is_ascii_digit())
        .next()?
        .parse()
        .ok()
}

/// Объекты по таблице `xref`: каждое смещение ведёт на свой объект.
fn objects(pdf: &[u8]) -> BTreeMap<usize, Object> {
    assert!(pdf.starts_with(b"%PDF-1.7\n"), "заголовок PDF");
    assert!(pdf.ends_with(b"%%EOF\n"), "конец файла");
    let tail = String::from_utf8_lossy(&pdf[pdf.len() - 40..]).into_owned();
    let start: usize = tail
        .split("startxref\n")
        .nth(1)
        .and_then(|rest| rest.lines().next())
        .and_then(|n| n.parse().ok())
        .expect("startxref");
    let table = String::from_utf8_lossy(&pdf[start..]).into_owned();
    let mut lines = table.lines();
    assert_eq!(lines.next(), Some("xref"));
    let count: usize = lines
        .next()
        .and_then(|l| l.split(' ').nth(1))
        .and_then(|n| n.parse().ok())
        .expect("число объектов");
    lines.next();
    let mut out = BTreeMap::new();
    for id in 1..count {
        let entry = lines.next().expect("запись xref");
        let offset: usize = entry[..10].parse().expect("смещение");
        let head = format!("{id} 0 obj\n");
        assert!(
            pdf[offset..].starts_with(head.as_bytes()),
            "xref: объект {id} не на месте"
        );
        let body = offset + head.len();
        let end = find(pdf, b"\nendobj\n", body).expect("endobj");
        let stream = find(pdf, b">>\nstream\n", body).filter(|&at| at < end);
        let object = match stream {
            None => Object {
                dict: String::from_utf8_lossy(&pdf[body..end]).into_owned(),
                data: None,
            },
            Some(at) => {
                let dict = String::from_utf8_lossy(&pdf[body..at + 2]).into_owned();
                let len = after(&dict, "/Length").expect("/Length");
                let start = at + ">>\nstream\n".len();
                let raw = &pdf[start..start + len];
                assert!(
                    pdf[start + len..].starts_with(b"\nendstream\nendobj\n"),
                    "длина потока {id}"
                );
                let data = if dict.contains("/FlateDecode") {
                    decompress_to_vec_zlib(raw).expect("поток разжимается")
                } else {
                    raw.to_vec()
                };
                Object {
                    dict,
                    data: Some(data),
                }
            }
        };
        out.insert(id, object);
    }
    out
}

/// Таблица `/ToUnicode` шрифта: глиф → текст.
fn cmap(objects: &BTreeMap<usize, Object>, font: usize) -> BTreeMap<u16, String> {
    let unicode = after(&objects[&font].dict, "/ToUnicode").expect("у шрифта есть /ToUnicode");
    let text = String::from_utf8(objects[&unicode].data.clone().expect("поток")).expect("текст");
    let mut out = BTreeMap::new();
    for line in text.lines().filter(|l| l.starts_with('<')) {
        let parts: Vec<&str> = line
            .split(['<', '>', ' '])
            .filter(|p| !p.is_empty())
            .collect();
        if let [glyph, code] = parts.as_slice()
            && glyph.len() == 4
        {
            let units: Vec<u16> = code
                .as_bytes()
                .chunks(4)
                .map(|c| {
                    u16::from_str_radix(std::str::from_utf8(c).expect("hex"), 16).expect("hex")
                })
                .collect();
            out.insert(
                u16::from_str_radix(glyph, 16).expect("hex"),
                String::from_utf16(&units).expect("UTF-16"),
            );
        }
    }
    out
}

/// Текст страниц PDF: строка на каждый `BT … ET`.
fn read_text(pdf: &[u8]) -> Vec<Vec<String>> {
    let objects = objects(pdf);
    let root = objects
        .values()
        .find(|o| o.dict.contains("/Type /Catalog"))
        .expect("каталог");
    let tree = after(&root.dict, "/Pages").expect("/Pages");
    let kids: Vec<usize> = {
        let dict = &objects[&tree].dict;
        let list = &dict[dict.find("/Kids [").expect("/Kids") + 7..];
        let list = &list[..list.find(']').expect("]")];
        list.split(" 0 R")
            .filter_map(|n| n.trim().parse().ok())
            .collect()
    };
    kids.iter()
        .map(|page| {
            let dict = &objects[page].dict;
            let mut fonts = BTreeMap::new();
            for piece in dict.split("/F").skip(1) {
                let mut words = piece.split(' ');
                if let (Some(name), Some(id)) = (words.next(), words.next())
                    && let (Ok(name), Ok(id)) = (name.parse::<usize>(), id.parse::<usize>())
                {
                    fonts.insert(name, cmap(&objects, id));
                }
            }
            let contents = after(dict, "/Contents").expect("/Contents");
            let content =
                String::from_utf8(objects[&contents].data.clone().expect("поток")).expect("текст");
            let mut lines = Vec::new();
            let mut line = String::new();
            let mut font = None;
            let tokens: Vec<&str> = content.split_whitespace().collect();
            for (i, token) in tokens.iter().enumerate() {
                if let Some(name) = token.strip_prefix("/F") {
                    font = name.parse::<usize>().ok();
                } else if token.starts_with('<') && tokens.get(i + 1) == Some(&"Tj") {
                    let map = &fonts[&font.expect("шрифт выбран")];
                    let hex = token.trim_matches(['<', '>']);
                    for chunk in hex.as_bytes().chunks(4) {
                        let glyph =
                            u16::from_str_radix(std::str::from_utf8(chunk).expect("hex"), 16)
                                .expect("hex");
                        line.push_str(map.get(&glyph).map_or("\u{FFFD}", String::as_str));
                    }
                } else if *token == "ET" {
                    lines.push(std::mem::take(&mut line));
                }
            }
            lines
        })
        .collect()
}

fn all_text(pages: &[Vec<String>]) -> String {
    pages
        .iter()
        .flatten()
        .cloned()
        .collect::<Vec<_>>()
        .join("\n")
}

/// Счёт на каждом языке: в тексте PDF — название и номер на языке
/// документа, обе стороны, строки, итог записью языка, реквизиты счёта
/// в валюте документа и ни одного знака без глифа. Провалы собираются по
/// всем языкам (Правило 2).
#[test]
fn invoice_text_is_in_the_pdf_in_every_language() {
    let mut wrong = Vec::new();
    for lang in Lang::ALL {
        let document = invoice(lang);
        let (pages, bytes, _) = make(&document, None, Some(&logo()));
        let text = all_text(&read_text(&bytes));
        let total = i18n::money(lang, Money::new(550_000, Currency::Usd));
        let number = i18n::fill(t(lang, Key::PdfNumber), &["2026-007"]);
        let expected = [
            t(lang, Key::KindInvoice),
            number.as_str(),
            t(lang, Key::PdfSeller),
            t(lang, Key::PdfBuyer),
            document
                .issued()
                .expect("выпущен")
                .seller()
                .name
                .get(lang)
                .as_str(),
            "Beispiel GmbH",
            "Hauptstraße 1",
            "DE123456789",
            "Software development services, September 2026",
            "Code review",
            total.as_str(),
            "1570012345678901",
            "ARMIAM22",
            "Citibank N.A., New York",
            &i18n::date(
                lang,
                crate::model::date::Date::new(2026, 10, 15).expect("дата"),
            ),
        ];
        for piece in expected {
            if !text.contains(piece) {
                wrong.push(format!("{lang:?}: в PDF нет «{piece}»"));
            }
        }
        // Драмовый счёт иностранному клиенту в долларах не нужен.
        if text.contains("1570012345678902") {
            wrong.push(format!("{lang:?}: в счёте в долларах — драмовый счёт"));
        }
        if text.contains('\u{FFFD}') {
            wrong.push(format!("{lang:?}: глиф без знака в /ToUnicode"));
        }
        let files = font_files();
        let missing = fonts(&files).missing(&pages);
        if !missing.is_empty() {
            wrong.push(format!("{lang:?}: нет в шрифтах {missing:?}"));
        }
        if pages.len() != 1 {
            wrong.push(format!("{lang:?}: страниц {}", pages.len()));
        }
        // Размер: шрифты — подмножествами, логотип и страница сжаты.
        if bytes.len() > 60_000 {
            wrong.push(format!("{lang:?}: {} байтов", bytes.len()));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// Банк — на языке документа (полный прогон 2026-11-10, п. 8): у счёта
/// с названием латиницей армянский PDF пишет банк, как он записан, а русский
/// и английский — латиницей. Без латиницы, как у документов, выпущенных до
/// формата 17, — как записан, на всех языках (тест выше).
#[test]
fn the_bank_is_written_in_the_documents_letters() {
    let latin = latin_seller();
    assert_ne!(latin, SELLER, "подмена не сработала");
    let mut wrong = Vec::new();
    for lang in Lang::ALL {
        let (_, bytes, _) = make(&invoice_with(lang, &latin), None, None);
        let text = all_text(&read_text(&bytes));
        let (shown, hidden) = match lang {
            Lang::Am => ("«Ամերիաբանկ» ՓԲԸ", "Ameriabank CJSC"),
            Lang::Ru | Lang::En => ("Ameriabank CJSC", "«Ամերիաբանկ» ՓԲԸ"),
        };
        if !text.contains(shown) {
            wrong.push(format!("{lang:?}: в PDF нет «{shown}»"));
        }
        if text.contains(hidden) {
            wrong.push(format!("{lang:?}: в PDF «{hidden}»"));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// Акт: основание, слова о приёмке и подписи сторон на языке документа,
/// ՀՎՀՀ армянской фирмы.
#[test]
fn act_has_basis_statement_and_signatures() {
    let mut wrong = Vec::new();
    let basis = number(2026, 7);
    for lang in Lang::ALL {
        let (_, bytes, _) = make(&act(lang), Some(basis), None);
        let text = all_text(&read_text(&bytes)).replace('\n', " ");
        let statement_start: String = t(lang, Key::PdfActStatement).chars().take(30).collect();
        let tin = i18n::fill(t(lang, Key::TinShort), &["00012345"]);
        let basis = i18n::fill(t(lang, Key::DocBasis), &["2026-007"]);
        for piece in [
            t(lang, Key::KindAct),
            statement_start.as_str(),
            t(lang, Key::PdfSignSeller),
            t(lang, Key::PdfSignBuyer),
            tin.as_str(),
            basis.as_str(),
            "«Արարատ» ՍՊԸ",
            &i18n::money(lang, Money::new(1_500_000, Currency::Amd)),
        ] {
            if !text.contains(piece) {
                wrong.push(format!("{lang:?}: в акте нет «{piece}»"));
            }
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// Сто строк — несколько страниц: у каждой страницы своя шапка таблицы
/// и номер «N / M», а итог — один, на последней.
#[test]
fn long_documents_break_into_numbered_pages() {
    let mut document = serde_json::to_value(invoice(Lang::Ru)).expect("в JSON");
    let item = document["data"]["items"][0].clone();
    document["data"]["items"] = serde_json::Value::Array(vec![item; 100]);
    let document: Document = serde_json::from_value(document).expect("счёт на сто строк");
    let (pages, bytes, _) = make(&document, None, None);
    let text = read_text(&bytes);
    assert!(pages.len() >= 3, "страниц {}", pages.len());
    let count = pages.len().to_string();
    for (i, page) in text.iter().enumerate() {
        let footer = format!("{} / {count}", i + 1);
        assert!(
            page.contains(&footer),
            "на странице {} нет «{footer}»",
            i + 1
        );
        assert!(
            page.iter().any(|l| l == t(Lang::Ru, Key::ItemText)),
            "на странице {} нет шапки",
            i + 1
        );
    }
    let rows = text
        .iter()
        .flatten()
        .filter(|l| l.starts_with("Software development"))
        .count();
    assert_eq!(rows, 100, "все строки на месте");
    let totals = text
        .iter()
        .flatten()
        .filter(|l| *l == t(Lang::Ru, Key::PdfTotal))
        .count();
    assert_eq!(totals, 1);
    // Ни один текст не вылез за поля страницы.
    for text in texts(&pages) {
        assert!(
            text.baseline > 0 && text.baseline < super::page::PAGE_HEIGHT,
            "{text:?}"
        );
    }
}

/// Тексты не налезают друг на друга и не выходят за поля: рамка текста —
/// его ширина и высота от подъёма до спуска шрифта. Наложение подписей на
/// слова о приёмке акта нашёл PDF-прогон 2026-10-14 — теперь его ловит
/// тест.
#[test]
fn texts_do_not_overlap_or_leave_the_page() {
    let files = font_files();
    let fonts = fonts(&files);
    let mut wrong = Vec::new();
    for lang in Lang::ALL {
        for (name, document, basis) in [
            ("счёт", invoice(lang), None),
            ("акт", act(lang), Some(number(2026, 7))),
        ] {
            let (pages, _, _) = make(&document, basis, Some(&logo()));
            for (p, page) in pages.iter().enumerate() {
                let boxes: Vec<(&super::page::Text, [i32; 4])> = page
                    .marks
                    .iter()
                    .filter_map(|mark| match mark {
                        super::page::Mark::Text(text) => {
                            let width = fonts.width(text.face, text.size, &text.text);
                            let top = text.baseline - text.size * 1069 / 1000;
                            let bottom = text.baseline + text.size * 293 / 1000;
                            Some((text, [text.x, top, text.x + width, bottom]))
                        }
                        super::page::Mark::Rule(_) | super::page::Mark::Logo(_) => None,
                    })
                    .collect();
                for (i, (a, [ax0, ay0, ax1, ay1])) in boxes.iter().enumerate() {
                    if *ax0 < 0 || *ax1 > super::page::PAGE_WIDTH {
                        wrong.push(format!(
                            "{lang:?} {name}, стр. {}: «{}» за полем",
                            p + 1,
                            a.text
                        ));
                    }
                    for (b, [bx0, by0, bx1, by1]) in &boxes[i + 1..] {
                        if ax0 < bx1 && bx0 < ax1 && ay0 < by1 && by0 < ay1 {
                            wrong.push(format!(
                                "{lang:?} {name}, стр. {}: «{}» налезает на «{}»",
                                p + 1,
                                a.text,
                                b.text
                            ));
                        }
                    }
                }
            }
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// Один документ — одни байты: ни даты, ни случайного `/ID`.
#[test]
fn same_document_same_bytes() {
    let document = invoice(Lang::Am);
    let (_, first, _) = make(&document, None, Some(&logo()));
    let (_, second, _) = make(&document, None, Some(&logo()));
    assert_eq!(first, second);
}

/// Окно берёт у сайта только нужные файлы шрифтов: русскому счёту
/// в долларах армянский не нужен, если армянских знаков в нём нет.
#[test]
fn only_needed_font_files_are_asked_for() {
    let mut document = serde_json::to_value(invoice(Lang::Ru)).expect("в JSON");
    // В русской подписи ИНН — армянское «ՀՎՀՀ» (`TinShort`): без ՀՎՀՀ
    // армянских знаков в счёте нет.
    document["issued"]["seller"]["tin"] = serde_json::Value::Null;
    let document: Document = serde_json::from_value(document).expect("счёт");
    let source = Source::issued(&document, None).expect("выпущен");
    let names: Vec<&str> = Sheet::new(&source)
        .needed_files()
        .iter()
        .map(|f| f.name)
        .collect();
    assert!(names.contains(&"noto-sans-cyrillic-400.ttf"), "{names:?}");
    assert!(names.contains(&"noto-serif-cyrillic-800.ttf"), "{names:?}");
    assert!(!names.iter().any(|n| n.contains("armenian")), "{names:?}");
    // «ß» из адреса клиента — латиница-1, расширенная не нужна.
    assert!(!names.iter().any(|n| n.contains("latin-ext")), "{names:?}");
}

#[test]
fn file_names_are_latin() {
    let number = number(2026, 1);
    assert_eq!(file_name(DocKind::Invoice, number), "Invoice-2026-001.pdf");
    assert_eq!(file_name(DocKind::Act, number), "Act-2026-001.pdf");
}

/// PDF-прогон (CLAUDE.md, Правило 2): счёт и акт на трёх языках, счёт на
/// сто строк — в `target/look/task-12/`, глазами. Не в обычном прогоне:
/// `cargo test -p kvartik-core look_pdf -- --ignored`.
#[test]
#[ignore = "пишет файлы для глаз"]
fn look_pdf() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/look/task-12");
    fs::create_dir_all(&dir).expect("папка");
    for lang in Lang::ALL {
        let (_, bytes, _) = make(&invoice(lang), None, Some(&logo()));
        fs::write(dir.join(format!("invoice-{}.pdf", lang.code())), bytes).expect("запись");
        let (_, bytes, _) = make(&act(lang), Some(number(2026, 7)), None);
        fs::write(dir.join(format!("act-{}.pdf", lang.code())), bytes).expect("запись");
        let (_, bytes, _) = make(&invoice_with(lang, &latin_seller()), None, Some(&logo()));
        fs::write(dir.join(format!("invoice-latin-bank-{}.pdf", lang.code())), bytes).expect("запись");
    }
    let mut long = serde_json::to_value(invoice(Lang::Am)).expect("в JSON");
    let item = long["data"]["items"][0].clone();
    long["data"]["items"] = serde_json::Value::Array(vec![item; 60]);
    let long: Document = serde_json::from_value(long).expect("счёт");
    let (_, bytes, _) = make(&long, None, Some(&logo()));
    fs::write(dir.join("invoice-hy-60.pdf"), bytes).expect("запись");
}
