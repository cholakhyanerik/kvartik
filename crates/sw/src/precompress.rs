//! Заранее сжатый wasm программы (задача 109).
//!
//! Cloudflare сжимает ответы сайта сам, но слабо: каждый файл, который отдал
//! kvartik.com 2026-11-18, по проводу ближе всего к brotli уровня 4 — и wasm
//! мимо кэша CDN, и CSS и JS из него. Главный wasm 0.0.153 — 281 260 Б,
//! а уровнем 11 — 232 277, и рабочий экран телефона на замере — 3,12 с против
//! 2,84: за целью в 3 с (CLAUDE.md, Правило 1). Уровень у Cloudflare не
//! выбирают, а заранее сжатых файлов Pages сам не отдаёт (docs Cloudflare,
//! «Compression» и «Compression Rules», 2026-11-18). Поэтому рядом с каждым
//! wasm программы ложится его копия уровнем 11 — `<имя>.wasm.br`, — а на адрес
//! самого wasm её отдаёт функция сайта (`public/root/_worker.js`,
//! `_routes.json`). В кэш service worker она не идёт: он берёт wasm по его
//! адресу, а `.br` — только тело ответа.
//!
//! Только у выпускной сборки: отладочную отдаёт `trunk serve` без функции,
//! а её wasm в разы больше. Только wasm: у клея JS и CSS разница — 4 КБ на
//! всех, а каждый адрес функции — её вызов на первом заходе каждого человека
//! (Workers Free — 100 000 в день).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

use brotli::enc::BrotliEncoderParams;

/// Что дописывается к имени wasm.
pub const SUFFIX: &str = ".br";

/// Уровень brotli: самый плотный, сжатие — один раз на сборку.
const QUALITY: i32 = 11;

/// Окно — 4 МиБ, как у `zlib` Node по умолчанию: самый большой wasm сайта
/// (часть книги) — 1,3 МБ, а окно больше 16 МиБ браузеры не принимают
/// (RFC 7932: у `br` HTTP — до 24).
const WINDOW: i32 = 22;

/// Сколько файлов сжимается разом, не больше числа ядер. Кодер уровня 11
/// с окном 4 МиБ держит десятки мегабайт, и восемь разом — до трети
/// гигабайта даже на малой машине CI.
const THREADS: usize = 8;

/// Рядом с каждым `*.wasm` папки `app` — его копия уровнем 11. Возвращает
/// число сжатых файлов.
pub fn write(app: &Path) -> Result<usize, String> {
    let mut wasm = Vec::new();
    let entries = fs::read_dir(app).map_err(|error| format!("{}: {error}", app.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("{}: {error}", app.display()))?;
        let path = entry.path();
        let is_file = entry
            .file_type()
            .map_err(|error| format!("{}: {error}", path.display()))?
            .is_file();
        if is_file && path.extension().is_some_and(|extension| extension == "wasm") {
            wasm.push(path);
        }
    }
    wasm.sort();
    let next = AtomicUsize::new(0);
    let workers = thread::available_parallelism().map_or(1, usize::from).clamp(1, THREADS);
    let failures: Vec<String> = thread::scope(|scope| {
        let handles: Vec<_> = (0..workers.min(wasm.len()))
            .map(|_| {
                scope.spawn(|| {
                    let mut failures = Vec::new();
                    while let Some(path) = wasm.get(next.fetch_add(1, Ordering::Relaxed)) {
                        if let Err(error) = write_one(path) {
                            failures.push(error);
                        }
                    }
                    failures
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|handle| handle.join().unwrap_or_else(|_| vec!["сжатие wasm упало".to_owned()]))
            .collect()
    });
    if failures.is_empty() {
        Ok(wasm.len())
    } else {
        Err(failures.join("; "))
    }
}

fn write_one(path: &Path) -> Result<(), String> {
    let bytes = fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let packed = pack(&bytes).map_err(|error| format!("{}: сжатие: {error}", path.display()))?;
    let mut out = path.as_os_str().to_owned();
    out.push(SUFFIX);
    let out = PathBuf::from(out);
    fs::write(&out, packed).map_err(|error| format!("{}: {error}", out.display()))
}

/// Байты, сжатые brotli уровня 11. Тот же вход — те же байты: сборка
/// повторяема (CLAUDE.md, Правило 6).
fn pack(bytes: &[u8]) -> std::io::Result<Vec<u8>> {
    let params = BrotliEncoderParams {
        quality: QUALITY,
        lgwin: WINDOW,
        size_hint: bytes.len(),
        ..BrotliEncoderParams::default()
    };
    let mut out = Vec::new();
    brotli::BrotliCompress(&mut &bytes[..], &mut out, &params)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unpack(bytes: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        brotli::BrotliDecompress(&mut &bytes[..], &mut out).expect("brotli разжимается");
        out
    }

    /// Похоже на wasm: повторы с вариациями, а не одни нули.
    fn sample(len: usize) -> Vec<u8> {
        (0..len).map(|i| ((i * 7919) % 251) as u8 ^ ((i / 1024) % 3) as u8).collect()
    }

    #[test]
    fn packs_back_to_the_same_bytes_and_the_same_way_twice() {
        let bytes = sample(300_000);
        let packed = pack(&bytes).unwrap();
        assert!(packed.len() < bytes.len() / 4, "{} из {}", packed.len(), bytes.len());
        assert_eq!(unpack(&packed), bytes);
        assert_eq!(pack(&bytes).unwrap(), packed);
        assert_eq!(unpack(&pack(b"").unwrap()), b"");
    }

    #[test]
    fn writes_a_copy_next_to_each_wasm_and_nothing_else() {
        let dir = std::env::temp_dir().join(format!("kvartik-sw-br-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("fonts")).unwrap();
        let main = sample(50_000);
        fs::write(dir.join("kvartik-app-1a2b_bg.wasm"), &main).unwrap();
        fs::write(dir.join("split_book-1a2b.wasm"), b"part").unwrap();
        fs::write(dir.join("kvartik-app-1a2b.js"), b"glue").unwrap();
        fs::write(dir.join("fonts/a.wasm"), b"deep").unwrap();
        assert_eq!(write(&dir), Ok(2));
        let mut names: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        assert_eq!(
            names,
            [
                "fonts",
                "kvartik-app-1a2b.js",
                "kvartik-app-1a2b_bg.wasm",
                "kvartik-app-1a2b_bg.wasm.br",
                "split_book-1a2b.wasm",
                "split_book-1a2b.wasm.br",
            ]
        );
        assert_eq!(unpack(&fs::read(dir.join("kvartik-app-1a2b_bg.wasm.br")).unwrap()), main);
        assert_eq!(unpack(&fs::read(dir.join("split_book-1a2b.wasm.br")).unwrap()), b"part");
        assert!(!dir.join("fonts/a.wasm.br").exists());
        fs::remove_dir_all(&dir).unwrap();
    }
}
