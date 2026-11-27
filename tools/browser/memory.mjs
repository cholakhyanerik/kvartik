// Замер памяти копии (задача 122): высота памяти wasm и рабочий набор
// процесса вкладки во время «Сделать копию» — на Windows (`Get-Process`).
//
//   node tools/browser/memory.mjs --task 122 --dist crates/app/dist --files 50 [--sample v14] [--name new]
//
// 1. Чистый профиль: образец копии кнопкой окна (`--sample` — часть пути из
//    `samples.rs`, по умолчанию последний), потом `--files` файлов книги по
//    2 000 000 байтов шума (PDF почти не сжимается, как сканы и фото) прямо
//    в IndexedDB базы `kvartik` — первого ИП.
// 2. Chrome закрыт командой `Browser.close` — `localStorage` он пишет на диск
//    лениво, и убитый процесс открылся бы без языка и пароля копий, — и
//    открыт заново с тем же профилем: пик рабочего набора процесса — только
//    от запуска и копии, без строк JS наполнения.
// 3. «Сделать копию» паролем образца — ждём «Копия готова». Печатает строку
//    JSON (время, части, пик и рабочий набор рендереров до и после, память
//    wasm до и после) и по секундам — рабочий набор самого большого рендерера
//    (каждые 100 мс) и память wasm.
//
// Память wasm — `buffer.byteLength` памяти модуля окна, которую подменённый
// `WebAssembly.instantiateStreaming` кладёт в `window.__kvMemory`: она не
// убывает, и это высота за всю жизнь вкладки. Рабочий набор шумит от сборки
// мусора JS — Chrome на компьютере собирает его лениво, — поэтому мерить
// по нескольку раз и сравнивать высоту wasm в первую очередь. Образец v27 —
// контора на три ИП (копия всех ИП, `make_all`), v14 — один ИП (`make`).
import { parseArgs } from "node:util";
import fs from "node:fs";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { Browser, CHROME, kill, sleep, spawnTree } from "./cdp.mjs";
import { REPO, own } from "./serve.mjs";
import { samples, t } from "./strings.mjs";

const { values: opt } = parseArgs({ options: {
  task: { type: "string" }, dist: { type: "string", default: "crates/app/dist" },
  files: { type: "string", default: "50" }, sample: { type: "string", default: "last" },
  name: { type: "string", default: "memory" }, port: { type: "string", default: "8791" },
  devtools: { type: "string", default: "9391" },
} });
if (!opt.task) {
  console.log("node tools/browser/memory.mjs --task <номер> [--dist …] [--files 50] [--sample v14] [--name …]");
  process.exit(2);
}
const WORK = `C:/tmp/k${opt.task}`;
fs.mkdirSync(WORK, { recursive: true });
const profile = path.join(WORK, `chrome-${opt.name}`);
const lang = "ru";
const files = Number(opt.files);
const mib = (bytes) => Math.round(bytes / 1048576);

/// Chrome с профилем `profile`; `fresh` — чистым.
async function launch(fresh) {
  if (fresh) fs.rmSync(profile, { recursive: true, force: true });
  const port = Number(opt.devtools);
  const child = spawnTree(CHROME, ["--headless=new", `--remote-debugging-port=${port}`, `--user-data-dir=${profile}`,
    "--no-first-run", "--no-default-browser-check", "about:blank"], { stdio: "ignore" });
  let url;
  for (let i = 0; i < 100 && !url; i++) {
    try { url = (await (await fetch(`http://127.0.0.1:${port}/json/version`)).json()).webSocketDebuggerUrl; }
    catch { await sleep(200); }
  }
  if (!url) { kill(child.pid); throw new Error(`Chrome не открыл DevTools на порту ${port}`); }
  const browser = new Browser(child, new WebSocket(url));
  await new Promise((res, rej) => { browser.ws.onopen = res; browser.ws.onerror = rej; });
  browser.ws.onmessage = (m) => browser.message(JSON.parse(m.data));
  return browser;
}

const WORKING = `return [...document.querySelectorAll("#backup h2")].some((h) => h.textContent === ${JSON.stringify(t("StorageTitle", lang))}) ? true : null;`;
const BACKUP_FILE = `input[type=file][accept=".json,application/json"]`;

// Память модуля окна — её делят и части wasm; файлы копии — сами Blob:
// `fetch` ссылки `blob:` не пускает CSP сервера (skill browser-run).
const HOOK = `(() => {
  const keep = (r) => { const m = (r.instance ?? r).exports?.memory; if (m && !window.__kvMemory) window.__kvMemory = m; return r; };
  const s = WebAssembly.instantiateStreaming; WebAssembly.instantiateStreaming = (...a) => s(...a).then(keep);
  const i = WebAssembly.instantiate; WebAssembly.instantiate = (...a) => i(...a).then(keep);
  window.__kvBlobs = [];
  const c = URL.createObjectURL; URL.createObjectURL = (b) => { window.__kvBlobs.push(b); return c.call(URL, b); };
})();`;
const WASM = `return window.__kvMemory ? window.__kvMemory.buffer.byteLength : null;`;

/// Рабочий набор и его пик у процессов-рендереров. Пропал один — код выхода
/// `Get-Process` не ноль, а остальные в выводе.
function renderers(info) {
  const ids = info.processInfo.filter((p) => p.type === "renderer").map((p) => p.id);
  if (!ids.length) return [];
  let out;
  try {
    out = execFileSync("powershell", ["-NoProfile", "-Command",
      `Get-Process -Id ${ids.join(",")} -ErrorAction SilentlyContinue | ForEach-Object { "$($_.WorkingSet64) $($_.PeakWorkingSet64)" }`],
      { encoding: "utf8" });
  } catch (error) {
    out = String(error.stdout ?? "");
  }
  return out.trim().split(/\r?\n/).filter(Boolean).map((l) => { const [ws, peak] = l.split(" ").map(Number); return { ws, peak }; });
}

/// Наибольшее за каждую секунду от `from`: `-1:115 0:508 1:646 …`.
function seconds(rows, from, until) {
  const out = [];
  for (let s = -1; from + s * 1000 <= until; s++) {
    const slot = rows.filter(([at]) => at >= from + s * 1000 && at < from + (s + 1) * 1000).map(([, v]) => v);
    if (slot.length) out.push(`${s}:${mib(Math.max(...slot))}`);
  }
  return out.join(" ");
}

const srv = own(path.resolve(REPO, opt.dist), Number(opt.port));
const origin = `http://127.0.0.1:${srv.port}`;
const sample = opt.sample === "last" ? samples().at(-1) : samples().find((s) => s.file.includes(opt.sample));
if (!sample) throw new Error(`нет образца ${opt.sample} в samples.rs`);
let browser;
try {
  // 1. Образец и файлы книги.
  browser = await launch(true);
  let page = await browser.page();
  await page.emulate(1280);
  await page.go(`${origin}/app/`);
  await page.setLang(lang);
  if (!await page.waitFor(WORKING, 60000)) throw new Error("рабочий экран не встал");
  await page.eval(`location.hash = "#/backup"; return true;`);
  await page.waitFor(`return document.querySelector(${JSON.stringify(BACKUP_FILE)}) ? true : null;`);
  await page.setFiles(BACKUP_FILE, [sample.file]);
  const submit = `[...document.querySelectorAll("form.backup button[type=submit]")].find((b) =>
    b.textContent.trim() === ${JSON.stringify(t("BackupRestore", lang))} && b.getClientRects().length > 0)`;
  if (!await page.waitFor(`return ${submit} ? true : null;`)) throw new Error("пароль образца не спрошен");
  await page.eval(`const b = ${submit}; const input = b.form.querySelector("input[type=password]");
    input.value = ${JSON.stringify(sample.password)}; input.dispatchEvent(new Event("input", { bubbles: true }));
    b.click(); return true;`);
  const restored = t("BackupRestored", lang).split("{}")[0];
  if (!await page.waitFor(`return [...document.querySelectorAll("p[role=status]")].some((p) => p.textContent.startsWith(${JSON.stringify(restored)})) ? true : null;`, 60000)) {
    throw new Error("образец не восстановлен");
  }
  const seeded = await page.eval(`
    const db = await new Promise((res, rej) => { const r = indexedDB.open("kvartik"); r.onsuccess = () => res(r.result); r.onerror = () => rej(r.error); });
    for (let n = 0; n < ${files}; n++) {
      const bytes = new Uint8Array(2000000);
      for (let at = 0; at < bytes.length; at += 65536) crypto.getRandomValues(bytes.subarray(at, Math.min(at + 65536, bytes.length)));
      bytes.set(new TextEncoder().encode("%PDF-1.7\\n"));
      let binary = "";
      for (let at = 0; at < bytes.length; at += 0x8000) binary += String.fromCharCode.apply(null, bytes.subarray(at, at + 0x8000));
      const id = crypto.randomUUID();
      const text = '{"format_version":27,"record":{"meta":{"id":"' + id + '","modified":' + Date.now() + ',"deleted":null},"owner":"'
        + crypto.randomUUID() + '","name":"scan-' + n + '.pdf","file":"' + btoa(binary) + '"}}';
      await new Promise((res, rej) => {
        const tx = db.transaction("records", "readwrite", { durability: "strict" });
        tx.objectStore("records").put(text, ["attachment", id]);
        tx.oncomplete = res; tx.onerror = () => rej(tx.error); tx.onabort = () => rej(tx.error);
      });
    }
    db.close();
    return ${files};`);
  if (seeded !== files) throw new Error(`наполнение: ${JSON.stringify(seeded)}`);
  const exited = new Promise((res) => browser.child.once("exit", res));
  await browser.send("Browser.close").catch(() => {});
  await Promise.race([exited, sleep(15000)]);
  browser.close();
  await sleep(1000);

  // 2. Тот же профиль, новый процесс.
  browser = await launch(false);
  page = await browser.page();
  await page.send("Page.addScriptToEvaluateOnNewDocument", { source: HOOK });
  await page.emulate(1280);
  await page.go(`${origin}/app/#/backup`);
  if (!await page.waitFor(WORKING, 60000)) throw new Error("рабочий экран не встал после перезапуска");
  await sleep(1500);
  // Пробный срок образца кончился — окно при запуске ведёт на экран лицензии.
  await page.eval(`location.hash = "#/backup"; return true;`);
  await page.waitFor(`return location.hash === "#/backup" && [...document.querySelectorAll("form.backup button[type=submit]")]
    .some((b) => b.getClientRects().length > 0) ? true : null;`);
  await sleep(500);
  const info = await browser.send("SystemInfo.getProcessInfo");
  const before = renderers(info);
  const wasmBefore = await page.eval(WASM);

  // 3. Копия, с графиком рядом.
  const fresh = await page.eval(`return [...document.querySelectorAll("label")].some((l) => l.getClientRects().length > 0
    && (l.firstChild?.textContent ?? "").trim() === ${JSON.stringify(t("BackupNewPassword", lang))});`);
  if (fresh) {
    await page.type(t("BackupNewPassword", lang), sample.password);
    await page.type(t("BackupRepeatPassword", lang), sample.password);
  } else {
    await page.type(t("BackupPassword", lang), sample.password);
  }
  const pids = info.processInfo.filter((p) => p.type === "renderer").map((p) => p.id);
  const log = path.join(WORK, `memory-${opt.name}.txt`);
  fs.rmSync(log, { force: true });
  const sampler = spawnTree("powershell", ["-NoProfile", "-Command",
    `while ($true) { $s = (Get-Process -Id ${pids.join(",")} -ErrorAction SilentlyContinue | Measure-Object WorkingSet64 -Maximum).Maximum; `
    + `"$([DateTimeOffset]::Now.ToUnixTimeMilliseconds()) $s" | Out-File -Append -Encoding ascii '${log}'; Start-Sleep -Milliseconds 100 }`],
    { stdio: "ignore" });
  await sleep(1500);
  const wasmRows = [];
  let polling = true;
  const poll = (async () => {
    while (polling) {
      const v = await page.eval(WASM);
      if (typeof v === "number") wasmRows.push([Date.now(), v]);
      await sleep(250);
    }
  })();
  const started = Date.now();
  await page.click(t("BackupMake", lang));
  const ready = t("BackupReady", lang).split("{}")[0];
  const parts = await page.waitFor(`return [...document.querySelectorAll("p[role=status]")].some((p) => p.textContent.startsWith(${JSON.stringify(ready)}))
    ? document.querySelectorAll(".backup-file a[download]").length : null;`, 600000);
  const took = (Date.now() - started) / 1000;
  await sleep(2000);
  polling = false;
  await poll;
  kill(sampler.pid);
  const rows = fs.readFileSync(log, "utf8").trim().split(/\r?\n/).map((l) => l.split(" ").map(Number)).filter(([at, ws]) => at && ws);
  const failed = await page.eval(`return [...document.querySelectorAll("p[role=alert]")].map((p) => p.textContent).join(" | ");`);
  const after = renderers(await browser.send("SystemInfo.getProcessInfo"));
  const sizes = await page.eval(`return window.__kvBlobs.slice(-${Number(parts) || 0}).map((b) => b.size);`);
  console.log(JSON.stringify({
    name: opt.name, sample: path.basename(path.dirname(sample.file)), files, parts, seconds: took,
    partsMiB: Array.isArray(sizes) ? sizes.map(mib) : sizes, failed,
    peakMiB: mib(Math.max(...after.map((p) => p.peak))),
    beforeMiB: before.map((p) => mib(p.ws)), afterMiB: after.map((p) => mib(p.ws)),
    wasmBeforeMiB: wasmBefore && mib(wasmBefore), wasmAfterMiB: mib(await page.eval(WASM)),
    errors: [...new Set(page.errors)].slice(0, 5),
  }));
  console.log(`  рендерер, МиБ по секундам: ${seconds(rows, started, Date.now())}`);
  console.log(`  wasm, МиБ по секундам: ${seconds(wasmRows, started, Date.now())}`);
} finally {
  browser?.close();
  await srv.stop();
}
