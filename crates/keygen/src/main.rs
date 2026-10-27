//! Выпуск ключей лицензии на машине автора.
//!
//! Закрытый ключ — вне репозитория и вне сборки, всегда («Общее»
//! в CLAUDE.md): утёкший ключ — бесплатные лицензии навсегда, а отозвать
//! его без сервера нельзя. Агент его не создаёт, не читает и не передаёт
//! в команды (Правило 9): программу запускает человек в своём терминале.
//!
//! Пару ключей человек создаёт один раз (задача 25): открытый встроен в ядро
//! (`kvartik_core::license::PUBLIC_KEY`), закрытый — 64 шестнадцатеричных
//! знака семени Ed25519 в файле вне рабочей копии. С 2026-10-27 он лежит
//! файлом [`SECRET`] в папке [`FOLDER`] домашнего каталога: там его
//! и ищет `issue`, если путь не назван. Ключ на ՀՎՀՀ (задача 26) — запись
//! ядра (`license::Key`), подписанная этим ключом, а каждый выпущенный
//! ложится строкой в журнал [`JOURNAL`] рядом с закрытым ключом.
//!
//! ```text
//! kvartik-keygen pair <путь закрытого ключа>
//! kvartik-keygen issue --tin 00054321 --plan standard --until 2027-10-10 [--key <путь закрытого ключа>]
//! ```

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use ed25519_dalek::{Signer, SigningKey};
use kvartik_core::license::{self, Key, PUBLIC_KEY_BYTES, Plan};
use kvartik_core::model::date::Date;
use kvartik_core::model::tin::Tin;

/// Папка закрытого ключа и журнала в домашнем каталоге автора. Любой путь
/// через неё hook агента не пропускает (`SECRET_DIRS` в
/// `.claude/hooks/guard.mjs`): переименовать — и там.
const FOLDER: &str = "kvartik-license";

/// Файл закрытого ключа в [`FOLDER`] — его создал `pair` 2026-10-27.
const SECRET: &str = "license.key";

/// Журнал выданных ключей — рядом с закрытым ключом: вне репозитория, как
/// и он, а без сервера другого списка выданного нет. Поля — через
/// табуляцию: в них её не бывает. Читать его текстовым редактором: Excel
/// срезал бы нули в начале ՀՎՀՀ.
const JOURNAL: &str = "issued.tsv";

/// Первая строка журнала.
const HEADER: &str = "выпущен\tՀՎՀՀ\tтариф\tпоследний день\tключ\n";

/// Флаги `issue` — в этом порядке их раскладывает [`Order::parse`].
const FLAGS: [&str; 4] = ["--tin", "--plan", "--until", "--key"];

/// Ереван — UTC+4 (шапка `model/date.rs` ядра), в секундах.
const YEREVAN: u64 = 4 * 3600;

/// Нулевой день времени Unix.
const EPOCH: Date = Date::new(1970, 1, 1).expect("1 января 1970 года в календаре есть");

const USAGE: &str = "использование:
  kvartik-keygen pair <путь закрытого ключа вне репозитория>
  kvartik-keygen issue --tin <ՀՎՀՀ> --plan standard|pro --until <ГГГГ-ММ-ДД> [--key <путь закрытого ключа>]
закрытый ключ issue по умолчанию — license.key в папке kvartik-license домашнего каталога";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.split_first() {
        Some((command, [path])) if command == "pair" => pair(Path::new(path)),
        Some((command, flags)) if command == "issue" => issue_command(flags),
        _ => Err(USAGE.to_owned()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

/// Новая пара: закрытый ключ — файлом по `path`, открытый — на экран.
///
/// Файл не перезаписывается никогда: прежний закрытый ключ, затёртый
/// новым, — это все выданные ключи, которые больше нечем продлить. И не
/// ложится в рабочую копию: `.gitignore` закрывает только шаблоны `*.key`,
/// `*.pem`, `keys/` и `kvartik-license/`, а файл с другим именем `git add .`
/// унёс бы в историю (Правило 11).
fn pair(path: &Path) -> Result<(), String> {
    let folder = path
        .parent()
        .filter(|folder| !folder.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let folder = fs::canonicalize(folder)
        .map_err(|error| format!("папка {}: {error} (создайте её сами)", folder.display()))?;
    if inside(&folder, &workspace()?) {
        return Err(format!(
            "{} — внутри рабочей копии: закрытый ключ хранится вне репозитория",
            path.display()
        ));
    }

    let mut seed = [0_u8; ed25519_dalek::SECRET_KEY_LENGTH];
    getrandom::fill(&mut seed).map_err(|error| format!("случайность системы: {error}"))?;
    let key = SigningKey::from_bytes(&seed);
    let text = format!("{}\n", hex(&seed));
    seed.fill(0);

    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("{}: {error} (существующий ключ не перезаписывается)", path.display()))?;
    file.write_all(text.as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|error| format!("{}: {error}", path.display()))?;

    println!("Закрытый ключ записан: {}", path.display());
    println!("Сохраните его копию вне этой машины: без него ключей не продлить.");
    println!("Открытый ключ — для сборки:");
    println!("{}", hex(key.verifying_key().as_bytes()));
    Ok(())
}

/// Заказ ключа — флаги `issue`.
#[derive(Debug, PartialEq, Eq)]
struct Order {
    tin: Tin,
    plan: Plan,
    /// Последний день срока, включительно.
    until: Date,
    /// Закрытый ключ; `None` — [`SECRET`] в [`FOLDER`] домашнего каталога.
    key: Option<PathBuf>,
}

impl Order {
    /// Флаги [`FLAGS`] в любом порядке, каждый со значением и не больше
    /// раза; `--key` можно не называть. Ошибка — словами, с тем, что ввели.
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut values: [Option<&str>; FLAGS.len()] = [None; FLAGS.len()];
        let mut args = args.iter();
        while let Some(flag) = args.next() {
            let Some(at) = FLAGS.iter().position(|known| known == flag) else {
                return Err(format!("{flag} — нет такого флага\n{USAGE}"));
            };
            let Some(value) = args.next() else {
                return Err(format!("{flag} — без значения\n{USAGE}"));
            };
            if values[at].replace(value).is_some() {
                return Err(format!("{flag} — дважды"));
            }
        }
        let [Some(tin), Some(plan), Some(until), key] = values else {
            return Err(format!("нужны все три: --tin, --plan и --until\n{USAGE}"));
        };
        let tin = Tin::parse(tin).map_err(|_| format!("--tin {tin}: ՀՎՀՀ — восемь цифр"))?;
        let plan = match plan {
            "standard" => Plan::Standard,
            "pro" => Plan::Pro,
            _ => return Err(format!("--plan {plan}: тариф — standard или pro")),
        };
        let until = Date::from_iso(until)
            .ok_or_else(|| format!("--until {until}: последний день срока — ГГГГ-ММ-ДД, например 2027-10-10"))?;
        Ok(Self {
            tin,
            plan,
            until,
            key: key.map(PathBuf::from),
        })
    }
}

/// `issue`: ключ — на экран, когда он уже в журнале.
fn issue_command(flags: &[String]) -> Result<(), String> {
    let order = Order::parse(flags)?;
    let secret = match &order.key {
        Some(path) => path.clone(),
        None => std::env::home_dir()
            .map(|home| home.join(FOLDER).join(SECRET))
            .filter(|path| path.exists())
            .ok_or("закрытого ключа нет в папке kvartik-license домашнего каталога: назовите его флагом --key")?,
    };
    let key = issue(&order, &secret, &license::PUBLIC_KEY, today(SystemTime::now())?)?;
    println!(
        "Ключ на ՀՎՀՀ {}, тариф {}, выпущен {}, последний день — {} ({} дн.):",
        key.tin().as_str(),
        plan_word(key.plan()),
        key.issued().iso(),
        key.until().iso(),
        key.until().days_since(key.issued()) + 1,
    );
    println!();
    println!("{}", key.text());
    println!();
    println!("Записан в журнал {}.", secret.with_file_name(JOURNAL).display());
    Ok(())
}

/// Ключ по заказу `order` с днём выпуска `today`, подписанный закрытым
/// ключом из файла `secret` и проверенный открытым `public`, — уже строкой
/// в журнале рядом с `secret`. Ключ, которого нет в журнале, к человеку не
/// уходит: журнал — единственный список выданного.
fn issue(order: &Order, secret: &Path, public: &[u8; PUBLIC_KEY_BYTES], today: Date) -> Result<Key, String> {
    let key = sign(&signing_key(secret)?, public, order, today)?;
    record(&secret.with_file_name(JOURNAL), &key)?;
    Ok(key)
}

/// Закрытый ключ из файла `path` — такого, какой пишет `pair`. Файл внутри
/// рабочей копии не читается: ключ там — уже беда (`git add .` унёс бы его
/// в историю), и программа говорит это, а не работает с ним молча.
fn signing_key(path: &Path) -> Result<SigningKey, String> {
    let file = fs::canonicalize(path).map_err(|error| format!("закрытый ключ {}: {error}", path.display()))?;
    if inside(&file, &workspace()?) {
        return Err(format!(
            "{} — внутри рабочей копии: перенесите закрытый ключ вне репозитория",
            path.display()
        ));
    }
    let mut text = fs::read(&file).map_err(|error| format!("закрытый ключ {}: {error}", path.display()))?;
    let seed = seed(&text);
    text.fill(0);
    let mut seed = seed.ok_or_else(|| {
        format!(
            "{}: не закрытый ключ — в файле ждали 64 шестнадцатеричных знака, как пишет pair",
            path.display()
        )
    })?;
    let key = SigningKey::from_bytes(&seed);
    seed.fill(0);
    Ok(key)
}

/// Семя из текста файла: 64 шестнадцатеричных знака, пробелы и переводы
/// строки по краям не важны. `None` — не так.
fn seed(text: &[u8]) -> Option<[u8; ed25519_dalek::SECRET_KEY_LENGTH]> {
    let text = text.trim_ascii();
    let mut seed = [0_u8; ed25519_dalek::SECRET_KEY_LENGTH];
    if text.len() != seed.len() * 2 {
        return None;
    }
    let digit = |letter: u8| char::from(letter).to_digit(16);
    for (byte, &[high, low]) in seed.iter_mut().zip(text.as_chunks::<2>().0) {
        *byte = u8::try_from(digit(high)? << 4 | digit(low)?).ok()?;
    }
    Some(seed)
}

/// Ключ по заказу `order` с днём выпуска `issued`, подписанный `signing`.
///
/// Закрытый ключ не от пары `public` — отказ: подписанный им ключ окно
/// назвало бы подделкой. Подписанный проверяется так же, как его проверит
/// окно (`Key::read` открытым ключом сборки), и годится, только если
/// прочитался тем же ключом.
fn sign(signing: &SigningKey, public: &[u8; PUBLIC_KEY_BYTES], order: &Order, issued: Date) -> Result<Key, String> {
    let own = signing.verifying_key().to_bytes();
    if own != *public {
        return Err(format!(
            "закрытый ключ не от этой сборки: его открытый — {}, а в программе — {}",
            hex(&own),
            hex(public)
        ));
    }
    let message = Key::message(&order.tin, order.plan, issued, order.until);
    let signature = signing.sign(&message).to_bytes();
    let key = Key::signed(order.tin.clone(), order.plan, issued, order.until, signature).map_err(|_| {
        format!(
            "последний день срока {} раньше дня выпуска {}",
            order.until.iso(),
            issued.iso()
        )
    })?;
    match Key::read(&key.text(), public) {
        Ok(read) if read == key => Ok(key),
        _ => Err("выпущенный ключ не прошёл проверку окна — не отправляйте его".to_owned()),
    }
}

/// Строка ключа `key` — в конец журнала `path`; журнал, которого ещё нет,
/// начинается заголовками. Успех — только когда строка легла на диск.
fn record(path: &Path, key: &Key) -> Result<(), String> {
    let failed = |error: std::io::Error| format!("журнал {}: {error} — ключ не выпущен", path.display());
    let mut file = OpenOptions::new().create(true).append(true).open(path).map_err(failed)?;
    let mut text = String::new();
    if file.metadata().map_err(failed)?.len() == 0 {
        text.push_str(HEADER);
    }
    text.push_str(&line(key));
    file.write_all(text.as_bytes()).and_then(|()| file.sync_all()).map_err(failed)
}

/// Строка журнала: день выпуска, ՀՎՀՀ, тариф, последний день и ключ.
fn line(key: &Key) -> String {
    format!(
        "{}\t{}\t{}\t{}\t{}\n",
        key.issued().iso(),
        key.tin().as_str(),
        plan_word(key.plan()),
        key.until().iso(),
        key.text()
    )
}

/// Тариф — словом флага `--plan`.
const fn plan_word(plan: Plan) -> &'static str {
    match plan {
        Plan::Standard => "standard",
        Plan::Pro => "pro",
    }
}

/// День выпуска — сегодня в Ереване: по UTC ключ, выпущенный до четырёх
/// утра, был бы вчерашним (шапка `model/date.rs` ядра).
fn today(now: SystemTime) -> Result<Date, String> {
    let wrong = || "часы машины: день выпуска не узнать".to_owned();
    let seconds = now.duration_since(UNIX_EPOCH).map_err(|_| wrong())?.as_secs();
    let days = i32::try_from((seconds + YEREVAN) / 86_400).map_err(|_| wrong())?;
    EPOCH.add_days(days).ok_or_else(wrong)
}

/// Корень рабочей копии — на два уровня выше этого крейта.
fn workspace() -> Result<PathBuf, String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
    fs::canonicalize(&root).map_err(|error| format!("рабочая копия {}: {error}", root.display()))
}

/// Лежит ли `path` внутри `root`. Без учёта регистра: на Windows
/// `C:\Dev` и `c:\dev` — одна папка.
fn inside(path: &Path, root: &Path) -> bool {
    let lower = |path: &Path| path.to_string_lossy().to_lowercase();
    let (path, root) = (lower(path), lower(root));
    path == root || path.strip_prefix(&root).is_some_and(|rest| rest.starts_with(['\\', '/']))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    /// Закрытый ключ проб — тот же, что у тестов ядра (`license/tests.rs`).
    /// Не авторский: им подписаны только ключи тестов.
    const SEED: [u8; 32] = [7; 32];

    fn signer() -> SigningKey {
        SigningKey::from_bytes(&SEED)
    }

    fn public() -> [u8; PUBLIC_KEY_BYTES] {
        signer().verifying_key().to_bytes()
    }

    fn day(text: &str) -> Date {
        Date::from_iso(text).expect("в тесте — существующая дата")
    }

    fn args(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_owned).collect()
    }

    fn order(until: &str) -> Order {
        Order::parse(&args(&format!("--tin 00054321 --plan standard --until {until}"))).expect("годный заказ")
    }

    /// Папка рабочей копии и вложенные в неё — внутри, соседняя с похожим
    /// именем — нет.
    #[test]
    fn inside_knows_the_working_copy() {
        let root = Path::new(r"C:\dev\kvartik");
        assert!(inside(Path::new(r"C:\dev\kvartik"), root));
        assert!(inside(Path::new(r"c:\DEV\kvartik\crates"), root));
        assert!(!inside(Path::new(r"C:\dev\kvartik-keys"), root));
        assert!(!inside(Path::new(r"C:\Users\author\kvartik"), root));
    }

    /// Флаги — в любом порядке, `--key` можно не называть; неизвестный,
    /// повторённый, без значения, без нужного и с негодным значением —
    /// отказ.
    #[test]
    fn order_reads_flags() {
        let full = Order::parse(&args("--until 2027-10-10 --key D:/flash/seed.txt --plan pro --tin 00054321"));
        let tin = Tin::parse("00054321").expect("годный ՀՎՀՀ");
        assert_eq!(
            full,
            Ok(Order {
                tin: tin.clone(),
                plan: Plan::Pro,
                until: day("2027-10-10"),
                key: Some(PathBuf::from("D:/flash/seed.txt")),
            })
        );
        assert_eq!(
            Order::parse(&args("--tin 00054321 --plan standard --until 2027-10-10")),
            Ok(Order {
                tin,
                plan: Plan::Standard,
                until: day("2027-10-10"),
                key: None,
            })
        );
        let mut accepted = Vec::new();
        for line in [
            "--tin 00054321 --plan standard",
            "--tin 00054321 --plan standard --until 2027-10-10 --tin 00054321",
            "--tin 00054321 --plan standard --until 2027-10-10 --days 365",
            "--tin 00054321 --plan standard --until",
            "--tin 0005432 --plan standard --until 2027-10-10",
            "--tin 00054321 --plan gold --until 2027-10-10",
            "--tin 00054321 --plan standard --until 2027-02-30",
            "--tin 00054321 --plan standard --until 10.10.2027",
        ] {
            if Order::parse(&args(line)).is_ok() {
                accepted.push(line);
            }
        }
        assert!(accepted.is_empty(), "приняты негодные флаги: {accepted:#?}");
    }

    /// Ключ, подписанный своей парой, окно читает тем же открытым ключом,
    /// с теми же полями; закрытый ключ другой пары — отказ: такой ключ окно
    /// назвало бы подделкой. Ключ проб — не пара сборки (тест ядра
    /// `the_author_key_is_a_real_point`).
    #[test]
    fn signs_only_with_the_pair_of_the_build() {
        let key = sign(&signer(), &public(), &order("2027-10-10"), day("2026-10-27")).expect("своя пара");
        assert_eq!(Key::read(&key.text(), &public()), Ok(key.clone()));
        assert_eq!(
            (key.tin().as_str(), key.plan(), key.issued(), key.until()),
            ("00054321", Plan::Standard, day("2026-10-27"), day("2027-10-10"))
        );
        let other = sign(&signer(), &license::PUBLIC_KEY, &order("2027-10-10"), day("2026-10-27"));
        assert!(
            other.as_ref().is_err_and(|error| error.contains("не от этой сборки")),
            "чужая пара: {other:?}"
        );
    }

    /// Срок — с дня выпуска: ключ на один день есть, а кончившийся раньше
    /// выпуска — нет.
    #[test]
    fn until_is_not_before_issue() {
        let issued = day("2026-10-27");
        assert!(sign(&signer(), &public(), &order("2026-10-27"), issued).is_ok());
        let late = sign(&signer(), &public(), &order("2026-10-26"), issued);
        assert!(late.as_ref().is_err_and(|error| error.contains("раньше дня выпуска")), "{late:?}");
    }

    /// Семя читается так, как его пишет `pair`: 64 знака и перевод строки —
    /// и с переводом строки Windows и большими буквами; не та длина и чужой
    /// знак — не ключ.
    #[test]
    fn seed_reads_what_pair_writes() {
        let text = format!("{}\n", hex(&SEED));
        assert_eq!(seed(text.as_bytes()), Some(SEED));
        assert_eq!(seed(format!("{}\r\n", hex(&SEED).to_uppercase()).as_bytes()), Some(SEED));
        let mut accepted = Vec::new();
        for bad in [text[..62].to_owned(), format!("{}0\n", hex(&SEED)), text.replace('7', "g"), String::new()] {
            if seed(bad.as_bytes()).is_some() {
                accepted.push(bad);
            }
        }
        assert!(accepted.is_empty(), "приняты не ключи: {accepted:?}");
    }

    /// День выпуска — по Еревану, UTC+4: полночь 27 октября 2026 года
    /// в Ереване — 20:00 26-го по UTC, 1 793 044 800 с от 1970 года (число
    /// посчитано вне программы).
    #[test]
    fn today_is_the_day_in_yerevan() {
        let at = |seconds: u64| today(UNIX_EPOCH + Duration::from_secs(seconds));
        assert_eq!(at(1_793_044_800), Ok(day("2026-10-27")));
        assert_eq!(at(1_793_044_799), Ok(day("2026-10-26")));
        assert_eq!(at(0), Ok(day("1970-01-01")));
    }

    /// Закрытый ключ внутри рабочей копии не читается — и отказ раньше
    /// разбора файла: `Cargo.toml` — не ключ, но слова — о месте.
    #[test]
    fn no_secret_inside_the_working_copy() {
        let inner = workspace().expect("рабочая копия").join("Cargo.toml");
        let refused = signing_key(&inner).map(|_| ());
        assert!(
            refused.as_ref().is_err_and(|error| error.contains("внутри рабочей копии")),
            "{refused:?}"
        );
    }

    /// Выпуск — по файлу, который пишет `pair`: журнал рядом с ним
    /// начинается заголовками, каждый ключ — строкой в конец, а ключ чужой
    /// пары в журнал не ложится.
    #[test]
    fn issue_keeps_the_journal_next_to_the_secret() {
        let folder = std::env::temp_dir().join(format!("kvartik-keygen-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(&folder).expect("временная папка");
        let secret = folder.join("seed.txt");
        fs::write(&secret, format!("{}\n", hex(&SEED))).expect("файл ключа проб");
        let (standard, issued) = (order("2027-10-10"), day("2026-10-27"));
        let pro = Order {
            plan: Plan::Pro,
            ..order("2027-12-31")
        };
        let first = issue(&standard, &secret, &public(), issued);
        let second = issue(&pro, &secret, &public(), issued);
        let other = issue(&standard, &secret, &license::PUBLIC_KEY, issued);
        let journal = fs::read_to_string(folder.join(JOURNAL));
        // Убрать до проверок: упавшая проверка оставила бы папку.
        let _ = fs::remove_dir_all(&folder);

        let (first, second) = (first.expect("своя пара"), second.expect("своя пара"));
        assert!(other.is_err(), "ключ чужой пары: {other:?}");
        assert_eq!(
            journal.expect("журнал"),
            format!(
                "выпущен\tՀՎՀՀ\tтариф\tпоследний день\tключ\n\
                 2026-10-27\t00054321\tstandard\t2027-10-10\t{}\n\
                 2026-10-27\t00054321\tpro\t2027-12-31\t{}\n",
                first.text(),
                second.text()
            )
        );
    }
}
