//! Свои цвета на странице (задача 88): палитра человека — переменными на
//! `<html>` — и тема устройства. Что за палитра, решает ядро
//! (`kvartik_core::theme`), а здесь — только поставить её и снять.
//!
//! Переменные — через CSSOM (`style.setProperty`): атрибут `style`
//! в разметке CSP сайта не пускает, а CSSOM — пускает (CLAUDE.md,
//! Правило 6). Свойство на самом `<html>` сильнее обеих тем `tokens.css`,
//! и тёмная тема устройства палитру человека не перебивает — так и задумано
//! (решение человека 2026-11-09). С ней — `color-scheme`: полосы прокрутки
//! и поля дат браузер рисует в тон выбранной палитре, а не теме устройства.

use kvartik_core::theme::{Paint, Scheme};
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{CssStyleDeclaration, HtmlElement, MediaQueryList};

/// Вопрос о тёмной теме устройства.
fn dark_query() -> Option<MediaQueryList> {
    web_sys::window()?.match_media("(prefers-color-scheme: dark)").ok().flatten()
}

fn scheme_of(query: Option<&MediaQueryList>) -> Scheme {
    if query.is_some_and(MediaQueryList::matches) {
        Scheme::Dark
    } else {
        Scheme::Light
    }
}

/// Тема устройства сейчас (`prefers-color-scheme`). Не сказало — светлая:
/// так у стилей, тёмная там только по запросу.
pub fn device_scheme() -> Scheme {
    scheme_of(dark_query().as_ref())
}

/// Слежка за темой устройства: пока ручка жива, `changed` зовётся с новой
/// темой. Устройство меняет её само — вечером, по расписанию, — и экран,
/// показанный однажды, иначе остался бы с цветами прежней. Событие `change`
/// у `matchMedia`, а не опрос: в покое работы нет (Правило 1).
pub struct SchemeWatch {
    query: MediaQueryList,
    callback: Closure<dyn FnMut()>,
}

impl Drop for SchemeWatch {
    fn drop(&mut self) {
        let _ = self
            .query
            .remove_event_listener_with_callback("change", self.callback.as_ref().unchecked_ref());
    }
}

/// Следить за темой устройства; `None` — браузер о ней не говорит.
pub fn watch_scheme(changed: impl Fn(Scheme) + 'static) -> Option<SchemeWatch> {
    let query = dark_query()?;
    let callback = Closure::<dyn FnMut()>::new({
        let query = query.clone();
        move || changed(scheme_of(Some(&query)))
    });
    query
        .add_event_listener_with_callback("change", callback.as_ref().unchecked_ref())
        .ok()?;
    Some(SchemeWatch { query, callback })
}

/// Поставить палитру из записи устройства при открытии
/// (`kvartik_core::theme::painted`): пары «переменная — цвет» как есть, до
/// первой отрисовки.
pub fn put(scheme: Scheme, colors: &[(&str, &str)]) {
    let Some(style) = root_style() else {
        return;
    };
    for &(name, color) in colors {
        let _ = style.set_property(name, color);
    }
    let _ = style.set_property("color-scheme", scheme.code());
}

/// Кружок у курсора — атрибутом `data-cursor` на `<html>`: рисовать ли его,
/// решают стили (`style/base.css`), а не разметка, и выключенный не
/// рисуется вовсе.
pub fn show_cursor(on: bool) {
    let Some(root) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.document_element())
    else {
        return;
    };
    let _ = if on {
        root.set_attribute("data-cursor", "")
    } else {
        root.remove_attribute("data-cursor")
    };
}

/// Поставить палитру на страницу — или, `None`, снять её: окно снова в цветах
/// программы и в теме устройства. Переменные новой палитры ставятся поверх
/// прежних, без снятия: между ними окно не мигает цветами программы.
pub fn paint(palette: Option<&Paint>) {
    let Some(style) = root_style() else {
        return;
    };
    match palette {
        Some(palette) => {
            for (name, color) in palette.colors.iter() {
                let _ = style.set_property(name, &color.hex());
            }
            let _ = style.set_property("color-scheme", palette.scheme.code());
        }
        None => {
            // Снимаются все переменные `<html>`: других, кроме палитры, окно
            // ему не ставит (точку мыши `ui/motion.rs` пишет на кнопках
            // и карточках). С конца: снятое сдвигает номера следующих.
            for at in (0..style.length()).rev() {
                let name = style.item(at);
                if name.starts_with("--") {
                    let _ = style.remove_property(&name);
                }
            }
            let _ = style.remove_property("color-scheme");
        }
    }
}

/// Стиль самого `<html>`; `None` — документа нет.
fn root_style() -> Option<CssStyleDeclaration> {
    let root = web_sys::window()?.document()?.document_element()?;
    Some(root.unchecked_into::<HtmlElement>().style())
}
