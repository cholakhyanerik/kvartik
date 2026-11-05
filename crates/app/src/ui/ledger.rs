//! Экран «Доходы и расходы» — книга (задача 14): квартал, итог в драмах,
//! доход из выпущенных документов и записи, которые человек вносит сам, —
//! с курсом ЦБ и файлами.
//!
//! Что считается доходом и в какой квартал он попадает, решает ядро
//! (`kvartik_core::model::ledger`); здесь — показ и форма. Каждое действие —
//! команда ядра (`Operation::save`, `delete`, `restore`) и одна транзакция
//! хранилища (CLAUDE.md, «Данные меняются одним путём»).
//!
//! Экран — своей частью wasm (`ui/split.rs`) и рисуется с первого показа,
//! а не при открытии окна: первому заходу он не нужен, а каждый экран
//! в главном wasm — около 45 КБ после brotli и четверть секунды рабочего
//! экрана на телефоне (CLAUDE.md, Правило 1; задача 61). В той же части —
//! налог (`ui/tax.rs`, задача 19): он считается из книги. Записи книги
//! читает часть, а не главный wasm ([`Operations::watch`]).
//!
//! Суммы и курс человек вводит по правилам языка окна, и окно сразу
//! показывает, что поняло (Правило 6, «Ввод суммы не угадывается»).

use std::rc::Rc;
use std::sync::Arc;

use kvartik_core::format::Stored;
use kvartik_core::i18n::{self, Key, Lang, fill, t};
use kvartik_core::model::activity::{ActivityCode, IncomeKind};
use kvartik_core::model::attachment::{self, Attachment, AttachmentRef, FileBytes, FileError, FileKind};
use kvartik_core::model::date::{Date, Quarter};
use kvartik_core::model::document::DocKind;
use kvartik_core::model::ledger::{
    self, Flow, Nature, Operation, OperationData, OperationError, Proof, Source,
};
use kvartik_core::model::money::{Currency, ExchangeRate, Lumas, Money, RateSource};
use kvartik_core::model::record::RecordId;
use kvartik_core::model::text::Line;
use kvartik_core::tax::turnover;
use leptos::ev::{Event, SubmitEvent};
use leptos::prelude::*;
use leptos::wasm_split::SplitLoaderError;
use web_sys::HtmlInputElement;

use super::backup::{self, AutoView, BackupChip, Backups};
use super::clients::Clients;
use super::documents::{DateField, Documents, kind_key, number_problem, retype, shown_problem};
use super::form::{CURRENCIES, Forms, TextField, currency_code, currency_key, optional};
use super::icon::{Icon, icon};
use super::office;
use super::profile::Profiles;
use super::route::{Router, Screen};
use super::split::{screen, split};
use super::storage::error_key;
use super::tax;
use super::{Base, tr};
use crate::platform::{clock, file, image};
use crate::platform::file::Download;
use crate::store::{Store, StoreError, Writes};

mod export;
use export::Export;

/// Сколько строк список показывает сразу и прибавляет по «Показать ещё»:
/// у торговли за квартал их сотни, а рисовать окну — только то, что видно
/// (Правило 1).
const PAGE: usize = 30;

/// Квартал, когда показать нечего и часы без даты.
const FALLBACK_QUARTER: Quarter = match Quarter::new(2026, 1) {
    Some(quarter) => quarter,
    None => panic!("первый квартал 2026 года существует"),
};

/// Больше этого выбранный файл не читается вовсе (Правило 13): фото
/// с телефона — до 20 МБ, а файл в сотни мегабайт повесил бы страницу ещё
/// до уменьшения.
const PICK_LIMIT: u64 = 50 << 20;

/// Что нужно экрану из окна.
#[derive(Clone, Copy)]
pub(super) struct Ctx {
    lang: ReadSignal<Lang>,
    documents: Documents,
    clients: Clients,
    profiles: Profiles,
    operations: Operations,
    base: Base,
    backups: Backups,
    /// Открытые формы окна: форма записи отмечается в них, пока открыта.
    forms: Forms,
    /// Сегодня — один раз (Архитектура, «Время — параметр»).
    today: Option<Date>,
    /// Снимки закрытых кварталов и квартал экрана налога — выгрузке, которая
    /// говорит о разошедшихся с поданным (задача 83). Не весь `tax::Ctx`:
    /// с ним вариант `BookView::Ledger` вырос бы вдвое.
    closings: tax::Closings,
    tax_quarter: RwSignal<Option<Quarter>>,
}

impl Ctx {
    /// Откуда налог берёт расчёт — те же документы, книга и профиль.
    fn sources(&self) -> tax::Sources {
        tax::Sources {
            documents: self.documents,
            operations: self.operations,
            profiles: self.profiles,
            closings: self.closings,
        }
    }
}

/// Экран книги на языке окна: заголовок — сразу, остальное — частью wasm
/// с первого показа (`split::screen`).
#[component]
pub fn LedgerScreen(
    lang: ReadSignal<Lang>,
    documents: Documents,
    clients: Clients,
    profiles: Profiles,
    operations: Operations,
    base: Base,
    backups: Backups,
    router: Router,
    forms: Forms,
    tax: tax::Ctx,
) -> impl IntoView {
    let ctx = Ctx {
        lang,
        documents,
        clients,
        profiles,
        operations,
        base,
        backups,
        forms,
        today: clock::today(),
        closings: tax.sources.closings,
        tax_quarter: tax.quarter,
    };
    view! {
        <h1 tabindex="-1">{tr(lang, Key::LedgerTitle)}</h1>
        <BackupChip lang backups nudge=true />
        {screen(lang, router, Screen::Ledger, move || book_part(BookView::Ledger(ctx)))}
    }
}

/// Что рисует часть книги.
#[derive(Clone, Copy)]
pub(super) enum BookView {
    /// Содержимое экрана книги.
    Ledger(Ctx),
    /// Карточка налога на главном экране или содержимое экрана налога.
    Tax(tax::Ctx, tax::Part),
    /// Автокопия на экране копии (`ui/backup.rs`, задача 29).
    Auto(AutoView),
    /// Полоса открытого ИП и экран списка ИП (`ui/office.rs`, задача 38):
    /// разметка у них общая с экранами, и своя часть умножила бы общие куски
    /// (CLAUDE.md, Правило 6).
    Office(office::OfficeView),
}

split! {
    /// Экран книги и налог — одной частью wasm (`ui/split.rs`). Налог
    /// считается из книги, а своей частью он делил общий код почти со всеми
    /// частями: правила — с формами, книгу — с копией, — и программа разреза
    /// завела бы 28 лишних кусков (замер 0.0.47; CLAUDE.md, Правило 6).
    /// Автокопия на экране копии — тоже здесь: своя часть у неё завела 13
    /// лишних кусков (замер задачи 29).
    pub(super) fn book_part(view: BookView) -> AnyView, preload preload_book {
        match view {
            BookView::Ledger(ctx) => view! { <LedgerBody ctx /> }.into_any(),
            BookView::Tax(ctx, part) => tax::content(ctx, part),
            BookView::Auto(view) => backup::auto_view(view),
            BookView::Office(view) => office::view(view),
        }
    }
}

/// Налог — карточку или экран — из части книги (`ui/tax.rs`). В тестах
/// функция части видна только своему модулю (`split!`).
pub(super) async fn tax_part(ctx: tax::Ctx, part: tax::Part) -> Result<AnyView, SplitLoaderError> {
    book_part(BookView::Tax(ctx, part)).await
}

/// Автокопия на экране копии — из части книги.
pub(super) async fn auto_part(view: AutoView) -> Result<AnyView, SplitLoaderError> {
    book_part(BookView::Auto(view)).await
}

/// Полоса открытого ИП и экран списка ИП — из части книги.
pub(super) async fn office_view(view: office::OfficeView) -> Result<AnyView, SplitLoaderError> {
    book_part(BookView::Office(view)).await
}

/// Где записи книги.
#[derive(Clone, PartialEq, Debug)]
enum Book {
    /// База ещё открывается или записи читаются.
    Loading,
    /// Прочитаны все записи, включая удалённые, и сколько не читается.
    Ready {
        operations: Arc<Vec<Operation>>,
        unreadable: usize,
    },
    Failed(StoreError),
}

/// Что показывает экран.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    List,
    New(Flow),
    Edit(RecordId),
}

/// Сообщение о сделанном — над списком.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Notice {
    Saved,
    /// Удалена — с кнопкой «Восстановить» рядом.
    Deleted(RecordId),
    Restored,
}

/// Что не вышло: хранилище или команда ядра.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Problem {
    Store(StoreError),
    Operation(OperationError),
}

impl From<StoreError> for Problem {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

impl From<OperationError> for Problem {
    fn from(error: OperationError) -> Self {
        Self::Operation(error)
    }
}

impl Problem {
    /// Слова для человека. Ошибки команды, до которых форма не пускает, —
    /// общими словами хранилища: это сбой, а не ошибка человека.
    const fn key(self) -> Key {
        match self {
            Self::Store(error) => error_key(error),
            Self::Operation(
                OperationError::NoText
                | OperationError::Amount
                | OperationError::Rate
                | OperationError::WrongFlow
                | OperationError::Attachments,
            ) => Key::StoreFailed,
        }
    }
}

/// Записи книги — одни на окно (задача 19): их показывает экран книги,
/// а налог из них считают карточка главного экрана и экран налога.
#[derive(Clone, Copy)]
pub struct Operations {
    book: RwSignal<Book>,
    /// Следит ли уже кто-то за базой (`watch`).
    watched: StoredValue<bool>,
}

impl Operations {
    pub fn new() -> Self {
        Self {
            book: RwSignal::new(Book::Loading),
            watched: StoredValue::new(false),
        }
    }

    /// Читать записи книги, когда база открылась и когда её заменило
    /// восстановление из копии. Зовут части, которым нужна книга, — экран
    /// книги и налог (`ui/tax.rs`), — а следит та, что позвала первой: она
    /// живёт до конца окна. В главном wasm чтения книги нет — ни байтов, ни
    /// работы до рабочего экрана (замер 0.0.47, docs/measurements.md).
    pub(super) fn watch(&self, base: Base) {
        if self.watched.get_value() {
            return;
        }
        self.watched.set_value(true);
        let operations = *self;
        Effect::watch(
            move || (base.is_open(), base.replacements()),
            move |(open, _), _, _| {
                if !*open {
                    return;
                }
                if let Some(store) = base.get() {
                    leptos::task::spawn_local(async move { operations.load(&store).await });
                }
            },
            true,
        );
    }

    /// Прочитать записи книги. Вложения не читаются: их читает форма.
    async fn load(&self, store: &Store) {
        let read = async { store.read()?.list::<Operation>().await };
        self.book.set(match read.await {
            Ok(listed) => Book::Ready {
                operations: Arc::new(listed.records),
                unreadable: listed.unreadable,
            },
            Err(error) => Book::Failed(error),
        });
    }

    /// Записанная запись — на место прежней или в конец списка.
    fn put(&self, operation: Operation) {
        self.book.update(|book| {
            if let Book::Ready { operations, .. } = book {
                let list = Arc::make_mut(operations);
                match list.iter_mut().find(|known| known.id() == operation.id()) {
                    Some(known) => *known = operation,
                    None => list.push(operation),
                }
            }
        });
    }

    /// Запись по идентификатору — без слежения: форма открывается один раз.
    fn get(&self, id: RecordId) -> Option<Operation> {
        self.book.with_untracked(|book| match book {
            Book::Ready { operations, .. } => operations.iter().find(|o| o.id() == id).cloned(),
            Book::Loading | Book::Failed(_) => None,
        })
    }

    /// Что-то, посчитанное по записям; пока их нет — значение по
    /// умолчанию. Отслеживаемое.
    pub(super) fn with<T: Default>(&self, read: impl FnOnce(&[Operation], usize) -> T) -> T {
        self.book.with(|book| match book {
            Book::Ready {
                operations,
                unreadable,
            } => read(operations, *unreadable),
            Book::Loading | Book::Failed(_) => T::default(),
        })
    }

    /// Почему записи не прочитались — отслеживаемое.
    pub(super) fn failure(&self) -> Option<StoreError> {
        self.book.with(|book| match book {
            Book::Failed(error) => Some(*error),
            Book::Loading | Book::Ready { .. } => None,
        })
    }

    /// Стадия чтения — отслеживаемое.
    fn phase(&self) -> Phase {
        self.book.with(|book| match book {
            Book::Loading => Phase::Loading,
            Book::Ready { .. } => Phase::Ready,
            Book::Failed(error) => Phase::Failed(*error),
        })
    }
}

/// Состояние экрана — у его частей общее.
#[derive(Clone, Copy)]
struct Ledger {
    ctx: Ctx,
    mode: RwSignal<Mode>,
    notice: RwSignal<Option<Notice>>,
}

impl Ledger {
    async fn load(&self, store: &Store) {
        self.ctx.operations.load(store).await;
    }

    fn put(&self, operation: Operation) {
        self.ctx.operations.put(operation);
    }

    fn get(&self, id: RecordId) -> Option<Operation> {
        self.ctx.operations.get(id)
    }

    fn with<T: Default>(&self, read: impl FnOnce(&[Operation], usize) -> T) -> T {
        self.ctx.operations.with(read)
    }
}

/// Стадия чтения — без самих записей: экран перерисовывается целиком,
/// только когда она меняется.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Phase {
    Loading,
    Ready,
    Failed(StoreError),
}

/// Содержимое экрана: список квартала или форма записи.
#[component]
fn LedgerBody(ctx: Ctx) -> impl IntoView {
    let ledger = Ledger {
        ctx,
        mode: RwSignal::new(Mode::List),
        notice: RwSignal::new(None),
    };
    // Книгу и снимки закрытых кварталов (выгрузка, задача 83) — читать.
    // Отсюда, а не из выгрузки: следит тот, кто позвал первым, и он обязан
    // жить до конца окна (`tax::Closings::watch`), а список с выгрузкой
    // пересоздаётся при каждой форме и восстановлении — его слежение умерло
    // бы с ним, и восстановленные снимки не перечитались бы (ревью задачи 83).
    ctx.sources().watch(ctx.base);
    // Копию восстановили — открытая форма показала бы прежнюю запись:
    // экран возвращается к списку, а записи перечитывает `Operations::watch`.
    Effect::watch(
        move || ctx.base.replacements(),
        move |_, _, _| {
            ledger.mode.set(Mode::List);
            ledger.notice.set(None);
        },
        false,
    );
    let phase = Memo::new(move |_| ctx.operations.phase());
    move || match (phase.get(), ledger.mode.get()) {
        (Phase::Loading, _) => None,
        (Phase::Failed(error), _) => Some(
            view! { <p role="alert">{tr(ctx.lang, error_key(error))}</p> }.into_any(),
        ),
        (Phase::Ready, Mode::List) => Some(view! { <LedgerList ledger /> }.into_any()),
        (Phase::Ready, Mode::New(flow)) => Some(view! { <OpForm ledger existing=None flow /> }.into_any()),
        (Phase::Ready, Mode::Edit(id)) => Some(match ledger.get(id) {
            Some(existing) => {
                let flow = existing.data().flow;
                view! { <OpForm ledger existing=Some(existing) flow /> }.into_any()
            }
            None => view! { <LedgerList ledger /> }.into_any(),
        }),
    }
}

/// Строка книги для показа — собранная из документа или записи один раз,
/// а слова — на языке окна при показе.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
struct Shown {
    id: RecordId,
    origin: Origin,
    flow: Flow,
    date: Date,
    amount: Money,
    /// Сумма в драмах с лумами; `None` — валюта без курса.
    amd: Option<Lumas>,
}

/// Откуда строка.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
enum Origin {
    /// Выпущенный документ: вид, номер и клиент из копии реквизитов.
    Document {
        kind: DocKind,
        number: String,
        client: String,
    },
    /// Запись: за что, клиент или продавец, документ продавца и число
    /// файлов.
    Operation {
        text: String,
        client: Option<RecordId>,
        supplier: String,
        proof: Option<Proof>,
        files: usize,
    },
}

impl Shown {
    fn from_row(row: &ledger::Row<'_>) -> Self {
        let origin = match row.source {
            Source::Document(document) => Origin::Document {
                kind: document.data().kind,
                number: document.number().map(|number| number.to_string()).unwrap_or_default(),
                client: document
                    .issued()
                    .map(|issued| issued.buyer().name.as_str().to_owned())
                    .unwrap_or_default(),
            },
            Source::Operation(operation) => {
                let data = operation.data();
                Origin::Operation {
                    text: data.text.as_str().to_owned(),
                    client: data.client,
                    supplier: data.supplier.as_str().to_owned(),
                    proof: data.proof,
                    files: data.attachments.len(),
                }
            }
        };
        Self {
            id: row.id(),
            origin,
            flow: row.flow,
            date: row.date,
            amount: row.amount,
            amd: row.lumas().ok(),
        }
    }
}

/// Квартал книги: строки и итог.
#[derive(Clone, PartialEq, Debug, Default)]
struct Quarterly {
    lines: Vec<Shown>,
    /// Итог; `None` — переполнение от испорченных данных.
    totals: Option<ledger::Totals>,
}

/// Какие строки показать: все, доходы или расходы.
const FILTERS: [(Option<Flow>, Key); 3] = [
    (None, Key::FilterAll),
    (Some(Flow::Income), Key::LedgerIncomes),
    (Some(Flow::Expense), Key::LedgerExpenses),
];

/// Список квартала: переключатель квартала, итог в драмах, «Добавить
/// доход» и «Добавить расход», строки по месяцам — свежие сверху,
/// удалённые записи — по кнопке.
#[component]
fn LedgerList(ledger: Ledger) -> impl IntoView {
    let Ledger { ctx, mode, notice, .. } = ledger;
    let lang = ctx.lang;
    let busy = RwSignal::new(false);
    let failure = RwSignal::new(None::<Problem>);
    let flow = RwSignal::new(None::<Flow>);
    let limit = RwSignal::new(PAGE);
    let show_deleted = RwSignal::new(false);
    // Квартал — сегодняшний; часы без даты — квартал самой свежей строки.
    let chosen = RwSignal::new(ctx.today.map(Date::quarter));
    let quarter = Memo::new(move |_| {
        chosen.get().unwrap_or_else(|| {
            let newest = ctx.documents.with(|documents, _, _| {
                ledger.with(|operations, _| {
                    ledger::income_documents(documents)
                        .iter()
                        .map(|document| document.data().service_date)
                        .chain(operations.iter().map(|operation| operation.data().date))
                        .max()
                })
            });
            newest.map_or(FALLBACK_QUARTER, Date::quarter)
        })
    });
    // Пересчитывается, только когда меняются документы, записи или квартал
    // (Правило 1).
    let quarterly = Memo::new(move |_| {
        let quarter = quarter.get();
        ctx.documents.with(|documents, _, _| {
            ledger.with(|operations, _| {
                let rows = ledger::rows(documents, operations, quarter);
                Quarterly {
                    lines: rows.iter().map(Shown::from_row).collect(),
                    totals: ledger::totals(&rows).ok(),
                }
            })
        })
    });
    // Строки выбранного вида; месяцы первой страницы — по порядку, у каждого
    // его строки.
    let filtered = Memo::new(move |_| {
        let shown_flow = flow.get();
        quarterly.with(|quarterly| {
            quarterly
                .lines
                .iter()
                .filter(|line| shown_flow.is_none_or(|flow| line.flow == flow))
                .cloned()
                .collect::<Vec<_>>()
        })
    });
    let months = Memo::new(move |_| {
        let limit = limit.get();
        filtered.with(|lines| {
            let mut months: Vec<((u16, u8), Vec<Shown>)> = Vec::new();
            for line in lines.iter().take(limit) {
                let month = (line.date.year(), line.date.month());
                match months.last_mut() {
                    Some((last, list)) if *last == month => list.push(line.clone()),
                    _ => months.push((month, vec![line.clone()])),
                }
            }
            months
        })
    });
    let deleted = Memo::new(move |_| {
        ledger.with(|operations, _| Operation::deleted(operations).into_iter().cloned().collect::<Vec<_>>())
    });
    let unreadable = Memo::new(move |_| ledger.with(|_, unreadable| unreadable));
    let this_year = move || ctx.today.map_or_else(|| quarter.get().year(), Date::year);
    let step = move |next: bool| {
        let now = quarter.get_untracked();
        let moved = if next { now.next() } else { now.previous() };
        if let Some(moved) = moved {
            chosen.set(Some(moved));
            limit.set(PAGE);
        }
    };
    let start = move |flow: Flow| {
        notice.set(None);
        mode.set(Mode::New(flow));
    };
    let restore = move |id: RecordId| {
        act(ledger, busy, failure, id, Change::Restore, move || notice.set(Some(Notice::Restored)));
    };

    view! {
        {move || (unreadable.get() > 0).then(|| view! {
            <p role="alert">
                {move || {
                    let lang = lang.get();
                    fill(t(lang, Key::LedgerUnreadable), &[&i18n::integer(lang, unreadable.get() as i64)])
                }}
            </p>
        })}
        {move || notice.get().map(|shown| match shown {
            Notice::Saved => view! { <p role="status">{tr(lang, Key::LedgerSaved)}</p> }.into_any(),
            Notice::Restored => view! { <p role="status">{tr(lang, Key::LedgerRestoredNotice)}</p> }.into_any(),
            Notice::Deleted(id) => view! {
                <div class="notice-row">
                    <p role="status">{tr(lang, Key::LedgerDeletedNotice)}</p>
                    <button type="button" class="secondary" disabled=move || busy.get() on:click=move |_| restore(id)>
                        {tr(lang, Key::ClientRestore)}
                    </button>
                </div>
            }
            .into_any(),
        })}
        {move || failure.get().map(|problem| view! { <p role="alert">{tr(lang, problem.key())}</p> })}
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
        <div class="ledger-card">
            <dl class="pairs">
                <div>
                    <dt>{tr(lang, Key::LedgerIncome)}</dt>
                    <dd class="ledger-sum">{move || {
                        let lang = lang.get();
                        quarterly.with(|q| q.totals.map(|totals| i18n::lumas(lang, totals.income)))
                    }}</dd>
                </div>
                <div>
                    <dt>{tr(lang, Key::LedgerExpenses)}</dt>
                    <dd class="ledger-sum">{move || {
                        let lang = lang.get();
                        quarterly.with(|q| q.totals.map(|totals| i18n::lumas(lang, totals.expense)))
                    }}</dd>
                </div>
            </dl>
            {move || {
                let unrated = quarterly.with(|q| q.totals.map_or(0, |totals| totals.unrated));
                (unrated > 0).then(|| view! {
                    <p role="alert">{move || {
                        let lang = lang.get();
                        fill(t(lang, Key::LedgerUnrated), &[&i18n::integer(lang, unrated as i64)])
                    }}</p>
                })
            }}
        </div>
        <div class="actions ledger-add">
            <button type="button" on:click=move |_| start(Flow::Income)>
                {icon(Icon::Plus)}
                <span>{tr(lang, Key::LedgerAddIncome)}</span>
            </button>
            <button type="button" class="secondary" on:click=move |_| start(Flow::Expense)>
                {icon(Icon::Plus)}
                <span>{tr(lang, Key::LedgerAddExpense)}</span>
            </button>
        </div>
        <p class="hint">{tr(lang, Key::LedgerAbout)}</p>
        <div class="seg" role="group" aria-label=move || t(lang.get(), Key::LedgerFilter)>
            {FILTERS
                .into_iter()
                .map(|(value, key)| view! {
                    <button
                        type="button"
                        aria-pressed=move || (flow.get() == value).to_string()
                        on:click=move |_| {
                            flow.set(value);
                            limit.set(PAGE);
                        }
                    >
                        {tr(lang, key)}
                    </button>
                })
                .collect_view()}
        </div>
        {move || months.with(Vec::is_empty).then(|| view! {
            <p class="hint">{tr(lang, Key::LedgerEmpty)}</p>
        })}
        <For each=move || months.with(|months| months.iter().map(|(month, _)| *month).collect::<Vec<_>>()) key=|month| *month let(month)>
            <h2 class="month">{move || i18n::month_heading(lang.get(), month.0, month.1, this_year())}</h2>
            <ul class="list ledger">
                <For
                    each=move || months.with(|months| {
                        months.iter().find(|(known, _)| *known == month).map(|(_, list)| list.clone()).unwrap_or_default()
                    })
                    key=|line| line.clone()
                    let(line)
                >
                    <LineEntry ledger line />
                </For>
            </ul>
        </For>
        {move || (filtered.with(Vec::len) > limit.get()).then(|| view! {
            <button type="button" class="secondary" on:click=move |_| limit.update(|limit| *limit += PAGE)>
                {tr(lang, Key::DocsMore)}
            </button>
        })}
        <Show when=move || !deleted.with(Vec::is_empty)>
            <button
                type="button"
                class="secondary"
                aria-expanded=move || show_deleted.get().to_string()
                on:click=move |_| show_deleted.update(|open| *open = !*open)
            >
                {move || {
                    let lang = lang.get();
                    if show_deleted.get() {
                        t(lang, Key::LedgerDeletedHide).to_owned()
                    } else {
                        let count = deleted.with(Vec::len) as i64;
                        fill(t(lang, Key::LedgerDeleted), &[&i18n::integer(lang, count)])
                    }
                }}
            </button>
            <Show when=move || show_deleted.get()>
                <ul class="documents deleted">
                    <For each=move || deleted.get() key=|operation| (operation.id(), operation.meta().modified()) let(operation)>
                        <li>
                            <p class="doc-title">{operation.data().text.as_str().to_owned()}</p>
                            <p class="hint">{
                                let amount = operation.data().amount;
                                move || i18n::money(lang.get(), amount)
                            }</p>
                            <div class="actions">
                                <button
                                    type="button"
                                    class="secondary"
                                    disabled=move || busy.get()
                                    on:click={
                                        let id = operation.id();
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
        <Export ctx quarter />
    }
}

/// Слова документа продавца.
const fn proof_key(proof: Proof) -> Key {
    match proof {
        Proof::TaxInvoice => Key::ProofTaxInvoice,
        Proof::SettlementInvoice => Key::ProofSettlementInvoice,
        Proof::CashReceipt => Key::ProofCashReceipt,
        Proof::IndividualPurchase => Key::ProofIndividualPurchase,
        Proof::Contract => Key::ProofContract,
        Proof::ForeignSupplier => Key::ProofForeignSupplier,
        Proof::BankPayment => Key::ProofBankPayment,
        Proof::None => Key::ProofNone,
    }
}

/// Слова вида расхода.
const fn nature_key(nature: Nature) -> Key {
    match nature {
        Nature::Ordinary => Key::NatureOrdinary,
        Nature::Levy => Key::NatureLevy,
        Nature::Asset => Key::NatureAsset,
        Nature::Travel => Key::NatureTravel,
        Nature::Hospitality => Key::NatureHospitality,
    }
}

/// Строка книги (дизайн «Kvartik Design», как строки документов): кто или
/// за что, вид и дата слева, сумма в валюте и в драмах справа. Доход
/// и расход различает слово, а не цвет (Правило 4). Строка документа —
/// ссылка на его карточку, строка записи открывает форму.
#[component]
fn LineEntry(ledger: Ledger, line: Shown) -> impl IntoView {
    let Ledger { ctx, mode, notice, .. } = ledger;
    let lang = ctx.lang;
    let Shown {
        id,
        origin,
        flow,
        date,
        amount,
        amd,
    } = line;
    let foreign = amount.currency() != Currency::Amd;
    let side = view! {
        <span class="entry-side">
            <span class="entry-amount">{move || i18n::money(lang.get(), amount)}</span>
            {match (foreign, amd) {
                (false, _) => None,
                (true, Some(amd)) => Some(view! {
                    <span class="hint">{move || i18n::lumas(lang.get(), amd)}</span>
                }
                .into_any()),
                (true, None) => Some(view! {
                    <span class="status waiting">{tr(lang, Key::LedgerNoRate)}</span>
                }
                .into_any()),
            }}
        </span>
    };
    let when = move || i18n::date(lang.get(), date);
    match origin {
        Origin::Document { kind, number, client } => view! {
            <li>
                <a class="entry" href=Screen::Document(id).href()>
                    <span class="entry-main">
                        <span class="entry-title">{client}</span>
                        <span class="entry-meta">
                            <span>{move || {
                                let lang = lang.get();
                                fill(t(lang, Key::DocNumbered), &[t(lang, kind_key(kind)), &number])
                            }}</span>
                            <span>{when}</span>
                        </span>
                    </span>
                    {side}
                </a>
            </li>
        }
        .into_any(),
        Origin::Operation {
            text,
            client,
            supplier,
            proof,
            files,
        } => {
            let who = match flow {
                Flow::Income => client.and_then(|client| ctx.clients.name(client)).map(|(name, _)| name),
                Flow::Expense => (!supplier.is_empty()).then_some(supplier),
            };
            view! {
                <li>
                    <button
                        type="button"
                        class="entry"
                        on:click=move |_| {
                            notice.set(None);
                            mode.set(Mode::Edit(id));
                        }
                    >
                        <span class="entry-main">
                            <span class="entry-title">{text}</span>
                            <span class="entry-meta">
                                <span>{move || {
                                    let lang = lang.get();
                                    match (flow, proof) {
                                        (Flow::Income, _) => t(lang, Key::LedgerIncome).to_owned(),
                                        (Flow::Expense, Some(proof)) => {
                                            format!("{} · {}", t(lang, Key::LedgerExpense), t(lang, proof_key(proof)))
                                        }
                                        (Flow::Expense, None) => t(lang, Key::LedgerExpense).to_owned(),
                                    }
                                }}</span>
                                {who.map(|who| view! { <span>{who}</span> })}
                                <span>{when}</span>
                                {(files > 0).then(|| view! {
                                    <span>{move || {
                                        let lang = lang.get();
                                        fill(t(lang, Key::LedgerFiles), &[&i18n::integer(lang, files as i64)])
                                    }}</span>
                                })}
                            </span>
                        </span>
                        {side}
                    </button>
                </li>
            }
            .into_any()
        }
    }
}

/// Правка записи одним действием.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Change {
    Delete,
    Restore,
}

/// Сделать `change` с записью `id` и, если записано, — `done`. Пока идёт
/// одно действие, второе не начинается.
fn act(
    ledger: Ledger,
    busy: RwSignal<bool>,
    failure: RwSignal<Option<Problem>>,
    id: RecordId,
    change: Change,
    done: impl FnOnce() + 'static,
) {
    if busy.get_untracked() {
        return;
    }
    let Some(store) = ledger.ctx.base.get() else {
        failure.set(Some(Problem::Store(StoreError::Failed)));
        return;
    };
    busy.set(true);
    failure.set(None);
    leptos::task::spawn_local(async move {
        let outcome = apply(&store, id, change).await;
        busy.set(false);
        match outcome {
            Ok(Some(operation)) => {
                done();
                ledger.put(operation);
                ledger.ctx.backups.data_saved();
            }
            // Запись уже в этом состоянии или пропала — её поменяла другая
            // вкладка: окно показывает, что в базе.
            Ok(None) => ledger.load(&store).await,
            Err(problem) => failure.set(Some(problem)),
        }
    });
}

/// Удалить или восстановить запись — одной транзакцией. Вложения остаются
/// при ней.
async fn apply(store: &Store, id: RecordId, change: Change) -> Result<Option<Operation>, Problem> {
    let tx = store.write()?;
    let Some(operation) = tx.get::<Operation>(id).await? else {
        return Ok(None);
    };
    let now = clock::now();
    let changed = match change {
        Change::Delete => operation.delete(now),
        Change::Restore => operation.restore(now),
    };
    let Some(changed) = changed else {
        return Ok(None);
    };
    let mut writes = Writes::default();
    writes.update(&changed);
    tx.commit(writes).await?;
    Ok(Some(changed))
}

/// Файл в форме: прежний — по ссылке, новый — с байтами.
#[derive(Clone, PartialEq, Debug)]
struct FileRow {
    /// Ключ строки — свой до конца формы.
    key: u32,
    name: String,
    kind: FileKind,
    size: u32,
    /// Прежнее вложение — его ссылка.
    saved: Option<AttachmentRef>,
    /// Новый файл — байты; ложатся в базу при сохранении записи.
    fresh: Option<FileBytes>,
}

/// Почему выбранный файл не приложен.
#[derive(Clone, PartialEq, Eq, Debug)]
enum FileProblem {
    TooLarge(String),
    NotSupported(String),
    Unreadable(String),
    Full,
}

impl FileProblem {
    fn text(&self, lang: Lang) -> String {
        let megabytes = i18n::integer(lang, (attachment::MAX_BYTES >> 20) as i64);
        match self {
            Self::TooLarge(name) => fill(t(lang, Key::OpFileTooLarge), &[name, &megabytes]),
            Self::NotSupported(name) => fill(t(lang, Key::OpFileNotSupported), &[name]),
            Self::Unreadable(name) => fill(t(lang, Key::OpFileUnreadable), &[name]),
            Self::Full => fill(
                t(lang, Key::OpFilesFull),
                &[&i18n::integer(lang, OperationData::MAX_ATTACHMENTS as i64)],
            ),
        }
    }
}

/// Байты выбранного файла для вложения: PDF — как есть, фото — уменьшенное
/// (`platform::image`), — и его имя: перекодированное в JPEG фото меняет
/// расширение, иначе файл «IMG_2041.HEIC» с байтами JPEG сбил бы с толку
/// и человека, и систему, которая его откроет.
async fn prepare(chosen: &web_sys::File) -> Result<(String, FileBytes), FileProblem> {
    let name = chosen.name();
    let pdf = chosen.type_() == FileKind::Pdf.mime();
    if pdf && chosen.size() > attachment::MAX_BYTES as f64 {
        return Err(FileProblem::TooLarge(name));
    }
    let bytes = match file::read_bytes(chosen, PICK_LIMIT).await {
        Ok(bytes) => bytes,
        Err(file::FileError::TooLarge) => return Err(FileProblem::TooLarge(name)),
        Err(file::FileError::NotText | file::FileError::Unreadable) => {
            return Err(FileProblem::Unreadable(name));
        }
    };
    let (name, bytes) = match FileKind::sniff(&bytes) {
        Some(FileKind::Pdf) => (name, bytes),
        Some(kind @ (FileKind::Jpeg | FileKind::Png)) => {
            let shrunk = image::shrink(&bytes, kind.mime(), true)
                .await
                .map_err(|_| FileProblem::Unreadable(name.clone()))?;
            (jpeg_name(&name, shrunk.reencoded), shrunk.bytes)
        }
        // Фото другого вида (HEIC, WebP) — если браузер его открывает,
        // ложится JPEG.
        None if chosen.type_().starts_with("image/") => {
            let shrunk = image::shrink(&bytes, &chosen.type_(), false)
                .await
                .map_err(|_| FileProblem::NotSupported(name.clone()))?;
            (jpeg_name(&name, shrunk.reencoded), shrunk.bytes)
        }
        None => return Err(FileProblem::NotSupported(name)),
    };
    match FileBytes::new(bytes) {
        Ok(file) => Ok((name, file)),
        Err(FileError::TooLarge) => Err(FileProblem::TooLarge(name)),
        Err(FileError::NotSupported) => Err(FileProblem::NotSupported(name)),
    }
}

/// Имя перекодированного в JPEG фото — с расширением `.jpg`.
fn jpeg_name(name: &str, reencoded: bool) -> String {
    if !reencoded {
        return name.to_owned();
    }
    let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
    format!("{stem}.jpg")
}

/// Поля формы — строками, как их ввёл человек; читает их ядро.
#[derive(Clone, Copy)]
struct Form {
    flow: Flow,
    text: RwSignal<String>,
    client: RwSignal<Option<RecordId>>,
    supplier: RwSignal<String>,
    /// Даты — записью ISO, как их отдаёт `<input type="date">`.
    date: RwSignal<String>,
    paid: RwSignal<String>,
    currency: RwSignal<Currency>,
    amount: RwSignal<String>,
    rate: RwSignal<String>,
    proof: RwSignal<Option<Proof>>,
    nature: RwSignal<Nature>,
    activity: RwSignal<Option<ActivityCode>>,
    income_kind: RwSignal<Option<IncomeKind>>,
    files: RwSignal<Vec<FileRow>>,
    next_file: StoredValue<u32>,
    tried: RwSignal<bool>,
    busy: RwSignal<bool>,
}

impl Form {
    /// Форма записи или пустая: дата — сегодня, валюта — драмы. Суммы —
    /// записью языка окна: её человек и правит.
    fn new(flow: Flow, existing: Option<&OperationData>, today: Option<Date>, window: Lang) -> Self {
        let form = Self {
            flow,
            text: RwSignal::new(String::new()),
            client: RwSignal::new(None),
            supplier: RwSignal::new(String::new()),
            date: RwSignal::new(today.map(Date::iso).unwrap_or_default()),
            paid: RwSignal::new(String::new()),
            currency: RwSignal::new(Currency::Amd),
            amount: RwSignal::new(String::new()),
            rate: RwSignal::new(String::new()),
            proof: RwSignal::new(None),
            nature: RwSignal::new(Nature::Ordinary),
            activity: RwSignal::new(None),
            income_kind: RwSignal::new(None),
            files: RwSignal::new(Vec::new()),
            next_file: StoredValue::new(0),
            tried: RwSignal::new(false),
            busy: RwSignal::new(false),
        };
        if let Some(data) = existing {
            form.text.set(data.text.as_str().to_owned());
            form.client.set(data.client);
            form.supplier.set(data.supplier.as_str().to_owned());
            form.date.set(data.date.iso());
            form.paid.set(data.paid.map(Date::iso).unwrap_or_default());
            form.currency.set(data.amount.currency());
            form.amount.set(i18n::amount(window, data.amount));
            form.rate
                .set(data.rate.map(|rate| i18n::rate(window, rate.per_unit_e4())).unwrap_or_default());
            form.proof.set(data.proof);
            form.nature.set(data.nature.unwrap_or(Nature::Ordinary));
            form.activity.set(data.activity);
            form.income_kind.set(data.income_kind);
            let rows = data
                .attachments
                .iter()
                .map(|saved| FileRow {
                    key: form.key(),
                    name: saved.name.as_str().to_owned(),
                    kind: saved.kind,
                    size: saved.size,
                    saved: Some(saved.clone()),
                    fresh: None,
                })
                .collect();
            form.files.set(rows);
        }
        form
    }

    fn key(&self) -> u32 {
        let key = self.next_file.get_value();
        self.next_file.set_value(key + 1);
        key
    }

    /// Сумма из поля; `None` — не прочиталась.
    fn amount(&self, window: Lang) -> Option<Money> {
        i18n::parse_money(window, &self.amount.get_untracked(), self.currency.get_untracked()).ok()
    }

    /// Данные из полей и новые файлы; `None` — где-то ошибка, и её видно
    /// у поля. `needed` — нужен ли доходу вид деятельности
    /// (`turnover::activity_needed`).
    fn data(&self, window: Lang, needed: bool) -> Option<(OperationData, Vec<(String, FileBytes)>)> {
        let text = Line::new(&self.text.get_untracked()).ok().filter(|text| !text.is_empty())?;
        let amount = self.amount(window).filter(|amount| amount.minor() > 0)?;
        let paid = optional(&self.paid.get_untracked(), |text| Date::from_iso(text).ok_or(()))?;
        let rows = self.files.get_untracked();
        let mut data = OperationData {
            flow: self.flow,
            date: Date::from_iso(&self.date.get_untracked())?,
            paid,
            amount,
            rate: None,
            text,
            client: None,
            supplier: Line::default(),
            proof: None,
            nature: None,
            activity: None,
            income_kind: None,
            attachments: rows.iter().filter_map(|row| row.saved.clone()).collect(),
        };
        match self.flow {
            Flow::Income => {
                data.client = self.client.get_untracked();
                data.activity = self.activity.get_untracked();
                data.income_kind = self.income_kind.get_untracked();
                // Доходу со своей строкой код не нужен (`tax::turnover`).
                if needed && data.activity.is_none() && data.income_kind.is_none() {
                    return None;
                }
            }
            Flow::Expense => {
                data.supplier = Line::new(&self.supplier.get_untracked()).ok()?;
                data.proof = Some(self.proof.get_untracked()?);
                data.nature = Some(self.nature.get_untracked());
                data.activity = self.activity.get_untracked();
            }
        }
        if amount.currency() != Currency::Amd {
            let typed = self.rate.get_untracked();
            if !typed.trim().is_empty() {
                let per_unit_e4 = i18n::parse_rate(window, &typed).ok()?;
                data.rate = Some(data.rate_for(per_unit_e4).ok()?);
            }
        }
        let fresh = rows
            .into_iter()
            .filter_map(|row| row.fresh.map(|file| (row.name, file)))
            .collect();
        data.check().ok()?;
        Some((data, fresh))
    }
}

/// Что не так с курсом: не прочитался или не больше нуля. Пусто — не
/// ошибка: курса ещё нет (Правило 6). Общая у формы книги и документа.
pub(super) fn rate_problem(lang: Lang, text: &str) -> Option<String> {
    if text.trim().is_empty() {
        return None;
    }
    match i18n::parse_rate(lang, text) {
        Ok(rate) if rate > 0 => None,
        Ok(_) => Some(t(lang, Key::RateZero).to_owned()),
        Err(error) => Some(number_problem(lang, error, &i18n::rate(lang, 3_880_000))),
    }
}

/// Что не так с суммой.
fn amount_problem(lang: Lang, text: &str, currency: Currency) -> Option<String> {
    match i18n::parse_money(lang, text, currency) {
        Ok(sum) if sum.minor() > 0 => None,
        Ok(_) => Some(t(lang, Key::OpAmountZero).to_owned()),
        Err(error) => {
            let example = i18n::amount(lang, Money::new(150_000, currency));
            Some(number_problem(lang, error, &example))
        }
    }
}

/// Сумма в драмах и курс парой — под суммой формы, как в дизайне «Kvartik
/// Design» (экран 04): `amount` — сумма из поля, `rate` — курс из поля,
/// `date` — день, для которого курс. `None` — сумма в драмах или чего-то
/// не хватает. Общая у формы книги и формы документа.
pub(super) fn equivalent(lang: Lang, amount: Option<Money>, rate: &str, date: Option<Date>) -> Option<String> {
    let amount = amount?;
    let per_unit_e4 = i18n::parse_rate(lang, rate).ok()?;
    let rate = ExchangeRate::new(amount.currency(), per_unit_e4, date?, RateSource::Manual).ok()?;
    let amd = amount.to_lumas(Some(rate)).ok()?;
    Some(fill(
        t(lang, Key::RateEquivalent),
        &[
            &i18n::lumas(lang, amd),
            &i18n::rate_pair(lang, amount.currency(), per_unit_e4),
        ],
    ))
}

/// Форма записи — одной страницей: дохода или расхода, новой или прежней.
#[component]
fn OpForm(ledger: Ledger, existing: Option<Operation>, flow: Flow) -> impl IntoView {
    let Ledger { ctx, mode, notice, .. } = ledger;
    ctx.forms.hold();
    let lang = ctx.lang;
    let form = Form::new(flow, existing.as_ref().map(Operation::data), ctx.today, lang.get_untracked());
    let id = existing.as_ref().map(Operation::id);
    let tried = form.tried;
    let failure = RwSignal::new(None::<Problem>);
    let invalid = RwSignal::new(false);
    let preparing = RwSignal::new(false);
    let file_problems = RwSignal::new(Vec::<FileProblem>::new());
    // Ссылки на файлы — адреса `blob:`: у прежних — когда прочитаны, у
    // новых — сразу. Живут, пока жива форма.
    let links = RwSignal::new_local(Vec::<(u32, Rc<Download>)>::new());
    let alive = Memo::new(move |_| ctx.clients.alive());
    let activities = Memo::new(move |_| ctx.profiles.activities());
    // Нужен ли доходу вид деятельности на день записи (задача 19).
    let needed = Memo::new(move |_| {
        let day = form.date.with(|text| Date::from_iso(text)).or(ctx.today);
        day.is_some_and(|day| ctx.profiles.with(|profile| turnover::activity_needed(profile, day)))
    });
    let required = Memo::new(move |_| needed.get() && form.income_kind.get().is_none());
    // Спросить ли у расхода вид и при одном коде (задача 70).
    let beside = Memo::new(move |_| {
        form.date.with(|text| Date::from_iso(text)).is_some_and(|day| {
            ctx.documents.with(|documents, _, _| {
                ctx.operations.with(|operations, _| {
                    ctx.profiles.with(|profile| turnover::expense_activity_needed(documents, operations, profile, day))
                })
            })
        })
    });

    // Прежние вложения — прочитать и дать на них ссылки. Не прочиталось —
    // файл в списке без ссылки: убрать его можно и так.
    if let (Some(store), Some(existing)) = (ctx.base.get(), existing.clone()) {
        let saved: Vec<(u32, RecordId)> = form
            .files
            .with_untracked(|rows| rows.iter().filter_map(|row| Some((row.key, row.saved.as_ref()?.id))).collect());
        if !saved.is_empty() {
            leptos::task::spawn_local(async move {
                let Ok(tx) = store.read() else {
                    return;
                };
                for (key, file) in saved {
                    if let Ok(Some(found)) = tx.get::<Attachment>(file).await
                        && found.owner() == existing.id()
                        && let Some(download) = Download::bytes(
                            found.file().bytes(),
                            found.name().as_str(),
                            found.file().kind().mime(),
                        )
                    {
                        links.update(|links| links.push((key, Rc::new(download))));
                    }
                }
            });
        }
    }

    let pick = move |event: Event| {
        let input: HtmlInputElement = event_target(&event);
        let chosen = file::chosen_all(&input);
        // Тот же файл можно будет выбрать снова: без сброса браузер не
        // сообщит о повторном выборе. Выбранные файлы уже в руках.
        input.set_value("");
        if chosen.is_empty() || preparing.get_untracked() {
            return;
        }
        preparing.set(true);
        file_problems.set(Vec::new());
        leptos::task::spawn_local(async move {
            let mut problems = Vec::new();
            for chosen in chosen {
                if form.files.with_untracked(Vec::len) >= OperationData::MAX_ATTACHMENTS {
                    problems.push(FileProblem::Full);
                    break;
                }
                match prepare(&chosen).await {
                    Ok((name, bytes)) => {
                        let key = form.key();
                        if let Some(download) = Download::bytes(bytes.bytes(), &name, bytes.kind().mime()) {
                            links.update(|links| links.push((key, Rc::new(download))));
                        }
                        let row = FileRow {
                            key,
                            name,
                            kind: bytes.kind(),
                            size: u32::try_from(bytes.bytes().len()).unwrap_or(u32::MAX),
                            saved: None,
                            fresh: Some(bytes),
                        };
                        form.files.update(|rows| rows.push(row));
                    }
                    Err(problem) => problems.push(problem),
                }
            }
            preparing.set(false);
            file_problems.set(problems);
        });
    };

    let submit = move |event: SubmitEvent| {
        event.prevent_default();
        if form.busy.get_untracked() || preparing.get_untracked() {
            return;
        }
        form.tried.set(true);
        let Some((data, fresh)) = form.data(lang.get_untracked(), needed.get_untracked()) else {
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
            let outcome = save(&store, id, data, fresh).await;
            form.busy.set(false);
            match outcome {
                Ok(operation) => {
                    ledger.put(operation);
                    ctx.backups.data_saved();
                    notice.set(Some(Notice::Saved));
                    mode.set(Mode::List);
                }
                Err(problem) => failure.set(Some(problem)),
            }
        });
    };
    let delete = move |_| {
        if let Some(id) = id {
            act(ledger, form.busy, failure, id, Change::Delete, move || {
                notice.set(Some(Notice::Deleted(id)));
                mode.set(Mode::List);
            });
        }
    };
    let leave = move |_| mode.set(Mode::List);

    // Сменили язык окна — сумма и курс записываются по-новому, а их смысл не
    // меняется (`retype`).
    Effect::watch(
        move || lang.get(),
        move |now: &Lang, before: Option<&Lang>, _| {
            let (Some(&before), now) = (before, *now) else {
                return;
            };
            let currency = form.currency.get_untracked();
            retype(
                form.amount,
                |text| i18n::parse_money(before, text, currency),
                |sum| i18n::amount(now, sum),
            );
            retype(form.rate, |text| i18n::parse_rate(before, text), |rate| i18n::rate(now, rate));
        },
        false,
    );

    let heading = match (flow, existing.is_some()) {
        (Flow::Income, false) => Key::OpNewIncome,
        (Flow::Expense, false) => Key::OpNewExpense,
        (Flow::Income, true) => Key::LedgerIncome,
        (Flow::Expense, true) => Key::LedgerExpense,
    };
    let (date_label, date_hint) = match flow {
        Flow::Income => (Key::DocServiceDate, Key::DocServiceDateHint),
        Flow::Expense => (Key::OpReceived, Key::OpReceivedHint),
    };
    let text_hint = match flow {
        Flow::Income => Key::OpTextIncomeHint,
        Flow::Expense => Key::OpTextExpenseHint,
    };
    let foreign = move || form.currency.get() != Currency::Amd;

    view! {
        <form class="operation" on:submit=submit novalidate>
            <p class="doc-title">{tr(lang, heading)}</p>
            <TextField lang label=Key::OpText value=form.text tried hint=text_hint autocomplete="off" />
            {shown_problem(tried, move || {
                form.text.with(|text| text.trim().is_empty()).then(|| t(lang.get(), Key::OpTextMissing).to_owned())
            })}
            {match flow {
                Flow::Income => view! {
                    <label>
                        {tr(lang, Key::DocClient)}
                        <select on:change=move |event| {
                            let chosen = event_target_value(&event);
                            let client = alive.with_untracked(|list| {
                                list.iter().find(|c| c.id().to_string() == chosen).cloned()
                            });
                            if let Some(currency) = client.as_ref().and_then(|c| c.data().currency)
                                && form.amount.with_untracked(String::is_empty)
                            {
                                form.currency.set(currency);
                            }
                            form.client.set(client.map(|c| c.id()));
                        }>
                            <option value="" prop:selected=move || form.client.get().is_none()>
                                {tr(lang, Key::OpNoClient)}
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
                    // Вид деятельности дохода — когда без него не узнать
                    // строку налога или режим (задача 19).
                    {move || (required.get() || form.activity.get().is_some()).then(|| view! {
                        <IncomeActivity
                            lang
                            activities
                            chosen=form.activity
                            date=form.date
                            tried
                            required
                        />
                    })}
                    <IncomeKindChoice lang profiles=ctx.profiles chosen=form.income_kind date=form.date />
                }
                .into_any(),
                Flow::Expense => view! {
                    <TextField lang label=Key::OpSupplier value=form.supplier tried hint=Key::OpSupplierHint autocomplete="off" />
                }
                .into_any(),
            }}
            <DateField lang label=date_label value=form.date tried />
            <p class="hint">{tr(lang, date_hint)}</p>
            <label>
                {tr(lang, Key::OpPaid)}
                <input type="date" bind:value=form.paid />
            </label>
            <label>
                {tr(lang, Key::DocCurrency)}
                <select on:change=move |event| {
                    let code = event_target_value(&event);
                    if let Some(currency) = CURRENCIES.into_iter().find(|c| currency_code(*c) == code) {
                        form.currency.set(currency);
                    }
                }>
                    {CURRENCIES
                        .into_iter()
                        .map(|currency| view! {
                            <option value=currency_code(currency) prop:selected=move || form.currency.get() == currency>
                                {tr(lang, currency_key(currency))}
                            </option>
                        })
                        .collect_view()}
                </select>
            </label>
            <TextField
                lang
                label=Key::OpAmount
                value=form.amount
                tried
                inputmode="decimal"
                autocomplete="off"
                max=32
                check=|_| None
            />
            {shown_problem(tried, move || {
                form.amount.with(|text| amount_problem(lang.get(), text, form.currency.get()))
            })}
            <Show when=foreign>
                <label>
                    {move || {
                        let lang = lang.get();
                        fill(t(lang, Key::OpRate), &[currency_code(form.currency.get())])
                    }}
                    <input type="text" inputmode="decimal" autocomplete="off" maxlength="24" bind:value=form.rate />
                </label>
                <p class="hint">{tr(lang, Key::RateHint)}</p>
                {shown_problem(tried, move || form.rate.with(|text| rate_problem(lang.get(), text)))}
            </Show>
            <p class="hint">{move || {
                let lang = lang.get();
                let amount = form.amount.with(|text| i18n::parse_money(lang, text, form.currency.get()).ok());
                let date = form.date.with(|text| Date::from_iso(text));
                form.rate.with(|rate| equivalent(lang, amount, rate, date))
            }}</p>
            {(flow == Flow::Expense).then(|| view! {
                <label>
                    {tr(lang, Key::OpProof)}
                    <select on:change=move |event| {
                        let chosen = event_target_value(&event);
                        form.proof.set(Proof::ALL.into_iter().find(|proof| proof_code(*proof) == chosen));
                    }>
                        <option value="" prop:selected=move || form.proof.get().is_none()>
                            {tr(lang, Key::NotChosen)}
                        </option>
                        {Proof::ALL
                            .into_iter()
                            .map(|proof| view! {
                                <option value=proof_code(proof) prop:selected=move || form.proof.get() == Some(proof)>
                                    {tr(lang, proof_key(proof))}
                                </option>
                            })
                            .collect_view()}
                    </select>
                </label>
                <p class="hint">{tr(lang, Key::ProofHint)}</p>
                {shown_problem(tried, move || {
                    form.proof.get().is_none().then(|| t(lang.get(), Key::OpProofMissing).to_owned())
                })}
                <label>
                    {tr(lang, Key::OpNature)}
                    <select on:change=move |event| {
                        let chosen = event_target_value(&event);
                        if let Some(nature) = Nature::ALL.into_iter().find(|nature| nature_code(*nature) == chosen) {
                            form.nature.set(nature);
                        }
                    }>
                        {Nature::ALL
                            .into_iter()
                            .map(|nature| view! {
                                <option value=nature_code(nature) prop:selected=move || form.nature.get() == nature>
                                    {tr(lang, nature_key(nature))}
                                </option>
                            })
                            .collect_view()}
                    </select>
                </label>
                <p class="hint">{tr(lang, Key::NatureHint)}</p>
                // Вид деятельности спрашивается, когда их в профиле больше
                // одного: у одного вида выбирать нечего, пока рядом нет дохода
                // со своей строкой — тогда он отделяет прямой расход от общего.
                {move || (activities.with(Vec::len) > 1 || beside.get() || form.activity.get().is_some()).then(|| view! {
                    <label>
                        {tr(lang, Key::OpActivity)}
                        <select on:change=move |event| {
                            let chosen = event_target_value(&event);
                            form.activity.set(ActivityCode::parse(&chosen).ok());
                        }>
                            <option value="" prop:selected=move || form.activity.get().is_none()>
                                {tr(lang, Key::OpActivityShared)}
                            </option>
                            {move || {
                                let mut codes = activities.get();
                                if let Some(own) = form.activity.get_untracked()
                                    && !codes.contains(&own)
                                {
                                    codes.push(own);
                                }
                                codes.into_iter().map(|code| view! {
                                    <option value=code.to_string() prop:selected=move || form.activity.get() == Some(code)>
                                        {code.to_string()}
                                    </option>
                                }).collect_view()
                            }}
                        </select>
                    </label>
                    <p class="hint">{tr(lang, Key::OpActivityHint)}</p>
                })}
            })}
            <fieldset class="files">
                <legend>{tr(lang, Key::OpFiles)}</legend>
                <p class="hint">{move || {
                    let lang = lang.get();
                    fill(
                        t(lang, Key::OpFilesHint),
                        &[
                            &i18n::integer(lang, OperationData::MAX_ATTACHMENTS as i64),
                            &i18n::integer(lang, (attachment::MAX_BYTES >> 20) as i64),
                        ],
                    )
                }}</p>
                <ul class="file-list">
                    <For each=move || form.files.get() key=|row| row.key let(row)>
                        <FileLine lang form links row />
                    </For>
                </ul>
                {move || preparing.get().then(|| view! {
                    <p role="status">{tr(lang, Key::OpFilePreparing)}</p>
                })}
                {move || file_problems.with(|problems| problems.iter().map(|problem| {
                    let problem = problem.clone();
                    view! { <p role="alert">{move || problem.text(lang.get())}</p> }
                }).collect_view())}
                // Поле выбора файла спрятано под подписью-кнопкой, как
                // у копии: нажатие на подпись открывает выбор и на iPhone.
                <label class="button secondary file" class:disabled=move || preparing.get()>
                    {icon(Icon::Clip)}
                    <span>{tr(lang, Key::OpFileAdd)}</span>
                    <input
                        type="file"
                        class="visually-hidden"
                        accept="image/jpeg,image/png,application/pdf"
                        multiple
                        on:change=pick
                    />
                </label>
            </fieldset>
            {move || invalid.get().then(|| view! { <p role="alert">{tr(lang, Key::ProfileFix)}</p> })}
            {move || failure.get().map(|problem| view! { <p role="alert">{tr(lang, problem.key())}</p> })}
            <div class="actions">
                <button type="submit" disabled=move || form.busy.get() || preparing.get()>
                    {tr(lang, Key::ProfileSave)}
                </button>
                {id.map(|_| view! {
                    <button type="button" class="secondary" disabled=move || form.busy.get() on:click=delete>
                        {tr(lang, Key::OpDelete)}
                    </button>
                })}
                <button type="button" class="secondary" disabled=move || form.busy.get() on:click=leave>
                    {tr(lang, Key::Cancel)}
                </button>
            </div>
        </form>
    }
}

/// Вид деятельности дохода — код из профиля (задача 19): у ИП, чьи коды
/// облагаются по разным строкам налога. Выбор обязателен, а доход по коду,
/// который закрывает налог с оборота, — предупреждением сразу (решение
/// человека 2026-10-19). Общее у записи дохода и формы документа.
#[component]
pub(super) fn IncomeActivity(
    lang: ReadSignal<Lang>,
    activities: Memo<Vec<ActivityCode>>,
    chosen: RwSignal<Option<ActivityCode>>,
    /// День дохода — записью ISO: по нему правило о кодах.
    date: RwSignal<String>,
    tried: RwSignal<bool>,
    /// Обязателен ли вид (`turnover::activity_needed`): нет — его можно не
    /// выбирать.
    required: Memo<bool>,
) -> impl IntoView {
    view! {
        <label>
            {tr(lang, Key::OpActivity)}
            <select on:change=move |event| {
                let picked = event_target_value(&event);
                chosen.set(ActivityCode::parse(&picked).ok());
            }>
                <option value="" prop:selected=move || chosen.get().is_none()>
                    {tr(lang, Key::NotChosen)}
                </option>
                {move || {
                    let mut codes = activities.get();
                    if let Some(own) = chosen.get_untracked()
                        && !codes.contains(&own)
                    {
                        codes.push(own);
                    }
                    codes.into_iter().map(|code| view! {
                        <option value=code.to_string() prop:selected=move || chosen.get() == Some(code)>
                            {code.to_string()}
                        </option>
                    }).collect_view()
                }}
            </select>
        </label>
        <p class="hint">{tr(lang, Key::IncomeActivityHint)}</p>
        {shown_problem(tried, move || {
            (required.get() && chosen.get().is_none()).then(|| t(lang.get(), Key::IncomeActivityMissing).to_owned())
        })}
        {move || {
            let code = chosen.get()?;
            let day = date.with(|text| Date::from_iso(text))?;
            turnover::excluded(&code, day)?;
            Some(view! {
                <p class="warning" role="alert">{move || {
                    fill(t(lang.get(), Key::IncomeExcluded), &[&code.to_string()])
                }}</p>
            })
        }}
    }
}

/// Строка налога дохода, если она не та, что у кода (задача 70): свёрнутым
/// вопросом, раскрытым у дохода с такой строкой. Ставка — по правилам на день
/// дохода и той строкой, какой посчитает расчёт; граница со строкой кода —
/// подсказкой: её решает человек с бухгалтером. Общее у записи дохода, формы
/// документа и его карточки.
#[component]
pub(super) fn IncomeKindChoice(
    lang: ReadSignal<Lang>,
    profiles: Profiles,
    chosen: RwSignal<Option<IncomeKind>>,
    /// День дохода — записью ISO: по нему ставка.
    date: RwSignal<String>,
) -> impl IntoView {
    let rate = move |kind| {
        let day = date.with(|text| Date::from_iso(text))?;
        let line = profiles.with(|profile| Some(turnover::chosen_kind(profile, kind, day)))?;
        turnover::rate(line, day).map(|rule| rule.rate)
    };
    view! {
        <details class="income-kind" prop:open=move || chosen.get().is_some()>
            <summary>{tr(lang, Key::IncomeKindAsk)}</summary>
            <label>
                {tr(lang, Key::IncomeKindLabel)}
                <select on:change=move |event| {
                    let picked = event_target_value(&event);
                    chosen.set(IncomeKind::OF_INCOME.into_iter().find(|kind| kind.row().to_string() == picked));
                }>
                    <option value="" prop:selected=move || chosen.get().is_none()>
                        {tr(lang, Key::IncomeKindByCode)}
                    </option>
                    {IncomeKind::OF_INCOME.into_iter().map(|kind| view! {
                        <option value=kind.row().to_string() prop:selected=move || chosen.get() == Some(kind)>
                            {tr(lang, turnover::kind_key(kind))}
                        </option>
                    }).collect_view()}
                </select>
            </label>
            {move || chosen.get().map(|kind| view! {
                {rate(kind).map(|rate| view! {
                    <p class="hint">{move || {
                        let lang = lang.get();
                        fill(t(lang, Key::IncomeKindRate), &[&tax::percent(lang, rate)])
                    }}</p>
                })}
                {income_kind_hint(kind).map(|key| view! { <p class="hint">{tr(lang, key)}</p> })}
            })}
        </details>
    }
}

/// Что сказать о выбранной строке (`IncomeKind::OF_INCOME`).
const fn income_kind_hint(kind: IncomeKind) -> Option<Key> {
    match kind {
        IncomeKind::Rent => Some(Key::IncomeKindRentHint),
        IncomeKind::Assets => Some(Key::IncomeKindAssetsHint),
        IncomeKind::Trade
        | IncomeKind::Recyclables
        | IncomeKind::Newspapers
        | IncomeKind::Production
        | IncomeKind::Catering
        | IncomeKind::CateringOther
        | IncomeKind::HighTech
        | IncomeKind::Other => None,
    }
}

/// Файл в форме: картинка — миниатюрой, имя — ссылкой на сам файл, размер
/// и «Убрать».
#[component]
fn FileLine(
    lang: ReadSignal<Lang>,
    form: Form,
    links: RwSignal<Vec<(u32, Rc<Download>)>, LocalStorage>,
    row: FileRow,
) -> impl IntoView {
    let key = row.key;
    let link = move || links.with(|links| links.iter().find(|(known, _)| *known == key).map(|(_, link)| Rc::clone(link)));
    let name = row.name.clone();
    let image = row.kind.is_image();
    let size = row.size;
    view! {
        <li class="file-row">
            {move || match link() {
                Some(download) => {
                    let url = download.url().to_owned();
                    let thumb = url.clone();
                    let file_name = download.name();
                    view! {
                        <a class="file-link" href=url download=file_name>
                            {image.then(|| view! { <img class="file-thumb" src=thumb alt="" /> })}
                            <span>{name.clone()}</span>
                        </a>
                    }
                    .into_any()
                }
                None => view! { <span class="file-link">{name.clone()}</span> }.into_any(),
            }}
            <span class="hint">{move || {
                let lang = lang.get();
                let kilobytes = i64::from(size.div_ceil(1024));
                fill(t(lang, Key::OpFileSize), &[&i18n::integer(lang, kilobytes)])
            }}</span>
            <button
                type="button"
                class="secondary"
                on:click=move |_| {
                    form.files.update(|rows| rows.retain(|known| known.key != key));
                    links.update(|links| links.retain(|(known, _)| *known != key));
                }
            >
                {tr(lang, Key::Remove)}
            </button>
        </li>
    }
}

/// Код документа продавца для `value` у `<option>` — как в данных.
const fn proof_code(proof: Proof) -> &'static str {
    match proof {
        Proof::TaxInvoice => "tax_invoice",
        Proof::SettlementInvoice => "settlement_invoice",
        Proof::CashReceipt => "cash_receipt",
        Proof::IndividualPurchase => "individual_purchase",
        Proof::Contract => "contract",
        Proof::ForeignSupplier => "foreign_supplier",
        Proof::BankPayment => "bank_payment",
        Proof::None => "none",
    }
}

/// Код вида расхода для `value` у `<option>` — как в данных.
const fn nature_code(nature: Nature) -> &'static str {
    match nature {
        Nature::Ordinary => "ordinary",
        Nature::Levy => "levy",
        Nature::Asset => "asset",
        Nature::Travel => "travel",
        Nature::Hospitality => "hospitality",
    }
}

/// Сохранить запись с файлами — одной транзакцией: команда ядра
/// (`Operation::save`), новые вложения, убранные — меткой.
async fn save(
    store: &Store,
    id: Option<RecordId>,
    data: OperationData,
    fresh: Vec<(String, FileBytes)>,
) -> Result<Operation, Problem> {
    let tx = store.write()?;
    let now = clock::now();
    let existing = match id {
        // Записи нет — её стёрли в обход программы: сохранять некуда.
        Some(id) => Some(tx.get::<Operation>(id).await?.ok_or(StoreError::Failed)?),
        None => None,
    };
    let saved = Operation::save(existing.as_ref(), data, fresh, now)?;
    let mut removed = Vec::new();
    for file in &saved.dropped {
        // Нечитаемое вложение убирать нечем: ссылка на него уходит из
        // записи, а сама запись остаётся в базе как была.
        match tx.get::<Attachment>(*file).await {
            Ok(Some(found)) => removed.extend(found.remove(now)),
            Ok(None) | Err(StoreError::Damaged) => {}
            Err(error) => return Err(error.into()),
        }
    }
    let mut writes = Writes::default();
    for file in &saved.added {
        writes.insert(file);
    }
    for file in &removed {
        writes.update(file);
    }
    let operation = match (existing, saved.operation) {
        (None, Some(created)) => {
            writes.insert(&created);
            created
        }
        (Some(_), Some(edited)) => {
            writes.update(&edited);
            edited
        }
        (Some(known), None) => known,
        // Новая запись без изменений не бывает: `save` её создаёт.
        (None, None) => return Err(Problem::Store(StoreError::Failed)),
    };
    tx.commit(writes).await?;
    Ok(operation)
}
