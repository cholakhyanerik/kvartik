//! Поля форм — общие у профиля (задача 9) и клиента (задача 10): подпись
//! над полем, подсказка под ним и ошибка словами после первой попытки
//! сохранить.
//!
//! Поля держат строки, как их ввёл человек, а читает их ядро (`new`
//! и `parse` у каждого типа). Окно только показывает, что не так.

use kvartik_core::i18n::{Key, Lang};
use kvartik_core::model::money::Currency;
use kvartik_core::model::text::{Line, Lines, TextError};
use leptos::prelude::*;

use super::tr;

/// Необязательное поле: пусто — `Some(None)`, прочитано — `Some(Some(…))`,
/// ошибка — `None`.
pub(super) fn optional<T, E>(
    text: &str,
    parse: impl Fn(&str) -> Result<T, E>,
) -> Option<Option<T>> {
    if text.trim().is_empty() {
        Some(None)
    } else {
        parse(text).ok().map(Some)
    }
}

/// Ошибка однострочного текста — словами.
pub(super) fn line_problem(text: &str) -> Option<Key> {
    Line::new(text).err().map(text_key)
}

fn lines_problem(text: &str) -> Option<Key> {
    Lines::new(text).err().map(text_key)
}

const fn text_key(error: TextError) -> Key {
    match error {
        TextError::TooLong { .. } => Key::TextTooLong,
    }
}

/// Подпись и поле в одну строку.
#[component]
pub(super) fn TextField(
    lang: ReadSignal<Lang>,
    label: Key,
    value: RwSignal<String>,
    tried: RwSignal<bool>,
    #[prop(default = "text")] kind: &'static str,
    #[prop(optional)] autocomplete: Option<&'static str>,
    #[prop(optional)] inputmode: Option<&'static str>,
    #[prop(optional)] hint: Option<Key>,
    #[prop(default = Line::MAX_CHARS)] max: usize,
    #[prop(default = line_problem)] check: fn(&str) -> Option<Key>,
) -> impl IntoView {
    view! {
        <label>
            {tr(lang, label)}
            <input
                type=kind
                bind:value=value
                autocomplete=autocomplete
                inputmode=inputmode
                maxlength=max.to_string()
            />
        </label>
        {hint.map(|hint| view! { <p class="hint">{tr(lang, hint)}</p> })}
        <FieldError lang tried error=move || value.with(|text| check(text)) />
    }
}

/// Подпись и многострочное поле.
#[component]
pub(super) fn AreaField(
    lang: ReadSignal<Lang>,
    label: Key,
    value: RwSignal<String>,
    tried: RwSignal<bool>,
    #[prop(optional)] autocomplete: Option<&'static str>,
    #[prop(optional)] hint: Option<Key>,
) -> impl IntoView {
    view! {
        <label>
            {tr(lang, label)}
            <textarea
                rows="3"
                bind:value=value
                autocomplete=autocomplete
                maxlength=Lines::MAX_CHARS.to_string()
            ></textarea>
        </label>
        {hint.map(|hint| view! { <p class="hint">{tr(lang, hint)}</p> })}
        <FieldError lang tried error=move || value.with(|text| lines_problem(text)) />
    }
}

/// Ошибка поля — словами, после первой попытки сохранить.
///
/// Обобщён замыканием, и у каждого места вызова — своя копия кода. Вариант
/// без обобщений — ошибка `Signal::derive`, выбор `Callback` у `Choice` —
/// замерен: wasm после brotli на 0,7 КБ больше, а не меньше (0.0.12,
/// Правило 1).
#[component]
pub(super) fn FieldError<F>(lang: ReadSignal<Lang>, tried: RwSignal<bool>, error: F) -> impl IntoView
where
    F: Fn() -> Option<Key> + Send + Sync + 'static,
{
    move || {
        tried
            .get()
            .then(&error)
            .flatten()
            .map(|key| view! { <p class="field-error" role="alert">{tr(lang, key)}</p> })
    }
}

/// Слова валюты в списке выбора.
pub(super) const fn currency_key(currency: Currency) -> Key {
    match currency {
        Currency::Amd => Key::CurrencyAmd,
        Currency::Usd => Key::CurrencyUsd,
        Currency::Eur => Key::CurrencyEur,
    }
}

/// Валюты, в которых бывают счета, — в порядке списка выбора.
pub(super) const CURRENCIES: [Currency; 3] = [Currency::Amd, Currency::Usd, Currency::Eur];

/// Код валюты для `value` у `<option>` — ISO 4217, как в данных.
pub(super) const fn currency_code(currency: Currency) -> &'static str {
    match currency {
        Currency::Amd => "AMD",
        Currency::Usd => "USD",
        Currency::Eur => "EUR",
    }
}
