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

/// Квитанция (задача 116): что принято — читается обратно той же; ряд цел
/// или дыра с моментом — отправителю словами, а квитанция чужого кода или
/// чужому устройству — «другая».
#[test]
fn a_receipt_says_what_was_taken() {
    let (me, them) = (RecordId::random(), RecordId::random());
    let header = Header::new("0.0.172", at(9_000), me, Some(at(5_000)));
    let mut wrong = Vec::new();
    for (name, continuity, expected) in [
        ("ряд цел", Continuity::Whole, Acked::Whole),
        ("впервые", Continuity::First, Acked::First),
        ("дыра", Continuity::Gap { got: at(3_000) }, Acked::Gap { got: at(3_000) }),
    ] {
        let receipt = Receipt::new(&header, them, continuity);
        let back = Receipt::read(&receipt.plain());
        if back.as_ref() != Ok(&receipt) {
            wrong.push(format!("{name}: не прочлась {back:?}"));
        }
        if acked(&receipt, me, at(9_000)) != expected {
            wrong.push(format!("{name}: не то"));
        }
        if acked(&receipt, them, at(9_000)) != Acked::Other || acked(&receipt, me, at(9_001)) != Acked::Other {
            wrong.push(format!("{name}: чужая квитанция своя"));
        }
    }
    let receipt = String::from_utf8(Receipt::new(&header, them, Continuity::Whole).plain()).expect("текст");
    assert!(receipt.contains(r#""taken":"whole""#), "вид квитанции: {receipt}");
    for broken in [
        receipt.replace(r#""taken":"whole""#, r#""taken":"partly""#),
        receipt.replace("kvartik-receipt", "kvartik-sync"),
        receipt.replace(r#""taken":"whole""#, r#""taken":"whole","extra":1"#),
        receipt.replace(r#""taken":"whole""#, r#""taken":{"gap":{"got":1,"extra":2}}"#),
        "не JSON".to_owned(),
    ] {
        if Receipt::read(broken.as_bytes()).is_ok() {
            wrong.push(format!("прочитана испорченная: {broken}"));
        }
    }
    assert!(wrong.is_empty(), "квитанция: {wrong:#?}");
}

/// Квитанция — один QR-код и один кусок текста: её показывают в ответ, и круга
/// у неё нет. Конверт открывается обратно, а чужие байты и код — нет.
#[test]
fn a_receipt_is_one_qr_code() {
    let header = Header::new("0.0.172", at(1_795_346_426_590), RecordId::random(), None);
    let plain = Receipt::new(&header, RecordId::random(), Continuity::Gap { got: at(1_795_346_000_000) }).plain();
    // «Шифротекст» — того же размера, что даст AES-GCM: текст и метка.
    let ciphertext = vec![7u8; plain.len() + backup::TAG_BYTES];
    let iv = [1u8; backup::IV_BYTES];
    let sealed = seal_receipt(&iv, &ciphertext);
    assert_eq!(sync_frames(&sealed), 1, "квитанция — не один кадр: {} Б", sealed.len());
    assert_eq!(qr_codes(&sealed, 1).len(), 1);
    // Своим QR-кодом — меньше кода передачи, и сканер ядра его читает.
    let small = receipt_qr(&sealed, 1).expect("помещается");
    let big = qr_codes(&sealed, 1).remove(0);
    assert!(small.side() < big.side(), "квитанция не меньше кода: {} против {}", small.side(), big.side());
    let pixels: Vec<u8> = (0..(small.side() + 8).pow(2))
        .map(|i| {
            let side = small.side() + 8;
            let (x, y) = (i % side, i / side);
            let dark = x >= 4 && y >= 4 && x - 4 < small.side() && y - 4 < small.side() && small.dark(x - 4, y - 4);
            if dark { 0 } else { 255 }
        })
        .collect();
    let side = small.side() + 8;
    let scaled: Vec<u8> = (0..(side * 4).pow(2)).map(|i| pixels[(i / (side * 4) / 4) * side + (i % (side * 4)) / 4]).collect();
    let found = qr::scan(&qr::Gray {
        width: side * 4,
        height: side * 4,
        pixels: &scaled,
    });
    assert_eq!(found.len(), 1, "сканер не прочёл QR-код квитанции");
    assert_eq!(frame(&found[0]).map(|frame| frame.chunk), Ok(sealed.clone()));
    assert_eq!(text_parts(&sealed, 1).len(), 1);
    assert_eq!(payload(&sealed), Payload::Receipt);
    let opened = open_receipt(&sealed).expect("открывается");
    assert_eq!((opened.iv(), opened.ciphertext()), (&iv[..], &ciphertext[..]));

    let code = seal(&lock(), &[9; 40]);
    assert_eq!(payload(&code), Payload::Code);
    assert_eq!(open_receipt(&code), Err(CodeError::NotCode));
    assert_eq!(payload(b"KV"), Payload::Other);
    let mut newer = sealed.clone();
    newer[3] = 2;
    assert_eq!(open_receipt(&newer), Err(CodeError::Newer));
    let mut wrong = Vec::new();
    for (name, bytes) in [
        ("без шифротекста", sealed[..4 + backup::IV_BYTES + 3].to_vec()),
        ("версия 0", [&sealed[..3], &[0], &sealed[4..]].concat()),
        ("длиннее предела", [sealed.clone(), vec![0; MAX_RECEIPT_BYTES]].concat()),
    ] {
        if !matches!(open_receipt(&bytes), Err(CodeError::Damaged(_))) {
            wrong.push(name);
        }
    }
    assert!(wrong.is_empty(), "открыта испорченная квитанция: {wrong:?}");
}

/// Сколько QR-кадров у этих байтов.
fn sync_frames(bytes: &[u8]) -> usize {
    frames(bytes, QR_CHUNK, 1).len()
}

/// Код всех ИП (задача 116): записи ИП списка — всегда, ключи «Pro» и записи
/// баз — изменённые, у записей баз — метка своего ИП; код читается обратно
/// теми же записями и ложится маршрутом копии всех ИП.
#[test]
fn an_office_code_carries_every_ip() {
    use crate::format::office;
    use crate::model::office::Ip;
    let day = Date::new(2026, 11, 1).expect("дата");
    let (first, second) = (Ip::new(day, at(500)), Ip::new(day, at(600)));
    let mut office_base = Base(Vec::new());
    office_base.put(&first);
    office_base.put(&second);
    let office_picked = pick_office(office_base.raw(), Some(at(4_000))).expect("собрано");
    assert_eq!(office_picked.entries().len(), 2, "записи ИП — всегда");
    assert_eq!(office_picked.changed(Some(at(4_000))), 0, "неизменённые ИП не в счёт");

    let (base_one, ip_one) = sample();
    let mut base_two = Base(Vec::new());
    let ip_two = profile("ИП Вахе", 1_000);
    base_two.put(&ip_two);
    base_two.put(&client("Клиент Вахе", 7_000));
    let mut picked = office_picked;
    picked.absorb(pick(base_one.raw(), Some(at(4_000))).expect("собрано").in_base(first.id()));
    picked.absorb(pick(base_two.raw(), Some(at(4_000))).expect("собрано").in_base(second.id()));
    assert_eq!(picked.changed(Some(at(4_000))), 2, "по клиенту в каждой базе");
    assert_eq!(picked.skipped(), 1);

    let header = Header::new("0.0.173", at(9_000), RecordId::random(), Some(at(4_000)));
    let packed = pack(&header, picked.entries()).expect("помещается");
    let sealed = open(&seal(&lock(), &packed)).expect("открывается");
    let code = read(&sealed, sealed.ciphertext()).expect("читается");
    assert!(backup::is_full(code.entries()), "код всех ИП");
    let bases: Vec<Option<RecordId>> = code.entries().iter().map(Entry::base).collect();
    assert_eq!(bases.iter().filter(|base| base.is_none()).count(), 2, "записи ИП — без метки");
    let route = office::route(code.into_entries(), &[], None).expect("маршрут");
    let landed: Vec<usize> = route.bases.iter().map(|landing| landing.entries.len()).collect();
    assert_eq!(landed, [2, 2], "в каждую базу — клиент и профиль");
    let profiles: Vec<RecordId> = route
        .bases
        .iter()
        .flat_map(|landing| landing.entries.iter().filter(|entry| entry.kind() == Profile::KIND))
        .map(|entry| entry.meta().id())
        .collect();
    assert_eq!(profiles, [ip_one.id(), ip_two.id()]);
}

/// Запись с меткой ИП, которого в коде нет, и записи конторы рядом с записями
/// без метки — порча всего кода: такие некуда положить.
#[test]
fn an_office_code_without_its_ips_is_refused() {
    use crate::model::office::Ip;
    let (base, _) = sample();
    let header = Header::new("0.0.173", at(9_000), RecordId::random(), None);
    let tagged = pick(base.raw(), None).expect("собрано").in_base(RecordId::random());
    let packed = pack(&header, tagged.entries()).expect("помещается");
    let sealed = open(&seal(&lock(), &packed)).expect("открывается");
    assert!(matches!(read(&sealed, sealed.ciphertext()), Err(CodeError::Damaged(_))), "метка без своего ИП");

    let mut office_base = Base(Vec::new());
    office_base.put(&Ip::new(Date::new(2026, 11, 1).expect("дата"), at(500)));
    let mut mixed = pick_office(office_base.raw(), None).expect("собрано");
    mixed.absorb(pick(base.raw(), None).expect("собрано"));
    let packed = pack(&header, mixed.entries()).expect("помещается");
    let sealed = open(&seal(&lock(), &packed)).expect("открывается");
    assert!(matches!(read(&sealed, sealed.ciphertext()), Err(CodeError::Damaged(_))), "контора с записями без метки");
}

/// ИП кода «с дня», которого здесь нет, — назван, если он не новый: новый
/// пришёл всеми записями, а у знакомого — та же запись, профиль или ИНН.
#[test]
fn an_unknown_ip_of_a_delta_is_named() {
    use crate::format::office;
    use crate::model::office::Ip;
    let day = Date::new(2026, 11, 1).expect("дата");
    let old = Ip::new(day, at(500));
    let new = Ip::new(day, at(5_000));
    let mut office_base = Base(Vec::new());
    office_base.put(&old);
    office_base.put(&new);
    let entries = pick_office(office_base.raw(), None).expect("собрано").entries().to_vec();
    let missing = office::missing(&entries, &[], Some(at(4_000))).expect("читается");
    assert_eq!(missing.map(|ip| ip.id()), Some(old.id()), "старый ИП, которого здесь нет");
    assert_eq!(office::missing(&entries, std::slice::from_ref(&old), Some(at(4_000))).expect("читается"), None);
    assert_eq!(
        office::missing(&entries, &[], Some(at(400))).expect("читается"),
        None,
        "оба добавлены после начала кода"
    );
    assert!(
        office::missing(&entries, std::slice::from_ref(&old), None).expect("читается").is_some(),
        "коду файлов новых нет"
    );
    // Тот же ИНН, но свой профиль и своя запись ИП — данных того устройства
    // здесь нет (ревью задачи 116).
    let mut theirs = Ip::new(day, at(500));
    let data = ProfileData {
        tin: crate::model::tin::Tin::parse("07654321").ok(),
        ..ProfileData::default()
    };
    theirs.copy_profile(&Profile::save(None, data.clone(), at(500)).expect("годный").expect("новый"), at(600));
    let mut ours = Ip::new(day, at(500));
    ours.copy_profile(&Profile::save(None, data, at(500)).expect("годный").expect("новый"), at(600));
    let mut office_base = Base(Vec::new());
    office_base.put(&theirs);
    let entries = pick_office(office_base.raw(), None).expect("собрано").entries().to_vec();
    assert_eq!(
        office::missing(&entries, &[ours], Some(at(4_000))).expect("читается").map(|ip| ip.id()),
        Some(theirs.id()),
        "совпал только ИНН"
    );
}

/// Файл книги с `len` байтами, которые не сжимаются.
fn noisy_file(len: usize, modified: i64) -> Attachment {
    let mut seed = len as u64 + 7;
    let mut bytes = b"%PDF-1.4 ".to_vec();
    bytes.extend((0..len).map(|_| {
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        (seed >> 56) as u8
    }));
    let file = FileBytes::new(bytes).expect("годный PDF");
    Attachment::create(RecordId::random(), "договор.pdf", file, at(modified))
}

/// Код файлов (задача 116): файлы книги, изменённые с дня, и живой профиль —
/// серией частей, каждая не больше обычного кода; части собираются в любом
/// порядке и с повторами, а собранный код читается обратно с меткой `files`.
#[test]
fn files_travel_as_a_series() {
    let (mut base, ip) = sample();
    let (big, old) = (noisy_file(70_000, 7_000), noisy_file(1_000, 500));
    base.put(&big);
    base.put(&old);
    let picked = pick_files(base.raw(), Some(at(4_000))).expect("собрано");
    let kinds: Vec<&str> = picked.entries().iter().map(Entry::kind).collect();
    assert_eq!(kinds, [Attachment::KIND, Attachment::KIND, Profile::KIND], "два изменённых файла и профиль");
    assert_eq!(picked.entries()[2].meta().id(), ip.id());

    let header = Header::new("0.0.174", at(9_000), RecordId::random(), Some(at(4_000))).for_files();
    let packed = pack(&header, picked.entries()).expect("помещается в серию");
    let code = seal(&lock(), &packed);
    assert!(code.len() > MAX_CODE_BYTES, "в один код не поместился бы");
    let parts = segments(&code, 77);
    assert_eq!(parts.len(), code.len().div_ceil(SEGMENT_BYTES));
    assert!(parts.iter().all(|part| part.len() <= MAX_CODE_BYTES && payload(part) == Payload::Segment));
    assert!(parts.iter().all(|part| sync_frames(part) <= MAX_FRAMES), "часть — обычный круг кадров");

    let mut series = Series::default();
    for part in parts.iter().rev().chain(parts.iter()) {
        series.take(part).expect("часть");
    }
    assert_eq!((series.got(), series.count()), (parts.len(), parts.len()));
    let whole = series.whole().expect("собран");
    assert_eq!(whole, code);
    assert_eq!(open(&whole).err(), Some(CodeError::Damaged("конверт кода не той версии или длины".to_owned())));
    let sealed = open_series(&whole).expect("открывается");
    let read = read(&sealed, sealed.ciphertext()).expect("читается");
    assert!(read.header().files());
    assert_eq!(read.entries().len(), 3);
}

/// Части серии: чужие байты и часть новой версии — не части, часть другой
/// серии начинает сборку заново, а файлов больше, чем вмещает серия, — отказ
/// с числом байтов, а не серия, которую никто не снимет.
#[test]
fn a_series_is_whole_or_refused() {
    let code = vec![9u8; SEGMENT_BYTES + 10];
    let (first, second) = (segments(&code, 1), segments(&code, 2));
    let mut series = Series::default();
    assert_eq!(series.take(&first[0]), Ok(Took::New));
    assert_eq!(series.take(&first[0]), Ok(Took::Again));
    assert_eq!(series.take(&second[1]), Ok(Took::Restarted));
    assert_eq!(series.whole(), None);
    assert_eq!(series.take(&second[0]), Ok(Took::New));
    assert_eq!(series.whole(), Some(code));
    let mut newer = first[0].clone();
    newer[3] = 2;
    assert_eq!(series.take(&newer), Err(FrameError::Newer));
    assert_eq!(series.take(b"KVF\x01"), Err(FrameError::NotFrame));
    let mut too_many = first[0].clone();
    too_many[10..12].copy_from_slice(&u16::try_from(MAX_SEGMENTS + 1).expect("число").to_be_bytes());
    assert_eq!(series.take(&too_many), Err(FrameError::NotFrame));

    let mut base = Base(Vec::new());
    base.put(&noisy_file(MAX_SERIES_BYTES, 7_000));
    let picked = pick_files(base.raw(), None).expect("собрано");
    let header = Header::new("0.0.174", at(9_000), RecordId::random(), None).for_files();
    assert!(matches!(pack(&header, picked.entries()), Err(TooLarge { bytes }) if bytes > MAX_SERIES_BYTES));
}

/// Встречный код (задача 124, п. 14) не возвращает отправителю его записей:
/// запись, которая здесь та же, что пришла, — тот же момент и тот же текст, —
/// выпадает, а профиль остаётся, но изменённым не считается. Запись, правленная
/// здесь позже, идёт; и с моментом пришедшей, но другим текстом — тоже: так
/// выходит у слияния по полям с правкой этого устройства в другом поле.
#[test]
fn the_back_code_leaves_the_echo_out() {
    let ip = profile("ИП Арам", 8_000);
    let echoed = client("Эхо", 8_000);
    let edited = client("Правленный здесь", 8_000);
    let mixed = client("Слитый", 8_000);
    let mut came = Base(Vec::new());
    for record in [&echoed, &edited, &mixed] {
        came.put(record);
    }
    came.put(&ip);
    let echo = Echo::of(pick(came.raw(), None).expect("собрано").entries());

    // Здесь после слияния: эхо и профиль — те же; «Правленный здесь» правлен
    // позже; «Слитый» — тот же момент, а текст другой; и свой новый клиент.
    let mut here = Base(Vec::new());
    here.put(&ip);
    here.put(&echoed);
    let later = edited
        .edit(ClientData { name: line("Правленный здесь позже"), ..edited.data().clone() }, at(9_000))
        .expect("годный")
        .expect("правка");
    here.put(&later);
    let text = encode(&mixed).replace("Слитый", "Слитый по полям");
    here.0.push((Client::KIND, mixed.id().to_string(), text));
    let own = client("Свой", 9_500);
    here.put(&own);

    let since = Some(at(7_000));
    let picked = pick(here.raw(), since).expect("собрано");
    assert_eq!(picked.changed(since), 5, "без эха — всё изменённое");
    let picked = picked.without_echo(&echo);
    let ids: Vec<RecordId> = picked.entries().iter().map(|entry| entry.meta().id()).collect();
    assert!(!ids.contains(&echoed.id()), "эхо не идёт обратно");
    for (record, why) in [(later.id(), "правка здесь позже"), (mixed.id(), "слитая по полям"), (own.id(), "своя"), (ip.id(), "профиль")] {
        assert!(ids.contains(&record), "нет записи: {why}");
    }
    assert_eq!(picked.changed(since), 3, "профиль-эхо — в коде, но не в счёт");
    assert_eq!(picked.tally(since).clients, 3);
    // Пустое эхо ничего не трогает.
    let all = pick(here.raw(), since).expect("собрано").without_echo(&Echo::default());
    assert_eq!(all.changed(since), 5);
}

/// Код всех ИП (задача 116): эхо узнаётся с меткой базы — запись ИП списка
/// и профиль остаются, хоть и пришли теми же, а клиент той же базы — нет;
/// тот же клиент в другой базе — не эхо.
#[test]
fn the_echo_knows_the_base() {
    let (first, second) = (RecordId::random(), RecordId::random());
    let ip = profile("ИП Арам", 8_000);
    let echoed = client("Эхо", 8_000);
    let mut came = Base(Vec::new());
    came.put(&ip);
    came.put(&echoed);
    let echo = Echo::of(pick(came.raw(), None).expect("собрано").in_base(first).entries());
    let mut back = pick(came.raw(), None).expect("собрано").in_base(first);
    back.absorb(pick(came.raw(), None).expect("собрано").in_base(second));
    let back = back.without_echo(&echo);
    let left: Vec<(Option<RecordId>, &str)> = back.entries().iter().map(|entry| (entry.base(), entry.kind())).collect();
    assert_eq!(
        left,
        [(Some(first), Profile::KIND), (Some(second), Profile::KIND), (Some(second), Client::KIND)]
    );
    assert_eq!(back.changed(None), 2, "профиль второй базы и её клиент; эхо первой — не в счёт");
}

/// Оценка кода файлов до него (задача 124): у несжимаемого файла — не меньше
/// того, что выходит упаковкой, и близко к нему, а больше серии — «не
/// поместится», хотя код ещё не собирали.
#[test]
fn the_files_estimate_is_close_and_not_below() {
    for len in [40_000, 300_000, 900_000] {
        let mut base = Base(Vec::new());
        base.put(&profile("ИП Арам", 1_000));
        base.put(&noisy_file(len, 6_000));
        let changes = pick(base.raw(), None).expect("собрано");
        let estimate = changes.skipped_bytes();
        let files = pick_files(base.raw(), None).expect("собрано");
        let header = Header::new("0.0.191", at(9_000), RecordId::random(), None).for_files();
        let packed = pack(&header, files.entries()).expect("помещается").len();
        assert!(estimate + 1_000 >= packed, "оценка {estimate} меньше кода {packed}");
        assert!(estimate <= packed + packed / 10, "оценка {estimate} много больше кода {packed}");
        assert!(files_fit(estimate), "{len} байтов помещаются");
    }
    let mut base = Base(Vec::new());
    base.put(&noisy_file(MAX_SERIES_BYTES, 6_000));
    let picked = pick(base.raw(), None).expect("собрано");
    assert!(!files_fit(picked.skipped_bytes()), "файл больше серии");
    assert_eq!(picked.skipped(), 1);
}
