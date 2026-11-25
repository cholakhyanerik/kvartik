//! Экран не гаснет, пока идёт круг QR-кодов (задача 124, A3): камера другого
//! устройства снимает код десятки секунд — круг кода в 32 КиБ идёт минуту, —
//! а телефон без касания гасит экран через полминуты, и круг обрывается
//! посреди кода.
//!
//! Screen Wake Lock API, `navigator.wakeLock.request("screen")`: Chrome и Edge
//! 84, Firefox 126, Safari 16.4, а на iPhone в программе с экрана «Домой» —
//! только с iOS 18.4: с 16.4 по 18.3 там она не работала (WebKit bug 254545;
//! по браузеру: MDN browser-compat-data, `api/WakeLock.json`, сверено
//! 2026-11-25). Браузер отпускает блокировку сам, когда страница уходит
//! с глаз, и сам её не возвращает (W3C Screen Wake Lock API, «Handling
//! document loss of visibility»), — поэтому окно просит её снова, когда
//! страница вернулась, а круг ещё на виду. Отказ — API нет, разрешения нет,
//! страница не на виду, батарея на исходе — молча: экран гаснет, как гас
//! раньше, и это не беда.
//!
//! Держат её те, кому она нужна ([`hold`] и [`let_go`]), — счётом: блокировка
//! одна на страницу, а показанных кодов бывает несколько. Привязок `web-sys`
//! нет — только `Reflect` и `Function`: новая привязка легла бы импортом
//! в главный wasm, хоть зовёт её одна часть (CLAUDE.md, Правило 6, «Импорт JS
//! — в главном wasm»).

use std::cell::RefCell;

use js_sys::{Function, Promise, Reflect};
use wasm_bindgen::{JsCast, JsValue};

/// Блокировка экрана на странице.
#[derive(Default)]
struct State {
    /// Скольким она сейчас нужна.
    holders: usize,
    /// Запрос в пути — второй не просить.
    asking: bool,
    /// Что дал браузер (`WakeLockSentinel`): ею блокировку и отпускают.
    sentinel: Option<JsValue>,
    /// Слушает ли страница своё возвращение на глаза — один раз за страницу.
    listening: bool,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::default();
}

/// Экран нужен включённым — до парного [`let_go`].
pub fn hold() {
    let listen = STATE.with_borrow_mut(|state| {
        state.holders += 1;
        !std::mem::replace(&mut state.listening, true)
    });
    if listen {
        super::when_shown(refresh);
    }
    refresh();
}

/// Экран больше не нужен тому, кто звал [`hold`]: не нужен никому — блокировка
/// отпущена.
pub fn let_go() {
    let sentinel = STATE.with_borrow_mut(|state| {
        state.holders = state.holders.saturating_sub(1);
        if state.holders == 0 { state.sentinel.take() } else { None }
    });
    if let Some(sentinel) = sentinel {
        release(&sentinel);
    }
}

/// Нужна, а её нет или браузер её уже отпустил, — попросить.
fn refresh() {
    let wanted = STATE.with_borrow(|state| {
        state.holders > 0 && !state.asking && state.sentinel.as_ref().is_none_or(released)
    });
    if !wanted {
        return;
    }
    let Some(promise) = request() else {
        return;
    };
    STATE.with_borrow_mut(|state| state.asking = true);
    js_sys::futures::spawn_local(async move {
        let got = promise.await.ok();
        let spare = STATE.with_borrow_mut(|state| {
            state.asking = false;
            match got {
                Some(sentinel) if state.holders > 0 => {
                    state.sentinel = Some(sentinel);
                    None
                }
                // Пока ждали, она стала не нужна.
                other => other,
            }
        });
        if let Some(sentinel) = spare {
            release(&sentinel);
        }
    });
}

/// `navigator.wakeLock.request("screen")`; `None` — API нет или вызов бросил.
fn request() -> Option<Promise> {
    let navigator = web_sys::window()?.navigator();
    let lock = Reflect::get(&navigator, &"wakeLock".into()).ok()?;
    if lock.is_undefined() || lock.is_null() {
        return None;
    }
    let ask: Function = Reflect::get(&lock, &"request".into()).ok()?.dyn_into().ok()?;
    let answer = ask.call1(&lock, &"screen".into()).ok()?;
    // Обещание — по его `then`, а не `instanceof Promise`: та привязка легла
    // бы новым импортом в главный wasm (замер 0.0.191).
    let then = Reflect::get(&answer, &"then".into()).ok()?;
    then.is_function().then(|| answer.unchecked_into())
}

/// Отпустил ли браузер блокировку сам: страница уходила с глаз.
fn released(sentinel: &JsValue) -> bool {
    Reflect::get(sentinel, &"released".into())
        .ok()
        .and_then(|released| released.as_bool())
        .unwrap_or(true)
}

/// Отпустить блокировку. Её обещание отказом не кончается, и ждать его незачем.
fn release(sentinel: &JsValue) {
    if let Ok(Ok(release)) = Reflect::get(sentinel, &"release".into()).map(JsCast::dyn_into::<Function>) {
        let _ = release.call0(sentinel);
    }
}
