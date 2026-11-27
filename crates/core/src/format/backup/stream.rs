//! Копия частями потоком (задача 122): записи — по одной, части — по одной.
//!
//! Копия частями (задача 117) собиралась целиком: все записи, открытый
//! текст части, её сжатый текст, шифротекст и файлы всех частей лежали
//! в памяти разом, а память wasm не убывает. Замер 0.0.181: копия в 135 МБ
//! держала вкладку у 1,4 ГиБ, а Safari на iPhone закрывает вкладку, которой
//! мало памяти, — и копия не вышла бы вовсе. Здесь — то, что даёт окну
//! держать меньше:
//!
//! - [`Packer`] сжимает открытый текст части по записи: текста целиком нет,
//!   только сжатое, и тот же текст разжимается, что дал бы [`plaintext`];
//! - [`Layout`] и [`plan`] — раскладка по частям по одним размерам записей:
//!   та же, что у [`split`], но без самих записей;
//! - [`Stream`] — части копии всех ИП потоком: записи приходят по базе за
//!   раз, а готовая часть уходит сразу.
//!
//! Число частей нужно уже в шапке первой части («часть 1 из 3»), а потоком
//! его не узнать. Поэтому копия всех ИП проходит базы дважды: первый проход
//! считает размеры и общие записи ([`plan`]), второй кладёт ([`Stream`]).
//! Между проходами база может измениться — каждая читается своей
//! транзакцией: выросла так, что частей нужно больше, — [`Grew`], и копию
//! собирают заново; стало меньше — недостающие части выходят с одними
//! общими записями: номер и число частей в шапке уже обещаны.
//!
//! [`plaintext`]: super::plaintext
//! [`split`]: super::split

use std::future::Future;

use serde_json::value::RawValue;

use super::{
    CopyId, Deflater, Entry, Header, PACK_LEVEL, PLAIN_HEAD_BYTES, PLAIN_TAIL, Part, RECORD_WEIGHT, RecordOut,
    SLICE_BYTES, packed_bound, plain_head,
};

/// Где кончается часть копии — по одним размерам записей ([`Entry::plain_bytes`]):
/// в каждой части сначала общие записи ([`Entry::in_every_part`]) с шапкой,
/// потом свои, пока часть не наберёт `limit` байтов открытого текста. Запись
/// больше места, что осталось, начинает новую часть — если в этой уже есть
/// своё, кроме общего: иначе и одна больше `limit`, она ложится частью сама.
#[derive(Clone, Debug)]
pub struct Layout {
    limit: usize,
    shared: usize,
    used: usize,
    own: bool,
    parts: usize,
}

impl Layout {
    /// Части, в каждой из которых общие записи весят `shared` байтов.
    pub const fn new(shared: usize, limit: usize) -> Self {
        let shared = PLAIN_HEAD_BYTES.saturating_add(shared);
        Self {
            limit,
            shared,
            used: shared,
            own: false,
            parts: 1,
        }
    }

    /// Ещё запись в `bytes` байтов; `true` — с неё начинается новая часть.
    pub const fn add(&mut self, bytes: usize) -> bool {
        let next = self.own && self.used.saturating_add(bytes) > self.limit;
        if next {
            self.parts += 1;
            self.used = self.shared;
        }
        self.used = self.used.saturating_add(bytes);
        self.own = true;
        next
    }

    /// Сколько частей начато.
    pub const fn parts(&self) -> usize {
        self.parts
    }

    /// Сколько байтов открытого текста — не больше — уже в части, что
    /// в работе, с шапкой и общими записями.
    pub const fn used(&self) -> usize {
        self.used
    }
}

/// Сколько байтов открытого текста — не больше — у каждой части копии,
/// у которой общие записи весят `shared`, а остальные — `sizes` в порядке
/// частей ([`super::in_order`]). Одна часть — копия одним файлом: влезает
/// копия в `limit` целиком — части не начнёт ни одна запись, и общие записи
/// в ней — один раз. Та же раскладка, что у [`super::split`]: копия всех ИП
/// узнаёт её первым проходом по базам, не держа записей, а кладёт записи
/// вторым ([`Stream`]).
pub fn plan(shared: usize, sizes: impl IntoIterator<Item = usize>, limit: usize) -> Vec<usize> {
    let mut layout = Layout::new(shared, limit);
    let mut parts = vec![layout.used()];
    for bytes in sizes {
        if layout.add(bytes) {
            parts.push(0);
        }
        if let Some(last) = parts.last_mut() {
            *last = layout.used();
        }
    }
    parts
}

/// Сжатая часть копии: её шапка и сжатый открытый текст — то, что окно
/// шифрует.
pub struct Packed {
    pub header: Header,
    pub packed: Vec<u8>,
}

/// Открытый текст части, сжатый по записи (задача 122): тот же текст, что
/// [`super::plaintext`], но целиком его в памяти нет — запись идёт в сжатие
/// прямо из своего JSON, — и сжатый текст разжимается в него байт в байт.
pub struct Packer {
    header: Header,
    deflater: Deflater,
    first: bool,
}

impl Packer {
    /// Часть с шапкой `header`, в которой будет не больше `plain` байтов
    /// открытого текста: под сжатое столько места берётся сразу, и буфер,
    /// растя, не переезжает.
    pub fn new(header: &Header, plain: usize) -> Self {
        let mut deflater = Deflater::new(PACK_LEVEL, packed_bound(plain));
        deflater.push(plain_head(header).as_bytes());
        Self {
            header: header.clone(),
            deflater,
            first: true,
        }
    }

    /// Ещё запись.
    pub fn add(&mut self, entry: &Entry) {
        if !self.first {
            self.deflater.push(b",");
        }
        self.first = false;
        let record: &RawValue = &entry.record;
        serde_json::to_writer(
            &mut self.deflater,
            &RecordOut {
                kind: entry.kind,
                base: entry.base,
                record,
            },
        )
        .expect("запись в JSON не падает: она уже в JSON, а сжатие не отказывает");
    }

    /// Часть готова.
    pub fn finish(mut self) -> Packed {
        self.deflater.push(PLAIN_TAIL.as_bytes());
        Packed {
            header: self.header,
            packed: self.deflater.finish(),
        }
    }
}

/// Частей стало бы больше, чем насчитал первый проход ([`plan`]): база
/// выросла между проходами, и копию собирают заново — номер и число частей
/// в шапке уже готовых частей не те.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Grew;

/// Сколько байтов прибавить к месту под сжатое сверх насчитанного: база могла
/// подрасти между проходами.
const GROWTH_BYTES: usize = 1 << 20;

/// Части копии потоком (задача 122): записи приходят по одной — у копии всех
/// ИП по базе за раз, — а часть, которую следующая запись уже не вмещает,
/// уходит сразу, сжатой. В памяти — записи одной базы и сжатое одной части,
/// а не все записи всех ИП и все части.
///
/// Раскладка — та же, что у первого прохода ([`plan`]): тот же [`Layout`] по
/// тем же размерам в том же порядке даёт те же границы частей. Порядок
/// записей ведёт окно, и он тот же, что у [`super::split`]: у копии всех
/// ИП — записи конторы, потом записи без файлов книги всех баз, база за
/// базой, потом файлы книги всех баз.
pub struct Stream {
    header: Header,
    copy: CopyId,
    plan: Vec<usize>,
    shared: Vec<Entry>,
    layout: Layout,
    number: usize,
    packer: Packer,
    done: usize,
}

impl Stream {
    /// Копия с шапкой `header`, общими записями `shared` в каждой части
    /// и частями по плану `plan` ([`plan`]); частей в нём — от одной до
    /// [`super::MAX_PARTS`]: больше окно проверяет и не собирает.
    pub fn new(header: &Header, shared: Vec<Entry>, plan: Vec<usize>, limit: usize) -> Self {
        let shared_bytes = shared.iter().map(Entry::plain_bytes).fold(0, usize::saturating_add);
        let copy = CopyId::random();
        let first = plan.first().copied().unwrap_or(0).saturating_add(GROWTH_BYTES);
        let packer = start(header, copy, plan.len(), &shared, 1, first);
        Self {
            header: header.clone(),
            copy,
            plan,
            shared,
            layout: Layout::new(shared_bytes, limit),
            number: 1,
            packer,
            done: 0,
        }
    }

    /// Ещё запись. Начала она новую часть — прежняя готова и отдаётся.
    /// Частей вышло бы больше, чем по плану, — [`Grew`].
    pub async fn add<P, F>(&mut self, entry: &Entry, pause: &mut P) -> Result<Option<Packed>, Grew>
    where
        P: FnMut() -> F,
        F: Future<Output = ()>,
    {
        let mut full = None;
        if self.layout.add(entry.plain_bytes()) {
            if self.number >= self.plan.len() {
                return Err(Grew);
            }
            self.number += 1;
            let planned = self.plan.get(self.number - 1).copied().unwrap_or(0);
            let next = start(
                &self.header,
                self.copy,
                self.plan.len(),
                &self.shared,
                self.number,
                planned.saturating_add(GROWTH_BYTES),
            );
            full = Some(std::mem::replace(&mut self.packer, next).finish());
        }
        self.packer.add(entry);
        self.done += entry.record.get().len().max(RECORD_WEIGHT);
        if self.done >= SLICE_BYTES {
            self.done = 0;
            pause().await;
        }
        Ok(full)
    }

    /// Записей больше нет: последняя часть — и, если записи легли в меньше
    /// частей, чем по плану, части до числа из шапки с одними общими
    /// записями. Каждая такая — тоже копия сама по себе: ложится и ничего
    /// не меняет.
    pub fn finish(self) -> Vec<Packed> {
        let Self {
            header,
            copy,
            plan,
            shared,
            layout,
            number,
            packer,
            ..
        } = self;
        let mut parts = vec![packer.finish()];
        for next in number + 1..=plan.len() {
            parts.push(start(&header, copy, plan.len(), &shared, next, layout.shared).finish());
        }
        parts
    }
}

/// Сжатие части `number` из `of` с общими записями `shared` впереди; `plain` —
/// сколько байтов открытого текста ждать.
fn start(header: &Header, copy: CopyId, of: usize, shared: &[Entry], number: usize, plain: usize) -> Packer {
    let header = if of < 2 {
        header.clone()
    } else {
        header.with_part(Part {
            copy,
            number: u16::try_from(number).unwrap_or(u16::MAX),
            of: u16::try_from(of).unwrap_or(u16::MAX),
        })
    };
    let mut packer = Packer::new(&header, plain);
    for entry in shared {
        packer.add(entry);
    }
    packer
}
