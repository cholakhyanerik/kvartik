//! ИИ-помощник (задача 102): экран внутри раздела налога. Человек подключает
//! ключ API своего ИИ — Claude, ChatGPT, Gemini или Grok, — и вопрос уходит
//! из его браузера прямо этому поставщику, а не на сервер Kvartik (решение
//! человека 2026-11-17 и 2026-11-19). Что спросить и как прочесть ответ,
//! решает ядро (`kvartik_core::ai`), сеть — `platform::ai`, ключ —
//! запись устройства (`store/device.rs`): в копию он не идёт.
//!
//! Экран — из части книги, как числа для расчёта (`ui/report.rs`): правила
//! налога, которые получает модель справкой, и коды профиля со строкой
//! налога — тот же код, что у налога, а своя часть делила бы его с налогом
//! (CLAUDE.md, Правило 6). В главном wasm — только оболочка.
//!
//! Разговор окно не хранит: «Новый разговор» и закрытое окно его стирают
//! (политика конфиденциальности, п. 2). Пока ответ пишется, начат вопрос или
//! ключ, — и пока разговор на виду, — новая версия ждёт: перезагрузка стёрла
//! бы его (`Forms::hold_while`). Ушёл человек с экрана — разговор новую
//! версию не держит: иначе один отвеченный вопрос держал бы её и выбор
//! другого ИП до закрытия вкладки (ревью задачи 102).
//!
//! Ответ виден по мере того, как его пишет модель, — без разметки Markdown
//! (`ai::plain`), а ссылкой становится только адрес сайтов закона, ՊԵԿ и ЦБ
//! (`ai::pieces`). У каждого вопроса — свой номер хода: «Остановить»
//! отцепляет ход сразу, и ответ, пришедший потом, окну уже не нужен.

use std::rc::Rc;

use kvartik_core::ai::{self, Ask, End, Failure, Fault, Piece, Provider, Reader, Setup, SetupProblem, Turn};
use kvartik_core::i18n::{self, Key, Lang, fill, t};
use leptos::prelude::*;

use super::backup::BackupChip;
use super::form::FieldError;
use super::icon::{Icon, icon};
use super::ledger::tax_part;
use super::route::{Router, Screen};
use super::split::screen;
use super::tax::{self, Part};
use super::tr;
use crate::platform::ai::{self as net, Chunk};
use crate::store::device::Device;

/// Сколько байт слов поставщика о сбое прочесть: тело ошибки — абзац JSON,
/// а накопитель ограничен (Правило 1).
const MAX_ERROR_BYTES: usize = 16 * 1024;

thread_local! {
    /// Подключённый ИИ — сигналом: его меняет экран помощника, а строка на
    /// экране налога, даже спрятанная, следит за ним.
    static SETUP: ArcRwSignal<Option<Setup>> = ArcRwSignal::new(stored());
}

/// Подключённый ИИ по записи устройства; не читается — не подключён.
fn stored() -> Option<Setup> {
    Device::new().ai().as_deref().and_then(Setup::parse)
}

/// Подключённый ИИ — отслеживаемое.
fn connected() -> Option<Setup> {
    SETUP.with(|shared| shared.get())
}

/// Подключённый ИИ — без слежения: для решения, которое принимают один раз.
fn connected_untracked() -> Option<Setup> {
    SETUP.with(|shared| shared.get_untracked())
}

/// Сигнал — к записи, только когда она другая: запись того же будила бы
/// всех, кто за ним следит (CLAUDE.md, Правило 2, «Общий сигнал»).
fn follow(next: Option<Setup>) {
    SETUP.with(|shared| {
        if shared.get_untracked() != next {
            shared.set(next);
        }
    });
}

/// Экран ИИ-помощника на языке окна: «назад» к налогу и заголовок — сразу,
/// остальное — частью книги с первого показа (`split::screen`).
#[component]
pub fn AssistantScreen(ctx: tax::Ctx, router: Router) -> impl IntoView {
    let tax::Ctx { lang, backups, .. } = ctx;
    view! {
        <a class="back" href=Screen::Tax.href()>
            {icon(Icon::Back)}
            <span>{tr(lang, Key::TaxTitle)}</span>
        </a>
        <h1 tabindex="-1">{tr(lang, Key::AiTitle)}</h1>
        <BackupChip lang backups nudge=true />
        {screen(lang, router, Screen::Assistant, move || tax_part(ctx, Part::Assistant(router)))}
    }
}

/// Строка на экране налога — к ИИ-помощнику: какой ИИ подключён или чей
/// ключ можно подключить.
#[component]
pub fn AssistantEntry(lang: ReadSignal<Lang>) -> impl IntoView {
    view! {
        <ul class="list assistant-entry">
            <li>
                <a class="entry" href=Screen::Assistant.href()>
                    <span class="entry-main">
                        <span class="entry-title">{tr(lang, Key::AiTitle)}</span>
                        <span class="entry-meta">
                            <span>{move || match connected() {
                                Some(setup) => fill(t(lang.get(), Key::AiEntryOn), &[setup.provider.name()]),
                                None => t(lang.get(), Key::AiEntryAbout).to_owned(),
                            }}</span>
                        </span>
                    </span>
                    <span class="entry-side">{icon(Icon::Forward)}</span>
                </a>
            </li>
        </ul>
    }
}

/// Вопрос и ответ на экране — с тем, чей ИИ ответил.
#[derive(Clone, PartialEq)]
struct Said {
    turn: Turn,
    by: Provider,
}

/// Идущий ответ: вопрос и чей ИИ отвечает; что он уже написал — свой сигнал,
/// чтобы кусок ответа менял один текст, а не весь разговор.
#[derive(Clone, PartialEq)]
struct Live {
    question: String,
    by: Provider,
    model: String,
}

/// Чем кончился последний вопрос, когда об этом надо сказать.
#[derive(Clone, PartialEq)]
enum Note {
    Stopped,
    Long,
    Refused,
    /// Ответ кончился, а текста в нём нет.
    Empty,
    Cut(Provider),
    Failed(Provider, String, Failure),
    TooLong,
    QuestionLong,
    /// Слова о ключе: убран, не сохранился.
    Key(Key),
}

/// Слова о том, чем кончился вопрос, — строками: главная, слова
/// поставщика, подсказка.
fn note_words(lang: Lang, note: &Note) -> Vec<String> {
    let one = |key: Key| vec![t(lang, key).to_owned()];
    match note {
        Note::Stopped => one(Key::AiStopped),
        Note::Long => one(Key::AiLong),
        Note::Refused => one(Key::AiRefused),
        Note::Empty => one(Key::AiEmpty),
        Note::Cut(by) => vec![fill(t(lang, Key::AiCut), &[by.name()])],
        Note::TooLong => one(Key::AiTooLong),
        Note::QuestionLong => vec![fill(
            t(lang, Key::AiQuestionLong),
            &[&i18n::integer(lang, ai::MAX_QUESTION_CHARS as i64)],
        )],
        Note::Key(key) => one(*key),
        Note::Failed(by, model, failure) => {
            let name = by.name();
            let mut lines = vec![match failure.fault {
                Fault::Key => fill(t(lang, Key::AiFaultKey), &[name]),
                Fault::Denied => fill(t(lang, Key::AiFaultDenied), &[name]),
                Fault::Model => fill(t(lang, Key::AiFaultModel), &[name, model]),
                Fault::Limit => fill(t(lang, Key::AiFaultLimit), &[name]),
                Fault::Busy => fill(t(lang, Key::AiFaultBusy), &[name]),
                Fault::Request => fill(t(lang, Key::AiFaultRequest), &[name]),
                Fault::Unreadable => fill(t(lang, Key::AiFaultUnreadable), &[name]),
                Fault::Network => fill(t(lang, Key::AiFaultNetwork), &[name]),
                Fault::Other => fill(
                    t(lang, Key::AiFaultOther),
                    &[name, &failure.status.map(|status| status.to_string()).unwrap_or_default()],
                ),
            }];
            if let Some(said) = &failure.said {
                lines.push(fill(t(lang, Key::AiSaid), &[name, said]));
            }
            if failure.fault == Fault::Network && *by == Provider::ChatGpt {
                lines.push(t(lang, Key::AiFaultNetworkOpenAi).to_owned());
            }
            lines
        }
    }
}

/// Ответ — без разметки и с нажимаемыми адресами знакомых сайтов.
fn answer_view(text: &str) -> AnyView {
    let plain = ai::plain(text);
    ai::pieces(&plain)
        .into_iter()
        .map(|piece| match piece {
            Piece::Text(text) => text.to_owned().into_any(),
            Piece::Link(url) => {
                let (href, words) = (url.to_owned(), url.to_owned());
                view! { <a href=href target="_blank" rel="noopener">{words}</a> }.into_any()
            }
        })
        .collect_view()
        .into_any()
}

/// Слова поставщика о сбое — не больше [`MAX_ERROR_BYTES`]: дальше тело не
/// читается, а поток закрывается.
async fn error_text(body: &net::Body) -> String {
    let mut bytes = Vec::new();
    while let Chunk::Bytes(chunk) = body.next().await {
        bytes.extend_from_slice(&chunk);
        if bytes.len() > MAX_ERROR_BYTES {
            body.cancel();
            bytes.truncate(MAX_ERROR_BYTES);
            break;
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Спросить и прочесть ответ потоком — ходом `id`: пока он текущий (`run`),
/// `writing` растёт по мере ответа, а тело ответа лежит в `body` для
/// «Остановить». Вернёт ответ и чем он кончился; `None` — ход отцепили
/// («Остановить»), и его итог окну не нужен.
async fn stream(
    provider: Provider,
    request: ai::Request,
    writing: RwSignal<String>,
    body: StoredValue<Option<Rc<net::Body>>, LocalStorage>,
    run: StoredValue<u32>,
    id: u32,
) -> Option<(String, End)> {
    let alive = move || run.get_value() == id;
    let Some(opened) = net::open(request.url, &request.headers, &request.body).await else {
        return alive().then(|| (String::new(), End::Failed(Failure::of(Fault::Network))));
    };
    let opened = Rc::new(opened);
    if !alive() {
        opened.cancel();
        return None;
    }
    if !(200..300).contains(&opened.status()) {
        let text = error_text(&opened).await;
        return alive().then(|| (String::new(), End::Failed(ai::failure(opened.status(), &text))));
    }
    body.set_value(Some(Rc::clone(&opened)));
    let mut reader = Reader::new(provider);
    let mut open = true;
    while alive() {
        match opened.next().await {
            Chunk::Bytes(bytes) => {
                if !alive() {
                    break;
                }
                reader.feed(&bytes);
                writing.set(ai::plain(reader.text()));
                if reader.end().is_some() {
                    break;
                }
            }
            Chunk::End | Chunk::Broken => {
                open = false;
                break;
            }
        }
    }
    // Отцепленный ход поток уже закрыл: «Остановить» сделал это сам.
    if !alive() {
        return None;
    }
    body.set_value(None);
    // Ответ кончился раньше потока — закрыть его: поставщик перестанет
    // писать (и брать плату за) ненужное.
    if open {
        opened.cancel();
    }
    reader.finish();
    let end = reader.end().cloned().unwrap_or(End::Cut);
    Some((reader.text().to_owned(), end))
}

/// Содержимое экрана — из части книги: подключение ИИ, разговор и вопрос.
#[component]
pub fn AssistantPage(ctx: tax::Ctx, router: Router) -> impl IntoView {
    let lang = ctx.lang;
    // Запись устройства могли сменить в другой вкладке: экран начинает с неё
    // и сверяется с ней, когда к окну возвращаются.
    follow(stored());
    let focus = window_event_listener(leptos::ev::focus, move |_| follow(stored()));
    on_cleanup(move || focus.remove());
    let start = connected_untracked();
    // Форма подключения: открыта, пока ключа нет, или по «Сменить ключ».
    let editing = RwSignal::new(start.is_none());
    let provider = RwSignal::new(start.as_ref().map_or(Provider::Claude, |setup| setup.provider));
    let model = RwSignal::new(start.as_ref().map_or_else(|| Provider::Claude.model().to_owned(), |setup| setup.model.clone()));
    let key = RwSignal::new(String::new());
    let tried = RwSignal::new(false);
    // Разговор.
    let talk = RwSignal::new(Vec::<Said>::new());
    let question = RwSignal::new(String::new());
    let live = RwSignal::new(None::<Live>);
    let writing = RwSignal::new(String::new());
    let note = RwSignal::new(None::<Note>);
    let body = StoredValue::new_local(None::<Rc<net::Body>>);
    let run = StoredValue::new(0_u32);
    let streaming = Memo::new(move |_| live.with(Option::is_some));
    let shown = Memo::new(move |_| streaming.get() || talk.with(|talk| !talk.is_empty()));
    // Начатое здесь ждёт новая версия: перезагрузка стёрла бы идущий ответ,
    // вопрос, вставленный ключ и разговор на виду.
    ctx.forms.hold_while(move || {
        streaming.get()
            || question.with(|question| !question.trim().is_empty())
            || (editing.get() && key.with(|key| !key.is_empty()))
            || (router.shows(Screen::Assistant) && talk.with(|talk| !talk.is_empty()))
    });

    // Итог вопроса: ответ — в разговор, без ответа вопрос — обратно в поле,
    // и слова о том, чем кончилось.
    let settle = move |asked: Live, answer: String, end: End, stopped: bool| {
        let empty = answer.trim().is_empty();
        let said = if stopped {
            Some(Note::Stopped)
        } else {
            match end {
                End::Done if empty => Some(Note::Empty),
                End::Done => None,
                End::Long => Some(Note::Long),
                End::Refused => Some(Note::Refused),
                End::Cut => Some(Note::Cut(asked.by)),
                End::Failed(failure) => Some(Note::Failed(asked.by, asked.model.clone(), failure)),
            }
        };
        // Отказ посреди ответа — недописанное не ответ (skill claude-api,
        // «refusal»): его нет, вопрос — в поле, чтобы спросить иначе.
        if empty || said == Some(Note::Refused) {
            if question.with_untracked(|question| question.trim().is_empty()) {
                question.set(asked.question);
            }
        } else {
            talk.update(|talk| {
                talk.push(Said {
                    turn: Turn {
                        question: asked.question,
                        answer,
                    },
                    by: asked.by,
                });
            });
        }
        note.set(said);
    };
    // «Остановить» — сразу, и до прихода ответа: ход отцепляется, поток
    // закрывается, а недописанное остаётся в разговоре (ревью задачи 102).
    let stop = move || {
        let Some(asked) = live.get_untracked() else {
            return;
        };
        run.set_value(run.get_value().wrapping_add(1));
        body.update_value(|body| {
            if let Some(body) = body.take() {
                body.cancel();
            }
        });
        live.set(None);
        settle(asked, writing.get_untracked(), End::Cut, true);
    };
    let problem = move || Setup::new(provider.get(), &key.get(), &model.get()).err();
    let choose = move |next: Provider| {
        let was = provider.get_untracked();
        if was != next {
            provider.set(next);
            // Модель по умолчанию прежнего — модель по умолчанию нового;
            // вписанную человеком окно не трогает.
            if model.with_untracked(|model| model.trim().is_empty() || model == was.model()) {
                model.set(next.model().to_owned());
            }
        }
    };
    let save = move || {
        tried.set(true);
        let Ok(setup) = Setup::new(provider.get_untracked(), &key.get_untracked(), &model.get_untracked()) else {
            return;
        };
        if Device::new().set_ai(&setup.to_text()) {
            follow(Some(setup));
            key.set(String::new());
            tried.set(false);
            editing.set(false);
            note.set(None);
        } else {
            note.set(Some(Note::Key(Key::AiNotSaved)));
        }
    };
    let change = move || {
        if let Some(setup) = connected_untracked() {
            provider.set(setup.provider);
            model.set(setup.model);
        }
        key.set(String::new());
        tried.set(false);
        editing.set(true);
    };
    let remove = move || {
        if Device::new().clear_ai() {
            follow(None);
            editing.set(true);
            note.set(Some(Note::Key(Key::AiRemoved)));
        }
    };
    let ask = move || {
        if live.with_untracked(Option::is_some) {
            return;
        }
        // Ключ — по записи устройства сейчас: его могли убрать или сменить
        // в другой вкладке.
        let now = stored();
        follow(now.clone());
        let Some(setup) = now else {
            editing.set(true);
            return;
        };
        let text = question.get_untracked().trim().to_owned();
        if text.is_empty() {
            return;
        }
        if text.chars().count() > ai::MAX_QUESTION_CHARS {
            note.set(Some(Note::QuestionLong));
            return;
        }
        if talk.with_untracked(Vec::len) >= ai::MAX_TURNS {
            note.set(Some(Note::TooLong));
            return;
        }
        let turns: Vec<Turn> = talk.with_untracked(|talk| talk.iter().map(|said| said.turn.clone()).collect());
        let request = ai::request(&Ask {
            setup: &setup,
            lang: lang.get_untracked(),
            today: ctx.today,
            turns: &turns,
            question: &text,
        });
        let id = run.get_value().wrapping_add(1);
        run.set_value(id);
        let asked = Live {
            question: text,
            by: setup.provider,
            model: setup.model,
        };
        question.set(String::new());
        note.set(None);
        writing.set(String::new());
        live.set(Some(asked.clone()));
        leptos::task::spawn_local(async move {
            let Some((answer, end)) = stream(asked.by, request, writing, body, run, id).await else {
                return;
            };
            live.set(None);
            settle(asked, answer, end, false);
        });
    };
    let add_me = move || {
        let profile = ctx.sources.profiles.profile();
        let (Some(profile), Some(today)) = (profile, ctx.today) else {
            return;
        };
        let words = ai::about_me(lang.get_untracked(), profile.data(), today);
        question.update(|question| {
            if !question.trim().is_empty() {
                question.push_str("\n\n");
            }
            question.push_str(&words);
        });
    };
    let can_add_me = move || ctx.sources.profiles.profile().is_some() && ctx.today.is_some();

    view! {
        <p>{tr(lang, Key::AiAbout)}</p>
        {move || if editing.get() {
            view! {
                <form class="ai-setup" on:submit=move |event| {
                    event.prevent_default();
                    save();
                }>
                    <h2>{tr(lang, Key::AiConnect)}</h2>
                    <div class="seg" role="group" aria-label=move || t(lang.get(), Key::AiProvider)>
                        {Provider::ALL
                            .into_iter()
                            .map(|choice| view! {
                                <button
                                    type="button"
                                    aria-pressed=move || (provider.get() == choice).to_string()
                                    on:click=move |_| choose(choice)
                                >
                                    {choice.name()}
                                </button>
                            })
                            .collect_view()}
                    </div>
                    // Ключ — не пароль: поле текстом. Полю пароля браузер
                    // подставил бы сохранённый пароль копий, и он ушёл бы
                    // поставщику, а ключ запомнил бы паролем сайта (ревью
                    // задачи 102).
                    <label>
                        {tr(lang, Key::AiKey)}
                        <input
                            type="text"
                            autocomplete="off"
                            autocapitalize="off"
                            spellcheck="false"
                            bind:value=key
                        />
                    </label>
                    <FieldError lang tried error=move || problem().filter(|problem| *problem != SetupProblem::Model).map(SetupProblem::key) />
                    {move || {
                        let chosen = provider.get();
                        let typed = key.get();
                        if typed.trim().is_empty() {
                            return None;
                        }
                        Some(match Provider::guess(&typed) {
                            Some(guess) if guess != chosen => view! {
                                <p class="hint">{move || fill(t(lang.get(), Key::AiKeyOther), &[guess.name(), chosen.name()])}</p>
                            }
                            .into_any(),
                            Some(_) => ().into_any(),
                            None => view! { <p class="hint">{tr(lang, Key::AiKeyUnknown)}</p> }.into_any(),
                        })
                    }}
                    <p class="hint">
                        <a href=move || provider.get().key_page() target="_blank" rel="noopener">
                            {move || fill(t(lang.get(), Key::AiKeyWhere), &[provider.get().company()])}
                        </a>
                    </p>
                    <p class="hint">{move || fill(t(lang.get(), Key::AiKeyStays), &[provider.get().company()])}</p>
                    <p class="hint">{tr(lang, Key::AiKeyLimit)}</p>
                    <label>
                        {tr(lang, Key::AiModel)}
                        <input type="text" autocomplete="off" autocapitalize="off" spellcheck="false" bind:value=model />
                    </label>
                    <p class="hint">{tr(lang, Key::AiModelHint)}</p>
                    <FieldError lang tried error=move || problem().filter(|problem| *problem == SetupProblem::Model).map(SetupProblem::key) />
                    <div class="actions">
                        <button type="submit"><span class="label">{tr(lang, Key::AiSave)}</span></button>
                        {move || connected().map(|_| view! {
                            <button type="button" class="secondary" on:click=move |_| editing.set(false)>
                                <span class="label">{tr(lang, Key::Cancel)}</span>
                            </button>
                        })}
                    </div>
                </form>
            }
            .into_any()
        } else {
            view! {
                {move || connected().map(|setup| view! {
                    <div class="ai-key">
                        <p class="hint">{move || fill(
                            t(lang.get(), Key::AiConnected),
                            &[setup.provider.name(), &setup.model, setup.tail()],
                        )}</p>
                        // Пока ответ пишется, ключ не меняют: сначала
                        // «Остановить» (ревью задачи 102).
                        <div class="actions">
                            <button type="button" class="secondary" disabled=move || streaming.get() on:click=move |_| change()>
                                <span class="label">{tr(lang, Key::AiChange)}</span>
                            </button>
                            <button type="button" class="secondary" disabled=move || streaming.get() on:click=move |_| remove()>
                                <span class="label">{tr(lang, Key::AiRemove)}</span>
                            </button>
                        </div>
                    </div>
                })}
            }
            .into_any()
        }}
        {move || shown.get().then(|| view! {
            <ol class="ai-talk">
                {move || talk.with(|talk| talk.iter().map(|said| {
                    let question = said.turn.question.clone();
                    let by = said.by.name();
                    let answer = answer_view(&said.turn.answer);
                    view! {
                        <li>
                            <p class="ai-who">{tr(lang, Key::AiYou)}</p>
                            <p class="ai-q">{question}</p>
                            <p class="ai-who">{by}</p>
                            // Абзацем, а не `div`: ссылка в абзаце — цветом
                            // ссылок программы (`main p a` в `base.css`), а не
                            // синим браузера (глазной прогон задачи 102).
                            <p class="ai-a">{answer}</p>
                        </li>
                    }
                }).collect_view())}
                // Идущий ответ — своим пунктом: кусок ответа меняет только
                // его текст, а не весь разговор (ревью задачи 102).
                {move || streaming.get().then(|| {
                    let (asked, by) = live.with_untracked(|live| {
                        live.as_ref().map(|live| (live.question.clone(), live.by.name())).unwrap_or_default()
                    });
                    view! {
                        <li>
                            <p class="ai-who">{tr(lang, Key::AiYou)}</p>
                            <p class="ai-q">{asked}</p>
                            <p class="ai-who">{by}</p>
                            <p class="ai-a">{move || writing.get()}</p>
                            <p role="status" class="hint">{tr(lang, Key::AiWriting)}</p>
                        </li>
                    }
                })}
            </ol>
        })}
        {move || note.get().map(|note| {
            let lang = lang.get();
            let alert = !matches!(note, Note::Stopped | Note::Key(Key::AiRemoved));
            let lines = note_words(lang, &note)
                .into_iter()
                .enumerate()
                .map(|(index, line)| match (index, alert) {
                    (0, true) => view! { <p role="alert">{line}</p> }.into_any(),
                    (0, false) => view! { <p role="status">{line}</p> }.into_any(),
                    _ => view! { <p class="hint">{line}</p> }.into_any(),
                })
                .collect_view();
            view! { <div class="ai-note">{lines}</div> }
        })}
        // «Новый разговор» — под разговором, а не в форме вопроса: и с убранным
        // ключом разговор можно стереть (ревью задачи 102).
        {move || (!streaming.get() && talk.with(|talk| !talk.is_empty())).then(|| view! {
            <div class="actions ai-new">
                <button
                    type="button"
                    class="secondary"
                    on:click=move |_| {
                        talk.set(Vec::new());
                        note.set(None);
                    }
                >
                    <span class="label">{tr(lang, Key::AiNew)}</span>
                </button>
            </div>
        })}
        {move || (!editing.get()).then(connected).flatten().map(|setup| view! {
            <form class="ai-ask" on:submit=move |event| {
                event.prevent_default();
                ask();
            }>
                <label>
                    {tr(lang, Key::AiQuestion)}
                    <textarea rows="4" bind:value=question maxlength=ai::MAX_QUESTION_CHARS.to_string()></textarea>
                </label>
                <p class="hint">{move || fill(t(lang.get(), Key::AiWhatGoes), &[setup.provider.name()])}</p>
                <div class="actions">
                    // Одна кнопка — «Спросить», а пока ответ пишется,
                    // «Остановить»: смена кнопки сбрасывала бы фокус клавиатуры
                    // на страницу (ревью задачи 102).
                    <button
                        type="submit"
                        disabled=move || !streaming.get() && question.with(|question| question.trim().is_empty())
                        on:click=move |event| {
                            if streaming.get_untracked() {
                                event.prevent_default();
                                stop();
                            }
                        }
                    >
                        <span class="label">{move || t(lang.get(), if streaming.get() { Key::AiStop } else { Key::AiAsk })}</span>
                    </button>
                    {move || can_add_me().then(|| view! {
                        <button type="button" class="secondary" on:click=move |_| add_me()>
                            <span class="label">{tr(lang, Key::AiAddMe)}</span>
                        </button>
                    })}
                </div>
                {move || can_add_me().then(|| view! { <p class="hint">{tr(lang, Key::AiAddMeHint)}</p> })}
            </form>
        })}
    }
}
