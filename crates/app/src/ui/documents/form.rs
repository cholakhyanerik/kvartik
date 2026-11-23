//! Форма счёта и акта (задача 119 — файлом из `documents.rs`): новый
//! документ по шагам — кому, что сделано, проверить и выпустить, — правка
//! черновика и акт по счёту одной формой, разбор цен и количеств и выпуск
//! одной транзакцией с номером (`save`). Предпросмотр PDF — `ui/paper.rs`.

use super::*;

/// С чего начинается форма.
#[derive(Clone, PartialEq, Debug)]
pub(super) enum Start {
    /// Пустой документ этого вида: дата — сегодня, одна пустая строка.
    Blank(DocKind, Option<Date>),
    /// Данные черновика или акта по счёту.
    Data(DocumentData),
}

/// Строка документа в форме — строками, как их ввёл человек.
#[derive(Clone, Copy)]
pub(super) struct ItemRow {
    key: u32,
    text: RwSignal<String>,
    quantity: RwSignal<String>,
    price: RwSignal<String>,
}

/// Поля формы. Читает их ядро (`parse` у каждого типа, `DocumentData::check`),
/// окно только показывает ошибки.
#[derive(Clone, Copy)]
pub(super) struct Form {
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
    /// Строка налога дохода, если она не та, что у кода (задача 70).
    income_kind: RwSignal<Option<IncomeKind>>,
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
            income_kind: RwSignal::new(data.and_then(|d| d.income_kind)),
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
            income_kind: self.income_kind.get_untracked(),
        };
        // Доходу со своей строкой код не нужен (`tax::turnover`).
        if needed && data.activity.is_none() && data.income_kind.is_none() {
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
/// окна. Живёт в ядре: её же зовёт калькулятор лендинга (задача 34).
pub(in crate::ui) use kvartik_core::i18n::number_problem;

/// Что не так с количеством: число не прочиталось или оно не больше нуля.
pub(super) fn quantity_problem(lang: Lang, text: &str) -> Option<String> {
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
pub(super) fn price_problem(lang: Lang, text: &str, currency: Option<Currency>) -> Option<String> {
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
pub(in crate::ui) fn shown_problem(
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


/// Форма документа. Новый — по шагам (дизайн «Kvartik Design»): кому, что
/// сделано, проверить и выпустить. Правка черновика и акт по счёту — одной
/// формой со всеми полями: там всё уже заполнено.
#[component]
pub(super) fn DocForm(ctx: Ctx, existing: Option<Document>, start: Start) -> impl IntoView {
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
    let required = Memo::new(move |_| needed.get() && form.income_kind.get().is_none());
    // Выпуск — по сроку лицензии (задача 25): кончился — вместо
    // «Выпустить» слова и ключ, а черновик сохраняется.
    ctx.licenses.watch(ctx.base, ctx.profiles);
    let access = Memo::new(move |_| ctx.licenses.access(ctx.profiles));
    let locked = Memo::new(move |_| license::issue_locked(ctx.licenses, access.get(), ctx.today));
    // На этом устройстве выпуск этого ИП выключен (задачи 41 и 116): номера
    // выдаёт другое, а черновик туда перенесёт копия. Тогда о сроке — ни
    // слова: выпускать будут там. Пока база не открыта, выпускать нечем.
    let elsewhere = Memo::new(move |_| {
        ctx.base.is_open() && ctx.base.get().is_some_and(|store| crate::ui::settings::issue_elsewhere(store.name()))
    });
    let blocked = Memo::new(move |_| elsewhere.get() || locked.get().is_some());
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
        // Выпуск выключен — и в другой вкладке тоже: запись устройства
        // читается заново (ревью задачи 41).
        let elsewhere_now = || ctx.base.get().is_some_and(|store| crate::ui::settings::issue_elsewhere_now(store.name()));
        if form.busy.get_untracked() || (issue && (locked.get_untracked().is_some() || elsewhere_now())) {
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
                        ctx.router.form_closed();
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
    let leave = move || {
        mode.set(Mode::List);
        ctx.router.form_closed();
    };
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
    // Акт по счёту (задача 84): сколько выпущено действующими актами этого
    // счёта, их число и сумма самого счёта — форма говорит, что этот акт
    // следующий этап, и сразу, а не при выпуске, — что он больше остатка.
    let stage = Memo::new(move |_| {
        let basis = form.basis.filter(|_| form.kind == DocKind::Act)?;
        documents.with(|list, _, _| {
            let invoice = list.iter().find(|known| known.id() == basis)?;
            let acted = invoice.acted(list).ok()?;
            Some((invoice.acts(list).count(), acted, invoice.total().ok()?))
        })
    });
    // Чем акт по счёту не годится — словами под итогом, сразу при вводе.
    // Выпуск откажет теми же словами, и внизу формы они не повторяются
    // (полный прогон 2026-11-10, п. 9).
    let over_key = move || {
        let (_, acted, whole) = stage.get()?;
        let sum = total_money(lang.get())?;
        if sum.currency() != whole.currency() {
            Some(Key::DocActCurrency)
        } else if acted.minor().saturating_add(sum.minor()) > whole.minor() {
            Some(Key::DocActsOverInvoice)
        } else {
            None
        }
    };
    let over_invoice = move || over_key().map(|key| view! { <p role="alert">{tr(lang, key)}</p> });
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
                        <span class="label">{tr(lang, Key::Change)}</span>
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
                                <span class="label">{tr(lang, Key::Remove)}</span>
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
                    <span class="label">{tr(lang, Key::ItemAdd)}</span>
                </button>
                <p class="total">{total}</p>
                // Акт-этап: сколько осталось по счёту — прямо под итогом,
                // чтобы сравнить; строки — как в счёте (полный прогон
                // 2026-11-10, п. 9).
                {move || stage.get().filter(|(count, ..)| *count > 0).map(|(_, acted, whole)| view! {
                    <p class="act-left">{move || {
                        let lang = lang.get();
                        let left = Money::new(whole.minor().saturating_sub(acted.minor()).max(0), whole.currency());
                        fill(t(lang, Key::DocActLeft), &[&i18n::money(lang, left)])
                    }}</p>
                })}
                <p class="hint">{in_drams}</p>
                {move || stage.get().filter(|(count, ..)| *count > 0).map(|(_, acted, _)| view! {
                    <p class="hint">{move || {
                        let lang = lang.get();
                        fill(t(lang, Key::DocActStage), &[&i18n::money(lang, acted)])
                    }}</p>
                })}
                {over_invoice}
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
                    <span class="label">{tr(lang, Key::DocLastDay)}</span>
                </button>
                // Вид деятельности дохода — когда без него не узнать строку
                // налога или режим (задача 19).
                {move || (required.get() || form.activity.get().is_some()).then(|| view! {
                    <IncomeActivity
                        lang
                        activities
                        chosen=form.activity
                        date=form.service_date
                        tried
                        required
                    />
                })}
                <IncomeKindChoice lang profiles=ctx.profiles chosen=form.income_kind date=form.service_date />
                <Show when=foreign>
                    <label>
                        {move || {
                            let lang = lang.get();
                            let code = form.currency.get().map(currency_code).unwrap_or_default();
                            fill(t(lang, Key::OpRate), &[code])
                        }}
                        <input type="text" inputmode="decimal" autocomplete="off" maxlength="24" bind:value=form.rate />
                    </label>
                    <CbaHint
                        lang
                        currency=Signal::derive(move || form.currency.get())
                        day=Signal::derive(move || form.service_date.with(|text| Date::from_iso(text)))
                        field=form.rate
                    />
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
                // Счёт в долларах или евро, а по реквизитам профиля из-за
                // границы не заплатить (полный прогон 2026-11-10, п. 8): PDF
                // даст драмовый счёт или счёт без SWIFT. Выпустить не мешает —
                // клиент может платить и из банка Армении. Пока профиль
                // читается, слов нет.
                {(form.kind == DocKind::Invoice).then_some(move || {
                    let currency = form.currency.get().filter(|currency| *currency != Currency::Amd)?;
                    let payable = ctx
                        .profiles
                        .with(|profile| profile.map(|profile| payable_abroad(&profile.accounts, currency)));
                    (payable == Some(false)).then(|| view! {
                        <p class="warning" role="alert">
                            {move || fill(t(lang.get(), Key::DocNotPayableAbroad), &[currency_code(currency)])}
                        </p>
                    })
                })}
                <div class="actions">
                    <button
                        type="button"
                        class="secondary"
                        aria-expanded=move || preview.with(Option::is_some).to_string()
                        on:click=show_preview
                    >
                        <span class="label">{tr(lang, Key::DocPreview)}</span>
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
            // Отказ, который уже сказан под итогом, второй раз не повторяется.
            {move || failure.get().filter(|problem| over_key() != Some(problem.key())).map(|problem| view! {
                <p role="alert">{tr(lang, problem.key())}</p>
            })}
            {move || match step.get() {
                1 => None,
                2 => Some(view! {
                    <button type="submit" class="wide"><span class="label">{tr(lang, Key::DocNextReview)}</span></button>
                }
                .into_any()),
                _ => Some(view! {
                    {move || match locked.get() {
                        _ if elsewhere.get() => view! { <IssueElsewhere lang /> }.into_any(),
                        Some(today) => view! { <IssueLocked lang access=access.get() today /> }.into_any(),
                        None => view! { <p class="hint">{tr(lang, Key::DocIssueHint)}</p> }.into_any(),
                    }}
                    <div class="actions">
                        {move || (!blocked.get()).then(|| view! {
                            <button
                                type="button"
                                class:wide=wizard
                                disabled=move || form.busy.get()
                                on:click=move |_| submit(true)
                            >
                                <span class="label">{tr(lang, Key::DocIssue)}</span>
                            </button>
                        })}
                        // Выпуск заперт — главное действие формы черновик.
                        <button
                            type="submit"
                            class:secondary=move || !blocked.get()
                            class:wide=move || wizard && blocked.get()
                            disabled=move || form.busy.get()
                        >
                            <span class="label">{tr(lang, Key::DocSaveDraft)}</span>
                        </button>
                        {id.map(|_| view! {
                            <button type="button" class="secondary" disabled=move || form.busy.get() on:click=delete>
                                <span class="label">{tr(lang, Key::DocDeleteDraft)}</span>
                            </button>
                        })}
                        <button
                            type="button"
                            class="secondary"
                            disabled=move || form.busy.get()
                            on:click=move |_| leave()
                        >
                            <span class="label">{tr(lang, Key::Cancel)}</span>
                        </button>
                    </div>
                }
                .into_any()),
            }}
        </form>
    }
}

/// Слова вместо «Выпустить», когда на этом устройстве выпуск выключен
/// (задача 41): выпускает другое устройство, а черновик туда перенесёт
/// копия. Вид — как у слов о запертом выпуске: то же место и то же «нельзя
/// здесь», а ссылка ведёт туда, где это меняют.
#[component]
pub(super) fn IssueElsewhere(lang: ReadSignal<Lang>) -> impl IntoView {
    view! {
        <div class="notice due" role="status">
            {icon(Icon::Document)}
            <p class="reminder">{tr(lang, Key::DocIssueElsewhere)}</p>
            <a class="button" href=Screen::Settings.href()><span class="label">{tr(lang, Key::SettingsTitle)}</span></a>
        </div>
    }
}

/// Сведения о клиенте документа — страна, валюта и язык, что известно.
pub(super) fn client_facts(
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
pub(super) fn line_total(lang: Lang, row: ItemRow, currency: Currency) -> Option<Money> {
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
pub(in crate::ui) fn DateField(lang: ReadSignal<Lang>, label: Key, value: RwSignal<String>, tried: RwSignal<bool>) -> impl IntoView {
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
/// и клиента (копия реквизитов), логотип профиля и запись его картинки для
/// выпуска (задача 63). Видна тестам окна: путь логотипа выпуска они
/// проверяют им самим.
pub(in crate::ui) async fn save(
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
    let mut writes = Writes::default();
    let document = if issue {
        let listed = tx.list::<Document>().await?;
        let profile = Profile::current(tx.all::<Profile>().await?);
        let client = tx
            .get::<Client>(draft.data().client)
            .await?
            .ok_or(DocError::ClientDeleted)?;
        let logo = issued_logo(&tx, profile.as_ref(), &mut writes, now).await?;
        draft.issue(
            &listed.records,
            listed.unreadable,
            profile.as_ref().map(Profile::data),
            logo,
            &client,
            now,
        )?
    } else {
        draft
    };
    match &existing {
        None => writes.insert(&document),
        Some(known) if *known != document => writes.update(known, &document),
        Some(_) => {}
    }
    tx.commit(writes).await?;
    Ok(document)
}

/// Логотип профиля на день выпуска (задача 63) — что запомнит документ
/// (`Document::issue`, `Issued::logo`); картинка ложится в `writes` записью
/// под своим отпечатком, если в базе её ещё нет. Нечитаемая запись логотипа
/// и картинка, которую PDF не возьмёт, — «неизвестно»: PDF возьмёт нынешний
/// логотип, как у документов до задачи 63, а выпуск они не держат.
/// Нечитаемая запись под отпечатком — испорченная
/// копия той же картинки (отпечаток — её содержимое), и на её место встаёт
/// исправная, а не отказ всему выпуску.
pub(super) async fn issued_logo(
    tx: &Tx<Write>,
    profile: Option<&Profile>,
    writes: &mut Writes,
    now: Timestamp,
) -> Result<Option<Option<RecordId>>, StoreError> {
    let Some(profile) = profile else {
        return Ok(Some(None));
    };
    let logo = match tx.get::<Logo>(profile.id()).await {
        Ok(logo) => logo.filter(|logo| !logo.is_removed()),
        Err(StoreError::Damaged) => return Ok(None),
        Err(error) => return Err(error),
    };
    let Some(logo) = logo else {
        return Ok(Some(None));
    };
    // Картинку, которую PDF не возьмёт (больше 2048 точек на сторону —
    // `doc::image::MAX_PIXELS`), выпуск не запоминает: с ней документ навсегда
    // остался бы без логотипа, а «неизвестно» берёт нынешний — исправленный
    // в профиле логотип дойдёт и до него, как до задачи 63 (ревью задачи 63).
    if !Picture::fits(logo.image()) {
        return Ok(None);
    }
    let id = IssuedLogo::id_of(logo.image());
    let (existing, damaged) = match tx.get::<IssuedLogo>(id).await {
        Ok(existing) => (existing, false),
        Err(StoreError::Damaged) => (None, true),
        Err(error) => return Err(error),
    };
    if let Some(kept) = IssuedLogo::keep(existing.as_ref(), logo.image(), now) {
        match &existing {
            None if !damaged => writes.insert(&kept),
            Some(before) => writes.update(before, &kept),
            None => writes.put(&kept),
        }
    }
    Ok(Some(Some(id)))
}
