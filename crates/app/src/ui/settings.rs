//! Настройки (задача 88): кружок у курсора и свои цвета окна.
//!
//! Обе настройки — на устройстве (`store/device.rs`), а не в базе: это не
//! данные человека, и в копию они не идут, как язык окна. Кружок по
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
//! ([`refresh`]). Пока wasm не запустился, страница — в цветах программы:
//! их рисует CSS до него.
//!
//! Кружок — атрибутом `data-cursor` на `<html>` (`platform::look`), а не
//! разметкой: кружок с условием в корне окна стоил главному wasm лишнего
//! кода больше, чем атрибут (замер задачи 88).

use kvartik_core::i18n::{self, Key, Lang, fill, t};
use kvartik_core::theme::{self, Choice, Low, Pair, Palette, Pick, Record, Rgb};
use leptos::prelude::*;

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
    /// настроек начнёт с них.
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

/// Пересчитать свои цвета после открытия — частью настроек ([`Job::Refresh`]).
/// Зовёт загрузка частей (`split::preload`), последней: часть уже в памяти.
/// У кого своих цветов нет, её не зовут.
pub async fn refresh() {
    if Device::new().colors().is_some() {
        let _ = settings_part(Job::Refresh).await;
    }
}

/// Что делает часть настроек.
#[derive(Clone, Copy)]
pub(super) enum Job {
    /// Нарисовать экран.
    Page(ReadSignal<Lang>, Look),
    /// Пересчитать палитру записи по выбору (`repaint`).
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
                let (light, dark) = themes();
                repaint(&light, &dark);
                ().into_any()
            }
        }
    }
}

/// Обе темы программы из `tokens.css`.
fn themes() -> (Palette, Palette) {
    theme::palettes(TOKENS)
}

/// Палитра записи — заново по выбору: у новой версии программы цвета
/// `tokens.css` могли смениться, а в записи — посчитанные прежней.
/// Совпало — ничего не пишется.
fn repaint(light: &Palette, dark: &Palette) {
    let device = Device::new();
    let Some(record) = device.colors().and_then(|text| Record::parse(&text)) else {
        return;
    };
    let Some(paint) = theme::paint(&record.choice, light, dark) else {
        return;
    };
    if paint != record.paint {
        look::paint(Some(&paint));
        device.set_colors(
            &Record {
                choice: record.choice,
                paint,
            }
            .to_text(),
        );
    }
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
    // перекрашивается, пока человек ведёт по выбору цвета.
    let apply = move |next: Choice| {
        let paint = themes.with_value(|(light, dark)| theme::paint(&next, light, dark));
        look::paint(paint.as_ref());
        let device = Device::new();
        match &paint {
            Some(paint) => {
                device.set_colors(
                    &Record {
                        choice: next,
                        paint: paint.clone(),
                    }
                    .to_text(),
                );
            }
            None => {
                device.clear_colors();
            }
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
        <h2>{tr(lang, Key::SettingsMouse)}</h2>
        <ul class="list">
            <li>
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
