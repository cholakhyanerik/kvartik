//! Лицензия в окне (задача 25): пробный период, ввод ключа, запертый выпуск
//! и числа для расчёта — всё окно в настоящем DOM и настоящей IndexedDB.
//! Окно под тестом проверяет подпись ключом пары проб (`ui/license.rs`,
//! `author`): закрытого ключа автора у тестов нет (Правило 9).
//!
//! День начала пробного периода лежит на устройстве (`kvartik.trial`),
//! а устройство у тестов общее: тест ставит свой день и в конце убирает его,
//! иначе выпуск в тестах документов оказался бы заперт.

use ed25519_dalek::{Signer, SigningKey};
use kvartik_core::i18n::{self, Key, Lang, fill, t};
use kvartik_core::license::{self, License, Plan};
use kvartik_core::model::activity::ActivityCode;
use kvartik_core::model::client::Client;
use kvartik_core::model::date::Date;
use kvartik_core::model::document::Document;
use kvartik_core::model::ledger::{Flow, Operation, OperationData};
use kvartik_core::model::money::{Currency, Money};
use kvartik_core::model::profile::Profile;
use kvartik_core::model::text::Line;
use kvartik_core::model::tin::Tin;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::wasm_bindgen_test;
use web_sys::{HtmlElement, HtmlInputElement};

use super::documents::{acme, commit, invoice_data, profile};
use super::{button, mount_at, sleep, type_into, wait_for};
use crate::platform::{clock, set_hash};
use crate::store::device::Device;
use crate::store::{Store, Writes};

/// Закрытый ключ проб — тот же, что у тестов ядра (`license/tests.rs`).
/// Не авторский: им подписаны только ключи тестов.
const SEED: [u8; 32] = [7; 32];

/// ՀՎՀՀ профиля в тестах.
const TIN: &str = "00054321";

/// Пара проб.
pub(in crate::ui) fn signer() -> SigningKey {
    SigningKey::from_bytes(&SEED)
}

/// Ключ «Стандарт» на `tin` до `until`, подписанный парой `seed`, — так его
/// записал бы `kvartik-keygen`.
fn key_text(seed: [u8; 32], tin: &str, until: Date) -> String {
    let tin = Tin::parse(tin).expect("годный ՀՎՀՀ");
    let issued = clock::today().expect("часы с датой");
    let message = license::Key::message(&tin, Plan::Standard, issued, until);
    let signature = SigningKey::from_bytes(&seed).sign(&message).to_bytes();
    license::Key::signed(tin, Plan::Standard, issued, until, signature)
        .expect("срок не раньше выпуска")
        .text()
}

/// Пробный период устройства начался в `start`.
fn trial_from(start: Date) {
    Device::new().set_trial_start(start);
}

/// Забыть пробный период устройства: следующее окно начнёт его сегодня.
fn forget_trial() {
    if let Some(storage) = web_sys::window().and_then(|window| window.local_storage().ok().flatten()) {
        let _ = storage.remove_item("kvartik.trial");
    }
}

/// База: профиль с ՀՎՀՀ и кодом дизайна, клиент, черновик счёта и доход
/// в прошлом квартале — чтобы у чисел для расчёта было что показать.
async fn base_with_draft() -> Document {
    crate::store::delete_database(crate::store::DB_NAME).await;
    let now = clock::now();
    let today = clock::today().expect("часы с датой");
    let previous = today.quarter().previous().expect("прошлый квартал есть");
    let mut data = profile();
    data.tin = Some(Tin::parse(TIN).expect("годный ՀՎՀՀ"));
    data.activities = vec![ActivityCode::parse("74.10.0").expect("код")];
    let saved = Profile::save(None, data, now).expect("годный профиль").expect("новый");
    let client = Client::create(acme(), now).expect("годный клиент");
    let draft = Document::create(invoice_data(&client), now).expect("черновик");
    let income = Operation::create(
        OperationData {
            flow: Flow::Income,
            date: previous.first_day(),
            paid: None,
            amount: Money::new(1_000_000, Currency::Amd),
            rate: None,
            text: Line::new("Дизайн сайта").expect("текст"),
            client: None,
            supplier: Line::default(),
            proof: None,
            nature: None,
            activity: None,
            attachments: Vec::new(),
        },
        now,
    )
    .expect("годная запись");
    let mut writes = Writes::default();
    writes.insert(&saved);
    writes.insert(&client);
    writes.insert(&draft);
    writes.insert(&income);
    commit(writes).await;
    draft
}

/// Открыть черновик из списка документов — форма на шаге проверки.
async fn open_draft(host: &HtmlElement) -> bool {
    set_hash("#/documents");
    for _ in 0..50 {
        if let Some(entry) = host.query_selector("a.entry[href=\"#/documents\"]").ok().flatten() {
            entry.unchecked_into::<HtmlElement>().click();
            sleep(200).await;
            return true;
        }
        sleep(100).await;
    }
    false
}

/// Пробный период кончился: на главном — карточка со сроком; в форме
/// черновика вместо «Выпустить» — слова о ключе, а «Сохранить черновик»
/// осталось; числа прошлого квартала, кончившегося после срока, заперты,
/// и экран называет, по какой день кварталы открыты; экран лицензии
/// говорит, что заперто, а что работает.
#[wasm_bindgen_test]
async fn an_ended_trial_locks_issuing_and_numbers() {
    base_with_draft().await;
    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let today = clock::today().expect("часы с датой");
    let previous = today.quarter().previous().expect("прошлый квартал есть");
    // Срок кончился за неделю до начала прошлого квартала.
    let start = previous.first_day().add_days(-20).expect("дата");
    let last = license::trial_last(start).expect("дата");
    trial_from(start);
    let ended = fill(t(lang, Key::LicenseTrialEnded), &[&i18n::date(lang, last)]);

    let (host, mounted) = mount_at("#/");
    let mut wrong = Vec::new();
    if !wait_for(&host, &ended).await {
        wrong.push("на главном нет карточки о конце пробного периода".to_owned());
    }
    if !open_draft(&host).await {
        wrong.push("в списке нет черновика".to_owned());
    }
    if !wait_for(&host, t(lang, Key::DocIssueLocked)).await {
        wrong.push("в форме нет слов о запертом выпуске".to_owned());
    }
    if button(&host, &[t(lang, Key::DocIssue)]).is_some() {
        wrong.push("«Выпустить» осталось".to_owned());
    }
    if button(&host, &[t(lang, Key::DocSaveDraft)]).is_none() {
        wrong.push("«Сохранить черновик» пропало".to_owned());
    }

    set_hash("#/report");
    let open = fill(t(lang, Key::LicenseQuartersOpen), &[&i18n::date(lang, last)]);
    if !wait_for(&host, t(lang, Key::ReportLocked)).await {
        wrong.push(format!("числа за {previous:?} не заперты"));
    }
    let report = host
        .query_selector("#report")
        .ok()
        .flatten()
        .and_then(|node| node.text_content())
        .unwrap_or_default();
    if !report.contains(&open) {
        wrong.push("экран чисел не сказал, какие кварталы открыты".to_owned());
    }
    if report.contains(t(lang, Key::ReportCopyAll)) {
        wrong.push("числа видны".to_owned());
    }

    set_hash("#/license");
    if !wait_for(&host, t(lang, Key::LicenseLocked)).await {
        wrong.push("экран лицензии не сказал, что заперто".to_owned());
    }

    drop(mounted);
    host.remove();
    forget_trial();
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// Видимая кнопка раздела `scope` с этой подписью: такие же есть и на
/// спрятанных экранах.
fn button_in(host: &HtmlElement, scope: &str, label: &str) -> Option<HtmlElement> {
    let buttons = host.query_selector_all(&format!("{scope} button")).ok()?;
    (0..buttons.length())
        .filter_map(|at| buttons.item(at))
        .map(JsCast::unchecked_into::<HtmlElement>)
        .find(|button| button.text_content().unwrap_or_default().trim() == label && button.offset_width() > 0)
}

/// Ключи окно читает и после того, как первой их прочитала форма,
/// а её закрыли: пустая база, форма профиля открыта и закрыта, потом
/// восстановлена копия с ключом — экран лицензии показывает ключ из копии
/// без перезагрузки (ревью задачи 25: слежение за базой жило у формы
/// и умирало с ней). До восстановления тот же экран говорит, что выпуск
/// заперт: без слежения он так и остался бы. Копия — образец 0.0.60, ключ
/// «Стандарт» на ՀՎՀՀ 07654321 до 31 октября 2027 года подписан парой проб.
#[wasm_bindgen_test]
async fn a_restored_key_is_read_after_the_form_that_read_keys_first() {
    crate::store::delete_database(crate::store::DB_NAME).await;
    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let today = clock::today().expect("часы с датой");
    trial_from(today.add_days(-100).expect("дата"));

    let (host, mounted) = mount_at("#/profile");
    let mut wrong = Vec::new();
    if !wait_for(&host, t(lang, Key::ProfileFill)).await {
        wrong.push("пустая база — а профиль не предлагают заполнить".to_owned());
    }
    match button_in(&host, "#profile", t(lang, Key::ProfileFill)) {
        Some(fill) => fill.click(),
        None => wrong.push("нет «Заполнить профиль»".to_owned()),
    }
    sleep(300).await;
    match button_in(&host, "#profile", t(lang, Key::Cancel)) {
        Some(cancel) => cancel.click(),
        None => wrong.push("форма профиля не открылась".to_owned()),
    }
    sleep(300).await;

    set_hash("#/license");
    if !wait_for(&host, t(lang, Key::LicenseLocked)).await {
        wrong.push("до восстановления экран лицензии не сказал, что выпуск заперт".to_owned());
    }

    set_hash("#/backup");
    let input = host
        .query_selector(r#"input[type="file"][accept=".json,application/json"]"#)
        .ok()
        .flatten()
        .map(JsCast::unchecked_into::<HtmlInputElement>);
    match &input {
        Some(input) => super::choose_file(
            input,
            "kvartik-2026-10-27.json",
            include_str!("../../../../core/tests/backups/v11/kvartik-2026-10-27.json"),
        ),
        None => wrong.push("нет поля выбора копии".to_owned()),
    }
    if !wait_for(&host, t(lang, Key::BackupFilePassword)).await {
        wrong.push("копия не открылась".to_owned());
    }
    match super::field(&host, t(lang, Key::BackupFilePassword), 0) {
        Some(password) => type_into(&password, "Պահուստ копия sample 0.0.60"),
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

    set_hash("#/license");
    let until = Date::new(2027, 10, 31).expect("дата");
    let term = fill(t(lang, Key::LicenseKeyUntil), &[t(lang, Key::PlanStandard), &i18n::date(lang, until)]);
    if !wait_for(&host, &term).await {
        wrong.push("после восстановления экран лицензии не показал ключ из копии".to_owned());
    }

    drop(mounted);
    host.remove();
    forget_trial();
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// Ключ вводится на экране лицензии: не ключ, чужая подпись и повтор —
/// словами; годный ключ на ՀՎՀՀ профиля ложится одной записью, срок — его,
/// и выпуск снова открыт; ключ на другой ՀՎՀՀ тоже ложится, но окно
/// говорит, что он не тот. В профиле, пока ՀՎՀՀ правят, — предупреждение,
/// что ключ выдан на прежний.
#[wasm_bindgen_test]
async fn a_key_opens_issuing_for_the_profile_tin() {
    base_with_draft().await;
    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let today = clock::today().expect("часы с датой");
    trial_from(today.add_days(-100).expect("дата"));
    let until = today.add_days(365).expect("дата");

    let (host, mounted) = mount_at("#/license");
    let mut wrong = Vec::new();
    if !wait_for(&host, t(lang, Key::LicenseLocked)).await {
        wrong.push("экран лицензии не открылся запертым".to_owned());
    }
    let enter = |text: &str| {
        let area = host.query_selector("#license textarea").ok().flatten();
        match area {
            Some(area) => {
                type_into(&area.unchecked_into::<HtmlInputElement>(), text);
                button(&host, &[t(lang, Key::LicenseEnter)]).map(|button| button.click()).is_some()
            }
            None => false,
        }
    };
    for (typed, said) in [
        ("не ключ".to_owned(), Key::LicenseMalformed),
        (key_text([9; 32], TIN, until), Key::LicenseForged),
    ] {
        if !enter(&typed) || !wait_for(&host, t(lang, said)).await {
            wrong.push(format!("на «{typed:.20}» нет «{}»", t(lang, said)));
        }
    }
    let good = key_text(SEED, TIN, until);
    if !enter(&good.to_lowercase().replace('-', " ")) || !wait_for(&host, t(lang, Key::LicenseEntered)).await {
        wrong.push("годный ключ не введён".to_owned());
    }
    let term = fill(t(lang, Key::LicenseKeyUntil), &[t(lang, Key::PlanStandard), &i18n::date(lang, until)]);
    if !wait_for(&host, &term).await {
        wrong.push("срок ключа не показан".to_owned());
    }
    if !enter(&good) || !wait_for(&host, t(lang, Key::LicenseKnown)).await {
        wrong.push("повтор ключа не сказан".to_owned());
    }
    let other = key_text(SEED, "07654321", until);
    let mismatch = fill(t(lang, Key::LicenseOtherTin), &["07654321", TIN]);
    if !enter(&other) || !wait_for(&host, &mismatch).await {
        wrong.push("ключ на другой ՀՎՀՀ не назван чужим".to_owned());
    }
    let store = Store::open(|_| {}).await.expect("база открыта");
    let stored = store
        .read()
        .expect("транзакция чтения")
        .list::<License>()
        .await
        .expect("ключи читаются")
        .records;
    if stored.len() != 2 {
        wrong.push(format!("записей ключей {}, а не 2", stored.len()));
    }

    if !open_draft(&host).await {
        wrong.push("в списке нет черновика".to_owned());
    }
    if !wait_for(&host, t(lang, Key::DocIssueHint)).await || button(&host, &[t(lang, Key::DocIssue)]).is_none() {
        wrong.push("с ключом «Выпустить» не вернулось".to_owned());
    }
    if host.text_content().unwrap_or_default().contains(t(lang, Key::DocIssueLocked)) {
        wrong.push("с ключом выпуск всё ещё заперт".to_owned());
    }

    // Профиль: ՀՎՀՀ правят — ключ выдан на прежний.
    set_hash("#/profile");
    match button(&host, &[t(lang, Key::ProfileEdit)]) {
        Some(edit) => edit.click(),
        None => {
            sleep(500).await;
            if let Some(edit) = button(&host, &[t(lang, Key::ProfileEdit)]) {
                edit.click();
            } else {
                wrong.push("нет «Изменить» у профиля".to_owned());
            }
        }
    }
    sleep(200).await;
    let warning = fill(t(lang, Key::ProfileTinLicense), &[TIN]);
    match super::field(&host, t(lang, Key::ProfileTin), 0) {
        Some(input) => {
            if host.text_content().unwrap_or_default().contains(&warning) {
                wrong.push("предупреждение до правки ՀՎՀՀ".to_owned());
            }
            type_into(&input, "07654321");
            if !wait_for(&host, &warning).await {
                wrong.push("нет предупреждения о ключе на прежний ՀՎՀՀ".to_owned());
            }
        }
        None => wrong.push("нет поля ՀՎՀՀ в форме профиля".to_owned()),
    }

    drop(mounted);
    host.remove();
    forget_trial();
    assert!(wrong.is_empty(), "{wrong:#?}");
}
