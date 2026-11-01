//! Годовые платежи ИП на налоге с оборота (задача 20): налог на прибыль,
//! соцплатёж, гербовый сбор и медстраховка — сколько, за что и до какого
//! дня, и сколько откладывать в месяц, чтобы к сроку набралась вся сумма.
//!
//! Всё ниже — прочтение агента по тексту законов (разведка задачи 20,
//! сверено по скачанным текстам arlis 2026-10-20), а где закон молчит —
//! ответы бухгалтера-агента 2026-10-19 (FEATURE_TASKS.md, «PDF против
//! текста закона»): по решению человека 2026-10-19 оно идёт в код словами
//! «сверьте с бухгалтером», пока расчёт не сверят настоящие бухгалтеры
//! (задача 82). Суммы, ступени и сроки — записи файла правил (`tax::rules`)
//! с источником, а не константы здесь.
//!
//! # Месяцы
//!
//! Налог на прибыль ИП на налоге с оборота — 5 000 ֏ за каждый месяц,
//! окончательно и при любом обороте, даже нулевом (НК, ст. 125, ч. 3),
//! одной суммой за год до 20 апреля следующего года, без авансов (ст. 136,
//! ч. 4; ст. 263, ч. 2, п. 1). Соцплатёж — тоже 5 000 ֏ в месяц
//! окончательно (закон о накопительных пенсиях ՀՕ-244-Ն, ст. 6, ч. 2, п. 3;
//! ч. 5) и в тот же срок (ст. 7, ч. 4 → НК, ст. 136, ч. 4).
//!
//! Месяцы года ([`Months`]) — с месяца регистрации ИП, или с января, по
//! месяц ухода с налога с оборота, или по декабрь. Неполный крайний месяц
//! закон не решает; по ответу бухгалтера-агента он платится целиком
//! (вопросы 3.1 и 3.8; практика). Месяц, в котором нет ни дня налога
//! с оборота, — нет: доход по закрывающему коду 1-го числа выводит из
//! режима с самого этого дня. Налог на прибыль не платится за целые
//! календарные месяцы внутри остановки деятельности — от дня остановки до
//! дня возобновления, который в остановку уже не входит (ст. 125, ч. 3,
//! п. 1 и 2). У соцплатежа нормы об остановке нет, но в расчёте ՊԵԿ месяцы
//! остановки не заполняют (вопрос 3.3; практика) — он считается за те же
//! месяцы.
//!
//! Остановка освобождает с дня в заявлении, но не раньше дня его подачи,
//! а день подачи профиль не хранит: окно говорит это словами. Доход
//! в месяце остановки окно называет: начатая без заявления работа
//! возобновляет деятельность с этого дня (НК, ст. 53, ч. 8), но переносить
//! это на налог на прибыль — толкование.
//!
//! # Гербовый сбор и медстраховка
//!
//! Гербовый сбор — ступень шкалы по обороту того же года, отражённому
//! в расчётах налога с оборота ([`declared`]; закон ՀՕ-245-Ն, ст. 9,
//! ч. 1, п. 3 и 5; ст. 12, ч. 1, п. 2), без пропорции за неполный год.
//! Взнос медстраховки (закон ՀՕ-459-Ն) за 2026 год — годовая ставка одной
//! суммой у ИП с оборотом 2025 года больше порога, а с 2027 года —
//! помесячно у всех ([`Health`]).
//!
//! # Чего программа не знает
//!
//! «Нет значения» — не ноль (Правило 6): без года рождения, ответа
//! о накопительной пенсии, статуса проживания и дня регистрации платёж или
//! итог не известны ([`Missing`], [`Total::complete`]), а не «не платится»;
//! ушедшему с налога с оборота месяцы общего режима считает бухгалтер
//! ([`Months::left`]), и итог года тогда не полон.

use std::collections::BTreeMap;

use super::closing::Closing;
use super::deadlines;
use super::rules::{self, FundedPension, HealthBy, HealthInsurance, Monthly, Rule, Version};
use super::turnover::{self, Books, Leaving, Regime, Span};
use crate::model::activity::IncomeKind;
use crate::model::date::{Date, Quarter};
use crate::model::ledger::Flow;
use crate::model::money::{Lumas, MoneyError};
use crate::model::profile::{Person, ProfileData, Residency, Stop};

/// Правило накопительной пенсии на день `on`; `None` — правила на этот
/// день нет.
///
/// Правила — `funded_pension` файла правил. Сейчас это закон
/// о накопительных пенсиях, ст. 5, ч. 1: соцплатёж делают «1974 թվականի
/// հունվարի 1-ին և դրանից հետո ծնված … անհատ ձեռնարկատերերը» (п. 1), а
/// родившиеся «մինչև 1974 թվականը» — если подали заявление о выборе фонда
/// (п. 2); решение не пересматривается (ч. 2). Иностранец и лицо без
/// гражданства с правом проживания (статусом проживания) платят на общих
/// основаниях, если международный договор не говорит иное (ч. 3).
///
/// Текст ч. 1–3 — с 2014-07-01 (ՀՕ-67-Ն от 21.06.2014; поправки после него
/// правили только ч. 5–11), но до 2018-07-01 тот, кто уже был ИП, мог
/// отказаться от платежа заявлением (ст. 81, ч. 1 и 3; срок — ՀՕ-76-Ն от
/// 12.05.2016). Поэтому правило без исключений и в файле — с 2018-07-01
/// (по закону; сверено по тексту arlis 224823, 91047, 106108 и поправок
/// 2017–2024 годов 2026-10-19, разведка задачи 18).
pub fn funded_pension(on: Date) -> Option<&'static FundedPension> {
    rules::armenia()
        .funded_pension()
        .iter()
        .find(|rule| rule.period().contains(on))
}

/// Решает ли человек сам, участвовать ли в накопительной пенсии, — по
/// правилу на день `on`: родился раньше границы. Граница — 1 января (это
/// держит разбор файла правил), поэтому хватает года рождения. `false` —
/// участие обязательно или правила на этот день нет: спрашивать не о чем.
pub fn pension_is_a_choice(birth_year: u16, on: Date) -> bool {
    funded_pension(on).is_some_and(|rule| birth_year < rule.mandatory_born_from.year())
}

/// Месяцы года, за которые ИП на налоге с оборота платит налог на прибыль
/// и соцплатёж (шапка модуля).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Months {
    /// Первый и последний месяц года на налоге с оборота, включительно.
    /// `None` — таких месяцев нет: ИП зарегистрирован позже или весь год
    /// не на налоге с оборота.
    pub span: Option<(u8, u8)>,
    /// День регистрации ИП, если он в этом году: месяц регистрации
    /// платится целиком.
    pub registered: Option<Date>,
    /// День регистрации в профиле не указан: месяцы — с января, и итог года
    /// не полон.
    pub registration_unknown: bool,
    /// С какого дня до конца года ИП не на налоге с оборота и почему:
    /// месяцы после месяца ухода — общий режим, их считает бухгалтер.
    pub left: Option<(Date, Leaving)>,
    /// Целые месяцы остановки деятельности внутри `span` — их номера.
    pub stopped: Vec<u8>,
}

impl Months {
    /// Сколько месяцев платится.
    pub fn count(&self) -> u8 {
        let Some((first, last)) = self.span else {
            return 0;
        };
        // Целые месяцы остановки — внутри `span`: их не больше его длины.
        (last - first + 1).saturating_sub(self.stopped.len() as u8)
    }

    /// Ушёл ли ИП с налога с оборота посреди года: месяцы после ухода —
    /// общий режим, и итог года без них не полон.
    pub fn left_midyear(&self) -> bool {
        self.span.is_some() && self.left.is_some()
    }
}

/// Месяцы года `year` по профилю и режиму года (шапка модуля).
fn months(person: Option<&Person>, year: u16, regime: Regime) -> Months {
    let registered = person.and_then(|person| person.registered);
    let first = match registered {
        Some(day) if day.year() > year => None,
        Some(day) if day.year() == year => Some(day.month()),
        Some(_) | None => Some(1),
    };
    let (last, left) = match regime {
        Regime::Turnover => (Some(12), None),
        // Весь год — общий режим: оборот прошлого года больше порога.
        Regime::Left {
            from,
            why: Leaving::PreviousYear,
        } => (None, Some((from, Leaving::PreviousYear))),
        // День дохода по закрывающему коду — весь вне режима
        // (`tax::turnover`): ушёл 1-го числа — в этом месяце нет ни дня
        // налога с оборота.
        Regime::Left {
            from,
            why: why @ Leaving::Code(_),
        } if from.day() == 1 => (Some(from.month() - 1).filter(|month| *month >= 1), Some((from, why))),
        // Месяц ухода — целиком на налоге с оборота (шапка модуля).
        Regime::Left { from, why } => (Some(from.month()), Some((from, why))),
    };
    let span = first.zip(last).filter(|(first, last)| first <= last);
    let stops = person.map_or(&[][..], |person| person.stops.as_slice());
    let stopped = span
        .map(|(first, last)| {
            (first..=last)
                .filter(|month| stops.iter().any(|stop| covers(*stop, year, *month)))
                .collect()
        })
        .unwrap_or_default();
    Months {
        span,
        registered: registered.filter(|day| day.year() == year),
        registration_unknown: registered.is_none(),
        left,
        stopped,
    }
}

/// Целиком ли месяц внутри остановки: остановлена не позже его первого дня
/// и не возобновлена до его последнего (НК, ст. 125, ч. 3, п. 1 —
/// «ամբողջական ամիսների համար»). День возобновления в остановку не входит:
/// «մինչև … վերսկսելու օրը» — без «ներառյալ», которым кодекс отмечает
/// включительные сроки, и деятельность возобновлена «վերսկսելու օրվանից»
/// (ст. 53, ч. 8; разведка задачи 20).
fn covers(stop: Stop, year: u16, month: u8) -> bool {
    let Some(first) = Date::new(year, month, 1) else {
        return false;
    };
    let last = first.last_of_month();
    stop.from() <= first && stop.until().is_none_or(|until| until > last)
}

/// Фиксированный платёж за месяцы года — налог на прибыль.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Fixed {
    /// `months` месяцев по `monthly` — `amount`, до дня `due`.
    Due {
        monthly: i64,
        months: u8,
        amount: i64,
        due: Date,
    },
    /// Правила на весь год нет: его нет вовсе или оно сменилось посреди
    /// года.
    NoRule,
}

impl Fixed {
    /// Сумма за год, драмы; `None` — правила нет.
    pub fn amount(self) -> Option<i64> {
        match self {
            Self::Due { amount, .. } => Some(amount),
            Self::NoRule => None,
        }
    }
}

/// Платёж по записи `rule` за месяцы `months`.
fn fixed(rule: Option<&Monthly>, year: u16, months: &Months) -> Fixed {
    let Some((rule, due)) = rule.and_then(|rule| Some((rule, rule.due.after(year)?))) else {
        return Fixed::NoRule;
    };
    let count = months.count();
    Fixed::Due {
        monthly: rule.monthly,
        months: count,
        amount: rule.monthly.saturating_mul(i64::from(count)),
        due,
    }
}

/// Чего не хватает в профиле, чтобы знать платёж: окно просит заполнить
/// это (Правило 6).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct Missing {
    /// Год рождения.
    pub birth_year: bool,
    /// Ответ, вступил ли человек в накопительную пенсию сам: спрашивается
    /// у родившихся до границы обязательного участия.
    pub pension_joined: bool,
    /// Гражданство и право проживания.
    pub residency: bool,
}

impl Missing {
    /// Всего хватает.
    pub fn is_empty(self) -> bool {
        self == Self::default()
    }
}

/// Почему соцплатежа нет.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum SocialOut {
    /// Родился до границы обязательного участия и не вступал сам (закон
    /// о накопительных пенсиях, ст. 5, ч. 1, п. 2).
    NotJoined,
    /// Иностранец или лицо без гражданства без статуса проживания: платят
    /// только со статусом (ст. 5, ч. 3; закон о персонифицированном учёте
    /// ՀՕ-247-Ն на таких не распространяется — ст. 1, ч. 2). Что без статуса
    /// не платят — вывод.
    NoStatus,
}

/// Соцплатёж за год.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Social {
    /// Платит за месяцы года. `voluntary` — вступил сам по заявлению:
    /// платит с месяца после дня заявления (ст. 6, ч. 4, п. 4), а его месяц
    /// профиль не хранит — счёт с первого месяца года, и окно говорит это.
    Due { payment: Fixed, voluntary: bool },
    /// Не участвует.
    Out(SocialOut),
    /// Участвует ли, не известно: в профиле нет ответа.
    Unknown(Missing),
    /// Правила на весь год нет.
    NoRule,
}

impl Social {
    /// Сумма за год, драмы; `None` — не платит или не известно.
    pub fn amount(self) -> Option<i64> {
        match self {
            Self::Due { payment, .. } => payment.amount(),
            Self::Out(_) | Self::Unknown(_) | Self::NoRule => None,
        }
    }
}

/// Соцплатёж года `year`: кто участвует — по правилу накопительной пенсии
/// на весь год и профилю (шапка [`funded_pension`]). В пенсионном возрасте
/// платёж не прекращается сам — только по заявлению (ст. 5, ч. 4, п. 1),
/// и возраст здесь не смотрится.
fn social(person: Option<&Person>, year: u16, months: &Months) -> Social {
    let all = rules::armenia();
    let (Some(rule), Some(pension)) = (
        rules::for_year(all.social_payment(), year),
        rules::for_year(all.funded_pension(), year),
    ) else {
        return Social::NoRule;
    };
    let residency = person.and_then(|person| person.residency);
    if residency == Some(Residency::ForeignWithoutStatus) {
        return Social::Out(SocialOut::NoStatus);
    }
    let birth_year = person.and_then(|person| person.birth_year);
    let voluntary = birth_year.is_some_and(|year| year < pension.mandatory_born_from.year());
    let joined = person.and_then(|person| person.pension_joined);
    if voluntary && joined == Some(false) {
        return Social::Out(SocialOut::NotJoined);
    }
    let missing = Missing {
        birth_year: birth_year.is_none(),
        pension_joined: voluntary && joined.is_none(),
        residency: residency.is_none(),
    };
    if !missing.is_empty() {
        return Social::Unknown(missing);
    }
    match fixed(Some(rule), year, months) {
        Fixed::NoRule => Social::NoRule,
        payment => Social::Due { payment, voluntary },
    }
}

/// Оборот года по книге — весь доход года по дню оказания услуги, как его
/// считают квартальные расчёты налога с оборота: доход каждого квартала по
/// строкам налога, каждая строка — целыми драмами, лумы отброшены (приказ
/// ՊԵԿ 299-Ն, прил. 1, п. 1), и сумма за четыре квартала. Так сумма
/// сходится с расчётами до драма и у самой границы. Доход, строку которого
/// не узнать, — своей строкой.
///
/// Это база гербового сбора (закон ՀՕ-245-Ն, ст. 9, ч. 1, п. 5, «բ»), а у
/// ушедшего с налога с оборота — с доходом общего режима: база тогда —
/// сумма баз обоих режимов («ե»), и окно велит сверить её с бухгалтером.
/// И доход, по которому за 2026 год платят взнос медстраховки (закон
/// ՀՕ-459-Ն, ст. 45, ч. 1, п. 5): там поданное не уменьшается уточнением, и
/// у закрытого квартала (`filed`, снимки `tax::closing`) берётся большее
/// из снимка и книги. Строки — из куска книги `span`. Ошибка — только
/// переполнение от испорченных данных.
fn declared(span: &Span<'_>, profile: Option<&ProfileData>, year: u16, filed: &[Closing]) -> Result<Turnover, MoneyError> {
    let mut turnover = Turnover::default();
    for number in 1..=4 {
        let Some(quarter) = Quarter::new(year, number) else {
            continue;
        };
        let mut lines: BTreeMap<Option<IncomeKind>, Vec<Lumas>> = BTreeMap::new();
        for row in span
            .between(quarter.first_day(), quarter.last_day())
            .iter()
            .filter(|row| row.flow == Flow::Income)
        {
            turnover.rows += 1;
            match row.lumas() {
                Ok(lumas) => lines.entry(turnover::income_kind(profile, row)).or_default().push(lumas),
                Err(MoneyError::RateUnknown) => turnover.unrated += 1,
                Err(error) => return Err(error),
            }
        }
        let mut live: i64 = 0;
        for line in lines.into_values() {
            live = live.checked_add(Lumas::total(line)?.drams()).ok_or(MoneyError::Overflow)?;
        }
        let quarter_sum = match Closing::current(filed, quarter) {
            Some(closing) => {
                turnover.closed += 1;
                let snapshot = closing
                    .data()
                    .lines
                    .iter()
                    .try_fold(0_i64, |sum, line| sum.checked_add(line.income))
                    .ok_or(MoneyError::Overflow)?;
                live.max(snapshot)
            }
            None => live,
        };
        turnover.drams = turnover.drams.checked_add(quarter_sum).ok_or(MoneyError::Overflow)?;
    }
    Ok(turnover)
}

/// Оборот года по книге ([`declared`]).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct Turnover {
    /// Сумма строк расчётов, целые драмы.
    pub drams: i64,
    /// Строк дохода в валюте без курса: в сумму они не вошли.
    pub unrated: usize,
    /// Строк дохода всего.
    pub rows: usize,
    /// Кварталов, взятых из снимков закрытых кварталов.
    pub closed: usize,
}

impl Turnover {
    /// Знает ли книга о годе хоть что-то: доход или закрытый квартал.
    fn known(self) -> bool {
        self.rows > 0 || self.closed > 0
    }
}

/// Гербовый сбор за год.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Stamp {
    /// Ступень шкалы по базе — обороту года по расчётам налога с оборота:
    /// `amount` до дня `due`. `up_to` — верхняя граница ступени (соседние
    /// ступени с той же суммой — одной), `next` — с какой базы и сколько
    /// следующая. `unrated` — строк дохода без курса: база без них, и сбор
    /// может быть выше.
    Due {
        amount: i64,
        base: i64,
        up_to: Option<i64>,
        next: Option<(i64, i64)>,
        due: Date,
        unrated: usize,
    },
    /// Дохода за год нет, а весь год не остановлен: закон читается двояко —
    /// «базы нет», и сбора нет (ст. 11, ч. 2, п. 2), или база ноль, и это
    /// нижняя ступень «до 2 400 000» (ст. 12, ч. 1, п. 2); осторожно —
    /// `cautious`, нижняя (ответ бухгалтера 2026-10-19, вопрос 4.2).
    NoTurnover { cautious: i64, due: Date },
    /// Весь год — остановка и дохода нет: расчётов нет (НК, ст. 53, ч. 8),
    /// базы нет, сбора нет (ст. 11, ч. 2, п. 2; вопрос 4.2; вывод).
    Stopped,
    /// Иностранец или лицо без гражданства без статуса проживания (ст. 11,
    /// ч. 2, п. 3).
    Exempt,
    /// Освобождён ли, не известно: в профиле нет статуса проживания.
    Unknown(Missing),
    /// Правила на этот год нет.
    NoRule,
}

impl Stamp {
    /// Сумма за год, драмы; `None` — не известно. При двояком прочтении —
    /// осторожная: её и откладывать.
    pub fn amount(self) -> Option<i64> {
        match self {
            Self::Due { amount, .. } => Some(amount),
            Self::NoTurnover { cautious, .. } => Some(cautious),
            Self::Stopped | Self::Exempt => Some(0),
            Self::Unknown(_) | Self::NoRule => None,
        }
    }
}

/// Гербовый сбор года `year`: ступень по обороту года ([`declared`]) — по
/// шкале отчётного года. Новый ИП и ушедший посреди года платят полную
/// ступень: пропорции закон не знает (ст. 10, ч. 1; ст. 12, ч. 1).
fn stamp(person: Option<&Person>, year: u16, months: &Months, base: Turnover) -> Stamp {
    let Some((rule, due)) =
        rules::for_year(rules::armenia().stamp_duty(), year).and_then(|rule| Some((rule, rule.due.after(year)?)))
    else {
        return Stamp::NoRule;
    };
    match person.and_then(|person| person.residency) {
        Some(Residency::ForeignWithoutStatus) => return Stamp::Exempt,
        Some(Residency::Citizen | Residency::ForeignWithStatus) => {}
        None => {
            return Stamp::Unknown(Missing {
                residency: true,
                ..Missing::default()
            });
        }
    }
    if base.rows == 0 {
        // Весь год внутри остановки и без дохода: расчётов за него нет.
        if months.span.is_some() && months.count() == 0 {
            return Stamp::Stopped;
        }
        return match rule.steps.first() {
            Some(lowest) => Stamp::NoTurnover {
                cautious: lowest.amount,
                due,
            },
            None => Stamp::NoRule,
        };
    }
    let Some(index) = rule.step(base.drams) else {
        return Stamp::NoRule;
    };
    let amount = rule.steps[index].amount;
    // Соседние ступени с той же суммой — одна: так закон пишет шкалу 2026
    // года (три ступени по 12 000 ֏).
    let top = rule.steps[index..]
        .iter()
        .take_while(|step| step.amount == amount)
        .last()
        .unwrap_or(&rule.steps[index]);
    let next = top
        .up_to
        .and_then(|up_to| up_to.checked_add(1))
        .and_then(|from| Some((from, rule.steps[rule.step(from)?].amount)));
    Stamp::Due {
        amount,
        base: base.drams,
        up_to: top.up_to,
        next,
        due,
        unrated: base.unrated,
    }
}

/// Почему взноса медстраховки нет.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum HealthOut {
    /// Оборот прошлого года по книге — `income` — не больше `over`: за
    /// этот год закон для ИП ещё не действует (закон ՀՕ-459-Ն, ст. 45, ч. 1,
    /// п. 6). Книга может знать не весь доход — окно говорит это.
    BelowThreshold { over: i64, income: i64 },
    /// Деятельность остановлена в день, когда закон начал действовать для
    /// ИП, и не возобновлена в этом году: закон для него — с месяца после
    /// возобновления (ст. 45, ч. 1, п. 6; ст. 46, ч. 20.1).
    Stopped(Date),
    /// Иностранец или лицо без гражданства без статуса проживания:
    /// страхованию не подлежит — закрытый список ст. 11, ч. 1 называет
    /// только граждан и иностранцев со статусом.
    NoStatus,
}

/// Чего не хватает расчёту взноса, кроме ответов в профиле: число тут —
/// «нет значения», а окно говорит словами, от чего оно зависит.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum HealthUnclear {
    /// Оборота прошлого года книга не знает: записей за него нет. Больше
    /// `over` — `amount` до дня `due`, нет — не платит.
    NoPreviousYear { over: i64, amount: i64, due: Date },
    /// В прошлом году — строки дохода без курса: оборот `income` без них не
    /// больше `over`, а с ними может быть больше — и тогда `amount` до дня
    /// `due`.
    PreviousUnrated {
        over: i64,
        income: i64,
        unrated: usize,
        amount: i64,
        due: Date,
    },
    /// Гражданину уже исполнилось столько лет: если он резидент Армении
    /// (183 дня и больше за 12 месяцев, ст. 2, ч. 1, п. 30), взнос за него
    /// платит государство (ст. 11, ч. 2, п. 2; ч. 3; ст. 14, ч. 2), а
    /// резидентство профиль не хранит.
    Elderly(u8),
    /// В этом году гражданину исполняется столько лет, с которых взнос
    /// платит государство: с какого месяца — зависит от дня рождения, а за
    /// 2026 год — ещё и от дня подачи расчёта за 2025-й (ст. 46, ч. 20.2).
    TurnsAge(u8),
    /// В день, когда закон начал действовать для ИП, деятельность была
    /// остановлена, а возобновлена в этом году — в этот день: закон для него
    /// — с месяца после возобновления, а сколько платить за этот год, закон
    /// не говорит (ст. 46, ч. 20.1).
    Resumed(Date),
    /// ИП зарегистрирован в этом году — году годовой ставки: оборота за
    /// прошлый он не задекларировал, и по прочтению за этот год не платит
    /// (закон для него — со следующего года, ст. 45, ч. 1, п. 6), но по букве
    /// ст. 14, ч. 4 возможен и помесячный взнос; разъяснения ՊԵԿ нет, и
    /// в программе это «нет значения», а не 0 (ответ бухгалтера 2026-10-19,
    /// вопрос 5.2).
    NewIe,
    /// В день `starts`, когда закон начинает действовать для «остальных»
    /// ИП, деятельность остановлена — а закон для этого ИП, возможно,
    /// действует уже с прошлого года: срез остановки — в пункте о тех, для
    /// кого закон вступает только в этот день (ст. 45, ч. 1, п. 6), и тогда
    /// остановка посреди его действия взноса не снимает (вопрос 5.7); по
    /// букве же срез — о любом ИП. `known` — что закон действовал раньше,
    /// известно по обороту позапрошлого года.
    StoppedInForce { starts: Date, known: bool },
    /// Записи книги испорчены так, что суммы переполнились.
    Broken,
}

/// Взнос медстраховки за год.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Health {
    /// За год одной суммой — оборот прошлого года `income` больше `over`:
    /// `amount` до дня `due` (2026 год: ст. 45, ч. 1, п. 5; ст. 46, ч. 10
    /// и 12).
    Year {
        amount: i64,
        due: Date,
        over: i64,
        income: i64,
    },
    /// Помесячно — `monthly` до числа `due_day` следующего месяца, за
    /// `months` месяцев года: `amount` за год (ст. 14, ч. 4). `after` —
    /// день возобновления, после месяца которого взнос начался: в день,
    /// когда закон начал действовать, деятельность была остановлена.
    Month {
        monthly: i64,
        due_day: u8,
        months: u8,
        amount: i64,
        after: Option<Date>,
    },
    Out(HealthOut),
    /// Не известно: в профиле нет ответа.
    Unknown(Missing),
    Unclear(HealthUnclear),
    /// Правила на этот год нет.
    NoRule,
}

/// Остановка, внутри которой лежит день `day`.
fn stop_on(person: Option<&Person>, day: Date) -> Option<Stop> {
    person?
        .stops
        .iter()
        .copied()
        .find(|stop| stop.from() <= day && stop.until().is_none_or(|until| until > day))
}

/// Действовал ли закон для ИП ещё до дня `starts` — по прежней записи
/// о взносе: `Some(false)` — нет (прежней записи нет, ИП зарегистрирован
/// позже, его оборот был не выше порога или та же остановка отложила и
/// прежний срез), `Some(true)` — да, `None` — не узнать: оборота того года
/// книга не знает.
fn in_force_before(books: Books<'_>, person: Option<&Person>, starts: Date) -> Option<bool> {
    let Some(earlier) = starts
        .add_days(-1)
        .and_then(|day| rules::on(rules::armenia().health_insurance(), day))
    else {
        return Some(false);
    };
    let HealthBy::Year { income_over, .. } = earlier.by else {
        // Прежняя запись помесячная: закон уже действовал для всех ИП.
        return Some(true);
    };
    let first_year = earlier.from.year();
    if person.and_then(|person| person.registered).is_some_and(|day| day.year() >= first_year) {
        return Some(false);
    }
    // Остановка, отложившая и прежний срез, и эту: закон ещё не вступил
    // (ст. 46, ч. 20.1).
    if stop_on(person, earlier.starts).is_some_and(|stop| stop.until().is_none_or(|until| until > starts)) {
        return Some(false);
    }
    let year = first_year.checked_sub(1)?;
    let span = Span::new(books, Date::new(year, 1, 1)?, Date::new(year, 12, 31)?);
    let turnover = declared(&span, books.profile, year, books.closings).ok()?;
    if turnover.drams > income_over {
        Some(true)
    } else if !turnover.known() || turnover.unrated > 0 {
        None
    } else {
        Some(false)
    }
}

/// Взнос медстраховки года `year` по записи файла правил на этот год:
/// кто платит — по профилю, а за 2026 год — ещё и по обороту прошлого года
/// `previous` ([`declared`], с поданным из снимков закрытых кварталов).
fn health(books: Books<'_>, person: Option<&Person>, year: u16, previous: Result<Turnover, MoneyError>) -> Health {
    let Some(rule) = rules::for_year(rules::armenia().health_insurance(), year) else {
        return Health::NoRule;
    };
    let residency = person.and_then(|person| person.residency);
    let birth_year = person.and_then(|person| person.birth_year);
    match residency {
        Some(Residency::ForeignWithoutStatus) => return Health::Out(HealthOut::NoStatus),
        // За гражданина-резидента 65 лет и старше платит государство,
        // а иностранец в этой группе не состоит (ст. 11, ч. 1, п. 2; ч. 2,
        // п. 2): возраст смотрится только у гражданина.
        Some(Residency::Citizen) => {
            let Some(born) = birth_year else {
                return Health::Unknown(Missing {
                    birth_year: true,
                    ..Missing::default()
                });
            };
            let age = i32::from(year) - i32::from(born);
            let limit = i32::from(rule.state_pays_from_age);
            if age > limit {
                return Health::Unclear(HealthUnclear::Elderly(rule.state_pays_from_age));
            }
            if age == limit {
                return Health::Unclear(HealthUnclear::TurnsAge(rule.state_pays_from_age));
            }
        }
        Some(Residency::ForeignWithStatus) => {}
        None => {
            return Health::Unknown(Missing {
                residency: true,
                ..Missing::default()
            });
        }
    }
    // Остановленному в день, когда закон начал действовать для ИП, он —
    // с месяца после возобновления; а тому, для кого закон уже действовал,
    // этот срез, по прочтению, не писан.
    let resumed = match stop_on(person, rule.starts).map(Stop::until) {
        None => None,
        Some(until) => {
            match in_force_before(books, person, rule.starts) {
                Some(false) => {}
                Some(true) => {
                    return Health::Unclear(HealthUnclear::StoppedInForce {
                        starts: rule.starts,
                        known: true,
                    });
                }
                None => {
                    return Health::Unclear(HealthUnclear::StoppedInForce {
                        starts: rule.starts,
                        known: false,
                    });
                }
            }
            match until {
                Some(until) if until.year() <= year => Some(until),
                Some(_) | None => return Health::Out(HealthOut::Stopped(rule.starts)),
            }
        }
    };
    let registered = person.and_then(|person| person.registered);
    match rule.by {
        HealthBy::Year { income_over, due } => {
            if let Some(until) = resumed {
                return Health::Unclear(HealthUnclear::Resumed(until));
            }
            if registered.is_some_and(|day| day.year() >= year) {
                return Health::Unclear(HealthUnclear::NewIe);
            }
            // Годовая ставка — двенадцать месячных (ст. 13, ч. 1; ст. 46, ч. 10).
            let amount = rule.monthly.saturating_mul(12);
            let Ok(previous) = previous else {
                return Health::Unclear(HealthUnclear::Broken);
            };
            if previous.drams > income_over {
                Health::Year {
                    amount,
                    due,
                    over: income_over,
                    income: previous.drams,
                }
            } else if !previous.known() {
                Health::Unclear(HealthUnclear::NoPreviousYear {
                    over: income_over,
                    amount,
                    due,
                })
            } else if previous.unrated > 0 {
                Health::Unclear(HealthUnclear::PreviousUnrated {
                    over: income_over,
                    income: previous.drams,
                    unrated: previous.unrated,
                    amount,
                    due,
                })
            } else {
                Health::Out(HealthOut::BelowThreshold {
                    over: income_over,
                    income: previous.drams,
                })
            }
        }
        HealthBy::Month { due_day } => monthly(rule, due_day, year, registered, resumed),
    }
}

/// Помесячный взнос года `year`: с месяца регистрации в этом году, с месяца,
/// когда закон начал действовать, или с месяца после возобновления
/// `resumed` — что позже — по декабрь. Остановка посреди года взноса не
/// снимает (вопрос 5.7).
fn monthly(rule: &HealthInsurance, due_day: u8, year: u16, registered: Option<Date>, resumed: Option<Date>) -> Health {
    let mut first = 1;
    if let Some(day) = registered.filter(|day| day.year() == year) {
        first = first.max(day.month());
    }
    if rule.starts.year() == year {
        first = first.max(rule.starts.month());
    }
    let mut after = None;
    if let Some(until) = resumed {
        match until.last_of_month().add_days(1) {
            Some(next) if next.year() == year => {
                first = first.max(next.month());
                after = Some(until);
            }
            Some(next) if next.year() < year => {}
            Some(_) | None => return Health::Out(HealthOut::Stopped(rule.starts)),
        }
    }
    let months = 13 - first;
    Health::Month {
        monthly: rule.monthly,
        due_day,
        months,
        amount: rule.monthly.saturating_mul(i64::from(months)),
        after,
    }
}

/// В каком положении ИП в году.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Status {
    /// На налоге с оборота весь год или его часть.
    Turnover,
    /// Весь год — общий режим: оборот прошлого года больше порога (НК,
    /// ст. 254, ч. 2) или доход по закрывающему коду 1 января; налог на
    /// прибыль, соцплатёж и гербовый сбор считаются иначе (НК, ст. 125,
    /// ч. 3.1; ՀՕ-244-Ն, ст. 6, ч. 2, п. 2; ՀՕ-245-Ն, ст. 9, ч. 1, п. 5) — их
    /// считает бухгалтер.
    General,
    /// В этом году ИП ещё не был: зарегистрирован позже.
    NotYet,
}

/// Годовые платежи за год.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Annual {
    pub year: u16,
    /// Версия правил, по которой посчитаны.
    pub rules: Version,
    pub status: Status,
    pub months: Months,
    /// Налог на прибыль.
    pub profit: Fixed,
    pub social: Social,
    pub stamp: Stamp,
    pub health: Health,
    /// Строк дохода без курса в прошлом и этом году — когда есть порог
    /// налога с оборота: без них не узнать, не перешёл ли его оборот, и режим
    /// года тогда под вопросом.
    pub regime_unrated: usize,
    /// Первый день дохода в целом месяце остановки: если деятельность
    /// фактически возобновлена, остановка кончилась в этот день (НК, ст. 53,
    /// ч. 8), и месяцы — другие.
    pub stop_income: Option<Date>,
    /// Документов и записей книги, которые не читаются: какой у них доход,
    /// неизвестно.
    pub unreadable: usize,
    /// Режим или оборот года не узнать: данные испорчены так, что суммы
    /// переполнились.
    pub broken: bool,
}

/// Сумма платежей года, которые платятся в срок налога на прибыль: налог
/// на прибыль, соцплатёж и гербовый сбор.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Total {
    /// Срок; `None` — правила налога на прибыль на этот год нет.
    pub due: Option<Date>,
    /// Сумма известных платежей, драмы; при двояком прочтении —
    /// осторожная. `None` — не известен ни один: ноль выглядел бы расчётом.
    pub sum: Option<i64>,
    /// Известно ли всё: платежи, профиль, книга, режим года и правила.
    pub complete: bool,
}

impl Total {
    /// Итог года `annual`.
    pub fn of(annual: &Annual) -> Self {
        let due = match annual.profit {
            Fixed::Due { due, .. } => Some(due),
            Fixed::NoRule => None,
        };
        match annual.status {
            Status::Turnover => {}
            Status::NotYet => {
                return Self {
                    due,
                    sum: Some(0),
                    complete: true,
                };
            }
            Status::General => {
                return Self {
                    due,
                    sum: None,
                    complete: false,
                };
            }
        }
        let social = match annual.social {
            Social::Due { payment, .. } => payment.amount(),
            Social::Out(_) => Some(0),
            Social::Unknown(_) | Social::NoRule => None,
        };
        let parts = [annual.profit.amount(), social, annual.stamp.amount()];
        let sum = parts
            .iter()
            .flatten()
            .copied()
            .reduce(|sum, part| sum.saturating_add(part));
        // Строки без курса меняют сбор, только если есть ступень выше.
        let unrated = matches!(annual.stamp, Stamp::Due { unrated, next: Some(_), .. } if unrated > 0);
        let complete = parts.iter().all(Option::is_some)
            && !unrated
            && !annual.months.registration_unknown
            && !annual.months.left_midyear()
            && annual.regime_unrated == 0
            && annual.stop_income.is_none()
            && annual.unreadable == 0
            && !annual.broken;
        Self { due, sum, complete }
    }
}

/// Сколько откладывать в месяц, чтобы к сроку набралась сумма [`Total`].
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Saving {
    /// Месяцев до срока — с этого по месяц срока включительно.
    pub months: u32,
    /// Откладывать в месяц, драмы: сумма, поделенная на месяцы, вверх до
    /// драма — чтобы к сроку набралось не меньше.
    pub per_month: i64,
}

/// Сколько откладывать в месяц с сегодняшнего дня `today`; `None` — срок
/// прошёл, его нет или платить нечего. Срок — с переносом с нерабочего
/// дня (`tax::deadlines`), как его показывает окно.
pub fn saving(annual: &Annual, today: Date) -> Option<Saving> {
    let total = Total::of(annual);
    let due = total.due.map(deadlines::last_day).filter(|due| today <= *due)?;
    let sum = total.sum.filter(|sum| *sum > 0)?;
    let index = |day: Date| u32::from(day.year()) * 12 + u32::from(day.month());
    let months = index(due) - index(today) + 1;
    // Сумма больше нуля: вверх до драма без переполнения.
    let per_month = (sum - 1) / i64::from(months) + 1;
    Some(Saving { months, per_month })
}

/// Год, платежи за который ждут ближе всех в день `today`: прошлый — пока
/// его срок не прошёл и человек был ИП уже в нём, иначе этот. Срок —
/// правило налога на прибыль прошлого года с переносом с нерабочего дня
/// (`tax::deadlines`); правила нет — этот год.
pub fn due_year(today: Date, person: Option<&Person>) -> u16 {
    let this = today.year();
    let Some(previous) = this.checked_sub(1) else {
        return this;
    };
    let registered_later = person
        .and_then(|person| person.registered)
        .is_some_and(|day| day.year() > previous);
    let due = rules::for_year(rules::armenia().profit_tax(), previous)
        .and_then(|rule| rule.due.after(previous))
        .map(deadlines::last_day);
    match due {
        Some(due) if today <= due && !registered_later => previous,
        Some(_) | None => this,
    }
}

/// Годовые платежи года `year` — по профилю и книге (шапка модуля). Книга
/// читается одним проходом — строки прошлого и этого года (`Span`), —
/// а режим года и оборот кварталов берут из них свои куски: окно считает
/// год на каждое изменение данных (Правило 1; замер — тест
/// `annual_timing`).
pub fn year(books: Books<'_>, year: u16) -> Annual {
    let previous_year = year.checked_sub(1).filter(|previous| *previous >= Date::MIN_YEAR);
    let (from, until) = (
        previous_year.or(Some(year)).and_then(|first| Date::new(first, 1, 1)),
        Date::new(year, 12, 31),
    );
    let span = match (from, until) {
        (Some(from), Some(until)) => Span::new(books, from, until),
        _ => Span::empty(),
    };
    let regime = turnover::year_regime(&span, books.profile, year);
    let base = declared(&span, books.profile, year, &[]);
    let previous = match previous_year {
        Some(previous) => declared(&span, books.profile, previous, books.closings),
        None => Ok(Turnover::default()),
    };
    let broken = regime.is_err() || base.is_err() || previous.is_err();
    let person = books.profile.map(|profile| &profile.person);
    // Переполнение — только от испорченных данных: месяцы — как на налоге
    // с оборота, а окно говорит, что расчёт неполон.
    let months = months(person, year, regime.unwrap_or(Regime::Turnover));
    let status = if person.and_then(|person| person.registered).is_some_and(|day| day.year() > year) {
        Status::NotYet
    } else if months.span.is_none() && months.left.is_some() {
        Status::General
    } else {
        Status::Turnover
    };
    let threshold = Date::new(year, 1, 1).and_then(|day| rules::on(rules::armenia().turnover_threshold(), day));
    let regime_unrated = match (threshold, &base, &previous) {
        (Some(_), Ok(base), Ok(previous)) => base.unrated + previous.unrated,
        _ => 0,
    };
    let stop_income = match (Date::new(year, 1, 1), until) {
        (Some(first), Some(last)) => span
            .between(first, last)
            .iter()
            .find(|row| row.flow == Flow::Income && months.stopped.contains(&row.date.month()))
            .map(|row| row.date),
        _ => None,
    };
    let all = rules::armenia();
    Annual {
        year,
        rules: all.version(),
        status,
        profit: fixed(rules::for_year(all.profit_tax(), year), year, &months),
        social: social(person, year, &months),
        stamp: stamp(person, year, &months, base.unwrap_or_default()),
        health: health(books, person, year, previous),
        months,
        regime_unrated,
        stop_income,
        unreadable: books.unreadable,
        broken,
    }
}

#[cfg(test)]
mod tests;
