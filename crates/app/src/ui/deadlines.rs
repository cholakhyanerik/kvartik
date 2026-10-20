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
//! карточкой при открытии, и экран говорит это словами.

use kvartik_core::i18n::{self, Key, Lang, fill, plural, t};
use kvartik_core::model::date::Date;
use kvartik_core::tax::deadlines::{self, Deadline, Kind, Reminder, When};
use leptos::prelude::*;

use super::backup::BackupChip;
use super::icon::{Icon, icon};
use super::ledger::tax_part;
use super::route::{Router, Screen};
use super::split::screen;
use super::tax::{self, Part};
use super::tr;

/// На сколько дней вперёд экран показывает сроки: год — все сроки года,
/// включая годовые платежи и заявление.
const AHEAD_DAYS: i32 = 366;

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

/// Что надо сделать к сроку — словами.
fn what(lang: Lang, kind: Kind) -> String {
    match kind {
        Kind::Turnover(quarter) => fill(t(lang, Key::DeadlineTurnover), &[&i18n::quarter(lang, quarter)]),
        Kind::Application(year) => fill(t(lang, Key::DeadlineApplication), &[&year.to_string()]),
        Kind::Annual(year) => fill(t(lang, Key::DeadlineAnnual), &[&year.to_string()]),
        Kind::HealthYear(year) => fill(t(lang, Key::DeadlineHealthYear), &[&year.to_string()]),
        // Месяц — всегда с годом: «за декабрь» в январском напоминании
        // читалось бы как декабрь этого года. Пробел перед «г.» CLDR ставит
        // обычный, и в строке срока «г.» уезжала одна на новую строку
        // (глазной прогон задачи 21): здесь он неразрывный.
        Kind::HealthMonth { year, month } => fill(
            t(lang, Key::DeadlineHealthMonth),
            &[&i18n::month_heading(lang, year, month, 0).replace(" г.", "\u{a0}г.")],
        ),
        Kind::Declaration(year) => fill(t(lang, Key::DeadlineDeclaration), &[&year.to_string()]),
    }
}

/// Слова под перенесённым днём: с какого дня по закону перенесён и что
/// надёжнее успеть до него (`tax::deadlines`, «Перенос»: перенос — прочтение
/// закона). `None` — день не переносился. Окно пишет их везде, где
/// показывает перенесённый день (ревью задачи 21).
pub(super) fn shift_note(lang: Lang, by_law: Date) -> Option<String> {
    let day = deadlines::last_day(by_law);
    (day != by_law).then(|| fill(t(lang, Key::DeadlineShifted), &[&i18n::date(lang, by_law)]))
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
                                {move || shift_note(lang.get(), by_law).map(|note| view! { <span>{note}</span> })}
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
                    <a class="button" href=Screen::Deadlines.href()>{tr(lang, Key::DeadlinesAll)}</a>
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
    let shifted = (deadline.day != deadline.by_law).then_some(deadline.by_law);
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
                        {shifted.map(|by_law| view! {
                            <span>{move || fill(t(lang.get(), Key::DeadlineShifted), &[&i18n::date(lang.get(), by_law)])}</span>
                        })}
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
                    <a class="button" href=Screen::Profile.href()>{tr(lang, Key::ProfileFill)}</a>
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
        <p class="hint">{tr(lang, Key::DeadlinesNoAlerts)}</p>
        <p class="tax-check">{tr(lang, Key::TaxCheck)}</p>
    }
}
