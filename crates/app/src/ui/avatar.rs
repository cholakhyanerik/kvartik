//! Кружок с инициалами (дизайн «Kvartik Design») — у клиента в списке
//! и у ИП в профиле. Только украшает: имя написано рядом, поэтому диктору
//! кружок не читается (`aria-hidden`).

use kvartik_core::model::record::RecordId;
use leptos::prelude::*;

/// Инициалы: первые буквы двух первых слов, заглавными. Кавычки и знаки
/// вокруг слова не в счёт, слово без букв и цифр пропускается:
/// «Brightline LLC» — «BL», ««Արարատ» ՍՊԸ» — «ԱՍ».
pub fn initials(name: &str) -> String {
    name.split_whitespace()
        .filter_map(|word| word.chars().find(|c| c.is_alphanumeric()))
        .take(2)
        .flat_map(char::to_uppercase)
        .collect()
}

/// Оттенок кружка — один из четырёх, по идентификатору записи: у клиента
/// он один и тот же от запуска к запуску, а соседние клиенты обычно разных
/// цветов.
pub fn tone(id: RecordId) -> u8 {
    let sum = id
        .to_string()
        .bytes()
        .fold(0_u32, |sum, byte| sum.wrapping_mul(31).wrapping_add(u32::from(byte)));
    (sum % 4) as u8 + 1
}

/// Кружок с инициалами имени `name` оттенка `tone` (1–4).
#[component]
pub fn Avatar(name: String, tone: u8) -> impl IntoView {
    view! {
        <span class=format!("avatar tone-{tone}") aria-hidden="true">{initials(&name)}</span>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Инициалы — первые буквы двух первых слов, без кавычек и знаков; одно
    /// слово — одна буква; пустое имя — пусто.
    #[test]
    fn initials_take_first_letters_of_two_words() {
        let mut wrong = Vec::new();
        for (name, expected) in [
            ("Brightline LLC", "BL"),
            ("«Արարատ» ՍՊԸ", "ԱՍ"),
            ("ООО Семён и партнёры", "ОС"),
            ("  acme   inc. ", "AI"),
            ("Kestrel", "K"),
            ("— Studio", "S"),
            ("", ""),
        ] {
            if initials(name) != expected {
                wrong.push(format!("«{name}»: «{}», а не «{expected}»", initials(name)));
            }
        }
        assert!(wrong.is_empty(), "инициалы: {wrong:#?}");
    }

    /// Оттенок — от 1 до 4 и один и тот же у одного идентификатора.
    #[test]
    fn tone_is_stable_and_in_range() {
        for _ in 0..100 {
            let id = RecordId::random();
            let first = tone(id);
            assert!((1..=4).contains(&first));
            assert_eq!(tone(id), first);
        }
    }
}
