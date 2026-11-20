//! Мост рамки с окном программы (задача 111). Сообщения и пары
//! происхождений — ядро (`kvartik_core::ai::frame`), здесь — `postMessage`.
//!
//! Рамка принимает сообщение, только когда его прислало её окно-родитель
//! (`event.source`) с происхождения своей пары (`event.origin`), и пишет
//! только ему: чужая страница, встроившая рамку, — а встроить её CSP пускает
//! только `kvartik.com` и предпросмотры (`frame-ancestors`), — не услышит
//! и не скажет ничего. Окно рамке не командует: поставщику уходит только то,
//! что человек отправил нажатием в самой рамке, а от окна приходят язык, день,
//! свои цвета, текст «моих данных» по нажатию «Добавить мои данные» и ключ
//! 0.0.158 — один раз.

use std::cell::{Cell, RefCell};

use kvartik_core::ai::Setup;
use kvartik_core::ai::frame::{FromFrame, ToFrame};
use kvartik_core::i18n::Lang;
use kvartik_core::model::date::Date;
use leptos::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen::closure::Closure;
use web_sys::MessageEvent;

use crate::page::{self, Page};
use crate::{look, store};

thread_local! {
    /// Происхождение окна — кому рамка пишет; `None` — рамка не в окне.
    static PARENT: RefCell<Option<String>> = const { RefCell::new(None) };
    /// Последнее сказанное «занято» и высота: то же самое окну второй раз не
    /// шлётся — каждое сообщение будит его обработчик.
    static BUSY: Cell<Option<(bool, bool)>> = const { Cell::new(None) };
    static HEIGHT: Cell<Option<u32>> = const { Cell::new(None) };
    /// Что рамка сказала окну — для браузерных тестов.
    #[cfg(all(test, target_arch = "wasm32"))]
    pub static SENT: RefCell<Vec<FromFrame>> = const { RefCell::new(Vec::new()) };
}

/// Написать окну.
pub fn post(message: &FromFrame) {
    match *message {
        FromFrame::Busy { busy, talk } if BUSY.replace(Some((busy, talk))) == Some((busy, talk)) => return,
        FromFrame::Height { px } if HEIGHT.replace(Some(px)) == Some(px) => return,
        _ => {}
    }
    #[cfg(all(test, target_arch = "wasm32"))]
    SENT.with_borrow_mut(|sent| sent.push(message.clone()));
    PARENT.with_borrow(|parent| {
        let Some(origin) = parent else {
            return;
        };
        let Some(target) = web_sys::window().and_then(|window| window.parent().ok().flatten()) else {
            return;
        };
        let _ = target.post_message(&JsValue::from_str(&message.to_text()), origin);
    });
}

/// Тестам: что рамка сказала окну с последнего сброса, и сброс — память о
/// сказанном тянулась бы из прошлого теста в следующий.
#[cfg(all(test, target_arch = "wasm32"))]
pub fn sent() -> Vec<FromFrame> {
    SENT.with_borrow(Clone::clone)
}

#[cfg(all(test, target_arch = "wasm32"))]
pub fn reset() {
    BUSY.set(None);
    HEIGHT.set(None);
    SENT.with_borrow_mut(Vec::clear);
}

/// Паника: окну — что рамка не занята.
pub fn release() {
    BUSY.set(None);
    post(&FromFrame::Busy { busy: false, talk: false });
}

/// Что рамка знает об окне — сигналами для экрана.
#[derive(Clone, Copy)]
pub struct Inbox {
    pub lang: RwSignal<Lang>,
    pub today: RwSignal<Option<Date>>,
    /// Есть ли у окна профиль для «Добавить мои данные».
    pub me: RwSignal<bool>,
    /// Текст «моих данных», пришедший по нажатию; экран берёт его в поле.
    pub incoming: RwSignal<Option<String>>,
}

impl Inbox {
    pub fn new(lang: Lang) -> Self {
        Self {
            lang: RwSignal::new(lang),
            today: RwSignal::new(None),
            me: RwSignal::new(false),
            incoming: RwSignal::new(None),
        }
    }

    /// Сообщение окна. Сигнал — к записи, только когда значение другое:
    /// запись того же будила бы всех, кто за ним следит.
    pub fn take(self, message: ToFrame) {
        match message {
            ToFrame::State { .. } => {
                let Some(state) = message.as_state() else {
                    return;
                };
                if self.lang.get_untracked() != state.lang {
                    self.lang.set(state.lang);
                    look::set_lang(state.lang);
                }
                if self.today.get_untracked() != state.today {
                    self.today.set(state.today);
                }
                if self.me.get_untracked() != state.me {
                    self.me.set(state.me);
                }
                look::put(state.colors.as_ref());
            }
            ToFrame::Me { text } => self.incoming.set(Some(text)),
            ToFrame::Setup { text } => migrate(&text),
            ToFrame::Unknown => {}
        }
    }
}

/// Ключ, подключённый в 0.0.158 ещё в окне: рамка его запоминает, если
/// своего у неё нет, — свой важнее, — и говорит окну стереть его запись.
/// Не запомнилось — окну ни слова: его запись остаётся до следующего раза.
fn migrate(text: &str) {
    if store::stored().is_none()
        && let Some(setup) = Setup::parse(text)
    {
        if !store::save(&setup) {
            return;
        }
        page::follow(Some(setup));
    }
    post(&FromFrame::SetupTaken);
}

/// Сообщение окна: от окна-родителя, с происхождения пары — иначе `None`.
fn accept(event: &MessageEvent, parent: &str) -> Option<ToFrame> {
    if event.origin() != parent {
        return None;
    }
    let source = event.source()?;
    let up = web_sys::window()?.parent().ok().flatten()?;
    if !js_sys::Object::is(source.as_ref(), up.as_ref()) {
        return None;
    }
    ToFrame::parse(&event.data().as_string()?)
}

/// Высоту содержимого — окну, при каждом её изменении: окно растит рамку
/// под неё, и своей прокрутки внутри экрана у рамки нет.
fn watch_height() {
    let Some(body) = web_sys::window().and_then(|window| window.document()).and_then(|document| document.body()) else {
        return;
    };
    let measured = body.clone();
    let changed = Closure::<dyn Fn()>::new(move || {
        let px = measured.get_bounding_client_rect().height().ceil();
        // Высота — неотрицательная и не больше предела ядра: иначе окно
        // сообщение отвергнет.
        let px = if px.is_finite() && px > 0.0 { px.min(200_000.0) as u32 } else { 0 };
        post(&FromFrame::Height { px });
    });
    if let Ok(observer) = web_sys::ResizeObserver::new(changed.as_ref().unchecked_ref()) {
        observer.observe(&body);
        // Наблюдатель живёт, пока живёт рамка.
        std::mem::forget(observer);
    }
    changed.forget();
}

/// Запустить рамку в окне с происхождения `parent`.
pub fn start(parent: String, lang: Lang) {
    PARENT.with_borrow_mut(|slot| *slot = Some(parent.clone()));
    look::set_lang(lang);
    leptos::mount::mount_to_body(move || {
        let inbox = Inbox::new(lang);
        let listener = window_event_listener(leptos::ev::message, move |event: MessageEvent| {
            if let Some(message) = accept(&event, &parent) {
                inbox.take(message);
            }
        });
        on_cleanup(move || listener.remove());
        watch_height();
        post(&FromFrame::Ready);
        view! { <Page inbox /> }
    });
}
