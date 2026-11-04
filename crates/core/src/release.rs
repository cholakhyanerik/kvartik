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
