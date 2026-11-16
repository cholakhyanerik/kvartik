//! Маршрут копии всех ИП (задача 38): какой ИП файла в какую базу этого
//! устройства ложится и что пишется в список конторы.
//!
//! Имя базы — местное (`model::office`): у каждого устройства своя
//! `kvartik`, поэтому копия метит записи идентификатором записи ИП, а здесь
//! он превращается в базу этого устройства. По порядку:
//!
//! 1. Та же запись ИП в списке — её база; в список ложится запись,
//!    изменённая позже (`Ip::merged`), с местным именем базы.
//! 2. Иначе тот же профиль или тот же ՀՎՀՀ у ИП списка — его база: ИП
//!    завели на двух устройствах независимо. Вторая запись того же ИП в
//!    список не идёт — он показал бы одного ИП дважды.
//! 3. Иначе — новая база по идентификатору записи (`Ip::arrived`) и новая
//!    запись в списке, с меткой «убран», если она была у записи копии.
//!    Есть у устройства свободная пустая база (`spare`: свежее устройство,
//!    списка ещё нет, а своя `kvartik` пуста) — в неё ложится такой ИП: иначе
//!    пустая база осталась бы в программе безымянным ИП или вне списка, а то,
//!    что в неё впишут, стало бы невидимым. Чей она: ИП, у которого на
//!    устройстве копии база называлась так же (первый ИП того устройства, —
//!    имя базы источника лежит в его записи), потом ИП, чья это база по
//!    идентификатору (`Ip::arrived`), иначе — добавленный раньше всех, тот,
//!    кого окно и откроет первым (`office::choose`). Добавленных в один день
//!    различает только идентификатор — случай, поэтому сначала имя.
//!
//! Всё решается до первой записи (решение человека 2026-11-05): маршрут —
//! чистая функция, а окно потом кладёт каждую базу своей транзакцией, и
//! повтор восстановления безвреден — слияние по идентификатору записи.
//!
//! Два ИП копии в одной базе — тот же ИП, заведённый в источнике дважды, —
//! могут нести одну и ту же запись в двух версиях. В базу идёт одна —
//! изменённая позже, как при любом слиянии ([`latest`]): иначе победила бы
//! последняя в файле (ревью 2026-11-06).
//!
//! Ключи «Pro» копии (`license::OfficeLicense`) — записи конторы, а не ИП:
//! они идут в базу конторы и сливаются с ней, как любые записи, по
//! идентификатору ([`Route::keys`]), и метка базы, если она у них есть, ничего
//! не значит. Ни в одну базу ИП они не ложатся: из неё ключ ушёл бы с копией
//! одного ИП к клиенту. Больше `license::MAX_KEYS` их в копии не бывает,
//! как и ключей «Стандарт» у одного ИП копии (с 0.0.135), — такую маршрут не
//! кладёт ([`RouteError::TooManyKeys`]), до первой записи. А сколько ключей
//! станет вместе с ключами базы, окно сверяет в транзакции этой базы.
//!
//! Копия одного ИП маршрута не знает: её окно кладёт в открытую базу. Но
//! если это копия ИП, который в списке есть, а открыт другой, окно говорит
//! об этом, а не заводит ИП второй раз ([`listed`]).

use std::collections::{BTreeMap, BTreeSet};

use super::backup::Entry;
use super::{FORMAT_VERSION, FormatError, Stored, decode_record};
use crate::license::{License, MAX_KEYS, OfficeLicense};
use crate::model::office::{self, Ip, MAX_IPS};
use crate::model::profile::Profile;
use crate::model::record::RecordId;

/// Куда ложатся записи одного ИП копии.
#[derive(Debug)]
pub struct Landing {
    /// База этого устройства.
    pub base: String,
    /// Базы ещё нет: окно её заведёт.
    pub new: bool,
    /// Записи ИП — без меток копии, в той же последовательности, что в ней.
    pub entries: Vec<Entry>,
}

/// Маршрут копии всех ИП.
#[derive(Debug)]
pub struct Route {
    /// Записи списка конторы, которые лягут: новые ИП и записи, изменённые
    /// в копии позже.
    pub office: Vec<Ip>,
    /// Ключи «Pro» копии — без меток, по одной версии каждой записи: окно
    /// сливает их с базой конторы (шапка модуля).
    pub keys: Vec<Entry>,
    /// Базы и их записи — по одной транзакции на базу.
    pub bases: Vec<Landing>,
}

/// Почему копию всех ИП не положить.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum RouteError {
    /// Запись ИП в копии не читается (её проверил разбор файла, так что
    /// это порча).
    Format(FormatError),
    /// С копией ИП в программе стало бы больше [`MAX_IPS`]: столько
    /// вышло бы.
    TooMany(usize),
    /// В копии записей ключей «Pro» — или ключей одного ИП — больше
    /// `license::MAX_KEYS`: столько. Программа такую не пишет, а проверка
    /// подписи каждого держала бы страницу (ревью задачи 38).
    TooManyKeys(usize),
}

impl From<FormatError> for RouteError {
    fn from(error: FormatError) -> Self {
        Self::Format(error)
    }
}

/// Маршрут записей копии всех ИП `incoming` (разобранных файлом,
/// `backup::read`) для списка этого устройства `local` — живые и убранные
/// ИП — и свободной пустой базы `spare` (шапка модуля, п. 3). Записи без
/// метки базы и записи конторы в базы не идут.
pub fn route(incoming: Vec<Entry>, local: &[Ip], spare: Option<&str>) -> Result<Route, RouteError> {
    let mut ips = Vec::new();
    let mut keys = Vec::new();
    let mut entries = Vec::new();
    for entry in incoming {
        if entry.kind() == Ip::KIND {
            ips.push(decode_record::<Ip>(FORMAT_VERSION, entry.record())?);
        } else if entry.kind() == OfficeLicense::KIND {
            keys.push(entry.untagged());
        } else {
            entries.push(entry);
        }
    }
    let ours = |theirs: &Ip| {
        local
            .iter()
            .find(|ours| ours.id() == theirs.id())
            .or_else(|| {
                local.iter().find(|ours| {
                    ours.profile().is_some() && ours.profile() == theirs.profile()
                })
            })
            .or_else(|| {
                local
                    .iter()
                    .find(|ours| ours.tin().is_some() && ours.tin() == theirs.tin())
            })
    };
    // Кому свободная база (шапка модуля, п. 3): ИП без пары здесь, у кого
    // она так и называлась, потом — чья она по идентификатору (иначе два ИП
    // легли бы в одну базу), а нет таких — живому, добавленному раньше всех.
    let homeless: Vec<Ip> = ips.iter().filter(|theirs| ours(theirs).is_none()).cloned().collect();
    let spare_for = spare.and_then(|spare| {
        homeless
            .iter()
            .find(|theirs| theirs.base() == spare)
            .or_else(|| homeless.iter().find(|theirs| Ip::arrived(theirs).base() == spare))
            .or_else(|| office::alive(&homeless).first().copied())
            .map(Ip::id)
    });
    let mut office = Vec::new();
    let mut bases: Vec<Landing> = Vec::new();
    // ИП копии → база этого устройства.
    let mut targets: Vec<(RecordId, usize)> = Vec::new();
    for theirs in &ips {
        let (base, new) = match (ours(theirs), spare) {
            (Some(ours), _) => {
                if let Some(merged) = ours.merged(theirs) {
                    office.push(merged);
                }
                (ours.base().to_owned(), false)
            }
            (None, Some(spare)) if spare_for == Some(theirs.id()) => {
                office.push(Ip::arrived_in(theirs, spare));
                (spare.to_owned(), false)
            }
            (None, _) => {
                let arrived = Ip::arrived(theirs);
                let base = arrived.base().to_owned();
                office.push(arrived);
                (base, true)
            }
        };
        let at = match bases.iter().position(|landing| landing.base == base) {
            Some(at) => at,
            None => {
                bases.push(Landing {
                    base,
                    new,
                    entries: Vec::new(),
                });
                bases.len() - 1
            }
        };
        targets.push((theirs.id(), at));
    }
    let added = office
        .iter()
        .filter(|ip| !local.iter().any(|ours| ours.id() == ip.id()))
        .count();
    if local.len() + added > MAX_IPS {
        return Err(RouteError::TooMany(local.len() + added));
    }
    for entry in entries {
        let Some(base) = entry.base() else {
            continue;
        };
        if let Some(&(_, at)) = targets.iter().find(|(id, _)| *id == base) {
            bases[at].entries.push(entry.untagged());
        }
    }
    for landing in &mut bases {
        landing.entries = latest(std::mem::take(&mut landing.entries));
        let keys = landing
            .entries
            .iter()
            .filter(|entry| entry.kind() == License::KIND)
            .count();
        if keys > MAX_KEYS {
            return Err(RouteError::TooManyKeys(keys));
        }
    }
    let keys = latest(keys);
    if keys.len() > MAX_KEYS {
        return Err(RouteError::TooManyKeys(keys.len()));
    }
    Ok(Route { office, keys, bases })
}

/// Одна запись — одна: из версий одной записи (вид и идентификатор) в одной
/// базе остаётся изменённая позже (шапка модуля), на месте первой.
fn latest(entries: Vec<Entry>) -> Vec<Entry> {
    let mut kept: Vec<Entry> = Vec::with_capacity(entries.len());
    let mut at: BTreeMap<(&'static str, RecordId), usize> = BTreeMap::new();
    for entry in entries {
        let key = (entry.kind(), entry.meta().id());
        match at.get(&key) {
            Some(&i) if kept[i].meta().modified() >= entry.meta().modified() => {}
            Some(&i) => kept[i] = entry,
            None => {
                at.insert(key, kept.len());
                kept.push(entry);
            }
        }
    }
    kept
}

/// Записи ИП из записей конторы — например, из выгрузки базы конторы для
/// копии всех ИП: список и записи файла тогда из одной транзакции, и ИП,
/// добавленный между двумя чтениями, не оставит в файле меток без своего ИП
/// (ревью 2026-11-06).
pub fn ips(entries: &[Entry]) -> Result<Vec<Ip>, FormatError> {
    entries
        .iter()
        .filter(|entry| entry.kind() == Ip::KIND)
        .map(|entry| decode_record::<Ip>(FORMAT_VERSION, entry.record()))
        .collect()
}

/// ИП списка `local`, чья это копия одного ИП `incoming`: тот же профиль или
/// тот же ՀՎՀՀ, что у профиля копии. `None` — ИП копии в списке нет.
pub fn listed<'a>(incoming: &[Entry], local: &'a [Ip]) -> Result<Option<&'a Ip>, FormatError> {
    let mut ids = BTreeSet::new();
    let mut tins = Vec::new();
    for profile in live_profiles(incoming)? {
        ids.insert(profile.id());
        tins.extend(profile.data().tin.clone());
    }
    Ok(local.iter().find(|ip| {
        ip.profile().is_some_and(|id| ids.contains(&id)) || ip.tin().is_some_and(|tin| tins.contains(tin))
    }))
}

/// Профиль копии одного ИП, которую кладут новым ИП: живой, изменённый
/// последним, — тот, с которым окно будет работать в его базе
/// (`Profile::current`). Из него в списке — имя, ՀՎՀՀ и запись профиля нового
/// ИП: без них он был бы «ИП без имени», пока его не откроют, а та же копия
/// ещё раз его не узнала бы ([`listed`]) и легла бы новым ИП второй раз
/// (найдено глазным прогоном задачи 38, 2026-11-06). `None` — живого профиля
/// в копии нет.
pub fn profile(incoming: &[Entry]) -> Result<Option<Profile>, FormatError> {
    Ok(Profile::current(live_profiles(incoming)?))
}

/// Живые профили копии.
fn live_profiles(incoming: &[Entry]) -> Result<Vec<Profile>, FormatError> {
    let mut live = Vec::new();
    for entry in incoming.iter().filter(|entry| entry.kind() == Profile::KIND) {
        let profile = decode_record::<Profile>(FORMAT_VERSION, entry.record())?;
        if !profile.meta().is_deleted() {
            live.push(profile);
        }
    }
    Ok(live)
}

#[cfg(test)]
mod tests;
