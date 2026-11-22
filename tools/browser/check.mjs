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
// 0. Сервер пробного периода (задача 106): функция сайта `_worker.js`
//    отвечает первым днём на ՀՎՀՀ тем видом, что читает ядро, хранит отпечаток,
//    а не номер, и отказывает не тем запросам — в Node, без браузера. Она же
//    отдаёт wasm программы его копией уровнем brotli 11 (задача 109), а когда
//    нельзя — самим wasm.
// 1. Часть не загрузилась — окно говорит словами и даёт «Отмену», а снятая
//    блокировка открывает экран (CLAUDE.md, Правило 2, «Формы и действия по
//    нажатию — частями wasm»). Лендинг (задача 34): страница каждого языка
//    со своим оформлением и ссылкой в программу, без прокрутки вбок на
//    телефоне, калькулятор отвечает числом ядра и прячет ненужные поля,
//    «Открыть Kvartik» берёт введённое с собой и программа предлагает его
//    в профиль, а старый адрес программы `/#/…` ведёт в /app/ на тот же
//    экран. Несколько ИП (задача 38): второй заводится и открывается
//    перезагрузкой, и первый открывается снова. Согласие на сверку ИНН
//    (задача 106): ссылки экрана лицензии — цветом ссылок программы, а не
//    синим браузера (0.0.153).
// 2. С сетью: восстановление образца копии, экраны и их формы, карточка
//    выпущенного документа, PDF, выгрузка с закрытым кварталом, разошедшимся
//    с поданным, новая копия, свои цвета настроек и они же после
//    перезагрузки (задача 88), ИИ-помощник с поставщиком-подменой за
//    настоящим CSP (задача 102) — каждая из десяти частей;
//    и все части `dist/app/split_*.wasm` загружены. Курс ЦБ (задача 43):
//    программа выкладки пишет из образца ответа ЦБ тот же файл курсов, что
//    читает тест ядра, программа берёт его со своего сайта, помнит
//    и подсказывает у поля курса формы книги. «Установить» на
//    главном экране — и без вопроса браузера, и раскладка, которую видно
//    только с настоящим CSS: раздел с отступом под карточкой налога.
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
import { pathToFileURL } from "node:url";
import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import zlib from "node:zlib";
import { Browser, Checks, DEVICES, sleep } from "./cdp.mjs";
import { REPO, newVersion, own } from "./serve.mjs";
import { samples, t } from "./strings.mjs";
import { parse as parseCba, render as renderRates } from "../deploy/rates.mjs";
import { changedPages, indexKey, pages as sitemapPages, request as indexNowRequest } from "../deploy/indexnow.mjs";

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
// Курсы ЦБ (задача 43): файл курсов кладёт на сайт выкладка, а не сборка, —
// сервер проверки отдаёт образец, который читает и тест ядра (`tax::cba`).
const RATES_DIR = path.join(REPO, "crates/core/tests/rates");
const RATES_FILE = fs.readFileSync(path.join(RATES_DIR, "rates.json"), "utf8");
const SERVED = { quality: 5, extra: { "/app/rates.json": RATES_FILE } };
const s = (key) => t(key, opt.lang);
const checks = new Checks();
// Страница «Купить» (задача 35) на языке `lang` — `release::buy_path`,
// а цена и счёт продавца — константы `release.rs`: проверка читает их оттуда
// же, откуда их берёт сборка, а не держит свою копию.
const buyPath = (lang) => (lang === "hy" ? "/buy/" : `/${lang}/buy/`);
// Страница «Отзывы и предложения» (задача 114) — `release::feedback_path`,
// а почта и Telegram продавца — `release::EMAIL` и `TELEGRAM`.
const feedbackPath = (lang) => (lang === "hy" ? "/feedback/" : `/${lang}/feedback/`);
const RELEASE = fs.readFileSync(path.join(REPO, "crates/core/src/release.rs"), "utf8");
const ACCOUNT = RELEASE.match(/pub const ACCOUNT: &str = "(\d+)";/)?.[1];
const PRICE = RELEASE.match(/pub const PRICE: Money = Money::new\(([\d_]+), Currency::Amd\);/)?.[1]?.replaceAll("_", "");
// Цена «Pro» (задача 103) — вторая карточка той же страницы.
const PRO_PRICE = RELEASE.match(/pub const PRO_PRICE: Money = Money::new\(([\d_]+), Currency::Amd\);/)?.[1]?.replaceAll("_", "");
const EMAIL = RELEASE.match(/pub const EMAIL: &str = "([^"]+)";/)?.[1];
const TELEGRAM = RELEASE.match(/pub const TELEGRAM: &str = "([^"]+)";/)?.[1];
if (!ACCOUNT || !PRICE || !PRO_PRICE || !EMAIL || !TELEGRAM) {
  throw new Error("в crates/core/src/release.rs не нашлись ACCOUNT, PRICE, PRO_PRICE, EMAIL и TELEGRAM");
}

// Рабочий экран: база открыта, разделы прочитаны — под экраном копии встают
// слова о хранении на языке окна (с 0.0.142; так же в skill browser-run).
// С 0.0.20 признаком было поле выбора файла копии, встававшее тогда же, но
// с задачи 91 экран копии рисуется с первого показа, а слова о хранении —
// в главном wasm и в тот же миг, что поле раньше.
const STORED = ["hy", "ru", "en"].map((lang) => t("StorageTitle", lang));
const WORKING = `return [...document.querySelectorAll("#backup h2")].some((h) =>
  ${JSON.stringify(STORED)}.includes(h.textContent)) ? true : null;`;
// Поле выбора файла копии — когда экран копии показан и база открыта.
const BACKUP_FILE = `input[type=file][accept=".json,application/json"]`;
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
/// Виден ли текст внутри раздела — выражением, для своих условий.
const SHOWN_IN = (scope, text) =>
  `document.querySelector(${JSON.stringify(scope)})?.innerText.includes(${JSON.stringify(text)})`;

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
/// (задача 24), лицензия (задача 25), «О программе» (задача 31) и настройки
/// (задача 88). «Сверьте расчёт с бухгалтером» части годовых платежей
/// и сроков, слова о полях формы — часть чисел, о ключе и ссылку на
/// страницу «Купить» своего языка (задача 35) — часть лицензии, правила
/// налога и ссылку на отзывы (задача 114) — часть «О программе», а слова о кружке у курсора — часть
/// настроек рисуют всегда, и с профилем, и без него.
const SCREENS = [
  { name: "Налог", hash: "#/tax", shown: TEXT_IN("#tax", s("TaxTurnover")) },
  { name: "Годовые платежи", hash: "#/annual", shown: TEXT_IN("#annual", s("TaxCheck")) },
  { name: "Сроки", hash: "#/deadlines", shown: TEXT_IN("#deadlines", s("DeadlinesShiftRule")) },
  { name: "Числа для расчёта", hash: "#/report", shown: TEXT_IN("#report", s("ReportAbout")) },
  { name: "Лицензия", hash: "#/license", shown: `return document.querySelector('#license a[href="${buyPath(opt.lang)}"]')
      && document.querySelector("#license")?.innerText.includes(${JSON.stringify(s("LicenseGives"))}) ? true : null;` },
  // «О программе» ведёт на «Отзывы и предложения» своего языка (задача 114)
  // новой вкладкой — как на условия.
  { name: "О программе", hash: "#/about", shown: `return document.querySelector('#about a[href="${feedbackPath(opt.lang)}"][target="_blank"]')
      && document.querySelector("#about")?.innerText.includes(${JSON.stringify(s("AboutRules"))}) ? true : null;` },
  { name: "Настройки", hash: "#/settings", shown: TEXT_IN("#settings", s("SettingsCursorHint")) },
  // Другое устройство (задача 110): слова о нём — часть экрана. Камеру здесь
  // не включают: у браузера проверки её нет, и отказ ушёл бы в консоль.
  { name: "Другое устройство", hash: "#/sync", shown: TEXT_IN("#sync", s("SyncAbout")) },
  // На адресе проверки пары у рамки нет, и окно говорит, где помощник;
  // саму рамку проверяет шаг «ИИ-помощник» — под своими адресами.
  { name: "ИИ-помощник", hash: "#/ask", shown: TEXT_IN("#ask", s("AiFrameAlone")) },
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

/// Свои цвета (задача 88) на собранном сайте. Пока своих нет, образец фона —
/// фон темы устройства, и тема, сменившаяся при открытом экране, меняет
/// и его: экран, показанный однажды, не снимается (найдено глазным прогоном
/// задачи 88 — образцы оставались светлыми в тёмной теме). Выбранный фон
/// встаёт на страницу сразу — его ставит часть настроек, — а после
/// перезагрузки его ставит главный wasm из записи устройства, ещё до части.
/// «Вернуть цвета программы» снимает его со страницы и с устройства.
///
/// С 0.0.156 (задача 89) свой фон — уже в первом кадре, до wasm: копию
/// палитры окно кладёт в кэш, а service worker вписывает в страницу ссылку
/// на неё и тему её мета-тегу. Первый кадр меряет `firstFrame`: окно ещё не
/// поставило на `<html>` ни одного свойства, а фон уже свой. Ссылку снимает
/// часть настроек, когда загрузились части. Без своих цветов копии нет,
/// и первый кадр — в цветах программы.
async function settings(page) {
  const SURFACE = `getComputedStyle(document.documentElement).getPropertyValue("--surface").trim()`;
  const SWATCH = `return document.querySelector("#settings input[type=color]")?.value === ${SURFACE} ? true : null;`;
  await go(page, "#/settings");
  checks.check("настройки: образец фона — фон темы устройства", await page.waitFor(SWATCH));
  await page.emulate(360, "dark");
  checks.check("настройки: тема сменилась — образец фона за ней", await page.waitFor(SWATCH, 3000));
  await page.emulate(360);
  checks.check("настройки: и обратно", await page.waitFor(SWATCH, 3000));
  const picked = await page.waitFor(`const field = document.querySelector("#settings input[type=color]");
    if (!field || !field.getClientRects().length) return null;
    field.value = "#0b1d33";
    field.dispatchEvent(new Event("input", { bubbles: true }));
    return ${SURFACE} === "#0b1d33" ? true : null;`);
  checks.check("настройки: свой фон встал на страницу", picked);
  const copied = await page.waitFor(`const look = await caches.match("look.css", { cacheName: "kvartik.look" });
    return look && (await look.text()).includes("--surface:#0b1d33;") ? look.headers.get("Kvartik-Mark") : null;`);
  checks.check("настройки: копия своих цветов — в кэше", copied);
  const first = await firstFrame(page);
  checks.check("настройки: свой фон — с первого кадра, до wasm",
    first && first.background === "rgb(11, 29, 51)" && first.inline === 0 && first.link === copied
      && first.scheme === "dark", JSON.stringify(first));
  checks.check("настройки: свой фон — и после перезагрузки, а ссылку на копию окно сняло",
    (await page.waitFor(WORKING)) && (await page.waitFor(`return ${SURFACE} === "#0b1d33"
      && !document.querySelector("link[data-look]") ? true : null;`)));
  await go(page, "#/settings");
  const reset = await page.waitFor(`return ${BUTTON("#settings", s("ColorsResetAll"))} ? true : null;`);
  checks.check("настройки: «Вернуть цвета программы» есть", reset && await press(page, "#settings", s("ColorsResetAll")));
  checks.check("настройки: цвета программы вернулись", await page.waitFor(`
    return document.documentElement.style.getPropertyValue("--surface") === ""
      && localStorage.getItem("kvartik.colors") === null ? true : null;`));
  checks.check("настройки: копии своих цветов больше нет",
    await page.waitFor(`return (await caches.has("kvartik.look")) ? null : true;`));
  const plain = await firstFrame(page);
  const background = (await page.waitFor(WORKING))
    && await page.eval(`return getComputedStyle(document.documentElement).backgroundColor;`);
  checks.check("настройки: без своих цветов первый кадр — в цветах программы",
    plain && plain.link === null && plain.scheme === "light dark" && plain.background === background,
    `${JSON.stringify(plain)}, после запуска — ${background}`);
}

/// ИИ-помощник (задачи 102 и 111) на собранном сайте — рамкой своего
/// происхождения, с настоящими CSP обоих: окно `https://kvartik.com/app/`
/// и рамка `https://ai.kvartik.com/` — подменой протокола DevTools
/// (`Fetch.enable`): их файлы отдают свои серверы проверки из `dist/`
/// и `dist-ai/` с заголовками из своих `_headers`, а в сеть не уходит ничего.
/// Так и пары происхождений — те, что в выкладке: окно знает свою рамку,
/// а рамка — своё окно (`ai::frame`). Service worker у окна здесь нет —
/// `navigator.serviceWorker` снят до первого скрипта, и окно работает, как
/// в браузере без него (`platform::pwa::container`): скрипт service worker
/// браузер берёт мимо перехвата страницы, и встал бы настоящий `sw.js`
/// с настоящего сайта, а с ним и запросы частей туда (2026-11-20).
/// Поставщик — подменой тоже: запрос к четырём адресам перехватывается уже
/// после того, как браузер сверил его с CSP, и получает поток SSE, как от
/// Anthropic. Предварительный запрос CORS (`OPTIONS`) — подменой: Chrome
/// показывает его перехвату (по протоколу DevTools, «Fetch.requestPaused»).
/// Что проверяется: ключ 0.0.158 окно стирает, а рамке не отдаёт, и ключ
/// вписывают в самой рамке; ответ встаёт в рамке со ссылкой цветом программы; запрос несёт
/// ключ, версию API и вопрос; CSP рамки не пускает другой путь поставщика
/// и чужой адрес, а CSP окна — и самого поставщика: дверь из окна закрыта;
/// «Убрать ключ» стирает ключ в рамке, и строка налога это знает; рамка,
/// открытая сама по себе, говорит, где помощник.
const AI_ANSWER = "Срок — до 20 января. Подробнее: https://www.arlis.am/hy/acts/230455";
const DIST_AI = path.resolve(DIST, "../dist-ai");
const SITE = "https://kvartik.com";
const FRAME = "https://ai.kvartik.com";

/// Тело функции — в рамке помощника, в отдельном мире
/// (`Page.createIsolatedWorld`): DOM и хранилище рамки, а переменных её
/// скриптов там не видно. Ошибка — `{ error }`.
async function inFrame(page, code) {
  try {
    const { frameTree } = await page.send("Page.getFrameTree");
    const frame = (frameTree.childFrames ?? []).find((child) => child.frame.url.startsWith(`${FRAME}/`));
    if (!frame) return { error: "нет рамки помощника" };
    const { executionContextId } = await page.send("Page.createIsolatedWorld", { frameId: frame.frame.id, worldName: "check" });
    const r = await page.send("Runtime.evaluate", {
      expression: `(async () => { ${code} })()`, contextId: executionContextId, awaitPromise: true, returnByValue: true,
    });
    if (r.exceptionDetails) return { error: r.exceptionDetails.exception?.description ?? r.exceptionDetails.text };
    return r.result.value;
  } catch (e) { return { error: String(e) }; }
}

async function inFrameWait(page, code, ms = 20000) {
  const end = Date.now() + ms;
  while (Date.now() < end) {
    const v = await inFrame(page, code);
    if (v && !v.error) return v;
    await sleep(200);
  }
  return null;
}

/// Ввести текст в поле рамки по подписи.
const FRAME_TYPE = (label, text) => `const el = [...document.querySelectorAll("label")]
  .find((l) => (l.firstChild?.textContent ?? "").trim() === ${JSON.stringify(label)})?.querySelector("input, textarea");
  if (!el) return null;
  el.value = ${JSON.stringify(text)};
  el.dispatchEvent(new Event("input", { bubbles: true }));
  return true;`;

async function assistant(browser) {
  if (!fs.existsSync(path.join(DIST_AI, "index.html"))) {
    checks.check("ИИ-помощник: сайт рамки собран", false, `${DIST_AI} — нет index.html`);
    return;
  }
  const frameServer = own(DIST_AI, PORT + 9, { extra: {} });
  // Своя вкладка в своём контексте: хранилище `kvartik.com` — с чистого
  // листа, а первая вкладка браузера остаётся следующим шагам.
  const page = await browser.page({ context: true });
  const misses = [];
  const sse = [
    `event: message_start\ndata: {"type":"message_start","message":{}}\n\n`,
    `event: content_block_delta\ndata: ${JSON.stringify({ type: "content_block_delta", index: 0, delta: { type: "text_delta", text: AI_ANSWER } })}\n\n`,
    `event: message_delta\ndata: {"type":"message_delta","delta":{"stop_reason":"end_turn"}}\n\n`,
    `event: message_stop\ndata: {"type":"message_stop"}\n\n`,
  ].join("");
  const seen = [];
  const cors = [
    { name: "Access-Control-Allow-Origin", value: "*" },
    { name: "Access-Control-Allow-Headers", value: "*, authorization, x-api-key" },
    { name: "Access-Control-Allow-Methods", value: "POST" },
  ];
  // Своё — со своих серверов проверки, с их заголовками: тело — уже
  // разжатое, поэтому без `content-encoding` и длины.
  const proxied = async (requestId, request, base) => {
    const url = new URL(request.url);
    const response = await fetch(`${base}${url.pathname}${url.search}`, { method: request.method, redirect: "manual" });
    if (response.status !== 200) misses.push(`${response.status} ${request.url}`);
    const body = Buffer.from(await response.arrayBuffer());
    const responseHeaders = [...response.headers]
      .filter(([name]) => !["content-encoding", "content-length", "transfer-encoding", "connection"].includes(name))
      .map(([name, value]) => ({ name, value }));
    return page.send("Fetch.fulfillRequest", {
      requestId, responseCode: response.status, responseHeaders, body: body.toString("base64"),
    });
  };
  page.on((msg) => {
    if (msg.method !== "Fetch.requestPaused") return;
    const { requestId, request } = msg.params;
    const origin = new URL(request.url).origin;
    let answering;
    if (origin === SITE) answering = proxied(requestId, request, ORIGIN);
    else if (origin === FRAME) answering = proxied(requestId, request, `http://127.0.0.1:${PORT + 9}`);
    else {
      seen.push({ url: request.url, method: request.method, headers: request.headers, body: request.postData ?? "" });
      answering = page.send("Fetch.fulfillRequest", request.method === "OPTIONS"
        ? { requestId, responseCode: 204, responseHeaders: cors }
        : { requestId, responseCode: 200, responseHeaders: [...cors, { name: "Content-Type", value: "text/event-stream" }],
          body: Buffer.from(sse).toString("base64") });
    }
    answering.catch(() => page.send("Fetch.failRequest", { requestId, errorReason: "Failed" }).catch(() => {}));
  });
  await page.send("Fetch.enable", { patterns: [SITE, FRAME, "https://api.anthropic.com", "https://api.openai.com",
    "https://generativelanguage.googleapis.com", "https://api.x.ai"].map((origin) => ({ urlPattern: `${origin}/*`, requestStage: "Request" })) });
  await page.send("Page.addScriptToEvaluateOnNewDocument", { source: "delete Navigator.prototype.serviceWorker;" });
  try {
    await page.emulate(360);
    // Ключ, подключённый в 0.0.158 ещё в окне, и язык окна — записи
    // устройства `kvartik.com`.
    await page.go(`${SITE}/app/`);
    await page.eval(`localStorage.setItem("kvartik.lang", ${JSON.stringify(opt.lang)});
      localStorage.setItem("kvartik.ai", JSON.stringify({ provider: "claude", key: "sk-ant-old-0000", model: "claude-opus-5-5" }));
      return true;`);
    // С `/app/` на `/app/#/ask` — переход внутри документа, без запуска:
    // окно не прочло бы записанное. Поэтому — через пустую страницу.
    await page.fresh(`${SITE}/app/#/ask`);
    checks.check("ИИ-помощник: рамка своего происхождения встала в окне",
      await page.waitFor(`const f = document.querySelector("#ask iframe.ai-frame.ready");
        return f && f.src.startsWith(${JSON.stringify(`${FRAME}/?lang=${opt.lang}`)}) && f.getBoundingClientRect().height > 100 ? true : null;`));
    checks.check("ИИ-помощник: рамка — на языке окна", await inFrameWait(page,
      `return document.documentElement.lang === ${JSON.stringify(opt.lang === "hy" ? "hy" : opt.lang)}
        && document.body.innerText.includes(${JSON.stringify(s("AiAbout"))}) ? true : null;`));
    // Ключ у рамки — только вписанный в ней (с 0.0.161).
    checks.check("ИИ-помощник: ключ 0.0.158 окно стёрло, а рамке не отдало",
      await page.waitFor(`return localStorage.getItem("kvartik.ai") === null ? true : null;`)
      && await inFrameWait(page, `return document.querySelector(".ai-setup input")
        && localStorage.getItem("kvartik.ai") === null ? true : null;`));
    const connected = t("AiConnected", opt.lang).replace("{}", "Claude").replace("{}", "claude-opus-5-5").replace("{}", "abcd");
    checks.check("ИИ-помощник: ключ вписан и сохранён в рамке, а строка налога это знает",
      await inFrame(page, FRAME_TYPE(s("AiKey"), "sk-ant-check-0123abcd")) === true
      && await inFrameWait(page, `const b = [...document.querySelectorAll("button")]
        .find((b) => b.textContent.trim() === ${JSON.stringify(s("AiSave"))});
        if (!b || b.disabled) return null; b.click(); return true;`)
      && await inFrameWait(page, `return document.body.innerText.includes(${JSON.stringify(connected)})
        && JSON.parse(localStorage.getItem("kvartik.ai") ?? "{}").key === "sk-ant-check-0123abcd" ? true : null;`)
      && await page.waitFor(`return localStorage.getItem("kvartik.ai_by") === "claude" ? true : null;`));
    checks.check("ИИ-помощник: вопрос вписан в рамке", await inFrame(page, FRAME_TYPE(s("AiQuestion"), "Когда срок расчёта за IV квартал?")) === true);
    // Кнопку включает привязка поля своей задачей — ждать её, а не паузу.
    checks.check("ИИ-помощник: «Спросить»", await inFrameWait(page, `const b = [...document.querySelectorAll("button")]
      .find((b) => b.textContent.trim() === ${JSON.stringify(s("AiAsk"))});
      if (!b || b.disabled) return null; b.click(); return true;`));
    checks.check("ИИ-помощник: ответ встал в рамке, адрес закона — ссылкой", await inFrameWait(page,
      `return document.body.innerText.includes(${JSON.stringify(AI_ANSWER)})
        && document.querySelector('.ai-a a[href="https://www.arlis.am/hy/acts/230455"][target="_blank"]') ? true : null;`));
    // Ссылка в ответе — цветом ссылок программы, а не синим браузера.
    const tint = await inFrame(page, `const a = document.querySelector(".ai-a a");
      const probe = document.createElement("span");
      probe.style.color = getComputedStyle(document.documentElement).getPropertyValue("--accent-text").trim();
      document.body.append(probe);
      const want = getComputedStyle(probe).color;
      probe.remove();
      return { want, got: a ? getComputedStyle(a).color : null };`);
    checks.check("ИИ-помощник: ссылка в ответе — цветом ссылок программы", tint?.got && tint.got === tint.want, JSON.stringify(tint));
    const posts = seen.filter((r) => r.method === "POST");
    const sent = posts[0];
    const header = (name) => Object.entries(sent?.headers ?? {}).find(([key]) => key.toLowerCase() === name)?.[1];
    checks.check("ИИ-помощник: один вопрос — на адрес Anthropic, с ключом, версией API и вопросом, без куки",
      posts.length === 1 && sent.url === "https://api.anthropic.com/v1/messages" && header("x-api-key") === "sk-ant-check-0123abcd"
        && header("anthropic-version") === "2026-10-21" && header("anthropic-dangerous-direct-browser-access") === "true"
        && !header("cookie") && !header("referer") && JSON.parse(sent.body).messages.at(-1).content === "Когда срок расчёта за IV квартал?",
      JSON.stringify(seen.map((r) => `${r.method} ${r.url}`)));
    // Мимо CSP не уйти: из рамки — на другой путь поставщика и чужой адрес,
    // из окна — и на сам адрес поставщика.
    const tryOut = (urls) => `const blocked = [];
      const listen = (event) => blocked.push(event.blockedURI);
      document.addEventListener("securitypolicyviolation", listen);
      const tries = [];
      for (const url of ${JSON.stringify(urls)}) {
        try { await fetch(url, { method: "POST", body: "x" }); tries.push("ушёл"); } catch { tries.push("не пущен"); }
      }
      await new Promise((r) => setTimeout(r, 200));
      document.removeEventListener("securitypolicyviolation", listen);
      return { tries, blocked };`;
    const fromFrame = await inFrame(page, tryOut(["https://api.anthropic.com/v1/files", "https://kvartik-check.invalid/collect"]));
    checks.check("ИИ-помощник: CSP рамки не пускает другой путь поставщика и чужой адрес",
      fromFrame?.tries?.every((r) => r === "не пущен") && fromFrame.blocked.some((u) => u.startsWith("https://api.anthropic.com/v1/files"))
        && fromFrame.blocked.some((u) => u.startsWith("https://kvartik-check.invalid")) && !seen.some((r) => r.url.includes("/v1/files")),
      JSON.stringify(fromFrame));
    const before = seen.length;
    const fromWindow = await page.eval(tryOut(["https://api.anthropic.com/v1/messages", "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions"]));
    checks.check("ИИ-помощник: CSP окна не пускает к поставщикам ИИ — дверь закрыта",
      fromWindow?.tries?.every((r) => r === "не пущен") && fromWindow.blocked.length === 2 && seen.length === before,
      JSON.stringify(fromWindow));
    // Отказы CSP, которые проверка вызвала сама, — не ошибки программы.
    for (let i = page.errors.length - 1; i >= 0; i--) {
      if (/v1\/files|kvartik-check\.invalid|api\.anthropic\.com|generativelanguage/.test(page.errors[i])) page.errors.splice(i, 1);
    }
    checks.check("ИИ-помощник: «Убрать ключ» стирает его в рамке, а строка налога это знает", await inFrameWait(page,
      `const b = [...document.querySelectorAll("button")].find((b) => b.textContent.trim() === ${JSON.stringify(s("AiRemove"))});
      if (!b) return null; b.click(); return true;`)
      && await inFrameWait(page, `return localStorage.getItem("kvartik.ai") === null && document.querySelector(".ai-setup input") ? true : null;`)
      && await page.waitFor(`return localStorage.getItem("kvartik.ai_by") === null ? true : null;`));
    checks.check("ИИ-помощник: свои файлы окна и рамки под подменой отданы", misses.length === 0, misses.join("; "));
    checks.check("ИИ-помощник: без ошибок в консоли", page.errors.length === 0, page.errors.join("; "));
    // Рамка сама по себе — не в окне программы — говорит, где помощник.
    await page.go(`${FRAME}/?lang=${opt.lang}`);
    checks.check("ИИ-помощник: рамка, открытая сама, говорит, где помощник",
      await page.waitFor(TEXT_IN("main", s("AiFrameAlone"))) && await page.eval(`return document.querySelector(".ai-setup") ? null : true;`));
  } finally {
    await page.send("Fetch.disable").catch(() => {});
    await page.close?.();
    frameServer.stop();
  }
}

/// Первый кадр после перезагрузки (задача 89) — в `DOMContentLoaded`, когда
/// wasm ещё не встал: фон `<html>`, сколько свойств окно уже поставило ему
/// (0 — ни одного), отметка ссылки на копию своих цветов и тема мета-тега.
/// Стили в `<head>` держат первую отрисовку, а модуль клея — и
/// `DOMContentLoaded`: фон здесь — тот, что в первом кадре. Скрипт ставит
/// протокол DevTools, и CSP страницы его не касается.
async function firstFrame(page) {
  const { identifier } = await page.send("Page.addScriptToEvaluateOnNewDocument", {
    source: `addEventListener("DOMContentLoaded", () => {
      const html = document.documentElement;
      window.firstFrame = { background: getComputedStyle(html).backgroundColor, inline: html.style.length,
        link: document.querySelector("link[data-look]")?.dataset.look ?? null,
        scheme: document.querySelector('meta[name="color-scheme"]')?.content ?? null };
    }, { once: true });`,
  });
  await page.reload();
  const got = await page.waitFor(`return window.firstFrame ?? null;`);
  await page.send("Page.removeScriptToEvaluateOnNewDocument", { identifier });
  return got;
}

/// Главный экран: кнопка «Установить» есть всегда, пока программа не
/// установлена (задача 55), — и без вопроса браузера: Chrome в CI может его
/// и не прислать, а Firefox и Safari не присылают никогда. Раздел
/// «Установка» — блоком с отступом, как карточки над ним, а не кнопкой
/// вплотную к соседнему блоку (найдено глазным прогоном задачи 20). Отступ
/// блоков — `--space-4`, 16 точек. Вне iPhone, iPad и Mac раздел — ниже
/// карточки налога (с 0.0.114): сверху он отодвигал её на телефоне. Кнопку
/// проверка не нажимает: пришёл настоящий вопрос — она открыла бы окно
/// установки Chrome.
async function home(page) {
  await go(page, "#/");
  // Первый кадр до запуска wasm (с 0.0.117): знак — в самой странице, а
  // запущенное окно его снимает, иначе он остался бы над программой.
  const boot = await page.eval(`const html = await (await fetch("/app/", { cache: "no-store" })).text();
    return { served: /class="?boot"?[\\s>]/.test(html), left: document.querySelector(".boot") !== null };`);
  checks.check("программа: первый кадр — в странице, окно его сняло",
    boot?.served === true && boot.left === false, JSON.stringify(boot));
  const place = await page.waitFor(`const button = ${BUTTON("#home", s("InstallButton"))};
    const card = [...document.querySelectorAll("#home .hero-card")].find((c) => c.getClientRects().length > 0);
    const section = button && button.closest("section.install");
    if (!section || !card) return null;
    let above = section.previousElementSibling;
    while (above && above.getClientRects().length === 0) above = above.previousElementSibling;
    if (!above) return null;
    const top = section.getBoundingClientRect().top;
    return { below: card.getBoundingClientRect().bottom <= top, gap: top - above.getBoundingClientRect().bottom };`);
  checks.check("главная: «Установить» без вопроса браузера — блоком с отступом под карточкой налога",
    place !== null && place.below && place.gap >= 12,
    place === null ? "нет раздела «Установка», карточки налога или блока над разделом"
      : `${place.below ? "ниже" : "выше"} карточки налога, зазор ${place.gap} точек`);
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
/// Файл, который окно сделало ссылкой `blob:`, — самим `Blob`: `fetch` по
/// такому адресу CSP сайта не пускает (`connect-src` — свой сайт и адреса
/// ИИ-помощника), а свой сервер проверки с задачи 102 ставит CSP, как сайт.
/// Подмена `URL.createObjectURL` запоминает каждый `Blob` по его адресу —
/// ставится до нажатия, в той же странице; окно зовёт `URL.createObjectURL`
/// в самом нажатии.
const KEEP_BLOBS = `if (!window.__kvartikBlobs) {
    window.__kvartikBlobs = new Map();
    const made = URL.createObjectURL.bind(URL);
    URL.createObjectURL = (blob) => { const url = made(blob); window.__kvartikBlobs.set(url, blob); return url; };
  }`;
const BLOB_BYTES = (href) => `new Uint8Array(await window.__kvartikBlobs.get(${href}).arrayBuffer())`;

async function exported(page, when) {
  await go(page, "#/ledger");
  const shown = await page.waitFor(VISIBLE("#ledger section.export"));
  if (!checks.check(`${when}: выгрузка под книгой`, shown)) return;
  for (const [key, head, extension] of [["ExportXlsx", "504b0304", ".xlsx"], ["ExportCsvDocuments", "efbbbf", ".csv"]]) {
    const file = await page.eval(`${KEEP_BLOBS}
      const a = [...document.querySelectorAll("#ledger section.export a")]
        .find((a) => a.textContent.trim() === ${JSON.stringify(s(key))} && a.getClientRects().length > 0);
      if (!a) return { error: "нет ссылки" };
      a.click();
      if (!a.href.startsWith("blob:")) return { error: "адрес не blob: " + a.href };
      if (!window.__kvartikBlobs.has(a.href)) return { error: "Blob ссылки не пойман: " + a.href };
      const bytes = ${BLOB_BYTES("a.href")};
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
  const sheet = await page.eval(`${KEEP_BLOBS}
    const a = [...document.querySelectorAll("#ledger section.export a")]
      .find((a) => a.textContent.trim() === ${JSON.stringify(s("ExportXlsx"))} && a.getClientRects().length > 0);
    if (!a) return { error: "нет ссылки" };
    a.click();
    if (!window.__kvartikBlobs.has(a.href)) return { error: "Blob ссылки не пойман: " + a.href };
    const text = new TextDecoder().decode(${BLOB_BYTES("a.href")});
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

/// Ссылка словами — цветом ссылок программы, а не браузера (0.0.153): ссылка
/// на политику в согласии на сверку ИНН (задача 106, согласия больше нет)
/// вышла синей — у ссылки без своего класса цвет ставил браузер. Теперь на
/// том же месте — письмо о пробном ключе (задача 113): оно встаёт, когда
/// ключа нет и не было, а в письме — ИНН профиля, — поэтому в своём контексте
/// браузера: в основной вкладке потом восстанавливается образец копии другого
/// ИП. Проверяется ссылка «Вопросы — в Telegram» и каждая видимая ссылка
/// экрана лицензии.
async function trialKeyLinks(browser) {
  const page = await browser.page({ context: true });
  try {
    await page.emulate(360);
    await page.go(`${ORIGIN}/app/`);
    await page.setLang(opt.lang);
    checks.check("пробный ключ: рабочий экран", await page.waitFor(WORKING));
    await go(page, "#/profile");
    await page.waitFor(`return ${BUTTON("#profile", s("ProfileFill"))} ? true : null;`);
    await press(page, "#profile", s("ProfileFill"));
    const typed = await page.waitFor(`const field = (label) => [...document.querySelectorAll("#profile form.profile label")]
        .find((l) => l.getClientRects().length > 0 && (l.firstChild?.textContent ?? "").trim() === label)?.querySelector("input");
      const name = field(${JSON.stringify(s("ProfileNameRu"))});
      const tin = field(${JSON.stringify(s("ProfileTin"))});
      if (!name || !tin) return null;
      for (const [input, value] of [[name, "ИП Проба"], [tin, "12345678"]]) {
        input.value = value;
        input.dispatchEvent(new Event("input", { bubbles: true }));
      }
      return true;`);
    checks.check("пробный ключ: в профиле — имя и ИНН", typed && (await press(page, "#profile", s("ProfileSave")))
      && (await page.waitFor(TEXT_IN("#profile", s("ProfileSaved")))));
    await go(page, "#/license");
    const links = await page.waitFor(`const policy = document.querySelector("#license .trial-key a.link");
      const mail = document.querySelector("#license .trial-key a.button:not(.link)");
      if (!policy || !mail) return null;
      const hex = getComputedStyle(document.documentElement).getPropertyValue("--accent-text").trim();
      const want = "rgb(" + [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16)).join(", ") + ")";
      const seen = [...document.querySelectorAll("#license a")].filter((a) => a.getClientRects().length > 0);
      return { want, policy: getComputedStyle(policy).color, mail: mail.getAttribute("href"),
        browser: seen.filter((a) => ["rgb(0, 0, 238)", "rgb(85, 26, 139)"].includes(getComputedStyle(a).color))
          .map((a) => a.textContent.trim()) };`);
    checks.check("пробный ключ: ссылка на Telegram — цветом ссылок программы, а не браузера",
      links && links.policy === links.want, JSON.stringify(links));
    checks.check("пробный ключ: письмо продавцу с ИНН профиля",
      links && links.mail.startsWith("mailto:") && links.mail.includes("12345678"), links?.mail);
    checks.check("пробный ключ: на экране лицензии нет ссылок цвета браузера",
      links && links.browser.length === 0, JSON.stringify(links?.browser));
    checks.check("пробный ключ: без плашки сбоя", await page.eval(NO_PANIC));
  } finally {
    await page.close();
  }
}

/// Подписка на общий календарь (задача 75): на Windows кнопки `webcal:` нет
/// ни у лендинга, ни у экрана сроков — её открыл бы Outlook, а он, по
/// жалобам людей, берёт её по `http`, — а адрес `https:` и «Скопировать
/// адрес» на месте; на Mac кнопка остаётся. Строка браузера — своя у каждого
/// случая: проверка бежит и на Windows, и в CI на Linux.
const AGENTS = {
  Windows: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/158.0.0.0 Safari/537.36",
  Mac: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/158.0.0.0 Safari/537.36",
};

async function calendarScheme(browser) {
  for (const [system, userAgent] of Object.entries(AGENTS)) {
    const page = await browser.page({ context: true });
    const shown = system !== "Windows";
    try {
      await page.send("Emulation.setUserAgentOverride", { userAgent });
      await page.emulate(360);
      await page.go(`${ORIGIN}/ru/`);
      const site = await page.waitFor(`if (!(${SCRIPTED})) return null;
        const link = document.querySelector('a[href^="webcal:"]');
        const address = document.querySelector('section[aria-labelledby="calendar"] .address')?.textContent ?? "";
        return link && { seen: link.getClientRects().length > 0, address };`);
      checks.check(`календарь, ${system}: на лендинге кнопка webcal ${shown ? "есть" : "спрятана"}, адрес https на месте`,
        site && site.seen === shown && site.address.startsWith("https://"), JSON.stringify(site));
      await page.go(`${ORIGIN}/app/`);
      await page.setLang(opt.lang);
      checks.check(`календарь, ${system}: рабочий экран`, await page.waitFor(WORKING));
      await go(page, "#/deadlines");
      const app = await page.waitFor(`const section = document.querySelector("#deadlines section.calendar");
        const copy = section && [...section.querySelectorAll("button")]
          .find((b) => b.textContent.trim() === ${JSON.stringify(s("CalendarCopy"))});
        if (!copy || copy.getClientRects().length === 0) return null;
        return { webcal: section.querySelectorAll('a[href^="webcal:"]').length,
          address: section.querySelector(".calendar-address")?.textContent ?? "" };`);
      checks.check(`календарь, ${system}: на экране сроков кнопка webcal ${shown ? "есть" : "убрана"}, «Скопировать адрес» и адрес — на месте`,
        app && app.webcal === (shown ? 1 : 0) && app.address === `${ORIGIN}/calendar/${opt.lang}.ics`, JSON.stringify(app));
      checks.check(`календарь, ${system}: без плашки сбоя`, await page.eval(NO_PANIC));
    } finally {
      await page.close();
    }
  }
}

/// Лендинг (задача 34): страницы языков — готовым HTML с оформлением окна,
/// ссылка в программу и старые адреса программы `/#/…` (до 0.0.34 она жила
/// в корне) — в /app/ с тем же экраном. В своём контексте браузера: service
/// worker, которого поставит переход в программу, не должен править
/// вкладкой следующих шагов.
// Скрипт лендинга подключён с `defer`: пока он не выполнился, переход между
// страницами в программу никто не снимает, и Chrome отменяет его уже в ней —
// ошибкой в её консоли (`site/landing.js`), а поля калькулятора ещё не
// слушают ввода. Шаг, который нажимает на странице лендинга, ждёт, пока он
// выполнится: `complete` — после отложенных скриптов (по спецификации HTML).
// Без этого проверка однажды нажала ссылку в программу раньше и покраснела
// (CI, Chrome 157 без видеокарты, 2026-11-16).
const SCRIPTED = `document.readyState === "complete"`;

async function landing(browser) {
  const page = await browser.page({ context: true });
  // Отказы CSP — с первого байта страницы (задача 118): у страниц языков
  // блок данных JSON-LD в разметке, и CSP без `'unsafe-inline'` его не
  // касается, только пока это блок данных, а не скрипт. Скрипт ставит
  // протокол DevTools, и CSP страницы его не касается.
  await page.send("Page.addScriptToEvaluateOnNewDocument", {
    source: `window.cspRefused = [];
      document.addEventListener("securitypolicyviolation", (e) => cspRefused.push(e.violatedDirective + " " + e.blockedURI), true);`,
  });
  try {
    await page.emulate(360);
    for (const [lang, at] of [["hy", "/"], ["ru", "/ru/"], ["en", "/en/"]]) {
      await page.go(`${ORIGIN}${at}`);
      checks.check(`лендинг ${at}: страница своего языка`, await page.waitFor(`return document.documentElement.lang
        === ${JSON.stringify(lang)} && document.querySelector("h1")?.textContent
        === ${JSON.stringify(t("WelcomeHeadline", lang))} ? true : null;`));
      // Оформление пришло: заголовок — засечками окна, кнопка — пилюлей.
      checks.check(`лендинг ${at}: оформление и ссылка в программу`, await page.waitFor(`
        const button = document.querySelector('a.button[href="/app/?lang=${lang}"]');
        return button && getComputedStyle(document.querySelector("h1")).fontFamily.includes("Kvartik Serif")
          && getComputedStyle(button).borderTopLeftRadius !== "0px" ? true : null;`));
      // С `clientWidth`, а не `innerWidth`: у страницы шире экрана телефон
      // растит `innerWidth` вместе с ней, и прежняя проверка (с 0.0.82)
      // прокрутки вбок не видела (Chrome 157, 2026-11-09).
      checks.check(`лендинг ${at}: без прокрутки вбок на 360`, await page.eval(`const d = document.documentElement;
        return d.clientWidth === 360 && d.scrollWidth <= d.clientWidth ? true : null;`));
      checks.check(`лендинг ${at}: карточка цены ведёт на «Купить»`, await page.eval(`return document
        .querySelector('section.plan .price')?.textContent.replace(/\\D/g, "") === ${JSON.stringify(PRICE)}
        && document.querySelector('section.plan a.button[href="${buyPath(lang)}"]') ? true : null;`));
      // Описание программы для поисковиков (задача 118): блок JSON-LD
      // разбирается, цена ключа — та же, что на карточке, и ни одного
      // отказа CSP на странице.
      checks.check(`лендинг ${at}: описание для поисковиков с ценой ключа, отказов CSP нет`, await page.eval(`
        const blocks = document.querySelectorAll('script[type="application/ld+json"]');
        const app = blocks.length === 1 ? JSON.parse(blocks[0].textContent)["@graph"][0] : null;
        return app?.["@type"] === "SoftwareApplication" && app.offers[0].price === ${JSON.stringify(PRICE)}
          && app.url === "https://kvartik.com${at}" && window.cspRefused.length === 0
          ? true : { blocks: blocks.length, refused: window.cspRefused };`));
    }
    // Страница «Купить» (задачи 35 и 103): своего языка, с ценами
    // «Стандарта» и «Pro», счётом продавца и письмом со словами согласия, без
    // прокрутки вбок на 360.
    for (const lang of ["hy", "ru", "en"]) {
      const at = buyPath(lang);
      await page.go(`${ORIGIN}${at}`);
      checks.check(`«Купить» ${at}: свой язык, цены «Стандарта» и «Pro», счёт и письмо`, await page.waitFor(`return document.documentElement.lang
        === ${JSON.stringify(lang)} && document.querySelector("article.buy h1")?.textContent === ${JSON.stringify(t("BuyTitle", lang))}
        && [...document.querySelectorAll("article.buy .price")].map((p) => p.textContent.replace(/\\D/g, "")).join()
          === ${JSON.stringify(`${PRICE},${PRO_PRICE}`)}
        && [...document.querySelectorAll("article.buy .requisites .copy")].some((e) => e.textContent === ${JSON.stringify(ACCOUNT)})
        && decodeURIComponent(document.querySelector('article.buy a.button[href^="mailto:"]')?.href ?? "")
          .includes(${JSON.stringify(t("BuyAccept", lang))})
        && getComputedStyle(document.querySelector("h1")).fontFamily.includes("Kvartik Serif") ? true : null;`));
      checks.check(`«Купить» ${at}: без прокрутки вбок на 360`, await page.eval(`const d = document.documentElement;
        return d.clientWidth === 360 && d.scrollWidth <= d.clientWidth ? true : null;`));
      // «Вопросы» (задача 93) — пять, раскрываются без скрипта (`<details>`),
      // и раскрытые все разом не растягивают страницу вбок.
      checks.check(`«Купить» ${at}: пять вопросов раскрываются, и без прокрутки вбок`, await page.eval(`
        const all = [...document.querySelectorAll("article.buy .faq details")];
        if (all.length !== 5 || all.some((d) => d.open)) return null;
        all[0].querySelector("summary").click();
        if (!all[0].open) return null;
        for (const d of all) d.open = true;
        const e = document.documentElement;
        return e.scrollWidth <= e.clientWidth
          && document.querySelector("article.buy .faq")?.innerText.includes(${JSON.stringify(t("LicenseDevicesData", lang))}) ? true : null;`));
    }
    // «Отзывы и предложения» (задача 114): своего языка, с письмом продавцу,
    // в котором первая строка — версия программы и где её найти, и Telegram
    // продавца, без прокрутки вбок на 360.
    for (const lang of ["hy", "ru", "en"]) {
      const at = feedbackPath(lang);
      await page.go(`${ORIGIN}${at}`);
      checks.check(`отзывы ${at}: свой язык, письмо продавцу с версией и Telegram`, await page.waitFor(`return document.documentElement.lang
        === ${JSON.stringify(lang)} && document.querySelector("article.feedback h1")?.textContent === ${JSON.stringify(t("FeedbackTitle", lang))}
        && decodeURIComponent(document.querySelector('article.feedback a.button[href^="mailto:${EMAIL}?subject="]')?.href ?? "")
          .includes(${JSON.stringify(t("FeedbackMailSubject", lang))})
        && decodeURIComponent(document.querySelector('article.feedback a.button[href^="mailto:"]')?.href ?? "")
          .includes(${JSON.stringify(t("AboutTitle", lang))})
        && document.querySelector('article.feedback a.button[href="${TELEGRAM}"]')
        && document.querySelector('article.feedback .hint a[href="mailto:${EMAIL}"]')
        && getComputedStyle(document.querySelector("h1")).fontFamily.includes("Kvartik Serif") ? true : null;`));
      checks.check(`отзывы ${at}: без прокрутки вбок на 360`, await page.eval(`const d = document.documentElement;
        return d.clientWidth === 360 && d.scrollWidth <= d.clientWidth ? true : null;`));
    }
    // Юридические тексты (задача 32): у каждого языка — свои условия
    // и политика, страницами из утверждённых файлов docs/legal/*.md, с тем же
    // оформлением, а подвал страницы языка ведёт к ним.
    for (const doc of ["terms", "privacy"]) {
      for (const [lang, home] of [["hy", "/"], ["ru", "/ru/"], ["en", "/en/"]]) {
        const at = `${home}${doc}/`;
        const source = fs.readFileSync(path.join(REPO, "docs/legal", `${doc}.${lang}.md`), "utf8");
        const title = source.split(/\r?\n/)[0].replace(/^# /, "");
        await page.go(`${ORIGIN}${at}`);
        checks.check(`текст ${at}: свой язык и название из файла`, await page.waitFor(`return document.documentElement.lang
          === ${JSON.stringify(lang)} && document.querySelector("article.legal h1")?.textContent === ${JSON.stringify(title)}
          && document.querySelectorAll("article.legal h2").length >= 5
          && getComputedStyle(document.querySelector("h1")).fontFamily.includes("Kvartik Serif") ? true : null;`));
        // Как у страниц языков: с `clientWidth` (418 против 360 у политики,
        // которой это не было видно, — глазной прогон задачи 32).
        checks.check(`текст ${at}: без прокрутки вбок на 360`, await page.eval(`const d = document.documentElement;
          return d.clientWidth === 360 && d.scrollWidth <= d.clientWidth ? true : null;`));
      }
    }
    // Прежние редакции (задача 93; условия, п. 11) — по своему адресу на
    // каждом языке, текстом из своего файла, со словами, что она прежняя,
    // и ссылкой на действующую, а поисковикам — `noindex`; действующая ведёт
    // на каждую. Какие есть — по файлам docs/legal: у условий одна точно есть.
    const editions = fs.readdirSync(path.join(REPO, "docs/legal"))
      .map((name) => name.match(/^(terms|privacy)\.(\d{4}-\d{2}-\d{2})\.(hy|ru|en)\.md$/)).filter(Boolean);
    checks.check("прежние редакции: файлы трёх языков, у условий — есть",
      editions.some((m) => m[1] === "terms") && editions.length % 3 === 0);
    for (const [name, doc, day, lang] of editions) {
      const home = lang === "hy" ? "/" : `/${lang}/`;
      const at = `${home}${doc}/${day}/`;
      const title = fs.readFileSync(path.join(REPO, "docs/legal", name), "utf8").split(/\r?\n/)[0].replace(/^# /, "");
      await page.go(`${ORIGIN}${at}`);
      checks.check(`прежняя редакция ${at}: свой язык, название, «прежняя» и ссылка на действующую`, await page.waitFor(`return document
        .documentElement.lang === ${JSON.stringify(lang)} && document.querySelector("article.legal h1")?.textContent === ${JSON.stringify(title)}
        && document.querySelector('article.legal .edition-note a[href="${home}${doc}/"]')
        && document.querySelector('meta[name="robots"][content="noindex"]') ? true : null;`));
      checks.check(`прежняя редакция ${at}: без прокрутки вбок на 360`, await page.eval(`const d = document.documentElement;
        return d.clientWidth === 360 && d.scrollWidth <= d.clientWidth ? true : null;`));
      await page.go(`${ORIGIN}${home}${doc}/`);
      checks.check(`текст ${home}${doc}/: ведёт на прежнюю редакцию ${day}`, await page.waitFor(`return document
        .querySelector('article.legal .editions a[href="${at}"]') && !document.querySelector('meta[name="robots"]') ? true : null;`));
    }
    await page.go(`${ORIGIN}/ru/`);
    await page.waitFor(`return document.querySelector('footer a[href="/ru/terms/"]') && ${SCRIPTED} ? true : null;`);
    await page.eval(`document.querySelector('footer a[href="/ru/terms/"]').click(); return true;`);
    checks.check("лендинг: подвал ведёт к условиям своего языка", await page.waitFor(`return location.pathname
      === "/ru/terms/" && document.querySelector("article.legal") && ${SCRIPTED} ? true : null;`));
    // Условия зовут страницу «Купить» по имени (п. 2), и с их страницы до
    // неё — ссылка подвала.
    await page.eval(`document.querySelector('footer a[href="/ru/buy/"]').click(); return true;`);
    checks.check("условия: подвал ведёт на «Купить» своего языка", await page.waitFor(`return location.pathname
      === "/ru/buy/" && document.querySelector("article.buy") ? true : null;`));
    // И к отзывам (задача 114) — ссылкой подвала каждой страницы.
    await page.eval(`document.querySelector('footer a[href="/ru/feedback/"]').click(); return true;`);
    checks.check("«Купить»: подвал ведёт на отзывы своего языка", await page.waitFor(`return location.pathname
      === "/ru/feedback/" && document.querySelector("article.feedback") ? true : null;`));
    // Калькулятор (задача 34, этап 2): wasm ядра грузится при первом касании
    // формы и отвечает словами страницы. IT 1 000 000 ֏ — 1 % (НК, ст. 258,
    // ч. 1, строка 8): 10 000 ֏; поля, нужные не всегда, не видны, пока ответ
    // их не попросит, — расходы встают у строки с вычетом.
    const field = (name, value) => `const f = document.querySelector('section[data-module] [name="${name}"]');
      f.focus(); f.value = ${JSON.stringify(value)};
      f.dispatchEvent(new Event(f.tagName === "SELECT" ? "change" : "input", { bubbles: true })); return true;`;
    const shown = (name) => `return document.querySelector('[data-field="${name}"]').getClientRects().length > 0;`;
    await page.go(`${ORIGIN}/ru/`);
    await page.waitFor(`return document.querySelector("section[data-module]") && ${SCRIPTED} ? true : null;`);
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
    // Язык страницы лендинга (/ru/) приходит в программу строкой запроса
    // (полный прогон 2026-11-10, п. 11): экран первого запуска — по-русски,
    // а запомнит язык человек — «Продолжить», не раньше.
    checks.check("программа: первый запуск на языке страницы лендинга, ещё не запомненном", await page.waitFor(`
      return location.search === "?lang=ru" && document.documentElement.lang === "ru"
        && localStorage.getItem("kvartik.lang") === null
        && [...document.querySelectorAll(".welcome button")].some((b) => b.textContent.trim()
          === ${JSON.stringify(t("WelcomeContinue", "ru"))}) ? true : null;`));
    const proceed = JSON.stringify(["hy", "ru", "en"].map((lang) => t("WelcomeContinue", lang)));
    await page.waitFor(`const b = [...document.querySelectorAll(".welcome button")].find((b) =>
      ${proceed}.includes(b.textContent.trim())); if (!b) return null; b.click(); return true;`);
    checks.check("программа: «Продолжить» запомнило язык страницы лендинга",
      await page.waitFor(`return localStorage.getItem("kvartik.lang") === "ru" ? true : null;`));
    checks.check("программа: предлагает взять введённое в калькуляторе", await page.waitFor(`
      const card = document.querySelector("#home .handoff");
      return card && card.getClientRects().length > 0 && card.innerText.includes("1985") ? true : null;`));
    // Язык, выбранный переключателем страниц сайта, берёт и программа
    // (0.0.125): она уже помнит русский, а на лендинге выбрали English —
    // «Open Kvartik» открывает её по-английски, и выбор запомнен. Язык
    // страницы без переключателя выбранного не меняет: с армянского корня
    // программа — по-прежнему на English.
    await page.go(`${ORIGIN}/ru/`);
    await page.waitFor(`return document.querySelector('.langs a[hreflang="en"]') && ${SCRIPTED} ? true : null;`);
    await page.eval(`document.querySelector('.langs a[hreflang="en"]').click(); return true;`);
    await page.waitFor(`return location.pathname === "/en/" && document.querySelector('a.button[href="/app/?lang=en"]')
      && ${SCRIPTED} ? true : null;`);
    await page.eval(`document.querySelector('a.button[href="/app/?lang=en"]').click(); return true;`);
    checks.check("программа: язык, выбранный на лендинге, сильнее выбранного в ней раньше", await page.waitFor(`
      return location.pathname === "/app/" && document.querySelector(".app") && document.documentElement.lang === "en"
        && localStorage.getItem("kvartik.lang") === "en" && localStorage.getItem("kvartik.site_lang") === null ? true : null;`));
    await page.go(`${ORIGIN}/`);
    await page.waitFor(`return document.querySelector('a.button[href="/app/?lang=hy"]') && ${SCRIPTED} ? true : null;`);
    await page.eval(`document.querySelector('a.button[href="/app/?lang=hy"]').click(); return true;`);
    const opened = await page.waitFor(`return location.pathname === "/app/" && document.querySelector(".app")
      ? document.documentElement.lang : null;`);
    checks.check("программа: язык страницы лендинга без переключателя выбранного в ней не меняет", opened === "en",
      `окно на ${opened}`);
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
/// браузер ищет цель нажатия), а подсветка открытой вкладки едет к новой
/// сдвигом, не снимком. Под мышью кнопка получает точку входа,
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
    // Кружок у курсора — только когда его включили в настройках (задача 88):
    // по умолчанию у страницы нет метки `data-cursor`, и стили его не рисуют
    // и с мышью. Дальше он включён — запись устройства, как её пишет
    // переключатель, — и у лендинга тоже.
    checks.check("движение: кружка у курсора нет, пока его не включили", await page.eval(`
      return !document.documentElement.hasAttribute("data-cursor")
        && getComputedStyle(document.querySelector(".cursor")).display === "none" ? true : null;`));
    await page.eval(`localStorage.setItem("kvartik.cursor", "1"); return true;`);
    await page.reload();
    checks.check("движение: рабочий экран с кружком у курсора", (await page.waitFor(WORKING))
      && (await page.waitFor(`return document.documentElement.hasAttribute("data-cursor") ? true : null;`)));
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

    // Подсветка открытой вкладки едет к новой сама, сдвигом под подписями,
    // а снимает переход только прежний экран (и документ): снятая вкладка
    // переезжала вместе с подписью, и посреди пути две подписи сливались
    // в одну — «Счетая» (2026-11-08). Смотреть — в первых кадрах перехода:
    // сдвиг идёт полсекунды.
    const pill = await page.eval(`location.hash = "#/ledger";
      const moving = () => document.getAnimations().some((a) => String(a.effect?.pseudoElement ?? "").startsWith("::view-transition"));
      for (let i = 0; i < 300 && !moving(); i++) await new Promise((r) => requestAnimationFrame(r));
      const all = document.getAnimations();
      return {
        moving: moving(),
        pill: all.some((a) => a instanceof CSSTransition && a.transitionProperty === "translate"
          && a.effect?.pseudoElement === "::before" && a.effect.target.matches(".tabs > li:first-child")),
        snapped: [...new Set(all.map((a) => String(a.effect?.pseudoElement ?? ""))
          .filter((p) => p.startsWith("::view-transition") && !/\\((screen|doc)\\)$/.test(p)))],
      };`);
    checks.check("движение: подсветка вкладки едет сдвигом, вкладка не снимается",
      pill?.moving && pill.pill && pill.snapped?.length === 0, JSON.stringify(pill));
    await sleep(700);

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
    // Выключили в программе — у лендинга кружка нет.
    await page.eval(`localStorage.removeItem("kvartik.cursor"); return true;`);
    await page.reload();
    checks.check("движение: у лендинга кружка нет, пока его не включили в программе", await page.waitFor(`
      return document.querySelector(".hero") && !document.querySelector(".cursor") ? true : null;`));
    checks.check("движение: без плашки сбоя", await page.eval(NO_PANIC));
    const errors = [...new Set(page.errors)];
    checks.check("движение: без ошибок в консоли", errors.length === 0, errors.slice(0, 8).join(" | "));
  } finally {
    await page.close();
  }
}

/// Курс ЦБ (задача 43): программа берёт файл курсов со своего сайта
/// и помнит его на устройстве, а форма дохода в долларах с днём услуги
/// в понедельник 16 ноября 2026 года подсказывает курс, опубликованный
/// в пятницу 13-го, — 360,54 ֏ из образца выкладки; «Подставить» пишет его
/// в поле. Браузерные тесты кладут курсы сами и запроса к сайту не видят.
async function rates(page) {
  await go(page, "#/ledger");
  await page.waitFor(`return ${BUTTON("#ledger", s("LedgerAddIncome"))} ? true : null;`);
  await press(page, "#ledger", s("LedgerAddIncome"));
  await page.waitFor(TEXT_IN("#ledger", s("OpNewIncome")));
  const set = await page.eval(`
    const label = (text) => [...document.querySelectorAll("#ledger label")]
      .find((l) => l.firstChild?.textContent?.trim() === text && l.getClientRects().length > 0);
    const currency = label(${JSON.stringify(s("DocCurrency"))})?.querySelector("select");
    const day = label(${JSON.stringify(s("DocServiceDate"))})?.querySelector("input");
    if (!currency || !day) return null;
    currency.value = "USD"; currency.dispatchEvent(new Event("change", { bubbles: true }));
    day.value = "2026-11-16"; day.dispatchEvent(new Event("input", { bubbles: true }));
    return true;`);
  checks.check("курс ЦБ: форма дохода в долларах", set === true);
  const [date, rate] = opt.lang === "en" ? ["Nov 13, 2026", "360.54"] : ["13.11.2026", "360,54"];
  checks.check("курс ЦБ: подсказка — курс, опубликованный 13 ноября, с сайта",
    await page.waitFor(`const text = document.querySelector("#ledger")?.innerText ?? "";
      return text.includes(${JSON.stringify(date)}) && text.includes(${JSON.stringify(rate)}) ? true : null;`));
  await press(page, "#ledger", s("CbaRateUse"));
  checks.check("курс ЦБ: «Подставить» пишет курс в поле",
    await page.waitFor(`return [...document.querySelectorAll("#ledger label input")]
      .some((input) => input.value === ${JSON.stringify(rate)}) ? true : null;`));
  checks.check("курс ЦБ: файл курсов запомнен на устройстве",
    (await page.eval(`return localStorage.getItem("kvartik.rates");`)) === RATES_FILE);
  await press(page, "#ledger", s("Cancel"));
  checks.check("курс ЦБ: форма закрылась", await page.waitFor(NO_TEXT_IN("#ledger", s("OpNewIncome"))));
}

/// Восстановить последний образец копии: в нём есть выпущенные документы.
/// Пароль образца станет паролем копий устройства.
async function restore(page) {
  const sample = samples().at(-1);
  await go(page, "#/backup");
  checks.check("копия: экран копии нарисован", await page.waitFor(`return document.querySelector(${JSON.stringify(BACKUP_FILE)}) ? true : null;`));
  await page.setFiles(BACKUP_FILE, [sample.file]);
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
  await page.waitFor(`return ${BUTTON("#backup", s("BackupMake"))} ? true : null;`);
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
/// кэш — прежний. Кэш копии своих цветов (`kvartik.look`, задача 89) — не
/// сборки, и новая версия его не трогает.
async function switched(page, next, running, when) {
  const name = `kvartik-${next.build}`;
  const cache = await page.waitFor(`const n = (await caches.keys()).filter((c) => c.startsWith("kvartik-"));
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
  let srv = own(next.dir, PORT, SERVED);
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
    // Страница открывается прежней сборкой — её отдаёт прежний service
    // worker, — а новую окно включает уже после захода и перезагружает
    // страницу (`location.reload()`, `platform/pwa.rs`). Прежний кэш новый
    // удаляет при активации, раньше перезагрузки: ждать надо перезагрузки,
    // а не нового кэша, иначе «Понятно» нажимается в окне страницы, которая
    // уже уходит (CI, 2026-11-09: окно встало снова через 0,2 с).
    await srv.stop();
    srv = own(later.dir, PORT, SERVED);
    await page.eval(SEEN("0.0.1"));
    await page.fresh(`${ORIGIN}/app/#/`);
    checks.check("новая версия: зашли на страницу — страница перезагрузилась сама",
      await page.waitFor(`return performance.getEntriesByType("navigation")[0]?.type === "reload"
        && document.readyState === "complete" ? true : null;`, 30000));
    await switched(page, later, running, "зашли на страницу");

    // Попытка включить эту версию уже была: она ждёт, а страница стоит.
    await srv.stop();
    srv = own(refused.dir, PORT, SERVED);
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

// Функция сайта `_worker.js` сборки — здесь же, в Node. Сервера пробного
// периода больше нет (задача 113): функцию зовут только адреса wasm
// программы (задача 109), а `/api/trial`, который ещё спросит прежняя версия
// из кэша, — не её: хостинг отдаёт его сам, отказом, и прежняя версия считает
// срок по устройству, как без сети. А дошёл до неё другой адрес — она отдаёт
// его файлом (`env.ASSETS`), а не своим 404: страница сайта не сломается.
// Копией `.mjs`: модуль ли файл `.js` без `package.json`, Node решает по своей
// версии.
async function siteFunction() {
  const copy = path.join(WORK, "site-worker.mjs");
  fs.copyFileSync(path.join(DIST, "_worker.js"), copy);
  const worker = (await import(pathToFileURL(copy).href)).default;
  checks.check("функция сайта: её зовут только адреса /app/*.wasm",
    fs.readFileSync(path.join(DIST, "_routes.json"), "utf8").trim()
      === `{"version":1,"include":["/app/*.wasm"],"exclude":[]}`);
  const origin = "https://kvartik.example";
  const assets = { fetch: async (request) => new Response(`файл ${new URL(request.url).pathname}`, { status: 404 }) };
  const page = await worker.fetch(new Request(`${origin}/app/`), { ASSETS: assets });
  checks.check("функция сайта: не wasm — файлом сайта", (await page.text()) === "файл /app/");
  const trial = await worker.fetch(new Request(`${origin}/api/trial`, {
    method: "POST", body: `{"tin":"07654321","consent":"2026-11-18"}`,
  }), { ASSETS: assets });
  const text = await trial.text();
  checks.check("функция сайта: вопрос о пробном периоде прежней версии — не её ответ, а отказ хостинга",
    trial.status === 404 && text === "файл /api/trial", `${trial.status} ${text}`);
}

// Заранее сжатый wasm (задача 109) — та же функция сайта в Node, с хранилищем
// файлов-подменой поверх `dist/`: на адрес wasm она отдаёт тело его копии
// уровнем 11 как есть — `Content-Encoding: br` и `encodeBody: "manual"`,
// иначе рантайм Cloudflare сжал бы его второй раз, — с заголовками копии,
// а без brotli, диапазоном, без копии и при сбое хранилища — сам wasm. Что
// Cloudflare доносит копию до браузера байт в байт, проверяет выкладка
// (`tools/deploy/check.sh`), а здесь — что копия есть у каждого wasm,
// разжимается в него и сжата уровнем 11, а не уровнем хостинга.
async function precompressedWasm() {
  const worker = (await import(pathToFileURL(path.join(WORK, "site-worker.mjs")).href)).default;
  const app = path.join(DIST, "app");
  const wasm = fs.readdirSync(app).filter((n) => n.endsWith(".wasm")).sort();
  const missing = wasm.filter((n) => !fs.existsSync(path.join(app, `${n}.br`)));
  checks.check("заранее сжатый wasm: у каждого wasm программы — копия .br", wasm.length > 0 && missing.length === 0,
    missing.join(", "));
  const wrong = wasm.filter((n) => !missing.includes(n)
    && !zlib.brotliDecompressSync(fs.readFileSync(path.join(app, `${n}.br`))).equals(fs.readFileSync(path.join(app, n))));
  checks.check("заранее сжатый wasm: копия разжимается в тот же wasm", wrong.length === 0, wrong.join(", "));
  const main = wasm.find((n) => /^kvartik-app-\w+_bg\.wasm$/.test(n));
  if (!main || missing.includes(main)) return;
  const raw = fs.readFileSync(path.join(app, main));
  const packed = fs.readFileSync(path.join(app, `${main}.br`));
  const level = (q) => zlib.brotliCompressSync(raw, { params: { [zlib.constants.BROTLI_PARAM_QUALITY]: q } }).length;
  // Кодер сборки — не `zlib`, байты у него свои: запас — 2 %.
  const [eleven, hosting] = [level(11), level(4)];
  checks.check("заранее сжатый wasm: главный — уровнем 11, а не уровнем хостинга",
    packed.length <= eleven * 1.02, `${packed.length} Б; zlib уровнем 11 — ${eleven}, уровнем хостинга — ${hosting}`);
  const rules = fs.readFileSync(path.join(DIST, "_headers"), "utf8").replaceAll("\r", "");
  checks.check("заранее сжатый wasm: у копии в _headers — год без сверки, как у wasm",
    /^\/app\/\*\.wasm\.br\n {2}Cache-Control: public, max-age=31536000, immutable$/m.test(rules));
  // Хранилище-подмена — как Pages: файл `dist/`, ETag по содержимому и 304 на
  // свой ETag, заголовки — от `_headers`.
  const state = { broken: false, encoded: false, asked: [] };
  const assets = {
    fetch: async (request) => {
      const file = new URL(request.url).pathname;
      state.asked.push(file);
      if (state.broken && file.endsWith(".br")) throw new Error("хранилище не ответило");
      const full = path.join(DIST, file);
      if (!fs.existsSync(full)) return new Response("нет", { status: 404 });
      const bytes = fs.readFileSync(full);
      const headers = {
        ETag: `"${crypto.createHash("sha1").update(bytes).digest("hex")}"`,
        "Content-Type": file.endsWith(".br") ? "application/octet-stream" : "application/wasm",
        "Cache-Control": "public, max-age=31536000, immutable",
        "X-Content-Type-Options": "nosniff",
        ...(state.encoded && file.endsWith(".br") ? { "Content-Encoding": "gzip" } : {}),
      };
      if (request.headers.get("If-None-Match") === headers.ETag) return new Response(null, { status: 304, headers });
      return new Response(request.method === "HEAD" ? null : bytes, { headers });
    },
  };
  const origin = "https://kvartik.example";
  const ask = async (file, headers = {}, method = "GET") => {
    state.asked.length = 0;
    const response = await worker.fetch(new Request(`${origin}/app/${file}`, { method, headers }), { ASSETS: assets });
    const body = method === "HEAD" ? null : Buffer.from(await response.arrayBuffer());
    return { response, body, init: response.init ?? {}, asked: [...state.asked] };
  };
  // `encodeBody` — свойство ответа Workers, Node его не знает: ответ функции
  // запоминает, с чем его создали.
  const Native = globalThis.Response;
  globalThis.Response = class extends Native {
    constructor(body, init) {
      super(body, init);
      this.init = init;
    }
  };
  try {
    const browser = { "Accept-Encoding": "gzip, deflate, br, zstd" };
    const first = await ask(main, browser);
    const h = first.response.headers;
    checks.check("заранее сжатый wasm: на адрес wasm — копия как есть, br, тип wasm, заголовки копии",
      first.response.status === 200 && first.body.equals(packed) && h.get("Content-Encoding") === "br"
        && first.init.encodeBody === "manual" && h.get("Content-Type") === "application/wasm"
        && h.get("Vary") === "Accept-Encoding" && h.get("Cache-Control") === "public, max-age=31536000, immutable"
        && h.get("X-Content-Type-Options") === "nosniff" && Boolean(h.get("ETag")),
      `${first.response.status} ${first.body.length} Б, ${JSON.stringify(Object.fromEntries(h))}, ${JSON.stringify(first.init.encodeBody)}`);
    const again = await ask(main, { ...browser, "If-None-Match": h.get("ETag") });
    checks.check("заранее сжатый wasm: сверка по ETag — 304, тело не качается второй раз",
      again.response.status === 304 && again.body.length === 0 && !again.response.headers.has("Content-Encoding"),
      `${again.response.status} ${again.body.length} Б`);
    const head = await ask(main, browser, "HEAD");
    checks.check("заранее сжатый wasm: HEAD — заголовки копии без тела",
      head.response.status === 200 && head.response.headers.get("Content-Encoding") === "br"
        && head.asked.join() === `/app/${main}.br`, `${head.response.status} ${head.asked.join()}`);
    const asIs = [
      ["без brotli", await ask(main, { "Accept-Encoding": "gzip, deflate" })],
      ["brotli с q=0", await ask(main, { "Accept-Encoding": "br;q=0, gzip" })],
      ["диапазон байтов", await ask(main, { ...browser, Range: "bytes=0-99" })],
      ["POST", await ask(main, browser, "POST")],
    ];
    state.broken = true;
    asIs.push(["хранилище не ответило о копии", await ask(main, browser)]);
    state.broken = false;
    state.encoded = true;
    asIs.push(["копию сжал хостинг", await ask(main, browser)]);
    state.encoded = false;
    const plain = asIs.filter(([, got]) => !(got.response.status === 200 && got.body.equals(raw)
      && !got.response.headers.has("Content-Encoding") && got.asked.at(-1) === `/app/${main}`))
      .map(([name, got]) => `${name}: ${got.response.status} ${got.body.length} Б, ${got.asked.join(" ")}`);
    checks.check("заранее сжатый wasm: когда копия не годится — сам wasm файлом", plain.length === 0, plain.join("; "));
    const nothing = await ask("kvartik-nothing-0000000000000000_bg.wasm", browser);
    checks.check("заранее сжатый wasm: нет ни копии, ни wasm — ответ хранилища (404)", nothing.response.status === 404,
      `${nothing.response.status}`);
  } finally {
    globalThis.Response = Native;
  }
}

/// IndexNow (задача 118): файл ключа лежит в корне сборки и отдаётся текстом
/// с самим ключом; сверка, которой выкладка ищет изменившиеся страницы,
/// у сайта этой же сборки не находит ни одной — иначе каждая выкладка
/// называла бы поисковикам все страницы, — а изменённую и новую находит;
/// запрос — все адреса одного сайта с ключом и его адресом на сайте.
async function indexNow() {
  const key = indexKey(DIST);
  const response = await fetch(`${ORIGIN}/${key}.txt`);
  const type = response.headers.get("content-type") ?? "";
  checks.check("IndexNow: файл ключа отдаётся текстом с ключом",
    response.status === 200 && (await response.text()) === key && type.startsWith("text/plain"), `${response.status} ${type}`);
  const all = sitemapPages(DIST);
  const same = await changedPages(DIST, ORIGIN);
  checks.check("IndexNow: у сайта этой же сборки изменившихся страниц нет", same.length === 0, same.join(" "));
  // Копия карты и страниц, где одна страница другая, а одной нет на сайте.
  const copy = fs.mkdtempSync(path.join(WORK, "indexnow-"));
  fs.copyFileSync(path.join(DIST, "sitemap.xml"), path.join(copy, "sitemap.xml"));
  for (const loc of all) {
    const rel = `${new URL(loc).pathname.slice(1)}index.html`;
    fs.mkdirSync(path.dirname(path.join(copy, rel)), { recursive: true });
    fs.copyFileSync(path.join(DIST, rel), path.join(copy, rel));
  }
  fs.appendFileSync(path.join(copy, "ru/index.html"), "<!-- другая -->");
  fs.appendFileSync(path.join(copy, "sitemap.xml"), "<url><loc>https://kvartik.com/no-such-page/</loc></url>");
  fs.mkdirSync(path.join(copy, "no-such-page"));
  fs.writeFileSync(path.join(copy, "no-such-page/index.html"), "<!doctype html>");
  const changed = await changedPages(copy, ORIGIN);
  checks.check("IndexNow: изменённая и новая страницы найдены, остальные — нет",
    changed.join(" ") === "https://kvartik.com/ru/ https://kvartik.com/no-such-page/", changed.join(" "));
  fs.rmSync(copy, { recursive: true, force: true });
  const body = indexNowRequest(DIST, all);
  checks.check("IndexNow: запрос — все страницы карты одного сайта с ключом и его адресом",
    body.host === "kvartik.com" && body.key === key && body.keyLocation === `https://kvartik.com/${key}.txt`
      && body.urlList.length === all.length && all.length >= 15, JSON.stringify(body).slice(0, 300));
}

async function main() {
  const parts = fs.readdirSync(path.join(DIST, "app")).filter((n) => /^split_\w+-\w+\.wasm$/.test(n));
  checks.check("в сборке есть части wasm", parts.length > 0, parts.join(", "));
  await siteFunction();
  await precompressedWasm();
  // Программа выкладки пишет из ответа ЦБ тот самый файл, который читают
  // тест ядра и программа: разойдись они — курсы на сайте окно отвергло бы.
  checks.check("курсы ЦБ: rates.mjs пишет из образца ответа ЦБ файл, который читает тест ядра",
    renderRates(parseCba(fs.readFileSync(path.join(RATES_DIR, "cba.xml"), "utf8"))) === RATES_FILE);
  checks.check("курсы ЦБ: в сборке файла курсов нет — его кладёт выкладка",
    !fs.existsSync(path.join(DIST, "app/rates.json")));
  let srv = own(DIST, PORT, SERVED);
  const browser = await Browser.launch({ work: WORK, name: "check", port: 9340 });
  try {
    await blocked(browser);
    await trialKeyLinks(browser);
    await landing(browser);
    await indexNow();
    await calendarScheme(browser);
    await office(browser);
    await motion(browser);
    await assistant(browser);

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
    await rates(page);
    await go(page, "#/");
    checks.check("первые шаги: у копии с документами карточки нет", await page.waitFor(HIDDEN("#home .first-steps")));
    await forms(page, "с сетью");
    await screens(page, "с сетью");
    await settings(page);
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
