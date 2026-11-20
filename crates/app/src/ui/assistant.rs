//! ИИ-помощник (задачи 102 и 111): экран внутри раздела налога, а сам
//! помощник — рамкой своего происхождения (`ai.kvartik.com`, крейт
//! `kvartik-ai`). Ключ, вопрос и разговор — в ней: только её CSP пускает
//! к поставщикам ИИ и только она хранит ключ, а окно с данными человека
//! ходит только к себе (CLAUDE.md, Правило 13, «ИИ-помощник»). Поставщику
//! уходит только то, что человек отправил нажатием в самой рамке.
//!
//! Окно рамке не командует: оно сообщает ей язык, сегодняшний день, есть ли
//! профиль и свои цвета, а по её просьбе — текст «моих данных» (`ai::about_me`,
//! из профиля и кодов: поэтому рамка — из части книги, как налог). Ключа окно
//! рамке не передаёт: ключ, подключённый в 0.0.158 ещё в окне, оно стирает,
//! и его вписывают в рамке заново (с 0.0.161; шапка `ai::frame`). Рамка
//! окну — что начато в ней (новая версия ждёт), свою высоту и какой ИИ
//! подключён. Сообщения и пары происхождений — ядро (`ai::frame`): от рамки
//! окно принимает только её окно (`event.source`) с её происхождения.
//!
//! Сообщение — строкой через `postMessage`, и всё окна — через `Reflect`, а
//! не привязками `web-sys`: импорты JS программа разреза оставляет в главном
//! wasm, даже когда их зовёт только часть (CLAUDE.md, Правило 6).

use js_sys::{Function, Reflect};
use kvartik_core::ai::frame::{self, Colors, FromFrame, ToFrame};
use kvartik_core::ai::{self, Provider};
use kvartik_core::i18n::{Key, Lang, fill, t};
use kvartik_core::theme;
use leptos::prelude::*;
use wasm_bindgen::{JsCast, JsValue};

use super::backup::BackupChip;
use super::icon::{Icon, icon};
use super::ledger::tax_part;
use super::route::{Router, Screen};
use super::split::screen;
use super::tax::{self, Part};
use super::tr;
use crate::store::device::Device;

/// Сколько ждать, пока рамка встанет, миллисекунд: дольше — слова о сбое и
/// «Загрузить снова». Ожидание одно, на загрузку, а не опрос (Правило 1).
const LOAD_MILLIS: i32 = 20_000;

thread_local! {
    /// Какой ИИ подключён в рамке — по её слову, записью устройства:
    /// строка на экране налога знает его и без рамки.
    static BY: ArcRwSignal<Option<Provider>> =
        ArcRwSignal::new(Device::new().ai_by().as_deref().and_then(Provider::from_code));
    /// Происхождение рамки в браузерных тестах: у раннера тестов своего
    /// домена нет, а рамка — пустая страница того же происхождения.
    #[cfg(test)]
    pub static TEST_ORIGIN: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

/// Какой ИИ подключён — отслеживаемое.
fn connected_by() -> Option<Provider> {
    BY.with(|shared| shared.get())
}

/// Сигнал — к записи, только когда значение другое (CLAUDE.md, Правило 2,
/// «Общий сигнал»).
fn follow_by(next: Option<Provider>) {
    BY.with(|shared| {
        if shared.get_untracked() != next {
            shared.set(next);
        }
    });
}

/// Экран ИИ-помощника на языке окна: «назад» к налогу и заголовок — сразу,
/// рамка — частью книги с первого показа (`split::screen`).
#[component]
pub fn AssistantScreen(ctx: tax::Ctx, router: Router) -> impl IntoView {
    let tax::Ctx { lang, backups, .. } = ctx;
    view! {
        <a class="back" href=Screen::Tax.href()>
            {icon(Icon::Back)}
            <span>{tr(lang, Key::TaxTitle)}</span>
        </a>
        <h1 tabindex="-1">{tr(lang, Key::AiTitle)}</h1>
        <BackupChip lang backups nudge=true />
        {screen(lang, router, Screen::Assistant, move || tax_part(ctx, Part::Assistant(router)))}
    }
}

/// Строка на экране налога — к ИИ-помощнику: какой ИИ подключён или чей
/// ключ можно подключить.
#[component]
pub fn AssistantEntry(lang: ReadSignal<Lang>) -> impl IntoView {
    view! {
        <ul class="list assistant-entry">
            <li>
                <a class="entry" href=Screen::Assistant.href()>
                    <span class="entry-main">
                        <span class="entry-title">{tr(lang, Key::AiTitle)}</span>
                        <span class="entry-meta">
                            <span>{move || match connected_by() {
                                Some(by) => fill(t(lang.get(), Key::AiEntryOn), &[by.name()]),
                                None => t(lang.get(), Key::AiEntryAbout).to_owned(),
                            }}</span>
                        </span>
                    </span>
                    <span class="entry-side">{icon(Icon::Forward)}</span>
                </a>
            </li>
        </ul>
    }
}

/// Встала ли рамка.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Load {
    Loading,
    Ready,
    Failed,
}

/// Происхождение рамки для этого окна: `ai.kvartik.com` или пара
/// предпросмотра (`frame::frame_origin`); `None` — окно не на своём домене.
fn origin() -> Option<String> {
    #[cfg(test)]
    if let Some(test) = TEST_ORIGIN.with_borrow(Clone::clone) {
        return Some(test);
    }
    let host = web_sys::window()?.location().host().ok()?;
    frame::frame_origin(&host)
}

/// Адрес рамки: язык окна — в адресе, и первый кадр рамки уже на нём.
fn source(origin: &str, lang: Lang) -> String {
    #[cfg(test)]
    if TEST_ORIGIN.with_borrow(Option::is_some) {
        return "about:blank".to_owned();
    }
    format!("{origin}/?lang={}", lang.code())
}

/// Свойство объекта JS — через `Reflect`.
fn get(target: &JsValue, name: &str) -> Option<JsValue> {
    Reflect::get(target, &JsValue::from_str(name)).ok().filter(|value| !value.is_undefined() && !value.is_null())
}

/// Окно рамки (`contentWindow`).
fn frame_window(frame: &web_sys::HtmlElement) -> Option<JsValue> {
    get(frame, "contentWindow")
}

/// Написать рамке — только на её происхождение.
fn send(frame: &web_sys::HtmlElement, origin: &str, message: &ToFrame) {
    let Some(target) = frame_window(frame) else {
        return;
    };
    let Some(post) = get(&target, "postMessage").and_then(|post| post.dyn_into::<Function>().ok()) else {
        return;
    };
    let _ = post.call2(&target, &JsValue::from_str(&message.to_text()), &JsValue::from_str(origin));
}

/// Сообщение рамки: от её окна (`event.source`), с её происхождения, —
/// иначе `None`.
fn accept(event: &JsValue, frame: &web_sys::HtmlElement, origin: &str) -> Option<FromFrame> {
    if get(event, "origin")?.as_string()? != origin {
        return None;
    }
    if get(event, "source")? != frame_window(frame)? {
        return None;
    }
    FromFrame::parse(&get(event, "data")?.as_string()?)
}

/// Свои цвета окна — рамке, как их ставит окно (`theme::painted`).
fn colors() -> Option<Colors> {
    let record = Device::new().colors()?;
    let (scheme, vars) = theme::painted(&record)?;
    Some(Colors {
        scheme: scheme.code().to_owned(),
        vars: vars.into_iter().map(|(name, color)| (name.to_owned(), color.to_owned())).collect(),
    })
}

/// Есть ли сеть, по слову браузера; не сказал — есть.
fn online() -> bool {
    web_sys::window()
        .and_then(|window| get(&window, "navigator"))
        .and_then(|navigator| get(&navigator, "onLine"))
        .and_then(|online| online.as_bool())
        .unwrap_or(true)
}

/// Рамка помощника — из части книги.
#[component]
pub fn AssistantFrame(ctx: tax::Ctx, router: Router) -> impl IntoView {
    let lang = ctx.lang;
    // Ключ 0.0.158 окну больше ни к чему: рамка его не берёт, а в окне его
    // прочёл бы любой код программы.
    Device::new().clear_ai();
    let Some(origin) = origin() else {
        return view! { <p>{tr(lang, Key::AiFrameAlone)}</p> }.into_any();
    };
    let load = RwSignal::new(if online() { Load::Loading } else { Load::Failed });
    // Номер рамки: «Загрузить снова» ставит новую.
    let generation = RwSignal::new(0_u32);
    let busy = RwSignal::new(false);
    let talk = RwSignal::new(false);
    // Начатое в рамке ждёт новая версия окна: перезагрузка окна стёрла бы и
    // рамку, а разговор держит её, только пока экран помощника на виду
    // (ревью задачи 102).
    ctx.forms.hold_while(move || busy.get() || (router.shows(Screen::Assistant) && talk.get()));
    let frame_ref = NodeRef::<leptos::html::Iframe>::new();
    let element = move || frame_ref.get_untracked().map(|frame| frame.unchecked_into::<web_sys::HtmlElement>());
    let state = {
        let origin = origin.clone();
        move || {
            if let Some(frame) = element() {
                let me = ctx.sources.profiles.profile().is_some() && ctx.today.is_some();
                send(&frame, &origin, &ToFrame::state(lang.get_untracked(), ctx.today, me, colors()));
            }
        }
    };
    let listener = {
        let origin = origin.clone();
        let state = state.clone();
        window_event_listener(leptos::ev::message, move |event| {
            let Some(frame) = element() else {
                return;
            };
            let Some(message) = accept(event.as_ref(), &frame, &origin) else {
                return;
            };
            match message {
                FromFrame::Ready => {
                    load.set(Load::Ready);
                    state();
                }
                FromFrame::Busy { busy: now, talk: talking } => {
                    if busy.get_untracked() != now {
                        busy.set(now);
                    }
                    if talk.get_untracked() != talking {
                        talk.set(talking);
                    }
                }
                FromFrame::Height { px } => {
                    let _ = frame.style().set_property("height", &format!("{px}px"));
                }
                FromFrame::WantMe => {
                    let (Some(profile), Some(today)) = (ctx.sources.profiles.profile(), ctx.today) else {
                        return;
                    };
                    let text = ai::about_me(lang.get_untracked(), profile.data(), today);
                    send(&frame, &origin, &ToFrame::Me { text });
                }
                FromFrame::Connected { provider } => {
                    let by = provider.as_deref().and_then(Provider::from_code);
                    Device::new().set_ai_by(by.map(Provider::code));
                    follow_by(by);
                }
                FromFrame::Unknown => {}
            }
        })
    };
    on_cleanup(move || listener.remove());
    // Язык, профиль и показ экрана — рамке сразу: свои цвета могли сменить
    // в настройках, пока экран был спрятан.
    Effect::new({
        let state = state.clone();
        move |_| {
            lang.track();
            let _ = ctx.sources.profiles.profile();
            let shown = router.shows(Screen::Assistant);
            if shown && load.get_untracked() == Load::Ready {
                state();
            }
        }
    });
    // Рамка не встала за отведённое — слова о сбое; ожидание одно, на
    // загрузку этой рамки.
    Effect::new(move |_| {
        let mine = generation.get();
        if load.get_untracked() != Load::Loading {
            return;
        }
        set_timeout(
            move || {
                if generation.get_untracked() == mine && load.get_untracked() == Load::Loading {
                    load.set(Load::Failed);
                }
            },
            std::time::Duration::from_millis(LOAD_MILLIS as u64),
        );
    });
    let retry = move || {
        busy.set(false);
        talk.set(false);
        load.set(if online() { Load::Loading } else { Load::Failed });
        generation.update(|generation| *generation = generation.wrapping_add(1));
    };
    view! {
        {move || match load.get() {
            Load::Loading => Some(view! { <p role="status" class="hint">{tr(lang, Key::PartLoading)}</p> }.into_any()),
            Load::Failed => Some(view! {
                <div class="ai-note">
                    <p role="alert">{tr(lang, Key::AiFrameFailed)}</p>
                    <div class="actions">
                        <button type="button" class="secondary" on:click=move |_| retry()>
                            <span class="label">{tr(lang, Key::AiFrameRetry)}</span>
                        </button>
                    </div>
                </div>
            }.into_any()),
            Load::Ready => None,
        }}
        {move || {
            generation.track();
            let src = source(&origin, lang.get_untracked());
            view! {
                <iframe
                    node_ref=frame_ref
                    class="ai-frame"
                    class:ready=move || load.get() == Load::Ready
                    src=src
                    title=move || t(lang.get(), Key::AiTitle)
                    sandbox="allow-scripts allow-same-origin allow-forms allow-popups allow-popups-to-escape-sandbox"
                ></iframe>
            }
        }}
    }
    .into_any()
}
