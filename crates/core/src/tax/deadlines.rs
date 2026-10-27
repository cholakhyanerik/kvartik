//! Сроки (задача 21): до какого дня подать расчёт и заплатить налог
//! с оборота за квартал, подать заявление о налоге с оборота на год,
//! заплатить годовые платежи и взнос медстраховки, — с переносом
//! последнего дня с нерабочего дня на следующий рабочий.
//!
//! Дни — записи файла правил (`tax::rules`) с источником: число месяца
//! после квартала (`turnover_due`), день заявления (`turnover_application`),
//! перенос и выходные дни недели (`deadline_shift`), праздники
//! (`holidays`) и решения правительства о нерабочих и рабочих днях
//! (`days_off`), всеобщая декларация о доходах гражданина
//! (`income_declaration`). Сроки годовых платежей и взноса медстраховки
//! считает `tax::annual` — здесь их только переносят, а не считают второй
//! раз.
//!
//! # Перенос
//!
//! В Налоговом кодексе общего правила переноса нет: последний день срока,
//! пришедшийся на нерабочий, переносит закон о нормативных правовых актах
//! (ՀՕ-180-Ն, ст. 32, ч. 4) — на следующий рабочий. Так его применяет к
//! срокам НК и ՊԵԿ: срок всеобщей декларации за 2025 год, воскресенье
//! 1 ноября 2026 года, по этому закону «տվյալ դեպքում՝ նոյեմբերի 2-ը»
//! (новость ՊԵԿ № 1816 от 2026-10-26, https://www.src.am/am/getNews/1816;
//! сверено 2026-10-27, задача 74). Новость — о подаче. Уплатой считается
//! день зачисления на единый счёт (НК, ст. 18, ч. 4), а о переносе её дня
//! ՊԵԿ не говорил: у срока, к которому платят, окно добавляет, что заплатить
//! надёжнее до дня по закону ([`moved`]; решение агента 2026-10-27 — пока
//! бухгалтера нет, решает он, решение человека). Доводы за и против
//! переноса до новости — FEATURE_TASKS.md, «Перенос срока».
//!
//! # Что сделано, программа знает только о квартале
//!
//! Подан ли расчёт квартала, говорит его закрытие (`tax::closing`): прошёл
//! срок, а квартал не закрыт — расчёт, по всей видимости, не подан
//! ([`overdue`]). Об уплате, заявлении и годовых платежах программе никто не
//! сообщает: у них напоминание до дня срока, а после — следующий срок.
//!
//! # Сроки этого ИП и сроки всех
//!
//! [`between`] — сроки по профилю и книге: экран сроков и личный календарь
//! (`tax::calendar`). [`general`] — сроки по одним правилам, без профиля:
//! общий календарь на сайте (задача 22). Дни у обоих — одни и те же функции
//! от правил, а не второй расчёт.

use super::annual::{self, Health, Status, Total};
use super::closing::Closing;
use super::rules::{self, HealthBy, Rule, Rules};
use super::turnover::Books;
use crate::i18n::{self, Key, Lang, fill, t};
use crate::model::date::{Date, Quarter};
use crate::model::profile::{Person, Residency};

/// За сколько дней до срока напоминать: модуль «Календарь» из описания
/// продукта — напоминание за 7 дней и за день.
pub const REMIND_DAYS: i32 = 7;

/// На сколько дней вперёд показывать сроки — экран сроков и личный
/// календарь (`tax::calendar`): год — все сроки года, включая годовые
/// платежи и заявление.
pub const AHEAD_DAYS: i32 = 366;

/// Сколько нерабочих дней подряд перенос перешагивает, не больше: по
/// правилам их не бывает больше шести (31 декабря — 2 января и выходные).
/// Предел держит окно, если правила когда-нибудь скажут «нерабочий» на
/// месяц подряд, — разбор этого не пропустит (`DeadlineShift`), но цикл
/// без предела зависал бы молча.
const MAX_SHIFT: usize = 31;

/// Что надо сделать к сроку.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Kind {
    /// Подать расчёт и заплатить налог с оборота за квартал.
    Turnover(Quarter),
    /// Подать заявление о налоге с оборота на год.
    Application(u16),
    /// Заплатить налог на прибыль, соцплатёж и гербовый сбор за год.
    Annual(u16),
    /// Заплатить взнос медстраховки за год одной суммой.
    HealthYear(u16),
    /// Заплатить взнос медстраховки за месяц `month` года `year`.
    HealthMonth { year: u16, month: u8 },
    /// Подать всеобщую декларацию о доходах за год — гражданину (решение
    /// человека 2026-10-20: доход ИП в ней подоходным налогом не облагается,
    /// но декларацию гражданин подаёт, и о ней спрашивают).
    Declaration(u16),
}

/// Срок.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Deadline {
    pub kind: Kind,
    /// Последний день по закону.
    pub by_law: Date,
    /// Последний день с переносом: `by_law`, если он рабочий, иначе —
    /// следующий рабочий.
    pub day: Date,
    /// Сделано ли: квартал закрыт — расчёт подан (`tax::closing`). О других
    /// сроках программе никто не сообщает, и у них — `false`.
    pub done: bool,
}

impl Deadline {
    fn new(kind: Kind, by_law: Date) -> Self {
        Self {
            kind,
            by_law,
            day: last_day(by_law),
            done: false,
        }
    }
}

/// Что надо сделать к сроку — словами: экран сроков, напоминание и календарь
/// (`tax::calendar`) говорят одно.
pub fn what(lang: Lang, kind: Kind) -> String {
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

/// Платят ли к сроку: перенос дня уплаты ՊԵԿ не подтверждал («Перенос»
/// в шапке). К сроку квартала и подают расчёт, и платят.
const fn pays(kind: Kind) -> bool {
    match kind {
        Kind::Turnover(_) | Kind::Annual(_) | Kind::HealthYear(_) | Kind::HealthMonth { .. } => true,
        Kind::Application(_) | Kind::Declaration(_) => false,
    }
}

/// Слова под перенесённым днём срока `kind`, который по закону кончается
/// `by_law`: с какого дня он перенесён, а если к сроку платят, — что
/// заплатить надёжнее до этого дня. `None` — день рабочий, переноса нет.
/// Слова одни у экрана сроков, напоминания, карточек и календаря.
pub fn moved(lang: Lang, kind: Kind, by_law: Date) -> Option<String> {
    let key = if pays(kind) { Key::DeadlineShiftedPay } else { Key::DeadlineShifted };
    (last_day(by_law) != by_law).then(|| fill(t(lang, key), &[&i18n::date(lang, by_law)]))
}

/// Насколько близок срок в день `today`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum When {
    /// Прошёл, а дело не сделано (только квартал: [`overdue`]).
    Overdue,
    Today,
    Tomorrow,
    /// Через столько дней — от двух до [`REMIND_DAYS`].
    In(i32),
}

/// Напоминание: срок и насколько он близок.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Reminder {
    pub deadline: Deadline,
    pub when: When,
}

/// Рабочий ли день `day` для срока по правилам `rules`: рабочий по решению
/// правительства — рабочий, хоть и выходной; нерабочий по решению,
/// праздник или выходной день недели — нет. Правил на этот день нет —
/// рабочий: переносить не по чему.
fn working(rules: &Rules, day: Date) -> bool {
    let decided = rules.days_off().iter().filter(|decision| decision.period().contains(day));
    let mut off = false;
    for decision in decided {
        if decision.work.contains(&day) {
            return true;
        }
        off |= decision.off.contains(&day);
    }
    if off || rules::on(rules.holidays(), day).is_some_and(|holidays| holidays.contains(day)) {
        return false;
    }
    !rules::on(rules.deadline_shift(), day).is_some_and(|shift| shift.weekend.contains(&day.weekday()))
}

/// Последний день срока, который по закону кончается в день `day`: он же,
/// если рабочий, иначе — следующий рабочий (ՀՕ-180-Ն, ст. 32, ч. 4).
pub fn last_day(day: Date) -> Date {
    shifted(rules::armenia(), day)
}

fn shifted(rules: &Rules, day: Date) -> Date {
    let mut last = day;
    for _ in 0..MAX_SHIFT {
        if working(rules, last) {
            return last;
        }
        match last.add_days(1) {
            Some(next) => last = next,
            None => break,
        }
    }
    day
}

/// Срок расчёта и налога с оборота за квартал `quarter` по закону —
/// число месяца после квартала по правилу, которое действует в первый день
/// этого месяца. `None` — правила нет.
pub fn turnover(quarter: Quarter) -> Option<Deadline> {
    let month = quarter.next()?.first_day();
    let rule = rules::on(rules::armenia().turnover_due(), month)?;
    Some(Deadline::new(Kind::Turnover(quarter), quarter.day_in_following_month(rule.day)?))
}

/// Подаёт ли ИП расчёт за квартал: был на налоге с оборота хоть месяц
/// квартала — по месяцам года, которые считает `tax::annual` (регистрация,
/// уход с режима), — и квартал не лежит целиком внутри остановки
/// деятельности: за полные отчётные периоды остановки расчёт, и нулевой,
/// не подают (НК, ст. 53, ч. 8). Неполный квартал остановки — подают:
/// оборот 261(1) не условие (разведка задачи 21, 2026-10-20).
fn files(span: Option<(u8, u8)>, person: Option<&Person>, quarter: Quarter) -> bool {
    let first = quarter.first_day().month();
    let last = first + 2;
    let on_turnover = span.is_some_and(|(from, until)| from <= last && until >= first);
    let stopped = person.is_some_and(|person| {
        person.stops.iter().any(|stop| {
            stop.from() <= quarter.first_day() && stop.until().is_none_or(|until| until > quarter.last_day())
        })
    });
    on_turnover && !stopped
}

/// Сроки ИП, которые с переносом кончаются с `from` по `until`
/// включительно, — по дням, а в один день — по порядку [`Kind`].
///
/// Квартал — пока ИП на налоге с оборота и не остановил деятельность на
/// весь квартал ([`files`]); заявление — у того, кто в этом году на налоге
/// с оборота ([`application`]); годовые платежи — за год на нём;
/// медстраховка — когда известно, что ИП её платит. Годовые платежи
/// и медстраховка года — в следующем году, поэтому смотрится и год до
/// `from`.
pub fn between(books: Books<'_>, from: Date, until: Date) -> Vec<Deadline> {
    let mut found = Vec::new();
    let all = rules::armenia();
    let person = books.profile.map(|profile| &profile.person);
    let first = from.year().saturating_sub(1).max(Date::MIN_YEAR);
    for year in first..=until.year() {
        let annual = annual::year(books, year);
        for number in 1..=4 {
            if let Some(quarter) = Quarter::new(year, number)
                && let Some(deadline) = filed(books, &annual, person, quarter)
            {
                found.push(deadline);
            }
        }
        let on_turnover_this_year = annual.status == Status::Turnover && annual.months.span.is_some();
        if on_turnover_this_year && let Some(deadline) = application(all, person, year) {
            found.push(deadline);
        }
        if on_turnover_this_year && let Some(due) = Total::of(&annual).due {
            found.push(Deadline::new(Kind::Annual(year), due));
        }
        // За год до регистрации ИП взноса нет: `tax::annual` смотрит
        // регистрацию только в самом году.
        let health = if annual.status == Status::NotYet { Health::NoRule } else { annual.health };
        match health {
            Health::Year { due, .. } => found.push(Deadline::new(Kind::HealthYear(year), due)),
            Health::Month { due_day, months, .. } => {
                found.extend(((13 - months)..=12).filter_map(|month| health_month(year, month, due_day)));
            }
            Health::Out(_) | Health::Unknown(_) | Health::Unclear(_) | Health::NoRule => {}
        }
        if person.is_some_and(|person| person.residency == Some(Residency::Citizen)) {
            found.extend(declaration(all, year));
        }
    }
    window(found, from, until)
}

/// Сроки всех ИП на налоге с оборота, которые с переносом кончаются с `from`
/// по `until` включительно, — по одним правилам, без профиля: общий
/// календарь на сайте (`tax::calendar`, задача 22). Каждый срок — как у ИП,
/// о котором ничего не известно: заявление — к общему дню (у нового ИП —
/// свой: день учёта и `after_days`), медстраховка — по правилу года, хоть
/// платят её не все,
/// декларация — хоть подают её только граждане. Кого срок касается,
/// говорят слова события, а точно — программа по профилю.
pub fn general(from: Date, until: Date) -> Vec<Deadline> {
    let all = rules::armenia();
    let mut found = Vec::new();
    let first = from.year().saturating_sub(1).max(Date::MIN_YEAR);
    for year in first..=until.year() {
        found.extend((1..=4).filter_map(|number| turnover(Quarter::new(year, number)?)));
        found.extend(application(all, None, year));
        // Срок годовых платежей — правило налога на прибыль, как у
        // `annual::Total`.
        if let Some(due) = rules::for_year(all.profit_tax(), year).and_then(|rule| rule.due.after(year)) {
            found.push(Deadline::new(Kind::Annual(year), due));
        }
        match rules::for_year(all.health_insurance(), year).map(|rule| rule.by) {
            Some(HealthBy::Year { due, .. }) => found.push(Deadline::new(Kind::HealthYear(year), due)),
            Some(HealthBy::Month { due_day }) => {
                found.extend((1..=12).filter_map(|month| health_month(year, month, due_day)));
            }
            None => {}
        }
        found.extend(declaration(all, year));
    }
    window(found, from, until)
}

/// Сроки, которые с переносом кончаются с `from` по `until` включительно, —
/// по дням, а в один день — по порядку [`Kind`].
fn window(mut found: Vec<Deadline>, from: Date, until: Date) -> Vec<Deadline> {
    found.retain(|deadline| from <= deadline.day && deadline.day <= until);
    found.sort_by_key(|deadline| (deadline.day, deadline.kind));
    found
}

/// Взнос медстраховки за месяц `month` года `year` — до числа `due_day`
/// следующего месяца (ст. 14, ч. 4).
fn health_month(year: u16, month: u8, due_day: u8) -> Option<Deadline> {
    let next = Date::new(year, month, 1)?.last_of_month().add_days(1)?;
    Some(Deadline::new(
        Kind::HealthMonth { year, month },
        Date::new(next.year(), next.month(), due_day)?,
    ))
}

/// Заявление о налоге с оборота на год `year` (НК, ст. 254, ч. 1) — у того,
/// кто в этом году на нём (зовущий смотрит `tax::annual`: не общий режим
/// из-за порога прошлого года):
///
/// - стал ИП в этом году — через `after_days` дней после дня учёта (п. 2);
///   день учёта — день регистрации из профиля, другого профиль не знает;
/// - не вёл деятельности с начала года по день срока, остановив её до
///   1 января, — через `after_days` дней после дня возобновления (п. 5),
///   а пока не возобновил — срока нет. Остановил позже — январь он
///   работал, и за него нужно заявление п. 1: «не вёл деятельности по
///   20 февраля» читается и так, и шире, и срок берётся ранний
///   (ревью задачи 21). Разъяснения ՊԵԿ к п. 5 разведка задачи 74 не
///   нашла, и ранний остаётся (решение агента 2026-10-27, бухгалтера пока
///   нет): заявление, поданное раньше, ничего не стоит, а опоздавшее
///   стоило бы режима;
/// - остальные — день `due` (20 февраля, п. 1).
///
/// «Каждый ли год» закон прямо не говорит: «до 20 февраля данного года»
/// читается и как «каждый год», и как «заявление, поданное до этого дня,
/// в том числе в прошлом году». Напоминать каждый год безопасно при обоих
/// прочтениях (FEATURE_TASKS.md, «Заявление о налоге с оборота»; ответ
/// 2026-10-19, вопрос 6.1; разведка задачи 21, 2026-10-20).
fn application(all: &Rules, person: Option<&Person>, year: u16) -> Option<Deadline> {
    let start = Date::new(year, 1, 1)?;
    let rule = rules::on(all.turnover_application(), start)?;
    let due = rule.due.of(year)?;
    let after = i32::from(rule.after_days);
    let registered = person.and_then(|person| person.registered);
    if let Some(day) = registered.filter(|day| day.year() == year) {
        return Some(Deadline::new(Kind::Application(year), day.add_days(after)?));
    }
    let stopped = person.and_then(|person| {
        person
            .stops
            .iter()
            .find(|stop| stop.from() <= start && stop.until().is_none_or(|until| until > due))
    });
    let by_law = match stopped {
        Some(stop) => stop.until().filter(|resumed| resumed.year() == year)?.add_days(after)?,
        None => due,
    };
    Some(Deadline::new(Kind::Application(year), by_law))
}

/// Всеобщая декларация о доходах за год `year`. Подают её граждане Армении
/// — резиденты (НК, ст. 156, ч. 4.1), а ИП на налоге с оборота — резидент
/// (ст. 254, ч. 1): [`between`] берёт её только у гражданина. Гражданство
/// профиль знает нынешнее, а закон смотрит на 31 декабря отчётного года:
/// сменившему его посреди года программа ошибётся годом.
fn declaration(all: &Rules, year: u16) -> Option<Deadline> {
    let rule = rules::for_year(all.income_declaration(), year)?;
    Some(Deadline::new(Kind::Declaration(year), rule.due.after(year)?))
}

/// Срок прошлого квартала, если он прошёл, а квартал не закрыт: расчёт,
/// по всей видимости, не подан. Только прошлый квартал — тот, что карточка
/// налога показывает «к уплате» (`ui/tax.rs`), пока он не закрыт: давно
/// прошедшие незакрытые кварталы — забота экрана налога, а не напоминания.
///
/// Квартал ухода с налога с оборота — не просрочен: программа его не
/// закрывает (`QuarterTax::closable`), он — забота бухгалтера, и напоминание
/// «закройте квартал» не ушло бы никогда.
pub fn overdue(books: Books<'_>, today: Date) -> Option<Deadline> {
    let quarter = today.quarter().previous()?;
    let annual = annual::year(books, quarter.year());
    if annual.months.left.is_some_and(|(from, _)| from <= quarter.last_day()) {
        return None;
    }
    let person = books.profile.map(|profile| &profile.person);
    filed(books, &annual, person, quarter).filter(|deadline| !deadline.done && deadline.day < today)
}

/// Срок квартала `quarter` у этого ИП — для карточки и экрана налога:
/// `None` — расчёта за квартал он не подаёт ([`files`]) или правила нет.
pub fn quarter_deadline(books: Books<'_>, quarter: Quarter) -> Option<Deadline> {
    let annual = annual::year(books, quarter.year());
    let person = books.profile.map(|profile| &profile.person);
    filed(books, &annual, person, quarter)
}

/// Срок квартала, если ИП подаёт за него расчёт, — сделанным, если квартал
/// закрыт.
fn filed(books: Books<'_>, annual: &annual::Annual, person: Option<&Person>, quarter: Quarter) -> Option<Deadline> {
    if !files(annual.months.span, person, quarter) {
        return None;
    }
    Some(Deadline {
        done: Closing::current(books.closings, quarter).is_some(),
        ..turnover(quarter)?
    })
}

/// Чего не хватает профилю, чтобы список сроков был полон: «нет значения»
/// — не «срока нет» (Правило 6).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct Gaps {
    /// Нет дня регистрации: у нового ИП заявление — через 20 дней после
    /// учёта, и этого срока список не знает.
    pub registered: bool,
    /// Нет гражданства и права проживания: не узнать ни взноса
    /// медстраховки, ни декларации.
    pub residency: bool,
    /// Взнос медстраховки года из окна не известен или под вопросом
    /// (`annual::Health::Unknown`, `Unclear`): что с ним, говорит экран
    /// годовых платежей.
    pub health: bool,
}

/// Может ли срок взноса медстраховки года `year` попасть между `from`
/// и `until`: за год одной суммой — его день, помесячно — от числа
/// февраля (за январь) до числа января следующего года (за декабрь).
fn health_due_between(year: u16, from: Date, until: Date) -> bool {
    let Some(rule) = rules::for_year(rules::armenia().health_insurance(), year) else {
        return false;
    };
    let (first, last) = match rule.by {
        HealthBy::Year { due, .. } => (Some(due), Some(due)),
        HealthBy::Month { due_day } => (
            Date::new(year, 2, due_day),
            year.checked_add(1).and_then(|next| Date::new(next, 1, due_day)),
        ),
    };
    match (first.map(last_day), last.map(last_day)) {
        (Some(first), Some(last)) => first <= until && from <= last,
        _ => false,
    }
}

/// Чего не хватает для сроков с `from` по `until` — годы те же, что
/// у [`between`].
pub fn gaps(books: Books<'_>, from: Date, until: Date) -> Gaps {
    let person = books.profile.map(|profile| &profile.person);
    let residency = person.is_none_or(|person| person.residency.is_none());
    let first = from.year().saturating_sub(1).max(Date::MIN_YEAR);
    // Без гражданства медстраховка не известна всегда — это уже сказано.
    // Год считается, только если его срок может попасть в окно.
    let health = !residency
        && (first..=until.year()).any(|year| {
            health_due_between(year, from, until) && {
                let annual = annual::year(books, year);
                annual.status != Status::NotYet && matches!(annual.health, Health::Unknown(_) | Health::Unclear(_))
            }
        });
    Gaps {
        registered: person.is_none_or(|person| person.registered.is_none()),
        residency,
        health,
    }
}

/// Напоминания в день `today`: незакрытый прошлый квартал, срок которого
/// прошёл, и несделанные сроки ближайших [`REMIND_DAYS`] дней: о закрытом
/// квартале напоминать нечего.
pub fn reminders(books: Books<'_>, today: Date) -> Vec<Reminder> {
    let mut found: Vec<Reminder> = overdue(books, today)
        .map(|deadline| Reminder {
            deadline,
            when: When::Overdue,
        })
        .into_iter()
        .collect();
    let Some(until) = today.add_days(REMIND_DAYS) else {
        return found;
    };
    for deadline in between(books, today, until).into_iter().filter(|deadline| !deadline.done) {
        let days = deadline.day.days_since(today);
        let when = match days {
            0 => When::Today,
            1 => When::Tomorrow,
            days => When::In(days),
        };
        found.push(Reminder { deadline, when });
    }
    found
}

#[cfg(test)]
mod tests;
