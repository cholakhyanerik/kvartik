//! Лендинг (задача 34): страницы сайта на трёх языках в корне —
//! `index.html` по-армянски, `ru/index.html` и `en/index.html`, — готовым
//! HTML, а не окном. Яндекс и часть поисковиков wasm не исполняют, а текст
//! готовой страницы читают все; и первый заход не ждёт ни wasm, ни данных.
//!
//! Пишет их сборка из таблицы строк ядра: то, что видит человек, — ключ
//! `Key::…`, как в окне (CLAUDE.md, «Общее»), и ключ без перевода не
//! соберётся. Числа — тоже из ядра: срок пробного периода
//! (`license::TRIAL_KEY_DAYS`), день сверки правил (`Rules::checked`), адрес
//! общего календаря (`tax::calendar::path`). Страница, обещающая не то, что
//! делает программа, разошлась бы с ней молча.
//!
//! Рядом с каждой — юридические тексты того же языка (задача 32): условия
//! лицензии и политика конфиденциальности, `terms/` и `privacy/` в папке
//! страницы (`release::Legal`). Их текст — не таблица строк, а утверждённые
//! файлы `docs/legal/*.md` (`legal.rs`): опубликовано ровно утверждённое.
//! Шапка, подвал и оформление у них те же, что у страницы языка, а в подвале
//! каждой страницы — ссылки на оба текста и почта и Telegram продавца.
//! Прежние редакции (`Legal::earlier`, задача 93) — в папке текста под своим
//! днём, `terms/2026-11-09/`: условия обещают держать их на сайте (п. 11).
//!
//! И страница «Купить» (задача 35), `buy/` в той же папке
//! (`release::buy_path`): тариф и цена, шаги покупки по условиям (п. 2)
//! и реквизиты перевода. Её текст — таблица строк, а числа — ядро: цена,
//! счёт, банк, владелец счёта и ՀՎՀՀ продавца (`release`). Условия зовут
//! её по имени и берут с неё цену на день оплаты (п. 5), поэтому слова
//! согласия, которые она просит написать, сборка сверяет с условиями своего
//! языка: новая редакция условий без новых слов на странице сайт не соберёт.
//!
//! И страница «Отзывы и предложения» (задача 114), `feedback/` там же
//! (`release::feedback_path`): что написать об ошибке или улучшении, письмо
//! продавцу с уже вписанными строками и его Telegram. Сама она ничего не
//! отправляет.
//!
//! И страница о налоге с оборота (задача 118), `tax/` там же (`tax_path`):
//! ответ сверху, ставки таблицей, порог, сроки на год, пример и курс ЦБ —
//! из файла правил ядра, с актом, статьёй и днём сверки у каждого числа
//! (у сроков — с задачи 126), сроки — те же, что в общем календаре,
//! пример — расчётом калькулятора. Такие страницы цитируют поисковики
//! и ИИ-ответы, а второго расчёта «для сайта» в ней нет.
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
//! проверка ниже. Одно исключение (задача 118) — описание программы для
//! поисковиков у страниц языков, `<script type="application/ld+json">`
//! (`described`), а с задачи 126 — и описание страницы о налоге
//! (`described_tax`): это блок данных, браузер его не исполняет, и CSP его не
//! касается — тип не JavaScript, и разбор элемента кончается раньше проверки
//! CSP (HTML, «prepare the script element»). Что отказов CSP нет, сверяет
//! проверка сайта в браузере (`tools/browser/check.mjs`, «лендинг»). Текст
//! таблицы экранируется: в HTML он только текст (Правило 13).
//!
//! И файл ключа IndexNow в корне (задача 118, `INDEXNOW_KEY`): после выкладки
//! всем CI называет им Bing и Яндексу изменившиеся страницы
//! (`tools/deploy/indexnow.mjs`).
//!
//! Повторяемость: ни часов, ни случайности — та же таблица и те же правила
//! дают те же байты, иначе пересборка без правок была бы у всех «новой
//! версией» (CLAUDE.md, Правило 6).

use std::fs;
use std::path::Path;

use kvartik_core::i18n::{self, Key, Lang, fill, plural, t};
use kvartik_core::license::TRIAL_KEY_DAYS;
use kvartik_core::model::activity::IncomeKind;
use kvartik_core::model::date::Date;
use kvartik_core::model::money::{Currency, Money};
use kvartik_core::model::profile::Residency;
use kvartik_core::release::{
    ACCOUNT, BANK, EMAIL, HOLDER, LICENSES, Legal, PRICE, PRO_PRICE, PRO_SEATS, SELLER_TIN, TELEGRAM, buy_path,
    feedback_path, mailto,
};
use kvartik_core::tax::{calendar, deadlines, example, rules, turnover};

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
/// Ключ IndexNow (задача 118): файл `<ключ>.txt` в корне сайта с ним самим
/// доказывает поисковикам, что изменившиеся страницы называет хозяин сайта
/// (indexnow.org, «Documentation», вариант 1 — корень, сверено 2026-11-22).
/// Ключ не секрет — он и так лежит на сайте; чужой с ним может только
/// попросить перечитать наши же страницы. Тридцать два знака 0–9 и a–f:
/// документация называет и «шестнадцатеричные», и «a–z, A–Z, 0–9, дефис», а
/// такие годятся под оба прочтения. Сменить — новой выкладкой: поисковики
/// сверяют файл при каждом запросе. CI берёт ключ из собранного файла, а не
/// отсюда (`tools/deploy/indexnow.mjs`).
const INDEXNOW_KEY: &str = "8638b8b11062681776b6f7d9a3f2f9b9";

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

/// Папка снимков и ролика раздела «Как это выглядит» (задача 119) рядом
/// с `landing.css`: их пишет `tools/browser/media.mjs` на demo-данных,
/// а сборка кладёт в корень сайта под именем с хэшем содержимого, как CSS
/// и скрипт, — `immutable` на год (`_headers`).
const MEDIA: &str = "media";

/// Экраны на снимках — имя файла (`<имя>-<язык>-<ширина>.webp`), подпись
/// и описание (`alt`): подпись — имя экрана, как в меню программы.
const SHOTS: [(&str, Key, Key); 4] = [
    ("home", Key::NavHome, Key::LandingShotHomeAlt),
    ("invoice", Key::KindInvoice, Key::LandingShotInvoiceAlt),
    ("ledger", Key::NavLedger, Key::LandingShotLedgerAlt),
    ("tax", Key::TaxTitle, Key::LandingShotTaxAlt),
];

/// Снимок или ролик в корне сайта: адрес и размер в пикселях — размер
/// пишется в разметку, чтобы место под картинкой было до её прихода.
struct Picture {
    url: String,
    width: u32,
    height: u32,
}

/// Медиа страницы одного языка: ролик (WebM, MP4, постер) и снимки экранов
/// — телефон и компьютер — по порядку [`SHOTS`].
struct Media {
    webm: String,
    mp4: String,
    poster: Picture,
    shots: Vec<(Picture, Picture)>,
}

/// Ширина и высота картинки WebP по её заголовку (RIFF, чанк `VP8 `,
/// `VP8L` или `VP8X`) — без декодера. `None` — не WebP.
fn webp_size(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.get(0..4)? != b"RIFF" || bytes.get(8..12)? != b"WEBP" {
        return None;
    }
    let u16le = |at: usize| Some(u32::from(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?)));
    let u24le = |at: usize| {
        let b = bytes.get(at..at + 3)?;
        Some(u32::from(b[0]) | u32::from(b[1]) << 8 | u32::from(b[2]) << 16)
    };
    match bytes.get(12..16)? {
        // С потерями: после трёх байтов кадра — подпись 9d 01 2a и 14 бит размеров.
        b"VP8 " => (bytes.get(23..26)? == [0x9d, 0x01, 0x2a]).then_some(())
            .and_then(|()| Some((u16le(26)? & 0x3fff, u16le(28)? & 0x3fff))),
        // Без потерь: подпись 2f и по 14 бит на ширину и высоту минус один.
        b"VP8L" => {
            if *bytes.get(20)? != 0x2f {
                return None;
            }
            let bits = u32::from_le_bytes(bytes.get(21..25)?.try_into().ok()?);
            Some(((bits & 0x3fff) + 1, ((bits >> 14) & 0x3fff) + 1))
        }
        // Расширенный: размер холста — по 24 бита минус один.
        b"VP8X" => Some((u24le(24)? + 1, u24le(27)? + 1)),
        _ => None,
    }
}

/// Файлы корня сайта, которые не текст: имя и байты.
type Blobs = Vec<(String, Vec<u8>)>;

/// Файлы медиа страниц всех языков: что положить в корень (имя и байты)
/// и что вписать каждой странице. Нет файла, не WebP — ошибка сборки, а не
/// страница с дырой вместо снимка.
fn media(dir: &Path) -> Result<(Blobs, Vec<(Lang, Media)>), String> {
    let mut files: Blobs = Vec::new();
    let mut put = |name: &str| -> Result<(String, Vec<u8>), String> {
        let path = dir.join(name);
        let bytes = fs::read(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        let ext = name.rsplit('.').next().unwrap_or_default();
        let url = format!("kvartik-site-{}.{ext}", digest([(name, bytes.as_slice())]));
        files.push((url.clone(), bytes.clone()));
        Ok((url, bytes))
    };
    let mut pages = Vec::new();
    for lang in Lang::ALL {
        let code = lang.code();
        let mut picture = |name: String| -> Result<Picture, String> {
            let (url, bytes) = put(&name)?;
            let (width, height) = webp_size(&bytes).ok_or_else(|| format!("{MEDIA}/{name}: не WebP"))?;
            Ok(Picture { url, width, height })
        };
        let poster = picture(format!("demo-{code}.webp"))?;
        let mut shots = Vec::new();
        for (name, _, _) in SHOTS {
            shots.push((picture(format!("{name}-{code}-360.webp"))?, picture(format!("{name}-{code}-1280.webp"))?));
        }
        let webm = put(&format!("demo-{code}.webm"))?.0;
        let mp4 = put(&format!("demo-{code}.mp4"))?.0;
        pages.push((lang, Media { webm, mp4, poster, shots }));
    }
    Ok((files, pages))
}

/// Раздел «Как это выглядит» (задача 119): ролик «новый счёт → выпустить →
/// PDF» и снимки четырёх экранов. Ролик — без `autoplay` и с `controls`:
/// без скрипта его включают сами, а скрипт лендинга снимает кнопки и
/// включает его, когда он на виду, — и только если система не просит
/// меньше движения и не экономит трафик (`site/landing.js`). `preload="none"`
/// — ролик качает только тот, кто до него долистал (Правило 1), и постер
/// тоже: атрибут `poster` браузер качает сразу, где бы ролик ни стоял
/// (+60 КБ открытой страницы, замер задачи 119), поэтому он в `data-poster`,
/// а ставит его скрипт по мере прокрутки, как ленивую картинку. Снимок —
/// телефона, а с ширины колонки в 48rem — компьютера (`<picture>`), и тоже
/// по мере прокрутки (`loading="lazy"`). Данные — пример, и это сказано
/// рядом (Правило 2, «Образец назван образцом»).
fn look(lang: Lang, media: &Media) -> String {
    let text = |key: Key| escape(t(lang, key));
    let mut shots = String::new();
    for ((_, caption, alt), (phone, desktop)) in SHOTS.iter().zip(&media.shots) {
        shots.push_str(&format!(
            r#"          <li>
            <figure>
              <picture>
                <source media="(min-width: 48rem)" srcset="/{desktop}" width="{dw}" height="{dh}" />
                <img src="/{phone}" width="{pw}" height="{ph}" alt="{alt}" loading="lazy" decoding="async" />
              </picture>
              <figcaption>{caption}</figcaption>
            </figure>
          </li>
"#,
            desktop = desktop.url,
            dw = desktop.width,
            dh = desktop.height,
            phone = phone.url,
            pw = phone.width,
            ph = phone.height,
            alt = text(*alt),
            caption = text(*caption),
        ));
    }
    format!(
        r#"      <section class="look" aria-labelledby="look">
        <div class="look-head">
          <h2 id="look">{title}</h2>
          <p>{about}</p>
        </div>
        <figure class="demo">
          <video data-demo muted loop playsinline controls preload="none" data-poster="/{poster}" width="{w}" height="{h}" aria-label="{alt}">
            <source src="/{webm}" type="video/webm" />
            <source src="/{mp4}" type="video/mp4" />
          </video>
          <figcaption>{caption}</figcaption>
        </figure>
        <ul class="shots">
{shots}        </ul>
      </section>
"#,
        title = text(Key::LandingLookTitle),
        about = text(Key::LandingLookText),
        poster = media.poster.url,
        w = media.poster.width,
        h = media.poster.height,
        alt = text(Key::LandingDemoAlt),
        webm = media.webm,
        mp4 = media.mp4,
        caption = text(Key::LandingDemoCaption),
    )
}

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

/// Адрес страницы о налоге с оборота (задача 118) — в папке страницы языка,
/// как «Купить». Адрес — навсегда: на него ведут поисковики и ИИ-ответы.
pub fn tax_path(lang: Lang) -> &'static str {
    match lang {
        Lang::Am => "/tax/",
        Lang::Ru => "/ru/tax/",
        Lang::En => "/en/tax/",
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
    let (media_files, media) = media(&site.join(MEDIA))?;
    let mut files = Vec::new();
    for (lang, media) in &media {
        files.push((page_file(url(*lang)), page(*lang, &css_name, &js_name, app, calc, media, checked)));
    }
    let mut editions = 0;
    for doc in Legal::ALL {
        // Действующая редакция и прежние (задача 93; условия, п. 11) —
        // каждая на трёх языках, и переводы одной редакции держатся друг
        // друга.
        let earlier = doc.earlier();
        for edition in std::iter::once(Edition::Current(earlier)).chain(earlier.iter().map(|&day| Edition::Earlier(day))) {
            let day = match edition {
                Edition::Current(_) => None,
                Edition::Earlier(day) => Some(day),
            };
            let mut texts = Vec::new();
            for lang in Lang::ALL {
                let name = legal::file_name(doc, lang, day);
                texts.push((lang, legal::parse(&name, &read(&legal.join(&name))?)?));
            }
            let pairs: Vec<(Lang, &legal::Doc)> = texts.iter().map(|(lang, text)| (*lang, text)).collect();
            legal::same_shape(&legal::file_name(doc, Lang::Ru, day), &pairs)?;
            if doc == Legal::Terms && day.is_none() {
                agreed(&texts)?;
            }
            for (lang, text) in &texts {
                let html = legal_page(*lang, doc, text, edition, &css_name, &js_name, app, checked);
                let path = match day {
                    None => doc.path(*lang).to_owned(),
                    Some(day) => doc.earlier_path(*lang, day),
                };
                files.push((page_file(&path), html));
            }
            editions += 1;
        }
    }
    for lang in Lang::ALL {
        files.push((page_file(buy_path(lang)), buy_page(lang, &css_name, &js_name, app, checked)));
        files.push((page_file(feedback_path(lang)), feedback_page(lang, &css_name, &js_name, app, checked)));
        files.push((page_file(tax_path(lang)), tax_page(lang, &css_name, &js_name, app, checked)));
    }
    files.push(("robots.txt".to_owned(), robots()));
    files.push(("sitemap.xml".to_owned(), sitemap(checked)));
    files.push((format!("{INDEXNOW_KEY}.txt"), INDEXNOW_KEY.to_owned()));
    files.push((css_name, css));
    files.push((js_name, js));
    let names = files.iter().map(|(name, _)| name).chain(media_files.iter().map(|(name, _)| name));
    if let Some(name) = names.into_iter().find(|name| staging.join(name).exists()) {
        return Err(format!("{}: файл корня лёг бы на файл лендинга", staging.join(name).display()));
    }
    let bytes = files.iter().map(|(name, text)| (name, text.as_bytes()));
    for (name, bytes) in bytes.chain(media_files.iter().map(|(name, bytes)| (name, bytes.as_slice()))) {
        let target = staging.join(name);
        if let Some(folder) = target.parent() {
            fs::create_dir_all(folder).map_err(|error| format!("{}: {error}", folder.display()))?;
        }
        fs::write(&target, bytes).map_err(|error| format!("{}: {error}", target.display()))?;
    }
    Ok(format!(
        "лендинг: страниц {}, «Купить» {}, отзывов {}, о налоге {}, юридических текстов {}, снимков и роликов {}",
        Lang::ALL.len(),
        Lang::ALL.len(),
        Lang::ALL.len(),
        Lang::ALL.len(),
        editions * Lang::ALL.len(),
        media_files.len()
    ))
}

/// Какая редакция юридического текста на странице (задача 93; условия,
/// п. 11): действующая называет прежние днями ссылками, а прежняя говорит,
/// что она прежняя, и ведёт на действующую. Прежнюю поисковики не
/// показывают (`noindex`, и в карте сайта её нет): искавший условия должен
/// найти действующие, а прочесть свои — по ссылке.
#[derive(Clone, Copy)]
enum Edition {
    /// Действующая; дни прежних редакций — от новой к старой.
    Current(&'static [Date]),
    /// Прежняя редакция этого дня.
    Earlier(Date),
}

/// Слова согласия, которые просит написать страница «Купить»
/// (`Key::BuyAccept`), — в условиях лицензии того же языка (п. 2.1), иначе
/// сборка останавливается: покупатель написал бы согласие с редакцией,
/// которой на сайте нет. `terms` — разобранные условия трёх языков.
fn agreed(terms: &[(Lang, legal::Doc)]) -> Result<(), String> {
    for (lang, text) in terms {
        let words = t(*lang, Key::BuyAccept);
        if !legal::contains(text, words) {
            return Err(format!(
                "{}.{}.md: нет слов согласия «{words}», которые просит написать страница «Купить» — \
                 новая редакция условий? Тогда и новые слова в Key::BuyAccept (crates/core/src/i18n/)",
                Legal::Terms.name(),
                lang.code()
            ));
        }
    }
    Ok(())
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
        <p class="actions"><a class="button" href="{link}" data-handoff><span class="label">{open}</span></a></p>
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
        link = app_link(app, lang),
    )
}

/// Адрес ссылки «Открыть Kvartik» со страницы языка `lang` — с языком
/// страницы строкой запроса: `/app/?lang=ru` (полный прогон 2026-11-10,
/// п. 11). Экран первого запуска открывается на нём, а запоминает язык
/// человек — «Продолжить» или переключатель (`ui/lang.rs`); выбравшему
/// язык раньше строка ничего не меняет. Страницу программы со строкой
/// запроса отдают и хостинг, и service worker (`ignoreSearch` у переходов,
/// `crates/app/public/sw.js`).
fn app_link(app: &str, lang: Lang) -> String {
    format!("{app}?lang={}", lang.code())
}

/// Страница языка `lang`. `css` и `js` — имена файлов оформления и скрипта
/// в корне, `app` — адрес программы, `calc` — модуль калькулятора, `media` —
/// снимки и ролик её языка, `checked` — день последней сверки правил: нет
/// его — нет и строки о нём. Снимки — после карточек «Что умеет Kvartik»:
/// сразу под первым экраном Chrome качал их вместе со страницей (ленивая
/// картинка ближе ~1250 точек к окну грузится сразу), и открытый лендинг
/// весил 432 КБ вместо 128 (замер задачи 119), а ниже их качает тот, кто
/// долистал.
#[allow(clippy::too_many_arguments)]
fn page(lang: Lang, css: &str, js: &str, app: &str, calc: &Calc, media: &Media, checked: Option<Date>) -> String {
    let text = |key: Key| escape(t(lang, key));
    let trial = trial(lang);
    let calendar = format!("{SITE}/{}", calendar::path(lang));
    let webcal = calendar.replacen("https://", "webcal://", 1);
    let own = url(lang);
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
        "    <meta name=\"description\" content=\"{description}\" />\n{}{}",
        preview(lang, &title, &description, own, app),
        described(lang, app),
    );
    format!(
        r#"{head}  <body>
{top}    <main>
      <section class="hero">
        <h1>{headline}</h1>
        <p class="lead">{lead}</p>
        <p class="actions"><a class="button" href="{link}" data-handoff><span class="label">{open}</span></a></p>
        <p class="trial">{trial} {after_trial}</p>
      </section>
{calculator}      <section aria-labelledby="features">
        <h2 id="features">{features_title}</h2>
        <ul class="cards">
{features}        </ul>
      </section>
{look}      <section class="card" aria-labelledby="tax" data-tilt>
        <h2 id="tax">{tax_title}</h2>
        <p>{tax_text}</p>
        <p class="actions"><a class="button secondary" href="{tax_link}"><span class="label">{tax_open}</span></a></p>
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
        <p class="actions"><a class="button" href="{link}" data-handoff><span class="label">{open}</span></a></p>
      </section>
{price}    </main>
{footer}  </body>
</html>
"#,
        head = head(lang, &title, url, &meta, css, js, app),
        top = top(lang, own, url),
        footer = footer(lang, app, checked),
        calculator = calculator(lang, calc, app),
        look = look(lang, media),
        link = app_link(app, lang),
        price = plan_card(
            lang,
            Tariff::Standard,
            "price",
            &format!(
                "        <p class=\"actions\"><a class=\"button\" href=\"{}\"><span class=\"label\">{}</span></a></p>\n",
                buy_path(lang),
                text(Key::BuyLink)
            )
        ),
        headline = text(Key::WelcomeHeadline),
        lead = text(Key::LandingLead),
        open = text(Key::LandingOpen),
        after_trial = text(Key::WelcomeWithoutKey),
        features_title = text(Key::LandingFeaturesTitle),
        calendar_title = text(Key::LandingCalendarTitle),
        calendar_text = text(Key::LandingCalendarText),
        subscribe = text(Key::CalendarSubscribe),
        subscribe_hint = text(Key::CalendarSubscribeHint),
        fit_title = text(Key::LandingFitTitle),
        fit_text = text(Key::LandingFitText),
        tax_title = text(Key::TaxPageLink),
        tax_text = text(Key::LandingTaxPage),
        tax_open = text(Key::LandingTaxPageOpen),
        tax_link = tax_path(lang),
    )
}

/// Страница юридического текста `doc` на языке `lang` — рядом со страницей
/// языка: та же шапка, оформление и подвал, а языки ведут к тому же тексту
/// той же редакции на другом языке. Описания для поисковиков нет — его
/// заменит сам текст. Какая это редакция и куда от неё вести — `edition`.
#[allow(clippy::too_many_arguments)]
fn legal_page(
    lang: Lang,
    doc: Legal,
    text: &legal::Doc,
    edition: Edition,
    css: &str,
    js: &str,
    app: &str,
    checked: Option<Date>,
) -> String {
    let path = move |other: Lang| match edition {
        Edition::Current(_) => doc.path(other).to_owned(),
        Edition::Earlier(day) => doc.earlier_path(other, day),
    };
    let (meta, before, after) = match edition {
        Edition::Current([]) => (String::new(), String::new(), String::new()),
        Edition::Current(earlier) => {
            let links: Vec<String> = earlier
                .iter()
                .map(|&day| format!("<a href=\"{}\">{}</a>", doc.earlier_path(lang, day), escape(&i18n::date(lang, day))))
                .collect();
            let after = format!(
                "        <p class=\"editions\">{} {}</p>\n",
                escape(t(lang, Key::LegalEditions)),
                links.join(", ")
            );
            (String::new(), String::new(), after)
        }
        Edition::Earlier(_) => (
            "    <meta name=\"robots\" content=\"noindex\" />\n".to_owned(),
            format!(
                "        <p class=\"edition-note\">{} <a href=\"{}\">{}</a></p>\n",
                escape(t(lang, Key::LegalOldEdition)),
                doc.path(lang),
                escape(t(lang, Key::LegalCurrentEdition))
            ),
            String::new(),
        ),
    };
    format!(
        "{head}  <body>\n{top}    <main>\n      <article class=\"legal\">\n{before}{body}{after}      </article>\n    </main>\n{footer}  </body>\n</html>\n",
        head = head(lang, &escape(&text.title), path, &meta, css, js, app),
        top = top(lang, url(lang), path),
        body = legal::html(text, escape),
        footer = footer(lang, app, checked),
    )
}

/// Пробный ключ словами — «Пробный ключ на 30 дней — бесплатно…» (задача
/// 113), — срок из ядра (`license::TRIAL_KEY_DAYS`), экранирован.
fn trial(lang: Lang) -> String {
    let days = u64::from(TRIAL_KEY_DAYS.unsigned_abs());
    let days = fill(plural(lang, days, Key::DaysOne, Key::DaysFew, Key::DaysMany), &[&days.to_string()]);
    escape(&fill(t(lang, Key::WelcomeTrial), &[&days]))
}

/// Описание программы для поисковиков (задача 118) — JSON-LD по словарю
/// schema.org у страницы языка `lang`: программа (`SoftwareApplication`) —
/// имя, страница, описание, языки, что умеет, ключи «Стандарт» и «Pro»
/// с ценами и страницей «Купить» — и продавец (`Organization`): имя, сайт,
/// значок, почта. Текст — из таблицы строк, числа — из ядра (цены
/// `release::PRICE` и `PRO_PRICE`, валюта — её кодом в данных), как у
/// видимой страницы: описание не обещает того, чего на ней нет.
///
/// Звёзд в выдаче Google это не даёт: карточке программы ему нужны ещё
/// оценки или отзывы (`aggregateRating` или `review`; Search Central,
/// «Software App structured data», сверено 2026-11-22), а выдумывать их
/// нельзя. Зато поисковики и ИИ-ответы читают, что это за программа, сколько
/// стоит и кто продаёт, — не угадывая по тексту.
///
/// Блок данных, а не скрипт: браузер его не исполняет, и CSP его не касается
/// (шапка файла). `<`, `>` и `&` — escape-последовательностями JSON: строка
/// таблицы не закроет элемент `</script>` и не станет разметкой
/// (Правило 13). Ключи serde_json кладёт по алфавиту — те же байты на
/// каждой сборке.
fn described(lang: Lang, app: &str) -> String {
    let offer = |tariff: Tariff| {
        serde_json::json!({
            "@type": "Offer",
            "name": fill(t(lang, Key::BuyPlan), &[tariff.name(lang)]),
            "price": tariff.price().minor().to_string(),
            "priceCurrency": tariff.price().currency(),
            "url": format!("{SITE}{}", buy_path(lang)),
        })
    };
    let data = serde_json::json!({
        "@context": "https://schema.org",
        "@graph": [
            {
                "@type": "SoftwareApplication",
                "@id": format!("{SITE}/#app"),
                "name": t(lang, Key::AppName),
                "url": format!("{SITE}{}", url(lang)),
                "description": t(lang, Key::LandingDescription),
                "applicationCategory": "BusinessApplication",
                "operatingSystem": "Web",
                "inLanguage": Lang::ALL.map(Lang::code),
                "image": icon(app),
                "featureList": FEATURES.map(|(title, _)| t(lang, title)),
                "offers": [offer(Tariff::Standard), offer(Tariff::Pro)],
                "publisher": { "@id": seller_id() },
            },
            seller(lang, app),
        ],
    });
    data_block(&data)
}

/// Продавец в данных для поисковиков (`Organization`) — один у страниц
/// языков и страниц о налоге (задача 126): имя, сайт, значок, почта ядра.
/// Адрес `@id` — один на сайте, и страница о налоге называет издателем того
/// же продавца, что и программа.
fn seller(lang: Lang, app: &str) -> serde_json::Value {
    serde_json::json!({
        "@type": "Organization",
        "@id": seller_id(),
        "name": t(lang, Key::AppName),
        "url": format!("{SITE}/"),
        "logo": icon(app),
        "email": EMAIL,
    })
}

/// `@id` продавца в данных для поисковиков: адрес навсегда, как сам сайт.
fn seller_id() -> String {
    format!("{SITE}/#seller")
}

/// День записью ISO 8601 — `2026-11-25`: так его ждут schema.org
/// (`dateModified`) и карта сайта (`lastmod`).
fn iso(day: Date) -> String {
    format!("{:04}-{:02}-{:02}", day.year(), day.month(), day.day())
}

/// Значок программы для поисковиков и превью в чатах — пока единственная
/// картинка сайта в полный размер.
fn icon(app: &str) -> String {
    format!("{SITE}{app}icons/icon-512.png")
}

/// Описание страницы о налоге для поисковиков (задача 126) — JSON-LD
/// у страницы `lang`: сама страница (`WebPage`) — заголовок, описание,
/// язык, день последней сверки правил (`dateModified`, `Rules::checked`:
/// числа на ней меняются только с правилами) и издатель — продавец, тот же
/// узел, что у страницы языка (`seller`), — и путь к ней хлебными крошками
/// (`BreadcrumbList`): страница языка → о налоге. ИИ-ответы и поисковики
/// цитируют такие страницы, и день сверки им виден без разбора текста.
/// Нет дня сверки — нет и `dateModified`: страница не называет дня, которого
/// у правил нет. Блок данных, а не скрипт (`data_block`, шапка файла).
fn described_tax(lang: Lang, app: &str, checked: Option<Date>) -> String {
    let page = format!("{SITE}{}", tax_path(lang));
    let mut web_page = serde_json::json!({
        "@type": "WebPage",
        "@id": format!("{page}#page"),
        "url": page,
        "name": t(lang, Key::TaxPageTitle),
        "description": t(lang, Key::TaxPageDescription),
        "inLanguage": lang.code(),
        "publisher": { "@id": seller_id() },
        "breadcrumb": { "@id": format!("{page}#breadcrumb") },
    });
    if let Some(day) = checked {
        web_page["dateModified"] = serde_json::Value::from(iso(day));
    }
    let crumb = |position: u8, name: &str, path: &str| {
        serde_json::json!({
            "@type": "ListItem",
            "position": position,
            "name": name,
            "item": format!("{SITE}{path}"),
        })
    };
    let data = serde_json::json!({
        "@context": "https://schema.org",
        "@graph": [
            web_page,
            {
                "@type": "BreadcrumbList",
                "@id": format!("{page}#breadcrumb"),
                "itemListElement": [
                    crumb(1, t(lang, Key::AppName), url(lang)),
                    crumb(2, t(lang, Key::TaxPageLink), tax_path(lang)),
                ],
            },
            seller(lang, app),
        ],
    });
    data_block(&data)
}

/// Превью страницы в чатах и соцсетях (Open Graph): заголовок, описание,
/// адрес `path`, картинка (пока значок программы) и язык. `title`
/// и `description` уже экранированы. Одна функция у лендинга и страниц
/// о налоге (задача 126) — а у «Купить», «Отзывов» и юридических текстов
/// превью нет: их тесты держат, что `og:` там не появится.
fn preview(lang: Lang, title: &str, description: &str, path: &str, app: &str) -> String {
    let locale = match lang {
        Lang::Am => "hy_AM",
        Lang::Ru => "ru_RU",
        Lang::En => "en_US",
    };
    format!(
        r#"    <meta property="og:type" content="website" />
    <meta property="og:site_name" content="{name}" />
    <meta property="og:title" content="{title}" />
    <meta property="og:description" content="{description}" />
    <meta property="og:url" content="{SITE}{path}" />
    <meta property="og:image" content="{image}" />
    <meta property="og:locale" content="{locale}" />
"#,
        name = escape(t(lang, Key::AppName)),
        image = icon(app),
    )
}

/// Данные JSON блоком `<script type="application/ld+json">`: `<`, `>` и `&`
/// — escape-последовательностями JSON (`\u003c`…), и ни `</script>`, ни
/// `<!--` внутри не встанут, а разбор JSON вернёт те же строки.
fn data_block(data: &serde_json::Value) -> String {
    let json = data.to_string().replace('<', "\\u003c").replace('>', "\\u003e").replace('&', "\\u0026");
    format!("    <script type=\"application/ld+json\">{json}</script>\n")
}

/// Тариф на карточке (задачи 35 и 103): «Стандарт» — один ИП, «Pro» —
/// бухгалтеру на `release::PRO_SEATS` ИП. Цены и места — из ядра: условия
/// берут их со страницы «Купить» (п. 4 и 5).
#[derive(Clone, Copy)]
enum Tariff {
    Standard,
    Pro,
}

impl Tariff {
    /// Имя тарифа, как его зовут окно и условия.
    fn name(self, lang: Lang) -> &'static str {
        match self {
            Tariff::Standard => t(lang, Key::PlanStandard),
            Tariff::Pro => t(lang, Key::PlanPro),
        }
    }

    /// Цена на год.
    const fn price(self) -> Money {
        match self {
            Tariff::Standard => PRICE,
            Tariff::Pro => PRO_PRICE,
        }
    }

    /// Что даёт ключ: у «Pro» — сколько ИП и почём каждый.
    fn about(self, lang: Lang) -> String {
        match self {
            Tariff::Standard => t(lang, Key::BuyPlanText).to_owned(),
            Tariff::Pro => fill(
                t(lang, Key::BuyPlanProText),
                &[
                    &PRO_SEATS.to_string(),
                    &i18n::money(lang, per_ip()),
                    t(lang, Key::OfficeTitle),
                    t(lang, Key::PlanStandard),
                ],
            ),
        }
    }
}

/// Цена «Pro» за одно место: столько стоит ИП в ключе «Pro».
fn per_ip() -> Money {
    Money::new(PRO_PRICE.minor() / i64::from(PRO_SEATS), Currency::Amd)
}

/// Карточка тарифа `tariff` (задача 35): «Ключ «Стандарт» на год», цена из
/// ядра крупно и что даёт ключ, а под ними `below` — готовая разметка: на
/// лендинге — ссылка на страницу «Купить». `id` — якорь заголовка.
fn plan_card(lang: Lang, tariff: Tariff, id: &str, below: &str) -> String {
    format!(
        r#"      <section class="card plan" aria-labelledby="{id}" data-tilt>
        <h2 id="{id}">{title}</h2>
        <p class="price">{price}</p>
        <p>{about}</p>
{below}      </section>
"#,
        title = escape(&fill(t(lang, Key::BuyPlan), &[tariff.name(lang)])),
        price = escape(&i18n::money(lang, tariff.price())),
        about = escape(&tariff.about(lang)),
    )
}

/// Страница «Купить» на языке `lang` (задача 35) — рядом со страницей языка,
/// с её шапкой, оформлением и подвалом: карточки тарифов — «Стандарт» и «Pro»
/// (задача 103) — и пробный период под ними, шаги покупки — те, что
/// в условиях лицензии (п. 2), — и реквизиты перевода с суммой каждого тарифа. «Написать
/// письмо» открывает почту покупателя с темой, строками, которые он
/// дополнит, и словами согласия: страница сама ничего не отправляет.
/// Реквизиты — как их выдал банк, а владелец счёта и банк — по-армянски
/// (`lang="hy"`): их и вписывают в перевод, и выделяются они целиком одним
/// касанием (`copy`).
fn buy_page(lang: Lang, css: &str, js: &str, app: &str, checked: Option<Date>) -> String {
    let text = |key: Key| escape(t(lang, key));
    let (standard, pro) = (Tariff::Standard, Tariff::Pro);
    let meta = format!(
        "    <meta name=\"description\" content=\"{}\" />\n",
        escape(&fill(
            t(lang, Key::BuyDescription),
            &[
                standard.name(lang),
                &i18n::money(lang, standard.price()),
                pro.name(lang),
                &PRO_SEATS.to_string(),
                &i18n::money(lang, pro.price()),
            ]
        ))
    );
    let mail = mailto(
        t(lang, Key::BuyMailSubject),
        &fill(t(lang, Key::BuyMailBody), &[standard.name(lang), pro.name(lang), t(lang, Key::BuyAccept)]),
    );
    let write = linked(t(lang, Key::BuyStepWrite), &format!("<a href=\"mailto:{EMAIL}\">{EMAIL}</a>"));
    let pay = text(Key::BuyStepPay);
    let amount = |tariff: Tariff| {
        escape(&fill(t(lang, Key::BuyAmountPlan), &[tariff.name(lang), &i18n::money(lang, tariff.price())]))
    };
    let key = escape(&fill(t(lang, Key::BuyStepKey), &[t(lang, Key::NavProfile), t(lang, Key::LicenseTitle)]));
    let holder_note = escape(&fill(t(lang, Key::BuyHolderNote), &[SELLER_TIN]));
    format!(
        r#"{head}  <body>
{top}    <main>
      <article class="legal buy">
        <h1>{title}</h1>
{standard_card}{pro_card}        <p class="hint">{trial} {after_trial}</p>
        <h2>{how}</h2>
        <ol class="steps">
          <li>
            <p>{write}</p>
            <p class="phrase">{accept}</p>
            <p class="actions"><a class="button" href="{mail}"><span class="label">{write_button}</span></a> <a class="button secondary" href="{terms}"><span class="label">{terms_label}</span></a></p>
          </li>
          <li>
            <p>{pay}</p>
            <dl class="requisites">
              <dt>{bank_label}</dt>
              <dd><span class="copy" lang="hy">{bank}</span> <span class="note">{bank_note}</span></dd>
              <dt>{holder_label}</dt>
              <dd><span class="copy" lang="hy">{holder}</span> <span class="note">{holder_note}</span></dd>
              <dt>{account_label}</dt>
              <dd><span class="copy">{ACCOUNT}</span></dd>
              <dt>{amount_label}</dt>
              <dd>{standard_amount}</dd>
              <dd>{pro_amount}</dd>
              <dt>{purpose_label}</dt>
              <dd>{purpose}</dd>
            </dl>
          </li>
          <li>
            <p>{key}</p>
          </li>
        </ol>
{faq}        <p class="hint">{only}</p>
      </article>
    </main>
{footer}  </body>
</html>
"#,
        head = head(lang, &text(Key::BuyTitle), buy_path, &meta, css, js, app),
        top = top(lang, url(lang), buy_path),
        footer = footer(lang, app, checked),
        title = text(Key::BuyTitle),
        standard_card = plan_card(lang, standard, "plan", ""),
        pro_card = plan_card(lang, pro, "plan-pro", ""),
        trial = trial(lang),
        after_trial = text(Key::WelcomeWithoutKey),
        faq = faq(lang),
        how = text(Key::BuyHow),
        accept = text(Key::BuyAccept),
        mail = escape(&mail),
        write_button = text(Key::BuyWrite),
        terms = Legal::Terms.path(lang),
        terms_label = text(Key::LegalTerms),
        bank_label = text(Key::BuyBank),
        bank = escape(BANK),
        bank_note = text(Key::BuyBankNote),
        holder_label = text(Key::BuyHolder),
        holder = escape(HOLDER),
        account_label = text(Key::BuyAccount),
        amount_label = text(Key::BuyAmount),
        standard_amount = amount(standard),
        pro_amount = amount(pro),
        purpose_label = text(Key::BuyPurpose),
        purpose = text(Key::BuyPurposeValue),
        only = text(Key::BuyOnly),
    )
}

/// Страница «Отзывы и предложения» на языке `lang` (задача 114) — рядом со
/// страницей языка, с её шапкой, оформлением и подвалом: зачем писать,
/// «Написать письмо» — почта человека открывается с темой и строками,
/// которые он дополнит (версия программы, устройство и браузер, что
/// случилось), — Telegram продавца, что приложить и чего не присылать. Сама
/// страница ничего не отправляет. Адрес почты — и словами под кнопками:
/// без почтовой программы `mailto:` не открывает ничего.
fn feedback_page(lang: Lang, css: &str, js: &str, app: &str, checked: Option<Date>) -> String {
    let text = |key: Key| escape(t(lang, key));
    let where_version = [t(lang, Key::NavProfile), t(lang, Key::AboutTitle)];
    let meta = format!("    <meta name=\"description\" content=\"{}\" />\n", text(Key::FeedbackAbout));
    let mail = mailto(t(lang, Key::FeedbackMailSubject), &fill(t(lang, Key::FeedbackMailBody), &where_version));
    let items: String = [
        escape(&fill(t(lang, Key::FeedbackVersion), &where_version)),
        text(Key::FeedbackDevice),
        text(Key::FeedbackSteps),
        text(Key::FeedbackScreenshot),
    ]
    .iter()
    .map(|item| format!("          <li>{item}</li>\n"))
    .collect();
    format!(
        r#"{head}  <body>
{top}    <main>
      <article class="legal feedback">
        <h1>{title}</h1>
        <p>{about}</p>
        <p class="actions"><a class="button" href="{mail}"><span class="label">{write}</span></a> <a class="button secondary" href="{TELEGRAM}"><span class="label">{telegram}</span></a></p>
        <p class="hint">{mail_hint}</p>
        <h2>{what}</h2>
        <ul>
{items}        </ul>
        <h2>{not}</h2>
        <p>{not_text}</p>
      </article>
    </main>
{footer}  </body>
</html>
"#,
        head = head(lang, &text(Key::FeedbackTitle), feedback_path, &meta, css, js, app),
        top = top(lang, url(lang), feedback_path),
        footer = footer(lang, app, checked),
        title = text(Key::FeedbackTitle),
        about = text(Key::FeedbackAbout),
        mail = escape(&mail),
        write = text(Key::BuyWrite),
        telegram = text(Key::FeedbackTelegram),
        mail_hint = linked(t(lang, Key::FeedbackMailHint), &format!("<a href=\"mailto:{EMAIL}\">{EMAIL}</a>")),
        what = text(Key::FeedbackWhat),
        not = text(Key::FeedbackNot),
        not_text = text(Key::FeedbackNotText),
    )
}

/// Пример страницы о налоге (задача 118): IT — строка высоких технологий,
/// у большинства, кто ищет «налог с оборота ИП», — с оборотом три миллиона
/// драмов в квартал, гражданин Армении 1990 года рождения: у него нет
/// вопросов о пенсии и резидентстве, и годовые платежи посчитаны целиком.
/// Считает его калькулятор (`example::answer`) в день сверки правил
/// (`example::today`): страница — из тех же байтов при каждой сборке.
const TAX_EXAMPLE: (IncomeKind, i64, Residency, u16) = (IncomeKind::HighTech, 3_000_000, Residency::Citizen, 1990);

/// Страница о налоге с оборота на языке `lang` (задача 118) — рядом со
/// страницей языка, с её шапкой, оформлением и подвалом. Всё — по правилам
/// дня последней сверки (`checked`): ставки строк налога, порог, сроки
/// всех ИП с этого дня по конец следующего года (`deadlines::general`, как
/// общий календарь), пример калькулятора и курс ЦБ для дохода в валюте.
/// У каждого правила — акт, статья ссылкой на текст и день сверки
/// (`rules::Source`), по-армянски, как в законе (`lang="hy"`). Правил дня
/// нет — нет и раздела: страница не обещает того, чего нет в ядре.
///
/// Сразу под заголовком — ответ двумя предложениями (задача 126): ставки
/// «от … до …», порог и ближайший срок квартала (`tax_answer`). Его цитируют
/// поисковики и ИИ-ответы, и абзац должен отвечать сам, без остальной
/// страницы. Под ним — оглавление разделов, которые есть на странице, по их
/// якорям: адреса `#rates`, `#deadlines`, `#example`, `#currency` — навсегда,
/// на них ведут поисковики. Превью в чатах — та же функция, что у лендинга
/// (`preview`), описание для поисковиков — `described_tax`.
fn tax_page(lang: Lang, css: &str, js: &str, app: &str, checked: Option<Date>) -> String {
    let text = |key: Key| escape(t(lang, key));
    let all = rules::armenia();
    let title = text(Key::TaxPageTitle);
    let description = text(Key::TaxPageDescription);
    let meta = format!(
        "    <meta name=\"description\" content=\"{description}\" />\n{}{}",
        preview(lang, &title, &description, tax_path(lang), app),
        described_tax(lang, app, checked),
    );
    let mut body = String::new();
    if let Some(day) = checked {
        let sections = [
            ("rates", Key::TaxPageRates, tax_rates(lang, all, day)),
            ("deadlines", Key::TaxPageDeadlines, tax_deadlines(lang, day)),
            ("example", Key::TaxPageExample, tax_example(lang, day)),
            ("currency", Key::TaxPageCurrency, tax_currency(lang, all, day)),
        ];
        let present: Vec<_> = sections.iter().filter(|(_, _, html)| !html.is_empty()).collect();
        body.push_str(&tax_answer(lang, all, day));
        if !present.is_empty() {
            body.push_str(&format!(
                "        <nav class=\"contents\" aria-label=\"{}\">\n          <ul>\n",
                text(Key::TaxPageContents)
            ));
            for (id, heading, _) in &present {
                body.push_str(&format!("            <li><a href=\"#{id}\">{}</a></li>\n", text(*heading)));
            }
            body.push_str("          </ul>\n        </nav>\n");
        }
        body.push_str(&format!(
            "        <p>{}</p>\n",
            escape(&fill(t(lang, Key::TaxPageLead), &[&i18n::date(lang, day)]))
        ));
        for (id, heading, html) in present {
            body.push_str(&format!("        <h2 id=\"{id}\">{}</h2>\n{html}", text(*heading)));
        }
    }
    format!(
        r#"{head}  <body>
{top}    <main>
      <article class="legal tax">
        <h1>{title}</h1>
{body}        <p class="actions"><a class="button" href="{link}" data-handoff><span class="label">{open}</span></a></p>
        <p class="hint">{trial} {after_trial}</p>
      </article>
    </main>
{footer}  </body>
</html>
"#,
        head = head(lang, &title, tax_path, &meta, css, js, app),
        top = top(lang, url(lang), tax_path),
        footer = footer(lang, app, checked),
        link = app_link(app, lang),
        open = text(Key::LandingOpen),
        trial = trial(lang),
        after_trial = text(Key::WelcomeWithoutKey),
    )
}

/// Строки налога, действующие в день `day`, — в порядке таблицы ст. 258,
/// ч. 1, как в файле правил.
fn rates_on(all: &rules::Rules, day: Date) -> impl Iterator<Item = &rules::Rate> {
    all.turnover_rates().iter().filter(move |rate| rate.from <= day && rate.until.is_none_or(|until| day <= until))
}

/// Конец списка сроков страницы, начатого днём `day`, — 31 декабря
/// следующего года, как у общего календаря. `None` — календарь кончился.
fn deadlines_until(day: Date) -> Option<Date> {
    day.year().checked_add(1).and_then(|year| Date::new(year, 12, 31))
}

/// Ответ сверху (задача 126): что налог с оборота выбирают заявлением при
/// обороте не больше порога и что часть видов деятельности он закрывает —
/// с днём и кодами записи `turnover_exclusions` (правка человека
/// 2026-11-25), — ставки строк налога, действующих в день
/// `day`, «от … до …» — только сама ставка (`Rate::rate`), без нижнего
/// предела после вычета: предел — не ставка строки, — порог оборота за год
/// и срок расчёта и уплаты: число месяца после квартала и ближайший после
/// дня `day` квартал с днём его срока по закону. Число месяца — общее
/// правило, а не «ближайший срок»: страница собирается не каждый день,
/// и через месяц «ближайший» стал бы прошлым, а «за IV квартал — до …»
/// верно всегда. Числа — из того же ядра, что разделы ниже. Чего-то из
/// этого в правилах дня нет — нет и абзаца: половина ответа читалась бы
/// целым.
fn tax_answer(lang: Lang, all: &rules::Rules, day: Date) -> String {
    let percent = |value: rules::Percent| i18n::percent(lang, i64::from(value.hundredths()));
    let lowest = rates_on(all, day).map(|rate| rate.rate).min();
    let highest = rates_on(all, day).map(|rate| rate.rate).max();
    let threshold = rules::on(all.turnover_threshold(), day);
    // День — по закону (`by_law`), а не перенесённый: абзац цитируют без
    // списка ниже, где у перенесённого стоит «заплатить надёжнее до него»
    // (`deadlines::moved`), а перенос уплаты ՊԵԿ не подтверждал (шапка
    // `tax::deadlines`, «Перенос»). Заплативший до дня по закону не опоздал
    // ни при каком прочтении, и «до 20-го: … — до 22.01» не спорит с собой.
    let next = deadlines_until(day).and_then(|until| {
        deadlines::general(day, until).into_iter().find_map(|deadline| match deadline.kind {
            deadlines::Kind::Turnover(quarter) => Some((deadline.by_law, quarter)),
            _ => None,
        })
    });
    // Число месяца — правило того квартала, чей срок назван.
    let rule = next.and_then(|(_, quarter)| rules::on(all.turnover_due(), quarter.next()?.first_day()));
    // Коды, закрывающие налог с оборота (НК 254(3)(3.1)), — последней по дню
    // начала записью, действующей в день `day`: «в том числе с … — коды …».
    let excluded = all
        .turnover_exclusions()
        .iter()
        .filter(|record| rules::Rule::period(*record).contains(day))
        .max_by_key(|record| record.from);
    let (Some(lowest), Some(highest), Some(threshold), Some((due, quarter)), Some(rule), Some(excluded)) =
        (lowest, highest, threshold, next, rule, excluded)
    else {
        return String::new();
    };
    let codes: Vec<&str> = excluded.codes.iter().map(String::as_str).collect();
    format!(
        "        <p class=\"answer\">{}</p>\n",
        escape(&fill(
            t(lang, Key::TaxPageAnswer),
            &[
                &i18n::money(lang, Money::new(threshold.amount, Currency::Amd)),
                &i18n::date(lang, excluded.from),
                &and_list(lang, &codes),
                &percent(lowest),
                &percent(highest),
                &i18n::month_day(lang, rule.day),
                &i18n::quarter(lang, quarter),
                &i18n::date(lang, due),
            ],
        ))
    )
}

/// Перечень словами языка: «69, 70, 78.2 и 78.3», «… և 78.3», «… and 78.3»
/// (`Key::ListAnd` между двумя последними). Один — он сам, пусто — пусто.
fn and_list(lang: Lang, items: &[&str]) -> String {
    match items {
        [] => String::new(),
        [one] => (*one).to_owned(),
        [head @ .., last] => fill(t(lang, Key::ListAnd), &[&head.join(", "), last]),
    }
}

/// Ссылки на тексты записи правил: акт и статья ссылкой — у записи и у
/// каждого другого её акта (`Source::also`, задача 126), через запятую.
/// Акт — по-армянски, как в законе (`lang="hy"`); ссылка — arlis.am или
/// src.am (других `rules` не примет).
fn source_links(source: &rules::Source) -> String {
    let link = |url: &str, law: &str, article: &str| {
        format!("<a href=\"{}\" lang=\"hy\">{}, {}</a>", escape(url), escape(law), escape(article))
    };
    std::iter::once(link(&source.url, &source.law, &source.article))
        .chain(source.also.iter().map(|also| link(&also.url, &also.law, &also.article)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Откуда правило — строкой под ним: «Источник:», ссылки на тексты
/// (`source_links`) и день сверки.
fn tax_source(lang: Lang, source: &rules::Source) -> String {
    format!(
        "          <span class=\"note source\">{} {}; {}</span>\n",
        escape(t(lang, Key::TaxPageSource)),
        source_links(source),
        escape(&fill(t(lang, Key::TaxPageChecked), &[&i18n::date(lang, source.checked)]))
    )
}

/// Ставки строк налога, действующие в день `day`, — таблицей (задача 126):
/// строка налога, ставка (и до какого дня действует), вычет расходов
/// с нижним пределом и источник. Подписи столбцов — те же, что у окна
/// (`ActivityKind`, `TaxLineRate`). На телефоне строки складываются
/// в карточки (`site/landing.css`): разметка та же, а подпись столбца у
/// ячейки — из `data-label`. Под таблицей — порог оборота за год. Ставок
/// дня нет — нет и раздела.
fn tax_rates(lang: Lang, all: &rules::Rules, day: Date) -> String {
    if rates_on(all, day).next().is_none() {
        return String::new();
    }
    let text = |key: Key| escape(t(lang, key));
    let percent = |value: rules::Percent| i18n::percent(lang, i64::from(value.hundredths()));
    let (kind, rate_label, deduction_label, source_label) = (
        text(Key::ActivityKind),
        text(Key::TaxLineRate),
        text(Key::TaxPageRateDeduction),
        text(Key::TaxPageSourceColumn),
    );
    let mut out = format!(
        "        <p>{}</p>\n        <table class=\"rates\">\n          <thead>\n            <tr><th scope=\"col\">{kind}</th><th scope=\"col\">{rate_label}</th><th scope=\"col\">{deduction_label}</th><th scope=\"col\">{source_label}</th></tr>\n          </thead>\n          <tbody>\n",
        text(Key::TaxPageRatesAbout)
    );
    for rate in rates_on(all, day) {
        let mut flat = fill(t(lang, Key::TaxRateFlat), &[&percent(rate.rate)]);
        if let Some(until) = rate.until {
            flat.push_str("; ");
            flat.push_str(&fill(t(lang, Key::TaxRateUntil), &[&i18n::date(lang, until)]));
        }
        let deduction = match (rate.deduction, rate.floor) {
            (Some(deduction), Some(floor)) => {
                fill(t(lang, Key::TaxRateDeduction), &[&percent(deduction), &percent(floor)])
            }
            _ => t(lang, Key::TaxRateNoDeduction).to_owned(),
        };
        out.push_str(&format!(
            "            <tr>\n              <th scope=\"row\">{}</th>\n              <td data-label=\"{rate_label}\">{}</td>\n              <td data-label=\"{deduction_label}\">{}</td>\n              <td data-label=\"{source_label}\" class=\"source\">{}; {}</td>\n            </tr>\n",
            escape(t(lang, turnover::kind_key(rate.kind))),
            escape(&flat),
            escape(&deduction),
            source_links(&rate.source),
            escape(&fill(t(lang, Key::TaxPageChecked), &[&i18n::date(lang, rate.source.checked)]))
        ));
    }
    out.push_str("          </tbody>\n        </table>\n");
    if let Some(threshold) = rules::on(all.turnover_threshold(), day) {
        let amount = i18n::money(lang, Money::new(threshold.amount, Currency::Amd));
        out.push_str(&format!(
            "        <p>{}<br />\n{}        </p>\n",
            escape(&fill(t(lang, Key::TaxPageThreshold), &[&amount])),
            tax_source(lang, &threshold.source)
        ));
    }
    out
}

/// Сроки всех ИП на налоге с оборота с дня `day` по конец следующего года —
/// те же, что в общем календаре (`deadlines::general`), — с переносом
/// с нерабочего дня словами экрана сроков (`deadlines::moved`), у каждого —
/// источники его дня по закону (`deadlines::sources`, задача 126), — и ссылка
/// на подписку. Перенос — одной строкой под списком, с источниками каждого
/// переноса списка по разу (`deadlines::moved_sources`): у дюжины строк одна
/// и та же статья читалась бы шумом.
fn tax_deadlines(lang: Lang, day: Date) -> String {
    let Some(until) = deadlines_until(day) else {
        return String::new();
    };
    let mut out = format!(
        "        <p>{}</p>\n        <ul class=\"deadlines\">\n",
        escape(&fill(t(lang, Key::TaxPageDeadlinesAbout), &[&i18n::date(lang, day), &i18n::date(lang, until)]))
    );
    let mut shifts: Vec<&rules::Source> = Vec::new();
    for deadline in deadlines::general(day, until) {
        let moved = deadlines::moved(lang, deadline.kind, deadline.by_law)
            .map(|words| format!("<br />\n          <span class=\"note\">{}</span>", escape(&words)))
            .unwrap_or_default();
        let source: String = deadlines::sources(deadline.kind)
            .into_iter()
            .map(|source| format!("<br />\n{}", tax_source(lang, source).trim_end()))
            .collect();
        for shift in deadlines::moved_sources(deadline.by_law) {
            if !shifts.iter().any(|known| std::ptr::eq(*known, shift)) {
                shifts.push(shift);
            }
        }
        out.push_str(&format!(
            "          <li><strong>{}</strong>\u{a0}— {}{moved}{source}</li>\n",
            escape(&i18n::date(lang, deadline.day)),
            escape(&deadlines::what(lang, deadline.kind))
        ));
    }
    out.push_str("        </ul>\n");
    if !shifts.is_empty() {
        let sources: Vec<String> = shifts.iter().map(|source| tax_source(lang, source)).collect();
        out.push_str(&format!(
            "        <p>{}<br />\n{}        </p>\n",
            escape(t(lang, Key::TaxPageShift)),
            sources.join("          <br />\n")
        ));
    }
    out.push_str(&format!(
        "        <p class=\"actions\"><a class=\"button secondary\" href=\"{}#calendar\"><span class=\"label\">{}</span></a></p>\n",
        url(lang),
        escape(t(lang, Key::TaxPageCalendar))
    ));
    out
}

/// Пример (`TAX_EXAMPLE`) — ответ калькулятора лендинга словами: те же
/// блоки, что кладёт в разметку скрипт лендинга (`site/landing.js`), —
/// заголовок, сумма, строка, слова о допущениях, и «пример» в каждом
/// заголовке (CLAUDE.md, Правило 2, «Образец назван образцом»).
fn tax_example(lang: Lang, day: Date) -> String {
    let (kind, turnover, residency, born) = TAX_EXAMPLE;
    let raw = example::Raw {
        kind: example::kind_id(kind),
        turnover: turnover.to_string(),
        birth: born.to_string(),
        residency: example::residency_id(residency),
        ..example::Raw::default()
    };
    let today = example::today(None).unwrap_or(day);
    let answer = example::answer(lang, &raw, today);
    let residency_text = match residency {
        Residency::Citizen => t(lang, Key::ResidencyCitizen),
        Residency::ForeignWithStatus => t(lang, Key::ResidencyForeignWithStatus),
        Residency::ForeignWithoutStatus => t(lang, Key::ResidencyForeignWithoutStatus),
    };
    let about = fill(
        t(lang, Key::TaxPageExampleAbout),
        &[
            t(lang, turnover::kind_key(kind)),
            &i18n::money(lang, Money::new(turnover, Currency::Amd)),
            residency_text,
            &born.to_string(),
        ],
    );
    let mut out = format!("        <p>{}</p>\n        <div class=\"result\">\n", escape(&about));
    for block in answer.blocks {
        let (tag, class) = match block.kind {
            example::BlockKind::Head => ("h3", "head"),
            example::BlockKind::Sum => ("p", "sum"),
            example::BlockKind::Line => ("p", "line"),
            example::BlockKind::Note => ("p", "note"),
        };
        out.push_str(&format!("          <{tag} class=\"{class}\">{}</{tag}>\n", escape(&block.text)));
    }
    out.push_str("        </div>\n");
    out
}

/// Курс ЦБ для дохода в валюте — по записи `rate_day` дня `day`: с какого
/// дня и откуда — закон и переходное положение закона о поправке, каждый
/// своей ссылкой (`Source::also`). Записи нет — нет и раздела.
fn tax_currency(lang: Lang, all: &rules::Rules, day: Date) -> String {
    let Some(rule) = rules::on(all.rate_day(), day) else {
        return String::new();
    };
    let kind = match rule.day {
        rules::RateDayKind::PreviousWorkingDay => Key::TaxPageCurrencyAbout,
    };
    format!(
        "        <p>{}<br />\n{}        </p>\n",
        escape(&fill(t(lang, kind), &[&i18n::date(lang, rule.from)])),
        tax_source(lang, &rule.source)
    )
}

/// «Вопросы» страницы «Купить» (задача 93): сколько устройств, как перенести
/// ключ, что без него, несколько ИП и когда продлевать — каждый раскрывается
/// сам, без скрипта (`<details>`: CSP встроенных скриптов не пускает). Ответы
/// — те же строки, что на экране лицензии и экране ИП программы, и списки
/// «что даёт ключ» — те же (`i18n::LICENSE_WITH_TERM`, `LICENSE_ALWAYS`):
/// тексты не разойдутся. Числа — у карточки тарифа выше, из ядра.
fn faq(lang: Lang) -> String {
    let text = |key: Key| escape(t(lang, key));
    let list = |keys: &[Key]| {
        let items: String = keys.iter().map(|&key| format!("              <li>{}</li>\n", text(key))).collect();
        format!("            <ul>\n{items}            </ul>\n")
    };
    let paragraphs = |lines: &[String]| -> String {
        lines.iter().map(|line| format!("            <p>{line}</p>\n")).collect()
    };
    let move_backup = escape(&fill(
        t(lang, Key::LicenseMoveBackup),
        &[t(lang, Key::BackupMake), t(lang, Key::WelcomeHaveBackup), t(lang, Key::BackupRestore)],
    ));
    let move_key = escape(&fill(
        t(lang, Key::LicenseMoveKey),
        &[t(lang, Key::NavProfile), t(lang, Key::LicenseTitle), t(lang, Key::LicenseCopy)],
    ));
    let answers = [
        (Key::BuyFaqDevices, paragraphs(&[text(Key::LicenseDevicesKey), text(Key::LicenseDevicesData)])),
        (Key::BuyFaqMove, paragraphs(&[move_backup, move_key, text(Key::LicenseDevicesApple)])),
        (
            Key::BuyFaqWithout,
            format!(
                "            <p class=\"list-title\">{}</p>\n{}            <p class=\"list-title\">{}</p>\n{}",
                text(Key::LicenseWithTerm),
                list(&i18n::LICENSE_WITH_TERM),
                text(Key::LicenseAlways),
                list(&i18n::LICENSE_ALWAYS)
            ),
        ),
        (Key::BuyFaqOffice, paragraphs(&[format!("{} {}", text(Key::OfficeKeyEach), office_pro(lang))])),
        (Key::BuyFaqRenew, paragraphs(&[format!("{} {}", text(Key::BuyFaqRenewWhen), text(Key::LicenseRenewKeeps))])),
    ];
    let mut out = format!("        <h2 id=\"faq\">{}</h2>\n        <div class=\"faq\">\n", text(Key::BuyFaq));
    for (question, answer) in answers {
        out.push_str(&format!(
            "          <details>\n            <summary>{}</summary>\n{answer}          </details>\n",
            text(question)
        ));
    }
    out.push_str("        </div>\n");
    out
}

/// «Или один ключ «Pro» на 10 ИП…» (задача 103) — после «ключ у каждого ИП
/// свой» в ответе «У меня несколько ИП», экранирован.
fn office_pro(lang: Lang) -> String {
    escape(&fill(
        t(lang, Key::OfficeKeyPro),
        &[t(lang, Key::PlanPro), &PRO_SEATS.to_string(), t(lang, Key::OfficeTitle)],
    ))
}

/// Шаблон с одним `{}`, на месте которого — готовая разметка `link`:
/// текст вокруг экранирован, а сама ссылка — нет.
fn linked(template: &str, link: &str) -> String {
    match template.split_once("{}") {
        Some((before, after)) => format!("{}{link}{}", escape(before), escape(after)),
        None => escape(template),
    }
}

/// Начало документа до `<body>`: язык, заголовок, та же страница на каждом
/// языке (`path`) — своя каноническая, армянская «по умолчанию», — `meta` —
/// описание страницы, если оно есть, значки, оформление и скрипт. `title`
/// уже экранирован.
fn head<P: std::fmt::Display>(
    lang: Lang,
    title: &str,
    path: impl Fn(Lang) -> P,
    meta: &str,
    css: &str,
    js: &str,
    app: &str,
) -> String {
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
fn top<P: std::fmt::Display>(lang: Lang, home: &str, path: impl Fn(Lang) -> P) -> String {
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
/// нет и строки), страница «Купить» и юридические тексты своего языка —
/// условия зовут её по имени, и с их страницы до неё один шаг (задача 35), —
/// тексты лицензий рядом с программой и как написать продавцу — почтой,
/// в Telegram (`release::EMAIL`, `release::TELEGRAM`) и страницей «Отзывы
/// и предложения» (задача 114).
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
{rules}      <p class="links"><a href="{tax}">{tax_label}</a> <a href="{buy}">{buy_label}</a> <a href="{terms}">{terms_label}</a> <a href="{privacy}">{privacy_label}</a> <a href="{app}{LICENSES}">{licenses}</a></p>
      <p class="links"><span>{contact}</span> <a href="mailto:{EMAIL}">{EMAIL}</a> <a href="{TELEGRAM}">{telegram}</a> <a href="{feedback}">{feedback_label}</a></p>
    </footer>
"#,
        tax = tax_path(lang),
        tax_label = text(Key::TaxPageLink),
        buy = buy_path(lang),
        feedback = feedback_path(lang),
        feedback_label = text(Key::FeedbackTitle),
        buy_label = text(Key::BuyLink),
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

/// Карта сайта: страницы трёх языков, страницы о налоге (задача 118),
/// «Купить» и «Отзывы и предложения» и юридические тексты к ним, у каждой — она же на всех
/// трёх и «по умолчанию» армянская (как
/// `hreflang` в самих страницах). Дня сборки в ней нет, иначе одна и та же
/// сборка давала бы разные байты. А у страниц о налоге `lastmod` — день
/// последней сверки правил `checked` (задача 126): их числа меняются только
/// с правилами, и день берётся из данных, а не из часов, — те же байты на
/// каждой сборке. Нет дня — нет и `lastmod`.
fn sitemap(checked: Option<Date>) -> String {
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\" xmlns:xhtml=\"http://www.w3.org/1999/xhtml\">\n",
    );
    let terms = |lang: Lang| Legal::Terms.path(lang);
    let privacy = |lang: Lang| Legal::Privacy.path(lang);
    let ruled = checked.map(|day| format!("    <lastmod>{}</lastmod>\n", iso(day))).unwrap_or_default();
    let pages: [(&dyn Fn(Lang) -> &'static str, &str); 6] = [
        (&url, ""),
        (&tax_path, &ruled),
        (&buy_path, ""),
        (&feedback_path, ""),
        (&terms, ""),
        (&privacy, ""),
    ];
    for (path, lastmod) in pages {
        for lang in Lang::ALL {
            out.push_str(&format!("  <url>\n    <loc>{SITE}{}</loc>\n{lastmod}", path(lang)));
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
    use kvartik_core::release::mailto_part;
    use std::path::PathBuf;

    /// Настоящие файлы: проверка ловит правку, после которой сборка их не
    /// узнала бы.
    const TOKENS_CSS: &str = include_str!("../../app/style/tokens.css");
    const FONTS_CSS: &str = include_str!("../../app/style/fonts.css");
    const LANDING_CSS: &str = include_str!("../../app/site/landing.css");
    const LANDING_JS: &str = include_str!("../../app/site/landing.js");
    /// Утверждённые юридические тексты (задача 32) и прежние редакции условий
    /// (задачи 93, 99, 103, 106, 102 и 113) и политики (задачи 106, 102, 113
    /// и 115) — те, что берёт сборка.
    const LEGAL: [(&str, &str); 36] = [
        ("terms.hy.md", include_str!("../../../docs/legal/terms.hy.md")),
        ("terms.ru.md", include_str!("../../../docs/legal/terms.ru.md")),
        ("terms.en.md", include_str!("../../../docs/legal/terms.en.md")),
        ("terms.2026-11-20.hy.md", include_str!("../../../docs/legal/terms.2026-11-20.hy.md")),
        ("terms.2026-11-20.ru.md", include_str!("../../../docs/legal/terms.2026-11-20.ru.md")),
        ("terms.2026-11-20.en.md", include_str!("../../../docs/legal/terms.2026-11-20.en.md")),
        ("terms.2026-11-18.hy.md", include_str!("../../../docs/legal/terms.2026-11-18.hy.md")),
        ("terms.2026-11-18.ru.md", include_str!("../../../docs/legal/terms.2026-11-18.ru.md")),
        ("terms.2026-11-18.en.md", include_str!("../../../docs/legal/terms.2026-11-18.en.md")),
        ("terms.2026-11-17.hy.md", include_str!("../../../docs/legal/terms.2026-11-17.hy.md")),
        ("terms.2026-11-17.ru.md", include_str!("../../../docs/legal/terms.2026-11-17.ru.md")),
        ("terms.2026-11-17.en.md", include_str!("../../../docs/legal/terms.2026-11-17.en.md")),
        ("terms.2026-11-15.hy.md", include_str!("../../../docs/legal/terms.2026-11-15.hy.md")),
        ("terms.2026-11-15.ru.md", include_str!("../../../docs/legal/terms.2026-11-15.ru.md")),
        ("terms.2026-11-15.en.md", include_str!("../../../docs/legal/terms.2026-11-15.en.md")),
        ("terms.2026-11-13.hy.md", include_str!("../../../docs/legal/terms.2026-11-13.hy.md")),
        ("terms.2026-11-13.ru.md", include_str!("../../../docs/legal/terms.2026-11-13.ru.md")),
        ("terms.2026-11-13.en.md", include_str!("../../../docs/legal/terms.2026-11-13.en.md")),
        ("terms.2026-11-09.hy.md", include_str!("../../../docs/legal/terms.2026-11-09.hy.md")),
        ("terms.2026-11-09.ru.md", include_str!("../../../docs/legal/terms.2026-11-09.ru.md")),
        ("terms.2026-11-09.en.md", include_str!("../../../docs/legal/terms.2026-11-09.en.md")),
        ("privacy.hy.md", include_str!("../../../docs/legal/privacy.hy.md")),
        ("privacy.ru.md", include_str!("../../../docs/legal/privacy.ru.md")),
        ("privacy.en.md", include_str!("../../../docs/legal/privacy.en.md")),
        ("privacy.2026-11-21.hy.md", include_str!("../../../docs/legal/privacy.2026-11-21.hy.md")),
        ("privacy.2026-11-21.ru.md", include_str!("../../../docs/legal/privacy.2026-11-21.ru.md")),
        ("privacy.2026-11-21.en.md", include_str!("../../../docs/legal/privacy.2026-11-21.en.md")),
        ("privacy.2026-11-20.hy.md", include_str!("../../../docs/legal/privacy.2026-11-20.hy.md")),
        ("privacy.2026-11-20.ru.md", include_str!("../../../docs/legal/privacy.2026-11-20.ru.md")),
        ("privacy.2026-11-20.en.md", include_str!("../../../docs/legal/privacy.2026-11-20.en.md")),
        ("privacy.2026-11-18.hy.md", include_str!("../../../docs/legal/privacy.2026-11-18.hy.md")),
        ("privacy.2026-11-18.ru.md", include_str!("../../../docs/legal/privacy.2026-11-18.ru.md")),
        ("privacy.2026-11-18.en.md", include_str!("../../../docs/legal/privacy.2026-11-18.en.md")),
        ("privacy.2026-11-09.hy.md", include_str!("../../../docs/legal/privacy.2026-11-09.hy.md")),
        ("privacy.2026-11-09.ru.md", include_str!("../../../docs/legal/privacy.2026-11-09.ru.md")),
        ("privacy.2026-11-09.en.md", include_str!("../../../docs/legal/privacy.2026-11-09.en.md")),
    ];

    /// Страница юридического текста из утверждённого файла: действующей
    /// редакции или прежней дня `day`.
    fn legal_sample(doc: Legal, lang: Lang, day: Option<Date>) -> String {
        let name = legal::file_name(doc, lang, day);
        let (_, source) = LEGAL.iter().find(|(file, _)| *file == name).expect("текст есть");
        let text = legal::parse(&name, source).expect("текст разбирается");
        let edition = day.map_or(Edition::Current(doc.earlier()), Edition::Earlier);
        legal_page(lang, doc, &text, edition, "kvartik-site-a.css", "kvartik-site-b.js", "/app/", checked())
    }

    /// Все страницы сайта: три страницы языков, три «Купить», три страницы
    /// отзывов, шесть юридических и прежние редакции.
    fn every_page() -> Vec<(String, String)> {
        let mut out = Vec::new();
        for lang in Lang::ALL {
            out.push((format!("{lang:?} {}", url(lang)), sample(lang)));
            out.push((format!("{lang:?} {}", buy_path(lang)), buy_sample(lang)));
            out.push((format!("{lang:?} {}", feedback_path(lang)), feedback_sample(lang)));
            out.push((format!("{lang:?} {}", tax_path(lang)), tax_sample(lang)));
            for doc in Legal::ALL {
                out.push((format!("{lang:?} {}", doc.path(lang)), legal_sample(doc, lang, None)));
                for &day in doc.earlier() {
                    out.push((format!("{lang:?} {}", doc.earlier_path(lang, day)), legal_sample(doc, lang, Some(day))));
                }
            }
        }
        out
    }

    /// Прежняя редакция (задача 93; условия, п. 11) — по своему адресу на
    /// каждом языке: канонический адрес и переводы — её же, а не
    /// действующей; страница говорит, что она прежняя, ведёт на действующую
    /// и просит поисковики её не показывать. Действующая называет прежние
    /// днями ссылками, а в карте сайта прежних нет. У условий и, с задачи
    /// 106, у политики конфиденциальности.
    #[test]
    fn earlier_editions_stay_at_their_own_addresses() {
        let mut wrong = Vec::new();
        for doc in Legal::ALL {
            let days = doc.earlier();
            assert!(!days.is_empty(), "у {doc:?} есть прежняя редакция");
            for lang in Lang::ALL {
                let current = legal_sample(doc, lang, None);
                for &day in days {
                    let at = doc.earlier_path(lang, day);
                    let link = format!("<a href=\"{at}\">{}</a>", escape(&i18n::date(lang, day)));
                    if !current.contains(&format!("<p class=\"editions\">{} ", escape(t(lang, Key::LegalEditions))))
                        || !current.contains(&link)
                    {
                        wrong.push(format!("{doc:?} {lang:?}: действующая не ведёт на прежнюю {link}"));
                    }
                    let old = legal_sample(doc, lang, Some(day));
                    let mut need = vec![
                        format!("<link rel=\"canonical\" href=\"https://kvartik.com{at}\" />"),
                        "<meta name=\"robots\" content=\"noindex\" />".to_owned(),
                        format!(
                            "<p class=\"edition-note\">{} <a href=\"{}\">{}</a></p>",
                            escape(t(lang, Key::LegalOldEdition)),
                            doc.path(lang),
                            escape(t(lang, Key::LegalCurrentEdition))
                        ),
                    ];
                    for other in Lang::ALL {
                        let there = doc.earlier_path(other, day);
                        need.push(format!("<link rel=\"alternate\" hreflang=\"{}\" href=\"https://kvartik.com{there}\" />", other.code()));
                        need.push(format!("<a href=\"{there}\" hreflang=\"{}\"", other.code()));
                    }
                    for piece in need {
                        if !old.contains(&piece) {
                            wrong.push(format!("{lang:?} {at}: нет {piece}"));
                        }
                    }
                    if old.contains("class=\"editions\"") || current.contains("noindex") || current.contains("edition-note") {
                        wrong.push(format!("{doc:?} {lang:?}: прежняя и действующая перепутали свои слова"));
                    }
                    if sitemap(checked()).contains(&at) {
                        wrong.push(format!("{at}: прежняя редакция в карте сайта"));
                    }
                }
            }
        }
        assert!(wrong.is_empty(), "{wrong:#?}");
    }

    /// «Вопросы» страницы «Купить» (задача 93) — пять, каждый раскрывается
    /// без скрипта, а ответы — те же строки, что в программе: экрана лицензии
    /// (ключ на ИНН, данные не переходят сами, перенос копией и ключом,
    /// Safari и программа с «Домой» — два места, оба списка «что даёт ключ»)
    /// и экрана ИП. «Когда продлевать?» — с днями прежнего ключа (условия,
    /// п. 4.1, редакция от 13.11.2026). Под карточкой тарифа — что работает
    /// после пробного периода.
    #[test]
    fn the_buy_page_answers_the_questions_in_the_app_s_words() {
        let mut wrong = Vec::new();
        for lang in Lang::ALL {
            let html = buy_sample(lang);
            let Some(at) = html.find("<h2 id=\"faq\">") else {
                wrong.push(format!("{lang:?}: нет вопросов"));
                continue;
            };
            let faq = &html[at..];
            if faq.matches("<details>").count() != 5 || faq.matches("<summary>").count() != 5 {
                wrong.push(format!("{lang:?}: вопросов не пять"));
            }
            let text = |key: Key| escape(t(lang, key));
            let mut need = vec![
                format!("<h2 id=\"faq\">{}</h2>", text(Key::BuyFaq)),
                escape(&fill(
                    t(lang, Key::LicenseMoveBackup),
                    &[t(lang, Key::BackupMake), t(lang, Key::WelcomeHaveBackup), t(lang, Key::BackupRestore)],
                )),
                escape(&fill(
                    t(lang, Key::LicenseMoveKey),
                    &[t(lang, Key::NavProfile), t(lang, Key::LicenseTitle), t(lang, Key::LicenseCopy)],
                )),
                format!("{} {}", text(Key::OfficeKeyEach), office_pro(lang)),
                format!("{} {}", text(Key::BuyFaqRenewWhen), text(Key::LicenseRenewKeeps)),
            ];
            for question in [Key::BuyFaqDevices, Key::BuyFaqMove, Key::BuyFaqWithout, Key::BuyFaqOffice, Key::BuyFaqRenew] {
                need.push(format!("<summary>{}</summary>", text(question)));
            }
            for key in [Key::LicenseDevicesKey, Key::LicenseDevicesData, Key::LicenseDevicesApple, Key::LicenseWithTerm, Key::LicenseAlways] {
                need.push(text(key));
            }
            for key in i18n::LICENSE_WITH_TERM.into_iter().chain(i18n::LICENSE_ALWAYS) {
                need.push(format!("<li>{}</li>", text(key)));
            }
            for piece in need {
                if !faq.contains(&piece) {
                    wrong.push(format!("{lang:?}: в вопросах нет {piece}"));
                }
            }
            let plan = &html[html.find("<section class=\"card plan\"").expect("карточка тарифа")..at];
            if !plan.contains(&format!("{} {}", trial(lang), text(Key::WelcomeWithoutKey))) {
                wrong.push(format!("{lang:?}: под тарифом не сказано о пробном ключе и что без ключа"));
            }
        }
        assert!(wrong.is_empty(), "{wrong:#?}");
    }

    /// Страница «Купить».
    fn buy_sample(lang: Lang) -> String {
        buy_page(lang, "kvartik-site-a.css", "kvartik-site-b.js", "/app/", checked())
    }

    /// Страница «Отзывы и предложения» (задача 114).
    fn feedback_sample(lang: Lang) -> String {
        feedback_page(lang, "kvartik-site-a.css", "kvartik-site-b.js", "/app/", checked())
    }

    /// Страница «Отзывы и предложения» (задача 114) — на своём языке, рядом
    /// со страницей языка: зачем писать, письмо продавцу с темой и строками,
    /// которые человек дополнит, — версия программы с тем, где её найти, —
    /// Telegram продавца, адрес почты словами, что приложить и чего не
    /// присылать.
    #[test]
    fn the_feedback_page_leads_to_the_seller_with_the_version() {
        let mut wrong = Vec::new();
        for lang in Lang::ALL {
            let html = feedback_sample(lang);
            let text = |key: Key| escape(t(lang, key));
            let where_version = [t(lang, Key::NavProfile), t(lang, Key::AboutTitle)];
            let mut need = vec![
                format!("<html lang=\"{}\">", lang.code()),
                format!("<title>{}</title>", text(Key::FeedbackTitle)),
                format!("<h1>{}</h1>", text(Key::FeedbackTitle)),
                format!("<meta name=\"description\" content=\"{}\" />", text(Key::FeedbackAbout)),
                format!("<link rel=\"canonical\" href=\"https://kvartik.com{}\" />", feedback_path(lang)),
                "<link rel=\"alternate\" hreflang=\"x-default\" href=\"https://kvartik.com/feedback/\" />".to_owned(),
                format!("<a class=\"brand\" href=\"{}\">", url(lang)),
                "<article class=\"legal feedback\">".to_owned(),
                format!("<p>{}</p>", text(Key::FeedbackAbout)),
                format!(
                    "<a class=\"button secondary\" href=\"{TELEGRAM}\"><span class=\"label\">{}</span></a>",
                    text(Key::FeedbackTelegram)
                ),
                linked(t(lang, Key::FeedbackMailHint), &format!("<a href=\"mailto:{EMAIL}\">{EMAIL}</a>")),
                format!("<h2>{}</h2>", text(Key::FeedbackWhat)),
                format!("<li>{}</li>", escape(&fill(t(lang, Key::FeedbackVersion), &where_version))),
                format!("<li>{}</li>", text(Key::FeedbackDevice)),
                format!("<li>{}</li>", text(Key::FeedbackSteps)),
                format!("<li>{}</li>", text(Key::FeedbackScreenshot)),
                format!("<h2>{}</h2>", text(Key::FeedbackNot)),
                format!("<p>{}</p>", text(Key::FeedbackNotText)),
            ];
            for other in Lang::ALL {
                need.push(format!(
                    "<link rel=\"alternate\" hreflang=\"{}\" href=\"https://kvartik.com{}\" />",
                    other.code(),
                    feedback_path(other)
                ));
                need.push(format!("<a href=\"{}\" hreflang=\"{}\"", feedback_path(other), other.code()));
            }
            for piece in need {
                if !html.contains(&piece) {
                    wrong.push(format!("{lang:?}: нет {piece}"));
                }
            }
            if html.matches("aria-current=\"page\"").count() != 1 {
                wrong.push(format!("{lang:?}: текущий язык не один"));
            }
            // Письмо — адресом продавца, с темой и строками, первая из
            // которых — версия и где её найти; `&` между ними — ссылкой на
            // знак (Правило 13).
            let mail = format!(
                "<a class=\"button\" href=\"mailto:{EMAIL}?subject={}&amp;body={}\"><span class=\"label\">{}</span></a>",
                mailto_part(t(lang, Key::FeedbackMailSubject)),
                mailto_part(&fill(t(lang, Key::FeedbackMailBody), &where_version)),
                text(Key::BuyWrite)
            );
            if !html.contains(&mail) {
                wrong.push(format!("{lang:?}: нет письма {mail}"));
            }
            let body = fill(t(lang, Key::FeedbackMailBody), &where_version);
            if !body.contains(t(lang, Key::NavProfile)) || !body.contains(t(lang, Key::AboutTitle)) || body.lines().count() != 3 {
                wrong.push(format!("{lang:?}: в письме не три строки или не сказано, где версия: {body}"));
            }
            if html.contains("og:") || html.contains("{}") || html.contains("<form") {
                wrong.push(format!("{lang:?}: разметка лендинга, незаполненный шаблон или форма"));
            }
        }
        assert!(wrong.is_empty(), "{wrong:#?}");
    }

    /// Модуль калькулятора в тестах страниц.
    const CALC: Calc = Calc {
        module: "kvartik-site-calc-c.js",
        wasm: "kvartik-site-calc-c_bg.wasm",
    };

    /// Медиа страницы для проверок разметки — имена, как у сборки.
    static MEDIA_SAMPLE: std::sync::LazyLock<Media> = std::sync::LazyLock::new(|| {
        let picture = |n: u8, width, height| Picture { url: format!("kvartik-site-{n:016x}.webp"), width, height };
        Media {
            webm: "kvartik-site-00000000000000aa.webm".to_owned(),
            mp4: "kvartik-site-00000000000000bb.mp4".to_owned(),
            poster: picture(0, 720, 1560),
            shots: (1..=4).map(|n| (picture(n, 720, 1560), picture(n + 10, 1280, 800))).collect(),
        }
    });

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
            // Ссылка в программу — с языком страницы (полный прогон
            // 2026-11-10, п. 11), и голой `/app/` не осталось.
            let link = format!("href=\"/app/?lang={}\"", lang.code());
            let links = html.matches(&link).count();
            if links != 3 || html.matches(&format!("{link} data-handoff>")).count() != links {
                wrong.push(format!("{lang:?}: не у всех трёх ссылок в программу есть data-handoff и язык"));
            }
            if html.contains("href=\"/app/\"") {
                wrong.push(format!("{lang:?}: ссылка в программу без языка страницы"));
            }
        }
        assert!(wrong.is_empty(), "{wrong:#?}");
    }

    fn sample(lang: Lang) -> String {
        page(lang, "kvartik-site-a.css", "kvartik-site-b.js", "/app/", &CALC, &MEDIA_SAMPLE, checked())
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
                format!("<a class=\"button\" href=\"/app/?lang={}\" data-handoff>", lang.code()),
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
                let html = legal_sample(doc, lang, None);
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

    /// Подвал каждой страницы ведёт на страницу «Купить» и оба юридических
    /// текста своего языка, на тексты лицензий и к продавцу — почтой,
    /// в Telegram и страницей отзывов (задача 114).
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
                format!("<a href=\"{}\">{}</a>", buy_path(lang), escape(t(lang, Key::BuyLink))),
                format!("<a href=\"{}\">{}</a>", Legal::Terms.path(lang), escape(t(lang, Key::LegalTerms))),
                format!("<a href=\"{}\">{}</a>", Legal::Privacy.path(lang), escape(t(lang, Key::LegalPrivacy))),
                format!("<a href=\"/app/{LICENSES}\">"),
                format!("<a href=\"mailto:{EMAIL}\">{EMAIL}</a>"),
                format!("<a href=\"{TELEGRAM}\">{}</a>", escape(t(lang, Key::ContactTelegram))),
                format!("<a href=\"{}\">{}</a>", feedback_path(lang), escape(t(lang, Key::FeedbackTitle))),
                format!("<a href=\"{}\">{}</a>", tax_path(lang), escape(t(lang, Key::TaxPageLink))),
                escape(t(lang, Key::LandingDisclaimer)),
            ] {
                if !footer.contains(&piece) {
                    wrong.push(format!("{name}: в подвале нет {piece}"));
                }
            }
        }
        assert!(wrong.is_empty(), "{wrong:#?}");
    }

    /// Страница о налоге на языке `lang`, как её пишет сборка.
    fn tax_sample(lang: Lang) -> String {
        tax_page(lang, "kvartik-site-a.css", "kvartik-site-b.js", "/app/", checked())
    }

    /// Кусок `html` от первого `start` до ближайшего после него `end`
    /// включительно — строка таблицы или пункт списка, чтобы искать в нём,
    /// а не во всей странице.
    fn piece<'a>(html: &'a str, start: &str, end: &str) -> Option<&'a str> {
        let at = html.find(start)?;
        let to = html[at..].find(end)? + at + end.len();
        Some(&html[at..to])
    }

    /// Ссылка страницы на текст акта записи правил — как её пишет
    /// `source_links`, без него.
    fn law_link(url: &str, law: &str, article: &str) -> String {
        format!("<a href=\"{}\" lang=\"hy\">{}, {}</a>", escape(url), escape(law), escape(article))
    }

    /// Страница о налоге (задачи 118 и 126) — на своём языке, по адресу
    /// в папке страницы языка, и всё на ней — из ядра. Сразу под заголовком —
    /// ответ: самая низкая и самая высокая ставка строк дня сверки, порог
    /// и ближайший срок квартала с кварталом; под ним — оглавление по якорям
    /// разделов, которые есть на странице. Ставки — таблицей: у строки
    /// налога — ставка, вычет с нижним пределом из файла правил и ссылка на
    /// свой источник; порог — суммой правил; сроки — те же, что у общего
    /// календаря, у каждого — ссылки на источники его записей
    /// (`deadlines::sources`), а у перенесённых — строка о переносе с его
    /// источниками; пример — ответ калькулятора на тот же ввод; курс ЦБ —
    /// со ссылками на кодекс и на закон о поправке. Превью в чатах — её
    /// заголовок и адрес. Лендинг ведёт на неё карточкой. Без дня сверки нет
    /// ни ответа, ни оглавления, ни разделов: нет и правил, по которым их
    /// писать.
    #[test]
    fn the_tax_page_takes_every_number_from_the_core() {
        let all = rules::armenia();
        let day = checked().expect("правила сверены");
        let until = Date::new(day.year() + 1, 12, 31).expect("день");
        let mut wrong = Vec::new();
        for lang in Lang::ALL {
            let html = tax_sample(lang);
            let percent = |value: rules::Percent| i18n::percent(lang, i64::from(value.hundredths()));
            let text = |key: Key| escape(t(lang, key));
            let mut need = vec![
                format!("<html lang=\"{}\">", lang.code()),
                format!("<link rel=\"canonical\" href=\"https://kvartik.com{}\" />", tax_path(lang)),
                escape(&fill(t(lang, Key::TaxPageLead), &[&i18n::date(lang, day)])),
                format!("<meta property=\"og:url\" content=\"https://kvartik.com{}\" />", tax_path(lang)),
                format!("<meta property=\"og:title\" content=\"{}\" />", text(Key::TaxPageTitle)),
                format!("<meta property=\"og:description\" content=\"{}\" />", text(Key::TaxPageDescription)),
                "<meta property=\"og:image\" content=\"https://kvartik.com/app/icons/icon-512.png\" />".to_owned(),
            ];
            let mut rates: Vec<_> =
                all.turnover_rates().iter().filter(|r| r.from <= day && r.until.is_none_or(|u| day <= u)).collect();
            if rates.len() < 10 {
                wrong.push(format!("{lang:?}: ставок дня сверки {} — меньше строк таблицы", rates.len()));
            }
            // Ответ сверху — первым абзацем под заголовком, а под ним
            // оглавление: «от» — самая низкая ставка строк, «до» — самая
            // высокая, срок — первый срок квартала после дня сверки.
            let terms = deadlines::general(day, until);
            let (due, quarter) = terms
                .iter()
                .find_map(|deadline| match deadline.kind {
                    deadlines::Kind::Turnover(quarter) => Some((deadline.by_law, quarter)),
                    _ => None,
                })
                .expect("срок квартала");
            let mut by_rate = rates.clone();
            by_rate.sort_by_key(|rate| rate.rate);
            let threshold = rules::on(all.turnover_threshold(), day).expect("порог");
            let amount = i18n::money(lang, Money::new(threshold.amount, Currency::Amd));
            let month_day = rules::on(all.turnover_due(), quarter.next().expect("квартал").first_day()).expect("срок").day;
            let excluded = all.turnover_exclusions().last().expect("коды, закрывающие налог с оборота");
            let codes: Vec<&str> = excluded.codes.iter().map(String::as_str).collect();
            let answer = escape(&fill(
                t(lang, Key::TaxPageAnswer),
                &[
                    &amount,
                    &i18n::date(lang, excluded.from),
                    &and_list(lang, &codes),
                    &percent(by_rate[0].rate),
                    &percent(by_rate[by_rate.len() - 1].rate),
                    &i18n::month_day(lang, month_day),
                    &i18n::quarter(lang, quarter),
                    &i18n::date(lang, due),
                ],
            ));
            need.push(format!(
                "<h1>{}</h1>\n        <p class=\"answer\">{answer}</p>\n        <nav class=\"contents\" aria-label=\"{}\">",
                text(Key::TaxPageTitle),
                text(Key::TaxPageContents)
            ));
            for (id, heading) in [
                ("rates", Key::TaxPageRates),
                ("deadlines", Key::TaxPageDeadlines),
                ("example", Key::TaxPageExample),
                ("currency", Key::TaxPageCurrency),
            ] {
                need.push(format!("<li><a href=\"#{id}\">{}</a></li>", text(heading)));
                need.push(format!("<h2 id=\"{id}\">{}</h2>", text(heading)));
            }
            need.push(format!(
                "<tr><th scope=\"col\">{}</th><th scope=\"col\">{}</th><th scope=\"col\">{}</th><th scope=\"col\">{}</th></tr>",
                text(Key::ActivityKind),
                text(Key::TaxLineRate),
                text(Key::TaxPageRateDeduction),
                text(Key::TaxPageSourceColumn)
            ));
            // Строка таблицы — у каждой ставки дня, в порядке файла правил.
            let mut after = 0;
            for rate in rates.drain(..) {
                let head = format!("<th scope=\"row\">{}</th>", escape(t(lang, turnover::kind_key(rate.kind))));
                let Some(row) = piece(&html[after..], &head, "</tr>") else {
                    wrong.push(format!("{lang:?}: нет строки {head} после прежней"));
                    continue;
                };
                after += html[after..].find(&head).unwrap_or_default();
                let mut flat = fill(t(lang, Key::TaxRateFlat), &[&percent(rate.rate)]);
                if let Some(until) = rate.until {
                    flat = format!("{flat}; {}", fill(t(lang, Key::TaxRateUntil), &[&i18n::date(lang, until)]));
                }
                let deduction = match (rate.deduction, rate.floor) {
                    (Some(deduction), Some(floor)) => {
                        fill(t(lang, Key::TaxRateDeduction), &[&percent(deduction), &percent(floor)])
                    }
                    _ => t(lang, Key::TaxRateNoDeduction).to_owned(),
                };
                let cells = [
                    format!("<td data-label=\"{}\">{}</td>", text(Key::TaxLineRate), escape(&flat)),
                    format!("<td data-label=\"{}\">{}</td>", text(Key::TaxPageRateDeduction), escape(&deduction)),
                    format!(
                        "<td data-label=\"{}\" class=\"source\">{}; {}</td>",
                        text(Key::TaxPageSourceColumn),
                        std::iter::once(law_link(&rate.source.url, &rate.source.law, &rate.source.article))
                            .chain(rate.source.also.iter().map(|also| law_link(&also.url, &also.law, &also.article)))
                            .collect::<Vec<_>>()
                            .join(", "),
                        escape(&fill(t(lang, Key::TaxPageChecked), &[&i18n::date(lang, rate.source.checked)]))
                    ),
                ];
                for cell in cells {
                    if !row.contains(&cell) {
                        wrong.push(format!("{lang:?}: в строке {head} нет {cell}"));
                    }
                }
            }
            need.push(escape(&fill(t(lang, Key::TaxPageThreshold), &[&amount])));
            // Сроки — у каждого ссылка на источник его записи, а перенос —
            // строкой под списком с источниками каждого переноса.
            if terms.len() < 4 {
                wrong.push(format!("{lang:?}: сроков {} — меньше четырёх кварталов", terms.len()));
            }
            let mut shifts = Vec::new();
            for deadline in &terms {
                let head = format!(
                    "<li><strong>{}</strong>\u{a0}— {}",
                    escape(&i18n::date(lang, deadline.day)),
                    escape(&deadlines::what(lang, deadline.kind))
                );
                let Some(item) = piece(&html, &head, "</li>") else {
                    wrong.push(format!("{lang:?}: нет срока {head}"));
                    continue;
                };
                let sources = deadlines::sources(deadline.kind);
                if sources.is_empty() {
                    wrong.push(format!("{lang:?}: у срока {:?} нет записи правил", deadline.kind));
                }
                if item.matches("<span class=\"note source\">").count() != sources.len() {
                    wrong.push(format!("{lang:?}: у срока {head} источников не {}", sources.len()));
                }
                for source in sources {
                    let link = law_link(&source.url, &source.law, &source.article);
                    if !item.contains(&link) {
                        wrong.push(format!("{lang:?}: у срока {head} нет источника {link}"));
                    }
                }
                shifts.extend(deadlines::moved_sources(deadline.by_law));
            }
            if shifts.is_empty() {
                wrong.push(format!("{lang:?}: ни один срок не перенесён — проверка переноса ничего не проверила"));
            }
            need.push(format!("<p>{}<br />", text(Key::TaxPageShift)));
            for source in shifts {
                need.push(law_link(&source.url, &source.law, &source.article));
                for also in &source.also {
                    need.push(law_link(&also.url, &also.law, &also.article));
                }
            }
            let (kind, turnover, residency, born) = TAX_EXAMPLE;
            let raw = example::Raw {
                kind: example::kind_id(kind),
                turnover: turnover.to_string(),
                birth: born.to_string(),
                residency: example::residency_id(residency),
                ..example::Raw::default()
            };
            let answer = example::answer(lang, &raw, example::today(None).expect("день"));
            if !answer.errors.is_empty() || answer.blocks.len() < 4 {
                wrong.push(format!("{lang:?}: пример не посчитан — {:?}", answer.errors));
            }
            for block in answer.blocks {
                need.push(format!(">{}</", escape(&block.text)));
            }
            // Курс ЦБ — кодекс и закон о поправке, каждый своей ссылкой.
            let rate_day = rules::on(all.rate_day(), day).expect("курс ЦБ");
            need.push(escape(&fill(t(lang, Key::TaxPageCurrencyAbout), &[&i18n::date(lang, rate_day.from)])));
            let source = &rate_day.source;
            if source.also.is_empty() {
                wrong.push("у курса ЦБ нет закона о поправке отдельной ссылкой".to_owned());
            }
            let mut links = vec![law_link(&source.url, &source.law, &source.article)];
            links.extend(source.also.iter().map(|also| law_link(&also.url, &also.law, &also.article)));
            need.push(links.join(", "));
            for piece in need {
                if !html.contains(&piece) {
                    wrong.push(format!("{lang:?}: нет {piece}"));
                }
            }
            // Ни одна ссылка на закон не несёт адреса в своём тексте.
            if html.contains(", https://") || html.contains("(https://") {
                wrong.push(format!("{lang:?}: адрес в тексте ссылки на закон"));
            }
            if !sample(lang).contains(&format!("<a class=\"button secondary\" href=\"{}\">", tax_path(lang))) {
                wrong.push(format!("{lang:?}: лендинг не ведёт на страницу о налоге"));
            }
        }
        let bare = tax_page(Lang::Ru, "a.css", "b.js", "/app/", None);
        for absent in ["id=\"rates\"", "id=\"example\"", "class=\"answer\"", "class=\"contents\"", "dateModified"] {
            if bare.contains(absent) {
                wrong.push(format!("без сверки на странице {absent}"));
            }
        }
        assert!(wrong.is_empty(), "{wrong:#?}");
    }

    /// Ответ сверху (задача 126) — числами, написанными рукой по файлу правил
    /// версии 2026.12 и календарю, а не теми же вызовами, что у страницы:
    /// порог 115 000 000 ֏, коды 69, 70, 78.2 и 78.3 с 01.07.2025 (НК
    /// 254(3)(3.1)), ставки строк от 1 % (высокие технологии) до 20 % (прочие
    /// доходы при заявлении об общепите), 20-е число — и по-английски «20th».
    /// И день — по закону, а не перенесённый: срок за II квартал 2025 года —
    /// 20 июля, воскресенье, перенос — на понедельник, 21-е (НК 261(1),
    /// 262(1); ՀՕ-180-Ն 32(4)), а абзац, который цитируют без оговорки об
    /// уплате, называет 20-е. Чего-то из этого в правилах дня нет — до
    /// 01.07.2025 кодов, до 2025 года ставок, — нет и абзаца, а не его половины.
    #[test]
    fn the_answer_names_the_day_by_law() {
        let all = rules::armenia();
        let answer = |lang, year, month, day| tax_answer(lang, all, Date::new(year, month, day).expect("день"));
        let july = answer(Lang::Ru, 2025, 7, 1);
        for piece in [
            "<p class=\"answer\">Налог с оборота\u{a0}— режим, который ИП в Армении выбирает заявлением в ՊԵԿ, \
             если оборот и за прошлый, и за текущий год не больше 115\u{a0}000\u{a0}000\u{a0}֏",
            "(ст.\u{a0}254 НК; в том числе с 01.07.2025\u{a0}— коды 69, 70, 78.2 и 78.3).",
            "Ставка\u{a0}— от 1\u{a0}% до 20\u{a0}% дохода квартала",
            "до 20-го числа месяца после квартала: за II\u{a0}квартал 2025\u{a0}— до 20.07.2025.</p>",
        ] {
            assert!(july.contains(piece), "нет «{piece}» в {july}");
        }
        assert!(!july.contains("21.07.2025"), "перенесённый день в ответе: {july}");
        let november = answer(Lang::Ru, 2026, 11, 25);
        assert!(november.contains("за IV\u{a0}квартал 2026\u{a0}— до 20.01.2027."), "{november}");
        let english = answer(Lang::En, 2026, 11, 25);
        for piece in ["codes 69, 70, 78.2 and 78.3", "by the 20th of the month after the quarter", "by Jan 20, 2027."] {
            assert!(english.contains(piece), "нет «{piece}» в {english}");
        }
        let armenian = answer(Lang::Am, 2026, 11, 25);
        for piece in ["69, 70, 78.2 և 78.3", "ամսվա 20-ը ներառյալ"] {
            assert!(armenian.contains(piece), "нет «{piece}» в {armenian}");
        }
        assert!(!armenian.contains(". "), "латинская точка в армянском ответе: {armenian}");
        assert_eq!(answer(Lang::Ru, 2025, 6, 30), "", "до кодов 254(3)(3.1)");
        assert_eq!(answer(Lang::Ru, 2024, 6, 1), "", "до ставок 2025 года");
    }

    /// Ссылки на законы (задача 126) — только текстом: адрес, акт и статья
    /// записи правил экранированы и в `href`, и в тексте, у записи и у её
    /// другого акта (`also`). Сегодня в файле правил таких знаков нет, и без
    /// этой проверки `escape`, убранный из `source_links`, прошёл бы молча.
    #[test]
    fn law_links_are_text() {
        let source = rules::Source {
            law: "ՀՕ-1-Ն \"A\" & <b>".to_owned(),
            article: "հոդված 1 <i>".to_owned(),
            url: "https://www.arlis.am/hy/acts/1?a=1&b=\"2\"".to_owned(),
            checked: Date::new(2026, 11, 25).expect("день"),
            also: vec![rules::Also {
                law: "ՀՕ-2-Ն 'x'".to_owned(),
                article: "<script>".to_owned(),
                url: "https://www.arlis.am/hy/acts/2?c=3&d=4".to_owned(),
            }],
        };
        let html = source_links(&source);
        assert_eq!(
            html,
            "<a href=\"https://www.arlis.am/hy/acts/1?a=1&amp;b=&quot;2&quot;\" lang=\"hy\">ՀՕ-1-Ն &quot;A&quot; &amp; &lt;b&gt;, \
             հոդված 1 &lt;i&gt;</a>, <a href=\"https://www.arlis.am/hy/acts/2?c=3&amp;d=4\" lang=\"hy\">ՀՕ-2-Ն &#39;x&#39;, \
             &lt;script&gt;</a>"
        );
        let line = tax_source(Lang::Ru, &source);
        assert!(line.contains(&html) && !line.contains("<script>") && !line.contains("<b>"), "{line}");
    }

    /// Описание страницы о налоге для поисковиков (задача 126) — один блок
    /// JSON-LD, и он разбирается как JSON: сама страница (`WebPage`) с её
    /// адресом, заголовком, описанием, языком, днём последней сверки правил
    /// (`dateModified`) и издателем — тем же продавцом, что у страницы языка;
    /// хлебные крошки ведут со страницы своего языка на неё. `<`, `>` и `&`
    /// строк таблицы элемент не закроют. Без дня сверки дня изменения нет.
    #[test]
    fn the_tax_page_describes_itself_for_search_engines() {
        let day = checked().expect("правила сверены");
        let mut wrong = Vec::new();
        for lang in Lang::ALL {
            let html = tax_sample(lang);
            if html.matches("<script type=\"application/ld+json\">").count() != 1 {
                wrong.push(format!("{lang:?}: блоков JSON-LD не один"));
                continue;
            }
            let Some(json) = json_ld(&html) else {
                wrong.push(format!("{lang:?}: нет JSON-LD"));
                continue;
            };
            if json.contains('<') || json.contains('>') || json.contains('&') {
                wrong.push(format!("{lang:?}: <, > или & в блоке как есть"));
            }
            let data: serde_json::Value = match serde_json::from_str(&json) {
                Ok(data) => data,
                Err(error) => {
                    wrong.push(format!("{lang:?}: не JSON — {error}"));
                    continue;
                }
            };
            let site = |path: &str| serde_json::Value::from(format!("https://kvartik.com{path}"));
            let page = &data["@graph"][0];
            let crumbs = &data["@graph"][1];
            let seller = &data["@graph"][2];
            let date = format!("{:04}-{:02}-{:02}", day.year(), day.month(), day.day());
            let items = &crumbs["itemListElement"];
            let checks = [
                (data["@context"] == "https://schema.org", "словарь schema.org"),
                (page["@type"] == "WebPage" && page["url"] == site(tax_path(lang)), "страница со своим адресом"),
                (page["name"] == t(lang, Key::TaxPageTitle), "заголовок — как у страницы"),
                (page["description"] == t(lang, Key::TaxPageDescription), "описание — как у страницы"),
                (page["inLanguage"] == lang.code(), "язык страницы"),
                (page["dateModified"] == date.as_str(), "день изменения — день сверки правил"),
                (page["publisher"]["@id"] == seller["@id"], "издатель — продавец"),
                (seller["@type"] == "Organization" && seller["email"] == EMAIL, "продавец с почтой"),
                (seller["@id"] == "https://kvartik.com/#seller", "тот же продавец, что у страницы языка"),
                (page["breadcrumb"]["@id"] == crumbs["@id"] && crumbs["@type"] == "BreadcrumbList", "хлебные крошки"),
                (items.as_array().map(Vec::len) == Some(2), "две ступени"),
                (items[0]["position"] == 1 && items[0]["item"] == site(url(lang)), "первая — страница своего языка"),
                (items[0]["name"] == t(lang, Key::AppName), "имя страницы языка"),
                (items[1]["position"] == 2 && items[1]["item"] == site(tax_path(lang)), "вторая — о налоге"),
                (items[1]["name"] == t(lang, Key::TaxPageLink), "имя страницы о налоге"),
            ];
            for (ok, what) in checks {
                if !ok {
                    wrong.push(format!("{lang:?}: {what}"));
                }
            }
        }
        // Продавец страницы о налоге — тот же узел, что у страницы языка.
        let landing: serde_json::Value =
            serde_json::from_str(&json_ld(&sample(Lang::Ru)).expect("блок")).expect("JSON");
        let tax: serde_json::Value = serde_json::from_str(&json_ld(&tax_sample(Lang::Ru)).expect("блок")).expect("JSON");
        if landing["@graph"][1] != tax["@graph"][2] {
            wrong.push("продавец у страницы о налоге другой".to_owned());
        }
        let bare = tax_page(Lang::Ru, "a.css", "b.js", "/app/", None);
        let data: serde_json::Value = serde_json::from_str(&json_ld(&bare).expect("блок")).expect("JSON");
        if !data["@graph"][0]["dateModified"].is_null() {
            wrong.push("без сверки — день изменения".to_owned());
        }
        assert!(wrong.is_empty(), "{wrong:#?}");
    }

    /// Числа страницы — из ядра, а не написаны ею: срок пробного ключа,
    /// день последней сверки правил и цена ключа — у лендинга и у страницы
    /// «Купить» одна, и лендинг ведёт к ней.
    #[test]
    fn numbers_come_from_the_core() {
        let html = sample(Lang::Ru);
        assert!(html.contains(&format!("Пробный ключ на {TRIAL_KEY_DAYS}\u{A0}дней")), "нет срока пробного ключа");
        let day = checked().expect("правила сверены");
        assert!(html.contains(&escape(&i18n::date(Lang::Ru, day))), "нет дня сверки правил");
        assert!(!page(Lang::Ru, "a.css", "b.js", "/app/", &CALC, &MEDIA_SAMPLE, None).contains("сверены с законом"));
        let mut wrong = Vec::new();
        for lang in Lang::ALL {
            let price = format!("<p class=\"price\">{}</p>", escape(&i18n::money(lang, PRICE)));
            if !sample(lang).contains(&price) || !buy_sample(lang).contains(&price) {
                wrong.push(format!("{lang:?}: нет цены {price}"));
            }
            if !sample(lang).contains(&format!("<a class=\"button\" href=\"{}\">", buy_path(lang))) {
                wrong.push(format!("{lang:?}: карточка цены не ведёт на «Купить»"));
            }
        }
        assert!(sample(Lang::Ru).contains("24\u{A0}000\u{A0}֏"), "цена в рублёвой записи");
        assert!(wrong.is_empty(), "{wrong:#?}");
    }

    /// Страница «Купить» (задача 35) — на своём языке, рядом со страницей
    /// языка: тарифы «Стандарт» и «Pro» (задача 103) с ценами и местами из
    /// ядра, шаги условий (п. 2) со словами согласия, письмом и ссылкой на
    /// условия своего языка, реквизиты перевода из ядра — банк и владелец
    /// счёта по-армянски, как их выдал банк, счёт, сумма каждого тарифа,
    /// ՀՎՀՀ продавца — и кому продаётся.
    #[test]
    fn the_buy_page_names_the_price_and_the_account() {
        let mut wrong = Vec::new();
        for lang in Lang::ALL {
            let html = buy_sample(lang);
            let mut need = vec![
                format!("<html lang=\"{}\">", lang.code()),
                format!("<title>{}</title>", escape(t(lang, Key::BuyTitle))),
                format!("<h1>{}</h1>", escape(t(lang, Key::BuyTitle))),
                format!("<link rel=\"canonical\" href=\"https://kvartik.com{}\" />", buy_path(lang)),
                "<link rel=\"alternate\" hreflang=\"x-default\" href=\"https://kvartik.com/buy/\" />".to_owned(),
                format!("<a class=\"brand\" href=\"{}\">", url(lang)),
                format!("<p class=\"phrase\">{}</p>", escape(t(lang, Key::BuyAccept))),
                format!("<a href=\"mailto:{EMAIL}\">{EMAIL}</a>"),
                format!("<a class=\"button secondary\" href=\"{}\">", Legal::Terms.path(lang)),
                format!("<span class=\"copy\" lang=\"hy\">{}</span>", escape(BANK)),
                format!("<span class=\"copy\" lang=\"hy\">{}</span>", escape(HOLDER)),
                format!("<span class=\"copy\">{ACCOUNT}</span>"),
                format!(
                    "<dd>{}</dd>",
                    escape(&fill(t(lang, Key::BuyAmountPlan), &[t(lang, Key::PlanStandard), &i18n::money(lang, PRICE)]))
                ),
                format!(
                    "<dd>{}</dd>",
                    escape(&fill(t(lang, Key::BuyAmountPlan), &[t(lang, Key::PlanPro), &i18n::money(lang, PRO_PRICE)]))
                ),
                format!("<p class=\"price\">{}</p>", escape(&i18n::money(lang, PRICE))),
                format!("<p class=\"price\">{}</p>", escape(&i18n::money(lang, PRO_PRICE))),
                escape(&fill(t(lang, Key::BuyPlan), &[t(lang, Key::PlanPro)])),
                escape(&fill(
                    t(lang, Key::BuyPlanProText),
                    &[
                        &PRO_SEATS.to_string(),
                        &i18n::money(lang, per_ip()),
                        t(lang, Key::OfficeTitle),
                        t(lang, Key::PlanStandard),
                    ],
                )),
                escape(&fill(t(lang, Key::BuyHolderNote), &[SELLER_TIN])),
                escape(t(lang, Key::BuyStepPay)),
                escape(t(lang, Key::BuyPurposeValue)),
                escape(t(lang, Key::BuyOnly)),
                escape(&fill(t(lang, Key::BuyStepKey), &[t(lang, Key::NavProfile), t(lang, Key::LicenseTitle)])),
                trial(lang),
                format!(
                    "<meta name=\"description\" content=\"{}\" />",
                    escape(&fill(
                        t(lang, Key::BuyDescription),
                        &[
                            t(lang, Key::PlanStandard),
                            &i18n::money(lang, PRICE),
                            t(lang, Key::PlanPro),
                            &PRO_SEATS.to_string(),
                            &i18n::money(lang, PRO_PRICE),
                        ]
                    ))
                ),
            ];
            for other in Lang::ALL {
                need.push(format!(
                    "<link rel=\"alternate\" hreflang=\"{}\" href=\"https://kvartik.com{}\" />",
                    other.code(),
                    buy_path(other)
                ));
                need.push(format!("<a href=\"{}\" hreflang=\"{}\"", buy_path(other), other.code()));
            }
            for piece in need {
                if !html.contains(&piece) {
                    wrong.push(format!("{lang:?}: нет {piece}"));
                }
            }
            if html.matches("aria-current=\"page\"").count() != 1 {
                wrong.push(format!("{lang:?}: текущий язык не один"));
            }
            // Письмо — тем же адресом, с темой и текстом, где слова
            // согласия; `&` между ними — ссылкой на знак (Правило 13).
            let mail = format!(
                "href=\"mailto:{EMAIL}?subject={}&amp;body={}\"",
                mailto_part(t(lang, Key::BuyMailSubject)),
                mailto_part(&fill(
                    t(lang, Key::BuyMailBody),
                    &[t(lang, Key::PlanStandard), t(lang, Key::PlanPro), t(lang, Key::BuyAccept)]
                ))
            );
            if !html.contains(&mail) {
                wrong.push(format!("{lang:?}: нет письма {mail}"));
            }
            if html.contains("og:") || html.contains("{}") {
                wrong.push(format!("{lang:?}: разметка лендинга или незаполненный шаблон"));
            }
        }
        assert!(wrong.is_empty(), "{wrong:#?}");
    }

    /// Адрес письма: незарезервированные знаки — как есть, остальное —
    /// байтами UTF-8, перенос строки — CRLF (RFC 6068).
    #[test]
    fn mail_text_is_percent_encoded() {
        assert_eq!(mailto_part("Kvartik-1.0_~"), "Kvartik-1.0_~");
        assert_eq!(mailto_part("a b&c=d?e"), "a%20b%26c%3Dd%3Fe");
        assert_eq!(mailto_part("ИП:\n\n֏"), "%D0%98%D0%9F%3A%0D%0A%0D%0A%D6%8F");
        assert_eq!(mailto_part("ՀՎՀՀ\u{A0}"), "%D5%80%D5%8E%D5%80%D5%80%C2%A0");
    }

    /// Ни встроенного скрипта, ни стиля, ни атрибута `style` или
    /// обработчика `on…`: CSP их не пустил бы, и страница молча осталась бы
    /// без них. Блок данных JSON-LD (задача 118) — не скрипт, и он не больше
    /// одного на страницу: у страниц языков и, с задачи 126, у страниц
    /// о налоге.
    #[test]
    fn nothing_inline_for_the_csp_to_block() {
        let mut wrong = Vec::new();
        for (page, html) in every_page() {
            let data = html.matches("<script type=\"application/ld+json\">").count();
            if html.matches("<script").count() != html.matches("<script src=").count() + data {
                wrong.push(format!("{page}: встроенный скрипт"));
            }
            if data > 1 {
                wrong.push(format!("{page}: блоков JSON-LD {data}"));
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

    /// Текст первого блока JSON-LD страницы `html`.
    fn json_ld(html: &str) -> Option<String> {
        let open = "<script type=\"application/ld+json\">";
        let at = html.find(open)? + open.len();
        Some(html[at..at + html[at..].find("</script>")?].to_owned())
    }

    /// Описание программы для поисковиков (задача 118) — у каждой страницы
    /// языка, одно, и разбирается как JSON: программа со страницей своего
    /// языка, описанием страницы и ключами с ценами ядра в драмах и страницей
    /// «Купить» своего языка, продавец — с почтой из ядра. `<`, `>` и `&`
    /// строки таблицы элемент не закроют. У страниц о налоге — свой блок
    /// без программы (задача 126), а у остальных страниц нет ни блока, ни
    /// превью в чатах: они не о программе целиком.
    #[test]
    fn language_pages_describe_the_app_for_search_engines() {
        let block = json_ld;
        let mut wrong = Vec::new();
        for lang in Lang::ALL {
            let html = sample(lang);
            let Some(json) = block(&html) else {
                wrong.push(format!("{lang:?}: нет JSON-LD"));
                continue;
            };
            if json.contains('<') || json.contains('>') || json.contains('&') {
                wrong.push(format!("{lang:?}: <, > или & в блоке как есть"));
            }
            let data: serde_json::Value = match serde_json::from_str(&json) {
                Ok(data) => data,
                Err(error) => {
                    wrong.push(format!("{lang:?}: не JSON — {error}"));
                    continue;
                }
            };
            let app = &data["@graph"][0];
            let seller = &data["@graph"][1];
            let site = |path: &str| serde_json::Value::from(format!("https://kvartik.com{path}"));
            let checks = [
                (data["@context"] == "https://schema.org", "словарь schema.org"),
                (app["@type"] == "SoftwareApplication" && app["name"] == "Kvartik", "программа"),
                (app["url"] == site(url(lang)), "страница своего языка"),
                (app["description"] == t(lang, Key::LandingDescription), "описание — как у страницы"),
                (app["inLanguage"] == serde_json::json!(["hy", "ru", "en"]), "три языка"),
                (app["featureList"].as_array().map(Vec::len) == Some(FEATURES.len()), "что умеет"),
                (app["publisher"]["@id"] == seller["@id"], "продаёт продавец"),
                (seller["@type"] == "Organization" && seller["email"] == EMAIL, "продавец с почтой"),
            ];
            for (ok, what) in checks {
                if !ok {
                    wrong.push(format!("{lang:?}: {what}"));
                }
            }
            for (at, price) in [PRICE, PRO_PRICE].into_iter().enumerate() {
                let offer = &app["offers"][at];
                if offer["price"].as_str().and_then(|text| text.parse::<i64>().ok()) != Some(price.minor())
                    || offer["priceCurrency"] != "AMD"
                    || offer["url"] != site(buy_path(lang))
                {
                    wrong.push(format!("{lang:?}: ключ {at} — не цена ядра {offer}"));
                }
            }
        }
        // Блок — у страниц языков и страниц о налоге (задача 126, у неё свой:
        // `the_tax_page_describes_itself_for_search_engines`), а программа —
        // только у страниц языков. У остальных — ни блока, ни превью.
        for (page, html) in every_page() {
            let language_page = Lang::ALL.into_iter().any(|lang| page == format!("{lang:?} {}", url(lang)));
            let tax_page = Lang::ALL.into_iter().any(|lang| page == format!("{lang:?} {}", tax_path(lang)));
            let found = block(&html);
            if !language_page && !tax_page && (found.is_some() || html.contains("og:")) {
                wrong.push(format!("{page}: JSON-LD или превью не у страницы языка и не о налоге"));
            }
            if (language_page || tax_page) && (found.is_none() || !html.contains("<meta property=\"og:url\"")) {
                wrong.push(format!("{page}: нет JSON-LD или превью"));
            }
            if tax_page && found.is_some_and(|json| json.contains("SoftwareApplication")) {
                wrong.push(format!("{page}: страница о налоге описывает программу"));
            }
        }
        assert!(wrong.is_empty(), "{wrong:#?}");
        // Строка с `</script>`, `<!--` и `&` элемент не закроет и не станет
        // разметкой, а разбор вернёт её как была.
        let text = "</script><!-- & <b>";
        let html = data_block(&serde_json::json!({ "name": text }));
        assert_eq!(html.matches("</script>").count(), 1, "{html}");
        assert!(!html.contains("<!--") && !html.contains("<b>") && !html.contains(" & "), "{html}");
        let back: serde_json::Value = serde_json::from_str(&block(&html).expect("блок на месте")).expect("JSON");
        assert_eq!(back["name"], text);
    }

    /// Ключ IndexNow (задача 118) — из знаков, которые принимают оба прочтения
    /// документации: от 8 до 128 цифр и букв a–f.
    #[test]
    fn the_indexnow_key_has_a_valid_shape() {
        assert!((8..=128).contains(&INDEXNOW_KEY.len()));
        assert!(INDEXNOW_KEY.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)), "{INDEXNOW_KEY}");
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

    /// Карта сайта называет все страницы, а `robots.txt` — карту. У страниц
    /// о налоге (задача 126) — `lastmod`, день последней сверки правил,
    /// у остальных — нет: дня их правки сборка не знает. Без дня сверки
    /// `lastmod` нет ни у кого.
    #[test]
    fn robots_and_sitemap_point_at_every_page() {
        let day = checked().expect("правила сверены");
        let lastmod = format!("<lastmod>{:04}-{:02}-{:02}</lastmod>", day.year(), day.month(), day.day());
        let map = sitemap(checked());
        for lang in Lang::ALL {
            let entry = format!("<loc>https://kvartik.com{}</loc>\n    {lastmod}\n", tax_path(lang));
            assert!(map.contains(&entry), "lastmod о налоге {lang:?}:\n{map}");
        }
        assert_eq!(map.matches("<lastmod>").count(), Lang::ALL.len(), "lastmod — только у страниц о налоге");
        assert!(!sitemap(None).contains("<lastmod>"), "без сверки — без lastmod");
        for lang in Lang::ALL {
            assert!(map.contains(&format!("<loc>https://kvartik.com{}</loc>", url(lang))), "{lang:?}");
            assert!(map.contains(&format!("<loc>https://kvartik.com{}</loc>", buy_path(lang))), "«Купить» {lang:?}");
            assert!(map.contains(&format!("<loc>https://kvartik.com{}</loc>", feedback_path(lang))), "отзывы {lang:?}");
            assert!(map.contains(&format!("<loc>https://kvartik.com{}</loc>", tax_path(lang))), "о налоге {lang:?}");
            for doc in Legal::ALL {
                assert!(map.contains(&format!("<loc>https://kvartik.com{}</loc>", doc.path(lang))), "{doc:?} {lang:?}");
            }
        }
        assert_eq!(map.matches("<url>").count(), Lang::ALL.len() * (4 + Legal::ALL.len()));
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
            self.put_bytes(path, text.as_bytes());
        }

        fn put_bytes(&self, path: &str, bytes: &[u8]) {
            let full = self.0.join(path);
            fs::create_dir_all(full.parent().expect("у файла есть папка")).expect("папка создаётся");
            fs::write(full, bytes).expect("файл пишется");
        }
    }

    /// Заголовок WebP расширенного вида (`VP8X`) с холстом `width`×`height`:
    /// сборке картинка нужна только ради размера.
    fn webp(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = b"RIFF\0\0\0\0WEBPVP8X\x0a\0\0\0\0\0\0\0".to_vec();
        bytes.extend_from_slice(&(width - 1).to_le_bytes()[..3]);
        bytes.extend_from_slice(&(height - 1).to_le_bytes()[..3]);
        bytes
    }

    /// Снимков и роликов на язык: постер, четыре экрана на двух ширинах,
    /// WebM и MP4.
    const MEDIA_PER_LANG: usize = 1 + 2 * SHOTS.len() + 2;

    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// Размер WebP — из заголовка любого из трёх видов, а не WebP — `None`:
    /// сборка пишет размер в разметку, и неверный сдвинул бы страницу.
    #[test]
    fn reads_the_size_of_any_webp() {
        // С потерями: кадр 640×480 после трёх байтов кадра и подписи.
        let mut lossy = b"RIFF\0\0\0\0WEBPVP8 \0\0\0\0\0\0\0\x9d\x01\x2a".to_vec();
        lossy.extend_from_slice(&640u16.to_le_bytes());
        lossy.extend_from_slice(&480u16.to_le_bytes());
        // Без потерь: 14 бит ширины и высоты минус один.
        let mut lossless = b"RIFF\0\0\0\0WEBPVP8L\0\0\0\0\x2f".to_vec();
        lossless.extend_from_slice(&(99u32 | (49 << 14)).to_le_bytes());
        type Size = Option<(u32, u32)>;
        let cases: [(&[u8], Size); 5] = [
            (&lossy, Some((640, 480))),
            (&lossless, Some((100, 50))),
            (&webp(720, 1560), Some((720, 1560))),
            (b"\x89PNG\r\n\x1a\n", None),
            (b"RIFF\0\0\0\0WEBP", None),
        ];
        let wrong: Vec<String> = cases
            .iter()
            .filter(|(bytes, want)| webp_size(bytes) != *want)
            .map(|(bytes, want)| format!("{:?}: {:?}, а ждали {want:?}", &bytes[..bytes.len().min(16)], webp_size(bytes)))
            .collect();
        assert!(wrong.is_empty(), "{wrong:#?}");
    }

    /// Раздел «Как это выглядит» (задача 119): ролик сам не играет и не
    /// качается, пока его не покажут, — у него кнопки, постер и размер;
    /// снимки — по мере прокрутки, с размером, описанием своего языка и
    /// снимком компьютера с 48rem; данные названы примером.
    #[test]
    fn the_look_section_shows_the_program_without_cost() {
        let mut wrong = Vec::new();
        for lang in Lang::ALL {
            let html = sample(lang);
            let section = &html[html.find("<section class=\"look\"").unwrap_or(0)..];
            let section = &section[..section.find("</section>").unwrap_or(0)];
            let video = section.find("<video").map(|at| &section[at..at + section[at..].find('>').unwrap_or(0)]).unwrap_or("");
            for (what, ok) in [
                ("ролик без кнопок", video.contains(" controls ")),
                ("ролик качается сразу", video.contains("preload=\"none\"")),
                ("ролик играет сам", !video.contains("autoplay")),
                ("ролик без звука и по кругу", video.contains(" muted loop playsinline ")),
                ("у ролика нет размера", video.contains("width=\"720\" height=\"1560\"")),
                ("постер качается сразу", !video.contains(" poster=") && video.contains(" data-poster=\"/kvartik-site-")),
                ("у ролика нет WebM и MP4", section.contains("type=\"video/webm\"") && section.contains("type=\"video/mp4\"")),
                ("снимков не четыре", section.matches("loading=\"lazy\"").count() == SHOTS.len()),
                ("снимок телефона без размера", section.matches("width=\"720\" height=\"1560\" alt=").count() == SHOTS.len()),
                ("снимок компьютера не с 48rem", section.matches("media=\"(min-width: 48rem)\"").count() == SHOTS.len()),
                ("не сказано, что данные — пример", section.contains(&escape(t(lang, Key::LandingLookText)))),
            ] {
                if !ok {
                    wrong.push(format!("{lang:?}: {what}"));
                }
            }
            for (_, caption, alt) in SHOTS {
                if !section.contains(&format!("alt=\"{}\"", escape(t(lang, alt)))) || !section.contains(&escape(t(lang, caption))) {
                    wrong.push(format!("{lang:?}: нет описания или подписи «{}»", t(lang, caption)));
                }
            }
            // После карточек «Что умеет Kvartik»: выше Chrome качал бы снимки
            // вместе со страницей.
            if !(html.find("id=\"features\"") < html.find("class=\"look\"")) {
                wrong.push(format!("{lang:?}: раздел выше карточек «Что умеет Kvartik»"));
            }
        }
        assert!(wrong.is_empty(), "{wrong:#?}");
    }

    fn sources(temp: &Temp) {
        temp.put("style/tokens.css", TOKENS_CSS);
        temp.put("style/fonts.css", FONTS_CSS);
        temp.put("site/landing.css", LANDING_CSS);
        temp.put("site/landing.js", LANDING_JS);
        for (name, text) in LEGAL {
            temp.put(&format!("legal/{name}"), text);
        }
        for lang in Lang::ALL {
            let code = lang.code();
            temp.put_bytes(&format!("site/{MEDIA}/demo-{code}.webp"), &webp(720, 1560));
            temp.put_bytes(&format!("site/{MEDIA}/demo-{code}.webm"), format!("webm {code}").as_bytes());
            temp.put_bytes(&format!("site/{MEDIA}/demo-{code}.mp4"), format!("mp4 {code}").as_bytes());
            for (name, _, _) in SHOTS {
                temp.put_bytes(&format!("site/{MEDIA}/{name}-{code}-360.webp"), &webp(720, 1560));
                temp.put_bytes(&format!("site/{MEDIA}/{name}-{code}-1280.webp"), &webp(1280, 800));
            }
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

    /// Лендинг ложится в корень сборки: три страницы, три «Купить», три
    /// страницы отзывов, шесть юридических, карта, `robots.txt` и по одному
    /// файлу оформления
    /// и скрипта, на которые страницы ссылаются. Та же сборка — те же имена
    /// и байты. Файл на месте файла лендинга — ошибка, а не молча затёртый
    /// файл.
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
        pages.extend(Lang::ALL.into_iter().map(|lang| page_file(buy_path(lang))));
        pages.extend(Lang::ALL.into_iter().map(|lang| page_file(feedback_path(lang))));
        pages.extend(Lang::ALL.into_iter().map(|lang| page_file(tax_path(lang))));
        for doc in Legal::ALL {
            pages.extend(Lang::ALL.into_iter().map(|lang| page_file(doc.path(lang))));
            for &day in doc.earlier() {
                pages.extend(Lang::ALL.into_iter().map(|lang| page_file(&doc.earlier_path(lang, day))));
            }
        }
        let key = format!("{INDEXNOW_KEY}.txt");
        for name in pages.iter().map(String::as_str).chain(["robots.txt", "sitemap.xml", key.as_str()]) {
            assert!(written.iter().any(|n| n == name), "нет {name}: {written:?}");
        }
        assert_eq!(written.len(), pages.len() + 5 + MEDIA_PER_LANG * Lang::ALL.len(), "{written:?}");
        assert_eq!(fs::read_to_string(stage.join(&key)).ok().as_deref(), Some(INDEXNOW_KEY), "файл ключа — сам ключ");
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
        let clash = Temp::new("clash-buy");
        sources(&clash);
        clash.put("stage/buy/index.html", "root page");
        let error = build(&clash).expect_err("файл корня на месте страницы «Купить»");
        assert!(error.contains("лёг бы на файл лендинга"), "{error}");
        let clash = Temp::new("clash-feedback");
        sources(&clash);
        clash.put("stage/en/feedback/index.html", "root page");
        let error = build(&clash).expect_err("файл корня на месте страницы отзывов");
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

        // Прежняя редакция (задача 93) — тоже: без её файла адрес, который
        // обещают условия (п. 11), остался бы пустым.
        let earlier = Temp::new("earlier");
        sources(&earlier);
        fs::create_dir_all(earlier.0.join("stage")).expect("папка сборки");
        fs::remove_file(earlier.0.join("legal/terms.2026-11-09.hy.md")).expect("файл удаляется");
        let error = build(&earlier).expect_err("нет прежней редакции");
        assert!(error.contains("terms.2026-11-09.hy.md"), "{error}");
        assert!(names(&earlier.0.join("stage")).is_empty(), "в сайт что-то легло");

        // Новая редакция условий со своими словами согласия, а страница
        // «Купить» просит прежние (задача 35): сборка останавливается,
        // и в сайт ничего не ложится.
        let edition = Temp::new("edition");
        sources(&edition);
        fs::create_dir_all(edition.0.join("stage")).expect("папка сборки");
        let (_, terms) = LEGAL.iter().find(|(name, _)| *name == "terms.en.md").expect("текст есть");
        assert!(terms.contains("November 21, 2026"), "в условиях — та редакция, что ждёт тест");
        edition.put("legal/terms.en.md", &terms.replace("November 21, 2026", "December 1, 2026"));
        let error = build(&edition).expect_err("слов согласия нет в условиях");
        assert!(error.contains("terms.en.md") && error.contains("Key::BuyAccept"), "{error}");
        assert!(names(&edition.0.join("stage")).is_empty(), "в сайт что-то легло");
    }
}
