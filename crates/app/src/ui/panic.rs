//! Сбой программы — словами, а не застывшей страницей (задача 48,
//! Правило 6).
//!
//! Паника в wasm32 — это `abort`: после хука wasm останавливается ловушкой
//! `unreachable`, состояние Rust брошено посреди работы, и звать wasm дальше
//! нельзя. Поэтому плашка строится внутри хука, синхронно и напрямую через
//! `web-sys`, без Leptos: его реактивная система посреди паники —
//! в неизвестном состоянии. По той же причине «Перезагрузить» — ссылка на
//! эту же страницу, а не кнопка с обработчиком: обработчик — вызов в wasm,
//! а по ссылке переходит сам браузер. Окно под плашкой — `inert`: каждое
//! нажатие в нём тоже было бы вызовом в остановленный wasm.
//!
//! Паника внутри самого хука — снова `unreachable` без слов, поэтому в нём
//! нет ничего, что может упасть: ответы DOM разбираются, а не
//! разворачиваются `unwrap`. Не вышло построить плашку — остаётся то, что
//! было до неё: причина в консоли.

use kvartik_core::i18n::{Key, Lang, t};
use wasm_bindgen::JsCast;
use web_sys::{Document, Element, HtmlElement};

/// `id` плашки: по нему вторая паника видит, что плашка уже есть.
const ID: &str = "panic";

/// Поставить хук. Первой строкой `main`, до любого кода, который может
/// упасть: иначе причина паники пропадёт, и в консоли останется одно
/// `unreachable`.
pub fn install() {
    std::panic::set_hook(Box::new(|info| {
        // Причина — в консоль: человеку она не нужна, а тому, кто чинит, —
        // нужна. Наружу она не уходит (Правило 13).
        console_error_panic_hook::hook(info);
        show();
    }));
}

/// На каких языках говорить: на языке окна — его ставит окно в `lang`
/// у `<html>`, — а код чужой или его нет — на всех трёх.
fn langs(code: Option<&str>) -> &'static [Lang] {
    match code.and_then(Lang::from_code) {
        Some(Lang::Am) => &[Lang::Am],
        Some(Lang::Ru) => &[Lang::Ru],
        Some(Lang::En) => &[Lang::En],
        None => &Lang::ALL,
    }
}

/// Показать плашку поверх окна.
fn show() {
    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        return;
    };
    // Паника бывает и не одна: браузер зовёт обработчики, которые wasm
    // поставил до остановки, и они падают снова.
    if document.get_element_by_id(ID).is_some() {
        return;
    }
    let Some(body) = document.body() else {
        return;
    };
    let code = document
        .document_element()
        .and_then(|html| html.get_attribute("lang"));
    let Some((banner, reload)) = banner(&document, langs(code.as_deref())) else {
        return;
    };
    let dead = body.children();
    for index in 0..dead.length() {
        if let Some(element) = dead.item(index) {
            let _ = element.set_attribute("inert", "");
        }
    }
    if body.append_child(&banner).is_ok() {
        // С клавиатуры перезагрузка — сразу по Enter.
        let _ = reload.unchecked_into::<HtmlElement>().focus();
    }
}

/// Плашка и её ссылка «Перезагрузить»; `None` — DOM отказал.
fn banner(document: &Document, langs: &[Lang]) -> Option<(Element, Element)> {
    let banner = document.create_element("div").ok()?;
    banner.set_id(ID);
    banner.set_class_name("panic");
    let card = document.create_element("div").ok()?;
    card.set_class_name("notice");
    for &lang in langs {
        let message = document.create_element("p").ok()?;
        // Диктор читает её сразу, не дожидаясь фокуса; цвет ошибки — тоже
        // по `role` (base.css).
        message.set_attribute("role", "alert").ok()?;
        message.set_attribute("lang", lang.code()).ok()?;
        message.set_text_content(Some(t(lang, Key::PanicMessage)));
        card.append_child(&message).ok()?;
    }
    let reload = document.create_element("a").ok()?;
    reload.set_class_name("button");
    // Пустой адрес — эта же страница без `#…`: переход грузит её заново,
    // как перезагрузка. С `#…` переход остался бы внутри страницы.
    reload.set_attribute("href", "").ok()?;
    // Подписи на трёх языках — без разделителей: их различает письмо,
    // а промежуток и перенос между ними даёт CSS (base.css).
    for &lang in langs {
        let label = document.create_element("span").ok()?;
        label.set_attribute("lang", lang.code()).ok()?;
        label.set_text_content(Some(t(lang, Key::Reload)));
        reload.append_child(&label).ok()?;
    }
    card.append_child(&reload).ok()?;
    banner.append_child(&card).ok()?;
    Some((banner, reload))
}

/// Только в отладочной сборке: невидимая кнопка, которая паникует, — чтобы
/// посмотреть плашку глазами. Нажимается из консоли:
/// `document.getElementById("debug-panic").click()`. Подписи у неё нет:
/// человек её не видит, а в выпускной сборке её нет вовсе.
#[cfg(debug_assertions)]
pub fn debug_trigger() -> impl leptos::IntoView {
    use leptos::prelude::*;

    view! {
        <button
            type="button"
            id="debug-panic"
            class="visually-hidden"
            tabindex="-1"
            aria-hidden="true"
            on:click=|_| panic!("проверка плашки сбоя (задача 48)")
        ></button>
    }
}

#[cfg(not(debug_assertions))]
pub fn debug_trigger() -> impl leptos::IntoView {}

#[cfg(test)]
mod tests {
    use super::*;

    /// Язык окна — один язык плашки; чужой код и его отсутствие — все три,
    /// а не армянский по умолчанию: человек, который не читает по-армянски,
    /// иначе не поймёт ни слова. `am` — амхарский, а не армянский.
    #[test]
    fn banner_speaks_the_window_language_or_all_three() {
        let mut wrong = Vec::new();
        let cases: [(Option<&str>, &[Lang]); 7] = [
            (Some("hy"), &[Lang::Am]),
            (Some("ru"), &[Lang::Ru]),
            (Some("en"), &[Lang::En]),
            (Some("am"), &Lang::ALL),
            (Some("ru-RU"), &Lang::ALL),
            (Some(""), &Lang::ALL),
            (None, &Lang::ALL),
        ];
        for (code, expected) in cases {
            if langs(code) != expected {
                wrong.push(format!("{code:?}: {:?}, а не {expected:?}", langs(code)));
            }
        }
        assert!(wrong.is_empty(), "не тот язык плашки: {wrong:#?}");
    }

    /// Слова плашки есть на каждом языке и переведены, а не скопированы:
    /// на всех трёх сразу плашка говорит, когда язык окна неизвестен.
    #[test]
    fn banner_words_exist_in_every_language() {
        let mut wrong = Vec::new();
        for key in [Key::PanicMessage, Key::Reload] {
            for lang in Lang::ALL {
                let text = t(lang, key);
                let copied = Lang::ALL
                    .into_iter()
                    .any(|other| other != lang && t(other, key) == text);
                if text.trim().is_empty() || copied {
                    wrong.push(format!("{key:?} ({lang:?}): «{text}»"));
                }
            }
        }
        assert!(wrong.is_empty(), "слова плашки: {wrong:#?}");
    }
}

/// Браузерный тест плашки: настоящий DOM в headless-браузере. Паники здесь
/// нет — она остановила бы весь прогон, — зовётся то, что зовёт хук.
#[cfg(all(test, target_arch = "wasm32"))]
mod browser {
    use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

    use super::*;

    wasm_bindgen_test_configure!(run_in_browser);

    /// Плашка одна, сколько бы паник ни было; говорит на языке страницы;
    /// окно под ней — `inert`; «Перезагрузить» ведёт на эту же страницу
    /// и стоит в фокусе.
    #[wasm_bindgen_test]
    fn banner_covers_the_dead_window_once() {
        let document = web_sys::window()
            .and_then(|window| window.document())
            .expect("есть документ");
        let html = document.document_element().expect("есть <html>");
        let body = document.body().expect("есть <body>");
        let page_lang = html.get_attribute("lang");
        let before = body.children().length();
        html.set_attribute("lang", "ru").expect("ставится");

        show();
        show();

        let banners = document.query_selector_all(".panic").expect("ищется");
        let banner = document.get_element_by_id(ID).expect("плашка есть");
        let paragraphs = banner.query_selector_all("p").expect("ищется");
        let message = banner.query_selector("p").expect("ищется");
        let reload = banner.query_selector("a").expect("ищется");
        let focused = document.active_element();
        let mut inert = 0;
        for index in 0..before {
            let child = body.children().item(index).expect("на месте");
            if child.has_attribute("inert") {
                inert += 1;
            }
            let _ = child.remove_attribute("inert");
        }
        banner.remove();
        match page_lang {
            Some(code) => html.set_attribute("lang", &code).expect("ставится"),
            None => html.remove_attribute("lang").expect("снимается"),
        }

        assert_eq!(banners.length(), 1, "плашек не одна");
        assert_eq!(paragraphs.length(), 1, "язык окна известен — абзац один");
        let message = message.expect("есть абзац");
        assert_eq!(message.get_attribute("lang").as_deref(), Some("ru"));
        assert_eq!(
            message.text_content().as_deref(),
            Some(t(Lang::Ru, Key::PanicMessage))
        );
        let reload = reload.expect("есть ссылка");
        assert_eq!(reload.get_attribute("href").as_deref(), Some(""));
        assert_eq!(reload.text_content().as_deref(), Some(t(Lang::Ru, Key::Reload)));
        assert_eq!(focused, Some(reload), "фокус не на «Перезагрузить»");
        assert_eq!(inert, before, "окно под плашкой не `inert`");
    }
}
