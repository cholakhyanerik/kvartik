// Функция сайта Kvartik — функция Cloudflare Pages на своём домене,
// «advanced mode»: файл `_worker.js` в корне выкладки (docs Cloudflare, Pages
// Functions, «Advanced mode», сверено 2026-11-18). Зовут её только адреса
// wasm программы `/app/*.wasm` (`_routes.json` рядом, задача 109): остальной
// сайт Pages отдаёт файлами, мимо неё и бесплатно, как раньше.
//
// Сервера пробного периода (`POST /api/trial`, задача 106) больше нет
// (задача 113; условия, п. 3, и политика конфиденциальности, п. 2, редакции
// от 21.11.2026): пробуют программу ключом продавца, а ՀՎՀՀ на сервер не
// уходит. Версия программы, которая ещё не обновилась, может при открытии
// спросить этот адрес: его отдаёт хостинг без функции — отказом, а не
// ответом, — и такая версия считает срок по тому, что знает устройство, как
// без сети (`platform::site::post` до 0.0.162 берёт только ответ 2xx). Ничего
// функция не хранит и не пишет, а ни куки, ни учётных записей, ни запросов
// наружу у неё нет. Заголовки `_headers` к её ответам Pages не применяет
// (docs Cloudflare, Pages, «Headers», сверено 2026-11-18) — свои она ставит
// сама.
//
// Заранее сжатый wasm (задача 109). Cloudflare сжимает ответы сам, но около
// brotli уровня 4: главный wasm 0.0.153 по проводу — 281 260 Б, а уровнем
// 11 — 232 277, и рабочий экран телефона на замере — 3,12 с против 2,84, за
// целью в 3 с (CLAUDE.md, Правило 1). Уровень у Cloudflare не выбирают,
// а заранее сжатых файлов Pages не отдаёт (docs Cloudflare, «Compression»
// и «Compression Rules», сверено 2026-11-18). Поэтому сборка кладёт рядом
// с каждым wasm программы его копию уровнем 11 — `<имя>.wasm.br`
// (`crates/sw/src/precompress.rs`), — а на адрес самого wasm функция отдаёт
// её тело как есть: с `Content-Encoding: br` и `encodeBody: "manual"`, иначе
// рантайм сжал бы его второй раз (docs Cloudflare, Workers, «Response»,
// 2026-11-18). Заголовки — те, что Pages поставил копии по `_headers`
// (правило `/app/*.wasm.br`: правила `_headers` к ответу функции не
// применяются), с типом wasm. Браузеру без brotli, запросу диапазона байтов,
// без копии и при любом сбое — wasm файлом сайта, как до задачи 109: сжат
// хуже, но программа открывается. Ничего не хранит и не пишет.

/// Wasm программы — адрес, ответ на который функция берёт из копии `.br`.
const WASM = /^\/app\/[^/]+\.wasm$/;

/// Принимает ли браузер brotli: `br` в `Accept-Encoding`, и не с `q=0`.
function brotli(request) {
  return (request.headers.get("Accept-Encoding") ?? "").split(",").some((part) => {
    const [name, ...params] = part.split(";").map((piece) => piece.trim().toLowerCase());
    return name === "br" && !params.some((param) => /^q=0(\.0*)?$/.test(param));
  });
}

async function precompressed(request, env, url) {
  const asIs = () => env.ASSETS.fetch(request);
  if ((request.method !== "GET" && request.method !== "HEAD") || request.headers.has("Range") || !brotli(request)) {
    return asIs();
  }
  try {
    // Сверка по ETag копии: service worker берёт wasm `no-cache` сразу после
    // страницы, и без 304 браузер скачал бы его второй раз.
    const headers = new Headers();
    for (const name of ["If-None-Match", "If-Modified-Since"]) {
      const value = request.headers.get(name);
      if (value !== null) headers.set(name, value);
    }
    const copy = await env.ASSETS.fetch(new Request(`${url.origin}${url.pathname}.br`, { method: request.method, headers }));
    // Копия, которую хостинг сам сжал, — уже не копия уровнем 11 как есть.
    if ((copy.status !== 200 && copy.status !== 304) || copy.headers.has("Content-Encoding")) return asIs();
    const out = new Headers(copy.headers);
    out.set("Content-Type", "application/wasm");
    out.set("Vary", "Accept-Encoding");
    if (copy.status === 200) out.set("Content-Encoding", "br");
    return new Response(copy.body, { status: copy.status, headers: out, encodeBody: "manual" });
  } catch {
    return asIs();
  }
}

export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    if (WASM.test(url.pathname) && env.ASSETS) return precompressed(request, env, url);
    // Сюда приходят только wasm (`_routes.json`), но в «advanced mode» функция
    // отвечает за все адреса, а файлы сайта отдаёт `env.ASSETS` (docs
    // Cloudflare, Pages Functions, «Advanced mode», сверено 2026-11-18): не
    // учти хостинг `_routes.json` — сайт отдавался бы файлами, как без
    // функции, а не ответом 404 на каждую страницу.
    if (env.ASSETS) return env.ASSETS.fetch(request);
    return new Response("not found", {
      status: 404,
      headers: { "Content-Type": "text/plain; charset=utf-8", "Cache-Control": "no-store", "X-Content-Type-Options": "nosniff" },
    });
  },
};
