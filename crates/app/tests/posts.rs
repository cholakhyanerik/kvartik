//! Посты и письма `docs/channels/posts.md` (задачи 33 и 36) говорят то же,
//! что программа и сайт: человек публикует их как есть, и пост, обещающий
//! другое, — дефект, как и такая страница (шапка файла).
//!
//! Молча ломается здесь вот что. Посты беты просят слова согласия с условиями
//! лицензии — те же, что страница «Купить» (`Key::BuyAccept`), — а новая
//! редакция условий меняет в них день: участник, приславший слова прежней
//! редакции, согласился с ней, а не с той, по которой выдан ключ. И текст
//! запуска обещал «14 дней — бесплатно и без регистрации», хотя с 0.0.162
//! пробного периода без ключа нет (`license::LAST_TRIAL_START`), а пробуют
//! ключом продавца на `license::TRIAL_KEY_DAYS` дней (задача 113). Оба
//! расхождения прожили в файле неделю после редакции от 21.11.2026 (найдено
//! 2026-11-27).
//!
//! Тест машинный: читает файл с диска. Под wasm32 его нет — там нечего
//! читать.
#![cfg(not(target_arch = "wasm32"))]

use std::fs;
use std::path::Path;

use kvartik_core::i18n::{Key, Lang, fill, plural, t};
use kvartik_core::license::{TRIAL_DAYS, TRIAL_KEY_DAYS};
use kvartik_core::model::date::Date;
use kvartik_core::release::Legal;

/// Текст файла одной строкой: без знаков цитаты `>` в начале строк,
/// с переносами и неразрывными пробелами — обычными. Пост делит фразу по
/// строкам где угодно, а таблица строк ставит неразрывный пробел перед
/// тире и в «30 дней».
fn flat(text: &str) -> String {
    let lines: Vec<&str> = text.lines().map(|line| line.trim_start().trim_start_matches('>')).collect();
    lines.join(" ").split_whitespace().collect::<Vec<_>>().join(" ")
}

fn posts() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/channels/posts.md");
    let text = fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    flat(&text)
}

/// Месяцы по-английски полностью, как их пишет `Key::BuyAccept`. Свои:
/// полные английские месяцы `i18n::locale` наружу не отдаёт, а код
/// программы ради теста не меняется.
const EN_MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// День редакции так, как его пишут слова согласия: «15.11.2026»
/// по-русски и по-армянски, «November 15, 2026» по-английски — полностью,
/// с месяцем: одни число и год («20, 2026») нашлись бы и в любой другой
/// английской дате того же числа, «December 20, 2026», и тест краснел бы зря.
fn edition_day(lang: Lang, day: Date) -> String {
    match lang {
        Lang::Am | Lang::Ru => format!("{:02}.{:02}.{}", day.day(), day.month(), day.year()),
        Lang::En => format!("{} {}, {}", EN_MONTHS[usize::from(day.month() - 1)], day.day(), day.year()),
    }
}

/// Срок словами языка — «30 дней», «30 days», «30 օր», как пишут лендинг
/// и окно (`Key::DaysOne`…), но с обычным пробелом.
fn days(lang: Lang, n: i32) -> String {
    let n = u64::from(n.unsigned_abs());
    flat(&fill(plural(lang, n, Key::DaysOne, Key::DaysFew, Key::DaysMany), &[&n.to_string()]))
}

/// Слова согласия — только действующей редакции: каждая фраза согласия
/// в постах — целиком `Key::BuyAccept` своего языка, и ни одного дня
/// прежних редакций (`Legal::earlier`). Новая редакция условий — новый
/// день в `release.rs` и новые слова в таблице строк, и тест краснеет,
/// пока посты не поправлены.
#[test]
fn posts_ask_for_the_current_terms_edition() {
    let posts = posts();
    let mut wrong = Vec::new();
    for lang in Lang::ALL {
        let accept = flat(t(lang, Key::BuyAccept));
        if !posts.contains(&accept) {
            wrong.push(format!("{lang:?}: нет слов согласия «{accept}»"));
        }
        // Начало фразы — до дня редакции: так находится и фраза с другим
        // днём, которого нет среди прежних (опечатка).
        let stem = accept.split(|c: char| c.is_ascii_digit()).next().unwrap_or_default();
        for (at, _) in posts.match_indices(stem) {
            if !posts[at..].starts_with(&accept) {
                let tail: String = posts[at..].chars().take(accept.chars().count()).collect();
                wrong.push(format!("{lang:?}: слова согласия не действующей редакции: «{tail}»"));
            }
        }
        for &day in Legal::Terms.earlier() {
            let old = edition_day(lang, day);
            if posts.contains(&old) {
                wrong.push(format!("{lang:?}: день прежней редакции условий «{old}»"));
            }
        }
    }
    assert!(wrong.is_empty(), "docs/channels/posts.md: {wrong:#?}");
}

/// Пробуют программу ключом продавца, а не днями без ключа: срока прежнего
/// пробного периода (`license::TRIAL_DAYS`) в постах нет ни на одном языке,
/// а текст запуска (раздел 2, только по-русски) говорит о пробном ключе
/// словами лендинга и страницы «Купить» (`Key::WelcomeTrial` со сроком
/// `license::TRIAL_KEY_DAYS`).
#[test]
fn posts_offer_the_trial_key_and_no_days_without_one() {
    let posts = posts();
    let mut wrong = Vec::new();
    for lang in Lang::ALL {
        let old = days(lang, TRIAL_DAYS);
        if posts.contains(&old) {
            wrong.push(format!("{lang:?}: «{old}» — пробного периода без ключа нет с 0.0.162"));
        }
    }
    let trial = flat(&fill(t(Lang::Ru, Key::WelcomeTrial), &[&days(Lang::Ru, TRIAL_KEY_DAYS)]));
    if !posts.contains(&trial) {
        wrong.push(format!("Ru: нет слов о пробном ключе «{trial}»"));
    }
    assert!(wrong.is_empty(), "docs/channels/posts.md: {wrong:#?}");
}
