//! Ответ браузера на запрос или транзакцию IndexedDB — как будущее Rust.

use std::cell::{Cell, RefCell};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

use js_sys::Function;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{IdbRequest, IdbTransaction};

/// То, у чего есть пара событий: «получилось» и «нет».
pub(super) trait Outcome: Clone {
    fn listen(&self, ok: Option<&Function>, fail: Option<&Function>);
}

/// Запрос: `success` или `error`.
impl Outcome for IdbRequest {
    fn listen(&self, ok: Option<&Function>, fail: Option<&Function>) {
        self.set_onsuccess(ok);
        self.set_onerror(fail);
    }
}

/// Транзакция: `complete` или `abort`.
impl Outcome for IdbTransaction {
    fn listen(&self, ok: Option<&Function>, fail: Option<&Function>) {
        self.set_oncomplete(ok);
        self.set_onabort(fail);
    }
}

/// Будущее: готово, когда пришло одно из двух событий. `Err(())` —
/// пришло «нет»; что именно случилось, говорит `error()` у самой цели.
///
/// Обработчики ставятся сразу, в `new`, а не при первом опросе: запрос,
/// поставленный раньше, чем его начали ждать, может ответить, пока ждут
/// другой, — и ответ без обработчика пропал бы навсегда.
///
/// Задача будится прямо из обработчика, и продолжение выполняется
/// в микрозадаче сразу за событием, пока транзакция ещё активна: на этом
/// держится «прочитать — посчитать — записать» одной транзакцией.
pub(super) struct Settle<T: Outcome> {
    target: T,
    state: Rc<State>,
    _ok: Closure<dyn FnMut()>,
    _fail: Closure<dyn FnMut()>,
}

#[derive(Default)]
struct State {
    outcome: Cell<Option<bool>>,
    waker: RefCell<Option<Waker>>,
}

impl State {
    fn settle(&self, ok: bool) {
        if self.outcome.get().is_none() {
            self.outcome.set(Some(ok));
            if let Some(waker) = self.waker.borrow_mut().take() {
                waker.wake();
            }
        }
    }
}

impl<T: Outcome> Settle<T> {
    pub(super) fn new(target: T) -> Self {
        let state = Rc::new(State::default());
        let on_ok = Rc::clone(&state);
        let ok = Closure::<dyn FnMut()>::new(move || on_ok.settle(true));
        let on_fail = Rc::clone(&state);
        let fail = Closure::<dyn FnMut()>::new(move || on_fail.settle(false));
        target.listen(
            Some(ok.as_ref().unchecked_ref()),
            Some(fail.as_ref().unchecked_ref()),
        );
        Self {
            target,
            state,
            _ok: ok,
            _fail: fail,
        }
    }

    /// То, чего ждём: у него потом берут `result()` или `error()`.
    pub(super) fn target(&self) -> &T {
        &self.target
    }
}

impl<T: Outcome> Future for Settle<T> {
    type Output = Result<(), ()>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.state.outcome.get() {
            Some(true) => Poll::Ready(Ok(())),
            Some(false) => Poll::Ready(Err(())),
            None => {
                *self.state.waker.borrow_mut() = Some(cx.waker().clone());
                Poll::Pending
            }
        }
    }
}

/// Замыкания уходят вместе с будущим, поэтому обработчики снимаются:
/// событие, пришедшее после этого, не должно звать освобождённый код.
impl<T: Outcome> Drop for Settle<T> {
    fn drop(&mut self) {
        self.target.listen(None, None);
    }
}
