//! Счёт и акт на бумаге: что в документе, на языке документа, и как это
//! ложится на страницы A4.
//!
//! Сначала [`Sheet::new`] собирает строки документа — подписи на языке
//! документа (`i18n`), суммы и даты его записью (`i18n::money`,
//! `i18n::date`), реквизиты сторон. Шрифтов для этого не нужно, и по этим
//! строкам окно узнаёт, какие файлы шрифтов взять у сайта
//! ([`Sheet::needed_files`]). Потом [`Sheet::pages`] раскладывает их по
//! страницам по ширинам глифов — это и рисуют PDF и превью.
//!
//! Порядок на странице — шапка (логотип, название и номер), даты, стороны,
//! услуги с итогом, платёжные реквизиты, у акта — слова о приёмке
//! и подписи. Строк услуг бывает до ста: таблица переносится на следующую
//! страницу со своей шапкой, а блоки не рвутся посередине.

use super::fonts::{Face, FontFile};
use super::page::{
    Fonts, Frame, Ink, Mark, PAGE_HEIGHT, PAGE_WIDTH, PT, Page, Pt, Rule, Text, needed_files, wrap,
};
use crate::i18n::{self, Key, Lang, fill, t};
use crate::model::bank::BankAccount;
use crate::model::client::ClientData;
use crate::model::date::Date;
use crate::model::document::{DocKind, Document, DocumentData, Number, QUANTITY_DIGITS, Seller};
use crate::model::money::{Currency, Money};

/// Поля страницы: слева и справа — 19 мм, сверху и снизу — 17,6 мм.
const MARGIN_X: Pt = 54 * PT;
const MARGIN_TOP: Pt = 50 * PT;
const MARGIN_BOTTOM: Pt = 50 * PT;
/// Ширина набора.
const WIDTH: Pt = PAGE_WIDTH - 2 * MARGIN_X;
/// Нижний край набора: ниже — только номер страницы.
const BOTTOM: Pt = PAGE_HEIGHT - MARGIN_BOTTOM;

/// Кегли.
const SIZE_TEXT: Pt = 950;
const SIZE_LABEL: Pt = 780;
const SIZE_NAME: Pt = 1050;
const SIZE_NUMBER: Pt = 1150;
const SIZE_TITLE: Pt = 2200;
const SIZE_TOTAL: Pt = 1250;

/// Самый большой логотип: 150 × 56 пунктов — около 5 × 2 см.
const LOGO_WIDTH: Pt = 150 * PT;
const LOGO_HEIGHT: Pt = 56 * PT;

/// Толщина линий таблицы.
const RULE: Pt = 60;

/// Межстрочное расстояние кегля `size`: 1,4 кегля.
const fn leading(size: Pt) -> Pt {
    size * 14 / 10
}

/// Базовая линия строки кегля `size`, чей верх — `top`: подъём Noto —
/// 1,069 кегля, и надстрочные знаки армянских и русских букв в него
/// помещаются.
const fn baseline(top: Pt, size: Pt) -> Pt {
    top + size * 1069 / 1000
}

/// Откуда берутся данные документа.
#[derive(Clone, Copy, Debug)]
pub struct Source<'a> {
    pub data: &'a DocumentData,
    /// Номер; `None` — черновик: номер он получит при выпуске.
    pub number: Option<Number>,
    pub seller: &'a Seller,
    pub buyer: &'a ClientData,
    /// Номер счёта-основания у акта, если счёт известен.
    pub basis: Option<Number>,
    /// День аннулирования; `None` — документ действует.
    pub annulled: Option<Date>,
}

impl<'a> Source<'a> {
    /// Выпущенный документ — с копией реквизитов на день выпуска. `None` —
    /// черновик: у него реквизиты — из профиля и клиента ([`Source::draft`]).
    pub fn issued(document: &'a Document, basis: Option<Number>) -> Option<Self> {
        let issued = document.issued()?;
        Some(Self {
            data: document.data(),
            number: Some(issued.number()),
            seller: issued.seller(),
            buyer: issued.buyer(),
            basis,
            annulled: document.annulled(),
        })
    }

    /// Черновик: реквизиты — нынешние, из профиля и записи клиента.
    pub fn draft(
        data: &'a DocumentData,
        seller: &'a Seller,
        buyer: &'a ClientData,
        basis: Option<Number>,
    ) -> Self {
        Self {
            data,
            number: None,
            seller,
            buyer,
            basis,
            annulled: None,
        }
    }
}

/// Сторона документа: подпись и строки реквизитов.
#[derive(Clone, PartialEq, Eq, Debug)]
struct Party {
    label: String,
    name: String,
    lines: Vec<String>,
}

/// Строка таблицы услуг: номер, услуга, количество, цена и сумма.
type Row = [String; 5];

/// Документ строками на его языке — до раскладки.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Sheet {
    lang: Lang,
    title: String,
    number: String,
    annulled: Option<String>,
    facts: Vec<(String, String)>,
    basis: Option<String>,
    parties: [Party; 2],
    head: Row,
    rows: Vec<Row>,
    total: (String, String),
    payment: Option<(String, Vec<(String, String)>)>,
    act: Option<(String, [String; 2])>,
}

/// Знак-разделитель в строке реквизитов: телефон · почта.
const DOT: &str = " · ";

impl Sheet {
    /// Строки документа на его языке.
    pub fn new(source: &Source<'_>) -> Self {
        let data = source.data;
        let lang = data.lang;
        let money = |minor: i64| i18n::money(lang, Money::new(minor, data.currency));
        let title = t(lang, kind_key(data.kind)).to_owned();
        let number = match source.number {
            Some(number) => fill(t(lang, Key::PdfNumber), &[&number.to_string()]),
            None => t(lang, Key::PdfNumberLater).to_owned(),
        };
        let annulled = source
            .annulled
            .map(|day| fill(t(lang, Key::PdfAnnulled), &[&i18n::date(lang, day)]));

        let mut facts = vec![(
            t(lang, Key::DocDate).to_owned(),
            i18n::date(lang, data.date),
        )];
        facts.push((
            t(lang, Key::DocServiceDate).to_owned(),
            i18n::date(lang, data.service_date),
        ));
        if let Some(due) = data.due {
            facts.push((t(lang, Key::DocDue).to_owned(), i18n::date(lang, due)));
        }
        let basis = source
            .basis
            .map(|number| fill(t(lang, Key::DocBasis), &[&number.to_string()]));

        let head = [
            t(lang, Key::PdfColumnNumber).to_owned(),
            t(lang, Key::ItemText).to_owned(),
            t(lang, Key::ItemQuantity).to_owned(),
            t(lang, Key::ItemPrice).to_owned(),
            t(lang, Key::PdfColumnAmount).to_owned(),
        ];
        let rows = data
            .items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                [
                    i18n::integer(lang, i as i64 + 1),
                    item.text.as_str().to_owned(),
                    i18n::decimal(lang, item.quantity_e3, QUANTITY_DIGITS),
                    money(item.price),
                    // Сумма строки помещается: это проверил `DocumentData::check`.
                    item.total(data.currency)
                        .map(|sum| i18n::money(lang, sum))
                        .unwrap_or_default(),
                ]
            })
            .collect();
        let total = (
            t(lang, Key::PdfTotal).to_owned(),
            data.total()
                .map(|sum| i18n::money(lang, sum))
                .unwrap_or_default(),
        );

        Self {
            lang,
            title,
            number,
            annulled,
            facts,
            basis,
            parties: [
                seller_party(lang, source.seller),
                buyer_party(lang, source.buyer),
            ],
            head,
            rows,
            total,
            payment: payment(lang, source.seller, data.currency),
            act: match data.kind {
                DocKind::Invoice => None,
                DocKind::Act => Some((
                    t(lang, Key::PdfActStatement).to_owned(),
                    [
                        t(lang, Key::PdfSignSeller).to_owned(),
                        t(lang, Key::PdfSignBuyer).to_owned(),
                    ],
                )),
            },
        }
    }

    /// Язык документа.
    pub const fn lang(&self) -> Lang {
        self.lang
    }

    /// Все строки документа — по ним видно, какие нужны шрифты.
    fn strings(&self) -> impl Iterator<Item = &str> {
        let parties = self.parties.iter().flat_map(|party| {
            [party.label.as_str(), party.name.as_str()]
                .into_iter()
                .chain(party.lines.iter().map(String::as_str))
        });
        let payment = self.payment.iter().flat_map(|(title, pairs)| {
            std::iter::once(title.as_str())
                .chain(pairs.iter().flat_map(|(l, v)| [l.as_str(), v.as_str()]))
        });
        let act = self.act.iter().flat_map(|(statement, signs)| {
            std::iter::once(statement.as_str()).chain(signs.iter().map(String::as_str))
        });
        [
            self.title.as_str(),
            self.number.as_str(),
            self.total.0.as_str(),
            self.total.1.as_str(),
        ]
        .into_iter()
        .chain(self.annulled.as_deref())
        .chain(self.basis.as_deref())
        .chain(
            self.facts
                .iter()
                .flat_map(|(l, v)| [l.as_str(), v.as_str()]),
        )
        .chain(parties)
        .chain(self.head.iter().map(String::as_str))
        .chain(self.rows.iter().flatten().map(String::as_str))
        .chain(payment)
        .chain(act)
        .chain(std::iter::once(t(self.lang, Key::PdfPage)))
    }

    /// Файлы шрифтов, которые нужны документу (`page::needed_files`).
    pub fn needed_files(&self) -> Vec<&'static FontFile> {
        needed_files(&Face::ALL, self.strings())
    }

    /// Страницы документа. `logo` — ширина и высота картинки логотипа
    /// в точках, если он есть: место под него — с теми же пропорциями.
    pub fn pages(&self, fonts: &Fonts<'_>, logo: Option<(u32, u32)>) -> Vec<Page> {
        let mut pen = Pen {
            fonts,
            pages: vec![Page::default()],
            y: MARGIN_TOP,
        };
        self.header(&mut pen, logo);
        self.facts(&mut pen);
        self.parties(&mut pen);
        self.items(&mut pen);
        self.payment(&mut pen);
        self.act(&mut pen);
        pen.number_pages(self.lang);
        pen.pages
    }

    /// Шапка: логотип слева, название и номер справа.
    fn header(&self, pen: &mut Pen<'_, '_>, logo: Option<(u32, u32)>) {
        let mut block = Block::default();
        let logo_height = logo
            .and_then(|(w, h)| fit(w, h))
            .map_or(0, |(width, height)| {
                block.marks.push(Mark::Logo(Frame {
                    x: MARGIN_X,
                    y: 0,
                    width,
                    height,
                }));
                height
            });
        // Название — правее логотипа, сколько бы ни заняло.
        let column = if logo.is_some() {
            WIDTH - LOGO_WIDTH - 20 * PT
        } else {
            WIDTH
        };
        let right = MARGIN_X + WIDTH;
        let mut y = 0;
        for line in wrap(pen.fonts, Face::Heading, SIZE_TITLE, &self.title, column) {
            let width = pen.fonts.width(Face::Heading, SIZE_TITLE, &line);
            block.text(
                right - width,
                baseline(y, SIZE_TITLE),
                SIZE_TITLE,
                Face::Heading,
                Ink::Accent,
                line,
            );
            y += leading(SIZE_TITLE);
        }
        y += 4 * PT;
        let width = pen.fonts.width(Face::Strong, SIZE_NUMBER, &self.number);
        block.text(
            right - width,
            baseline(y, SIZE_NUMBER),
            SIZE_NUMBER,
            Face::Strong,
            Ink::Text,
            self.number.clone(),
        );
        y += leading(SIZE_NUMBER);
        if let Some(annulled) = &self.annulled {
            let width = pen.fonts.width(Face::Strong, SIZE_TEXT, annulled);
            block.text(
                right - width,
                baseline(y, SIZE_TEXT),
                SIZE_TEXT,
                Face::Strong,
                Ink::Accent,
                annulled.clone(),
            );
            y += leading(SIZE_TEXT);
        }
        block.height = y.max(logo_height);
        pen.place(block);
    }

    /// Даты документа строкой столбцов: подпись над значением.
    fn facts(&self, pen: &mut Pen<'_, '_>) {
        pen.gap(20 * PT);
        let mut block = Block::default();
        let count = Pt::try_from(self.facts.len()).unwrap_or(1).max(3);
        let column = WIDTH / count;
        let mut height = 0;
        for (i, (label, value)) in self.facts.iter().enumerate() {
            let x = MARGIN_X + column * Pt::try_from(i).unwrap_or(0);
            let mut y = block.label(pen.fonts, x, 0, column - 8 * PT, label);
            for line in wrap(pen.fonts, Face::Regular, SIZE_TEXT, value, column - 8 * PT) {
                block.text(
                    x,
                    baseline(y, SIZE_TEXT),
                    SIZE_TEXT,
                    Face::Regular,
                    Ink::Text,
                    line,
                );
                y += leading(SIZE_TEXT);
            }
            height = height.max(y);
        }
        if let Some(basis) = &self.basis {
            height += 6 * PT;
            for line in wrap(pen.fonts, Face::Regular, SIZE_TEXT, basis, WIDTH) {
                block.text(
                    MARGIN_X,
                    baseline(height, SIZE_TEXT),
                    SIZE_TEXT,
                    Face::Regular,
                    Ink::Text,
                    line,
                );
                height += leading(SIZE_TEXT);
            }
        }
        block.height = height;
        pen.place(block);
    }

    /// Стороны: ИП слева, клиент справа.
    fn parties(&self, pen: &mut Pen<'_, '_>) {
        pen.gap(18 * PT);
        let mut block = Block::default();
        let column = (WIDTH - 24 * PT) / 2;
        for (i, party) in self.parties.iter().enumerate() {
            let x = MARGIN_X + (column + 24 * PT) * Pt::try_from(i).unwrap_or(0);
            let mut y = block.label(pen.fonts, x, 0, column, &party.label);
            for line in wrap(pen.fonts, Face::Strong, SIZE_NAME, &party.name, column) {
                block.text(
                    x,
                    baseline(y, SIZE_NAME),
                    SIZE_NAME,
                    Face::Strong,
                    Ink::Text,
                    line,
                );
                y += leading(SIZE_NAME);
            }
            for text in &party.lines {
                for line in wrap(pen.fonts, Face::Regular, SIZE_TEXT, text, column) {
                    block.text(
                        x,
                        baseline(y, SIZE_TEXT),
                        SIZE_TEXT,
                        Face::Regular,
                        Ink::Text,
                        line,
                    );
                    y += leading(SIZE_TEXT);
                }
            }
            block.height = block.height.max(y);
        }
        pen.place(block);
    }

    /// Ширины столбцов таблицы: номер, услуга, количество, цена, сумма.
    /// Числа не переносятся: их столбцы — по самому широкому числу, услуге —
    /// остальное.
    fn columns(&self, fonts: &Fonts<'_>) -> [Pt; 5] {
        let widest = |i: usize, face: Face, size: Pt, min: Pt| {
            self.rows
                .iter()
                .map(|row| fonts.width(face, size, &row[i]))
                .chain(std::iter::once(fonts.width(
                    Face::Regular,
                    SIZE_LABEL,
                    &self.head[i],
                )))
                .max()
                .unwrap_or(0)
                .max(min)
        };
        let number = widest(0, Face::Regular, SIZE_TEXT, 14 * PT) + 10 * PT;
        let quantity = widest(2, Face::Regular, SIZE_TEXT, 30 * PT) + 14 * PT;
        let price = widest(3, Face::Regular, SIZE_TEXT, 50 * PT) + 14 * PT;
        let amount = widest(4, Face::Regular, SIZE_TEXT, 50 * PT).max(fonts.width(
            Face::Strong,
            SIZE_TOTAL,
            &self.total.1,
        )) + 14 * PT;
        // Услуге — не меньше трети ширины, даже если суммы огромные.
        let rest = (WIDTH - number - quantity - price - amount).max(WIDTH / 3);
        [number, rest, quantity, price, amount]
    }

    /// Таблица услуг и итог.
    fn items(&self, pen: &mut Pen<'_, '_>) {
        pen.gap(22 * PT);
        let columns = self.columns(pen.fonts);
        let head = self.row_block(
            pen.fonts,
            &columns,
            &self.head,
            Face::Regular,
            SIZE_LABEL,
            Ink::Muted,
        );
        // Шапка не остаётся внизу страницы без единой строки под ней.
        let first = self.rows.first().map_or(0, |row| {
            self.row_block(
                pen.fonts,
                &columns,
                row,
                Face::Regular,
                SIZE_TEXT,
                Ink::Text,
            )
            .height
        });
        pen.room_for(head.height + first);
        pen.place(head.clone());
        for row in &self.rows {
            let block = self.row_block(
                pen.fonts,
                &columns,
                row,
                Face::Regular,
                SIZE_TEXT,
                Ink::Text,
            );
            if pen.room() < block.height {
                pen.new_page();
                pen.place(head.clone());
            }
            pen.place(block);
        }
        // Итог: подпись и сумма справа, под столбцом суммы.
        let mut block = Block::default();
        let right = MARGIN_X + WIDTH;
        let top = 10 * PT;
        let amount = pen.fonts.width(Face::Strong, SIZE_TOTAL, &self.total.1);
        block.text(
            right - amount,
            baseline(top, SIZE_TOTAL),
            SIZE_TOTAL,
            Face::Strong,
            Ink::Accent,
            self.total.1.clone(),
        );
        let label = pen.fonts.width(Face::Regular, SIZE_TEXT, &self.total.0);
        let label_x = right - columns[4] - label;
        block.text(
            label_x.max(MARGIN_X),
            baseline(top, SIZE_TOTAL),
            SIZE_TEXT,
            Face::Regular,
            Ink::Muted,
            self.total.0.clone(),
        );
        block.height = top + leading(SIZE_TOTAL);
        pen.place(block);
    }

    /// Строка таблицы: номер и услуга — слева, числа — справа в своём
    /// столбце; под строкой — линия.
    fn row_block(
        &self,
        fonts: &Fonts<'_>,
        columns: &[Pt; 5],
        row: &Row,
        face: Face,
        size: Pt,
        ink: Ink,
    ) -> Block {
        let mut block = Block::default();
        let pad = 5 * PT;
        let mut x = MARGIN_X;
        let mut height = 0;
        for (i, (cell, &width)) in row.iter().zip(columns).enumerate() {
            let lines = match i {
                1 => wrap(fonts, face, size, cell, width - 10 * PT),
                _ => vec![cell.clone()],
            };
            let mut y = pad;
            for line in lines {
                let left = match i {
                    0 | 1 => x,
                    _ => x + width - fonts.width(face, size, &line),
                };
                block.text(left, baseline(y, size), size, face, ink, line);
                y += leading(size);
            }
            height = height.max(y);
            x += width;
        }
        height += pad;
        block.marks.push(Mark::Rule(Rule {
            x1: MARGIN_X,
            y1: height,
            x2: MARGIN_X + WIDTH,
            y2: height,
            width: RULE,
        }));
        block.height = height;
        block
    }

    /// Платёжные реквизиты: подпись слева, значение справа.
    fn payment(&self, pen: &mut Pen<'_, '_>) {
        let Some((title, pairs)) = &self.payment else {
            return;
        };
        pen.gap(22 * PT);
        let mut block = Block::default();
        let mut y = 0;
        for line in wrap(pen.fonts, Face::Strong, SIZE_TEXT, title, WIDTH) {
            block.text(
                MARGIN_X,
                baseline(y, SIZE_TEXT),
                SIZE_TEXT,
                Face::Strong,
                Ink::Text,
                line,
            );
            y += leading(SIZE_TEXT);
        }
        y += 4 * PT;
        let label_width = 130 * PT;
        for (label, value) in pairs {
            // Пустая подпись — новый счёт: отступ перед ним.
            if label.is_empty() && value.is_empty() {
                y += 6 * PT;
                continue;
            }
            let labels = wrap(
                pen.fonts,
                Face::Regular,
                SIZE_LABEL,
                label,
                label_width - 8 * PT,
            );
            let values = wrap(
                pen.fonts,
                Face::Regular,
                SIZE_TEXT,
                value,
                WIDTH - label_width,
            );
            let mut ly = y;
            for line in labels {
                block.text(
                    MARGIN_X,
                    baseline(ly, SIZE_TEXT),
                    SIZE_LABEL,
                    Face::Regular,
                    Ink::Muted,
                    line,
                );
                ly += leading(SIZE_LABEL);
            }
            let mut vy = y;
            for line in values {
                block.text(
                    MARGIN_X + label_width,
                    baseline(vy, SIZE_TEXT),
                    SIZE_TEXT,
                    Face::Regular,
                    Ink::Text,
                    line,
                );
                vy += leading(SIZE_TEXT);
            }
            y = ly.max(vy);
        }
        block.height = y;
        pen.place(block);
    }

    /// У акта — слова о приёмке и подписи сторон.
    fn act(&self, pen: &mut Pen<'_, '_>) {
        let Some((statement, signs)) = &self.act else {
            return;
        };
        pen.gap(22 * PT);
        let mut block = Block::default();
        let mut y = 0;
        for line in wrap(pen.fonts, Face::Regular, SIZE_TEXT, statement, WIDTH) {
            block.text(
                MARGIN_X,
                baseline(y, SIZE_TEXT),
                SIZE_TEXT,
                Face::Regular,
                Ink::Text,
                line,
            );
            y += leading(SIZE_TEXT);
        }
        block.height = y;
        pen.place(block);

        pen.gap(30 * PT);
        let mut block = Block::default();
        let column = (WIDTH - 24 * PT) / 2;
        for (i, (sign, party)) in signs.iter().zip(&self.parties).enumerate() {
            let x = MARGIN_X + (column + 24 * PT) * Pt::try_from(i).unwrap_or(0);
            let mut y = block.label(pen.fonts, x, 0, column, sign);
            y += 26 * PT;
            block.marks.push(Mark::Rule(Rule {
                x1: x,
                y1: y,
                x2: x + column - 20 * PT,
                y2: y,
                width: RULE,
            }));
            y += 4 * PT;
            for line in wrap(pen.fonts, Face::Regular, SIZE_LABEL, &party.name, column) {
                block.text(
                    x,
                    baseline(y, SIZE_LABEL),
                    SIZE_LABEL,
                    Face::Regular,
                    Ink::Muted,
                    line,
                );
                y += leading(SIZE_LABEL);
            }
            block.height = block.height.max(y);
        }
        pen.place(block);
    }
}

/// Ключ названия вида документа.
const fn kind_key(kind: DocKind) -> Key {
    match kind {
        DocKind::Invoice => Key::KindInvoice,
        DocKind::Act => Key::KindAct,
    }
}

/// Размер логотипа в сотых пункта: вписан в [`LOGO_WIDTH`] × [`LOGO_HEIGHT`]
/// с прежними пропорциями. `None` — у картинки нулевая сторона.
fn fit(width: u32, height: u32) -> Option<(Pt, Pt)> {
    if width == 0 || height == 0 {
        return None;
    }
    let (w, h) = (i64::from(width), i64::from(height));
    let (max_w, max_h) = (i64::from(LOGO_WIDTH), i64::from(LOGO_HEIGHT));
    // Сравнение пропорций без деления: w/h против max_w/max_h.
    let (fit_w, fit_h) = if w * max_h >= h * max_w {
        (max_w, h * max_w / w)
    } else {
        (w * max_h / h, max_h)
    };
    Some((Pt::try_from(fit_w).ok()?, Pt::try_from(fit_h.max(1)).ok()?))
}

/// Текст на языке `lang`, а нет его — на первом языке, где он есть: адрес,
/// записанный только по-армянски, в английском документе лучше, чем
/// никакого.
fn localized<'s>(lang: Lang, get: impl Fn(Lang) -> &'s str) -> &'s str {
    std::iter::once(lang)
        .chain(Lang::ALL)
        .map(&get)
        .find(|text| !text.is_empty())
        .unwrap_or_default()
}

/// ИП: имя, адрес, ՀՎՀՀ, номер регистрации, телефон и почта.
fn seller_party(lang: Lang, seller: &Seller) -> Party {
    let mut lines = Vec::new();
    let address = localized(lang, |l| seller.address.get(l).as_str());
    if !address.is_empty() {
        lines.push(address.to_owned());
    }
    if let Some(tin) = &seller.tin {
        lines.push(fill(t(lang, Key::TinShort), &[tin.as_str()]));
    }
    if !seller.registration.is_empty() {
        lines.push(fill(
            t(lang, Key::PdfRegistration),
            &[seller.registration.as_str()],
        ));
    }
    let contacts: Vec<&str> = [
        seller.phone.as_str(),
        seller.email.as_ref().map_or("", |e| e.as_str()),
    ]
    .into_iter()
    .filter(|text| !text.is_empty())
    .collect();
    if !contacts.is_empty() {
        lines.push(contacts.join(DOT));
    }
    Party {
        label: t(lang, Key::PdfSeller).to_owned(),
        name: seller.name.get(lang).as_str().to_owned(),
        lines,
    }
}

/// Клиент: название, адрес, страна и налоговый номер.
fn buyer_party(lang: Lang, buyer: &ClientData) -> Party {
    let mut lines = Vec::new();
    if !buyer.address.is_empty() {
        lines.push(buyer.address.as_str().to_owned());
    }
    if let Some(country) = buyer.country {
        lines.push(country.name(lang).to_owned());
    }
    if let Some(tin) = &buyer.tin {
        lines.push(fill(t(lang, Key::TinShort), &[tin.as_str()]));
    } else if !buyer.tax_id.is_empty() {
        lines.push(fill(t(lang, Key::PdfTaxId), &[buyer.tax_id.as_str()]));
    }
    Party {
        label: t(lang, Key::PdfBuyer).to_owned(),
        name: buyer.name.as_str().to_owned(),
        lines,
    }
}

/// Счета ИП в валюте документа, а в ней нет ни одного — все (решение
/// человека 2026-10-14): иностранному клиенту драмовый счёт ни к чему,
/// но и без реквизитов документ хуже.
fn accounts(seller: &Seller, currency: Currency) -> Vec<&BankAccount> {
    let matching: Vec<&BankAccount> = seller
        .accounts
        .iter()
        .filter(|account| account.currency() == currency)
        .collect();
    if matching.is_empty() {
        seller.accounts.iter().collect()
    } else {
        matching
    }
}

/// Платёжные реквизиты: получатель и счета. `None` — счетов в профиле нет.
fn payment(
    lang: Lang,
    seller: &Seller,
    currency: Currency,
) -> Option<(String, Vec<(String, String)>)> {
    let accounts = accounts(seller, currency);
    if accounts.is_empty() {
        return None;
    }
    let label = |key: Key| t(lang, key).to_owned();
    let mut pairs = vec![(
        label(Key::PdfBeneficiary),
        seller.name.get(lang).as_str().to_owned(),
    )];
    for account in accounts {
        // Между счетами — отступ (пустая пара).
        if pairs.len() > 1 {
            pairs.push((String::new(), String::new()));
        }
        let number = if account.currency() == currency {
            account.number().as_str().to_owned()
        } else {
            format!(
                "{} ({})",
                account.number().as_str(),
                currency_code(account.currency())
            )
        };
        pairs.push((label(Key::AccountNumber), number));
        if !account.bank().is_empty() {
            pairs.push((label(Key::AccountBank), account.bank().as_str().to_owned()));
        }
        if let Some(swift) = account.swift() {
            pairs.push((label(Key::AccountSwift), swift.as_str().to_owned()));
        }
        if !account.correspondent().is_empty() {
            pairs.push((
                label(Key::AccountCorrespondent),
                account.correspondent().as_str().to_owned(),
            ));
        }
    }
    Some((label(Key::PdfPayment), pairs))
}

/// Код валюты ISO 4217 — у чужого счёта в реквизитах.
const fn currency_code(currency: Currency) -> &'static str {
    match currency {
        Currency::Amd => "AMD",
        Currency::Usd => "USD",
        Currency::Eur => "EUR",
    }
}

/// Кусок страницы: что на нём — местами от его верха — и его высота.
#[derive(Clone, Default)]
struct Block {
    marks: Vec<Mark>,
    height: Pt,
}

impl Block {
    fn text(&mut self, x: Pt, baseline: Pt, size: Pt, face: Face, ink: Ink, text: String) {
        if text.is_empty() {
            return;
        }
        self.marks.push(Mark::Text(Text {
            x,
            baseline,
            size,
            face,
            ink,
            text,
        }));
    }

    /// Подпись поля с верха `top`; возвращает верх следующей строки.
    fn label(&mut self, fonts: &Fonts<'_>, x: Pt, top: Pt, width: Pt, label: &str) -> Pt {
        let mut y = top;
        for line in wrap(fonts, Face::Regular, SIZE_LABEL, label, width) {
            self.text(
                x,
                baseline(y, SIZE_LABEL),
                SIZE_LABEL,
                Face::Regular,
                Ink::Muted,
                line,
            );
            y += leading(SIZE_LABEL);
        }
        y + 2 * PT
    }
}

/// Куда ложится следующий кусок: страницы и верх свободного места.
struct Pen<'f, 'a> {
    fonts: &'f Fonts<'a>,
    pages: Vec<Page>,
    y: Pt,
}

impl Pen<'_, '_> {
    /// Свободного места до низа набора.
    fn room(&self) -> Pt {
        BOTTOM - self.y
    }

    fn new_page(&mut self) {
        self.pages.push(Page::default());
        self.y = MARGIN_TOP;
    }

    /// Отступ — если страница не только что началась.
    fn gap(&mut self, gap: Pt) {
        if self.y > MARGIN_TOP {
            self.y += gap;
        }
    }

    /// Новая страница, если места меньше `height`, а эта не пустая.
    fn room_for(&mut self, height: Pt) {
        if self.room() < height && self.y > MARGIN_TOP {
            self.new_page();
        }
    }

    /// Кусок целиком — на эту страницу, а не помещается — на следующую.
    fn place(&mut self, block: Block) {
        self.room_for(block.height);
        let dy = self.y;
        let page = self.pages.last_mut().expect("страница есть всегда");
        page.marks
            .extend(block.marks.into_iter().map(|mark| match mark {
                Mark::Text(text) => Mark::Text(Text {
                    baseline: text.baseline + dy,
                    ..text
                }),
                Mark::Rule(rule) => Mark::Rule(Rule {
                    y1: rule.y1 + dy,
                    y2: rule.y2 + dy,
                    ..rule
                }),
                Mark::Logo(frame) => Mark::Logo(Frame {
                    y: frame.y + dy,
                    ..frame
                }),
            }));
        self.y += block.height;
    }

    /// Номера страниц внизу — если страниц больше одной.
    fn number_pages(&mut self, lang: Lang) {
        let count = self.pages.len();
        if count < 2 {
            return;
        }
        for (i, page) in self.pages.iter_mut().enumerate() {
            let text = fill(
                t(lang, Key::PdfPage),
                &[
                    &i18n::integer(lang, i as i64 + 1),
                    &i18n::integer(lang, count as i64),
                ],
            );
            let width = self.fonts.width(Face::Regular, SIZE_LABEL, &text);
            page.marks.push(Mark::Text(Text {
                x: MARGIN_X + WIDTH - width,
                baseline: PAGE_HEIGHT - MARGIN_BOTTOM / 2,
                size: SIZE_LABEL,
                face: Face::Regular,
                ink: Ink::Muted,
                text,
            }));
        }
    }
}
