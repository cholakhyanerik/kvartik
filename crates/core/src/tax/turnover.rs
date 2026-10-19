//! Налог с оборота (задача 19): кто не может быть его плательщиком, какой
//! строкой таблицы облагается доход и налог квартала.
//!
//! Всё ниже — прочтение агента, сверенное ответами бухгалтера-агента
//! 2026-10-19 (FEATURE_TASKS.md, «PDF против текста закона»): по решению
//! человека 2026-10-19 оно идёт в код словами «сверьте с бухгалтером», пока
//! расчёт не сверят настоящие бухгалтеры (задача 27).
//!
//! # Как считается квартал
//!
//! Так же, как форма расчёта ՊԵԿ (приказ 299-Ն в ред. 420-Ն,
//! https://www.arlis.am/hy/acts/205434; сверено по тексту 2026-10-19), —
//! строкой на каждый вид дохода таблицы НК, ст. 258, ч. 1 ([`Line`]):
//!
//! - доход строки (x.1) — доход квартала по дню оказания услуги, в драмах
//!   с лумами по курсу ЦБ, доведённый до драма один раз (`Lumas::drams`);
//! - у строк без вычета налог — ставка, умноженная на доход (ст. 260, ч. 1);
//! - у строк с вычетом (торговля, производство, общепит, прочая
//!   деятельность; ст. 258, ч. 2–5) налог по ставке (x.3) уменьшается на
//!   процент расходов квартала (x.6) и перенос с прошлых кварталов (x.7), но
//!   не ниже нижнего предела (x.9); чего не хватило — переносится дальше
//!   (x.11, ст. 258, ч. 7): уменьшение налога в драмах, а не расходы, без
//!   срока и через кварталы без дохода этой строки;
//! - расход с видом деятельности идёт в его строку, а общий —
//!   административный и сбытовой — делится между строками с вычетом по
//!   весу их дохода в обороте всех видов, 1 % IT тоже (ст. 260, ч. 3; ответ
//!   бухгалтера 2026-10-19, вопрос 1.4);
//! - каждая строка формы — целые драмы, лумы отбрасываются
//!   ([`Percent::of`]).
//!
//! Строку дохода решает вид деятельности ([`kind_of`]): у кода профиля —
//! строка, которую человек выбрал сам, иначе по правилам — высокие
//! технологии по списку правительства, остальное — прочая деятельность.
//!
//! # Когда налога с оборота нет
//!
//! - Оборот всех видов за год больше порога — с момента превышения до конца
//!   года ИП на общем режиме; сделка, на которой порог превышен, делится:
//!   до порога — налог с оборота, сверх — НДС (НК, ст. 255, ч. 1, п. 2;
//!   ст. 59, ч. 1, п. 5; ответ бухгалтера 2026-10-19, вопрос 7.3). Оборот
//!   прошлого года больше порога — весь год (ст. 254, ч. 2).
//! - Доход по коду, который закрывает налог с оборота (`excluded`), — со
//!   дня этой услуги до конца года (ст. 255, ч. 1, п. 3).
//!
//! Общий режим программа не считает: окно говорит «к бухгалтеру».
//!
//! # Перенос и закрытый квартал
//!
//! Перенос идёт цепочкой от первого квартала с записями — или от последнего
//! закрытого (`tax::closing`): закрытый квартал отдаёт перенос из своего
//! снимка, а не пересчитанный (Правило 2). Перенос из расчётов до первой
//! записи в программе неизвестен, и окно говорит это ([`QuarterTax::carry_unknown`]).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::closing::Closing;
use super::rules::{self, Exclusion, Percent, Rate, Rule, Version};
use crate::model::activity::{ActivityCode, IncomeKind};
use crate::model::date::{Date, Quarter};
use crate::model::document::Document;
use crate::model::ledger::{self, Flow, Nature, Operation, Proof, Row};
use crate::model::money::{Lumas, MoneyError};
use crate::model::profile::ProfileData;
use crate::model::record::RecordId;

/// Закрывает ли код налог с оборота в день `on`: правило, по которому
/// закрывает, или `None`.
///
/// Правила — `turnover_exclusions` файла правил. Сейчас это Налоговый
/// кодекс, ст. 254, ч. 3, п. 3.1: плательщиками налога с оборота не могут
/// быть «69․ Իրավաբանական և հաշվապահական գործունեություն», «70․
/// Գլխամասային գրասենյակների գործունեություն. կառավարման հարցերով
/// խորհրդատվական ծառայություններ», «78.2․ Ժամանակավոր աշխատուժով
/// ապահովման գործունեություն», «78.3․ Աշխատուժով ապահովման այլ
/// գործունեություն» «…գործունեություններ իրականացնողները» — те, кто ведёт
/// эту деятельность. Пункт внёс закон ՀՕ-72-Ն от 26.03.2025 — его ст. 2,
/// а ст. 255 поправила его ст. 3, — в силе с 2025-07-01 (ст. 9, ч. 3);
/// переходных правил для пункта нет — ст. 9 только откладывает ст. 2 и 3,
/// а поправки 2027 года (ՀՕ-83-Ն, ՀՕ-385-Ն) его не трогают (по закону;
/// сверено по тексту arlis 230455, 205201, 223255 и 228632 2026-10-19,
/// разведка задачи 18).
///
/// Это про человека целиком, а не про оборот одного вида: ИП —
/// плательщик налога с оборота «по всем видам деятельности» (ст. 254,
/// ч. 4), а со дня, когда «этот факт имел место», перестаёт им быть до
/// конца года (ст. 255, ч. 1, п. 3). Ведёт ли деятельность тот, у кого код
/// записан в реестре, но не используется, закон не говорит — поэтому окно
/// говорит «кто ведёт» и советует сверить с бухгалтером.
pub fn excluded(code: &ActivityCode, on: Date) -> Option<&'static Exclusion> {
    rules::armenia().turnover_exclusions().iter().find(|rule| {
        rule.period().contains(on) && rule.codes.iter().any(|prefix| code.starts_with(prefix))
    })
}

/// Строка налога для кода в день `on`: та, что человек выбрал коду
/// в профиле, иначе — по правилам ([`default_kind`]).
pub fn kind_of(profile: Option<&ProfileData>, code: &ActivityCode, on: Date) -> IncomeKind {
    profile
        .and_then(|profile| profile.kinds.get(code))
        .copied()
        .unwrap_or_else(|| default_kind(code, on))
}

/// Строка налога для кода по правилам, без выбора человека: код из списка
/// высоких технологий на день `on` — строка 8, остальное — строка 10,
/// прочая деятельность. По коду закон узнаёт только высокие технологии:
/// торговлю, производство, аренду он называет видом дохода, а не кодом
/// (ответ бухгалтера 2026-10-19, вопросы 1.3 и 1.6).
pub fn default_kind(code: &ActivityCode, on: Date) -> IncomeKind {
    let listed = rules::on(rules::armenia().high_tech(), on).is_some_and(|list| list.contains(code));
    if listed { IncomeKind::HighTech } else { IncomeKind::Other }
}

/// Ставка строки в день `on`; `None` — правила на этот день нет: у строки 8
/// — с 2032 года (ответ бухгалтера 2026-10-19, вопрос 1.7: «правила нет»,
/// а не 10 %), у всех — до 2025 года, когда ставки были другими.
pub fn rate(kind: IncomeKind, on: Date) -> Option<&'static Rate> {
    rules::armenia()
        .turnover_rates()
        .iter()
        .find(|rate| rate.kind == kind && rate.period().contains(on))
}

/// Ставка строки на весь квартал: одна запись в первый и в последний день.
/// `None` — правила нет или оно сменилось посреди квартала: форма ՊԵԿ
/// знает одну ставку строки на квартал, и такой квартал — к бухгалтеру.
fn quarter_rate(kind: IncomeKind, quarter: Quarter) -> Option<&'static Rate> {
    let first = rate(kind, quarter.first_day())?;
    let last = rate(kind, quarter.last_day())?;
    (first == last).then_some(first)
}

/// Первый квартал, у которого есть ставки, — раньше программа налог не
/// считает: ставки 2024 года были другими (ՀՕ-285-Ն, ст. 16, ч. 1).
pub fn first_quarter() -> Option<Quarter> {
    rules::armenia()
        .turnover_rates()
        .iter()
        .map(|rate| rate.from.quarter())
        .min()
}

/// Почему расход не идёт в вычет.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Refusal {
    /// Документа продавца нет — или он не из тех, что подтверждают расход.
    NoDocument,
    /// Только платёж банка, а расход — покупка: платежа хватает только
    /// зарплате, налогам, пошлинам и услугам банка (НК, ст. 55, ч. 12).
    PaymentOnly,
    /// Такие расходы не вычитаются: основные средства, командировки,
    /// представительские (ст. 258, ч. 6, п. 2).
    Excluded,
    /// Правила на этот день нет.
    NoRule,
}

/// Идёт ли расход в вычет — по документу продавца и виду расхода, правилом
/// `turnover_expenses` на день расхода.
pub fn deductible(proof: Proof, nature: Nature, on: Date) -> Result<(), Refusal> {
    let rule = rules::on(rules::armenia().turnover_expenses(), on).ok_or(Refusal::NoRule)?;
    if rule.excluded.contains(&nature) {
        return Err(Refusal::Excluded);
    }
    match proof {
        Proof::BankPayment if rule.payment_only.contains(&nature) => Ok(()),
        Proof::BankPayment => Err(Refusal::PaymentOnly),
        Proof::None => Err(Refusal::NoDocument),
        Proof::TaxInvoice
        | Proof::SettlementInvoice
        | Proof::CashReceipt
        | Proof::IndividualPurchase
        | Proof::Contract
        | Proof::ForeignSupplier => {
            if rule.proofs.contains(&proof) {
                Ok(())
            } else {
                Err(Refusal::NoDocument)
            }
        }
    }
}

/// Строка расчёта — вид дохода, как в форме ՊԵԿ (приказ 299-Ն, прил. 1):
/// у строк с вычетом — разделы 5–7 и 9 формы, у остальных — строки 12–20.
/// Все суммы — целые драмы.
///
/// Лежит и в снимке закрытого квартала (`tax::closing`): в данных —
/// `{"kind":"other","income":…,"rate":"10","gross":…,"deduction":{…},"tax":…}`.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Line {
    pub kind: IncomeKind,
    /// Доход строки за квартал (x.1; [Ա]).
    pub income: i64,
    /// Ставка (x.2; [Բ]).
    pub rate: Percent,
    /// Налог по ставке, до вычета (x.3).
    pub gross: i64,
    /// Вычет расходов — у строк, где он есть; `None` — нет.
    pub deduction: Option<Deduction>,
    /// Налог строки к уплате (x.10; [Գ]).
    pub tax: i64,
}

/// Вычет расходов строки (ст. 258, ч. 2–7).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Deduction {
    /// Процент расходов (x.5).
    pub percent: Percent,
    /// Нижний предел — процент дохода.
    pub floor_percent: Percent,
    /// Расходы, отнесённые к строке видом деятельности расхода.
    pub attributed: i64,
    /// Доля общих административных и сбытовых расходов — по весу дохода
    /// строки в обороте всех видов (ст. 260, ч. 3).
    pub shared: i64,
    /// Расходы квартала (x.4).
    pub expenses: i64,
    /// Уменьшение налога за расходы квартала (x.6).
    pub current: i64,
    /// Перенос с прошлых кварталов (x.7).
    pub carry_in: i64,
    /// Уменьшение всего (x.8).
    pub total: i64,
    /// Нижний предел налога (x.9).
    pub floor: i64,
    /// Перенос на следующие кварталы (x.11).
    pub carry_out: i64,
}

/// Чего не хватает, чтобы налог квартала был посчитан целиком, — окно
/// обязано это назвать (Правило 6).
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Gaps {
    /// Строк в валюте без курса ЦБ: в драмы их не перевести.
    pub unrated: usize,
    /// Доходов без вида деятельности, когда коды профиля облагаются по
    /// разным строкам или среди них есть закрывающий налог с оборота
    /// (`activity_needed`).
    pub unassigned: usize,
    /// Доход есть, а кодов деятельности в профиле нет: строку налога не
    /// узнать.
    pub no_codes: bool,
    /// Общих расходов, которые не по чему делить: дохода в квартале нет,
    /// а строк с вычетом у ИП несколько.
    pub unallocated: usize,
    /// Строки, у которых на этот квартал нет правила.
    pub no_rule: Vec<IncomeKind>,
    /// Доходов без курса ЦБ в прошлом году и в прошлых кварталах этого:
    /// оборот для порога без них неизвестен.
    pub year_unrated: usize,
    /// Записей — документов и книги, — которые не читаются: какой у них
    /// доход и расход, неизвестно.
    pub unreadable: usize,
    /// Первый неполный квартал цепочки до этого: перенос из него тоже
    /// неполный.
    pub previous: Option<Quarter>,
}

impl Gaps {
    /// Ничего не упущено.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Расходы квартала, не вошедшие в вычет, — по причине.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct Refused {
    /// Без документа продавца.
    pub no_document: usize,
    /// Покупки, подтверждённые только платежом банка.
    pub payment_only: usize,
    /// Основные средства, командировки, представительские.
    pub excluded: usize,
}

/// Почему ИП перестал быть плательщиком налога с оборота.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Leaving {
    /// Оборот этого года превысил порог (ст. 255, ч. 1, п. 2).
    Threshold,
    /// Доход по коду, который закрывает налог с оборота (ст. 255, ч. 1,
    /// п. 3).
    Code(ActivityCode),
    /// Оборот прошлого года превысил порог (ст. 254, ч. 2).
    PreviousYear,
}

/// Режим в квартале.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Regime {
    /// Весь квартал — плательщик налога с оборота.
    Turnover,
    /// С дня `from` до конца года — нет: общий режим, к бухгалтеру.
    Left { from: Date, why: Leaving },
}

/// Налог квартала.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct QuarterTax {
    pub quarter: Quarter,
    /// Версия правил, по которой посчитан.
    pub rules: Version,
    /// Строки — в порядке таблицы закона.
    pub lines: Vec<Line>,
    /// Налог квартала — сумма строк, драмы.
    pub tax: i64,
    /// Доход, вошедший в расчёт, по месяцам квартала — драмы с лумами.
    pub months: [Lumas; 3],
    pub gaps: Gaps,
    pub refused: Refused,
    pub regime: Regime,
    /// Оборот всех видов с начала года по конец квартала, драмы.
    pub year_turnover: i64,
    /// Порог оборота года; `None` — правила нет.
    pub threshold: Option<i64>,
    /// Перенос из расчётов до первой записи в программе неизвестен: квартал
    /// — первый с записями, и в нём есть строка с вычетом.
    pub carry_unknown: bool,
    /// Коды профиля, которые закрывают налог с оборота, и с какого дня:
    /// ведёт ли их человек, программа не знает — окно говорит словами.
    pub excluded_codes: Vec<(ActivityCode, Date)>,
}

impl QuarterTax {
    /// Посчитан ли налог целиком: у всего дохода и расхода есть курс,
    /// строка и правило, и общие расходы поделены.
    pub fn complete(&self) -> bool {
        self.gaps.is_empty()
    }

    /// Перенос на следующий квартал — по строкам с вычетом. Ушедший
    /// с налога с оборота переносить некуда: невычтенное идёт в вычет
    /// общего режима (ст. 264, ч. 4) — это считает бухгалтер.
    pub fn carry_out(&self) -> BTreeMap<IncomeKind, i64> {
        if self.regime != Regime::Turnover {
            return BTreeMap::new();
        }
        carry_of(&self.lines)
    }

    /// Можно ли закрыть: квартал кончился до дня `today`, весь он — на
    /// налоге с оборота, и налог посчитан целиком: неполный снимок закрепил
    /// бы неверные числа, а снимок квартала с уходом с режима отдал бы
    /// перенос следующему году (ст. 264, ч. 4) — такой квартал считает
    /// бухгалтер.
    pub fn closable(&self, today: Date) -> bool {
        self.quarter.last_day() < today && self.complete() && self.regime == Regime::Turnover
    }

    /// Расходится ли расчёт со снимком закрытого квартала: данные квартала
    /// поправили после закрытия — нужен уточнённый расчёт (299-Ն, п. 1).
    pub fn differs_from(&self, closing: &Closing) -> bool {
        let data = closing.data();
        data.lines != self.lines || data.tax != self.tax
    }
}

/// Перенос по строкам — из строк расчёта или снимка.
pub(crate) fn carry_of(lines: &[Line]) -> BTreeMap<IncomeKind, i64> {
    lines
        .iter()
        .filter_map(|line| Some((line.kind, line.deduction?.carry_out)))
        .filter(|(_, carry)| *carry > 0)
        .collect()
}

/// Что нужно расчёту: все записи — документы и операции, включая удалённые
/// (расчёт отбирает живые сам), профиль и снимки закрытых кварталов.
#[derive(Clone, Copy, Debug)]
pub struct Books<'a> {
    pub documents: &'a [Document],
    pub operations: &'a [Operation],
    pub profile: Option<&'a ProfileData>,
    pub closings: &'a [Closing],
    /// Сколько документов и записей книги не читается.
    pub unreadable: usize,
}

/// Нужен ли доходу дня `on` вид деятельности — код профиля: коды
/// облагаются по разным строкам, или среди нескольких кодов есть
/// закрывающий налог с оборота. Без кода дохода тогда не узнать ни строку,
/// ни режим (ст. 255, ч. 1, п. 3); окно спрашивает его в форме дохода.
pub fn activity_needed(profile: Option<&ProfileData>, on: Date) -> bool {
    profile_kinds(profile, on).len() > 1 || code_unknown(profile, on)
}

/// Код дохода без вида не узнать, а он решает режим: кодов несколько, и
/// среди них есть закрывающий налог с оборота в день `on`.
fn code_unknown(profile: Option<&ProfileData>, on: Date) -> bool {
    profile.is_some_and(|data| {
        data.activities.len() > 1 && data.activities.iter().any(|code| excluded(code, on).is_some())
    })
}

/// Налог квартала `quarter` — с переносом цепочкой от последнего закрытого
/// квартала или от первого квартала с записями (шапка модуля).
pub fn quarter(books: Books<'_>, quarter: Quarter) -> QuarterTax {
    let first = first_quarter();
    let earliest = earliest(books).map(|earliest| first.map_or(earliest, |first| earliest.max(first)));
    let closed = books
        .closings
        .iter()
        .filter(|closing| !closing.is_deleted() && closing.quarter() < quarter)
        .max_by_key(|closing| (closing.quarter(), closing.modified()));
    let (start, mut carry, unknown) = match (closed, earliest) {
        // Следующий за закрытым есть: закрытый раньше `quarter`.
        (Some(closing), _) => (closing.quarter().next().unwrap_or(quarter), closing.carry_out(), false),
        (None, Some(earliest)) => (earliest, BTreeMap::new(), first.is_some_and(|first| earliest > first)),
        (None, None) => (quarter, BTreeMap::new(), false),
    };
    // С прошлого года первого квартала цепочки: по нему порог (`year`).
    let from = start
        .year()
        .checked_sub(1)
        .and_then(|year| Date::new(year, 1, 1))
        .unwrap_or_else(|| start.first_day());
    let span = Span::new(books, from, quarter.last_day());
    let mut incomplete = None;
    let mut current = start;
    while current < quarter {
        let tax = compute(books, &span, current, &carry, false);
        if incomplete.is_none() && !tax.complete() && tax.regime == Regime::Turnover {
            incomplete = Some(current);
        }
        carry = tax.carry_out();
        match current.next() {
            Some(next) => current = next,
            None => break,
        }
    }
    let mut tax = compute(books, &span, quarter, &carry, unknown && start == quarter);
    // Перенос из неполного квартала — тоже неполный (ст. 258, ч. 7); важен
    // он только строкам с вычетом.
    if tax.lines.iter().any(|line| line.deduction.is_some()) {
        tax.gaps.previous = incomplete;
    }
    tax
}

/// Строки книги на всю цепочку: один проход по документам и операциям,
/// а годы и кварталы берут свои куски по дню. Три прохода на квартал по всем
/// записям за все годы стоили цепочке в пять лет без закрытых кварталов
/// 19,5 мс на компьютере (замер задачи 19, тест `chain_timing`), а карточка
/// и экран считают две-три цепочки на каждое изменение данных — на телефоне
/// это не кадр, а четверть секунды (Правило 1).
struct Span<'a> {
    /// По дню, в день — по идентификатору: так порог делит строки (`year`).
    rows: Vec<Row<'a>>,
}

impl<'a> Span<'a> {
    fn new(books: Books<'a>, from: Date, until: Date) -> Self {
        let mut rows = ledger::between(books.documents, books.operations, from, until);
        rows.sort_by_key(|row| (row.date, row.id()));
        Self { rows }
    }

    /// Строки с дня `from` по день `until` включительно.
    fn between(&self, from: Date, until: Date) -> &[Row<'a>] {
        let start = self.rows.partition_point(|row| row.date < from);
        let end = self.rows.partition_point(|row| row.date <= until).max(start);
        &self.rows[start..end]
    }
}

/// Квартал самой ранней живой записи книги; `None` — записей нет.
fn earliest(books: Books<'_>) -> Option<Quarter> {
    let documents = ledger::income_documents(books.documents)
        .into_iter()
        .map(|document| document.data().service_date);
    let operations = books
        .operations
        .iter()
        .filter(|operation| !operation.is_deleted())
        .map(|operation| operation.data().date);
    documents.chain(operations).min().map(Date::quarter)
}

/// Строки налога, по которым облагаются коды профиля в день `on`.
fn profile_kinds(profile: Option<&ProfileData>, on: Date) -> BTreeSet<IncomeKind> {
    profile
        .map(|data| data.activities.iter().map(|code| kind_of(Some(data), code, on)).collect())
        .unwrap_or_default()
}

/// Код дохода: указанный у дохода, а без него — единственный код профиля.
/// Из нескольких кодов одной строки программа выбрать не может.
fn income_code(profile: Option<&ProfileData>, row: &Row<'_>) -> Option<ActivityCode> {
    row.activity.or_else(|| match profile.map(|data| data.activities.as_slice()) {
        Some([only]) => Some(*only),
        Some(_) | None => None,
    })
}

/// Что известно о годе квартала: режим и сколько дохода каждой строки вошло
/// в налог с оборота.
struct Year {
    regime: Regime,
    /// Доход, вошедший в налог с оборота: вся строка книги или, у сделки,
    /// на которой превышен порог, — часть до порога. Строк после выхода из
    /// режима и строк без курса здесь нет.
    taxed: BTreeMap<RecordId, Lumas>,
    /// Оборот всех видов с начала года по конец квартала.
    turnover: Lumas,
    /// Порог года в лумах.
    threshold: Option<i64>,
    /// Доходов без курса в прошлом году и в прошлых кварталах этого — до
    /// ухода с режима: без них оборот для порога неизвестен. Строки самого
    /// квартала считает `compute`.
    unrated: usize,
}

/// Режим года по конец квартала `quarter`: порог прошлого и этого года
/// и доход по коду, который закрывает налог с оборота (шапка модуля).
fn year(span: &Span<'_>, profile: Option<&ProfileData>, quarter: Quarter) -> Result<Year, MoneyError> {
    let Some(start) = Date::new(quarter.year(), 1, 1) else {
        return Ok(Year {
            regime: Regime::Turnover,
            taxed: BTreeMap::new(),
            turnover: Lumas::ZERO,
            threshold: None,
            unrated: 0,
        });
    };
    let threshold = rules::on(rules::armenia().turnover_threshold(), start).map(|rule| rule.amount);
    let threshold_lumas = threshold.and_then(Lumas::from_drams).map(Lumas::get);
    let income = |rows: &[Row<'_>]| -> Vec<(Date, RecordId, Option<ActivityCode>, Option<Lumas>)> {
        rows.iter()
            .filter(|row| row.flow == Flow::Income)
            .map(|row| (row.date, row.id(), income_code(profile, row), row.lumas().ok()))
            .collect()
    };
    let previous = match (quarter.year().checked_sub(1).and_then(|year| Date::new(year, 1, 1)), start.add_days(-1)) {
        (Some(from), Some(until)) => income(span.between(from, until)),
        _ => Vec::new(),
    };
    let previous_unrated = previous.iter().filter(|(_, _, _, lumas)| lumas.is_none()).count();
    let previous = Lumas::total(previous.iter().filter_map(|(_, _, _, lumas)| *lumas))?;
    // Уже по порядку дня и идентификатора (`Span`).
    let rows = income(span.between(start, quarter.last_day()));
    let turnover = Lumas::total(rows.iter().filter_map(|(_, _, _, lumas)| *lumas))?;
    let mut regime = match threshold_lumas {
        Some(limit) if previous.get() > limit => Regime::Left {
            from: start,
            why: Leaving::PreviousYear,
        },
        _ => Regime::Turnover,
    };
    let mut unrated = 0;
    let mut taxed = BTreeMap::new();
    if regime == Regime::Turnover {
        unrated = previous_unrated;
        // День дохода по закрывающему коду — весь вне режима («со дня»,
        // ст. 255, ч. 1, п. 3): порядок строк одного дня программе не
        // известен.
        let code_day = rows.iter().find_map(|(date, _, code, _)| {
            code.filter(|code| excluded(code, *date).is_some()).map(|code| (*date, code))
        });
        let first_day = quarter.first_day();
        let mut so_far: i64 = 0;
        for (date, id, _, lumas) in rows {
            if let Some((day, code)) = code_day
                && date >= day
            {
                regime = Regime::Left {
                    from: day,
                    why: Leaving::Code(code),
                };
                break;
            }
            let Some(lumas) = lumas else {
                if date < first_day {
                    unrated += 1;
                }
                continue;
            };
            match threshold_lumas {
                Some(limit) if so_far.saturating_add(lumas.get()) > limit => {
                    taxed.insert(id, Lumas::new(limit - so_far));
                    regime = Regime::Left {
                        from: date,
                        why: Leaving::Threshold,
                    };
                    break;
                }
                _ => {
                    so_far = so_far.saturating_add(lumas.get());
                    taxed.insert(id, lumas);
                }
            }
        }
    }
    // Без порога строкам без курса для режима решать нечего.
    if threshold_lumas.is_none() {
        unrated = 0;
    }
    Ok(Year {
        regime,
        taxed,
        turnover,
        threshold,
        unrated,
    })
}

/// Налог квартала `quarter` с переносом `carry_in`; строки книги — из
/// куска цепочки `span`.
fn compute(
    books: Books<'_>,
    span: &Span<'_>,
    quarter: Quarter,
    carry_in: &BTreeMap<IncomeKind, i64>,
    carry_unknown: bool,
) -> QuarterTax {
    let rules_version = rules::armenia().version();
    let excluded_codes = books
        .profile
        .map(|data| {
            data.activities
                .iter()
                .filter_map(|code| Some((*code, excluded(code, quarter.last_day())?.from)))
                .collect()
        })
        .unwrap_or_default();
    // Переполнение — только от испорченных данных, а их чтение не пропускает:
    // такой квартал — пустой и неполный, а не плашка сбоя.
    let year = match year(span, books.profile, quarter) {
        Ok(year) => year,
        Err(_) => {
            return QuarterTax {
                quarter,
                rules: rules_version,
                lines: Vec::new(),
                tax: 0,
                months: [Lumas::ZERO; 3],
                gaps: Gaps {
                    unrated: 1,
                    ..Gaps::default()
                },
                refused: Refused::default(),
                regime: Regime::Turnover,
                year_turnover: 0,
                threshold: None,
                carry_unknown,
                excluded_codes,
            };
        }
    };
    let left_from = match year.regime {
        Regime::Turnover => None,
        Regime::Left { from, .. } => Some(from),
    };
    let rows = span.between(quarter.first_day(), quarter.last_day());
    let mut gaps = Gaps {
        year_unrated: year.unrated,
        unreadable: books.unreadable,
        ..Gaps::default()
    };
    let mut refused = Refused::default();
    let mut months = [Lumas::ZERO; 3];
    let mut income: BTreeMap<IncomeKind, i64> = BTreeMap::new();
    let mut attributed: BTreeMap<IncomeKind, i64> = BTreeMap::new();
    let mut unassigned_lumas: i64 = 0;
    let mut shared: i64 = 0;
    let mut shared_rows = 0;
    let first_month = quarter.first_day().month();
    for row in rows {
        match row.flow {
            Flow::Income => {
                let Some(lumas) = year.taxed.get(&row.id()) else {
                    // Без курса доход не перевести в драмы — и в налог он не
                    // вошёл бы, пока режим не кончился.
                    if row.unrated() && left_from.is_none_or(|from| row.date < from) {
                        gaps.unrated += 1;
                    }
                    continue;
                };
                let month = usize::from(row.date.month() - first_month);
                months[month] = Lumas::new(months[month].get().saturating_add(lumas.get()));
                // Без вида — строка по единственной строке кодов профиля;
                // но если среди кодов есть закрывающий налог с оборота, код
                // дохода решает режим, и без него расчёт неполон.
                let (kind, unknown) = match row.activity {
                    Some(code) => (Some(kind_of(books.profile, &code, row.date)), false),
                    None => {
                        let kinds = profile_kinds(books.profile, row.date);
                        if kinds.is_empty() {
                            gaps.no_codes = true;
                        }
                        let mut kinds = kinds.into_iter();
                        let kind = match (kinds.next(), kinds.next()) {
                            (Some(only), None) => Some(only),
                            _ => None,
                        };
                        (kind, kind.is_none() || code_unknown(books.profile, row.date))
                    }
                };
                if unknown {
                    gaps.unassigned += 1;
                }
                match kind {
                    Some(kind) => {
                        let sum = income.entry(kind).or_insert(0);
                        *sum = sum.saturating_add(lumas.get());
                    }
                    None => {
                        unassigned_lumas = unassigned_lumas.saturating_add(lumas.get());
                    }
                }
            }
            Flow::Expense => {
                if left_from.is_some_and(|from| row.date >= from) {
                    continue;
                }
                let Ok(lumas) = row.lumas() else {
                    gaps.unrated += 1;
                    continue;
                };
                let Some((proof, nature)) = row.expense() else {
                    continue;
                };
                match deductible(proof, nature, row.date) {
                    Ok(()) => {}
                    Err(Refusal::NoDocument) => {
                        refused.no_document += 1;
                        continue;
                    }
                    Err(Refusal::PaymentOnly) => {
                        refused.payment_only += 1;
                        continue;
                    }
                    Err(Refusal::Excluded) => {
                        refused.excluded += 1;
                        continue;
                    }
                    Err(Refusal::NoRule) => {
                        continue;
                    }
                }
                match row.activity {
                    Some(code) => {
                        let sum = attributed.entry(kind_of(books.profile, &code, row.date)).or_insert(0);
                        *sum = sum.saturating_add(lumas.get());
                    }
                    None => {
                        shared = shared.saturating_add(lumas.get());
                        shared_rows += 1;
                    }
                }
            }
        }
    }
    // Общие расходы — по весу дохода строки в обороте всех видов (ст. 260,
    // ч. 3). Оборота нет — делить не по чему: у ИП с одной строкой
    // с вычетом всё идёт в неё и переносится, а с несколькими — к бухгалтеру.
    let turnover: i64 = income.values().fold(unassigned_lumas, |sum, lumas| sum.saturating_add(*lumas));
    let mut shared_by_kind: BTreeMap<IncomeKind, i64> = BTreeMap::new();
    if shared > 0 {
        if turnover > 0 {
            for (kind, lumas) in &income {
                let part = i128::from(shared) * i128::from(*lumas) / i128::from(turnover);
                shared_by_kind.insert(*kind, i64::try_from(part).unwrap_or(i64::MAX));
            }
        } else {
            let deductible: Vec<IncomeKind> = profile_kinds(books.profile, quarter.first_day())
                .into_iter()
                .filter(|kind| quarter_rate(*kind, quarter).is_some_and(|rate| rate.deduction.is_some()))
                .collect();
            match deductible.as_slice() {
                [only] => {
                    shared_by_kind.insert(*only, shared);
                }
                // Кодов нет — строку не узнать; а у ИП только со строками без
                // вычета общим расходам по закону идти некуда.
                [] => {
                    if books.profile.is_none_or(|data| data.activities.is_empty()) {
                        gaps.no_codes = true;
                    }
                }
                _ => gaps.unallocated = shared_rows,
            }
        }
    }
    let mut lines = Vec::new();
    for kind in IncomeKind::ALL {
        let income_lumas = income.get(&kind).copied().unwrap_or(0);
        let attributed_lumas = attributed.get(&kind).copied().unwrap_or(0);
        let shared_lumas = shared_by_kind.get(&kind).copied().unwrap_or(0);
        let carry = carry_in.get(&kind).copied().unwrap_or(0);
        if income_lumas == 0 && attributed_lumas == 0 && shared_lumas == 0 && carry == 0 {
            continue;
        }
        let Some(rule) = quarter_rate(kind, quarter) else {
            if income_lumas > 0 {
                gaps.no_rule.push(kind);
            }
            continue;
        };
        let income_drams = Lumas::new(income_lumas).drams();
        let gross = rule.rate.of(income_drams);
        let (deduction, tax) = match (rule.deduction, rule.floor) {
            (Some(percent), Some(floor_percent)) => {
                let attributed = Lumas::new(attributed_lumas).drams();
                let shared = Lumas::new(shared_lumas).drams();
                let expenses = attributed.saturating_add(shared);
                let current = percent.of(expenses);
                let total = current.saturating_add(carry);
                let floor = floor_percent.of(income_drams);
                // Форма 299-Ն, п. 16–17: налог не ниже предела, а чего
                // не хватило — переносится.
                let (tax, carry_out) = if gross - total < floor {
                    (floor, total - (gross - floor))
                } else {
                    (gross - total, 0)
                };
                let deduction = Deduction {
                    percent,
                    floor_percent,
                    attributed,
                    shared,
                    expenses,
                    current,
                    carry_in: carry,
                    total,
                    floor,
                    carry_out,
                };
                (Some(deduction), tax)
            }
            _ => {
                if income_lumas == 0 {
                    continue;
                }
                (None, gross)
            }
        };
        lines.push(Line {
            kind,
            income: income_drams,
            rate: rule.rate,
            gross,
            deduction,
            tax,
        });
    }
    let tax = lines.iter().fold(0_i64, |sum, line| sum.saturating_add(line.tax));
    let has_deduction = lines.iter().any(|line| line.deduction.is_some());
    QuarterTax {
        quarter,
        rules: rules_version,
        lines,
        tax,
        months,
        gaps,
        refused,
        regime: year.regime,
        year_turnover: year.turnover.drams(),
        threshold: year.threshold,
        carry_unknown: carry_unknown && has_deduction,
        excluded_codes,
    }
}

#[cfg(test)]
mod tests;
