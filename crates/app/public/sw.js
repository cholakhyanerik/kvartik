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

// Свои цвета с первого кадра (задача 89). Палитру человека окно ставит на
// <html> при запуске wasm, а страницу браузер рисует раньше — в цветах
// программы и темы устройства. Поэтому окно кладёт копию палитры стилем
// в кэш LOOK (crates/app/src/platform/look.rs), а страницу программы этот
// файл отдаёт со ссылкой на неё: стиль в <head> держит первую отрисовку,
// а приходит отсюда, из кэша, без круга сети. Своего скрипта в странице нет
// — встроенный CSP не пускает. Окно, встав, ставит палитру само, а ссылку
// снимает его часть настроек.
//
// Решений здесь по-прежнему нет: что в копии, решает окно, а здесь — только
// вписать её. Переписанная страница существует лишь на пути к вкладке:
// в кэше лежат байты сборки, их сверяет установка (LINKED), и проверка
// выкладки берёт страницу с сервера, а не отсюда. LOOK — не кэш сборки: имя
// не начинается с PREFIX, и новая версия его не удаляет.
const LOOK = "kvartik.look";
const SHEET = "look.css";
const SHEET_PATH = new URL(SHEET, self.location).pathname;
const COLOR_SCHEME = /<meta\b[^>]*?\bname=(?:"color-scheme"|color-scheme(?=[\s>\/]))[^>]*>/;

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

// «Поделиться → Kvartik» на Android (задача 124, часть D): Chrome шлёт
// поделённое запросом POST на адрес цели из манифеста (share_target), и
// принимает его этот файл — в сеть оно не уходит. Текст и файл ложатся в свой
// кэш SHARED — его имя не начинается с PREFIX, и новая версия его не удаляет,
// а стирает окно, забрав своё (crates/app/src/platform/shared.rs), — а ответ —
// переход 303 на экран, где окно их заберёт, с меткой ?share= в адресе.
// Решений здесь нет: только положить; код это или копия, решает окно.
const SHARE_PATH = new URL("share", self.location).pathname;
const SHARED = "kvartik.share";
// Больше не кладётся: файл копии программа больше 128 МБ не открывает
// (backup::MAX_FILE_BYTES), частей у копии не больше 999 (backup::MAX_PARTS),
// а код текстом — не больше мегабайта. Всех файлов разом — не больше
// гигабайта, восьми частей по 128 МБ (решение агента 2026-11-25, ревью
// 0.0.199): POST на адрес цели service worker принимает с любой страницы, не
// только из меню «Поделиться», а незабранное лежит в хранилище kvartik.com,
// общем с базой, до следующего раза. Часть копии сверх предела окно назовёт
// недостающей, и её выбирают на экране копии. Числа сверяет tests/pwa.rs.
const MAX_SHARED_FILE = 128 * 1024 * 1024;
const MAX_SHARED_FILES = 999;
const MAX_SHARED_BYTES = 1024 * 1024 * 1024;
const MAX_SHARED_TEXT = 1024 * 1024;

async function shared(request) {
  let to = "./#/sync";
  try {
    const form = await request.formData();
    // Поделённое прошлый раз, которого окно не забрало (программу закрыли
    // раньше), — прочь: его файлы легли бы в цепочку рядом с новыми.
    await caches.delete(SHARED);
    const cache = await caches.open(SHARED);
    // Части копии делятся разом (задача 124): все файлы — по номерам с 1,
    // в порядке мессенджера, а не только первый; цепочке восстановления
    // порядок частей не важен. Лёг хоть один — экран копии: какие части не
    // легли, скажет окно. Ключи — договор с окном (platform/shared.rs): его
    // сторону держат браузерные тесты, эту — tools/browser/check.mjs.
    const files = [];
    let bytes = 0;
    for (const file of form.getAll("backup")) {
      if (!(file instanceof File) || file.size === 0 || file.size > MAX_SHARED_FILE) {
        continue;
      }
      if (files.length === MAX_SHARED_FILES || bytes + file.size > MAX_SHARED_BYTES) {
        break;
      }
      bytes += file.size;
      files.push(file);
    }
    for (const [at, file] of files.entries()) {
      await cache.put(`shared-file-${at + 1}`, new Response(file));
      await cache.put(`shared-name-${at + 1}`, new Response(file.name));
      to = "./?share=file#/backup";
    }
    if (files.length === 0) {
      const text = ["text", "title", "url"]
        .map((name) => form.get(name))
        .filter((part) => typeof part === "string" && part.length > 0)
        .join("\n");
      if (text.length > 0 && text.length <= MAX_SHARED_TEXT) {
        await cache.put("shared-text", new Response(text));
        to = "./?share=text#/sync";
      }
    }
  } catch (error) {
    // Не разобралось — программа открывается на «Другом устройстве», без метки.
  }
  return Response.redirect(new URL(to, self.location).href, 303);
}

// Сначала кэш, потом сеть. Чужие адреса и всё, кроме GET, не трогаем:
// программа никуда, кроме своего сайта, не ходит, а данные человека живут
// в IndexedDB, а не в запросах. Кроме поделённого: его POST — свой (выше).
self.addEventListener("fetch", (event) => {
  const request = event.request;
  const url = new URL(request.url);
  if (request.method === "POST" && url.origin === self.location.origin && url.pathname === SHARE_PATH) {
    event.respondWith(shared(request));
    return;
  }
  if (request.method !== "GET" || url.origin !== self.location.origin) {
    return;
  }
  if (url.pathname === SHEET_PATH) {
    event.respondWith(sheet());
    return;
  }
  const navigate = request.mode === "navigate";
  event.respondWith(
    caches.open(CACHE).then(async (cache) => {
      const hit = await cache.match(request, { ignoreSearch: navigate });
      if (hit) {
        return navigate ? dressed(hit) : hit;
      }
      if (navigate) {
        const shell = await cache.match(SHELL);
        if (shell) {
          return dressed(shell);
        }
      }
      return fetch(request);
    }),
  );
});

// Страница — со ссылкой на копию палитры, если она есть, и с её темой
// у мета-тега color-scheme: полосы прокрутки первого кадра — тоже в тон.
// Копию ищет caches.match с cacheName: caches.open завёл бы пустой кэш
// у каждого, у кого своих цветов нет. Тема и отметка — заголовками копии,
// и вписываются они, только если это они: в разметку не попадёт ничего,
// кроме слова темы и шестнадцатеричной отметки. Нет копии, не та она или
// это не страница — ответ кэша как есть.
async function dressed(page) {
  const look = await caches.match(SHEET, { cacheName: LOOK });
  const scheme = look ? look.headers.get("Kvartik-Scheme") : null;
  const mark = look ? look.headers.get("Kvartik-Mark") : null;
  const html = (page.headers.get("Content-Type") || "").startsWith("text/html");
  if (!html || !/^(light|dark)$/.test(scheme || "") || !/^[0-9a-f]{16}$/.test(mark || "")) {
    return page;
  }
  const text = await page.clone().text();
  // Конец <head> — или начало <body>: выпускная сборка сжимает разметку
  // (Trunk, minify-html), и закрывающий тег мог бы и пропасть.
  const head = text.search(/<\/head>|<body[\s>]/);
  if (head < 0) {
    return page;
  }
  const link = `<link rel="stylesheet" href="${SHEET}" data-look="${mark}">`;
  // Мета-тег — заменой целиком: сжатая разметка пишет его по-своему
  // (`<meta content="light dark" name=color-scheme>`).
  const body = text.slice(0, head).replace(COLOR_SCHEME, `<meta name="color-scheme" content="${scheme}">`)
    + link + text.slice(head);
  // Заголовки — страницы, и CSP с ними; длины и сжатия у нового тела нет.
  const headers = new Headers(page.headers);
  headers.delete("Content-Length");
  headers.delete("Content-Encoding");
  return new Response(body, { status: page.status, statusText: page.statusText, headers });
}

// Копия палитры для ссылки. Её убрали, пока страница шла, — пустой стиль:
// страница рисуется в цветах программы, без ошибки в консоли.
async function sheet() {
  const look = await caches.match(SHEET, { cacheName: LOOK });
  return look || new Response("", { headers: { "Content-Type": "text/css; charset=utf-8" } });
}
