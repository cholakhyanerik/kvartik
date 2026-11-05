//! Поля форм — общие у профиля (задача 9) и клиента (задача 10): подпись
//! над полем, подсказка под ним и ошибка словами после первой попытки
//! сохранить. И шапка форм по шагам — у нового клиента и документа, и счёт
//! открытых форм ([`Forms`]) — его ждёт новая версия.
//!
//! Поля держат строки, как их ввёл человек, а читает их ядро (`new`
//! и `parse` у каждого типа). Окно только показывает, что не так.

use kvartik_core::i18n::{self, Key, Lang, fill, t};
use kvartik_core::model::money::Currency;
use kvartik_core::model::text::{Line, Lines, TextError};
use leptos::prelude::*;

use super::icon::{Icon, icon};
use super::tr;

/// Сколько форм окна открыто сейчас — одно на окно (задача 67). Новая версия
/// ждёт, пока закроются все: она включается сама, а её перезагрузка стёрла бы
/// начатое (решения человека 2026-10-18 и 2026-11-01). Форма отмечается сама
/// ([`Forms::hold`]) — и та, что живёт в части wasm, — а поля вне окна формы
/// — пока в них что-то начато ([`Forms::hold_while`]).
#[derive(Clone, Copy)]
pub struct Forms(RwSignal<usize>);

impl Forms {
    pub fn new() -> Self {
        Self(RwSignal::new(0))
    }

    /// Форма открылась — и считается открытой, пока жив её вид: закрытую
    /// Leptos снимает со страницы, и вместе с ней уходит отметка. Звать
    /// в начале компонента формы. Окно, снятое целиком (браузерные тесты),
    /// раньше снимает сам счёт: тогда отметке уходить некуда, и `update`
    /// снятого сигнала молчит.
    pub fn hold(self) {
        self.0.update(|open| *open += 1);
        on_cleanup(move || self.0.update(|open| *open = open.saturating_sub(1)));
    }

    /// Считаться открытой формой, пока `started` — отслеживаемое условие —
    /// истинно: у полей карточки документа, ключа лицензии и копии нет формы,
    /// которая открывается и закрывается, а начатое в них перезагрузка стёрла
    /// бы так же. Звать в начале компонента. Снимается и вместе с ним.
    pub fn hold_while(self, started: impl Fn() -> bool + Send + Sync + 'static) {
        let started = Memo::new(move |_| started());
        Effect::new(move |_| {
            if started.get() {
                self.hold();
            }
        });
    }

    /// Открыта ли хоть одна — отслеживаемое.
    pub fn any(self) -> bool {
        self.0.get() > 0
    }

    /// Открыта ли хоть одна сейчас — в обработчике нажатия, без слежения:
    /// переход к другому ИП перезагружает страницу (`ui/office.rs`).
    pub fn busy(self) -> bool {
        self.0.get_untracked() > 0
    }
}

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

/// Шапка формы по шагам (дизайн «Kvartik Design»): «назад» кружком,
/// название формы, «1 / 2» и полоски шагов. «Назад» на первом шаге —
/// выход из формы (`leave`). Номер шага — и словами для диктора: «шаг 1
/// из 2». Новый шаг — с начала страницы.
#[component]
pub(super) fn WizardHead<L>(
    lang: ReadSignal<Lang>,
    title: Key,
    step: RwSignal<u8>,
    steps: u8,
    leave: L,
) -> impl IntoView
where
    L: Fn() + Copy + Send + Sync + 'static,
{
    Effect::watch(
        move || step.get(),
        |_, _, _| {
            if let Some(window) = web_sys::window() {
                window.scroll_to_with_x_and_y(0.0, 0.0);
            }
        },
        false,
    );
    let numbers = move |lang: Lang| {
        [
            i18n::integer(lang, i64::from(step.get())),
            i18n::integer(lang, i64::from(steps)),
        ]
    };
    view! {
        <div class="wizard-head">
            <button
                type="button"
                class="round"
                aria-label=move || t(lang.get(), Key::Back)
                on:click=move |_| {
                    if step.get_untracked() > 1 {
                        step.update(|step| *step -= 1);
                    } else {
                        leave();
                    }
                }
            >
                {icon(Icon::Back)}
            </button>
            <p class="wizard-title">{tr(lang, title)}</p>
            <p class="wizard-step">
                <span aria-hidden="true">{move || {
                    let lang = lang.get();
                    let [now, all] = numbers(lang);
                    fill(t(lang, Key::StepShort), &[&now, &all])
                }}</span>
                <span class="visually-hidden">{move || {
                    let lang = lang.get();
                    let [now, all] = numbers(lang);
                    fill(t(lang, Key::StepOf), &[&now, &all])
                }}</span>
            </p>
        </div>
        <div class="steps" aria-hidden="true">
            {(1..=steps)
                .map(|n| view! { <span class:done={move || step.get() >= n}></span> })
                .collect_view()}
        </div>
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
