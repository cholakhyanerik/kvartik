//! Экран «Другое устройство» (задача 110): изменения с одного устройства
//! человека на другое без файла и без сервера (решение человека
//! 2026-11-21) — QR-кодами, которые снимает камера, или текстом, который
//! пересылают себе мессенджером.
//!
//! Что в коде и как он устроен — ядро (`kvartik_core::format::sync`); из
//! базы и обратно — хранилище (`store::sync`), ложится код тем же путём, что
//! копия (`store::backup::restore`): одной транзакцией, со сверкой ИП, тем же
//! слиянием. Пароль — пароль копий: код идёт через мессенджер, а QR-код
//! снимет любая камера рядом (CLAUDE.md, Правило 13).
//!
//! Экран — своей частью wasm с первого показа (`split::screen`), как экран
//! копии: первому заходу он не нужен (Правило 1). Камера и сканер кадров —
//! ещё одной частью ([`scanner_part`]): нужны только по «Снять QR-код».
//!
//! QR-коды идут по кругу, пока код на экране и экран открыт: это работа,
//! которую человек сам начал, а не покой (Правило 1), — ушёл с экрана или
//! нажал «Готово» — круг стоит. Камера горит, только пока сканер на экране:
//! уход с экрана её выключает.

use std::fmt::Write as _;
use std::rc::Rc;
use std::time::Duration;

use kvartik_core::format::Stored;
use kvartik_core::format::backup::{self, DeviceKey, KDF_ITERATIONS, KINDS, Owner};
use kvartik_core::format::office as office_route;
use kvartik_core::format::qr::{self, Gray};
use kvartik_core::format::sync::{self, CodeError, Collector, Continuity, FrameError, MAX_CODE_BYTES, Sealed, Took};
use kvartik_core::i18n::{self, Key, Lang, fill, t};
use kvartik_core::model::date::Date;
use kvartik_core::model::profile::Profile;
use leptos::ev::{MouseEvent, SubmitEvent};
use leptos::prelude::*;

use super::backup::{Backups, IssuerAsk, Problem, counted, key_for, who};
use super::form::Forms;
use super::icon::{Icon, icon};
use super::office::Office;
use super::route::{Router, Screen};
use super::split::{screen, split};
use super::storage::Storage;
use super::{Base, tr};
use crate::platform::camera::{Camera, CameraError};
use crate::platform::crypto::{self, CryptoError};
use crate::platform::{clipboard, clock};
use crate::store::backup::{self as stored, Merged, OtherIe, Restored};
use crate::store::device::Device;
use crate::store::office::list as office_list;
use crate::store::StoreError;
use crate::store::sync::{self as stored_sync, Sealing, Unmade, Unread};

/// Что нужно экрану из окна: через границу части — одним значением.
#[derive(Clone, Copy)]
pub struct Ctx {
    pub lang: ReadSignal<Lang>,
    pub base: Base,
    pub backups: Backups,
    pub office: Office,
    pub forms: Forms,
    pub storage: RwSignal<Storage>,
    pub router: Router,
}

/// Строка «Другое устройство» в группе «Программа» на экране профиля.
#[component]
pub fn SyncEntry(lang: ReadSignal<Lang>) -> impl IntoView {
    view! {
        <a class="entry" href=Screen::Sync.href()>
            <span class="entry-main">
                <span class="entry-title">{tr(lang, Key::SyncTitle)}</span>
                <span class="entry-meta">{tr(lang, Key::SyncEntryAbout)}</span>
            </span>
            {icon(Icon::Forward)}
        </a>
    }
}

/// Экран: «назад» к профилю, заголовок и метка копии — сразу, остальное —
/// частью с первого показа.
#[component]
pub fn SyncScreen(ctx: Ctx) -> impl IntoView {
    let lang = ctx.lang;
    view! {
        <a class="back" href=Screen::Profile.href()>
            {icon(Icon::Back)}
            <span>{tr(lang, Key::NavProfile)}</span>
        </a>
        <h1 tabindex="-1">{tr(lang, Key::SyncTitle)}</h1>
        <super::backup::BackupChip lang backups=ctx.backups nudge=true />
        {screen(lang, ctx.router, Screen::Sync, move || sync_part(ctx))}
    }
}

split! {
    /// Экран «Другое устройство» — своей частью wasm (`ui/split.rs`).
    pub(super) fn sync_part(ctx: Ctx) -> AnyView, preload preload_sync {
        view! { <SyncBody ctx /> }.into_any()
    }
}

/// Содержимое экрана — пока хранилище открыто: без базы передавать нечего
/// и принимать некуда.
#[component]
fn SyncBody(ctx: Ctx) -> impl IntoView {
    let lang = ctx.lang;
    let ready = move || matches!(ctx.storage.get(), Storage::Ready(_));
    view! {
        <Show when=ready>
            <p>{tr(lang, Key::SyncAbout)}</p>
            <p class="hint">
                {tr(lang, Key::SyncIssueNote)}" "
                <a href=Screen::Settings.href()>{tr(lang, Key::SettingsTitle)}</a>
            </p>
            <h2>{tr(lang, Key::SyncSendTitle)}</h2>
            <Send ctx />
            <h2>{tr(lang, Key::SyncReceiveTitle)}</h2>
            <Receive ctx />
        </Show>
    }
}

/// Что сказать человеку на этом экране.
#[derive(Clone)]
enum Say {
    /// Слова экрана копии: пароль, шифр, хранилище.
    Backup(Problem),
    Key(Key),
    /// Код больше предела: сколько КБ вышло бы.
    TooLarge(usize),
    /// Здесь пусто, а в коде — изменения с дня.
    EmptyDelta(Option<Date>),
    /// Код ИП из списка, а открыт другой: его имя.
    Elsewhere(String),
    /// Код другого ИП: оба — ИП программы (`None` — профиль не читается)
    /// и ИП кода.
    OtherIe(Box<(Option<Owner>, Owner)>),
}

impl Say {
    fn text(&self, lang: Lang) -> String {
        match self {
            Self::Backup(problem) => problem.text(lang),
            Self::Key(key) => t(lang, *key).to_owned(),
            Self::TooLarge(bytes) => fill(
                t(lang, Key::SyncTooLarge),
                &[
                    &i18n::integer(lang, bytes.div_ceil(1024) as i64),
                    &i18n::integer(lang, (MAX_CODE_BYTES / 1024) as i64),
                ],
            ),
            Self::EmptyDelta(day) => fill(
                t(lang, Key::SyncEmptyDelta),
                &[&day.map(|day| i18n::date(lang, day)).unwrap_or_default()],
            ),
            Self::Elsewhere(name) => fill(t(lang, Key::SyncElsewhere), &[name]),
            Self::OtherIe(both) => {
                let (ours, theirs) = &**both;
                let ours = ours
                    .as_ref()
                    .map_or_else(|| t(lang, Key::BackupOtherIeUnreadable).to_owned(), |ours| who(lang, ours));
                format!(
                    "{} {} {}",
                    t(lang, Key::SyncOtherIe),
                    fill(t(lang, Key::BackupOtherIeOurs), &[&ours]),
                    fill(t(lang, Key::SyncOtherIeTheirs), &[&who(lang, theirs)]),
                )
            }
        }
    }
}

impl From<Problem> for Say {
    fn from(problem: Problem) -> Self {
        Self::Backup(problem)
    }
}

/// Готовый код на экране.
struct Shown {
    /// Контуры тёмных модулей каждого QR-кода — для `<path d>`.
    qr: Vec<String>,
    /// Сторона QR-кода с полем, в модулях.
    side: usize,
    /// Куски текста.
    parts: Vec<String>,
    /// С какого дня изменения; `None` — все записи.
    since: Option<Date>,
    changed: usize,
    skipped: usize,
    unreadable: usize,
}

/// Где передача.
#[derive(Clone)]
enum Sending {
    Idle,
    Busy,
    Ready(Rc<Shown>),
    Failed(Say),
}

/// Поле светлых модулей вокруг QR-кода: четыре, как велит стандарт
/// (ISO/IEC 18004, п. 6.3.8), — без него сканер не найдёт край кода.
const QUIET: usize = 4;

/// Сколько показывается каждый QR-код круга: камера с разбором успевает
/// снять его дважды-трижды, а круг из 15 кодов — меньше восьми секунд.
const QR_STEP: Duration = Duration::from_millis(500);

/// Контур тёмных модулей QR-кода — полосами по строкам, со сдвигом на поле.
fn qr_path(code: &qr::Code) -> String {
    let side = code.side();
    let mut path = String::new();
    for y in 0..side {
        let mut x = 0;
        while x < side {
            if !code.dark(x, y) {
                x += 1;
                continue;
            }
            let start = x;
            while x < side && code.dark(x, y) {
                x += 1;
            }
            let run = x - start;
            let _ = write!(path, "M{} {}h{run}v1h-{run}z", start + QUIET, y + QUIET);
        }
    }
    path
}

/// День по умолчанию для поля «Изменения с этого дня» (`sync::default_day`).
fn default_day(base: &str) -> Option<Date> {
    let device = Device::new();
    let today = clock::today()?;
    sync::default_day(
        device.sync_made(base).and_then(clock::local_date),
        device.sync_arrived(base).and_then(sync::day_covering),
        today,
    )
}

/// Передать отсюда: что, пароль копий — и код на экране.
#[component]
fn Send(ctx: Ctx) -> impl IntoView {
    let Ctx {
        lang,
        backups,
        office,
        forms,
        router,
        ..
    } = ctx;
    let first = default_day(&office.open_name());
    let all = RwSignal::new(first.is_none());
    let day = RwSignal::new(first.or_else(clock::today).map(Date::iso).unwrap_or_default());
    let password = RwSignal::new(String::new());
    let repeat = RwSignal::new(String::new());
    let sending = RwSignal::new_local(Sending::Idle);
    let new_password = move || !backups.key_set();
    let busy = move || matches!(sending.get(), Sending::Busy);
    forms.hold_while(move || {
        password.with(|text| !text.is_empty())
            || repeat.with(|text| !text.is_empty())
            || sending.with(|state| match state {
                Sending::Busy => true,
                Sending::Ready(_) => router.shows(Screen::Sync),
                Sending::Idle | Sending::Failed(_) => false,
            })
    });

    let submit = move |event: SubmitEvent| {
        event.prevent_default();
        if busy() {
            return;
        }
        let since = if all.get_untracked() {
            None
        } else {
            match Date::from_iso(&day.get_untracked()) {
                Some(day) => Some(day),
                None => {
                    sending.set(Sending::Failed(Say::Key(Key::SyncDayMissing)));
                    return;
                }
            }
        };
        let fresh = new_password();
        let (typed, again) = (password.get_untracked(), repeat.get_untracked());
        sending.set(Sending::Busy);
        leptos::task::spawn_local(async move {
            let outcome = make(ctx, &typed, fresh.then_some(again.as_str()), since).await;
            if outcome.is_ok() {
                password.set(String::new());
                repeat.set(String::new());
            }
            sending.set(match outcome {
                Ok(shown) => Sending::Ready(Rc::new(shown)),
                Err(say) => Sending::Failed(say),
            });
        });
    };

    view! {
        <form class="sync" hidden=move || matches!(sending.get(), Sending::Ready(_)) on:submit=submit>
            <fieldset class="group">
                <legend>{tr(lang, Key::SyncWhat)}</legend>
                <label class="choice">
                    <input type="radio" name="sync-what" prop:checked=move || !all.get() on:change=move |_| all.set(false) />
                    {tr(lang, Key::SyncSince)}
                </label>
                <label class="choice">
                    <input type="radio" name="sync-what" prop:checked=move || all.get() on:change=move |_| all.set(true) />
                    {tr(lang, Key::SyncAll)}
                </label>
            </fieldset>
            <label hidden=move || all.get()>
                {tr(lang, Key::SyncSince)}
                <input type="date" max=clock::today().map(Date::iso) bind:value=day />
            </label>
            <p class="hint" hidden=move || all.get()>{tr(lang, Key::SyncSinceHint)}</p>
            {move || if new_password() {
                view! {
                    <label>
                        {tr(lang, Key::BackupNewPassword)}
                        <input type="password" autocomplete="new-password" bind:value=password />
                    </label>
                    <label>
                        {tr(lang, Key::BackupRepeatPassword)}
                        <input type="password" autocomplete="new-password" bind:value=repeat />
                    </label>
                    <p class="hint">
                        {counted(lang, Key::BackupPasswordHint, backup::MIN_PASSWORD_CHARS as i64)}
                    </p>
                }
                .into_any()
            } else {
                view! {
                    <label>
                        {tr(lang, Key::BackupPassword)}
                        <input type="password" autocomplete="current-password" bind:value=password />
                    </label>
                }
                .into_any()
            }}
            <button type="submit" class="wide" disabled=busy>
                <span class="label">{tr(lang, Key::SyncMake)}</span>
            </button>
        </form>
        {move || match sending.get() {
            Sending::Idle => None,
            Sending::Busy => Some(view! { <p role="status">{tr(lang, Key::SyncMaking)}</p> }.into_any()),
            // «Передавать нечего» — не беда, а ответ: спокойными словами, без
            // цвета тревоги.
            Sending::Failed(say @ Say::Key(Key::SyncNothing)) => {
                Some(view! { <p role="status">{move || say.text(lang.get())}</p> }.into_any())
            }
            Sending::Failed(say) => Some(view! { <p role="alert">{move || say.text(lang.get())}</p> }.into_any()),
            // Код убран — день по умолчанию уже другой: день этого кода.
            Sending::Ready(shown) => Some(view! {
                <ShownCode lang router shown close=move || {
                    sending.set(Sending::Idle);
                    if let Some(next) = default_day(&office.open_name()) {
                        day.set(next.iso());
                        all.set(false);
                    }
                } />
            }.into_any()),
        }}
    }
}

/// Собрать код: пароль копий — сверить с устройством или задать, записи
/// с дня `since` — в код, код — в QR-коды и куски текста.
async fn make(ctx: Ctx, typed: &str, repeat: Option<&str>, since: Option<Date>) -> Result<Shown, Say> {
    let (key, device_key, fresh) = key_for(ctx.backups, typed, repeat).await?;
    let store = ctx.base.get().ok_or(Say::Key(Key::BackupFailed))?;
    let base = ctx.office.open_name();
    let device = Device::new();
    let now = clock::now();
    let header = sync::Header::new(env!("CARGO_PKG_VERSION"), now, device.sync_id(), since.map(sync::since_day));
    let sealing = Sealing {
        header: &header,
        key: &key,
        salt: device_key.salt(),
        iterations: device_key.iterations(),
    };
    let made = match stored_sync::make(&store, &sealing).await {
        Ok(made) => made,
        Err(Unmade::Failure(failure)) => return Err(Problem::from(failure).into()),
        Err(Unmade::TooLarge(large)) => return Err(Say::TooLarge(large.bytes)),
    };
    // Новый пароль запоминается, когда код под ним уже собран, — как у копии.
    if fresh {
        ctx.backups.password_set(&device_key);
    }
    ctx.backups.password_entered();
    if made.changed == 0 && made.skipped == 0 {
        return Err(Say::Key(Key::SyncNothing));
    }
    device.set_sync_made(&base, now);
    // Записи, легшие сюда копией или кодом, код взял, если начат не позже
    // самой ранней: дальше по умолчанию — день этого кода. Начат позже —
    // человек выбрал день сам, и они ждут следующего.
    let taken = header
        .since()
        .is_none_or(|since| device.sync_arrived(&base).is_none_or(|arrived| since <= arrived));
    if taken {
        device.clear_sync_arrived(&base);
    }
    let codes = sync::qr_codes(&made.code, sync::transfer_id());
    let side = codes.first().map_or(0, qr::Code::side) + 2 * QUIET;
    Ok(Shown {
        qr: codes.iter().map(qr_path).collect(),
        side,
        parts: sync::text_parts(&made.code, sync::transfer_id()),
        since,
        changed: made.changed,
        skipped: made.skipped,
        unreadable: made.unreadable,
    })
}

/// Следующий QR-код круга — через [`QR_STEP`], и снова, пока круг идёт.
fn cycle(index: RwSignal<usize>, count: usize, running: StoredValue<bool>, timer: StoredValue<Option<TimeoutHandle>>) {
    let handle = set_timeout_with_handle(
        move || {
            if running.try_get_value() == Some(true) {
                index.try_update(|i| *i = (*i + 1) % count);
                cycle(index, count, running, timer);
            }
        },
        QR_STEP,
    );
    timer.try_set_value(handle.ok());
}

/// Остановить круг.
fn halt(running: StoredValue<bool>, timer: StoredValue<Option<TimeoutHandle>>) {
    running.try_set_value(false);
    if let Some(Some(handle)) = timer.try_get_value() {
        handle.clear();
    }
}

/// Готовый код: что в нём, QR-коды по кругу и текст кусками.
#[component]
fn ShownCode(lang: ReadSignal<Lang>, router: Router, shown: Rc<Shown>, close: impl Fn() + Copy + 'static) -> impl IntoView {
    let count = shown.qr.len();
    let index = RwSignal::new(0usize);
    // Круг — сменой через `setTimeout`, пока экран на виду: таймер ставится
    // заново на каждый шаг, а ушедший экран круг останавливает — и вернувшийся
    // запускает снова (Правило 1: в покое таймеров нет).
    let running = StoredValue::new(false);
    let timer = StoredValue::new(None::<TimeoutHandle>);
    Effect::new(move |_| {
        let visible = router.shows(Screen::Sync);
        if visible && count > 1 && !running.get_value() {
            running.set_value(true);
            cycle(index, count, running, timer);
        } else if !visible {
            halt(running, timer);
        }
    });
    on_cleanup(move || halt(running, timer));

    let side = shown.side;
    let paths = shown.qr.clone();
    let path = move || paths.get(index.get()).cloned().unwrap_or_default();
    let summary = {
        let (since, changed) = (shown.since, shown.changed as i64);
        move || {
            let lang = lang.get();
            match since {
                Some(day) => fill(t(lang, Key::SyncMadeSince), &[&i18n::date(lang, day), &i18n::integer(lang, changed)]),
                None => fill(t(lang, Key::SyncMadeAll), &[&i18n::integer(lang, changed)]),
            }
        }
    };
    let copied = RwSignal::new(None::<bool>);
    let parts = shown.parts.clone();
    let total = parts.len();
    let all_text = parts.join("\n\n");
    view! {
        <p role="status">{summary}</p>
        {(shown.skipped > 0).then(|| view! { <p class="hint">{counted(lang, Key::SyncSkipped, shown.skipped as i64)}</p> })}
        {(shown.unreadable > 0).then(|| view! { <p role="alert">{counted(lang, Key::SyncUnreadable, shown.unreadable as i64)}</p> })}
        <figure class="sync-qr">
            <svg viewBox=format!("0 0 {side} {side}") role="img" aria-label=move || t(lang.get(), Key::SyncQrLabel)>
                <rect class="sync-qr-paper" width=side height=side />
                <path class="sync-qr-ink" d=path />
            </svg>
            {(count > 1).then(|| view! {
                <figcaption>{move || {
                    let lang = lang.get();
                    fill(t(lang, Key::SyncQrCount), &[&i18n::integer(lang, index.get() as i64 + 1), &i18n::integer(lang, count as i64)])
                }}</figcaption>
            })}
        </figure>
        <p class="hint">{tr(lang, Key::SyncQrHint)}</p>
        <p>{tr(lang, Key::SyncTextAbout)}</p>
        <div class="actions">
            {parts.into_iter().enumerate().map(|(i, part)| {
                // В буфер — в самом нажатии, до первого `await` (Правило 6).
                let copy = move |_: MouseEvent| {
                    let written = clipboard::copy(&part);
                    leptos::task::spawn_local(async move {
                        copied.set(Some(written.await.is_ok()));
                    });
                };
                view! {
                    <button type="button" class="secondary" on:click=copy>
                        {icon(Icon::Copy)}
                        <span class="label">{move || {
                            let lang = lang.get();
                            if total == 1 {
                                t(lang, Key::SyncCopy).to_owned()
                            } else {
                                fill(t(lang, Key::SyncCopyPart), &[&i18n::integer(lang, i as i64 + 1), &i18n::integer(lang, total as i64)])
                            }
                        }}</span>
                    </button>
                }
            }).collect_view()}
        </div>
        {move || copied.get().map(|done| view! {
            <p role=if done { "status" } else { "alert" }>
                {tr(lang, if done { Key::SyncCopied } else { Key::SyncCopyFailed })}
            </p>
        })}
        <details class="sync-text">
            <summary>{tr(lang, Key::SyncTextLabel)}</summary>
            <textarea readonly rows="5" prop:value=all_text></textarea>
        </details>
        <button type="button" class="link" on:click=move |_| close()>
            <span class="label">{tr(lang, Key::SyncClose)}</span>
        </button>
    }
}

/// Принятый код.
struct Accepted {
    restored: Restored,
    /// Цел ли ряд изменений от того устройства.
    continuity: Continuity,
    /// С какого дня код и до какого принято прежде — для слов.
    since: Option<Date>,
    got: Option<Date>,
    /// Правки позже «сейчас» этого устройства (`backup::ahead`).
    ahead: bool,
    /// Легло в пустую программу: так заводят устройство — где выпускать.
    fresh: bool,
}

/// Где приём.
#[derive(Clone)]
enum Receiving {
    Idle,
    /// Камера на экране.
    Scanning,
    /// Код собран — нужен пароль; с проблемой — прошлый не подошёл.
    Password(Rc<Sealed>, Option<Say>),
    Busy,
    Done(Rc<Accepted>),
    Failed(Say),
}

/// Принять сюда: камера или текст — пароль — слияние.
#[component]
fn Receive(ctx: Ctx) -> impl IntoView {
    let Ctx { lang, forms, router, .. } = ctx;
    let receiving = RwSignal::new_local(Receiving::Idle);
    let collector = StoredValue::new_local(Collector::default());
    // Сколько кадров собрано из скольких — для слов; (0, 0) — ни одного.
    let progress = RwSignal::new((0usize, 0usize));
    let pasted = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    forms.hold_while(move || {
        password.with(|text| !text.is_empty())
            || receiving.with(|state| matches!(state, Receiving::Busy | Receiving::Password(..) | Receiving::Scanning))
    });
    // Ушли с экрана — камера гаснет: сканер снимается.
    Effect::new(move |_| {
        if !router.shows(Screen::Sync) && receiving.with_untracked(|state| matches!(state, Receiving::Scanning)) {
            receiving.set(Receiving::Idle);
        }
    });

    // Кадр — в сборку; собран код — открыть и спросить пароль.
    let take = move |found: Result<sync::Frame, FrameError>| {
        let frame = match found {
            Ok(frame) => frame,
            Err(FrameError::Newer) => {
                receiving.set(Receiving::Failed(Say::Key(Key::SyncNewer)));
                return;
            }
            Err(FrameError::NotFrame) => return,
        };
        let (took, got, count, whole) = collector
            .try_update_value(|collector| {
                let took = collector.take(frame);
                (took, collector.got(), collector.count(), collector.whole())
            })
            .unwrap_or((Took::Again, 0, 0, None));
        if took != Took::Again {
            progress.set((got, count));
        }
        if let Some(code) = whole {
            collector.set_value(Collector::default());
            receiving.set(match sync::open(&code) {
                Ok(sealed) => Receiving::Password(Rc::new(sealed), None),
                Err(error) => Receiving::Failed(code_problem(error)),
            });
        }
    };

    let on_paste = move |text: String| {
        for found in sync::frames_in_text(&text) {
            take(found);
        }
    };

    let submit = move |event: SubmitEvent| {
        event.prevent_default();
        let Receiving::Password(sealed, _) = receiving.get_untracked() else {
            return;
        };
        let typed = password.get_untracked();
        receiving.set(Receiving::Busy);
        leptos::task::spawn_local(async move {
            let outcome = accept(ctx, &sealed, &typed).await;
            if outcome.is_ok() {
                password.set(String::new());
                pasted.set(String::new());
                progress.set((0, 0));
            }
            receiving.set(match outcome {
                Ok(accepted) => Receiving::Done(Rc::new(accepted)),
                // Пароль не подошёл — спросить снова, а не снимать код заново.
                Err(say @ Say::Key(Key::SyncWrongPassword)) => Receiving::Password(sealed, Some(say)),
                Err(say) => Receiving::Failed(say),
            });
        });
    };

    let restart = move || {
        collector.set_value(Collector::default());
        progress.set((0, 0));
        pasted.set(String::new());
        password.set(String::new());
        receiving.set(Receiving::Idle);
    };
    let got_words = move || {
        let (got, count) = progress.get();
        (count > 0).then(|| {
            let lang = lang.get();
            fill(t(lang, Key::SyncGot), &[&i18n::integer(lang, got as i64), &i18n::integer(lang, count as i64)])
        })
    };

    view! {
        {move || match receiving.get() {
            Receiving::Password(_, problem) => view! {
                <form class="sync" on:submit=submit>
                    <label>
                        {tr(lang, Key::SyncPassword)}
                        <input type="password" autocomplete="current-password" bind:value=password />
                    </label>
                    {problem.map(|problem| view! { <p role="alert">{move || problem.text(lang.get())}</p> })}
                    <div class="actions">
                        <button type="submit"><span class="label">{tr(lang, Key::SyncAccept)}</span></button>
                        <button type="button" class="secondary" on:click=move |_| restart()>
                            <span class="label">{tr(lang, Key::Cancel)}</span>
                        </button>
                    </div>
                </form>
            }
            .into_any(),
            Receiving::Busy => view! { <p role="status">{tr(lang, Key::SyncAccepting)}</p> }.into_any(),
            Receiving::Scanning => {
                let found = Callback::new(move |bytes: Vec<u8>| take(sync::frame(&bytes)));
                let stop = Callback::new(move |()| receiving.set(Receiving::Idle));
                view! {
                    <p>{tr(lang, Key::SyncScanHint)}</p>
                    {Suspend::new(async move {
                        scanner_part(ScanView { lang, found, stop }).await.ok()
                    })}
                    <p role="status">{got_words}</p>
                }
                .into_any()
            }
            state => view! {
                <div class="actions">
                    <button type="button" on:click=move |_| receiving.set(Receiving::Scanning)>
                        <span class="label">{tr(lang, Key::SyncScan)}</span>
                    </button>
                </div>
                <label class="sync-paste">
                    {tr(lang, Key::SyncPaste)}
                    <textarea
                        rows="3"
                        autocomplete="off"
                        spellcheck="false"
                        prop:value=move || pasted.get()
                        on:input=move |event| {
                            let text = event_target_value(&event);
                            pasted.set(text.clone());
                            on_paste(text);
                        }
                    ></textarea>
                </label>
                <p role="status">{got_words}</p>
                {match state {
                    Receiving::Done(accepted) => Some(view! { <AcceptedWords lang accepted /> }.into_any()),
                    Receiving::Failed(say) => Some(view! { <p role="alert">{move || say.text(lang.get())}</p> }.into_any()),
                    _ => None,
                }}
            }
            .into_any(),
        }}
    }
}

/// Что сказать о принятом: итог слияния, о дыре или первом приёме, о часах
/// и, если программа была пуста, — где выпускать дальше.
#[component]
fn AcceptedWords(lang: ReadSignal<Lang>, accepted: Rc<Accepted>) -> impl IntoView {
    let restored = accepted.restored;
    let (since, got) = (accepted.since, accepted.got);
    let date = move |lang: Lang, day: Option<Date>| day.map(|day| i18n::date(lang, day)).unwrap_or_default();
    let flow = match accepted.continuity {
        Continuity::Whole => None,
        Continuity::First => since.map(|_| {
            view! { <p class="warning">{move || fill(t(lang.get(), Key::SyncFirst), &[&date(lang.get(), since)])}</p> }.into_any()
        }),
        Continuity::Gap { .. } => Some(view! {
            <p class="warning">{move || {
                let lang = lang.get();
                fill(t(lang, Key::SyncGap), &[&date(lang, got), &date(lang, since), &date(lang, got)])
            }}</p>
        }.into_any()),
    };
    view! {
        <p role="status">{move || {
            let lang = lang.get();
            fill(
                t(lang, Key::BackupRestored),
                &[
                    &i18n::integer(lang, restored.added as i64),
                    &i18n::integer(lang, restored.updated as i64),
                    &i18n::integer(lang, restored.unchanged as i64),
                ],
            )
        }}</p>
        {flow}
        {accepted.ahead.then(|| view! { <p class="warning">{tr(lang, Key::BackupAhead)}</p> })}
        {(accepted.fresh && restored.added > 0 && !super::settings::issue_elsewhere_untracked())
            .then(|| view! { <IssuerAsk lang /> })}
    }
}

/// Ошибка кода — словами.
fn code_problem(error: CodeError) -> Say {
    Say::Key(match error {
        CodeError::NotCode => Key::SyncNotCode,
        CodeError::Newer => Key::SyncNewer,
        CodeError::Damaged(why) => {
            web_sys::console::error_1(&format!("Kvartik, код передачи: {why}").into());
            Key::SyncDamaged
        }
    })
}

/// Расшифровать код и слить с базой — или сказать, почему нельзя.
async fn accept(ctx: Ctx, sealed: &Sealed, typed: &str) -> Result<Accepted, Say> {
    let password = crypto::normalize(typed);
    let (code, key) = match stored_sync::unlock(sealed, &password).await {
        Ok(opened) => opened,
        Err(Unread::Crypto(CryptoError::WrongPassword)) => return Err(Say::Key(Key::SyncWrongPassword)),
        Err(Unread::Crypto(error)) => return Err(Problem::from(error).into()),
        Err(Unread::Code(error)) => return Err(code_problem(error)),
    };
    let device = Device::new();
    let header = code.header().clone();
    if header.device() == device.sync_id() {
        return Err(Say::Key(Key::SyncOwn));
    }
    let store = ctx.base.get().ok_or(Say::Key(Key::BackupFailed))?;
    let since = header.since().and_then(sync::day_of_since);
    let failure = |error| Say::from(Problem::from(stored::Failure::from(error)));
    let tx = store.read().map_err(failure)?;
    let fresh = tx.is_empty().await.map_err(failure)?;
    // Изменения «с дня» ложатся, только если здесь данные того устройства:
    // есть его запись профиля — она идёт в каждом коде. Пустая программа,
    // профиль, заведённый здесь своим, — и остальное сюда не придёт никогда,
    // а ряд выглядел бы целым (ревью задачи 110). Профиль не читается —
    // сверять не с чем, и решает слияние, как у копии.
    if header.since().is_some() {
        let theirs: Vec<_> = code
            .entries()
            .iter()
            .filter(|entry| entry.kind() == Profile::KIND)
            .map(|entry| entry.meta().id())
            .collect();
        let shared = match tx.all::<Profile>().await {
            Ok(ours) => theirs.is_empty() && !fresh || ours.iter().any(|our| theirs.contains(&our.id())),
            Err(StoreError::Damaged) => true,
            Err(error) => return Err(failure(error)),
        };
        if !shared {
            return Err(Say::EmptyDelta(since));
        }
    }
    drop(tx);
    // ИП несколько, а код — ИП из списка, не открытого: в открытую базу его
    // не класть (как у копии одного ИП).
    let base = ctx.office.open_name();
    if device.office() {
        let office_store = ctx.office.store().await.map_err(|error| Say::from(Problem::from(stored::Failure::from(error))))?;
        let listed = office_list(&office_store)
            .await
            .map_err(|error| Say::from(Problem::from(stored::Failure::from(error))))?
            .records;
        let found = office_route::listed(code.entries(), &listed).map_err(|error| Say::from(Problem::from(stored::newer(error))))?;
        if let Some(ip) = found
            && ip.base() != base
        {
            let name = match ip.name(ctx.lang.get_untracked()) {
                "" => t(ctx.lang.get_untracked(), Key::OfficeNameless).to_owned(),
                name => name.to_owned(),
            };
            return Err(Say::Elsewhere(name));
        }
    }
    let mut received = device.sync_received(&base);
    let got = received.until(header.device());
    let continuity = sync::continuity(header.since(), got);
    // Часы отправителя впереди — и тогда, когда записи кода старые: момент
    // самого кода позже «сейчас» (Правило 6).
    let now = clock::now();
    let ahead = backup::ahead(code.entries(), now)
        || header.created().unix_millis() > now.unix_millis().saturating_add(backup::CLOCK_SLACK_MILLIS);
    let restored = match stored::restore(&store, KINDS, code.entries(), OtherIe::Ask).await {
        Ok(Merged::Done(restored)) => restored,
        Ok(Merged::Other { ours, theirs }) => return Err(Say::OtherIe(Box::new((ours, theirs)))),
        Err(failure) => return Err(Problem::from(failure).into()),
    };
    if restored.added + restored.updated > 0 {
        ctx.base.replaced();
    }
    super::backup::arrived(&base, restored.oldest);
    if !code.entries().is_empty() {
        ctx.backups.data_saved();
    }
    // Ряд цел — запомнить, до какого момента его часов от него принято;
    // с дырой — нет: следующий код с того же дня скажет о ней снова, пока
    // её не закроют.
    // Момент — не позже «сейчас» этого устройства: часы отправителя, убежавшие
    // вперёд, иначе сделали бы целым любой его следующий код, пока настоящее
    // время их не догонит (ревью задачи 110).
    if !matches!(continuity, Continuity::Gap { .. }) {
        received.note(header.device(), header.created().min(now));
        device.set_sync_received(&base, &received);
    }
    // Своего пароля у устройства нет — берётся пароль кода: человек только что
    // показал, что знает его, а коды отсюда пойдут под ним же туда. Только
    // у устройства с одним ИП, как у копии: у конторы пароль клиента открыл бы
    // её копию всех ИП (ревью задачи 110).
    // И только с полным числом итераций: код с одной итерацией иначе оставил
    // бы устройству слабую проверку пароля (ревью задачи 110).
    let lock = sealed.lock();
    if device.backup_key().is_none()
        && !device.office()
        && lock.iterations() >= KDF_ITERATIONS
        && let Some(opened_with) = DeviceKey::new(lock.iterations(), lock.salt().to_vec(), key.check().to_vec())
    {
        ctx.backups.password_set(&opened_with);
        ctx.backups.password_entered();
    }
    Ok(Accepted {
        restored,
        continuity,
        since,
        got: got.and_then(sync::day_covering),
        ahead,
        fresh,
    })
}

/// Что нужно сканеру: язык, куда отдавать прочитанное и как остановиться.
#[derive(Clone, Copy)]
pub(super) struct ScanView {
    lang: ReadSignal<Lang>,
    found: Callback<Vec<u8>>,
    stop: Callback<()>,
}

split! {
    /// Камера и сканер кадров — своей частью wasm (`ui/split.rs`): нужны
    /// только по «Снять QR-код».
    pub(super) fn scanner_part(view: ScanView) -> AnyView, preload preload_scanner {
        view! { <Scanner view /> }.into_any()
    }
}

/// Сколько точек по длинной стороне у кадра для сканера: код версии 12 на
/// экране телефона занимает в кадре треть — три-четыре точки на модуль,
/// а больший кадр дольше разбирать (замер ядра: 640×480 — 11 мс, 1024×576 —
/// 17 мс на машине автора; в wasm телефона — в несколько раз дольше).
const SCAN_SIDE: usize = 800;

/// Не чаще раза в столько разбирается кадр: между разборами страница
/// отвечает, а камера даёт новый кадр.
const SCAN_EVERY_MILLIS: f64 = 150.0;

/// Где камера.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Lens {
    Starting,
    On,
    Off(CameraError),
}

/// Видео с камеры и разбор его кадров — пока сканер на экране.
#[component]
fn Scanner(view: ScanView) -> impl IntoView {
    let ScanView { lang, found, stop } = view;
    let video = NodeRef::<leptos::html::Video>::new();
    let lens = RwSignal::new(Lens::Starting);
    let camera = StoredValue::new_local(None::<Rc<Camera>>);
    let alive = StoredValue::new(true);
    on_cleanup(move || {
        alive.try_set_value(false);
        camera.try_set_value(None);
    });
    // Разбор кадра — по кадру анимации, но не чаще `SCAN_EVERY_MILLIS`.
    fn tick(camera: StoredValue<Option<Rc<Camera>>, LocalStorage>, alive: StoredValue<bool>, found: Callback<Vec<u8>>, last: f64) {
        request_animation_frame(move || {
            if alive.try_get_value() != Some(true) {
                return;
            }
            let now = js_sys::Date::now();
            let mut last = last;
            if now - last >= SCAN_EVERY_MILLIS
                && let Some(Some(lens)) = camera.try_get_value()
            {
                last = now;
                if let Some(frame) = lens.frame(SCAN_SIDE) {
                    let image = Gray {
                        width: frame.width,
                        height: frame.height,
                        pixels: &frame.gray,
                    };
                    for bytes in qr::scan(&image) {
                        found.run(bytes);
                    }
                }
            }
            tick(camera, alive, found, last);
        });
    }
    Effect::new(move |started: Option<bool>| {
        if started == Some(true) {
            return true;
        }
        let Some(element) = video.get() else {
            return false;
        };
        leptos::task::spawn_local(async move {
            match Camera::open(element.into()).await {
                Ok(opened) => {
                    if alive.try_get_value() == Some(true) {
                        camera.set_value(Some(Rc::new(opened)));
                        lens.set(Lens::On);
                        tick(camera, alive, found, 0.0);
                    }
                }
                Err(error) => lens.set(Lens::Off(error)),
            }
        });
        true
    });
    view! {
        <video class="sync-video" node_ref=video hidden=move || matches!(lens.get(), Lens::Off(_))></video>
        {move || match lens.get() {
            Lens::Starting => Some(view! { <p role="status">{tr(lang, Key::SyncCameraStarting)}</p> }.into_any()),
            Lens::On => None,
            Lens::Off(error) => Some(view! {
                <p role="alert">{tr(lang, match error {
                    CameraError::Denied => Key::SyncCameraDenied,
                    CameraError::Missing => Key::SyncCameraMissing,
                    CameraError::Failed => Key::SyncCameraFailed,
                })}</p>
            }.into_any()),
        }}
        <div class="actions">
            <button type="button" class="secondary" on:click=move |_| stop.run(())>
                <span class="label">{tr(lang, Key::SyncCameraStop)}</span>
            </button>
        </div>
    }
}
