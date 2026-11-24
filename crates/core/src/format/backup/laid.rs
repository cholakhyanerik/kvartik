//! Какие части одной копии уже легли на этом устройстве (задача 123): запись
//! устройства, а не данных — её держит `localStorage` окна, в копию она не
//! идёт. Без неё части, легшие в разные заходы или после перезагрузки, никогда
//! не давали «восстановлена целиком», а слова просили восстановить остальные,
//! когда их не осталось (найдено человеком 2026-11-24, 0.0.182).
//!
//! Запись — одна, последней копии частями: части другой копии её заменяют.
//! Ключа в ней нет — ключ части окно держит только в памяти страницы, пока
//! открыт экран копии (`ui/backup/restore.rs`). А «программа была пуста» —
//! с первой части, легшей или легшей частью: спросить, где выпускать дальше,
//! окно должно, когда легли все части, а к тому времени программа давно не
//! пуста, и страницу могли перезагрузить.
//!
//! Запись — память, а не данные: базу браузер может стереть, а `localStorage`
//! оставить (Правило 6), и номера из записи о ней тогда врали бы. Поэтому часть,
//! легшая в пустую программу, начинает счёт заново: в каждой части — профиль
//! ИП и список ИП, и до первой легшей части программа пуста не бывает (ревью
//! задачи 123).

use serde::{Deserialize, Serialize};

use super::{CopyId, MAX_PARTS, Part};

/// Части одной копии, легшие здесь, и была ли программа пуста до первой.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Laid {
    copy: CopyId,
    of: u16,
    /// Номера легших — по возрастанию, без повторов, каждый от единицы до
    /// `of`: больше [`MAX_PARTS`] их не бывает. Пусто — часть легла частью
    /// ([`Self::failed`]), а программа была пуста.
    numbers: Vec<u16>,
    fresh: bool,
}

/// Запись для хранилища устройства.
#[derive(Serialize, Deserialize)]
struct LaidFields {
    copy: CopyId,
    of: u16,
    laid: Vec<u16>,
    fresh: bool,
}

impl Laid {
    /// Легла часть `part`; `empty` — программа до неё была пуста. Из `known`
    /// берётся то, что помнилось, — запись устройства и память экрана: запись
    /// устройства могла не лечь, когда `localStorage` отказал, а память экрана
    /// пропадает с перезагрузкой. Номера — только той же копии с тем же
    /// числом частей (шапка — чужой ввод) и только когда программа не была
    /// пуста: в пустой прежних частей нет. «Пуста» — и от другой копии, если
    /// та легла не вся: вопрос, где выпускать, иначе не встал бы ни у одной
    /// из двух (ревью задачи 123).
    pub fn after(known: impl IntoIterator<Item = Self>, part: Part, empty: bool) -> Self {
        let mut laid = Self {
            copy: part.copy(),
            of: part.of(),
            numbers: vec![part.number()],
            fresh: empty,
        };
        for known in known {
            if known.copy == laid.copy && known.of == laid.of {
                if !empty {
                    laid.numbers.extend(known.numbers);
                    laid.fresh |= known.fresh;
                }
            } else if !known.whole() {
                laid.fresh |= known.fresh;
            }
        }
        laid.numbers.sort_unstable();
        laid.numbers.dedup();
        laid
    }

    /// Часть `part` легла частью — копия всех ИП, сбой на одной из баз, —
    /// а программа до неё была пуста: номеров нет, но «пуста» помнится, и
    /// вопрос, где выпускать, встанет, когда лягут все части, — и после
    /// перезагрузки страницы (ревью задачи 123).
    pub fn failed(part: Part) -> Self {
        Self {
            copy: part.copy(),
            of: part.of(),
            numbers: Vec::new(),
            fresh: true,
        }
    }

    /// Копия, чьи это части.
    pub const fn copy(&self) -> CopyId {
        self.copy
    }

    /// Сколько частей у копии.
    pub const fn of(&self) -> u16 {
        self.of
    }

    /// Легли ли все части.
    pub fn whole(&self) -> bool {
        self.numbers.len() == usize::from(self.of)
    }

    /// Номера частей, которые ещё не легли, по возрастанию.
    pub fn missing(&self) -> Vec<u16> {
        (1..=self.of).filter(|number| self.numbers.binary_search(number).is_err()).collect()
    }

    /// Спросить ли, где выпускать дальше: программа была пуста до первой части,
    /// а легли все — этой частью, а не раньше. `before` — что помнилось до
    /// неё. Уже целая копия, положенная снова, не спрашивает: «Выпускать
    /// здесь» включило бы выпуск у всех ИП, и у тех, чей выпуск бухгалтер
    /// с тех пор выключил (ревью задачи 123).
    pub fn asks(&self, before: &[Self]) -> bool {
        self.whole()
            && self.fresh
            && !before
                .iter()
                .any(|known| known.copy == self.copy && known.of == self.of && known.whole())
    }

    /// Запись для хранилища устройства.
    pub fn to_text(&self) -> String {
        serde_json::to_string(&LaidFields {
            copy: self.copy,
            of: self.of,
            laid: self.numbers.clone(),
            fresh: self.fresh,
        })
        .expect("в записи только строки, числа и да-нет")
    }

    /// Из записи хранилища устройства; `None` — запись не читается, и окно
    /// говорит только о частях, легших при открытом экране.
    pub fn from_text(text: &str) -> Option<Self> {
        let fields: LaidFields = serde_json::from_str(text).ok()?;
        let fits = (2..=MAX_PARTS).contains(&fields.of)
            && fields.laid.is_sorted_by(|a, b| a < b)
            && fields.laid.iter().all(|number| (1..=fields.of).contains(number));
        fits.then_some(Self {
            copy: fields.copy,
            of: fields.of,
            numbers: fields.laid,
            fresh: fields.fresh,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(copy: CopyId, number: u16, of: u16) -> Part {
        Part { copy, number, of }
    }

    /// Части одной копии, легшие в разные заходы, складываются: вторая из
    /// двух первой — осталась первая, потом первая — легли все, и только
    /// тогда — вопрос, где выпускать.
    #[test]
    fn parts_of_one_copy_add_up() {
        let copy = CopyId::random();
        let second = Laid::after(None, part(copy, 2, 2), true);
        assert!(!second.whole());
        assert_eq!(second.missing(), [1]);
        assert!(second.fresh);
        assert!(!second.asks(&[]));
        let first = Laid::after(Some(second.clone()), part(copy, 1, 2), false);
        assert!(first.whole());
        assert!(first.missing().is_empty());
        // Пустой программа была до первой легшей части, а не до последней.
        assert!(first.fresh);
        assert!(first.asks(&[second]));
    }

    /// Та же часть дважды — по-прежнему одна, а запись устройства и память
    /// экрана о той же копии складываются: одна из них могла не лечь.
    #[test]
    fn a_part_counts_once_and_both_memories_add_up() {
        let copy = CopyId::random();
        let device = Laid::after(None, part(copy, 1, 3), false);
        let screen = Laid::after(None, part(copy, 3, 3), false);
        let again = Laid::after([device.clone(), screen], part(copy, 1, 3), false);
        assert_eq!(again.missing(), [2]);
        assert_eq!(Laid::after(Some(device), part(copy, 1, 3), false).missing(), [2, 3]);
    }

    /// Целая копия, положенная снова, о выпуске не спрашивает: ответ уже был
    /// (ревью задачи 123).
    #[test]
    fn a_whole_copy_laid_again_does_not_ask() {
        let copy = CopyId::random();
        let one = Laid::after(None, part(copy, 1, 2), true);
        let whole = Laid::after(Some(one), part(copy, 2, 2), false);
        let again = Laid::after(Some(whole.clone()), part(copy, 1, 2), false);
        assert!(again.whole());
        assert!(!again.asks(&[whole]));
    }

    /// В пустую программу прежних частей не ложилось, что бы ни помнила
    /// запись: базу стёрли, а `localStorage` остался (ревью задачи 123).
    #[test]
    fn an_empty_program_starts_over() {
        let copy = CopyId::random();
        let mut whole = Laid::after(None, part(copy, 1, 3), true);
        for number in [2, 3] {
            whole = Laid::after(Some(whole), part(copy, number, 3), false);
        }
        assert!(whole.whole());
        let wiped = Laid::after(Some(whole), part(copy, 2, 3), true);
        assert_eq!(wiped.missing(), [1, 3]);
        assert!(wiped.fresh);
        let next = Laid::after(Some(wiped), part(copy, 1, 3), false);
        let last = Laid::after(Some(next.clone()), part(copy, 3, 3), false);
        assert!(last.asks(&[next]), "стёртая и восстановленная заново — снова новое устройство");
    }

    /// Часть, легшая частью в пустую программу, помнит «пуста» без номеров:
    /// после повтора — уже в непустую — и последней части вопрос встаёт.
    #[test]
    fn a_failed_part_keeps_the_question() {
        let copy = CopyId::random();
        let failed = Laid::failed(part(copy, 1, 2));
        assert_eq!(failed.missing(), [1, 2]);
        assert_eq!(Laid::from_text(&failed.to_text()), Some(failed.clone()));
        let retried = Laid::after(Some(failed.clone()), part(copy, 1, 2), false);
        assert_eq!(retried.missing(), [2]);
        let last = Laid::after(Some(retried.clone()), part(copy, 2, 2), false);
        assert!(last.asks(&[retried]));
    }

    /// Часть другой копии — новая запись: номера прежней не в счёт, и та же
    /// копия с другим числом частей — подделанная шапка — тоже. «Пуста»
    /// прежней, легшей не вся, переходит к новой: иначе вопрос не встал бы ни
    /// у одной; у целой — нет: её вопрос уже был.
    #[test]
    fn another_copy_starts_over() {
        let (one, other) = (CopyId::random(), CopyId::random());
        let known = Laid::after(None, part(one, 1, 2), true);
        let next = Laid::after(Some(known.clone()), part(other, 2, 2), false);
        assert_eq!(next.copy(), other);
        assert_eq!(next.missing(), [1]);
        assert!(next.fresh, "прежняя копия легла не вся, и вопроса ещё не было");
        let forged = Laid::after(Some(known.clone()), part(one, 2, 3), false);
        assert_eq!(forged.of(), 3);
        assert_eq!(forged.missing(), [1, 3]);
        let whole = Laid::after(Some(known), part(one, 2, 2), false);
        assert!(!Laid::after(Some(whole), part(other, 1, 2), false).fresh);
    }

    /// Запись устройства читается той, какой записана, а чужая или
    /// испорченная — «нет записи», а не паника: её мог переписать кто угодно.
    #[test]
    fn the_device_record_round_trips_and_rejects_nonsense() {
        let copy = CopyId::random();
        let laid = Laid::after(None, part(copy, 3, 5), true);
        let laid = Laid::after(Some(laid), part(copy, 1, 5), false);
        assert_eq!(Laid::from_text(&laid.to_text()), Some(laid));
        let id = serde_json::to_string(&copy).expect("строка");
        let mut wrong = Vec::new();
        for text in [
            String::new(),
            "{".to_owned(),
            format!(r#"{{"copy":{id},"of":1,"laid":[1],"fresh":false}}"#),
            format!(r#"{{"copy":{id},"of":1000,"laid":[1],"fresh":false}}"#),
            format!(r#"{{"copy":{id},"of":3,"laid":[0],"fresh":false}}"#),
            format!(r#"{{"copy":{id},"of":3,"laid":[4],"fresh":false}}"#),
            format!(r#"{{"copy":{id},"of":3,"laid":[2,1],"fresh":false}}"#),
            format!(r#"{{"copy":{id},"of":3,"laid":[1,1],"fresh":false}}"#),
            r#"{"copy":"не копия","of":3,"laid":[1],"fresh":false}"#.to_owned(),
        ] {
            if Laid::from_text(&text).is_some() {
                wrong.push(text);
            }
        }
        assert!(wrong.is_empty(), "прочитаны: {wrong:#?}");
    }
}
