//! Экран ИИ-помощника в рамке (задача 111; до 0.0.160 — экран окна, задача
//! 102). Человек подключает ключ API своего ИИ — Claude, ChatGPT, Gemini или
//! Grok, — и вопрос уходит из его браузера прямо этому поставщику (решение
//! человека 2026-11-17 и 2026-11-19). Что спросить и как прочесть ответ,
//! решает ядро (`kvartik_core::ai`), сеть — `net`, ключ — запись рамки
//! (`store`): окно программы её не видит, и в копию она не идёт.
//!
//! Разговор рамка не хранит: «Новый разговор» и закрытое окно его стирают
//! (политика конфиденциальности, п. 2). Что начато здесь — идёт ответ, вписан
//! вопрос или ключ, есть разговор, — рамка говорит окну (`FromFrame::Busy`),
//! и новая версия окна ждёт: перезагрузка окна стёрла бы и рамку.
//!
//! Ответ виден по мере того, как его пишет модель, — без разметки Markdown
//! (`ai::plain`), а ссылкой становится только адрес сайтов закона, ՊԵԿ и ЦБ
//! (`ai::pieces`). У каждого вопроса — свой номер хода: «Остановить»
//! отцепляет ход сразу, и ответ, пришедший потом, рамке уже не нужен.

use std::rc::Rc;

use kvartik_core::ai::frame::FromFrame;
use kvartik_core::ai::{self, Ask, End, Failure, Fault, Piece, Provider, Reader, Setup, SetupProblem, Turn};
use kvartik_core::i18n::{self, Key, Lang, fill, t};
use leptos::prelude::*;

use crate::bridge::{self, Inbox};
use crate::net::{self, Chunk};
use crate::store;

/// Сколько байт слов поставщика о сбое прочесть: тело ошибки — абзац JSON,
/// а накопитель ограничен (Правило 1).
const MAX_ERROR_BYTES: usize = 16 * 1024;

thread_local! {
    /// Подключённый ИИ — сигналом: его меняют экран и ключ, пришедший от
    /// окна (`bridge`).
    static SETUP: ArcRwSignal<Option<Setup>> = ArcRwSignal::new(store::stored());
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
pub fn follow(next: Option<Setup>) {
    SETUP.with(|shared| {
        if shared.get_untracked() != next {
            shared.set(next);
        }
    });
}

/// Строка на языке рамки — производное от сигнала языка (Правило 4).
fn tr(lang: RwSignal<Lang>, key: Key) -> impl Fn() -> &'static str + Copy + Send + 'static {
    move || t(lang.get(), key)
}

/// Ошибка поля — словами, после первой попытки сохранить (как у форм окна).
#[component]
fn FieldError<F>(lang: RwSignal<Lang>, tried: RwSignal<bool>, error: F) -> impl IntoView
where
    F: Fn() -> Option<Key> + Send + Sync + 'static,
{
    move || {
        tried
            .get()
            .then(&error)
            .flatten()
            .map(|key| view! { <p class="field-error" role="alert">{tr(lang, key)}</p> })
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

/// Экран рамки: подключение ИИ, разговор и вопрос.
#[component]
pub fn Page(inbox: Inbox) -> impl IntoView {
    let lang = inbox.lang;
    // Запись рамки могли сменить в другой вкладке: экран начинает с неё
    // и сверяется с ней, когда к рамке возвращаются.
    follow(store::stored());
    let focus = window_event_listener(leptos::ev::focus, move |_| follow(store::stored()));
    on_cleanup(move || focus.remove());
    let start = connected_untracked();
    // Форма подключения: открыта, пока ключа нет, или по «Сменить ключ».
    let editing = RwSignal::new(start.is_none());
    let provider = RwSignal::new(start.as_ref().map_or(Provider::Claude, |setup| setup.provider));
    let model = RwSignal::new(start.as_ref().map_or_else(|| Provider::Claude.model().to_owned(), |setup| setup.model.clone()));
    let key = RwSignal::new(String::new());
    let tried = RwSignal::new(false);
    // Ключ пришёл не из этой формы — от окна (ключ 0.0.158) или из другой
    // вкладки, — а в форму ничего не вписано: она закрывается, как после
    // «Сохранить» (найдено браузерным тестом переезда, задача 111). Прежнее —
    // с создания экрана: эффект впервые идёт следующей задачей, и ключ,
    // пришедший до неё, он иначе принял бы за бывший всегда.
    let had = start.is_some();
    Effect::new(move |was: Option<bool>| {
        let now = connected().is_some();
        if now && !was.unwrap_or(had) && key.with_untracked(String::is_empty) {
            editing.set(false);
        }
        now
    });
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
    // Начатое здесь — окну: новая версия окна ждёт, пока идёт ответ, вписан
    // вопрос или ключ, а разговор держит её, пока экран помощника на виду.
    let busy = Memo::new(move |_| {
        streaming.get()
            || question.with(|question| !question.trim().is_empty())
            || (editing.get() && key.with(|key| !key.is_empty()))
    });
    let talking = Memo::new(move |_| talk.with(|talk| !talk.is_empty()));
    Effect::new(move |_| {
        bridge::post(&FromFrame::Busy {
            busy: busy.get(),
            talk: talking.get(),
        });
    });
    // Какой ИИ подключён — окну, для строки на экране налога.
    Effect::new(move |_| {
        let provider = connected().map(|setup| setup.provider.code().to_owned());
        bridge::post(&FromFrame::Connected { provider });
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
        if store::save(&setup) {
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
        if store::clear() {
            follow(None);
            editing.set(true);
            note.set(Some(Note::Key(Key::AiRemoved)));
        }
    };
    let ask = move || {
        if live.with_untracked(Option::is_some) {
            return;
        }
        // Ключ — по записи рамки сейчас: его могли убрать или сменить
        // в другой вкладке.
        let now = store::stored();
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
            today: inbox.today.get_untracked(),
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
    // «Мои данные» знает окно — у него профиль: рамка просит их нажатием
    // и вписывает в поле, когда они пришли, — до отправки их видно.
    let add_me = move || bridge::post(&FromFrame::WantMe);
    Effect::new(move |_| {
        let Some(words) = inbox.incoming.get() else {
            return;
        };
        inbox.incoming.set(None);
        question.update(|question| {
            if !question.trim().is_empty() {
                question.push_str("\n\n");
            }
            question.push_str(&words);
        });
    });
    let can_add_me = move || inbox.me.get();

    view! {
        <main id="ask" class="ai-page">
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
        </main>
    }
}
