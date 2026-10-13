//! Раздел «Счета и акты» (задача 11): список, черновик в форме, выпуск
//! с номером, оплата, аннулирование, акт из счёта и повторяющийся счёт.
//!
//! Экрана документов ещё нет, и раздел стоит на единственном экране
//! каркаса, под клиентами. Каждое действие — команда ядра
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
use kvartik_core::model::client::Client;
use kvartik_core::model::country::Country;
use kvartik_core::model::date::Date;
use kvartik_core::model::document::{
    DocError, DocKind, Document, DocumentData, Item, Payment, QUANTITY_DIGITS, Status,
};
use kvartik_core::model::money::{Currency, Money};
use kvartik_core::model::profile::Profile;
use kvartik_core::model::record::RecordId;
use kvartik_core::model::recurring::Recurring;
use kvartik_core::model::text::Line;
use leptos::ev::SubmitEvent;
use leptos::prelude::*;

use super::backup::Backups;
use super::clients::Clients;
use super::form::{CURRENCIES, TextField, currency_code, currency_key};
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
        }
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
    /// нет — значение по умолчанию.
    fn with<T: Default>(&self, read: impl FnOnce(&[Document], &[Recurring], usize) -> T) -> T {
        self.state.with(|state| match state {
            State::Ready {
                documents,
                recurrings,
                unreadable,
            } => read(documents, recurrings, *unreadable),
            State::Loading | State::Failed(_) => T::default(),
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

/// Что показывает раздел.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    List,
    /// Новый документ этого вида.
    New(DocKind),
    /// Акт по выпущенному счёту — форма с его данными.
    ActFor(RecordId),
    /// Черновик в форме.
    Edit(RecordId),
    /// Выпущенный документ.
    View(RecordId),
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
                | DocError::TooManyPayments
                | DocError::WrongClient
                | DocError::NumbersExhausted,
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
    base: Base,
    backups: Backups,
    mode: RwSignal<Mode>,
    notice: RwSignal<Option<Notice>>,
    /// Сегодня — один раз (Архитектура, «Время — параметр»).
    today: Option<Date>,
}

impl Ctx {
    /// Сегодня, а часы без даты — дата документа: срока они не просрочат.
    fn today_or(&self, date: Date) -> Date {
        self.today.unwrap_or(date)
    }
}

/// Раздел на языке окна. Пока база открывается — пусто: «открываем…»
/// только мигнуло бы.
#[component]
pub fn DocumentsSection(
    lang: ReadSignal<Lang>,
    documents: Documents,
    clients: Clients,
    base: Base,
    backups: Backups,
) -> impl IntoView {
    let ctx = Ctx {
        lang,
        documents,
        clients,
        base,
        backups,
        mode: RwSignal::new(Mode::List),
        notice: RwSignal::new(None),
        today: clock::today(),
    };
    let phase = Memo::new(move |_| {
        documents.state.with(|state| match state {
            State::Loading => Phase::Loading,
            State::Ready { .. } => Phase::Ready,
            State::Failed(error) => Phase::Failed(*error),
        })
    });
    // Копию восстановили — документы в базе могли стать другими.
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
    move || {
        let body = match (phase.get(), ctx.mode.get()) {
            (Phase::Loading, _) => return None,
            (Phase::Failed(error), _) => view! {
                <p role="alert">{tr(lang, error_key(error))}</p>
            }
            .into_any(),
            (Phase::Ready, Mode::List) => view! { <DocList ctx /> }.into_any(),
            (Phase::Ready, Mode::New(kind)) => {
                view! { <DocForm ctx existing=None start=Start::Blank(kind, ctx.today) /> }.into_any()
            }
            (Phase::Ready, Mode::ActFor(id)) => {
                let start = ctx
                    .documents
                    .get(id)
                    .and_then(|invoice| invoice.act_data(ctx.today_or(invoice.data().date)).ok());
                match start {
                    Some(data) => {
                        view! { <DocForm ctx existing=None start=Start::Data(data) /> }.into_any()
                    }
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
            (Phase::Ready, Mode::View(id)) => view! { <DocView ctx id /> }.into_any(),
        };
        Some(view! {
            <h2>{tr(lang, Key::DocsTitle)}</h2>
            {body}
        })
    }
}

/// Слова вида документа.
const fn kind_key(kind: DocKind) -> Key {
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
fn retype<T>(
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

/// Список: новые документы, сообщения, документы по страницам, удалённые
/// черновики — по кнопке.
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
    let limit = RwSignal::new(PAGE);
    let show_deleted = RwSignal::new(false);
    let failure = RwSignal::new(None::<Problem>);
    let busy = RwSignal::new(false);
    // Производные от списка — пересчитываются, только когда меняется
    // список (Правило 1).
    let listed = Memo::new(move |_| {
        documents.with(|list, _, _| Document::listed(list).into_iter().cloned().collect::<Vec<_>>())
    });
    let page = Memo::new(move |_| {
        listed.with(|all| all.iter().take(limit.get()).cloned().collect::<Vec<_>>())
    });
    let deleted = Memo::new(move |_| {
        documents.with(|list, _, _| Document::deleted(list).into_iter().cloned().collect::<Vec<_>>())
    });
    let unreadable = Memo::new(move |_| documents.with(|_, _, unreadable| unreadable));
    let has_clients = Memo::new(move |_| !clients.alive().is_empty());

    let restore = move |id: RecordId| {
        act(ctx, busy, failure, id, Change::Restore, move |_| {
            notice.set(Some(Notice::Restored));
        });
    };
    let open = move |document: &Document| {
        notice.set(None);
        mode.set(if document.issued().is_some() {
            Mode::View(document.id())
        } else {
            Mode::Edit(document.id())
        });
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
        {move || listed.with(Vec::is_empty).then(|| view! { <p>{tr(lang, Key::DocsAbout)}</p> })}
        {move || (!has_clients.get()).then(|| view! { <p class="hint">{tr(lang, Key::DocsNeedClient)}</p> })}
        <div class="actions">
            <button
                type="button"
                disabled=move || !has_clients.get()
                on:click=move |_| {
                    notice.set(None);
                    documents.created.set(0);
                    mode.set(Mode::New(DocKind::Invoice));
                }
            >
                {tr(lang, Key::DocNewInvoice)}
            </button>
            <button
                type="button"
                class="secondary"
                disabled=move || !has_clients.get()
                on:click=move |_| {
                    notice.set(None);
                    documents.created.set(0);
                    mode.set(Mode::New(DocKind::Act));
                }
            >
                {tr(lang, Key::DocNewAct)}
            </button>
        </div>
        <NoticeLine ctx busy failure />
        {move || failure.get().map(|problem| view! { <p role="alert">{tr(lang, problem.key())}</p> })}
        <ul class="documents">
            <For each=move || page.get() key=|document| (document.id(), document.meta().modified()) let(document)>
                <DocRow ctx document open />
            </For>
        </ul>
        {move || (listed.with(Vec::len) > limit.get()).then(|| view! {
            <button type="button" class="secondary" on:click=move |_| limit.update(|limit| *limit += PAGE)>
                {tr(lang, Key::DocsMore)}
            </button>
        })}
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

/// Документ в списке: название и статус, клиент, дата и итог, «Открыть».
#[component]
fn DocRow<O>(ctx: Ctx, document: Document, open: O) -> impl IntoView
where
    O: Fn(&Document) + Copy + Send + Sync + 'static,
{
    let Ctx { lang, clients, .. } = ctx;
    let status = document.status(ctx.today_or(document.data().date));
    let (status_key, status_class) = status_look(status);
    let total = document.total().ok();
    let date = document.data().date;
    let shown = document.clone();
    let label = document.clone();
    view! {
        <li>
            <p class="doc-title">
                <span>{move || row_title(lang.get(), &shown)}</span>
                <span class=status_class>{tr(lang, status_key)}</span>
            </p>
            <p class="hint facts">
                <span>{move || client_label(lang.get(), &label, clients)}</span>
                <span>{move || i18n::date(lang.get(), date)}</span>
                <span>{move || money_text(lang.get(), total)}</span>
            </p>
            <div class="actions">
                <button type="button" class="secondary" on:click=move |_| open(&document)>
                    {tr(lang, Key::DocOpen)}
                </button>
            </div>
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
            None => vec![form.row()],
        };
        form.items.set(rows);
        form
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

    /// Данные из полей; `None` — где-то ошибка, и её видно у поля.
    fn data(&self, window: Lang) -> Option<DocumentData> {
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
        let data = DocumentData {
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
        };
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
fn number_problem(lang: Lang, error: ParseError, example: &str) -> String {
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
fn shown_problem(
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

/// Форма черновика: новый — `existing` пуст, правка — черновик.
#[component]
fn DocForm(ctx: Ctx, existing: Option<Document>, start: Start) -> impl IntoView {
    let Ctx {
        lang,
        documents,
        clients,
        mode,
        notice,
        ..
    } = ctx;
    let form = Form::new(&start, lang.get_untracked());
    let id = existing.as_ref().map(Document::id);
    let failure = RwSignal::new(None::<Problem>);
    let invalid = RwSignal::new(false);
    let tried = form.tried;
    let alive = Memo::new(move |_| clients.alive());
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

    let submit = move |issue: bool| {
        if form.busy.get_untracked() {
            return;
        }
        form.tried.set(true);
        let Some(data) = form.data(lang.get_untracked()) else {
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
                        mode.set(Mode::View(shown));
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
    let last_day = move |_| {
        let text = form.service_date.get_untracked();
        let base = Date::from_iso(&text).or_else(|| Date::from_iso(&form.date.get_untracked()));
        if let Some(day) = base {
            form.service_date.set(day.last_of_month().iso());
        }
    };
    let total = move || {
        let lang = lang.get();
        let currency = form.currency.get()?;
        let lines = form.items.with(|rows| {
            rows.iter()
                .map(|row| line_total(lang, *row, currency))
                .collect::<Option<Vec<_>>>()
        })?;
        Money::total(currency, lines)
            .ok()
            .map(|sum| fill(t(lang, Key::DocTotal), &[&i18n::money(lang, sum)]))
    };
    let full = move || form.items.with(|rows| rows.len() >= DocumentData::MAX_ITEMS);
    // Сменили язык окна — цены и количества записываются по-новому, а их
    // смысл не меняется (`retype`).
    Effect::watch(
        move || lang.get(),
        move |now: &Lang, before: Option<&Lang>, _| {
            let (Some(&before), now) = (before, *now) else {
                return;
            };
            let currency = form.currency.get_untracked();
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

    view! {
        <form class="document" on:submit=move |event: SubmitEvent| {
            event.prevent_default();
            submit(false);
        } novalidate>
            <p class="doc-title">{move || {
                let kind = t(lang.get(), kind_key(form.kind));
                fill(t(lang.get(), Key::DocDraftTitle), &[kind])
            }}</p>
            <fieldset>
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
                {move || (local() == Some(true)).then(|| view! {
                    <p class="hint">{tr(lang, Key::DocCashReceipt)}</p>
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
                <DateField lang label=Key::DocDate value=form.date tried />
                <DateField lang label=Key::DocServiceDate value=form.service_date tried />
                <p class="hint">{tr(lang, Key::DocServiceDateHint)}</p>
                <button type="button" class="secondary" on:click=last_day>
                    {tr(lang, Key::DocLastDay)}
                </button>
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
            </fieldset>
            <fieldset>
                <legend>{tr(lang, Key::DocItems)}</legend>
                <p class="hint">{tr(lang, Key::DocItemsHint)}</p>
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
                    class="secondary"
                    disabled=full
                    on:click=move |_| {
                        let row = form.row();
                        form.items.update(|rows| rows.push(row));
                    }
                >
                    {tr(lang, Key::ItemAdd)}
                </button>
                <p class="total">{total}</p>
            </fieldset>
            {move || invalid.get().then(|| view! {
                <p role="alert">{tr(lang, Key::ProfileFix)}</p>
            })}
            {move || failure.get().map(|problem| view! {
                <p role="alert">{tr(lang, problem.key())}</p>
            })}
            <p class="hint">{tr(lang, Key::DocIssueHint)}</p>
            <div class="actions">
                <button type="button" disabled=move || form.busy.get() on:click=move |_| submit(true)>
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
                    on:click=move |_| mode.set(Mode::List)
                >
                    {tr(lang, Key::Cancel)}
                </button>
            </div>
        </form>
    }
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
fn DateField(lang: ReadSignal<Lang>, label: Key, value: RwSignal<String>, tried: RwSignal<bool>) -> impl IntoView {
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

/// Выпущенный документ: реквизиты, строки, итог, оплата, действия.
/// Документ читается из списка со слежением: оплата и аннулирование
/// показываются сразу.
#[component]
fn DocView(ctx: Ctx, id: RecordId) -> impl IntoView {
    let Ctx {
        lang,
        documents,
        mode,
        ..
    } = ctx;
    let document = Memo::new(move |_| {
        documents.with(|list, _, _| list.iter().find(|d| d.id() == id).cloned())
    });
    let failure = RwSignal::new(None::<Problem>);
    let busy = RwSignal::new(false);
    move || {
        let Some(shown) = document.get() else {
            return view! { <DocList ctx /> }.into_any();
        };
        view! {
            <DocDetails ctx document=shown.clone() />
            <NoticeLine ctx busy failure />
            {move || failure.get().map(|problem| view! { <p role="alert">{tr(lang, problem.key())}</p> })}
            {(shown.data().kind == DocKind::Invoice).then(|| view! { <Payments ctx document=shown.clone() busy failure /> })}
            <DocActions ctx document=shown busy failure />
            <div class="actions">
                <button type="button" class="secondary" on:click=move |_| {
                    ctx.notice.set(None);
                    mode.set(Mode::List);
                }>
                    {tr(lang, Key::DocClose)}
                </button>
            </div>
        }
        .into_any()
    }
}

/// Реквизиты и строки выпущенного документа — на языке окна; суммы —
/// записью `i18n::money`.
#[component]
fn DocDetails(ctx: Ctx, document: Document) -> impl IntoView {
    let Ctx {
        lang,
        documents,
        clients,
        ..
    } = ctx;
    let data = document.data().clone();
    let status = document.status(ctx.today_or(data.date));
    let (status_key, status_class) = status_look(status);
    let named = document.clone();
    let client = document.clone();
    let basis = data.basis.and_then(|basis| {
        documents
            .get(basis)
            .and_then(|invoice| invoice.number())
            .map(|number| number.to_string())
    });
    let currency = data.currency;
    let total = document.total().ok();
    view! {
        <p class="doc-title">
            <span>{move || title(lang.get(), &named)}</span>
            <span class=status_class>{tr(lang, status_key)}</span>
        </p>
        <dl class="summary">
            <dt>{tr(lang, Key::DocClient)}</dt>
            <dd>{move || client_label(lang.get(), &client, clients)}</dd>
            <dt>{tr(lang, Key::DocLanguage)}</dt>
            <dd>{data.lang.native_name()}</dd>
            <dt>{tr(lang, Key::DocCurrency)}</dt>
            <dd>{currency_code(currency)}</dd>
            <dt>{tr(lang, Key::DocDate)}</dt>
            <dd>{move || i18n::date(lang.get(), data.date)}</dd>
            <dt>{tr(lang, Key::DocServiceDate)}</dt>
            <dd>{move || i18n::date(lang.get(), data.service_date)}</dd>
            {data.due.map(|due| view! {
                <dt>{tr(lang, Key::DocDue)}</dt>
                <dd>{move || i18n::date(lang.get(), due)}</dd>
            })}
        </dl>
        {basis.map(|number| view! {
            <p class="hint">{move || fill(t(lang.get(), Key::DocBasis), &[&number])}</p>
        })}
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
    let left = match (document.total(), document.paid()) {
        (Ok(total), Ok(paid)) => Some(Money::new(total.minor() - paid.minor(), currency)),
        _ => None,
    };
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

/// Действия с выпущенным документом: акт по счёту, повтор, аннулирование.
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
        mode,
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
            {invoice.then(|| view! {
                <button
                    type="button"
                    on:click=move |_| {
                        notice.set(None);
                        mode.set(Mode::ActFor(id));
                    }
                >
                    {tr(lang, Key::DocActFromInvoice)}
                </button>
            })}
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
