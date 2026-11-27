//! Браузерные тесты экрана ИИ-помощника в окне (задачи 102 и 111): сам
//! помощник — рамкой своего происхождения, и его экран проверяют тесты рамки
//! (`crates/ai/src/tests.rs`), а здесь — окно вокруг неё: рамка встаёт, окно
//! отвечает на её сообщения и только на её. У раннера тестов своего домена
//! нет: рамка — пустая страница того же происхождения
//! (`assistant::TEST_ORIGIN`), а сообщение «от рамки» тест шлёт кодом из её
//! окна — `event.source` у такого сообщения её окно, как у настоящей.

use js_sys::{Array, Function, JSON, Reflect};
use kvartik_core::ai::{Provider, Setup};
use kvartik_core::i18n::{Key, Lang, fill, t};
use leptos::prelude::Owner;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_test::wasm_bindgen_test;
use web_sys::HtmlElement;

use super::{button, mount_at, opened, sleep, wait_for};
use crate::store::device::Device;
use crate::ui::assistant::{LOAD_WAIT, TEST_ORIGIN};
use crate::ui::route::Screen;

/// Подождать, пока условие станет истинным; `false` — не дождались за пять
/// секунд.
async fn until(check: impl Fn() -> bool) -> bool {
    for _ in 0..100 {
        if check() {
            return true;
        }
        sleep(50).await;
    }
    false
}

fn frame(host: &HtmlElement) -> Option<HtmlElement> {
    host.query_selector("#ask iframe.ai-frame").ok().flatten().map(JsCast::unchecked_into)
}

/// Окно рамки.
fn inner(frame: &HtmlElement) -> JsValue {
    Reflect::get(frame, &"contentWindow".into()).unwrap_or(JsValue::NULL)
}

/// Код `body` — функцией, созданной в окне рамки: что она отправит, придёт
/// от рамки (`event.source`).
fn run_in(window: &JsValue, args: &str, body: &str, values: &[JsValue]) -> JsValue {
    let make = Reflect::get(window, &"Function".into()).expect("у окна рамки есть Function");
    let make: &Function = make.unchecked_ref();
    let made = Reflect::construct(make, &Array::of2(&JsValue::from_str(args), &JsValue::from_str(body)))
        .expect("функция в окне рамки создаётся");
    let call: Function = made.unchecked_into();
    let list = Array::new();
    for value in values {
        list.push(value);
    }
    call.apply(&JsValue::NULL, &list).expect("функция в окне рамки идёт")
}

/// Сообщения окна, которые получила рамка, — JSON-текстом.
fn got(window: &JsValue) -> String {
    Reflect::get(window, &"__got".into())
        .ok()
        .and_then(|got| JSON::stringify(&got).ok())
        .and_then(|text| text.as_string())
        .unwrap_or_default()
}

thread_local! {
    static PANICKED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Была ли паника, пока шло `wait`, — в таймере, задаче, обработчике:
/// хук отмечает её и отдаёт прежнему, а после — прежний на месте.
async fn panics_while(wait: impl std::future::Future<Output = ()>) -> bool {
    type Hook = dyn Fn(&std::panic::PanicHookInfo<'_>) + Send + Sync;
    PANICKED.with(|panicked| panicked.set(false));
    let previous: std::sync::Arc<Hook> = std::panic::take_hook().into();
    let chained = std::sync::Arc::clone(&previous);
    std::panic::set_hook(Box::new(move |info| {
        PANICKED.with(|panicked| panicked.set(true));
        chained(info);
    }));
    wait.await;
    // После паники хук не сменить: для std поток так и паникует — `abort`
    // в wasm счёт паник не снимает, — и тест красный в любом случае.
    if PANICKED.with(std::cell::Cell::get) {
        return true;
    }
    drop(std::panic::take_hook());
    std::panic::set_hook(Box::new(move |info| previous(info)));
    false
}

/// Рамка — окну: сообщение строкой, как шлёт рамка.
fn say(window: &JsValue, text: &str) {
    run_in(window, "text", "parent.postMessage(text, '*');", &[JsValue::from_str(text)]);
}

/// Рамка встаёт, окно говорит ей язык и день, а ключ 0.0.158 стирает и рамке
/// не отдаёт; на «подключён Gemini» — пишет это строкой экрана налога, на
/// высоту — растит рамку; сообщение не из рамки — мимо.
#[wasm_bindgen_test]
async fn the_assistant_is_a_frame_of_its_own_origin_and_the_window_answers_it() {
    let lang = Lang::Ru;
    let device = Device::new();
    device.set_ui_lang(lang);
    let old = Setup::new(Provider::Claude, "sk-ant-old-1", "").expect("годится");
    device.set_ai(&old.to_text());
    device.set_ai_by(None);
    let origin = web_sys::window().and_then(|window| window.location().origin().ok()).expect("у страницы тестов есть адрес");
    TEST_ORIGIN.with_borrow_mut(|test| *test = Some(origin));
    let (host, mounted) = mount_at("#/ask");
    let mut wrong = Vec::new();
    if !until(|| frame(&host).is_some()).await {
        wrong.push("рамки помощника нет".to_owned());
    }
    let Some(element) = frame(&host) else {
        drop(mounted);
        host.remove();
        TEST_ORIGIN.with_borrow_mut(|test| *test = None);
        panic!("{wrong:#?}");
    };
    if element.get_attribute("title").as_deref() != Some(t(lang, Key::AiTitle)) {
        wrong.push(format!("у рамки подпись {:?}", element.get_attribute("title")));
    }
    if !element.get_attribute("sandbox").is_some_and(|sandbox| sandbox.contains("allow-scripts") && !sandbox.contains("allow-top-navigation")) {
        wrong.push(format!("у рамки sandbox {:?}", element.get_attribute("sandbox")));
    }
    if !wait_for(&host, t(lang, Key::PartLoading)).await {
        wrong.push("пока рамка не встала, нет «Загрузка…»".to_owned());
    }
    // Окно сначала дочитывает базу и профиль: тогда состояние рамке шлёт
    // только ответ на её «встала», а не дочитанный профиль.
    if !opened(&host, lang).await {
        wrong.push("база окна не открылась".to_owned());
    }
    let window = inner(&element);
    run_in(&window, "", "window.__got = []; addEventListener('message', (event) => window.__got.push(event.data));", &[]);
    say(&window, r#"{"kind":"ready"}"#);
    if !until(|| {
        let got = got(&window);
        got.contains(r#"\"kind\":\"state\""#) && got.contains(r#"\"lang\":\"ru\""#)
    })
    .await
    {
        wrong.push(format!("рамка не получила состояние окна: {}", got(&window)));
    }
    if device.ai().is_some() {
        wrong.push("ключ 0.0.158 остался в окне".to_owned());
    }
    if !until(|| !host.text_content().unwrap_or_default().contains(t(lang, Key::PartLoading))).await {
        wrong.push("встала рамка, а «Загрузка…» осталась".to_owned());
    }
    say(&window, r#"{"kind":"connected","provider":"gemini"}"#);
    if !until(|| device.ai_by().as_deref() == Some("gemini")).await {
        wrong.push(format!("какой ИИ подключён, окно не запомнило: {:?}", device.ai_by()));
    }
    say(&window, r#"{"kind":"height","px":640}"#);
    if !until(|| element.style().get_property_value("height").ok().as_deref() == Some("640px")).await {
        wrong.push(format!("высота рамки — {:?}", element.style().get_property_value("height")));
    }
    // Всё, что окно послало рамке до высоты, к ней уже пришло: ключа среди
    // этого нет.
    if got(&window).contains("sk-ant-old-1") {
        wrong.push(format!("ключ 0.0.158 ушёл рамке: {}", got(&window)));
    }
    // Сообщение не от рамки — от самого окна — мимо, сколько бы оно ни
    // называло себя рамкой.
    if let Some(top) = web_sys::window() {
        let _ = top.post_message(&JsValue::from_str(r#"{"kind":"connected","provider":"grok"}"#), "*");
    }
    // И от рамки — испорченное и чужого вида — мимо.
    say(&window, r#"{"kind":"connected","provider":"mistral"}"#);
    say(&window, "не JSON");
    sleep(300).await;
    if device.ai_by().as_deref() != Some("gemini") {
        wrong.push(format!("окно послушало не рамку: {:?}", device.ai_by()));
    }
    // Строка на экране налога — какой ИИ подключён в рамке. Экран налога
    // рисуется с первого показа.
    crate::platform::set_hash(&Screen::Tax.href());
    let entry = fill(t(lang, Key::AiEntryOn), &["Gemini"]);
    if !wait_for(&host, &entry).await {
        wrong.push(format!("нет строки «{entry}» на экране налога"));
    }
    drop(mounted);
    host.remove();
    TEST_ORIGIN.with_borrow_mut(|test| *test = None);
    device.clear_ai();
    device.set_ai_by(None);
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// Рамка не встала в срок — слова о сбое и «Загрузить снова», а нажатие снова
/// ждёт её. Окно, снятое посреди ожидания, снимает и его: таймер, переживший
/// окно, читал его освобождённые сигналы, и паника роняла программу — в CI
/// Firefox 0.0.214 она оборвала прогон посреди чужого теста.
#[wasm_bindgen_test]
async fn a_frame_that_does_not_come_says_so_and_its_wait_goes_with_the_window() {
    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let origin = web_sys::window().and_then(|window| window.location().origin().ok()).expect("у страницы тестов есть адрес");
    TEST_ORIGIN.with_borrow_mut(|test| *test = Some(origin));
    // Рамка тестов — пустая страница: «встала» она не скажет никогда.
    LOAD_WAIT.with(|wait| wait.set(Some(std::time::Duration::from_millis(300))));
    // Окно — под своим владельцем теста: снятое окно в прогоне тестов держит
    // начатая им работа, и его сигналы живы, пока она идёт, — в CI их
    // освободило, а здесь освобождает тест сам (`Owner::cleanup`).
    let root = Owner::new();
    let (host, mounted) = root.with(|| mount_at("#/ask"));
    let mut wrong = Vec::new();
    if !wait_for(&host, t(lang, Key::AiFrameFailed)).await {
        wrong.push("рамка не встала, а слов о сбое нет".to_owned());
    }
    if !opened(&host, lang).await {
        wrong.push("база окна не открылась".to_owned());
    }
    // «Загрузить снова» — новое ожидание, и окно снято посреди него.
    LOAD_WAIT.with(|wait| wait.set(Some(std::time::Duration::from_millis(500))));
    match button(&host, &[t(lang, Key::AiFrameRetry)]) {
        Some(retry) => retry.click(),
        None => wrong.push("нет «Загрузить снова»".to_owned()),
    }
    if !until(|| !host.text_content().unwrap_or_default().contains(t(lang, Key::AiFrameFailed))).await {
        wrong.push("после «Загрузить снова» слова о сбое остались".to_owned());
    }
    drop(mounted);
    host.remove();
    root.cleanup();
    // Срок нового ожидания проходит уже без окна. Паника в таймере — вне
    // теста, и раннер её тесту не засчитывает: wasm после неё ещё отвечает,
    // а в CI следующий тест повис. Поэтому её ловит хук на время ожидания.
    if panics_while(sleep(900)).await {
        wrong.push("паника: ожидание рамки пережило окно".to_owned());
    }
    LOAD_WAIT.with(|wait| wait.set(None));
    TEST_ORIGIN.with_borrow_mut(|test| *test = None);
    assert!(wrong.is_empty(), "{wrong:#?}");
}
