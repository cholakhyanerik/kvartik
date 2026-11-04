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
//    выпущенного документа, PDF, выгрузка с закрытым кварталом, разошедшимся
//    с поданным, новая копия — каждая из девяти частей;
//    и все части `dist/app/split_*.wasm` загружены. «Установить» на
//    главном экране — и без вопроса браузера, и раскладка, которую видно
//    только с настоящим CSS: кнопка не вплотную к карточке налога под ней.
// 3. Сервер выключен: программа открывается из кэша service worker, и те же
//    экраны, формы, карточка и PDF работают без сети.
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
//   частей (`wasm_split_cli_support` 0.2.4) пишет отказ заблокированной
//   части в консоль (`console.error` в `wrapAsyncCb`) — это его код,
//   человеку он не мешает, и провалом здесь не считается.
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
