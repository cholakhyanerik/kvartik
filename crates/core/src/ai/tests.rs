//! Тесты ИИ-помощника (задача 102). Ответы поставщиков — того вида, что они
//! отдали вживую 2026-11-19 (curl и Chrome 158 с неверным ключом), и потоки
//! из их документации; поток режется на куски на каждой границе байта —
//! и посреди армянской буквы.

use serde_json::{Value, json};

use super::*;
use crate::model::activity::ActivityCode;
use crate::model::profile::Person;

fn setup(provider: Provider) -> Setup {
    Setup::new(provider, " key-0123456789abcdef ", "").expect("ключ годится")
}

fn day(text: &str) -> Date {
    Date::from_iso(text).expect("день")
}

/// Поток, поданный читателю кусками по `size` байт, — и чем он кончился.
fn read(provider: Provider, stream: &str, size: usize) -> (String, Option<End>) {
    let mut reader = Reader::new(provider);
    for chunk in stream.as_bytes().chunks(size) {
        reader.feed(chunk);
    }
    reader.finish();
    (reader.text().to_owned(), reader.end().cloned())
}

/// Поток при любой нарезке читается одинаково: провалы собираются, а не
/// обрывают тест на первом (Правило 2).
fn every_cut(provider: Provider, stream: &str, text: &str, end: &End) -> Vec<String> {
    (1..=stream.len())
        .filter_map(|size| {
            let got = read(provider, stream, size);
            (got != (text.to_owned(), Some(end.clone()))).then(|| format!("по {size} Б: {got:?}"))
        })
        .collect()
}

#[test]
fn providers_keep_their_codes_and_addresses() {
    let mut wrong = Vec::new();
    for provider in Provider::ALL {
        if Provider::from_code(provider.code()) != Some(provider) {
            wrong.push(format!("{provider:?}: код {}", provider.code()));
        }
        let url = provider.url();
        if !url.starts_with("https://") || url.ends_with('/') || url.contains(['?', '#', '*']) {
            wrong.push(format!("{provider:?}: адрес {url} — не точный путь для CSP"));
        }
        if Setup::new(provider, "key-0123456789abcdef", provider.model()).map(|s| s.model) != Ok(provider.model().to_owned()) {
            wrong.push(format!("{provider:?}: модель по умолчанию {} не принимается", provider.model()));
        }
        if !provider.key_page().starts_with("https://") {
            wrong.push(format!("{provider:?}: страница ключа {}", provider.key_page()));
        }
    }
    assert_eq!(Provider::from_code("openai"), None);
    assert!(wrong.is_empty(), "{wrong:#?}");
}

#[test]
fn setup_takes_a_clean_key_and_a_default_model() {
    let got = Setup::new(Provider::Claude, "  sk-ant-abc123\n", " ").expect("годится");
    assert_eq!(got.key, "sk-ant-abc123");
    assert_eq!(got.model, "claude-opus-5-5");
    assert_eq!(got.tail(), "c123");
    assert_eq!(Setup::new(Provider::Claude, "abc", "").map(|s| s.tail().to_owned()), Ok("abc".to_owned()));
    let cases = [
        ("", "", SetupProblem::NoKey),
        ("   ", "", SetupProblem::NoKey),
        ("sk-ant abc", "", SetupProblem::KeySpaces),
        ("sk-ant-\nabc", "", SetupProblem::KeySpaces),
        ("ключ123", "", SetupProblem::KeySpaces),
        (&"k".repeat(MAX_KEY_CHARS + 1), "", SetupProblem::KeyLong),
        ("sk-1", "gpt 6", SetupProblem::Model),
        ("sk-1", "модель", SetupProblem::Model),
        ("sk-1", &"m".repeat(MAX_MODEL_CHARS + 1), SetupProblem::Model),
    ];
    let mut wrong = Vec::new();
    for (key, model, want) in cases {
        let got = Setup::new(Provider::ChatGpt, key, model);
        if got != Err(want) {
            wrong.push(format!("«{key}», «{model}»: {got:?}, а не {want:?}"));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
    assert!(Setup::new(Provider::Gemini, "AIza-x", "models/gemini-3.8-flash:latest_1").is_ok());
    assert_eq!(Setup::new(Provider::Grok, &"k".repeat(MAX_KEY_CHARS), "").map(|s| s.key.len()), Ok(MAX_KEY_CHARS));
}

#[test]
fn setup_survives_the_device_record_and_rejects_a_broken_one() {
    let mine = Setup::new(Provider::Grok, r#"xai-"quoted"\slash"#, "grok-4.7").expect("годится");
    assert_eq!(Setup::parse(&mine.to_text()), Some(mine));
    for broken in [
        "",
        "{}",
        "не JSON",
        r#"{"provider":"openai","key":"sk-1","model":"gpt"}"#,
        r#"{"provider":"chatgpt","key":"sk 1","model":"gpt"}"#,
        r#"{"provider":"chatgpt","key":"","model":"gpt"}"#,
    ] {
        assert_eq!(Setup::parse(broken), None, "«{broken}» прочитан");
    }
    let empty_model = Setup::parse(r#"{"provider":"gemini","key":"AIza1","model":""}"#).expect("читается");
    assert_eq!(empty_model.model, Provider::Gemini.model());
}

#[test]
fn a_key_tells_whose_it_looks_like() {
    assert_eq!(Provider::guess("sk-ant-api03-x"), Some(Provider::Claude));
    assert_eq!(Provider::guess(" sk-proj-x"), Some(Provider::ChatGpt));
    assert_eq!(Provider::guess("AIzaSyX"), Some(Provider::Gemini));
    assert_eq!(Provider::guess("xai-x"), Some(Provider::Grok));
    assert_eq!(Provider::guess("abc"), None);
}

/// Тело запроса — JSON, разобранный обратно.
fn body(request: &Request) -> Value {
    serde_json::from_str(&request.body).expect("тело — JSON")
}

fn header<'a>(request: &'a Request, name: &str) -> Option<&'a str> {
    request.headers.iter().find(|(key, _)| *key == name).map(|(_, value)| value.as_str())
}

#[test]
fn claude_gets_the_messages_api_with_the_direct_browser_header() {
    let mine = setup(Provider::Claude);
    let turns = [Turn {
        question: "Когда платить налог?".to_owned(),
        answer: "До 20-го.".to_owned(),
    }];
    let request = request(&Ask {
        setup: &mine,
        lang: Lang::Ru,
        today: Some(day("2026-11-19")),
        turns: &turns,
        question: "А за IV квартал?",
    });
    assert_eq!(request.url, "https://api.anthropic.com/v1/messages");
    assert_eq!(header(&request, "x-api-key"), Some("key-0123456789abcdef"));
    assert_eq!(header(&request, "anthropic-version"), Some("2026-10-21"));
    assert_eq!(header(&request, "anthropic-dangerous-direct-browser-access"), Some("true"));
    assert_eq!(header(&request, "anthropic-beta"), Some("server-side-fallback-2026-07-01"));
    assert_eq!(header(&request, "authorization"), None);
    let body = body(&request);
    assert_eq!(body["model"], "claude-opus-5-5");
    assert_eq!(body["stream"], true);
    assert_eq!(body["max_tokens"], MAX_TOKENS);
    assert_eq!(body["fallbacks"], "default");
    assert_eq!(body["cache_control"]["type"], "ephemeral");
    for gone in ["temperature", "top_p", "top_k", "thinking"] {
        assert!(body.get(gone).is_none(), "{gone} в теле: с версией 2026-10-21 это 400");
    }
    let roles: Vec<_> = body["messages"].as_array().expect("массив").iter().map(|m| m["role"].clone()).collect();
    assert_eq!(roles, ["user", "assistant", "user"]);
    assert_eq!(body["messages"][2]["content"], "А за IV квартал?");
    let system = body["system"].as_str().expect("наставление строкой");
    for part in ["Today is 2026-11-19", "in Russian", "«Налог»", "«Числа для расчёта»", rules::armenia_text()] {
        assert!(system.contains(part), "в наставлении нет «{}»", part.chars().take(40).collect::<String>());
    }
}

#[test]
fn claude_without_server_fallback_gets_no_beta() {
    let mine = Setup::new(Provider::Claude, "sk-ant-1", "claude-haiku-5-5").expect("годится");
    let request = request(&Ask {
        setup: &mine,
        lang: Lang::Am,
        today: None,
        turns: &[],
        question: "Բարև",
    });
    assert_eq!(header(&request, "anthropic-beta"), None);
    let body = body(&request);
    assert!(body.get("fallbacks").is_none());
    let system = body["system"].as_str().expect("строкой");
    assert!(system.contains("Today is unknown") && system.contains("in Armenian") && system.contains("«Հարկ»"));
}

#[test]
fn the_others_get_chat_completions_with_a_bearer_key() {
    let mut wrong = Vec::new();
    for provider in [Provider::ChatGpt, Provider::Gemini, Provider::Grok] {
        let mine = setup(provider);
        let request = request(&Ask {
            setup: &mine,
            lang: Lang::En,
            today: Some(day("2026-11-19")),
            turns: &[],
            question: "When is the deadline?",
        });
        let body = body(&request);
        let messages = body["messages"].as_array().cloned().unwrap_or_default();
        let fine = request.url == provider.url()
            && header(&request, "authorization") == Some("Bearer key-0123456789abcdef")
            && header(&request, "x-api-key").is_none()
            && body["model"] == provider.model()
            && body["stream"] == true
            && body.get("max_tokens").is_none()
            && body.get("store").is_none()
            && messages.len() == 2
            && messages[0]["role"] == "system"
            && messages[0]["content"].as_str().is_some_and(|text| text.contains(rules::armenia_text()))
            && messages[1] == json!({ "role": "user", "content": "When is the deadline?" });
        if !fine {
            wrong.push(format!("{provider:?}: {} {:?}", request.url, request.headers));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// Поток Anthropic — как в его документации: начало сообщения, рассуждение
/// (у Opus 5.5 оно пустое), текст двумя кусками, причина остановки и конец.
fn claude_stream(stop: &str) -> String {
    [
        r#"event: message_start"#,
        r#"data: {"type":"message_start","message":{"id":"msg_1","type":"message","role":"assistant","content":[]}}"#,
        "",
        r#"event: content_block_start"#,
        r#"data: {"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}"#,
        "",
        r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":""}}"#,
        "",
        r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"EqQ"}}"#,
        "",
        r#"event: ping"#,
        r#"data: {"type": "ping"}"#,
        "",
        r#"event: content_block_delta"#,
        r#"data: {"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"Հարկը՝ "}}"#,
        "",
        r#"event: content_block_delta"#,
        r#"data: {"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"до 20-го \"числа\"."}}"#,
        "",
        r#"event: message_delta"#,
        &format!(r#"data: {{"type":"message_delta","delta":{{"stop_reason":"{stop}"}},"usage":{{"output_tokens":12}}}}"#),
        "",
        r#"event: message_stop"#,
        r#"data: {"type":"message_stop"}"#,
        "",
    ]
    .join("\n")
}

#[test]
fn claude_stream_reads_the_same_however_it_is_cut() {
    let text = "Հարկը՝ до 20-го \"числа\".";
    let mut wrong = every_cut(Provider::Claude, &claude_stream("end_turn"), text, &End::Done);
    wrong.extend(every_cut(Provider::Claude, &claude_stream("max_tokens"), text, &End::Long));
    wrong.extend(every_cut(Provider::Claude, &claude_stream("refusal"), text, &End::Refused));
    wrong.extend(every_cut(Provider::Claude, &claude_stream("end_turn").replace('\n', "\r\n"), text, &End::Done));
    assert!(wrong.is_empty(), "{wrong:#?}");
}

#[test]
fn claude_stream_without_its_end_is_cut_or_ends_by_the_stop_reason() {
    let full = claude_stream("end_turn");
    let no_stop = &full[..full.find("event: message_delta").expect("есть")];
    assert_eq!(read(Provider::Claude, no_stop, 64).1, Some(End::Cut));
    let no_last = &full[..full.find("event: message_stop").expect("есть")];
    assert_eq!(read(Provider::Claude, no_last, 64).1, Some(End::Done));
    // Последняя строка без переноса — тоже строка.
    let tail = full.trim_end();
    assert_eq!(read(Provider::Claude, tail, 7).1, Some(End::Done));
    // Оборванная посреди строки — не «ответ не того вида», а обрыв: текст —
    // что успел прийти (ревью задачи 102).
    let cut_at = full.find(r#""text":"до 20-го"#).expect("есть") + 10;
    assert_eq!(read(Provider::Claude, &full[..cut_at], 16), ("Հարկը՝ ".to_owned(), Some(End::Cut)));
}

#[test]
fn claude_error_in_the_stream_names_the_fault() {
    let stream = concat!(
        "event: content_block_delta\n",
        r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Нач"}}"#,
        "\n\nevent: error\n",
        r#"data: {"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#,
        "\n\n",
        r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"после конца"}}"#,
        "\n\n",
    );
    let (text, end) = read(Provider::Claude, stream, 5);
    assert_eq!(text, "Нач");
    assert_eq!(
        end,
        Some(End::Failed(Failure {
            fault: Fault::Busy,
            status: None,
            said: Some("Overloaded".to_owned()),
        }))
    );
}

/// Поток Chat Completions — как у OpenAI: роль, текст кусками, причина
/// остановки отдельным куском и `[DONE]`.
fn chat_stream(reason: &str, done: bool) -> String {
    let mut lines = vec![
        r#"data: {"id":"c1","object":"chat.completion.chunk","choices":[{"index":0,"delta":{"role":"assistant","content":""},"finish_reason":null}]}"#.to_owned(),
        String::new(),
        r#"data: {"id":"c1","choices":[{"index":0,"delta":{"content":"Срок — "},"finish_reason":null}]}"#.to_owned(),
        String::new(),
        r#"data: {"id":"c1","choices":[{"index":0,"delta":{"content":"20 января. Ժամկետ"},"finish_reason":null}]}"#.to_owned(),
        String::new(),
        format!(r#"data: {{"id":"c1","choices":[{{"index":0,"delta":{{}},"finish_reason":"{reason}"}}],"usage":null}}"#),
        String::new(),
    ];
    if done {
        lines.push("data: [DONE]".to_owned());
        lines.push(String::new());
    }
    lines.join("\n")
}

#[test]
fn chat_stream_reads_the_same_however_it_is_cut() {
    let text = "Срок — 20 января. Ժամկետ";
    let mut wrong = Vec::new();
    for provider in [Provider::ChatGpt, Provider::Gemini, Provider::Grok] {
        wrong.extend(every_cut(provider, &chat_stream("stop", true), text, &End::Done));
    }
    wrong.extend(every_cut(Provider::ChatGpt, &chat_stream("length", true), text, &End::Long));
    wrong.extend(every_cut(Provider::Gemini, &chat_stream("content_filter", true), text, &End::Refused));
    // Без `[DONE]` — по причине остановки, а без неё — оборвался.
    wrong.extend(every_cut(Provider::Grok, &chat_stream("stop", false), text, &End::Done));
    assert!(wrong.is_empty(), "{wrong:#?}");
    let full = chat_stream("stop", false);
    let no_reason = &full[..full.find(r#"data: {"id":"c1","choices":[{"index":0,"delta":{},"#).expect("есть")];
    assert_eq!(read(Provider::ChatGpt, no_reason, 32), (text.to_owned(), Some(End::Cut)));
}

#[test]
fn chat_error_in_the_stream_names_the_fault() {
    let stream = concat!(
        r#"data: {"choices":[{"delta":{"content":"Да"}}]}"#,
        "\n\n",
        r#"data: {"error":{"message":"Rate limit reached for gpt-6.2-sol","type":"requests","code":"rate_limit_exceeded"}}"#,
        "\n\n",
    );
    let (text, end) = read(Provider::ChatGpt, stream, 9);
    assert_eq!(text, "Да");
    assert_eq!(
        end,
        Some(End::Failed(Failure {
            fault: Fault::Limit,
            status: None,
            said: Some("Rate limit reached for gpt-6.2-sol".to_owned()),
        }))
    );
}

#[test]
fn stream_of_another_kind_is_unreadable_and_bounded() {
    let unreadable = End::Failed(Failure::of(Fault::Unreadable));
    assert_eq!(read(Provider::ChatGpt, "data: <html>\n\n", 4).1, Some(unreadable.clone()));
    assert_eq!(read(Provider::Claude, "data: {\"text\":1}\n\n", 4).1, Some(unreadable.clone()));
    // Бесконечная строка без переноса не занимает всю память.
    let mut reader = Reader::new(Provider::Claude);
    let block = vec![b'x'; 64 * 1024];
    for _ in 0..(MAX_LINE_BYTES / block.len() + 2) {
        reader.feed(&block);
    }
    assert_eq!(reader.end(), Some(&unreadable));
    assert!(reader.line.len() <= MAX_LINE_BYTES);
    // Ответ длиннее предела кончается словами «дошёл до предела».
    let chunk = format!(r#"data: {{"choices":[{{"delta":{{"content":"{}"}}}}]}}"#, "а".repeat(10_000));
    let mut reader = Reader::new(Provider::Grok);
    for _ in 0..(MAX_ANSWER_BYTES / 20_000 + 2) {
        reader.feed(chunk.as_bytes());
        reader.feed(b"\n\n");
    }
    assert_eq!(reader.end(), Some(&End::Long));
    assert!(reader.text().len() <= MAX_ANSWER_BYTES);
    // Пустой поток — оборвался, а не «целиком».
    assert_eq!(read(Provider::Gemini, "", 1).1, Some(End::Cut));
}

#[test]
fn http_failures_name_what_happened() {
    // Ответы с неверным ключом — вживую, 2026-11-19 (curl, Origin: example.com).
    let anthropic = r#"{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"},"request_id":"req_011"}"#;
    let gemini = r#"[{
  "error": {
    "code": 400,
    "message": "Please pass a valid API key",
    "status": "INVALID_ARGUMENT"
  }
}
]"#;
    let gemini_native = r#"{"error":{"code":400,"message":"API key not valid. Please pass a valid API key.","status":"INVALID_ARGUMENT","details":[{"reason":"API_KEY_INVALID"}]}}"#;
    let xai = r#"{"code":"invalid-argument","error":"Incorrect API key provided. You can obtain an API key from https://console.x.ai."}"#;
    let openai = r#"{"error":{"message":"The model `gpt-9` does not exist or you do not have access to it.","type":"invalid_request_error","code":"model_not_found"}}"#;
    let long = format!(r#"{{"error":{{"message":"{}"}}}}"#, "слово ".repeat(100));
    let cases = [
        (401, anthropic, Fault::Key, Some("invalid x-api-key")),
        (400, gemini, Fault::Key, Some("Please pass a valid API key")),
        (400, gemini_native, Fault::Key, Some("API key not valid. Please pass a valid API key.")),
        (400, xai, Fault::Key, Some("Incorrect API key provided. You can obtain an API key from https://console.x.ai.")),
        (404, openai, Fault::Model, Some("The model `gpt-9` does not exist or you do not have access to it.")),
        (403, r#"{"error":{"message":"  Country,\n region not supported "}}"#, Fault::Denied, Some("Country, region not supported")),
        (429, r#"{"message":"Too many requests"}"#, Fault::Limit, Some("Too many requests")),
        (402, "", Fault::Limit, None),
        (529, r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#, Fault::Busy, Some("Overloaded")),
        (500, "<html>oops</html>", Fault::Busy, None),
        (400, r#"{"error":{"message":"messages: at least one message is required"}}"#, Fault::Request, Some("messages: at least one message is required")),
        (413, "", Fault::Request, None),
        (302, "", Fault::Other, None),
    ];
    let mut wrong = Vec::new();
    for (status, body, fault, said) in cases {
        let got = failure(status, body);
        let want = Failure {
            fault,
            status: Some(status),
            said: said.map(str::to_owned),
        };
        if got != want {
            wrong.push(format!("{status}: {got:?}, а не {want:?}"));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
    let cut = failure(400, &long).said.expect("слова");
    assert_eq!(cut.chars().count(), MAX_SAID_CHARS + 1);
    assert!(cut.ends_with('…'));
}

#[test]
fn plain_drops_markdown_that_models_write_anyway() {
    let cases = [
        ("## Срок\n**До 20-го** числа", "Срок\nДо 20-го числа"),
        ("#хэштег и # не заголовок", "#хэштег и # не заголовок"),
        ("###", ""),
        ("* первый\n  * второй\n- третий", "- первый\n  - второй\n- третий"),
        ("Код `62.01.0` и __важно__", "Код 62.01.0 и важно"),
        (
            "См. [ст. 254](https://www.arlis.am/hy/acts/230455) и [https://src.am](https://src.am).",
            "См. ст. 254 (https://www.arlis.am/hy/acts/230455) и https://src.am.",
        ),
        ("[не ссылка] и [пусто]() и [a](b c)", "[не ссылка] и [пусто]() и [a](b c)"),
        ("[a] [b](https://x.am)", "[a] b (https://x.am)"),
    ];
    let mut wrong = Vec::new();
    for (from, want) in cases {
        let got = plain(from);
        if got != want {
            wrong.push(format!("«{from}» — «{got}», а не «{want}»"));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

#[test]
fn links_are_only_to_known_sites() {
    use Piece::{Link, Text};
    let text = "Закон: https://www.arlis.am/hy/acts/230455. Форма — https://evil.example/x, а ՊԵԿ (https://src.am/am/news)";
    assert_eq!(
        pieces(text),
        [
            Text("Закон: "),
            Link("https://www.arlis.am/hy/acts/230455"),
            Text(". Форма — https://evil.example/x, а ՊԵԿ ("),
            Link("https://src.am/am/news"),
            Text(")"),
        ]
    );
    assert_eq!(pieces("https://kvartik.com/app/"), [Link("https://kvartik.com/app/")]);
    assert_eq!(pieces("«https://cba.am»"), [Text("«"), Link("https://cba.am"), Text("»")]);
    assert_eq!(pieces("https://arlis.am.evil.example/"), [Text("https://arlis.am.evil.example/")]);
    // С `логин:пароль@` браузер ведёт на сайт после `@` (стандарт URL) —
    // такие адреса только текстом (ревью задачи 102).
    for trap in [
        "https://file-online.taxservice.am:443@taxservice-am.example/login",
        "https://www.arlis.am:x@evil.example/hy/acts/230455",
        "https://kvartik.com:@evil.example",
        "https://src.am@evil.example",
    ] {
        assert_eq!(pieces(trap), [Text(trap)], "{trap}");
    }
    assert_eq!(pieces("https://arlis.am:443/hy/acts/1"), [Link("https://arlis.am:443/hy/acts/1")]);
    assert_eq!(pieces("https://arlis.am\\@evil.example"), [Link("https://arlis.am\\@evil.example")]);
    assert_eq!(pieces("https://"), [Text("https://")]);
    assert_eq!(pieces("http://arlis.am"), [Text("http://arlis.am")]);
    assert_eq!(pieces(""), []);
}

#[test]
fn about_me_says_regime_codes_registration_and_citizenship() {
    let today = day("2026-11-19");
    let profile = ProfileData {
        activities: vec![
            ActivityCode::parse("62.01.0").expect("код"),
            ActivityCode::parse("70.22.0").expect("код"),
        ],
        regime: Some(Regime::Turnover),
        person: Person {
            registered: Some(day("2026-11-06")),
            residency: Some(Residency::Citizen),
            ..Person::default()
        },
        ..ProfileData::default()
    };
    assert_eq!(
        about_me(Lang::Ru, &profile, today),
        "Мои данные в Kvartik:\n- налог с оборота\n- код 62.01.0\u{A0}— Высокие технологии из списка правительства\n\
         - код 70.22.0\u{A0}— Прочая деятельность\n- день регистрации ИП: 06.11.2026\n- гражданин Армении"
    );
    let micro = ProfileData {
        micro: [2026].into(),
        person: Person {
            residency: Some(Residency::ForeignWithoutStatus),
            ..Person::default()
        },
        ..ProfileData::default()
    };
    let words = about_me(Lang::En, &micro, today);
    assert!(words.contains("2026: micro-entrepreneurship") && words.contains("without a residence status"), "{words}");
    assert_eq!(
        about_me(Lang::Am, &ProfileData::default(), today),
        format!("{}\n- {}", t(Lang::Am, Key::AiMeTitle), t(Lang::Am, Key::AiMeNoRegime))
    );
    // Ни ՀՎՀՀ, ни имени — даже когда они в профиле есть.
    let named = ProfileData {
        tin: crate::model::tin::Tin::parse("58111423").ok(),
        ..profile
    };
    let words = about_me(Lang::Ru, &named, today);
    assert!(!words.contains("58111423"), "{words}");
}
