//! Экран «Другое устройство» (задача 110): код с одного «устройства» на
//! другое в одном браузере. Устройство — это его записи `localStorage`
//! (идентификатор, пароль копий) и база программы: тест стирает их между
//! «отправителем» и «получателем». Камеру headless-браузер не даёт, поэтому
//! код идёт текстом; QR-коды и сканер кадров проверяют тесты ядра
//! (`format::qr`, `format::sync`).

use js_sys::Reflect;
use kvartik_core::model::tin::Tin;
use wasm_bindgen::JsCast;
use web_sys::{HtmlElement, HtmlInputElement};

use super::*;
use crate::platform::clock;

/// Пароль копий обоих «устройств» — как у человека, у которого он один.
const PASSWORD: &str = "синхронизация двух устройств 110";

/// Забыть записи устройства, которые решают передачу: другое устройство —
/// другой идентификатор и, пока не принял код, без пароля копий.
fn other_device() {
    if let Some(storage) = web_sys::window().and_then(|window| window.local_storage().ok().flatten()) {
        for name in [
            "backup_key",
            "sync_id",
            "sync_made",
            "sync_received",
            "sync_arrived",
            "last_backup",
            "password_entered",
            "issue_elsewhere",
            "issue_at",
        ] {
            let _ = storage.remove_item(&format!("kvartik.{name}"));
        }
    }
}

/// Элемент экрана «Другое устройство» по селектору — дождаться его.
async fn find(host: &HtmlElement, selector: &str) -> Option<HtmlElement> {
    for _ in 0..100 {
        if let Some(found) = host.query_selector(&format!("#sync {selector}")).ok().flatten() {
            return Some(found.unchecked_into());
        }
        sleep(50).await;
    }
    None
}

/// Поле пароля внутри подписи `label` на экране.
fn password_field(host: &HtmlElement, label: &str) -> Option<HtmlInputElement> {
    let labels = host.query_selector_all("#sync label").ok()?;
    (0..labels.length())
        .filter_map(|at| labels.item(at))
        .map(|node| node.unchecked_into::<Element>())
        .find(|element| element.first_child().and_then(|text| text.text_content()).is_some_and(|text| text.trim() == label))?
        .query_selector("input[type=password]")
        .ok()
        .flatten()
        .map(JsCast::unchecked_into)
}

/// Кнопка экрана с подписью `label`.
fn sync_button(host: &HtmlElement, label: &str) -> Option<HtmlElement> {
    let buttons = host.query_selector_all("#sync button").ok()?;
    (0..buttons.length())
        .filter_map(|at| buttons.item(at))
        .map(|node| node.unchecked_into::<HtmlElement>())
        .find(|button| button.text_content().unwrap_or_default().trim() == label)
}

/// Сделать код открытой базы — всех записей (`all`) или изменений с сегодня:
/// у первого кода день по умолчанию — все записи, а поле дня — сегодня.
/// Новый пароль копий с повтором, «Сделать код» — и текст кода. В базе —
/// профиль и клиент, изменённые сегодня. `None` — не вышло.
async fn make(host: &HtmlElement, lang: Lang, all: bool, wrong: &mut Vec<String>) -> Option<String> {
    crate::platform::set_hash(&Screen::Sync.href());
    if find(host, "input[type=radio][name=sync-what]").await.is_none() {
        wrong.push("нет выбора, что передать".to_owned());
        return None;
    }
    let radio = host
        .query_selector_all("#sync input[type=radio][name=sync-what]")
        .ok()
        .and_then(|radios| radios.item(u32::from(all)))
        .map(JsCast::unchecked_into::<HtmlElement>);
    match radio {
        Some(radio) => radio.click(),
        None => {
            wrong.push("нет варианта, что передать".to_owned());
            return None;
        }
    }
    for (nth, label) in [Key::BackupNewPassword, Key::BackupRepeatPassword].into_iter().enumerate() {
        match password_field(host, t(lang, label)) {
            Some(input) => type_into(&input, PASSWORD),
            None => wrong.push(format!("нет поля пароля {nth}")),
        }
    }
    match sync_button(host, t(lang, Key::SyncMake)) {
        Some(make) => make.click(),
        None => wrong.push("нет «Сделать код»".to_owned()),
    }
    let today = clock::today().expect("часы с датой");
    let ready = if all {
        fill(t(lang, Key::SyncMadeAll), &["2"])
    } else {
        fill(t(lang, Key::SyncMadeSince), &[&kvartik_core::i18n::date(lang, today), "2"])
    };
    if !wait_for(host, &ready).await {
        wrong.push(format!("код не готов: «{}»", section_text(host, "sync")));
        return None;
    }
    if host.query_selector("#sync .sync-qr path").ok().flatten().and_then(|path| path.get_attribute("d")).is_none_or(|d| d.is_empty()) {
        wrong.push("нет QR-кода".to_owned());
    }
    let text = find(host, ".sync-text textarea").await?;
    Reflect::get(&text, &"value".into()).ok()?.as_string()
}

/// Вставить код текстом и дождаться поля пароля.
async fn paste(host: &HtmlElement, lang: Lang, code: &str, wrong: &mut Vec<String>) -> Option<HtmlInputElement> {
    crate::platform::set_hash(&Screen::Sync.href());
    let Some(area) = find(host, ".sync-paste textarea").await else {
        wrong.push("нет поля для текста кода".to_owned());
        return None;
    };
    // С подписью вокруг, как из мессенджера.
    let pasted = format!("Kvartik — мой код:\n{code}\nконец");
    Reflect::set(&area, &"value".into(), &pasted.into()).expect("значение поставлено");
    area.dispatch_event(&web_sys::Event::new("input").expect("событие")).expect("отправлено");
    for _ in 0..100 {
        if let Some(input) = password_field(host, t(lang, Key::SyncPassword)) {
            return Some(input);
        }
        sleep(50).await;
    }
    wrong.push(format!("код не собрался: «{}»", section_text(host, "sync")));
    None
}

/// Ввести пароль и нажать «Принять».
fn accept(host: &HtmlElement, lang: Lang, input: &HtmlInputElement, password: &str) {
    type_into(input, password);
    if let Some(accept) = sync_button(host, t(lang, Key::SyncAccept)) {
        accept.click();
    }
}

/// Код всех записей с одного устройства ложится в пустую программу другого:
/// текст с подписью вокруг собирается, неверный пароль просит пароль снова,
/// верный кладёт профиль и клиента, другое устройство берёт себе пароль копий
/// и спрашивает, где выпускать. А свой код на своём же устройстве не
/// принимается.
#[wasm_bindgen_test]
async fn a_code_carries_all_records_to_another_device() {
    let client = documents::fresh_base_with_client().await;
    let lang = Lang::Ru;
    other_device();
    Device::new().set_ui_lang(lang);
    let mut wrong = Vec::new();

    // Отправитель.
    let (host, mounted) = mount();
    if !opened(&host, lang).await {
        wrong.push("база не открылась".to_owned());
    }
    let code = make(&host, lang, true, &mut wrong).await.unwrap_or_default();
    if !code.starts_with(kvartik_core::format::sync::TEXT_PREFIX) {
        wrong.push(format!("текст кода без метки: {}", code.chars().take(20).collect::<String>()));
    }
    let sender = Device::new().sync_id();
    if Device::new().backup_key().is_none() {
        wrong.push("новый пароль копий не запомнился".to_owned());
    }
    // Свой код у себя — не принимается.
    if let Some(input) = paste(&host, lang, &code, &mut wrong).await {
        accept(&host, lang, &input, PASSWORD);
        if !wait_for(&host, t(lang, Key::SyncOwn)).await {
            wrong.push(format!("свой код принят: «{}»", section_text(&host, "sync")));
        }
    }
    drop(mounted);
    host.remove();

    // Получатель: пустая база, своё устройство.
    crate::store::delete_database(crate::store::DB_NAME).await;
    other_device();
    let (host, mounted) = mount();
    if !opened(&host, lang).await {
        wrong.push("база получателя не открылась".to_owned());
    }
    if let Some(input) = paste(&host, lang, &code, &mut wrong).await {
        accept(&host, lang, &input, "не тот пароль совсем не тот");
        if !wait_for(&host, t(lang, Key::SyncWrongPassword)).await {
            wrong.push("неверный пароль не назван".to_owned());
        }
        match password_field(&host, t(lang, Key::SyncPassword)) {
            Some(input) => accept(&host, lang, &input, PASSWORD),
            None => wrong.push("после неверного пароля нет поля пароля".to_owned()),
        }
        let done = fill(t(lang, Key::BackupRestored), &["2", "0", "0"]);
        if !wait_for(&host, &done).await {
            wrong.push(format!("код не лёг: «{}»", section_text(&host, "sync")));
        }
        if !section_text(&host, "sync").contains(t(lang, Key::BackupIssuerAsk)) {
            wrong.push("пустая программа не спросила, где выпускать".to_owned());
        }
    }
    match stored_clients().await {
        Ok(clients) if clients.iter().any(|known| known.id() == client.id()) => {}
        other => wrong.push(format!("клиента нет в базе получателя: {other:?}")),
    }
    let device = Device::new();
    if device.backup_key().is_none() {
        wrong.push("получатель не взял пароль кода".to_owned());
    }
    if device.sync_received(crate::store::DB_NAME).until(sender).is_none() {
        wrong.push("получатель не запомнил, что принял от отправителя".to_owned());
    }
    drop(mounted);
    host.remove();
    other_device();
    crate::platform::set_hash("");
    documents::forget_device();
    assert!(wrong.is_empty(), "передача кодом: {wrong:#?}");
}

/// База ИП с ИНН: профиль и клиент, записанные командами ядра, — как у
/// человека, который завёл программу на устройстве сам.
async fn base_with_tin() -> Client {
    crate::store::delete_database(crate::store::DB_NAME).await;
    let now = clock::now();
    let mut data = documents::profile();
    data.tin = Some(Tin::parse("07654321").expect("годный ИНН"));
    let profile = Profile::save(None, data, now).expect("годный профиль").expect("новый");
    let client = Client::create(documents::acme(), now).expect("годный клиент");
    let mut writes = Writes::default();
    writes.insert(&profile);
    writes.insert(&client);
    documents::commit(writes).await;
    client
}

/// Изменения «с дня» ложатся, только если здесь данные того устройства — его
/// запись профиля. Тот же ИП с тем же ИНН, заведённый на втором устройстве
/// своим профилем, кода с изменениями не берёт и не пишет ничего: остальное
/// туда не придёт никогда, а ряд выглядел бы целым (ревью задачи 110).
#[wasm_bindgen_test]
async fn changes_land_only_where_the_senders_data_is() {
    let theirs = base_with_tin().await;
    let lang = Lang::Ru;
    other_device();
    Device::new().set_ui_lang(lang);
    let mut wrong = Vec::new();

    let (host, mounted) = mount();
    if !opened(&host, lang).await {
        wrong.push("база не открылась".to_owned());
    }
    let code = make(&host, lang, false, &mut wrong).await.unwrap_or_default();
    drop(mounted);
    host.remove();

    // Второе устройство: тот же ИП, свой профиль и свой клиент.
    let ours = base_with_tin().await;
    other_device();
    let (host, mounted) = mount();
    if !opened(&host, lang).await {
        wrong.push("база получателя не открылась".to_owned());
    }
    if let Some(input) = paste(&host, lang, &code, &mut wrong).await {
        accept(&host, lang, &input, PASSWORD);
        let today = clock::today().expect("часы с датой");
        let refused = fill(t(lang, Key::SyncEmptyDelta), &[&kvartik_core::i18n::date(lang, today)]);
        if !wait_for(&host, &refused).await {
            wrong.push(format!("код с изменениями лёг в чужие данные: «{}»", section_text(&host, "sync")));
        }
    }
    match stored_clients().await {
        Ok(clients) if clients.iter().all(|known| known.id() == ours.id()) && clients.len() == 1 => {}
        other => wrong.push(format!("в базе не только свой клиент ({:?}): {other:?}", theirs.id())),
    }
    drop(mounted);
    host.remove();
    other_device();
    crate::platform::set_hash("");
    documents::forget_device();
    assert!(wrong.is_empty(), "изменения без данных отправителя: {wrong:#?}");
}
