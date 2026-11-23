//! Список счетов и актов (задача 119 — файлом из `documents.rs`): поиск,
//! выбор вида, месяцы и страницы, строка документа и слова под списком,
//! а на главном экране — счета, которые ждут оплаты, и напоминание об
//! отметке ՊԵԿ. Состояние, команды и общие помощники — в `mod.rs`.

use super::*;

/// Какие документы показать: все, только счета или только акты.
pub(super) const FILTERS: [(Option<DocKind>, Key); 3] = [
    (None, Key::FilterAll),
    (Some(DocKind::Invoice), Key::NavDocuments),
    (Some(DocKind::Act), Key::FilterActs),
];

/// Список (дизайн «Kvartik Design»): поиск по клиенту и номеру, «все,
/// счета, акты», документы по месяцам — свежие сверху — и по страницам,
/// удалённые черновики — по кнопке. Новый счёт на телефоне — круглой
/// кнопкой «+» внизу, на компьютере — в меню; новый акт — под списком.
#[component]
pub(super) fn DocList(ctx: Ctx) -> impl IntoView {
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
    let unsettled = documents.unsettled;
    let limit = RwSignal::new(PAGE);
    let show_deleted = RwSignal::new(false);
    let failure = RwSignal::new(None::<Problem>);
    let busy = RwSignal::new(false);
    // Производные от списка, поиска и фильтра — пересчитываются, только
    // когда меняется одно из них (Правило 1).
    let any = Memo::new(move |_| documents.with(|list, _, _| list.iter().any(|d| !d.is_deleted())));
    // Документы без отметки о расчётном документе и их последние дни
    // (задача 23).
    let pending = Memo::new(move |_| {
        documents.with(|list, _, _| {
            settlement::pending(list)
                .iter()
                .map(|item| (item.document.id(), item.due))
                .collect::<BTreeMap<_, _>>()
        })
    });
    let shown = Memo::new(move |_| {
        let (query, kind, only) = (query.get(), kind.get(), unsettled.get());
        documents.with(|list, _, _| {
            Document::by_date(list)
                .into_iter()
                .filter(|document| kind.is_none_or(|kind| document.data().kind == kind))
                .filter(|document| !only || pending.with(|pending| pending.contains_key(&document.id())))
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
    // Номера, которые выдали два устройства порознь (задача 41): слияние
    // копий оставило оба документа.
    let repeated = Memo::new(move |_| documents.with(|list, _, _| Document::repeated_numbers(list)));
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
        // Номера искать в поиске списка: он находит документ и по номеру.
        // Плашкой, как другие слова, которые просят действия: абзац жирным
        // над поиском читался тяжело (глазной прогон задачи 41).
        {move || repeated.with(|list| !list.is_empty()).then(|| view! {
            <div class="notice" role="status">
                <p class="warning">{move || {
                    let lang = lang.get();
                    let named = repeated.with(|list| {
                        list.iter()
                            .map(|&(kind, number)| {
                                fill(t(lang, Key::DocNumbered), &[t(lang, kind_key(kind)), &number.to_string()])
                            })
                            .collect::<Vec<_>>()
                            .join(", ")
                    });
                    fill(t(lang, Key::DocsRepeated), &[&named])
                }}</p>
                <p>{tr(lang, Key::DocsRepeatedWhy)}</p>
                <a class="button secondary" href=Screen::Settings.href()>
                    <span class="label">{tr(lang, Key::SettingsTitle)}</span>
                </a>
            </div>
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
                                <span class="label">{tr(lang, key)}</span>
                            </button>
                        })
                        .collect_view()}
                </div>
            </div>
            // Расчётный документ в системе ՊԵԿ (задача 23): сколько документов
            // без отметки — и список только их.
            {move || {
                let count = pending.with(BTreeMap::len);
                (count > 0 || unsettled.get()).then(|| view! {
                    <div class="notice-row settle-row">
                        <p>{move || {
                            let lang = lang.get();
                            if unsettled.get() {
                                t(lang, Key::SettleShown).to_owned()
                            } else {
                                fill(t(lang, Key::SettlePending), &[&documents_count(lang, count)])
                            }
                        }}</p>
                        <button
                            type="button"
                            class="secondary"
                            on:click=move |_| {
                                unsettled.update(|only| *only = !*only);
                                limit.set(PAGE);
                            }
                        >
                            <span class="label">{move || {
                                let key = if unsettled.get() { Key::SettleShowAll } else { Key::SettleShow };
                                t(lang.get(), key)
                            }}</span>
                        </button>
                    </div>
                })
            }}
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
                    <DocEntry ctx document pending />
                </For>
            </ul>
        </For>
        {move || (shown.with(Vec::len) > limit.get()).then(|| view! {
            <button type="button" class="secondary" on:click=move |_| limit.update(|limit| *limit += PAGE)>
                <span class="label">{tr(lang, Key::DocsMore)}</span>
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
                <span class="label">{tr(lang, Key::DocNewAct)}</span>
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
                <span class="label">{move || {
                    let lang = lang.get();
                    if show_deleted.get() {
                        t(lang, Key::DocsDeletedHide).to_owned()
                    } else {
                        let count = deleted.with(Vec::len) as i64;
                        fill(t(lang, Key::DocsDeleted), &[&i18n::integer(lang, count)])
                    }
                }}</span>
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
                                    <span class="label">{tr(lang, Key::ClientRestore)}</span>
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
/// Открытый на компьютере — отмечен (`aria-current`) и подсвечен. В списке
/// без расчётного документа (задача 23) вместо статуса — его срок.
#[component]
pub(super) fn DocEntry(ctx: Ctx, document: Document, pending: Memo<BTreeMap<RecordId, Option<Date>>>) -> impl IntoView {
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
    let settle = move || {
        let today = ctx.today?;
        let due = ctx.documents.unsettled.get().then(|| pending.with(|map| map.get(&id).copied()))??;
        Some(settle_look(lang.get(), due, today))
    };
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
                    {move || match settle() {
                        Some((text, class)) => view! { <span class=class>{text}</span> }.into_any(),
                        None => view! { <span class=status_class>{tr(lang, status_key)}</span> }.into_any(),
                    }}
                </span>
            </a>
        </li>
    }
}

/// Срок расчётного документа словом и класс оформления: слово говорит, цвет
/// помогает (Правило 4). Прошёл, сегодня, до какого дня; правила на день
/// услуги нет — «не отмечен», без срока.
pub(super) fn settle_look(lang: Lang, due: Option<Date>, today: Date) -> (String, &'static str) {
    match due {
        Some(due) if due < today => (t(lang, Key::SettleLate).to_owned(), "status late"),
        Some(due) if due == today => (t(lang, Key::SettleToday).to_owned(), "status waiting"),
        Some(due) => (fill(t(lang, Key::SettleBy), &[&i18n::date(lang, due)]), "status waiting"),
        None => (t(lang, Key::SettleNone).to_owned(), "status muted"),
    }
}

/// «3 документа» — по правилам множественного числа языка.
pub(super) fn documents_count(lang: Lang, count: usize) -> String {
    let n = count as u64;
    fill(
        plural(lang, n, Key::DocumentsOne, Key::DocumentsFew, Key::DocumentsMany),
        &[&i18n::integer(lang, count as i64)],
    )
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

/// Напоминание на главном экране (задача 23): документы, по которым пора
/// выписать расчётный документ в системе ՊԵԿ — срок наступил или прошёл, —
/// а отметки нет. Нет таких — карточки нет; часы без даты — тоже: без
/// сегодня не сказать, что пора. «Открыть список» открывает список только
/// документов без отметки. Зовёт её часть книги (`ui/tax.rs`), как
/// напоминание о сроках: срок — запись файла правил, а его разбору в главном
/// wasm не место (Правило 1).
#[component]
pub fn SettlementReminder(lang: ReadSignal<Lang>, documents: Documents, today: Option<Date>) -> impl IntoView {
    let count = Memo::new(move |_| match today {
        Some(today) => documents.with(|list, _, _| settlement::due_count(list, today)),
        None => 0,
    });
    move || {
        let count = count.get();
        (count > 0).then(|| view! {
            <div class="notice due" role="status">
                {icon(Icon::Document)}
                <p class="reminder">{move || {
                    let lang = lang.get();
                    fill(t(lang, Key::SettleRemind), &[&documents_count(lang, count)])
                }}</p>
                <a class="button" href=Screen::Documents.href() on:click=move |_| documents.show_unsettled()>
                    <span class="label">{tr(lang, Key::SettleOpen)}</span>
                </a>
            </div>
        })
    }
}

/// Счёт в списке «ждут оплаты».
#[component]
pub(super) fn AwaitingRow(
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
pub(super) fn NoticeLine(ctx: Ctx, busy: RwSignal<bool>, failure: RwSignal<Option<Problem>>) -> impl IntoView {
    let Ctx { lang, notice, .. } = ctx;
    move || {
        notice.get().map(|shown| match shown {
            Notice::Saved => view! { <p role="status">{tr(lang, Key::DocSaved)}</p> }.into_any(),
            Notice::Paid => view! { <p role="status">{tr(lang, Key::PaymentSaved)}</p> }.into_any(),
            Notice::PaymentReturned => view! { <p role="status">{tr(lang, Key::PaymentReturned)}</p> }.into_any(),
            Notice::PaymentRemoved(id, payment) => view! {
                <div class="notice-row">
                    <p role="status">{tr(lang, Key::PaymentRemoved)}</p>
                    <button
                        type="button"
                        class="secondary"
                        disabled=move || busy.get()
                        on:click=move |_| {
                            act(ctx, busy, failure, id, Change::AddPayment(payment), move |_| {
                                ctx.notice.set(Some(Notice::PaymentReturned));
                            });
                        }
                    >
                        <span class="label">{tr(lang, Key::DocUnannul)}</span>
                    </button>
                </div>
            }
            .into_any(),
            Notice::RateSaved => view! { <p role="status">{tr(lang, Key::DocRateSaved)}</p> }.into_any(),
            Notice::ActivitySaved => view! { <p role="status">{tr(lang, Key::DocActivitySaved)}</p> }.into_any(),
            Notice::IncomeKindSaved => view! { <p role="status">{tr(lang, Key::DocIncomeKindSaved)}</p> }.into_any(),
            Notice::Signed => view! { <p role="status">{tr(lang, Key::SignSaved)}</p> }.into_any(),
            Notice::Unsigned => view! { <p role="status">{tr(lang, Key::SignRemoved)}</p> }.into_any(),
            Notice::Settled => view! { <p role="status">{tr(lang, Key::SettleSaved)}</p> }.into_any(),
            Notice::Unsettled => view! { <p role="status">{tr(lang, Key::SettleRemoved)}</p> }.into_any(),
            Notice::Receipted => view! { <p role="status">{tr(lang, Key::ReceiptSaved)}</p> }.into_any(),
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
                            <span class="label">{tr(lang, Key::DocUnannul)}</span>
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
                        <span class="label">{tr(lang, Key::ClientRestore)}</span>
                    </button>
                </div>
            }
            .into_any(),
        })
    }
}
