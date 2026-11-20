//! ИИ-помощник Kvartik на своём происхождении — `ai.kvartik.com` (задача
//! 111). Окно программы на `kvartik.com` встраивает эту страницу рамкой, и
//! весь помощник — ключ, вопрос и разговор — здесь: только CSP этой страницы
//! пускает к четырём адресам поставщиков ИИ (`public/_headers`), и только
//! она хранит ключ, а программа с данными человека снова ходит только к себе
//! (CLAUDE.md, Правило 13, «ИИ-помощник»).
//!
//! Работает страница только рамкой в окне своей пары — `kvartik.com` или
//! предпросмотра той же ветки (`frame::parent_origin`). Открытая сама по
//! себе или с чужого адреса, она говорит, где помощник, и ничего не умеет:
//! ключ, подключённый в рамке чужой страницы, был бы ключом той страницы.

mod bridge;
mod look;
mod net;
mod page;
mod panic;
mod store;
#[cfg(all(test, target_arch = "wasm32"))]
mod tests;

use kvartik_core::ai::frame;
use kvartik_core::i18n::{Key, Lang, t};
use leptos::prelude::*;

fn main() {
    panic::install();
    let Some(window) = web_sys::window() else {
        return;
    };
    let location = window.location();
    let lang = location.search().ok().as_deref().and_then(lang_of).unwrap_or(Lang::Am);
    let host = location.host().unwrap_or_default();
    match frame::parent_origin(&host).filter(|_| framed(&window)) {
        Some(parent) => bridge::start(parent, lang),
        None => alone(lang),
    }
}

/// Язык из адреса рамки (`?lang=ru`): окно ставит его в адрес, и первый кадр
/// рамки — уже на языке окна, до первого сообщения.
fn lang_of(search: &str) -> Option<Lang> {
    search
        .trim_start_matches('?')
        .split('&')
        .find_map(|pair| pair.strip_prefix("lang="))
        .and_then(Lang::from_code)
}

/// Внутри ли страница другой — рамкой.
fn framed(window: &web_sys::Window) -> bool {
    window
        .parent()
        .ok()
        .flatten()
        .is_some_and(|parent| !js_sys::Object::is(parent.as_ref(), window.as_ref()))
}

/// Страницу открыли не в окне программы: она говорит, где помощник.
fn alone(lang: Lang) {
    look::set_lang(lang);
    leptos::mount::mount_to_body(move || {
        view! {
            <main class="ai-page ai-alone">
                <p>{t(lang, Key::AiFrameAlone)}</p>
                <p><a class="button" href=format!("https://{}/app/#/ask", frame::SITE)>{t(lang, Key::LandingOpen)}</a></p>
            </main>
        }
    });
}
