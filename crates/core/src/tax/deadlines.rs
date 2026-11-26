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
//! # Что сделано, программа знает о квартале и о заявлении нового ИП
//!
//! Подан ли расчёт квартала, говорит его закрытие (`tax::closing`): прошёл
//! срок, а квартал не закрыт — расчёт, по всей видимости, не подан
//! ([`overdue`]). Заявление нового ИП отмечает сам человек — днём подачи
//! в профиле (`Person::turnover_applied`, полный прогон 2026-11-10, п. 7):
//! пропущенное стоит режима, и о нём окно напоминает с дня регистрации, а
//! после срока говорит, чем это грозит ([`applying`]). Об уплате, заявлениях
//! следующих лет и годовых платежах программе никто не сообщает: у них
//! напоминание до дня срока, а после — следующий срок.
//!
//! # Сроки этого ИП и сроки всех
//!
//! [`between`] — сроки по профилю и книге: экран сроков и личный календарь
//! (`tax::calendar`). [`general`] — сроки по одним правилам, без профиля:
//! общий календарь на сайте (задача 22). Дни у обоих — одни и те же функции
//! от правил, а не второй расчёт.

use super::annual::{self, Health, Status, Total};
use super::closing::Closing;
use super::rules::{self, HealthBy, Rule, Rules, Source};
use super::turnover::{Books, Leaving};
use crate::i18n::{self, Key, Lang, fill, t};
use crate::model::date::{Date, Quarter};
use crate::model::profile::{Person, Residency};

/// За сколько дней до срока напоминать: модуль «Календарь» из описания
/// продукта — напоминание за 7 дней и за день.
pub const REMIND_DAYS: i32 = 7;

/// На сколько дней день с безопасной стороны ([`safe`]) раньше самого
/// срока: 20 января против 20 февраля — 31 день. Напоминания берут сроки
/// на столько дальше недели ([`reminders`]).
const SAFE_AHEAD_DAYS: i32 = 31;

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
    /// Подать заявление о микропредпринимательстве на год (задача 100).
    MicroApplication(u16),
    /// Подать отчёт микропредприятия об обороте за год (задача 100).
    MicroReport(u16),
    /// Заплатить налог на прибыль, соцплатёж и гербовый сбор за год.
    Annual(u16),
    /// Заплатить гербовый сбор за год микропредпринимательства: налога на
    /// прибыль у микропредприятия нет, а соцплатёж самозанятого — в срок
    /// подоходного налога (`tax::annual::Social::SelfEmployed`), задача 100.
    MicroAnnual(u16),
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
    /// следующий рабочий. У первого взноса помесячной медстраховки — сам
    /// `by_law`, без переноса: у него есть день с безопасной стороны
    /// ([`safe`]).
    pub day: Date,
    /// Сделано ли: квартал закрыт — расчёт подан (`tax::closing`); заявление
    /// нового ИП отмечено днём подачи (`Person::turnover_applied`). О других
    /// сроках программе никто не сообщает, и у них — `false`.
    pub done: bool,
}

impl Deadline {
    fn new(kind: Kind, by_law: Date) -> Self {
        Self {
            kind,
            by_law,
            day: if safe(kind).is_some() { by_law } else { last_day(by_law) },
            done: false,
        }
    }
}

/// День с безопасной стороны у первого взноса помесячной медстраховки —
/// за январь года, с 1 января которого взнос помесячный (`health_insurance`
/// с `HealthBy::Month`; с 2027 года — ՀՕ-459-Ն, ст. 45, ч. 1, п. 6): то же
/// число самого января. Закон велит платить до числа каждого месяца после
/// месяца, в котором ИП стал плательщиком (ст. 14, ч. 4), а за какой месяц
/// каждый платёж, не говорит; ՊԵԿ об этом спрошен (вопрос 16 письма задачи
/// 82). Поэтому срок такого взноса — день по прочтению программы («за
/// месяц — до числа следующего»), без переноса: перенос уплаты ՊԵԿ не
/// подтверждал, — а заплатить надёжнее в январе, до этого дня ([`moved`]),
/// и напоминание до него приходит к нему ([`remind_day`], календарь —
/// своим событием). Сайт, программа и календарь говорят одно (решения
/// человека 2026-11-26). `None` — у остальных сроков: у всех платежей
/// после первого прочтения сходятся.
pub fn safe(kind: Kind) -> Option<Date> {
    let Kind::HealthMonth { year, month: 1 } = kind else {
        return None;
    };
    let rule = rules::for_year(rules::armenia().health_insurance(), year)?;
    let HealthBy::Month { due_day } = rule.by else {
        return None;
    };
    let january = Date::new(year, 1, due_day)?;
    (rule.starts == Date::new(year, 1, 1)?).then_some(january)
}

/// День, к которому считается, насколько близок срок `deadline` в день
/// `today`: до дня с безопасной стороны ([`safe`]) — он, потом — сам срок.
/// Иначе напоминание о первом взносе пришло бы за неделю до 20 февраля,
/// когда «надёжнее в январе» уже прошло.
pub fn remind_day(deadline: &Deadline, today: Date) -> Date {
    match safe(deadline.kind) {
        Some(safe) if today <= safe => safe,
        _ => deadline.day,
    }
}

/// Что надо сделать к сроку — словами: экран сроков, напоминание и календарь
/// (`tax::calendar`) говорят одно.
pub fn what(lang: Lang, kind: Kind) -> String {
    match kind {
        Kind::Turnover(quarter) => fill(t(lang, Key::DeadlineTurnover), &[&i18n::quarter(lang, quarter)]),
        Kind::Application(year) => fill(t(lang, Key::DeadlineApplication), &[&year.to_string()]),
        Kind::MicroApplication(year) => fill(t(lang, Key::DeadlineMicroApplication), &[&year.to_string()]),
        Kind::MicroReport(year) => fill(t(lang, Key::DeadlineMicroReport), &[&year.to_string()]),
        Kind::Annual(year) => fill(t(lang, Key::DeadlineAnnual), &[&year.to_string()]),
        Kind::MicroAnnual(year) => fill(t(lang, Key::DeadlineMicroAnnual), &[&year.to_string()]),
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

/// Откуда день срока `kind` по закону (задача 126): источники тех записей
/// файла правил, из которых его берут функции ниже, — `turnover_due` по
/// первому дню месяца после квартала, заявления — по 1 января года,
/// медстраховка и декларация — по записи года. Годовые платежи — три
/// платежа, и источника три: налог на прибыль (его день берёт [`general`]
/// и `annual::Total`; НК, ст. 136, ч. 4, — только о нём), соцплатёж
/// и гербовый сбор года — у каждого свой закон и свой срок. Гербовый сбор
/// микропредприятия — его шкала года. Записи и дни сверки — данные, а не
/// код: второго списка статей здесь нет, и он не разойдётся с файлом. Пусто —
/// правила нет: у такого срока нет и дня.
///
/// Перенос с нерабочего дня — другой источник: [`moved_sources`].
pub fn sources(kind: Kind) -> Vec<&'static Source> {
    let all = rules::armenia();
    let january = |year: u16| Date::new(year, 1, 1);
    let found = match kind {
        Kind::Turnover(quarter) => {
            vec![quarter.next().and_then(|next| rules::on(all.turnover_due(), next.first_day())).map(Rule::source)]
        }
        Kind::Application(year) => {
            vec![january(year).and_then(|day| rules::on(all.turnover_application(), day)).map(Rule::source)]
        }
        Kind::MicroApplication(year) => {
            vec![january(year).and_then(|day| rules::on(all.micro_application(), day)).map(Rule::source)]
        }
        Kind::MicroReport(year) => vec![rules::for_year(all.micro_report(), year).map(Rule::source)],
        // Без налога на прибыль года нет и дня срока: остальным двум
        // источникам не к чему стоять.
        Kind::Annual(year) => match rules::for_year(all.profit_tax(), year) {
            Some(profit) => vec![
                Some(profit.source()),
                rules::for_year(all.social_payment(), year).map(Rule::source),
                rules::for_year(all.stamp_duty(), year).map(Rule::source),
            ],
            None => Vec::new(),
        },
        Kind::MicroAnnual(year) => vec![rules::for_year(all.stamp_duty(), year).map(Rule::source)],
        Kind::HealthYear(year) | Kind::HealthMonth { year, .. } => {
            vec![rules::for_year(all.health_insurance(), year).map(Rule::source)]
        }
        Kind::Declaration(year) => vec![rules::for_year(all.income_declaration(), year).map(Rule::source)],
    };
    found.into_iter().flatten().collect()
}

/// Откуда перенос срока, который по закону кончается `by_law` (задача 126):
/// запись переноса (`deadline_shift`: ՀՕ-180-Ն, ст. 32, ч. 4, и выходные
/// дни недели), а если нерабочими были праздник или день решения
/// правительства — и их записи, по разу, в этом порядке. Пусто — день
/// рабочий, переноса нет. Дни те же, что у [`last_day`].
pub fn moved_sources(by_law: Date) -> Vec<&'static Source> {
    let all = rules::armenia();
    let last = last_day(by_law);
    let mut found: Vec<&'static Source> = Vec::new();
    let mut add = |source: &'static Source| {
        if !found.iter().any(|known| std::ptr::eq(*known, source)) {
            found.push(source);
        }
    };
    if last == by_law {
        return found;
    }
    if let Some(shift) = rules::on(all.deadline_shift(), by_law) {
        add(shift.source());
    }
    let mut day = Some(by_law);
    while let Some(off) = day.filter(|day| *day < last) {
        if let Some(holidays) = rules::on(all.holidays(), off).filter(|holidays| holidays.contains(off)) {
            add(holidays.source());
        }
        for decision in all.days_off().iter().filter(|decision| decision.period().contains(off)) {
            if decision.off.contains(&off) {
                add(decision.source());
            }
        }
        day = off.add_days(1);
    }
    found
}

/// Платят ли к сроку: перенос дня уплаты ՊԵԿ не подтверждал («Перенос»
/// в шапке). К сроку квартала и подают расчёт, и платят.
const fn pays(kind: Kind) -> bool {
    match kind {
        Kind::Turnover(_) | Kind::Annual(_) | Kind::MicroAnnual(_) | Kind::HealthYear(_) | Kind::HealthMonth { .. } => {
            true
        }
        Kind::Application(_) | Kind::MicroApplication(_) | Kind::MicroReport(_) | Kind::Declaration(_) => false,
    }
}

/// Слова под днём срока `kind`, который по закону кончается `by_law`:
/// с какого дня он перенесён, а если к сроку платят, — что заплатить
/// надёжнее до этого дня; у первого взноса помесячной медстраховки — что
/// заплатить надёжнее в январе, до дня с безопасной стороны ([`safe`]).
/// `None` — день рабочий, переноса нет. Слова одни у экрана сроков,
/// напоминания, карточек и календаря.
pub fn moved(lang: Lang, kind: Kind, by_law: Date) -> Option<String> {
    if let Some(safe) = safe(kind) {
        return Some(fill(t(lang, Key::DeadlineSafe), &[&i18n::date(lang, safe)]));
    }
    let key = if pays(kind) { Key::DeadlineShiftedPay } else { Key::DeadlineShifted };
    (last_day(by_law) != by_law).then(|| fill(t(lang, key), &[&i18n::date(lang, by_law)]))
}

/// Строки под днём срока `kind` (по закону — `by_law`) в день `today` —
/// у экрана сроков, напоминания, календаря и сайта. У первого взноса
/// помесячной медстраховки ([`safe`]): до дня с безопасной стороны —
/// «надёжнее заплатить в январе, до …», после него — что заплатившему
/// в январе второй раз платить не нужно (`today` — `None` у календаря
/// и сайта: дня у них нет, и строки обе), и всегда — заплатившему годовую
/// ставку прошлого года уточнить у бухгалтера, нужен ли ему взнос за январь
/// ([`safe_annual`]). Решения человека 2026-11-26 (ревью 0.0.209). У
/// остальных — слова переноса ([`moved`]).
pub fn notes(lang: Lang, kind: Kind, by_law: Date, today: Option<Date>) -> Vec<String> {
    let Some(safe) = safe(kind) else {
        return moved(lang, kind, by_law).into_iter().collect();
    };
    let mut lines = Vec::new();
    if today.is_none_or(|today| today <= safe) {
        lines.push(fill(t(lang, Key::DeadlineSafe), &[&i18n::date(lang, safe)]));
    }
    if today.is_none_or(|today| today > safe) {
        lines.push(t(lang, Key::DeadlineSafePaid).to_owned());
    }
    lines.extend(safe_annual(lang, kind));
    lines
}

/// Строка под первым взносом помесячной медстраховки ([`safe`]) для того,
/// кто заплатил годовую ставку прошлого года: застрахован он 12 месяцев
/// с месяца после оплаты (ՀՕ-459-Ն, ст. 46, ч. 12), а нужен ли ему взнос
/// за январь, закон прямо не говорит, — уточнить у бухгалтера. Знать,
/// заплатил ли человек, программе не из чего, и строка — условием в самих
/// словах, как оговорка экрана годовых платежей. `None` — не первый взнос
/// или прошлый год не годовой ставкой.
pub fn safe_annual(lang: Lang, kind: Kind) -> Option<String> {
    let Kind::HealthMonth { year, .. } = kind else {
        return None;
    };
    safe(kind)?;
    let before = year.checked_sub(1)?;
    let rule = rules::for_year(rules::armenia().health_insurance(), before)?;
    matches!(rule.by, HealthBy::Year { .. }).then(|| fill(t(lang, Key::DeadlineSafeAnnual), &[&before.to_string()]))
}

/// Строка под списком сроков, в котором есть первый взнос помесячной
/// медстраховки ([`safe`]): его день указан без переноса с нерабочего дня
/// — решение программы, а не закона: ՀՕ-180-Ն, ст. 32, ч. 4 мог бы его
/// перенести (правка человека 2026-11-26),
/// а заплатить раньше безопаснее, — рядом со словами о переносе, которые
/// говорят обратное (решение человека 2026-11-26). `None` — у остальных.
pub fn no_shift(lang: Lang, kind: Kind) -> Option<String> {
    let Kind::HealthMonth { year, .. } = kind else {
        return None;
    };
    safe(kind).map(|_| fill(t(lang, Key::DeadlinesSafeNoShift), &[&year.to_string()]))
}

/// Насколько близок срок в день `today`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum When {
    /// Прошёл, а дело не сделано (только квартал: [`overdue`]).
    Overdue,
    Today,
    Tomorrow,
    /// Через столько дней — от двух: в напоминаниях до [`REMIND_DAYS`],
    /// в сводке конторы (`tax::summary`) — сколько бы ни было.
    In(i32),
}

/// Напоминание: срок и насколько он близок.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Reminder {
    pub deadline: Deadline,
    pub when: When,
    /// День, к которому считается `when` ([`remind_day`]): срок, а у
    /// первого взноса помесячной медстраховки до дня с безопасной стороны —
    /// этот день. У просроченного квартала — его срок.
    pub day: Date,
}

/// Рабочий ли день `day` для срока по правилам `rules`: рабочий по решению
/// правительства — рабочий, хоть и выходной; нерабочий по решению,
/// праздник или выходной день недели — нет. Правил на этот день нет —
/// рабочий: переносить не по чему. Те же дни — у курса ЦБ «предыдущего
/// рабочего дня» ([`previous_working`]).
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

/// Рабочий день, предыдущий дню `day`, по правилам `rules`: курс ЦБ,
/// опубликованный в этот день, берёт закон для поставки дня `day`
/// (`tax::cba`, задача 43). Рабочие дни — те же, что у переноса сроков:
/// дни, в которые ЦБ публиковал курс с 2025-01-03 по 2026-11-16 (471),
/// совпали с ними все до одного (сверено 2026-11-16, задача 43). `None` —
/// рабочего дня не нашлось за [`MAX_SHIFT`] дней или календарь кончился.
pub(crate) fn previous_working(rules: &Rules, day: Date) -> Option<Date> {
    let mut before = day;
    for _ in 0..MAX_SHIFT {
        before = before.add_days(-1)?;
        if working(rules, before) {
            return Some(before);
        }
    }
    None
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
            found.push(Deadline {
                done: applied(person, year),
                ..deadline
            });
        }
        if on_turnover_this_year && let Some(due) = Total::of(&annual).due {
            found.push(Deadline::new(Kind::Annual(year), due));
        }
        // Год микропредпринимательства (задача 100): заявление на него, отчёт
        // об обороте за него и гербовый сбор. Ушедшему с режима посреди года
        // сроки этого года считает бухгалтер (`Status::General`).
        if annual.status == Status::Micro {
            found.extend(micro_application(all, person, year));
            found.extend(micro_report(all, year));
            if let Some(due) = annual::micro_due(&annual) {
                found.push(Deadline::new(Kind::MicroAnnual(year), due));
            }
        }
        // Перешедший порог микропредпринимательства — плательщик налога
        // с оборота со дня превышения, если в 20 дней подаст о нём заявление,
        // иначе НДС (НК, ст. 254, ч. 1, п. 3; ст. 59, ч. 1, п. 3). Срок — по
        // записи `turnover_application`; налоги после ухода считает бухгалтер.
        // Ушедшему по коду (ст. 267, ч. 5, п. 2.1) налог с оборота закрыт тем
        // же кодом (ст. 254, ч. 3, п. 3.1), и заявления нет.
        if let Some((left, Leaving::MicroThreshold)) = annual.months.left
            && let Some(rule) = rules::on(all.turnover_application(), left)
            && let Some(day) = left.add_days(i32::from(rule.after_days))
        {
            found.push(Deadline::new(Kind::Application(year), day));
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
    applying_by(all.turnover_application(), Kind::Application(year), person, year)
}

/// Заявление о микропредпринимательстве на год `year` (задача 100): те же
/// дни, что у [`application`], по своей статье — НК, ст. 267, ч. 1, п. 1–3
/// (по закону, сверено по тексту arlis 230455 2026-11-14).
fn micro_application(all: &Rules, person: Option<&Person>, year: u16) -> Option<Deadline> {
    applying_by(all.micro_application(), Kind::MicroApplication(year), person, year)
}

/// Отчёт микропредприятия об обороте за год `year` — до дня следующего года
/// по записи `micro_report` (НК, ст. 270, ч. 1). Кто микропредприятие только
/// по видам приложения 3 к кодексу, его не подаёт (там же), а программе это
/// знать не из чего: срок показывается всем, а слова экрана налога говорят
/// об исключении.
fn micro_report(all: &Rules, year: u16) -> Option<Deadline> {
    let rule = rules::for_year(all.micro_report(), year)?;
    Some(Deadline::new(Kind::MicroReport(year), rule.due.after(year)?))
}

/// Заявление вида `kind` на год `year` по записям `records` — общий ход
/// у [`application`] и [`micro_application`].
fn applying_by(
    records: &[super::rules::TurnoverApplication],
    kind: Kind,
    person: Option<&Person>,
    year: u16,
) -> Option<Deadline> {
    let start = Date::new(year, 1, 1)?;
    let rule = rules::on(records, start)?;
    let due = rule.due.of(year)?;
    let after = i32::from(rule.after_days);
    let registered = person.and_then(|person| person.registered);
    if let Some(day) = registered.filter(|day| day.year() == year) {
        return Some(Deadline::new(kind, day.add_days(after)?));
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
    Some(Deadline::new(kind, by_law))
}

/// Последний день заявления нового ИП, учтённого в день `registered`, —
/// с переносом, как у [`application`]: форме профиля — спрашивать ли день
/// подачи. `None` — правила нет.
pub fn new_ie_due(registered: Date) -> Option<Date> {
    let rule = rules::on(rules::armenia().turnover_application(), Date::new(registered.year(), 1, 1)?)?;
    Some(last_day(registered.add_days(i32::from(rule.after_days))?))
}

/// Заявление на год `year` — заявление нового ИП: он зарегистрирован в этом
/// году, и срок у него свой, от дня учёта (НК, ст. 254, ч. 1, п. 2).
fn new_ie(person: Option<&Person>, year: u16) -> bool {
    person
        .and_then(|person| person.registered)
        .is_some_and(|day| day.year() == year)
}

/// Отмечено ли заявление на год `year` поданным: отметка — только у
/// заявления нового ИП ([`Person::turnover_applied`]).
fn applied(person: Option<&Person>, year: u16) -> bool {
    new_ie(person, year) && person.is_some_and(|person| person.turnover_applied.is_some())
}

/// Заявление нового ИП о налоге с оборота — своей карточкой на главном
/// экране (полный прогон 2026-11-10, п. 7; слова — решение человека
/// 2026-11-10).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Applying {
    /// Срок не прошёл: подать до дня срока.
    Due(Deadline),
    /// Срок прошёл, а дня подачи нет.
    Missed(Deadline),
}

/// Что сказать о заявлении нового ИП в день `today` — с дня регистрации, а не
/// за [`REMIND_DAYS`]: всё окно нового ИП — 20 дней (НК, ст. 254, ч. 1, п. 2),
/// а пропущенное стоит режима — без заявления он с дня учёта до конца года
/// плательщик НДС (ст. 59, ч. 1, п. 2; по закону, редакция arlis 230455,
/// сверено 2026-11-10). Поэтому после срока окно не молчит, как у других
/// сроков, а говорит это — до конца года регистрации, пока человек не
/// отметит день подачи. Год, а не «пока не отметит»: НДС — до конца этого
/// года, а на следующий заявление подают к общему дню (п. 1) — о нём обычное
/// напоминание. Учтённый в последние 20 дней года, чей срок кончился уже
/// в следующем, о пропуске так и не услышит: слова «с дня регистрации»
/// говорили бы тогда о прошлом годе. `None` — сказать нечего: дня регистрации
/// нет, он позже `today`, в год регистрации ИП не на налоге с оборота (как
/// у [`between`]), день подачи отмечен или год регистрации кончился.
///
/// Закон ставит НДС в вину двум неподанным заявлениям сразу — о налоге
/// с оборота и о микропредприятии (59(1)(2)). Программа — для налога
/// с оборота, и её человек второго не подаёт: слова говорят о первом.
pub fn applying(books: Books<'_>, today: Date) -> Option<Applying> {
    let person = &books.profile?.person;
    let registered = person.registered.filter(|day| *day <= today)?;
    if person.turnover_applied.is_some() {
        return None;
    }
    let year = registered.year();
    let annual = annual::year(books, year);
    if annual.status != Status::Turnover || annual.months.span.is_none() {
        return None;
    }
    let deadline = application(rules::armenia(), Some(person), year)?;
    if today <= deadline.day {
        Some(Applying::Due(deadline))
    } else {
        (today.year() == year).then_some(Applying::Missed(deadline))
    }
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
/// квартале напоминать нечего. Заявления нового ИП здесь нет: о нём — своя
/// карточка с дня регистрации ([`applying`]), и второй строкой о том же она
/// не нужна.
pub fn reminders(books: Books<'_>, today: Date) -> Vec<Reminder> {
    let person = books.profile.map(|profile| &profile.person);
    let mut found: Vec<Reminder> = overdue(books, today)
        .map(|deadline| Reminder {
            deadline,
            when: When::Overdue,
            day: deadline.day,
        })
        .into_iter()
        .collect();
    let Some(until) = today.add_days(REMIND_DAYS) else {
        return found;
    };
    // День с безопасной стороны — за месяц до срока ([`safe`]): срок, чей
    // день ещё за неделей, напоминает уже им, и сроки берутся на месяц
    // дальше недели.
    let Some(reach) = until.add_days(SAFE_AHEAD_DAYS) else {
        return found;
    };
    // Заявление нового ИП — своей карточкой ([`applying`]), когда год его
    // регистрации на налоге с оборота; заявление после ухода
    // с микропредпринимательства (задача 100) карточки нет — оно здесь.
    let own_card = |kind: Kind| match kind {
        Kind::Application(year) if new_ie(person, year) => {
            let annual = annual::year(books, year);
            annual.status == Status::Turnover && annual.months.span.is_some()
        }
        _ => false,
    };
    for deadline in between(books, today, reach)
        .into_iter()
        .filter(|deadline| !deadline.done && !own_card(deadline.kind))
    {
        let day = remind_day(&deadline, today);
        if day > until {
            continue;
        }
        let when = match day.days_since(today) {
            0 => When::Today,
            1 => When::Tomorrow,
            days => When::In(days),
        };
        found.push(Reminder { deadline, when, day });
    }
    found
}

#[cfg(test)]
mod tests;
