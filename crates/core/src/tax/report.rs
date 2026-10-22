//! Числа для расчёта (задача 24): налог с оборота за квартал полями формы
//! расчёта ՊԵԿ — номер поля, что в нём и число, — чтобы человек переписал
//! их в форму в системе ՊԵԿ или отдал бухгалтеру.
//!
//! # Форма
//!
//! Приказ ՊԵԿ 299-Ն, прил. 1, в редакции приказов 1430-Ն (переписал форму:
//! её подают за отчётные периоды после 2025-01-01, п. 2) и 420-Ն (с 10.04.2025:
//! x.4.2 — сбытовые и административные расходы, а x.1 вписывает
//! плательщик) — по закону, сверено по тексту arlis 205434 и 201683
//! 2026-10-22:
//!
//! - разделы 5, 6, 7 и 9 — строки налога с вычетом расходов: торговля,
//!   производство, общепит и прочая деятельность, — по полю на каждое число
//!   расчёта строки (`turnover::Line`, `Deduction`, [`What`]);
//! - строки 11–21 — доход [Ա], ставка [Բ] и налог [Գ] каждой строки налога;
//!   у строки с разделом в [Բ] — «X», а в [Գ] — её x.10 (п. 63);
//! - 22 — итог дохода и налога строк 11–21, 27 — оборот всех видов
//!   деятельности, итог строк 22–26 (п. 64, 69).
//!
//! Номера полей — запись файла правил (`rules::TurnoverForm`), а не код:
//! форма меняется вместе с законом.
//!
//! Чего здесь нет. Раздела 8 и строки 17 — общепита только вне Еревана по
//! правилам до ՀՕ-285-Ն, за периоды до 2026 года (п. 42): такой строки
//! налога программа не знает (задача 71). Строк 23–26 — видов деятельности
//! закона о госпошлине и льгот приграничных сёл: их программа тоже не
//! знает, и строка 27 у неё — итог строки 22. Ставки x.2, x.5 и [Բ] форма
//! ставит сама (п. 8, 20, 32, 51, 62), и вписывать их не нужно.
//!
//! # Откуда числа
//!
//! У закрытого квартала — из снимка (`tax::closing`): это поданный расчёт,
//! и новые данные или правила его чисел не меняют (Правило 2); разошёлся
//! снимок с расчётом по данным сейчас — окно говорит, что нужен уточнённый.
//! У открытого — расчёт (`turnover::quarter`), и только полный и на налоге
//! с оборота: неполные числа в форме — неверный расчёт, а квартал с уходом
//! с режима считает бухгалтер (Правило 6).
//!
//! # x.4.1 и x.4.2
//!
//! Форма делит расходы на прямые (x.4.1: у торговли — стоимость товаров,
//! у остальных — расходы, прямо связанные с деятельностью) и сбытовые
//! с административными (x.4.2; приказ 299-Ն, прил. 1, п. 10, 22, 34, 53),
//! а книга их не различает. Вид деятельности у расхода говорит, к какой
//! строке налога он относится, а не прямой ли он, — и у ИП с одним кодом
//! форма расхода его даже не спрашивает. Поэтому, когда расходы есть, у
//! x.4.1 и x.4.2 числа нет ([`Value::Unknown`]; Правило 6: ноль выглядел бы
//! фактом), и окно просит разделить их сумму x.4 самому. На налог деление
//! не влияет: в вычет идёт сумма, x.4 (п. 11–12). Различать их в книге —
//! задача 79.
//!
//! # Доход по видам деятельности
//!
//! Таблица в конце формы — коды и доли дохода по ним, по убыванию (п. 70):
//! она для статистики и налог не меняет. Доли считает [`activities`] по
//! книге — тем же кодом дохода, что и налог (`turnover::income_code`), —
//! сотыми процента, и в сумме они ровно 100 %: остаток от округления идёт
//! долям с большими остатками. Сколько знаков после точки ждёт форма,
//! приказ не говорит. У ИП с несколькими кодами одной строки налога вид
//! у дохода формы не спрашивают (`turnover::activity_needed`), и долей нет:
//! окно просит вписать их самому. У закрытого квартала таблицы нет вовсе
//! ([`Ready::has_activities`]).

use std::collections::BTreeMap;

use super::closing::Closing;
use super::rules::{self, Percent, SectionFields, TurnoverForm, Version};
use super::turnover::{self, Books, Deduction, Line, QuarterTax, Regime, kind_key};
use crate::i18n::{self, Key, Lang, fill, t};
use crate::model::activity::{ActivityCode, IncomeKind};
use crate::model::date::{Date, Quarter};
use crate::model::ledger::{self, Flow};

/// Что в поле раздела строки с вычетом — по порядку формы.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum What {
    /// Доход квартала (x.1).
    Income,
    /// Ставка (x.2).
    Rate,
    /// Налог по ставке (x.3).
    Gross,
    /// Прямые расходы (x.4.1).
    Direct,
    /// Сбытовые и административные расходы (x.4.2).
    Overhead,
    /// Расходы всего (x.4).
    Expenses,
    /// Процент расходов (x.5).
    Percent,
    /// Вычет за расходы квартала (x.6).
    Current,
    /// Перенос с прошлых кварталов (x.7).
    CarryIn,
    /// Вычет всего (x.8).
    Total,
    /// Нижний предел налога (x.9).
    Floor,
    /// Налог к уплате (x.10).
    Tax,
    /// Перенос на следующие кварталы (x.11).
    CarryOut,
}

impl What {
    /// Все поля раздела — по порядку формы.
    pub const ALL: [What; 13] = [
        What::Income,
        What::Rate,
        What::Gross,
        What::Direct,
        What::Overhead,
        What::Expenses,
        What::Percent,
        What::Current,
        What::CarryIn,
        What::Total,
        What::Floor,
        What::Tax,
        What::CarryOut,
    ];

    /// Слова поля.
    pub const fn key(self) -> Key {
        match self {
            What::Income => Key::ReportIncome,
            What::Rate => Key::ReportRate,
            What::Gross => Key::ReportGross,
            What::Direct => Key::ReportDirect,
            What::Overhead => Key::ReportOverhead,
            What::Expenses => Key::ReportExpenses,
            What::Percent => Key::ReportPercent,
            What::Current => Key::ReportCurrent,
            What::CarryIn => Key::ReportCarryIn,
            What::Total => Key::ReportDeduction,
            What::Floor => Key::ReportFloor,
            What::Tax => Key::ReportTax,
            What::CarryOut => Key::ReportCarryOut,
        }
    }

    /// Номер поля внутри раздела — по записи формы.
    fn number(self, fields: &SectionFields) -> &str {
        match self {
            What::Income => &fields.income,
            What::Rate => &fields.rate,
            What::Gross => &fields.gross,
            What::Direct => &fields.direct,
            What::Overhead => &fields.overhead,
            What::Expenses => &fields.expenses,
            What::Percent => &fields.percent,
            What::Current => &fields.current,
            What::CarryIn => &fields.carry_in,
            What::Total => &fields.total,
            What::Floor => &fields.floor,
            What::Tax => &fields.tax,
            What::CarryOut => &fields.carry_out,
        }
    }

    /// Число строки расчёта в этом поле. Прямые расходы и сбытовые
    /// с административными книга не различает: расходов нет — оба ноль,
    /// есть — числа нет (шапка модуля, «x.4.1 и x.4.2»).
    fn value(self, line: &Line, deduction: Deduction) -> Value {
        match self {
            What::Income => Value::Drams(line.income),
            What::Rate => Value::Percent(line.rate),
            What::Gross => Value::Drams(line.gross),
            What::Direct | What::Overhead if deduction.expenses == 0 => Value::Drams(0),
            What::Direct | What::Overhead => Value::Unknown,
            What::Expenses => Value::Drams(deduction.expenses),
            What::Percent => Value::Percent(deduction.percent),
            What::Current => Value::Drams(deduction.current),
            What::CarryIn => Value::Drams(deduction.carry_in),
            What::Total => Value::Drams(deduction.total),
            What::Floor => Value::Drams(deduction.floor),
            What::Tax => Value::Drams(line.tax),
            What::CarryOut => Value::Drams(deduction.carry_out),
        }
    }
}

/// Число в поле формы.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Value {
    /// Драмы — целые: форму заполняют «в драмах, без лум» (п. 1).
    Drams(i64),
    /// Ставка или процент — их форма ставит сама.
    Percent(Percent),
    /// Числа программа не знает: прямые расходы и сбытовые
    /// с административными (x.4.1 и x.4.2) делит человек (шапка модуля).
    Unknown,
}

impl Value {
    /// Число, как его вписывают в форму: драмы — цифрами, без пробелов
    /// и знака драма, процент — как его пишет приказ, с точкой: `4.5%`.
    /// Запись по правилам языка поле формы могло бы и не принять:
    /// «1 000 000» с неразрывными пробелами или «4,5». `None` — числа нет.
    pub fn plain(self) -> Option<String> {
        match self {
            Value::Drams(drams) => Some(drams.to_string()),
            Value::Percent(percent) => Some(format!("{percent}%")),
            Value::Unknown => None,
        }
    }
}

/// Поле раздела.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Field {
    /// Номер поля: `9.4.1`.
    pub number: String,
    pub what: What,
    pub value: Value,
}

/// Раздел формы — строка налога с вычетом расходов.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Section {
    /// Номер раздела: `9`.
    pub number: String,
    pub kind: IncomeKind,
    /// Поля — по порядку формы.
    pub fields: Vec<Field>,
}

/// Строка сводки: доход [Ա], ставка [Բ] и налог [Գ] строки налога.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Row {
    /// Номер строки: `21`.
    pub number: String,
    pub kind: IncomeKind,
    pub income: i64,
    /// Ставка; `None` — у строки с разделом в [Բ] стоит «X»: ставка —
    /// в разделе (x.2).
    pub rate: Option<Percent>,
    /// Налог — у строки с разделом это её x.10.
    pub tax: i64,
}

/// Строка «Итого» сводки: доход и налог всех строк.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Total {
    pub number: String,
    pub income: i64,
    pub tax: i64,
}

/// Строка оборота всех видов деятельности.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Turnover {
    pub number: String,
    pub income: i64,
}

/// Числа расчёта полями формы.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Numbers {
    /// Разделы строк с вычетом — по номеру.
    pub sections: Vec<Section>,
    /// Строки сводки — по номеру.
    pub rows: Vec<Row>,
    pub total: Total,
    pub turnover: Turnover,
}

/// Порядок номера формы: числа между точками.
fn order(number: &str) -> Vec<u32> {
    number.split('.').map(|part| part.parse().unwrap_or(u32::MAX)).collect()
}

impl Numbers {
    /// Строки расчёта `lines` полями формы `form`. `None` — форма и расчёт
    /// не сошлись: строки налога в форме нет, у строки с вычетом нет
    /// раздела или раздел есть у строки без вычета, — и вписывать нечего.
    pub fn of(form: &TurnoverForm, lines: &[Line]) -> Option<Self> {
        let mut sections = Vec::new();
        let mut rows = Vec::new();
        let (mut income, mut tax) = (0_i64, 0_i64);
        for line in lines {
            let place = form.line(line.kind)?;
            let rate = match (&place.section, line.deduction) {
                (Some(section), Some(deduction)) => {
                    let fields = What::ALL
                        .into_iter()
                        .map(|what| Field {
                            number: format!("{section}.{}", what.number(&form.fields)),
                            what,
                            value: what.value(line, deduction),
                        })
                        .collect();
                    sections.push(Section {
                        number: section.clone(),
                        kind: line.kind,
                        fields,
                    });
                    None
                }
                (None, None) => Some(line.rate),
                (Some(_), None) | (None, Some(_)) => return None,
            };
            rows.push(Row {
                number: place.row.clone(),
                kind: line.kind,
                income: line.income,
                rate,
                tax: line.tax,
            });
            income = income.saturating_add(line.income);
            tax = tax.saturating_add(line.tax);
        }
        sections.sort_by_key(|section| order(&section.number));
        rows.sort_by_key(|row| order(&row.number));
        Some(Self {
            sections,
            rows,
            total: Total {
                number: form.total.clone(),
                income,
                tax,
            },
            turnover: Turnover {
                number: form.turnover.clone(),
                income,
            },
        })
    }
}

/// Форма расчёта за квартал `quarter`; `None` — номеров полей на этот
/// квартал у программы нет. Её дни — целые кварталы (`rules::TurnoverForm`).
pub fn form(quarter: Quarter) -> Option<&'static TurnoverForm> {
    rules::on(rules::armenia().turnover_form(), quarter.first_day())
}

/// Откуда числа.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Origin {
    /// Снимок закрытого квартала — поданный расчёт; `changed` — расчёт по
    /// данным сейчас с ним расходится, и нужен уточнённый.
    Closed { changed: bool },
    /// Расчёт по данным сейчас; `carry_unknown` — перенос из расчётов до
    /// первой записи в программе ей неизвестен (`QuarterTax::carry_unknown`),
    /// и x.7 может быть больше.
    Calculated { carry_unknown: bool },
}

/// Почему чисел нет.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Missing {
    /// В квартале ИП ушёл с налога с оборота: расчёт — у бухгалтера.
    Left,
    /// Посчитано не всё: чего не хватает, говорит расчёт (`turnover::Gaps`).
    Incomplete,
    /// Номеров полей формы на этот квартал у программы нет.
    NoForm,
}

/// Числа, которые можно вписывать в форму.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Ready {
    pub quarter: Quarter,
    /// Версия правил, по которой посчитаны числа: у закрытого квартала —
    /// его снимка.
    pub rules: Version,
    pub numbers: Numbers,
    pub origin: Origin,
}

impl Ready {
    /// Есть ли у чисел таблица по видам деятельности. У закрытого квартала
    /// её нет: снимок долей не хранит, а посчитанные по нынешним книге
    /// и профилю они разошлись бы с поданными — добавили код той же строки,
    /// перенесли доход между кодами, — хотя строки налога те же.
    pub fn has_activities(&self) -> bool {
        matches!(self.origin, Origin::Calculated { .. })
    }

    /// Что сказать о числах словами — на экране и в тексте: квартал закрыт,
    /// его данные изменились после закрытия, квартал ещё идёт на день
    /// `today`, перенос до программы неизвестен.
    pub fn notes(&self, today: Option<Date>) -> Vec<Key> {
        match self.origin {
            Origin::Closed { changed } => {
                let mut notes = vec![Key::ReportClosed];
                if changed {
                    notes.push(Key::ReportClosedChanged);
                }
                notes
            }
            Origin::Calculated { carry_unknown } => {
                let mut notes = Vec::new();
                if today.is_some_and(|today| self.quarter.last_day() >= today) {
                    notes.push(Key::ReportOngoing);
                }
                if carry_unknown {
                    notes.push(Key::TaxCarryUnknown);
                }
                notes
            }
        }
    }
}

/// Числа для расчёта квартала `tax` — или почему их нет. `closing` —
/// снимок квартала, если он закрыт: тогда числа — его.
pub fn report(tax: &QuarterTax, closing: Option<&Closing>) -> Result<Ready, Missing> {
    let quarter = tax.quarter;
    let form = form(quarter);
    match closing.filter(|closing| closing.quarter() == quarter) {
        Some(closing) => {
            let data = closing.data();
            let numbers = form.and_then(|form| Numbers::of(form, &data.lines)).ok_or(Missing::NoForm)?;
            Ok(Ready {
                quarter,
                rules: data.rules,
                numbers,
                origin: Origin::Closed {
                    changed: tax.differs_from(closing),
                },
            })
        }
        None if tax.regime != Regime::Turnover => Err(Missing::Left),
        None if !tax.complete() => Err(Missing::Incomplete),
        None => {
            let numbers = form.and_then(|form| Numbers::of(form, &tax.lines)).ok_or(Missing::NoForm)?;
            Ok(Ready {
                quarter,
                rules: tax.rules,
                numbers,
                origin: Origin::Calculated {
                    carry_unknown: tax.carry_unknown,
                },
            })
        }
    }
}

/// Доля дохода квартала по коду деятельности.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Share {
    pub code: ActivityCode,
    /// Доля — сотыми процента; доли всех кодов в сумме — ровно 100 %.
    pub percent: Percent,
}

/// Таблица «Доход по видам деятельности» (п. 70).
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Activities {
    /// Доли — по убыванию, при равных — по коду. Пусто — дохода нет или
    /// у части дохода код неизвестен.
    pub shares: Vec<Share>,
    /// Доходов, чей код программе неизвестен: кодов в профиле несколько,
    /// а у дохода вид не указан. Тогда долей нет вовсе: без них доли были
    /// бы неверны все.
    pub unknown: usize,
}

/// Доход квартала по кодам деятельности — по книге. Доход в валюте без
/// курса в налог не вошёл и сюда не идёт: о нём говорит расчёт
/// (`turnover::Gaps::unrated`).
pub fn activities(books: Books<'_>, quarter: Quarter) -> Activities {
    let mut by_code: BTreeMap<ActivityCode, i64> = BTreeMap::new();
    let mut unknown = 0;
    for row in ledger::between(books.documents, books.operations, quarter.first_day(), quarter.last_day()) {
        if row.flow != Flow::Income {
            continue;
        }
        let Ok(lumas) = row.lumas() else {
            continue;
        };
        match turnover::income_code(books.profile, &row) {
            Some(code) => {
                let sum = by_code.entry(code).or_insert(0);
                *sum = sum.saturating_add(lumas.get());
            }
            None => unknown += 1,
        }
    }
    if unknown > 0 {
        return Activities {
            shares: Vec::new(),
            unknown,
        };
    }
    Activities {
        shares: shares(&by_code),
        unknown: 0,
    }
}

/// Доли дохода в сотых процента, в сумме — ровно 100 %: каждой — её доля
/// с отброшенным остатком, а недостающие сотые — по одной долям с самыми
/// большими остатками, при равных — по коду.
fn shares(by_code: &BTreeMap<ActivityCode, i64>) -> Vec<Share> {
    let total: i128 = by_code.values().map(|lumas| i128::from(*lumas)).sum();
    if total <= 0 {
        return Vec::new();
    }
    let whole = i128::from(Percent::WHOLE.hundredths());
    let mut parts: Vec<(ActivityCode, i128, i128)> = by_code
        .iter()
        .map(|(code, lumas)| {
            let scaled = i128::from(*lumas) * whole;
            (*code, scaled / total, scaled % total)
        })
        .collect();
    let mut left = whole - parts.iter().map(|(_, hundredths, _)| hundredths).sum::<i128>();
    let mut by_rest: Vec<usize> = (0..parts.len()).collect();
    by_rest.sort_by(|a, b| parts[*b].2.cmp(&parts[*a].2).then(parts[*a].0.cmp(&parts[*b].0)));
    for index in by_rest {
        if left <= 0 {
            break;
        }
        parts[index].1 += 1;
        left -= 1;
    }
    let mut shares: Vec<Share> = parts
        .into_iter()
        .filter_map(|(code, hundredths, _)| {
            let percent = Percent::from_hundredths(u16::try_from(hundredths).ok()?)?;
            Some(Share { code, percent })
        })
        .collect();
    shares.sort_by(|a, b| b.percent.cmp(&a.percent).then(a.code.cmp(&b.code)));
    shares
}

/// Код с буквой раздела, как в форме и в официальных текстах: `J62.01.0`
/// (`model::activity`).
pub fn lettered(code: ActivityCode) -> String {
    format!("{}{code}", code.section())
}

/// Числа для расчёта текстом — «Скопировать всё»: например, чтобы отправить
/// бухгалтеру. Слова — на языке `lang`, числа — как их вписывают в форму
/// ([`Value::plain`]): так их не прочтут по-разному на разных языках.
/// Таблица по видам деятельности — когда доли посчитаны и квартал не
/// закрыт ([`Ready::has_activities`]); поле, у которого числа нет, — словами.
pub fn text(lang: Lang, ready: &Ready, activities: Option<&Activities>, today: Option<Date>) -> String {
    let field = |number: &str, key: Key, value: Value| {
        let value = value.plain().unwrap_or_else(|| t(lang, Key::ReportSplitYourself).to_owned());
        fill(t(lang, Key::ReportTextField), &[number, t(lang, key), &value])
    };
    let heading = |number: &str, key: Key| fill(t(lang, Key::ReportHeading), &[number, t(lang, key)]);
    let numbers = &ready.numbers;
    let mut lines = vec![fill(t(lang, Key::ReportTextTitle), &[&i18n::quarter(lang, ready.quarter)])];
    lines.extend(ready.notes(today).into_iter().map(|key| t(lang, key).to_owned()));
    for section in &numbers.sections {
        lines.push(String::new());
        lines.push(heading(&section.number, kind_key(section.kind)));
        for item in &section.fields {
            lines.push(field(&item.number, item.what.key(), item.value));
        }
    }
    for row in &numbers.rows {
        lines.push(String::new());
        lines.push(heading(&row.number, kind_key(row.kind)));
        lines.push(field(&row.number, Key::ReportColIncome, Value::Drams(row.income)));
        if let Some(rate) = row.rate {
            lines.push(field(&row.number, Key::ReportColRate, Value::Percent(rate)));
        }
        lines.push(field(&row.number, Key::ReportColTax, Value::Drams(row.tax)));
    }
    let total = &numbers.total;
    lines.push(String::new());
    lines.push(heading(&total.number, Key::ReportTotal));
    lines.push(field(&total.number, Key::ReportColIncome, Value::Drams(total.income)));
    lines.push(field(&total.number, Key::ReportColTax, Value::Drams(total.tax)));
    let turnover = &numbers.turnover;
    lines.push(field(&turnover.number, Key::ReportTurnover, Value::Drams(turnover.income)));
    if let Some(activities) = activities.filter(|activities| ready.has_activities() && !activities.shares.is_empty()) {
        lines.push(String::new());
        lines.push(t(lang, Key::ReportActivities).to_owned());
        for share in &activities.shares {
            lines.push(field(&lettered(share.code), Key::ReportShare, Value::Percent(share.percent)));
        }
    }
    lines.push(String::new());
    lines.push(fill(t(lang, Key::ReportTextSource), &[&ready.rules.to_string()]));
    lines.join("\n")
}

#[cfg(test)]
mod tests;
