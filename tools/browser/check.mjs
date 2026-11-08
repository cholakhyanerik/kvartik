// Собранный сайт — в браузере (задача 60): части wasm, формы, копия и работа
// без сети — на выпускной сборке, как её получат люди. Шаг CI после
// `trunk build --release`; на машине — так же:
//
//   node tools/browser/check.mjs [--dist crates/app/dist] [--work <папка>]
//
// Зачем. Браузерные тесты идут на тестовом wasm, который никто не режет:
// точки разреза там — обычные `async fn` (`crates/app/src/ui/split.rs`).
// Хук сборки проверяет разрез статически (`crates/sw/src/wasm.rs`), но рисует
// ли часть экран, собирает ли копию и приходит ли без сети из кэша service
// worker, видно только здесь. Программа разреза держится за внутренности
// wasm-bindgen (на 0.2.128 она уже ломалась — leptos#4870): после
// обновления сборка прошла бы зелёной, а экраны перестали бы открываться.
//
// Что проверяется, по порядку:
// 1. Часть не загрузилась — окно говорит словами и даёт «Отмену», а снятая
//    блокировка открывает экран (CLAUDE.md, Правило 2, «Формы и действия по
//    нажатию — частями wasm»). Лендинг (задача 34): страница каждого языка
//    со своим оформлением и ссылкой в программу, без прокрутки вбок на
//    телефоне, калькулятор отвечает числом ядра и прячет ненужные поля,
//    «Открыть Kvartik» берёт введённое с собой и программа предлагает его
//    в профиль, а старый адрес программы `/#/…` ведёт в /app/ на тот же
//    экран. Несколько ИП (задача 38): второй заводится и открывается
//    перезагрузкой, и первый открывается снова.
// 2. С сетью: восстановление образца копии, экраны и их формы, карточка
//    выпущенного документа, PDF, выгрузка с закрытым кварталом, разошедшимся
//    с поданным, новая копия — каждая из девяти частей;
//    и все части `dist/app/split_*.wasm` загружены. «Установить» на
//    главном экране — и без вопроса браузера, и раскладка, которую видно
//    только с настоящим CSS: кнопка не вплотную к карточке налога под ней.
// 3. Сервер выключен: программа открывается из кэша service worker, и те же
//    экраны, формы, карточка, PDF и тексты лицензий работают без сети.
// 4. Новая версия (задача 67): на сервере — копия сборки новой версией. Пока
//    открыта форма или начат ввод вне формы, над экраном только слова; всё
//    закрыли — новая сборка включается сама, страница перезагружается,
//    и встаёт окно «Что нового». Так же — при заходе на страницу. Сломанное,
//    это заперло бы программу или стёрло бы начатое у всех, кто получит
//    следующую версию.
// Везде — без плашки сбоя и без ошибок в консоли.
//
// Почему так:
// - Минуты CI считаются (CLAUDE.md, Правило 11): один язык, одна ширина,
//   а не глазной прогон по языкам и ширинам.
// - Сервер — свой (`serve.mjs`): отвечает, как Cloudflare Pages, и сжимает
//   всё при старте, а не при первом запросе.
// - Блокировка части — `Network.setBlockedURLs` в своём контексте браузера
//   и мимо service worker (`Network.setBypassServiceWorker`): он при первом
//   заходе сам скачивает весь сайт и забирает страницу (`clients.claim`
//   в `sw.js`), и часть пришла бы из его кэша мимо блокировки. Загрузчик
//   частей (`wasm_split_cli_support` 0.2.5) пишет отказ заблокированной
//   части в консоль (`console.error` в `wrapAsyncCb`) — это его код,
//   человеку он не мешает, и провалом здесь не считается.
// - «Без сети» — выключенный сервер, а не эмуляция (Правило 2).

import { parseArgs } from "node:util";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { Browser, Checks, DEVICES, sleep } from "./cdp.mjs";
import { REPO, newVersion, own } from "./serve.mjs";
import { samples, t } from "./strings.mjs";

const { values: opt } = parseArgs({
  options: {
    dist: { type: "string", default: "crates/app/dist" },
    work: { type: "string" },
    lang: { type: "string", default: "ru" },
    port: { type: "string", default: "8810" },
  },
});
const DIST = path.resolve(REPO, opt.dist);
// Своя временная папка (профили Chrome) убирается в конце; названная — нет.
const WORK = opt.work ? path.resolve(opt.work) : fs.mkdtempSync(path.join(os.tmpdir(), "kvartik-check-"));
fs.mkdirSync(WORK, { recursive: true });
const PORT = Number(opt.port);
const ORIGIN = `http://127.0.0.1:${PORT}`;
const s = (key) => t(key, opt.lang);
const checks = new Checks();

// Рабочий экран: база открыта, разделы прочитаны — встаёт поле выбора файла
// копии (с 0.0.20; так же в skill browser-run).
const WORKING = `return document.querySelector('input[type=file][accept=".json,application/json"]') ? true : null;`;
const CONTROLLER = `return navigator.serviceWorker.controller ? navigator.serviceWorker.controller.scriptURL : null;`;
const VISIBLE = (selector) =>
  `return [...document.querySelectorAll(${JSON.stringify(selector)})].some((e) => e.getClientRects().length > 0) || null;`;
const HIDDEN = (selector) =>
  `return [...document.querySelectorAll(${JSON.stringify(selector)})].some((e) => e.getClientRects().length > 0) ? null : true;`;
// Видимый текст внутри раздела экрана: спрятанное `innerText` не отдаёт.
const TEXT_IN = (scope, text) =>
  `return document.querySelector(${JSON.stringify(scope)})?.innerText.includes(${JSON.stringify(text)}) ? true : null;`;
const NO_TEXT_IN = (scope, text) =>
  `return document.querySelector(${JSON.stringify(scope)})?.innerText.includes(${JSON.stringify(text)}) ? null : true;`;
const NO_PANIC = `return document.getElementById("panic") ? null : true;`;

async function go(page, hash) {
  await page.eval(`location.hash = ${JSON.stringify(hash)}; return true;`);
  await sleep(300);
}

/// Видимая кнопка раздела `scope` с текстом или `aria-label` `label`.
const BUTTON = (scope, label) => `[...document.querySelectorAll(${JSON.stringify(`${scope} button`)})].find((b) =>
  (b.textContent.trim() === ${JSON.stringify(label)} || b.getAttribute("aria-label") === ${JSON.stringify(label)})
  && b.getClientRects().length > 0)`;

async function press(page, scope, label) {
  const pressed = await page.eval(`const b = ${BUTTON(scope, label)}; if (!b) return null; b.click(); return true;`);
  return pressed === true;
}

/// Экран открывает свою форму и закрывает её. Экран и его форма — одна
/// часть (`ui/split.rs`), поэтому кнопка и форма ищутся внутри раздела
/// экрана: на компьютере «Новый счёт» есть ещё и в меню, вне части, и
/// проверка прошла бы без неё. Ширина — телефонная: там своя кнопка
/// нового счёта у экрана, а у меню её нет.
const FORMS = [
  { name: "Счета", hash: "#/documents", scope: "#documents", open: "DocNewInvoice", close: "Back",
    shown: VISIBLE("#documents .wizard-head"), gone: HIDDEN("#documents .wizard-head") },
  { name: "Клиенты", hash: "#/clients", scope: "#clients", open: "ClientAdd", close: "Back",
    shown: VISIBLE("#clients .wizard-head"), gone: HIDDEN("#clients .wizard-head") },
  { name: "Книга", hash: "#/ledger", scope: "#ledger", open: "LedgerAddIncome", close: "Cancel",
    shown: TEXT_IN("#ledger", s("OpNewIncome")), gone: NO_TEXT_IN("#ledger", s("OpNewIncome")) },
  { name: "Профиль", hash: "#/profile", scope: "#profile", open: "ProfileEdit", close: "Cancel",
    shown: VISIBLE("#profile-accounts"), gone: HIDDEN("#profile-accounts") },
];

/// Экраны без формы: их содержимое рисует часть экрана — налог (задача 19),
/// годовые платежи (задача 20), сроки (задача 21), числа для расчёта
/// (задача 24), лицензия (задача 25) и «О программе» (задача 31). «Сверьте
/// расчёт с бухгалтером» части годовых платежей и сроков, слова о полях
/// формы — часть чисел, о ключе — часть лицензии, а правила налога — часть
/// «О программе» рисуют всегда, и с профилем, и без него.
const SCREENS = [
  { name: "Налог", hash: "#/tax", shown: TEXT_IN("#tax", s("TaxTurnover")) },
  { name: "Годовые платежи", hash: "#/annual", shown: TEXT_IN("#annual", s("TaxCheck")) },
  { name: "Сроки", hash: "#/deadlines", shown: TEXT_IN("#deadlines", s("DeadlinesShiftRule")) },
  { name: "Числа для расчёта", hash: "#/report", shown: TEXT_IN("#report", s("ReportAbout")) },
  { name: "Лицензия", hash: "#/license", shown: TEXT_IN("#license", s("LicenseAbout")) },
  { name: "О программе", hash: "#/about", shown: TEXT_IN("#about", s("AboutRules")) },
];

async function screens(page, when) {
  for (const screen of SCREENS) {
    await go(page, screen.hash);
    checks.check(`${when}: экран «${screen.name}»`, await page.waitFor(screen.shown));
  }
  await licenses(page, when);
}

/// Тексты лицензий (задача 51): блок «О программе» раскрывается файлом,
/// который пишет сборка (`crates/sw/src/licenses.rs`), а приходит файл и без
/// сети — из кэша service worker. В нём — все четыре раздела и крейты Leptos,
/// у которых своего файла лицензии нет.
async function licenses(page, when) {
  await go(page, "#/about");
  const opened = await page.waitFor(`const summary = document.querySelector("#about details.licenses > summary");
    if (!summary || !summary.getClientRects().length) return null;
    if (!summary.parentElement.open) summary.click();
    return true;`);
  const got = opened && await page.waitFor(`const pre = document.querySelector("#about details.licenses[open] pre.licenses-text");
    if (!pre) return null;
    const text = pre.textContent;
    return { parts: ["SIL OPEN FONT LICENSE", "The Rust Standard Library (Rust ", "UNICODE LICENSE V3", "4. Rust crates"]
        .filter((part) => !text.includes(part)),
      leptos: text.includes("leptos 0.") && text.includes("Copyright (c) 2022 Greg Johnston"),
      wide: document.documentElement.scrollWidth > document.documentElement.clientWidth };`);
  checks.check(`${when}: тексты лицензий`, got && got.parts.length === 0 && got.leptos && !got.wide, JSON.stringify(got));
}

/// Главный экран: кнопка «Установить» есть всегда, пока программа не
/// установлена (задача 55), — и без вопроса браузера: Chrome в CI может его
/// и не прислать, а Firefox и Safari не присылают никогда. Раздел
/// «Установка» — блоком с отступом, как карточки под ним, а не кнопкой
/// вплотную к карточке налога (найдено глазным прогоном задачи 20). Отступ
/// блоков — `--space-4`, 16 точек. Кнопку проверка не нажимает: пришёл
/// настоящий вопрос — она открыла бы окно установки Chrome.
async function home(page) {
  await go(page, "#/");
  const gap = await page.waitFor(`const button = ${BUTTON("#home", s("InstallButton"))};
    const card = [...document.querySelectorAll("#home .hero-card")].find((c) => c.getClientRects().length > 0);
    if (!button || !card) return null;
    return { gap: card.getBoundingClientRect().top - button.getBoundingClientRect().bottom };`);
  checks.check("главная: «Установить» без вопроса браузера, не вплотную к карточке налога",
    gap !== null && gap.gap >= 12,
    gap === null ? "нет кнопки «Установить» или карточки налога" : `зазор ${gap.gap} точек`);
}

async function forms(page, when) {
  for (const f of FORMS) {
    await go(page, f.hash);
    const screen = await page.waitFor(`return ${BUTTON(f.scope, s(f.open))} ? true : null;`);
    if (!checks.check(`${when}: экран «${f.name}»`, screen)) continue;
    await press(page, f.scope, s(f.open));
    checks.check(`${when}: форма экрана «${f.name}» открылась`, await page.waitFor(f.shown));
    await press(page, f.scope, s(f.close));
    checks.check(`${when}: форма экрана «${f.name}» закрылась`, await page.waitFor(f.gone));
  }
}

/// Карточка выпущенного документа и его PDF: шрифты документа — с сайта или
/// из кэша service worker.
async function card(page, when) {
  await go(page, "#/documents");
  const href = await page.waitFor(`const a = [...document.querySelectorAll('#documents a.entry[href^="#/documents/"]')]
    .find((a) => a.getClientRects().length > 0); return a ? a.getAttribute("href") : null;`);
  if (!checks.check(`${when}: в списке есть выпущенный документ`, href)) return;
  await go(page, href);
  const pdf = await page.waitFor(`return ${BUTTON("#documents", s("DocPdf"))} ? true : null;`);
  if (!checks.check(`${when}: карточка документа`, pdf)) return;
  await press(page, "#documents", s("DocPdf"));
  checks.check(`${when}: PDF собран`, await page.waitFor(TEXT_IN("#documents", s("PdfDownload")), 30000));
}

/// Карточка документа закрывается на телефоне (найдено 2026-11-08): экран
/// открывается с начала, и строка документа может оказаться под меню —
/// тогда заголовок карточки в неё не перетекает, а растворяется: его
/// снимок, летящий к строке, лёг бы на вкладки. Высота окна — такая, чтобы
/// верх меню пришёлся на середину последней строки списка. А в строку на
/// виду (первую) заголовок перетекает по-прежнему.
async function cardClose(page) {
  const media = (motion) => page.send("Emulation.setEmulatedMedia", {
    features: [{ name: "prefers-color-scheme", value: "light" }, { name: "prefers-reduced-motion", value: motion }],
  });
  await go(page, "#/documents");
  const rows = await page.waitFor(`const rows = [...document.querySelectorAll('#documents a.entry[href^="#/documents/"]')]
      .filter((a) => a.getClientRects().length > 0);
    if (rows.length < 2) return null;
    const last = rows.at(-1).getBoundingClientRect();
    const menu = document.querySelector(".app > .nav").getBoundingClientRect();
    return { first: rows[0].getAttribute("href"), last: rows.at(-1).getAttribute("href"),
      height: Math.round(last.top + scrollY + last.height / 2 + menu.height) };`);
  if (!checks.check("с сетью: в списке документов две строки — для карточки на телефоне", rows)) return;
  await page.send("Emulation.setDeviceMetricsOverride", { ...DEVICES[360], height: rows.height });
  await media("no-preference");
  await go(page, rows.last);
  await sleep(900);
  const low = await page.eval(`${freeze("#/documents", 300)}
    const menu = document.querySelector(".app > .nav").getBoundingClientRect();
    const top = Math.ceil(menu.top) + 2;
    return { clip: { x: 0, y: top, width: Math.floor(menu.width), height: Math.floor(menu.bottom) - top, scale: 1 } };`);
  if (!low?.clip) {
    checks.check("движение, телефон: карточка закрылась переходом", false, low?.error ?? "");
  } else {
    const menu = await overMenu(page, low.clip);
    checks.check("движение, телефон: карточка закрылась, строка под меню — на меню посреди перехода ничего не лежит",
      menu.ok, menu.detail);
  }
  await go(page, rows.first);
  await sleep(900);
  const seen = await page.eval(`${freeze("#/documents", 300)}
    return window.__frozen.some((a) => a.effect?.pseudoElement === "::view-transition-group(doc)");`);
  await page.eval(THAW);
  checks.check("движение, телефон: карточка закрылась, строка на виду — заголовок перетекает в неё", seen === true,
    JSON.stringify(seen));
  await page.emulate(360);
}

/// Выгрузка для бухгалтера (задача 30) — под списком книги, из её части:
/// ссылка получает файл по адресу `blob:` в самом нажатии, XLSX — пакет ZIP
/// («PK»), CSV — с BOM UTF-8. Браузерные тесты частей не видят, а файл
/// собирает код части книги.
async function exported(page, when) {
  await go(page, "#/ledger");
  const shown = await page.waitFor(VISIBLE("#ledger section.export"));
  if (!checks.check(`${when}: выгрузка под книгой`, shown)) return;
  for (const [key, head, extension] of [["ExportXlsx", "504b0304", ".xlsx"], ["ExportCsvDocuments", "efbbbf", ".csv"]]) {
    const file = await page.eval(`const a = [...document.querySelectorAll("#ledger section.export a")]
        .find((a) => a.textContent.trim() === ${JSON.stringify(s(key))} && a.getClientRects().length > 0);
      if (!a) return { error: "нет ссылки" };
      a.click();
      if (!a.href.startsWith("blob:")) return { error: "адрес не blob: " + a.href };
      const bytes = new Uint8Array(await (await fetch(a.href)).arrayBuffer());
      return { name: a.getAttribute("download"), size: bytes.length,
        head: [...bytes.subarray(0, 4)].map((b) => b.toString(16).padStart(2, "0")).join("") };`);
    checks.check(`${when}: файл «${s(key)}»`,
      file?.head?.startsWith(head) && file.name.endsWith(extension) && file.size > 100, JSON.stringify(file));
  }
  checks.check(`${when}: выгрузка без слов о сбое`, await page.eval(`return document.querySelector("#ledger section.export [role=alert]") ? null : true;`));
  // Адрес прежнего файла снимается: нажата соседняя ссылка — у XLSX его нет,
  // выбран год — нет ни у одной, а имя файла — года (ревью задачи 30). Ссылки
  // файлов — в `.actions`: над ними бывает ссылка на экран налога (задача 83).
  const links = `[...document.querySelectorAll("#ledger section.export .actions a")]`;
  checks.check(`${when}: соседняя ссылка сняла адрес прежнего файла`,
    await page.eval(`return ${links}[0].href.startsWith("blob:") ? null : true;`));
  const year = await page.eval(`[...document.querySelectorAll("#ledger section.export .seg button")][1].click();
    await new Promise((r) => setTimeout(r, 300));
    const all = ${links};
    return { blobs: all.filter((a) => a.href.startsWith("blob:")).length, name: all[0].getAttribute("download") };`);
  checks.check(`${when}: выбран год — адресов прежних файлов нет, имя года`,
    year?.blobs === 0 && /^kvartik-\d{4}\.xlsx$/.test(year?.name ?? ""), JSON.stringify(year));
  await page.eval(`[...document.querySelectorAll("#ledger section.export .seg button")][0].click(); return true;`);
  await drifted(page, when);
}

/// В образце III квартал 2026 закрыт, а книгу поправили после закрытия
/// (задача 83): выгрузка за 2026 год говорит о нём до нажатия, а XLSX несёт
/// лист закрытых кварталов с его сверкой. Книга открыта на сегодняшнем
/// квартале — к 2026 году ведёт «Предыдущий квартал», и проверка не устареет
/// с календарём. Слова и лист рисует код части книги. Что образец обязан для
/// этого держать — в шапке `crates/core/tests/backups/samples.rs`: провал
/// здесь после нового образца — сначала туда.
async function drifted(page, when) {
  const name = `[...document.querySelectorAll("#ledger .quarter-name")].find((e) => e.getClientRects().length > 0)?.textContent ?? ""`;
  for (let i = 0; i < 40 && !(await page.eval(`return ${name};`)).endsWith("2026"); i++) {
    await press(page, "#ledger", s("LedgerPrevQuarter"));
    await sleep(100);
  }
  const seg = `[...document.querySelectorAll("#ledger section.export .seg button")]`;
  await page.eval(`${seg}[1].click(); return true;`);
  const words = s("ExportDrift").split("{}")[1].slice(0, 40);
  checks.check(`${when}: выгрузка за 2026 год говорит о разошедшемся закрытом квартале`,
    await page.waitFor(`return [...document.querySelectorAll("#ledger section.export p[role=alert]")]
      .some((p) => p.textContent.includes(${JSON.stringify(words)})) ? true : null;`), words);
  const sheet = await page.eval(`const a = [...document.querySelectorAll("#ledger section.export a")]
      .find((a) => a.textContent.trim() === ${JSON.stringify(s("ExportXlsx"))} && a.getClientRects().length > 0);
    if (!a) return { error: "нет ссылки" };
    a.click();
    const text = new TextDecoder().decode(await (await fetch(a.href)).arrayBuffer());
    return { sheet: text.includes(${JSON.stringify(s("ExportClosingsSheet"))}),
      drift: text.includes(${JSON.stringify(s("ExportClosingDrift"))}) };`);
  checks.check(`${when}: в XLSX лист закрытых кварталов, сверка — «${s("ExportClosingDrift")}»`,
    sheet?.sheet && sheet?.drift, JSON.stringify(sheet));
  await page.eval(`${seg}[0].click(); return true;`);
}

/// Часть не загрузилась — слова и «Отмена»; снятая блокировка — экран.
async function blocked(browser) {
  const page = await browser.page({ context: true });
  try {
    await page.send("Network.enable");
    await page.send("Network.setBypassServiceWorker", { bypass: true });
    await page.send("Network.setBlockedURLs", { urls: ["*split_book_part-*"] });
    await page.emulate(360);
    await page.go(`${ORIGIN}/app/`);
    await page.setLang(opt.lang);
    checks.check("блокировка: рабочий экран", await page.waitFor(WORKING));
    await go(page, "#/ledger");
    const failed = await page.waitFor(`return ${BUTTON("#ledger", s("Cancel"))} && document.querySelector("#ledger")
      .innerText.includes(${JSON.stringify(s("PartFailed"))}) ? true : null;`);
    checks.check("блокировка: слова о части и «Отмена»", failed);
    await page.send("Network.setBlockedURLs", { urls: [] });
    await press(page, "#ledger", s("Cancel"));
    checks.check("блокировка: «Отмена» — на главный экран",
      await page.waitFor(`return ["", "#/"].includes(location.hash) ? true : null;`));
    await go(page, "#/ledger");
    checks.check("блокировка снята: книга открылась",
      await page.waitFor(`return ${BUTTON("#ledger", s("LedgerAddIncome"))} ? true : null;`));
    checks.check("блокировка: без плашки сбоя", await page.eval(NO_PANIC));
    const errors = [...new Set(page.errors)].filter((e) => !/split_book_part|Failed to fetch/.test(e));
    checks.check("блокировка: других ошибок в консоли нет", errors.length === 0, errors.slice(0, 8).join(" | "));
  } finally {
    await page.close();
  }
}

/// Лендинг (задача 34): страницы языков — готовым HTML с оформлением окна,
/// ссылка в программу и старые адреса программы `/#/…` (до 0.0.34 она жила
/// в корне) — в /app/ с тем же экраном. В своём контексте браузера: service
/// worker, которого поставит переход в программу, не должен править
/// вкладкой следующих шагов.
async function landing(browser) {
  const page = await browser.page({ context: true });
  try {
    await page.emulate(360);
    for (const [lang, at] of [["hy", "/"], ["ru", "/ru/"], ["en", "/en/"]]) {
      await page.go(`${ORIGIN}${at}`);
      checks.check(`лендинг ${at}: страница своего языка`, await page.waitFor(`return document.documentElement.lang
        === ${JSON.stringify(lang)} && document.querySelector("h1")?.textContent
        === ${JSON.stringify(t("WelcomeHeadline", lang))} ? true : null;`));
      // Оформление пришло: заголовок — засечками окна, кнопка — пилюлей.
      checks.check(`лендинг ${at}: оформление и ссылка в программу`, await page.waitFor(`
        const button = document.querySelector('a.button[href="/app/"]');
        return button && getComputedStyle(document.querySelector("h1")).fontFamily.includes("Kvartik Serif")
          && getComputedStyle(button).borderTopLeftRadius !== "0px" ? true : null;`));
      checks.check(`лендинг ${at}: без прокрутки вбок на 360`,
        await page.eval(`return document.documentElement.scrollWidth <= innerWidth ? true : null;`));
    }
    // Калькулятор (задача 34, этап 2): wasm ядра грузится при первом касании
    // формы и отвечает словами страницы. IT 1 000 000 ֏ — 1 % (НК, ст. 258,
    // ч. 1, строка 8): 10 000 ֏; поля, нужные не всегда, не видны, пока ответ
    // их не попросит, — расходы встают у строки с вычетом.
    const field = (name, value) => `const f = document.querySelector('section[data-module] [name="${name}"]');
      f.focus(); f.value = ${JSON.stringify(value)};
      f.dispatchEvent(new Event(f.tagName === "SELECT" ? "change" : "input", { bubbles: true })); return true;`;
    const shown = (name) => `return document.querySelector('[data-field="${name}"]').getClientRects().length > 0;`;
    await page.go(`${ORIGIN}/ru/`);
    await page.waitFor(`return document.querySelector("section[data-module]") ? true : null;`);
    await page.eval(field("turnover", "1 000 000"));
    checks.check("калькулятор: IT 1 000 000 ֏ — налог 10 000 ֏", await page.waitFor(`const s = document
      .querySelector("section[data-module] .result .sum"); return s && s.textContent.replace(/\\D/g, "") === "10000" ? true : null;`));
    checks.check("калькулятор: расходов, пенсии и резидентства не видно",
      !(await page.eval(shown("expenses"))) && !(await page.eval(shown("pension"))) && !(await page.eval(shown("resident"))));
    await page.eval(field("kind", "other"));
    checks.check("калькулятор: у прочей деятельности — поле расходов", await page.waitFor(`return document
      .querySelector('[data-field="expenses"]').getClientRects().length ? true : null;`));
    // «Открыть Kvartik» берёт введённое с собой (задача 34, этап 3): wasm
    // калькулятора оставляет его записью сайта, а программа после экрана
    // первого запуска предлагает взять его в профиль карточкой. Язык окна
    // выбирает браузер, поэтому «Продолжить» — на любом из трёх.
    await page.eval(field("kind", "trade"));
    await page.eval(field("birth", "1985"));
    await page.eval(field("residency", "citizen"));
    await page.waitFor(`return document.querySelector("section[data-module] .result .sum") ? true : null;`);
    await page.eval(`document.querySelector("section[data-module] a[data-handoff]").click(); return true;`);
    checks.check("калькулятор: «Открыть Kvartik» оставил программе введённое", await page.waitFor(`
      if (location.pathname !== "/app/") return null;
      const handed = JSON.parse(localStorage.getItem("kvartik.calc") ?? "null");
      return handed?.kind === "trade" && handed.birth_year === 1985 && handed.residency === "citizen" ? true : null;`));
    const proceed = JSON.stringify(["hy", "ru", "en"].map((lang) => t("WelcomeContinue", lang)));
    await page.waitFor(`const b = [...document.querySelectorAll(".welcome button")].find((b) =>
      ${proceed}.includes(b.textContent.trim())); if (!b) return null; b.click(); return true;`);
    checks.check("программа: предлагает взять введённое в калькуляторе", await page.waitFor(`
      const card = document.querySelector("#home .handoff");
      return card && card.getClientRects().length > 0 && card.innerText.includes("1985") ? true : null;`));
    await page.go(`${ORIGIN}/#/documents`);
    checks.check("лендинг: старый адрес программы /#/… — в /app/ на тот же экран",
      await page.waitFor(`return location.pathname === "/app/" && location.hash === "#/documents" ? true : null;`));
    const errors = [...new Set(page.errors)];
    checks.check("лендинг: без ошибок в консоли", errors.length === 0, errors.slice(0, 8).join(" | "));
  } finally {
    await page.close();
  }
}

/// Несколько ИП (задача 38): «Добавить ИП» на экране «ИП в программе»
/// заводит второго и открывает его перезагрузкой страницы — вкладка помнит
/// его базу, а над экранами его имя; «Открыть» у первого возвращает к нему.
/// Экран — из части книги, и переход — настоящая перезагрузка, которой
/// браузерные тесты не делают. В своём контексте браузера: метка конторы
/// и вторая база не должны попасть в следующие шаги.
async function office(browser) {
  const page = await browser.page({ context: true });
  try {
    await page.emulate(360);
    await page.go(`${ORIGIN}/app/`);
    await page.setLang(opt.lang);
    checks.check("ИП: рабочий экран", await page.waitFor(WORKING));
    await go(page, "#/office");
    checks.check("ИП: «Добавить ИП» нажата", await page.waitFor(`const b = ${BUTTON("#office", s("OfficeAdd"))};
      if (!b) return null; b.click(); return true;`));
    checks.check("ИП: открыт новый — вкладка помнит его базу, над экраном его имя", await page.waitFor(`
      const bar = document.querySelector(".office-bar");
      return (sessionStorage.getItem("kvartik.base") ?? "").startsWith("kvartik-ip-") && bar
        && bar.getClientRects().length > 0 && bar.innerText.includes(${JSON.stringify(s("OfficeNameless"))}) ? true : null;`, 30000));
    // Перезагрузка оставила экран списка — новый ИП пустой, с первыми шагами
    // на главном.
    await go(page, "#/");
    checks.check("ИП: новый — пустой, с первыми шагами", await page.waitFor(VISIBLE("#home .first-steps"), 30000));
    await go(page, "#/office");
    checks.check("ИП: «Открыть» у первого нажата", await page.waitFor(`const b = ${BUTTON("#office", s("OfficeOpen"))};
      if (!b) return null; b.click(); return true;`));
    checks.check("ИП: открыт первый — вкладка помнит его базу", await page.waitFor(`
      return sessionStorage.getItem("kvartik.base") === "kvartik" && document.querySelector(".office-bar") ? true : null;`, 30000));
    checks.check("ИП: без плашки сбоя", await page.eval(NO_PANIC));
    const errors = [...new Set(page.errors)];
    checks.check("ИП: без ошибок в консоли", errors.length === 0, errors.slice(0, 8).join(" | "));
  } finally {
    await page.close();
  }
}

/// Начать переход экрана на `hash` и заморозить его на `at` мс: снимки
/// и въезд экрана стоят, а переходы цвета под мышью доведены до конца — их
/// ведёт мышь машины, а не переход. Тело функции `page.eval`; отпускает
/// `THAW`.
const freeze = (hash, at = 150) => `
  location.hash = ${JSON.stringify(hash)};
  const moving = () => document.getAnimations().some((a) => String(a.effect?.pseudoElement ?? "").startsWith("::view-transition"));
  for (let i = 0; i < 300 && !moving(); i++) await new Promise((r) => requestAnimationFrame(r));
  if (!moving()) return { error: "перехода нет" };
  const running = document.getAnimations().filter((a) => a.playState === "running" && a.timeline === document.timeline);
  for (const a of running) if (a instanceof CSSTransition) a.finish(); else { a.pause(); a.currentTime = ${at}; }
  window.__frozen = running;
  await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));`;
const THAW = `for (const a of window.__frozen ?? []) a.finish(); window.__frozen = null; return true;`;

/// Меню телефона посреди замороженного перехода (`freeze`): ложится ли на
/// него что-нибудь из перехода — снимок прежнего экрана, вкладки, заголовка
/// карточки — и та же ли вкладка открыта, что после. Кадр сравнивается
/// побайтно с тем же кадром, где слой перехода (`::view-transition`)
/// прозрачен: разница — только то, что кладёт сам переход. Не с кадром
/// после перехода: без видеокарты, как в CI, Chrome растрирует то же меню
/// в другом состоянии страницы чуть иначе. При закрытой карточке кадры
/// разошлись в CI (Chrome 156), хотя на меню не лежало ничего, а у Chrome
/// 157 с `--disable-gpu` — на 33 точки сглаживания края значка «%»
/// у «Налога» (2026-11-08); кадры одного состояния совпадают. Вкладка — по
/// разметке. Отпускает переход (`THAW`).
async function overMenu(page, clip) {
  const shot = async () => (await page.send("Page.captureScreenshot", { format: "png", clip })).data;
  const TAB = `return document.querySelector('.app > .nav a[aria-current="page"]')?.getAttribute("href") ?? "";`;
  const during = await shot();
  const tab = await page.eval(TAB);
  await page.eval(`const sheet = new CSSStyleSheet();
    sheet.replaceSync("::view-transition { opacity: 0 !important; }");
    document.adoptedStyleSheets = [...document.adoptedStyleSheets, sheet];
    window.__bare = sheet;
    await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
    return true;`);
  const bare = await shot();
  await page.eval(`document.adoptedStyleSheets = document.adoptedStyleSheets.filter((s) => s !== window.__bare);
    window.__bare = null; ${THAW}`);
  await sleep(300);
  const after = await page.eval(TAB);
  return {
    ok: during === bare && tab === after,
    detail: `${during === bare ? "" : "на меню лежит переход; "}вкладка ${tab}, после — ${after}`,
  };
}

/// Движение (задача 86), на компьютере — ширина 1280, мышь. Переход экрана
/// снимает только прежний экран: новый живой, и нажатие посреди перехода
/// попадает в него, а не в снимок страницы (`elementFromPoint`, как
/// браузер ищет цель нажатия). Под мышью кнопка получает точку входа,
/// а кружок у курсора встаёт над ней — виден он, только если у системы есть
/// мышь. На телефоне меню вкладок — внизу, поверх экрана, и посреди перехода
/// на него не ложится ничего, а открыта та же вкладка, что после него. У
/// того, кто выключил анимацию в системе, — ни перехода, ни одной анимации,
/// ни кружка. У лендинга — переход между страницами языков в стилях
/// и кружок под мышью.
/// В своём контексте браузера: эмуляция медиа не должна попасть в
/// следующие шаги.
async function motion(browser) {
  const page = await browser.page({ context: true });
  const media = (motion) => page.send("Emulation.setEmulatedMedia", {
    features: [{ name: "prefers-color-scheme", value: "light" }, { name: "prefers-reduced-motion", value: motion }],
  });
  const mouse = (x, y) => page.send("Input.dispatchMouseEvent", { type: "mouseMoved", x, y });
  const TRANSITION = `return document.getAnimations().some((a) =>
    String(a.effect?.pseudoElement ?? "").startsWith("::view-transition")) ? true : null;`;
  // Середина видимой кнопки раздела, и мышь — туда.
  const hover = async (scope) => {
    const at = await page.eval(`const b = [...document.querySelectorAll(${JSON.stringify(`${scope} :is(button, .button)`)})]
      .find((b) => b.getClientRects().length > 0 && !b.disabled);
    if (!b) return null; b.scrollIntoView({ block: "center" }); const r = b.getBoundingClientRect();
    b.dataset.hovered = ""; return { x: r.x + r.width / 2, y: r.y + r.height / 2 };`);
    if (at) {
      await mouse(at.x - 40, at.y);
      await mouse(at.x, at.y);
      await sleep(100);
    }
    return at;
  };
  try {
    await page.emulate(1280);
    await media("no-preference");
    await page.go(`${ORIGIN}/app/`);
    await page.setLang(opt.lang);
    checks.check("движение: рабочий экран", await page.waitFor(WORKING));
    await page.eval(`location.hash = "#/clients"; return true;`);
    checks.check("движение: экран сменился переходом", await page.waitFor(TRANSITION, 3000));
    checks.check("движение: посреди перехода нажатие — новому экрану, а не снимку", await page.eval(`
      const h = document.querySelector("#clients h1"); if (!h || !h.getClientRects().length) return null;
      const r = h.getBoundingClientRect();
      return document.getElementById("clients").contains(document.elementFromPoint(r.x + 8, r.y + r.height / 2)) || null;`));
    await sleep(700);
    checks.check("движение: под мышью у кнопки — точка входа", Boolean(await hover("#clients")) && await page.waitFor(`
      const b = document.querySelector("[data-hovered]");
      return b && b.style.getPropertyValue("--pointer-x") !== "" ? true : null;`, 3000));
    // Виден ли кружок, решает мышь системы (`hover: hover` и `pointer:
    // fine`), а не события: у headless Chrome на Linux в CI мыши нет —
    // и кружка нет, как у телефона. Эмулировать эти признаки протокол
    // DevTools не умеет (`Emulation.setEmulatedMedia`, Chrome 157,
    // 2026-11-06), поэтому проверяется то, что верно для этой машины.
    const pointing = await page.eval(`return matchMedia("(hover: hover) and (pointer: fine)").matches;`);
    checks.check(`движение: кружок у курсора — над кнопкой, ${pointing ? "виден: мышь есть" : "не виден: мыши нет"}`,
      await page.waitFor(`const c = document.querySelector(".cursor");
      return c && c.dataset.over === "press" && getComputedStyle(c).display === ${JSON.stringify(pointing ? "block" : "none")}
        ? true : null;`, 3000));

    // Телефон (найдено на телефоне 2026-11-07): посреди перехода на меню
    // вкладок не ложится ничего (`overMenu`). Снимок прежнего экрана браузер
    // рисует над страницей, и у книги под меню лежит выгрузка: необрезанный,
    // он лёг бы на вкладки; снятая открытая вкладка переезжала бы к новой
    // значком и подписью и не нажималась бы. «Новый счёт» закреплён у окна,
    // а экран въезжает сдвигом: пока экран не встал, кнопки нет, а не стоит
    // она не на месте. Экран счетов — сначала показать: он частью wasm.
    await mouse(5, 5);
    await page.emulate(360);
    await media("no-preference");
    await page.eval(`location.hash = "#/documents"; return true;`);
    await page.waitFor(`return document.querySelector("#documents .fab") ? true : null;`);
    await page.eval(`location.hash = "#/ledger"; return true;`);
    await page.waitFor(VISIBLE("#ledger .export"));
    await sleep(900);
    const middle = await page.eval(`${freeze("#/clients")}
      const menu = document.querySelector(".app > .nav").getBoundingClientRect();
      const top = Math.ceil(menu.top) + 2;
      return {
        missed: [...document.querySelectorAll(".tabs a")].filter((a) => {
          const r = a.getBoundingClientRect();
          return !a.contains(document.elementFromPoint(r.x + r.width / 2, r.y + r.height / 2));
        }).map((a) => a.getAttribute("href")),
        clip: { x: 0, y: top, width: Math.floor(menu.width), height: Math.floor(menu.bottom) - top, scale: 1 },
      };`);
    if (!middle?.clip) {
      checks.check("движение, телефон: экран сменился переходом", false, middle?.error ?? "");
    } else {
      const menu = await overMenu(page, middle.clip);
      checks.check("движение, телефон: посреди перехода на меню ничего не лежит", menu.ok, menu.detail);
      checks.check("движение, телефон: посреди перехода каждая вкладка нажимается", middle.missed.length === 0,
        middle.missed.join(", "));
    }
    const FAB = `const b = document.querySelector("#documents .fab"); const r = b.getBoundingClientRect();
      return { shown: getComputedStyle(b).visibility !== "hidden", at: Math.round(r.left) + "," + Math.round(r.top) };`;
    const entering = await page.eval(`${freeze("#/documents")} ${FAB}`);
    await page.eval(THAW);
    await sleep(1300);
    const settled = await page.eval(FAB);
    checks.check("движение, телефон: «Новый счёт», пока экран въезжает, — не на чужом месте",
      Boolean(entering?.at) && (!entering.shown || entering.at === settled?.at),
      `посреди ${entering?.at ?? entering?.error}, потом ${settled?.at}`);
    await page.emulate(1280);

    await media("reduce");
    await mouse(5, 5);
    await sleep(700);
    await page.eval(`location.hash = "#/"; return true;`);
    await sleep(150);
    checks.check("без движения: экран сменился без перехода",
      (await page.waitFor(VISIBLE("#home"), 3000)) && !(await page.eval(TRANSITION)));
    await hover("#home");
    checks.check("без движения: ни одной анимации и кружка нет", await page.eval(`
      return document.getAnimations().length === 0
        && getComputedStyle(document.querySelector(".cursor")).display === "none" ? true : null;`),
      await page.eval(`return document.getAnimations().map((a) => a.animationName ?? a.transitionProperty ?? a.constructor.name).join(", ");`));

    await media("no-preference");
    await page.go(`${ORIGIN}/ru/`);
    checks.check("движение: у лендинга — переход между страницами", await page.waitFor(`
      const rules = (list) => [...list].some((r) => r.constructor.name === "CSSViewTransitionRule"
        || (r.cssRules && rules(r.cssRules)));
      return [...document.styleSheets].some((sheet) => rules(sheet.cssRules)) ? true : null;`));
    await hover(".hero");
    checks.check("движение: у лендинга кружок — над кнопкой", await page.waitFor(`const c = document.querySelector(".cursor");
      const b = document.querySelector("[data-hovered]");
      return c?.dataset.over === "press" && b?.style.getPropertyValue("--pointer-x") !== "" ? true : null;`, 3000));
    checks.check("движение: без плашки сбоя", await page.eval(NO_PANIC));
    const errors = [...new Set(page.errors)];
    checks.check("движение: без ошибок в консоли", errors.length === 0, errors.slice(0, 8).join(" | "));
  } finally {
    await page.close();
  }
}

/// Восстановить последний образец копии: в нём есть выпущенные документы.
/// Пароль образца станет паролем копий устройства.
async function restore(page) {
  const sample = samples().at(-1);
  await go(page, "#/backup");
  await page.setFiles(`input[type=file][accept=".json,application/json"]`, [sample.file]);
  const submit = `[...document.querySelectorAll("form.backup button[type=submit]")].find((b) =>
    b.textContent.trim() === ${JSON.stringify(s("BackupRestore"))} && b.getClientRects().length > 0)`;
  checks.check("копия: образец открыт, спрошен пароль", await page.waitFor(`return ${submit} ? true : null;`));
  await page.eval(`const b = ${submit}; const input = b.form.querySelector("input[type=password]");
    input.value = ${JSON.stringify(sample.password)}; input.dispatchEvent(new Event("input", { bubbles: true }));
    b.click(); return true;`);
  const done = s("BackupRestored").split("{}")[0];
  checks.check(`копия: образец ${path.basename(path.dirname(sample.file))} восстановлен`,
    await page.waitFor(`return [...document.querySelectorAll("p[role=status]")].some((p) =>
      p.textContent.startsWith(${JSON.stringify(done)})) ? true : null;`, 60000));
  return sample;
}

/// Новая копия: пароль — во все видимые поля формы копии (новый пароль
/// с повтором или пароль устройства).
async function backup(page, password) {
  await go(page, "#/backup");
  const made = await page.eval(`const form = ${BUTTON("#backup", s("BackupMake"))}?.form;
    if (!form) return { error: "нет формы копии" };
    for (const input of form.querySelectorAll("input[type=password]")) {
      input.value = ${JSON.stringify(password)}; input.dispatchEvent(new Event("input", { bubbles: true }));
    }
    form.requestSubmit(); return true;`);
  checks.check("копия: форма отправлена", made === true, made?.error);
  checks.check("копия: файл готов", await page.waitFor(VISIBLE(`a[download$=".json"]`), 30000));
}

const NO_DIALOG = `return document.querySelector("[role=dialog]") ? null : true;`;

/// Вписать текст в поле, как человек: со своим событием `input`.
const TYPE = (selector, text) => `const input = document.querySelector(${JSON.stringify(selector)});
  if (!input) return null;
  input.value = ${JSON.stringify(text)}; input.dispatchEvent(new Event("input", { bubbles: true })); return true;`;

/// Что держит новую версию, кроме форм: начатый ввод вне формы.
const FIELDS = [
  { name: "ключ лицензии", hash: "#/license", field: "#license textarea" },
  { name: "пароль копии", hash: "#/backup", field: "#backup form.backup input[type=password]" },
];

/// Всё, что держит новую версию, — одним списком: начать и закончить.
const HOLDS = [
  ...FORMS.map((f) => ({
    name: `форма экрана «${f.name}»`,
    start: async (page) => {
      await go(page, f.hash);
      await page.waitFor(`return ${BUTTON(f.scope, s(f.open))} ? true : null;`);
      await press(page, f.scope, s(f.open));
      return page.waitFor(f.shown);
    },
    finish: async (page) => {
      await go(page, f.hash);
      await press(page, f.scope, s(f.close));
      return page.waitFor(f.gone);
    },
  })),
  ...FIELDS.map((f) => ({
    name: f.name,
    start: async (page) => {
      await go(page, f.hash);
      return page.waitFor(TYPE(f.field, "начато"));
    },
    finish: async (page) => {
      await go(page, f.hash);
      return page.waitFor(TYPE(f.field, ""));
    },
  })),
  // Готовый файл копии: держит, пока открыт её экран.
  {
    name: "файл копии на её экране",
    start: async (page, password) => {
      await backup(page, password);
      return true;
    },
    finish: async (page) => {
      await go(page, "#/");
      return true;
    },
  },
];

/// Версия, о которой человек знает, — на устройстве. Копия сборки — тот же
/// wasm с теми же числами (`newVersion`), поэтому после её включения окно
/// «Что нового» встанет, только если устройство помнит версию старше.
const SEEN = (version) => `localStorage.setItem("kvartik.seen_version", ${JSON.stringify(version)}); return true;`;

/// Заработала сборка `next`: прежний кэш удалён, стоит окно «Что нового»
/// с заметкой работающей версии, а не той, что в заметках копии сверху.
/// Программа под окном неактивна, «Понятно» его закрывает, и перезагрузка его
/// больше не показывает. Включение здесь доказывает кэш, а не метка на
/// странице: при заходе на страницу она могла бы встать уже на новую сборку.
/// Ждётся именно кэш новой сборки: до того, как браузер нашёл её, единственный
/// кэш — прежний.
async function switched(page, next, running, when) {
  const name = `kvartik-${next.build}`;
  const cache = await page.waitFor(`const n = await caches.keys();
    return n.length === 1 && n[0] === ${JSON.stringify(name)} ? n[0] : null;`, 30000);
  checks.check(`новая версия: ${when} — новая сборка, прежний кэш удалён`, cache === name, cache);
  const dialog = await page.waitFor(`const d = document.querySelector("[role=dialog]");
    return d && d.innerText.includes(${JSON.stringify(s("UpdateWhatsNew"))}) ? d.innerText : null;`, 10000);
  checks.check(`новая версия: ${when} — окно «Что нового» с заметкой ${running}`,
    dialog?.includes(running) && !dialog.includes(next.version), dialog);
  checks.check("новая версия: программа под окном неактивна",
    await page.eval(`return document.querySelector(".app").inert ? true : null;`));
  checks.check("новая версия: «Понятно» в окне", await press(page, "[role=dialog]", s("UpdateGotIt")));
  checks.check("новая версия: «Понятно» закрыло окно, программа работает",
    (await page.waitFor(NO_DIALOG)) && (await page.waitFor(WORKING))
      && (await page.eval(`return document.querySelector(".app").inert ? null : true;`)));
  await page.fresh(`${ORIGIN}/app/#/`);
  await page.waitFor(WORKING);
  await sleep(300);
  checks.check("новая версия: после перезагрузки окна больше нет", await page.eval(NO_DIALOG));
}

/// Новая версия — на той же странице: сервер отдаёт копию сборки новой
/// версией (`newVersion`), а проверку новой версии окно делает и само, при
/// возвращении к нему (`platform/pwa.rs`) — здесь её зовёт проверка, пока
/// что-то начато. Новую версию держит каждая форма и поле сами: они
/// открываются по очереди, и следующее — раньше, чем закончено прежнее, — так
/// в каждый миг начато одно, и страница стоит. То, что версию не держит, —
/// перезагрузка, а метка `__old` на странице её выдаёт. Потом — заход на
/// страницу: браузер находит новую версию сам, и она включается без форм.
/// И последнее: версию, которую на устройстве уже пробовали включить, окно
/// второй раз не включает — иначе сборка, которая не заработала, перезагружала
/// бы страницу без конца, и открыть программу было бы нельзя.
async function update(page, password) {
  const running = JSON.parse(fs.readFileSync(path.join(DIST, "app/notes.json"), "utf8")).notes[0].version;
  const next = newVersion(DIST, path.join(WORK, "v2"));
  const later = newVersion(next.dir, path.join(WORK, "v3"));
  const refused = newVersion(later.dir, path.join(WORK, "v4"));
  let srv = own(next.dir, PORT, { quality: 5 });
  const NOTICE = TEXT_IN("main", s("UpdateAfterForm"));
  try {
    page.errors.length = 0;
    await page.eval(SEEN("0.0.1"));
    await page.eval(`window.__old = 1; return true;`);
    let open = null;
    for (const hold of HOLDS) {
      checks.check(`новая версия: начато — ${hold.name}`, await hold.start(page, password));
      if (open) {
        checks.check(`новая версия: закончено — ${open.name}`, await open.finish(page));
      } else {
        await page.eval(`navigator.serviceWorker.getRegistration().then((r) => r.update()); return true;`);
      }
      const waiting = await page.waitFor(NOTICE, open ? 5000 : 30000);
      await sleep(300);
      checks.check(`новая версия: начато одно — ${hold.name}; над экраном слова, страница стоит`,
        waiting && (await page.eval(`return window.__old === 1 ? true : null;`)) && (await page.eval(NO_DIALOG)));
      open = hold;
    }
    await open.finish(page);
    checks.check("новая версия: всё закрыли — страница перезагрузилась сама",
      await page.waitFor(`return window.__old === undefined && document.readyState === "complete" ? true : null;`, 30000));
    await switched(page, next, running, "всё закрыли");

    // Заход на страницу: новую версию находит сам браузер, форм нет.
    await srv.stop();
    srv = own(later.dir, PORT, { quality: 5 });
    await page.eval(SEEN("0.0.1"));
    await page.fresh(`${ORIGIN}/app/#/`);
    await switched(page, later, running, "зашли на страницу");

    // Попытка включить эту версию уже была: она ждёт, а страница стоит.
    await srv.stop();
    srv = own(refused.dir, PORT, { quality: 5 });
    await page.eval(`localStorage.setItem("kvartik.update_tried", ${JSON.stringify(refused.version)});
      window.__old = 1; return true;`);
    await page.eval(`navigator.serviceWorker.getRegistration().then((r) => r.update()); return true;`);
    const waiting = await page.waitFor(`const r = await navigator.serviceWorker.getRegistration();
      return r.waiting ? true : null;`, 30000);
    await sleep(1500);
    checks.check("новая версия: уже пробовали — ждёт, страница не перезагрузилась",
      waiting && (await page.eval(`return window.__old === 1 ? true : null;`)));

    checks.check("новая версия: без плашки сбоя", await page.eval(NO_PANIC));
    const errors = [...new Set(page.errors)];
    checks.check("новая версия: без ошибок в консоли", errors.length === 0, errors.slice(0, 8).join(" | "));
  } finally {
    await srv.stop();
  }
}

/// Первые шаги нового человека: карточка приходит частью профиля, ведёт
/// в короткую форму той же части, а «Отмена» возвращает на главный экран.
async function firstSteps(page) {
  checks.check("первые шаги: карточка из части", await page.waitFor(VISIBLE("#home .first-steps")));
  checks.check("первые шаги: кнопка шага", await press(page, "#home", s("StepsFillRequisites")));
  checks.check("первые шаги: короткая форма", await page.waitFor(TEXT_IN("#profile", s("ProfileBriefAbout"))));
  checks.check("первые шаги: «Отмена» в форме", await press(page, "#profile", s("Cancel")));
  checks.check("первые шаги: снова на главном экране", await page.waitFor(VISIBLE("#home .first-steps")));
}

async function main() {
  const parts = fs.readdirSync(path.join(DIST, "app")).filter((n) => /^split_\w+-\w+\.wasm$/.test(n));
  checks.check("в сборке есть части wasm", parts.length > 0, parts.join(", "));
  let srv = own(DIST, PORT, { quality: 5 });
  const browser = await Browser.launch({ work: WORK, name: "check", port: 9340 });
  try {
    await blocked(browser);
    await landing(browser);
    await office(browser);
    await motion(browser);

    const page = await browser.page();
    await page.emulate(360);
    await page.go(`${ORIGIN}/app/`);
    await page.setLang(opt.lang);
    checks.check("с сетью: рабочий экран", await page.waitFor(WORKING));
    checks.check("первый запуск: версия запомнена, окна «Что нового» нет",
      (await page.eval(`return localStorage.getItem("kvartik.seen_version");`)) && (await page.eval(NO_DIALOG)));
    await firstSteps(page);
    const sample = await restore(page);
    checks.check("с сетью: страницей правит service worker", (await page.waitFor(CONTROLLER)) === `${ORIGIN}/app/sw.js`);
    await home(page);
    await go(page, "#/");
    checks.check("первые шаги: у копии с документами карточки нет", await page.waitFor(HIDDEN("#home .first-steps")));
    await forms(page, "с сетью");
    await screens(page, "с сетью");
    await card(page, "с сетью");
    await cardClose(page);
    await exported(page, "с сетью");
    await backup(page, sample.password);
    const loaded = await page.eval(`return performance.getEntriesByType("resource").map((e) => e.name);`);
    const missing = parts.filter((p) => !loaded.some((u) => u.endsWith(`/app/${p}`)));
    checks.check("с сетью: все части загружены", missing.length === 0, missing.join(", "));
    checks.check("с сетью: без плашки сбоя", await page.eval(NO_PANIC));
    const errors = [...new Set(page.errors)];
    checks.check("с сетью: без ошибок в консоли", errors.length === 0, errors.slice(0, 8).join(" | "));

    await srv.stop();
    srv = null;
    page.errors.length = 0;
    await page.fresh(`${ORIGIN}/app/#/`);
    checks.check("без сети: рабочий экран из кэша",
      (await page.waitFor(WORKING)) && (await page.eval(CONTROLLER)) === `${ORIGIN}/app/sw.js`);
    await forms(page, "без сети");
    await screens(page, "без сети");
    await card(page, "без сети");
    await exported(page, "без сети");
    checks.check("без сети: без плашки сбоя", await page.eval(NO_PANIC));
    const offline = [...new Set(page.errors)];
    checks.check("без сети: без ошибок в консоли", offline.length === 0, offline.slice(0, 8).join(" | "));

    await update(page, sample.password);
  } finally {
    browser.close();
    if (srv) await srv.stop();
  }
}

try {
  await main();
} catch (e) {
  checks.check("проверка дошла до конца", false, e.stack);
}
checks.summary();
if (!opt.work) fs.rmSync(WORK, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 });
process.exit(checks.failed ? 1 : 0);
