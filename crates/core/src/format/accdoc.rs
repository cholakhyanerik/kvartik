//! Файл для системы расчётных документов ՊԵԿ (задача 44): черновик
//! «Ծառայությունների մատուցման, աշխատանքների կատարման հաշիվ վավերագիր» по
//! выпущенному документу дохода — XML по схеме ՊԵԿ `accdoc.xsd`. Человек
//! вводит сделку один раз, у нас, а в системе ՊԵԿ проверяет и выписывает.
//!
//! # Откуда схема
//!
//! Пакет «API-ի, XSD-ների և XML-ների տվյալներ» на src.am
//! (`getMenusContents/111`, архив `invoice_package_6982e94737782.zip`,
//! `accdoc.xsd` от 2025-09-25; сверено 2026-11-16). Вид документа — `3`:
//! так он записан в файле, выгруженном из системы, со строками услуг
//! (GitHub `ArsenMkrtchyan/smart`, `public/export.xml`, 2024-12-14), а у
//! перевозки в образце пакета — `2`; у самой схемы номера без слов (разведка
//! 2026-11-16). Схема без версии и даты: новую видно только по новому
//! архиву, и меняет её ՊԵԿ без нас (Правило 6).
//!
//! # Как его загружают
//!
//! Человек загружает файл сам: подраздел документов об услугах, «Սևագրեր»,
//! «Ներբեռնել». Система проверяет, что вид в файле — вид подраздела, и кладёт
//! черновик, который человек проверяет и выписывает там же (руководство
//! e-invoicing, 5.1.3; предел — 500 документов и 10 МБ). Программа файл
//! никуда не отправляет (Архитектура, «Сеть — только к своему сайту»).
//!
//! # Что в нём
//!
//! - День поставки — день оказания услуги. Поля сделки (`DealInfo`) — это
//!   договор (руководство, 3.5.1), а не документ программы, и они пустые.
//! - Поставщик и покупатель — копии реквизитов на день выпуска, по-армянски.
//!   У фирмы другой страны ՀՎՀՀ нет — `TinNotRequired`, а её налоговый номер
//!   — в `AdditionalData`. Покупатель без страны и без ՀՎՀՀ — не «ՀՎՀՀ не
//!   обязателен», а «нет значения»: файла нет (Правило 6).
//! - Строки — услуги документа: единица — «հատ» (у строк программы единицы
//!   нет, а система её требует; так её пишут и выгруженные документы, и в
//!   черновике её меняют), количество, цена за единицу в драмах — пять знаков
//!   после точки, — и стоимость строки — два. Вид сделки (`DealType`) не
//!   пишется: «Հարկման հատուկ համակարգ» или «ԱԱՀ չվճարողներ» у ИП на налоге
//!   с оборота — толкование, а ՊԵԿ прямо не пишет ни того, ни другого
//!   (разведка и ревью задачи 44, 2026-11-16), и это решение человека. Поле
//!   обязательно, и система проверяет его при подписи (руководство, 3.5.1),
//!   так что человек выбирает его в черновике.
//! - Суммы — в драмах: поля валюты у документа нет. Документ в валюте — по
//!   своему курсу (`DocumentData::rate`), как его доход в книге: строка —
//!   `Money::to_lumas`, цена — до стотысячной драма тем же округлением, а
//!   разницу округления строк с доходом документа (`total_lumas`, один курс
//!   на итог) берёт последняя строка: итог файла — доход в книге. Стоимость
//!   строки система пересчитывает из количества и цены сама и правит в
//!   пределах 10 драмов (руководство, 3.5.2). Без курса файла нет; у услуги
//!   в валюте до 2026-07-01 — тоже: тогда курс брался на день выписки
//!   расчётного документа (`tax::rules::RateDay`), а не тот, что в книге.
//! - Только об услугах. Строка налога — торговля, вторсырьё, газеты,
//!   производство, а у самого дохода — продажа имущества: документ о товарах,
//!   и его выписывают в системе сами. Строка неизвестна — кодов нет, или
//!   прочие доходы заявителя общепита (строка 7 не говорит, товар это или
//!   услуга): не «услуги по умолчанию», а «нет значения» (Правило 6).
//! - Только на особом режиме. Код документа закрывает налог с оборота или,
//!   в году заявления о микропредпринимательстве, его (`turnover::excluded`,
//!   `micro_excluded`): с этого дня — к бухгалтеру, и файла нет. Переход порога
//!   года программа узнаёт по книге, а её здесь нет: о нём окно говорит словами.

use std::fmt::Write as _;

use crate::model::activity::IncomeKind;
use crate::model::country::Country;
use crate::model::bank::BankAccount;
use crate::model::document::{DocKind, Document, DocumentData, Item};
use crate::model::ledger::{Flow, Row, Source};
use crate::model::money::{Currency, Lumas, MoneyError, div_round_half_away_from_zero};
use crate::model::profile::{Localized, ProfileData};
use crate::model::text::{Line, Lines};
use crate::tax::{rules, turnover};

/// Вид документа об услугах в схеме (`AccDocTypeType`).
const SERVICES: &str = "3";

/// Единица строки, когда своей нет (шапка модуля).
const UNIT: &str = "հատ";

/// Пределы текста схемы, в знаках (`StringInputType`, `GoodName`,
/// `CompanyNameType`).
const MAX_TEXT: usize = 256;
const MAX_NAME: usize = 512;

/// Что в документе длиннее, чем принимает система.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Field {
    /// Название ИП.
    SellerName,
    /// Адрес ИП.
    SellerAddress,
    /// Банк или номер счёта ИП.
    SellerBank,
    /// Название покупателя.
    BuyerName,
    /// Адрес покупателя.
    BuyerAddress,
    /// Налоговый номер покупателя другой страны.
    BuyerTaxId,
    /// Услуга в строке документа.
    Service,
}

/// Почему файла нет.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AccDocError {
    /// Документ не выпущен или аннулирован.
    NotIssued,
    /// Документ о товарах (шапка модуля).
    Goods,
    /// Строку налога не узнать: товар это или услуга, неизвестно.
    KindUnknown,
    /// Код документа закрывает особый режим: с дня услуги — к бухгалтеру.
    Closed,
    /// Сумма в валюте без курса: сумм в драмах нет.
    NoRate,
    /// Услуга в валюте до 2026-07-01: курс — на день выписки расчётного
    /// документа, а не курс книги.
    OldRate,
    /// В реквизитах ИП на день выпуска нет ՀՎՀՀ.
    NoSellerTin,
    /// У армянской фирмы нет ՀՎՀՀ.
    NoBuyerTin,
    /// У покупателя нет ни страны, ни ՀՎՀՀ: обязателен ли ՀՎՀՀ, неизвестно.
    BuyerUnknown,
    /// Текст длиннее предела схемы.
    TooLong(Field),
    /// Сумма не помещается — только от испорченных данных.
    Overflow,
}

impl From<MoneyError> for AccDocError {
    fn from(error: MoneyError) -> Self {
        match error {
            MoneyError::RateUnknown => Self::NoRate,
            MoneyError::CurrencyMismatch | MoneyError::Overflow => Self::Overflow,
        }
    }
}

/// Имя файла — латиницей, одно на всех языках: `accdoc-Act-2026-003.xml`.
/// `None` — документ не выпущен.
pub fn file_name(document: &Document) -> Option<String> {
    let number = document.number()?;
    let kind = match document.data().kind {
        DocKind::Invoice => "Invoice",
        DocKind::Act => "Act",
    };
    Some(format!("accdoc-{kind}-{number}.xml"))
}

/// Файл черновика расчётного документа по выпущенному действующему документу
/// `document` (шапка модуля). `profile` — профиль сейчас: по нему узнаётся
/// строка налога, а реквизиты — из копии на день выпуска.
pub fn accdoc(document: &Document, profile: Option<&ProfileData>) -> Result<String, AccDocError> {
    let issued = document.issued().ok_or(AccDocError::NotIssued)?;
    if document.annulled().is_some() {
        return Err(AccDocError::NotIssued);
    }
    let data = document.data();
    supply(document, profile)?;
    let seller = issued.seller();
    let buyer = issued.buyer();
    let seller_tin = seller.tin.as_ref().ok_or(AccDocError::NoSellerTin)?;
    match (buyer.country, &buyer.tin) {
        (Some(country), None) if country == Country::ARMENIA => return Err(AccDocError::NoBuyerTin),
        (None, None) => return Err(AccDocError::BuyerUnknown),
        _ => {}
    }
    match data.currency {
        Currency::Amd => {}
        Currency::Usd | Currency::Eur => {
            if rules::on(rules::armenia().rate_day(), data.service_date).is_none() {
                return Err(AccDocError::OldRate);
            }
        }
    }

    // Строки: стоимость каждой — по курсу документа, а разницу округления
    // с доходом документа берёт последняя (шапка модуля).
    let mut lines = Vec::with_capacity(data.items.len());
    for item in &data.items {
        let (price_e5, value) = amounts(item, data)?;
        let description = fits(item.text.as_str(), MAX_TEXT, Field::Service)?;
        lines.push((item, description, price_e5, i128::from(value.get())));
    }
    let total = i128::from(data.total_lumas()?.get());
    let sum: i128 = lines.iter().map(|(.., value)| value).sum();
    if let Some(last) = lines.last_mut() {
        last.3 += total - sum;
    }
    let mut goods_xml = String::new();
    for (item, description, price_e5, value) in &lines {
        goods_xml.push_str("<Good>");
        tag(&mut goods_xml, "Description", description);
        tag(&mut goods_xml, "Unit", UNIT);
        tag(&mut goods_xml, "Amount", &decimal(i128::from(item.quantity_e3), 3));
        tag(&mut goods_xml, "PricePerUnit", &decimal(*price_e5, 5));
        tag(&mut goods_xml, "Price", &decimal(*value, 2));
        tag(&mut goods_xml, "TotalPrice", &decimal(*value, 2));
        goods_xml.push_str("</Good>");
    }

    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str("<ExportedAccDocData xmlns=\"http://www.taxservice.am/tp3/invoice/definitions\">");
    out.push_str("<AccountingDocument Version=\"1.0\">");
    tag(&mut out, "Type", SERVICES);
    out.push_str("<GeneralInfo>");
    tag(&mut out, "DeliveryDate", &data.service_date.iso());
    tag(&mut out, "Procedure", "1");
    out.push_str("</GeneralInfo>");

    // Поставщик: порядок элементов `AccDocTaxpayerInfo` — последовательность
    // схемы (`xs:sequence`), а не любой.
    out.push_str("<SupplierInfo><Taxpayer>");
    tag(&mut out, "TIN", seller_tin.as_str());
    tag(&mut out, "Name", fits(armenian_first(&seller.name), MAX_NAME, Field::SellerName)?);
    let address = joined(armenian_lines(&seller.address));
    if !address.is_empty() {
        tag(&mut out, "Address", fits(&address, MAX_TEXT, Field::SellerAddress)?);
    }
    if let Some(account) = account(&seller.accounts, data.currency) {
        out.push_str("<BankAccount>");
        if !account.bank().is_empty() {
            tag(&mut out, "BankName", fits(account.bank().as_str(), MAX_TEXT, Field::SellerBank)?);
        }
        tag(&mut out, "BankAccountNumber", fits(account.number().as_str(), MAX_TEXT, Field::SellerBank)?);
        out.push_str("</BankAccount>");
    }
    out.push_str("</Taxpayer></SupplierInfo>");

    out.push_str("<BuyerInfo><Taxpayer>");
    if let Some(tin) = &buyer.tin {
        tag(&mut out, "TIN", tin.as_str());
    }
    tag(&mut out, "Name", fits(buyer.name.as_str(), MAX_NAME, Field::BuyerName)?);
    let address = joined(&buyer.address);
    if !address.is_empty() {
        tag(&mut out, "Address", fits(&address, MAX_TEXT, Field::BuyerAddress)?);
    }
    if buyer.tin.is_none() && !buyer.tax_id.is_empty() {
        tag(&mut out, "AdditionalData", fits(buyer.tax_id.as_str(), MAX_TEXT, Field::BuyerTaxId)?);
    }
    tag(&mut out, "TinNotRequired", if buyer.tin.is_none() { "true" } else { "false" });
    out.push_str("</Taxpayer></BuyerInfo>");

    out.push_str("<GoodsInfo>");
    out.push_str(&goods_xml);
    out.push_str("<Total>");
    tag(&mut out, "TotalPrice", &decimal(total, 2));
    out.push_str("</Total>");
    out.push_str("</GoodsInfo>");
    out.push_str("</AccountingDocument></ExportedAccDocData>\n");
    Ok(out)
}

/// Документ об услугах на особом режиме — или почему нет (шапка модуля).
/// Код, который решает режим, и строку налога решает тот же расчёт, что
/// налог (`turnover::regime_code`, `turnover::income_kind`): второй расчёт
/// однажды разошёлся бы с первым.
fn supply(document: &Document, profile: Option<&ProfileData>) -> Result<(), AccDocError> {
    let data = document.data();
    let day = data.service_date;
    let row = Row {
        source: Source::Document(document),
        flow: Flow::Income,
        date: day,
        amount: document.total()?,
        rate: data.rate,
        activity: data.activity,
        income_kind: data.income_kind,
    };
    if let Some(code) = turnover::regime_code(profile, &row) {
        // В году заявления о микропредпринимательстве — его исключения
        // (НК, ст. 267, ч. 5, п. 2.1), иначе — налога с оборота (ст. 254).
        let closes = if profile.is_some_and(|profile| profile.micro_in(day.year())) {
            turnover::micro_excluded(&code, day)
        } else {
            turnover::excluded(&code, day)
        };
        if closes.is_some() {
            return Err(AccDocError::Closed);
        }
    }
    // Строка, выбранная у самого дохода, говорит сама: продажа имущества —
    // товар, аренда, проценты и роялти — услуга, и у заявителя общепита тоже,
    // хоть налог и берёт её там строкой 7.
    let kind = match data.income_kind {
        Some(kind) => Some(kind),
        None => turnover::income_kind(profile, &row),
    };
    match kind {
        Some(IncomeKind::Rent | IncomeKind::Catering | IncomeKind::HighTech | IncomeKind::Other) => Ok(()),
        Some(
            IncomeKind::Trade
            | IncomeKind::Recyclables
            | IncomeKind::Newspapers
            | IncomeKind::Production
            | IncomeKind::Assets,
        ) => Err(AccDocError::Goods),
        Some(IncomeKind::CateringOther) | None => Err(AccDocError::KindUnknown),
    }
}

/// Цена за единицу в стотысячных драма и стоимость строки в драмах
/// с лумами. Драмы — как есть; валюта — по курсу документа.
fn amounts(item: &Item, data: &DocumentData) -> Result<(i128, Lumas), AccDocError> {
    let value = item.total(data.currency)?.to_lumas(data.rate)?;
    let price_e5 = match data.currency {
        Currency::Amd => i128::from(item.price) * 100_000,
        Currency::Usd | Currency::Eur => {
            let rate = data.rate.ok_or(AccDocError::NoRate)?;
            // Цена в центах на драмы за единицу в десятитысячных — миллионные
            // доли драма; до стотысячных — тем же округлением, что перевод
            // в драмы (`model::money`).
            div_round_half_away_from_zero(i128::from(item.price) * i128::from(rate.per_unit_e4()), 10)
        }
    };
    Ok((price_e5, value))
}

/// Счёт ИП для документа: в валюте документа, иначе в драмах, иначе первый.
fn account(accounts: &[BankAccount], currency: Currency) -> Option<&BankAccount> {
    accounts
        .iter()
        .find(|account| account.currency() == currency)
        .or_else(|| accounts.iter().find(|account| account.currency() == Currency::Amd))
        .or_else(|| accounts.first())
}

/// Армянская запись, а нет её — русская, потом английская.
fn armenian_first(text: &Localized<Line>) -> &str {
    [&text.hy, &text.ru, &text.en]
        .into_iter()
        .find(|line| !line.is_empty())
        .map_or("", |line| line.as_str())
}

/// То же для адреса.
fn armenian_lines(text: &Localized<Lines>) -> &Lines {
    [&text.hy, &text.ru, &text.en]
        .into_iter()
        .find(|lines| !lines.as_str().is_empty())
        .unwrap_or(&text.hy)
}

/// Адрес в одну строку: строки — через запятую, а своя запятая или точка
/// с запятой в конце строки не удваивается.
fn joined(lines: &Lines) -> String {
    lines
        .as_str()
        .split('\n')
        .map(|part| part.trim().trim_end_matches([',', ';']).trim_end())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Текст в пределе схемы, в знаках.
fn fits(text: &str, max: usize, field: Field) -> Result<&str, AccDocError> {
    if text.chars().count() > max { Err(AccDocError::TooLong(field)) } else { Ok(text) }
}

/// Элемент с текстом: знаки разметки — сущностями, а знаки, которых XML 1.0
/// не допускает (U+FFFE, U+FFFF), — знаком замены. Управляющих знаков в
/// тексте программы нет (`model::text`).
fn tag(out: &mut String, name: &str, text: &str) {
    let _ = write!(out, "<{name}>");
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\u{FFFE}' | '\u{FFFF}' => out.push('\u{FFFD}'),
            c => out.push(c),
        }
    }
    let _ = write!(out, "</{name}>");
}

/// Число `scaled`, умноженное на 10^`places`, — записью `xs:decimal` без
/// лишних нулей справа: `931200`, `0.125`, `3.60544`.
fn decimal(scaled: i128, places: u32) -> String {
    let scale = 10_i128.pow(places);
    let sign = if scaled < 0 { "-" } else { "" };
    let whole = (scaled / scale).unsigned_abs();
    let mut fraction = (scaled % scale).unsigned_abs();
    let mut digits = places;
    while digits > 0 && fraction.is_multiple_of(10) {
        fraction /= 10;
        digits -= 1;
    }
    if digits == 0 {
        format!("{sign}{whole}")
    } else {
        format!("{sign}{whole}.{fraction:0width$}", width = digits as usize)
    }
}

#[cfg(test)]
mod tests;
