//! Кадр камеры — в QR-коды: порог по окрестности, три узора поиска
//! 1:1:3:1:1 (п. 6.3.3), узор выравнивания у правого нижнего угла — для
//! перспективы, снятие модулей и чтение ([`super::read`]).
//!
//! Устроено, как у читателей, которые этим живут (ZXing, quirc): строки
//! кадра ищут отношение полос узора поиска, столбец и строка через его
//! середину его подтверждают, а тройка узоров с прямым углом даёт углы
//! кода. Код на экране другого устройства — плоский и контрастный, но
//! камера видит его под углом, с бликом и муаром пикселей экрана, поэтому
//! перспектива берётся по четырём точкам, а не по трём, а ошибки модулей
//! исправляет код Рида — Соломона.
//!
//! Кадр зеркальный (фронтальная камера ноутбука) читается тоже: у зеркала
//! узоры поиска идут в другую сторону, и квадрат снимается переставленным.

use super::read::{Unread, read};
use super::{MAX_VERSION, MIN_VERSION, alignment, side};

/// Кадр в оттенках серого: яркость каждого пикселя, строками.
#[derive(Clone, Copy, Debug)]
pub struct Gray<'a> {
    pub width: usize,
    pub height: usize,
    pub pixels: &'a [u8],
}

/// Больше пикселей сканер не берёт: суммы окрестностей — в `u32`, и кадр
/// в четыре мегапикселя — уже вдвое больше, чем нужно коду на экране
/// (окно уменьшает кадр камеры до 1024 точек по длинной стороне).
pub const MAX_PIXELS: usize = 4_000_000;

/// Сколько троек узоров сканер пробует на кадре: в кадре с узорами на
/// обоях и клавиатуре ложных узоров бывает десятки, а каждая тройка — снять
/// и прочитать квадрат.
const MAX_TRIES: usize = 16;

/// Сколько узоров поиска берётся в тройки — самые подтверждённые.
const MAX_FINDERS: usize = 12;

/// Коды, найденные в кадре: их байты, без повторов. Пусто — кода нет или
/// он не прочитался.
pub fn scan(image: &Gray<'_>) -> Vec<Vec<u8>> {
    let (width, height) = (image.width, image.height);
    if width < 21 || height < 21 || width.saturating_mul(height) > MAX_PIXELS || image.pixels.len() < width * height {
        return Vec::new();
    }
    let bits = Bits::of(image);
    let finders = finders(&bits);
    let mut found: Vec<Vec<u8>> = Vec::new();
    for corners in triples(&finders).into_iter().take(MAX_TRIES) {
        if let Some(data) = decode(&bits, &corners)
            && !found.contains(&data)
        {
            found.push(data);
        }
    }
    found
}

/// Кадр, разделённый на тёмное и светлое.
struct Bits {
    width: usize,
    height: usize,
    dark: Vec<bool>,
}

impl Bits {
    /// Порог — по окрестности каждого пикселя (средняя яркость квадрата со
    /// стороной в шестую часть кадра): блик и тень делают одну половину кода
    /// светлее другой, и общий порог потерял бы её. Тёмный — на 8 % темнее
    /// окрестности.
    fn of(image: &Gray<'_>) -> Self {
        let (width, height) = (image.width, image.height);
        let radius = (width.min(height) / 12).clamp(4, 96);
        let stride = width + 1;
        let mut sums = vec![0u32; stride * (height + 1)];
        for y in 0..height {
            let mut row = 0u32;
            for x in 0..width {
                row += u32::from(image.pixels[y * width + x]);
                sums[(y + 1) * stride + x + 1] = sums[y * stride + x + 1] + row;
            }
        }
        let mut dark = vec![false; width * height];
        for y in 0..height {
            let (top, bottom) = (y.saturating_sub(radius), (y + radius + 1).min(height));
            for x in 0..width {
                let (left, right) = (x.saturating_sub(radius), (x + radius + 1).min(width));
                let sum = u64::from(sums[bottom * stride + right]) + u64::from(sums[top * stride + left])
                    - u64::from(sums[top * stride + right])
                    - u64::from(sums[bottom * stride + left]);
                let count = ((right - left) * (bottom - top)) as u64;
                let pixel = u64::from(image.pixels[y * width + x]);
                dark[y * width + x] = pixel * count * 100 < sum * 92;
            }
        }
        Self { width, height, dark }
    }

    fn at(&self, x: usize, y: usize) -> bool {
        self.dark[y * self.width + x]
    }

    /// Тёмный ли пиксель под точкой; за краем — светлый.
    fn at_point(&self, point: Point) -> bool {
        let (x, y) = (point.x.floor(), point.y.floor());
        x >= 0.0 && y >= 0.0 && (x as usize) < self.width && (y as usize) < self.height && self.at(x as usize, y as usize)
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
struct Point {
    x: f64,
    y: f64,
}

impl Point {
    fn minus(self, other: Point) -> Point {
        Point {
            x: self.x - other.x,
            y: self.y - other.y,
        }
    }

    fn length(self) -> f64 {
        self.x.hypot(self.y)
    }
}

/// Узор поиска: середина, размер модуля и сколько строк его нашли.
#[derive(Clone, Copy, Debug)]
struct Finder {
    at: Point,
    module: f64,
    hits: u32,
}

/// Полосы 1:1:3:1:1 — размер модуля; `None` — не узор поиска. Каждая полоса —
/// в пределах половины модуля от своей доли, как у ZXing.
fn finder_module(counts: [usize; 5]) -> Option<f64> {
    let total: usize = counts.iter().sum();
    if total < 7 {
        return None;
    }
    let module = total as f64 / 7.0;
    let slack = module / 2.0;
    let near = |count: usize, share: f64| (count as f64 - share * module).abs() < share * slack;
    (near(counts[0], 1.0) && near(counts[1], 1.0) && near(counts[2], 3.0) && near(counts[3], 1.0) && near(counts[4], 1.0))
        .then_some(module)
}

/// Полосы вдоль линии через `center` — по `step` в обе стороны: тёмная
/// середина, светлое, тёмное. Сколько пикселей в каждой из пяти и где
/// середина центральной; `None` — линия ушла за край или полоса длиннее
/// `max`.
fn runs_through(bits: &Bits, center: (usize, usize), step: (isize, isize), max: usize) -> Option<([usize; 5], f64)> {
    let inside = |x: isize, y: isize| x >= 0 && y >= 0 && (x as usize) < bits.width && (y as usize) < bits.height;
    let dark = |x: isize, y: isize| bits.at(x as usize, y as usize);
    let (cx, cy) = (center.0 as isize, center.1 as isize);
    let mut counts = [0usize; 5];
    // Назад: середина, светлое, тёмное.
    let (mut x, mut y) = (cx, cy);
    for (slot, want) in [(2usize, true), (1, false), (0, true)] {
        while inside(x, y) && dark(x, y) == want && counts[slot] <= max {
            counts[slot] += 1;
            x -= step.0;
            y -= step.1;
        }
        if counts[slot] == 0 || counts[slot] > max || (slot != 0 && !inside(x, y)) {
            return None;
        }
    }
    let back = counts[2];
    // Вперёд: середина дальше, светлое, тёмное.
    let (mut x, mut y) = (cx + step.0, cy + step.1);
    while inside(x, y) && dark(x, y) && counts[2] <= max * 3 {
        counts[2] += 1;
        x += step.0;
        y += step.1;
    }
    for (slot, want) in [(3usize, false), (4, true)] {
        if !inside(x, y) {
            return None;
        }
        while inside(x, y) && dark(x, y) == want && counts[slot] <= max {
            counts[slot] += 1;
            x += step.0;
            y += step.1;
        }
        if counts[slot] == 0 || counts[slot] > max {
            return None;
        }
    }
    // Середина центральной полосы — от `center` назад на `back` и вперёд на
    // остаток.
    let offset = counts[2] as f64 / 2.0 - back as f64 + 1.0;
    Some((counts, offset))
}

/// Узоры поиска кадра: строки, в которых полосы 1:1:3:1:1, подтверждённые
/// столбцом и строкой через середину; совпавшие — один узор с числом строк.
fn finders(bits: &Bits) -> Vec<Finder> {
    let mut found: Vec<Finder> = Vec::new();
    for y in 0..bits.height {
        // Полосы строки: цвет и где кончаются.
        let mut runs: Vec<(bool, usize, usize)> = Vec::new();
        let mut start = 0;
        for x in 1..=bits.width {
            if x == bits.width || bits.at(x, y) != bits.at(start, y) {
                runs.push((bits.at(start, y), start, x - start));
                start = x;
            }
        }
        for window in runs.windows(5) {
            if !window[0].0 {
                continue;
            }
            let counts = [window[0].2, window[1].2, window[2].2, window[3].2, window[4].2];
            let Some(_) = finder_module(counts) else {
                continue;
            };
            let total: usize = counts.iter().sum();
            let cx = window[2].1 + window[2].2 / 2;
            let max = total;
            // Столбец через середину.
            let Some((vertical, dy)) = runs_through(bits, (cx, y), (0, 1), max) else {
                continue;
            };
            let Some(vertical_module) = finder_module(vertical) else {
                continue;
            };
            let vertical_total: usize = vertical.iter().sum();
            if 5 * vertical_total.abs_diff(total) >= 2 * total {
                continue;
            }
            let cy = (y as f64 + dy).max(0.0);
            // Строка через уточнённую середину.
            let row = (cy.floor() as usize).min(bits.height - 1);
            let Some((horizontal, dx)) = runs_through(bits, (cx, row), (1, 0), max) else {
                continue;
            };
            let Some(horizontal_module) = finder_module(horizontal) else {
                continue;
            };
            let at = Point {
                x: cx as f64 + dx,
                y: cy,
            };
            let module = (vertical_module + horizontal_module) / 2.0;
            match found
                .iter_mut()
                .find(|known| (known.at.x - at.x).abs() <= known.module && (known.at.y - at.y).abs() <= known.module)
            {
                Some(known) => {
                    let n = f64::from(known.hits);
                    known.at = Point {
                        x: (known.at.x * n + at.x) / (n + 1.0),
                        y: (known.at.y * n + at.y) / (n + 1.0),
                    };
                    known.module = (known.module * n + module) / (n + 1.0);
                    known.hits += 1;
                }
                None => found.push(Finder { at, module, hits: 1 }),
            }
        }
    }
    found.sort_by_key(|finder| std::cmp::Reverse(finder.hits));
    // Подтверждённые двумя строками и больше — если таких хотя бы три.
    let sure = found.iter().filter(|finder| finder.hits >= 2).count();
    if sure >= 3 {
        found.truncate(sure);
    }
    found.truncate(MAX_FINDERS);
    found
}

/// Углы кода: узоры поиска левого верхнего, правого верхнего и левого
/// нижнего углов.
#[derive(Clone, Copy, Debug)]
struct Corners {
    top_left: Finder,
    top_right: Finder,
    bottom_left: Finder,
}

/// Тройки узоров, похожие на углы кода, — лучшие первыми: угол между
/// сторонами ближе к прямому, стороны и модули ближе по размеру.
fn triples(finders: &[Finder]) -> Vec<Corners> {
    let mut scored: Vec<(f64, Corners)> = Vec::new();
    let n = finders.len();
    for i in 0..n {
        for j in i + 1..n {
            for k in j + 1..n {
                let three = [finders[i], finders[j], finders[k]];
                let (small, large) = three
                    .iter()
                    .fold((f64::MAX, 0f64), |(small, large), f| (small.min(f.module), large.max(f.module)));
                if large > small * 2.5 {
                    continue;
                }
                // Прямой угол — у того, напротив кого самая длинная сторона.
                let Some((corner, score)) = (0..3)
                    .map(|c| {
                        let a = three[(c + 1) % 3].at.minus(three[c].at);
                        let b = three[(c + 2) % 3].at.minus(three[c].at);
                        let cos = (a.x * b.x + a.y * b.y) / (a.length() * b.length());
                        (c, cos.abs())
                    })
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                else {
                    continue;
                };
                let top_left = three[corner];
                let (mut first, mut second) = (three[(corner + 1) % 3], three[(corner + 2) % 3]);
                let a = first.at.minus(top_left.at);
                let b = second.at.minus(top_left.at);
                let ratio = a.length() / b.length();
                if score > 0.4 || !(0.5..=2.0).contains(&ratio) || a.length().min(b.length()) < small * 10.0 {
                    continue;
                }
                // Ось x — к правому верхнему: поворот от него к левому
                // нижнему — по часовой стрелке (ось y кадра — вниз).
                if a.x * b.y - a.y * b.x < 0.0 {
                    std::mem::swap(&mut first, &mut second);
                }
                // Модуль узоров меряли строкой и столбцом кадра, а код повёрнут:
                // под углом θ строка пересекает квадрат узора по хорде в
                // 1 / max(|cos θ|, |sin θ|) раз длиннее стороны — при 45° в √2,
                // и версия вышла бы на треть меньше (на 300 случайных снимках
                // без поправки не прочитались 8, с ней — 3; 2026-11-21).
                let axis = direction(top_left.at, first.at);
                let chord = axis.x.abs().max(axis.y.abs());
                let mut corners = Corners {
                    top_left,
                    top_right: first,
                    bottom_left: second,
                };
                for finder in [&mut corners.top_left, &mut corners.top_right, &mut corners.bottom_left] {
                    finder.module *= chord;
                }
                scored.push((score + ratio.ln().abs(), corners));
            }
        }
    }
    scored.sort_by(|a, b| a.0.total_cmp(&b.0));
    scored.into_iter().map(|(_, corners)| corners).collect()
}

/// Прочитать код у этих углов: версия — по расстоянию между узорами в
/// модулях, а если слово версии говорит другое — по нему; не вышло —
/// соседние версии и зеркало.
fn decode(bits: &Bits, corners: &Corners) -> Option<Vec<u8>> {
    // Модулей между узорами — по каждой стороне своим средним модулем:
    // под наклоном ближний к камере узор крупнее дальнего.
    let leg = |far: &Finder| far.at.minus(corners.top_left.at).length() / ((far.module + corners.top_left.module) / 2.0);
    let across = (leg(&corners.top_right) + leg(&corners.bottom_left)) / 2.0;
    let estimate = ((across + 7.0 - 17.0) / 4.0).round();
    if !(f64::from(MIN_VERSION) - 1.0..=f64::from(MAX_VERSION) + 1.0).contains(&estimate) {
        return None;
    }
    let estimate = estimate.clamp(f64::from(MIN_VERSION), f64::from(MAX_VERSION)) as u8;
    let mut tried = Vec::new();
    let mut queue = vec![
        estimate,
        estimate.saturating_add(1),
        estimate.saturating_sub(1),
        estimate.saturating_add(2),
        estimate.saturating_sub(2),
    ];
    while let Some(version) = queue.first().copied() {
        queue.remove(0);
        if !(MIN_VERSION..=MAX_VERSION).contains(&version) || tried.contains(&version) {
            continue;
        }
        tried.push(version);
        let Some(map) = Map::new(bits, corners, version) else {
            continue;
        };
        let size = side(version);
        for mirrored in [false, true] {
            let dark = |x: usize, y: usize| {
                let (gx, gy) = if mirrored { (y, x) } else { (x, y) };
                bits.at_point(map.point(gx as f64 + 0.5, gy as f64 + 0.5))
            };
            match read(&dark, size) {
                Ok(data) => return Some(data),
                Err(Unread::Version(found)) => queue.insert(0, found),
                Err(Unread::No) => {}
            }
        }
    }
    None
}

/// Перспектива: точка кода (в модулях) — точка кадра.
struct Map {
    h: [f64; 8],
    /// Во сколько раз уменьшены точки кода и кадра перед расчётом.
    code_scale: f64,
    image_scale: f64,
}

/// Пара точек: в коде (в модулях) и в кадре.
type Pair = ((f64, f64), Point);

impl Map {
    /// Перспектива кода версии `version` у этих углов.
    ///
    /// Сначала — по трём узорам поиска и узору выравнивания у правого
    /// нижнего угла: где он, три угла знают только приблизительно, поэтому
    /// пробуются ближний к ожиданию и «параллелограмм» без него. Каждая
    /// уточняется: узоры выравнивания ищутся там, где она их ждёт,
    /// и перспектива считается заново по всем найденным точкам — у кода
    /// с полусотней модулей ошибка в полмодуля у одного угла иначе сбивала
    /// бы дальнюю половину. Остаётся та, с которой лучше сходятся линии
    /// синхронизации и все узоры выравнивания. Замер на 300 случайных
    /// снимках (`random_shots_of_the_program_code_are_read`, 2026-11-21):
    /// без уточнения не прочитались 12, с ним — 3; восемь ближних кандидатов
    /// вместо одного не дали ничего — 3.
    fn new(bits: &Bits, corners: &Corners, version: u8) -> Option<Self> {
        let size = side(version) as f64;
        let (near, far) = (3.5, size - 3.5);
        let top_left = corners.top_left.at;
        let top_right = corners.top_right.at;
        let bottom_left = corners.bottom_left.at;
        let base: [Pair; 3] = [((near, near), top_left), ((far, near), top_right), ((near, far), bottom_left)];
        let estimated = Point {
            x: top_right.x + bottom_left.x - top_left.x,
            y: top_right.y + bottom_left.y - top_left.y,
        };
        let parallelogram = Self::fit(&[base[0], base[1], base[2], ((far, far), estimated)])?;
        let mut maps = vec![parallelogram];
        if let Some(&last) = alignment(version).last() {
            let center = last as f64 + 0.5;
            let guess = maps[0].point(center, center);
            // Модуль у правого нижнего угла: перспектива меняет его, как
            // у правого верхнего и левого нижнего против левого верхнего.
            let module = (corners.top_right.module * corners.bottom_left.module / corners.top_left.module).clamp(
                corners.top_left.module.min(corners.top_right.module).min(corners.bottom_left.module),
                corners.top_left.module.max(corners.top_right.module).max(corners.bottom_left.module),
            );
            let axes = (direction(top_left, top_right), direction(top_left, bottom_left));
            if let Some(&found) = find_alignment(bits, guess, module, axes).first() {
                maps.extend(Self::fit(&[base[0], base[1], base[2], ((center, center), found)]));
            }
        }
        // Каждая — и уточнённая дважды: уточнение по узорам, найденным
        // неточной перспективой, ближе к настоящей, а второе — ещё ближе.
        let mut all = Vec::with_capacity(maps.len() * 3);
        for map in maps {
            let once = map.refined(bits, &base, version);
            let twice = once.as_ref().and_then(|once| once.refined(bits, &base, version));
            all.push(map);
            all.extend(once);
            all.extend(twice);
        }
        all.into_iter().max_by_key(|map| map.agreement(bits, version))
    }

    /// Та же перспектива, посчитанная заново по узорам выравнивания, которые
    /// нашлись там, где она их ждёт; `None` — ни одного не нашлось.
    fn refined(&self, bits: &Bits, base: &[Pair; 3], version: u8) -> Option<Self> {
        let centers = alignment(version);
        let last = centers.len().checked_sub(1)?;
        let mut pairs: Vec<Pair> = base.to_vec();
        for (i, &x) in centers.iter().enumerate() {
            for (j, &y) in centers.iter().enumerate() {
                if (i == 0 && j == 0) || (i == 0 && j == last) || (i == last && j == 0) {
                    continue;
                }
                let at = (x as f64 + 0.5, y as f64 + 0.5);
                let guess = self.point(at.0, at.1);
                let across = self.point(at.0 + 1.0, at.1).minus(guess);
                let down = self.point(at.0, at.1 + 1.0).minus(guess);
                let module = (across.length() + down.length()) / 2.0;
                let axes = (direction(guess, self.point(at.0 + 1.0, at.1)), direction(guess, self.point(at.0, at.1 + 1.0)));
                if let Some(found) = find_alignment_within(bits, guess, module, axes, 2.5).into_iter().next() {
                    pairs.push((at, found));
                }
            }
        }
        (pairs.len() > base.len()).then(|| Self::fit(&pairs)).flatten()
    }

    /// Сколько клеток сходится с тем, что у кода этой версии должно быть
    /// на месте: линии синхронизации и узоры выравнивания.
    fn agreement(&self, bits: &Bits, version: u8) -> usize {
        let size = side(version);
        let mut score = 0;
        for i in 8..size - 8 {
            let expect = i % 2 == 0;
            let at = i as f64 + 0.5;
            score += usize::from(bits.at_point(self.point(at, 6.5)) == expect);
            score += usize::from(bits.at_point(self.point(6.5, at)) == expect);
        }
        let centers = alignment(version);
        let last = centers.len().saturating_sub(1);
        for (i, &x) in centers.iter().enumerate() {
            for (j, &y) in centers.iter().enumerate() {
                if (i == 0 && j == 0) || (i == 0 && j == last) || (i == last && j == 0) {
                    continue;
                }
                for dy in -2i32..=2 {
                    for dx in -2i32..=2 {
                        let expect = dx.abs().max(dy.abs()) != 1;
                        let point = self.point(x as f64 + 0.5 + f64::from(dx), y as f64 + 0.5 + f64::from(dy));
                        score += usize::from(bits.at_point(point) == expect);
                    }
                }
            }
        }
        score
    }

    /// Перспектива по парам точек — наименьшими квадратами (по четырём —
    /// точно): нормальные уравнения 8×8, метод Гаусса.
    fn fit(pairs: &[Pair]) -> Option<Self> {
        if pairs.len() < 4 {
            return None;
        }
        // Точки кода и кадра — в долях своих наибольших: иначе в нормальных
        // уравнениях числа расходились бы на десять порядков.
        let code_scale = pairs.iter().map(|&((x, y), _)| x.abs().max(y.abs())).fold(1.0, f64::max);
        let image_scale = pairs.iter().map(|&(_, p)| p.x.abs().max(p.y.abs())).fold(1.0, f64::max);
        let mut normal = [[0f64; 9]; 8];
        for &((gx, gy), image) in pairs {
            let (gx, gy) = (gx / code_scale, gy / code_scale);
            let image = Point {
                x: image.x / image_scale,
                y: image.y / image_scale,
            };
            for row in [
                [gx, gy, 1.0, 0.0, 0.0, 0.0, -gx * image.x, -gy * image.x, image.x],
                [0.0, 0.0, 0.0, gx, gy, 1.0, -gx * image.y, -gy * image.y, image.y],
            ] {
                for i in 0..8 {
                    for k in 0..9 {
                        normal[i][k] += row[i] * row[k];
                    }
                }
            }
        }
        let mut m = normal;
        for column in 0..8 {
            let pivot = (column..8).max_by(|&a, &b| m[a][column].abs().total_cmp(&m[b][column].abs()))?;
            if m[pivot][column].abs() < 1e-12 {
                return None;
            }
            m.swap(column, pivot);
            for row in 0..8 {
                if row != column {
                    let factor = m[row][column] / m[column][column];
                    for k in column..9 {
                        m[row][k] -= factor * m[column][k];
                    }
                }
            }
        }
        let mut h = [0f64; 8];
        for (i, slot) in h.iter_mut().enumerate() {
            *slot = m[i][8] / m[i][i];
        }
        h.iter().all(|v| v.is_finite()).then_some(Self {
            h,
            code_scale,
            image_scale,
        })
    }

    fn point(&self, x: f64, y: f64) -> Point {
        let h = &self.h;
        let (x, y) = (x / self.code_scale, y / self.code_scale);
        let w = h[6] * x + h[7] * y + 1.0;
        Point {
            x: (h[0] * x + h[1] * y + h[2]) / w * self.image_scale,
            y: (h[3] * x + h[4] * y + h[5]) / w * self.image_scale,
        }
    }
}

/// Направление от `from` к `to` — единичный вектор.
fn direction(from: Point, to: Point) -> Point {
    let d = to.minus(from);
    let length = d.length().max(1e-9);
    Point {
        x: d.x / length,
        y: d.y / length,
    }
}

/// Узоры выравнивания рядом с `guess` (п. 6.3.6) — ближние первыми — в окне
/// 16 модулей, как самое широкое у ZXing: при сильном наклоне узор дальше,
/// чем ждут три угла, а ближний к ожиданию бывает соседним узором кода. Какой
/// настоящий, решает [`Map::agreement`].
fn find_alignment(bits: &Bits, guess: Point, module: f64, axes: (Point, Point)) -> Vec<Point> {
    find_alignment_within(bits, guess, module, axes, 16.0)
}

/// Узоры выравнивания в окне `steps` модулей от `guess`: в строке и в
/// столбце — светлое, тёмная середина, светлое, по модулю каждое, а весь
/// узор 5×5 — тёмная рамка, светлое кольцо, тёмная середина — сходится
/// с ожидаемым почти в каждой клетке. Светлое и тёмное по модулю в данных
/// встречается на каждом шагу, весь узор — редко. Ближние к `guess` первыми,
/// без повторов.
fn find_alignment_within(bits: &Bits, guess: Point, module: f64, axes: (Point, Point), steps: f64) -> Vec<Point> {
    // Насыщением: у почти вырожденной перспективы ожидание и модуль бывают
    // какими угодно, и сложение без него переполнилось бы (ревью задачи 110).
    let reach = (module * steps).ceil() as isize;
    let (gx, gy) = (guess.x.round() as isize, guess.y.round() as isize);
    let fits = |count: usize| (module * 0.4..module * 1.8).contains(&(count as f64));
    let mut found: Vec<(f64, Point)> = Vec::new();
    for y in gy.saturating_sub(reach).max(0)..gy.saturating_add(reach).min(bits.height as isize) {
        let y = y as usize;
        let left = gx.saturating_sub(reach).max(0) as usize;
        let right = (gx.saturating_add(reach).max(0) as usize).min(bits.width);
        if left >= right {
            continue;
        }
        let mut runs: Vec<(bool, usize, usize)> = Vec::new();
        let mut start = left;
        for x in left + 1..=right {
            if x == right || bits.at(x, y) != bits.at(start, y) {
                runs.push((bits.at(start, y), start, x - start));
                start = x;
            }
        }
        for window in runs.windows(3) {
            if window[0].0 || !window[1].0 || window[2].0 || !(fits(window[0].2) && fits(window[1].2) && fits(window[2].2)) {
                continue;
            }
            let cx = window[1].1 + window[1].2 / 2;
            // Столбец: тёмная середина и светлое над и под ней.
            let up = (0..=y).rev().take_while(|&row| bits.at(cx, row)).count();
            let above = (0..=y.saturating_sub(up)).rev().take_while(|&row| row < y + 1 - up && !bits.at(cx, row)).count();
            let down = (y + 1..bits.height).take_while(|&row| bits.at(cx, row)).count();
            let below = (y + 1 + down..bits.height).take_while(|&row| !bits.at(cx, row)).count();
            if !(fits(up + down) && fits(above) && fits(below)) {
                continue;
            }
            let at = Point {
                x: window[1].1 as f64 + window[1].2 as f64 / 2.0,
                y: (y + 1) as f64 - up as f64 + (up + down) as f64 / 2.0,
            };
            // Шаг узора — по своим полосам: под наклоном модуль здесь не тот,
            // что у узоров поиска.
            let local = (window[0].2 + window[1].2 + window[2].2 + above + up + down + below) as f64 / 6.0;
            if !whole_alignment(bits, at, local, axes) {
                continue;
            }
            // Тот же узор с соседней строки — одна находка.
            if found.iter().any(|(_, known)| known.minus(at).length() < local) {
                continue;
            }
            found.push((at.minus(guess).length(), at));
        }
    }
    found.sort_by(|a, b| a.0.total_cmp(&b.0));
    found.into_iter().map(|(_, at)| at).collect()
}

/// Сходится ли узор 5×5 с серединой в `at` с узором выравнивания — не меньше
/// 22 клеток из 25: край кадра и блик портят клетку-другую.
fn whole_alignment(bits: &Bits, at: Point, module: f64, (across, down): (Point, Point)) -> bool {
    let mut matched = 0;
    for dy in -2i32..=2 {
        for dx in -2i32..=2 {
            let expect = dx.abs().max(dy.abs()) != 1;
            let point = Point {
                x: at.x + (across.x * f64::from(dx) + down.x * f64::from(dy)) * module,
                y: at.y + (across.y * f64::from(dx) + down.y * f64::from(dy)) * module,
            };
            if bits.at_point(point) == expect {
                matched += 1;
            }
        }
    }
    matched >= 22
}



