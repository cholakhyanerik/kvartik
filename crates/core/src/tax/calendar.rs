//! Сроки в календаре телефона и почты (задача 22): файл iCalendar
//! (RFC 5545). Программа без сервера не напомнит, пока закрыта
//! (`ui/deadlines.rs`), а календарь Google, Apple или Outlook напомнит сам.
//!
//! # Общий и личный
//!
//! Общий ([`public`]) — сроки всех ИП на налоге с оборота
//! (`deadlines::general`) на трёх языках: файлы сайта для подписки, которые
//! пишет программа сборки (`crates/sw`) по адресу [`path`]. Новая выкладка
//! — новый календарь у всех подписчиков. Адрес подписки — навсегда, как
//! адрес программы (CLAUDE.md, Архитектура): подписавшиеся помнят его сами.
//!
//! Личный ([`personal`]) — сроки этого ИП (`deadlines::between`) на год
//! вперёд с суммами на день выгрузки: его собирает программа, а отдаёт
//! файлом окно.
//!
//! # Повторяемость
//!
//! Общий календарь — файл сайта, а сайт обязан собираться в те же байты из
//! того же кода (CLAUDE.md, Правило 6): иначе пересборка без правок была бы
//! у всех «новой версией». Поэтому в нём нет ни часов сборки, ни «сегодня»:
//! сроки — с 1 января года последней сверки правил (`Rules::checked`) по
//! 31 декабря следующего, а `DTSTAMP` — день той сверки. Правила, не
//! сверенные больше года, календарь дальше не обещает; новая сверка —
//! новый год сроков.
//!
//! # Формат
//!
//! Срок — событие на весь день (`DTSTART;VALUE=DATE`): день в календаре,
//! без часового пояса (CLAUDE.md, Правило 2), — и в Ереване, и у ИП
//! в поездке. Напоминания — за 7 дней и за день, в 9 утра по часам
//! устройства: `TRIGGER` считается от полуночи дня срока. У события
//! постоянный `UID`: обновлённая подписка меняет событие, а не добавляет
//! второе.
//!
//! У первого взноса помесячной медстраховки (за январь 2027 года) событий
//! два: срок по прочтению программы, без переноса, и день с безопасной
//! стороны — 20 января (`deadlines::safe`, [`safe_event`]): сайт, программа
//! и календарь говорят одно, и напоминание приходит до 20 января (решения
//! человека 2026-11-26).
//!
//! Текст — из таблицы строк на языке календаря, и экранируется он по
//! RFC 5545, 3.3.11 (CLAUDE.md, Правило 13): `\`, `;`, `,` и перевод строки
//! — знаком `\`, прочие управляющие знаки отбрасываются. Строки длиннее
//! 75 октетов складываются (3.1) по границе знака: армянская буква — два
//! октета, и разрезанная пополам стала бы у читателя двумя мусорными.

use super::annual::{self, Annual, Health, Total};
use super::deadlines::{self, AHEAD_DAYS, Deadline, Kind};
use super::rules;
use super::turnover::{self, Books, Regime};
use crate::i18n::{self, Key, Lang, fill, plural, t};
use crate::model::date::Date;
use crate::model::money::{Currency, Money};

/// Адрес программы — ссылка в событиях общего календаря. Адрес навсегда
/// (CLAUDE.md, Архитектура): ссылка в календаре подписчика живёт годами.
pub const APP_URL: &str = "https://kvartik.com/app/";

/// Предел длины строки файла без `CRLF`, октетов (RFC 5545, 3.1).
const LINE_OCTETS: usize = 75;

/// Путь общего календаря на языке `lang` — от корня сайта, без косой черты
/// в начале: `calendar/ru.ics`. Код языка — `hy`, а не `am` (`Lang::code`).
pub fn path(lang: Lang) -> String {
    format!("calendar/{}.ics", lang.code())
}

/// Общий календарь сроков на языке `lang` — файл сайта (шапка модуля,
/// «Повторяемость»): из одних правил, без часов.
pub fn public(lang: Lang) -> String {
    let all = rules::armenia();
    let checked = all.checked();
    let span = checked.and_then(|checked| {
        let from = Date::new(checked.year(), 1, 1)?;
        let until = Date::new(checked.year().checked_add(1)?, 12, 31)?;
        Some((from, until))
    });
    let mut events: Vec<Event> = Vec::new();
    if let Some((from, until)) = span {
        for deadline in deadlines::general(from, until) {
            events.extend(safe_event(lang, deadline, from, "", |lines| {
                lines.push(fill(t(lang, Key::CalendarYours), &[APP_URL]));
            }));
            events.push(Event {
                uid: format!("{}@kvartik.com", uid(deadline.kind)),
                day: deadline.day,
                summary: deadlines::what(lang, deadline.kind),
                lines: public_lines(lang, deadline),
            });
        }
    }
    events.sort_by_key(|event| event.day);
    let about = match checked {
        Some(checked) => fill(t(lang, Key::CalendarPublicAbout), &[&i18n::date(lang, checked)]),
        None => String::new(),
    };
    let head = Head {
        lang,
        name: t(lang, Key::CalendarPublicName),
        about: &about,
        // Для подписки: адрес файла и как часто его перечитывать
        // (RFC 7986, 5.7 и 5.8; `X-PUBLISHED-TTL` — то же для тех, кто
        // RFC 7986 не знает). Раз в день: сроки меняет только выкладка.
        source: Some(format!("https://kvartik.com/{}", path(lang))),
    };
    write(&head, checked, &events)
}

/// Личный календарь: несделанные сроки этого ИП с `today` на год вперёд —
/// с суммами по книге на этот день, со ссылкой на программу `app_url`.
/// `None` — сроков нет: пустой файл человеку ни к чему.
pub fn personal(books: Books<'_>, today: Date, lang: Lang, app_url: &str) -> Option<String> {
    let until = today.add_days(AHEAD_DAYS)?;
    let mut years: Vec<Annual> = Vec::new();
    let mut events: Vec<Event> = Vec::new();
    for deadline in deadlines::between(books, today, until).into_iter().filter(|deadline| !deadline.done) {
        events.extend(safe_event(lang, deadline, today, "mine-", |lines| {
            lines.push(fill(t(lang, Key::CalendarOpen), &[app_url]));
        }));
        let amount = amount(books, today, deadline.kind, &mut years);
        events.push(Event {
            // Не те, что у общего календаря: событие своего файла
            // не должно заменять событие подписки и наоборот.
            uid: format!("mine-{}@kvartik.com", uid(deadline.kind)),
            day: deadline.day,
            summary: deadlines::what(lang, deadline.kind),
            lines: personal_lines(lang, today, deadline, amount, app_url),
        });
    }
    events.sort_by_key(|event| event.day);
    if events.is_empty() {
        return None;
    }
    // Чего не хватает профилю, того срока нет и в файле — файл говорит это
    // сам, как экран сроков: «нет значения» — не «срока нет» (Правило 6).
    let gaps = deadlines::gaps(books, today, until);
    let mut about = fill(t(lang, Key::CalendarPersonalAbout), &[&i18n::date(lang, today)]);
    for (missing, key) in [
        (gaps.registered, Key::DeadlinesNeedRegistered),
        (gaps.residency, Key::DeadlinesNeedResidency),
        (gaps.health, Key::DeadlinesHealthUnclear),
    ] {
        if missing {
            about.push('\n');
            about.push_str(t(lang, key));
        }
    }
    let head = Head {
        lang,
        name: t(lang, Key::CalendarPersonalName),
        about: &about,
        source: None,
    };
    Some(write(&head, Some(today), &events))
}

/// Постоянная часть `UID` срока: что и за какой период. Не меняется ни
/// с языком, ни с днём срока — иначе перенос дня или новая выкладка
/// добавили бы второе событие вместо того, чтобы сдвинуть первое.
fn uid(kind: Kind) -> String {
    match kind {
        Kind::Turnover(quarter) => format!("turnover-{}-q{}", quarter.year(), quarter.number()),
        Kind::Application(year) => format!("application-{year}"),
        Kind::MicroApplication(year) => format!("micro-application-{year}"),
        Kind::MicroReport(year) => format!("micro-report-{year}"),
        Kind::Annual(year) => format!("annual-{year}"),
        Kind::MicroAnnual(year) => format!("micro-annual-{year}"),
        Kind::HealthYear(year) => format!("health-{year}"),
        Kind::HealthMonth { year, month } => format!("health-{year}-{month:02}"),
        Kind::Declaration(year) => format!("declaration-{year}"),
    }
}

/// Платят ли к сроку налог: к нему — слова о дне зачисления на единый счёт
/// (НК, ст. 18, ч. 4). Взнос медстраховки — не налог, и применима ли к нему
/// эта норма, не сверено (ревью задачи 22): у его события этих слов нет.
const fn tax_payment(kind: Kind) -> bool {
    match kind {
        Kind::Turnover(_) | Kind::Annual(_) => true,
        // Гербовый сбор — не налог кодекса (закон ՀՕ-245-Ն), и о норме
        // ст. 18, ч. 4 к нему не сверено — как у взноса медстраховки.
        Kind::Application(_)
        | Kind::MicroApplication(_)
        | Kind::MicroReport(_)
        | Kind::MicroAnnual(_)
        | Kind::HealthYear(_)
        | Kind::HealthMonth { .. }
        | Kind::Declaration(_) => false,
    }
}

/// Слова о сроке, общие для обоих календарей: кому декларация, с какого
/// дня перенесён срок и когда налог считается заплаченным.
fn common_lines(lang: Lang, deadline: Deadline, lines: &mut Vec<String>) {
    if matches!(deadline.kind, Kind::Declaration(_)) {
        lines.push(t(lang, Key::DeadlinesDeclarationAbout).to_owned());
    }
    // Дня у календаря нет: событие живёт до срока и после дня с безопасной
    // стороны, — строки под первым взносом медстраховки обе (`notes`).
    lines.extend(deadlines::notes(lang, deadline.kind, deadline.by_law, None));
    if tax_payment(deadline.kind) {
        lines.push(t(lang, Key::DeadlinesPayEarly).to_owned());
    }
}

/// Описание события общего календаря: кого срок касается не всех — слова
/// о том, кого; общие слова; и что точно скажет программа по профилю.
fn public_lines(lang: Lang, deadline: Deadline) -> Vec<String> {
    let all = rules::armenia();
    let mut lines = Vec::new();
    match deadline.kind {
        Kind::Application(year) => {
            let rule = Date::new(year, 1, 1).and_then(|start| rules::on(all.turnover_application(), start));
            if let Some(rule) = rule {
                let days = u64::from(rule.after_days);
                let count = fill(
                    plural(lang, days, Key::DaysOne, Key::DaysFew, Key::DaysMany),
                    &[&i18n::integer(lang, i64::from(rule.after_days))],
                );
                lines.push(fill(t(lang, Key::CalendarApplicationNew), &[&count]));
            }
        }
        Kind::HealthYear(year) => {
            if let Some(rules::HealthBy::Year { income_over, .. }) =
                rules::for_year(all.health_insurance(), year).map(|rule| rule.by)
            {
                let over = i18n::money(lang, Money::new(income_over, Currency::Amd));
                lines.push(fill(t(lang, Key::CalendarHealthOver), &[&over]));
            }
        }
        // Общий календарь — налог с оборота: сроков микропредпринимательства
        // в нём нет (`deadlines::general`).
        Kind::Turnover(_)
        | Kind::Annual(_)
        | Kind::MicroApplication(_)
        | Kind::MicroReport(_)
        | Kind::MicroAnnual(_)
        | Kind::HealthMonth { .. }
        | Kind::Declaration(_) => {}
    }
    common_lines(lang, deadline, &mut lines);
    lines.push(fill(t(lang, Key::CalendarYours), &[APP_URL]));
    lines
}

/// Сумма к сроку по книге: драмы и что сказать, если посчитано не всё.
/// Драм нет — не посчитана ни одна строка: ноль выглядел бы расчётом
/// (CLAUDE.md, Правило 6), и событие говорит только «посчитано не всё».
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Amount {
    drams: Option<i64>,
    partial: Option<Key>,
}

/// Сумма к сроку `kind` по книге на день `today`; `None` — к сроку платить
/// нечего, её не знать или её считает бухгалтер. Налог квартала — только
/// когда квартал кончился: сумма незаконченного выросла бы после выгрузки,
/// а выглядела бы расчётом (CLAUDE.md, Правило 6). Годовые платежи
/// незаконченного года — сколько известно на сегодня, как на карточке
/// главного экрана: гербовый сбор зависит от оборота года, и описание
/// говорит, на какой день сумма. Годы считаются по разу на файл (`years`).
fn amount(books: Books<'_>, today: Date, kind: Kind, years: &mut Vec<Annual>) -> Option<Amount> {
    let mut annual = |year: u16| -> Annual {
        if let Some(found) = years.iter().find(|annual| annual.year == year) {
            return found.clone();
        }
        let counted = annual::year(books, year);
        years.push(counted.clone());
        counted
    };
    match kind {
        Kind::Turnover(quarter) if quarter.last_day() < today => {
            let tax = turnover::quarter(books, quarter);
            let complete = tax.complete();
            (tax.regime == Regime::Turnover).then(|| Amount {
                drams: (complete || !tax.lines.is_empty()).then_some(tax.tax),
                partial: (!complete).then_some(Key::TaxPartial),
            })
        }
        Kind::Annual(year) => {
            let total = Total::of(&annual(year));
            total.sum.map(|drams| Amount {
                drams: Some(drams),
                partial: (!total.complete).then_some(Key::AnnualPartialCard),
            })
        }
        Kind::HealthYear(year) => match annual(year).health {
            Health::Year { amount: drams, .. } => Some(Amount {
                drams: Some(drams),
                partial: None,
            }),
            _ => None,
        },
        Kind::HealthMonth { year, .. } => match annual(year).health {
            Health::Month { monthly: drams, .. } => Some(Amount {
                drams: Some(drams),
                partial: None,
            }),
            _ => None,
        },
        // Год микропредпринимательства — один гербовый сбор: налога на
        // прибыль нет, а соцплатёж самозанятого — не к этому сроку
        // (`annual::Social::SelfEmployed`). Строки без курса — сбор может
        // быть ступенью выше.
        // Нечитаемые записи и строки без курса прошлого и этого года — база
        // может быть больше, а сбор ступенью выше, как у `Total::of`.
        Kind::MicroAnnual(year) => {
            let annual = annual(year);
            let unrated = matches!(annual.stamp, annual::Stamp::Due { unrated, next: Some(_), .. } if unrated > 0);
            let partial = unrated || annual.unreadable > 0 || annual.broken || annual.regime_unrated > 0;
            annual.stamp.amount().map(|drams| Amount {
                drams: Some(drams),
                partial: partial.then_some(Key::AnnualPartialCard),
            })
        }
        Kind::Turnover(_)
        | Kind::Application(_)
        | Kind::MicroApplication(_)
        | Kind::MicroReport(_)
        | Kind::Declaration(_) => None,
    }
}

/// Описание события личного календаря: сумма на день выгрузки, общие
/// слова и ссылка на программу.
fn personal_lines(lang: Lang, today: Date, deadline: Deadline, amount: Option<Amount>, app_url: &str) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(Amount { drams, partial }) = amount {
        if let Some(drams) = drams {
            let sum = i18n::money(lang, Money::new(drams, Currency::Amd));
            lines.push(fill(t(lang, Key::CalendarAmount), &[&sum, &i18n::date(lang, today)]));
        }
        lines.extend(partial.map(|key| t(lang, key).to_owned()));
    }
    common_lines(lang, deadline, &mut lines);
    lines.push(fill(t(lang, Key::CalendarOpen), &[app_url]));
    lines
}

/// Событие на день с безопасной стороны у первого взноса помесячной
/// медстраховки (`deadlines::safe`; решение агента 2026-11-26 по поручению
/// человека): своё событие на весь день, а не напоминание за месяц у события
/// срока. Напоминания подписки календарь может снять молча — Apple, по всей
/// видимости, снимает, Outlook берёт только первое (Правило 6, «Календарь
/// принимает файл…»), — а событие на сам день видно в сетке и без них, и
/// у него те же два напоминания, что у всех: за неделю и за день до него.
/// `UID` — `UID` срока с `-safe`: постоянный, как у всех. Дня раньше `from`
/// — начала общего календаря или «сегодня» личного — нет: прошедшее
/// событие ни к чему. `prefix` — `mine-` у личного; `link` дописывает
/// ссылку на программу, как у событий своего календаря.
fn safe_event(lang: Lang, deadline: Deadline, from: Date, prefix: &str, link: impl FnOnce(&mut Vec<String>)) -> Option<Event> {
    let safe = deadlines::safe(deadline.kind).filter(|safe| *safe >= from)?;
    let mut lines = vec![fill(
        t(lang, Key::CalendarSafeAbout),
        &[&i18n::date(lang, deadline.by_law), &i18n::date(lang, safe)],
    )];
    lines.extend(deadlines::safe_annual(lang, deadline.kind));
    link(&mut lines);
    Some(Event {
        uid: format!("{prefix}{}-safe@kvartik.com", uid(deadline.kind)),
        day: safe,
        summary: fill(t(lang, Key::CalendarSafeSummary), &[&deadlines::what(lang, deadline.kind)]),
        lines,
    })
}

/// Событие: день и слова о нём.
struct Event {
    uid: String,
    day: Date,
    summary: String,
    /// Абзацы описания.
    lines: Vec<String>,
}

/// Шапка календаря.
struct Head<'a> {
    lang: Lang,
    name: &'a str,
    about: &'a str,
    /// Адрес файла подписки; `None` — файл отдают один раз.
    source: Option<String>,
}

/// Файл календаря: шапка и события. `stamp` — день, на который собраны
/// данные (`DTSTAMP` обязателен у события: RFC 5545, 3.6.1); `None` —
/// событий и так нет.
fn write(head: &Head<'_>, stamp: Option<Date>, events: &[Event]) -> String {
    let mut out = String::new();
    let mut line = |text: &str| fold(&mut out, text);
    line("BEGIN:VCALENDAR");
    line("VERSION:2.0");
    line(&format!("PRODID:-//Kvartik//Kvartik//{}", head.lang.code().to_ascii_uppercase()));
    line("CALSCALE:GREGORIAN");
    // `PUBLISH` — календарь для чтения, а не приглашение на встречу
    // (RFC 5546, 3.2.1).
    line("METHOD:PUBLISH");
    line(&format!("NAME:{}", escape(head.name)));
    line(&format!("X-WR-CALNAME:{}", escape(head.name)));
    if !head.about.is_empty() {
        line(&format!("DESCRIPTION:{}", escape(head.about)));
        line(&format!("X-WR-CALDESC:{}", escape(head.about)));
    }
    if let Some(source) = &head.source {
        line(&format!("SOURCE;VALUE=URI:{source}"));
        line("REFRESH-INTERVAL;VALUE=DURATION:P1D");
        line("X-PUBLISHED-TTL:P1D");
    }
    let stamp = stamp.map(|day| format!("{}T000000Z", ymd(day)));
    for event in events {
        let Some(stamp) = &stamp else { break };
        let day = event.day;
        let summary = escape(&event.summary);
        line("BEGIN:VEVENT");
        line(&format!("UID:{}", event.uid));
        line(&format!("DTSTAMP:{stamp}"));
        line(&format!("DTSTART;VALUE=DATE:{}", ymd(day)));
        // Конец события на весь день — следующий день, не включая его
        // (RFC 5545, 3.6.1); без него событие длится день и так.
        if let Some(next) = day.add_days(1) {
            line(&format!("DTEND;VALUE=DATE:{}", ymd(next)));
        }
        line(&format!("SUMMARY:{summary}"));
        line(&format!("DESCRIPTION:{}", escape(&event.lines.join("\n"))));
        // Срок не занимает времени: в расписании человек свободен.
        line("TRANSP:TRANSPARENT");
        // За 7 дней и за день, в 9 утра: от полуночи дня срока назад на
        // 6 дней и 15 часов и на 15 часов.
        for trigger in ["-P6DT15H", "-PT15H"] {
            line("BEGIN:VALARM");
            line("ACTION:DISPLAY");
            line(&format!("DESCRIPTION:{summary}"));
            line(&format!("TRIGGER:{trigger}"));
            line("END:VALARM");
        }
        line("END:VEVENT");
    }
    line("END:VCALENDAR");
    out
}

/// День для iCalendar: `20261020`.
fn ymd(day: Date) -> String {
    format!("{:04}{:02}{:02}", day.year(), day.month(), day.day())
}

/// Текст для значения вида TEXT (RFC 5545, 3.3.11): `\`, `;`, `,`
/// и перевод строки — со знаком `\`, прочие управляющие знаки — прочь:
/// в файле им не место (CONTROL в 3.1), а `CR` разорвал бы строку.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            ';' => out.push_str("\\;"),
            ',' => out.push_str("\\,"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push(c),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

/// Строка файла: не длиннее [`LINE_OCTETS`] октетов, а длиннее —
/// продолжается следующей строкой с пробелом в начале (RFC 5545, 3.1).
/// Режется только между знаками.
fn fold(out: &mut String, line: &str) {
    let mut used = 0;
    for c in line.chars() {
        let octets = c.len_utf8();
        if used + octets > LINE_OCTETS {
            out.push_str("\r\n ");
            used = 1;
        }
        out.push(c);
        used += octets;
    }
    out.push_str("\r\n");
}

#[cfg(test)]
mod tests;
