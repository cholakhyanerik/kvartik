//! Пробный период на ИНН (задача 106): с какого дня он идёт на ՀՎՀՀ профиля,
//! по серверу своего сайта (`license::TRIAL_PATH`), — чтобы пробный период не
//! начинался заново ни в другом браузере, ни на другом устройстве, ни после
//! очистки данных (условия, п. 3, редакция от 18.11.2026).
//!
//! Спрашивает окно, только когда ответ что-то решает и человек согласился:
//! в профиле есть ՀՎՀՀ, ни своего ключа, ни места в ключе «Pro» сегодня нет
//! (`Licenses::watch`), а на сверку этого ՀՎՀՀ нажато «Согласен» ([`agree`]).
//! ՀՎՀՀ того, кто только пробует программу, уходит на сервер лишь с его
//! согласия (ՀՕ-49-Ն, ст. 8 ч. 1 п. 1; решение человека 2026-11-18), и без
//! согласия выпуск в пробный период заперт (`Licenses::consent_needed`) —
//! иначе тот, кто хочет обойти срок, просто не согласился бы. Купившему — ни
//! одного запроса (политика конфиденциальности, п. 2). Не чаще раза в час на
//! ՀՎՀՀ, и после перезагрузки тоже (`Device::trial_asked`), — при открытии и
//! при возвращении к окну, как курсы ЦБ (`ui/cba.rs`), а только что согласие
//! данное — сразу. Не ответил сервер — срок по тому, что устройство уже знает:
//! без сети работает всё (Правило 2). Отзыв согласия ([`withdraw`]) удаляет
//! запись на сервере и снова запирает выпуск в пробный период.
//!
//! Ответ — день. Он идёт в расчёт срока ([`day`], `Licenses::access`) и, если
//! раньше прежнего, в запись устройства (`Device::trial_tin`): со следующего
//! запуска его знает и главный wasm, без сети тоже. Последний ответ помнит
//! устройство вместе с ՀՎՀՀ (`Device::trial_asked`): после перезагрузки
//! экран лицензии сразу знает, почему срок короче. Код — в частях, которые
//! считают срок: главному wasm он не нужен (Правило 1).

use std::cell::{Cell, RefCell};

use kvartik_core::license;
use kvartik_core::model::date::Date;
use kvartik_core::model::tin::Tin;
use leptos::prelude::*;

use crate::store::device::{Asked, Device};

/// Как часто спрашивать сервер о том же ՀՎՀՀ: раз в час, как о курсах ЦБ
/// и новой версии.
const REFRESH_MS: i64 = 60 * 60 * 1000;

/// Согласий на устройстве — не больше: у бухгалтера ИП в конторе бывает
/// много, а накопитель ограничен (Правило 1); сверх — уходит самое давнее,
/// и о нём окно спросит снова.
const MAX_CONSENTS: usize = 100;

thread_local! {
    /// На какие ՀՎՀՀ на этом устройстве согласились и под какой версией слов
    /// согласия — с записи устройства (`Device::trial_consents`).
    static CONSENTS: ArcRwSignal<Vec<(String, String)>> = ArcRwSignal::new(Device::new().trial_consents());
    /// Идёт ли вопрос к серверу: только что данное согласие ждёт его ответа,
    /// прежде чем открыть выпуск, — иначе выпустили бы документ на ՀՎՀՀ,
    /// чей пробный период уже кончился в другом браузере.
    static ASKING: ArcRwSignal<bool> = ArcRwSignal::new(false);
    /// Последний ответ сервера: о каком ՀՎՀՀ и с какого дня на нём идёт пробный
    /// период. Один — ИП в окне один, и спрашивается ՀՎՀՀ его профиля; с
    /// записи устройства, чтобы после перезагрузки ответ был сразу.
    static ANSWER: ArcRwSignal<Option<(Tin, Date)>> = ArcRwSignal::new(
        Device::new().trial_asked().and_then(|asked| Some((Tin::parse(&asked.tin).ok()?, asked.first?))),
    );
    /// ՀՎՀՀ, о котором окну нужно знать: в профиле, а ключа сегодня нет.
    static WANTED: RefCell<Option<Tin>> = const { RefCell::new(None) };
    /// Слушатель возвращения к окну поставлен.
    static STARTED: Cell<bool> = const { Cell::new(false) };
}

/// Первый день пробного периода на ՀՎՀՀ `tin`, когда сервер о нём отвечал, —
/// отслеживаемое: ответ приходит позже расчёта срока.
pub(super) fn day(tin: &Tin) -> Option<Date> {
    ANSWER.with(|answer| answer.with(|answer| answer.as_ref().filter(|(known, _)| known == tin).map(|(_, day)| *day)))
}

/// Согласился ли человек на этом устройстве сверять ՀՎՀՀ `tin` с сервером под
/// нынешними словами согласия (`license::TRIAL_CONSENT`) — отслеживаемое.
pub(super) fn consented(tin: &Tin) -> bool {
    CONSENTS.with(|consents| {
        consents.with(|list| {
            list.iter()
                .any(|(known, version)| known == tin.as_str() && version == license::TRIAL_CONSENT)
        })
    })
}

/// «Согласен» на сверку ՀՎՀՀ `tin`: согласие — на устройство, и сервер
/// спрашивается сразу, не дожидаясь часа (расчёт срока зовёт [`want`] сам,
/// увидев согласие).
pub(super) fn agree(tin: &Tin) {
    let device = Device::new();
    if device.trial_asked().is_some_and(|asked| asked.tin == tin.as_str()) {
        device.clear_trial_asked();
    }
    CONSENTS.with(|consents| {
        consents.update(|list| {
            list.retain(|(known, _)| known != tin.as_str());
            if list.len() >= MAX_CONSENTS {
                list.remove(0);
            }
            list.push((tin.as_str().to_owned(), license::TRIAL_CONSENT.to_owned()));
            device.set_trial_consents(list);
        });
    });
}

/// Отозвать согласие на сверку ՀՎՀՀ `tin` (ՀՕ-49-Ն, ст. 9 ч. 3): окно больше
/// не спрашивает о нём сервер, а сервер удаляет его запись. `false` — сервер
/// не ответил: на устройстве согласие снято, а запись удалит продавец по
/// письму (политика конфиденциальности, п. 6).
pub(super) async fn withdraw(tin: Tin) -> bool {
    CONSENTS.with(|consents| {
        consents.update(|list| {
            list.retain(|(known, _)| known != tin.as_str());
            Device::new().set_trial_consents(list);
        });
    });
    crate::platform::site::post(license::TRIAL_PATH, &license::trial_withdraw(&tin))
        .await
        .is_some()
}

/// Идёт ли вопрос к серверу — отслеживаемое.
pub(super) fn asking() -> bool {
    ASKING.with(|asking| asking.get())
}

/// Окну нужно знать о пробном периоде на `tin`: `None` — ՀՎՀՀ в профиле нет,
/// ключ сегодня идёт или согласия нет, и спрашивать нельзя или незачем. Зовёт
/// расчёт срока (`Licenses::watch`) при каждом его изменении; спрашивает
/// сервер [`refresh`] — сейчас, если пора, и при каждом возвращении к окну.
pub(super) fn want(tin: Option<Tin>) {
    WANTED.with(|wanted| *wanted.borrow_mut() = tin);
    if !STARTED.replace(true) {
        crate::platform::when_shown(refresh);
    }
    refresh();
}

/// Спросить сервер о нужном ՀՎՀՀ, если о нём не спрашивали в этот час — и
/// в этом окне, и в прошлых. Момент запроса запоминается до ответа: не ответил
/// сервер — следующий раз через час, а не при каждом открытии. В браузерных
/// тестах сервера нет: ответ кладёт тест ([`set_for_tests`]).
fn refresh() {
    let Some(tin) = WANTED.with(|wanted| wanted.borrow().clone()) else {
        return;
    };
    if cfg!(all(test, target_arch = "wasm32")) {
        return;
    }
    let now = crate::platform::clock::now();
    let device = Device::new();
    let previous = device.trial_asked().filter(|asked| asked.tin == tin.as_str());
    // Часы, переведённые назад, делают прошлый запрос «будущим» — он не в счёт.
    if previous
        .as_ref()
        .is_some_and(|asked| (0..REFRESH_MS).contains(&(now.unix_millis() - asked.at.unix_millis())))
    {
        return;
    }
    device.set_trial_asked(&Asked {
        tin: tin.as_str().to_owned(),
        at: now,
        first: previous.and_then(|asked| asked.first),
    });
    ASKING.with(|asking| asking.set(true));
    leptos::task::spawn_local(async move {
        ask(tin, now).await;
        ASKING.with(|asking| asking.set(false));
    });
}

/// Спросить сервер о ՀՎՀՀ `tin` (спрошен в момент `at`) и запомнить ответ.
async fn ask(tin: Tin, at: kvartik_core::model::record::Timestamp) {
    let Some(text) = crate::platform::site::post(license::TRIAL_PATH, &license::trial_ask(&tin)).await else {
        return;
    };
    let Some(first) = license::trial_answer(&text) else {
        return;
    };
    let device = Device::new();
    if device.trial_tin().is_none_or(|known| first < known) {
        device.set_trial_tin(first);
    }
    device.set_trial_asked(&Asked {
        tin: tin.as_str().to_owned(),
        at,
        first: Some(first),
    });
    ANSWER.with(|answer| answer.set(Some((tin, first))));
}

/// Ответ сервера, который видит окно, — для браузерных тестов. Сигнал —
/// общий для всех окон прогона, а экраны снятых окон тестов ещё подписаны на
/// него: запись того же значения будила бы их посреди запуска следующего окна
/// (тест запуска с кончившимся сроком так и не успевал на экран лицензии).
/// Поэтому пишется только другое значение.
#[cfg(all(test, target_arch = "wasm32"))]
pub(super) fn set_for_tests(answer: Option<(Tin, Date)>) {
    ANSWER.with(|shared| {
        if shared.with_untracked(|known| *known != answer) {
            shared.set(answer);
        }
    });
}

/// Забыть согласия окна и устройства — для браузерных тестов: согласие
/// одного теста другому не достаётся. Как и [`set_for_tests`], сигнал
/// трогается, только когда есть что забыть.
#[cfg(all(test, target_arch = "wasm32"))]
pub(super) fn forget_consents_for_tests() {
    CONSENTS.with(|consents| {
        if consents.with_untracked(|list| !list.is_empty()) {
            consents.set(Vec::new());
        }
    });
    Device::new().set_trial_consents(&[]);
}
