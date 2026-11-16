//! Тесты файла для системы ՊԵԿ. Ожидаемый XML выписан руками по схеме
//! `accdoc.xsd`, а не собран тем же кодом (Правило 2).

use super::*;
use crate::i18n::Lang;
use crate::model::activity::ActivityCode;
use crate::model::client::{Client, ClientData};
use crate::model::date::Date;
use crate::model::document::DocumentData;
use crate::model::money::{ExchangeRate, RateSource};
use crate::model::record::Timestamp;
use crate::model::tin::Tin;

fn day(year: u16, month: u8, day: u8) -> Date {
    Date::new(year, month, day).expect("день")
}

fn at(millis: i64) -> Timestamp {
    Timestamp::from_unix_millis(millis)
}

fn line(text: &str) -> Line {
    Line::new(text).expect("годный текст")
}

fn code(text: &str) -> ActivityCode {
    ActivityCode::parse(text).expect("код")
}

/// ИП на услугах — код 62.01.0 — с ՀՎՀՀ, адресом в две строки (у первой
/// своя запятая) и счетами в долларах и драмах.
fn profile() -> ProfileData {
    let mut data = ProfileData::default();
    data.name.hy = line("Արամ Պետրոսյան ԱՁ");
    data.name.en = line("IE Aram Petrosyan");
    data.tin = Some(Tin::parse("12345678").expect("ՀՎՀՀ"));
    data.address.hy = Lines::new("Երևան,\nԱբովյան 1").expect("адрес");
    data.accounts = vec![
        BankAccount::new(Currency::Usd, line("1180012345678901"), line("Բանկ ՓԲԸ"), None, Lines::default())
            .expect("счёт"),
        BankAccount::new(Currency::Amd, line("1180012345678900"), line("Բանկ ՓԲԸ"), None, Lines::default())
            .expect("счёт"),
    ];
    data.activities = vec![code("62.01.0")];
    data
}

/// Армянская фирма с ՀՎՀՀ — в названии знаки разметки XML.
fn armenian() -> Client {
    let data = ClientData {
        name: line("«Արարատ & Co» <ՍՊԸ>"),
        country: Some(Country::ARMENIA),
        address: Lines::new("Երևան").expect("адрес"),
        tin: Some(Tin::parse("07654321").expect("ՀՎՀՀ")),
        tax_id: Line::default(),
        currency: Some(Currency::Amd),
        lang: Some(Lang::Am),
        payment_days: None,
    };
    Client::create(data, at(1_000)).expect("клиент")
}

/// Фирма другой страны со своим налоговым номером.
fn foreign() -> Client {
    let data = ClientData {
        name: line("Beispiel GmbH"),
        country: Country::from_code("DE"),
        address: Lines::new("Hauptstraße 1\n10115 Berlin").expect("адрес"),
        tin: None,
        tax_id: line("DE123456789"),
        currency: Some(Currency::Usd),
        lang: Some(Lang::En),
        payment_days: None,
    };
    Client::create(data, at(1_000)).expect("клиент")
}

fn data(client: &Client, kind: DocKind, currency: Currency, items: Vec<Item>) -> DocumentData {
    DocumentData {
        kind,
        client: client.id(),
        lang: Lang::Am,
        currency,
        date: day(2026, 11, 16),
        service_date: day(2026, 11, 13),
        due: None,
        items,
        basis: None,
        recurring: None,
        rate: None,
        activity: None,
        income_kind: None,
    }
}

fn dollars(mut data: DocumentData, per_unit_e4: i64) -> DocumentData {
    data.rate = Some(ExchangeRate::new(Currency::Usd, per_unit_e4, data.service_date, RateSource::Manual).expect("курс"));
    data
}

fn issued(data: DocumentData, client: &Client, profile: &ProfileData) -> Document {
    Document::create(data, at(2_000))
        .expect("черновик")
        .issue(&[], 0, Some(profile), Some(None), client, at(3_000))
        .expect("выпущен")
}

fn item(text: &str, quantity_e3: i64, price: i64) -> Item {
    Item { text: line(text), quantity_e3, price }
}

/// Акт в драмах армянской фирме: вид 3, день услуги, без полей договора
/// и без вида сделки (решение человека — шапка модуля), реквизиты
/// по-армянски — адрес строкой через одну запятую, счёт в драмах, —
/// у покупателя ՀՎՀՀ, знаки разметки — сущностями; строки — «հատ»,
/// количество без лишних нулей, цена и стоимость в драмах, итог — сумма
/// строк: 500 000 + 2,5 × 20 000 = 550 000 ֏.
#[test]
fn drams_act_to_an_armenian_firm() {
    let profile = profile();
    let client = armenian();
    let act = issued(
        data(
            &client,
            DocKind::Act,
            Currency::Amd,
            vec![item("Ծրագրի մշակում", 1_000, 500_000), item("Հոստինգ, ամիս", 2_500, 20_000)],
        ),
        &client,
        &profile,
    );
    let want = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
        "<ExportedAccDocData xmlns=\"http://www.taxservice.am/tp3/invoice/definitions\">",
        "<AccountingDocument Version=\"1.0\">",
        "<Type>3</Type>",
        "<GeneralInfo><DeliveryDate>2026-11-13</DeliveryDate><Procedure>1</Procedure></GeneralInfo>",
        "<SupplierInfo><Taxpayer><TIN>12345678</TIN><Name>Արամ Պետրոսյան ԱՁ</Name>",
        "<Address>Երևան, Աբովյան 1</Address>",
        "<BankAccount><BankName>Բանկ ՓԲԸ</BankName><BankAccountNumber>1180012345678900</BankAccountNumber></BankAccount>",
        "</Taxpayer></SupplierInfo>",
        "<BuyerInfo><Taxpayer><TIN>07654321</TIN><Name>«Արարատ &amp; Co» &lt;ՍՊԸ&gt;</Name>",
        "<Address>Երևան</Address><TinNotRequired>false</TinNotRequired></Taxpayer></BuyerInfo>",
        "<GoodsInfo>",
        "<Good><Description>Ծրագրի մշակում</Description><Unit>հատ</Unit><Amount>1</Amount>",
        "<PricePerUnit>500000</PricePerUnit><Price>500000</Price><TotalPrice>500000</TotalPrice></Good>",
        "<Good><Description>Հոստինգ, ամիս</Description><Unit>հատ</Unit><Amount>2.5</Amount>",
        "<PricePerUnit>20000</PricePerUnit><Price>50000</Price><TotalPrice>50000</TotalPrice></Good>",
        "<Total><TotalPrice>550000</TotalPrice></Total></GoodsInfo>",
        "</AccountingDocument></ExportedAccDocData>\n",
    );
    assert_eq!(accdoc(&act, Some(&profile)).as_deref(), Ok(want));
    assert_eq!(file_name(&act).as_deref(), Some("accdoc-Act-2026-001.xml"));
}

/// Счёт в долларах иностранной фирме по курсу 360,54 ֏: цена 1 234,56 $ —
/// 445 108,2624 ֏ за единицу, 0,99 $ — 356,9346 ֏; строка — по курсу до лумы,
/// как доход в книге: 0,333 × 1 234,56 $ = 411,11 $ (411,1085) —
/// 148 221,60 ֏ (148 221,5994); 3 × 0,99 $ = 2,97 $ — 1 070,80 ֏
/// (1 070,8038). Итог — 149 292,40 ֏, доход счёта в книге: 414,08 $ ×
/// 360,54 = 149 292,4032. У покупателя ՀՎՀՀ нет — `TinNotRequired`, а его
/// номер — в `AdditionalData`; счёт ИП — в долларах.
#[test]
fn dollar_invoice_to_a_foreign_firm() {
    let profile = profile();
    let client = foreign();
    let data = dollars(
        data(
            &client,
            DocKind::Invoice,
            Currency::Usd,
            vec![item("Development", 333, 123_456), item("Domain, month", 3_000, 99)],
        ),
        3_605_400,
    );
    let invoice = issued(data, &client, &profile);
    let text = accdoc(&invoice, Some(&profile)).expect("файл");
    let mut wrong = Vec::new();
    for part in [
        "<BankAccountNumber>1180012345678901</BankAccountNumber>",
        "<BuyerInfo><Taxpayer><Name>Beispiel GmbH</Name><Address>Hauptstraße 1, 10115 Berlin</Address>",
        "<AdditionalData>DE123456789</AdditionalData><TinNotRequired>true</TinNotRequired></Taxpayer></BuyerInfo>",
        "<Amount>0.333</Amount><PricePerUnit>445108.2624</PricePerUnit><Price>148221.6</Price>",
        "<TotalPrice>148221.6</TotalPrice>",
        "<Amount>3</Amount><PricePerUnit>356.9346</PricePerUnit><Price>1070.8</Price>",
        "<Total><TotalPrice>149292.4</TotalPrice></Total>",
    ] {
        if !text.contains(part) {
            wrong.push(part);
        }
    }
    if text.contains("DealType") || text.contains("DealInfo") {
        wrong.push("вид сделки или поля договора");
    }
    assert!(wrong.is_empty(), "не так в файле: {wrong:#?}\n{text}");
    assert_eq!(file_name(&invoice).as_deref(), Some("accdoc-Invoice-2026-001.xml"));
}

/// Итог файла — доход документа в книге, хотя строки переводятся каждая:
/// две строки по 100,01 $ по 360,54 ֏ — по 36 057,6054 ֏, то есть
/// 36 057,61 ֏ каждая и 72 115,22 ֏ вместе, а доход счёта — 200,02 $ ×
/// 360,54 = 72 115,2108, то есть 72 115,21 ֏. Лишнюю луму берёт последняя
/// строка: 36 057,61 + 36 057,60.
#[test]
fn total_is_the_income_in_the_book() {
    let profile = profile();
    let client = foreign();
    let data = dollars(
        data(&client, DocKind::Invoice, Currency::Usd, vec![item("One", 1_000, 10_001), item("Two", 1_000, 10_001)]),
        3_605_400,
    );
    let text = accdoc(&issued(data, &client, &profile), Some(&profile)).expect("файл");
    let want = concat!(
        "<Price>36057.61</Price><TotalPrice>36057.61</TotalPrice></Good>",
        "<Good><Description>Two</Description><Unit>հատ</Unit><Amount>1</Amount>",
        "<PricePerUnit>36057.6054</PricePerUnit><Price>36057.6</Price><TotalPrice>36057.6</TotalPrice></Good>",
        "<Total><TotalPrice>72115.21</TotalPrice></Total>",
    );
    assert!(text.contains(want), "нет {want}:\n{text}");
}

/// Цена за единицу до стотысячной драма — округлением половины от нуля, как
/// перевод в драмы: у курса ЦБ доллара два знака, и цена делится нацело,
/// а курс, набранный руками, бывает и с четырьмя. 0,01 $ по 360,5405 ֏ —
/// 3,605405 ֏ за единицу, в файле — 3.60541; строка — 3,61 ֏ (3,605405).
#[test]
fn unit_price_rounds_half_away_from_zero() {
    let profile = profile();
    let client = foreign();
    let data = dollars(data(&client, DocKind::Invoice, Currency::Usd, vec![item("Ping", 1_000, 1)]), 3_605_405);
    let text = accdoc(&issued(data, &client, &profile), Some(&profile)).expect("файл");
    let part = "<Amount>1</Amount><PricePerUnit>3.60541</PricePerUnit><Price>3.61</Price>";
    assert!(text.contains(part), "нет {part}:\n{text}");
}

/// Ждали ошибку `want` или, при `None`, файл; не так — строка в `wrong`.
fn check(wrong: &mut Vec<String>, what: &str, got: Result<String, AccDocError>, want: Option<AccDocError>) {
    if got.as_ref().err() != want.as_ref() {
        wrong.push(format!("{what}: {:?}, а ждали {want:?}", got.err()));
    }
}

/// Чего не хватает — файла нет, и окно говорит почему. Каждый случай — рядом
/// с годным, который отличается от него одним местом: без проверки файл
/// вышел бы и у него.
#[test]
fn what_is_missing_is_named() {
    let profile = profile();
    let client = armenian();
    let good = data(&client, DocKind::Act, Currency::Amd, vec![item("Ծառայություն", 1_000, 10_000)]);
    assert!(accdoc(&issued(good.clone(), &client, &profile), Some(&profile)).is_ok(), "годный");
    let file = |data: DocumentData, profile: &ProfileData| accdoc(&issued(data, &client, profile), Some(profile));
    let mut wrong = Vec::new();

    let draft = Document::create(good.clone(), at(2_000)).expect("черновик");
    check(&mut wrong, "черновик", accdoc(&draft, Some(&profile)), Some(AccDocError::NotIssued));
    let annulled = issued(good.clone(), &client, &profile)
        .annul(day(2026, 11, 16), at(4_000))
        .expect("аннулируется")
        .expect("аннулирован");
    check(&mut wrong, "аннулирован", accdoc(&annulled, Some(&profile)), Some(AccDocError::NotIssued));

    // Валюта: без курса — нет; с курсом, но день услуги до 2026-07-01 — нет,
    // а с 1 июля — есть.
    let mut unrated = good.clone();
    unrated.currency = Currency::Usd;
    check(&mut wrong, "доллары без курса", file(unrated.clone(), &profile), Some(AccDocError::NoRate));
    let mut june = unrated.clone();
    june.service_date = day(2026, 6, 30);
    check(&mut wrong, "доллары 30 июня", file(dollars(june, 3_871_500), &profile), Some(AccDocError::OldRate));
    let mut july = unrated;
    july.service_date = day(2026, 7, 1);
    check(&mut wrong, "доллары 1 июля", file(dollars(july, 3_871_500), &profile), None);

    // Покупатель: армянская фирма без ՀՎՀՀ, фирма без страны и без ՀՎՀՀ.
    for (what, country, want) in [
        ("армянская фирма без ՀՎՀՀ", Some(Country::ARMENIA), AccDocError::NoBuyerTin),
        ("фирма без страны и ՀՎՀՀ", None, AccDocError::BuyerUnknown),
    ] {
        let mut buyer = armenian().data().clone();
        buyer.tin = None;
        buyer.country = country;
        let other = Client::create(buyer, at(1_000)).expect("клиент");
        let mut to_other = good.clone();
        to_other.client = other.id();
        check(&mut wrong, what, accdoc(&issued(to_other, &other, &profile), Some(&profile)), Some(want));
    }
    let mut seller = profile.clone();
    seller.tin = None;
    check(&mut wrong, "ИП без ՀՎՀՀ", file(good.clone(), &seller), Some(AccDocError::NoSellerTin));

    let mut long = good.clone();
    long.items = vec![item(&"ա".repeat(257), 1_000, 10_000)];
    check(&mut wrong, "услуга в 257 знаков", file(long, &profile), Some(AccDocError::TooLong(Field::Service)));
    let mut edge = good.clone();
    edge.items = vec![item(&"ա".repeat(256), 1_000, 10_000)];
    check(&mut wrong, "услуга в 256 знаков", file(edge, &profile), None);

    // Строка налога: торговля — товары; без кодов — неизвестно; прочие
    // доходы заявителя общепита — неизвестно; продажа имущества у самого
    // дохода — товары, аренда — услуга.
    let mut trader = profile.clone();
    trader.activities = vec![code("47.91.0")];
    trader.kinds.insert(code("47.91.0"), IncomeKind::Trade);
    check(&mut wrong, "торговля", file(good.clone(), &trader), Some(AccDocError::Goods));
    let mut bare = profile.clone();
    bare.activities.clear();
    check(&mut wrong, "кодов нет", file(good.clone(), &bare), Some(AccDocError::KindUnknown));
    let mut catering = profile.clone();
    catering.activities = vec![code("56.10.0"), code("47.11.0")];
    catering.kinds.insert(code("56.10.0"), IncomeKind::Catering);
    catering.kinds.insert(code("47.11.0"), IncomeKind::CateringOther);
    let mut other_income = good.clone();
    other_income.activity = Some(code("47.11.0"));
    check(&mut wrong, "прочий доход общепита", file(other_income, &catering), Some(AccDocError::KindUnknown));
    let mut sold = good.clone();
    sold.income_kind = Some(IncomeKind::Assets);
    check(&mut wrong, "продажа имущества", file(sold, &catering), Some(AccDocError::Goods));
    let mut rent = good.clone();
    rent.income_kind = Some(IncomeKind::Rent);
    check(&mut wrong, "аренда", file(rent, &profile), None);

    // Код, который закрывает налог с оборота (69 — юридические услуги, с
    // 2025-07-01), — к бухгалтеру; в году микропредпринимательства — его
    // исключения: 69 там не исключён, 70 — исключён.
    let mut lawyer = profile.clone();
    lawyer.activities = vec![code("69.10.0")];
    check(&mut wrong, "код 69 на налоге с оборота", file(good.clone(), &lawyer), Some(AccDocError::Closed));
    // Аренду режим решает не по коду профиля: юристу файл есть.
    let mut lawyer_rent = good.clone();
    lawyer_rent.income_kind = Some(IncomeKind::Rent);
    check(&mut wrong, "аренда у кода 69", file(lawyer_rent, &lawyer), None);
    let mut micro_lawyer = lawyer.clone();
    micro_lawyer.micro.insert(2026);
    check(&mut wrong, "код 69 в году микропредпринимательства", file(good.clone(), &micro_lawyer), None);
    let mut micro_manager = profile.clone();
    micro_manager.activities = vec![code("70.22.0")];
    micro_manager.micro.insert(2026);
    check(&mut wrong, "код 70 в году микропредпринимательства", file(good, &micro_manager), Some(AccDocError::Closed));

    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// Числа `xs:decimal` — без лишних нулей справа и без потери знаков.
#[test]
fn decimals_are_written_exactly() {
    let mut wrong = Vec::new();
    for (scaled, places, want) in [
        (55_000_000, 2, "550000"),
        (14_822_160, 2, "148221.6"),
        (14_822_159, 2, "148221.59"),
        (107_080, 2, "1070.8"),
        (333, 3, "0.333"),
        (2_500, 3, "2.5"),
        (44_510_826_240, 5, "445108.2624"),
        (1, 5, "0.00001"),
        (0, 2, "0"),
    ] {
        let seen = decimal(scaled, places);
        if seen != want {
            wrong.push(format!("{scaled}/{places}: {seen}, а не {want}"));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}
