//! Общие поля каждой записи: постоянный идентификатор, момент изменения
//! и метка удаления (CLAUDE.md, «Записи не стираются, а помечаются
//! удалёнными»).
//!
//! Синхронизации в первой версии нет (задача 41), но без этих полей
//! невозможны ни слияние копий, ни второе устройство: два устройства не
//! договорятся, что новее, а удалённое на одном воскреснет копией с другого.
//! Добавить поля потом — значит мигрировать данные всех пользователей.
//!
//! Меняет их только путь команд ядра (CLAUDE.md, «Данные меняются одним
//! путём»): создание — `RecordMeta::new`, правка — `touch`, удаление
//! и восстановление — `delete` и `restore`. Пока команд нет, эти методы
//! открыты всем; первая команда ядра сужает их до `pub(crate)`, чтобы окно
//! не могло поставить метку в обход команды.

use uuid::Uuid;

/// Постоянный идентификатор записи — случайный UUID версии 4.
///
/// Случайный, а не порядковый номер: две копии и два устройства (задача 41)
/// создают записи независимо, и номера по порядку у них совпали бы.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct RecordId(Uuid);

impl RecordId {
    /// Новый идентификатор.
    ///
    /// Случайность ядро берёт само, а не параметром, как «сегодня»: на wasm32
    /// она работает (источник — WebCrypto, его включает фича `js` ядра,
    /// Правило 3), тестам конкретное значение не нужно, а свежий
    /// идентификатор у каждой новой записи так гарантирует ядро — окно не
    /// выдаст один и тот же дважды.
    ///
    /// Паникует, если браузер не дал случайных чисел. Это не сбой посреди
    /// работы, а неподдерживаемая платформа: `crypto.getRandomValues` есть во
    /// всех браузерах с июля 2015 года, в воркерах и на страницах без HTTPS,
    /// а отказывает только на просьбе больше 65 536 байт — у нас их 16
    /// (по браузеру: MDN, «Crypto: getRandomValues()», сверено 2026-10-10).
    pub fn random() -> Self {
        Self(Uuid::new_v4())
    }
}

/// Момент времени: миллисекунды от 1970-01-01 00:00 UTC.
///
/// Только для служебных отметок — когда запись изменили и когда удалили.
/// Дата счёта и операции — календарная `Date`, а не момент (Правило 2):
/// момент по UTC уезжает на сутки. Здесь нужен именно момент: за день правок
/// бывает много, и при слиянии копий (задача 41) важно, какая из них позже.
/// Берёт его оболочка у часов устройства и передаёт параметром — ядро часов
/// не спрашивает (Архитектура, «Время — параметр»).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Timestamp(i64);

impl Timestamp {
    /// Момент по числу миллисекунд от 1970-01-01 00:00 UTC — в тех же
    /// единицах, что `js_sys::Date::now()`.
    pub const fn from_unix_millis(millis: i64) -> Self {
        Self(millis)
    }

    /// Миллисекунды от 1970-01-01 00:00 UTC.
    pub const fn unix_millis(self) -> i64 {
        self.0
    }
}

/// Общие поля записи.
///
/// Без `Copy`, хотя все поля копируются: у типа есть изменяющие методы,
/// и с `Copy` `запись.meta().delete(now)` молча удалил бы копию, а запись
/// осталась бы живой. Без него такой код не соберётся.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct RecordMeta {
    id: RecordId,
    modified: Timestamp,
    deleted: Option<Timestamp>,
}

impl RecordMeta {
    /// Поля новой записи: свежий идентификатор, изменена в `now`, не удалена.
    pub fn new(now: Timestamp) -> Self {
        Self {
            id: RecordId::random(),
            modified: now,
            deleted: None,
        }
    }

    /// Постоянный идентификатор.
    pub const fn id(&self) -> RecordId {
        self.id
    }

    /// Когда запись меняли в последний раз.
    pub const fn modified(&self) -> Timestamp {
        self.modified
    }

    /// Когда запись удалили; `None` — запись жива.
    pub const fn deleted(&self) -> Option<Timestamp> {
        self.deleted
    }

    /// Удалена ли запись.
    pub const fn is_deleted(&self) -> bool {
        self.deleted.is_some()
    }

    /// Запись изменилась в `now`.
    ///
    /// Момент изменения только растёт: если часы устройства перевели назад
    /// (Правило 6, «Часы устройства могут врать») или две правки пришлись на
    /// одну миллисекунду, новая всё равно позже прежней — иначе при слиянии
    /// копий (задача 41) победила бы старая. Расти некуда только у края
    /// `i64`, а туда попадает лишь испорченный файл копии: моменты за
    /// пределами дат JS (±8,64·10¹⁵ мс) разбор копии обязан отвергать
    /// (задача 7).
    pub fn touch(&mut self, now: Timestamp) {
        let after_previous = self.modified.0.saturating_add(1);
        self.modified = Timestamp(now.0.max(after_previous));
    }

    /// Пометить запись удалённой. Запись не стирается: её можно восстановить,
    /// а слияние копий узнаёт об удалении по метке.
    ///
    /// Удалённую запись не трогает: время удаления остаётся первым, а запись
    /// не становится «новее» без настоящей правки.
    pub fn delete(&mut self, now: Timestamp) {
        if self.deleted.is_none() {
            self.touch(now);
            self.deleted = Some(self.modified);
        }
    }

    /// Снять метку удаления. Живую запись не трогает.
    pub fn restore(&mut self, now: Timestamp) {
        if self.deleted.is_some() {
            self.touch(now);
            self.deleted = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn at(millis: i64) -> Timestamp {
        Timestamp::from_unix_millis(millis)
    }

    /// Новая запись жива, и момент изменения — момент создания.
    #[test]
    fn new_record_is_alive() {
        let meta = RecordMeta::new(at(1_000));
        assert_eq!(meta.modified(), at(1_000));
        assert_eq!(meta.deleted(), None);
        assert!(!meta.is_deleted());
    }

    /// Идентификатор — случайный UUID версии 4 по RFC 9562, и у каждой записи
    /// свой.
    #[test]
    fn ids_are_random_v4() {
        let ids: Vec<RecordId> = (0..1_000).map(|_| RecordId::random()).collect();
        let distinct: HashSet<RecordId> = ids.iter().copied().collect();
        assert_eq!(distinct.len(), ids.len(), "идентификаторы повторились");
        let mut wrong = Vec::new();
        for id in &ids {
            if id.0.get_version_num() != 4 || id.0.get_variant() != uuid::Variant::RFC4122 {
                wrong.push(format!("{id:?}"));
            }
        }
        assert!(wrong.is_empty(), "не UUID версии 4: {wrong:#?}");
    }

    /// Удаление ставит метку, восстановление снимает; оба — правки,
    /// а идентификатор остаётся прежним.
    #[test]
    fn delete_marks_and_restore_unmarks() {
        let mut meta = RecordMeta::new(at(1_000));
        let id = meta.id();

        meta.delete(at(2_000));
        assert_eq!(meta.deleted(), Some(at(2_000)));
        assert_eq!(meta.modified(), at(2_000));
        assert!(meta.is_deleted());

        meta.restore(at(3_000));
        assert_eq!(meta.deleted(), None);
        assert_eq!(meta.modified(), at(3_000));
        assert_eq!(meta.id(), id);
    }

    /// Повторное удаление и восстановление живой записи ничего не меняют:
    /// время удаления — первое, и запись не «новеет» без правки.
    #[test]
    fn delete_and_restore_are_idempotent() {
        let mut meta = RecordMeta::new(at(1_000));
        meta.restore(at(1_500));
        assert_eq!(meta.modified(), at(1_000), "живую запись восстановили");

        meta.delete(at(2_000));
        let deleted = meta.clone();
        meta.delete(at(9_000));
        assert_eq!(meta, deleted, "удалённую запись удалили второй раз");
    }

    /// Часы перевели назад или правки в одну миллисекунду — момент изменения
    /// всё равно растёт.
    #[test]
    fn modified_only_grows() {
        let mut meta = RecordMeta::new(at(5_000));
        meta.touch(at(1_000));
        assert_eq!(meta.modified(), at(5_001), "часы назад");
        meta.touch(at(5_001));
        assert_eq!(meta.modified(), at(5_002), "та же миллисекунда");
        meta.touch(at(9_000));
        assert_eq!(meta.modified(), at(9_000), "обычная правка");

        // Удаление после перевода часов назад тоже позже прежней правки.
        meta.delete(at(100));
        assert_eq!(meta.deleted(), Some(at(9_001)));
    }
}
