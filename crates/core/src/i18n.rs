//! Языки, таблица строк и запись чисел, сумм и дат.
//!
//! Модуль зовут и домен, и документы, и окно. Сам он знает только значения
//! домена — сумму, валюту и дату (`model::money`, `model::date`), чтобы
//! записать их по правилам языка. Те ни от кого не зависят, поэтому цикла
//! нет и тогда, когда документ домена хранит свой `Lang`. Ни Leptos, ни
//! хранилища, ни состояния экрана здесь нет (таблица слоёв CLAUDE.md).
//!
//! Язык **ездит параметром**, а не лежит глобалью: язык окна и язык
//! документа — разные вещи. ИП с русским окном выписывает счёт по-английски,
//! а акт местной фирме — по-армянски.
//!
//! Таблица строк живёт здесь, а запись чисел, сумм, курсов и дат и разбор
//! введённой суммы — в `locale`: таблицу правит почти каждая задача, запись
//! чисел — редко, и в одном файле они мешали бы друг другу (Правило 8).
//!
//! # Правило 1 и эти функции
//!
//! [`t`] и [`plural`] отдают `&'static str` и ничего не собирают. Где в строку
//! надо подставить число или имя, в таблице лежит **шаблон** с `{}`,
//! а подстановку делает [`fill`]. Собранная строка в Leptos — производное от
//! сигнала языка, а не значение, вычисленное один раз: иначе при смене языка
//! она останется прежней (Правило 4).

mod locale;

pub use locale::{ParseError, amount, date, integer, money, parse_money, parse_rate, rate};

/// Язык окна или документа.
///
/// Вариант армянского называется `Am`, как в Arpa и Savio, но код в файлах
/// и настройках — `hy` (ISO 639-1): `am` — это амхарский.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Lang {
    Am,
    Ru,
    En,
}

impl Lang {
    /// Все языки. Порядок — армянский первым: программа для ИП Армении.
    pub const ALL: [Lang; 3] = [Lang::Am, Lang::Ru, Lang::En];

    /// Код для настроек, файлов и атрибута `lang` в разметке. Двухбуквенный
    /// и устойчивый: имя варианта менять можно, а это — нет, иначе у всех
    /// разом слетит запомненный выбор языка.
    pub const fn code(self) -> &'static str {
        match self {
            Lang::Am => "hy",
            Lang::Ru => "ru",
            Lang::En => "en",
        }
    }

    /// Разбирает код из настроек или файла. `None` — код чужой.
    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|lang| lang.code() == code)
    }
}

/// Объявляет ключ и три его перевода разом.
///
/// Макрос, а не три таблицы рядом, ровно затем, чтобы переводы **нельзя**
/// было разъехать: ключ без одного языка не соберётся (опыт Savio).
macro_rules! strings {
    ($( $(#[$meta:meta])* $key:ident = $ru:literal, $en:literal, $am:literal; )*) => {
        /// Ключ строки, которую видит человек.
        ///
        /// Перечисление, а не сама строка: по ключу компилятор ловит опечатку,
        /// а по русскому тексту — нет.
        #[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
        pub enum Key {
            $( $(#[$meta])* $key, )*
        }

        /// Строка на выбранном языке. Ничего не собирает и не выделяет.
        pub const fn t(lang: Lang, key: Key) -> &'static str {
            match key {
                $(
                    Key::$key => match lang {
                        Lang::Ru => $ru,
                        Lang::En => $en,
                        Lang::Am => $am,
                    },
                )*
            }
        }

        /// Все ключи вместе с их именами — для проверок таблицы: полноты здесь
        /// и знаков в шрифтах окна (`crates/app/tests/fonts.rs`). Открыт, а не
        /// `#[cfg(test)]`: тест другого крейта видит ядро как обычную
        /// зависимость. В wasm окна массива нет, пока его никто не зовёт.
        pub const ALL_KEYS: &[(Key, &str)] = &[ $( (Key::$key, stringify!($key)), )* ];
    };
}

strings! {
    /// Название программы. Рабочее, до задачи 16 реестра (имя и домен решает
    /// человек): армянское — из описания проекта, русское и английское — его
    /// дословный перевод.
    AppName = "Офис ИП", "IE Office", "ԱՁ գրասենյակ";

    /// Раздел о том, где и как хранятся данные (задача 6). Пока экрана
    /// настроек нет, раздел стоит на единственном экране каркаса.
    StorageTitle = "Хранение данных", "Data storage", "Տվյալների պահպանում";
    /// Браузер дал постоянное хранение (`navigator.storage.persisted()`).
    StoragePersistent =
        "Хранение постоянное: браузер не сотрёт данные сам.",
        "Storage is persistent: the browser won't erase your data on its own.",
        "Պահպանումը մշտական է. դիտարկիչն ինքնուրույն չի ջնջի տվյալները։";
    /// Постоянного хранения нет — частый ответ, а не ошибка (Правило 6).
    StorageBestEffort =
        "Браузер может стереть данные сам: когда на устройстве мало места или программу долго не открывают, а в приватном окне\u{A0}— при его закрытии. Делайте резервные копии.",
        "The browser may erase your data on its own: when the device is low on space or the app hasn't been opened for a long time, and in a private window\u{A0}— when it's closed. Make backups.",
        "Դիտարկիչը կարող է ինքնուրույն ջնջել տվյալները. երբ սարքում տեղը քիչ է կամ ծրագիրը երկար չեն բացում, իսկ գաղտնի պատուհանում՝ այն փակելիս։ Պարբերաբար պահուստային պատճեն արեք։";
    /// Браузер не говорит, постоянное ли хранение: `navigator.storage` нет
    /// (страница не по HTTPS) или он не ответил.
    StorageUnknown =
        "Браузер не сообщает, может ли он стереть данные сам. Делайте резервные копии.",
        "The browser doesn't say whether it may erase your data on its own. Make backups.",
        "Դիտարկիչը չի հայտնում՝ արդյոք կարող է ինքնուրույն ջնջել տվյալները։ Պարբերաբար պահուստային պատճեն արեք։";
    /// Новой версии базы мешает вкладка со старой (`blocked`). Пройдёт само,
    /// когда та вкладка закроется.
    StorageBlocked =
        "Программа открыта в другой вкладке со старой версией. Закройте другие вкладки с программой\u{A0}— эта продолжит сама.",
        "The app is open in another tab with an older version. Close its other tabs\u{A0}— this one will continue on its own.",
        "Ծրագիրը բաց է մեկ այլ ներդիրում՝ հին տարբերակով։ Փակեք ծրագրի մյուս ներդիրները՝ այս մեկն ինքը կշարունակի։";

    /// Хранилища нет или браузер не дал его открыть. В актуальных браузерах
    /// это почти всегда запрет хранить данные сайтов: IndexedDB в приватном
    /// окне есть и в Chrome, и в Safari, и в Firefox с 115-й версии
    /// (разведка задачи 6, 2026-10-12).
    StoreUnavailable =
        "Браузер не даёт программе хранить данные\u{A0}— обычно потому, что в его настройках сайтам запрещено сохранять данные. Разрешите это программе и перезагрузите страницу.",
        "The browser won't let the app store data\u{A0}— usually because its settings don't allow sites to save data. Allow it for the app and reload the page.",
        "Դիտարկիչը թույլ չի տալիս ծրագրին պահել տվյալները. սովորաբար այն պատճառով, որ նրա կարգավորումներում կայքերին արգելված է տվյալներ պահել։ Թույլ տվեք դա ծրագրին և վերաբեռնեք էջը։";
    /// База или запись новее этой версии программы (Правило 2).
    StoreNewer =
        "Данные сохранены более новой версией программы. Перезагрузите страницу\u{A0}— откроется новая версия.",
        "Your data was saved by a newer version of the app. Reload the page to open the new version.",
        "Տվյալները պահվել են ծրագրի ավելի նոր տարբերակով։ Վերաբեռնեք էջը՝ կբացվի նոր տարբերակը։";
    /// Другая вкладка обновила базу, и эта закрыла соединение
    /// (`versionchange`, Правило 6).
    StoreReplaced =
        "Программа обновилась в другой вкладке. Перезагрузите эту страницу: до перезагрузки изменения здесь не сохраняются.",
        "The app was updated in another tab. Reload this page: until you do, changes here can't be saved.",
        "Ծրագիրը թարմացվել է մեկ այլ ներդիրում։ Վերաբեռնեք այս էջը. մինչ այդ այստեղ փոփոխությունները չեն պահվի։";
    /// Браузер сам закрыл базу (событие `close`): данные сайта стёрли или
    /// у Safari упал процесс хранилища. Во втором случае данные целы, и до
    /// Safari 27 помогает только перезагрузка (WebKit bug 273827; разведка
    /// задачи 6, 2026-10-12).
    StoreLost =
        "Браузер закрыл доступ к данным. Перезагрузите страницу\u{A0}— обычно этого достаточно. Если после этого данных нет, их стёрли: восстановите их из резервной копии.",
        "The browser closed access to your data. Reload the page\u{A0}— that's usually enough. If your data is gone after that, it was erased: restore it from a backup.",
        "Դիտարկիչը փակել է տվյալների հասանելիությունը։ Վերաբեռնեք էջը. սովորաբար դա բավական է։ Եթե դրանից հետո տվյալները չկան, ուրեմն դրանք ջնջվել են. վերականգնեք դրանք պահուստային պատճենից։";
    /// `QuotaExceededError`: действие не сохранено (Правило 6). Совет —
    /// пример из «Общего» в CLAUDE.md.
    StoreFull =
        "Не сохранено: браузеру не хватило места. Сделайте резервную копию и освободите место на устройстве.",
        "Not saved: the browser ran out of space. Make a backup and free up space on your device.",
        "Չպահվեց. դիտարկիչին տեղը չբավարարեց։ Պահուստային պատճեն արեք և տեղ ազատեք սարքում։";
    /// Запись в базе не читается (`FormatError::Damaged`).
    StoreDamaged =
        "Часть сохранённых данных не читается. Восстановите их из резервной копии.",
        "Some saved data can't be read. Restore it from a backup.",
        "Պահված տվյալների մի մասը չի կարդացվում։ Վերականգնեք դրանք պահուստային պատճենից։";
    /// Любая другая ошибка хранилища. Подробность — в консоли.
    StoreFailed =
        "Не получилось: браузер ответил ошибкой при работе с данными. Перезагрузите страницу и попробуйте ещё раз.",
        "That didn't work: the browser returned an error while working with your data. Reload the page and try again.",
        "Չստացվեց. դիտարկիչը սխալ տվեց տվյալների հետ աշխատելիս։ Վերաբեռնեք էջը և նորից փորձեք։";
    /// Кнопка «перезагрузить страницу».
    Reload = "Перезагрузить", "Reload", "Վերաբեռնել";
}

/// Подставляет значения в шаблон вместо `{}`, по порядку.
///
/// Своя подстановка, а не `format!`: `format!` требует строку-литерал, а
/// шаблон приезжает из таблицы. Порядок мест — порядок `{}` в самом шаблоне,
/// и ради этого она и нужна: в армянском число и слово стоят иначе, чем
/// в русском, и склейка кусками на стороне вызова зашила бы русский порядок
/// слов во все три языка (опыт Savio).
///
/// Лишний `{}` остаётся в строке как есть: видимая дыра лучше молча
/// съеденного куска текста.
pub fn fill(template: &str, args: &[&str]) -> String {
    let mut out =
        String::with_capacity(template.len() + args.iter().map(|arg| arg.len()).sum::<usize>());
    let mut rest = template;
    let mut args = args.iter();
    while let Some(at) = rest.find("{}") {
        let Some(arg) = args.next() else {
            break;
        };
        out.push_str(&rest[..at]);
        out.push_str(arg);
        rest = &rest[at + 2..];
    }
    out.push_str(rest);
    out
}

/// Форма слова при числительном.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Plural {
    One,
    Few,
    Many,
}

/// Какая форма слова нужна при числе `n`.
///
/// Русскому нужны три: «1 счёт, 2 счёта, 5 счетов», и одиннадцать —
/// четырнадцать — исключение: «11 счетов», «112 счетов». Правило совпадает
/// с CLDR 48 на всех числах от 0 до 2000 (сверено с `Intl.PluralRules`
/// 2026-10-11). Английскому нужны две, `Few` у него не бывает.
///
/// Армянскому — одна: при числительном существительное остаётся
/// в единственном числе, «5 հաշիվ» (решение человека 2026-10-11, как
/// в Arpa и Savio). У CLDR для армянского две категории — `one` для 0 и 1
/// и `other`, — но форму существительного при числе они не меняют.
const fn plural_form(lang: Lang, n: u64) -> Plural {
    match lang {
        Lang::Ru => {
            if n % 100 >= 11 && n % 100 <= 14 {
                Plural::Many
            } else {
                match n % 10 {
                    1 => Plural::One,
                    2..=4 => Plural::Few,
                    _ => Plural::Many,
                }
            }
        }
        Lang::En => {
            if n == 1 {
                Plural::One
            } else {
                Plural::Many
            }
        }
        Lang::Am => Plural::One,
    }
}

/// Форма слова при числе `n` из ключей `…One`, `…Few`, `…Many`.
///
/// Ключей всегда три, потому что их требует русский. В английском переводе
/// `…Few` повторяет `…Many`, а в армянском все три одинаковы — это правило
/// языка, а не упрощение. Само число записывает [`integer`]: «1 000 счетов»,
/// а не «1000 счетов».
pub const fn plural(lang: Lang, n: u64, one: Key, few: Key, many: Key) -> &'static str {
    let key = match plural_form(lang, n) {
        Plural::One => one,
        Plural::Few => few,
        Plural::Many => many,
    };
    t(lang, key)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Пустая подпись в окне выглядит поломкой, а не пропущенным переводом,
    /// и найти её можно только глазами и только на том языке, где её забыли.
    #[test]
    fn every_key_says_something_in_every_language() {
        let mut empty = Vec::new();
        for (key, name) in ALL_KEYS {
            for lang in Lang::ALL {
                if t(lang, *key).trim().is_empty() {
                    empty.push(format!("{name} ({lang:?})"));
                }
            }
        }
        assert!(empty.is_empty(), "пустые строки: {empty:?}");
    }

    /// Ни в одной строке нет дыры из нескольких пробелов подряд и пробела по
    /// краям — признака потерянного переноса строки в исходнике (опыт Savio).
    #[test]
    fn no_string_has_a_gap_in_it() {
        let mut bad = Vec::new();
        for (key, name) in ALL_KEYS {
            for lang in Lang::ALL {
                let text = t(lang, *key);
                if text.contains("  ") || text.starts_with(' ') || text.ends_with(' ') {
                    bad.push(format!("{name} ({lang:?}): «{text}»"));
                }
            }
        }
        assert!(bad.is_empty(), "дыры из пробелов: {bad:#?}");
    }

    /// Тире не начинает строку: перед ним неразрывный пробел (U+00A0).
    /// С обычным пробелом перенос оставляет «— при его закрытии» в начале
    /// строки — на телефоне это видно сразу (глазной прогон задачи 6).
    #[test]
    fn a_dash_never_starts_a_line() {
        let mut wrong = Vec::new();
        for (key, name) in ALL_KEYS {
            for lang in Lang::ALL {
                let text = t(lang, *key);
                if text.contains(" —") || text.starts_with('—') {
                    wrong.push(format!("{name} ({lang:?}): {text}"));
                }
            }
        }
        assert!(wrong.is_empty(), "тире после обычного пробела: {wrong:#?}");
    }

    /// Кавычки принадлежат языку, а не строке (Правило 4): «ёлочки» —
    /// в русском и армянском, “лапки” — в английском. Перевод пишут
    /// копированием русской строки, и «ёлочки» уезжают в английский текст.
    #[test]
    fn each_language_uses_its_own_quotation_marks() {
        let mut wrong = Vec::new();
        for (key, name) in ALL_KEYS {
            let en = t(Lang::En, *key);
            if en.contains(['«', '»']) {
                wrong.push(format!("{name} (En): {en}"));
            }
            for lang in [Lang::Ru, Lang::Am] {
                let text = t(lang, *key);
                if text.contains(['“', '”']) {
                    wrong.push(format!("{name} ({lang:?}): {text}"));
                }
            }
        }
        assert!(wrong.is_empty(), "кавычки не того языка: {wrong:#?}");
    }

    /// Тот же путь «скопировали русскую строку и не перевели целиком»:
    /// кириллица в английском или армянском переводе выглядит в окне как
    /// «эту почему-то не перевели» (опыт Arpa).
    #[test]
    fn no_russian_left_in_other_languages() {
        let cyrillic = |c: char| ('\u{400}'..='\u{4FF}').contains(&c);
        let mut wrong = Vec::new();
        for (key, name) in ALL_KEYS {
            for lang in [Lang::En, Lang::Am] {
                let text = t(lang, *key);
                if text.chars().any(cyrillic) {
                    wrong.push(format!("{name} ({lang:?}): {text}"));
                }
            }
        }
        assert!(wrong.is_empty(), "кириллица в переводе: {wrong:#?}");
    }

    /// У шаблона одинаковое число мест во всех трёх языках: потерянное `{}` —
    /// молча пропавшее число, лишнее — видимые «{}» в окне.
    #[test]
    fn a_template_keeps_its_placeholders_in_every_language() {
        let mut wrong = Vec::new();
        for (key, name) in ALL_KEYS {
            let places = |lang| t(lang, *key).matches("{}").count();
            let ru = places(Lang::Ru);
            for lang in [Lang::En, Lang::Am] {
                if places(lang) != ru {
                    wrong.push(format!(
                        "{name} ({lang:?}): мест {}, в русском {ru}",
                        places(lang)
                    ));
                }
            }
        }
        assert!(wrong.is_empty(), "места в шаблонах разошлись: {wrong:#?}");
    }

    /// Код армянского — `hy`, а не `am` (амхарский), и каждый код читается
    /// обратно в тот же язык: иначе запомненный выбор не переживёт
    /// перезапуска.
    #[test]
    fn codes_are_iso_and_round_trip() {
        let codes: Vec<&str> = Lang::ALL.iter().map(|lang| lang.code()).collect();
        assert_eq!(codes, ["hy", "ru", "en"]);

        let mut lost = Vec::new();
        for lang in Lang::ALL {
            if Lang::from_code(lang.code()) != Some(lang) {
                lost.push(lang);
            }
        }
        assert!(lost.is_empty(), "код не читается обратно: {lost:?}");
        assert_eq!(Lang::from_code("am"), None);
    }

    #[test]
    fn fill_puts_values_where_the_template_asks() {
        assert_eq!(fill("{} из {}", &["5", "10"]), "5 из 10");
        assert_eq!(fill("без мест", &["5"]), "без мест");
        assert_eq!(fill("{} и {}", &["раз"]), "раз и {}");
        assert_eq!(fill("{}", &["раз", "два"]), "раз");
        assert_eq!(fill("Քայլ {}՝ {}-ից", &["2", "3"]), "Քայլ 2՝ 3-ից");
    }

    /// Формы выписаны человеком по грамматике, а не той же формулой. Русские
    /// числа — те, на которых правило ошибается проще всего: 11–14 и 111–114
    /// исключение, а 21, 101 и 1001 — снова «один».
    #[test]
    fn plural_follows_each_language_rules() {
        use Plural::{Few, Many, One};
        let mut cases = vec![
            (Lang::Ru, 0, Many),
            (Lang::Ru, 1, One),
            (Lang::Ru, 2, Few),
            (Lang::Ru, 4, Few),
            (Lang::Ru, 5, Many),
            (Lang::Ru, 10, Many),
            (Lang::Ru, 11, Many),
            (Lang::Ru, 12, Many),
            (Lang::Ru, 14, Many),
            (Lang::Ru, 15, Many),
            (Lang::Ru, 21, One),
            (Lang::Ru, 22, Few),
            (Lang::Ru, 25, Many),
            (Lang::Ru, 101, One),
            (Lang::Ru, 111, Many),
            (Lang::Ru, 112, Many),
            (Lang::Ru, 122, Few),
            (Lang::Ru, 1_001, One),
            (Lang::Ru, 1_011, Many),
            (Lang::En, 0, Many),
            (Lang::En, 1, One),
            (Lang::En, 2, Many),
            (Lang::En, 21, Many),
            (Lang::En, 101, Many),
        ];
        for n in [0, 1, 2, 5, 11, 21, 1_000] {
            cases.push((Lang::Am, n, One));
        }
        let mut wrong = Vec::new();
        for (lang, n, expected) in cases {
            let got = plural_form(lang, n);
            if got != expected {
                wrong.push(format!("{lang:?} {n}: {got:?}, а не {expected:?}"));
            }
        }
        assert!(wrong.is_empty(), "не та форма слова: {wrong:#?}");
    }
}
