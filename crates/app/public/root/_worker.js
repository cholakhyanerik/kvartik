// Сервер Kvartik (задача 106) — функция Cloudflare Pages на своём домене,
// «advanced mode»: файл `_worker.js` в корне выкладки (docs Cloudflare, Pages
// Functions, «Advanced mode», сверено 2026-11-18). Зовут её только адреса
// `/api/*` (`_routes.json` рядом): остальной сайт Pages отдаёт файлами, мимо
// неё и бесплатно, как раньше.
//
// Что делает. `POST /api/trial` с телом `{"tin":"<8 цифр>","consent":"<день>"}`
// — с какого дня идёт пробный период на этом ՀՎՀՀ
// (`kvartik_core::license::TRIAL_PATH`; условия, п. 3, редакция от
// 18.11.2026). Первый вопрос о номере записывает день — сегодня по Еревану, —
// следующие отвечают им же: так пробный период не начинается заново ни в
// другом браузере, ни на другом устройстве, ни после очистки данных. Ответ —
// `{"first":"ГГГГ-ММ-ДД"}`: договор с окном всех версий, как файл курсов ЦБ, —
// новые поля можно, смысл поля менять нельзя. Образец —
// `crates/core/tests/trial/answer.json`.
//
// Согласие (решение человека 2026-11-18; политика конфиденциальности, п. 2).
// ՀՎՀՀ того, кто только пробует программу, обрабатывается лишь с его согласия
// (ՀՕ-49-Ն, ст. 8 ч. 1 п. 1), поэтому вопрос без версии слов согласия
// (`license::TRIAL_CONSENT`) — отказ, а версия хранится у записи: факт
// согласия доказывает оператор (ст. 9 ч. 8). Отзыв — `{"tin":…,"withdraw":
// true}` (ст. 9 ч. 3): запись удаляется. Запись хранится 3 года с первого дня
// пробного периода (ст. 9 ч. 2; решение человека 2026-11-18) — старшие
// удаляет каждый вопрос.
//
// Что хранит. Не номер, а HMAC-SHA256 номера секретом сервера `TRIAL_KEY`
// (секрет проекта Pages, в репозитории его нет; ставит его
// `tools/deploy/trial-key.mjs`): утечка базы без секрета номеров не раскроет,
// а SHA-256 без секрета перебирается за секунды — номеров всего 10⁸. И день
// начала, момент первой записи и версию согласия. IP, браузера и прочего —
// нет. База — D1 `TRIALS` (`tools/deploy/wrangler.toml`): у выкладки всем
// своя, у предпросмотров своя.
//
// Чего не делает. Ни куки, ни учётных записей, ни запросов наружу.
// Заголовки `_headers` к ответам функции Pages не применяет (docs Cloudflare,
// Pages, «Headers», сверено 2026-11-18) — свои она ставит сама.
//
// Чужой сайт. Запрос с телом-строкой — «простой», браузер шлёт его и с чужой
// страницы без вопроса (CORS), а чужая страница могла бы так начинать
// пробный период за людей по их ՀՎՀՀ. Поэтому запрос браузера принимается
// только со своей страницы: `Sec-Fetch-Site: same-origin` и свой `Origin`.
// Программа вне браузера (проверка выкладки `tools/deploy/check.sh`) этих
// заголовков не шлёт и проходит; подделать их она может — это не защита
// от неё, а только от чужих страниц.
//
// Без секрета или базы — 503 «not configured»: окно тогда считает срок по
// тому, что знает само (Правило 2), а проверка выкладки краснеет.

const PATH = "/api/trial";
// Тело — `{"tin":"…","consent":"…"}`, полсотни байт; запас — на новые поля.
const MAX_BODY = 256;
const TIN = /^\d{8}$/;
const DAY = /^\d{4}-\d{2}-\d{2}$/;
// Ереван — UTC+4 круглый год, перехода на летнее время нет.
const YEREVAN_MS = 4 * 60 * 60 * 1000;
// Сколько лет хранится запись, считая с первого дня пробного периода.
const KEEP_YEARS = 3;
const SCHEMA = [
  "CREATE TABLE IF NOT EXISTS trials (tin_mac TEXT PRIMARY KEY, first_day TEXT NOT NULL, at INTEGER NOT NULL, consent TEXT NOT NULL) WITHOUT ROWID",
  "CREATE INDEX IF NOT EXISTS trials_first_day ON trials (first_day)",
];
const EXPIRE = "DELETE FROM trials WHERE first_day < ?1";
const INSERT =
  "INSERT INTO trials (tin_mac, first_day, at, consent) VALUES (?1, ?2, ?3, ?4) ON CONFLICT (tin_mac) DO UPDATE SET consent = excluded.consent";
const SELECT = "SELECT first_day FROM trials WHERE tin_mac = ?1";
const FORGET = "DELETE FROM trials WHERE tin_mac = ?1";

// На изолят: таблица заведена, ключ HMAC разобран.
let schema = null;
let hmac = null;

function reply(status, body, extra = {}) {
  return new Response(JSON.stringify(body), {
    status,
    headers: {
      "Content-Type": "application/json; charset=utf-8",
      "Cache-Control": "no-store",
      "X-Content-Type-Options": "nosniff",
      "Content-Security-Policy": "default-src 'none'; frame-ancestors 'none'",
      "Cross-Origin-Resource-Policy": "same-origin",
      "Referrer-Policy": "no-referrer",
      ...extra,
    },
  });
}

/// HMAC-SHA256 номера `tin` секретом `secret` — шестнадцатеричной строкой.
async function mac(secret, tin) {
  if (hmac === null || hmac.secret !== secret) {
    const key = await crypto.subtle.importKey(
      "raw",
      new TextEncoder().encode(secret),
      { name: "HMAC", hash: "SHA-256" },
      false,
      ["sign"],
    );
    hmac = { secret, key };
  }
  const signature = await crypto.subtle.sign("HMAC", hmac.key, new TextEncoder().encode(tin));
  return [...new Uint8Array(signature)].map((byte) => byte.toString(16).padStart(2, "0")).join("");
}

/// День по Еревану момента `now` на `years` лет раньше: `ГГГГ-ММ-ДД`.
function day(now, years = 0) {
  const date = new Date(now + YEREVAN_MS);
  date.setUTCFullYear(date.getUTCFullYear() - years);
  return date.toISOString().slice(0, 10);
}

async function trial(request, env, url) {
  if (request.method !== "POST") return reply(405, { error: "method" }, { Allow: "POST" });
  const site = request.headers.get("Sec-Fetch-Site");
  const origin = request.headers.get("Origin");
  if ((site !== null && site !== "same-origin") || (origin !== null && origin !== url.origin)) {
    return reply(403, { error: "cross-site" });
  }
  if (Number(request.headers.get("Content-Length") ?? 0) > MAX_BODY) return reply(413, { error: "too large" });
  const text = await request.text();
  if (text.length > MAX_BODY) return reply(413, { error: "too large" });
  let body;
  try {
    body = JSON.parse(text);
  } catch {
    return reply(400, { error: "json" });
  }
  const tin = body?.tin;
  if (typeof tin !== "string" || !TIN.test(tin)) return reply(400, { error: "tin" });
  const withdraw = body.withdraw === true;
  const consent = body.consent;
  if (!withdraw && (typeof consent !== "string" || !DAY.test(consent))) return reply(400, { error: "consent" });
  const secret = env.TRIAL_KEY;
  if (typeof secret !== "string" || secret.length < 32 || !env.TRIALS) {
    return reply(503, { error: "not configured" });
  }
  try {
    schema ??= env.TRIALS.batch(SCHEMA.map((sql) => env.TRIALS.prepare(sql)));
    await schema;
    const id = await mac(secret, tin);
    if (withdraw) {
      await env.TRIALS.prepare(FORGET).bind(id).run();
      return reply(200, { withdrawn: true });
    }
    const now = Date.now();
    const [, , found] = await env.TRIALS.batch([
      env.TRIALS.prepare(EXPIRE).bind(day(now, KEEP_YEARS)),
      env.TRIALS.prepare(INSERT).bind(id, day(now), now, consent),
      env.TRIALS.prepare(SELECT).bind(id),
    ]);
    const first = found.results[0]?.first_day;
    if (typeof first !== "string") return reply(503, { error: "storage" });
    return reply(200, { first });
  } catch {
    // Таблицу заведёт следующий запрос: обещание с отказом не помнится.
    schema = null;
    return reply(503, { error: "storage" });
  }
}

export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    if (url.pathname === PATH) return trial(request, env, url);
    if (url.pathname.startsWith("/api/")) return reply(404, { error: "not found" });
    // Сюда приходят только `/api/*` (`_routes.json`), но в «advanced mode»
    // функция отвечает за все адреса, а файлы сайта отдаёт `env.ASSETS` (docs
    // Cloudflare, Pages Functions, «Advanced mode», сверено 2026-11-18): не
    // учти хостинг `_routes.json` — сайт отдавался бы файлами, как без функции,
    // а не ответом 404 на каждую страницу.
    return env.ASSETS ? env.ASSETS.fetch(request) : reply(404, { error: "not found" });
  },
};
