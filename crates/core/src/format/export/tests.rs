//! Тесты выгрузки. Ожидаемые байты, номера дней Excel и контрольные суммы
//! выписаны руками, а не посчитаны тем же кодом (Правило 2).

use super::*;
use crate::model::activity::ActivityCode;
use crate::model::client::ClientData;
use crate::model::country::Country;
use crate::model::date::Quarter;
use crate::model::document::{DocumentData, Item, Payment, Settlement, SettlementKind};
use crate::model::ledger::OperationData;
use crate::model::money::{ExchangeRate, RateSource};
use crate::model::profile::ProfileData;
use crate::model::record::Timestamp;
use crate::model::text::{Line, Lines};
use crate::tax::turnover::Books;

fn day(year: u16, month: u8, day: u8) -> Date {
    Date::new(year, month, day).expect("в тесте — существующая дата")
}

fn at(millis: i64) -> Timestamp {
    Timestamp::from_unix_millis(millis)
}

fn line(text: &str) -> Line {
    Line::new(text).expect("в тесте — годный текст")
}

fn text(value: &str) -> Cell {
    Cell::Text(value.to_owned())
}

fn number(minor: i64, places: u8) -> Cell {
    Cell::Number { minor, places }
}

#[test]
fn decimal_writes_digits_with_a_point() {
    let mut wrong = Vec::new();
    for (minor, places, want) in [
        (123_456, 2, "1234.56"),
        (-5, 2, "-0.05"),
        (0, 2, "0.00"),
        (100, 2, "1.00"),
        (7, 0, "7"),
        (-60_000, 0, "-60000"),
        (3_874_500, 4, "387.4500"),
        (1, 4, "0.0001"),
        (i64::MIN, 2, "-92233720368547758.08"),
    ] {
        let seen = decimal(minor, places);
        if seen != want {
            wrong.push(format!("{minor}/{places}: {seen}, а не {want}"));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// CSV по RFC 4180: BOM, строки через CRLF, текст в кавычках с удвоенной
/// кавычкой внутри, дробная часть — через точку, даты — ISO; текст,
/// похожий на формулу, — с апострофом, а минус числа — нет.
#[test]
fn csv_is_rfc_4180_with_bom_and_guarded_formulas() {
    let sheet = Sheet {
        name: "Книга".to_owned(),
        columns: vec!["Дата".to_owned(), "За что".to_owned(), "Сумма".to_owned(), "Курс".to_owned()],
        rows: vec![
            vec![Cell::Date(day(2026, 11, 3)), text("Сайт, «Ереван»"), number(150_000, 2), Cell::Empty],
            vec![Cell::Date(day(2026, 11, 4)), text("Он сказал \"да\""), number(-60_000, 0), number(3_874_500, 4)],
            vec![Cell::Empty, text("=HYPERLINK(\"x\")"), Cell::Empty, Cell::Empty],
            vec![Cell::Empty, text("+1"), Cell::Empty, Cell::Empty],
            vec![Cell::Empty, text("-2"), Cell::Empty, Cell::Empty],
            vec![Cell::Empty, text("@SUM(A1)"), Cell::Empty, Cell::Empty],
            vec![Cell::Empty, text("\tтаб"), Cell::Empty, Cell::Empty],
            vec![Cell::Empty, text("\rвозврат"), Cell::Empty, Cell::Empty],
            vec![Cell::Empty, text("a = b"), Cell::Empty, Cell::Empty],
            vec![Cell::Empty, text("ООО Ромашка;=1+1;"), Cell::Empty, Cell::Empty],
            vec![Cell::Empty, text("a; -2;\"@x"), Cell::Empty, Cell::Empty],
            vec![Cell::Empty, text("Development; Hosting; 3+4"), Cell::Empty, Cell::Empty],
            vec![Cell::Empty, text("  =1"), Cell::Empty, Cell::Empty],
        ],
    };
    let want = concat!(
        "\u{FEFF}",
        "\"Дата\",\"За что\",\"Сумма\",\"Курс\"\r\n",
        "2026-11-03,\"Сайт, «Ереван»\",1500.00,\r\n",
        "2026-11-04,\"Он сказал \"\"да\"\"\",-60000,387.4500\r\n",
        ",\"'=HYPERLINK(\"\"x\"\")\",,\r\n",
        ",\"'+1\",,\r\n",
        ",\"'-2\",,\r\n",
        ",\"'@SUM(A1)\",,\r\n",
        ",\"'\tтаб\",,\r\n",
        ",\"'\rвозврат\",,\r\n",
        ",\"a = b\",,\r\n",
        ",\"ООО Ромашка;'=1+1;\",,\r\n",
        ",\"a; '-2;\"\"'@x\",,\r\n",
        ",\"Development; Hosting; 3+4\",,\r\n",
        ",\"  '=1\",,\r\n",
    );
    assert_eq!(String::from_utf8(csv(&sheet)).expect("UTF-8"), want);
}

#[test]
fn crc32_matches_the_check_value() {
    // Контрольное значение CRC-32/ISO-HDLC для «123456789» — 0xCBF43926
    // (каталог Rocksoft™ Model CRC, «CRC-32»).
    assert_eq!(zip::crc32(b"123456789"), 0xCBF4_3926);
    assert_eq!(zip::crc32(b""), 0);
}

/// Читает ZIP так, как читает его любой распаковщик: по местным заголовкам
/// и центральному каталогу. Возвращает имена и байты файлов.
fn unzip(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    let u16_at = |at: usize| u16::from_le_bytes([bytes[at], bytes[at + 1]]);
    let u32_at = |at: usize| u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
    let end = bytes.len() - 22;
    assert_eq!(u32_at(end), 0x0605_4b50, "нет конца каталога");
    let count = usize::from(u16_at(end + 10));
    let mut central = u32_at(end + 16) as usize;
    let mut files = Vec::new();
    for _ in 0..count {
        assert_eq!(u32_at(central), 0x0201_4b50, "нет записи каталога");
        let crc = u32_at(central + 16);
        let size = u32_at(central + 24) as usize;
        let name_len = usize::from(u16_at(central + 28));
        let local = u32_at(central + 42) as usize;
        let name = String::from_utf8(bytes[central + 46..central + 46 + name_len].to_vec()).expect("имя");
        assert_eq!(u32_at(local), 0x0403_4b50, "{name}: нет местного заголовка");
        assert_eq!(u16_at(local + 8), 0, "{name}: сжат");
        assert_eq!(u32_at(local + 14), crc, "{name}: CRC в заголовках разные");
        let local_name_len = usize::from(u16_at(local + 26));
        let start = local + 30 + local_name_len + usize::from(u16_at(local + 28));
        let data = bytes[start..start + size].to_vec();
        assert_eq!(zip::crc32(&data), crc, "{name}: CRC не тот");
        files.push((name, data));
        central += 46 + name_len;
    }
    files
}

/// XLSX — ZIP без сжатия из частей пакета в нужном порядке; в листе
/// заголовок жирным, дата — номером дня Excel, числа — числами с нужным
/// числом знаков, текст — строкой в ячейке, похожий на формулу — со стилем
/// `quotePrefix`, служебные знаки XML — сущностями.
#[test]
fn xlsx_is_a_stored_zip_of_the_package_parts() {
    let sheet = Sheet {
        name: "Книга".to_owned(),
        columns: vec!["Дата".to_owned(), "Текст".to_owned(), "Сумма".to_owned(), "Курс".to_owned()],
        rows: vec![
            vec![Cell::Date(day(2026, 11, 3)), text("A & B <Ереван>"), number(123_456, 2), number(3_874_500, 4)],
            vec![Cell::Date(day(2000, 1, 1)), text("=1+1"), number(60_000, 0), Cell::Empty],
            vec![Cell::Empty, text("знак\u{1}"), Cell::Empty, Cell::Empty],
            vec![Cell::Empty, text("_x003D_1+1 и a_x41_b и _xZZZZ_ и _x0041z"), Cell::Empty, Cell::Empty],
        ],
    };
    let other = Sheet {
        name: "Документы".to_owned(),
        columns: vec!["Номер".to_owned()],
        rows: Vec::new(),
    };
    let files = unzip(&xlsx(&[sheet, other]));
    let names: Vec<&str> = files.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(
        names,
        [
            "[Content_Types].xml",
            "_rels/.rels",
            "xl/workbook.xml",
            "xl/_rels/workbook.xml.rels",
            "xl/styles.xml",
            "xl/worksheets/sheet1.xml",
            "xl/worksheets/sheet2.xml",
        ]
    );
    let part = |name: &str| {
        let (_, data) = files.iter().find(|(file, _)| file == name).expect("часть есть");
        String::from_utf8(data.clone()).expect("UTF-8")
    };
    let workbook = part("xl/workbook.xml");
    assert!(workbook.contains(r#"<sheet name="Книга" sheetId="1" r:id="rId1"/>"#), "{workbook}");
    assert!(workbook.contains(r#"<sheet name="Документы" sheetId="2" r:id="rId2"/>"#), "{workbook}");
    let types = part("[Content_Types].xml");
    assert!(types.contains(r#"PartName="/xl/worksheets/sheet2.xml""#), "{types}");
    let rels = part("xl/_rels/workbook.xml.rels");
    assert!(rels.contains(r#"Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles""#), "{rels}");

    let first = part("xl/worksheets/sheet1.xml");
    let mut wrong = Vec::new();
    for piece in [
        r#"<pane ySplit="1" topLeftCell="A2" activePane="bottomLeft" state="frozen"/>"#,
        r#"<c r="A1" s="5" t="inlineStr"><is><t xml:space="preserve">Дата</t></is></c>"#,
        // 2026-11-03 — день 46329 Excel, 2000-01-01 — 36526.
        r#"<c r="A2" s="1"><v>46329</v></c>"#,
        r#"<c r="A3" s="1"><v>36526</v></c>"#,
        r#"<c r="B2" t="inlineStr"><is><t xml:space="preserve">A &amp; B &lt;Ереван&gt;</t></is></c>"#,
        r#"<c r="C2" s="3"><v>1234.56</v></c>"#,
        r#"<c r="D2" s="4"><v>387.4500</v></c>"#,
        r#"<c r="B3" s="6" t="inlineStr"><is><t xml:space="preserve">=1+1</t></is></c>"#,
        r#"<c r="C3" s="2"><v>60000</v></c>"#,
        "<t xml:space=\"preserve\">знак\u{FFFD}</t>",
        r#"<row r="4">"#,
        // Не `=`, а запись как есть: без `quotePrefix` формулой её не сделать.
        r#"<c r="B5" t="inlineStr"><is><t xml:space="preserve">_x005F_x003D_1+1 и a_x41_b и _xZZZZ_ и _x0041z</t></is></c>"#,
    ] {
        if !first.contains(piece) {
            wrong.push(piece);
        }
    }
    assert!(wrong.is_empty(), "нет в листе: {wrong:#?}\n{first}");
    assert!(!first.contains("D3"), "пустая ячейка записана");
}

#[test]
fn sheet_and_column_names_follow_excel_rules() {
    let columns: Vec<String> = [0, 25, 26, 27, 701, 702].into_iter().map(xlsx::column).collect();
    assert_eq!(columns, ["A", "Z", "AA", "AB", "ZZ", "AAA"]);
    assert_eq!(xlsx::sheet_name("Книга [Q3]: 2026/27?"), "Книга  Q3   2026 27");
    assert_eq!(xlsx::sheet_name(&"Ա".repeat(40)).chars().count(), 31);
    assert_eq!(xlsx::sheet_name("[]"), "1");
    assert_eq!(xlsx::serial(day(1900, 3, 1)), 61);
    assert_eq!(xlsx::clip(&"ա".repeat(40_000)).chars().count(), 32_767);
    assert_eq!(xlsx::clip(&"😀".repeat(20_000)).chars().count(), 16_383);
    assert_eq!(xlsx::clip("Գումարը"), "Գումարը");
}

fn client() -> Client {
    let data = ClientData {
        name: line("Beispiel GmbH"),
        country: Country::from_code("DE"),
        address: Lines::default(),
        tin: None,
        tax_id: line("DE123456789"),
        currency: Some(Currency::Usd),
        lang: Some(Lang::En),
        payment_days: None,
    };
    Client::create(data, at(1_000)).expect("годный клиент")
}

fn profile() -> ProfileData {
    let mut data = ProfileData::default();
    data.name.en = line("IE Aram Petrosyan");
    data
}

/// Счёт 1 000,00 $ за услугу `service`, курс 388,0000 ֏.
fn invoice_data(client: &Client, date: Date, service: Date) -> DocumentData {
    DocumentData {
        kind: DocKind::Invoice,
        client: client.id(),
        lang: Lang::En,
        currency: Currency::Usd,
        date,
        service_date: service,
        due: Some(date),
        items: vec![
            Item { text: line("Development"), quantity_e3: 1_000, price: 60_000 },
            Item { text: line("Hosting"), quantity_e3: 1_000, price: 40_000 },
        ],
        basis: None,
        recurring: None,
        rate: Some(ExchangeRate::new(Currency::Usd, 3_880_000, service, RateSource::Manual).expect("курс")),
        activity: None,
        income_kind: None,
    }
}

fn issue(data: DocumentData, client: &Client, documents: &[Document]) -> Document {
    Document::create(data, at(2_000))
        .expect("годный черновик")
        .issue(documents, 0, Some(&profile()), Some(None), client, at(3_000))
        .expect("выпускается")
}

/// Расход 60 000 ֏ — аренда за октябрь, «հաշիվ վավերագիր», код 62.01.0.
const EXPENSE: &str = concat!(
    r#"{"meta":{"id":"6e2d3c4b-5a6f-4e7d-9c8b-0a9f8e7d6c5b","modified":1760000000500,"deleted":null},"#,
    r#""data":{"flow":"expense","date":"2026-10-31","paid":"2026-11-02","#,
    r#""amount":{"minor":60000,"currency":"AMD"},"rate":null,"#,
    r#""text":"=Аренда офиса","client":null,"#,
    r#""supplier":"«Արարատ» ՍՊԸ","proof":"settlement_invoice","nature":"ordinary","cost":"overhead","activity":"62.01.0","income_kind":null,"#,
    r#""attachments":[]}}"#
);

/// Книга периода: доход из выпущенного счёта — его номер, клиент, строки,
/// сумма, курс, драмы и день последней оплаты, когда оплачен целиком; доход
/// по счёту с актом — акт, а день оплаты — у его счёта; запись
/// расхода — продавец, документ продавца, вид, прямой ли он, код и день
/// оплаты. Строки — по
/// дню, а то, что вне периода, не попадает.
#[test]
fn ledger_sheet_lists_documents_and_records() {
    let client = client();
    let october = day(2026, 10, 10);
    let mut documents = Vec::new();
    let paid = issue(invoice_data(&client, october, october), &client, &documents)
        .add_payment(Payment { date: day(2026, 10, 15), amount: 40_000 }, at(4_000))
        .expect("оплата")
        .add_payment(Payment { date: day(2026, 10, 20), amount: 60_000 }, at(5_000))
        .expect("оплата");
    documents.push(paid);
    // Вне периода — в следующем квартале.
    let later = issue(invoice_data(&client, day(2027, 1, 5), day(2027, 1, 5)), &client, &documents);
    documents.push(later);
    // Счёт 2026-002 оплачен целиком, а доход по нему — акт.
    let invoiced = issue(invoice_data(&client, day(2026, 10, 25), day(2026, 10, 25)), &client, &documents)
        .add_payment(Payment { date: day(2026, 10, 28), amount: 100_000 }, at(4_000))
        .expect("оплата");
    documents.push(invoiced.clone());
    let act = issue(invoiced.act_data(day(2026, 10, 26)).expect("акт по счёту"), &client, &documents);
    documents.push(act);
    // Счёт 2026-003 оплачен не целиком: дня оплаты у дохода нет.
    let partly = issue(invoice_data(&client, day(2026, 11, 5), day(2026, 11, 5)), &client, &documents)
        .add_payment(Payment { date: day(2026, 11, 6), amount: 40_000 }, at(4_000))
        .expect("оплата");
    documents.push(partly);
    let expense: Operation = serde_json::from_str(EXPENSE).expect("расход читается");
    let lang = Lang::Ru;
    let sheet = ledger_sheet(lang, &documents, &[expense], &[client], day(2026, 10, 1), day(2026, 12, 31));
    assert_eq!(sheet.columns.len(), 15);
    assert_eq!(sheet.columns[0], "Дата оказания или получения");
    assert_eq!(sheet.columns[7], "Прямой или сбытовой и административный");
    assert_eq!(
        sheet.rows,
        [
            vec![
                Cell::Date(october),
                text("Доход"),
                text("Счёт №\u{A0}2026-001"),
                text("Beispiel GmbH"),
                text("Development; Hosting"),
                Cell::Empty,
                Cell::Empty,
                Cell::Empty,
                Cell::Empty,
                Cell::Empty,
                number(100_000, 2),
                text("USD"),
                number(3_880_000, 4),
                // 1 000,00 $ по 388 ֏ — 388 000,00 ֏, в лумах.
                number(38_800_000, 2),
                Cell::Date(day(2026, 10, 20)),
            ],
            vec![
                Cell::Date(day(2026, 10, 25)),
                text("Доход"),
                text("Акт сдачи-приёмки №\u{A0}2026-001"),
                text("Beispiel GmbH"),
                text("Development; Hosting"),
                Cell::Empty,
                Cell::Empty,
                Cell::Empty,
                Cell::Empty,
                Cell::Empty,
                number(100_000, 2),
                text("USD"),
                number(3_880_000, 4),
                number(38_800_000, 2),
                Cell::Date(day(2026, 10, 28)),
            ],
            vec![
                Cell::Date(day(2026, 10, 31)),
                text("Расход"),
                Cell::Empty,
                text("«Արարատ» ՍՊԸ"),
                text("=Аренда офиса"),
                text("Счёт-фактура (հաշիվ վավերագիր)"),
                text("Обычный: товары, материалы, услуги"),
                text("Сбытовой и административный"),
                text("62.01.0"),
                Cell::Empty,
                number(60_000, 0),
                text("AMD"),
                Cell::Empty,
                number(6_000_000, 2),
                Cell::Date(day(2026, 11, 2)),
            ],
            vec![
                Cell::Date(day(2026, 11, 5)),
                text("Доход"),
                text("Счёт №\u{A0}2026-003"),
                text("Beispiel GmbH"),
                text("Development; Hosting"),
                Cell::Empty,
                Cell::Empty,
                Cell::Empty,
                Cell::Empty,
                Cell::Empty,
                number(100_000, 2),
                text("USD"),
                number(3_880_000, 4),
                number(38_800_000, 2),
                Cell::Empty,
            ],
        ]
    );
}

/// Расход, отнесённый к строке дохода (задача 95), — в столбце строки налога,
/// как доход со своей строкой, а кода и «прямой ли» у него нет: бухгалтер
/// видит, почему расход не в вычете.
#[test]
fn an_expense_of_an_income_row_shows_its_row() {
    let text_of = EXPENSE.replacen(
        r#""cost":"overhead","activity":"62.01.0","income_kind":null"#,
        r#""cost":null,"activity":null,"income_kind":"rent""#,
        1,
    );
    assert_ne!(text_of, EXPENSE, "подмена не сработала");
    let expense: Operation = serde_json::from_str(&text_of).expect("расход строки дохода читается");
    let sheet = ledger_sheet(Lang::Ru, &[], &[expense], &[], day(2026, 10, 1), day(2026, 12, 31));
    assert_eq!((sheet.columns[8].as_str(), sheet.columns[9].as_str()), ("Код вида деятельности", "Строка налога"));
    assert_eq!(sheet.rows.len(), 1);
    assert_eq!(
        sheet.rows[0][7..10],
        [Cell::Empty, Cell::Empty, text("Аренда, проценты, роялти")]
    );
}

/// Вид расхода «товар, не являющийся кулинарной продукцией» (задача 94) —
/// в столбце вида расхода словами окна.
#[test]
fn goods_a_cafe_sells_uncooked_show_their_nature() {
    let text_of = EXPENSE.replacen(r#""nature":"ordinary""#, r#""nature":"non_culinary""#, 1);
    assert_ne!(text_of, EXPENSE, "подмена не сработала");
    let expense: Operation = serde_json::from_str(&text_of).expect("расход non_culinary читается");
    let sheet = ledger_sheet(Lang::Ru, &[], &[expense], &[], day(2026, 10, 1), day(2026, 12, 31));
    assert_eq!(sheet.rows.len(), 1);
    assert_eq!(sheet.rows[0][6], text("Общепит: товар, не являющийся кулинарной продукцией"));
}

/// Документ иностранного поставщика со всеми данными (задача 72) — в столбце
/// документа продавца словами окна.
#[test]
fn a_foreign_document_shows_its_name() {
    let text_of = EXPENSE.replacen(r#""proof":"settlement_invoice""#, r#""proof":"foreign_document""#, 1);
    assert_ne!(text_of, EXPENSE, "подмена не сработала");
    let expense: Operation = serde_json::from_str(&text_of).expect("расход foreign_document читается");
    let sheet = ledger_sheet(Lang::Ru, &[], &[expense], &[], day(2026, 10, 1), day(2026, 12, 31));
    assert_eq!(sheet.rows.len(), 1);
    assert_eq!(sheet.rows[0][5], text("Документ иностранного поставщика со всеми данными по закону"));
}

/// Строки одного дня — по виду и номеру документа, а запись книги — за
/// ними: по одному случайному идентификатору акт 2026-003 вставал раньше
/// 2026-002 (полный прогон 0.0.96). Восемь документов одного дня — случайный
/// порядок совпал бы с номерами раз на десятки тысяч; в список они идут
/// задом наперёд.
#[test]
fn ledger_sheet_orders_a_day_by_kind_and_number() {
    let client = client();
    let october = day(2026, 10, 10);
    let mut documents = Vec::new();
    for kind in [DocKind::Invoice, DocKind::Act] {
        for _ in 0..4 {
            let data = DocumentData {
                kind,
                due: None,
                ..invoice_data(&client, october, october)
            };
            let document = issue(data, &client, &documents);
            documents.push(document);
        }
    }
    documents.reverse();
    let record = income(october, 5_000, Currency::Amd);
    let sheet = ledger_sheet(Lang::Ru, &documents, &[record], &[client], october, october);
    let got: Vec<&Cell> = sheet.rows.iter().map(|row| &row[2]).collect();
    let mut expected: Vec<Cell> = (1..=4).map(|seq| text(&format!("Счёт №\u{A0}2026-00{seq}"))).collect();
    expected.extend((1..=4).map(|seq| text(&format!("Акт сдачи-приёмки №\u{A0}2026-00{seq}"))));
    expected.push(Cell::Empty);
    assert_eq!(got, expected.iter().collect::<Vec<_>>());
}

/// Лист документов периода: выпущенные счета и акты по дате документа,
/// и аннулированные; черновика и удалённого нет. Статус и остаток — на день
/// выгрузки, у акта — счёт-основание, отметка о расчётном документе ՊԵԿ —
/// днём и номером, налоговый номер иностранного клиента — как есть.
#[test]
fn documents_sheet_lists_issued_documents_of_the_period() {
    let client = client();
    let october = day(2026, 10, 10);
    let lang = Lang::En;
    let mut documents = Vec::new();
    // Счёт отмечен выписанным, акт по нему выпущен отмеченным, а отметку
    // поправили у акта: своя отметка счёта устарела.
    let invoice = issue(invoice_data(&client, october, october), &client, &documents)
        .add_payment(Payment { date: day(2026, 10, 15), amount: 40_000 }, at(4_000))
        .expect("оплата")
        .set_settlement(Settlement { day: day(2026, 10, 10), number: line("A-1"), kind: SettlementKind::System }, at(5_000))
        .expect("отметка")
        .expect("отмечен");
    documents.push(invoice.clone());
    let act = issue(invoice.act_data(day(2026, 10, 12)).expect("акт по счёту"), &client, &documents)
        .set_settlement(Settlement { day: day(2026, 10, 12), number: line("2026-77"), kind: SettlementKind::System }, at(6_000))
        .expect("отметка")
        .expect("отмечен");
    documents.push(act);
    // Аннулированный был отмечен чеком ККМ (задача 76): отметка в листе
    // остаётся, с видом.
    let annulled = issue(invoice_data(&client, day(2026, 11, 1), day(2026, 11, 1)), &client, &documents)
        .set_settlement(
            Settlement { day: day(2026, 11, 1), number: Line::default(), kind: SettlementKind::Receipt },
            at(6_500),
        )
        .expect("отметка")
        .expect("отмечен")
        .annul(day(2026, 11, 2), at(7_000))
        .expect("выпущен")
        .expect("аннулирован");
    documents.push(annulled);
    documents.push(Document::create(invoice_data(&client, october, october), at(8_000)).expect("черновик"));
    // Вне периода — счёт следующего года.
    let later = issue(invoice_data(&client, day(2027, 1, 5), day(2027, 1, 5)), &client, &documents);
    documents.push(later);
    // Порядок листа — его собственный, а не порядок записей.
    documents.reverse();
    let today = day(2026, 11, 3);
    let sheet = documents_sheet(lang, &documents, day(2026, 10, 1), day(2026, 12, 31), Some(today));
    assert_eq!(sheet.columns.len(), 20);
    let number_of = |row: &Vec<Cell>| row[1].clone();
    let numbers: Vec<Cell> = sheet.rows.iter().map(number_of).collect();
    assert_eq!(numbers, [text("2026-001"), text("2026-001"), text("2026-002")], "счёт, акт, аннулированный счёт");
    let first = &sheet.rows[0];
    let mut wrong = Vec::new();
    let mut expect = |what: &str, seen: &Cell, want: Cell| {
        if *seen != want {
            wrong.push(format!("{what}: {seen:?}, а не {want:?}"));
        }
    };
    expect("вид", &first[0], text("Invoice"));
    expect("клиент", &first[4], text("Beispiel GmbH"));
    expect("налоговый номер", &first[5], text("DE123456789"));
    expect("сумма", &first[6], number(100_000, 2));
    // Доход по счёту — его акт: у счёта ни курса, ни дохода в драмах,
    // ни своей отметки ՊԵԿ.
    expect("курс счёта с актом", &first[8], Cell::Empty);
    expect("доход счёта с актом", &first[9], Cell::Empty);
    expect("день ՊԵԿ счёта с актом", &first[17], Cell::Empty);
    expect("номер ՊԵԿ счёта с актом", &first[18], Cell::Empty);
    expect("вид ՊԵԿ счёта с актом", &first[19], Cell::Empty);
    expect("оплачено", &first[10], number(40_000, 2));
    expect("остаток", &first[11], number(60_000, 2));
    // Срок — день счёта, и он прошёл, а оплачено не всё.
    expect("статус", &first[13], text("Overdue"));
    let act = &sheet.rows[1];
    expect("курс акта", &act[8], number(3_880_000, 4));
    expect("доход акта", &act[9], number(38_800_000, 2));
    // У акта оплат не бывает: ни «оплачено», ни «осталось».
    expect("оплачено у акта", &act[10], Cell::Empty);
    expect("остаток у акта", &act[11], Cell::Empty);
    expect("основание акта", &act[16], text("Invoice No.\u{A0}2026-001"));
    expect("день ՊԵԿ", &act[17], Cell::Date(day(2026, 10, 12)));
    expect("номер ՊԵԿ", &act[18], text("2026-77"));
    expect("вид ՊԵԿ", &act[19], text("in the SRC system"));
    let annulled = &sheet.rows[2];
    expect("статус аннулированного", &annulled[13], text("Cancelled"));
    expect("день аннулирования", &annulled[15], Cell::Date(day(2026, 11, 2)));
    expect("остаток аннулированного", &annulled[11], Cell::Empty);
    expect("курс аннулированного", &annulled[8], Cell::Empty);
    expect("доход аннулированного", &annulled[9], Cell::Empty);
    expect("сумма аннулированного", &annulled[6], number(100_000, 2));
    expect("день чека аннулированного", &annulled[17], Cell::Date(day(2026, 11, 1)));
    expect("номер чека аннулированного", &annulled[18], Cell::Empty);
    expect("вид — чек ККМ", &annulled[19], text("cash register receipt"));
    assert!(wrong.is_empty(), "{wrong:#?}");
    // Часы без даты: «просрочен» не узнать, и статуса нет ни у кого.
    let unknown = documents_sheet(lang, &documents, day(2026, 10, 1), day(2026, 12, 31), None);
    assert!(unknown.rows.iter().all(|row| row[13] == Cell::Empty), "{:#?}", unknown.rows);
}

/// Доход книги — запись в валюте `currency` без курса, по коду профиля.
fn income(date: Date, minor: i64, currency: Currency) -> Operation {
    let data = OperationData {
        flow: Flow::Income,
        date,
        paid: None,
        amount: Money::new(minor, currency),
        rate: None,
        text: line("Услуга"),
        client: None,
        supplier: Line::default(),
        proof: None,
        nature: None,
        cost: None,
        activity: None,
        income_kind: None,
        attachments: Vec::new(),
    };
    Operation::create(data, at(1_000)).expect("годная запись")
}

/// Запись `base` с поправленными полями — та же сумма и день.
fn changed(base: &Operation, change: impl FnOnce(&mut OperationData)) -> Operation {
    let mut data = base.data().clone();
    change(&mut data);
    Operation::create(data, at(1_000)).expect("годная запись")
}

/// Профиль ИП с этими кодами деятельности.
fn coded(codes: &[&str]) -> ProfileData {
    let mut data = profile();
    data.activities = codes.iter().map(|code| ActivityCode::parse(code).expect("код")).collect();
    data
}

/// Книга налога: только эти записи, профиль и снимки.
fn books<'a>(profile: &'a ProfileData, operations: &'a [Operation], closings: &'a [Closing]) -> Books<'a> {
    Books {
        documents: &[],
        operations,
        profile: Some(profile),
        closings,
        openings: &[],
        unreadable: 0,
    }
}

/// День, когда все кварталы тестов кончились.
const LATER: (u16, u8, u8) = (2030, 1, 2);

/// Закрыть кварталы `quarters` по записям, как «Закрыть квартал» на экране
/// налога: каждый следующий — со снимками прежних.
fn close(profile: &ProfileData, operations: &[Operation], quarters: &[Quarter]) -> Vec<Closing> {
    let mut closings = Vec::new();
    for quarter in quarters {
        let tax = turnover::quarter(books(profile, operations, &closings), *quarter);
        let today = day(LATER.0, LATER.1, LATER.2);
        closings.push(Closing::close(&tax, today, at(5_000)).expect("квартал кончился и посчитан"));
    }
    closings
}

/// Лист закрытых кварталов по записям и снимкам — как его собирает окно.
fn sheet_of(lang: Lang, profile: &ProfileData, operations: &[Operation], closings: &[Closing]) -> Sheet {
    let today = day(LATER.0, LATER.1, LATER.2);
    let closed = turnover::closed_in(books(profile, operations, closings), today, ..);
    closings_sheet(lang, closed.iter().map(|(tax, closing)| (tax, *closing)))
}

/// Лист закрытых кварталов (задача 83): у каждого — правила снимка
/// и нынешние, доход и налог в снимке и по книге сейчас и сверка. I квартал
/// совпадает: 1 000 000 × 10 % = 100 000. Во II забыли 200 000: в снимке
/// 500 000 и 50 000, сейчас 700 000 и 70 000. В III после закрытия внесли
/// доход в долларах без курса: расчёт неполон, чисел сейчас нет, и сверка не
/// говорит ни «совпадает», ни «расходится» (Правило 6).
#[test]
fn closings_sheet_sets_each_closed_quarter_beside_the_ledger() {
    let profile = coded(&["74.10.0"]);
    let quarter = |number| Quarter::new(2026, number).expect("квартал");
    let mut operations = vec![
        income(day(2026, 1, 15), 1_000_000, Currency::Amd),
        income(day(2026, 4, 15), 500_000, Currency::Amd),
        income(day(2026, 7, 15), 700_000, Currency::Amd),
    ];
    let closings = close(&profile, &operations, &[quarter(1), quarter(2), quarter(3)]);
    operations.push(income(day(2026, 6, 1), 200_000, Currency::Amd));
    operations.push(income(day(2026, 8, 1), 10_000, Currency::Usd));
    let sheet = sheet_of(Lang::Ru, &profile, &operations, &closings);
    assert_eq!(sheet.name, "Закрытые кварталы");
    assert_eq!(sheet.columns.len(), 8);
    let rules = text(&closings[0].data().rules.to_string());
    assert_eq!(
        sheet.rows,
        [
            vec![
                text("I\u{A0}квартал 2026"),
                rules.clone(),
                rules.clone(),
                number(1_000_000, 0),
                number(100_000, 0),
                number(1_000_000, 0),
                number(100_000, 0),
                text("Совпадает"),
            ],
            vec![
                text("II\u{A0}квартал 2026"),
                rules.clone(),
                rules.clone(),
                number(500_000, 0),
                number(50_000, 0),
                number(700_000, 0),
                number(70_000, 0),
                text("Расходится"),
            ],
            vec![
                text("III\u{A0}квартал 2026"),
                rules.clone(),
                rules,
                number(700_000, 0),
                number(70_000, 0),
                Cell::Empty,
                Cell::Empty,
                text("Не сравнить: расчёту по книге не хватает данных"),
            ],
        ]
    );
    // Закрытых кварталов нет — лист пустой, но на месте: файл одного вида.
    let empty = closings_sheet(Lang::En, []);
    assert_eq!((empty.name.as_str(), empty.rows.len(), empty.columns.len()), ("Closed quarters", 0, 8));
}

/// Сверка называет то, чего из двух сумм не видно (ревью задачи 83), как
/// экран налога. I квартал 2026: доход 100 000, расход 2 000 000 — вычет
/// 2 000 000 × 6 % = 120 000 больше налога 10 000, и налог — нижний предел
/// 100 000 × 4,5 % = 4 500; забытый расход 500 000 вычет и перенос меняет,
/// а налог — нет. IV квартал 2026 у ИП с кодами 74.10.0 и 70.22.0: доход
/// 1 000 000 по первому — 100 000; после закрытия внесли доход 20 декабря по
/// второму, закрывающему налог с оборота: с этого дня — общий режим, и сумм
/// «сейчас» нет (`QuarterTax::known`).
#[test]
fn closings_sheet_names_what_two_sums_hide() {
    let profile = coded(&["74.10.0"]);
    let first = Quarter::new(2026, 1).expect("квартал");
    let spent = changed(&income(day(2026, 2, 10), 2_000_000, Currency::Amd), |data| {
        data.flow = Flow::Expense;
        data.proof = Some(Proof::SettlementInvoice);
        data.nature = Some(Nature::Ordinary);
    });
    let mut operations = vec![income(day(2026, 1, 15), 100_000, Currency::Amd), spent.clone()];
    let closings = close(&profile, &operations, &[first]);
    operations.push(changed(&spent, |data| data.amount = Money::new(500_000, Currency::Amd)));
    let row = &sheet_of(Lang::Ru, &profile, &operations, &closings).rows[0];
    assert_eq!(
        row[3..],
        [
            number(100_000, 0),
            number(4_500, 0),
            number(100_000, 0),
            number(4_500, 0),
            text("Расходится: налог тот же, а строки расчёта другие"),
        ]
    );

    let two = coded(&["74.10.0", "70.22.0"]);
    let fourth = Quarter::new(2026, 4).expect("квартал");
    fn by(code: &'static str) -> impl FnOnce(&mut OperationData) {
        move |data| data.activity = Some(ActivityCode::parse(code).expect("код"))
    }
    let mut operations = vec![changed(&income(day(2026, 10, 5), 1_000_000, Currency::Amd), by("74.10.0"))];
    let closings = close(&two, &operations, &[fourth]);
    operations.push(changed(&income(day(2026, 12, 20), 300_000, Currency::Amd), by("70.22.0")));
    let mut wrong = Vec::new();
    for (lang, check) in [
        (Lang::Ru, "Расходится: с 20.12.2026\u{A0}— общий режим"),
        (Lang::En, "Differs: from Dec 20, 2026, the general regime"),
    ] {
        let row = &sheet_of(lang, &two, &operations, &closings).rows[0];
        let want = [number(1_000_000, 0), number(100_000, 0), Cell::Empty, Cell::Empty, text(check)];
        if row[3..] != want {
            wrong.push(format!("{lang:?}: {:?}", &row[3..]));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

#[test]
fn file_names_are_latin() {
    assert_eq!(file_name("2026-Q3", None, "xlsx"), "kvartik-2026-Q3.xlsx");
    assert_eq!(file_name("2026", Some("ledger"), "csv"), "kvartik-2026-ledger.csv");
}

/// Образцы выгрузки на трёх языках — для глаз и для Excel: `cargo test -p
/// kvartik-core look_export -- --ignored` пишет их в `target/look/task-30/`.
#[test]
#[ignore = "пишет образцы в target/look"]
fn look_export() {
    use std::fs;
    use std::path::PathBuf;
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/look/task-30");
    fs::create_dir_all(&dir).expect("папка");
    let client = client();
    let october = day(2026, 10, 10);
    let mut documents = Vec::new();
    let invoice = issue(invoice_data(&client, october, october), &client, &documents)
        .add_payment(Payment { date: day(2026, 10, 15), amount: 40_000 }, at(4_000))
        .expect("оплата");
    documents.push(invoice.clone());
    let act = issue(invoice.act_data(day(2026, 10, 12)).expect("акт по счёту"), &client, &documents)
        .set_settlement(Settlement { day: day(2026, 10, 12), number: line("2026-77"), kind: SettlementKind::System }, at(6_000))
        .expect("отметка")
        .expect("отмечен");
    documents.push(act);
    let paid = issue(invoice_data(&client, day(2026, 11, 1), day(2026, 11, 1)), &client, &documents)
        .add_payment(Payment { date: day(2026, 11, 2), amount: 100_000 }, at(7_000))
        .expect("оплата");
    documents.push(paid);
    let expense: Operation = serde_json::from_str(EXPENSE).expect("расход читается");
    let operations = [expense];
    let clients = [client];
    let (from, until) = (day(2026, 10, 1), day(2026, 12, 31));
    for lang in Lang::ALL {
        let ledger = ledger_sheet(lang, &documents, &operations, &clients, from, until);
        let issued = documents_sheet(lang, &documents, from, until, Some(day(2026, 11, 3)));
        let code = lang.code();
        fs::write(dir.join(format!("kvartik-2026-Q4-{code}.xlsx")), xlsx(&[ledger.clone(), issued.clone()])).expect("запись");
        fs::write(dir.join(format!("kvartik-2026-Q4-ledger-{code}.csv")), csv(&ledger)).expect("запись");
        fs::write(dir.join(format!("kvartik-2026-Q4-documents-{code}.csv")), csv(&issued)).expect("запись");
    }
}

/// Номер выгрузка пишет своей записью (`number_text`) — та же запись, что
/// у номера в окне и в PDF.
#[test]
fn number_text_is_the_number_as_written() {
    let mut wrong = Vec::new();
    for (seq, want) in [(7, "2026-007"), (42, "2026-042"), (1234, "2026-1234")] {
        let number: Number = serde_json::from_str(&format!(r#"{{"year":2026,"seq":{seq}}}"#)).expect("номер");
        let seen = number_text(number);
        if seen != want || seen != number.to_string() {
            wrong.push(format!("{seq}: {seen}, а нужно {want} и как у номера {number}"));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// День оплаты дохода по акту — у его счёта, оплаченного целиком, и только
/// когда акт у счёта один: актов-этапов два — на разработку и на хостинг
/// (задача 84), — и чьи деньги пришли, не узнать.
#[test]
fn act_payment_day_needs_its_only_act() {
    let client = client();
    let mut documents = Vec::new();
    let paid = |data, documents: &[Document]| {
        issue(data, &client, documents)
            .add_payment(Payment { date: day(2026, 10, 20), amount: 100_000 }, at(4_000))
            .expect("оплата")
    };
    let single = paid(invoice_data(&client, day(2026, 10, 1), day(2026, 10, 1)), &documents);
    documents.push(single.clone());
    let staged = paid(invoice_data(&client, day(2026, 10, 2), day(2026, 10, 2)), &documents);
    documents.push(staged.clone());
    // Этап — одна строка счёта: `None` — акт на весь счёт.
    for (invoice, date, line) in
        [(&single, day(2026, 10, 3), None), (&staged, day(2026, 10, 4), Some(0)), (&staged, day(2026, 10, 5), Some(1))]
    {
        let mut data = invoice.act_data(date).expect("акт по счёту");
        if let Some(line) = line {
            data.items = vec![data.items[line].clone()];
        }
        let act = issue(data, &client, &documents);
        documents.push(act);
    }
    let sheet = ledger_sheet(Lang::Ru, &documents, &[], std::slice::from_ref(&client), day(2026, 10, 1), day(2026, 12, 31));
    // В один день строки идут по идентификатору записи: порядок актов-этапов
    // здесь не важен.
    let mut paid_days: Vec<(Cell, Cell)> = sheet.rows.iter().map(|row| (row[2].clone(), row[14].clone())).collect();
    paid_days.sort_by_key(|(title, _)| format!("{title:?}"));
    assert_eq!(
        paid_days,
        [
            (text("Акт сдачи-приёмки №\u{A0}2026-001"), Cell::Date(day(2026, 10, 20))),
            (text("Акт сдачи-приёмки №\u{A0}2026-002"), Cell::Empty),
            (text("Акт сдачи-приёмки №\u{A0}2026-003"), Cell::Empty),
        ]
    );
}

/// Неизвестное — пустая ячейка, а не ноль (Правило 6): доход в валюте без
/// курса — без курса и без драмов в обоих листах, и `unrated` его называет.
#[test]
fn unknown_values_stay_empty() {
    let client = client();
    let mut data = invoice_data(&client, day(2026, 10, 10), day(2026, 10, 10));
    data.rate = None;
    let invoice = issue(data, &client, &[]);
    let documents = [invoice];
    let (from, until) = (day(2026, 10, 1), day(2026, 12, 31));
    let ledger = ledger_sheet(Lang::Ru, &documents, &[], &[client], from, until);
    assert_eq!((&ledger.rows[0][12], &ledger.rows[0][13]), (&Cell::Empty, &Cell::Empty), "книга");
    let issued = documents_sheet(Lang::Ru, &documents, from, until, Some(day(2026, 11, 3)));
    assert_eq!((&issued.rows[0][8], &issued.rows[0][9]), (&Cell::Empty, &Cell::Empty), "документы");
    assert_eq!(unrated(&documents, &[], from, until), 1);
    assert_eq!(unrated(&documents, &[], day(2027, 1, 1), day(2027, 3, 31)), 0);
}
