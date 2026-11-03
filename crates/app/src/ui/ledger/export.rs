//! Выгрузка для бухгалтера (задача 30) — под списком книги: квартал книги
//! или весь его год файлом XLSX (два листа: доходы и расходы, документы)
//! и двумя CSV. Листы и байты файлов собирает ядро
//! (`kvartik_core::format::export`), здесь — выбор периода и ссылки.
//!
//! Файл собирается в самом нажатии на ссылку, как файл сроков
//! (`ui/deadlines.rs`): он нужен раз в квартал, и собирать его на каждое
//! изменение данных — лишняя работа (CLAUDE.md, Правило 1). Сборка
//! синхронная — квартал это десятки строк, год торговли — тысячи, — и ссылка
//! получает адрес файла ещё в обработчике нажатия: переход по ней браузер
//! делает после обработчиков (по спецификации HTML, «activation behavior»
//! у `a`), и скачивание идёт от того же нажатия.
//!
//! Адрес файла живёт в ссылке до следующего выбора: сменились период или
//! квартал, нажата соседняя ссылка — адреса прежних файлов снимаются. Иначе
//! «Сохранить ссылку как…», которое нажатия не вызывает, сохранило бы файл
//! прошлого периода под именем нового (ревью задачи 30).
//!
//! Лицензия выгрузку не запирает: это данные человека (Правило 2).

use kvartik_core::format::export;
use kvartik_core::i18n::{self, Key, fill, t};
use kvartik_core::model::date::{Date, Quarter};
use leptos::ev::MouseEvent;
use leptos::html::A;
use leptos::prelude::*;

use super::Ctx;
use crate::platform::clock;
use crate::platform::file::Download;
use crate::ui::icon::{Icon, icon};
use crate::ui::route::Screen;
use crate::ui::tr;

/// Тип файла XLSX (ECMA-376, часть 2, приложение; регистрация IANA).
const XLSX: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";

/// Тип CSV — `application/octet-stream`, как у файла копии, а не
/// `text/csv`: текстовый файл со ссылки iPhone открывает во вкладке вместо
/// загрузки (WebKit bug 263608, `platform::file::Download`). Тип файлу даёт
/// расширение `.csv`.
const CSV: &str = "application/octet-stream";

/// За какой период выгрузка: квартал, который показывает книга, или весь
/// его год.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Period {
    Quarter,
    Year,
}

/// Какой файл.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum What {
    /// XLSX: оба листа.
    Book,
    /// CSV доходов и расходов.
    Ledger,
    /// CSV документов.
    Documents,
}

/// Ссылки на файлы — по порядку на экране.
const FILES: [What; 3] = [What::Book, What::Ledger, What::Documents];

impl What {
    const fn key(self) -> Key {
        match self {
            What::Book => Key::ExportXlsx,
            What::Ledger => Key::ExportCsvLedger,
            What::Documents => Key::ExportCsvDocuments,
        }
    }

    /// Часть в имени файла, расширение и тип.
    const fn file(self) -> (Option<&'static str>, &'static str, &'static str) {
        match self {
            What::Book => (None, "xlsx", XLSX),
            What::Ledger => (Some("ledger"), "csv", CSV),
            What::Documents => (Some("documents"), "csv", CSV),
        }
    }
}

/// Первый и последний день периода и его имя в имени файла: `2026-Q3`
/// или `2026`.
fn span(quarter: Quarter, period: Period) -> (Date, Date, String) {
    match period {
        Period::Quarter => (
            quarter.first_day(),
            quarter.last_day(),
            format!("{}-Q{}", quarter.year(), quarter.number()),
        ),
        Period::Year => {
            let year = quarter.year();
            let first = Quarter::new(year, 1).unwrap_or(quarter);
            let last = Quarter::new(year, 4).unwrap_or(quarter);
            (first.first_day(), last.last_day(), year.to_string())
        }
    }
}

/// Имя файла — латиницей, одно на всех языках.
fn file_name(quarter: Quarter, period: Period, what: What) -> String {
    let (_, _, name) = span(quarter, period);
    let (part, extension, _) = what.file();
    export::file_name(&name, part, extension)
}

/// Файл выгрузки из прочитанных документов, записей книги и клиентов —
/// и сколько в нём строк в валюте без курса. `None` — что-то из них ещё не
/// прочитано или не прочиталось, или браузер не дал собрать файл: тогда окно
/// говорит это словами, а не отдаёт неполную выгрузку молча.
///
/// «Сегодня» для статуса документов — у часов в самом нажатии, а не день
/// открытия программы: установленная живёт открытой неделями (ревью
/// задачи 30; Архитектура, «Время — параметр»).
fn build(ctx: Ctx, quarter: Quarter, period: Period, what: What) -> Option<(Download, usize)> {
    let lang = ctx.lang.get_untracked();
    let today = clock::today();
    let (from, until, _) = span(quarter, period);
    let (_, _, mime) = what.file();
    let name = file_name(quarter, period, what);
    // Обработчик нажатия ничего не отслеживает: ни одно чтение здесь не
    // должно стать подпиской.
    untrack(|| {
        ctx.documents.with(|documents, _, _| {
            ctx.operations.with(|operations, _| {
                ctx.clients.with(|clients, _| {
                    let ledger = || export::ledger_sheet(lang, documents, operations, clients, from, until);
                    let issued = || export::documents_sheet(lang, documents, from, until, today);
                    let bytes = match what {
                        What::Book => export::xlsx(&[ledger(), issued()]),
                        What::Ledger => export::csv(&ledger()),
                        What::Documents => export::csv(&issued()),
                    };
                    let unrated = export::unrated(documents, operations, from, until);
                    Download::bytes(&bytes, &name, mime).map(|download| (download, unrated))
                })
            })
        })
    })
}

/// Выгрузка под списком книги: заголовок, что в файлах, выбор периода
/// и три ссылки на файлы.
#[component]
pub(super) fn Export(ctx: Ctx, quarter: Memo<Quarter>) -> impl IntoView {
    let lang = ctx.lang;
    let period = RwSignal::new(Period::Quarter);
    // Файл держит адрес `blob:`, пока его не сменит следующий: начатое
    // скачивание адрес уже разобрало (`platform::file::Download`).
    let file = StoredValue::new_local(None::<Download>);
    let failed = RwSignal::new(false);
    // Сколько строк без курса в отданном файле; 0 — говорить не о чем.
    let unrated = RwSignal::new(0_usize);
    // Ссылки — по `NodeRef`, а не `currentTarget` события: Leptos слушает
    // нажатия выше по дереву (`ui/deadlines.rs`).
    let nodes = [NodeRef::<A>::new(), NodeRef::<A>::new(), NodeRef::<A>::new()];
    // Снять адреса прежних файлов: ссылки снова ведут на экран, а файл
    // отпущен.
    let reset = move || {
        for node in &nodes {
            if let Some(node) = node.get_untracked() {
                let _ = node.set_attribute("href", &Screen::Ledger.href());
            }
        }
        file.set_value(None);
    };
    Effect::watch(
        move || (quarter.get(), period.get()),
        move |_, _, _| {
            reset();
            failed.set(false);
            unrated.set(0);
        },
        false,
    );
    // Нечитаемые записи в выгрузку не попадают, и окно говорит, сколько их.
    let unreadable = Memo::new(move |_| {
        ctx.documents.with(|_, _, unreadable| unreadable) + ctx.operations.with(|_, unreadable| unreadable)
    });
    let choice = move |value: Period| {
        view! {
            <button
                type="button"
                aria-pressed=move || (period.get() == value).to_string()
                on:click=move |_| period.set(value)
            >
                {move || {
                    let lang = lang.get();
                    let quarter = quarter.get();
                    match value {
                        Period::Quarter => i18n::quarter(lang, quarter),
                        Period::Year => fill(t(lang, Key::AnnualYear), &[&quarter.year().to_string()]),
                    }
                }}
            </button>
        }
    };
    let link = move |at: usize| {
        let what = FILES[at];
        let node = nodes[at];
        let click = move |event: MouseEvent| {
            reset();
            let made = build(ctx, quarter.get_untracked(), period.get_untracked(), what);
            match (made, node.get_untracked()) {
                (Some((download, count)), Some(node)) if node.set_attribute("href", download.url()).is_ok() => {
                    failed.set(false);
                    unrated.set(count);
                    file.set_value(Some(download));
                }
                _ => {
                    event.prevent_default();
                    failed.set(true);
                }
            }
        };
        let class = if what == What::Book { "button" } else { "button secondary" };
        view! {
            <a
                class=class
                node_ref=node
                href=Screen::Ledger.href()
                download=move || file_name(quarter.get(), period.get(), what)
                on:click=click
            >
                {icon(Icon::Download)}
                <span>{tr(lang, what.key())}</span>
            </a>
        }
    };
    view! {
        <section class="export">
            <h2>{tr(lang, Key::ExportTitle)}</h2>
            <p class="hint">{tr(lang, Key::ExportAbout)}</p>
            <div class="seg" role="group" aria-label=move || t(lang.get(), Key::ExportPeriod)>
                {choice(Period::Quarter)}
                {choice(Period::Year)}
            </div>
            <div class="actions">
                {link(0)}
                {link(1)}
                {link(2)}
            </div>
            {move || failed.get().then(|| view! { <p role="alert">{tr(lang, Key::ExportFailed)}</p> })}
            {move || (unrated.get() > 0).then(|| view! {
                <p role="status">{move || {
                    let lang = lang.get();
                    fill(t(lang, Key::ExportUnrated), &[&i18n::integer(lang, unrated.get() as i64)])
                }}</p>
            })}
            {move || (unreadable.get() > 0).then(|| view! {
                <p role="alert">{move || {
                    let lang = lang.get();
                    fill(t(lang, Key::ExportUnreadable), &[&i18n::integer(lang, unreadable.get() as i64)])
                }}</p>
            })}
        </section>
    }
}
