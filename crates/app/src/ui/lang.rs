//! Язык окна (задача 53): с каким открыться, переключатель и `lang`
//! у страницы.
//!
//! Язык окна — не язык документа (Архитектура): переключатель меняет
//! только окно, а язык счетов и актов выбирается у клиента и документа.

use kvartik_core::i18n::{Key, Lang, t};
use leptos::prelude::*;

use crate::platform;
use crate::store::device::Device;

/// Язык окна — один на всё окно. Меняется только [`WindowLang::choose`]:
/// вместе с ним — выбор на устройстве и `lang` у страницы.
#[derive(Clone, Copy)]
pub struct WindowLang(RwSignal<Lang>);

impl WindowLang {
    /// Язык при открытии: выбранный на этом устройстве, а не выбирали —
    /// по языкам браузера (`Lang::preferred`). Читается синхронно, до первой
    /// отрисовки, — окно не мигает чужим языком, — и сразу ставится
    /// у страницы: паника в первой же отрисовке скажет на нём (`ui::panic`).
    pub fn start() -> Self {
        let lang = Device::new().ui_lang().unwrap_or_else(|| {
            let browser = platform::languages();
            Lang::preferred(browser.iter().map(String::as_str))
        });
        mark_page(lang);
        Self(RwSignal::new(lang))
    }

    /// Язык окна для разметки: каждая строка — производное от него
    /// (Правило 4).
    pub fn signal(self) -> ReadSignal<Lang> {
        self.0.read_only()
    }

    /// Человек выбрал язык: всё окно переводится сразу, без перезагрузки.
    /// Не запомнилось (`localStorage` отказал) — окно всё равно на новом
    /// языке, а в следующий раз откроется по языкам браузера.
    fn choose(self, lang: Lang) {
        if self.0.get_untracked() == lang {
            return;
        }
        Device::new().set_ui_lang(lang);
        mark_page(lang);
        self.0.set(lang);
    }
}

/// `lang` у `<html>`: по нему браузер выбирает шрифт и переносы, экранный
/// диктор — произношение, а плашка сбоя — свой язык.
fn mark_page(lang: Lang) {
    if let Some(html) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.document_element())
    {
        let _ = html.set_attribute("lang", lang.code());
    }
}

/// Переключатель: имена языков на них самих, «Հայերեն · Русский · English».
/// Выбранный — нажатая кнопка (`aria-pressed`) с заливкой, остальные —
/// с рамкой: отличается он не только цветом (Правило 4).
#[component]
pub fn LangSwitch(lang: WindowLang) -> impl IntoView {
    let current = lang.signal();
    view! {
        <div class="langs" role="group" aria-label=move || t(current.get(), Key::Language)>
            {Lang::ALL
                .into_iter()
                .map(|option| {
                    let chosen = move || current.get() == option;
                    view! {
                        // `lang` у кнопки: имя языка диктор читает на нём самом.
                        <button
                            type="button"
                            lang=option.code()
                            class:secondary=move || !chosen()
                            aria-pressed=move || if chosen() { "true" } else { "false" }
                            on:click=move |_| lang.choose(option)
                        >
                            {option.native_name()}
                        </button>
                    }
                })
                .collect_view()}
        </div>
    }
}
