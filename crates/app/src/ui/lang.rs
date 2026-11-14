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
    /// язык страницы сайта, с которой пришли (`/app/?lang=ru`, полный прогон
    /// 2026-11-10, п. 11), а нет его — по языкам браузера (`Lang::preferred`).
    /// Язык страницы сайта только предлагается: запоминает его экран первого
    /// запуска — «Продолжить» или переключатель ([`Self::keep`]).
    ///
    /// Сильнее всех — язык, выбранный переключателем на страницах сайта после
    /// выбора здесь (`Device::site_lang`, 0.0.125): выбрал человек на
    /// лендинге English — программа открывалась языком, выбранным в ней
    /// раньше. Выбранный на сайте окно берёт, запоминает и снимает; при
    /// первом запуске — только предлагает, как язык страницы. Язык страницы
    /// без переключателя выбора здесь не меняет: корень сайта армянский у
    /// каждого, кто набрал kvartik.com.
    ///
    /// Читается синхронно, до первой отрисовки, — окно не мигает чужим
    /// языком, — и сразу ставится у страницы: паника в первой же отрисовке
    /// скажет на нём (`ui::panic`).
    pub fn start() -> Self {
        let device = Device::new();
        let site = device.site_lang();
        let lang = match (device.ui_lang(), site) {
            (Some(_), Some(site)) => {
                device.set_ui_lang(site);
                device.clear_site_lang();
                site
            }
            (Some(remembered), None) => remembered,
            (None, site) => site
                .or_else(|| platform::address_lang().as_deref().and_then(Lang::from_code))
                .unwrap_or_else(|| {
                    let browser = platform::languages();
                    Lang::preferred(browser.iter().map(String::as_str))
                }),
        };
        mark_page(lang);
        Self(RwSignal::new(lang))
    }

    /// Язык, выбранный на этом устройстве раньше; `None` — не выбирали:
    /// тогда окно открывается экраном первого запуска (`ui::onboarding`).
    pub fn remembered() -> Option<Lang> {
        Device::new().ui_lang()
    }

    /// Язык окна для разметки: каждая строка — производное от него
    /// (Правило 4).
    pub fn signal(self) -> ReadSignal<Lang> {
        self.0.read_only()
    }

    /// Человек выбрал язык: всё окно переводится сразу, без перезагрузки.
    /// Не запомнилось (`localStorage` отказал) — окно всё равно на новом
    /// языке, а в следующий раз откроется по языкам браузера. Выбор здесь
    /// новее выбранного на страницах сайта: тот снимается, и следующее
    /// открытие его не возьмёт.
    pub fn choose(self, lang: Lang) {
        let device = Device::new();
        device.clear_site_lang();
        if self.0.get_untracked() == lang {
            return;
        }
        device.set_ui_lang(lang);
        mark_page(lang);
        self.0.set(lang);
    }

    /// Запомнить язык окна как выбранный — и тот, что предложил браузер или
    /// сайт: «Продолжить» на экране первого запуска — тоже выбор.
    pub fn keep(self) {
        let device = Device::new();
        device.set_ui_lang(self.0.get_untracked());
        device.clear_site_lang();
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
                            <span class="label">{option.native_name()}</span>
                        </button>
                    }
                })
                .collect_view()}
        </div>
    }
}
