//! Настройки (задача 88): кружок у курсора — только когда его включили,
//! и выбор помнит устройство; свои цвета встают на страницу сразу, остаются
//! после нового открытия окна, о плохо читаемой паре окно говорит числом,
//! а «Вернуть цвета программы» снимает всё.

use kvartik_core::i18n::{Key, Lang, t};
use kvartik_core::theme::{Pick, Record, Rgb};
use wasm_bindgen::JsCast;
use wasm_bindgen_test::wasm_bindgen_test;
use web_sys::{Event, HtmlElement, HtmlInputElement};

use super::{button, mount, mount_at, shown_screens, sleep};
use crate::platform::{look, set_hash};
use crate::store::device::Device;

/// Язык и настройки, запомненные тестом, — прочь, а страница — без своих
/// цветов: следующий тест открывается, как у нового человека.
fn forget() {
    let device = Device::new();
    device.set_cursor(false);
    device.clear_colors();
    look::paint(None);
    look::show_cursor(false);
    if let Some(storage) = web_sys::window().and_then(|window| window.local_storage().ok().flatten()) {
        let _ = storage.remove_item("kvartik.lang");
    }
    set_hash("");
}

/// Переменная на самом `<html>`: свои цвета окно ставит туда.
fn root_property(name: &str) -> String {
    web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.document_element())
        .map(|root| root.unchecked_into::<HtmlElement>().style())
        .and_then(|style| style.get_property_value(name).ok())
        .unwrap_or_default()
}

/// Стоит ли у страницы метка кружка: по ней стили его рисуют.
fn cursor_marked() -> bool {
    web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.document_element())
        .is_some_and(|root| root.has_attribute("data-cursor"))
}

/// Подождать, пока в окне есть элемент; `None` — не дождались за две секунды.
async fn element(host: &HtmlElement, selector: &str) -> Option<HtmlElement> {
    for _ in 0..100 {
        if let Some(found) = host.query_selector(selector).ok().flatten() {
            return Some(found.unchecked_into());
        }
        sleep(20).await;
    }
    None
}

/// Поле цвета строки `pick` — по порядку строк экрана.
fn color_field(host: &HtmlElement, pick: Pick) -> Option<HtmlInputElement> {
    let at = Pick::ALL.iter().position(|&own| own == pick)?;
    let fields = host.query_selector_all("#settings input[type=color]").ok()?;
    fields.item(u32::try_from(at).ok()?).map(JsCast::unchecked_into)
}

/// Выбрать цвет, как человек: поле получает значение, а окно — `input`.
/// Цвет — числами, а не `#…` литералом: цвет литералом в коде окна, тестов
/// тоже, ловит `tests/tokens.rs` (Правило 4).
fn choose(field: &HtmlInputElement, color: Rgb) {
    field.set_value(&color.hex());
    let event = Event::new("input").expect("событие создаётся");
    field.dispatch_event(&event).expect("событие доставлено");
}

/// Тёмно-синий фон и текст почти его цвета.
const NAVY: Rgb = Rgb::from_u32(0x0b1d33);
const DUSK: Rgb = Rgb::from_u32(0x13253b);

/// Кружка нет, пока его не включили; переключатель настроек ставит
/// и убирает его сразу — меткой страницы, по которой его рисуют стили, —
/// и помнит выбор на устройстве, и окно, открытое заново, его ставит.
#[wasm_bindgen_test]
async fn the_cursor_circle_follows_the_setting() {
    forget();
    Device::new().set_ui_lang(Lang::Ru);
    let (host, mounted) = mount();
    let mut wrong = Vec::new();
    for _ in 0..100 {
        if shown_screens(&host) == ["home"] {
            break;
        }
        sleep(10).await;
    }
    if cursor_marked() {
        wrong.push("кружок есть, хотя его не включали".to_owned());
    }
    set_hash("#/settings");
    let Some(switch) = element(&host, "#settings input[role=switch]").await else {
        drop(mounted);
        host.remove();
        forget();
        panic!("на экране настроек нет переключателя");
    };
    switch.click();
    sleep(50).await;
    if !cursor_marked() {
        wrong.push("включили — кружка нет".to_owned());
    }
    if !Device::new().cursor() {
        wrong.push("включили — устройство не помнит".to_owned());
    }
    // Окно открыто заново — кружок, каким его оставили.
    drop(mounted);
    host.remove();
    look::show_cursor(false);
    let (host, mounted) = mount_at("#/settings");
    if !cursor_marked() {
        wrong.push("после нового открытия кружка нет".to_owned());
    }
    let Some(switch) = element(&host, "#settings input[role=switch]").await else {
        drop(mounted);
        host.remove();
        forget();
        panic!("после нового открытия нет переключателя");
    };
    if !switch.unchecked_ref::<HtmlInputElement>().checked() {
        wrong.push("после нового открытия переключатель выключен".to_owned());
    }
    switch.click();
    sleep(50).await;
    if cursor_marked() {
        wrong.push("выключили — кружок остался".to_owned());
    }
    if Device::new().cursor() {
        wrong.push("выключили — устройство помнит включённым".to_owned());
    }
    drop(mounted);
    host.remove();
    forget();
    assert!(wrong.is_empty(), "кружок у курсора: {wrong:#?}");
}

/// Выбранный фон встаёт на страницу сразу — со всей палитрой тёмной темы
/// под тёмный фон, — устройство его помнит, и окно, открытое заново, ставит
/// его до первой отрисовки. Плохо читаемый выбранный текст назван числом
/// под своим цветом. «Вернуть цвета программы» снимает палитру со страницы
/// и с устройства.
#[wasm_bindgen_test]
async fn own_colors_paint_the_window_and_stay() {
    forget();
    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let (host, mounted) = mount_at("#/settings");
    let mut wrong = Vec::new();
    if element(&host, "#settings input[type=color]").await.is_none() {
        drop(mounted);
        host.remove();
        forget();
        panic!("на экране настроек нет полей цвета");
    }
    let field = |pick| color_field(&host, pick).expect("поле цвета есть");
    let (navy, dusk) = (NAVY.hex(), DUSK.hex());

    choose(&field(Pick::Background), NAVY);
    sleep(50).await;
    if root_property("--surface") != navy {
        wrong.push(format!("фон на странице — «{}»", root_property("--surface")));
    }
    if root_property("color-scheme") != "dark" {
        wrong.push(format!("тема под тёмный фон — «{}»", root_property("color-scheme")));
    }
    let remembered = Device::new().colors().and_then(|text| Record::parse(&text));
    if remembered.as_ref().and_then(|record| record.choice.get(Pick::Background)).map(|color| color.hex())
        != Some(navy.clone())
    {
        wrong.push(format!("устройство помнит {remembered:?}"));
    }
    // Выбранный фон — свой цвет, и у строки появилось «Сбросить».
    if !host
        .query_selector("#settings li.color-row")
        .ok()
        .flatten()
        .and_then(|row| row.text_content())
        .is_some_and(|text| text.contains(t(lang, Key::ColorOwn)) && text.contains(t(lang, Key::ColorReset)))
    {
        wrong.push("у фона нет «Свой цвет» и «Сбросить»".to_owned());
    }

    // Текст почти цвета фона — читается плохо, и окно это говорит.
    choose(&field(Pick::Text), DUSK);
    sleep(50).await;
    let low = host
        .query_selector("#settings p.color-low")
        .ok()
        .flatten()
        .and_then(|words| words.text_content())
        .unwrap_or_default();
    if !low.starts_with("Текст на фоне читается плохо: контраст 1,") || !low.ends_with("а нужен хотя бы 4,5.") {
        wrong.push(format!("о плохо читаемом тексте — «{low}»"));
    }
    if root_property("--text") != dusk {
        wrong.push(format!("выбранный текст на странице — «{}»", root_property("--text")));
    }
    drop(mounted);
    host.remove();

    // Окно открыто заново: палитра — с устройства, до первой отрисовки.
    look::paint(None);
    let (host, mounted) = mount();
    if root_property("--surface") != navy {
        wrong.push(format!("после нового открытия фон — «{}»", root_property("--surface")));
    }
    set_hash("#/settings");
    match element(&host, "#settings button.colors-reset").await {
        Some(_) => {
            if let Some(reset) = button(&host, &[t(lang, Key::ColorsResetAll)]) {
                reset.click();
            }
            sleep(50).await;
            if !root_property("--surface").is_empty() || !root_property("color-scheme").is_empty() {
                wrong.push(format!("вернули цвета программы — на странице «{}»", root_property("--surface")));
            }
            if Device::new().colors().is_some() {
                wrong.push("вернули цвета программы — устройство помнит свои".to_owned());
            }
        }
        None => wrong.push("нет «Вернуть цвета программы»".to_owned()),
    }
    drop(mounted);
    host.remove();
    forget();
    assert!(wrong.is_empty(), "свои цвета: {wrong:#?}");
}
