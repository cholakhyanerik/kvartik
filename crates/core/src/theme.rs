//! Свои цвета окна (задача 88): что человек выбрал в настройках — фон,
//! кнопки, текст на кнопках и текст, — и палитра, которую окно из этого
//! строит. Строки на входе и на выходе: про браузер и хранилище модуль не
//! знает (таблица слоёв CLAUDE.md).
//!
//! Палитра программы живёт в `crates/app/style/tokens.css` (Правило 4):
//! две темы, светлая и тёмная по настройке устройства. Свои цвета действуют
//! всегда, и днём, и ночью (решение человека 2026-11-09): окно пишет на
//! `<html>` всю палитру переменными, и тема устройства её больше не меняет.
//! Чего человек не выбирал, берётся у той темы программы, к которой ближе
//! фон ([`Scheme::of`]): у светлого — у светлой, у тёмного — у тёмной.
//! Производное от выбранного — карточки и поля от фона, приглушённый текст
//! и рамки от текста, ссылки, кольцо фокуса и заливки под мышью от кнопок —
//! окно считает само и дотягивает до порога WCAG 2.1 ([`readable`]): ссылки,
//! рамки и слова статусов читаются на любом выбранном фоне. Сами выбранные
//! пары — текст на фоне, текст на кнопке, кнопка на фоне — выбор человека:
//! плохо читаемую окно называет числом ([`low`]), но цвет применяет (решение
//! человека 2026-11-09).
//!
//! Бумага превью и PDF (`--…-paper…`) своими цветами не красится никогда:
//! в превью — то же, что в PDF (Правило 2).
//!
//! `f64` здесь законен: это не деньги, а яркость (как в
//! `crates/app/tests/tokens.rs`).

/// Порог текста — 4,5:1, порог рамки, кольца фокуса и заливки кнопки на
/// фоне — 3:1 (WCAG 2.1, 1.4.3 и 1.4.11; шапка `tokens.css`).
pub const TEXT: f64 = 4.5;
pub const LARGE: f64 = 3.0;

/// Фоны палитры: с ними сравнивают текст (шапка `tokens.css`).
const SURFACES: [&str; 3] = ["--surface", "--surface-raised", "--surface-sunken"];
/// Слова о сделанном, о внимании и об ошибке: свой тон, на любом фоне
/// дотянутый до порога.
const STATUSES: [&str; 3] = ["--success-text", "--warning-text", "--danger-text"];
/// Тон кнопок на фоне и текст на нём: открытая вкладка меню на компьютере
/// и второстепенная кнопка под мышью.
const TINTS: [(&str, &str); 2] = [
    ("--fill-selected", "--on-fill-selected"),
    ("--fill-hover-soft", "--on-fill-hover-soft"),
];

/// Цвет `#rrggbb` — без прозрачности, как в `tokens.css`: контраст
/// полупрозрачного зависит от того, что под ним.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Rgb(pub [u8; 3]);

impl Rgb {
    pub const BLACK: Rgb = Rgb([0, 0, 0]);
    pub const WHITE: Rgb = Rgb([255, 255, 255]);

    /// Цвет из числа `0xRRGGBB` — для проб и тестов: цвет литералом `#…`
    /// или функцией `rgb(…)` в коде окна ловит тест оформления
    /// (`crates/app/tests/tokens.rs`, Правило 4).
    pub const fn from_u32(value: u32) -> Self {
        let [_, r, g, b] = value.to_be_bytes();
        Self([r, g, b])
    }

    /// `#rrggbb` в любом регистре — так его пишут `tokens.css` и поле
    /// `<input type="color">`; другое — `None`.
    pub fn parse(text: &str) -> Option<Self> {
        let digits = text.strip_prefix('#')?;
        if digits.len() != 6 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let channel = |at: usize| u8::from_str_radix(&digits[at..at + 2], 16).ok();
        Some(Self([channel(0)?, channel(2)?, channel(4)?]))
    }

    /// `#rrggbb` строчными. Своими руками, а не `format!("{:02x}")`: запись
    /// шестнадцатеричных чисел тянула бы в wasm код, которому он больше не
    /// нужен.
    pub fn hex(self) -> String {
        const DIGITS: &[u8; 16] = b"0123456789abcdef";
        let mut out = String::with_capacity(7);
        out.push('#');
        for channel in self.0 {
            out.push(char::from(DIGITS[usize::from(channel >> 4)]));
            out.push(char::from(DIGITS[usize::from(channel & 0xf)]));
        }
        out
    }

    /// Относительная яркость по WCAG 2.1: от 0 (чёрный) до 1 (белый).
    pub fn luminance(self) -> f64 {
        let linear = |channel: u8| {
            let c = f64::from(channel) / 255.0;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        let [r, g, b] = self.0;
        0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
    }

    /// Цвет на `share` тысячных пути к `toward`, каждый канал — с округлением.
    pub fn mix(self, toward: Rgb, share: u16) -> Rgb {
        let share = u32::from(share.min(1000));
        let mut out = [0; 3];
        for ((out, from), to) in out.iter_mut().zip(self.0).zip(toward.0) {
            let value = (u32::from(from) * (1000 - share) + u32::from(to) * share + 500) / 1000;
            *out = u8::try_from(value).unwrap_or(u8::MAX);
        }
        Rgb(out)
    }
}

/// Контраст двух цветов по WCAG 2.1: от 1 до 21.
pub fn contrast(a: Rgb, b: Rgb) -> f64 {
    let (la, lb) = (a.luminance(), b.luminance());
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// Край, который сильнее отличается от `back`: чёрный на светлом, белый на
/// тёмном. С ним у любого цвета контраст не меньше 4,58.
fn extreme(back: Rgb) -> Rgb {
    if contrast(back, Rgb::BLACK) >= contrast(back, Rgb::WHITE) {
        Rgb::BLACK
    } else {
        Rgb::WHITE
    }
}

/// `color` — или, если он не держит порог `min` хоть на одном из `backs`,
/// тот же цвет, шаг за шагом сдвинутый к краю `edge`: тон остаётся, пока
/// хватает порога, а на самом краю — лучшее, что есть.
pub fn readable(color: Rgb, backs: &[Rgb], min: f64, edge: Rgb) -> Rgb {
    let reads = |c: Rgb| backs.iter().all(|&back| contrast(c, back) >= min);
    (0..=20)
        .map(|step| color.mix(edge, step * 50))
        .find(|&c| reads(c))
        .unwrap_or(edge)
}

/// Тема программы: светлая или тёмная. Её же имя — значение `color-scheme`:
/// полосы прокрутки и поля дат браузер рисует в тон.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scheme {
    Light,
    Dark,
}

impl Scheme {
    /// Имя в записи устройства и в `color-scheme`.
    pub const fn code(self) -> &'static str {
        match self {
            Scheme::Light => "light",
            Scheme::Dark => "dark",
        }
    }

    fn from_code(code: &str) -> Option<Self> {
        match code {
            "light" => Some(Scheme::Light),
            "dark" => Some(Scheme::Dark),
            _ => None,
        }
    }

    /// Тема, к которой ближе фон: на светлом чёрный текст контрастнее
    /// белого.
    pub fn of(background: Rgb) -> Self {
        if extreme(background) == Rgb::BLACK {
            Scheme::Light
        } else {
            Scheme::Dark
        }
    }

    /// Край текста этой темы: тёмный текст светлой темы дотягивается
    /// к чёрному, светлый текст тёмной — к белому.
    const fn ink(self) -> Rgb {
        match self {
            Scheme::Light => Rgb::BLACK,
            Scheme::Dark => Rgb::WHITE,
        }
    }
}

/// Что человек выбирает сам.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pick {
    /// Фон страницы — `--surface`.
    Background,
    /// Заливка главных кнопок — `--accent`.
    Buttons,
    /// Текст на главных кнопках — `--on-accent`.
    ButtonText,
    /// Текст страницы — `--text`.
    Text,
}

impl Pick {
    /// В порядке экрана настроек.
    pub const ALL: [Pick; 4] = [Pick::Background, Pick::Buttons, Pick::ButtonText, Pick::Text];

    /// Имя в записи устройства — постоянное: сменишь — у всех пропадёт
    /// выбранный цвет.
    pub const fn code(self) -> &'static str {
        match self {
            Pick::Background => "background",
            Pick::Buttons => "buttons",
            Pick::ButtonText => "button-text",
            Pick::Text => "text",
        }
    }

    fn from_code(code: &str) -> Option<Self> {
        Pick::ALL.into_iter().find(|pick| pick.code() == code)
    }

    /// Переменная `tokens.css`, которую цвет задаёт.
    pub const fn token(self) -> &'static str {
        match self {
            Pick::Background => "--surface",
            Pick::Buttons => "--accent",
            Pick::ButtonText => "--on-accent",
            Pick::Text => "--text",
        }
    }

    const fn index(self) -> usize {
        match self {
            Pick::Background => 0,
            Pick::Buttons => 1,
            Pick::ButtonText => 2,
            Pick::Text => 3,
        }
    }
}

/// Цвета одной темы: переменная `tokens.css` и её цвет — в порядке файла.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Palette(Vec<(String, Rgb)>);

impl Palette {
    pub fn get(&self, name: &str) -> Option<Rgb> {
        self.0.iter().find(|(own, _)| own == name).map(|&(_, color)| color)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, Rgb)> {
        self.0.iter().map(|(name, color)| (name.as_str(), *color))
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Поставить цвет переменной: была — заменить на месте, не было —
    /// дописать.
    pub fn set(&mut self, name: &str, color: Rgb) {
        match self.0.iter_mut().find(|(own, _)| own == name) {
            Some((_, own)) => *own = color,
            None => self.0.push((name.to_owned(), color)),
        }
    }
}

/// Палитры светлой и тёмной темы из текста `tokens.css`: светлая — `:root`
/// верхнего уровня, тёмная — `:root` внутри `@media (prefers-color-scheme:
/// dark)`. Разбор построчный, по образцу файла: его держит тест
/// `crates/app/tests/tokens.rs`, и он же сверяет, что этот разбор видит те
/// же цвета. Значения не `#rrggbb` — шрифты, отступы, движение — не цвета
/// и пропускаются.
pub fn palettes(css: &str) -> (Palette, Palette) {
    let (mut light, mut dark) = (Palette::default(), Palette::default());
    let mut in_comment = false;
    let mut depth = 0_usize;
    let mut dark_depth = None;
    for raw in css.lines() {
        let line = raw.trim();
        if in_comment {
            in_comment = !line.contains("*/");
            continue;
        }
        if line.starts_with("/*") {
            in_comment = !line.contains("*/");
            continue;
        }
        if line.starts_with("@media") && line.contains("prefers-color-scheme: dark") {
            dark_depth = Some(depth);
        }
        if let Some((name, rest)) = line.split_once(':')
            && name.starts_with("--")
            && let Some((value, _)) = rest.split_once(';')
            && let Some(color) = Rgb::parse(value.trim())
        {
            let theme = if dark_depth.is_some() { &mut dark } else { &mut light };
            theme.set(name, color);
        }
        depth += line.matches('{').count();
        depth = depth.saturating_sub(line.matches('}').count());
        if dark_depth == Some(depth) {
            dark_depth = None;
        }
    }
    (light, dark)
}

/// Что человек выбрал: свои цвета и тема, у которой окно берёт остальное,
/// пока фон не выбран.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Choice {
    picks: [Option<Rgb>; 4],
    /// Тема устройства, когда человек выбрал первый цвет, а после — тема
    /// последнего выбранного фона: вернув фон программы, он остаётся в той
    /// же теме, а не в чужой с выбранным под неё текстом.
    scheme: Scheme,
}

impl Choice {
    /// Ничего не выбрано; `scheme` — тема устройства сейчас.
    pub const fn new(scheme: Scheme) -> Self {
        Self {
            picks: [None; 4],
            scheme,
        }
    }

    pub fn get(&self, pick: Pick) -> Option<Rgb> {
        self.picks[pick.index()]
    }

    /// Выбрать цвет или вернуть цвет программы (`None`).
    pub fn set(&mut self, pick: Pick, color: Option<Rgb>) {
        self.picks[pick.index()] = color;
        if let (Pick::Background, Some(background)) = (pick, color) {
            self.scheme = Scheme::of(background);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.picks.iter().all(Option::is_none)
    }

    /// Тема, у которой окно берёт невыбранное: по фону, а без фона — та,
    /// от которой начали.
    pub fn base(&self) -> Scheme {
        self.get(Pick::Background).map_or(self.scheme, Scheme::of)
    }
}

/// Что окно пишет на `<html>`: переменные `tokens.css` с цветами
/// и `color-scheme`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Paint {
    pub scheme: Scheme,
    pub colors: Palette,
}

/// Карточка и поле — светлее фона, утопленная дорожка — темнее, на столько
/// же, сколько у тем программы: у светлой карточка — около 45 % пути
/// к белому, дорожка — 6 % к чёрному, у тёмной — 6 % к белому и 32 %
/// к чёрному. Но не дальше, чем держит текст своей темы: у светлой дорожка
/// не темнее яркости 0,18, у тёмной карточка не светлее её — тогда чёрный
/// или белый текст на всех трёх фонах держит 4,5:1. У почти белого фона
/// светлее некуда, и карточка с дорожкой — на шаг и на два темнее него.
fn surfaces(background: Rgb, scheme: Scheme) -> (Rgb, Rgb) {
    const STEPS: [u16; 5] = [60, 45, 30, 15, 0];
    match scheme {
        Scheme::Light => {
            let raised = background.mix(Rgb::WHITE, 450);
            let near_white = contrast(raised, background) < 1.04;
            let raised = if near_white { background.mix(Rgb::BLACK, 35) } else { raised };
            let deepest = if near_white { 90 } else { 60 };
            let sunken = [deepest]
                .into_iter()
                .chain(STEPS.into_iter().filter(|&step| step < deepest))
                .map(|step| background.mix(Rgb::BLACK, step))
                .find(|c| c.luminance() >= 0.18)
                .unwrap_or(background);
            (raised, sunken)
        }
        Scheme::Dark => {
            let raised = STEPS
                .into_iter()
                .map(|step| background.mix(Rgb::WHITE, step))
                .find(|c| c.luminance() <= 0.18)
                .unwrap_or(background);
            (raised, background.mix(Rgb::BLACK, 320))
        }
    }
}

/// Палитра из выбранного: вся тема [`Choice::base`] без бумаги, выбранные
/// цвета и производные от них (шапка модуля). Ничего не выбрано — `None`:
/// окно держит палитру программы и тему устройства.
///
/// Невыбранный текст дотягивается до порога на выбранном фоне сам, как
/// и производные; выбранный — как есть: это выбор человека ([`low`]).
pub fn paint(choice: &Choice, light: &Palette, dark: &Palette) -> Option<Paint> {
    if choice.is_empty() {
        return None;
    }
    let scheme = choice.base();
    let theme = match scheme {
        Scheme::Light => light,
        Scheme::Dark => dark,
    };
    let ink = scheme.ink();
    let mut out = Palette(
        theme
            .0
            .iter()
            .filter(|(name, _)| !name.contains("paper"))
            .cloned()
            .collect(),
    );
    let at = |out: &Palette, name: &str| out.get(name).unwrap_or(ink);
    let background = choice.get(Pick::Background);
    let text_picked = choice.get(Pick::Text);
    let buttons_picked = choice.get(Pick::Buttons);
    let on_buttons_picked = choice.get(Pick::ButtonText);

    if let Some(background) = background {
        let (raised, sunken) = surfaces(background, scheme);
        out.set("--surface", background);
        out.set("--surface-raised", raised);
        out.set("--surface-sunken", sunken);
    }
    let backs = SURFACES.map(|name| at(&out, name));
    let text = text_picked.unwrap_or_else(|| readable(at(&out, "--text"), &backs, TEXT, ink));
    out.set("--text", text);
    let buttons = buttons_picked.unwrap_or_else(|| at(&out, "--accent"));
    out.set("--accent", buttons);
    let on_buttons = on_buttons_picked
        .unwrap_or_else(|| readable(at(&out, "--on-accent"), &[buttons], TEXT, extreme(buttons)));
    out.set("--on-accent", on_buttons);

    // Приглушённый текст и рамки — между фоном и текстом, слова статусов —
    // своим тоном, дотянутым до порога на новом фоне. Плашки без своего
    // смысла — метка копии, плашка открытого ИП, «черновик» — на шаг от фона
    // к тексту, как у тем программы: у светлой — 4 %, у тёмной — 13 %.
    // Плашки со смыслом — «оплачен», «просрочен», напоминание — своего тона
    // темы: их текст читается на них самих.
    if background.is_some() || text_picked.is_some() {
        let page = backs[0];
        out.set("--text-muted", readable(page.mix(text, 700), &backs, TEXT, ink));
        out.set("--border", page.mix(text, 120));
        out.set("--border-strong", readable(page.mix(text, 500), &backs, LARGE, ink));
        for name in STATUSES {
            let own = at(&out, name);
            out.set(name, readable(own, &backs, TEXT, ink));
        }
        let muted = page.mix(
            text,
            match scheme {
                Scheme::Light => 40,
                Scheme::Dark => 130,
            },
        );
        out.set("--fill-muted", muted);
        out.set("--on-fill-muted", readable(text, &[muted], TEXT, extreme(muted)));
    }
    // Ссылки, открытая вкладка, кольцо фокуса и тон кнопок на фоне — цветом
    // кнопок, а без него — своим цветом темы, дотянутым до порога.
    if background.is_some() || buttons_picked.is_some() {
        let start = |out: &Palette, name: &str| buttons_picked.unwrap_or_else(|| at(out, name));
        let accent_text = readable(start(&out, "--accent-text"), &backs, TEXT, ink);
        let focus = readable(start(&out, "--focus"), &backs, LARGE, ink);
        out.set("--accent-text", accent_text);
        out.set("--focus", focus);
        let tint = backs[0].mix(buttons, 180);
        let on_tint = readable(text, &[tint], TEXT, extreme(tint));
        for (fill, on) in TINTS {
            out.set(fill, tint);
            out.set(on, on_tint);
        }
    }
    // Главная кнопка под мышью — на шаг прочь от своего текста: темнее под
    // светлым текстом, светлее под тёмным, и текст читается на ней не хуже,
    // чем на самой кнопке.
    if buttons_picked.is_some() || on_buttons_picked.is_some() {
        let hover = buttons.mix(extreme(on_buttons), 200);
        let on_hover = match on_buttons_picked {
            Some(own) => own,
            None => readable(on_buttons, &[hover], TEXT, extreme(hover)),
        };
        out.set("--fill-hover", hover);
        out.set("--on-fill-hover", on_hover);
    }
    Some(Paint { scheme, colors: out })
}

/// Выбранная пара, которая читается плохо.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pair {
    /// Текст на фоне: 4,5:1 на каждом фоне.
    Text,
    /// Текст на главной кнопке: 4,5:1.
    ButtonText,
    /// Главная кнопка на фоне: 3:1 на каждом фоне (WCAG 1.4.11).
    Buttons,
}

/// Пара, которая не держит порог, — её контраст и порог десятыми: `21` —
/// 2,1. Контраст округлён вниз, чтобы 4,49 не выглядело как 4,5.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Low {
    pub pair: Pair,
    pub tenths: i64,
    pub need_tenths: i64,
}

/// Пары с выбранным цветом, которые не держат порог. Пары без выбранного
/// цвета — палитра программы или дотянутое окном: их держат тесты
/// (`crates/app/tests/tokens.rs`). Кнопка на фоне — и тогда, когда выбран
/// только фон: кнопку программы на нём может быть не видно.
pub fn low(choice: &Choice, paint: &Paint) -> Vec<Low> {
    let get = |name: &str| paint.colors.get(name);
    let backs: Vec<Rgb> = SURFACES.iter().filter_map(|name| get(name)).collect();
    let worst = |color: Rgb| {
        backs
            .iter()
            .map(|&back| contrast(color, back))
            .fold(f64::INFINITY, f64::min)
    };
    let mut out = Vec::new();
    let mut check = |pair: Pair, ratio: f64, need: f64| {
        if ratio < need {
            out.push(Low {
                pair,
                tenths: (ratio * 10.0).floor() as i64,
                need_tenths: (need * 10.0).round() as i64,
            });
        }
    };
    if choice.get(Pick::Text).is_some()
        && let Some(text) = get("--text")
    {
        check(Pair::Text, worst(text), TEXT);
    }
    if choice.get(Pick::ButtonText).is_some()
        && let (Some(on), Some(buttons)) = (get("--on-accent"), get("--accent"))
    {
        check(Pair::ButtonText, contrast(on, buttons), TEXT);
    }
    if (choice.get(Pick::Buttons).is_some() || choice.get(Pick::Background).is_some())
        && let Some(buttons) = get("--accent")
    {
        check(Pair::Buttons, worst(buttons), LARGE);
    }
    out
}

/// Первая строка записи: номер растёт, только если прежняя версия прочла
/// бы запись неверно. Новый вид строки номера не меняет — прежняя его
/// пропустит.
const HEADER: &str = "kvartik-colors 1";

/// Запись устройства: выбор и посчитанная по нему палитра. Палитра — в самой
/// записи, чтобы окно при открытии красило сразу, до первой отрисовки, без
/// расчёта: расчёт — в части wasm настроек, а главному wasm хватает прочесть
/// и поставить (Правило 1).
///
/// Строками: заголовок, `scheme <тема>`, `pick <что> <цвет>`, `paint <тема>`
/// и строки палитры `<переменная> <цвет>`. Незнакомую строку чтение
/// пропускает: запись новой версии читает и прежняя.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    pub choice: Choice,
    pub paint: Paint,
}

impl Record {
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        let mut line = |words: &[&str]| {
            out.push_str(&words.join(" "));
            out.push('\n');
        };
        line(&[HEADER]);
        line(&["scheme", self.choice.scheme.code()]);
        for pick in Pick::ALL {
            if let Some(color) = self.choice.get(pick) {
                line(&["pick", pick.code(), &color.hex()]);
            }
        }
        line(&["paint", self.paint.scheme.code()]);
        for (name, color) in self.paint.colors.iter() {
            line(&[name, &color.hex()]);
        }
        out
    }

    /// Запись, как её прочтёт окно; `None` — не наша, другого номера или без
    /// выбранного цвета: окно остаётся в цветах программы. Строка палитры
    /// с чужим именем или не цветом пропускается — на страницу ставятся
    /// только переменные палитры, и бумаги среди них нет.
    pub fn parse(text: &str) -> Option<Self> {
        let mut lines = text.lines();
        if lines.next()? != HEADER {
            return None;
        }
        let (mut scheme, mut painted) = (None, None);
        let mut picks = [None; 4];
        let mut colors = Palette::default();
        for line in lines {
            let mut words = line.split(' ');
            match (words.next(), words.next(), words.next(), words.next()) {
                (Some("scheme"), Some(code), None, None) => scheme = Scheme::from_code(code),
                (Some("paint"), Some(code), None, None) => painted = Scheme::from_code(code),
                (Some("pick"), Some(code), Some(color), None) => {
                    if let (Some(pick), Some(color)) = (Pick::from_code(code), Rgb::parse(color)) {
                        picks[pick.index()] = Some(color);
                    }
                }
                (Some(name), Some(color), None, None) if is_token(name) => {
                    if let Some(color) = Rgb::parse(color) {
                        colors.set(name, color);
                    }
                }
                _ => {}
            }
        }
        let choice = Choice { picks, scheme: scheme? };
        if choice.is_empty() {
            return None;
        }
        Some(Self {
            choice,
            paint: Paint {
                scheme: painted?,
                colors,
            },
        })
    }
}

/// Палитра записи — как её ставит окно при открытии: тема и пары
/// «переменная — цвет» прямо из текста записи, без разбора выбора. Главному
/// wasm хватает поставить их (Правило 1), а выбор читает часть настроек
/// ([`Record::parse`]). Записи, которую не прочтёт [`Record::parse`], здесь
/// тоже нет, а строки палитры с чужим именем или не цветом пропускаются так
/// же.
pub fn painted(text: &str) -> Option<(Scheme, Vec<(&str, &str)>)> {
    let mut lines = text.lines();
    if lines.next()? != HEADER {
        return None;
    }
    let (mut chose, mut scheme, mut painted) = (false, false, None);
    let mut colors = Vec::new();
    for line in lines {
        let Some((first, rest)) = line.split_once(' ') else {
            continue;
        };
        match first {
            "scheme" => scheme = Scheme::from_code(rest).is_some(),
            "paint" => painted = Scheme::from_code(rest),
            "pick" => {
                chose |= rest.split_once(' ').is_some_and(|(code, color)| {
                    Pick::from_code(code).is_some() && Rgb::parse(color).is_some()
                });
            }
            name if is_token(name) && Rgb::parse(rest).is_some() => colors.push((name, rest)),
            _ => {}
        }
    }
    (chose && scheme).then_some(())?;
    Some((painted?, colors))
}

/// Палитра стилем — для первого кадра, до wasm (задача 89). Окно ставит
/// палитру на `<html>` при запуске wasm, а страницу браузер рисует раньше:
/// у кого свой фон тёмный, а устройство в светлой теме, каждое открытие
/// начиналось с кремового кадра. Поэтому окно держит копию палитры этим
/// стилем, а service worker вписывает в страницу ссылку на неё
/// (`crates/app/public/sw.js`).
///
/// `html:root` сильнее `:root` обеих тем `tokens.css` — и тёмной тоже,
/// в каком бы порядке ни легли стили. `color-scheme` — тот же, что окно
/// ставит на `<html>`: полосы прокрутки и поля первого кадра — в тон
/// палитре.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sheet {
    /// Сам стиль: одно правило, переменные палитры и `color-scheme`.
    pub css: String,
    /// Тема палитры — service worker ставит её и мета-тегу страницы.
    pub scheme: Scheme,
    /// Отметка стиля: по ней окно узнаёт, та ли копия у страницы, не читая
    /// её. FNV-1a текста — 16 шестнадцатеричных знаков: это не защита,
    /// а сверка своей же записи.
    pub mark: String,
}

impl Sheet {
    /// Стиль палитры. Переменные — только палитры, как у [`painted`]: имя
    /// со скобкой или точкой с запятой сломало бы правило.
    pub fn of(paint: &Paint) -> Self {
        let mut css = format!("html:root{{color-scheme:{};", paint.scheme.code());
        for (name, color) in paint.colors.iter().filter(|&(name, _)| is_token(name)) {
            css.push_str(name);
            css.push(':');
            css.push_str(&color.hex());
            css.push(';');
        }
        css.push_str("}\n");
        let mark = css
            .bytes()
            .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
                (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
            });
        Self {
            css,
            scheme: paint.scheme,
            mark: format!("{mark:016x}"),
        }
    }
}

/// Имя переменной палитры: `--`, строчные латинские буквы, цифры и дефисы —
/// и не бумага.
fn is_token(name: &str) -> bool {
    name.strip_prefix("--").is_some_and(|rest| {
        !rest.is_empty()
            && rest
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    }) && !name.contains("paper")
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn rgb(value: u32) -> Rgb {
        Rgb::from_u32(value)
    }

    /// Две темы по образцу `tokens.css`: цвета — его же, только нужные
    /// расчёту. Настоящий файл проверяет `crates/app/tests/tokens.rs`.
    fn themes() -> (Palette, Palette) {
        palettes(
            "/* Шапка
               в несколько строк { с фигурной скобкой } */
            :root {
              color-scheme: light dark;
              --surface: #f5ead8;
              --surface-raised: #f9f4ed;
              --surface-sunken: #ebddc5;
              --text: #201e1d; /* 13.9 на surface */
              --text-muted: #645c50;
              --border: #dcd3c4;
              --border-strong: #82796a;
              --focus: #b2622d;
              --accent: #a85b27;
              --on-accent: #fffaf3;
              --accent-text: #8c491a;
              --success-text: #56633f;
              --warning-text: #7a3f15;
              --danger-text: #a3301f;
              --fill-selected: #ffe1d0;
              --on-fill-selected: #402310;
              --fill-muted: #eee7db;
              --on-fill-muted: #474238;
              --fill-paper: #ffffff;
              --on-fill-paper: #201e1d;
              --fill-hover: #8c491a;
              --on-fill-hover: #fffaf3;
              --fill-hover-soft: #ffe1d0;
              --on-fill-hover-soft: #402310;
              --font-sans: \"Kvartik Sans\", system-ui, sans-serif;
              --space-1: 0.25rem;
            }

            @media (prefers-color-scheme: dark) {
              :root {
                --surface: #1d1b18;
                --surface-raised: #2a2722;
                --surface-sunken: #141210;
                --text: #f5ead8;
                --text-muted: #c0b6a5;
                --border: #3d3830;
                --border-strong: #8a8172;
                --focus: #f6a06b;
                --accent: #e9915a;
                --on-accent: #1d1b18;
                --accent-text: #f6a06b;
                --success-text: #aebf92;
                --warning-text: #ffc6a5;
                --danger-text: #ff9b87;
                --fill-selected: #4d2b15;
                --on-fill-selected: #ffe1d0;
                --fill-muted: #3a3630;
                --on-fill-muted: #eee7db;
                --fill-paper: #ffffff;
                --on-fill-paper: #201e1d;
                --fill-hover: #f6a06b;
                --on-fill-hover: #1d1b18;
                --fill-hover-soft: #4d2b15;
                --on-fill-hover-soft: #ffe1d0;
              }
            }
            ",
        )
    }

    /// Цвет читается и пишется так, как его пишут `tokens.css` и поле
    /// выбора цвета, а всё прочее — не цвет.
    #[test]
    fn colors_are_read_and_written_as_hex() {
        assert_eq!(Rgb::parse("#F5eAd8"), Some(Rgb([245, 234, 216])));
        assert_eq!(Rgb([245, 234, 216]).hex(), "#f5ead8");
        assert_eq!(Rgb([0, 10, 255]).hex(), "#000aff");
        let mut wrong = Vec::new();
        for text in ["", "#", "f5ead8", "#f5ead", "#f5ead8a", "#gg0000", "#+f0000", "#f5ead8 ", "#ф5ead8"] {
            if Rgb::parse(text).is_some() {
                wrong.push(text);
            }
        }
        assert!(wrong.is_empty(), "прочитаны как цвет: {wrong:?}");
    }

    /// Контраст — по WCAG 2.1: чёрное на белом — 21, цвет сам с собой — 1,
    /// текст на фоне светлой темы — 13,9, как пишет комментарий `tokens.css`.
    #[test]
    fn contrast_follows_wcag() {
        assert!((contrast(Rgb::BLACK, Rgb::WHITE) - 21.0).abs() < 1e-9);
        assert!((contrast(rgb(0x8c491a), rgb(0x8c491a)) - 1.0).abs() < 1e-9);
        let ratio = contrast(rgb(0x201e1d), rgb(0xf5ead8));
        assert_eq!((ratio * 10.0).floor(), 139.0, "{ratio}");
        assert_eq!(Rgb::BLACK.mix(Rgb::WHITE, 500), Rgb([128, 128, 128]));
        assert_eq!(rgb(0x102030).mix(Rgb::WHITE, 0), rgb(0x102030));
        assert_eq!(rgb(0x102030).mix(Rgb::WHITE, 1000), Rgb::WHITE);
    }

    /// Разбор `tokens.css`: светлая тема — верхний `:root`, тёмная — внутри
    /// `@media`, многострочный комментарий со скобками разбору не мешает,
    /// а шрифты и отступы — не цвета.
    #[test]
    fn palettes_come_from_tokens_css() {
        let (light, dark) = themes();
        assert_eq!(light.get("--surface"), Some(rgb(0xf5ead8)));
        assert_eq!(dark.get("--surface"), Some(rgb(0x1d1b18)));
        assert_eq!(light.get("--text"), Some(rgb(0x201e1d)));
        assert_eq!(dark.get("--text"), Some(rgb(0xf5ead8)));
        assert_eq!(light.get("--font-sans"), None);
        assert_eq!(light.get("--space-1"), None);
        assert_eq!(light.iter().count(), 24);
        assert_eq!(dark.iter().count(), 24);
    }

    /// Ничего не выбрано — палитры нет: окно держит цвета программы и тему
    /// устройства.
    #[test]
    fn nothing_chosen_paints_nothing() {
        let (light, dark) = themes();
        assert_eq!(paint(&Choice::new(Scheme::Dark), &light, &dark), None);
    }

    /// Тёмный фон берёт остальное у тёмной темы, светлый — у светлой; без
    /// фона — тема, от которой начали. Выбранные цвета — как есть, а бумаги
    /// в палитре нет.
    #[test]
    fn the_background_chooses_the_theme() {
        let (light, dark) = themes();
        let mut choice = Choice::new(Scheme::Light);
        choice.set(Pick::Background, Some(rgb(0x0b1d33)));
        let painted = paint(&choice, &light, &dark).expect("выбран фон");
        assert_eq!(painted.scheme, Scheme::Dark);
        assert_eq!(painted.colors.get("--surface"), Some(rgb(0x0b1d33)));
        assert_eq!(painted.colors.get("--text"), dark.get("--text"));
        assert!(painted.colors.iter().all(|(name, _)| !name.contains("paper")));

        choice.set(Pick::Background, Some(rgb(0xe8f0ff)));
        assert_eq!(paint(&choice, &light, &dark).map(|p| p.scheme), Some(Scheme::Light));
        // У белого фона светлее некуда: карточка на шаг темнее, дорожка — на два.
        choice.set(Pick::Background, Some(Rgb::WHITE));
        let white = paint(&choice, &light, &dark).expect("выбран фон");
        let (raised, sunken) = (
            white.colors.get("--surface-raised").expect("карточка"),
            white.colors.get("--surface-sunken").expect("дорожка"),
        );
        assert!(raised.luminance() < 1.0 && sunken.luminance() < raised.luminance(), "{raised:?} {sunken:?}");
        // Фон программы вернули — тема последнего фона, а не начальная.
        choice.set(Pick::Background, Some(rgb(0x0b1d33)));
        choice.set(Pick::Background, None);
        choice.set(Pick::Buttons, Some(rgb(0x2266cc)));
        let painted = paint(&choice, &light, &dark).expect("выбраны кнопки");
        assert_eq!(painted.scheme, Scheme::Dark);
        assert_eq!(painted.colors.get("--surface"), dark.get("--surface"));
        assert_eq!(painted.colors.get("--accent"), Some(rgb(0x2266cc)));

        let mut buttons = Choice::new(Scheme::Light);
        buttons.set(Pick::Buttons, Some(rgb(0x2266cc)));
        let painted = paint(&buttons, &light, &dark).expect("выбраны кнопки");
        assert_eq!(painted.scheme, Scheme::Light);
        // Чего не трогали, то — как у темы.
        for name in ["--surface", "--text", "--text-muted", "--border-strong", "--success-text"] {
            assert_eq!(painted.colors.get(name), light.get(name), "{name}");
        }
    }

    /// Всё, что окно дотягивает само, держит порог на любом фоне: перебор
    /// фонов по сетке цвета, с кнопками и без. Провалы — списком в конце.
    #[test]
    fn derived_colors_read_on_any_background() {
        let (light, dark) = themes();
        let levels = [0x00, 0x33, 0x66, 0x80, 0x99, 0xcc, 0xff];
        let mut wrong = Vec::new();
        for &r in &levels {
            for &g in &levels {
                for &b in &levels {
                    for buttons in [None, Some(rgb(0xffd400)), Some(rgb(0x1a3a8a))] {
                        let mut choice = Choice::new(Scheme::Light);
                        choice.set(Pick::Background, Some(Rgb([r, g, b])));
                        choice.set(Pick::Buttons, buttons);
                        let painted = paint(&choice, &light, &dark).expect("выбран фон");
                        let get = |name: &str| painted.colors.get(name).expect(name);
                        let backs = SURFACES.map(get);
                        let mut need = |name: &str, on: &[Rgb], min: f64| {
                            let color = get(name);
                            for &back in on {
                                let ratio = contrast(color, back);
                                if ratio < min {
                                    wrong.push(format!(
                                        "фон {} кнопки {buttons:?}: {name} {ratio:.2} на {}",
                                        Rgb([r, g, b]).hex(),
                                        back.hex()
                                    ));
                                }
                            }
                        };
                        for name in ["--text", "--text-muted", "--accent-text"].into_iter().chain(STATUSES) {
                            need(name, &backs, TEXT);
                        }
                        need("--border-strong", &backs, LARGE);
                        need("--focus", &backs, LARGE);
                        need("--on-accent", &[get("--accent")], TEXT);
                        need("--on-fill-hover", &[get("--fill-hover")], TEXT);
                        need("--on-fill-muted", &[get("--fill-muted")], TEXT);
                        for (fill, on) in TINTS {
                            need(on, &[get(fill)], TEXT);
                        }
                    }
                }
            }
        }
        assert!(wrong.is_empty(), "{} провалов: {:#?}", wrong.len(), &wrong[..wrong.len().min(20)]);
    }

    /// Цвет, который и так держит порог, остаётся как был: окно не трогает
    /// того, что читается.
    #[test]
    fn readable_keeps_what_already_reads() {
        let back = rgb(0xf5ead8);
        assert_eq!(readable(rgb(0x201e1d), &[back], TEXT, Rgb::BLACK), rgb(0x201e1d));
        let pushed = readable(rgb(0xc0b6a5), &[back], TEXT, Rgb::BLACK);
        assert_ne!(pushed, rgb(0xc0b6a5));
        assert!(contrast(pushed, back) >= TEXT);
    }

    /// Плохо читаемая выбранная пара названа числом — десятыми вниз, —
    /// а читаемая и невыбранная — нет.
    #[test]
    fn low_names_only_chosen_pairs() {
        let (light, dark) = themes();
        let mut choice = Choice::new(Scheme::Light);
        choice.set(Pick::Text, Some(rgb(0xd8cfc0)));
        let painted = paint(&choice, &light, &dark).expect("выбран текст");
        let lows = low(&choice, &painted);
        assert_eq!(lows.len(), 1, "{lows:?}");
        assert_eq!(lows[0].pair, Pair::Text);
        assert_eq!(lows[0].need_tenths, 45);
        assert!(lows[0].tenths < 45);

        let mut fine = Choice::new(Scheme::Light);
        fine.set(Pick::Text, Some(rgb(0x102030)));
        fine.set(Pick::ButtonText, Some(Rgb::WHITE));
        let painted = paint(&fine, &light, &dark).expect("выбран текст");
        assert_eq!(low(&fine, &painted), Vec::new());

        let mut buttons = Choice::new(Scheme::Light);
        buttons.set(Pick::Buttons, Some(rgb(0xf3e9d8)));
        buttons.set(Pick::ButtonText, Some(rgb(0xfffaf3)));
        let painted = paint(&buttons, &light, &dark).expect("выбраны кнопки");
        let pairs: Vec<Pair> = low(&buttons, &painted).into_iter().map(|low| low.pair).collect();
        assert_eq!(pairs, [Pair::ButtonText, Pair::Buttons]);
    }

    /// Запись устройства читается обратно в тот же выбор и ту же палитру;
    /// незнакомая строка пропускается, чужое имя и бумага на страницу не
    /// встают, а запись без выбора или чужого номера — не запись.
    #[test]
    fn the_record_survives_the_device() {
        let (light, dark) = themes();
        let mut choice = Choice::new(Scheme::Dark);
        choice.set(Pick::Buttons, Some(rgb(0x2266cc)));
        choice.set(Pick::Text, Some(rgb(0xeeeeee)));
        let record = Record {
            choice,
            paint: paint(&choice, &light, &dark).expect("выбраны кнопки"),
        };
        let text = record.to_text();
        assert!(text.starts_with("kvartik-colors 1\nscheme dark\npick buttons #2266cc\npick text #eeeeee\npaint dark\n"));
        assert_eq!(Record::parse(&text), Some(record.clone()));

        let noisy = format!(
            "{text}someday new words\n--Surface #000000\n--fill-paper #000000\n--x;y #000000\n--surface red\ncolor #000000\npick sparkles #000000\n"
        );
        assert_eq!(Record::parse(&noisy), Some(record.clone()));

        // При открытии окно берёт из записи только палитру — ту же самую.
        for text in [&text, &noisy] {
            let (scheme, colors) = painted(text).expect("палитра читается");
            assert_eq!(scheme, record.paint.scheme);
            let read: Vec<(String, String)> =
                colors.iter().map(|&(name, color)| (name.to_owned(), color.to_owned())).collect();
            let expected: Vec<(String, String)> =
                record.paint.colors.iter().map(|(name, color)| (name.to_owned(), color.hex())).collect();
            assert_eq!(read, expected);
        }

        for broken in [
            "",
            "kvartik-colors 2\nscheme light\npick text #000000\npaint light\n",
            "kvartik-colors 1\nscheme light\npaint light\n--surface #000000\n",
            "kvartik-colors 1\nscheme light\npick sparkles #000000\npaint light\n--surface #000000\n",
            "kvartik-colors 1\npick text #000000\npaint light\n",
            "kvartik-colors 1\nscheme light\npick text #000000\n",
        ] {
            assert_eq!(Record::parse(broken), None, "{broken:?}");
            assert_eq!(painted(broken), None, "{broken:?}");
        }
    }

    /// Стиль первого кадра (задача 89) — та же палитра, что окно ставит на
    /// `<html>`: каждая переменная и `color-scheme`, одним правилом
    /// `html:root`, а чужое имя в правило не попадает. Отметка — от текста:
    /// та же палитра — та же отметка, другой цвет — другая.
    #[test]
    fn the_sheet_is_the_palette() {
        let (light, dark) = themes();
        let mut choice = Choice::new(Scheme::Light);
        choice.set(Pick::Background, Some(rgb(0x0b1d33)));
        let mut painted = paint(&choice, &light, &dark).expect("выбран фон");
        let sheet = Sheet::of(&painted);
        assert_eq!(sheet.scheme, Scheme::Dark);
        assert!(sheet.css.starts_with("html:root{color-scheme:dark;--"), "{}", sheet.css);
        assert!(sheet.css.ends_with(";}\n") && sheet.css.matches('{').count() == 1, "{}", sheet.css);
        assert!(sheet.css.contains("--surface:#0b1d33;"), "{}", sheet.css);
        for (name, color) in painted.colors.iter() {
            assert!(sheet.css.contains(&format!("{name}:{};", color.hex())), "{name}");
        }
        assert_eq!(sheet.mark.len(), 16);
        assert!(sheet.mark.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)), "{}", sheet.mark);
        assert_eq!(Sheet::of(&painted), sheet);

        painted.colors.set("--x}body{color:red", rgb(0x000000));
        painted.colors.set("--fill-paper", rgb(0x000000));
        assert_eq!(Sheet::of(&painted), sheet, "чужое имя и бумага в стиль не идут");

        painted.colors.set("--surface", rgb(0x0b1d34));
        let other = Sheet::of(&painted);
        assert!(other.css.contains("--surface:#0b1d34;"));
        assert_ne!(other.mark, sheet.mark);
    }
}
