//! Base64 по RFC 4648 (§4: алфавит `+/`, выравнивание `=`) — соль, вектор
//! и шифротекст в файле копии (`backup`).
//!
//! Свой, а не крейт `base64`: это полсотни строк под тестами на векторах
//! самого RFC, а зависимость оплачивал бы wasm каждого пользователя
//! («Общее» в CLAUDE.md).
//!
//! Чтение строгое: только то, что пишет [`encode`], — длина кратна четырём,
//! `=` только в конце и не больше двух, лишние биты последнего знака —
//! нули. Файл копии — недоверенный ввод (Правило 13), и у одних байтов
//! должна быть одна запись: иначе испорченный файл читался бы «как-нибудь».

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Байты в base64. Знаки копятся байтами, а строкой становятся один раз:
/// посимвольная запись в `String` была вчетверо медленнее, а у копии
/// с файлами книги это десятки мегабайт (замер 0.0.28).
pub(crate) fn encode(bytes: &[u8]) -> String {
    let mut out = Vec::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let byte = |at: usize| u32::from(chunk.get(at).copied().unwrap_or(0));
        let group = (byte(0) << 16) | (byte(1) << 8) | byte(2);
        // Из трёх байтов — четыре знака; из одного или двух — два или три
        // и выравнивание.
        let kept = chunk.len() + 1;
        for (at, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            out.push(if at < kept {
                ALPHABET[((group >> shift) & 63) as usize]
            } else {
                b'='
            });
        }
    }
    String::from_utf8(out).expect("в base64 только знаки ASCII")
}

/// Байты из base64; `None` — запись не base64 или не та, что дал бы
/// [`encode`].
pub(crate) fn decode(text: &str) -> Option<Vec<u8>> {
    let text = text.as_bytes();
    if !text.len().is_multiple_of(4) {
        return None;
    }
    let groups = text.len() / 4;
    let mut out = Vec::with_capacity(groups * 3);
    let (chunks, _) = text.as_chunks::<4>();
    for (at, chunk) in chunks.iter().enumerate() {
        let padding = chunk.iter().rev().take_while(|&&c| c == b'=').count();
        if padding > 2 || (padding > 0 && at + 1 != groups) {
            return None;
        }
        let mut group = 0_u32;
        for &c in &chunk[..4 - padding] {
            group = (group << 6) | value(c)?;
        }
        group <<= 6 * padding;
        let bytes = [(group >> 16) as u8, (group >> 8) as u8, group as u8];
        let kept = 3 - padding;
        // Биты последнего знака сверх нужных — нули: `Zh==` и `Zg==`
        // не могут значить одно и то же.
        if bytes[kept..].iter().any(|&b| b != 0) {
            return None;
        }
        out.extend_from_slice(&bytes[..kept]);
    }
    Some(out)
}

/// Значение знака алфавита; `None` — знак чужой.
const fn value(c: u8) -> Option<u32> {
    let value = match c {
        b'A'..=b'Z' => c - b'A',
        b'a'..=b'z' => c - b'a' + 26,
        b'0'..=b'9' => c - b'0' + 52,
        b'+' => 62,
        b'/' => 63,
        _ => return None,
    };
    Some(value as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Векторы RFC 4648, §10 — выписаны оттуда, а не посчитаны этим кодом.
    const VECTORS: [(&str, &str); 7] = [
        ("", ""),
        ("f", "Zg=="),
        ("fo", "Zm8="),
        ("foo", "Zm9v"),
        ("foob", "Zm9vYg=="),
        ("fooba", "Zm9vYmE="),
        ("foobar", "Zm9vYmFy"),
    ];

    #[test]
    fn rfc_4648_vectors() {
        let mut wrong = Vec::new();
        for (plain, coded) in VECTORS {
            if encode(plain.as_bytes()) != coded {
                wrong.push(format!("запись «{plain}»: {}", encode(plain.as_bytes())));
            }
            if decode(coded).as_deref() != Some(plain.as_bytes()) {
                wrong.push(format!("чтение «{coded}»: {:?}", decode(coded)));
            }
        }
        assert!(wrong.is_empty(), "векторы RFC 4648: {wrong:#?}");
    }

    /// Все значения байта и все длины остатка читаются обратно — вместе с
    /// `+` и `/`, которых нет в векторах RFC.
    #[test]
    fn every_byte_round_trips() {
        let bytes: Vec<u8> = (0..=255).collect();
        let mut lost = Vec::new();
        for len in 0..=bytes.len() {
            let slice = &bytes[..len];
            if decode(&encode(slice)).as_deref() != Some(slice) {
                lost.push(len);
            }
        }
        assert!(lost.is_empty(), "не читаются обратно, длины: {lost:?}");
        assert_eq!(encode(&[0xfb, 0xff]), "+/8=");
    }

    /// Чужое и неканоничное не читается.
    #[test]
    fn only_canonical_base64_is_read() {
        let rejected = [
            "Zg", "Zg=", "Zg===", "Z===", "====", "Zm9v=", "Zg==Zg==", "Zm=v", "Zh==", "Zm9=",
            "Zm-v", "Zm_v", "Zm9v\n", " Zm9v", "Zm 9v", "Zm9vá",
        ];
        let read: Vec<&str> = rejected
            .into_iter()
            .filter(|text| decode(text).is_some())
            .collect();
        assert!(read.is_empty(), "прочитано неканоничное: {read:?}");
    }
}
