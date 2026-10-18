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

// Рабочий экран: база открыта, разделы прочитаны — встаёт поле выбора файла
// копии (с 0.0.20 оно — признак готовности, CLAUDE.md, Правило 1).
const WORKING = `return document.querySelector('input[type=file][accept=".json,application/json"]') ? true : null;`;
// Рабочий экран именно этой сборки: старая страница под новым service worker
// проходит все остальные проверки (задача 65, 2026-10-16).
const BUILD_PAGE = `const s = document.querySelector("script[type=module]");
  return s && s.src.startsWith(location.origin + "/app/kvartik-app-") && document.querySelector('input[type=file][accept=".json,application/json"]') ? s.src : null;`;
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
  await page.waitFor(WORKING);
  await page.setFiles(`input[type=file][accept=".json,application/json"]`, [sample.file]);
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
/// том же порту. Пока открыта форма — слова над экраном; закрыли — окно
/// «Что нового» поверх неактивной программы, снимки окна — на `--langs` ×
/// `--widths` × `--themes` (перезагрузка снова показывает окно: новая
/// версия ждёт); «Обновить» открывает новую сборку, прежний кэш удалён.
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
    const shown = await page.waitFor(DIALOG, 10000);
    checks.check(`форму закрыли — окно «Что нового» с заметкой ${next.version}`, shown?.includes(next.version), shown);
    checks.check("программа под окном неактивна", await page.eval(`return document.querySelector(".app").inert ? true : null;`));

    for (const lang of list(opt.langs)) {
      await page.setLang(lang);
      const text = await page.waitFor(DIALOG, 30000);
      checks.check(`${lang}: окно новой версии после перезагрузки`, text?.includes(t("UpdateTitle", lang)), text);
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
    await page.eval(`window.__old = 1; return true;`);
    await page.click(t("UpdateApply", opt.lang));
    const reloaded = await page.waitFor(`return window.__old === undefined && document.readyState === "complete" ? true : null;`, 30000);
    const cache = await page.waitFor(`const n = await caches.keys(); return n.length === 1 ? n[0] : null;`, 30000);
    checks.check("после «Обновить» — новая сборка, прежний кэш удалён", reloaded && cache === `kvartik-${next.build}`, cache);
    checks.check("рабочий экран новой сборки, окна нет", (await page.waitFor(BUILD_PAGE)) && !(await page.eval(DIALOG)));
    await page.shot(path.join(OUT, "update-after.png"));
    reportErrors(page, "новая версия");
  } finally {
    browser.close();
    await srv.stop().catch(() => {});
  }
}

/// Установка: манифест без ошибок, `id` — `/app/`, кнопка «Установить» на
/// главном. Основной контекст браузера: в отдельном вопроса об установке нет.
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
    checks.check("кнопка «Установить»", await page.waitFor(hasText(t("InstallButton", opt.lang)), 15000));
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
/// worker. Сравнивать можно только с замером тем же способом в тот же день.
async function measure() {
  const servers = DISTS.map((d, i) => own(d, 8801 + i));
  const browser = await Browser.launch({ work: WORK, name: "measure", port: 9334 });
  const OBSERVER = `new MutationObserver((_, obs) => {
    if (document.querySelector('input[type=file][accept=".json,application/json"]')) { window.__working = performance.now(); obs.disconnect(); }
  }).observe(document, { childList: true, subtree: true });`;
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
      return { working: window.__working, fcp: (performance.getEntriesByName("first-contentful-paint")[0] || {}).startTime,
        fonts: fonts.length ? Math.max(...fonts) : null };`;
    const first = await page.waitFor(read, 30000);
    let repeat = null;
    if (opt.repeat && first) {
      await page.waitFor(CONTROLLER, 30000);
      await page.send("Page.reload");
      await sleep(300);
      repeat = (await page.waitFor(read, 30000))?.working ?? null;
    }
    await page.close();
    return first && { ...first, repeat };
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
    console.log(`${r.name} ${opt.path}: первый текст ${sec(median(ok.map((x) => x.fcp)))} с, рабочий экран ${sec(median(ok.map((x) => x.working)))} с, `
      + `шрифты ${sec(median(ok.map((x) => x.fonts)))} с${opt.repeat ? `, повторный заход ${median(ok.map((x) => x.repeat)).toFixed(0)} мс` : ""} `
      + `(медиана ${ok.length}; Chrome ${(await chromeVersion())})`);
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

const SCENARIOS = { screens, offline, update, install, measure, pdf };
try {
  if (!SCENARIOS[scenario]) throw new Error(`нет сценария «${scenario}»: ${Object.keys(SCENARIOS).join(", ")}`);
  await SCENARIOS[scenario]();
} catch (e) {
  checks.check(`сценарий ${scenario} дошёл до конца`, false, e.stack);
}
checks.summary();
process.exit(checks.failed ? 1 : 0);
