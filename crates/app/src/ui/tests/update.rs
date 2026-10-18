//! Браузерные тесты окна новой версии (задача 67). Service worker есть
//! только в выпускной сборке, и о новой версии браузер здесь не сообщит:
//! за него говорит тест (`Program::pretend`), а окно рисуется со своей
//! плашкой, с формой-заглушкой и со стилями сайта. Настоящую новую версию —
//! service worker, его кэш и «Обновить» — проверяет `tools/browser/check.mjs`
//! на собранном сайте: нажатие здесь перезагрузило бы страницу тестов.

use kvartik_core::i18n::{Key, Lang, t};
use kvartik_core::release::{Notes, Version};
use leptos::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::wasm_bindgen_test;
use web_sys::{Element, HtmlElement};

use super::{button, document, leftovers, sleep, wait_for};
use crate::platform::pwa::Event;
use crate::ui::form::Forms;
use crate::ui::pwa::{Program, UpdateDialog, UpdateNotice};

/// Заметки: пять версий новее этой — у самой свежей по короткому пункту
/// на каждом языке, у остальных по два длинных, чтобы список не поместился
/// на экран, — и сама эта: её окно показывать не должно, она уже работает.
fn notes() -> Notes {
    let current = Version::CURRENT.to_string();
    let mut numbers = current.split('.').map(|n| n.parse::<u32>().expect("число"));
    let (major, minor, patch) = (
        numbers.next().expect("major"),
        numbers.next().expect("minor"),
        numbers.next().expect("patch"),
    );
    let long = |text: &str| {
        let item = format!("{text} ").repeat(8);
        format!(r#"["{0}", "{0}"]"#, item.trim_end())
    };
    let mut entries = Vec::new();
    for step in (1..=5).rev() {
        let version = Version::new(major, minor, patch + step);
        let items = if step == 5 {
            r#""hy": ["Նոր կետ"], "ru": ["Новый пункт"], "en": ["A new item"]"#.to_owned()
        } else {
            format!(
                r#""hy": {}, "ru": {}, "en": {}"#,
                long("Երկար կետ, որ ցանկը չտեղավորվի էկրանին։"),
                long("Длинный пункт, чтобы список не поместился на экран."),
                long("A long item so that the list does not fit on the screen."),
            )
        };
        entries.push(format!(r#"{{"version": "{version}", "date": "2026-10-19", {items}}}"#));
    }
    entries.push(format!(
        r#"{{"version": "{current}", "date": "2026-10-18", "hy": ["Հին կետ"], "ru": ["Старый пункт"], "en": ["An old item"]}}"#
    ));
    Notes::parse(&format!(r#"{{"notes": [{}]}}"#, entries.join(", "))).expect("заметки читаются")
}

/// Форма-заглушка: отмечается в открытых формах, как настоящая.
#[component]
fn FormStub(forms: Forms) -> impl IntoView {
    forms.hold();
    view! {
        <form class="stub">
            <input type="text" />
        </form>
    }
}

fn find(host: &HtmlElement, selector: &str) -> Option<Element> {
    host.query_selector(selector).ok().flatten()
}

/// Окно новой версии ждёт открытой формы — до тех пор над экраном слова без
/// кнопки, — а с закрытой встаёт поверх всего: программа под ним
/// неактивна, фокус на его заголовке, в «Что нового» — только версии новее
/// работающей, на языке окна, и всё окно — на экране: длинный список
/// прокручивается внутри, а «Обновить» видна. Посторонняя весть окно не
/// пересобирает. Без заметок окно говорит о новой версии без списка,
/// включённая в другой вкладке — «Перезагрузить», а сборка не новее
/// работающей окна не поднимает.
#[wasm_bindgen_test]
async fn the_update_window_waits_for_forms_and_blocks_the_rest() {
    let document = document();
    let body = document.body().expect("есть <body>");
    let style = document.create_element("style").expect("создаётся");
    style.set_text_content(Some(concat!(
        include_str!("../../../style/tokens.css"),
        include_str!("../../../style/base.css"),
    )));
    body.append_child(&style).expect("вставляется");
    let host: HtmlElement = document.create_element("div").expect("создаётся").unchecked_into();
    body.append_child(&host).expect("вставляется");

    let (lang, set_lang) = signal(Lang::Ru);
    let program = Program::new();
    let forms = Forms::new();
    let open = RwSignal::new(true);
    let mounted = leptos::mount::mount_to(host.clone(), move || {
        view! {
            <div class="under" inert=move || program.blocks(forms)>
                <UpdateNotice lang program forms />
                {move || open.get().then(|| view! { <FormStub forms /> })}
            </div>
            <UpdateDialog lang program forms />
        }
    });
    let under = find(&host, ".under").expect("окно под окном новой версии");
    let ru = |key| t(Lang::Ru, key);
    let mut wrong = Vec::new();

    sleep(50).await;
    if find(&host, "[role=dialog]").is_some() || host.text_content().unwrap_or_default().contains(ru(Key::UpdateAfterForm)) {
        wrong.push("новой версии нет, а окно или слова о ней есть".to_owned());
    }

    // Пришла, пока открыта форма: только слова над экраном, без кнопки.
    program.pretend(Event::UpdateReady, Some(notes()));
    if !wait_for(&host, ru(Key::UpdateAfterForm)).await {
        wrong.push("с открытой формой нет слов «откроется, когда закроете форму»".to_owned());
    }
    if find(&host, "[role=dialog]").is_some() || under.has_attribute("inert") {
        wrong.push("с открытой формой окно новой версии уже встало".to_owned());
    }
    if button(&host, &[ru(Key::UpdateApply)]).is_some() {
        wrong.push("у слов над экраном есть «Обновить» — нажатая, она стёрла бы форму".to_owned());
    }

    // Форму закрыли — окно встаёт, программа под ним неактивна.
    open.set(false);
    if !wait_for(&host, ru(Key::UpdateTitle)).await {
        wrong.push("форму закрыли, а окна новой версии нет".to_owned());
    }
    sleep(50).await;
    let dialog = find(&host, "[role=dialog]").map(|d| d.text_content().unwrap_or_default()).unwrap_or_default();
    for (needed, what) in [
        (ru(Key::UpdateWhatsNew), "«Что нового»"),
        ("Новый пункт", "пункта новой версии"),
        (ru(Key::UpdateApply), "«Обновить»"),
        (ru(Key::UpdateKeepsData), "слов о данных"),
    ] {
        if !dialog.contains(needed) {
            wrong.push(format!("в окне нет {what}: {dialog}"));
        }
    }
    if dialog.contains("Старый пункт") {
        wrong.push("в окне — заметка работающей версии".to_owned());
    }
    if !under.has_attribute("inert") {
        wrong.push("программа под окном не неактивна".to_owned());
    }
    if host.text_content().unwrap_or_default().contains(ru(Key::UpdateAfterForm)) {
        wrong.push("окно встало, а слова «откроется, когда закроете форму» остались".to_owned());
    }
    let focused = document.active_element().map(|element| element.id());
    if focused.as_deref() != Some("update-title") {
        wrong.push(format!("фокус не на заголовке окна, а на {focused:?}"));
    }
    let window = web_sys::window().expect("есть окно");
    let (width, height) = (
        window.inner_width().ok().and_then(|w| w.as_f64()).unwrap_or(0.0),
        window.inner_height().ok().and_then(|h| h.as_f64()).unwrap_or(0.0),
    );
    // Карточка и кнопка меряются от слоя окна: он закреплён во весь экран,
    // а сама карточка не позиционирована.
    if let (Some(card), Some(apply)) = (find(&host, ".update-card"), button(&host, &[ru(Key::UpdateApply)])) {
        let card: HtmlElement = card.unchecked_into();
        let (left, right) = (card.offset_left(), card.offset_left() + card.offset_width());
        let bottom = apply.offset_top() + apply.offset_height();
        if left < 0 || f64::from(right) > width || f64::from(bottom) > height || apply.offset_height() < 40 {
            wrong.push(format!(
                "окно не на экране {width}×{height}: карточка {left}–{right}, кнопка снизу {bottom} высотой {}",
                apply.offset_height()
            ));
        }
    } else {
        wrong.push("нет карточки окна или кнопки «Обновить»".to_owned());
    }

    // Весть браузера, которая окна не убирает, его не пересобирает: фокус,
    // ушедший на кнопку, на ней и остаётся.
    if let Some(apply) = button(&host, &[ru(Key::UpdateApply)]) {
        let _ = apply.focus();
        program.pretend(Event::UpdateReady, Some(notes()));
        sleep(50).await;
        let focused = document.active_element().and_then(|element| element.text_content());
        if focused.as_deref().map(str::trim) != Some(ru(Key::UpdateApply)) {
            wrong.push(format!("заметки перечитаны — фокус ушёл с «Обновить» на {focused:?}"));
        }
    }

    // Язык окна сменили — окно на новом языке целиком.
    for (next, item) in [(Lang::En, "A new item"), (Lang::Am, "Նոր կետ")] {
        set_lang.set(next);
        sleep(50).await;
        let text = find(&host, "[role=dialog]").and_then(|d| d.text_content()).unwrap_or_default();
        if !text.contains(t(next, Key::UpdateTitle)) || !text.contains(item) {
            wrong.push(format!("{next:?}: окно не перевелось — {text}"));
        }
        let left = leftovers(&text, next);
        if !left.is_empty() {
            wrong.push(format!("{next:?}: строки других языков в окне — {left:?}"));
        }
    }
    set_lang.set(Lang::Ru);

    // Включили в другой вкладке — «Перезагрузить» и слова почему.
    program.pretend(Event::UpdatedElsewhere, Some(notes()));
    sleep(50).await;
    if !wait_for(&host, ru(Key::UpdateElsewhere)).await || button(&host, &[ru(Key::Reload)]).is_none() {
        wrong.push("включённая в другой вкладке — без слов о ней или без «Перезагрузить»".to_owned());
    }

    // Заметки не прочитались — окно без списка, но с кнопкой.
    program.pretend(Event::UpdateReady, None);
    sleep(50).await;
    let bare = find(&host, "[role=dialog]").and_then(|d| d.text_content()).unwrap_or_default();
    if bare.contains(ru(Key::UpdateWhatsNew)) || button(&host, &[ru(Key::Reload)]).is_none() {
        wrong.push(format!("без заметок окно не то: {bare}"));
    }

    // Пришла сборка не новее работающей — прежняя, которую хостинг ещё
    // отдаёт после выкладки: окна нет, программа работает.
    let stale = format!(
        r#"{{"notes": [{{"version": "{}", "date": "2026-10-18", "ru": ["Старый пункт"]}}]}}"#,
        Version::CURRENT
    );
    program.pretend(Event::UpdateReady, Some(Notes::parse(&stale).expect("заметки читаются")));
    sleep(50).await;
    if find(&host, "[role=dialog]").is_some() || under.has_attribute("inert") {
        wrong.push("пришла сборка не новее работающей, а окно стоит".to_owned());
    }

    drop(mounted);
    host.remove();
    style.remove();
    assert!(wrong.is_empty(), "окно новой версии: {wrong:#?}");
}
