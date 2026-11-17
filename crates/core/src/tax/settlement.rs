//! Расчётный документ в системе ՊԵԿ (задача 23): по каким документам он ещё
//! не отмечен и до какого дня его выписать.
//!
//! «Հաշիվ վավերագիր» ИП выписывает в системе ՊԵԿ сам: программа его не
//! выписывает и никуда не отправляет (Архитектура, «Сеть — только к своему
//! сайту»), а помнит отметку человека (`Document::set_settlement`). Документ
//! — на поставку, а поставка в программе — документ дохода: акт или счёт
//! без акта, как в книге (`ledger::income_documents`). Доход без документа
//! в программе (операция книги) сюда не входит: его документы — вне
//! программы.
//!
//! Последний день — запись файла правил (`settlement_due`): дни после дня
//! оказания услуги, который человек указал у документа. По закону документ
//! по услуге выписывается в момент окончания услуги или её этапа, без
//! допуска в днях (НК, ст. 56, ч. 4; с 2026-09-01 — её п. 2 в редакции
//! ՀՕ-309-Ն, для услуг тот же момент): ноль дней. Что «в момент» — это тот
//! же день, — прочтение (разведка задачи 23, 2026-10-21). С нерабочего дня
//! он не переносится: это момент поставки, а не срок, который кончается
//! в какой-то день (`tax::deadlines`, «Перенос»). Выписать документ раньше
//! закон разрешает (ст. 56, ч. 8, п. 1), и день отметки программа не
//! ограничивает, а о документе, выписанном в квартале раньше услуги,
//! предупреждает ([`early`]).

use super::{rules, turnover};
use crate::model::activity::IncomeKind;
use crate::model::country::Country;
use crate::model::date::{Date, Quarter};
use crate::model::document::{Document, SettlementKind};
use crate::model::ledger::{self, Flow, Row, Source};
use crate::model::profile::ProfileData;

/// Последний день выписки расчётного документа по услуге, оказанной в день
/// `service`, — по правилу, которое действует в этот день. `None` — правила
/// на этот день нет: «нет значения», а не «срок прошёл» (Правило 6).
pub fn due(service: Date) -> Option<Date> {
    let rule = rules::on(rules::armenia().settlement_due(), service)?;
    service.add_days(i32::from(rule.after_days))
}

/// Документ дохода без отметки о расчётном документе.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Pending<'a> {
    pub document: &'a Document,
    /// Последний день выписки ([`due`]); `None` — правила нет.
    pub due: Option<Date>,
}

impl Pending<'_> {
    /// Пора ли выписать в день `today`: последний день наступил или прошёл.
    /// Без правила — не пора: срока программа не знает.
    pub fn due_by(&self, today: Date) -> bool {
        self.due.is_some_and(|due| due <= today)
    }
}

/// Документы дохода без отметки о расчётном документе — по сроку: чей
/// раньше, тот выше; без срока — в конце, по дню оказания услуги. Документ,
/// выпущенный до 0.0.55, сюда не входит: выписан ли его документ,
/// программа не знает (`Document::settlement_known`), а «не знаю» — не
/// «не выписан» (Правило 6).
pub fn pending(documents: &[Document]) -> Vec<Pending<'_>> {
    let mut pending: Vec<Pending<'_>> = ledger::income_documents(documents)
        .into_iter()
        .filter(|document| document.settlement_known() && document.settlement().is_none())
        .map(|document| Pending {
            document,
            due: due(document.data().service_date),
        })
        .collect();
    pending.sort_by_key(|item| {
        (
            item.due.is_none(),
            item.due,
            item.document.data().service_date,
            item.document.number(),
            item.document.id(),
        )
    });
    pending
}

/// Сколько документов в день `today` пора выписать — для напоминания на
/// главном экране.
pub fn due_count(documents: &[Document], today: Date) -> usize {
    pending(documents).iter().filter(|item| item.due_by(today)).count()
}

/// Квартал выписки, когда расчётный документ вида `kind` выписан днём
/// `settled` в квартале раньше квартала услуги документа `document` (задача
/// 77). `None` — квартал тот же или позже, покупатель из другой страны, чек
/// ККМ или поставка, у которой подтверждение момента не двигает. `profile` —
/// чтобы узнать строку налога документа по кодам, как в налоге.
///
/// Момент выполнения работы или оказания услуги — более ранний из двух (НК,
/// ст. 40, ч. 4): окончание или передача результата (п. 1) или подтверждение
/// покупателем расчётного документа исполнителя (п. 2), а выписать документ
/// заранее закон разрешает (ст. 56, ч. 8, п. 1; по тексту arlis 230455,
/// сверено 2026-11-17). Подтвердил покупатель документ до окончания услуги
/// — доход в квартале подтверждения, а не услуги. Подтверждения программа не
/// знает — его видно только в системе ՊԵԿ — и доход считает, как прежде, по
/// дню услуги, а окно предупреждает: спрашивать день подтверждения —
/// толкование закона, вопрос ушёл в письмо ՊԵԿ (задача 82; выбор агента —
/// решение человека 2026-11-17). В том же квартале момент поставки двигает
/// только курс ЦБ документа в валюте, а о нём говорят слова у поля курса.
///
/// Молчит окно, где правило не действует: у товара момент — передача или
/// приёмка, подтверждения в ст. 38 нет, а документ на товар и выписывают до
/// поставки (ст. 56, ч. 4, п. 1); ч. 4 ст. 40 — для услуг, не названных
/// в ч. 1–3: у аренды момент — последний день отчётного периода (ч. 2),
/// у лицензий и роялти по подписке — последний день месяца (ч. 3), а
/// проценты — не услуга. Строка, выбранная у самого дохода, говорит сама,
/// как у файла для системы ՊԵԿ (`format::accdoc`); строку не узнать — может
/// быть и услугой, и окно предупреждает. Покупатель из другой страны
/// документ в системе ՊԵԿ не подтверждает — его в ней нет; страна не
/// указана — может быть и армянской. Чек ККМ (задача 76) покупатель в
/// системе не подтверждает: подтверждают документ, выписанный в ней.
pub fn early(
    document: &Document,
    profile: Option<&ProfileData>,
    settled: Date,
    kind: SettlementKind,
) -> Option<Quarter> {
    let data = document.data();
    let quarter = settled.quarter();
    let foreign = document
        .issued()
        .and_then(|issued| issued.buyer().country)
        .is_some_and(|country| country != Country::ARMENIA);
    let confirmed = match kind {
        SettlementKind::System => true,
        SettlementKind::Receipt => false,
    };
    if quarter >= data.service_date.quarter() || foreign || !confirmed {
        return None;
    }
    let kind = match data.income_kind {
        Some(kind) => Some(kind),
        None => {
            let row = Row {
                source: Source::Document(document),
                flow: Flow::Income,
                date: data.service_date,
                amount: document.total().ok()?,
                rate: data.rate,
                activity: data.activity,
                income_kind: None,
            };
            turnover::income_kind(profile, &row)
        }
    };
    let confirmed_moves = match kind {
        Some(IncomeKind::Catering | IncomeKind::CateringOther | IncomeKind::HighTech | IncomeKind::Other) | None => true,
        Some(
            IncomeKind::Trade
            | IncomeKind::Recyclables
            | IncomeKind::Newspapers
            | IncomeKind::Production
            | IncomeKind::Assets
            | IncomeKind::Rent,
        ) => false,
    };
    confirmed_moves.then_some(quarter)
}

#[cfg(test)]
mod tests;
