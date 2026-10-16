//! Части окна в отдельных файлах wasm (задача 58).
//!
//! Главный wasm несёт то, что нужно рабочему экрану, а экраны, карточка
//! документа и действия, которые нужны только по нажатию, лежат отдельными
//! частями: первый заход
//! скачивает и компилирует меньше (Правило 1; замер — CLAUDE.md, 0.0.18).
//! Режет wasm хук сборки (`crates/sw`), а здесь — точки разреза: функции,
//! объявленные [`split!`]. Её вызов — `async`: при первом вызове окно
//! загружает часть, потом она уже в памяти.
//!
//! Без сети части приходят из кэша service worker: он кладёт туда весь сайт
//! (Правило 2). Заранее, пока человек смотрит на экран, окно загружает их
//! в память ([`preload`]), — но только из этого кэша: на первом заходе
//! service worker и так скачивает их в фоне, и второе скачивание делило бы
//! узкий канал со шрифтами. До того часть грузится по нажатию, и окно
//! говорит «Загрузка…» ([`part`]).
//!
//! Так же — содержимое спрятанных экранов ([`screen`], задача 61): первому
//! заходу нужен только главный экран, и счета, книга, клиенты и профиль
//! рисуются с первого показа. Форма экрана — в его же части, а не в своей:
//! программа разреза кладёт общий код частей в кусок на каждый свой набор
//! частей, которым он нужен, и экраны со своими формами — двенадцать частей —
//! дали 112 кусков вместо 44. Пока service worker ставил лишние файлы по
//! шести соединениям, вкладка книги, нажатая на первом заходе, ждала 7,0 с
//! вместо 3,4 (замер задачи 61; CLAUDE.md, Правило 6). Без экрана форму не
//! открыть, и своя часть ей ничего не даёт.
//!
//! Не загрузилась (нет сети, а кэша ещё нет) — функция возвращает `Err`,
//! и окно говорит это словами, а следующее нажатие пробует снова. `#[lazy]`
//! из Leptos так не умеет: он паникует (`expect` в `wasm_split_helpers`
//! 0.2.3), и человек увидел бы плашку сбоя.
//!
//! Тестовый wasm никто не режет (`wasm-bindgen-test-runner` ничего не знает
//! о частях), а неразрезанный не запускается вовсе (CLAUDE.md, Правило 6).
//! Поэтому в тестах функция из [`split!`] — обычная `async fn` того же
//! вида: окно в тестах то же, только без частей.

use std::future::Future;
use std::pin::Pin;

use kvartik_core::i18n::{Key, Lang};
use leptos::prelude::*;
use leptos::wasm_split::SplitLoaderError;

use super::route::{Router, Screen};
use super::tr;

/// Работа из части, которую ждут после загрузки (сборка копии,
/// восстановление): через границу части `async fn` не передаётся, а будущее
/// в коробке — да.
pub type Task<T> = Pin<Box<dyn Future<Output = T>>>;

/// Функция в своей части wasm. Возвращает `Result` с
/// [`SplitLoaderError`]: часть может не загрузиться. Аргументы и результат
/// идут через границу части как `extern "C"`, поэтому функция не `async`
/// и без обобщений; работу после загрузки она отдаёт [`Task`].
///
/// `preload` — имя функции, которая только загружает часть (её зовёт
/// [`preload`]).
///
/// Имя функции — имя части: хук сборки называет файл `split_<имя>.wasm`.
/// Поэтому имя у каждой части своё на всё окно: две функции с одним
/// именем в разных модулях программа разреза молча кладёт в одну часть
/// (проверено 2026-10-13: три `form` легли в одну часть на 214 КБ).
macro_rules! split {
    (
        $(#[$meta:meta])*
        $vis:vis fn $name:ident($($arg:ident: $ty:ty),* $(,)?) -> $ret:ty, preload $preload:ident
        $body:block
    ) => {
        $(#[$meta])*
        #[cfg(not(test))]
        #[::leptos::wasm_split::wasm_split(
            $name,
            wasm_split_path = ::leptos::wasm_split,
            preload($preload),
            fallible
        )]
        $vis fn $name($($arg: $ty),*)
            -> ::core::result::Result<$ret, ::leptos::wasm_split::SplitLoaderError>
        {
            ::core::result::Result::Ok($body)
        }

        // Видимость нужна функции загрузки — её зовёт [`preload`], — а самой
        // функции хватит её модуля: в ней типы этого модуля. Вне тестов
        // `wasm_split` даёт обеим одну видимость.
        $(#[$meta])*
        #[cfg(test)]
        async fn $name($($arg: $ty),*)
            -> ::core::result::Result<$ret, ::leptos::wasm_split::SplitLoaderError>
        {
            ::core::result::Result::Ok($body)
        }

        #[cfg(test)]
        $vis async fn $preload()
            -> ::core::result::Result<(), ::leptos::wasm_split::SplitLoaderError>
        {
            ::core::result::Result::Ok(())
        }
    };
}
pub(super) use split;

/// Вид из части: пока она грузится — «Загрузка…», не загрузилась — слова
/// и «Отмена» (`back` — куда вернуться). Загруженная часть рисуется сразу,
/// в том же кадре.
pub fn part(
    lang: ReadSignal<Lang>,
    content: impl Future<Output = Result<AnyView, SplitLoaderError>> + Send + 'static,
    back: impl Fn() + Copy + Send + Sync + 'static,
) -> AnyView {
    let loading = RwSignal::new(true);
    view! {
        {move || loading.get().then(|| view! {
            <p role="status">{tr(lang, Key::PartLoading)}</p>
        })}
        {Suspend::new(async move {
            let loaded = content.await;
            loading.set(false);
            match loaded {
                Ok(content) => content,
                Err(_) => view! {
                    <p role="alert">{tr(lang, Key::PartFailed)}</p>
                    <div class="actions">
                        <button type="button" class="secondary" on:click=move |_| back()>
                            {tr(lang, Key::Cancel)}
                        </button>
                    </div>
                }
                .into_any(),
            }
        })}
    }
    .into_any()
}

/// Содержимое экрана `page` — частью wasm с первого показа (задача 61):
/// первому заходу нужен только главный экран, а каждый экран в главном
/// wasm — около 45 КБ после brotli и четверть секунды рабочего экрана на
/// телефоне (CLAUDE.md, Правило 1). Заголовок экрана — вне части: на него
/// встаёт фокус после перехода (`route::shown`), а окно, открытое сразу на
/// этом экране, рисует часть уже в первом кадре — словами «Загрузка…», а не
/// пустотой. Показанный однажды экран больше не снимается (CLAUDE.md,
/// Правило 2): начатое в форме не пропадает при переходе. Часть не
/// загрузилась — «Отмена» ведёт на главный экран, и следующий показ
/// пробует снова.
pub fn screen<F>(
    lang: ReadSignal<Lang>,
    router: Router,
    page: Screen,
    content: impl Fn() -> F + Copy + Send + Sync + 'static,
) -> impl IntoView
where
    F: Future<Output = Result<AnyView, SplitLoaderError>> + Send + 'static,
{
    let shown = RwSignal::new(untrack(|| router.shows(page)));
    Effect::new(move |_| {
        if router.shows(page) && !shown.get_untracked() {
            shown.set(true);
        }
    });
    let back = move || {
        shown.set(false);
        router.go(Screen::Home);
    };
    move || shown.get().then(|| part(lang, content(), back))
}

/// Все части — в память. Зовёт окно, когда рабочий экран готов и страницей
/// правит service worker: части приходят из его кэша, без сети. Не
/// загрузилась — не беда: загрузится по нажатию. Экраны — первыми: вкладку
/// нажимают раньше, чем открывают форму.
pub async fn preload() {
    let _ = super::documents::preload_screen().await;
    let _ = super::ledger::preload_ledger().await;
    let _ = super::clients::preload_screen().await;
    let _ = super::profile::preload_screen().await;
    let _ = super::documents::preload_opened().await;
    let _ = super::paper::preload_paper().await;
    let _ = super::backup::preload_make().await;
    let _ = super::backup::preload_open().await;
    let _ = super::backup::preload_restore().await;
}
