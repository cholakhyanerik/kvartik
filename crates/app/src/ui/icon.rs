//! Значки окна — линии толщиной и скруглением как в дизайне «Kvartik
//! Design». Нарисованы здесь простой геометрией на сетке 24×24, а не взяты
//! из набора значков: чужие пути — чужой код со своей лицензией (задача
//! 51), а дому, листу и человеку она не нужна. Цвет — `currentColor`:
//! значок берёт цвет текста рядом (Правило 4).

use leptos::prelude::*;

/// Какой значок.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Icon {
    /// Главная — дом.
    Home,
    /// Счета и акты — лист с загнутым углом и строками.
    Document,
    /// Книга доходов и расходов — раскрытая книга.
    Book,
    /// Вложение — скрепка.
    Clip,
    /// Клиенты — двое.
    Clients,
    /// Профиль — один человек.
    Person,
    /// Новое — плюс.
    Plus,
    /// Резервная копия — щит.
    Shield,
    /// Назад — угол влево.
    Back,
    /// Дальше — угол вправо.
    Forward,
    /// Поиск — лупа.
    Search,
    /// Сделано — галочка.
    Check,
    /// Сохранить файл — стрелка вниз на полку.
    Download,
    /// Отправить файл — стрелка вверх из лотка, как «Поделиться» у
    /// телефона.
    Share,
    /// Налог — знак процента, как в дизайне «Kvartik Design» (задача 19).
    Percent,
    /// Сроки — лист календаря (задача 21).
    Calendar,
    /// Скопировать — два листа один на другом (задача 24).
    Copy,
}

/// Значок для разметки. Он только украшает: что делает кнопка или ссылка,
/// говорит подпись рядом, поэтому диктору значок не читается
/// (`aria-hidden`).
pub fn icon(icon: Icon) -> impl IntoView {
    let shape = match icon {
        Icon::Home => view! {
            <path d="M4 10.5 12 4l8 6.5V20a1 1 0 0 1-1 1h-4v-6h-6v6H5a1 1 0 0 1-1-1z" />
        }
        .into_any(),
        Icon::Document => view! {
            <path d="M14 3H6a1 1 0 0 0-1 1v16a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1V8z" />
            <path d="M14 3v5h5M9 13h6M9 17h6" />
        }
        .into_any(),
        Icon::Book => view! {
            <path d="M12 6.5C10.3 5 7.8 4.5 4 4.5v13c3.8 0 6.3.5 8 2 1.7-1.5 4.2-2 8-2v-13c-3.8 0-6.3.5-8 2z" />
            <path d="M12 6.5v13" />
        }
        .into_any(),
        Icon::Clip => view! {
            <path d="m20 11.5-7.6 7.6a5 5 0 0 1-7-7l8.2-8.2a3.3 3.3 0 0 1 4.7 4.7l-8.2 8.2a1.7 1.7 0 0 1-2.4-2.4l7.5-7.5" />
        }
        .into_any(),
        Icon::Clients => view! {
            <circle cx="9" cy="8" r="3.5" />
            <path d="M3 20c0-3.3 2.7-6 6-6s6 2.7 6 6M16 4.5a3.5 3.5 0 0 1 0 7M18 14.4c1.8.9 3 2.9 3 5.6" />
        }
        .into_any(),
        Icon::Person => view! {
            <circle cx="12" cy="8" r="4" />
            <path d="M4 21c0-4.4 3.6-8 8-8s8 3.6 8 8" />
        }
        .into_any(),
        Icon::Plus => view! { <path d="M12 5v14M5 12h14" /> }.into_any(),
        Icon::Shield => view! { <path d="M12 3 5 6v6c0 4.5 3 7.8 7 9 4-1.2 7-4.5 7-9V6z" /> }.into_any(),
        Icon::Back => view! { <path d="m15 18-6-6 6-6" /> }.into_any(),
        Icon::Forward => view! { <path d="m9 18 6-6-6-6" /> }.into_any(),
        Icon::Search => view! {
            <circle cx="11" cy="11" r="7" />
            <path d="m20 20-4-4" />
        }
        .into_any(),
        Icon::Check => view! { <path d="M20 6 9 17l-5-5" /> }.into_any(),
        Icon::Download => view! { <path d="M12 4v11M7 10l5 5 5-5M5 20h14" /> }.into_any(),
        Icon::Share => view! { <path d="M12 15V4M7 9l5-5 5 5M5 13v7h14v-7" /> }.into_any(),
        Icon::Percent => view! {
            <path d="M19 5 5 19" />
            <circle cx="6.5" cy="6.5" r="2.5" />
            <circle cx="17.5" cy="17.5" r="2.5" />
        }
        .into_any(),
        Icon::Calendar => view! {
            <rect x="4" y="5" width="16" height="16" rx="2" />
            <path d="M8 3v4M16 3v4M4 10h16" />
        }
        .into_any(),
        Icon::Copy => view! {
            <rect x="9" y="9" width="11" height="11" rx="2" />
            <path d="M15 9V5a1 1 0 0 0-1-1H5a1 1 0 0 0-1 1v9a1 1 0 0 0 1 1h4" />
        }
        .into_any(),
    };
    view! {
        <svg
            class="icon"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2.5"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
        >
            {shape}
        </svg>
    }
}
