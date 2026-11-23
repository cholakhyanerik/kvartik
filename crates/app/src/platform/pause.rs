//! Пауза в долгой работе (0.0.28): отдать управление браузеру и продолжить
//! следующей задачей — между ними страница рисуется и отвечает на нажатия
//! (`.claude/rules/speed.md`, «Главный поток один на всё»). Долгие шаги копии
//! ядро делает кусками и между ними ждёт эту паузу (`format::backup`,
//! `*_sliced`).
//!
//! Сообщением `MessageChannel`, а не `setTimeout`: таймеры фоновой вкладки
//! браузер выравнивает до секунды, и копия из десятков кусков шла бы
//! минуту, стоило переключиться на другую вкладку, а сообщения он доставляет
//! без задержки (по спецификации HTML, «timer initialisation steps»;
//! Chrome — «Background tab throttling»). Не микрозадачей: между
//! микрозадачами браузер не рисует.
//!
//! Канал — один на окно: каждая пауза ставит своё «продолжить» в очередь
//! и посылает одно сообщение, а обработчик на каждое сообщение будит одну
//! паузу — по порядку.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use js_sys::{Function, Promise};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::MessageChannel;

/// Канал пауз и очередь ждущих.
struct Channel {
    channel: MessageChannel,
    waiting: Rc<RefCell<VecDeque<Function>>>,
    /// Обработчик сообщений живёт, пока жив канал.
    _on_message: Closure<dyn FnMut()>,
}

impl Channel {
    fn new() -> Option<Self> {
        let channel = MessageChannel::new().ok()?;
        let waiting: Rc<RefCell<VecDeque<Function>>> = Rc::default();
        let queue = Rc::clone(&waiting);
        let on_message = Closure::<dyn FnMut()>::new(move || {
            let next = queue.borrow_mut().pop_front();
            if let Some(resume) = next {
                let _ = resume.call0(&JsValue::NULL);
            }
        });
        // Обработчик `onmessage` сам открывает порт (по спецификации HTML).
        channel.port1().set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        Some(Self {
            channel,
            waiting,
            _on_message: on_message,
        })
    }

    /// Поставить `resume` в очередь и послать сообщение; `false` — не вышло.
    fn post(&self, resume: Function) -> bool {
        self.waiting.borrow_mut().push_back(resume);
        if self.channel.port2().post_message(&JsValue::NULL).is_ok() {
            return true;
        }
        self.waiting.borrow_mut().pop_back();
        false
    }
}

thread_local! {
    static CHANNEL: Option<Channel> = Channel::new();
}

/// Пауза: продолжить следующей задачей браузера. Канала нет — продолжить
/// сразу: работа пойдёт одним куском, как раньше, но не встанет.
pub async fn breathe() {
    let promise = Promise::new(&mut |resolve, _| {
        let posted = CHANNEL.with(|channel| {
            channel
                .as_ref()
                .is_some_and(|channel| channel.post(resolve.clone()))
        });
        if !posted {
            let _ = resolve.call0(&JsValue::NULL);
        }
    });
    let _ = promise.await;
}
