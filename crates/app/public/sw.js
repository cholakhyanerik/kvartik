// Service worker Kvartik: программа открывается и работает без сети
// (задача 8).
//
// Это единственный JS проекта, кроме клея wasm-bindgen и загрузчика частей
// wasm (их пишут программы сборки), и решений в нём нет — только кэш. Когда
// сказать человеку о новой версии и когда перезагрузиться, решает окно
// (crates/app/src/platform/pwa.rs).
//
// Три строки ниже при сборке переписывает kvartik-sw (crates/sw, хук
// post_build в crates/app/Trunk.toml): BUILD — хэш всех файлов сборки,
// FILES — их список, LINKED — те из них, на которые ссылается страница.
// Браузер узнаёт о новой версии сайта только так: сравнивает sw.js
// с прошлым байт в байт (по спецификации Service Worker, «Update»). Поэтому
// sw.js обязан меняться с любым файлом сайта — и со шрифтами, у которых
// в имени нет хэша: иначе новые шрифты не дошли бы до тех, у кого в кэше
// старые. Сам шаблон в сайт не копируется.
//
// Он лежит в папке программы /app/, и его scope — она же: лендинг в корне
// сайта (задача 65) он не трогает, и кэш программы страниц лендинга не
// отдаёт. Все адреса здесь — от его папки. В корне сайта — свой sw.js:
// он снимает service worker, стоявший там до переезда.
const BUILD = "dev";
const FILES = [];
const LINKED = [];

// Свой кэш у каждой сборки: старый удаляется, только когда новая сборка
// начала работать, — до тех пор им открывается старая. Кэши у сайта
// общие, по имени, и под этим же началом лежит и кэш service worker,
// стоявшего в корне до переезда в /app/: его удаляет тоже этот файл
// (sw.js корня свой не трогает — удалил бы и наш).
const PREFIX = "kvartik-";
const CACHE = PREFIX + BUILD;

// Страница программы лежит в кэше под адресом папки, а не index.html:
// хостинг отвечает на /app/index.html переадресацией на /app/, а ответ
// с переадресацией браузер на переход по странице не принимает.
const SHELL = "./";

self.addEventListener("install", (event) => {
  event.waitUntil(install());
});

// Скачать всё, проверить и только потом положить в кэш. Не сошлось —
// установка не удалась, и браузер попробует снова при следующей проверке,
// а работает прежняя версия.
//
// Проверка — потому что хостинг может ответить успехом не тем. Cloudflare
// Pages без 404.html отдаёт на неизвестный адрес страницу с кодом 200
// (404.html в сайте есть с 0.0.29, задача 15, но забыть его легко), а во
// время выкладки часть файлов приходит уже от новой
// сборки: HTML под именем wasm в кэше — программа не запустится и с сетью,
// а плашка новой версии живёт в том же wasm. Поэтому HTML — только там, где
// его ждут, а страница обязана ссылаться на файлы своей сборки (LINKED):
// страница новее файлов открылась бы без сети пустой.
async function install() {
  const files = [SHELL, ...FILES];
  const responses = await Promise.all(files.map(load));
  const page = await responses[0].clone().text();
  const missing = LINKED.filter((file) => !page.includes(file));
  if (missing.length > 0) {
    throw new Error(`страница не этой сборки: в ней нет ${missing.join(", ")}`);
  }
  const cache = await caches.open(CACHE);
  await Promise.all(files.map((file, i) => cache.put(file, responses[i])));
}

// Файл сборки с сервера. «no-cache» — сверка с сервером: файл без хэша
// в имени из кэша HTTP мог остаться от прошлой сборки.
async function load(file) {
  const response = await fetch(new Request(file, { cache: "no-cache" }));
  const type = response.headers.get("Content-Type") || "";
  const page = file === SHELL || file.endsWith(".html");
  if (!response.ok || response.redirected || type.startsWith("text/html") !== page) {
    throw new Error(`${file}: ${response.status} ${type}${response.redirected ? ", переадресация" : ""}`);
  }
  return response;
}

self.addEventListener("activate", (event) => {
  // claim — чтобы и страница первого захода, открытая ещё без service
  // worker, дальше брала файлы из кэша.
  event.waitUntil(
    caches
      .keys()
      .then((names) =>
        Promise.all(
          names
            .filter((name) => name.startsWith(PREFIX) && name !== CACHE)
            .map((name) => caches.delete(name)),
        ),
      )
      .then(() => self.clients.claim()),
  );
});

// Новая версия ждёт («waiting»), пока работает старая, — и начинает
// работать, когда окно решит, что это никому не мешает: оно присылает
// «skip-waiting» (Правило 6).
self.addEventListener("message", (event) => {
  if (event.data === "skip-waiting") {
    self.skipWaiting();
  }
});

// Сначала кэш, потом сеть. Чужие адреса и всё, кроме GET, не трогаем:
// программа никуда, кроме своего сайта, не ходит, а данные человека живут
// в IndexedDB, а не в запросах.
self.addEventListener("fetch", (event) => {
  const request = event.request;
  if (request.method !== "GET" || new URL(request.url).origin !== self.location.origin) {
    return;
  }
  const navigate = request.mode === "navigate";
  event.respondWith(
    caches.open(CACHE).then(async (cache) => {
      const hit = await cache.match(request, { ignoreSearch: navigate });
      if (hit) {
        return hit;
      }
      if (navigate) {
        const shell = await cache.match(SHELL);
        if (shell) {
          return shell;
        }
      }
      return fetch(request);
    }),
  );
});
