//! Отметки выпущенного документа (задача 119 — файлом из `documents.rs`):
//! оплаты, подпись клиента под актом, отметка о расчётном документе в
//! системе ՊԵԿ или чеке ККМ и файл XML для системы ՊԵԿ.

use super::*;

/// Оплата счёта: поступления, сколько осталось и «Отметить оплату».
#[component]
pub(super) fn Payments(
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
    // Переплату программа не запрещает, но называет: в остатке её не видно.
    let overpaid = document.overpaid().ok().filter(|over| over.minor() > 0);
    let date = RwSignal::new(ctx.today.map(Date::iso).unwrap_or_default());
    let amount = RwSignal::new(
        left.filter(|left| left.minor() > 0)
            .map(|left| i18n::amount(lang.get_untracked(), left))
            .unwrap_or_default(),
    );
    // Сумма больше остатка и день из будущего — сразу, до записи, и записать
    // не мешают (`Doubts`): раньше опечатка закрывала счёт молча.
    let doubts = {
        let document = document.clone();
        let today = ctx.today;
        Memo::new(move |_| {
            let lang = lang.get();
            let day = date.with(|text| Date::from_iso(text));
            let sum = amount
                .with(|text| i18n::parse_money(lang, text, currency).ok())
                .map(Money::minor)
                .filter(|sum| *sum > 0);
            document.doubts(day, sum, today)
        })
    };
    let tried = RwSignal::new(false);
    let filled = (date.get_untracked(), amount.get_untracked());
    ctx.forms.hold_while(move || date.with(|day| *day != filled.0) || amount.with(|sum| *sum != filled.1));
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
                                    on:click=move |_| {
                                        act(ctx, busy, failure, id, Change::RemovePayment(payment), move |_| {
                                            notice.set(Some(Notice::PaymentRemoved(id, payment)));
                                        });
                                    }
                                >
                                    <span class="label">{tr(lang, Key::Remove)}</span>
                                </button>
                            </li>
                        }).collect_view()}
                    </ul>
                }
                .into_any()
            }}
            // У аннулированного оплачивать нечего: остатка нет, а поступления,
            // которые были, — видны (полный прогон 0.0.96).
            {left.filter(|left| open && left.minor() > 0).map(|left| view! {
                <p class="total">{move || fill(t(lang.get(), Key::PaymentLeft), &[&i18n::money(lang.get(), left)])}</p>
            })}
            {overpaid.map(|over| view! {
                <p class="total">{move || fill(t(lang.get(), Key::PaymentOverpaid), &[&i18n::money(lang.get(), over)])}</p>
            })}
            {open.then(|| view! {
                <DateField lang label=Key::PaymentDate value=date tried />
                {move || doubts.with(|doubts| doubts.ahead).then(|| view! {
                    <p class="warning" role="alert">{tr(lang, Key::PaymentAhead)}</p>
                })}
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
                {move || doubts.with(|doubts| doubts.over).map(|over| view! {
                    <p class="warning" role="alert">{move || {
                        let lang = lang.get();
                        fill(t(lang, Key::PaymentOver), &[&i18n::money(lang, over)])
                    }}</p>
                })}
                <button type="button" disabled=move || busy.get() on:click=record>
                    <span class="label">{tr(lang, Key::PaymentAdd)}</span>
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
pub(super) fn Signature(
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
    let filled = date.get_untracked();
    ctx.forms.hold_while(move || date.with(|day| *day != filled));
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
                            <span class="label">{tr(lang, Key::SignRemove)}</span>
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
                        <span class="label">{tr(lang, Key::SignMark)}</span>
                    </button>
                }
                .into_any(),
            }}
        </fieldset>
    })
}

/// Расчётный документ в системе ՊԵԿ (задача 23) — у документа дохода: пока
/// он не отмечен — до какого дня его выписать, номер и день выписки
/// и «Отметить, что выписан»; у отмеченного — день и номер и «Снять
/// отметку». У документа, выпущенного до 0.0.55, вместо срока — слова, что
/// программа не знает, выписан ли он (Правило 6): «срок прошёл» у него
/// звучал бы обвинением без оснований. У счёта с действующим актом отметка —
/// у акта: поставка одна (`ledger::income_documents`), и здесь только слова
/// об этом. Аннулированный показывает отметку, если она была, а действий над
/// ним нет. Выписан расчётный документ в квартале раньше услуги — и у
/// отметки, и под днём, который человек вводит, — слова, что доход мог уйти
/// в квартал подтверждения (`settlement::early`, задача 77).
#[component]
pub(super) fn SettlementMark(
    ctx: Ctx,
    document: Document,
    busy: RwSignal<bool>,
    failure: RwSignal<Option<Problem>>,
) -> impl IntoView {
    let Ctx { lang, notice, .. } = ctx;
    let profiles = ctx.profiles;
    let id = document.id();
    let open = document.annulled().is_none();
    let mark = document.settlement().cloned();
    if mark.is_none() && !open {
        return None;
    }
    let covered = covered_by_act(ctx, &document);
    let known = document.settlement_known();
    let for_file = document.clone();
    let service = document.data().service_date.quarter();
    // У покупателя ИНН (ՀՎՀՀ) — фирма или ИП, а чек вместо документа закон
    // разрешает в рознице и при работах и услугах населению (ст. 56, ч. 2):
    // окно напоминает условие, а кнопку не прячет — «население» закон не
    // определяет (ревью задачи 76).
    let firm = document.issued().is_some_and(|issued| issued.buyer().tin.is_some());
    let checked = StoredValue::new(document.clone());
    // Квартал выписки раньше квартала услуги; пока профиль читается — молчит:
    // без кодов строку налога не узнать (`settlement::early`).
    let early = move |day: Date, kind: SettlementKind| {
        profiles.with(|profile| checked.with_value(|document| settlement::early(document, profile, day, kind)))
    };
    // Последний день — по правилу на день оказания услуги; правила нет —
    // о сроке и о правиле окно молчит, а не пугает (Правило 6).
    let due = settlement::due(document.data().service_date);
    let late = matches!((due, ctx.today), (Some(due), Some(today)) if due < today);
    let date = RwSignal::new(ctx.today.map(Date::iso).unwrap_or_default());
    let number = RwSignal::new(String::new());
    let tried = RwSignal::new(false);
    let filled = date.get_untracked();
    ctx.forms.hold_while(move || date.with(|day| *day != filled) || number.with(|text| !text.is_empty()));
    // Отметить расчётный документ вида `kind`: в системе ՊԵԿ или чек ККМ
    // (задача 76) — тем же днём и номером из полей.
    let save = move |kind: SettlementKind| {
        tried.set(true);
        let Some(day) = Date::from_iso(&date.get_untracked()) else {
            return;
        };
        let Ok(number) = Line::new(&number.get_untracked()) else {
            return;
        };
        let said = match kind {
            SettlementKind::System => Notice::Settled,
            SettlementKind::Receipt => Notice::Receipted,
        };
        act(ctx, busy, failure, id, Change::Settle(Settlement { day, number, kind }), move |_| {
            notice.set(Some(said));
        });
    };
    let remove = move |_| {
        act(ctx, busy, failure, id, Change::Unsettle, move |_| {
            notice.set(Some(Notice::Unsettled));
        });
    };
    Some(view! {
        <fieldset class="settlement">
            <legend>{tr(lang, Key::SettleTitle)}</legend>
            // У счёта, закрытого актом, — только слова, что отметка у акта:
            // своя отметка счёта могла устареть, пока её правили у акта
            // (ревью задачи 23).
            {move || if covered.get() {
                view! { <p class="hint">{tr(lang, Key::SettleByAct)}</p> }.into_any()
            } else if let Some(mark) = mark.clone() {
                let (settled, kind) = (mark.day, mark.kind);
                let text = move || {
                    let lang = lang.get();
                    let day = i18n::date(lang, mark.day);
                    let (plain, numbered) = match kind {
                        SettlementKind::System => (Key::SettledOn, Key::SettledOnNumbered),
                        SettlementKind::Receipt => (Key::ReceiptOn, Key::ReceiptOnNumbered),
                    };
                    if mark.number.is_empty() {
                        fill(t(lang, plain), &[&day])
                    } else {
                        fill(t(lang, numbered), &[&day, mark.number.as_str()])
                    }
                };
                view! {
                    <p>{text}</p>
                    {move || early(settled, kind).map(|quarter| view! {
                        <p class="warning">{move || early_words(lang.get(), quarter, service)}</p>
                    })}
                    {open.then(|| view! {
                        <button type="button" class="secondary" disabled=move || busy.get() on:click=remove>
                            <span class="label">{tr(lang, Key::SignRemove)}</span>
                        </button>
                    })}
                }
                .into_any()
            } else {
                view! {
                    {due.is_some().then(|| view! { <p class="hint">{tr(lang, Key::SettleAbout)}</p> })}
                    {if known {
                        due.map(|due| view! {
                            <p class:warning=late>{move || {
                                let lang = lang.get();
                                let key = if late { Key::SettleDueWas } else { Key::SettleDueOn };
                                fill(t(lang, key), &[&i18n::date(lang, due)])
                            }}</p>
                        })
                        .into_any()
                    } else {
                        view! { <p>{tr(lang, Key::SettleUnknown)}</p> }.into_any()
                    }}
                    // Выписан ли документ до 0.0.55, неизвестно, и второй
                    // черновик после выписанного удвоил бы поставку (ревью
                    // задачи 44).
                    {known.then(|| view! { <SettlementFile ctx document=for_file.clone() /> })}
                    <TextField
                        lang
                        label=Key::SettleNumber
                        value=number
                        tried
                        autocomplete="off"
                        hint=Key::SettleNumberHint
                    />
                    <DateField lang label=Key::SettleDate value=date tried />
                    {move || {
                        let quarter = early(Date::from_iso(&date.get())?, SettlementKind::System)?;
                        Some(view! {
                            <p class="warning" role="alert">{move || early_words(lang.get(), quarter, service)}</p>
                        })
                    }}
                    <button type="button" disabled=move || busy.get() on:click=move |_| save(SettlementKind::System)>
                        <span class="label">{tr(lang, Key::SettleMark)}</span>
                    </button>
                    // Чек ККМ вместо документа в системе (задача 76): кто
                    // покупатель, программа не знает, и решает человек.
                    <p class="hint">{tr(lang, Key::SettleReceiptAbout)}</p>
                    {firm.then(|| view! { <p class="hint">{tr(lang, Key::SettleReceiptFirm)}</p> })}
                    <button
                        type="button"
                        class="secondary"
                        disabled=move || busy.get()
                        on:click=move |_| save(SettlementKind::Receipt)
                    >
                        <span class="label">{tr(lang, Key::SettleReceipt)}</span>
                    </button>
                }
                .into_any()
            }}
        </fieldset>
    })
}

/// Слова о расчётном документе, выписанном в квартале `settled` раньше
/// квартала услуги `service` (`settlement::early`, задача 77).
pub(super) fn early_words(lang: Lang, settled: Quarter, service: Quarter) -> String {
    fill(t(lang, Key::SettleEarly), &[&i18n::quarter(lang, settled), &i18n::quarter(lang, service)])
}

/// Файл черновика расчётного документа для системы ՊԵԿ (задача 44): XML по
/// схеме ՊԵԿ (`format::accdoc`) ссылкой, файл которой собирается в самом
/// нажатии, как выгрузка для бухгалтера (`ui/ledger/export.rs`): ссылка
/// получает адрес файла ещё в обработчике, и скачивание идёт от того же
/// нажатия. Чего не хватает файлу — словами до нажатия. Профиль нужен строке
/// налога и режиму (документ о товарах или после ухода с режима — не сюда),
/// реквизиты — из копии на день выпуска; пока профиль не прочитан, ни ссылки,
/// ни слов: без него строку не узнать, а ссылка молчала бы.
#[component]
pub(super) fn SettlementFile(ctx: Ctx, document: Document) -> impl IntoView {
    let lang = ctx.lang;
    let profiles = ctx.profiles;
    let id = document.id();
    let name = accdoc::file_name(&document).unwrap_or_default();
    let document = StoredValue::new(document);
    let checked = Memo::new(move |_| {
        profiles.ready().then(|| {
            profiles.with(|profile| document.with_value(|document| accdoc::accdoc(document, profile).err()))
        })
    });
    // Файл держит адрес `blob:`, пока его не сменит следующий
    // (`platform::file::Download`).
    let file = StoredValue::new_local(None::<Download>);
    let node = NodeRef::<A>::new();
    let click = move |event: MouseEvent| {
        let made = untrack(|| {
            profiles.with(|profile| {
                document.with_value(|document| {
                    let text = accdoc::accdoc(document, profile).ok()?;
                    Download::text(&text, &accdoc::file_name(document)?)
                })
            })
        });
        match (made, node.get_untracked()) {
            (Some(download), Some(node)) if node.set_attribute("href", download.url()).is_ok() => {
                file.set_value(Some(download));
            }
            _ => event.prevent_default(),
        }
    };
    move || {
        let key = match checked.get()? {
            None => {
                return Some(view! {
                    <p class="hint">{tr(lang, Key::SettleFileAbout)}</p>
                    <a
                        class="button secondary"
                        node_ref=node
                        href=Screen::Document(id).href()
                        download=name.clone()
                        on:click=click
                    >
                        {icon(Icon::Download)}
                        <span>{tr(lang, Key::SettleFile)}</span>
                    </a>
                }
                .into_any());
            }
            Some(AccDocError::Goods) => Key::SettleFileGoods,
            Some(AccDocError::KindUnknown) => Key::SettleFileKind,
            Some(AccDocError::Closed) => Key::SettleFileClosed,
            Some(AccDocError::NoRate) => Key::SettleFileRate,
            Some(AccDocError::OldRate) => Key::SettleFileOld,
            Some(AccDocError::NoSellerTin | AccDocError::NoBuyerTin | AccDocError::BuyerUnknown) => Key::SettleFileTin,
            Some(AccDocError::TooLong(_)) => Key::SettleFileLong,
            // У выпущенного действующего документа этого не бывает.
            Some(AccDocError::NotIssued | AccDocError::Overflow) => return None,
        };
        Some(view! { <p class="hint">{tr(lang, key)}</p> }.into_any())
    }
}
