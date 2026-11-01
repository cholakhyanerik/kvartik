//! Перенос вычета с кварталов до программы (задача 69): что записано и форма
//! на экране налога.
//!
//! Какие строки спросить, что записать и как пойдёт цепочка, решает ядро
//! (`kvartik_core::tax::opening`, `tax::turnover`); здесь — поля и одна
//! транзакция (CLAUDE.md, «Данные меняются одним путём»). Запись читается
//! вместе со снимками закрытых кварталов (`tax::Closings`): без неё расчёт
//! неполон.
//!
//! Форма — из части книги, как весь экран налога: своя часть у формы плодит
//! общие куски (CLAUDE.md, Правило 6). Открывается она на начале цепочки
//! (`QuarterTax::carry_unknown`), какой бы квартал ни был на экране: перенос
//! указывают на него.

use std::collections::BTreeMap;

use kvartik_core::i18n::{self, Key, Lang, fill, t};
use kvartik_core::model::activity::IncomeKind;
use kvartik_core::model::date::Quarter;
use kvartik_core::model::money::{Currency, Money};
use kvartik_core::tax::opening::{self, Opening};
use kvartik_core::tax::turnover::{QuarterTax, kind_key};
use leptos::prelude::*;

use super::documents::{number_problem, retype, shown_problem};
use super::form::TextField;
use super::storage::error_key;
use super::tax::{Ctx, drams};
use super::tr;
use crate::platform::clock;
use crate::store::{Store, StoreError, Writes};

/// Что не так с суммой в поле. Пустое поле — ноль: переноса у этой строки не
/// было.
fn problem(lang: Lang, text: &str) -> Option<String> {
    if text.trim().is_empty() {
        return None;
    }
    match i18n::parse_money(lang, text, Currency::Amd) {
        Ok(sum) if sum.minor() >= 0 => None,
        Ok(_) => Some(t(lang, Key::TaxOpeningNegative).to_owned()),
        Err(error) => {
            let example = i18n::amount(lang, Money::new(12_000, Currency::Amd));
            Some(number_problem(lang, error, &example))
        }
    }
}

/// Сохранить перенос: запись читается и меняется в одной транзакции. `None` —
/// ничего не изменилось.
async fn save(store: &Store, start: Quarter, carry: BTreeMap<IncomeKind, i64>) -> Result<Option<Opening>, StoreError> {
    let tx = store.write()?;
    let openings = tx.list::<Opening>().await?.records;
    let existing = Opening::current(&openings, start).cloned();
    let new = existing.is_none();
    // Такого форма не пропускает: это сбой, а не ошибка человека.
    let saved = Opening::save(existing, start, carry, clock::now()).map_err(|_| StoreError::Failed)?;
    let Some(opening) = saved else {
        return Ok(None);
    };
    let mut writes = Writes::default();
    if new {
        writes.insert(&opening);
    } else {
        writes.update(&opening);
    }
    tx.commit(writes).await?;
    Ok(Some(opening))
}

/// Перенос на экране налога: у квартала, на начало которого он указан, —
/// что записано, а пока он неизвестен — кнопка к форме.
#[component]
pub(super) fn OpeningEntry(ctx: Ctx, quarter: Memo<Quarter>, tax: Memo<Option<QuarterTax>>) -> impl IntoView {
    let lang = ctx.lang;
    let sources = ctx.sources;
    let shown = Memo::new(move |_| sources.opening(quarter.get()));
    let unknown = Memo::new(move |_| tax.with(|tax| tax.as_ref().and_then(|tax| tax.carry_unknown)));
    // Начало цепочки, на которое открыта форма; `None` — форма закрыта.
    let editing = RwSignal::new(None::<Quarter>);
    let saved = RwSignal::new(false);
    let unreadable = Memo::new(move |_| sources.closings.unreadable_openings());
    // Другой квартал — слова не о нём. Форму это не закрывает: она о начале
    // цепочки, а не о квартале на экране.
    Effect::watch(move || quarter.get(), move |_, _, _| saved.set(false), false);
    // Копию восстановили — открытая форма показывала бы прежнее, а записала
    // бы поверх восстановленного.
    Effect::watch(
        move || ctx.base.replacements(),
        move |_, _, _| {
            saved.set(false);
            editing.set(None);
        },
        false,
    );
    view! {
        {move || (unreadable.get() > 0).then(|| view! {
            <p role="alert">{move || {
                let lang = lang.get();
                fill(t(lang, Key::TaxOpeningUnreadable), &[&i18n::integer(lang, unreadable.get() as i64)])
            }}</p>
        })}
        {move || saved.get().then(|| view! { <p role="status">{tr(lang, Key::TaxOpeningSaved)}</p> })}
        {move || {
            if let Some(start) = editing.get() {
                return Some(view! { <OpeningForm ctx start editing saved /> }.into_any());
            }
            if let Some(opening) = shown.get() {
                let start = opening.quarter();
                return Some(view! {
                    <div class="tax-opening">
                        <Carried lang opening />
                        <button type="button" class="secondary" on:click=move |_| editing.set(Some(start))>
                            {tr(lang, Key::TaxOpeningChange)}
                        </button>
                    </div>
                }
                .into_any());
            }
            unknown.get().map(|start| view! {
                <div class="tax-opening">
                    <button type="button" class="secondary" on:click=move |_| editing.set(Some(start))>
                        {tr(lang, Key::TaxOpeningAsk)}
                    </button>
                </div>
            }
            .into_any())
        }}
    }
}

/// Что записано: перенос по строкам налога — или что его не было.
#[component]
fn Carried(lang: ReadSignal<Lang>, opening: Opening) -> impl IntoView {
    let rows: Vec<(IncomeKind, i64)> = opening.carry().iter().map(|(kind, amount)| (*kind, *amount)).collect();
    if rows.is_empty() {
        return view! { <p class="hint">{tr(lang, Key::TaxOpeningNone)}</p> }.into_any();
    }
    view! {
        <p class="hint">{tr(lang, Key::TaxOpeningShown)}</p>
        <dl class="pairs">
            {rows.into_iter().map(|(kind, amount)| view! {
                <div>
                    <dt>{tr(lang, kind_key(kind))}</dt>
                    <dd>{move || drams(lang.get(), amount)}</dd>
                </div>
            }).collect_view()}
        </dl>
    }
    .into_any()
}

/// Форма: по полю на строку налога, у которой на начало квартала `start` мог
/// быть перенос (`opening::kinds`). Открыта — новая версия ждёт.
#[component]
fn OpeningForm(ctx: Ctx, start: Quarter, editing: RwSignal<Option<Quarter>>, saved: RwSignal<bool>) -> impl IntoView {
    ctx.forms.hold();
    let lang = ctx.lang;
    let sources = ctx.sources;
    let known = sources.opening(start);
    let fields: Vec<(IncomeKind, RwSignal<String>)> = opening::kinds(start)
        .iter()
        .map(|kind| {
            let amount = known.as_ref().and_then(|opening| opening.carry().get(kind));
            let text = amount.map(|amount| i18n::amount(lang.get_untracked(), Money::new(*amount, Currency::Amd)));
            (*kind, RwSignal::new(text.unwrap_or_default()))
        })
        .collect();
    let tried = RwSignal::new(false);
    let busy = RwSignal::new(false);
    let failure = RwSignal::new(None::<StoreError>);
    // Сменили язык окна — суммы в полях записываются по-новому (`retype`).
    let retyped = fields.clone();
    Effect::watch(
        move || lang.get(),
        move |now: &Lang, before: Option<&Lang>, _| {
            if let Some(&before) = before {
                for (_, field) in &retyped {
                    retype(
                        *field,
                        |text| i18n::parse_money(before, text, Currency::Amd),
                        |sum| i18n::amount(*now, sum),
                    );
                }
            }
        },
        false,
    );
    let rows = fields.clone();
    let keep = move |_| {
        tried.set(true);
        if busy.get_untracked() {
            return;
        }
        let lang = lang.get_untracked();
        if fields.iter().any(|(_, field)| field.with_untracked(|text| problem(lang, text)).is_some()) {
            return;
        }
        let carry: BTreeMap<IncomeKind, i64> = fields
            .iter()
            .filter_map(|(kind, field)| {
                let sum = field.with_untracked(|text| i18n::parse_money(lang, text, Currency::Amd));
                Some((*kind, sum.ok()?.minor()))
            })
            .collect();
        let Some(store) = ctx.base.get() else {
            failure.set(Some(StoreError::Failed));
            return;
        };
        busy.set(true);
        failure.set(None);
        leptos::task::spawn_local(async move {
            let outcome = save(&store, start, carry).await;
            busy.set(false);
            match outcome {
                Ok(written) => {
                    match written {
                        Some(opening) => {
                            sources.closings.put_opening(opening);
                            ctx.backups.data_saved();
                            saved.set(true);
                        }
                        // То же уже в базе, а окно могло этого не знать: запись
                        // сохранила другая вкладка. Окно показывает, что в базе.
                        None => sources.closings.load(&store).await,
                    }
                    editing.set(None);
                }
                Err(error) => failure.set(Some(error)),
            }
        });
    };
    view! {
        <fieldset class="tax-opening">
            <legend>{move || fill(t(lang.get(), Key::TaxOpeningTitle), &[&i18n::quarter(lang.get(), start)])}</legend>
            <p class="hint">{move || {
                let lang = lang.get();
                fill(t(lang, Key::TaxOpeningAbout), &[&i18n::quarter(lang, start.previous().unwrap_or(start))])
            }}</p>
            {rows.into_iter().map(|(kind, value)| view! {
                <TextField
                    lang
                    label=kind_key(kind)
                    value
                    tried
                    inputmode="numeric"
                    autocomplete="off"
                    max=32
                    check=|_| None
                />
                {shown_problem(tried, move || value.with(|text| problem(lang.get(), text)))}
            }).collect_view()}
            {move || failure.get().map(|error| view! { <p role="alert">{tr(lang, error_key(error))}</p> })}
            <div class="actions">
                <button type="button" disabled=move || busy.get() on:click=keep>
                    {tr(lang, Key::TaxOpeningSave)}
                </button>
                <button type="button" class="secondary" on:click=move |_| editing.set(None)>
                    {tr(lang, Key::Cancel)}
                </button>
            </div>
        </fieldset>
    }
}
