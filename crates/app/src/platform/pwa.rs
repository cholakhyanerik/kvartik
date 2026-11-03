//! Программа как приложение (задача 8): работа без сети, новая версия,
//! установка.
//!
//! Без сети программу открывает service worker — `crates/app/public/sw.js`,
//! и в нём только кэш. Здесь — его регистрация и то, что браузер сообщает
//! о новой версии и об установке. Что с этим делать, решает окно.
//!
//! # Новая версия
//!
//! Новая версия сайта ставится в фоне и ждёт (`waiting`), пока работает
//! старая (Правило 6). Об этом — [`Event::UpdateReady`], а включает её окно,
//! когда это никому не мешает ([`Pwa::update`]): service worker получает
//! «skip-waiting», и когда новая версия заработала, страница
//! перезагружается. Остальные открытые вкладки к этому времени уже под новой
//! версией, но со старой страницей, — им [`Event::UpdatedElsewhere`].
//! Новее ли пришедшая сборка и что в ней нового, окно читает из её кэша
//! ([`cached_texts`], задача 67): он полон, как только она установилась.
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
    CacheQueryOptions, Response, ServiceWorker, ServiceWorkerContainer, ServiceWorkerRegistration,
    ServiceWorkerState, VisibilityState,
};

use super::reload;

/// Адрес service worker — от адреса страницы: программа живёт в папке
/// сайта `/app/` (задача 65), и он — `/app/sw.js`. Его scope по умолчанию —
/// папка скрипта, та же `/app/`: лендинг в корне он не перехватывает.
/// Страница без косой черты (`/app`) взяла бы `/sw.js` — тот, что снимает
/// прежний service worker корня, — но такой страницы нет: Pages сам ведёт
/// `/app` на `/app/` (308; проверено wrangler pages dev 4.149.0,
/// 2026-10-16), и `tools/deploy/check.sh` сверяет это на каждой выкладке.
const SCRIPT: &str = "sw.js";

/// Начало имён кэшей service worker (`PREFIX` в `sw.js`).
const CACHE_PREFIX: &str = "kvartik-";

/// Сообщение, по которому новая версия начинает работать (`sw.js`).
const SKIP_WAITING: &str = "skip-waiting";

/// Файл заметок «Что нового» (задача 67) — в папке программы, рядом со
/// страницей: `crates/app/public/notes.json`; разбор —
/// `kvartik_core::release`.
pub const NOTES: &str = "notes.json";

/// Как часто, не чаще, спрашивать сайт о новой версии при возвращении
/// к окну: час. Проверка — один запрос `sw.js` в пару килобайт, но телефон
/// возвращается к программе десятки раз в день.
const CHECK_EVERY_MS: f64 = 60.0 * 60.0 * 1000.0;

/// Что программа как приложение сообщает окну сама.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Event {
    /// Новая версия скачана и ждёт включения.
    UpdateReady,
    /// Новая версия начала работать — её включили в другой вкладке, а эта
    /// страница ещё старая.
    UpdatedElsewhere,
    /// Браузер готов установить программу по кнопке (`beforeinstallprompt`).
    CanInstall,
    /// Программу установили.
    Installed,
    /// Страницей правит service worker, и весь сайт лежит в его кэше: сразу
    /// при открытии или после первой установки (`clients.claim()`). Части
    /// wasm теперь приходят из кэша, а не из сети (`ui/split.rs`).
    Serving,
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
    /// Новая версия включается: страница перезагрузится, когда та
    /// заработает.
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

    /// Включить новую версию и перезагрузить страницу, когда она заработает.
    /// Ждущей версии уже нет — её включили в другой вкладке — перезагрузить
    /// сразу.
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

    /// Есть ли вопрос браузера, который можно задать. После отказа Chromium
    /// присылает новый сразу, на той же странице (по исходникам Chromium,
    /// `AppBannerManager::SendBannerDismissed`, 2026-10-20) — до ответа
    /// или после.
    pub fn can_ask(&self) -> bool {
        self.shared.prompt.borrow().is_some()
    }
}

impl Shared {
    fn emit(&self, event: Event) {
        (self.on_event)(event);
    }

    /// Страницей стал править другой service worker.
    fn controller_changed(&self) {
        // Включает эта вкладка — перезагрузка, когда новая версия заработает
        // ([`Pwa::update`]).
        if self.updating.get() {
            return;
        }
        // Первый раз — это `clients.claim()` первой установки: страница была
        // без service worker, и новой версии нет.
        if self.controlled.replace(true) {
            self.emit(Event::UpdatedElsewhere);
        } else {
            self.emit(Event::Serving);
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
    if shared.controlled.get() {
        shared.emit(Event::Serving);
    }
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
/// — значит, ждёт включения. Без старой это первая установка, а не новая
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

/// Тексты файла программы `file` из кэшей её service worker — каждой
/// сборки, что лежит на устройстве: работающей и новой, которая ждёт
/// включения (задача 67). Какой новее, решает тот, кто читает: имя кэша —
/// хэш сборки, а не версия. Кэш ищется по имени (`caches.match`
/// с `cacheName`), а не открывается: `caches.open` завёл бы заново пустым
/// кэш, который новая версия только что удалила (Правило 6), а `match`
/// с именем ищет только среди тех, что есть (по спецификации Service
/// Worker, `CacheStorage.match`, 2026-10-18). Файл — от адреса страницы,
/// как его положил service worker от своего: оба в папке программы. Кэшей
/// нет — нет и текстов: в отладочной сборке, по адресу без HTTPS.
pub async fn cached_texts(file: &str) -> Vec<String> {
    let mut texts = Vec::new();
    let Some(caches) = web_sys::window().and_then(|window| window.caches().ok()) else {
        return texts;
    };
    let Ok(names) = caches.keys().await else {
        return texts;
    };
    for name in Array::from(&names).iter() {
        let Some(name) = name.as_string().filter(|name| name.starts_with(CACHE_PREFIX)) else {
            continue;
        };
        let options = CacheQueryOptions::new();
        options.set_cache_name(&name);
        // Файла в кэше нет — `undefined`, а не ответ.
        let Ok(found) = caches.match_with_str_and_options(file, &options).await else {
            continue;
        };
        let Ok(response) = found.dyn_into::<Response>() else {
            continue;
        };
        if let Ok(text) = response.text()
            && let Ok(text) = text.await
            && let Some(text) = text.as_string()
        {
            texts.push(text);
        }
    }
    texts
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
/// `fullscreen` — тоже установленная, но только у WebKit: программа
/// с экрана «Домой» на iPhone с iOS 17 отвечает им вместо `standalone` из
/// манифеста (WebKit bug 264218, открыт; по данным browser-compat-data,
/// 2026-10-12), а у Chromium им отвечает и обычная вкладка на весь экран
/// (F11; по исходникам Chromium, `BrowserWebContentsDelegate::
/// GetDisplayMode`, 2026-10-20). И `minimal-ui`: окно программы на панели
/// задач Firefox для Windows ставит его, что бы ни писал манифест (по
/// исходникам Firefox 158, `TaskbarTabsWindowManager.sys.mjs`, 2026-10-20).
/// Во вкладке браузера режим — `browser`.
pub fn installed() -> bool {
    let matches = |query: &str| {
        web_sys::window()
            .and_then(|window| window.match_media(query).ok().flatten())
            .is_some_and(|list| list.matches())
    };
    let webkit = navigator_standalone();
    webkit == Some(true)
        || matches("(display-mode: standalone)")
        || (webkit.is_some() && matches("(display-mode: fullscreen)"))
        || matches("(display-mode: minimal-ui)")
}

/// Где открыта страница — чтобы сказать словами, как установить программу
/// там, где браузер своего вопроса не задаёт (задача 55): он бывает только
/// у Chromium, а и там его нет, пока программа уже стоит или человек
/// недавно отказался. Свойства — первыми, строка `User-Agent` — последней:
/// iPad называет себя Mac'ом, а Chrome и Firefox на iPhone — тот же WebKit,
/// что Safari.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Browser {
    /// Safari на iPhone или iPad.
    SafariTouch,
    /// Chrome, Edge или Firefox на iPhone или iPad.
    OtherTouch,
    /// Safari на Mac.
    SafariMac,
    /// Chrome на компьютере.
    Chrome,
    /// Chrome на Android.
    ChromeAndroid,
    /// Edge на компьютере.
    Edge,
    /// Edge на Android.
    EdgeAndroid,
    /// Samsung Internet.
    Samsung,
    /// Firefox на Android.
    FirefoxAndroid,
    /// Firefox на Windows: программой на панели задач (с Firefox 143).
    FirefoxWindows,
    /// Firefox на Mac и Linux: сайт программой не ставит (по исходникам
    /// Firefox 158: на Linux выключено, на Mac нет вовсе; 2026-10-20).
    Firefox,
    /// Остальные: Opera, Яндекс, браузеры внутри других программ
    /// и незнакомые.
    Other,
}

impl Browser {
    /// Установленная программа здесь не видит данных браузера (Правило 6):
    /// сначала установка, потом данные.
    pub const fn install_first(self) -> bool {
        matches!(self, Self::SafariTouch | Self::OtherTouch | Self::SafariMac)
    }

    /// Браузер этой страницы.
    pub fn detect() -> Self {
        let Some(window) = web_sys::window() else {
            return Self::Other;
        };
        let navigator = window.navigator();
        let agent = navigator.user_agent().unwrap_or_default();
        Self::of(
            &agent,
            navigator_standalone().is_some(),
            navigator.max_touch_points() > 0,
        )
    }

    /// По строке `User-Agent`, свойству WebKit `navigator.standalone`
    /// и касанию.
    fn of(agent: &str, webkit: bool, touch: bool) -> Self {
        let has = |token: &str| agent.contains(token);
        if webkit {
            return if touch {
                if has("CriOS/") || has("EdgiOS/") || has("FxiOS/") {
                    Self::OtherTouch
                } else {
                    Self::SafariTouch
                }
            } else if has("Version/") && has("Safari/") {
                Self::SafariMac
            } else {
                Self::Other
            };
        }
        let android = android_agent(agent, touch);
        if has("SamsungBrowser/") {
            Self::Samsung
        } else if has("Firefox/") {
            if android {
                Self::FirefoxAndroid
            } else if has("Windows") {
                Self::FirefoxWindows
            } else {
                Self::Firefox
            }
        } else if has("EdgA/") || (has("Edg/") && android) {
            Self::EdgeAndroid
        } else if has("Edg/") {
            Self::Edge
        } else if has("OPR/") || has("YaBrowser/") || has("; wv)") || !has("Chrome/") {
            Self::Other
        } else if android {
            Self::ChromeAndroid
        } else {
            Self::Chrome
        }
    }
}

/// Android — в любом браузере и во встроенном окне другой программы
/// (WebView): автокопии там нет (`platform::autobackup::available`).
pub fn android() -> bool {
    let Some(window) = web_sys::window() else {
        return false;
    };
    let navigator = window.navigator();
    android_agent(&navigator.user_agent().unwrap_or_default(), navigator.max_touch_points() > 0)
}

/// Android по строке `User-Agent` и касанию. Планшет на Android в режиме
/// «версия для компьютера» называет себя Linux'ом — так у Firefox на
/// планшете по умолчанию (по исходникам Firefox 158, 2026-10-20). Его
/// выдаёт касание: у компьютера на Linux экран обычно без него, а
/// Chromebook — `CrOS`, а не Linux.
fn android_agent(agent: &str, touch: bool) -> bool {
    agent.contains("Android") || (touch && agent.contains("Linux"))
}

/// iPhone или iPad — в браузере или программой с экрана «Домой».
///
/// `navigator.standalone` есть у WebKit на всех устройствах Apple — с Safari
/// 17 и на Mac (WebKit bug 257776; по исходникам WebKit, `Navigator.idl`,
/// 2026-10-12), поэтому iPhone и iPad отличаются касанием: у iPad Safari
/// называет себя Mac'ом, а у Mac точек касания нет (`maxTouchPoints` — 5
/// на iPhone и iPad, 0 на Mac; по исходникам WebKit, `Navigator.cpp`,
/// 2026-10-15).
pub fn apple_touch() -> bool {
    let touch = web_sys::window().is_some_and(|window| window.navigator().max_touch_points() > 0);
    touch && navigator_standalone().is_some()
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

#[cfg(test)]
mod tests {
    use super::Browser;

    /// Строки `User-Agent` — из документации и исходников браузеров
    /// (разведка задачи 55, 2026-10-20): Chromium `user_agent_utils.cc`,
    /// Microsoft Learn «User-Agent guidance», developer.samsung.com,
    /// Firefox 158 `nsHttpHandler.cpp` и `UserAgent.swift`, WebKit
    /// `UserAgentIOS.mm`. iPad называет себя Mac'ом, и Firefox на iPad —
    /// тоже Safari: его не отличить, и он получит слова Safari.
    #[test]
    fn browsers_are_told_apart() {
        const MAC: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/26.0 Safari/605.1.15";
        const LINUX_FIREFOX: &str =
            "Mozilla/5.0 (X11; Linux x86_64; rv:158.0) Gecko/20100101 Firefox/158.0";
        // Строка, есть ли `navigator.standalone`, есть ли касание, браузер.
        let cases = [
            (
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/155.0.0.0 Safari/537.36",
                false,
                false,
                Browser::Chrome,
            ),
            (
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/155.0.0.0 Safari/537.36",
                false,
                false,
                Browser::Chrome,
            ),
            (
                "Mozilla/5.0 (Linux; Android 10; K) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/156.0.0.0 Mobile Safari/537.36",
                false,
                true,
                Browser::ChromeAndroid,
            ),
            (
                "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/156.0.0.0 Safari/537.36",
                false,
                true,
                Browser::ChromeAndroid,
            ),
            (
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/155.0.0.0 Safari/537.36 Edg/155.0.0.0",
                false,
                true,
                Browser::Edge,
            ),
            (
                "Mozilla/5.0 (Linux; Android 10; K) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Mobile Safari/537.36 EdgA/120.0.0.0",
                false,
                true,
                Browser::EdgeAndroid,
            ),
            (
                "Mozilla/5.0 (Linux; Android 10; K) AppleWebKit/537.36 (KHTML, like Gecko) SamsungBrowser/24.0 Chrome/117.0.0.0 Mobile Safari/537.36",
                false,
                true,
                Browser::Samsung,
            ),
            (
                "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) SamsungBrowser/24.0 Chrome/117.0.0.0 Safari/537.36",
                false,
                true,
                Browser::Samsung,
            ),
            (
                "Mozilla/5.0 (Android 14; Mobile; rv:158.0) Gecko/158.0 Firefox/158.0",
                false,
                true,
                Browser::FirefoxAndroid,
            ),
            (LINUX_FIREFOX, false, true, Browser::FirefoxAndroid),
            (LINUX_FIREFOX, false, false, Browser::Firefox),
            (
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:158.0) Gecko/20100101 Firefox/158.0",
                false,
                false,
                Browser::FirefoxWindows,
            ),
            (
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:158.0) Gecko/20100101 Firefox/158.0",
                false,
                false,
                Browser::Firefox,
            ),
            (MAC, true, false, Browser::SafariMac),
            (MAC, true, true, Browser::SafariTouch),
            (
                "Mozilla/5.0 (iPhone; CPU iPhone OS 18_6 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/26.0 Mobile/15E148 Safari/604.1",
                true,
                true,
                Browser::SafariTouch,
            ),
            (
                "Mozilla/5.0 (iPhone; CPU iPhone OS 18_6 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) CriOS/155.0.7390.96 Mobile/15E148 Safari/604.1",
                true,
                true,
                Browser::OtherTouch,
            ),
            (
                "Mozilla/5.0 (iPhone; CPU iPhone OS 18_6 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) EdgiOS/155.0.3537.71 Mobile/15E148 Safari/605.1.15",
                true,
                true,
                Browser::OtherTouch,
            ),
            (
                "Mozilla/5.0 (iPhone; CPU iPhone OS 18_7 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) FxiOS/158.1 Mobile/15E148 Safari/604.1",
                true,
                true,
                Browser::OtherTouch,
            ),
            // Программа на WKWebView для Mac: без хвоста Safari.
            (
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko)",
                true,
                false,
                Browser::Other,
            ),
            (
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/155.0.0.0 Safari/537.36 OPR/120.0.0.0",
                false,
                false,
                Browser::Other,
            ),
            (
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/154.0.0.0 YaBrowser/26.9.0.0 Safari/537.36",
                false,
                false,
                Browser::Other,
            ),
            (
                "Mozilla/5.0 (Linux; Android 14; wv) AppleWebKit/537.36 (KHTML, like Gecko) Version/4.0 Chrome/155.0.0.0 Mobile Safari/537.36",
                false,
                true,
                Browser::Other,
            ),
        ];
        let wrong: Vec<String> = cases
            .iter()
            .filter_map(|&(agent, webkit, touch, expected)| {
                let got = Browser::of(agent, webkit, touch);
                (got != expected).then(|| format!("{got:?} вместо {expected:?}: {agent}"))
            })
            .collect();
        assert!(wrong.is_empty(), "браузер не тот: {wrong:#?}");
    }
}
