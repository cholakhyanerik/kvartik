//! Раздел «Профиль ИП» (задача 9): реквизиты для счёта и акта, виды
//! деятельности и режим для налога, данные человека для годовых платежей.
//!
//! Раздел — содержимое экрана «Профиль» (`ui/route.rs`): заполненный
//! профиль — коротким итогом с кнопкой «Изменить», форма — по нажатию. Сохранение — команды ядра (`Profile::save`, `Logo::set`)
//! и одна транзакция хранилища (CLAUDE.md, «Данные меняются одним путём»).
//!
//! Ошибки полей окно показывает после первой попытки сохранить, а дальше —
//! сразу при вводе: красное под пустой формой только пугает. Предупреждение
//! о коде, который закрывает налог с оборота, — сразу, ещё до сохранения:
//! узнать об этом в конце квартала поздно. Так же — о номере счёта с
//! несходящейся контрольной цифрой: опечатку видно, пока номер перед глазами.

use std::collections::BTreeMap;
use std::sync::Arc;

use kvartik_core::i18n::{self, Key, Lang, fill, t};
use kvartik_core::model::activity::{ActivityCode, CodeError, IncomeKind};
use kvartik_core::model::bank::{BankAccount, Swift};
use kvartik_core::model::date::Date;
use kvartik_core::model::logo::{self, Image, ImageError, Logo};
use kvartik_core::model::money::Currency;
// Год рождения разбирает ядро: его же поле — у калькулятора лендинга
// (задача 34).
use kvartik_core::model::profile::{
    Email, Localized, Person, Profile, ProfileData, Regime, Residency, Stop, birth_year,
};
use kvartik_core::model::text::{Line, Lines};
use kvartik_core::model::tin::Tin;
use kvartik_core::tax::example::Handoff;
use kvartik_core::tax::{annual, deadlines, turnover};
use leptos::ev::{Event, SubmitEvent};
use leptos::prelude::*;
use leptos::wasm_split::SplitLoaderError;
use web_sys::HtmlInputElement;

use super::avatar;
use super::backup::Backups;
use super::form::{
    AreaField, CURRENCIES, FieldError, Forms, TextField, currency_code, currency_key, optional,
};
use super::icon::{Icon, icon};
use super::license::Licenses;
use super::onboarding::{Card, StepsCard};
use super::route::{Router, Screen};
use super::split::{screen, split};
use super::storage::error_key;
use super::tax;
use super::{Base, tr};
use crate::platform::clock;
use crate::platform::file::{self, FileError, Picture};
use crate::store::device::Device;
use crate::store::{Store, StoreError, Writes};

/// Что окно знает о профиле.
#[derive(Clone, Copy)]
pub struct Profiles {
    state: RwSignal<State>,
    /// Сохранённый логотип — картинкой для показа.
    picture: RwSignal<Option<Arc<Picture>>>,
    /// Открыта ли форма — здесь, а не в разделе: первые шаги открывают её
    /// короткой (`open_brief`).
    editing: RwSignal<bool>,
    /// На какой группе полей открыть форму.
    focus: RwSignal<Option<Part>>,
    /// Форма — короткая: только то, что нужно первому счёту.
    brief: RwSignal<bool>,
    /// Ждёт ли на устройстве введённое в калькуляторе лендинга (задача 34):
    /// окно предлагает взять его, пока человек не взял или не отказался.
    handoff: RwSignal<bool>,
    /// Что из него подставить в форму, которую открывает предложение.
    handed: RwSignal<Option<Handoff>>,
}

/// Где профиль.
#[derive(Clone, PartialEq, Debug)]
enum State {
    /// База ещё открывается.
    Loading,
    /// Прочитан; `None` — профиля ещё нет. В `Arc`: сигнал отдаёт
    /// состояние копией, а профиль — сотни байтов.
    Ready(Option<Arc<Profile>>),
    /// Не прочитан.
    Failed(StoreError),
}

impl Profiles {
    pub fn new() -> Self {
        Self {
            state: RwSignal::new(State::Loading),
            picture: RwSignal::new(None),
            editing: RwSignal::new(false),
            focus: RwSignal::new(None),
            brief: RwSignal::new(false),
            handoff: RwSignal::new(Device::new().handoff().is_some()),
            handed: RwSignal::new(None),
        }
    }

    /// Открыть короткую форму — из первых шагов (`ui/onboarding.rs`): на
    /// реквизитах или на кодах деятельности. Открытую форму это не трогает:
    /// начатое в ней не пропадает.
    pub fn open_brief(&self, activities: bool) {
        if !self.editing.get_untracked() {
            self.brief.set(true);
            self.focus.set(activities.then_some(Part::Activities));
            self.editing.set(true);
        }
    }

    /// Открыть форму с тем, что передал калькулятор лендинга (задача 34):
    /// полную — года рождения и гражданства в короткой нет. С переданной
    /// строкой налога — на кодах: строке нужен код, и вписать его надо там,
    /// иначе — на данных человека. Открытую форму это не трогает: предложение
    /// видно, только пока она закрыта.
    pub(super) fn open_handoff(&self, handed: Handoff) {
        if !self.editing.get_untracked() {
            self.handed.set(Some(handed));
            self.brief.set(false);
            self.focus.set(Some(if handed.kind.is_some() { Part::Activities } else { Part::Person }));
            self.editing.set(true);
        }
    }

    /// Человек взял переданное калькулятором или отказался: забыть его на
    /// устройстве.
    pub(super) fn drop_handoff(&self) {
        Device::new().clear_handoff();
        self.handoff.set(false);
    }

    /// Ждёт ли предложение калькулятора — отслеживаемое.
    pub(super) fn handoff_waiting(&self) -> bool {
        self.handoff.get()
    }

    /// Открыта ли форма — отслеживаемое.
    pub(super) fn form_open(&self) -> bool {
        self.editing.get()
    }

    /// Прочитан ли профиль — есть он или нет. Отслеживаемое.
    pub(super) fn ready(&self) -> bool {
        self.state.with(|state| matches!(state, State::Ready(_)))
    }

    /// Сохранённый профиль; `None` — его нет или он ещё не прочитан.
    /// Отслеживаемое: по нему контора обновляет имя ИП в списке (задача 38).
    pub(super) fn profile(&self) -> Option<Arc<Profile>> {
        self.state.with(|state| match state {
            State::Ready(profile) => profile.clone(),
            State::Loading | State::Failed(_) => None,
        })
    }

    /// Профиля ещё нет — база прочитана, а его в ней нет. Отслеживаемое:
    /// главный экран предлагает заполнить профиль, пока его нет.
    pub fn missing(&self) -> bool {
        self.state.with(|state| *state == State::Ready(None))
    }

    /// Коды деятельности профиля — у расхода книги (задача 14); пока
    /// профиля нет — пусто. Отслеживаемое.
    pub(super) fn activities(&self) -> Vec<ActivityCode> {
        self.state.with(|state| match state {
            State::Ready(Some(profile)) => profile.data().activities.clone(),
            State::Ready(None) | State::Loading | State::Failed(_) => Vec::new(),
        })
    }

    /// Что-то, посчитанное по профилю; пока он читается или не прочитан —
    /// значение по умолчанию. Профиля нет — `None`. Отслеживаемое: по кодам
    /// профиля считается налог (задача 19).
    pub(super) fn with<T: Default>(&self, read: impl FnOnce(Option<&ProfileData>) -> T) -> T {
        self.state.with(|state| match state {
            State::Ready(profile) => read(profile.as_deref().map(Profile::data)),
            State::Loading | State::Failed(_) => T::default(),
        })
    }

    /// Почему профиль не прочитался — отслеживаемое.
    pub(super) fn failure(&self) -> Option<StoreError> {
        self.state.with(|state| match state {
            State::Failed(error) => Some(*error),
            State::Loading | State::Ready(_) => None,
        })
    }

    /// Отметить день подачи заявления о налоге с оборота нового ИП (`Some`) или
    /// снять отметку (`None`) — с карточки на главном экране (полный прогон
    /// 2026-11-10, п. 7). Команда ядра — `Profile::save` над профилем,
    /// прочитанным в той же транзакции: правка из другой вкладки не теряется,
    /// а остальные поля остаются как были. Профиля нет — отмечать нечего:
    /// карточки без него не бывает.
    pub(super) async fn mark_applied(&self, store: &Store, day: Option<Date>) -> Result<(), StoreError> {
        let tx = store.write()?;
        let existing = Profile::current(tx.all::<Profile>().await?).ok_or(StoreError::Failed)?;
        let mut data = existing.data().clone();
        data.person.turnover_applied = day;
        let mut writes = Writes::default();
        let profile = match Profile::save(Some(existing.clone()), data, clock::now()) {
            Ok(Some(profile)) => {
                writes.update(&profile);
                profile
            }
            Ok(None) => existing,
            Err(_) => return Err(StoreError::Failed),
        };
        tx.commit(writes).await?;
        self.state.set(State::Ready(Some(Arc::new(profile))));
        Ok(())
    }

    /// База открылась: прочитать профиль и его логотип.
    pub async fn load(&self, store: &Store) {
        match read(store).await {
            Ok((profile, logo)) => {
                self.picture.set(logo.as_ref().and_then(picture));
                self.state.set(State::Ready(profile.map(Arc::new)));
            }
            Err(error) => self.state.set(State::Failed(error)),
        }
    }
}

/// Профиль, с которым работает окно, и его логотип — одним чтением.
async fn read(store: &Store) -> Result<(Option<Profile>, Option<Logo>), StoreError> {
    let tx = store.read()?;
    let profile = Profile::current(tx.all::<Profile>().await?);
    let logo = match &profile {
        Some(profile) => live_logo(tx.get::<Logo>(profile.id()).await)?,
        None => None,
    };
    Ok((profile, logo))
}

/// Логотип, если он есть и не убран. Нечитаемый логотип — не повод не
/// показать профиль: его заменит следующий выбранный.
fn live_logo(read: Result<Option<Logo>, StoreError>) -> Result<Option<Logo>, StoreError> {
    match read {
        Ok(logo) => Ok(logo.filter(|logo| !logo.is_removed())),
        Err(StoreError::Damaged) => Ok(None),
        Err(error) => Err(error),
    }
}

/// Картинка логотипа для `<img>`.
fn picture(logo: &Logo) -> Option<Arc<Picture>> {
    let image = logo.image();
    Picture::new(image.bytes(), image.format().mime()).map(Arc::new)
}

/// Группа полей формы профиля, к которой ведёт строка итога: форма
/// открывается на ней.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Part {
    Requisites,
    Accounts,
    Activities,
    Person,
}

impl Part {
    /// `id` группы полей в форме.
    const fn id(self) -> &'static str {
        match self {
            Part::Requisites => "profile-requisites",
            Part::Accounts => "profile-accounts",
            Part::Activities => "profile-activities",
            Part::Person => "profile-person",
        }
    }
}

/// Общее у частей раздела.
#[derive(Clone, Copy)]
pub(super) struct Ctx {
    lang: ReadSignal<Lang>,
    profiles: Profiles,
    base: Base,
    backups: Backups,
    /// Открыта ли форма.
    editing: RwSignal<bool>,
    /// Сохранили ли только что: «Сохранено» над итогом.
    saved: RwSignal<bool>,
    /// На какой группе полей открыть форму.
    focus: RwSignal<Option<Part>>,
    /// Открытые формы окна: форма профиля отмечается в них, пока открыта.
    forms: Forms,
    /// Ключи лицензии: ИНН, на который выдан ключ (задача 25).
    licenses: Licenses,
    /// Сегодня — один раз (Архитектура, «Время — параметр»): по нему
    /// проверяется год рождения и берутся правила налога.
    today: Option<Date>,
    router: Router,
}

/// Раздел на языке окна — под заголовком экрана, частью wasm с первого
/// показа (`split::screen`).
#[component]
pub fn ProfileSection(
    lang: ReadSignal<Lang>,
    profiles: Profiles,
    licenses: Licenses,
    base: Base,
    backups: Backups,
    router: Router,
    forms: Forms,
) -> impl IntoView {
    let ctx = Ctx {
        lang,
        profiles,
        base,
        backups,
        editing: profiles.editing,
        saved: RwSignal::new(false),
        focus: profiles.focus,
        forms,
        licenses,
        today: clock::today(),
        router,
    };
    // Копию восстановили — профиль в базе мог стать другим или появиться.
    // Здесь, а не в части экрана: профиль нужен и главному экрану, и книге,
    // а экран мог ещё ни разу не показываться.
    Effect::watch(
        move || base.replacements(),
        move |_, _, _| {
            if let Some(store) = base.get() {
                ctx.editing.set(false);
                ctx.saved.set(false);
                leptos::task::spawn_local(async move { profiles.load(&store).await });
            }
        },
        false,
    );
    screen(lang, router, Screen::Profile, move || profile_screen(ProfilePart::Screen(ctx)))
}

/// Первые шаги — из части профиля. В тестах функция части видна только своему
/// модулю (`split!`).
pub(super) async fn steps_part(card: Card) -> Result<AnyView, SplitLoaderError> {
    profile_screen(ProfilePart::Steps(card)).await
}

/// Предложение калькулятора лендинга — из той же части.
pub(super) async fn handoff_part(card: Card) -> Result<AnyView, SplitLoaderError> {
    profile_screen(ProfilePart::Handoff(card)).await
}

/// Что рисует часть профиля.
#[derive(Clone, Copy)]
pub(super) enum ProfilePart {
    /// Итог или форму экрана.
    Screen(Ctx),
    /// Первые шаги на главном экране (`ui/onboarding.rs`): ведут сюда же.
    Steps(Card),
    /// Предложение взять в профиль введённое в калькуляторе лендинга
    /// (задача 34): ведёт сюда же, в форму.
    Handoff(Card),
}

split! {
    /// Итог или форма экрана «Профиль» — своей частью wasm (`ui/split.rs`):
    /// первому заходу нужен только главный экран. Первые шаги и предложение
    /// калькулятора — в ней же: своя часть умножала бы общие куски (там же).
    pub(super) fn profile_screen(part: ProfilePart) -> AnyView, preload preload_screen {
        match part {
            ProfilePart::Screen(ctx) => view! { <ProfileBody ctx /> }.into_any(),
            ProfilePart::Steps(card) => view! { <StepsCard card /> }.into_any(),
            ProfilePart::Handoff(card) => view! { <HandoffCard card /> }.into_any(),
        }
    }
}

/// Предложение взять в профиль введённое в калькуляторе лендинга (задача 34)
/// — уже в части (`ui/onboarding.rs`, `HandoffOffer`). Запись читается один
/// раз: пишет её только лендинг, до открытия программы. Предлагается только то,
/// чего профиль ещё не знает (`Handoff::new_for`), а нечего — запись
/// забывается: предлагать нечего и потом. «Взять» открывает форму с
/// подставленным, и сохраняет её человек: молча окно ничего не берёт.
#[component]
fn HandoffCard(card: Card) -> impl IntoView {
    let Card {
        lang, router, profiles, ..
    } = card;
    let handed = Handoff::parse(&Device::new().handoff().unwrap_or_default());
    let offer = Memo::new(move |_| profiles.ready().then(|| profiles.with(|data| handed.new_for(data))));
    Effect::new(move || {
        if offer.get().is_some_and(|offer| offer.is_empty()) {
            profiles.drop_handoff();
        }
    });
    let take = move |offer: Handoff| {
        profiles.open_handoff(offer);
        router.guide(Screen::Profile);
    };
    move || {
        let offer = offer.get().filter(|offer| !offer.is_empty())?;
        let lines = move || {
            handoff_lines(lang.get(), offer)
                .into_iter()
                .map(|line| view! { <li>{line}</li> })
                .collect_view()
        };
        Some(view! {
            <section class="notice handoff">
                <h2>{tr(lang, Key::HandoffTitle)}</h2>
                <p>{tr(lang, Key::HandoffAbout)}</p>
                <ul>{lines}</ul>
                <button type="button" on:click=move |_| take(offer)><span class="label">{tr(lang, Key::HandoffTake)}</span></button>
                <button type="button" class="link" on:click=move |_| profiles.drop_handoff()>
                    <span class="label">{tr(lang, Key::HandoffDismiss)}</span>
                </button>
            </section>
        })
    }
}

/// Что предлагает калькулятор — строками на языке окна, в порядке формы.
fn handoff_lines(lang: Lang, offer: Handoff) -> Vec<String> {
    let row = |label: Key, value: &str| fill(t(lang, Key::CalcRow), &[t(lang, label), value]);
    let mut lines = Vec::new();
    if let Some(kind) = offer.kind {
        lines.push(row(Key::ActivityKind, t(lang, turnover::kind_key(kind))));
    }
    if let Some(year) = offer.birth_year {
        lines.push(row(Key::BirthYear, &year.to_string()));
    }
    if let Some(residency) = offer.residency {
        lines.push(row(Key::ProfileResidency, t(lang, residency_key(residency))));
    }
    if let Some(joined) = offer.pension_joined {
        lines.push(t(lang, if joined { Key::HandoffPensionYes } else { Key::HandoffPensionNo }).to_owned());
    }
    if let Some(resident) = offer.resident {
        lines.push(t(lang, if resident { Key::HandoffResidentYes } else { Key::HandoffResidentNo }).to_owned());
    }
    lines
}

/// Подпись гражданства — та же, что у выбора в форме.
const fn residency_key(residency: Residency) -> Key {
    match residency {
        Residency::Citizen => Key::ResidencyCitizen,
        Residency::ForeignWithStatus => Key::ResidencyForeignWithStatus,
        Residency::ForeignWithoutStatus => Key::ResidencyForeignWithoutStatus,
    }
}

/// Итог или форма — форма в той же части: без экрана её не открыть,
/// а своя часть у неё умножала бы общие куски частей (`ui/split.rs`).
/// Пока база открывается — пусто: «открываем…» только мигнуло бы.
#[component]
fn ProfileBody(ctx: Ctx) -> impl IntoView {
    let Ctx {
        lang,
        profiles,
        base,
        backups,
        editing,
        saved,
        focus,
        forms,
        licenses,
        today,
        router,
    } = ctx;
    let open = move |part: Option<Part>| {
        saved.set(false);
        focus.set(part);
        editing.set(true);
    };
    move || {
        let state = profiles.state.get();
        let body = match state {
            State::Loading => return None,
            State::Failed(error) => view! {
                <p role="alert">{tr(lang, error_key(error))}</p>
            }
            .into_any(),
            State::Ready(profile) if editing.get() => {
                let data = profile.map(|profile| profile.data().clone());
                let focus = focus.get_untracked();
                let brief = profiles.brief.get_untracked();
                view! { <ProfileForm lang data profiles licenses base backups editing saved forms today focus brief router /> }
                    .into_any()
            }
            State::Ready(Some(profile)) => view! {
                {move || saved.get().then(|| view! {
                    <p role="status">{tr(lang, Key::ProfileSaved)}</p>
                })}
                <Summary lang data=profile.data().clone() picture=profiles.picture today open />
                <button type="button" class="secondary" on:click=move |_| open(None)>
                    <span class="label">{tr(lang, Key::ProfileEdit)}</span>
                </button>
            }
            .into_any(),
            State::Ready(None) => view! {
                <p>{tr(lang, Key::ProfileAbout)}</p>
                <button type="button" on:click=move |_| open(None)>
                    <span class="label">{tr(lang, Key::ProfileFill)}</span>
                </button>
            }
            .into_any(),
        };
        Some(body)
    }
}

/// Имя ИП на языке окна, а нет его — на любом из заполненных.
pub(super) fn name_for(name: &Localized<Line>, lang: Lang) -> String {
    std::iter::once(lang)
        .chain(Lang::ALL)
        .map(|lang| name.get(lang).as_str())
        .find(|text| !text.is_empty())
        .unwrap_or_default()
        .to_owned()
}

/// Предупреждение: код закрывает налог с оборота на день `today`;
/// `None` — не закрывает или часы без даты.
fn excluded_warning(
    lang: ReadSignal<Lang>,
    code: ActivityCode,
    today: Option<Date>,
) -> Option<impl IntoView + use<>> {
    let rule = turnover::excluded(&code, today?)?;
    let (shown, from) = (code.to_string(), rule.from);
    Some(view! {
        <p class="warning" role="alert">
            {move || {
                let lang = lang.get();
                fill(t(lang, Key::ActivityExcluded), &[&shown, &i18n::date(lang, from)])
            }}
        </p>
    })
}

/// Строка итога профиля: подпись, коротко — что в ней, и угол вправо. Вся
/// строка — кнопка: открывает форму на своей группе полей.
#[component]
fn SummaryRow<F>(
    lang: ReadSignal<Lang>,
    label: Key,
    meta: String,
    part: Part,
    open: F,
) -> impl IntoView
where
    F: Fn(Option<Part>) + Copy + Send + Sync + 'static,
{
    view! {
        <li>
            <button type="button" class="entry" on:click=move |_| open(Some(part))>
                <span class="entry-main">
                    <span class="entry-title">{tr(lang, label)}</span>
                    <span class="entry-meta">{meta}</span>
                </span>
                {icon(Icon::Forward)}
            </button>
        </li>
    }
}

/// Заполненный профиль коротко (дизайн «Kvartik Design»): карточка ИП —
/// логотип или кружок с инициалами, имя, ՀՎՀՀ и режим, — предупреждения
/// о кодах и строки групп: что попадает в счета и акты и что нужно для
/// налога.
#[component]
fn Summary<F>(
    lang: ReadSignal<Lang>,
    data: ProfileData,
    picture: RwSignal<Option<Arc<Picture>>>,
    today: Option<Date>,
    open: F,
) -> impl IntoView
where
    F: Fn(Option<Part>) + Copy + Send + Sync + 'static,
{
    let name = data.name.clone();
    let initials = data.name.clone();
    let tin = data.tin.as_ref().map(|tin| tin.as_str().to_owned());
    let turnover = data.regime == Some(Regime::Turnover);
    // Языки, на которых имя заполнено, — их имена на них самих.
    let named: Vec<&str> = Lang::ALL
        .into_iter()
        .filter(|lang| !data.name.get(*lang).is_empty())
        .map(Lang::native_name)
        .collect();
    let named = named.join(" · ");
    let accounts = data.accounts.len();
    let codes = data
        .activities
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    let born = data.person.birth_year.map(|year| year.to_string()).unwrap_or_default();
    view! {
        <div class="identity">
            {move || match picture.get() {
                Some(picture) => view! {
                    <img class="identity-logo" src=picture.url().to_owned() alt=move || t(lang.get(), Key::ProfileLogo) />
                }
                .into_any(),
                None => {
                    let initials = initials.clone();
                    view! {
                        <span class="avatar hero" aria-hidden="true">
                            {move || avatar::initials(&name_for(&initials, lang.get()))}
                        </span>
                    }
                    .into_any()
                }
            }}
            <div class="identity-text">
                <p class="identity-name">{move || name_for(&name, lang.get())}</p>
                <p class="dotted">
                    {tin.map(|tin| view! {
                        <span>{move || fill(t(lang.get(), Key::TinShort), &[&tin])}</span>
                    })}
                    {turnover.then(|| view! { <span>{tr(lang, Key::RegimeTurnover)}</span> })}
                </p>
            </div>
        </div>
        {data
            .activities
            .iter()
            .filter_map(|code| excluded_warning(lang, *code, today))
            .collect_view()}
        <h2>{tr(lang, Key::ProfileOnDocuments)}</h2>
        <ul class="list">
            <SummaryRow lang label=Key::ProfileNameAddress meta=named part=Part::Requisites open />
            <SummaryRow
                lang
                label=Key::ProfileAccounts
                meta=accounts.to_string()
                part=Part::Accounts
                open
            />
        </ul>
        <h2>{tr(lang, Key::ProfileForTax)}</h2>
        <ul class="list">
            <SummaryRow lang label=Key::ProfileActivities meta=codes part=Part::Activities open />
            <SummaryRow lang label=Key::ProfilePerson meta=born part=Part::Person open />
        </ul>
    }
}

/// Логотип в форме: оставить сохранённый, поставить новый или убрать.
#[derive(Clone)]
enum LogoChoice {
    Keep,
    New(Arc<Chosen>),
    Remove,
}

/// Выбранная картинка: байты для записи и адрес для показа.
struct Chosen {
    image: Image,
    picture: Arc<Picture>,
}

/// Строка банковского счёта в форме.
#[derive(Clone, Copy)]
struct AccountRow {
    key: u32,
    currency: RwSignal<Currency>,
    number: RwSignal<String>,
    bank: RwSignal<String>,
    swift: RwSignal<String>,
    correspondent: RwSignal<String>,
}

/// Строка кода деятельности в форме.
#[derive(Clone, Copy)]
struct CodeRow {
    key: u32,
    text: RwSignal<String>,
    /// Строка налога, выбранная коду; `None` — по закону (задача 19).
    kind: RwSignal<Option<IncomeKind>>,
}

/// Строка периода остановки в форме: даты — записью ISO, как их отдаёт
/// `<input type="date">`.
#[derive(Clone, Copy)]
struct StopRow {
    key: u32,
    from: RwSignal<String>,
    until: RwSignal<String>,
}

/// Поля формы — строками, как их ввёл человек. Читает их ядро (`parse`
/// у каждого типа), окно только показывает ошибки.
#[derive(Clone, Copy)]
struct Form {
    name: Localized<RwSignal<String>>,
    tin: RwSignal<String>,
    registration: RwSignal<String>,
    address: Localized<RwSignal<String>>,
    phone: RwSignal<String>,
    email: RwSignal<String>,
    accounts: RwSignal<Vec<AccountRow>>,
    activities: RwSignal<Vec<CodeRow>>,
    regime: RwSignal<Option<Regime>>,
    birth_year: RwSignal<String>,
    pension_joined: RwSignal<Option<bool>>,
    /// День заявления о фонде — записью ISO, как и день регистрации.
    pension_applied: RwSignal<String>,
    residency: RwSignal<Option<Residency>>,
    resident: RwSignal<Option<bool>>,
    /// День регистрации ИП — записью ISO, как его отдаёт `<input type="date">`.
    registered: RwSignal<String>,
    /// День подачи заявления о налоге с оборота нового ИП — так же.
    turnover_applied: RwSignal<String>,
    stops: RwSignal<Vec<StopRow>>,
    logo: RwSignal<LogoChoice>,
    logo_problem: RwSignal<Option<LogoProblem>>,
    /// Сохранённый логотип — то, что останется при `LogoChoice::Keep`.
    saved_picture: RwSignal<Option<Arc<Picture>>>,
    /// Ключ следующей строки списка: у строки он свой до конца формы,
    /// и `For` не путает строки после удаления соседней.
    next_row: StoredValue<u32>,
    /// Была ли попытка сохранить: после неё ошибки полей видны.
    tried: RwSignal<bool>,
    /// Идёт сохранение.
    busy: RwSignal<bool>,
}

/// Почему не принят логотип.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum LogoProblem {
    TooLarge,
    NotSupported,
    Unreadable,
}

impl LogoProblem {
    fn text(self, lang: Lang) -> String {
        match self {
            Self::TooLarge => fill(
                t(lang, Key::LogoTooLarge),
                &[&i18n::integer(lang, (logo::MAX_BYTES / 1024) as i64)],
            ),
            Self::NotSupported => t(lang, Key::LogoNotSupported).to_owned(),
            Self::Unreadable => t(lang, Key::BackupFileUnreadable).to_owned(),
        }
    }
}

fn localized(text: impl Fn(Lang) -> String) -> Localized<RwSignal<String>> {
    Localized {
        hy: RwSignal::new(text(Lang::Am)),
        ru: RwSignal::new(text(Lang::Ru)),
        en: RwSignal::new(text(Lang::En)),
    }
}

impl Form {
    /// Форма с данными сохранённого профиля или пустая.
    fn new(data: Option<&ProfileData>, saved_picture: RwSignal<Option<Arc<Picture>>>) -> Self {
        let empty = ProfileData::default();
        let data = data.unwrap_or(&empty);
        let form = Self {
            name: localized(|lang| data.name.get(lang).as_str().to_owned()),
            tin: RwSignal::new(data.tin.as_ref().map(|tin| tin.as_str().to_owned()).unwrap_or_default()),
            registration: RwSignal::new(data.registration.as_str().to_owned()),
            address: localized(|lang| data.address.get(lang).as_str().to_owned()),
            phone: RwSignal::new(data.phone.as_str().to_owned()),
            email: RwSignal::new(
                data.email.as_ref().map(|email| email.as_str().to_owned()).unwrap_or_default(),
            ),
            accounts: RwSignal::new(Vec::new()),
            activities: RwSignal::new(Vec::new()),
            regime: RwSignal::new(data.regime),
            birth_year: RwSignal::new(
                data.person.birth_year.map(|year| year.to_string()).unwrap_or_default(),
            ),
            pension_joined: RwSignal::new(data.person.pension_joined),
            pension_applied: RwSignal::new(data.person.pension_applied.map(Date::iso).unwrap_or_default()),
            residency: RwSignal::new(data.person.residency),
            resident: RwSignal::new(data.person.resident),
            registered: RwSignal::new(data.person.registered.map(Date::iso).unwrap_or_default()),
            turnover_applied: RwSignal::new(data.person.turnover_applied.map(Date::iso).unwrap_or_default()),
            stops: RwSignal::new(Vec::new()),
            logo: RwSignal::new(LogoChoice::Keep),
            logo_problem: RwSignal::new(None),
            saved_picture,
            next_row: StoredValue::new(0),
            tried: RwSignal::new(false),
            busy: RwSignal::new(false),
        };
        let accounts = data.accounts.iter().map(|account| {
            let row = form.account_row(account.currency());
            row.number.set(account.number().as_str().to_owned());
            row.bank.set(account.bank().as_str().to_owned());
            row.swift.set(account.swift().map(|swift| swift.as_str().to_owned()).unwrap_or_default());
            row.correspondent.set(account.correspondent().as_str().to_owned());
            row
        });
        form.accounts.set(accounts.collect());
        let codes = data
            .activities
            .iter()
            .map(|code| form.code_row(code.to_string(), data.kinds.get(code).copied()));
        form.activities.set(codes.collect());
        let stops = data.person.stops.iter().map(|stop| {
            let row = form.stop_row();
            row.from.set(stop.from().iso());
            row.until.set(stop.until().map(Date::iso).unwrap_or_default());
            row
        });
        form.stops.set(stops.collect());
        form
    }

    fn key(&self) -> u32 {
        let key = self.next_row.get_value();
        self.next_row.set_value(key + 1);
        key
    }

    fn account_row(&self, currency: Currency) -> AccountRow {
        AccountRow {
            key: self.key(),
            currency: RwSignal::new(currency),
            number: RwSignal::new(String::new()),
            bank: RwSignal::new(String::new()),
            swift: RwSignal::new(String::new()),
            correspondent: RwSignal::new(String::new()),
        }
    }

    fn code_row(&self, text: String, kind: Option<IncomeKind>) -> CodeRow {
        CodeRow {
            key: self.key(),
            text: RwSignal::new(text),
            kind: RwSignal::new(kind),
        }
    }

    fn stop_row(&self) -> StopRow {
        StopRow {
            key: self.key(),
            from: RwSignal::new(String::new()),
            until: RwSignal::new(String::new()),
        }
    }

    /// Подставить переданное калькулятором лендинга (задача 34) — только
    /// в пустые поля: заполненное человеком не переписывается. Строка налога —
    /// у единственной строки кода без строки налога, а строк кода нет — у новой
    /// пустой. `true` — строка налога стоит у строки без кода: пустой код
    /// форма не сохраняет, и строка пропала бы молча, поэтому форма говорит
    /// об этом.
    fn handoff(&self, handed: Handoff) -> bool {
        if let Some(year) = handed.birth_year
            && self.birth_year.with_untracked(|text| text.trim().is_empty())
        {
            self.birth_year.set(year.to_string());
        }
        for (field, value) in [(self.pension_joined, handed.pension_joined), (self.resident, handed.resident)] {
            if field.get_untracked().is_none() {
                field.set(value);
            }
        }
        if self.residency.get_untracked().is_none() {
            self.residency.set(handed.residency);
        }
        let Some(kind) = handed.kind else {
            return false;
        };
        match self.activities.get_untracked().as_slice() {
            [] => {
                let row = self.code_row(String::new(), Some(kind));
                self.activities.set(vec![row]);
                true
            }
            [row] if row.kind.get_untracked().is_none() => {
                row.kind.set(Some(kind));
                row.text.with_untracked(|text| text.trim().is_empty())
            }
            _ => false,
        }
    }

    /// Данные профиля из полей; `None` — где-то ошибка, и её видно у поля.
    fn data(&self, today: Option<Date>) -> Option<ProfileData> {
        let line = |field: RwSignal<String>| Line::new(&field.get_untracked()).ok();
        let lines = |field: RwSignal<String>| Lines::new(&field.get_untracked()).ok();
        let accounts = self
            .accounts
            .get_untracked()
            .iter()
            .filter(|row| !row.is_blank())
            .map(AccountRow::account)
            .collect::<Option<Vec<_>>>()?;
        let rows: Vec<CodeRow> = self
            .activities
            .get_untracked()
            .into_iter()
            .filter(|row| !row.text.get_untracked().trim().is_empty())
            .collect();
        let activities = rows
            .iter()
            .map(|row| ActivityCode::parse(&row.text.get_untracked()).ok())
            .collect::<Option<Vec<_>>>()?;
        let kinds: BTreeMap<ActivityCode, IncomeKind> = activities
            .iter()
            .zip(&rows)
            .filter_map(|(code, row)| Some((*code, row.kind.get_untracked()?)))
            .collect();
        let stops = self
            .stops
            .get_untracked()
            .iter()
            .filter(|row| !row.is_blank())
            .map(StopRow::stop)
            .collect::<Option<Vec<_>>>()?;
        let tin = optional(&self.tin.get_untracked(), Tin::parse)?;
        let email = optional(&self.email.get_untracked(), Email::parse)?;
        let birth_year = optional(&self.birth_year.get_untracked(), |text| {
            birth_year(text, today).ok_or(())
        })?;
        let registered = optional(&self.registered.get_untracked(), |text| {
            registered(text, today).ok_or(())
        })?;
        // Спрятанное поле не мешает сохранить: ошибки под ним не видно, а день
        // остаётся как был — что он значит, решает расчёт (`tax::annual`).
        let pension_day_shown = self.pension_joined.get_untracked() == Some(true)
            && self.birth_year.with_untracked(|text| asks_pension(text, today));
        let pension_applied = if pension_day_shown {
            optional(&self.pension_applied.get_untracked(), |text| past_day(text, today).ok_or(()))?
        } else {
            Date::from_iso(self.pension_applied.get_untracked().trim())
        };
        // Так же день подачи заявления: спрятан — остаётся как был.
        let applied_shown = asks_applied(
            &self.registered.get_untracked(),
            &self.turnover_applied.get_untracked(),
            today,
        );
        let turnover_applied = if applied_shown {
            optional(&self.turnover_applied.get_untracked(), |text| past_day(text, today).ok_or(()))?
        } else {
            Date::from_iso(self.turnover_applied.get_untracked().trim())
        };
        Some(ProfileData {
            name: Localized {
                hy: line(self.name.hy)?,
                ru: line(self.name.ru)?,
                en: line(self.name.en)?,
            },
            tin,
            registration: line(self.registration)?,
            address: Localized {
                hy: lines(self.address.hy)?,
                ru: lines(self.address.ru)?,
                en: lines(self.address.en)?,
            },
            phone: line(self.phone)?,
            email,
            accounts,
            activities,
            kinds,
            regime: self.regime.get_untracked(),
            person: Person {
                birth_year,
                pension_joined: self.pension_joined.get_untracked(),
                pension_applied,
                residency: self.residency.get_untracked(),
                resident: self.resident.get_untracked(),
                registered,
                turnover_applied,
                stops,
            },
        })
    }
}

/// Спрашивать ли день подачи заявления о налоге с оборота нового ИП: день
/// уже есть (`applied` — поле дня) — или ИП зарегистрирован (`registered` —
/// поле дня регистрации) в этом году либо в прошлом так поздно, что срок
/// заявления пришёлся на этот (`deadlines::new_ie_due`). Давно работающему ИП
/// поле только мешало бы: о заявлениях следующих лет оно не говорит.
fn asks_applied(registered_text: &str, applied: &str, today: Option<Date>) -> bool {
    let new_ie = registered(registered_text, today).zip(today).is_some_and(|(day, today)| {
        day.year() == today.year() || deadlines::new_ie_due(day).is_some_and(|due| due.year() == today.year())
    });
    !applied.trim().is_empty() || new_ie
}

/// Спрашивать ли о накопительной пенсии — только тех, для кого она по выбору
/// (`tax::annual`): родившихся раньше, чем начинается обязательное участие
/// по правилу на сегодня. `text` — поле года рождения.
fn asks_pension(text: &str, today: Option<Date>) -> bool {
    birth_year(text, today)
        .zip(today)
        .is_some_and(|(year, today)| annual::pension_is_a_choice(year, today))
}

/// День регистрации ИП из поля даты: не позже сегодняшнего дня.
fn registered(text: &str, today: Option<Date>) -> Option<Date> {
    past_day(text, today)
}

/// День из поля даты: не позже сегодняшнего. Часы без даты — без верхней
/// границы.
fn past_day(text: &str, today: Option<Date>) -> Option<Date> {
    Date::from_iso(text.trim()).filter(|day| today.is_none_or(|today| *day <= today))
}

impl AccountRow {
    /// Пустая строка счёта не сохраняется и ошибкой не считается.
    fn is_blank(&self) -> bool {
        [self.number, self.bank, self.swift, self.correspondent]
            .iter()
            .all(|field| field.get_untracked().trim().is_empty())
    }

    fn account(&self) -> Option<BankAccount> {
        let swift = optional(&self.swift.get_untracked(), Swift::parse)?;
        BankAccount::new(
            self.currency.get_untracked(),
            Line::new(&self.number.get_untracked()).ok()?,
            Line::new(&self.bank.get_untracked()).ok()?,
            swift,
            Lines::new(&self.correspondent.get_untracked()).ok()?,
        )
        .ok()
    }
}

impl StopRow {
    fn is_blank(&self) -> bool {
        self.from.get_untracked().is_empty() && self.until.get_untracked().is_empty()
    }

    fn stop(&self) -> Option<Stop> {
        stop(&self.from.get_untracked(), &self.until.get_untracked())
    }
}

/// Период из полей дат: день остановки обязателен, день возобновления — нет.
fn stop(from: &str, until: &str) -> Option<Stop> {
    let from = Date::from_iso(from)?;
    let until = match until {
        "" => None,
        text => Some(Date::from_iso(text)?),
    };
    Stop::new(from, until).ok()
}

const fn code_key(error: CodeError) -> Key {
    match error {
        CodeError::Malformed | CodeError::NoSuchDivision | CodeError::WrongSection => {
            Key::ActivityCodeInvalid
        }
    }
}

/// Форма профиля.
#[component]
fn ProfileForm(
    lang: ReadSignal<Lang>,
    data: Option<ProfileData>,
    profiles: Profiles,
    licenses: Licenses,
    base: Base,
    backups: Backups,
    editing: RwSignal<bool>,
    saved: RwSignal<bool>,
    forms: Forms,
    today: Option<Date>,
    focus: Option<Part>,
    brief: bool,
    router: Router,
) -> impl IntoView {
    forms.hold();
    let form = Form::new(data.as_ref(), profiles.picture);
    // Открыли предложением калькулятора лендинга (задача 34) — подставить
    // переданное. Один раз: форма, открытая потом иначе, его не получит.
    let handed = profiles.handed.get_untracked();
    profiles.handed.set(None);
    let code_needed = handed.is_some_and(|handed| form.handoff(handed));
    // Короткая форма сразу со строками счёта и кода: пустые не сохраняются.
    if brief {
        if form.accounts.with_untracked(Vec::is_empty) {
            form.accounts.set(vec![form.account_row(Currency::Amd)]);
        }
        if form.activities.with_untracked(Vec::is_empty) {
            form.activities.set(vec![form.code_row(String::new(), None)]);
        }
    }
    let close = move || {
        profiles.brief.set(false);
        editing.set(false);
        router.form_closed();
    };
    let saved_tin = data.as_ref().and_then(|data| data.tin.clone());
    licenses.watch(base);
    // Открыли по строке итога — к её группе полей; после кадра: к нему
    // форма уже в странице.
    if let Some(part) = focus {
        request_animation_frame(move || {
            let group = web_sys::window()
                .and_then(|window| window.document())
                .and_then(|document| document.get_element_by_id(part.id()));
            if let Some(group) = group {
                group.scroll_into_view();
            }
        });
    }
    let failure = RwSignal::new(None::<StoreError>);
    let invalid = RwSignal::new(false);

    let submit = move |event: SubmitEvent| {
        event.prevent_default();
        if form.busy.get_untracked() {
            return;
        }
        form.tried.set(true);
        let Some(data) = form.data(today) else {
            invalid.set(true);
            return;
        };
        invalid.set(false);
        failure.set(None);
        let Some(store) = base.get() else {
            failure.set(Some(StoreError::Failed));
            return;
        };
        let choice = form.logo.get_untracked();
        form.busy.set(true);
        leptos::task::spawn_local(async move {
            let outcome = save(&store, data, &choice).await;
            form.busy.set(false);
            match outcome {
                Ok(profile) => {
                    match choice {
                        LogoChoice::Keep => {}
                        LogoChoice::New(chosen) => {
                            profiles.picture.set(Some(Arc::clone(&chosen.picture)));
                        }
                        LogoChoice::Remove => profiles.picture.set(None),
                    }
                    profiles.state.set(State::Ready(Some(Arc::new(profile))));
                    backups.data_saved();
                    // Взятое из калькулятора сохранено вместе с формой: больше
                    // его не предлагать.
                    if handed.is_some() {
                        profiles.drop_handoff();
                    }
                    saved.set(true);
                    close();
                }
                Err(error) => failure.set(Some(error)),
            }
        });
    };

    view! {
        <form class="profile" on:submit=submit novalidate>
            {brief.then(|| view! { <p class="hint">{tr(lang, Key::ProfileBriefAbout)}</p> })}
            {handed.is_some().then(|| view! { <p role="status">{tr(lang, Key::HandoffApplied)}</p> })}
            <Requisites lang form licenses saved_tin brief />
            <Accounts lang form />
            <Activities lang form today code_needed />
            {(!brief).then(|| view! { <PersonFields lang form today /> })}
            {move || invalid.get().then(|| view! {
                <p role="alert">{tr(lang, Key::ProfileFix)}</p>
            })}
            {move || failure.get().map(|error| view! {
                <p role="alert">{tr(lang, error_key(error))}</p>
            })}
            <div class="actions">
                <button type="submit" disabled=move || form.busy.get()>
                    <span class="label">{tr(lang, Key::ProfileSave)}</span>
                </button>
                <button
                    type="button"
                    class="secondary"
                    disabled=move || form.busy.get()
                    on:click=move |_| close()
                >
                    <span class="label">{tr(lang, Key::Cancel)}</span>
                </button>
            </div>
        </form>
    }
}

/// Сохранить профиль и логотип — одним действием: команды ядра
/// и одна транзакция. Между чтением и записью — только расчёт ядра и запросы
/// этой же транзакции (шапка `store/mod.rs`).
async fn save(store: &Store, data: ProfileData, choice: &LogoChoice) -> Result<Profile, StoreError> {
    let tx = store.write()?;
    let existing = Profile::current(tx.all::<Profile>().await?);
    let now = clock::now();
    let is_new = existing.is_none();
    let unchanged = existing.clone();
    let mut writes = Writes::default();
    let profile = match Profile::save(existing, data, now) {
        Ok(Some(profile)) => {
            if is_new {
                writes.insert(&profile);
            } else {
                writes.update(&profile);
            }
            profile
        }
        // Данные не изменились: записывать профиль нечего.
        Ok(None) => unchanged.ok_or(StoreError::Failed)?,
        // Списки длиннее предела — форма до него не доходит.
        Err(_) => return Err(StoreError::Failed),
    };
    let logo = match choice {
        LogoChoice::Keep => None,
        LogoChoice::New(chosen) => {
            // Нечитаемую запись логотипа новая заменяет целиком.
            let old = match tx.get::<Logo>(profile.id()).await {
                Err(StoreError::Damaged) => None,
                read => read?,
            };
            Some(Logo::set(old, &profile, chosen.image.clone(), now))
        }
        LogoChoice::Remove => live_logo(tx.get::<Logo>(profile.id()).await)?
            .map(|logo| logo.remove(now)),
    };
    // Логотип лежит под идентификатором профиля, и его запись — всегда
    // правка: нового идентификатора, который мог бы совпасть с чужим, у неё
    // не бывает.
    if let Some(logo) = &logo {
        writes.update(logo);
    }
    tx.commit(writes).await?;
    Ok(profile)
}

/// Реквизиты: имя и адрес на трёх языках, ՀՎՀՀ, регистрация, связь,
/// логотип. Под ՀՎՀՀ — сразу при вводе: ключ лицензии выдан на сохранённый
/// ИНН, и с другим он не откроет выпуск (задача 25); сохранить это не мешает
/// — данные не запираются (Правило 2). Короткая форма без номера регистрации,
/// связи и логотипа: в счёт они не обязательны, а сигналы их остаются в форме
/// и сохраняются как были.
#[component]
fn Requisites(
    lang: ReadSignal<Lang>,
    form: Form,
    licenses: Licenses,
    saved_tin: Option<Tin>,
    brief: bool,
) -> impl IntoView {
    let tried = form.tried;
    let tin_license = move || {
        let saved = saved_tin.clone()?;
        let same = form.tin.with(|typed| Tin::parse(typed).is_ok_and(|tin| tin == saved));
        (!same && licenses.issued_to(&saved)).then(|| {
            let number = saved.as_str().to_owned();
            view! {
                <p class="warning" role="status">
                    {move || fill(t(lang.get(), Key::ProfileTinLicense), &[&number])}
                </p>
            }
        })
    };
    view! {
        <fieldset id=Part::Requisites.id()>
            <legend>{tr(lang, Key::ProfileRequisites)}</legend>
            <p class="hint">{tr(lang, Key::ProfileNameHint)}</p>
            <TextField lang label=Key::ProfileNameHy value=form.name.hy tried autocomplete="organization" />
            <TextField lang label=Key::ProfileNameRu value=form.name.ru tried autocomplete="off" />
            <TextField lang label=Key::ProfileNameEn value=form.name.en tried autocomplete="off" />
            <TextField
                lang
                label=Key::ProfileTin
                value=form.tin
                tried
                inputmode="numeric"
                autocomplete="off"
                hint=Key::ProfileTinHint
                max=Tin::DIGITS
                check=|text| (!text.trim().is_empty() && Tin::parse(text).is_err()).then_some(Key::TinInvalid)
            />
            {tin_license}
            {(!brief).then(|| view! {
                <TextField
                    lang
                    label=Key::ProfileRegistration
                    value=form.registration
                    tried
                    autocomplete="off"
                />
            })}
            <AreaField lang label=Key::ProfileAddressHy value=form.address.hy tried autocomplete="street-address" />
            <AreaField lang label=Key::ProfileAddressRu value=form.address.ru tried />
            <AreaField lang label=Key::ProfileAddressEn value=form.address.en tried />
            {(!brief).then(|| view! {
                <TextField lang label=Key::ProfilePhone value=form.phone tried kind="tel" autocomplete="tel" />
                <TextField
                    lang
                    label=Key::ProfileEmail
                    value=form.email
                    tried
                    kind="email"
                    autocomplete="email"
                    max=Email::MAX_CHARS
                    check=|text| (!text.trim().is_empty() && Email::parse(text).is_err()).then_some(Key::EmailInvalid)
                />
                <LogoField lang form />
            })}
        </fieldset>
    }
}

/// Логотип: показать, выбрать другой, убрать.
#[component]
fn LogoField(lang: ReadSignal<Lang>, form: Form) -> impl IntoView {
    // Файл читается сразу при выборе (`platform::file::read_bytes`).
    let on_file = move |event: Event| {
        let input: HtmlInputElement = event_target(&event);
        let Some(chosen) = file::chosen(&input) else {
            return;
        };
        leptos::task::spawn_local(async move {
            let read = file::read_bytes(&chosen, logo::MAX_BYTES as u64).await;
            // Тот же файл можно будет выбрать снова.
            input.set_value("");
            let image = match read {
                Ok(bytes) => Image::new(bytes).map_err(|error| match error {
                    ImageError::TooLarge => LogoProblem::TooLarge,
                    ImageError::NotSupported => LogoProblem::NotSupported,
                }),
                Err(FileError::TooLarge) => Err(LogoProblem::TooLarge),
                Err(FileError::NotText | FileError::Unreadable) => Err(LogoProblem::Unreadable),
            };
            match image {
                Ok(image) => match Picture::new(image.bytes(), image.format().mime()) {
                    Some(picture) => {
                        form.logo_problem.set(None);
                        form.logo.set(LogoChoice::New(Arc::new(Chosen {
                            image,
                            picture: Arc::new(picture),
                        })));
                    }
                    None => form.logo_problem.set(Some(LogoProblem::Unreadable)),
                },
                Err(problem) => form.logo_problem.set(Some(problem)),
            }
        });
    };
    view! {
        <div class="logo-field">
            <p>{tr(lang, Key::ProfileLogo)}</p>
            <LogoPreview lang form />
            <div class="actions">
                <label class="button secondary file">
                    {tr(lang, Key::LogoChoose)}
                    <input
                        type="file"
                        class="visually-hidden"
                        accept="image/png,image/jpeg"
                        on:change=on_file
                    />
                </label>
                <LogoRemove lang form />
            </div>
            <p class="hint">
                {move || {
                    let lang = lang.get();
                    fill(
                        t(lang, Key::LogoHint),
                        &[&i18n::integer(lang, (logo::MAX_BYTES / 1024) as i64)],
                    )
                }}
            </p>
            {move || form.logo_problem.get().map(|problem| view! {
                <p class="field-error" role="alert">{move || problem.text(lang.get())}</p>
            })}
        </div>
    }
}

/// Картинка, которая будет логотипом после сохранения.
#[component]
fn LogoPreview(lang: ReadSignal<Lang>, form: Form) -> impl IntoView {
    move || {
        let url = match form.logo.get() {
            LogoChoice::Keep => form.saved_picture.get().map(|picture| picture.url().to_owned()),
            LogoChoice::New(chosen) => Some(chosen.picture.url().to_owned()),
            LogoChoice::Remove => None,
        };
        url.map(|url| view! {
            <img class="logo" src=url alt=move || t(lang.get(), Key::ProfileLogo) />
        })
    }
}

/// «Убрать логотип» — когда он есть.
#[component]
fn LogoRemove(lang: ReadSignal<Lang>, form: Form) -> impl IntoView {
    move || {
        let shown = match form.logo.get() {
            LogoChoice::Keep => form.saved_picture.with(Option::is_some),
            LogoChoice::New(_) => true,
            LogoChoice::Remove => false,
        };
        shown.then(|| view! {
            <button
                type="button"
                class="secondary"
                on:click=move |_| {
                    form.logo_problem.set(None);
                    form.logo.set(LogoChoice::Remove);
                }
            >
                <span class="label">{tr(lang, Key::LogoRemove)}</span>
            </button>
        })
    }
}

/// Банковские счета.
#[component]
fn Accounts(lang: ReadSignal<Lang>, form: Form) -> impl IntoView {
    let tried = form.tried;
    let full = move || form.accounts.with(|rows| rows.len() >= ProfileData::MAX_ACCOUNTS);
    view! {
        <fieldset id=Part::Accounts.id()>
            <legend>{tr(lang, Key::ProfileAccounts)}</legend>
            <For each=move || form.accounts.get() key=|row| row.key let(row)>
                <div class="row">
                    <label>
                        {tr(lang, Key::AccountCurrency)}
                        <select on:change=move |event| {
                            let code = event_target_value(&event);
                            if let Some(currency) = CURRENCIES
                                .into_iter()
                                .find(|currency| currency_code(*currency) == code)
                            {
                                row.currency.set(currency);
                            }
                        }>
                            {CURRENCIES
                                .into_iter()
                                .map(|currency| view! {
                                    <option
                                        value=currency_code(currency)
                                        prop:selected=move || row.currency.get() == currency
                                    >
                                        {tr(lang, currency_key(currency))}
                                    </option>
                                })
                                .collect_view()}
                        </select>
                    </label>
                    <TextField lang label=Key::AccountNumber value=row.number tried autocomplete="off" />
                    // Пустая строка счёта не сохраняется, а начатая — без
                    // номера не годится.
                    <FieldError
                        lang
                        tried
                        error=move || {
                            let started = [row.bank, row.swift, row.correspondent]
                                .iter()
                                .any(|field| field.with(|text| !text.trim().is_empty()));
                            let missing = row.number.with(|text| text.trim().is_empty());
                            (started && missing).then_some(Key::AccountNumberMissing)
                        }
                    />
                    // Сразу, ещё до сохранения, и сохранить не мешает
                    // (задача 56): счёт бывает и в чужом банке.
                    {move || {
                        let swift = row.swift.with(|text| Swift::parse(text).ok());
                        row.number
                            .with(|number| BankAccount::mistyped(number, swift.as_ref()))
                            .then(|| view! {
                                <p class="warning" role="alert">{tr(lang, Key::AccountNumberTypo)}</p>
                            })
                    }}
                    <TextField lang label=Key::AccountBank value=row.bank tried autocomplete="off" />
                    <TextField
                        lang
                        label=Key::AccountSwift
                        value=row.swift
                        tried
                        autocomplete="off"
                        max=11
                        check=|text| (!text.trim().is_empty() && Swift::parse(text).is_err()).then_some(Key::SwiftInvalid)
                    />
                    <AreaField
                        lang
                        label=Key::AccountCorrespondent
                        value=row.correspondent
                        tried
                        hint=Key::AccountCorrespondentHint
                    />
                    <button
                        type="button"
                        class="secondary"
                        on:click=move |_| form.accounts.update(|rows| rows.retain(|r| r.key != row.key))
                    >
                        <span class="label">{tr(lang, Key::AccountRemove)}</span>
                    </button>
                </div>
            </For>
            <button
                type="button"
                class="secondary"
                disabled=full
                on:click=move |_| {
                    let row = form.account_row(Currency::Amd);
                    form.accounts.update(|rows| rows.push(row));
                }
            >
                <span class="label">{tr(lang, Key::AccountAdd)}</span>
            </button>
        </fieldset>
    }
}

/// Коды видов деятельности и режим налогообложения.
#[component]
fn Activities(
    lang: ReadSignal<Lang>,
    form: Form,
    today: Option<Date>,
    /// Калькулятор лендинга поставил строку налога у строки без кода
    /// (задача 34): пустой код форма не сохраняет, и об этом — здесь же, над
    /// кодом.
    code_needed: bool,
) -> impl IntoView {
    let tried = form.tried;
    let full = move || form.activities.with(|rows| rows.len() >= ProfileData::MAX_ACTIVITIES);
    view! {
        <fieldset id=Part::Activities.id()>
            <legend>{tr(lang, Key::ProfileActivities)}</legend>
            <p class="hint">{tr(lang, Key::ActivityHint)}</p>
            {code_needed.then(|| view! { <p role="status">{tr(lang, Key::HandoffAppliedCode)}</p> })}
            <For each=move || form.activities.get() key=|row| row.key let(row)>
                <div class="row">
                    <TextField
                        lang
                        label=Key::ActivityCode
                        value=row.text
                        tried
                        autocomplete="off"
                        max=ActivityCode::MAX_INPUT
                        check=|text| {
                            if text.trim().is_empty() {
                                None
                            } else {
                                ActivityCode::parse(text).err().map(code_key)
                            }
                        }
                    />
                    // Строка налога: по коду закон узнаёт только высокие
                    // технологии, остальное человек выбирает сам (задача 19).
                    <label>
                        {tr(lang, Key::ActivityKind)}
                        <select on:change=move |event| {
                            let chosen = event_target_value(&event);
                            row.kind.set(IncomeKind::ALL.into_iter().find(|kind| tax::kind_code(*kind) == chosen));
                        }>
                            <option value="" prop:selected=move || row.kind.get().is_none()>
                                {move || {
                                    let lang = lang.get();
                                    match (ActivityCode::parse(&row.text.get()).ok(), today) {
                                        (Some(code), Some(day)) => {
                                            let default = turnover::default_kind(&code, day);
                                            fill(t(lang, Key::ActivityKindDefault), &[&tax::kind_label(lang, default, today)])
                                        }
                                        _ => t(lang, Key::NotChosen).to_owned(),
                                    }
                                }}
                            </option>
                            {IncomeKind::ALL
                                .into_iter()
                                .map(|kind| view! {
                                    <option value=tax::kind_code(kind) prop:selected=move || row.kind.get() == Some(kind)>
                                        {move || tax::kind_label(lang.get(), kind, today)}
                                    </option>
                                })
                                .collect_view()}
                        </select>
                    </label>
                    // Сразу, ещё до сохранения (задача 9).
                    {move || {
                        let code = ActivityCode::parse(&row.text.get()).ok()?;
                        excluded_warning(lang, code, today)
                    }}
                    // Что значит этот код: ставка и ближайший срок (задача 28).
                    {move || {
                        let day = today?;
                        let code = ActivityCode::parse(&row.text.get()).ok()?;
                        if turnover::excluded(&code, day).is_some() {
                            return None;
                        }
                        let kind = row.kind.get().unwrap_or_else(|| turnover::default_kind(&code, day));
                        let rate = turnover::rate(kind, day)?.rate;
                        let quarter = day.quarter();
                        let due = deadlines::turnover(quarter)?.day;
                        Some(view! {
                            <p class="hint">
                                {move || {
                                    let lang = lang.get();
                                    fill(
                                        t(lang, Key::ActivityRateDue),
                                        &[&tax::percent(lang, rate), &i18n::quarter(lang, quarter), &i18n::date(lang, due)],
                                    )
                                }}
                            </p>
                        })
                    }}
                    <button
                        type="button"
                        class="secondary"
                        on:click=move |_| form.activities.update(|rows| rows.retain(|r| r.key != row.key))
                    >
                        <span class="label">{tr(lang, Key::Remove)}</span>
                    </button>
                </div>
            </For>
            <button
                type="button"
                class="secondary"
                disabled=full
                on:click=move |_| {
                    let row = form.code_row(String::new(), None);
                    form.activities.update(|rows| rows.push(row));
                }
            >
                <span class="label">{tr(lang, Key::ActivityAdd)}</span>
            </button>
            <p class="hint">{tr(lang, Key::ActivityKindHint)}</p>
            <fieldset class="group">
                <legend>{tr(lang, Key::ProfileRegime)}</legend>
                <Choice lang name="regime" label=Key::RegimeTurnover value=Regime::Turnover chosen=form.regime />
                <p class="hint">{tr(lang, Key::RegimeOthers)}</p>
            </fieldset>
        </fieldset>
    }
}

/// Один вариант выбора-переключателя (`radio`): выбран, когда сигнал равен
/// значению; нажатие ставит значение.
#[component]
fn Choice<T>(
    lang: ReadSignal<Lang>,
    name: &'static str,
    label: Key,
    value: T,
    chosen: RwSignal<Option<T>>,
) -> impl IntoView
where
    T: Copy + PartialEq + Send + Sync + 'static,
{
    view! {
        <label class="choice">
            <input
                type="radio"
                name=name
                prop:checked=move || chosen.get() == Some(value)
                on:change=move |_| chosen.set(Some(value))
            />
            {tr(lang, label)}
        </label>
    }
}

/// Данные человека для годовых платежей.
#[component]
fn PersonFields(lang: ReadSignal<Lang>, form: Form, today: Option<Date>) -> impl IntoView {
    let tried = form.tried;
    let pension_asked = move || form.birth_year.with(|text| asks_pension(text, today));
    let pension_day_asked = move || pension_asked() && form.pension_joined.get() == Some(true);
    // Вопрос о резидентстве — гражданину в возрасте, с которого за резидента
    // взнос медстраховки платит государство.
    let resident_asked = move || {
        form.residency.get() == Some(Residency::Citizen)
            && form
                .birth_year
                .with(|text| birth_year(text, today))
                .zip(today)
                .is_some_and(|(year, today)| annual::residence_matters(year, today))
    };
    let state_pays_from = today.and_then(annual::state_pays_from_age).unwrap_or_default();
    let full = move || form.stops.with(|rows| rows.len() >= ProfileData::MAX_STOPS);
    view! {
        <fieldset id=Part::Person.id()>
            <legend>{tr(lang, Key::ProfilePerson)}</legend>
            <p class="hint">{tr(lang, Key::ProfilePersonAbout)}</p>
            <label>
                {tr(lang, Key::BirthYear)}
                <input
                    type="text"
                    inputmode="numeric"
                    autocomplete="bday-year"
                    maxlength="4"
                    bind:value=form.birth_year
                />
            </label>
            <FieldError
                lang
                tried
                error=move || form.birth_year.with(|text| {
                    (!text.trim().is_empty() && birth_year(text, today).is_none())
                        .then_some(Key::BirthYearInvalid)
                })
            />
            <label>
                {tr(lang, Key::RegisteredDay)}
                <input type="date" bind:value=form.registered />
            </label>
            <p class="hint">{tr(lang, Key::RegisteredDayHint)}</p>
            <FieldError
                lang
                tried
                error=move || form.registered.with(|text| {
                    (!text.trim().is_empty() && registered(text, today).is_none())
                        .then_some(Key::RegisteredDayInvalid)
                })
            />
            <Show when=move || form.registered.with(|registered| form.turnover_applied.with(|applied| asks_applied(registered, applied, today)))>
                <label>
                    {tr(lang, Key::TurnoverApplied)}
                    <input type="date" bind:value=form.turnover_applied />
                </label>
                <p class="hint">{tr(lang, Key::TurnoverAppliedHint)}</p>
                <FieldError
                    lang
                    tried
                    error=move || form.turnover_applied.with(|text| {
                        (!text.trim().is_empty() && past_day(text, today).is_none())
                            .then_some(Key::TurnoverAppliedInvalid)
                    })
                />
            </Show>
            <Show when=pension_asked>
                <fieldset class="group">
                    <legend>{tr(lang, Key::PensionJoined)}</legend>
                    <Choice lang name="pension" label=Key::Yes value=true chosen=form.pension_joined />
                    <Choice lang name="pension" label=Key::No value=false chosen=form.pension_joined />
                </fieldset>
            </Show>
            <Show when=pension_day_asked>
                <label>
                    {tr(lang, Key::PensionApplied)}
                    <input type="date" bind:value=form.pension_applied />
                </label>
                <p class="hint">{tr(lang, Key::PensionAppliedHint)}</p>
                <FieldError
                    lang
                    tried
                    error=move || form.pension_applied.with(|text| {
                        (!text.trim().is_empty() && past_day(text, today).is_none())
                            .then_some(Key::PensionAppliedInvalid)
                    })
                />
            </Show>
            <fieldset class="group">
                <legend>{tr(lang, Key::ProfileResidency)}</legend>
                <Choice lang name="residency" label=Key::ResidencyCitizen value=Residency::Citizen chosen=form.residency />
                <Choice
                    lang
                    name="residency"
                    label=Key::ResidencyForeignWithStatus
                    value=Residency::ForeignWithStatus
                    chosen=form.residency
                />
                <Choice
                    lang
                    name="residency"
                    label=Key::ResidencyForeignWithoutStatus
                    value=Residency::ForeignWithoutStatus
                    chosen=form.residency
                />
            </fieldset>
            <Show when=resident_asked>
                <fieldset class="group">
                    <legend>{tr(lang, Key::ResidentQuestion)}</legend>
                    <p class="hint">
                        {move || fill(t(lang.get(), Key::ResidentHint), &[&state_pays_from.to_string()])}
                    </p>
                    <Choice lang name="resident" label=Key::Yes value=true chosen=form.resident />
                    <Choice lang name="resident" label=Key::No value=false chosen=form.resident />
                </fieldset>
            </Show>
            <fieldset class="group">
                <legend>{tr(lang, Key::ProfileStops)}</legend>
                <p class="hint">{tr(lang, Key::ProfileStopsHint)}</p>
                <For each=move || form.stops.get() key=|row| row.key let(row)>
                    <div class="row">
                        <label>
                            {tr(lang, Key::StopFrom)}
                            <input type="date" bind:value=row.from />
                        </label>
                        <label>
                            {tr(lang, Key::StopUntil)}
                            <input type="date" bind:value=row.until />
                        </label>
                        <FieldError
                            lang
                            tried
                            error=move || {
                                let (from, until) = (row.from.get(), row.until.get());
                                let blank = from.is_empty() && until.is_empty();
                                (!blank && stop(&from, &until).is_none()).then_some(Key::StopInvalid)
                            }
                        />
                        <button
                            type="button"
                            class="secondary"
                            on:click=move |_| form.stops.update(|rows| rows.retain(|r| r.key != row.key))
                        >
                            <span class="label">{tr(lang, Key::Remove)}</span>
                        </button>
                    </div>
                </For>
                <button
                    type="button"
                    class="secondary"
                    disabled=full
                    on:click=move |_| {
                        let row = form.stop_row();
                        form.stops.update(|rows| rows.push(row));
                    }
                >
                    <span class="label">{tr(lang, Key::StopAdd)}</span>
                </button>
            </fieldset>
        </fieldset>
    }
}
