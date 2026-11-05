//! Пример расчёта — калькулятор лендинга (задача 34). Человек на странице
//! kvartik.com выбирает строку налога и вводит оборот и расходы квартала,
//! год рождения и гражданство, а модуль собирает из этого книгу — такую,
//! какую он вёл бы сам в программе, — и считает налог квартала и годовые
//! платежи тем же [`turnover::quarter`] и [`annual::year`], что окно. Второй
//! расчёт «для сайта» однажды разошёлся бы с первым (CLAUDE.md, Архитектура).
//!
//! # Книга примера
//!
//! - Профиль: один код деятельности и режим налога с оборота. У IT — код из
//!   списка высоких технологий на этот день, у остальных — код вне списка
//!   ([`PLAIN_CODE`]), а торговлю, производство или общепит человек выбирает
//!   коду, как в профиле окна. Аренда и роялти — строка самого дохода
//!   (`IncomeKind::OF_INCOME`), как у записи книги.
//! - Доход — записью в драмах на первый день каждого квартала этого
//!   и прошлого года, расход с документом продавца — так же, когда у строки
//!   есть вычет. Год — четыре квартала: от его оборота зависит гербовый
//!   сбор, а от оборота прошлого года — взнос медстраховки. Кварталов раньше
//!   первого квартала правил (`turnover::first_quarter`) нет.
//! - Перенос вычета из расчётов до программы — «не было» (`tax::opening`).
//! - ИП зарегистрирован в начале прошлого года и деятельность не
//!   останавливал: месяцы года — все.
//!
//! Эти допущения ответ называет словами (`Key::CalcAssume`): число — пример,
//! а не расчёт человека (CLAUDE.md, Правило 2, «Образец назван образцом»).
//!
//! # Ответ странице
//!
//! [`answer`] разбирает поля формы по правилам языка страницы — тем же
//! [`i18n::parse_money`] и [`birth_year`], что окно, — и отдаёт ответ уже
//! словами, блоками: страница кладёт их в разметку текстом и сама ничего не
//! считает и не пишет (Правило 13: в DOM — только текстом). Часов модуль не
//! спрашивает: «сегодня» передаёт оболочка (Архитектура, «Время —
//! параметр»).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::annual::{self, Annual, Fixed, Health, HealthUnclear, Missing, Social, Stamp, Status, Total};
use super::deadlines;
use super::opening::Opening;
use super::rules;
use super::turnover::{self, Books, Leaving, QuarterTax, Regime};
use crate::i18n::{self, Key, Lang, fill, number_problem, t};
use crate::model::activity::{ActivityCode, IncomeKind};
use crate::model::date::{Date, Quarter};
use crate::model::ledger::{Flow, Nature, Operation, OperationData, Proof};
use crate::model::money::{Currency, Money};
use crate::model::profile::{Person, ProfileData, Regime as TaxRegime, Residency, birth_year};
use crate::model::record::Timestamp;
use crate::model::text::Line;

/// Строки налога калькулятора — в порядке его списка: те, что бывают у ИП,
/// который его открывает. Остальные (вторсырьё, газеты, отчуждение активов,
/// прочие доходы при заявлении об общепите) — редкость, и их считает
/// программа.
pub const KINDS: [IncomeKind; 6] = [
    IncomeKind::HighTech,
    IncomeKind::Other,
    IncomeKind::Trade,
    IncomeKind::Production,
    IncomeKind::Catering,
    IncomeKind::Rent,
];

/// Код вне списка высоких технологий и вне кодов, закрывающих налог
/// с оборота: «прочая профессиональная, научная и техническая
/// деятельность». Строку ему решает выбор в профиле или правило «прочая
/// деятельность» (`turnover::default_kind`); что он не в списках, держит
/// тест.
const PLAIN_CODE: &str = "74.90.0";

/// Момент изменения записей книги примера: она нигде не хранится, и часы ей
/// не нужны.
const STAMP: Timestamp = Timestamp::from_unix_millis(0);

/// Самая большая сумма поля: оборот квартала больше триллиона драмов
/// давно за порогом налога с оборота, а суммы восьми кварталов книги так
/// не переполнятся.
const MAX_DRAMS: i64 = 1_000_000_000_000;

/// Что ввёл человек — уже разобранным.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Input {
    /// Строка налога — одна из [`KINDS`].
    pub kind: IncomeKind,
    /// Оборот квартала, драмы.
    pub turnover: i64,
    /// Расходы квартала с документом продавца, драмы.
    pub expenses: i64,
    /// Год рождения, гражданство и ответы о пенсии и резидентстве — как
    /// в профиле; «нет значения» — `None` (Правило 6).
    pub birth_year: Option<u16>,
    pub residency: Option<Residency>,
    pub pension_joined: Option<bool>,
    pub resident: Option<bool>,
}

/// Пример: налог квартала, в котором `today`, и годовые платежи его года.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Example {
    pub quarter: QuarterTax,
    pub annual: Annual,
}

/// «Сегодня» примера: день часов устройства `clock`, но не раньше дня
/// последней сверки правил. Часы, отстающие от него, врут (CLAUDE.md,
/// Правило 6, «Часы устройства могут врать»), а пример на правилах прошлых
/// лет был бы не тем. Часов нет — день сверки.
pub fn today(clock: Option<Date>) -> Option<Date> {
    match (clock, rules::armenia().checked()) {
        (Some(day), Some(checked)) => Some(day.max(checked)),
        (day, checked) => day.or(checked),
    }
}

/// Пример расчёта для `input` в день `today` (шапка модуля, «Книга примера»).
pub fn example(input: &Input, today: Date) -> Example {
    let book = Book::new(input, today);
    let books = Books {
        documents: &[],
        operations: &book.operations,
        profile: Some(&book.profile),
        closings: &[],
        openings: &book.openings,
        unreadable: 0,
    };
    Example {
        quarter: turnover::quarter(books, today.quarter()),
        annual: annual::year(books, today.year()),
    }
}

/// Есть ли у строки `kind` в день `on` вычет расходов: только тогда форма
/// спрашивает расходы.
pub fn has_deduction(kind: IncomeKind, on: Date) -> bool {
    turnover::rate(kind, on).is_some_and(|rule| rule.deduction.is_some())
}

/// Имя строки в разметке страницы — как в данных (`"high_tech"`): одно на
/// страницу и ответ.
pub fn kind_id(kind: IncomeKind) -> String {
    serde_json::to_value(kind)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// Строка калькулятора по имени из разметки; чужое имя — `None`.
fn kind_from(id: &str) -> Option<IncomeKind> {
    KINDS.into_iter().find(|kind| kind_id(*kind) == id)
}

/// Имя гражданства в разметке страницы — как в данных (`"citizen"`).
pub fn residency_id(residency: Residency) -> String {
    serde_json::to_value(residency)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// Все варианты гражданства — в порядке списка формы.
pub const RESIDENCIES: [Residency; 3] =
    [Residency::Citizen, Residency::ForeignWithStatus, Residency::ForeignWithoutStatus];

/// Книга примера.
struct Book {
    profile: ProfileData,
    operations: Vec<Operation>,
    openings: Vec<Opening>,
}

impl Book {
    fn new(input: &Input, today: Date) -> Self {
        let year = today.year();
        let previous = year.checked_sub(1).filter(|previous| *previous >= Date::MIN_YEAR);
        let code = code(input.kind, today);
        let mut kinds = BTreeMap::new();
        if let Some(code) = code
            && matches!(input.kind, IncomeKind::Trade | IncomeKind::Production | IncomeKind::Catering)
        {
            kinds.insert(code, input.kind);
        }
        let person = Person {
            birth_year: input.birth_year,
            pension_joined: input.pension_joined,
            pension_applied: None,
            residency: input.residency,
            resident: input.resident,
            registered: previous.and_then(|previous| Date::new(previous, 1, 1)),
            stops: Vec::new(),
        };
        let profile = ProfileData {
            activities: code.into_iter().collect(),
            kinds,
            regime: Some(TaxRegime::Turnover),
            person,
            ..ProfileData::default()
        };
        let income_kind = IncomeKind::OF_INCOME.contains(&input.kind).then_some(input.kind);
        let first = turnover::first_quarter();
        let quarters: Vec<Quarter> = previous
            .into_iter()
            .chain([year])
            .flat_map(|year| (1..=4).filter_map(move |number| Quarter::new(year, number)))
            .filter(|quarter| first.is_some_and(|first| *quarter >= first))
            .collect();
        let mut operations = Vec::new();
        for quarter in &quarters {
            let day = quarter.first_day();
            operations.extend(record(Flow::Income, day, input.turnover, income_kind));
            if has_deduction(input.kind, day) {
                operations.extend(record(Flow::Expense, day, input.expenses, None));
            }
        }
        let openings = quarters
            .first()
            .and_then(|first| Opening::save(None, *first, BTreeMap::new(), STAMP).ok().flatten())
            .into_iter()
            .collect();
        Self {
            profile,
            operations,
            openings,
        }
    }
}

/// Код профиля примера: у IT — первый код списка высоких технологий на день
/// `on`, у остальных — [`PLAIN_CODE`]. `None` — списка на этот день нет.
fn code(kind: IncomeKind, on: Date) -> Option<ActivityCode> {
    let text = match kind {
        IncomeKind::HighTech => rules::on(rules::armenia().high_tech(), on)?.codes.first()?.as_str(),
        _ => PLAIN_CODE,
    };
    ActivityCode::parse(text).ok()
}

/// Запись книги: доход или расход в драмах с документом продавца. Нулевой
/// суммы записи нет — её нет и в книге человека.
fn record(flow: Flow, day: Date, drams: i64, income_kind: Option<IncomeKind>) -> Option<Operation> {
    let (proof, nature) = match flow {
        Flow::Income => (None, None),
        Flow::Expense => (Some(Proof::SettlementInvoice), Some(Nature::Ordinary)),
    };
    let data = OperationData {
        flow,
        date: day,
        paid: None,
        amount: Money::new(drams, Currency::Amd),
        rate: None,
        text: Line::new("Kvartik").ok()?,
        client: None,
        supplier: Line::default(),
        proof,
        nature,
        activity: None,
        income_kind,
        attachments: Vec::new(),
    };
    Operation::create(data, STAMP).ok()
}

/// Поля формы, как их прислала страница, — строками. Чего нет — пустая
/// строка: страница без поля — не ошибка.
#[derive(Clone, Default, Debug, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Raw {
    pub kind: String,
    pub turnover: String,
    pub expenses: String,
    pub birth: String,
    pub residency: String,
    pub pension: String,
    pub resident: String,
}

/// Ответ странице — уже словами на её языке.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Answer {
    /// Какие поля показать.
    pub fields: Fields,
    /// Ошибки полей словами — по имени поля; без ошибки поля нет.
    pub errors: BTreeMap<&'static str, String>,
    /// Ответ — блоками по порядку.
    pub blocks: Vec<Block>,
}

/// Поля, которые нужны не всегда: расходы — у строки с вычетом, вопрос
/// о пенсии — тому, для кого она по выбору, о резидентстве — гражданину
/// в возрасте, с которого за резидента взнос медстраховки платит
/// государство. Те же условия, что у профиля окна.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Fields {
    pub expenses: bool,
    pub pension: bool,
    pub resident: bool,
}

/// Блок ответа: заголовок, сумма, строка расчёта или слова о нём.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Block {
    pub kind: BlockKind,
    pub text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockKind {
    Head,
    Sum,
    Line,
    Note,
}

impl Block {
    fn new(kind: BlockKind, text: impl Into<String>) -> Self {
        Self {
            kind,
            text: text.into(),
        }
    }
}

/// Ответ на поля `raw` в день `today` словами языка `lang` (шапка модуля,
/// «Ответ странице»).
pub fn answer(lang: Lang, raw: &Raw, today: Date) -> Answer {
    let mut errors = BTreeMap::new();
    let kind = kind_from(&raw.kind).unwrap_or(KINDS[0]);
    let deduction = has_deduction(kind, today);
    let turnover = drams(lang, &raw.turnover, "turnover", &mut errors);
    let expenses = if deduction {
        drams(lang, &raw.expenses, "expenses", &mut errors)
    } else {
        None
    };
    let birth = if raw.birth.trim().is_empty() {
        None
    } else {
        let year = birth_year(&raw.birth, Some(today));
        if year.is_none() {
            errors.insert("birth", t(lang, Key::BirthYearInvalid).to_owned());
        }
        year
    };
    let residency = RESIDENCIES.into_iter().find(|residency| residency_id(*residency) == raw.residency);
    let fields = Fields {
        expenses: deduction,
        pension: birth.is_some_and(|year| annual::pension_is_a_choice(year, today)),
        resident: residency == Some(Residency::Citizen)
            && birth.is_some_and(|year| annual::residence_matters(year, today)),
    };
    let answer = |blocks| Answer {
        fields,
        errors: errors.clone(),
        blocks,
    };
    let prompt = || vec![Block::new(BlockKind::Note, t(lang, Key::CalcPrompt))];
    let Some(turnover) = turnover else {
        return answer(prompt());
    };
    if errors.contains_key("turnover") || errors.contains_key("expenses") {
        return answer(prompt());
    }
    let input = Input {
        kind,
        turnover,
        expenses: expenses.unwrap_or(0),
        birth_year: birth,
        residency,
        pension_joined: fields.pension.then(|| yes_no(&raw.pension)).flatten(),
        resident: fields.resident.then(|| yes_no(&raw.resident)).flatten(),
    };
    answer(render(lang, &example(&input, today), today))
}

/// Сумма поля в драмах. Пустое поле — `None` без ошибки: человек ещё не
/// ввёл. Ошибка — словами, как у полей окна (`i18n::number_problem`).
fn drams(lang: Lang, text: &str, field: &'static str, errors: &mut BTreeMap<&'static str, String>) -> Option<i64> {
    if text.trim().is_empty() {
        return None;
    }
    let example = i18n::amount(lang, Money::new(1_500_000, Currency::Amd));
    match i18n::parse_money(lang, text, Currency::Amd) {
        Ok(sum) if sum.minor() < 0 => {
            errors.insert(field, t(lang, Key::CalcNotNegative).to_owned());
            None
        }
        Ok(sum) if sum.minor() > MAX_DRAMS => {
            errors.insert(field, t(lang, Key::NumberTooLarge).to_owned());
            None
        }
        Ok(sum) => Some(sum.minor()),
        Err(error) => {
            errors.insert(field, number_problem(lang, error, &example));
            None
        }
    }
}

/// Ответ «да» или «нет» из списка формы; не выбран — `None`.
fn yes_no(text: &str) -> Option<bool> {
    match text {
        "yes" => Some(true),
        "no" => Some(false),
        _ => None,
    }
}

/// Сумма в драмах словами языка.
fn amd(lang: Lang, drams: i64) -> String {
    i18n::money(lang, Money::new(drams, Currency::Amd))
}

/// Процент словами языка.
fn percent(lang: Lang, value: rules::Percent) -> String {
    i18n::percent(lang, i64::from(value.hundredths()))
}

/// Пример словами: налог квартала, годовые платежи, чего не хватает,
/// и допущения книги примера.
fn render(lang: Lang, example: &Example, today: Date) -> Vec<Block> {
    let Example { quarter, annual } = example;
    let mut blocks = vec![Block::new(
        BlockKind::Head,
        fill(t(lang, Key::CalcQuarterHead), &[&i18n::quarter(lang, quarter.quarter)]),
    )];
    if let Regime::Left {
        why: Leaving::Threshold | Leaving::PreviousYear,
        ..
    } = quarter.regime
    {
        let threshold = quarter.threshold.map(|sum| amd(lang, sum)).unwrap_or_default();
        blocks.push(Block::new(BlockKind::Note, fill(t(lang, Key::CalcOverThreshold), &[&threshold])));
        return blocks;
    }
    quarter_lines(lang, quarter, &mut blocks);
    if annual.status == Status::Turnover {
        annual_lines(lang, annual, today, &mut blocks);
    }
    blocks.push(Block::new(BlockKind::Note, t(lang, Key::CalcAssume)));
    blocks
}

/// Налог квартала: сумма и строка расчёта — процент, вычет, перенос
/// и нижний предел, как в форме 299-Ն.
fn quarter_lines(lang: Lang, quarter: &QuarterTax, blocks: &mut Vec<Block>) {
    let Some(tax) = quarter.known() else {
        blocks.push(Block::new(BlockKind::Note, t(lang, Key::CalcQuarterUnknown)));
        return;
    };
    blocks.push(Block::new(BlockKind::Sum, amd(lang, tax)));
    for line in &quarter.lines {
        let name = t(lang, turnover::kind_key(line.kind));
        blocks.push(Block::new(
            BlockKind::Line,
            fill(
                t(lang, Key::CalcTimes),
                &[name, &percent(lang, line.rate), &amd(lang, line.income), &amd(lang, line.gross)],
            ),
        ));
        let Some(deduction) = line.deduction else {
            continue;
        };
        blocks.push(Block::new(
            BlockKind::Line,
            fill(
                t(lang, Key::CalcTimes),
                &[
                    t(lang, Key::CalcDeduction),
                    &percent(lang, deduction.percent),
                    &amd(lang, deduction.expenses),
                    &amd(lang, deduction.current),
                ],
            ),
        ));
        if deduction.carry_in > 0 {
            blocks.push(Block::new(
                BlockKind::Line,
                fill(t(lang, Key::CalcRow), &[t(lang, Key::CalcCarry), &amd(lang, deduction.carry_in)]),
            ));
        }
        blocks.push(Block::new(
            BlockKind::Line,
            fill(
                t(lang, Key::CalcTimes),
                &[
                    t(lang, Key::CalcFloor),
                    &percent(lang, deduction.floor_percent),
                    &amd(lang, line.income),
                    &amd(lang, deduction.floor),
                ],
            ),
        ));
    }
}

/// Годовые платежи: налог на прибыль, соцплатёж, гербовый сбор и их срок,
/// сколько откладывать в месяц, взнос медстраховки и чего не хватает.
fn annual_lines(lang: Lang, annual: &Annual, today: Date, blocks: &mut Vec<Block>) {
    blocks.push(Block::new(
        BlockKind::Head,
        fill(t(lang, Key::CalcAnnualHead), &[&annual.year.to_string()]),
    ));
    let none = t(lang, Key::CalcValueNone);
    let unknown = t(lang, Key::CalcValueUnknown);
    let profit = match annual.profit {
        Fixed::Due { amount, .. } => amd(lang, amount),
        Fixed::NoRule => unknown.to_owned(),
    };
    let social = match annual.social {
        Social::Due { payment, .. } => payment.amount().map_or_else(|| unknown.to_owned(), |sum| amd(lang, sum)),
        Social::Out(_) => none.to_owned(),
        Social::Unknown(_) | Social::NoRule => unknown.to_owned(),
    };
    let stamp = match annual.stamp {
        Stamp::Due { amount, .. } | Stamp::NoTurnover { cautious: amount, .. } => amd(lang, amount),
        Stamp::Stopped | Stamp::Exempt => none.to_owned(),
        Stamp::Unknown(_) | Stamp::NoRule => unknown.to_owned(),
    };
    for (label, value) in [(Key::AnnualProfit, profit), (Key::AnnualSocial, social), (Key::AnnualStamp, stamp)] {
        blocks.push(Block::new(BlockKind::Line, fill(t(lang, Key::CalcRow), &[t(lang, label), &value])));
    }
    let total = Total::of(annual);
    if let (Some(due), Some(sum)) = (total.due, total.sum) {
        let due = i18n::date(lang, deadlines::last_day(due));
        blocks.push(Block::new(BlockKind::Sum, fill(t(lang, Key::CalcAnnualTotal), &[&due, &amd(lang, sum)])));
    }
    if !total.complete {
        blocks.push(Block::new(BlockKind::Note, t(lang, Key::AnnualPartialShort)));
    }
    if let Some(saving) = annual::saving(annual, today) {
        let text = if saving.months <= 1 {
            t(lang, Key::AnnualSaveLast).to_owned()
        } else {
            let key = if total.complete { Key::AnnualSave } else { Key::AnnualSavePartial };
            fill(t(lang, key), &[&amd(lang, saving.per_month)])
        };
        blocks.push(Block::new(BlockKind::Note, text));
    }
    let health = match annual.health {
        Health::Year { amount, due, .. } => fill(
            t(lang, Key::CalcHealthYear),
            &[&amd(lang, amount), &i18n::date(lang, deadlines::last_day(due))],
        ),
        Health::Month { monthly, due_day, .. } => fill(
            t(lang, Key::CalcHealthMonthly),
            &[&amd(lang, monthly), &due_day.to_string()],
        ),
        Health::Out(_) => none.to_owned(),
        Health::Unknown(_) | Health::Unclear(_) | Health::NoRule => unknown.to_owned(),
    };
    blocks.push(Block::new(
        BlockKind::Line,
        fill(t(lang, Key::CalcRow), &[t(lang, Key::AnnualHealth), &health]),
    ));
    asks(lang, annual, blocks);
}

/// Чего не хватает примеру — вопросами о полях страницы.
fn asks(lang: Lang, annual: &Annual, blocks: &mut Vec<Block>) {
    let mut missing = Missing::default();
    let mut add = |more: Missing| {
        missing.birth_year |= more.birth_year;
        missing.pension_joined |= more.pension_joined;
        missing.residency |= more.residency;
    };
    if let Social::Unknown(more) = annual.social {
        add(more);
    }
    if let Stamp::Unknown(more) = annual.stamp {
        add(more);
    }
    if let Health::Unknown(more) = annual.health {
        add(more);
    }
    let resident = matches!(annual.health, Health::Unclear(HealthUnclear::Elderly(_)));
    for (asked, key) in [
        (missing.birth_year, Key::CalcAskBirthYear),
        (missing.residency, Key::CalcAskResidency),
        (missing.pension_joined, Key::CalcAskPension),
        (resident, Key::CalcAskResident),
    ] {
        if asked {
            blocks.push(Block::new(BlockKind::Note, t(lang, key)));
        }
    }
    if matches!(annual.health, Health::Unclear(unclear) if !matches!(unclear, HealthUnclear::Elderly(_))) {
        blocks.push(Block::new(BlockKind::Note, t(lang, Key::CalcHealthUnclear)));
    }
}

#[cfg(test)]
mod tests;
