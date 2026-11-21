use super::read::{Unread, read};
use super::*;

/// Случайные байты без часов и без крейта: xorshift64 от своего зерна.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn byte(&mut self) -> u8 {
        (self.next() >> 24) as u8
    }

    fn bytes(&mut self, len: usize) -> Vec<u8> {
        (0..len).map(|_| self.byte()).collect()
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

const LEVELS: [Ecc; 4] = [Ecc::Low, Ecc::Medium, Ecc::Quartile, Ecc::High];

fn theirs(ecc: Ecc) -> qrcodegen::QrCodeEcc {
    match ecc {
        Ecc::Low => qrcodegen::QrCodeEcc::Low,
        Ecc::Medium => qrcodegen::QrCodeEcc::Medium,
        Ecc::Quartile => qrcodegen::QrCodeEcc::Quartile,
        Ecc::High => qrcodegen::QrCodeEcc::High,
    }
}

/// Код `qrcodegen` той же версии, уровня и маски.
fn reference(data: &[u8], version: u8, ecc: Ecc, mask: u8) -> qrcodegen::QrCode {
    let segments = [qrcodegen::QrSegment::make_bytes(data)];
    qrcodegen::QrCode::encode_segments_advanced(
        &segments,
        theirs(ecc),
        qrcodegen::Version::new(version),
        qrcodegen::Version::new(version),
        Some(qrcodegen::Mask::new(mask)),
        false,
    )
    .expect("данные по ёмкости")
}

/// Каждая версия и уровень, полная ёмкость и маска по кругу: свой код —
/// модуль в модуль тот же, что у независимого кодировщика. Так сверены
/// таблицы блоков, узоры, слова формата и версии, порядок модулей, маски
/// и байты проверки — числами не из этого кода.
#[test]
fn every_version_matches_an_independent_encoder() {
    let mut rng = Rng(0x5eed_0110);
    let mut wrong = Vec::new();
    for version in MIN_VERSION..=MAX_VERSION {
        for ecc in LEVELS {
            let data = rng.bytes(capacity(version, ecc));
            let mask = (version as usize + ecc.ordinal()) as u8 % 8;
            let ours = encode_masked(&data, version, ecc, Some(mask)).expect("помещается");
            let them = reference(&data, version, ecc, mask);
            let size = them.size() as usize;
            let same = ours.side() == size
                && (0..size).all(|y| (0..size).all(|x| ours.dark(x, y) == them.get_module(x as i32, y as i32)));
            if !same {
                wrong.push((version, ecc));
            }
        }
    }
    assert!(wrong.is_empty(), "расходятся с qrcodegen: {wrong:?}");
}

/// Ёмкость — та, что у независимого кодировщика: байт больше уже не
/// помещается ни у него, ни у нас.
#[test]
fn capacity_is_the_independent_one() {
    let mut wrong = Vec::new();
    for version in MIN_VERSION..=MAX_VERSION {
        for ecc in LEVELS {
            let fits = capacity(version, ecc);
            let over = vec![0u8; fits + 1];
            let theirs_refuse = qrcodegen::QrCode::encode_segments_advanced(
                &[qrcodegen::QrSegment::make_bytes(&over)],
                theirs(ecc),
                qrcodegen::Version::new(version),
                qrcodegen::Version::new(version),
                None,
                false,
            )
            .is_err();
            if !theirs_refuse || encode(&over, version, ecc).is_some() || encode(&over[1..], version, ecc).is_none() {
                wrong.push((version, ecc));
            }
        }
    }
    assert!(wrong.is_empty(), "ёмкость расходится: {wrong:?}");
}

/// Коды независимого кодировщика читаются обратно в те же байты — с любой
/// маской, которую он выбрал сам.
#[test]
fn independent_codes_read_back() {
    let mut rng = Rng(42);
    let mut wrong = Vec::new();
    for version in MIN_VERSION..=MAX_VERSION {
        for ecc in LEVELS {
            let len = rng.below(capacity(version, ecc) + 1);
            let data = rng.bytes(len);
            let them = qrcodegen::QrCode::encode_segments_advanced(
                &[qrcodegen::QrSegment::make_bytes(&data)],
                theirs(ecc),
                qrcodegen::Version::new(version),
                qrcodegen::Version::new(version),
                None,
                false,
            )
            .expect("помещается");
            let size = them.size() as usize;
            let dark = |x: usize, y: usize| them.get_module(x as i32, y as i32);
            if read(&dark, size) != Ok(data.clone()) {
                wrong.push((version, ecc));
            }
        }
    }
    assert!(wrong.is_empty(), "не прочитались: {wrong:?}");
}

/// Испорченные модули код исправляет: в каждом блоке до половины байтов
/// проверки — и читаются те же байты; а квадрат без слова формата не
/// читается вовсе.
#[test]
fn damaged_modules_are_corrected() {
    let mut rng = Rng(7);
    let data = rng.bytes(capacity(12, Ecc::Medium));
    let code = encode(&data, 12, Ecc::Medium).expect("помещается");
    let canvas = Canvas::new(12);
    let order = canvas.data_modules();
    // 24 испорченных модуля вразброс — меньше, чем по байту на блок сверх
    // предела: у 12-M восемь блоков по 22 байта проверки.
    let mut flipped = std::collections::BTreeSet::new();
    while flipped.len() < 24 {
        flipped.insert(order[rng.below(order.len())]);
    }
    let dark = |x: usize, y: usize| code.dark(x, y) ^ flipped.contains(&(x, y));
    assert_eq!(read(&dark, code.side()), Ok(data));
    let blank = |_: usize, _: usize| false;
    assert_eq!(read(&blank, code.side()), Err(Unread::No));
}

/// Реед — Соломон сам по себе: до ⌊n/2⌋ ошибок в случайных местах
/// исправляются в то же слово, а слово без ошибок не трогается.
#[test]
fn reed_solomon_corrects_up_to_half_the_check_bytes() {
    let mut rng = Rng(99);
    let mut wrong = 0;
    for round in 0..300 {
        let ecc = [7, 10, 18, 22, 26, 30][round % 6];
        let len = 1 + rng.below(120);
        let data = rng.bytes(len);
        let mut word = data.clone();
        word.extend(gf::remainder(&data, &gf::divisor(ecc)));
        let clean = word.clone();
        assert_eq!(gf::correct(&mut word.clone(), ecc), Some(0));
        let errors = rng.below(ecc / 2 + 1);
        let mut spots = std::collections::BTreeSet::new();
        while spots.len() < errors {
            spots.insert(rng.below(word.len()));
        }
        for &spot in &spots {
            word[spot] ^= 1 + rng.below(255) as u8;
        }
        if gf::correct(&mut word, ecc) != Some(errors) || word != clean {
            wrong += 1;
        }
    }
    assert_eq!(wrong, 0, "не исправилось слов: {wrong}");
}

/// Картинка кода: `scale` точек на модуль, четыре модуля светлого поля
/// вокруг, тёмное и светлое — как экран под камерой, а не чистые 0 и 255.
fn render(code: &Code, scale: usize) -> (usize, Vec<u8>) {
    let quiet = 4;
    let size = (code.side() + 2 * quiet) * scale;
    let mut pixels = vec![210u8; size * size];
    for y in 0..size {
        for x in 0..size {
            let (mx, my) = (x / scale, y / scale);
            if mx >= quiet && my >= quiet && code.dark(mx - quiet, my - quiet) {
                pixels[y * size + x] = 40;
            }
        }
    }
    (size, pixels)
}

/// Перспектива по четырём парам точек — своя, не сканера: проверка не
/// опирается на то, что проверяет.
fn homography(from: [(f64, f64); 4], to: [(f64, f64); 4]) -> impl Fn(f64, f64) -> (f64, f64) {
    let mut m = [[0f64; 9]; 8];
    for i in 0..4 {
        let ((x, y), (u, v)) = (from[i], to[i]);
        m[2 * i] = [x, y, 1.0, 0.0, 0.0, 0.0, -x * u, -y * u, u];
        m[2 * i + 1] = [0.0, 0.0, 0.0, x, y, 1.0, -x * v, -y * v, v];
    }
    for c in 0..8 {
        let p = (c..8).max_by(|&a, &b| m[a][c].abs().total_cmp(&m[b][c].abs())).unwrap();
        m.swap(c, p);
        for r in 0..8 {
            if r != c {
                let f = m[r][c] / m[c][c];
                for k in c..9 {
                    m[r][k] -= f * m[c][k];
                }
            }
        }
    }
    let h: Vec<f64> = (0..8).map(|i| m[i][8] / m[i][i]).collect();
    move |x, y| {
        let w = h[6] * x + h[7] * y + 1.0;
        ((h[0] * x + h[1] * y + h[2]) / w, (h[3] * x + h[4] * y + h[5]) / w)
    }
}

/// Снимок «камерой»: картинка кода в кадре `width`×`height`, углами в
/// `corners` (по часовой от левого верхнего), с размытием, шумом и светом,
/// который слабеет к краю, — и, если `mirror`, отражённая.
struct Shot {
    width: usize,
    height: usize,
    corners: [(f64, f64); 4],
    noise: u8,
    mirror: bool,
}

fn photograph(source: (usize, &[u8]), shot: &Shot, rng: &mut Rng) -> Vec<u8> {
    let (size, pixels) = source;
    let s = size as f64;
    let back = homography(shot.corners, [(0.0, 0.0), (s, 0.0), (s, s), (0.0, s)]);
    let mut out = vec![0u8; shot.width * shot.height];
    for y in 0..shot.height {
        for x in 0..shot.width {
            let fx = if shot.mirror { (shot.width - 1 - x) as f64 } else { x as f64 } + 0.5;
            let (u, v) = back(fx, y as f64 + 0.5);
            let value = if u >= 0.0 && v >= 0.0 && u < s - 1.0 && v < s - 1.0 {
                // Билинейно — края модулей мягкие, как у камеры.
                let (x0, y0) = (u.floor() as usize, v.floor() as usize);
                let (fu, fv) = (u - x0 as f64, v - y0 as f64);
                let p = |xx: usize, yy: usize| f64::from(pixels[yy * size + xx]);
                p(x0, y0) * (1.0 - fu) * (1.0 - fv)
                    + p(x0 + 1, y0) * fu * (1.0 - fv)
                    + p(x0, y0 + 1) * (1.0 - fu) * fv
                    + p(x0 + 1, y0 + 1) * fu * fv
            } else {
                150.0
            };
            // Свет слабеет к правому нижнему краю — на треть.
            let light = 1.0 - 0.33 * (x + y) as f64 / (shot.width + shot.height) as f64;
            let noise = f64::from(rng.byte() % (shot.noise.max(1))) - f64::from(shot.noise) / 2.0;
            out[y * shot.width + x] = (value * light + noise).clamp(0.0, 255.0) as u8;
        }
    }
    out
}

/// Код на экране под камерой читается: в кадре VGA и HD, повёрнутый,
/// наклонённый, с шумом и тенью, мелкий — до трёх точек на модуль — и
/// в зеркале фронтальной камеры. Провалы собираются и называются в конце.
#[test]
fn photographed_codes_are_read() {
    let mut rng = Rng(20_261_121);
    let shots = [
        Shot { width: 640, height: 480, corners: [(170.0, 90.0), (470.0, 90.0), (470.0, 390.0), (170.0, 390.0)], noise: 20, mirror: false },
        Shot { width: 640, height: 480, corners: [(220.0, 60.0), (520.0, 140.0), (440.0, 440.0), (140.0, 360.0)], noise: 30, mirror: false },
        Shot { width: 640, height: 480, corners: [(150.0, 120.0), (500.0, 80.0), (470.0, 420.0), (180.0, 380.0)], noise: 30, mirror: false },
        Shot { width: 800, height: 450, corners: [(500.0, 400.0), (300.0, 410.0), (290.0, 210.0), (490.0, 200.0)], noise: 20, mirror: false },
        Shot { width: 640, height: 480, corners: [(200.0, 100.0), (450.0, 120.0), (430.0, 380.0), (190.0, 360.0)], noise: 25, mirror: true },
        Shot { width: 1024, height: 576, corners: [(300.0, 100.0), (750.0, 90.0), (740.0, 500.0), (310.0, 520.0)], noise: 40, mirror: false },
        Shot { width: 640, height: 480, corners: [(260.0, 140.0), (500.0, 150.0), (495.0, 380.0), (255.0, 370.0)], noise: 15, mirror: false },
    ];
    let mut wrong = Vec::new();
    for (version, ecc) in [(2u8, Ecc::Medium), (6, Ecc::Medium), (12, Ecc::Medium), (12, Ecc::Low), (15, Ecc::Medium)] {
        let data = rng.bytes(capacity(version, ecc));
        let code = encode(&data, version, ecc).expect("помещается");
        let (size, pixels) = render(&code, 6);
        for (i, shot) in shots.iter().enumerate() {
            let frame = photograph((size, &pixels), shot, &mut rng);
            let image = Gray { width: shot.width, height: shot.height, pixels: &frame };
            if scan(&image) != vec![data.clone()] {
                wrong.push((version, i));
            }
        }
    }
    assert!(wrong.is_empty(), "не прочитались (версия, снимок): {wrong:?}");
}

/// Кадр без кода — шум, полосы, клетка, пустота — не даёт ничего и не
/// роняет сканер; кадр меньше кода или больше предела — тоже.
#[test]
fn frames_without_a_code_give_nothing() {
    let mut rng = Rng(5);
    let (w, h) = (320, 240);
    let noise: Vec<u8> = (0..w * h).map(|_| rng.byte()).collect();
    let stripes: Vec<u8> = (0..w * h).map(|i| if (i % w) / 3 % 2 == 0 { 30 } else { 220 }).collect();
    let checker: Vec<u8> = (0..w * h).map(|i| if ((i % w) / 4 + (i / w) / 4) % 2 == 0 { 30 } else { 220 }).collect();
    let flat = vec![128u8; w * h];
    for pixels in [&noise, &stripes, &checker, &flat] {
        assert!(scan(&Gray { width: w, height: h, pixels }).is_empty());
    }
    assert!(scan(&Gray { width: 10, height: 10, pixels: &[0; 100] }).is_empty());
    assert!(scan(&Gray { width: w, height: h, pixels: &[0; 10] }).is_empty());
    assert!(scan(&Gray { width: 4000, height: 1001, pixels: &[] }).is_empty());
}

/// Половина кода закрыта ладонью — кода нет, но и чужих байтов нет.
#[test]
fn a_covered_code_is_not_misread() {
    let mut rng = Rng(11);
    let data = rng.bytes(capacity(12, Ecc::Medium));
    let code = encode(&data, 12, Ecc::Medium).expect("помещается");
    let (size, mut pixels) = render(&code, 5);
    for y in size / 2..size {
        for x in size / 3..size {
            pixels[y * size + x] = 200;
        }
    }
    let found = scan(&Gray { width: size, height: size, pixels: &pixels });
    assert!(found.iter().all(|got| *got == data), "прочитаны чужие байты");
    assert!(found.is_empty());
}


/// Снимок со случайным поворотом, наклоном, размером и шумом: код занимает
/// от трети до трёх четвертей кадра, наклон — до четверти стороны.
fn random_shot(rng: &mut Rng, width: usize, height: usize, code_px: f64) -> Shot {
    loop {
        let shot = any_shot(rng, width, height, code_px);
        // Код целиком в кадре: вылезший за край не читает никто.
        if shot.corners.iter().all(|&(x, y)| x >= 0.0 && y >= 0.0 && x < width as f64 && y < height as f64) {
            return shot;
        }
    }
}

fn any_shot(rng: &mut Rng, width: usize, height: usize, code_px: f64) -> Shot {
    let (w, h) = (width as f64, height as f64);
    let angle = (rng.below(3600) as f64 / 10.0).to_radians();
    let (cx, cy) = (w / 2.0 + (rng.below(81) as f64 - 40.0), h / 2.0 + (rng.below(61) as f64 - 30.0));
    let r = code_px / std::f64::consts::SQRT_2;
    let mut corners = [(0.0, 0.0); 4];
    for (k, corner) in corners.iter_mut().enumerate() {
        let a = angle + std::f64::consts::FRAC_PI_2 * k as f64 - std::f64::consts::FRAC_PI_4 * 3.0;
        // Наклон: каждый угол сдвинут на долю стороны.
        let tilt = code_px * 0.12;
        let jitter = |rng: &mut Rng| (rng.below(1001) as f64 / 1000.0 - 0.5) * 2.0 * tilt;
        *corner = (cx + r * a.cos() + jitter(rng), cy + r * a.sin() + jitter(rng));
    }
    Shot { width, height, corners, noise: 10 + rng.below(31) as u8, mirror: rng.below(5) == 0 }
}

/// Код программы (версия 12, уровень M) на сотне случайных снимков VGA
/// читается почти всегда — от трёх с половиной точек на модуль. «Почти»:
/// случайный наклон углов изредка даёт трапецию, у которой дальняя сторона
/// втрое мельче ближней (так экран к камере не держат), а камера даёт десяток
/// кадров в секунду, и следующий кадр — другой.
#[test]
fn random_shots_of_the_program_code_are_read() {
    let mut rng = Rng(110);
    let data = rng.bytes(capacity(12, Ecc::Medium));
    let code = encode(&data, 12, Ecc::Medium).expect("помещается");
    let (size, pixels) = render(&code, 6);
    let mut wrong = Vec::new();
    for i in 0..100 {
        // 73 модуля с полем — от 260 до 420 точек кадра.
        let px = 260.0 + rng.below(161) as f64;
        let shot = random_shot(&mut rng, 640, 480, px);
        let frame = photograph((size, &pixels), &shot, &mut rng);
        if scan(&Gray { width: 640, height: 480, pixels: &frame }) != vec![data.clone()] {
            wrong.push((i, px as u32));
        }
    }
    assert!(wrong.len() <= 2, "не прочитались (снимок, точек): {wrong:?}");
}

/// Код, повёрнутый на 40–50°, читается: строка кадра пересекает узор поиска
/// по диагонали, и без поправки модуль казался бы в √2 раз крупнее,
/// а версия — на треть меньше (9 вместо 12, задача 110).
#[test]
fn codes_turned_by_45_degrees_are_read() {
    let mut rng = Rng(45);
    let data = rng.bytes(capacity(12, Ecc::Medium));
    let code = encode(&data, 12, Ecc::Medium).expect("помещается");
    let (size, pixels) = render(&code, 6);
    let mut wrong = Vec::new();
    for i in 0..10 {
        let angle = (40.0 + i as f64).to_radians();
        let (cx, cy, r) = (320.0, 240.0, 310.0 / std::f64::consts::SQRT_2);
        let mut corners = [(0.0, 0.0); 4];
        for (k, corner) in corners.iter_mut().enumerate() {
            let a = angle + std::f64::consts::FRAC_PI_2 * k as f64 - std::f64::consts::FRAC_PI_4 * 3.0;
            *corner = (cx + r * a.cos(), cy + r * a.sin());
        }
        let shot = Shot { width: 640, height: 480, corners, noise: 20, mirror: false };
        let frame = photograph((size, &pixels), &shot, &mut rng);
        if scan(&Gray { width: 640, height: 480, pixels: &frame }) != vec![data.clone()] {
            wrong.push(40 + i);
        }
    }
    assert!(wrong.is_empty(), "не прочитались под углом: {wrong:?}");
}

/// Сколько идёт разбор кадра — в выпускной сборке:
/// `cargo test -p kvartik-core --release --lib scan_speed -- --ignored --nocapture`.
#[test]
#[ignore = "замер, а не проверка"]
fn scan_speed() {
    let mut rng = Rng(1);
    let data = rng.bytes(capacity(12, Ecc::Medium));
    let code = encode(&data, 12, Ecc::Medium).expect("помещается");
    let (size, pixels) = render(&code, 6);
    for (width, height, px) in [(640usize, 480usize, 300.0), (1024, 576, 400.0)] {
        let shot = random_shot(&mut rng, width, height, px);
        let frame = photograph((size, &pixels), &shot, &mut rng);
        let empty = vec![128u8; width * height];
        let start = std::time::Instant::now();
        let rounds = 20;
        for _ in 0..rounds {
            assert_eq!(scan(&Gray { width, height, pixels: &frame }).len(), 1);
        }
        let with = start.elapsed() / rounds;
        let start = std::time::Instant::now();
        for _ in 0..rounds {
            assert!(scan(&Gray { width, height, pixels: &empty }).is_empty());
        }
        let without = start.elapsed() / rounds;
        eprintln!("{width}×{height}: с кодом {with:?}, без кода {without:?}");
    }
}


