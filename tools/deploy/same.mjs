// Выкладка курсов по расписанию (`.github/workflows/rates.yml`, задача 43)
// меняет на сайте только файл курсов: всё остальное в её папке — байт
// в байт то, что сайт отдаёт сейчас.
//
//   node tools/deploy/same.mjs <адрес сайта> <папка> <_headers репозитория>
//
// Зачем. Папку выкладка курсов берёт из артефакта `site` прошлой выкладки,
// а не собирает: она не видит, кто и что положил в артефакт, и не должна
// этому верить (ревью задачи 43, Правило 13). Сверка с живым сайтом делает
// её безвредной: подменённая или отставшая папка — отказ, а не откат сайта
// к прежней сборке. Файлы, которые хостинг не отдаёт, сверяются иначе:
// `_headers` — с файлом репозитория, других `_…` и скрытых файлов в папке
// быть не должно.
//
// Сразу после выкладки край Cloudflare минуты отдаёт прежнюю (CLAUDE.md,
// Правило 6, «Хостинг отвечает успехом не тем»), поэтому несовпавшее
// повторяется до трёх минут, а провал — только устойчивый.
import fs from "node:fs";
import path from "node:path";

const RATES = "app/rates.json";
const HTML_ACCEPT = "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8";
const SETTLE_MS = 180_000;

/// Файлы папки — путями от её корня через `/`.
function files(dir, prefix = "") {
  const out = [];
  for (const entry of fs.readdirSync(path.join(dir, prefix), { withFileTypes: true })) {
    const rel = prefix ? `${prefix}/${entry.name}` : entry.name;
    if (entry.isDirectory()) out.push(...files(dir, rel));
    else out.push(rel);
  }
  return out;
}

/// Адрес файла на сайте и чего ждать: страница папки — адрес папки,
/// `404.html` — ответ 404 на неизвестный адрес (на `/404.html` хостинг
/// отвечает переадресацией).
function address(site, rel) {
  if (rel === "404.html") return { url: `${site}/kvartik-no-such-file-0000000000000000.js`, status: 404, html: true };
  if (rel === "index.html") return { url: `${site}/`, status: 200, html: true };
  if (rel.endsWith("/index.html")) return { url: `${site}/${rel.slice(0, -"index.html".length)}`, status: 200, html: true };
  return { url: `${site}/${rel}`, status: 200, html: rel.endsWith(".html") };
}

async function same(site, dir, rel) {
  const { url, status, html } = address(site, rel);
  try {
    const response = await fetch(url, {
      headers: { Accept: html ? HTML_ACCEPT : "*/*" },
      redirect: "manual",
      signal: AbortSignal.timeout(30_000),
    });
    const body = Buffer.from(await response.arrayBuffer());
    return response.status === status && body.equals(fs.readFileSync(path.join(dir, rel)));
  } catch {
    return false;
  }
}

async function main() {
  const [site, dir, headers] = process.argv.slice(2);
  if (!site || !dir || !headers) {
    console.error("использование: same.mjs <адрес сайта> <папка> <_headers репозитория>");
    process.exit(2);
  }
  const base = site.replace(/\/$/, "");
  const all = files(dir);
  const failures = [];
  for (const rel of all) {
    const name = rel.split("/").at(-1);
    if (rel === "_headers") {
      if (!fs.readFileSync(path.join(dir, rel)).equals(fs.readFileSync(headers))) {
        failures.push("_headers — не тот, что в репозитории");
      }
    } else if (name.startsWith("_") || name.startsWith(".")) {
      failures.push(`${rel} — лишний файл`);
    }
  }
  if (!all.includes("_headers")) failures.push("нет _headers");
  let left = all.filter((rel) => rel !== RATES && !rel.split("/").at(-1).startsWith("_")
    && !rel.split("/").at(-1).startsWith("."));
  const checked = left.length;
  const until = Date.now() + SETTLE_MS;
  for (;;) {
    const next = [];
    for (const rel of left) {
      if (!(await same(base, dir, rel))) next.push(rel);
    }
    left = next;
    if (left.length === 0 || Date.now() > until) break;
    await new Promise((resolve) => setTimeout(resolve, 5000));
  }
  failures.push(...left.map((rel) => `${rel} — на сайте другой`));
  if (failures.length > 0) {
    console.error(`Папка ${dir} — не сайт ${base}: ${failures.length}`);
    for (const failure of failures.slice(0, 20)) console.error(`  ${failure}`);
    process.exit(1);
  }
  console.error(`Папка ${dir} — сайт ${base}: ${checked} файлов байт в байт, кроме файла курсов`);
}

await main();
