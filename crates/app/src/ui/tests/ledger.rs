//! Браузерные тесты книги доходов и расходов (задача 14): всё окно
//! в настоящем DOM и настоящей IndexedDB. Профиль, клиент и документы
//! кладутся в базу командами ядра — их формы проверяют свои тесты.

use js_sys::{Array, Uint8Array};
use kvartik_core::i18n::{self, Key, Lang, fill, t};
use kvartik_core::model::activity::{ActivityCode, IncomeKind};
use kvartik_core::model::attachment::{Attachment, FileKind};
use kvartik_core::model::date::Date;
use kvartik_core::model::document::{DocKind, Document};
use kvartik_core::model::ledger::{Cost, Flow, Nature, Operation, OperationData, Proof};
use kvartik_core::model::money::{Currency, Money};
use kvartik_core::model::profile::Profile;
use kvartik_core::model::record::RecordId;
use kvartik_core::model::text::Line;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::wasm_bindgen_test;
use web_sys::{
    Blob, BlobPropertyBag, File, FilePropertyBag, HtmlElement, HtmlInputElement, ImageBitmap,
    ImageEncodeOptions, OffscreenCanvas,
};

use super::documents::{
    commit, forget_device, fresh_base_with_client, invoice_data, profile, row_with, wait_selector,
};
use super::{button, field, mount_at, pick, select, sleep, type_into, wait_for};
use crate::platform::{clock, set_hash};
use crate::store::device::Device;
use crate::store::{Store, Writes};

fn amd(minor: i64) -> Money {
    Money::new(minor, Currency::Amd)
}

/// Записи книги и вложения из базы программы.
async fn stored() -> (Vec<Operation>, Vec<Attachment>) {
    let store = Store::open(|_| {}).await.expect("база открыта");
    let tx = store.read().expect("транзакция чтения");
    let operations = tx.list::<Operation>().await.expect("записи читаются").records;
    let files = tx.list::<Attachment>().await.expect("вложения читаются").records;
    (operations, files)
}

/// Выпущенный счёт клиенту на 2 400 долларов — с курсом 388,00 ֏ или без.
async fn issued_invoice(rate: Option<i64>) -> Document {
    let client = fresh_base_with_client().await;
    let now = clock::now();
    let mut data = invoice_data(&client);
    data.rate = rate.map(|rate| data.rate_for(rate).expect("курс годится"));
    let invoice = Document::create(data, now)
        .expect("черновик")
        .issue(&[], 0, Some(&profile()), &client, now)
        .expect("выпущен");
    let mut writes = Writes::default();
    writes.insert(&invoice);
    commit(writes).await;
    invoice
}

/// Заполнить поле с подписью `label`; нет поля — строка в `wrong`.
fn fill_field(host: &HtmlElement, label: &str, text: &str, wrong: &mut Vec<String>) {
    match field(host, label, 0) {
        Some(input) => type_into(&input, text),
        None => wrong.push(format!("нет поля «{label}»")),
    }
}

/// Выбрать `value` в списке с подписью `label`.
fn choose(host: &HtmlElement, label: &str, value: &str, wrong: &mut Vec<String>) {
    match select(host, label, 0) {
        Some(list) => pick(&list, value),
        None => wrong.push(format!("нет списка «{label}»")),
    }
}

/// Нажать кнопку с подписью `label`.
fn press(host: &HtmlElement, label: &str, wrong: &mut Vec<String>) {
    match button(host, &[label]) {
        Some(button) => button.click(),
        None => wrong.push(format!("нет кнопки «{label}»")),
    }
}

/// Строка записи книги, в тексте которой есть `text`, — нажать, чтобы
/// открыть форму.
fn open_entry(host: &HtmlElement, text: &str, wrong: &mut Vec<String>) {
    let entry = row_with(host, "ul.ledger", text)
        .and_then(|row| row.query_selector("button.entry").ok().flatten());
    match entry {
        Some(entry) => entry.unchecked_into::<HtmlElement>().click(),
        None => wrong.push(format!("нет строки «{text}»")),
    }
}

/// Книга: доход из выпущенного счёта — в квартал по дню оказания услуги,
/// в драмах по его курсу; расход вносится формой с документом продавца;
/// доход в валюте без курса в итог не идёт, и окно говорит это словами,
/// а с курсом — идёт; запись удаляется и восстанавливается; соседний
/// квартал — без записей.
#[wasm_bindgen_test]
async fn ledger_counts_documents_and_entries() {
    let invoice = issued_invoice(Some(3_880_000)).await;
    let today = clock::today().expect("часы с датой");
    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let (host, mounted) = mount_at("#/ledger");
    let mut wrong = Vec::new();
    if !wait_for(&host, t(lang, Key::LedgerAddExpense)).await {
        wrong.push("экран книги не открылся".to_owned());
    }
    // 2 400,00 $ × 388,00 = 931 200 ֏ — как в дизайне «Kvartik Design».
    let income = i18n::money(lang, amd(931_200));
    if !wait_for(&host, &income).await {
        wrong.push(format!("нет дохода счёта {income}"));
    }
    if row_with(&host, "ul.ledger", "Acme Inc.").is_none() {
        wrong.push("нет строки счёта".to_owned());
    }

    press(&host, t(lang, Key::LedgerAddExpense), &mut wrong);
    sleep(50).await;
    // Пустая форма не сохраняется и говорит, чего не хватает: кодов в профиле
    // нет, и прямой ли расход, тоже спрошено (задача 79).
    press(&host, t(lang, Key::ProfileSave), &mut wrong);
    sleep(50).await;
    for missing in [Key::OpTextMissing, Key::OpProofMissing, Key::OpCostMissing] {
        if !host.text_content().unwrap_or_default().contains(t(lang, missing)) {
            wrong.push(format!("нет жалобы {missing:?}"));
        }
    }
    fill_field(&host, t(lang, Key::OpText), "Хостинг сайта", &mut wrong);
    fill_field(&host, t(lang, Key::OpSupplier), "«Վեբ» ՍՊԸ", &mut wrong);
    fill_field(&host, t(lang, Key::OpAmount), &i18n::amount(lang, amd(60_000)), &mut wrong);
    choose(&host, t(lang, Key::OpProof), "settlement_invoice", &mut wrong);
    // Без ответа, прямой ли расход, запись не ложится, даже когда остальное
    // заполнено.
    press(&host, t(lang, Key::ProfileSave), &mut wrong);
    sleep(100).await;
    if !stored().await.0.is_empty() {
        wrong.push("расход без ответа «прямой или сбытовой» записан".to_owned());
    }
    choose(&host, t(lang, Key::OpCost), "overhead", &mut wrong);
    press(&host, t(lang, Key::ProfileSave), &mut wrong);
    if !wait_for(&host, t(lang, Key::LedgerSaved)).await {
        wrong.push("расход не сохранился".to_owned());
    }
    let (operations, _) = stored().await;
    match operations.as_slice() {
        [spent] => {
            let data = spent.data();
            let expected = (
                Flow::Expense,
                today,
                amd(60_000),
                Some(Proof::SettlementInvoice),
                Some(Nature::Ordinary),
                Some(Cost::Overhead),
            );
            if (data.flow, data.date, data.amount, data.proof, data.nature, data.cost) != expected {
                wrong.push(format!("расход записан не так: {data:?}"));
            }
        }
        other => wrong.push(format!("записей {}, а не одна", other.len())),
    }
    if !host.text_content().unwrap_or_default().contains(&i18n::money(lang, amd(60_000))) {
        wrong.push("нет итога расходов".to_owned());
    }

    // Доход в евро без курса — в итог не идёт, и окно это говорит.
    press(&host, t(lang, Key::LedgerAddIncome), &mut wrong);
    sleep(50).await;
    fill_field(&host, t(lang, Key::OpText), "Консультация", &mut wrong);
    choose(&host, t(lang, Key::DocCurrency), "EUR", &mut wrong);
    sleep(50).await;
    fill_field(
        &host,
        t(lang, Key::OpAmount),
        &i18n::amount(lang, Money::new(10_000, Currency::Eur)),
        &mut wrong,
    );
    press(&host, t(lang, Key::ProfileSave), &mut wrong);
    let unrated = fill(t(lang, Key::LedgerUnrated), &["1"]);
    if !wait_for(&host, &unrated).await {
        wrong.push("нет слов о строке без курса".to_owned());
    }
    if !host.text_content().unwrap_or_default().contains(t(lang, Key::LedgerNoRate)) {
        wrong.push("у строки без курса нет пометки".to_owned());
    }

    // С курсом 405,58 ֏: 100,00 € — 40 558 ֏, доход — 971 758 ֏.
    open_entry(&host, "Консультация", &mut wrong);
    sleep(50).await;
    let rate_label = fill(t(lang, Key::OpRate), &["EUR"]);
    fill_field(&host, &rate_label, &i18n::rate(lang, 4_055_800), &mut wrong);
    sleep(50).await;
    if !host.text_content().unwrap_or_default().contains(&i18n::money(lang, amd(40_558))) {
        wrong.push("форма не показала сумму в драмах".to_owned());
    }
    press(&host, t(lang, Key::ProfileSave), &mut wrong);
    if !wait_for(&host, &i18n::money(lang, amd(971_758))).await {
        wrong.push("доход с курсом не вошёл в итог".to_owned());
    }
    if host.text_content().unwrap_or_default().contains(&unrated) {
        wrong.push("слова о строке без курса остались".to_owned());
    }

    // Удалить и восстановить.
    open_entry(&host, "Консультация", &mut wrong);
    sleep(50).await;
    press(&host, t(lang, Key::OpDelete), &mut wrong);
    if !wait_for(&host, t(lang, Key::LedgerDeletedNotice)).await {
        wrong.push("нет сообщения об удалении".to_owned());
    }
    if !host.text_content().unwrap_or_default().contains(&i18n::money(lang, amd(931_200))) {
        wrong.push("удалённая запись осталась в итоге".to_owned());
    }
    press(&host, t(lang, Key::ClientRestore), &mut wrong);
    if !wait_for(&host, t(lang, Key::LedgerRestoredNotice)).await {
        wrong.push("запись не восстановилась".to_owned());
    }
    let (operations, _) = stored().await;
    if operations.iter().any(Operation::is_deleted) {
        wrong.push("в базе осталась метка удаления".to_owned());
    }

    // Соседний квартал — пустой, и обратно.
    match host.query_selector(".quarter-bar button:last-child").ok().flatten() {
        Some(next) => next.unchecked_into::<HtmlElement>().click(),
        None => wrong.push("нет кнопки следующего квартала".to_owned()),
    }
    if !wait_for(&host, t(lang, Key::LedgerEmpty)).await {
        wrong.push("следующий квартал не пустой".to_owned());
    }
    let shown = i18n::quarter(lang, today.quarter().next().expect("квартал есть"));
    if !host.text_content().unwrap_or_default().contains(&shown) {
        wrong.push(format!("квартал не назван: {shown}"));
    }
    let _ = invoice;

    drop(mounted);
    host.remove();
    set_hash("");
    forget_device();
    assert!(wrong.is_empty(), "книга: {wrong:#?}");
}

/// Длинный квартал — страницами (Правило 1, «рисовать только видимое»):
/// из 31 записи видно 30 и «Показать ещё», после него — все.
#[wasm_bindgen_test]
async fn long_quarter_shows_by_pages() {
    crate::store::delete_database(crate::store::DB_NAME).await;
    let today = clock::today().expect("часы с датой");
    let now = clock::now();
    let mut writes = Writes::default();
    for n in 1..=31 {
        let data = kvartik_core::model::ledger::OperationData {
            flow: Flow::Income,
            date: today,
            paid: None,
            amount: amd(1_000 * n),
            rate: None,
            text: super::documents::line(&format!("Урок {n}")),
            client: None,
            supplier: kvartik_core::model::text::Line::default(),
            proof: None,
            nature: None,
            cost: None,
            activity: None,
            income_kind: None,
            attachments: Vec::new(),
        };
        writes.insert(&Operation::create(data, now).expect("годна"));
    }
    commit(writes).await;
    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let (host, mounted) = mount_at("#/ledger");
    let mut wrong = Vec::new();
    if !wait_for(&host, t(lang, Key::LedgerAddExpense)).await {
        wrong.push("экран книги не открылся".to_owned());
    }
    let rows = || host.query_selector_all("ul.ledger li").map_or(0, |rows| rows.length());
    // Строки — когда записи прочитаны: подождать первую.
    if !wait_selector(&host, "ul.ledger li").await {
        wrong.push("строк нет".to_owned());
    }
    sleep(100).await;
    if rows() != 30 {
        wrong.push(format!("сразу видно {} строк, а не 30", rows()));
    }
    press(&host, t(lang, Key::DocsMore), &mut wrong);
    sleep(200).await;
    if rows() != 31 {
        wrong.push(format!("после «Показать ещё» — {} строк", rows()));
    }
    if button(&host, &[t(lang, Key::DocsMore)]).is_some_and(|more| more.closest("#ledger").ok().flatten().is_some()) {
        wrong.push("«Показать ещё» осталась".to_owned());
    }

    drop(mounted);
    host.remove();
    set_hash("");
    forget_device();
    assert!(wrong.is_empty(), "страницы книги: {wrong:#?}");
}

/// Файл с байтами `bytes`, именем `name` и типом `mime`.
fn file(bytes: &[u8], name: &str, mime: &str) -> File {
    let options = FilePropertyBag::new();
    options.set_type(mime);
    File::new_with_buffer_source_sequence_and_options(&Array::of1(&Uint8Array::from(bytes)), name, &options)
        .expect("файл собран")
}

/// Положить файлы в поле выбора, как будто их выбрал человек.
fn choose_files(input: &HtmlInputElement, files: &[File]) {
    let transfer = web_sys::DataTransfer::new().expect("DataTransfer есть");
    for file in files {
        transfer.items().add_with_file(file).expect("файл положен");
    }
    input.set_files(transfer.files().as_ref());
    let event = web_sys::Event::new("change").expect("событие создаётся");
    input.dispatch_event(&event).expect("событие отправлено");
}

/// PNG `width`×`height` — пустой холст браузера.
async fn png(width: u32, height: u32) -> Vec<u8> {
    let canvas = OffscreenCanvas::new(width, height).expect("холст есть");
    let _ = canvas.get_context("2d").expect("2d есть");
    let options = ImageEncodeOptions::new();
    options.set_type("image/png");
    let blob: Blob = canvas
        .convert_to_blob_with_options(&options)
        .expect("PNG собирается")
        .await
        .expect("PNG готов")
        .unchecked_into();
    let buffer = blob.array_buffer().await.expect("байты есть");
    Uint8Array::new(&buffer).to_vec()
}

/// Ширина и высота картинки по её байтам.
async fn size_of(bytes: &[u8], mime: &str) -> (u32, u32) {
    let options = BlobPropertyBag::new();
    options.set_type(mime);
    let blob = Blob::new_with_buffer_source_sequence_and_options(&Array::of1(&Uint8Array::from(bytes)), &options)
        .expect("Blob собран");
    let window = web_sys::window().expect("окно есть");
    let bitmap: ImageBitmap = window
        .create_image_bitmap_with_blob(&blob)
        .expect("разбор начат")
        .await
        .expect("картинка разобрана")
        .unchecked_into();
    (bitmap.width(), bitmap.height())
}

/// Файлы записи: фото больше 1600 точек ложится JPEG в 1600 точек
/// и с расширением `.jpg`, PDF — как есть; оба — записями-вложениями этой
/// записи, а в форме — ссылками с миниатюрой. Убранный файл помечается
/// убранным, а не стирается.
#[wasm_bindgen_test]
async fn files_are_attached_shrunk_and_removed() {
    crate::store::delete_database(crate::store::DB_NAME).await;
    let lang = Lang::En;
    Device::new().set_ui_lang(lang);
    let (host, mounted) = mount_at("#/ledger");
    let mut wrong = Vec::new();
    if !wait_for(&host, t(lang, Key::LedgerAddExpense)).await {
        wrong.push("экран книги не открылся".to_owned());
    }
    press(&host, t(lang, Key::LedgerAddExpense), &mut wrong);
    sleep(50).await;
    fill_field(&host, t(lang, Key::OpText), "Office chair", &mut wrong);
    fill_field(&host, t(lang, Key::OpAmount), &i18n::amount(lang, amd(45_000)), &mut wrong);
    choose(&host, t(lang, Key::OpProof), "cash_receipt", &mut wrong);
    choose(&host, t(lang, Key::OpNature), "asset", &mut wrong);

    let photo = file(&png(2_400, 1_200).await, "IMG_2041.png", "image/png");
    let pdf = file(b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n", "receipt.pdf", "application/pdf");
    match host.query_selector("form.operation input[type=file]").ok().flatten() {
        Some(input) => choose_files(input.unchecked_ref(), &[photo, pdf]),
        None => wrong.push("нет поля файлов".to_owned()),
    }
    if !wait_selector(&host, ".file-row:nth-child(2)").await {
        wrong.push("файлы не появились в форме".to_owned());
    }
    let names: Vec<String> = host
        .query_selector_all(".file-row .file-link")
        .map(|links| {
            (0..links.length())
                .filter_map(|at| links.item(at))
                .filter_map(|link| link.text_content())
                .collect()
        })
        .unwrap_or_default();
    if names != ["IMG_2041.jpg", "receipt.pdf"] {
        wrong.push(format!("имена файлов — {names:?}"));
    }
    press(&host, t(lang, Key::ProfileSave), &mut wrong);
    if !wait_for(&host, t(lang, Key::LedgerSaved)).await {
        wrong.push("расход с файлами не сохранился".to_owned());
    }

    let (operations, files) = stored().await;
    let Some(operation) = operations.first() else {
        panic!("записи нет: {wrong:#?}");
    };
    let refs: Vec<RecordId> = operation.data().attachments.iter().map(|file| file.id).collect();
    let ids: Vec<RecordId> = files.iter().map(Attachment::id).collect();
    if refs.len() != 2 || refs.iter().any(|id| !ids.contains(id)) {
        wrong.push(format!("ссылки {refs:?}, а записи {ids:?}"));
    }
    if files.iter().any(|file| file.owner() != operation.id()) {
        wrong.push("вложение не этой записи".to_owned());
    }
    match files.iter().find(|file| file.file().kind() == FileKind::Jpeg) {
        Some(photo) => {
            let size = size_of(photo.file().bytes(), "image/jpeg").await;
            if size != (1_600, 800) {
                wrong.push(format!("фото не уменьшено: {size:?}"));
            }
        }
        None => wrong.push("фото не легло JPEG".to_owned()),
    }
    let files_shown = fill(t(lang, Key::LedgerFiles), &["2"]);
    if !host.text_content().unwrap_or_default().contains(&files_shown) {
        wrong.push("в строке не сказано, сколько файлов".to_owned());
    }

    // Форма снова: файлы — ссылками, фото — миниатюрой. Убрать PDF.
    open_entry(&host, "Office chair", &mut wrong);
    if !wait_selector(&host, r#".file-row:nth-child(2) a.file-link[href^="blob:"]"#).await {
        wrong.push("у прежних файлов нет ссылок".to_owned());
    }
    if host.query_selector("img.file-thumb").ok().flatten().is_none() {
        wrong.push("у фото нет миниатюры".to_owned());
    }
    match row_with(&host, "ul.file-list", "receipt.pdf").and_then(|row| button(&row, &[t(lang, Key::Remove)])) {
        Some(remove) => remove.click(),
        None => wrong.push("нет «Убрать» у PDF".to_owned()),
    }
    sleep(50).await;
    press(&host, t(lang, Key::ProfileSave), &mut wrong);
    if !wait_for(&host, t(lang, Key::LedgerSaved)).await {
        wrong.push("правка не сохранилась".to_owned());
    }
    let (operations, files) = stored().await;
    let kept = operations.first().map_or(0, |operation| operation.data().attachments.len());
    if kept != 1 {
        wrong.push(format!("у записи {kept} файлов, а не один"));
    }
    match files.iter().find(|file| file.file().kind() == FileKind::Pdf) {
        Some(pdf) if pdf.is_removed() && !pdf.file().bytes().is_empty() => {}
        Some(_) => wrong.push("убранный PDF не помечен или стёрт".to_owned()),
        None => wrong.push("запись PDF пропала".to_owned()),
    }

    drop(mounted);
    host.remove();
    set_hash("");
    forget_device();
    assert!(wrong.is_empty(), "файлы: {wrong:#?}");
}

/// Курс документа: у выпущенного счёта в долларах без курса книга говорит
/// «нет курса ЦБ», карточка счёта даёт указать курс, и доход в драмах
/// появляется и в карточке, и в итоге книги. В базе — курс на день
/// оказания услуги.
#[wasm_bindgen_test]
async fn document_rate_reaches_the_ledger() {
    let invoice = issued_invoice(None).await;
    let lang = Lang::Am;
    Device::new().set_ui_lang(lang);
    let (host, mounted) = mount_at("#/ledger");
    let mut wrong = Vec::new();
    if !wait_for(&host, t(lang, Key::LedgerNoRate)).await {
        wrong.push("у счёта без курса нет пометки".to_owned());
    }
    if !host.text_content().unwrap_or_default().contains(&fill(t(lang, Key::LedgerUnrated), &["1"])) {
        wrong.push("нет слов о строке без курса".to_owned());
    }

    set_hash(&crate::ui::route::Screen::Document(invoice.id()).href());
    if !wait_for(&host, t(lang, Key::DocRateSet)).await {
        wrong.push("в карточке нет «Указать курс ЦБ»".to_owned());
    }
    press(&host, t(lang, Key::DocRateSet), &mut wrong);
    sleep(50).await;
    fill_field(&host, &fill(t(lang, Key::OpRate), &["USD"]), &i18n::rate(lang, 3_880_000), &mut wrong);
    press(&host, t(lang, Key::DocRateSave), &mut wrong);
    if !wait_for(&host, t(lang, Key::DocRateSaved)).await {
        wrong.push("курс не сохранился".to_owned());
    }
    let in_drams = fill(t(lang, Key::LedgerInDrams), &[&i18n::money(lang, amd(931_200))]);
    if !wait_for(&host, &in_drams).await {
        wrong.push("в карточке нет суммы в драмах".to_owned());
    }

    set_hash("#/ledger");
    sleep(100).await;
    let ledger_text = host
        .query_selector("#ledger")
        .ok()
        .flatten()
        .and_then(|ledger| ledger.text_content())
        .unwrap_or_default();
    if !ledger_text.contains(&i18n::money(lang, amd(931_200))) {
        wrong.push("доход счёта не вошёл в итог книги".to_owned());
    }
    if ledger_text.contains(t(lang, Key::LedgerNoRate)) {
        wrong.push("пометка «нет курса» осталась".to_owned());
    }
    let store = Store::open(|_| {}).await.expect("база открыта");
    let saved = store
        .read()
        .expect("транзакция чтения")
        .get::<Document>(invoice.id())
        .await
        .expect("счёт читается")
        .expect("счёт есть");
    let rate = saved.data().rate.map(|rate| (rate.per_unit_e4(), rate.date()));
    if rate != Some((3_880_000, invoice.data().service_date)) {
        wrong.push(format!("курс в базе — {rate:?}"));
    }
    if saved.data().kind != DocKind::Invoice || saved.number() != invoice.number() {
        wrong.push("документ изменился не только курсом".to_owned());
    }

    drop(mounted);
    host.remove();
    set_hash("");
    forget_device();
    assert!(wrong.is_empty(), "курс документа: {wrong:#?}");
}

/// Строка налога у дохода: вопрос свёрнут, а выбор раскрывает ставку
/// и подсказку. У ИП, чьи коды облагаются по-разному, вид деятельности
/// обязателен, пока строка не выбрана: своей строке код не нужен. Запись ложится
/// с этой строкой, открывается снова раскрытой и считается ею: 300 000 ֏ × 10 %
/// = 30 000 ֏, а не 1 % кода.
#[wasm_bindgen_test]
async fn income_row_is_chosen_in_the_ledger_form() {
    crate::store::delete_database(crate::store::DB_NAME).await;
    let mut data = profile();
    data.activities = ["62.01.0", "74.10.0"].map(|text| ActivityCode::parse(text).expect("код")).to_vec();
    let saved = Profile::save(None, data, clock::now()).expect("годный профиль").expect("новый");
    let mut writes = Writes::default();
    writes.insert(&saved);
    commit(writes).await;
    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let (host, mounted) = mount_at("#/ledger");
    let mut wrong = Vec::new();
    if !wait_for(&host, t(lang, Key::LedgerAddIncome)).await {
        wrong.push("экран книги не открылся".to_owned());
    }
    press(&host, t(lang, Key::LedgerAddIncome), &mut wrong);
    sleep(50).await;
    let open = || {
        host.query_selector("#ledger details.income-kind")
            .ok()
            .flatten()
            .map(|details| details.has_attribute("open"))
    };
    if open() != Some(false) {
        wrong.push(format!("вопрос о строке налога не свёрнут: {:?}", open()));
    }
    fill_field(&host, t(lang, Key::OpText), "Лицензия программы", &mut wrong);
    fill_field(&host, t(lang, Key::OpAmount), &i18n::amount(lang, amd(300_000)), &mut wrong);
    press(&host, t(lang, Key::ProfileSave), &mut wrong);
    sleep(50).await;
    if !host.text_content().unwrap_or_default().contains(t(lang, Key::IncomeActivityMissing)) {
        wrong.push("без кода и без строки форма не сказала, чего не хватает".to_owned());
    }
    if !stored().await.0.is_empty() {
        wrong.push("запись без кода и строки сохранилась".to_owned());
    }
    match host.query_selector("#ledger details.income-kind summary").ok().flatten() {
        Some(summary) => summary.unchecked_into::<HtmlElement>().click(),
        None => wrong.push("у вопроса нет заголовка".to_owned()),
    }
    if open() != Some(true) {
        wrong.push("вопрос не раскрылся".to_owned());
    }
    choose(&host, t(lang, Key::IncomeKindLabel), "5", &mut wrong);
    sleep(50).await;
    let shown = host.text_content().unwrap_or_default();
    let rate = fill(t(lang, Key::IncomeKindRate), &[&i18n::percent(lang, 1_000)]);
    if !shown.contains(&rate) || !shown.contains(t(lang, Key::IncomeKindRentHint)) {
        wrong.push("у выбранной строки нет ставки и подсказки".to_owned());
    }
    if shown.contains(t(lang, Key::IncomeActivityMissing)) {
        wrong.push("строка выбрана, а форма всё просит вид деятельности".to_owned());
    }
    press(&host, t(lang, Key::ProfileSave), &mut wrong);
    if !wait_for(&host, t(lang, Key::LedgerSaved)).await {
        wrong.push("доход не сохранился".to_owned());
    }
    match stored().await.0.as_slice() {
        [operation] => {
            let data = operation.data();
            if (data.income_kind, data.activity) != (Some(IncomeKind::Rent), None) {
                wrong.push(format!("в базе: {:?} {:?}", data.income_kind, data.activity));
            }
        }
        other => wrong.push(format!("записей: {}", other.len())),
    }
    open_entry(&host, "Лицензия программы", &mut wrong);
    sleep(100).await;
    if open() != Some(true) {
        wrong.push(format!("запись открылась со свёрнутой строкой: {:?}", open()));
    }
    if select(&host, t(lang, Key::IncomeKindLabel), 0).map(|list| list.value()).as_deref() != Some("5") {
        wrong.push("запись открылась с другой строкой".to_owned());
    }
    press(&host, t(lang, Key::Cancel), &mut wrong);
    sleep(50).await;
    set_hash("#/tax");
    if !wait_for(&host, &i18n::money(lang, amd(30_000))).await {
        wrong.push("налог посчитан не строкой дохода".to_owned());
    }
    drop(mounted);
    host.remove();
    set_hash("");
    forget_device();
    assert!(wrong.is_empty(), "строка налога у дохода: {wrong:#?}");
}

/// Расход рядом с доходом со своей строкой: при одном коде форма спрашивает вид
/// деятельности только в этом квартале — и у прямого тоже: он может быть
/// и расходом этого дохода (задача 79), — записи ложатся с кодом, правка
/// прежней записи без ответа «прямой или сбытовой» просит ответа, а про расход
/// без вида экран налога говорит, что его доля ушла на строку без вычета.
#[wasm_bindgen_test]
async fn expense_form_asks_the_code_beside_an_income_row() {
    crate::store::delete_database(crate::store::DB_NAME).await;
    let now = clock::now();
    let today = clock::today().expect("часы с датой");
    let code = ActivityCode::parse("74.10.0").expect("код");
    let mut data = profile();
    data.activities = vec![code];
    let saved = Profile::save(None, data, now).expect("годный профиль").expect("новый");
    let line = |text: &str| Line::new(text).expect("текст");
    let income = |text: &str, flow: Flow, proof, nature, income_kind| OperationData {
        flow,
        date: today,
        paid: None,
        amount: amd(400_000),
        rate: None,
        text: line(text),
        client: None,
        supplier: Line::default(),
        proof,
        nature,
        cost: None,
        activity: None,
        income_kind,
        attachments: Vec::new(),
    };
    let sale = Operation::create(income("Продажа ноутбука", Flow::Income, None, None, Some(IncomeKind::Assets)), now)
        .expect("годна");
    let design = Operation::create(income("Дизайн сайта", Flow::Income, None, None, None), now).expect("годна");
    let hosting = Operation::create(
        income("Хостинг", Flow::Expense, Some(Proof::SettlementInvoice), Some(Nature::Ordinary), None),
        now,
    )
    .expect("годна");
    let mut writes = Writes::default();
    writes.insert(&saved);
    writes.insert(&sale);
    writes.insert(&design);
    writes.insert(&hosting);
    commit(writes).await;
    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let (host, mounted) = mount_at("#/ledger");
    let mut wrong = Vec::new();
    if !wait_for(&host, "Продажа ноутбука").await {
        wrong.push("запись не видна".to_owned());
    }
    press(&host, t(lang, Key::LedgerAddExpense), &mut wrong);
    sleep(100).await;
    if select(&host, t(lang, Key::OpActivity), 0).is_none() {
        wrong.push("рядом с доходом со своей строкой форма не спросила вид".to_owned());
    }
    // В прошлом году такого дохода нет: один код, выбирать нечего.
    let last_year = Date::new(today.year() - 1, 6, 15).expect("дата").iso();
    match field(&host, t(lang, Key::OpReceived), 0) {
        Some(input) => type_into(&input, &last_year),
        None => wrong.push("нет поля даты расхода".to_owned()),
    }
    sleep(100).await;
    if select(&host, t(lang, Key::OpActivity), 0).is_some() {
        wrong.push("в другом квартале форма спросила вид при одном коде".to_owned());
    }
    match field(&host, t(lang, Key::OpReceived), 0) {
        Some(input) => type_into(&input, &today.iso()),
        None => wrong.push("нет поля даты расхода".to_owned()),
    }
    sleep(100).await;
    fill_field(&host, t(lang, Key::OpText), "Бухгалтер", &mut wrong);
    fill_field(&host, t(lang, Key::OpAmount), &i18n::amount(lang, amd(100_000)), &mut wrong);
    choose(&host, t(lang, Key::OpProof), "settlement_invoice", &mut wrong);
    choose(&host, t(lang, Key::OpCost), "overhead", &mut wrong);
    choose(&host, t(lang, Key::OpActivity), "74.10.0", &mut wrong);
    press(&host, t(lang, Key::ProfileSave), &mut wrong);
    if !wait_for(&host, t(lang, Key::LedgerSaved)).await {
        wrong.push("расход не сохранился".to_owned());
    }
    press(&host, t(lang, Key::LedgerAddExpense), &mut wrong);
    sleep(100).await;
    fill_field(&host, t(lang, Key::OpText), "Товар", &mut wrong);
    fill_field(&host, t(lang, Key::OpAmount), &i18n::amount(lang, amd(50_000)), &mut wrong);
    choose(&host, t(lang, Key::OpProof), "settlement_invoice", &mut wrong);
    choose(&host, t(lang, Key::OpCost), "direct", &mut wrong);
    sleep(100).await;
    if select(&host, t(lang, Key::OpActivity), 0).is_none() {
        wrong.push("у прямого рядом с доходом со своей строкой форма не спросила вид".to_owned());
    }
    choose(&host, t(lang, Key::OpActivity), "74.10.0", &mut wrong);
    press(&host, t(lang, Key::ProfileSave), &mut wrong);
    if !wait_for(&host, t(lang, Key::LedgerSaved)).await {
        wrong.push("прямой расход не сохранился".to_owned());
    }
    // Прежняя запись без ответа «прямой или сбытовой»: правка его просит.
    open_entry(&host, "Хостинг", &mut wrong);
    sleep(100).await;
    press(&host, t(lang, Key::ProfileSave), &mut wrong);
    if !wait_for(&host, t(lang, Key::OpCostMissing)).await {
        wrong.push("правка прежнего расхода не спросила, прямой ли он".to_owned());
    }
    choose(&host, t(lang, Key::OpCost), "overhead", &mut wrong);
    press(&host, t(lang, Key::ProfileSave), &mut wrong);
    if !wait_for(&host, t(lang, Key::LedgerSaved)).await {
        wrong.push("прежний расход не сохранился".to_owned());
    }
    let (operations, _) = stored().await;
    let data = |text: &str| {
        operations
            .iter()
            .find(|operation| operation.data().text.as_str() == text)
            .map(|operation| (operation.data().cost, operation.data().activity))
    };
    if data("Бухгалтер") != Some((Some(Cost::Overhead), Some(code))) {
        wrong.push(format!("сбытовой расход лёг не так: {:?}", data("Бухгалтер")));
    }
    if data("Товар") != Some((Some(Cost::Direct), Some(code))) {
        wrong.push(format!("прямой расход лёг не так: {:?}", data("Товар")));
    }
    if data("Хостинг") != Some((Some(Cost::Overhead), None)) {
        wrong.push(format!("прежний расход лёг не так: {:?}", data("Хостинг")));
    }
    // Доля ушла только у хостинга: он без вида, а товар и бухгалтер — с кодом.
    set_hash("#/tax");
    if !wait_for(&host, &fill(t(lang, Key::TaxRefusedSplit), &["1"])).await {
        wrong.push("экран налога не сказал о расходе без вида".to_owned());
    }
    drop(mounted);
    host.remove();
    set_hash("");
    forget_device();
    assert!(wrong.is_empty(), "вид у расхода: {wrong:#?}");
}

/// Коды облагаются по разным строкам: прямой расход без вида деятельности
/// форма не пишет и говорит почему — он идёт в строку своего вида целиком
/// (задача 79), — у командировки этого не требует, а с кодом пишет.
#[wasm_bindgen_test]
async fn a_direct_expense_asks_its_code_among_rows() {
    crate::store::delete_database(crate::store::DB_NAME).await;
    let now = clock::now();
    let design = ActivityCode::parse("74.10.0").expect("код");
    let mut data = profile();
    data.activities = vec![ActivityCode::parse("62.01.0").expect("код"), design];
    let saved = Profile::save(None, data, now).expect("годный профиль").expect("новый");
    let mut writes = Writes::default();
    writes.insert(&saved);
    commit(writes).await;
    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let (host, mounted) = mount_at("#/ledger");
    let mut wrong = Vec::new();
    if !wait_for(&host, t(lang, Key::LedgerAddExpense)).await {
        wrong.push("экран книги не открылся".to_owned());
    }
    press(&host, t(lang, Key::LedgerAddExpense), &mut wrong);
    sleep(100).await;
    fill_field(&host, t(lang, Key::OpText), "Печать каталога", &mut wrong);
    fill_field(&host, t(lang, Key::OpAmount), &i18n::amount(lang, amd(80_000)), &mut wrong);
    choose(&host, t(lang, Key::OpProof), "settlement_invoice", &mut wrong);
    choose(&host, t(lang, Key::OpCost), "direct", &mut wrong);
    press(&host, t(lang, Key::ProfileSave), &mut wrong);
    if !wait_for(&host, t(lang, Key::OpActivityDirect)).await {
        wrong.push("форма не сказала, что прямому расходу нужен вид".to_owned());
    }
    if !stored().await.0.is_empty() {
        wrong.push("прямой расход без вида записан".to_owned());
    }
    // Командировка в вычет не идёт: прямой ли она, не спрашивается, и вид ей
    // не нужен, хотя прежний ответ «прямой» в поле остался (ревью задачи 79).
    choose(&host, t(lang, Key::OpNature), "travel", &mut wrong);
    sleep(100).await;
    if host.text_content().unwrap_or_default().contains(t(lang, Key::OpActivityDirect)) {
        wrong.push("у командировки форма требует вид деятельности".to_owned());
    }
    choose(&host, t(lang, Key::OpNature), "ordinary", &mut wrong);
    choose(&host, t(lang, Key::OpActivity), "74.10.0", &mut wrong);
    press(&host, t(lang, Key::ProfileSave), &mut wrong);
    if !wait_for(&host, t(lang, Key::LedgerSaved)).await {
        wrong.push("расход с видом не сохранился".to_owned());
    }
    let (operations, _) = stored().await;
    let got: Vec<_> = operations.iter().map(|operation| (operation.data().cost, operation.data().activity)).collect();
    if got != [(Some(Cost::Direct), Some(design))] {
        wrong.push(format!("записано {got:?}"));
    }
    drop(mounted);
    host.remove();
    set_hash("");
    forget_device();
    assert!(wrong.is_empty(), "прямой расход среди строк: {wrong:#?}");
}
