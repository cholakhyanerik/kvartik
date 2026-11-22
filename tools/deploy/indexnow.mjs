// IndexNow (задача 118): после выкладки всем CI называет поисковикам
// протокола — Bing, Яндексу и другим — страницы, которые изменились, и они
// перечитывают их сразу, а не когда дойдёт очередь обхода.
//
//   node tools/deploy/indexnow.mjs changed <папка> <адрес сайта> <файл списка>
//   node tools/deploy/indexnow.mjs send <папка> <файл списка>
//
// `changed` — до выкладки всем: страницы карты сайта папки, которые живой
// сайт отдаёт не байт в байт с папкой или не отдаёт вовсе (новые), —
// в файл списка, адресами из карты. `send` — после выкладки и её проверки на
// своём домене: список одним запросом к `api.indexnow.org` с ключом из папки
// (`<ключ>.txt` в корне; его пишет сборка — `INDEXNOW_KEY` в
// `crates/sw/src/landing.rs`, — и CI берёт его отсюда, а не держит копией).
// Пустой список — запроса нет: выкладка, не тронувшая страниц (только
// программа), поисковикам ничего не говорит. Нет файла списка (`changed`
// не дошёл до конца) — вся карта: лишний раз перечитать не вредно, а
// пропустить изменённое — да.
//
// Протокол — indexnow.org, «Documentation» (сверено 2026-11-22): POST JSON
// с `host`, `key`, `keyLocation` и `urlList` (до 10 000 адресов), 200 —
// принято, 202 — принято, ключ ещё сверяется; остальное — отказ (400 —
// формат, 403 — ключ, 422 — адрес не того сайта, 429 — слишком часто).
// Принятые адреса участники протокола передают друг другу сами.
//
// Чужой адрес из CI — решение человека 2026-11-22 (CLAUDE.md, Архитектура:
// новый запрос наружу — его решение). Уходят только адреса наших страниц
// и ключ, который и так лежит на сайте; данных людей в запросе нет — их нет
// и на сайте. Выкладку шаг не держит: поисковики — чужой сервис
// (`continue-on-error` в `ci.yml`), а исход он пишет в сводку задания.
import fs from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";

export const ENDPOINT = "https://api.indexnow.org/indexnow";
const HTML_ACCEPT = "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8";
/// Ключ — 8–128 знаков 0–9 и a–f (шапка `INDEXNOW_KEY`).
const KEY_FILE = /^([0-9a-f]{8,128})\.txt$/;

/// Адреса страниц из карты сайта папки — как в карте.
export function pages(dir) {
  const map = fs.readFileSync(path.join(dir, "sitemap.xml"), "utf8");
  const out = [...map.matchAll(/<loc>([^<]+)<\/loc>/g)].map((m) => m[1]);
  if (out.length === 0) throw new Error(`${dir}/sitemap.xml: ни одного <loc>`);
  return out;
}

/// Файл страницы в папке: адрес папки сайта — её `index.html`.
function file(dir, loc) {
  const { pathname } = new URL(loc);
  if (!pathname.endsWith("/")) throw new Error(`${loc}: в карте сайта ждали адрес папки`);
  return path.join(dir, `${pathname.slice(1)}index.html`);
}

/// Ключ из папки: единственный файл `<ключ>.txt` в корне, в котором сам ключ.
export function indexKey(dir) {
  const keys = fs.readdirSync(dir)
    .map((name) => name.match(KEY_FILE)?.[1])
    .filter((key) => key && fs.readFileSync(path.join(dir, `${key}.txt`), "utf8") === key);
  if (keys.length !== 1) throw new Error(`${dir}: файлов ключа IndexNow ${keys.length}, ждали один`);
  return keys[0];
}

/// Страницы карты, которые сайт `site` отдаёт не так, как лежат в папке:
/// иначе, с другим кодом или никак. Адрес сайта заменяет начало адреса из
/// карты — так сверяется и предпросмотр.
export async function changedPages(dir, site) {
  const out = [];
  for (const loc of pages(dir)) {
    const { pathname } = new URL(loc);
    let same = false;
    try {
      const response = await fetch(`${site}${pathname}`, {
        headers: { Accept: HTML_ACCEPT },
        redirect: "manual",
        signal: AbortSignal.timeout(30_000),
      });
      const body = Buffer.from(await response.arrayBuffer());
      same = response.status === 200 && body.equals(fs.readFileSync(file(dir, loc)));
    } catch {
      same = false;
    }
    if (!same) out.push(loc);
  }
  return out;
}

/// Тело запроса: сайт, ключ и где он лежит, адреса.
export function request(dir, urls) {
  const key = indexKey(dir);
  const host = new URL(urls[0]).host;
  const foreign = urls.filter((url) => new URL(url).host !== host);
  if (foreign.length) throw new Error(`адреса не одного сайта: ${foreign.join(" ")}`);
  return { host, key, keyLocation: `https://${host}/${key}.txt`, urlList: urls };
}

function summary(line) {
  console.log(line);
  if (process.env.GITHUB_STEP_SUMMARY) fs.appendFileSync(process.env.GITHUB_STEP_SUMMARY, `${line}\n`);
}

async function main() {
  const [command, dir, ...rest] = process.argv.slice(2);
  if (command === "changed" && dir && rest.length === 2) {
    const [site, list] = rest;
    const changed = await changedPages(dir, site);
    fs.writeFileSync(list, changed.map((url) => `${url}\n`).join(""));
    console.log(`IndexNow: изменилось страниц ${changed.length} из ${pages(dir).length}`);
    return;
  }
  if (command === "send" && dir && rest.length === 1) {
    const [list] = rest;
    const urls = fs.existsSync(list)
      ? fs.readFileSync(list, "utf8").split("\n").filter(Boolean)
      : pages(dir);
    if (urls.length === 0) {
      summary("IndexNow: страницы не менялись — поисковикам говорить нечего");
      return;
    }
    const body = request(dir, urls);
    const response = await fetch(ENDPOINT, {
      method: "POST",
      headers: { "Content-Type": "application/json; charset=utf-8" },
      body: JSON.stringify(body),
      signal: AbortSignal.timeout(30_000),
    });
    const text = (await response.text()).slice(0, 500);
    if (response.status === 200 || response.status === 202) {
      summary(`IndexNow: ${response.status}, страниц ${urls.length} — ${urls.join(" ")}`);
      return;
    }
    summary(`IndexNow: отказ ${response.status} ${text}`);
    process.exit(1);
  }
  console.error("node tools/deploy/indexnow.mjs changed <папка> <адрес сайта> <файл списка>\n"
    + "node tools/deploy/indexnow.mjs send <папка> <файл списка>");
  process.exit(2);
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch((error) => {
    summary(`IndexNow: ${error.message}`);
    process.exit(1);
  });
}
