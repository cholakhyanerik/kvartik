//! Числа для расчёта (задача 24): экран внутри раздела налога — поля формы
//! расчёта налога с оборота ՊԵԿ с номерами, как в форме, у каждого числа —
//! «скопировать», и «Скопировать всё» — текстом для бухгалтера.
//!
//! Раскладывает числа по полям ядро (`kvartik_core::tax::report`): из
//! снимка закрытого квартала или из полного расчёта открытого; здесь —
//! показ и буфер обмена (`platform::clipboard`). Своих данных у экрана нет.
//!
//! Экран — из части книги, как налог (`ui/tax.rs`): числа — тот же расчёт,
//! а своя часть делила бы с налогом почти весь код (CLAUDE.md, Правило 6).
//! В главном wasm — только оболочка. Квартал — общий с экраном налога
//! (`tax::shown`): числа открывают с него на том же квартале.
//!
//! Число показывается по правилам языка, а копируется так, как его
//! вписывают в форму: цифрами, без пробелов и знака драма
//! (`report::Value::plain`). Ставки форма ставит сама — у них «скопировать»
//! нет, как и у клеток «X».

use kvartik_core::i18n::{self, Key, Lang, fill, t};
use kvartik_core::model::activity::IncomeKind;
use kvartik_core::model::date::Date;
use kvartik_core::tax::opening;
use kvartik_core::tax::report::{self, Activities, Missing, Origin, Ready, Section, Share, Value, What};
use kvartik_core::tax::rules;
use kvartik_core::tax::turnover::kind_key;
use leptos::prelude::*;

use super::backup::BackupChip;
use super::icon::{Icon, icon};
use super::ledger::tax_part;
use super::license::{ReportLocked, report_locked};
use super::route::{Router, Screen};
use super::split::screen;
use super::storage::error_key;
use super::tax::{self, Part, QuarterBar, alerts, drams, percent, shown};
use super::tr;
use crate::platform::clipboard;

/// Экран чисел для расчёта на языке окна: «назад» к налогу и заголовок —
/// сразу, остальное — частью wasm с первого показа (`split::screen`).
#[component]
pub fn ReportScreen(ctx: tax::Ctx, router: Router) -> impl IntoView {
    let tax::Ctx { lang, backups, .. } = ctx;
    view! {
        <a class="back" href=Screen::Tax.href()>
            {icon(Icon::Back)}
            <span>{tr(lang, Key::TaxTitle)}</span>
        </a>
        <h1 tabindex="-1">{tr(lang, Key::ReportTitle)}</h1>
        <BackupChip lang backups nudge=true />
        {screen(lang, router, Screen::Report, move || tax_part(ctx, Part::Report))}
    }
}

/// Что скопировано последним и легло ли в буфер: «скопировано» или
/// «не скопировалось» стоит у этого поля, пока не скопируют другое.
#[derive(Clone, PartialEq, Eq, Debug)]
struct Copied {
    /// Своё имя поля на экране; `None` — «Скопировать всё».
    spot: Option<String>,
    done: bool,
}

/// Скопировать `text` — прямо в обработчике нажатия, до первого `await`
/// (`platform::clipboard`), — и сказать, легло ли, у поля `spot`.
fn copy(last: RwSignal<Option<Copied>>, spot: Option<String>, text: &str) {
    let written = clipboard::copy(text);
    leptos::task::spawn_local(async move {
        let done = written.await.is_ok();
        last.set(Some(Copied { spot, done }));
    });
}

/// Содержимое экрана: квартал стрелками, откуда числа, поля формы
/// с «скопировать» — или почему чисел нет.
#[component]
pub fn ReportPage(ctx: tax::Ctx) -> impl IntoView {
    let lang = ctx.lang;
    let sources = ctx.sources;
    sources.watch(ctx.base);
    let quarter = shown(ctx);
    let tax = Memo::new(move |_| sources.quarter(quarter.get()));
    let closing = Memo::new(move |_| sources.closing(quarter.get()));
    let found = Memo::new(move |_| {
        tax.with(|tax| tax.as_ref().map(|tax| closing.with(|closing| report::report(tax, closing.as_ref()))))
    });
    // Таблица по видам деятельности — по книге и только у открытого
    // квартала: у закрытого долей снимок не хранит (`Ready::has_activities`).
    let shares = Memo::new(move |_| {
        let counted = found.with(|found| matches!(found, Some(Ok(ready)) if ready.has_activities()));
        counted
            .then(|| sources.with_books(|books| report::activities(books, quarter.get())))
            .flatten()
    });
    let unreadable = Memo::new(move |_| sources.closings.with(|_, _, unreadable| unreadable));
    // Числа квартала — по сроку лицензии (задача 25): пока срок идёт —
    // любого, после — кончившегося в срок.
    ctx.licenses.watch(ctx.base, sources.profiles);
    let access = Memo::new(move |_| ctx.licenses.access(sources.profiles));
    let locked = Memo::new(move |_| report_locked(ctx.licenses, access.get(), quarter.get(), ctx.today));
    let last = RwSignal::new(None::<Copied>);
    // Другой квартал — прежнее «скопировано» не о нём.
    Effect::watch(move || quarter.get(), move |_, _, _| last.set(None), false);
    let rules_version = rules::armenia().version().to_string();
    view! {
        <QuarterBar ctx quarter />
        <p>{tr(lang, Key::ReportAbout)}</p>
        {move || sources.failure().map(|error| view! { <p role="alert">{tr(lang, error_key(error))}</p> })}
        {move || (unreadable.get() > 0).then(|| view! {
            <p role="alert">{move || {
                let lang = lang.get();
                fill(t(lang, Key::TaxUnreadable), &[&i18n::integer(lang, unreadable.get() as i64)])
            }}</p>
        })}
        {move || found.get().map(|found| {
            // Красным — то же, что на экране налога: без этого чисел нет или
            // они не всё (у закрытого квартала — снимок, и его числа такие).
            let said = match &found {
                Ok(Ready { origin: Origin::Closed { .. }, .. }) => Vec::new(),
                Ok(_) | Err(_) => tax.with(|tax| tax.as_ref().map(alerts).unwrap_or_default()),
            };
            let said = said
                .into_iter()
                .map(|alert| view! { <p role="alert">{move || alert.text(lang.get())}</p> })
                .collect_view();
            match found {
                Ok(ready) => match locked.get() {
                    Some(today) => view! {
                        {said}
                        <ReportLocked lang access=access.get() today />
                    }
                    .into_any(),
                    None => view! {
                        {said}
                        <Form lang ready shares today=ctx.today last />
                    }
                    .into_any(),
                },
                Err(missing) => {
                    let key = match missing {
                        Missing::Left => Key::ReportLeft,
                        Missing::Micro => Key::ReportMicro,
                        Missing::LeftMicro => Key::ReportLeftMicro,
                        Missing::Incomplete => Key::ReportIncomplete,
                        Missing::NoForm => Key::ReportNoForm,
                        // Квартал до регистрации ИП — не тревога, а те же слова,
                        // что на экране налога; о его записях говорит тот экран.
                        Missing::NotYet { registered } => {
                            return view! {
                                <p>{move || {
                                    let lang = lang.get();
                                    fill(t(lang, Key::TaxNotYet), &[&i18n::date(lang, registered)])
                                }}</p>
                            }
                            .into_any();
                        }
                    };
                    view! {
                        <p role="alert">{tr(lang, key)}</p>
                        {said}
                    }
                    .into_any()
                }
            }
        })}
        <p class="tax-check">{tr(lang, Key::TaxCheck)}</p>
        <p class="hint">{move || fill(t(lang.get(), Key::TaxRulesVersion), &[&rules_version])}</p>
    }
}

/// «Скопировано» или «не скопировалось» — у поля `spot`, когда скопировано
/// последним оно.
#[component]
fn Status(lang: ReadSignal<Lang>, last: RwSignal<Option<Copied>>, spot: Option<String>) -> impl IntoView {
    move || {
        last.with(|copied| copied.as_ref().filter(|copied| copied.spot == spot).map(|copied| copied.done))
            .map(|done| {
                let key = if done { Key::ReportCopied } else { Key::ReportCopyFailed };
                view! { <p class="report-status" role="status">{tr(lang, key)}</p> }
            })
    }
}

/// Числа формы: что о них сказать, «Скопировать всё», разделы строк
/// с вычетом, сводка с итогом и оборотом и доход по видам деятельности.
#[component]
fn Form(
    lang: ReadSignal<Lang>,
    ready: Ready,
    shares: Memo<Option<Activities>>,
    today: Option<Date>,
    last: RwSignal<Option<Copied>>,
) -> impl IntoView {
    // Строки, у которых перенос до программы мог быть: у их x.7 это и сказано.
    let unknown: &'static [IncomeKind] = match ready.origin {
        Origin::Calculated { carry_unknown: Some(start), .. } => opening::kinds(start),
        Origin::Calculated { carry_unknown: None, .. } | Origin::Closed { .. } => &[],
    };
    // Перенос до программы неизвестен — это сказано у самого x.7.
    let notes = ready
        .notes(today)
        .into_iter()
        .filter(|key| *key != Key::TaxCarryUnknown)
        .map(|key| {
            let role = (key == Key::ReportClosedChanged).then_some("alert");
            view! { <p class:hint=role.is_none() role=role>{tr(lang, key)}</p> }
        })
        .collect_view();
    // Расчёт разошёлся со снимком: уточнённые числа — после «Открыть
    // снова», а если поменялся файл правил, часть расхождения — от него
    // (как на экране налога).
    let changed = ready.origin == Origin::Closed { changed: true };
    let current = rules::armenia().version();
    let versions = (changed && ready.rules != current).then(|| (ready.rules.to_string(), current.to_string()));
    // Квартал кончился, а расчёт его ещё не закрыт: подали — закройте.
    let close = matches!(ready.origin, Origin::Calculated { .. })
        && today.is_some_and(|today| ready.quarter.last_day() < today);
    let counted = ready.has_activities();
    let closed = matches!(ready.origin, Origin::Closed { .. });
    let numbers = ready.numbers.clone();
    let all = StoredValue::new(ready);
    let copy_all = move |_| {
        let text = all.with_value(|ready| report::text(lang.get_untracked(), ready, shares.get_untracked().as_ref(), today));
        copy(last, None, &text);
    };
    let total = numbers.total;
    let total_number = total.number.clone();
    let turnover = numbers.turnover;
    view! {
        {notes}
        {changed.then(|| view! { <p class="hint">{tr(lang, Key::ReportReopenHint)}</p> })}
        {versions.map(|(then, now)| view! {
            <p class="hint">{move || fill(t(lang.get(), Key::TaxClosedRulesChanged), &[&then, &now])}</p>
        })}
        {close.then(|| view! { <p class="hint">{tr(lang, Key::TaxCloseReminder)}</p> })}
        <div class="report-all">
            <button type="button" class="secondary" on:click=copy_all>
                {icon(Icon::Copy)}
                <span class="label">{tr(lang, Key::ReportCopyAll)}</span>
            </button>
            <Status lang last spot=None />
            <p class="hint">{tr(lang, Key::ReportCopyHint)}</p>
        </div>
        {numbers
            .sections
            .into_iter()
            .map(|section| view! { <SectionCard lang section unknown closed last /> })
            .collect_view()}
        <section class="report-section">
            <h2>{tr(lang, Key::ReportRows)}</h2>
            {numbers
                .rows
                .into_iter()
                .map(|row| {
                    let heading = row.number.clone();
                    view! {
                        <h3>{move || fill(t(lang.get(), Key::ReportHeading), &[&heading, t(lang.get(), kind_key(row.kind))])}</h3>
                        <Columns lang number=row.number income=row.income rate=row.rate.map(Value::Percent) tax=row.tax last />
                    }
                })
                .collect_view()}
            <h3>{move || fill(t(lang.get(), Key::ReportHeading), &[&total_number, t(lang.get(), Key::ReportTotal)])}</h3>
            <Columns lang number=total.number income=total.income rate=None tax=total.tax last />
            <ul class="report-fields">
                <FieldRow
                    lang
                    number=turnover.number.clone()
                    label=Key::ReportTurnover
                    value=Some(Value::Drams(turnover.income))
                    spot=turnover.number
                    note=None
                    last
                />
            </ul>
        </section>
        {move || shares.get().map(|activities| view! { <ActivityTable lang activities last /> })}
        {(!counted).then(|| view! {
            <section class="report-section">
                <h2>{tr(lang, Key::ReportActivities)}</h2>
                <p class="hint">{tr(lang, Key::ReportActivitiesClosed)}</p>
            </section>
        })}
    }
}

/// Раздел формы — строка с вычетом: поля по порядку формы, у x.7 — слова,
/// если перенос до программы неизвестен, а когда x.4.1 и x.4.2 не
/// разделены — почему и что делать (`tax::report`, «x.4.1 и x.4.2»): у
/// открытого квартала — отметить расходы в книге или разделить самому, у
/// закрытого — вписать, как в поданном расчёте.
#[component]
fn SectionCard(
    lang: ReadSignal<Lang>,
    section: Section,
    unknown: &'static [IncomeKind],
    /// Числа — из снимка закрытого квартала.
    closed: bool,
    last: RwSignal<Option<Copied>>,
) -> impl IntoView {
    let split_key = if closed { Key::ReportSplitClosed } else { Key::ReportSplit };
    let number_of = |what: What| {
        section
            .fields
            .iter()
            .find(|field| field.what == what)
            .map(|field| field.number.clone())
            .unwrap_or_default()
    };
    let split = [number_of(What::Direct), number_of(What::Overhead), number_of(What::Expenses)];
    let spent = section.fields.iter().any(|field| field.value == Value::Unknown);
    let heading = section.number;
    let kind = section.kind;
    view! {
        <section class="report-section">
            <h2>{move || fill(t(lang.get(), Key::ReportHeading), &[&heading, t(lang.get(), kind_key(kind))])}</h2>
            <ul class="report-fields">
                {section
                    .fields
                    .into_iter()
                    .map(|field| {
                        let note = (field.what == What::CarryIn && unknown.contains(&kind)).then_some(Key::TaxCarryUnknown);
                        view! {
                            <FieldRow
                                lang
                                number=field.number.clone()
                                label=field.what.key_in(kind)
                                value=Some(field.value)
                                spot=field.number
                                note
                                last
                            />
                        }
                    })
                    .collect_view()}
            </ul>
            {spent.then(|| view! {
                <p class="hint">{move || fill(t(lang.get(), split_key), &[&split[0], &split[1], &split[2]])}</p>
            })}
        </section>
    }
}

/// Строка сводки: доход [Ա], ставка [Բ] и налог [Գ]. Ставки нет — клетка
/// «X»: у строки с разделом и у итога.
#[component]
fn Columns(
    lang: ReadSignal<Lang>,
    number: String,
    income: i64,
    rate: Option<Value>,
    tax: i64,
    last: RwSignal<Option<Copied>>,
) -> impl IntoView {
    // Своё имя клетки на экране — номер строки и буква столбца латиницей.
    let spot = |column: &str| format!("{number}{column}");
    view! {
        <ul class="report-fields">
            <FieldRow lang number=number.clone() label=Key::ReportColIncome value=Some(Value::Drams(income)) spot=spot("a") note=None last />
            <FieldRow lang number=number.clone() label=Key::ReportColRate value=rate spot=spot("b") note=None last />
            <FieldRow lang number=number.clone() label=Key::ReportColTax value=Some(Value::Drams(tax)) spot=spot("c") note=None last />
        </ul>
    }
}

/// Поле формы: номер, что в нём, число по правилам языка и «скопировать» —
/// у драмов. Ставку форма ставит сама, клетку «X» не заполняют, а прямые
/// и сбытовые расходы делит человек: у них вместо кнопки слова.
#[component]
fn FieldRow(
    lang: ReadSignal<Lang>,
    /// Номер, как в форме: `9.4.1`, у строки сводки — её номер.
    number: String,
    label: Key,
    /// Число; `None` — клетка «X».
    value: Option<Value>,
    /// Своё имя поля на экране — для «скопировано».
    spot: String,
    /// Слова под полем.
    note: Option<Key>,
    last: RwSignal<Option<Copied>>,
) -> impl IntoView {
    let shown = move || {
        let lang = lang.get();
        match value {
            Some(Value::Drams(amount)) => drams(lang, amount),
            Some(Value::Percent(rate)) => percent(lang, rate),
            Some(Value::Unknown) => t(lang, Key::ReportSplitYourself).to_owned(),
            None => t(lang, Key::ReportCrossed).to_owned(),
        }
    };
    let auto = matches!(value, Some(Value::Percent(_)));
    let button = match value {
        Some(Value::Drams(_)) => Some((number.clone(), label)),
        Some(Value::Percent(_) | Value::Unknown) | None => None,
    };
    let text = value.and_then(Value::plain).unwrap_or_default();
    let status_spot = Some(spot.clone());
    view! {
        <li class="report-field">
            <span class="report-no">{number}</span>
            <span class="report-what">{tr(lang, label)}</span>
            <span class="report-value">{shown}</span>
            {button.map(|(number, label)| view! {
                <button
                    type="button"
                    class="round"
                    aria-label=move || {
                        let lang = lang.get();
                        let field = fill(t(lang, Key::ReportHeading), &[&number, t(lang, label)]);
                        fill(t(lang, Key::ReportCopyField), &[&field])
                    }
                    on:click=move |_| copy(last, Some(spot.clone()), &text)
                >
                    {icon(Icon::Copy)}
                </button>
            })}
            {auto.then(|| view! { <p class="report-note">{tr(lang, Key::ReportAuto)}</p> })}
            {note.map(|key| view! { <p class="report-note">{tr(lang, key)}</p> })}
            <Status lang last spot=status_spot />
        </li>
    }
}

/// Таблица «Доход по видам деятельности» (приказ 299-Ն, п. 70): коды
/// и доли — или почему долей нет. Дохода нет — нет и таблицы.
#[component]
fn ActivityTable(lang: ReadSignal<Lang>, activities: Activities, last: RwSignal<Option<Copied>>) -> impl IntoView {
    let Activities { shares, unknown } = activities;
    (unknown > 0 || !shares.is_empty()).then(|| {
        view! {
            <section class="report-section">
                <h2>{tr(lang, Key::ReportActivities)}</h2>
                <p class="hint">{tr(lang, Key::ReportActivitiesAbout)}</p>
                {(unknown > 0).then(|| view! {
                    <p role="alert">{move || {
                        let lang = lang.get();
                        fill(t(lang, Key::ReportActivitiesUnknown), &[&i18n::integer(lang, unknown as i64)])
                    }}</p>
                })}
                <ul class="report-fields">
                    {shares.into_iter().map(|share| view! { <ShareRow lang share last /> }).collect_view()}
                </ul>
            </section>
        }
    })
}

/// Код и доля дохода по нему; копируется доля числом с точкой — столбец
/// формы уже в процентах, — и подпись кнопки для диктора так и говорит.
#[component]
fn ShareRow(lang: ReadSignal<Lang>, share: Share, last: RwSignal<Option<Copied>>) -> impl IntoView {
    let code = report::lettered(share.code);
    let text = share.percent.to_string();
    let spot = code.clone();
    let status_spot = Some(code.clone());
    let label = code.clone();
    view! {
        <li class="report-field share">
            <span class="report-what">{code}</span>
            <span class="report-value">{move || percent(lang.get(), share.percent)}</span>
            <button
                type="button"
                class="round"
                aria-label=move || fill(t(lang.get(), Key::ReportCopyShare), &[&label])
                on:click=move |_| copy(last, Some(spot.clone()), &text)
            >
                {icon(Icon::Copy)}
            </button>
            <Status lang last spot=status_spot />
        </li>
    }
}
