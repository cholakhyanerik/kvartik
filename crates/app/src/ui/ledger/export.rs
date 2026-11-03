//! Выгрузка для бухгалтера (задача 30) — под списком книги: квартал книги
//! или весь его год файлом XLSX (три листа: доходы и расходы, документы,
//! закрытые кварталы) и двумя CSV. Листы и байты файлов собирает ядро
//! (`kvartik_core::format::export`), здесь — выбор периода, ссылки и слова.
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
//! Всё, в чём файл расходится с поданным или неполон, блок говорит до
//! нажатия, одним местом (задача 83, решение человека 2026-11-03: «надо обо
//! всём информировать»): закрытые кварталы периода, чей расчёт по книге
//! сейчас разошёлся со снимком или неполон, строки в валюте без курса
//! и нечитаемые записи. Чего не хватает расчёту налога открытых кварталов,
//! говорит экран налога: в книге этого нет, и файл с поданным не расходится.
//! Закрытые кварталы считаются `Memo` от данных и периода одним проходом по
//! цепочке (`turnover::closed_in`), как карточка на главном.
//!
//! Лицензия выгрузку не запирает: это данные человека (Правило 2).

use kvartik_core::format::export;
use kvartik_core::i18n::{self, Key, fill, t};
use kvartik_core::model::date::{Date, Quarter};
use kvartik_core::tax::closing::Closing;
use kvartik_core::tax::turnover::QuarterTax;
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
    /// XLSX: все три листа.
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

/// Первый и последний квартал периода.
fn quarters(quarter: Quarter, period: Period) -> (Quarter, Quarter) {
    match period {
        Period::Quarter => (quarter, quarter),
        Period::Year => {
            let year = quarter.year();
            (Quarter::new(year, 1).unwrap_or(quarter), Quarter::new(year, 4).unwrap_or(quarter))
        }
    }
}

/// Первый и последний день периода и его имя в имени файла: `2026-Q3`
/// или `2026`.
fn span(quarter: Quarter, period: Period) -> (Date, Date, String) {
    let (first, last) = quarters(quarter, period);
    let name = match period {
        Period::Quarter => format!("{}-Q{}", quarter.year(), quarter.number()),
        Period::Year => quarter.year().to_string(),
    };
    (first.first_day(), last.last_day(), name)
}

/// Имя файла — латиницей, одно на всех языках.
fn file_name(quarter: Quarter, period: Period, what: What) -> String {
    let (_, _, name) = span(quarter, period);
    let (part, extension, _) = what.file();
    export::file_name(&name, part, extension)
}

/// Файл выгрузки из прочитанных документов, записей книги и клиентов,
/// а XLSX — и из закрытых кварталов периода `closed` (`None` — снимки или
/// профиль не прочитаны). `None` — что-то из них ещё не прочитано или не
/// прочиталось, или браузер не дал собрать файл: тогда окно говорит это
/// словами, а не отдаёт неполную выгрузку молча — пустой лист закрытых
/// кварталов читался бы «ничего не закрыто» (ревью задачи 83).
///
/// «Сегодня» для статуса документов — у часов в самом нажатии, а не день
/// открытия программы: установленная живёт открытой неделями (ревью
/// задачи 30; Архитектура, «Время — параметр»).
fn build(
    ctx: Ctx,
    quarter: Quarter,
    period: Period,
    what: What,
    closed: Memo<Option<Vec<(QuarterTax, Closing)>>>,
) -> Option<Download> {
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
                        What::Book => {
                            let closings = closed.with(|closed| {
                                let closed = closed.as_ref()?;
                                Some(export::closings_sheet(lang, closed.iter().map(|(tax, closing)| (tax, closing))))
                            })?;
                            export::xlsx(&[ledger(), issued(), closings])
                        }
                        What::Ledger => export::csv(&ledger()),
                        What::Documents => export::csv(&issued()),
                    };
                    Download::bytes(&bytes, &name, mime)
                })
            })
        })
    })
}

/// Выгрузка под списком книги: заголовок, что в файлах, выбор периода, что
/// в выгрузке расходится с поданным или неполно, и три ссылки на файлы.
#[component]
pub(super) fn Export(ctx: Ctx, quarter: Memo<Quarter>) -> impl IntoView {
    let lang = ctx.lang;
    let period = RwSignal::new(Period::Quarter);
    // Файл держит адрес `blob:`, пока его не сменит следующий: начатое
    // скачивание адрес уже разобрало (`platform::file::Download`).
    let file = StoredValue::new_local(None::<Download>);
    let failed = RwSignal::new(false);
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
        },
        false,
    );
    // Закрытые кварталы периода с их расчётом: о разошедшихся и неполных —
    // словами до нажатия, а в XLSX — своим листом (задача 83). Снимки читать
    // велит экран книги (`LedgerBody`): он живёт до конца окна.
    let sources = ctx.sources();
    let closed = Memo::new(move |_| {
        let (first, last) = quarters(quarter.get(), period.get());
        sources.closed_in(ctx.today, first, last)
    });
    // Строки периода в валюте без курса: сумма в драмах у них в файле пустая.
    let unrated = Memo::new(move |_| {
        let (from, until, _) = span(quarter.get(), period.get());
        ctx.documents.with(|documents, _, _| {
            ctx.operations.with(|operations, _| export::unrated(documents, operations, from, until))
        })
    });
    // Нечитаемые записи в выгрузку не попадают, и окно говорит, сколько их:
    // документы и книга, а ещё снимки закрытых кварталов — их квартала нет
    // в листе — и перенос с кварталов до программы, без которого расчёт
    // «сейчас» мог выйти другим.
    let unreadable = Memo::new(move |_| {
        ctx.documents.with(|_, _, unreadable| unreadable)
            + ctx.operations.with(|_, unreadable| unreadable)
            + ctx.closings.with(|_, _, unreadable| unreadable)
            + ctx.closings.unreadable_openings()
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
    // Закрытый квартал, о котором есть что сказать: разошёлся с поданным —
    // тревогой, расчёт неполон — спокойно, совпал — ничего. Ссылка открывает
    // экран налога на этом квартале, как карточка на главном. Сменился файл
    // правил — часть расхождения от него, а не от книги, и окно говорит это
    // словами экрана налога.
    let drift = move |(now, closing): &(QuarterTax, Closing)| {
        let (key, role) = if now.drifted_from(closing) {
            (Key::ExportDrift, "alert")
        } else if !now.complete() {
            (Key::ExportDriftUnknown, "status")
        } else {
            return None;
        };
        let shown = closing.quarter();
        let chosen = ctx.tax_quarter;
        let (then, rules) = (closing.data().rules, now.rules);
        let versions = (role == "alert" && rules != then).then(|| (then.to_string(), rules.to_string()));
        Some(view! {
            <div class="notice-row">
                <p role=role>{move || {
                    let lang = lang.get();
                    fill(t(lang, key), &[&i18n::quarter(lang, shown)])
                }}</p>
                <a class="button link" href=Screen::Tax.href() on:click=move |_| chosen.set(Some(shown))>
                    {tr(lang, Key::ExportDriftOpen)}
                </a>
            </div>
            {versions.map(|(then, now)| view! {
                <p class="hint">{move || fill(t(lang.get(), Key::TaxClosedRulesChanged), &[&then, &now])}</p>
            })}
        })
    };
    let link = move |at: usize| {
        let what = FILES[at];
        let node = nodes[at];
        let click = move |event: MouseEvent| {
            reset();
            let made = build(ctx, quarter.get_untracked(), period.get_untracked(), what, closed);
            match (made, node.get_untracked()) {
                (Some(download), Some(node)) if node.set_attribute("href", download.url()).is_ok() => {
                    failed.set(false);
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
            {move || closed.with(|closed| closed.iter().flatten().filter_map(drift).collect_view())}
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
            <div class="actions">
                {link(0)}
                {link(1)}
                {link(2)}
            </div>
            {move || failed.get().then(|| view! { <p role="alert">{tr(lang, Key::ExportFailed)}</p> })}
        </section>
    }
}
