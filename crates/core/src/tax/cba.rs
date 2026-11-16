//! Курсы ЦБ для подсказки у поля курса (задача 43).
//!
//! Курс суммы в валюте вводит человек (`DocumentData::rate_for`,
//! `OperationData::rate_for`), а окно подсказывает курс ЦБ того дня, который
//! берёт закон, — запись файла правил `rate_day` на день поставки (НК,
//! ст. 16): с 2026-07-01 — опубликованный ЦБ в рабочий день, предыдущий дню
//! поставки ([`hint`]). Подсказка — не решение: человек её подставляет или
//! правит, и в данных подставленный курс — такой же, как введённый руками.
//! До 2026-07-01 подсказки нет: закон брал курс, опубликованный до 16:00
//! дня расчётного документа, а час публикации данные ЦБ не говорят
//! (`tax::rules::RateDay`).
//!
//! # Файл курсов
//!
//! Курсы приходят файлом своего сайта [`FILE`] в папке программы
//! (Архитектура, «Сеть — только к своему сайту»). Его кладёт на сайт
//! выкладка — `tools/deploy/rates.mjs` при каждой выкладке и раз в день по
//! расписанию (`.github/workflows/rates.yml`) — из ответа ЦБ (api.cba.am,
//! `ExchangeRatesByDateRangeByISO`). В программу файл не входит: ни в кэш
//! service worker, ни в хэш сборки, — иначе новый курс каждый день был бы
//! новой версией программы.
//!
//! Файл — договор между выкладкой и всеми версиями программы, как заметки
//! «Что нового» (`release`): прежняя версия из кэша читает файл, который
//! написала новая выкладка. Поэтому новые поля и валюты прежняя пропускает,
//! а смысл поля не меняется:
//!
//! ```json
//! {"source":"https://api.cba.am/exchangerates.asmx",
//!  "currencies":{"USD":[["2026-11-13",3605400],["2026-11-16",3604400]]}}
//! ```
//!
//! `currencies` — по коду валюты ISO 4217 пары «день публикации, драмы за
//! единицу валюты в десятитысячных» (`RATE_SCALE`), дни — по возрастанию,
//! без повторов. Метка дня у ЦБ — день публикации: Armenpress в 17:24
//! 2026-03-10 сообщает курс «сегодня, 10 марта» с тем же изменением, что
//! поле `Diff` записи ЦБ с этой меткой (и так же 2026-02-13; сверено
//! 2026-11-16). Файл пришёл по сети, и разбор строгий: что-то не так —
//! файла нет целиком (Правило 13), а не половина курсов.

use std::collections::BTreeMap;

use serde::Deserialize;

use super::deadlines;
use super::rules::{self, RateDayKind, Rules};
use crate::model::date::Date;
use crate::model::money::{Currency, RATE_SCALE};

/// Имя файла курсов в папке программы.
pub const FILE: &str = "rates.json";

/// Больше файл курсов не бывает: за пять лет — около 2 500 дней публикаций
/// двух валют по 25 байт, 125 КБ (выкладка держит пять лет). Предел — до
/// разбора: файл пришёл по сети (Правило 13).
pub const MAX_BYTES: usize = 1 << 20;

/// Курс больше этого — порча, а не курс: 100 000 ֏ за единицу валюты,
/// а доллар и евро — около 400 ֏ (2026-11-16).
const MAX_RATE_E4: i64 = 100_000 * RATE_SCALE;

/// Курсы ЦБ из файла: у каждой валюты — дни публикации по возрастанию.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Rates {
    usd: Vec<(Date, i64)>,
    eur: Vec<(Date, i64)>,
}

/// Почему файл курсов не принят. Человек этих слов не видит: без файла
/// подсказки просто нет, а курс он вводит сам.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RatesError {
    /// Больше [`MAX_BYTES`].
    TooLarge,
    /// Не JSON, не те поля или типы, день не из календаря, дни не по
    /// возрастанию или с повтором, курс не больше нуля или больше предела.
    Malformed,
}

/// Файл, как он лежит, — до проверки. Незнакомые поля и валюты пропускаются
/// (шапка модуля), и значение незнакомой валюты не читается вовсе: какое оно
/// у новой выкладки, прежней версии неизвестно.
#[derive(Deserialize)]
struct RatesFile {
    currencies: BTreeMap<String, serde_json::Value>,
}

impl Rates {
    /// Разобрать файл курсов и проверить его (шапка модуля).
    pub fn parse(text: &str) -> Result<Self, RatesError> {
        if text.len() > MAX_BYTES {
            return Err(RatesError::TooLarge);
        }
        let file: RatesFile = serde_json::from_str(text).map_err(|_| RatesError::Malformed)?;
        let mut rates = Self::default();
        for (code, value) in file.currencies {
            let series = match code.as_str() {
                "USD" => &mut rates.usd,
                "EUR" => &mut rates.eur,
                _ => continue,
            };
            let pairs: Vec<(String, i64)> = serde_json::from_value(value).map_err(|_| RatesError::Malformed)?;
            *series = Self::series(&pairs).ok_or(RatesError::Malformed)?;
        }
        Ok(rates)
    }

    /// Курсы одной валюты: дни по возрастанию и без повторов, курс больше
    /// нуля и не больше [`MAX_RATE_E4`]. `None` — что-то не так.
    fn series(pairs: &[(String, i64)]) -> Option<Vec<(Date, i64)>> {
        let mut series: Vec<(Date, i64)> = Vec::with_capacity(pairs.len());
        for (day, per_unit_e4) in pairs {
            let day = Date::from_iso(day)?;
            let fits = (1..=MAX_RATE_E4).contains(per_unit_e4);
            let later = series.last().is_none_or(|(before, _)| *before < day);
            if !fits || !later {
                return None;
            }
            series.push((day, *per_unit_e4));
        }
        Some(series)
    }

    /// Курсы валюты `currency`; у драма курса нет.
    fn of(&self, currency: Currency) -> &[(Date, i64)] {
        match currency {
            Currency::Amd => &[],
            Currency::Usd => &self.usd,
            Currency::Eur => &self.eur,
        }
    }

    /// Курс валюты `currency`, опубликованный ЦБ в день `day`, — драмы за
    /// единицу в десятитысячных. `None` — в файле его нет.
    pub fn published(&self, currency: Currency, day: Date) -> Option<i64> {
        let series = self.of(currency);
        series
            .binary_search_by_key(&day, |(published, _)| *published)
            .ok()
            .map(|at| series[at].1)
    }
}

/// Что сказать у поля курса.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Hint {
    /// Курс ЦБ, опубликованный в день `published`, — драмы за единицу
    /// в десятитысячных: его берёт закон.
    Rate { published: Date, per_unit_e4: i64 },
    /// Закон берёт курс дня `published`, а в файле курсы кончаются раньше:
    /// он ещё не опубликован или не дошёл до устройства.
    NotYet { published: Date },
    /// Закон берёт курс дня `published`, а в файле его нет, хотя курсы
    /// и позже есть: файл с пропуском или начинается позже, — или ЦБ
    /// публиковал курс и между ним и днём поставки, то есть календарь файла
    /// правил разошёлся с ЦБ. Курс человек сверяет на cba.am сам.
    Missing { published: Date },
}

/// Подсказка у поля курса суммы в валюте `currency` с днём поставки
/// `supply` — по правилу дня курса на этот день (шапка модуля). `rates` —
/// курсы с устройства, `None` — их нет. `None` — подсказки нет: драмы,
/// правила на этот день нет (до 2026-07-01) или рабочего дня перед ним не
/// нашлось.
pub fn hint(rates: Option<&Rates>, currency: Currency, supply: Date) -> Option<Hint> {
    hint_by(rules::armenia(), rates, currency, supply)
}

fn hint_by(rules: &Rules, rates: Option<&Rates>, currency: Currency, supply: Date) -> Option<Hint> {
    match currency {
        Currency::Amd => return None,
        Currency::Usd | Currency::Eur => {}
    }
    let rule = rules::on(rules.rate_day(), supply)?;
    let published = match rule.day {
        RateDayKind::PreviousWorkingDay => deadlines::previous_working(rules, supply)?,
    };
    let series = rates.map_or(&[][..], |rates| rates.of(currency));
    // ЦБ публиковал курс между днём по календарю и днём поставки — календарь
    // файла правил отстал от решения правительства или закона о праздниках
    // (рабочая суббота, отменённый праздник), и какой день рабочий, тогда
    // неизвестно: курс дня календаря выдавать нельзя (ревью задачи 43). На
    // настоящих данных так не бывает: дни публикаций ЦБ с 2025-01-03 по
    // 2026-11-16 совпали с календарём все (`deadlines::previous_working`).
    let after = series.partition_point(|(day, _)| *day <= published);
    if series.get(after).is_some_and(|(day, _)| *day < supply) {
        return Some(Hint::Missing { published });
    }
    if let Some(per_unit_e4) = rates.and_then(|rates| rates.published(currency, published)) {
        return Some(Hint::Rate {
            published,
            per_unit_e4,
        });
    }
    let later = series.last().is_some_and(|(last, _)| *last > published);
    Some(if later {
        Hint::Missing { published }
    } else {
        Hint::NotYet { published }
    })
}

#[cfg(test)]
mod tests;
