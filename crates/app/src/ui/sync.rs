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
//! QR-коды идут по кругу, пока код на экране, экран открыт и квитанции этого
//! кода нет: это работа, которую человек сам начал, а не покой (Правило 1), —
//! ушёл с экрана или нажал «Готово» — круг стоит, а квитанция, что код
//! принят, останавливает его насовсем (задача 123): экран сказал, что всё
//! дошло, а коды крутились дальше. Камера горит, только пока сканер на
//! экране: уход с экрана её выключает.
//!
//! У кого ИП несколько, код бывает и всех ИП (задача 116, «Код конторы»
//! в `format::sync`): ложится он маршрутом копии всех ИП
//! (`store::backup::restore_all`), а своё о передаче помнит под именем базы
//! конторы (`store::office::OFFICE_DB`), как код одного ИП — под именем базы.
//!
//! Файлы книги и логотипы, которых в коде изменений нет, отправитель передаёт
//! следом кодом файлов (задача 116): он больше обычного и режется на части
//! (`sync::segments`) — у каждой свой круг QR-кодов и свой текст, и человек
//! листает их «Следующий код». Принимающее собирает части в любом порядке
//! (`sync::Series`) и кладёт код, когда собраны все, — тем же путём, что код
//! изменений, ключом его пароля, если он тот же.
//!
//! Принявшее устройство показывает в ответ квитанцию (задача 116,
//! `sync::Receipt`) — одним QR-кодом и текстом, под ключом кода, — а
//! отправитель снимает её под своими QR-кодами, пока ключ ещё в руках: так он
//! узнаёт, что код принят, а о дыре — с какого дня сделать новый, и этот день
//! сам встаёт в поле.
//!
//! Экран начинается с выбора (задача 124): плитки «Передать отсюда» и
//! «Принять сюда», дальше — только выбранный путь с одной главной кнопкой;
//! пустой программе — сразу «Принять» ([`Ways`]). Что войдёт в код — строкой,
//! а день — под «Настроить»; код текстом уходит в меню «Поделиться», а на
//! приёме — «Вставить» из буфера; формы пароля — с логином для менеджера
//! паролей (`ui/password.rs`). Когда был последний обмен, говорит строка
//! профиля (`Device::sync_last`).

use std::fmt::Write as _;
use std::rc::Rc;
use std::time::Duration;

use kvartik_core::format::Stored;
use kvartik_core::format::backup::{self, DeviceKey, IV_BYTES, KDF_ITERATIONS, KINDS, Owner, Tally};
use kvartik_core::format::office as office_route;
use kvartik_core::format::qr::{self, Gray};
use kvartik_core::format::sync::{
    self, Acked, CodeError, Collector, Continuity, Echo, FrameError, MAX_CODE_BYTES, MAX_SERIES_BYTES, Payload, Receipt,
    Sealed, Series, Took,
};
use kvartik_core::i18n::{self, Key, Lang, fill, t};
use kvartik_core::model::date::Date;
use kvartik_core::model::profile::Profile;
use kvartik_core::model::record::Timestamp;
use leptos::ev::{KeyboardEvent, MouseEvent, SubmitEvent};
use leptos::prelude::*;

use super::backup::{Backups, IssuerAsk, Problem, counted, key_for, restored_words, tally_words, who};
use super::form::Forms;
use super::icon::{Icon, icon};
use super::office::Office;
use super::password::{PasswordInput, PasswordLogin, PasswordShow, conceal};
use super::route::{Router, Screen};
use super::split::{screen, split};
use super::storage::Storage;
use super::{Base, tr};
use crate::platform::camera::{Camera, CameraError};
use crate::platform::crypto::{self, CryptoError};
use crate::platform::share::{self, ShareError};
use crate::platform::awake as platform_awake;
use crate::platform::clipboard::Paste;
use crate::platform::{clipboard, clock};
use crate::store::backup::{self as stored, Merged, OtherIe, Restored};
use crate::store::device::Device;
use crate::store::office::{OFFICE_DB, list as office_list};
use crate::store::{Store, StoreError};
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

/// Строка «Другое устройство» в группе «Программа» на экране профиля: день
/// последнего обмена (задача 124, `Device::sync_last`), а не было — что
/// это. Перечитывается, когда показан профиль: обмен делают на соседнем
/// экране. В главном wasm — только чтение одной записи и дата (Правило 1).
#[component]
pub fn SyncEntry(lang: ReadSignal<Lang>, router: Router) -> impl IntoView {
    let meta = move || {
        router.shows(Screen::Profile);
        let lang = lang.get();
        match Device::new().sync_last().and_then(clock::local_date) {
            Some(day) => fill(t(lang, Key::SyncEntryLast), &[&i18n::date(lang, day)]),
            None => t(lang, Key::SyncEntryAbout).to_owned(),
        }
    };
    view! {
        <a class="entry" href=Screen::Sync.href()>
            <span class="entry-main">
                <span class="entry-title">{tr(lang, Key::SyncTitle)}</span>
                <span class="entry-meta">{meta}</span>
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
    let ready = move || matches!(ctx.storage.get(), Storage::Ready(_));
    view! {
        <Show when=ready>
            <Ways ctx />
        </Show>
    }
}

/// Путь на этом устройстве (задача 124).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Way {
    Send,
    Receive,
}

/// Вкладка браузера, данных которой установленная программа не увидит
/// (iPhone, iPad, Mac; Правило 6), а программа ещё не установлена: отсюда
/// данные переносят в неё кодом (задача 124).
#[cfg(not(test))]
fn carrying() -> bool {
    use crate::platform::pwa::{self, Browser};
    Browser::detect().install_first() && !pwa::installed()
}

// Браузерные тесты окна идут в headless Chrome и Firefox — вкладки Safari там
// нет, и перенос тест включает сам.
#[cfg(test)]
thread_local! {
    pub(super) static CARRYING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
fn carrying() -> bool {
    CARRYING.with(std::cell::Cell::get)
}

/// Сначала выбор — две плитки «Передать отсюда» и «Принять сюда», — а дальше
/// только выбранный путь со своей главной кнопкой (задача 124): до неё два
/// раздела шли подряд, «Принять» — на полтора экрана ниже, и главных кнопок
/// было две. Пустой программе передавать нечего: сразу «Принять», без плиток
/// (как экран копии с 0.0.24); вкладке, откуда переносят в установленную
/// программу, — сразу «Передать». Пуста ли программа, решается, когда база
/// открылась, и после принятого кода экран не перестраивается — иначе
/// пропали бы итог и квитанция. Невыбранный путь спрятан, а не убран:
/// начатое в нём не пропадает, а круг QR-кодов и камера стоят, пока он не на
/// виду (`seen`).
#[component]
fn Ways(ctx: Ctx) -> impl IntoView {
    let Ctx { lang, backups, router, .. } = ctx;
    let empty = !untrack(|| backups.has_data());
    let carry = !empty && carrying();
    let way = RwSignal::new(if empty {
        Some(Way::Receive)
    } else if carry {
        Some(Way::Send)
    } else {
        None
    });
    let seen = move |path: Way| Signal::derive(move || router.shows(Screen::Sync) && way.get() == Some(path));
    view! {
        // Первая строка видна, остальное — под «Подробнее» (задача 124).
        <p>{tr(lang, Key::SyncAbout)}</p>
        <details class="more">
            <summary>{tr(lang, Key::More)}</summary>
            <p>{tr(lang, Key::SyncAboutMore)}</p>
        </details>
        <p class="hint">
            {tr(lang, Key::SyncIssueNote)}" "
            <a href=Screen::Settings.href()>{tr(lang, Key::SettingsTitle)}</a>
        </p>
        {if empty {
            view! { <h2>{tr(lang, Key::SyncReceiveTitle)}</h2> }.into_any()
        } else {
            view! {
                <div class="choices sync-ways" role="radiogroup" aria-label=move || t(lang.get(), Key::SyncWay)>
                    <WayChoice lang way path=Way::Send label=Key::SyncSendTitle hint=Key::SyncSendHint />
                    <WayChoice lang way path=Way::Receive label=Key::SyncReceiveTitle hint=Key::SyncReceiveHint />
                </div>
            }
            .into_any()
        }}
        {(!empty).then(|| view! {
            <div class="sync-way" hidden=move || way.get() != Some(Way::Send)>
                <Send ctx seen=seen(Way::Send) carry />
            </div>
        })}
        <div class="sync-way" hidden=move || way.get() != Some(Way::Receive)>
            <Receive ctx seen=seen(Way::Receive) />
        </div>
    }
}

/// Плитка пути — карточкой выбора, как «Где зарегистрирован клиент».
#[component]
fn WayChoice(lang: ReadSignal<Lang>, way: RwSignal<Option<Way>>, path: Way, label: Key, hint: Key) -> impl IntoView {
    view! {
        <label class="choice-card" data-tilt="">
            <input
                type="radio"
                name="sync-way"
                class="visually-hidden"
                prop:checked=move || way.get() == Some(path)
                on:change=move |_| way.set(Some(path))
            />
            <span class="choice-title">{tr(lang, label)} {icon(Icon::Check)}</span>
            <span class="choice-hint">{tr(lang, hint)}</span>
        </label>
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
    /// Код файлов больше серии: сколько КБ вышло бы.
    FilesTooLarge(usize),
    /// Здесь пусто, а в коде — изменения с дня.
    EmptyDelta(Option<Date>),
    /// Код ИП из списка, а открыт другой: его имя.
    Elsewhere(String),
    /// Код другого ИП: оба — ИП программы (`None` — профиль не читается)
    /// и ИП кода.
    OtherIe(Box<(Option<Owner>, Owner)>),
    /// Код всех ИП «с дня» с ИП, которого здесь нет: его имя.
    MissingIp(String),
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
            Self::FilesTooLarge(bytes) => fill(
                t(lang, Key::SyncFilesTooLarge),
                &[&megabytes(lang, *bytes), &megabytes(lang, MAX_SERIES_BYTES)],
            ),
            Self::EmptyDelta(day) => fill(
                t(lang, Key::SyncEmptyDelta),
                &[&day.map(|day| i18n::date(lang, day)).unwrap_or_default()],
            ),
            Self::Elsewhere(name) => fill(t(lang, Key::SyncElsewhere), &[name]),
            Self::MissingIp(name) => fill(t(lang, Key::SyncMissingIp), &[name]),
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

/// Мегабайты с десятыми (задача 124): «130 161 КБ из 1 023» о файлах книги
/// человек не читал — «127,1 МБ из 1» читает. Мегабайт — 2²⁰ байтов, как
/// у копии (`backup::megabytes`); десятые — вверх, чтобы файл в 30 КБ не был
/// «0 МБ».
fn megabytes(lang: Lang, bytes: usize) -> String {
    let tenths = (bytes as u64 * 10).div_ceil(1 << 20);
    i18n::decimal(lang, i64::try_from(tenths).unwrap_or(i64::MAX), 1)
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
    /// Изменённые записи кода — по видам (задача 124, п. 21).
    tally: Tally,
    skipped: usize,
    /// Сколько байтов вышло бы у кода этих файлов — оценкой (задача 124):
    /// не поместятся в серию — кнопка копии файлом, а не серии.
    skipped_bytes: usize,
    unreadable: usize,
    /// Ключ кода — им зашифрована квитанция в ответ (задача 116).
    key: Rc<crypto::Key>,
    /// Момент кода по часам этого устройства — по нему квитанция узнаёт
    /// свой код.
    created: Timestamp,
    /// База, чей код: квитанция с дырой сдвигает день её следующего кода.
    base: String,
    /// Чтобы собрать код файлов того же охвата: с какого момента, всех ли ИП,
    /// соль и итерации пароля копий — как у этого кода.
    since_at: Option<Timestamp>,
    everyone: bool,
    salt: Vec<u8>,
    iterations: u32,
}

/// Поместятся ли файлы в серию кодов (`sync::files_fit`).
#[cfg(not(test))]
fn files_fit(bytes: usize) -> bool {
    sync::files_fit(bytes)
}

// Браузерный тест отказа серии (размер разошёлся с оценкой) ставит «поместятся»
// сам: файлов, у которых код вышел бы больше оценки, тест не соберёт.
#[cfg(test)]
thread_local! {
    pub(super) static FITS: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}

#[cfg(test)]
fn files_fit(bytes: usize) -> bool {
    FITS.with(std::cell::Cell::get).unwrap_or_else(|| sync::files_fit(bytes))
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

/// День по умолчанию для поля «Изменения с этого дня» (`sync::default_day`):
/// `base` — база ИП, а у кода всех ИП — база конторы.
fn default_day(base: &str) -> Option<Date> {
    day_after(base, Device::new().sync_arrived(base))
}

/// День по умолчанию с моментом легших сюда записей `arrived` — у встречного
/// кода он свой: до принятого кода ([`Unsent`]).
fn day_after(base: &str, arrived: Option<Timestamp>) -> Option<Date> {
    let today = clock::today()?;
    sync::default_day(
        Device::new().sync_made(base).and_then(clock::local_date),
        arrived.and_then(sync::day_covering),
        today,
    )
}

/// Передать отсюда: что, пароль копий — и код на экране. `seen` — путь на
/// виду: круг QR-кодов и камера квитанции идут только тогда. `carry` —
/// вкладка, откуда переносят в установленную программу ([`carrying`]).
#[component]
fn Send(ctx: Ctx, seen: Signal<bool>, carry: bool) -> impl IntoView {
    let Ctx {
        lang,
        backups,
        office,
        forms,
        router,
        ..
    } = ctx;
    // ИП несколько — по умолчанию код всех: бухгалтер ведёт их на двух
    // устройствах разом (задача 116).
    let several = Device::new().office();
    let everyone = RwSignal::new(several);
    let key_of = move |everyone: bool| if everyone { OFFICE_DB.to_owned() } else { office.open_name() };
    // День по умолчанию (`sync::default_day`): по нему строка над «Настроить»
    // говорит «новое с прошлой передачи», а не просто день (задача 124).
    let usual = RwSignal::new(default_day(&key_of(several)));
    let first = usual.get_untracked();
    let all = RwSignal::new(first.is_none());
    let day = RwSignal::new(first.or_else(clock::today).map(Date::iso).unwrap_or_default());
    // Сменили, чьи изменения, — день по умолчанию того кода.
    let whose = move |chosen: bool| {
        everyone.set(chosen);
        let next = default_day(&key_of(chosen));
        usual.set(next);
        all.set(next.is_none());
        day.set(next.or_else(clock::today).map(Date::iso).unwrap_or_default());
    };
    // Что войдёт в код — одной строкой (задача 124): день и «все записи»
    // нужны редко и свёрнуты под «Настроить».
    let what = move || {
        let lang = lang.get();
        if all.get() {
            return t(lang, if usual.get().is_none() { Key::SyncWhatFirst } else { Key::SyncWhatAll }).to_owned();
        }
        match Date::from_iso(&day.get()) {
            Some(chosen) if Some(chosen) == usual.get() => fill(t(lang, Key::SyncWhatNew), &[&i18n::date(lang, chosen)]),
            Some(chosen) => fill(t(lang, Key::SyncWhatDay), &[&i18n::date(lang, chosen)]),
            None => t(lang, Key::SyncDayMissing).to_owned(),
        }
    };
    let password = RwSignal::new(String::new());
    let repeat = RwSignal::new(String::new());
    let shown = RwSignal::new(false);
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
        conceal(&event, shown);
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
            let outcome = make(ctx, &typed, fresh.then_some(again.as_str()), since, everyone.get_untracked()).await;
            if outcome.is_ok() {
                password.set(String::new());
                repeat.set(String::new());
                // Код переноса сделан — карточка переноса на главном больше не
                // нужна (`ui::pwa::CarryCard`, ревью задачи 124).
                if carry {
                    Device::new().set_carry_closed();
                }
            }
            sending.set(match outcome {
                Ok(shown) => Sending::Ready(Rc::new(shown)),
                Err(say) => Sending::Failed(say),
            });
        });
    };

    view! {
        // Код готов — форма пропадает: так менеджер паролей видит, что она
        // отправлена (`ui/password.rs`).
        <form class="sync" hidden=move || matches!(sending.get(), Sending::Ready(_)) on:submit=submit>
            <PasswordLogin />
            {carry.then(|| view! { <p class="hint">{tr(lang, Key::SyncCarryNote)}</p> })}
            {several.then(|| view! {
                <fieldset class="group">
                    <legend>{tr(lang, Key::SyncWhose)}</legend>
                    <label class="choice">
                        <input type="radio" name="sync-whose" prop:checked=move || everyone.get() on:change=move |_| whose(true) />
                        {tr(lang, Key::SyncWhoseAll)}
                    </label>
                    <label class="choice">
                        <input type="radio" name="sync-whose" prop:checked=move || !everyone.get() on:change=move |_| whose(false) />
                        {tr(lang, Key::SyncWhoseOpen)}
                    </label>
                </fieldset>
            })}
            <p class="sync-what">{what}</p>
            <details class="more sync-tune">
                <summary>{tr(lang, Key::SyncTune)}</summary>
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
            </details>
            {move || if new_password() {
                // Свои слова под паролем (задача 124): подсказка копии
                // говорила «спросит при каждой копии» — о копии, а не о коде.
                view! {
                    <PasswordInput lang label=Key::BackupNewPassword value=password shown fresh=true />
                    <PasswordInput lang label=Key::BackupRepeatPassword value=repeat shown fresh=true />
                    <PasswordShow lang shown />
                    <p class="hint">
                        {counted(lang, Key::SyncPasswordHint, backup::MIN_PASSWORD_CHARS as i64)}
                    </p>
                }
                .into_any()
            } else {
                view! {
                    <PasswordInput lang label=Key::BackupPassword value=password shown />
                    <PasswordShow lang shown />
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
            // Перенос из вкладки, а код больше предела (ревью задачи 124): не
            // «выберите день позже» — старое осталось бы во вкладке, которую
            // Safari сотрёт, а человек решил бы, что перенёс всё. Копией файлом
            // переедет всё, и файлы книги тоже.
            Sending::Failed(Say::TooLarge(bytes)) if carry => Some(view! {
                <p role="alert">{move || {
                    let lang = lang.get();
                    fill(
                        t(lang, Key::SyncCarryTooLarge),
                        &[
                            &i18n::integer(lang, bytes.div_ceil(1024) as i64),
                            &i18n::integer(lang, (MAX_CODE_BYTES / 1024) as i64),
                        ],
                    )
                }}</p>
                <a class="button" href=Screen::Backup.href()><span class="label">{tr(lang, Key::BackupGoTo)}</span></a>
            }.into_any()),
            Sending::Failed(say) => Some(view! { <p role="alert">{move || say.text(lang.get())}</p> }.into_any()),
            // Код убран — день по умолчанию уже другой: день этого кода.
            Sending::Ready(shown) => Some(view! {
                <ShownCode ctx seen carry shown gap=Callback::new(move |chosen: Date| {
                    day.set(chosen.iso());
                    all.set(false);
                }) close=move || {
                    sending.set(Sending::Idle);
                    let next = default_day(&key_of(everyone.get_untracked()));
                    usual.set(next);
                    if let Some(next) = next {
                        day.set(next.iso());
                        all.set(false);
                    }
                } />
            }.into_any()),
        }}
    }
}

/// Ключ кода и замок его пароля: соль и итерации пароля копий того
/// устройства, которое код сделало первым.
#[derive(Clone)]
struct CodeLock {
    key: Rc<crypto::Key>,
    salt: Vec<u8>,
    iterations: u32,
}

/// Собрать код: пароль копий — сверить с устройством или задать, записи
/// с дня `since` — в код (`everyone` — всех ИП), код — в QR-коды и куски
/// текста. Сделанный код сдвигает ряд изменений отсюда: день следующего
/// кода по умолчанию — день этого.
async fn make(ctx: Ctx, typed: &str, repeat: Option<&str>, since: Option<Date>, everyone: bool) -> Result<Shown, Say> {
    let (key, device_key, fresh) = key_for(ctx.backups, typed, repeat).await?;
    let lock = CodeLock {
        key: Rc::new(key),
        salt: device_key.salt().to_vec(),
        iterations: device_key.iterations(),
    };
    let (shown, changed) = seal_code(ctx, &lock, since, everyone, None).await?;
    // Новый пароль запоминается, когда код под ним уже собран, — как у копии.
    if fresh {
        ctx.backups.password_set(&device_key);
    }
    ctx.backups.password_entered();
    if changed == 0 && shown.skipped == 0 {
        return Err(Say::Key(Key::SyncNothing));
    }
    let device = Device::new();
    device.set_sync_made(&shown.base, shown.created);
    // «Последний обмен» — не здесь: код, который никто не снял, обменом не
    // был (ревью задачи 124). Его ставят квитанция «принято» (`Reply`)
    // и принятый код (`accept`).
    // Записи, легшие сюда копией или кодом, код взял, если начат не позже
    // самой ранней: дальше по умолчанию — день этого кода. Начат позже —
    // человек выбрал день сам, и они ждут следующего.
    let taken = shown
        .since_at
        .is_none_or(|since| device.sync_arrived(&shown.base).is_none_or(|arrived| since <= arrived));
    if taken {
        device.clear_sync_arrived(&shown.base);
    }
    Ok(shown)
}

/// Что ждало здесь кода отсюда до принятого кода (`Device::sync_arrived`
/// открытой базы и базы конторы). Встречный код берёт записи с дня по нему,
/// а не по легшим принятым кодом: те у того устройства уже есть, и код вышел
/// бы вдвое длиннее.
#[derive(Clone, Copy)]
struct Unsent {
    open: Option<Timestamp>,
    office: Option<Timestamp>,
}

/// Встречный код (задача 124, п. 14): изменения отсюда — ключом только что
/// принятого кода, без пароля, с дня по умолчанию этого устройства до того
/// кода (`unsent`). Ряд изменений отсюда он не сдвигает: квитанции на
/// встречный код нет, и день следующего кода, сдвинутый кодом, который не
/// сняли, спрятал бы дыру (риск в записи задачи) — как у кода файлов. Охват —
/// тот же, что у принятого кода (`everyone` — он был кодом всех ИП): на код
/// одного ИП бухгалтер с конторой отвечал кодом всех своих ИП, и тому, у кого
/// ИП один, без вопроса легли бы все ИП конторы (ревью задачи 124). Записей,
/// которые здесь те же, что пришли принятым кодом (`echo`), в нём нет: день
/// берёт и правки того устройства после него, а они у него уже есть
/// (решение человека 2026-11-25). Остались одни они — «передавать нечего».
async fn make_back(ctx: Ctx, lock: &CodeLock, unsent: Unsent, echo: &Echo, everyone: bool) -> Result<Shown, Say> {
    let (base, arrived) = if everyone {
        (OFFICE_DB.to_owned(), unsent.office)
    } else {
        (ctx.office.open_name(), unsent.open)
    };
    let (shown, changed) = seal_code(ctx, lock, day_after(&base, arrived), everyone, Some(echo)).await?;
    if changed == 0 && shown.skipped == 0 {
        return Err(Say::Key(Key::SyncNothing));
    }
    Ok(shown)
}

/// Код ключом `lock`: записи с дня `since` (`everyone` — всех ИП), кроме эха
/// принятого кода (`echo`, у встречного кода), — в QR-коды и куски текста;
/// вместе с кодом — сколько в нём изменённых записей. Ряда изменений не
/// трогает: это решает тот, кто зовёт.
async fn seal_code(
    ctx: Ctx,
    lock: &CodeLock,
    since: Option<Date>,
    everyone: bool,
    echo: Option<&Echo>,
) -> Result<(Shown, usize), Say> {
    let store = ctx.base.get().ok_or(Say::Key(Key::BackupFailed))?;
    // Контору не открыть — отказ, а не код одного открытого ИП молча: человек
    // считал бы, что в нём все.
    let office = if everyone {
        Some(ctx.office.full().await.map_err(|error| Say::from(Problem::from(stored::Failure::from(error))))?.ok_or(Say::Key(Key::BackupFailed))?)
    } else {
        None
    };
    let base = if office.is_some() { OFFICE_DB.to_owned() } else { ctx.office.open_name() };
    let device = Device::new();
    let now = clock::now();
    let header = sync::Header::new(env!("CARGO_PKG_VERSION"), now, device.sync_id(), since.map(sync::since_day));
    let sealing = Sealing {
        header: &header,
        key: &lock.key,
        salt: &lock.salt,
        iterations: lock.iterations,
        echo,
    };
    let made = match &office {
        Some((office_store, open_name)) => stored_sync::make_all(&store, open_name, office_store, &sealing).await,
        None => stored_sync::make(&store, &sealing).await,
    };
    let made = match made {
        Ok(made) => made,
        Err(Unmade::Failure(failure)) => return Err(Problem::from(failure).into()),
        Err(Unmade::TooLarge(large)) => return Err(Say::TooLarge(large.bytes)),
    };
    let codes = sync::qr_codes(&made.code, sync::transfer_id());
    let side = codes.first().map_or(0, qr::Code::side) + 2 * QUIET;
    let shown = Shown {
        qr: codes.iter().map(qr_path).collect(),
        side,
        parts: sync::text_parts(&made.code, sync::transfer_id()),
        since,
        tally: made.tally,
        skipped: made.skipped,
        skipped_bytes: made.skipped_bytes,
        unreadable: made.unreadable,
        key: Rc::clone(&lock.key),
        created: now,
        base,
        since_at: header.since(),
        everyone: office.is_some(),
        salt: lock.salt.clone(),
        iterations: lock.iterations,
    };
    Ok((shown, made.changed))
}

/// Дольше этого экран при коде не держится (ревью задачи 124): три круга
/// кода из `count` QR-кодов, но не меньше двух минут — камере хватает
/// и одного круга, а оставленный телефон гаснет и запирается сам.
fn awake_limit(count: usize) -> Duration {
    #[cfg(test)]
    if let Some(limit) = AWAKE_LIMIT.with(std::cell::Cell::get) {
        return limit;
    }
    (QR_STEP * u32::try_from(count.saturating_mul(3)).unwrap_or(u32::MAX)).max(Duration::from_secs(120))
}

// Браузерный тест предела ждёт не две минуты, а долю секунды.
#[cfg(test)]
thread_local! {
    pub(super) static AWAKE_LIMIT: std::cell::Cell<Option<Duration>> = const { std::cell::Cell::new(None) };
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

/// Код на экране — QR-коды по кругу и текст кусками со «Скопировать»: у кода
/// изменений и у каждой части кода файлов. `accepted` — квитанция сказала,
/// что код принят (задача 123): круг стоит и после возврата на экран.
/// `hint` — подсказка под кодами: когда круг кончится. `seen` — путь
/// «Передать отсюда» на виду (задача 124).
#[component]
fn CodeShown(
    lang: ReadSignal<Lang>,
    seen: Signal<bool>,
    qr: Vec<String>,
    side: usize,
    parts: Vec<String>,
    accepted: Signal<bool>,
    hint: Key,
    /// Перенос из вкладки в установленную программу ([`carrying`]).
    carry: bool,
) -> impl IntoView {
    let count = qr.len();
    let index = RwSignal::new(0usize);
    // Круг — сменой через `setTimeout`, пока путь на виду и код не принят:
    // таймер ставится заново на каждый шаг, а ушедший экран или выбранное
    // «Принять сюда» круг останавливает — и вернувшийся запускает снова
    // (Правило 1: в покое таймеров нет). Квитанция этого кода круг
    // останавливает навсегда: экран сказал, что всё дошло, и показывать работу
    // дальше незачем (найдено человеком 2026-11-24).
    let running = StoredValue::new(false);
    let timer = StoredValue::new(None::<TimeoutHandle>);
    // Экран не гаснет, пока код на виду и не принят (задача 124, A3): камера
    // того устройства снимает круг до минуты, а телефон без касания гасит
    // экран через полминуты. «Принято», «Готово» и уход с пути отпускают его,
    // а дольше [`awake_limit`] он не держится и без них: у встречного кода
    // и кода файлов квитанции нет, и оставленный с кодом телефон иначе не гас
    // бы и не запирался сам (ревью задачи 124). Вернулись на путь — снова.
    let awake = StoredValue::new(false);
    let tired = RwSignal::new(false);
    let tired_timer = StoredValue::new(None::<TimeoutHandle>);
    let wake = move |on: bool| {
        if awake.try_get_value() != Some(!on) {
            return;
        }
        awake.try_set_value(on);
        if let Some(Some(handle)) = tired_timer.try_get_value() {
            handle.clear();
        }
        if on {
            platform_awake::hold();
            let handle = set_timeout_with_handle(move || { tired.try_set(true); }, awake_limit(count));
            tired_timer.try_set_value(handle.ok());
        } else {
            tired_timer.try_set_value(None);
            platform_awake::let_go();
        }
    };
    Effect::new(move |_| {
        let go = seen.get() && !accepted.get();
        if !go && tired.get_untracked() {
            tired.set(false);
        }
        wake(go && !tired.get());
        if go && count > 1 && !running.get_value() {
            running.set_value(true);
            cycle(index, count, running, timer);
        } else if !go {
            halt(running, timer);
        }
    });
    on_cleanup(move || {
        halt(running, timer);
        wake(false);
    });
    let frames = StoredValue::new(qr);
    let path = move || frames.with_value(|qr| qr.get(index.get()).cloned()).unwrap_or_default();
    let caption = move || {
        let lang = lang.get();
        fill(t(lang, Key::SyncQrCount), &[&i18n::integer(lang, index.get() as i64 + 1), &i18n::integer(lang, count as i64)])
    };
    let copied = RwSignal::new(None::<bool>);
    let share_failed = RwSignal::new(false);
    let total = parts.len();
    let all_text = parts.join("\n\n");
    // «Отправить» — где меню «Поделиться» берёт текст (задача 124): код —
    // в мессенджер одним нажатием, без «Скопировать» и вставки там.
    let shares = parts.first().is_some_and(|part| share::can_share_text(part));
    // QR-коды на весь экран (задача 124, п. 17): камера другого устройства
    // читает крупный код с метра, а в строке экрана телефона он мелкий. Круг
    // идёт и там — тот же индекс.
    let full = RwSignal::new(false);
    let open_full = NodeRef::<leptos::html::Button>::new();
    let close_full = NodeRef::<leptos::html::Button>::new();
    // Фокус — в окне, пока оно открыто, и на кнопке, что его открыла, когда
    // закрыто (задача 124, A4): окно `aria-modal`, а Tab уводил фокус под него,
    // на кнопки программы, которых не видно.
    let focus = move |button: NodeRef<leptos::html::Button>| {
        request_animation_frame(move || {
            if let Some(button) = button.get_untracked() {
                let _ = button.focus();
            }
        });
    };
    let shut = move || {
        if full.get_untracked() {
            full.set(false);
            focus(open_full);
        }
    };
    Effect::new(move |_| {
        if full.get() {
            focus(close_full);
        }
    });
    // Квитанция «принято» пришла — окно закрывается само: код сняли, и он
    // больше не нужен на весь экран.
    Effect::new(move |_| {
        if accepted.get() {
            shut();
        }
    });
    view! {
        <figure class="sync-qr">
            <svg viewBox=format!("0 0 {side} {side}") role="img" aria-label=move || t(lang.get(), Key::SyncQrLabel)>
                <rect class="sync-qr-paper" width=side height=side />
                <path class="sync-qr-ink" d=path />
            </svg>
            {(count > 1).then(|| view! { <figcaption>{caption}</figcaption> })}
        </figure>
        <div class="actions">
            <button type="button" class="secondary" node_ref=open_full on:click=move |_| full.set(true)>
                <span class="label">{tr(lang, Key::SyncQrFull)}</span>
            </button>
        </div>
        // Окно поверх программы, как «Что нового»: Escape и «Свернуть»
        // возвращают код на место, фокус — на кнопке. Кнопка в окне одна,
        // и Tab остаётся на ней.
        {move || full.get().then(|| view! {
            <div
                class="sync-full"
                role="dialog"
                aria-modal="true"
                aria-label=move || t(lang.get(), Key::SyncQrLabel)
                on:keydown=move |event: KeyboardEvent| match event.key().as_str() {
                    "Escape" => shut(),
                    "Tab" => {
                        event.prevent_default();
                        focus(close_full);
                    }
                    _ => {}
                }
            >
                <svg viewBox=format!("0 0 {side} {side}") role="img" aria-label=move || t(lang.get(), Key::SyncQrLabel)>
                    <rect class="sync-qr-paper" width=side height=side />
                    <path class="sync-qr-ink" d=path />
                </svg>
                {(count > 1).then(|| view! { <p class="sync-full-count">{caption}</p> })}
                <p>{tr(lang, Key::SyncQrBright)}</p>
                <button type="button" class="wide" node_ref=close_full on:click=move |_| shut()>
                    <span class="label">{tr(lang, Key::SyncQrFullClose)}</span>
                </button>
            </div>
        })}
        <p class="hint">{tr(lang, hint)}</p>
        <p>{tr(lang, Key::SyncTextAbout)}</p>
        <div class="actions">
            // Перенос из вкладки в программу на том же устройстве (ревью
            // задачи 124): одна главная кнопка — весь текст разом. Приём
            // разбирает все части из одной вставки (`sync::frames_in_text`),
            // а код в 32 КиБ — около двенадцати кругов «скопировать —
            // вставить».
            {carry.then(|| {
                let whole = all_text.clone();
                let copy_all = move |_: MouseEvent| {
                    let written = clipboard::copy(&whole);
                    leptos::task::spawn_local(async move {
                        copied.set(Some(written.await.is_ok()));
                    });
                };
                view! {
                    <button type="button" on:click=copy_all>
                        {icon(Icon::Copy)}
                        <span class="label">{tr(lang, Key::SyncCopyAll)}</span>
                    </button>
                }
            })}
            {(!carry).then(|| parts.into_iter().enumerate().map(|(i, part)| {
                let text = part.clone();
                // В буфер — в самом нажатии, до первого `await` (Правило 6).
                let copy = move |_: MouseEvent| {
                    let written = clipboard::copy(&part);
                    leptos::task::spawn_local(async move {
                        copied.set(Some(written.await.is_ok()));
                    });
                };
                // Меню — тоже в самом нажатии: оно тратит его и при отказе
                // (`platform::share`). Закрытое меню молчит.
                let send = move |_: MouseEvent| {
                    share_failed.set(false);
                    let sent = share::share_text(&text);
                    leptos::task::spawn_local(async move {
                        if sent.await == Err(ShareError::Failed) {
                            share_failed.set(true);
                        }
                    });
                };
                // Подпись кнопки: у единственного куска — без номера.
                let numbered = move |whole: Key, piece: Key| {
                    move || {
                        let lang = lang.get();
                        if total == 1 {
                            t(lang, whole).to_owned()
                        } else {
                            fill(t(lang, piece), &[&i18n::integer(lang, i as i64 + 1), &i18n::integer(lang, total as i64)])
                        }
                    }
                };
                view! {
                    {shares.then(|| view! {
                        <button type="button" class="secondary" on:click=send>
                            {icon(Icon::Share)}
                            <span class="label">{numbered(Key::SyncShare, Key::SyncSharePart)}</span>
                        </button>
                    })}
                    <button type="button" class="secondary" on:click=copy>
                        {icon(Icon::Copy)}
                        <span class="label">{numbered(Key::SyncCopy, Key::SyncCopyPart)}</span>
                    </button>
                }
            }).collect_view())}
        </div>
        {move || copied.get().map(|done| view! {
            <p role=if done { "status" } else { "alert" }>
                {tr(lang, if done { Key::SyncCopied } else { Key::SyncCopyFailed })}
            </p>
        })}
        {move || share_failed.get().then(|| view! { <p role="alert">{tr(lang, Key::SyncShareFailed)}</p> })}
        <details class="sync-text">
            <summary>{tr(lang, Key::SyncTextLabel)}</summary>
            <textarea readonly rows="5" prop:value=all_text></textarea>
        </details>
    }
}

/// Часть кода файлов на экране: QR-коды и куски текста.
struct FilePart {
    qr: Vec<String>,
    side: usize,
    text: Vec<String>,
}

/// Готовый код файлов — части серии.
struct FilesShown {
    parts: Vec<FilePart>,
}

/// Где код файлов у отправителя.
#[derive(Clone)]
enum FilesState {
    /// Код изменений на экране.
    Off,
    Busy,
    /// Часть с этим номером на экране.
    On(Rc<FilesShown>, usize),
    Failed(Say),
}

/// Собрать код файлов того же охвата, что код изменений `shown` (задача 116):
/// файлы книги и логотипы, изменённые с того же момента, тем же ключом, —
/// и разрезать на части серии.
async fn make_files(ctx: Ctx, shown: Rc<Shown>) -> Result<FilesShown, Say> {
    let store = ctx.base.get().ok_or(Say::Key(Key::BackupFailed))?;
    let header = sync::Header::new(env!("CARGO_PKG_VERSION"), clock::now(), Device::new().sync_id(), shown.since_at).for_files();
    let sealing = Sealing {
        header: &header,
        key: &shown.key,
        salt: &shown.salt,
        iterations: shown.iterations,
        echo: None,
    };
    let made = if shown.everyone {
        let (office_store, open_name) = ctx
            .office
            .full()
            .await
            .map_err(|error| Say::from(Problem::from(stored::Failure::from(error))))?
            .ok_or(Say::Key(Key::BackupFailed))?;
        stored_sync::make_all(&store, &open_name, &office_store, &sealing).await
    } else {
        stored_sync::make(&store, &sealing).await
    };
    let made = match made {
        Ok(made) => made,
        Err(Unmade::Failure(failure)) => return Err(Problem::from(failure).into()),
        Err(Unmade::TooLarge(large)) => return Err(Say::FilesTooLarge(large.bytes)),
    };
    let parts = sync::segments(&made.code, sync::transfer_id())
        .iter()
        .map(|segment| {
            let codes = sync::qr_codes(segment, sync::transfer_id());
            FilePart {
                side: codes.first().map_or(0, qr::Code::side) + 2 * QUIET,
                qr: codes.iter().map(qr_path).collect(),
                text: sync::text_parts(segment, sync::transfer_id()),
            }
        })
        .collect();
    Ok(FilesShown { parts })
}

/// Что в готовом коде — по видам (задача 124, п. 21), сколько QR-кодов
/// и сколько идёт круг, что не вошло: у кода изменений и у встречного кода.
/// `skipped` — слова о файлах, которых в коде нет: у встречного кода под ним
/// нет кнопки кода файлов, а файлы больше серии кодов везёт копия файлом.
/// Шаблон берёт число файлов, их МБ и сколько МБ вмещает серия.
#[component]
fn MadeWords(lang: ReadSignal<Lang>, shown: Rc<Shown>, skipped: Key) -> impl IntoView {
    let (since, tally) = (shown.since, shown.tally);
    let summary = move || {
        let lang = lang.get();
        match since {
            Some(day) => fill(t(lang, Key::SyncMadeSince), &[&i18n::date(lang, day), &tally_words(lang, tally)]),
            None => fill(t(lang, Key::SyncMadeAll), &[&tally_words(lang, tally)]),
        }
    };
    // Сколько QR-кодов и сколько идёт круг (задача 124): у кода в 32 КиБ их
    // около ста двадцати, и пропущенный ждёт до минуты, — человек знает это
    // до того, как поднял камеру. У одного QR-кода круга нет.
    let codes = shown.qr.len();
    let round = move || {
        let lang = lang.get();
        let seconds = (QR_STEP.as_millis() * codes as u128).div_ceil(1000);
        fill(
            t(lang, Key::SyncMadeQr),
            &[&i18n::integer(lang, codes as i64), &i18n::integer(lang, i64::try_from(seconds).unwrap_or(i64::MAX))],
        )
    };
    view! {
        <p role="status">{summary}</p>
        {(codes > 1).then(|| view! { <p class="hint">{round}</p> })}
        {(shown.skipped > 0).then(|| {
            let (count, bytes) = (shown.skipped, shown.skipped_bytes);
            view! {
                <p class="hint">{move || {
                    let lang = lang.get();
                    fill(
                        t(lang, skipped),
                        &[&i18n::integer(lang, count as i64), &megabytes(lang, bytes), &megabytes(lang, MAX_SERIES_BYTES)],
                    )
                }}</p>
            }
        })}
        {(shown.unreadable > 0).then(|| view! { <p role="alert">{counted(lang, Key::SyncUnreadable, shown.unreadable as i64)}</p> })}
    }
}

/// Готовый код: что в нём, QR-коды по кругу и текст кусками, а под ними —
/// квитанция в ответ. Файлы, которых в коде нет, — кодом файлов по нажатию,
/// частями вместо кода изменений. `gap` — квитанция назвала дыру: день,
/// с которого сделать новый код.
#[component]
fn ShownCode(
    ctx: Ctx,
    seen: Signal<bool>,
    carry: bool,
    shown: Rc<Shown>,
    gap: Callback<Date>,
    close: impl Fn() + Copy + 'static,
) -> impl IntoView {
    let Ctx { lang, .. } = ctx;
    let files = RwSignal::new_local(FilesState::Off);
    let source = StoredValue::new_local(Rc::clone(&shown));
    let build = move |_: MouseEvent| {
        if matches!(files.get_untracked(), FilesState::Busy) {
            return;
        }
        files.set(FilesState::Busy);
        leptos::task::spawn_local(async move {
            let Some(shown) = source.try_get_value() else {
                return;
            };
            files.set(match make_files(ctx, shown).await {
                Ok(made) => FilesState::On(Rc::new(made), 0),
                Err(say) => FilesState::Failed(say),
            });
        });
    };
    let (qr, side, parts) = (shown.qr.clone(), shown.side, shown.parts.clone());
    let main = StoredValue::new((qr, side, parts));
    // Квитанция этого кода пришла (задача 123): круг стоит, а «Готово,
    // убрать код» — главной кнопкой.
    let accepted = RwSignal::new(false);
    let (skipped, skipped_bytes) = (shown.skipped, shown.skipped_bytes);
    // Поместятся ли файлы в серию кодов — оценкой, до неё (задача 124): 50
    // файлов в 127 МБ вели к кнопке серии, а она отказывала словами «130 161
    // КБ из 1 023» рядом со второй главной кнопкой (найдено вживую
    // 2026-11-25). Не поместятся — слова и копия файлом сразу, а серии нет.
    let fits = files_fit(skipped_bytes);
    // Файлов больше серии кодов (задача 124, пп. 18 и A5): ручная копия тем
    // же паролем — экран копии соберёт её ключом этого кода и откроется с ней
    // готовой.
    let as_backup = move |_: MouseEvent| {
        if let Some(key) = source.try_with_value(|shown| Rc::clone(&shown.key)) {
            ctx.backups.hand_off(key);
            ctx.router.go(Screen::Backup);
        }
    };
    view! {
        <MadeWords lang shown=Rc::clone(&shown) skipped=if fits { Key::SyncSkipped } else { Key::SyncSkippedLarge } />
        {move || match files.get() {
            FilesState::On(made, at) => {
                let total = made.parts.len();
                let part = &made.parts[at.min(total - 1)];
                let (qr, side, text) = (part.qr.clone(), part.side, part.text.clone());
                let step = move |to: usize| files.update(|state| {
                    if let FilesState::On(_, at) = state {
                        *at = to;
                    }
                });
                view! {
                    <h3>{move || {
                        let lang = lang.get();
                        fill(t(lang, Key::SyncFilesPart), &[&i18n::integer(lang, at as i64 + 1), &i18n::integer(lang, total as i64)])
                    }}</h3>
                    <p class="hint">{tr(lang, Key::SyncFilesHint)}</p>
                    // Квитанции у кода файлов нет: круг части стоит, когда
                    // она убрана с экрана.
                    <CodeShown lang seen qr side parts=text accepted=Signal::stored(false) hint=Key::SyncFilesQrHint carry />
                    <div class="actions">
                        <button type="button" class="secondary" disabled={at == 0} on:click=move |_| step(at.saturating_sub(1))>
                            <span class="label">{tr(lang, Key::SyncFilesPrev)}</span>
                        </button>
                        <button type="button" disabled={at + 1 >= total} on:click=move |_| step(at + 1)>
                            <span class="label">{tr(lang, Key::SyncFilesNext)}</span>
                        </button>
                    </div>
                    <button type="button" class="link" on:click=move |_| files.set(FilesState::Off)>
                        <span class="label">{tr(lang, Key::SyncFilesBack)}</span>
                    </button>
                }
                .into_any()
            }
            state => {
                let (qr, side, parts) = main.get_value();
                // Серия всё же отказала — размер разошёлся с оценкой: кнопки
                // серии больше нет, а слова спокойные — путь есть, кнопка над
                // ними (задача 124).
                let refused = match &state {
                    FilesState::Failed(say @ Say::FilesTooLarge(_)) => Some(say.clone()),
                    _ => None,
                };
                let series = fits && refused.is_none();
                let gave_up = refused.is_some();
                view! {
                    <CodeShown lang seen qr side parts accepted=accepted.into() hint=Key::SyncQrHint carry />
                    // Файлы — без отдельного шага (задача 124, п. 18): пришла
                    // квитанция «принято» — следующей главной кнопкой и словами,
                    // сколько их: код файлов, а не помещаются они в серию —
                    // копия файлом тем же паролем.
                    {(skipped > 0).then(|| view! {
                        {move || (accepted.get() && !gave_up).then(|| {
                            let key = if series { Key::SyncFilesAfter } else { Key::SyncFilesBeyond };
                            view! {
                                <p role="status">{move || {
                                    let lang = lang.get();
                                    fill(
                                        t(lang, key),
                                        &[
                                            &i18n::integer(lang, skipped as i64),
                                            &megabytes(lang, skipped_bytes),
                                            &megabytes(lang, MAX_SERIES_BYTES),
                                        ],
                                    )
                                }}</p>
                            }
                        })}
                        {refused.clone().map(|say| view! { <p role="status">{move || say.text(lang.get())}</p> })}
                        <div class="actions">
                            {if series {
                                view! {
                                    <button
                                        type="button"
                                        class=move || (!accepted.get()).then_some("secondary")
                                        disabled=matches!(state, FilesState::Busy)
                                        on:click=build
                                    >
                                        <span class="label">{tr(lang, Key::SyncFilesMake)}</span>
                                    </button>
                                }
                                .into_any()
                            } else {
                                view! {
                                    <button
                                        type="button"
                                        class=move || (!accepted.get() && !gave_up).then_some("secondary")
                                        on:click=as_backup
                                    >
                                        <span class="label">{tr(lang, Key::SyncFilesAsBackup)}</span>
                                    </button>
                                }
                                .into_any()
                            }}
                        </div>
                    })}
                    {match state {
                        FilesState::Busy => Some(view! { <p role="status">{tr(lang, Key::SyncFilesMaking)}</p> }.into_any()),
                        FilesState::Failed(Say::FilesTooLarge(_)) | FilesState::Off | FilesState::On(..) => None,
                        FilesState::Failed(say) => Some(view! { <p role="alert">{move || say.text(lang.get())}</p> }.into_any()),
                    }}
                    <Reply ctx seen shown=source.get_value() gap accepted />
                }
                .into_any()
            }
        }}
        // Главная — «Готово», когда код приняли и больше передавать нечего:
        // файлы ждут — главная у них.
        <button type="button" class=move || (!accepted.get() || skipped > 0).then_some("link") on:click=move |_| close()>
            <span class="label">{tr(lang, Key::SyncClose)}</span>
        </button>
    }
}

/// Где квитанция у отправителя.
#[derive(Clone)]
enum Replying {
    Waiting,
    /// Камера на экране.
    Scanning,
    Reading,
    Read(Acked),
    /// Встречный код того устройства принят сюда (задача 124, п. 14).
    Took(Rc<Accepted>),
    Failed(Say),
}

/// Квитанция в ответ на показанный код (задача 116): снять камерой или
/// вставить текстом, расшифровать ключом кода и сказать, что она значит.
/// Дыра — день, с которого сделать новый код, встаёт в поле формы и в день
/// следующего кода этой базы (`Device::sync_made`). Квитанция этого кода —
/// «всё дошло», «впервые» или дыра — ставит `accepted` (задача 123): код
/// принят, и круг QR-кодов стоит; квитанция другого кода и нечитаемая его не
/// трогают — этот код, может быть, ещё не сняли. Здесь же снимают встречный
/// код того устройства (задача 124, п. 14): он под замком этого кода
/// и открывается его ключом, без пароля; его приём тоже ставит `accepted` —
/// того кода, не открыв этот, не сделать.
#[component]
fn Reply(ctx: Ctx, seen: Signal<bool>, shown: Rc<Shown>, gap: Callback<Date>, accepted: RwSignal<bool>) -> impl IntoView {
    let lang = ctx.lang;
    let replying = RwSignal::new_local(Replying::Waiting);
    let collector = StoredValue::new_local(Collector::default());
    // Сколько кадров встречного кода собрано из скольких; у квитанции кадр один.
    let progress = RwSignal::new((0usize, 0usize));
    let key = StoredValue::new_local(Rc::clone(&shown.key));
    let (created, base, since) = (shown.created, StoredValue::new(shown.base.clone()), shown.since);
    let pasted = RwSignal::new(String::new());
    let paste_failed = RwSignal::new(false);
    let paste_hint = RwSignal::new(None::<Key>);
    let reply_area = NodeRef::<leptos::html::Textarea>::new();
    // Ушли с экрана или с пути — камера гаснет.
    Effect::new(move |_| {
        if !seen.get() && replying.with_untracked(|state| matches!(state, Replying::Scanning)) {
            replying.set(Replying::Waiting);
        }
    });
    // Встречный код — принять ключом этого кода. Код под другим замком этим
    // ключом не откроется: он не в ответ на этот.
    let take_back = move |bytes: &[u8]| {
        let sealed = match sync::open(bytes) {
            Ok(sealed) => sealed,
            Err(error) => return replying.set(Replying::Failed(code_problem(error))),
        };
        replying.set(Replying::Reading);
        leptos::task::spawn_local(async move {
            let Some(key) = key.try_get_value() else {
                return;
            };
            replying.set(match accept(ctx, &sealed, Secret::Known(key), false).await {
                Ok((took, _)) => {
                    if !accepted.get_untracked() {
                        accepted.set(true);
                    }
                    pasted.set(String::new());
                    Replying::Took(Rc::new(took))
                }
                // Свой код этого устройства — код, а не ответ на него.
                Err(Say::Key(Key::SyncOwn)) => Replying::Failed(Say::Key(Key::SyncIsCode)),
                Err(Say::Key(Key::SyncWrongPassword)) => Replying::Failed(Say::Key(Key::SyncBackOther)),
                Err(say) => Replying::Failed(say),
            });
        });
    };
    let read = move |bytes: Vec<u8>| {
        let sealed = match sync::payload(&bytes) {
            Payload::Receipt => match sync::open_receipt(&bytes) {
                Ok(sealed) => sealed,
                Err(error) => return replying.set(Replying::Failed(receipt_problem(error))),
            },
            Payload::Code => return take_back(&bytes),
            Payload::Segment => return replying.set(Replying::Failed(Say::Key(Key::SyncIsCode))),
            Payload::Other => return replying.set(Replying::Failed(Say::Key(Key::SyncNotCode))),
        };
        replying.set(Replying::Reading);
        leptos::task::spawn_local(async move {
            let Some(key) = key.try_get_value() else {
                return;
            };
            let opened = crypto::decrypt(&key, sealed.iv(), sealed.ciphertext()).await;
            let receipt = match opened.map(|plain| Receipt::read(&plain)) {
                Ok(Ok(receipt)) => receipt,
                Ok(Err(error)) => return replying.set(Replying::Failed(receipt_problem(error))),
                Err(_) => return replying.set(Replying::Failed(Say::Key(Key::SyncReceiptDamaged))),
            };
            let device = Device::new();
            let acked = sync::acked(&receipt, device.sync_id(), created);
            if let Acked::Gap { got } = acked {
                let base = base.get_value();
                if device.sync_made(&base).is_none_or(|made| got < made) {
                    device.set_sync_made(&base, got);
                }
                if let Some(day) = clock::local_date(got) {
                    gap.run(day);
                }
            }
            match acked {
                Acked::Whole | Acked::First | Acked::Gap { .. } => {
                    if !accepted.get_untracked() {
                        accepted.set(true);
                    }
                    // Код сняли — это и есть обмен (ревью задачи 124): строка
                    // профиля «Последний обмен».
                    device.set_sync_last(clock::now());
                }
                Acked::Other => {}
            }
            pasted.set(String::new());
            replying.set(Replying::Read(acked));
        });
    };
    let take = move |found: Result<sync::Frame, FrameError>| {
        let frame = match found {
            Ok(frame) => frame,
            Err(FrameError::Newer) => return replying.set(Replying::Failed(Say::Key(Key::SyncNewer))),
            Err(FrameError::NotFrame) => return,
        };
        let (got, count, whole) = collector
            .try_update_value(|collector| {
                collector.take(frame);
                (collector.got(), collector.count(), collector.whole())
            })
            .unwrap_or((0, 0, None));
        progress.set((got, count));
        if let Some(bytes) = whole {
            collector.set_value(Collector::default());
            progress.set((0, 0));
            read(bytes);
        }
    };
    // Снято столько-то кадров встречного кода — как у «Принять сюда».
    let got_words = move || {
        let (got, count) = progress.get();
        let lang = lang.get();
        (count > 1).then(|| fill(t(lang, Key::SyncGot), &[&i18n::integer(lang, got as i64), &i18n::integer(lang, count as i64)]))
    };
    // Квитанция из буфера — в самом нажатии ([`paste_into`]).
    let paste_now = move |_: MouseEvent| {
        paste_into(reply_area, paste_hint, paste_failed, move |text| {
            pasted.set(text.clone());
            for found in sync::frames_in_text(&text) {
                take(found);
            }
        });
    };
    view! {
        <p class="hint">{tr(lang, Key::SyncReceiptAbout)}</p>
        {move || match replying.get() {
            Replying::Scanning => {
                let found = Callback::new(move |bytes: Vec<u8>| take(sync::frame(&bytes)));
                let stop = Callback::new(move |()| replying.set(Replying::Waiting));
                view! {
                    {Suspend::new(async move {
                        scanner_part(ScanView { lang, found, stop }).await.ok()
                    })}
                    <p role="status">{got_words}</p>
                }
                .into_any()
            }
            Replying::Reading => view! { <p role="status">{tr(lang, Key::SyncAccepting)}</p> }.into_any(),
            state => view! {
                <div class="actions">
                    <button type="button" class="secondary" on:click=move |_| replying.set(Replying::Scanning)>
                        <span class="label">{tr(lang, Key::SyncReceiptScan)}</span>
                    </button>
                    <button type="button" class="secondary" on:click=paste_now>
                        <span class="label">{tr(lang, Key::SyncPasteNow)}</span>
                    </button>
                </div>
                {move || paste_failed.get().then(|| view! { <p role="alert">{tr(lang, Key::SyncPasteFailed)}</p> })}
                {move || paste_hint.get().map(|key| view! { <p class="hint" role="status">{tr(lang, key)}</p> })}
                <label class="sync-paste sync-reply">
                    {tr(lang, Key::SyncReceiptPaste)}
                    <textarea
                        rows="2"
                        autocomplete="off"
                        spellcheck="false"
                        node_ref=reply_area
                        prop:value=move || pasted.get()
                        on:input=move |event| {
                            let text = event_target_value(&event);
                            paste_hint.set(None);
                            pasted.set(text.clone());
                            for found in sync::frames_in_text(&text) {
                                take(found);
                            }
                        }
                    ></textarea>
                </label>
                <p role="status">{got_words}</p>
                {match state {
                    Replying::Took(took) => Some(view! {
                        <p role="status">{tr(lang, Key::SyncBackTook)}</p>
                        <AcceptedWords lang accepted=took />
                    }.into_any()),
                    Replying::Read(Acked::Whole) => Some(view! { <p role="status">{tr(lang, Key::SyncReceiptWhole)}</p> }.into_any()),
                    Replying::Read(Acked::First) => Some(view! {
                        <p class="warning">{move || {
                            let lang = lang.get();
                            let day = since.map(|day| i18n::date(lang, day)).unwrap_or_default();
                            fill(t(lang, Key::SyncReceiptFirst), &[&day])
                        }}</p>
                    }.into_any()),
                    Replying::Read(Acked::Gap { got }) => Some(view! {
                        <p class="warning">{move || {
                            let lang = lang.get();
                            let day = clock::local_date(got).map(|day| i18n::date(lang, day)).unwrap_or_default();
                            fill(t(lang, Key::SyncReceiptGap), &[&day])
                        }}</p>
                    }.into_any()),
                    Replying::Read(Acked::Other) => Some(view! { <p role="alert">{tr(lang, Key::SyncReceiptOther)}</p> }.into_any()),
                    Replying::Failed(say) => Some(view! { <p role="alert">{move || say.text(lang.get())}</p> }.into_any()),
                    Replying::Waiting | Replying::Scanning | Replying::Reading => None,
                }}
            }
            .into_any(),
        }}
    }
}

/// «Вставить» у поля кода (задача 124): в Safari и Firefox — текст из буфера
/// в самом нажатии, до первого `await` (`platform::clipboard::paste`), в
/// Chromium — курсор в поле `area` и слова, как вставить самому (решение
/// человека 2026-11-25: там разрешение на чтение буфера остаётся за сайтом).
/// `got` — что вставили, `failed` — буфер не отдал текст, `hint` — слова для
/// Chromium.
fn paste_into(
    area: NodeRef<leptos::html::Textarea>,
    hint: RwSignal<Option<Key>>,
    failed: RwSignal<bool>,
    got: impl Fn(String) + 'static,
) {
    failed.set(false);
    let way = clipboard::paste_way();
    if way != Paste::Read {
        if let Some(area) = area.get_untracked() {
            let _ = area.focus();
        }
        hint.set(Some(match way {
            Paste::Keys { mac: true } => Key::SyncPasteCmd,
            Paste::Keys { mac: false } => Key::SyncPasteCtrl,
            Paste::Keyboard | Paste::Read => Key::SyncPasteKeyboard,
        }));
        return;
    }
    let read = clipboard::paste();
    leptos::task::spawn_local(async move {
        match read.await {
            Ok(text) => got(text),
            Err(()) => failed.set(true),
        }
    });
}

/// Ошибка квитанции — словами.
fn receipt_problem(error: CodeError) -> Say {
    Say::Key(match error {
        CodeError::NotCode => Key::SyncNotCode,
        CodeError::Newer => Key::SyncNewer,
        CodeError::Damaged(why) => {
            web_sys::console::error_1(&format!("Kvartik, квитанция: {why}").into());
            Key::SyncReceiptDamaged
        }
    })
}

/// Квитанция на принятый код — чтобы показать: QR-код и текст.
#[derive(Clone)]
struct ShownReceipt {
    qr: String,
    side: usize,
    text: String,
}

/// Квитанция на код с шапкой `header`, принятый с целостью `continuity`, —
/// под ключом кода, одним QR-кодом и одним куском текста. `None` — шифр
/// отказал: код принят, а квитанции нет — отправитель её просто не снимет.
async fn receipt(key: &crypto::Key, header: &sync::Header, continuity: Continuity) -> Option<ShownReceipt> {
    let receipt = Receipt::new(header, Device::new().sync_id(), continuity);
    let iv = crypto::random(IV_BYTES).ok()?;
    let ciphertext = crypto::encrypt(key, &iv, &receipt.plain()).await.ok()?;
    let sealed = sync::seal_receipt(&iv, &ciphertext);
    let code = sync::receipt_qr(&sealed, sync::transfer_id())?;
    Some(ShownReceipt {
        qr: qr_path(&code),
        side: code.side() + 2 * QUIET,
        text: sync::text_parts(&sealed, sync::transfer_id()).join("\n\n"),
    })
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
    /// Легло в пустую программу, а выпуск здесь не выключен: так заводят
    /// устройство — где выпускать.
    fresh: bool,
    /// Квитанция тому устройству (задача 116); у кода файлов и у встречного
    /// кода её нет.
    receipt: Option<ShownReceipt>,
    /// Легло в пустую программу: встречному коду (задача 124, п. 14) нечего
    /// нести — здесь только записи того устройства.
    was_empty: bool,
    /// Что ждало кода отсюда до этого кода — день встречного кода.
    unsent: Unsent,
    /// Записи кода, какими они пришли (задача 124, п. 14): встречный код
    /// их обратно не несёт, пока они здесь те же.
    echo: Rc<Echo>,
    /// Код был кодом всех ИП: встречный — тоже, а на код одного ИП — кодом
    /// одного ИП.
    everyone: bool,
}

/// Чем открыть код: паролем, который вписал человек, или ключом прошлого
/// кода того же пароля — код файлов идёт вслед за кодом изменений (задача 116).
enum Secret {
    Typed(String),
    Known(Rc<crypto::Key>),
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

/// Принять сюда: камера или текст — пароль — слияние. `seen` — путь на виду:
/// камера горит только тогда.
#[component]
fn Receive(ctx: Ctx, seen: Signal<bool>) -> impl IntoView {
    let Ctx { lang, forms, .. } = ctx;
    let receiving = RwSignal::new_local(Receiving::Idle);
    let collector = StoredValue::new_local(Collector::default());
    // Части кода файлов (задача 116): сколько собрано из скольких.
    let series = StoredValue::new_local(Series::default());
    let series_progress = RwSignal::new((0usize, 0usize));
    // Ключ последнего принятого кода — с солью и итерациями его замка: код
    // файлов того же пароля откроется без второго ввода, а встречный код
    // (задача 124, п. 14) закроется им же.
    let known = StoredValue::new_local(None::<CodeLock>);
    // Сколько кадров собрано из скольких — для слов; (0, 0) — ни одного.
    let progress = RwSignal::new((0usize, 0usize));
    let pasted = RwSignal::new(String::new());
    let paste_failed = RwSignal::new(false);
    let paste_hint = RwSignal::new(None::<Key>);
    let code_area = NodeRef::<leptos::html::Textarea>::new();
    let password = RwSignal::new(String::new());
    let shown = RwSignal::new(false);
    forms.hold_while(move || {
        password.with(|text| !text.is_empty())
            || receiving.with(|state| matches!(state, Receiving::Busy | Receiving::Password(..) | Receiving::Scanning))
    });
    // Ушли с экрана или с пути — камера гаснет: сканер снимается.
    Effect::new(move |_| {
        if !seen.get() && receiving.with_untracked(|state| matches!(state, Receiving::Scanning)) {
            receiving.set(Receiving::Idle);
        }
    });

    // Собран код: тот же пароль, что у прошлого принятого, — принять его
    // ключом, иначе спросить пароль.
    let run = move |sealed: Rc<Sealed>, secret: Secret| {
        receiving.set(Receiving::Busy);
        leptos::task::spawn_local(async move {
            let known_key = matches!(secret, Secret::Known(_));
            let outcome = accept(ctx, &sealed, secret, true).await;
            if let Ok((_, key)) = &outcome {
                let lock = sealed.lock();
                known.set_value(Some(CodeLock {
                    key: Rc::clone(key),
                    salt: lock.salt().to_vec(),
                    iterations: lock.iterations(),
                }));
                password.set(String::new());
                pasted.set(String::new());
                progress.set((0, 0));
                series_progress.set((0, 0));
            }
            receiving.set(match outcome {
                Ok((accepted, _)) => Receiving::Done(Rc::new(accepted)),
                // Ключ прошлого кода не подошёл — спросить пароль, без слов
                // о неверном: человек его ещё не вписывал.
                Err(Say::Key(Key::SyncWrongPassword)) if known_key => Receiving::Password(sealed, None),
                // Пароль не подошёл — спросить снова, а не снимать код заново.
                Err(say @ Say::Key(Key::SyncWrongPassword)) => Receiving::Password(sealed, Some(say)),
                Err(say) => Receiving::Failed(say),
            });
        });
    };
    let open_with_known = move |sealed: Rc<Sealed>| {
        let lock = sealed.lock();
        let key = known.with_value(|known| {
            known
                .as_ref()
                .filter(|known| known.salt == lock.salt() && known.iterations == lock.iterations())
                .map(|known| Rc::clone(&known.key))
        });
        match key {
            Some(key) => {
                run(sealed, Secret::Known(key));
                Receiving::Busy
            }
            None => Receiving::Password(sealed, None),
        }
    };
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
            if sync::payload(&code) == Payload::Segment {
                // Часть кода файлов — в серию; камера снимает дальше.
                let taken = series.try_update_value(|series| {
                    let took = series.take(&code);
                    (took, series.got(), series.count(), series.whole())
                });
                match taken {
                    Some((Ok(_), got, count, whole)) => {
                        series_progress.set((got, count));
                        if let Some(code) = whole {
                            series.set_value(Series::default());
                            receiving.set(match sync::open_series(&code) {
                                Ok(sealed) => open_with_known(Rc::new(sealed)),
                                Err(error) => Receiving::Failed(code_problem(error)),
                            });
                        }
                    }
                    Some((Err(FrameError::Newer), ..)) => receiving.set(Receiving::Failed(Say::Key(Key::SyncNewer))),
                    Some((Err(FrameError::NotFrame), ..)) | None => {}
                }
                return;
            }
            receiving.set(match sync::payload(&code) {
                Payload::Receipt => Receiving::Failed(Say::Key(Key::SyncIsReceipt)),
                Payload::Code | Payload::Segment | Payload::Other => match sync::open(&code) {
                    Ok(sealed) => open_with_known(Rc::new(sealed)),
                    Err(error) => Receiving::Failed(code_problem(error)),
                },
            });
        }
    };

    let on_paste = move |text: String| {
        for found in sync::frames_in_text(&text) {
            take(found);
        }
    };
    // Код из буфера — в самом нажатии ([`paste_into`]): переслан себе
    // мессенджером, а вставлять жестом в поле на телефоне — три касания.
    let paste_now = move |_: MouseEvent| {
        paste_into(code_area, paste_hint, paste_failed, move |text| {
            pasted.set(text.clone());
            on_paste(text);
        });
    };

    let submit = move |event: SubmitEvent| {
        conceal(&event, shown);
        event.prevent_default();
        let Receiving::Password(sealed, _) = receiving.get_untracked() else {
            return;
        };
        run(sealed, Secret::Typed(password.get_untracked()));
    };

    let restart = move || {
        collector.set_value(Collector::default());
        series.set_value(Series::default());
        series_progress.set((0, 0));
        progress.set((0, 0));
        pasted.set(String::new());
        password.set(String::new());
        receiving.set(Receiving::Idle);
    };
    let got_words = move || {
        let (got, count) = progress.get();
        let (files, parts) = series_progress.get();
        let lang = lang.get();
        let mut words = Vec::new();
        if parts > 0 {
            words.push(fill(t(lang, Key::SyncFilesGot), &[&i18n::integer(lang, files as i64), &i18n::integer(lang, parts as i64)]));
        }
        if count > 0 {
            words.push(fill(t(lang, Key::SyncGot), &[&i18n::integer(lang, got as i64), &i18n::integer(lang, count as i64)]));
        }
        (!words.is_empty()).then(|| words.join(" "))
    };

    view! {
        {move || match receiving.get() {
            // Принят — форма сменяется итогом: так менеджер паролей видит, что
            // она отправлена, и подставит тот же пароль копий (`ui/password.rs`).
            Receiving::Password(_, problem) => view! {
                <form class="sync" on:submit=submit>
                    <PasswordLogin />
                    <PasswordInput lang label=Key::SyncPassword value=password shown />
                    <PasswordShow lang shown />
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
                    <button type="button" class="secondary" on:click=paste_now>
                        <span class="label">{tr(lang, Key::SyncPasteNow)}</span>
                    </button>
                </div>
                {move || paste_failed.get().then(|| view! { <p role="alert">{tr(lang, Key::SyncPasteFailed)}</p> })}
                {move || paste_hint.get().map(|key| view! { <p class="hint" role="status">{tr(lang, key)}</p> })}
                <label class="sync-paste">
                    {tr(lang, Key::SyncPaste)}
                    <textarea
                        rows="3"
                        autocomplete="off"
                        spellcheck="false"
                        node_ref=code_area
                        prop:value=move || pasted.get()
                        on:input=move |event| {
                            let text = event_target_value(&event);
                            paste_hint.set(None);
                            pasted.set(text.clone());
                            on_paste(text);
                        }
                    ></textarea>
                </label>
                <p role="status">{got_words}</p>
                {match state {
                    // Под квитанцией — встречный код (задача 124, п. 14): в
                    // пустую программу легли только записи того устройства,
                    // и нести обратно нечего.
                    Receiving::Done(accepted) => {
                        let lock = (accepted.receipt.is_some() && !accepted.was_empty).then(|| known.get_value()).flatten();
                        let unsent = accepted.unsent;
                        let echo = Rc::clone(&accepted.echo);
                        let everyone = accepted.everyone;
                        Some(view! {
                            <AcceptedWords lang accepted />
                            {lock.map(|lock| view! { <BackCode ctx seen lock unsent echo everyone /> })}
                        }.into_any())
                    }
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
        // Что легло — по видам (задача 124, п. 21).
        <p role="status">{move || restored_words(lang.get(), &restored, None)}</p>
        {flow}
        {accepted.ahead.then(|| view! { <p class="warning">{tr(lang, Key::BackupAhead)}</p> })}
        {(accepted.fresh && restored.added > 0).then(|| view! { <IssuerAsk lang /> })}
        {accepted.receipt.clone().map(|receipt| view! { <ReceiptShown lang receipt /> })}
    }
}

/// Квитанция на принявшем устройстве: QR-код и текст со «Скопировать».
#[component]
fn ReceiptShown(lang: ReadSignal<Lang>, receipt: ShownReceipt) -> impl IntoView {
    let side = receipt.side;
    let (text, shown) = (receipt.text.clone(), receipt.text);
    let copied = RwSignal::new(None::<bool>);
    // В буфер — в самом нажатии, до первого `await` (Правило 6).
    let copy = move |_: MouseEvent| {
        let written = clipboard::copy(&text);
        leptos::task::spawn_local(async move {
            copied.set(Some(written.await.is_ok()));
        });
    };
    view! {
        <h3>{tr(lang, Key::SyncReceiptTitle)}</h3>
        <p>{tr(lang, Key::SyncReceiptShow)}</p>
        <figure class="sync-qr">
            <svg viewBox=format!("0 0 {side} {side}") role="img" aria-label=move || t(lang.get(), Key::SyncReceiptQrLabel)>
                <rect class="sync-qr-paper" width=side height=side />
                <path class="sync-qr-ink" d=receipt.qr />
            </svg>
        </figure>
        <div class="actions">
            <button type="button" class="secondary" on:click=copy>
                {icon(Icon::Copy)}
                <span class="label">{tr(lang, Key::SyncReceiptCopy)}</span>
            </button>
        </div>
        {move || copied.get().map(|done| view! {
            <p role=if done { "status" } else { "alert" }>
                {tr(lang, if done { Key::SyncCopied } else { Key::SyncCopyFailed })}
            </p>
        })}
        <details class="sync-text sync-receipt">
            <summary>{tr(lang, Key::SyncReceiptText)}</summary>
            <textarea readonly rows="3" prop:value=shown></textarea>
        </details>
    }
}

/// Встречный код (задача 124, п. 14): под квитанцией — изменения отсюда тому
/// устройству тем же ключом, без пароля, по нажатию, без записей, что пришли
/// от него (`echo`). Снимает его тот, кто показал код, — там же, где
/// квитанцию (`Reply`). Квитанции на него нет: круг идёт, пока путь на виду.
/// Больше предела — копией файлом: дня у встречного кода не выбрать.
#[component]
fn BackCode(ctx: Ctx, seen: Signal<bool>, lock: CodeLock, unsent: Unsent, echo: Rc<Echo>, everyone: bool) -> impl IntoView {
    let lang = ctx.lang;
    let back = RwSignal::new_local(Sending::Idle);
    let lock = StoredValue::new_local(lock);
    let echo = StoredValue::new_local(echo);
    // Копией файлом — тем же паролем, если он здесь тот же (`Backups::hand_off`):
    // экран копии соберёт её сам.
    let as_backup = move |_: MouseEvent| {
        if let Some(key) = lock.try_with_value(|lock| Rc::clone(&lock.key)) {
            ctx.backups.hand_off(key);
        }
        ctx.router.go(Screen::Backup);
    };
    let start = move |_: MouseEvent| {
        if matches!(back.get_untracked(), Sending::Busy) {
            return;
        }
        back.set(Sending::Busy);
        leptos::task::spawn_local(async move {
            let (Some(lock), Some(echo)) = (lock.try_get_value(), echo.try_get_value()) else {
                return;
            };
            back.set(match make_back(ctx, &lock, unsent, &echo, everyone).await {
                Ok(shown) => Sending::Ready(Rc::new(shown)),
                Err(say) => Sending::Failed(say),
            });
        });
    };
    view! {
        {move || match back.get() {
            Sending::Ready(shown) => {
                let (qr, side, parts) = (shown.qr.clone(), shown.side, shown.parts.clone());
                view! {
                    <h3>{tr(lang, Key::SyncBackTitle)}</h3>
                    <MadeWords lang shown skipped=Key::SyncBackSkipped />
                    <CodeShown lang seen qr side parts accepted=Signal::stored(false) hint=Key::SyncBackHint carry=false />
                }
                .into_any()
            }
            state => view! {
                // Не уместился — снова тот же код не уместится: главная кнопка
                // одна, копия файлом.
                {(!matches!(state, Sending::Failed(Say::TooLarge(_)))).then(|| view! {
                    <div class="actions">
                        <button type="button" disabled=matches!(state, Sending::Busy) on:click=start>
                            <span class="label">{tr(lang, Key::SyncBackMake)}</span>
                        </button>
                    </div>
                })}
                {match state {
                    Sending::Busy => Some(view! { <p role="status">{tr(lang, Key::SyncMaking)}</p> }.into_any()),
                    // «Передавать нечего» — ответ, а не беда, как у «Передать отсюда».
                    Sending::Failed(say @ Say::Key(Key::SyncNothing)) => {
                        Some(view! { <p role="status">{move || say.text(lang.get())}</p> }.into_any())
                    }
                    // Больше предела (задача 124): «выберите день позже» здесь
                    // не годится — дня у встречного кода нет, — путь один,
                    // копия файлом.
                    Sending::Failed(Say::TooLarge(bytes)) => Some(view! {
                        <p role="alert">{move || {
                            let lang = lang.get();
                            fill(
                                t(lang, Key::SyncBackTooLarge),
                                &[
                                    &i18n::integer(lang, bytes.div_ceil(1024) as i64),
                                    &i18n::integer(lang, (MAX_CODE_BYTES / 1024) as i64),
                                ],
                            )
                        }}</p>
                        <div class="actions">
                            <button type="button" on:click=as_backup>
                                <span class="label">{tr(lang, Key::SyncFilesAsBackup)}</span>
                            </button>
                        </div>
                    }.into_any()),
                    Sending::Failed(say) => Some(view! { <p role="alert">{move || say.text(lang.get())}</p> }.into_any()),
                    Sending::Idle | Sending::Ready(_) => None,
                }}
            }
            .into_any(),
        }}
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

/// Расшифровать код и слить с базой — или сказать, почему нельзя. Вместе
/// с принятым — ключ кода: код файлов того же пароля откроется им. `answer` —
/// показать квитанцию в ответ; встречный код (задача 124, п. 14) принимают
/// без неё: снять её там негде, и ряд встречного кода там не двигается.
async fn accept(ctx: Ctx, sealed: &Sealed, secret: Secret, answer: bool) -> Result<(Accepted, Rc<crypto::Key>), Say> {
    let opened = match secret {
        Secret::Typed(typed) => stored_sync::unlock(sealed, &crypto::normalize(&typed))
            .await
            .map(|(code, key)| (code, Rc::new(key))),
        Secret::Known(key) => stored_sync::unlock_with(sealed, &key).await.map(|code| (code, key)),
    };
    let (code, key) = match opened {
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
    // Часы отправителя впереди — и тогда, когда записи кода старые: момент
    // самого кода позже «сейчас» (Правило 6).
    let now = clock::now();
    let ahead = backup::ahead(code.entries(), now)
        || header.created().unix_millis() > now.unix_millis().saturating_add(backup::CLOCK_SLACK_MILLIS);
    let had_office = device.office();
    // Какими записи пришли — до слияния: встречный код их не вернёт.
    let echo = Rc::new(Echo::of(code.entries()));
    let everyone = backup::is_full(code.entries());
    // До слияния: что здесь ждало кода отсюда (`Unsent`).
    let unsent = Unsent {
        open: device.sync_arrived(&ctx.office.open_name()),
        office: device.sync_arrived(OFFICE_DB),
    };
    let (landed, fresh, base) = if everyone {
        land_all(ctx, &store, code.into_entries(), header.since(), header.files()).await?
    } else {
        land_one(ctx, &store, code.into_entries(), header.since(), since, header.files()).await?
    };
    let mut received = device.sync_received(&base);
    // Код файлов ряд изменений не продолжает и не рвёт (задача 116): о нём
    // ни дыры, ни отметки, ни квитанции.
    let files = header.files();
    let got = (!files).then(|| received.until(header.device())).flatten();
    let continuity = if files { Continuity::Whole } else { sync::continuity(header.since(), got) };
    // Ряд цел — запомнить, до какого момента его часов от него принято;
    // с дырой — нет: следующий код с того же дня скажет о ней снова, пока
    // её не закроют.
    // Момент — не позже «сейчас» этого устройства: часы отправителя, убежавшие
    // вперёд, иначе сделали бы целым любой его следующий код, пока настоящее
    // время их не догонит (ревью задачи 110).
    if !files && !matches!(continuity, Continuity::Gap { .. }) {
        received.note(header.device(), header.created().min(now));
        device.set_sync_received(&base, &received);
    }
    // Строка профиля «Последний обмен» (задача 124) — и о коде файлов.
    device.set_sync_last(now);
    // Своего пароля у устройства нет — берётся пароль кода: человек только что
    // показал, что знает его, а коды отсюда пойдут под ним же туда. Только
    // у устройства, которое не было конторой, как у копии: у конторы пароль
    // клиента открыл бы её копию всех ИП (ревью задачи 110).
    // И только с полным числом итераций: код с одной итерацией иначе оставил
    // бы устройству слабую проверку пароля (ревью задачи 110).
    let lock = sealed.lock();
    if device.backup_key().is_none()
        && !had_office
        && lock.iterations() >= KDF_ITERATIONS
        && let Some(opened_with) = DeviceKey::new(lock.iterations(), lock.salt().to_vec(), key.check().to_vec())
    {
        ctx.backups.password_set(&opened_with);
        ctx.backups.password_entered();
    }
    let receipt = if files || !answer { None } else { receipt(&key, &header, continuity).await };
    Ok((
        Accepted {
            restored: landed,
            continuity,
            since,
            got: got.and_then(sync::day_covering),
            ahead,
            // Выпуск уже выключен — спрашивать не о чем; код файлов программу не
            // заводит.
            fresh: !files && fresh && !super::settings::issue_elsewhere_untracked(store.name()),
            receipt,
            was_empty: fresh,
            unsent,
            echo,
            everyone,
        },
        key,
    ))
}

/// Код одного ИП — в открытую базу, как копия одного ИП: что легло, была ли
/// база пуста и под каким именем помнить принятое (имя открытой базы).
async fn land_one(
    ctx: Ctx,
    store: &Store,
    entries: Vec<backup::Entry>,
    from: Option<kvartik_core::model::record::Timestamp>,
    since: Option<Date>,
    files: bool,
) -> Result<(Restored, bool, String), Say> {
    let device = Device::new();
    let failure = |error| Say::from(Problem::from(stored::Failure::from(error)));
    let tx = store.read().map_err(failure)?;
    let fresh = tx.is_empty().await.map_err(failure)?;
    // Изменения «с дня» ложатся, только если здесь данные того устройства:
    // есть его запись профиля — она идёт в каждом коде. Пустая программа,
    // профиль, заведённый здесь своим, — и остальное сюда не придёт никогда,
    // а ряд выглядел бы целым (ревью задачи 110). Профиль не читается —
    // сверять не с чем, и решает слияние, как у копии. Код файлов — так же
    // и со всеми записями: он идёт следом за кодом изменений, и в программу
    // без них лёг бы раньше них — пустая программа перестала бы быть пустой
    // и не спросила бы, где выпускать (ревью задачи 116).
    if from.is_some() || files {
        let theirs: Vec<_> = entries
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
            return Err(if files { Say::Key(Key::SyncFilesFirst) } else { Say::EmptyDelta(since) });
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
        let found = office_route::listed(&entries, &listed).map_err(|error| Say::from(Problem::from(stored::newer(error))))?;
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
    let restored = match stored::restore(store, KINDS, &entries, OtherIe::Ask).await {
        Ok(Merged::Done(restored)) => restored,
        Ok(Merged::Other { ours, theirs }) => return Err(Say::OtherIe(Box::new((ours, theirs)))),
        Err(failure) => return Err(Problem::from(failure).into()),
    };
    if restored.added + restored.updated > 0 {
        ctx.base.replaced();
    }
    super::backup::arrived(&base, restored.oldest);
    if !entries.is_empty() {
        ctx.backups.data_saved();
    }
    Ok((restored, fresh, base))
}

/// Код всех ИП (задача 116) — маршрутом копии всех ИП: по ИП в базы этого
/// устройства, списка не было — он заводится (`store::backup::restore_all`).
/// Код «с дня» (`from`) с ИП, которого здесь нет и который не новый, не пишет
/// ничего: изменения такого ИП легли бы новым ИП без остального его
/// (`format::office::missing`). Принятое помнится под именем базы конторы.
async fn land_all(
    ctx: Ctx,
    store: &Store,
    entries: Vec<backup::Entry>,
    from: Option<kvartik_core::model::record::Timestamp>,
    files: bool,
) -> Result<(Restored, bool, String), Say> {
    let failure = |error: stored::Failure| Say::from(Problem::from(error));
    let office_store = ctx.office.store().await.map_err(|error| failure(error.into()))?;
    let open_name = ctx.office.open_name();
    let (first, spare) = super::backup::unlisted(&office_store, store, &open_name).await.map_err(failure)?;
    // Код «с дня» и код файлов — только туда, где данные каждого его ИП уже
    // есть (`format::office::missing`; код файлов — следом за кодом изменений).
    if from.is_some() || files {
        let mut local = office_list(&office_store).await.map_err(|error| failure(error.into()))?.records;
        local.extend(first.clone());
        let since = if files { None } else { from };
        if let Some(ip) = office_route::missing(&entries, &local, since).map_err(|error| failure(stored::newer(error)))? {
            if files {
                return Err(Say::Key(Key::SyncFilesFirst));
            }
            let lang = ctx.lang.get_untracked();
            let name = match ip.name(lang) {
                "" => t(lang, Key::OfficeNameless).to_owned(),
                name => name.to_owned(),
            };
            return Err(Say::MissingIp(name));
        }
    }
    let done = stored::restore_all(store, &open_name, &office_store, entries, first, spare.as_deref())
        .await
        .map_err(failure)?;
    for (base, oldest) in &done.landed {
        super::backup::arrived(base, Some(*oldest));
    }
    // Список уже записан — и тогда, когда база не легла: перечитать его
    // и тронутое (как у копии всех ИП).
    ctx.office.several_now();
    ctx.office.refresh(&office_store).await;
    if done.open_touched {
        ctx.base.replaced();
        ctx.backups.data_saved();
    }
    if let Some(failed) = done.failed {
        return Err(Say::Backup(match Problem::from(failed) {
            Problem::Say(why) => Problem::Partly(why),
            _ => Problem::Partly(Key::BackupFailed),
        }));
    }
    Ok((done.restored, spare.is_some(), OFFICE_DB.to_owned()))
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
