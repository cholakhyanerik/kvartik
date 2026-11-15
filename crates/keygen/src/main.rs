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
//! Продление заранее (условия, п. 4.1, редакция от 13.11.2026; задача 97) —
//! [`RENEW`] вместо `--until`: последний день нового ключа программа берёт из
//! того же журнала сама ([`renewal`]). Глазами по журналу ошибка на день —
//! подарок покупателю или его потерянный день.
//!
//! ```text
//! kvartik-keygen pair <путь закрытого ключа>
//! kvartik-keygen issue --tin 00054321 --plan standard --until 2027-10-10 [--key <путь закрытого ключа>]
//! kvartik-keygen issue --tin 00054321 --plan standard --renew [--key <путь закрытого ключа>]
//! ```

use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
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

/// Флаги `issue` со значением — в этом порядке их раскладывает
/// [`Order::parse`].
const FLAGS: [&str; 4] = ["--tin", "--plan", "--until", "--key"];

/// Флаг `issue` без значения — продление по журналу вместо `--until`.
const RENEW: &str = "--renew";

/// Срок тарифа, месяцев, — год: на странице «Купить» продаётся только
/// «Стандарт» на год (`release::PRICE`, решение человека 2026-11-09).
/// Появится тариф с другим сроком (задача 90) — у продления срок его тарифа.
const TERM_MONTHS: u32 = 12;

/// Ереван — UTC+4 (шапка `model/date.rs` ядра), в секундах.
const YEREVAN: u64 = 4 * 3600;

/// Нулевой день времени Unix.
const EPOCH: Date = Date::new(1970, 1, 1).expect("1 января 1970 года в календаре есть");

const USAGE: &str = "использование:
  kvartik-keygen pair <путь закрытого ключа вне репозитория>
  kvartik-keygen issue --tin <ՀՎՀՀ> --plan standard|pro --until <ГГГГ-ММ-ДД> [--key <путь закрытого ключа>]
  kvartik-keygen issue --tin <ՀՎՀՀ> --plan standard|pro --renew [--key <путь закрытого ключа>]
--renew — продление заранее (условия, п. 4.1): последний день — год после последнего дня самого долгого ключа на этот ՀՎՀՀ в журнале, пока тот ещё идёт
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
    until: Until,
    /// Закрытый ключ; `None` — [`SECRET`] в [`FOLDER`] домашнего каталога.
    key: Option<PathBuf>,
}

/// Последний день срока, включительно.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Until {
    /// Назван флагом `--until`.
    Day(Date),
    /// [`RENEW`]: у продления самого долгого ключа на этот ՀՎՀՀ из журнала
    /// ([`renewal`]).
    Renew,
}

impl Order {
    /// Флаги [`FLAGS`] в любом порядке, каждый со значением и не больше
    /// раза, и [`RENEW`] без значения вместо `--until`; `--key` можно не
    /// называть. Ошибка — словами, с тем, что ввели.
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut values: [Option<&str>; FLAGS.len()] = [None; FLAGS.len()];
        let mut renew = false;
        let mut args = args.iter();
        while let Some(flag) = args.next() {
            if flag == RENEW {
                if std::mem::replace(&mut renew, true) {
                    return Err(format!("{flag} — дважды"));
                }
                continue;
            }
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
        let missing = || format!("нужны --tin, --plan и --until или {RENEW}\n{USAGE}");
        let [Some(tin), Some(plan), until, key] = values else {
            return Err(missing());
        };
        let tin = Tin::parse(tin).map_err(|_| format!("--tin {tin}: ՀՎՀՀ — восемь цифр"))?;
        let plan = match plan {
            "standard" => Plan::Standard,
            "pro" => Plan::Pro,
            _ => return Err(format!("--plan {plan}: тариф — standard или pro")),
        };
        let until = match (until, renew) {
            (Some(until), false) => Until::Day(Date::from_iso(until).ok_or_else(|| {
                format!("--until {until}: последний день срока — ГГГГ-ММ-ДД, например 2027-10-10")
            })?),
            (None, true) => Until::Renew,
            (Some(_), true) => {
                return Err(format!("--until и {RENEW} — что-то одно: продление считает последний день само"));
            }
            (None, false) => return Err(missing()),
        };
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
    let Issued { key, renewed } = issue(&order, &secret, &license::PUBLIC_KEY, today(SystemTime::now())?)?;
    if let Some(renewed) = renewed {
        // Дни самого продления, а не ключа от дня выдачи: ниже ключ считается
        // со дня выпуска, и при продлении заранее их больше года.
        println!(
            "Продлён ключ на ՀՎՀՀ {}, тариф {}, выпущенный {}, с последним днём {}: новый считается со дня после него, по {} — {} дн. (условия, п. 4.1).",
            renewed.tin().as_str(),
            plan_word(renewed.plan()),
            renewed.issued().iso(),
            renewed.until().iso(),
            key.until().iso(),
            key.until().days_since(renewed.until()),
        );
        println!();
    }
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

/// Выпущенный ключ и, у продления, тот, который он продлил.
#[derive(Debug)]
struct Issued {
    key: Key,
    renewed: Option<Key>,
}

/// Ключ по заказу `order` с днём выпуска `today`, подписанный закрытым
/// ключом из файла `secret` и проверенный открытым `public`, — уже строкой
/// в журнале рядом с `secret`. Ключ, которого нет в журнале, к человеку не
/// уходит: журнал — единственный список выданного. У продления последний
/// день — по тому же журналу ([`renewal`]).
fn issue(order: &Order, secret: &Path, public: &[u8; PUBLIC_KEY_BYTES], today: Date) -> Result<Issued, String> {
    let signing = signing_key(secret)?;
    let journal = secret.with_file_name(JOURNAL);
    let (until, renewed) = match order.until {
        Until::Day(until) => (until, None),
        Until::Renew => {
            let (renewed, until) = renewal(&journal, &order.tin, public, today)?;
            (until, Some(renewed))
        }
    };
    let key = sign(&signing, public, &order.tin, order.plan, today, until)?;
    record(&journal, &key)?;
    Ok(Issued { key, renewed })
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

/// Ключ на `tin` с тарифом `plan`, днём выпуска `issued` и последним днём
/// `until`, подписанный `signing`.
///
/// Закрытый ключ не от пары `public` — отказ: подписанный им ключ окно
/// назвало бы подделкой. Подписанный проверяется так же, как его проверит
/// окно (`Key::read` открытым ключом сборки), и годится, только если
/// прочитался тем же ключом.
fn sign(
    signing: &SigningKey,
    public: &[u8; PUBLIC_KEY_BYTES],
    tin: &Tin,
    plan: Plan,
    issued: Date,
    until: Date,
) -> Result<Key, String> {
    let own = signing.verifying_key().to_bytes();
    if own != *public {
        return Err(format!(
            "закрытый ключ не от этой сборки: его открытый — {}, а в программе — {}",
            hex(&own),
            hex(public)
        ));
    }
    let message = Key::message(tin, plan, issued, until);
    let signature = signing.sign(&message).to_bytes();
    let key = Key::signed(tin.clone(), plan, issued, until, signature)
        .map_err(|_| format!("последний день срока {} раньше дня выпуска {}", until.iso(), issued.iso()))?;
    match Key::read(&key.text(), public) {
        Ok(read) if read == key => Ok(key),
        _ => Err("выпущенный ключ не прошёл проверку окна — не отправляйте его".to_owned()),
    }
}

/// Продление (`--renew`; условия, п. 4.1): самый долгий ключ на `tin`
/// в журнале `path` и последний день ключа, который его продлевает, —
/// [`year_after`] его последнего дня. Продлевается только идущий ключ: срок
/// ключа, оплаченного после конца действующего, — со дня выдачи, и его
/// называет `--until`. Поэтому кончившийся раньше `today` ключ — отказ,
/// как и ՀՎՀՀ без ключей, и журнал при отказе не меняется.
///
/// Журнала нет — отказ своими словами, а не «ключей нет»: рядом с копией
/// закрытого ключа, которую `pair` велит хранить вне машины, журнала
/// и не бывает, а «со дня выдачи» отняло бы у покупателя остаток
/// действующего ключа (ревью задачи 97).
fn renewal(
    path: &Path,
    tin: &Tin,
    public: &[u8; PUBLIC_KEY_BYTES],
    today: Date,
) -> Result<(Key, Date), String> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Err(format!(
                "журнала {} нет — тот ли закрытый ключ назван? --renew считает по журналу рядом с ключом, \
                 которым выпускали ключи; если их ещё не выпускали, новый — со дня выдачи: назовите --until",
                path.display()
            ));
        }
        Err(error) => return Err(format!("журнал {}: {error}", path.display())),
    };
    let keys = journal(&text, public)
        .map_err(|problem| format!("журнал {}: {problem} — поправьте её или назовите --until", path.display()))?;
    let Some(last) = keys.into_iter().filter(|key| key.tin() == tin).max_by_key(Key::until) else {
        return Err(format!(
            "в журнале {} ключей на ՀՎՀՀ {} нет: срок нового ключа — со дня выдачи, назовите --until",
            path.display(),
            tin.as_str()
        ));
    };
    let until = year_after(last.until())
        .ok_or_else(|| format!("последний день {} — у края календаря", last.until().iso()))?;
    if last.until() < today {
        return Err(format!(
            "ключ на ՀՎՀՀ {} кончился {}: продлевается только идущий. Если новый оплачен до конца того дня, \
             срок — со дня после него: --until {} (условия, п. 4.1), иначе — со дня выдачи: назовите --until",
            tin.as_str(),
            last.until().iso(),
            until.iso()
        ));
    }
    Ok((last, until))
}

/// Ключи журнала `text` — строк, которые пишет [`record`], — подписанные
/// парой `public`. Строка не такая, какой её пишет выпуск, — правленная
/// рукой, пересохранённая Excel без нулей ՀՎՀՀ, ключ другой пары, — отказ
/// с её номером, а не пропуск: пропущенной могла быть строка самого долгого
/// ключа, и продление вышло бы короче оплаченного. Пустые строки и пробелы
/// в конце строк ключа не спрячут, а BOM и переводы строки Windows оставил
/// бы Блокнот, — их журнал прощает.
fn journal(text: &str, public: &[u8; PUBLIC_KEY_BYTES]) -> Result<Vec<Key>, String> {
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    let mut rows = text.lines().enumerate();
    if let Some((_, first)) = rows.next()
        && first.trim_end() != HEADER.trim_end()
    {
        return Err("первая строка — не заголовки журнала".to_owned());
    }
    let mut keys = Vec::new();
    for (at, row) in rows {
        let row = row.trim_end();
        if row.is_empty() {
            continue;
        }
        let key = row
            .rsplit('\t')
            .next()
            .and_then(|text| Key::read(text, public).ok())
            .filter(|key| line(key).trim_end() == row)
            .ok_or_else(|| format!("строка {} — не та, что пишет выпуск", at + 1))?;
        keys.push(key);
    }
    Ok(keys)
}

/// Последний день ключа, продлевающего ключ с последним днём `last`: `last`
/// плюс срок тарифа — «последний день действующего плюс срок тарифа»
/// (условия, п. 4.1, и «Для проверки» там же), а конец февраля
/// `Date::add_months` держит концом месяца: 2027-11-08 — 2028-11-08,
/// 2027-02-28 — 2028-02-29. В году продления так 365 дней, а 366 — ровно
/// когда в нём есть 29 февраля.
///
/// Счёт «год со дня после и минус день» расходится с этим в двух днях на
/// четыре года, и выбран этот (задача 97): 28 февраля високосного года тот
/// счёт продлил бы до 27 февраля — тем же днём, что ключ до 27-го, и
/// покупатель потерял бы день, — а 27 февраля года перед високосным — до
/// 28 февраля, на 366 дней без 29-го: день в подарок. Выдаче без рук
/// (задача 90) — этот же счёт. `None` — за краем календаря.
const fn year_after(last: Date) -> Option<Date> {
    last.add_months(TERM_MONTHS)
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

    fn tin(text: &str) -> Tin {
        Tin::parse(text).expect("в тесте — годный ՀՎՀՀ")
    }

    fn args(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_owned).collect()
    }

    fn order(until: &str) -> Order {
        Order::parse(&args(&format!("--tin 00054321 --plan standard --until {until}"))).expect("годный заказ")
    }

    /// Продление ключа на `tin` — флагом, как его закажет человек.
    fn renew(tin: &str) -> Order {
        Order::parse(&args(&format!("--tin {tin} --plan standard --renew"))).expect("годный заказ")
    }

    /// Своя временная папка теста `name` — тесты идут потоками одного
    /// процесса — и в ней файл ключа проб, такой, какой пишет `pair`.
    fn folder(name: &str) -> (PathBuf, PathBuf) {
        let folder = std::env::temp_dir().join(format!("kvartik-keygen-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(&folder).expect("временная папка");
        let secret = folder.join("seed.txt");
        fs::write(&secret, format!("{}\n", hex(&SEED))).expect("файл ключа проб");
        (folder, secret)
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

    /// Флаги — в любом порядке, `--key` можно не называть, а `--renew` —
    /// без значения и вместо `--until`; неизвестный, повторённый, без
    /// значения, без нужного, оба срока сразу и негодное значение — отказ.
    #[test]
    fn order_reads_flags() {
        let full = Order::parse(&args("--until 2027-10-10 --key D:/flash/seed.txt --plan pro --tin 00054321"));
        assert_eq!(
            full,
            Ok(Order {
                tin: tin("00054321"),
                plan: Plan::Pro,
                until: Until::Day(day("2027-10-10")),
                key: Some(PathBuf::from("D:/flash/seed.txt")),
            })
        );
        assert_eq!(
            Order::parse(&args("--tin 00054321 --plan standard --until 2027-10-10")),
            Ok(Order {
                tin: tin("00054321"),
                plan: Plan::Standard,
                until: Until::Day(day("2027-10-10")),
                key: None,
            })
        );
        assert_eq!(
            Order::parse(&args("--tin 00054321 --renew --key D:/flash/seed.txt --plan standard")),
            Ok(Order {
                tin: tin("00054321"),
                plan: Plan::Standard,
                until: Until::Renew,
                key: Some(PathBuf::from("D:/flash/seed.txt")),
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
            "--tin 00054321 --plan standard --until 2027-10-10 --renew",
            "--renew --tin 00054321 --plan standard --renew",
            "--tin 00054321 --plan standard --renew 2027-10-10",
            "--tin 00054321 --renew",
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
        let (owner, issued, until) = (tin("00054321"), day("2026-10-27"), day("2027-10-10"));
        let key = sign(&signer(), &public(), &owner, Plan::Standard, issued, until).expect("своя пара");
        assert_eq!(Key::read(&key.text(), &public()), Ok(key.clone()));
        assert_eq!(
            (key.tin().as_str(), key.plan(), key.issued(), key.until()),
            ("00054321", Plan::Standard, issued, until)
        );
        let other = sign(&signer(), &license::PUBLIC_KEY, &owner, Plan::Standard, issued, until);
        assert!(
            other.as_ref().is_err_and(|error| error.contains("не от этой сборки")),
            "чужая пара: {other:?}"
        );
    }

    /// Срок — с дня выпуска: ключ на один день есть, а кончившийся раньше
    /// выпуска — нет.
    #[test]
    fn until_is_not_before_issue() {
        let (owner, issued) = (tin("00054321"), day("2026-10-27"));
        assert!(sign(&signer(), &public(), &owner, Plan::Standard, issued, issued).is_ok());
        let late = sign(&signer(), &public(), &owner, Plan::Standard, issued, day("2026-10-26"));
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
        let (folder, secret) = folder("journal");
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

        let (first, second) = (first.expect("своя пара").key, second.expect("своя пара").key);
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

    /// Продление — последний день плюс год: в году продления 365 дней, а 366
    /// — ровно когда в нём есть 29 февраля, и ключ, кончившийся позже, не
    /// продлевается раньше. Счёт «год со дня после и минус день» здесь
    /// краснеет: продление ключа до 2028-02-28 кончилось бы 2029-02-27 — 365
    /// дней вместе с 29 февраля (выбор задачи 97).
    #[test]
    fn renewal_is_a_year_after_the_last_day() {
        let mut wrong = Vec::new();
        for (last, expected) in [
            ("2027-11-08", "2028-11-08"),
            ("2027-02-28", "2028-02-29"),
            ("2028-02-27", "2029-02-27"),
            ("2028-02-28", "2029-02-28"),
            ("2028-02-29", "2029-02-28"),
        ] {
            let got = year_after(day(last));
            if got != Some(day(expected)) {
                wrong.push(format!("{last}: {got:?}"));
            }
        }
        let (mut last, mut before) = (day("2026-01-01"), None);
        while last <= day("2033-12-31") {
            let start = last.add_days(1).expect("в календаре");
            let end = year_after(last).expect("в календаре");
            let leap = [start.year(), end.year()]
                .into_iter()
                .filter_map(|year| Date::new(year, 2, 29))
                .any(|february| start <= february && february <= end);
            let length = end.days_since(start) + 1;
            if length != if leap { 366 } else { 365 } || before.is_some_and(|before| end < before) {
                wrong.push(format!("{}: {} — {length} дн.", last.iso(), end.iso()));
            }
            before = Some(end);
            last = start;
        }
        assert!(wrong.is_empty(), "продление: {wrong:#?}");
        assert_eq!(year_after(day("9999-06-30")), None);
    }

    /// Продлевается самый долгий ключ на этот ՀՎՀՀ, где бы в журнале он ни
    /// стоял — ключи короче него есть и до, и после, — а ключ другого ՀՎՀՀ,
    /// даже более долгий, не в счёт. Продление заранее считается от
    /// последнего дня ключа, а не от дня выпуска, а последний день — ещё его
    /// срок: в этот день продление есть. Новый ключ — строкой в тот же журнал.
    #[test]
    fn renew_extends_the_longest_key_on_the_tin() {
        let (folder, secret) = folder("renew");
        let other = |until: &str| Order {
            tin: tin("07654321"),
            ..order(until)
        };
        let issued: Vec<_> = [
            (order("2027-06-30"), "2026-07-01"),
            (order("2027-11-08"), "2026-11-09"),
            (order("2027-09-30"), "2026-11-10"),
            (other("2028-01-31"), "2026-11-10"),
        ]
        .into_iter()
        .map(|(order, on)| issue(&order, &secret, &public(), day(on)))
        .collect();
        let early = issue(&renew("00054321"), &secret, &public(), day("2027-10-09"));
        let last_day = issue(&renew("07654321"), &secret, &public(), day("2028-01-31"));
        let journal = fs::read_to_string(folder.join(JOURNAL));
        let _ = fs::remove_dir_all(&folder);

        let issued: Vec<Key> = issued.into_iter().map(|issued| issued.expect("своя пара").key).collect();
        let (mut wrong, mut rows) = (Vec::new(), Vec::new());
        for (what, result, longest, until) in [
            ("заранее", early, &issued[1], "2028-11-08"),
            ("в последний день", last_day, &issued[3], "2029-01-31"),
        ] {
            match result {
                Ok(Issued { key, renewed: Some(old) })
                    if old == *longest && key.tin() == longest.tin() && key.until() == day(until) =>
                {
                    rows.push(line(&key));
                }
                other => wrong.push(format!("{what}: {other:?}")),
            }
        }
        assert!(wrong.is_empty(), "{wrong:#?}");
        let journal = journal.expect("журнал");
        let last: Vec<String> = journal.lines().skip(5).map(|row| format!("{row}\n")).collect();
        assert_eq!((journal.lines().count(), last), (7, rows), "{journal}");
    }

    /// Продлевать нечего — отказ словами, и журнал не тронут: ключей на этот
    /// ՀՎՀՀ нет — «назовите --until»; ключ кончился — со днём, который дало
    /// бы продление, оплаченное в срок; строка журнала не та, что пишет
    /// выпуск, — с её номером. Журнала нет или он не читается — свои слова,
    /// а не «ключей нет»: скорее назван не тот закрытый ключ, и срок «со дня
    /// выдачи» отнял бы у покупателя остаток действующего (ревью задачи 97).
    #[test]
    fn renew_refuses_without_a_running_key() {
        let (folder, secret) = folder("refuse");
        let path = folder.join(JOURNAL);
        let missing = issue(&renew("00054321"), &secret, &public(), day("2026-11-09"));
        let created = path.exists();
        let first = issue(&order("2027-11-08"), &secret, &public(), day("2026-11-09"));
        let before = fs::read_to_string(&path).unwrap_or_default();
        let foreign = issue(&renew("07654321"), &secret, &public(), day("2026-11-10"));
        let ended = issue(&renew("00054321"), &secret, &public(), day("2027-11-09"));
        let after = fs::read_to_string(&path).unwrap_or_default();
        // Строка ключа ещё раз — с ՀՎՀՀ без нулей, как её оставил бы Excel.
        let excel = before.lines().nth(1).unwrap_or_default().replacen("\t00054321\t", "\t54321\t", 1);
        let broken = format!("{before}{excel}\n");
        let written = fs::write(&path, &broken);
        let edited = issue(&renew("00054321"), &secret, &public(), day("2026-11-10"));
        let left = fs::read_to_string(&path).unwrap_or_default();
        // Журнал, пересохранённый в UTF-16, — уже не текст UTF-8.
        let utf16: Vec<u8> = [0xFF, 0xFE]
            .into_iter()
            .chain(before.encode_utf16().flat_map(u16::to_le_bytes))
            .collect();
        let written16 = fs::write(&path, &utf16);
        let unreadable = issue(&renew("00054321"), &secret, &public(), day("2026-11-10"));
        let left16 = fs::read(&path).unwrap_or_default();
        let _ = fs::remove_dir_all(&folder);

        assert!(
            first.is_ok() && written.is_ok() && written16.is_ok(),
            "{first:?} {written:?} {written16:?}"
        );
        assert!(!created, "журнал появился от отказа");
        assert_eq!((after, left, left16), (before, broken, utf16), "журнал тронут отказом");
        let mut wrong = Vec::new();
        for (what, result, words, not) in [
            ("без журнала", &missing, &["тот ли закрытый ключ назван", "назовите --until"][..], "ключей на"),
            ("другой ՀՎՀՀ", &foreign, &["ключей на ՀՎՀՀ 07654321 нет", "назовите --until"][..], "тот ли"),
            ("кончился", &ended, &["кончился 2027-11-08", "--until 2028-11-08", "назовите --until"][..], "ключей на"),
            ("строка журнала", &edited, &["строка 3", "назовите --until"][..], "ключей на"),
            ("журнал не UTF-8", &unreadable, &["журнал "][..], "ключей на"),
        ] {
            match result {
                Err(error) if words.iter().all(|word| error.contains(word)) && !error.contains(not) => {}
                other => wrong.push(format!("{what}: {other:?}")),
            }
        }
        assert!(wrong.is_empty(), "{wrong:#?}");
    }

    /// Журнал читается, только каким его пишет выпуск: строку, правленную
    /// рукой, — ՀՎՀՀ без нулей (Excel), другой последний день или тариф, —
    /// ключ другой пары и чужую первую строку он не пропускает, а называет
    /// номером; пустую строку, пробелы в конце строк, BOM и переводы строки
    /// Windows — прощает.
    #[test]
    fn journal_reads_only_what_issue_writes() {
        let owner = tin("00054321");
        let key = sign(&signer(), &public(), &owner, Plan::Standard, day("2026-11-09"), day("2027-11-08"))
            .expect("своя пара");
        let stranger = SigningKey::from_bytes(&[9; 32]);
        let foreign = sign(
            &stranger,
            &stranger.verifying_key().to_bytes(),
            &owner,
            Plan::Standard,
            day("2026-11-09"),
            day("2028-11-08"),
        )
        .expect("пара другого");
        let row = line(&key);
        assert_eq!(journal(&format!("{HEADER}{row}"), &public()), Ok(vec![key.clone()]));
        let notepad = format!("\u{FEFF}{HEADER}\n{row}").replace('\n', "\r\n");
        assert_eq!(journal(&notepad, &public()), Ok(vec![key.clone()]));
        let spaces = format!("{HEADER}\n{row}").replace('\n', " \n");
        assert_eq!(journal(&spaces, &public()), Ok(vec![key.clone()]));
        assert_eq!(journal("", &public()), Ok(Vec::new()));
        let mut accepted = Vec::new();
        for bad in [
            format!("{HEADER}{}", row.replacen("\t00054321\t", "\t54321\t", 1)),
            format!("{HEADER}{}", row.replacen("\t2027-11-08\t", "\t2028-11-08\t", 1)),
            format!("{HEADER}{}", row.replacen("\tstandard\t", "\tpro\t", 1)),
            format!("{HEADER}{}", row.replacen('\t', " ", 1)),
            format!("{HEADER}{row}{}", line(&foreign)),
            format!("ключ\n{row}"),
        ] {
            if journal(&bad, &public()).is_ok() {
                accepted.push(bad);
            }
        }
        assert!(accepted.is_empty(), "приняты строки, которых выпуск не писал: {accepted:#?}");
        let numbered = journal(&format!("{HEADER}{row}\n{}", line(&foreign)), &public());
        assert!(numbered.as_ref().is_err_and(|error| error.contains("строка 4")), "{numbered:?}");
    }
}
