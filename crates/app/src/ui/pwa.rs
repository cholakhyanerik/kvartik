//! Программа как приложение (задача 8): новая версия и раздел «Установка».
//!
//! Новая версия включается сама (решение человека 2026-11-01): так у всех
//! последняя версия — с последними правилами налога. Страница при этом
//! перезагружается, поэтому пока открыта форма или начато что-то ещё
//! ([`Forms`]), версия ждёт, а над экраном — слова (решение человека
//! 2026-10-18): начатое не пропадает. Что в ней нового, окно говорит уже
//! после, окном поверх программы (задача 67) — по записи устройства о версии,
//! которую человек видел.
//!
//! Страницу здесь перезагружает код, а не нажатие. Поэтому включить версию
//! окно пробует один раз на устройстве (`Device::update_tried`): сборка,
//! которая по какой-то причине не заработала, иначе перезагружала бы страницу
//! без конца, и открыть программу было бы нельзя. Такую версию браузер
//! включит сам, когда закроют все вкладки.
//!
//! Установка — кнопкой «Установить» всегда, пока программа не установлена
//! (задача 55): там, где браузер даёт свой вопрос (Chromium), она задаёт
//! его, а где нет — раскрывает слова, как установить в этом браузере
//! руками. На iPhone, iPad и Mac от установки зависит, увидит ли программа
//! данные (Правило 6), поэтому раздел стоит выше разделов о данных.

use std::sync::Arc;

use kvartik_core::i18n::{self, Key, Lang, fill, t};
use kvartik_core::release::{Notes, Version};
use leptos::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{HtmlElement, KeyboardEvent};

use super::form::Forms;
use super::tr;
use crate::platform::pwa::{self as browser, Browser, Event, Pwa};
use crate::store::device::Device;

/// Новая версия программы.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NewVersion {
    /// Работает последняя известная версия.
    Current,
    /// Новая версия скачана и ждёт включения.
    Ready,
    /// Включаем: страница вот-вот перезагрузится.
    Applying,
    /// Новую версию включили в другой вкладке: эта страница — ещё старая.
    Elsewhere,
}

impl NewVersion {
    /// Браузер сообщил о новой версии. «Включили в другой вкладке» сильнее
    /// «ждёт включения»: ждущая версия уже работает, и включать здесь нечего
    /// — нужна перезагрузка.
    pub fn apply(&mut self, event: Event) {
        match event {
            Event::UpdateReady => {
                if *self == Self::Current {
                    *self = Self::Ready;
                }
            }
            Event::UpdatedElsewhere => {
                if *self != Self::Applying {
                    *self = Self::Elsewhere;
                }
            }
            Event::CanInstall | Event::Installed | Event::Serving => {}
        }
    }

    /// Новая версия пришла и ждёт, когда её включат или перезагрузят
    /// страницу.
    const fn waits(self) -> bool {
        matches!(self, Self::Ready | Self::Elsewhere)
    }

    /// Пора ли перейти на пришедшую версию: `target` — версия новее работающей
    /// по заметкам, `tried` — та, которую на устройстве уже пробовали
    /// включить. Заметок новее нет — пришла прежняя сборка: хостинг ещё
    /// минуты после выкладки отдаёт часть ответов прежней (Правило 6),
    /// и «обновление» откатило бы программу назад, — или та же, или заметки
    /// не прочитались, и что пришло, неизвестно. Новую сборку найдёт
    /// следующая проверка, а не знакомую браузер включит сам, когда закроют
    /// все вкладки. Включённая в другой вкладке уже работает: здесь только
    /// перезагрузка, а она второй раз сама не случится.
    fn due(self, target: Option<Version>, tried: Option<Version>) -> bool {
        match self {
            Self::Current | Self::Applying => false,
            Self::Ready => target.is_some() && target != tried,
            Self::Elsewhere => target.is_some(),
        }
    }
}

/// Что показать в окне «Что нового»: заметки на устройстве и версия, о которой
/// человек уже знает.
#[derive(Clone)]
struct Updated {
    seen: Version,
    notes: Arc<Notes>,
}

/// Что предложить про установку. Кнопка «Установить» есть всегда, пока
/// программа не установлена (решение человека 2026-10-20, задача 55):
/// вопрос браузера бывает только у Chromium, а и там его нет, пока
/// программа уже стоит или человек недавно отказался.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Install {
    /// Программа установлена или открыта установленной: предлагать нечего,
    /// что бы ни говорил браузер.
    Installed,
    /// Вопроса браузера нет: кнопка раскрывает слова, как установить
    /// программу в этом браузере руками.
    Manual,
    /// Браузер даёт установить своим вопросом — его и задаёт кнопка.
    Button,
    /// Вопрос браузера на экране — ждём ответа человека.
    Asking,
}

impl Install {
    /// С чего начать: открыта ли программа установленной.
    pub const fn initial(installed: bool) -> Self {
        if installed { Self::Installed } else { Self::Manual }
    }

    /// Браузер сообщил об установке.
    pub fn apply(&mut self, event: Event) {
        match event {
            Event::CanInstall => {
                if *self == Self::Manual {
                    *self = Self::Button;
                }
            }
            Event::Installed => *self = Self::Installed,
            Event::UpdateReady | Event::UpdatedElsewhere | Event::Serving => {}
        }
    }

    /// Человек ответил браузеру; `again` — браузер уже прислал новый вопрос
    /// ([`Pwa::can_ask`]). Отказался — кнопка снова задаёт вопрос, а нет
    /// его — раскрывает слова; согласился — установку подтвердит событие
    /// `Installed`, до или после ответа.
    pub fn answered(&mut self, again: bool) {
        if *self == Self::Asking {
            *self = if again { Self::Button } else { Self::Manual };
        }
    }
}

/// Слова раздела «Установка» для браузера: над кнопкой, как установить
/// руками и что помнить о данных.
#[derive(Clone, Copy)]
struct Words {
    about: Key,
    steps: Key,
    hint: Key,
}

impl Words {
    const fn of(browser: Browser) -> Self {
        let (about, steps) = match browser {
            Browser::SafariTouch => (Key::InstallFirst, Key::InstallSteps),
            Browser::OtherTouch => (Key::InstallFirst, Key::InstallStepsIosOther),
            Browser::SafariMac => (Key::InstallFirstMac, Key::InstallStepsMac),
            Browser::Chrome => (Key::InstallAbout, Key::InstallStepsChrome),
            Browser::ChromeAndroid => (Key::InstallAbout, Key::InstallStepsChromeAndroid),
            Browser::Edge => (Key::InstallAbout, Key::InstallStepsEdge),
            Browser::EdgeAndroid => (Key::InstallAbout, Key::InstallStepsEdgeAndroid),
            Browser::Samsung => (Key::InstallAbout, Key::InstallStepsSamsung),
            Browser::FirefoxAndroid => (Key::InstallAbout, Key::InstallStepsFirefoxAndroid),
            Browser::FirefoxWindows => (Key::InstallAbout, Key::InstallStepsFirefoxWindows),
            Browser::Firefox => (Key::InstallAbout, Key::InstallElsewhere),
            Browser::Other => (Key::InstallAbout, Key::InstallStepsOther),
        };
        // Что установленная программа видит данные вкладки, известно по
        // исходникам Chromium и Firefox (разведка задачи 55, 2026-10-20).
        // На iPhone, iPad и Mac не видит, у Samsung Internet, Edge на
        // Android и незнакомых браузеров — неизвестно, а из другого браузера
        // данные не видны точно: туда — копией. Восстановление сливает
        // записи (Правило 2), и лишняя копия ничего не задвоит.
        let hint = match browser {
            Browser::Chrome
            | Browser::ChromeAndroid
            | Browser::Edge
            | Browser::FirefoxAndroid
            | Browser::FirefoxWindows => Key::InstallAlready,
            Browser::SafariTouch
            | Browser::OtherTouch
            | Browser::SafariMac
            | Browser::EdgeAndroid
            | Browser::Samsung
            | Browser::Firefox
            | Browser::Other => Key::InstallMoveData,
        };
        Self { about, steps, hint }
    }
}

/// Слова об установке руками в этом браузере и подсказка о данных —
/// браузерным тестам окна (`ui/tests/install.rs`).
#[cfg(all(test, target_arch = "wasm32"))]
pub(super) const fn manual_words(browser: Browser) -> (Key, Key) {
    let words = Words::of(browser);
    (words.steps, words.hint)
}

/// Что окно знает о программе как приложении — одно на все разделы.
#[derive(Clone, Copy)]
pub struct Program {
    version: RwSignal<NewVersion>,
    /// Заметки пришедшей версии; `None` — ещё читаются или не прочитались.
    news: RwSignal<Option<Arc<Notes>>>,
    /// Что нового в заработавшей версии; `None` — окна «Что нового» нет.
    updated: RwSignal<Option<Updated>>,
    install: RwSignal<Install>,
    /// Где открыта страница — чьими словами объяснять установку руками.
    browser: Browser,
    /// Раскрыты ли слова об установке руками.
    steps: RwSignal<bool>,
    /// Страницей правит service worker, и весь сайт — в его кэше
    /// ([`Event::Serving`]).
    serving: RwSignal<bool>,
    pwa: StoredValue<Pwa, LocalStorage>,
}

impl Program {
    /// Зарегистрировать service worker и слушать браузер.
    pub fn new() -> Self {
        let version = RwSignal::new(NewVersion::Current);
        let news = RwSignal::new(None);
        let updated = RwSignal::new(None);
        let install = RwSignal::new(Install::initial(browser::installed()));
        let serving = RwSignal::new(false);
        read_updated(updated);
        let pwa = Pwa::start(move |event| {
            // Подписчикам — только перемена: событие установки или правки
            // страницы service worker'ом новой версии ничего не меняет.
            version.maybe_update(|state| {
                let before = *state;
                state.apply(event);
                *state != before
            });
            // Заметки — заново при каждой вести о новой версии: пока ждала
            // одна, могла прийти ещё новее, и её заметки новее.
            if matches!(event, Event::UpdateReady | Event::UpdatedElsewhere)
                && version.get_untracked().waits()
            {
                read_news(news);
            }
            install.update(|state| state.apply(event));
            if event == Event::Serving {
                serving.set(true);
            }
        });
        Self {
            version,
            news,
            updated,
            install,
            browser: Browser::detect(),
            steps: RwSignal::new(false),
            serving,
            pwa: StoredValue::new_local(pwa),
        }
    }

    /// Ставить ли программу до данных: на iPhone, iPad и Mac установленная
    /// не видит данных браузера (Правило 6), а здесь она ещё не установлена.
    pub fn install_first(&self) -> bool {
        self.browser.install_first() && self.install.get_untracked() != Install::Installed
    }

    /// Та же программа в другом браузере — браузерным тестам окна: настоящий
    /// там всегда один (`ui/tests/onboarding.rs`).
    #[cfg(all(test, target_arch = "wasm32"))]
    pub(super) fn in_browser(self, browser: Browser) -> Self {
        Self { browser, ..self }
    }

    /// Правит ли страницей service worker — отслеживаемое: части wasm
    /// окно загружает заранее, только когда они придут из его кэша.
    pub fn serving(&self) -> bool {
        self.serving.get()
    }

    /// Стоит ли на экране окно «Что нового» — отслеживаемое: программа под
    /// ним неактивна (`ui/mod.rs`).
    pub fn blocks(&self) -> bool {
        self.updated.with(Option::is_some)
    }

    /// Версия, на которую пора перейти ([`NewVersion::due`]), — отслеживаемое.
    fn due(&self) -> Option<Version> {
        let target = self
            .news
            .with(|notes| notes.as_deref().and_then(Notes::newest))
            .filter(|newest| *newest > Version::CURRENT);
        let tried = Device::new().update_tried();
        self.version.get().due(target, tried).then_some(target).flatten()
    }

    /// Слова над экраном о новой версии — отслеживаемое: она включается, или
    /// её держит открытая форма.
    fn notice(&self, forms: Forms) -> Option<Key> {
        if self.version.get() == NewVersion::Applying {
            return Some(Key::UpdateApplying);
        }
        (forms.any() && self.due().is_some()).then_some(Key::UpdateAfterForm)
    }

    /// Перейти на пришедшую версию, как только её не держит ни одна форма
    /// (шапка модуля). Попытка запоминается до перезагрузки, а не запомнилась
    /// — перезагрузки нет: без записи счесть попытки нечем. Программа тогда
    /// работает дальше, а версию браузер включит сам, когда закроют все
    /// вкладки.
    pub fn update_when_free(self, forms: Forms) {
        Effect::new(move |_| {
            if forms.any() {
                return;
            }
            if let Some(target) = self.due()
                && Device::new().set_update_tried(target)
            {
                self.version.set(NewVersion::Applying);
                self.pwa.with_value(Pwa::update);
            }
        });
    }

    /// Окно «Что нового» прочитано: о работающей версии человек теперь знает.
    fn dismiss(&self) {
        Device::new().set_seen_version(Version::CURRENT);
        self.updated.set(None);
    }

    /// «Установить»: задать вопрос браузера, а нет его — раскрыть или
    /// спрятать слова, как установить руками.
    fn press_install(&self) {
        match self.install.get_untracked() {
            Install::Button => {
                // Отказался — раздел снова закрыт, а не раскрыт словами,
                // открытыми до вопроса.
                self.steps.set(false);
                self.install.set(Install::Asking);
                let pwa = self.pwa.get_value();
                let install = self.install;
                leptos::task::spawn_local(async move {
                    pwa.install().await;
                    install.update(|state| state.answered(pwa.can_ask()));
                });
            }
            Install::Manual => self.steps.update(|open| *open = !*open),
            Install::Asking | Install::Installed => {}
        }
    }

    /// Как будто браузер сообщил о новой версии, а её заметки — `notes`:
    /// браузерным тестам окна (`ui/tests/update.rs`) — service worker есть
    /// только в выпускной сборке.
    #[cfg(all(test, target_arch = "wasm32"))]
    pub(super) fn pretend(&self, event: Event, notes: Option<Notes>) {
        self.version.update(|state| state.apply(event));
        self.news.set(notes.map(Arc::new));
    }

    /// Как будто версия уже включается: браузерным тестам окна — настоящее
    /// включение перезагрузило бы страницу тестов.
    #[cfg(all(test, target_arch = "wasm32"))]
    pub(super) fn pretend_applying(&self) {
        self.version.set(NewVersion::Applying);
    }

    /// Как будто заработала версия новее `seen`, а на устройстве заметки
    /// `notes`: браузерным тестам окна.
    #[cfg(all(test, target_arch = "wasm32"))]
    pub(super) fn pretend_updated(&self, seen: Version, notes: Notes) {
        self.updated.set(Some(Updated {
            seen,
            notes: Arc::new(notes),
        }));
    }
}

/// Самый новый файл заметок на устройстве — из кэшей service worker, а не из
/// сети: кэш полон, как только сборка установилась, и без сети тоже. Файлов
/// бывает несколько — работающей сборки и пришедшей, — и новее тот, где
/// версия больше. `None` — кэшей нет или ни один файл не читается.
async fn notes_on_device() -> Option<Arc<Notes>> {
    browser::cached_texts(browser::NOTES)
        .await
        .iter()
        .filter_map(|text| Notes::parse(text).ok())
        .max_by_key(Notes::newest)
        .map(Arc::new)
}

/// Прочитать заметки пришедшей версии, чтобы узнать, новее ли она
/// ([`NewVersion::due`]). Прочитанное прежде остаётся, пока не прочитано
/// новое.
fn read_news(news: RwSignal<Option<Arc<Notes>>>) {
    leptos::task::spawn_local(async move {
        news.set(notes_on_device().await);
    });
}

/// Заработала версия новее той, о которой человек знает, — поднять окно «Что
/// нового». Записи о ней нет — это первый запуск или переход с версии до
/// 0.0.63: о том, что в этой нового, сказало окно прежней. На обычном
/// открытии здесь только чтение записи: кэши читаются после обновления.
fn read_updated(updated: RwSignal<Option<Updated>>) {
    let Some(seen) = Device::new().seen_version() else {
        Device::new().set_seen_version(Version::CURRENT);
        return;
    };
    if seen >= Version::CURRENT {
        return;
    }
    leptos::task::spawn_local(async move {
        // Не прочитались — запись не тронута, и окно встанет при следующем
        // открытии.
        let Some(notes) = notes_on_device().await else {
            return;
        };
        if notes.between(seen, Version::CURRENT).next().is_some() {
            updated.set(Some(Updated { seen, notes }));
        } else {
            Device::new().set_seen_version(Version::CURRENT);
        }
    });
}

/// Окно «Что нового» на языке окна (задача 67): уже после обновления, поверх
/// всей программы — программа под ним неактивна (`inert`, `ui/mod.rs`).
/// Закрывают его «Понятно» или Escape; закрытое больше не встаёт, а не
/// закрытое встанет при следующем открытии. Пусто, пока обновления не было.
///
/// Рисуется заново, только когда окно встаёт или уходит (`Memo`): иначе
/// любая весть браузера пересобрала бы его, и фокус с кнопки прыгал бы на
/// заголовок.
#[component]
pub fn UpdateDialog(lang: ReadSignal<Lang>, program: Program) -> impl IntoView {
    let shown = Memo::new(move |_| program.blocks());
    move || shown.get().then(|| view! { <UpdateCard lang program /> })
}

/// Само окно. Фокус — на его заголовке, после кадра: к нему окно уже
/// в странице. Диктор называет окно, а Tab ведёт к кнопке; под окном фокусу
/// встать некуда.
#[component]
fn UpdateCard(lang: ReadSignal<Lang>, program: Program) -> impl IntoView {
    request_animation_frame(|| {
        let title = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.get_element_by_id(TITLE));
        if let Some(title) = title {
            let _ = title.unchecked_into::<HtmlElement>().focus();
        }
    });
    let keydown = move |event: KeyboardEvent| {
        if event.key() == "Escape" {
            program.dismiss();
        }
    };
    view! {
        <div class="update" role="dialog" aria-modal="true" aria-labelledby=TITLE on:keydown=keydown>
            <div class="update-card">
                <h2 id=TITLE tabindex="-1">{tr(lang, Key::UpdateTitle)}</h2>
                <UpdateNews lang program />
                <button type="button" class="wide" on:click=move |_| program.dismiss()>
                    <span class="label">{tr(lang, Key::UpdateGotIt)}</span>
                </button>
                <p class="hint">{tr(lang, Key::UpdateKeepsData)}</p>
            </div>
        </div>
    }
}

/// `id` заголовка окна «Что нового»: по нему окно названо для диктора и на
/// нём встаёт фокус.
const TITLE: &str = "update-title";

/// Заметки версий новее той, что видел человек, и не новее работающей,
/// свежие сверху, на языке окна: номер и день выпуска, под ними пункты.
#[component]
fn UpdateNews(lang: ReadSignal<Lang>, program: Program) -> impl IntoView {
    move || {
        let Updated { seen, notes } = program.updated.get()?;
        let lang = lang.get();
        let versions: Vec<_> = notes
            .between(seen, Version::CURRENT)
            .filter(|note| !note.items(lang).is_empty())
            .map(|note| {
                let heading = fill(
                    t(lang, Key::UpdateVersion),
                    &[&note.version().to_string(), &i18n::date(lang, note.date())],
                );
                let items = note
                    .items(lang)
                    .iter()
                    .map(|item| view! { <li>{item.clone()}</li> })
                    .collect_view();
                view! {
                    <h4>{heading}</h4>
                    <ul>{items}</ul>
                }
            })
            .collect();
        (!versions.is_empty()).then(|| view! {
            <div class="update-news">
                <h3>{t(lang, Key::UpdateWhatsNew)}</h3>
                {versions}
            </div>
        })
    }
}

/// Слова над экраном о новой версии: пришла, пока открыта форма, — и включится
/// сама, когда форму сохранят или закроют; или включается прямо сейчас. Пусто
/// в остальное время.
#[component]
pub fn UpdateNotice(lang: ReadSignal<Lang>, program: Program, forms: Forms) -> impl IntoView {
    let shown = Memo::new(move |_| program.notice(forms));
    move || {
        shown.get().map(|key| view! {
            <div class="notice" role="status">
                <p>{tr(lang, key)}</p>
            </div>
        })
    }
}

/// Раздел «Установка» на языке окна — блоком, как карточки под ним (стиль
/// `.install`). Есть всегда, пока программа не установлена (задача 55):
/// кнопка задаёт вопрос браузера, а нет его — раскрывает слова, как
/// установить программу в этом браузере руками. Пусто в установленной.
///
/// Рисуется заново, только когда программу установили (`Memo`): вопрос
/// браузера приходит, когда раздел уже на экране, и фокус с кнопки не
/// должен пропадать. Слова раскрываются под кнопкой, а не заменяют её:
/// второе нажатие их прячет (`aria-expanded`). Спрятанных слов нет и в
/// разметке: в армянских — английские названия пунктов меню, и проверка
/// смены языка (`ui/tests.rs`) приняла бы их за непереведённые строки.
#[component]
pub fn InstallSection(lang: ReadSignal<Lang>, program: Program) -> impl IntoView {
    let offered = Memo::new(move |_| program.install.get() != Install::Installed);
    let words = Words::of(program.browser);
    move || {
        offered.get().then(|| {
            let state = move || program.install.get();
            let open = move || state() == Install::Manual && program.steps.get();
            // Раскрывает слова кнопка, только пока вопроса браузера нет.
            let expanded = move || (state() == Install::Manual).then(|| open().to_string());
            view! {
                <section class="install">
                    <h2>{tr(lang, Key::InstallTitle)}</h2>
                    <p>{tr(lang, words.about)}</p>
                    <button
                        type="button"
                        aria-expanded=expanded
                        disabled=move || state() == Install::Asking
                        on:click=move |_| program.press_install()
                    >
                        <span class="label">{tr(lang, Key::InstallButton)}</span>
                    </button>
                    {move || open().then(|| view! {
                        <div class="install-steps">
                            <p>{tr(lang, words.steps)}</p>
                            <p class="hint">{tr(lang, words.hint)}</p>
                        </div>
                    })}
                </section>
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// «Включили в другой вкладке» не теряется, что бы ни пришло потом,
    /// а своё включение им не перебивается: страница и так перезагрузится.
    #[test]
    fn update_states_follow_the_browser() {
        let mut version = NewVersion::Current;
        version.apply(Event::CanInstall);
        assert_eq!(version, NewVersion::Current);
        version.apply(Event::UpdateReady);
        assert_eq!(version, NewVersion::Ready);
        version.apply(Event::UpdatedElsewhere);
        assert_eq!(version, NewVersion::Elsewhere);
        version.apply(Event::UpdateReady);
        assert_eq!(version, NewVersion::Elsewhere);

        let mut applying = NewVersion::Applying;
        applying.apply(Event::UpdatedElsewhere);
        applying.apply(Event::UpdateReady);
        assert_eq!(applying, NewVersion::Applying);
    }

    /// Ждёт включения только пришедшая версия: та, что уже включается, — нет.
    #[test]
    fn only_a_new_version_waits() {
        let waiting: Vec<NewVersion> = [
            NewVersion::Current,
            NewVersion::Ready,
            NewVersion::Applying,
            NewVersion::Elsewhere,
        ]
        .into_iter()
        .filter(|state| state.waits())
        .collect();
        assert_eq!(waiting, [NewVersion::Ready, NewVersion::Elsewhere]);
    }

    /// Версию включают, только когда она новее работающей и её ещё не
    /// пробовали: сборка, которая не заработала, не перезагружает страницу
    /// второй раз, и программа открывается прежней. Следующая версия — уже
    /// новая попытка. Включённой в другой вкладке нужна только перезагрузка:
    /// её держит прежняя страница, а не запись о попытке.
    #[test]
    fn a_version_is_switched_on_once() {
        let next = Some(Version::new(9, 0, 1));
        let later = Some(Version::new(9, 0, 2));
        let mut wrong = Vec::new();
        for (state, target, tried, due) in [
            (NewVersion::Current, next, None, false),
            (NewVersion::Applying, next, None, false),
            (NewVersion::Ready, None, None, false),
            (NewVersion::Ready, next, None, true),
            (NewVersion::Ready, next, next, false),
            (NewVersion::Ready, later, next, true),
            (NewVersion::Elsewhere, None, None, false),
            (NewVersion::Elsewhere, next, None, true),
            (NewVersion::Elsewhere, next, next, true),
        ] {
            if state.due(target, tried) != due {
                wrong.push(format!("{state:?}, цель {target:?}, пробовали {tried:?}: не {due}"));
            }
        }
        assert!(wrong.is_empty(), "включение версии: {wrong:#?}");
    }

    /// Установленной программе установку не предлагают, даже если браузер
    /// прислал вопрос (так делает headless Chrome); в браузере кнопка есть
    /// с первого кадра — слова, пока браузер не дал вопроса, и его вопрос,
    /// когда дал. После ответа — снова вопрос, если браузер его прислал
    /// (Chromium шлёт новый сразу после отказа), иначе слова.
    #[test]
    fn install_states_follow_the_browser() {
        assert_eq!(Install::initial(true), Install::Installed);
        assert_eq!(Install::initial(false), Install::Manual);

        let mut installed = Install::Installed;
        installed.apply(Event::CanInstall);
        assert_eq!(installed, Install::Installed);
        installed.answered(true);
        assert_eq!(installed, Install::Installed);

        let mut install = Install::Manual;
        install.apply(Event::UpdateReady);
        assert_eq!(install, Install::Manual);
        install.apply(Event::CanInstall);
        assert_eq!(install, Install::Button);
        install = Install::Asking;
        install.apply(Event::CanInstall);
        assert_eq!(install, Install::Asking);
        install.answered(false);
        assert_eq!(install, Install::Manual);
        install = Install::Asking;
        install.answered(true);
        assert_eq!(install, Install::Button);
        install = Install::Manual;
        install.answered(true);
        assert_eq!(install, Install::Manual);
        install.apply(Event::Installed);
        assert_eq!(install, Install::Installed);
    }
}
