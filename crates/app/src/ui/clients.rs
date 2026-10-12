//! Раздел «Клиенты» (задача 10): список с поиском по названию, форма
//! клиента, удаление меткой и восстановление.
//!
//! Экрана клиентов ещё нет, и раздел стоит на единственном экране каркаса,
//! под профилем. Каждое действие — команда ядра (`Client::create`, `edit`,
//! `delete`, `restore`) и одна транзакция хранилища (CLAUDE.md, «Данные
//! меняются одним путём»).
//!
//! Список читается `Tx::list`: испорченная запись не прячет остальных
//! клиентов, а называется числом с советом восстановить её из копии.
//! После восстановления из копии раздел перечитывает базу по
//! `Base::replacements` — иначе показал бы прежний список (дефект 0.0.12
//! у профиля).

use std::sync::Arc;

use kvartik_core::format::Stored;
use kvartik_core::i18n::{self, Key, Lang, fill, plural, t};
use kvartik_core::model::client::{Client, ClientData};
use kvartik_core::model::country::Country;
use kvartik_core::model::money::Currency;
use kvartik_core::model::record::RecordId;
use kvartik_core::model::text::{Line, Lines};
use kvartik_core::model::tin::Tin;
use leptos::ev::SubmitEvent;
use leptos::prelude::*;

use super::backup::Backups;
use super::form::{
    AreaField, CURRENCIES, FieldError, TextField, currency_code, currency_key, optional,
};
use super::storage::error_key;
use super::{Base, tr};
use crate::platform::clock;
use crate::store::{Store, StoreError, Writes};

/// Что окно знает о клиентах.
#[derive(Clone, Copy)]
pub struct Clients {
    state: RwSignal<State>,
}

/// Где клиенты.
#[derive(Clone, PartialEq, Debug)]
enum State {
    /// База ещё открывается.
    Loading,
    /// Прочитаны: все клиенты, включая удалённых, и сколько записей не
    /// читается. В `Arc`: сигнал отдаёт состояние копией.
    Ready {
        clients: Arc<Vec<Client>>,
        unreadable: usize,
    },
    /// Не прочитаны.
    Failed(StoreError),
}

impl Clients {
    pub fn new() -> Self {
        Self {
            state: RwSignal::new(State::Loading),
        }
    }

    /// Прочитать клиентов из базы: когда она открылась и когда её
    /// заменило восстановление из копии.
    pub async fn load(&self, store: &Store) {
        let read = match store.read() {
            Ok(tx) => tx.list::<Client>().await,
            Err(error) => Err(error),
        };
        self.state.set(match read {
            Ok(listed) => State::Ready {
                clients: Arc::new(listed.records),
                unreadable: listed.unreadable,
            },
            Err(error) => State::Failed(error),
        });
    }

    /// Записанный клиент — на место прежнего или в конец списка: перечитывать
    /// базу ради одной записи незачем.
    fn put(&self, client: Client) {
        self.state.update(|state| {
            if let State::Ready { clients, .. } = state {
                let list = Arc::make_mut(clients);
                match list.iter_mut().find(|known| known.id() == client.id()) {
                    Some(known) => *known = client,
                    None => list.push(client),
                }
            }
        });
    }

    /// Клиент по идентификатору — без слежения: форма открывается один раз.
    fn get(&self, id: RecordId) -> Option<Client> {
        self.state.with_untracked(|state| match state {
            State::Ready { clients, .. } => clients.iter().find(|c| c.id() == id).cloned(),
            State::Loading | State::Failed(_) => None,
        })
    }

    /// Что-то из списка, посчитанное по прочитанным клиентам; пока их нет —
    /// значение по умолчанию.
    fn with<T: Default>(&self, read: impl FnOnce(&[Client], usize) -> T) -> T {
        self.state.with(|state| match state {
            State::Ready {
                clients,
                unreadable,
            } => read(clients, *unreadable),
            State::Loading | State::Failed(_) => T::default(),
        })
    }
}

/// Что показывает раздел: список или форму.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    List,
    New,
    Edit(RecordId),
}

/// Сообщение о сделанном — над списком.
#[derive(Clone, PartialEq, Eq, Debug)]
enum Notice {
    Saved,
    /// Удалён — с кнопкой «Восстановить» рядом.
    Deleted(RecordId, String),
    Restored(String),
}

/// Стадия чтения — без самих клиентов: раздел перерисовывается целиком,
/// только когда она меняется, а не на каждую правку списка.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Phase {
    Loading,
    Ready,
    Failed(StoreError),
}

/// Раздел на языке окна. Пока база открывается — пусто: «открываем…»
/// только мигнуло бы.
#[component]
pub fn ClientsSection(
    lang: ReadSignal<Lang>,
    clients: Clients,
    base: Base,
    backups: Backups,
) -> impl IntoView {
    let mode = RwSignal::new(Mode::List);
    let notice = RwSignal::new(None::<Notice>);
    let phase = Memo::new(move |_| {
        clients.state.with(|state| match state {
            State::Loading => Phase::Loading,
            State::Ready { .. } => Phase::Ready,
            State::Failed(error) => Phase::Failed(*error),
        })
    });
    // Копию восстановили — клиенты в базе могли стать другими.
    Effect::watch(
        move || base.replacements(),
        move |_, _, _| {
            if let Some(store) = base.get() {
                mode.set(Mode::List);
                notice.set(None);
                leptos::task::spawn_local(async move { clients.load(&store).await });
            }
        },
        false,
    );
    move || {
        let body = match (phase.get(), mode.get()) {
            (Phase::Loading, _) => return None,
            (Phase::Failed(error), _) => view! {
                <p role="alert">{tr(lang, error_key(error))}</p>
            }
            .into_any(),
            (Phase::Ready, Mode::List) => view! {
                <ClientList lang clients base backups mode notice />
            }
            .into_any(),
            (Phase::Ready, Mode::New) => view! {
                <ClientForm lang existing=None clients base backups mode notice />
            }
            .into_any(),
            (Phase::Ready, Mode::Edit(id)) => view! {
                <ClientForm lang existing=clients.get(id) clients base backups mode notice />
            }
            .into_any(),
        };
        Some(view! {
            <h2>{tr(lang, Key::ClientsTitle)}</h2>
            {body}
        })
    }
}

/// Действие над клиентом из списка.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Action {
    Delete,
    Restore,
}

/// Список: поиск, живые клиенты по алфавиту, удалённые — по кнопке.
#[component]
fn ClientList(
    lang: ReadSignal<Lang>,
    clients: Clients,
    base: Base,
    backups: Backups,
    mode: RwSignal<Mode>,
    notice: RwSignal<Option<Notice>>,
) -> impl IntoView {
    let query = RwSignal::new(String::new());
    let show_deleted = RwSignal::new(false);
    let failure = RwSignal::new(None::<StoreError>);
    let busy = RwSignal::new(false);
    // Производные от списка и запроса — пересчитываются, только когда
    // меняется одно из них (Правило 1).
    let shown = Memo::new(move |_| {
        let query = query.get();
        clients.with(|list, _| Client::find(list, &query).into_iter().cloned().collect::<Vec<_>>())
    });
    let deleted = Memo::new(move |_| {
        clients.with(|list, _| Client::deleted(list).into_iter().cloned().collect::<Vec<_>>())
    });
    let alive = Memo::new(move |_| clients.with(|list, _| list.iter().filter(|c| !c.is_deleted()).count()));
    let unreadable = Memo::new(move |_| clients.with(|_, unreadable| unreadable));

    let act = move |id: RecordId, action: Action| {
        if busy.get_untracked() {
            return;
        }
        let Some(store) = base.get() else {
            failure.set(Some(StoreError::Failed));
            return;
        };
        busy.set(true);
        failure.set(None);
        leptos::task::spawn_local(async move {
            let outcome = change(&store, id, action).await;
            busy.set(false);
            match outcome {
                Ok(Some(client)) => {
                    let name = client.data().name.as_str().to_owned();
                    notice.set(Some(match action {
                        Action::Delete => Notice::Deleted(client.id(), name),
                        Action::Restore => Notice::Restored(name),
                    }));
                    clients.put(client);
                    backups.data_saved();
                }
                // Клиент уже в этом состоянии или пропал — его поменяла
                // другая вкладка: окно показывает, что в базе.
                Ok(None) => clients.load(&store).await,
                Err(error) => failure.set(Some(error)),
            }
        });
    };

    view! {
        {move || (unreadable.get() > 0).then(|| view! {
            <p role="alert">
                {move || {
                    let lang = lang.get();
                    fill(t(lang, Key::ClientsUnreadable), &[&i18n::integer(lang, unreadable.get() as i64)])
                }}
            </p>
        })}
        {move || (alive.get() == 0).then(|| view! { <p>{tr(lang, Key::ClientsAbout)}</p> })}
        <button
            type="button"
            on:click=move |_| {
                notice.set(None);
                mode.set(Mode::New);
            }
        >
            {tr(lang, Key::ClientAdd)}
        </button>
        {move || notice.get().map(|shown| match shown {
            Notice::Saved => view! { <p role="status">{tr(lang, Key::ClientSaved)}</p> }.into_any(),
            Notice::Deleted(id, name) => view! {
                <div class="notice-row">
                    <p role="status">{move || fill(t(lang.get(), Key::ClientDeleted), &[&name])}</p>
                    <button
                        type="button"
                        class="secondary"
                        disabled=move || busy.get()
                        on:click=move |_| act(id, Action::Restore)
                    >
                        {tr(lang, Key::ClientRestore)}
                    </button>
                </div>
            }
            .into_any(),
            Notice::Restored(name) => view! {
                <p role="status">{move || fill(t(lang.get(), Key::ClientRestored), &[&name])}</p>
            }
            .into_any(),
        })}
        {move || failure.get().map(|error| view! { <p role="alert">{tr(lang, error_key(error))}</p> })}
        <Show when=move || { alive.get() > 0 }>
            <label class="search">
                {tr(lang, Key::ClientSearch)}
                <input type="search" bind:value=query autocomplete="off" maxlength=Line::MAX_CHARS.to_string() />
            </label>
            <ul class="clients">
                <For
                    each=move || shown.get()
                    key=|client| (client.id(), client.meta().modified())
                    let(client)
                >
                    <ClientRow lang client mode notice busy act />
                </For>
            </ul>
            {move || shown.with(Vec::is_empty).then(|| view! {
                <p class="hint">{tr(lang, Key::ClientsNotFound)}</p>
            })}
        </Show>
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
                        t(lang, Key::ClientsDeletedHide).to_owned()
                    } else {
                        let count = deleted.with(Vec::len) as i64;
                        fill(t(lang, Key::ClientsDeleted), &[&i18n::integer(lang, count)])
                    }
                }}
            </button>
            <Show when=move || show_deleted.get()>
                <ul class="clients deleted">
                    <For
                        each=move || deleted.get()
                        key=|client| (client.id(), client.meta().modified())
                        let(client)
                    >
                        <DeletedRow lang client busy act />
                    </For>
                </ul>
            </Show>
        </Show>
    }
}

/// Живой клиент в списке: название, короткие сведения и кнопки.
#[component]
fn ClientRow<A>(
    lang: ReadSignal<Lang>,
    client: Client,
    mode: RwSignal<Mode>,
    notice: RwSignal<Option<Notice>>,
    busy: RwSignal<bool>,
    act: A,
) -> impl IntoView
where
    A: Fn(RecordId, Action) + Copy + Send + Sync + 'static,
{
    let id = client.id();
    let data = client.data().clone();
    view! {
        <li>
            <p class="client-name">{data.name.as_str().to_owned()}</p>
            <p class="hint facts">
                {move || facts(lang.get(), &data).into_iter().map(|fact| view! { <span>{fact}</span> }).collect_view()}
            </p>
            <div class="actions">
                <button
                    type="button"
                    class="secondary"
                    on:click=move |_| {
                        notice.set(None);
                        mode.set(Mode::Edit(id));
                    }
                >
                    {tr(lang, Key::ClientEdit)}
                </button>
                <button
                    type="button"
                    class="secondary"
                    disabled=move || busy.get()
                    on:click=move |_| act(id, Action::Delete)
                >
                    {tr(lang, Key::ClientDelete)}
                </button>
            </div>
        </li>
    }
}

/// Удалённый клиент в списке удалённых: название и «Восстановить».
#[component]
fn DeletedRow<A>(lang: ReadSignal<Lang>, client: Client, busy: RwSignal<bool>, act: A) -> impl IntoView
where
    A: Fn(RecordId, Action) + Copy + Send + Sync + 'static,
{
    let id = client.id();
    view! {
        <li>
            <p class="client-name">{client.data().name.as_str().to_owned()}</p>
            <div class="actions">
                <button
                    type="button"
                    class="secondary"
                    disabled=move || busy.get()
                    on:click=move |_| act(id, Action::Restore)
                >
                    {tr(lang, Key::ClientRestore)}
                </button>
            </div>
        </li>
    }
}

/// Короткие сведения о клиенте на языке окна: страна, ՀՎՀՀ, валюта, язык
/// документов и срок оплаты — что заполнено.
fn facts(lang: Lang, data: &ClientData) -> Vec<String> {
    let mut facts = Vec::new();
    if let Some(country) = data.country {
        facts.push(country.name(lang).to_owned());
    }
    if let Some(tin) = &data.tin {
        facts.push(fill(t(lang, Key::TinShort), &[tin.as_str()]));
    }
    if !data.tax_id.is_empty() {
        facts.push(fill(t(lang, Key::TaxIdShort), &[data.tax_id.as_str()]));
    }
    if let Some(currency) = data.currency {
        facts.push(currency_code(currency).to_owned());
    }
    if let Some(doc_lang) = data.lang {
        facts.push(doc_lang.native_name().to_owned());
    }
    if let Some(days) = data.payment_days {
        let n = u64::from(days);
        let count = fill(
            plural(lang, n, Key::DaysOne, Key::DaysFew, Key::DaysMany),
            &[&i18n::integer(lang, i64::from(days))],
        );
        facts.push(fill(t(lang, Key::ClientPaymentTerm), &[&count]));
    }
    facts
}

/// Удалить или восстановить клиента — одной транзакцией. `Ok(None)` —
/// клиента нет или он уже в этом состоянии: записывать нечего.
async fn change(store: &Store, id: RecordId, action: Action) -> Result<Option<Client>, StoreError> {
    let tx = store.write()?;
    let Some(client) = tx.get::<Client>(id).await? else {
        return Ok(None);
    };
    let now = clock::now();
    let changed = match action {
        Action::Delete => client.delete(now),
        Action::Restore => client.restore(now),
    };
    let Some(changed) = changed else {
        return Ok(None);
    };
    let mut writes = Writes::default();
    writes.update(&changed);
    tx.commit(writes).await?;
    Ok(Some(changed))
}

/// Поля формы — строками и выбором, как их ввёл человек. Читает их ядро.
#[derive(Clone, Copy)]
struct Form {
    name: RwSignal<String>,
    country: RwSignal<Option<Country>>,
    address: RwSignal<String>,
    tin: RwSignal<String>,
    tax_id: RwSignal<String>,
    currency: RwSignal<Option<Currency>>,
    doc_lang: RwSignal<Option<Lang>>,
    days: RwSignal<String>,
    /// Была ли попытка сохранить: после неё ошибки полей видны.
    tried: RwSignal<bool>,
    /// Идёт сохранение.
    busy: RwSignal<bool>,
}

impl Form {
    /// Форма с данными клиента или пустая.
    fn new(data: Option<&ClientData>) -> Self {
        Self {
            name: RwSignal::new(data.map(|d| d.name.as_str().to_owned()).unwrap_or_default()),
            country: RwSignal::new(data.and_then(|d| d.country)),
            address: RwSignal::new(data.map(|d| d.address.as_str().to_owned()).unwrap_or_default()),
            tin: RwSignal::new(
                data.and_then(|d| d.tin.as_ref())
                    .map(|tin| tin.as_str().to_owned())
                    .unwrap_or_default(),
            ),
            tax_id: RwSignal::new(data.map(|d| d.tax_id.as_str().to_owned()).unwrap_or_default()),
            currency: RwSignal::new(data.and_then(|d| d.currency)),
            doc_lang: RwSignal::new(data.and_then(|d| d.lang)),
            days: RwSignal::new(
                data.and_then(|d| d.payment_days)
                    .map(|days| days.to_string())
                    .unwrap_or_default(),
            ),
            tried: RwSignal::new(false),
            busy: RwSignal::new(false),
        }
    }

    /// ՀՎՀՀ спрашивается только у армянской фирмы.
    fn asks_tin(&self) -> bool {
        self.country.get() == Some(Country::ARMENIA)
    }

    /// Налоговый номер своей страны — у фирмы любой другой выбранной
    /// страны. Пока страна не выбрана, не спрашивается ни он, ни ՀՎՀՀ.
    fn asks_tax_id(&self) -> bool {
        self.country.get().is_some_and(|country| country != Country::ARMENIA)
    }

    /// Данные клиента из полей; `None` — где-то ошибка, и её видно у поля.
    fn data(&self) -> Option<ClientData> {
        let country = self.country.get_untracked();
        // У фирмы другой страны поле ՀՎՀՀ спрятано, и введённое в него раньше
        // не сохраняется: номер чужой страны в акте был бы ошибкой.
        let tin = if country == Some(Country::ARMENIA) {
            optional(&self.tin.get_untracked(), Tin::parse)?
        } else {
            None
        };
        // И наоборот: номер чужой страны у армянской фирмы не сохраняется.
        let tax_id = if country.is_some_and(|country| country != Country::ARMENIA) {
            Line::new(&self.tax_id.get_untracked()).ok()?
        } else {
            Line::default()
        };
        let data = ClientData {
            name: Line::new(&self.name.get_untracked()).ok()?,
            country,
            address: Lines::new(&self.address.get_untracked()).ok()?,
            tin,
            tax_id,
            currency: self.currency.get_untracked(),
            lang: self.doc_lang.get_untracked(),
            payment_days: ClientData::parse_payment_days(&self.days.get_untracked()).ok()?,
        };
        data.check().ok()?;
        Some(data)
    }
}

/// Форма клиента: новый — `existing` пуст, правка — с его данными.
#[component]
fn ClientForm(
    lang: ReadSignal<Lang>,
    existing: Option<Client>,
    clients: Clients,
    base: Base,
    backups: Backups,
    mode: RwSignal<Mode>,
    notice: RwSignal<Option<Notice>>,
) -> impl IntoView {
    let form = Form::new(existing.as_ref().map(Client::data));
    let id = existing.as_ref().map(Client::id);
    let failure = RwSignal::new(None::<StoreError>);
    let invalid = RwSignal::new(false);
    let tried = form.tried;

    let submit = move |event: SubmitEvent| {
        event.prevent_default();
        if form.busy.get_untracked() {
            return;
        }
        form.tried.set(true);
        let Some(data) = form.data() else {
            invalid.set(true);
            return;
        };
        invalid.set(false);
        failure.set(None);
        let Some(store) = base.get() else {
            failure.set(Some(StoreError::Failed));
            return;
        };
        form.busy.set(true);
        leptos::task::spawn_local(async move {
            let outcome = save(&store, id, data).await;
            form.busy.set(false);
            match outcome {
                Ok(client) => {
                    clients.put(client);
                    backups.data_saved();
                    notice.set(Some(Notice::Saved));
                    mode.set(Mode::List);
                }
                Err(error) => failure.set(Some(error)),
            }
        });
    };

    view! {
        <form class="client" on:submit=submit novalidate>
            <fieldset>
                <legend>{tr(lang, Key::ProfileRequisites)}</legend>
                <TextField lang label=Key::ClientName value=form.name tried hint=Key::ClientNameHint autocomplete="off" />
                <FieldError
                    lang
                    tried
                    error=move || form.name.with(|name| name.trim().is_empty()).then_some(Key::ClientNameMissing)
                />
                <CountryField lang chosen=form.country />
                <AreaField lang label=Key::ClientAddress value=form.address tried />
                <Show when=move || form.asks_tin()>
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
                </Show>
                <Show when=move || form.asks_tax_id()>
                    <TextField
                        lang
                        label=Key::ClientTaxId
                        value=form.tax_id
                        tried
                        autocomplete="off"
                        hint=Key::ClientTaxIdHint
                    />
                </Show>
            </fieldset>
            <fieldset>
                <legend>{tr(lang, Key::ClientDefaults)}</legend>
                <label>
                    {tr(lang, Key::ClientCurrency)}
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
                <label>
                    {tr(lang, Key::ClientLang)}
                    <select on:change=move |event| form.doc_lang.set(Lang::from_code(&event_target_value(&event)))>
                        <option value="" prop:selected=move || form.doc_lang.get().is_none()>
                            {tr(lang, Key::NotChosen)}
                        </option>
                        {Lang::ALL
                            .into_iter()
                            .map(|doc_lang| view! {
                                <option
                                    value=doc_lang.code()
                                    lang=doc_lang.code()
                                    prop:selected=move || form.doc_lang.get() == Some(doc_lang)
                                >
                                    {doc_lang.native_name()}
                                </option>
                            })
                            .collect_view()}
                    </select>
                </label>
                <TextField
                    lang
                    label=Key::ClientPaymentDays
                    value=form.days
                    tried
                    inputmode="numeric"
                    autocomplete="off"
                    hint=Key::ClientPaymentDaysHint
                    max=3
                />
                {move || (tried.get() && form.days.with(|days| ClientData::parse_payment_days(days).is_err())).then(|| view! {
                    <p class="field-error" role="alert">
                        {move || {
                            let lang = lang.get();
                            let max = i18n::integer(lang, i64::from(ClientData::MAX_PAYMENT_DAYS));
                            fill(t(lang, Key::PaymentDaysInvalid), &[&max])
                        }}
                    </p>
                })}
            </fieldset>
            {move || invalid.get().then(|| view! {
                <p role="alert">{tr(lang, Key::ProfileFix)}</p>
            })}
            {move || failure.get().map(|error| view! {
                <p role="alert">{tr(lang, error_key(error))}</p>
            })}
            <div class="actions">
                <button type="submit" disabled=move || form.busy.get()>
                    {tr(lang, Key::ProfileSave)}
                </button>
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

/// Страна: все страны ISO 3166-1 по алфавиту названий на языке окна.
/// Порядок посчитан в ядре заранее (`Country::by_name`), а при смене языка
/// список строится заново — по новому алфавиту.
#[component]
fn CountryField(lang: ReadSignal<Lang>, chosen: RwSignal<Option<Country>>) -> impl IntoView {
    view! {
        <label>
            {tr(lang, Key::ClientCountry)}
            // Без `autocomplete="country"`: браузер подставил бы страну самого
            // ИП, а здесь — страна клиента.
            <select on:change=move |event| chosen.set(Country::from_code(&event_target_value(&event)))>
                <option value="" prop:selected=move || chosen.get().is_none()>
                    {tr(lang, Key::NotChosen)}
                </option>
                {move || {
                    let lang = lang.get();
                    Country::by_name(lang)
                        .map(|country| view! {
                            <option
                                value=country.code()
                                prop:selected=move || chosen.get() == Some(country)
                            >
                                {country.name(lang)}
                            </option>
                        })
                        .collect_view()
                }}
            </select>
        </label>
    }
}

/// Сохранить клиента — одним действием: команда ядра и одна транзакция.
/// Между чтением и записью — только расчёт ядра (шапка `store/mod.rs`).
async fn save(store: &Store, id: Option<RecordId>, data: ClientData) -> Result<Client, StoreError> {
    let tx = store.write()?;
    let now = clock::now();
    let mut writes = Writes::default();
    let client = match id {
        None => {
            // Неверные данные форма до команды не пускает.
            let client = Client::create(data, now).map_err(|_| StoreError::Failed)?;
            writes.insert(&client);
            client
        }
        Some(id) => {
            // Клиента нет — его стёрли в обход программы: сохранять некуда.
            let existing = tx.get::<Client>(id).await?.ok_or(StoreError::Failed)?;
            match existing.edit(data, now) {
                Ok(Some(client)) => {
                    writes.update(&client);
                    client
                }
                // Данные не изменились: записывать нечего.
                Ok(None) => existing,
                Err(_) => return Err(StoreError::Failed),
            }
        }
    };
    tx.commit(writes).await?;
    Ok(client)
}
