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

// Переход между страницами (landing.css, `@view-transition`) — только между
// страницами языков. Браузер начинает его и по ссылке в программу или на
// тексты лицензий, а они переходов не объявляют: Chrome отменяет его уже
// в новой странице и отвергает там обещание `ready`, которого никто не ждёт,
// — ошибка в консоли программы (проверено вживую: Chrome 157, 2026-11-06).
// Поэтому уходящая страница сама снимает переход, когда идёт не на страницу
// языка: тогда у новой его нет вовсе. Куда идёт, говорит `activation`; не
// сказал браузер — переход остаётся. Адреса языков — из переключателя.
addEventListener("pageswap", (event) => {
  const transition = event.viewTransition;
  if (!transition) return;
  transition.ready.catch(() => {});
  const to = event.activation?.entry?.url;
  const languages = [...document.querySelectorAll(".langs a")].map((link) => link.href);
  if (to && !languages.includes(to.split("#")[0])) transition.skipTransition();
});

// Движение за мышью (задача 86) — то же, что у программы (`ui/motion.rs`):
// кнопка заливается из точки, где навели курсор, главная тянется к нему,
// карточка наклоняется вслед, а кружок у курсора догоняет стрелку. Как это
// выглядит, решает оформление (landing.css): здесь только точка мыши —
// переменными CSS на самом элементе — и место кружка, а без мыши и при
// выключенной в системе анимации стили не рисуют ничего. Палец и перо не в
// счёт: наклон и магнит под пальцем только мешали бы. Работа — только пока
// мышь движется: в покое ни таймеров, ни опросов (CLAUDE.md, Правило 1).
{
  // Над чем кружок растёт и над чем его нет: над полем ввода целятся
  // текстовым курсором.
  const press = "a[href], button, label, select, summary";
  const typing = "input:not([type=checkbox], [type=radio]), textarea";
  // Кружок — только если его включили в настройках программы (задача 88):
  // запись устройства у лендинга и программы общая, а по умолчанию кружка
  // нет. `localStorage` закрыт — кружка тоже нет.
  let circle = false;
  try {
    circle = localStorage.getItem("kvartik.cursor") === "1";
  } catch {
    circle = false;
  }
  const cursor = circle ? document.createElement("div") : null;
  if (cursor) {
    cursor.className = "cursor";
    cursor.setAttribute("aria-hidden", "true");
    document.body.append(cursor);
  }
  const over = (value) => {
    if (!cursor) return;
    if (value === null) delete cursor.dataset.over;
    else cursor.dataset.over = value;
  };

  // Кнопка и карточка под мышью — с местом на странице, взятым при входе:
  // кнопка-магнит сама едет к мыши, и её нынешнее место тянуло бы её
  // дальше.
  let button = null;
  let card = null;
  const place = (element) => {
    const box = element.getBoundingClientRect();
    return { element, left: box.left + scrollX, top: box.top + scrollY, width: box.width, height: box.height };
  };
  // Точка мыши в долях ширины и высоты элемента, от 0 до 1.
  const share = (at, event) => [
    Math.min(Math.max((event.pageX - at.left) / at.width, 0), 1),
    Math.min(Math.max((event.pageY - at.top) / at.height, 0), 1),
  ];
  const write = (element, names, [x, y]) => {
    element.style.setProperty(names[0], x.toFixed(3));
    element.style.setProperty(names[1], y.toFixed(3));
  };
  const POINTER = ["--pointer-x", "--pointer-y"];
  const PULL = ["--pull-x", "--pull-y"];

  document.addEventListener("pointerover", (event) => {
    if (event.pointerType !== "mouse") {
      over(null);
      return;
    }
    const target = event.target;
    over(target.closest(typing) ? "text" : target.closest(press) ? "press" : "");
    const nextButton = target.closest(".button");
    if (nextButton && nextButton !== button?.element) {
      button = place(nextButton);
      write(nextButton, POINTER, share(button, event));
    }
    const nextCard = target.closest("[data-tilt]");
    if (nextCard && nextCard !== card?.element) card = place(nextCard);
  });

  document.addEventListener("pointerout", (event) => {
    if (event.pointerType !== "mouse") return;
    const next = event.relatedTarget;
    // Мышь ушла из окна — кружок прячется до её возвращения.
    if (!next) over(null);
    // Ушла с кнопки — заливка сжимается в точку ухода, кнопка — на место.
    if (button && !button.element.contains(next)) {
      write(button.element, POINTER, share(button, event));
      write(button.element, PULL, [0, 0]);
      button = null;
    }
    // С карточки — она выпрямляется.
    if (card && !card.element.contains(next)) {
      write(card.element, POINTER, [0.5, 0.5]);
      card = null;
    }
  });

  document.addEventListener("pointermove", (event) => {
    if (event.pointerType !== "mouse") return;
    cursor?.style.setProperty("translate", `${event.clientX}px ${event.clientY}px`);
    // Тяга магнита — от −1 до 1: от середины кнопки к мыши.
    if (button) {
      const [x, y] = share(button, event);
      write(button.element, PULL, [x * 2 - 1, y * 2 - 1]);
    }
    if (card) write(card.element, POINTER, share(card, event));
  });
}
