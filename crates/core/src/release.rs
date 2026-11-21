//! Выпуск программы: её версия и «Что нового» (задача 67), а ещё имя файла
//! текстов лицензий, который сборка кладёт рядом с программой ([`LICENSES`],
//! задача 51).
//!
//! Новая версия сайта приходит в фоне и включается сама, как только не
//! открыта ни одна форма (Правило 2). Что в ней нового, окно говорит уже
//! после — словами из файла `notes.json` (`crates/app/public/notes.json`):
//! заметка о каждой версии на трёх языках. Пишет её задача, поднявшая
//! версию, вместе с `CHANGELOG.md`, а service worker кладёт файл в свой кэш
//! вместе со всей программой — поэтому заметки есть и без сети.
//!
//! # Файл — договор между версиями
//!
//! Читает файл не только программа, что его принесла, но и прежняя: по
//! заметкам она узнаёт, новее ли пришедшая сборка ([`Notes::newest`]),
//! а версии до 0.0.63 ещё и показывают их в окне с «Обновить». Поэтому:
//!
//! - лишнего разбор не замечает: незнакомое поле и незнакомый язык
//!   пропускает (здесь нет `deny_unknown_fields`, в отличие от записей
//!   формата данных), а заметку, которую не прочёл, — версию не из трёх
//!   чисел, дату не из календаря, — откладывает и считает
//!   ([`Notes::skipped`]). Сменить смысл поля нельзя: прежняя программа
//!   прочтёт его по-старому;
//! - не прочитался файл совсем — прежняя программа не знает, новее ли
//!   пришедшая, и сама страницу не перезагрузит: браузер включит новую
//!   версию, когда закроют все вкладки.
//!
//! Свежие заметки — сверху, и в файле их не больше [`MAX_NOTES`]: кто долго
//! не открывал программу, увидит изменения последних версий, а не историю
//! с первой (вся она — в `CHANGELOG.md`). Что ещё держит сам файл — тест
//! `crates/app/tests/notes.rs`.

use std::fmt;

use serde::Deserialize;
use serde::de::{self, Deserializer, Visitor};
use serde_json::value::RawValue;

use crate::i18n::Lang;
use crate::model::date::Date;
use crate::model::money::{Currency, Money};

/// Больше этого файл заметок не разбирается: двадцать версий на трёх языках
/// — десятки килобайт, а файл в мегабайты — уже не заметки, и разбирать его
/// посреди работы окна незачем (Правило 13: размер — до чтения).
pub const MAX_BYTES: usize = 256 * 1024;

/// Сколько версий держит файл и сколько, не больше, покажет окно. Выпуски
/// идут по нескольку в день (0.0.36–0.0.44 вышли 2026-10-16), и кто не
/// открывал программу неделю, отстал на десяток версий; кто дольше —
/// увидит последние двадцать.
pub const MAX_NOTES: usize = 20;

/// Файл текстов лицензий чужого кода и шрифтов в папке программы
/// (задача 51): пишет его программа сборки (`crates/sw/src/licenses.rs`),
/// а открывает экран «О программе». Имя — одно на обоих: ссылка на файл
/// другого имени вела бы на 404.
pub const LICENSES: &str = "licenses.txt";

/// Юридический текст сайта (задача 32): условия лицензии или политика
/// конфиденциальности. Страницы на трёх языках пишет программа сборки из
/// утверждённых файлов `docs/legal/<имя>.<код языка>.md`
/// (`crates/sw/src/legal.rs`), а ведут на них подвал лендинга и экран
/// «О программе». Адрес — один на всех, иначе ссылка вела бы на 404.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Legal {
    Terms,
    Privacy,
}

impl Legal {
    pub const ALL: [Legal; 2] = [Legal::Terms, Legal::Privacy];

    /// Имя файла текста и папки страницы: `terms`, `privacy`.
    pub const fn name(self) -> &'static str {
        match self {
            Legal::Terms => "terms",
            Legal::Privacy => "privacy",
        }
    }

    /// Адрес страницы на языке `lang` от корня сайта — рядом со страницей
    /// лендинга того же языка: армянская в корне (`/terms/`), русская
    /// и английская — в своих папках (`/ru/terms/`). Адрес — навсегда: им
    /// делятся, и его запоминают поисковики.
    pub const fn path(self, lang: Lang) -> &'static str {
        match (self, lang) {
            (Legal::Terms, Lang::Am) => "/terms/",
            (Legal::Terms, Lang::Ru) => "/ru/terms/",
            (Legal::Terms, Lang::En) => "/en/terms/",
            (Legal::Privacy, Lang::Am) => "/privacy/",
            (Legal::Privacy, Lang::Ru) => "/ru/privacy/",
            (Legal::Privacy, Lang::En) => "/en/privacy/",
        }
    }

    /// Прежние редакции текста — дни их редакций, от новой к старой. Каждая
    /// остаётся на сайте по своему адресу (условия, п. 11): ключ действует на
    /// условиях дня оплаты, и покупатель вправе прочесть, на каких. Текст
    /// прежней — файл `docs/legal/<имя>.<день>.<код языка>.md`, ровно тот, что
    /// был на сайте; новая редакция — новый день в начале списка и прежний
    /// файл под его днём, а не правка опубликованного.
    pub const fn earlier(self) -> &'static [Date] {
        match self {
            Legal::Terms => &TERMS_EARLIER,
            Legal::Privacy => &PRIVACY_EARLIER,
        }
    }

    /// Адрес прежней редакции дня `day` на языке `lang` — в папке текста:
    /// `/terms/2026-11-09/`, `/ru/terms/2026-11-09/`. Адрес — навсегда, как
    /// у самого текста: его называют сохранённые договоры.
    pub fn earlier_path(self, lang: Lang, day: Date) -> String {
        format!("{}{}/", self.path(lang), day.iso())
    }
}

/// Прежние редакции условий лицензии (`Legal::earlier`), от новой к старой:
/// от 20.11.2026 — до редакции от 21.11.2026 (задача 113, пробный ключ
/// продавца вместо 14 дней без ключа, п. 3, 4.4 и 6); от 18.11.2026 — по ней
/// продавались ключи до редакции от 20.11.2026
/// (задача 102, ответы ИИ-помощника — не консультация, п. 8); от 17.11.2026 —
/// до редакции от 18.11.2026 (задача 106, пробный период и по ՀՎՀՀ, п. 3);
/// от 15.11.2026 — до редакции
/// от 17.11.2026 (задача 103, тариф «Pro», п. 1, 2.1, 4.1, 4.2 и 5); от
/// 13.11.2026 — до редакции от 15.11.2026 (задача 99, пробный период с
/// первого дня данных, п. 3); от 09.11.2026 — первая, до редакции от
/// 13.11.2026 (задача 93, продление заранее, п. 4.1).
const TERMS_EARLIER: [Date; 6] = [
    Date::new(2026, 11, 20).expect("20 ноября 2026 года в календаре есть"),
    Date::new(2026, 11, 18).expect("18 ноября 2026 года в календаре есть"),
    Date::new(2026, 11, 17).expect("17 ноября 2026 года в календаре есть"),
    Date::new(2026, 11, 15).expect("15 ноября 2026 года в календаре есть"),
    Date::new(2026, 11, 13).expect("13 ноября 2026 года в календаре есть"),
    Date::new(2026, 11, 9).expect("9 ноября 2026 года в календаре есть"),
];

/// Прежние редакции политики конфиденциальности (`Legal::earlier`), от новой
/// к старой: от 20.11.2026 — до редакции от 21.11.2026 (задача 113: ՀՎՀՀ на
/// сервер больше не уходит, «Коротко», п. 2, 3, 5 и 6); от 18.11.2026 — до
/// редакции от 20.11.2026 (задача 102: вопрос
/// ИИ-помощнику уходит выбранному человеком поставщику, «Коротко» и п. 2);
/// от 09.11.2026 — первая, до редакции от 18.11.2026 (задача 106: ՀՎՀՀ уходит
/// на сервер сайта ради пробного периода, п. 2). Своё место у прежней — по тому
/// же обещанию, что у условий (п. 11 условий, п. 7 политики): человек вправе
/// прочесть, на что соглашался.
const PRIVACY_EARLIER: [Date; 3] = [
    Date::new(2026, 11, 20).expect("20 ноября 2026 года в календаре есть"),
    Date::new(2026, 11, 18).expect("18 ноября 2026 года в календаре есть"),
    Date::new(2026, 11, 9).expect("9 ноября 2026 года в календаре есть"),
];

/// Почта продавца и автора — та же, что в условиях лицензии и политике
/// конфиденциальности (задача 32): на неё пишут ՀՎՀՀ и согласие с условиями,
/// жалобы и запросы о данных. Почты на kvartik.com нет (решение человека
/// 2026-11-09). Что тексты называют её же, держит тест программы сборки.
pub const EMAIL: &str = "cholakhyanerik@gmail.com";

/// Telegram продавца — для вопросов (решение человека 2026-11-09): ссылкой
/// в подвале сайта и в «О программе», а не в тексте условий — личный адрес
/// продавец сменит, а новая редакция ради этого не нужна. ՀՎՀՀ и согласие
/// с условиями — только почтой (условия, п. 2; политика, п. 5).
pub const TELEGRAM: &str = "https://t.me/cholakhyanerik";

/// Письмо продавцу ([`EMAIL`]) адресом `mailto:` с темой `subject` и текстом
/// `body` — «Написать письмо» страницы «Купить» (задача 35) и «Написать
/// продавцу» о пробном ключе в окне (задача 113): почта человека открывается
/// с уже вписанным, а сама страница ничего не отправляет.
pub fn mailto(subject: &str, body: &str) -> String {
    format!("mailto:{EMAIL}?subject={}&body={}", mailto_part(subject), mailto_part(body))
}

/// Тема или текст письма в адресе `mailto:` (RFC 6068): всё, кроме
/// незарезервированных знаков URI, — байтами UTF-8 через `%`, а перенос
/// строки — `%0D%0A`, как RFC велит для текста письма (п. 5).
pub fn mailto_part(text: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(text.len() * 3);
    let mut bytes = [0; 4];
    for c in text.chars() {
        if c == '\n' {
            out.push_str("%0D%0A");
        } else if c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_' | '~') {
            out.push(c);
        } else {
            for byte in c.encode_utf8(&mut bytes).bytes() {
                out.push('%');
                out.push(char::from(HEX[usize::from(byte >> 4)]));
                out.push(char::from(HEX[usize::from(byte & 0xF)]));
            }
        }
    }
    out
}

/// Страница «Купить» (задача 35) на языке `lang` от корня сайта — рядом со
/// страницей лендинга того же языка, как юридические тексты: армянская —
/// `/buy/`, русская и английская — в своих папках. Её пишет программа
/// сборки (`crates/sw/src/landing.rs`), а ведут на неё лендинг, подвал
/// каждой страницы сайта и экран лицензии. Условия лицензии зовут её по
/// имени (п. 2, 4 и 5): тариф, его срок, цена и способы оплаты — на ней,
/// и ни новая цена, ни новый способ оплаты новой редакции условий не
/// требуют. Адрес — навсегда: его называют условия.
pub const fn buy_path(lang: Lang) -> &'static str {
    match lang {
        Lang::Am => "/buy/",
        Lang::Ru => "/ru/buy/",
        Lang::En => "/en/buy/",
    }
}

/// Страница «Отзывы и предложения» (задача 114) на языке `lang` от корня
/// сайта — рядом со страницей лендинга того же языка, как [`buy_path`]: что
/// написать об ошибке или улучшении, письмо продавцу ([`EMAIL`]) с уже
/// вписанными строками и [`TELEGRAM`]. Её пишет программа сборки
/// (`crates/sw/src/landing.rs`), а ведут на неё подвал каждой страницы сайта
/// и экран «О программе». Адрес — навсегда: прежние версии программы
/// открывают его годами.
pub const fn feedback_path(lang: Lang) -> &'static str {
    match lang {
        Lang::Am => "/feedback/",
        Lang::Ru => "/ru/feedback/",
        Lang::En => "/en/feedback/",
    }
}

/// Цена ключа «Стандарт» на год (решение человека 2026-11-09, была задача
/// 42): на старте продаётся только он, а помесячный — потом, с выдачей без
/// рук, и так, чтобы год помесячно выходил дороже. Число одно на страницу
/// «Купить», лендинг и экран лицензии: условия берут цену со страницы
/// «Купить» на день оплаты (п. 5), и цена, разошедшаяся на двух страницах,
/// — спор с покупателем.
pub const PRICE: Money = Money::new(24_000, Currency::Amd);

/// Цена ключа «Pro» на год и сколько ИП он открывает (решение человека
/// 2026-11-17, задача 103): 10 мест за 120 000 ֏ — по 12 000 ֏ за ИП, вдвое
/// дешевле «Стандарта». Числа одни на страницу «Купить» и экран лицензии,
/// как у [`PRICE`], а мест у ключа без `--seats` выпускает столько же
/// `kvartik-keygen`: проданный ключ открывает то, что обещала страница.
pub const PRO_PRICE: Money = Money::new(120_000, Currency::Amd);

/// Мест у ключа «Pro» на странице «Купить» ([`PRO_PRICE`]).
pub const PRO_SEATS: u8 = 10;

// «Pro» — на несколько ИП, и цена его делится на места нацело: страница
// «Купить» называет цену за ИП, и она не округлена. Проверка — при сборке.
const _: () = assert!(PRO_SEATS > 1 && PRO_PRICE.minor() % PRO_SEATS as i64 == 0);

/// Счёт продавца для перевода — в драмах, в IDBank (открыт 2026-11-09;
/// номер, банк и владелец — из реквизитов, которые выдал банк). Первые
/// три цифры — код банка, `118`. Контрольная цифра сходится (решение ЦБ
/// № 178-Ն, `BankAccount::mistyped`), и это держит тест ниже: с опечаткой
/// в номере перевод покупателя ушёл бы не туда или вернулся бы через
/// неделю.
pub const ACCOUNT: &str = "11815068174000";

/// Банк счёта [`ACCOUNT`] — как в реквизитах банка.
pub const BANK: &str = "ԱյԴի Բանկ ՓԲԸ";

/// Владелец счёта [`ACCOUNT`] — как в реквизитах банка, по-армянски: это
/// имя покупатель вписывает в перевод, и страница не переводит его, а
/// называет рядом, как продавца зовут условия.
pub const HOLDER: &str = "Էրիկ Չոլախյան";

/// ՀՎՀՀ продавца — ИП Эрика Чолахяна (выписка из реестра, 2026-11-06), тот
/// же, что в условиях лицензии (п. 1 и 12): это держит тест программы
/// сборки.
pub const SELLER_TIN: &str = "58111423";

/// Версия программы — три числа `major.minor.patch`, как `version`
/// в `Cargo.toml` (Правило 5). Сравнивается по числам: 0.0.10 новее 0.0.9.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Version {
    major: u32,
    minor: u32,
    patch: u32,
}

impl Version {
    /// Версия этой программы — `version` из `[workspace.package]` корневого
    /// `Cargo.toml`: у ядра она та же, что у окна. Разбирается при сборке,
    /// и запись не из трёх чисел там — ошибка сборки, а не паника в окне.
    pub const CURRENT: Version = match Version::parse(env!("CARGO_PKG_VERSION")) {
        Some(version) => version,
        None => panic!("version в Cargo.toml — не три числа через точку"),
    };

    pub const fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    /// Версия из записи `0.0.45`. `None` — не три числа через точку: с хвостом
    /// (`1.0.0-beta`), с ведущим нулём (`0.01.0`), с пробелом, с числом больше
    /// `u32`. Разбор строгий, как у даты: что пишет `Cargo.toml`, то и
    /// читается, а прочее угадывать нельзя.
    pub const fn parse(text: &str) -> Option<Self> {
        let bytes = text.as_bytes();
        let mut parts = [0_u32; 3];
        let mut part = 0;
        let mut digits = 0;
        let mut at = 0;
        while at < bytes.len() {
            let byte = bytes[at];
            if byte == b'.' {
                if digits == 0 || part == 2 {
                    return None;
                }
                part += 1;
                digits = 0;
            } else if byte.is_ascii_digit() {
                // Ведущий ноль: «0» — число, а «01» — нет.
                if digits == 1 && parts[part] == 0 {
                    return None;
                }
                let Some(shifted) = parts[part].checked_mul(10) else {
                    return None;
                };
                let Some(value) = shifted.checked_add((byte - b'0') as u32) else {
                    return None;
                };
                parts[part] = value;
                digits += 1;
            } else {
                return None;
            }
            at += 1;
        }
        if part != 2 || digits == 0 {
            return None;
        }
        Some(Self::new(parts[0], parts[1], parts[2]))
    }
}

/// `0.0.45` — одна запись на всех языках: цифры и точки.
impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// В файле версия — строка `"0.0.45"`, и читается она только через
/// [`Version::parse`].
impl<'de> Deserialize<'de> for Version {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Text;

        impl Visitor<'_> for Text {
            type Value = Version;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("версию из трёх чисел через точку, вида 0.0.45")
            }

            fn visit_str<E: de::Error>(self, text: &str) -> Result<Version, E> {
                Version::parse(text).ok_or_else(|| E::invalid_value(de::Unexpected::Str(text), &self))
            }
        }

        deserializer.deserialize_str(Text)
    }
}

/// Что изменилось в версии — словами человека, на трёх языках: строка —
/// пункт списка. Язык — его код в файле (`hy`, `ru`, `en`), как в данных.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
pub struct Note {
    version: Version,
    /// День выпуска — как у записи `CHANGELOG.md`.
    date: Date,
    #[serde(default)]
    hy: Vec<String>,
    #[serde(default)]
    ru: Vec<String>,
    #[serde(default)]
    en: Vec<String>,
}

impl Note {
    pub const fn version(&self) -> Version {
        self.version
    }

    pub const fn date(&self) -> Date {
        self.date
    }

    /// Пункты на языке `lang`; пусто — на нём о версии ничего нет.
    pub fn items(&self, lang: Lang) -> &[String] {
        match lang {
            Lang::Am => &self.hy,
            Lang::Ru => &self.ru,
            Lang::En => &self.en,
        }
    }
}

/// Файл заметок не прочитался.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NotesError {
    /// Больше [`MAX_BYTES`].
    TooBig,
    /// Не JSON или в нём нет списка `notes`.
    Malformed,
}

/// Заметки файла `notes.json` — в его порядке: свежие сверху.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Notes {
    notes: Vec<Note>,
    skipped: usize,
}

impl Notes {
    /// Разобрать файл `{"notes": [{"version": "0.0.45", "date":
    /// "2026-10-18", "hy": […], "ru": […], "en": […]}, …]}`. Заметка, которую
    /// разбор не прочёл, не роняет остальные (шапка модуля).
    pub fn parse(text: &str) -> Result<Self, NotesError> {
        #[derive(Deserialize)]
        struct File {
            notes: Vec<Box<RawValue>>,
        }

        if text.len() > MAX_BYTES {
            return Err(NotesError::TooBig);
        }
        let file: File = serde_json::from_str(text).map_err(|_| NotesError::Malformed)?;
        let mut notes = Vec::with_capacity(file.notes.len());
        let mut skipped = 0;
        for raw in &file.notes {
            match serde_json::from_str::<Note>(raw.get()) {
                Ok(note) => notes.push(note),
                Err(_) => skipped += 1,
            }
        }
        Ok(Self { notes, skipped })
    }

    /// Все прочитанные заметки, в порядке файла.
    pub fn all(&self) -> &[Note] {
        &self.notes
    }

    /// Сколько заметок разбор не прочёл.
    pub const fn skipped(&self) -> usize {
        self.skipped
    }

    /// Самая новая версия, о которой есть заметка: из двух файлов на
    /// устройстве — работающей сборки и пришедшей — новее тот, где она
    /// больше.
    pub fn newest(&self) -> Option<Version> {
        self.notes.iter().map(Note::version).max()
    }

    /// Заметки версий новее `seen` и не новее `running` — того, что
    /// изменилось с версии, о которой человек знает, — в порядке файла и не
    /// больше [`MAX_NOTES`]. В файле могут быть и версии новее работающей:
    /// читается самый новый файл на устройстве, а новая сборка может уже
    /// ждать включения.
    pub fn between(&self, seen: Version, running: Version) -> impl Iterator<Item = &Note> {
        self.notes
            .iter()
            .filter(move |note| seen < note.version && note.version <= running)
            .take(MAX_NOTES)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(text: &str) -> Version {
        Version::parse(text).unwrap_or_else(|| panic!("«{text}» — версия"))
    }

    /// Заметка в JSON: версия, день и по пункту на каждом языке.
    fn note(version: &str, date: &str) -> String {
        format!(
            r#"{{"version": "{version}", "date": "{date}", "hy": ["Նոր {version}"], "ru": ["Новое {version}"], "en": ["New {version}"]}}"#
        )
    }

    fn file(notes: &[String]) -> String {
        format!(r#"{{"notes": [{}]}}"#, notes.join(", "))
    }

    /// Цена «Pro» — в драмах (задача 103), как и «Стандарта»: страница
    /// «Купить» берёт их одним счётом продавца в драмах.
    #[test]
    fn the_pro_price_is_in_drams() {
        assert_eq!((PRO_PRICE.currency(), PRICE.currency()), (Currency::Amd, Currency::Amd));
    }

    /// Страница юридического текста — у каждого текста и языка своя, рядом
    /// со страницей лендинга этого языка, и в адресе — имя текста: шесть
    /// разных адресов, и ни один не ложится на чужой.
    #[test]
    fn legal_pages_have_their_own_addresses() {
        let mut seen = Vec::new();
        for doc in Legal::ALL {
            for lang in Lang::ALL {
                let path = doc.path(lang);
                let home = if lang == Lang::Am { "/".to_owned() } else { format!("/{}/", lang.code()) };
                assert_eq!(path, format!("{home}{}/", doc.name()), "{doc:?} {lang:?}");
                assert!(!seen.contains(&path), "{path} — дважды");
                seen.push(path);
            }
        }
        assert!(EMAIL.contains('@') && TELEGRAM.starts_with("https://t.me/"));
    }

    /// Прежняя редакция — в папке своего текста и языка, у каждого дня
    /// и языка свой адрес, и ни один не ложится на действующую редакцию или
    /// на чужой текст. Дни — от новой к старой, без повторов.
    #[test]
    fn earlier_editions_have_their_own_addresses() {
        let mut wrong = Vec::new();
        let mut seen: Vec<String> = Legal::ALL
            .into_iter()
            .flat_map(|doc| Lang::ALL.map(|lang| doc.path(lang).to_owned()))
            .collect();
        for doc in Legal::ALL {
            let days = doc.earlier();
            if days.windows(2).any(|pair| pair[0] <= pair[1]) {
                wrong.push(format!("{doc:?}: дни не от новой к старой"));
            }
            for &day in days {
                for lang in Lang::ALL {
                    let path = doc.earlier_path(lang, day);
                    if path != format!("{}{}/", doc.path(lang), day.iso()) {
                        wrong.push(format!("{doc:?} {lang:?}: {path}"));
                    }
                    if seen.contains(&path) {
                        wrong.push(format!("{path} — дважды"));
                    }
                    seen.push(path);
                }
            }
        }
        assert_eq!(Legal::Terms.earlier_path(Lang::Ru, Legal::Terms.earlier()[0]), "/ru/terms/2026-11-20/");
        assert_eq!(Legal::Terms.earlier_path(Lang::Ru, Legal::Terms.earlier()[1]), "/ru/terms/2026-11-18/");
        assert_eq!(Legal::Terms.earlier_path(Lang::Ru, Legal::Terms.earlier()[2]), "/ru/terms/2026-11-17/");
        assert_eq!(Legal::Terms.earlier_path(Lang::Ru, Legal::Terms.earlier()[3]), "/ru/terms/2026-11-15/");
        assert_eq!(Legal::Terms.earlier_path(Lang::Ru, Legal::Terms.earlier()[4]), "/ru/terms/2026-11-13/");
        assert_eq!(Legal::Terms.earlier_path(Lang::Ru, Legal::Terms.earlier()[5]), "/ru/terms/2026-11-09/");
        assert_eq!(Legal::Privacy.earlier_path(Lang::Am, Legal::Privacy.earlier()[0]), "/privacy/2026-11-20/");
        assert_eq!(Legal::Privacy.earlier_path(Lang::Am, Legal::Privacy.earlier()[1]), "/privacy/2026-11-18/");
        assert_eq!(Legal::Privacy.earlier_path(Lang::Am, Legal::Privacy.earlier()[2]), "/privacy/2026-11-09/");
        assert!(wrong.is_empty(), "{wrong:#?}");
    }

    /// Страницы «Купить» и «Отзывы и предложения» (задача 114) — у каждого
    /// языка свои, в папке страницы этого языка, рядом с его юридическими
    /// текстами, и ни одна не ложится на юридический текст или на другую.
    #[test]
    fn buy_and_feedback_pages_sit_next_to_the_legal_texts() {
        /// Адрес страницы на языке.
        type Path = fn(Lang) -> &'static str;
        let mut wrong = Vec::new();
        let pages: [(Path, &str); 2] = [(buy_path, "buy/"), (feedback_path, "feedback/")];
        for (page, name) in pages {
            for lang in Lang::ALL {
                let own = page(lang);
                for doc in Legal::ALL {
                    let path = doc.path(lang);
                    if own == path || own.trim_end_matches(name) != path.trim_end_matches(&format!("{}/", doc.name())) {
                        wrong.push(format!("{lang:?}: {own} и {path}"));
                    }
                }
                if Lang::ALL.iter().filter(|other| page(**other) == own).count() != 1 {
                    wrong.push(format!("{own} — у двух языков"));
                }
                if buy_path(lang) == feedback_path(lang) {
                    wrong.push(format!("{lang:?}: «Купить» и отзывы — один адрес"));
                }
            }
        }
        assert!(wrong.is_empty(), "{wrong:#?}");
    }

    /// Реквизиты продавца — без опечатки, которую можно поймать: номер
    /// счёта — счёт банка Армении с верной контрольной цифрой, `118` —
    /// IDBank, ՀՎՀՀ — восемь цифр, цена — целые драмы больше нуля.
    #[test]
    fn the_seller_details_have_no_catchable_typo() {
        use crate::model::bank::BankAccount;
        use crate::model::tin::Tin;
        assert!(!BankAccount::mistyped(ACCOUNT, None), "контрольная цифра счёта {ACCOUNT} не сходится");
        assert!(ACCOUNT.len() >= 12 && ACCOUNT.bytes().all(|b| b.is_ascii_digit()) && ACCOUNT.starts_with("118"));
        assert_eq!(Tin::parse(SELLER_TIN).map(|tin| tin.as_str().to_owned()), Ok(SELLER_TIN.to_owned()));
        assert_eq!(PRICE.currency(), Currency::Amd);
        assert!(PRICE.minor() > 0);
        assert!(!BANK.is_empty() && !HOLDER.is_empty());
    }

    /// Версии сравниваются по числам, а не строками: 0.0.10 новее 0.0.9,
    /// а запись читается обратно той же.
    #[test]
    fn versions_compare_by_numbers() {
        let order = ["0.0.0", "0.0.9", "0.0.10", "0.1.0", "0.10.0", "1.0.0", "4294967295.0.0"];
        for pair in order.windows(2) {
            assert!(v(pair[0]) < v(pair[1]), "{} не раньше {}", pair[0], pair[1]);
        }
        let mut wrong = Vec::new();
        for text in order {
            if v(text).to_string() != text {
                wrong.push(text);
            }
        }
        assert!(wrong.is_empty(), "запись не сошлась: {wrong:?}");
    }

    /// Разбор строгий: только три числа через точку, без хвоста, пробелов,
    /// ведущих нулей и переполнения.
    #[test]
    fn version_parse_is_strict() {
        let mut wrong = Vec::new();
        for text in [
            "", "0", "0.0", "0.0.", ".0.0", "0..1", "0.0.0.0", "1.0.0-beta", "v1.0.0", " 0.0.1",
            "0.0.1 ", "01.0.0", "0.0.045", "a.b.c", "0.0.+1", "0.0.-1", "4294967296.0.0", "0.0.٣",
        ] {
            if let Some(version) = Version::parse(text) {
                wrong.push(format!("«{text}» — {version}"));
            }
        }
        assert!(wrong.is_empty(), "приняты: {wrong:#?}");
    }

    /// Версия программы — та, что в `Cargo.toml`.
    #[test]
    fn current_is_the_cargo_version() {
        assert_eq!(Version::CURRENT.to_string(), env!("CARGO_PKG_VERSION"));
    }

    /// Окно показывает версии новее той, что человек видел, и не новее
    /// работающей — в порядке файла, со словами на языке окна, — а самая
    /// новая версия файла — наибольшая в нём.
    #[test]
    fn notes_between_the_seen_and_the_running_version() {
        let text = file(&[note("0.0.47", "2026-10-20"), note("0.0.46", "2026-10-19"), note("0.0.45", "2026-10-18")]);
        let notes = Notes::parse(&text).expect("файл читается");
        assert_eq!(notes.skipped(), 0);
        assert_eq!(notes.newest(), Some(v("0.0.47")));
        let shown: Vec<String> = notes
            .between(v("0.0.45"), v("0.0.47"))
            .map(|note| note.version().to_string())
            .collect();
        assert_eq!(shown, ["0.0.47", "0.0.46"]);
        assert_eq!(notes.between(v("0.0.47"), v("0.0.47")).count(), 0);
        assert_eq!(notes.between(v("0.0.9"), v("0.0.47")).count(), 3);
        // Сборка новее работающей ждёт включения: её заметки — ещё не про
        // то, что человек получил.
        let waiting: Vec<String> = notes
            .between(v("0.0.45"), v("0.0.46"))
            .map(|note| note.version().to_string())
            .collect();
        assert_eq!(waiting, ["0.0.46"]);
        let newest = &notes.all()[0];
        assert_eq!(newest.date(), Date::new(2026, 10, 20).expect("день есть"));
        assert_eq!(newest.items(Lang::Am), ["Նոր 0.0.47"]);
        assert_eq!(newest.items(Lang::Ru), ["Новое 0.0.47"]);
        assert_eq!(newest.items(Lang::En), ["New 0.0.47"]);
    }

    /// Прежняя программа читает файл новой: незнакомое поле и незнакомый
    /// язык ей не мешают, а заметка, которую она не прочла, не роняет
    /// остальные — её она пропускает и считает.
    #[test]
    fn a_newer_file_still_reads() {
        let text = format!(
            r#"{{"format": 2, "notes": [
                {{"version": "0.1.0", "date": "2026-11-01", "ru": ["Новое"], "de": ["Neu"], "links": []}},
                {{"version": "1.0.0-beta", "date": "2026-10-30", "ru": ["Бета"]}},
                {{"version": "0.0.50", "date": "2026-13-01", "ru": ["Не тот день"]}},
                {{"version": "0.0.49", "ru": ["Без дня"]}},
                "не заметка",
                {}
            ]}}"#,
            note("0.0.48", "2026-10-29"),
        );
        let notes = Notes::parse(&text).expect("файл читается");
        assert_eq!(notes.skipped(), 4);
        let read: Vec<String> = notes.all().iter().map(|note| note.version().to_string()).collect();
        assert_eq!(read, ["0.1.0", "0.0.48"]);
        assert_eq!(notes.all()[0].items(Lang::Ru), ["Новое"]);
        assert!(notes.all()[0].items(Lang::En).is_empty());
    }

    /// Не JSON, JSON без списка и файл больше предела — ошибка, а не паника
    /// и не пустой список.
    #[test]
    fn a_broken_or_huge_file_is_an_error() {
        let mut wrong = Vec::new();
        for (text, error) in [
            ("", NotesError::Malformed),
            ("<!doctype html>", NotesError::Malformed),
            ("[]", NotesError::Malformed),
            (r#"{"notes": {}}"#, NotesError::Malformed),
            (r#"{"releases": []}"#, NotesError::Malformed),
        ] {
            if Notes::parse(text) != Err(error) {
                wrong.push(text);
            }
        }
        assert!(wrong.is_empty(), "не ошибка: {wrong:?}");
        let padded = format!(r#"{{"notes": [], "pad": "{}"}}"#, " ".repeat(MAX_BYTES));
        assert_eq!(Notes::parse(&padded), Err(NotesError::TooBig));
        assert_eq!(Notes::parse(r#"{"notes": []}"#).map(|notes| notes.all().len()), Ok(0));
    }

    /// Сколько бы версий ни было новее, окно покажет не больше
    /// [`MAX_NOTES`] — свежие.
    #[test]
    fn between_is_bounded() {
        let many: Vec<String> = (0..MAX_NOTES + 5)
            .rev()
            .map(|n| note(&format!("0.1.{n}"), "2026-10-18"))
            .collect();
        let notes = Notes::parse(&file(&many)).expect("файл читается");
        let newest = Version::new(0, 1, MAX_NOTES as u32 + 4);
        let shown: Vec<Version> = notes.between(v("0.0.45"), newest).map(Note::version).collect();
        assert_eq!(shown.len(), MAX_NOTES);
        assert_eq!(shown[0], newest);
    }
}
