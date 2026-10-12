//! Домен: деньги, курсы, календарные даты, общие поля записей, профиль ИП
//! и клиенты.
//!
//! Чистые преобразования без браузера, хранилища и часов: «сегодня»
//! и «сейчас» приходят параметром (таблица слоёв CLAUDE.md). Документы
//! и книга появятся здесь вместе со своими задачами (11–14).

pub mod activity;
pub mod bank;
pub mod client;
pub mod country;
pub mod date;
pub mod logo;
pub mod money;
pub mod profile;
pub mod record;
pub mod text;
pub mod tin;
