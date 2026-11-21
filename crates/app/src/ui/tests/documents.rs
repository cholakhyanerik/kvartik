//! Браузерные тесты раздела «Счета и акты» (задача 11): всё окно в настоящем
//! DOM и настоящей IndexedDB. Профиль и клиенты кладутся в базу командами
//! ядра — их формы проверяют свои тесты.

use kvartik_core::format::Stored;
use kvartik_core::format::accdoc;
use kvartik_core::i18n::{self, Key, Lang, fill, t};
use kvartik_core::model::activity::{ActivityCode, IncomeKind};
use kvartik_core::model::client::{Client, ClientData};
use kvartik_core::model::country::Country;
use kvartik_core::model::date::Date;
use kvartik_core::model::document::{
    DocKind, Document, DocumentData, Issued, Item, Payment, Settlement, SettlementKind, Status,
};
use kvartik_core::doc::image::Picture;
use kvartik_core::model::logo::{self, Image, IssuedLogo, Logo};
use kvartik_core::model::money::{Currency, Money};
use kvartik_core::model::office::{Ip, Issuing};
use kvartik_core::model::profile::{Profile, ProfileData};
use kvartik_core::model::record::{RecordId, Timestamp};
use kvartik_core::model::recurring::Recurring;
use kvartik_core::model::text::{Line, Lines};
use kvartik_core::model::tin::Tin;
use js_sys::{Array, Function, Promise, Reflect, Uint8Array};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_test::wasm_bindgen_test;
use web_sys::{HtmlElement, Response};

use super::ledger::png;
use super::share::ShareStub;
use super::{
    backup_file, button, choose_file, field, mount, mount_at, pick_client, select, shown_step, sleep,
    type_into, visit, wait_for,
};
use crate::ui::documents::save;
use crate::ui::profile::fitted;
use crate::ui::route::Screen;
use crate::platform::clock;
use crate::store::device::Device;
use crate::store::{DB_NAME, Store, Writes};

pub(super) fn line(text: &str) -> Line {
    Line::new(text).expect("в тесте — годный текст")
}

/// Профиль ИП с именем на трёх языках.
pub(super) fn profile() -> ProfileData {
    let mut data = ProfileData::default();
    data.name.hy = line("ԱՁ Արամ Պետրոսյան");
    data.name.ru = line("ИП Арам Петросян");
    data.name.en = line("IE Aram Petrosyan");
    data
}

/// Американская фирма: документы — в долларах, по-английски, оплата через
/// 15 дней.
pub(super) fn acme() -> ClientData {
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
pub(super) async fn fresh_base_with_client() -> Client {
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
pub(super) async fn commit(writes: Writes) {
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
pub(super) fn row_with(host: &HtmlElement, list: &str, text: &str) -> Option<HtmlElement> {
    let rows = host.query_selector_all(&format!("{list} li")).ok()?;
    (0..rows.length())
        .filter_map(|at| rows.item(at))
        .map(JsCast::unchecked_into::<HtmlElement>)
        .find(|row| row.text_content().unwrap_or_default().contains(text))
}

/// Убрать за собой то, что тест оставил на устройстве.
pub(super) fn forget_device() {
    if let Some(storage) = web_sys::window().and_then(|window| window.local_storage().ok().flatten())
    {
        let _ = storage.remove_item("kvartik.lang");
        let _ = storage.remove_item("kvartik.site_lang");
        // День данных пробного периода (задача 99): копия со старыми
        // записями сократила бы пробный период следующим тестам.
        let _ = storage.remove_item("kvartik.trial_data");
    }
}

/// Счёт от черновика до оплаты и акта: новый — по шагам, и шаг строк не
/// пускает дальше с пустой строкой и говорит об этом у поля; клиент
/// выбирается строкой и приносит язык, валюту и срок оплаты, а «Изменить»
/// возвращает к выбору;
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
    // Удаляют клиента — из его экрана: он тоже должен быть в странице.
    visit(&host, &[Screen::Documents, Screen::Clients]).await;
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
    let choose_client = |wrong: &mut Vec<String>| {
        if !pick_client(host, client.data().name.as_str()) {
            wrong.push("клиента нет в выборе".to_owned());
        }
    };
    let step = || shown_step(host, "form.document");
    // Строка списка — ссылка: выпущенный открывается карточкой, черновик —
    // формой.
    let open_row = |title: &str, wrong: &mut Vec<String>| {
        let row = row_with(host, "ul.documents:not(.deleted)", title);
        match row.and_then(|row| row.query_selector("a.entry").ok().flatten()) {
            Some(open) => open.unchecked_into::<HtmlElement>().click(),
            None => wrong.push(format!("в списке нет «{title}»")),
        }
    };
    // Карточка документа закрывается ссылкой «назад» к списку.
    let close = |wrong: &mut Vec<String>| {
        match host.query_selector(".docs-detail a.back").ok().flatten() {
            Some(back) => back.unchecked_into::<HtmlElement>().click(),
            None => wrong.push("у карточки документа нет «назад»".to_owned()),
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
    if step() != Some(1) || button(host, &[t(lang, Key::DocIssue)]).is_some() {
        wrong.push(format!("новый счёт — не с выбора клиента: шаг {:?}", step()));
    }
    choose_client(&mut wrong);
    sleep(100).await;
    if step() != Some(2) {
        wrong.push(format!("выбор клиента не повёл дальше: шаг {:?}", step()));
    }
    // «Изменить» у клиента — снова к выбору.
    press(t(lang, Key::Change), &mut wrong);
    sleep(50).await;
    if step() != Some(1) {
        wrong.push(format!("«Изменить» не вернуло к выбору клиента: шаг {:?}", step()));
    }
    choose_client(&mut wrong);
    sleep(100).await;
    // Пустая строка дальше не пускает и говорит об этом у поля.
    press(t(lang, Key::DocNextReview), &mut wrong);
    sleep(100).await;
    let text = host.text_content().unwrap_or_default();
    if step() != Some(2) || !text.contains(t(lang, Key::ItemTextMissing)) || !text.contains(t(lang, Key::StepFix)) {
        wrong.push(format!("пустая строка прошла к проверке: шаг {:?}", step()));
    }
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
    if field(host, t(lang, Key::ItemQuantity), 0).map(|input| input.value()).as_deref() != Some("1") {
        wrong.push("количество новой строки — не 1".to_owned());
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
    press(t(lang, Key::DocNextReview), &mut wrong);
    sleep(100).await;
    if step() != Some(3) {
        wrong.push(format!("верная строка не пустила к проверке: шаг {:?}", step()));
    }
    // Счёт в долларах, а в профиле нет счёта в долларах с SWIFT (полный
    // прогон 2026-11-10, п. 8): шаг проверки говорит это, а выпустить не
    // мешает.
    let abroad = fill(t(lang, Key::DocNotPayableAbroad), &["USD"]);
    if !host.text_content().unwrap_or_default().contains(&abroad) {
        wrong.push("счёт в долларах без счёта ИП в долларах — а о реквизитах ни слова".to_owned());
    }
    press(t(lang, Key::DocIssue), &mut wrong);
    let first = numbered(Key::KindInvoice, 1);
    if !wait_for(host, &fill(t(lang, Key::DocIssuedNotice), &[&first])).await {
        wrong.push(format!("выпуск не подтвердился: нет «{first}»"));
        return wrong;
    }
    // Карточку выпущенного окно открывает переходом по адресу, а он приходит
    // событием своей задачей — позже сообщения о выпуске: тест ждёт карточку
    // (Правило 2; в CI Firefox проверка сразу после сообщения застала
    // прежний экран, 2026-11-13).
    if !wait_until(|| field(host, t(lang, Key::PaymentAmount), 0).is_some()).await {
        wrong.push("карточка выпущенного счёта не открылась".to_owned());
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
    // Карточка акта приходит позже сообщения (выше, у счёта): её ждать.
    let basis = fill(t(lang, Key::DocBasis), &[&format!("{}-001", today.year())]);
    if !wait_for(host, &basis).await {
        wrong.push("у акта не названо основание".to_owned());
    }
    sign_flow(host, lang, today, &mut wrong).await;
    close(&mut wrong);
    sleep(100).await;

    // Аннулировать счёт и вернуть его. Строка списка — ссылка: карточка
    // приходит переходом по адресу, и её ждать, а не фиксированную паузу.
    open_row(&first, &mut wrong);
    // Ждать карточку именно этого счёта и нажимать в ней, а не первую
    // «Аннулировать» в странице: на занятом раннере CI переход «назад» из
    // карточки акта обработался позже нажатия строки, кнопка нашлась ещё
    // в карточке акта, и аннулировался акт (Chrome, 2026-11-17).
    let annul = || {
        let card: HtmlElement = host.query_selector(".docs-detail").ok().flatten()?.unchecked_into();
        let heading = card.query_selector("h2.doc-heading").ok().flatten()?.text_content()?;
        (heading.trim() == first).then(|| button(&card, &[t(lang, Key::DocAnnul)])).flatten()
    };
    if !wait_until(|| annul().is_some()).await {
        wrong.push("карточка выпущенного счёта не открылась из списка".to_owned());
        return wrong;
    }
    match annul() {
        Some(button) => button.click(),
        None => wrong.push("в карточке счёта нет «Аннулировать»".to_owned()),
    }
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
    close(&mut wrong);
    sleep(100).await;

    // Черновик: сохранить, удалить, восстановить, выпустить — номер 002:
    // удалённый черновик номера не занимал.
    press(t(lang, Key::DocNewInvoice), &mut wrong);
    sleep(100).await;
    choose_client(&mut wrong);
    sleep(50).await;
    fill_in(&[(Key::ItemText, "Поддержка"), (Key::ItemPrice, "100")], &mut wrong);
    press(t(lang, Key::DocNextReview), &mut wrong);
    sleep(50).await;
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
    if !wait_until(|| button(host, &[t(lang, Key::DocIssue)]).is_some()).await {
        wrong.push("форма черновика не открылась из списка".to_owned());
        return wrong;
    }
    press(t(lang, Key::DocIssue), &mut wrong);
    let second = numbered(Key::KindInvoice, 2);
    if !wait_for(host, &fill(t(lang, Key::DocIssuedNotice), &[&second])).await {
        wrong.push(format!("второй счёт не выпущен: нет «{second}»"));
        return wrong;
    }
    // Карточка выпущенного — позже сообщения (выше, у первого счёта).
    if !wait_selector(host, ".docs-detail a.back").await {
        wrong.push("карточка второго счёта не открылась".to_owned());
        return wrong;
    }
    close(&mut wrong);
    sleep(100).await;
    // И ещё один черновик — у удалённого клиента он останется с пометкой.
    press(t(lang, Key::DocNewInvoice), &mut wrong);
    sleep(100).await;
    choose_client(&mut wrong);
    sleep(50).await;
    fill_in(&[(Key::ItemText, "Поддержка"), (Key::ItemQuantity, "1"), (Key::ItemPrice, "100")], &mut wrong);
    press(t(lang, Key::DocNextReview), &mut wrong);
    sleep(50).await;
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
    // Удаляют из формы клиента: строка списка открывает её.
    let entry = row_with(host, "ul.clients:not(.deleted)", "Acme Inc.")
        .and_then(|row| row.query_selector("button.entry").ok().flatten());
    match entry {
        Some(entry) => entry.unchecked_into::<HtmlElement>().click(),
        None => wrong.push("нет строки клиента".to_owned()),
    }
    sleep(50).await;
    match button(host, &[t(lang, Key::ClientDelete)]) {
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

/// Подпись открытого выпущенного акта клиентом (задача 62): день раньше
/// даты акта не проходит и говорит у поля; сегодняшний — статус «подписан»,
/// событие в истории и день подписи в базе; «Снять отметку» возвращает
/// «выпущен», и в базе подписи нет.
async fn sign_flow(host: &HtmlElement, lang: Lang, today: Date, wrong: &mut Vec<String>) {
    let status = || {
        host.query_selector(".docs-detail .doc-card .status")
            .ok()
            .flatten()
            .and_then(|status| status.text_content())
            .unwrap_or_default()
    };
    let signed_in_base = || async {
        let (documents, _) = stored().await;
        documents
            .iter()
            .find(|d| d.data().kind == DocKind::Act && d.number().is_some_and(|n| n.seq() == 1))
            .map(Document::signed)
    };
    if !wait_until(|| status() == t(lang, Key::StatusIssued)).await {
        wrong.push(format!("выпущенный акт — не «выпущен»: «{}»", status()));
    }
    let Some(input) = field(host, t(lang, Key::SignDate), 0) else {
        wrong.push("у выпущенного акта нет дня подписи".to_owned());
        return;
    };
    if input.value() != today.iso() {
        wrong.push(format!("день подписи — не сегодня: {}", input.value()));
    }
    let yesterday = today.add_days(-1).expect("дата есть");
    type_into(&input, &yesterday.iso());
    match button(host, &[t(lang, Key::SignMark)]) {
        Some(mark) => mark.click(),
        None => wrong.push("нет «Отметить, что подписан»".to_owned()),
    }
    sleep(100).await;
    if !host.text_content().unwrap_or_default().contains(t(lang, Key::SignBeforeDate)) {
        wrong.push("подпись раньше акта прошла без слова".to_owned());
    }
    if signed_in_base().await != Some(None) {
        wrong.push(format!("подпись раньше акта легла в базу: {:?}", signed_in_base().await));
    }

    type_into(&input, &today.iso());
    if let Some(mark) = button(host, &[t(lang, Key::SignMark)]) {
        mark.click();
    }
    if !wait_for(host, t(lang, Key::SignSaved)).await {
        wrong.push("подпись не подтвердилась".to_owned());
        return;
    }
    let text = host.text_content().unwrap_or_default();
    if status() != t(lang, Key::StatusSigned) {
        wrong.push(format!("подписанный акт — не «подписан»: «{}»", status()));
    }
    if !text.contains(t(lang, Key::HistorySigned)) {
        wrong.push("в истории нет подписи".to_owned());
    }
    if !text.contains(&fill(t(lang, Key::SignedOn), &[&i18n::date(lang, today)])) {
        wrong.push("не назван день подписи".to_owned());
    }
    if signed_in_base().await != Some(Some(today)) {
        wrong.push(format!("в базе подпись не та: {:?}", signed_in_base().await));
    }

    match button(host, &[t(lang, Key::SignRemove)]) {
        Some(remove) => remove.click(),
        None => wrong.push("нет «Снять отметку»".to_owned()),
    }
    if !wait_for(host, t(lang, Key::SignRemoved)).await {
        wrong.push("отметка не снялась".to_owned());
        return;
    }
    if status() != t(lang, Key::StatusIssued) {
        wrong.push(format!("после снятия отметки — не «выпущен»: «{}»", status()));
    }
    if signed_in_base().await != Some(None) {
        wrong.push(format!("после снятия отметки в базе подпись: {:?}", signed_in_base().await));
    }
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
    let (host, mounted) = mount_at("#/documents");
    let mut wrong = Vec::new();
    if !wait_for(&host, t(lang, Key::DocsAbout)).await {
        wrong.push("пустая база — а о документах ни слова".to_owned());
    }
    // Экран документов уже нарисован — восстановление идёт с экрана копии.
    match backup_file(&host).await {
        Some(input) => choose_file(
            &input,
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
    // Копия легла в пустую программу — так заводят второе устройство
    // (задача 41): окно спрашивает, где выпускать, а ответ помнит устройство.
    if !wait_for(&host, t(lang, Key::BackupIssuerAsk)).await {
        wrong.push("после копии в пустую программу не спросили, где выпускать".to_owned());
    }
    match button(&host, &[t(lang, Key::BackupIssuerThere)]) {
        Some(there) => there.click(),
        None => wrong.push(format!("нет «{}»", t(lang, Key::BackupIssuerThere))),
    }
    if !wait_for(&host, t(lang, Key::BackupIssuerThereDone)).await || !Device::new().issue_elsewhere(DB_NAME) {
        wrong.push("ответ «там» не выключил выпуск здесь".to_owned());
    }
    // Повтор образца (с 13 октября 2026 года) ждёт черновика за ноябрь, но
    // восстановление его не создаёт: вопрос, где выпускать, встаёт позже
    // чтения базы (задача 41) — черновик создаст следующее открытие. Ждать
    // нечего — черновика не должно быть, — поэтому пауза, как у повтора выше.
    sleep(300).await;
    let (documents, _) = stored().await;
    if documents.iter().any(|document| document.data().recurring.is_some()) {
        wrong.push("восстановление создало черновик повтора до ответа, где выпускать".to_owned());
    }
    crate::ui::settings::set_issue_elsewhere(None, false);
    drop(mounted);
    host.remove();
    if let Some(storage) = web_sys::window().and_then(|window| window.local_storage().ok().flatten())
    {
        // И день данных: образец копии приносит прошедшие дни пробного
        // периода, и устройство их помнит (задача 99) — следующим тестам
        // нужен свой пробный период.
        for name in ["kvartik.lang", "kvartik.backup_key", "kvartik.last_backup", "kvartik.trial_data"] {
            let _ = storage.remove_item(name);
        }
    }
    // База с записями образца — тоже: по их моментам пробный период
    // следующих тестов кончился бы (задача 99).
    crate::store::delete_database(crate::store::DB_NAME).await;
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
        rate: None,
        activity: None,
        income_kind: None,
    };
    let source = Document::create(data, now)
        .expect("черновик")
        .issue(&[], 0, Some(&profile()), Some(None), &client, now)
        .expect("выпущен");
    let recurring = Recurring::start(None, &source, date, now)
        .expect("годится")
        .expect("новый");
    let mut writes = Writes::default();
    writes.insert(&source);
    writes.insert(&recurring);
    commit(writes).await;

    let (host, mounted) = mount_at("#/documents");
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

    // Второе открытие черновиков не прибавляет. Ждать — счёт в списке:
    // заголовок экрана виден и до того, как база прочитана.
    let (host, mounted) = mount_at("#/documents");
    let number = source.number().map(|number| number.to_string()).unwrap_or_default();
    let named = fill(t(lang, Key::DocNumbered), &[t(lang, Key::KindInvoice), &number]);
    if !wait_for(&host, &named).await {
        wrong.push(format!("в списке нет «{named}»"));
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

/// Два устройства (задача 41). Слитые копии двух устройств, выпускавших
/// порознь, — два счёта с одним номером: список называет номер и ведёт
/// в настройки. На устройстве, где выпуск выключен, повтор черновика месяца
/// не создаёт, а форма черновика вместо «Выпустить» говорит, где выпуск, —
/// черновик сохраняется; выпуск, включённый на экране настроек, возвращает
/// «Выпустить» той же форме.
#[wasm_bindgen_test]
async fn a_device_without_issuing_keeps_drafts_and_names_repeated_numbers() {
    let client = fresh_base_with_client().await;
    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let today = clock::today().expect("часы с датой");
    let now = clock::now();
    let date = today.add_days(-40).expect("дата есть");
    let data = |kind| DocumentData {
        kind,
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
        rate: None,
        activity: None,
        income_kind: None,
    };
    // Каждое устройство выпустило свой первый счёт: номера одинаковые.
    let issue = || {
        Document::create(data(DocKind::Invoice), now)
            .expect("черновик")
            .issue(&[], 0, Some(&profile()), Some(None), &client, now)
            .expect("выпущен")
    };
    let (here, there) = (issue(), issue());
    let draft = Document::create(data(DocKind::Invoice), now).expect("черновик");
    let recurring = Recurring::start(None, &here, date, now)
        .expect("годится")
        .expect("новый");
    let mut writes = Writes::default();
    for document in [&here, &there, &draft] {
        writes.insert(document);
    }
    writes.insert(&recurring);
    commit(writes).await;
    assert!(crate::ui::settings::set_issue_elsewhere(None, true), "устройство не записало выпуск");

    let (host, mounted) = mount_at("#/documents");
    let mut wrong = Vec::new();
    let number = here.number().map(|number| number.to_string()).unwrap_or_default();
    let named = fill(t(lang, Key::DocNumbered), &[t(lang, Key::KindInvoice), &number]);
    let repeated = fill(t(lang, Key::DocsRepeated), &[&named]);
    if !wait_for(&host, &repeated).await {
        wrong.push(format!("нет «{repeated}»"));
    }
    // Список прочитан — повтор черновик здесь не создал.
    sleep(300).await;
    let (documents, _) = stored().await;
    if documents.iter().any(|document| document.data().recurring.is_some()) {
        wrong.push("повтор создал черновик там, где выпуск выключен".to_owned());
    }
    // Черновик в списке — строка со статусом «Черновик»: номера у него нет.
    match row_with(&host, "ul.documents:not(.deleted)", t(lang, Key::StatusDraft))
        .and_then(|row| row.query_selector("a.entry").ok().flatten())
    {
        Some(open) => open.unchecked_into::<HtmlElement>().click(),
        None => wrong.push("в списке нет черновика".to_owned()),
    }
    if !wait_for(&host, t(lang, Key::DocIssueElsewhere)).await {
        wrong.push("форма черновика не сказала, где выпуск".to_owned());
    }
    if button(&host, &[t(lang, Key::DocIssue)]).is_some() {
        wrong.push("«Выпустить» там, где выпуск выключен".to_owned());
    }
    if button(&host, &[t(lang, Key::DocSaveDraft)]).is_none() {
        wrong.push("нет «Сохранить черновик»".to_owned());
    }
    // Выпуск включают на экране настроек — форма, спрятанная за ним, снова
    // с «Выпустить».
    crate::platform::set_hash("#/settings");
    let switch = "#settings li.issuing input[role=switch]";
    if !wait_selector(&host, switch).await {
        wrong.push("на экране настроек нет переключателя выпуска".to_owned());
    } else if let Some(input) = host.query_selector(switch).ok().flatten() {
        let input: web_sys::HtmlInputElement = input.unchecked_into();
        if input.checked() {
            wrong.push("переключатель выпуска включён, а выпуск выключен".to_owned());
        }
        input.click();
    }
    if !wait_until(|| !Device::new().issue_elsewhere(DB_NAME)).await {
        wrong.push("включили выпуск — устройство не помнит".to_owned());
    }
    crate::platform::set_hash("#/documents");
    if !wait_until(|| button(&host, &[t(lang, Key::DocIssue)]).is_some()).await {
        wrong.push("выпуск включили — «Выпустить» не вернулось".to_owned());
    }
    // Выпуск выключен у другого ИП устройства (задача 116) — у этого форма
    // по-прежнему выпускает: она спрашивает о своей базе.
    let other = Ip::new(clock::today().expect("часы с датой"), clock::now());
    assert!(crate::ui::settings::set_issue_elsewhere(Some(other.base()), true), "устройство не записало выпуск");
    sleep(100).await;
    if button(&host, &[t(lang, Key::DocIssue)]).is_none() {
        wrong.push("выпуск выключили у другого ИП — а «Выпустить» пропало у этого".to_owned());
    }
    // Выпуск этого ИП выключили в другой вкладке — запись устройства есть,
    // а сигнал этой вкладки о ней не знает: «Выпустить» здесь не выпускает,
    // а форма встаёт со словами (ревью задачи 41).
    Device::new().set_issuing(&Issuing::default().with(other.base(), true).with(DB_NAME, true));
    match button(&host, &[t(lang, Key::DocIssue)]) {
        Some(issue) => issue.click(),
        None => wrong.push("нет «Выпустить»".to_owned()),
    }
    if !wait_for(&host, t(lang, Key::DocIssueElsewhere)).await {
        wrong.push("выпуск, выключенный в другой вкладке, — а слов нет".to_owned());
    }
    sleep(300).await;
    let (documents, _) = stored().await;
    if documents.iter().any(|document| document.id() == draft.id() && document.number().is_some()) {
        wrong.push("выпустился черновик, хотя выпуск выключили в другой вкладке".to_owned());
    }
    drop(mounted);
    host.remove();
    crate::ui::settings::set_issue_elsewhere(None, false);
    forget_device();
    assert!(wrong.is_empty(), "устройство без выпуска: {wrong:#?}");
}

/// Числа в форме документа читаются по языку окна, и при смене языка они
/// переписываются его записью, а не меняют смысла: «1,234.50» по-английски
/// по-русски — «1 234,50», а не одна целая с тысячными.
#[wasm_bindgen_test]
async fn numbers_follow_the_window_language() {
    let client = fresh_base_with_client().await;
    Device::new().set_ui_lang(Lang::En);
    let (host, mounted) = mount_at("#/documents");
    let mut wrong = Vec::new();
    if !wait_for(&host, t(Lang::En, Key::DocsAbout)).await {
        wrong.push("раздела документов нет".to_owned());
    }
    match button(&host, &[t(Lang::En, Key::DocNewInvoice)]) {
        Some(new) => new.click(),
        None => wrong.push("нет кнопки нового счёта".to_owned()),
    }
    sleep(100).await;
    if !pick_client(&host, client.data().name.as_str()) {
        wrong.push("клиента нет в выборе".to_owned());
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

/// Видимые строки списка документов — их текст.
fn listed_rows(host: &HtmlElement) -> Vec<String> {
    let Ok(rows) = host.query_selector_all("ul.documents:not(.deleted) a.entry") else {
        return Vec::new();
    };
    (0..rows.length())
        .filter_map(|at| rows.item(at))
        .filter_map(|row| row.text_content())
        .collect()
}

/// Экран документов по дизайну: поиск по клиенту и номеру и «все, счета,
/// акты» сужают список; документы — под заголовком своего месяца; строка
/// выпущенного открывает его карточку по своему адресу; «Отметить, что
/// оплачен» вносит остаток сегодняшним днём — статус «оплачен», в истории —
/// поступление; сумму сверх остатка и день из будущего форма называет до
/// записи, а переплату — карточка; «назад» возвращает к списку.
#[wasm_bindgen_test]
async fn documents_are_found_opened_and_marked_paid() {
    let client = fresh_base_with_client().await;
    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let today = clock::today().expect("часы с датой");
    let now = clock::now();
    let data = |kind: DocKind, price: i64| DocumentData {
        kind,
        client: client.id(),
        lang: Lang::En,
        currency: Currency::Usd,
        date: today,
        service_date: today,
        due: None,
        items: vec![Item {
            text: line("Software development"),
            quantity_e3: 1_000,
            price,
        }],
        basis: None,
        recurring: None,
        rate: None,
        activity: None,
        income_kind: None,
    };
    let issue = |kind: DocKind, price: i64, before: &[Document]| {
        Document::create(data(kind, price), now)
            .expect("черновик")
            .issue(before, 0, Some(&profile()), Some(None), &client, now)
            .expect("выпущен")
    };
    let first = issue(DocKind::Invoice, 240_000, &[]);
    let second = issue(DocKind::Invoice, 100_000, std::slice::from_ref(&first));
    let act = issue(DocKind::Act, 240_000, &[]);
    let mut writes = Writes::default();
    for document in [&first, &second, &act] {
        writes.insert(document);
    }
    commit(writes).await;

    let (host, mounted) = mount_at("#/documents");
    let mut wrong = Vec::new();
    let named = |document: &Document| {
        let kind = match document.data().kind {
            DocKind::Invoice => Key::KindInvoice,
            DocKind::Act => Key::KindAct,
        };
        let number = document.number().map(|number| number.to_string()).unwrap_or_default();
        fill(t(lang, Key::DocNumbered), &[t(lang, kind), &number])
    };
    if !wait_for(&host, &named(&first)).await {
        wrong.push("документов в списке нет".to_owned());
    }
    let month = i18n::month_heading(lang, today.year(), today.month(), today.year());
    let headings = host.query_selector_all("h2.month").map(|list| list.length()).unwrap_or(0);
    if headings != 1 || !host.text_content().unwrap_or_default().contains(&month) {
        wrong.push(format!("заголовков месяца {headings}, а не один «{month}»"));
    }

    let search = host
        .query_selector(".docs-tools input[type=search]")
        .ok()
        .flatten()
        .map(JsCast::unchecked_into::<web_sys::HtmlInputElement>);
    let number = second.number().map(|number| number.to_string()).unwrap_or_default();
    for (query, expected) in [(number.as_str(), 1), ("acme", 3), ("zzz", 0), ("", 3)] {
        match &search {
            Some(input) => type_into(input, query),
            None => wrong.push("нет поиска".to_owned()),
        }
        sleep(50).await;
        let rows = listed_rows(&host);
        if rows.len() != expected {
            wrong.push(format!("поиск «{query}»: {rows:?}"));
        }
        let none = host.text_content().unwrap_or_default().contains(t(lang, Key::DocsNotFound));
        if none != (expected == 0) {
            wrong.push(format!("поиск «{query}»: «ничего не нашлось» — {none}"));
        }
    }
    for (filter, expected) in [(Key::FilterActs, 1), (Key::NavDocuments, 2), (Key::FilterAll, 3)] {
        match button(&host, &[t(lang, filter)]) {
            Some(choice) => choice.click(),
            None => wrong.push(format!("нет выбора {filter:?}")),
        }
        sleep(50).await;
        if listed_rows(&host).len() != expected {
            wrong.push(format!("{filter:?}: {:?}", listed_rows(&host)));
        }
    }

    match row_with(&host, "ul.documents:not(.deleted)", &named(&first))
        .and_then(|row| row.query_selector("a.entry").ok().flatten())
    {
        Some(open) => open.unchecked_into::<HtmlElement>().click(),
        None => wrong.push("нет строки первого счёта".to_owned()),
    }
    sleep(200).await;
    let address = crate::platform::hash();
    if address != format!("#/documents/{}", first.id()) {
        wrong.push(format!("адрес карточки — {address}"));
    }
    if host.query_selector(".docs.opened .docs-detail").ok().flatten().is_none() {
        wrong.push("карточка не открылась".to_owned());
    }
    match button(&host, &[t(lang, Key::DocMarkPaid)]) {
        Some(paid) => paid.click(),
        None => wrong.push("нет «Отметить, что оплачен»".to_owned()),
    }
    if !wait_for(&host, t(lang, Key::PaymentSaved)).await {
        wrong.push("оплата не подтвердилась".to_owned());
    }
    let detail = host
        .query_selector(".docs-detail")
        .ok()
        .flatten()
        .and_then(|detail| detail.text_content())
        .unwrap_or_default();
    let payment = fill(t(lang, Key::HistoryPaid), &[&i18n::money(lang, Money::new(240_000, Currency::Usd))]);
    if !detail.contains(t(lang, Key::StatusPaid)) || !detail.contains(&payment) {
        wrong.push(format!("в карточке нет «{}» и «{payment}»", t(lang, Key::StatusPaid)));
    }
    let (documents, _) = stored().await;
    match documents.iter().find(|d| d.id() == first.id()).map(Document::payments) {
        Some([paid]) if paid.date == today && paid.amount == 240_000 => {}
        other => wrong.push(format!("оплаты в базе: {other:?}")),
    }

    // Оплаченному целиком любая сумма — сверх остатка, а день через месяц
    // ещё не наступил: форма говорит это до записи, а записать не мешает, и
    // переплата — строкой в карточке (полный прогон 0.0.96: молчала).
    let ahead = today.add_days(30).map(Date::iso).unwrap_or_default();
    for (label, value) in [(Key::PaymentDate, ahead.as_str()), (Key::PaymentAmount, "10")] {
        match field(&host, t(lang, label), 0) {
            Some(input) => type_into(&input, value),
            None => wrong.push(format!("нет поля {label:?}")),
        }
    }
    sleep(100).await;
    let ten = i18n::money(lang, Money::new(1_000, Currency::Usd));
    let said = host.text_content().unwrap_or_default();
    for doubt in [fill(t(lang, Key::PaymentOver), &[&ten]), t(lang, Key::PaymentAhead).to_owned()] {
        if !said.contains(&doubt) {
            wrong.push(format!("форма оплаты не сказала «{doubt}»"));
        }
    }
    match button(&host, &[t(lang, Key::PaymentAdd)]) {
        Some(add) => add.click(),
        None => wrong.push("нет «Отметить оплату»".to_owned()),
    }
    if !wait_for(&host, &fill(t(lang, Key::PaymentOverpaid), &[&ten])).await {
        wrong.push("переплата не названа в карточке".to_owned());
    }

    match host.query_selector(".docs-detail a.back").ok().flatten() {
        Some(back) => back.unchecked_into::<HtmlElement>().click(),
        None => wrong.push("у карточки нет «назад»".to_owned()),
    }
    sleep(200).await;
    if crate::platform::hash() != "#/documents" || host.query_selector(".docs.opened").ok().flatten().is_some() {
        wrong.push(format!("«назад» не вернул к списку: {}", crate::platform::hash()));
    }
    drop(mounted);
    host.remove();
    forget_device();
    assert!(wrong.is_empty(), "экран документов: {wrong:#?}");
}

/// Аннулированный счёт ни о чём не просит: в карточке нет ни «Оплатить до»,
/// ни «Осталось оплатить», а поступление, которое было, видно. У действующего
/// такого же счёта есть оба (полный прогон 0.0.96: аннулированный просил
/// оплатить остаток до срока).
#[wasm_bindgen_test]
async fn an_annulled_invoice_asks_for_no_payment() {
    let client = fresh_base_with_client().await;
    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let today = clock::today().expect("часы с датой");
    let now = clock::now();
    let data = DocumentData {
        due: today.add_days(15),
        ..invoice_data(&client)
    };
    let issue = |before: &[Document]| {
        Document::create(data.clone(), now)
            .expect("черновик")
            .issue(before, 0, Some(&profile()), Some(None), &client, now)
            .expect("выпущен")
            .add_payment(Payment { date: today, amount: 100_000 }, now)
            .expect("оплата отмечена")
    };
    let live = issue(&[]);
    let annulled = issue(std::slice::from_ref(&live))
        .annul(today, now)
        .expect("выпущен")
        .expect("аннулирован");
    let mut writes = Writes::default();
    writes.insert(&live);
    writes.insert(&annulled);
    commit(writes).await;

    let paid = i18n::money(lang, Money::new(100_000, Currency::Usd));
    let left = fill(t(lang, Key::PaymentLeft), &[&i18n::money(lang, Money::new(140_000, Currency::Usd))]);
    let mut wrong = Vec::new();
    for (document, asks) in [(&live, true), (&annulled, false)] {
        let address = Screen::Document(document.id()).href();
        let (host, mounted) = mount_at(&address);
        if !wait_selector(&host, ".docs-detail fieldset.payments").await {
            wrong.push(format!("карточка {:?} не открылась", document.number()));
        }
        let detail = host
            .query_selector(".docs-detail")
            .ok()
            .flatten()
            .and_then(|detail| detail.text_content())
            .unwrap_or_default();
        let due = detail.contains(t(lang, Key::DocDue));
        if due != asks || detail.contains(&left) != asks {
            wrong.push(format!(
                "{}: «Оплатить до» — {due}, «{left}» — {}, а ждали {asks}",
                if asks { "действующий" } else { "аннулированный" },
                detail.contains(&left)
            ));
        }
        if !detail.contains(&paid) {
            wrong.push(format!("поступления {paid} не видно"));
        }
        drop(mounted);
        host.remove();
    }
    forget_device();
    assert!(wrong.is_empty(), "аннулированный счёт: {wrong:#?}");
}

/// «Убрать» у оплаты говорит, что оплата убрана, а «Вернуть» рядом
/// возвращает её — и тоже словами (полный прогон 0.0.96: «Убрать» молчал,
/// а над карточкой оставалось прежнее «Оплата отмечена.»).
#[wasm_bindgen_test]
async fn a_removed_payment_is_said_and_can_be_returned() {
    let client = fresh_base_with_client().await;
    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let today = clock::today().expect("часы с датой");
    let now = clock::now();
    let payment = Payment { date: today, amount: 100_000 };
    let invoice = Document::create(invoice_data(&client), now)
        .expect("черновик")
        .issue(&[], 0, Some(&profile()), Some(None), &client, now)
        .expect("выпущен")
        .add_payment(payment, now)
        .expect("оплата отмечена");
    let mut writes = Writes::default();
    writes.insert(&invoice);
    commit(writes).await;
    let stored_payments = || async {
        let (documents, _) = stored().await;
        documents
            .iter()
            .find(|document| document.id() == invoice.id())
            .map(|document| document.payments().to_vec())
    };

    let address = Screen::Document(invoice.id()).href();
    let (host, mounted) = mount_at(&address);
    let mut wrong = Vec::new();
    let line = fill(
        t(lang, Key::PaymentLine),
        &[&i18n::date(lang, today), &i18n::money(lang, Money::new(100_000, Currency::Usd))],
    );
    if wait_for(&host, &line).await {
        match button(&host, &[t(lang, Key::Remove)]) {
            Some(remove) => remove.click(),
            None => wrong.push("у оплаты нет «Убрать»".to_owned()),
        }
        if !wait_for(&host, t(lang, Key::PaymentRemoved)).await {
            wrong.push("«Убрать» не сказал, что оплата убрана".to_owned());
        }
        if stored_payments().await != Some(Vec::new()) {
            wrong.push(format!("после «Убрать» в базе оплаты {:?}", stored_payments().await));
        }
        match button(&host, &[t(lang, Key::DocUnannul)]) {
            Some(back) => back.click(),
            None => wrong.push("рядом со словами нет «Вернуть»".to_owned()),
        }
        if !wait_for(&host, t(lang, Key::PaymentReturned)).await {
            wrong.push("«Вернуть» не сказал, что оплата возвращена".to_owned());
        }
        if stored_payments().await != Some(vec![payment]) {
            wrong.push(format!("после «Вернуть» в базе оплаты {:?}", stored_payments().await));
        }
    } else {
        wrong.push(format!("в карточке нет оплаты «{line}»"));
    }
    drop(mounted);
    host.remove();
    forget_device();
    assert!(wrong.is_empty(), "убранная оплата: {wrong:#?}");
}

/// Файлы шрифтов документа — те же байты, что лежат в `public/fonts/`:
/// раннер тестов раздаёт только свой wasm, а не папку шрифтов сайта.
const FONT_FILES: [(&str, &[u8]); 11] = [
    ("noto-sans-latin-400.ttf", include_bytes!("../../../public/fonts/noto-sans-latin-400.ttf")),
    ("noto-sans-latin-ext-400.ttf", include_bytes!("../../../public/fonts/noto-sans-latin-ext-400.ttf")),
    ("noto-sans-cyrillic-400.ttf", include_bytes!("../../../public/fonts/noto-sans-cyrillic-400.ttf")),
    ("noto-sans-armenian-400.ttf", include_bytes!("../../../public/fonts/noto-sans-armenian-400.ttf")),
    ("noto-sans-latin-600.ttf", include_bytes!("../../../public/fonts/noto-sans-latin-600.ttf")),
    ("noto-sans-latin-ext-600.ttf", include_bytes!("../../../public/fonts/noto-sans-latin-ext-600.ttf")),
    ("noto-sans-cyrillic-600.ttf", include_bytes!("../../../public/fonts/noto-sans-cyrillic-600.ttf")),
    ("noto-sans-armenian-600.ttf", include_bytes!("../../../public/fonts/noto-sans-armenian-600.ttf")),
    ("noto-serif-latin-800.ttf", include_bytes!("../../../public/fonts/noto-serif-latin-800.ttf")),
    ("noto-serif-cyrillic-800.ttf", include_bytes!("../../../public/fonts/noto-serif-cyrillic-800.ttf")),
    ("noto-serif-armenian-800.ttf", include_bytes!("../../../public/fonts/noto-serif-armenian-800.ttf")),
];

/// Ответ подмены `fetch`.
type Answer = Closure<dyn Fn(JsValue) -> Promise>;

/// Подмена `fetch` на время теста: на `fonts/<имя>` — байты файла из
/// [`FONT_FILES`], на остальное — сетевая ошибка. Окно берёт шрифты
/// документа у своего сайта (`platform::site`), и так тест проходит весь
/// путь PDF в настоящем браузере. При сбросе прежний `fetch` возвращается.
struct FontServer {
    original: JsValue,
    _answer: Answer,
}

impl FontServer {
    fn install() -> Self {
        let window = web_sys::window().expect("есть окно");
        let original = Reflect::get(&window, &"fetch".into()).expect("есть fetch");
        let answer = Answer::new(|input: JsValue| {
            let url = input.as_string().unwrap_or_default();
            let name = url.rsplit('/').next().unwrap_or_default();
            let response = match FONT_FILES.iter().find(|(file, _)| *file == name) {
                Some((_, bytes)) => Response::new_with_opt_u8_array(Some(&mut bytes.to_vec()))
                    .unwrap_or_else(|_| Response::error()),
                None => Response::error(),
            };
            Promise::resolve(&JsValue::from(response))
        });
        Reflect::set(&window, &"fetch".into(), answer.as_ref()).expect("fetch подменён");
        Self {
            original,
            _answer: answer,
        }
    }

    /// Байты по адресу — настоящим `fetch`: адрес `blob:` подмена не знает.
    async fn bytes(&self, url: &str) -> Option<Vec<u8>> {
        let window = web_sys::window()?;
        let fetch: Function = self.original.clone().dyn_into().ok()?;
        let promise: Promise = fetch.call1(&window, &url.into()).ok()?.dyn_into().ok()?;
        let response: Response = promise.await.ok()?.dyn_into().ok()?;
        let buffer = response.array_buffer().ok()?.await.ok()?;
        Some(Uint8Array::new(&buffer).to_vec())
    }
}

impl Drop for FontServer {
    fn drop(&mut self) {
        if let Some(window) = web_sys::window() {
            let _ = Reflect::set(&window, &"fetch".into(), &self.original);
        }
    }
}

/// Подождать элемент по селектору; `false` — не дождались за пять секунд.
/// Подождать, пока `ready` не станет правдой; `false` — не дождались за пять
/// секунд. Для состояния, которое приходит событием своей задачей, — карточки
/// по адресу после сообщения (Правило 2: тест ждёт состояния, а не времени).
pub(super) async fn wait_until(ready: impl Fn() -> bool) -> bool {
    for _ in 0..100 {
        if ready() {
            return true;
        }
        sleep(50).await;
    }
    false
}

pub(super) async fn wait_selector(host: &HtmlElement, selector: &str) -> bool {
    for _ in 0..100 {
        if host.query_selector(selector).ok().flatten().is_some() {
            return true;
        }
        sleep(50).await;
    }
    false
}

/// Счёт клиенту на английском, сегодняшним днём, на 2 400 долларов.
pub(super) fn invoice_data(client: &Client) -> DocumentData {
    let today = clock::today().expect("часы с датой");
    DocumentData {
        kind: DocKind::Invoice,
        client: client.id(),
        lang: Lang::En,
        currency: Currency::Usd,
        date: today,
        service_date: today,
        due: None,
        items: vec![Item {
            text: line("Software development"),
            quantity_e3: 1_000,
            price: 240_000,
        }],
        basis: None,
        recurring: None,
        rate: None,
        activity: None,
        income_kind: None,
    }
}

/// PDF и превью (задача 12): у выпущенного счёта «PDF» показывает его
/// страницы на языке документа и даёт скачать файл PDF с понятным именем,
/// «Скрыть» убирает их; у черновика на шаге проверки «Как будет выглядеть»
/// показывает страницы без файла и говорит, что номер — при выпуске. Весь
/// путь — ядро документа под wasm32, шрифты у сайта, файл и картинка.
#[wasm_bindgen_test]
async fn issued_document_gives_a_pdf_and_a_draft_a_preview() {
    let client = fresh_base_with_client().await;
    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let now = clock::now();
    let data = invoice_data(&client);
    let invoice = Document::create(data.clone(), now)
        .expect("черновик")
        .issue(&[], 0, Some(&profile()), Some(None), &client, now)
        .expect("выпущен");
    let draft = Document::create(data, now).expect("черновик");
    let mut writes = Writes::default();
    writes.insert(&invoice);
    writes.insert(&draft);
    commit(writes).await;
    let fonts = FontServer::install();

    let address = format!("#/documents/{}", invoice.id());
    let (host, mounted) = mount_at(&address);
    let mut wrong = Vec::new();
    let number = invoice.number().map(|number| number.to_string()).unwrap_or_default();
    if !wait_for(&host, &number).await {
        wrong.push("карточка счёта не открылась".to_owned());
    }
    match button(&host, &[t(lang, Key::DocPdf)]) {
        Some(pdf) => pdf.click(),
        None => wrong.push("в карточке нет «PDF»".to_owned()),
    }
    if wait_selector(&host, ".paper .paper-page").await {
        let page = host
            .query_selector(".paper .paper-page")
            .ok()
            .flatten()
            .and_then(|page| page.text_content())
            .unwrap_or_default();
        let total = i18n::money(Lang::En, Money::new(240_000, Currency::Usd));
        for piece in [t(Lang::En, Key::KindInvoice), "IE Aram Petrosyan", "Acme Inc.", total.as_str()] {
            if !page.contains(piece) {
                wrong.push(format!("на странице нет «{piece}»"));
            }
        }
    } else {
        let said = host.query_selector(".paper").ok().flatten().and_then(|p| p.text_content());
        wrong.push(format!("страниц нет: {said:?}"));
    }
    match host.query_selector(".paper a[download]").ok().flatten() {
        Some(link) => {
            let name = link.get_attribute("download").unwrap_or_default();
            if name != format!("Invoice-{number}.pdf") {
                wrong.push(format!("имя файла — {name}"));
            }
            let href = link.get_attribute("href").unwrap_or_default();
            let bytes = fonts.bytes(&href).await.unwrap_or_default();
            if !bytes.starts_with(b"%PDF-1.7") || bytes.len() < 5_000 {
                wrong.push(format!("файл PDF — {} байтов, начало {:?}", bytes.len(), bytes.get(..8)));
            }
        }
        None => wrong.push("нет «Скачать PDF»".to_owned()),
    }
    if host.query_selector(".paper [role=alert]").ok().flatten().is_some() {
        wrong.push("у PDF жалоба".to_owned());
    }
    match button(&host, &[t(lang, Key::PdfHide)]) {
        Some(hide) => hide.click(),
        None => wrong.push("нет «Скрыть»".to_owned()),
    }
    sleep(100).await;
    if host.query_selector(".paper").ok().flatten().is_some() {
        wrong.push("«Скрыть» не убрал страницы".to_owned());
    }

    // Черновик: строка списка открывает форму, а в ней — «Как будет выглядеть».
    crate::platform::set_hash("#/documents");
    sleep(200).await;
    match host.query_selector("a.entry[href=\"#/documents\"]").ok().flatten() {
        Some(entry) => entry.unchecked_into::<HtmlElement>().click(),
        None => wrong.push("в списке нет черновика".to_owned()),
    }
    sleep(200).await;
    match button(&host, &[t(lang, Key::DocPreview)]) {
        Some(preview) => preview.click(),
        None => wrong.push("в форме черновика нет «Как будет выглядеть»".to_owned()),
    }
    if !wait_selector(&host, ".paper .paper-page").await {
        wrong.push("у черновика нет страниц".to_owned());
    }
    if host.query_selector(".paper a[download]").ok().flatten().is_some() {
        wrong.push("у черновика — файл PDF".to_owned());
    }
    if !host.text_content().unwrap_or_default().contains(t(lang, Key::PdfDraftNote)) {
        wrong.push("у черновика нет слов о номере".to_owned());
    }
    drop(mounted);
    drop(fonts);
    host.remove();
    forget_device();
    assert!(wrong.is_empty(), "PDF и превью: {wrong:#?}");
}

/// Поставить профилю логотип `image` или, без картинки, убрать его —
/// командой ядра, как форма профиля.
async fn put_logo(store: &Store, profile: &Profile, image: Option<Image>) {
    let tx = store.write().expect("транзакция записи");
    let existing = tx.get::<Logo>(profile.id()).await.expect("логотип читается");
    let now = clock::now();
    let logo = match image {
        Some(image) => Logo::set(existing.clone(), profile, image, now),
        None => existing.clone().expect("убирают поставленный").remove(now),
    };
    let mut writes = Writes::default();
    if existing.is_some() {
        writes.update(&logo);
    } else {
        writes.insert(&logo);
    }
    tx.commit(writes).await.expect("записано");
}

/// Когда менялась запись картинки выпуска `id`; `None` — её нет или она не
/// читается.
async fn kept_at(store: &Store, id: RecordId) -> Option<Timestamp> {
    let tx = store.read().expect("транзакция чтения");
    tx.get::<IssuedLogo>(id).await.ok().flatten().map(|kept| kept.meta().modified())
}

/// Испортить запись: под ключом `[вид, id]` — строка, которая не JSON.
async fn spoil(store: &Store, kind: &str, id: RecordId) {
    let key: JsValue = Array::of2(&kind.into(), &id.to_string().into()).into();
    crate::store::tests::put_raw(store, &key, "не JSON").await;
}

/// Профиль базы, которую завёл [`fresh_base_with_client`].
async fn owner(store: &Store) -> Profile {
    let tx = store.read().expect("транзакция чтения");
    Profile::current(tx.all::<Profile>().await.expect("профили читаются")).expect("профиль есть")
}

/// Что выпуск помнит о логотипе (задача 63) — выпуском окна (`save`).
/// Картинку логотипа он кладёт записью под её отпечатком один раз: второй
/// выпуск с той же картинкой её не переписывает и не спотыкается о занятый
/// ключ, а испорченную запись под отпечатком первый чинит. С убранным
/// логотипом документ — «без логотипа»; с картинкой, которую PDF не возьмёт
/// (больше 2048 точек на сторону), и с нечитаемой записью логотипа —
/// «неизвестно», и такой картинки он не кладёт.
#[wasm_bindgen_test]
async fn issue_remembers_the_logo_of_its_day() {
    let client = fresh_base_with_client().await;
    let store = Store::open(|_| {}).await.expect("база открыта");
    let owner = owner(&store).await;
    let old = png(2, 2).await;
    let image = |bytes: &[u8]| Image::new(bytes.to_vec()).expect("годная картинка");
    let print = IssuedLogo::id_of(&image(&old));
    let mut wrong = Vec::new();

    put_logo(&store, &owner, Some(image(&old))).await;
    spoil(&store, IssuedLogo::KIND, print).await;
    let first = save(&store, None, invoice_data(&client), true).await;
    let once = kept_at(&store, print).await;
    let second = save(&store, None, invoice_data(&client), true).await;
    if once.is_none() || kept_at(&store, print).await != once {
        wrong.push(format!("картинка выпуска не починена или переписана вторым выпуском: {once:?}"));
    }
    put_logo(&store, &owner, None).await;
    let bare = save(&store, None, invoice_data(&client), true).await;
    // Пустой холст в 2100 точек — лёгкий файл, а PDF его не возьмёт.
    let huge = png(2_100, 2_100).await;
    put_logo(&store, &owner, Some(image(&huge))).await;
    let oversized = save(&store, None, invoice_data(&client), true).await;
    spoil(&store, Logo::KIND, owner.id()).await;
    let unread = save(&store, None, invoice_data(&client), true).await;

    let mark = |issued: &Result<Document, _>| issued.as_ref().ok().and_then(Document::issued).map(Issued::logo);
    for (what, issued, expected) in [
        ("первый", &first, Some(Some(print))),
        ("второй с той же картинкой", &second, Some(Some(print))),
        ("с убранным логотипом", &bare, Some(None)),
        ("с логотипом больше PDF", &oversized, None),
        ("с нечитаемой записью логотипа", &unread, None),
    ] {
        if mark(issued) != Some(expected) {
            wrong.push(format!("{what}: {issued:?}"));
        }
    }
    let kept = {
        let tx = store.read().expect("транзакция чтения");
        tx.list::<IssuedLogo>().await.expect("логотипы выпуска читаются").records
    };
    if kept.len() != 1 || kept[0].image().bytes() != old.as_slice() || kept[0].meta().id() != print {
        wrong.push(format!("картинок выпуска {}, а нужна одна — первая", kept.len()));
    }
    forget_device();
    assert!(wrong.is_empty(), "выпуск и логотип: {wrong:#?}");
}

/// PDF — с логотипом дня выпуска (задача 63), хотя в профиле уже другой.
/// У документа, выпущенного до задачи 63, — нынешний, у «без логотипа»
/// картинки нет, а пропавшую или нечитаемую картинку дня выпуска окно
/// называет словами и нынешней не подменяет. Превью черновика — с нынешним:
/// что в превью, то и в PDF выпущенного. Документы — командами ядра.
#[wasm_bindgen_test]
async fn pdf_shows_the_logo_of_its_day() {
    let client = fresh_base_with_client().await;
    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let store = Store::open(|_| {}).await.expect("база открыта");
    let owner = owner(&store).await;
    let (old, new) = (png(2, 2).await, png(3, 3).await);
    let image = |bytes: &[u8]| Image::new(bytes.to_vec()).expect("годная картинка");
    put_logo(&store, &owner, Some(image(&new))).await;

    let now = clock::now();
    let kept = IssuedLogo::keep(None, &image(&old), now).expect("новая картинка выпуска");
    let rotten = RecordId::random();
    let mut documents: Vec<Document> = Vec::new();
    for logo in [
        Some(Some(kept.meta().id())),
        Some(None),
        None,
        Some(Some(RecordId::random())),
        Some(Some(rotten)),
    ] {
        let issued = Document::create(invoice_data(&client), now)
            .expect("черновик")
            .issue(&documents, 0, Some(&profile()), logo, &client, now)
            .expect("выпущен");
        documents.push(issued);
    }
    let draft = Document::create(invoice_data(&client), now).expect("черновик");
    let mut writes = Writes::default();
    writes.insert(&kept);
    writes.insert(&draft);
    for document in &documents {
        writes.insert(document);
    }
    commit(writes).await;
    spoil(&store, IssuedLogo::KIND, rotten).await;

    let fonts = FontServer::install();
    let skipped = t(lang, Key::PdfIssuedLogoSkipped);
    let mut wrong = Vec::new();
    let pictured = |host: &HtmlElement| {
        host.query_selector(".paper .paper-page image")
            .ok()
            .flatten()
            .and_then(|image| image.get_attribute("href"))
    };
    for (what, document, logo, alert) in [
        ("с картинкой дня выпуска", &documents[0], Some(&old), None),
        ("без логотипа", &documents[1], None, None),
        ("до задачи 63 — нынешний", &documents[2], Some(&new), None),
        ("картинка пропала — словами", &documents[3], None, Some(skipped)),
        ("картинка не читается — словами", &documents[4], None, Some(skipped)),
    ] {
        let number = document.number().map(|number| number.to_string()).unwrap_or_default();
        let address = format!("#/documents/{}", document.id());
        let (host, mounted) = mount_at(&address);
        if !wait_for(&host, &number).await || !open_paper(&host, lang).await {
            wrong.push(format!("{what}: PDF не собрался"));
        }
        let shown = match pictured(&host) {
            Some(href) => fonts.bytes(&href).await,
            None => None,
        };
        if shown.as_ref() != logo || paper_alert(&host).as_deref() != alert {
            wrong.push(format!(
                "{what}: картинка {:?} Б, жалоба {:?}",
                shown.map(|bytes| bytes.len()),
                paper_alert(&host)
            ));
        }
        drop(mounted);
        host.remove();
    }

    // Черновик: строка списка открывает форму, а в ней — «Как будет выглядеть».
    let (host, mounted) = mount_at("#/documents");
    let draft_row = "a.entry[href=\"#/documents\"]";
    if !wait_selector(&host, draft_row).await {
        wrong.push("в списке нет черновика".to_owned());
    }
    if let Some(entry) = host.query_selector(draft_row).ok().flatten() {
        entry.unchecked_into::<HtmlElement>().click();
    }
    if !wait_until(|| button(&host, &[t(lang, Key::DocPreview)]).is_some()).await {
        wrong.push("в форме черновика нет «Как будет выглядеть»".to_owned());
    }
    if let Some(preview) = button(&host, &[t(lang, Key::DocPreview)]) {
        preview.click();
    }
    if !wait_selector(&host, ".paper .paper-page").await {
        wrong.push("у черновика нет страниц".to_owned());
    }
    let shown = match pictured(&host) {
        Some(href) => fonts.bytes(&href).await,
        None => None,
    };
    if shown.as_ref() != Some(&new) {
        wrong.push(format!("превью черновика: картинка {:?} Б", shown.map(|bytes| bytes.len())));
    }
    drop(mounted);
    host.remove();
    drop(fonts);
    forget_device();
    assert!(wrong.is_empty(), "логотип в PDF: {wrong:#?}");
}

/// Лёгкий логотип больше 2048 точек на сторону форма профиля уменьшает
/// сама, как тяжёлый файл: такой PDF не берёт (найдено ревью задачи 63).
/// Логотип в пределе ложится как есть.
#[wasm_bindgen_test]
async fn a_logo_beyond_the_pdf_limit_is_made_smaller() {
    let huge = png(2_100, 2_100).await;
    let small = png(3, 3).await;
    let mut wrong = Vec::new();
    if huge.len() > logo::MAX_BYTES {
        wrong.push(format!("пустой холст весит {} Б — не лёгкий", huge.len()));
    }
    match fitted(huge, "image/png").await {
        Ok(image) if Picture::fits(&image) => {}
        other => wrong.push(format!("больше 2048 точек: {:?}", other.map(|image| image.bytes().len()))),
    }
    match fitted(small.clone(), "image/png").await {
        Ok(image) if image.bytes() == small.as_slice() => {}
        other => wrong.push(format!("в пределе: {other:?}")),
    }
    assert!(wrong.is_empty(), "логотип формы: {wrong:#?}");
}

/// PDF выпущенного документа профиль не нужен: реквизиты ИП — в самом
/// документе (найдено ревью задачи 63). Запись профиля не читается — PDF
/// всё равно собирается: у документа с известным логотипом выпуска — как
/// обычно, у выпущенного до задачи 63 — без логотипа и со словами об этом.
/// Раньше оба не собирались: «документ не прочитался».
#[wasm_bindgen_test]
async fn issued_pdf_needs_no_profile() {
    let client = fresh_base_with_client().await;
    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let store = Store::open(|_| {}).await.expect("база открыта");
    let owner = owner(&store).await;
    let now = clock::now();
    let issue = |documents: &[Document], logo| {
        Document::create(invoice_data(&client), now)
            .expect("черновик")
            .issue(documents, 0, Some(&profile()), logo, &client, now)
            .expect("выпущен")
    };
    let bare = issue(&[], Some(None));
    let unknown = issue(std::slice::from_ref(&bare), None);
    let mut writes = Writes::default();
    writes.insert(&bare);
    writes.insert(&unknown);
    commit(writes).await;
    spoil(&store, Profile::KIND, owner.id()).await;

    let fonts = FontServer::install();
    let mut wrong = Vec::new();
    for (what, document, alert) in [
        ("без логотипа", &bare, None),
        ("выпущенный до задачи 63", &unknown, Some(t(lang, Key::PdfLogoSkipped))),
    ] {
        let number = document.number().map(|number| number.to_string()).unwrap_or_default();
        let address = format!("#/documents/{}", document.id());
        let (host, mounted) = mount_at(&address);
        if !wait_for(&host, &number).await || !open_paper(&host, lang).await {
            wrong.push(format!("{what}: PDF не собрался — {:?}", host.query_selector(".paper").ok().flatten().and_then(|p| p.text_content())));
        } else if paper_alert(&host).as_deref() != alert {
            wrong.push(format!("{what}: жалоба {:?}", paper_alert(&host)));
        }
        drop(mounted);
        host.remove();
    }
    drop(fonts);
    forget_device();
    assert!(wrong.is_empty(), "PDF без профиля: {wrong:#?}");
}

/// Открыть страницы документа кнопкой «PDF»; `false` — файла не дождались.
async fn open_paper(host: &HtmlElement, lang: Lang) -> bool {
    match button(host, &[t(lang, Key::DocPdf)]) {
        Some(pdf) => pdf.click(),
        None => return false,
    }
    wait_selector(host, ".paper a[download]").await
}

/// Что на панели документа сказано тревогой.
fn paper_alert(host: &HtmlElement) -> Option<String> {
    host.query_selector(".paper [role=alert]")
        .ok()
        .flatten()
        .and_then(|alert| alert.text_content())
}

/// «Отправить PDF» (задача 13): где браузер умеет «Поделиться», кнопка
/// отдаёт меню один файл PDF с понятным именем — в самом нажатии, до
/// первого `await`, иначе Safari и Chrome откажут за несвежестью нажатия.
/// Отмена меню молчит, отказ говорит словами; где «Поделиться» нет — одна
/// «Скачать PDF».
#[wasm_bindgen_test]
async fn issued_document_is_sent_by_the_share_menu() {
    let client = fresh_base_with_client().await;
    let lang = Lang::Am;
    Device::new().set_ui_lang(lang);
    let now = clock::now();
    let invoice = Document::create(invoice_data(&client), now)
        .expect("черновик")
        .issue(&[], 0, Some(&profile()), Some(None), &client, now)
        .expect("выпущен");
    let mut writes = Writes::default();
    writes.insert(&invoice);
    commit(writes).await;
    let fonts = FontServer::install();
    let sharing = ShareStub::install();

    let address = format!("#/documents/{}", invoice.id());
    let (host, mounted) = mount_at(&address);
    let mut wrong = Vec::new();
    let number = invoice.number().map(|number| number.to_string()).unwrap_or_default();
    if !wait_for(&host, &number).await {
        wrong.push("карточка счёта не открылась".to_owned());
    }
    if !open_paper(&host, lang).await {
        wrong.push("файла PDF нет".to_owned());
    }
    let send = button(&host, &[t(lang, Key::PdfShare)]);
    match &send {
        Some(send) => {
            send.click();
            // Ни одного `await` после нажатия: меню открыто в нём самом.
            if sharing.calls() != 1 {
                wrong.push(format!("в самом нажатии share позван {} раз", sharing.calls()));
            }
        }
        None => wrong.push("нет «Отправить PDF»".to_owned()),
    }
    if let Some((keys, files)) = sharing.shared(0) {
        if keys != ["files"] {
            wrong.push(format!("отдано меню: {keys:?}"));
        }
        match files.as_slice() {
            [file] => {
                if file.name() != format!("Invoice-{number}.pdf") {
                    wrong.push(format!("имя файла — {}", file.name()));
                }
                if file.type_() != "application/pdf" || file.size() < 5_000.0 {
                    wrong.push(format!("файл — {} байтов, тип {}", file.size(), file.type_()));
                }
            }
            _ => wrong.push(format!("файлов отдано: {}", files.len())),
        }
    }
    sleep(100).await;
    if let Some(said) = paper_alert(&host) {
        wrong.push(format!("после отправки — жалоба: {said}"));
    }
    match host.query_selector(".paper a[download]").ok().flatten() {
        Some(link) if link.class_name() != "button secondary" => {
            wrong.push(format!("«Скачать PDF» рядом с «Отправить» — {}", link.class_name()));
        }
        _ => {}
    }

    // Человек закрыл меню — говорить не о чем.
    sharing.answer.set(Some("AbortError"));
    if let Some(send) = &send {
        send.click();
    }
    sleep(100).await;
    if let Some(said) = paper_alert(&host) {
        wrong.push(format!("после отмены — жалоба: {said}"));
    }
    // Браузер отказал — словами, а следующее нажатие пробует снова.
    sharing.answer.set(Some("NotAllowedError"));
    if let Some(send) = &send {
        send.click();
    }
    sleep(100).await;
    if paper_alert(&host).as_deref() != Some(t(lang, Key::ShareFailed)) {
        wrong.push(format!("после отказа сказано: {:?}", paper_alert(&host)));
    }
    sharing.answer.set(None);
    if let Some(send) = &send {
        send.click();
    }
    sleep(100).await;
    if let Some(said) = paper_alert(&host) {
        wrong.push(format!("после новой отправки жалоба осталась: {said}"));
    }
    if sharing.calls() != 4 {
        wrong.push(format!("share позван {} раз на четыре нажатия", sharing.calls()));
    }

    // Браузер не умеет «Поделиться» — одна «Скачать PDF», главной кнопкой.
    match button(&host, &[t(lang, Key::PdfHide)]) {
        Some(hide) => hide.click(),
        None => wrong.push("нет «Скрыть»".to_owned()),
    }
    sleep(100).await;
    sharing.can.set(false);
    if !open_paper(&host, lang).await {
        wrong.push("файла PDF нет без «Поделиться»".to_owned());
    }
    if button(&host, &[t(lang, Key::PdfShare)]).is_some() {
        wrong.push("«Отправить PDF» там, где браузер не умеет".to_owned());
    }
    match host.query_selector(".paper a[download]").ok().flatten() {
        Some(link) if link.class_name() != "button" => {
            wrong.push(format!("одна «Скачать PDF» — {}", link.class_name()));
        }
        _ => {}
    }
    drop(mounted);
    drop(sharing);
    drop(fonts);
    host.remove();
    forget_device();
    assert!(wrong.is_empty(), "«Отправить PDF»: {wrong:#?}");
}

/// Курс и вид деятельности у счёта, закрытого действующим актом, ни на что
/// не влияют: доход книги — по акту (`ledger::income_documents`). Карточка
/// такого счёта не предлагает курс, а говорит, что он у акта; у самого
/// акта — «Указать курс ЦБ» (дефект 0.0.27–0.0.47, найден ревью задачи 19).
/// Один акт закрыл счёт целиком — обычный случай: карточка не говорит лишней
/// строки о сумме актов и второго акта не предлагает — выпуск его не пустил
/// бы (задача 84).
#[wasm_bindgen_test]
async fn an_invoice_with_an_act_leaves_the_rate_to_the_act() {
    let client = fresh_base_with_client().await;
    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let now = clock::now();
    let today = clock::today().expect("часы с датой");
    let invoice = Document::create(invoice_data(&client), now)
        .expect("черновик счёта")
        .issue(&[], 0, Some(&profile()), Some(None), &client, now)
        .expect("счёт выпущен");
    let act = Document::create(invoice.act_data(today).expect("акт по счёту"), now)
        .expect("черновик акта")
        .issue(&[], 0, Some(&profile()), Some(None), &client, now)
        .expect("акт выпущен");
    let mut writes = Writes::default();
    writes.insert(&invoice);
    writes.insert(&act);
    commit(writes).await;

    let address = format!("#/documents/{}", invoice.id());
    let (host, mounted) = mount_at(&address);
    let mut wrong = Vec::new();
    if !wait_for(&host, t(lang, Key::DocIncomeByAct)).await {
        wrong.push("у счёта с актом нет слов, что курс — у акта".to_owned());
    }
    if button(&host, &[t(lang, Key::DocRateSet)]).is_some() {
        wrong.push("у счёта с актом — «Указать курс ЦБ»".to_owned());
    }
    let acted = t(lang, Key::DocActsSoFar).split("{}").next().unwrap_or_default();
    if host.text_content().unwrap_or_default().contains(acted) {
        wrong.push("у счёта с одним актом на всю сумму — строка о сумме актов".to_owned());
    }
    // Кнопка прячется атрибутом, а не снимается (`DocCard`).
    if button(&host, &[t(lang, Key::DocActFromInvoice)]).is_some_and(|button| !button.hidden()) {
        wrong.push("у счёта, закрытого актом целиком, — «Акт по этому счёту»".to_owned());
    }
    if host.text_content().unwrap_or_default().contains(t(lang, Key::DocRateMissing)) {
        wrong.push("у счёта с актом — «в книге этот доход без суммы в драмах»".to_owned());
    }
    crate::platform::set_hash(&format!("#/documents/{}", act.id()));
    if !wait_for(&host, t(lang, Key::DocRateSet)).await {
        wrong.push("у акта нет «Указать курс ЦБ»".to_owned());
    }
    drop(mounted);
    host.remove();
    crate::platform::set_hash("");
    forget_device();
    assert!(wrong.is_empty(), "курс у счёта с актом: {wrong:#?}");
}

/// Второй акт-этап (полный прогон 2026-11-10, п. 9): счёт на 2 400,00 $,
/// первый акт — на 1 200,00 $. «Акт по этому счёту» открывает форму со
/// строками счёта, а под итогом — «Осталось актировать по счёту: 1 200,00 $».
/// Форма на всю сумму счёта говорит под итогом, что акт больше остатка, а
/// «Выпустить» не повторяет тех же слов ещё раз внизу формы.
#[wasm_bindgen_test]
async fn the_next_act_stage_shows_what_is_left_once() {
    let client = fresh_base_with_client().await;
    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let now = clock::now();
    let today = clock::today().expect("часы с датой");
    let invoice = Document::create(invoice_data(&client), now)
        .expect("черновик счёта")
        .issue(&[], 0, Some(&profile()), Some(None), &client, now)
        .expect("счёт выпущен");
    let mut half = invoice.act_data(today).expect("акт по счёту");
    half.items[0].price = 120_000;
    let act = Document::create(half, now)
        .expect("черновик акта")
        .issue(std::slice::from_ref(&invoice), 0, Some(&profile()), Some(None), &client, now)
        .expect("первый акт выпущен");
    let mut writes = Writes::default();
    writes.insert(&invoice);
    writes.insert(&act);
    commit(writes).await;

    let address = format!("#/documents/{}", invoice.id());
    let (host, mounted) = mount_at(&address);
    let mut wrong = Vec::new();
    if !wait_for(&host, t(lang, Key::DocActFromInvoice)).await {
        wrong.push("нет «Акт по этому счёту»".to_owned());
    }
    match button(&host, &[t(lang, Key::DocActFromInvoice)]) {
        Some(open) => open.click(),
        None => wrong.push("нет кнопки «Акт по этому счёту»".to_owned()),
    }
    let left = fill(t(lang, Key::DocActLeft), &[&i18n::money(lang, Money::new(120_000, Currency::Usd))]);
    if !wait_for(&host, &left).await {
        wrong.push(format!("в форме акта нет «{left}»"));
    }
    match field(&host, t(lang, Key::ItemText), 0) {
        Some(input) if input.value() == "Software development" => {}
        other => wrong.push(format!("строки акта — не как в счёте: {:?}", other.map(|input| input.value()))),
    }
    let refusal = t(lang, Key::DocActsOverInvoice);
    let said = |host: &HtmlElement| host.text_content().unwrap_or_default().matches(refusal).count();
    if said(&host) != 1 {
        wrong.push(format!("до выпуска слова отказа — {} раз", said(&host)));
    }
    match button(&host, &[t(lang, Key::DocIssue)]) {
        Some(issue) => issue.click(),
        None => wrong.push("нет «Выпустить»".to_owned()),
    }
    sleep(500).await;
    if said(&host) != 1 {
        wrong.push(format!("после «Выпустить» слова отказа — {} раз", said(&host)));
    }
    drop(mounted);
    host.remove();
    crate::platform::set_hash("");
    forget_device();
    assert!(wrong.is_empty(), "второй акт-этап: {wrong:#?}");
}

/// Два акта-этапа закрыли счёт целиком (полный прогон 2026-11-10, п. 10):
/// карточка счёта говорит «Акты по этому счёту закрывают его целиком», а не
/// «на 2 400,00 $, а сам счёт — на 2 400,00 $» — «а» звучало расхождением.
#[wasm_bindgen_test]
async fn acts_covering_the_invoice_say_so_plainly() {
    let client = fresh_base_with_client().await;
    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let now = clock::now();
    let today = clock::today().expect("часы с датой");
    let invoice = Document::create(invoice_data(&client), now)
        .expect("черновик счёта")
        .issue(&[], 0, Some(&profile()), Some(None), &client, now)
        .expect("счёт выпущен");
    let mut half = invoice.act_data(today).expect("акт по счёту");
    half.items[0].price = 120_000;
    let first = Document::create(half.clone(), now)
        .expect("черновик акта")
        .issue(std::slice::from_ref(&invoice), 0, Some(&profile()), Some(None), &client, now)
        .expect("первый акт выпущен");
    let second = Document::create(half, now)
        .expect("черновик второго акта")
        .issue(&[invoice.clone(), first.clone()], 0, Some(&profile()), Some(None), &client, now)
        .expect("второй акт выпущен");
    let mut writes = Writes::default();
    writes.insert(&invoice);
    writes.insert(&first);
    writes.insert(&second);
    commit(writes).await;

    let address = format!("#/documents/{}", invoice.id());
    let (host, mounted) = mount_at(&address);
    let mut wrong = Vec::new();
    if !wait_for(&host, t(lang, Key::DocActsWhole)).await {
        wrong.push("у счёта, закрытого двумя актами, нет «закрывают его целиком»".to_owned());
    }
    let so_far = t(lang, Key::DocActsSoFar).split("{}").next().unwrap_or_default();
    if host.text_content().unwrap_or_default().contains(so_far) {
        wrong.push("у счёта, закрытого актами целиком, — «на …, а сам счёт — на …»".to_owned());
    }
    drop(mounted);
    host.remove();
    crate::platform::set_hash("");
    forget_device();
    assert!(wrong.is_empty(), "акты на весь счёт: {wrong:#?}");
}

/// Строка налога выпущенного документа — в его карточке, свёрнутым вопросом:
/// сам выбор сохраняет её, номер и строки документа не меняются, а сохранённая
/// строка раскрывает вопрос. Акт по этому счёту берёт её в свою форму и в свой
/// черновик. Вернуть «по коду» можно тем же выбором. Выбор у выпущенного акта
/// идёт и в счёт, а карточка счёта с актом говорит, где её указать.
#[wasm_bindgen_test]
async fn the_income_row_is_set_in_the_card_and_kept_by_the_act() {
    let client = fresh_base_with_client().await;
    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let now = clock::now();
    let invoice = Document::create(invoice_data(&client), now)
        .expect("черновик")
        .issue(&[], 0, Some(&profile()), Some(None), &client, now)
        .expect("выпущен");
    let mut writes = Writes::default();
    writes.insert(&invoice);
    commit(writes).await;

    let address = format!("#/documents/{}", invoice.id());
    let (host, mounted) = mount_at(&address);
    let mut wrong = Vec::new();
    if !wait_for(&host, t(lang, Key::IncomeKindAsk)).await {
        wrong.push("в карточке нет вопроса о строке налога".to_owned());
    }
    let open = || {
        host.query_selector(".docs-detail details.income-kind")
            .ok()
            .flatten()
            .map(|details| details.has_attribute("open"))
    };
    let chosen = |value: &str, wrong: &mut Vec<String>| match select(&host, t(lang, Key::IncomeKindLabel), 0) {
        Some(list) => super::pick(&list, value),
        None => wrong.push("нет списка строк налога".to_owned()),
    };
    let saved_kind = || async {
        let store = Store::open(|_| {}).await.expect("база открыта");
        let found = store
            .read()
            .expect("транзакция чтения")
            .get::<Document>(invoice.id())
            .await
            .expect("счёт читается")
            .expect("счёт есть");
        (found.data().income_kind, found.number(), found.data().items.clone())
    };
    if open() != Some(false) {
        wrong.push(format!("вопрос не свёрнут: {:?}", open()));
    }
    chosen("9", &mut wrong);
    if !wait_for(&host, t(lang, Key::DocIncomeKindSaved)).await {
        wrong.push("строка налога не сохранилась".to_owned());
    }
    if saved_kind().await != (Some(IncomeKind::Assets), invoice.number(), invoice.data().items.clone()) {
        wrong.push(format!("в базе: {:?}", saved_kind().await.0));
    }
    sleep(50).await;
    if open() != Some(true) {
        wrong.push(format!("после сохранения вопрос свёрнут: {:?}", open()));
    }

    match button(&host, &[t(lang, Key::DocActFromInvoice)]) {
        Some(act) => act.click(),
        None => wrong.push("нет «Акт по этому счёту»".to_owned()),
    }
    sleep(150).await;
    if select(&host, t(lang, Key::IncomeKindLabel), 0).map(|list| list.value()).as_deref() != Some("9") {
        wrong.push("форма акта не взяла строку счёта".to_owned());
    }
    match button(&host, &[t(lang, Key::DocSaveDraft)]) {
        Some(save) => save.click(),
        None => wrong.push("у акта нет «Сохранить черновик»".to_owned()),
    }
    if !wait_for(&host, t(lang, Key::DocSaved)).await {
        wrong.push("черновик акта не сохранился".to_owned());
    }
    let (documents, _) = stored().await;
    let act = documents.iter().find(|document| document.data().kind == DocKind::Act);
    if act.map(|act| act.data().income_kind) != Some(Some(IncomeKind::Assets)) {
        wrong.push(format!("черновик акта: {:?}", act.map(|act| act.data().income_kind)));
    }

    crate::platform::set_hash(&address);
    sleep(150).await;
    chosen("", &mut wrong);
    sleep(200).await;
    if saved_kind().await.0.is_some() {
        wrong.push("«по коду» не вернулось".to_owned());
    }

    // Выпущенный акт: строка, указанная у него, идёт и в счёт.
    let (documents, _) = stored().await;
    let draft = documents.iter().find(|document| document.data().kind == DocKind::Act).expect("черновик акта");
    let later = clock::now();
    let issued = draft.issue(&documents, 0, Some(&profile()), Some(None), &client, later).expect("акт выпущен");
    let mut writes = Writes::default();
    writes.update(&issued);
    commit(writes).await;
    drop(mounted);
    host.remove();

    let address = format!("#/documents/{}", issued.id());
    let (host, mounted) = mount_at(&address);
    if !wait_for(&host, t(lang, Key::IncomeKindAsk)).await {
        wrong.push("в карточке акта нет вопроса о строке налога".to_owned());
    }
    match select(&host, t(lang, Key::IncomeKindLabel), 0) {
        Some(list) => super::pick(&list, "5"),
        None => wrong.push("у акта нет списка строк налога".to_owned()),
    }
    if !wait_for(&host, t(lang, Key::DocIncomeKindSaved)).await {
        wrong.push("строка акта не сохранилась".to_owned());
    }
    let (documents, _) = stored().await;
    let kinds: Vec<Option<IncomeKind>> = documents.iter().map(|document| document.data().income_kind).collect();
    if kinds != [Some(IncomeKind::Rent), Some(IncomeKind::Rent)] {
        wrong.push(format!("строка акта не дошла до счёта: {kinds:?}"));
    }
    crate::platform::set_hash(&format!("#/documents/{}", invoice.id()));
    if !wait_for(&host, t(lang, Key::DocIncomeByAct)).await {
        wrong.push("карточка счёта с актом не говорит, где строка налога".to_owned());
    }
    if host.query_selector(".docs-detail details.income-kind").ok().flatten().is_some() {
        wrong.push("у счёта с актом остался выбор строки".to_owned());
    }
    drop(mounted);
    host.remove();
    crate::platform::set_hash("");
    forget_device();
    assert!(wrong.is_empty(), "строка налога в карточке: {wrong:#?}");
}

/// Код у выпущенного документа можно снять: со своей строкой он не нужен,
/// а оставленный по ошибке закрыл бы налог с оборота (`turnover::regime_code`).
#[wasm_bindgen_test]
async fn the_code_is_cleared_beside_an_income_row() {
    let client = fresh_base_with_client().await;
    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let now = clock::now();
    let code = ActivityCode::parse("69.20.0").expect("код");
    let mut seller = profile();
    seller.activities = vec![code, ActivityCode::parse("73.11.0").expect("код")];
    let saved_profile = Profile::save(None, seller.clone(), now).expect("годный профиль").expect("новый");
    let mut data = invoice_data(&client);
    data.activity = Some(code);
    data.income_kind = Some(IncomeKind::Assets);
    let invoice = Document::create(data, now)
        .expect("черновик")
        .issue(&[], 0, Some(&seller), Some(None), &client, now)
        .expect("выпущен");
    let mut writes = Writes::default();
    writes.update(&saved_profile);
    writes.insert(&invoice);
    commit(writes).await;

    let address = format!("#/documents/{}", invoice.id());
    let (host, mounted) = mount_at(&address);
    let mut wrong = Vec::new();
    if !wait_for(&host, t(lang, Key::OpActivity)).await {
        wrong.push("в карточке нет вида деятельности".to_owned());
    }
    match select(&host, t(lang, Key::OpActivity), 0) {
        Some(list) => super::pick(&list, ""),
        None => wrong.push("нет списка кодов".to_owned()),
    }
    sleep(50).await;
    match button(&host, &[t(lang, Key::DocActivitySave)]) {
        Some(save) => save.click(),
        None => wrong.push("нет «Сохранить вид деятельности»".to_owned()),
    }
    if !wait_for(&host, t(lang, Key::DocActivitySaved)).await {
        wrong.push("код не снялся".to_owned());
    }
    let (documents, _) = stored().await;
    if documents.first().map(|document| document.data().activity) != Some(None) {
        wrong.push("в базе остался код".to_owned());
    }
    drop(mounted);
    host.remove();
    crate::platform::set_hash("");
    forget_device();
    assert!(wrong.is_empty(), "снять код: {wrong:#?}");
}

/// Ссылка с подписью `text` внутри `scope` (CSS).
fn link(host: &HtmlElement, scope: &str, text: &str) -> Option<HtmlElement> {
    let links = host.query_selector_all(&format!("{scope} a")).ok()?;
    (0..links.length())
        .filter_map(|at| links.item(at))
        .map(JsCast::unchecked_into::<HtmlElement>)
        .find(|link| link.text_content().unwrap_or_default().trim() == text)
}

/// Отметка о расчётном документе у документа `id` в базе программы;
/// `None` — документа нет.
async fn stored_settlement(id: RecordId) -> Option<Option<Settlement>> {
    let (documents, _) = stored().await;
    documents.iter().find(|d| d.id() == id).map(|d| d.settlement().cloned())
}

/// Файл для системы ՊԵԿ (задача 44): у выпущенного акта об услугах армянской
/// фирме ссылка «Файл для системы ՊԵԿ (XML)» в самом нажатии получает адрес
/// `blob:` с черновиком — вид 3, день услуги, ՀՎՀՀ сторон, итог, без вида
/// сделки, — под именем файла акта; у акта о товарах (код профиля —
/// торговля) ссылки нет, а слова говорят выписать его в системе самим.
#[wasm_bindgen_test]
async fn settlement_file_is_given_for_services() {
    crate::store::delete_database(crate::store::DB_NAME).await;
    let now = clock::now();
    let today = clock::today().expect("часы с датой");
    let (it, shop) = (ActivityCode::parse("62.01.0").expect("код"), ActivityCode::parse("47.91.0").expect("код"));
    let mut data = profile();
    data.tin = Some(Tin::parse("12345678").expect("ՀՎՀՀ"));
    data.activities = vec![it, shop];
    data.kinds.insert(shop, IncomeKind::Trade);
    let saved = Profile::save(None, data.clone(), now).expect("годный профиль").expect("новый");
    let firm = ClientData {
        name: line("«Արարատ» ՍՊԸ"),
        country: Some(Country::ARMENIA),
        address: Lines::default(),
        tin: Some(Tin::parse("07654321").expect("ՀՎՀՀ")),
        tax_id: Line::default(),
        currency: Some(Currency::Amd),
        lang: Some(Lang::Am),
        payment_days: None,
    };
    let client = Client::create(firm, now).expect("клиент");
    let act = |activity: ActivityCode, before: &[Document]| {
        let data_of = DocumentData {
            kind: DocKind::Act,
            client: client.id(),
            lang: Lang::Am,
            currency: Currency::Amd,
            date: today,
            service_date: today,
            due: None,
            items: vec![Item { text: line("Ծրագրի մշակում"), quantity_e3: 1_000, price: 500_000 }],
            basis: None,
            recurring: None,
            rate: None,
            activity: Some(activity),
            income_kind: None,
        };
        Document::create(data_of, now)
            .expect("черновик")
            .issue(before, 0, Some(&data), Some(None), &client, now)
            .expect("выпущен")
    };
    let services = act(it, &[]);
    let goods = act(shop, std::slice::from_ref(&services));
    let mut writes = Writes::default();
    writes.insert(&saved);
    writes.insert(&client);
    writes.insert(&services);
    writes.insert(&goods);
    commit(writes).await;
    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let address = Screen::Document(services.id()).href();
    let (host, mounted) = mount_at(&address);
    let mut wrong = Vec::new();
    if !wait_for(&host, t(lang, Key::SettleFile)).await {
        wrong.push("у акта об услугах нет ссылки на файл".to_owned());
    }
    match host.query_selector(".docs-detail .settlement a[download]").ok().flatten() {
        Some(link) => {
            let link: HtmlElement = link.unchecked_into();
            if link.get_attribute("download") != accdoc::file_name(&services) {
                wrong.push(format!("имя файла: {:?}", link.get_attribute("download")));
            }
            // Нажатие — без скачивания: свой слушатель на ссылке отменяет
            // переход, а обработчик окна выше по дереву всё равно ставит адрес.
            let stop = Closure::<dyn Fn(web_sys::Event)>::new(|event: web_sys::Event| event.prevent_default());
            link.add_event_listener_with_callback("click", stop.as_ref().unchecked_ref()).expect("слушатель");
            link.click();
            let _ = link.remove_event_listener_with_callback("click", stop.as_ref().unchecked_ref());
            let href = link.get_attribute("href").unwrap_or_default();
            if href.starts_with("blob:") {
                let text = blob_text(&href).await.unwrap_or_default();
                for part in [
                    "<Type>3</Type>".to_owned(),
                    format!("<DeliveryDate>{}</DeliveryDate>", today.iso()),
                    "<TIN>12345678</TIN>".to_owned(),
                    "<TIN>07654321</TIN>".to_owned(),
                    "<Total><TotalPrice>500000</TotalPrice></Total>".to_owned(),
                ] {
                    if !text.contains(&part) {
                        wrong.push(format!("в файле нет {part}"));
                    }
                }
                // Вид сделки выбирает человек в черновике (решение человека).
                if text.contains("DealType") {
                    wrong.push("в файле есть вид сделки".to_owned());
                }
            } else {
                wrong.push(format!("после нажатия адрес не файла: {href}"));
            }
        }
        None => wrong.push("нет ссылки a[download] в блоке расчётного документа".to_owned()),
    }

    crate::platform::set_hash(&Screen::Document(goods.id()).href());
    if !wait_for(&host, t(lang, Key::SettleFileGoods)).await {
        wrong.push("у акта о товарах нет слов, что файл — только для услуг".to_owned());
    }
    let link = host.query_selector(".docs-detail .settlement a[download]").ok().flatten();
    if link.is_some() {
        wrong.push("у акта о товарах есть ссылка на файл".to_owned());
    }

    drop(mounted);
    host.remove();
    crate::platform::set_hash("");
    forget_device();
    assert!(wrong.is_empty(), "файл для ՊԵԿ: {wrong:#?}");
}

/// Текст по адресу `blob:` — настоящим `fetch`.
async fn blob_text(url: &str) -> Option<String> {
    let window = web_sys::window()?;
    let response: Response = window.fetch_with_str(url).await.ok()?.dyn_into().ok()?;
    response.text().ok()?.await.ok()?.as_string()
}

/// Расчётный документ в системе ՊԵԿ (задача 23). Главный экран напоминает
/// о счёте, срок которого прошёл, а о счёте со сроком через неделю и о счёте,
/// выпущенном до 0.0.55, — нет: о последнем программа не знает, и его
/// карточка так и говорит; «Открыть список» открывает список только
/// документов без отметки — со сроком вместо статуса. В карточке — срок
/// и «Отметить, что выписан»: отметка с номером ложится в базу, карточка
/// называет её день и номер, в истории — событие, а счёт уходит из списка
/// без отметки. «Снять отметку» убирает её из базы, а «Показать все»
/// возвращает весь список.
#[wasm_bindgen_test]
async fn settlement_documents_are_marked_and_listed() {
    let client = fresh_base_with_client().await;
    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let now = clock::now();
    let today = clock::today().expect("часы с датой");
    let yesterday = today.add_days(-1).expect("дата есть");
    let next_week = today.add_days(7).expect("дата есть");
    let issue = |service: Date, before: &[Document]| {
        let data = DocumentData {
            service_date: service,
            ..invoice_data(&client)
        };
        Document::create(data, now)
            .expect("черновик")
            .issue(before, 0, Some(&profile()), Some(None), &client, now)
            .expect("выпущен")
    };
    let late = issue(yesterday, &[]);
    let ahead = issue(next_week, std::slice::from_ref(&late));
    // Выпущенный до 0.0.55: в его записи ключа отметки нет.
    let before = issue(yesterday, &[late.clone(), ahead.clone()]);
    let text = kvartik_core::format::encode(&before);
    let before: Document =
        kvartik_core::format::decode(&text.replacen(r#""settlement":null,"#, "", 1)).expect("читается");
    assert!(!before.settlement_known(), "документ прежней версии не получился");
    let mut writes = Writes::default();
    writes.insert(&late);
    writes.insert(&ahead);
    writes.insert(&before);
    commit(writes).await;

    let (host, mounted) = mount_at("#/");
    let mut wrong = Vec::new();
    let one = fill(t(lang, Key::DocumentsOne), &["1"]);
    let remind = fill(t(lang, Key::SettleRemind), &[&one]);
    if !wait_for(&host, &remind).await {
        wrong.push(format!("на главном нет «{remind}»"));
    }
    match link(&host, "#home .notice.due", t(lang, Key::SettleOpen)) {
        Some(show) => show.click(),
        None => wrong.push("у напоминания нет «Открыть список»".to_owned()),
    }
    if !wait_for(&host, t(lang, Key::SettleShown)).await {
        wrong.push("список без отметки не открылся".to_owned());
    }
    let rows = listed_rows(&host);
    let ahead_chip = fill(t(lang, Key::SettleBy), &[&i18n::date(lang, next_week)]);
    match rows.as_slice() {
        [first, second]
            if first.contains(&ahead_chip) && second.contains(t(lang, Key::SettleLate)) => {}
        other => wrong.push(format!("строки без отметки: {other:?}")),
    }

    crate::platform::set_hash(&format!("#/documents/{}", before.id()));
    if !wait_for(&host, t(lang, Key::SettleUnknown)).await {
        wrong.push("у документа прежней версии нет слов, что программа не знает".to_owned());
    }
    crate::platform::set_hash(&format!("#/documents/{}", late.id()));
    let due_was = fill(t(lang, Key::SettleDueWas), &[&i18n::date(lang, yesterday)]);
    if !wait_for(&host, &due_was).await {
        wrong.push(format!("в карточке нет «{due_was}»"));
    }
    let fieldset = || host.query_selector(".docs-detail .settlement").ok().flatten().map(JsCast::unchecked_into::<HtmlElement>);
    match fieldset().and_then(|box_| field(&box_, t(lang, Key::SettleNumber), 0)) {
        Some(input) => type_into(&input, "Ա 0123456789"),
        None => wrong.push("нет поля номера".to_owned()),
    }
    match fieldset().and_then(|box_| button(&box_, &[t(lang, Key::SettleMark)])) {
        Some(mark) => mark.click(),
        None => wrong.push("нет «Отметить, что выписан»".to_owned()),
    }
    if !wait_for(&host, t(lang, Key::SettleSaved)).await {
        wrong.push("отметка не подтвердилась".to_owned());
    }
    let expected = Settlement {
        day: today,
        number: line("Ա 0123456789"),
        kind: SettlementKind::System,
    };
    if stored_settlement(late.id()).await != Some(Some(expected)) {
        wrong.push(format!("в базе отметка не та: {:?}", stored_settlement(late.id()).await));
    }
    let text = host.query_selector(".docs-detail").ok().flatten().and_then(|d| d.text_content()).unwrap_or_default();
    let settled = fill(t(lang, Key::SettledOnNumbered), &[&i18n::date(lang, today), "Ա 0123456789"]);
    if !text.contains(&settled) || !text.contains(t(lang, Key::HistorySettled)) {
        wrong.push(format!("в карточке нет «{settled}» или события истории"));
    }

    crate::platform::set_hash("#/documents");
    sleep(200).await;
    let rows = listed_rows(&host);
    if rows.len() != 1 || !rows[0].contains(&ahead_chip) {
        wrong.push(format!("после отметки в списке без отметки: {rows:?}"));
    }
    match button(&host, &[t(lang, Key::SettleShowAll)]) {
        Some(all) => all.click(),
        None => wrong.push("нет «Показать все»".to_owned()),
    }
    sleep(100).await;
    if listed_rows(&host).len() != 3 {
        wrong.push(format!("«Показать все»: {:?}", listed_rows(&host)));
    }

    crate::platform::set_hash(&format!("#/documents/{}", late.id()));
    sleep(200).await;
    match fieldset().and_then(|box_| button(&box_, &[t(lang, Key::SignRemove)])) {
        Some(remove) => remove.click(),
        None => wrong.push("нет «Снять отметку»".to_owned()),
    }
    if !wait_for(&host, t(lang, Key::SettleRemoved)).await {
        wrong.push("отметка не снялась".to_owned());
    }
    if stored_settlement(late.id()).await != Some(None) {
        wrong.push(format!("после снятия в базе: {:?}", stored_settlement(late.id()).await));
    }
    drop(mounted);
    host.remove();
    crate::platform::set_hash("");
    forget_device();
    assert!(wrong.is_empty(), "расчётный документ: {wrong:#?}");
}

/// Расчётный документ, выписанный в квартале раньше услуги (задача 77):
/// под днём выписки, пока его вводят, и у отметки — слова, что доход мог
/// уйти в квартал подтверждения. День того же квартала и покупатель из
/// другой страны — без них: американская фирма документ в системе ՊԵԿ не
/// подтверждает. У армянской фирмы с ИНН (ՀՎՀՀ) рядом с кнопкой чека ККМ —
/// условие освобождения (задача 76), у американской без ИНН — нет.
#[wasm_bindgen_test]
async fn early_settlement_is_warned() {
    let foreign = fresh_base_with_client().await;
    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let now = clock::now();
    let today = clock::today().expect("часы с датой");
    // Услуга — в первый день следующего квартала: день выписки, который
    // форма предлагает, — сегодня, в квартале раньше.
    let service = today.quarter().last_day().add_days(1).expect("дата есть");
    let armenian = Client::create(
        ClientData {
            name: line("«Արարատ» ՍՊԸ"),
            country: Some(Country::ARMENIA),
            tin: Some(Tin::parse("07654321").expect("ՀՎՀՀ")),
            currency: Some(Currency::Amd),
            lang: Some(Lang::Am),
            ..acme()
        },
        now,
    )
    .expect("годный клиент");
    let issue = |client: &Client, before: &[Document]| {
        let data = DocumentData {
            client: client.id(),
            currency: client.data().currency.unwrap_or(Currency::Amd),
            service_date: service,
            ..invoice_data(client)
        };
        Document::create(data, now)
            .expect("черновик")
            .issue(before, 0, Some(&profile()), Some(None), client, now)
            .expect("выпущен")
    };
    let local = issue(&armenian, &[]);
    let abroad = issue(&foreign, std::slice::from_ref(&local));
    let mut writes = Writes::default();
    writes.insert(&armenian);
    writes.insert(&local);
    writes.insert(&abroad);
    commit(writes).await;

    let card = format!("#/documents/{}", local.id());
    let (host, mounted) = mount_at(&card);
    let mut wrong = Vec::new();
    let warned = fill(
        t(lang, Key::SettleEarly),
        &[&i18n::quarter(lang, today.quarter()), &i18n::quarter(lang, service.quarter())],
    );
    let said = || host.query_selector(".docs-detail").ok().flatten().and_then(|d| d.text_content()).unwrap_or_default();
    if !wait_for(&host, &warned).await {
        wrong.push(format!("под сегодняшним днём выписки нет «{warned}»"));
    }
    if !said().contains(t(lang, Key::SettleReceiptFirm)) {
        wrong.push("у фирмы с ИНН нет условия чека ККМ".to_owned());
    }
    let fieldset = || host.query_selector(".docs-detail .settlement").ok().flatten().map(JsCast::unchecked_into::<HtmlElement>);
    let day_field = || fieldset().and_then(|box_| field(&box_, t(lang, Key::SettleDate), 0));
    match day_field() {
        Some(input) => type_into(&input, &service.iso()),
        None => wrong.push("нет поля дня выписки".to_owned()),
    }
    if !wait_until(|| !said().contains(&warned)).await {
        wrong.push("день того же квартала, что услуга, — а слова остались".to_owned());
    }
    match day_field() {
        Some(input) => type_into(&input, &today.iso()),
        None => wrong.push("нет поля дня выписки".to_owned()),
    }
    match fieldset().and_then(|box_| button(&box_, &[t(lang, Key::SettleMark)])) {
        Some(mark) => mark.click(),
        None => wrong.push("нет «Отметить, что выписан»".to_owned()),
    }
    let settled = fill(t(lang, Key::SettledOn), &[&i18n::date(lang, today)]);
    if !wait_for(&host, &settled).await {
        wrong.push("отметка не легла".to_owned());
    } else if !said().contains(&warned) {
        wrong.push(format!("у отметки нет «{warned}»"));
    }

    // Американская фирма: тот же день выписки — без слов. Профиль уже
    // прочитан — слова выше это показали, — и молчание не от загрузки.
    crate::platform::set_hash(&format!("#/documents/{}", abroad.id()));
    if !wait_for(&host, t(lang, Key::SettleMark)).await {
        wrong.push("карточка документа американской фирмы не открылась".to_owned());
    }
    if said().contains(&warned) {
        wrong.push("у покупателя из другой страны — слова о подтверждении".to_owned());
    }
    if said().contains(t(lang, Key::SettleReceiptFirm)) {
        wrong.push("у фирмы без ИНН (ՀՎՀՀ) — условие для фирмы с ним".to_owned());
    }
    drop(mounted);
    host.remove();
    crate::platform::set_hash("");
    forget_device();
    assert!(wrong.is_empty(), "расчётный документ раньше квартала услуги: {wrong:#?}");
}

/// Чек ККМ вместо расчётного документа в системе ՊԵԿ (задача 76): у
/// документа без отметки — слова, когда так можно, и «Отметить, что выдан чек
/// ККМ»; нажатие кладёт в базу отметку вида «чек» с днём и номером из полей,
/// карточка и история говорят «Выдан чек ККМ», а «Снять отметку» её снимает.
#[wasm_bindgen_test]
async fn receipt_marks_the_settlement() {
    let client = fresh_base_with_client().await;
    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let now = clock::now();
    let today = clock::today().expect("часы с датой");
    let document = Document::create(invoice_data(&client), now)
        .expect("черновик")
        .issue(&[], 0, Some(&profile()), Some(None), &client, now)
        .expect("выпущен");
    let mut writes = Writes::default();
    writes.insert(&document);
    commit(writes).await;

    let card = format!("#/documents/{}", document.id());
    let (host, mounted) = mount_at(&card);
    let mut wrong = Vec::new();
    if !wait_for(&host, t(lang, Key::SettleReceiptAbout)).await {
        wrong.push("нет слов о чеке ККМ".to_owned());
    }
    let fieldset = || host.query_selector(".docs-detail .settlement").ok().flatten().map(JsCast::unchecked_into::<HtmlElement>);
    match fieldset().and_then(|box_| field(&box_, t(lang, Key::SettleNumber), 0)) {
        Some(input) => type_into(&input, "00012345"),
        None => wrong.push("нет поля номера".to_owned()),
    }
    match fieldset().and_then(|box_| button(&box_, &[t(lang, Key::SettleReceipt)])) {
        Some(receipt) => receipt.click(),
        None => wrong.push("нет «Отметить, что выдан чек ККМ»".to_owned()),
    }
    if !wait_for(&host, t(lang, Key::ReceiptSaved)).await {
        wrong.push("отметка чека не подтвердилась".to_owned());
    }
    let expected = Settlement {
        day: today,
        number: line("00012345"),
        kind: SettlementKind::Receipt,
    };
    if stored_settlement(document.id()).await != Some(Some(expected)) {
        wrong.push(format!("в базе отметка не та: {:?}", stored_settlement(document.id()).await));
    }
    let said = host.query_selector(".docs-detail").ok().flatten().and_then(|d| d.text_content()).unwrap_or_default();
    let receipt = fill(t(lang, Key::ReceiptOnNumbered), &[&i18n::date(lang, today), "00012345"]);
    if !said.contains(&receipt) || !said.contains(t(lang, Key::HistoryReceipt)) {
        wrong.push(format!("в карточке нет «{receipt}» или события истории"));
    }
    if said.contains(t(lang, Key::HistorySettled)) {
        wrong.push("в истории — «выписан в системе ՊԵԿ», а выдан чек".to_owned());
    }
    match fieldset().and_then(|box_| button(&box_, &[t(lang, Key::SignRemove)])) {
        Some(remove) => remove.click(),
        None => wrong.push("нет «Снять отметку»".to_owned()),
    }
    if !wait_for(&host, t(lang, Key::SettleRemoved)).await {
        wrong.push("отметка не снялась".to_owned());
    }
    if stored_settlement(document.id()).await != Some(None) {
        wrong.push(format!("после снятия в базе: {:?}", stored_settlement(document.id()).await));
    }
    drop(mounted);
    host.remove();
    crate::platform::set_hash("");
    forget_device();
    assert!(wrong.is_empty(), "чек ККМ: {wrong:#?}");
}
