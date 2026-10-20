//! Программа как приложение (задача 8): окно новой версии и раздел
//! «Установка».
//!
//! Новая версия включается только по согласию (Правило 6): страница
//! перезагрузится, и несохранённый ввод на ней пропадёт, поэтому решает
//! человек. Говорит о ней окно поверх всей программы — что в ней нового
//! и «Обновить» (задача 67), — и оно обязательное: ни «Позже», ни крестика,
//! а программа под ним неактивна, пока человек не обновит. Так у всех
//! последняя версия — с последними правилами налога. Но пока открыта форма,
//! окно ждёт, когда её сохранят или закроют, а до того над экраном — только
//! слова (решение человека 2026-10-18): начатое в форме не пропадает.
//!
//! Установка — кнопкой там, где браузер её даёт (Chromium), а на iPhone
//! и iPad — словами: браузеры там установку кнопкой не дают, а от неё
//! зависит, увидит ли программа данные (Правило 6), поэтому раздел стоит
//! выше разделов о данных.

use std::sync::Arc;

use kvartik_core::i18n::{self, Key, Lang, fill, t};
use kvartik_core::release::{Notes, Version};
use leptos::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::HtmlElement;

use super::form::Forms;
use super::tr;
use crate::platform::pwa::{self as browser, Event, Pwa};

/// Новая версия программы.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NewVersion {
    /// Работает последняя известная версия.
    Current,
    /// Новая версия скачана и ждёт согласия.
    Ready,
    /// Человек согласился: страница вот-вот перезагрузится.
    Applying,
    /// Новую версию включили в другой вкладке: эта страница — ещё старая.
    Elsewhere,
}

impl NewVersion {
    /// Браузер сообщил о новой версии. «Включили в другой вкладке» сильнее
    /// «ждёт согласия»: ждущая версия уже работает, и согласие здесь ничего
    /// не включит — нужна перезагрузка.
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

    /// Новая версия пришла и ждёт, когда человек её включит — согласием или
    /// перезагрузкой.
    const fn waits(self) -> bool {
        matches!(self, Self::Ready | Self::Elsewhere)
    }
}

/// Что нового в пришедшей версии.
#[derive(Clone, PartialEq, Debug)]
enum News {
    /// Заметки ещё читаются — окно подождёт их: иначе список въехал бы
    /// в него на глазах.
    Reading,
    /// Прочитаны; `None` — файла нет или он не читается: окно скажет
    /// о новой версии без списка.
    Read(Option<Arc<Notes>>),
}

impl News {
    /// Новее ли пришедшая сборка работающей; `None` — заметки ещё читаются.
    /// Новее — если в её заметках есть версия новее этой. Прочитались,
    /// а новее нет — пришла прежняя сборка: хостинг ещё минуты после
    /// выкладки отдаёт часть ответов прежней (Правило 6), и «обновление»
    /// откатило бы программу назад, — или та же. Окно тогда не встаёт,
    /// а новую сборку найдёт следующая проверка. Не прочитались вовсе —
    /// новее: о новой версии лучше сказать без списка, чем промолчать.
    fn newer(&self) -> Option<bool> {
        match self {
            News::Reading => None,
            News::Read(None) => Some(true),
            News::Read(Some(notes)) => Some(notes.newest() > Some(Version::CURRENT)),
        }
    }
}

/// Что предложить про установку.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Install {
    /// Программа установлена или открыта установленной: предлагать нечего,
    /// что бы ни говорил браузер.
    Installed,
    /// Нечего: браузер установку не даёт или ещё не сказал, что даёт.
    Nothing,
    /// Браузер даёт установить по кнопке.
    Button,
    /// Вопрос браузера на экране — ждём ответа человека.
    Asking,
    /// iPhone или iPad не в установленной программе: установить можно
    /// только руками, из меню браузера.
    Manual,
}

impl Install {
    /// С чего начать: открыта ли программа установленной и ставят ли её
    /// здесь только руками.
    pub const fn initial(installed: bool, by_hand: bool) -> Self {
        if installed {
            Self::Installed
        } else if by_hand {
            Self::Manual
        } else {
            Self::Nothing
        }
    }

    /// Браузер сообщил об установке.
    pub fn apply(&mut self, event: Event) {
        match event {
            Event::CanInstall => {
                if *self == Self::Nothing {
                    *self = Self::Button;
                }
            }
            Event::Installed => *self = Self::Installed,
            Event::UpdateReady | Event::UpdatedElsewhere | Event::Serving => {}
        }
    }

    /// Человек ответил браузеру. Второй раз тот же вопрос браузер не задаёт:
    /// отказался — кнопки больше нет до следующего открытия, согласился —
    /// установку подтвердит событие `Installed`, до или после ответа.
    pub fn answered(&mut self) {
        if *self == Self::Asking {
            *self = Self::Nothing;
        }
    }
}

/// Что окно знает о программе как приложении — одно на все разделы.
#[derive(Clone, Copy)]
pub struct Program {
    version: RwSignal<NewVersion>,
    news: RwSignal<News>,
    install: RwSignal<Install>,
    /// Страницей правит service worker, и весь сайт — в его кэше
    /// ([`Event::Serving`]).
    serving: RwSignal<bool>,
    pwa: StoredValue<Pwa, LocalStorage>,
}

impl Program {
    /// Зарегистрировать service worker и слушать браузер.
    pub fn new() -> Self {
        let version = RwSignal::new(NewVersion::Current);
        let news = RwSignal::new(News::Reading);
        let install = RwSignal::new(Install::initial(
            browser::installed(),
            browser::install_by_hand(),
        ));
        let serving = RwSignal::new(false);
        let pwa = Pwa::start(move |event| {
            // Подписчикам — только перемена: событие установки или правки
            // страницы service worker'ом окну новой версии ничего не меняет.
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
            install,
            serving,
            pwa: StoredValue::new_local(pwa),
        }
    }

    /// Правит ли страницей service worker — отслеживаемое: части wasm
    /// окно загружает заранее, только когда они придут из его кэша.
    pub fn serving(&self) -> bool {
        self.serving.get()
    }

    /// Стоит ли на экране окно новой версии — отслеживаемое: программа под
    /// ним неактивна (`ui/mod.rs`). Пришедшая версия ждёт, пока прочитаны
    /// её заметки и не открыто ни одной формы, а не новее работающей — не
    /// встаёт вовсе ([`News::newer`]); согласие окно уже не снимает.
    pub fn blocks(&self, forms: Forms) -> bool {
        match self.version.get() {
            NewVersion::Current => false,
            NewVersion::Applying => true,
            NewVersion::Ready | NewVersion::Elsewhere => {
                !forms.any() && self.news.with(News::newer) == Some(true)
            }
        }
    }

    /// Пришла версия новее, а открыта форма — окно ждёт её: слова над
    /// экраном. Отслеживаемое.
    fn waits_for_forms(&self, forms: Forms) -> bool {
        self.version.get().waits() && forms.any() && self.news.with(News::newer) == Some(true)
    }

    fn apply_update(&self) {
        self.version.set(NewVersion::Applying);
        self.pwa.with_value(Pwa::update);
    }

    fn ask_install(&self) {
        if self.install.get_untracked() != Install::Button {
            return;
        }
        self.install.set(Install::Asking);
        let pwa = self.pwa.get_value();
        let install = self.install;
        leptos::task::spawn_local(async move {
            pwa.install().await;
            install.update(Install::answered);
        });
    }

    /// Как будто браузер сообщил о новой версии, а её заметки — `notes`:
    /// браузерным тестам окна (`ui/tests/update.rs`) — service worker есть
    /// только в выпускной сборке.
    #[cfg(all(test, target_arch = "wasm32"))]
    pub(super) fn pretend(&self, event: Event, notes: Option<Notes>) {
        self.version.update(|state| state.apply(event));
        self.news.set(News::Read(notes.map(Arc::new)));
    }
}

/// Прочитать заметки пришедшей версии — из её кэша, а не из сети: он полон,
/// как только она установилась, и без сети тоже. На устройстве файлов
/// несколько — работающей сборки и пришедших, — и новее тот, где версия
/// больше: его и покажет окно, а пришла ли вообще версия новее, решает
/// [`News::newer`]. Прочитанное прежде остаётся, пока не прочитано новое.
fn read_news(news: RwSignal<News>) {
    leptos::task::spawn_local(async move {
        let notes = browser::cached_texts(browser::NOTES)
            .await
            .iter()
            .filter_map(|text| Notes::parse(text).ok())
            .max_by_key(Notes::newest);
        news.set(News::Read(notes.map(Arc::new)));
    });
}

/// Окно новой версии на языке окна (задача 67): что нового и «Обновить»
/// поверх всей программы. Обязательное — без «Позже» и крестика, и Escape
/// его не закрывает; программа под ним неактивна (`inert`, `ui/mod.rs`).
/// Пусто, пока новой версии нет или открыта форма.
///
/// Не `<dialog>`: модальный `<dialog>` закрывает запрос на закрытие —
/// Escape, «Назад» на Android, жест закрытия диктора, — а не дать ему
/// закрыться страница может только после свежего действия человека: второй
/// запрос подряд закрывает его всегда (по спецификации HTML, «close
/// watchers», Living Standard от 2026-10-16). `closedby="none"` это
/// снимает, но он моложе самого `<dialog>`, и поддержку его пришлось бы
/// сверять в каждом браузере. Слой с `inert` под ним, как у плашки сбоя,
/// не закрывает ничто.
///
/// Рисуется заново, только когда окно встаёт или уходит (`Memo`): иначе
/// любая весть браузера пересобрала бы его, и фокус с кнопки прыгал бы на
/// заголовок.
#[component]
pub fn UpdateDialog(lang: ReadSignal<Lang>, program: Program, forms: Forms) -> impl IntoView {
    let shown = Memo::new(move |_| program.blocks(forms));
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
    let state = move || program.version.get();
    view! {
        <div class="update" role="dialog" aria-modal="true" aria-labelledby=TITLE>
            <div class="update-card">
                <h2 id=TITLE tabindex="-1">{tr(lang, Key::UpdateTitle)}</h2>
                <UpdateNews lang program />
                {move || (state() == NewVersion::Elsewhere).then(|| view! {
                    <p class="update-elsewhere">{tr(lang, Key::UpdateElsewhere)}</p>
                })}
                {move || if state() == NewVersion::Applying {
                    view! { <p class="update-applying" role="status">{tr(lang, Key::UpdateApplying)}</p> }
                        .into_any()
                } else {
                    let label = if state() == NewVersion::Elsewhere { Key::Reload } else { Key::UpdateApply };
                    view! {
                        <button type="button" class="wide" on:click=move |_| program.apply_update()>
                            {tr(lang, label)}
                        </button>
                    }
                    .into_any()
                }}
                <p class="hint">{tr(lang, Key::UpdateKeepsData)}</p>
            </div>
        </div>
    }
}

/// `id` заголовка окна новой версии: по нему окно названо для диктора
/// и на нём встаёт фокус.
const TITLE: &str = "update-title";

/// «Что нового» — заметки версий новее работающей, свежие сверху, на языке
/// окна: номер и день выпуска, под ними пункты. Не прочитались или новее
/// нет ни одной — ничего: о новой версии говорит заголовок окна.
#[component]
fn UpdateNews(lang: ReadSignal<Lang>, program: Program) -> impl IntoView {
    move || {
        let notes = program.news.with(|news| match news {
            News::Read(Some(notes)) => Some(Arc::clone(notes)),
            News::Read(None) | News::Reading => None,
        })?;
        let lang = lang.get();
        let versions: Vec<_> = notes
            .since(Version::CURRENT)
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

/// Новая версия пришла, пока открыта форма: окно с ней встанет, когда форму
/// сохранят или закроют, а до того — только эти слова над экраном, без
/// кнопки: нажатая, она стёрла бы начатое. Пусто в остальное время.
#[component]
pub fn UpdateNotice(lang: ReadSignal<Lang>, program: Program, forms: Forms) -> impl IntoView {
    let shown = Memo::new(move |_| program.waits_for_forms(forms));
    move || {
        shown.get().then(|| view! {
            <div class="notice" role="status">
                <p>{tr(lang, Key::UpdateAfterForm)}</p>
            </div>
        })
    }
}

/// Раздел «Установка» на языке окна — блоком, как карточки под ним (стиль
/// `.install`). Пусто, когда предложить нечего.
#[component]
pub fn InstallSection(lang: ReadSignal<Lang>, program: Program) -> impl IntoView {
    move || match program.install.get() {
        Install::Installed | Install::Nothing => None,
        state @ (Install::Button | Install::Asking) => Some(
            view! {
                <section class="install">
                    <h2>{tr(lang, Key::InstallTitle)}</h2>
                    <p>{tr(lang, Key::InstallAbout)}</p>
                    <button
                        type="button"
                        disabled=state == Install::Asking
                        on:click=move |_| program.ask_install()
                    >
                        {tr(lang, Key::InstallButton)}
                    </button>
                </section>
            }
            .into_any(),
        ),
        Install::Manual => Some(
            view! {
                <section class="install">
                    <h2>{tr(lang, Key::InstallTitle)}</h2>
                    <p>{tr(lang, Key::InstallFirst)}</p>
                    <p>{tr(lang, Key::InstallSteps)}</p>
                    <p class="hint">{tr(lang, Key::InstallMoveData)}</p>
                </section>
            }
            .into_any(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// «Включили в другой вкладке» не теряется, что бы ни пришло потом,
    /// а собственное согласие им не перебивается: страница и так
    /// перезагрузится.
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

    /// Ждёт включения только пришедшая версия: своя, согласие на которую уже
    /// дано, — нет.
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

    /// Установленной программе установку не предлагают, даже если браузер
    /// прислал вопрос (так делает headless Chrome); на iPhone — слова
    /// с первого кадра, а кнопка появляется, только когда браузер её дал,
    /// и пропадает после ответа: второй раз браузер не спросит.
    #[test]
    fn install_states_follow_the_browser() {
        assert_eq!(Install::initial(true, true), Install::Installed);
        assert_eq!(Install::initial(true, false), Install::Installed);
        assert_eq!(Install::initial(false, true), Install::Manual);
        assert_eq!(Install::initial(false, false), Install::Nothing);

        let mut installed = Install::Installed;
        installed.apply(Event::CanInstall);
        assert_eq!(installed, Install::Installed);

        let mut manual = Install::Manual;
        manual.apply(Event::CanInstall);
        assert_eq!(manual, Install::Manual);
        manual.apply(Event::Installed);
        assert_eq!(manual, Install::Installed);

        let mut install = Install::Nothing;
        install.apply(Event::UpdateReady);
        assert_eq!(install, Install::Nothing);
        install.apply(Event::CanInstall);
        assert_eq!(install, Install::Button);
        install = Install::Asking;
        install.apply(Event::CanInstall);
        assert_eq!(install, Install::Asking);
        install.answered();
        assert_eq!(install, Install::Nothing);
        install = Install::Manual;
        install.answered();
        assert_eq!(install, Install::Manual);
    }
}
