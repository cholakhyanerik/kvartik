//! Свои цвета на странице (задача 88): палитра человека — переменными на
//! `<html>` — и тема устройства. Что за палитра, решает ядро
//! (`kvartik_core::theme`), а здесь — только поставить её и снять.
//!
//! Переменные — через CSSOM (`style.setProperty`): атрибут `style`
//! в разметке CSP сайта не пускает, а CSSOM — пускает (CLAUDE.md,
//! Правило 6). Свойство на самом `<html>` сильнее обеих тем `tokens.css`,
//! и тёмная тема устройства палитру человека не перебивает — так и задумано
//! (решение человека 2026-11-09). С ней — `color-scheme`: полосы прокрутки
//! и поля дат браузер рисует в тон выбранной палитре, а не теме устройства.
//!
//! Первый кадр, до wasm, палитру берёт у service worker (задача 89): окно
//! кладёт копию палитры стилем (`kvartik_core::theme::Sheet`) в кэш
//! [`COPY_CACHE`], а service worker вписывает в страницу программы ссылку на
//! неё с её отметкой (`crates/app/public/sw.js`). Своего скрипта в странице
//! нет — встроенный CSP не пускает (Правило 13), — а стиль приходит из кэша,
//! без круга сети (Правило 1). Окно, встав, ставит ту же палитру на `<html>`
//! само ([`put`]) — она сильнее любого стиля, — а ссылку снимает часть
//! настроек после запуска ([`take_copy`]). Копия — не в базе устройства
//! `kvartik-device`, а в Cache Storage: его service worker и так читает на
//! каждом переходе, а соединение с базой из service worker мешало бы её новой
//! версии (Правило 6, «Старая вкладка молча держит базу»). Первый заход
//! и браузер без service worker — без ссылки: страница в цветах программы,
//! как до 0.0.156.

use js_sys::{Object, Reflect};
use kvartik_core::theme::{Paint, Scheme, Sheet};
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{Cache, CssStyleDeclaration, HtmlElement, MediaQueryList, Response, ResponseInit};

/// Кэш копии палитры. Не сборки: имя не начинается с `kvartik-`, и новая
/// версия программы, встав, его не удаляет (`activate` в `sw.js`), — иначе
/// первый кадр после каждого обновления был бы в цветах программы. Те же
/// имена — в `sw.js`, и разойтись им не даёт тест ниже.
pub const COPY_CACHE: &str = "kvartik.look";
/// Адрес копии — в папке программы, от адреса страницы, как его видит
/// service worker от своего.
const COPY_FILE: &str = "look.css";
/// Тема и отметка копии — заголовками её ответа: service worker ставит тему
/// мета-тегу `color-scheme`, а отметку — ссылке (`data-look`).
const COPY_SCHEME: &str = "Kvartik-Scheme";
const COPY_MARK: &str = "Kvartik-Mark";

/// Вопрос о тёмной теме устройства.
fn dark_query() -> Option<MediaQueryList> {
    web_sys::window()?.match_media("(prefers-color-scheme: dark)").ok().flatten()
}

fn scheme_of(query: Option<&MediaQueryList>) -> Scheme {
    if query.is_some_and(MediaQueryList::matches) {
        Scheme::Dark
    } else {
        Scheme::Light
    }
}

/// Тема устройства сейчас (`prefers-color-scheme`). Не сказало — светлая:
/// так у стилей, тёмная там только по запросу.
pub fn device_scheme() -> Scheme {
    scheme_of(dark_query().as_ref())
}

/// Слежка за темой устройства: пока ручка жива, `changed` зовётся с новой
/// темой. Устройство меняет её само — вечером, по расписанию, — и экран,
/// показанный однажды, иначе остался бы с цветами прежней. Событие `change`
/// у `matchMedia`, а не опрос: в покое работы нет (Правило 1).
pub struct SchemeWatch {
    query: MediaQueryList,
    callback: Closure<dyn FnMut()>,
}

impl Drop for SchemeWatch {
    fn drop(&mut self) {
        let _ = self
            .query
            .remove_event_listener_with_callback("change", self.callback.as_ref().unchecked_ref());
    }
}

/// Следить за темой устройства; `None` — браузер о ней не говорит.
pub fn watch_scheme(changed: impl Fn(Scheme) + 'static) -> Option<SchemeWatch> {
    let query = dark_query()?;
    let callback = Closure::<dyn FnMut()>::new({
        let query = query.clone();
        move || changed(scheme_of(Some(&query)))
    });
    query
        .add_event_listener_with_callback("change", callback.as_ref().unchecked_ref())
        .ok()?;
    Some(SchemeWatch { query, callback })
}

/// Поставить палитру из записи устройства при открытии
/// (`kvartik_core::theme::painted`): пары «переменная — цвет» как есть, до
/// первой отрисовки.
pub fn put(scheme: Scheme, colors: &[(&str, &str)]) {
    let Some(style) = root_style() else {
        return;
    };
    for &(name, color) in colors {
        let _ = style.set_property(name, color);
    }
    let _ = style.set_property("color-scheme", scheme.code());
}

/// Кружок у курсора — атрибутом `data-cursor` на `<html>`: рисовать ли его,
/// решают стили (`style/base.css`), а не разметка, и выключенный не
/// рисуется вовсе.
pub fn show_cursor(on: bool) {
    let Some(root) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.document_element())
    else {
        return;
    };
    let _ = if on {
        root.set_attribute("data-cursor", "")
    } else {
        root.remove_attribute("data-cursor")
    };
}

/// Поставить палитру на страницу — или, `None`, снять её: окно снова в цветах
/// программы и в теме устройства. Переменные новой палитры ставятся поверх
/// прежних, без снятия: между ними окно не мигает цветами программы.
pub fn paint(palette: Option<&Paint>) {
    let Some(style) = root_style() else {
        return;
    };
    match palette {
        Some(palette) => {
            for (name, color) in palette.colors.iter() {
                let _ = style.set_property(name, &color.hex());
            }
            let _ = style.set_property("color-scheme", palette.scheme.code());
        }
        None => {
            // Снимаются все переменные `<html>`: других, кроме палитры, окно
            // ему не ставит (точку мыши `ui/motion.rs` пишет на кнопках
            // и карточках). С конца: снятое сдвигает номера следующих.
            for at in (0..style.length()).rev() {
                let name = style.item(at);
                if name.starts_with("--") {
                    let _ = style.remove_property(&name);
                }
            }
            let _ = style.remove_property("color-scheme");
            // И копия первого кадра, если часть ещё не сняла её ссылку:
            // иначе палитра осталась бы на странице стилем.
            let _ = take_copy();
        }
    }
}

/// Стиль самого `<html>`; `None` — документа нет.
fn root_style() -> Option<CssStyleDeclaration> {
    let root = web_sys::window()?.document()?.document_element()?;
    Some(root.unchecked_into::<HtmlElement>().style())
}

/// Ссылка на копию палитры, которую вписал service worker: снять её и
/// сказать её отметку; `None` — ссылки нет. Зовёт часть настроек после
/// запуска и [`paint`] без палитры: дальше палитру держит окно, и «Вернуть
/// цвета программы» не осталось бы под стилем копии. Копия без записи
/// устройства (убрать её не удалось) видна до загрузки части — секунду-другую
/// на открытии — и потом снимается вместе с самой копией. Пустая отметка —
/// ссылка не та, и копию часть настроек перепишет.
pub fn take_copy() -> Option<String> {
    let link = web_sys::window()?
        .document()?
        .query_selector("link[data-look]")
        .ok()
        .flatten()?;
    let mark = link.get_attribute("data-look").unwrap_or_default();
    link.remove();
    Some(mark)
}

/// Положить копию палитры в кэш [`COPY_CACHE`] — её отдаст service worker
/// при следующем открытии. Не вышло — не беда: открытие начнётся в цветах
/// программы, а при следующем запуске часть настроек увидит, что отметка у
/// страницы не та, и запишет снова. Кэша нет без HTTPS (`[SecureContext]`):
/// тогда нет и service worker.
pub async fn keep_copy(sheet: &Sheet) {
    let Some(caches) = web_sys::window().and_then(|window| window.caches().ok()) else {
        return;
    };
    let headers = Object::new();
    for (name, value) in [
        ("Content-Type", "text/css; charset=utf-8"),
        (COPY_SCHEME, sheet.scheme.code()),
        (COPY_MARK, sheet.mark.as_str()),
    ] {
        let _ = Reflect::set(&headers, &name.into(), &value.into());
    }
    // Заголовки — через `Reflect`, а не сеттером `ResponseInit`: каждый
    // импорт JS держит главный wasm, даже когда его зовёт только часть
    // (замер задачи 89), а `Reflect::set` в нём уже есть.
    let init = ResponseInit::new();
    let _ = Reflect::set(&init, &"headers".into(), &headers);
    let Ok(response) = Response::new_with_opt_str_and_init(Some(&sheet.css), &init) else {
        return;
    };
    let Ok(cache) = caches.open(COPY_CACHE).await else {
        return;
    };
    let _ = cache.unchecked_into::<Cache>().put_with_str(COPY_FILE, &response).await;
}

/// Убрать копию палитры: своих цветов больше нет, и первый кадр — в цветах
/// программы.
pub async fn drop_copy() {
    if let Some(caches) = web_sys::window().and_then(|window| window.caches().ok()) {
        let _ = caches.delete(COPY_CACHE).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Имена копии палитры у окна и у service worker — одни: разойдись они,
    /// service worker не нашёл бы копию, и ошибки не было бы нигде.
    #[test]
    fn the_service_worker_knows_the_copy() {
        let sw = include_str!("../../public/sw.js");
        for needed in [
            format!(r#"const LOOK = "{COPY_CACHE}";"#),
            format!(r#"const SHEET = "{COPY_FILE}";"#),
            format!(r#"get("{COPY_SCHEME}")"#),
            format!(r#"get("{COPY_MARK}")"#),
            "data-look=".to_owned(),
        ] {
            assert!(sw.contains(&needed), "в sw.js нет {needed}");
        }
        assert!(!COPY_CACHE.starts_with("kvartik-"), "кэш копии удалила бы новая версия");
    }
}
