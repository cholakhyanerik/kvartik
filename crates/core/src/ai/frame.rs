//! Рамка ИИ-помощника (задача 111). Помощник живёт на своём происхождении —
//! `ai.kvartik.com`: только его CSP пускает к четырём адресам поставщиков ИИ
//! и только он хранит ключ, а у программы на `kvartik.com` в `connect-src`
//! снова один свой сайт. Окно встраивает помощника рамкой, и весь помощник —
//! ключ, вопрос и разговор — в ней: поставщику уходит только то, что человек
//! отправил нажатием в самой рамке, и заражённый код окна не задаст и вопроса
//! (CLAUDE.md, Правило 13, «ИИ-помощник»).
//!
//! Здесь — какое происхождение у рамки окна и у окна рамки и сообщения между
//! ними. Сообщение — строка JSON через `postMessage`, и от другой стороны оно
//! чужой ввод (Правило 13): длина проверяется до разбора, поле — при разборе,
//! а незнакомое пропускается. Это договор между версиями, как заметки «Что
//! нового»: окно приходит из кэша service worker и бывает старше рамки,
//! которую отдаёт сеть, — новое поле и новый вид сообщения прежняя сторона
//! пропустит, а переименовать или сменить смысл нельзя.

use serde::{Deserialize, Serialize};

use crate::i18n::Lang;
use crate::model::date::Date;
use crate::theme::Rgb;

use super::MAX_QUESTION_CHARS;

/// Домен программы.
pub const SITE: &str = "kvartik.com";
/// Домен рамки помощника — своё происхождение навсегда: ключ человека
/// хранит он (CLAUDE.md, «Происхождение сайта — навсегда»).
pub const FRAME: &str = "ai.kvartik.com";
/// Предпросмотр программы — `<ветка>.kvartik.pages.dev`, а рамки той же
/// выкладки — `<ветка>.kvartik-ai.pages.dev`: ветка у обеих одна.
const SITE_PREVIEW: &str = ".kvartik.pages.dev";
const FRAME_PREVIEW: &str = ".kvartik-ai.pages.dev";

/// Предел сообщения в байтах: самое длинное — вопрос с «моими данными»,
/// а дальше разбор не идёт (Правило 13).
pub const MAX_MESSAGE_BYTES: usize = 32 * 1024;
/// Предел своих цветов в сообщении: палитра программы — пара десятков
/// переменных.
const MAX_COLORS: usize = 64;
/// Предел записи ключа из окна (`ai::Setup::to_text`).
const MAX_SETUP_BYTES: usize = 2 * 1024;
/// Предел высоты рамки, точек CSS: дальше — ошибка, а не разговор.
const MAX_HEIGHT: u32 = 200_000;

/// Происхождение рамки для окна на `host` (`location.host`): `None` — окно
/// не на своём домене и не на предпросмотре, и рамки у него нет.
pub fn frame_origin(host: &str) -> Option<String> {
    if host == SITE {
        return Some(format!("https://{FRAME}"));
    }
    let label = host.strip_suffix(SITE_PREVIEW).filter(|label| is_label(label))?;
    Some(format!("https://{label}{FRAME_PREVIEW}"))
}

/// Происхождение окна для рамки на `host` (`location.host`): только с него
/// рамка принимает сообщения и только ему отвечает. `None` — рамку открыли
/// не там, и она не работает.
pub fn parent_origin(host: &str) -> Option<String> {
    if host == FRAME {
        return Some(format!("https://{SITE}"));
    }
    let label = host.strip_suffix(FRAME_PREVIEW).filter(|label| is_label(label))?;
    Some(format!("https://{label}{SITE_PREVIEW}"))
}

/// Метка адреса предпросмотра: латиница, цифры и дефис, не с краю дефис.
/// Точки нет: иначе `x.kvartik.pages.dev` оказался бы меткой
/// `чужое.x`.
fn is_label(label: &str) -> bool {
    !label.is_empty()
        && label.len() <= 63
        && label.bytes().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && !label.starts_with('-')
        && !label.ends_with('-')
}

/// Свои цвета окна: тема и переменные палитры (`theme::painted`).
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug, Default)]
pub struct Colors {
    /// `light` или `dark` — для `color-scheme`.
    pub scheme: String,
    /// Переменная CSS и цвет `#rrggbb`.
    pub vars: Vec<(String, String)>,
}

impl Colors {
    /// Годные цвета: тема — одна из двух, переменные — `--имя` и цвет
    /// `#rrggbb`, и не больше [`MAX_COLORS`]. Иное — не цвета: рамка
    /// останется в цветах программы.
    fn valid(&self) -> bool {
        let name = |name: &str| {
            name.len() <= 48
                && name.strip_prefix("--").is_some_and(|rest| {
                    !rest.is_empty() && rest.bytes().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
                })
        };
        matches!(self.scheme.as_str(), "light" | "dark")
            && self.vars.len() <= MAX_COLORS
            && self
                .vars
                .iter()
                .all(|(var, color)| name(var) && color.len() == 7 && Rgb::parse(color).is_some())
    }
}

/// Окно — рамке.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ToFrame {
    /// Что рамке знать об окне — целиком, при каждом изменении: язык окна,
    /// сегодня по часам окна (`None` — часы врут), есть ли профиль для
    /// «Добавить мои данные» и свои цвета (`None` — цвета программы).
    State {
        lang: String,
        #[serde(default)]
        today: Option<String>,
        #[serde(default)]
        me: bool,
        #[serde(default)]
        colors: Option<Colors>,
    },
    /// «Мои данные» текстом — ответ на [`FromFrame::WantMe`].
    Me { text: String },
    /// Ключ, подключённый в 0.0.158 ещё в окне, — записью `ai::Setup`:
    /// рамка его запомнит, а окно сотрёт свою запись.
    Setup { text: String },
    /// Вид сообщения новее этой версии.
    #[serde(other)]
    Unknown,
}

/// Рамка — окну.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FromFrame {
    /// Рамка запустилась и ждёт [`ToFrame::State`].
    Ready,
    /// Начато ли в рамке то, что стёрла бы перезагрузка окна: `busy` — ответ
    /// пишется, вписан вопрос или ключ; `talk` — есть разговор. Пока `busy`,
    /// новая версия окна ждёт, а разговор держит её, только пока экран
    /// помощника на виду (`Forms::hold_while`, ревью задачи 102).
    Busy {
        busy: bool,
        #[serde(default)]
        talk: bool,
    },
    /// Высота содержимого рамки, точек CSS: окно растит рамку под неё, без
    /// своей прокрутки внутри экрана.
    Height { px: u32 },
    /// Нажали «Добавить мои данные».
    WantMe,
    /// Какой ИИ подключён (`ai::Provider::code`) — для строки на экране
    /// налога; `None` — никакой.
    Connected {
        #[serde(default)]
        provider: Option<String>,
    },
    /// Ключ из окна запомнен — окно стирает свою запись.
    SetupTaken,
    /// Вид сообщения новее этой версии.
    #[serde(other)]
    Unknown,
}

/// Что рамка поняла из [`ToFrame::State`].
#[derive(Clone, PartialEq, Debug)]
pub struct State {
    pub lang: Lang,
    pub today: Option<Date>,
    pub me: bool,
    pub colors: Option<Colors>,
}

impl ToFrame {
    /// Состояние окна — сообщением.
    pub fn state(lang: Lang, today: Option<Date>, me: bool, colors: Option<Colors>) -> Self {
        Self::State {
            lang: lang.code().to_owned(),
            today: today.map(Date::iso),
            me,
            colors,
        }
    }

    /// Текст сообщения.
    pub fn to_text(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// Сообщение окна; `None` — длиннее предела, не JSON или поле не годится.
    pub fn parse(text: &str) -> Option<Self> {
        if text.len() > MAX_MESSAGE_BYTES {
            return None;
        }
        let message: Self = serde_json::from_str(text).ok()?;
        match &message {
            Self::State { lang, today, colors, .. } => {
                Lang::from_code(lang)?;
                if today.as_deref().is_some_and(|day| Date::from_iso(day).is_none()) {
                    return None;
                }
                if colors.as_ref().is_some_and(|colors| !colors.valid()) {
                    return None;
                }
            }
            Self::Me { text } => {
                if text.chars().count() > MAX_QUESTION_CHARS {
                    return None;
                }
            }
            Self::Setup { text } => {
                if text.len() > MAX_SETUP_BYTES {
                    return None;
                }
            }
            Self::Unknown => {}
        }
        Some(message)
    }

    /// Состояние окна, когда сообщение — оно.
    pub fn as_state(&self) -> Option<State> {
        match self {
            Self::State { lang, today, me, colors } => Some(State {
                lang: Lang::from_code(lang)?,
                today: today.as_deref().and_then(Date::from_iso),
                me: *me,
                colors: colors.clone(),
            }),
            _ => None,
        }
    }
}

impl FromFrame {
    /// Текст сообщения.
    pub fn to_text(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// Сообщение рамки; `None` — длиннее предела, не JSON или поле не годится.
    pub fn parse(text: &str) -> Option<Self> {
        if text.len() > MAX_MESSAGE_BYTES {
            return None;
        }
        let message: Self = serde_json::from_str(text).ok()?;
        match &message {
            Self::Height { px } if *px > MAX_HEIGHT => None,
            Self::Connected { provider: Some(code) } if super::Provider::from_code(code).is_none() => None,
            _ => Some(message),
        }
    }
}

#[cfg(test)]
mod tests;
