//! Лендинг (задача 34): страницы сайта на трёх языках в корне —
//! `index.html` по-армянски, `ru/index.html` и `en/index.html`, — готовым
//! HTML, а не окном. Яндекс и часть поисковиков wasm не исполняют, а текст
//! готовой страницы читают все; и первый заход не ждёт ни wasm, ни данных.
//!
//! Пишет их сборка из таблицы строк ядра: то, что видит человек, — ключ
//! `Key::…`, как в окне (CLAUDE.md, «Общее»), и ключ без перевода не
//! соберётся. Числа — тоже из ядра: срок пробного периода
//! (`license::TRIAL_DAYS`), день сверки правил (`Rules::checked`), адрес
//! общего календаря (`tax::calendar::path`). Страница, обещающая не то, что
//! делает программа, разошлась бы с ней молча.
//!
//! Рядом с каждой — юридические тексты того же языка (задача 32): условия
//! лицензии и политика конфиденциальности, `terms/` и `privacy/` в папке
//! страницы (`release::Legal`). Их текст — не таблица строк, а утверждённые
//! файлы `docs/legal/*.md` (`legal.rs`): опубликовано ровно утверждённое.
//! Шапка, подвал и оформление у них те же, что у страницы языка, а в подвале
//! каждой страницы — ссылки на оба текста и почта и Telegram продавца.
//!
//! Рядом — оформление и скрипт, одним файлом каждый, с хэшем содержимого
//! в имени (`kvartik-site-<хэш>.css` и `.js`): они кэшируются на год
//! (`_headers`), а новая сборка — новое имя. CSS — это `tokens.css`
//! и `fonts.css` окна и `site/landing.css` подряд: цвета и шрифты те же, что
//! у программы (Правило 4), и всё приходит одним запросом. Адреса шрифтов
//! в `fonts.css` — от папки CSS окна (`fonts/…`), а здесь CSS лежит в корне,
//! поэтому они переписываются на файлы программы (`/app/fonts/…`): лендинг
//! берёт её шрифты, и браузер, зашедший сюда, уже держит их в кэше.
//!
//! Встроенных скриптов и стилей нет — CSP их не пускает (`_headers`): скрипт
//! — файлом, а атрибутов `style` и `on…` в разметке нет, и это держит
//! проверка ниже. Текст таблицы экранируется: в HTML он только текст
//! (Правило 13).
//!
//! Повторяемость: ни часов, ни случайности — та же таблица и те же правила
//! дают те же байты, иначе пересборка без правок была бы у всех «новой
//! версией» (CLAUDE.md, Правило 6).

use std::fs;
use std::path::Path;

use kvartik_core::i18n::{self, Key, Lang, fill, plural, t};
use kvartik_core::license::TRIAL_DAYS;
use kvartik_core::model::date::Date;
use kvartik_core::release::{EMAIL, LICENSES, Legal, TELEGRAM};
use kvartik_core::tax::{calendar, example, rules, turnover};

use super::{digest, legal};

/// Происхождение сайта — навсегда (CLAUDE.md, Архитектура). Канонические
/// адреса страниц, карта сайта и подписка на календарь — с ним, а не
/// с адресом проверки: страница на предпросмотре называет настоящий сайт.
const SITE: &str = "https://kvartik.com";
/// Источники лендинга в папке `--site`.
const STYLE: &str = "landing.css";
const SCRIPT: &str = "landing.js";
/// Файлы оформления окна в папке `--style`: на них стоит и лендинг.
const TOKENS: &str = "tokens.css";
const FONTS: &str = "fonts.css";
/// Адрес шрифта в `fonts.css` — от папки CSS окна.
const FONT_URL: &str = "url(\"fonts/";

/// Что умеет программа — карточками, по порядку пути человека: документы,
/// книга, налог, сроки, где лежат данные и на чём работает.
const FEATURES: [(Key, Key); 6] = [
    (Key::LandingDocsTitle, Key::LandingDocsText),
    (Key::LandingBookTitle, Key::LandingBookText),
    (Key::LandingTaxTitle, Key::LandingTaxText),
    (Key::LandingDeadlinesTitle, Key::LandingDeadlinesText),
    (Key::LandingPrivacyTitle, Key::LandingPrivacyText),
    (Key::LandingDevicesTitle, Key::LandingDevicesText),
];

/// Калькулятор в корне сайта (`wasm::calculator`): клей wasm-bindgen и сам
/// wasm. Скрипт лендинга просит их вместе, а не wasm после клея: круг сети
/// на телефоне — 0,56 с.
pub struct Calc<'a> {
    pub module: &'a str,
    pub wasm: &'a str,
}

/// Адрес страницы языка от корня сайта. Армянская — корень: программа для
/// ИП Армении, и армянский — язык окна, пока браузер не назвал другого
/// (`Lang::preferred`).
pub fn url(lang: Lang) -> &'static str {
    match lang {
        Lang::Am => "/",
        Lang::Ru => "/ru/",
        Lang::En => "/en/",
    }
}

/// Файл страницы в сборке: её адрес от корня плюс `index.html`.
fn page_file(path: &str) -> String {
    format!("{}index.html", path.trim_start_matches('/'))
}

/// Лендинг в папку сборки `staging`: оформление и скрипт с хэшем в имени,
/// страницы трёх языков и юридические тексты к каждой, `robots.txt`
/// и `sitemap.xml`. Источники — из папки `site`, оформление окна — из `style`,
/// утверждённые тексты — из `legal`, адрес программы — `app` (`/app/`),
/// `calc` — модуль калькулятора в корне (`wasm::calculator`). Файл на месте
/// любого из них — ошибка, а не молча затёртый файл.
pub fn write(staging: &Path, site: &Path, style: &Path, legal: &Path, app: &str, calc: &Calc) -> Result<String, String> {
    let read = |path: &Path| fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()));
    let css = stylesheet(
        &read(&style.join(TOKENS))?,
        &read(&style.join(FONTS))?,
        &read(&site.join(STYLE))?,
        app,
    )?;
    let js = read(&site.join(SCRIPT))?;
    let css_name = format!("kvartik-site-{}.css", digest([(STYLE, css.as_bytes())]));
    let js_name = format!("kvartik-site-{}.js", digest([(SCRIPT, js.as_bytes())]));
    let checked = rules::armenia().checked();
    let mut files = Vec::new();
    for lang in Lang::ALL {
        files.push((page_file(url(lang)), page(lang, &css_name, &js_name, app, calc, checked)));
    }
    for doc in Legal::ALL {
        let mut texts = Vec::new();
        for lang in Lang::ALL {
            let name = format!("{}.{}.md", doc.name(), lang.code());
            texts.push((lang, legal::parse(&name, &read(&legal.join(&name))?)?));
        }
        let pairs: Vec<(Lang, &legal::Doc)> = texts.iter().map(|(lang, text)| (*lang, text)).collect();
        legal::same_shape(doc.name(), &pairs)?;
        for (lang, text) in &texts {
            let html = legal_page(*lang, doc, text, &css_name, &js_name, app, checked);
            files.push((page_file(doc.path(*lang)), html));
        }
    }
    files.push(("robots.txt".to_owned(), robots()));
    files.push(("sitemap.xml".to_owned(), sitemap()));
    files.push((css_name, css));
    files.push((js_name, js));
    if let Some((name, _)) = files.iter().find(|(name, _)| staging.join(name).exists()) {
        return Err(format!("{}: файл корня лёг бы на файл лендинга", staging.join(name).display()));
    }
    for (name, text) in &files {
        let target = staging.join(name);
        if let Some(folder) = target.parent() {
            fs::create_dir_all(folder).map_err(|error| format!("{}: {error}", folder.display()))?;
        }
        fs::write(&target, text).map_err(|error| format!("{}: {error}", target.display()))?;
    }
    Ok(format!(
        "лендинг: страниц {}, юридических текстов {}",
        Lang::ALL.len(),
        Legal::ALL.len() * Lang::ALL.len()
    ))
}

/// Оформление лендинга одним файлом: переменные, шрифты, свои правила —
/// в этом порядке, как у окна (`crates/app/index.html`). Адреса шрифтов —
/// на файлы программы. Адрес, который сборка не узнала, — ошибка, а не
/// системный шрифт вместо своего, о котором браузер промолчал бы
/// (Правило 4).
fn stylesheet(tokens: &str, fonts: &str, landing: &str, app: &str) -> Result<String, String> {
    let all = fonts.matches("url(").count();
    let known = fonts.matches(FONT_URL).count();
    if all == 0 || all != known {
        return Err(format!(
            "{FONTS}: адресов шрифтов {all}, из них вида {FONT_URL}…\") — {known}, а сборка лендинга \
             переписывает только их (crates/sw/src/landing.rs)"
        ));
    }
    let fonts = fonts.replace(FONT_URL, &format!("url(\"{app}fonts/"));
    Ok(format!("{tokens}\n{fonts}\n{landing}"))
}

/// Калькулятор (задача 34): форма — готовым HTML, а считает её wasm
/// `calc`, который скрипт лендинга грузит при первом касании формы
/// (`site/landing.js`). Без wasm форма видна, но не считает. Имена полей —
/// те, что разбирает ядро (`tax::example::Raw`), имена строк и гражданства —
/// как в данных (`example::kind_id`, `residency_id`). Поля, которые нужны не
/// всегда, спрятаны, пока ответ ядра не попросит их (`Fields`), а ошибки
/// поля и слова о загрузке лежат в разметке: свои тексты скрипту не нужны.
/// Ссылки в программу — с `data-handoff`: по нажатию скрипт отдаёт поля wasm,
/// и тот оставляет программе введённое (`example::handoff`).
fn calculator(lang: Lang, calc: &Calc, app: &str) -> String {
    let text = |key: Key| escape(t(lang, key));
    let choose = format!("<option value=\"\">{}</option>", text(Key::NotChosen));
    let mut kinds = String::new();
    for kind in example::KINDS {
        kinds.push_str(&format!(
            "<option value=\"{}\">{}</option>",
            escape(&example::kind_id(kind)),
            text(turnover::kind_key(kind))
        ));
    }
    let mut residencies = choose.clone();
    for (residency, key) in example::RESIDENCIES.into_iter().zip([
        Key::ResidencyCitizen,
        Key::ResidencyForeignWithStatus,
        Key::ResidencyForeignWithoutStatus,
    ]) {
        residencies.push_str(&format!(
            "<option value=\"{}\">{}</option>",
            escape(&example::residency_id(residency)),
            text(key)
        ));
    }
    let yes_no = format!(
        "{choose}<option value=\"yes\">{}</option><option value=\"no\">{}</option>",
        text(Key::Yes),
        text(Key::No)
    );
    // Поле: подпись, само поле и место для ошибки под ним, связанное с полем
    // для экранного диктора. `optional` — поле спрятано, пока не нужно.
    let field = |name: &str, label: Key, control: String, optional: bool| {
        let hidden = if optional { format!(" data-field=\"{name}\" hidden") } else { String::new() };
        format!(
            "          <label class=\"field\"{hidden}>\n            <span>{}</span>\n            {control}\n            \
             <span class=\"error\" id=\"calc-{name}-error\" data-error=\"{name}\" hidden></span>\n          </label>\n",
            text(label)
        )
    };
    let input = |name: &str, mode: &str, extra: &str| {
        format!(
            "<input name=\"{name}\" inputmode=\"{mode}\" autocomplete=\"off\"{extra} \
             aria-describedby=\"calc-{name}-error\" />"
        )
    };
    let select = |name: &str, options: &str| {
        format!("<select name=\"{name}\" aria-describedby=\"calc-{name}-error\">{options}</select>")
    };
    let mut fields = String::new();
    fields.push_str(&field("kind", Key::StepActivity, select("kind", &kinds), false));
    fields.push_str(&field("turnover", Key::CalcTurnover, input("turnover", "decimal", ""), false));
    fields.push_str(&field("expenses", Key::CalcExpenses, input("expenses", "decimal", ""), true));
    fields.push_str(&field("birth", Key::BirthYear, input("birth", "numeric", " maxlength=\"4\""), false));
    fields.push_str(&field("residency", Key::ProfileResidency, select("residency", &residencies), false));
    fields.push_str(&field("pension", Key::PensionJoined, select("pension", &yes_no), true));
    fields.push_str(&field("resident", Key::ResidentQuestion, select("resident", &yes_no), true));
    format!(
        r#"      <section class="card calc" aria-labelledby="calc" data-module="/{module}" data-wasm="/{wasm}">
        <h2 id="calc">{title}</h2>
        <p>{about}</p>
        <div class="fields">
{fields}        </div>
        <div class="result" aria-live="polite">
          <p class="note">{prompt}</p>
        </div>
        <p class="note" data-state="loading" hidden>{loading}</p>
        <p class="note" data-state="failed" role="alert" hidden>{failed}</p>
        <p class="actions"><a class="button" href="{app}" data-handoff><span class="label">{open}</span></a></p>
        <p class="hint">{handoff}</p>
      </section>
"#,
        module = escape(calc.module),
        wasm = escape(calc.wasm),
        title = text(Key::CalcTitle),
        about = text(Key::CalcAbout),
        prompt = text(Key::CalcPrompt),
        loading = text(Key::PartLoading),
        failed = text(Key::CalcFailed),
        open = text(Key::LandingOpen),
        handoff = text(Key::CalcHandoffHint),
    )
}

/// Страница языка `lang`. `css` и `js` — имена файлов оформления и скрипта
/// в корне, `app` — адрес программы, `calc` — модуль калькулятора,
/// `checked` — день последней сверки правил: нет его — нет и строки о нём.
fn page(lang: Lang, css: &str, js: &str, app: &str, calc: &Calc, checked: Option<Date>) -> String {
    let text = |key: Key| escape(t(lang, key));
    let days = u64::from(TRIAL_DAYS.unsigned_abs());
    let days = fill(plural(lang, days, Key::DaysOne, Key::DaysFew, Key::DaysMany), &[&days.to_string()]);
    let trial = escape(&fill(t(lang, Key::WelcomeTrial), &[&days]));
    let calendar = format!("{SITE}/{}", calendar::path(lang));
    let webcal = calendar.replacen("https://", "webcal://", 1);
    let own = url(lang);
    let locale = match lang {
        Lang::Am => "hy_AM",
        Lang::Ru => "ru_RU",
        Lang::En => "en_US",
    };
    let mut features = String::new();
    for (title, about) in FEATURES {
        features.push_str(&format!(
            "          <li data-tilt>\n            <h3>{}</h3>\n            <p>{}</p>\n          </li>\n",
            text(title),
            text(about)
        ));
    }
    let title = text(Key::LandingTitle);
    let description = text(Key::LandingDescription);
    let meta = format!(
        r#"    <meta name="description" content="{description}" />
    <meta property="og:type" content="website" />
    <meta property="og:site_name" content="{name}" />
    <meta property="og:title" content="{title}" />
    <meta property="og:description" content="{description}" />
    <meta property="og:url" content="{SITE}{own}" />
    <meta property="og:image" content="{SITE}{app}icons/icon-512.png" />
    <meta property="og:locale" content="{locale}" />
"#,
        name = text(Key::AppName),
    );
    format!(
        r#"{head}  <body>
{top}    <main>
      <section class="hero">
        <h1>{headline}</h1>
        <p class="lead">{lead}</p>
        <p class="actions"><a class="button" href="{app}" data-handoff><span class="label">{open}</span></a></p>
        <p class="trial">{trial} {after_trial}</p>
      </section>
{calculator}      <section aria-labelledby="features">
        <h2 id="features">{features_title}</h2>
        <ul class="cards">
{features}        </ul>
      </section>
      <section class="card" aria-labelledby="calendar" data-tilt>
        <h2 id="calendar">{calendar_title}</h2>
        <p>{calendar_text}</p>
        <p class="actions"><a class="button secondary" href="{webcal}"><span class="label">{subscribe}</span></a></p>
        <p class="hint">{subscribe_hint}</p>
        <p class="address">{calendar}</p>
      </section>
      <section class="card" aria-labelledby="fit" data-tilt>
        <h2 id="fit">{fit_title}</h2>
        <p>{fit_text}</p>
        <p class="actions"><a class="button" href="{app}" data-handoff><span class="label">{open}</span></a></p>
      </section>
    </main>
{footer}  </body>
</html>
"#,
        head = head(lang, &title, url, &meta, css, js, app),
        top = top(lang, own, url),
        footer = footer(lang, app, checked),
        calculator = calculator(lang, calc, app),
        headline = text(Key::WelcomeHeadline),
        lead = text(Key::LandingLead),
        open = text(Key::LandingOpen),
        after_trial = text(Key::LandingAfterTrial),
        features_title = text(Key::LandingFeaturesTitle),
        calendar_title = text(Key::LandingCalendarTitle),
        calendar_text = text(Key::LandingCalendarText),
        subscribe = text(Key::CalendarSubscribe),
        subscribe_hint = text(Key::CalendarSubscribeHint),
        fit_title = text(Key::LandingFitTitle),
        fit_text = text(Key::LandingFitText),
    )
}

/// Страница юридического текста `doc` на языке `lang` — рядом со страницей
/// языка: та же шапка, оформление и подвал, а языки ведут к тому же тексту
/// на другом языке. Описания для поисковиков нет — его заменит сам текст.
fn legal_page(
    lang: Lang,
    doc: Legal,
    text: &legal::Doc,
    css: &str,
    js: &str,
    app: &str,
    checked: Option<Date>,
) -> String {
    let path = |other: Lang| doc.path(other);
    format!(
        "{head}  <body>\n{top}    <main>\n      <article class=\"legal\">\n{body}      </article>\n    </main>\n{footer}  </body>\n</html>\n",
        head = head(lang, &escape(&text.title), path, "", css, js, app),
        top = top(lang, url(lang), path),
        body = legal::html(text, escape),
        footer = footer(lang, app, checked),
    )
}

/// Начало документа до `<body>`: язык, заголовок, та же страница на каждом
/// языке (`path`) — своя каноническая, армянская «по умолчанию», — `meta` —
/// описание страницы, если оно есть, значки, оформление и скрипт. `title`
/// уже экранирован.
fn head(lang: Lang, title: &str, path: impl Fn(Lang) -> &'static str, meta: &str, css: &str, js: &str, app: &str) -> String {
    let own = path(lang);
    let mut alternates = String::new();
    for other in Lang::ALL {
        alternates.push_str(&format!(
            "    <link rel=\"alternate\" hreflang=\"{}\" href=\"{SITE}{}\" />\n",
            other.code(),
            path(other)
        ));
    }
    alternates.push_str(&format!(
        "    <link rel=\"alternate\" hreflang=\"x-default\" href=\"{SITE}{}\" />\n",
        path(Lang::Am)
    ));
    format!(
        r#"<!doctype html>
<html lang="{code}">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <meta name="color-scheme" content="light dark" />
    <title>{title}</title>
    <link rel="canonical" href="{SITE}{own}" />
{alternates}{meta}    <link rel="icon" href="{app}icons/icon-32.png" sizes="32x32" type="image/png" />
    <link rel="icon" href="{app}icons/icon-192.png" sizes="192x192" type="image/png" />
    <link rel="apple-touch-icon" href="{app}icons/apple-touch-icon.png" />
    <link rel="stylesheet" href="/{css}" />
    <script src="/{js}" defer></script>
  </head>
"#,
        code = lang.code(),
    )
}

/// Шапка страницы: круги украшения, «Kvartik» — ссылкой на страницу языка
/// `home` — и языки ссылками на ту же страницу на каждом (`path`).
fn top(lang: Lang, home: &str, path: impl Fn(Lang) -> &'static str) -> String {
    // Язык — ссылкой на свою страницу, именем на нём самом (как
    // переключатель окна, `Lang::native_name`). Текущий — `aria-current`:
    // им же его выделяет оформление, не одним цветом (Правило 4).
    let mut langs = String::new();
    for other in Lang::ALL {
        let current = if other == lang { " aria-current=\"page\"" } else { "" };
        langs.push_str(&format!(
            "<a href=\"{}\" hreflang=\"{c}\" lang=\"{c}\"{current}>{}</a>",
            path(other),
            escape(other.native_name()),
            c = other.code()
        ));
    }
    format!(
        r#"    <div class="decor" aria-hidden="true"><span></span><span></span></div>
    <header class="top">
      <a class="brand" href="{home}">{brand}</a>
      <nav class="langs" aria-label="{language}">{langs}</nav>
    </header>
"#,
        brand = escape(t(lang, Key::Brand)),
        language = escape(t(lang, Key::Language)),
    )
}

/// Подвал каждой страницы: оговорка о расчёте, день сверки правил (нет его —
/// нет и строки), юридические тексты своего языка, тексты лицензий рядом
/// с программой и как написать продавцу — почтой и в Telegram
/// (`release::EMAIL`, `release::TELEGRAM`).
fn footer(lang: Lang, app: &str, checked: Option<Date>) -> String {
    let text = |key: Key| escape(t(lang, key));
    let rules = checked
        .map(|day| {
            let line = fill(t(lang, Key::LandingRulesChecked), &[&i18n::date(lang, day)]);
            format!("      <p>{}</p>\n", escape(&line))
        })
        .unwrap_or_default();
    format!(
        r#"    <footer>
      <p>{disclaimer}</p>
{rules}      <p class="links"><a href="{terms}">{terms_label}</a> <a href="{privacy}">{privacy_label}</a> <a href="{app}{LICENSES}">{licenses}</a></p>
      <p class="links"><span>{contact}</span> <a href="mailto:{EMAIL}">{EMAIL}</a> <a href="{TELEGRAM}">{telegram}</a></p>
    </footer>
"#,
        telegram = text(Key::ContactTelegram),
        disclaimer = text(Key::LandingDisclaimer),
        terms = Legal::Terms.path(lang),
        terms_label = text(Key::LegalTerms),
        privacy = Legal::Privacy.path(lang),
        privacy_label = text(Key::LegalPrivacy),
        licenses = text(Key::LandingLicenses),
        contact = text(Key::Contact),
    )
}

/// Поисковикам — всё, и где карта сайта. Программа не закрыта: её страница
/// сама просит её не показывать (`noindex` в `crates/app/index.html`), а
/// закрытую здесь поисковик не прочёл бы и этого.
fn robots() -> String {
    format!("User-agent: *\nAllow: /\n\nSitemap: {SITE}/sitemap.xml\n")
}

/// Карта сайта: страницы трёх языков и юридические тексты к ним, у каждой —
/// она же на всех трёх и «по умолчанию» армянская (как `hreflang` в самих
/// страницах). Без `lastmod`: дня сборки в ней нет, иначе одна и та же
/// сборка давала бы разные байты.
fn sitemap() -> String {
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\" xmlns:xhtml=\"http://www.w3.org/1999/xhtml\">\n",
    );
    let terms = |lang: Lang| Legal::Terms.path(lang);
    let privacy = |lang: Lang| Legal::Privacy.path(lang);
    let pages: [&dyn Fn(Lang) -> &'static str; 3] = [&url, &terms, &privacy];
    for path in pages {
        for lang in Lang::ALL {
            out.push_str(&format!("  <url>\n    <loc>{SITE}{}</loc>\n", path(lang)));
            for other in Lang::ALL {
                out.push_str(&format!(
                    "    <xhtml:link rel=\"alternate\" hreflang=\"{}\" href=\"{SITE}{}\" />\n",
                    other.code(),
                    path(other)
                ));
            }
            out.push_str(&format!(
                "    <xhtml:link rel=\"alternate\" hreflang=\"x-default\" href=\"{SITE}{}\" />\n  </url>\n",
                path(Lang::Am)
            ));
        }
    }
    out.push_str("</urlset>\n");
    out
}

/// Текст в HTML — только текстом (Правило 13): `&`, `<`, `>` и кавычки —
/// ссылками на знаки, и строка таблицы не станет разметкой ни в тексте,
/// ни в значении атрибута.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Настоящие файлы: проверка ловит правку, после которой сборка их не
    /// узнала бы.
    const TOKENS_CSS: &str = include_str!("../../app/style/tokens.css");
    const FONTS_CSS: &str = include_str!("../../app/style/fonts.css");
    const LANDING_CSS: &str = include_str!("../../app/site/landing.css");
    const LANDING_JS: &str = include_str!("../../app/site/landing.js");
    /// Утверждённые юридические тексты (задача 32) — те, что берёт сборка.
    const LEGAL: [(&str, &str); 6] = [
        ("terms.hy.md", include_str!("../../../docs/legal/terms.hy.md")),
        ("terms.ru.md", include_str!("../../../docs/legal/terms.ru.md")),
        ("terms.en.md", include_str!("../../../docs/legal/terms.en.md")),
        ("privacy.hy.md", include_str!("../../../docs/legal/privacy.hy.md")),
        ("privacy.ru.md", include_str!("../../../docs/legal/privacy.ru.md")),
        ("privacy.en.md", include_str!("../../../docs/legal/privacy.en.md")),
    ];

    /// Страница юридического текста из утверждённого файла.
    fn legal_sample(doc: Legal, lang: Lang) -> String {
        let name = format!("{}.{}.md", doc.name(), lang.code());
        let (_, source) = LEGAL.iter().find(|(file, _)| *file == name).expect("текст есть");
        let text = legal::parse(&name, source).expect("текст разбирается");
        legal_page(lang, doc, &text, "kvartik-site-a.css", "kvartik-site-b.js", "/app/", checked())
    }

    /// Все страницы сайта: три страницы языков и шесть юридических.
    fn every_page() -> Vec<(String, String)> {
        let mut out = Vec::new();
        for lang in Lang::ALL {
            out.push((format!("{lang:?} {}", url(lang)), sample(lang)));
            for doc in Legal::ALL {
                out.push((format!("{lang:?} {}", doc.path(lang)), legal_sample(doc, lang)));
            }
        }
        out
    }

    /// Модуль калькулятора в тестах страниц.
    const CALC: Calc = Calc {
        module: "kvartik-site-calc-c.js",
        wasm: "kvartik-site-calc-c_bg.wasm",
    };

    fn checked() -> Option<Date> {
        rules::armenia().checked()
    }

    /// Калькулятор (задача 34): модуль — по адресу от корня, у каждой строки
    /// и гражданства — вариант с именем, как в данных, поля — с именами,
    /// которые разбирает ядро (`example::Raw`), а поля, нужные не всегда,
    /// спрятаны, пока их не попросит ответ.
    #[test]
    fn the_calculator_form_matches_the_core() {
        let mut wrong = Vec::new();
        for lang in Lang::ALL {
            let html = sample(lang);
            let mut need = vec![
                format!("data-module=\"/{}\" data-wasm=\"/{}\"", CALC.module, CALC.wasm),
                escape(t(lang, Key::CalcTitle)),
                escape(t(lang, Key::CalcPrompt)),
            ];
            for kind in example::KINDS {
                need.push(format!(
                    "<option value=\"{}\">{}</option>",
                    example::kind_id(kind),
                    escape(t(lang, turnover::kind_key(kind)))
                ));
            }
            for residency in example::RESIDENCIES {
                need.push(format!("<option value=\"{}\">", example::residency_id(residency)));
            }
            for name in ["kind", "turnover", "expenses", "birth", "residency", "pension", "resident"] {
                need.push(format!("name=\"{name}\""));
                need.push(format!("data-error=\"{name}\""));
            }
            for optional in ["expenses", "pension", "resident"] {
                need.push(format!("data-field=\"{optional}\" hidden"));
            }
            for piece in need {
                if !html.contains(&piece) {
                    wrong.push(format!("{lang:?}: нет {piece}"));
                }
            }
            if html.matches("<input name=\"").count() + html.matches("<select name=\"").count() != 7 {
                wrong.push(format!("{lang:?}: полей не семь"));
            }
            // Введённое берёт с собой любая ссылка в программу, а под
            // ссылкой калькулятора сказано, что она возьмёт (задача 34).
            if !html.contains(&escape(t(lang, Key::CalcHandoffHint))) {
                wrong.push(format!("{lang:?}: не сказано, что ссылка возьмёт с собой"));
            }
            let links = html.matches("href=\"/app/\"").count();
            if links != 3 || html.matches("href=\"/app/\" data-handoff>").count() != links {
                wrong.push(format!("{lang:?}: не у всех трёх ссылок в программу есть data-handoff"));
            }
        }
        assert!(wrong.is_empty(), "{wrong:#?}");
    }

    fn sample(lang: Lang) -> String {
        page(lang, "kvartik-site-a.css", "kvartik-site-b.js", "/app/", &CALC, checked())
    }

    /// Каждая страница — на своём языке, называет себя канонической, все
    /// три языка и армянскую «по умолчанию», ведёт в программу, на своё
    /// оформление и скрипт и на календарь своего языка, а текущий язык
    /// у переключателя — ровно один, свой.
    #[test]
    fn pages_name_every_language_and_lead_into_the_app() {
        let mut wrong = Vec::new();
        for lang in Lang::ALL {
            let html = sample(lang);
            let mut need = vec![
                format!("<html lang=\"{}\">", lang.code()),
                format!("<link rel=\"canonical\" href=\"https://kvartik.com{}\" />", url(lang)),
                "<link rel=\"alternate\" hreflang=\"x-default\" href=\"https://kvartik.com/\" />".to_owned(),
                "<link rel=\"stylesheet\" href=\"/kvartik-site-a.css\" />".to_owned(),
                "<script src=\"/kvartik-site-b.js\" defer></script>".to_owned(),
                "<a class=\"button\" href=\"/app/\" data-handoff>".to_owned(),
                format!("href=\"webcal://kvartik.com/calendar/{}.ics\"", lang.code()),
                format!("<a href=\"/app/{LICENSES}\">"),
                escape(t(lang, Key::WelcomeHeadline)),
                escape(t(lang, Key::LandingDisclaimer)),
                format!("hreflang=\"{0}\" lang=\"{0}\" aria-current=\"page\"", lang.code()),
            ];
            for other in Lang::ALL {
                need.push(format!(
                    "<link rel=\"alternate\" hreflang=\"{}\" href=\"https://kvartik.com{}\" />",
                    other.code(),
                    url(other)
                ));
            }
            for piece in need {
                if !html.contains(&piece) {
                    wrong.push(format!("{lang:?}: нет {piece}"));
                }
            }
            if html.matches("aria-current=\"page\"").count() != 1 {
                wrong.push(format!("{lang:?}: текущий язык не один"));
            }
        }
        assert!(wrong.is_empty(), "{wrong:#?}");
    }

    /// Юридический текст (задача 32) — на своём языке, рядом со страницей
    /// этого языка: канонический адрес — свой, переводы и языки шапки ведут
    /// к тому же тексту на другом языке, «Kvartik» — на страницу языка,
    /// заголовок — название из файла, а заметок для человека на странице нет.
    #[test]
    fn legal_pages_link_their_translations() {
        let mut wrong = Vec::new();
        for doc in Legal::ALL {
            for lang in Lang::ALL {
                let html = legal_sample(doc, lang);
                let name = format!("{}.{}.md", doc.name(), lang.code());
                let (_, source) = LEGAL.iter().find(|(file, _)| *file == name).expect("текст есть");
                let title = source.lines().next().and_then(|line| line.strip_prefix("# ")).expect("название");
                let mut need = vec![
                    format!("<html lang=\"{}\">", lang.code()),
                    format!("<title>{}</title>", escape(title)),
                    format!("<link rel=\"canonical\" href=\"https://kvartik.com{}\" />", doc.path(lang)),
                    format!(
                        "<link rel=\"alternate\" hreflang=\"x-default\" href=\"https://kvartik.com{}\" />",
                        doc.path(Lang::Am)
                    ),
                    format!("<a class=\"brand\" href=\"{}\">", url(lang)),
                    format!("<h1>{}</h1>", escape(title)),
                    "<article class=\"legal\">".to_owned(),
                    format!("<a href=\"mailto:{EMAIL}\">"),
                ];
                for other in Lang::ALL {
                    need.push(format!(
                        "<link rel=\"alternate\" hreflang=\"{}\" href=\"https://kvartik.com{}\" />",
                        other.code(),
                        doc.path(other)
                    ));
                    need.push(format!("<a href=\"{}\" hreflang=\"{}\"", doc.path(other), other.code()));
                }
                for piece in need {
                    if !html.contains(&piece) {
                        wrong.push(format!("{name}: нет {piece}"));
                    }
                }
                if html.matches("aria-current=\"page\"").count() != 1 {
                    wrong.push(format!("{name}: текущий язык не один"));
                }
                if html.contains("Для проверки") || html.contains("og:") {
                    wrong.push(format!("{name}: заметка для человека или разметка лендинга"));
                }
            }
        }
        assert!(wrong.is_empty(), "{wrong:#?}");
    }

    /// Подвал каждой страницы ведёт на оба юридических текста своего языка,
    /// на тексты лицензий и к продавцу — почтой и в Telegram.
    #[test]
    fn every_footer_leads_to_the_terms_and_the_seller() {
        let mut wrong = Vec::new();
        for (name, html) in every_page() {
            let lang = Lang::ALL
                .into_iter()
                .find(|lang| html.contains(&format!("<html lang=\"{}\">", lang.code())))
                .expect("язык страницы");
            let footer = &html[html.find("<footer>").expect("подвал")..];
            for piece in [
                format!("<a href=\"{}\">{}</a>", Legal::Terms.path(lang), escape(t(lang, Key::LegalTerms))),
                format!("<a href=\"{}\">{}</a>", Legal::Privacy.path(lang), escape(t(lang, Key::LegalPrivacy))),
                format!("<a href=\"/app/{LICENSES}\">"),
                format!("<a href=\"mailto:{EMAIL}\">{EMAIL}</a>"),
                format!("<a href=\"{TELEGRAM}\">{}</a>", escape(t(lang, Key::ContactTelegram))),
                escape(t(lang, Key::LandingDisclaimer)),
            ] {
                if !footer.contains(&piece) {
                    wrong.push(format!("{name}: в подвале нет {piece}"));
                }
            }
        }
        assert!(wrong.is_empty(), "{wrong:#?}");
    }

    /// Числа страницы — из ядра, а не написаны ею: срок пробного периода
    /// и день последней сверки правил.
    #[test]
    fn numbers_come_from_the_core() {
        let html = sample(Lang::Ru);
        assert!(html.contains(&format!("{TRIAL_DAYS}\u{A0}дней бесплатно")), "нет срока пробного периода");
        let day = checked().expect("правила сверены");
        assert!(html.contains(&escape(&i18n::date(Lang::Ru, day))), "нет дня сверки правил");
        assert!(!page(Lang::Ru, "a.css", "b.js", "/app/", &CALC, None).contains("сверены с законом"));
    }

    /// Ни встроенного скрипта, ни стиля, ни атрибута `style` или
    /// обработчика `on…`: CSP их не пустил бы, и страница молча осталась бы
    /// без них.
    #[test]
    fn nothing_inline_for_the_csp_to_block() {
        let mut wrong = Vec::new();
        for (page, html) in every_page() {
            if html.matches("<script").count() != html.matches("<script src=").count() {
                wrong.push(format!("{page}: встроенный скрипт"));
            }
            if html.contains("<style") || html.contains(" style=") {
                wrong.push(format!("{page}: встроенный стиль"));
            }
            for (at, _) in html.match_indices(" on") {
                let rest = &html[at + 3..];
                let name = rest.chars().take_while(char::is_ascii_alphabetic).count();
                if name > 0 && rest[name..].starts_with('=') {
                    wrong.push(format!("{page}: обработчик on{}", &rest[..name]));
                }
            }
        }
        assert!(wrong.is_empty(), "{wrong:#?}");
    }

    /// Текст — только текстом (Правило 13).
    #[test]
    fn text_is_escaped() {
        assert_eq!(
            escape(r#"<a href="x">Tom's & co</a>"#),
            "&lt;a href=&quot;x&quot;&gt;Tom&#39;s &amp; co&lt;/a&gt;"
        );
    }

    /// Шрифты — файлы программы: адресов от папки CSS окна не осталось,
    /// а незнакомый адрес или файл без адресов — ошибка сборки, а не
    /// системный шрифт.
    #[test]
    fn fonts_point_into_the_app() {
        let css = stylesheet(TOKENS_CSS, FONTS_CSS, LANDING_CSS, "/app/").expect("адреса шрифтов узнаны");
        assert!(!css.contains(FONT_URL));
        assert_eq!(css.matches("url(\"/app/fonts/").count(), FONTS_CSS.matches("url(").count());
        assert!(css.starts_with(TOKENS_CSS) && css.ends_with(LANDING_CSS));
        assert!(stylesheet(TOKENS_CSS, "src: url(\"other/a.ttf\");", LANDING_CSS, "/app/").is_err());
        assert!(stylesheet(TOKENS_CSS, FONTS_CSS.replace(FONT_URL, "url(fonts/").as_str(), LANDING_CSS, "/app/").is_err());
        assert!(stylesheet(TOKENS_CSS, "", LANDING_CSS, "/app/").is_err());
    }

    /// Карта сайта называет все страницы, а `robots.txt` — карту.
    #[test]
    fn robots_and_sitemap_point_at_every_page() {
        let map = sitemap();
        for lang in Lang::ALL {
            assert!(map.contains(&format!("<loc>https://kvartik.com{}</loc>", url(lang))), "{lang:?}");
            for doc in Legal::ALL {
                assert!(map.contains(&format!("<loc>https://kvartik.com{}</loc>", doc.path(lang))), "{doc:?} {lang:?}");
            }
        }
        assert_eq!(map.matches("<url>").count(), Lang::ALL.len() * (1 + Legal::ALL.len()));
        assert!(robots().contains("Sitemap: https://kvartik.com/sitemap.xml\n"));
    }

    /// Папка во временной папке системы — своя у каждого теста.
    struct Temp(PathBuf);

    impl Temp {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("kvartik-landing-{}-{name}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).expect("временная папка создаётся");
            Self(dir)
        }

        fn put(&self, path: &str, text: &str) {
            let full = self.0.join(path);
            fs::create_dir_all(full.parent().expect("у файла есть папка")).expect("папка создаётся");
            fs::write(full, text).expect("файл пишется");
        }
    }

    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn sources(temp: &Temp) {
        temp.put("style/tokens.css", TOKENS_CSS);
        temp.put("style/fonts.css", FONTS_CSS);
        temp.put("site/landing.css", LANDING_CSS);
        temp.put("site/landing.js", LANDING_JS);
        for (name, text) in LEGAL {
            temp.put(&format!("legal/{name}"), text);
        }
    }

    fn build(temp: &Temp) -> Result<String, String> {
        let at = |name: &str| temp.0.join(name);
        write(&at("stage"), &at("site"), &at("style"), &at("legal"), "/app/", &CALC)
    }

    /// Файлы папки со всеми вложенными — путями от неё, по порядку.
    fn names(dir: &Path) -> Vec<String> {
        let mut out = Vec::new();
        let mut folders = vec![dir.to_path_buf()];
        while let Some(folder) = folders.pop() {
            for entry in fs::read_dir(&folder).expect("папка читается") {
                let path = entry.expect("запись папки").path();
                if path.is_dir() {
                    folders.push(path);
                } else {
                    let name = path.strip_prefix(dir).expect("внутри папки").to_string_lossy().replace('\\', "/");
                    out.push(name);
                }
            }
        }
        out.sort();
        out
    }

    /// Лендинг ложится в корень сборки: три страницы, шесть юридических,
    /// карта, `robots.txt` и по одному файлу оформления и скрипта, на которые
    /// страницы ссылаются. Та же сборка — те же имена и байты. Файл на месте
    /// файла лендинга — ошибка, а не молча затёртый файл.
    #[test]
    fn writes_the_site_and_refuses_to_overwrite() {
        let temp = Temp::new("write");
        sources(&temp);
        let stage = temp.0.join("stage");
        fs::create_dir_all(&stage).expect("папка сборки");
        build(&temp).expect("лендинг пишется");
        let written = names(&stage);
        let css: Vec<&String> = written.iter().filter(|n| n.starts_with("kvartik-site-") && n.ends_with(".css")).collect();
        let js: Vec<&String> = written.iter().filter(|n| n.starts_with("kvartik-site-") && n.ends_with(".js")).collect();
        assert_eq!((css.len(), js.len()), (1, 1), "{written:?}");
        let mut pages: Vec<String> = Lang::ALL.into_iter().map(|lang| page_file(url(lang))).collect();
        for doc in Legal::ALL {
            pages.extend(Lang::ALL.into_iter().map(|lang| page_file(doc.path(lang))));
        }
        for name in pages.iter().map(String::as_str).chain(["robots.txt", "sitemap.xml"]) {
            assert!(written.iter().any(|n| n == name), "нет {name}: {written:?}");
        }
        assert_eq!(written.len(), pages.len() + 4, "{written:?}");
        for page in &pages {
            let html = fs::read_to_string(stage.join(page)).expect("страница читается");
            assert!(html.contains(&format!("href=\"/{}\"", css[0])) && html.contains(&format!("src=\"/{}\"", js[0])));
        }

        let again = Temp::new("again");
        sources(&again);
        fs::create_dir_all(again.0.join("stage")).expect("папка сборки");
        build(&again).expect("лендинг пишется");
        let stage_again = again.0.join("stage");
        assert_eq!(names(&stage_again), written, "та же сборка — те же имена");
        for name in &written {
            assert_eq!(fs::read(stage.join(name)).ok(), fs::read(stage_again.join(name)).ok(), "{name}");
        }

        let clash = Temp::new("clash");
        sources(&clash);
        clash.put("stage/ru/index.html", "root page");
        let error = build(&clash).expect_err("файл корня на месте страницы лендинга");
        assert!(error.contains("лёг бы на файл лендинга"), "{error}");
        let clash = Temp::new("clash-legal");
        sources(&clash);
        clash.put("stage/ru/terms/index.html", "root page");
        let error = build(&clash).expect_err("файл корня на месте юридического текста");
        assert!(error.contains("лёг бы на файл лендинга"), "{error}");
    }

    /// Утверждённый текст, который не разобрался или разошёлся с переводом,
    /// останавливает сборку словами с именем файла, а не уходит на сайт.
    #[test]
    fn a_broken_legal_text_stops_the_build() {
        let broken = Temp::new("broken");
        sources(&broken);
        fs::create_dir_all(broken.0.join("stage")).expect("папка сборки");
        broken.put("legal/terms.en.md", "# Terms\n\n**Edition.**\n\nText with *markup*.\n");
        let error = build(&broken).expect_err("разметка, которой разбор не знает");
        assert!(error.contains("terms.en.md"), "{error}");

        let short = Temp::new("short");
        sources(&short);
        fs::create_dir_all(short.0.join("stage")).expect("папка сборки");
        short.put("legal/privacy.hy.md", "# Գաղտնիություն\n\n**Խմբագրություն։**\n\nՏեքստ։\n");
        let error = build(&short).expect_err("перевод без разделов");
        assert!(error.contains("privacy") && error.contains("разошлись"), "{error}");

        let missing = Temp::new("missing");
        sources(&missing);
        fs::create_dir_all(missing.0.join("stage")).expect("папка сборки");
        fs::remove_file(missing.0.join("legal/privacy.ru.md")).expect("файл удаляется");
        let error = build(&missing).expect_err("нет перевода");
        assert!(error.contains("privacy.ru.md"), "{error}");
    }
}
