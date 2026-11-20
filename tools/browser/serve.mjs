// Серверы `dist/` для глазного прогона (skill browser-run) и проверки
// собранного сайта в CI (`check.mjs` рядом, задача 60).
//
// - `wrangler` — `wrangler pages dev` из `tools/deploy`: тот же разбор
//   `_headers` (CSP, кэш), `_redirects` и `404.html`, что у Cloudflare Pages
//   (проверено 2026-10-15 и 2026-10-16, wrangler 4.149.0). Для экранов,
//   работы без сети, новой версии и установки.
// - `own` — свой сервер Node со сжатием brotli заранее и теми же ответами:
//   папка — её `index.html` (лендинг в `/`, `/ru/`, `/en/` — задача 34),
//   `/app` — 308, неизвестный адрес — ближайший `404.html` с кодом 404.
//   Для замера первого захода: сжимать файл при
//   первом запросе нельзя — сжатие шрифтов держало шесть соединений HTTP/1.1,
//   и части wasm выглядели на 650 мс медленнее (задача 58, 2026-10-13).
//   Из `_headers` он ставит только правило `/*` — его Pages ставит на каждый
//   ответ, и CSP в нём: до задачи 102 прогоны шли вовсе без CSP, и запрос,
//   который CSP сайта не пустил бы, проверка видела ушедшим. Правила
//   с путями (кэш) он не ставит. Качество сжатия — параметр: замеру
//   нужны байты хостинга (`HOSTING_QUALITY`, по умолчанию), а проверке CI —
//   только сжатие, и 5 сжимает его за доли секунды.
//
// И сайт, который им отдать новой версией, — `newVersion` (задача 67):
// проверке собранного сайта и сценарию `update` skill browser-run.

import http from "node:http";
import zlib from "node:zlib";
import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { kill, sleep, spawnTree } from "./cdp.mjs";

export const REPO = path.resolve(import.meta.dirname, "../..");
const WRANGLER = path.join(REPO, "tools/deploy/node_modules/wrangler/bin/wrangler.js");

/// Файлы сайта, которые кладёт выкладка, а не сборка, — адрес → байты: файл
/// курсов ЦБ (задача 43, `tools/deploy/rates.mjs`) образцом из тестов ядра.
/// Оба сервера отдают их по умолчанию, как сайт: окно спрашивает файл курсов
/// при каждом открытии, и без него каждый прогон skill ловил 404 в консоли
/// и краснел (найдено глазным прогоном задачи 44).
export const DEPLOYED = {
  "/app/rates.json": fs.readFileSync(path.join(REPO, "crates/core/tests/rates/rates.json")),
};

const TYPES = {
  ".html": "text/html; charset=utf-8", ".js": "text/javascript", ".wasm": "application/wasm", ".css": "text/css",
  ".ttf": "font/ttf", ".png": "image/png", ".webmanifest": "application/manifest+json", ".json": "application/json",
  ".pdf": "application/pdf", ".svg": "image/svg+xml", ".ics": "text/calendar; charset=utf-8",
  // Как Cloudflare Pages: `text/plain; charset=utf-8` (tools/deploy/check.sh).
  ".txt": "text/plain; charset=utf-8",
  // Карта сайта лендинга (задача 34).
  ".xml": "application/xml",
};

/// Уровень brotli, которым сжимает хостинг (задача 109). Каждый файл,
/// который отдал kvartik.com 2026-11-18 (узел EVN, 0.0.153), по проводу
/// ближе всего к уровню 4: и wasm мимо кэша CDN (`cf-cache-status:
/// DYNAMIC`), и CSS и JS из него (`HIT`). Главный wasm — 281 260 Б,
/// уровнем 4 — 279 108, уровнем 11 — 232 277; всё, что ставит service
/// worker, — около четверти сверх уровня 11. Выбрать уровень у Cloudflare
/// нельзя, а заранее сжатых файлов Pages не отдаёт (docs Cloudflare,
/// «Compression» и «Compression Rules», 2026-11-18). До задачи 109 замер
/// сжимал уровнем 11 и занижал первый заход телефона: ~49 КБ главного wasm,
/// рабочий экран 2,84 с вместо 3,12. С неё wasm программы сайт отдаёт копией
/// уровнем 11, которую кладёт сборка, — `own` тоже, а остальное — этим.
export const HOSTING_QUALITY = 4;

/// Заголовки правила `/*` из `_headers` сборки — CSP, COOP, Referrer-Policy,
/// nosniff: Pages ставит их на каждый ответ сайта (шапка `_headers`). Нет
/// файла — заголовков нет.
function everywhere(dist) {
  const file = path.join(dist, "_headers");
  if (!fs.existsSync(file)) return {};
  const out = {};
  let inside = false;
  for (const line of fs.readFileSync(file, "utf8").split(/\r?\n/)) {
    if (/^\S/.test(line)) {
      inside = line.trim() === "/*";
      continue;
    }
    const header = inside && line.match(/^\s+([A-Za-z-]+):\s*(.+)$/);
    if (header) out[header[1]] = header[2].trim();
  }
  return out;
}

/// Свой сервер. Возвращает `{ port, stop() }`. `extra` — файлы, которых нет
/// в сборке, адрес → байты, по умолчанию — файлы выкладки (`DEPLOYED`):
/// свой файл `dist/` важнее — копия сайта может нести другой. `dist/` он не
/// трогает.
export function own(dist, port, { quality = HOSTING_QUALITY, extra = DEPLOYED } = {}) {
  const site = everywhere(dist);
  const files = new Map();
  const walk = (dir, prefix) => {
    for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
      // `_headers` и `_redirects` хостинг не отдаёт, `.`-файлы — тоже.
      if (e.name.startsWith(".") || e.name.startsWith("_")) continue;
      const full = path.join(dir, e.name);
      if (e.isDirectory()) { walk(full, `${prefix}${e.name}/`); continue; }
      const raw = fs.readFileSync(full);
      files.set(`/${prefix}${e.name}`, {
        raw, br: zlib.brotliCompressSync(raw, { params: { [zlib.constants.BROTLI_PARAM_QUALITY]: quality } }),
        type: TYPES[path.extname(e.name)] ?? "application/octet-stream",
      });
    }
  };
  walk(dist, "");
  // Wasm программы сайт отдаёт его копией уровнем 11 (`*.wasm.br`, задача
  // 109: функция сайта `_worker.js`), а не сжатием хостинга, — так же и здесь.
  for (const [url, file] of files) {
    const copy = files.get(`${url}.br`);
    if (url.endsWith(".wasm") && copy) file.br = copy.raw;
  }
  for (const [url, body] of Object.entries(extra)) {
    if (files.has(url)) continue;
    const raw = Buffer.from(body);
    files.set(url, {
      raw, br: zlib.brotliCompressSync(raw, { params: { [zlib.constants.BROTLI_PARAM_QUALITY]: quality } }),
      type: TYPES[path.extname(url)] ?? "application/octet-stream",
    });
  }
  const notFound = (p) => {
    for (let dir = path.posix.dirname(p); ; dir = path.posix.dirname(dir)) {
      const f = files.get(`${dir === "/" ? "" : dir}/404.html`);
      if (f || dir === "/") return f;
    }
  };
  const server = http.createServer((req, res) => {
    const url = new URL(req.url, "http://x");
    let p = decodeURIComponent(url.pathname);
    if (p === "/app") { res.writeHead(308, { Location: `/app/${url.search}` }); return res.end(); }
    if (p.endsWith("/")) p += "index.html";
    const found = files.get(p);
    const f = found ?? notFound(p);
    if (!f) { res.writeHead(404); return res.end(); }
    const br = (req.headers["accept-encoding"] ?? "").includes("br");
    res.writeHead(found ? 200 : 404, {
      ...site, "Content-Type": f.type, "Cache-Control": "no-cache", ...(br ? { "Content-Encoding": "br" } : {}),
    });
    res.end(br ? f.br : f.raw);
  }).listen(port, "127.0.0.1");
  // `server.close` ждёт, пока закроются все соединения, а браузер держит их
  // живыми и шлёт по ним новые запросы: проверка могла стоять на остановке
  // сервера без конца (CI, 2026-11-15). Поэтому соединения закрываются сразу.
  const stop = () => new Promise((r) => {
    server.close(r);
    server.closeAllConnections();
  });
  return { port, stop };
}

/// `wrangler pages dev`. Он кладёт `.wrangler/` в текущую папку — поэтому
/// идёт из `work`, а не из рабочей копии. Отдаёт он копию сайта в `work` —
/// сборку и файлы выкладки (`DEPLOYED`), которых в ней нет, а `dist` не
/// трогает; копия у каждого запуска своя: папку прежнего Windows может
/// держать ещё после остановки.
/// Возвращает `{ port, stop() }` после первого ответа 200 на `/app/`.
export async function wrangler(dist, port, work) {
  if (!fs.existsSync(WRANGLER)) {
    throw new Error(`нет ${WRANGLER}: поставьте wrangler из замка — npm ci --ignore-scripts в tools/deploy`);
  }
  fs.mkdirSync(work, { recursive: true });
  const site = fs.mkdtempSync(path.join(work, `site-${port}-`));
  fs.cpSync(dist, site, { recursive: true });
  for (const [url, body] of Object.entries(DEPLOYED)) {
    const file = path.join(site, url);
    if (!fs.existsSync(file)) fs.writeFileSync(file, body);
  }
  const log = fs.openSync(path.join(work, `wrangler-${port}.log`), "a");
  // Под `CLAUDECODE=1` wrangler считает себя в руках агента (CLAUDE.md,
  // Правило 3) — сервер прогона так не делаем.
  const env = { ...process.env, WRANGLER_SEND_METRICS: "false" };
  delete env.CLAUDECODE;
  // Функция сайта (`_worker.js` сборки, задача 109) исполняется и здесь,
  // а дата рантайма — та же, что у выкладки (`tools/deploy/wrangler.toml`).
  const child = spawnTree(process.execPath, [WRANGLER, "pages", "dev", path.resolve(site), "--port", String(port),
    "--compatibility-date", "2026-10-06"],
    { cwd: work, env, stdio: ["ignore", log, log] });
  const stop = async () => {
    // У wrangler дочерние workerd: TaskStop оставил бы их держать порт.
    kill(child.pid);
    for (let i = 0; i < 50; i++) {
      try { await fetch(`http://127.0.0.1:${port}/app/`); } catch { return; }
      await sleep(200);
    }
    throw new Error(`сервер на порту ${port} не погас`);
  };
  for (let i = 0; i < 150; i++) {
    try { if ((await fetch(`http://127.0.0.1:${port}/app/`)).status === 200) return { port, stop }; } catch {}
    await sleep(400);
  }
  await stop().catch(() => {});
  throw new Error(`wrangler на порту ${port} не ответил за минуту — лог: ${path.join(work, `wrangler-${port}.log`)}`);
}

export function start(kind, dist, port, work) {
  return kind === "own" ? own(dist, port) : wrangler(dist, port, work);
}

/// Хэш сборки из `sw.js` программы.
export function build(dist) {
  return /const BUILD = "(\w+)"/.exec(fs.readFileSync(path.join(dist, "app/sw.js"), "utf8"))[1];
}

/// Копия сайта `dist` новой версией — в папке `dir` (задача 67): сверху
/// заметок «Что нового» — версия на одну новее, словами последней, файл
/// корня правлен, а `sw.js` программы переписан `kvartik-sw`, как его пишет
/// сборка. Новой версией считается и правка файла корня: хэш сборки — по
/// всему сайту. Возвращает `{ dir, version, build }`.
export function newVersion(dist, dir) {
  fs.rmSync(dir, { recursive: true, force: true });
  fs.cpSync(dist, dir, { recursive: true });
  const file = path.join(dir, "app/notes.json");
  const notes = JSON.parse(fs.readFileSync(file, "utf8"));
  const [major, minor, patch] = notes.notes[0].version.split(".").map(Number);
  const version = `${major}.${minor}.${patch + 1}`;
  notes.notes.unshift({ ...notes.notes[0], version });
  fs.writeFileSync(file, JSON.stringify(notes, null, 2));
  fs.appendFileSync(path.join(dir, "404.html"), "\n<!-- новая версия для прогона -->\n");
  const sw = spawnSync("cargo", ["run", "-q", "-p", "kvartik-sw", "--", "public/sw.js"], {
    cwd: path.join(REPO, "crates/app"), encoding: "utf8",
    env: { ...process.env, TRUNK_STAGING_DIR: dir, TRUNK_PUBLIC_URL: "/app/" },
  });
  if (sw.status !== 0) throw new Error(`kvartik-sw: ${sw.stderr || sw.error}`);
  return { dir, version, build: build(dir) };
}
