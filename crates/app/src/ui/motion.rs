//! Движение окна (задача 86): переход между экранами и то, что следует за
//! мышью. Как это выглядит, решает оформление (`style/base.css`, раздел
//! «Движение»), а здесь — когда начать переход и где мышь.
//!
//! Переход экрана — View Transitions (`platform::motion`): браузер снимает
//! прежний экран, окно меняет разметку, и снимок растворяется поверх нового
//! экрана — живого, чтобы нажатия шли ему сразу. Разметку Leptos
//! перерисовывает не сразу после сигнала, а следующей задачей, поэтому
//! обещание перехода ждёт её (`tick`): без этого браузер снял бы новое
//! состояние документа и вкладки до перерисовки. Кто выключил анимацию
//! в системе, видит смену сразу, а чей браузер переходов не знает — только
//! въезд нового экрана.
//!
//! За мышью — три слушателя на всё окно, а не на каждую кнопку: кнопке —
//! точка, где в неё вошли и откуда ушли (заливка), и тяга к мыши (магнит),
//! карточке `data-tilt` — точка мыши (наклон и блик), кружку у курсора —
//! место и то, над чем он. Всё это — переменные CSS на самом элементе, а
//! рисовать ли, решают стили: без мыши и без движения они не рисуют ничего.
//! Касание пальцем и пером не в счёт. Работа — только пока мышь движется
//! (Правило 1). То же делает `site/landing.js` у лендинга.

use std::cell::RefCell;
use std::rc::Rc;

use leptos::ev;
use leptos::html;
use leptos::prelude::*;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{Element, HtmlElement, Node, PointerEvent};

use super::route::Screen;
use crate::platform;

/// Над чем кружок у курсора растёт (на это нажимают) и над чем его нет:
/// в поле целятся текстовым курсором.
const PRESS: &str = "a[href], button, label, select, summary";
const TYPING: &str = "input:not([type=checkbox], [type=radio]), textarea";
/// Кнопка: заливка и магнит. Карточка: наклон.
const BUTTON: &str = "button, .button";
const CARD: &str = "[data-tilt]";

/// Имя перехода у строки документа и у заголовка его карточки
/// (`style/base.css`) и у прежнего экрана.
const DOCUMENT: &str = "doc";
const SCREEN: &str = "screen";

thread_local! {
    /// Строка документа, которой дано имя перехода после него: следующий
    /// переход снимет его первым — два элемента с одним именем браузер
    /// не ведёт вовсе.
    static NAMED: RefCell<Option<HtmlElement>> = const { RefCell::new(None) };
}

/// Сменить экран `from` на `to`: `update` меняет разметку. Снимок —
/// у прежнего экрана: он растворяется поверх нового, живого, который
/// въезжает своей анимацией (`style/base.css`). Документ перетекает из своей
/// строки в списке в заголовок карточки — ему имя даёт оформление, пока
/// карточка открыта, — и обратно, в строку.
pub fn screen(from: Screen, to: Screen, update: impl FnOnce() + 'static) {
    if !platform::motion::allowed() {
        update();
        return;
    }
    NAMED.with(|named| {
        if let Some(row) = named.borrow_mut().take() {
            name(&row, None);
        }
    });
    let opening = matches!(to, Screen::Document(_)) && !matches!(from, Screen::Document(_));
    let before = if opening { row(to) } else { None };
    let after = match (from, to) {
        (Screen::Document(_), Screen::Document(_)) => None,
        (Screen::Document(_), _) => Some(from),
        _ => None,
    };
    let page = by_id(from.id());
    if let Some(page) = &page {
        name(page, Some(SCREEN));
    }
    if let Some(row) = &before {
        name(row, Some(DOCUMENT));
    }
    platform::motion::transition(move || {
        for named in [&page, &before].into_iter().flatten() {
            name(named, None);
        }
        // Новый экран въезжает с первого перехода, а не при запуске:
        // метка — уже после снимка прежнего, иначе он въезжал бы сам.
        if let Some(root) = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.document_element())
        {
            let _ = root.set_attribute("data-motion", "");
        }
        update();
        js_sys::futures::future_to_promise(async move {
            leptos::task::tick().await;
            if let Some(row) = after.and_then(row) {
                name(&row, Some(DOCUMENT));
                NAMED.with(|named| *named.borrow_mut() = Some(row));
            }
            Ok(JsValue::UNDEFINED)
        })
    });
}

/// Строка документа в списке.
fn row(document: Screen) -> Option<HtmlElement> {
    let selector = format!("#documents a.entry[href=\"{}\"]", document.href());
    web_sys::window()?
        .document()?
        .query_selector(&selector)
        .ok()
        .flatten()
        .map(JsCast::unchecked_into)
}

/// Страница экрана по её `id`.
fn by_id(id: &str) -> Option<HtmlElement> {
    web_sys::window()?
        .document()?
        .get_element_by_id(id)
        .map(JsCast::unchecked_into)
}

fn name(element: &HtmlElement, name: Option<&str>) {
    let style = element.style();
    let _ = match name {
        Some(name) => style.set_property("view-transition-name", name),
        None => style.remove_property("view-transition-name").map(drop),
    };
}

/// Кнопка или карточка под мышью — с местом на странице, взятым при входе:
/// кнопка-магнит сама едет к мыши, и её нынешнее место тянуло бы её
/// дальше.
struct Spot {
    element: HtmlElement,
    left: f64,
    top: f64,
    width: f64,
    height: f64,
}

impl Spot {
    fn of(element: Element) -> Self {
        let box_ = element.get_bounding_client_rect();
        let (x, y) = web_sys::window().map_or((0.0, 0.0), |window| {
            (window.scroll_x().unwrap_or(0.0), window.scroll_y().unwrap_or(0.0))
        });
        Self {
            element: element.unchecked_into(),
            left: box_.left() + x,
            top: box_.top() + y,
            width: box_.width(),
            height: box_.height(),
        }
    }

    /// Точка мыши в долях ширины и высоты, от 0 до 1.
    fn share(&self, event: &PointerEvent) -> (f64, f64) {
        let along = |at: i32, start: f64, size: f64| {
            if size > 0.0 {
                ((f64::from(at) - start) / size).clamp(0.0, 1.0)
            } else {
                0.5
            }
        };
        (
            along(event.page_x(), self.left, self.width),
            along(event.page_y(), self.top, self.height),
        )
    }

    fn write(&self, names: [&str; 2], (x, y): (f64, f64)) {
        let style = self.element.style();
        let _ = style.set_property(names[0], &fraction(x));
        let _ = style.set_property(names[1], &fraction(y));
    }

    fn holds(&self, node: Option<&Node>) -> bool {
        self.element.contains(node)
    }
}

const POINTER: [&str; 2] = ["--pointer-x", "--pointer-y"];
const PULL: [&str; 2] = ["--pull-x", "--pull-y"];

/// Доля от −1 до 1 тремя знаками — целыми числами: запись `f64` тянула бы
/// в главный wasm код, который ему больше не нужен, а трёх знаков CSS
/// хватает.
fn fraction(value: f64) -> String {
    let thousandths = (value.clamp(-1.0, 1.0) * 1000.0).round() as i32;
    let sign = if thousandths < 0 { "-" } else { "" };
    let whole = thousandths.unsigned_abs();
    format!("{sign}{}.{:03}", whole / 1000, whole % 1000)
}

fn closest(target: &Element, selector: &str) -> Option<Element> {
    target.closest(selector).ok().flatten()
}

/// Что под мышью сейчас.
#[derive(Default)]
struct Hover {
    button: Option<Spot>,
    card: Option<Spot>,
}

/// Следить за мышью, пока окно открыто. `cursor` — кружок у курсора.
pub fn follow(cursor: NodeRef<html::Div>) {
    let hover = Rc::new(RefCell::new(Hover::default()));
    let mark = move |over: Option<&str>| {
        if let Some(cursor) = cursor.get_untracked() {
            let _ = match over {
                Some(over) => cursor.set_attribute("data-over", over),
                None => cursor.remove_attribute("data-over"),
            };
        }
    };

    let entered = {
        let hover = Rc::clone(&hover);
        window_event_listener(ev::pointerover, move |event: PointerEvent| {
            if event.pointer_type() != "mouse" {
                mark(None);
                return;
            }
            let Some(target) = event.target().and_then(|target| target.dyn_into::<Element>().ok()) else {
                return;
            };
            mark(Some(if closest(&target, TYPING).is_some() {
                "text"
            } else if closest(&target, PRESS).is_some() {
                "press"
            } else {
                ""
            }));
            let mut hover = hover.borrow_mut();
            if let Some(button) = closest(&target, BUTTON)
                && !hover.button.as_ref().is_some_and(|spot| *spot.element == button)
            {
                let spot = Spot::of(button);
                spot.write(POINTER, spot.share(&event));
                hover.button = Some(spot);
            }
            if let Some(card) = closest(&target, CARD)
                && !hover.card.as_ref().is_some_and(|spot| *spot.element == card)
            {
                hover.card = Some(Spot::of(card));
            }
        })
    };

    let left = {
        let hover = Rc::clone(&hover);
        window_event_listener(ev::pointerout, move |event: PointerEvent| {
            if event.pointer_type() != "mouse" {
                return;
            }
            let next = event.related_target().and_then(|next| next.dyn_into::<Node>().ok());
            // Мышь ушла из окна — кружок прячется до её возвращения.
            if next.is_none() {
                mark(None);
            }
            let mut hover = hover.borrow_mut();
            // С кнопки — заливка сжимается в точку ухода, кнопка — на место.
            if let Some(spot) = hover.button.take_if(|spot| !spot.holds(next.as_ref())) {
                spot.write(POINTER, spot.share(&event));
                spot.write(PULL, (0.0, 0.0));
            }
            // С карточки — она выпрямляется.
            if let Some(spot) = hover.card.take_if(|spot| !spot.holds(next.as_ref())) {
                spot.write(POINTER, (0.5, 0.5));
            }
        })
    };

    let moved = window_event_listener(ev::pointermove, move |event: PointerEvent| {
        if event.pointer_type() != "mouse" {
            return;
        }
        if let Some(cursor) = cursor.get_untracked() {
            let place = format!("{}px {}px", event.client_x(), event.client_y());
            // `style` у `HtmlElement`, а не одноимённый метод Leptos для разметки.
            let _ = HtmlElement::style(&cursor).set_property("translate", &place);
        }
        let hover = hover.borrow();
        // Тяга магнита — от −1 до 1: от середины кнопки к мыши.
        if let Some(spot) = &hover.button {
            let (x, y) = spot.share(&event);
            spot.write(PULL, (x * 2.0 - 1.0, y * 2.0 - 1.0));
        }
        if let Some(spot) = &hover.card {
            spot.write(POINTER, spot.share(&event));
        }
    });

    // Браузерные тесты монтируют окно много раз: слушатели уходят с ним.
    on_cleanup(move || {
        entered.remove();
        left.remove();
        moved.remove();
    });
}

#[cfg(test)]
mod tests {
    use super::fraction;

    /// Доля пишется тремя знаками и со знаком, а выход за −1…1 срезается:
    /// CSS читает её числом.
    #[test]
    fn fractions_are_written_for_css() {
        let mut wrong = Vec::new();
        for (value, expected) in [
            (0.0, "0.000"),
            (0.5, "0.500"),
            (1.0, "1.000"),
            (0.1234, "0.123"),
            (0.9996, "1.000"),
            (-0.25, "-0.250"),
            (-1.0, "-1.000"),
            (2.0, "1.000"),
            (-3.5, "-1.000"),
            (-0.0004, "0.000"),
        ] {
            let got = fraction(value);
            if got != expected {
                wrong.push(format!("{value}: {got}, а не {expected}"));
            }
        }
        assert!(wrong.is_empty(), "{wrong:#?}");
    }
}
