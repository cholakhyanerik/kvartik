//! Домен: деньги, курсы, календарные даты, общие поля записей, профиль ИП,
//! клиенты, счета и акты.
//!
//! Чистые преобразования без браузера, хранилища и часов: «сегодня»
//! и «сейчас» приходят параметром (таблица слоёв CLAUDE.md). Книга
//! появится здесь вместе со своей задачей (14).

pub mod activity;
pub mod bank;
pub mod client;
pub mod country;
pub mod date;
pub mod document;
pub mod logo;
pub mod money;
pub mod profile;
pub mod record;
pub mod recurring;
pub mod text;
pub mod tin;
