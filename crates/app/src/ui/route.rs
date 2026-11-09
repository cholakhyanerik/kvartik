//! Экраны окна и адрес (дизайн «Kvartik Design»): какой экран открыт,
//! говорит часть адреса после `#` — `#/clients`, `#/documents/<id>`.
//!
//! Адрес, а не только сигнал: кнопка «Назад» на телефоне ведёт на прошлый
//! экран, а не закрывает программу, и перезагрузка открывает тот же экран.
//! Экраны при этом остаются в странице и только прячутся: начатое в форме
//! не пропадает, когда человек заглянул в другой раздел, а переход — без
//! новой отрисовки (Правило 1): прежний экран растворяется, новый въезжает
//! снизу (`ui/motion.rs`, задача 86).

use kvartik_core::model::record::RecordId;
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
    /// Выпущенный документ — карточкой: на телефоне вместо списка, на
    /// компьютере рядом с ним. «Назад» ведёт от него к списку.
    Document(RecordId),
    /// Книга доходов и расходов (задача 14).
    Ledger,
    Clients,
    /// Налог с оборота за квартал (задача 19).
    Tax,
    /// Годовые платежи — экран внутри раздела налога (задача 20).
    Annual,
    /// Сроки — экран внутри раздела налога (задача 21).
    Deadlines,
    /// Числа для расчёта ՊԵԿ — экран внутри раздела налога (задача 24).
    Report,
    Profile,
    /// Резервная копия и хранение данных — экран внутри профиля.
    Backup,
    /// Лицензия и пробный период — экран внутри профиля (задача 25).
    License,
    /// О программе — экран внутри профиля (задача 31).
    About,
    /// ИП в программе — экран внутри профиля (задача 38).
    Office,
    /// Настройки — свой раздел (задача 88): на компьютере у него своя строка
    /// внизу меню, а на телефоне вкладки для него нет — шестерёнка на главном
    /// экране и строка в профиле.
    Settings,
}

/// Начало адреса документа: дальше — его идентификатор.
const DOCUMENT: &str = "#/documents/";

impl Screen {
    /// Экраны без параметров — в порядке разделов.
    pub const PLAIN: [Screen; 14] = [
        Screen::Home,
        Screen::Documents,
        Screen::Ledger,
        Screen::Clients,
        Screen::Tax,
        Screen::Annual,
        Screen::Deadlines,
        Screen::Report,
        Screen::Profile,
        Screen::Backup,
        Screen::License,
        Screen::About,
        Screen::Office,
        Screen::Settings,
    ];

    /// Адрес экрана — для ссылок. У главного — `#/`, а не пустой: пустая
    /// ссылка вела бы на страницу заново, как перезагрузка.
    pub fn href(self) -> String {
        match self {
            Screen::Home => "#/".to_owned(),
            Screen::Documents => "#/documents".to_owned(),
            Screen::Document(id) => format!("{DOCUMENT}{id}"),
            Screen::Ledger => "#/ledger".to_owned(),
            Screen::Clients => "#/clients".to_owned(),
            Screen::Tax => "#/tax".to_owned(),
            Screen::Annual => "#/annual".to_owned(),
            Screen::Deadlines => "#/deadlines".to_owned(),
            Screen::Report => "#/report".to_owned(),
            Screen::Profile => "#/profile".to_owned(),
            Screen::Backup => "#/backup".to_owned(),
            Screen::License => "#/license".to_owned(),
            Screen::About => "#/about".to_owned(),
            Screen::Office => "#/office".to_owned(),
            Screen::Settings => "#/settings".to_owned(),
        }
    }

    /// Экран по части адреса после `#`. Пустая, чужая или устаревшая —
    /// главный экран: человек, открывший старую закладку, не увидит пустоты;
    /// документ с нечитаемым идентификатором — список документов.
    pub fn from_hash(hash: &str) -> Self {
        if let Some(id) = hash.strip_prefix(DOCUMENT) {
            return RecordId::parse(id).map_or(Screen::Documents, Screen::Document);
        }
        Self::PLAIN
            .into_iter()
            .find(|screen| screen.href() == hash)
            .unwrap_or(Screen::Home)
    }

    /// Экран-страница, на которой он показан: документ — на странице
    /// документов, рядом со списком.
    pub const fn page(self) -> Screen {
        match self {
            Screen::Document(_) => Screen::Documents,
            other => other,
        }
    }

    /// Раздел, который подсвечен в меню на этом экране: копия, лицензия,
    /// «О программе» и ИП в программе — часть профиля, годовые платежи, сроки
    /// и числа для расчёта — налога, документ — раздела документов.
    pub const fn section(self) -> Screen {
        match self {
            Screen::Backup | Screen::License | Screen::About | Screen::Office => Screen::Profile,
            Screen::Annual | Screen::Deadlines | Screen::Report => Screen::Tax,
            Screen::Document(_) => Screen::Documents,
            other => other,
        }
    }

    /// `id` разметки страницы экрана.
    pub const fn id(self) -> &'static str {
        match self.page() {
            Screen::Home => "home",
            Screen::Documents | Screen::Document(_) => "documents",
            Screen::Ledger => "ledger",
            Screen::Clients => "clients",
            Screen::Tax => "tax",
            Screen::Annual => "annual",
            Screen::Deadlines => "deadlines",
            Screen::Report => "report",
            Screen::Profile => "profile",
            Screen::Backup => "backup",
            Screen::License => "license",
            Screen::About => "about",
            Screen::Office => "office",
            Screen::Settings => "settings",
        }
    }
}

/// Экран открылся по ссылке или «Назад»: страница — с начала, а фокус — на
/// заголовке экрана, у документа — на заголовке его карточки: диктор
/// называет, куда пришли, а Tab продолжает с начала экрана. После кадра: к
/// нему экран уже не спрятан.
pub fn shown(screen: Screen) {
    request_animation_frame(move || {
        let Some(window) = web_sys::window() else {
            return;
        };
        window.scroll_to_with_x_and_y(0.0, 0.0);
        let selector = match screen {
            Screen::Document(_) => "#documents .docs-detail [tabindex=\"-1\"]".to_owned(),
            other => format!("#{} h1", other.id()),
        };
        let heading = window
            .document()
            .and_then(|document| document.query_selector(&selector).ok())
            .flatten();
        if let Some(heading) = heading {
            let _ = heading.unchecked_into::<HtmlElement>().focus();
        }
    });
}

/// Какой экран открыт — один на всё окно.
#[derive(Clone, Copy)]
pub struct Router {
    /// Экран на виду: за ним — разметка окна. Сменяется переходом
    /// (`ui/motion.rs`) — на кадр-другой позже адреса: прежний экран браузер
    /// снимает до смены разметки (задача 86).
    screen: RwSignal<Screen>,
    /// Экран по адресу — сразу. По нему решают переходы и первые шаги.
    target: RwSignal<Screen>,
    /// Экран, на который привели первые шаги (`ui/onboarding.rs`): его форма,
    /// закрывшись, вернёт на главный экран ([`Router::form_closed`]). Ушёл
    /// человек с него сам — больше не ведём.
    guided: RwSignal<Option<Screen>>,
}

impl Router {
    /// Экран из адреса, и дальше — за адресом: ссылки меню, «Назад»
    /// и «Вперёд» браузера меняют его, а окно слушает `hashchange`.
    /// Слушатель снимается вместе с окном: браузерные тесты монтируют окно
    /// много раз.
    pub fn start() -> Self {
        let first = Screen::from_hash(&platform::hash());
        let router = Self {
            screen: RwSignal::new(first),
            target: RwSignal::new(first),
            guided: RwSignal::new(None),
        };
        let listener = window_event_listener(ev::hashchange, move |_| {
            let next = Screen::from_hash(&platform::hash());
            if router.guided.get_untracked() != Some(next) {
                router.guided.set(None);
            }
            router.show(next);
        });
        on_cleanup(move || listener.remove());
        router
    }

    /// Открытый экран — отслеживаемое: тот, что на виду.
    pub fn screen(self) -> Screen {
        self.screen.get()
    }

    /// Видна ли страница экрана — отслеживаемое: документ открыт на
    /// странице документов.
    pub fn shows(self, page: Screen) -> bool {
        self.screen.with(|open| open.page() == page)
    }

    /// Перейти на экран из кода (кнопка «Новый счёт», выпуск документа):
    /// адрес меняется, и «Назад» вернёт прежний экран.
    pub fn go(self, screen: Screen) {
        self.guided.set(None);
        if self.target.get_untracked() != screen {
            platform::set_hash(&screen.href());
            self.show(screen);
        }
    }

    /// Экран по адресу сменился: разметка — переходом. Адрес, который
    /// поставил [`Router::go`], приходит потом и своим `hashchange` — он уже
    /// показан. Переходы, начатые один за другим, кончаются последним
    /// экраном по адресу, а окно, снятое до конца перехода, ничего не ждёт.
    fn show(self, next: Screen) {
        let from = self.target.get_untracked();
        if from == next {
            return;
        }
        self.target.set(next);
        let (screen, target) = (self.screen, self.target);
        super::motion::screen(from, next, move || {
            if let Some(next) = target.try_get_untracked() {
                screen.try_set(next);
            }
        });
    }

    /// Перейти на экран по первым шагам: его форма, закрывшись, вернёт на
    /// главный экран к следующему шагу.
    pub fn guide(self, screen: Screen) {
        self.go(screen);
        self.guided.set(Some(screen));
    }

    /// Форма закрылась — сохранили или передумали. Если на её экран привели
    /// первые шаги, дальше главный экран: следующий шаг там.
    pub fn form_closed(self) {
        if self.guided.get_untracked() == Some(self.target.get_untracked()) {
            self.go(Screen::Home);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Адрес экрана читается обратно в тот же экран, у документа — со своим
    /// идентификатором, а пустой, чужой и неполный адрес — главный экран;
    /// документ с нечитаемым идентификатором — список документов.
    #[test]
    fn screens_round_trip_through_the_address() {
        let mut wrong = Vec::new();
        let document = Screen::Document(RecordId::random());
        for screen in Screen::PLAIN.into_iter().chain([document]) {
            if Screen::from_hash(&screen.href()) != screen {
                wrong.push(format!("{screen:?}: {}", screen.href()));
            }
        }
        for hash in ["", "#", "#backup", "#/clients/", "#/nothing", "/clients"] {
            if Screen::from_hash(hash) != Screen::Home {
                wrong.push(format!("«{hash}» — {:?}, а не главный", Screen::from_hash(hash)));
            }
        }
        for hash in ["#/documents/", "#/documents/123", "#/documents/A0B1"] {
            if Screen::from_hash(hash) != Screen::Documents {
                wrong.push(format!("«{hash}» — {:?}, а не список", Screen::from_hash(hash)));
            }
        }
        assert!(wrong.is_empty(), "адреса экранов: {wrong:#?}");
    }

    /// В меню подсвечен раздел экрана: у копии — профиль, у годовых
    /// платежей, сроков и чисел для расчёта — налог, у документа —
    /// документы; документ показан на странице документов.
    #[test]
    fn screens_live_in_their_sections() {
        let document = Screen::Document(RecordId::random());
        assert_eq!(Screen::Backup.section(), Screen::Profile);
        assert_eq!(Screen::Annual.section(), Screen::Tax);
        assert_eq!(Screen::Annual.page(), Screen::Annual);
        assert_eq!(Screen::Deadlines.section(), Screen::Tax);
        assert_eq!(Screen::Deadlines.page(), Screen::Deadlines);
        assert_eq!(Screen::Report.section(), Screen::Tax);
        assert_eq!(Screen::Report.page(), Screen::Report);
        assert_eq!(Screen::Report.href(), "#/report");
        assert_eq!(document.section(), Screen::Documents);
        assert_eq!(document.page(), Screen::Documents);
        assert_eq!(document.id(), "documents");
        assert_eq!(Screen::Backup.page(), Screen::Backup);
        assert_eq!(Screen::License.section(), Screen::Profile);
        assert_eq!(Screen::License.page(), Screen::License);
        assert_eq!(Screen::License.href(), "#/license");
        assert_eq!(Screen::About.section(), Screen::Profile);
        assert_eq!(Screen::About.page(), Screen::About);
        assert_eq!(Screen::About.href(), "#/about");
        assert_eq!(Screen::Office.section(), Screen::Profile);
        assert_eq!(Screen::Office.page(), Screen::Office);
        assert_eq!(Screen::Office.href(), "#/office");
        assert_eq!(Screen::Settings.href(), "#/settings");
        for screen in [
            Screen::Home,
            Screen::Documents,
            Screen::Ledger,
            Screen::Clients,
            Screen::Tax,
            Screen::Profile,
            Screen::Settings,
        ] {
            assert_eq!(screen.section(), screen);
            assert_eq!(screen.page(), screen);
        }
    }
}
