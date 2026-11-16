//! Контора (задача 38): список ИП в программе — записи [`Ip`].
//!
//! У каждого ИП своя база IndexedDB (разбор задачи 38, 2026-11-05): первая —
//! [`FIRST_BASE`] навсегда, следующие — `kvartik-ip-<id>`, где id —
//! идентификатор записи ИП здесь. Номера документов, налог, выпуск и копия
//! одного ИП так живут в своей базе без правок: одна база — один ИП.
//! Список лежит в своей базе устройства (окно, `store/office.rs`) и идёт
//! только в полную копию: в копии одного ИП, отданной клиенту, ни списка,
//! ни ключа Pro нет по построению.
//!
//! **Имя базы — местное.** У каждого устройства своя `kvartik`: первый ИП
//! одного устройства на другом ляжет в базу по идентификатору записи.
//! Поэтому копия всех ИП метит записи баз идентификатором записи ИП, а не
//! именем базы, а запись ИП из копии сливается со своей, оставляя своё имя
//! базы ([`Ip::merged`]).
//!
//! Имя и ՀՎՀՀ в записи — копия из профиля базы для списка: открывать
//! каждую базу ради имён незачем (Правило 1). Её обновляет окно, когда
//! профиль открытой базы прочитан или сохранён ([`Ip::copy_profile`]);
//! расхождение — устаревший список, а не порча. Идентификатор профиля —
//! чтобы копию одного ИП узнать среди всех ИП программы.
//!
//! В данных: `{"meta":{…},"base":"kvartik","profile":"…"|null,
//! "name":{"hy":…,"ru":…,"en":…},"tin":"…"|null,"added":"2026-11-05",
//! "added_at":<момент>|null}` — момента нет у записей до 0.0.134, и они
//! читаются без него; образец — в тесте `data_shape_is_pinned`.
//!
//! **Порядок списка — порядок добавления** ([`alive`]): по дню, а в один
//! день — по моменту. По нему ключ «Pro» отдаёт места (`license::seat`), и
//! с одним днём на всех места внутри дня решал бы случайный идентификатор:
//! ИП, добавленный позже, мог бы отнять место у сидящего (ревью задачи 38).
//! Момента нет у первого ИП программы — он жил в ней до конторы и встаёт
//! раньше добавленных в тот же день — и у ИП, добавленных до 0.0.134: между
//! собой они — по идентификатору, как раньше.

use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize};

use super::date::Date;
use super::profile::{Localized, Profile};
use super::record::{RecordId, RecordMeta, Timestamp};
use super::text::Line;
use super::tin::Tin;
use crate::format::Stored;
use crate::i18n::Lang;

/// База первого ИП — та, в которой жили данные до конторы. Имя — навсегда:
/// данные привязаны к нему и к адресу сайта (CLAUDE.md, «Происхождение
/// сайта»).
pub const FIRST_BASE: &str = "kvartik";

/// Начало имени базы следующих ИП: `kvartik-ip-<id>`.
const BASE_PREFIX: &str = "kvartik-ip-";

/// Больше ИП в программе не заводится: накопители ограничены (Правило 1),
/// а полная копия и сводка открывают каждую базу. «Pro» — до 30 ИП (PDF),
/// предел — с запасом втрое.
pub const MAX_IPS: usize = 100;

/// Годно ли имя базы ИП: [`FIRST_BASE`] или `kvartik-ip-<id>` с id в той
/// записи, какой его пишет `RecordId`. Негодное имя окно не открывает:
/// `indexedDB.open` молча создал бы пустую базу (Правило 6).
pub fn is_base_name(name: &str) -> bool {
    name == FIRST_BASE
        || name
            .strip_prefix(BASE_PREFIX)
            .is_some_and(|id| RecordId::parse(id).is_some())
}

/// ИП в программе — запись конторы.
#[derive(Clone, PartialEq, Eq, Debug, Serialize)]
pub struct Ip {
    meta: RecordMeta,
    base: String,
    profile: Option<RecordId>,
    name: Localized<Line>,
    tin: Option<Tin>,
    added: Date,
    /// Момент добавления (с 0.0.134) — порядок внутри дня (шапка модуля);
    /// `None` — первый ИП программы или добавленный раньше. Пишется всегда,
    /// и `null` тоже: своя запись «без ключа» связала бы запись ИП с кодом
    /// записи документов, и разрез потянул бы его в часть конторы при запуске
    /// (замер задачи 38).
    added_at: Option<Timestamp>,
}

/// Поля записи, как они лежат в данных, — до проверки имени базы.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IpFields {
    meta: RecordMeta,
    base: String,
    profile: Option<RecordId>,
    name: Localized<Line>,
    tin: Option<Tin>,
    added: Date,
    #[serde(default)]
    added_at: Option<Timestamp>,
}

impl<'de> Deserialize<'de> for Ip {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let IpFields {
            meta,
            base,
            profile,
            name,
            tin,
            added,
            added_at,
        } = IpFields::deserialize(deserializer)?;
        if !is_base_name(&base) {
            return Err(de::Error::custom("имя базы ИП не годится"));
        }
        Ok(Self {
            meta,
            base,
            profile,
            name,
            tin,
            added,
            added_at,
        })
    }
}

impl Stored for Ip {
    const KIND: &'static str = "ip";

    fn meta(&self) -> &RecordMeta {
        &self.meta
    }
}

impl Ip {
    /// Первый ИП программы — тот, что жил в ней до конторы: база
    /// [`FIRST_BASE`], имя и ՀՎՀՀ — из его профиля, если он есть.
    pub fn first(profile: Option<&Profile>, today: Date, now: Timestamp) -> Self {
        let mut ip = Self {
            meta: RecordMeta::new(now),
            base: FIRST_BASE.to_owned(),
            profile: None,
            name: Localized::default(),
            tin: None,
            added: today,
            // Он жил в программе до конторы: раньше добавленных с ним в один
            // день (шапка модуля).
            added_at: None,
        };
        if let Some(profile) = profile {
            ip.fill(profile);
        }
        ip
    }

    /// Новый ИП: своя, ещё пустая база под идентификатором записи.
    pub fn new(today: Date, now: Timestamp) -> Self {
        let meta = RecordMeta::new(now);
        Self {
            base: format!("{BASE_PREFIX}{}", meta.id()),
            meta,
            profile: None,
            name: Localized::default(),
            tin: None,
            added: today,
            added_at: Some(now),
        }
    }

    /// ИП из копии всех ИП, которого на устройстве нет: та же запись под
    /// тем же идентификатором, а база — своя, по идентификатору (у
    /// устройства своя [`FIRST_BASE`]).
    pub fn arrived(incoming: &Self) -> Self {
        Self {
            base: format!("{BASE_PREFIX}{}", incoming.meta.id()),
            ..incoming.clone()
        }
    }

    /// ИП из копии всех ИП — в пустую базу этого устройства `base`: у
    /// свежего устройства есть своя пустая [`FIRST_BASE`], и первый ИП копии
    /// ложится в неё, а не рядом с ней (`format::office::route`).
    pub fn arrived_in(incoming: &Self, base: &str) -> Self {
        Self {
            base: base.to_owned(),
            ..incoming.clone()
        }
    }

    /// Запись этого ИП после слияния с той же записью из копии: из двух
    /// остаётся изменённая позже, как у любой записи (`backup::merge`), но
    /// имя базы — своё, местное. `None` — своя не старше: писать нечего.
    pub fn merged(&self, incoming: &Self) -> Option<Self> {
        (incoming.meta.id() == self.meta.id() && incoming.meta.modified() > self.meta.modified())
            .then(|| Self {
                base: self.base.clone(),
                ..incoming.clone()
            })
    }

    /// Обновить копию имени, ՀՎՀՀ и идентификатора профиля из профиля базы.
    /// `false` — всё уже такое же, и записывать нечего: запись не
    /// становится «новее» без настоящей правки.
    pub fn copy_profile(&mut self, profile: &Profile, now: Timestamp) -> bool {
        let same = self.profile == Some(profile.id())
            && self.name == profile.data().name
            && self.tin == profile.data().tin;
        if !same {
            self.fill(profile);
            self.meta.touch(now);
        }
        !same
    }

    fn fill(&mut self, profile: &Profile) {
        self.profile = Some(profile.id());
        self.name = profile.data().name.clone();
        self.tin = profile.data().tin.clone();
    }

    /// Убрать ИП из списка. База не стирается: «Вернуть» показывает её
    /// снова со всеми данными.
    pub fn remove(&mut self, now: Timestamp) {
        self.meta.delete(now);
    }

    /// Вернуть ИП в список.
    pub fn bring_back(&mut self, now: Timestamp) {
        self.meta.restore(now);
    }

    /// Постоянный идентификатор — им копия всех ИП метит записи этой базы.
    pub const fn id(&self) -> RecordId {
        self.meta.id()
    }

    /// Убран ли из списка.
    pub const fn is_removed(&self) -> bool {
        self.meta.is_deleted()
    }

    /// Имя базы IndexedDB на этом устройстве.
    pub fn base(&self) -> &str {
        &self.base
    }

    /// Идентификатор профиля ИП; `None` — профиль ещё не прочитан.
    pub const fn profile(&self) -> Option<RecordId> {
        self.profile
    }

    /// Имя на языке `lang`, а без него — на первом языке, где оно есть;
    /// пусто — имени ещё нет.
    pub fn name(&self, lang: Lang) -> &str {
        std::iter::once(lang)
            .chain(Lang::ALL)
            .map(|lang| self.name.get(lang).as_str())
            .find(|name| !name.is_empty())
            .unwrap_or_default()
    }

    /// ՀՎՀՀ; `None` — в профиле не указан.
    pub const fn tin(&self) -> Option<&Tin> {
        self.tin.as_ref()
    }

    /// День, когда ИП завели в программе.
    pub const fn added(&self) -> Date {
        self.added
    }
}

/// Живые ИП списка в порядке добавления (шапка модуля): по дню, в один
/// день — по моменту, а без момента — раньше и по идентификатору, чтобы
/// порядок не зависел от того, как отдала база.
pub fn alive(ips: &[Ip]) -> Vec<&Ip> {
    let mut alive: Vec<&Ip> = ips.iter().filter(|ip| !ip.is_removed()).collect();
    alive.sort_by_key(|ip| (ip.added(), ip.added_at, ip.id()));
    alive
}

/// Место ИП с базой `base` среди живых ИП списка ([`alive`]): с нуля, в
/// порядке добавления. По нему ключ «Pro» открывает первых N ИП
/// (`license::seat`; решение человека 2026-11-05): убранный места не
/// занимает. `None` — среди живых его нет.
///
/// Счётом тех, кто раньше, а не сортировкой: место считают все части окна,
/// которые считают срок, и сортировка списка (её зовёт и часть конторы
/// при запуске) стала бы их общим кодом — разрез переложил бы его, и кусок,
/// нужный части конторы при запуске, вырос бы (замер задачи 38). Порядок —
/// тот же, что у [`alive`]: идентификаторы различны.
pub fn place(ips: &[Ip], base: &str) -> Option<usize> {
    let order = |ip: &Ip| (ip.added, ip.added_at, ip.id());
    let me = ips.iter().find(|ip| !ip.is_removed() && ip.base() == base)?;
    Some(
        ips.iter()
            .filter(|ip| !ip.is_removed() && order(ip) < order(me))
            .count(),
    )
}

/// Базу какого ИП открыть: первую из запомненных `remembered` (вкладкой,
/// потом устройством), если она у живого ИП списка; иначе — базу живого ИП,
/// добавленного раньше всех; живых нет — [`FIRST_BASE`]. Убранного ИП окно
/// само не открывает — его возвращают из списка, — а имени, которого нет
/// в списке, не открывает никогда: `indexedDB.open` создал бы под ним пустую
/// базу (Правило 6).
pub fn choose<'a>(ips: &'a [Ip], remembered: &[Option<&str>]) -> &'a str {
    let alive = alive(ips);
    remembered
        .iter()
        .flatten()
        .find_map(|name| alive.iter().find(|ip| ip.base() == *name))
        .or_else(|| alive.first())
        .map_or(FIRST_BASE, |ip| ip.base())
}

#[cfg(test)]
mod tests;
