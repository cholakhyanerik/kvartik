//! Программа как приложение (задача 8): работа без сети, новая версия по
//! согласию, установка.
//!
//! Без сети программу открывает service worker — `crates/app/public/sw.js`,
//! и в нём только кэш. Здесь — его регистрация и то, что браузер сообщает
//! о новой версии и об установке. Что из этого сказать человеку, решает
//! окно.
//!
//! # Новая версия — по согласию
//!
//! Новая версия сайта ставится в фоне и ждёт (`waiting`), пока работает
//! старая (Правило 6). Окно говорит о ней ([`Event::UpdateReady`]),
//! а включает её только согласие человека ([`Pwa::update`]): service worker
//! получает «skip-waiting», и когда новая версия заработала, страница
//! перезагружается. Остальные открытые вкладки к этому времени уже под новой
//! версией, но со старой страницей, — им [`Event::UpdatedElsewhere`].
//!
//! Браузер проверяет `sw.js` сам — при каждом открытии страницы. Окно,
//! которое не закрывают неделями, проверяет ещё и при возвращении к нему,
//! не чаще раза в [`CHECK_EVERY_MS`]: событие, а не таймер — в покое
//! программа не работает (Правило 1).
//!
//! # Только в выпускной сборке
//!
//! В отладке (`trunk serve`) service worker не регистрируется: страница из
//! его кэша показывала бы прошлую сборку после каждой правки. Мало того,
//! отладочная сборка снимает его и чистит его кэш ([`unregister`]): он живёт
//! по адресу вместе с портом, и оставшийся от выпускной сборки на том же
//! порту тихо отдавал бы старое. Выпускную сборку на машине смотрят на своём
//! порту (CLAUDE.md, Правило 2).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use js_sys::{Array, Promise, Reflect};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{
    ServiceWorker, ServiceWorkerContainer, ServiceWorkerRegistration, ServiceWorkerState,
    VisibilityState,
};

use super::reload;

/// Адрес service worker — от адреса страницы: сайт может жить и не в корне
/// домена (задача 16; Trunk тогда собирает с `public_url`).
const SCRIPT: &str = "sw.js";

/// Начало имён кэшей service worker (`PREFIX` в `sw.js`).
const CACHE_PREFIX: &str = "kvartik-";

/// Сообщение, по которому новая версия начинает работать (`sw.js`).
const SKIP_WAITING: &str = "skip-waiting";

/// Как часто, не чаще, спрашивать сайт о новой версии при возвращении
/// к окну: час. Проверка — один запрос `sw.js` в пару килобайт, но телефон
/// возвращается к программе десятки раз в день.
const CHECK_EVERY_MS: f64 = 60.0 * 60.0 * 1000.0;

/// Что программа как приложение сообщает окну сама.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Event {
    /// Новая версия скачана и ждёт согласия.
    UpdateReady,
    /// Новая версия начала работать — её включили в другой вкладке, а эта
    /// страница ещё старая.
    UpdatedElsewhere,
    /// Браузер готов установить программу по кнопке (`beforeinstallprompt`).
    CanInstall,
    /// Программу установили.
    Installed,
}

/// Программа как приложение. Одна на окно.
#[derive(Clone)]
pub struct Pwa {
    shared: Rc<Shared>,
}

struct Shared {
    on_event: Box<dyn Fn(Event)>,
    registration: RefCell<Option<ServiceWorkerRegistration>>,
    /// Правит ли страницей service worker — с открытия или с первого
    /// `clients.claim()` после первой установки.
    controlled: Cell<bool>,
    /// Человек согласился на новую версию: страница перезагрузится, когда
    /// та заработает.
    updating: Cell<bool>,
    /// Когда в последний раз спрашивали сайт о новой версии, мс.
    checked: Cell<f64>,
    /// Вопрос браузера об установке, отложенный до кнопки.
    prompt: RefCell<Option<InstallPrompt>>,
}

#[wasm_bindgen]
extern "C" {
    /// `BeforeInstallPromptEvent`. Его нет в `web-sys`: событие есть только
    /// у Chromium и описано не стандартом, а черновиком WICG.
    #[wasm_bindgen(extends = web_sys::Event, js_name = BeforeInstallPromptEvent)]
    type InstallPrompt;

    /// Показать вопрос браузера. Можно один раз на событие.
    #[wasm_bindgen(method, catch)]
    fn prompt(this: &InstallPrompt) -> Result<Promise, JsValue>;

    /// Ответ человека: обещание, которое исполняется, когда вопрос закрыт.
    #[wasm_bindgen(method, getter, js_name = userChoice)]
    fn user_choice(this: &InstallPrompt) -> Promise;
}

impl Pwa {
    /// Слушать установку и, в выпускной сборке, зарегистрировать service
    /// worker. `on_event` получает то, о чём браузер сообщает сам.
    pub fn start(on_event: impl Fn(Event) + 'static) -> Self {
        let shared = Rc::new(Shared {
            on_event: Box::new(on_event),
            registration: RefCell::new(None),
            controlled: Cell::new(false),
            updating: Cell::new(false),
            checked: Cell::new(js_sys::Date::now()),
            prompt: RefCell::new(None),
        });
        listen_install(&shared);
        if cfg!(debug_assertions) {
            js_sys::futures::spawn_local(unregister());
        } else {
            js_sys::futures::spawn_local(register(Rc::clone(&shared)));
        }
        Self { shared }
    }

    /// Человек согласился на новую версию: включить её и перезагрузить
    /// страницу, когда она заработает. Ждущей версии уже нет — её включили
    /// в другой вкладке — перезагрузить сразу.
    pub fn update(&self) {
        let waiting = self
            .shared
            .registration
            .borrow()
            .as_ref()
            .and_then(ServiceWorkerRegistration::waiting);
        let Some(worker) = waiting else {
            reload();
            return;
        };
        self.shared.updating.set(true);
        let watched = worker.clone();
        listen(
            |handler| worker.set_onstatechange(handler),
            move || {
                // `redundant` — её вытеснила версия ещё новее: перезагрузка
                // откроет то, что работает, а не оставит «Обновляем…»
                // навсегда.
                if matches!(
                    watched.state(),
                    ServiceWorkerState::Activated | ServiceWorkerState::Redundant
                ) {
                    reload();
                }
            },
        );
        if let Err(error) = worker.post_message(&SKIP_WAITING.into()) {
            log("postMessage", &error);
            reload();
        }
    }

    /// Задать вопрос браузера об установке — по нажатию кнопки: без свежего
    /// нажатия браузер его не покажет. Кончается, когда человек ответил;
    /// согласие подтвердит событие [`Event::Installed`].
    pub async fn install(&self) {
        let Some(prompt) = self.shared.prompt.borrow_mut().take() else {
            return;
        };
        let shown = match prompt.prompt() {
            Ok(promise) => promise.await,
            Err(error) => Err(error),
        };
        if let Err(error) = shown {
            log("prompt", &error);
            return;
        }
        if let Err(error) = prompt.user_choice().await {
            log("userChoice", &error);
        }
    }
}

impl Shared {
    fn emit(&self, event: Event) {
        (self.on_event)(event);
    }

    /// Страницей стал править другой service worker.
    fn controller_changed(&self) {
        // Своё согласие — перезагрузка, когда новая версия заработает
        // ([`Pwa::update`]).
        if self.updating.get() {
            return;
        }
        // Первый раз — это `clients.claim()` первой установки: страница была
        // без service worker, и новой версии нет.
        if self.controlled.replace(true) {
            self.emit(Event::UpdatedElsewhere);
        }
    }

    /// Человек вернулся к окну: если давно не спрашивали, спросить сайт
    /// о новой версии. Ответ придёт событием `updatefound`.
    fn returned(&self) {
        let now = js_sys::Date::now();
        if now - self.checked.get() < CHECK_EVERY_MS {
            return;
        }
        let Some(registration) = self.registration.borrow().clone() else {
            return;
        };
        self.checked.set(now);
        let Ok(promise) = registration.update() else {
            return;
        };
        // Без сети проверка падает — это не ошибка: проверим в следующий
        // раз.
        js_sys::futures::spawn_local(async move {
            let _ = promise.await;
        });
    }
}

/// Зарегистрировать service worker и слушать, что он сообщает.
async fn register(shared: Rc<Shared>) {
    let Some(container) = container() else {
        return;
    };
    shared.controlled.set(container.controller().is_some());
    let changed = Rc::clone(&shared);
    listen(
        |handler| container.set_oncontrollerchange(handler),
        move || changed.controller_changed(),
    );

    let registration: ServiceWorkerRegistration = match container.register(SCRIPT).await {
        Ok(registration) => registration.unchecked_into(),
        Err(error) => {
            log("register", &error);
            return;
        }
    };
    shared.registration.replace(Some(registration.clone()));

    // Новая версия могла скачаться ещё в прошлый раз и ждать.
    if registration.waiting().is_some() {
        shared.emit(Event::UpdateReady);
    }
    if let Some(worker) = registration.installing() {
        watch(&shared, &registration, worker);
    }
    let found = Rc::clone(&shared);
    let watched = registration.clone();
    listen(
        |handler| registration.set_onupdatefound(handler),
        move || {
            if let Some(worker) = watched.installing() {
                watch(&found, &watched, worker);
            }
        },
    );

    if let Some(document) = web_sys::window().and_then(|window| window.document()) {
        let visible = document.clone();
        let returned = Rc::clone(&shared);
        listen(
            |handler| document.set_onvisibilitychange(handler),
            move || {
                if visible.visibility_state() == VisibilityState::Visible {
                    returned.returned();
                }
            },
        );
    }
}

/// Следить за устанавливаемой версией: установилась, а старая ещё работает,
/// — значит, ждёт согласия. Без старой это первая установка, а не новая
/// версия. Не установилась — `sw.js` отверг ответы сервера (шапка `sw.js`):
/// человеку об этом знать нечего, браузер попробует снова, а разработчику
/// причина — в консоли service worker.
fn watch(shared: &Rc<Shared>, registration: &ServiceWorkerRegistration, worker: ServiceWorker) {
    let shared = Rc::clone(shared);
    let registration = registration.clone();
    let watched = worker.clone();
    listen(
        |handler| worker.set_onstatechange(handler),
        move || match watched.state() {
            ServiceWorkerState::Installed if registration.active().is_some() => {
                shared.emit(Event::UpdateReady);
            }
            ServiceWorkerState::Redundant => {
                log("install", &"новая версия не установилась".into());
            }
            _ => {}
        },
    );
}

/// Снять service worker и его кэши — в отладочной сборке.
async fn unregister() {
    let Some(container) = container() else {
        return;
    };
    if let Ok(registrations) = container.get_registrations().await {
        for registration in Array::from(&registrations).iter() {
            let registration: ServiceWorkerRegistration = registration.unchecked_into();
            if let Ok(promise) = registration.unregister() {
                let _ = promise.await;
            }
        }
    }
    let Some(caches) = web_sys::window().and_then(|window| window.caches().ok()) else {
        return;
    };
    let Ok(names) = caches.keys().await else {
        return;
    };
    for name in Array::from(&names).iter() {
        if let Some(name) = name.as_string()
            && name.starts_with(CACHE_PREFIX)
        {
            let _ = caches.delete(&name).await;
        }
    }
}

/// Вопрос браузера об установке и сама установка.
fn listen_install(shared: &Rc<Shared>) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let offered = Rc::clone(shared);
    let on_prompt = Closure::<dyn FnMut(web_sys::Event)>::new(move |event: web_sys::Event| {
        // Своя кнопка вместо панели браузера: на Android Chrome без
        // `preventDefault` показывает поверх страницы свою (по браузеру:
        // web.dev, «How to provide your own in-app install experience»).
        event.prevent_default();
        offered.prompt.replace(Some(event.unchecked_into()));
        offered.emit(Event::CanInstall);
    });
    let installed = Rc::clone(shared);
    let on_installed = Closure::<dyn FnMut()>::new(move || {
        installed.prompt.replace(None);
        installed.emit(Event::Installed);
    });
    for (name, handler) in [
        ("beforeinstallprompt", on_prompt.as_ref()),
        ("appinstalled", on_installed.as_ref()),
    ] {
        if let Err(error) = window.add_event_listener_with_callback(name, handler.unchecked_ref()) {
            log(name, &error);
        }
    }
    // Слушают, пока жива страница.
    on_prompt.forget();
    on_installed.forget();
}

/// Открыта ли программа установленной — своим окном или с экрана «Домой».
///
/// `fullscreen` — тоже установленная: программа с экрана «Домой» на iPhone
/// с iOS 17 отвечает им вместо `standalone` из манифеста (WebKit bug 264218,
/// открыт; по данным browser-compat-data, 2026-10-12).
pub fn installed() -> bool {
    let matches = |query: &str| {
        web_sys::window()
            .and_then(|window| window.match_media(query).ok().flatten())
            .is_some_and(|list| list.matches())
    };
    navigator_standalone() == Some(true)
        || matches("(display-mode: standalone)")
        || matches("(display-mode: fullscreen)")
}

/// Ставят ли программу здесь только руками: iPhone или iPad в браузере.
///
/// `navigator.standalone` есть у WebKit на всех устройствах Apple — с Safari
/// 17 и на Mac (WebKit bug 257776; по исходникам WebKit, `Navigator.idl`,
/// 2026-10-12), поэтому iPhone и iPad отличаются касанием: у iPad Safari
/// называет себя Mac'ом, а у Mac точек касания нет.
pub fn install_by_hand() -> bool {
    let touch = web_sys::window().is_some_and(|window| window.navigator().max_touch_points() > 0);
    touch && navigator_standalone() == Some(false)
}

/// `navigator.standalone` — свойство WebKit: `true` в программе с экрана
/// «Домой» или из Dock, `false` во вкладке браузера. У других движков его
/// нет.
fn navigator_standalone() -> Option<bool> {
    let navigator = web_sys::window()?.navigator();
    Reflect::get(&navigator, &"standalone".into()).ok()?.as_bool()
}

/// Поставить обработчик, который живёт, пока жива страница: таких здесь
/// несколько, и снимать их некому.
fn listen(set: impl FnOnce(Option<&js_sys::Function>), handler: impl FnMut() + 'static) {
    let closure = Closure::<dyn FnMut()>::new(handler);
    set(Some(closure.as_ref().unchecked_ref()));
    closure.forget();
}

/// `navigator.serviceWorker`. Без защищённого контекста — HTTPS или
/// `localhost` — его нет вовсе (по спецификации Service Worker,
/// `[SecureContext]`), как и шифра: программа тогда работает, но без сети
/// не откроется.
fn container() -> Option<ServiceWorkerContainer> {
    let navigator = web_sys::window()?.navigator();
    let container = Reflect::get(&navigator, &"serviceWorker".into()).ok()?;
    if container.is_undefined() || container.is_null() {
        return None;
    }
    Some(container.unchecked_into())
}

/// Причина — в консоль для разработчика; наружу ничего не уходит
/// (Правило 13).
fn log(context: &str, error: &JsValue) {
    web_sys::console::error_2(&format!("Kvartik, приложение ({context}):").into(), error);
}
