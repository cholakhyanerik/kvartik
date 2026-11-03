//! ZIP без сжатия — то, во что упакован XLSX (ECMA-376, часть 2, «Open
//! Packaging Conventions», упаковка — ZIP по APPNOTE PKWARE 6.3).
//!
//! Без сжатия (метод 0, «stored»), а не DEFLATE: сжимал бы `miniz_oxide`,
//! который уже лежит в частях копии и PDF, и выгрузка в части книги сделала
//! бы его общим куском ещё одного набора частей (CLAUDE.md, Правило 6).
//! Файл от этого больше — XML листа за квартал сотни килобайт, — но он
//! ложится на диск человека, а не идёт по сети. Excel и другие читают
//! такие пакеты: части OPC могут быть и несжатыми (ECMA-376, часть 2,
//! §8.4, «Compression Option»).
//!
//! Время файлов — 1980-01-01, начало времени ZIP: тот же лист — те же байты
//! файла, как у PDF (`doc`), и часы ядру не нужны (Архитектура).

/// Файл внутри пакета: имя и байты.
pub(super) struct Entry<'a> {
    pub name: &'a str,
    pub bytes: &'a [u8],
}

/// Время и день файла в записи MS-DOS: 00:00:00 и 1980-01-01.
const DOS_TIME: u16 = 0;
const DOS_DATE: u16 = (1 << 5) | 1;

/// Версия ZIP, нужная для распаковки: 2.0 — без сжатия хватило бы и 1.0,
/// но 2.0 пишут почти все упаковщики, и читатели к ней привыкли.
const VERSION: u16 = 20;

/// Пакет из файлов `entries` — в том порядке, в каком они даны.
pub(super) fn stored(entries: &[Entry<'_>]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for entry in entries {
        let offset = len32(out.len());
        let crc = crc32(entry.bytes);
        let size = len32(entry.bytes.len());
        let name = entry.name.as_bytes();
        // Местный заголовок (APPNOTE 4.3.7).
        put32(&mut out, 0x0403_4b50);
        put16(&mut out, VERSION);
        put16(&mut out, 0); // флаги
        put16(&mut out, 0); // метод: без сжатия
        put16(&mut out, DOS_TIME);
        put16(&mut out, DOS_DATE);
        put32(&mut out, crc);
        put32(&mut out, size); // сжатый размер
        put32(&mut out, size); // размер
        put16(&mut out, len16(name.len()));
        put16(&mut out, 0); // дополнительное поле
        out.extend_from_slice(name);
        out.extend_from_slice(entry.bytes);
        // Запись центрального каталога (APPNOTE 4.3.12).
        put32(&mut central, 0x0201_4b50);
        put16(&mut central, VERSION); // кем сделан
        put16(&mut central, VERSION); // что нужно
        put16(&mut central, 0);
        put16(&mut central, 0);
        put16(&mut central, DOS_TIME);
        put16(&mut central, DOS_DATE);
        put32(&mut central, crc);
        put32(&mut central, size);
        put32(&mut central, size);
        put16(&mut central, len16(name.len()));
        put16(&mut central, 0); // дополнительное поле
        put16(&mut central, 0); // комментарий
        put16(&mut central, 0); // диск
        put16(&mut central, 0); // внутренние признаки
        put32(&mut central, 0); // внешние признаки
        put32(&mut central, offset);
        central.extend_from_slice(name);
    }
    let central_offset = len32(out.len());
    let central_size = len32(central.len());
    out.extend_from_slice(&central);
    // Конец центрального каталога (APPNOTE 4.3.16).
    put32(&mut out, 0x0605_4b50);
    put16(&mut out, 0);
    put16(&mut out, 0);
    put16(&mut out, len16(entries.len()));
    put16(&mut out, len16(entries.len()));
    put32(&mut out, central_size);
    put32(&mut out, central_offset);
    put16(&mut out, 0); // комментарий
    out
}

/// Размер для поля ZIP. Пакет выгрузки — мегабайты, а не гигабайты: больше
/// 4 ГБ (ZIP64) ему не бывать, и поле прижимается к краю, а не переполняется.
fn len32(len: usize) -> u32 {
    u32::try_from(len).unwrap_or(u32::MAX)
}

fn len16(len: usize) -> u16 {
    u16::try_from(len).unwrap_or(u16::MAX)
}

fn put16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

/// Таблица CRC-32 (IEEE 802.3, многочлен 0xEDB88320 в обратной записи —
/// тот, что требует ZIP, APPNOTE 4.4.7) — при сборке, а не при запуске.
const TABLE: [u32; 256] = {
    let mut table = [0_u32; 256];
    let mut n = 0;
    while n < 256 {
        let mut c = n as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 == 1 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            k += 1;
        }
        table[n] = c;
        n += 1;
    }
    table
};

/// CRC-32 байтов — контрольная сумма файла в пакете.
pub(super) fn crc32(bytes: &[u8]) -> u32 {
    !bytes.iter().fold(!0_u32, |crc, &byte| {
        TABLE[usize::from((crc as u8) ^ byte)] ^ (crc >> 8)
    })
}
