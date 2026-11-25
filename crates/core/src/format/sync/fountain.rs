//! Fountain-код круга QR-кодов (задача 124, часть F, п. 16; решение человека
//! 2026-11-25). Кадр версии 1 несёт свой кусок кода, и пропущенный кадр ждёт
//! следующего круга: у кода в 32 КиБ это 120 кадров, минута. Кадр версии 2 —
//! символ fountain-кода: первые K — куски кода как есть (систематический
//! код: камера, снявшая круг без пропусков, кончает за те же K кадров),
//! а дальше — без конца новые, и каждый — XOR случайного набора кусков. Код
//! собирается из любых K символов, линейно независимых над GF(2), — в среднем
//! из K и ещё полутора-двух (тест `any_k_and_a_few_frames_make_the_code`).
//!
//! Код — случайный линейный, а не LT с распределением «устойчивый солитон»
//! (решение агента 2026-11-25): у LT символы разреженные ради декодера
//! «очисткой», и кадров сверх K ему нужно на 5–20 % больше, а при K не больше
//! [`super::MAX_FRAMES`] исключение Гаусса над GF(2) — доли миллисекунды на
//! кадр. Какие куски в символе, решает генератор от метки передачи и номера
//! символа — целочисленный (splitmix64), одинаковый на любой машине: у
//! отправителя и получателя бывают разные версии программы, а `ln` и
//! другие функции чисел с плавающей точкой в wasm и на машине считают разные
//! библиотеки.

/// Генератор splitmix64 (Steele, Lea, Flood, 2014): 64 бита за шаг, без
/// состояния, кроме счётчика.
struct Mix(u64);

impl Mix {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

/// Кусков у кода в `len` байтов по `block`.
pub(super) const fn pieces(len: usize, block: usize) -> usize {
    len.div_ceil(block)
}

/// Какие из `k` кусков в символе `esi` передачи `id` — битами, по слову на
/// 64 куска. Первые `k` символов — по одному куску, свой у каждого.
fn mask(id: u32, k: usize, esi: u16) -> Vec<u64> {
    // Пустого кода кадр не несёт (`frame` его отвергает), а своё окно его
    // не делает; ноль кусков — пустая маска, а не деление на ноль ниже.
    if k == 0 {
        return Vec::new();
    }
    let words = k.div_ceil(64);
    let mut mask = vec![0u64; words];
    let at = usize::from(esi);
    if at < k {
        mask[at / 64] |= 1 << (at % 64);
        return mask;
    }
    let mut mix = Mix((u64::from(id) << 32) | u64::from(esi));
    for word in &mut mask {
        *word = mix.next();
    }
    let rest = k % 64;
    if rest != 0
        && let Some(last) = mask.last_mut()
    {
        *last &= (1u64 << rest) - 1;
    }
    // Пустой символ ничего не несёт: тогда — один кусок.
    if mask.iter().all(|word| *word == 0) {
        let at = at % k;
        mask[at / 64] |= 1 << (at % 64);
    }
    mask
}

fn has(mask: &[u64], at: usize) -> bool {
    mask[at / 64] >> (at % 64) & 1 == 1
}

fn xor(into: &mut [u8], from: &[u8]) {
    for (to, byte) in into.iter_mut().zip(from) {
        *to ^= byte;
    }
}

/// Символ `esi` кода `code` кусками по `block` байтов, передачи `id`.
pub(super) fn symbol(code: &[u8], block: usize, id: u32, esi: u16) -> Vec<u8> {
    let k = pieces(code.len(), block);
    let mask = mask(id, k, esi);
    let mut out = vec![0u8; block];
    for (at, piece) in code.chunks(block).enumerate() {
        if has(&mask, at) {
            xor(&mut out, piece);
        }
    }
    out
}

/// Сборка символов: строки в ступенчатом виде, у каждой — свой ведущий
/// кусок. Память — `k` строк по куску и маске: при [`super::MAX_FRAMES`]
/// кусках — десятки килобайтов.
#[derive(Debug)]
pub(super) struct Solver {
    id: u32,
    k: usize,
    block: usize,
    rows: Vec<Option<(Vec<u64>, Vec<u8>)>>,
    rank: usize,
}

impl Solver {
    pub(super) fn new(id: u32, k: usize, block: usize) -> Self {
        Self {
            id,
            k,
            block,
            rows: vec![None; k],
            rank: 0,
        }
    }

    /// Взять символ `esi`; `true` — он принёс новое (ранг вырос).
    pub(super) fn take(&mut self, esi: u16, chunk: &[u8]) -> bool {
        if chunk.len() != self.block || self.rank == self.k {
            return false;
        }
        let mut mask = mask(self.id, self.k, esi);
        let mut data = chunk.to_vec();
        for at in 0..self.k {
            if !has(&mask, at) {
                continue;
            }
            match &self.rows[at] {
                // У строки с ведущим куском `at` кусков ниже `at` нет:
                // сложение их не вносит.
                Some((row, bytes)) => {
                    for (word, other) in mask.iter_mut().zip(row) {
                        *word ^= other;
                    }
                    xor(&mut data, bytes);
                }
                None => {
                    self.rows[at] = Some((mask, data));
                    self.rank += 1;
                    return true;
                }
            }
        }
        false
    }

    /// Сколько кусков уже определено.
    pub(super) const fn rank(&self) -> usize {
        self.rank
    }

    /// Сколько кусков у кода.
    pub(super) const fn pieces(&self) -> usize {
        self.k
    }

    /// Длина куска.
    pub(super) const fn block(&self) -> usize {
        self.block
    }

    /// Код в `len` байтов, когда определены все куски.
    pub(super) fn whole(&self, len: usize) -> Option<Vec<u8>> {
        if self.rank < self.k {
            return None;
        }
        let mut solved: Vec<Vec<u8>> = vec![Vec::new(); self.k];
        for at in (0..self.k).rev() {
            let (mask, bytes) = self.rows[at].as_ref()?;
            let mut data = bytes.clone();
            for (later, piece) in solved.iter().enumerate().skip(at + 1) {
                if has(mask, later) {
                    xor(&mut data, piece);
                }
            }
            solved[at] = data;
        }
        let mut code: Vec<u8> = solved.concat();
        code.truncate(len);
        (code.len() == len).then_some(code)
    }
}

/// Байты кадров версии 2 — договор между версиями программы: отправитель
/// и получатель бывают разных версий, а тесты сборки собирают кадры и
/// разбирают их кодом одной сборки и смены генератора не заметят (ревью
/// задачи 124). Здесь — числа не из этого кода: эталон splitmix64
/// (Vigna, `splitmix64.c`, начало 0) и маска, посчитанная отдельно по тому же
/// правилу (2026-11-25). Покраснел — это другая версия кадра, а не правка.
#[cfg(test)]
mod tests {
    use super::{Mix, mask};

    #[test]
    fn the_generator_and_the_mask_are_a_contract() {
        let mut mix = Mix(0);
        assert_eq!(
            [mix.next(), mix.next(), mix.next()],
            [0xE220_A839_7B1D_CDAF, 0x6E78_9E6A_A1B9_65F4, 0x06C4_5D18_8009_454F]
        );
        assert_eq!(mask(0x1234_5678, 70, 100), vec![0x2519_88B4_67AF_8AAB, 0x20]);
        // Первые K символов — по куску: 5-й из 70 — бит 5.
        assert_eq!(mask(0x1234_5678, 70, 5), vec![1 << 5, 0]);
    }
}
