//! Экраны окна и адрес (задача 2 дизайна «Kvartik Design»): какой экран
//! открыт, говорит часть адреса после `#` — `#/clients`.
//!
//! Адрес, а не только сигнал: кнопка «Назад» на телефоне ведёт на прошлый
//! экран, а не закрывает программу, и перезагрузка открывает тот же экран.
//! Экраны при этом остаются в странице и только прячутся: начатое в форме
//! не пропадает, когда человек заглянул в другой раздел, а переход —
//! мгновенный, без новой отрисовки (Правило 1).

use leptos::ev;
use leptos::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::HtmlElement;

use crate::platform;

/// Экран окна.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Screen {
    /// Главная: что ждёт внимания — копия, неоплаченные счета.
    Home,
    Documents,
    Clients,
    Profile,
    /// Резервная копия и хранение данных — экран внутри профиля.
    Backup,
}

impl Screen {
    /// Все экраны — в порядке разделов.
    pub const ALL: [Screen; 5] = [
        Screen::Home,
        Screen::Documents,
        Screen::Clients,
        Screen::Profile,
        Screen::Backup,
    ];

    /// Адрес экрана — для ссылок. У главного — `#/`, а не пустой: пустая
    /// ссылка вела бы на страницу заново, как перезагрузка.
    pub const fn href(self) -> &'static str {
        match self {
            Screen::Home => "#/",
            Screen::Documents => "#/documents",
            Screen::Clients => "#/clients",
            Screen::Profile => "#/profile",
            Screen::Backup => "#/backup",
        }
    }

    /// Экран по части адреса после `#`. Пустая, чужая или устаревшая —
    /// главный экран: человек, открывший старую закладку, не увидит пустоты.
    pub fn from_hash(hash: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|screen| screen.href() == hash)
            .unwrap_or(Screen::Home)
    }

    /// Раздел, который подсвечен в меню на этом экране: копия — часть
    /// профиля.
    pub const fn section(self) -> Screen {
        match self {
            Screen::Backup => Screen::Profile,
            other => other,
        }
    }

    /// `id` разметки экрана.
    pub const fn id(self) -> &'static str {
        match self {
            Screen::Home => "home",
            Screen::Documents => "documents",
            Screen::Clients => "clients",
            Screen::Profile => "profile",
            Screen::Backup => "backup",
        }
    }
}

/// Экран открылся по ссылке или «Назад»: страница — с начала, а фокус — на
/// заголовке экрана: диктор называет, куда пришли, а Tab продолжает с начала
/// экрана. После кадра: к нему экран уже не спрятан.
pub fn shown(screen: Screen) {
    request_animation_frame(move || {
        let Some(window) = web_sys::window() else {
            return;
        };
        window.scroll_to_with_x_and_y(0.0, 0.0);
        let heading = window
            .document()
            .and_then(|document| document.query_selector(&format!("#{} h1", screen.id())).ok())
            .flatten();
        if let Some(heading) = heading {
            let _ = heading.unchecked_into::<HtmlElement>().focus();
        }
    });
}

/// Какой экран открыт — один на всё окно.
#[derive(Clone, Copy)]
pub struct Router(RwSignal<Screen>);

impl Router {
    /// Экран из адреса, и дальше — за адресом: ссылки меню, «Назад»
    /// и «Вперёд» браузера меняют его, а окно слушает `hashchange`.
    /// Слушатель снимается вместе с окном: браузерные тесты монтируют окно
    /// много раз.
    pub fn start() -> Self {
        let screen = RwSignal::new(Screen::from_hash(&platform::hash()));
        let listener = window_event_listener(ev::hashchange, move |_| {
            screen.set(Screen::from_hash(&platform::hash()));
        });
        on_cleanup(move || listener.remove());
        Self(screen)
    }

    /// Открытый экран — отслеживаемое.
    pub fn screen(self) -> Screen {
        self.0.get()
    }

    /// Открыт ли экран — отслеживаемое.
    pub fn is(self, screen: Screen) -> bool {
        self.0.with(|open| *open == screen)
    }

    /// Перейти на экран из кода (кнопка «Новый счёт», строка списка):
    /// адрес меняется, и «Назад» вернёт прежний экран.
    pub fn go(self, screen: Screen) {
        if self.0.get_untracked() != screen {
            platform::set_hash(screen.href());
            self.0.set(screen);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Адрес экрана читается обратно в тот же экран, а пустой, чужой и
    /// неполный адрес — главный экран.
    #[test]
    fn screens_round_trip_through_the_address() {
        let mut wrong = Vec::new();
        for screen in Screen::ALL {
            if Screen::from_hash(screen.href()) != screen {
                wrong.push(format!("{screen:?}: {}", screen.href()));
            }
        }
        for hash in ["", "#", "#backup", "#/clients/", "#/nothing", "/clients"] {
            if Screen::from_hash(hash) != Screen::Home {
                wrong.push(format!("«{hash}» — {:?}, а не главный", Screen::from_hash(hash)));
            }
        }
        assert!(wrong.is_empty(), "адреса экранов: {wrong:#?}");
    }

    /// В меню подсвечен раздел экрана: у копии — профиль, у остальных —
    /// они сами.
    #[test]
    fn backup_lives_under_the_profile() {
        assert_eq!(Screen::Backup.section(), Screen::Profile);
        for screen in [Screen::Home, Screen::Documents, Screen::Clients, Screen::Profile] {
            assert_eq!(screen.section(), screen);
        }
    }
}
