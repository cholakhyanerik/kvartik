//! Домен: деньги, курсы, календарные даты, общие поля записей, профиль ИП,
//! клиенты, счета и акты, книга доходов и расходов с вложениями.
//!
//! Чистые преобразования без браузера, хранилища и часов: «сегодня»
//! и «сейчас» приходят параметром (таблица слоёв CLAUDE.md).

pub mod activity;
pub mod attachment;
pub mod bank;
pub mod client;
pub mod country;
pub mod date;
pub mod document;
pub mod ledger;
pub mod logo;
pub mod money;
pub mod office;
pub mod profile;
pub mod record;
pub mod recurring;
pub mod text;
pub mod tin;
