//! Налог с оборота (задача 19): кто не может быть его плательщиком, какой
//! строкой таблицы облагается доход и налог квартала.
//!
//! Всё ниже — прочтение агента, сверенное ответами бухгалтера-агента
//! 2026-10-19 и 2026-11-01 (FEATURE_TASKS.md, «PDF против текста закона»
//! и «Сверка 2026-11-01»): по решению человека 2026-10-19 оно идёт в код
//! словами «сверьте с бухгалтером», пока расчёт не сверят настоящие
//! бухгалтеры (задача 82). Формулы сошлись с примерами ՊԵԿ и бухгалтеров
//! (`tests/cases/`) и с формулами калькулятора ՊԵԿ — разница только в лумах.
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
//!   бухгалтера 2026-10-19, вопрос 1.4). У строки без дохода вес — ноль:
//!   если доход только у строк без вычета, общий расход пропадает, и окно
//!   это говорит (`Refused::no_base`), а отнесённый к строке — переносится.
//!   Прямых расходов и товара ст. 260, ч. 3 не делит: их ведут отдельно
//!   (там же), и прямой расход без вида идёт в единственную строку кодов
//!   профиля целиком, а из нескольких её не узнать — расчёт неполон
//!   ([`Gaps::undirected`]). Рядом с доходом со своей строкой прямой без вида
//!   может быть и расходом этого дохода — аренды, роялти, продажи имущества, —
//!   а тот в строку с вычетом не идёт (ст. 260, ч. 1 и 3; ревью задачи 79):
//!   строку не угадать, и такой расход — общий, как до задачи 79. А расход
//!   самого такого дохода человек относит к его строке (задача 95,
//!   `ledger::OperationData::income_kind`): он не вычитается и не делится
//!   (ст. 260, ч. 1 и 3), и окно его называет ([`Refused::income_row`]). Расход,
//!   о котором не сказано, прямой ли он (`ledger::Cost`: записи до задачи 79),
//!   считается, как прежде: без вида — общий. Доход со своей строкой весит
//!   и у ИП с одним кодом, поэтому окно говорит, что доля общих ушла на него
//!   (`Refused::split`), а форма спрашивает вид и при одном коде и предлагает
//!   рядом с кодами строки таких доходов квартала ([`expense_rows`]);
//! - прямые расходы строки (x.4.1) — целыми драмами, а сбытовые
//!   и административные (x.4.2) — остаток x.4: форма складывает их в x.4
//!   (приказ 299-Ն, прил. 1, п. 11). x.4 считается, как прежде, — расходы
//!   с видом и без вида, каждые целыми драмами, — и данные до задачи 79 не
//!   меняют ни драма, а отметка «прямой» у расхода без вида x.4 не меняет вовсе.
//!   Не у всех расходов строки известно, прямые ли они в этой строке, — деления
//!   нет ([`Deduction::direct`]);
//! - каждая строка формы — целые драмы, лумы отбрасываются
//!   ([`Percent::of`]).
//!
//! Строку дохода решает вид деятельности ([`kind_of`]): у кода профиля —
//! строка, которую человек выбрал сам, иначе по правилам — высокие
//! технологии по списку правительства, остальное — прочая деятельность.
//!
//! Но закон ставит строку и по виду самого дохода: аренда, проценты, роялти —
//! строка 5, отчуждение активов — строка 9, какой бы код ни был у ИП (ст. 258,
//! ч. 1; объект — ст. 256, ч. 1). Поэтому строка, выбранная у дохода
//! ([`IncomeKind::OF_INCOME`]), важнее строки кода ([`chosen_kind`]). Кода такому
//! доходу не нужно, и режим он не решает: это не деятельность по коду профиля.
//! А в оборот всех видов — для порога и для долей общих расходов (ст. 260,
//! ч. 3) — идёт, как любой доход: база — вся сумма продажи (ст. 257, ч. 1;
//! ст. 4, ч. 1, п. 50), вычета нет (ст. 260, ч. 1). Где граница между услугой
//! по коду и роялти — например, у лицензии своей программы (ст. 4, ч. 1,
//! п. 34), — решает человек с бухгалтером, и окно это говорит. Нижние границы
//! дохода — от кадастровой стоимости (ст. 109, ч. 1) и у ввезённого автомобиля
//! (ч. 1.2) — программа не считает: первая говорит о базе налога на прибыль,
//! а ст. 257, ч. 2 отсылает к разделу целиком, — и окно советует сверить. День
//! дохода — у человека: аренда — по кварталам (ст. 40, ч. 2), недвижимость —
//! день регистрации права (ст. 38, ч. 2), и окно это говорит. Всё — по закону,
//! сверено по тексту arlis 230455 2026-11-01.
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
//! # Общепит
//!
//! Строки общепита программа считает по общему правилу, а двух вещей не
//! спрашивает (решение 2026-11-02, задача 71: аудитория — IT и услуги, а строка
//! раздела 8 формы нужна только прошедшему году). Кто работал только вне
//! Еревана, до 2026 года считал по прежней редакции кодекса (ՀՕ-285-Ն, ст. 16,
//! ч. 6), и окно это говорит ([`QuarterTax::place_unknown`]). Кто подал
//! заявление об общепите (ст. 254, ч. 6, 7), считает только по строкам 6 и 7
//! (ст. 258, ч. 8) и остаётся плательщиком сверх порога, а программа видит
//! заявление только по строке 7 у кода ([`chosen_kind`]), и окно говорит и это
//! ([`QuarterTax::declaration_unknown`]). Квартал от этих слов не перестаёт
//! закрываться: расчёт ресторана в Ереване верен, а заявивший сверх порога
//! и так уходит к бухгалтеру.
//!
//! # Перенос и закрытый квартал
//!
//! Перенос идёт цепочкой от первого квартала с записями — или от последнего
//! закрытого (`tax::closing`): закрытый квартал отдаёт перенос из своего
//! снимка, а не пересчитанный (Правило 2). Перенос из расчётов до первой
//! записи человек указывает сам (`tax::opening`), а пока не указал, он
//! неизвестен, и окно говорит это на всей цепочке
//! ([`QuarterTax::carry_unknown`]). Закрытый квартал, чей расчёт разошёлся со
//! снимком, находит [`drifted`], а закрытые кварталы периода с их расчётом
//! даёт выгрузке [`closed_in`].

use std::collections::{BTreeMap, BTreeSet};
use std::ops::RangeBounds;

use serde::{Deserialize, Serialize};

use super::closing::Closing;
use super::opening::{self, Opening};
use super::rules::{self, Exclusion, Percent, Rate, Rule, Version};
use crate::i18n::Key;
use crate::model::activity::{ActivityCode, IncomeKind};
use crate::model::date::{Date, Quarter};
use crate::model::document::Document;
use crate::model::ledger::{self, Cost, Flow, Nature, Operation, Proof, Row};
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

/// Строка налога дохода, которому в день `on` выбрали строку `kind` (шапка
/// модуля). Кто подал заявление об общепите, считает только по строкам 6 и 7
/// (ст. 258, ч. 8): прочий его доход, в том числе от отчуждения активов, —
/// строка 7, а не 5 и не 9. Что заявление подано, программа знает только по
/// строке 7 у кода профиля.
pub fn chosen_kind(profile: Option<&ProfileData>, kind: IncomeKind, on: Date) -> IncomeKind {
    if profile_kinds(profile, on).contains(&IncomeKind::CateringOther) {
        IncomeKind::CateringOther
    } else {
        kind
    }
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

/// Слова строки налога: в окне, в числах отчёта (`tax::report`) и в их
/// тексте для бухгалтера.
pub const fn kind_key(kind: IncomeKind) -> Key {
    match kind {
        IncomeKind::Trade => Key::KindTrade,
        IncomeKind::Recyclables => Key::KindRecyclables,
        IncomeKind::Newspapers => Key::KindNewspapers,
        IncomeKind::Production => Key::KindProduction,
        IncomeKind::Rent => Key::KindRent,
        IncomeKind::Catering => Key::KindCatering,
        IncomeKind::CateringOther => Key::KindCateringOther,
        IncomeKind::HighTech => Key::KindHighTech,
        IncomeKind::Assets => Key::KindAssets,
        IncomeKind::Other => Key::KindOther,
    }
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
    /// Только платёж банка, а расход — обычный: платежа хватает только
    /// зарплате и услугам банка (НК, ст. 55, ч. 12).
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
    /// Расходы без вида деятельности, пришедшиеся на строку: доля общих
    /// административных и сбытовых — по весу дохода строки в обороте всех
    /// видов (ст. 260, ч. 3), а прямые — целиком, у ИП с одной строкой кодов.
    pub shared: i64,
    /// Прямые расходы (x.4.1) — целые драмы; сбытовые и административные
    /// (x.4.2) — остаток [`Self::expenses`]: форма складывает их в x.4
    /// (приказ 299-Ն, прил. 1, п. 11). `None` — не у всех расходов строки
    /// сказано, прямые ли они, и делит человек (`tax::report`). В снимке
    /// закрытого квартала — с версии формата 18; у прежних снимков — `None`.
    #[serde(default)]
    pub direct: Option<i64>,
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
    /// Прямых расходов без вида деятельности, когда коды профиля облагаются
    /// по разным строкам: прямой расход идёт в строку своего вида целиком
    /// (ст. 260, ч. 3), а в какую — без кода не узнать.
    pub undirected: usize,
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
    /// Общие расходы квартала, где доход есть только у строк без вычета:
    /// вес строк с вычетом — ноль, их доля — ноль (ст. 260, ч. 3).
    pub no_base: usize,
    /// Общие расходы рядом с доходом со своей строкой: в вычет вошла только
    /// доля строк с вычетом (ст. 260, ч. 3). Так делятся сбытовые
    /// и административные; прямой без вида и расход, о котором не сказано,
    /// прямой ли он, здесь тоже — строку прямого не угадать, — и окно просит
    /// указать вид или строку дохода (шапка модуля).
    pub split: usize,
    /// Расходы дохода со своей строкой (задача 95): у строк аренды и продажи
    /// имущества вычета нет (ст. 260, ч. 1), а в строку с вычетом расход
    /// другого вида не идёт ни целиком, ни долей (ст. 260, ч. 3).
    pub income_row: usize,
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
    /// Перенос из расчётов до первой записи в программе неизвестен: цепочка
    /// идёт от этого квартала без снимка и без указанного переноса, а в этом
    /// квартале есть строка, у которой он мог быть (`tax::opening`).
    pub carry_unknown: Option<Quarter>,
    /// Коды профиля, которые закрывают налог с оборота, и с какого дня:
    /// ведёт ли их человек, программа не знает — окно говорит словами.
    pub excluded_codes: Vec<(ActivityCode, Date)>,
    /// Квартал кончился до дня регистрации ИП: человек в нём ещё не был ИП.
    /// `None` — был, или день регистрации неизвестен (Правило 6: неизвестное
    /// — не «ещё не был»).
    pub not_yet: Option<NotYet>,
}

/// Квартал до регистрации ИП — по образцу года до неё
/// (`annual::Status::NotYet`): расчёта за такой квартал нет, и сроки его
/// не знают (`tax::deadlines`), поэтому закрывать его нечего
/// ([`QuarterTax::closable`]) и чисел для формы у него нет (`tax::report`).
/// Квартал регистрации — обычный: в нём ИП уже был.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct NotYet {
    /// День регистрации из профиля.
    pub registered: Date,
    /// Строк книги в квартале — доходов и расходов. Запись до регистрации —
    /// скорее опечатка в дне регистрации или в дате записи, и окно об этом
    /// говорит: расчёт молча обошёл бы её стороной.
    pub records: usize,
}

impl QuarterTax {
    /// Посчитан ли налог целиком: у всего дохода и расхода есть курс,
    /// строка и правило, и общие расходы поделены.
    pub fn complete(&self) -> bool {
        self.gaps.is_empty()
    }

    /// Налог квартала, если его можно назвать: расчёт полон, и весь квартал —
    /// на налоге с оборота. Иначе это «нет значения»: неполный расчёт даёт
    /// часть суммы или ноль, а квартал с уходом с режима считает бухгалтер
    /// (Правило 6).
    pub fn known(&self) -> Option<i64> {
        (self.complete() && self.regime == Regime::Turnover).then_some(self.tax)
    }

    /// Есть ли доход строки, чьё правило в этом квартале зависело от места
    /// деятельности (шапка модуля): запись `turnover_place` на первый день
    /// квартала.
    pub fn place_unknown(&self) -> bool {
        rules::on(rules::armenia().turnover_place(), self.quarter.first_day()).is_some_and(|rule| {
            self.lines
                .iter()
                .any(|line| line.income > 0 && rule.kinds.contains(&line.kind))
        })
    }

    /// Есть ли рядом с общепитом доход строки, которой у подавшего заявление
    /// быть не может: весь его прочий доход — строка 7 (шапка модуля).
    pub fn declaration_unknown(&self) -> bool {
        let earned = || self.lines.iter().filter(|line| line.income > 0);
        earned().any(|line| line.kind == IncomeKind::Catering)
            && earned().any(|line| !matches!(line.kind, IncomeKind::Catering | IncomeKind::CateringOther))
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
    /// бухгалтер. Раньше правил налога закрывать нечего: такой снимок чтение
    /// считает порчей (`tax::closing`). И до регистрации ИП ([`NotYet`]):
    /// снимок закрепил бы расчёт квартала, которого у ИП не было.
    pub fn closable(&self, today: Date) -> bool {
        first_quarter().is_some_and(|first| self.quarter >= first)
            && self.quarter.last_day() < today
            && self.complete()
            && self.regime == Regime::Turnover
            && self.not_yet.is_none()
    }

    /// Расходится ли расчёт со снимком закрытого квартала: данные квартала
    /// поправили после закрытия — нужен уточнённый расчёт (299-Ն, п. 1).
    /// Сверяется то, что идёт в форму, — иначе смена вида у расхода просила
    /// бы подать тот же расчёт ещё раз ([`same_in_form`]). Уход с режима —
    /// расхождение и при тех же строках: закрывают только квартал на налоге
    /// с оборота, а новый доход после ухода в строки не идёт.
    pub fn differs_from(&self, closing: &Closing) -> bool {
        let data = closing.data();
        self.regime != Regime::Turnover
            || data.tax != self.tax
            || data.lines.len() != self.lines.len()
            || !data.lines.iter().zip(&self.lines).all(|(filed, now)| same_in_form(filed, now))
    }

    /// Разошёлся ли с поданным расчёт, который можно с ним сравнить: полный
    /// ([`Self::complete`]) и отличный от снимка ([`Self::differs_from`]).
    /// Неполный не сравнивается — почему, сказано у [`drifted`].
    pub fn drifted_from(&self, closing: &Closing) -> bool {
        self.complete() && self.differs_from(closing)
    }
}

/// Одинаковы ли строки в форме расчёта. Расходы, отнесённые к строке,
/// и общие форма не различает: в ней их сумма, x.4. Прямые (x.4.1)
/// сравниваются, когда деление знают оба: снимок до задачи 79 его не
/// хранит, и что человек вписал тогда, программе не известно.
fn same_in_form(one: &Line, other: &Line) -> bool {
    let split = |line: &Line| line.deduction.and_then(|deduction| deduction.direct);
    let both = split(one).is_some() && split(other).is_some();
    let form = |line: &Line| Line {
        deduction: line.deduction.map(|deduction| Deduction {
            attributed: 0,
            shared: 0,
            direct: deduction.direct.filter(|_| both),
            ..deduction
        }),
        ..line.clone()
    };
    form(one) == form(other)
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
/// (расчёт отбирает живые сам), профиль, снимки закрытых кварталов и перенос
/// с кварталов до программы, если его указали.
#[derive(Clone, Copy, Debug)]
pub struct Books<'a> {
    pub documents: &'a [Document],
    pub operations: &'a [Operation],
    pub profile: Option<&'a ProfileData>,
    pub closings: &'a [Closing],
    pub openings: &'a [Opening],
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

/// Спросить ли у расхода дня `on` вид деятельности, хотя код один: в его
/// квартале есть доход со своей строкой, а у кодов — вычет ([`expense_rows`]).
/// Расход без вида тогда общий, и его доля уходит на строку без вычета
/// (ст. 260, ч. 3), — а ведёт ли человек сбытовой отдельно и для своего ли кода
/// прямой, программа не знает: прямой может быть и расходом самого этого
/// дохода (шапка модуля).
pub fn expense_activity_needed(
    documents: &[Document],
    operations: &[Operation],
    profile: Option<&ProfileData>,
    on: Date,
) -> bool {
    !expense_rows(documents, operations, profile, on).is_empty()
}

/// Строки доходов со своей строкой ([`IncomeKind::OF_INCOME`]) в квартале дня
/// `on`, к которым форма расхода даёт отнести расход рядом с кодами (задача 95):
/// агенту за сдачу квартиры — к аренде. Только когда у строк кодов профиля
/// есть вычет: иначе расходы не вычитаются вовсе, и отделять нечего. Строки —
/// у доходов: расход, уже отнесённый к строке, дохода в квартале не делает.
pub fn expense_rows(
    documents: &[Document],
    operations: &[Operation],
    profile: Option<&ProfileData>,
    on: Date,
) -> Vec<IncomeKind> {
    let deducting = profile_kinds(profile, on)
        .into_iter()
        .any(|kind| rate(kind, on).is_some_and(|rule| rule.deduction.is_some()));
    if !deducting {
        return Vec::new();
    }
    let quarter = on.quarter();
    let earned: BTreeSet<IncomeKind> = ledger::between(documents, operations, quarter.first_day(), quarter.last_day())
        .iter()
        .filter(|row| row.flow == Flow::Income)
        .filter_map(|row| row.income_kind)
        .collect();
    earned.into_iter().collect()
}

/// Спросить ли у расхода дня `on` вида `nature` с документом продавца `proof`,
/// прямой ли он (`ledger::Cost`): такой расход идёт в вычет по правилам этого
/// дня ([`deductible`]; документ ещё не выбран — смотрится только вид),
/// а среди строк кодов профиля есть строка с вычетом — или кодов нет, и строку
/// не узнать (Правило 6). Иначе полей x.4.1 и x.4.2 у расхода нет: в них
/// только вычитаемые расходы, и только у строк с вычетом.
pub fn cost_needed(profile: Option<&ProfileData>, proof: Option<Proof>, nature: Nature, on: Date) -> bool {
    let counted = match proof {
        Some(proof) => deductible(proof, nature, on).is_ok(),
        None => rules::on(rules::armenia().turnover_expenses(), on).is_some_and(|rule| !rule.excluded.contains(&nature)),
    };
    let kinds = profile_kinds(profile, on);
    counted
        && (kinds.is_empty()
            || kinds
                .into_iter()
                .any(|kind| rate(kind, on).is_some_and(|rule| rule.deduction.is_some())))
}

/// Нужен ли прямому расходу дня `on` вид деятельности: коды профиля
/// облагаются по разным строкам, и в какую строку прямой расход идёт целиком,
/// без кода не узнать ([`Gaps::undirected`]).
pub fn direct_activity_needed(profile: Option<&ProfileData>, on: Date) -> bool {
    profile_kinds(profile, on).len() > 1
}

/// Налог квартала `quarter` — с переносом цепочкой от последнего закрытого
/// квартала или от первого квартала с записями (шапка модуля).
pub fn quarter(books: Books<'_>, quarter: Quarter) -> QuarterTax {
    let start = chain_start(books, earliest(books), quarter);
    let span = Span::new(books, chain_from(start.0), quarter.last_day());
    chain(books, &span, start, quarter)
}

/// Первый по календарю закрытый квартал, чей расчёт разошёлся со снимком
/// ([`QuarterTax::drifted_from`]), — расчёт и снимок. С первого, потому что
/// уточнённый расчёт поправит перенос на следующие и может свести или
/// развести их.
///
/// Неполный расчёт сюда не идёт: с поданным сравнивать ещё нечего (Правило 6).
/// Второй код в профиле делает старые доходы без вида ничьими, испорченная
/// запись выпадает из книги, — и расчёт «разошёлся» бы у всех закрытых
/// кварталов, хотя поданное верно. Цена — настоящая правка рядом с таким
/// пропуском видна только на экране самого квартала, пока пропуск не закрыт.
///
/// Квартал, не кончившийся на день `today`, не сверяется: такой не закрыть,
/// а снимок 9999 года из чужой копии растянул бы цепочку на десятки тысяч
/// кварталов на каждое изменение данных (Правило 13).
///
/// Считается всё, что даёт [`closed_in`]: и без расхождения проходятся все
/// закрытые кварталы, а это и есть обычный случай.
pub fn drifted<'a>(books: Books<'a>, today: Date) -> Option<(QuarterTax, &'a Closing)> {
    closed_in(books, today, ..)
        .into_iter()
        .find(|(tax, closing)| tax.drifted_from(closing))
}

/// Закрытые кварталы в пределах `within`, кончившиеся до дня `today`, —
/// по календарю, у каждого расчёт и снимок. Выгрузка (задача 83) говорит
/// о каждом, чей расчёт разошёлся с поданным ([`QuarterTax::drifted_from`]),
/// и кладёт все в свой лист: бухгалтер видит файл, а не окно.
///
/// Считает то же, что [`quarter`] каждого закрытого квартала, но одним
/// проходом по книге: цепочка каждого идёт от предыдущего снимка, и кварталы
/// считаются по разу на все. Отдельные [`quarter`] на каждое изменение данных
/// прошли бы всю книгу по разу на квартал (Правило 1, замер — `drift_timing`).
/// Квартал не кончился — не сверяется (шапка [`drifted`]).
pub fn closed_in<'a>(
    books: Books<'a>,
    today: Date,
    within: impl RangeBounds<Quarter>,
) -> Vec<(QuarterTax, &'a Closing)> {
    let closed: BTreeSet<Quarter> = books
        .closings
        .iter()
        .filter(|closing| !closing.is_deleted())
        .map(Closing::quarter)
        .filter(|quarter| quarter.last_day() < today && within.contains(quarter))
        .collect();
    let (Some(&first), Some(&last)) = (closed.first(), closed.last()) else {
        return Vec::new();
    };
    let earliest = earliest(books);
    let span = Span::new(books, chain_from(chain_start(books, earliest, first).0), last.last_day());
    closed
        .into_iter()
        .filter_map(|quarter| {
            let closing = Closing::current(books.closings, quarter)?;
            Some((chain(books, &span, chain_start(books, earliest, quarter), quarter), closing))
        })
        .collect()
}

/// Откуда идёт цепочка переноса: квартал, перенос на него и строки, у которых
/// он неизвестен ([`QuarterTax::carry_unknown`]).
type ChainStart = (Quarter, BTreeMap<IncomeKind, i64>, &'static [IncomeKind]);

/// Начало цепочки квартала `quarter`: следующий за последним закрытым или
/// квартал с указанным переносом — что из них позже, а на одном квартале
/// указанное: человек взял его из поданного расчёта, а снимок мог закрыть
/// квартал, ничего о переносе не зная. Так запись на экране квартала всегда
/// действует. Иначе — первый квартал с записями (`earliest`, в пределах
/// правил налога).
fn chain_start(books: Books<'_>, earliest: Option<Quarter>, quarter: Quarter) -> ChainStart {
    let first = first_quarter();
    let earliest = earliest.map(|earliest| first.map_or(earliest, |first| earliest.max(first)));
    let closed = books
        .closings
        .iter()
        .filter(|closing| !closing.is_deleted() && closing.quarter() < quarter)
        .max_by_key(|closing| (closing.quarter(), closing.modified()));
    let opening = books
        .openings
        .iter()
        .filter(|opening| !opening.is_deleted() && opening.quarter() <= quarter)
        .max_by_key(|opening| (opening.quarter(), opening.modified()));
    // Следующий за закрытым есть: закрытый раньше `quarter`.
    let after = closed.map(|closing| (closing.quarter().next().unwrap_or(quarter), closing));
    let filed = books.closings.iter().any(|closing| {
        !closing.is_deleted() && closing.quarter() < quarter && closing.data().lines.iter().any(|line| line.deduction.is_some())
    });
    match (opening, after, earliest) {
        (Some(opening), after, _) if after.is_none_or(|(start, _)| opening.quarter() >= start) => {
            (opening.quarter(), opening.carry().clone(), &[])
        }
        // Снимок без строк с вычетом о переносе не знает ничего: так закрывают
        // кварталы до первой записи, о чём просит и напоминание о сроке. Пока
        // ни снимок раньше, ни ответ переноса не знают, вопрос остаётся — и он
        // о первом квартале с записями, а не о том, что за пустым снимком.
        (opening, Some((start, closing)), earliest) => {
            if opening.is_some() || filed {
                (start, closing.carry_out(), &[])
            } else {
                let start = earliest.map_or(start, |earliest| earliest.max(start));
                (start, closing.carry_out(), unknown_kinds(books, start))
            }
        }
        (_, None, Some(earliest)) => (earliest, BTreeMap::new(), unknown_kinds(books, earliest)),
        (_, None, None) => (quarter, BTreeMap::new(), &[]),
    }
}

/// Строки, у которых на начало квартала `start` мог быть неизвестный перенос.
/// Кто зарегистрирован в этом квартале или позже, расчётов до него не подавал.
fn unknown_kinds(books: Books<'_>, start: Quarter) -> &'static [IncomeKind] {
    let registered = books.profile.and_then(|profile| profile.person.registered);
    if registered.is_some_and(|day| day.quarter() >= start) {
        return &[];
    }
    opening::kinds(start)
}

/// С какого дня цепочке от квартала `start` нужны строки книги: с прошлого
/// года этого квартала — по нему порог (`year`).
fn chain_from(start: Quarter) -> Date {
    start
        .year()
        .checked_sub(1)
        .and_then(|year| Date::new(year, 1, 1))
        .unwrap_or_else(|| start.first_day())
}

/// Налог квартала `quarter` цепочкой от `start` по куску книги `span`.
fn chain(books: Books<'_>, span: &Span<'_>, start: ChainStart, quarter: Quarter) -> QuarterTax {
    let (start, mut carry, mut unknown) = start;
    let mut incomplete = None;
    let mut current = start;
    while current < quarter {
        let tax = compute(books, span, current, &carry);
        if incomplete.is_none() && !tax.complete() && tax.regime == Regime::Turnover {
            incomplete = Some(current);
        }
        carry = tax.carry_out();
        // С уходом с режима цепочка кончилась: спрашивать больше не о чем.
        if tax.regime != Regime::Turnover {
            unknown = &[];
        }
        match current.next() {
            Some(next) => current = next,
            None => break,
        }
    }
    let mut tax = compute(books, span, quarter, &carry);
    // Перенос из неполного квартала — тоже неполный (ст. 258, ч. 7); важен
    // он только строкам с вычетом.
    if tax.lines.iter().any(|line| line.deduction.is_some()) {
        tax.gaps.previous = incomplete;
    }
    // Неизвестный перенос тянется по всей цепочке, а не только в её первом
    // квартале: важен он только строкам, у которых мог быть.
    if tax.lines.iter().any(|line| line.deduction.is_some() && unknown.contains(&line.kind)) {
        tax.carry_unknown = Some(start);
    }
    tax
}

/// Режим года `year` к его концу — для годовых платежей (`tax::annual`):
/// ушёл ли ИП с налога с оборота и с какого дня. Считается так же, как
/// у квартала ([`quarter`]): по порогу прошлого и этого года и по доходу
/// с кодом, закрывающим налог с оборота. `span` — строки книги с начала
/// прошлого года по конец этого. Ошибка — только переполнение от
/// испорченных данных.
pub(crate) fn year_regime(span: &Span<'_>, profile: Option<&ProfileData>, year: u16) -> Result<Regime, MoneyError> {
    match Quarter::new(year, 4) {
        Some(quarter) => Ok(self::year(span, profile, quarter)?.regime),
        None => Ok(Regime::Turnover),
    }
}

/// Строки книги на всю цепочку: один проход по документам и операциям,
/// а годы и кварталы берут свои куски по дню. Три прохода на квартал по всем
/// записям за все годы стоили цепочке в пять лет без закрытых кварталов
/// 19,5 мс на компьютере (замер задачи 19, тест `chain_timing`), а карточка
/// и экран считают две-три цепочки на каждое изменение данных — на телефоне
/// это не кадр, а четверть секунды (Правило 1). Годовые платежи берут его
/// же (`tax::annual`).
pub(crate) struct Span<'a> {
    /// По дню, в день — по идентификатору: так порог делит строки (`year`).
    rows: Vec<Row<'a>>,
}

impl<'a> Span<'a> {
    pub(crate) fn new(books: Books<'a>, from: Date, until: Date) -> Self {
        let mut rows = ledger::between(books.documents, books.operations, from, until);
        rows.sort_by_key(|row| (row.date, row.id()));
        Self { rows }
    }

    /// Пустой кусок: дней за пределами календаря `Date` в книге нет.
    pub(crate) const fn empty() -> Self {
        Self { rows: Vec::new() }
    }

    /// Строки с дня `from` по день `until` включительно.
    pub(crate) fn between(&self, from: Date, until: Date) -> &[Row<'a>] {
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

/// Строка налога дохода `row`: выбранная у него самого, по виду деятельности
/// у дохода, а без них — единственная строка кодов профиля, как в расчёте
/// квартала (`compute`); `None` — не узнать. Для базы гербового сбора
/// (`tax::annual`): оборот в расчёте — строками, каждая целыми драмами.
pub(crate) fn income_kind(profile: Option<&ProfileData>, row: &Row<'_>) -> Option<IncomeKind> {
    if let Some(kind) = row.income_kind {
        return Some(chosen_kind(profile, kind, row.date));
    }
    match row.activity {
        Some(code) => Some(kind_of(profile, &code, row.date)),
        None => {
            let mut kinds = profile_kinds(profile, row.date).into_iter();
            match (kinds.next(), kinds.next()) {
                (Some(only), None) => Some(only),
                _ => None,
            }
        }
    }
}

/// Код дохода: указанный у дохода, а без него — единственный код профиля.
/// Из нескольких кодов одной строки программа выбрать не может. Им же
/// числа отчёта делят доход по кодам деятельности (`tax::report`): у дохода со
/// своей строкой другого кода программа не знает, а таблица на налог не влияет.
pub(crate) fn income_code(profile: Option<&ProfileData>, row: &Row<'_>) -> Option<ActivityCode> {
    row.activity.or_else(|| match profile.map(|data| data.activities.as_slice()) {
        Some([only]) => Some(*only),
        Some(_) | None => None,
    })
}

/// Код, который решает режим: у дохода со своей строкой — только указанный
/// у него самого: аренда и продажа имущества — не деятельность по единственному
/// коду профиля (шапка модуля).
fn regime_code(profile: Option<&ProfileData>, row: &Row<'_>) -> Option<ActivityCode> {
    if row.income_kind.is_some() { row.activity } else { income_code(profile, row) }
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
            .map(|row| (row.date, row.id(), regime_code(profile, row), row.lumas().ok()))
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
    let rows = span.between(quarter.first_day(), quarter.last_day());
    let not_yet = books
        .profile
        .and_then(|profile| profile.person.registered)
        .filter(|registered| quarter.last_day() < *registered)
        .map(|registered| NotYet {
            registered,
            records: rows.len(),
        });
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
                carry_unknown: None,
                excluded_codes,
                not_yet,
            };
        }
    };
    let left_from = match year.regime {
        Regime::Turnover => None,
        Regime::Left { from, .. } => Some(from),
    };
    let mut gaps = Gaps {
        year_unrated: year.unrated,
        unreadable: books.unreadable,
        ..Gaps::default()
    };
    let mut refused = Refused::default();
    let mut months = [Lumas::ZERO; 3];
    let mut income: BTreeMap<IncomeKind, i64> = BTreeMap::new();
    // Расходы с видом деятельности — лумами по строкам, из них прямые,
    // и строки, где среди таких расходов есть не сказавший, прямой ли он:
    // у этих строк деления x.4 нет.
    let mut attributed: BTreeMap<IncomeKind, i64> = BTreeMap::new();
    let mut attributed_direct: BTreeMap<IncomeKind, i64> = BTreeMap::new();
    let mut uncosted: BTreeSet<IncomeKind> = BTreeSet::new();
    // Прямые без вида — целиком в единственную строку кодов.
    let mut whole: BTreeMap<IncomeKind, i64> = BTreeMap::new();
    let mut unassigned_lumas: i64 = 0;
    let mut shared: i64 = 0;
    let mut shared_rows = 0;
    // Среди общих есть такой, у которого не узнать, прямой ли он в строке:
    // о нём не сказано, или это прямой без вида рядом с доходом со своей
    // строкой.
    let mut shared_unsplit = false;
    let mut chosen_income = false;
    // В квартале — доход со своей строкой: прямой без вида рядом с ним может
    // быть и его расходом (шапка модуля).
    let beside = rows.iter().any(|row| row.flow == Flow::Income && row.income_kind.is_some());
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
                chosen_income |= row.income_kind.is_some();
                // Без вида — строка по единственной строке кодов профиля;
                // но если среди кодов есть закрывающий налог с оборота, код
                // дохода решает режим, и без него расчёт неполон. Доходу со
                // своей строкой код не нужен.
                let (kind, unknown) = match (row.income_kind, row.activity) {
                    (Some(kind), _) => (Some(chosen_kind(books.profile, kind, row.date)), false),
                    (None, Some(code)) => (Some(kind_of(books.profile, &code, row.date)), false),
                    (None, None) => {
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
                let verdict = deductible(proof, nature, row.date);
                // Расход дохода со своей строкой (задача 95): у неё вычета нет,
                // а в строку с вычетом он не идёт ни целиком, ни долей (ст. 260,
                // ч. 1 и 3) — с любым документом. Без правил на этот день окно
                // молчит о нём, как о прочих.
                if row.income_kind.is_some() && verdict != Err(Refusal::NoRule) {
                    refused.income_row += 1;
                    continue;
                }
                match verdict {
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
                let add = |sums: &mut BTreeMap<IncomeKind, i64>, kind: IncomeKind| {
                    let sum = sums.entry(kind).or_insert(0);
                    *sum = sum.saturating_add(lumas.get());
                };
                let cost = row.cost();
                match (row.activity, cost) {
                    (Some(code), cost) => {
                        let kind = kind_of(books.profile, &code, row.date);
                        add(&mut attributed, kind);
                        match cost {
                            Some(Cost::Direct) => add(&mut attributed_direct, kind),
                            Some(Cost::Overhead) => {}
                            None => {
                                uncosted.insert(kind);
                            }
                        }
                    }
                    // Прямой расход по долям не делят (ст. 260, ч. 3): без вида
                    // он идёт в единственную строку кодов профиля целиком,
                    // а из нескольких её не узнать. Рядом с доходом со своей
                    // строкой он может быть и расходом этого дохода, а тот
                    // в строку с вычетом не идёт: строку не угадать, и расход
                    // — общий, как до задачи 79, без деления x.4.
                    (None, Some(Cost::Direct)) => {
                        let mut kinds = profile_kinds(books.profile, row.date).into_iter();
                        match (kinds.next(), kinds.next()) {
                            (Some(_), Some(_)) => gaps.undirected += 1,
                            _ if beside => {
                                shared = shared.saturating_add(lumas.get());
                                shared_rows += 1;
                                shared_unsplit = true;
                            }
                            (Some(only), None) => add(&mut whole, only),
                            (None, _) => gaps.no_codes = true,
                        }
                    }
                    (None, Some(Cost::Overhead) | None) => {
                        shared = shared.saturating_add(lumas.get());
                        shared_rows += 1;
                        shared_unsplit |= cost.is_none();
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
        let deductible: Vec<IncomeKind> = profile_kinds(books.profile, quarter.first_day())
            .into_iter()
            .filter(|kind| quarter_rate(*kind, quarter).is_some_and(|rate| rate.deduction.is_some()))
            .collect();
        // Кодов нет — строку расхода не узнать, даже если доход со своей
        // строкой код и не спрашивает.
        if books.profile.is_none_or(|data| data.activities.is_empty()) {
            gaps.no_codes = true;
        }
        if turnover > 0 {
            for (kind, lumas) in &income {
                let part = i128::from(shared) * i128::from(*lumas) / i128::from(turnover);
                shared_by_kind.insert(*kind, i64::try_from(part).unwrap_or(i64::MAX));
            }
            // Весь доход квартала — у строк без вычета, а строка с вычетом
            // у ИП есть: вычета нет и переносить нечего. Перенос ст. 258, ч. 7
            // — у расхода, отнесённого к строке видом деятельности; окно так
            // и говорит (ответ бухгалтера-агента 2026-11-01). У ИП только со
            // строками без вычета сказать нечего: расходы ему и не вычитаются.
            if !deductible.is_empty()
                && !income.is_empty()
                && income
                    .keys()
                    .all(|kind| quarter_rate(*kind, quarter).is_none_or(|rate| rate.deduction.is_none()))
            {
                refused.no_base = shared_rows;
            } else if !deductible.is_empty() && chosen_income {
                refused.split = shared_rows;
            }
        } else {
            match deductible.as_slice() {
                [only] => {
                    shared_by_kind.insert(*only, shared);
                }
                // У ИП только со строками без вычета общим расходам по закону
                // идти некуда.
                [] => {}
                _ => gaps.unallocated = shared_rows,
            }
        }
    }
    let mut lines = Vec::new();
    for kind in IncomeKind::ALL {
        let income_lumas = income.get(&kind).copied().unwrap_or(0);
        let attributed_lumas = attributed.get(&kind).copied().unwrap_or(0);
        // Без вида: доля общих по весу и прямые целиком — одной суммой, как
        // до задачи 79 шли все расходы без вида: целиком прямой идёт, только
        // когда вес строки и так весь, и отметка «прямой» x.4 не меняет
        // (шапка модуля).
        let share_lumas = shared_by_kind.get(&kind).copied().unwrap_or(0);
        let whole_lumas = whole.get(&kind).copied().unwrap_or(0);
        let shared_lumas = share_lumas.saturating_add(whole_lumas);
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
                // x.4.1 — целыми драмами, x.4.2 — остаток x.4 (шапка модуля).
                // Прямые с видом — часть отнесённых, без вида — часть
                // расходов без вида, и каждая часть доводится до драма, как
                // её сумма в x.4: x.4.1 не больше x.4. `min` — на случай
                // испорченных сумм: снимок с x.4.1 больше x.4 не прочитался
                // бы (`tax::closing`).
                let direct = (!uncosted.contains(&kind) && !(shared_unsplit && share_lumas > 0)).then(|| {
                    let with_code = Lumas::new(attributed_direct.get(&kind).copied().unwrap_or(0)).drams();
                    with_code.saturating_add(Lumas::new(whole_lumas).drams()).min(expenses)
                });
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
                    direct,
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
        carry_unknown: None,
        excluded_codes,
        not_yet,
    }
}

#[cfg(test)]
mod tests;
