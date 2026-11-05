//! Главный экран (дизайн «Kvartik Design»): какой сегодня день и что ждёт
//! внимания — копия, установка, первые шаги, сроки, расчётные документы
//! ՊԵԿ, налог квартала, незаполненный профиль, неоплаченные счета, — и главное
//! действие, «Новый счёт».

use kvartik_core::i18n::{self, Key, Lang};
use leptos::prelude::*;

use super::backup::{BackupReminder, Backups};
use super::clients::Clients;
use super::documents::{Awaiting, Documents};
use super::icon::{Icon, icon};
use super::onboarding::{Card, FirstSteps, HandoffOffer, Steps};
use super::profile::Profiles;
use super::pwa::{InstallSection, Program};
use super::route::{Router, Screen};
use super::tax::{self, TaxCard};
use super::tr;
use crate::platform::clock;

/// Главный экран на языке окна.
#[component]
pub fn Home(
    lang: ReadSignal<Lang>,
    router: Router,
    program: Program,
    backups: Backups,
    profiles: Profiles,
    documents: Documents,
    clients: Clients,
    tax: tax::Ctx,
    /// Стоит ли ещё экран первого запуска.
    welcome: RwSignal<bool>,
) -> impl IntoView {
    // Сегодня — один раз (Архитектура, «Время — параметр»).
    let today = clock::today();
    let steps = Steps::new();
    let card = Card {
        lang,
        router,
        steps,
        profiles,
        clients,
        documents,
    };
    let new_invoice = move |_| {
        documents.new_invoice(clients);
        router.go(Screen::Documents);
    };
    view! {
        <header class="greeting">
            {today.map(|day| view! {
                <p class="today">{move || i18n::day_long(lang.get(), day)}</p>
            })}
            <h1 tabindex="-1">{tr(lang, Key::HomeHello)}</h1>
        </header>
        <BackupReminder lang backups />
        // Установка — выше всего о данных: на iPhone программу ставят до
        // того, как вводить данные (Правило 6).
        <InstallSection lang program />
        // Введённое в калькуляторе лендинга — до первых шагов: первый шаг —
        // тот же профиль.
        <HandoffOffer card welcome />
        <FirstSteps card welcome />
        <TaxCard ctx=tax />
        <Show when=move || profiles.missing() && steps.hidden()>
            <div class="notice">
                <p>{tr(lang, Key::HomeProfileMissing)}</p>
                <a class="button" href=Screen::Profile.href()>{tr(lang, Key::ProfileFill)}</a>
            </div>
        </Show>
        <Awaiting lang documents clients />
        <button type="button" class="wide" on:click=new_invoice>
            {icon(Icon::Plus)}
            <span>{tr(lang, Key::DocNewInvoice)}</span>
        </button>
    }
}
