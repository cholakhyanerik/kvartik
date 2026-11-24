//! Поля пароля копий (задача 124): одной разметкой у копии, автокопии,
//! восстановления и «Другого устройства».
//!
//! Пароль копий — фраза не короче `backup::MIN_PASSWORD_CHARS` знаков, и её
//! вводят на каждом устройстве. Менеджер паролей браузера сохранит её
//! и подставит, только если в форме есть логин: форму с одним паролем он
//! за вход не считает (по браузеру: chromium.org, «Password Form Styles that
//! Chromium Understands», сверено 2026-11-24). Поэтому в форме пароля копий —
//! спрятанное поле логина с постоянным значением, одним на всех языках:
//! иначе у человека вышло бы три записи. Спрятано оно атрибутом `hidden`:
//! так его понимает Chromium (там же), а фокус и диктор на него не встают.
//!
//! У копии для клиента логина нет: её пароль разовый, и менеджер не должен
//! записать его поверх пароля копий.
//!
//! Сохранить пароль менеджер предлагает, только когда видит, что форма
//! отправлена: она пропадает со страницы (там же, «Create Amazing Password
//! Forms»). Формы окна гасят отправку и остаются на странице, поэтому каждая
//! форма пароля копий после успеха прячется или сменяется словами — это
//! держат сами формы. Вживую предложение менеджера в Chrome, Safari и Firefox
//! не проверено: в headless-браузере его нет.
//!
//! Компоненты без обобщений: одна копия кода на все части wasm, которые их
//! зовут (Правило 1).

use kvartik_core::i18n::{Key, Lang};
use leptos::ev::SubmitEvent;
use leptos::prelude::*;
use wasm_bindgen::JsCast;
// `autocorrect` (Safari) нет в списке атрибутов Leptos.
use leptos::tachys::html::attribute::custom::custom_attribute;

use super::tr;

/// Логин записи пароля копий в менеджере паролей. Не слово окна: человек
/// видит его только в списке менеджера, и он один на всех языках.
const LOGIN: &str = "Kvartik";

/// Спрятанное поле логина — первым в форме пароля копий.
#[component]
pub fn PasswordLogin() -> impl IntoView {
    view! { <input type="text" name="username" autocomplete="username" value=LOGIN hidden /> }
}

/// Поле пароля с подписью над ним. `fresh` — новый пароль
/// (`new-password`): менеджер предложит придумать его сам; иначе —
/// заданный раньше. `shown` — набранное видно буквами ([`PasswordShow`]):
/// тогда поле текстовое, и у видимой фразы клавиатура телефона не должна
/// ставить заглавную и править слова — они вошли бы в пароль.
#[component]
pub fn PasswordInput(
    lang: ReadSignal<Lang>,
    label: Key,
    value: RwSignal<String>,
    shown: RwSignal<bool>,
    #[prop(optional)] fresh: bool,
) -> impl IntoView {
    view! {
        <label>
            {tr(lang, label)}
            <input
                type=move || if shown.get() { "text" } else { "password" }
                autocomplete=if fresh { "new-password" } else { "current-password" }
                bind:value=value
                autocapitalize="none"
                spellcheck="false"
                {..custom_attribute("autocorrect", "off")}
            />
        </label>
    }
}

/// Перед отправкой формы — снова точки (ревью задачи 124): при отмеченном
/// «Показать пароль» поле текстовое, и браузер запомнил бы фразу как обычный
/// ввод (GOV.UK Design System, «Password input»: при отправке вернуть тип
/// `password`; сверено 2026-11-24). Флажок снимается, а тип ставится и прямо
/// в DOM: Leptos сменит его следующей микрозадачей (`RenderEffect`, по
/// исходникам reactive_graph 0.2.16), а браузер смотрит на поля в самом
/// `submit`. Звать первой строкой обработчика отправки.
pub fn conceal(event: &SubmitEvent, shown: RwSignal<bool>) {
    shown.set(false);
    let Some(form) = event.target().and_then(|target| target.dyn_into::<web_sys::Element>().ok()) else {
        return;
    };
    while let Ok(Some(field)) = form.query_selector(r#"input[type="text"][autocomplete$="password"]"#) {
        if field.set_attribute("type", "password").is_err() {
            break;
        }
    }
}

/// «Показать пароль» — флажком под полями формы: один на все её поля,
/// и пароль с повтором видно разом.
#[component]
pub fn PasswordShow(lang: ReadSignal<Lang>, shown: RwSignal<bool>) -> impl IntoView {
    view! {
        <label class="choice">
            <input
                type="checkbox"
                prop:checked=move || shown.get()
                on:change=move |event| shown.set(event_target_checked(&event))
            />
            {tr(lang, Key::PasswordShow)}
        </label>
    }
}
