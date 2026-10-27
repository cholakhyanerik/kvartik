//! Лицензия (задача 25): ключ, подписанный автором, и пробный период.
//!
//! Ключ — строка, которую автор выпускает на своей машине программой
//! `kvartik-keygen` (задача 26) и присылает человеку: ՀՎՀՀ, тариф, день
//! выпуска и последний день срока, а за ними — подпись Ed25519. Программа
//! проверяет подпись открытым ключом автора [`PUBLIC_KEY`], встроенным
//! в сборку, — без сервера и без сети (Архитектура, «Сеть — только к своему
//! сайту»). Привязка к ՀՎՀՀ и есть защита: ՀՎՀՀ стоит в каждом счёте,
//! и ключ, выданный на чужой, человеку бесполезен. К устройству ключ не
//! привязан: один ՀՎՀՀ — сколько угодно устройств, а запись ключа
//! ([`License`]) переезжает с копией.
//!
//! Защита ключа — не безопасность (Правило 13): программу без сервера
//! взломать можно, и это принятый риск. Ради неё данные не запираются,
//! не прячутся и не замедляются (Правило 2).
//!
//! # Что запирает конец срока
//!
//! Решение человека 2026-10-27: без срока — пробного периода или ключа на
//! ՀՎՀՀ профиля — нельзя выпустить новый счёт или акт ([`Access::issue`])
//! и открыть числа для расчёта ([`Access::report`]). Всё остальное работает
//! всегда: данные, черновики, книга, налог на экране, оплаты,
//! аннулирование, PDF выпущенных документов, копия. Годовое продление
//! теперь — не «правила нового года» (новые правила на сайте получают все
//! сразу), а кварталы: числа квартала, кончившегося в срок, открыты и после
//! него — последний оплаченный квартал можно подать.
//!
//! # Пробный период
//!
//! [`TRIAL_DAYS`] дней с первого запуска на устройстве, без регистрации.
//! День начала помнит устройство (`store/device.rs` окна), а не база:
//! очистка браузера начинает его заново — вместе со всеми данными, — и
//! бороться с этим не нужно (PDF: «пиратство — не стоит борьбы»). Часы,
//! переведённые назад, продлевают срок — принято как есть (Правило 6).
//!
//! # Ключ
//!
//! Байты версии 1 (числа — старшим байтом вперёд):
//!
//! | байты | что |
//! |---|---|
//! | 0 | версия ключа — 1 |
//! | 1–4 | ՀՎՀՀ числом: восемь цифр, нули впереди возвращает запись |
//! | 5 | тариф: 1 — «Стандарт», 2 — «Pro» |
//! | 6–9 | день выпуска: год (2 байта), месяц, день |
//! | 10–13 | последний день срока, так же |
//! | 14–77 | подпись Ed25519 над [`DOMAIN`] и байтами 0–13 |
//!
//! Новый тариф или поле — новая версия ключа: прежняя программа скажет
//! «ключ для новой версии», а не «ключ испорчен» ([`KeyError::Newer`]).
//!
//! Записью — base32 (RFC 4648, без `=`) группами по пять знаков через
//! дефис: ключ приходит в мессенджере или письме, а у base32 только
//! латиница и цифры — размечать и переносить там нечего. Регистр, пробелы,
//! переносы строк и дефисы при вводе не важны. Опечатку ловит подпись:
//! ключ с ней не сходится ([`KeyError::Forged`]).
//!
//! В данных — `{"meta":{…},"key":"AEAAA-…"}`: ключ той же записью, какой
//! его выдал автор.

use ed25519_dalek::{Signature, VerifyingKey};
use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize, Serializer};

use crate::format::Stored;
use crate::model::date::{Date, Quarter};
use crate::model::record::{RecordId, RecordMeta, Timestamp};
use crate::model::tin::Tin;

/// Открытый ключ автора: им окно проверяет подпись каждого ключа. Пару
/// создал человек программой `kvartik-keygen pair` 2026-10-27; закрытый —
/// у него, вне репозитория (Правило 13). Сменить его — значит перевыпустить
/// ключи всем. Шестнадцатеричной записью, как её напечатала программа:
/// `5a4b379e0c5baf3986f54e8d5d3afa266109330846e42cfe6788b29a6ea396c1`.
pub const PUBLIC_KEY: [u8; PUBLIC_KEY_BYTES] = [
    0x5a, 0x4b, 0x37, 0x9e, 0x0c, 0x5b, 0xaf, 0x39, 0x86, 0xf5, 0x4e, 0x8d, 0x5d, 0x3a, 0xfa, 0x26,
    0x61, 0x09, 0x33, 0x08, 0x46, 0xe4, 0x2c, 0xfe, 0x67, 0x88, 0xb2, 0x9a, 0x6e, 0xa3, 0x96, 0xc1,
];

/// Длина открытого ключа Ed25519.
pub const PUBLIC_KEY_BYTES: usize = ed25519_dalek::PUBLIC_KEY_LENGTH;

/// Дней пробного периода, считая первый (PDF: «пробный период 14 дней без
/// регистрации»).
pub const TRIAL_DAYS: i32 = 14;

/// Начало подписанных байтов: подпись ключа не годится ни для чего другого,
/// что автор подпишет той же парой.
pub const DOMAIN: &[u8] = b"Kvartik license key\n";

/// Версия ключа, которую пишет и понимает эта программа.
const VERSION: u8 = 1;

/// Полей ключа в байтах — до подписи.
const FIELDS: usize = 14;

/// Подписи в байтах.
const SIGNATURE: usize = ed25519_dalek::SIGNATURE_LENGTH;

/// Ключ в байтах.
const BYTES: usize = FIELDS + SIGNATURE;

/// Знаков base32 в группе записи.
const GROUP: usize = 5;

/// Знаков во вводе, дальше которых не читается: у ключа — 149 с дефисами,
/// а мегабайт, вставленный в поле, не должен разбираться.
const MAX_INPUT: usize = 1_000;

/// Знаки base32 по RFC 4648.
const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

/// Самый большой ՀՎՀՀ — восемь девяток.
const MAX_TIN: u32 = 99_999_999;

/// Тариф (PDF, «Деньги»): «Стандарт» — один ИП, «Pro» — бухгалтеру со
/// многими (задача 38). Сейчас оба открывают одно и то же.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Plan {
    Standard,
    Pro,
}

impl Plan {
    const fn byte(self) -> u8 {
        match self {
            Plan::Standard => 1,
            Plan::Pro => 2,
        }
    }

    const fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            1 => Some(Plan::Standard),
            2 => Some(Plan::Pro),
            _ => None,
        }
    }
}

/// Почему ключ не принят.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum KeyError {
    /// Не ключ: чужие знаки, не та длина, поля, которых автор не пишет.
    /// Человеку — «скопируйте ключ целиком».
    Malformed,
    /// Ключ новой версии: его выпустили для программы новее этой.
    Newer,
    /// Подпись не сходится: опечатка или ключ не от автора.
    Forged,
}

/// Ключ лицензии.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Key {
    tin: Tin,
    plan: Plan,
    issued: Date,
    until: Date,
    signature: [u8; SIGNATURE],
}

impl Key {
    /// Байты, которые подписывает автор: [`DOMAIN`] и поля ключа. Зовёт
    /// `kvartik-keygen`, а проверка — [`Key::verify`].
    pub fn message(tin: &Tin, plan: Plan, issued: Date, until: Date) -> Vec<u8> {
        let mut message = DOMAIN.to_vec();
        message.extend_from_slice(&fields(tin, plan, issued, until));
        message
    }

    /// Ключ из полей и подписи автора — для `kvartik-keygen`. Подпись здесь
    /// не проверяется. Срок, кончившийся раньше выпуска, — не ключ.
    pub fn signed(
        tin: Tin,
        plan: Plan,
        issued: Date,
        until: Date,
        signature: [u8; SIGNATURE],
    ) -> Result<Self, KeyError> {
        if issued > until {
            return Err(KeyError::Malformed);
        }
        Ok(Self {
            tin,
            plan,
            issued,
            until,
            signature,
        })
    }

    /// Ключ из записи — без проверки подписи: так ключ лежит в данных, а
    /// подпись проверяет [`Access`] там, где ключ что-то открывает.
    pub fn parse(text: &str) -> Result<Self, KeyError> {
        if text.len() > MAX_INPUT {
            return Err(KeyError::Malformed);
        }
        let bytes = unbase32(text).ok_or(KeyError::Malformed)?;
        match bytes.first() {
            Some(&version) if version > VERSION => return Err(KeyError::Newer),
            Some(&VERSION) => {}
            _ => return Err(KeyError::Malformed),
        }
        let bytes: &[u8; BYTES] = bytes.as_slice().try_into().map_err(|_| KeyError::Malformed)?;
        let tin = u32::from_be_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]);
        let tin = (tin <= MAX_TIN)
            .then(|| Tin::parse(&format!("{tin:08}")).ok())
            .flatten()
            .ok_or(KeyError::Malformed)?;
        let plan = Plan::from_byte(bytes[5]).ok_or(KeyError::Malformed)?;
        let day = |at: usize| {
            let year = u16::from_be_bytes([bytes[at], bytes[at + 1]]);
            Date::new(year, bytes[at + 2], bytes[at + 3]).ok_or(KeyError::Malformed)
        };
        let (issued, until) = (day(6)?, day(10)?);
        let mut signature = [0; SIGNATURE];
        signature.copy_from_slice(&bytes[FIELDS..]);
        Self::signed(tin, plan, issued, until, signature)
    }

    /// Ключ, который ввёл человек: запись и подпись открытым ключом
    /// `public`. Окно зовёт с [`PUBLIC_KEY`], тесты — со своим.
    pub fn read(text: &str, public: &[u8; PUBLIC_KEY_BYTES]) -> Result<Self, KeyError> {
        let key = Self::parse(text)?;
        if key.verify(public) {
            Ok(key)
        } else {
            Err(KeyError::Forged)
        }
    }

    /// Подписан ли ключ закрытой парой `public`. Строгая проверка
    /// (`verify_strict`): у подписи нет второго вида, который тоже сошёлся
    /// бы, и слабый открытый ключ ничего не подписывает.
    pub fn verify(&self, public: &[u8; PUBLIC_KEY_BYTES]) -> bool {
        let Ok(public) = VerifyingKey::from_bytes(public) else {
            return false;
        };
        let message = Self::message(&self.tin, self.plan, self.issued, self.until);
        public
            .verify_strict(&message, &Signature::from_bytes(&self.signature))
            .is_ok()
    }

    /// Запись ключа: base32 группами по пять знаков через дефис.
    pub fn text(&self) -> String {
        let mut bytes = fields(&self.tin, self.plan, self.issued, self.until).to_vec();
        bytes.extend_from_slice(&self.signature);
        let plain = base32(&bytes);
        let mut text = String::with_capacity(plain.len() + plain.len() / GROUP);
        for (index, letter) in plain.chars().enumerate() {
            if index > 0 && index % GROUP == 0 {
                text.push('-');
            }
            text.push(letter);
        }
        text
    }

    /// ՀՎՀՀ, на который выдан ключ.
    pub const fn tin(&self) -> &Tin {
        &self.tin
    }

    /// Тариф.
    pub const fn plan(&self) -> Plan {
        self.plan
    }

    /// День выпуска.
    pub const fn issued(&self) -> Date {
        self.issued
    }

    /// Последний день срока.
    pub const fn until(&self) -> Date {
        self.until
    }
}

/// В данных ключ — его запись, как её выдал автор.
impl Serialize for Key {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.text())
    }
}

/// Читается только запись, которую программа пишет сама: что пишет, то
/// и читает (Правило 13). Подпись — не здесь, а в [`Access`].
impl<'de> Deserialize<'de> for Key {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        match Key::parse(&text) {
            Ok(key) if key.text() == text => Ok(key),
            _ => Err(de::Error::invalid_value(
                de::Unexpected::Str(&text),
                &"ключ лицензии, как его записывает программа",
            )),
        }
    }
}

/// Поля ключа в байтах — таблица в шапке модуля.
fn fields(tin: &Tin, plan: Plan, issued: Date, until: Date) -> [u8; FIELDS] {
    // `Tin` — восемь цифр ASCII: число не больше `MAX_TIN`.
    let number = tin
        .as_str()
        .bytes()
        .fold(0_u32, |number, digit| number * 10 + u32::from(digit - b'0'));
    let mut bytes = [0; FIELDS];
    bytes[0] = VERSION;
    bytes[1..5].copy_from_slice(&number.to_be_bytes());
    bytes[5] = plan.byte();
    for (at, day) in [(6, issued), (10, until)] {
        bytes[at..at + 2].copy_from_slice(&day.year().to_be_bytes());
        bytes[at + 2] = day.month();
        bytes[at + 3] = day.day();
    }
    bytes
}

/// Байты в base32 по RFC 4648, без `=`.
fn base32(bytes: &[u8]) -> String {
    let mut text = String::with_capacity((bytes.len() * 8).div_ceil(5));
    let (mut buffer, mut bits) = (0_u32, 0_u32);
    for &byte in bytes {
        buffer = buffer << 8 | u32::from(byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            text.push(char::from(ALPHABET[(buffer >> bits & 31) as usize]));
        }
        buffer &= (1 << bits) - 1;
    }
    if bits > 0 {
        text.push(char::from(ALPHABET[(buffer << (5 - bits) & 31) as usize]));
    }
    text
}

/// Байты из base32 по RFC 4648, без `=`, в любом регистре; пробелы,
/// переносы строк и дефисы пропускаются. `None` — чужой знак или лишние
/// биты в хвосте: так base32 не пишет никто.
fn unbase32(text: &str) -> Option<Vec<u8>> {
    let mut bytes = Vec::with_capacity(text.len() * 5 / 8);
    let (mut buffer, mut bits) = (0_u32, 0_u32);
    for letter in text.chars() {
        if letter.is_whitespace() || matches!(letter, '-' | '‐' | '‑' | '–' | '—') {
            continue;
        }
        let letter = letter.to_ascii_uppercase();
        let value = match letter {
            'A'..='Z' => letter as u32 - 'A' as u32,
            '2'..='7' => letter as u32 - '2' as u32 + 26,
            _ => return None,
        };
        buffer = buffer << 5 | value;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            bytes.push((buffer >> bits) as u8);
            buffer &= (1 << bits) - 1;
        }
    }
    (bits < 5 && buffer == 0).then_some(bytes)
}

/// Последний день пробного периода, начатого в `start`: день начала —
/// первый из [`TRIAL_DAYS`]. `None` — календарь `Date` кончился.
pub const fn trial_last(start: Date) -> Option<Date> {
    start.add_days(TRIAL_DAYS - 1)
}

/// Срок программы: пробный период или ключ.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Term {
    /// Пробный период до этого дня включительно.
    Trial { last: Date },
    /// Ключ на ՀՎՀՀ профиля до этого дня включительно.
    Key { plan: Plan, last: Date },
}

impl Term {
    /// Последний день срока.
    pub const fn last(self) -> Date {
        match self {
            Term::Trial { last } | Term::Key { last, .. } => last,
        }
    }
}

/// Что открыто: самый долгий из сроков — пробного периода и ключей на
/// ՀՎՀՀ профиля с верной подписью. Ключ на другой ՀՎՀՀ не открывает ничего,
/// как и ключ без ՀՎՀՀ в профиле: ключ выдан на ИП, а не на устройство.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct Access {
    term: Option<Term>,
}

impl Access {
    /// Доступ по ключам `keys`, проверенным открытым ключом `public`,
    /// ՀՎՀՀ профиля `tin` и дню начала пробного периода `trial` (`None` —
    /// устройство его не помнит).
    pub fn new<'a>(
        public: &[u8; PUBLIC_KEY_BYTES],
        keys: impl IntoIterator<Item = &'a Key>,
        tin: Option<&Tin>,
        trial: Option<Date>,
    ) -> Self {
        let trial = trial
            .and_then(trial_last)
            .map(|last| Term::Trial { last });
        let keys = keys
            .into_iter()
            .filter(|key| Some(&key.tin) == tin && key.verify(public))
            .map(|key| Term::Key {
                plan: key.plan,
                last: key.until,
            });
        // При равном последнем дне — ключ: о нём окно и скажет.
        let term = trial
            .into_iter()
            .chain(keys)
            .max_by_key(|term| (term.last(), matches!(term, Term::Key { .. })));
        Self { term }
    }

    /// Самый долгий срок; `None` — ни пробного периода, ни ключа.
    pub const fn term(self) -> Option<Term> {
        self.term
    }

    /// Идёт ли срок в день `today`: можно ли выпустить документ.
    pub fn issue(self, today: Date) -> bool {
        self.term.is_some_and(|term| today <= term.last())
    }

    /// Открыты ли числа для расчёта квартала `quarter` в день `today`:
    /// пока идёт срок — любого, а после — кончившегося в срок.
    pub fn report(self, quarter: Quarter, today: Date) -> bool {
        self.issue(today) || self.term.is_some_and(|term| quarter.last_day() <= term.last())
    }

    /// Сколько дней срока осталось, считая `today`: в последний день — 1.
    /// `None` — срока нет или он кончился.
    pub fn days_left(self, today: Date) -> Option<i32> {
        self.term
            .filter(|term| today <= term.last())
            .map(|term| term.last().days_since(today) + 1)
    }
}

/// Введённый ключ — запись.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct License {
    meta: RecordMeta,
    key: Key,
}

impl Stored for License {
    const KIND: &'static str = "license";

    fn meta(&self) -> &RecordMeta {
        &self.meta
    }
}

impl License {
    /// Команда «ввести ключ» — проверенный `Key::read`. `None` — этот ключ
    /// уже введён: вторая запись того же ключа ничего не открыла бы.
    pub fn enter(key: Key, entered: &[License], now: Timestamp) -> Option<Self> {
        let known = entered
            .iter()
            .any(|license| !license.is_deleted() && license.key == key);
        (!known).then(|| Self {
            meta: RecordMeta::new(now),
            key,
        })
    }

    /// Ключи живых записей.
    pub fn keys(licenses: &[License]) -> impl Iterator<Item = &Key> {
        licenses
            .iter()
            .filter(|license| !license.is_deleted())
            .map(|license| &license.key)
    }

    /// Постоянный идентификатор.
    pub const fn id(&self) -> RecordId {
        self.meta.id()
    }

    /// Ключ.
    pub const fn key(&self) -> &Key {
        &self.key
    }

    /// Убрана ли запись.
    pub const fn is_deleted(&self) -> bool {
        self.meta.is_deleted()
    }
}

#[cfg(test)]
mod tests;
