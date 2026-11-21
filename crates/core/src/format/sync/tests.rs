//! Тесты кода передачи. Шифра здесь нет — его делает WebCrypto окна, и путь
//! от базы до базы проверяют браузерные тесты окна; вместо шифротекста —
//! любые байты.

use super::*;
use crate::format::encode;
use crate::format::qr;
use crate::model::attachment::FileBytes;
use crate::model::client::{Client, ClientData};
use crate::model::country::Country;
use crate::model::money::Currency;
use crate::model::profile::ProfileData;
use crate::model::text::{Line, Lines};
use crate::i18n::Lang;

fn at(millis: i64) -> Timestamp {
    Timestamp::from_unix_millis(millis)
}

fn line(text: &str) -> Line {
    Line::new(text).expect("годный текст")
}

fn client(name: &str, modified: i64) -> Client {
    Client::create(
        ClientData {
            name: line(name),
            country: Some(Country::ARMENIA),
            address: Lines::default(),
            tin: None,
            tax_id: Line::default(),
            currency: Some(Currency::Amd),
            lang: Some(Lang::Am),
            payment_days: None,
        },
        at(modified),
    )
    .expect("годный клиент")
}

fn profile(name: &str, modified: i64) -> Profile {
    let mut data = ProfileData::default();
    data.name.ru = line(name);
    Profile::save(None, data, at(modified)).expect("годный").expect("новый")
}

/// База, как её отдаёт хранилище: вид, идентификатор, текст записи.
struct Base(Vec<(&'static str, String, String)>);

impl Base {
    fn put<T: Stored>(&mut self, record: &T) {
        self.0.push((T::KIND, record.meta().id().to_string(), encode(record)));
    }

    fn raw(&self) -> Vec<Raw<'_>> {
        self.0
            .iter()
            .map(|(kind, id, text)| Raw {
                kind,
                id,
                text: Some(text),
            })
            .collect()
    }
}

/// База для проб: профиль от 1 000, клиент от 1 000 и от 5 000, файл книги
/// от 6 000 и нечитаемая запись.
fn sample() -> (Base, Profile) {
    let mut base = Base(Vec::new());
    let ip = profile("ИП Арам", 1_000);
    base.put(&ip);
    base.put(&client("Старый", 1_000));
    base.put(&client("Новый", 5_000));
    let file = FileBytes::new(b"%PDF-1.4 sample".to_vec()).expect("годный PDF");
    base.put(&Attachment::create(RecordId::random(), "чек.pdf", file, at(6_000)));
    base.0.push((Client::KIND, RecordId::random().to_string(), "{не json".to_owned()));
    (base, ip)
}

/// С дня — только изменённые с него; файл книги — не в коде, а числом;
/// нечитаемая — числом; профиль — всегда, хоть и не менялся.
#[test]
fn a_delta_takes_changes_and_the_live_profile() {
    let (base, ip) = sample();
    let picked = pick(base.raw(), Some(at(4_000))).expect("собрано");
    let kinds: Vec<&str> = picked.entries().iter().map(Entry::kind).collect();
    assert_eq!(kinds, [Client::KIND, Profile::KIND]);
    assert_eq!(picked.entries()[1].meta().id(), ip.id());
    assert_eq!(picked.skipped(), 1);
    assert_eq!(picked.unreadable(), 1);
    assert_eq!(picked.changed(Some(at(4_000))), 1, "профиль не в счёт изменённых");
    let all = pick(base.raw(), None).expect("собрано");
    assert_eq!(all.entries().len(), 3, "все записи, кроме файла и нечитаемой");
    let none = pick(base.raw(), Some(at(9_000))).expect("собрано");
    assert_eq!(none.entries().len(), 1, "только профиль");
    assert_eq!(none.skipped(), 0);
}

/// Удалённая запись — тоже изменение: её метка уходит в код, и на другом
/// устройстве запись удаляется.
#[test]
fn a_deletion_travels() {
    let mut base = Base(Vec::new());
    let gone = client("Удалённый", 1_000).delete(at(7_000)).expect("удаляется");
    base.put(&gone);
    let picked = pick(base.raw(), Some(at(6_000))).expect("собрано");
    assert_eq!(picked.entries().len(), 1);
    assert!(picked.entries()[0].meta().is_deleted());
}

/// Запись новее программы — отказ целиком, как у копии: её писала новая
/// версия, и код сделает она.
#[test]
fn a_newer_record_refuses_the_code() {
    let mut base = Base(Vec::new());
    base.0.push((
        Client::KIND,
        RecordId::random().to_string(),
        format!(r#"{{"format_version":{},"record":{{}}}}"#, FORMAT_VERSION + 1),
    ));
    assert!(matches!(pick(base.raw(), None), Err(FormatError::Newer { .. })));
}

fn lock() -> Lock {
    Lock::new(600_000, (0..16).collect(), (100..112).collect()).expect("годный замок")
}

/// Шапка и записи проходят весь путь: сжатие, конверт, «шифр» (здесь —
/// байты как есть), открытие и чтение — те же.
#[test]
fn a_code_reads_back() {
    let (base, _) = sample();
    let picked = pick(base.raw(), Some(at(4_000))).expect("собрано");
    let device = RecordId::random();
    let header = Header::new("0.0.166", at(9_000), device, Some(at(4_000)));
    let packed = pack(&header, picked.entries()).expect("помещается");
    let code = seal(&lock(), &packed);
    let sealed = open(&code).expect("открывается");
    assert_eq!(sealed.lock(), &lock());
    let read = read(&sealed, sealed.ciphertext()).expect("читается");
    assert_eq!(read.header(), &header);
    let ids = |entries: &[Entry]| entries.iter().map(|entry| (entry.kind(), entry.meta().id())).collect::<Vec<_>>();
    assert_eq!(ids(read.entries()), ids(picked.entries()));
}

/// Чужое — не код; код новее программы — «новее», до пароля; обрезанный
/// и с соседней версией конверта — порча.
#[test]
fn foreign_newer_and_broken_codes() {
    let code = seal(&lock(), &[7; 40]);
    assert_eq!(open(b"hello"), Err(CodeError::NotCode));
    assert_eq!(open(b""), Err(CodeError::NotCode));
    let mut newer_envelope = code.clone();
    newer_envelope[3] = ENVELOPE + 1;
    assert_eq!(open(&newer_envelope), Err(CodeError::Newer));
    let mut newer_format = code.clone();
    newer_format[4..8].copy_from_slice(&(FORMAT_VERSION + 1).to_be_bytes());
    assert_eq!(open(&newer_format), Err(CodeError::Newer));
    assert!(matches!(open(&code[..30]), Err(CodeError::Damaged(_))));
    let mut zero = code.clone();
    zero[3] = 0;
    assert!(matches!(open(&zero), Err(CodeError::Damaged(_))));
    // Расшифрованное — не сжатый текст или не тот: порча, а не паника.
    let sealed = open(&code).expect("открывается");
    assert!(matches!(read(&sealed, &[1, 2, 3]), Err(CodeError::Damaged(_))));
}

/// Запись конторы и двойная запись в коде — порча всего кода.
#[test]
fn office_records_and_doubles_are_refused() {
    let (base, _) = sample();
    let picked = pick(base.raw(), None).expect("собрано");
    let header = Header::new("0.0.166", at(9_000), RecordId::random(), None);
    let mut doubled = picked.entries().to_vec();
    doubled.push(doubled[0].clone());
    let packed = pack(&header, &doubled).expect("помещается");
    let sealed = open(&seal(&lock(), &packed)).expect("открывается");
    assert!(matches!(read(&sealed, sealed.ciphertext()), Err(CodeError::Damaged(_))));
    let office = r#"{"file":"kvartik-sync","format_version":25,"app_version":"0.0.166","created":1,"device":"0f8c3a52-5b1e-4c7d-9a2b-6e4f1d3c2b1a","since":null,"records":[{"kind":"ip","record":{}}]}"#;
    let packed = miniz_oxide::deflate::compress_to_vec(office.as_bytes(), 6);
    let sealed = open(&seal(&lock(), &packed)).expect("открывается");
    assert!(matches!(read(&sealed, sealed.ciphertext()), Err(CodeError::Damaged(_))));
}

/// Изменений больше предела — отказ с числом байтов, а не код, который
/// никто не снимет.
#[test]
fn too_many_changes_are_refused() {
    let mut base = Base(Vec::new());
    base.put(&profile("ИП", 1_000));
    // Имена из случайных букв не сжимаются.
    let mut seed = 1u64;
    for _ in 0..1_500 {
        let name: String = (0..60)
            .map(|_| {
                seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
                char::from(b'a' + (seed >> 59) as u8 % 26)
            })
            .collect();
        base.put(&client(&name, 2_000));
    }
    let picked = pick(base.raw(), None).expect("собрано");
    let header = Header::new("0.0.166", at(9_000), RecordId::random(), None);
    let refused = pack(&header, picked.entries()).expect_err("больше предела");
    assert!(refused.bytes > MAX_CODE_BYTES);
}

fn sample_code(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i * 31 % 251) as u8).collect()
}

/// Кадры собираются в любом порядке и с повторами; кадр другой передачи
/// начинает сборку заново.
#[test]
fn frames_collect_in_any_order() {
    let code = sample_code(2_000);
    let frames = super::frames(&code, QR_CHUNK, 77);
    assert_eq!(frames.len(), 2_000usize.div_ceil(QR_CHUNK));
    let mut collector = Collector::default();
    let order: Vec<usize> = (0..frames.len()).rev().chain([0, 3]).collect();
    for (step, &i) in order.iter().enumerate() {
        assert!(collector.whole().is_none() || step >= frames.len());
        let took = collector.take(frame(&frames[i]).expect("кадр"));
        assert_eq!(took, if step < frames.len() { Took::New } else { Took::Again });
    }
    assert_eq!(collector.got(), frames.len());
    assert_eq!(collector.whole(), Some(code.clone()));
    let other = super::frames(&code, QR_CHUNK, 78);
    assert_eq!(collector.take(frame(&other[1]).expect("кадр")), Took::Restarted);
    assert_eq!((collector.got(), collector.count()), (1, other.len()));
    assert_eq!(collector.whole(), None);
}

/// Чужие байты — не кадр; кадр новой версии — «новее»; номер за числом
/// кадров и пустой кусок — не кадр.
#[test]
fn foreign_frames_are_not_taken() {
    let good = super::frames(&sample_code(50), QR_CHUNK, 1).remove(0);
    assert!(frame(&good).is_ok());
    assert_eq!(frame(b"https://example.com"), Err(FrameError::NotFrame));
    // Посторонний QR-код со словом программы — не кадр, а не «новее»
    // (ревью задачи 110).
    assert_eq!(frame(b"Kvartik"), Err(FrameError::NotFrame));
    assert_eq!(frame(b"KVARTIK code"), Err(FrameError::NotFrame));
    assert_eq!(frame(&[]), Err(FrameError::NotFrame));
    let mut newer = good.clone();
    newer[3] = FRAME_VERSION + 1;
    assert_eq!(frame(&newer), Err(FrameError::Newer));
    let mut beyond = good.clone();
    beyond[8..10].copy_from_slice(&5u16.to_be_bytes());
    beyond[10..12].copy_from_slice(&5u16.to_be_bytes());
    assert_eq!(frame(&beyond), Err(FrameError::NotFrame));
    assert_eq!(frame(&good[..FRAME_HEADER]), Err(FrameError::NotFrame));
}

/// Каждый QR-код передачи — версии 12, и с его модулей читается его кадр.
#[test]
fn qr_codes_carry_the_frames() {
    let code = sample_code(1_000);
    let codes = qr_codes(&code, 9);
    let frames = super::frames(&code, QR_CHUNK, 9);
    assert_eq!(codes.len(), frames.len());
    for (qr_code, frame) in codes.iter().zip(&frames) {
        assert_eq!(qr_code.version(), QR_VERSION);
        assert_eq!(qr::read_code(qr_code).as_ref(), Some(frame));
    }
}

/// Куски текста — с меткой, не длиннее сообщения Telegram, только буквы
/// и цифры; находятся во вставленном тексте с подписями, переносами
/// и в обратном порядке.
#[test]
fn text_parts_survive_a_messenger() {
    let code = sample_code(7_000);
    let parts = text_parts(&code, 5);
    assert_eq!(parts.len(), 3);
    for part in &parts {
        assert!(part.starts_with(TEXT_PREFIX));
        assert!(part.chars().count() + 100 <= 4_096, "кусок длиннее сообщения: {}", part.len());
        assert!(part.bytes().all(|b| b.is_ascii_alphanumeric()));
    }
    let pasted = format!(
        "Kvartik: изменения с 21.11.2026, часть 3 из 3\n{}\n\n— Пересланное сообщение —\nKvartik, часть 1 из 3:\r\n{} \n{}.",
        parts[2], parts[0], parts[1]
    );
    let mut collector = Collector::default();
    for found in frames_in_text(&pasted) {
        collector.take(found.expect("кадр"));
    }
    assert_eq!(collector.whole(), Some(code));
    assert!(frames_in_text("KVARTIK KVARTIKzz обычный текст").iter().all(Result::is_err));
    // Слово длиннее куска не разбирается вовсе.
    let huge = format!("{TEXT_PREFIX}{}", "a".repeat(MAX_TEXT_WORD));
    assert!(frames_in_text(&huge).is_empty());
}

/// base62 — туда и обратно для любой длины хвоста; чужие знаки, длина
/// хвоста, какой не бывает, и число больше хвоста — не байты.
#[test]
fn base62_round_trips() {
    for len in 0..40 {
        let bytes: Vec<u8> = (0..len).map(|i| (i * 97 + 255) as u8).collect();
        assert_eq!(base62::decode(&base62::encode(&bytes)), Some(bytes.clone()), "длина {len}");
        let full = vec![255u8; len];
        assert_eq!(base62::decode(&base62::encode(&full)), Some(full), "длина {len}, все 255");
    }
    assert_eq!(base62::encode(&[0; 8]).len(), 11);
    assert_eq!(base62::decode("abc-"), None);
    assert_eq!(base62::decode("a"), None, "хвоста из одного знака не бывает");
    assert_eq!(base62::decode("zzz"), None, "3 знака — два байта, а zzz больше 65 535");
}

/// Принятое помнится по отправителю, только растёт, не больше предела,
/// и испорченная запись не мешает остальным.
#[test]
fn received_is_remembered_per_sender() {
    let a = RecordId::random();
    let b = RecordId::random();
    let mut received = Received::default();
    received.note(a, at(5_000));
    received.note(b, at(7_000));
    received.note(a, at(3_000));
    assert_eq!(received.until(a), Some(at(5_000)), "назад не ходит");
    let text = received.to_text();
    assert_eq!(Received::parse(&text), received);
    let broken = format!("{text};мусор;{}=не число", RecordId::random());
    assert_eq!(Received::parse(&broken), received);
    for _ in 0..MAX_PEERS {
        received.note(RecordId::random(), at(1));
    }
    assert_eq!(received.until(a), None, "самый давний забыт");
    assert_eq!(Received::parse(&received.to_text()).peers.len(), MAX_PEERS);
}

/// Ряд цел, пока код начинается не позже принятого; все записи — всегда;
/// от нового отправителя — «впервые».
#[test]
fn continuity_names_the_gap() {
    assert_eq!(continuity(None, None), Continuity::Whole);
    assert_eq!(continuity(None, Some(at(1))), Continuity::Whole);
    assert_eq!(continuity(Some(at(5)), Some(at(5))), Continuity::Whole);
    assert_eq!(continuity(Some(at(4)), Some(at(5))), Continuity::Whole);
    assert_eq!(continuity(Some(at(6)), Some(at(5))), Continuity::Gap { got: at(5) });
    assert_eq!(continuity(Some(at(6)), None), Continuity::First);
}

/// Начало дня — по самому раннему поясу: 21 ноября 2026 года начинается
/// в UTC+14 в 10:00 UTC 20 ноября — 1 795 168 800 000 мс (посчитано не этим
/// кодом, а `datetime` Python: полночь 21.11.2026 в поясе +14:00).
#[test]
fn a_day_starts_in_the_earliest_zone() {
    let day = Date::new(2026, 11, 21).expect("дата");
    assert_eq!(since_day(day).unix_millis(), 1_795_168_800_000);
}

/// День обратно из начала дня — тот же; день, покрывающий момент, начинается
/// не позже его в любом поясе.
#[test]
fn days_back_from_moments() {
    let day = Date::new(2026, 11, 21).expect("дата");
    assert_eq!(day_of_since(since_day(day)), Some(day));
    // 21.11.2026 00:30 UTC: в UTC−12 это ещё 20-е.
    let got = at(1_795_221_000_000);
    let covering = day_covering(got).expect("день");
    assert_eq!(covering, Date::new(2026, 11, 20).expect("дата"));
    assert!(since_day(covering) <= got);
    assert_eq!(continuity(Some(since_day(covering)), Some(got)), Continuity::Whole);
}

/// День по умолчанию — прошлого кода, а легли записи старше — их день, не
/// позже сегодня; без прошлого кода — все записи, что бы ни легло.
#[test]
fn the_default_day() {
    let d = |n: u8| Date::new(2026, 11, n).expect("дата");
    assert_eq!(default_day(Some(d(10)), None, d(21)), Some(d(10)));
    assert_eq!(default_day(Some(d(10)), Some(d(3)), d(21)), Some(d(3)), "легли записи старше");
    assert_eq!(default_day(Some(d(10)), Some(d(15)), d(21)), Some(d(10)));
    assert_eq!(default_day(Some(d(25)), None, d(21)), Some(d(21)), "часы убегали вперёд");
    assert_eq!(default_day(None, Some(d(3)), d(21)), None);
    assert_eq!(default_day(None, None, d(21)), None);
}
