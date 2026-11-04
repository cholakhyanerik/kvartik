// Серверы `dist/` для глазного прогона (skill browser-run) и проверки
// собранного сайта в CI (`check.mjs` рядом, задача 60).
//
// - `wrangler` — `wrangler pages dev` из `tools/deploy`: тот же разбор
//   `_headers` (CSP, кэш), `_redirects` и `404.html`, что у Cloudflare Pages
//   (проверено 2026-10-15 и 2026-10-16, wrangler 4.149.0). Для экранов,
//   работы без сети, новой версии и установки.
// - `own` — свой сервер Node со сжатием brotli заранее и теми же ответами:
//   `/` — 302 на `/app/`, `/app` — 308, неизвестный адрес — ближайший
//   `404.html` с кодом 404. Для замера первого захода: сжимать файл при
//   первом запросе нельзя — сжатие шрифтов держало шесть соединений HTTP/1.1,
//   и части wasm выглядели на 650 мс медленнее (задача 58, 2026-10-13).
//   Заголовков `_headers` он не ставит. Качество сжатия — параметр: замеру
//   нужны байты, близкие к хостингу (11, весь сайт — 3,5 с на машине
//   автора), а проверке CI — только сжатие, и 5 сжимает его за доли секунды.
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

const TYPES = {
  ".html": "text/html; charset=utf-8", ".js": "text/javascript", ".wasm": "application/wasm", ".css": "text/css",
  ".ttf": "font/ttf", ".png": "image/png", ".webmanifest": "application/manifest+json", ".json": "application/json",
  ".pdf": "application/pdf", ".svg": "image/svg+xml", ".ics": "text/calendar; charset=utf-8",
  // Как Cloudflare Pages: `text/plain; charset=utf-8` (tools/deploy/check.sh).
  ".txt": "text/plain; charset=utf-8",
};

/// Свой сервер. Возвращает `{ port, stop() }`.
export function own(dist, port, { quality = 11 } = {}) {
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
  const notFound = (p) => {
    for (let dir = path.posix.dirname(p); ; dir = path.posix.dirname(dir)) {
      const f = files.get(`${dir === "/" ? "" : dir}/404.html`);
      if (f || dir === "/") return f;
    }
  };
  const server = http.createServer((req, res) => {
    const url = new URL(req.url, "http://x");
    let p = decodeURIComponent(url.pathname);
    if (p === "/") { res.writeHead(302, { Location: `/app/${url.search}` }); return res.end(); }
    if (p === "/app") { res.writeHead(308, { Location: `/app/${url.search}` }); return res.end(); }
    if (p.endsWith("/")) p += "index.html";
    const found = files.get(p);
    const f = found ?? notFound(p);
    if (!f) { res.writeHead(404); return res.end(); }
    const br = (req.headers["accept-encoding"] ?? "").includes("br");
    res.writeHead(found ? 200 : 404, {
      "Content-Type": f.type, "Cache-Control": "no-cache", ...(br ? { "Content-Encoding": "br" } : {}),
    });
    res.end(br ? f.br : f.raw);
  }).listen(port, "127.0.0.1");
  return { port, stop: () => new Promise((r) => server.close(r)) };
}

/// `wrangler pages dev`. Он кладёт `.wrangler/` в текущую папку — поэтому
/// идёт из `work`, а не из рабочей копии. Возвращает `{ port, stop() }`
/// после первого ответа 200 на `/app/`.
export async function wrangler(dist, port, work) {
  if (!fs.existsSync(WRANGLER)) {
    throw new Error(`нет ${WRANGLER}: поставьте wrangler из замка — npm ci --ignore-scripts в tools/deploy`);
  }
  fs.mkdirSync(work, { recursive: true });
  const log = fs.openSync(path.join(work, `wrangler-${port}.log`), "a");
  // Под `CLAUDECODE=1` wrangler считает себя в руках агента (CLAUDE.md,
  // Правило 3) — сервер прогона так не делаем.
  const env = { ...process.env, WRANGLER_SEND_METRICS: "false" };
  delete env.CLAUDECODE;
  const child = spawnTree(process.execPath, [WRANGLER, "pages", "dev", path.resolve(dist), "--port", String(port)],
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
