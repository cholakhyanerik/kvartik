//! Курс ЦБ у поля курса (задача 43): какой курс ЦБ берёт закон для дня
//! поставки — «Подставить» у формы документа, карточки выпущенного
//! документа и формы книги, а у карточки без курса ещё и «Указать курс ЦБ»
//! одним нажатием. Какой день — решает ядро (`kvartik_core::tax::cba`),
//! подставленный курс — такой же курс человека, как набранный руками, но
//! в форме он следует за днём и валютой, пока его не поправили.
//!
//! Курсы — файл своего сайта (`cba::FILE`), его кладёт выкладка, а окно
//! помнит последний на устройстве (`Device::rates`): подсказка есть и без
//! сети. Спрашивает его окно при открытии — загрузка частей (`split::preload`)
//! зовёт [`start`] у всех одинаково, есть у человека суммы в валюте или нет,
//! — и при возвращении к окну, не чаще раза в час, как о новой версии
//! (`platform::pwa`). Запрос, который уходил бы только от тех, у кого суммы
//! в валюте, говорил бы о данных человека хостингу (ревью задачи 43;
//! политика конфиденциальности, п. 3: запросы — при открытии и проверке
//! новой версии). Код — в частях документов и книги: главному wasm он не
//! нужен (Правило 1).

use std::cell::Cell;
use std::sync::Arc;

use kvartik_core::i18n::{self, Key, Lang, fill, t};
use kvartik_core::model::date::Date;
use kvartik_core::model::money::Currency;
pub(super) use kvartik_core::tax::cba::Hint;
use kvartik_core::tax::cba::{self, Rates};
use leptos::prelude::*;

use crate::store::device::Device;

/// Как часто спрашивать сайт о курсах: раз в час, как о новой версии.
const REFRESH_MS: f64 = 60.0 * 60.0 * 1000.0;

/// Курсы на устройстве; `None` — их нет или запись не читается.
type Shared = ArcRwSignal<Option<Arc<Rates>>>;

thread_local! {
    static RATES: Shared = ArcRwSignal::new(
        Device::new().rates().and_then(|text| Rates::parse(&text).ok()).map(Arc::new),
    );
    /// Слушатель возвращения к окну поставлен.
    static STARTED: Cell<bool> = const { Cell::new(false) };
    /// Когда окно в последний раз спрашивало сайт, мс: не ответил — следующий
    /// раз через час, а не на каждую подсказку.
    static ASKED: Cell<f64> = const { Cell::new(f64::NEG_INFINITY) };
}

/// Курсы на устройстве — сигналом.
fn rates() -> Shared {
    RATES.with(Clone::clone)
}

/// Начать обновлять курсы: спросить сайт сейчас, если курсам больше часа,
/// и при каждом возвращении к окну — не чаще раза в час. Зовёт загрузка
/// частей при открытии программы, у всех (шапка модуля); второй вызов на
/// странице ничего не делает.
pub(super) fn start() {
    if !STARTED.replace(true) {
        crate::platform::when_shown(refresh);
        refresh();
    }
}

/// Спросить сайт о курсах, если курсам на устройстве больше часа и окно
/// не спрашивало его в этот час. Не ответил или ответил не тем — остаются
/// прежние. В браузерных тестах сайта с курсами нет: курсы кладёт тест
/// ([`set_for_tests`]).
fn refresh() {
    let now = js_sys::Date::now();
    let fresh = Device::new()
        .rates_at()
        .is_some_and(|at| now - (at.unix_millis() as f64) < REFRESH_MS);
    if fresh || now - ASKED.get() < REFRESH_MS || cfg!(all(test, target_arch = "wasm32")) {
        return;
    }
    ASKED.set(now);
    leptos::task::spawn_local(fetch());
}

/// Взять курсы с сайта и запомнить на устройстве.
async fn fetch() {
    let Some(text) = crate::platform::site::optional_text(cba::FILE).await else {
        return;
    };
    let Ok(parsed) = Rates::parse(&text) else {
        return;
    };
    let device = Device::new();
    device.set_rates(&text);
    device.set_rates_at(crate::platform::clock::now());
    RATES.with(|rates| rates.set(Some(Arc::new(parsed))));
}

/// Курсы, которые видит окно, — для браузерных тестов.
#[cfg(all(test, target_arch = "wasm32"))]
pub(super) fn set_for_tests(rates: Option<Rates>) {
    RATES.with(|shared| shared.set(rates.map(Arc::new)));
}

/// Подсказка для валюты `currency` и дня поставки `day` — производное от
/// курсов на устройстве.
pub(super) fn hint(currency: Signal<Option<Currency>>, day: Signal<Option<Date>>) -> Memo<Option<Hint>> {
    let rates = rates();
    Memo::new(move |_| {
        let (currency, day) = (currency.get()?, day.get()?);
        rates.with(|rates| cba::hint(rates.as_deref(), currency, day))
    })
}

/// Подсказка под полем курса: курс ЦБ, который берёт закон для дня
/// поставки, и «Подставить», когда в поле другой; курса нет — почему.
/// `field` — текст поля курса.
#[component]
pub(super) fn CbaHint(
    lang: ReadSignal<Lang>,
    currency: Signal<Option<Currency>>,
    day: Signal<Option<Date>>,
    field: RwSignal<String>,
) -> impl IntoView {
    let hint = hint(currency, day);
    // Курс, который подставила подсказка. Сменились день или валюта, а в
    // поле всё ещё он — он следует за подсказкой: курс нового дня, а нет
    // его — пустое поле, а не курс чужого дня или валюты, похожий на курс
    // человека (ревью задачи 43). Набранный руками курс окно не трогает.
    let filled = RwSignal::new(None::<i64>);
    Effect::new(move |_| {
        let now = match hint.get() {
            Some(Hint::Rate { per_unit_e4, .. }) => Some(per_unit_e4),
            Some(Hint::NotYet { .. } | Hint::Missing { .. }) | None => None,
        };
        let Some(before) = filled.get_untracked() else {
            return;
        };
        let lang = lang.get_untracked();
        if field.with_untracked(|text| i18n::parse_rate(lang, text).ok()) != Some(before) {
            filled.set(None);
            return;
        }
        if now != Some(before) {
            field.set(now.map(|rate| i18n::rate(lang, rate)).unwrap_or_default());
            filled.set(now);
        }
    });
    move || {
        let lang = lang.get();
        let currency = currency.get()?;
        Some(match hint.get()? {
            Hint::Rate {
                published,
                per_unit_e4,
            } => {
                let pair = i18n::rate_pair(lang, currency, per_unit_e4);
                let said = fill(t(lang, Key::CbaRate), &[&i18n::date(lang, published), &pair]);
                let typed = field.with(|text| i18n::parse_rate(lang, text).ok());
                view! {
                    <p class="hint">{said}</p>
                    {(typed != Some(per_unit_e4)).then(|| view! {
                        <button
                            type="button"
                            class="link"
                            on:click=move |_| {
                                field.set(i18n::rate(lang, per_unit_e4));
                                filled.set(Some(per_unit_e4));
                            }
                        >
                            <span class="label">{t(lang, Key::CbaRateUse)}</span>
                        </button>
                    })}
                }
                .into_any()
            }
            Hint::NotYet { published } => {
                view! { <p class="hint">{fill(t(lang, Key::CbaRateNotYet), &[&i18n::date(lang, published)])}</p> }
                    .into_any()
            }
            Hint::Missing { published } => {
                view! { <p class="hint">{fill(t(lang, Key::CbaRateMissing), &[&i18n::date(lang, published)])}</p> }
                    .into_any()
            }
        })
    }
}
