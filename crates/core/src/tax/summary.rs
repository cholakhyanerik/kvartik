//! Сводка конторы (задача 38, этап 5): «кто что должен и когда» — у каждого
//! ИП ближайший срок, прошлый квартал — к уплате или «расчёт подан» — и
//! годовые платежи года ближайшего срока. Итога по всем ИП нет: каждый ИП
//! платит своё (решение человека 2026-11-05).
//!
//! Строка собирает то, что считают карточки главного экрана одного ИП:
//! просроченный квартал — [`deadlines::overdue`], сроки —
//! [`deadlines::between`], квартал — [`turnover::quarter`] и снимок
//! [`Closing`], годовые платежи — [`annual::year`] и [`Total`]. Второй расчёт
//! «для сводки» однажды разошёлся бы с первым (Архитектура: калькулятор
//! лендинга зовёт тот же `tax` по той же причине), поэтому здесь своего
//! расчёта нет. Окно считает строку каждого ИП по книгам его базы
//! (`ui/office.rs`), по одной.

use super::annual::{self, Status, Total};
use super::closing::Closing;
use super::deadlines::{self, AHEAD_DAYS, Deadline, When};
use super::turnover::{self, Books, Regime};
use crate::model::date::{Date, Quarter};

/// Строка сводки одного ИП.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Row {
    /// Ближайший срок: прошлый квартал, срок которого прошёл, а расчёт не
    /// подан, — первым, иначе первый несделанный срок на год вперёд
    /// ([`AHEAD_DAYS`]). `None` — сроков нет.
    pub next: Option<Next>,
    /// Прошлый квартал; `None` — расчёта за него ИП не подаёт: ещё не был
    /// зарегистрирован, остановил деятельность на весь квартал или был
    /// микропредприятием. Общий режим весь год — [`Previous::Left`] с его
    /// начала, как у карточки налога.
    pub previous: Option<Previous>,
    /// Годовые платежи года, чей срок ближе всех (`annual::due_year`);
    /// `None` — платить в срок нечего, как и у карточки главного экрана:
    /// весь год общий режим (их считает бухгалтер) или ИП в том году ещё не
    /// был.
    pub annual: Option<Yearly>,
}

/// Ближайший срок.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Next {
    pub deadline: Deadline,
    /// Насколько он близок — словами напоминаний главного экрана:
    /// [`When::Overdue`] — срок прошёл, а расчёт не подан
    /// (`deadlines::overdue`), иначе — сегодня, завтра или через столько
    /// дней, хоть бы и через полгода.
    pub when: When,
    /// День, к которому считается `when`, — как у напоминания главного
    /// экрана (`deadlines::remind_day`): у первого взноса помесячной
    /// медстраховки до 20 января — этот день, иначе — сам срок.
    pub day: Date,
}

/// Прошлый квартал.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Previous {
    /// Расчёт подан — квартал закрыт; налог — из снимка, как его подали.
    Filed { quarter: Quarter, tax: i64 },
    /// Расчёт не подан: налог по книге — сумма посчитанных строк, — всё ли
    /// посчитано (`QuarterTax::complete`) и срок с переносом.
    ToPay {
        quarter: Quarter,
        tax: i64,
        complete: bool,
        due: Deadline,
    },
    /// В квартале ИП ушёл с налога с оборота — с этого дня, а `micro` — с
    /// микропредпринимательства (задача 100): налог такого квартала считает
    /// бухгалтер (`turnover::Regime::Left`).
    Left { quarter: Quarter, from: Date, micro: bool },
}

/// Годовые платежи года.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Yearly {
    pub year: u16,
    pub total: Total,
}

/// Строка сводки ИП по его книге `books` в день `today`.
pub fn row(books: Books<'_>, today: Date) -> Row {
    let next = match deadlines::overdue(books, today) {
        Some(deadline) => Some(Next {
            deadline,
            when: When::Overdue,
            day: deadline.day,
        }),
        None => today.add_days(AHEAD_DAYS).and_then(|until| {
            deadlines::between(books, today, until)
                .into_iter()
                .find(|deadline| !deadline.done)
                .map(|deadline| {
                    let day = deadlines::remind_day(&deadline, today);
                    Next {
                        deadline,
                        when: match day.days_since(today) {
                            0 => When::Today,
                            1 => When::Tomorrow,
                            days => When::In(days),
                        },
                        day,
                    }
                })
        }),
    };
    let previous = today.quarter().previous().and_then(|quarter| previous(books, quarter));
    let year = annual::due_year(today, books.profile.map(|profile| &profile.person));
    let annual = annual::year(books, year);
    Row {
        next,
        previous,
        // Год микропредпринимательства — тоже: к сроку — гербовый сбор
        // (задача 100), как у карточки годовых платежей.
        annual: matches!(annual.status, Status::Turnover | Status::Micro).then(|| Yearly {
            year,
            total: Total::of(&annual),
        }),
    }
}

/// Прошлый квартал `quarter`: подан ли за него расчёт, что к уплате и не
/// ушёл ли ИП с налога с оборота.
fn previous(books: Books<'_>, quarter: Quarter) -> Option<Previous> {
    if let Some(closing) = Closing::current(books.closings, quarter) {
        return Some(Previous::Filed {
            quarter,
            tax: closing.data().tax,
        });
    }
    let tax = turnover::quarter(books, quarter);
    match tax.regime {
        // Расчёт за квартал на налоге с оборота подают не всегда: ИП ещё не
        // был зарегистрирован или весь квартал стоял — тогда и срока нет
        // (`deadlines::quarter_deadline`).
        Regime::Turnover => deadlines::quarter_deadline(books, quarter).map(|due| Previous::ToPay {
            quarter,
            tax: tax.tax,
            complete: tax.complete(),
            due,
        }),
        Regime::Left { from, why } => Some(Previous::Left {
            quarter,
            from,
            micro: why.micro(),
        }),
        // Квартал микропредприятия расчёта не подаёт, и срока у него нет:
        // сюда он не доходит, а дойдёт — сказать о нём нечего (задача 100).
        Regime::Micro => None,
    }
}

#[cfg(test)]
mod tests;
