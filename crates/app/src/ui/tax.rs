//! Налог с оборота (задача 19): карточка на главном экране и экран «Налог»
//! (дизайн «Kvartik Design», экраны 02 и 10).
//!
//! Считает ядро (`kvartik_core::tax::turnover`) — по книге, профилю
//! и снимкам закрытых кварталов; здесь — показ, закрытие квартала и снятие
//! закрытия. Каждое из двух действий — команда ядра (`Closing::close`,
//! `reopen`) и одна транзакция хранилища (CLAUDE.md, «Данные меняются одним
//! путём»).
//!
//! Карточка и экран — частью wasm книги (`ledger::book_part`): расчёт
//! и разбор файла правил в главном wasm стоили рабочему экрану на телефоне
//! 0,27 с (2,72 → 2,99 с, замер 0.0.47 — docs/measurements.md). В главном
//! — только оболочки: карточка грузит часть, когда в базе есть данные,
//! экран — с первого показа (`split::screen`), а книгу и снимки закрытых
//! кварталов читает сама часть (`Sources::watch`).
//!
//! Число налога — расчёт, а не пример, но расчёт по прочтению закона:
//! экран говорит «сверьте с бухгалтером» и называет всё, чего расчёту не
//! хватило (Правило 6).

use std::sync::Arc;

use kvartik_core::i18n::{self, Key, Lang, fill, t};
use kvartik_core::model::activity::IncomeKind;
use kvartik_core::model::date::{Date, Quarter};
use kvartik_core::model::money::{Currency, Money};
use kvartik_core::model::record::RecordId;
use kvartik_core::tax::annual::{self, Annual};
use kvartik_core::tax::closing::Closing;
use kvartik_core::tax::deadlines;
use kvartik_core::tax::rules::{self, Percent};
use kvartik_core::tax::turnover::{self, Books, Leaving, Line, QuarterTax, Regime};
use leptos::prelude::*;

use super::annual::{AnnualCard, AnnualEntry, AnnualPage};
use super::deadlines::{DeadlineReminder, DeadlinesEntry, DeadlinesPage, shift_note};
use super::backup::{BackupChip, Backups};
use super::documents::{Documents, SettlementReminder};
use super::icon::{Icon, icon};
use super::ledger::{Operations, tax_part};
use super::profile::Profiles;
use super::route::{Router, Screen};
use super::split::screen;
use super::storage::error_key;
use super::{Base, tr};
use crate::platform::clock;
use crate::store::{Store, StoreError, Writes};

/// Квартал, когда часы без даты.
const FALLBACK_QUARTER: Quarter = match Quarter::new(2026, 1) {
    Some(quarter) => quarter,
    None => panic!("первый квартал 2026 года существует"),
};

/// Закрытые кварталы — снимки (`tax::closing`), одни на окно.
#[derive(Clone, Copy)]
pub struct Closings {
    state: RwSignal<State>,
    /// Следит ли уже кто-то за базой (`watch`).
    watched: StoredValue<bool>,
}

/// Где снимки.
#[derive(Clone, PartialEq, Debug)]
enum State {
    Loading,
    /// Прочитаны — все, включая открытые снова, — и сколько не читается.
    Ready {
        closings: Arc<Vec<Closing>>,
        unreadable: usize,
    },
    Failed(StoreError),
}

impl Closings {
    pub fn new() -> Self {
        Self {
            state: RwSignal::new(State::Loading),
            watched: StoredValue::new(false),
        }
    }

    /// Читать снимки, когда база открылась и когда её заменило
    /// восстановление из копии. Зовут карточка и экран налога, а следит тот,
    /// кто позвал первым: он живёт до конца окна.
    fn watch(&self, base: Base) {
        if self.watched.get_value() {
            return;
        }
        self.watched.set_value(true);
        let closings = *self;
        Effect::watch(
            move || (base.is_open(), base.replacements()),
            move |(open, _), _, _| {
                if !*open {
                    return;
                }
                if let Some(store) = base.get() {
                    leptos::task::spawn_local(async move { closings.load(&store).await });
                }
            },
            true,
        );
    }

    /// Прочитать снимки.
    async fn load(&self, store: &Store) {
        let read = async { store.read()?.list::<Closing>().await };
        self.state.set(match read.await {
            Ok(listed) => State::Ready {
                closings: Arc::new(listed.records),
                unreadable: listed.unreadable,
            },
            Err(error) => State::Failed(error),
        });
    }

    /// Записанный снимок — на место прежнего или в конец списка.
    fn put(&self, closing: Closing) {
        self.state.update(|state| {
            if let State::Ready { closings, .. } = state {
                let list = Arc::make_mut(closings);
                match list.iter_mut().find(|known| known.id() == closing.id()) {
                    Some(known) => *known = closing,
                    None => list.push(closing),
                }
            }
        });
    }

    /// Почему снимки не прочитались — отслеживаемое.
    fn failure(&self) -> Option<StoreError> {
        self.state.with(|state| match state {
            State::Failed(error) => Some(*error),
            State::Loading | State::Ready { .. } => None,
        })
    }

    /// Что-то, посчитанное по снимкам; пока их нет — значение по умолчанию.
    /// Отслеживаемое.
    fn with<T: Default>(&self, read: impl FnOnce(&[Closing], usize) -> T) -> T {
        self.state.with(|state| match state {
            State::Ready { closings, unreadable } => read(closings, *unreadable),
            State::Loading | State::Failed(_) => T::default(),
        })
    }
}

/// Откуда окно берёт расчёт: документы, книга, профиль и снимки.
#[derive(Clone, Copy)]
pub struct Sources {
    pub documents: Documents,
    pub operations: Operations,
    pub profiles: Profiles,
    pub closings: Closings,
}

impl Sources {
    /// Книгу и снимки — читать (`Operations::watch`, `Closings::watch`).
    pub(super) fn watch(&self, base: Base) {
        self.operations.watch(base);
        self.closings.watch(base);
    }

    /// Почему расчёта не будет: документы, профиль, книга или снимки не
    /// прочитались. Отслеживаемое.
    pub(super) fn failure(&self) -> Option<StoreError> {
        self.documents
            .failure()
            .or_else(|| self.profiles.failure())
            .or_else(|| self.operations.failure())
            .or_else(|| self.closings.failure())
    }

    /// Расчёт по всей книге — отслеживаемое; `None` — что-то ещё читается
    /// или не прочиталось: неполные данные дали бы неверное число.
    pub(super) fn with_books<T>(&self, read: impl FnOnce(Books<'_>) -> T) -> Option<T> {
        let Self {
            documents,
            operations,
            profiles,
            closings,
        } = *self;
        documents.with(|documents, _, documents_unreadable| {
            operations.with(|operations, operations_unreadable| {
                profiles.with(|profile| {
                    closings.with(|closings, _| {
                        Some(read(Books {
                            documents,
                            operations,
                            profile,
                            closings,
                            unreadable: documents_unreadable + operations_unreadable,
                        }))
                    })
                })
            })
        })
    }

    /// Налог квартала — отслеживаемое; `None` — как у [`Self::with_books`].
    fn quarter(&self, quarter: Quarter) -> Option<QuarterTax> {
        self.with_books(|books| turnover::quarter(books, quarter))
    }

    /// Снимок квартала, если он закрыт. Отслеживаемое.
    fn closing(&self, quarter: Quarter) -> Option<Closing> {
        self.closings.with(|closings, _| Closing::current(closings, quarter).cloned())
    }

    /// Годовые платежи года — отслеживаемое; `None` — как у
    /// [`Self::with_books`].
    pub(super) fn annual(&self, year: u16) -> Option<Annual> {
        self.with_books(|books| annual::year(books, year))
    }
}

/// Слова строки налога.
pub(super) const fn kind_key(kind: IncomeKind) -> Key {
    match kind {
        IncomeKind::Trade => Key::KindTrade,
        IncomeKind::Recyclables => Key::KindRecyclables,
        IncomeKind::Newspapers => Key::KindNewspapers,
        IncomeKind::Production => Key::KindProduction,
        IncomeKind::Rent => Key::KindRent,
        IncomeKind::Catering => Key::KindCatering,
        IncomeKind::CateringOther => Key::KindCateringOther,
        IncomeKind::HighTech => Key::KindHighTech,
        IncomeKind::Assets => Key::KindAssets,
        IncomeKind::Other => Key::KindOther,
    }
}

/// Код строки налога для `value` у `<option>` — как в данных.
pub(super) const fn kind_code(kind: IncomeKind) -> &'static str {
    match kind {
        IncomeKind::Trade => "trade",
        IncomeKind::Recyclables => "recyclables",
        IncomeKind::Newspapers => "newspapers",
        IncomeKind::Production => "production",
        IncomeKind::Rent => "rent",
        IncomeKind::Catering => "catering",
        IncomeKind::CateringOther => "catering_other",
        IncomeKind::HighTech => "high_tech",
        IncomeKind::Assets => "assets",
        IncomeKind::Other => "other",
    }
}

/// Строка налога со ставкой на день `on` — для списка выбора: «Прочая
/// деятельность — 10 %». Правила на этот день нет — только слова.
pub(super) fn kind_label(lang: Lang, kind: IncomeKind, on: Option<Date>) -> String {
    let name = t(lang, kind_key(kind));
    match on.and_then(|on| turnover::rate(kind, on)) {
        Some(rule) => fill(t(lang, Key::KindWithRate), &[name, &percent(lang, rule.rate)]),
        None => name.to_owned(),
    }
}

/// Процент по правилам языка.
fn percent(lang: Lang, value: Percent) -> String {
    i18n::percent(lang, i64::from(value.hundredths()))
}

/// Сумма налога — драмы.
pub(super) fn drams(lang: Lang, drams: i64) -> String {
    i18n::money(lang, Money::new(drams, Currency::Amd))
}

/// Есть ли о квартале что сказать: налог, пропуск или уход с режима.
fn has_news(tax: &QuarterTax) -> bool {
    !tax.lines.is_empty() || !tax.gaps.is_empty() || tax.regime != Regime::Turnover
}

/// Какой квартал показать сразу и к уплате ли он: прошлый — пока он не
/// закрыт и о нём есть что сказать (его расчёт подают в этом месяце),
/// иначе — текущий. `None` — данные читаются или сказать нечего.
fn shown_quarter(sources: Sources, today: Date) -> Option<(QuarterTax, bool)> {
    let current = today.quarter();
    if let Some(previous) = current.previous()
        && sources.closing(previous).is_none()
    {
        let tax = sources.quarter(previous)?;
        if has_news(&tax) {
            return Some((tax, true));
        }
    }
    let tax = sources.quarter(current)?;
    has_news(&tax).then_some((tax, false))
}

/// Что сказать под суммой на карточке.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Note {
    /// С этого дня — общий режим.
    Left(Date),
    /// Посчитано не всё.
    Partial,
    /// Квартал к уплате: подали расчёт — закройте.
    Close,
}

impl Note {
    fn of(tax: &QuarterTax, due: bool) -> Option<Self> {
        if let Regime::Left { from, .. } = tax.regime {
            return Some(Self::Left(from));
        }
        if !tax.complete() {
            return Some(Self::Partial);
        }
        due.then_some(Self::Close)
    }

    fn text(self, lang: Lang) -> String {
        match self {
            Self::Left(from) => fill(t(lang, Key::TaxLeftShort), &[&i18n::date(lang, from)]),
            Self::Partial => t(lang, Key::TaxPartial).to_owned(),
            Self::Close => t(lang, Key::TaxCloseReminder).to_owned(),
        }
    }
}

/// Карточки налога квартала (дизайн «Kvartik Design», экран 02) и годовых
/// платежей (`ui/annual.rs`) на главном экране и напоминания о сроках
/// и о расчётных документах ՊԵԿ (`ui/deadlines.rs`, `ui/documents.rs`) —
/// из части книги, где считается налог и разобран файл правил. Часть грузится, когда в базе есть данные: на пустой
/// карточке сказать нечего, и первый заход её не качает. Пока часть
/// грузится или если не загрузилась — карточек нет: о сбое скажет экран
/// налога, а налог открывается и вкладкой.
#[component]
pub fn TaxCard(ctx: Ctx) -> impl IntoView {
    let wanted = Memo::new(move |_| ctx.backups.has_data());
    move || {
        wanted
            .get()
            .then(|| Suspend::new(async move { tax_part(ctx, Part::Card).await.ok() }))
    }
}

/// Карточка: квартал, налог к уплате или пока набежавший — и что не так.
/// Вся карточка — ссылка на экран налога. Пока данные читаются или сказать
/// нечего — карточки нет: ноль без дохода выглядел бы расчётом.
#[component]
fn Card(ctx: Ctx) -> impl IntoView {
    let Ctx {
        lang,
        sources,
        base,
        today,
        ..
    } = ctx;
    sources.watch(base);
    let shown = Memo::new(move |_| today.and_then(|today| shown_quarter(sources, today)));
    move || {
        shown.get().map(|(tax, due)| {
            let quarter = tax.quarter;
            let sum = tax.tax;
            let note = Note::of(&tax, due);
            let label = if due { Key::TaxToPay } else { Key::TaxSoFar };
            // Квартал к уплате — с последним днём, перенесённым с нерабочего
            // (задача 21), если расчёт за него подают.
            let deadline = due
                .then(|| sources.with_books(|books| deadlines::quarter_deadline(books, quarter)))
                .flatten()
                .flatten();
            view! {
                <a class="hero-card" href=Screen::Tax.href()>
                    <span class="hero-meta">{move || i18n::quarter(lang.get(), quarter)}</span>
                    <span class="hero-label">{move || {
                        let lang = lang.get();
                        match deadline {
                            Some(deadline) => fill(t(lang, Key::TaxToPayBy), &[&i18n::date(lang, deadline.day)]),
                            None => t(lang, label).to_owned(),
                        }
                    }}</span>
                    <span class="hero-sum">{move || drams(lang.get(), sum)}</span>
                    {deadline.map(|deadline| move || shift_note(lang.get(), deadline.by_law).map(|note| view! {
                        <span class="hero-note">{note}</span>
                    }))}
                    {note.map(|note| view! { <span class="hero-note">{move || note.text(lang.get())}</span> })}
                </a>
            }
        })
    }
}

/// Что нужно карточке и экранам налога и годовых платежей из окна — одно
/// на окно.
#[derive(Clone, Copy)]
pub struct Ctx {
    pub(super) lang: ReadSignal<Lang>,
    pub(super) sources: Sources,
    pub(super) base: Base,
    pub(super) backups: Backups,
    /// Сегодня — один раз (Архитектура, «Время — параметр»).
    pub(super) today: Option<Date>,
}

impl Ctx {
    pub fn new(lang: ReadSignal<Lang>, sources: Sources, base: Base, backups: Backups) -> Self {
        Self {
            lang,
            sources,
            base,
            backups,
            today: clock::today(),
        }
    }
}

/// Экран налога на языке окна: заголовок — сразу, остальное — частью wasm
/// с первого показа (`split::screen`).
#[component]
pub fn TaxScreen(ctx: Ctx, router: Router) -> impl IntoView {
    let Ctx { lang, backups, .. } = ctx;
    view! {
        <h1 tabindex="-1">{tr(lang, Key::TaxTitle)}</h1>
        <BackupChip lang backups nudge=true />
        {screen(lang, router, Screen::Tax, move || tax_part(ctx, Part::Screen))}
    }
}

/// Что рисует налог в части книги.
#[derive(Clone, Copy)]
pub(super) enum Part {
    /// Карточки главного экрана: напоминания о сроках и о расчётных
    /// документах, налог квартала и годовые платежи.
    Card,
    /// Содержимое экрана налога.
    Screen,
    /// Содержимое экрана годовых платежей (`ui/annual.rs`, задача 20).
    Annual,
    /// Содержимое экрана сроков (`ui/deadlines.rs`, задача 21).
    Deadlines,
}

/// Карточки или экран — зовёт часть книги (`ledger::book_part`). Годовые
/// платежи — в той же части: они считаются из книги и профиля, а своя
/// часть делила бы с налогом почти весь код (CLAUDE.md, Правило 6).
pub(super) fn content(ctx: Ctx, part: Part) -> AnyView {
    match part {
        Part::Card => view! {
            <DeadlineReminder ctx />
            <SettlementReminder lang=ctx.lang documents=ctx.sources.documents today=ctx.today />
            <Card ctx />
            <AnnualCard ctx />
        }
        .into_any(),
        Part::Screen => view! { <TaxBody ctx /> }.into_any(),
        Part::Annual => view! { <AnnualPage ctx /> }.into_any(),
        Part::Deadlines => view! { <DeadlinesPage ctx /> }.into_any(),
    }
}

/// Закрыть квартал — записать снимок одной транзакцией.
async fn close(store: &Store, closing: &Closing) -> Result<(), StoreError> {
    let tx = store.write()?;
    let mut writes = Writes::default();
    writes.insert(closing);
    tx.commit(writes).await
}

/// Открыть квартал снова — снимок, прочитанный в той же транзакции,
/// помечается. `None` — его уже открыли или стёрли в другой вкладке.
async fn reopen(store: &Store, id: RecordId) -> Result<Option<Closing>, StoreError> {
    let tx = store.write()?;
    let Some(closing) = tx.get::<Closing>(id).await? else {
        return Ok(None);
    };
    let Some(reopened) = closing.reopen(clock::now()) else {
        return Ok(None);
    };
    let mut writes = Writes::default();
    writes.update(&reopened);
    tx.commit(writes).await?;
    Ok(Some(reopened))
}

/// Содержимое экрана: квартал стрелками, налог карточкой, что не так,
/// доход по месяцам, расчёт по строкам формы и закрытие квартала.
#[component]
fn TaxBody(ctx: Ctx) -> impl IntoView {
    let lang = ctx.lang;
    let sources = ctx.sources;
    sources.watch(ctx.base);
    let chosen = RwSignal::new(None::<Quarter>);
    // Квартал сразу — тот же, что на карточке главного экрана, и выбирается
    // один раз, когда данные прочитаны: закрыли его — экран остаётся на нём,
    // а не уходит к следующему вслед за карточкой.
    Effect::new(move |_| {
        if chosen.get_untracked().is_none()
            && let Some((tax, _)) = ctx.today.and_then(|today| shown_quarter(sources, today))
        {
            chosen.set(Some(tax.quarter));
        }
    });
    let quarter = Memo::new(move |_| {
        chosen
            .get()
            .unwrap_or_else(|| ctx.today.map_or(FALLBACK_QUARTER, Date::quarter))
    });
    let tax = Memo::new(move |_| sources.quarter(quarter.get()));
    let closing = Memo::new(move |_| sources.closing(quarter.get()));
    // Срок расчёта и налога квартала — пока он не закрыт (задача 21).
    let deadline = Memo::new(move |_| {
        sources
            .with_books(|books| deadlines::quarter_deadline(books, quarter.get()))
            .flatten()
    });
    let unreadable = Memo::new(move |_| sources.closings.with(|_, unreadable| unreadable));
    let busy = RwSignal::new(false);
    let notice = RwSignal::new(None::<Key>);
    let failure = RwSignal::new(None::<StoreError>);
    let step = move |next: bool| {
        let now = quarter.get_untracked();
        let moved = if next { now.next() } else { now.previous() };
        if let Some(moved) = moved {
            chosen.set(Some(moved));
            notice.set(None);
            failure.set(None);
        }
    };
    let close_quarter = move |_| {
        if busy.get_untracked() {
            return;
        }
        let (Some(current), Some(today)) = (tax.get_untracked(), ctx.today) else {
            return;
        };
        let Ok(snapshot) = Closing::close(&current, today, clock::now()) else {
            return;
        };
        let Some(store) = ctx.base.get() else {
            failure.set(Some(StoreError::Failed));
            return;
        };
        busy.set(true);
        failure.set(None);
        leptos::task::spawn_local(async move {
            let outcome = close(&store, &snapshot).await;
            busy.set(false);
            match outcome {
                Ok(()) => {
                    sources.closings.put(snapshot);
                    ctx.backups.data_saved();
                    notice.set(Some(Key::TaxClosedNotice));
                }
                Err(error) => failure.set(Some(error)),
            }
        });
    };
    let reopen_quarter = move |_| {
        if busy.get_untracked() {
            return;
        }
        let Some(id) = closing.with_untracked(|closing| closing.as_ref().map(Closing::id)) else {
            return;
        };
        let Some(store) = ctx.base.get() else {
            failure.set(Some(StoreError::Failed));
            return;
        };
        busy.set(true);
        failure.set(None);
        leptos::task::spawn_local(async move {
            let outcome = reopen(&store, id).await;
            busy.set(false);
            match outcome {
                Ok(Some(reopened)) => {
                    sources.closings.put(reopened);
                    notice.set(Some(Key::TaxReopenedNotice));
                }
                // Снимок поменяла другая вкладка: окно показывает, что в базе.
                Ok(None) => sources.closings.load(&store).await,
                Err(error) => failure.set(Some(error)),
            }
        });
    };
    // Квартал кончился, но не закрыт: закрыть — или сказать, чего не хватает.
    let ended = move || ctx.today.is_some_and(|today| quarter.get().last_day() < today);
    // Версия файла правил — его, а не расчёта: пока данные читаются, расчёта
    // нет.
    let rules_version = rules::armenia().version().to_string();

    view! {
        <div class="quarter-bar">
            <button
                type="button"
                class="round"
                aria-label=move || t(lang.get(), Key::LedgerPrevQuarter)
                on:click=move |_| step(false)
            >
                {icon(Icon::Back)}
            </button>
            <p class="quarter-name" aria-live="polite">{move || i18n::quarter(lang.get(), quarter.get())}</p>
            <button
                type="button"
                class="round"
                aria-label=move || t(lang.get(), Key::LedgerNextQuarter)
                on:click=move |_| step(true)
            >
                {icon(Icon::Forward)}
            </button>
        </div>
        {move || notice.get().map(|key| view! { <p role="status">{tr(lang, key)}</p> })}
        {move || failure.get().map(|error| view! { <p role="alert">{tr(lang, error_key(error))}</p> })}
        {move || sources.failure().map(|error| view! { <p role="alert">{tr(lang, error_key(error))}</p> })}
        {move || (unreadable.get() > 0).then(|| view! {
            <p role="alert">{move || {
                let lang = lang.get();
                fill(t(lang, Key::TaxUnreadable), &[&i18n::integer(lang, unreadable.get() as i64)])
            }}</p>
        })}
        {move || closing.get().map(|closed| {
            let closed_rules = closed.data().rules;
            let version = closed_rules.to_string();
            let before = closed.data().tax;
            view! {
                <div class="notice-row tax-closed">
                    <p role="status">{move || fill(t(lang.get(), Key::TaxClosed), &[&version])}</p>
                    <button type="button" class="secondary" disabled=move || busy.get() on:click=reopen_quarter>
                        {tr(lang, Key::TaxReopen)}
                    </button>
                </div>
                {move || {
                    let (now, rules) = tax.with(|tax| {
                        tax.as_ref().filter(|tax| tax.differs_from(&closed)).map(|tax| (tax.tax, tax.rules))
                    })?;
                    // Поменялся файл правил — часть расхождения от него, а не
                    // от данных.
                    let versions = (rules != closed_rules).then(|| (closed_rules.to_string(), rules.to_string()));
                    Some(view! {
                        <p role="alert">{move || {
                            let lang = lang.get();
                            fill(t(lang, Key::TaxChangedAfterClose), &[&drams(lang, now), &drams(lang, before)])
                        }}</p>
                        {versions.map(|(then, now)| view! {
                            <p class="hint">{move || fill(t(lang.get(), Key::TaxClosedRulesChanged), &[&then, &now])}</p>
                        })}
                    })
                }}
            }
        })}
        {move || closing.with(Option::is_none).then(|| deadline.get()).flatten().map(|deadline| view! {
            <div class="tax-deadline">
                <p>{move || fill(t(lang.get(), Key::TaxDeadline), &[&i18n::date(lang.get(), deadline.day)])}</p>
                {move || shift_note(lang.get(), deadline.by_law).map(|note| view! { <p class="hint">{note}</p> })}
            </div>
        })}
        {move || tax.get().map(|tax| view! { <Quarterly lang tax /> })}
        {move || (ended() && closing.with(Option::is_none) && tax.with(Option::is_some)).then(|| {
            let closable = move || tax.with(|tax| tax.as_ref().zip(ctx.today).is_some_and(|(tax, today)| tax.closable(today)));
            view! {
                <div class="tax-close">
                    {move || if closable() {
                        view! {
                            <button type="button" disabled=move || busy.get() on:click=close_quarter>
                                {icon(Icon::Check)}
                                <span>{tr(lang, Key::TaxClose)}</span>
                            </button>
                            <p class="hint">{tr(lang, Key::TaxCloseHint)}</p>
                        }
                        .into_any()
                    } else {
                        let left = tax.with(|tax| tax.as_ref().is_some_and(|tax| tax.regime != Regime::Turnover));
                        let key = if left { Key::TaxCannotCloseLeft } else { Key::TaxCannotClose };
                        view! { <p class="hint">{tr(lang, key)}</p> }.into_any()
                    }}
                </div>
            }
        })}
        <AnnualEntry ctx />
        <DeadlinesEntry ctx />
        <p class="tax-check">{tr(lang, Key::TaxCheck)}</p>
        <p class="hint">{move || fill(t(lang.get(), Key::TaxRulesVersion), &[&rules_version])}</p>
    }
}

/// Налог квартала: карточка, что не так, доход по месяцам и расчёт по
/// строкам формы.
#[component]
fn Quarterly(lang: ReadSignal<Lang>, tax: QuarterTax) -> impl IntoView {
    let sum = tax.tax;
    // Строк нет, а доход, пропуск или уход с режима есть — «дохода нет» и
    // ноль были бы неправдой (Правило 6).
    let nothing = tax.complete() && tax.regime == Regime::Turnover && tax.months.iter().all(|month| month.get() == 0);
    let subtitle = match tax.lines.as_slice() {
        [] if nothing => Subtitle::NoIncome,
        [] => Subtitle::NotCounted,
        [line] => Subtitle::One(line.income, line.rate),
        _ => Subtitle::Several,
    };
    let alerts = alerts(&tax);
    let hints = hints(&tax);
    let months: Vec<(u16, u8, i64)> = tax
        .months
        .iter()
        .enumerate()
        .map(|(index, lumas)| {
            let month = tax.quarter.first_day().month() + index as u8;
            (tax.quarter.year(), month, lumas.drams())
        })
        .collect();
    let this_year = tax.quarter.year();
    let lines = tax.lines.clone();
    view! {
        <div class="hero-card">
            <span class="hero-label">{tr(lang, Key::TaxTurnover)}</span>
            {(subtitle != Subtitle::NotCounted).then(|| view! {
                <span class="hero-sum">{move || drams(lang.get(), sum)}</span>
            })}
            <span class="hero-note">{move || subtitle.text(lang.get())}</span>
        </div>
        {alerts.into_iter().map(|alert| view! {
            <p role="alert">{move || alert.text(lang.get())}</p>
        }).collect_view()}
        {hints.into_iter().map(|hint| view! {
            <p class="hint">{move || hint.text(lang.get())}</p>
        }).collect_view()}
        <h2>{tr(lang, Key::TaxByMonth)}</h2>
        <dl class="pairs months">
            {months.into_iter().map(|(year, month, income)| view! {
                <div>
                    <dt>{move || i18n::month_heading(lang.get(), year, month, this_year)}</dt>
                    <dd>{move || drams(lang.get(), income)}</dd>
                </div>
            }).collect_view()}
        </dl>
        {(!lines.is_empty()).then(|| view! {
            <details class="tax-how">
                <summary>{tr(lang, Key::TaxHow)}</summary>
                {lines.into_iter().map(|line| view! { <LineCard lang line /> }).collect_view()}
            </details>
        })}
    }
}

/// Строка расчёта по полям формы ՊԵԿ (приказ 299-Ն): доход, ставка, налог
/// по ставке, у строк с вычетом — расходы, вычет, перенос и предел.
#[component]
fn LineCard(lang: ReadSignal<Lang>, line: Line) -> impl IntoView {
    let Line {
        kind,
        income,
        rate,
        gross,
        deduction,
        tax,
    } = line;
    let pair = move |label: String, value: String| {
        view! {
            <div>
                <dt>{label}</dt>
                <dd>{value}</dd>
            </div>
        }
    };
    view! {
        <section class="tax-line">
            <h3>{move || t(lang.get(), kind_key(kind))}</h3>
            <p class="hint">{move || fill(t(lang.get(), Key::TaxRow), &[&kind.row().to_string()])}</p>
            {move || {
                let lang = lang.get();
                let mut rows = vec![
                    pair(t(lang, Key::LedgerIncome).to_owned(), drams(lang, income)),
                    pair(t(lang, Key::TaxLineRate).to_owned(), percent(lang, rate)),
                    pair(t(lang, Key::TaxLineGross).to_owned(), drams(lang, gross)),
                ];
                if let Some(deduction) = deduction {
                    rows.push(pair(t(lang, Key::TaxLineExpenses).to_owned(), drams(lang, deduction.expenses)));
                    if deduction.shared > 0 && deduction.attributed > 0 {
                        rows.push(pair(t(lang, Key::TaxLineShared).to_owned(), drams(lang, deduction.shared)));
                    }
                    rows.push(pair(
                        fill(t(lang, Key::TaxLineDeduction), &[&percent(lang, deduction.percent)]),
                        drams(lang, deduction.current),
                    ));
                    rows.push(pair(t(lang, Key::TaxLineCarryIn).to_owned(), drams(lang, deduction.carry_in)));
                    rows.push(pair(
                        fill(t(lang, Key::TaxLineFloor), &[&percent(lang, deduction.floor_percent)]),
                        drams(lang, deduction.floor),
                    ));
                }
                rows.push(pair(t(lang, Key::TaxLineTax).to_owned(), drams(lang, tax)));
                if let Some(deduction) = deduction {
                    rows.push(pair(t(lang, Key::TaxLineCarryOut).to_owned(), drams(lang, deduction.carry_out)));
                }
                view! { <dl class="pairs">{rows}</dl> }
            }}
            {deduction.is_none().then(|| view! { <p class="hint">{tr(lang, Key::TaxLineNoDeduction)}</p> })}
        </section>
    }
}

/// Подпись под суммой налога на экране.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Subtitle {
    NoIncome,
    /// Строк нет, а доход или пропуск есть.
    NotCounted,
    /// Одна строка: доход и ставка, как в дизайне.
    One(i64, Percent),
    Several,
}

impl Subtitle {
    fn text(self, lang: Lang) -> String {
        match self {
            Self::NoIncome => t(lang, Key::TaxNoIncome).to_owned(),
            Self::NotCounted => t(lang, Key::TaxNotCounted).to_owned(),
            Self::One(income, rate) => fill(t(lang, Key::TaxIncomeTimesRate), &[&drams(lang, income), &percent(lang, rate)]),
            Self::Several => t(lang, Key::TaxSeveralLines).to_owned(),
        }
    }
}

/// Что окно обязано сказать о квартале — красным: число без этого неполно
/// или уже не всё (Правило 6).
#[derive(Clone, PartialEq, Eq, Debug)]
enum Alert {
    Left { from: Date, why: Leaving, threshold: Option<i64> },
    Unrated(usize),
    YearUnrated(usize),
    Unreadable(usize),
    Previous(Quarter),
    Unassigned(usize),
    NoCodes,
    Unallocated(usize),
    NoRule(IncomeKind),
    Excluded(String, Date),
}

impl Alert {
    fn text(&self, lang: Lang) -> String {
        let count = |count: usize| i18n::integer(lang, count as i64);
        match self {
            Self::Left { from, why, threshold } => {
                let limit = threshold.map(|limit| drams(lang, limit)).unwrap_or_default();
                match why {
                    Leaving::Threshold => fill(t(lang, Key::TaxLeftThreshold), &[&i18n::date(lang, *from), &limit]),
                    Leaving::Code(code) => {
                        fill(t(lang, Key::TaxLeftCode), &[&i18n::date(lang, *from), &code.to_string()])
                    }
                    Leaving::PreviousYear => fill(t(lang, Key::TaxLeftPrevious), &[&limit]),
                }
            }
            Self::Unrated(rows) => fill(t(lang, Key::TaxUnrated), &[&count(*rows)]),
            Self::YearUnrated(rows) => fill(t(lang, Key::TaxYearUnrated), &[&count(*rows)]),
            Self::Unreadable(records) => fill(t(lang, Key::TaxUnreadableRecords), &[&count(*records)]),
            Self::Previous(quarter) => fill(t(lang, Key::TaxPreviousIncomplete), &[&i18n::quarter(lang, *quarter)]),
            Self::Unassigned(rows) => fill(t(lang, Key::TaxUnassigned), &[&count(*rows)]),
            Self::NoCodes => t(lang, Key::TaxNoCodes).to_owned(),
            Self::Unallocated(rows) => fill(t(lang, Key::TaxUnallocated), &[&count(*rows)]),
            Self::NoRule(kind) => fill(t(lang, Key::TaxNoRule), &[t(lang, kind_key(*kind))]),
            Self::Excluded(code, from) => {
                fill(t(lang, Key::ActivityExcluded), &[code, &i18n::date(lang, *from)])
            }
        }
    }
}

fn alerts(tax: &QuarterTax) -> Vec<Alert> {
    let mut alerts = Vec::new();
    if let Regime::Left { from, why } = tax.regime {
        alerts.push(Alert::Left {
            from,
            why,
            threshold: tax.threshold,
        });
    }
    let gaps = &tax.gaps;
    if gaps.unrated > 0 {
        alerts.push(Alert::Unrated(gaps.unrated));
    }
    if gaps.year_unrated > 0 {
        alerts.push(Alert::YearUnrated(gaps.year_unrated));
    }
    if gaps.unreadable > 0 {
        alerts.push(Alert::Unreadable(gaps.unreadable));
    }
    if let Some(quarter) = gaps.previous {
        alerts.push(Alert::Previous(quarter));
    }
    if gaps.no_codes {
        alerts.push(Alert::NoCodes);
    } else if gaps.unassigned > 0 {
        alerts.push(Alert::Unassigned(gaps.unassigned));
    }
    if gaps.unallocated > 0 {
        alerts.push(Alert::Unallocated(gaps.unallocated));
    }
    alerts.extend(gaps.no_rule.iter().map(|kind| Alert::NoRule(*kind)));
    alerts.extend(tax.excluded_codes.iter().map(|(code, from)| Alert::Excluded(code.to_string(), *from)));
    alerts
}

/// Что окно говорит о квартале спокойно: вычет, которого не было, перенос
/// до программы, оборот года.
#[derive(Clone, PartialEq, Eq, Debug)]
enum Hint {
    CarryUnknown,
    NoDocument(usize),
    PaymentOnly(usize),
    Excluded(usize),
    Threshold { year: u16, turnover: i64, threshold: i64 },
}

impl Hint {
    fn text(&self, lang: Lang) -> String {
        let count = |count: usize| i18n::integer(lang, count as i64);
        match self {
            Self::CarryUnknown => t(lang, Key::TaxCarryUnknown).to_owned(),
            Self::NoDocument(rows) => fill(t(lang, Key::TaxRefusedNoDocument), &[&count(*rows)]),
            Self::PaymentOnly(rows) => fill(t(lang, Key::TaxRefusedPayment), &[&count(*rows)]),
            Self::Excluded(rows) => fill(t(lang, Key::TaxRefusedExcluded), &[&count(*rows)]),
            Self::Threshold { year, turnover, threshold } => {
                // Доля порога — целыми процентами, с отбросом: «99 %»
                // у 114,9 млн не обещает лишнего.
                let share = i128::from(*turnover) * 100 / i128::from((*threshold).max(1));
                let share = i64::try_from(share).unwrap_or(i64::MAX).saturating_mul(100);
                fill(
                    t(lang, Key::TaxThreshold),
                    &[&year.to_string(), &drams(lang, *turnover), &i18n::percent(lang, share), &drams(lang, *threshold)],
                )
            }
        }
    }
}

fn hints(tax: &QuarterTax) -> Vec<Hint> {
    let mut hints = Vec::new();
    if tax.carry_unknown {
        hints.push(Hint::CarryUnknown);
    }
    let refused = tax.refused;
    if refused.no_document > 0 {
        hints.push(Hint::NoDocument(refused.no_document));
    }
    if refused.payment_only > 0 {
        hints.push(Hint::PaymentOnly(refused.payment_only));
    }
    if refused.excluded > 0 {
        hints.push(Hint::Excluded(refused.excluded));
    }
    if let Some(threshold) = tax.threshold
        && tax.regime == Regime::Turnover
        && tax.year_turnover > 0
    {
        hints.push(Hint::Threshold {
            year: tax.quarter.year(),
            turnover: tax.year_turnover,
            threshold,
        });
    }
    hints
}
