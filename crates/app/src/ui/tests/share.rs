//! Браузерные тесты «Поделиться» (задача 13) для файла копии и подмена Web
//! Share — для них и для теста PDF (`documents.rs`). Настоящее меню
//! headless Chrome не откроет, а в Chrome на Linux, где идёт CI, Web Share
//! нет вовсе: тесты проверяют, что окно отдаёт меню и как слушает ответ.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use js_sys::{Array, Object, Promise, Reflect};
use kvartik_core::i18n::{Key, Lang, t};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_test::wasm_bindgen_test;
use web_sys::{DomException, File, HtmlElement};

use super::{button, field, mount_at, sleep, type_into, wait_for};
use crate::store::device::Device;

/// Подмена Web Share на время теста: `canShare` отвечает [`ShareStub::can`],
/// `share` запоминает данные и отвечает [`ShareStub::answer`] — успехом
/// (`None`) или исключением с этим именем. Подмена — свои свойства
/// `navigator` поверх свойств прототипа: при сбросе они удаляются, и снова
/// видны настоящие.
pub(super) struct ShareStub {
    pub(super) can: Rc<Cell<bool>>,
    pub(super) answer: Rc<Cell<Option<&'static str>>>,
    calls: Rc<RefCell<Vec<JsValue>>>,
    _can_share: Closure<dyn Fn(JsValue) -> bool>,
    _share: Closure<dyn Fn(JsValue) -> Promise>,
}

/// Свойства `navigator`, которые подменяет [`ShareStub`].
const STUBBED: [&str; 4] = ["canShare", "share", "standalone", "maxTouchPoints"];

impl ShareStub {
    pub(super) fn install() -> Self {
        let navigator = web_sys::window().expect("есть окно").navigator();
        let can = Rc::new(Cell::new(true));
        let answer = Rc::new(Cell::new(None::<&'static str>));
        let calls = Rc::new(RefCell::new(Vec::new()));
        let can_share = Closure::<dyn Fn(JsValue) -> bool>::new({
            let can = can.clone();
            move |_| can.get()
        });
        let share = Closure::<dyn Fn(JsValue) -> Promise>::new({
            let (answer, calls) = (answer.clone(), calls.clone());
            move |data: JsValue| {
                calls.borrow_mut().push(data);
                match answer.get() {
                    None => Promise::resolve(&JsValue::UNDEFINED),
                    Some(name) => Promise::reject(
                        &DomException::new_with_message_and_name("подмена", name)
                            .expect("исключение")
                            .into(),
                    ),
                }
            }
        });
        Reflect::set(&navigator, &"canShare".into(), can_share.as_ref()).expect("canShare подменён");
        Reflect::set(&navigator, &"share".into(), share.as_ref()).expect("share подменён");
        Self {
            can,
            answer,
            calls,
            _can_share: can_share,
            _share: share,
        }
    }

    /// Сколько раз позван `share`.
    pub(super) fn calls(&self) -> usize {
        self.calls.borrow().len()
    }

    /// Что отдано меню `n`-м вызовом: имена полей данных и файлы.
    pub(super) fn shared(&self, n: usize) -> Option<(Vec<String>, Vec<File>)> {
        let calls = self.calls.borrow();
        let data = calls.get(n)?;
        let keys = Object::keys(data.unchecked_ref::<Object>())
            .iter()
            .filter_map(|key| key.as_string())
            .collect();
        let files = Reflect::get(data, &"files".into())
            .ok()
            .and_then(|files| files.dyn_into::<Array>().ok())
            .map(|files| files.iter().filter_map(|file| file.dyn_into().ok()).collect())
            .unwrap_or_default();
        Some((keys, files))
    }

    /// Притвориться iPhone: `navigator.standalone` и пять точек касания —
    /// по ним окно узнаёт iPhone и iPad (`platform::pwa::apple_touch`).
    pub(super) fn pretend_iphone(&self) {
        let navigator = web_sys::window().expect("есть окно").navigator();
        for (name, value) in [("standalone", JsValue::FALSE), ("maxTouchPoints", JsValue::from(5))] {
            let descriptor = Object::new();
            Reflect::set(&descriptor, &"value".into(), &value).expect("значение");
            Reflect::set(&descriptor, &"configurable".into(), &JsValue::TRUE).expect("удаляемо");
            Reflect::define_property(&navigator, &name.into(), &descriptor).expect("подменено");
        }
    }
}

impl Drop for ShareStub {
    fn drop(&mut self) {
        if let Some(window) = web_sys::window() {
            let navigator = window.navigator();
            for name in STUBBED {
                let _ = Reflect::delete_property(&navigator, &name.into());
            }
        }
    }
}

/// Убрать с устройства язык, пароль копий и день последней копии.
fn forget_device() {
    if let Some(storage) = web_sys::window().and_then(|window| window.local_storage().ok().flatten())
    {
        for name in ["kvartik.lang", "kvartik.backup_key", "kvartik.last_backup"] {
            let _ = storage.remove_item(name);
        }
    }
}

/// Кнопка с этой подписью — когда появится; `None` — не дождались за пять
/// секунд.
async fn wait_button(host: &HtmlElement, label: &str) -> Option<HtmlElement> {
    for _ in 0..100 {
        if let Some(found) = button(host, &[label]) {
            return Some(found);
        }
        sleep(50).await;
    }
    None
}

/// Файл копии на iPhone и iPad (задача 13): рядом со «Сохранить» —
/// «Отправить или сохранить в Files», и она отдаёт меню один файл копии
/// в самом нажатии, до первого `await`. Отданный файл — сделанная копия,
/// закрытое меню — нет. Вне iPhone и iPad кнопки нет, хотя `canShare`
/// и отвечает «можно»: Chromium отказывает файлу `.json` уже в `share`.
#[wasm_bindgen_test]
async fn backup_goes_to_the_share_menu_on_iphone_only() {
    // С данными: в пустой базе «Сделать копию» спрятана (полный прогон
    // 2026-11-10, п. 12) — копировать нечего.
    super::documents::fresh_base_with_client().await;
    forget_device();
    let lang = Lang::En;
    let device = Device::new();
    device.set_ui_lang(lang);
    let sharing = ShareStub::install();
    let (host, mounted) = mount_at("#/backup");
    let mut wrong = Vec::new();
    let password = "Пароль копии задачи 13";

    // Компьютер: новый пароль, копия — и только «Сохранить».
    match wait_button(&host, t(lang, Key::BackupMake)).await {
        Some(make) => {
            for label in [Key::BackupNewPassword, Key::BackupRepeatPassword] {
                match field(&host, t(lang, label), 0) {
                    Some(input) => type_into(&input, password),
                    None => wrong.push(format!("нет поля {label:?}")),
                }
            }
            make.click();
        }
        None => wrong.push("нет «Сделать копию»".to_owned()),
    }
    let ready = t(lang, Key::BackupReady).split("{}").next().unwrap_or_default();
    if !wait_for(&host, ready).await {
        wrong.push("копия не собралась".to_owned());
    }
    if button(&host, &[t(lang, Key::BackupShare)]).is_some() {
        wrong.push("«Отправить» у копии не на iPhone".to_owned());
    }

    // iPhone: пароль уже задан — копия снова, и рядом с «Сохранить» меню.
    sharing.pretend_iphone();
    match field(&host, t(lang, Key::BackupPassword), 0) {
        Some(input) => type_into(&input, password),
        None => wrong.push("нет поля пароля копий".to_owned()),
    }
    match button(&host, &[t(lang, Key::BackupMake)]) {
        Some(make) => make.click(),
        None => wrong.push("нет «Сделать копию» второй раз".to_owned()),
    }
    let send = wait_button(&host, t(lang, Key::BackupShare)).await;
    match &send {
        Some(send) => {
            sharing.answer.set(Some("AbortError"));
            send.click();
            // Ни одного `await` после нажатия: меню открыто в нём самом.
            if sharing.calls() != 1 {
                wrong.push(format!("в самом нажатии share позван {} раз", sharing.calls()));
            }
        }
        None => wrong.push("на iPhone у копии нет «Отправить»".to_owned()),
    }
    sleep(100).await;
    let said = host.text_content().unwrap_or_default();
    if device.last_backup().is_some() || said.contains(t(lang, Key::BackupShared)) {
        wrong.push("закрытое меню засчитано копией".to_owned());
    }
    if let Some((keys, files)) = sharing.shared(0) {
        let file = files.first();
        let name = file.map(File::name).unwrap_or_default();
        if keys != ["files"] || files.len() != 1 {
            wrong.push(format!("отдано меню: {keys:?}, файлов {}", files.len()));
        }
        if !name.starts_with("kvartik-") || !name.ends_with(".json") {
            wrong.push(format!("имя файла копии — {name}"));
        }
        if file.is_some_and(|file| file.size() < 100.0) {
            wrong.push("файл копии пуст".to_owned());
        }
    }
    sharing.answer.set(None);
    if let Some(send) = &send {
        send.click();
    }
    if !wait_for(&host, t(lang, Key::BackupShared)).await {
        wrong.push("отданный файл — а слов нет".to_owned());
    }
    if device.last_backup().is_none() {
        wrong.push("отданный файл не засчитан копией".to_owned());
    }
    drop(mounted);
    drop(sharing);
    host.remove();
    forget_device();
    assert!(wrong.is_empty(), "копия в «Поделиться»: {wrong:#?}");
}
