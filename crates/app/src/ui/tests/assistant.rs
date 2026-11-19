//! Браузерные тесты ИИ-помощника (задача 102): всё окно в настоящем DOM.
//! Поставщик — подменой `window.fetch`: запрос на `https://…` она
//! запоминает и отвечает по плану теста — потоком SSE, кодом ошибки или
//! обрывом сети, — а запросы окна к своему сайту отдаёт настоящему `fetch`.
//! CSP в раннере тестов нет: что он пускает четыре адреса и не пускает
//! других, проверяет `crates/app/tests/csp.rs` и проверка собранного сайта
//! (`tools/browser/check.mjs`, «ИИ-помощник»).

use js_sys::{Function, JSON, Reflect};
use kvartik_core::ai::{Provider, Setup};
use kvartik_core::i18n::{Key, Lang, fill, t};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_test::wasm_bindgen_test;
use web_sys::{Element, HtmlElement};

use super::{button, field, mount_at, sleep, type_into, wait_for};
use crate::store::device::Device;

/// Подмена `fetch`: план — `window.__kvartikAi` (`status`, `text`, `chunks`,
/// `hold` — не закрывать поток, `error` — обрыв сети), запросы —
/// `window.__kvartikAiCalls`, отмену потока отмечает `__kvartikAiCancelled`.
const STUB: &str = r#"
if (!window.__kvartikAiOriginal) window.__kvartikAiOriginal = window.fetch;
const original = window.__kvartikAiOriginal;
window.__kvartikAiCalls = [];
window.__kvartikAiCancelled = false;
window.fetch = function (url, init) {
  const plan = window.__kvartikAi;
  const address = String(url);
  if (!address.startsWith("https://")) return original.apply(this, arguments);
  // Без плана наружу не уходит ничего: тест не должен трогать настоящую сеть.
  if (!plan) return Promise.reject(new TypeError("нет плана подмены"));
  window.__kvartikAiCalls.push({ url: address, headers: (init && init.headers) || {}, body: (init && init.body) || "" });
  if (plan.error) return Promise.reject(new TypeError("Failed to fetch"));
  // Поставщик молчит: ответ не приходит вовсе — «Остановить» до заголовков.
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
  // Поставщик отвечает с опозданием — после того, как человек остановил.
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
    let encoded: Vec<String> = chunks.into_iter().map(|chunk| JSON::stringify(&chunk.into()).ok().and_then(|text| text.as_string()).unwrap_or_default()).collect();
    format!("[{}]", encoded.join(","))
}

/// Ввести текст в многострочное поле внутри подписи `label`.
fn type_area(host: &HtmlElement, label: &str, text: &str) -> bool {
    let Some(area) = labelled(host, label, "textarea") else {
        return false;
    };
    let _ = Reflect::set(&area, &"value".into(), &text.into());
    let event = web_sys::Event::new("input").expect("событие создаётся");
    area.dispatch_event(&event).is_ok()
}

/// Значение многострочного поля внутри подписи `label`.
fn area_value(host: &HtmlElement, label: &str) -> Option<String> {
    let area = labelled(host, label, "textarea")?;
    Reflect::get(&area, &"value".into()).ok()?.as_string()
}

fn labelled(host: &HtmlElement, label: &str, what: &str) -> Option<Element> {
    let labels = host.query_selector_all("#ask label").ok()?;
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

/// Нажать кнопку экрана помощника с подписью `label`; `false` — её нет.
fn press(host: &HtmlElement, label: &str) -> bool {
    let Some(scope) = host.query_selector("#ask").ok().flatten() else {
        return false;
    };
    match button(scope.unchecked_ref(), &[label]) {
        Some(button) => {
            button.click();
            true
        }
        None => false,
    }
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

/// Подождать, пока кнопка с подписью `label` станет нажимаемой, и нажать:
/// после ввода в поле привязка Leptos включает кнопку своей задачей, и
/// нажатие раньше ушло бы в выключенную — без следа (ревью задачи 102).
async fn press_when_ready(host: &HtmlElement, label: &str) -> bool {
    let ready = || {
        host.query_selector("#ask")
            .ok()
            .flatten()
            .and_then(|scope| button(scope.unchecked_ref(), &[label]))
            .is_some_and(|button| !button.has_attribute("disabled"))
    };
    until(ready).await && press(host, label)
}

/// Ключ подключается, ответ приходит потоком и встаёт в разговор со ссылкой
/// на закон, в запросе — ключ, версия API, заголовок прямого доступа
/// и вопрос; «Новый разговор» стирает разговор, а «Убрать ключ» — запись
/// устройства.
#[wasm_bindgen_test]
async fn a_key_is_connected_an_answer_streams_in_and_the_key_is_removed() {
    let lang = Lang::Ru;
    let device = Device::new();
    device.set_ui_lang(lang);
    device.clear_ai();
    stub_fetch();
    let (host, mounted) = mount_at("#/ask");
    let mut wrong = Vec::new();
    if !wait_for(&host, t(lang, Key::AiAbout)).await {
        wrong.push("экран помощника не встал".to_owned());
    }
    match field(&host, t(lang, Key::AiKey), 0) {
        Some(input) => type_into(&input, "sk-ant-test-0123abcd"),
        None => wrong.push("нет поля ключа".to_owned()),
    }
    if !press(&host, t(lang, Key::AiSave)) {
        wrong.push("нет «Сохранить ключ»".to_owned());
    }
    let connected = fill(t(lang, Key::AiConnected), &["Claude", "claude-opus-5-5", "abcd"]);
    if !wait_for(&host, &connected).await {
        wrong.push(format!("нет строки «{connected}»"));
    }
    let saved = device.ai().as_deref().and_then(Setup::parse);
    if saved.as_ref().map(|setup| (setup.provider, setup.key.as_str())) != Some((Provider::Claude, "sk-ant-test-0123abcd")) {
        wrong.push(format!("на устройстве — {saved:?}"));
    }
    plan(&format!(r#"{{"status":200,"chunks":{}}}"#, claude_chunks(true)));
    if !type_area(&host, t(lang, Key::AiQuestion), "Когда платить налог за IV квартал?") {
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
    if area_value(&host, t(lang, Key::AiQuestion)).as_deref() != Some("") {
        wrong.push("поле вопроса после ответа не пустое".to_owned());
    }
    if !press(&host, t(lang, Key::AiNew)) {
        wrong.push("нет «Новый разговор»".to_owned());
    }
    if !until(|| host.query_selector("#ask .ai-talk").ok().flatten().is_none()).await {
        wrong.push("разговор не стёрся".to_owned());
    }
    if !press(&host, t(lang, Key::AiRemove)) {
        wrong.push("нет «Убрать ключ»".to_owned());
    }
    if !wait_for(&host, t(lang, Key::AiRemoved)).await {
        wrong.push("нет слов «Ключ убран»".to_owned());
    }
    if device.ai().is_some() {
        wrong.push("ключ остался на устройстве".to_owned());
    }
    if field(&host, t(lang, Key::AiKey), 0).is_none() {
        wrong.push("форма ключа не вернулась".to_owned());
    }
    drop(mounted);
    host.remove();
    unstub_fetch();
    device.clear_ai();
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// Отказ ключу, обрыв сети и остановка — словами: у отказа — и слова
/// поставщика, у обрыва ChatGPT — что неверный ключ OpenAI выглядит так же,
/// а остановленный ответ остаётся в разговоре недописанным, и поток
/// закрывается.
#[wasm_bindgen_test]
async fn a_refused_key_a_dropped_network_and_a_stop_are_said_in_words() {
    let lang = Lang::Ru;
    let device = Device::new();
    device.set_ui_lang(lang);
    device.set_ai(&Setup::new(Provider::Claude, "sk-ant-wrong-1", "").expect("годится").to_text());
    stub_fetch();
    let (host, mounted) = mount_at("#/ask");
    let mut wrong = Vec::new();
    if !wait_for(&host, t(lang, Key::AiWhatGoes).split("{}").nth(1).unwrap_or_default()).await {
        wrong.push("поле вопроса не встало".to_owned());
    }
    plan(r#"{"status":401,"text":"{\"type\":\"error\",\"error\":{\"type\":\"authentication_error\",\"message\":\"invalid x-api-key\"}}"}"#);
    type_area(&host, t(lang, Key::AiQuestion), "Вопрос");
    if !press_when_ready(&host, t(lang, Key::AiAsk)).await {
        wrong.push("нет «Спросить»".to_owned());
    }
    for words in [fill(t(lang, Key::AiFaultKey), &["Claude"]), fill(t(lang, Key::AiSaid), &["Claude", "invalid x-api-key"])] {
        if !wait_for(&host, &words).await {
            wrong.push(format!("нет слов «{words}»"));
        }
    }
    if area_value(&host, t(lang, Key::AiQuestion)).as_deref() != Some("Вопрос") {
        wrong.push("вопрос без ответа не вернулся в поле".to_owned());
    }

    // Обрыв сети у ChatGPT: и о сети, и о ключе OpenAI.
    device.set_ai(&Setup::new(Provider::ChatGpt, "sk-proj-1", "").expect("годится").to_text());
    plan(r#"{"error":true}"#);
    press_when_ready(&host, t(lang, Key::AiAsk)).await;
    for words in [fill(t(lang, Key::AiFaultNetwork), &["ChatGPT"]), t(lang, Key::AiFaultNetworkOpenAi).to_owned()] {
        if !wait_for(&host, &words).await {
            wrong.push(format!("нет слов «{words}»"));
        }
    }

    // Поток без конца — «Остановить»: недописанный ответ остаётся, поток
    // закрыт.
    device.set_ai(&Setup::new(Provider::Claude, "sk-ant-right-1", "").expect("годится").to_text());
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

    // Поставщик молчит — «Остановить» срабатывает сразу, до ответа: слова,
    // вопрос — обратно в поле, а запоздавший ответ окну уже не нужен.
    plan(r#"{"status":200,"pending":true}"#);
    type_area(&host, t(lang, Key::AiQuestion), "Вопрос без ответа");
    press_when_ready(&host, t(lang, Key::AiAsk)).await;
    if !wait_for(&host, t(lang, Key::AiWriting)).await {
        wrong.push("до ответа нет «ИИ отвечает…»".to_owned());
    }
    if !press_when_ready(&host, t(lang, Key::AiStop)).await {
        wrong.push("до ответа нет «Остановить»".to_owned());
    }
    if !until(|| area_value(&host, t(lang, Key::AiQuestion)).as_deref() == Some("Вопрос без ответа")).await {
        wrong.push("остановленный до ответа вопрос не вернулся в поле".to_owned());
    }
    if !wait_for(&host, t(lang, Key::AiAsk)).await {
        wrong.push("после остановки нет «Спросить»".to_owned());
    }

    // Остановили, а поставщик ответил потом — запоздавший ответ окну не нужен:
    // ход отцеплен, и в разговор он не ложится.
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
    let empty = [
        "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{}}\n\n",
        "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"}}\n\n",
        "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
    ];
    let encoded: Vec<String> = empty
        .iter()
        .map(|chunk| JSON::stringify(&(*chunk).into()).ok().and_then(|text| text.as_string()).unwrap_or_default())
        .collect();
    plan(&format!(r#"{{"status":200,"chunks":[{}]}}"#, encoded.join(",")));
    press_when_ready(&host, t(lang, Key::AiAsk)).await;
    if !wait_for(&host, t(lang, Key::AiEmpty)).await {
        wrong.push("о пустом ответе — ни слова".to_owned());
    }
    if !until(|| area_value(&host, t(lang, Key::AiQuestion)).as_deref() == Some("Вопрос без ответа")).await {
        wrong.push("вопрос без ответа не остался в поле".to_owned());
    }
    drop(mounted);
    host.remove();
    unstub_fetch();
    device.clear_ai();
    assert!(wrong.is_empty(), "{wrong:#?}");
}
