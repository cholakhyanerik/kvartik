//! Меню разделов (дизайн «Kvartik Design»): на телефоне — вкладки внизу
//! экрана, на компьютере — колонка слева со знаком программы, «Новым
//! счётом» и меткой копии. Разметка одна, раскладку меняет CSS
//! (`base.css`, `.nav`).
//!
//! Вкладка — ссылка на адрес экрана (`ui/route.rs`): переход делает сам
//! браузер, и «Назад» возвращает прежний экран. Открытая вкладка —
//! `aria-current="page"`, жирнее и цветом акцента: не одним цветом
//! (Правило 4).

use kvartik_core::i18n::{Key, Lang, t};
use leptos::prelude::*;

use super::backup::{BackupChip, Backups};
use super::clients::Clients;
use super::documents::Documents;
use super::icon::{Icon, icon};
use super::route::{Router, Screen};
use super::tr;

/// Разделы меню: экран, значок и подпись. «Налог» появится с расчётом
/// налога (строка плана 0.2): пустой вкладки, которая обещает расчёт, нет.
const TABS: [(Screen, Icon, Key); 4] = [
    (Screen::Home, Icon::Home, Key::NavHome),
    (Screen::Documents, Icon::Document, Key::NavDocuments),
    (Screen::Clients, Icon::Clients, Key::ClientsTitle),
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
                <span>{tr(lang, Key::DocNewInvoice)}</span>
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
            <BackupChip lang backups />
        </nav>
    }
}
