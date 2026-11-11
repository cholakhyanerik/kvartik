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
//!
//! Сколько дней осталось — всегда на виду (задача 92, решение человека
//! 2026-11-11): меткой ([`LicenseChip`]) в меню на компьютере и на главном
//! экране телефона, а в последние [`license::SOON_DAYS`] дней и после конца срока — и
//! над остальными экранами телефона. Кончился срок — программа открывается
//! экраном лицензии (`Licenses::show`), а за неделю до конца ключа встаёт
//! окно ([`RenewDialog`]). Метке нужен срок с первого кадра, а подпись
//! проверяет часть, поэтому главный wasm берёт срок без проверки
//! ([`Licenses::shown`]): у пустой базы ключей нет, а иначе — какой ключ
//! в прошлый раз нашла часть, помнит устройство. Часть, проверив подписи,
//! ставит срок сама и обновляет запись. Запирает выпуск и числа по-прежнему
//! только срок с проверкой ([`Licenses::access`]).

use std::sync::Arc;

use kvartik_core::i18n::{self, Key, Lang, fill, plural, t};
use kvartik_core::license::{self, Access, KeyError, License, PUBLIC_KEY_BYTES, Paid, Plan, Term};
use kvartik_core::model::date::{Date, Quarter};
use kvartik_core::model::tin::Tin;
use kvartik_core::release;
use leptos::ev::SubmitEvent;
use leptos::portal::Portal;
use leptos::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{HtmlElement, KeyboardEvent};

use super::backup::{BackupChip, chip};
use super::form::Forms;
use super::icon::{Icon, icon};
use super::ledger::tax_part;
use super::profile::Profiles;
use super::pwa::Program;
use super::route::{Router, Screen};
use super::split::screen;
use super::storage::error_key;
use super::tax::{self, Part};
use super::{Base, tr};
use crate::platform::{self, clock};
use crate::store::device::{self, Device};
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
    /// Сегодня — один раз (Архитектура, «Время — параметр»).
    today: Option<Date>,
    /// Срок, который окно показывает, — метка, экран лицензии при запуске
    /// и окно о конце ключа: без проверки подписи (шапка модуля). `None` —
    /// ещё неизвестен.
    shown: RwSignal<Option<Access>>,
    /// Имя открытой базы: под ним устройство помнит её ключ; пусто — база
    /// ещё не выбрана. Сигналом и строкой — тем же типом, что у конторы:
    /// новый тип сигнала — новый код в главном wasm (Правило 1).
    base_name: RwSignal<String>,
    /// Стоит ли окно о конце ключа: программа под ним неактивна.
    covering: RwSignal<bool>,
    /// Запуск ли это программы во вкладке, а срок ещё не стал известен:
    /// тогда кончившийся срок открывает экран лицензии ([`Licenses::show`]).
    launch: StoredValue<bool>,
    /// Что мешает увести на экран лицензии и поднять окно о конце ключа, —
    /// одной ручкой: `Licenses` лежит в `Ctx` каждого экрана, и поле в нём —
    /// байты при каждом переносе `Ctx`, а у профиля — ещё и вариант
    /// перечисления больше порога клиппи.
    calm: StoredValue<Calm>,
}

/// Что мешает увести на экран лицензии и поднять окно о конце ключа
/// (задача 92): начатая форма, экран первого запуска, окно «Что нового» —
/// и экран, с которым окно открылось.
#[derive(Clone, Copy)]
struct Calm {
    router: Router,
    forms: Forms,
    welcome: RwSignal<bool>,
    program: Program,
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
    pub fn start(
        today: Option<Date>,
        router: Router,
        forms: Forms,
        welcome: RwSignal<bool>,
        program: Program,
    ) -> Self {
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
            today,
            shown: RwSignal::new(None),
            base_name: RwSignal::new(String::new()),
            covering: RwSignal::new(false),
            launch: StoredValue::new(!device::relaunched()),
            calm: StoredValue::new(Calm {
                router,
                forms,
                welcome,
                program,
            }),
        }
    }

    /// Вкладка выбрала базу `name`: её ключ — каким его в прошлый раз нашла
    /// часть, если устройство это помнит. Так метка срока стоит с первого
    /// кадра, а не ждёт части.
    pub fn chosen(&self, name: &str) {
        self.base_name.set(name.to_owned());
        if let Some(key) = Device::new().key_term(name) {
            self.show(self.known(key));
        }
    }

    /// База открылась пустой: ключей в ней нет — ни записи, ни удалённой, —
    /// и срок — один пробный период, что бы ни помнило устройство.
    pub fn empty(&self) {
        self.show(self.known(None));
    }

    /// Срок по ключу, проверенному раньше, и пробному периоду устройства.
    fn known(&self, key: Option<Paid>) -> Access {
        Access::known(self.trial.and_then(license::trial_last), key)
    }

    /// Показать срок; тот же — подписчиков не будить.
    ///
    /// Первый известный срок при запуске программы решает и экран: кончился
    /// он — пробный период или ключ — программа открывается экраном лицензии,
    /// где всё о покупке (задача 92, решение человека 2026-11-11). Только
    /// при запуске, а не при перезагрузке, возврате «Назад» и включённой
    /// новой версии — перезагрузка открывает тот же экран (CLAUDE.md,
    /// Правило 2), а какой это из них, помнит вкладка (`device::relaunched`).
    /// Данные это не запирает: с экрана лицензии человек уходит куда хочет,
    /// а «Назад» вернёт его на экран, с которым программа открылась. Успел
    /// он уйти с первого экрана или открыть форму — остаётся где был; стоит
    /// экран первого запуска — тоже. Решение одно, своего эффекта у него нет:
    /// эффект в главном wasm — около килобайта (Правило 1).
    fn show(&self, access: Access) {
        if self.shown.get_untracked() != Some(access) {
            self.shown.set(Some(access));
        }
        if self.launch.get_value() {
            self.launch.set_value(false);
            let ended = self.today.is_some_and(|today| !access.issue(today));
            let calm = self.calm.get_value();
            // Адресом, а не `Router::go`: окно идёт за адресом само, и «Назад»
            // вернёт прежний экран.
            if ended && !calm.welcome.get_untracked() && calm.router.untouched() && !calm.forms.busy() {
                platform::set_hash(&Screen::License.href());
            }
        }
    }

    /// Срок, который окно показывает, — отслеживаемое. Без проверки подписи:
    /// только для показа (шапка модуля); `None` — ещё неизвестен.
    pub(super) fn shown(&self) -> Option<Access> {
        self.shown.get()
    }

    /// Читать ключи, когда база открылась и когда её заменило
    /// восстановление из копии: ключ переезжает с копией. Зовут части,
    /// которым ключи нужны, а слежение одно и живёт у корня окна, до его
    /// конца: эффект Leptos умирает с владельцем, а первой может позвать
    /// форма профиля или документа — после её закрытия восстановленный
    /// ключ остался бы непрочитанным (ревью задачи 25).
    ///
    /// Там же срок, проверенный подписью, идёт в метку и в запись устройства
    /// ([`Licenses::shown`]) — когда прочитаны и ключи, и профиль: ключ на
    /// ՀՎՀՀ профиля, и до профиля любой ключ казался бы чужим, а у купившего
    /// мелькнуло бы «пробный период кончился». `profiles` — профиль открытой
    /// базы, у всех, кто зовёт, один.
    pub(super) fn watch(&self, base: Base, profiles: Profiles) {
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
            Effect::new(move |_| {
                if !licenses.ready() || !profiles.ready() {
                    return;
                }
                let access = licenses.access(profiles);
                licenses.show(access);
                let name = licenses.base_name.get_untracked();
                let device = Device::new();
                if !name.is_empty() && device.key_term(&name) != Some(access.key()) {
                    device.set_key_term(&name, access.key());
                }
            });
        };
        match self.root.get_value() {
            Some(root) => root.with(follow),
            None => follow(),
        }
    }

    /// Ключ, о конце которого пора сказать окном поверх программы, — если
    /// о нём ещё не сказано: в этом окне (`told`) или раньше, по записи
    /// устройства. Отслеживаемое.
    fn ending(&self, told: RwSignal<Option<Date>>) -> Option<Paid> {
        let key = self.shown()?.ending_key(self.today?)?;
        let base = self.base_name.get_untracked();
        let said = told.get() == Some(key.1) || Device::new().key_told(&base) == Some(key.1);
        (!base.is_empty() && !said).then_some(key)
    }

    /// Окно о конце ключа `up` закрыли: о нём сказано — больше не встанет,
    /// ни в этом окне (`told`, и когда устройство не записало), ни потом.
    fn renewed(&self, up: RwSignal<Option<Paid>>, told: RwSignal<Option<Date>>) {
        if let Some((_, last)) = up.get_untracked() {
            told.set(Some(last));
            Device::new().set_key_told(&self.base_name.get_untracked(), last);
        }
        self.covering.set(false);
        up.set(None);
    }

    /// Стоит ли окно о конце ключа — отслеживаемое: программа под ним
    /// неактивна (`ui/mod.rs`).
    pub fn covering(&self) -> bool {
        self.covering.get()
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

/// Экран лицензии на языке окна: «назад» к профилю, заголовок и метка
/// копии, когда пора её сделать (Правило 2), — сразу, остальное — частью
/// книги с первого показа (`split::screen`).
#[component]
pub fn LicenseScreen(ctx: tax::Ctx, router: Router) -> impl IntoView {
    let lang = ctx.lang;
    view! {
        <a class="back" href=Screen::Profile.href()>
            {icon(Icon::Back)}
            <span>{tr(lang, Key::NavProfile)}</span>
        </a>
        <h1 tabindex="-1">{tr(lang, Key::LicenseTitle)}</h1>
        <BackupChip lang backups=ctx.backups nudge=true />
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

/// Срок словами: что идёт, до какого дня и сколько дней осталось, а кончился —
/// когда.
pub(super) fn term_words(lang: Lang, access: Access, today: Date) -> String {
    let date = |day| i18n::date(lang, day);
    match (access.term(), access.days_left(today)) {
        (Some(Term::Trial { last }), Some(left)) => {
            fill(t(lang, Key::LicenseTrialUntil), &[&date(last), &days(lang, left)])
        }
        (Some(Term::Key { plan, last }), Some(left)) => fill(
            t(lang, Key::LicenseKeyUntil),
            &[t(lang, plan_key(plan)), &date(last), &days(lang, left)],
        ),
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

/// Строка «Купить ключ» с тарифом и ценой (задача 35): ссылка на страницу
/// «Купить» сайта на языке окна — в новой вкладке, как условия лицензии
/// в «О программе»: страница вне программы, и программа остаётся на своём
/// экране. Цена — из ядра (`release::PRICE`), та же, что на сайте.
#[component]
fn BuyEntry(lang: ReadSignal<Lang>) -> impl IntoView {
    view! {
        <ul class="list">
            <li>
                <a class="entry" href=move || release::buy_path(lang.get()) target="_blank" rel="noopener">
                    <span class="entry-main">
                        <span class="entry-title">{tr(lang, Key::LicenseBuy)}</span>
                        <span class="entry-meta">{move || {
                            let lang = lang.get();
                            let price = i18n::money(lang, release::PRICE);
                            fill(t(lang, Key::LicenseBuyPrice), &[t(lang, Key::PlanStandard), &price])
                        }}</span>
                    </span>
                    {icon(Icon::Forward)}
                </a>
            </li>
        </ul>
    }
}

/// Ссылка на экран лицензии — из слов о запертом.
#[component]
pub(super) fn LicenseLink(lang: ReadSignal<Lang>) -> impl IntoView {
    view! { <a class="button" href=Screen::License.href()><span class="label">{tr(lang, Key::LicenseEnter)}</span></a> }
}

/// Карточка на главном экране: ключ — когда пора продлить, кончился срок —
/// что заперто. Сколько дней идёт пробный период или ключ, говорит метка
/// срока над карточками ([`LicenseChip`], задача 92), и карточки тогда нет.
/// Из части книги, как карточка налога.
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
    licenses.watch(base, sources.profiles);
    let access = Memo::new(move |_| licenses.access(sources.profiles));
    move || {
        let today = today?;
        // Пока ключи или профиль читаются — карточки нет: у купившего ключ
        // мелькнуло бы «пробный период кончился», а ключ — на ՀՎՀՀ профиля.
        if !licenses.ready() || !sources.profiles.ready() {
            return None;
        }
        let access = access.get();
        let left = access.days_left(today);
        let renew = matches!(access.term(), Some(Term::Key { .. })) && left.is_some_and(|left| left <= RENEW_DAYS);
        let over = left.is_none();
        (renew || over).then(|| view! {
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

/// Где стоит метка срока.
#[derive(Clone, Copy)]
pub enum Place {
    /// В меню — на компьютере, всегда.
    Menu,
    /// На главном экране под приветствием — на телефоне, всегда.
    Home,
    /// Над остальными экранами — на телефоне, в последние
    /// [`license::SOON_DAYS`] дней и после конца срока; на главном и на
    /// экране лицензии её нет — там о сроке сказано и так.
    Screens(Router),
}

/// Метка срока (задача 92, решение человека 2026-11-11): сколько дней
/// осталось — пробного периода или ключа, — а кончился — что кончился.
/// Ведёт на экран лицензии; где стоит и когда видна — [`Place`], а на
/// компьютере ли она или на телефоне, решают стили (`.nav .chip`,
/// `.chip.nudge`), как у метки копии. Громче — и заливкой, и словами, не
/// одним цветом (Правило 4).
///
/// В главном wasm — срок без проверки подписи ([`Licenses::shown`]); пока он
/// неизвестен или часы без даты, метки нет: у купившего ключ мелькнуло бы
/// «пробный период кончился».
#[component]
pub fn LicenseChip(lang: ReadSignal<Lang>, licenses: Licenses, place: Place) -> impl IntoView {
    move || {
        let today = licenses.today?;
        let access = licenses.shown()?;
        let term = access.term()?;
        let left = access.days_left(today);
        let loud = access.soon(today);
        if let Place::Screens(router) = place
            && (!loud || matches!(router.screen(), Screen::Home | Screen::License))
        {
            return None;
        }
        // Язык читается здесь же: сменился — метка рисуется заново, и своё
        // замыкание у слов было бы лишним кодом в главном wasm.
        let words = chip_words(lang.get(), term, left);
        Some(chip(Screen::License.href(), Icon::Key, words, loud, !matches!(place, Place::Menu)))
    }
}

/// Слова метки срока.
fn chip_words(lang: Lang, term: Term, left: Option<i32>) -> String {
    match (term, left) {
        (Term::Trial { .. }, Some(left)) => fill(t(lang, Key::LicenseChipTrial), &[&days(lang, left)]),
        (Term::Key { .. }, Some(left)) => fill(t(lang, Key::LicenseChipKey), &[&days(lang, left)]),
        (Term::Trial { .. }, None) => t(lang, Key::LicenseChipTrialEnded).to_owned(),
        (Term::Key { .. }, None) => t(lang, Key::LicenseChipKeyEnded).to_owned(),
    }
}

/// Окно «Срок лицензии подходит к концу» (задача 92, решение человека
/// 2026-11-11): до конца ключа не больше [`license::SOON_DAYS`] дней — окно
/// поверх программы, один раз на ключ: закрытое, оно больше не встаёт,
/// а дальше говорят метка цветом напоминания и карточка «продлите ключ». Ждёт,
/// пока стоят первый запуск или «Что нового» и пока открыта форма.
///
/// Всё окно — в части книги, с карточками главного экрана (`Part::Card`): оно
/// встаёт раз в срок ключа, а у владельца ключа в базе есть данные, и часть
/// грузится при открытии и так. Карточки — внутри главного экрана, спрятанного,
/// когда открыт другой, поэтому окно встаёт в конец страницы (`Portal`), а не
/// на месте карточек. Программа под ним неактивна ([`Licenses::covering`]).
#[component]
pub(super) fn RenewDialog(ctx: tax::Ctx) -> impl IntoView {
    let licenses = ctx.licenses;
    let up = RwSignal::new(None::<Paid>);
    // О каком ключе сказано в этом окне: карточки главного экрана живут, пока
    // живо окно, и вместе с ними — это.
    let told = RwSignal::new(None::<Date>);
    Effect::new(move |_| {
        if up.get_untracked().is_some() {
            return;
        }
        let Some(key) = licenses.ending(told) else {
            return;
        };
        let calm = licenses.calm.get_value();
        if !calm.welcome.get() && !calm.program.blocks() && !ctx.forms.any() {
            licenses.covering.set(true);
            up.set(Some(key));
        }
    });
    let shown = Memo::new(move |_| up.with(Option::is_some));
    move || shown.get().then(|| view! { <Portal><RenewCard ctx up told /></Portal> })
}

/// `id` заголовка окна о конце ключа: по нему окно названо для диктора и на
/// нём встаёт фокус.
const RENEW_TITLE: &str = "renew-title";

/// Само окно о конце ключа — уже в части книги: сколько дней и до какого дня,
/// что запрёт конец срока, «Продлить» — на экран лицензии — и «Понятно».
/// Закрывают его обе кнопки и Escape. Фокус — на заголовке, после кадра,
/// как у «Что нового».
#[component]
fn RenewCard(ctx: tax::Ctx, up: RwSignal<Option<Paid>>, told: RwSignal<Option<Date>>) -> impl IntoView {
    let tax::Ctx { lang, licenses, .. } = ctx;
    request_animation_frame(|| {
        let title = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.get_element_by_id(RENEW_TITLE));
        if let Some(title) = title {
            let _ = title.unchecked_into::<HtmlElement>().focus();
        }
    });
    let close = move || licenses.renewed(up, told);
    let keydown = move |event: KeyboardEvent| {
        if event.key() == "Escape" {
            close();
        }
    };
    let renew = move |_| {
        close();
        platform::set_hash(&Screen::License.href());
    };
    let words = move || {
        let lang = lang.get();
        let key = up.get()?;
        let left = Access::known(None, Some(key)).days_left(licenses.today?)?;
        Some(fill(t(lang, Key::LicenseSoonText), &[&days(lang, left), &i18n::date(lang, key.1)]))
    };
    view! {
        <div class="update" role="dialog" aria-modal="true" aria-labelledby=RENEW_TITLE on:keydown=keydown>
            <div class="update-card">
                <h2 id=RENEW_TITLE tabindex="-1">{tr(lang, Key::LicenseSoonTitle)}</h2>
                <p class="renew-words">{words}</p>
                <button type="button" class="wide" on:click=renew>
                    <span class="label">{tr(lang, Key::LicenseSoonRenew)}</span>
                </button>
                <button type="button" class="wide secondary" on:click=move |_| close()>
                    <span class="label">{tr(lang, Key::LicenseSoonOk)}</span>
                </button>
            </div>
        </div>
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
    licenses.watch(base, profiles);
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
        if !licenses.ready() || !profiles.ready() {
            return None;
        }
        let mut lines = vec![(term_words(lang, access, today), false)];
        match access.days_left(today) {
            Some(left) if matches!(access.term(), Some(Term::Key { .. })) && left <= RENEW_DAYS => {
                lines.push((t(lang, Key::LicenseRenew).to_owned(), true));
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
        <BuyEntry lang />
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
                <span class="label">{tr(lang, Key::LicenseEnter)}</span>
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
