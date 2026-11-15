// headless Chrome через протокол DevTools: свой профиль, свой порт, одна
// вкладка. Общее у глазного прогона (skill browser-run) и проверки
// собранного сайта в CI (`check.mjs` рядом, задача 60). Node 22 и новее без
// зависимостей: WebSocket у него свой. Windows — машина автора, Linux — CI.
//
// Почему так, а не иначе (CLAUDE.md, Правило 2, «глазной прогон»):
// - ширина 360 и тема — только эмуляцией: окно уже ~500 точек headless Chrome
//   не сужает, а тему берёт у Windows (Chrome 155, 2026-10-11);
// - честность ширины — числом: `scrollWidth` равен ширине;
// - снимок фоновой вкладки не делается — команда висит: перед ним
//   `Page.bringToFront` (2026-10-12);
// - выражение `Runtime.evaluate` — в своей функции: у консоли одна общая
//   область, и второй `const f` бросает исключение (2026-10-15);
// - гасится только свой Chrome — деревом своего процесса: браузер человека
//   работает рядом (2026-10-15);
// - на Linux — `--no-sandbox` и `--disable-dev-shm-usage`, как у раннера
//   браузерных тестов в том же CI (по исходникам wasm-bindgen-cli 0.2.130,
//   `headless.rs`): машина CI одноразовая, а открывается только свой сайт;
// - свои флаги Chrome — переменная CHROME_ARGS. В CI видеокарты нет, и
//   рисует Chrome там иначе, чем на машине автора: проверку, зелёную здесь
//   и красную там, повторяет `CHROME_ARGS=--disable-gpu` (2026-11-08).

import { spawn, execSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

const WINDOWS = process.platform === "win32";
export const CHROME = process.env.CHROME
  ?? (WINDOWS ? "C:/Program Files/Google/Chrome/Application/chrome.exe" : "google-chrome");
export const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/// Сколько ждать ответа одной команды DevTools. Самое долгое законное
/// ожидание проверок — опрос `waitFor` по 60 с, но он — много коротких
/// команд; одна команда дольше минуты — значит, ответа не будет.
const SEND_MS = 120_000;

/// Эмуляции окна: телефон и компьютер (Правило 2: 360 и 1280).
export const DEVICES = {
  360: { width: 360, height: 780, deviceScaleFactor: 2, mobile: true },
  1280: { width: 1280, height: 800, deviceScaleFactor: 1, mobile: false },
};

/// Запустить программу так, чтобы `kill` погасил её вместе с дочерними:
/// на Linux — своей группой процессов.
export function spawnTree(command, args, options = {}) {
  return spawn(command, args, { ...options, detached: !WINDOWS });
}

/// Погасить процесс `spawnTree` с деревом дочерних.
export function kill(pid) {
  if (WINDOWS) {
    try { execSync(`taskkill /pid ${pid} /T /F`, { stdio: "ignore" }); } catch {}
    return;
  }
  try { process.kill(-pid, "SIGKILL"); } catch {
    try { process.kill(pid, "SIGKILL"); } catch {}
  }
}

export class Browser {
  /// Chrome с пустым профилем `<work>/chrome-<name>`. `port` — порт
  /// DevTools, у каждого прогона свой.
  static async launch({ work, name, port }) {
    const profile = path.join(work, `chrome-${name}`);
    fs.rmSync(profile, { recursive: true, force: true });
    const child = spawnTree(CHROME, ["--headless=new", `--remote-debugging-port=${port}`, `--user-data-dir=${profile}`,
      "--no-first-run", "--no-default-browser-check", ...(WINDOWS ? [] : ["--no-sandbox", "--disable-dev-shm-usage"]),
      ...(process.env.CHROME_ARGS ?? "").split(/\s+/).filter(Boolean), "about:blank"], { stdio: "ignore" });
    let failed;
    child.on("error", (e) => { failed = e; });
    let url;
    for (let i = 0; i < 100 && !url && !failed; i++) {
      try { url = (await (await fetch(`http://127.0.0.1:${port}/json/version`)).json()).webSocketDebuggerUrl; }
      catch { await sleep(200); }
    }
    if (failed) throw new Error(`Chrome не запустился (${CHROME}; путь — переменная CHROME): ${failed.message}`);
    if (!url) { kill(child.pid); throw new Error(`Chrome не открыл DevTools на порту ${port}`); }
    const browser = new Browser(child, new WebSocket(url));
    await new Promise((res, rej) => { browser.ws.onopen = res; browser.ws.onerror = rej; });
    browser.ws.onmessage = (m) => browser.message(JSON.parse(m.data));
    return browser;
  }

  constructor(child, ws) {
    this.child = child; this.ws = ws; this.id = 0; this.pending = new Map(); this.listeners = new Map();
  }

  message(msg) {
    if (msg.id && this.pending.has(msg.id)) {
      const { res, rej, method } = this.pending.get(msg.id);
      this.pending.delete(msg.id);
      if (msg.error) rej(new Error(`${method}: ${JSON.stringify(msg.error)}`));
      else res(msg.result);
    } else if (msg.method) {
      this.listeners.get(msg.sessionId)?.(msg);
    }
  }

  /// Команда DevTools. Ответа нет за [`SEND_MS`] — отказ с её именем, а не
  /// вечное ожидание: обещание в странице, которое так и не разрешилось
  /// (`await` в `eval`), держало бы проверку, пока CI не снимет задание
  /// (2026-11-15: «Собранный сайт в браузере» простоял 22 минуты до отмены).
  /// Таймер не держит Node после конца проверки (`unref`).
  send(method, params = {}, sessionId) {
    const id = ++this.id;
    this.ws.send(JSON.stringify({ id, method, params, sessionId }));
    return new Promise((res, rej) => {
      const timer = setTimeout(() => {
        if (this.pending.delete(id)) rej(new Error(`${method}: нет ответа за ${SEND_MS / 1000} с`));
      }, SEND_MS);
      timer.unref();
      this.pending.set(id, {
        res: (value) => { clearTimeout(timer); res(value); },
        rej: (error) => { clearTimeout(timer); rej(error); },
        method,
      });
    });
  }

  /// Вкладка. `context: true` — в своём контексте браузера, как в инкогнито:
  /// хранилище пустое (для замера первого захода). Вопроса об установке там
  /// нет вовсе — установку смотрят в основном (2026-10-13).
  async page({ context = false } = {}) {
    let browserContextId;
    if (context) ({ browserContextId } = await this.send("Target.createBrowserContext"));
    let targetId;
    if (context) ({ targetId } = await this.send("Target.createTarget", { url: "about:blank", browserContextId }));
    else targetId = (await this.send("Target.getTargets")).targetInfos.find((t) => t.type === "page").targetId;
    const { sessionId } = await this.send("Target.attachToTarget", { targetId, flatten: true });
    const page = new Page(this, sessionId, targetId, browserContextId);
    this.listeners.set(sessionId, (msg) => page.event(msg));
    await page.send("Page.enable");
    await page.send("Runtime.enable");
    await page.send("Log.enable");
    return page;
  }

  close() { try { this.ws.close(); } catch {} kill(this.child.pid); }
}

export class Page {
  constructor(browser, session, targetId, contextId) {
    Object.assign(this, { browser, session, targetId, contextId });
    /// Ошибки страницы: исключения, `console.error`, отказы CSP и сети.
    this.errors = [];
    this.logs = [];
    this.handlers = [];
  }

  send(method, params = {}) { return this.browser.send(method, params, this.session); }
  on(handler) { this.handlers.push(handler); }

  event(msg) {
    const p = msg.params;
    if (msg.method === "Runtime.consoleAPICalled") {
      const text = p.args.map((a) => a.value ?? a.description).join(" ");
      this.logs.push(text);
      if (p.type === "error") this.errors.push(`console.error: ${text}`);
    } else if (msg.method === "Runtime.exceptionThrown") {
      this.errors.push(`исключение: ${p.exceptionDetails.exception?.description ?? p.exceptionDetails.text}`);
    } else if (msg.method === "Log.entryAdded" && p.entry.level === "error") {
      this.errors.push(`${p.entry.source}: ${p.entry.text}${p.entry.url ? ` (${p.entry.url})` : ""}`);
    }
    for (const h of this.handlers) h(msg);
  }

  /// Тело функции, исполненное в странице; `await` можно. Ошибка — `{ error }`.
  async eval(code) {
    try {
      const r = await this.send("Runtime.evaluate", {
        expression: `(async () => { ${code} })()`, awaitPromise: true, returnByValue: true,
      });
      if (r.exceptionDetails) return { error: r.exceptionDetails.exception?.description ?? r.exceptionDetails.text };
      return r.result.value;
    } catch (e) { return { error: String(e) }; }
  }

  /// Ждёт, пока тело функции вернёт непустое значение без ошибки; `null` —
  /// не дождались.
  async waitFor(code, ms = 20000) {
    const end = Date.now() + ms;
    while (Date.now() < end) {
      const v = await this.eval(code);
      if (v && !v.error) return v;
      await sleep(200);
    }
    return null;
  }

  async go(url) { await this.send("Page.navigate", { url }); await sleep(300); }
  async reload() { await this.send("Page.reload"); await sleep(300); }
  /// Переход с `/` на `/#/x` — внутри документа, без перезагрузки и без
  /// проверки новой версии: сначала `about:blank` (2026-10-16).
  async fresh(url) { await this.go("about:blank"); await this.go(url); }

  /// Ширина 360 или 1280 (`DEVICES`) и тема: `dark` — тёмная.
  async emulate(width, theme = "light") {
    const device = DEVICES[width] ?? { width, height: 800, deviceScaleFactor: 1, mobile: false };
    await this.send("Emulation.setDeviceMetricsOverride", device);
    await this.send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-color-scheme", value: theme }] });
    this.device = device;
  }

  /// Языки браузера для первого захода: меняет и `navigator.languages`.
  async acceptLanguage(languages) {
    const { userAgent } = await this.browser.send("Browser.getVersion");
    await this.send("Emulation.setUserAgentOverride", { userAgent, acceptLanguage: languages });
  }

  /// Язык окна: `hy`, `ru` или `en` — как его запоминает окно
  /// (`store/device.rs`), потом перезагрузка: смена `#…` страницу не
  /// перезагружает, и окно осталось бы на прежнем языке (2026-10-15).
  async setLang(lang) {
    await this.eval(`localStorage.setItem("kvartik.lang", ${JSON.stringify(lang)}); return true;`);
    await this.reload();
  }

  /// Нажать видимую кнопку, ссылку или подпись по тексту или `aria-label`.
  /// Спрятанные экраны лежат в странице с такими же кнопками — их не трогает.
  async click(text) {
    const r = await this.eval(`const want = ${JSON.stringify(text)};
      const el = [...document.querySelectorAll("button, a, label, [role=button]")].find((e) =>
        (e.textContent.trim() === want || e.getAttribute("aria-label") === want) && e.getClientRects().length > 0);
      if (!el) return { error: "нет видимой кнопки «" + want + "»" };
      el.click(); return true;`);
    if (r !== true) throw new Error(r?.error ?? `не нажалось «${text}»`);
  }

  /// Ввести текст в поле по подписи (первый текст `<label>`) или селектору:
  /// `value` и событие `input` — его слушает `bind:value` Leptos.
  async type(label, text) {
    const r = await this.eval(`const want = ${JSON.stringify(label)};
      let el = [...document.querySelectorAll("label")].filter((l) => l.getClientRects().length > 0)
        .find((l) => (l.firstChild?.textContent ?? "").trim() === want)?.querySelector("input, textarea, select");
      if (!el) { try { el = [...document.querySelectorAll(want)].find((e) => e.getClientRects().length > 0); } catch {} }
      if (!el) return { error: "нет видимого поля «" + want + "»" };
      el.value = ${JSON.stringify(text)};
      el.dispatchEvent(new Event("input", { bubbles: true }));
      el.dispatchEvent(new Event("change", { bubbles: true }));
      return true;`);
    if (r !== true) throw new Error(r?.error ?? `не ввелось в «${label}»`);
  }

  /// Выбрать файлы в `<input type=file>` по селектору (спрятанный — тоже).
  async setFiles(selector, files) {
    const { result } = await this.send("Runtime.evaluate", { expression: `document.querySelector(${JSON.stringify(selector)})` });
    if (!result.objectId) throw new Error(`нет поля ${selector}`);
    await this.send("DOM.setFileInputFiles", { files, objectId: result.objectId });
  }

  /// Снимок всей страницы в `file`. Возвращает `{ scroll, client }` — ширины
  /// документа и окна без полосы прокрутки: `scroll` больше `client` —
  /// страница едет вбок, и это дефект. На телефоне полосы нет, и `client`
  /// равен эмулированной ширине — иначе эмуляция не та.
  async shot(file) {
    await this.send("Page.bringToFront");
    await this.eval(`await document.fonts.ready; return true;`);
    const size = await this.eval(`const d = document.documentElement;
      return { w: d.scrollWidth, c: d.clientWidth, h: d.scrollHeight };`);
    const width = this.device?.width ?? size.w;
    const { data } = await this.send("Page.captureScreenshot", {
      format: "png", captureBeyondViewport: true,
      clip: { x: 0, y: 0, width, height: Math.min(size.h, 16000), scale: 1 },
    });
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, Buffer.from(data, "base64"));
    return { scroll: size.w, client: size.c };
  }

  /// Снимок одного элемента по селектору — когда страница длинная, а смотреть
  /// надо раздел.
  async shotOf(file, selector) {
    await this.send("Page.bringToFront");
    const box = await this.eval(`const r = document.querySelector(${JSON.stringify(selector)})?.getBoundingClientRect();
      return r ? { x: r.x + scrollX, y: r.y + scrollY, width: r.width, height: r.height } : null;`);
    if (!box) throw new Error(`нет элемента ${selector}`);
    const { data } = await this.send("Page.captureScreenshot", { format: "png", captureBeyondViewport: true, clip: { ...box, scale: 1 } });
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, Buffer.from(data, "base64"));
  }

  async close() {
    await this.browser.send("Target.closeTarget", { targetId: this.targetId }).catch(() => {});
    if (this.contextId) await this.browser.send("Target.disposeBrowserContext", { browserContextId: this.contextId }).catch(() => {});
  }
}

/// Проверки сценария: провалы собираются и выдаются в конце (Правило 2).
export class Checks {
  constructor() { this.results = []; }
  check(name, ok, detail = "") {
    this.results.push({ name, ok: !!ok, detail });
    console.log(`${ok ? "OK  " : "FAIL"} ${name}${detail ? ` — ${detail}` : ""}`);
    return !!ok;
  }
  get failed() { return this.results.filter((r) => !r.ok).length; }
  summary() {
    for (const r of this.results.filter((r) => !r.ok)) console.log(`ПРОВАЛ ${r.name}${r.detail ? ` — ${r.detail}` : ""}`);
    console.log(`итог: ${this.results.length - this.failed} из ${this.results.length}`);
  }
}
