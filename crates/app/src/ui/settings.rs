//! Настройки (задача 88): выпуск на этом устройстве (задача 41), кружок
//! у курсора и свои цвета окна.
//!
//! Все — на устройстве (`store/device.rs`), а не в базе: это не данные
//! человека, и в копию они не идут, как язык окна. Выпуск — потому что
//! номера документов выдаёт одно устройство (`Document::next_number` знает
//! только свою базу): копия с телефона, где выпуск выключен, восстановленная
//! на основном компьютере, не выключила бы выпуск на нём самом. Кружок по
//! умолчанию выключен (решение человека 2026-11-09), и лендинг читает ту же
//! запись: у его страниц кружок тот же.
//!
//! Свои цвета — фон, кнопки, текст на кнопках и текст — действуют всегда,
//! и днём, и ночью: окно пишет на `<html>` всю палитру (`platform::look`),
//! и тема устройства её не меняет (решение человека 2026-11-09). Какую,
//! считает ядро (`kvartik_core::theme`): невыбранное — у той темы
//! программы, к которой ближе фон, производное — дотянуто до порога WCAG,
//! а плохо читаемую выбранную пару экран называет числом, но цвет применяет.
//!
//! Посчитанная палитра лежит в записи устройства целиком, и при открытии
//! главный wasm только ставит её ([`Look::start`]) — до первой отрисовки,
//! без расчёта: разбор `tokens.css` и расчёт — в части настроек, вместе с её
//! экраном (Правило 1). После обновления программы цвета `tokens.css` могли
//! смениться, поэтому части, загрузившись, пересчитывают палитру по выбору
//! ([`refresh`]).
//!
//! До запуска wasm палитру ставит стиль-копия, который service worker
//! вписывает в страницу (задача 89, `platform::look`): иначе первый кадр был
//! бы в цветах программы. Копия идёт вслед за записью устройства ([`copy`]),
//! а читается при открытии одна запись: копия, разошедшаяся с ней — запись
//! в кэш не удалась, — видна по отметке ссылки, и [`refresh`] её
//! переписывает. Первый заход и браузер без service worker — в цветах
//! программы, пока wasm не запустился.
//!
//! Кружок — атрибутом `data-cursor` на `<html>` (`platform::look`), а не
//! разметкой: кружок с условием в корне окна стоил главному wasm лишнего
//! кода больше, чем атрибут (замер задачи 88).

use std::cell::RefCell;

use kvartik_core::i18n::{self, Key, Lang, fill, t};
use kvartik_core::theme::{self, Choice, Low, Paint, Pair, Palette, Pick, Record, Rgb, Sheet};
use leptos::prelude::*;
use web_sys::HtmlInputElement;

use crate::platform::look;
use crate::store::device::Device;

use super::backup::{BackupChip, Backups};
use super::icon::{Icon, icon};
use super::route::{Router, Screen};
use super::split::{screen, split};
use super::tr;

/// Палитра программы — тот же файл, что рисует окно: расчёт берёт из него
/// обе темы, и разойтись с ним он не может.
const TOKENS: &str = include_str!("../../style/tokens.css");

/// Как выглядит окно на этом устройстве — одно на всё окно.
#[derive(Clone, Copy)]
pub struct Look {
    /// Включён ли кружок у курсора.
    cursor: RwSignal<bool>,
}

impl Look {
    /// При открытии, до первой отрисовки: свои цвета — на страницу сразу,
    /// готовой палитрой из записи устройства (`theme::painted`: главному wasm
    /// не нужен ни разбор выбора, ни расчёт — Правило 1); кружок — каким его
    /// оставили. Запись не читается — окно в цветах программы, а экран
    /// настроек начнёт с них. Ссылку на копию палитры, которую вписал service
    /// worker, снимает часть настроек ([`refresh`]): палитра на `<html>`
    /// сильнее любого стиля, и до неё ссылка ничего не меняет.
    pub fn start() -> Self {
        let device = Device::new();
        let record = device.colors();
        if let Some((scheme, colors)) = record.as_deref().and_then(theme::painted) {
            look::put(scheme, &colors);
        }
        let on = device.cursor();
        look::show_cursor(on);
        Self {
            cursor: RwSignal::new(on),
        }
    }

    /// Включён ли кружок у курсора: место ему `ui/motion.rs` пишет, только
    /// пока он включён.
    pub fn cursor(self) -> RwSignal<bool> {
        self.cursor
    }

    /// Включить или выключить кружок: сразу и на этом устройстве.
    fn set_cursor(self, on: bool) {
        Device::new().set_cursor(on);
        look::show_cursor(on);
        self.cursor.set(on);
    }
}

/// Строка «Настройки» в группе «Программа» на экране профиля.
#[component]
pub fn SettingsEntry(lang: ReadSignal<Lang>) -> impl IntoView {
    view! {
        <a class="entry" href=Screen::Settings.href()>
            <span class="entry-main">
                <span class="entry-title">{tr(lang, Key::SettingsTitle)}</span>
                <span class="entry-meta">{tr(lang, Key::SettingsAbout)}</span>
            </span>
            {icon(Icon::Forward)}
        </a>
    }
}

/// Экран настроек на языке окна: заголовок — сразу, остальное — частью
/// с первого показа (`split::screen`). «Назад» нет: на компьютере это свой
/// раздел меню, а на телефоне вкладки внизу видны всегда.
#[component]
pub fn SettingsScreen(lang: ReadSignal<Lang>, look: Look, backups: Backups, router: Router) -> impl IntoView {
    view! {
        <h1 tabindex="-1">{tr(lang, Key::SettingsTitle)}</h1>
        <BackupChip lang backups nudge=true />
        {screen(lang, router, Screen::Settings, move || settings_part(Job::Page(lang, look)))}
    }
}

/// Пересчитать свои цвета после открытия — частью настроек ([`Job::Refresh`]),
/// и копию для первого кадра — вслед за ними. Зовёт загрузка частей
/// (`split::preload`), последней: часть уже в памяти. Зовут её у всех: есть ли
/// свои цвета и пришла ли страница с копией, часть узнаёт сама: проверка
/// копии здесь стоила главному wasm байтов (замер задачи 89, CLAUDE.md,
/// Правило 6), а у кого нет ни того, ни другого, часть ничего не делает.
pub async fn refresh() {
    let _ = settings_part(Job::Refresh).await;
}

/// Что делает часть настроек.
#[derive(Clone, Copy)]
pub(super) enum Job {
    /// Нарисовать экран.
    Page(ReadSignal<Lang>, Look),
    /// Пересчитать палитру записи по выбору (`repaint`) и сверить с ней
    /// копию, с которой пришла страница.
    Refresh,
}

split! {
    /// Экран настроек и расчёт своих цветов — своей частью wasm
    /// (`ui/split.rs`): разбор `tokens.css` и расчёт палитры нужны только
    /// тому, кто открыл настройки или выбрал свои цвета.
    pub(super) fn settings_part(job: Job) -> AnyView, preload preload_settings {
        match job {
            Job::Page(lang, look) => view! { <SettingsPage lang look /> }.into_any(),
            Job::Refresh => {
                let page = look::take_copy();
                if page.is_some() || Device::new().colors().is_some() {
                    let (light, dark) = themes();
                    let paint = repaint(&light, &dark);
                    // Что в кэше, говорит отметка ссылки, с которой пришла
                    // страница, — если экран настроек не записал копию раньше.
                    COPIER.with_borrow_mut(|copier| {
                        copier.known.get_or_insert(page);
                    });
                    copy(paint.as_ref());
                }
                ().into_any()
            }
        }
    }
}

/// Копия палитры для первого кадра (задача 89): что в кэше и что ещё
/// записать. Запись в кэш — не мгновенна, а выбор цвета зовёт [`copy`] на
/// каждое движение по полю: пишется одна копия за раз, а ждёт — только
/// последняя.
#[derive(Default)]
struct Copier {
    /// Отметка копии, которая в кэше или уже ушла туда; `Some(None)` — копии
    /// нет, `None` — неизвестно.
    known: Option<Option<String>>,
    /// Что записать следующим: `Some(None)` — убрать копию.
    next: Option<Option<Sheet>>,
    /// Идёт ли запись.
    busy: bool,
}

thread_local! {
    static COPIER: RefCell<Copier> = RefCell::default();
    /// Выключен ли выпуск на этом устройстве (задача 41) — сигналом: его
    /// меняют экран настроек и вопрос после восстановления копии, а форма
    /// документа, даже спрятанная, следит за ним.
    static ELSEWHERE: ArcRwSignal<bool> = ArcRwSignal::new(Device::new().issue_elsewhere());
}

/// Выключен ли выпуск счетов и актов на этом устройстве (задача 41): номера
/// выдаёт другое устройство человека. Отслеживаемое.
pub(super) fn issue_elsewhere() -> bool {
    ELSEWHERE.with(|shared| shared.get())
}

/// То же — без слежения: для решения, которое принимают один раз.
pub(super) fn issue_elsewhere_untracked() -> bool {
    ELSEWHERE.with(|shared| shared.get_untracked())
}

/// Выключен ли выпуск — по записи устройства сейчас, а не по сигналу
/// вкладки: его могли выключить в другой вкладке, а сигнал берёт запись один
/// раз (ревью задачи 41). Разошлись — сигнал догоняет запись, и форма
/// вместо «Выпустить» показывает слова. Зовёт выпуск перед записью.
pub(super) fn issue_elsewhere_now() -> bool {
    let now = Device::new().issue_elsewhere();
    follow(now);
    now
}

/// Выключить или включить выпуск здесь — на устройстве и, если запись легла,
/// в окне. `false` — `localStorage` отказал: сигнал остаётся прежним,
/// иначе вкладка говорила бы «выключен», а после перезагрузки выпуск снова
/// был бы включён без единого слова (ревью задачи 41).
pub(super) fn set_issue_elsewhere(on: bool) -> bool {
    let saved = Device::new().set_issue_elsewhere(on);
    if saved {
        follow(on);
    }
    saved
}

/// Сигнал — к значению `on`, только когда оно другое: запись того же будила
/// бы всех, кто за ним следит (CLAUDE.md, Правило 2, «Общий сигнал»).
fn follow(on: bool) {
    ELSEWHERE.with(|shared| {
        if shared.get_untracked() != on {
            shared.set(on);
        }
    });
}

/// Копию палитры — вслед за записью устройства: палитра — стилем в кэш,
/// `None` — убрать копию. Та, что уже там, второй раз не пишется.
fn copy(paint: Option<&Paint>) {
    let sheet = paint.map(Sheet::of);
    let mark = sheet.as_ref().map(|sheet| sheet.mark.clone());
    let start = COPIER.with_borrow_mut(|copier| {
        if copier.known.as_ref() == Some(&mark) {
            return false;
        }
        copier.known = Some(mark);
        copier.next = Some(sheet);
        !std::mem::replace(&mut copier.busy, true)
    });
    if start {
        leptos::task::spawn_local(async {
            while let Some(next) = COPIER.with_borrow_mut(|copier| {
                let next = copier.next.take();
                copier.busy = next.is_some();
                next
            }) {
                match next {
                    Some(sheet) => look::keep_copy(&sheet).await,
                    None => look::drop_copy().await,
                }
            }
        });
    }
}

/// Обе темы программы из `tokens.css`.
fn themes() -> (Palette, Palette) {
    theme::palettes(TOKENS)
}

/// Палитра записи — заново по выбору: у новой версии программы цвета
/// `tokens.css` могли смениться, а в записи — посчитанные прежней.
/// Совпало — ничего не пишется. Вернёт палитру записи; `None` — своих цветов
/// нет или запись не читается.
fn repaint(light: &Palette, dark: &Palette) -> Option<Paint> {
    let device = Device::new();
    let record = device.colors().and_then(|text| Record::parse(&text))?;
    let paint = theme::paint(&record.choice, light, dark)?;
    if paint != record.paint {
        look::paint(Some(&paint));
        if device.set_colors(
            &Record {
                choice: record.choice,
                paint: paint.clone(),
            }
            .to_text(),
        ) {
            copy(Some(&paint));
        }
    }
    Some(paint)
}

/// Цвета под названием в строке — по паре, которая читается плохо.
const fn row_of(pair: Pair) -> Pick {
    match pair {
        Pair::Text => Pick::Text,
        Pair::ButtonText => Pick::ButtonText,
        Pair::Buttons => Pick::Buttons,
    }
}

/// Слова о плохо читаемой паре: контраст и порог — десятыми, по правилам
/// языка («2,1», «4,5»).
fn low_words(lang: Lang, low: Low) -> String {
    let key = match low.pair {
        Pair::Text => Key::ColorLowText,
        Pair::ButtonText => Key::ColorLowButtonText,
        Pair::Buttons => Key::ColorLowButtons,
    };
    fill(
        t(lang, key),
        &[&i18n::decimal(lang, low.tenths, 1), &i18n::decimal(lang, low.need_tenths, 1)],
    )
}

/// Подпись строки цвета.
const fn label_of(pick: Pick) -> Key {
    match pick {
        Pick::Background => Key::ColorBackground,
        Pick::Buttons => Key::ColorButtons,
        Pick::ButtonText => Key::ColorButtonText,
        Pick::Text => Key::ColorText,
    }
}

/// Содержимое экрана — из части: кружок у курсора и свои цвета.
#[component]
fn SettingsPage(lang: ReadSignal<Lang>, look: Look) -> impl IntoView {
    // Запись после обновления могла остаться от прежней версии: экран
    // показывает то, что стоит на странице.
    let themes = StoredValue::new(themes());
    themes.with_value(|(light, dark)| repaint(light, dark));
    // Тема устройства — сигналом: экран, показанный однажды, живёт, пока
    // открыто окно, а устройство меняет тему само. Слежка уходит вместе
    // с экраном.
    let scheme = RwSignal::new(look::device_scheme());
    StoredValue::new_local(look::watch_scheme(move |now| scheme.set(now)));
    let record = Device::new().colors().and_then(|text| Record::parse(&text));
    let choice = RwSignal::new(
        record
            .as_ref()
            .map_or_else(|| Choice::new(scheme.get_untracked()), |record| record.choice),
    );
    let painted = RwSignal::new(record.map(|record| record.paint));
    // Выбор сменился — палитра на страницу сразу, и на устройство: окно
    // перекрашивается, пока человек ведёт по выбору цвета. Копия первого
    // кадра — только вслед за записью: иначе она разошлась бы с ней.
    let apply = move |next: Choice| {
        let paint = themes.with_value(|(light, dark)| theme::paint(&next, light, dark));
        look::paint(paint.as_ref());
        let device = Device::new();
        let saved = match &paint {
            Some(paint) => device.set_colors(
                &Record {
                    choice: next,
                    paint: paint.clone(),
                }
                .to_text(),
            ),
            None => device.clear_colors(),
        };
        if saved {
            copy(paint.as_ref());
        }
        choice.set(next);
        painted.set(paint);
    };
    // Цвет в строке — тот, что сейчас у окна: своей палитры или темы
    // устройства.
    let shown = move |pick: Pick| {
        painted
            .with(|paint| paint.as_ref().and_then(|paint| paint.colors.get(pick.token())))
            .or_else(|| {
                let scheme = scheme.get();
                themes.with_value(|(light, dark)| match scheme {
                    theme::Scheme::Light => light.get(pick.token()),
                    theme::Scheme::Dark => dark.get(pick.token()),
                })
            })
    };
    // Выбор, с которого начать: свой — как есть, а пустой — от темы
    // устройства сейчас, а не той, что была при первом показе экрана.
    let current = move || {
        let now = choice.get_untracked();
        if now.is_empty() { Choice::new(scheme.get_untracked()) } else { now }
    };
    let lows = Memo::new(move |_| {
        painted.with(|paint| {
            paint
                .as_ref()
                .map(|paint| choice.with(|choice| theme::low(choice, paint)))
                .unwrap_or_default()
        })
    });

    let rows = Pick::ALL
        .into_iter()
        .map(|pick| {
            let own = move || choice.with(|choice| choice.get(pick).is_some());
            let pick_color = move |event: leptos::ev::Event| {
                if let Some(color) = Rgb::parse(&event_target_value(&event)) {
                    let mut next = current();
                    next.set(pick, Some(color));
                    apply(next);
                }
            };
            let reset = move |_| {
                let mut next = current();
                next.set(pick, None);
                apply(next);
            };
            let low = move || {
                lows.with(|lows| lows.iter().copied().find(|low| row_of(low.pair) == pick))
                    .map(|low| low_words(lang.get(), low))
            };
            view! {
                <li class="color-row">
                    <label class="color-pick">
                        <span class="entry-main">
                            <span class="entry-title">{tr(lang, label_of(pick))}</span>
                            <span class="entry-meta">
                                {move || t(lang.get(), if own() { Key::ColorOwn } else { Key::ColorApp })}
                            </span>
                        </span>
                        <input
                            type="color"
                            prop:value=move || shown(pick).map(Rgb::hex).unwrap_or_default()
                            on:input=pick_color
                        />
                    </label>
                    {move || own().then(|| view! {
                        <button type="button" class="link" on:click=reset>
                            <span class="label">{tr(lang, Key::ColorReset)}</span>
                        </button>
                    })}
                    {move || low().map(|words| view! { <p class="color-low" role="status">{words}</p> })}
                </li>
            }
        })
        .collect_view();

    let reset_all = move |_| apply(Choice::new(scheme.get_untracked()));
    view! {
        // Выпуск — первым: от него зависят номера документов, а кружок
        // и цвета — только вид окна (Правило 14).
        <h2>{tr(lang, Key::SettingsDevice)}</h2>
        <ul class="list">
            <li class="issuing">
                <label class="switch-row">
                    <span class="entry-main">
                        <span class="entry-title">{tr(lang, Key::SettingsIssue)}</span>
                        <span class="entry-meta">{tr(lang, Key::SettingsIssueHint)}</span>
                    </span>
                    <input
                        type="checkbox"
                        role="switch"
                        class="visually-hidden"
                        prop:checked=move || !issue_elsewhere()
                        on:change=move |event| {
                            let here = event_target_checked(&event);
                            // Запись не легла — переключатель назад: выпуск
                            // остался прежним.
                            if !set_issue_elsewhere(!here) {
                                event_target::<HtmlInputElement>(&event).set_checked(!here);
                            }
                        }
                    />
                    <span class="switch" aria-hidden="true"></span>
                </label>
            </li>
        </ul>
        <h2>{tr(lang, Key::SettingsMouse)}</h2>
        <ul class="list">
            <li class="mouse">
                <label class="switch-row">
                    <span class="entry-main">
                        <span class="entry-title">{tr(lang, Key::SettingsCursor)}</span>
                        <span class="entry-meta">{tr(lang, Key::SettingsCursorHint)}</span>
                    </span>
                    <input
                        type="checkbox"
                        role="switch"
                        class="visually-hidden"
                        prop:checked=move || look.cursor.get()
                        on:change=move |event| look.set_cursor(event_target_checked(&event))
                    />
                    <span class="switch" aria-hidden="true"></span>
                </label>
            </li>
        </ul>
        <h2>{tr(lang, Key::SettingsColors)}</h2>
        <p class="hint">{tr(lang, Key::SettingsColorsAbout)}</p>
        <ul class="list colors">{rows}</ul>
        // Выход — ссылкой, как «Сбросить»: цвет ссылок окно дотягивает до
        // порога на любом фоне, а у кнопки — цвет выбранного текста, и при
        // тексте цвета фона она пропала бы вместе с ним (глазной прогон
        // задачи 88).
        {move || (!choice.with(Choice::is_empty)).then(|| view! {
            <button type="button" class="link colors-reset" on:click=reset_all>
                <span class="label">{tr(lang, Key::ColorsResetAll)}</span>
            </button>
        })}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Каждая пара, о которой экран говорит, стоит под своим цветом, а слова
    /// называют контраст и порог по правилам языка.
    #[test]
    fn low_pairs_stand_under_their_colors() {
        assert_eq!(row_of(Pair::Text), Pick::Text);
        assert_eq!(row_of(Pair::ButtonText), Pick::ButtonText);
        assert_eq!(row_of(Pair::Buttons), Pick::Buttons);
        let low = Low {
            pair: Pair::Text,
            tenths: 21,
            need_tenths: 45,
        };
        assert_eq!(
            low_words(Lang::Ru, low),
            "Текст на фоне читается плохо: контраст 2,1, а нужен хотя бы 4,5."
        );
        assert!(low_words(Lang::En, low).contains("contrast 2.1, and at least 4.5"));
        let buttons = Low {
            pair: Pair::Buttons,
            tenths: 14,
            need_tenths: 30,
        };
        assert!(low_words(Lang::Ru, buttons).ends_with("контраст 1,4, а нужен хотя бы 3."));
    }

    /// Расчёт берёт палитры из того же `tokens.css`, что рисует окно, и обе
    /// темы в нём есть.
    #[test]
    fn the_palette_comes_from_the_window_tokens() {
        let (light, dark) = themes();
        for pick in Pick::ALL {
            assert!(light.get(pick.token()).is_some(), "светлая: {}", pick.token());
            assert!(dark.get(pick.token()).is_some(), "тёмная: {}", pick.token());
        }
    }
}
