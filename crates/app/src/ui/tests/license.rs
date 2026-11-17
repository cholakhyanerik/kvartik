//! Лицензия в окне (задача 25): пробный период, ввод ключа, запертый выпуск
//! и числа для расчёта — всё окно в настоящем DOM и настоящей IndexedDB.
//! Окно под тестом проверяет подпись ключом пары проб (`ui/license.rs`,
//! `author`): закрытого ключа автора у тестов нет (Правило 9).
//!
//! День начала пробного периода лежит на устройстве (`kvartik.trial`),
//! а устройство у тестов общее: тест ставит свой день и в конце убирает его,
//! иначе выпуск в тестах документов оказался бы заперт.

use ed25519_dalek::{Signer, SigningKey};
use kvartik_core::format::Stored;
use kvartik_core::i18n::{self, Key, Lang, fill, t};
use kvartik_core::license::{self, License, OfficeLicense, Plan};
use kvartik_core::model::activity::ActivityCode;
use kvartik_core::model::office::Ip;
use kvartik_core::model::client::Client;
use kvartik_core::model::date::Date;
use kvartik_core::model::document::Document;
use kvartik_core::model::ledger::{Flow, Operation, OperationData};
use kvartik_core::model::money::{Currency, Money};
use kvartik_core::model::profile::Profile;
use kvartik_core::model::record::Timestamp;
use kvartik_core::model::text::Line;
use kvartik_core::model::tin::Tin;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::wasm_bindgen_test;
use web_sys::{HtmlElement, HtmlInputElement};

use super::documents::{acme, commit, invoice_data, profile, wait_until};
use super::{button, launch_at, mount_at, shown_screens, sleep, type_into, wait_for};
use crate::platform::{clock, hash, set_hash};
use crate::store::device::Device;
use crate::store::{Store, Writes};
use crate::ui::route::Screen;

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
    let message = license::Key::message(&tin, Plan::Standard, issued, until, None);
    let signature = SigningKey::from_bytes(&seed).sign(&message).to_bytes();
    license::Key::signed(tin, Plan::Standard, issued, until, None, signature)
        .expect("срок не раньше выпуска")
        .text()
}

/// Ключ «Pro» на `seats` мест на `tin` до `until`, подписанный парой проб, —
/// так его записал бы `kvartik-keygen` (задача 38).
fn pro_text(tin: &str, until: Date, seats: u8) -> String {
    let tin = Tin::parse(tin).expect("годный ՀՎՀՀ");
    let issued = clock::today().expect("часы с датой");
    let message = license::Key::message(&tin, Plan::Pro, issued, until, Some(seats));
    let signature = signer().sign(&message).to_bytes();
    license::Key::signed(tin, Plan::Pro, issued, until, Some(seats), signature)
        .expect("годный ключ Pro")
        .text()
}

/// Пробный период устройства начался в `start`.
pub(super) fn trial_from(start: Date) {
    Device::new().set_trial_start(start);
}

/// Забыть пробный период устройства — следующее окно начнёт его сегодня, —
/// и первый день данных (задача 99), и что устройство помнит о ключе базы:
/// каким его нашла часть и о конце
/// какого сказано окном (задача 92), — и что сказано о конце пробного
/// периода (задача 93).
pub(super) fn forget_trial() {
    if let Some(storage) = web_sys::window().and_then(|window| window.local_storage().ok().flatten()) {
        for name in [
            "kvartik.trial",
            "kvartik.trial_data",
            "kvartik.license",
            "kvartik.license_told",
            "kvartik.trial_told",
            "kvartik.pro_told",
        ] {
            let _ = storage.remove_item(name);
        }
    }
}

/// Забыть, каким срок базы в прошлый раз нашла часть окна: метка срока тогда
/// встанет, только когда часть проверит подписи снова, — и решит, вставать
/// ли окну о конце срока. Проверка «окно не встало» без этого могла успеть
/// раньше части и пропустить окно (ревью задачи 93).
fn forget_found_term() {
    if let Some(storage) = web_sys::window().and_then(|window| window.local_storage().ok().flatten()) {
        let _ = storage.remove_item("kvartik.license");
    }
}

/// Текст экрана лицензии.
fn license_text(host: &HtmlElement) -> String {
    host.query_selector("#license")
        .ok()
        .flatten()
        .and_then(|node| node.text_content())
        .unwrap_or_default()
}

/// Число дней — как его пишет окно: «365 дней», «1 день».
fn days(lang: Lang, n: i32) -> String {
    fill(
        i18n::plural(lang, u64::from(n.unsigned_abs()), Key::DaysOne, Key::DaysFew, Key::DaysMany),
        &[&i18n::integer(lang, i64::from(n))],
    )
}

/// Срок ключа «Стандарт» до `until` словами экрана лицензии — со днями,
/// которые остались, считая сегодня.
fn key_until(lang: Lang, until: Date) -> String {
    plan_until(lang, Key::PlanStandard, until)
}

/// Срок ключа тарифа `plan` до `until` словами экрана лицензии.
fn plan_until(lang: Lang, plan: Key, until: Date) -> String {
    let today = clock::today().expect("часы с датой");
    fill(
        t(lang, Key::LicenseKeyUntil),
        &[t(lang, plan), &i18n::date(lang, until), &days(lang, until.days_since(today) + 1)],
    )
}

/// Записать в базу ключ «Стандарт» на ՀՎՀՀ профиля до `until`, подписанный
/// парой проб.
async fn store_key(until: Date) {
    let key = license::Key::read(&key_text(SEED, TIN, until), &signer().verifying_key().to_bytes())
        .expect("ключ пары проб");
    let entered = License::enter(key, &[], clock::now()).expect("новый ключ");
    let mut writes = Writes::default();
    writes.insert(&entered);
    commit(writes).await;
}

/// Метка срока в разделе `scope` — по адресу экрана лицензии; рядом бывает
/// метка копии.
fn term_chip(host: &HtmlElement, scope: &str) -> Option<HtmlElement> {
    host.query_selector(&format!("{scope} a.chip[href=\"{}\"]", Screen::License.href()))
        .ok()
        .flatten()
        .map(JsCast::unchecked_into)
}

/// Подождать метку срока в `scope` с текстом `text`; `None` — не дождались
/// за пять секунд, а что было — в `Err`.
async fn wait_chip(host: &HtmlElement, scope: &str, text: &str) -> Result<HtmlElement, String> {
    let mut seen = String::new();
    for _ in 0..100 {
        if let Some(chip) = term_chip(host, scope) {
            seen = chip.text_content().unwrap_or_default();
            if seen.trim() == text {
                return Ok(chip);
            }
        }
        sleep(50).await;
    }
    Err(seen)
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
            cost: None,
            activity: None,
            income_kind: None,
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

/// Пробный период кончился: на главном — карточка со сроком и «Купить или
/// ввести ключ»; в форме черновика вместо «Выпустить» — слова о ключе,
/// а «Сохранить черновик» осталось; числа прошлого квартала, кончившегося
/// после срока, заперты, и экран называет, по какой день кварталы открыты;
/// экран лицензии говорит, что заперто — «сейчас закрыто» у списка того, что
/// открывает срок, — а что работает, и с каким днём открыты числа кварталов
/// (задача 93), и спрашивает купившего, где его ключ.
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
    let link = host
        .query_selector(&format!("#home .notice a.button[href=\"{}\"]", Screen::License.href()))
        .ok()
        .flatten()
        .and_then(|link| link.text_content())
        .unwrap_or_default();
    if link.trim() != t(lang, Key::LicenseBuyOrEnter) {
        wrong.push(format!("у карточки на главном кнопка «{}»", link.trim()));
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
    if !wait_for(&host, t(lang, Key::LicenseClosedNow)).await {
        wrong.push("экран лицензии не сказал, что заперто".to_owned());
    }
    let screen = license_text(&host);
    let until = fill(t(lang, Key::LicenseAlwaysReportUntil), &[&i18n::date(lang, last)]);
    for (said, what) in [
        (until.as_str(), "с каким днём открыты числа кварталов"),
        (t(lang, Key::LicenseLockedShort), "что заперто, одной строкой"),
        (t(lang, Key::LicenseHaveKey), "«Уже купили ключ?»"),
        (t(lang, Key::LicenseDevicesData), "что данные сами не переходят"),
        (t(lang, Key::LicenseAlwaysUpdates), "что обновления — и без ключа"),
    ] {
        if !screen.contains(said) {
            wrong.push(format!("экран лицензии не сказал {what}"));
        }
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
    if !wait_for(&host, t(lang, Key::LicenseClosedNow)).await {
        wrong.push("до восстановления экран лицензии не сказал, что выпуск заперт".to_owned());
    }

    match super::backup_file(&host).await {
        Some(input) => super::choose_file(
            &input,
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
    let term = key_until(lang, until);
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
    if !wait_for(&host, t(lang, Key::LicenseClosedNow)).await {
        wrong.push("экран лицензии не открылся запертым".to_owned());
    }
    if !license_text(&host).contains(t(lang, Key::LicenseHaveKey)) {
        wrong.push("без ключа нет «Уже купили ключ?»".to_owned());
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
    let term = key_until(lang, until);
    if !wait_for(&host, &term).await {
        wrong.push("срок ключа не показан".to_owned());
    }
    // Ключ идёт: ни «Уже купили ключ?», ни «сейчас закрыто».
    let open = wait_until(|| {
        let screen = license_text(&host);
        !screen.contains(t(lang, Key::LicenseHaveKey)) && !screen.contains(t(lang, Key::LicenseClosedNow))
    })
    .await;
    if !open {
        wrong.push("с ключом экран всё ещё говорит о запертом".to_owned());
    }
    // Строка введённого ключа на ՀՎՀՀ профиля: «до … включительно» есть и в
    // сроке над ключами, и искать надо в ней самой.
    let entry = || {
        let entries = host.query_selector_all("#license li.entry").ok()?;
        (0..entries.length())
            .filter_map(|at| entries.item(at))
            .map(JsCast::unchecked_into::<HtmlElement>)
            .find(|entry| entry.text_content().is_some_and(|text| text.contains(TIN)))
    };
    // День выдачи и последний день — порознь, а не промежутком: ключ,
    // оплаченный заранее, считается со дня после прежнего (условия, п. 4.1).
    let facts = entry().and_then(|entry| entry.text_content()).unwrap_or_default();
    for fact in [
        fill(t(lang, Key::LicenseIssuedOn), &[&i18n::date(lang, today)]),
        fill(t(lang, Key::LicenseLastDay), &[&i18n::date(lang, until)]),
    ] {
        if !facts.contains(&fact) {
            wrong.push(format!("у введённого ключа нет «{fact}»: «{facts}»"));
        }
    }

    // «Скопировать ключ» (задача 93) — записью, как его выдал автор, а не
    // как его вставили; не легло в буфер — словами, и ключ встаёт текстом.
    let copy = || {
        entry()?
            .query_selector("button")
            .ok()
            .flatten()
            .map(JsCast::unchecked_into::<HtmlElement>)
    };
    super::report::stub_clipboard(false);
    match copy() {
        Some(button) if button.text_content().is_some_and(|text| text.trim() == t(lang, Key::LicenseCopy)) => {
            button.click();
            if !wait_for(&host, t(lang, Key::ReportCopied)).await || super::report::copied().as_deref() != Some(good.as_str()) {
                wrong.push(format!("скопировано {:?}, а не ключ", super::report::copied()));
            }
        }
        other => wrong.push(format!("у введённого ключа нет «Скопировать ключ»: {other:?}")),
    }
    super::report::stub_clipboard(true);
    if let Some(button) = copy() {
        button.click();
        let shown = || {
            host.query_selector("#license .key-text")
                .ok()
                .flatten()
                .and_then(|node| node.text_content())
        };
        if !wait_for(&host, t(lang, Key::LicenseCopyFailed)).await || shown().as_deref() != Some(good.as_str()) {
            wrong.push(format!("не скопировалось, а ключ на экране — {:?}", shown()));
        }
    }
    super::report::unstub_clipboard();

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

/// Ждать, пока окно покажет один экран `id`, но не дольше двух секунд.
async fn shows(host: &HtmlElement, id: &str) -> bool {
    for _ in 0..100 {
        if shown_screens(host) == [id] {
            return true;
        }
        sleep(20).await;
    }
    false
}

/// Метка над экранами — на телефоне, в последнюю неделю и после конца срока.
const ABOVE: &str = "main >";

/// Срок на виду (задача 92): метка в меню и на главном экране — сколько дней
/// осталось пробного периода, а над остальными экранами её нет, пока до
/// конца больше недели; в последнюю неделю она цвета напоминания и стоит над
/// каждым экраном, кроме главного, где она и так есть, и экрана лицензии.
/// С ключом метка — дни ключа, а ключ, найденный частью, устройство помнит
/// записью: со следующего открытия метка встанет, не дожидаясь части.
#[wasm_bindgen_test]
async fn the_term_is_always_on_view() {
    base_with_draft().await;
    forget_trial();
    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let today = clock::today().expect("часы с датой");
    let mut wrong = Vec::new();

    // Десять дней пробного периода — метка в меню и на главном, тихая.
    trial_from(today.add_days(-4).expect("дата"));
    let ten = fill(t(lang, Key::LicenseChipTrial), &[&days(lang, 10)]);
    let (host, mounted) = mount_at("#/");
    for scope in ["nav", "#home"] {
        match wait_chip(&host, scope, &ten).await {
            Ok(chip) if chip.class_list().contains("due") => {
                wrong.push(format!("{scope}: за десять дней метка цвета напоминания"));
            }
            Ok(_) => {}
            Err(seen) => wrong.push(format!("{scope}: метка «{seen}», а не «{ten}»")),
        }
    }
    set_hash(&Screen::Clients.href());
    if !shows(&host, "clients").await {
        wrong.push("экран клиентов не открылся".to_owned());
    }
    sleep(200).await;
    if term_chip(&host, ABOVE).is_some() {
        wrong.push("за десять дней метка над экраном клиентов".to_owned());
    }
    drop(mounted);
    host.remove();

    // Три дня — громче и под заголовком каждого экрана. О конце пробного
    // периода уже сказано: окно поверх программы — свой тест ниже.
    let start = today.add_days(-11).expect("дата");
    trial_from(start);
    Device::new().set_trial_told(license::trial_last(start).expect("дата"));
    let three = fill(t(lang, Key::LicenseChipTrial), &[&days(lang, 3)]);
    let (host, mounted) = mount_at("#/");
    match wait_chip(&host, "nav", &three).await {
        Ok(chip) if !chip.class_list().contains("due") => wrong.push("за три дня метка тихая".to_owned()),
        Ok(_) => {}
        Err(seen) => wrong.push(format!("меню: метка «{seen}», а не «{three}»")),
    }
    for screen in Screen::PLAIN {
        set_hash(&screen.href());
        // Над главным и экраном лицензии метки нет: там о сроке и так сказано.
        let wanted = !matches!(screen, Screen::Home | Screen::License);
        let mut seen = false;
        for _ in 0..100 {
            if shown_screens(&host) == [screen.id()] && term_chip(&host, ABOVE).is_some() == wanted {
                seen = true;
                break;
            }
            sleep(20).await;
        }
        if !seen {
            let what = if wanted { "нет метки над экраном" } else { "метка над экраном лишняя" };
            wrong.push(format!("{screen:?}: за три дня {what}"));
        }
    }
    drop(mounted);
    host.remove();

    // Ключ на год — его дни; устройство помнит, каким его нашла часть.
    let until = today.add_days(364).expect("дата");
    store_key(until).await;
    let year = fill(t(lang, Key::LicenseChipKey), &[&days(lang, 365)]);
    let (host, mounted) = mount_at("#/");
    if let Err(seen) = wait_chip(&host, "nav", &year).await {
        wrong.push(format!("с ключом метка «{seen}», а не «{year}»"));
    }
    let remembered = Device::new().key_term(crate::store::DB_NAME);
    if remembered != Some(Some((Plan::Standard, until))) {
        wrong.push(format!("устройство помнит ключ как {remembered:?}"));
    }
    drop(mounted);
    host.remove();
    set_hash("");
    forget_trial();
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// Срок кончился — программа открывается экраном лицензии при запуске
/// (задача 92), а «Назад» возвращает на экран, с которым она открылась.
/// Перезагрузка открывает тот же экран, что был (Правило 2), а запуск, пока
/// срок идёт, остаётся на своём экране.
#[wasm_bindgen_test]
async fn an_ended_term_opens_the_license_screen_at_launch() {
    base_with_draft().await;
    forget_trial();
    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let today = clock::today().expect("часы с датой");
    let mut wrong = Vec::new();
    trial_from(today.add_days(-30).expect("дата"));

    let (host, mounted) = launch_at("#/");
    if !shows(&host, "license").await || hash() != Screen::License.href() {
        wrong.push(format!("запуск: видны {:?}, адрес {}", shown_screens(&host), hash()));
    }
    if let Some(window) = web_sys::window() {
        let _ = window.history().map(|history| history.back());
    }
    if !shows(&host, "home").await {
        wrong.push(format!("«Назад»: видны {:?}", shown_screens(&host)));
    }
    drop(mounted);
    host.remove();

    let (host, mounted) = mount_at("#/clients");
    let ended = t(lang, Key::LicenseChipTrialEnded);
    if let Err(seen) = wait_chip(&host, "nav", ended).await {
        wrong.push(format!("перезагрузка: метка «{seen}», а не «{ended}»"));
    }
    sleep(300).await;
    if shown_screens(&host) != ["clients"] {
        wrong.push(format!("перезагрузка увела с экрана: видны {:?}", shown_screens(&host)));
    }
    drop(mounted);
    host.remove();

    trial_from(today);
    let (host, mounted) = launch_at("#/");
    let fourteen = fill(t(lang, Key::LicenseChipTrial), &[&days(lang, 14)]);
    if let Err(seen) = wait_chip(&host, "nav", &fourteen).await {
        wrong.push(format!("срок идёт: метка «{seen}», а не «{fourteen}»"));
    }
    sleep(300).await;
    if shown_screens(&host) != ["home"] {
        wrong.push(format!("срок идёт, а запуск увёл с главного: видны {:?}", shown_screens(&host)));
    }
    drop(mounted);
    host.remove();
    set_hash("");
    forget_trial();
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// Пробный период едет с данными (задача 99; условия, п. 3, редакция от
/// 15.11.2026): клиент, записанный двадцать дней назад, в браузере, открытом
/// сегодня, приносит и прошедшие дни — экран лицензии говорит, что пробный
/// период кончился и считается с первого дня данных, а устройство запоминает
/// этот день. Со следующего запуска срок знает и главный wasm: у пустой базы
/// части нет, а метка — «кончился».
#[wasm_bindgen_test]
async fn the_trial_travels_with_the_data() {
    crate::store::delete_database(crate::store::DB_NAME).await;
    forget_trial();
    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let today = clock::today().expect("часы с датой");
    let mut wrong = Vec::new();
    trial_from(today);
    let then = Timestamp::from_unix_millis(clock::now().unix_millis() - 20 * 86_400_000);
    let client = Client::create(acme(), then).expect("годный клиент");
    let mut writes = Writes::default();
    writes.insert(&client);
    commit(writes).await;
    let data = license::data_since(license::moments(client.meta()), today).expect("день данных");

    let href = Screen::License.href();
    let (host, mounted) = mount_at(&href);
    let last = license::trial_last(data).expect("дата");
    let ended = fill(t(lang, Key::LicenseTrialEnded), &[&i18n::date(lang, last)]);
    let why = fill(t(lang, Key::LicenseTrialFromData), &[&i18n::date(lang, data)]);
    let said = wait_until(|| {
        let text = license_text(&host);
        text.contains(&ended) && text.contains(&why)
    })
    .await;
    if !said {
        wrong.push(format!("экран лицензии: «{}», а ждали «{ended}» и «{why}»", license_text(&host)));
    }
    if !wait_until(|| Device::new().trial_data() == Some(data)).await {
        wrong.push(format!("устройство помнит день данных как {:?}", Device::new().trial_data()));
    }
    drop(mounted);
    host.remove();

    // Пустая база — части нет, а срок уже известен по записи устройства.
    crate::store::delete_database(crate::store::DB_NAME).await;
    let (host, mounted) = mount_at("#/");
    let chip = t(lang, Key::LicenseChipTrialEnded);
    if let Err(seen) = wait_chip(&host, "nav", chip).await {
        wrong.push(format!("следующий запуск: метка «{seen}», а не «{chip}»"));
    }
    drop(mounted);
    host.remove();
    set_hash("");
    forget_trial();
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// За неделю до конца ключа — окно поверх программы (задача 92): сколько дней
/// и до какого дня; программа под ним неактивна. «Понятно» закрывает его,
/// и больше оно не встаёт, а «Продлить» закрывает и ведёт на экран лицензии.
/// Окно встаёт в конец страницы (`Portal`), а не внутри окна программы,
/// поэтому его ищут во всём `<body>`.
#[wasm_bindgen_test]
async fn a_key_ending_within_a_week_asks_once() {
    base_with_draft().await;
    forget_trial();
    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let today = clock::today().expect("часы с датой");
    trial_from(today.add_days(-30).expect("дата"));
    let until = today.add_days(4).expect("дата");
    store_key(until).await;
    let words = fill(t(lang, Key::LicenseSoonText), &[&days(lang, 5), &i18n::date(lang, until)]);
    let body = super::document().body().expect("есть <body>");
    let dialog = || body.query_selector("[role=dialog]").ok().flatten().is_some();
    let inert = |host: &HtmlElement| {
        host.query_selector(".app")
            .ok()
            .flatten()
            .is_some_and(|app| app.has_attribute("inert"))
    };
    let mut wrong = Vec::new();

    let (host, mounted) = mount_at("#/");
    if !wait_for(&body, &words).await {
        wrong.push("за пять дней до конца ключа окна нет".to_owned());
    }
    if !inert(&host) {
        wrong.push("под окном программа активна".to_owned());
    }
    match button(&body, &[t(lang, Key::LicenseSoonOk)]) {
        Some(ok) => ok.click(),
        None => wrong.push("нет «Понятно»".to_owned()),
    }
    sleep(200).await;
    if dialog() || inert(&host) {
        wrong.push("«Понятно» не закрыло окно".to_owned());
    }
    if Device::new().key_told(crate::store::DB_NAME) != Some(until) {
        wrong.push("устройство не запомнило, что о ключе сказано".to_owned());
    }
    drop(mounted);
    host.remove();

    // Метка — только от части, проверившей подпись (ниже, `forget_found_term`).
    forget_found_term();
    let (host, mounted) = mount_at("#/");
    let five = fill(t(lang, Key::LicenseChipKey), &[&days(lang, 5)]);
    if let Err(seen) = wait_chip(&host, "nav", &five).await {
        wrong.push(format!("метка «{seen}», а не «{five}»"));
    }
    sleep(500).await;
    if dialog() {
        wrong.push("окно встало второй раз".to_owned());
    }
    drop(mounted);
    host.remove();

    if let Some(storage) = web_sys::window().and_then(|window| window.local_storage().ok().flatten()) {
        let _ = storage.remove_item("kvartik.license_told");
    }
    let (host, mounted) = mount_at("#/");
    if !wait_for(&body, &words).await {
        wrong.push("окна нет и после того, как устройство забыло".to_owned());
    }
    match button(&body, &[t(lang, Key::LicenseSoonRenew)]) {
        Some(renew) => renew.click(),
        None => wrong.push("нет «Продлить»".to_owned()),
    }
    if !shows(&host, "license").await || dialog() || inert(&host) {
        wrong.push(format!("«Продлить»: видны {:?}, окно — {}", shown_screens(&host), dialog()));
    }
    drop(mounted);
    host.remove();
    if dialog() {
        wrong.push("окно осталось на странице после окна программы".to_owned());
    }
    set_hash("");
    forget_trial();
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// За неделю до конца пробного периода — то же окно поверх программы
/// (задача 93): сколько дней и до какого дня, что запрёт конец срока и что
/// ключ приходит в течение рабочего дня после оплаты; программа под ним
/// неактивна. «Понятно» закрывает его, и больше оно не встаёт — запись одна
/// на устройство, — а «Купить ключ» закрывает и ведёт на экран лицензии.
/// С ключом дольше пробного периода окна о пробном нет.
#[wasm_bindgen_test]
async fn a_trial_ending_within_a_week_asks_once() {
    base_with_draft().await;
    forget_trial();
    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let today = clock::today().expect("часы с датой");
    // Последний день — через четыре дня: осталось пять, считая сегодня.
    let start = today.add_days(-9).expect("дата");
    trial_from(start);
    let last = license::trial_last(start).expect("дата");
    let words = fill(t(lang, Key::TrialSoonText), &[&days(lang, 5), &i18n::date(lang, last)]);
    let body = super::document().body().expect("есть <body>");
    let dialog = || body.query_selector("[role=dialog]").ok().flatten().is_some();
    let inert = |host: &HtmlElement| {
        host.query_selector(".app")
            .ok()
            .flatten()
            .is_some_and(|app| app.has_attribute("inert"))
    };
    let mut wrong = Vec::new();

    let (host, mounted) = mount_at("#/");
    if !wait_for(&body, &words).await {
        wrong.push("за пять дней до конца пробного периода окна нет".to_owned());
    }
    let card = body.text_content().unwrap_or_default();
    if !card.contains(t(lang, Key::TrialSoonTitle)) || card.contains(t(lang, Key::LicenseSoonTitle)) {
        wrong.push("у окна не заголовок пробного периода".to_owned());
    }
    if !inert(&host) {
        wrong.push("под окном программа активна".to_owned());
    }
    match button(&body, &[t(lang, Key::LicenseSoonOk)]) {
        Some(ok) => ok.click(),
        None => wrong.push("нет «Понятно»".to_owned()),
    }
    if !wait_until(|| !dialog() && !inert(&host)).await {
        wrong.push("«Понятно» не закрыло окно".to_owned());
    }
    if Device::new().trial_told() != Some(last) {
        wrong.push("устройство не запомнило, что о пробном периоде сказано".to_owned());
    }
    drop(mounted);
    host.remove();

    // Устройство забыло, каким нашла срок часть, и метка встанет, только когда
    // часть проверит его снова, — а с ней и решение об окне (ревью задачи 93:
    // по записи метка встала бы раньше части, и окно, вставшее позже, тест
    // не застал бы).
    forget_found_term();
    let (host, mounted) = mount_at("#/");
    let five = fill(t(lang, Key::LicenseChipTrial), &[&days(lang, 5)]);
    if let Err(seen) = wait_chip(&host, "nav", &five).await {
        wrong.push(format!("метка «{seen}», а не «{five}»"));
    }
    sleep(500).await;
    if dialog() {
        wrong.push("окно встало второй раз".to_owned());
    }
    drop(mounted);
    host.remove();

    if let Some(storage) = web_sys::window().and_then(|window| window.local_storage().ok().flatten()) {
        let _ = storage.remove_item("kvartik.trial_told");
    }
    let (host, mounted) = mount_at("#/");
    if !wait_for(&body, &words).await {
        wrong.push("окна нет и после того, как устройство забыло".to_owned());
    }
    match button(&body, &[t(lang, Key::LicenseBuy)]) {
        Some(buy) => buy.click(),
        None => wrong.push("нет «Купить ключ»".to_owned()),
    }
    if !shows(&host, "license").await || !wait_until(|| !dialog() && !inert(&host)).await {
        wrong.push(format!("«Купить ключ»: видны {:?}, окно — {}", shown_screens(&host), dialog()));
    }
    drop(mounted);
    host.remove();

    // Ключ на год дольше пробного периода: окна о пробном нет, хоть
    // устройство о нём и не помнит.
    if let Some(storage) = web_sys::window().and_then(|window| window.local_storage().ok().flatten()) {
        let _ = storage.remove_item("kvartik.trial_told");
    }
    store_key(today.add_days(364).expect("дата")).await;
    let (host, mounted) = mount_at("#/");
    let year = fill(t(lang, Key::LicenseChipKey), &[&days(lang, 365)]);
    if let Err(seen) = wait_chip(&host, "nav", &year).await {
        wrong.push(format!("с ключом метка «{seen}», а не «{year}»"));
    }
    sleep(500).await;
    if dialog() {
        wrong.push("с ключом на год — окно о конце пробного периода".to_owned());
    }
    drop(mounted);
    host.remove();
    set_hash("");
    forget_trial();
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// Ввести строку `text` в поле ключа и нажать «Ввести ключ».
fn enter_key(host: &HtmlElement, lang: Lang, text: &str) -> bool {
    match host.query_selector("#license textarea").ok().flatten() {
        Some(area) => {
            type_into(&area.unchecked_into::<HtmlInputElement>(), text);
            button(host, &[t(lang, Key::LicenseEnter)]).map(|button| button.click()).is_some()
        }
        None => false,
    }
}

/// Убрать за тестом конторы всё, что он оставил: метку устройства, базы ИП
/// списка и саму контору, — иначе следующие тесты открыли бы не ту базу.
async fn forget_office_bases() {
    if let Ok(office) = Store::open_office(|_| {}).await
        && let Ok(listed) = crate::store::office::list(&office).await
    {
        drop(office);
        for ip in listed.records.iter().filter(|ip| ip.base() != crate::store::DB_NAME) {
            crate::store::delete_database(ip.base()).await;
        }
    }
    crate::store::delete_database(crate::store::office::OFFICE_DB).await;
    super::office::forget_office();
}

/// Ключ «Pro» (задача 38) — на ՀՎՀՀ бухгалтера, не профиля, — ложится не
/// в базу ИП, а в контору: её ещё нет, и она заводится с открытым ИП в списке,
/// а устройство помнит, что ведёт контору. Окно говорит о местах, а не
/// «ключ на другой ИНН», срок — «Pro», выпуск снова открыт, а в списке
/// введённых у ключа — число мест. Повтор — «уже введён».
#[wasm_bindgen_test]
async fn a_pro_key_goes_to_the_office() {
    base_with_draft().await;
    forget_office_bases().await;
    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let today = clock::today().expect("часы с датой");
    trial_from(today.add_days(-100).expect("дата"));
    let until = today.add_days(365).expect("дата");

    let (host, mounted) = mount_at("#/license");
    let mut wrong = Vec::new();
    if !wait_for(&host, t(lang, Key::LicenseClosedNow)).await {
        wrong.push("экран лицензии не открылся запертым".to_owned());
    }
    let pro = pro_text("07654321", until, 3);
    let entered = fill(t(lang, Key::LicenseProEntered), &["3", t(lang, Key::OfficeTitle)]);
    if !enter_key(&host, lang, &pro) || !wait_for(&host, &entered).await {
        wrong.push(format!("ключ Pro не введён: «{}»", license_text(&host)));
    }
    if !wait_for(&host, &plan_until(lang, Key::PlanPro, until)).await {
        wrong.push("срок — не ключа Pro".to_owned());
    }
    let screen = license_text(&host);
    let mismatch = fill(t(lang, Key::LicenseOtherTin), &["07654321", TIN]);
    for (what, found) in [
        ("«ключ на другой ИНН»", screen.contains(&mismatch)),
        ("«не тот ИНН»", screen.contains(t(lang, Key::LicenseNotProfileTin))),
        ("«сейчас закрыто»", screen.contains(t(lang, Key::LicenseClosedNow))),
    ] {
        if found {
            wrong.push(format!("у ключа Pro — {what}"));
        }
    }
    for fact in [fill(t(lang, Key::LicenseSeats), &["3"]), t(lang, Key::LicenseForOffice).to_owned()] {
        if !screen.contains(&fact) {
            wrong.push(format!("у введённого ключа Pro нет «{fact}»"));
        }
    }
    if !enter_key(&host, lang, &pro) || !wait_for(&host, t(lang, Key::LicenseKnown)).await {
        wrong.push("повтор ключа Pro не сказан".to_owned());
    }
    // Ключ «Pro» без ИНН — подарок на пробу (`license::NO_TIN`): у него
    // «без ИНН», а не номер из нулей, и мест — сколько в нём.
    let gift = pro_text(license::NO_TIN, until, 100);
    let gift_entered = fill(t(lang, Key::LicenseProEntered), &["100", t(lang, Key::OfficeTitle)]);
    if !enter_key(&host, lang, &gift) || !wait_for(&host, &gift_entered).await {
        wrong.push(format!("ключ Pro без ИНН не введён: «{}»", license_text(&host)));
    }
    let screen = license_text(&host);
    let zeros = fill(t(lang, Key::TinShort), &[license::NO_TIN]);
    if !screen.contains(t(lang, Key::LicenseKeyNoTin)) || screen.contains(&zeros) {
        wrong.push(format!("у ключа без ИНН нет «{}» или есть «{zeros}»", t(lang, Key::LicenseKeyNoTin)));
    }
    if !screen.contains(&fill(t(lang, Key::LicenseSeats), &["100"])) {
        wrong.push("у ключа без ИНН нет «мест: 100»".to_owned());
    }

    let office = Store::open_office(|_| {}).await.expect("контора открыта");
    let ips = crate::store::office::list(&office).await.expect("список читается");
    let keys = crate::store::office::keys(&office).await.expect("ключи читаются");
    drop(office);
    let bases: Vec<&str> = ips.records.iter().map(Ip::base).collect();
    // Два ключа: на ИНН бухгалтера и подарочный без ИНН.
    if keys.records.len() != 2 || bases != [crate::store::DB_NAME] {
        wrong.push(format!("в конторе ключей {}, а ИП {bases:?}", keys.records.len()));
    }
    if !Device::new().office() {
        wrong.push("устройство не помнит контору".to_owned());
    }
    let store = Store::open(|_| {}).await.expect("база открыта");
    let in_base = store.read().expect("чтение").list::<License>().await.expect("ключи").records.len();
    drop(store);
    if in_base != 0 {
        wrong.push(format!("ключ Pro лёг в базу ИП: записей ключей {in_base}"));
    }

    if !open_draft(&host).await {
        wrong.push("в списке нет черновика".to_owned());
    }
    if !wait_for(&host, t(lang, Key::DocIssueHint)).await || button(&host, &[t(lang, Key::DocIssue)]).is_none() {
        wrong.push("с ключом Pro «Выпустить» не вернулось".to_owned());
    }

    drop(mounted);
    host.remove();
    set_hash("");
    forget_trial();
    forget_office_bases().await;
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// Ключ «Pro» открывает первых ИП списка по дню добавления: на одно место
/// у открытого ИП, добавленного позже другого, места нет — экран лицензии
/// так и говорит, выпуск заперт, а экран ИП называет его «без места».
/// Убрали из списка того, кто раньше, — место освободилось, и срок — «Pro».
#[wasm_bindgen_test]
async fn a_pro_key_opens_only_its_seats() {
    base_with_draft().await;
    forget_office_bases().await;
    let lang = Lang::Ru;
    let device = Device::new();
    device.set_ui_lang(lang);
    let (today, now) = (clock::today().expect("часы с датой"), clock::now());
    trial_from(today.add_days(-100).expect("дата"));
    let until = today.add_days(365).expect("дата");
    let earlier = Ip::new(today.add_days(-2).expect("дата"), now);
    let open = Ip::first(None, today, now);
    let key = license::Key::read(&pro_text("07654321", until, 1), &signer().verifying_key().to_bytes())
        .expect("ключ пары проб");
    let pro = OfficeLicense::enter(key, &[], now).expect("ключ Pro");
    let office = Store::open_office(|_| {}).await.expect("контора открыта");
    let mut writes = Writes::default();
    writes.insert(&earlier);
    writes.insert(&open);
    writes.insert(&pro);
    office.write().expect("транзакция").commit(writes).await.expect("записано");
    drop(office);
    device.set_office();
    device.set_base(crate::store::DB_NAME);

    let (host, mounted) = mount_at("#/license");
    let mut wrong = Vec::new();
    let no_seat = t(lang, Key::LicenseProNoSeat).to_owned();
    if !wait_for(&host, &no_seat).await {
        wrong.push(format!("нет слов, что места нет: «{}»", license_text(&host)));
    }
    if !license_text(&host).contains(t(lang, Key::LicenseClosedNow)) {
        wrong.push("без места выпуск не заперт".to_owned());
    }

    set_hash("#/office");
    let nameless = t(lang, Key::OfficeNameless);
    if !wait_for(&host, t(lang, Key::OfficeNoSeat)).await {
        wrong.push("экран ИП не назвал открытого «без места»".to_owned());
    }
    let pro_line = fill(t(lang, Key::OfficePro), &[&i18n::date(lang, until), "1"]);
    if !super::section_text(&host, "office").contains(&pro_line) {
        wrong.push(format!("на экране ИП нет «{pro_line}»"));
    }
    // Пока идёт ключ «Pro», «ключ у каждого ИП свой» на экране ИП неправда.
    if super::section_text(&host, "office").contains(t(lang, Key::OfficeKeyEach)) {
        wrong.push("под ключом Pro — «ключ у каждого ИП свой»".to_owned());
    }
    match super::office::wait_row_button(&host, nameless, t(lang, Key::OfficeRemove)).await {
        Some(remove) => remove.click(),
        None => wrong.push("у раннего ИП нет «Убрать из списка»".to_owned()),
    }
    let freed = wait_until(|| !super::section_text(&host, "office").contains(t(lang, Key::OfficeNoSeat))).await;
    if !freed {
        wrong.push("убранный ранний ИП не освободил место".to_owned());
    }

    set_hash("#/license");
    if !wait_for(&host, &plan_until(lang, Key::PlanPro, until)).await {
        wrong.push("с освободившимся местом срок — не «Pro»".to_owned());
    }
    if license_text(&host).contains(&no_seat) {
        wrong.push("место освободилось, а слова о нём остались".to_owned());
    }

    drop(mounted);
    host.remove();
    set_hash("");
    forget_trial();
    forget_office_bases().await;
    crate::store::delete_database(earlier.base()).await;
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// ИП со своим действующим ключом «Стандарт» места в ключе «Pro» не занимает
/// (решение человека 2026-11-17, задача 103): ключ — на одно место, у ИП,
/// добавленного раньше, свой ключ в его базе, и место достаётся открытому,
/// добавленному позже. Экран лицензии о месте молчит и выпуск не запирает,
/// срок — «Pro», а экран ИП называет раннего «свой ключ», и «без места» нет
/// ни у кого.
#[wasm_bindgen_test]
async fn an_own_key_leaves_its_seat_to_the_next() {
    base_with_draft().await;
    forget_office_bases().await;
    let lang = Lang::Ru;
    let device = Device::new();
    device.set_ui_lang(lang);
    let (today, now) = (clock::today().expect("часы с датой"), clock::now());
    trial_from(today.add_days(-100).expect("дата"));
    let until = today.add_days(365).expect("дата");
    // Ранний ИП — со своим профилем на свой ՀՎՀՀ и своим ключом в своей базе.
    let own_tin = "07654321";
    let mut data = profile();
    data.tin = Some(Tin::parse(own_tin).expect("годный ՀՎՀՀ"));
    let own_profile = Profile::save(None, data, now).expect("годный профиль").expect("новый");
    let mut earlier = Ip::new(today.add_days(-2).expect("дата"), now);
    earlier.copy_profile(&own_profile, now);
    let key = license::Key::read(&key_text(SEED, own_tin, until), &signer().verifying_key().to_bytes())
        .expect("ключ пары проб");
    let own_key = License::enter(key, &[], now).expect("новый ключ");
    let store = Store::open_base(earlier.base(), |_| {}).await.expect("база раннего ИП");
    let mut writes = Writes::default();
    writes.insert(&own_profile);
    writes.insert(&own_key);
    store.write().expect("транзакция").commit(writes).await.expect("записано");
    drop(store);
    let open = Ip::first(None, today, now);
    office_with_pro(&[&earlier, &open], &pro_text("09876541", until, 1)).await;
    device.set_base(crate::store::DB_NAME);

    let (host, mounted) = mount_at("#/license");
    let mut wrong = Vec::new();
    if !wait_for(&host, &plan_until(lang, Key::PlanPro, until)).await {
        wrong.push(format!("открытому ИП места не досталось: «{}»", license_text(&host)));
    }
    let screen = license_text(&host);
    for (what, words) in [
        ("«места не хватило»", t(lang, Key::LicenseProNoSeat)),
        ("«сейчас закрыто»", t(lang, Key::LicenseClosedNow)),
    ] {
        if screen.contains(words) {
            wrong.push(format!("на экране лицензии — {what}"));
        }
    }

    set_hash("#/office");
    let own_name = own_profile.data().name.ru.as_str().to_owned();
    let row_says = |text: &str| {
        let section = super::section_text(&host, "office");
        section.contains(&format!("{own_name}{}", fill(t(lang, Key::TinShort), &[own_tin]))) && section.contains(text)
    };
    if !wait_until(|| row_says(t(lang, Key::OfficeOwnKey))).await {
        wrong.push(format!("экран ИП не назвал раннего «свой ключ»: «{}»", super::section_text(&host, "office")));
    }
    if super::section_text(&host, "office").contains(t(lang, Key::OfficeNoSeat)) {
        wrong.push("на экране ИП — «без места»".to_owned());
    }

    drop(mounted);
    host.remove();
    set_hash("");
    forget_trial();
    forget_office_bases().await;
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// Записать в контору список и ключ «Pro» в обход окна — так их оставила
/// бы другая вкладка — и метку устройства.
async fn office_with_pro(ips: &[&Ip], key: &str) {
    let key = license::Key::read(key, &signer().verifying_key().to_bytes()).expect("ключ пары проб");
    let pro = OfficeLicense::enter(key, &[], clock::now()).expect("ключ Pro");
    let office = Store::open_office(|_| {}).await.expect("контора открыта");
    let mut writes = Writes::default();
    for ip in ips {
        writes.insert(*ip);
    }
    writes.insert(&pro);
    office.write().expect("транзакция").commit(writes).await.expect("записано");
    Device::new().set_office();
}

/// Ключ «Pro» ввели в другой вкладке, а эта контору не читала: тот же ключ
/// здесь — «уже введён», и окно перечитывает контору, а не остаётся запертым
/// до перезагрузки (ревью задачи 38).
#[wasm_bindgen_test]
async fn a_known_pro_key_brings_the_office_in() {
    base_with_draft().await;
    forget_office_bases().await;
    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let today = clock::today().expect("часы с датой");
    trial_from(today.add_days(-100).expect("дата"));
    let until = today.add_days(365).expect("дата");
    let (host, mounted) = mount_at("#/license");
    let mut wrong = Vec::new();
    if !wait_for(&host, t(lang, Key::LicenseClosedNow)).await {
        wrong.push("экран лицензии не открылся запертым".to_owned());
    }
    let pro = pro_text("07654321", until, 3);
    office_with_pro(&[&Ip::first(None, today, clock::now())], &pro).await;
    if !enter_key(&host, lang, &pro) || !wait_for(&host, t(lang, Key::LicenseKnown)).await {
        wrong.push("тот же ключ Pro — не «уже введён»".to_owned());
    }
    if !wait_for(&host, &plan_until(lang, Key::PlanPro, until)).await {
        wrong.push(format!("после «уже введён» срок — не «Pro»: «{}»", license_text(&host)));
    }
    drop(mounted);
    host.remove();
    set_hash("");
    forget_trial();
    forget_office_bases().await;
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// Сводка конторы (задача 38, этап 5) открыта и после конца пробного периода,
/// пока идёт ключ «Pro» (решение человека 2026-11-05): у обоих ИП списка — свои
/// строки, а слов «открыта в пробный период и с ключом» нет. У второго ИП база
/// пуста — «данных пока нет», а не налог в 0 ֏.
#[wasm_bindgen_test]
async fn the_summary_opens_with_a_pro_key() {
    base_with_draft().await;
    forget_office_bases().await;
    let lang = Lang::Ru;
    let device = Device::new();
    device.set_ui_lang(lang);
    let (today, now) = (clock::today().expect("часы с датой"), clock::now());
    trial_from(today.add_days(-100).expect("дата"));
    let until = today.add_days(365).expect("дата");
    let other = Ip::new(today, now);
    office_with_pro(&[&Ip::first(None, today, now), &other], &pro_text("07654321", until, 30)).await;
    device.set_base(crate::store::DB_NAME);
    let (host, mounted) = mount_at("#/office");
    let mut wrong = Vec::new();
    let rows = || {
        host.query_selector_all("#office .office-summary > li")
            .map(|rows| rows.length())
            .unwrap_or(0)
    };
    if !wait_until(|| rows() == 2).await {
        wrong.push(format!("в сводке не два ИП: {}", super::section_text(&host, "office")));
    }
    if super::section_text(&host, "office").contains(t(lang, Key::OfficeSummaryLocked)) {
        wrong.push("под ключом «Pro» — «сводка в пробный период и с ключом»".to_owned());
    }
    if !super::section_text(&host, "office").contains(t(lang, Key::OfficeSummaryEmpty)) {
        wrong.push(format!("у ИП с пустой базой — не «данных пока нет»: {}", super::section_text(&host, "office")));
    }
    drop(mounted);
    host.remove();
    set_hash("");
    crate::store::delete_database(other.base()).await;
    forget_trial();
    forget_office_bases().await;
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// Пустая база ИП с местом в ключе «Pro», запуск после конца пробного
/// периода: срок — ключа «Pro», и окно остаётся на главном, а не уводит на
/// экран лицензии с «пробный период кончился» (ревью задачи 38: срок пустой
/// базы считался пробным периодом, пока его не пересчитает часть).
#[wasm_bindgen_test]
async fn an_empty_base_with_a_pro_seat_opens_home() {
    crate::store::delete_database(crate::store::DB_NAME).await;
    forget_office_bases().await;
    let lang = Lang::Ru;
    let device = Device::new();
    device.set_ui_lang(lang);
    let today = clock::today().expect("часы с датой");
    trial_from(today.add_days(-100).expect("дата"));
    let until = today.add_days(365).expect("дата");
    office_with_pro(&[&Ip::first(None, today, clock::now())], &pro_text("07654321", until, 1)).await;
    device.set_base(crate::store::DB_NAME);
    let (host, mounted) = launch_at("#/");
    let mut wrong = Vec::new();
    let chip = fill(t(lang, Key::LicenseChipKey), &[&days(lang, until.days_since(today) + 1)]);
    if let Err(seen) = wait_chip(&host, "nav", &chip).await {
        wrong.push(format!("метка «{seen}», а не «{chip}»"));
    }
    if shown_screens(&host) != ["home"] {
        wrong.push(format!("запуск увёл с главного: {:?}", shown_screens(&host)));
    }
    drop(mounted);
    host.remove();
    set_hash("");
    forget_trial();
    forget_office_bases().await;
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// У ИП уже `MAX_KEYS` записей ключей — новый ключ не вводится, а окно
/// говорит словами, сколько их и что больше не бывает (ревью задачи 38).
#[wasm_bindgen_test]
async fn a_key_over_the_limit_is_refused() {
    base_with_draft().await;
    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let today = clock::today().expect("часы с датой");
    trial_from(today.add_days(-100).expect("дата"));
    let mut writes = Writes::default();
    for n in 0..license::MAX_KEYS {
        let until = today.add_days(i32::try_from(n).expect("число") + 1).expect("дата");
        let tin = Tin::parse("07654321").expect("годный ՀՎՀՀ");
        let key = license::Key::signed(tin, Plan::Standard, today, until, None, [0; 64]).expect("ключ");
        writes.insert(&License::enter(key, &[], clock::now()).expect("ключ"));
    }
    commit(writes).await;
    let (host, mounted) = mount_at("#/license");
    let mut wrong = Vec::new();
    if !wait_for(&host, t(lang, Key::LicenseClosedNow)).await {
        wrong.push("экран лицензии не открылся".to_owned());
    }
    let full = fill(t(lang, Key::LicenseKeysFull), &[&i18n::integer(lang, license::MAX_KEYS as i64)]);
    let good = key_text(SEED, TIN, today.add_days(365).expect("дата"));
    if !enter_key(&host, lang, &good) || !wait_for(&host, &full).await {
        wrong.push(format!("нет «{full}»: «{}»", license_text(&host)));
    }
    if license_text(&host).contains(t(lang, Key::LicenseEntered)) {
        wrong.push("ключ сверх предела введён".to_owned());
    }
    drop(mounted);
    host.remove();
    set_hash("");
    forget_trial();
    assert!(wrong.is_empty(), "{wrong:#?}");
}
