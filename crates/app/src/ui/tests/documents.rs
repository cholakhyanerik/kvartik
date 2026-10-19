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
use js_sys::{Function, Promise, Reflect, Uint8Array};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_test::wasm_bindgen_test;
use web_sys::{HtmlElement, Response};

use super::share::ShareStub;
use super::{
    button, choose_file, field, mount, mount_at, pick_client, select, shown_step, sleep, type_into,
    visit, wait_for,
};
use crate::ui::route::Screen;
use crate::platform::clock;
use crate::store::device::Device;
use crate::store::{Store, Writes};

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
    sign_flow(host, lang, today, &mut wrong).await;
    close(&mut wrong);
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
    sleep(100).await;
    press(t(lang, Key::DocIssue), &mut wrong);
    let second = numbered(Key::KindInvoice, 2);
    if !wait_for(host, &fill(t(lang, Key::DocIssuedNotice), &[&second])).await {
        wrong.push(format!("второй счёт не выпущен: нет «{second}»"));
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
    if status() != t(lang, Key::StatusIssued) {
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
        rate: None,
        activity: None,
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
/// поступление; «назад» возвращает к списку.
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
    };
    let issue = |kind: DocKind, price: i64, before: &[Document]| {
        Document::create(data(kind, price), now)
            .expect("черновик")
            .issue(before, 0, Some(&profile()), &client, now)
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
        .issue(&[], 0, Some(&profile()), &client, now)
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
        .issue(&[], 0, Some(&profile()), &client, now)
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
#[wasm_bindgen_test]
async fn an_invoice_with_an_act_leaves_the_rate_to_the_act() {
    let client = fresh_base_with_client().await;
    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let now = clock::now();
    let today = clock::today().expect("часы с датой");
    let invoice = Document::create(invoice_data(&client), now)
        .expect("черновик счёта")
        .issue(&[], 0, Some(&profile()), &client, now)
        .expect("счёт выпущен");
    let act = Document::create(invoice.act_data(today).expect("акт по счёту"), now)
        .expect("черновик акта")
        .issue(&[], 0, Some(&profile()), &client, now)
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
