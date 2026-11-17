use ed25519_dalek::{Signer, SigningKey, VerifyingKey};

use super::*;
use crate::format::{decode, encode};

/// Закрытый ключ проб. Не авторский: им подписаны только ключи тестов.
const SEED: [u8; 32] = [7; 32];

fn signer() -> SigningKey {
    SigningKey::from_bytes(&SEED)
}

fn public() -> [u8; PUBLIC_KEY_BYTES] {
    signer().verifying_key().to_bytes()
}

fn day(year: u16, month: u8, day: u8) -> Date {
    Date::new(year, month, day).expect("в тесте — существующая дата")
}

fn tin(text: &str) -> Tin {
    Tin::parse(text).expect("в тесте — годный ՀՎՀՀ")
}

/// Ключ, подписанный парой проб, — так его подписал бы `kvartik-keygen`.
fn key(on: &str, plan: Plan, issued: Date, until: Date) -> Key {
    let tin = tin(on);
    let signature = signer().sign(&Key::message(&tin, plan, issued, until, None)).to_bytes();
    Key::signed(tin, plan, issued, until, None, signature).expect("срок не раньше выпуска")
}

/// Ключ «Pro» версии 2 на `seats` мест, подписанный парой проб.
fn pro(on: &str, issued: Date, until: Date, seats: u8) -> Key {
    let tin = tin(on);
    let signature = signer()
        .sign(&Key::message(&tin, Plan::Pro, issued, until, Some(seats)))
        .to_bytes();
    Key::signed(tin, Plan::Pro, issued, until, Some(seats), signature).expect("годный ключ Pro")
}

fn standard() -> Key {
    key("00054321", Plan::Standard, day(2026, 10, 27), day(2027, 10, 31))
}

/// Открытый ключ автора — точка кривой, и не слабая: ключ из нулей или
/// чужие байты не проверили бы ни одной подписи, и окно отказало бы
/// каждому купленному ключу. И не ключ пары проб: её закрытый ключ лежит
/// в репозитории, и подписать ключ смог бы любой.
#[test]
fn the_author_key_is_a_real_point() {
    let public = VerifyingKey::from_bytes(&PUBLIC_KEY);
    assert!(public.is_ok_and(|public| !public.is_weak()), "PUBLIC_KEY — не открытый ключ Ed25519");
    assert_ne!(PUBLIC_KEY, self::public(), "PUBLIC_KEY — ключ пары проб");
}

/// Поля лежат так, как написано в шапке модуля: таблица выписана
/// человеком, а не посчитана кодом. ՀՎՀՀ 00054321 — 54 321, 0xD431;
/// 2026 — 0x07EA, 2027 — 0x07EB.
#[test]
fn fields_follow_the_table() {
    let message = Key::message(&tin("00054321"), Plan::Standard, day(2026, 10, 27), day(2027, 10, 31), None);
    let (domain, fields) = message.split_at(DOMAIN.len());
    assert_eq!(domain, b"Kvartik license key\n");
    assert_eq!(fields, [1, 0x00, 0x00, 0xD4, 0x31, 1, 0x07, 0xEA, 10, 27, 0x07, 0xEB, 10, 31]);
    let pro = Key::message(&tin("99999999"), Plan::Pro, day(2026, 1, 1), day(2026, 1, 1), None);
    assert_eq!(pro[DOMAIN.len()..], [1, 0x05, 0xF5, 0xE0, 0xFF, 2, 0x07, 0xEA, 1, 1, 0x07, 0xEA, 1, 1]);
    // Версия 2 — те же поля и байт числа мест в конце: 30 — 0x1E.
    let seats = Key::message(&tin("00054321"), Plan::Pro, day(2026, 10, 27), day(2027, 10, 31), Some(30));
    assert_eq!(
        seats[DOMAIN.len()..],
        [2, 0x00, 0x00, 0xD4, 0x31, 2, 0x07, 0xEA, 10, 27, 0x07, 0xEB, 10, 31, 0x1E]
    );
}

/// base32 — по образцам RFC 4648 (раздел 10), без `=`.
#[test]
fn base32_follows_rfc_4648() {
    let mut wrong = Vec::new();
    for (plain, coded) in [
        ("", ""),
        ("f", "MY"),
        ("fo", "MZXQ"),
        ("foo", "MZXW6"),
        ("foob", "MZXW6YQ"),
        ("fooba", "MZXW6YTB"),
        ("foobar", "MZXW6YTBOI"),
    ] {
        if base32(plain.as_bytes()) != coded {
            wrong.push(format!("«{plain}» записан как {}", base32(plain.as_bytes())));
        }
        if unbase32(coded).as_deref() != Some(plain.as_bytes()) {
            wrong.push(format!("{coded} прочитан как {:?}", unbase32(coded)));
        }
    }
    assert!(wrong.is_empty(), "base32: {wrong:#?}");
}

/// Ключ записывается группами по пять знаков base32 через дефис и читается
/// обратно тем же ключом, с верной подписью.
#[test]
fn a_key_round_trips_through_its_text() {
    let key = standard();
    let text = key.text();
    assert_eq!(text.len(), 149, "{text}");
    let groups: Vec<&str> = text.split('-').collect();
    assert_eq!(groups.len(), 25);
    assert!(groups.iter().all(|group| group.len() == 5 && group.bytes().all(|b| ALPHABET.contains(&b))));
    assert_eq!(Key::read(&text, &public()), Ok(key.clone()));
    assert_eq!(key.tin().as_str(), "00054321");
    assert_eq!(key.plan(), Plan::Standard);
    assert_eq!(key.issued(), day(2026, 10, 27));
    assert_eq!(key.until(), day(2027, 10, 31));
    assert_eq!(key.seats(), None);
}

/// Ключ «Pro» версии 2 — на байт длиннее: 79 байт — 127 знаков base32, 26
/// групп, последняя из двух знаков, — и читается обратно с числом мест.
/// У «Pro» версии 1 числа мест в байтах нет, и мест у него `V1_PRO_SEATS`.
#[test]
fn a_pro_key_carries_its_seats() {
    let key = pro("00054321", day(2026, 10, 27), day(2027, 10, 31), 12);
    let text = key.text();
    assert_eq!(text.len(), 152, "{text}");
    let groups: Vec<&str> = text.split('-').collect();
    assert_eq!((groups.len(), groups.last().map(|group| group.len())), (26, Some(2)));
    assert_eq!(Key::read(&text, &public()), Ok(key.clone()));
    assert_eq!((key.plan(), key.seats()), (Plan::Pro, Some(12)));
    let old = self::key("00054321", Plan::Pro, day(2026, 10, 27), day(2027, 10, 31));
    assert_eq!((old.text().len(), old.seats()), (149, Some(V1_PRO_SEATS)));
    assert_eq!(Key::read(&old.text(), &public()), Ok(old));
    // Тот же ключ на другое число мест — другие байты, и подпись не сходится.
    let mut more = key.clone();
    more.seats = Some(99);
    assert_eq!(Key::read(&more.text(), &public()), Err(KeyError::Forged));
}

/// Ключ из мессенджера или письма: строчными, без дефисов, с пробелами,
/// переносами строк и типографскими тире — тот же ключ.
#[test]
fn a_pasted_key_is_read_however_it_came() {
    let key = standard();
    let text = key.text();
    let plain = text.replace('-', "");
    let mut wrong = Vec::new();
    for pasted in [
        text.to_lowercase(),
        plain.clone(),
        format!("  {}\n{}  ", &plain[..60], &plain[60..]),
        text.replace('-', " "),
        text.replace('-', "–"),
        text.replace('-', "\u{2011}"),
        format!("{}\r\n", text),
    ] {
        if Key::read(&pasted, &public()).as_ref() != Ok(&key) {
            wrong.push(pasted);
        }
    }
    assert!(wrong.is_empty(), "не прочитан: {wrong:#?}");
}

/// Опечатка в любом знаке — не ключ: замена знака соседним знаком base32
/// на каждом месте даёт отказ, а не ключ с другими полями. Соседний, а не
/// каждый: проверка подписи в отладочной сборке — миллисекунды, и 31 замена
/// на 125 местах шла бы десятки секунд.
#[test]
fn a_typo_anywhere_is_refused() {
    let plain = standard().text().replace('-', "");
    let mut accepted = Vec::new();
    for (at, letter) in plain.bytes().enumerate() {
        let index = ALPHABET.iter().position(|&known| known == letter).expect("знак base32");
        let typo = char::from(ALPHABET[(index + 1) % ALPHABET.len()]);
        let text = format!("{}{typo}{}", &plain[..at], &plain[at + 1..]);
        if let Ok(key) = Key::read(&text, &public()) {
            accepted.push(format!("{at}: {typo} → {key:?}"));
        }
    }
    assert!(accepted.is_empty(), "опечатка принята: {accepted:#?}");
}

/// Ключ, подписанный не той парой, и ключ, проверенный не тем открытым
/// ключом, — подделка; ключ из нулей не проверяет ничего.
#[test]
fn a_foreign_signature_is_forged() {
    let text = standard().text();
    let other = SigningKey::from_bytes(&[9; 32]).verifying_key().to_bytes();
    assert_eq!(Key::read(&text, &other), Err(KeyError::Forged));
    assert_eq!(Key::read(&text, &[0; PUBLIC_KEY_BYTES]), Err(KeyError::Forged));
    let mut forged = standard();
    forged.until = day(2099, 12, 31);
    assert!(!forged.verify(&public()), "продлённый руками ключ прошёл");
    assert_eq!(Key::read(&forged.text(), &public()), Err(KeyError::Forged));
}

/// Байты, которых автор не пишет, — не ключ; ключ новой версии — «новее»,
/// а не «испорчен».
#[test]
fn malformed_keys_are_refused() {
    let good = {
        let key = standard();
        let mut bytes = fields(&key.tin, key.plan, key.issued, key.until, None);
        bytes.extend_from_slice(&key.signature);
        bytes
    };
    let good_pro = {
        let key = pro("00054321", day(2026, 10, 27), day(2027, 10, 31), 30);
        let mut bytes = fields(&key.tin, key.plan, key.issued, key.until, key.seats);
        bytes.extend_from_slice(&key.signature);
        bytes
    };
    let pro_with = |at: usize, value: u8| {
        let mut bytes = good_pro.clone();
        bytes[at] = value;
        base32(&bytes)
    };
    let bytes_v1 = FIELDS_V1 + SIGNATURE;
    let with = |at: usize, value: u8| {
        let mut bytes = good.clone();
        bytes[at] = value;
        base32(&bytes)
    };
    let mut cases = vec![
        (String::new(), KeyError::Malformed),
        ("не ключ".to_owned(), KeyError::Malformed),
        (base32(&good[..bytes_v1 - 1]), KeyError::Malformed),
        (base32(&[good.as_slice(), &[0]].concat()), KeyError::Malformed),
        (with(0, 0), KeyError::Malformed),
        // Версия 2 длиной версии 1: байта мест нет.
        (with(0, 2), KeyError::Malformed),
        (with(0, 3), KeyError::Newer),
        (with(0, 255), KeyError::Newer),
        (base32(&[3]), KeyError::Newer),
        (base32(&[2]), KeyError::Malformed),
        // Версия 2: «Стандарт» с местами, ноль мест, без байта и с лишним.
        (pro_with(5, 1), KeyError::Malformed),
        (pro_with(14, 0), KeyError::Malformed),
        (base32(&good_pro[..good_pro.len() - 1]), KeyError::Malformed),
        (base32(&[good_pro.as_slice(), &[0]].concat()), KeyError::Malformed),
        // ՀՎՀՀ больше восьми цифр: 0x05F5E100 — сто миллионов.
        (base32(&[&good[..1], &[0x05, 0xF5, 0xE1, 0x00], &good[5..]].concat()), KeyError::Malformed),
        (with(5, 0), KeyError::Malformed),
        (with(5, 3), KeyError::Malformed),
        // 31 февраля, тринадцатый месяц, нулевой день.
        (with(12, 2), KeyError::Malformed),
        (with(12, 13), KeyError::Malformed),
        (with(13, 0), KeyError::Malformed),
        // Выпуск в 2028 году, позже конца срока.
        (with(7, 0xEC), KeyError::Malformed),
        ("1".repeat(125), KeyError::Malformed),
        (format!("{}0", &base32(&good)[..124]), KeyError::Malformed),
        ("A".repeat(MAX_INPUT + 1), KeyError::Malformed),
    ];
    // Лишние биты в хвосте: 78 байт — 624 бита, а 125 знаков — 625.
    let tail = base32(&good);
    let last = ALPHABET.iter().position(|&b| tail.ends_with(char::from(b))).expect("знак base32");
    cases.push((format!("{}{}", &tail[..124], char::from(ALPHABET[last | 1])), KeyError::Malformed));
    let mut wrong = Vec::new();
    for (text, error) in &cases {
        let got = Key::parse(text);
        if got.as_ref().err() != Some(error) {
            wrong.push(format!("{text:.40}: {got:?}, а не {error:?}"));
        }
    }
    assert!(wrong.is_empty(), "ключи: {wrong:#?}");
    let mut accepted = Vec::new();
    for (plan, issued, seats) in [
        (Plan::Standard, day(2027, 1, 1), None),
        (Plan::Standard, day(2026, 1, 1), Some(5)),
        (Plan::Pro, day(2026, 1, 1), Some(0)),
    ] {
        if Key::signed(tin("00054321"), plan, issued, day(2026, 12, 31), seats, [0; SIGNATURE]).is_ok() {
            accepted.push(format!("{plan:?}, {issued:?}, {seats:?}"));
        }
    }
    assert!(accepted.is_empty(), "приняты ключи, которых автор не пишет: {accepted:?}");
    // Годный «Pro» той же проверкой принят: отказы выше — не всему подряд.
    let fine = Key::signed(tin("00054321"), Plan::Pro, day(2026, 1, 1), day(2026, 12, 31), Some(1), [0; SIGNATURE]);
    assert!(fine.is_ok(), "{fine:?}");
}

/// Пробный период — 14 дней, считая первый: начатый 27 октября кончается
/// 9 ноября включительно.
#[test]
fn the_trial_lasts_fourteen_days() {
    let start = day(2026, 10, 27);
    assert_eq!(trial_last(start), Some(day(2026, 11, 9)));
    let access = Access::new(&public(), [], None, Some(start));
    assert_eq!(access.term(), Some(Term::Trial { last: day(2026, 11, 9) }));
    assert_eq!(access.days_left(start), Some(14));
    assert_eq!(access.days_left(day(2026, 11, 9)), Some(1));
    assert_eq!(access.days_left(day(2026, 11, 10)), None);
    assert!(access.issue(day(2026, 11, 9)));
    assert!(!access.issue(day(2026, 11, 10)));
    // Часы, переведённые назад, — срок идёт: принято как есть.
    assert!(access.issue(day(2026, 10, 1)));
    let nothing = Access::new(&public(), [], None, None);
    assert_eq!(nothing.term(), None);
    assert!(!nothing.issue(start));
    assert!(!nothing.report(Quarter::new(2026, 3).expect("квартал"), start));
}

/// Момент `hour:minute` дня по UTC.
fn utc(on: Date, hour: i64, minute: i64) -> Timestamp {
    let days = i64::from(on.days_since(day(1970, 1, 1)));
    Timestamp::from_unix_millis(((days * 24 + hour) * 60 + minute) * 60_000)
}

/// День данных — самый ранний из моментов записей; моментов нет — нет и дня
/// (задача 99).
#[test]
fn the_data_day_is_the_earliest_moment() {
    let today = day(2026, 11, 15);
    let moments = [utc(day(2026, 11, 5), 10, 0), utc(day(2026, 11, 3), 5, 0), utc(day(2026, 11, 10), 23, 0)];
    assert_eq!(data_since(moments, today), Some(day(2026, 11, 3)));
    assert_eq!(data_since([], today), None);
}

/// День момента — не раньше местного ни в одном поясе Земли, от UTC−12 до
/// UTC+14: день данных никогда не отнимет у человека дней. Сутки — по
/// четверти часа, пояса — по часу; провалы — списком в конце.
#[test]
fn the_data_day_is_never_before_the_local_one() {
    let today = day(2026, 11, 30);
    let mut failures = Vec::new();
    for quarter in 0..96 {
        let at = utc(day(2026, 11, 3), 0, quarter * 15);
        let Some(data) = data_since([at], today) else {
            failures.push(format!("{} мс: дня нет", at.unix_millis()));
            continue;
        };
        for zone in -12..=14_i64 {
            let shifted = at.unix_millis() + zone * 3_600_000;
            let local = day(1970, 1, 1)
                .add_days(i32::try_from(shifted.div_euclid(86_400_000)).expect("сутки"))
                .expect("день");
            if data < local {
                failures.push(format!("{} мс, UTC{zone:+}: день данных {}, местный {}", at.unix_millis(), data.iso(), local.iso()));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
    // Край суток в Ереване (UTC+4): 20:30 UTC 3 ноября — уже 4 ноября там,
    // и день данных тот же; 09:59 UTC — 3 ноября везде до UTC+14.
    assert_eq!(data_since([utc(day(2026, 11, 3), 20, 30)], today), Some(day(2026, 11, 4)));
    assert_eq!(data_since([utc(day(2026, 11, 3), 9, 59)], today), Some(day(2026, 11, 3)));
    assert_eq!(data_since([utc(day(2026, 11, 3), 10, 0)], today), Some(day(2026, 11, 4)));
}

/// Момент раньше первой версии программы или позже сегодняшнего дня —
/// ошибка часов, а не данные: не в счёт. День первой версии — в счёт.
#[test]
fn clock_errors_are_not_data() {
    let today = day(2026, 11, 15);
    let before = utc(day(2026, 10, 1), 12, 0);
    let later = utc(day(2026, 11, 20), 12, 0);
    // 08:00 UTC — ещё 8 ноября и в UTC+14 (22:00).
    let real = utc(day(2026, 11, 8), 8, 0);
    assert_eq!(data_since([before, later, real], today), Some(day(2026, 11, 8)));
    assert_eq!(data_since([before, later], today), None);
    assert_eq!(data_since([utc(FIRST_DAY, 0, 0)], today), Some(FIRST_DAY));
    assert_eq!(data_since([Timestamp::from_unix_millis(i64::MIN)], today), None);
}

/// Удалённая запись — тоже данные: её моменты в счёт.
#[test]
fn deleted_records_count() {
    let mut meta = RecordMeta::new(utc(day(2026, 11, 2), 8, 0));
    meta.delete(utc(day(2026, 11, 4), 8, 0));
    let deleted: Vec<Timestamp> = moments(&meta).collect();
    assert_eq!(deleted, [utc(day(2026, 11, 4), 8, 0), utc(day(2026, 11, 4), 8, 0)]);
    assert_eq!(data_since(deleted, day(2026, 11, 15)), Some(day(2026, 11, 4)));
    let alive = RecordMeta::new(utc(day(2026, 11, 2), 8, 0));
    assert_eq!(moments(&alive).collect::<Vec<_>>(), [utc(day(2026, 11, 2), 8, 0)]);
}

/// Пробный период — с первого запуска на устройстве или с первого дня данных,
/// если он раньше (условия, п. 3, редакция от 15.11.2026): копия с данными
/// с 20 октября, восстановленная в браузере, открытом 14 ноября, приносит
/// и прошедшие дни, и 15 ноября срок уже кончился.
#[test]
fn the_trial_starts_with_the_earlier_day() {
    let device = day(2026, 11, 14);
    let data = day(2026, 10, 20);
    assert_eq!(trial_start(Some(device), Some(data)), Some(data));
    assert_eq!(trial_start(Some(data), Some(device)), Some(data));
    assert_eq!(trial_start(Some(device), None), Some(device));
    assert_eq!(trial_start(None, Some(data)), Some(data));
    assert_eq!(trial_start(None, None), None);
    let today = day(2026, 11, 15);
    assert!(Access::new(&public(), [], None, Some(device)).issue(today));
    assert!(!Access::new(&public(), [], None, trial_start(Some(device), Some(data))).issue(today));
}

/// После срока числа для расчёта открыты за кварталы, кончившиеся в срок, —
/// и у пробного периода, и у ключа; пока срок идёт — за любой.
#[test]
fn quarters_ended_within_the_term_stay_open() {
    let quarter = |year, number| Quarter::new(year, number).expect("квартал");
    let trial = Access::new(&public(), [], None, Some(day(2026, 10, 27)));
    let after = day(2026, 12, 1);
    assert!(trial.report(quarter(2026, 3), after));
    assert!(!trial.report(quarter(2026, 4), after));
    assert!(trial.report(quarter(2026, 4), day(2026, 11, 9)));
    assert!(!trial.issue(after));

    let key = standard();
    let licensed = Access::new(&public(), [&key], Some(&tin("00054321")), Some(day(2026, 10, 27)));
    assert_eq!(licensed.term(), Some(Term::Key { plan: Plan::Standard, last: day(2027, 10, 31) }));
    let expired = day(2027, 11, 1);
    assert!(licensed.issue(day(2027, 10, 31)));
    assert!(!licensed.issue(expired));
    assert!(licensed.report(quarter(2027, 3), expired));
    assert!(licensed.report(quarter(2025, 1), expired));
    assert!(!licensed.report(quarter(2027, 4), expired));
    assert!(licensed.report(quarter(2027, 4), day(2027, 10, 31)));
}

/// Открывает только ключ на ՀՎՀՀ профиля с верной подписью: ключ на другой
/// ՀՎՀՀ, ключ без ՀՎՀՀ в профиле и поддельный не открывают ничего.
#[test]
fn only_a_signed_key_for_the_profile_tin_counts() {
    let key = standard();
    let today = day(2027, 1, 15);
    let mut wrong = Vec::new();
    for (case, tin) in [("другой ՀՎՀՀ", Some(tin("07654321"))), ("ՀՎՀՀ нет", None)] {
        let access = Access::new(&public(), [&key], tin.as_ref(), None);
        if access.issue(today) {
            wrong.push(case);
        }
    }
    let other = SigningKey::from_bytes(&[9; 32]).verifying_key().to_bytes();
    if Access::new(&other, [&key], Some(&tin("00054321")), None).issue(today) {
        wrong.push("чужая подпись");
    }
    assert!(wrong.is_empty(), "открыли: {wrong:?}");
}

/// Из нескольких сроков — самый долгий; при равном — ключ.
#[test]
fn the_longest_term_wins() {
    let on = tin("00054321");
    let old = key("00054321", Plan::Standard, day(2025, 10, 1), day(2026, 9, 30));
    let pro = key("00054321", Plan::Pro, day(2026, 10, 1), day(2026, 11, 9));
    let access = Access::new(&public(), [&old], Some(&on), Some(day(2026, 10, 27)));
    assert_eq!(access.term(), Some(Term::Trial { last: day(2026, 11, 9) }));
    let access = Access::new(&public(), [&old, &pro], Some(&on), Some(day(2026, 10, 27)));
    assert_eq!(access.term(), Some(Term::Key { plan: Plan::Pro, last: day(2026, 11, 9) }));
    let access = Access::new(&public(), [&pro, &standard(), &old], Some(&on), None);
    assert_eq!(access.term().map(Term::last), Some(day(2027, 10, 31)));
}

/// Срок, собранный из уже проверенного ключа (`Access::known`), — тот же, что
/// с проверкой подписи: главный wasm окна показывает его без Ed25519, и метка
/// не должна разойтись с тем, что запирает выпуск. Самый долгий ключ
/// помнится и кончившимся: он короче пробного периода, но запись о нём —
/// правда.
#[test]
fn a_known_term_matches_the_checked_one() {
    let on = tin("00054321");
    let old = key("00054321", Plan::Standard, day(2025, 10, 1), day(2026, 9, 30));
    let year = standard();
    let start = day(2026, 10, 27);
    let mut wrong = Vec::new();
    for (case, keys) in [("без ключа", vec![]), ("кончившийся", vec![&old]), ("годовой", vec![&old, &year])] {
        let checked = Access::new(&public(), keys, Some(&on), Some(start));
        let known = Access::known(trial_last(start), checked.key());
        for today in [start, day(2026, 11, 9), day(2026, 11, 10), day(2027, 10, 31), day(2027, 11, 1)] {
            if (known.term(), known.days_left(today), known.issue(today))
                != (checked.term(), checked.days_left(today), checked.issue(today))
            {
                wrong.push(format!("{case}, {today:?}: {known:?} против {checked:?}"));
            }
        }
    }
    let expired = Access::new(&public(), [&old], Some(&on), Some(start));
    assert_eq!(expired.key(), Some((Plan::Standard, day(2026, 9, 30))));
    assert_eq!(expired.term(), Some(Term::Trial { last: day(2026, 11, 9) }));
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// Громче — в последние семь дней, считая последний, после конца срока
/// и без срока; окно поверх программы — только о ключе, пока он идёт.
#[test]
fn the_last_week_is_said_louder() {
    let last = day(2027, 10, 31);
    let licensed = Access::known(None, Some((Plan::Standard, last)));
    let mut wrong = Vec::new();
    for (today, soon, ending) in [
        (day(2027, 10, 24), false, false),
        (day(2027, 10, 25), true, true),
        (last, true, true),
        (day(2027, 11, 1), true, false),
    ] {
        if licensed.soon(today) != soon {
            wrong.push(format!("ключ, {today:?}: громче — {}", licensed.soon(today)));
        }
        let told = licensed.ending_key(today);
        if told.is_some() != ending || told.is_some_and(|key| key != (Plan::Standard, last)) {
            wrong.push(format!("ключ, {today:?}: окно — {told:?}"));
        }
    }
    let trial = Access::known(trial_last(day(2026, 10, 27)), None);
    if !trial.soon(day(2026, 11, 3)) || trial.soon(day(2026, 11, 2)) {
        wrong.push("пробный период: громче не с 3 ноября".to_owned());
    }
    if trial.ending_key(day(2026, 11, 9)).is_some() {
        wrong.push("окно о конце — у пробного периода".to_owned());
    }
    if !Access::default().soon(day(2026, 11, 3)) {
        wrong.push("без срока — тихо".to_owned());
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// Окно о конце пробного периода (задача 93) — в его последние семь дней,
/// считая последний, и только пока срок — он: при ключе дольше него и при
/// равном последнем дне о сроке скажет окно ключа. Кончился период — окна
/// нет: экран лицензии открывается при запуске сам.
#[test]
fn the_trial_end_is_said_a_week_ahead() {
    let last = day(2026, 11, 9);
    let trial = Access::known(trial_last(day(2026, 10, 27)), None);
    let mut wrong = Vec::new();
    for (today, ending) in [
        (day(2026, 11, 2), None),
        (day(2026, 11, 3), Some(last)),
        (last, Some(last)),
        (day(2026, 11, 10), None),
    ] {
        if trial.ending_trial(today) != ending {
            wrong.push(format!("пробный период, {today:?}: окно — {:?}", trial.ending_trial(today)));
        }
        if trial.ending_key(today).is_some() {
            wrong.push(format!("пробный период, {today:?}: окно ключа"));
        }
    }
    let soon = day(2026, 11, 5);
    let with = |until| Access::known(Some(last), Some((Plan::Standard, until)));
    if with(day(2027, 10, 31)).ending_trial(soon).is_some() {
        wrong.push("ключ дольше пробного периода, а окно — о пробном".to_owned());
    }
    if with(last).ending_trial(soon).is_some() || with(last).ending_key(soon) != Some((Plan::Standard, last)) {
        wrong.push("при равном последнем дне окно не о ключе".to_owned());
    }
    // Ключ короче пробного периода: срок — пробный период, окно — его.
    if with(day(2026, 11, 6)).ending_trial(soon) != Some(last) {
        wrong.push("ключ короче пробного периода, а окна о пробном нет".to_owned());
    }
    if Access::default().ending_trial(soon).is_some() {
        wrong.push("без срока — окно".to_owned());
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// Запись устройства о ключе читается обратно — и «ключа нет» тоже, — а чужая
/// или испорченная не читается: срок тогда скажет часть окна, проверив
/// подписи. Сами записи выписаны руками: их читает и прежняя версия.
#[test]
fn the_remembered_key_round_trips() {
    let mut wrong = Vec::new();
    for (key, text) in [
        (Some((Plan::Standard, day(2027, 11, 10))), "standard:2027-11-10"),
        (Some((Plan::Pro, day(2026, 1, 1))), "pro:2026-01-01"),
        (None, "none"),
    ] {
        if remember(key) != text {
            wrong.push(format!("{key:?} записан как {}", remember(key)));
        }
        if remembered(text) != Some(key) {
            wrong.push(format!("{text} прочитан как {:?}", remembered(text)));
        }
    }
    for spoiled in ["", "None", "standard", "standard:", "basic:2027-11-10", "standard:2027-02-30", "standard:2027-11-10:1", ":2027-11-10"] {
        if remembered(spoiled).is_some() {
            wrong.push(format!("«{spoiled}» прочитан как {:?}", remembered(spoiled)));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// Запись ключа читается обратно, а ключ в ней — только своей записью:
/// строчными, без дефисов или испорченный — порча.
#[test]
fn a_license_record_round_trips() {
    let license = License::enter(standard(), &[], Timestamp::from_unix_millis(1_790_000_000_000)).expect("новый ключ");
    let text = encode(&license);
    assert!(text.contains(&format!(r#""key":"{}""#, standard().text())), "{text}");
    assert_eq!(decode::<License>(&text), Ok(license.clone()));
    let mut wrong = Vec::new();
    for spoiled in [
        text.replace(&standard().text(), &standard().text().to_lowercase()),
        text.replace(&standard().text(), &standard().text().replace('-', "")),
        text.replace(&standard().text(), "AAAAA"),
        text.replace(r#""key""#, r#""wings":1,"key""#),
    ] {
        if decode::<License>(&spoiled).is_ok() {
            wrong.push(spoiled);
        }
    }
    assert!(wrong.is_empty(), "прочитано: {wrong:#?}");
}

/// Тот же ключ второй раз не вводится, пока жива его запись; другой —
/// вводится.
#[test]
fn a_key_is_entered_once() {
    let now = Timestamp::from_unix_millis(1_790_000_000_000);
    let first = License::enter(standard(), &[], now).expect("новый ключ");
    assert_eq!(License::enter(standard(), std::slice::from_ref(&first), now), None);
    let next = key("00054321", Plan::Standard, day(2027, 10, 20), day(2028, 10, 31));
    let second = License::enter(next.clone(), std::slice::from_ref(&first), now).expect("другой ключ");
    let entered = [first, second];
    let keys: Vec<&Key> = License::keys(&entered).collect();
    assert_eq!(keys, [&standard(), &next]);
}

/// Ключ «Pro» конторы открывает первых N живых ИП — места с нуля, — а N-е
/// место и дальше нет; ИП, которого нет в списке, — тоже нет. ՀՎՀՀ ключа не
/// сверяется: ключ на ՀՎՀՀ бухгалтера открывает ИП с любым. «Стандарт»
/// и ключ с чужой подписью мест не дают, а «Pro» версии 1 даёт 30.
#[test]
fn a_pro_key_opens_its_first_places() {
    let (issued, last) = (day(2026, 11, 1), day(2027, 10, 31));
    let two = pro("07654321", issued, last, 2);
    let seats = pro_seats(&public(), [&two]);
    let mut wrong = Vec::new();
    for (place, expected) in [
        (Some(0), Some((Plan::Pro, last))),
        (Some(1), Some((Plan::Pro, last))),
        (Some(2), None),
        (None, None),
    ] {
        let got = seat(&seats, place);
        if got != expected {
            wrong.push(format!("место {place:?}: {got:?}"));
        }
    }
    let other = SigningKey::from_bytes(&[9; 32]).verifying_key().to_bytes();
    if !pro_seats(&other, [&two]).is_empty() {
        wrong.push("чужая подпись дала места".to_owned());
    }
    if !pro_seats(&public(), [&standard()]).is_empty() {
        wrong.push("«Стандарт» дал места".to_owned());
    }
    let old = key("07654321", Plan::Pro, issued, day(2027, 1, 31));
    let old_seats = pro_seats(&public(), [&old]);
    if seat(&old_seats, Some(29)) != Some((Plan::Pro, day(2027, 1, 31))) || seat(&old_seats, Some(30)).is_some() {
        wrong.push("«Pro» версии 1 — не 30 мест".to_owned());
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// Из ключей «Pro» с местом для ИП — самый долгий, и кончившийся тоже: его
/// помнит устройство, как и ключ на ՀՎՀՀ. Ключ короче, но на больше мест,
/// открывает дальние места до своего дня.
#[test]
fn the_longest_pro_key_with_a_place_counts() {
    let issued = day(2026, 11, 1);
    let wide = pro("07654321", issued, day(2027, 3, 31), 50);
    let long = pro("07654321", issued, day(2027, 10, 31), 30);
    let ended = pro("07654321", day(2025, 11, 1), day(2026, 10, 31), 30);
    let all = pro_seats(&public(), [&wide, &long, &ended]);
    assert_eq!(seat(&all, Some(0)), Some((Plan::Pro, day(2027, 10, 31))));
    assert_eq!(seat(&all, Some(40)), Some((Plan::Pro, day(2027, 3, 31))));
    assert_eq!(seat(&pro_seats(&public(), [&ended]), Some(0)), Some((Plan::Pro, day(2026, 10, 31))));
}

/// Один и тот же ключ в нескольких записях — одно место в списке, а не
/// столько, сколько записей: подпись его проверяется один раз (ревью задачи
/// 38, чужой файл копии с тысячей одинаковых записей).
#[test]
fn the_same_pro_key_counts_once() {
    let one = pro("07654321", day(2026, 11, 1), day(2027, 10, 31), 30);
    let other = pro("07654321", day(2026, 11, 1), day(2027, 6, 30), 30);
    let copies = [one.clone(), one.clone(), one.clone(), other.clone(), one.clone()];
    assert_eq!(
        pro_seats(&public(), copies.iter()),
        [(30, day(2027, 10, 31)), (30, day(2027, 6, 30))]
    );
}

/// Место в ключе «Pro» и свой ключ на ՀՎՀՀ профиля — срок у самого долгого,
/// при равном последнем дне — свой; без места доступ тот же, что был.
#[test]
fn a_seat_joins_the_own_key() {
    let on = tin("00054321");
    let start = day(2026, 10, 27);
    let own = Access::new(&public(), [&standard()], Some(&on), Some(start));
    let mut wrong = Vec::new();
    for (seat, expected) in [
        (None, Some((Plan::Standard, day(2027, 10, 31)))),
        (Some((Plan::Pro, day(2028, 1, 31))), Some((Plan::Pro, day(2028, 1, 31)))),
        (Some((Plan::Pro, day(2027, 10, 31))), Some((Plan::Standard, day(2027, 10, 31)))),
        (Some((Plan::Pro, day(2027, 1, 31))), Some((Plan::Standard, day(2027, 10, 31)))),
    ] {
        if own.with_seat(seat).key() != expected {
            wrong.push(format!("{seat:?}: {:?}", own.with_seat(seat).key()));
        }
    }
    let trial = Access::new(&public(), [], None, Some(start));
    let seated = trial.with_seat(Some((Plan::Pro, day(2027, 10, 31))));
    if seated.term() != Some(Term::Key { plan: Plan::Pro, last: day(2027, 10, 31) }) {
        wrong.push(format!("пробный период и место: {:?}", seated.term()));
    }
    if trial.with_seat(None) != trial {
        wrong.push("без места доступ сменился".to_owned());
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// Ключ «Pro» конторы — запись своего вида: читается обратно, тот же ключ
/// второй раз не вводится, а «Стандарт» в контору не ложится.
#[test]
fn an_office_license_takes_only_pro_once() {
    let now = Timestamp::from_unix_millis(1_790_000_000_000);
    let new = pro("07654321", day(2026, 11, 1), day(2027, 10, 31), 30);
    let first = OfficeLicense::enter(new.clone(), &[], now).expect("новый ключ Pro");
    let text = encode(&first);
    assert!(text.contains(&format!(r#""key":"{}""#, new.text())), "{text}");
    assert_eq!(decode::<OfficeLicense>(&text), Ok(first.clone()));
    assert_eq!(OfficeLicense::enter(new.clone(), std::slice::from_ref(&first), now), None);
    assert_eq!(OfficeLicense::enter(standard(), &[], now), None);
    let old = key("07654321", Plan::Pro, day(2026, 11, 1), day(2027, 1, 31));
    let second = OfficeLicense::enter(old.clone(), std::slice::from_ref(&first), now).expect("другой ключ Pro");
    let entered = [first, second];
    let keys: Vec<&Key> = OfficeLicense::keys(&entered).collect();
    assert_eq!(keys, [&new, &old]);
    assert_eq!(OfficeLicense::KIND, "office_license");
}
