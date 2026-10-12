//! Программа как приложение (задача 8): «есть новая версия — обновить»
//! и раздел «Установка».
//!
//! Новая версия включается только по согласию (Правило 6): страница
//! перезагрузится, и несохранённый ввод на ней пропадёт, поэтому решает
//! человек. Установка — кнопкой там, где браузер её даёт (Chromium),
//! а на iPhone и iPad — словами: браузеры там установку кнопкой не дают,
//! а от неё зависит, увидит ли программа данные (Правило 6), поэтому
//! раздел стоит выше разделов о данных.

use kvartik_core::i18n::{Key, Lang};
use leptos::prelude::*;

use super::tr;
use crate::platform;
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
            Event::CanInstall | Event::Installed => {}
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
            Event::UpdateReady | Event::UpdatedElsewhere => {}
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
    install: RwSignal<Install>,
    pwa: StoredValue<Pwa, LocalStorage>,
}

impl Program {
    /// Зарегистрировать service worker и слушать браузер.
    pub fn new() -> Self {
        let version = RwSignal::new(NewVersion::Current);
        let install = RwSignal::new(Install::initial(
            browser::installed(),
            browser::install_by_hand(),
        ));
        let pwa = Pwa::start(move |event| {
            version.update(|state| state.apply(event));
            install.update(|state| state.apply(event));
        });
        Self {
            version,
            install,
            pwa: StoredValue::new_local(pwa),
        }
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
}

/// «Есть новая версия» на языке окна. Пусто, пока новой версии нет.
#[component]
pub fn UpdateNotice(lang: ReadSignal<Lang>, program: Program) -> impl IntoView {
    move || match program.version.get() {
        NewVersion::Current => None,
        NewVersion::Ready => Some(
            view! {
                <div class="notice" role="status">
                    <p>{tr(lang, Key::UpdateReady)}</p>
                    <button type="button" on:click=move |_| program.apply_update()>
                        {tr(lang, Key::UpdateApply)}
                    </button>
                </div>
            }
            .into_any(),
        ),
        NewVersion::Applying => Some(
            view! {
                <div class="notice" role="status">
                    <p>{tr(lang, Key::UpdateApplying)}</p>
                </div>
            }
            .into_any(),
        ),
        NewVersion::Elsewhere => Some(
            view! {
                <div class="notice" role="status">
                    <p>{tr(lang, Key::UpdateElsewhere)}</p>
                    <button type="button" on:click=|_| platform::reload()>
                        {tr(lang, Key::Reload)}
                    </button>
                </div>
            }
            .into_any(),
        ),
    }
}

/// Раздел «Установка» на языке окна. Пусто, когда предложить нечего.
#[component]
pub fn InstallSection(lang: ReadSignal<Lang>, program: Program) -> impl IntoView {
    move || match program.install.get() {
        Install::Installed | Install::Nothing => None,
        state @ (Install::Button | Install::Asking) => Some(
            view! {
                <h2>{tr(lang, Key::InstallTitle)}</h2>
                <p>{tr(lang, Key::InstallAbout)}</p>
                <button
                    type="button"
                    disabled=state == Install::Asking
                    on:click=move |_| program.ask_install()
                >
                    {tr(lang, Key::InstallButton)}
                </button>
            }
            .into_any(),
        ),
        Install::Manual => Some(
            view! {
                <h2>{tr(lang, Key::InstallTitle)}</h2>
                <p>{tr(lang, Key::InstallFirst)}</p>
                <p>{tr(lang, Key::InstallSteps)}</p>
                <p class="hint">{tr(lang, Key::InstallMoveData)}</p>
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
