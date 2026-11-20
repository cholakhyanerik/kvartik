//! Рамка помощника: пары происхождений, сообщения и их пределы. Проверки
//! циклом собирают провалы и выдают их в конце (CLAUDE.md, Правило 2).

use super::*;

#[test]
fn frame_of_the_window_is_its_pair_and_nothing_else() {
    let cases = [
        ("kvartik.com", Some("https://ai.kvartik.com")),
        ("task-111-ai-origin.kvartik.pages.dev", Some("https://task-111-ai-origin.kvartik-ai.pages.dev")),
        ("ci-check.kvartik.pages.dev", Some("https://ci-check.kvartik-ai.pages.dev")),
        ("fcdb2d82.kvartik.pages.dev", Some("https://fcdb2d82.kvartik-ai.pages.dev")),
        // Без ветки, с портом, с точкой в метке, чужой домен, сама рамка,
        // проверочный сервер — рамки нет.
        ("kvartik.pages.dev", None),
        ("kvartik.com:8443", None),
        ("www.kvartik.com", None),
        ("a.b.kvartik.pages.dev", None),
        ("-x.kvartik.pages.dev", None),
        ("x-.kvartik.pages.dev", None),
        ("X.kvartik.pages.dev", None),
        ("kvartik.com.evil.com", None),
        ("xkvartik.com", None),
        ("evil.com", None),
        ("ai.kvartik.com", None),
        ("127.0.0.1:8080", None),
        ("", None),
    ];
    let mut failures = Vec::new();
    for (host, want) in cases {
        let got = frame_origin(host);
        if got.as_deref() != want {
            failures.push(format!("{host}: {got:?}, а ждали {want:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn window_of_the_frame_is_its_pair_and_nothing_else() {
    let cases = [
        ("ai.kvartik.com", Some("https://kvartik.com")),
        ("task-111-ai-origin.kvartik-ai.pages.dev", Some("https://task-111-ai-origin.kvartik.pages.dev")),
        ("ci-check.kvartik-ai.pages.dev", Some("https://ci-check.kvartik.pages.dev")),
        ("kvartik-ai.pages.dev", None),
        ("ai.kvartik.com:443", None),
        ("kvartik.com", None),
        ("a.b.kvartik-ai.pages.dev", None),
        ("ai.kvartik.com.evil.com", None),
        // Своё окно — только у самой рамки, не у соседей по имени.
        ("xai.kvartik.com", None),
        ("evil.ai.kvartik.com", None),
        ("evil.kvartik-ai.pages.dev.evil.com", None),
        ("localhost:8080", None),
    ];
    let mut failures = Vec::new();
    for (host, want) in cases {
        let got = parent_origin(host);
        if got.as_deref() != want {
            failures.push(format!("{host}: {got:?}, а ждали {want:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn pairs_go_both_ways() {
    let mut failures = Vec::new();
    for site in ["kvartik.com", "ci-check.kvartik.pages.dev", "task-9.kvartik.pages.dev"] {
        let frame = frame_origin(site).unwrap_or_default();
        let host = frame.strip_prefix("https://").unwrap_or_default();
        let back = parent_origin(host);
        if back.as_deref() != Some(format!("https://{site}").as_str()) {
            failures.push(format!("{site} → {frame} → {back:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn colors() -> Colors {
    Colors {
        scheme: "dark".to_owned(),
        vars: vec![("--surface".to_owned(), "#1a1b1c".to_owned()), ("--accent-text".to_owned(), "#ffcc00".to_owned())],
    }
}

#[test]
fn state_goes_over_and_back() {
    let sent = ToFrame::state(Lang::Ru, Date::new(2026, 11, 20), true, Some(colors()));
    let text = sent.to_text();
    let got = ToFrame::parse(&text).and_then(|message| message.as_state());
    assert_eq!(
        got,
        Some(State {
            lang: Lang::Ru,
            today: Date::new(2026, 11, 20),
            me: true,
            colors: Some(colors()),
        }),
        "{text}"
    );
    // Часы окна врут — дня нет, и рамка скажет это модели.
    let no_day = ToFrame::parse(&ToFrame::state(Lang::Am, None, false, None).to_text()).and_then(|message| message.as_state());
    assert_eq!(no_day.map(|state| (state.lang, state.today)), Some((Lang::Am, None)));
}

#[test]
fn wire_format_is_the_contract() {
    // Договор между версиями: эти строки читает прежняя сторона.
    let cases = [
        (FromFrame::Ready.to_text(), r#"{"kind":"ready"}"#),
        (FromFrame::Busy { busy: true, talk: false }.to_text(), r#"{"kind":"busy","busy":true,"talk":false}"#),
        (FromFrame::Height { px: 640 }.to_text(), r#"{"kind":"height","px":640}"#),
        (FromFrame::WantMe.to_text(), r#"{"kind":"want_me"}"#),
        (
            FromFrame::Connected {
                provider: Some("gemini".to_owned()),
            }
            .to_text(),
            r#"{"kind":"connected","provider":"gemini"}"#,
        ),
        (FromFrame::SetupTaken.to_text(), r#"{"kind":"setup_taken"}"#),
        (ToFrame::Me { text: "ԱՁ".to_owned() }.to_text(), r#"{"kind":"me","text":"ԱՁ"}"#),
        (ToFrame::Setup { text: "{}".to_owned() }.to_text(), r#"{"kind":"setup","text":"{}"}"#),
        (
            ToFrame::state(Lang::En, Date::new(2026, 1, 2), false, None).to_text(),
            r#"{"kind":"state","lang":"en","today":"2026-01-02","me":false,"colors":null}"#,
        ),
    ];
    let mut failures = Vec::new();
    for (got, want) in cases {
        if got != want {
            failures.push(format!("{got}, а ждали {want}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn messages_of_a_newer_version_are_read_or_skipped() {
    // Незнакомый вид — `Unknown`, незнакомое поле — мимо, недостающее
    // новое поле — по умолчанию.
    assert_eq!(ToFrame::parse(r#"{"kind":"theme","x":1}"#), Some(ToFrame::Unknown));
    assert_eq!(FromFrame::parse(r#"{"kind":"scroll","to":3}"#), Some(FromFrame::Unknown));
    let state = ToFrame::parse(r#"{"kind":"state","lang":"hy","font":"big"}"#).and_then(|message| message.as_state());
    assert_eq!(
        state,
        Some(State {
            lang: Lang::Am,
            today: None,
            me: false,
            colors: None,
        })
    );
    assert_eq!(
        FromFrame::parse(r#"{"kind":"connected"}"#),
        Some(FromFrame::Connected { provider: None })
    );
    assert_eq!(
        FromFrame::parse(r#"{"kind":"busy","busy":false,"why":"typing"}"#),
        Some(FromFrame::Busy { busy: false, talk: false })
    );
}

#[test]
fn foreign_input_is_bounded_and_checked() {
    let long = "я".repeat(MAX_QUESTION_CHARS + 1);
    let many: Vec<(String, String)> = (0..65).map(|index| (format!("--c{index}"), "#000000".to_owned())).collect();
    let with = |vars: Vec<(&str, &str)>| {
        ToFrame::State {
            lang: "ru".to_owned(),
            today: None,
            me: false,
            colors: Some(Colors {
                scheme: "light".to_owned(),
                vars: vars.into_iter().map(|(name, color)| (name.to_owned(), color.to_owned())).collect(),
            }),
        }
        .to_text()
    };
    let refused: Vec<(&str, String)> = vec![
        ("не JSON", "kind=state".to_owned()),
        ("длиннее предела", format!(r#"{{"kind":"me","text":"{}"}}"#, "a".repeat(MAX_MESSAGE_BYTES))),
        // Годное во всём, кроме длины: разбор и не начинается.
        ("годное, но длиннее предела", format!(r#"{{"kind":"state","lang":"ru"{}}}"#, " ".repeat(MAX_MESSAGE_BYTES))),
        ("чужой язык", r#"{"kind":"state","lang":"de"}"#.to_owned()),
        ("не день", r#"{"kind":"state","lang":"ru","today":"2026-13-01"}"#.to_owned()),
        ("«мои данные» длиннее вопроса", ToFrame::Me { text: long }.to_text()),
        ("запись ключа длиннее предела", ToFrame::Setup { text: "k".repeat(MAX_SETUP_BYTES + 1) }.to_text()),
        ("цвет словом", with(vec![("--surface", "red")])),
        ("цвет короче", with(vec![("--surface", "#12345")])),
        ("цвет с альфой", with(vec![("--surface", "#12345678")])),
        ("имя без --", with(vec![("surface", "#123456")])),
        ("имя с заглавной", with(vec![("--Surface", "#123456")])),
        ("имя со скобкой", with(vec![("--x;}", "#123456")])),
        ("пустое имя", with(vec![("--", "#123456")])),
        (
            "чужая тема",
            ToFrame::State {
                lang: "ru".to_owned(),
                today: None,
                me: false,
                colors: Some(Colors {
                    scheme: "sepia".to_owned(),
                    vars: Vec::new(),
                }),
            }
            .to_text(),
        ),
        (
            "цветов больше предела",
            ToFrame::State {
                lang: "ru".to_owned(),
                today: None,
                me: false,
                colors: Some(Colors {
                    scheme: "dark".to_owned(),
                    vars: many,
                }),
            }
            .to_text(),
        ),
    ];
    let mut failures = Vec::new();
    for (what, text) in &refused {
        if let Some(message) = ToFrame::parse(text) {
            failures.push(format!("{what}: принято {message:?}"));
        }
    }
    for (what, text) in [
        ("высота больше предела", r#"{"kind":"height","px":200001}"#),
        ("отрицательная высота", r#"{"kind":"height","px":-1}"#),
        ("чужой поставщик", r#"{"kind":"connected","provider":"mistral"}"#),
        ("без вида", r#"{"busy":true}"#),
    ] {
        if let Some(message) = FromFrame::parse(text) {
            failures.push(format!("{what}: принято {message:?}"));
        }
    }
    let padded = format!(r#"{{"kind":"ready"{}}}"#, " ".repeat(MAX_MESSAGE_BYTES));
    if let Some(message) = FromFrame::parse(&padded) {
        failures.push(format!("годное от рамки, но длиннее предела: принято {message:?}"));
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    // А годное — принято.
    assert!(ToFrame::parse(&with(vec![("--surface", "#0a0B0c"), ("--on-fill-waiting", "#ffffff")])).is_some());
    assert_eq!(FromFrame::parse(r#"{"kind":"height","px":200000}"#), Some(FromFrame::Height { px: 200_000 }));
}
