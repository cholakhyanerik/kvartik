//! Язык и свои цвета окна — на `<html>` рамки (задача 111): рамка стоит
//! посреди экрана программы и выглядит его частью (Правило 14). Цвета
//! присылает окно (`frame::Colors`, проверены ядром), а ставятся они, как
//! в окне, через CSSOM (`style.setProperty`): атрибут `style` CSP не пускает
//! (CLAUDE.md, Правило 6). Без своих цветов рамка — в палитре программы
//! и теме устройства, как окно.

use std::cell::RefCell;

use kvartik_core::ai::frame::Colors;
use kvartik_core::i18n::Lang;
use wasm_bindgen::JsCast;
use web_sys::HtmlElement;

thread_local! {
    /// Какие свойства рамка поставила: новые цвета снимают прежние.
    static SET: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn root() -> Option<HtmlElement> {
    web_sys::window()?.document()?.document_element()?.dyn_into().ok()
}

/// Свои цвета окна — или цвета программы (`None`).
pub fn put(colors: Option<&Colors>) {
    let Some(root) = root() else {
        return;
    };
    let style = root.style();
    SET.with_borrow_mut(|set| {
        for name in set.drain(..) {
            let _ = style.remove_property(&name);
        }
        if let Some(colors) = colors {
            for (name, color) in &colors.vars {
                let _ = style.set_property(name, color);
                set.push(name.clone());
            }
            // С ней — полосы прокрутки и поля браузера той же темы, а фон
            // рамки и окна одной темы браузер не закрашивает (CSS Color
            // Adjust, `color-scheme` у рамки).
            let _ = style.set_property("color-scheme", &colors.scheme);
            set.push("color-scheme".to_owned());
        }
    });
}

/// Язык страницы — у `<html>`: по нему говорит плашка сбоя и читает диктор.
pub fn set_lang(lang: Lang) {
    if let Some(root) = root() {
        let _ = root.set_attribute("lang", lang.code());
    }
}
