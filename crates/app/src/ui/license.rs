//! Лицензия и пробный период (задача 25): ключ на ИНН профиля, срок и что
//! без него заперто — выпуск счетов и актов и числа для расчёта
//! (`kvartik_core::license`, решение человека 2026-10-27).
//!
//! Главному экрану ключи не нужны, и в главном wasm их нет: ключи читают
//! и подпись проверяют части, которым они нужны (Правило 1), — форма
//! документа (выпуск), форма профиля (ИНН ключа), числа для расчёта
//! и экран лицензии с карточкой на главном — из части книги
//! (`ledger::book_part`): карточка налога и так грузит её, когда в базе
//! есть данные. Своя часть у экрана лицензии завела бы новые общие куски
//! (CLAUDE.md, Правило 6). Проверка Ed25519 — 22,5 КБ после brotli —
//! легла общим куском частей книги и документов; чтение ключей в главном
//! wasm стоило бы ему 4,5 КБ (замер 0.0.60, docs/measurements.md).
//!
//! День начала пробного периода помнит устройство (`store/device.rs`),
//! а ключ — база: ключ выдан на ИП, а не на устройство, и переезжает с копией.

use std::sync::Arc;

use kvartik_core::i18n::{self, Key, Lang, fill, plural, t};
use kvartik_core::license::{self, Access, KeyError, License, PUBLIC_KEY_BYTES, Plan, Term};
use kvartik_core::model::date::{Date, Quarter};
use kvartik_core::model::tin::Tin;
use leptos::ev::SubmitEvent;
use leptos::prelude::*;

use super::icon::{Icon, icon};
use super::ledger::tax_part;
use super::profile::Profiles;
use super::route::{Router, Screen};
use super::split::screen;
use super::storage::error_key;
use super::tax::{self, Part};
use super::{Base, tr};
use crate::platform::clock;
use crate::store::device::Device;
use crate::store::{Store, StoreError, Writes};

/// За столько дней до конца ключа окно напоминает его продлить: месяц —
/// время написать автору и получить новый ключ, не прерывая выпуск.
const RENEW_DAYS: i32 = 30;

/// Открытый ключ, которым окно проверяет подпись: ключ автора.
#[cfg(not(all(test, target_arch = "wasm32")))]
const fn author() -> [u8; PUBLIC_KEY_BYTES] {
    license::PUBLIC_KEY
}

/// В браузерных тестах — ключ пары проб: подписать ключ закрытым ключом
/// автора тест не может (Правило 9), а окно под тестом то же.
#[cfg(all(test, target_arch = "wasm32"))]
fn author() -> [u8; PUBLIC_KEY_BYTES] {
    super::tests::license::signer().verifying_key().to_bytes()
}

/// Введённые ключи и пробный период — одни на окно.
#[derive(Clone, Copy)]
pub struct Licenses {
    state: RwSignal<State>,
    /// Следит ли уже кто-то за базой (`watch`).
    watched: StoredValue<bool>,
    /// Владелец корня окна: слежение за базой живёт у него, а не у того,
    /// кто позвал `watch` первым, — первой может позвать форма, а она
    /// закрывается (ревью задачи 25).
    root: StoredValue<Option<Owner>>,
    /// День начала пробного периода на этом устройстве; `None` —
    /// устройство его не помнит и часы без даты.
    trial: Option<Date>,
    /// Открытый ключ, которым проверяется подпись: в программе —
    /// [`PUBLIC_KEY`], в тестах — ключ проб.
    public: [u8; PUBLIC_KEY_BYTES],
}

/// Где ключи.
#[derive(Clone, PartialEq, Debug)]
enum State {
    Loading,
    /// Прочитаны — все, и сколько не читается.
    Ready {
        licenses: Arc<Vec<License>>,
        unreadable: usize,
    },
    Failed(StoreError),
}

impl Licenses {
    /// Ключи окна и пробный период: начат ли он на этом устройстве, а нет —
    /// начинается сегодня. Не запомнилось (`localStorage` отказал) — период
    /// начнётся заново при следующем открытии: бороться с этим не нужно
    /// (`kvartik_core::license`).
    pub fn start(today: Option<Date>) -> Self {
        let device = Device::new();
        let trial = device.trial_start().or_else(|| {
            let today = today?;
            device.set_trial_start(today);
            Some(today)
        });
        Self {
            state: RwSignal::new(State::Loading),
            watched: StoredValue::new(false),
            root: StoredValue::new(Owner::current()),
            trial,
            public: author(),
        }
    }

    /// Читать ключи, когда база открылась и когда её заменило
    /// восстановление из копии: ключ переезжает с копией. Зовут части,
    /// которым ключи нужны, а слежение одно и живёт у корня окна, до его
    /// конца: эффект Leptos умирает с владельцем, а первой может позвать
    /// форма профиля или документа — после её закрытия восстановленный
    /// ключ остался бы непрочитанным (ревью задачи 25).
    pub(super) fn watch(&self, base: Base) {
        if self.watched.get_value() {
            return;
        }
        self.watched.set_value(true);
        let licenses = *self;
        let follow = move || {
            Effect::watch(
                move || (base.is_open(), base.replacements()),
                move |(open, _), _, _| {
                    if !*open {
                        return;
                    }
                    if let Some(store) = base.get() {
                        leptos::task::spawn_local(async move { licenses.load(&store).await });
                    }
                },
                true,
            );
        };
        match self.root.get_value() {
            Some(root) => root.with(follow),
            None => follow(),
        }
    }

    /// Прочитать ключи.
    async fn load(&self, store: &Store) {
        let read = async { store.read()?.list::<License>().await };
        self.state.set(match read.await {
            Ok(listed) => State::Ready {
                licenses: Arc::new(listed.records),
                unreadable: listed.unreadable,
            },
            Err(error) => State::Failed(error),
        });
    }

    /// Записанный ключ — в конец списка.
    fn put(&self, license: License) {
        self.state.update(|state| {
            if let State::Ready { licenses, .. } = state {
                Arc::make_mut(licenses).push(license);
            }
        });
    }

    /// Что-то, посчитанное по ключам; пока их нет — значение по умолчанию.
    /// Отслеживаемое.
    pub(super) fn with_keys<T: Default>(&self, read: impl FnOnce(&[License]) -> T) -> T {
        self.state.with(|state| match state {
            State::Ready { licenses, .. } => read(licenses),
            State::Loading | State::Failed(_) => T::default(),
        })
    }

    /// Прочитаны ли ключи — отслеживаемое: пока нет, выпуск не запирается,
    /// иначе у купившего ключ мигали бы слова о запертом выпуске.
    pub(super) fn ready(&self) -> bool {
        self.state.with(|state| matches!(state, State::Ready { .. }))
    }

    /// Что открыто — по ключам на ИНН профиля и пробному периоду.
    /// Отслеживаемое. Проверяет подпись каждого ключа — зовут только части
    /// (шапка модуля); пока ключи читаются, считается один пробный период.
    pub(super) fn access(&self, profiles: Profiles) -> Access {
        let tin = profiles.with(|data| data.and_then(|data| data.tin.clone()));
        self.state.with(|state| {
            let licenses: &[License] = match state {
                State::Ready { licenses, .. } => licenses,
                State::Loading | State::Failed(_) => &[],
            };
            Access::new(&self.public, License::keys(licenses), tin.as_ref(), self.trial)
        })
    }

    /// Введён ли ключ, выданный на `tin`, — по записи, без проверки
    /// подписи: форма профиля предупреждает, что ключ на прежний ИНН
    /// с новым не подойдёт. Отслеживаемое.
    pub(super) fn issued_to(&self, tin: &Tin) -> bool {
        self.with_keys(|licenses| License::keys(licenses).any(|key| key.tin() == tin))
    }

    /// Почему ключи не прочитались — отслеживаемое.
    fn failure(&self) -> Option<StoreError> {
        self.state.with(|state| match state {
            State::Failed(error) => Some(*error),
            State::Loading | State::Ready { .. } => None,
        })
    }

    /// Сколько записей ключей не читается — отслеживаемое.
    fn unreadable(&self) -> usize {
        self.state.with(|state| match state {
            State::Ready { unreadable, .. } => *unreadable,
            State::Loading | State::Failed(_) => 0,
        })
    }
}

/// Строка «Лицензия» в группе «Программа» на экране профиля — ссылка на
/// экран лицензии. Без срока рядом: срок — это проверка подписи, а ей
/// в главном wasm не место (шапка модуля).
#[component]
pub fn LicenseEntry(lang: ReadSignal<Lang>) -> impl IntoView {
    view! {
        <a class="entry" href=Screen::License.href()>
            <span class="entry-main">
                <span class="entry-title">{tr(lang, Key::LicenseTitle)}</span>
            </span>
            {icon(Icon::Forward)}
        </a>
    }
}

/// Экран лицензии на языке окна: «назад» к профилю и заголовок — сразу,
/// остальное — частью книги с первого показа (`split::screen`).
#[component]
pub fn LicenseScreen(ctx: tax::Ctx, router: Router) -> impl IntoView {
    let lang = ctx.lang;
    view! {
        <a class="back" href=Screen::Profile.href()>
            {icon(Icon::Back)}
            <span>{tr(lang, Key::NavProfile)}</span>
        </a>
        <h1 tabindex="-1">{tr(lang, Key::LicenseTitle)}</h1>
        {screen(lang, router, Screen::License, move || tax_part(ctx, Part::License))}
    }
}

/// Число дней — «14 дней», «1 день».
fn days(lang: Lang, n: i32) -> String {
    fill(
        plural(lang, u64::from(n.unsigned_abs()), Key::DaysOne, Key::DaysFew, Key::DaysMany),
        &[&i18n::integer(lang, i64::from(n))],
    )
}

/// Имя тарифа.
const fn plan_key(plan: Plan) -> Key {
    match plan {
        Plan::Standard => Key::PlanStandard,
        Plan::Pro => Key::PlanPro,
    }
}

/// Срок словами: что идёт и до какого дня, а кончился — когда.
pub(super) fn term_words(lang: Lang, access: Access, today: Date) -> String {
    let date = |day| i18n::date(lang, day);
    match (access.term(), access.days_left(today)) {
        (Some(Term::Trial { last }), Some(left)) => {
            fill(t(lang, Key::LicenseTrialUntil), &[&date(last), &days(lang, left)])
        }
        (Some(Term::Key { plan, last }), Some(_)) => {
            fill(t(lang, Key::LicenseKeyUntil), &[t(lang, plan_key(plan)), &date(last)])
        }
        (Some(Term::Trial { last }), None) => fill(t(lang, Key::LicenseTrialEnded), &[&date(last)]),
        (Some(Term::Key { last, .. }), None) => fill(t(lang, Key::LicenseKeyEnded), &[&date(last)]),
        (None, _) => t(lang, Key::LicenseNone).to_owned(),
    }
}

/// Какие кварталы открыты и после срока — «числа кварталов, кончившихся до
/// 9 ноября 2026 года включительно». Без срока — ничего.
fn quarters_open(lang: Lang, access: Access) -> Option<String> {
    access
        .term()
        .map(|term| fill(t(lang, Key::LicenseQuartersOpen), &[&i18n::date(lang, term.last())]))
}

/// Ссылка на экран лицензии — из слов о запертом.
#[component]
pub(super) fn LicenseLink(lang: ReadSignal<Lang>) -> impl IntoView {
    view! { <a class="button" href=Screen::License.href()>{tr(lang, Key::LicenseEnter)}</a> }
}

/// Карточка на главном экране: пробный период — сколько осталось, ключ —
/// когда пора продлить, кончился срок — что заперто. Ключ идёт, и до конца
/// дальше месяца — карточки нет. Из части книги, как карточка налога.
#[component]
pub(super) fn LicenseCard(ctx: tax::Ctx) -> impl IntoView {
    let tax::Ctx {
        lang,
        sources,
        base,
        licenses,
        today,
        ..
    } = ctx;
    licenses.watch(base);
    let access = Memo::new(move |_| licenses.access(sources.profiles));
    move || {
        let today = today?;
        // Пока ключи читаются — карточки нет: у купившего ключ мелькнуло бы
        // «пробный период кончился».
        if !licenses.ready() {
            return None;
        }
        let access = access.get();
        let left = access.days_left(today);
        let renew = matches!(access.term(), Some(Term::Key { .. })) && left.is_some_and(|left| left <= RENEW_DAYS);
        let trial = matches!(access.term(), Some(Term::Trial { .. })) && left.is_some();
        let over = left.is_none();
        (trial || renew || over).then(|| view! {
            <div class="notice due" role="status">
                {icon(Icon::Document)}
                <p class="reminder">{move || {
                    let lang = lang.get();
                    let mut words = term_words(lang, access, today);
                    if renew {
                        words.push(' ');
                        words.push_str(t(lang, Key::LicenseRenew));
                    }
                    if over {
                        words.push(' ');
                        words.push_str(t(lang, Key::LicenseLockedShort));
                    }
                    words
                }}</p>
                <LicenseLink lang />
            </div>
        })
    }
}

/// Что сказать после «Ввести ключ».
#[derive(Clone, PartialEq, Debug)]
enum Said {
    /// Ключ записан; ИНН — его и профиля, когда они не сходятся.
    Entered { key_tin: Tin, profile_tin: Option<Tin> },
    /// Этот ключ уже введён.
    Known,
    /// Не ключ, ключ новее программы или подпись не сходится.
    Refused(KeyError),
    /// Хранилище не записало.
    Failed(StoreError),
}

/// Содержимое экрана лицензии: срок и что заперто, ввод ключа и введённые
/// ключи.
#[component]
pub(super) fn LicensePage(ctx: tax::Ctx) -> impl IntoView {
    let tax::Ctx {
        lang,
        sources,
        base,
        licenses,
        forms,
        today,
        ..
    } = ctx;
    let profiles = sources.profiles;
    licenses.watch(base);
    let access = Memo::new(move |_| licenses.access(profiles));
    let profile_tin = Memo::new(move |_| profiles.with(|data| data.and_then(|data| data.tin.clone())));
    let typed = RwSignal::new(String::new());
    let said = RwSignal::new(None::<Said>);
    let busy = RwSignal::new(false);
    forms.hold_while(move || busy.get() || typed.with(|text| !text.is_empty()));

    let submit = move |event: SubmitEvent| {
        event.prevent_default();
        if busy.get_untracked() {
            return;
        }
        let key = match license::Key::read(&typed.get_untracked(), &licenses.public) {
            Ok(key) => key,
            Err(error) => {
                said.set(Some(Said::Refused(error)));
                return;
            }
        };
        let Some(store) = base.get() else {
            said.set(Some(Said::Failed(StoreError::Unavailable)));
            return;
        };
        busy.set(true);
        leptos::task::spawn_local(async move {
            let entered = enter(&store, key.clone()).await;
            busy.set(false);
            said.set(Some(match entered {
                Ok(Some(license)) => {
                    licenses.put(license);
                    typed.set(String::new());
                    let profile_tin = profile_tin.get_untracked();
                    Said::Entered {
                        key_tin: key.tin().clone(),
                        profile_tin,
                    }
                }
                // Ключ уже в базе, а окно его, может быть, не прочитало —
                // показать то, что в базе.
                Ok(None) => {
                    licenses.load(&store).await;
                    Said::Known
                }
                Err(error) => Said::Failed(error),
            }));
        });
    };

    let status = move || {
        let lang = lang.get();
        let access = access.get();
        let today = today?;
        if !licenses.ready() {
            return None;
        }
        let mut lines = vec![(term_words(lang, access, today), false)];
        match access.days_left(today) {
            Some(left) if matches!(access.term(), Some(Term::Key { .. })) && left <= RENEW_DAYS => {
                let left = fill(t(lang, Key::LicenseLeft), &[&days(lang, left)]);
                lines.push((format!("{left} {}", t(lang, Key::LicenseRenew)), true));
            }
            Some(_) => {}
            None => {
                lines.push((t(lang, Key::LicenseLocked).to_owned(), true));
                lines.extend(quarters_open(lang, access).map(|words| (words, false)));
            }
        }
        Some(
            lines
                .into_iter()
                .map(|(words, strong)| view! { <p class:reminder=strong>{words}</p> })
                .collect_view(),
        )
    };

    view! {
        {move || status().map(|lines| view! { <div class="notice" role="status">{lines}</div> })}
        <p class="hint">{tr(lang, Key::LicenseAbout)}</p>
        {move || licenses.failure().map(|error| view! { <p role="alert">{tr(lang, error_key(error))}</p> })}
        {move || (licenses.unreadable() > 0).then(|| view! {
            <p role="alert">{move || {
                let lang = lang.get();
                fill(t(lang, Key::LicenseUnreadable), &[&i18n::integer(lang, licenses.unreadable() as i64)])
            }}</p>
        })}
        <form on:submit=submit>
            <label>
                {tr(lang, Key::LicenseKey)}
                <textarea
                    rows="4"
                    autocomplete="off"
                    autocapitalize="characters"
                    spellcheck="false"
                    maxlength="1000"
                    bind:value=typed
                ></textarea>
            </label>
            <p class="hint">{tr(lang, Key::LicenseKeyHint)}</p>
            {move || said.get().map(|said| view! { <Said lang said /> })}
            <button type="submit" class="wide" disabled=move || busy.get()>
                {tr(lang, Key::LicenseEnter)}
            </button>
        </form>
        <EnteredKeys lang licenses profile_tin />
    }
}

/// Слова после «Ввести ключ».
#[component]
fn Said(lang: ReadSignal<Lang>, said: Said) -> impl IntoView {
    match said {
        Said::Entered { key_tin, profile_tin } => {
            let mismatch = (profile_tin.as_ref() != Some(&key_tin)).then(|| {
                let key_tin = key_tin.as_str().to_owned();
                let profile_tin = profile_tin.map(|tin| tin.as_str().to_owned());
                view! {
                    <p class="warning" role="alert">{move || {
                        let lang = lang.get();
                        match &profile_tin {
                            Some(profile_tin) => fill(t(lang, Key::LicenseOtherTin), &[&key_tin, profile_tin]),
                            None => fill(t(lang, Key::LicenseNoTin), &[&key_tin]),
                        }
                    }}</p>
                }
            });
            view! {
                <p role="status">{tr(lang, Key::LicenseEntered)}</p>
                {mismatch}
            }
            .into_any()
        }
        Said::Known => view! { <p role="status">{tr(lang, Key::LicenseKnown)}</p> }.into_any(),
        Said::Refused(error) => {
            let key = match error {
                KeyError::Malformed => Key::LicenseMalformed,
                KeyError::Newer => Key::LicenseNewer,
                KeyError::Forged => Key::LicenseForged,
            };
            view! { <p role="alert">{tr(lang, key)}</p> }.into_any()
        }
        Said::Failed(error) => view! { <p role="alert">{tr(lang, error_key(error))}</p> }.into_any(),
    }
}

/// Введённые ключи — новые сверху: тариф, ИНН, срок и что с ключом не так.
#[component]
fn EnteredKeys(lang: ReadSignal<Lang>, licenses: Licenses, profile_tin: Memo<Option<Tin>>) -> impl IntoView {
    let keys = Memo::new(move |_| {
        licenses.with_keys(|list| {
            let mut keys: Vec<license::Key> = License::keys(list).cloned().collect();
            keys.sort_by_key(|key| std::cmp::Reverse((key.until(), key.issued())));
            keys
        })
    });
    move || {
        let keys = keys.get();
        (!keys.is_empty()).then(|| {
            let rows = keys
                .into_iter()
                .map(|key| {
                    let signed = key.verify(&licenses.public);
                    let tin = key.tin().as_str().to_owned();
                    let (plan, issued, until) = (key.plan(), key.issued(), key.until());
                    let other = move || profile_tin.with(|profile| profile.as_ref() != Some(key.tin()));
                    view! {
                        <li class="entry">
                            <span class="entry-main">
                                <span class="entry-title">{tr(lang, plan_key(plan))}</span>
                                <span class="entry-meta">{move || {
                                    let lang = lang.get();
                                    let mut facts = vec![
                                        fill(t(lang, Key::TinShort), &[&tin]),
                                        fill(
                                            t(lang, Key::LicensePeriod),
                                            &[&i18n::date(lang, issued), &i18n::date(lang, until)],
                                        ),
                                    ];
                                    if !signed {
                                        facts.push(t(lang, Key::LicenseNotSigned).to_owned());
                                    } else if other() {
                                        facts.push(t(lang, Key::LicenseNotProfileTin).to_owned());
                                    }
                                    facts.join(" · ")
                                }}</span>
                            </span>
                        </li>
                    }
                })
                .collect_view();
            view! {
                <h2>{tr(lang, Key::LicenseKeys)}</h2>
                <ul class="list">{rows}</ul>
            }
        })
    }
}

/// Ввести ключ — одной транзакцией: прочитать введённые, записать новый
/// (команда ядра `License::enter`). `None` — этот ключ уже введён.
async fn enter(store: &Store, key: license::Key) -> Result<Option<License>, StoreError> {
    let tx = store.write()?;
    let entered = tx.list::<License>().await?;
    let Some(license) = License::enter(key, &entered.records, clock::now()) else {
        return Ok(None);
    };
    let mut writes = Writes::default();
    writes.insert(&license);
    tx.commit(writes).await?;
    Ok(Some(license))
}

/// Слова о запертом выпуске — в форме документа вместо «Выпустить»: срок,
/// что выпуск с ключом, а черновик сохраняется и без него.
#[component]
pub(super) fn IssueLocked(lang: ReadSignal<Lang>, access: Access, today: Date) -> impl IntoView {
    view! {
        <div class="notice due" role="status">
            {icon(Icon::Document)}
            <p class="reminder">{move || {
                let lang = lang.get();
                format!("{} {}", term_words(lang, access, today), t(lang, Key::DocIssueLocked))
            }}</p>
            <LicenseLink lang />
        </div>
    }
}

/// Слова о запертых числах квартала — на экране чисел для расчёта вместо
/// полей: срок и какие кварталы открыты.
#[component]
pub(super) fn ReportLocked(lang: ReadSignal<Lang>, access: Access, today: Date) -> impl IntoView {
    view! {
        <div class="notice due" role="status">
            {icon(Icon::Document)}
            <p class="reminder">{move || {
                let lang = lang.get();
                let mut words = format!("{} {}", term_words(lang, access, today), t(lang, Key::ReportLocked));
                if let Some(open) = quarters_open(lang, access) {
                    words.push(' ');
                    words.push_str(&open);
                }
                words
            }}</p>
            <LicenseLink lang />
        </div>
    }
}

/// Заперт ли выпуск сегодня — срок кончился; тогда — сегодня, для слов.
/// `None` — выпуск открыт, ключи ещё читаются или часы без даты: без
/// сегодня срок не сравнить, и выпуск не запирается. Отслеживаемое.
pub(super) fn issue_locked(licenses: Licenses, access: Access, today: Option<Date>) -> Option<Date> {
    let today = today?;
    (licenses.ready() && !access.issue(today)).then_some(today)
}

/// Заперты ли числа квартала `quarter` — так же, как выпуск
/// ([`issue_locked`]). Отслеживаемое.
pub(super) fn report_locked(
    licenses: Licenses,
    access: Access,
    quarter: Quarter,
    today: Option<Date>,
) -> Option<Date> {
    let today = today?;
    (licenses.ready() && !access.report(quarter, today)).then_some(today)
}
