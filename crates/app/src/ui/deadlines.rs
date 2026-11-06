//! Сроки (задача 21): экран внутри раздела налога, напоминание на главном
//! экране и строка на экране налога.
//!
//! Считает ядро (`kvartik_core::tax::deadlines`) — по книге, профилю,
//! закрытым кварталам и файлу правил; здесь — показ. Своих данных нет:
//! сроки — производное правил и книги.
//!
//! Экран, напоминание и строка — из части книги, как налог квартала
//! (`ui/tax.rs`): сроки читают книгу и файл правил, а их место — там
//! (CLAUDE.md, Правило 1). В главном wasm — только оболочка экрана.
//!
//! Уведомлений с закрытой программой нет: у неё нет сервера, и в фоне она
//! ничего не делает (Правило 1, «Покой»; Архитектура). Напоминание —
//! карточкой при открытии, и экран говорит это словами. Напомнить
//! с закрытой программой может календарь телефона и почты — сроки ему
//! отдаёт экран сроков файлом и подпиской (задача 22, [`Calendar`]).

use kvartik_core::i18n::{self, Key, Lang, fill, plural, t};
use kvartik_core::model::date::Date;
use kvartik_core::tax::calendar;
use kvartik_core::tax::deadlines::{self, AHEAD_DAYS, Deadline, Kind, Reminder, When, what};
use leptos::ev::MouseEvent;
use leptos::html::A;
use leptos::prelude::*;

use crate::platform::{self, file::Download};

use super::backup::BackupChip;
use super::icon::{Icon, icon};
use super::ledger::tax_part;
use super::route::{Router, Screen};
use super::split::screen;
use super::tax::{self, Part};
use super::tr;

/// Экран сроков на языке окна: «назад» к налогу и заголовок — сразу,
/// остальное — частью wasm с первого показа (`split::screen`).
#[component]
pub fn DeadlinesScreen(ctx: tax::Ctx, router: Router) -> impl IntoView {
    let tax::Ctx { lang, backups, .. } = ctx;
    view! {
        <a class="back" href=Screen::Tax.href()>
            {icon(Icon::Back)}
            <span>{tr(lang, Key::TaxTitle)}</span>
        </a>
        <h1 tabindex="-1">{tr(lang, Key::DeadlinesTitle)}</h1>
        <BackupChip lang backups nudge=true />
        {screen(lang, router, Screen::Deadlines, move || tax_part(ctx, Part::Deadlines))}
    }
}

/// Насколько близок срок — словами.
fn when(lang: Lang, when: When, day: Date) -> String {
    match when {
        When::Overdue => fill(t(lang, Key::RemindOverdue), &[&i18n::date(lang, day)]),
        When::Today => t(lang, Key::RemindToday).to_owned(),
        When::Tomorrow => t(lang, Key::RemindTomorrow).to_owned(),
        When::In(days) => {
            let n = u64::from(days.unsigned_abs());
            let count = fill(
                plural(lang, n, Key::DaysOne, Key::DaysFew, Key::DaysMany),
                &[&i18n::integer(lang, i64::from(days))],
            );
            fill(t(lang, Key::RemindIn), &[&count, &i18n::date(lang, day)])
        }
    }
}

/// Где дело срока в программе: квартал — экран налога, годовые платежи
/// и медстраховка — их экран, заявление и декларация — экран сроков: их
/// подают в системе ՊԵԿ, а не здесь.
const fn place(kind: Kind) -> Screen {
    match kind {
        Kind::Turnover(_) => Screen::Tax,
        Kind::Annual(_) | Kind::HealthYear(_) | Kind::HealthMonth { .. } => Screen::Annual,
        Kind::Application(_) | Kind::Declaration(_) => Screen::Deadlines,
    }
}

/// Напоминания на главном экране (задача 21): незакрытый квартал, срок
/// которого прошёл, и сроки ближайших дней — словами, когда и что. Нет
/// таких — карточки нет. Каждая строка ведёт туда, где дело делается.
#[component]
pub fn DeadlineReminder(ctx: tax::Ctx) -> impl IntoView {
    let lang = ctx.lang;
    let found = Memo::new(move |_| {
        let today = ctx.today?;
        ctx.sources
            .with_books(|books| deadlines::reminders(books, today))
            .filter(|reminders| !reminders.is_empty())
    });
    move || {
        found.get().map(|reminders| {
            let rows = reminders
                .into_iter()
                .map(|Reminder { deadline, when: soon }| {
                    let overdue = soon == When::Overdue;
                    let by_law = deadline.by_law;
                    view! {
                        <li>
                            <a href=place(deadline.kind).href()>
                                <span class="reminder">{move || when(lang.get(), soon, deadline.day)}</span>
                                <span>{move || what(lang.get(), deadline.kind)}</span>
                                {move || deadlines::moved(lang.get(), deadline.kind, by_law).map(|note| view! { <span>{note}</span> })}
                                {overdue.then(|| view! { <span>{tr(lang, Key::TaxCloseReminder)}</span> })}
                            </a>
                        </li>
                    }
                })
                .collect_view();
            view! {
                <div class="notice due deadline-reminder" role="status">
                    {icon(Icon::Calendar)}
                    <ul>{rows}</ul>
                    <a class="button" href=Screen::Deadlines.href()><span class="label">{tr(lang, Key::DeadlinesAll)}</span></a>
                </div>
            }
        })
    }
}

/// Строка на экране налога — к экрану сроков: ближайший срок.
#[component]
pub fn DeadlinesEntry(ctx: tax::Ctx) -> impl IntoView {
    let lang = ctx.lang;
    let next = Memo::new(move |_| {
        let today = ctx.today?;
        let until = today.add_days(AHEAD_DAYS)?;
        ctx.sources
            .with_books(|books| deadlines::between(books, today, until).into_iter().find(|deadline| !deadline.done))
            .flatten()
    });
    view! {
        <ul class="list deadlines-entry">
            <li>
                <a class="entry" href=Screen::Deadlines.href()>
                    <span class="entry-main">
                        <span class="entry-title">{tr(lang, Key::DeadlinesTitle)}</span>
                        {move || next.get().map(|deadline| view! {
                            <span class="entry-meta">
                                <span>{move || fill(t(lang.get(), Key::DeadlinesNext), &[&i18n::date(lang.get(), deadline.day)])}</span>
                                <span>{move || what(lang.get(), deadline.kind)}</span>
                            </span>
                        })}
                    </span>
                    <span class="entry-side">{icon(Icon::Forward)}</span>
                </a>
            </li>
        </ul>
    }
}

/// Строка списка сроков: день, насколько он близок или что уже сделано, что
/// сделать, — и откуда день, если его перенесли с нерабочего. Близость —
/// строкой под днём, а не справа: на телефоне колонка справа сжала бы
/// слова срока в узкий столбец (глазной прогон задачи 21).
#[component]
fn Row(lang: ReadSignal<Lang>, deadline: Deadline, soon: Option<When>) -> impl IntoView {
    let soon = soon.filter(|_| !deadline.done);
    view! {
        <li class="deadline">
            <a class="entry" href=place(deadline.kind).href()>
                <span class="entry-main">
                    <span class="entry-title">{move || i18n::date(lang.get(), deadline.day)}</span>
                    {soon.map(|soon| view! {
                        <span class="deadline-soon">{move || when(lang.get(), soon, deadline.day)}</span>
                    })}
                    {deadline.done.then(|| view! { <span class="deadline-done">{tr(lang, Key::DeadlineDone)}</span> })}
                    <span class="entry-meta">
                        <span>{move || what(lang.get(), deadline.kind)}</span>
                        {move || deadlines::moved(lang.get(), deadline.kind, deadline.by_law).map(|note| view! { <span>{note}</span> })}
                    </span>
                </span>
                {icon(Icon::Forward)}
            </a>
        </li>
    }
}

/// Чего не хватает профилю, чтобы список был полон (Правило 6: «нет
/// значения» — не «срока нет»): слова и ссылка на профиль, а о
/// медстраховке — на экран годовых платежей, где сказано, что с ней.
#[component]
fn Gaps(lang: ReadSignal<Lang>, gaps: deadlines::Gaps) -> impl IntoView {
    let profile = gaps.registered || gaps.residency;
    (profile || gaps.health).then(|| {
        view! {
            <div class="notice deadline-gaps">
                {gaps.registered.then(|| view! { <p>{tr(lang, Key::DeadlinesNeedRegistered)}</p> })}
                {gaps.residency.then(|| view! { <p>{tr(lang, Key::DeadlinesNeedResidency)}</p> })}
                {profile.then(|| view! {
                    <a class="button" href=Screen::Profile.href()><span class="label">{tr(lang, Key::ProfileFill)}</span></a>
                })}
                {gaps.health.then(|| view! {
                    <p>
                        <a href=Screen::Annual.href()>{tr(lang, Key::DeadlinesHealthUnclear)}</a>
                    </p>
                })}
            </div>
        }
    })
}

/// Содержимое экрана сроков: что прошло и не сделано, сроки на год вперёд
/// и почему напоминания только при открытии.
#[component]
pub fn DeadlinesPage(ctx: tax::Ctx) -> impl IntoView {
    let lang = ctx.lang;
    ctx.sources.watch(ctx.base);
    let list = Memo::new(move |_| {
        let today = ctx.today?;
        let until = today.add_days(AHEAD_DAYS)?;
        ctx.sources.with_books(|books| {
            let gaps = deadlines::gaps(books, today, until);
            let overdue = deadlines::overdue(books, today);
            let ahead: Vec<(Deadline, Option<When>)> = deadlines::between(books, today, until)
                .into_iter()
                .map(|deadline| {
                    let days = deadline.day.days_since(today);
                    let soon = match days {
                        0 => Some(When::Today),
                        1 => Some(When::Tomorrow),
                        days if days <= deadlines::REMIND_DAYS => Some(When::In(days)),
                        _ => None,
                    };
                    (deadline, soon)
                })
                .collect();
            (gaps, overdue, ahead)
        })
    });
    view! {
        <p>{tr(lang, Key::DeadlinesAbout)}</p>
        {move || list.get().map(|(gaps, overdue, ahead)| {
            let empty = overdue.is_none() && ahead.is_empty();
            let declaration = ahead.iter().any(|(deadline, _)| matches!(deadline.kind, Kind::Declaration(_)));
            view! {
                <ul class="list deadlines">
                    {overdue.map(|deadline| view! { <Row lang deadline soon=Some(When::Overdue) /> })}
                    {ahead
                        .into_iter()
                        .map(|(deadline, soon)| view! { <Row lang deadline soon /> })
                        .collect_view()}
                </ul>
                {empty.then(|| view! { <p class="hint">{tr(lang, Key::DeadlinesNone)}</p> })}
                {declaration.then(|| view! { <p class="hint">{tr(lang, Key::DeadlinesDeclarationAbout)}</p> })}
                <Gaps lang gaps />
            }
        })}
        <p class="hint">{tr(lang, Key::DeadlinesShiftRule)}</p>
        <p class="hint">{tr(lang, Key::DeadlinesPayEarly)}</p>
        <Calendar ctx mine=Signal::derive(move || {
            list.with(|list| {
                list.as_ref()
                    .is_some_and(|(_, _, ahead)| ahead.iter().any(|(deadline, _)| !deadline.done))
            })
        }) />
        <p class="tax-check">{tr(lang, Key::TaxCheck)}</p>
    }
}

/// Сроки в календаре телефона и почты (задача 22): программа без сервера
/// не напомнит, пока закрыта, а календарь — напомнит. Свои сроки с суммами
/// — файлом (`tax::calendar::personal`), когда есть несделанные (`mine`);
/// общий календарь всех ИП — подпиской на файл сайта (`calendar::public`).
///
/// Файл собирается в самом нажатии на ссылку, а не заранее: он нужен
/// редко, и пересобирать его на каждое изменение данных — лишняя работа
/// (Правило 1). Сборка синхронная, и ссылка получает адрес файла ещё
/// в обработчике нажатия: переход по ссылке браузер делает после
/// обработчиков, по адресу на этот момент (по спецификации HTML,
/// «activation behavior» у `a`), — скачивание идёт от того же нажатия.
#[component]
fn Calendar(ctx: tax::Ctx, mine: Signal<bool>) -> impl IntoView {
    let lang = ctx.lang;
    // Файл держит адрес `blob:`, пока его не сменит следующий: начатое
    // скачивание адрес уже разобрало (`platform::file::Download`).
    let file = StoredValue::new_local(None::<Download>);
    let failed = RwSignal::new(false);
    // Ссылка — по `NodeRef`, а не `currentTarget` события: Leptos слушает
    // нажатия выше по дереву, и `currentTarget` там — не ссылка (по
    // исходникам tachys 0.2.19, `add_event_listener_delegated`).
    let link = NodeRef::<A>::new();
    let (origin, page) = platform::address().unzip();
    let page = page.unwrap_or_else(|| calendar::APP_URL.to_owned());
    let origin = StoredValue::new(origin);
    let export = move |event: MouseEvent| {
        let lang = lang.get_untracked();
        let made = ctx
            .today
            .and_then(|today| ctx.sources.with_books(|books| calendar::personal(books, today, lang, &page)))
            .flatten()
            .and_then(|text| Download::bytes(text.as_bytes(), FILE_NAME, MIME));
        match (made, link.get_untracked()) {
            (Some(download), Some(link)) if link.set_attribute("href", download.url()).is_ok() => {
                failed.set(false);
                file.set_value(Some(download));
            }
            _ => {
                event.prevent_default();
                failed.set(true);
            }
        }
    };
    // Адрес общего календаря — на этом же сайте: на kvartik.com — его,
    // на адресе проверки — свой. Подписка по ссылке `webcal:` открывает
    // календарь устройства, а адрес `https:` вставляют в календарь сами.
    let shared = move || {
        origin.with_value(|origin| {
            origin
                .as_ref()
                .map(|origin| format!("{origin}/{}", calendar::path(lang.get())))
        })
    };
    let subscribe = move || shared().map(|url| url.replacen("https://", "webcal://", 1).replacen("http://", "webcal://", 1));
    view! {
        <section class="calendar">
            <h2>{tr(lang, Key::CalendarTitle)}</h2>
            <p>{tr(lang, Key::DeadlinesNoAlerts)}</p>
            <p>{tr(lang, Key::CalendarAbout)}</p>
            {move || mine.get().then(|| view! {
                <a class="button" node_ref=link href=Screen::Deadlines.href() download=FILE_NAME on:click=export.clone()>
                    {icon(Icon::Download)}
                    <span class="label">{tr(lang, Key::CalendarMine)}</span>
                </a>
                <p class="hint">{tr(lang, Key::CalendarMineHint)}</p>
            })}
            {move || failed.get().then(|| view! { <p role="alert">{tr(lang, Key::CalendarFailed)}</p> })}
            <a class="button secondary" href=subscribe>
                {icon(Icon::Calendar)}
                <span class="label">{tr(lang, Key::CalendarSubscribe)}</span>
            </a>
            <p class="hint">{tr(lang, Key::CalendarSubscribeHint)}</p>
            <p class="calendar-address">{shared}</p>
        </section>
    }
}

/// Имя файла личного календаря: одно на всех языках, как у копии.
const FILE_NAME: &str = "Kvartik-deadlines.ics";

/// Тип файла — настоящий, а не `application/octet-stream`, как у копии:
/// iPhone с ним открывает лист «Добавить в Календарь», а безымянный тип
/// молча кладёт файл в «Загрузки» (вторичный: tsml-ui, issue 560 и PR 562,
/// iOS 27, 2026-10-04). Во вкладке, как текст копии, его WebKit не
/// показывает: `text/calendar` у него в списке неподдержанных текстовых
/// типов (по исходникам WebKit, `MIMETypeRegistry.cpp`, 2026-10-21).
/// «Поделиться» нет: Chromium такой файл в меню не пустит (по исходникам,
/// `share_service_impl.cc`, 2026-10-21; `platform::share`).
const MIME: &str = "text/calendar;charset=utf-8";
