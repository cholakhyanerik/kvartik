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
//! Решение человека 2026-10-27: без срока — пробного периода, ключа на
//! ՀՎՀՀ профиля или места в ключе «Pro» конторы — нельзя выпустить новый
//! счёт или акт ([`Access::issue`])
//! и открыть числа для расчёта ([`Access::report`]). Всё остальное работает
//! всегда: данные, черновики, книга, налог на экране, оплаты,
//! аннулирование, PDF выпущенных документов, копия. Годовое продление
//! теперь — не «правила нового года» (новые правила на сайте получают все
//! сразу), а кварталы: числа квартала, кончившегося в срок, открыты и после
//! него — последний оплаченный квартал можно подать.
//!
//! # Пробный период
//!
//! [`TRIAL_DAYS`] дней без регистрации — с первого запуска на устройстве или
//! с первого дня данных в программе, если он раньше ([`trial_start`]; условия,
//! п. 3, редакция от 15.11.2026; задача 99). День первого запуска помнит
//! устройство (`store/device.rs` окна), а день данных считается по моментам
//! их записей ([`data_since`]) — они едут с копией. Так копия,
//! восстановленная в чистом браузере, приносит и прошедшие дни, а пустой
//! браузер — по-прежнему чистый лист: 14 дней, но без профиля, клиентов, книги
//! и нумерации. Этим без сервера и кончается: отпечаток устройства — сервер,
//! который помнит отпечатки, и ложное «срок кончился» у нового покупателя
//! с таким же iPhone (разбор задачи 99), а бороться ценой данных нельзя
//! (Правило 13). Часы, переведённые назад, продлевают срок — принято как есть
//! (Правило 6).
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
//! Байты версии 2 (с 0.0.134) — ключ «Pro» с числом мест (задача 38):
//!
//! | байты | что |
//! |---|---|
//! | 0 | версия ключа — 2 |
//! | 1–13 | как у версии 1; тариф — только 2, «Pro» |
//! | 14 | число мест: сколько ИП конторы открывает ключ, от 1 |
//! | 15–78 | подпись Ed25519 над [`DOMAIN`] и байтами 0–14 |
//!
//! Новый тариф или поле — новая версия ключа: прежняя программа скажет
//! «ключ для новой версии», а не «ключ испорчен» ([`KeyError::Newer`]).
//! «Стандарт» по-прежнему выпускается версией 1: его читают и программы до
//! 0.0.134. У ключа «Pro» версии 1 числа мест нет, и мест у него
//! [`V1_PRO_SEATS`].
//!
//! Записью — base32 (RFC 4648, без `=`) группами по пять знаков через
//! дефис: ключ приходит в мессенджере или письме, а у base32 только
//! латиница и цифры — размечать и переносить там нечего. Регистр, пробелы,
//! переносы строк и дефисы при вводе не важны. Опечатку ловит подпись:
//! ключ с ней не сходится ([`KeyError::Forged`]).
//!
//! В данных — `{"meta":{…},"key":"AEAAA-…"}`: ключ той же записью, какой
//! его выдал автор.
//!
//! # Ключ Pro
//!
//! Бухгалтеру со многими ИП (задача 38, решение человека 2026-11-05): ключ
//! «Pro» лежит не в базе ИП, а в базе конторы ([`OfficeLicense`]), и в копию
//! одного ИП не попадает по построению. Он открывает первых N живых ИП списка
//! конторы по дню добавления, где N — число мест ключа ([`seat`]). ՀՎՀՀ у
//! него — бухгалтера, и с ՀՎՀՀ профиля ИП он не сверяется: ՀՎՀՀ бухгалтера
//! нет в счетах его клиентов, и привязка ни от чего не защищала бы — это
//! принятый риск (Правило 13). Ключ «Pro» в базе ИП — его могли ввести до
//! 0.0.134 — открывает, как и раньше, только ИП своего ՀՎՀՀ.
//!
//! # Срок на виду
//!
//! Сколько дней осталось, окно показывает всегда — меткой в меню и на
//! главном экране, — а когда срок кончился, открывается экраном лицензии;
//! за [`SOON_DAYS`] дней до конца ключа — окно поверх программы (решение
//! человека 2026-11-11). Подпись проверяют части окна, а метка нужна с
//! первого кадра, поэтому срок ключа устройство помнит записью
//! ([`remember`]) — без подписи: она только показывает, а запирает по-прежнему
//! [`Access::new`] с проверкой. За столько же дней до конца пробного периода —
//! тоже окно, один раз на устройстве ([`Access::ending_trial`], задача 93).
//!
//! # Продление заранее
//!
//! Ключ, оплаченный до конца срока действующего ключа на тот же ՀՎՀՀ, считается
//! со дня после последнего дня действующего (условия, п. 4.1, редакция от
//! 13.11.2026; решение человека 2026-11-11, вариант Б): продавец выпускает его
//! с последним днём «прежний + срок тарифа». Окну правок не нужно — срок у него
//! самый долгий из ключей ([`Access::new`]).

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

/// В последние столько дней срока, считая последний, окно говорит о нём
/// громче: метка — цветом напоминания и на каждом экране телефона, а у ключа —
/// ещё и окно поверх программы, один раз (решение человека 2026-11-11:
/// «через неделю подписка завершается»).
pub const SOON_DAYS: i32 = 7;

/// Начало подписанных байтов: подпись ключа не годится ни для чего другого,
/// что автор подпишет той же парой.
pub const DOMAIN: &[u8] = b"Kvartik license key\n";

/// Самая новая версия ключа, которую понимает эта программа: 1 — «Стандарт»
/// и «Pro» без числа мест, 2 — «Pro» с числом мест (шапка модуля).
const VERSION: u8 = 2;

/// Полей ключа версии 1 в байтах — до подписи.
const FIELDS_V1: usize = 14;

/// Полей ключа версии 2: ещё байт числа мест.
const FIELDS_V2: usize = FIELDS_V1 + 1;

/// Подписи в байтах.
const SIGNATURE: usize = ed25519_dalek::SIGNATURE_LENGTH;

/// Мест у ключа «Pro» версии 1: в нём числа мест нет, и он открывает 30 —
/// столько ИП «Pro» обещал бухгалтеру PDF («Деньги»; предложение агента
/// 2026-11-05). Смысл ключа, выданного раньше, не меняется: новый «Pro» —
/// версии 2, и число мест в нём своё (у проданного — `release::PRO_SEATS`).
pub const V1_PRO_SEATS: u8 = 30;

/// Знаков base32 в группе записи.
const GROUP: usize = 5;

/// Знаков во вводе, дальше которых не читается: у ключа — 149 с дефисами,
/// а мегабайт, вставленный в поле, не должен разбираться.
const MAX_INPUT: usize = 1_000;

/// Знаки base32 по RFC 4648.
const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

/// Самый большой ՀՎՀՀ — восемь девяток.
const MAX_TIN: u32 = 99_999_999;

/// Тариф (PDF, «Деньги»): «Стандарт» — один ИП, ИП своего ՀՎՀՀ, «Pro» —
/// бухгалтеру со многими: в конторе он открывает места ([`seat`], задача 38).
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

    /// Имя в записи устройства ([`remember`]).
    const fn code(self) -> &'static str {
        match self {
            Plan::Standard => "standard",
            Plan::Pro => "pro",
        }
    }

    /// Тариф по имени [`Plan::code`]; что они сходятся, держит тест записи.
    fn from_code(code: &str) -> Option<Self> {
        match code {
            "standard" => Some(Plan::Standard),
            "pro" => Some(Plan::Pro),
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
    /// Число мест — у ключа версии 2, а она бывает только у «Pro»; у версии 1
    /// его в байтах нет ([`Key::seats`]).
    seats: Option<u8>,
    signature: [u8; SIGNATURE],
}

impl Key {
    /// Байты, которые подписывает автор: [`DOMAIN`] и поля ключа; `seats` —
    /// число мест ключа «Pro» версии 2, `None` — ключ версии 1. Зовёт
    /// `kvartik-keygen`, а проверка — [`Key::verify`].
    pub fn message(tin: &Tin, plan: Plan, issued: Date, until: Date, seats: Option<u8>) -> Vec<u8> {
        let mut message = DOMAIN.to_vec();
        message.extend_from_slice(&fields(tin, plan, issued, until, seats));
        message
    }

    /// Ключ из полей и подписи автора — для `kvartik-keygen`. Подпись здесь
    /// не проверяется. Срок, кончившийся раньше выпуска, — не ключ, как
    /// и «Стандарт» с числом мест и «Pro» на ноль мест: так автор не пишет.
    pub fn signed(
        tin: Tin,
        plan: Plan,
        issued: Date,
        until: Date,
        seats: Option<u8>,
        signature: [u8; SIGNATURE],
    ) -> Result<Self, KeyError> {
        let seats_fit = match (plan, seats) {
            (_, None) => true,
            (Plan::Pro, Some(seats)) => seats > 0,
            (Plan::Standard, Some(_)) => false,
        };
        if issued > until || !seats_fit {
            return Err(KeyError::Malformed);
        }
        Ok(Self {
            tin,
            plan,
            issued,
            until,
            seats,
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
        let fields = match bytes.first() {
            Some(&version) if version > VERSION => return Err(KeyError::Newer),
            Some(1) => FIELDS_V1,
            Some(2) => FIELDS_V2,
            _ => return Err(KeyError::Malformed),
        };
        if bytes.len() != fields + SIGNATURE {
            return Err(KeyError::Malformed);
        }
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
        let seats = (fields == FIELDS_V2).then(|| bytes[FIELDS_V1]);
        let mut signature = [0; SIGNATURE];
        signature.copy_from_slice(&bytes[fields..]);
        Self::signed(tin, plan, issued, until, seats, signature)
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
        let message = Self::message(&self.tin, self.plan, self.issued, self.until, self.seats);
        public
            .verify_strict(&message, &Signature::from_bytes(&self.signature))
            .is_ok()
    }

    /// Запись ключа: base32 группами по пять знаков через дефис.
    pub fn text(&self) -> String {
        let mut bytes = fields(&self.tin, self.plan, self.issued, self.until, self.seats);
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

    /// Версия ключа — 1 или 2 (шапка модуля): по ней журнал выпуска пишет,
    /// названо ли в ключе число мест.
    pub const fn version(&self) -> u8 {
        if self.seats.is_some() { 2 } else { 1 }
    }

    /// Сколько ИП конторы открывает ключ «Pro» ([`seat`]); у «Стандарта» —
    /// `None`: он открывает ИП своего ՀՎՀՀ. У «Pro» версии 1 числа мест нет —
    /// у него [`V1_PRO_SEATS`].
    pub const fn seats(&self) -> Option<u8> {
        match (self.plan, self.seats) {
            (Plan::Standard, _) => None,
            (Plan::Pro, Some(seats)) => Some(seats),
            (Plan::Pro, None) => Some(V1_PRO_SEATS),
        }
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

/// Поля ключа в байтах — таблицы в шапке модуля: с числом мест — версия 2,
/// без него — 1.
fn fields(tin: &Tin, plan: Plan, issued: Date, until: Date, seats: Option<u8>) -> Vec<u8> {
    // `Tin` — восемь цифр ASCII: число не больше `MAX_TIN`.
    let number = tin
        .as_str()
        .bytes()
        .fold(0_u32, |number, digit| number * 10 + u32::from(digit - b'0'));
    let mut bytes = Vec::with_capacity(FIELDS_V2 + SIGNATURE);
    bytes.push(if seats.is_some() { 2 } else { 1 });
    bytes.extend_from_slice(&number.to_be_bytes());
    bytes.push(plan.byte());
    for day in [issued, until] {
        bytes.extend_from_slice(&day.year().to_be_bytes());
        bytes.push(day.month());
        bytes.push(day.day());
    }
    bytes.extend(seats);
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

/// День начала пробного периода: первый запуск на устройстве `device` или
/// первый день данных `data` ([`data_since`]), если он раньше (условия, п. 3,
/// редакция от 15.11.2026). `None` у одного — берётся другой. Без итераторов
/// и `min`: его зовёт и главный wasm окна (CLAUDE.md, Правило 1).
pub const fn trial_start(device: Option<Date>, data: Option<Date>) -> Option<Date> {
    match (device, data) {
        (Some(device), Some(data)) => Some(if data.days_since(device) < 0 { data } else { device }),
        (Some(day), None) | (None, Some(day)) => Some(day),
        (None, None) => None,
    }
}

/// День первой версии программы (0.0.1): момент записи раньше него —
/// ошибка часов, а не день данных (Правило 6, «Часы устройства могут
/// врать»).
pub const FIRST_DAY: Date = Date::new(2026, 10, 10).expect("10 октября 2026 года в календаре есть");

/// Сдвиг пояса UTC+14 — самого позднего на Земле (острова Лайн, Кирибати):
/// в любом поясе местный день момента не позже дня в нём.
const LATEST_ZONE_MILLIS: i64 = 14 * 60 * 60 * 1000;

/// Миллисекунд в сутках.
const DAY_MILLIS: i64 = 24 * 60 * 60 * 1000;

/// Первый день данных (задача 99) по моментам `moments` их записей —
/// изменений и меток удаления ([`moments`]): копия переносит их, а с ними
/// и прошедшие дни пробного периода ([`trial_start`]).
///
/// День момента — по поясу UTC+14: где бы запись ни сделали, местный день
/// не позже, и день данных не раньше настоящего — ошибка только в пользу
/// человека. Так же и правка: запись, которую правили, «молодеет», а не
/// стареет. Моменты раньше [`FIRST_DAY`] и позже `today` — ошибка часов,
/// а не данные: не в счёт. `None` — таких моментов нет.
pub fn data_since(moments: impl IntoIterator<Item = Timestamp>, today: Date) -> Option<Date> {
    moments
        .into_iter()
        .filter_map(|at| {
            // Момент — в пределах дат JS (`Timestamp::MAX_MILLIS`): сумма
            // не переполняется, а сутки — меньше ста миллионов по модулю.
            let days = (at.unix_millis() + LATEST_ZONE_MILLIS).div_euclid(DAY_MILLIS);
            Date::from_day_number(i32::try_from(days).ok()?)
        })
        .filter(|&day| FIRST_DAY <= day && day <= today)
        .min()
}

/// Моменты записи для [`data_since`]: изменение и метка удаления — удалённая
/// запись тоже была данными.
pub fn moments(meta: &RecordMeta) -> impl Iterator<Item = Timestamp> {
    [Some(meta.modified()), meta.deleted()].into_iter().flatten()
}

/// Срок программы: пробный период или ключ.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Term {
    /// Пробный период до этого дня включительно.
    Trial { last: Date },
    /// Ключ на ՀՎՀՀ профиля или место в ключе «Pro» конторы до этого дня
    /// включительно.
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

/// Ключ, который открывает срок: тариф и последний день.
pub type Paid = (Plan, Date);

/// Больше записей ключей не бывает — ключей «Pro» в конторе и ключей в базе
/// каждого ИП (Правило 1, «Накопители ограничены»): ключ — год, и сто — век
/// продлений. Подпись ключа окно проверяет при расчёте срока, и тысячи
/// поддельных из чужого файла копии держали бы страницу секундами на каждом
/// сохранении, а убрать запись ключа нечем (ревью задачи 38; у ключей базы —
/// с 0.0.60, исправлено в 0.0.135). Поэтому копию, с которой ключей стало бы
/// больше, программа не кладёт (`format::office::route`, восстановление окна
/// по `format::backup::count_after`), а ключ сверх предела не вводит.
pub const MAX_KEYS: usize = 100;

/// Места ключей «Pro» `keys` с верной подписью `public` — число мест
/// и последний день каждого — для [`seat`]. Подпись каждого ключа проверяется
/// один раз за расчёт, а один и тот же ключ в двух записях — один раз вовсе:
/// места по готовому списку считаются без неё (ревью задачи 38).
pub fn pro_seats<'a>(public: &[u8; PUBLIC_KEY_BYTES], keys: impl IntoIterator<Item = &'a Key>) -> Vec<(u8, Date)> {
    let mut seen: Vec<&Key> = Vec::new();
    let mut seats = Vec::new();
    for key in keys {
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        if let Some(count) = key.seats()
            && key.verify(public)
        {
            seats.push((count, key.until));
        }
    }
    seats
}

/// Ключ «Pro» конторы, который открывает ИП на месте `place` — его номере
/// среди живых ИП списка в порядке добавления, с нуля
/// (`model::office::place`): самый долгий из ключей `seats` ([`pro_seats`]),
/// у которого мест больше `place`, — и кончившийся, как у [`Access::new`].
/// ՀՎՀՀ ключа не сверяется (шапка модуля, «Ключ Pro»). `None` — ИП нет среди
/// живых или ключа с местом для него нет.
pub fn seat(seats: &[(u8, Date)], place: Option<usize>) -> Option<Paid> {
    let place = place?;
    seats
        .iter()
        .filter(|&&(count, _)| place < usize::from(count))
        .map(|&(_, last)| (Plan::Pro, last))
        .max_by_key(|&(_, last)| last)
}

/// Что открыто: самый долгий из сроков — пробного периода, ключей на ՀՎՀՀ
/// профиля с верной подписью и места в ключе «Pro» конторы
/// ([`Access::with_seat`]). Ключ на другой ՀՎՀՀ не открывает ничего, как
/// и ключ без ՀՎՀՀ в профиле: ключ выдан на ИП, а не на устройство.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct Access {
    /// Последний день пробного периода.
    trial: Option<Date>,
    /// Самый долгий ключ с верной подписью на ՀՎՀՀ профиля или место в ключе
    /// «Pro» — и кончившийся: его помнит устройство ([`remember`]).
    key: Option<Paid>,
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
        let key = keys
            .into_iter()
            .filter(|key| Some(&key.tin) == tin && key.verify(public))
            .map(|key| (key.plan, key.until))
            .max_by_key(|&(_, last)| last);
        Self::known(trial.and_then(trial_last), key)
    }

    /// Тот же доступ и место в ключе «Pro» конторы `seat` ([`seat`]): ключ —
    /// самый долгий из своего на ՀՎՀՀ профиля и места, при равном последнем
    /// дне — свой.
    #[must_use]
    pub fn with_seat(self, seat: Option<Paid>) -> Self {
        let key = match (self.key, seat) {
            (Some(own), Some(pro)) if pro.1 > own.1 => Some(pro),
            (own, pro) => own.or(pro),
        };
        Self { key, ..self }
    }

    /// Доступ по срокам, которые уже известны: последнему дню пробного
    /// периода и ключу, проверенному раньше, — без проверки подписи. Так
    /// главный wasm окна показывает срок, не неся в себе Ed25519
    /// (`ui/license.rs`); запирает выпуск — только [`Access::new`].
    pub const fn known(trial_last: Option<Date>, key: Option<Paid>) -> Self {
        Self { trial: trial_last, key }
    }

    /// Самый долгий срок; `None` — ни пробного периода, ни ключа. Без
    /// итераторов: его зовёт и главный wasm окна, где каждый байт — время
    /// первого захода (CLAUDE.md, Правило 1).
    pub const fn term(self) -> Option<Term> {
        match (self.trial, self.key) {
            (None, None) => None,
            (Some(last), None) => Some(Term::Trial { last }),
            (None, Some((plan, last))) => Some(Term::Key { plan, last }),
            // При равном последнем дне — ключ: о нём окно и скажет.
            (Some(trial), Some((plan, last))) => Some(if trial.days_since(last) > 0 {
                Term::Trial { last: trial }
            } else {
                Term::Key { plan, last }
            }),
        }
    }

    /// Самый долгий ключ — и кончившийся; `None` — ключа на ՀՎՀՀ профиля
    /// нет. Его помнит устройство ([`remember`]).
    pub const fn key(self) -> Option<Paid> {
        self.key
    }

    /// Идёт ли срок в день `today`: можно ли выпустить документ.
    pub fn issue(self, today: Date) -> bool {
        self.term().is_some_and(|term| today <= term.last())
    }

    /// Открыты ли числа для расчёта квартала `quarter` в день `today`:
    /// пока идёт срок — любого, а после — кончившегося в срок.
    pub fn report(self, quarter: Quarter, today: Date) -> bool {
        self.issue(today) || self.term().is_some_and(|term| quarter.last_day() <= term.last())
    }

    /// Сколько дней срока осталось, считая `today`: в последний день — 1.
    /// `None` — срока нет или он кончился.
    pub fn days_left(self, today: Date) -> Option<i32> {
        self.term()
            .filter(|term| today <= term.last())
            .map(|term| term.last().days_since(today) + 1)
    }

    /// Пора ли говорить о сроке громче: осталось не больше [`SOON_DAYS`]
    /// дней или срок кончился, или его нет вовсе.
    pub fn soon(self, today: Date) -> bool {
        self.days_left(today).is_none_or(|left| left <= SOON_DAYS)
    }

    /// Ключ, о конце которого пора сказать окном поверх программы: идёт
    /// срок ключа, а осталось не больше [`SOON_DAYS`] дней. Последний день —
    /// по нему устройство помнит, что о нём уже сказано.
    pub fn ending_key(self, today: Date) -> Option<Paid> {
        match (self.term()?, self.days_left(today)?) {
            (Term::Key { plan, last }, left) if left <= SOON_DAYS => Some((plan, last)),
            _ => None,
        }
    }

    /// Пробный период, о конце которого пора сказать окном поверх программы
    /// (задача 93, решение человека 2026-11-11): идёт он, а не ключ, а
    /// осталось не больше [`SOON_DAYS`] дней. Ключ приходит в течение рабочего
    /// дня после оплаты (условия, п. 2.3), и узнавший о конце в пятницу вечером
    /// ждал бы до понедельника. Последний день — по нему устройство помнит, что
    /// о нём уже сказано. При равном последнем дне срок — ключ ([`Access::term`]),
    /// и окна нет: о ключе скажет [`Access::ending_key`].
    pub fn ending_trial(self, today: Date) -> Option<Date> {
        match (self.term()?, self.days_left(today)?) {
            (Term::Trial { last }, left) if left <= SOON_DAYS => Some(last),
            _ => None,
        }
    }
}

/// Запись устройства о ключе базы (`store/device.rs` окна): тариф
/// и последний день — `standard:2027-11-10`, — а ключа на ՀՎՀՀ профиля нет —
/// `none`. Пишет её часть окна, проверив подписи, а читает главный wasm при
/// открытии: метке срока не ждать части. Это договор между версиями на одном
/// устройстве: незнакомую запись прочитать нельзя ([`remembered`]), и срок
/// тогда скажет часть, — поэтому смысл записи не меняется, а новое — новой
/// записью.
pub fn remember(key: Option<Paid>) -> String {
    match key {
        Some((plan, last)) => format!("{}:{}", plan.code(), last.iso()),
        None => NONE.to_owned(),
    }
}

/// Запись [`remember`]; `None` — не она: испорчена или от программы новее.
pub fn remembered(text: &str) -> Option<Option<Paid>> {
    if text == NONE {
        return Some(None);
    }
    let (plan, last) = text.split_once(':')?;
    Some(Some((Plan::from_code(plan)?, Date::from_iso(last)?)))
}

/// Ключа нет — в записи [`remember`].
const NONE: &str = "none";

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

/// Ключ «Pro», введённый в конторе (задача 38, шапка модуля, «Ключ Pro»), —
/// запись базы конторы: в данных так же, как [`License`], но своим видом,
/// и в базу ИП не идёт никогда. Так в копию одного ИП, отданную клиенту, он
/// не попадёт, а ключ «Стандарт» не ляжет в контору.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OfficeLicense {
    meta: RecordMeta,
    key: Key,
}

impl Stored for OfficeLicense {
    const KIND: &'static str = "office_license";

    fn meta(&self) -> &RecordMeta {
        &self.meta
    }
}

impl OfficeLicense {
    /// Команда «ввести ключ Pro» — проверенный `Key::read`. `None` — этот
    /// ключ уже введён или это не «Pro»: «Стандарт» конторе ничего не открыл
    /// бы, его место — в базе своего ИП ([`License::enter`]).
    pub fn enter(key: Key, entered: &[OfficeLicense], now: Timestamp) -> Option<Self> {
        let known = entered
            .iter()
            .any(|license| !license.is_deleted() && license.key == key);
        (key.plan == Plan::Pro && !known).then(|| Self {
            meta: RecordMeta::new(now),
            key,
        })
    }

    /// Ключи живых записей.
    pub fn keys(licenses: &[OfficeLicense]) -> impl Iterator<Item = &Key> {
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
