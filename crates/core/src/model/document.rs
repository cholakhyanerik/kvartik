//! Счёт и акт сдачи-приёмки (задача 11): строки, номера, выпуск,
//! аннулирование и оплата.
//!
//! **Черновик и выпущенный документ.** Черновик правится как угодно
//! и удаляется меткой. Выпуск даёт документу номер и копию реквизитов ИП
//! и клиента на этот день: правка профиля или клиента выпущенный документ
//! не меняет. Выпущенный документ не правится и не удаляется — только
//! аннулируется, и его номер второй раз не выдаётся (Правило 2).
//!
//! **Номер** — порядковый в серии «вид документа и год даты документа»:
//! `2026-001`. Следующий — на единицу больше самого большого выданного
//! в серии, считая аннулированные: записи не стираются, и номер, однажды
//! выданный, остаётся занятым. Поэтому, пока часть записей документов не
//! читается, выпуск отказывает: номер нечитаемой записи мог бы повториться.
//! Два устройства выдадут одинаковые номера — задача 41.
//!
//! **Две даты.** Дата документа — день выписки, по её году — серия номера.
//! Дата оказания услуги (или этапа) — по ней доход попадает в квартал,
//! а не по дню оплаты (НК, ст. 15, ч. 1, ст. 40, ст. 257, ч. 2; прочтение
//! агента 2026-10-10, ответ бухгалтера 2026-10-19 — FEATURE_TASKS.md,
//! «PDF против текста закона», вопросы 2.1–2.7). Оплата — деньги на
//! счёте, а не доход для налога (задача 14).
//!
//! **Деньги** — целые младшие единицы валюты документа (Правило 2): цена
//! строки и сумма оплаты лежат числом, валюта — одна на документ, и строки
//! в чужой валюте не бывает. Количество — в тысячных: 37,5 часа — 37 500.
//!
//! **Курс** документа в валюте — курс ЦБ, который человек указал для дня
//! оказания услуги (задача 14): по нему доход документа ложится в книгу
//! в драмах. Неизвестный курс — `None`, а не ноль (Правило 6). В PDF курса
//! нет, поэтому у выпущенного документа его можно указать и поправить
//! ([`Document::set_rate`]): текст документа от этого не меняется.
//!
//! **Подпись акта** (задача 62) — день, когда клиент подписал акт и вернул
//! экземпляр ИП ([`Document::sign`]): пока подпись не вернулась, услуга не
//! принята. Налога она не меняет: доход по акту — по дате оказания услуги
//! (Правило 2), а не по дню подписи.
//!
//! **Вид деятельности** (задача 19) — код из профиля, по которому получен
//! доход: от него зависит строка налога, когда коды ИП облагаются по-разному.
//! В PDF его нет, и у выпущенного документа его можно указать
//! ([`Document::set_activity`]), как курс.
//!
//! **Строка налога дохода** (задача 70) — если она не та, что у кода: аренда,
//! проценты, роялти или продажа имущества. Важнее вида деятельности
//! (`tax::turnover`), в PDF её тоже нет, а выпущенному документу её указывает
//! [`Document::set_income_kind`]. Акт по счёту и повтор счёта берут её вместе
//! с остальными данными: месячный счёт за аренду остаётся арендой. Счёт и акт
//! по нему — одна услуга: выпуск акта берёт строку счёта, указанную, пока акт
//! лежал черновиком ([`Document::issue`]), а строка, указанная у акта, идёт
//! и в счёт: иначе повтор счёта и новый акт после аннулирования снова вышли бы
//! без неё (окно, `ui/documents.rs`).
//!
//! **Расчётный документ в системе ՊԵԿ** (задача 23) — «հաշիվ վավերագիր»,
//! который ИП выписывает там сам: счёт программы его не заменяет.
//! Программа его не выписывает и никуда не отправляет, а помнит отметку
//! человека — день и номер ([`Document::set_settlement`]); по каким
//! документам её нет и до какого дня, говорит `tax::settlement`. Отметка —
//! о поставке, а акт по счёту — та же поставка: выпуск акта берёт отметку
//! счёта-основания ([`Document::issue`]). У документа, выпущенного до
//! 0.0.55, отметить было негде, и выписан ли документ, программа не знает:
//! это «нет значения», а не «не выписан» (Правило 6,
//! [`Document::settlement_known`]).
//!
//! Меняется документ только командами ядра (CLAUDE.md, «Данные меняются
//! одним путём»): [`Document::create`], `edit`, `issue`, `annul`,
//! `unannul`, `delete`, `restore`, `add_payment`, `remove_payment`,
//! `set_rate`, `set_activity`, `set_income_kind`, `sign`, `unsign`,
//! `set_settlement`, `remove_settlement`.
//!
//! В данных: `{"meta":{…},"data":{…},"issued":{…},"annulled":null,
//! "signed":null,"settlement":null,"payments":[…]}` — у документа,
//! выпущенного до 0.0.55, ключа `settlement` нет; образец — в тесте
//! `data_shape_is_pinned`.

use std::fmt;

use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize};

use super::activity::{ActivityCode, IncomeKind};
use super::bank::BankAccount;
use super::client::{Client, ClientData};
use super::date::Date;
use super::money::{
    Currency, ExchangeRate, Lumas, Money, MoneyError, RateError, RateSource,
    div_round_half_away_from_zero,
};
use super::profile::{Email, Localized, ProfileData};
use super::record::{RecordId, RecordMeta, Timestamp};
use super::text::{Line, Lines, fold};
use super::tin::Tin;
use crate::format::Stored;
use crate::i18n::Lang;

/// Вид документа.
///
/// Расширяется, а смысл существующих вариантов не меняется (Правило 2);
/// `match` — без `_`. В данных — `"invoice"`, `"act"`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocKind {
    /// Счёт — просьба об оплате.
    Invoice,
    /// Акт сдачи-приёмки (հանձնման-ընդունման ակտ) — услуга оказана
    /// и принята.
    Act,
}

/// Номер документа: год серии и порядковый номер в ней, с единицы.
///
/// Человеку — `2026-001` (`Display`): год, дефис и номер не короче трёх
/// цифр. Это номер, а не текст для перевода: на всех языках он пишется
/// одинаково. В данных — `{"year":2026,"seq":1}`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize)]
pub struct Number {
    year: u16,
    seq: u32,
}

/// Поля номера, как они лежат в данных, — до проверки.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NumberFields {
    year: u16,
    seq: u32,
}

impl Number {
    /// Год серии — год даты документа.
    pub const fn year(self) -> u16 {
        self.year
    }

    /// Порядковый номер в серии.
    pub const fn seq(self) -> u32 {
        self.seq
    }
}

impl fmt::Display for Number {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}-{:03}", self.year, self.seq)
    }
}

/// Номер читается, только если такой бывает: номера — с единицы, год —
/// в пределах календаря.
impl<'de> Deserialize<'de> for Number {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let NumberFields { year, seq } = NumberFields::deserialize(deserializer)?;
        if seq == 0 || !(Date::MIN_YEAR..=Date::MAX_YEAR).contains(&year) {
            return Err(de::Error::custom("номер с нуля или вне календаря"));
        }
        Ok(Self { year, seq })
    }
}

/// Знаков после запятой в количестве: оно хранится в тысячных.
pub const QUANTITY_DIGITS: u32 = 3;

/// Единица количества в тысячных.
pub const QUANTITY_ONE: i64 = 1_000;

/// Строка документа: услуга, количество и цена за единицу.
///
/// Что годится, проверяет [`DocumentData::check`]: строки читаются вместе
/// с документом. В данных — `{"text":"…","quantity_e3":1000,"price":500000}`.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Item {
    /// Услуга так, как она стоит в документе. Не пустая.
    pub text: Line,
    /// Количество в тысячных: 1 — 1 000, 37,5 часа — 37 500. Больше нуля.
    pub quantity_e3: i64,
    /// Цена за единицу — в младших единицах валюты документа. Не меньше
    /// нуля.
    pub price: i64,
}

impl Item {
    /// Сумма строки — цена, умноженная на количество, до младшей единицы
    /// валюты. Округление — то же, что у перевода в драмы
    /// (`div_round_half_away_from_zero`): 0,333 часа по 100,00 $ — 33,30 $.
    pub fn total(&self, currency: Currency) -> Result<Money, MoneyError> {
        // В `i128` произведение двух `i64` не переполняется.
        let exact = i128::from(self.price) * i128::from(self.quantity_e3);
        let minor = div_round_half_away_from_zero(exact, i128::from(QUANTITY_ONE));
        i64::try_from(minor)
            .map(|minor| Money::new(minor, currency))
            .map_err(|_| MoneyError::Overflow)
    }
}

/// Документ, как его заполнил человек, — то, что меняет форма черновика.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize)]
pub struct DocumentData {
    pub kind: DocKind,
    /// Клиент — его запись. Пока документ черновик, реквизиты клиента
    /// берутся из записи; при выпуске документ копирует их в себя.
    pub client: RecordId,
    /// Язык документа — по умолчанию язык документов клиента. Не язык окна
    /// (Архитектура): ИП с русским окном выписывает счёт по-английски.
    pub lang: Lang,
    /// Валюта документа: в ней цены строк и суммы оплаты.
    pub currency: Currency,
    /// Дата документа — день выписки; её год — серия номера.
    pub date: Date,
    /// Дата оказания услуги или этапа — по ней доход попадает в квартал
    /// (шапка модуля). У подписки на программу — последний день месяца.
    pub service_date: Date,
    /// Срок оплаты — у счёта; `None` — не указан. Не раньше даты документа.
    pub due: Option<Date>,
    /// Строки — от одной до [`DocumentData::MAX_ITEMS`].
    pub items: Vec<Item>,
    /// Основание акта — счёт, из которого он сделан; у счёта — `None`.
    pub basis: Option<RecordId>,
    /// Повтор, черновиком которого появился счёт (`model::recurring`); у акта
    /// и у счёта, выписанного руками, — `None`.
    pub recurring: Option<RecordId>,
    /// Курс валюты документа к драму — для книги (задача 14); у документа
    /// в драмах и пока курс не указан — `None`. Дата курса — день оказания
    /// услуги: курс, который человек указал для него ([`Self::rate_for`]).
    pub rate: Option<ExchangeRate>,
    /// Вид деятельности ИП, по которому получен доход, — код из профиля
    /// (задача 19). Нужен, когда коды профиля облагаются по разным строкам
    /// налога; `None` — не указан, и строку берут коды профиля
    /// (`tax::turnover`). В PDF его нет, как и курса.
    pub activity: Option<ActivityCode>,
    /// Строка налога дохода, если она не та, что у кода
    /// ([`IncomeKind::OF_INCOME`]); `None` — по коду. В PDF её нет.
    pub income_kind: Option<IncomeKind>,
}

/// Поля документа, как они лежат в данных, — до проверки.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DocumentDataFields {
    kind: DocKind,
    client: RecordId,
    lang: Lang,
    currency: Currency,
    date: Date,
    service_date: Date,
    due: Option<Date>,
    items: Vec<Item>,
    basis: Option<RecordId>,
    recurring: Option<RecordId>,
    /// С версии формата 6 (0.0.27): у документа прошлых версий курса нет.
    #[serde(default)]
    rate: Option<ExchangeRate>,
    /// С версии формата 8 (задача 19): у документа прошлых версий вид
    /// деятельности не указан.
    #[serde(default)]
    activity: Option<ActivityCode>,
    /// С версии формата 13 (задача 70): у документа прошлых версий строка —
    /// по коду.
    #[serde(default)]
    income_kind: Option<IncomeKind>,
}

/// Почему документ или команда над ним не приняты.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum DocError {
    /// Строк нет: документ ни о чём.
    NoItems,
    /// Строк больше [`DocumentData::MAX_ITEMS`]. Форма до предела не
    /// пускает — это порча.
    TooManyItems,
    /// В строке не названа услуга.
    NoText,
    /// Количество не больше нуля.
    Quantity,
    /// Цена меньше нуля.
    Price,
    /// Срок оплаты раньше даты документа.
    DueBeforeDate,
    /// Поле не того вида документа: срок оплаты или повтор у акта,
    /// основание у счёта; строка налога, которую у дохода не выбирают.
    WrongKind,
    /// Сумма документа не помещается в `i64` — только от испорченных
    /// данных или нелепых чисел.
    TooLarge,
    /// Документ выпущен: править и удалять его нельзя — только
    /// аннулировать.
    Issued,
    /// Документ — черновик: аннулировать, отмечать оплату и делать акт
    /// можно только у выпущенного.
    NotIssued,
    /// Документ аннулирован.
    Annulled,
    /// Черновик удалён: выпускать нечего.
    Deleted,
    /// Оплата, акт и повтор — только у счёта.
    NotInvoice,
    /// Подпись клиента — только у акта.
    NotAct,
    /// День подписи раньше даты акта: подписать акт до того, как его
    /// выписали, нельзя.
    SignedBeforeDate,
    /// Сумма оплаты не больше нуля.
    PaymentAmount,
    /// Оплат больше [`Document::MAX_PAYMENTS`].
    TooManyPayments,
    /// Профиля нет или в нём нет имени ИП на языке документа: выпускать
    /// документ без продавца нельзя.
    NoSellerName,
    /// Клиент документа удалён: восстановить его или выбрать другого.
    ClientDeleted,
    /// Передан не клиент документа — ошибка оболочки, а не человека.
    WrongClient,
    /// Часть записей документов не читается, и номер нечитаемой мог бы
    /// повториться: сначала восстановить их из копии.
    NumbersUnknown,
    /// Номера серии кончились (`u32::MAX`) — только от испорченных данных.
    NumbersExhausted,
    /// Курс не годится: не больше нуля, у документа в драмах или другой
    /// валюты, чем документ.
    Rate,
}

impl DocumentData {
    /// Строк — с запасом: у счёта ИП их обычно одна-три.
    pub const MAX_ITEMS: usize = 100;

    /// Годны ли данные: строки есть и в пределах, у каждой — услуга,
    /// количество больше нуля и цена не меньше нуля, срок оплаты не раньше
    /// даты, поля — своего вида документа, сумма помещается.
    pub fn check(&self) -> Result<(), DocError> {
        if self.items.is_empty() {
            return Err(DocError::NoItems);
        }
        if self.items.len() > Self::MAX_ITEMS {
            return Err(DocError::TooManyItems);
        }
        for item in &self.items {
            if item.text.is_empty() {
                return Err(DocError::NoText);
            }
            if item.quantity_e3 <= 0 {
                return Err(DocError::Quantity);
            }
            if item.price < 0 {
                return Err(DocError::Price);
            }
        }
        match self.kind {
            DocKind::Invoice if self.basis.is_some() => return Err(DocError::WrongKind),
            DocKind::Invoice if self.due.is_some_and(|due| due < self.date) => {
                return Err(DocError::DueBeforeDate);
            }
            DocKind::Act if self.due.is_some() || self.recurring.is_some() => {
                return Err(DocError::WrongKind);
            }
            DocKind::Invoice | DocKind::Act => {}
        }
        if self.rate.is_some_and(|rate| rate.currency() != self.currency) {
            return Err(DocError::Rate);
        }
        if self.income_kind.is_some_and(|kind| !IncomeKind::OF_INCOME.contains(&kind)) {
            return Err(DocError::WrongKind);
        }
        self.total().map(|_| ()).map_err(|_| DocError::TooLarge)
    }

    /// Итог документа — сумма строк (`Money::total`): что человек сложит по
    /// строкам на калькуляторе, то и будет в итоге.
    pub fn total(&self) -> Result<Money, MoneyError> {
        let lines = self
            .items
            .iter()
            .map(|item| item.total(self.currency))
            .collect::<Result<Vec<_>, _>>()?;
        Money::total(self.currency, lines)
    }

    /// Итог в драмах с лумами — по курсу документа (`Money::to_lumas`):
    /// документ — одна поставка и один курс. Валюта без курса —
    /// `RateUnknown`, а не ноль.
    pub fn total_lumas(&self) -> Result<Lumas, MoneyError> {
        self.total()?.to_lumas(self.rate)
    }

    /// Курс валюты документа для дня оказания услуги: `per_unit_e4` — драмы
    /// за единицу валюты в десятитысячных (`RATE_SCALE`), как их прочитал
    /// `i18n::parse_rate`. Курс ввёл человек (`RateSource::Manual`); его дата
    /// — день оказания услуги, к которому он относится. Какой курс ЦБ брать
    /// для этого дня, говорит закон (НК 16), а окно — подсказкой.
    pub fn rate_for(&self, per_unit_e4: i64) -> Result<ExchangeRate, DocError> {
        ExchangeRate::new(self.currency, per_unit_e4, self.service_date, RateSource::Manual).map_err(
            |error| match error {
                RateError::NotPositive | RateError::Amd => DocError::Rate,
            },
        )
    }
}

/// Читается только то, что пропустил бы [`DocumentData::check`]:
/// программа другого не пишет, и в данных это порча.
impl<'de> Deserialize<'de> for DocumentData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let DocumentDataFields {
            kind,
            client,
            lang,
            currency,
            date,
            service_date,
            due,
            items,
            basis,
            recurring,
            rate,
            activity,
            income_kind,
        } = DocumentDataFields::deserialize(deserializer)?;
        let data = Self {
            kind,
            client,
            lang,
            currency,
            date,
            service_date,
            due,
            items,
            basis,
            recurring,
            rate,
            activity,
            income_kind,
        };
        data.check()
            .map_err(|error| de::Error::custom(format!("документ не годится: {error:?}")))?;
        Ok(data)
    }
}

/// Реквизиты ИП в выпущенном документе — копия профиля на день выпуска.
/// Имя и адрес — на всех трёх языках, как в профиле: копия не теряет
/// ничего, что могло бы понадобиться документу.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Seller {
    pub name: Localized<Line>,
    pub tin: Option<Tin>,
    pub registration: Line,
    pub address: Localized<Lines>,
    pub phone: Line,
    pub email: Option<Email>,
    /// Банковские счета ИП — все, какие были в профиле. PDF показывает счета
    /// в валюте документа, а нет таких — все (`doc::sheet`, решение человека
    /// 2026-10-14).
    pub accounts: Vec<BankAccount>,
}

impl Seller {
    /// Копия реквизитов профиля. Логотипа в ней нет: он — отдельная запись
    /// в сотни килобайт, и копировать его в каждый документ — раздувать базу
    /// и копию. PDF старого документа берёт нынешний логотип профиля (решение
    /// человека 2026-10-14; задача 63).
    pub fn from_profile(profile: &ProfileData) -> Self {
        Self {
            name: profile.name.clone(),
            tin: profile.tin.clone(),
            registration: profile.registration.clone(),
            address: profile.address.clone(),
            phone: profile.phone.clone(),
            email: profile.email.clone(),
            accounts: profile.accounts.clone(),
        }
    }
}

/// Выпуск: номер и реквизиты сторон на день выпуска.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Issued {
    number: Number,
    seller: Seller,
    /// Карточка клиента на день выпуска — целиком: с ней проще сверить
    /// документ с клиентом, а лишние поля (валюта и язык по умолчанию) —
    /// несколько байтов.
    buyer: ClientData,
}

impl Issued {
    /// Номер документа.
    pub const fn number(&self) -> Number {
        self.number
    }

    /// Реквизиты ИП на день выпуска.
    pub const fn seller(&self) -> &Seller {
        &self.seller
    }

    /// Реквизиты клиента на день выпуска.
    pub const fn buyer(&self) -> &ClientData {
        &self.buyer
    }
}

/// Поступление оплаты по счёту: день и сумма в валюте счёта. Это деньги,
/// а не доход для налога (шапка модуля).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Payment {
    /// День поступления.
    pub date: Date,
    /// Сумма — в младших единицах валюты счёта, больше нуля.
    pub amount: i64,
}

/// Расчётный документ в системе ՊԵԿ, который ИП выписал по этой поставке
/// сам (задача 23): день и номер — как их показала система. Номер
/// человек может не записать: отметка «выписан» важнее, а номер нужен, чтобы
/// сверить его с системой.
///
/// В данных — `{"day":"2026-09-30","number":"…"}`.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settlement {
    /// День, которым он выписан в системе ՊԵԿ.
    pub day: Date,
    /// Номер в системе ՊԵԿ; пустой — не записан.
    pub number: Line,
}

/// Где документ: черновик, выпущен, оплачен, аннулирован. Окно говорит
/// его словом, а цвет только помогает (Правило 4).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Status {
    /// Черновик: номера ещё нет.
    Draft,
    /// Выпущенный акт, подписи клиента ещё нет.
    Issued,
    /// Акт подписан клиентом (задача 62).
    Signed,
    /// Счёт выпущен, оплат нет, срок не прошёл.
    Unpaid,
    /// Счёт оплачен не целиком, срок не прошёл.
    PartlyPaid,
    /// Счёт оплачен целиком.
    Paid,
    /// Срок оплаты прошёл, а счёт оплачен не целиком.
    Overdue,
    /// Аннулирован.
    Annulled,
}

/// Счёт или акт — запись.
#[derive(Clone, PartialEq, Eq, Debug, Serialize)]
pub struct Document {
    meta: RecordMeta,
    data: DocumentData,
    /// Выпуск; `None` — черновик.
    issued: Option<Issued>,
    /// День, когда документ аннулирован; `None` — действует. Только
    /// у выпущенного.
    annulled: Option<Date>,
    /// День, когда клиент подписал акт; `None` — подписи нет. Только
    /// у выпущенного акта; аннулирование её не снимает: подписанный акт
    /// аннулируют, а не «отписывают».
    signed: Option<Date>,
    /// Расчётный документ в системе ՊԵԿ (задача 23): `Some(Some(…))` —
    /// отмечен, `Some(None)` — не отмечен, `None` — неизвестно: документ
    /// выпущен до 0.0.55, когда отмечать было негде. Отмечен — только
    /// выпущенный; аннулирование отметку не снимает: документ в системе
    /// ՊԵԿ от этого никуда не делся. Неизвестное в данных — нет ключа, как
    /// у записей прежних версий, и правка документа его не теряет.
    #[serde(skip_serializing_if = "Option::is_none")]
    settlement: Option<Option<Settlement>>,
    /// Поступления оплаты — только у выпущенного счёта, по дням.
    payments: Vec<Payment>,
}

/// Ключ есть — `Some` с его значением, и с `null` тоже; ключа нет — `None`
/// (`#[serde(default)]`). Так «не отмечен» отличается от «неизвестно».
fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(deserializer: D) -> Result<Option<T>, D::Error> {
    T::deserialize(deserializer).map(Some)
}

/// Поля документа, как они лежат в данных, — до проверки.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DocumentFields {
    meta: RecordMeta,
    data: DocumentData,
    issued: Option<Issued>,
    annulled: Option<Date>,
    /// С версии формата 7 (задача 62); у записей прежних — подписи нет.
    #[serde(default)]
    signed: Option<Date>,
    /// С версии формата 10 (задача 23). Ключа нет — неизвестно, отмечен ли
    /// (записи прежних версий и документы, выпущенные до 0.0.55); `null` —
    /// не отмечен.
    #[serde(default, deserialize_with = "present")]
    settlement: Option<Option<Settlement>>,
    payments: Vec<Payment>,
}

impl Stored for Document {
    const KIND: &'static str = "document";

    fn meta(&self) -> &RecordMeta {
        &self.meta
    }
}

/// Читается только то, что пишут команды: номер — того года, что дата
/// документа; аннулирован, оплачен и отмечен выписанным в системе ՊԵԿ —
/// только выпущенный, оплачен — только счёт, подписан — только акт и не
/// раньше своей даты; выпущенный не удалён; в копии реквизитов есть имя на
/// языке документа; списки — в пределах (Правило 13).
impl<'de> Deserialize<'de> for Document {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let DocumentFields {
            meta,
            data,
            issued,
            annulled,
            signed,
            settlement,
            payments,
        } = DocumentFields::deserialize(deserializer)?;
        let fits = match &issued {
            None => {
                annulled.is_none()
                    && signed.is_none()
                    && settlement.as_ref().and_then(Option::as_ref).is_none()
                    && payments.is_empty()
            }
            Some(issued) => {
                issued.number.year == data.date.year()
                    && !meta.is_deleted()
                    && !issued.seller.name.get(data.lang).is_empty()
                    && issued.seller.accounts.len() <= ProfileData::MAX_ACCOUNTS
                    && (payments.is_empty() || data.kind == DocKind::Invoice)
                    && signed.is_none_or(|day| data.kind == DocKind::Act && day >= data.date)
            }
        };
        let payments_fit = payments.len() <= Document::MAX_PAYMENTS
            && payments.iter().all(|payment| payment.amount > 0)
            && payments.windows(2).all(|pair| pair[0].date <= pair[1].date);
        if !fits || !payments_fit {
            return Err(de::Error::custom(
                "состояние документа, которого команды не пишут",
            ));
        }
        let document = Self {
            meta,
            data,
            issued,
            annulled,
            signed,
            settlement,
            payments,
        };
        // Сумма оплат помещается, как и сумма строк.
        document
            .paid()
            .map_err(|_| de::Error::custom("сумма оплат не помещается"))?;
        Ok(document)
    }
}

impl Document {
    /// Оплат — с запасом: счёт обычно оплачивают одним-двумя платежами.
    pub const MAX_PAYMENTS: usize = 100;

    /// Команда «новый черновик».
    pub fn create(data: DocumentData, now: Timestamp) -> Result<Self, DocError> {
        data.check()?;
        Ok(Self {
            meta: RecordMeta::new(now),
            data,
            issued: None,
            annulled: None,
            signed: None,
            settlement: Some(None),
            payments: Vec::new(),
        })
    }

    /// Команда «изменить черновик» — прочитанный в той же транзакции.
    /// `Ok(None)` — данные не изменились, и записывать нечего.
    pub fn edit(&self, data: DocumentData, now: Timestamp) -> Result<Option<Self>, DocError> {
        self.must_be_draft()?;
        data.check()?;
        if self.data == data {
            return Ok(None);
        }
        let mut document = self.clone();
        document.meta.touch(now);
        document.data = data;
        Ok(Some(document))
    }

    /// Команда «выпустить»: следующий номер серии и копия реквизитов ИП
    /// и клиента на этот день; у акта по счёту — ещё и отметка о расчётном
    /// документе у счёта, какая она есть: поставка та же (шапка модуля).
    /// Выпущенный без основания — не отмечен.
    ///
    /// `documents` — все документы, прочитанные в той же транзакции,
    /// включая удалённые и аннулированные; `unreadable` — сколько записей
    /// документов не прочиталось: с ними номер мог бы повториться, и выпуск
    /// отказывает. `profile` — профиль, с которым работает окно; `client` —
    /// запись клиента документа.
    pub fn issue(
        &self,
        documents: &[Document],
        unreadable: usize,
        profile: Option<&ProfileData>,
        client: &Client,
        now: Timestamp,
    ) -> Result<Self, DocError> {
        self.must_be_draft()?;
        if self.meta.is_deleted() {
            return Err(DocError::Deleted);
        }
        self.data.check()?;
        if client.id() != self.data.client {
            return Err(DocError::WrongClient);
        }
        if client.is_deleted() {
            return Err(DocError::ClientDeleted);
        }
        let profile = profile
            .filter(|profile| !profile.name.get(self.data.lang).is_empty())
            .ok_or(DocError::NoSellerName)?;
        if unreadable > 0 {
            return Err(DocError::NumbersUnknown);
        }
        let number = Self::next_number(documents, self.data.kind, self.data.date.year())
            .ok_or(DocError::NumbersExhausted)?;
        let mut document = self.clone();
        document.meta.touch(now);
        document.issued = Some(Issued {
            number,
            seller: Seller::from_profile(profile),
            buyer: client.data().clone(),
        });
        let basis = self
            .data
            .basis
            .and_then(|basis| documents.iter().find(|known| known.id() == basis));
        // Без этого акт, ставший документом дохода вместо отмеченного счёта,
        // снова ждал бы расчётного документа, уже выписанного, а акт по
        // счёту, выпущенному до 0.0.55, — того, о чём программа не знает.
        document.settlement = basis.map_or(Some(None), |invoice| invoice.settlement.clone());
        // Строку указали у счёта, пока акт лежал черновиком: доход теперь по
        // акту, и она осталась бы у счёта, где ни на что не влияет. «По коду»,
        // выбранное в самом черновике, от незаполненного не отличить; его
        // вернёт карточка акта.
        if let Some(invoice) = basis {
            document.data.income_kind = document.data.income_kind.or(invoice.data.income_kind);
        }
        Ok(document)
    }

    /// Следующий номер серии `kind` и `year` — на единицу больше самого
    /// большого выданного, считая аннулированные. `None` — номера кончились.
    pub fn next_number(documents: &[Document], kind: DocKind, year: u16) -> Option<Number> {
        let last = documents
            .iter()
            .filter(|document| document.data.kind == kind)
            .filter_map(Document::number)
            .filter(|number| number.year == year)
            .map(Number::seq)
            .max()
            .unwrap_or(0);
        last.checked_add(1).map(|seq| Number { year, seq })
    }

    /// Команда «аннулировать» выпущенный документ в день `today`. Номер
    /// остаётся занятым. `Ok(None)` — уже аннулирован.
    pub fn annul(&self, today: Date, now: Timestamp) -> Result<Option<Self>, DocError> {
        self.must_be_issued()?;
        if self.annulled.is_some() {
            return Ok(None);
        }
        let mut document = self.clone();
        document.meta.touch(now);
        document.annulled = Some(today);
        Ok(Some(document))
    }

    /// Команда «вернуть аннулированный» — сразу после ошибочного
    /// аннулирования. `None` — документ не аннулирован.
    pub fn unannul(&self, now: Timestamp) -> Option<Self> {
        self.annulled?;
        let mut document = self.clone();
        document.meta.touch(now);
        document.annulled = None;
        Some(document)
    }

    /// Команда «удалить черновик»: метка, а не стирание. Выпущенный
    /// документ не удаляется — только аннулируется. `Ok(None)` — уже удалён.
    pub fn delete(&self, now: Timestamp) -> Result<Option<Self>, DocError> {
        self.must_be_draft()?;
        if self.meta.is_deleted() {
            return Ok(None);
        }
        let mut document = self.clone();
        document.meta.delete(now);
        Ok(Some(document))
    }

    /// Команда «восстановить удалённый черновик». `None` — не удалён.
    pub fn restore(&self, now: Timestamp) -> Option<Self> {
        if !self.meta.is_deleted() {
            return None;
        }
        let mut document = self.clone();
        document.meta.restore(now);
        Some(document)
    }

    /// Команда «отметить оплату» выпущенного действующего счёта. Оплаты
    /// лежат по дням: та же сумма в тот же день — вторая оплата, а не
    /// повтор.
    pub fn add_payment(&self, payment: Payment, now: Timestamp) -> Result<Self, DocError> {
        self.must_be_invoice()?;
        if self.annulled.is_some() {
            return Err(DocError::Annulled);
        }
        if payment.amount <= 0 {
            return Err(DocError::PaymentAmount);
        }
        if self.payments.len() >= Self::MAX_PAYMENTS {
            return Err(DocError::TooManyPayments);
        }
        let mut document = self.clone();
        // После оплат того же дня — по порядку, в каком их отметили.
        let at = document
            .payments
            .partition_point(|known| known.date <= payment.date);
        document.payments.insert(at, payment);
        document
            .paid()
            .map_err(|_| DocError::TooLarge)?;
        document.meta.touch(now);
        Ok(document)
    }

    /// Команда «убрать оплату» — отмеченную по ошибке. `None` — такой
    /// оплаты нет: её уже убрали.
    pub fn remove_payment(&self, payment: Payment, now: Timestamp) -> Option<Self> {
        let at = self.payments.iter().position(|known| *known == payment)?;
        let mut document = self.clone();
        document.payments.remove(at);
        document.meta.touch(now);
        Some(document)
    }

    /// Команда «указать курс» выпущенного действующего документа в валюте:
    /// `per_unit_e4` — как у [`DocumentData::rate_for`], `None` — убрать
    /// курс. Курса в тексте документа и в PDF нет, поэтому выпущенный
    /// документ от этого не меняется — меняется его строка в книге. Черновик
    /// получает курс из своей формы (`edit`). `Ok(None)` — курс тот же.
    pub fn set_rate(&self, per_unit_e4: Option<i64>, now: Timestamp) -> Result<Option<Self>, DocError> {
        self.must_be_issued()?;
        if self.annulled.is_some() {
            return Err(DocError::Annulled);
        }
        let rate = per_unit_e4.map(|e4| self.data.rate_for(e4)).transpose()?;
        if rate == self.data.rate {
            return Ok(None);
        }
        let mut document = self.clone();
        document.data.rate = rate;
        document.meta.touch(now);
        Ok(Some(document))
    }

    /// Команда «указать вид деятельности» выпущенного действующего
    /// документа: код из профиля, `None` — убрать. Вида в тексте документа
    /// и в PDF нет, поэтому выпущенный документ от этого не меняется —
    /// меняется строка налога, по которой идёт его доход (`tax::turnover`).
    /// Черновик получает вид из своей формы (`edit`). `Ok(None)` — вид тот же.
    pub fn set_activity(
        &self,
        activity: Option<ActivityCode>,
        now: Timestamp,
    ) -> Result<Option<Self>, DocError> {
        self.must_be_issued()?;
        if self.annulled.is_some() {
            return Err(DocError::Annulled);
        }
        if activity == self.data.activity {
            return Ok(None);
        }
        let mut document = self.clone();
        document.data.activity = activity;
        document.meta.touch(now);
        Ok(Some(document))
    }

    /// Команда «указать строку налога дохода» выпущенного действующего
    /// документа: одна из [`IncomeKind::OF_INCOME`], `None` — по коду. Как
    /// и вид деятельности, в тексте документа и в PDF её нет: меняется только
    /// строка налога его дохода. Черновик получает её из своей формы (`edit`).
    /// `Ok(None)` — строка та же.
    pub fn set_income_kind(
        &self,
        income_kind: Option<IncomeKind>,
        now: Timestamp,
    ) -> Result<Option<Self>, DocError> {
        self.must_be_issued()?;
        if self.annulled.is_some() {
            return Err(DocError::Annulled);
        }
        if income_kind == self.data.income_kind {
            return Ok(None);
        }
        let mut document = self.clone();
        document.data.income_kind = income_kind;
        document.data.check()?;
        document.meta.touch(now);
        Ok(Some(document))
    }

    /// Команда «акт подписан клиентом» в день `day` — у выпущенного
    /// действующего акта. Другой день — поправка прежней отметки.
    /// `Ok(None)` — отмечен этим же днём.
    pub fn sign(&self, day: Date, now: Timestamp) -> Result<Option<Self>, DocError> {
        self.must_be_issued()?;
        if self.data.kind != DocKind::Act {
            return Err(DocError::NotAct);
        }
        if self.annulled.is_some() {
            return Err(DocError::Annulled);
        }
        if day < self.data.date {
            return Err(DocError::SignedBeforeDate);
        }
        if self.signed == Some(day) {
            return Ok(None);
        }
        let mut document = self.clone();
        document.signed = Some(day);
        document.meta.touch(now);
        Ok(Some(document))
    }

    /// Команда «снять отметку о подписи» — поставленную по ошибке. `None` —
    /// отметки нет.
    pub fn unsign(&self, now: Timestamp) -> Option<Self> {
        self.signed?;
        let mut document = self.clone();
        document.signed = None;
        document.meta.touch(now);
        Some(document)
    }

    /// Команда «расчётный документ выписан в системе ՊԵԿ» — у выпущенного
    /// действующего документа (задача 23), и у выпущенного до 0.0.55 тоже.
    /// Другой день или номер — поправка прежней отметки. Текст документа
    /// и PDF от неё не меняются. `Ok(None)` — отмечен так же.
    pub fn set_settlement(&self, settlement: Settlement, now: Timestamp) -> Result<Option<Self>, DocError> {
        self.must_be_issued()?;
        if self.annulled.is_some() {
            return Err(DocError::Annulled);
        }
        if self.settlement() == Some(&settlement) {
            return Ok(None);
        }
        let mut document = self.clone();
        document.settlement = Some(Some(settlement));
        document.meta.touch(now);
        Ok(Some(document))
    }

    /// Команда «снять отметку о расчётном документе» — поставленную по
    /// ошибке: документ становится неотмеченным. `None` — отметки нет.
    pub fn remove_settlement(&self, now: Timestamp) -> Option<Self> {
        self.settlement()?;
        let mut document = self.clone();
        document.settlement = Some(None);
        document.meta.touch(now);
        Some(document)
    }

    /// Данные акта по этому счёту — «акт из счёта одним нажатием»: тот же
    /// клиент, язык, валюта, строки, дата оказания услуги и курс для неё,
    /// дата акта — `today`, основание — этот счёт.
    pub fn act_data(&self, today: Date) -> Result<DocumentData, DocError> {
        self.must_be_invoice()?;
        if self.annulled.is_some() {
            return Err(DocError::Annulled);
        }
        Ok(DocumentData {
            kind: DocKind::Act,
            date: today,
            due: None,
            basis: Some(self.id()),
            recurring: None,
            ..self.data.clone()
        })
    }

    /// Данные `n`-го черновика повтора `recurring` — копия счёта на `n`
    /// месяцев позже (`model::recurring`): даты сдвинуты
    /// (`Date::add_months`), срок оплаты — тот же промежуток от даты счёта.
    /// Курса нет: он был для прежнего дня оказания услуги, а у нового —
    /// свой. `None` — календарь кончился.
    pub(crate) fn repeat_data(&self, n: u32, recurring: RecordId) -> Option<DocumentData> {
        let date = self.data.date.add_months(n)?;
        let due = match self.data.due {
            Some(due) => Some(date.add_days(due.days_since(self.data.date))?),
            None => None,
        };
        Some(DocumentData {
            date,
            service_date: self.data.service_date.add_months(n)?,
            due,
            recurring: Some(recurring),
            rate: None,
            ..self.data.clone()
        })
    }

    /// Можно ли повторять этот документ: выпущенный действующий счёт.
    pub fn repeatable(&self) -> bool {
        self.must_be_invoice().is_ok() && self.annulled.is_none() && !self.meta.is_deleted()
    }

    /// Черновик ли — иначе `Issued`.
    fn must_be_draft(&self) -> Result<(), DocError> {
        match self.issued {
            Some(_) => Err(DocError::Issued),
            None => Ok(()),
        }
    }

    /// Выпущен ли — иначе `NotIssued`.
    fn must_be_issued(&self) -> Result<&Issued, DocError> {
        self.issued.as_ref().ok_or(DocError::NotIssued)
    }

    /// Выпущенный ли счёт.
    fn must_be_invoice(&self) -> Result<&Issued, DocError> {
        let issued = self.must_be_issued()?;
        match self.data.kind {
            DocKind::Invoice => Ok(issued),
            DocKind::Act => Err(DocError::NotInvoice),
        }
    }

    /// Постоянный идентификатор.
    pub const fn id(&self) -> RecordId {
        self.meta.id()
    }

    /// Данные документа.
    pub const fn data(&self) -> &DocumentData {
        &self.data
    }

    /// Выпуск; `None` — черновик.
    pub const fn issued(&self) -> Option<&Issued> {
        self.issued.as_ref()
    }

    /// Номер; `None` — черновик.
    pub fn number(&self) -> Option<Number> {
        self.issued.as_ref().map(Issued::number)
    }

    /// День аннулирования; `None` — действует.
    pub const fn annulled(&self) -> Option<Date> {
        self.annulled
    }

    /// День подписи акта клиентом; `None` — подписи нет.
    pub const fn signed(&self) -> Option<Date> {
        self.signed
    }

    /// Расчётный документ в системе ՊԵԿ; `None` — не отмечен или неизвестно
    /// ([`Self::settlement_known`]).
    pub fn settlement(&self) -> Option<&Settlement> {
        self.settlement.as_ref().and_then(Option::as_ref)
    }

    /// Знает ли программа, выписан ли расчётный документ: у документа,
    /// выпущенного до 0.0.55, отметить его было негде — не знает, и такой
    /// документ не числится невыписанным (Правило 6).
    pub const fn settlement_known(&self) -> bool {
        self.settlement.is_some()
    }

    /// Поступления оплаты — по дням.
    pub fn payments(&self) -> &[Payment] {
        &self.payments
    }

    /// Удалён ли черновик.
    pub const fn is_deleted(&self) -> bool {
        self.meta.is_deleted()
    }

    /// Итог документа.
    pub fn total(&self) -> Result<Money, MoneyError> {
        self.data.total()
    }

    /// Сколько оплачено — сумма поступлений.
    pub fn paid(&self) -> Result<Money, MoneyError> {
        let currency = self.data.currency;
        Money::total(
            currency,
            self.payments
                .iter()
                .map(|payment| Money::new(payment.amount, currency)),
        )
    }

    /// Сколько осталось оплатить: итог минус поступления, а у оплаченного
    /// целиком — ноль.
    pub fn left(&self) -> Result<Money, MoneyError> {
        let (total, paid) = (self.total()?, self.paid()?);
        Ok(Money::new(
            (total.minor() - paid.minor()).max(0),
            self.data.currency,
        ))
    }

    /// Где документ на день `today`: просрочен счёт со сроком раньше
    /// сегодняшнего, оплаченный не целиком.
    pub fn status(&self, today: Date) -> Status {
        if self.issued.is_none() {
            return Status::Draft;
        }
        if self.annulled.is_some() {
            return Status::Annulled;
        }
        match self.data.kind {
            DocKind::Act if self.signed.is_some() => Status::Signed,
            DocKind::Act => Status::Issued,
            DocKind::Invoice => {
                // Суммы помещаются: это проверено при записи и при чтении.
                let total = self.total().map_or(0, Money::minor);
                let paid = self.paid().map_or(0, Money::minor);
                if paid >= total {
                    Status::Paid
                } else if self.data.due.is_some_and(|due| due < today) {
                    Status::Overdue
                } else if paid > 0 {
                    Status::PartlyPaid
                } else {
                    Status::Unpaid
                }
            }
        }
    }

    /// Живые документы для списка: черновики первыми — их ещё выпускать, —
    /// потом выпущенные, свежие сверху: по дате, при одной дате — по
    /// номеру.
    pub fn listed(documents: &[Document]) -> Vec<&Document> {
        let mut listed: Vec<&Document> = documents.iter().filter(|d| !d.is_deleted()).collect();
        listed.sort_by(|a, b| {
            (a.issued.is_some(), b.data.date, b.number(), b.id()).cmp(&(
                b.issued.is_some(),
                a.data.date,
                a.number(),
                a.id(),
            ))
        });
        listed
    }

    /// Живые документы по дате — для списка, разбитого по месяцам: свежие
    /// сверху; в один день черновик — выше выпущенных (его ещё выпускать),
    /// выпущенные — по номеру, больший выше.
    pub fn by_date(documents: &[Document]) -> Vec<&Document> {
        let mut listed: Vec<&Document> = documents.iter().filter(|d| !d.is_deleted()).collect();
        listed.sort_by(|a, b| {
            (b.data.date, a.issued.is_some(), b.number(), b.id()).cmp(&(
                a.data.date,
                b.issued.is_some(),
                a.number(),
                a.id(),
            ))
        });
        listed
    }

    /// Подходит ли документ к поиску `query`: часть номера (`2026-014`) или
    /// названия клиента `client`, без учёта регистра, «ё» и «և» (`fold`);
    /// пустой запрос — подходит любой. Название клиента — от окна: у
    /// черновика оно ещё в записи клиента, а не в документе.
    pub fn matches(&self, query: &str, client: &str) -> bool {
        let query = fold(query);
        if query.is_empty() {
            return true;
        }
        let number = self.number().map(|number| number.to_string()).unwrap_or_default();
        fold(client).contains(&query) || number.contains(&query)
    }

    /// Счета, которые на день `today` ждут оплаты, — для главного экрана:
    /// выпущенные, действующие и оплаченные не целиком. По сроку: чей срок
    /// раньше, тот выше — просроченные поэтому первыми; без срока — в конце,
    /// по дате счёта.
    pub fn awaiting(documents: &[Document], today: Date) -> Vec<&Document> {
        let mut awaiting: Vec<&Document> = documents
            .iter()
            .filter(|document| !document.is_deleted())
            .filter(|document| {
                matches!(
                    document.status(today),
                    Status::Unpaid | Status::PartlyPaid | Status::Overdue
                )
            })
            .collect();
        awaiting.sort_by_key(|document| {
            (
                document.data.due.is_none(),
                document.data.due,
                document.data.date,
                document.number(),
                document.id(),
            )
        });
        awaiting
    }

    /// Удалённые черновики — последний удалённый первым.
    pub fn deleted(documents: &[Document]) -> Vec<&Document> {
        let mut deleted: Vec<&Document> = documents.iter().filter(|d| d.is_deleted()).collect();
        deleted.sort_by_key(|document| std::cmp::Reverse((document.meta.deleted(), document.id())));
        deleted
    }

    /// Сколько живых документов у клиента — их число окно называет, когда
    /// клиента удаляют: документы остаются.
    pub fn count_for_client(documents: &[Document], client: RecordId) -> usize {
        documents
            .iter()
            .filter(|document| !document.is_deleted() && document.data.client == client)
            .count()
    }
}

#[cfg(test)]
mod tests;
