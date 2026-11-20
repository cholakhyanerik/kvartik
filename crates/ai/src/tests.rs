//! Браузерные тесты рамки ИИ-помощника (задачи 102 и 111): экран рамки
//! в настоящем DOM. Поставщик — подменой `window.fetch`: запрос на
//! `https://…` она запоминает и отвечает по плану теста — потоком SSE, кодом
//! ошибки или обрывом сети. Окно-родитель — сообщениями прямо в `Inbox`, а
//! что рамка сказала окну, — `bridge::sent`. CSP в раннере тестов нет: что
//! CSP рамки пускает четыре адреса и не пускает других, проверяет
//! `crates/ai/tests/csp.rs` и проверка собранного сайта
//! (`tools/browser/check.mjs`, «ИИ-помощник»).

use js_sys::{Function, JSON, Promise, Reflect};
use kvartik_core::ai::frame::{Colors, FromFrame, ToFrame};
use kvartik_core::ai::{Provider, Setup};
use kvartik_core::i18n::{Key, Lang, fill, t};
use kvartik_core::model::date::Date;
use leptos::prelude::*;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};
use web_sys::{Element, HtmlElement};

use crate::bridge::{self, Inbox};
use crate::page::Page;
use crate::{look, store};

wasm_bindgen_test_configure!(run_in_browser);

/// Подмена `fetch`: план — `window.__kvartikAi` (`status`, `text`, `chunks`,
/// `hold` — не закрывать поток, `error` — обрыв сети, `pending` — ответа нет,
/// `delay` — ответ с опозданием), запросы — `window.__kvartikAiCalls`,
/// отмену потока отмечает `__kvartikAiCancelled`.
const STUB: &str = r#"
if (!window.__kvartikAiOriginal) window.__kvartikAiOriginal = window.fetch;
const original = window.__kvartikAiOriginal;
window.__kvartikAiCalls = [];
window.__kvartikAiCancelled = false;
window.fetch = function (url, init) {
  const plan = window.__kvartikAi;
  const address = String(url);
  if (!address.startsWith("https://")) return original.apply(this, arguments);
  if (!plan) return Promise.reject(new TypeError("нет плана подмены"));
  window.__kvartikAiCalls.push({ url: address, headers: (init && init.headers) || {}, body: (init && init.body) || "" });
  if (plan.error) return Promise.reject(new TypeError("Failed to fetch"));
  if (plan.pending) return new Promise(() => {});
  if (plan.status !== 200) return Promise.resolve(new Response(plan.text || "", { status: plan.status }));
  const encoder = new TextEncoder();
  const stream = new ReadableStream({
    start(controller) {
      for (const chunk of plan.chunks || []) controller.enqueue(encoder.encode(chunk));
      if (!plan.hold) controller.close();
    },
    cancel() { window.__kvartikAiCancelled = true; },
  });
  const response = new Response(stream, { status: 200, headers: { "content-type": "text/event-stream" } });
  if (plan.delay) return new Promise((resolve) => setTimeout(() => resolve(response), plan.delay));
  return Promise.resolve(response);
};
"#;

fn window() -> web_sys::Window {
    web_sys::window().expect("окно есть")
}

fn stub_fetch() {
    Function::new_no_args(STUB).call0(&JsValue::NULL).expect("подмена встала");
}

fn unstub_fetch() {
    let window = window();
    if let Ok(original) = Reflect::get(&window, &"__kvartikAiOriginal".into())
        && original.is_function()
    {
        Reflect::set(&window, &"fetch".into(), &original).expect("fetch вернулся");
    }
    let _ = Reflect::delete_property(&window, &"__kvartikAi".into());
}

/// План ответа поставщика — JSON.
fn plan(json: &str) {
    let plan = JSON::parse(json).expect("план — JSON");
    Reflect::set(&window(), &"__kvartikAi".into(), &plan).expect("план встал");
}

/// Запросы к поставщику, что видела подмена, — JSON-текстом.
fn calls() -> String {
    Reflect::get(&window(), &"__kvartikAiCalls".into())
        .ok()
        .and_then(|calls| JSON::stringify(&calls).ok())
        .and_then(|text| text.as_string())
        .unwrap_or_default()
}

/// Поток Anthropic: текст двумя кусками и конец — или без конца.
fn claude_chunks(end: bool) -> String {
    let mut chunks = vec![
        "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{}}\n\n",
        "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Срок — до 20 января. \"}}\n\n",
        "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"См. https://www.arlis.am/hy/acts/230455\"}}\n\n",
    ];
    if end {
        chunks.push("event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"}}\n\n");
        chunks.push("event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n");
    }
    encoded(&chunks)
}

fn encoded(chunks: &[&str]) -> String {
    let parts: Vec<String> = chunks
        .iter()
        .map(|chunk| JSON::stringify(&(*chunk).into()).ok().and_then(|text| text.as_string()).unwrap_or_default())
        .collect();
    format!("[{}]", parts.join(","))
}

/// Подождать `ms` миллисекунд: перерисовка Leptos идёт следующей задачей.
async fn sleep(ms: i32) {
    let promise = Promise::new(&mut |resolve, _| {
        window()
            .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms)
            .expect("таймер поставлен");
    });
    promise.await.expect("таймер срабатывает");
}

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

async fn wait_for(host: &HtmlElement, text: &str) -> bool {
    until(|| host.text_content().is_some_and(|content| content.contains(text))).await
}

/// Экран рамки в своём `<div>` и его почта от окна; снять — бросить ручку
/// и убрать `<div>`.
fn mount(lang: Lang) -> (HtmlElement, Inbox, impl Sized) {
    let document = window().document().expect("есть документ");
    let host: HtmlElement = document.create_element("div").expect("создаётся").unchecked_into();
    document.body().expect("есть <body>").append_child(&host).expect("вставляется");
    let inbox = Inbox::new(lang);
    let mounted = leptos::mount::mount_to(host.clone(), move || view! { <Page inbox /> });
    (host, inbox, mounted)
}

/// Поле (`input` или `textarea`) внутри подписи `label`.
fn labelled(host: &HtmlElement, label: &str, what: &str) -> Option<Element> {
    let labels = host.query_selector_all("label").ok()?;
    (0..labels.length())
        .filter_map(|at| labels.item(at))
        .map(JsCast::unchecked_into::<Element>)
        .find(|element| {
            element
                .first_child()
                .and_then(|text| text.text_content())
                .is_some_and(|text| text.trim() == label)
        })?
        .query_selector(what)
        .ok()
        .flatten()
}

/// Ввести текст в поле, как человек: значение и событие `input`.
fn type_in(host: &HtmlElement, label: &str, what: &str, text: &str) -> bool {
    let Some(field) = labelled(host, label, what) else {
        return false;
    };
    let _ = Reflect::set(&field, &"value".into(), &text.into());
    let event = web_sys::Event::new("input").expect("событие создаётся");
    field.dispatch_event(&event).is_ok()
}

fn value(host: &HtmlElement, label: &str, what: &str) -> Option<String> {
    let field = labelled(host, label, what)?;
    Reflect::get(&field, &"value".into()).ok()?.as_string()
}

fn button(host: &HtmlElement, label: &str) -> Option<HtmlElement> {
    let buttons = host.query_selector_all("button").ok()?;
    (0..buttons.length())
        .filter_map(|at| buttons.item(at))
        .map(JsCast::unchecked_into::<HtmlElement>)
        .find(|button| button.text_content().unwrap_or_default().trim() == label)
}

fn press(host: &HtmlElement, label: &str) -> bool {
    match button(host, label) {
        Some(button) => {
            button.click();
            true
        }
        None => false,
    }
}

/// Нажать, когда кнопка станет нажимаемой: после ввода привязка Leptos
/// включает её своей задачей (ревью задачи 102).
async fn press_when_ready(host: &HtmlElement, label: &str) -> bool {
    until(|| button(host, label).is_some_and(|button| !button.has_attribute("disabled"))).await && press(host, label)
}

/// Последнее «занято», что рамка сказала окну.
fn last_busy() -> Option<(bool, bool)> {
    bridge::sent().into_iter().rev().find_map(|message| match message {
        FromFrame::Busy { busy, talk } => Some((busy, talk)),
        _ => None,
    })
}

fn said(message: &FromFrame) -> bool {
    bridge::sent().contains(message)
}

/// Ключ подключается, ответ приходит потоком и встаёт в разговор со ссылкой
/// на закон, в запросе — ключ, версия API, заголовок прямого доступа
/// и вопрос; окну — какой ИИ подключён и что начато; «Новый разговор»
/// стирает разговор, а «Убрать ключ» — запись рамки.
#[wasm_bindgen_test]
async fn a_key_is_connected_an_answer_streams_in_and_the_key_is_removed() {
    let lang = Lang::Ru;
    store::clear();
    bridge::reset();
    stub_fetch();
    let (host, _inbox, mounted) = mount(lang);
    let mut wrong = Vec::new();
    if !wait_for(&host, t(lang, Key::AiAbout)).await {
        wrong.push("экран помощника не встал".to_owned());
    }
    if !type_in(&host, t(lang, Key::AiKey), "input", "sk-ant-test-0123abcd") {
        wrong.push("нет поля ключа".to_owned());
    }
    if !until(|| last_busy() == Some((true, false))).await {
        wrong.push(format!("вписанный ключ окну не «занято»: {:?}", last_busy()));
    }
    if !press(&host, t(lang, Key::AiSave)) {
        wrong.push("нет «Сохранить ключ»".to_owned());
    }
    let connected = fill(t(lang, Key::AiConnected), &["Claude", "claude-opus-5-5", "abcd"]);
    if !wait_for(&host, &connected).await {
        wrong.push(format!("нет строки «{connected}»"));
    }
    let saved = store::stored();
    if saved.as_ref().map(|setup| (setup.provider, setup.key.as_str())) != Some((Provider::Claude, "sk-ant-test-0123abcd")) {
        wrong.push(format!("в рамке — {saved:?}"));
    }
    let claude = FromFrame::Connected {
        provider: Some("claude".to_owned()),
    };
    if !until(|| said(&claude)).await {
        wrong.push("окну не сказано, что подключён Claude".to_owned());
    }
    plan(&format!(r#"{{"status":200,"chunks":{}}}"#, claude_chunks(true)));
    if !type_in(&host, t(lang, Key::AiQuestion), "textarea", "Когда платить налог за IV квартал?") {
        wrong.push("нет поля вопроса".to_owned());
    }
    if !press_when_ready(&host, t(lang, Key::AiAsk)).await {
        wrong.push("нет «Спросить»".to_owned());
    }
    if !wait_for(&host, "Срок — до 20 января. См. https://www.arlis.am/hy/acts/230455").await {
        wrong.push("ответ не встал".to_owned());
    }
    let link = host
        .query_selector(r#"#ask .ai-a a[href="https://www.arlis.am/hy/acts/230455"][target="_blank"]"#)
        .ok()
        .flatten();
    if link.is_none() {
        wrong.push("адрес закона в ответе — не ссылка".to_owned());
    }
    let sent = calls();
    for part in [
        "https://api.anthropic.com/v1/messages",
        r#""x-api-key":"sk-ant-test-0123abcd""#,
        r#""anthropic-version":"2026-10-21""#,
        r#""anthropic-dangerous-direct-browser-access":"true""#,
        "Когда платить налог за IV квартал?",
    ] {
        if !sent.contains(part) {
            wrong.push(format!("в запросе нет {part}"));
        }
    }
    if value(&host, t(lang, Key::AiQuestion), "textarea").as_deref() != Some("") {
        wrong.push("поле вопроса после ответа не пустое".to_owned());
    }
    // Ответ есть, вопроса нет: окну — разговор, но не «занято».
    if !until(|| last_busy() == Some((false, true))).await {
        wrong.push(format!("после ответа окну не «разговор»: {:?}", last_busy()));
    }
    if !press(&host, t(lang, Key::AiNew)) {
        wrong.push("нет «Новый разговор»".to_owned());
    }
    if !until(|| host.query_selector("#ask .ai-talk").ok().flatten().is_none()).await {
        wrong.push("разговор не стёрся".to_owned());
    }
    if !until(|| last_busy() == Some((false, false))).await {
        wrong.push(format!("после «Новый разговор» окну не «свободно»: {:?}", last_busy()));
    }
    if !press(&host, t(lang, Key::AiRemove)) {
        wrong.push("нет «Убрать ключ»".to_owned());
    }
    if !wait_for(&host, t(lang, Key::AiRemoved)).await {
        wrong.push("нет слов «Ключ убран»".to_owned());
    }
    if store::stored().is_some() {
        wrong.push("ключ остался в рамке".to_owned());
    }
    if !until(|| bridge::sent().last() == Some(&FromFrame::Connected { provider: None })).await {
        wrong.push(format!("окну не сказано, что ключа нет: {:?}", bridge::sent().last()));
    }
    drop(mounted);
    host.remove();
    unstub_fetch();
    store::clear();
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// Отказ ключу, обрыв сети и остановка — словами: у отказа — и слова
/// поставщика, у обрыва ChatGPT — что неверный ключ OpenAI выглядит так же,
/// а остановленный ответ остаётся в разговоре недописанным, и поток
/// закрывается; запоздавший после остановки ответ в разговор не ложится,
/// а пустой — словами.
#[wasm_bindgen_test]
async fn a_refused_key_a_dropped_network_and_a_stop_are_said_in_words() {
    let lang = Lang::Ru;
    bridge::reset();
    store::save(&Setup::new(Provider::Claude, "sk-ant-wrong-1", "").expect("годится"));
    stub_fetch();
    let (host, _inbox, mounted) = mount(lang);
    let mut wrong = Vec::new();
    if !wait_for(&host, t(lang, Key::AiWhatGoes).split("{}").nth(1).unwrap_or_default()).await {
        wrong.push("поле вопроса не встало".to_owned());
    }
    plan(r#"{"status":401,"text":"{\"type\":\"error\",\"error\":{\"type\":\"authentication_error\",\"message\":\"invalid x-api-key\"}}"}"#);
    type_in(&host, t(lang, Key::AiQuestion), "textarea", "Вопрос");
    if !press_when_ready(&host, t(lang, Key::AiAsk)).await {
        wrong.push("нет «Спросить»".to_owned());
    }
    for words in [fill(t(lang, Key::AiFaultKey), &["Claude"]), fill(t(lang, Key::AiSaid), &["Claude", "invalid x-api-key"])] {
        if !wait_for(&host, &words).await {
            wrong.push(format!("нет слов «{words}»"));
        }
    }
    if value(&host, t(lang, Key::AiQuestion), "textarea").as_deref() != Some("Вопрос") {
        wrong.push("вопрос без ответа не вернулся в поле".to_owned());
    }

    // Обрыв сети у ChatGPT: и о сети, и о ключе OpenAI.
    store::save(&Setup::new(Provider::ChatGpt, "sk-proj-1", "").expect("годится"));
    plan(r#"{"error":true}"#);
    press_when_ready(&host, t(lang, Key::AiAsk)).await;
    for words in [fill(t(lang, Key::AiFaultNetwork), &["ChatGPT"]), t(lang, Key::AiFaultNetworkOpenAi).to_owned()] {
        if !wait_for(&host, &words).await {
            wrong.push(format!("нет слов «{words}»"));
        }
    }

    // Поток без конца — «Остановить»: недописанный ответ остаётся, поток
    // закрыт.
    store::save(&Setup::new(Provider::Claude, "sk-ant-right-1", "").expect("годится"));
    plan(&format!(r#"{{"status":200,"hold":true,"chunks":{}}}"#, claude_chunks(false)));
    press_when_ready(&host, t(lang, Key::AiAsk)).await;
    if !wait_for(&host, "Срок — до 20 января.").await {
        wrong.push("начало ответа не встало".to_owned());
    }
    if !wait_for(&host, t(lang, Key::AiWriting)).await {
        wrong.push("нет «ИИ отвечает…»".to_owned());
    }
    if !press(&host, t(lang, Key::AiStop)) {
        wrong.push("нет «Остановить»".to_owned());
    }
    if !wait_for(&host, t(lang, Key::AiStopped)).await {
        wrong.push("нет «Ответ остановлен»".to_owned());
    }
    let cancelled = Reflect::get(&window(), &"__kvartikAiCancelled".into()).ok().and_then(|value| value.as_bool());
    if cancelled != Some(true) {
        wrong.push("поток не закрыт".to_owned());
    }
    if host.query_selector("#ask .ai-talk li").ok().flatten().is_none() {
        wrong.push("остановленный ответ не остался в разговоре".to_owned());
    }

    // Поставщик молчит — «Остановить» срабатывает сразу, до ответа.
    plan(r#"{"status":200,"pending":true}"#);
    type_in(&host, t(lang, Key::AiQuestion), "textarea", "Вопрос без ответа");
    press_when_ready(&host, t(lang, Key::AiAsk)).await;
    if !wait_for(&host, t(lang, Key::AiWriting)).await {
        wrong.push("до ответа нет «ИИ отвечает…»".to_owned());
    }
    if !press_when_ready(&host, t(lang, Key::AiStop)).await {
        wrong.push("до ответа нет «Остановить»".to_owned());
    }
    if !until(|| value(&host, t(lang, Key::AiQuestion), "textarea").as_deref() == Some("Вопрос без ответа")).await {
        wrong.push("остановленный до ответа вопрос не вернулся в поле".to_owned());
    }

    // Остановили, а поставщик ответил потом — в разговор ответ не ложится.
    plan(&format!(r#"{{"status":200,"delay":300,"chunks":{}}}"#, claude_chunks(true)));
    let before = host.query_selector_all("#ask .ai-talk li").map(|list| list.length()).unwrap_or_default();
    press_when_ready(&host, t(lang, Key::AiAsk)).await;
    if !press_when_ready(&host, t(lang, Key::AiStop)).await {
        wrong.push("перед запоздавшим ответом нет «Остановить»".to_owned());
    }
    sleep(600).await;
    let after = host.query_selector_all("#ask .ai-talk li").map(|list| list.length()).unwrap_or_default();
    if after != before {
        wrong.push(format!("запоздавший ответ лёг в разговор: пунктов {before} → {after}"));
    }

    // Ответ кончился, а текста в нём нет — словами, вопрос — в поле.
    plan(&format!(
        r#"{{"status":200,"chunks":{}}}"#,
        encoded(&[
            "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{}}\n\n",
            "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"}}\n\n",
            "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
        ])
    ));
    press_when_ready(&host, t(lang, Key::AiAsk)).await;
    if !wait_for(&host, t(lang, Key::AiEmpty)).await {
        wrong.push("о пустом ответе — ни слова".to_owned());
    }
    if !until(|| value(&host, t(lang, Key::AiQuestion), "textarea").as_deref() == Some("Вопрос без ответа")).await {
        wrong.push("вопрос без ответа не остался в поле".to_owned());
    }
    drop(mounted);
    host.remove();
    unstub_fetch();
    store::clear();
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// Окно сообщает рамке язык, день, есть ли профиль и свои цвета: рамка
/// переходит на язык окна, ставит его цвета на `<html>`, по «Добавить мои
/// данные» просит у окна текст и вписывает пришедший в поле, а день окна
/// уходит модели.
#[wasm_bindgen_test]
async fn the_window_gives_language_day_colors_and_my_data() {
    bridge::reset();
    store::save(&Setup::new(Provider::Claude, "sk-ant-day-1", "").expect("годится"));
    stub_fetch();
    let (host, inbox, mounted) = mount(Lang::Ru);
    let mut wrong = Vec::new();
    if !wait_for(&host, t(Lang::Ru, Key::AiAbout)).await {
        wrong.push("экран не встал".to_owned());
    }
    if button(&host, t(Lang::Ru, Key::AiAddMe)).is_some() {
        wrong.push("«Добавить мои данные» без профиля окна".to_owned());
    }
    let colors = Colors {
        scheme: "dark".to_owned(),
        vars: vec![("--surface".to_owned(), "#1a1b1c".to_owned())],
    };
    inbox.take(ToFrame::state(Lang::En, Date::new(2026, 11, 20), true, Some(colors)));
    if !wait_for(&host, t(Lang::En, Key::AiAbout)).await {
        wrong.push("рамка не перешла на язык окна".to_owned());
    }
    let root = window().document().and_then(|document| document.document_element()).expect("есть <html>");
    if root.get_attribute("lang").as_deref() != Some("en") {
        wrong.push(format!("у <html> язык {:?}", root.get_attribute("lang")));
    }
    let surface = root.unchecked_ref::<HtmlElement>().style().get_property_value("--surface").unwrap_or_default();
    if surface.trim() != "#1a1b1c" {
        wrong.push(format!("свой цвет окна у <html>: «{surface}»"));
    }
    if !press_when_ready(&host, t(Lang::En, Key::AiAddMe)).await {
        wrong.push("нет «Add my details» с профилем окна".to_owned());
    }
    if !until(|| said(&FromFrame::WantMe)).await {
        wrong.push("окну не сказано «Добавить мои данные»".to_owned());
    }
    inbox.take(ToFrame::Me {
        text: "Regime: turnover tax".to_owned(),
    });
    if !until(|| value(&host, t(Lang::En, Key::AiQuestion), "textarea").as_deref() == Some("Regime: turnover tax")).await {
        wrong.push("«мои данные» окна не встали в поле".to_owned());
    }
    plan(&format!(r#"{{"status":200,"chunks":{}}}"#, claude_chunks(true)));
    press_when_ready(&host, t(Lang::En, Key::AiAsk)).await;
    if !until(|| calls().contains("Today is 2026-11-20")).await {
        wrong.push("день окна не ушёл модели".to_owned());
    }
    // Цвета программы — снова свои у рамки.
    inbox.take(ToFrame::state(Lang::En, None, true, None));
    let surface = root.unchecked_ref::<HtmlElement>().style().get_property_value("--surface").unwrap_or_default();
    if !surface.is_empty() {
        wrong.push(format!("свой цвет остался после цветов программы: «{surface}»"));
    }
    drop(mounted);
    host.remove();
    unstub_fetch();
    look::put(None);
    look::set_lang(Lang::Ru);
    store::clear();
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// Ключ, подключённый в 0.0.158 ещё в окне, переезжает в рамку один раз:
/// рамка его запоминает и говорит окну стереть свой; а свой ключ рамки
/// важнее — второй от окна его не заменяет.
#[wasm_bindgen_test]
async fn a_key_from_the_window_moves_in_once_and_the_frames_own_stays() {
    bridge::reset();
    store::clear();
    let (host, inbox, mounted) = mount(Lang::Ru);
    let mut wrong = Vec::new();
    let old = Setup::new(Provider::Claude, "sk-ant-old-9999", "").expect("годится");
    inbox.take(ToFrame::Setup { text: old.to_text() });
    if store::stored().as_ref() != Some(&old) {
        wrong.push(format!("ключ окна не встал в рамку: {:?}", store::stored()));
    }
    if !said(&FromFrame::SetupTaken) {
        wrong.push("окну не сказано стереть свой ключ".to_owned());
    }
    let connected = fill(t(Lang::Ru, Key::AiConnected), &["Claude", "claude-opus-5-5", "9999"]);
    if !wait_for(&host, &connected).await {
        wrong.push(format!("нет строки «{connected}»"));
    }
    bridge::reset();
    let other = Setup::new(Provider::ChatGpt, "sk-proj-other-1111", "").expect("годится");
    inbox.take(ToFrame::Setup { text: other.to_text() });
    if store::stored().as_ref() != Some(&old) {
        wrong.push(format!("второй ключ окна заменил свой ключ рамки: {:?}", store::stored()));
    }
    if !said(&FromFrame::SetupTaken) {
        wrong.push("о втором ключе окну не сказано стереть".to_owned());
    }
    drop(mounted);
    host.remove();
    store::clear();
    assert!(wrong.is_empty(), "{wrong:#?}");
}
