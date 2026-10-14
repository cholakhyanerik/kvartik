//! Первый запуск (задача 28; дизайн «Kvartik Design», экран 01): что это
//! за программа, выбор языка окна и «У меня есть копия».
//!
//! Экран стоит, пока язык окна на этом устройстве не выбран. Окно под ним
//! уже открывает базу и готовит экраны, только спрятанным: после
//! «Продолжить» оно открывается сразу, а не начинает с начала. Дальше задача
//! 28 продолжит его: установка на iPhone, профиль, первый клиент и счёт.

use kvartik_core::i18n::{Key, Lang};
use leptos::prelude::*;

use super::icon::{Icon, icon};
use super::lang::WindowLang;
use super::route::{self, Router, Screen};
use super::tr;

/// Нужен ли экран первого запуска: язык окна на устройстве ещё не выбран.
pub fn needed() -> bool {
    WindowLang::remembered().is_none()
}

/// Экран первого запуска. Язык выбирается сразу: экран переводится на
/// выбранный, и выбор запоминается (Правило 2). «Продолжить» запоминает
/// и язык, который предложил браузер, — экран больше не появится.
/// «У меня есть копия» ведёт к восстановлению из файла копии.
#[component]
pub fn Welcome(window_lang: WindowLang, open: RwSignal<bool>, router: Router) -> impl IntoView {
    let lang = window_lang.signal();
    let close = move |screen: Option<Screen>| {
        window_lang.keep();
        open.set(false);
        match screen {
            Some(screen) => router.go(screen),
            // Окно уже на своём экране: показать его с начала и с фокусом
            // на заголовке, как при переходе.
            None => route::shown(untrack(|| router.screen())),
        }
    };
    view! {
        <section class="welcome">
            <div class="welcome-decor" aria-hidden="true">
                <span></span>
                <span></span>
            </div>
            <div class="welcome-body">
                <p class="brand">{tr(lang, Key::Brand)}</p>
                <h1>{tr(lang, Key::WelcomeHeadline)}</h1>
                <p class="lead">{tr(lang, Key::WelcomeAbout)}</p>
                <fieldset class="welcome-langs">
                    <legend>{tr(lang, Key::WelcomeLang)}</legend>
                    {Lang::ALL
                        .into_iter()
                        .map(|option| view! {
                            // `lang` у варианта: имя языка диктор читает на
                            // нём самом.
                            <label class="lang-option" lang=option.code()>
                                <input
                                    type="radio"
                                    name="welcome-lang"
                                    class="visually-hidden"
                                    prop:checked=move || lang.get() == option
                                    on:change=move |_| window_lang.choose(option)
                                />
                                <span>{option.native_name()}</span>
                                <span class="radio-mark" aria-hidden="true">{icon(Icon::Check)}</span>
                            </label>
                        })
                        .collect_view()}
                </fieldset>
                <button type="button" class="wide" on:click=move |_| close(None)>
                    {tr(lang, Key::WelcomeContinue)}
                </button>
                <button type="button" class="link" on:click=move |_| close(Some(Screen::Backup))>
                    {tr(lang, Key::WelcomeHaveBackup)}
                </button>
            </div>
        </section>
    }
}
