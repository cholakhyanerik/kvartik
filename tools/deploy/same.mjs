// Выкладка курсов по расписанию (`.github/workflows/rates.yml`, задача 43)
// меняет на сайте только файл курсов: всё остальное в её папке — байт
// в байт то, что сайт отдаёт сейчас.
//
//   node tools/deploy/same.mjs <адрес сайта> <папка> <папка корня в репозитории>
//
// Зачем. Папку выкладка курсов берёт из артефакта `site` прошлой выкладки,
// а не собирает: она не видит, кто и что положил в артефакт, и не должна
// этому верить (ревью задачи 43, Правило 13). Сверка с живым сайтом делает
// её безвредной: подменённая или отставшая папка — отказ, а не откат сайта
// к прежней сборке. Файлы, которые хостинг не отдаёт, сверяются иначе: файлы
// настроек хостинга в корне папки — `_headers`, а с задачи 106 ещё функция
// сайта `_worker.js` (с задачи 113 — только отдача wasm, задача 109) и её
// адреса `_routes.json` — байт в байт с файлами корня
// сайта в репозитории (`crates/app/public/root/`): подложенная в артефакт
// функция исполнялась бы у всех на сервере, а сверить её с живым сайтом
// нельзя — сам файл хостинг не отдаёт. Других `_…` и скрытых файлов в папке
// быть не должно, а файлы настроек репозитория в ней есть все.
//
// Сразу после выкладки край Cloudflare минуты отдаёт прежнюю
// (`.claude/rules/pitfalls/cloudflare.md`, «Хостинг отвечает успехом не тем»),
// поэтому несовпавшее повторяется до трёх минут, а провал — только устойчивый.
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
  const [site, dir, root] = process.argv.slice(2);
  if (!site || !dir || !root) {
    console.error("использование: same.mjs <адрес сайта> <папка> <папка корня в репозитории>");
    process.exit(2);
  }
  const base = site.replace(/\/$/, "");
  const all = files(dir);
  const settings = fs.readdirSync(root).filter((name) => name.startsWith("_"));
  const failures = [];
  for (const rel of all) {
    const name = rel.split("/").at(-1);
    if (settings.includes(rel)) {
      if (!fs.readFileSync(path.join(dir, rel)).equals(fs.readFileSync(path.join(root, rel)))) {
        failures.push(`${rel} — не тот, что в репозитории`);
      }
    } else if (name.startsWith("_") || name.startsWith(".")) {
      failures.push(`${rel} — лишний файл`);
    }
  }
  for (const name of ["_headers", ...settings]) {
    if (!all.includes(name)) failures.push(`нет ${name}`);
  }
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
