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
