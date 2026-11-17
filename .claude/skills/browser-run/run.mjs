// Глазной прогон и замер Kvartik в headless Chrome (skill browser-run,
// задача 66). Образец — скрипты задачи 65 (`target/look/task-65/`).
//
//   node .claude/skills/browser-run/run.mjs <сценарий> --task <номер> [опции]
//
// Сценарии — `SCENARIOS` внизу; опции — в SKILL.md рядом. Временное (профили
// Chrome, копии `dist/`, логи серверов) — в `C:/tmp/k<номер>`, снимки —
// в `target/look/task-<номер>/`. Проверки собираются и выдаются в конце,
// код выхода 1 — если хоть одна не прошла.

import { parseArgs } from "node:util";
import fs from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { Browser, Checks, sleep } from "../../../tools/browser/cdp.mjs";
import { REPO, build, newVersion, own, start, wrangler } from "../../../tools/browser/serve.mjs";
import { samples, t } from "../../../tools/browser/strings.mjs";

const { values: opt, positionals } = parseArgs({
  allowPositionals: true,
  options: {
    task: { type: "string" },
    dist: { type: "string", multiple: true },
    server: { type: "string" },
    port: { type: "string" },
    routes: { type: "string", default: "home" },
    langs: { type: "string", default: "hy,ru,en" },
    widths: { type: "string", default: "360,1280" },
    themes: { type: "string", default: "light,dark" },
    lang: { type: "string", default: "ru" },
    sample: { type: "string", default: "last" },
    script: { type: "string" },
    runs: { type: "string", default: "5" },
    path: { type: "string", default: "/app/" },
    repeat: { type: "boolean", default: false },
    sw: { type: "boolean", default: false },
    pdf: { type: "string", multiple: true },
  },
});

const scenario = positionals[0];
if (!opt.task || !scenario) {
  console.log("node .claude/skills/browser-run/run.mjs <screens|offline|update|install|measure|pdf> --task <номер> [опции] — SKILL.md");
  process.exit(2);
}
const WORK = `C:/tmp/k${opt.task}`;
const OUT = path.join(REPO, "target/look", `task-${opt.task}`);
const DISTS = (opt.dist ?? ["crates/app/dist"]).map((d) => path.resolve(REPO, d));
const DIST = DISTS[0];
fs.mkdirSync(WORK, { recursive: true });
const checks = new Checks();
const list = (s) => s.split(",").map((x) => x.trim()).filter(Boolean);
// Экран — именем (`home`, `documents`, `documents/<id>`) или адресом `#/…`.
// Имя — ради Git Bash: аргумент с `/` он переписывает путём Windows.
const hash = (route) => route.startsWith("#") ? route : route === "home" ? "#/" : `#/${route}`;

// Рабочий экран: база открыта, разделы прочитаны — под экраном копии встают
// слова о хранении на языке окна (с 0.0.142, CLAUDE.md, Правило 1; так же
// в `tools/browser/check.mjs`). С 0.0.20 признаком было поле выбора файла
// копии, а с задачи 91 экран копии рисуется с первого показа; слова
// о хранении — в главном wasm и встают в тот же миг, что поле раньше.
const STORED = ["hy", "ru", "en"].map((lang) => t("StorageTitle", lang));
const READY = `[...document.querySelectorAll("#backup h2")].some((h) => ${JSON.stringify(STORED)}.includes(h.textContent))`;
const WORKING = `return ${READY} ? true : null;`;
// Поле выбора файла копии — когда экран копии показан и база открыта.
const BACKUP_FILE = `input[type=file][accept=".json,application/json"]`;
// Рабочий экран именно этой сборки: старая страница под новым service worker
// проходит все остальные проверки (задача 65, 2026-10-16).
const BUILD_PAGE = `const s = document.querySelector("script[type=module]");
  return s && s.src.startsWith(location.origin + "/app/kvartik-app-") && ${READY} ? s.src : null;`;
const CONTROLLER = `return navigator.serviceWorker.controller ? navigator.serviceWorker.controller.scriptURL : null;`;
const CACHES = `const out = {}; for (const n of await caches.keys()) out[n] = (await (await caches.open(n)).keys()).map((r) => r.url); return out;`;
const hasText = (text) => `return document.body && document.body.innerText.includes(${JSON.stringify(text)}) ? true : null;`;

function reportErrors(page, name) {
  const errors = [...new Set(page.errors)];
  checks.check(`${name}: без ошибок в консоли, CSP и сети`, errors.length === 0, errors.slice(0, 8).join(" | "));
}

/// Восстановить образец копии: экранам — данные. Пароль образца станет
/// паролем копий устройства (2026-10-15).
async function restore(page, lang) {
  if (opt.sample === "none") return;
  const all = samples();
  const sample = opt.sample === "last" ? all.at(-1) : all.find((s) => s.file.includes(`${path.sep}${opt.sample}${path.sep}`));
  if (!sample) throw new Error(`нет образца ${opt.sample} в samples.rs`);
  await page.eval(`location.hash = "#/backup"; return true;`);
  checks.check("образец: экран копии нарисован",
    await page.waitFor(`return document.querySelector(${JSON.stringify(BACKUP_FILE)}) ? true : null;`));
  await page.setFiles(BACKUP_FILE, [sample.file]);
  const submit = `[...document.querySelectorAll("form.backup button[type=submit]")].find((b) =>
    b.textContent.trim() === ${JSON.stringify(t("BackupRestore", lang))} && b.getClientRects().length > 0)`;
  checks.check("образец: спросил пароль копии", await page.waitFor(`return ${submit} ? true : null;`));
  await page.eval(`const b = ${submit}; const input = b.form.querySelector("input[type=password]");
    input.value = ${JSON.stringify(sample.password)}; input.dispatchEvent(new Event("input", { bubbles: true }));
    b.click(); return true;`);
  const done = t("BackupRestored", lang).split("{}")[0];
  checks.check(`образец ${path.basename(path.dirname(sample.file))}: восстановлен`,
    await page.waitFor(`return [...document.querySelectorAll("p[role=status]")].some((p) => p.textContent.startsWith(${JSON.stringify(done)})) ? true : null;`, 60000));
}

/// Экраны `--routes` на `--langs` × `--widths` × `--themes`. `--script` —
/// модуль с `export default async (page, ctx)`: шаги перед каждым снимком
/// (открыть форму, ввести текст); `ctx` — `{ lang, width, theme, route, t, shot }`.
async function screens() {
  const srv = await start(opt.server ?? "own", DIST, Number(opt.port ?? 8780), WORK);
  const browser = await Browser.launch({ work: WORK, name: "screens", port: 9330 });
  const page = await browser.page();
  const origin = `http://127.0.0.1:${srv.port}`;
  const step = opt.script ? (await import(pathToFileURL(path.resolve(opt.script)))).default : null;
  try {
    await page.emulate(1280);
    await page.go(`${origin}/app/`);
    await page.setLang(opt.lang);
    checks.check("рабочий экран", await page.waitFor(WORKING));
    await restore(page, opt.lang);
    for (const lang of list(opt.langs)) {
      await page.setLang(lang);
      await page.waitFor(WORKING);
      for (const width of list(opt.widths).map(Number)) {
        for (const theme of list(opt.themes)) {
          await page.emulate(width, theme);
          // Снимок раскладки — без движения (задача 86): блок ниже первого
          // экрана иначе снят бы до прокрутки — ещё не выплывшим. Движение
          // включает своё `--script`.
          await page.send("Emulation.setEmulatedMedia", { features: [
            { name: "prefers-color-scheme", value: theme }, { name: "prefers-reduced-motion", value: "reduce" }] });
          for (const route of list(opt.routes)) {
            await page.eval(`location.hash = ${JSON.stringify(hash(route))}; return true;`);
            await sleep(400);
            const name = `${route.replace(/^#?\/?/, "").replace(/[^\w-]+/g, "-") || "home"}-${lang}-${width}-${theme}`;
            const shot = (suffix) => page.shot(path.join(OUT, `${name}${suffix ? `-${suffix}` : ""}.png`));
            if (step) await step(page, { lang, width, theme, route, t: (key) => t(key, lang), shot });
            const { scroll, client } = await shot();
            checks.check(`${name}: не шире экрана`, scroll <= client && (!page.device.mobile || client === width),
              `scrollWidth ${scroll}, clientWidth ${client}`);
          }
        }
      }
    }
    reportErrors(page, "экраны");
  } finally {
    browser.close();
    await srv.stop();
  }
  console.log(`снимки: ${OUT}`);
}

/// Программа ставится и правит страницей, потом сервер гаснет — и она
/// открывается из кэша service worker. «Без сети» — выключенный сервер,
/// а не эмуляция (Правило 2).
async function offline() {
  const port = Number(opt.port ?? 8790);
  const srv = await wrangler(DIST, port, WORK);
  const browser = await Browser.launch({ work: WORK, name: "offline", port: 9331 });
  const page = await browser.page();
  const origin = `http://127.0.0.1:${port}`;
  try {
    await page.emulate(360);
    await page.go(`${origin}/app/`);
    await page.setLang(opt.lang);
    const scope = await page.waitFor(`return (await navigator.serviceWorker.ready).scope;`);
    checks.check("scope service worker — /app/", scope === `${origin}/app/`, scope);
    await page.reload();
    checks.check("страницей правит /app/sw.js", (await page.waitFor(CONTROLLER)) === `${origin}/app/sw.js`);
    const cached = await page.eval(CACHES);
    const urls = Object.values(cached).flat();
    checks.check("один кэш, в нём страница и только файлы /app/",
      Object.keys(cached).length === 1 && urls.includes(`${origin}/app/`) && urls.every((u) => u.startsWith(`${origin}/app/`)),
      `${Object.keys(cached)} — ${urls.length} файлов`);
    checks.check("рабочий экран", await page.waitFor(WORKING));
    await page.shot(path.join(OUT, "offline-online.png"));
    reportErrors(page, "с сетью");
    await srv.stop();
    page.errors.length = 0;
    await page.fresh(`${origin}/app/#/documents`);
    checks.check("без сети: рабочий экран из кэша", (await page.waitFor(WORKING)) && (await page.eval(CONTROLLER)) === `${origin}/app/sw.js`);
    await page.shot(path.join(OUT, "offline-offline.png"));
  } finally {
    browser.close();
    await srv.stop().catch(() => {});
  }
}

/// Новая версия: копия `dist/` новой версией (`newVersion` — заметка «Что
/// нового» новее, правка файла корня, `sw.js`, переписанный `kvartik-sw`) на
/// том же порту. Пока открыта форма — слова над экраном; закрыли — новая
/// сборка включается сама, прежний кэш удалён, и встаёт окно «Что нового»
/// поверх неактивной программы. Снимки окна — на `--langs` × `--widths` ×
/// `--themes` (перезагрузка снова показывает его: пока не закрыто, окно
/// встаёт на каждом открытии); «Понятно» его закрывает. Копия — тот же wasm
/// с теми же числами: окно встаёт, только если устройство помнит версию
/// старше, и в нём — заметки до работающей версии.
async function update() {
  const port = Number(opt.port ?? 8791);
  const next = newVersion(DIST, path.join(WORK, "v2"));
  const one = build(DIST);
  checks.check("копия — другая сборка", one !== next.build, `${one} → ${next.build}`);
  const DIALOG = `const d = document.querySelector("[role=dialog]"); return d && d.getClientRects().length > 0 ? d.innerText : null;`;

  let srv = await wrangler(DIST, port, WORK);
  const browser = await Browser.launch({ work: WORK, name: "update", port: 9332 });
  const page = await browser.page();
  const origin = `http://127.0.0.1:${port}`;
  try {
    await page.emulate(360);
    await page.go(`${origin}/app/`);
    await page.setLang(opt.lang);
    await page.waitFor(`return (await navigator.serviceWorker.ready).scope;`);
    await page.reload();
    checks.check("прежняя сборка правит страницей", await page.waitFor(CONTROLLER));
    await srv.stop();
    srv = await wrangler(next.dir, port, WORK);
    await page.eval(`localStorage.setItem("kvartik.seen_version", "0.0.1"); window.__old = 1; return true;`);

    // Новая версия пришла, пока открыта форма: слова над экраном, а не окно.
    await page.eval(`location.hash = "#/clients"; return true;`);
    await page.waitFor(hasText(t("ClientAdd", opt.lang)));
    await page.click(t("ClientAdd", opt.lang));
    await page.eval(`navigator.serviceWorker.getRegistration().then((r) => r.update()); return true;`);
    checks.check("с открытой формой — слова над экраном, окна нет",
      (await page.waitFor(hasText(t("UpdateAfterForm", opt.lang)), 30000)) && !(await page.eval(DIALOG)));
    await page.eval(`scrollTo(0, 0); return true;`);
    await page.shot(path.join(OUT, `update-form-${opt.lang}-360.png`));
    await page.click(t("Back", opt.lang));
    const reloaded = await page.waitFor(`return window.__old === undefined && document.readyState === "complete" ? true : null;`, 30000);
    const cache = await page.waitFor(`const n = await caches.keys(); return n.length === 1 ? n[0] : null;`, 30000);
    checks.check("форму закрыли — новая сборка сама, прежний кэш удалён", reloaded && cache === `kvartik-${next.build}`, cache);
    const shown = await page.waitFor(DIALOG, 10000);
    checks.check("после обновления — окно «Что нового»", shown?.includes(t("UpdateWhatsNew", opt.lang)), shown);
    checks.check("программа под окном неактивна", await page.eval(`return document.querySelector(".app").inert ? true : null;`));

    for (const lang of list(opt.langs)) {
      await page.setLang(lang);
      const text = await page.waitFor(DIALOG, 30000);
      checks.check(`${lang}: окно «Что нового» после перезагрузки`, text?.includes(t("UpdateTitle", lang)), text);
      for (const width of list(opt.widths).map(Number)) {
        for (const theme of list(opt.themes)) {
          await page.emulate(width, theme);
          await sleep(300);
          const { scroll, client } = await page.shot(path.join(OUT, `update-${lang}-${width}-${theme}.png`));
          checks.check(`update-${lang}-${width}-${theme}: не шире экрана`, scroll <= client, `scrollWidth ${scroll}, clientWidth ${client}`);
        }
      }
    }

    await page.setLang(opt.lang);
    await page.waitFor(DIALOG, 30000);
    await page.emulate(360);
    await page.click(t("UpdateGotIt", opt.lang));
    await sleep(300);
    checks.check("«Понятно» — рабочий экран новой сборки, окна нет", (await page.waitFor(BUILD_PAGE)) && !(await page.eval(DIALOG)));
    await page.shot(path.join(OUT, "update-after.png"));
    reportErrors(page, "новая версия");
  } finally {
    browser.close();
    await srv.stop().catch(() => {});
  }
}

/// Установка: манифест без ошибок, `id` — `/app/`, кнопка «Установить» на
/// главном задаёт вопрос браузера. Кнопка есть и без него (задача 55),
/// но тогда раскрывает слова и несёт `aria-expanded`; пришёл вопрос — этого
/// атрибута у неё нет. Основной контекст браузера: в отдельном вопроса об
/// установке нет.
async function install() {
  const port = Number(opt.port ?? 8792);
  const srv = await wrangler(DIST, port, WORK);
  const browser = await Browser.launch({ work: WORK, name: "install", port: 9333 });
  const page = await browser.page();
  const origin = `http://127.0.0.1:${port}`;
  try {
    await page.emulate(1280);
    await page.go(`${origin}/app/`);
    await page.setLang(opt.lang);
    await page.waitFor(WORKING);
    const { installabilityErrors } = await page.send("Page.getInstallabilityErrors");
    checks.check("устанавливается: ошибок установки нет", installabilityErrors.length === 0, JSON.stringify(installabilityErrors));
    const { appId } = await page.send("Page.getAppId");
    checks.check("id программы — /app/", appId === `${origin}/app/`, appId);
    const asks = `const button = [...document.querySelectorAll("#home .install button")]
      .find((b) => b.textContent.trim() === ${JSON.stringify(t("InstallButton", opt.lang))});
      return button && !button.hasAttribute("aria-expanded") ? true : null;`;
    checks.check("кнопка «Установить» задаёт вопрос браузера", await page.waitFor(asks, 15000));
    await page.shot(path.join(OUT, "install.png"));
  } finally {
    browser.close();
    await srv.stop();
  }
}

/// Первый заход на телефонном профиле по образцу Lighthouse (Правило 1):
/// RTT 562,5 мс, 1,47 Мбит/с, процессор ×4, без кэша, каждый заход в своём
/// контексте. Сборки `--dist` — по кругу после одного прогревочного захода,
/// медиана `--runs`. `--repeat` — ещё и повторный заход из кэша service
/// worker. `--sw` — ещё и когда страницей стал править service worker:
/// он поставил весь сайт, и части приходят из его кэша (задача 80; каждый
/// лишний файл — круг сети по шести соединениям, CLAUDE.md, Правило 6).
/// Сравнивать можно только с замером тем же способом в тот же день.
async function measure() {
  const servers = DISTS.map((d, i) => own(d, 8801 + i));
  const browser = await Browser.launch({ work: WORK, name: "measure", port: 9334 });
  // Рабочий экран — слова о хранении (`READY`). Поле файла копии — прежний
  // признак (до задачи 91): у сборок до 0.0.142 оно встаёт тогда же, и замер
  // их обоими признаками сшивает ряд `docs/measurements.md`; у новых его нет
  // до показа экрана копии — `old` пустой.
  const OBSERVER = `new MutationObserver((_, obs) => {
    if (window.__old === undefined && document.querySelector(${JSON.stringify(BACKUP_FILE)})) window.__old = performance.now();
    if (${READY}) { window.__working = performance.now(); obs.disconnect(); }
  }).observe(document, { childList: true, subtree: true });
  navigator.serviceWorker?.addEventListener("controllerchange", () => { window.__sw ??= performance.now(); });`;
  const visit = async (url) => {
    const page = await browser.page({ context: true });
    await page.send("Network.enable");
    await page.send("Network.setCacheDisabled", { cacheDisabled: true });
    await page.send("Network.emulateNetworkConditions", { offline: false, latency: 562.5,
      downloadThroughput: 1474.56 * 1024 / 8, uploadThroughput: 675 * 1024 / 8 });
    await page.send("Emulation.setCPUThrottlingRate", { rate: 4 });
    await page.send("Emulation.setDeviceMetricsOverride", { width: 412, height: 823, deviceScaleFactor: 1.75, mobile: true });
    await page.send("Page.addScriptToEvaluateOnNewDocument", { source: OBSERVER });
    await page.send("Page.navigate", { url });
    const read = `if (window.__working === undefined) return null; await document.fonts.ready;
      const fonts = performance.getEntriesByType("resource").filter((r) => r.name.endsWith(".ttf")).map((r) => r.responseEnd);
      return { working: window.__working, old: window.__old ?? null,
        fcp: (performance.getEntriesByName("first-contentful-paint")[0] || {}).startTime,
        fonts: fonts.length ? Math.max(...fonts) : null };`;
    const first = await page.waitFor(read, 30000);
    const sw = opt.sw && first ? await page.waitFor(`return window.__sw ?? null;`, 60000) : null;
    let repeat = null;
    if (opt.repeat && first) {
      await page.waitFor(CONTROLLER, 30000);
      await page.send("Page.reload");
      await sleep(300);
      repeat = (await page.waitFor(read, 30000))?.working ?? null;
    }
    await page.close();
    return first && { ...first, sw, repeat };
  };
  const runs = DISTS.map((d, i) => ({ name: path.relative(REPO, d) || d, url: `http://127.0.0.1:${8801 + i}${opt.path}`, results: [] }));
  try {
    for (const r of runs) await visit(r.url);
    for (let round = 0; round < Number(opt.runs); round++) {
      for (const r of runs) {
        const v = await visit(r.url);
        r.results.push(v);
        console.log(round, r.name, JSON.stringify(v));
      }
    }
  } finally {
    browser.close();
    for (const s of servers) await s.stop();
  }
  const median = (xs) => { const s = xs.filter((x) => x != null).sort((a, b) => a - b); return s.length ? s[s.length >> 1] : NaN; };
  const sec = (ms) => (ms / 1000).toFixed(2);
  for (const r of runs) {
    const ok = r.results.filter(Boolean);
    checks.check(`${r.name}: все заходы дошли до рабочего экрана`, ok.length === Number(opt.runs), `${ok.length} из ${opt.runs}`);
    const old = ok.some((x) => x.old != null) ? ` (поле файла копии — ${sec(median(ok.map((x) => x.old)))} с)` : "";
    console.log(`${r.name} ${opt.path}: первый текст ${sec(median(ok.map((x) => x.fcp)))} с, рабочий экран ${sec(median(ok.map((x) => x.working)))} с${old}, `
      + `шрифты ${sec(median(ok.map((x) => x.fonts)))} с${opt.sw ? `, под service worker ${sec(median(ok.map((x) => x.sw)))} с` : ""}`
      + `${opt.repeat ? `, повторный заход ${median(ok.map((x) => x.repeat)).toFixed(0)} мс` : ""} `
      + `(медиана ${ok.length}; Chrome ${(await chromeVersion())})`);
  }
}

/// Движение под мышью (задача 87): на лендинге `/ru/`, 1280 точек, процессор
/// ×4 — мышь над простым текстом (контроль: только кружок у курсора),
/// по карточке «что умеет» (наклон и блик), на главную кнопку и с неё
/// (заливка и подпись), на ссылку подвала и с неё (линия). Трассировка
/// Chrome считает перерисовки (`Paint`) и пересчёты стиля главного потока
/// и его занятость, цикл `requestAnimationFrame` — пропущенные кадры
/// (промежуток больше полутора кадров при 60 Гц). Сборки — по кругу,
/// каждая в своём браузере; медиана `--runs` кругов. Эффект, который ведёт
/// видеокарта (`scale`, `translate`, `opacity`), перерисовок почти не даёт.
async function motion() {
  const SCENARIOS = {
    plain: { selector: ".hero .lead", moves: (b) => Array.from({ length: 61 }, (_, i) =>
      [b.x + 10 + b.w * 0.8 * (i / 60), b.y + b.h * 0.5]), start: (b) => [b.x + 10, b.y + 10] },
    card: { selector: ".cards li:nth-child(2)", moves: (b) => Array.from({ length: 61 }, (_, i) =>
      [b.x + b.w * (0.1 + 0.8 * (i / 60)), b.y + b.h * (0.5 + 0.35 * Math.sin((i / 60) * 6.28))]),
      start: (b) => [b.x - 20, b.y + b.h / 2] },
    button: { selector: ".hero .button", hover: (b) => [[b.x + 16, b.y + b.h / 2], [b.x + b.w + 40, b.y + b.h / 2]],
      start: (b) => [b.x - 40, b.y + b.h / 2] },
    link: { selector: "footer a", hover: (b) => [[b.x + 6, b.y + b.h / 2], [b.x - 30, b.y - 30]],
      start: (b) => [b.x - 30, b.y - 30] },
  };
  const results = DISTS.map(() => ({}));
  for (let round = 0; round < Number(opt.runs); round++) {
    for (const [index, dist] of DISTS.entries()) {
      const srv = own(dist, 8821 + index);
      const browser = await Browser.launch({ work: WORK, name: `motion-${index}`, port: 9336 + index });
      try {
        for (const [name, s] of Object.entries(SCENARIOS)) {
          const page = await browser.page({ context: true });
          const events = [];
          let complete;
          const finished = new Promise((r) => { complete = r; });
          page.on((msg) => {
            if (msg.method === "Tracing.dataCollected") events.push(...msg.params.value);
            if (msg.method === "Tracing.tracingComplete") complete();
          });
          await page.emulate(1280);
          await page.send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-reduced-motion", value: "no-preference" }] });
          await page.go(`http://127.0.0.1:${srv.port}/ru/`);
          await page.waitFor(`return document.fonts.ready.then(() => true);`);
          await sleep(500);
          const mouse = ([x, y]) => page.send("Input.dispatchMouseEvent", { type: "mouseMoved", x, y });
          const b = await page.eval(`const e = document.querySelector(${JSON.stringify(s.selector)});
            e.scrollIntoView({ block: "center" }); const r = e.getBoundingClientRect();
            return { x: r.x, y: r.y, w: r.width, h: r.height };`);
          await mouse(s.start(b));
          await sleep(300);
          await page.send("Emulation.setCPUThrottlingRate", { rate: 4 });
          await page.send("Tracing.start", { transferMode: "ReportEvents",
            traceConfig: { includedCategories: ["devtools.timeline", "disabled-by-default-devtools.timeline"] } });
          await page.eval(`window.__gaps = []; let last = performance.now(); window.__run = true;
            const tick = (t) => { if (!window.__run) return; window.__gaps.push(t - last); last = t; requestAnimationFrame(tick); };
            requestAnimationFrame(tick); return true;`);
          if (s.moves) {
            for (const at of s.moves(b)) { await mouse(at); await sleep(16); }
            await mouse(s.start(b));
            await sleep(600);
          } else {
            for (let n = 0; n < 2; n++) {
              for (const at of s.hover(b)) { await mouse(at); await sleep(700); }
            }
          }
          const gaps = await page.eval(`window.__run = false; return window.__gaps.slice(1);`);
          await page.send("Tracing.end");
          await finished;
          await page.send("Emulation.setCPUThrottlingRate", { rate: 1 });
          const paints = events.filter((e) => e.name === "Paint" && e.ph === "X");
          const tid = paints[0]?.tid ?? events.find((e) => e.name === "UpdateLayoutTree")?.tid;
          (results[index][name] ??= []).push({
            paints: paints.length,
            style: events.filter((e) => e.tid === tid && e.name === "UpdateLayoutTree").length,
            busy: events.filter((e) => e.tid === tid && e.ph === "X" && e.name === "RunTask")
              .reduce((sum, e) => sum + (e.dur ?? 0), 0) / 1000,
            dropped: gaps.filter((g) => g > (1000 / 60) * 1.5).length,
          });
          await page.close();
        }
      } finally {
        browser.close();
        await srv.stop();
      }
    }
  }
  const median = (xs) => [...xs].sort((a, b) => a - b)[xs.length >> 1];
  for (const [index, dist] of DISTS.entries()) {
    for (const [name, runs] of Object.entries(results[index])) {
      const pick = (key) => median(runs.map((r) => r[key]));
      console.log(`${path.relative(REPO, dist) || dist} ${name}: перерисовок ${pick("paints")}, пересчётов стиля ${pick("style")}, `
        + `главный поток ${Math.round(pick("busy"))} мс, пропущено кадров ${pick("dropped")} (медиана ${runs.length}; Chrome ${await chromeVersion()})`);
    }
    checks.check(`${path.relative(REPO, dist) || dist}: замер движения дошёл до конца`, Object.keys(results[index]).length === 4);
  }
}

async function chromeVersion() {
  // `chrome.exe --version` на Windows открывает окно вместо ответа: версия —
  // имя папки рядом с программой (2026-10-13).
  const dir = path.dirname(process.env.CHROME ?? "C:/Program Files/Google/Chrome/Application/chrome.exe");
  return fs.readdirSync(dir).find((n) => /^\d+\.\d+\.\d+\.\d+$/.test(n)) ?? "?";
}

/// Страница PDF читалкой самого Chrome: `--pdf <файл>` (можно несколько).
/// Читалке нужно секунды три (проверено 2026-10-14).
async function pdf() {
  const browser = await Browser.launch({ work: WORK, name: "pdf", port: 9335 });
  const page = await browser.page();
  try {
    await page.emulate(1280);
    for (const file of opt.pdf ?? []) {
      await page.go(pathToFileURL(path.resolve(file)).href);
      await sleep(3000);
      const out = path.join(OUT, `${path.basename(file, ".pdf")}.png`);
      await page.shot(out);
      checks.check(`${path.basename(file)}: снимок`, fs.existsSync(out), out);
    }
  } finally {
    browser.close();
  }
}

const SCENARIOS = { screens, offline, update, install, measure, motion, pdf };
try {
  if (!SCENARIOS[scenario]) throw new Error(`нет сценария «${scenario}»: ${Object.keys(SCENARIOS).join(", ")}`);
  await SCENARIOS[scenario]();
} catch (e) {
  checks.check(`сценарий ${scenario} дошёл до конца`, false, e.stack);
}
checks.summary();
process.exit(checks.failed ? 1 : 0);
