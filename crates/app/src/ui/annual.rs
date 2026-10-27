//! Годовые платежи (задача 20): экран внутри раздела налога, карточка на
//! главном экране и строка на экране налога.
//!
//! Считает ядро (`kvartik_core::tax::annual`) — по профилю и книге; здесь —
//! показ. Своих данных у экрана нет: годовые платежи — производное
//! профиля и книги, и записывать нечего.
//!
//! Экран, карточка и строка — из части книги, как налог квартала
//! (`ui/tax.rs`): расчёт читает книгу и файл правил, а их место — там
//! (CLAUDE.md, Правило 1). В главном wasm — только оболочка экрана.
//!
//! Число — расчёт по прочтению закона, а не пример: экран говорит «сверьте
//! с бухгалтером» и называет всё, чего расчёту не хватило (Правило 6).
//! Суммы, ступени и годы в словах — из расчёта и файла правил, а не из
//! строк.

use kvartik_core::i18n::{self, Key, Lang, fill, plural, t};
use kvartik_core::model::date::Date;
use kvartik_core::model::profile::Residency;
use kvartik_core::tax::annual::{
    self, Annual, Fixed, Health, HealthOut, HealthUnclear, Missing, Saving, Social, SocialOut, Stamp, Status,
    Total,
};
use kvartik_core::tax::deadlines::{self, Kind};
use kvartik_core::tax::rules::{self, HealthBy};
use kvartik_core::tax::turnover::Leaving;
use leptos::prelude::*;

use super::backup::BackupChip;
use super::icon::{Icon, icon};
use super::ledger::tax_part;
use super::route::{Router, Screen};
use super::split::screen;
use super::storage::error_key;
use super::tax::{self, Part, drams};
use super::tr;

/// Год, когда часы без даты.
const FALLBACK_YEAR: u16 = 2026;

/// Экран годовых платежей на языке окна: «назад» к налогу и заголовок —
/// сразу, остальное — частью wasm с первого показа (`split::screen`).
#[component]
pub fn AnnualScreen(ctx: tax::Ctx, router: Router) -> impl IntoView {
    let tax::Ctx { lang, backups, .. } = ctx;
    view! {
        <a class="back" href=Screen::Tax.href()>
            {icon(Icon::Back)}
            <span>{tr(lang, Key::TaxTitle)}</span>
        </a>
        <h1 tabindex="-1">{tr(lang, Key::AnnualTitle)}</h1>
        <BackupChip lang backups nudge=true />
        {screen(lang, router, Screen::Annual, move || tax_part(ctx, Part::Annual))}
    }
}

/// Год, платежи за который ждут ближе всех (`annual::due_year`); `None` —
/// профиль читается или его нет. Отслеживаемое.
fn due_year(ctx: tax::Ctx) -> Option<u16> {
    let today = ctx.today;
    ctx.sources.profiles.with(|profile| {
        let person = profile.map(|data| &data.person)?;
        Some(today.map_or(FALLBACK_YEAR, |today| annual::due_year(today, Some(person))))
    })
}

/// Статус проживания из профиля — для слов о медстраховке. Отслеживаемое.
fn residency(ctx: tax::Ctx) -> Option<Residency> {
    ctx.sources.profiles.with(|profile| profile.and_then(|data| data.person.residency))
}

/// Карточка на главном экране: годовые платежи ближайшего срока и сколько
/// откладывать в месяц. Вся карточка — ссылка на экран годовых платежей.
/// Без профиля, пока данные читаются и когда платить в срок нечего
/// (общий режим весь год, ИП ещё не был) — карточки нет.
#[component]
pub fn AnnualCard(ctx: tax::Ctx) -> impl IntoView {
    let lang = ctx.lang;
    let shown = Memo::new(move |_| {
        let year = due_year(ctx)?;
        let annual = ctx.sources.annual(year)?;
        if annual.status != Status::Turnover {
            return None;
        }
        let saving = ctx.today.and_then(|today| annual::saving(&annual, today));
        Some((year, Total::of(&annual), saving))
    });
    move || {
        shown.get().map(|(year, total, saving)| {
            view! {
                <a class="annual-card" href=Screen::Annual.href()>
                    <span class="annual-meta">{move || fill(t(lang.get(), Key::AnnualFor), &[&year.to_string()])}</span>
                    {total.due.map(|due| view! {
                        <span class="annual-label">
                            {move || fill(t(lang.get(), Key::AnnualDue), &[&i18n::date(lang.get(), deadlines::last_day(due))])}
                        </span>
                        {move || deadlines::moved(lang.get(), Kind::Annual(year), due).map(|note| view! { <span class="annual-note">{note}</span> })}
                    })}
                    {total.sum.map(|sum| view! { <span class="annual-sum">{move || drams(lang.get(), sum)}</span> })}
                    {saving.map(|saving| view! {
                        <span class="annual-note">{move || saving_text(lang.get(), saving, total.complete)}</span>
                    })}
                    {(!total.complete).then(|| view! { <span class="annual-note">{tr(lang, Key::AnnualPartialCard)}</span> })}
                </a>
            }
        })
    }
}

/// Строка на экране налога — к экрану годовых платежей: год, срок и сумма,
/// а когда посчитано не всё — и это.
#[component]
pub fn AnnualEntry(ctx: tax::Ctx) -> impl IntoView {
    let lang = ctx.lang;
    let shown = Memo::new(move |_| {
        let year = due_year(ctx)?;
        let annual = ctx.sources.annual(year)?;
        let total = (annual.status == Status::Turnover).then(|| Total::of(&annual));
        Some((year, total))
    });
    move || {
        shown.get().map(|(year, total)| {
            view! {
                <ul class="list annual-entry">
                    <li>
                        <a class="entry" href=Screen::Annual.href()>
                            <span class="entry-main">
                                <span class="entry-title">
                                    {move || fill(t(lang.get(), Key::AnnualFor), &[&year.to_string()])}
                                </span>
                                {total.map(|total| view! {
                                    <span class="entry-meta">
                                        {total.due.map(|due| view! {
                                            <span>{move || fill(t(lang.get(), Key::AnnualDue), &[&i18n::date(lang.get(), deadlines::last_day(due))])}</span>
                                            {move || deadlines::moved(lang.get(), Kind::Annual(year), due).map(|note| view! { <span>{note}</span> })}
                                        })}
                                        {(!total.complete).then(|| view! { <span>{tr(lang, Key::AnnualPartialShort)}</span> })}
                                    </span>
                                })}
                            </span>
                            {total.and_then(|total| total.sum).map(|sum| view! {
                                <span class="entry-side">
                                    <span class="entry-amount">{move || drams(lang.get(), sum)}</span>
                                </span>
                            })}
                            {icon(Icon::Forward)}
                        </a>
                    </li>
                </ul>
            }
        })
    }
}

/// Слова «отложите N в месяц»: когда посчитано не всё, к сроку наберётся
/// только посчитанное.
fn saving_text(lang: Lang, saving: Saving, complete: bool) -> String {
    if saving.months <= 1 {
        return t(lang, Key::AnnualSaveLast).to_owned();
    }
    let key = if complete { Key::AnnualSave } else { Key::AnnualSavePartial };
    fill(t(lang, key), &[&drams(lang, saving.per_month)])
}

/// Содержимое экрана: год стрелками, итог к сроку, сколько откладывать,
/// чего не хватает и каждый платёж своей карточкой.
#[component]
pub fn AnnualPage(ctx: tax::Ctx) -> impl IntoView {
    let lang = ctx.lang;
    let sources = ctx.sources;
    sources.watch(ctx.base);
    let chosen = RwSignal::new(None::<u16>);
    let year = Memo::new(move |_| {
        chosen
            .get()
            .or_else(|| due_year(ctx))
            .unwrap_or_else(|| ctx.today.map_or(FALLBACK_YEAR, Date::year))
    });
    let has_profile = Memo::new(move |_| sources.profiles.with(|profile| profile.is_some()));
    let shown = Memo::new(move |_| {
        if !has_profile.get() {
            return None;
        }
        Some((sources.annual(year.get())?, residency(ctx)))
    });
    let step = move |next: bool| {
        let now = year.get_untracked();
        let moved = if next { now.checked_add(1) } else { now.checked_sub(1) };
        if let Some(moved) = moved.filter(|moved| (Date::MIN_YEAR..Date::MAX_YEAR).contains(moved)) {
            chosen.set(Some(moved));
        }
    };
    let loaded = move || sources.profiles.with(|_| true);
    let rules_version = rules::armenia().version().to_string();
    view! {
        <div class="quarter-bar">
            <button
                type="button"
                class="round"
                aria-label=move || t(lang.get(), Key::AnnualPrevYear)
                on:click=move |_| step(false)
            >
                {icon(Icon::Back)}
            </button>
            <p class="quarter-name" aria-live="polite">
                {move || fill(t(lang.get(), Key::AnnualYear), &[&year.get().to_string()])}
            </p>
            <button
                type="button"
                class="round"
                aria-label=move || t(lang.get(), Key::AnnualNextYear)
                on:click=move |_| step(true)
            >
                {icon(Icon::Forward)}
            </button>
        </div>
        {move || sources.failure().map(|error| view! { <p role="alert">{tr(lang, error_key(error))}</p> })}
        {move || (loaded() && !has_profile.get()).then(|| view! {
            <div class="notice">
                <p>{tr(lang, Key::AnnualNoProfile)}</p>
                <a class="button" href=Screen::Profile.href()>{tr(lang, Key::ProfileFill)}</a>
            </div>
        })}
        {move || shown.get().map(|(annual, residency)| view! {
            <Payments lang annual residency today=ctx.today />
        })}
        <p class="tax-check">{tr(lang, Key::TaxCheck)}</p>
        <p class="hint">{move || fill(t(lang.get(), Key::AnnualRulesVersion), &[&rules_version])}</p>
    }
}

/// Итог года к сроку или слова об общем режиме, чего не хватает — и
/// платежи. Весь год на общем режиме — только медстраховка: её взнос от
/// режима не зависит, а остальное считает бухгалтер.
#[component]
fn Payments(lang: ReadSignal<Lang>, annual: Annual, residency: Option<Residency>, today: Option<Date>) -> impl IntoView {
    let year = annual.year;
    let head = match annual.status {
        Status::NotYet => {
            return view! {
                <div class="notice">
                    <p>{move || fill(t(lang.get(), Key::AnnualNotYet), &[&year.to_string()])}</p>
                </div>
            }
            .into_any();
        }
        Status::General => {
            let code = match annual.months.left {
                Some((from, Leaving::Code(code))) => Some((from, code)),
                Some((_, Leaving::Threshold | Leaving::PreviousYear)) | None => None,
            };
            view! {
                <div class="notice">
                    <p>{move || {
                        let lang = lang.get();
                        match code {
                            Some((from, code)) => fill(t(lang, Key::AnnualGeneralCode), &[&i18n::date(lang, from), &code.to_string()]),
                            None => t(lang, Key::AnnualGeneral).to_owned(),
                        }
                    }}</p>
                </div>
            }
            .into_any()
        }
        Status::Turnover => view! { <Summary lang annual=annual.clone() today /> }.into_any(),
    };
    let asks = asks(&annual);
    let alerts = alerts(&annual);
    let cards = if annual.status == Status::General {
        vec![health_card(&annual, residency)]
    } else {
        vec![
            profit_card(&annual),
            social_card(&annual),
            stamp_card(&annual, today),
            health_card(&annual, residency),
        ]
    };
    view! {
        {head}
        {(!asks.is_empty()).then(|| view! {
            <div class="notice">
                {asks.into_iter().map(|key| view! { <p role="alert">{tr(lang, key)}</p> }).collect_view()}
                <a class="button" href=Screen::Profile.href()>{tr(lang, Key::ProfileEdit)}</a>
            </div>
        })}
        {alerts.into_iter().map(|alert| view! { <p role="alert">{move || alert.text(lang.get())}</p> }).collect_view()}
        {cards.into_iter().map(|card| view! { <PaymentCard lang card /> }).collect_view()}
    }
    .into_any()
}

/// Итог года к сроку: сумма, сколько откладывать и посчитано ли всё. Суммы
/// нет — не известен ни один платёж: ноль выглядел бы расчётом (Правило 6).
#[component]
fn Summary(lang: ReadSignal<Lang>, annual: Annual, today: Option<Date>) -> impl IntoView {
    let total = Total::of(&annual);
    let saving = today.and_then(|today| annual::saving(&annual, today));
    // Срок — с переносом с нерабочего дня, как в календаре сроков
    // (`tax::deadlines`, задача 21).
    let by_law = total.due;
    let due = by_law.map(deadlines::last_day);
    let kind = Kind::Annual(annual.year);
    let passed = due.zip(today).is_some_and(|(due, today)| due < today);
    view! {
        <div class="hero-card">
            {due.map(|due| view! {
                <span class="hero-meta">
                    {move || {
                        let lang = lang.get();
                        let key = if passed { Key::AnnualDuePassed } else { Key::AnnualDue };
                        fill(t(lang, key), &[&i18n::date(lang, due)])
                    }}
                </span>
            })}
            {by_law.map(|by_law| move || deadlines::moved(lang.get(), kind, by_law).map(|note| view! {
                <span class="hero-note">{note}</span>
            }))}
            <span class="hero-label">{tr(lang, Key::AnnualSumOf)}</span>
            {match total.sum {
                Some(sum) => view! { <span class="hero-sum">{move || drams(lang.get(), sum)}</span> }.into_any(),
                None => view! { <span class="hero-note">{tr(lang, Key::AnnualSumUnknown)}</span> }.into_any(),
            }}
            {saving.map(|saving| view! {
                <span class="hero-note">{move || saving_text(lang.get(), saving, total.complete)}</span>
            })}
            {(!total.complete && total.sum.is_some()).then(|| view! {
                <span class="hero-note">{tr(lang, Key::AnnualPartial)}</span>
            })}
        </div>
    }
}

/// Чего не хватает в профиле — у всех платежей вместе, без повторов.
fn asks(annual: &Annual) -> Vec<Key> {
    let social = if let Social::Unknown(missing) = annual.social { missing } else { Missing::default() };
    let stamp = if let Stamp::Unknown(missing) = annual.stamp { missing } else { Missing::default() };
    let health = if let Health::Unknown(missing) = annual.health { missing } else { Missing::default() };
    let missing = [social, stamp, health];
    let any = |pick: fn(&Missing) -> bool| missing.iter().any(pick);
    let mut asks = Vec::new();
    // Без дня регистрации месяцы — с января: на общем режиме весь год они
    // не нужны.
    if annual.status == Status::Turnover && annual.months.registration_unknown {
        asks.push(Key::AnnualAskRegistered);
    }
    if any(|missing| missing.birth_year) {
        asks.push(Key::AnnualAskBirthYear);
    }
    if any(|missing| missing.pension_joined) {
        asks.push(Key::AnnualAskPension);
    }
    if any(|missing| missing.residency) {
        asks.push(Key::AnnualAskResidency);
    }
    asks
}

/// Что окно обязано сказать о годе — красным над платежами: без этого
/// числа неполны (Правило 6).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Alert {
    /// Документов и записей книги, которые не читаются.
    Unreadable(usize),
    /// Суммы переполнились: данные испорчены.
    Broken,
    /// Доходов без курса в прошлом и этом году — годы и число.
    RegimeUnrated { year: u16, rows: usize },
}

impl Alert {
    fn text(self, lang: Lang) -> String {
        let count = |count: usize| i18n::integer(lang, count as i64);
        match self {
            Self::Unreadable(records) => fill(t(lang, Key::TaxUnreadableRecords), &[&count(records)]),
            Self::Broken => t(lang, Key::AnnualBroken).to_owned(),
            Self::RegimeUnrated { year, rows } => fill(
                t(lang, Key::AnnualRegimeUnrated),
                &[&year.saturating_sub(1).to_string(), &year.to_string(), &count(rows)],
            ),
        }
    }
}

/// Предупреждения года. Доход без курса у порога налога с оборота — только
/// пока год на нём: на общем режиме весь год порог уже перейдён.
fn alerts(annual: &Annual) -> Vec<Alert> {
    let mut alerts = Vec::new();
    if annual.unreadable > 0 {
        alerts.push(Alert::Unreadable(annual.unreadable));
    }
    if annual.broken {
        alerts.push(Alert::Broken);
    }
    if annual.status == Status::Turnover && annual.regime_unrated > 0 {
        alerts.push(Alert::RegimeUnrated {
            year: annual.year,
            rows: annual.regime_unrated,
        });
    }
    alerts
}

/// Сумма за месяцы: «12 месяцев × 5 000 ֏».
fn months_times(lang: Lang, months: u8, monthly: i64) -> String {
    let count = i18n::integer(lang, i64::from(months));
    let months = fill(
        plural(lang, u64::from(months), Key::MonthsOne, Key::MonthsFew, Key::MonthsMany),
        &[&count],
    );
    fill(t(lang, Key::AnnualTimes), &[&months, &drams(lang, monthly)])
}

/// Откуда сумма платежа — подпись слева от неё.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Basis {
    /// «12 месяцев × 5 000 ֏».
    Months { months: u8, monthly: i64 },
    /// Оборот года — своей строкой: «Оборот за 2026 год — 3 840 000 ֏»,
    /// а под ним «Сбор»: в одну строку подпись с оборотом столкнула бы сумму
    /// на телефоне в начало следующей (глазной прогон задачи 20).
    Turnover { year: u16, base: i64 },
    /// Осторожная нижняя ступень гербового сбора.
    Cautious,
    /// Годовая ставка медстраховки одной суммой.
    YearRate,
}

impl Basis {
    fn text(self, lang: Lang) -> String {
        match self {
            Self::Months { months, monthly } => months_times(lang, months, monthly),
            Self::Turnover { .. } => t(lang, Key::AnnualStampSum).to_owned(),
            Self::Cautious => t(lang, Key::AnnualStampCautious).to_owned(),
            Self::YearRate => t(lang, Key::AnnualHealthYearRate).to_owned(),
        }
    }
}

/// Сумма платежа или слово вместо неё.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Amount {
    Sum(Basis, i64),
    /// Не платится.
    Nothing,
    /// Не посчитан: почему — словами под ним или над платежами.
    Unknown,
}

/// Срок платежа.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Due {
    Day(Date),
    /// Каждый месяц — до этого числа следующего.
    Monthly(u8),
}

/// Что сказать под платежом. Красное (`is_alert`) — то, без чего число
/// неполно (Правило 6).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Note {
    ProfitLaw,
    Registered(Date),
    /// День регистрации не указан — год, о котором речь.
    RegistrationUnknown(u16),
    Stopped(u8),
    /// Доход в целом месяце остановки — его день.
    StopIncome(Date),
    Left(Date),
    SocialLaw(u16),
    SocialVoluntary,
    SocialNotJoined(u16),
    SocialNoStatus,
    SocialStops,
    NoRule,
    StampScale(u16),
    StampMayGrow { up_to: i64, next: i64 },
    StampUnrated { rows: usize, up_to: i64, next: i64 },
    StampNoTurnover(u16),
    StampStopped,
    StampExempt,
    StampLeft,
    HealthYearWhy { year: u16, income: i64, over: i64 },
    HealthMonthLaw,
    HealthAfterYear,
    HealthEmployed,
    HealthStateless,
    HealthBelow { year: u16, income: i64, over: i64 },
    /// ИП зарегистрирован в году годовой ставки — этот год.
    HealthNewIe(u16),
    HealthStopped(Date),
    /// Остановлена в день `stopped`, возобновлена `resumed`.
    HealthResumedMonthly { stopped: Date, resumed: Date },
    HealthStoppedInForce(Date),
    HealthStoppedMaybe(Date),
    HealthElderly(u8),
    HealthNoStatus,
    HealthNoPrevious { year: u16, over: i64, amount: i64, due: Date },
    HealthPreviousUnrated { year: u16, rows: usize, income: i64, over: i64, amount: i64, due: Date },
    HealthTurnsAge(u8),
    HealthResumed(Date),
    HealthBefore(u16),
}

impl Note {
    fn is_alert(self) -> bool {
        matches!(
            self,
            Self::NoRule
                | Self::StopIncome(_)
                | Self::StampUnrated { .. }
                | Self::HealthNewIe(_)
                | Self::HealthStoppedInForce(_)
                | Self::HealthStoppedMaybe(_)
                | Self::HealthElderly(_)
                | Self::HealthNoPrevious { .. }
                | Self::HealthPreviousUnrated { .. }
                | Self::HealthTurnsAge(_)
                | Self::HealthResumed(_)
        )
    }

    fn text(self, lang: Lang) -> String {
        let date = |day: Date| i18n::date(lang, day);
        let count = |count: usize| i18n::integer(lang, count as i64);
        let money = |sum: i64| drams(lang, sum);
        match self {
            Self::ProfitLaw => t(lang, Key::AnnualProfitLaw).to_owned(),
            Self::Registered(day) => fill(t(lang, Key::AnnualRegistered), &[&date(day)]),
            Self::RegistrationUnknown(year) => fill(t(lang, Key::AnnualRegistrationUnknown), &[&year.to_string()]),
            Self::Stopped(months) => fill(t(lang, Key::AnnualStopped), &[&count(usize::from(months))]),
            Self::StopIncome(day) => fill(t(lang, Key::AnnualStopIncome), &[&date(day)]),
            Self::Left(day) => fill(t(lang, Key::AnnualLeft), &[&date(day)]),
            Self::SocialLaw(year) => fill(t(lang, Key::AnnualSocialLaw), &[&year.to_string()]),
            Self::SocialVoluntary => t(lang, Key::AnnualSocialVoluntary).to_owned(),
            Self::SocialNotJoined(year) => fill(t(lang, Key::AnnualSocialNotJoined), &[&year.to_string()]),
            Self::SocialNoStatus => t(lang, Key::AnnualSocialNoStatus).to_owned(),
            Self::SocialStops => t(lang, Key::AnnualSocialStops).to_owned(),
            Self::NoRule => t(lang, Key::AnnualNoRule).to_owned(),
            Self::StampScale(year) => fill(t(lang, Key::AnnualStampScale), &[&scale(lang, year)]),
            Self::StampMayGrow { up_to, next } => fill(t(lang, Key::AnnualStampMayGrow), &[&money(up_to), &money(next)]),
            Self::StampUnrated { rows, up_to, next } => {
                fill(t(lang, Key::AnnualStampUnrated), &[&count(rows), &money(up_to), &money(next)])
            }
            Self::StampNoTurnover(year) => fill(t(lang, Key::AnnualStampNoTurnover), &[&year.to_string()]),
            Self::StampStopped => t(lang, Key::AnnualStampStopped).to_owned(),
            Self::StampExempt => t(lang, Key::AnnualStampExempt).to_owned(),
            Self::StampLeft => t(lang, Key::AnnualStampLeft).to_owned(),
            Self::HealthYearWhy { year, income, over } => {
                fill(t(lang, Key::AnnualHealthYearWhy), &[&year.to_string(), &money(income), &money(over)])
            }
            Self::HealthMonthLaw => t(lang, Key::AnnualHealthMonthLaw).to_owned(),
            Self::HealthAfterYear => t(lang, Key::AnnualHealthAfterYear).to_owned(),
            Self::HealthEmployed => t(lang, Key::AnnualHealthEmployed).to_owned(),
            Self::HealthStateless => t(lang, Key::AnnualHealthStateless).to_owned(),
            Self::HealthBelow { year, income, over } => {
                fill(t(lang, Key::AnnualHealthBelow), &[&year.to_string(), &money(income), &money(over)])
            }
            Self::HealthNewIe(year) => fill(t(lang, Key::AnnualHealthNewIe), &[&year.to_string()]),
            Self::HealthStopped(day) => fill(t(lang, Key::AnnualHealthStopped), &[&date(day)]),
            Self::HealthResumedMonthly { stopped, resumed } => {
                fill(t(lang, Key::AnnualHealthResumedMonthly), &[&date(stopped), &date(resumed)])
            }
            Self::HealthStoppedInForce(day) => fill(t(lang, Key::AnnualHealthStoppedInForce), &[&date(day)]),
            Self::HealthStoppedMaybe(day) => fill(t(lang, Key::AnnualHealthStoppedMaybe), &[&date(day)]),
            Self::HealthElderly(age) => fill(t(lang, Key::AnnualHealthElderly), &[&age.to_string()]),
            Self::HealthNoStatus => t(lang, Key::AnnualHealthNoStatus).to_owned(),
            Self::HealthNoPrevious { year, over, amount, due } => fill(
                t(lang, Key::AnnualHealthNoPrevious),
                &[&year.to_string(), &money(over), &money(amount), &date(deadlines::last_day(due))],
            ),
            Self::HealthPreviousUnrated {
                year,
                rows,
                income,
                over,
                amount,
                due,
            } => fill(
                t(lang, Key::AnnualHealthPreviousUnrated),
                &[&year.to_string(), &count(rows), &money(income), &money(over), &money(amount), &date(deadlines::last_day(due))],
            ),
            Self::HealthTurnsAge(age) => fill(t(lang, Key::AnnualHealthTurnsAge), &[&age.to_string()]),
            Self::HealthResumed(day) => fill(t(lang, Key::AnnualHealthResumed), &[&date(day)]),
            Self::HealthBefore(year) => fill(t(lang, Key::AnnualHealthBefore), &[&year.to_string()]),
        }
    }
}

/// Шкала гербового сбора года словами: «до 12 000 000 ֏ — 12 000 ֏,
/// больше 12 000 000 ֏ — 120 000 ֏». Соседние ступени с той же суммой —
/// одна. Правила на год нет — пусто: карточка тогда говорит «правила нет».
fn scale(lang: Lang, year: u16) -> String {
    let Some(rule) = rules::for_year(rules::armenia().stamp_duty(), year) else {
        return String::new();
    };
    let mut parts = Vec::new();
    let mut below = None;
    for (index, step) in rule.steps.iter().enumerate() {
        let merged = rule.steps.get(index + 1).is_some_and(|next| next.amount == step.amount);
        if merged {
            continue;
        }
        let amount = drams(lang, step.amount);
        match step.up_to {
            Some(up_to) => parts.push(fill(t(lang, Key::AnnualStampStep), &[&drams(lang, up_to), &amount])),
            None => match below {
                Some(below) => parts.push(fill(t(lang, Key::AnnualStampTop), &[&drams(lang, below), &amount])),
                None => parts.push(amount),
            },
        }
        below = step.up_to;
    }
    parts.join(", ")
}

/// Карточка платежа — что показать.
#[derive(Clone, PartialEq, Eq, Debug)]
struct Card {
    title: Key,
    amount: Amount,
    due: Option<Due>,
    notes: Vec<Note>,
}

/// Карточка платежа: название, сумма или слово вместо неё, срок и слова —
/// почему так.
#[component]
fn PaymentCard(lang: ReadSignal<Lang>, card: Card) -> impl IntoView {
    let Card {
        title,
        amount,
        due,
        notes,
    } = card;
    let pair = move |label: String, value: String| {
        view! {
            <div>
                <dt>{label}</dt>
                <dd>{value}</dd>
            </div>
        }
    };
    view! {
        <section class="tax-line annual-line">
            <h3>{tr(lang, title)}</h3>
            {move || {
                let lang = lang.get();
                let mut rows = Vec::new();
                match amount {
                    Amount::Sum(basis, sum) => {
                        if let Basis::Turnover { year, base } = basis {
                            rows.push(pair(
                                fill(t(lang, Key::AnnualStampTurnover), &[&year.to_string()]),
                                drams(lang, base),
                            ));
                        }
                        rows.push(pair(basis.text(lang), drams(lang, sum)));
                    }
                    Amount::Nothing => rows.push(pair(t(lang, Key::AnnualNone).to_owned(), drams(lang, 0))),
                    Amount::Unknown => rows.push(pair(t(lang, Key::AnnualUnknown).to_owned(), String::new())),
                }
                match due {
                    Some(Due::Day(day)) => rows.push(pair(t(lang, Key::AnnualDueLabel).to_owned(), i18n::date(lang, deadlines::last_day(day)))),
                    Some(Due::Monthly(day)) => rows.push(pair(
                        t(lang, Key::AnnualDueLabel).to_owned(),
                        fill(t(lang, Key::AnnualHealthMonthDue), &[&day.to_string()]),
                    )),
                    None => {}
                }
                view! { <dl class="pairs">{rows}</dl> }
            }}
            {notes.into_iter().map(|note| {
                if note.is_alert() {
                    view! { <p role="alert">{move || note.text(lang.get())}</p> }.into_any()
                } else {
                    view! { <p class="hint">{move || note.text(lang.get())}</p> }.into_any()
                }
            }).collect_view()}
        </section>
    }
}

/// Налог на прибыль — и как посчитаны месяцы года.
fn profit_card(annual: &Annual) -> Card {
    let months = &annual.months;
    let mut notes = Vec::new();
    if matches!(annual.profit, Fixed::Due { .. }) {
        notes.push(Note::ProfitLaw);
    }
    if let Some(day) = months.registered {
        notes.push(Note::Registered(day));
    } else if months.registration_unknown {
        notes.push(Note::RegistrationUnknown(annual.year));
    }
    if !months.stopped.is_empty() {
        notes.push(Note::Stopped(months.stopped.len() as u8));
    }
    if let Some(day) = annual.stop_income {
        notes.push(Note::StopIncome(day));
    }
    if let Some((day, Leaving::Threshold | Leaving::Code(_))) = months.left {
        notes.push(Note::Left(day));
    }
    let (amount, due) = match annual.profit {
        Fixed::Due {
            monthly,
            months,
            amount,
            due,
        } => (Amount::Sum(Basis::Months { months, monthly }, amount), Some(Due::Day(due))),
        Fixed::NoRule => {
            notes.push(Note::NoRule);
            (Amount::Unknown, None)
        }
    };
    Card {
        title: Key::AnnualProfit,
        amount,
        due,
        notes,
    }
}

/// Год, с которого участие в накопительной пенсии обязательно, — по правилу
/// на начало года.
fn mandatory_from(year: u16) -> Option<u16> {
    Date::new(year, 1, 1)
        .and_then(annual::funded_pension)
        .map(|rule| rule.mandatory_born_from.year())
}

/// Соцплатёж.
fn social_card(annual: &Annual) -> Card {
    let mandatory = mandatory_from(annual.year);
    let (amount, due, notes) = match annual.social {
        Social::Due {
            payment: Fixed::Due {
                monthly,
                months,
                amount,
                due,
            },
            voluntary,
        } => {
            let mut notes = Vec::new();
            if voluntary {
                notes.push(Note::SocialVoluntary);
            } else if let Some(year) = mandatory {
                notes.push(Note::SocialLaw(year));
            }
            if !annual.months.stopped.is_empty() {
                notes.push(Note::SocialStops);
            }
            (Amount::Sum(Basis::Months { months, monthly }, amount), Some(Due::Day(due)), notes)
        }
        Social::Out(SocialOut::NotJoined) => (
            Amount::Nothing,
            None,
            mandatory.map(Note::SocialNotJoined).into_iter().collect(),
        ),
        Social::Out(SocialOut::NoStatus) => (Amount::Nothing, None, vec![Note::SocialNoStatus]),
        Social::Unknown(_) => (Amount::Unknown, None, Vec::new()),
        Social::Due {
            payment: Fixed::NoRule,
            ..
        }
        | Social::NoRule => (Amount::Unknown, None, vec![Note::NoRule]),
    };
    Card {
        title: Key::AnnualSocial,
        amount,
        due,
        notes,
    }
}

/// Гербовый сбор.
fn stamp_card(annual: &Annual, today: Option<Date>) -> Card {
    let year = annual.year;
    let left = matches!(annual.months.left, Some((_, Leaving::Threshold | Leaving::Code(_))));
    let (amount, due, mut notes) = match annual.stamp {
        Stamp::Due {
            amount,
            base,
            up_to,
            next,
            due,
            unrated,
        } => {
            let mut notes = vec![Note::StampScale(year)];
            if let (Some(up_to), Some((_, next))) = (up_to, next) {
                if unrated > 0 {
                    notes.push(Note::StampUnrated {
                        rows: unrated,
                        up_to,
                        next,
                    });
                } else if today.is_some_and(|today| today.year() <= year) {
                    notes.push(Note::StampMayGrow { up_to, next });
                }
            }
            (Amount::Sum(Basis::Turnover { year, base }, amount), Some(Due::Day(due)), notes)
        }
        Stamp::NoTurnover { cautious, due } => (
            Amount::Sum(Basis::Cautious, cautious),
            Some(Due::Day(due)),
            vec![Note::StampNoTurnover(year), Note::StampScale(year)],
        ),
        Stamp::Stopped => (Amount::Nothing, None, vec![Note::StampStopped]),
        Stamp::Exempt => (Amount::Nothing, None, vec![Note::StampExempt]),
        Stamp::Unknown(_) => (Amount::Unknown, None, Vec::new()),
        Stamp::NoRule => (Amount::Unknown, None, vec![Note::NoRule]),
    };
    if left && matches!(amount, Amount::Sum(..)) {
        notes.push(Note::StampLeft);
    }
    Card {
        title: Key::AnnualStamp,
        amount,
        due,
        notes,
    }
}

/// Первый год, за который у программы есть правило медстраховки.
fn first_health_year() -> Option<u16> {
    rules::armenia()
        .health_insurance()
        .iter()
        .map(|rule| rule.from.year())
        .min()
}

/// Взнос медстраховки. Испорченные записи (`HealthUnclear::Broken`) —
/// «не посчитан» без слов под ним: о них говорит предупреждение над
/// платежами.
fn health_card(annual: &Annual, residency: Option<Residency>) -> Card {
    let year = annual.year;
    let previous = year.saturating_sub(1);
    // Лицо без гражданства закон о медстраховке не называет (ст. 11, ч. 1),
    // а профиль держит его вместе с иностранцем со статусом проживания.
    let stateless = residency == Some(Residency::ForeignWithStatus);
    let (amount, due, notes) = match annual.health {
        Health::Year {
            amount,
            due,
            over,
            income,
        } => {
            let mut notes = vec![Note::HealthYearWhy {
                year: previous,
                income,
                over,
            }];
            if stateless {
                notes.push(Note::HealthStateless);
            }
            (Amount::Sum(Basis::YearRate, amount), Some(Due::Day(due)), notes)
        }
        Health::Month {
            monthly,
            due_day,
            months,
            amount,
            after,
        } => {
            let mut notes = vec![Note::HealthMonthLaw];
            let rule = rules::for_year(rules::armenia().health_insurance(), year);
            if let (Some(resumed), Some(rule)) = (after, rule) {
                notes.push(Note::HealthResumedMonthly {
                    stopped: rule.starts,
                    resumed,
                });
            }
            let after_year = rules::for_year(rules::armenia().health_insurance(), previous)
                .is_some_and(|rule| matches!(rule.by, HealthBy::Year { .. }));
            if after_year {
                notes.push(Note::HealthAfterYear);
            }
            notes.push(Note::HealthEmployed);
            if stateless {
                notes.push(Note::HealthStateless);
            }
            (Amount::Sum(Basis::Months { months, monthly }, amount), Some(Due::Monthly(due_day)), notes)
        }
        Health::Out(out) => {
            let note = match out {
                HealthOut::BelowThreshold { over, income } => Note::HealthBelow {
                    year: previous,
                    income,
                    over,
                },
                HealthOut::Stopped(day) => Note::HealthStopped(day),
                HealthOut::NoStatus => Note::HealthNoStatus,
            };
            (Amount::Nothing, None, vec![note])
        }
        Health::Unknown(_) => (Amount::Unknown, None, Vec::new()),
        Health::Unclear(unclear) => {
            let note = match unclear {
                HealthUnclear::NoPreviousYear { over, amount, due } => Some(Note::HealthNoPrevious {
                    year: previous,
                    over,
                    amount,
                    due,
                }),
                HealthUnclear::PreviousUnrated {
                    over,
                    income,
                    unrated,
                    amount,
                    due,
                } => Some(Note::HealthPreviousUnrated {
                    year: previous,
                    rows: unrated,
                    income,
                    over,
                    amount,
                    due,
                }),
                HealthUnclear::Elderly(age) => Some(Note::HealthElderly(age)),
                HealthUnclear::TurnsAge(age) => Some(Note::HealthTurnsAge(age)),
                HealthUnclear::Resumed(day) => Some(Note::HealthResumed(day)),
                HealthUnclear::NewIe => Some(Note::HealthNewIe(year)),
                HealthUnclear::StoppedInForce { starts, known: true } => Some(Note::HealthStoppedInForce(starts)),
                HealthUnclear::StoppedInForce { starts, known: false } => Some(Note::HealthStoppedMaybe(starts)),
                HealthUnclear::Broken => None,
            };
            (Amount::Unknown, None, note.into_iter().collect())
        }
        Health::NoRule => match first_health_year().filter(|first| year < *first) {
            // До первого правила медстраховки у ИП не было: закон ՀՕ-459-Ն
            // принят в декабре 2025 года.
            Some(first) => (Amount::Nothing, None, vec![Note::HealthBefore(first)]),
            None => (Amount::Unknown, None, vec![Note::NoRule]),
        },
    };
    Card {
        title: Key::AnnualHealth,
        amount,
        due,
        notes,
    }
}
