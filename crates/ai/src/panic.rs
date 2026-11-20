//! Сбой рамки — словами (как у окна, `crates/app/src/ui/panic.rs`): паника
//! в wasm — это `abort`, и застывшая рамка посреди экрана выглядела бы
//! программой, которая не отвечает. Плашка — напрямую через `web-sys`, без
//! Leptos и без `unwrap`, одна на все паники; «Перезагрузить» — ссылка на
//! эту же страницу, и перезагружается одна рамка, а не окно с его формами.
//!
//! И окну — что рамка не занята: иначе начатое в ней, которого уже нет,
//! держало бы новую версию окна до закрытия вкладки (`frame::FromFrame::Busy`).

use kvartik_core::i18n::{Key, Lang, t};
use web_sys::Document;

use crate::bridge;

/// `id` плашки: по нему вторая паника видит, что плашка уже есть.
const ID: &str = "panic";

/// Поставить хук — первой строкой `main`.
pub fn install() {
    std::panic::set_hook(Box::new(|info| {
        console_error_panic_hook::hook(info);
        show();
        bridge::release();
    }));
}

fn show() {
    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        return;
    };
    if document.get_element_by_id(ID).is_some() {
        return;
    }
    let Some(body) = document.body() else {
        return;
    };
    let lang = document
        .document_element()
        .and_then(|html| html.get_attribute("lang"))
        .as_deref()
        .and_then(Lang::from_code);
    let langs: &[Lang] = match lang {
        Some(Lang::Am) => &[Lang::Am],
        Some(Lang::Ru) => &[Lang::Ru],
        Some(Lang::En) => &[Lang::En],
        None => &Lang::ALL,
    };
    if let Some(card) = card(&document, langs) {
        body.set_text_content(None);
        let _ = body.append_child(&card);
    }
}

fn card(document: &Document, langs: &[Lang]) -> Option<web_sys::Element> {
    let card = document.create_element("div").ok()?;
    card.set_id(ID);
    card.set_class_name("notice");
    for &lang in langs {
        let message = document.create_element("p").ok()?;
        message.set_attribute("role", "alert").ok()?;
        message.set_attribute("lang", lang.code()).ok()?;
        message.set_text_content(Some(t(lang, Key::PanicMessage)));
        card.append_child(&message).ok()?;
    }
    let reload = document.create_element("a").ok()?;
    reload.set_class_name("button");
    reload.set_attribute("href", "").ok()?;
    for &lang in langs {
        let label = document.create_element("span").ok()?;
        label.set_attribute("lang", lang.code()).ok()?;
        label.set_text_content(Some(t(lang, Key::Reload)));
        reload.append_child(&label).ok()?;
    }
    card.append_child(&reload).ok()?;
    Some(card)
}
