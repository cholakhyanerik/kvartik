//! Поле Галуа GF(2⁸) и код Рида — Соломона QR-кода (ISO/IEC 18004:2015,
//! п. 7.5.2): порождающий многочлен поля x⁸ + x⁴ + x³ + x² + 1 (0x11D),
//! делитель кода — с корнями α⁰…αⁿ⁻¹. Кодовое слово — байты от старшей
//! степени: `c[0]` — при xⁿ⁻¹.
//!
//! Исправление — Берлекэмп — Мэсси, поиск Ченя и Форни: до ⌊n/2⌋ ошибочных
//! байтов на блок. Исправленное слово проверяется заново — синдромы нули,
//! — иначе блок не читается: кадр камеры с бликом даёт и больше ошибок, чем
//! код исправит, и тогда лучше «не прочитан», чем чужие байты (их всё равно
//! отвергла бы метка подлинности шифра, но позже и без слов о причине).

/// Степени α (с повтором, чтобы сумма двух логарифмов не выходила за край)
/// и логарифмы — считаются при сборке.
const TABLES: ([u8; 512], [u8; 256]) = tables();

const fn tables() -> ([u8; 512], [u8; 256]) {
    let mut exp = [0u8; 512];
    let mut log = [0u8; 256];
    let mut x: u16 = 1;
    let mut i = 0;
    while i < 255 {
        exp[i] = x as u8;
        log[x as usize] = i as u8;
        x <<= 1;
        if x & 0x100 != 0 {
            x ^= 0x11D;
        }
        i += 1;
    }
    while i < 512 {
        exp[i] = exp[i - 255];
        i += 1;
    }
    (exp, log)
}

fn exp(power: usize) -> u8 {
    TABLES.0[power % 255]
}

fn log(value: u8) -> usize {
    usize::from(TABLES.1[usize::from(value)])
}

/// Произведение в поле.
pub(super) fn mul(a: u8, b: u8) -> u8 {
    if a == 0 || b == 0 { 0 } else { TABLES.0[log(a) + log(b)] }
}

/// Частное в поле; делитель не ноль.
fn div(a: u8, b: u8) -> u8 {
    debug_assert_ne!(b, 0, "деление на ноль в поле");
    if a == 0 { 0 } else { TABLES.0[log(a) + 255 - log(b)] }
}

/// Делитель кода степени `degree` — коэффициенты от старшего, без ведущей
/// единицы: (x − α⁰)(x − α¹)…(x − α^(degree−1)).
pub(super) fn divisor(degree: usize) -> Vec<u8> {
    let mut result = vec![0u8; degree];
    if let Some(last) = result.last_mut() {
        *last = 1;
    }
    let mut root = 1u8;
    for _ in 0..degree {
        for j in 0..degree {
            result[j] = mul(result[j], root);
            if j + 1 < degree {
                result[j] ^= result[j + 1];
            }
        }
        root = mul(root, 2);
    }
    result
}

/// Байты проверки для `data`: остаток от деления на `divisor`.
pub(super) fn remainder(data: &[u8], divisor: &[u8]) -> Vec<u8> {
    let mut result = vec![0u8; divisor.len()];
    for &byte in data {
        let factor = byte ^ result[0];
        result.rotate_left(1);
        if let Some(last) = result.last_mut() {
            *last = 0;
        }
        for (slot, &coefficient) in result.iter_mut().zip(divisor) {
            *slot ^= mul(coefficient, factor);
        }
    }
    result
}

/// Синдромы слова: c(α^j) для j от 0 до `ecc` − 1.
fn syndromes(word: &[u8], ecc: usize) -> Vec<u8> {
    (0..ecc)
        .map(|j| {
            let alpha = exp(j);
            word.iter().fold(0u8, |sum, &byte| mul(sum, alpha) ^ byte)
        })
        .collect()
}

/// Значение многочлена с коэффициентами от младшего в точке `x`.
fn eval_low_first(poly: &[u8], x: u8) -> u8 {
    poly.iter().rev().fold(0u8, |sum, &c| mul(sum, x) ^ c)
}

/// Исправить слово `word`, в конце которого `ecc` байтов проверки. Сколько
/// байтов исправлено; `None` — ошибок больше, чем код исправляет, и слово
/// оставлено как было.
pub(super) fn correct(word: &mut [u8], ecc: usize) -> Option<usize> {
    let n = word.len();
    if ecc == 0 || ecc >= n || n > 255 {
        return None;
    }
    let s = syndromes(word, ecc);
    if s.iter().all(|&value| value == 0) {
        return Some(0);
    }
    // Берлекэмп — Мэсси: многочлен локаторов ошибок Λ, от младшего.
    let mut lambda = vec![1u8];
    let mut prev = vec![1u8];
    let mut errors = 0usize;
    let mut shift = 1usize;
    let mut last = 1u8;
    for r in 0..ecc {
        let mut d = s[r];
        for i in 1..=errors.min(lambda.len() - 1) {
            d ^= mul(lambda[i], s[r - i]);
        }
        if d == 0 {
            shift += 1;
            continue;
        }
        let coefficient = div(d, last);
        let mut next = lambda.clone();
        if next.len() < prev.len() + shift {
            next.resize(prev.len() + shift, 0);
        }
        for (i, &p) in prev.iter().enumerate() {
            next[i + shift] ^= mul(coefficient, p);
        }
        if 2 * errors <= r {
            prev = std::mem::replace(&mut lambda, next);
            errors = r + 1 - errors;
            last = d;
            shift = 1;
        } else {
            lambda = next;
            shift += 1;
        }
    }
    while lambda.len() > 1 && lambda.last() == Some(&0) {
        lambda.pop();
    }
    if errors * 2 > ecc || lambda.len() - 1 != errors {
        return None;
    }
    // Поиск Ченя: байт k ошибочен, когда Λ(X⁻¹) = 0 при X = α^(n−1−k).
    let mut positions = Vec::with_capacity(errors);
    for k in 0..n {
        let power = n - 1 - k;
        let x_inv = exp(255 - power % 255);
        if eval_low_first(&lambda, x_inv) == 0 {
            positions.push(k);
        }
    }
    if positions.len() != errors {
        return None;
    }
    // Ω(x) = S(x)·Λ(x) mod x^ecc.
    let mut omega = vec![0u8; ecc];
    for (i, &si) in s.iter().enumerate() {
        for (j, &lj) in lambda.iter().enumerate() {
            if i + j < ecc {
                omega[i + j] ^= mul(si, lj);
            }
        }
    }
    // Форни при первом корне α⁰: e = X·Ω(X⁻¹) / Λ′(X⁻¹).
    let mut fixed = word.to_vec();
    for &k in &positions {
        let x = exp(n - 1 - k);
        let x_inv = exp(255 - (n - 1 - k) % 255);
        let numerator = eval_low_first(&omega, x_inv);
        // Производная в поле характеристики 2 — только нечётные степени.
        let mut denominator = 0u8;
        let mut power = 1u8;
        for (i, &li) in lambda.iter().enumerate().skip(1) {
            if i % 2 == 1 {
                denominator ^= mul(li, power);
            }
            power = mul(power, x_inv);
        }
        if denominator == 0 {
            return None;
        }
        fixed[k] ^= mul(x, div(numerator, denominator));
    }
    if syndromes(&fixed, ecc).iter().any(|&value| value != 0) {
        return None;
    }
    word.copy_from_slice(&fixed);
    Some(errors)
}
