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
//    нажатию — частями wasm»).
// 2. С сетью: восстановление образца копии, экраны и их формы, карточка
//    выпущенного документа, PDF, новая копия — каждая из девяти частей;
//    и все части `dist/app/split_*.wasm` загружены.
// 3. Сервер выключен: программа открывается из кэша service worker, и те же
//    экраны, формы, карточка и PDF работают без сети.
// 4. Новая версия (задача 67): на сервере — копия сборки новой версией. Пока
//    открыта форма, над экраном только слова; форму закрыли — окно «Что
//    нового» с заметкой новой версии поверх неактивной программы, а
//    «Обновить» открывает новую сборку. Окно обязательное: сломанное, оно
//    заперло бы программу у всех, кто получит следующую версию.
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
//   частей (`wasm_split_cli_support` 0.2.3) оставляет на заблокированной
//   части необработанный отказ `Failed to fetch` — это его код, человеку он
//   не мешает, и провалом здесь не считается.
// - «Без сети» — выключенный сервер, а не эмуляция (Правило 2).

import { parseArgs } from "node:util";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { Browser, Checks, sleep } from "./cdp.mjs";
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

/// Часть не загрузилась — слова и «Отмена»; снятая блокировка — экран.
async function blocked(browser) {
  const page = await browser.page({ context: true });
  try {
    await page.send("Network.enable");
    await page.send("Network.setBypassServiceWorker", { bypass: true });
    await page.send("Network.setBlockedURLs", { urls: ["*split_ledger_screen-*"] });
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
    const errors = [...new Set(page.errors)].filter((e) => !/split_ledger_screen|Failed to fetch/.test(e));
    checks.check("блокировка: других ошибок в консоли нет", errors.length === 0, errors.slice(0, 8).join(" | "));
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

/// Новая версия — на той же странице: сервер отдаёт копию сборки новой
/// версией (`newVersion`), а проверку новой версии окно делает и само, при
/// возвращении к нему (`platform/pwa.rs`) — здесь её зовёт проверка, пока
/// открыта форма. Окно держит каждая форма сама: формы открываются по
/// очереди, и следующая — раньше, чем закрыта прежняя, — так в каждый миг
/// открыта одна, и окна быть не должно. Форма, которая окна не держит, —
/// окно, вставшее над ней.
async function update(page) {
  const next = newVersion(DIST, path.join(WORK, "v2"));
  const srv = own(next.dir, PORT, { quality: 5 });
  const NOTICE = TEXT_IN("main", s("UpdateAfterForm"));
  try {
    page.errors.length = 0;
    let open = null;
    for (const f of FORMS) {
      await go(page, f.hash);
      await page.waitFor(`return ${BUTTON(f.scope, s(f.open))} ? true : null;`);
      await press(page, f.scope, s(f.open));
      checks.check(`новая версия: форма экрана «${f.name}» открыта`, await page.waitFor(f.shown));
      if (open) {
        await go(page, open.hash);
        await press(page, open.scope, s(open.close));
        checks.check(`новая версия: форма экрана «${open.name}» закрыта`, await page.waitFor(open.gone));
      } else {
        await page.eval(`navigator.serviceWorker.getRegistration().then((r) => r.update()); return true;`);
      }
      const waiting = await page.waitFor(NOTICE, open ? 5000 : 30000);
      await sleep(300);
      checks.check(`новая версия: открыта одна форма — «${f.name}»; над экраном слова, а не окно`,
        waiting && (await page.eval(NO_DIALOG)));
      open = f;
    }
    await go(page, open.hash);
    await press(page, open.scope, s(open.close));
    const dialog = await page.waitFor(`const d = document.querySelector("[role=dialog]");
      return d && d.innerText.includes(${JSON.stringify(s("UpdateWhatsNew"))}) ? d.innerText : null;`, 10000);
    checks.check(`новая версия: форму закрыли — окно «Что нового» с заметкой ${next.version}`,
      dialog?.includes(next.version), dialog);
    checks.check("новая версия: программа под окном неактивна",
      await page.eval(`return document.querySelector(".app").inert ? true : null;`));
    await page.eval(`window.__old = 1; return true;`);
    checks.check("новая версия: «Обновить» в окне", await press(page, "[role=dialog]", s("UpdateApply")));
    const reloaded = await page.waitFor(`return window.__old === undefined && document.readyState === "complete" ? true : null;`, 30000);
    const cache = await page.waitFor(`const n = await caches.keys(); return n.length === 1 ? n[0] : null;`, 30000);
    checks.check("новая версия: «Обновить» — новая сборка, прежний кэш удалён",
      reloaded && cache === `kvartik-${next.build}`, cache);
    checks.check("новая версия: рабочий экран, окна больше нет", (await page.waitFor(WORKING)) && (await page.eval(NO_DIALOG)));
    checks.check("новая версия: без плашки сбоя", await page.eval(NO_PANIC));
    const errors = [...new Set(page.errors)];
    checks.check("новая версия: без ошибок в консоли", errors.length === 0, errors.slice(0, 8).join(" | "));
  } finally {
    await srv.stop();
  }
}

async function main() {
  const parts = fs.readdirSync(path.join(DIST, "app")).filter((n) => /^split_\w+-\w+\.wasm$/.test(n));
  checks.check("в сборке есть части wasm", parts.length > 0, parts.join(", "));
  let srv = own(DIST, PORT, { quality: 5 });
  const browser = await Browser.launch({ work: WORK, name: "check", port: 9340 });
  try {
    await blocked(browser);

    const page = await browser.page();
    await page.emulate(360);
    await page.go(`${ORIGIN}/app/`);
    await page.setLang(opt.lang);
    checks.check("с сетью: рабочий экран", await page.waitFor(WORKING));
    const sample = await restore(page);
    checks.check("с сетью: страницей правит service worker", (await page.waitFor(CONTROLLER)) === `${ORIGIN}/app/sw.js`);
    await forms(page, "с сетью");
    await card(page, "с сетью");
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
    await card(page, "без сети");
    checks.check("без сети: без плашки сбоя", await page.eval(NO_PANIC));
    const offline = [...new Set(page.errors)];
    checks.check("без сети: без ошибок в консоли", offline.length === 0, offline.slice(0, 8).join(" | "));

    await update(page);
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
