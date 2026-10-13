//! Браузерные тесты раздела «Счета и акты» (задача 11): всё окно в настоящем
//! DOM и настоящей IndexedDB. Профиль и клиенты кладутся в базу командами
//! ядра — их формы проверяют свои тесты.

use kvartik_core::i18n::{self, Key, Lang, fill, t};
use kvartik_core::model::client::{Client, ClientData};
use kvartik_core::model::country::Country;
use kvartik_core::model::date::Date;
use kvartik_core::model::document::{DocKind, Document, DocumentData, Item, Status};
use kvartik_core::model::money::{Currency, Money};
use kvartik_core::model::profile::{Profile, ProfileData};
use kvartik_core::model::recurring::Recurring;
use kvartik_core::model::text::{Line, Lines};
use wasm_bindgen::JsCast;
use wasm_bindgen_test::wasm_bindgen_test;
use web_sys::HtmlElement;

use super::{button, choose_file, field, mount, pick, select, sleep, type_into, wait_for};
use crate::platform::clock;
use crate::store::device::Device;
use crate::store::{Store, Writes};

fn line(text: &str) -> Line {
    Line::new(text).expect("в тесте — годный текст")
}

/// Профиль ИП с именем на трёх языках.
fn profile() -> ProfileData {
    let mut data = ProfileData::default();
    data.name.hy = line("ԱՁ Արամ Պետրոսյան");
    data.name.ru = line("ИП Арам Петросян");
    data.name.en = line("IE Aram Petrosyan");
    data
}

/// Американская фирма: документы — в долларах, по-английски, оплата через
/// 15 дней.
fn acme() -> ClientData {
    ClientData {
        name: line("Acme Inc."),
        country: Country::from_code("US"),
        address: Lines::default(),
        tin: None,
        tax_id: Line::default(),
        currency: Some(Currency::Usd),
        lang: Some(Lang::En),
        payment_days: Some(15),
    }
}

/// Пустая база программы с профилем и клиентом — записанными командами
/// ядра. Возвращает клиента.
async fn fresh_base_with_client() -> Client {
    crate::store::delete_database(crate::store::DB_NAME).await;
    let now = clock::now();
    let profile = Profile::save(None, profile(), now)
        .expect("годный профиль")
        .expect("новый");
    let client = Client::create(acme(), now).expect("годный клиент");
    let mut writes = Writes::default();
    writes.insert(&profile);
    writes.insert(&client);
    commit(writes).await;
    client
}

/// Записать в базу программы одним действием.
async fn commit(writes: Writes) {
    let store = Store::open(|_| {}).await.expect("база открыта");
    store
        .write()
        .expect("транзакция записи")
        .commit(writes)
        .await
        .expect("записано");
}

/// Документы и повторы из базы программы.
async fn stored() -> (Vec<Document>, Vec<Recurring>) {
    let store = Store::open(|_| {}).await.expect("база открыта");
    let tx = store.read().expect("транзакция чтения");
    let documents = tx.list::<Document>().await.expect("документы читаются").records;
    let recurrings = tx.list::<Recurring>().await.expect("повторы читаются").records;
    (documents, recurrings)
}

/// Строка списка `list` (CSS), в тексте которой есть `text`.
fn row_with(host: &HtmlElement, list: &str, text: &str) -> Option<HtmlElement> {
    let rows = host.query_selector_all(&format!("{list} li")).ok()?;
    (0..rows.length())
        .filter_map(|at| rows.item(at))
        .map(JsCast::unchecked_into::<HtmlElement>)
        .find(|row| row.text_content().unwrap_or_default().contains(text))
}

/// Убрать за собой то, что тест оставил на устройстве.
fn forget_device() {
    if let Some(storage) = web_sys::window().and_then(|window| window.local_storage().ok().flatten())
    {
        let _ = storage.remove_item("kvartik.lang");
    }
}

/// Счёт от черновика до оплаты и акта: форма не выпускает без клиента
/// и говорит об этом у поля; клиент приносит язык, валюту и срок оплаты;
/// сумма строки — по правилам языка окна; выпуск даёт номер серии года,
/// а следующий выпущенный — следующий номер; оплата меняет статус; акт по
/// счёту — одним нажатием и со своей серией; аннулирование возвращается;
/// черновик удаляется и восстанавливается; удалённый клиент оставляет
/// документы, и сообщение называет их число.
#[wasm_bindgen_test]
async fn an_invoice_is_issued_paid_and_followed_by_an_act() {
    let client = fresh_base_with_client().await;
    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let today = clock::today().expect("часы с датой");
    let (host, mounted) = mount();
    let wrong = invoice_flow(&host, &client, lang, today).await;
    drop(mounted);
    host.remove();
    forget_device();
    assert!(wrong.is_empty(), "счёт и акт: {wrong:#?}");
}

/// Шаги теста выше. Шаг, без которого дальше проверять нечего, кончает
/// проверку сразу: иначе каждое следующее ожидание тратило бы свои пять
/// секунд, и тест падал бы по времени, а не словами.
async fn invoice_flow(host: &HtmlElement, client: &Client, lang: Lang, today: Date) -> Vec<String> {
    let mut wrong = Vec::new();
    let press = |label: &str, wrong: &mut Vec<String>| match button(host, &[label]) {
        Some(button) => button.click(),
        None => wrong.push(format!("нет кнопки «{label}»")),
    };
    let fill_in = |values: &[(Key, &str)], wrong: &mut Vec<String>| {
        for (label, value) in values {
            match field(host, t(lang, *label), 0) {
                Some(input) => type_into(&input, value),
                None => wrong.push(format!("нет поля {label:?}")),
            }
        }
    };
    let choose_client = |wrong: &mut Vec<String>| match select(host, t(lang, Key::DocClient), 0) {
        Some(clients) => pick(&clients, &client.id().to_string()),
        None => wrong.push("нет выбора клиента".to_owned()),
    };
    let open_row = |title: &str, wrong: &mut Vec<String>| {
        let row = row_with(host, "ul.documents:not(.deleted)", title);
        match row.and_then(|row| button(&row, &[t(lang, Key::DocOpen)])) {
            Some(open) => open.click(),
            None => wrong.push(format!("в списке нет «{title}»")),
        }
    };
    let numbered = |kind: Key, seq: u32| {
        fill(t(lang, Key::DocNumbered), &[t(lang, kind), &format!("{}-{seq:03}", today.year())])
    };
    if !wait_for(host, t(lang, Key::DocsAbout)).await {
        wrong.push("пустая база — а о документах ни слова".to_owned());
        return wrong;
    }

    press(t(lang, Key::DocNewInvoice), &mut wrong);
    sleep(100).await;
    press(t(lang, Key::DocIssue), &mut wrong);
    sleep(100).await;
    let text = host.text_content().unwrap_or_default();
    for key in [Key::DocClientMissing, Key::DocLanguageMissing, Key::DocCurrencyMissing, Key::ItemTextMissing] {
        if !text.contains(t(lang, key)) {
            wrong.push(format!("пустая форма выпущена без {key:?}"));
        }
    }

    choose_client(&mut wrong);
    sleep(100).await;
    let chosen = |label: Key| select(host, t(lang, label), 0).map(|select| select.value());
    if (chosen(Key::DocLanguage), chosen(Key::DocCurrency)) != (Some("en".to_owned()), Some("USD".to_owned())) {
        wrong.push(format!(
            "язык и валюта клиента не подставились: {:?} {:?}",
            chosen(Key::DocLanguage),
            chosen(Key::DocCurrency)
        ));
    }
    let due = today.add_days(15).map(Date::iso);
    if field(host, t(lang, Key::DocDue), 0).map(|input| input.value()) != due {
        wrong.push("срок оплаты клиента не подставился".to_owned());
    }
    fill_in(
        &[(Key::ItemText, "Разработка программ"), (Key::ItemQuantity, "1,5"), (Key::ItemPrice, "1 000,50")],
        &mut wrong,
    );
    sleep(100).await;
    // 1,5 × 1 000,50 $ = 1 500,75 $ — так прочитано по-русски.
    let line_total = i18n::money(lang, Money::new(150_075, Currency::Usd));
    if !host.text_content().unwrap_or_default().contains(&line_total) {
        wrong.push(format!("сумма строки не {line_total}"));
    }
    press(t(lang, Key::DocIssue), &mut wrong);
    let first = numbered(Key::KindInvoice, 1);
    if !wait_for(host, &fill(t(lang, Key::DocIssuedNotice), &[&first])).await {
        wrong.push(format!("выпуск не подтвердился: нет «{first}»"));
        return wrong;
    }

    let left = i18n::amount(lang, Money::new(150_075, Currency::Usd));
    if field(host, t(lang, Key::PaymentAmount), 0).map(|input| input.value()) != Some(left.clone()) {
        wrong.push(format!("в поле оплаты не остаток {left}"));
    }
    fill_in(&[(Key::PaymentAmount, "500")], &mut wrong);
    press(t(lang, Key::PaymentAdd), &mut wrong);
    if !wait_for(host, t(lang, Key::PaymentSaved)).await {
        wrong.push("оплата не подтвердилась".to_owned());
        return wrong;
    }
    if !wait_for(host, t(lang, Key::StatusPartlyPaid)).await {
        wrong.push("после части оплаты статус не «оплачен частично»".to_owned());
    }

    press(t(lang, Key::DocActFromInvoice), &mut wrong);
    sleep(100).await;
    match field(host, t(lang, Key::ItemText), 0) {
        Some(input) if input.value() == "Разработка программ" => {}
        other => wrong.push(format!("акт без строк счёта: {:?}", other.map(|input| input.value()))),
    }
    press(t(lang, Key::DocIssue), &mut wrong);
    let act = numbered(Key::KindAct, 1);
    if !wait_for(host, &fill(t(lang, Key::DocIssuedNotice), &[&act])).await {
        wrong.push(format!("акт не выпущен: нет «{act}»"));
        return wrong;
    }
    let basis = fill(t(lang, Key::DocBasis), &[&format!("{}-001", today.year())]);
    if !host.text_content().unwrap_or_default().contains(&basis) {
        wrong.push("у акта не названо основание".to_owned());
    }
    press(t(lang, Key::DocClose), &mut wrong);
    sleep(100).await;

    // Аннулировать счёт и вернуть его.
    open_row(&first, &mut wrong);
    sleep(100).await;
    press(t(lang, Key::DocAnnul), &mut wrong);
    if !wait_for(host, &fill(t(lang, Key::DocAnnulledNotice), &[&first])).await {
        wrong.push("аннулирование не подтвердилось".to_owned());
        return wrong;
    }
    if !host.text_content().unwrap_or_default().contains(t(lang, Key::StatusAnnulled)) {
        wrong.push("статус не «аннулирован»".to_owned());
    }
    press(t(lang, Key::DocUnannul), &mut wrong);
    if !wait_for(host, &fill(t(lang, Key::DocUnannulledNotice), &[&first])).await {
        wrong.push("аннулирование не вернулось".to_owned());
    }
    press(t(lang, Key::DocClose), &mut wrong);
    sleep(100).await;

    // Черновик: сохранить, удалить, восстановить, выпустить — номер 002:
    // удалённый черновик номера не занимал.
    press(t(lang, Key::DocNewInvoice), &mut wrong);
    sleep(100).await;
    choose_client(&mut wrong);
    sleep(50).await;
    fill_in(&[(Key::ItemText, "Поддержка"), (Key::ItemQuantity, "1"), (Key::ItemPrice, "100")], &mut wrong);
    press(t(lang, Key::DocSaveDraft), &mut wrong);
    if !wait_for(host, t(lang, Key::DocSaved)).await {
        wrong.push("черновик не сохранён".to_owned());
        return wrong;
    }
    open_row(t(lang, Key::KindInvoice), &mut wrong);
    sleep(100).await;
    press(t(lang, Key::DocDeleteDraft), &mut wrong);
    if !wait_for(host, t(lang, Key::DocDeletedNotice)).await {
        wrong.push("удаление черновика не подтвердилось".to_owned());
        return wrong;
    }
    press(t(lang, Key::ClientRestore), &mut wrong);
    if !wait_for(host, t(lang, Key::DocRestoredNotice)).await {
        wrong.push("черновик не восстановлен".to_owned());
        return wrong;
    }
    open_row(t(lang, Key::KindInvoice), &mut wrong);
    sleep(100).await;
    press(t(lang, Key::DocIssue), &mut wrong);
    let second = numbered(Key::KindInvoice, 2);
    if !wait_for(host, &fill(t(lang, Key::DocIssuedNotice), &[&second])).await {
        wrong.push(format!("второй счёт не выпущен: нет «{second}»"));
        return wrong;
    }
    press(t(lang, Key::DocClose), &mut wrong);
    sleep(100).await;
    // И ещё один черновик — у удалённого клиента он останется с пометкой.
    press(t(lang, Key::DocNewInvoice), &mut wrong);
    sleep(100).await;
    choose_client(&mut wrong);
    sleep(50).await;
    fill_in(&[(Key::ItemText, "Поддержка"), (Key::ItemQuantity, "1"), (Key::ItemPrice, "100")], &mut wrong);
    press(t(lang, Key::DocSaveDraft), &mut wrong);
    if !wait_for(host, t(lang, Key::DocSaved)).await {
        wrong.push("второй черновик не сохранён".to_owned());
        return wrong;
    }

    let (documents, _) = stored().await;
    let mut numbers: Vec<String> = documents
        .iter()
        .filter_map(|document| document.number().map(|number| format!("{:?} {number}", document.data().kind)))
        .collect();
    numbers.sort();
    let year = today.year();
    let expected = [format!("Act {year}-001"), format!("Invoice {year}-001"), format!("Invoice {year}-002")];
    if numbers != expected {
        wrong.push(format!("в базе номера {numbers:?}"));
    }
    let paid = documents
        .iter()
        .find(|d| d.number().is_some_and(|n| n.seq() == 1) && d.data().kind == DocKind::Invoice);
    match paid {
        Some(invoice) => {
            let issued = invoice.issued().expect("выпущен");
            if issued.buyer().name.as_str() != "Acme Inc."
                || issued.seller().name.get(Lang::En).as_str() != "IE Aram Petrosyan"
                || invoice.payments().len() != 1
                || invoice.status(today) != Status::PartlyPaid
            {
                wrong.push(format!("счёт в базе не тот: {invoice:?}"));
            }
        }
        None => wrong.push("выпущенного счёта в базе нет".to_owned()),
    }
    let drafts = documents.iter().filter(|d| d.number().is_none() && !d.is_deleted()).count();
    if drafts != 1 {
        wrong.push(format!("живых черновиков {drafts}, а не 1"));
    }

    // Удалить клиента: документы остаются, и сообщение называет их число.
    let delete = row_with(host, "ul.clients:not(.deleted)", "Acme Inc.")
        .and_then(|row| button(&row, &[t(lang, Key::ClientDelete)]));
    match delete {
        Some(delete) => delete.click(),
        None => wrong.push("нет кнопки удаления клиента".to_owned()),
    }
    let said = fill(t(lang, Key::ClientDeletedDocs), &["Acme Inc.", "4"]);
    if !wait_for(host, &said).await {
        wrong.push(format!("нет сообщения «{said}»"));
    }
    let mark = fill(t(lang, Key::ClientGoneMark), &["Acme Inc."]);
    if !host.text_content().unwrap_or_default().contains(&mark) {
        wrong.push("у черновика удалённого клиента нет пометки".to_owned());
    }
    wrong
}

/// Восстановление из копии меняет базу в обход раздела, и раздел документов
/// обязан перечитать её, как профиль и клиенты. Копия — образец 0.0.17:
/// оплаченный счёт с повтором, акт по нему, аннулированный счёт, удалённый
/// черновик акта и живой черновик счёта.
#[wasm_bindgen_test]
async fn restoring_a_backup_shows_its_documents() {
    crate::store::delete_database(crate::store::DB_NAME).await;
    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let (host, mounted) = mount();
    let mut wrong = Vec::new();
    if !wait_for(&host, t(lang, Key::DocsAbout)).await {
        wrong.push("пустая база — а о документах ни слова".to_owned());
    }
    if !wait_for(&host, t(lang, Key::BackupChooseFile)).await {
        wrong.push("раздела копии нет".to_owned());
    }
    match host
        .query_selector(r#"input[type="file"][accept=".json,application/json"]"#)
        .ok()
        .flatten()
    {
        Some(input) => choose_file(
            input.unchecked_ref(),
            "kvartik-2026-10-13.json",
            include_str!("../../../../core/tests/backups/v5/kvartik-2026-10-13.json"),
        ),
        None => wrong.push("нет поля выбора копии".to_owned()),
    }
    if !wait_for(&host, t(lang, Key::BackupFilePassword)).await {
        wrong.push("копия не открылась".to_owned());
    }
    match field(&host, t(lang, Key::BackupFilePassword), 0) {
        Some(password) => type_into(&password, "Պահուստ копия sample 0.0.17"),
        None => wrong.push("нет поля пароля копии".to_owned()),
    }
    match button(&host, &[t(lang, Key::BackupRestore)]) {
        Some(restore) => restore.click(),
        None => wrong.push("нет кнопки «Восстановить»".to_owned()),
    }
    let done = t(lang, Key::BackupRestored).split("{}").next().unwrap_or_default();
    if !wait_for(&host, done).await {
        wrong.push("восстановление не завершилось".to_owned());
    }
    let numbered = |kind: Key, number: &str| fill(t(lang, Key::DocNumbered), &[t(lang, kind), number]);
    for (title, status) in [
        (numbered(Key::KindInvoice, "2026-001"), Key::StatusPaid),
        (numbered(Key::KindAct, "2026-001"), Key::StatusIssued),
        (numbered(Key::KindInvoice, "2026-002"), Key::StatusAnnulled),
        (t(lang, Key::KindInvoice).to_owned(), Key::StatusDraft),
    ] {
        if !wait_for(&host, &title).await {
            wrong.push(format!("после восстановления нет «{title}»"));
            continue;
        }
        match row_with(&host, "ul.documents:not(.deleted)", &title) {
            Some(row) if row.text_content().unwrap_or_default().contains(t(lang, status)) => {}
            _ => wrong.push(format!("у «{title}» не статус {status:?}")),
        }
    }
    let deleted = fill(t(lang, Key::DocsDeleted), &["1"]);
    if !host.text_content().unwrap_or_default().contains(&deleted) {
        wrong.push("удалённый черновик копии не в удалённых".to_owned());
    }
    drop(mounted);
    host.remove();
    if let Some(storage) = web_sys::window().and_then(|window| window.local_storage().ok().flatten())
    {
        for name in ["kvartik.lang", "kvartik.backup_key", "kvartik.last_backup"] {
            let _ = storage.remove_item(name);
        }
    }
    assert!(wrong.is_empty(), "восстановление документов: {wrong:#?}");
}

/// Повторяющийся счёт: черновик появляется, когда программу открывают в его
/// день или позже, — один раз, а не при каждом открытии.
#[wasm_bindgen_test]
async fn a_repeating_invoice_appears_when_the_app_opens() {
    let client = fresh_base_with_client().await;
    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let today = clock::today().expect("часы с датой");
    let now = clock::now();
    // Счёт сорок дней назад: месяц после него уже наступил, а два — нет.
    let date = today.add_days(-40).expect("дата есть");
    let data = DocumentData {
        kind: DocKind::Invoice,
        client: client.id(),
        lang: Lang::En,
        currency: Currency::Usd,
        date,
        service_date: date,
        due: None,
        items: vec![Item {
            text: line("Software development"),
            quantity_e3: 1_000,
            price: 500_000,
        }],
        basis: None,
        recurring: None,
    };
    let source = Document::create(data, now)
        .expect("черновик")
        .issue(&[], 0, Some(&profile()), &client, now)
        .expect("выпущен");
    let recurring = Recurring::start(None, &source, date, now)
        .expect("годится")
        .expect("новый");
    let mut writes = Writes::default();
    writes.insert(&source);
    writes.insert(&recurring);
    commit(writes).await;

    let (host, mounted) = mount();
    let mut wrong = Vec::new();
    let created = fill(t(lang, Key::RecurCreated), &["1"]);
    if !wait_for(&host, &created).await {
        wrong.push(format!("нет «{created}»"));
    }
    drop(mounted);
    host.remove();
    let (documents, recurrings) = stored().await;
    let drafts: Vec<&Document> = documents
        .iter()
        .filter(|document| document.data().recurring == Some(recurring.id()))
        .collect();
    match drafts.as_slice() {
        [draft] if draft.number().is_none() && draft.data().date == date.add_months(1).expect("дата") => {}
        other => wrong.push(format!("черновики повтора: {other:?}")),
    }
    if recurrings.first().map(|r| r.data().next) != Some(2) {
        wrong.push(format!("счётчик повтора не сдвинут: {recurrings:?}"));
    }

    // Второе открытие черновиков не прибавляет.
    let (host, mounted) = mount();
    if !wait_for(&host, t(lang, Key::DocsTitle)).await {
        wrong.push("раздела документов нет".to_owned());
    }
    sleep(300).await;
    drop(mounted);
    host.remove();
    let (documents, _) = stored().await;
    let count = documents.iter().filter(|d| d.data().recurring.is_some()).count();
    if count != 1 {
        wrong.push(format!("после второго открытия черновиков повтора {count}"));
    }
    forget_device();
    assert!(wrong.is_empty(), "повтор: {wrong:#?}");
}

/// Числа в форме документа читаются по языку окна, и при смене языка они
/// переписываются его записью, а не меняют смысла: «1,234.50» по-английски
/// по-русски — «1 234,50», а не одна целая с тысячными.
#[wasm_bindgen_test]
async fn numbers_follow_the_window_language() {
    let client = fresh_base_with_client().await;
    Device::new().set_ui_lang(Lang::En);
    let (host, mounted) = mount();
    let mut wrong = Vec::new();
    if !wait_for(&host, t(Lang::En, Key::DocsAbout)).await {
        wrong.push("раздела документов нет".to_owned());
    }
    match button(&host, &[t(Lang::En, Key::DocNewInvoice)]) {
        Some(new) => new.click(),
        None => wrong.push("нет кнопки нового счёта".to_owned()),
    }
    sleep(100).await;
    if let Some(clients) = select(&host, t(Lang::En, Key::DocClient), 0) {
        pick(&clients, &client.id().to_string());
    }
    sleep(50).await;
    for (label, value) in [(Key::ItemQuantity, "1,000.5"), (Key::ItemPrice, "1,234.50")] {
        match field(&host, t(Lang::En, label), 0) {
            Some(input) => type_into(&input, value),
            None => wrong.push(format!("нет поля {label:?}")),
        }
    }
    sleep(50).await;
    for (lang, quantity, price) in [
        (Lang::Ru, "1\u{A0}000,5", "1\u{A0}234,50"),
        (Lang::Am, "1000,5", "1234,50"),
        (Lang::En, "1,000.5", "1,234.50"),
    ] {
        match host
            .query_selector(&format!(".langs button[lang=\"{}\"]", lang.code()))
            .ok()
            .flatten()
        {
            Some(switch) => switch.unchecked_ref::<HtmlElement>().click(),
            None => wrong.push(format!("{lang:?}: нет кнопки языка")),
        }
        sleep(100).await;
        let value = |label: Key| field(&host, t(lang, label), 0).map(|input| input.value());
        if (value(Key::ItemQuantity).as_deref(), value(Key::ItemPrice).as_deref()) != (Some(quantity), Some(price)) {
            wrong.push(format!(
                "{lang:?}: в полях {:?} и {:?}, а не «{quantity}» и «{price}»",
                value(Key::ItemQuantity),
                value(Key::ItemPrice)
            ));
        }
        // 1 000,5 × 1 234,50 $ = 1 235 117,25 $.
        let sum = i18n::money(lang, Money::new(123_511_725, Currency::Usd));
        if !host.text_content().unwrap_or_default().contains(&sum) {
            wrong.push(format!("{lang:?}: сумма строки не {sum}"));
        }
    }
    drop(mounted);
    host.remove();
    forget_device();
    assert!(wrong.is_empty(), "числа и язык окна: {wrong:#?}");
}
