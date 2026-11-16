// Курсы ЦБ для программы (задача 43): файл `app/rates.json` выкладки.
//
//   node tools/deploy/rates.mjs <папка сборки> [--fallback <адрес сайта>] [--optional]
//
// Зовут его выкладка CI (`.github/workflows/ci.yml`) и выкладка курсов по
// расписанию (`.github/workflows/rates.yml`), а `tools/browser/check.mjs`
// сверяет разбор на образце ответа ЦБ из `crates/core/tests/rates/` —
// с тем файлом, что читает тест ядра (`tax::cba`).
//
// Откуда. Курсы — у ЦБ, его сервисом для сайтов (api.cba.am,
// `ExchangeRatesByDateRangeByISO`: «integrate them to your web parts» —
// cba.am/en/exchange-rates-retrieval, 2026-11-16). Программа сама к ЦБ не
// ходит (Архитектура, «Сеть — только к своему сайту»): файл лежит на сайте
// рядом с ней. В программу он не входит — ни в кэш service worker, ни в хэш
// сборки, — иначе новый курс каждый день был бы новой версией программы.
//
// Что в файле — шапка `crates/core/src/tax/cba.rs`: это договор между
// выкладкой и всеми версиями программы, и смысл его полей не меняется.
// Курс — драмы за единицу валюты в десятитысячных, целым: ЦБ пишет
// десятичной записью до четырёх знаков (у рубля — четыре), и переводится
// она по частям — целая часть и знаки после точки, каждая целым числом, —
// а не `Number("360.44")`: 0,1 в двоичной дроби не представимо (Правило 2,
// «Деньги — в целых числах»).
//
// Ответ ЦБ разбирается строго: незнакомая запись дня (не `+04:00`
// Еревана), единиц валюты не одна, курс не той записью, повтор дня — отказ,
// а не угаданный курс. Не ответил ЦБ или ответ не разобран — `--fallback`
// берёт файл с живого сайта (тоже проверенный), а `--optional` и без него
// выкладку не держит: без файла программа просто не подсказывает курс.
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

/// Валюты программы (`model::money::Currency`, кроме драма).
export const CURRENCIES = ["EUR", "USD"];
export const SOURCE = "https://api.cba.am/exchangerates.asmx";
/// Курс для поставки 2026-07-01 — первого дня правила «предыдущий рабочий
/// день» (запись `rate_day` файла правил) — опубликован 2026-06-30. Сдвинут
/// начало правила, а здесь забудут — ответ безопасный: курсов раньше нет,
/// и окно говорит «сверьте на cba.am», а не берёт чужой день.
const FIRST = "2026-06-30";
/// Сколько лет курсов держать: документы о доходах хранятся пять лет (НК,
/// ст. 33, ч. 1, п. 5) — дольше подсказка не нужна, а файл не растёт без
/// конца (Правило 1, «Накопители ограничены»).
const YEARS = 5;
/// Курс больше этого — порча (`MAX_RATE_E4` в `tax::cba`).
const MAX_E4 = 100_000 * 10_000;
/// Больше файл не бывает (`MAX_BYTES` в `tax::cba`).
const MAX_BYTES = 1 << 20;
/// Больше ответ ЦБ не бывает: за пять лет двух валют — около полумегабайта
/// (у ответа за 2025-01-01…2026-11-16 одного доллара — 104 КБ). Больший
/// ответ не читается дальше предела (Правило 13).
const MAX_RESPONSE = 4 << 20;

/// Сегодня в Ереване: UTC+4, без перевода часов с 2012 года.
export function yerevanToday(now = Date.now()) {
  return new Date(now + 4 * 3600 * 1000).toISOString().slice(0, 10);
}

/// С какого дня просить курсы у ЦБ на день `today`.
export function firstDay(today) {
  const window = `${Number(today.slice(0, 4)) - YEARS}-01-01`;
  return window > FIRST ? window : FIRST;
}

/// День календаря записью `ГГГГ-ММ-ДД`: `2026-02-30` — не день.
function realDay(day) {
  const at = new Date(`${day}T00:00:00Z`);
  return /^\d{4}-\d{2}-\d{2}$/.test(day) && !Number.isNaN(at.getTime()) && at.toISOString().slice(0, 10) === day;
}

/// Десятичная запись курса ЦБ (`360.44`, `4.3693`) — в десятитысячных.
/// `null` — не та запись.
export function e4(text) {
  const m = /^(\d{1,9})(?:\.(\d{1,4}))?$/.exec(text);
  if (!m) return null;
  const value = Number(m[1]) * 10_000 + Number((m[2] ?? "").padEnd(4, "0"));
  return value >= 1 && value <= MAX_E4 ? value : null;
}

/// Курсы из ответа ЦБ: `{ EUR: [[день, курс], …], USD: […] }`, дни по
/// возрастанию. Бросает ошибку на всём, что не похоже на ответ ЦБ. Строки
/// ответа режутся по закрывающему тегу — за один проход, а не ленивым
/// выражением по всему ответу: на ответе из тысяч незакрытых строк оно шло
/// бы квадратично и держало выкладку (ревью задачи 43).
export function parse(xml) {
  if (xml.length > MAX_RESPONSE) throw new Error("ответ ЦБ больше предела");
  if (/<(?:\w+:)?Fault\b/.test(xml)) throw new Error("ЦБ ответил ошибкой SOAP");
  if (!xml.includes("<ExchangeRatesByDateRangeByISOResult")) throw new Error("не ответ ExchangeRatesByDateRangeByISO");
  const rates = Object.fromEntries(CURRENCIES.map((code) => [code, []]));
  const pieces = xml.split("</ExchangeRatesByRange>");
  pieces.pop();
  for (const piece of pieces) {
    const open = piece.lastIndexOf("<ExchangeRatesByRange");
    if (open < 0) throw new Error("строка ЦБ без открывающего тега");
    const row = piece.slice(open);
    const field = (name) => {
      const found = [...row.matchAll(new RegExp(`<${name}>([^<]*)</${name}>`, "g"))];
      if (found.length !== 1) throw new Error(`строка ЦБ без ${name} или с двумя: ${row.slice(0, 200)}`);
      return found[0][1];
    };
    const iso = field("ISO");
    if (!CURRENCIES.includes(iso)) continue;
    const amount = field("Amount");
    if (amount !== "1") throw new Error(`${iso}: курс за ${amount} единиц, а ждали за одну`);
    const day = /^(\d{4}-\d{2}-\d{2})T00:00:00\+04:00$/.exec(field("RateDate"))?.[1];
    if (!day || !realDay(day)) throw new Error(`${iso}: день курса «${field("RateDate")}» — не день Еревана`);
    const value = e4(field("Rate"));
    if (value === null) throw new Error(`${iso} ${day}: курс «${field("Rate")}» — не та запись`);
    rates[iso].push([day, value]);
  }
  for (const code of CURRENCIES) {
    const series = rates[code].sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0));
    if (series.length === 0) throw new Error(`${code}: в ответе ЦБ нет ни одного курса`);
    for (let i = 1; i < series.length; i++) {
      if (series[i - 1][0] === series[i][0]) throw new Error(`${code}: день ${series[i][0]} дважды`);
    }
  }
  return rates;
}

/// Файл курсов — всегда одни и те же байты для одних и тех же курсов:
/// валюты по алфавиту, дни по возрастанию.
export function render(rates) {
  const currencies = CURRENCIES.map((code) => `${JSON.stringify(code)}:${JSON.stringify(rates[code])}`).join(",");
  return `{"source":${JSON.stringify(SOURCE)},"currencies":{${currencies}}}\n`;
}

/// Проверить готовый файл курсов (с живого сайта) теми же правилами, что
/// разбор программы (`Rates::parse`), и вернуть его текст. Бросает ошибку.
export function check(text) {
  if (Buffer.byteLength(text) > MAX_BYTES) throw new Error("файл курсов больше предела");
  const file = JSON.parse(text);
  for (const code of CURRENCIES) {
    const series = file?.currencies?.[code];
    if (!Array.isArray(series) || series.length === 0) throw new Error(`${code}: курсов нет`);
    let before = "";
    for (const pair of series) {
      const ok = Array.isArray(pair) && pair.length === 2 && typeof pair[0] === "string" && realDay(pair[0])
        && Number.isInteger(pair[1]) && pair[1] >= 1 && pair[1] <= MAX_E4 && pair[0] > before;
      if (!ok) throw new Error(`${code}: пара ${JSON.stringify(pair)} не годится`);
      before = pair[0];
    }
  }
  return text;
}

/// Тело ответа текстом — не больше [`MAX_RESPONSE`] байт: больший ответ
/// бросается, не дочитанный.
async function limited(response) {
  const chunks = [];
  let size = 0;
  for await (const chunk of response.body) {
    size += chunk.length;
    if (size > MAX_RESPONSE) throw new Error("ответ больше предела");
    chunks.push(chunk);
  }
  return Buffer.concat(chunks).toString("utf8");
}

/// Ответ ЦБ на дни `from`…`to` — тремя попытками: сервис ЦБ бывает
/// отвечает не с первого раза.
export async function fetchCba(from, to) {
  const body = `<?xml version="1.0" encoding="utf-8"?>
<soap:Envelope xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xmlns:xsd="http://www.w3.org/2001/XMLSchema" xmlns:soap="http://schemas.xmlsoap.org/soap/envelope/"><soap:Body><ExchangeRatesByDateRangeByISO xmlns="http://www.cba.am/"><ISOCodes>${CURRENCIES.join(",")}</ISOCodes><DateFrom>${from}</DateFrom><DateTo>${to}</DateTo></ExchangeRatesByDateRangeByISO></soap:Body></soap:Envelope>`;
  let failure;
  for (let attempt = 1; attempt <= 3; attempt++) {
    try {
      const response = await fetch(SOURCE, {
        method: "POST",
        headers: {
          "Content-Type": "text/xml; charset=utf-8",
          SOAPAction: '"http://www.cba.am/ExchangeRatesByDateRangeByISO"',
        },
        body,
        signal: AbortSignal.timeout(30_000),
      });
      if (!response.ok) throw new Error(`ЦБ ответил ${response.status}`);
      return await limited(response);
    } catch (error) {
      failure = error;
      await new Promise((resolve) => setTimeout(resolve, attempt * 2000));
    }
  }
  throw failure;
}

/// Тот же ли файл пишет разбор из образца ответа ЦБ, что лежит рядом,
/// в `<папка>/rates.json`, — его читает тест ядра (`tax::cba`). Сверяют
/// `check.mjs` и выкладка курсов по расписанию перед тем, как довериться
/// программе (ревью задачи 43).
function sample(dir) {
  const made = render(parse(fs.readFileSync(path.join(dir, "cba.xml"), "utf8")));
  return made === fs.readFileSync(path.join(dir, "rates.json"), "utf8");
}

async function main() {
  const { values: opt, positionals } = parseArgs({
    allowPositionals: true,
    options: {
      fallback: { type: "string" },
      optional: { type: "boolean", default: false },
      sample: { type: "string" },
    },
  });
  if (opt.sample) {
    if (!sample(opt.sample)) {
      console.error(`rates.mjs пишет из ${opt.sample}/cba.xml не тот файл, что ${opt.sample}/rates.json`);
      process.exit(1);
    }
    console.error("Разбор ответа ЦБ сходится с образцом");
    return;
  }
  if (positionals.length !== 1) {
    console.error("использование: rates.mjs <папка сборки> [--fallback <адрес сайта>] [--optional] | --sample <папка образца>");
    process.exit(2);
  }
  const out = path.join(positionals[0], "app", "rates.json");
  if (!fs.existsSync(path.dirname(out))) {
    console.error(`нет папки программы ${path.dirname(out)}`);
    process.exit(2);
  }
  const today = yerevanToday();
  let text;
  try {
    text = render(parse(await fetchCba(firstDay(today), today)));
    console.error(`Курсы ЦБ: ${firstDay(today)}…${today} — из ответа ЦБ`);
  } catch (error) {
    console.error(`Курсы ЦБ не получены: ${error.message}`);
    if (opt.fallback) {
      try {
        const response = await fetch(`${opt.fallback.replace(/\/$/, "")}/app/rates.json`, {
          signal: AbortSignal.timeout(30_000),
        });
        if (!response.ok) throw new Error(`сайт ответил ${response.status}`);
        text = check(await limited(response));
        console.error("Курсы ЦБ — прежние, с сайта");
      } catch (fallbackError) {
        console.error(`и с сайта не взяты: ${fallbackError.message}`);
      }
    }
  }
  if (text === undefined) {
    if (opt.optional) {
      console.error("Выкладка — без файла курсов: программа их просто не подсказывает");
      return;
    }
    process.exit(1);
  }
  fs.writeFileSync(out, text);
}

// Программой — только когда её зовут: `check.mjs` берёт отсюда разбор.
if (process.argv[1] && fileURLToPath(import.meta.url) === path.resolve(process.argv[1])) {
  await main();
}
