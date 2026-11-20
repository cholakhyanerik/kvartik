//! Главный экран (дизайн «Kvartik Design»): какой сегодня день, сколько дней
//! осталось пробного периода или ключа (задача 92; на компьютере — в меню),
//! и что ждёт внимания — копия, установка, первые шаги, сроки, расчётные
//! документы ՊԵԿ, налог квартала, незаполненный профиль, неоплаченные счета, —
//! и главное действие, «Новый счёт».

use kvartik_core::i18n::{self, Key, Lang, t};
use leptos::prelude::*;

use super::backup::{BackupReminder, Backups};
use super::clients::Clients;
use super::documents::{Awaiting, Documents};
use super::icon::{Icon, icon};
use super::license::{LicenseChip, Place};
use super::office::Office;
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
    /// ИП в программе: первые шаги — у каждого свои (задача 38).
    office: Office,
) -> impl IntoView {
    // Сегодня — один раз (Архитектура, «Время — параметр»).
    let today = clock::today();
    let steps = Steps::new(office);
    let card = Card {
        lang,
        router,
        steps,
        profiles,
        clients,
        documents,
        licenses: tax.licenses,
    };
    let new_invoice = move |_| {
        documents.new_invoice(clients);
        router.go(Screen::Documents);
    };
    // Установка на iPhone, iPad и Mac — выше всего о данных: программа
    // с экрана «Домой» данных браузера не видит (Правило 6). У остальных —
    // ниже карточек налога, сроков и счетов: на телефоне она стояла сверху
    // и отодвигала их (полный прогон 2026-11-10, п. 21). С главного экрана
    // она не уходит (Правило 2).
    let first = program.install_first();
    view! {
        <header class="greeting">
            <div>
                {today.map(|day| view! {
                    <p class="today">{move || i18n::day_long(lang.get(), day)}</p>
                })}
                <h1 tabindex="-1">{tr(lang, Key::HomeHello)}</h1>
            </div>
            // Настройки на телефоне (задача 88): вкладки для них внизу нет.
            // На компьютере их строка — в меню, и шестерёнки здесь нет.
            <a
                class="settings-open"
                href=Screen::Settings.href()
                aria-label=move || t(lang.get(), Key::SettingsTitle)
            >
                {icon(Icon::Gear)}
            </a>
        </header>
        <LicenseChip lang licenses=tax.licenses place=Place::Home />
        <BackupReminder lang backups />
        {first.then(|| view! { <InstallSection lang program /> })}
        // Введённое в калькуляторе лендинга — до первых шагов: первый шаг —
        // тот же профиль.
        <HandoffOffer card welcome />
        <FirstSteps card welcome />
        <TaxCard ctx=tax />
        <Show when=move || profiles.missing() && steps.hidden()>
            <div class="notice">
                <p>{tr(lang, Key::HomeProfileMissing)}</p>
                <a class="button" href=Screen::Profile.href()><span class="label">{tr(lang, Key::ProfileFill)}</span></a>
            </div>
        </Show>
        <Awaiting lang documents clients />
        {(!first).then(|| view! { <InstallSection lang program /> })}
        <button type="button" class="wide" on:click=new_invoice>
            {icon(Icon::Plus)}
            <span class="label">{tr(lang, Key::DocNewInvoice)}</span>
        </button>
    }
}
