//! Раздел «Счета и акты» (задача 11): список, черновик в форме, выпуск
//! с номером, оплата, аннулирование, акт из счёта и повторяющийся счёт.
//!
//! Раздел — содержимое экрана «Счета и акты» (`ui/route.rs`); счета,
//! которые ждут оплаты, показывает и главный экран. Каждое действие —
//! команда ядра
//! (`Document::create`, `edit`, `issue`, `annul`…, `Recurring::start`,
//! `stop`, `generate`) и одна транзакция хранилища (CLAUDE.md, «Данные
//! меняются одним путём»).
//!
//! Выпуск читает в той же транзакции все документы — номер следующий за
//! самым большим выданным, — профиль и клиента: копия их реквизитов
//! ложится в документ. Черновики повторяющихся счетов появляются при
//! открытии программы (`Documents::load`): сервера, который создал бы их
//! по расписанию, нет.
//!
//! Цены и количества человек вводит по правилам языка окна, и окно сразу
//! показывает, что поняло: сумму строки записью `i18n::money` (Правило 6,
//! «Ввод суммы не угадывается»).

use std::sync::Arc;

use kvartik_core::format::Stored;
use kvartik_core::i18n::{self, Key, Lang, ParseError, fill, t};
use kvartik_core::model::activity::ActivityCode;
use kvartik_core::model::client::Client;
use kvartik_core::model::country::Country;
use kvartik_core::model::date::Date;
use kvartik_core::model::document::{
    DocError, DocKind, Document, DocumentData, Item, Payment, QUANTITY_DIGITS, Status,
};
use kvartik_core::model::ledger;
use kvartik_core::model::money::{Currency, Money};
use kvartik_core::model::profile::Profile;
use kvartik_core::model::record::RecordId;
use kvartik_core::model::recurring::Recurring;
use kvartik_core::model::text::Line;
use kvartik_core::tax::turnover;
use leptos::ev::SubmitEvent;
use leptos::prelude::*;

use super::avatar::{Avatar, tone};
use super::backup::{BackupChip, Backups};
use super::clients::Clients;
use super::form::{CURRENCIES, Forms, TextField, WizardHead, currency_code, currency_key};
use super::icon::{Icon, icon};
use super::ledger::{IncomeActivity, equivalent, rate_problem};
use super::paper::{PaperPanel, Request};
use super::profile::Profiles;
use super::route::{Router, Screen};
use super::split::{part, screen, split};
use super::storage::error_key;
use super::{Base, tr};
use crate::platform::clock;
use crate::store::{Store, StoreError, Writes};

/// Сколько документов список показывает сразу и прибавляет по «Показать
/// ещё»: за год у ИП их сотни, а рисовать окну — только то, что видно
/// (Правило 1).
const PAGE: usize = 30;

/// Что окно знает о документах.
#[derive(Clone, Copy)]
pub struct Documents {
    state: RwSignal<State>,
    /// Сколько черновиков повторяющихся счетов создало последнее открытие.
    created: RwSignal<usize>,
    /// Что показывает раздел и сообщение о сделанном — здесь, а не в самом
    /// разделе: «Новый счёт» в меню и на главном экране открывает форму
    /// (`new_invoice`).
    mode: RwSignal<Mode>,
    notice: RwSignal<Option<Notice>>,
}

/// Где документы.
#[derive(Clone, PartialEq, Debug)]
enum State {
    /// База ещё открывается.
    Loading,
    /// Прочитаны: все документы и повторы, включая удалённые, и сколько
    /// записей не читается. В `Arc`: сигнал отдаёт состояние копией.
    Ready {
        documents: Arc<Vec<Document>>,
        recurrings: Arc<Vec<Recurring>>,
        unreadable: usize,
    },
    /// Не прочитаны.
    Failed(StoreError),
}

impl Documents {
    pub fn new() -> Self {
        Self {
            state: RwSignal::new(State::Loading),
            created: RwSignal::new(0),
            mode: RwSignal::new(Mode::List),
            notice: RwSignal::new(None),
        }
    }

    /// Открыть форму нового счёта — из меню и с главного экрана. Без живого
    /// клиента формы нет: раздел скажет, что сначала нужен клиент.
    pub fn new_invoice(&self, clients: Clients) {
        if clients.alive().is_empty() {
            return;
        }
        self.notice.set(None);
        self.created.set(0);
        self.mode.set(Mode::New(DocKind::Invoice));
    }


    /// Прочитать документы и повторы: когда база открылась и когда её
    /// заменило восстановление из копии. Повторам, которым к дню `today`
    /// пора, — создать черновики: транзакция записи открывается, только
    /// когда пора.
    pub async fn load(&self, store: &Store, today: Option<Date>) {
        let read = async {
            let tx = store.read()?;
            Ok::<_, StoreError>((tx.list::<Document>().await?, tx.list::<Recurring>().await?))
        };
        let (documents, recurrings) = match read.await {
            Ok(read) => read,
            Err(error) => {
                self.state.set(State::Failed(error));
                return;
            }
        };
        let due = today.filter(|today| {
            recurrings.records.iter().any(|recurring| {
                documents
                    .records
                    .iter()
                    .find(|document| document.id() == recurring.data().source)
                    .is_some_and(|source| recurring.due(source, *today))
            })
        });
        self.state.set(State::Ready {
            documents: Arc::new(documents.records),
            recurrings: Arc::new(recurrings.records),
            unreadable: documents.unreadable + recurrings.unreadable,
        });
        if let Some(today) = due {
            // Не вышло — черновики появятся при следующем открытии, а об
            // ошибке хранилище скажет на первом же действии.
            if let Ok((drafts, moved)) = create_due(store, today).await {
                self.created.set(drafts.len());
                for recurring in moved {
                    self.put_recurring(recurring);
                }
                for draft in drafts {
                    self.put(draft);
                }
            }
        }
    }

    /// Записанный документ — на место прежнего или в конец списка.
    fn put(&self, document: Document) {
        self.state.update(|state| {
            if let State::Ready { documents, .. } = state {
                let list = Arc::make_mut(documents);
                match list.iter_mut().find(|known| known.id() == document.id()) {
                    Some(known) => *known = document,
                    None => list.push(document),
                }
            }
        });
    }

    /// Записанный повтор — на место прежнего или в конец списка.
    fn put_recurring(&self, recurring: Recurring) {
        self.state.update(|state| {
            if let State::Ready { recurrings, .. } = state {
                let list = Arc::make_mut(recurrings);
                match list.iter_mut().find(|known| known.id() == recurring.id()) {
                    Some(known) => *known = recurring,
                    None => list.push(recurring),
                }
            }
        });
    }

    /// Документ по идентификатору — без слежения: форма открывается один
    /// раз.
    fn get(&self, id: RecordId) -> Option<Document> {
        self.state.with_untracked(|state| match state {
            State::Ready { documents, .. } => documents.iter().find(|d| d.id() == id).cloned(),
            State::Loading | State::Failed(_) => None,
        })
    }

    /// Что-то, посчитанное по прочитанным документам и повторам; пока их
    /// нет — значение по умолчанию. Отслеживаемое: книга (`ui/ledger.rs`)
    /// берёт из документов доход.
    pub(super) fn with<T: Default>(&self, read: impl FnOnce(&[Document], &[Recurring], usize) -> T) -> T {
        self.state.with(|state| match state {
            State::Ready {
                documents,
                recurrings,
                unreadable,
            } => read(documents, recurrings, *unreadable),
            State::Loading | State::Failed(_) => T::default(),
        })
    }

    /// Почему документы не прочитались — отслеживаемое.
    pub(super) fn failure(&self) -> Option<StoreError> {
        self.state.with(|state| match state {
            State::Failed(error) => Some(*error),
            State::Loading | State::Ready { .. } => None,
        })
    }

    /// Сколько живых документов у клиента — их число называет сообщение об
    /// удалении клиента: документы остаются.
    pub fn count_for_client(&self, client: RecordId) -> usize {
        self.state.with_untracked(|state| match state {
            State::Ready { documents, .. } => Document::count_for_client(documents, client),
            State::Loading | State::Failed(_) => 0,
        })
    }
}

/// Черновики повторов, которым к дню `today` пора, — одной транзакцией
/// с повторами, у которых сдвинут счётчик: иначе вторая вкладка или сбой
/// посреди записи создали бы черновик дважды.
async fn create_due(
    store: &Store,
    today: Date,
) -> Result<(Vec<Document>, Vec<Recurring>), StoreError> {
    let tx = store.write()?;
    let documents = tx.list::<Document>().await?.records;
    let recurrings = tx.list::<Recurring>().await?.records;
    let clients = tx.list::<Client>().await?.records;
    let now = clock::now();
    let mut writes = Writes::default();
    let (mut drafts, mut moved) = (Vec::new(), Vec::new());
    for recurring in &recurrings {
        let Some(source) = documents.iter().find(|d| d.id() == recurring.data().source) else {
            continue;
        };
        let client_alive = clients
            .iter()
            .any(|client| client.id() == source.data().client && !client.is_deleted());
        if let Some((recurring, new)) = recurring.generate(source, client_alive, today, now) {
            writes.update(&recurring);
            for draft in &new {
                writes.insert(draft);
            }
            moved.push(recurring);
            drafts.extend(new);
        }
    }
    tx.commit(writes).await?;
    Ok((drafts, moved))
}

/// Что показывает раздел: список или форму. Выпущенный документ — не
/// режим, а адрес экрана (`Screen::Document`): «Назад» на телефоне ведёт
/// от него к списку.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    List,
    /// Новый документ этого вида.
    New(DocKind),
    /// Акт по выпущенному счёту — форма с его данными.
    ActFor(RecordId),
    /// Черновик в форме.
    Edit(RecordId),
}

/// Сообщение о сделанном — над списком или над документом.
#[derive(Clone, PartialEq, Eq, Debug)]
enum Notice {
    Saved,
    /// Выпущен — с названием и номером.
    Issued(String),
    /// Аннулирован — с кнопкой «Вернуть» рядом.
    Annulled(RecordId, String),
    Unannulled(String),
    /// Черновик удалён — с кнопкой «Восстановить» рядом.
    Deleted(RecordId),
    Restored,
    Paid,
    /// Курс документа сохранён (задача 14).
    RateSaved,
    /// Вид деятельности документа сохранён (задача 19).
    ActivitySaved,
    /// Акт отмечен подписанным клиентом и отметка снята (задача 62).
    Signed,
    Unsigned,
}

/// Что не вышло: хранилище или команда ядра.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Problem {
    Store(StoreError),
    Doc(DocError),
}

impl From<StoreError> for Problem {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

impl From<DocError> for Problem {
    fn from(error: DocError) -> Self {
        Self::Doc(error)
    }
}

impl Problem {
    /// Слова для человека. Ошибки команды, до которых форма не пускает, —
    /// общими словами хранилища: это сбой, а не ошибка человека.
    const fn key(self) -> Key {
        match self {
            Self::Store(error) => error_key(error),
            Self::Doc(DocError::NoSellerName) => Key::IssueNoSellerName,
            Self::Doc(DocError::ClientDeleted) => Key::IssueClientDeleted,
            Self::Doc(DocError::NumbersUnknown) => Key::IssueNumbersUnknown,
            Self::Doc(DocError::PaymentAmount) => Key::PaymentInvalid,
            Self::Doc(DocError::SignedBeforeDate) => Key::SignBeforeDate,
            Self::Doc(DocError::TooLarge) => Key::DocTooLarge,
            Self::Doc(
                DocError::NoItems
                | DocError::TooManyItems
                | DocError::NoText
                | DocError::Quantity
                | DocError::Price
                | DocError::DueBeforeDate
                | DocError::WrongKind
                | DocError::Issued
                | DocError::NotIssued
                | DocError::Annulled
                | DocError::Deleted
                | DocError::NotInvoice
                | DocError::NotAct
                | DocError::TooManyPayments
                | DocError::WrongClient
                | DocError::NumbersExhausted
                | DocError::Rate,
            ) => Key::StoreFailed,
        }
    }
}

/// Стадия чтения — без самих документов: раздел перерисовывается целиком,
/// только когда она меняется, а не на каждую правку списка.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Phase {
    Loading,
    Ready,
    Failed(StoreError),
}

/// Общее у частей раздела.
#[derive(Clone, Copy)]
struct Ctx {
    lang: ReadSignal<Lang>,
    documents: Documents,
    clients: Clients,
    /// Профиль — его коды: вид деятельности дохода (задача 19).
    profiles: Profiles,
    base: Base,
    backups: Backups,
    mode: RwSignal<Mode>,
    notice: RwSignal<Option<Notice>>,
    /// Открытый экран: выпущенный документ — по адресу.
    router: Router,
    /// Открытые формы окна: форма документа отмечается в них, пока открыта.
    forms: Forms,
    /// Сегодня — один раз (Архитектура, «Время — параметр»).
    today: Option<Date>,
}

impl Ctx {
    /// Сегодня, а часы без даты — дата документа: срока они не просрочат.
    fn today_or(&self, date: Date) -> Date {
        self.today.unwrap_or(date)
    }
}

/// Экран «Счета и акты» на языке окна (дизайн «Kvartik Design»): список
/// или форма — частью wasm с первого показа (`split::screen`), а выпущенный
/// документ — карточкой, своей частью: на телефоне вместо списка, на
/// компьютере рядом с ним.
#[component]
pub fn DocumentsScreen(
    lang: ReadSignal<Lang>,
    documents: Documents,
    clients: Clients,
    profiles: Profiles,
    base: Base,
    backups: Backups,
    router: Router,
    forms: Forms,
) -> impl IntoView {
    let ctx = Ctx {
        lang,
        documents,
        clients,
        profiles,
        base,
        backups,
        mode: documents.mode,
        notice: documents.notice,
        router,
        forms,
        today: clock::today(),
    };
    let phase = Memo::new(move |_| {
        documents.state.with(|state| match state {
            State::Loading => Phase::Loading,
            State::Ready { .. } => Phase::Ready,
            State::Failed(error) => Phase::Failed(*error),
        })
    });
    // Копию восстановили — документы в базе могли стать другими. Здесь,
    // а не в части экрана: документы нужны и главному экрану, а экран мог
    // ещё ни разу не показываться.
    Effect::watch(
        move || base.replacements(),
        move |_, _, _| {
            if let Some(store) = base.get() {
                ctx.mode.set(Mode::List);
                ctx.notice.set(None);
                leptos::task::spawn_local(async move { documents.load(&store, ctx.today).await });
            }
        },
        false,
    );
    // Часть карточки не загрузилась — назад к списку.
    let close = move || router.go(Screen::Documents);
    // Открытый документ — из адреса; открытая форма его прячет.
    let opened = Memo::new(move |_| match router.screen() {
        Screen::Document(id) => Some(id),
        _ => None,
    });
    let listing = Memo::new(move |_| ctx.mode.get() == Mode::List && phase.get() == Phase::Ready);
    let detail = move || {
        let id = opened.get()?;
        listing.get().then(|| view! {
            <div class="docs-detail">{part(lang, opened_document(ctx, id), close)}</div>
        })
    };
    view! {
        <div class="docs" class:opened=move || opened.get().is_some() && listing.get()>
            <div class="docs-main">
                <h1 tabindex="-1">{tr(lang, Key::DocsTitle)}</h1>
                <BackupChip lang backups nudge=true />
                {screen(lang, router, Screen::Documents, move || documents_screen(ctx, phase))}
            </div>
            {detail}
        </div>
    }
}

split! {
    /// Список или форма экрана «Счета и акты» — своей частью wasm
    /// (`ui/split.rs`): первому заходу нужен только главный экран.
    pub(super) fn documents_screen(ctx: Ctx, phase: Memo<Phase>) -> AnyView, preload preload_screen {
        view! { <DocBody ctx phase /> }.into_any()
    }
}

/// Список или форма — форма в той же части: без экрана её не открыть,
/// а своя часть у неё умножала бы общие куски частей (`ui/split.rs`).
/// Пока база открывается — пусто: «открываем…» лишь мигнуло бы.
#[component]
fn DocBody(ctx: Ctx, phase: Memo<Phase>) -> impl IntoView {
    let lang = ctx.lang;
    move || {
        let body = match (phase.get(), ctx.mode.get()) {
            (Phase::Loading, _) => return None,
            (Phase::Failed(error), _) => view! {
                <p role="alert">{tr(lang, error_key(error))}</p>
            }
            .into_any(),
            (Phase::Ready, Mode::List) => view! { <DocList ctx /> }.into_any(),
            (Phase::Ready, Mode::New(kind)) => {
                let start = Start::Blank(kind, ctx.today);
                view! { <DocForm ctx existing=None start /> }.into_any()
            }
            (Phase::Ready, Mode::ActFor(id)) => {
                let start = ctx
                    .documents
                    .get(id)
                    .and_then(|invoice| invoice.act_data(ctx.today_or(invoice.data().date)).ok());
                match start {
                    Some(data) => view! { <DocForm ctx existing=None start=Start::Data(data) /> }.into_any(),
                    None => view! { <DocList ctx /> }.into_any(),
                }
            }
            (Phase::Ready, Mode::Edit(id)) => match ctx.documents.get(id) {
                Some(draft) => {
                    let start = Start::Data(draft.data().clone());
                    view! { <DocForm ctx existing=Some(draft) start /> }.into_any()
                }
                None => view! { <DocList ctx /> }.into_any(),
            },
        };
        Some(body)
    }
}

/// Слова вида документа.
pub(super) const fn kind_key(kind: DocKind) -> Key {
    match kind {
        DocKind::Invoice => Key::KindInvoice,
        DocKind::Act => Key::KindAct,
    }
}

/// Название документа на языке окна: «Счёт № 2026-001» или «Счёт,
/// черновик».
fn title(lang: Lang, document: &Document) -> String {
    let kind = t(lang, kind_key(document.data().kind));
    match document.number() {
        Some(number) => fill(t(lang, Key::DocNumbered), &[kind, &number.to_string()]),
        None => fill(t(lang, Key::DocDraftTitle), &[kind]),
    }
}

/// Название в строке списка: у черновика — только вид документа,
/// а «черновик» говорит его статус рядом.
fn row_title(lang: Lang, document: &Document) -> String {
    match document.number() {
        Some(_) => title(lang, document),
        None => t(lang, kind_key(document.data().kind)).to_owned(),
    }
}

/// Число в поле — записью нового языка окна, если прежний его читает:
/// «1,234» по-английски — тысяча двести тридцать четыре, а по-русски —
/// одна целая с тысячными (Правило 6, «Ввод суммы не угадывается»), и без
/// этого цена после смены языка молча стала бы другой или перестала бы
/// читаться. Не прочитанное прежним языком остаётся как есть: его ещё
/// набирают.
pub(super) fn retype<T>(
    field: RwSignal<String>,
    read: impl FnOnce(&str) -> Result<T, ParseError>,
    write: impl FnOnce(T) -> String,
) {
    let Ok(value) = field.with_untracked(|text| read(text)) else {
        return;
    };
    field.set(write(value));
}

/// Слова и класс оформления статуса: слово говорит, цвет помогает
/// (Правило 4).
const fn status_look(status: Status) -> (Key, &'static str) {
    match status {
        Status::Draft => (Key::StatusDraft, "status muted"),
        Status::Issued => (Key::StatusIssued, "status done"),
        Status::Signed => (Key::StatusSigned, "status done"),
        Status::Unpaid => (Key::StatusUnpaid, "status waiting"),
        Status::PartlyPaid => (Key::StatusPartlyPaid, "status waiting"),
        Status::Paid => (Key::StatusPaid, "status done"),
        Status::Overdue => (Key::StatusOverdue, "status late"),
        Status::Annulled => (Key::StatusAnnulled, "status muted"),
    }
}

/// Клиент документа на языке окна: у выпущенного — из копии реквизитов,
/// у черновика — из записи клиента, с пометкой, если он удалён.
fn client_label(lang: Lang, document: &Document, clients: Clients) -> String {
    if let Some(issued) = document.issued() {
        return issued.buyer().name.as_str().to_owned();
    }
    match clients.name(document.data().client) {
        Some((name, false)) => name,
        Some((name, true)) => fill(t(lang, Key::ClientGoneMark), &[&name]),
        None => String::new(),
    }
}

/// Сумма на языке окна; не посчиталась — пусто.
fn money_text(lang: Lang, sum: Option<Money>) -> String {
    sum.map(|sum| i18n::money(lang, sum)).unwrap_or_default()
}

/// Какие документы показать: все, только счета или только акты.
const FILTERS: [(Option<DocKind>, Key); 3] = [
    (None, Key::FilterAll),
    (Some(DocKind::Invoice), Key::NavDocuments),
    (Some(DocKind::Act), Key::FilterActs),
];

/// Список (дизайн «Kvartik Design»): поиск по клиенту и номеру, «все,
/// счета, акты», документы по месяцам — свежие сверху — и по страницам,
/// удалённые черновики — по кнопке. Новый счёт на телефоне — круглой
/// кнопкой «+» внизу, на компьютере — в меню; новый акт — под списком.
#[component]
fn DocList(ctx: Ctx) -> impl IntoView {
    let Ctx {
        lang,
        documents,
        clients,
        mode,
        notice,
        ..
    } = ctx;
    let query = RwSignal::new(String::new());
    let kind = RwSignal::new(None::<DocKind>);
    let limit = RwSignal::new(PAGE);
    let show_deleted = RwSignal::new(false);
    let failure = RwSignal::new(None::<Problem>);
    let busy = RwSignal::new(false);
    // Производные от списка, поиска и фильтра — пересчитываются, только
    // когда меняется одно из них (Правило 1).
    let any = Memo::new(move |_| documents.with(|list, _, _| list.iter().any(|d| !d.is_deleted())));
    let shown = Memo::new(move |_| {
        let (query, kind) = (query.get(), kind.get());
        documents.with(|list, _, _| {
            Document::by_date(list)
                .into_iter()
                .filter(|document| kind.is_none_or(|kind| document.data().kind == kind))
                .filter(|document| {
                    let lang = lang.get_untracked();
                    document.matches(&query, &client_label(lang, document, clients))
                })
                .cloned()
                .collect::<Vec<_>>()
        })
    });
    // Месяцы страницы — по порядку, у каждого его документы.
    let months = Memo::new(move |_| {
        let limit = limit.get();
        shown.with(|shown| {
            let mut months: Vec<((u16, u8), Vec<Document>)> = Vec::new();
            for document in shown.iter().take(limit) {
                let date = document.data().date;
                let month = (date.year(), date.month());
                match months.last_mut() {
                    Some((last, list)) if *last == month => list.push(document.clone()),
                    _ => months.push((month, vec![document.clone()])),
                }
            }
            months
        })
    });
    let deleted = Memo::new(move |_| {
        documents.with(|list, _, _| Document::deleted(list).into_iter().cloned().collect::<Vec<_>>())
    });
    let unreadable = Memo::new(move |_| documents.with(|_, _, unreadable| unreadable));
    let has_clients = Memo::new(move |_| !clients.alive().is_empty());
    // Год «этого» месяца — без года в заголовке: сегодня, а часы без даты —
    // год самого свежего документа.
    let this_year = move || {
        ctx.today.map(Date::year).unwrap_or_else(|| {
            shown.with(|shown| shown.first().map_or(Date::MIN_YEAR, |d| d.data().date.year()))
        })
    };

    let restore = move |id: RecordId| {
        act(ctx, busy, failure, id, Change::Restore, move |_| {
            notice.set(Some(Notice::Restored));
        });
    };
    let start = move |doc_kind: DocKind| {
        notice.set(None);
        documents.created.set(0);
        mode.set(Mode::New(doc_kind));
    };

    view! {
        {move || (unreadable.get() > 0).then(|| view! {
            <p role="alert">
                {move || {
                    let lang = lang.get();
                    fill(t(lang, Key::DocsUnreadable), &[&i18n::integer(lang, unreadable.get() as i64)])
                }}
            </p>
        })}
        {move || (documents.created.get() > 0).then(|| view! {
            <p role="status">
                {move || {
                    let lang = lang.get();
                    let count = i18n::integer(lang, documents.created.get() as i64);
                    fill(t(lang, Key::RecurCreated), &[&count])
                }}
            </p>
        })}
        {move || (!any.get()).then(|| view! { <p>{tr(lang, Key::DocsAbout)}</p> })}
        {move || (!has_clients.get()).then(|| view! { <p class="hint">{tr(lang, Key::DocsNeedClient)}</p> })}
        // Сообщение о сделанном — в карточке, когда она открыта, иначе здесь:
        // дважды на экране оно было бы лишним.
        <Show when=move || !matches!(ctx.router.screen(), Screen::Document(_))>
            <NoticeLine ctx busy failure />
        </Show>
        {move || failure.get().map(|problem| view! { <p role="alert">{tr(lang, problem.key())}</p> })}
        <Show when=move || any.get()>
            <div class="docs-tools">
                <label class="search-field">
                    <span class="visually-hidden">{tr(lang, Key::DocsSearch)}</span>
                    {icon(Icon::Search)}
                    <input
                        type="search"
                        bind:value=query
                        placeholder=move || t(lang.get(), Key::DocsSearch)
                        autocomplete="off"
                        maxlength=Line::MAX_CHARS.to_string()
                        on:input=move |_| limit.set(PAGE)
                    />
                </label>
                <div class="seg" role="group" aria-label=move || t(lang.get(), Key::DocsFilter)>
                    {FILTERS
                        .into_iter()
                        .map(|(value, key)| view! {
                            <button
                                type="button"
                                aria-pressed=move || (kind.get() == value).to_string()
                                on:click=move |_| {
                                    kind.set(value);
                                    limit.set(PAGE);
                                }
                            >
                                {tr(lang, key)}
                            </button>
                        })
                        .collect_view()}
                </div>
            </div>
            {move || shown.with(Vec::is_empty).then(|| view! {
                <p class="hint">{tr(lang, Key::DocsNotFound)}</p>
            })}
        </Show>
        <For each=move || months.with(|months| months.iter().map(|(month, _)| *month).collect::<Vec<_>>()) key=|month| *month let(month)>
            <h2 class="month">{move || i18n::month_heading(lang.get(), month.0, month.1, this_year())}</h2>
            <ul class="list documents">
                <For
                    each=move || months.with(|months| {
                        months.iter().find(|(known, _)| *known == month).map(|(_, list)| list.clone()).unwrap_or_default()
                    })
                    key=|document| (document.id(), document.meta().modified())
                    let(document)
                >
                    <DocEntry ctx document />
                </For>
            </ul>
        </For>
        {move || (shown.with(Vec::len) > limit.get()).then(|| view! {
            <button type="button" class="secondary" on:click=move |_| limit.update(|limit| *limit += PAGE)>
                {tr(lang, Key::DocsMore)}
            </button>
        })}
        <div class="actions docs-more">
            <button
                type="button"
                class="secondary"
                disabled=move || !has_clients.get()
                on:click=move |_| start(DocKind::Act)
            >
                {icon(Icon::Plus)}
                <span>{tr(lang, Key::DocNewAct)}</span>
            </button>
        </div>
        // Новый счёт — главное действие экрана: на телефоне круглой кнопкой
        // над вкладками, на компьютере он в меню (`ui/nav.rs`).
        <button
            type="button"
            class="fab"
            aria-label=move || t(lang.get(), Key::DocNewInvoice)
            disabled=move || !has_clients.get()
            on:click=move |_| start(DocKind::Invoice)
        >
            {icon(Icon::Plus)}
        </button>
        <Show when=move || { !deleted.with(Vec::is_empty) }>
            <button
                type="button"
                class="secondary"
                aria-expanded=move || show_deleted.get().to_string()
                on:click=move |_| show_deleted.update(|open| *open = !*open)
            >
                {move || {
                    let lang = lang.get();
                    if show_deleted.get() {
                        t(lang, Key::DocsDeletedHide).to_owned()
                    } else {
                        let count = deleted.with(Vec::len) as i64;
                        fill(t(lang, Key::DocsDeleted), &[&i18n::integer(lang, count)])
                    }
                }}
            </button>
            <Show when=move || show_deleted.get()>
                <ul class="documents deleted">
                    <For each=move || deleted.get() key=|document| (document.id(), document.meta().modified()) let(document)>
                        <li>
                            <p class="doc-title">{
                                let document = document.clone();
                                move || title(lang.get(), &document)
                            }</p>
                            <div class="actions">
                                <button
                                    type="button"
                                    class="secondary"
                                    disabled=move || busy.get()
                                    on:click={
                                        let id = document.id();
                                        move |_| restore(id)
                                    }
                                >
                                    {tr(lang, Key::ClientRestore)}
                                </button>
                            </div>
                        </li>
                    </For>
                </ul>
            </Show>
        </Show>
    }
}

/// Документ в списке (дизайн «Kvartik Design»): клиент, вид с номером
/// и дата слева, итог и статус словом справа. Вся строка — ссылка:
/// выпущенный открывается карточкой по своему адресу, черновик — формой.
/// Открытый на компьютере — отмечен (`aria-current`) и подсвечен.
#[component]
fn DocEntry(ctx: Ctx, document: Document) -> impl IntoView {
    let Ctx { lang, clients, router, .. } = ctx;
    let status = document.status(ctx.today_or(document.data().date));
    let (status_key, status_class) = status_look(status);
    let total = document.total().ok();
    let date = document.data().date;
    let id = document.id();
    let issued = document.issued().is_some();
    let href = if issued {
        Screen::Document(id).href()
    } else {
        Screen::Documents.href()
    };
    let label = document.clone();
    view! {
        <li>
            <a
                class="entry"
                href=href
                aria-current=move || (router.screen() == Screen::Document(id)).then_some("page")
                on:click=move |_| {
                    if !issued {
                        ctx.notice.set(None);
                        ctx.mode.set(Mode::Edit(id));
                    }
                }
            >
                <span class="entry-main">
                    <span class="entry-title">{move || client_label(lang.get(), &label, clients)}</span>
                    <span class="entry-meta">
                        <span>{move || row_title(lang.get(), &document)}</span>
                        <span>{move || i18n::date(lang.get(), date)}</span>
                    </span>
                </span>
                <span class="entry-side">
                    <span class="entry-amount">{move || money_text(lang.get(), total)}</span>
                    <span class=status_class>{tr(lang, status_key)}</span>
                </span>
            </a>
        </li>
    }
}

/// Счета, которые ждут оплаты, — на главном экране (дизайн «Kvartik
/// Design»): кому, номер и срок, сколько осталось и статус словом.
/// Строка — ссылка на карточку счёта. Пока база открывается — пусто, а не
/// «неоплаченных нет»: это было бы неправдой.
#[component]
pub fn Awaiting(lang: ReadSignal<Lang>, documents: Documents, clients: Clients) -> impl IntoView {
    // Сегодня — один раз (Архитектура, «Время — параметр»); часы без даты —
    // списка нет: без сегодня не сказать, что просрочено.
    let today = clock::today();
    let ready = Memo::new(move |_| {
        documents.state.with(|state| matches!(state, State::Ready { .. }))
    });
    // Пересчитывается, только когда меняются документы (Правило 1).
    let awaiting = Memo::new(move |_| {
        let Some(today) = today else {
            return Vec::new();
        };
        documents.with(|list, _, _| {
            Document::awaiting(list, today).into_iter().cloned().collect::<Vec<_>>()
        })
    });
    view! {
        <Show when=move || ready.get() && today.is_some()>
            <h2>{tr(lang, Key::HomeAwaiting)}</h2>
            <Show
                when=move || !awaiting.with(Vec::is_empty)
                fallback=move || view! { <p class="hint">{tr(lang, Key::HomeNoneAwaiting)}</p> }
            >
                <ul class="list">
                    <For each=move || awaiting.get() key=|document| (document.id(), document.meta().modified()) let(document)>
                        <AwaitingRow lang document clients today />
                    </For>
                </ul>
            </Show>
        </Show>
    }
}

/// Счёт в списке «ждут оплаты».
#[component]
fn AwaitingRow(
    lang: ReadSignal<Lang>,
    document: Document,
    clients: Clients,
    today: Option<Date>,
) -> impl IntoView {
    let today = today.unwrap_or(document.data().date);
    let (status_key, status_class) = status_look(document.status(today));
    let left = document.left().ok();
    let number = document.number().map(|number| number.to_string()).unwrap_or_default();
    let due = document.data().due;
    let href = Screen::Document(document.id()).href();
    view! {
        <li>
            <a class="entry" href=href>
                <span class="entry-main">
                    <span class="entry-title">{move || client_label(lang.get(), &document, clients)}</span>
                    <span class="entry-meta">
                        <span>{number}</span>
                        {due.map(|due| view! {
                            <span>{move || {
                                let lang = lang.get();
                                fill(t(lang, Key::DueBy), &[&i18n::date(lang, due)])
                            }}</span>
                        })}
                    </span>
                </span>
                <span class="entry-side">
                    <span class="entry-amount">{move || money_text(lang.get(), left)}</span>
                    <span class=status_class>{tr(lang, status_key)}</span>
                </span>
            </a>
        </li>
    }
}

/// Сообщение о сделанном: с кнопкой «Вернуть» после аннулирования
/// и «Восстановить» после удаления черновика.
#[component]
fn NoticeLine(ctx: Ctx, busy: RwSignal<bool>, failure: RwSignal<Option<Problem>>) -> impl IntoView {
    let Ctx { lang, notice, .. } = ctx;
    move || {
        notice.get().map(|shown| match shown {
            Notice::Saved => view! { <p role="status">{tr(lang, Key::DocSaved)}</p> }.into_any(),
            Notice::Paid => view! { <p role="status">{tr(lang, Key::PaymentSaved)}</p> }.into_any(),
            Notice::RateSaved => view! { <p role="status">{tr(lang, Key::DocRateSaved)}</p> }.into_any(),
            Notice::ActivitySaved => view! { <p role="status">{tr(lang, Key::DocActivitySaved)}</p> }.into_any(),
            Notice::Signed => view! { <p role="status">{tr(lang, Key::SignSaved)}</p> }.into_any(),
            Notice::Unsigned => view! { <p role="status">{tr(lang, Key::SignRemoved)}</p> }.into_any(),
            Notice::Restored => {
                view! { <p role="status">{tr(lang, Key::DocRestoredNotice)}</p> }.into_any()
            }
            Notice::Issued(title) => view! {
                <p role="status">{move || fill(t(lang.get(), Key::DocIssuedNotice), &[&title])}</p>
            }
            .into_any(),
            Notice::Unannulled(title) => view! {
                <p role="status">{move || fill(t(lang.get(), Key::DocUnannulledNotice), &[&title])}</p>
            }
            .into_any(),
            Notice::Annulled(id, title) => {
                let shown = title.clone();
                view! {
                    <div class="notice-row">
                        <p role="status">{move || fill(t(lang.get(), Key::DocAnnulledNotice), &[&shown])}</p>
                        <button
                            type="button"
                            class="secondary"
                            disabled=move || busy.get()
                            on:click=move |_| {
                                let title = title.clone();
                                act(ctx, busy, failure, id, Change::Unannul, move |_| {
                                    ctx.notice.set(Some(Notice::Unannulled(title)));
                                });
                            }
                        >
                            {tr(lang, Key::DocUnannul)}
                        </button>
                    </div>
                }
                .into_any()
            }
            Notice::Deleted(id) => view! {
                <div class="notice-row">
                    <p role="status">{tr(lang, Key::DocDeletedNotice)}</p>
                    <button
                        type="button"
                        class="secondary"
                        disabled=move || busy.get()
                        on:click=move |_| {
                            act(ctx, busy, failure, id, Change::Restore, move |_| {
                                ctx.notice.set(Some(Notice::Restored));
                            });
                        }
                    >
                        {tr(lang, Key::ClientRestore)}
                    </button>
                </div>
            }
            .into_any(),
        })
    }
}

/// Правка документа одним действием.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Change {
    Annul(Date),
    Unannul,
    Delete,
    Restore,
    AddPayment(Payment),
    RemovePayment(Payment),
    /// Курс выпущенного документа: драмы за единицу в десятитысячных,
    /// `None` — убрать (задача 14).
    SetRate(Option<i64>),
    /// Вид деятельности выпущенного документа, `None` — убрать (задача 19).
    SetActivity(Option<ActivityCode>),
    /// Акт подписан клиентом в этот день и отметка снята (задача 62).
    Sign(Date),
    Unsign,
}

/// Сделать `change` с документом `id` и, если записано, — `done` с ним.
/// Пока идёт одно действие, второе не начинается.
fn act(
    ctx: Ctx,
    busy: RwSignal<bool>,
    failure: RwSignal<Option<Problem>>,
    id: RecordId,
    change: Change,
    done: impl FnOnce(&Document) + 'static,
) {
    if busy.get_untracked() {
        return;
    }
    let Some(store) = ctx.base.get() else {
        failure.set(Some(Problem::Store(StoreError::Failed)));
        return;
    };
    busy.set(true);
    failure.set(None);
    leptos::task::spawn_local(async move {
        let outcome = apply(&store, id, change).await;
        busy.set(false);
        match outcome {
            Ok(Some(document)) => {
                done(&document);
                ctx.documents.put(document);
                ctx.backups.data_saved();
            }
            // Документ уже в этом состоянии или пропал — его поменяла другая
            // вкладка: окно показывает, что в базе.
            Ok(None) => ctx.documents.load(&store, ctx.today).await,
            Err(problem) => failure.set(Some(problem)),
        }
    });
}

/// Правка документа — одной транзакцией: прочитать, команда ядра,
/// записать. `Ok(None)` — записывать нечего.
async fn apply(store: &Store, id: RecordId, change: Change) -> Result<Option<Document>, Problem> {
    let tx = store.write()?;
    let Some(document) = tx.get::<Document>(id).await? else {
        return Ok(None);
    };
    let now = clock::now();
    let changed = match change {
        Change::Annul(today) => document.annul(today, now)?,
        Change::Unannul => document.unannul(now),
        Change::Delete => document.delete(now)?,
        Change::Restore => document.restore(now),
        Change::AddPayment(payment) => Some(document.add_payment(payment, now)?),
        Change::RemovePayment(payment) => document.remove_payment(payment, now),
        Change::SetRate(per_unit_e4) => document.set_rate(per_unit_e4, now)?,
        Change::SetActivity(activity) => document.set_activity(activity, now)?,
        Change::Sign(day) => document.sign(day, now)?,
        Change::Unsign => document.unsign(now),
    };
    let Some(changed) = changed else {
        return Ok(None);
    };
    let mut writes = Writes::default();
    writes.update(&changed);
    tx.commit(writes).await?;
    Ok(Some(changed))
}

/// С чего начинается форма.
#[derive(Clone, PartialEq, Debug)]
enum Start {
    /// Пустой документ этого вида: дата — сегодня, одна пустая строка.
    Blank(DocKind, Option<Date>),
    /// Данные черновика или акта по счёту.
    Data(DocumentData),
}

/// Строка документа в форме — строками, как их ввёл человек.
#[derive(Clone, Copy)]
struct ItemRow {
    key: u32,
    text: RwSignal<String>,
    quantity: RwSignal<String>,
    price: RwSignal<String>,
}

/// Поля формы. Читает их ядро (`parse` у каждого типа, `DocumentData::check`),
/// окно только показывает ошибки.
#[derive(Clone, Copy)]
struct Form {
    kind: DocKind,
    client: RwSignal<Option<RecordId>>,
    lang: RwSignal<Option<Lang>>,
    currency: RwSignal<Option<Currency>>,
    /// Даты — записью ISO, как их отдаёт `<input type="date">`.
    date: RwSignal<String>,
    service_date: RwSignal<String>,
    due: RwSignal<String>,
    /// Курс ЦБ для дня оказания услуги — строкой языка окна; пусто — курса
    /// ещё нет (задача 14).
    rate: RwSignal<String>,
    /// Вид деятельности дохода — код профиля (задача 19).
    activity: RwSignal<Option<ActivityCode>>,
    items: RwSignal<Vec<ItemRow>>,
    basis: Option<RecordId>,
    recurring: Option<RecordId>,
    /// Ключ следующей строки: у строки он свой до конца формы, и `For` не
    /// путает строки после удаления соседней.
    next_row: StoredValue<u32>,
    /// Была ли попытка сохранить: после неё ошибки полей видны.
    tried: RwSignal<bool>,
    /// Идёт сохранение.
    busy: RwSignal<bool>,
}

impl Form {
    /// Форма с данными черновика или пустая. Цены и количества — записью
    /// языка окна: её человек и правит.
    fn new(start: &Start, window: Lang) -> Self {
        let (kind, today) = match start {
            Start::Blank(kind, today) => (*kind, *today),
            Start::Data(data) => (data.kind, None),
        };
        let data = match start {
            Start::Data(data) => Some(data),
            Start::Blank(..) => None,
        };
        let iso = |date: Option<Date>| date.map(Date::iso).unwrap_or_default();
        let form = Self {
            kind,
            client: RwSignal::new(data.map(|d| d.client)),
            lang: RwSignal::new(data.map(|d| d.lang)),
            currency: RwSignal::new(data.map(|d| d.currency)),
            date: RwSignal::new(iso(data.map(|d| d.date).or(today))),
            service_date: RwSignal::new(iso(data.map(|d| d.service_date).or(today))),
            due: RwSignal::new(iso(data.and_then(|d| d.due))),
            rate: RwSignal::new(
                data.and_then(|d| d.rate)
                    .map(|rate| i18n::rate(window, rate.per_unit_e4()))
                    .unwrap_or_default(),
            ),
            activity: RwSignal::new(data.and_then(|d| d.activity)),
            items: RwSignal::new(Vec::new()),
            basis: data.and_then(|d| d.basis),
            recurring: data.and_then(|d| d.recurring),
            next_row: StoredValue::new(0),
            tried: RwSignal::new(false),
            busy: RwSignal::new(false),
        };
        let rows = match data {
            Some(data) => data
                .items
                .iter()
                .map(|item| {
                    let row = form.row();
                    row.text.set(item.text.as_str().to_owned());
                    row.quantity.set(i18n::decimal(window, item.quantity_e3, QUANTITY_DIGITS));
                    row.price.set(i18n::amount(window, Money::new(item.price, data.currency)));
                    row
                })
                .collect(),
            None => vec![form.fresh_row(window)],
        };
        form.items.set(rows);
        form
    }

    /// Новая строка: количество — 1, по записи языка окна. У услуги без
    /// часов остаётся вписать только цену (дизайн «Kvartik Design»: «что
    /// сделано» и «сумма»).
    fn fresh_row(&self, window: Lang) -> ItemRow {
        let row = self.row();
        row.quantity.set(i18n::decimal(window, 1_000, QUANTITY_DIGITS));
        row
    }

    fn row(&self) -> ItemRow {
        let key = self.next_row.get_value();
        self.next_row.set_value(key + 1);
        ItemRow {
            key,
            text: RwSignal::new(String::new()),
            quantity: RwSignal::new(String::new()),
            price: RwSignal::new(String::new()),
        }
    }

    /// Клиент выбран: язык, валюта и срок оплаты — те, что у него по
    /// умолчанию, если они у него есть.
    fn choose_client(&self, client: Option<Client>) {
        self.client.set(client.as_ref().map(Client::id));
        let Some(client) = client else {
            return;
        };
        let data = client.data();
        if data.lang.is_some() {
            self.lang.set(data.lang);
        }
        if data.currency.is_some() {
            self.currency.set(data.currency);
        }
        let date = Date::from_iso(&self.date.get_untracked());
        if let (DocKind::Invoice, Some(days), Some(date)) = (self.kind, data.payment_days, date)
            && let Some(due) = date.add_days(i32::from(days))
        {
            self.due.set(due.iso());
        }
    }

    /// Данные из полей; `None` — где-то ошибка, и её видно у поля. `needed`
    /// — нужен ли доходу вид деятельности (`turnover::activity_needed`,
    /// задача 19).
    fn data(&self, window: Lang, needed: bool) -> Option<DocumentData> {
        let currency = self.currency.get_untracked()?;
        let items = self
            .items
            .get_untracked()
            .iter()
            .map(|row| row.item(window, currency))
            .collect::<Option<Vec<_>>>()?;
        let due = match self.due.get_untracked().as_str() {
            "" => None,
            text => Some(Date::from_iso(text)?),
        };
        let mut data = DocumentData {
            kind: self.kind,
            client: self.client.get_untracked()?,
            lang: self.lang.get_untracked()?,
            currency,
            date: Date::from_iso(&self.date.get_untracked())?,
            service_date: Date::from_iso(&self.service_date.get_untracked())?,
            due: match self.kind {
                DocKind::Invoice => due,
                DocKind::Act => None,
            },
            items,
            basis: self.basis,
            recurring: self.recurring,
            rate: None,
            activity: self.activity.get_untracked(),
        };
        if needed && data.activity.is_none() {
            return None;
        }
        // Курс — только у валюты и для дня оказания услуги: поменяли день
        // или валюту — курс в данных следует за ними (`rate_for`).
        let typed = self.rate.get_untracked();
        if currency != Currency::Amd && !typed.trim().is_empty() {
            let per_unit_e4 = i18n::parse_rate(window, &typed).ok()?;
            data.rate = Some(data.rate_for(per_unit_e4).ok()?);
        }
        data.check().ok()?;
        Some(data)
    }
}

impl ItemRow {
    /// Строка из полей; `None` — где-то ошибка.
    fn item(&self, window: Lang, currency: Currency) -> Option<Item> {
        let item = Item {
            text: Line::new(&self.text.get_untracked()).ok()?,
            quantity_e3: i18n::parse_decimal(window, &self.quantity.get_untracked(), QUANTITY_DIGITS)
                .ok()?,
            price: i18n::parse_money(window, &self.price.get_untracked(), currency)
                .ok()?
                .minor(),
        };
        (!item.text.is_empty() && item.quantity_e3 > 0 && item.price >= 0).then_some(item)
    }
}

/// Ошибка введённого числа — словами, с примером верной записи на языке
/// окна.
pub(super) fn number_problem(lang: Lang, error: ParseError, example: &str) -> String {
    match error {
        ParseError::Empty => fill(t(lang, Key::NumberEmpty), &[example]),
        ParseError::Invalid => fill(t(lang, Key::NumberInvalid), &[example]),
        ParseError::Separators => fill(t(lang, Key::NumberSeparators), &[example]),
        ParseError::TooPrecise { max: 0 } => fill(t(lang, Key::NumberWhole), &[example]),
        ParseError::TooPrecise { max } => {
            fill(t(lang, Key::NumberTooPrecise), &[&i18n::integer(lang, i64::from(max)), example])
        }
        ParseError::TooLarge => t(lang, Key::NumberTooLarge).to_owned(),
    }
}

/// Что не так с количеством: число не прочиталось или оно не больше нуля.
fn quantity_problem(lang: Lang, text: &str) -> Option<String> {
    match i18n::parse_decimal(lang, text, QUANTITY_DIGITS) {
        Ok(quantity) if quantity > 0 => None,
        Ok(_) => Some(t(lang, Key::ItemQuantityZero).to_owned()),
        Err(error) => {
            let example = i18n::decimal(lang, 37_500, QUANTITY_DIGITS);
            Some(number_problem(lang, error, &example))
        }
    }
}

/// Что не так с ценой. Валюта не выбрана — молчит: об этом говорит поле
/// валюты, а прочесть цену без неё нельзя.
fn price_problem(lang: Lang, text: &str, currency: Option<Currency>) -> Option<String> {
    let currency = currency?;
    match i18n::parse_money(lang, text, currency) {
        Ok(price) if price.minor() >= 0 => None,
        Ok(_) => Some(t(lang, Key::ItemPriceNegative).to_owned()),
        Err(error) => {
            let example = i18n::amount(lang, Money::new(150_000, currency));
            Some(number_problem(lang, error, &example))
        }
    }
}

/// Ошибка поля — словами, после первой попытки сохранить.
pub(super) fn shown_problem(
    tried: RwSignal<bool>,
    problem: impl Fn() -> Option<String> + Send + Sync + 'static,
) -> impl IntoView {
    move || {
        tried
            .get()
            .then(&problem)
            .flatten()
            .map(|text| view! { <p class="field-error" role="alert">{text}</p> })
    }
}

split! {
    /// Выпущенный документ — своей частью wasm (`ui/split.rs`): просмотр
    /// с оплатами и действиями нужен только по «Открыть».
    pub(super) fn opened_document(ctx: Ctx, id: RecordId) -> AnyView, preload preload_opened {
        view! { <DocView ctx id /> }.into_any()
    }
}

/// Форма документа. Новый — по шагам (дизайн «Kvartik Design»): кому, что
/// сделано, проверить и выпустить. Правка черновика и акт по счёту — одной
/// формой со всеми полями: там всё уже заполнено.
#[component]
fn DocForm(ctx: Ctx, existing: Option<Document>, start: Start) -> impl IntoView {
    let Ctx {
        lang,
        documents,
        clients,
        mode,
        notice,
        forms,
        ..
    } = ctx;
    forms.hold();
    let form = Form::new(&start, lang.get_untracked());
    let id = existing.as_ref().map(Document::id);
    // Новый — по шагам; у правки и акта по счёту шаг всегда последний,
    // и видны все поля.
    let wizard = matches!(start, Start::Blank(..));
    let step = RwSignal::new(if wizard { 1_u8 } else { 3 });
    let shows = move |n: u8| !wizard || step.get() == n;
    let failure = RwSignal::new(None::<Problem>);
    let invalid = RwSignal::new(false);
    let tried = form.tried;
    let alive = Memo::new(move |_| clients.alive());
    // Коды профиля и нужен ли доходу вид деятельности на день услуги
    // (задача 19).
    let activities = Memo::new(move |_| ctx.profiles.activities());
    let needed = Memo::new(move |_| {
        let day = form.service_date.with(|text| Date::from_iso(text)).or(ctx.today);
        day.is_some_and(|day| ctx.profiles.with(|profile| turnover::activity_needed(profile, day)))
    });
    // Выбранный клиент — из живых.
    let chosen = move || {
        let chosen = form.client.get()?;
        alive.with(|list| list.iter().find(|client| client.id() == chosen).cloned())
    };
    // Клиент из Армении: счёт программы не заменяет чек кассы, и окно
    // говорит это местным ИП, а не молчит (задача 11).
    let local = move || {
        let chosen = form.client.get()?;
        alive.with(|list| {
            list.iter()
                .find(|client| client.id() == chosen)
                .map(|client| client.data().country == Some(Country::ARMENIA))
        })
    };
    // Клиент черновика удалён: в списке выбора его нет, и окно говорит, кто
    // это был.
    let gone = move || {
        let chosen = form.client.get()?;
        match clients.name(chosen) {
            Some((name, true)) => Some(name),
            Some((_, false)) | None => None,
        }
    };

    // Шаг готов — можно дальше: клиент выбран; валюта выбрана, и каждая
    // строка читается. Остальное проверят выпуск и сохранение.
    let ready = move |step: u8| match step {
        1 => form.client.get_untracked().is_some(),
        _ => {
            let window = lang.get_untracked();
            form.currency.get_untracked().is_some_and(|currency| {
                form.items
                    .with_untracked(|rows| rows.iter().all(|row| row.item(window, currency).is_some()))
            })
        }
    };
    let next = move || {
        let now = step.get_untracked();
        if ready(now) {
            invalid.set(false);
            form.tried.set(false);
            step.set(now + 1);
        } else {
            form.tried.set(true);
            invalid.set(true);
        }
    };
    let submit = move |issue: bool| {
        if form.busy.get_untracked() {
            return;
        }
        form.tried.set(true);
        let Some(data) = form.data(lang.get_untracked(), needed.get_untracked()) else {
            invalid.set(true);
            return;
        };
        invalid.set(false);
        failure.set(None);
        let Some(store) = ctx.base.get() else {
            failure.set(Some(Problem::Store(StoreError::Failed)));
            return;
        };
        form.busy.set(true);
        leptos::task::spawn_local(async move {
            let outcome = save(&store, id, data, issue).await;
            form.busy.set(false);
            match outcome {
                Ok(document) => {
                    let shown = document.id();
                    let issued = document.issued().is_some();
                    let named = title(lang.get_untracked(), &document);
                    documents.put(document);
                    ctx.backups.data_saved();
                    if issued {
                        notice.set(Some(Notice::Issued(named)));
                        mode.set(Mode::List);
                        ctx.router.go(Screen::Document(shown));
                    } else {
                        notice.set(Some(Notice::Saved));
                        mode.set(Mode::List);
                    }
                }
                Err(problem) => failure.set(Some(problem)),
            }
        });
    };
    let delete = move |_| {
        if let Some(id) = id {
            act(ctx, form.busy, failure, id, Change::Delete, move |_| {
                notice.set(Some(Notice::Deleted(id)));
                mode.set(Mode::List);
            });
        }
    };
    let leave = move || mode.set(Mode::List);
    // Шаг сменился — прежняя жалоба к нему не относится.
    Effect::watch(move || step.get(), move |_, _, _| invalid.set(false), false);
    // «Как будет выглядеть» (задача 12): страницы черновика — снимок полей
    // в момент нажатия. Поменяли поле шага или сам шаг — снимок устарел
    // и убирается, а не показывает прежнее.
    let preview = RwSignal::new(None::<Request>);
    let show_preview = move |_| {
        form.tried.set(true);
        let Some(data) = form.data(lang.get_untracked(), needed.get_untracked()) else {
            invalid.set(true);
            return;
        };
        let client = alive.with_untracked(|list| {
            list.iter()
                .find(|client| client.id() == data.client)
                .map(|client| client.data().clone())
        });
        let Some(client) = client else {
            invalid.set(true);
            return;
        };
        invalid.set(false);
        let basis = data
            .basis
            .and_then(|basis| documents.get(basis))
            .and_then(|invoice| invoice.number());
        preview.set(Some(Request::Draft {
            data: Box::new(data),
            client,
            basis,
        }));
    };
    Effect::watch(
        move || {
            (
                step.get(),
                form.client.get(),
                form.lang.get(),
                form.currency.get(),
                form.date.get(),
                form.service_date.get(),
                form.due.get(),
            )
        },
        move |_, _, _| preview.set(None),
        false,
    );
    let last_day = move |_| {
        let text = form.service_date.get_untracked();
        let base = Date::from_iso(&text).or_else(|| Date::from_iso(&form.date.get_untracked()));
        if let Some(day) = base {
            form.service_date.set(day.last_of_month().iso());
        }
    };
    let total_money = move |lang: Lang| {
        let currency = form.currency.get()?;
        let lines = form.items.with(|rows| {
            rows.iter()
                .map(|row| line_total(lang, *row, currency))
                .collect::<Option<Vec<_>>>()
        })?;
        Money::total(currency, lines).ok()
    };
    let total = move || {
        let lang = lang.get();
        total_money(lang).map(|sum| fill(t(lang, Key::DocTotal), &[&i18n::money(lang, sum)]))
    };
    // Итог в драмах по курсу из поля — как в дизайне «Kvartik Design»
    // (экран 04), когда курс уже есть.
    let in_drams = move || {
        let lang = lang.get();
        let date = form.service_date.with(|text| Date::from_iso(text));
        form.rate.with(|rate| equivalent(lang, total_money(lang), rate, date))
    };
    let foreign = move || form.currency.get().is_some_and(|currency| currency != Currency::Amd);
    let full = move || form.items.with(|rows| rows.len() >= DocumentData::MAX_ITEMS);
    // Выбранный клиент карточкой — на втором и третьем шаге: что о нём
    // знает документ и «Изменить», которое возвращает к выбору.
    let picked = move || {
        chosen().map(|client| {
            let id = client.id();
            let data = client.data().clone();
            let name = data.name.as_str().to_owned();
            view! {
                <div class="picked">
                    <Avatar name=name.clone() tone=tone(id) />
                    <span class="entry-main">
                        <span class="entry-title">{name}</span>
                        <span class="entry-meta">
                            {move || client_facts(lang.get(), data.country, form.currency.get(), form.lang.get())}
                        </span>
                    </span>
                    <button type="button" class="link" on:click=move |_| step.set(1)>
                        {tr(lang, Key::Change)}
                    </button>
                </div>
            }
        })
    };
    // Сменили язык окна — цены и количества записываются по-новому, а их
    // смысл не меняется (`retype`).
    Effect::watch(
        move || lang.get(),
        move |now: &Lang, before: Option<&Lang>, _| {
            let (Some(&before), now) = (before, *now) else {
                return;
            };
            let currency = form.currency.get_untracked();
            retype(form.rate, |text| i18n::parse_rate(before, text), |rate| i18n::rate(now, rate));
            for row in form.items.get_untracked() {
                retype(
                    row.quantity,
                    |text| i18n::parse_decimal(before, text, QUANTITY_DIGITS),
                    |quantity| i18n::decimal(now, quantity, QUANTITY_DIGITS),
                );
                if let Some(currency) = currency {
                    retype(
                        row.price,
                        |text| i18n::parse_money(before, text, currency),
                        |price| i18n::amount(now, price),
                    );
                }
            }
        },
        false,
    );
    let (heading, who) = match form.kind {
        DocKind::Invoice => (Key::DocNewInvoice, Key::DocWhoInvoice),
        DocKind::Act => (Key::DocNewAct, Key::DocWhoAct),
    };

    view! {
        // «Ввод» в поле до последнего шага — «Дальше», на последнем —
        // «Сохранить черновик», как и было.
        <form class="document" on:submit=move |event: SubmitEvent| {
            event.prevent_default();
            if step.get_untracked() < 3 {
                next();
            } else {
                submit(false);
            }
        } novalidate>
            {if wizard {
                view! { <WizardHead lang title=heading step steps=3 leave /> }.into_any()
            } else {
                view! {
                    <p class="doc-title">{move || {
                        let kind = t(lang.get(), kind_key(form.kind));
                        fill(t(lang.get(), Key::DocDraftTitle), &[kind])
                    }}</p>
                }
                .into_any()
            }}
            <fieldset hidden=move || !shows(1)>
                {if wizard {
                    // Клиент выбирается нажатием на строку, и оно же ведёт
                    // дальше: отдельная «Дальше» была бы лишним шагом.
                    view! {
                        <legend class="question">{tr(lang, who)}</legend>
                        <ul class="list pick">
                            {move || alive.get().into_iter().map(|client| {
                                let id = client.id();
                                let data = client.data().clone();
                                let name = data.name.as_str().to_owned();
                                let current = move || form.client.get() == Some(id);
                                view! {
                                    <li>
                                        <button
                                            type="button"
                                            class="entry"
                                            aria-current=move || current().then_some("true")
                                            on:click=move |_| {
                                                form.choose_client(Some(client.clone()));
                                                step.set(2);
                                            }
                                        >
                                            <Avatar name=name.clone() tone=tone(id) />
                                            <span class="entry-main">
                                                <span class="entry-title">{name}</span>
                                                <span class="entry-meta">
                                                    {move || client_facts(lang.get(), data.country, data.currency, data.lang)}
                                                </span>
                                            </span>
                                            {move || icon(if current() { Icon::Check } else { Icon::Forward })}
                                        </button>
                                    </li>
                                }
                            }).collect_view()}
                        </ul>
                    }
                    .into_any()
                } else {
                    view! {
                        <label>
                            {tr(lang, Key::DocClient)}
                            <select on:change=move |event| {
                                let chosen = event_target_value(&event);
                                let client = alive.with_untracked(|list| {
                                    list.iter().find(|c| c.id().to_string() == chosen).cloned()
                                });
                                form.choose_client(client);
                            }>
                                <option value="" prop:selected=move || form.client.get().is_none()>
                                    {tr(lang, Key::NotChosen)}
                                </option>
                                {move || alive.get().into_iter().map(|client| {
                                    let id = client.id();
                                    view! {
                                        <option value=id.to_string() prop:selected=move || form.client.get() == Some(id)>
                                            {client.data().name.as_str().to_owned()}
                                        </option>
                                    }
                                }).collect_view()}
                            </select>
                        </label>
                        {move || gone().map(|name| view! {
                            <p class="field-error" role="alert">{move || fill(t(lang.get(), Key::DocClientGone), &[&name])}</p>
                        })}
                        {shown_problem(tried, move || {
                            form.client.get().is_none().then(|| t(lang.get(), Key::DocClientMissing).to_owned())
                        })}
                    }
                    .into_any()
                }}
            </fieldset>
            <fieldset hidden=move || !shows(2)>
                {if wizard {
                    view! {
                        <legend class="question">{tr(lang, Key::DocWhat)}</legend>
                        <p class="hint lead">{tr(lang, Key::DocWhatHint)}</p>
                        {picked}
                    }
                    .into_any()
                } else {
                    view! {
                        <legend>{tr(lang, Key::DocItems)}</legend>
                        <p class="hint">{tr(lang, Key::DocItemsHint)}</p>
                    }
                    .into_any()
                }}
                <label>
                    {tr(lang, Key::DocCurrency)}
                    <select on:change=move |event| {
                        let code = event_target_value(&event);
                        form.currency.set(CURRENCIES.into_iter().find(|c| currency_code(*c) == code));
                    }>
                        <option value="" prop:selected=move || form.currency.get().is_none()>
                            {tr(lang, Key::NotChosen)}
                        </option>
                        {CURRENCIES
                            .into_iter()
                            .map(|currency| view! {
                                <option
                                    value=currency_code(currency)
                                    prop:selected=move || form.currency.get() == Some(currency)
                                >
                                    {tr(lang, currency_key(currency))}
                                </option>
                            })
                            .collect_view()}
                    </select>
                </label>
                {shown_problem(tried, move || {
                    form.currency.get().is_none().then(|| t(lang.get(), Key::DocCurrencyMissing).to_owned())
                })}
                <For each=move || form.items.get() key=|row| row.key let(row)>
                    <div class="row">
                        <TextField lang label=Key::ItemText value=row.text tried autocomplete="off" />
                        {shown_problem(tried, move || {
                            row.text.with(|text| text.trim().is_empty())
                                .then(|| t(lang.get(), Key::ItemTextMissing).to_owned())
                        })}
                        <TextField
                            lang
                            label=Key::ItemQuantity
                            value=row.quantity
                            tried
                            inputmode="decimal"
                            autocomplete="off"
                            max=24
                            check=|_| None
                        />
                        {shown_problem(tried, move || row.quantity.with(|text| quantity_problem(lang.get(), text)))}
                        <TextField
                            lang
                            label=Key::ItemPrice
                            value=row.price
                            tried
                            inputmode="decimal"
                            autocomplete="off"
                            max=32
                            check=|_| None
                        />
                        {shown_problem(tried, move || {
                            row.price.with(|text| price_problem(lang.get(), text, form.currency.get()))
                        })}
                        <p class="hint">{move || {
                            let lang = lang.get();
                            let currency = form.currency.get()?;
                            let sum = line_total(lang, row, currency)?;
                            let quantity = i18n::parse_decimal(lang, &row.quantity.get(), QUANTITY_DIGITS).ok()?;
                            let price = i18n::parse_money(lang, &row.price.get(), currency).ok()?;
                            Some(fill(
                                t(lang, Key::ItemLine),
                                &[
                                    &i18n::decimal(lang, quantity, QUANTITY_DIGITS),
                                    &i18n::money(lang, price),
                                    &i18n::money(lang, sum),
                                ],
                            ))
                        }}</p>
                        {move || (form.items.with(Vec::len) > 1).then(|| view! {
                            <button
                                type="button"
                                class="secondary"
                                on:click=move |_| form.items.update(|rows| rows.retain(|r| r.key != row.key))
                            >
                                {tr(lang, Key::Remove)}
                            </button>
                        })}
                    </div>
                </For>
                <button
                    type="button"
                    class="link"
                    disabled=full
                    on:click=move |_| {
                        let row = form.fresh_row(lang.get_untracked());
                        form.items.update(|rows| rows.push(row));
                    }
                >
                    {icon(Icon::Plus)}
                    <span>{tr(lang, Key::ItemAdd)}</span>
                </button>
                <p class="total">{total}</p>
                <p class="hint">{in_drams}</p>
            </fieldset>
            <fieldset hidden=move || !shows(3)>
                {wizard.then(|| view! {
                    <legend class="question">{tr(lang, Key::DocReview)}</legend>
                    {picked}
                    <p class="total">{total}</p>
                })}
                <label>
                    {tr(lang, Key::DocLanguage)}
                    <select on:change=move |event| form.lang.set(Lang::from_code(&event_target_value(&event)))>
                        <option value="" prop:selected=move || form.lang.get().is_none()>
                            {tr(lang, Key::NotChosen)}
                        </option>
                        {Lang::ALL
                            .into_iter()
                            .map(|doc_lang| view! {
                                <option
                                    value=doc_lang.code()
                                    lang=doc_lang.code()
                                    prop:selected=move || form.lang.get() == Some(doc_lang)
                                >
                                    {doc_lang.native_name()}
                                </option>
                            })
                            .collect_view()}
                    </select>
                </label>
                {shown_problem(tried, move || {
                    form.lang.get().is_none().then(|| t(lang.get(), Key::DocLanguageMissing).to_owned())
                })}
                <DateField lang label=Key::DocDate value=form.date tried />
                <DateField lang label=Key::DocServiceDate value=form.service_date tried />
                <p class="hint">{tr(lang, Key::DocServiceDateHint)}</p>
                <button type="button" class="secondary" on:click=last_day>
                    {tr(lang, Key::DocLastDay)}
                </button>
                // Вид деятельности дохода — когда без него не узнать строку
                // налога или режим (задача 19).
                {move || (needed.get() || form.activity.get().is_some()).then(|| view! {
                    <IncomeActivity
                        lang
                        activities
                        chosen=form.activity
                        date=form.service_date
                        tried
                        required=needed
                    />
                })}
                <Show when=foreign>
                    <label>
                        {move || {
                            let lang = lang.get();
                            let code = form.currency.get().map(currency_code).unwrap_or_default();
                            fill(t(lang, Key::OpRate), &[code])
                        }}
                        <input type="text" inputmode="decimal" autocomplete="off" maxlength="24" bind:value=form.rate />
                    </label>
                    <p class="hint">{tr(lang, Key::RateHint)}</p>
                    {shown_problem(tried, move || form.rate.with(|text| rate_problem(lang.get(), text)))}
                    {wizard.then(|| view! { <p class="hint">{in_drams}</p> })}
                </Show>
                {(form.kind == DocKind::Invoice).then(|| view! {
                    <label>
                        {tr(lang, Key::DocDue)}
                        <input type="date" bind:value=form.due />
                    </label>
                    {shown_problem(tried, move || {
                        let due = Date::from_iso(&form.due.get())?;
                        let date = Date::from_iso(&form.date.get())?;
                        (due < date).then(|| t(lang.get(), Key::DocDueEarly).to_owned())
                    })}
                })}
                {move || (local() == Some(true)).then(|| view! {
                    <p class="hint">{tr(lang, Key::DocCashReceipt)}</p>
                })}
                <div class="actions">
                    <button
                        type="button"
                        class="secondary"
                        aria-expanded=move || preview.with(Option::is_some).to_string()
                        on:click=show_preview
                    >
                        {tr(lang, Key::DocPreview)}
                    </button>
                </div>
                {move || preview.get().map(|request| view! {
                    <PaperPanel lang base=ctx.base request close=move || preview.set(None) />
                })}
            </fieldset>
            {move || invalid.get().then(|| view! {
                <p role="alert">{move || {
                    let fix = if step.get() < 3 { Key::StepFix } else { Key::ProfileFix };
                    t(lang.get(), fix)
                }}</p>
            })}
            {move || failure.get().map(|problem| view! {
                <p role="alert">{tr(lang, problem.key())}</p>
            })}
            {move || match step.get() {
                1 => None,
                2 => Some(view! {
                    <button type="submit" class="wide">{tr(lang, Key::DocNextReview)}</button>
                }
                .into_any()),
                _ => Some(view! {
                    <p class="hint">{tr(lang, Key::DocIssueHint)}</p>
                    <div class="actions">
                        <button
                            type="button"
                            class:wide=wizard
                            disabled=move || form.busy.get()
                            on:click=move |_| submit(true)
                        >
                            {tr(lang, Key::DocIssue)}
                        </button>
                        <button type="submit" class="secondary" disabled=move || form.busy.get()>
                            {tr(lang, Key::DocSaveDraft)}
                        </button>
                        {id.map(|_| view! {
                            <button type="button" class="secondary" disabled=move || form.busy.get() on:click=delete>
                                {tr(lang, Key::DocDeleteDraft)}
                            </button>
                        })}
                        <button
                            type="button"
                            class="secondary"
                            disabled=move || form.busy.get()
                            on:click=move |_| leave()
                        >
                            {tr(lang, Key::Cancel)}
                        </button>
                    </div>
                }
                .into_any()),
            }}
        </form>
    }
}

/// Сведения о клиенте документа — страна, валюта и язык, что известно.
fn client_facts(
    lang: Lang,
    country: Option<Country>,
    currency: Option<Currency>,
    doc_lang: Option<Lang>,
) -> impl IntoView {
    let country = country.map(|country| country.name(lang).to_owned());
    let currency = currency.map(|currency| currency_code(currency).to_owned());
    let doc_lang = doc_lang.map(|doc_lang| doc_lang.native_name().to_owned());
    [country, currency, doc_lang]
        .into_iter()
        .flatten()
        .map(|fact| view! { <span>{fact}</span> })
        .collect_view()
}

/// Сумма строки из полей; `None` — поле не прочиталось.
fn line_total(lang: Lang, row: ItemRow, currency: Currency) -> Option<Money> {
    let item = Item {
        text: Line::default(),
        quantity_e3: row
            .quantity
            .with(|text| i18n::parse_decimal(lang, text, QUANTITY_DIGITS).ok())?,
        price: row
            .price
            .with(|text| i18n::parse_money(lang, text, currency).ok())?
            .minor(),
    };
    item.total(currency).ok()
}

/// Поле даты: подпись, `<input type="date">` и ошибка, если даты нет.
#[component]
pub(super) fn DateField(lang: ReadSignal<Lang>, label: Key, value: RwSignal<String>, tried: RwSignal<bool>) -> impl IntoView {
    view! {
        <label>
            {tr(lang, label)}
            <input type="date" bind:value=value />
        </label>
        {shown_problem(tried, move || {
            value.with(|text| Date::from_iso(text).is_none()).then(|| t(lang.get(), Key::DocDateMissing).to_owned())
        })}
    }
}

/// Сохранить черновик или выпустить — одним действием: команды ядра и одна
/// транзакция. Выпуск читает в ней же все документы (номер), профиль
/// и клиента (копия реквизитов).
async fn save(
    store: &Store,
    id: Option<RecordId>,
    data: DocumentData,
    issue: bool,
) -> Result<Document, Problem> {
    let tx = store.write()?;
    let now = clock::now();
    let existing = match id {
        // Черновика нет — его стёрли в обход программы: сохранять некуда.
        Some(id) => Some(tx.get::<Document>(id).await?.ok_or(StoreError::Failed)?),
        None => None,
    };
    let draft = match &existing {
        None => Document::create(data, now)?,
        Some(known) => known.edit(data, now)?.unwrap_or_else(|| known.clone()),
    };
    let document = if issue {
        let listed = tx.list::<Document>().await?;
        let profile = Profile::current(tx.all::<Profile>().await?);
        let client = tx
            .get::<Client>(draft.data().client)
            .await?
            .ok_or(DocError::ClientDeleted)?;
        draft.issue(
            &listed.records,
            listed.unreadable,
            profile.as_ref().map(Profile::data),
            &client,
            now,
        )?
    } else {
        draft
    };
    let mut writes = Writes::default();
    match &existing {
        None => writes.insert(&document),
        Some(known) if *known != document => writes.update(&document),
        Some(_) => {}
    }
    tx.commit(writes).await?;
    Ok(document)
}

/// Выпущенный документ карточкой (дизайн «Kvartik Design»): статус, итог,
/// клиент и даты, главное действие, строки, оплаты, история и редкие
/// действия внизу. Документ читается из списка со слежением: оплата
/// и аннулирование показываются сразу.
#[component]
fn DocView(ctx: Ctx, id: RecordId) -> impl IntoView {
    let Ctx { lang, documents, .. } = ctx;
    let document = Memo::new(move |_| {
        documents.with(|list, _, _| list.iter().find(|d| d.id() == id).cloned())
    });
    let failure = RwSignal::new(None::<Problem>);
    let busy = RwSignal::new(false);
    move || {
        let back = view! {
            <a class="back" href=Screen::Documents.href()>
                {icon(Icon::Back)}
                <span>{tr(lang, Key::NavDocuments)}</span>
            </a>
        };
        let Some(shown) = document.get() else {
            return view! {
                {back}
                <p class="hint">{tr(lang, Key::DocsNotFound)}</p>
            }
            .into_any();
        };
        let invoice = shown.data().kind == DocKind::Invoice;
        view! {
            {back}
            <DocCard ctx document=shown.clone() busy failure />
            <ForBook ctx document=shown.clone() busy failure />
            <NoticeLine ctx busy failure />
            {move || failure.get().map(|problem| view! { <p role="alert">{tr(lang, problem.key())}</p> })}
            <DocItems ctx document=shown.clone() />
            {invoice.then(|| view! { <Payments ctx document=shown.clone() busy failure /> })}
            {(!invoice).then(|| view! { <Signature ctx document=shown.clone() busy failure /> })}
            <History ctx document=shown.clone() />
            <DocActions ctx document=shown busy failure />
        }
        .into_any()
    }
}

/// Карточка документа: номер заголовком, статус словом, итог крупно,
/// клиент, даты и язык. У счёта, который ждёт оплаты, — главное действие
/// «Отметить, что оплачен»: поступление на весь остаток сегодняшним днём;
/// оплату частями и другим днём отмечают ниже, в оплатах. У действующего
/// счёта — «Акт по счёту», у всякого выпущенного документа рядом — «PDF»
/// (задача 12): аннулированный тоже хранят, и в PDF он с пометкой.
#[component]
fn DocCard(
    ctx: Ctx,
    document: Document,
    busy: RwSignal<bool>,
    failure: RwSignal<Option<Problem>>,
) -> impl IntoView {
    let Ctx {
        lang,
        documents,
        clients,
        mode,
        notice,
        ..
    } = ctx;
    let data = document.data().clone();
    let status = document.status(ctx.today_or(data.date));
    let (status_key, status_class) = status_look(status);
    let id = document.id();
    let named = document.clone();
    let client = document.clone();
    let basis_number = data
        .basis
        .and_then(|basis| documents.get(basis))
        .and_then(|invoice| invoice.number());
    let basis = basis_number.map(|number| number.to_string());
    // Документ на бумаге — по «PDF» (задача 12).
    let paper = RwSignal::new(false);
    let request = Request::Issued {
        document: Box::new(document.clone()),
        basis: basis_number,
    };
    let total = document.total().ok();
    let left = document.left().ok().filter(|left| left.minor() > 0);
    let awaiting = matches!(status, Status::Unpaid | Status::PartlyPaid | Status::Overdue);
    let invoice = data.kind == DocKind::Invoice;
    let open = document.annulled().is_none();
    let covered = covered_by_act(ctx, &document);
    // Сколько дней до срока — у счёта, который ждёт оплаты и ещё не
    // просрочен: о просроченном говорит статус.
    let days_left = match (ctx.today, data.due) {
        (Some(today), Some(due)) if awaiting && due >= today => Some(due.days_since(today)),
        _ => None,
    };
    let mark_paid = move |_| {
        let (Some(today), Some(left)) = (ctx.today, left) else {
            return;
        };
        let payment = Payment {
            date: today,
            amount: left.minor(),
        };
        act(ctx, busy, failure, id, Change::AddPayment(payment), move |_| {
            notice.set(Some(Notice::Paid));
        });
    };
    view! {
        <div class="doc-card">
            <h2 class="doc-heading" tabindex="-1">{move || title(lang.get(), &named)}</h2>
            <span class=format!("{status_class} dot")>{tr(lang, status_key)}</span>
            <p class="amount">{move || money_text(lang.get(), total)}</p>
            // Итог в драмах по курсу ЦБ — под итогом, как в дизайне «Kvartik
            // Design» (экран 05); у действующего документа в валюте. У счёта
            // с действующим актом без курса — ничего: книга берёт акт, и о нём
            // говорит `ForBook`.
            {(data.currency != Currency::Amd && open).then(|| {
                let amd = data.total_lumas().ok();
                let (currency, rate) = (data.currency, data.rate);
                move || (rate.is_some() || !covered.get()).then(|| view! {
                    <p class="hint">{move || {
                        let lang = lang.get();
                        match (amd, rate) {
                            (Some(amd), Some(rate)) => format!(
                                "{} ({})",
                                fill(t(lang, Key::LedgerInDrams), &[&i18n::lumas(lang, amd)]),
                                i18n::rate_pair(lang, currency, rate.per_unit_e4())
                            ),
                            _ => t(lang, Key::DocRateMissing).to_owned(),
                        }
                    }}</p>
                })
            })}
            <p class="doc-client">{move || client_label(lang.get(), &client, clients)}</p>
            <dl class="pairs">
                <div>
                    <dt>{tr(lang, Key::DocDate)}</dt>
                    <dd>{move || i18n::date(lang.get(), data.date)}</dd>
                </div>
                <div>
                    <dt>{tr(lang, Key::DocServiceDate)}</dt>
                    <dd>{move || i18n::date(lang.get(), data.service_date)}</dd>
                </div>
                {data.due.map(|due| view! {
                    <div>
                        <dt>{tr(lang, Key::DocDue)}</dt>
                        <dd class="dotted">
                            <span>{move || i18n::date(lang.get(), due)}</span>
                            {days_left.map(|days| view! {
                                <span>{move || days_text(lang.get(), days)}</span>
                            })}
                        </dd>
                    </div>
                })}
                <div>
                    <dt>{tr(lang, Key::DocLanguage)}</dt>
                    <dd lang=data.lang.code()>{data.lang.native_name()}</dd>
                </div>
            </dl>
            {basis.map(|number| view! {
                <p class="hint">{move || fill(t(lang.get(), Key::DocBasis), &[&number])}</p>
            })}
        </div>
        {(awaiting && left.is_some() && ctx.today.is_some()).then(|| view! {
            <button type="button" class="wide" disabled=move || busy.get() on:click=mark_paid>
                {icon(Icon::Check)}
                <span>{tr(lang, Key::DocMarkPaid)}</span>
            </button>
        })}
        <div class="actions doc-next">
            {(invoice && open).then(|| view! {
                <button
                    type="button"
                    class="secondary"
                    on:click=move |_| {
                        notice.set(None);
                        mode.set(Mode::ActFor(id));
                    }
                >
                    {tr(lang, Key::DocActFromInvoice)}
                </button>
            })}
            <button
                type="button"
                class="secondary"
                aria-expanded=move || paper.get().to_string()
                on:click=move |_| paper.update(|shown| *shown = !*shown)
            >
                {icon(Icon::Download)}
                <span>{tr(lang, Key::DocPdf)}</span>
            </button>
        </div>
        {move || paper.get().then(|| view! {
            <PaperPanel lang base=ctx.base request=request.clone() close=move || paper.set(false) />
        })}
    }
}

/// Курс ЦБ и вид деятельности выпущенного документа — для книги и налога.
/// У счёта с действующим актом доход — по акту (`ledger::income_documents`):
/// курс и вид у такого счёта ни на что не влияют, и карточка говорит, что
/// они у акта, а не предлагает их (дефект 0.0.27–0.0.47: «Указать курс ЦБ»
/// был и у такого счёта, и курс молча не доходил до книги).
#[component]
fn ForBook(
    ctx: Ctx,
    document: Document,
    busy: RwSignal<bool>,
    failure: RwSignal<Option<Problem>>,
) -> impl IntoView {
    let lang = ctx.lang;
    let data = document.data();
    let live = document.annulled().is_none();
    let foreign = data.currency != Currency::Amd && live;
    let service = data.service_date;
    let covered = covered_by_act(ctx, &document);
    let needed = Memo::new(move |_| ctx.profiles.with(|profile| turnover::activity_needed(profile, service)));
    move || {
        if covered.get() {
            view! {
                {move || (foreign || needed.get()).then(|| view! {
                    <div class="rate-box">
                        <p class="hint">{tr(lang, Key::DocIncomeByAct)}</p>
                    </div>
                })}
            }
            .into_any()
        } else {
            view! {
                {foreign.then(|| view! { <DocRate ctx document=document.clone() busy failure /> })}
                {live.then(|| view! { <DocActivity ctx document=document.clone() busy failure /> })}
            }
            .into_any()
        }
    }
}

/// Закрыт ли счёт действующим актом: доход тогда — по акту
/// (`ledger::income_documents`), и курс с видом деятельности у счёта ни на
/// что не влияют. Пока документы читаются — нет. Отслеживаемое.
fn covered_by_act(ctx: Ctx, document: &Document) -> Memo<bool> {
    let id = document.id();
    let invoice = document.data().kind == DocKind::Invoice;
    let live = document.issued().is_some() && document.annulled().is_none() && !document.is_deleted();
    Memo::new(move |_| {
        invoice
            && live
            && ctx.documents.with(|documents, _, _| {
                Some(ledger::income_documents(documents).iter().any(|document| document.id() == id))
            }) == Some(false)
    })
}

/// Курс документа в валюте для книги (задача 14): «Указать курс ЦБ» или
/// «Изменить курс» — полем по нажатию; итог в драмах — в карточке, под
/// итогом. В PDF курса нет: выпущенный документ от него не меняется
/// (`Document::set_rate`).
#[component]
fn DocRate(
    ctx: Ctx,
    document: Document,
    busy: RwSignal<bool>,
    failure: RwSignal<Option<Problem>>,
) -> impl IntoView {
    let Ctx { lang, notice, .. } = ctx;
    let id = document.id();
    let data = document.data().clone();
    let rate = data.rate;
    let editing = RwSignal::new(false);
    let typed = RwSignal::new(
        rate.map(|rate| i18n::rate(lang.get_untracked(), rate.per_unit_e4()))
            .unwrap_or_default(),
    );
    let tried = RwSignal::new(false);
    // Сменили язык окна — курс в поле записывается по-новому (`retype`).
    Effect::watch(
        move || lang.get(),
        move |now: &Lang, before: Option<&Lang>, _| {
            if let Some(&before) = before {
                retype(typed, |text| i18n::parse_rate(before, text), |rate| i18n::rate(*now, rate));
            }
        },
        false,
    );
    let save = move |_| {
        tried.set(true);
        let text = typed.get_untracked();
        let per_unit_e4 = if text.trim().is_empty() {
            None
        } else {
            match i18n::parse_rate(lang.get_untracked(), &text) {
                Ok(rate) if rate > 0 => Some(rate),
                _ => return,
            }
        };
        act(ctx, busy, failure, id, Change::SetRate(per_unit_e4), move |_| {
            notice.set(Some(Notice::RateSaved));
        });
    };
    let currency = data.currency;
    let service = data.service_date;
    let total = data.total().ok();
    view! {
        <div class="rate-box">
            {move || if editing.get() {
                view! {
                    <label>
                        {move || fill(t(lang.get(), Key::OpRate), &[currency_code(currency)])}
                        <input type="text" inputmode="decimal" autocomplete="off" maxlength="24" bind:value=typed />
                    </label>
                    <p class="hint">{tr(lang, Key::RateHint)}</p>
                    {shown_problem(tried, move || typed.with(|text| rate_problem(lang.get(), text)))}
                    <p class="hint">{move || {
                        let lang = lang.get();
                        typed.with(|text| equivalent(lang, total, text, Some(service)))
                    }}</p>
                    <div class="actions">
                        <button type="button" disabled=move || busy.get() on:click=save>
                            {tr(lang, Key::DocRateSave)}
                        </button>
                        <button type="button" class="secondary" on:click=move |_| editing.set(false)>
                            {tr(lang, Key::Cancel)}
                        </button>
                    </div>
                }
                .into_any()
            } else {
                view! {
                    <button type="button" class="secondary" on:click=move |_| editing.set(true)>
                        {tr(lang, if rate.is_some() { Key::DocRateChange } else { Key::DocRateSet })}
                    </button>
                }
                .into_any()
            }}
        </div>
    }
}

/// Вид деятельности выпущенного документа (задача 19): у ИП, которому он
/// нужен (`turnover::activity_needed`), — выбором и «Сохранить», как курс.
/// В PDF его нет: выпущенный документ от него не меняется
/// (`Document::set_activity`). У счёта с действующим актом его нет вовсе
/// (`ForBook`).
#[component]
fn DocActivity(
    ctx: Ctx,
    document: Document,
    busy: RwSignal<bool>,
    failure: RwSignal<Option<Problem>>,
) -> impl IntoView {
    let Ctx { lang, notice, .. } = ctx;
    let id = document.id();
    let saved = document.data().activity;
    let service = document.data().service_date;
    let chosen = RwSignal::new(saved);
    let date = RwSignal::new(service.iso());
    let tried = RwSignal::new(false);
    let activities = Memo::new(move |_| ctx.profiles.activities());
    let needed = Memo::new(move |_| ctx.profiles.with(|profile| turnover::activity_needed(profile, service)));
    let save = move |_| {
        tried.set(true);
        let activity = chosen.get_untracked();
        if activity.is_none() {
            return;
        }
        act(ctx, busy, failure, id, Change::SetActivity(activity), move |_| {
            notice.set(Some(Notice::ActivitySaved));
        });
    };
    move || {
        (needed.get() || saved.is_some()).then(|| {
            view! {
                <div class="rate-box">
                    <IncomeActivity lang activities chosen date tried required=needed />
                    {move || (chosen.get() != saved).then(|| view! {
                        <button type="button" disabled=move || busy.get() on:click=save>
                            {tr(lang, Key::DocActivitySave)}
                        </button>
                    })}
                </div>
            }
        })
    }
}

/// Сколько дней до срока словами: «сегодня», «5 дней».
fn days_text(lang: Lang, days: i32) -> String {
    if days == 0 {
        return t(lang, Key::DueToday).to_owned();
    }
    fill(
        i18n::plural(lang, u64::from(days.unsigned_abs()), Key::DaysOne, Key::DaysFew, Key::DaysMany),
        &[&i18n::integer(lang, i64::from(days))],
    )
}

/// Строки документа и итог — на языке окна; суммы — записью `i18n::money`.
#[component]
fn DocItems(ctx: Ctx, document: Document) -> impl IntoView {
    let lang = ctx.lang;
    let data = document.data().clone();
    let currency = data.currency;
    let total = document.total().ok();
    view! {
        <h2>{tr(lang, Key::DocItems)}</h2>
        <ul class="items">
            {data.items.iter().map(|item| {
                let text = item.text.as_str().to_owned();
                let item = item.clone();
                view! {
                    <li>
                        <p>{text}</p>
                        <p class="hint">{move || {
                            let lang = lang.get();
                            fill(
                                t(lang, Key::ItemLine),
                                &[
                                    &i18n::decimal(lang, item.quantity_e3, QUANTITY_DIGITS),
                                    &i18n::money(lang, Money::new(item.price, currency)),
                                    &money_text(lang, item.total(currency).ok()),
                                ],
                            )
                        }}</p>
                    </li>
                }
            }).collect_view()}
        </ul>
        <p class="total">{move || {
            let lang = lang.get();
            fill(t(lang, Key::DocTotal), &[&money_text(lang, total)])
        }}</p>
    }
}

/// Событие в истории документа.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Event {
    Issued,
    Paid(i64),
    Signed,
    Annulled,
}

/// Что было с документом — свежее сверху (дизайн «Kvartik Design»): выпуск,
/// поступления оплаты, подпись акта клиентом, аннулирование. Дата выпуска —
/// дата документа: момент выпуска программа не хранит, а в документе
/// написана она.
#[component]
fn History(ctx: Ctx, document: Document) -> impl IntoView {
    let lang = ctx.lang;
    let number = document.number().map(|number| number.to_string()).unwrap_or_default();
    let currency = document.data().currency;
    // В один день: аннулирование — позже оплат и подписи, они — позже
    // выпуска.
    let mut events = vec![(document.data().date, 0, Event::Issued)];
    events.extend(document.payments().iter().map(|payment| (payment.date, 1, Event::Paid(payment.amount))));
    if let Some(day) = document.signed() {
        events.push((day, 1, Event::Signed));
    }
    if let Some(day) = document.annulled() {
        events.push((day, 2, Event::Annulled));
    }
    events.sort_by_key(|&(day, order, _)| std::cmp::Reverse((day, order)));
    view! {
        <h2>{tr(lang, Key::HistoryTitle)}</h2>
        <ol class="history">
            {events.into_iter().map(|(day, _, event)| {
                let number = number.clone();
                let text = move || {
                    let lang = lang.get();
                    match event {
                        Event::Issued => fill(t(lang, Key::HistoryIssued), &[&number]),
                        Event::Paid(amount) => fill(
                            t(lang, Key::HistoryPaid),
                            &[&i18n::money(lang, Money::new(amount, currency))],
                        ),
                        Event::Signed => t(lang, Key::HistorySigned).to_owned(),
                        Event::Annulled => t(lang, Key::StatusAnnulled).to_owned(),
                    }
                };
                view! {
                    <li class:muted=event == Event::Annulled>
                        <span>{text}</span>
                        <span class="hint">{move || i18n::date(lang.get(), day)}</span>
                    </li>
                }
            }).collect_view()}
        </ol>
    }
}

/// Оплата счёта: поступления, сколько осталось и «Отметить оплату».
#[component]
fn Payments(
    ctx: Ctx,
    document: Document,
    busy: RwSignal<bool>,
    failure: RwSignal<Option<Problem>>,
) -> impl IntoView {
    let Ctx { lang, notice, .. } = ctx;
    let id = document.id();
    let currency = document.data().currency;
    let open = document.annulled().is_none();
    let left = document.left().ok();
    let date = RwSignal::new(ctx.today.map(Date::iso).unwrap_or_default());
    let amount = RwSignal::new(
        left.filter(|left| left.minor() > 0)
            .map(|left| i18n::amount(lang.get_untracked(), left))
            .unwrap_or_default(),
    );
    let tried = RwSignal::new(false);
    // Сменили язык окна — сумма в поле записывается по-новому (`retype`).
    Effect::watch(
        move || lang.get(),
        move |now: &Lang, before: Option<&Lang>, _| {
            if let Some(&before) = before {
                retype(
                    amount,
                    |text| i18n::parse_money(before, text, currency),
                    |sum| i18n::amount(*now, sum),
                );
            }
        },
        false,
    );
    let payments = document.payments().to_vec();
    let record = move |_| {
        tried.set(true);
        let payment = Date::from_iso(&date.get_untracked()).and_then(|day| {
            let sum = i18n::parse_money(lang.get_untracked(), &amount.get_untracked(), currency).ok()?;
            (sum.minor() > 0).then_some(Payment {
                date: day,
                amount: sum.minor(),
            })
        });
        if let Some(payment) = payment {
            act(ctx, busy, failure, id, Change::AddPayment(payment), move |_| {
                notice.set(Some(Notice::Paid));
            });
        }
    };
    view! {
        <fieldset class="payments">
            <legend>{tr(lang, Key::PaymentsTitle)}</legend>
            <p class="hint">{tr(lang, Key::PaymentsAbout)}</p>
            {if payments.is_empty() {
                view! { <p>{tr(lang, Key::PaymentNone)}</p> }.into_any()
            } else {
                view! {
                    <ul class="items">
                        {payments.into_iter().map(|payment| view! {
                            <li class="payment">
                                <p>{move || {
                                    let lang = lang.get();
                                    fill(
                                        t(lang, Key::PaymentLine),
                                        &[
                                            &i18n::date(lang, payment.date),
                                            &i18n::money(lang, Money::new(payment.amount, currency)),
                                        ],
                                    )
                                }}</p>
                                <button
                                    type="button"
                                    class="secondary"
                                    disabled=move || busy.get()
                                    on:click=move |_| act(ctx, busy, failure, id, Change::RemovePayment(payment), |_| {})
                                >
                                    {tr(lang, Key::Remove)}
                                </button>
                            </li>
                        }).collect_view()}
                    </ul>
                }
                .into_any()
            }}
            {left.filter(|left| left.minor() > 0).map(|left| view! {
                <p class="total">{move || fill(t(lang.get(), Key::PaymentLeft), &[&i18n::money(lang.get(), left)])}</p>
            })}
            {open.then(|| view! {
                <DateField lang label=Key::PaymentDate value=date tried />
                <TextField
                    lang
                    label=Key::PaymentAmount
                    value=amount
                    tried
                    inputmode="decimal"
                    autocomplete="off"
                    max=32
                    check=|_| None
                />
                {shown_problem(tried, move || {
                    let lang = lang.get();
                    match amount.with(|text| i18n::parse_money(lang, text, currency)) {
                        Ok(sum) if sum.minor() > 0 => None,
                        Ok(_) => Some(t(lang, Key::PaymentInvalid).to_owned()),
                        Err(error) => {
                            let example = i18n::amount(lang, Money::new(150_000, currency));
                            Some(number_problem(lang, error, &example))
                        }
                    }
                })}
                <button type="button" disabled=move || busy.get() on:click=record>
                    {tr(lang, Key::PaymentAdd)}
                </button>
            })}
        </fieldset>
    }
}

/// Подпись акта клиентом (задача 62): пока подписи нет — день подписи
/// (сегодня) и «Отметить, что подписан»; у подписанного — его день
/// и «Снять отметку». Аннулированный акт показывает подпись, если она была,
/// а действий над ним нет.
#[component]
fn Signature(
    ctx: Ctx,
    document: Document,
    busy: RwSignal<bool>,
    failure: RwSignal<Option<Problem>>,
) -> impl IntoView {
    let Ctx { lang, notice, .. } = ctx;
    let id = document.id();
    let open = document.annulled().is_none();
    let signed = document.signed();
    if signed.is_none() && !open {
        return None;
    }
    let act_date = document.data().date;
    let date = RwSignal::new(ctx.today.map(Date::iso).unwrap_or_default());
    let tried = RwSignal::new(false);
    let mark = move |_| {
        tried.set(true);
        let Some(day) = Date::from_iso(&date.get_untracked()).filter(|day| *day >= act_date) else {
            return;
        };
        act(ctx, busy, failure, id, Change::Sign(day), move |_| {
            notice.set(Some(Notice::Signed));
        });
    };
    let unsign = move |_| {
        act(ctx, busy, failure, id, Change::Unsign, move |_| {
            notice.set(Some(Notice::Unsigned));
        });
    };
    Some(view! {
        <fieldset class="signature">
            <legend>{tr(lang, Key::SignTitle)}</legend>
            {match signed {
                Some(day) => view! {
                    <p>{move || fill(t(lang.get(), Key::SignedOn), &[&i18n::date(lang.get(), day)])}</p>
                    {open.then(|| view! {
                        <button type="button" class="secondary" disabled=move || busy.get() on:click=unsign>
                            {tr(lang, Key::SignRemove)}
                        </button>
                    })}
                }
                .into_any(),
                None => view! {
                    <p class="hint">{tr(lang, Key::SignAbout)}</p>
                    <DateField lang label=Key::SignDate value=date tried />
                    {shown_problem(tried, move || {
                        date.with(|text| Date::from_iso(text))
                            .filter(|day| *day < act_date)
                            .map(|_| t(lang.get(), Key::SignBeforeDate).to_owned())
                    })}
                    <button type="button" disabled=move || busy.get() on:click=mark>
                        {tr(lang, Key::SignMark)}
                    </button>
                }
                .into_any(),
            }}
        </fieldset>
    })
}

/// Редкие действия с выпущенным документом — внизу карточки, под «Ещё»:
/// повтор счёта и аннулирование. Акт по счёту — в карточке, рядом с главным
/// действием.
#[component]
fn DocActions(
    ctx: Ctx,
    document: Document,
    busy: RwSignal<bool>,
    failure: RwSignal<Option<Problem>>,
) -> impl IntoView {
    let Ctx {
        lang,
        documents,
        notice,
        ..
    } = ctx;
    if document.annulled().is_some() {
        return None;
    }
    let id = document.id();
    let today = ctx.today_or(document.data().date);
    let named = title(lang.get_untracked(), &document);
    let invoice = document.data().kind == DocKind::Invoice;
    let repeat = document.clone();
    let recurring = Memo::new(move |_| {
        documents.with(|_, recurrings, _| Recurring::of(recurrings, id).filter(|r| !r.is_stopped()).cloned())
    });
    let annul = move |_| {
        let named = named.clone();
        act(ctx, busy, failure, id, Change::Annul(today), move |_| {
            notice.set(Some(Notice::Annulled(id, named)));
        });
    };
    let start = move |_| {
        if busy.get_untracked() {
            return;
        }
        let Some(store) = ctx.base.get() else {
            return;
        };
        busy.set(true);
        failure.set(None);
        leptos::task::spawn_local(async move {
            let outcome = start_repeat(&store, id, today).await;
            busy.set(false);
            match outcome {
                Ok((recurring, drafts)) => {
                    documents.created.set(drafts.len());
                    documents.put_recurring(recurring);
                    for draft in drafts {
                        documents.put(draft);
                    }
                    ctx.backups.data_saved();
                }
                Err(problem) => failure.set(Some(problem)),
            }
        });
    };
    let stop = move |recurring: Recurring| {
        if busy.get_untracked() {
            return;
        }
        let Some(store) = ctx.base.get() else {
            return;
        };
        busy.set(true);
        failure.set(None);
        leptos::task::spawn_local(async move {
            let outcome = stop_repeat(&store, recurring.id()).await;
            busy.set(false);
            match outcome {
                Ok(Some(stopped)) => documents.put_recurring(stopped),
                Ok(None) => {}
                Err(problem) => failure.set(Some(problem)),
            }
        });
    };
    Some(view! {
        <h2>{tr(lang, Key::DocMoreActions)}</h2>
        {invoice.then(|| view! {
            <div class="repeat">
                {move || match recurring.get() {
                    Some(active) => {
                        let next = active.next_date(&repeat);
                        view! {
                            <p>{move || next.map(|next| fill(t(lang.get(), Key::RecurActive), &[&i18n::date(lang.get(), next)]))}</p>
                            <button
                                type="button"
                                class="secondary"
                                disabled=move || busy.get()
                                on:click=move |_| stop(active.clone())
                            >
                                {tr(lang, Key::RecurStop)}
                            </button>
                        }
                        .into_any()
                    }
                    None => view! {
                        <p class="hint">{tr(lang, Key::RecurAbout)}</p>
                        <button type="button" class="secondary" disabled=move || busy.get() on:click=start>
                            {tr(lang, Key::RecurStart)}
                        </button>
                    }
                    .into_any(),
                }}
            </div>
        })}
        <div class="actions">
            <button type="button" class="secondary" disabled=move || busy.get() on:click=annul>
                {tr(lang, Key::DocAnnul)}
            </button>
        </div>
    })
}

/// Запустить повтор счёта и сразу создать черновик, если его месяц уже
/// наступил, — одной транзакцией.
async fn start_repeat(
    store: &Store,
    source: RecordId,
    today: Date,
) -> Result<(Recurring, Vec<Document>), Problem> {
    let tx = store.write()?;
    let document = tx.get::<Document>(source).await?.ok_or(StoreError::Failed)?;
    let recurrings = tx.list::<Recurring>().await?.records;
    let existing = Recurring::of(&recurrings, source);
    let now = clock::now();
    let started = match Recurring::start(existing, &document, today, now)? {
        Some(started) => started,
        // Уже повторяется — его запустила другая вкладка.
        None => existing.cloned().ok_or(StoreError::Failed)?,
    };
    let client_alive = tx
        .get::<Client>(document.data().client)
        .await?
        .is_some_and(|client| !client.is_deleted());
    let mut writes = Writes::default();
    let (recurring, drafts) = match started.generate(&document, client_alive, today, now) {
        Some((recurring, drafts)) => (recurring, drafts),
        None => (started, Vec::new()),
    };
    if existing != Some(&recurring) {
        match existing {
            None => writes.insert(&recurring),
            Some(_) => writes.update(&recurring),
        }
    }
    for draft in &drafts {
        writes.insert(draft);
    }
    tx.commit(writes).await?;
    Ok((recurring, drafts))
}

/// Остановить повтор — одной транзакцией. `Ok(None)` — уже остановлен.
async fn stop_repeat(store: &Store, id: RecordId) -> Result<Option<Recurring>, Problem> {
    let tx = store.write()?;
    let Some(recurring) = tx.get::<Recurring>(id).await? else {
        return Ok(None);
    };
    let Some(stopped) = recurring.stop(clock::now()) else {
        return Ok(None);
    };
    let mut writes = Writes::default();
    writes.update(&stopped);
    tx.commit(writes).await?;
    Ok(Some(stopped))
}
