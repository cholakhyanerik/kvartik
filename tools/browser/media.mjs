// Снимки и видео программы для лендинга (задача 119): demo-данные, четыре
// экрана — главный, карточка счёта, книга, налог — на 360 и 1280 и ролик
// «новый счёт → выпустить → PDF» на 360, на каждом из трёх языков.
//
//   node tools/browser/media.mjs [--dist crates/app/dist] [--work C:/tmp/k119]
//                                 [--out crates/app/site/media] [--langs hy,ru,en]
//
// Данные — пример: ИП «Ани Оринакян» с учебным ИНН 12345678, клиенты
// «Example Studio Ltd», «Օրինակ» ՍՊԸ и ООО «Пример», банк «Example Bank»
// с нулевыми счетами — ни одного настоящего лица и номера (задача 119;
// CLAUDE.md, Правило 2, «Образец назван образцом»: лендинг подписывает их
// примером). Записи кладутся прямо в базу программы (`records`, ключ
// `[вид, id]`, значение — JSON с `format_version`), как их пишет окно;
// закрывает III квартал само окно — кнопкой на экране налога.
//
// Выпуск без ключа открыт пробным периодом, начатым до `LAST_TRIAL_START`
// (`kvartik.trial` = 2026-11-21, `license::trial_last`): он идёт до
// 2026-12-04, и позже этот сценарий выпустить счёт не сможет — тогда нужен
// пробный ключ продавца на ИНН 12345678 (поле `--key`).
//
// Снимки — WebP из Chrome (`Page.captureScreenshot`), ролик — кадрами
// `Page.startScreencast` по их времени, собранными ffmpeg в WebM (VP9)
// и MP4 (H.264) с постером WebP. ffmpeg — из PATH (на машине автора —
// пакет winget Gyan.FFmpeg); в CI сценария нет: снимки лежат в репозитории.

import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

import { Browser, sleep } from "./cdp.mjs";
import { DEPLOYED, REPO, own } from "./serve.mjs";
import { t } from "./strings.mjs";

const args = process.argv.slice(2);
const opt = (name, fallback) => {
  const at = args.indexOf(`--${name}`);
  return at >= 0 ? args[at + 1] : fallback;
};
const DIST = path.resolve(REPO, opt("dist", "crates/app/dist"));
const WORK = opt("work", "C:/tmp/k119");
const OUT = path.resolve(REPO, opt("out", "crates/app/site/media"));
const LANGS = opt("langs", "hy,ru,en").split(",");
const KEY = opt("key", null);
// Курсы ЦБ — файл своего сайта, как у программы (`tax::cba`): настоящие,
// по дню публикации. Нужны и подсказке в ролике, и курсам demo-документов.
const RATES = opt("rates", "https://kvartik.com/app/rates.json");
const PORT = 8790;
const ORIGIN = `http://127.0.0.1:${PORT}`;
const FORMAT = 26;
const VERSION = fs.readFileSync(path.join(REPO, "Cargo.toml"), "utf8").match(/^version = "([^"]+)"/m)[1];

// ——— Данные ———

const id = (n) => `d3a0c0de-0119-4000-8000-${String(n).padStart(12, "0")}`;
const AT = Date.UTC(2026, 10, 21, 8, 0, 0);
let tick = 0;
const meta = (n) => ({ id: id(n), modified: AT + (tick++) * 60_000, deleted: null });

const SELLER = {
  name: { hy: "ԱՁ Անի Օրինակյան", ru: "ИП Ани Оринакян", en: "IE Ani Orinakyan" },
  tin: "12345678",
  registration: "290.0000000",
  address: {
    hy: "ք. Երևան, Օրինակի փ. 1",
    ru: "г. Ереван, ул. Примерная 1",
    en: "1 Example St\nYerevan 0001, Armenia",
  },
  phone: "+374 10 000000",
  email: "ani@example.com",
  accounts: [
    { currency: "AMD", number: "1600000000000000", bank: "«Օրինակ բանկ» ՓԲԸ", bank_latin: "Example Bank CJSC", swift: null, correspondent: "" },
    { currency: "USD", number: "1600000000000001", bank: "«Օրինակ բանկ» ՓԲԸ", bank_latin: "Example Bank CJSC", swift: "EXMPAM22", correspondent: "" },
  ],
};

const CLIENTS = {
  en: { n: 2, data: { name: "Example Studio Ltd", country: "GB", address: "1 Example Road\nLondon EC1A 1AA", tin: null, tax_id: "GB000000000", currency: "USD", lang: "en", payment_days: 14 } },
  hy: { n: 3, data: { name: "«Օրինակ» ՍՊԸ", country: "AM", address: "ք. Երևան, Օրինակի փ. 10", tin: "87654321", tax_id: "", currency: "AMD", lang: "hy", payment_days: 10 } },
  ru: { n: 4, data: { name: "ООО «Пример»", country: "RU", address: "г. Москва, ул. Примерная, д. 1", tin: null, tax_id: "ИНН 0000000000", currency: "USD", lang: "ru", payment_days: 14 } },
};

let usd = [];
/// Курс доллара для дня услуги — опубликованный в последний день публикации
/// до него (НК 16(2)(1) с 2026-07-01: предыдущий рабочий день).
function rate(service) {
  const found = usd.filter(([day]) => day < service).at(-1);
  if (!found) throw new Error(`в курсах ЦБ нет дня до ${service}`);
  return { currency: "USD", per_unit_e4: found[1], date: found[0], source: "manual" };
}

/// Выпущенный документ: `n` — номер записи, `seq` — номер в серии.
function issued(n, { kind = "invoice", seq, client, date, service, due, items, paid = [], signed = null }) {
  const c = CLIENTS[client];
  const dollars = c.data.currency === "USD";
  return {
    kind: "document",
    record: {
      meta: meta(n),
      data: {
        kind, client: id(c.n), lang: c.data.lang, currency: c.data.currency, date, service_date: service, due,
        items, basis: null, recurring: null, rate: dollars ? rate(service) : null, activity: null, income_kind: null,
      },
      issued: { number: { year: 2026, seq }, seller: SELLER, buyer: c.data, logo: null },
      annulled: null,
      signed,
      settlement: { day: service, number: `Ա ${String(seq).padStart(10, "0")}` },
      payments: paid,
    },
  };
}

const item = (text, price, quantity = 1000) => ({ text, quantity_e3: quantity, price });

/// Расходы книги пишет сам ИП — на языке страницы, как писал бы он.
const EXPENSES = {
  hy: { rent: "Աշխատատեղի վարձ", months: ["հոկտեմբեր", "նոյեմբեր"] },
  ru: { rent: "Аренда рабочего места", months: ["октябрь", "ноябрь"] },
  en: { rent: "Desk rent", months: ["October", "November"] },
};

function records(lang) {
  tick = 0;
  const out = [];
  out.push({ kind: "profile", record: { meta: meta(1), data: {
    ...SELLER,
    activities: ["62.01.0"], kinds: {}, regime: "turnover", micro: [],
    person: { birth_year: 1990, pension_joined: null, pension_applied: null, residency: "citizen", resident: true,
      registered: "2025-02-03", turnover_applied: "2025-02-05", stops: [] },
  } } });
  for (const c of Object.values(CLIENTS)) out.push({ kind: "client", record: { meta: meta(c.n), data: c.data } });
  out.push(issued(10, { seq: 1, client: "en", date: "2026-07-31", service: "2026-07-31", due: "2026-08-14",
    items: [item("Web app development, July 2026", 240_000)], paid: [{ date: "2026-08-11", amount: 240_000 }] }));
  out.push(issued(11, { seq: 2, client: "en", date: "2026-08-31", service: "2026-08-31", due: "2026-09-14",
    items: [item("Web app development, August 2026", 240_000)], paid: [{ date: "2026-09-08", amount: 240_000 }] }));
  out.push(issued(12, { kind: "act", seq: 1, client: "hy", date: "2026-09-20", service: "2026-09-20", due: null,
    items: [item("Ծրագրային ապահովման մշակում", 450_000)], signed: "2026-09-22" }));
  out.push(issued(13, { seq: 3, client: "ru", date: "2026-09-30", service: "2026-09-30", due: "2026-10-14",
    items: [item("Разработка мобильного приложения, сентябрь 2026", 180_000)], paid: [{ date: "2026-10-09", amount: 180_000 }] }));
  out.push(issued(14, { seq: 4, client: "en", date: "2026-10-31", service: "2026-10-31", due: "2026-11-14",
    items: [item("Web app development, October 2026", 260_000), item("Code review", 4_000, 6_000)],
    paid: [{ date: "2026-11-10", amount: 284_000 }] }));
  out.push(issued(15, { seq: 5, client: "hy", date: "2026-11-16", service: "2026-11-16", due: "2026-11-26",
    items: [item("Կայքի տեխնիկական սպասարկում, նոյեմբեր", 150_000)] }));
  out.push(issued(16, { seq: 6, client: "ru", date: "2026-11-20", service: "2026-11-20", due: "2026-12-04",
    items: [item("Разработка мобильного приложения, ноябрь 2026", 180_000)] }));
  const expense = (n, date, minor, text, supplier, proof, cost) => ({ kind: "operation", record: { meta: meta(n), data: {
    flow: "expense", date, paid: null, amount: { minor, currency: "AMD" }, rate: null, text, client: null, supplier, proof,
    nature: "ordinary", cost, activity: null, income_kind: null, attachments: [],
  } } });
  const { rent, months: [october, november] } = EXPENSES[lang];
  out.push(expense(30, "2026-10-05", 60_000, `${rent}, ${october}`, "«Օրինակ» ՍՊԸ", "settlement_invoice", "overhead"));
  out.push(expense(31, "2026-10-17", 7_200, `Figma Professional, ${october}`, "Figma, Inc.", "foreign_document", "overhead"));
  out.push(expense(32, "2026-11-05", 60_000, `${rent}, ${november}`, "«Օրինակ» ՍՊԸ", "settlement_invoice", "overhead"));
  out.push(expense(33, "2026-11-17", 7_200, `Figma Professional, ${november}`, "Figma, Inc.", "foreign_document", "overhead"));
  return out.map(({ kind, record }) => ({ key: [kind, record.meta.id], value: JSON.stringify({ format_version: FORMAT, record }) }));
}

// ——— Окно ———

const STORED = ["hy", "ru", "en"].map((lang) => t("StorageTitle", lang));
const READY = `return [...document.querySelectorAll("#backup h2")].some((h) => ${JSON.stringify(STORED)}.includes(h.textContent)) ? true : null;`;

async function must(page, code, what, ms = 20000) {
  const v = await page.waitFor(code, ms);
  if (!v) throw new Error(`не дождались: ${what}`);
  return v;
}

/// Нажать видимое по тексту — с кружком касания на месте нажатия: в ролике
/// видно, куда нажали. Кружок рисует страница (CSSOM, как точка мыши окна,
/// CLAUDE.md, Правило 6), в программе его нет.
async function tap(page, text, { contains = false } = {}) {
  const r = await page.eval(`const want = ${JSON.stringify(text)};
    const el = [...document.querySelectorAll("button, a, [role=button], summary")].find((e) =>
      (${contains ? "e.textContent.includes(want)" : "e.textContent.trim() === want"} || e.getAttribute("aria-label") === want)
      && e.getClientRects().length > 0);
    if (!el) return { error: "нет видимой кнопки «" + want + "»" };
    el.scrollIntoView({ block: "nearest" });
    const r = el.getBoundingClientRect();
    const dot = document.createElement("div");
    for (const [k, v] of Object.entries({ position: "fixed", left: (r.x + r.width / 2 - 24) + "px",
      top: (r.y + r.height / 2 - 24) + "px", width: "48px", height: "48px", "border-radius": "50%",
      background: "var(--accent)", opacity: "0.4", "pointer-events": "none", "z-index": "2147483647" })) dot.style.setProperty(k, v);
    document.body.append(dot);
    setTimeout(() => { dot.remove(); el.click(); }, 380);
    return true;`);
  if (r !== true) throw new Error(r?.error ?? `не нажалось «${text}»`);
  await sleep(450);
}

/// Ввести по буквам в поле с подписью `label` — как набирает человек.
async function typeSlowly(page, label, text, ms = 55) {
  const field = `[...document.querySelectorAll("label")].filter((l) => l.getClientRects().length > 0)
    .find((l) => (l.firstChild?.textContent ?? "").trim() === ${JSON.stringify(label)})?.querySelector("input, textarea")`;
  const r = await page.eval(`const el = ${field}; if (!el) return { error: "нет поля" };
    el.scrollIntoView({ block: "center" }); el.focus(); el.value = ""; el.dispatchEvent(new Event("input", { bubbles: true })); return true;`);
  if (r !== true) throw new Error(`«${label}»: ${r?.error}`);
  await sleep(300);
  for (let i = 1; i <= text.length; i++) {
    await page.eval(`const el = ${field}; el.value = ${JSON.stringify(text)}.slice(0, ${i});
      el.dispatchEvent(new Event("input", { bubbles: true })); return true;`);
    await sleep(ms);
  }
  await page.eval(`const el = ${field}; el.dispatchEvent(new Event("change", { bubbles: true })); el.blur(); return true;`);
}

/// Программа с demo-данными на языке `lang` в своём контексте браузера.
async function open(browser, lang) {
  const page = await browser.page({ context: true });
  await page.go(`${ORIGIN}/app/`);
  await page.eval(`localStorage.setItem("kvartik.lang", ${JSON.stringify(lang)});
    localStorage.setItem("kvartik.trial", "2026-11-21");
    localStorage.setItem("kvartik.last_backup", String(Date.now()));
    localStorage.setItem("kvartik.seen_version", ${JSON.stringify(VERSION)});
    return true;`);
  await page.reload();
  await must(page, READY, "рабочий экран", 60000);
  const put = await page.eval(`const rows = ${JSON.stringify(records(lang))};
    const db = await new Promise((ok, no) => { const r = indexedDB.open("kvartik"); r.onsuccess = () => ok(r.result); r.onerror = () => no(r.error); });
    await new Promise((ok, no) => {
      const tx = db.transaction("records", "readwrite", { durability: "strict" });
      for (const row of rows) tx.objectStore("records").put(row.value, row.key);
      tx.oncomplete = ok; tx.onerror = () => no(tx.error);
    });
    db.close(); return rows.length;`);
  if (typeof put !== "number") throw new Error(`записи не легли: ${JSON.stringify(put)}`);
  await page.reload();
  await must(page, READY, "рабочий экран с данными", 60000);
  return page;
}

async function emulate(page, width) {
  await page.emulate(width, "light");
  await page.send("Emulation.setEmulatedMedia", { features: [
    { name: "prefers-color-scheme", value: "light" }, { name: "prefers-reduced-motion", value: "reduce" }] });
}

/// Снимок окна — того, что видно, как его увидел бы человек, а не всей
/// страницы: на лендинге он стоит рамкой экрана.
async function shot(page, file) {
  await page.send("Page.bringToFront");
  await page.eval(`await document.fonts.ready; window.scrollTo(0, 0); return true;`);
  await sleep(600);
  const { data } = await page.send("Page.captureScreenshot", { format: "webp", quality: 82 });
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, Buffer.from(data, "base64"));
  console.log("  снимок", path.relative(REPO, file), fs.statSync(file).size, "Б");
}

async function go(page, hash, ready) {
  await page.eval(`location.hash = ${JSON.stringify(hash)}; return true;`);
  if (ready) await must(page, ready, hash);
  await sleep(500);
}

const hasText = (text) => `return document.querySelector("main") && [...document.querySelectorAll("h1, h2, h3, p, li, span, strong")]
  .some((e) => e.getClientRects().length > 0 && e.textContent.includes(${JSON.stringify(text)})) ? true : null;`;

async function closeQuarter(page, lang) {
  await go(page, "#/tax");
  const close = t("TaxClose", lang);
  await must(page, `return [...document.querySelectorAll("button")].some((b) => b.textContent.trim() === ${JSON.stringify(close)} && b.getClientRects().length > 0) ? true : null;`, "кнопка «Закрыть квартал»", 30000);
  await page.click(close);
  await must(page, hasText(t("TaxClosed", lang).split("{}")[0].trim()), "квартал закрыт", 20000);
}

async function shots(page, lang) {
  for (const width of [360, 1280]) {
    await emulate(page, width);
    await go(page, "#/", READY);
    await sleep(1500);
    await shot(page, path.join(OUT, `home-${lang}-${width}.webp`));
    const [n, number] = { hy: [15, "2026-005"], ru: [16, "2026-006"], en: [14, "2026-004"] }[lang];
    await go(page, `#/documents/${id(n)}`, hasText(number));
    await shot(page, path.join(OUT, `invoice-${lang}-${width}.webp`));
    await go(page, "#/ledger");
    await sleep(1500);
    await shot(page, path.join(OUT, `ledger-${lang}-${width}.webp`));
    await go(page, "#/tax");
    await sleep(1500);
    await shot(page, path.join(OUT, `tax-${lang}-${width}.webp`));
  }
}

/// Что набирает ролик: клиент своего языка, услуга и цена.
const DEMO = {
  hy: { text: "Կայքի տեխնիկական սպասարկում, դեկտեմբեր", price: "150000" },
  ru: { text: "Доработка мобильного приложения", price: "900" },
  en: { text: "UI design review", price: "650" },
};

/// Ролик «новый счёт → выпустить → PDF» на 360: кадры screencast по их
/// времени. Движение окна — включено: ролик показывает, как оно выглядит.
async function video(page, lang) {
  await emulate(page, 360);
  await page.send("Emulation.setTouchEmulationEnabled", { enabled: true, maxTouchPoints: 5 });
  await page.send("Emulation.setEmulatedMedia", { features: [
    { name: "prefers-color-scheme", value: "light" }, { name: "prefers-reduced-motion", value: "no-preference" }] });
  await go(page, "#/documents");
  await sleep(1500);
  const dir = path.join(WORK, `frames-${lang}`);
  fs.rmSync(dir, { recursive: true, force: true });
  fs.mkdirSync(dir, { recursive: true });
  // Кадры — `Page.startScreencast`, каждый держится до следующего по своему
  // времени. В пикселях устройства (720×1560) он их отдаёт, только когда
  // масштаб 2 у самого Chrome (`--force-device-scale-factor=2` ниже):
  // с одной эмуляцией DPR — в точках CSS, 360×780, при любом `maxWidth`
  // и `scale`, а снимки подряд (`captureScreenshot`) — 2,5 кадра в секунду
  // (Chrome 158, 2026-11-23).
  const frames = [];
  let recording = true;
  page.on((msg) => {
    if (msg.method !== "Page.screencastFrame") return;
    const { data, metadata, sessionId } = msg.params;
    page.send("Page.screencastFrameAck", { sessionId }).catch(() => {});
    if (!recording) return;
    const file = path.join(dir, `${String(frames.length).padStart(5, "0")}.jpg`);
    fs.writeFileSync(file, Buffer.from(data, "base64"));
    frames.push({ file, at: metadata.timestamp });
  });
  await page.send("Page.startScreencast", { format: "jpeg", quality: 92, maxWidth: 720, maxHeight: 1560, everyNthFrame: 1 });
  await sleep(1800);
  const client = CLIENTS[lang].data;
  const demo = DEMO[lang];
  await tap(page, t("DocNewInvoice", lang));
  await sleep(1500);
  await tap(page, client.name, { contains: true });
  await sleep(1300);
  await typeSlowly(page, t("ItemText", lang), demo.text, 75);
  await sleep(700);
  await typeSlowly(page, t("ItemPrice", lang), demo.price, 160);
  await sleep(1600);
  await tap(page, t("DocNextReview", lang));
  await sleep(2500);
  // Курс ЦБ подсказкой (задача 43): у документа в долларах — «Подставить».
  const use = t("CbaRateUse", lang);
  if (await page.eval(`return [...document.querySelectorAll("button")].some((b) => b.textContent.trim() === ${JSON.stringify(use)} && b.getClientRects().length > 0);`) === true) {
    await tap(page, use);
    await sleep(1800);
  }
  // Проверка — до кнопки выпуска: её видно внизу формы.
  await page.eval(`window.scrollTo({ top: document.documentElement.scrollHeight, behavior: "smooth" }); return true;`);
  await sleep(1800);
  await tap(page, t("DocIssue", lang));
  await must(page, hasText("2026-007"), "карточка выпущенного счёта", 20000);
  const card = Date.now() / 1000;
  await sleep(2800);
  await tap(page, t("DocPdf", lang));
  await must(page, `return document.querySelector("svg.paper-page") ? true : null;`, "страницы PDF", 20000);
  await sleep(800);
  await page.eval(`const p = document.querySelector("svg.paper-page");
    p.scrollIntoView({ behavior: "smooth", block: "start" }); return true;`);
  await sleep(4000);
  recording = false;
  await page.send("Page.stopScreencast");
  // Постер — карточка выпущенного счёта, чуть погодя после её появления.
  const poster = frames.find((f) => f.at >= card + 1.5) ?? frames.at(-1);
  return { frames, dir, poster };
}

function encode(lang, frames, dir, cover) {
  // Длительность кадра — до следующего; последний держится 2 с.
  const list = [];
  for (let i = 0; i < frames.length; i++) {
    const next = frames[i + 1]?.at ?? frames[i].at + 2;
    list.push(`file '${frames[i].file.replaceAll("\\", "/")}'`, `duration ${Math.max(0.001, next - frames[i].at).toFixed(4)}`);
  }
  list.push(`file '${frames.at(-1).file.replaceAll("\\", "/")}'`);
  const concat = path.join(dir, "list.txt");
  fs.writeFileSync(concat, list.join("\n"));
  // Сжатие подобрано на глаз по кадру с суммой и мелким текстом: VP9 crf 38
  // и 48, H.264 crf 28 и 34 неотличимы при 100 %, а весят ~950 и ~600 КБ
  // (задача 119, 2026-11-23): ролик качает каждый, кто до него долистал.
  const base = ["-y", "-hide_banner", "-loglevel", "error", "-f", "concat", "-safe", "0", "-i", concat, "-vf", "fps=30,format=yuv420p", "-an"];
  const webm = path.join(OUT, `demo-${lang}.webm`);
  const mp4 = path.join(OUT, `demo-${lang}.mp4`);
  execFileSync("ffmpeg", [...base, "-c:v", "libvpx-vp9", "-b:v", "0", "-crf", "48", "-row-mt", "1", "-deadline", "good", webm]);
  execFileSync("ffmpeg", [...base, "-c:v", "libx264", "-preset", "veryslow", "-crf", "34", "-profile:v", "high", "-movflags", "+faststart", mp4]);
  const poster = path.join(OUT, `demo-${lang}.webp`);
  execFileSync("ffmpeg", ["-y", "-hide_banner", "-loglevel", "error", "-i", cover.file, "-c:v", "libwebp", "-quality", "80", poster]);
  for (const f of [webm, mp4, poster]) console.log("  ролик", path.relative(REPO, f), fs.statSync(f).size, "Б");
}

const rates = Buffer.from(await (await fetch(RATES)).arrayBuffer());
usd = JSON.parse(rates).currencies.USD;
const server = own(DIST, PORT, { extra: { ...DEPLOYED, "/app/rates.json": rates } });
const LOCALE = { hy: "hy-AM", ru: "ru-RU", en: "en-US" };
try {
  for (const lang of LANGS) {
    console.log(lang);
    // Язык Chrome — языком страницы: поле даты он пишет по своему языку.
    process.env.CHROME_ARGS = `--lang=${LOCALE[lang]} --force-device-scale-factor=2`;
    const browser = await Browser.launch({ work: WORK, name: `media-${lang}`, port: 9339 });
    const page = await open(browser, lang);
    await page.send("Emulation.setLocaleOverride", { locale: LOCALE[lang] });
    if (KEY) throw new Error("--key: пока не нужен");
    await emulate(page, 1280);
    await closeQuarter(page, lang);
    await shots(page, lang);
    const { frames, dir, poster } = await video(page, lang);
    console.log("  кадров", frames.length);
    if (frames.length > 1) encode(lang, frames, dir, poster);
    const errors = [...new Set(page.errors)];
    if (errors.length) console.log("  ошибки страницы:", errors.slice(0, 5).join(" | "));
    await page.close();
    browser.close();
  }
} finally {
  await server.stop();
}
