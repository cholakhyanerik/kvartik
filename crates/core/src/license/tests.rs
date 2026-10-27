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
    let signature = signer().sign(&Key::message(&tin, plan, issued, until)).to_bytes();
    Key::signed(tin, plan, issued, until, signature).expect("срок не раньше выпуска")
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
    let message = Key::message(&tin("00054321"), Plan::Standard, day(2026, 10, 27), day(2027, 10, 31));
    let (domain, fields) = message.split_at(DOMAIN.len());
    assert_eq!(domain, b"Kvartik license key\n");
    assert_eq!(fields, [1, 0x00, 0x00, 0xD4, 0x31, 1, 0x07, 0xEA, 10, 27, 0x07, 0xEB, 10, 31]);
    let pro = Key::message(&tin("99999999"), Plan::Pro, day(2026, 1, 1), day(2026, 1, 1));
    assert_eq!(pro[DOMAIN.len()..], [1, 0x05, 0xF5, 0xE0, 0xFF, 2, 0x07, 0xEA, 1, 1, 0x07, 0xEA, 1, 1]);
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
        let mut bytes = fields(&key.tin, key.plan, key.issued, key.until).to_vec();
        bytes.extend_from_slice(&key.signature);
        bytes
    };
    let with = |at: usize, value: u8| {
        let mut bytes = good.clone();
        bytes[at] = value;
        base32(&bytes)
    };
    let mut cases = vec![
        (String::new(), KeyError::Malformed),
        ("не ключ".to_owned(), KeyError::Malformed),
        (base32(&good[..BYTES - 1]), KeyError::Malformed),
        (base32(&[good.as_slice(), &[0]].concat()), KeyError::Malformed),
        (with(0, 0), KeyError::Malformed),
        (with(0, 2), KeyError::Newer),
        (with(0, 255), KeyError::Newer),
        (base32(&[2]), KeyError::Newer),
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
    assert_eq!(
        Key::signed(tin("00054321"), Plan::Standard, day(2027, 1, 1), day(2026, 12, 31), [0; SIGNATURE]),
        Err(KeyError::Malformed)
    );
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
