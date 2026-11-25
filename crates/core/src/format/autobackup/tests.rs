//! Тесты автокопии: имена выписаны руками, а не собраны тем же `format!`,
//! что и в коде (Правило 2).

use super::*;

fn day(year: u16, month: u8, day: u8) -> Date {
    Date::new(year, month, day).expect("день есть в календаре")
}

fn tag(text: &str) -> DeviceTag {
    DeviceTag::parse(text).expect("метка по образцу")
}

fn at(millis: i64) -> Timestamp {
    Timestamp::from_unix_millis(millis)
}

#[test]
fn file_name_is_day_then_device() {
    assert_eq!(file_name(day(2026, 11, 3), tag("1a2b")), "kvartik-auto-2026-11-03-1a2b.json");
    assert_eq!(file_name(day(2027, 1, 9), tag("00ff")), "kvartik-auto-2027-01-09-00ff.json");
}

#[test]
fn own_names_are_read_back() {
    let mine = tag("1a2b");
    assert_eq!(day_of("kvartik-auto-2026-11-03-1a2b.json", mine), Some(day(2026, 11, 3)));
    assert_eq!(day_of("kvartik-auto-2024-02-29-1a2b.json", mine), Some(day(2024, 2, 29)));
    // Части одной копии дня (задача 117).
    assert_eq!(day_of("kvartik-auto-2026-11-03-1a2b-1of3.json", mine), Some(day(2026, 11, 3)));
    assert_eq!(day_of("kvartik-auto-2026-11-03-1a2b-3of3.json", mine), Some(day(2026, 11, 3)));
    assert_eq!(day_of("kvartik-auto-2026-11-03-1a2b-12of999.json", mine), Some(day(2026, 11, 3)));
}

/// Всё, что не своё имя целиком, — не файл этого устройства: его нельзя
/// убирать. Провалы собираются и выдаются в конце (Правило 2).
#[test]
fn foreign_names_are_not_ours() {
    let mine = tag("1a2b");
    let foreign = [
        // Другое устройство.
        "kvartik-auto-2026-11-03-9f9f.json",
        // Ручная копия.
        "kvartik-2026-11-03.json",
        // Копия, которую облако или человек переименовали.
        "kvartik-auto-2026-11-03-1a2b (1).json",
        "kvartik-auto-2026-11-03-1a2b.json.bak",
        "Copy of kvartik-auto-2026-11-03-1a2b.json",
        "kvartik-auto-2026-11-03-1a2b.JSON",
        "KVARTIK-AUTO-2026-11-03-1a2b.json",
        // Метка не та по записи.
        "kvartik-auto-2026-11-03-1A2B.json",
        "kvartik-auto-2026-11-03-1a2b0.json",
        "kvartik-auto-2026-11-03-1a2.json",
        "kvartik-auto-2026-11-03.json",
        "kvartik-auto-2026-11-031a2b.json",
        // Часть — не по образцу (задача 117).
        "kvartik-auto-2026-11-03-1a2b-0of3.json",
        "kvartik-auto-2026-11-03-1a2b-4of3.json",
        "kvartik-auto-2026-11-03-1a2b-1of1.json",
        "kvartik-auto-2026-11-03-1a2b-01of3.json",
        "kvartik-auto-2026-11-03-1a2b-1of03.json",
        "kvartik-auto-2026-11-03-1a2b-1of1000.json",
        "kvartik-auto-2026-11-03-1a2b-1of99999999999.json",
        "kvartik-auto-2026-11-03-1a2b-1of.json",
        "kvartik-auto-2026-11-03-1a2b-of3.json",
        "kvartik-auto-2026-11-03-1a2b-1of3x.json",
        "kvartik-auto-2026-11-03-1a2b-1-of-3.json",
        "kvartik-auto-2026-11-03-1a2b-+1of3.json",
        "kvartik-auto-2026-11-03-1a2b1of3.json",
        "kvartik-auto-2026-11-03-1a2b-1of3-1of3.json",
        "kvartik-auto-2026-11-03-9f9f-1of3.json",
        "kvartik-auto-2026-11-03-1a2b-Աof3.json",
        // Дня нет в календаре или он записан не так.
        "kvartik-auto-2026-02-30-1a2b.json",
        "kvartik-auto-2026-1-03-1a2b.json",
        "kvartik-auto-26-11-03-1a2b.json",
        // Не латиница там, где ждут цифры: срез по байтам не должен паниковать.
        "kvartik-auto-2026-11-0Ա-1a2b.json",
        "kvartik-auto-ԱԲԳԴԵԶԷԸԹԺ-1a2b.json",
        "kvartik-auto-.json",
        "",
    ];
    let wrong: Vec<&str> = foreign.into_iter().filter(|name| day_of(name, mine).is_some()).collect();
    assert!(wrong.is_empty(), "прочитаны как свои: {wrong:?}");
}

#[test]
fn stale_keeps_the_newest_own_files() {
    let mine = tag("1a2b");
    let mut names: Vec<String> = (1..=10).map(|n| file_name(day(2026, 11, n), mine)).collect();
    names.push("kvartik-auto-2026-10-01-9f9f.json".to_owned());
    names.push("kvartik-2026-10-01.json".to_owned());
    names.push("отчёт.xlsx".to_owned());
    // Порядок в папке — любой.
    names.reverse();
    names.swap(0, 7);
    let today = file_name(day(2026, 11, 10), mine);
    let mut stale = stale(names.iter().map(String::as_str), mine, day(2026, 11, 10), &[today.as_str()]);
    stale.sort_unstable();
    assert_eq!(
        stale,
        [
            "kvartik-auto-2026-11-01-1a2b.json",
            "kvartik-auto-2026-11-02-1a2b.json",
            "kvartik-auto-2026-11-03-1a2b.json",
        ]
    );
}

#[test]
fn stale_leaves_a_short_folder_alone() {
    let mine = tag("1a2b");
    let names: Vec<String> = (1..=7).map(|n| file_name(day(2026, 11, n), mine)).collect();
    assert!(stale(names.iter().map(String::as_str), mine, day(2026, 11, 7), &[names[6].as_str()]).is_empty());
    // Чужие файлы не убираются, сколько бы их ни было.
    let foreign: Vec<String> = (1..=20).map(|n| file_name(day(2026, 10, n), tag("9f9f"))).collect();
    assert!(stale(foreign.iter().map(String::as_str), mine, day(2026, 11, 7), &[]).is_empty());
}

/// Свои файлы с днём позже сегодняшнего — подложенные в общую папку или
/// записанные, пока часы не ушли назад, — не убираются и не вытесняют
/// сегодняшний: он остаётся, а лишние прошлые по-прежнему убираются.
#[test]
fn stale_never_takes_today_for_files_from_the_future() {
    let mine = tag("1a2b");
    let today = day(2026, 11, 3);
    let future: Vec<String> = (1..=7).map(|n| file_name(day(2099, 1, n), mine)).collect();
    let past: Vec<String> = (1..=8).map(|n| file_name(day(2026, 10, n), mine)).collect();
    let fresh = file_name(today, mine);
    let names: Vec<&str> = future
        .iter()
        .chain(&past)
        .map(String::as_str)
        .chain([fresh.as_str()])
        .collect();
    let mut stale = stale(names, mine, today, &[fresh.as_str()]);
    stale.sort_unstable();
    // Из восьми прошлых и сегодняшнего остаются семь самых свежих: сегодняшний
    // и шесть прошлых, — а из будущего не уходит ни один.
    assert_eq!(
        stale,
        [
            "kvartik-auto-2026-10-01-1a2b.json",
            "kvartik-auto-2026-10-02-1a2b.json",
        ]
    );
}

/// Копия частями (задача 117): остаются все части семи свежих дней, а не семь
/// файлов, и из сегодняшних — только что записанные: прежняя копия дня была
/// в трёх частях, новая — в двух, и `3of3` прежней с ними не смешивается.
#[test]
fn stale_counts_days_and_replaces_the_copy_of_today() {
    let mine = tag("1a2b");
    let today = day(2026, 11, 10);
    let mut names: Vec<String> = Vec::new();
    for n in 1..=9 {
        for part in ["1of3", "2of3", "3of3"] {
            names.push(format!("kvartik-auto-2026-11-0{n}-1a2b-{part}.json"));
        }
    }
    // Прежняя копия сегодня — одним файлом и частями, новая — две части.
    names.push("kvartik-auto-2026-11-10-1a2b.json".to_owned());
    names.extend(["1of3", "2of3", "3of3", "1of2", "2of2"].map(|part| format!("kvartik-auto-2026-11-10-1a2b-{part}.json")));
    // Чужие сегодняшние не трогаются.
    names.push("kvartik-auto-2026-11-10-9f9f-1of2.json".to_owned());
    names.push("kvartik-2026-11-10-1of2.json".to_owned());
    let written = ["kvartik-auto-2026-11-10-1a2b-1of2.json", "kvartik-auto-2026-11-10-1a2b-2of2.json"];
    let mut stale = stale(names.iter().map(String::as_str), mine, today, &written);
    stale.sort_unstable();
    assert_eq!(
        stale,
        [
            "kvartik-auto-2026-11-01-1a2b-1of3.json",
            "kvartik-auto-2026-11-01-1a2b-2of3.json",
            "kvartik-auto-2026-11-01-1a2b-3of3.json",
            "kvartik-auto-2026-11-02-1a2b-1of3.json",
            "kvartik-auto-2026-11-02-1a2b-2of3.json",
            "kvartik-auto-2026-11-02-1a2b-3of3.json",
            "kvartik-auto-2026-11-03-1a2b-1of3.json",
            "kvartik-auto-2026-11-03-1a2b-2of3.json",
            "kvartik-auto-2026-11-03-1a2b-3of3.json",
            "kvartik-auto-2026-11-10-1a2b-1of3.json",
            "kvartik-auto-2026-11-10-1a2b-2of3.json",
            "kvartik-auto-2026-11-10-1a2b-3of3.json",
            "kvartik-auto-2026-11-10-1a2b.json",
        ]
    );
}

/// Брошенные временные файлы своих копий прошлых дней убираются, а свой
/// сегодняшний, чужой и сам файл копии — нет.
#[test]
fn leftovers_are_own_swap_files_of_past_days() {
    let mine = tag("1a2b");
    let today = day(2026, 11, 3);
    let gone = [
        "kvartik-auto-2026-11-02-1a2b.json.crswap",
        "kvartik-auto-2026-11-02-1a2b.json.1.crswap",
        "kvartik-auto-2025-12-31-1a2b.json.17.crswap",
        // Часть копии (задача 117).
        "kvartik-auto-2026-11-02-1a2b-2of3.json.crswap",
    ];
    let kept = [
        // Сегодняшний: его, может быть, пишет другая вкладка.
        "kvartik-auto-2026-11-03-1a2b.json.crswap",
        "kvartik-auto-2026-11-03-1a2b.json.1.crswap",
        // Чужое устройство.
        "kvartik-auto-2026-11-02-9f9f.json.crswap",
        // Сам файл копии и ручная копия.
        "kvartik-auto-2026-11-02-1a2b.json",
        "kvartik-2026-11-02.json.crswap",
        // Не временный файл браузера.
        "kvartik-auto-2026-11-02-1a2b.json.crswap.txt",
        "kvartik-auto-2026-11-02-1a2b.json.x.crswap",
        "kvartik-auto-2026-11-02-1a2b.json..crswap",
        ".crswap",
    ];
    let mut wrong: Vec<String> = gone
        .iter()
        .filter(|name| !leftover(name, mine, today))
        .map(|name| format!("не убран: {name}"))
        .collect();
    wrong.extend(
        kept.iter()
            .filter(|name| leftover(name, mine, today))
            .map(|name| format!("убран: {name}")),
    );
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// Своя папка — «Kvartik» в любом регистре; в выбранной другой она
/// заводится, и человек видит обе: «OneDrive/Kvartik».
#[test]
fn own_folder_is_kvartik_inside_the_chosen_one() {
    let mut wrong = Vec::new();
    for name in ["Kvartik", "kvartik", "KVARTIK"] {
        if !own_folder(name) || place(name) != name {
            wrong.push(format!("своя: {name}"));
        }
    }
    for (name, shown) in [
        ("OneDrive", "OneDrive/Kvartik"),
        ("Мой диск", "Мой диск/Kvartik"),
        ("Kvartik копии", "Kvartik копии/Kvartik"),
        ("Kvartiks", "Kvartiks/Kvartik"),
    ] {
        if own_folder(name) || place(name) != shown {
            wrong.push(format!("чужая: {name} — {}", place(name)));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

#[test]
fn pending_means_a_change_after_the_last_copy() {
    assert!(!pending(None, None), "без изменений копировать нечего");
    assert!(!pending(None, Some(at(5_000))));
    assert!(pending(Some(at(5_000)), None), "копии ещё не было");
    assert!(pending(Some(at(5_001)), Some(at(5_000))));
    assert!(pending(Some(at(5_000)), Some(at(5_000))), "в тот же миг — не в копии");
    assert!(!pending(Some(at(4_999)), Some(at(5_000))));
}

/// Освежить автокопию без изменений — раз в день, когда её не было вовсе
/// и когда её момент «из будущего»; сегодняшняя — не повод.
#[test]
fn refresh_is_due_once_a_day_and_after_the_clock_moved() {
    let today = day(2026, 11, 3);
    let now = at(1_762_150_000_000);
    let earlier = at(1_762_140_000_000);
    let mut wrong = Vec::new();
    let mut expect = |what: &str, seen: bool, want: bool| {
        if seen != want {
            wrong.push(format!("{what}: {seen}, а не {want}"));
        }
    };
    expect("не было вовсе", refresh_due(None, now, None, today), true);
    expect("была сегодня", refresh_due(Some(earlier), now, Some(today), today), false);
    expect("была вчера", refresh_due(Some(earlier), now, Some(day(2026, 11, 2)), today), true);
    expect("день неизвестен", refresh_due(Some(earlier), now, None, today), true);
    expect(
        "момент из будущего",
        refresh_due(Some(at(now.unix_millis() + 1)), now, Some(today), today),
        true,
    );
    expect("в тот же миг", refresh_due(Some(now), now, Some(today), today), false);
    assert!(wrong.is_empty(), "{wrong:#?}");
}

#[test]
fn delay_waits_a_minute_and_keeps_the_gap() {
    let now = at(100 * 60_000);
    assert_eq!(delay(None, now), 60_000);
    // Последняя была давно — только минута.
    assert_eq!(delay(Some(at(0)), now), 60_000);
    // Три минуты назад — ещё семь до промежутка в десять.
    assert_eq!(delay(Some(at(97 * 60_000)), now), 7 * 60_000);
    // Только что — все десять.
    assert_eq!(delay(Some(now), now), 10 * 60_000);
    // Девять с половиной минут назад — до промежутка полминуты, но не
    // меньше минуты после изменения.
    assert_eq!(delay(Some(at(100 * 60_000 - 570_000)), now), 60_000);
    // Часы перевели назад — копия «из будущего» не откладывает новую.
    assert_eq!(delay(Some(at(200 * 60_000)), now), 60_000);
}

#[test]
fn password_check_is_due_after_thirty_days() {
    let today = day(2026, 11, 3);
    assert!(check_due(None, today), "неизвестно, когда вводили");
    assert!(!check_due(Some(today), today));
    assert!(!check_due(Some(day(2026, 10, 5)), today), "29 дней");
    assert!(check_due(Some(day(2026, 10, 4)), today), "30 дней");
    assert!(check_due(Some(day(2025, 11, 3)), today));
    assert!(!check_due(Some(day(2026, 12, 1)), today), "день из будущего — часы, а не ввод");
}

#[test]
fn device_tag_is_four_lowercase_hex_digits() {
    for _ in 0..64 {
        let random = DeviceTag::random();
        assert_eq!(DeviceTag::parse(random.as_str()), Some(random));
        assert_eq!(random.as_str().len(), 4);
    }
    assert_eq!(tag("09af").as_str(), "09af");
    let bad = ["", "1a2", "1a2b3", "1A2B", "zzzz", "1a-b", "١٢٣٤", "1a2Ա"];
    let accepted: Vec<&str> = bad.into_iter().filter(|text| DeviceTag::parse(text).is_some()).collect();
    assert!(accepted.is_empty(), "приняты: {accepted:?}");
}

/// Имя файла — с моментом его изменения, как их даёт папка.
fn files(list: &[(&str, i64)]) -> Vec<(String, i64)> {
    list.iter().map(|(name, at)| ((*name).to_owned(), *at)).collect()
}

#[test]
fn any_device_names_are_read() {
    assert_eq!(
        named("kvartik-auto-2026-11-03-1a2b.json"),
        Some(Named { day: day(2026, 11, 3), tag: tag("1a2b"), part: None })
    );
    assert_eq!(
        named("kvartik-auto-2026-11-03-00ff-2of3.json"),
        Some(Named { day: day(2026, 11, 3), tag: tag("00ff"), part: Some((2, 3)) })
    );
    for foreign in [
        "kvartik-2026-11-03.json",
        "kvartik-auto-2026-11-03-1a2b.json.crswap",
        "kvartik-auto-2026-11-03-1A2B.json",
        "kvartik-auto-2026-11-03-1a2b-0of3.json",
        "kvartik-auto-2026-11-03-1a2b-4of3.json",
        "kvartik-auto-2026-11-03-1a2b-1of3 (1).json",
        "kvartik-auto-2026-02-30-1a2b.json",
        "kvartik-auto-2026-11-03-1a2.json",
    ] {
        assert_eq!(named(foreign), None, "{foreign}");
    }
}

/// Имена файлов каждой найденной копии — для сравнения.
fn names<'a>(found: &[Found<'a>]) -> Vec<Vec<&'a str>> {
    found.iter().map(|copy| copy.names.clone()).collect()
}

const DAY_MILLIS: i64 = 24 * 60 * 60 * 1000;

/// «Сейчас» проб — позже всех моментов файлов, кроме нарочно «будущих».
const NOW: i64 = 200 * DAY_MILLIS;

#[test]
fn the_newest_copy_comes_whole_and_in_order() {
    // Части — не по порядку папки, а по номерам.
    let folder = files(&[
        ("kvartik-auto-2026-11-03-1a2b-3of3.json", 30),
        ("kvartik-auto-2026-11-03-1a2b-1of3.json", 31),
        ("kvartik-auto-2026-11-02-1a2b.json", 10),
        ("kvartik-auto-2026-11-03-1a2b-2of3.json", 32),
        ("notes.txt", 99),
    ]);
    let found = newest(&folder, NOW);
    assert_eq!(
        names(&found),
        [[
            "kvartik-auto-2026-11-03-1a2b-1of3.json",
            "kvartik-auto-2026-11-03-1a2b-2of3.json",
            "kvartik-auto-2026-11-03-1a2b-3of3.json",
        ]]
    );
    assert_eq!((found[0].tag, found[0].day), (tag("1a2b"), day(2026, 11, 3)));
    // Одним файлом.
    let folder = files(&[("kvartik-auto-2026-11-02-1a2b.json", 10), ("kvartik-auto-2026-11-01-1a2b.json", 5)]);
    assert_eq!(names(&newest(&folder, NOW)), [["kvartik-auto-2026-11-02-1a2b.json"]]);
    assert!(newest(&files(&[("kvartik-2026-11-03.json", 1)]), NOW).is_empty());
    assert!(newest(&[], NOW).is_empty());
}

#[test]
fn every_device_brings_its_newest_copy() {
    // Два компьютера в одной папке облака (решение человека 2026-11-25): у
    // каждого — своя самая свежая копия, а свежей первой идёт та, что
    // записана позже, хотя день в её имени раньше — часы ушли назад.
    let folder = files(&[
        ("kvartik-auto-2026-11-01-1a2b.json", 50),
        ("kvartik-auto-2026-11-03-1a2b.json", 100),
        ("kvartik-auto-2026-11-02-00ff.json", 200),
        ("kvartik-auto-2026-11-01-00ff.json", 150),
    ]);
    let found = newest(&folder, NOW);
    assert_eq!(
        names(&found),
        [["kvartik-auto-2026-11-02-00ff.json"], ["kvartik-auto-2026-11-03-1a2b.json"]]
    );
    assert_eq!(found.iter().map(|copy| copy.tag).collect::<Vec<_>>(), [tag("00ff"), tag("1a2b")]);
}

#[test]
fn an_older_copy_of_the_same_day_stays_out() {
    // Утром копия была из трёх частей, к вечеру — из двух, а утренние
    // не убраны: берётся вечерняя, и только её части. У второго устройства
    // в папке пока одна часть из двух — она и идёт: каждая часть — копия
    // сама по себе, а какой нет, окно скажет.
    let folder = files(&[
        ("kvartik-auto-2026-11-03-1a2b-1of3.json", 10),
        ("kvartik-auto-2026-11-03-1a2b-2of3.json", 11),
        ("kvartik-auto-2026-11-03-1a2b-3of3.json", 12),
        ("kvartik-auto-2026-11-03-1a2b-1of2.json", 50),
        ("kvartik-auto-2026-11-03-1a2b-2of2.json", 51),
        ("kvartik-auto-2026-11-03-00ff-1of2.json", 20),
    ]);
    let found = newest(&folder, NOW);
    assert_eq!(
        names(&found),
        [
            vec!["kvartik-auto-2026-11-03-1a2b-1of2.json", "kvartik-auto-2026-11-03-1a2b-2of2.json"],
            vec!["kvartik-auto-2026-11-03-00ff-1of2.json"],
        ]
    );
    assert_eq!((found[0].of, found[0].missing.clone()), (2, vec![]));
    assert_eq!((found[1].of, found[1].missing.clone()), (2, vec![2]));
}

#[test]
fn an_incomplete_copy_goes_after_the_whole_ones() {
    // Копия 00ff свежее, но облако скачало только её часть 2 из 3: она идёт
    // после целой копии 1a2b — слова о недостающих частях говорит последняя
    // часть цепочки (ревью задачи 124).
    let folder = files(&[
        ("kvartik-auto-2026-11-03-00ff-2of3.json", 90),
        ("kvartik-auto-2026-11-02-1a2b.json", 40),
    ]);
    let found = newest(&folder, NOW);
    assert_eq!(
        names(&found),
        [vec!["kvartik-auto-2026-11-02-1a2b.json"], vec!["kvartik-auto-2026-11-03-00ff-2of3.json"]]
    );
    assert_eq!((found[0].of, found[0].missing.clone()), (1, vec![]));
    assert_eq!((found[1].of, found[1].missing.clone()), (3, vec![1, 3]));
}

#[test]
fn clocks_from_the_future_do_not_cut_the_others() {
    // Часы одного компьютера ушли на сорок дней вперёд (Правило 6): его копия
    // идёт, но отсчёт тридцати дней — от самой свежей копии не позже суток
    // после «сейчас», и копии остальных на месте.
    let folder = files(&[
        ("kvartik-auto-2026-12-30-00ff.json", NOW + 40 * DAY_MILLIS),
        ("kvartik-auto-2026-11-20-1a2b.json", NOW - DAY_MILLIS),
        ("kvartik-auto-2026-11-11-c0de.json", NOW - 10 * DAY_MILLIS),
    ]);
    assert_eq!(
        names(&newest(&folder, NOW)),
        [
            ["kvartik-auto-2026-12-30-00ff.json"],
            ["kvartik-auto-2026-11-20-1a2b.json"],
            ["kvartik-auto-2026-11-11-c0de.json"],
        ]
    );
    // Все «из будущего» — отсчёт от самой свежей из них, как прежде.
    let folder = files(&[
        ("kvartik-auto-2026-12-30-00ff.json", NOW + 40 * DAY_MILLIS),
        ("kvartik-auto-2026-12-01-1a2b.json", NOW + 2 * DAY_MILLIS),
    ]);
    assert_eq!(names(&newest(&folder, NOW)).len(), 1);
}

#[test]
fn a_device_silent_for_a_month_stays_out() {
    // Копия, записанная на тридцать дней раньше самой свежей, ещё берётся,
    // а на день старше — нет: компьютер, скорее, брошен.
    let fresh = 100 * DAY_MILLIS;
    let folder = files(&[
        ("kvartik-auto-2026-11-25-1a2b.json", fresh),
        ("kvartik-auto-2026-10-26-00ff.json", fresh - 30 * DAY_MILLIS),
        ("kvartik-auto-2026-10-25-c0de.json", fresh - 30 * DAY_MILLIS - 1),
    ]);
    assert_eq!(
        names(&newest(&folder, NOW)),
        [["kvartik-auto-2026-11-25-1a2b.json"], ["kvartik-auto-2026-10-26-00ff.json"]]
    );
    // Своя прежняя копия того же устройства — не повод брать его старую.
    let folder = files(&[
        ("kvartik-auto-2026-11-25-1a2b.json", fresh),
        ("kvartik-auto-2026-09-01-1a2b.json", fresh - 80 * DAY_MILLIS),
    ]);
    assert_eq!(names(&newest(&folder, NOW)), [["kvartik-auto-2026-11-25-1a2b.json"]]);
}
