//! Экран «Другое устройство» (задача 110): код с одного «устройства» на
//! другое в одном браузере. Устройство — это его записи `localStorage`
//! (идентификатор, пароль копий) и база программы: тест стирает их между
//! «отправителем» и «получателем». Камеру headless-браузер не даёт, поэтому
//! код идёт текстом; QR-коды и сканер кадров проверяют тесты ядра
//! (`format::qr`, `format::sync`).

use js_sys::Reflect;
use kvartik_core::model::record::RecordId;
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
            "sync_last",
            "carry_closed",
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

/// Выбрать путь плиткой (задача 124): `send` — «Передать отсюда», иначе
/// «Принять сюда». В пустой программе плиток нет: «Принять сюда» уже на
/// экране, а передавать нечего.
fn choose_way(host: &HtmlElement, send: bool) {
    let tile = host
        .query_selector_all("#sync input[type=radio][name=sync-way]")
        .ok()
        .and_then(|tiles| tiles.item(u32::from(!send)))
        .map(JsCast::unchecked_into::<HtmlElement>);
    if let Some(tile) = tile {
        tile.click();
    }
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
    choose_way(host, true);
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
    let Some(area) = find(host, ".sync-paste:not(.sync-reply) textarea").await else {
        wrong.push("нет поля для текста кода".to_owned());
        return None;
    };
    choose_way(host, false);
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
        // Пароль показан, а отправлен — снова точками (ревью задачи 124):
        // текстовое поле браузер запомнил бы как обычный ввод.
        match host.query_selector("#sync form.sync label.choice input[type=checkbox]").ok().flatten() {
            Some(show) => show.unchecked_into::<HtmlElement>().click(),
            None => wrong.push("у пароля приёма нет «Показать пароль»".to_owned()),
        }
        for _ in 0..20 {
            if input.type_() == "text" {
                break;
            }
            sleep(10).await;
        }
        if input.type_() != "text" {
            wrong.push(format!("«Показать пароль» не показал: {}", input.type_()));
        }
        accept(&host, lang, &input, "не тот пароль совсем не тот");
        if !wait_for(&host, t(lang, Key::SyncWrongPassword)).await {
            wrong.push("неверный пароль не назван".to_owned());
        }
        let field = host
            .query_selector(r#"#sync form.sync input[autocomplete="current-password"]"#)
            .ok()
            .flatten()
            .map(JsCast::unchecked_into::<HtmlInputElement>);
        if field.as_ref().is_none_or(|field| field.type_() != "password") {
            wrong.push(format!("после отправки пароль виден: {:?}", field.map(|field| field.type_())));
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

/// Шапка кода из его текста и ключ кода — из пароля и замка кода: так их
/// получит принявшее устройство.
async fn opened_code(code: &str) -> Option<(kvartik_core::format::sync::Header, crate::platform::crypto::Key)> {
    use kvartik_core::format::sync;
    let mut collector = sync::Collector::default();
    for frame in sync::frames_in_text(code) {
        collector.take(frame.ok()?);
    }
    let sealed = sync::open(&collector.whole()?).ok()?;
    let lock = sealed.lock();
    let key = crate::platform::crypto::derive(&crate::platform::crypto::normalize(PASSWORD), lock.salt(), lock.iterations())
        .await
        .ok()?;
    let plain = crate::platform::crypto::decrypt(&key, lock.iv(), sealed.ciphertext()).await.ok()?;
    Some((sync::read(&sealed, &plain).ok()?.header().clone(), key))
}

/// Квитанция `receipt` — текстом, под ключом `key`, как её покажет принявшее.
async fn receipt_text(key: &crate::platform::crypto::Key, receipt: &kvartik_core::format::sync::Receipt) -> String {
    use kvartik_core::format::sync;
    let iv = crate::platform::crypto::random(kvartik_core::format::backup::IV_BYTES).expect("вектор");
    let ciphertext = crate::platform::crypto::encrypt(key, &iv, &receipt.plain()).await.expect("шифр");
    sync::text_parts(&sync::seal_receipt(&iv, &ciphertext), 1).join("\n")
}

/// Вставить квитанцию текстом под показанным кодом.
async fn paste_reply(host: &HtmlElement, text: &str, wrong: &mut Vec<String>) {
    let Some(area) = find(host, ".sync-reply textarea").await else {
        wrong.push("нет поля для квитанции".to_owned());
        return;
    };
    Reflect::set(&area, &"value".into(), &text.into()).expect("значение поставлено");
    area.dispatch_event(&web_sys::Event::new("input").expect("событие")).expect("отправлено");
}

/// Подпись под QR-кодом на экране — какой он из скольких; `None` — код один,
/// и круга нет.
fn qr_caption(host: &HtmlElement) -> Option<String> {
    host.query_selector("#sync .sync-qr figcaption").ok().flatten()?.text_content()
}

/// Идёт ли круг QR-кодов: подпись сменилась за две секунды — четыре шага
/// круга (`QR_STEP`).
async fn circling(host: &HtmlElement) -> bool {
    let first = qr_caption(host);
    for _ in 0..40 {
        sleep(50).await;
        if qr_caption(host) != first {
            return true;
        }
    }
    false
}

/// Квитанция (задача 116): принявшее устройство показывает её под ключом
/// кода — на код этого отправителя, с целым рядом; отправитель, вставив
/// квитанцию с дырой, слышит день и находит его в поле и в дне следующего
/// кода, а квитанцию чужого кода и сам код вместо квитанции называет словами.
/// Квитанция, что код принят, останавливает круг QR-кодов — и после возврата
/// на экран, — а «Готово, убрать код» становится главной кнопкой; квитанция
/// чужого кода круга не трогает (задача 123).
#[wasm_bindgen_test]
async fn a_receipt_tells_the_sender_what_arrived() {
    use kvartik_core::format::sync::{self, Continuity, Receipt};
    documents::fresh_base_with_client().await;
    let lang = Lang::Ru;
    other_device();
    Device::new().set_ui_lang(lang);
    let mut wrong = Vec::new();

    // Отправитель: код всех записей, а потом — квитанции под ним.
    let (host, mounted) = mount();
    if !opened(&host, lang).await {
        wrong.push("база не открылась".to_owned());
    }
    let code = make(&host, lang, true, &mut wrong).await.unwrap_or_default();
    let sender = Device::new().sync_id();
    match opened_code(&code).await {
        Some((header, key)) => {
            let got = kvartik_core::model::record::Timestamp::from_unix_millis(header.created().unix_millis() - 3 * 86_400_000);
            let day = clock::local_date(got).expect("день");
            let other = Receipt::new(
                &sync::Header::new("0.0.172", kvartik_core::model::record::Timestamp::from_unix_millis(header.created().unix_millis() + 1), sender, None),
                RecordId::random(),
                Continuity::Whole,
            );
            if !circling(&host).await {
                wrong.push(format!("до квитанции круг QR-кодов не идёт: {:?}", qr_caption(&host)));
            }
            paste_reply(&host, &receipt_text(&key, &other).await, &mut wrong).await;
            if !wait_for(&host, t(lang, Key::SyncReceiptOther)).await {
                wrong.push(format!("квитанция чужого кода не названа: «{}»", section_text(&host, "sync")));
            }
            if !circling(&host).await {
                wrong.push("квитанция чужого кода остановила круг".to_owned());
            }
            paste_reply(&host, &code, &mut wrong).await;
            if !wait_for(&host, t(lang, Key::SyncIsCode)).await {
                wrong.push("код вместо квитанции не назван".to_owned());
            }
            // «Всё дошло» — круг стоит, и после возврата на экран тоже, а убрать
            // код — главной кнопкой.
            let whole = Receipt::new(&header, RecordId::random(), Continuity::Whole);
            paste_reply(&host, &receipt_text(&key, &whole).await, &mut wrong).await;
            if !wait_for(&host, t(lang, Key::SyncReceiptWhole)).await {
                wrong.push("целый ряд не назван".to_owned());
            }
            if circling(&host).await {
                wrong.push("квитанция «всё дошло» — а круг QR-кодов идёт".to_owned());
            }
            match button(&host, &[t(lang, Key::SyncClose)]) {
                Some(close) if !close.class_list().contains("link") => {}
                Some(_) => wrong.push("после квитанции «Готово, убрать код» — не главная кнопка".to_owned()),
                None => wrong.push("нет «Готово, убрать код»".to_owned()),
            }
            crate::platform::set_hash(&Screen::Home.href());
            for _ in 0..100 {
                if shown_screens(&host) == [Screen::Home.id()] {
                    break;
                }
                sleep(10).await;
            }
            crate::platform::set_hash(&Screen::Sync.href());
            for _ in 0..100 {
                if shown_screens(&host) == [Screen::Sync.id()] {
                    break;
                }
                sleep(10).await;
            }
            if circling(&host).await {
                wrong.push("после возврата на экран круг QR-кодов пошёл снова".to_owned());
            }
            let gap = Receipt::new(&header, RecordId::random(), Continuity::Gap { got });
            paste_reply(&host, &receipt_text(&key, &gap).await, &mut wrong).await;
            let words = fill(t(lang, Key::SyncReceiptGap), &[&kvartik_core::i18n::date(lang, day)]);
            if !wait_for(&host, &words).await {
                wrong.push(format!("дыра не названа: «{}»", section_text(&host, "sync")));
            }
            let field = host
                .query_selector("#sync form.sync input[type=date]")
                .ok()
                .flatten()
                .and_then(|input| Reflect::get(&input, &"value".into()).ok())
                .and_then(|value| value.as_string());
            if field.as_deref() != Some(day.iso().as_str()) {
                wrong.push(format!("в поле не день дыры: {field:?}"));
            }
            if Device::new().sync_made(crate::store::DB_NAME) != Some(got) {
                wrong.push("день следующего кода не сдвинулся".to_owned());
            }
            // Впервые — не «цел» (ревью задачи 116): что там было раньше,
            // принявший не знает.
            let first = Receipt::new(&header, RecordId::random(), Continuity::First);
            paste_reply(&host, &receipt_text(&key, &first).await, &mut wrong).await;
            let words = fill(t(lang, Key::SyncReceiptFirst), &[""]);
            if !wait_for(&host, &words).await {
                wrong.push(format!("«впервые» не названо: «{}»", section_text(&host, "sync")));
            }
        }
        None => wrong.push("код не открылся".to_owned()),
    }
    drop(mounted);
    host.remove();

    // Получатель: пустая база — принимает код и показывает квитанцию.
    crate::store::delete_database(crate::store::DB_NAME).await;
    other_device();
    let (host, mounted) = mount();
    if !opened(&host, lang).await {
        wrong.push("база получателя не открылась".to_owned());
    }
    if let Some(input) = paste(&host, lang, &code, &mut wrong).await {
        accept(&host, lang, &input, PASSWORD);
        if !wait_for(&host, t(lang, Key::SyncReceiptTitle)).await {
            wrong.push(format!("квитанции нет: «{}»", section_text(&host, "sync")));
        }
        let shown = find(&host, ".sync-receipt textarea")
            .await
            .and_then(|area| Reflect::get(&area, &"value".into()).ok())
            .and_then(|value| value.as_string())
            .unwrap_or_default();
        let read = async {
            let (header, key) = opened_code(&code).await?;
            let mut collector = sync::Collector::default();
            for frame in sync::frames_in_text(&shown) {
                collector.take(frame.ok()?);
            }
            let sealed = sync::open_receipt(&collector.whole()?).ok()?;
            let plain = crate::platform::crypto::decrypt(&key, sealed.iv(), sealed.ciphertext()).await.ok()?;
            Some(sync::acked(&Receipt::read(&plain).ok()?, sender, header.created()))
        };
        if read.await != Some(sync::Acked::Whole) {
            wrong.push(format!("квитанция получателя не та: {}", shown.chars().take(30).collect::<String>()));
        }
        // Квитанция вместо кода — словами, а не «это не код».
        crate::platform::set_hash(&Screen::Sync.href());
        if let Some(area) = find(&host, ".sync-paste:not(.sync-reply) textarea").await {
            Reflect::set(&area, &"value".into(), &shown.into()).expect("значение поставлено");
            area.dispatch_event(&web_sys::Event::new("input").expect("событие")).expect("отправлено");
            if !wait_for(&host, t(lang, Key::SyncIsReceipt)).await {
                wrong.push("квитанция вместо кода не названа".to_owned());
            }
        }
    }
    drop(mounted);
    host.remove();
    other_device();
    crate::platform::set_hash("");
    documents::forget_device();
    assert!(wrong.is_empty(), "квитанция: {wrong:#?}");
}

/// Сделать код на экране, где форма уже может знать пароль: выбрать, что
/// передать (`all` — все записи), вписать пароль во все его поля и дождаться
/// текста кода; потом убрать код с экрана. `None` — не вышло.
async fn make_any(host: &HtmlElement, lang: Lang, all: bool, wrong: &mut Vec<String>) -> Option<String> {
    crate::platform::set_hash(&Screen::Sync.href());
    // Прежний код убран — дождаться, пока его нет на экране: перерисовка
    // приходит своей задачей (CLAUDE.md, «браузерный тест ждёт состояния»).
    for _ in 0..100 {
        if host.query_selector("#sync .sync-text textarea").ok().flatten().is_none() {
            break;
        }
        sleep(50).await;
    }
    let radio = find(host, "input[type=radio][name=sync-what]").await;
    if radio.is_none() {
        wrong.push("нет выбора, что передать".to_owned());
        return None;
    }
    choose_way(host, true);
    let radios = host.query_selector_all("#sync input[type=radio][name=sync-what]").ok()?;
    radios.item(u32::from(all)).map(JsCast::unchecked_into::<HtmlElement>)?.click();
    let fields = host.query_selector_all("#sync form.sync input[type=password]").ok()?;
    for at in 0..fields.length() {
        if let Some(input) = fields.item(at) {
            type_into(&input.unchecked_into(), PASSWORD);
        }
    }
    sync_button(host, t(lang, Key::SyncMake))?.click();
    for _ in 0..200 {
        let text = host
            .query_selector("#sync .sync-text textarea")
            .ok()
            .flatten()
            .and_then(|area| Reflect::get(&area, &"value".into()).ok())
            .and_then(|value| value.as_string())
            .filter(|text| !text.is_empty());
        if let Some(text) = text {
            sync_button(host, t(lang, Key::SyncClose))?.click();
            return Some(text);
        }
        sleep(50).await;
    }
    wrong.push(format!("код не готов: «{}»", section_text(host, "sync")));
    None
}

/// Код всех ИП (задача 116): у кого ИП несколько, код по умолчанию — всех,
/// и в пустой программе другого устройства он заводит оба ИП со списком,
/// как копия всех ИП. А код всех ИП «с дня» в программу, где одного из них
/// нет, не пишет ничего и называет его: остального его сюда не придёт.
#[wasm_bindgen_test]
async fn an_office_code_carries_every_ie() {
    use super::office::{forget_office, named, seed, seed_office, stored_office};
    use crate::store::office::OFFICE_DB;
    use kvartik_core::model::office::Ip;
    let lang = Lang::Ru;
    let aram = named("ИП Арам Петросян");
    let anna = named("ИП Анна Акопян");
    let today = clock::today().expect("сегодня");
    let first = {
        let mut ip = Ip::first(Some(&aram), today, clock::now());
        ip.copy_profile(&aram, clock::now());
        ip
    };
    let mut second = Ip::new(today, clock::now());
    second.copy_profile(&anna, clock::now());
    let fresh = || async {
        crate::store::delete_database(crate::store::DB_NAME).await;
        crate::store::delete_database(OFFICE_DB).await;
        crate::store::delete_database(second.base()).await;
        forget_office();
        other_device();
        Device::new().set_ui_lang(lang);
    };
    fresh().await;
    seed(crate::store::DB_NAME, &aram).await;
    seed(second.base(), &anna).await;
    // Клиент второго ИП — заведён позавчера: код «с сегодня» несёт его, потому
    // что сам ИП добавлен сегодня (шапка `format::sync`, «Код конторы»).
    let old_client = Client::create(
        documents::acme(),
        kvartik_core::model::record::Timestamp::from_unix_millis(clock::now().unix_millis() - 2 * 86_400_000),
    )
    .expect("годный клиент");
    {
        let store = crate::store::Store::open_base(second.base(), |_| {}).await.expect("база открыта");
        let mut writes = Writes::default();
        writes.insert(&old_client);
        store.write().expect("транзакция").commit(writes).await.expect("записано");
    }
    seed_office(&[&first, &second]).await;
    Device::new().set_office();
    crate::store::office::set_tab_base(crate::store::DB_NAME);
    let mut wrong = Vec::new();

    // Отправитель: контора двух ИП — код всех записей и код «с сегодня».
    let (host, mounted) = mount();
    if !opened(&host, lang).await {
        wrong.push("база не открылась".to_owned());
    }
    crate::platform::set_hash(&Screen::Sync.href());
    if !wait_for(&host, t(lang, Key::SyncWhose)).await {
        wrong.push("нет выбора, чьи изменения".to_owned());
    }
    let all = make_any(&host, lang, true, &mut wrong).await.unwrap_or_default();
    let delta = make_any(&host, lang, false, &mut wrong).await.unwrap_or_default();
    drop(mounted);
    host.remove();

    // Получатель — пустая программа: оба ИП со списком.
    fresh().await;
    let (host, mounted) = mount();
    if !opened(&host, lang).await {
        wrong.push("база получателя не открылась".to_owned());
    }
    if let Some(input) = paste(&host, lang, &all, &mut wrong).await {
        accept(&host, lang, &input, PASSWORD);
        if !wait_for(&host, t(lang, Key::SyncReceiptTitle)).await {
            wrong.push(format!("код всех ИП не лёг: «{}»", section_text(&host, "sync")));
        }
    }
    let listed = stored_office().await;
    let mut names: Vec<String> = listed.iter().map(|ip| ip.name(lang).to_owned()).collect();
    names.sort();
    if names != ["ИП Анна Акопян", "ИП Арам Петросян"] {
        wrong.push(format!("в списке не оба ИП: {names:?}"));
    }
    if !Device::new().office() {
        wrong.push("устройство не стало конторой".to_owned());
    }
    drop(mounted);
    host.remove();

    // Получатель, у которого только второй ИП: код «с сегодня» его не пишет.
    fresh().await;
    seed(crate::store::DB_NAME, &anna).await;
    let (host, mounted) = mount();
    if !opened(&host, lang).await {
        wrong.push("база второго получателя не открылась".to_owned());
    }
    if let Some(input) = paste(&host, lang, &delta, &mut wrong).await {
        accept(&host, lang, &input, PASSWORD);
        let refused = fill(t(lang, Key::SyncMissingIp), &["ИП Арам Петросян"]);
        if !wait_for(&host, &refused).await {
            wrong.push(format!("ИП, которого здесь нет, не назван: «{}»", section_text(&host, "sync")));
        }
    }
    if !stored_office().await.is_empty() {
        wrong.push("отказ записал список ИП".to_owned());
    }
    drop(mounted);
    host.remove();

    // Получатель, у которого только первый ИП: второй — новый, и код «с
    // сегодня» кладёт его всеми записями, с позавчерашним клиентом.
    fresh().await;
    seed(crate::store::DB_NAME, &aram).await;
    let (host, mounted) = mount();
    if !opened(&host, lang).await {
        wrong.push("база третьего получателя не открылась".to_owned());
    }
    if let Some(input) = paste(&host, lang, &delta, &mut wrong).await {
        accept(&host, lang, &input, PASSWORD);
        if !wait_for(&host, t(lang, Key::SyncReceiptTitle)).await {
            wrong.push(format!("код с новым ИП не лёг: «{}»", section_text(&host, "sync")));
        }
    }
    let landed = match crate::store::Store::open_base(second.base(), |_| {}).await {
        Ok(store) => store.read().expect("чтение").all::<Client>().await.unwrap_or_default(),
        Err(_) => Vec::new(),
    };
    if !landed.iter().any(|client| client.id() == old_client.id()) {
        wrong.push("новый ИП пришёл без давних записей".to_owned());
    }
    drop(mounted);
    host.remove();
    fresh().await;
    crate::platform::set_hash("");
    documents::forget_device();
    assert!(wrong.is_empty(), "код всех ИП: {wrong:#?}");
}

/// Код файлов (задача 116): файл книги в код изменений не идёт, а следом
/// уходит кодом файлов — частями, каждая своим кругом и своим текстом.
/// Получатель, принявший код изменений паролем, собирает части в любом
/// порядке и кладёт файл без второго пароля.
#[wasm_bindgen_test]
async fn files_follow_as_a_series_of_codes() {
    use kvartik_core::model::attachment::{Attachment, FileBytes};
    documents::fresh_base_with_client().await;
    let lang = Lang::Ru;
    other_device();
    Device::new().set_ui_lang(lang);
    // Файл, который не сжимается: 70 КБ — три части серии.
    let mut seed = 11u64;
    let mut bytes = b"%PDF-1.4 ".to_vec();
    bytes.extend((0..70_000).map(|_| {
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        (seed >> 56) as u8
    }));
    let file = Attachment::create(
        kvartik_core::model::record::RecordId::random(),
        "договор.pdf",
        FileBytes::new(bytes).expect("годный PDF"),
        clock::now(),
    );
    let mut writes = Writes::default();
    writes.insert(&file);
    documents::commit(writes).await;
    let mut wrong = Vec::new();

    // Отправитель: код изменений, потом — код файлов, часть за частью.
    let (host, mounted) = mount();
    if !opened(&host, lang).await {
        wrong.push("база не открылась".to_owned());
    }
    let code = make(&host, lang, true, &mut wrong).await.unwrap_or_default();
    // Квитанция «принято» — код файлов следующей главной кнопкой, а «Готово»
    // — ссылкой (задача 124, п. 18).
    match opened_code(&code).await {
        Some((header, key)) => {
            let whole = kvartik_core::format::sync::Receipt::new(&header, RecordId::random(), kvartik_core::format::sync::Continuity::Whole);
            paste_reply(&host, &receipt_text(&key, &whole).await, &mut wrong).await;
            let after = fill(t(lang, Key::SyncFilesAfter), &["1"]);
            if !wait_for(&host, &after).await {
                wrong.push(format!("после квитанции нет «{after}»"));
            }
            match sync_button(&host, t(lang, Key::SyncFilesMake)) {
                Some(files) if !files.class_list().contains("secondary") => {}
                Some(_) => wrong.push("после квитанции код файлов — не главная кнопка".to_owned()),
                None => wrong.push("после квитанции нет кода файлов".to_owned()),
            }
            if sync_button(&host, t(lang, Key::SyncClose)).is_some_and(|close| !close.class_list().contains("link")) {
                wrong.push("файлы ждут, а «Готово, убрать код» — главная".to_owned());
            }
        }
        None => wrong.push("код файлов: код изменений не открылся".to_owned()),
    }
    match sync_button(&host, t(lang, Key::SyncFilesMake)) {
        Some(button) => button.click(),
        None => wrong.push("нет «Передать файлы серией кодов»".to_owned()),
    }
    let first = fill(t(lang, Key::SyncFilesPart), &["1", "3"]);
    if !wait_for(&host, &first).await {
        wrong.push(format!("кода файлов нет: «{}»", section_text(&host, "sync")));
    }
    let mut parts = Vec::new();
    for at in 1..=3 {
        let heading = fill(t(lang, Key::SyncFilesPart), &[&at.to_string(), "3"]);
        if !wait_for(&host, &heading).await {
            wrong.push(format!("нет части {at}"));
            break;
        }
        let text = host
            .query_selector("#sync .sync-text textarea")
            .ok()
            .flatten()
            .and_then(|area| Reflect::get(&area, &"value".into()).ok())
            .and_then(|value| value.as_string())
            .unwrap_or_default();
        parts.push(text);
        if at < 3 {
            match sync_button(&host, t(lang, Key::SyncFilesNext)) {
                Some(next) => next.click(),
                None => wrong.push(format!("нет «Следующий код»: «{}»", section_text(&host, "sync"))),
            }
        }
    }
    drop(mounted);
    host.remove();

    // Получатель: пустая база. Код файлов раньше кода изменений не ложится
    // (ревью задачи 116); потом код изменений паролем, части — в обратном
    // порядке, без пароля.
    crate::store::delete_database(crate::store::DB_NAME).await;
    other_device();
    let (host, mounted) = mount();
    if !opened(&host, lang).await {
        wrong.push("база получателя не открылась".to_owned());
    }
    if let Some(input) = paste(&host, lang, &parts.join("\n"), &mut wrong).await {
        accept(&host, lang, &input, PASSWORD);
        if !wait_for(&host, t(lang, Key::SyncFilesFirst)).await {
            wrong.push(format!("код файлов лёг раньше кода изменений: «{}»", section_text(&host, "sync")));
        }
    }
    if let Some(input) = paste(&host, lang, &code, &mut wrong).await {
        accept(&host, lang, &input, PASSWORD);
        if !wait_for(&host, t(lang, Key::SyncReceiptTitle)).await {
            wrong.push(format!("код изменений не лёг: «{}»", section_text(&host, "sync")));
        }
    }
    let area = find(&host, ".sync-paste:not(.sync-reply) textarea").await;
    for part in parts.iter().rev() {
        if let Some(area) = &area {
            Reflect::set(area, &"value".into(), &part.into()).expect("значение поставлено");
            area.dispatch_event(&web_sys::Event::new("input").expect("событие")).expect("отправлено");
        }
    }
    let landed = fill(t(lang, Key::BackupRestored), &["1", "0", "1"]);
    if !wait_for(&host, &landed).await {
        wrong.push(format!("файлы не легли: «{}»", section_text(&host, "sync")));
    }
    if password_field(&host, t(lang, Key::SyncPassword)).is_some() {
        wrong.push("код файлов спросил пароль снова".to_owned());
    }
    let stored = crate::store::Store::open(|_| {})
        .await
        .expect("база открыта")
        .read()
        .expect("чтение")
        .all::<Attachment>()
        .await
        .unwrap_or_default();
    if !stored.iter().any(|found| found.id() == file.id()) {
        wrong.push("файла нет в базе получателя".to_owned());
    }
    drop(mounted);
    host.remove();
    other_device();
    crate::platform::set_hash("");
    documents::forget_device();
    assert!(wrong.is_empty(), "код файлов: {wrong:#?}");
}

/// Виден ли элемент экрана по селектору: спрятанный путь остаётся в разметке.
fn visible(host: &HtmlElement, selector: &str) -> bool {
    host.query_selector(&format!("#sync {selector}"))
        .ok()
        .flatten()
        .is_some_and(|found| found.unchecked_into::<HtmlElement>().offset_width() > 0)
}

/// Сначала выбор (задача 124): в пустой программе плиток нет — «Принять
/// сюда» уже на экране, а «Передать отсюда» нет вовсе, и «Вставить» без
/// буфера просит вставить самому. С данными — две плитки и ни одного пути;
/// выбранная показывает свой путь и прячет другой. В форме передачи — логин
/// для менеджера паролей, что войдёт — строкой, своя подсказка пароля,
/// а «Показать пароль» делает поле текстовым. Готовый код называет число
/// QR-кодов и круг, круг стоит, пока выбрано «Принять сюда», а строка
/// профиля называет день последнего обмена — не когда код сделан, а когда
/// пришла квитанция, что его приняли (ревью задачи 124).
#[wasm_bindgen_test]
async fn the_screen_starts_with_a_choice() {
    let lang = Lang::Ru;
    let mut wrong = Vec::new();

    // Пустая программа: сразу «Принять сюда».
    crate::store::delete_database(crate::store::DB_NAME).await;
    other_device();
    Device::new().set_ui_lang(lang);
    let (host, mounted) = mount_at("#/sync");
    if !opened(&host, lang).await {
        wrong.push("пустая база не открылась".to_owned());
    }
    if find(&host, ".sync-paste:not(.sync-reply) textarea").await.is_none() || !visible(&host, ".sync-paste:not(.sync-reply) textarea") {
        wrong.push("в пустой программе «Принять сюда» не на экране".to_owned());
    }
    if host.query_selector("#sync input[name=sync-way]").ok().flatten().is_some() {
        wrong.push("в пустой программе — плитки выбора".to_owned());
    }
    if host.query_selector("#sync input[name=sync-what]").ok().flatten().is_some() {
        wrong.push("в пустой программе есть «Передать отсюда»".to_owned());
    }
    // Буфер headless-браузер тесту не отдаёт: нажатие не человека.
    match sync_button(&host, t(lang, Key::SyncPasteNow)) {
        Some(paste) => {
            paste.click();
            if !wait_for(&host, t(lang, Key::SyncPasteFailed)).await {
                wrong.push(format!("«Вставить» без буфера молчит: «{}»", section_text(&host, "sync")));
            }
        }
        None => wrong.push("у поля кода нет «Вставить»".to_owned()),
    }
    drop(mounted);
    host.remove();

    // С данными: две плитки, ни одного пути.
    documents::fresh_base_with_client().await;
    other_device();
    let (host, mounted) = mount_at("#/sync");
    if !opened(&host, lang).await {
        wrong.push("база не открылась".to_owned());
    }
    if find(&host, "input[name=sync-way]").await.is_none() {
        wrong.push("с данными нет плиток выбора".to_owned());
    }
    let tiles = host.query_selector_all("#sync input[name=sync-way]").map(|tiles| tiles.length()).unwrap_or(0);
    if tiles != 2 {
        wrong.push(format!("плиток {tiles}, а не две"));
    }
    if visible(&host, "form.sync") || visible(&host, ".sync-paste:not(.sync-reply) textarea") {
        wrong.push("до выбора на экране уже путь".to_owned());
    }
    choose_way(&host, true);
    sleep(50).await;
    if !visible(&host, "form.sync") || visible(&host, ".sync-paste:not(.sync-reply) textarea") {
        wrong.push("«Передать отсюда» не показало свой путь или не спрятало другой".to_owned());
    }
    // Логин для менеджера паролей — спрятанный, один на все языки.
    let login = host
        .query_selector("#sync form.sync input[autocomplete=username]")
        .ok()
        .flatten()
        .map(JsCast::unchecked_into::<HtmlInputElement>);
    match login {
        Some(login) if login.hidden() && login.value() == "Kvartik" => {}
        Some(login) => wrong.push(format!("логин: виден {}, «{}»", !login.hidden(), login.value())),
        None => wrong.push("в форме передачи нет логина".to_owned()),
    }
    let said = section_text(&host, "sync");
    if !said.contains(t(lang, Key::SyncWhatFirst)) {
        wrong.push(format!("первый код — не «{}»: «{said}»", t(lang, Key::SyncWhatFirst)));
    }
    let hint = fill(t(lang, Key::SyncPasswordHint), &["15"]);
    if !said.contains(&hint) {
        wrong.push("под паролем передачи нет своей подсказки".to_owned());
    }
    if said.contains(&fill(t(lang, Key::BackupPasswordHint), &["15"])) {
        wrong.push("под паролем передачи — подсказка копии".to_owned());
    }
    // «Показать пароль» — поле текстом, и обратно.
    let field = password_field(&host, t(lang, Key::BackupNewPassword));
    let show = host
        .query_selector("#sync form.sync label.choice input[type=checkbox]")
        .ok()
        .flatten()
        .map(JsCast::unchecked_into::<HtmlElement>);
    match (field, show) {
        (Some(field), Some(show)) => {
            show.click();
            sleep(50).await;
            if field.type_() != "text" {
                wrong.push(format!("«Показать пароль» не показал: {}", field.type_()));
            }
            show.click();
            sleep(50).await;
            if field.type_() != "password" {
                wrong.push(format!("«Показать пароль» не спрятал: {}", field.type_()));
            }
        }
        _ => wrong.push("нет поля пароля или «Показать пароль»".to_owned()),
    }
    // Готовый код — с числом QR-кодов и кругом.
    let code = make(&host, lang, true, &mut wrong).await;
    if code.is_some() {
        let round = t(lang, Key::SyncMadeQr).split("{}").next().unwrap_or_default();
        if qr_caption(&host).is_some() && !section_text(&host, "sync").contains(round) {
            wrong.push(format!("у кода нет «{round}»"));
        }
        if !circling(&host).await {
            wrong.push("круг QR-кодов не идёт".to_owned());
        }
        // «Отправить» — там, где меню «Поделиться» берёт текст (headless
        // Chrome — да, Firefox на компьютере — нет).
        let shares = crate::platform::share::can_share_text("Kvartik");
        if sync_button(&host, t(lang, Key::SyncShare)).is_some() != shares {
            wrong.push(format!("«Отправить код» есть: {}, а меню с текстом: {shares}", !shares));
        }
        // Выбрано «Принять сюда» — код спрятан, и круг стоит.
        choose_way(&host, false);
        sleep(50).await;
        if !visible(&host, ".sync-paste:not(.sync-reply) textarea") || visible(&host, ".sync-qr") {
            wrong.push("«Принять сюда» не сменило путь".to_owned());
        }
        if circling(&host).await {
            wrong.push("код спрятан, а круг QR-кодов идёт".to_owned());
        }
    }
    // Код сделан, но его никто не снял — обмена ещё не было (ревью задачи
    // 124): строка профиля прежняя.
    let profile_row = |host: &HtmlElement| {
        host.query_selector(&format!(r#"#profile a.entry[href="{}"]"#, Screen::Sync.href()))
            .ok()
            .flatten()
            .and_then(|row| row.text_content())
            .unwrap_or_default()
    };
    // Строку экран перечитывает, когда показан профиль, — дождаться экрана,
    // а не читать прежнюю строку.
    crate::platform::set_hash(&Screen::Profile.href());
    for _ in 0..100 {
        if shown_screens(&host) == [Screen::Profile.id()] {
            break;
        }
        sleep(10).await;
    }
    sleep(100).await;
    if !profile_row(&host).contains(t(lang, Key::SyncEntryAbout)) {
        wrong.push(format!("код никто не снял, а строка профиля: «{}»", profile_row(&host)));
    }
    // Квитанция «принято» — обмен был: строка называет день.
    if let Some((header, key)) = match &code {
        Some(code) => opened_code(code).await,
        None => None,
    } {
        crate::platform::set_hash(&Screen::Sync.href());
        choose_way(&host, true);
        let whole = kvartik_core::format::sync::Receipt::new(&header, RecordId::random(), kvartik_core::format::sync::Continuity::Whole);
        paste_reply(&host, &receipt_text(&key, &whole).await, &mut wrong).await;
        if !wait_for(&host, t(lang, Key::SyncReceiptWhole)).await {
            wrong.push("квитанция «всё дошло» не принята".to_owned());
        }
        crate::platform::set_hash(&Screen::Profile.href());
        let today = clock::today().expect("часы с датой");
        let last = fill(t(lang, Key::SyncEntryLast), &[&kvartik_core::i18n::date(lang, today)]);
        if !wait_for(&host, &last).await {
            wrong.push(format!("квитанция пришла, а строка профиля не сказала «{last}»: «{}»", profile_row(&host)));
        }
    }
    drop(mounted);
    host.remove();
    other_device();
    crate::platform::set_hash("");
    documents::forget_device();
    assert!(wrong.is_empty(), "выбор пути: {wrong:#?}");
}

/// База, у которой код всех записей больше `sync::MAX_CODE_BYTES`: профиль
/// и шестьдесят клиентов с адресами из случайных букв — их не сжать.
async fn big_base() {
    documents::fresh_base_with_client().await;
    let mut writes = crate::store::Writes::default();
    let letters = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut seed: u64 = 0x2545_F491_4F6C_DD1D;
    let now = clock::now();
    for _ in 0..60 {
        let address: String = (0..900)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                char::from(letters[(seed % letters.len() as u64) as usize])
            })
            .collect();
        let data = kvartik_core::model::client::ClientData {
            address: kvartik_core::model::text::Lines::new(&address).expect("адрес"),
            ..documents::acme()
        };
        writes.insert(&kvartik_core::model::client::Client::create(data, now).expect("годный клиент"));
    }
    documents::commit(writes).await;
}

/// Сделать код на экране переноса: новый пароль копий с повтором и «Сделать
/// код».
fn make_carry_code(host: &HtmlElement, lang: Lang, wrong: &mut Vec<String>) {
    for label in [Key::BackupNewPassword, Key::BackupRepeatPassword] {
        match password_field(host, t(lang, label)) {
            Some(input) => type_into(&input, PASSWORD),
            None => wrong.push(format!("перенос: нет поля {label:?}")),
        }
    }
    match sync_button(host, t(lang, Key::SyncMake)) {
        Some(make) => make.click(),
        None => wrong.push("перенос: нет «Сделать код»".to_owned()),
    }
}

/// Перенос из вкладки Safari в установленную программу (ревью задачи 124):
/// экран сразу открывает «Передать» со словами о переносе; у готового кода
/// одна главная кнопка — «Скопировать весь код», без кусков; сделанный код
/// убирает карточку переноса. А код больше предела не советует день позже —
/// старое осталось бы во вкладке, которую Safari сотрёт, — а ведёт на копию
/// файлом.
#[wasm_bindgen_test]
async fn carrying_from_a_tab_moves_everything() {
    let lang = Lang::Ru;
    let mut wrong = Vec::new();
    crate::ui::sync::CARRYING.with(|carrying| carrying.set(true));
    documents::fresh_base_with_client().await;
    other_device();
    Device::new().set_ui_lang(lang);
    let (host, mounted) = mount_at("#/sync");
    if !opened(&host, lang).await {
        wrong.push("база не открылась".to_owned());
    }
    for _ in 0..100 {
        if visible(&host, "form.sync") {
            break;
        }
        sleep(50).await;
    }
    if !visible(&host, "form.sync") {
        wrong.push("вкладка переноса не открыла «Передать отсюда»".to_owned());
    }
    if !section_text(&host, "sync").contains(t(lang, Key::SyncCarryNote)) {
        wrong.push("нет слов о переносе".to_owned());
    }
    make_carry_code(&host, lang, &mut wrong);
    if find(&host, ".sync-qr path").await.is_none() {
        wrong.push(format!("код переноса не готов: «{}»", section_text(&host, "sync")));
    }
    if sync_button(&host, t(lang, Key::SyncCopyAll)).is_none() {
        wrong.push("нет «Скопировать весь код»".to_owned());
    }
    let piece = t(lang, Key::SyncCopyPart).split("{}").next().unwrap_or_default().to_owned();
    let pieces = host
        .query_selector_all("#sync button")
        .map(|buttons| {
            (0..buttons.length())
                .filter_map(|at| buttons.item(at))
                .filter_map(|node| node.text_content())
                .filter(|text| text.trim() == t(lang, Key::SyncCopy) || text.trim().starts_with(&piece))
                .count()
        })
        .unwrap_or(0);
    if pieces > 0 {
        wrong.push(format!("при переносе — кнопки кусков: {pieces}"));
    }
    if !Device::new().carry_closed() {
        wrong.push("код переноса сделан, а карточка переноса не убрана".to_owned());
    }
    drop(mounted);
    host.remove();

    // Код больше предела — путь копией, а не день позже.
    big_base().await;
    other_device();
    let (host, mounted) = mount_at("#/sync");
    if !opened(&host, lang).await {
        wrong.push("большая база не открылась".to_owned());
    }
    for _ in 0..100 {
        if visible(&host, "form.sync") {
            break;
        }
        sleep(50).await;
    }
    make_carry_code(&host, lang, &mut wrong);
    let carry_words = t(lang, Key::SyncCarryTooLarge).split("{}").next().unwrap_or_default().to_owned();
    let day_words = t(lang, Key::SyncTooLarge).split("{}").next().unwrap_or_default().to_owned();
    let mut said = false;
    for _ in 0..300 {
        said = section_text(&host, "sync").contains(&carry_words);
        if said {
            break;
        }
        sleep(50).await;
    }
    if !said {
        wrong.push(format!("большой код при переносе — без пути копией: «{}»", section_text(&host, "sync")));
    }
    if section_text(&host, "sync").contains(&day_words) {
        wrong.push("при переносе — совет выбрать день позже".to_owned());
    }
    if !visible(&host, &format!(r#"a[href="{}"]"#, Screen::Backup.href())) {
        wrong.push("большой код при переносе — без ссылки на копию".to_owned());
    }
    drop(mounted);
    host.remove();
    crate::ui::sync::CARRYING.with(|carrying| carrying.set(false));
    other_device();
    crate::platform::set_hash("");
    documents::forget_device();
    assert!(wrong.is_empty(), "перенос из вкладки: {wrong:#?}");
}

/// Файлов больше, чем вмещает серия кодов (задача 124, п. 18): одна кнопка
/// «Отправить копией файлом» — и экран копии открывается с готовой ручной
/// копией, без второго ввода пароля: его только что ввели для кода.
#[wasm_bindgen_test]
async fn files_beyond_a_series_go_as_a_backup_file() {
    use kvartik_core::model::attachment::{Attachment, FileBytes};
    documents::fresh_base_with_client().await;
    let lang = Lang::Ru;
    other_device();
    for name in ["last_backup", "last_size"] {
        if let Some(storage) = web_sys::window().and_then(|window| window.local_storage().ok().flatten()) {
            let _ = storage.remove_item(&format!("kvartik.{name}"));
        }
    }
    Device::new().set_ui_lang(lang);
    // Файл, который не сжимается и больше серии: 1,2 МБ.
    let mut seed = 23u64;
    let mut bytes = b"%PDF-1.4 ".to_vec();
    bytes.extend((0..1_200_000).map(|_| {
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        (seed >> 56) as u8
    }));
    let file = Attachment::create(RecordId::random(), "скан.pdf", FileBytes::new(bytes).expect("годный PDF"), clock::now());
    let mut writes = Writes::default();
    writes.insert(&file);
    documents::commit(writes).await;
    let mut wrong = Vec::new();
    let (host, mounted) = mount();
    if !opened(&host, lang).await {
        wrong.push("база не открылась".to_owned());
    }
    make(&host, lang, true, &mut wrong).await;
    match sync_button(&host, t(lang, Key::SyncFilesMake)) {
        Some(button) => button.click(),
        None => wrong.push("нет «Передать файлы серией кодов»".to_owned()),
    }
    let large = t(lang, Key::SyncFilesTooLarge).split("{}").next().unwrap_or_default().to_owned();
    if !wait_for(&host, &large).await {
        wrong.push(format!("нет «{large}»: «{}»", section_text(&host, "sync")));
    }
    match sync_button(&host, t(lang, Key::SyncFilesAsBackup)) {
        Some(button) => button.click(),
        None => wrong.push("нет «Отправить копией файлом»".to_owned()),
    }
    let ready = t(lang, Key::BackupReady).split("{}").next().unwrap_or_default().to_owned();
    let mut done = false;
    for _ in 0..300 {
        done = section_text(&host, "backup").contains(&ready) && host.query_selector("#backup a[download]").ok().flatten().is_some();
        if done {
            break;
        }
        sleep(50).await;
    }
    if !done {
        wrong.push(format!("экран копии без готовой копии: «{}»", section_text(&host, "backup")));
    }
    if crate::platform::hash() != Screen::Backup.href() {
        wrong.push(format!("открыт не экран копии: {}", crate::platform::hash()));
    }
    drop(mounted);
    host.remove();
    other_device();
    crate::platform::set_hash("");
    documents::forget_device();
    assert!(wrong.is_empty(), "файлы копией: {wrong:#?}");
}
