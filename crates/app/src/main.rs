//! Окно Kvartik в браузере: точка входа wasm.
//!
//! Крейт окна — тонкая оболочка: считать налог и деньги, решать бизнес-правила
//! и держать текст литералом здесь нельзя (таблица слоёв CLAUDE.md).

mod platform;
mod store;
mod ui;

fn main() {
    // Первой строкой, до любого кода, который может упасть: иначе причина
    // паники пропадёт, в консоли останется одно `unreachable`, а человек
    // увидит застывшую страницу вместо слов (Правило 6).
    ui::panic::install();
    // Первый кадр из `index.html` уступает место окну.
    if let Some(boot) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.query_selector(".boot").ok().flatten())
    {
        boot.remove();
    }
    leptos::mount::mount_to_body(ui::App);
}
