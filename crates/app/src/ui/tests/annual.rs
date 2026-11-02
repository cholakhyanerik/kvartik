//! Браузерные тесты годовых платежей (задача 20): всё окно в настоящем DOM
//! и настоящей IndexedDB. Профиль и доход кладутся в базу командами ядра —
//! их формы проверяют свои тесты; суммы посчитаны вручную.

use kvartik_core::i18n::{self, Key, Lang, fill, t};
use kvartik_core::model::date::Date;
use kvartik_core::model::ledger::{Flow, Operation, OperationData};
use kvartik_core::model::money::{Currency, Money};
use kvartik_core::model::profile::{Person, Profile, Residency};
use kvartik_core::model::text::Line;
use kvartik_core::tax::annual;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::wasm_bindgen_test;
use web_sys::{Element, HtmlElement};

use super::documents::{commit, forget_device, profile};
use super::{button, field, mount_at, sleep, stored_profile, type_into, wait_for};
use crate::platform::{clock, set_hash};
use crate::store::Writes;
use crate::store::device::Device;

/// Адрес экрана годовых платежей — договор с закладками (CLAUDE.md,
/// Правило 2, «Разделы — экранами»).
const ANNUAL: &str = "#/annual";

fn drams(lang: Lang, drams: i64) -> String {
    i18n::money(lang, Money::new(drams, Currency::Amd))
}

/// Текст первого элемента `selector`; `None` — его нет.
fn text_of(host: &HtmlElement, selector: &str) -> Option<String> {
    host.query_selector(selector).ok().flatten().and_then(|element| element.text_content())
}

/// Адрес ссылки `selector`; `None` — её нет.
fn href_of(host: &HtmlElement, selector: &str) -> Option<String> {
    host.query_selector(selector).ok().flatten().and_then(|element| element.get_attribute("href"))
}

/// Пары «подпись — значение» карточки платежа с заголовком `title`;
/// `None` — такой карточки нет. Сумма ищется в своей карточке и в своей
/// строке: число шкалы в заметке или сумма соседнего платежа её не
/// подменят.
fn pairs(host: &HtmlElement, title: &str) -> Option<Vec<(String, String)>> {
    let cards = host.query_selector_all("#annual .annual-line").ok()?;
    let card = (0..cards.length())
        .filter_map(|at| cards.item(at))
        .map(JsCast::unchecked_into::<Element>)
        .find(|card| text_of(card.unchecked_ref(), "h3").as_deref() == Some(title))?;
    let rows = card.query_selector_all("dl.pairs > div").ok()?;
    Some(
        (0..rows.length())
            .filter_map(|at| rows.item(at))
            .map(JsCast::unchecked_into::<HtmlElement>)
            .map(|row| {
                (
                    text_of(&row, "dt").unwrap_or_default(),
                    text_of(&row, "dd").unwrap_or_default(),
                )
            })
            .collect(),
    )
}

/// Пустая база с профилем этого человека и доходом 1 000 000 ֏ в январе
/// года ближайшего срока; возвращает этот год.
async fn seed(person: Person) -> u16 {
    crate::store::delete_database(crate::store::DB_NAME).await;
    let today = clock::today().expect("часы с датой");
    let now = clock::now();
    let year = annual::due_year(today, Some(&person));
    let mut data = profile();
    data.person = person;
    let saved = Profile::save(None, data, now).expect("годный профиль").expect("новый");
    let income = Operation::create(
        OperationData {
            flow: Flow::Income,
            date: Date::new(year, 1, 15).expect("день есть"),
            paid: None,
            amount: Money::new(1_000_000, Currency::Amd),
            rate: None,
            text: Line::new("Разработка").expect("текст"),
            client: None,
            supplier: Line::default(),
            proof: None,
            nature: None,
            activity: None,
            income_kind: None,
            attachments: Vec::new(),
        },
        now,
    )
    .expect("годная запись");
    let mut writes = Writes::default();
    writes.insert(&saved);
    writes.insert(&income);
    commit(writes).await;
    year
}

/// Открыть экран налога и дождаться строки к годовым платежам: «Сверьте
/// расчёт с бухгалтером» есть и на экране годовых платежей, и ждать надо
/// саму строку.
async fn tax_entry(host: &HtmlElement) -> Option<String> {
    set_hash("#/tax");
    for _ in 0..100 {
        if let Some(entry) = text_of(host, "#tax .annual-entry") {
            return Some(entry);
        }
        sleep(50).await;
    }
    None
}

/// Гражданин 1990 года, ИП с 2022 года, с доходом 1 000 000 ֏ в году
/// ближайшего срока. Годовые платежи за этот год — вручную: налог на
/// прибыль 12 × 5 000 = 60 000, соцплатёж — столько же, гербовый сбор
/// 12 000 (оборот до 12 000 000), к 20 апреля следующего года — 132 000.
/// Главный экран показывает это карточкой со сроком, экран годовых
/// платежей — итогом и карточками платежей, стрелка листает год, а экран
/// налога ведёт к годовым платежам строкой; посчитано всё.
#[wasm_bindgen_test]
async fn annual_payments_are_shown_on_home_and_their_screen() {
    let year = seed(Person {
        birth_year: Some(1990),
        pension_joined: None,
        pension_applied: None,
        residency: Some(Residency::Citizen),
        resident: None,
        registered: Date::new(2022, 5, 12),
        stops: Vec::new(),
    })
    .await;
    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let (host, mounted) = mount_at("#/");
    let mut wrong = Vec::new();
    let title = fill(t(lang, Key::AnnualFor), &[&year.to_string()]);
    let total = drams(lang, 132_000);
    let due = Date::new(year + 1, 4, 20).expect("день есть");
    let due_text = fill(t(lang, Key::AnnualDue), &[&i18n::date(lang, due)]);
    if !wait_for(&host, &title).await {
        wrong.push("на главном нет карточки годовых платежей".to_owned());
    }
    match text_of(&host, "#home .annual-card") {
        Some(card)
            if card.contains(&total) && card.contains(&due_text) && !card.contains(t(lang, Key::AnnualPartialCard)) => {}
        other => wrong.push(format!("карточка на главном: {other:?}")),
    }
    if href_of(&host, "#home .annual-card").as_deref() != Some(ANNUAL) {
        wrong.push(format!("карточка ведёт не туда: {:?}", href_of(&host, "#home .annual-card")));
    }

    set_hash(ANNUAL);
    if !wait_for(&host, t(lang, Key::AnnualSumOf)).await {
        wrong.push("экран годовых платежей не открылся".to_owned());
    }
    match text_of(&host, "#annual .hero-card") {
        Some(hero) if hero.contains(&total) && !hero.contains(t(lang, Key::AnnualPartial)) => {}
        other => wrong.push(format!("итог на экране: {other:?}")),
    }
    let months = fill(
        t(lang, Key::AnnualTimes),
        &[&fill(t(lang, Key::MonthsMany), &["12"]), &drams(lang, 5_000)],
    );
    let deadline = (t(lang, Key::AnnualDueLabel).to_owned(), i18n::date(lang, due));
    let turnover = fill(t(lang, Key::AnnualStampTurnover), &[&year.to_string()]);
    for (title, expected) in [
        (Key::AnnualProfit, vec![(months.clone(), drams(lang, 60_000)), deadline.clone()]),
        (Key::AnnualSocial, vec![(months.clone(), drams(lang, 60_000)), deadline.clone()]),
        (
            Key::AnnualStamp,
            vec![
                (turnover, drams(lang, 1_000_000)),
                (t(lang, Key::AnnualStampSum).to_owned(), drams(lang, 12_000)),
                deadline.clone(),
            ],
        ),
    ] {
        let got = pairs(&host, t(lang, title));
        if got.as_ref() != Some(&expected) {
            wrong.push(format!("карточка «{}»: {got:?}, а не {expected:?}", t(lang, title)));
        }
    }
    if pairs(&host, t(lang, Key::AnnualHealth)).is_none_or(|pairs| pairs.is_empty()) {
        wrong.push("нет карточки медстраховки".to_owned());
    }
    let back: Option<HtmlElement> = host
        .query_selector(&format!("#annual button[aria-label=\"{}\"]", t(lang, Key::AnnualPrevYear)))
        .ok()
        .flatten()
        .map(JsCast::unchecked_into);
    match back {
        Some(back) => back.click(),
        None => wrong.push("нет стрелки прошлого года".to_owned()),
    }
    sleep(100).await;
    let previous = fill(t(lang, Key::AnnualYear), &[&(year - 1).to_string()]);
    if !text_of(&host, "#annual .quarter-name").is_some_and(|name| name.contains(&previous)) {
        wrong.push(format!("стрелка не перелистнула на «{previous}»"));
    }

    match tax_entry(&host).await {
        Some(entry)
            if entry.contains(&title) && entry.contains(&total) && !entry.contains(t(lang, Key::AnnualPartialShort)) => {}
        other => wrong.push(format!("строка на экране налога: {other:?}")),
    }
    if href_of(&host, "#tax .annual-entry a.entry").as_deref() != Some(ANNUAL) {
        wrong.push(format!("строка ведёт не туда: {:?}", href_of(&host, "#tax .annual-entry a.entry")));
    }

    drop(mounted);
    host.remove();
    set_hash("");
    forget_device();
    assert!(wrong.is_empty(), "годовые платежи: {wrong:#?}");
}

/// Нажать ответ `answer` в группе полей с заголовком `legend`; `false` —
/// такой группы или ответа нет.
fn choose(host: &HtmlElement, legend: &str, answer: &str) -> bool {
    let Ok(groups) = host.query_selector_all("fieldset.group") else {
        return false;
    };
    let group = (0..groups.length())
        .filter_map(|at| groups.item(at))
        .map(JsCast::unchecked_into::<HtmlElement>)
        .find(|group| text_of(group, "legend").as_deref() == Some(legend));
    let choice = group.and_then(|group| {
        let choices = group.query_selector_all("label.choice").ok()?;
        (0..choices.length())
            .filter_map(|at| choices.item(at))
            .map(JsCast::unchecked_into::<Element>)
            .find(|choice| choice.text_content().as_deref() == Some(answer))
            .and_then(|choice| choice.query_selector("input").ok().flatten())
    });
    match choice {
        Some(input) => {
            input.unchecked_into::<HtmlElement>().click();
            true
        }
        None => false,
    }
}

/// Гражданин 1950 года рождения, ИП с 2022 года: вопросы профиля решают то,
/// что без них было словами. Форма спрашивает день заявления о фонде, только
/// когда вступил сам, а резидентство — только у гражданина; ответы ложатся
/// в базу, а экран платежей называет день заявления и говорит, что взнос
/// платит государство.
#[wasm_bindgen_test]
async fn profile_answers_settle_annual_payments() {
    let year = seed(Person {
        birth_year: Some(1950),
        pension_joined: None,
        pension_applied: None,
        residency: Some(Residency::Citizen),
        resident: None,
        registered: Date::new(2022, 5, 12),
        stops: Vec::new(),
    })
    .await;
    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let (host, mounted) = mount_at(ANNUAL);
    let mut wrong = Vec::new();
    if !wait_for(&host, &fill(t(lang, Key::AnnualHealthElderly), &["65"])).await {
        wrong.push("нет просьбы ответить о резидентстве".to_owned());
    }

    set_hash("#/profile");
    if !wait_for(&host, t(lang, Key::ProfilePerson)).await {
        wrong.push("экран профиля не открылся".to_owned());
    }
    let rows = host.query_selector_all("#profile button.entry").expect("селектор годен");
    let row = (0..rows.length())
        .filter_map(|at| rows.item(at))
        .map(JsCast::unchecked_into::<HtmlElement>)
        .find(|row| row.text_content().is_some_and(|text| text.contains(t(lang, Key::ProfilePerson))));
    match row {
        Some(row) => row.click(),
        None => wrong.push("в итоге профиля нет строки «О вас»".to_owned()),
    }
    sleep(100).await;

    let (joined, day_label) = (t(lang, Key::PensionJoined), t(lang, Key::PensionApplied));
    let (question, yes, no) = (t(lang, Key::ResidentQuestion), t(lang, Key::Yes), t(lang, Key::No));
    let text = || host.text_content().unwrap_or_default();
    if text().contains(day_label) {
        wrong.push("день заявления спрашивают до ответа о вступлении".to_owned());
    }
    if !choose(&host, joined, yes) {
        wrong.push("нет вопроса о вступлении в накопительную пенсию".to_owned());
    }
    sleep(50).await;
    if !text().contains(day_label) {
        wrong.push("вступившего сам не спрашивают о дне заявления".to_owned());
    }
    if !choose(&host, joined, no) {
        wrong.push("нет ответа «нет» о вступлении".to_owned());
    }
    sleep(50).await;
    if text().contains(day_label) {
        wrong.push("о дне заявления спрашивают того, кто не вступал".to_owned());
    }
    choose(&host, joined, yes);
    sleep(50).await;
    let applied = Date::new(year - 1, 3, 10).expect("день есть");
    match field(&host, day_label, 0) {
        Some(input) => type_into(&input, &applied.iso()),
        None => wrong.push("нет поля дня заявления".to_owned()),
    }
    // Вопрос о резидентстве — только гражданину.
    if !choose(&host, t(lang, Key::ProfileResidency), t(lang, Key::ResidencyForeignWithStatus)) {
        wrong.push("нет выбора гражданства".to_owned());
    }
    sleep(50).await;
    if text().contains(question) {
        wrong.push("о резидентстве спрашивают иностранца".to_owned());
    }
    choose(&host, t(lang, Key::ProfileResidency), t(lang, Key::ResidencyCitizen));
    sleep(50).await;
    if !choose(&host, question, yes) {
        wrong.push("гражданина 1950 года не спрашивают о резидентстве".to_owned());
    }
    match button(&host, &[t(lang, Key::ProfileSave)]) {
        Some(save) => save.click(),
        None => wrong.push("в форме нет «Сохранить»".to_owned()),
    }
    if !wait_for(&host, t(lang, Key::ProfileSaved)).await {
        wrong.push("сохранение не подтвердилось".to_owned());
    }
    match stored_profile().await {
        Ok(Some(saved)) => {
            let person = &saved.data().person;
            if person.pension_joined != Some(true)
                || person.pension_applied != Some(applied)
                || person.resident != Some(true)
            {
                wrong.push(format!("в базе не то: {person:?}"));
            }
        }
        other => wrong.push(format!("в базе профиля нет: {other:?}")),
    }

    set_hash(ANNUAL);
    let joined_note = fill(t(lang, Key::AnnualSocialJoined), &[&i18n::date(lang, applied)]);
    if !wait_for(&host, &joined_note).await {
        wrong.push("экран платежей не называет день заявления".to_owned());
    }
    if !text().contains(&fill(t(lang, Key::AnnualHealthStatePays), &["65"])) {
        wrong.push("экран платежей не говорит, что взнос платит государство".to_owned());
    }
    let months = fill(
        t(lang, Key::AnnualTimes),
        &[&fill(t(lang, Key::MonthsMany), &["12"]), &drams(lang, 5_000)],
    );
    let social = pairs(&host, t(lang, Key::AnnualSocial)).unwrap_or_default();
    if social.first() != Some(&(months, drams(lang, 60_000))) {
        wrong.push(format!("соцплатёж: {social:?}"));
    }

    drop(mounted);
    host.remove();
    set_hash("");
    forget_device();
    assert!(wrong.is_empty(), "ответы профиля: {wrong:#?}");
}

/// Без даты регистрации и года рождения: месяцы — с января, соцплатёж не
/// известен. Итог — то, что известно: налог на прибыль 60 000 и гербовый
/// сбор 12 000, 72 000 ֏ (вручную), — и каждое место говорит, что посчитано
/// не всё, а экран просит заполнить профиль (Правило 6).
#[wasm_bindgen_test]
async fn annual_payments_say_what_is_missing() {
    let year = seed(Person {
        birth_year: None,
        pension_joined: None,
        pension_applied: None,
        residency: Some(Residency::Citizen),
        resident: None,
        registered: None,
        stops: Vec::new(),
    })
    .await;
    let lang = Lang::Ru;
    Device::new().set_ui_lang(lang);
    let (host, mounted) = mount_at("#/");
    let mut wrong = Vec::new();
    let title = fill(t(lang, Key::AnnualFor), &[&year.to_string()]);
    let known = drams(lang, 72_000);
    if !wait_for(&host, &title).await {
        wrong.push("на главном нет карточки годовых платежей".to_owned());
    }
    match text_of(&host, "#home .annual-card") {
        Some(card) if card.contains(&known) && card.contains(t(lang, Key::AnnualPartialCard)) => {}
        other => wrong.push(format!("карточка на главном: {other:?}")),
    }

    set_hash(ANNUAL);
    if !wait_for(&host, t(lang, Key::AnnualSumOf)).await {
        wrong.push("экран годовых платежей не открылся".to_owned());
    }
    // «Отложите … — к сроку наберётся то, что посчитано», а в месяц срока —
    // «нужна вся сумма»: сколько в месяц, зависит от сегодняшнего дня.
    let partial_saving = t(lang, Key::AnnualSavePartial).rsplit("{}").next().unwrap_or_default();
    match text_of(&host, "#annual .hero-card") {
        Some(hero)
            if hero.contains(&known)
                && hero.contains(t(lang, Key::AnnualPartial))
                && (hero.contains(partial_saving) || hero.contains(t(lang, Key::AnnualSaveLast))) => {}
        other => wrong.push(format!("итог на экране: {other:?}")),
    }
    let notice = text_of(&host, "#annual .notice").unwrap_or_default();
    for key in [Key::AnnualAskRegistered, Key::AnnualAskBirthYear] {
        if !notice.contains(t(lang, key)) {
            wrong.push(format!("экран не просит: «{}»", t(lang, key)));
        }
    }
    let screen = text_of(&host, "#annual").unwrap_or_default();
    let from_january = fill(t(lang, Key::AnnualRegistrationUnknown), &[&year.to_string()]);
    if !screen.contains(&from_january) {
        wrong.push("налог на прибыль не говорит, что месяцы — с января".to_owned());
    }
    let unknown = (t(lang, Key::AnnualUnknown).to_owned(), String::new());
    if pairs(&host, t(lang, Key::AnnualSocial)).as_deref() != Some(&[unknown][..]) {
        wrong.push(format!("соцплатёж: {:?}", pairs(&host, t(lang, Key::AnnualSocial))));
    }

    match tax_entry(&host).await {
        Some(entry) if entry.contains(&known) && entry.contains(t(lang, Key::AnnualPartialShort)) => {}
        other => wrong.push(format!("строка на экране налога: {other:?}")),
    }

    drop(mounted);
    host.remove();
    set_hash("");
    forget_device();
    assert!(wrong.is_empty(), "неполные годовые платежи: {wrong:#?}");
}
