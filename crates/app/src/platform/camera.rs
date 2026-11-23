//! Камера для QR-кода передачи изменений (задача 110): видео с камеры
//! в элементе `<video>` и кадр с него — яркостью, для сканера ядра
//! (`kvartik_core::format::qr::scan`).
//!
//! `navigator.mediaDevices` есть только в защищённом контексте — по HTTPS
//! и на `localhost` (по спецификации Media Capture and Streams,
//! `[SecureContext]`; MDN, `MediaDevices.getUserMedia()`, сверено
//! 2026-11-21): на `http://` по адресу сети камеры нет, как шифра. Просит
//! камеру только нажатие «Снять QR-код»: браузер спрашивает разрешение,
//! отказ — `NotAllowedError`, нет камеры — `NotFoundError`, занята другой
//! программой — `NotReadableError`. На iPhone видео играет в странице, только
//! если у `<video>` есть `playsinline`, — иначе Safari открывает его на весь
//! экран (MDN, `<video>`, «playsinline»).
//!
//! Вызовы — через `Reflect`, а не привязками `web-sys`: каждая новая
//! привязка — импорт JS главного wasm, даже когда её зовёт только часть
//! (`.claude/rules/pitfalls/wasm.md`, «Импорт JS — в главном wasm»), а
//! `Reflect` он уже импортирует. Видео и кадры — только пока сканер на экране:
//! снятый экран останавливает камеру ([`Camera`] гасит её при `drop`), и в
//! покое она не работает (Правило 1).

use js_sys::{Array, Function, Object, Promise, Reflect, Uint8Array};
use wasm_bindgen::{JsCast, JsValue};

/// Почему камеры нет.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CameraError {
    /// Человек или настройки браузера не дали камеру.
    Denied,
    /// Камеры нет, она занята или адрес не защищённый.
    Missing,
    /// Прочее; подробность — в консоли.
    Failed,
}

/// Включённая камера: поток, его `<video>` и холст для кадров.
pub struct Camera {
    stream: JsValue,
    video: JsValue,
    canvas: JsValue,
    context: JsValue,
}

/// Кадр яркостью — строками, по байту на точку.
pub struct Frame {
    pub width: usize,
    pub height: usize,
    pub gray: Vec<u8>,
}

/// Метод `name` объекта `target` с аргументами `args`.
fn call(target: &JsValue, name: &str, args: &[&JsValue]) -> Result<JsValue, JsValue> {
    let method = Reflect::get(target, &JsValue::from_str(name))?;
    if !method.is_function() {
        return Err(JsValue::from_str(&format!("{name} — не функция")));
    }
    let list = Array::new();
    for arg in args {
        list.push(arg);
    }
    Reflect::apply(&method.unchecked_into::<Function>(), target, &list)
}

/// Объект `{ключ: значение, …}`.
fn object(pairs: &[(&str, JsValue)]) -> JsValue {
    let object = Object::new();
    for (key, value) in pairs {
        let _ = Reflect::set(&object, &JsValue::from_str(key), value);
    }
    object.into()
}

/// Имя исключения DOM, если это оно.
fn name_of(error: &JsValue) -> Option<String> {
    Reflect::get(error, &JsValue::from_str("name")).ok()?.as_string()
}

impl Camera {
    /// Включить заднюю камеру (на компьютере — какая есть) в элементе `video`.
    pub async fn open(video: JsValue) -> Result<Self, CameraError> {
        let window = web_sys::window().ok_or(CameraError::Missing)?;
        let devices = Reflect::get(&window.navigator(), &JsValue::from_str("mediaDevices"))
            .map_err(|_| CameraError::Missing)?;
        if devices.is_undefined() || devices.is_null() {
            return Err(CameraError::Missing);
        }
        // Задняя камера телефона и HD: код на экране другого устройства
        // занимает в кадре треть, и ему нужны точки.
        let ideal = |value: JsValue| object(&[("ideal", value)]);
        let constraints = object(&[
            ("audio", JsValue::FALSE),
            (
                "video",
                object(&[
                    ("facingMode", ideal(JsValue::from_str("environment"))),
                    ("width", ideal(JsValue::from_f64(1280.0))),
                    ("height", ideal(JsValue::from_f64(720.0))),
                ]),
            ),
        ]);
        let started = call(&devices, "getUserMedia", &[&constraints]).map_err(|error| failure(&error))?;
        let stream = started
            .unchecked_into::<Promise>()
            .await
            .map_err(|error| failure(&error))?;
        // Холст — до видео: не вышло — поток гаснет сразу (`with`).
        let camera = Self::with(stream, video)?;
        // На iPhone видео без `playsinline` открылось бы на весь экран.
        for (name, value) in [("playsInline", JsValue::TRUE), ("muted", JsValue::TRUE), ("srcObject", camera.stream.clone())] {
            let _ = Reflect::set(&camera.video, &JsValue::from_str(name), &value);
        }
        if let Ok(playing) = call(&camera.video, "play", &[]) {
            // Отказ `play` — не беда: кадр всё равно приходит, когда видео
            // готово, а без кадра сканер ждёт.
            let _ = playing.unchecked_into::<Promise>().await;
        }
        Ok(camera)
    }

    /// Камера с холстом для кадров; холста нет — поток гаснет, а не горит
    /// лампочкой без сканера (ревью задачи 110).
    fn with(stream: JsValue, video: JsValue) -> Result<Self, CameraError> {
        let canvas = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.create_element("canvas").ok())
            .map(JsValue::from);
        // Кадры читаются часто: браузер держит холст в памяти, а не
        // в видеокарте (HTML, «willReadFrequently»).
        let options = object(&[("willReadFrequently", JsValue::TRUE)]);
        let context = canvas
            .as_ref()
            .and_then(|canvas| call(canvas, "getContext", &[&JsValue::from_str("2d"), &options]).ok())
            .filter(|context| !context.is_null() && !context.is_undefined());
        let (Some(canvas), Some(context)) = (canvas, context) else {
            stop(&stream);
            return Err(CameraError::Failed);
        };
        Ok(Self {
            stream,
            video,
            canvas,
            context,
        })
    }

    /// Кадр с камеры, уменьшенный до `max_side` точек по длинной стороне;
    /// `None` — видео ещё не готово.
    pub fn frame(&self, max_side: usize) -> Option<Frame> {
        let size = |name: &str| Reflect::get(&self.video, &JsValue::from_str(name)).ok()?.as_f64();
        let (width, height) = (size("videoWidth")?, size("videoHeight")?);
        if width < 1.0 || height < 1.0 {
            return None;
        }
        let scale = (max_side as f64 / width.max(height)).min(1.0);
        let (w, h) = ((width * scale).round().max(1.0), (height * scale).round().max(1.0));
        for (name, value) in [("width", w), ("height", h)] {
            if size_of(&self.canvas, name) != Some(value) {
                let _ = Reflect::set(&self.canvas, &JsValue::from_str(name), &JsValue::from_f64(value));
            }
        }
        let zero = JsValue::from_f64(0.0);
        let (jw, jh) = (JsValue::from_f64(w), JsValue::from_f64(h));
        call(&self.context, "drawImage", &[&self.video, &zero, &zero, &jw, &jh]).ok()?;
        let image = call(&self.context, "getImageData", &[&zero, &zero, &jw, &jh]).ok()?;
        let rgba = Uint8Array::new(&Reflect::get(&image, &JsValue::from_str("data")).ok()?).to_vec();
        // Яркость по BT.601 — целыми: 77, 150 и 29 в сумме 256.
        let gray = rgba
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| ((77 * u32::from(p[0]) + 150 * u32::from(p[1]) + 29 * u32::from(p[2])) >> 8) as u8)
            .collect();
        Some(Frame {
            width: w as usize,
            height: h as usize,
            gray,
        })
    }
}

impl Drop for Camera {
    /// Камера гаснет — и лампочка у неё тоже: дорожки потока остановлены,
    /// а видео отвязано.
    fn drop(&mut self) {
        stop(&self.stream);
        let _ = Reflect::set(&self.video, &JsValue::from_str("srcObject"), &JsValue::NULL);
    }
}

fn size_of(target: &JsValue, name: &str) -> Option<f64> {
    Reflect::get(target, &JsValue::from_str(name)).ok()?.as_f64()
}

/// Остановить все дорожки потока.
fn stop(stream: &JsValue) {
    if let Ok(tracks) = call(stream, "getTracks", &[]) {
        for track in Array::from(&tracks).iter() {
            let _ = call(&track, "stop", &[]);
        }
    }
}

/// Отказ браузера — словами окна; подробность — в консоль разработчику
/// (наружу ничего не уходит, Правило 13).
fn failure(error: &JsValue) -> CameraError {
    web_sys::console::error_2(&"Kvartik, камера:".into(), error);
    match name_of(error).as_deref() {
        Some("NotAllowedError" | "SecurityError") => CameraError::Denied,
        Some("NotFoundError" | "NotReadableError" | "OverconstrainedError" | "AbortError") => CameraError::Missing,
        _ => CameraError::Failed,
    }
}
