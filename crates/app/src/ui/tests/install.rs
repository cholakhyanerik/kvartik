//! Раздел «Установка» на главном экране (задача 55): кнопка «Установить»
//! есть всегда, пока программа не установлена, — вопрос браузера, когда он
//! есть, и слова, как установить в этом браузере, когда его нет.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use js_sys::{Array, Function, Object, Promise, Reflect};
use kvartik_core::i18n::{Key, Lang, t};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_test::wasm_bindgen_test;
use web_sys::HtmlElement;

use super::{mount, mount_at, sleep, wait_for};
use crate::platform::pwa::Browser;
use crate::store::device::Device;
use crate::ui::pwa::manual_words;

/// Вопрос браузера, подложенный тестом: событие `beforeinstallprompt`,
/// у которого `prompt()` считает вызовы и ждёт, пока тест не ответит за
/// человека ([`FakePrompt::dismiss`]). Событие — обычное `Event`:
/// `BeforeInstallPromptEvent` есть только у Chromium, а окно зовёт
/// `prompt` и `userChoice` как свойства.
struct FakePrompt {
    calls: Rc<Cell<u32>>,
    answer: Rc<RefCell<Option<Function>>>,
    prompt: Closure<dyn FnMut() -> Promise>,
}

impl FakePrompt {
    fn new() -> Self {
        let calls = Rc::new(Cell::new(0));
        let answer = Rc::new(RefCell::new(None::<Function>));
        let prompt = Closure::<dyn FnMut() -> Promise>::new({
            let (calls, answer) = (calls.clone(), answer.clone());
            move || {
                calls.set(calls.get() + 1);
                Promise::new(&mut |resolve, _| {
                    answer.replace(Some(resolve));
                })
            }
        });
        Self {
            calls,
            answer,
            prompt,
        }
    }

    /// Прислать окну вопрос — как браузер.
    fn send(&self) {
        let window = web_sys::window().expect("есть окно");
        let init = Object::new();
        Reflect::set(&init, &"cancelable".into(), &JsValue::TRUE).expect("ставится");
        let constructor: Function = Reflect::get(&window, &"Event".into())
            .expect("есть Event")
            .unchecked_into();
        let event = Reflect::construct(
            &constructor,
            &Array::of2(&"beforeinstallprompt".into(), &init),
        )
        .expect("событие создаётся");
        let choice = Object::new();
        Reflect::set(&choice, &"outcome".into(), &"dismissed".into()).expect("ставится");
        Reflect::set(&event, &"prompt".into(), self.prompt.as_ref()).expect("ставится");
        Reflect::set(&event, &"userChoice".into(), &Promise::resolve(&choice)).expect("ставится");
        window
            .dispatch_event(event.unchecked_ref())
            .expect("событие доставлено");
    }

    /// Человек закрыл вопрос браузера.
    fn dismiss(&self) {
        if let Some(resolve) = self.answer.borrow_mut().take() {
            resolve.call0(&JsValue::NULL).expect("ответ дан");
        }
    }
}

/// Кнопка «Установить» главного экрана на языке `lang`.
fn install_button(host: &HtmlElement, lang: Lang) -> Option<HtmlElement> {
    let buttons = host.query_selector_all("#home .install button").ok()?;
    (0..buttons.length())
        .filter_map(|at| buttons.item(at))
        .map(|node| node.unchecked_into::<HtmlElement>())
        .find(|button| {
            button.text_content().unwrap_or_default().trim() == t(lang, Key::InstallButton)
        })
}

/// Текст раздела «Установка»; пусто — раздела нет.
fn section_text(host: &HtmlElement) -> String {
    host.query_selector("#home .install")
        .ok()
        .flatten()
        .and_then(|section| section.text_content())
        .unwrap_or_default()
}

/// Кнопка «Установить» — когда появится; `None` — не дождались за пять
/// секунд.
async fn wait_install(host: &HtmlElement, lang: Lang) -> Option<HtmlElement> {
    for _ in 0..100 {
        if let Some(found) = install_button(host, lang) {
            return Some(found);
        }
        sleep(50).await;
    }
    None
}

/// Кнопка есть с первого кадра, без вопроса браузера, и раскрывает слова
/// этого браузера с подсказкой о данных, а второе нажатие их прячет.
/// Раскрытые слова переводятся со всем окном. Пришёл вопрос браузера —
/// кнопка задаёт его, а слов не раскрывает; человек отказался, а браузер
/// прислал новый вопрос (так Chromium) — кнопка снова задаёт вопрос; нового
/// нет — снова раскрывает слова.
#[wasm_bindgen_test]
async fn install_is_always_offered() {
    let device = Device::new();
    device.set_ui_lang(Lang::Ru);
    let (steps, hint) = manual_words(Browser::detect());
    let fake = FakePrompt::new();
    let (host, mounted) = mount();
    let mut wrong = Vec::new();

    let expanded = |button: &HtmlElement| button.get_attribute("aria-expanded");
    match wait_install(&host, Lang::Ru).await {
        None => wrong.push("нет кнопки «Установить» без вопроса браузера".to_owned()),
        Some(install) => {
            if expanded(&install).as_deref() != Some("false")
                || section_text(&host).contains(t(Lang::Ru, steps))
            {
                wrong.push(format!(
                    "слова раскрыты до нажатия: {:?}",
                    expanded(&install)
                ));
            }
            install.click();
            sleep(50).await;
            let text = section_text(&host);
            if expanded(&install).as_deref() != Some("true")
                || !text.contains(t(Lang::Ru, steps))
                || !text.contains(t(Lang::Ru, hint))
            {
                wrong.push(format!("нажатие не раскрыло слов этого браузера: {text}"));
            }

            match host
                .query_selector(".langs button[lang=\"en\"]")
                .ok()
                .flatten()
            {
                Some(english) => english.unchecked_into::<HtmlElement>().click(),
                None => wrong.push("нет кнопки английского языка".to_owned()),
            }
            sleep(50).await;
            let text = section_text(&host);
            if !text.contains(t(Lang::En, steps)) || text.contains(t(Lang::Ru, steps)) {
                wrong.push(format!("слова не перевелись: {text}"));
            }
            install.click();
            sleep(50).await;
            if expanded(&install).as_deref() != Some("false")
                || section_text(&host).contains(t(Lang::En, steps))
            {
                wrong.push("второе нажатие не спрятало слов".to_owned());
            }
        }
    }

    fake.send();
    sleep(50).await;
    match install_button(&host, Lang::En) {
        None => wrong.push("с вопросом браузера кнопки нет".to_owned()),
        Some(install) => {
            if expanded(&install).is_some() {
                wrong.push("кнопка с вопросом браузера — раскрывающая".to_owned());
            }
            install.click();
            sleep(50).await;
            if fake.calls.get() != 1 || !install.has_attribute("disabled") {
                wrong.push(format!("вопрос не задан: вызовов {}", fake.calls.get()));
            }
            // Отказ, и браузер прислал новый вопрос до ответа.
            fake.send();
            fake.dismiss();
            sleep(50).await;
            if install.has_attribute("disabled") || expanded(&install).is_some() {
                wrong.push("после отказа с новым вопросом кнопка не задаёт его".to_owned());
            }
            install.click();
            sleep(50).await;
            fake.dismiss();
            sleep(50).await;
            if fake.calls.get() != 2 || expanded(&install).as_deref() != Some("false") {
                wrong.push(format!(
                    "после отказа без нового вопроса кнопка не раскрывает слов: вызовов {}, {:?}",
                    fake.calls.get(),
                    expanded(&install)
                ));
            }
        }
    }

    drop(mounted);
    host.remove();
    if let Some(storage) =
        web_sys::window().and_then(|window| window.local_storage().ok().flatten())
    {
        let _ = storage.remove_item("kvartik.lang");
    }
    assert!(wrong.is_empty(), "установка: {wrong:#?}");
}

/// «Установка» на главном — ниже карточек налога, сроков и счетов, которые
/// ждут оплаты (полный прогон 2026-11-10, п. 21): в Chrome программа
/// с экрана «Домой» видит данные браузера, и ставить её первой незачем, а на
/// телефоне она отодвигала налог и счета. С главного экрана она не уходит.
#[wasm_bindgen_test]
async fn install_comes_after_the_cards_where_data_is_shared() {
    super::documents::fresh_base_with_client().await;
    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let (host, mounted) = mount_at("#/");
    let mut wrong = Vec::new();
    if !wait_for(&host, t(lang, Key::HomeAwaiting)).await {
        wrong.push("на главном нет «Ждут оплаты»".to_owned());
    }
    let install = host.query_selector("#home .install").ok().flatten();
    let awaiting = host.query_selector_all("#home h2").ok().and_then(|headings| {
        (0..headings.length())
            .filter_map(|at| headings.item(at))
            .find(|heading| heading.text_content().as_deref() == Some(t(lang, Key::HomeAwaiting)))
    });
    match (install, awaiting) {
        (Some(install), Some(awaiting)) => {
            if awaiting.compare_document_position(&install) & web_sys::Node::DOCUMENT_POSITION_FOLLOWING == 0 {
                wrong.push("«Установка» выше «Ждут оплаты»".to_owned());
            }
        }
        (None, _) => wrong.push("на главном нет «Установки»".to_owned()),
        (_, None) => wrong.push("на главном нет заголовка «Ждут оплаты»".to_owned()),
    }
    drop(mounted);
    host.remove();
    crate::platform::set_hash("");
    super::documents::forget_device();
    assert!(wrong.is_empty(), "место «Установки»: {wrong:#?}");
}

/// Вкладка Safari с данными (задача 124): установленная программа их не
/// увидит, и карточка главного экрана ведёт перенести их кодом — ссылкой на
/// «Другое устройство». Там, где программа видит данные вкладки, карточки
/// нет. Без данных её нет и так: карточки главного экрана рисует часть книги,
/// а её грузят, только когда данные есть (`ui/tax.rs`, `TaxCard`).
#[wasm_bindgen_test]
async fn install_offers_to_carry_the_tab_data() {
    use leptos::prelude::*;

    use crate::ui::pwa::CarryCard;
    let lang = Lang::Ru;
    let mut wrong = Vec::new();
    let cases = [
        (Browser::SafariTouch, true),
        (Browser::OtherTouch, true),
        (Browser::SafariMac, true),
        (Browser::Chrome, false),
        (Browser::ChromeAndroid, false),
    ];
    for (browser, offered) in cases {
        let page = web_sys::window().and_then(|window| window.document()).expect("документ есть");
        let host: HtmlElement = page.create_element("div").expect("создаётся").unchecked_into();
        page.body().expect("есть <body>").append_child(&host).expect("вставляется");
        let mounted = leptos::mount::mount_to(host.clone(), move || {
            let (lang, _) = signal(lang);
            view! { <CarryCard lang browser /> }
        });
        sleep(50).await;
        let link = host.query_selector(r##"a[href="#/sync"]"##).ok().flatten();
        let said = host.text_content().unwrap_or_default().contains(t(lang, Key::InstallCarry));
        if link.is_some() != offered || said != offered {
            wrong.push(format!("{browser:?}: ссылка {}, слова {said}", link.is_some()));
        }
        drop(mounted);
        host.remove();
    }
    assert!(wrong.is_empty(), "перенос данных вкладки: {wrong:#?}");
}
