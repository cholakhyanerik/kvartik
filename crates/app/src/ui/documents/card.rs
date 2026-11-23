//! Карточка выпущенного документа (задача 119 — файлом из `documents.rs`):
//! реквизиты, курс, вид деятельности и строка налога, строки и история,
//! а внизу — действия: PDF, акт по счёту, повтор и аннулирование. Оплаты,
//! подпись и отметка ՊԵԿ — в `marks.rs`, страницы PDF — `ui/paper.rs`.

use super::*;

/// Выпущенный документ карточкой (дизайн «Kvartik Design»): статус, итог,
/// клиент и даты, главное действие, строки, оплаты, история и редкие
/// действия внизу. Документ читается из списка со слежением: оплата
/// и аннулирование показываются сразу.
#[component]
pub(super) fn DocView(ctx: Ctx, id: RecordId) -> impl IntoView {
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
            <SettlementMark ctx document=shown.clone() busy failure />
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
pub(super) fn DocCard(
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
    let staged = staged_acts(ctx, &document);
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
                // Аннулированный ни о чём не просит: «Оплатить до» у него читалось
                // бы как ещё действующий срок (полный прогон 0.0.96).
                {data.due.filter(|_| open).map(|due| view! {
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
            // Акты на весь счёт — «закрывают его целиком»: «на X, а сам счёт
            // — на X» звучало бы расхождением (полный прогон 2026-11-10, п. 10).
            {move || staged.get().and_then(|staged| Some((staged.worth_saying()?, staged.whole))).map(|(acted, whole)| view! {
                <p class="hint">{move || {
                    let lang = lang.get();
                    if acted == whole {
                        t(lang, Key::DocActsWhole).to_owned()
                    } else {
                        fill(t(lang, Key::DocActsSoFar), &[&i18n::money(lang, acted), &i18n::money(lang, whole)])
                    }
                }}</p>
            })}
        </div>
        {move || staged.get().and_then(Staged::over).map(|over| view! {
            <p role="alert">{move || {
                let lang = lang.get();
                fill(t(lang, Key::DocActsOver), &[&i18n::money(lang, over)])
            }}</p>
        })}
        {move || staged.get().is_some_and(|staged| staged.acted.is_none()).then(|| view! {
            <p role="alert">{tr(lang, Key::DocActsMixed)}</p>
        })}
        {(awaiting && left.is_some() && ctx.today.is_some()).then(|| view! {
            <button type="button" class="wide" disabled=move || busy.get() on:click=mark_paid>
                {icon(Icon::Check)}
                <span class="label">{tr(lang, Key::DocMarkPaid)}</span>
            </button>
        })}
        <div class="actions doc-next">
            // Акт — пока по счёту что-то осталось: акты вместе не больше
            // счёта (задача 84), и кнопка, которую выпуск отвергнет, путала бы.
            // Прячется атрибутом, а не пересоздаётся: кнопка с обработчиком,
            // которую можно снять, легла бы своим куском wasm (замер задачи 84).
            {(invoice && open).then(|| view! {
                <button
                    type="button"
                    class="secondary"
                    hidden=move || staged.get().is_some_and(|staged| !staged.room())
                    on:click=move |_| {
                        notice.set(None);
                        mode.set(Mode::ActFor(id));
                    }
                >
                    <span class="label">{tr(lang, Key::DocActFromInvoice)}</span>
                </button>
            })}
            <button
                type="button"
                class="secondary"
                aria-expanded=move || paper.get().to_string()
                on:click=move |_| paper.update(|shown| *shown = !*shown)
            >
                {icon(Icon::Download)}
                <span class="label">{tr(lang, Key::DocPdf)}</span>
            </button>
        </div>
        {move || paper.get().then(|| view! {
            <PaperPanel lang base=ctx.base request=request.clone() close=move || paper.set(false) />
        })}
    }
}

/// Курс ЦБ, вид деятельности и строка налога выпущенного документа — для книги
/// и налога. У счёта с действующим актом доход — по акту
/// (`ledger::income_documents`): курс и вид у такого счёта ни на что не влияют,
/// и карточка говорит, что они у акта, а не предлагает их (дефект
/// 0.0.27–0.0.47: «Указать курс ЦБ» был и у такого счёта, и курс молча не
/// доходил до книги).
#[component]
pub(super) fn ForBook(
    ctx: Ctx,
    document: Document,
    busy: RwSignal<bool>,
    failure: RwSignal<Option<Problem>>,
) -> impl IntoView {
    let lang = ctx.lang;
    let data = document.data();
    let live = document.annulled().is_none();
    let foreign = data.currency != Currency::Amd && live;
    let covered = covered_by_act(ctx, &document);
    move || {
        if covered.get() {
            view! {
                <div class="rate-box">
                    <p class="hint">{tr(lang, Key::DocIncomeByAct)}</p>
                </div>
            }
            .into_any()
        } else {
            view! {
                {foreign.then(|| view! { <DocRate ctx document=document.clone() busy failure /> })}
                {live.then(|| view! { <DocActivity ctx document=document.clone() busy failure /> })}
                {live.then(|| view! { <DocIncomeKind ctx document=document.clone() busy failure /> })}
            }
            .into_any()
        }
    }
}

/// Акты-этапы выпущенного действующего счёта (задача 84): сколько их, на
/// сколько они выпущены и сколько сам счёт, в его валюте. `acted` — `None`,
/// когда среди актов есть акт в другой валюте (так выпускалось до 0.0.75):
/// сумму не назвать, и нового акта по счёту выпуск не пустит.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) struct Staged {
    count: usize,
    acted: Option<Money>,
    whole: Money,
}

impl Staged {
    /// Осталось ли по счёту на акт: сумма актов названа и меньше счёта.
    fn room(&self) -> bool {
        self.acted.is_some_and(|acted| acted.minor() < self.whole.minor())
    }

    /// Сумма актов, если о ней есть что сказать: актов больше одного или они
    /// не на весь счёт. Один акт на весь счёт — обычный случай, и о нём
    /// говорит `DocIncomeByAct` (ревью задачи 84).
    fn worth_saying(&self) -> Option<Money> {
        let acted = self.acted?;
        (self.count > 1 || acted != self.whole).then_some(acted)
    }

    /// На сколько акты больше счёта — так выпускалось до 0.0.75; `None` — не
    /// больше или сумму не назвать.
    fn over(self) -> Option<Money> {
        let over = self.acted?.minor().checked_sub(self.whole.minor())?;
        (over > 0).then(|| Money::new(over, self.whole.currency()))
    }
}

/// Акты-этапы счёта (`Document::acts`): `None` — актов нет, документ не
/// выпущенный действующий счёт или документы ещё читаются. Отслеживаемое.
pub(super) fn staged_acts(ctx: Ctx, document: &Document) -> Memo<Option<Staged>> {
    let live = document.data().kind == DocKind::Invoice && document.issued().is_some() && document.annulled().is_none();
    let invoice = live.then(|| document.clone());
    Memo::new(move |_| {
        let invoice = invoice.as_ref()?;
        let whole = invoice.total().ok()?;
        ctx.documents.with(|documents, _, _| {
            let count = invoice.acts(documents).count();
            (count > 0).then(|| Staged {
                count,
                acted: invoice.acted(documents).ok(),
                whole,
            })
        })
    })
}

/// Закрыт ли счёт действующим актом: доход тогда — по акту
/// (`ledger::income_documents`), и курс с видом деятельности у счёта ни на
/// что не влияют. Пока документы читаются — нет. Отслеживаемое.
pub(super) fn covered_by_act(ctx: Ctx, document: &Document) -> Memo<bool> {
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
pub(super) fn DocRate(
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
    ctx.forms.hold_while(move || editing.get());
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
    let ours = Signal::derive(move || Some(currency));
    let day = Signal::derive(move || Some(service));
    // Курса нет, а курс ЦБ для дня услуги есть — «Указать курс ЦБ» одним
    // нажатием, с курсом в подписи (задача 43).
    let found = cba::hint(ours, day);
    let quick = move || match (rate, found.get()) {
        (None, Some(cba::Hint::Rate { per_unit_e4, .. })) => Some(view! {
            <button
                type="button"
                disabled=move || busy.get()
                on:click=move |_| {
                    act(ctx, busy, failure, id, Change::SetRate(Some(per_unit_e4)), move |_| {
                        notice.set(Some(Notice::RateSaved));
                    });
                }
            >
                <span class="label">{move || {
                    let lang = lang.get();
                    fill(t(lang, Key::DocRateCba), &[&i18n::rate_pair(lang, currency, per_unit_e4)])
                }}</span>
            </button>
        }),
        _ => None,
    };
    view! {
        <div class="rate-box">
            {move || if editing.get() {
                view! {
                    <label>
                        {move || fill(t(lang.get(), Key::OpRate), &[currency_code(currency)])}
                        <input type="text" inputmode="decimal" autocomplete="off" maxlength="24" bind:value=typed />
                    </label>
                    <CbaHint lang currency=ours day field=typed />
                    <p class="hint">{tr(lang, Key::RateHint)}</p>
                    {shown_problem(tried, move || typed.with(|text| rate_problem(lang.get(), text)))}
                    <p class="hint">{move || {
                        let lang = lang.get();
                        typed.with(|text| equivalent(lang, total, text, Some(service)))
                    }}</p>
                    <div class="actions">
                        <button type="button" disabled=move || busy.get() on:click=save>
                            <span class="label">{tr(lang, Key::DocRateSave)}</span>
                        </button>
                        <button type="button" class="secondary" on:click=move |_| editing.set(false)>
                            <span class="label">{tr(lang, Key::Cancel)}</span>
                        </button>
                    </div>
                }
                .into_any()
            } else {
                view! {
                    {quick}
                    <button type="button" class="secondary" on:click=move |_| editing.set(true)>
                        <span class="label">{tr(lang, if rate.is_some() { Key::DocRateChange } else { Key::DocRateSet })}</span>
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
pub(super) fn DocActivity(
    ctx: Ctx,
    document: Document,
    busy: RwSignal<bool>,
    failure: RwSignal<Option<Problem>>,
) -> impl IntoView {
    let Ctx { lang, notice, .. } = ctx;
    let id = document.id();
    let saved = document.data().activity;
    let service = document.data().service_date;
    // Доходу со своей строкой код не нужен (`tax::turnover`).
    let free = document.data().income_kind.is_some();
    let chosen = RwSignal::new(saved);
    let date = RwSignal::new(service.iso());
    let tried = RwSignal::new(false);
    ctx.forms.hold_while(move || chosen.get() != saved);
    let activities = Memo::new(move |_| ctx.profiles.activities());
    let needed = Memo::new(move |_| !free && ctx.profiles.with(|profile| turnover::activity_needed(profile, service)));
    let save = move |_| {
        tried.set(true);
        let activity = chosen.get_untracked();
        if activity.is_none() && needed.get_untracked() {
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
                            <span class="label">{tr(lang, Key::DocActivitySave)}</span>
                        </button>
                    })}
                </div>
            }
        })
    }
}

/// Строка налога дохода выпущенного документа (задача 70): свёрнутым вопросом,
/// и сам выбор — уже команда. С отдельным «Сохранить», как у вида деятельности,
/// выбор пропадал бы молча от любого соседнего действия: карточка строится
/// заново на каждое изменение документа, а о пропущенной строке, в отличие от
/// вида, окну сказать нечего — `None` законно. В PDF её нет: выпущенный документ
/// от неё не меняется (`Document::set_income_kind`). У счёта с действующим
/// актом её нет вовсе (`ForBook`).
#[component]
pub(super) fn DocIncomeKind(
    ctx: Ctx,
    document: Document,
    busy: RwSignal<bool>,
    failure: RwSignal<Option<Problem>>,
) -> impl IntoView {
    let Ctx { lang, notice, .. } = ctx;
    let id = document.id();
    let saved = document.data().income_kind;
    let chosen = RwSignal::new(saved);
    let date = RwSignal::new(document.data().service_date.iso());
    Effect::watch(
        move || chosen.get(),
        move |now: &Option<IncomeKind>, _, _| {
            if *now == saved {
                return;
            }
            // Другое действие ещё пишет: `act` молча откажет, а выбор на
            // экране не должен выглядеть сохранённым.
            if busy.get_untracked() {
                chosen.set(saved);
                return;
            }
            act(ctx, busy, failure, id, Change::SetIncomeKind(*now), move |_| {
                notice.set(Some(Notice::IncomeKindSaved));
            });
        },
        false,
    );
    view! {
        <div class="rate-box">
            <IncomeKindChoice lang profiles=ctx.profiles chosen date />
        </div>
    }
}

/// Сколько дней до срока словами: «сегодня», «5 дней».
pub(super) fn days_text(lang: Lang, days: i32) -> String {
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
pub(super) fn DocItems(ctx: Ctx, document: Document) -> impl IntoView {
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
pub(super) enum Event {
    Issued,
    Paid(i64),
    Signed,
    /// Расчётный документ выписан в системе ՊԵԿ (задача 23) или выдан чек
    /// ККМ (задача 76).
    Settled(SettlementKind),
    Annulled,
}

/// Что было с документом — свежее сверху (дизайн «Kvartik Design»): выпуск,
/// поступления оплаты, подпись акта клиентом, расчётный документ в системе
/// ՊԵԿ, аннулирование. Дата выпуска — дата документа: момент выпуска
/// программа не хранит, а в документе написана она.
#[component]
pub(super) fn History(ctx: Ctx, document: Document) -> impl IntoView {
    let lang = ctx.lang;
    let number = document.number().map(|number| number.to_string()).unwrap_or_default();
    let currency = document.data().currency;
    // В один день: аннулирование — позже оплат, подписи и расчётного
    // документа, они — позже выпуска.
    let mut events = vec![(document.data().date, 0, Event::Issued)];
    events.extend(document.payments().iter().map(|payment| (payment.date, 1, Event::Paid(payment.amount))));
    if let Some(day) = document.signed() {
        events.push((day, 1, Event::Signed));
    }
    if let Some(settlement) = document.settlement() {
        events.push((settlement.day, 1, Event::Settled(settlement.kind)));
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
                        Event::Settled(SettlementKind::System) => t(lang, Key::HistorySettled).to_owned(),
                        Event::Settled(SettlementKind::Receipt) => t(lang, Key::HistoryReceipt).to_owned(),
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

/// Редкие действия с выпущенным документом — внизу карточки, под «Ещё»:
/// повтор счёта и аннулирование. Акт по счёту — в карточке, рядом с главным
/// действием.
#[component]
pub(super) fn DocActions(
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
                                <span class="label">{tr(lang, Key::RecurStop)}</span>
                            </button>
                        }
                        .into_any()
                    }
                    None => view! {
                        <p class="hint">{tr(lang, Key::RecurAbout)}</p>
                        <button type="button" class="secondary" disabled=move || busy.get() on:click=start>
                            <span class="label">{tr(lang, Key::RecurStart)}</span>
                        </button>
                    }
                    .into_any(),
                }}
            </div>
        })}
        <div class="actions">
            <button type="button" class="secondary" disabled=move || busy.get() on:click=annul>
                <span class="label">{tr(lang, Key::DocAnnul)}</span>
            </button>
        </div>
    })
}

/// Запустить повтор счёта и сразу создать черновик, если его месяц уже
/// наступил, — одной транзакцией.
pub(super) async fn start_repeat(
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
            Some(before) => writes.update(before, &recurring),
        }
    }
    for draft in &drafts {
        writes.insert(draft);
    }
    tx.commit(writes).await?;
    Ok((recurring, drafts))
}

/// Остановить повтор — одной транзакцией. `Ok(None)` — уже остановлен.
pub(super) async fn stop_repeat(store: &Store, id: RecordId) -> Result<Option<Recurring>, Problem> {
    let tx = store.write()?;
    let Some(recurring) = tx.get::<Recurring>(id).await? else {
        return Ok(None);
    };
    let Some(stopped) = recurring.stop(clock::now()) else {
        return Ok(None);
    };
    let mut writes = Writes::default();
    writes.update(&recurring, &stopped);
    tx.commit(writes).await?;
    Ok(Some(stopped))
}
