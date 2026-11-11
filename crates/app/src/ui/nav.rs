//! Меню разделов (дизайн «Kvartik Design»): на телефоне — вкладки внизу
//! экрана, на компьютере — колонка слева со знаком программы, «Новым
//! счётом» и меткой копии. Разметка одна, раскладку меняет CSS
//! (`base.css`, `.nav`).
//!
//! Вкладка — ссылка на адрес экрана (`ui/route.rs`): переход делает сам
//! браузер, и «Назад» возвращает прежний экран. Открытая вкладка —
//! `aria-current="page"`, жирнее и цветом акцента: не одним цветом
//! (Правило 4).
//!
//! Настройки (задача 88) — не вкладкой, а своей строкой с шестерёнкой внизу
//! колонки, над метками срока и копии: это не раздел с данными. На телефоне
//! её нет — седьмая вкладка не помещается в 360 точек, — там шестерёнка на
//! главном экране (`ui/home.rs`) и строка в профиле. Метки срока лицензии
//! (задача 92) и копии на телефоне — на главном экране, а когда пора, —
//! над экранами и под заголовками.

use kvartik_core::i18n::{Key, Lang, t};
use leptos::prelude::*;

use super::backup::{BackupChip, Backups};
use super::clients::Clients;
use super::documents::Documents;
use super::icon::{Icon, icon};
use super::license::{LicenseChip, Licenses, Place};
use super::route::{Router, Screen};
use super::tr;

/// Разделы меню: экран, значок и подпись. Книга — пятой вкладкой (решение
/// человека 2026-10-15, задача 14), налог — перед профилем, как в дизайне
/// «Kvartik Design» (экран 10; задача 19).
const TABS: [(Screen, Icon, Key); 6] = [
    (Screen::Home, Icon::Home, Key::NavHome),
    (Screen::Documents, Icon::Document, Key::NavDocuments),
    (Screen::Ledger, Icon::Book, Key::NavLedger),
    (Screen::Clients, Icon::Clients, Key::ClientsTitle),
    (Screen::Tax, Icon::Percent, Key::TaxTitle),
    (Screen::Profile, Icon::Person, Key::NavProfile),
];

/// Меню на языке окна.
#[component]
pub fn Nav(
    lang: ReadSignal<Lang>,
    router: Router,
    documents: Documents,
    clients: Clients,
    backups: Backups,
    licenses: Licenses,
) -> impl IntoView {
    // «Новый счёт» — главное действие программы, и на компьютере оно
    // в меню, на любом экране. Без клиента откроется раздел документов
    // со словами, что сначала нужен клиент.
    let new_invoice = move |_| {
        documents.new_invoice(clients);
        router.go(Screen::Documents);
    };
    view! {
        <nav class="nav" aria-label=move || t(lang.get(), Key::NavSections)>
            <a class="brand" href=Screen::Home.href()>{tr(lang, Key::Brand)}</a>
            <button type="button" class="new" on:click=new_invoice>
                {icon(Icon::Plus)}
                <span class="label">{tr(lang, Key::DocNewInvoice)}</span>
            </button>
            <ul class="tabs">
                {TABS
                    .into_iter()
                    .map(|(screen, glyph, key)| view! {
                        <li>
                            <a
                                href=screen.href()
                                aria-current=move || (router.screen().section() == screen).then_some("page")
                            >
                                {icon(glyph)}
                                <span>{tr(lang, key)}</span>
                            </a>
                        </li>
                    })
                    .collect_view()}
            </ul>
            <a
                class="settings-link"
                href=Screen::Settings.href()
                aria-current=move || (router.screen() == Screen::Settings).then_some("page")
            >
                {icon(Icon::Gear)}
                <span>{tr(lang, Key::SettingsTitle)}</span>
            </a>
            <LicenseChip lang licenses place=Place::Menu />
            <BackupChip lang backups />
        </nav>
    }
}
