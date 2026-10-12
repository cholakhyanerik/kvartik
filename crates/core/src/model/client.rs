//! Клиент ИП (задача 10): кому выписываются счёт и акт.
//!
//! У клиента — название, страна и адрес для шапки документа, ՀՎՀՀ
//! армянской фирмы (он нужен в акте и в расчётном документе) или налоговый
//! номер фирмы другой страны и то, что документ этому клиенту берёт по
//! умолчанию: валюту, язык и срок оплаты.
//! Счёт и акт (задача 11) копируют их в себя, и правка клиента выпущенного
//! документа не меняет.
//!
//! Меняется клиент только командами — [`Client::create`], [`Client::edit`],
//! [`Client::delete`], [`Client::restore`] (CLAUDE.md, «Данные меняются
//! одним путём»). Удалённый клиент не стирается, а помечается удалённым,
//! и его можно восстановить (Правило 2).
//!
//! В данных: `{"meta":{…},"data":{…}}`, поля `data` — как у
//! [`ClientData`]; образец — в тесте `data_shape_is_pinned`.

use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize};

use super::country::Country;
use super::money::Currency;
use super::record::{RecordId, RecordMeta, Timestamp};
use super::text::{Line, Lines, fold};
use super::tin::Tin;
use crate::format::Stored;
use crate::i18n::Lang;

/// Клиент, как его заполнил человек, — то, что меняет форма. «Нет
/// значения» — `None`, а не значение по умолчанию (Правило 6): невыбранная
/// валюта — не драм, а вопрос в документе.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize)]
pub struct ClientData {
    /// Название так, как оно стоит в документе: «Acme Inc.»,
    /// «„Արարատ“ ՍՊԸ». Не пустое.
    pub name: Line,
    /// Страна; `None` — не выбрана.
    pub country: Option<Country>,
    /// Адрес для шапки документа.
    pub address: Lines,
    /// ՀՎՀՀ армянской фирмы; `None` — не указан.
    pub tin: Option<Tin>,
    /// Налоговый номер фирмы другой страны так, как его пишут там: VAT ID
    /// «DE123456789», ИНН российской фирмы. Пустой — не указан. Проверять
    /// его не по чему: у каждой страны свой вид. Его просят вписать в счёт
    /// сами клиенты — их бухгалтерии и банки (с 0.0.15).
    pub tax_id: Line,
    /// Валюта документов по умолчанию; `None` — не выбрана.
    pub currency: Option<Currency>,
    /// Язык документов по умолчанию; `None` — не выбран. В данных — код
    /// языка (`"hy"`).
    pub lang: Option<Lang>,
    /// Срок оплаты — дней от даты счёта, не больше
    /// [`ClientData::MAX_PAYMENT_DAYS`]; `None` — не указан.
    pub payment_days: Option<u16>,
}

/// Поля клиента, как они лежат в данных, — до проверки.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ClientDataFields {
    name: Line,
    country: Option<Country>,
    address: Lines,
    tin: Option<Tin>,
    /// Поле версии формата 4: у записей версии 3 его нет, и номер у них
    /// пустой.
    #[serde(default)]
    tax_id: Line,
    currency: Option<Currency>,
    lang: Option<Lang>,
    payment_days: Option<u16>,
}

/// Почему данные клиента не приняты.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ClientError {
    /// Название пустое: без него клиента не найти в списке и не назвать
    /// в документе.
    NoName,
    /// Срок оплаты — не целое число дней от 0 до
    /// [`ClientData::MAX_PAYMENT_DAYS`].
    PaymentDays,
}

impl ClientData {
    /// Самый долгий срок оплаты — год: дольше не ждут, а число больше —
    /// опечатка.
    pub const MAX_PAYMENT_DAYS: u16 = 365;

    /// Годны ли данные: название не пустое, срок оплаты в пределах.
    pub fn check(&self) -> Result<(), ClientError> {
        if self.name.is_empty() {
            return Err(ClientError::NoName);
        }
        if self.payment_days.is_some_and(|days| days > Self::MAX_PAYMENT_DAYS) {
            return Err(ClientError::PaymentDays);
        }
        Ok(())
    }

    /// Срок оплаты из поля ввода: целое число дней цифрами, пробелы по
    /// краям не важны. Пусто — `Ok(None)`.
    pub fn parse_payment_days(input: &str) -> Result<Option<u16>, ClientError> {
        let text = input.trim();
        if text.is_empty() {
            return Ok(None);
        }
        // Не длиннее трёх цифр: `u16` из «0000000000365» прочитался бы,
        // а такое человек не пишет.
        if text.len() > 3 || !text.bytes().all(|b| b.is_ascii_digit()) {
            return Err(ClientError::PaymentDays);
        }
        match text.parse::<u16>() {
            Ok(days) if days <= Self::MAX_PAYMENT_DAYS => Ok(Some(days)),
            _ => Err(ClientError::PaymentDays),
        }
    }
}

/// Читается только то, что пропустил бы [`ClientData::check`]: программа
/// пустого названия не пишет, и в данных это порча.
impl<'de> Deserialize<'de> for ClientData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let ClientDataFields {
            name,
            country,
            address,
            tin,
            tax_id,
            currency,
            lang,
            payment_days,
        } = ClientDataFields::deserialize(deserializer)?;
        let data = Self {
            name,
            country,
            address,
            tin,
            tax_id,
            currency,
            lang,
            payment_days,
        };
        data.check().map_err(|error| match error {
            ClientError::NoName => de::Error::custom("у клиента пустое название"),
            ClientError::PaymentDays => de::Error::custom("срок оплаты больше года"),
        })?;
        Ok(data)
    }
}

/// Клиент — запись.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Client {
    meta: RecordMeta,
    data: ClientData,
}

impl Stored for Client {
    const KIND: &'static str = "client";

    fn meta(&self) -> &RecordMeta {
        &self.meta
    }
}

impl Client {
    /// Команда «добавить клиента».
    pub fn create(data: ClientData, now: Timestamp) -> Result<Self, ClientError> {
        data.check()?;
        Ok(Self {
            meta: RecordMeta::new(now),
            data,
        })
    }

    /// Команда «изменить клиента» — прочитанного в той же транзакции.
    /// `Ok(None)` — данные не изменились, и записывать нечего: запись не
    /// становится «новее» без настоящей правки.
    pub fn edit(&self, data: ClientData, now: Timestamp) -> Result<Option<Self>, ClientError> {
        data.check()?;
        if self.data == data {
            return Ok(None);
        }
        let mut client = self.clone();
        client.meta.touch(now);
        client.data = data;
        Ok(Some(client))
    }

    /// Команда «удалить клиента»: метка, а не стирание. `None` — клиент уже
    /// удалён, записывать нечего.
    pub fn delete(&self, now: Timestamp) -> Option<Self> {
        if self.meta.is_deleted() {
            return None;
        }
        let mut client = self.clone();
        client.meta.delete(now);
        Some(client)
    }

    /// Команда «восстановить клиента». `None` — клиент не удалён.
    pub fn restore(&self, now: Timestamp) -> Option<Self> {
        if !self.meta.is_deleted() {
            return None;
        }
        let mut client = self.clone();
        client.meta.restore(now);
        Some(client)
    }

    /// Постоянный идентификатор.
    pub const fn id(&self) -> RecordId {
        self.meta.id()
    }

    /// Данные клиента.
    pub const fn data(&self) -> &ClientData {
        &self.data
    }

    /// Удалён ли клиент.
    pub const fn is_deleted(&self) -> bool {
        self.meta.is_deleted()
    }

    /// Живые клиенты, в названии которых есть `query`, — по алфавиту
    /// названий. Регистр, «ё» и армянская «և» не важны ([`fold`]); пустой
    /// запрос — все живые.
    pub fn find<'a>(clients: &'a [Client], query: &str) -> Vec<&'a Client> {
        let query = fold(query);
        let mut found: Vec<(String, &Client)> = clients
            .iter()
            .filter(|client| !client.is_deleted())
            .map(|client| (fold(client.data.name.as_str()), client))
            .filter(|(name, _)| name.contains(&query))
            .collect();
        // При одинаковых названиях порядок держит идентификатор: иначе два
        // «ООО Альфа» менялись бы местами от чтения к чтению.
        found.sort_by(|(a, x), (b, y)| a.cmp(b).then(x.id().cmp(&y.id())));
        found.into_iter().map(|(_, client)| client).collect()
    }

    /// Удалённые клиенты — последний удалённый первым: его чаще всего
    /// и восстанавливают.
    pub fn deleted(clients: &[Client]) -> Vec<&Client> {
        let mut deleted: Vec<&Client> = clients.iter().filter(|c| c.is_deleted()).collect();
        deleted.sort_by(|a, b| {
            (b.meta.deleted(), b.id()).cmp(&(a.meta.deleted(), a.id()))
        });
        deleted
    }
}

#[cfg(test)]
mod tests;
