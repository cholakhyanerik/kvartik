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
//! Расчётный документ в системе ՊԵԿ (задача 23) человек выписывает там сам,
//! а здесь отмечает — в карточке документа дохода. Какие документы ждут
//! отметки, говорят строка над списком, список «без расчётного документа»
//! и напоминание на главном экране (`tax::settlement`); напоминание — из
//! части книги, как сроки: срок — запись файла правил (Правило 1).
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
//!
//! Файлы раздела — по экранам (задача 119): здесь состояние раздела, `Ctx`,
//! команды карточки (`act`) и общие помощники, а также обе точки разреза
//! wasm (`split!`): часть определяет код, до которого функция дотягивается,
//! а не файл. Список — `list`, форма — `form`, карточка выпущенного —
//! `card`, её оплаты, подпись и отметка ՊԵԿ — `marks`; страницы PDF —
//! `ui/paper.rs`. Соседи видят друг друга через `use super::*`.

use std::collections::BTreeMap;
use std::sync::Arc;

use kvartik_core::format::Stored;
use kvartik_core::format::accdoc::{self, AccDocError};
use kvartik_core::doc::image::Picture;
use kvartik_core::i18n::{self, Key, Lang, ParseError, fill, plural, t};
use kvartik_core::model::activity::{ActivityCode, IncomeKind};
use kvartik_core::model::bank::payable_abroad;
use kvartik_core::model::client::Client;
use kvartik_core::model::country::Country;
use kvartik_core::model::date::{Date, Quarter};
use kvartik_core::model::document::{
    DocError, DocKind, Document, DocumentData, Item, Payment, QUANTITY_DIGITS, Settlement, SettlementKind, Status,
};
use kvartik_core::model::ledger;
use kvartik_core::model::logo::{IssuedLogo, Logo};
use kvartik_core::model::money::{Currency, Money};
use kvartik_core::model::profile::Profile;
use kvartik_core::model::record::{RecordId, Timestamp};
use kvartik_core::model::recurring::Recurring;
use kvartik_core::model::text::Line;
use kvartik_core::tax::{settlement, turnover};
use leptos::ev::{MouseEvent, SubmitEvent};
use leptos::html::A;
use leptos::prelude::*;

use super::avatar::{Avatar, tone};
use super::backup::{BackupChip, Backups};
use super::cba::{self, CbaHint};
use super::clients::Clients;
use super::form::{CURRENCIES, Forms, TextField, WizardHead, currency_code, currency_key};
use super::icon::{Icon, icon};
use super::ledger::{IncomeActivity, IncomeKindChoice, equivalent, rate_problem};
use super::license::{self, IssueLocked, Licenses};
use super::paper::{PaperPanel, Request};
use super::profile::Profiles;
use super::route::{Router, Screen};
use super::split::{part, screen, split};
use super::storage::error_key;
use super::{Base, tr};
use crate::platform::clock;
use crate::platform::file::Download;
use crate::store::{Store, StoreError, Tx, Write, Writes};

mod card;
mod form;
mod list;
mod marks;

use card::*;
use form::*;
use list::*;
use marks::*;

pub use list::{Awaiting, SettlementReminder};
pub(super) use form::{DateField, number_problem, shown_problem};
// Выпуск без формы зовёт только браузерный тест документов.
#[cfg(all(test, target_arch = "wasm32"))]
pub(super) use form::save;

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
    /// Список — только документы без отметки о расчётном документе
    /// (задача 23). Тоже здесь: напоминание на главном экране открывает
    /// список уже таким (`show_unsettled`).
    unsettled: RwSignal<bool>,
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
            unsettled: RwSignal::new(false),
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

    /// Прочитаны ли документы — отслеживаемое: окно читает их последними,
    /// и первые шаги (`ui/onboarding.rs`) знают всё, когда прочитаны они.
    pub fn ready(&self) -> bool {
        self.state.with(|state| matches!(state, State::Ready { .. }))
    }

    /// Сколько документов выпущено и первый из них — отслеживаемое.
    pub fn issued(&self) -> (usize, Option<RecordId>) {
        self.with(|documents, _, _| {
            let mut issued = documents
                .iter()
                .filter(|document| !document.meta().is_deleted() && document.issued().is_some());
            let first = issued.next().map(Document::id);
            (first.map_or(0, |_| 1 + issued.count()), first)
        })
    }

    /// Показывать в списке только документы без отметки о расчётном
    /// документе — из напоминания на главном экране. Открытую форму это не
    /// закрывает: начатое в ней не пропадает, а список станет таким, когда
    /// форму закроют.
    pub fn show_unsettled(&self) {
        self.unsettled.set(true);
    }

    /// Прочитать документы и повторы: когда база открылась и когда её
    /// заменило восстановление из копии. Повторам, которым к дню `today`
    /// пора, — создать черновики: транзакция записи открывается, только
    /// когда пора. Где выпуск этого ИП выключен (задачи 41 и 116), — нет:
    /// черновик месяца создаст устройство, которое выпускает, а созданный
    /// и там, и здесь лёг бы после слияния копий вторым — и счёт за месяц
    /// выпустили бы дважды. Запись устройства — напрямую, а не сигналом
    /// настроек: этот код в главном wasm (Правило 1).
    pub async fn load(&self, store: &Store, today: Option<Date>) {
        let today = today.filter(|_| !crate::store::device::Device::new().issue_elsewhere(store.name()));
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
            // Без моментов полей, как есть: этот код — в главном wasm, а разбор
            // моментов стоил бы первому заходу 4,6 КБ после brotli (замер задачи
            // 116; Правило 1). Сдвигает повтор только устройство, которое
            // выпускает (задача 41), — двух правок счётчика с двух устройств не
            // бывает, и слияние такого повтора — прежнее, по записи целиком.
            writes.put(&recurring);
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
    /// Оплата документа убрана — с кнопкой «Вернуть» рядом: «Убрать» молчал,
    /// и над карточкой оставалось прежнее «Оплата отмечена.» (полный прогон
    /// 0.0.96).
    PaymentRemoved(RecordId, Payment),
    PaymentReturned,
    /// Курс документа сохранён (задача 14).
    RateSaved,
    /// Вид деятельности документа сохранён (задача 19).
    ActivitySaved,
    IncomeKindSaved,
    /// Акт отмечен подписанным клиентом и отметка снята (задача 62).
    Signed,
    Unsigned,
    /// Расчётный документ отмечен выписанным и отметка снята (задача 23).
    Settled,
    Unsettled,
    /// Отмечено, что вместо него выдан чек ККМ (задача 76).
    Receipted,
}

/// Что не вышло: хранилище или команда ядра.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Problem {
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
            Self::Doc(DocError::ActsOverInvoice) => Key::DocActsOverInvoice,
            Self::Doc(DocError::ReturnOverInvoice) => Key::DocReturnOverInvoice,
            Self::Doc(DocError::ActCurrency) => Key::DocActCurrency,
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
    /// Ключи лицензии: выпуск — по сроку (задача 25).
    licenses: Licenses,
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
    licenses: Licenses,
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
        licenses,
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
    // ещё ни разу не показываться. Черновиков повторов — нет, без дня: копию
    // в пустую программу кладут на второе устройство, и вопрос, выпускать ли
    // на нём (задача 41), встаёт уже после этого чтения — черновик месяца,
    // созданный до ответа «там», лёг бы после слияния вторым рядом с черновиком
    // устройства, которое выпускает (ревью задачи 41). Их создаст следующее
    // открытие программы, как всегда.
    Effect::watch(
        move || base.replacements(),
        move |_, _, _| {
            if let Some(store) = base.get() {
                ctx.mode.set(Mode::List);
                ctx.notice.set(None);
                leptos::task::spawn_local(async move { documents.load(&store, None).await });
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

/// Правка документа одним действием.
#[derive(Clone, PartialEq, Eq, Debug)]
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
    SetIncomeKind(Option<IncomeKind>),
    /// Акт подписан клиентом в этот день и отметка снята (задача 62).
    Sign(Date),
    Unsign,
    /// Расчётный документ выписан в системе ՊԵԿ и отметка снята (задача 23).
    Settle(Settlement),
    Unsettle,
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
            Ok(Some((document, invoice))) => {
                done(&document);
                ctx.documents.put(document);
                if let Some(invoice) = invoice {
                    ctx.documents.put(invoice);
                }
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
/// записать. `Ok(None)` — записывать нечего. Строка налога, указанная у акта,
/// идёт и в счёт-основание — он вторым в ответе: это одна услуга (шапка
/// `model::document`).
async fn apply(store: &Store, id: RecordId, change: Change) -> Result<Option<(Document, Option<Document>)>, Problem> {
    let tx = store.write()?;
    let Some(document) = tx.get::<Document>(id).await? else {
        return Ok(None);
    };
    let now = clock::now();
    let invoice = match (&change, document.data().basis) {
        (Change::SetIncomeKind(kind), Some(basis)) => tx
            .get::<Document>(basis)
            .await?
            .and_then(|invoice| {
                let changed = invoice.set_income_kind(*kind, now).ok().flatten()?;
                Some((invoice, changed))
            }),
        _ => None,
    };
    let changed = match change {
        Change::Annul(today) => document.annul(today, now)?,
        // Акт по счёту возвращается, только если помещается в счёт: пока он
        // был аннулирован, по счёту могли выпустить другой (задача 84).
        Change::Unannul => document.unannul(&tx.list::<Document>().await?.records, now)?,
        Change::Delete => document.delete(now)?,
        Change::Restore => document.restore(now),
        Change::AddPayment(payment) => Some(document.add_payment(payment, now)?),
        Change::RemovePayment(payment) => document.remove_payment(payment, now),
        Change::SetRate(per_unit_e4) => document.set_rate(per_unit_e4, now)?,
        Change::SetActivity(activity) => document.set_activity(activity, now)?,
        Change::SetIncomeKind(kind) => document.set_income_kind(kind, now)?,
        Change::Sign(day) => document.sign(day, now)?,
        Change::Unsign => document.unsign(now),
        Change::Settle(settlement) => document.set_settlement(settlement, now)?,
        Change::Unsettle => document.remove_settlement(now),
    };
    let Some(changed) = changed else {
        return Ok(None);
    };
    let mut writes = Writes::default();
    writes.update(&document, &changed);
    if let Some((before, invoice)) = &invoice {
        writes.update(before, invoice);
    }
    tx.commit(writes).await?;
    Ok(Some((changed, invoice.map(|(_, invoice)| invoice))))
}

split! {
    /// Выпущенный документ — своей частью wasm (`ui/split.rs`): просмотр
    /// с оплатами и действиями нужен только по «Открыть».
    pub(super) fn opened_document(ctx: Ctx, id: RecordId) -> AnyView, preload preload_opened {
        view! { <DocView ctx id /> }.into_any()
    }
}
