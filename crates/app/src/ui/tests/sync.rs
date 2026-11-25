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
    make_with(host, lang, all, &kinds(lang, 1, 0, 1), wrong).await
}

/// [`make`] с базой, где в коде — записи `words` по видам.
async fn make_with(host: &HtmlElement, lang: Lang, all: bool, words: &str, wrong: &mut Vec<String>) -> Option<String> {
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
    // Записи — словами по видам (задача 124, п. 21).
    let ready = if all {
        fill(t(lang, Key::SyncMadeAll), &[words])
    } else {
        fill(t(lang, Key::SyncMadeSince), &[&kvartik_core::i18n::date(lang, today), words])
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

/// Записи по видам словами (задача 124, п. 21) — из ключей таблицы строк,
/// а не функцией окна: иначе сломанная функция проверяла бы сама себя.
/// Порядок — как у окна: клиенты, файлы книги, прочее; пустые молчат.
fn kinds(lang: Lang, clients: usize, files: usize, other: usize) -> String {
    [(clients, Key::TallyClients), (files, Key::TallyFiles), (other, Key::TallyOther)]
        .into_iter()
        .filter(|(count, _)| *count > 0)
        .map(|(count, key)| fill(t(lang, key), &[&count.to_string()]))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Итог принятого кода: легло — словами `kinds`, уже было — `unchanged`.
fn landed_words(lang: Lang, kinds: &str, unchanged: usize) -> String {
    let mut words = fill(t(lang, Key::BackupRestored), &[kinds]);
    if unchanged > 0 {
        words.push(' ');
        words.push_str(&fill(t(lang, Key::BackupRestoredSame), &[&unchanged.to_string()]));
    }
    words
}

/// Начало шаблона `key`, заполненного `args`, — до следующего «{}»: слова
/// с числом, которого тест не считает (мегабайты оценкой), сверяются по
/// началу.
fn words_until(lang: Lang, key: Key, args: &[&str]) -> String {
    fill(t(lang, key), args).split("{}").next().unwrap_or_default().to_owned()
}

/// Подписи видимых главных кнопок экрана — без `secondary` и `link`.
fn main_buttons(host: &HtmlElement) -> Vec<String> {
    let Ok(found) = host.query_selector_all("#sync button") else {
        return Vec::new();
    };
    (0..found.length())
        .filter_map(|at| found.item(at))
        .map(|node| node.unchecked_into::<HtmlElement>())
        .filter(|button| {
            let classes = button.class_list();
            button.offset_width() > 0 && !classes.contains("secondary") && !classes.contains("link")
        })
        .map(|button| button.text_content().unwrap_or_default().trim().to_owned())
        .collect()
}

/// Записи кода из его текста — вид, идентификатор, текст записи и метка
/// базы (у кода всех ИП), — как их прочтёт принявшее устройство паролем
/// [`PASSWORD`].
async fn code_records(code: &str) -> Option<Vec<(&'static str, RecordId, String, Option<RecordId>)>> {
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
    Some(
        sync::read(&sealed, &plain)
            .ok()?
            .entries()
            .iter()
            .map(|entry| (entry.kind(), entry.meta().id(), entry.stored(), entry.base()))
            .collect(),
    )
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
        let done = landed_words(lang, &kinds(lang, 1, 0, 1), 0);
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
        // В пустую программу легли только записи того устройства — нести
        // обратно нечего (задача 124, п. 14).
        if sync_button(&host, t(lang, Key::SyncBackMake)).is_some() {
            wrong.push("в пустой программе — встречный код".to_owned());
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
            let after = words_until(lang, Key::SyncFilesAfter, &["1"]);
            if !wait_for(&host, &after).await {
                wrong.push(format!("после квитанции нет «{after}»"));
            }
            if main_buttons(&host) != [t(lang, Key::SyncFilesMake)] {
                wrong.push(format!("после квитанции главные кнопки — не одна серия: {:?}", main_buttons(&host)));
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
    let landed = landed_words(lang, &kinds(lang, 0, 1, 0), 1);
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
    let mut wrong = Vec::new();
    let lang = Lang::Ru;
    let (host, mounted, failed) = big_file_code(lang).await;
    wrong.extend(failed);
    let code = make(&host, lang, true, &mut wrong).await.unwrap_or_default();
    // Оценка (задача 124): файл больше серии — кнопки серии нет, а копия
    // файлом есть сразу; до квитанции она второстепенная.
    if sync_button(&host, t(lang, Key::SyncFilesMake)).is_some() {
        wrong.push("файл больше серии, а кнопка серии есть".to_owned());
    }
    let large = words_until(lang, Key::SyncSkippedLarge, &["1"]);
    if !section_text(&host, "sync").contains(&large) {
        wrong.push(format!("нет «{large}»: «{}»", section_text(&host, "sync")));
    }
    match sync_button(&host, t(lang, Key::SyncFilesAsBackup)) {
        Some(button) if button.class_list().contains("secondary") => {}
        Some(_) => wrong.push("до квитанции копия файлом — главная".to_owned()),
        None => wrong.push("нет «Отправить копией файлом» до квитанции".to_owned()),
    }
    // Квитанция «принято»: слова о файлах больше серии и одна главная
    // кнопка — копия файлом.
    match opened_code(&code).await {
        Some((header, key)) => {
            let whole = kvartik_core::format::sync::Receipt::new(&header, RecordId::random(), kvartik_core::format::sync::Continuity::Whole);
            paste_reply(&host, &receipt_text(&key, &whole).await, &mut wrong).await;
            let beyond = words_until(lang, Key::SyncFilesBeyond, &["1"]);
            if !wait_for(&host, &beyond).await {
                wrong.push(format!("после квитанции нет «{beyond}»: «{}»", section_text(&host, "sync")));
            }
            if main_buttons(&host) != [t(lang, Key::SyncFilesAsBackup)] {
                wrong.push(format!("после квитанции главные кнопки: {:?}", main_buttons(&host)));
            }
        }
        None => wrong.push("код не открылся".to_owned()),
    }
    match sync_button(&host, t(lang, Key::SyncFilesAsBackup)) {
        Some(button) => button.click(),
        None => wrong.push("нет «Отправить копией файлом»".to_owned()),
    }
    ready_backup(&host, &mut wrong).await;
    drop(mounted);
    host.remove();
    other_device();
    crate::platform::set_hash("");
    documents::forget_device();
    assert!(wrong.is_empty(), "файлы копией: {wrong:#?}");
}

/// База с файлом книги больше серии кодов (1,2 МБ, не сжимается), окно —
/// на русском; день и размер последней копии забыты. Не открылась база — это
/// третьим в ответе.
async fn big_file_code(lang: Lang) -> (HtmlElement, impl Sized, Option<String>) {
    use kvartik_core::model::attachment::{Attachment, FileBytes};
    documents::fresh_base_with_client().await;
    other_device();
    for name in ["last_backup", "last_size"] {
        if let Some(storage) = web_sys::window().and_then(|window| window.local_storage().ok().flatten()) {
            let _ = storage.remove_item(&format!("kvartik.{name}"));
        }
    }
    Device::new().set_ui_lang(lang);
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
    let (host, mounted) = mount();
    let failed = (!opened(&host, lang).await).then(|| "база не открылась".to_owned());
    (host, mounted, failed)
}

/// Экран копии открылся с готовой копией.
async fn ready_backup(host: &HtmlElement, wrong: &mut Vec<String>) {
    let ready = t(Lang::Ru, Key::BackupReady).split("{}").next().unwrap_or_default().to_owned();
    let mut done = false;
    for _ in 0..300 {
        done = section_text(host, "backup").contains(&ready) && host.query_selector("#backup a[download]").ok().flatten().is_some();
        if done {
            break;
        }
        sleep(50).await;
    }
    if !done {
        wrong.push(format!("экран копии без готовой копии: «{}»", section_text(host, "backup")));
    }
    if crate::platform::hash() != Screen::Backup.href() {
        wrong.push(format!("открыт не экран копии: {}", crate::platform::hash()));
    }
}

/// Серия отказала, хотя оценка обещала (размер разошёлся; оценку тест
/// подменяет): кнопки серии больше нет, слова — спокойные, не тревога, без
/// «передайте следом» и после квитанции, а главная кнопка одна — копия
/// файлом (задача 124).
#[wasm_bindgen_test]
async fn a_refused_series_leaves_one_way() {
    let mut wrong = Vec::new();
    let lang = Lang::Ru;
    crate::ui::sync::FITS.with(|fits| fits.set(Some(true)));
    let (host, mounted, failed) = big_file_code(lang).await;
    wrong.extend(failed);
    let code = make(&host, lang, true, &mut wrong).await.unwrap_or_default();
    match sync_button(&host, t(lang, Key::SyncFilesMake)) {
        Some(button) => button.click(),
        None => wrong.push("нет «Передать файлы серией кодов»".to_owned()),
    }
    let large = words_until(lang, Key::SyncFilesTooLarge, &[]);
    if !wait_for(&host, &large).await {
        wrong.push(format!("нет «{large}»: «{}»", section_text(&host, "sync")));
    }
    let said = host
        .query_selector_all("#sync p[role=alert]")
        .ok()
        .map(|found| (0..found.length()).filter_map(|at| found.item(at)).filter_map(|node| node.text_content()).collect::<Vec<_>>())
        .unwrap_or_default();
    if said.iter().any(|text| text.contains(&large)) {
        wrong.push("отказ серии — тревогой, а не спокойными словами".to_owned());
    }
    if sync_button(&host, t(lang, Key::SyncFilesMake)).is_some() {
        wrong.push("после отказа кнопка серии осталась".to_owned());
    }
    if main_buttons(&host) != [t(lang, Key::SyncFilesAsBackup)] {
        wrong.push(format!("после отказа главные кнопки: {:?}", main_buttons(&host)));
    }
    match opened_code(&code).await {
        Some((header, key)) => {
            let whole = kvartik_core::format::sync::Receipt::new(&header, RecordId::random(), kvartik_core::format::sync::Continuity::Whole);
            paste_reply(&host, &receipt_text(&key, &whole).await, &mut wrong).await;
            if !wait_for(&host, t(lang, Key::SyncReceiptWhole)).await {
                wrong.push("квитанция не принята".to_owned());
            }
            let after = words_until(lang, Key::SyncFilesAfter, &["1"]);
            if section_text(&host, "sync").contains(&after) {
                wrong.push("после отказа серии — «передайте следом»".to_owned());
            }
            if main_buttons(&host) != [t(lang, Key::SyncFilesAsBackup)] {
                wrong.push(format!("после квитанции главные кнопки: {:?}", main_buttons(&host)));
            }
        }
        None => wrong.push("код не открылся".to_owned()),
    }
    match sync_button(&host, t(lang, Key::SyncFilesAsBackup)) {
        Some(button) => button.click(),
        None => wrong.push("нет «Отправить копией файлом»".to_owned()),
    }
    ready_backup(&host, &mut wrong).await;
    crate::ui::sync::FITS.with(|fits| fits.set(None));
    drop(mounted);
    host.remove();
    other_device();
    crate::platform::set_hash("");
    documents::forget_device();
    assert!(wrong.is_empty(), "отказ серии: {wrong:#?}");
}

/// QR-коды на весь экран (задача 124, п. 17): кнопка под кодом открывает окно
/// с тем же кодом, словами о яркости и подписью круга; Escape и «Свернуть»
/// его закрывают, а фокус в окне — на кнопке. Tab из окна не уходит,
/// закрытое возвращает фокус на кнопку под кодом, а квитанция «принято»
/// закрывает его само (A4).
#[wasm_bindgen_test]
async fn the_qr_code_opens_full_screen() {
    documents::fresh_base_with_client().await;
    let lang = Lang::Ru;
    other_device();
    Device::new().set_ui_lang(lang);
    let mut wrong = Vec::new();
    let (host, mounted) = mount();
    if !opened(&host, lang).await {
        wrong.push("база не открылась".to_owned());
    }
    let code = make(&host, lang, true, &mut wrong).await.unwrap_or_default();
    let open = |host: &HtmlElement, wrong: &mut Vec<String>| match sync_button(host, t(lang, Key::SyncQrFull)) {
        Some(button) => button.click(),
        None => wrong.push("нет «QR-код на весь экран»".to_owned()),
    };
    open(&host, &mut wrong);
    let Some(full) = find(&host, ".sync-full").await else {
        wrong.push("окно QR-кода не открылось".to_owned());
        drop(mounted);
        host.remove();
        assert!(wrong.is_empty(), "QR на весь экран: {wrong:#?}");
        return;
    };
    let small = host.query_selector("#sync .sync-qr path").ok().flatten().and_then(|path| path.get_attribute("d"));
    let large = full.query_selector("path").ok().flatten().and_then(|path| path.get_attribute("d"));
    if large.is_none_or(|d| d.is_empty()) || small.is_none() {
        wrong.push("в окне нет QR-кода".to_owned());
    }
    if !full.text_content().unwrap_or_default().contains(t(lang, Key::SyncQrBright)) {
        wrong.push("в окне нет слов о яркости".to_owned());
    }
    sleep(50).await;
    let focused = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.active_element())
        .and_then(|element| element.text_content());
    if focused.as_deref().map(str::trim) != Some(t(lang, Key::SyncQrFullClose)) {
        wrong.push(format!("фокус в окне не на «Свернуть»: {focused:?}"));
    }
    let init = web_sys::KeyboardEventInit::new();
    init.set_key("Escape");
    init.set_bubbles(true);
    let escape = web_sys::KeyboardEvent::new_with_keyboard_event_init_dict("keydown", &init).expect("создаётся");
    full.dispatch_event(&escape).expect("отправлено");
    sleep(50).await;
    if host.query_selector("#sync .sync-full").ok().flatten().is_some() {
        wrong.push("Escape не закрыл окно QR-кода".to_owned());
    }
    open(&host, &mut wrong);
    if find(&host, ".sync-full").await.is_none() {
        wrong.push("окно QR-кода не открылось второй раз".to_owned());
    }
    // Tab не уводит фокус из окна (задача 124, A4): кнопка в нём одна.
    if let Some(full) = host.query_selector("#sync .sync-full").ok().flatten() {
        let init = web_sys::KeyboardEventInit::new();
        init.set_key("Tab");
        init.set_bubbles(true);
        init.set_cancelable(true);
        let tab = web_sys::KeyboardEvent::new_with_keyboard_event_init_dict("keydown", &init).expect("создаётся");
        full.dispatch_event(&tab).expect("отправлено");
        if !tab.default_prevented() {
            wrong.push("Tab в окне QR-кода не остановлен".to_owned());
        }
    }
    match sync_button(&host, t(lang, Key::SyncQrFullClose)) {
        Some(close) => close.click(),
        None => wrong.push("в окне нет «Свернуть»".to_owned()),
    }
    sleep(50).await;
    if host.query_selector("#sync .sync-full").ok().flatten().is_some() {
        wrong.push("«Свернуть» не закрыло окно QR-кода".to_owned());
    }
    let focused = || {
        web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.active_element())
            .and_then(|element| element.text_content())
    };
    let mut back = false;
    for _ in 0..40 {
        back = focused().as_deref().map(str::trim) == Some(t(lang, Key::SyncQrFull));
        if back {
            break;
        }
        sleep(25).await;
    }
    if !back {
        wrong.push(format!("закрытое окно не вернуло фокус на свою кнопку: {:?}", focused()));
    }
    // Квитанция «принято» закрывает открытое окно само.
    open(&host, &mut wrong);
    if find(&host, ".sync-full").await.is_none() {
        wrong.push("окно QR-кода не открылось третий раз".to_owned());
    }
    match opened_code(&code).await {
        Some((header, key)) => {
            let whole = kvartik_core::format::sync::Receipt::new(&header, RecordId::random(), kvartik_core::format::sync::Continuity::Whole);
            paste_reply(&host, &receipt_text(&key, &whole).await, &mut wrong).await;
            if !wait_for(&host, t(lang, Key::SyncReceiptWhole)).await {
                wrong.push("квитанция не принята".to_owned());
            }
            sleep(50).await;
            if host.query_selector("#sync .sync-full").ok().flatten().is_some() {
                wrong.push("квитанция «принято» не закрыла окно QR-кода".to_owned());
            }
        }
        None => wrong.push("код не открылся".to_owned()),
    }
    drop(mounted);
    host.remove();
    other_device();
    crate::platform::set_hash("");
    documents::forget_device();
    assert!(wrong.is_empty(), "QR на весь экран: {wrong:#?}");
}

/// Сколько блокировок экрана сейчас держит подменённый `navigator.wakeLock`.
fn wake_held() -> f64 {
    web_sys::window()
        .and_then(|window| Reflect::get(&window, &"kvartikWake".into()).ok())
        .and_then(|log| Reflect::get(&log, &"held".into()).ok())
        .and_then(|held| held.as_f64())
        .unwrap_or(-1.0)
}

/// Дождаться, пока блокировок станет `held`.
async fn wake_becomes(held: f64) -> bool {
    for _ in 0..40 {
        if wake_held() == held {
            return true;
        }
        sleep(50).await;
    }
    false
}

/// Экран не гаснет, пока идёт круг QR-кодов (задача 124, A3): блокировка
/// экрана берётся, когда код на экране и путь на виду, отпускается, когда
/// выбрано «Принять сюда», берётся снова на «Передать отсюда», отпускается
/// по пределу без квитанции и берётся снова на возвращённом пути,
/// а квитанция «принято» отпускает её насовсем. Настоящей блокировки headless-браузер не даёт —
/// `navigator.wakeLock` подменён счётчиком.
#[wasm_bindgen_test]
async fn the_screen_stays_on_while_codes_circle() {
    documents::fresh_base_with_client().await;
    let lang = Lang::Ru;
    other_device();
    Device::new().set_ui_lang(lang);
    let stub = js_sys::Function::new_no_args(
        "const log = { held: 0 };
         Object.defineProperty(navigator, 'wakeLock', { configurable: true, value: { request(type) {
             if (type !== 'screen') return Promise.reject(new TypeError('type'));
             log.held++;
             const sentinel = { released: false, release() {
                 if (!this.released) { this.released = true; log.held--; }
                 return Promise.resolve();
             } };
             return Promise.resolve(sentinel);
         } } });
         window.kvartikWake = log;",
    );
    stub.call0(&wasm_bindgen::JsValue::NULL).expect("подмена");
    let mut wrong = Vec::new();
    let (host, mounted) = mount();
    if !opened(&host, lang).await {
        wrong.push("база не открылась".to_owned());
    }
    let code = make(&host, lang, true, &mut wrong).await.unwrap_or_default();
    if !wake_becomes(1.0).await {
        wrong.push(format!("код на экране, а экран не держится: {}", wake_held()));
    }
    choose_way(&host, false);
    if !wake_becomes(0.0).await {
        wrong.push(format!("выбрано «Принять сюда», а экран держится: {}", wake_held()));
    }
    choose_way(&host, true);
    if !wake_becomes(1.0).await {
        wrong.push(format!("снова «Передать отсюда», а экран не держится: {}", wake_held()));
    }
    // Без квитанции — не дольше предела (ревью задачи 124): оставленный
    // телефон гаснет и запирается сам; вернулись на путь — снова держится.
    // Путь сменился — дождаться, пока экран отпущен: два переключения подряд
    // окно видит одним.
    let away_and_back = || async {
        choose_way(&host, false);
        let mut away = false;
        for _ in 0..40 {
            away = visible(&host, ".sync-paste:not(.sync-reply) textarea") && wake_held() == 0.0;
            if away {
                break;
            }
            sleep(50).await;
        }
        choose_way(&host, true);
        away
    };
    crate::ui::sync::AWAKE_LIMIT.with(|limit| limit.set(Some(std::time::Duration::from_millis(300))));
    if !away_and_back().await || !wake_becomes(1.0).await {
        wrong.push(format!("с коротким пределом экран не взят: {}", wake_held()));
    }
    if !wake_becomes(0.0).await {
        wrong.push(format!("предел вышел, а экран держится: {}", wake_held()));
    }
    if !away_and_back().await || !wake_becomes(1.0).await {
        wrong.push(format!("вернулись на путь, а экран не держится: {}", wake_held()));
    }
    crate::ui::sync::AWAKE_LIMIT.with(|limit| limit.set(None));
    if !away_and_back().await || !wake_becomes(1.0).await {
        wrong.push(format!("без короткого предела экран не взят: {}", wake_held()));
    }
    match opened_code(&code).await {
        Some((header, key)) => {
            let whole = kvartik_core::format::sync::Receipt::new(&header, RecordId::random(), kvartik_core::format::sync::Continuity::Whole);
            paste_reply(&host, &receipt_text(&key, &whole).await, &mut wrong).await;
            if !wait_for(&host, t(lang, Key::SyncReceiptWhole)).await {
                wrong.push("квитанция не принята".to_owned());
            }
            if !wake_becomes(0.0).await {
                wrong.push(format!("код принят, а экран держится: {}", wake_held()));
            }
        }
        None => wrong.push("код не открылся".to_owned()),
    }
    drop(mounted);
    host.remove();
    crate::ui::sync::AWAKE_LIMIT.with(|limit| limit.set(None));
    if let Some(window) = web_sys::window() {
        let _ = Reflect::delete_property(&window.navigator(), &"wakeLock".into());
        let _ = Reflect::delete_property(&window, &"kvartikWake".into());
    }
    other_device();
    crate::platform::set_hash("");
    documents::forget_device();
    assert!(wrong.is_empty(), "экран при круге QR-кодов: {wrong:#?}");
}

/// Записи этой программы на устройстве (`kvartik.*`) — чтобы «вернуться» на
/// устройство после другого.
fn device_records() -> Vec<(String, String)> {
    let Some(storage) = web_sys::window().and_then(|window| window.local_storage().ok().flatten()) else {
        return Vec::new();
    };
    (0..storage.length().unwrap_or(0))
        .filter_map(|at| storage.key(at).ok().flatten())
        .filter(|name| name.starts_with("kvartik."))
        .filter_map(|name| {
            let value = storage.get_item(&name).ok().flatten()?;
            Some((name, value))
        })
        .collect()
}

/// Вернуть устройству его записи `records`, а чужие убрать.
fn restore_device(records: &[(String, String)]) {
    let Some(storage) = web_sys::window().and_then(|window| window.local_storage().ok().flatten()) else {
        return;
    };
    for (name, _) in device_records() {
        let _ = storage.remove_item(&name);
    }
    for (name, value) in records {
        let _ = storage.set_item(name, value);
    }
}

/// База программы с профилем `profile` и клиентом `client` — с нуля.
async fn seed_pair(profile: &Profile, client: &Client) {
    crate::store::delete_database(crate::store::DB_NAME).await;
    let mut writes = Writes::default();
    writes.insert(profile);
    writes.insert(client);
    documents::commit(writes).await;
}

/// Обмен в обе стороны за один сеанс (задача 124, п. 14): принявшее
/// устройство под квитанцией делает встречный код тем же ключом — без
/// пароля, с дня по умолчанию до принятого кода (легшее им туда не
/// возвращается) и не сдвигая своего ряда изменений. Отправитель вставляет
/// его там же, где квитанцию: код ложится, квитанции на него нет, «Готово»
/// становится главной кнопкой, а код под чужим замком назван словами. В
/// пустой программе встречного кода нет — это держит тест квитанции. Правки
/// отправителя после того дня — сегодняшние — встречный код ему не
/// возвращает, а запись, которую получатель правил после них, — несёт
/// (решение человека 2026-11-25).
#[wasm_bindgen_test]
async fn changes_go_both_ways_in_one_session() {
    use kvartik_core::format::sync;
    use kvartik_core::model::client::ClientData;
    use kvartik_core::model::record::Timestamp;
    use kvartik_core::model::text::Line;
    const DAY: i64 = 86_400_000;
    let lang = Lang::Ru;
    let now = clock::now();
    let profile = Profile::save(None, documents::profile(), now).expect("годный профиль").expect("новый");
    let named = |name: &str, at: Timestamp| {
        Client::create(ClientData { name: Line::new(name).expect("имя"), ..documents::acme() }, at).expect("годный клиент")
    };
    // Клиент отправителя правлен десять дней назад: встречный код с дня до
    // принятого кода его обратно не несёт. Два — сегодня: один придёт
    // обратно эхом, если встречный код его не выкинет, другой получатель
    // правит позже.
    let theirs = named("Acme Ltd", Timestamp::from_unix_millis(now.unix_millis() - 10 * DAY));
    let echoed = named("Эхо сегодня", now);
    let edited = named("Правят там", now);
    let ours = named("Свой клиент", now);
    let seed_sender = || async {
        seed_pair(&profile, &theirs).await;
        let mut writes = Writes::default();
        writes.insert(&echoed);
        writes.insert(&edited);
        documents::commit(writes).await;
    };
    let mut wrong = Vec::new();

    // Отправитель: код всех записей.
    seed_sender().await;
    other_device();
    Device::new().set_ui_lang(lang);
    let (host, mounted) = mount();
    if !opened(&host, lang).await {
        wrong.push("база отправителя не открылась".to_owned());
    }
    let code = make_with(&host, lang, true, &kinds(lang, 3, 0, 1), &mut wrong).await.unwrap_or_default();
    let sender = device_records();
    drop(mounted);
    host.remove();

    // Получатель: тот же ИП, свой клиент и файл книги, прошлый код отсюда —
    // два дня назад.
    seed_pair(&profile, &ours).await;
    let file = kvartik_core::model::attachment::Attachment::create(
        RecordId::random(),
        "чек.pdf",
        kvartik_core::model::attachment::FileBytes::new(b"%PDF-1.4 receipt".to_vec()).expect("годный PDF"),
        now,
    );
    let mut writes = Writes::default();
    writes.insert(&file);
    documents::commit(writes).await;
    other_device();
    let made = Timestamp::from_unix_millis(now.unix_millis() - 2 * DAY);
    Device::new().set_sync_made(crate::store::DB_NAME, made);
    let receiver = Device::new().sync_id();
    let (host, mounted) = mount();
    if !opened(&host, lang).await {
        wrong.push("база получателя не открылась".to_owned());
    }
    let mut back = String::new();
    let later = edited
        .edit(ClientData { name: Line::new("Правят здесь позже").expect("имя"), ..edited.data().clone() }, clock::now())
        .expect("годный")
        .expect("правка");
    if let Some(input) = paste(&host, lang, &code, &mut wrong).await {
        accept(&host, lang, &input, PASSWORD);
        if !wait_for(&host, &landed_words(lang, &kinds(lang, 3, 0, 0), 1)).await {
            wrong.push(format!("код не лёг: «{}»", section_text(&host, "sync")));
        }
        // Получатель правит запись того устройства — позже, чем она пришла.
        let mut writes = Writes::default();
        writes.update(&edited, &later);
        documents::commit(writes).await;
        match sync_button(&host, t(lang, Key::SyncBackMake)) {
            Some(button) => button.click(),
            None => wrong.push(format!("под квитанцией нет встречного кода: «{}»", section_text(&host, "sync"))),
        }
        let day = clock::local_date(made).expect("день");
        // Свой клиент и правленный здесь; профиль и «Эхо» пришли теми же —
        // их нет в счёте.
        let ready = fill(t(lang, Key::SyncMadeSince), &[&kvartik_core::i18n::date(lang, day), &kinds(lang, 2, 0, 0)]);
        if !wait_for(&host, &ready).await {
            wrong.push(format!("встречный код не тот: «{}»", section_text(&host, "sync")));
        }
        // Кнопки кода файлов под встречным кодом нет — и слова о файлах свои.
        if !section_text(&host, "sync").contains(&words_until(lang, Key::SyncBackSkipped, &["1"])) {
            wrong.push(format!("о файлах встречного кода — не те слова: «{}»", section_text(&host, "sync")));
        }
        back = find(&host, ".sync-text:not(.sync-receipt) textarea")
            .await
            .and_then(|area| Reflect::get(&area, &"value".into()).ok())
            .and_then(|value| value.as_string())
            .unwrap_or_default();
        match opened_code(&back).await {
            Some((header, _)) => {
                if header.device() != receiver {
                    wrong.push("встречный код не от получателя".to_owned());
                }
                if header.since() != Some(sync::since_day(day)) {
                    wrong.push(format!("встречный код не с дня до принятого: {:?}", header.since()));
                }
            }
            None => wrong.push(format!("встречный код не открылся паролем того кода: {}", back.chars().take(20).collect::<String>())),
        }
        match code_records(&back).await {
            Some(records) => {
                let has = |id: RecordId| records.iter().any(|(_, found, _, _)| *found == id);
                if has(echoed.id()) {
                    wrong.push("встречный код вернул отправителю его сегодняшнюю правку".to_owned());
                }
                if has(theirs.id()) {
                    wrong.push("встречный код вернул запись старше своего дня".to_owned());
                }
                if !has(ours.id()) {
                    wrong.push("во встречном коде нет своего клиента".to_owned());
                }
                if !records.iter().any(|(_, id, text, _)| *id == later.id() && text.contains("Правят здесь позже")) {
                    wrong.push("во встречном коде нет записи, правленной здесь позже".to_owned());
                }
                if !has(profile.id()) {
                    wrong.push("во встречном коде нет профиля — того устройства ИП не узнает".to_owned());
                }
            }
            None => wrong.push("записи встречного кода не читаются".to_owned()),
        }
        if Device::new().sync_made(crate::store::DB_NAME) != Some(made) {
            wrong.push("встречный код сдвинул ряд изменений получателя".to_owned());
        }
    }
    drop(mounted);
    host.remove();

    // Снова отправитель: код тем же паролем копий — замок тот же, — и под ним
    // встречный код получателя.
    seed_sender().await;
    restore_device(&sender);
    let (host, mounted) = mount();
    if !opened(&host, lang).await {
        wrong.push("база отправителя не открылась снова".to_owned());
    }
    crate::platform::set_hash(&Screen::Sync.href());
    if find(&host, "input[type=radio][name=sync-what]").await.is_none() {
        wrong.push("нет выбора, что передать".to_owned());
    }
    choose_way(&host, true);
    match password_field(&host, t(lang, Key::BackupPassword)) {
        Some(input) => type_into(&input, PASSWORD),
        None => wrong.push("нет поля пароля копий".to_owned()),
    }
    match sync_button(&host, t(lang, Key::SyncMake)) {
        Some(make) => make.click(),
        None => wrong.push("нет «Сделать код»".to_owned()),
    }
    if find(&host, ".sync-reply textarea").await.is_none() {
        wrong.push(format!("код отсюда не готов: «{}»", section_text(&host, "sync")));
    }
    // Код под чужим замком — не встречный этому.
    let salt = [7u8; 16];
    let foreign_key = crate::platform::crypto::derive(&crate::platform::crypto::normalize(PASSWORD), &salt, 1)
        .await
        .expect("ключ");
    let header = sync::Header::new("0.0.189", clock::now(), RecordId::random(), None);
    let store = crate::store::Store::open(|_| {}).await.expect("база открыта");
    let sealing = crate::store::sync::Sealing {
        header: &header,
        key: &foreign_key,
        salt: &salt,
        iterations: 1,
        echo: None,
    };
    let foreign = match crate::store::sync::make(&store, &sealing).await {
        Ok(made) => sync::text_parts(&made.code, 1).join("\n"),
        Err(_) => {
            wrong.push("чужой код не собрался".to_owned());
            String::new()
        }
    };
    drop(store);
    paste_reply(&host, &foreign, &mut wrong).await;
    if !wait_for(&host, t(lang, Key::SyncBackOther)).await {
        wrong.push(format!("код под чужим замком не назван: «{}»", section_text(&host, "sync")));
    }
    paste_reply(&host, &back, &mut wrong).await;
    if !wait_for(&host, t(lang, Key::SyncBackTook)).await {
        wrong.push(format!("встречный код не принят: «{}»", section_text(&host, "sync")));
    }
    if !wait_for(&host, &landed_words(lang, &kinds(lang, 2, 0, 0), 1)).await {
        wrong.push(format!("встречный код не лёг: «{}»", section_text(&host, "sync")));
    }
    if section_text(&host, "sync").contains(t(lang, Key::SyncReceiptTitle)) {
        wrong.push("на встречный код показана квитанция".to_owned());
    }
    match button(&host, &[t(lang, Key::SyncClose)]) {
        Some(close) if !close.class_list().contains("link") => {}
        Some(_) => wrong.push("после встречного кода «Готово, убрать код» — не главная кнопка".to_owned()),
        None => wrong.push("нет «Готово, убрать код»".to_owned()),
    }
    match stored_clients().await {
        Ok(clients) => {
            if !clients.iter().any(|known| known.id() == ours.id()) {
                wrong.push("клиента получателя нет у отправителя".to_owned());
            }
            if !clients.iter().any(|known| known.id() == later.id() && known.data().name.as_str() == "Правят здесь позже") {
                wrong.push("правка получателя не легла у отправителя".to_owned());
            }
        }
        Err(error) => wrong.push(format!("клиенты не читаются: {error:?}")),
    }
    if Device::new().sync_received(crate::store::DB_NAME).until(receiver).is_none() {
        wrong.push("отправитель не запомнил, что принял от получателя".to_owned());
    }
    drop(mounted);
    host.remove();
    other_device();
    crate::platform::set_hash("");
    documents::forget_device();
    assert!(wrong.is_empty(), "обмен в обе стороны: {wrong:#?}");
}

/// Встречный код — того же охвата, что принятый (ревью задачи 124): контора
/// двух ИП, принявшая код одного ИП, отвечает кодом только этого ИП — без
/// записей ИП списка, без меток баз и без второго ИП. Кодом всех ИП тому, у
/// кого ИП один, легли бы все ИП конторы.
#[wasm_bindgen_test]
async fn the_back_code_keeps_the_scope_of_the_code() {
    use super::office::{forget_office, named, seed, seed_office};
    use crate::store::office::OFFICE_DB;
    use kvartik_core::model::office::Ip;
    let lang = Lang::Ru;
    let aram = named("ИП Арам Петросян");
    let anna = named("ИП Анна Акопян");
    let client = Client::create(documents::acme(), clock::now()).expect("годный клиент");
    let annas = Client::create(documents::acme(), clock::now()).expect("годный клиент");
    let fresh = || async {
        crate::store::delete_database(crate::store::DB_NAME).await;
        crate::store::delete_database(OFFICE_DB).await;
        forget_office();
        other_device();
        Device::new().set_ui_lang(lang);
    };
    let mut wrong = Vec::new();

    // Отправитель: один ИП, код всех записей.
    fresh().await;
    seed_pair(&aram, &client).await;
    let (host, mounted) = mount();
    if !opened(&host, lang).await {
        wrong.push("база отправителя не открылась".to_owned());
    }
    let code = make(&host, lang, true, &mut wrong).await.unwrap_or_default();
    drop(mounted);
    host.remove();

    // Получатель: контора — тот же ИП открыт и ещё второй ИП со своим клиентом.
    fresh().await;
    let today = clock::today().expect("сегодня");
    let first = {
        let mut ip = Ip::first(Some(&aram), today, clock::now());
        ip.copy_profile(&aram, clock::now());
        ip
    };
    let mut second = Ip::new(today, clock::now());
    second.copy_profile(&anna, clock::now());
    crate::store::delete_database(second.base()).await;
    seed(crate::store::DB_NAME, &aram).await;
    seed(second.base(), &anna).await;
    {
        let store = crate::store::Store::open_base(second.base(), |_| {}).await.expect("база открыта");
        let mut writes = Writes::default();
        writes.insert(&annas);
        store.write().expect("транзакция").commit(writes).await.expect("записано");
    }
    seed_office(&[&first, &second]).await;
    Device::new().set_office();
    crate::store::office::set_tab_base(crate::store::DB_NAME);
    let (host, mounted) = mount();
    if !opened(&host, lang).await {
        wrong.push("база конторы не открылась".to_owned());
    }
    if let Some(input) = paste(&host, lang, &code, &mut wrong).await {
        accept(&host, lang, &input, PASSWORD);
        if !wait_for(&host, t(lang, Key::SyncReceiptTitle)).await {
            wrong.push(format!("код одного ИП не лёг в контору: «{}»", section_text(&host, "sync")));
        }
        match sync_button(&host, t(lang, Key::SyncBackMake)) {
            Some(button) => button.click(),
            None => wrong.push(format!("нет встречного кода: «{}»", section_text(&host, "sync"))),
        }
        let back = match find(&host, ".sync-text:not(.sync-receipt) textarea").await {
            Some(area) => Reflect::get(&area, &"value".into()).ok().and_then(|value| value.as_string()).unwrap_or_default(),
            None => String::new(),
        };
        if back.is_empty() && !section_text(&host, "sync").contains(t(lang, Key::SyncNothing)) {
            wrong.push(format!("встречный код не готов: «{}»", section_text(&host, "sync")));
        }
        if !back.is_empty() {
            match code_records(&back).await {
                Some(records) => {
                    if records.iter().any(|(_, _, _, base)| base.is_some()) {
                        wrong.push("встречный код на код одного ИП — с метками баз, кодом всех ИП".to_owned());
                    }
                    if records.iter().any(|(kind, ..)| *kind == Ip::KIND) {
                        wrong.push("во встречном коде — записи ИП списка".to_owned());
                    }
                    if records.iter().any(|(_, id, ..)| *id == annas.id() || *id == anna.id()) {
                        wrong.push("во встречном коде — второй ИП конторы".to_owned());
                    }
                }
                None => wrong.push("встречный код не читается".to_owned()),
            }
        }
    }
    drop(mounted);
    host.remove();
    crate::store::delete_database(second.base()).await;
    fresh().await;
    crate::platform::set_hash("");
    documents::forget_device();
    assert!(wrong.is_empty(), "охват встречного кода: {wrong:#?}");
}

/// Встречный код, в котором нести нечего, кроме эха (задача 124, п. 14): у
/// получателя после того дня ничего своего — только то, что пришло, — и он
/// говорит «передавать нечего», а не показывает код с тем же самым.
#[wasm_bindgen_test]
async fn the_echo_alone_is_nothing_to_send_back() {
    use kvartik_core::model::record::Timestamp;
    const DAY: i64 = 86_400_000;
    let lang = Lang::Ru;
    let now = clock::now();
    let profile = Profile::save(None, documents::profile(), now).expect("годный профиль").expect("новый");
    let theirs = Client::create(documents::acme(), now).expect("годный клиент");
    let mut wrong = Vec::new();
    seed_pair(&profile, &theirs).await;
    other_device();
    Device::new().set_ui_lang(lang);
    let (host, mounted) = mount();
    if !opened(&host, lang).await {
        wrong.push("база отправителя не открылась".to_owned());
    }
    let code = make(&host, lang, true, &mut wrong).await.unwrap_or_default();
    drop(mounted);
    host.remove();

    // Получатель: тот же профиль, своего ничего, прошлый код — два дня назад.
    crate::store::delete_database(crate::store::DB_NAME).await;
    let mut writes = Writes::default();
    writes.insert(&profile);
    documents::commit(writes).await;
    other_device();
    Device::new().set_sync_made(crate::store::DB_NAME, Timestamp::from_unix_millis(now.unix_millis() - 2 * DAY));
    let (host, mounted) = mount();
    if !opened(&host, lang).await {
        wrong.push("база получателя не открылась".to_owned());
    }
    if let Some(input) = paste(&host, lang, &code, &mut wrong).await {
        accept(&host, lang, &input, PASSWORD);
        if !wait_for(&host, &landed_words(lang, &kinds(lang, 1, 0, 0), 1)).await {
            wrong.push(format!("код не лёг: «{}»", section_text(&host, "sync")));
        }
        match sync_button(&host, t(lang, Key::SyncBackMake)) {
            Some(button) => button.click(),
            None => wrong.push(format!("под квитанцией нет встречного кода: «{}»", section_text(&host, "sync"))),
        }
        if !wait_for(&host, t(lang, Key::SyncNothing)).await {
            wrong.push(format!("эхо вместо «передавать нечего»: «{}»", section_text(&host, "sync")));
        }
        if host.query_selector("#sync .sync-text:not(.sync-receipt) textarea").ok().flatten().is_some() {
            wrong.push("встречный код из одного эха показан".to_owned());
        }
    }
    drop(mounted);
    host.remove();
    other_device();
    crate::platform::set_hash("");
    documents::forget_device();
    assert!(wrong.is_empty(), "встречный код из эха: {wrong:#?}");
}
