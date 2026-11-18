//! Что знает это устройство: когда здесь в последний раз сделали копию,
//! проверочное значение пароля копий (`DeviceKey`), выбранный язык окна,
//! день начала пробного периода (задача 25), что с новыми версиями: о какой
//! человек уже знает и какую окно уже пробовало включить, — скрыты ли
//! первые шаги на главном экране (задача 28) и что с автокопией (задача 29):
//! когда была последняя, когда в последний раз менялись данные, метка
//! устройства и когда вводили пароль, — настройки окна (задача 88): включён
//! ли кружок у курсора и свои цвета, — и срок ключа каждой базы с днём, о конце
//! которого окно уже сказало (задача 92), а о конце пробного периода и ключа
//! «Pro» — одним днём на устройство (задачи 93 и 38), — последние курсы ЦБ
//! с сайта (задача 43): копия файла сайта, а не данные, — и что сервер сайта
//! ответил о пробном периоде на ՀՎՀՀ (задача 106). Папку и ключ автокопии
//! `localStorage` не держит — это объекты браузера, они в своей базе
//! (`store/autobackup.rs`).
//!
//! Это не данные человека, а состояние устройства, поэтому оно лежит не
//! в базе записей и не попадает в копию: перенесённая на новое устройство
//! дата копии обещала бы копию, которой там не делали, а копия, открытая
//! бухгалтером, сменила бы ему язык окна, а пробный период идёт «с первого
//! запуска» на этом устройстве (`kvartik_core::license`). Место — `localStorage`: он
//! синхронный — язык окна читается до первой отрисовки, и окно не мигает
//! чужим языком, — и не требует новой версии базы, а стирает его браузер
//! вместе с базой (по браузеру: данные сайта, записанные скриптом,
//! стираются целиком — WebKit, «Updates to Storage Policy»; сверено
//! 2026-10-10, Правило 6). Транзакций ему не нужно: записи независимы,
//! и потерянная запись стоит одного лишнего напоминания, вопроса о пароле
//! или выбора языка.
//!
//! `localStorage` может отказать — сайтам запрещено хранить данные, место
//! кончилось: тогда чтение даёт «нет», а запись — `false`, и окно живёт
//! дальше без них.
//!
//! Две записи пишет не окно, а лендинг: калькулятор — что из введённого
//! в нём «Открыть Kvartik» взял с собой ([`Device::handoff`], задача 34),
//! а переключатель страниц — какой язык человек выбрал на сайте
//! ([`Device::site_lang`], 0.0.125). У лендинга и программы одно
//! происхождение, и записи видны окну без переноса. А одну запись окна
//! читает лендинг: включён ли кружок у курсора ([`Device::cursor`]) — у его
//! страниц кружок тот же.

use kvartik_core::format::autobackup::DeviceTag;
use kvartik_core::format::backup::DeviceKey;
use kvartik_core::i18n::Lang;
use kvartik_core::license::{self, Paid};
use kvartik_core::model::date::Date;
use kvartik_core::model::record::Timestamp;
use kvartik_core::release::Version;

/// Записи устройства под своим префиксом: программа — под [`Device::new`],
/// браузерные тесты — каждый под своим, чтобы не мешать друг другу.
pub struct Device {
    prefix: String,
}

/// Последний запрос окна к серверу о пробном периоде на ՀՎՀՀ (задача 106,
/// [`Device::trial_asked`]).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Asked {
    /// ՀՎՀՀ профиля — восемь цифр.
    pub tin: String,
    /// Когда спросило окно.
    pub at: Timestamp,
    /// Что ответил сервер: первый день пробного периода на этом ՀՎՀՀ; `None`
    /// — ещё не ответил.
    pub first: Option<Date>,
}

/// Имя записи калькулятора — `tax::example::HANDOFF_KEY` без префикса
/// [`Device::new`]: её пишет wasm калькулятора (`crates/calc`) под полным
/// именем из ядра, и тест ниже держит их вместе.
const HANDOFF: &str = "calc";

/// Имя записи кружка у курсора: его читает и лендинг (`site/landing.js`)
/// под полным именем, и тест ниже держит их вместе.
const CURSOR: &str = "cursor";

/// Имя записи языка, выбранного переключателем страниц сайта: её пишет
/// лендинг (`site/landing.js`) под полным именем, и тест ниже держит их
/// вместе.
const SITE_LANG: &str = "site_lang";

impl Device {
    /// Записи программы.
    pub fn new() -> Self {
        Self::with_prefix("kvartik.")
    }

    pub(super) fn with_prefix(prefix: &str) -> Self {
        Self {
            prefix: prefix.to_owned(),
        }
    }

    /// Когда на этом устройстве сделана последняя копия; `None` — не
    /// делалась или запись не читается.
    pub fn last_backup(&self) -> Option<Timestamp> {
        self.moment("last_backup")
    }

    /// Запомнить момент копии. `false` — `localStorage` отказал.
    pub fn set_last_backup(&self, at: Timestamp) -> bool {
        self.set("last_backup", &at.unix_millis().to_string())
    }

    /// Пароль копий этого устройства; `None` — не задан или запись не
    /// читается, и пароль зададут заново.
    pub fn backup_key(&self) -> Option<DeviceKey> {
        DeviceKey::from_text(&self.get("backup_key")?)
    }

    /// Запомнить пароль копий. `false` — `localStorage` отказал.
    pub fn set_backup_key(&self, key: &DeviceKey) -> bool {
        self.set("backup_key", &key.to_text())
    }

    /// Язык окна, который выбрал человек; `None` — не выбирал или запись
    /// не читается, и язык подскажет браузер.
    pub fn ui_lang(&self) -> Option<Lang> {
        Lang::from_code(&self.get("lang")?)
    }

    /// Запомнить язык окна. `false` — `localStorage` отказал.
    pub fn set_ui_lang(&self, lang: Lang) -> bool {
        self.set("lang", lang.code())
    }

    /// Язык, который человек выбрал переключателем на страницах сайта, а окно
    /// ещё не взяло (`ui/lang.rs`); `None` — не выбирал, окно его уже взяло
    /// или запись не читается. Пишет её лендинг: это договор между версиями,
    /// как запись калькулятора, — смысл не меняется, а незнакомый код языка —
    /// «нет записи».
    pub fn site_lang(&self) -> Option<Lang> {
        Lang::from_code(&self.get(SITE_LANG)?)
    }

    /// Забыть язык сайта: окно его взяло или человек выбрал язык в самом
    /// окне — тот новее. `false` — `localStorage` отказал.
    pub fn clear_site_lang(&self) -> bool {
        self.remove(SITE_LANG)
    }

    /// День, когда на этом устройстве начался пробный период; `None` —
    /// ещё не начинался или запись не читается.
    pub fn trial_start(&self) -> Option<Date> {
        Date::from_iso(&self.get("trial")?)
    }

    /// Запомнить день начала пробного периода. `false` — `localStorage`
    /// отказал.
    pub fn set_trial_start(&self, day: Date) -> bool {
        self.set("trial", &day.iso())
    }

    /// Первый день данных, открытых на этом устройстве (задача 99,
    /// `license::data_since`), когда он раньше первого запуска: пробный период
    /// — с него (условия, п. 3, редакция от 15.11.2026). Пишет его часть окна,
    /// проверяющая ключи, а читает главный wasm при запуске: метке срока и
    /// экрану лицензии не ждать части. Одна на устройство, как и день первого
    /// запуска ([`Self::trial_start`]): пробный период у всех ИП устройства
    /// общий. `None` — данные не раньше первого запуска или запись не
    /// читается.
    pub fn trial_data(&self) -> Option<Date> {
        Date::from_iso(&self.get("trial_data")?)
    }

    /// Запомнить первый день данных. `false` — `localStorage` отказал: метка
    /// при следующем запуске скажет срок по устройству, пока часть его не
    /// поправит.
    pub fn set_trial_data(&self, day: Date) -> bool {
        self.set("trial_data", &day.iso())
    }

    /// Версия, о которой человек уже знает: работала здесь первой или её
    /// «Что нового» он закрыл. `None` — записи нет или она не читается.
    pub fn seen_version(&self) -> Option<Version> {
        Version::parse(&self.get("seen_version")?)
    }

    /// Запомнить версию, о которой человек знает. `false` — `localStorage`
    /// отказал.
    pub fn set_seen_version(&self, version: Version) -> bool {
        self.set("seen_version", &version.to_string())
    }

    /// Версия, которую окно уже пробовало включить на этом устройстве
    /// (`ui/pwa.rs`); `None` — не пробовало или запись не читается.
    pub fn update_tried(&self) -> Option<Version> {
        Version::parse(&self.get("update_tried")?)
    }

    /// Запомнить попытку включить версию. `false` — `localStorage` отказал.
    pub fn set_update_tried(&self, version: Version) -> bool {
        self.set("update_tried", &version.to_string())
    }

    /// Скрыты ли первые шаги на главном экране у ИП базы `base`
    /// (`ui/onboarding.rs`): у каждого ИП свои, и новый ИП начинает с них
    /// (задача 38).
    pub fn steps_hidden(&self, base: &str) -> bool {
        self.get(&steps_name(base)).is_some()
    }

    /// Скрыть первые шаги у ИП базы `base`. `false` — `localStorage`
    /// отказал: при следующем открытии они появятся снова.
    pub fn set_steps_hidden(&self, base: &str) -> bool {
        self.set(&steps_name(base), "1")
    }

    /// Ключ базы `base`, каким его в прошлый раз нашла часть окна, проверив
    /// подписи (`license::remember`): `Some(None)` — ключа нет. `None` —
    /// записи нет или она не читается, и срок скажет часть. У каждого ИП
    /// свой ключ — на его ՀՎՀՀ.
    pub fn key_term(&self, base: &str) -> Option<Option<Paid>> {
        license::remembered(&self.get(&per_base("license", base))?)
    }

    /// Запомнить ключ базы `base`. `false` — `localStorage` отказал: при
    /// следующем открытии метка срока подождёт части.
    pub fn set_key_term(&self, base: &str, key: Option<Paid>) -> bool {
        self.set(&per_base("license", base), &license::remember(key))
    }

    /// Последний день ключа базы `base`, о конце которого окно уже сказало
    /// окном поверх программы; `None` — не говорило или запись не читается.
    pub fn key_told(&self, base: &str) -> Option<Date> {
        Date::from_iso(&self.get(&per_base("license_told", base))?)
    }

    /// Запомнить, что о конце ключа с последним днём `last` сказано.
    /// `false` — `localStorage` отказал: окно встанет при следующем запуске
    /// снова.
    pub fn set_key_told(&self, base: &str, last: Date) -> bool {
        self.set(&per_base("license_told", base), &last.iso())
    }

    /// Последний день пробного периода, о конце которого окно уже сказало
    /// окном поверх программы (задача 93); `None` — не говорило или запись не
    /// читается. Одна на устройство, а не на базу: пробный период у всех ИП
    /// устройства общий, как и день его начала ([`Self::trial_start`]).
    pub fn trial_told(&self) -> Option<Date> {
        Date::from_iso(&self.get("trial_told")?)
    }

    /// Запомнить, что о конце пробного периода с последним днём `last`
    /// сказано. `false` — `localStorage` отказал: окно встанет при следующем
    /// запуске снова.
    pub fn set_trial_told(&self, last: Date) -> bool {
        self.set("trial_told", &last.iso())
    }

    /// Последний день ключа «Pro», о конце которого окно уже сказало окном
    /// поверх программы (задача 38); `None` — не говорило или запись не
    /// читается. Одна на устройство, а не на базу: ключ «Pro» лежит в конторе
    /// и открывает ИП разом, и бухгалтер с тридцатью ИП не должен закрывать
    /// одно окно тридцать раз.
    pub fn pro_told(&self) -> Option<Date> {
        Date::from_iso(&self.get("pro_told")?)
    }

    /// Запомнить, что о конце ключа «Pro» с последним днём `last` сказано.
    /// `false` — `localStorage` отказал: окно встанет при следующем запуске
    /// снова.
    pub fn set_pro_told(&self, last: Date) -> bool {
        self.set("pro_told", &last.iso())
    }

    /// Когда последняя удавшаяся автокопия (задача 29) начала читать базу;
    /// `None` — не было или запись не читается.
    pub fn last_auto(&self) -> Option<Timestamp> {
        self.moment("last_auto")
    }

    /// Запомнить момент автокопии. `false` — `localStorage` отказал.
    pub fn set_last_auto(&self, at: Timestamp) -> bool {
        self.set("last_auto", &at.unix_millis().to_string())
    }

    /// Когда в базу в последний раз что-то записали — для автокопии: есть ли
    /// изменения, которых нет в её файле. Помнится между открытиями:
    /// закрытая до автокопии вкладка оставляет её на следующее открытие.
    pub fn changed(&self) -> Option<Timestamp> {
        self.moment("changed")
    }

    /// Запомнить момент записи. `false` — `localStorage` отказал.
    pub fn set_changed(&self, at: Timestamp) -> bool {
        self.set("changed", &at.unix_millis().to_string())
    }

    /// Метка этого устройства в именах файлов автокопии; `None` — ещё нет.
    pub fn auto_tag(&self) -> Option<DeviceTag> {
        DeviceTag::parse(&self.get("auto_tag")?)
    }

    /// Запомнить метку устройства. `false` — `localStorage` отказал: метка
    /// будет новой, и файлы прежней метки останутся в папке, как чужие.
    pub fn set_auto_tag(&self, tag: DeviceTag) -> bool {
        self.set("auto_tag", tag.as_str())
    }

    /// Когда человек в последний раз ввёл пароль копий этого устройства
    /// и тот подошёл; `None` — неизвестно.
    pub fn password_entered(&self) -> Option<Timestamp> {
        self.moment("password_entered")
    }

    /// Запомнить ввод пароля. `false` — `localStorage` отказал.
    pub fn set_password_entered(&self, at: Timestamp) -> bool {
        self.set("password_entered", &at.unix_millis().to_string())
    }

    /// Ведёт ли устройство больше одного ИП (задача 38): тогда окно при
    /// открытии читает список конторы (`store/office.rs`). Без метки базы
    /// конторы не открывают: `indexedDB.open` создал бы её пустой у каждого,
    /// кто ведёт одного ИП.
    pub fn office(&self) -> bool {
        self.get("office").is_some()
    }

    /// Запомнить, что ИП больше одного. `false` — `localStorage` отказал:
    /// окно откроет первого ИП, а список найдёт экран конторы.
    pub fn set_office(&self) -> bool {
        self.set("office", "1")
    }

    /// База ИП, открытая на устройстве последней, — для новой вкладки;
    /// `None` — не помнит.
    pub fn base(&self) -> Option<String> {
        self.get("base")
    }

    /// Запомнить базу, открытую последней. `false` — `localStorage` отказал.
    pub fn set_base(&self, name: &str) -> bool {
        self.set("base", name)
    }

    /// Что оставил программе калькулятор лендинга (задача 34) — текстом,
    /// как его записал wasm калькулятора; разбирает ядро
    /// (`tax::example::Handoff::parse`). `None` — ничего не оставлял.
    pub fn handoff(&self) -> Option<String> {
        self.get(HANDOFF)
    }

    /// Забыть переданное калькулятором: человек взял его в профиль или
    /// отказался. `false` — `localStorage` отказал, и предложение встанет
    /// при следующем открытии снова.
    pub fn clear_handoff(&self) -> bool {
        self.remove(HANDOFF)
    }

    /// Включён ли кружок у курсора (задача 88). По умолчанию — нет (решение
    /// человека 2026-11-09): запись есть, только пока его включили.
    pub fn cursor(&self) -> bool {
        self.get(CURSOR).is_some()
    }

    /// Включить или выключить кружок у курсора. `false` — `localStorage`
    /// отказал: при следующем открытии кружок будет прежним.
    pub fn set_cursor(&self, on: bool) -> bool {
        if on { self.set(CURSOR, "1") } else { self.remove(CURSOR) }
    }

    /// Свои цвета окна (задача 88) — текстом записи
    /// `kvartik_core::theme::Record`; `None` — не выбирали.
    pub fn colors(&self) -> Option<String> {
        self.get("colors")
    }

    /// Запомнить свои цвета. `false` — `localStorage` отказал: окно в них
    /// и сейчас, а при следующем открытии будет в прежних.
    pub fn set_colors(&self, record: &str) -> bool {
        self.set("colors", record)
    }

    /// Вернуть цвета программы. `false` — `localStorage` отказал.
    pub fn clear_colors(&self) -> bool {
        self.remove("colors")
    }

    /// Курсы ЦБ с сайта (задача 43) — текстом файла `rates.json`, каким он
    /// пришёл; разбирает ядро (`tax::cba::Rates::parse`). Это не данные
    /// человека, а копия файла сайта: без сети подсказка курса берёт её.
    /// `None` — курсов на устройстве нет.
    pub fn rates(&self) -> Option<String> {
        self.get("rates")
    }

    /// Запомнить курсы ЦБ. `false` — `localStorage` отказал: подсказка
    /// будет по прежним курсам, а файл окно спросит снова.
    pub fn set_rates(&self, text: &str) -> bool {
        self.set("rates", text)
    }

    /// Когда курсы ЦБ взяты с сайта; `None` — не брали.
    pub fn rates_at(&self) -> Option<Timestamp> {
        self.moment("rates_at")
    }

    /// Запомнить, когда взяты курсы. `false` — `localStorage` отказал.
    pub fn set_rates_at(&self, at: Timestamp) -> bool {
        self.set("rates_at", &at.unix_millis().to_string())
    }

    /// День начала пробного периода на ՀՎՀՀ по серверу сайта (задача 106,
    /// `license::TRIAL_PATH`): самый ранний из ответов о ՀՎՀՀ профилей этого
    /// устройства. Пишет его часть окна, когда сервер ответил днём раньше
    /// прежнего, а читает главный wasm при запуске — как день данных
    /// ([`Self::trial_data`]): метке срока и экрану лицензии не ждать сервера,
    /// и без сети срок тот же. Один на устройство: пробный период у всех ИП
    /// устройства общий. `None` — сервер ещё не отвечал или запись не читается.
    pub fn trial_tin(&self) -> Option<Date> {
        Date::from_iso(&self.get("trial_tin")?)
    }

    /// Запомнить день начала пробного периода на ՀՎՀՀ. `false` —
    /// `localStorage` отказал: метка при следующем запуске скажет срок по
    /// устройству, пока сервер не ответит снова.
    pub fn set_trial_tin(&self, day: Date) -> bool {
        self.set("trial_tin", &day.iso())
    }

    /// О каком ՀՎՀՀ и когда окно в последний раз спрашивало сервер о пробном
    /// периоде и что он ответил (задача 106): спрашивает оно не чаще раза
    /// в час на ՀՎՀՀ и после перезагрузки тоже, а новый ՀՎՀՀ — сразу. `None` —
    /// не спрашивало или запись не читается.
    pub fn trial_asked(&self) -> Option<Asked> {
        let text = self.get("trial_asked")?;
        let mut parts = text.split(' ');
        let (tin, millis, first) = (parts.next()?, parts.next()?, parts.next());
        let millis: i64 = millis.parse().ok()?;
        if parts.next().is_some() || millis.unsigned_abs() > Timestamp::MAX_MILLIS.unsigned_abs() {
            return None;
        }
        let first = match first {
            Some(day) => Some(Date::from_iso(day)?),
            None => None,
        };
        Some(Asked {
            tin: tin.to_owned(),
            at: Timestamp::from_unix_millis(millis),
            first,
        })
    }

    /// Запомнить запрос о пробном периоде — до ответа: не ответил сервер —
    /// следующий раз через час, а не на каждое открытие. `false` —
    /// `localStorage` отказал.
    pub fn set_trial_asked(&self, asked: &Asked) -> bool {
        let text = match asked.first {
            Some(day) => format!("{} {} {}", asked.tin, asked.at.unix_millis(), day.iso()),
            None => format!("{} {}", asked.tin, asked.at.unix_millis()),
        };
        self.set("trial_asked", &text)
    }

    /// Забыть последний запрос о пробном периоде: только что данное согласие
    /// спрашивает сервер сразу, а не через час. `false` — `localStorage`
    /// отказал.
    pub fn clear_trial_asked(&self) -> bool {
        self.remove("trial_asked")
    }

    /// На какие ՀՎՀՀ на этом устройстве согласились сверять пробный период
    /// с сервером сайта и под какой версией слов согласия (задача 106,
    /// `license::TRIAL_CONSENT`): ՀՎՀՀ и версия. Согласие — на устройстве, а не
    /// в базе: его дал человек за этим устройством, и в копию, открытую
    /// бухгалтером, оно не едет. Пусто — согласий нет или запись не
    /// читается: тогда окно спросит снова.
    pub fn trial_consents(&self) -> Vec<(String, String)> {
        self.get("trial_consent")
            .map(|text| {
                text.lines()
                    .filter_map(|line| {
                        let (tin, version) = line.split_once(' ')?;
                        Some((tin.to_owned(), version.to_owned()))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Запомнить согласия — строками «ՀՎՀՀ версия». `false` — `localStorage`
    /// отказал: согласие окно спросит при следующем выпуске снова.
    pub fn set_trial_consents(&self, list: &[(String, String)]) -> bool {
        if list.is_empty() {
            return self.remove("trial_consent");
        }
        let lines: Vec<String> = list.iter().map(|(tin, version)| format!("{tin} {version}")).collect();
        self.set("trial_consent", &lines.join("\n"))
    }

    /// Момент под этим именем — как у [`Self::last_backup`].
    fn moment(&self, name: &str) -> Option<Timestamp> {
        let millis: i64 = self.get(name)?.parse().ok()?;
        (millis.unsigned_abs() <= Timestamp::MAX_MILLIS.unsigned_abs())
            .then(|| Timestamp::from_unix_millis(millis))
    }

    fn get(&self, name: &str) -> Option<String> {
        storage()?
            .get_item(&format!("{}{name}", self.prefix))
            .ok()
            .flatten()
    }

    fn set(&self, name: &str, value: &str) -> bool {
        storage().is_some_and(|storage| {
            storage
                .set_item(&format!("{}{name}", self.prefix), value)
                .is_ok()
        })
    }

    fn remove(&self, name: &str) -> bool {
        storage().is_some_and(|storage| {
            storage
                .remove_item(&format!("{}{name}", self.prefix))
                .is_ok()
        })
    }

    /// Стереть записи этого префикса — для тестов.
    #[cfg(all(test, target_arch = "wasm32"))]
    pub(super) fn clear(&self) {
        if let Some(storage) = storage() {
            for name in [
                "last_backup",
                "backup_key",
                "lang",
                "trial",
                "seen_version",
                "update_tried",
                "steps_hidden",
                "last_auto",
                "changed",
                "auto_tag",
                "password_entered",
                HANDOFF,
                "office",
                "base",
                CURSOR,
                SITE_LANG,
                "colors",
                "license",
                "license_told",
                "trial_told",
                "trial_data",
                "pro_told",
                "rates",
                "rates_at",
            ] {
                let _ = storage.remove_item(&format!("{}{name}", self.prefix));
            }
        }
    }
}

/// Имя записи скрытых первых шагов: у первого ИП — прежнее, какое было до
/// конторы, у следующих — с именем базы.
fn steps_name(base: &str) -> String {
    per_base("steps_hidden", base)
}

/// Имя записи `name` ИП базы `base`: у первого ИП — само имя, у следующих —
/// с именем базы.
fn per_base(name: &str, base: &str) -> String {
    if base == super::DB_NAME {
        name.to_owned()
    } else {
        format!("{name}.{base}")
    }
}

/// Открывалась ли программа в этой вкладке раньше — это перезагрузка,
/// возврат «Назад» из другого сайта или включённая новая версия, а не
/// запуск, — и запомнить, что открывалась. Запись — во вкладке
/// (`sessionStorage`): она переживает перезагрузку и восстановление вкладки,
/// а закрытая вкладка её стирает, и новая вкладка или окно начинают свою (по
/// браузеру: MDN, `Window.sessionStorage`, правка 2026-10-12, сверено
/// 2026-11-11). По ней экран лицензии при конце срока встаёт только при
/// запуске (`ui/license.rs`). `sessionStorage` отказал — «открывалась»:
/// перезагрузка открывает тот же экран (CLAUDE.md, Правило 2), а без записи
/// их не отличить.
pub fn relaunched() -> bool {
    let Some(session) = super::office::session() else {
        return true;
    };
    let before = session.get_item(LAUNCHED).ok().flatten().is_some();
    before || session.set_item(LAUNCHED, "1").is_err()
}

/// Имя записи вкладки о запуске.
const LAUNCHED: &str = "kvartik.launched";

/// Как будто программа во вкладке открывается впервые (`fresh`) или уже
/// открывалась — браузерным тестам: у них одна вкладка на весь прогон.
#[cfg(all(test, target_arch = "wasm32"))]
pub(crate) fn pretend_launch(fresh: bool) {
    if let Some(session) = super::office::session() {
        let _ = if fresh {
            session.remove_item(LAUNCHED)
        } else {
            session.set_item(LAUNCHED, "1")
        };
    }
}

/// `localStorage`; `None` — его нет или браузер не даёт.
fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Окно ищет запись калькулятора там, куда её пишет wasm калькулятора:
    /// разойдись имена — предложение молча не встало бы ни у кого.
    #[test]
    fn the_handoff_is_read_where_the_calculator_writes_it() {
        assert_eq!(
            format!("{}{HANDOFF}", Device::new().prefix),
            kvartik_core::tax::example::HANDOFF_KEY
        );
    }

    /// Лендинг ищет запись кружка у курсора там, куда её пишет окно, и тем же
    /// значением: разойдись они — кружок, включённый в программе, на страницах
    /// сайта молча не встал бы.
    #[test]
    fn the_cursor_is_read_where_the_landing_reads_it() {
        let landing = include_str!("../../site/landing.js");
        let read = format!("localStorage.getItem(\"{}{CURSOR}\") === \"1\"", Device::new().prefix);
        assert!(landing.contains(&read), "в site/landing.js нет «{read}»");
    }

    /// Окно ищет язык, выбранный на страницах сайта, там, куда его пишет
    /// лендинг: разойдись имена — выбор на сайте молча не дошёл бы до
    /// программы. Что код из `hreflang` ссылки переключателя окно читает,
    /// проверяет собранный сайт (`tools/browser/check.mjs`, «лендинг»).
    #[test]
    fn the_site_language_is_read_where_the_landing_writes_it() {
        let landing = include_str!("../../site/landing.js");
        let write = format!(
            "localStorage.setItem(\"{}{SITE_LANG}\", link.hreflang)",
            Device::new().prefix
        );
        assert!(landing.contains(&write), "в site/landing.js нет «{write}»");
    }
}
