//! ИИ-помощник (задача 102): поставщики и их адреса, тело запроса, разбор
//! потока ответа и слова о сбое — байты и строки на входе и на выходе.
//!
//! Ключ API подключает человек сам — своей учётной записи у Anthropic
//! (Claude), OpenAI (ChatGPT), Google (Gemini) или xAI (Grok), — и вопрос
//! уходит прямо из его браузера этому поставщику, а не на сервер Kvartik
//! (решение человека 2026-11-17, общего ключа продавца нет — 2026-11-19).
//! Сеть, ключ на устройстве и экран — в окне (`platform::ai`,
//! `store/device.rs`, `ui/assistant.rs`); здесь — что спросить и как прочесть
//! ответ, без браузера и без часов: «сегодня» приходит параметром
//! (Архитектура, «Время — параметр»).
//!
//! Адрес поставщика — ровно тот, что стоит в CSP сайта
//! (`crates/app/public/root/_headers`, `connect-src`): адрес с путём, а не
//! весь сервер, — заражённая зависимость не дотянется ни до файлов, ни до
//! пакетов вопросов поставщика; сам вопрос — всё же дверь наружу (CLAUDE.md,
//! Правило 13, «ИИ-помощник»). Что адреса и CSP одни, держит тест окна
//! `crates/app/tests/csp.rs`.
//!
//! Форматов провода два: Messages у Anthropic и Chat Completions у трёх
//! остальных — OpenAI, xAI и OpenAI-совместимый адрес Gemini. Ответ — потоком
//! (SSE): первые слова видно сразу, а не через полминуты (Правило 1).
//!
//! Суммы налога модель не считает: в вопросе нет данных книги, а справка
//! Kvartik, которую получает модель, — файл правил с источниками, тот же, по
//! которому считает программа (`tax::rules`). Ответ модели — не консультация,
//! и окно говорит это словами (условия, п. 8).

use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

use crate::i18n::{self, Key, Lang, fill, t};
use crate::model::date::Date;
use crate::model::profile::{ProfileData, Regime, Residency};
use crate::tax::{rules, turnover};

/// Поставщик ИИ. Имя в записи устройства — [`Provider::code`]: оно не
/// меняется, иначе подключённый ключ у всех пропал бы.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Provider {
    Claude,
    ChatGpt,
    Gemini,
    Grok,
}

/// Формат провода поставщика.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Wire {
    /// Messages API Anthropic.
    Messages,
    /// Chat Completions — OpenAI и совместимые с ним адреса.
    Chat,
}

impl Provider {
    /// Все — в порядке выбора на экране.
    pub const ALL: [Provider; 4] = [Provider::Claude, Provider::ChatGpt, Provider::Gemini, Provider::Grok];

    /// Имя в записи устройства.
    pub const fn code(self) -> &'static str {
        match self {
            Provider::Claude => "claude",
            Provider::ChatGpt => "chatgpt",
            Provider::Gemini => "gemini",
            Provider::Grok => "grok",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|provider| provider.code() == code)
    }

    /// Имя ИИ на экране — имя продукта, не перевод.
    pub const fn name(self) -> &'static str {
        match self {
            Provider::Claude => "Claude",
            Provider::ChatGpt => "ChatGPT",
            Provider::Gemini => "Gemini",
            Provider::Grok => "Grok",
        }
    }

    /// Компания, которая выдаёт ключ и берёт плату за ответы.
    pub const fn company(self) -> &'static str {
        match self {
            Provider::Claude => "Anthropic",
            Provider::ChatGpt => "OpenAI",
            Provider::Gemini => "Google",
            Provider::Grok => "xAI",
        }
    }

    /// Адрес, на который уходит вопрос, — тот же, что в CSP (шапка модуля).
    /// У Gemini — его OpenAI-совместимый адрес: тот же формат провода, что
    /// у OpenAI и xAI, и путь без имени модели — CSP держит его целиком.
    /// Все четыре отвечают браузеру (CORS): из страницы на 127.0.0.1 в
    /// Chrome 158 и Firefox 160 — ответ с кодом читается, у Anthropic — только
    /// с заголовком прямого доступа из браузера (проверено вживую
    /// 2026-11-19). Chat Completions у xAI в его документации — «legacy»
    /// и рядом с Responses API назван устаревшим, но описан и работает
    /// (docs.x.ai, правка 2026-10-20; сверено 2026-11-19): уберут — новый
    /// адрес здесь и в CSP.
    pub const fn url(self) -> &'static str {
        match self {
            Provider::Claude => "https://api.anthropic.com/v1/messages",
            Provider::ChatGpt => "https://api.openai.com/v1/chat/completions",
            Provider::Gemini => "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions",
            Provider::Grok => "https://api.x.ai/v1/chat/completions",
        }
    }

    /// Модель по умолчанию — чужое имя, которое стареет (Правило 6): сверено
    /// 2026-11-19 по документации поставщиков. Claude Opus 5.5 — нынешний
    /// Opus (skill claude-api, таблица от 2026-11-03); GPT-6.2 Sol — «баланс
    /// ума и цены», Chat Completions и поток у него есть
    /// (developers.openai.com, страница модели); Gemini 3.8 Flash — среди
    /// стабильных (aistudio.google.com/docs/models); Grok 4.7 — нынешний
    /// флагман xAI (docs.x.ai/developers/models). Другую человек впишет сам,
    /// а модель, которой поставщик не знает, окно называет словами.
    pub const fn model(self) -> &'static str {
        match self {
            Provider::Claude => "claude-opus-5-5",
            Provider::ChatGpt => "gpt-6.2-sol",
            Provider::Gemini => "gemini-3.8-flash",
            Provider::Grok => "grok-4.7",
        }
    }

    /// Страница, где поставщик выдаёт ключ API (сверено 2026-11-19: у
    /// Anthropic прежний адрес console.anthropic.com ведёт сюда же).
    pub const fn key_page(self) -> &'static str {
        match self {
            Provider::Claude => "https://platform.claude.com/settings/keys",
            Provider::ChatGpt => "https://platform.openai.com/api-keys",
            Provider::Gemini => "https://aistudio.google.com/apikey",
            Provider::Grok => "https://console.x.ai/",
        }
    }

    const fn wire(self) -> Wire {
        match self {
            Provider::Claude => Wire::Messages,
            Provider::ChatGpt | Provider::Gemini | Provider::Grok => Wire::Chat,
        }
    }

    /// Чей это ключ по его началу; `None` — не узнать. Только подсказка
    /// человеку, вставившему не тот ключ: запрет по чужому соглашению об
    /// именах отказал бы и верному ключу нового вида. У Gemini видов два:
    /// прежний `AIza…` и `AQ.…`, который AI Studio выдаёт теперь (форум
    /// Google AI для разработчиков, 2026; такой ключ ответил и на адресе,
    /// совместимом с OpenAI, — проверено вживую 2026-11-20).
    pub fn guess(key: &str) -> Option<Self> {
        let key = key.trim();
        if key.starts_with("sk-ant-") {
            Some(Provider::Claude)
        } else if key.starts_with("xai-") {
            Some(Provider::Grok)
        } else if key.starts_with("AIza") || key.starts_with("AQ.") {
            Some(Provider::Gemini)
        } else if key.starts_with("sk-") {
            Some(Provider::ChatGpt)
        } else {
            None
        }
    }
}

/// Версия API Anthropic — заголовок `anthropic-version`. С 2026-10-21
/// запрос с `temperature`, `top_p` или `top_k` получает 400 — их здесь нет
/// (skill claude-api, `api-version-migration.md`, 2026-11-19).
const ANTHROPIC_VERSION: &str = "2026-10-21";

/// Модели Anthropic, у которых классификаторы безопасности могут отклонить
/// запрос, и сервер тогда сам отвечает другой моделью (`fallbacks:
/// "default"`, бета `server-side-fallback-2026-07-01`; skill claude-api,
/// 2026-11-19). Другой модели поле не шлётся: у неё такой подмены нет.
const FALLBACK_MODELS: [&str; 5] = [
    "claude-fable-5-2",
    "claude-fable-5-1",
    "claude-opus-5-5",
    "claude-opus-5",
    "claude-sonnet-5-5",
];

/// Бета-заголовок подмены отклонённого запроса.
const FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";

/// Предел длины ответа Anthropic в знаках модели (у него поле обязательное).
/// Платит человек: 16 тыс. на выходе у Claude Opus 5.5 — не больше 0,32 $ за
/// ответ (20 $ за миллион, skill claude-api, 2026-11-03), а ответ о налоге
/// с рассуждением укладывается и в 4–8 тыс. Остальным поставщикам предел не
/// шлётся: у их моделей с рассуждением поле называется по-разному.
const MAX_TOKENS: u32 = 16_000;

/// Сколько вопросов в одном разговоре. Каждый вопрос несёт с собой весь
/// разговор, и цена растёт с каждым; к тому же длинный разговор модель
/// держит хуже. Дальше — «Новый разговор».
pub const MAX_TURNS: usize = 10;

/// Самый длинный вопрос, в знаках: в поле — не текст договора.
pub const MAX_QUESTION_CHARS: usize = 4_000;

/// Самый длинный ответ, в байтах: дальше поток обрывается словами «дошёл до
/// предела» — накопитель ограничен (Правило 1).
const MAX_ANSWER_BYTES: usize = 200_000;

/// Самая длинная строка потока: событие SSE — строка JSON в килобайты.
/// Дальше — ответ не того вида, а не повод занять всю память.
const MAX_LINE_BYTES: usize = 1_000_000;

/// Самый длинный ключ API: у поставщиков — до двухсот знаков.
const MAX_KEY_CHARS: usize = 512;

/// Самое длинное имя модели.
const MAX_MODEL_CHARS: usize = 100;

/// Сколько знаков слов поставщика о сбое показать.
const MAX_SAID_CHARS: usize = 300;

/// Подключённый ИИ — запись устройства (`store/device.rs`): в копию не идёт,
/// как и язык окна.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Setup {
    pub provider: Provider,
    /// Ключ API — без пробелов по краям.
    pub key: String,
    /// Имя модели.
    pub model: String,
}

/// Что не так с введённым ключом или моделью.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SetupProblem {
    /// Ключа нет.
    NoKey,
    /// В ключе пробел, перенос строки или знак не из ASCII: в заголовок
    /// запроса такой не встанет, а скопирован он, значит, не целиком.
    KeySpaces,
    /// Ключ длиннее [`MAX_KEY_CHARS`].
    KeyLong,
    /// Имя модели — не из букв, цифр и `. - _ : /` или длиннее
    /// [`MAX_MODEL_CHARS`].
    Model,
}

impl SetupProblem {
    /// Слова о нём под полем.
    pub const fn key(self) -> Key {
        match self {
            SetupProblem::NoKey => Key::AiKeyEmpty,
            SetupProblem::KeySpaces => Key::AiKeySpaces,
            SetupProblem::KeyLong => Key::AiKeyLong,
            SetupProblem::Model => Key::AiModelBad,
        }
    }
}

/// Запись устройства: JSON со своими именами полей.
#[derive(Serialize)]
struct Stored<'a> {
    provider: &'a str,
    key: &'a str,
    model: &'a str,
}

/// Та же запись при чтении — строками, а не ссылками в текст: в JSON ключ
/// с кавычкой записан с `\`, и ссылкой его не прочесть.
#[derive(Deserialize)]
struct Read {
    provider: String,
    key: String,
    model: String,
}

impl Setup {
    /// Подключить ИИ: ключ — без пробелов по краям, пустая модель — модель
    /// по умолчанию.
    pub fn new(provider: Provider, key: &str, model: &str) -> Result<Self, SetupProblem> {
        let key = key.trim();
        if key.is_empty() {
            return Err(SetupProblem::NoKey);
        }
        if !key.chars().all(|c| c.is_ascii_graphic()) {
            return Err(SetupProblem::KeySpaces);
        }
        if key.chars().count() > MAX_KEY_CHARS {
            return Err(SetupProblem::KeyLong);
        }
        let model = match model.trim() {
            "" => provider.model(),
            model => model,
        };
        let allowed = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ':' | '/');
        if model.chars().count() > MAX_MODEL_CHARS || !model.chars().all(allowed) {
            return Err(SetupProblem::Model);
        }
        Ok(Self {
            provider,
            key: key.to_owned(),
            model: model.to_owned(),
        })
    }

    /// Текст записи устройства.
    pub fn to_text(&self) -> String {
        serde_json::to_string(&Stored {
            provider: self.provider.code(),
            key: &self.key,
            model: &self.model,
        })
        .unwrap_or_default()
    }

    /// Запись устройства; `None` — не читается: окно попросит ключ заново,
    /// а не пошлёт поставщику то, что в ней лежит.
    pub fn parse(text: &str) -> Option<Self> {
        let stored: Read = serde_json::from_str(text).ok()?;
        Self::new(Provider::from_code(&stored.provider)?, &stored.key, &stored.model).ok()
    }

    /// Последние четыре знака ключа — показать, какой подключён, не
    /// показывая его.
    pub fn tail(&self) -> &str {
        let start = self.key.char_indices().rev().nth(3).map_or(0, |(at, _)| at);
        &self.key[start..]
    }
}

/// Вопрос и ответ разговора.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Turn {
    pub question: String,
    pub answer: String,
}

/// Что спросить: подключённый ИИ, язык окна, сегодня, прежние вопросы
/// и ответы разговора и сам вопрос.
pub struct Ask<'a> {
    pub setup: &'a Setup,
    pub lang: Lang,
    /// `None` — часы устройства врут, и модель узнает, что дня нет.
    pub today: Option<Date>,
    pub turns: &'a [Turn],
    pub question: &'a str,
}

/// Запрос к поставщику: адрес, заголовки и тело JSON.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Request {
    pub url: &'static str,
    pub headers: Vec<(&'static str, String)>,
    pub body: String,
}

/// Наставление модели — одинаковое у всех, без данных человека. По-английски:
/// это не слова окна, их человек не видит, а модели всех четырёх поставщиков
/// следуют английским наставлениям надёжнее всего; отвечает модель на языке
/// вопроса. Названия экранов — на языке окна, как в меню: модель отсылает к
/// ним, а не считает суммы сама.
fn instructions(lang: Lang, today: Option<Date>) -> String {
    let language = match lang {
        Lang::Am => "Armenian",
        Lang::Ru => "Russian",
        Lang::En => "English",
    };
    let today = today.map_or_else(|| "unknown (the device clock is wrong)".to_owned(), Date::iso);
    let screen = |key: Key| format!("«{}»", t(lang, key));
    format!(
        "You are the tax assistant built into Kvartik, a web app for individual entrepreneurs (ԱՁ) in \
Armenia, most of them on turnover tax (շրջանառության հարկ) or micro-entrepreneurship. Today is {today}. \
The user's Kvartik window is in {language}; answer in the language the user writes in.

How to answer:
- Answer about Armenian taxes, deadlines, returns and applications to the State Revenue Committee (ՊԵԿ, \
src.am), invoices, acts and settlement documents, and how to do things in Kvartik.
- Rely on the Tax Code of Armenia (ՀՕ-165-Ն) and the other laws in force on the date the question is \
about. Name the article, part and point, and give the arlis.am link when you know it. Say when the answer \
depends on facts you don't know, and which ones.
- Kvartik's rules file follows: the rates, thresholds, deadlines and lists the app calculates with, each \
record with its legal source and validity dates. Use these numbers; if you believe one is outdated or \
wrong, say so plainly.
- Don't calculate the user's tax amounts: Kvartik calculates them from the user's own records. Point to \
its screens instead: {tax}, {annual}, {deadlines}, {report}.
- If you aren't sure, say so and suggest checking with an accountant or asking ՊԵԿ in writing. Never \
invent article numbers or links.
- Write plain text: short paragraphs and simple lists with \"-\". No Markdown: no **, no #, no tables.

Kvartik's rules file (JSON):
{rules}",
        tax = screen(Key::TaxTitle),
        annual = screen(Key::AnnualTitle),
        deadlines = screen(Key::DeadlinesTitle),
        report = screen(Key::ReportTitle),
        rules = rules::armenia_text(),
    )
}

/// Сообщение разговора в теле запроса.
#[derive(Serialize)]
struct Message<'a> {
    role: &'static str,
    content: &'a str,
}

/// Тело запроса Messages API Anthropic. Своими типами, а не деревом
/// `serde_json::Value`: дерево и его сериализатор легли бы в часть wasm ради
/// одного запроса (CLAUDE.md, Правило 1).
#[derive(Serialize)]
struct MessagesBody<'a> {
    model: &'a str,
    max_tokens: u32,
    stream: bool,
    cache_control: CacheControl,
    system: &'a str,
    messages: &'a [Message<'a>],
    #[serde(skip_serializing_if = "Option::is_none")]
    fallbacks: Option<&'static str>,
}

#[derive(Serialize)]
struct CacheControl {
    #[serde(rename = "type")]
    kind: &'static str,
}

/// Тело запроса Chat Completions.
#[derive(Serialize)]
struct ChatBody<'a> {
    model: &'a str,
    stream: bool,
    messages: &'a [Message<'a>],
}

/// Запрос с вопросом: наставление, прежние вопросы и ответы, новый вопрос.
/// Прежние ответы — только текстом: рассуждений модели окно не хранит, а
/// история без них у Anthropic не ломает проверку истории (убраны все блоки
/// рассуждения — skill claude-api, «preserved thinking», 2026-11-19).
pub fn request(ask: &Ask<'_>) -> Request {
    let setup = ask.setup;
    let system = instructions(ask.lang, ask.today);
    let wire = setup.provider.wire();
    let mut messages = Vec::with_capacity(ask.turns.len() * 2 + 2);
    if wire == Wire::Chat {
        messages.push(Message {
            role: "system",
            content: &system,
        });
    }
    for turn in ask.turns {
        messages.push(Message {
            role: "user",
            content: &turn.question,
        });
        messages.push(Message {
            role: "assistant",
            content: &turn.answer,
        });
    }
    messages.push(Message {
        role: "user",
        content: ask.question,
    });
    let json = "application/json".to_owned();
    match wire {
        Wire::Messages => {
            let fallback = FALLBACK_MODELS.contains(&setup.model.as_str());
            let mut headers = vec![
                ("content-type", json),
                ("x-api-key", setup.key.clone()),
                ("anthropic-version", ANTHROPIC_VERSION.to_owned()),
                // Ключ человека — в его же браузере: заголовок говорит
                // Anthropic, что это и задумано (без него CORS отказывает).
                ("anthropic-dangerous-direct-browser-access", "true".to_owned()),
            ];
            if fallback {
                headers.push(("anthropic-beta", FALLBACK_BETA.to_owned()));
            }
            // Кэш подсказки — весь запрос до последнего вопроса: наставление
            // с файлом правил — около десяти тысяч знаков модели, и второй
            // вопрос разговора читает его из кэша, в двадцать раз дешевле.
            let body = MessagesBody {
                model: &setup.model,
                max_tokens: MAX_TOKENS,
                stream: true,
                cache_control: CacheControl { kind: "ephemeral" },
                system: &system,
                messages: &messages,
                fallbacks: fallback.then_some("default"),
            };
            Request {
                url: setup.provider.url(),
                headers,
                body: serde_json::to_string(&body).unwrap_or_default(),
            }
        }
        Wire::Chat => {
            let body = ChatBody {
                model: &setup.model,
                stream: true,
                messages: &messages,
            };
            Request {
                url: setup.provider.url(),
                headers: vec![("content-type", json), ("authorization", format!("Bearer {}", setup.key))],
                body: serde_json::to_string(&body).unwrap_or_default(),
            }
        }
    }
}

/// Чем кончился ответ.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum End {
    /// Целиком.
    Done,
    /// Оборвался на пределе длины.
    Long,
    /// Модель или её классификатор не стали отвечать.
    Refused,
    /// Поставщик сообщил о сбое посреди потока.
    Failed(Failure),
    /// Поток кончился без конца ответа: связь оборвалась.
    Cut,
}

/// Сбой: что случилось, код ответа и слова поставщика.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Failure {
    pub fault: Fault,
    /// Код ответа HTTP; `None` — сбой посреди потока или без ответа.
    pub status: Option<u16>,
    /// Слова поставщика о сбое — коротко, одной строкой; `None` — их нет.
    pub said: Option<String>,
}

/// Что случилось.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Fault {
    /// Ключ не принят.
    Key,
    /// Нет доступа: модель, страна, права ключа.
    Denied,
    /// Модели с таким именем у поставщика нет.
    Model,
    /// Предел частоты или кончились деньги на балансе у поставщика.
    Limit,
    /// Поставщик перегружен или сбоит.
    Busy,
    /// Поставщик не принял запрос — слова его.
    Request,
    /// Ответ не того вида: программа его не прочтёт.
    Unreadable,
    /// Ответа нет: сеть, а у OpenAI — и неверный ключ (его отказ браузеру
    /// не виден, CORS).
    Network,
    /// Другой код ответа.
    Other,
}

impl Failure {
    /// Сбой без слов поставщика.
    pub const fn of(fault: Fault) -> Self {
        Self {
            fault,
            status: None,
            said: None,
        }
    }
}

/// Сбой по ответу не с кодом 2xx: код и тело. Неверный ключ Gemini и xAI
/// отвечают кодом 400 со словами о ключе (проверено 2026-11-19), остальные —
/// 401. У OpenAI ответ 401 без заголовков CORS, и браузер его не видит
/// (там же): такой сбой — [`Fault::Network`] в окне.
pub fn failure(status: u16, body: &str) -> Failure {
    let lower = body.to_ascii_lowercase();
    let fault = match status {
        401 => Fault::Key,
        400 if lower.contains("api key") || lower.contains("api_key") => Fault::Key,
        403 => Fault::Denied,
        404 => Fault::Model,
        402 | 429 => Fault::Limit,
        500..=599 => Fault::Busy,
        400..=499 => Fault::Request,
        _ => Fault::Other,
    };
    Failure {
        fault,
        status: Some(status),
        said: said(body),
    }
}

/// Тело ответа об ошибке: `error` — объект или строка, рядом бывает
/// `message`. Кусок `error` читается потом, по его виду.
#[derive(Deserialize)]
struct ErrorBody<'a> {
    #[serde(borrow)]
    error: Option<&'a RawValue>,
    message: Option<String>,
}

#[derive(Deserialize)]
struct ErrorObject {
    message: Option<String>,
}

/// Слова поставщика о сбое из тела ответа: у Anthropic, OpenAI и Gemini —
/// `error.message`, у xAI — `error` строкой, у OpenAI-совместимого адреса
/// Gemini — массивом из одного такого (проверено 2026-11-19).
fn said(body: &str) -> Option<String> {
    let body = body.trim();
    let error: ErrorBody<'_> = if body.starts_with('[') {
        serde_json::from_str::<Vec<ErrorBody<'_>>>(body).ok()?.into_iter().next()?
    } else {
        serde_json::from_str(body).ok()?
    };
    let text = match error.error {
        Some(raw) => text_of(raw).or_else(|| serde_json::from_str::<ErrorObject>(raw.get()).ok()?.message)?,
        None => error.message?,
    };
    short(&text)
}

/// Кусок JSON, если это строка.
fn text_of(raw: &RawValue) -> Option<String> {
    raw.get().starts_with('"').then(|| serde_json::from_str(raw.get()).ok()).flatten()
}

/// Слова одной строкой и не длиннее [`MAX_SAID_CHARS`]; пусто — `None`.
fn short(text: &str) -> Option<String> {
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.is_empty() {
        return None;
    }
    if line.chars().count() <= MAX_SAID_CHARS {
        return Some(line);
    }
    let mut cut: String = line.chars().take(MAX_SAID_CHARS).collect();
    cut.push('…');
    Some(cut)
}

/// Событие потока Anthropic — то, что из него нужно.
#[derive(Deserialize)]
struct MessageEvent<'a> {
    #[serde(rename = "type")]
    kind: String,
    delta: Option<MessageDelta>,
    #[serde(borrow)]
    error: Option<StreamError<'a>>,
}

#[derive(Deserialize)]
struct MessageDelta {
    #[serde(rename = "type")]
    kind: Option<String>,
    text: Option<String>,
    stop_reason: Option<String>,
}

/// Сбой посреди потока: тип и слова. Код у OpenAI — строка, у других
/// бывает числом: читается потом, по его виду.
#[derive(Deserialize)]
struct StreamError<'a> {
    #[serde(rename = "type")]
    kind: Option<String>,
    #[serde(borrow)]
    code: Option<&'a RawValue>,
    message: Option<String>,
}

/// Кусок потока Chat Completions.
#[derive(Deserialize)]
struct ChatChunk<'a> {
    #[serde(default)]
    choices: Vec<ChatChoice>,
    #[serde(borrow)]
    error: Option<StreamError<'a>>,
}

#[derive(Deserialize)]
struct ChatChoice {
    delta: Option<ChatDelta>,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct ChatDelta {
    content: Option<String>,
}

/// Читает поток ответа по кускам, как они приходят по сети: строки событий
/// SSE (`data: …`) — с любыми границами кусков, и посреди буквы тоже.
#[derive(Debug)]
pub struct Reader {
    wire: Wire,
    /// Начало строки, у которой ещё нет конца.
    line: Vec<u8>,
    /// Ответ — как его пишет модель.
    text: String,
    /// Почему модель остановилась, — до конца потока.
    stop: Option<End>,
    /// Чем кончился ответ; `None` — ещё идёт.
    end: Option<End>,
}

impl Reader {
    pub fn new(provider: Provider) -> Self {
        Self {
            wire: provider.wire(),
            line: Vec::new(),
            text: String::new(),
            stop: None,
            end: None,
        }
    }

    /// Ответ, как он есть сейчас.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Чем кончился ответ; `None` — ещё идёт.
    pub fn end(&self) -> Option<&End> {
        self.end.as_ref()
    }

    /// Следующий кусок потока. После конца ответа куски не читаются.
    pub fn feed(&mut self, mut bytes: &[u8]) {
        while self.end.is_none() {
            let Some(at) = bytes.iter().position(|&byte| byte == b'\n') else {
                self.grow(bytes);
                return;
            };
            self.grow(&bytes[..at]);
            bytes = &bytes[at + 1..];
            if self.end.is_none() {
                let line = std::mem::take(&mut self.line);
                self.take(&line);
            }
        }
    }

    /// Поток кончился: последняя строка без переноса — тоже строка, а ответ
    /// без конца — оборвавшийся. Строку, оборванную посреди (остановили,
    /// связь пропала), не прочесть — её отбрасывают, как недописанное событие
    /// по спецификации SSE, а не называют ответом не того вида (ревью
    /// задачи 102).
    pub fn finish(&mut self) {
        if self.end.is_none() && !self.line.is_empty() {
            let line = std::mem::take(&mut self.line);
            self.take(&line);
            if let Some(End::Failed(Failure {
                fault: Fault::Unreadable,
                ..
            })) = self.end
            {
                self.end = None;
            }
        }
        if self.end.is_none() {
            self.end = Some(self.stop.take().unwrap_or(End::Cut));
        }
    }

    fn grow(&mut self, bytes: &[u8]) {
        if self.line.len() + bytes.len() > MAX_LINE_BYTES {
            self.end = Some(End::Failed(Failure::of(Fault::Unreadable)));
        } else {
            self.line.extend_from_slice(bytes);
        }
    }

    /// Строка потока. Событие — строка `data:` целиком: каждое событие
    /// поставщики пишут JSON в одну строку, и склейка строк `data:` по
    /// спецификации SSE им не нужна. `event:`, `id:` и пустые строки
    /// ничего не добавляют.
    fn take(&mut self, line: &[u8]) {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        let Some(data) = line.strip_prefix(b"data:") else {
            return;
        };
        let data = data.strip_prefix(b" ").unwrap_or(data);
        let Ok(data) = std::str::from_utf8(data) else {
            self.end = Some(End::Failed(Failure::of(Fault::Unreadable)));
            return;
        };
        match self.wire {
            Wire::Messages => self.message(data),
            Wire::Chat => self.chat(data),
        }
    }

    fn message(&mut self, data: &str) {
        let Ok(event) = serde_json::from_str::<MessageEvent>(data) else {
            self.end = Some(End::Failed(Failure::of(Fault::Unreadable)));
            return;
        };
        match event.kind.as_str() {
            "content_block_delta" => {
                if let Some(delta) = event.delta.filter(|delta| delta.kind.as_deref() == Some("text_delta"))
                    && let Some(text) = delta.text
                {
                    self.push(&text);
                }
            }
            "message_delta" => {
                if let Some(reason) = event.delta.and_then(|delta| delta.stop_reason) {
                    self.stop = Some(match reason.as_str() {
                        "max_tokens" | "model_context_window_exceeded" => End::Long,
                        "refusal" => End::Refused,
                        _ => End::Done,
                    });
                }
            }
            "message_stop" => self.end = Some(self.stop.take().unwrap_or(End::Done)),
            "error" => self.end = Some(End::Failed(stream_failure(event.error))),
            _ => {}
        }
    }

    fn chat(&mut self, data: &str) {
        if data.trim() == "[DONE]" {
            self.end = Some(self.stop.take().unwrap_or(End::Done));
            return;
        }
        let Ok(chunk) = serde_json::from_str::<ChatChunk>(data) else {
            self.end = Some(End::Failed(Failure::of(Fault::Unreadable)));
            return;
        };
        if let Some(error) = chunk.error {
            self.end = Some(End::Failed(stream_failure(Some(error))));
            return;
        }
        for choice in chunk.choices {
            if let Some(text) = choice.delta.and_then(|delta| delta.content) {
                self.push(&text);
            }
            if let Some(reason) = choice.finish_reason {
                self.stop = Some(match reason.as_str() {
                    "length" => End::Long,
                    "content_filter" => End::Refused,
                    _ => End::Done,
                });
            }
        }
    }

    fn push(&mut self, text: &str) {
        if self.text.len() + text.len() > MAX_ANSWER_BYTES {
            self.end = Some(End::Long);
        } else {
            self.text.push_str(text);
        }
    }
}

/// Сбой посреди потока — по его типу: у Anthropic `overloaded_error`
/// и другие (skill claude-api, `error-codes.md`), у OpenAI — `type` или
/// `code` его ошибки.
fn stream_failure(error: Option<StreamError<'_>>) -> Failure {
    let Some(error) = error else {
        return Failure::of(Fault::Unreadable);
    };
    let code = error.code.and_then(text_of).unwrap_or_default();
    let names = [error.kind.as_deref().unwrap_or_default(), code.as_str()];
    let fault = names
        .into_iter()
        .find_map(|name| match name {
            "authentication_error" | "invalid_api_key" => Some(Fault::Key),
            "permission_error" => Some(Fault::Denied),
            "not_found_error" | "model_not_found" => Some(Fault::Model),
            "rate_limit_error" | "rate_limit_exceeded" | "billing_error" | "insufficient_quota" => Some(Fault::Limit),
            "overloaded_error" | "api_error" | "server_error" => Some(Fault::Busy),
            _ => None,
        })
        .unwrap_or(Fault::Request);
    Failure {
        fault,
        status: None,
        said: error.message.as_deref().and_then(short),
    }
}

/// Ответ без разметки Markdown, которую модели пишут и вопреки наставлению:
/// окно показывает текст как есть, и звёздочки остались бы звёздочками.
/// Жирное — без `**` и `__`, код — без обратных кавычек, заголовок — без `#`,
/// пункт `* ` — пунктом `- `, ссылка `[слова](адрес)` — словами и адресом
/// в скобках: его делает ссылкой [`pieces`].
pub fn plain(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for (index, line) in text.split('\n').enumerate() {
        if index > 0 {
            out.push('\n');
        }
        let body = line.trim_start();
        let indent = &line[..line.len() - body.len()];
        let body = match body.trim_start_matches('#') {
            rest if rest.len() < body.len() && (rest.is_empty() || rest.starts_with(' ')) => rest.trim_start(),
            _ => body,
        };
        out.push_str(indent);
        let body = match body.strip_prefix("* ") {
            Some(rest) => {
                out.push_str("- ");
                rest
            }
            None => body,
        };
        out.push_str(&links(&body.replace("**", "").replace("__", "").replace('`', "")));
    }
    out
}

/// `[слова](https://…)` — «слова (https://…)».
fn links(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(open) = rest.find('[') {
        let after = &rest[open + 1..];
        let Some((label, tail)) = after.split_once("](") else {
            break;
        };
        let Some(close) = tail.find(')') else {
            break;
        };
        let url = &tail[..close];
        if label.contains(['[', ']']) || !url.starts_with("http") || url.contains(char::is_whitespace) {
            out.push_str(&rest[..=open]);
            rest = after;
            continue;
        }
        out.push_str(&rest[..open]);
        out.push_str(label);
        if label != url {
            out.push_str(" (");
            out.push_str(url);
            out.push(')');
        }
        rest = &tail[close + 1..];
    }
    out.push_str(rest);
    out
}

/// Кусок ответа: текст или ссылка, которую окно делает нажимаемой.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Piece<'a> {
    Text(&'a str),
    Link(&'a str),
}

/// Сайты, на которые ответ ведёт ссылкой: закон, ՊԵԿ, ЦБ, госуслуги
/// и сам Kvartik. Адрес другого сайта остаётся текстом: модель могла его
/// выдумать, а ссылка в программе выглядела бы рекомендацией.
const LINKED: [&str; 16] = [
    "arlis.am",
    "www.arlis.am",
    "src.am",
    "www.src.am",
    "petekamutner.am",
    "www.petekamutner.am",
    "taxservice.am",
    "www.taxservice.am",
    "file-online.taxservice.am",
    "e-invoicing.taxservice.am",
    "cba.am",
    "www.cba.am",
    "e-gov.am",
    "www.e-gov.am",
    "e-register.moj.am",
    "kvartik.com",
];

/// Ответ кусками: адрес `https://` знакомого сайта ([`LINKED`]) —
/// ссылкой, остальное — текстом. Знаки препинания в конце адреса — не адрес.
pub fn pieces(text: &str) -> Vec<Piece<'_>> {
    let mut out = Vec::new();
    let mut rest = text;
    let mut kept = 0;
    while let Some(at) = rest[kept..].find("https://").map(|at| at + kept) {
        let tail = &rest[at..];
        let end = tail
            .find(|c: char| c.is_whitespace() || matches!(c, '<' | '>' | '"' | '«' | '»' | '“' | '”' | '\''))
            .unwrap_or(tail.len());
        let url = tail[..end].trim_end_matches(['.', ',', ';', ':', '!', '?', ')', ']', '}']);
        // Сайт — до первого `/`, `?`, `#` или `\` (`\` у `https` — то же,
        // что `/`, по стандарту URL WHATWG), а с `логин:пароль@` впереди
        // браузер ведёт на сайт после `@`: `https://arlis.am:1@evil.example`
        // — это evil.example. Такой адрес — текстом (ревью задачи 102).
        let authority = url["https://".len()..].split(['/', '?', '#', '\\']).next().unwrap_or_default();
        let host = (!authority.contains('@')).then(|| authority.split(':').next().unwrap_or_default());
        if url.len() > "https://".len() && host.is_some_and(|host| LINKED.contains(&host)) {
            if at > 0 {
                out.push(Piece::Text(&rest[..at]));
            }
            out.push(Piece::Link(url));
            rest = &rest[at + url.len()..];
            kept = 0;
        } else {
            kept = at + "https://".len();
        }
    }
    if !rest.is_empty() {
        out.push(Piece::Text(rest));
    }
    out
}

/// «Мои данные» — строки для вопроса из профиля на языке окна: режим, коды
/// деятельности со строкой налога, день регистрации и гражданство. Ни ՀՎՀՀ,
/// ни имени, ни сумм: их человек впишет сам, если захочет. Строки встают
/// в поле вопроса, и до отправки он их видит и может стереть (политика
/// конфиденциальности, п. 2).
pub fn about_me(lang: Lang, profile: &ProfileData, today: Date) -> String {
    let mut lines = vec![t(lang, Key::AiMeTitle).to_owned()];
    let year = today.year();
    if profile.micro.contains(&year) {
        lines.push(fill(t(lang, Key::AiMeMicro), &[&year.to_string()]));
    } else {
        lines.push(
            match profile.regime {
                Some(Regime::Turnover) => t(lang, Key::AiMeTurnover),
                None => t(lang, Key::AiMeNoRegime),
            }
            .to_owned(),
        );
    }
    for code in &profile.activities {
        let kind = turnover::kind_of(Some(profile), code, today);
        lines.push(fill(t(lang, Key::AiMeCode), &[&code.to_string(), t(lang, turnover::kind_key(kind))]));
    }
    if let Some(day) = profile.person.registered {
        lines.push(fill(t(lang, Key::AiMeRegistered), &[&i18n::date(lang, day)]));
    }
    if let Some(residency) = profile.person.residency {
        lines.push(
            t(
                lang,
                match residency {
                    Residency::Citizen => Key::AiMeCitizen,
                    Residency::ForeignWithStatus => Key::AiMeForeignStatus,
                    Residency::ForeignWithoutStatus => Key::AiMeForeignNoStatus,
                },
            )
            .to_owned(),
        );
    }
    lines.join("\n- ")
}

pub mod frame;

#[cfg(test)]
mod tests;
