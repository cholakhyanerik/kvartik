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
//! ограничивает.

use super::rules;
use crate::model::date::Date;
use crate::model::document::Document;
use crate::model::ledger;

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

#[cfg(test)]
mod tests;
