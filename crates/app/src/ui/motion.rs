//! Движение окна (задача 86): переход между экранами и то, что следует за
//! мышью. Как это выглядит, решает оформление (`style/base.css`, раздел
//! «Движение»), а здесь — когда начать переход и где мышь.
//!
//! Переход экрана — View Transitions (`platform::motion`): браузер снимает
//! прежний экран, окно меняет разметку, и снимок растворяется поверх нового
//! экрана — живого, чтобы нажатия шли ему сразу. Разметку Leptos
//! перерисовывает не сразу после сигнала, а следующей задачей, поэтому
//! обещание перехода ждёт её (`tick`): без этого браузер снял бы новое
//! состояние документа и вкладки до перерисовки. Снимок браузер рисует над
//! всей страницей, а меню на телефоне лежит поверх экрана — поэтому снятое
//! обрезается по его верхнему краю (`clear_of_menu`). Кто выключил анимацию
//! в системе, видит смену сразу, а чей браузер переходов не знает — только
//! въезд нового экрана.
//!
//! За мышью — три слушателя на всё окно, а не на каждую кнопку: кнопке —
//! точка, где в неё вошли и откуда ушли (заливка), и тяга к мыши (магнит),
//! карточке `data-tilt` — точка мыши (наклон и блик), кружку у курсора —
//! место и то, над чем он. Всё это — переменные CSS на самом элементе, а
//! рисовать ли, решают стили: без мыши и без движения они не рисуют ничего.
//! Кружок — ещё и только когда его включили в настройках (задача 88; по
//! умолчанию его нет — решение человека 2026-11-09): иначе стили его не
//! рисуют, и место ему не пишется. Касание пальцем и пером не в счёт.
//! Работа — только пока мышь движется (Правило 1). То же делает
//! `site/landing.js` у лендинга.

use std::cell::RefCell;
use std::rc::Rc;

use leptos::ev;
use leptos::html;
use leptos::prelude::*;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{DomRect, Element, HtmlElement, Node, PointerEvent};

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
/// Меню разделов (`ui/nav.rs`).
const MENU: &str = ".app > .nav";

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
/// карточка открыта, — и обратно, в строку, если её будет видно
/// (`seen_whole`).
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
        clear_of_menu(page);
    }
    if let Some(row) = &before {
        name(row, Some(DOCUMENT));
        clear_of_menu(row);
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
            if let Some(row) = after.and_then(row).filter(seen_whole) {
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

/// Имя перехода элементу или прочь — и имя, и обрезку под меню.
fn name(element: &HtmlElement, name: Option<&str>) {
    let style = element.style();
    let _ = match name {
        Some(name) => style.set_property("view-transition-name", name),
        None => style
            .remove_property("view-transition-name")
            .and_then(|_| style.remove_property("clip-path"))
            .map(drop),
    };
}

/// Обрезать снимок элемента по верхнему краю меню. На телефоне меню —
/// вкладки внизу окна, поверх экрана (`position: fixed`), а снимок
/// перехода браузер рисует над всей страницей: что лежало под вкладками,
/// на время перехода легло бы на них. Снять само меню нельзя — снятое не
/// отвечает на нажатия (CSS View Transitions 1, §4.2), — а свой `clip-path`
/// элемента входит в его снимок (там же, §1.5 и §7.7.1). Обрезано только то,
/// что ниже верха меню: вбок и вверх граница далеко, и «Новый счёт», который
/// закреплён у окна и может стоять правее экрана, остаётся в снимке. Меню
/// колонкой рядом (компьютер) элемента не перекрывает — обрезать нечего.
/// Снимает обрезку `name` вместе с именем.
fn clear_of_menu(element: &HtmlElement) {
    let own = element.get_bounding_client_rect();
    let Some(top) = menu_top(&own) else {
        return;
    };
    if top <= own.top() || top >= own.bottom() {
        return;
    }
    // От верха элемента до верха меню — целыми точками: запись `f64`
    // тянула бы в главный wasm лишний код (как у `fraction`).
    let edge = (top - own.top()).floor() as i32;
    let _ = element.style().set_property(
        "clip-path",
        &format!("polygon(-100vw -100vh, 200vw -100vh, 200vw {edge}px, -100vw {edge}px)"),
    );
}

/// Верх меню, когда оно лежит поперёк элемента — вкладками внизу окна, как
/// на телефоне; меню колонкой рядом (компьютер) — `None`. Меню закреплено
/// у окна, и прокрутка его верха не двигает.
fn menu_top(own: &DomRect) -> Option<f64> {
    let menu = web_sys::window()?
        .document()?
        .query_selector(MENU)
        .ok()
        .flatten()?
        .get_bounding_client_rect();
    (menu.left() < own.right() && menu.right() > own.left()).then(|| menu.top())
}

/// Строка будет видна целиком — в окне и не под меню, — когда новый экран
/// встанет. Сменённый экран открывается с начала: `route::shown` прокручивает
/// страницу в следующем кадре, а новое состояние браузер снимает после него,
/// поэтому место считается от начала страницы (проверено вживую: Chrome 157,
/// 2026-11-08 — заголовок прокрученной карточки приходит туда, где строка
/// после прокрутки). Под меню и за окном заголовок
/// в строку не перетекает, а растворяется: его снимок, летящий к ней, лёг бы
/// на вкладки (найдено на телефоне, 2026-11-08).
fn seen_whole(element: &HtmlElement) -> bool {
    let Some(window) = web_sys::window() else {
        return false;
    };
    let scrolled = window.scroll_y().unwrap_or(0.0);
    let own = element.get_bounding_client_rect();
    let Some(floor) = menu_top(&own).or_else(|| window.inner_height().ok()?.as_f64()) else {
        return false;
    };
    own.height() > 0.0 && own.top() + scrolled >= 0.0 && own.bottom() + scrolled <= floor
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

/// Следить за мышью, пока окно открыто. `cursor` — кружок у курсора, `shown`
/// — включён ли он: выключенный стили не рисуют, и писать ему нечего.
pub fn follow(cursor: NodeRef<html::Div>, shown: RwSignal<bool>) {
    let hover = Rc::new(RefCell::new(Hover::default()));
    let circle = move || cursor.get_untracked().filter(|_| shown.get_untracked());
    let mark = move |over: Option<&str>| {
        if let Some(cursor) = circle() {
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
        if let Some(cursor) = circle() {
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
