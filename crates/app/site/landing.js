// Лендинг (задача 34): единственный скрипт страниц kvartik.com. Файлом,
// а не в странице: встроенных скриптов CSP не пускает
// (crates/app/public/root/_headers). Сборка кладёт его в корень под именем
// с хэшем (crates/sw/src/landing.rs), и страница без него — та же страница.
//
// До 0.0.34 программа жила в корне сайта, и её адреса — `/#/…`. Сюда
// приходят закладка на её экран и перезагрузка прежней программы, когда
// service worker корня снял себя (crates/app/public/root/sw.js). Раньше их
// вёл в /app/ сам корень переадресацией, а теперь в корне лендинг, и `#/…`
// сервер не видит. Поэтому экран программы — в /app/, с тем же `#/…`
// и строкой запроса, как раньше. `replace`, а не переход: «Назад» не
// вернёт на страницу, которую человек не открывал.
if (location.hash.startsWith("#/")) {
  location.replace(`/app/${location.search}${location.hash}`);
}

// Калькулятор: форма — в разметке страницы, а считает и пишет ответ wasm
// ядра (crates/calc, `tax::example`) — тот же расчёт, что у программы.
// Здесь ни чисел, ни слов: поля уходят в wasm строкой JSON, а ответ — уже
// словами языка страницы — ложится в разметку только текстом (CLAUDE.md,
// Правило 13). Wasm — около 90 КБ после сжатия, поэтому грузится он при
// первом касании формы, а не у каждого, кто открыл страницу (Правило 1):
// пока человек вводит оборот, он успевает прийти. Клей и wasm просятся
// вместе: клей сам попросил бы wasm только после себя — на круг сети позже.
// Не пришли — слова об этом из разметки, и следующее касание пробует снова.
const calc = document.querySelector("section[data-module]");
if (calc) {
  const fields = [...calc.querySelectorAll("[name]")];
  const result = calc.querySelector(".result");
  const state = (name, shown) => {
    calc.querySelector(`[data-state="${name}"]`).hidden = !shown;
  };
  let module = null;
  let loading = null;

  // Ответ ядра: какие поля показать, ошибки полей и блоки ответа.
  const show = (answer) => {
    for (const [name, needed] of Object.entries(answer.fields)) {
      calc.querySelector(`[data-field="${name}"]`).hidden = !needed;
    }
    for (const field of fields) {
      const text = answer.errors[field.name] ?? "";
      const error = calc.querySelector(`[data-error="${field.name}"]`);
      error.textContent = text;
      error.hidden = !text;
      if (text) field.setAttribute("aria-invalid", "true");
      else field.removeAttribute("aria-invalid");
    }
    result.replaceChildren(...answer.blocks.map(({ kind, text }) => {
      const block = document.createElement(kind === "head" ? "h3" : "p");
      block.className = kind;
      block.textContent = text;
      return block;
    }));
  };

  const values = () => JSON.stringify(Object.fromEntries(fields.map((field) => [field.name, field.value])));

  const update = () => {
    if (!module) return;
    const answer = module.calculate(document.documentElement.lang, values());
    if (answer) show(JSON.parse(answer));
  };

  // «Открыть Kvartik» берёт введённое с собой: wasm оставляет его записью
  // на этом устройстве, а программа предложит взять его в профиль. В самом
  // нажатии: запись `localStorage` синхронна и ложится до перехода. Wasm
  // нет — нечего и брать: примера не было.
  for (const link of document.querySelectorAll("a[data-handoff]")) {
    link.addEventListener("click", () => {
      if (module) module.handoff(document.documentElement.lang, values());
    });
  }

  const load = () => {
    if (module || loading) return;
    state("failed", false);
    state("loading", true);
    const wasm = fetch(calc.dataset.wasm);
    // Отказ запроса wasm доходит до `default` ниже; без этого обработчика
    // браузер ещё и назвал бы его необработанным, если клей не пришёл.
    wasm.catch(() => {});
    loading = import(calc.dataset.module)
      .then(async (loaded) => {
        await loaded.default({ module_or_path: wasm });
        module = loaded;
        update();
      })
      .catch(() => state("failed", true))
      .finally(() => {
        state("loading", false);
        loading = null;
      });
  };

  calc.addEventListener("focusin", load);
  calc.addEventListener("pointerdown", load);
  calc.addEventListener("input", update);
  calc.addEventListener("change", update);
}
