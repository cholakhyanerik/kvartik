//! Языки и таблица строк.
//!
//! Модуль-лист: его зовут и домен, и документы, и окно, а сам он ни от кого
//! не зависит — ни от Leptos, ни от хранилища, ни от состояния экрана
//! (таблица слоёв CLAUDE.md).
//!
//! Язык **ездит параметром**, а не лежит глобалью: язык окна и язык
//! документа — разные вещи. ИП с русским окном выписывает счёт по-английски,
//! а акт местной фирме — по-армянски.
//!
//! Пока здесь только то, что нужно каркасу: языки и название программы.
//! Множественное число, запись сумм, чисел и дат и разбор введённой суммы
//! добавляет задача 4 реестра.

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

        /// Все ключи вместе с их именами — для тестов полноты таблицы.
        #[cfg(test)]
        const ALL_KEYS: &[(Key, &str)] = &[ $( (Key::$key, stringify!($key)), )* ];
    };
}

strings! {
    /// Название программы. Рабочее, до задачи 16 реестра (имя и домен решает
    /// человек): армянское — из описания проекта, русское и английское — его
    /// дословный перевод.
    AppName = "Офис ИП", "IE Office", "ԱՁ գրասենյակ";
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
}
