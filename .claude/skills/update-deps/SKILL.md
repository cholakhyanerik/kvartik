---
name: update-deps
description: Обновление зависимостей и программ сборки Kvartik (Правило 3 CLAUDE.md) — cargo update и cargo upgrade, новый Rust, что сверять после обновления Leptos, wasm-bindgen, wasm_split_cli_support, js-sys, uuid, Trunk, binaryen, cargo-deny, wrangler; где на машине лежат wasm-bindgen, wasm-opt, wasm-bindgen-test-runner и chromedriver и как их брать. Брать, когда cargo outdated показал обновления, rustc новее rust-version, Chrome обновился и браузерные тесты пишут «session not created», или Trunk не может скачать программу.
---

# Обновление зависимостей

Правило и его причины — в CLAUDE.md, Правило 3: обновлять, а не откатывать;
сломалось — адаптировать код; сверять по исходникам в реестре
(`~/.cargo/registry/src/*/`), а не по памяти. Здесь — как.

## Порядок

```
cargo update            # в рамках semver, из Cargo.lock
cargo upgrade           # поднять сами требования версий в Cargo.toml
```

Потом — семь проверок Правила 2 (skill `finish-task`, шаг 1; `cargo deny
check` здесь обязателен: новые лицензии, дубли и записи RustSec), потом —
открыть приложение и пройти сценарии Правила 2 (skill `browser-run`),
выпустить PDF образца. В отчёт — что обновилось и что пришлось адаптировать.

**Новый Rust.** `rust-version` в корневом `Cargo.toml` — major.minor
установленного `rustc`; патч числа не меняет. Разошлись — поднять
`rust-version` и число в README, `cargo update` (резолвер edition 2024
подбирает зависимости под `rust-version`), проверки, адаптировать код, а не
глушить `allow`.

**Что `cargo outdated` показывает, а поднимать не надо** (2026-10-16): у
`kvartik-sw` — `wasmparser` и `wasm-encoder` 0.262, `sha2` 0.11, `base64`
0.23. Их версия — та, что уже в дереве через `wasm_split_cli_support`
и Leptos (комментарии в `crates/sw/Cargo.toml`): своя новее дала бы второй
экземпляр крейта в сборке. Поднимаются вместе с тем, кто их тянет.

## Что сверять после обновления какого крейта

- **Leptos** меняет API между минорными версиями: 0.7 переписала
  реактивность и разметку целиком. На 2026-10-10 последняя стабильная —
  0.8.22 (crates.io, от 2026-10-05), и рядом уже 0.9.0-beta2. Примеры из
  интернета под 0.6 выглядят иначе, и копировать их нельзя. Фич по
  умолчанию у `leptos` нет: нужна ровно `csr`.
- **wasm-bindgen:** версия крейта в `Cargo.lock` и версия программы
  `wasm-bindgen`, которой обрабатывается wasm, должны совпадать. Wasm окна
  с 0.0.18 обрабатывает хук сборки (`crates/sw/src/wasm.rs`) программой
  `wasm-bindgen` из PATH и сам сверяет её `--version` с замком. Wasm,
  который собирал бы Trunk (воркер; с 0.0.25 PDF обходится без него), — его
  программой: Trunk подбирает её из `[tools] wasm_bindgen` в `Trunk.toml`,
  иначе из `Cargo.lock` рядом с манифестом крейта, иначе из `cargo metadata`
  (по исходникам, trunk 0.22.0-rc.2, `src/pipelines/rust/wasm_bindgen.rs`);
  закреплять её в `Trunk.toml` не надо: число разойдётся с замком при первом
  `cargo update`. С той же версией поднимаются `wasm-bindgen-test` (в его
  Cargo.toml — точные `=`, пара 0.3.79 ↔ 0.2.129) и программы `wasm-bindgen`
  и `wasm-bindgen-test-runner` — на машине (ниже) и в CI
  (`WASM_BINDGEN_VERSION` и сумма в `ci.yml`; шаг сверяет её с замком).
  Раннер соседней версии может пройти молча: сверяется только номер схемы
  (по исходникам wasm-bindgen-cli-support 0.2.129).
- **Разрез wasm на части держится за внутренности wasm-bindgen**
  (`wasm_split_cli_support` 0.2.3; на wasm-bindgen 0.2.128 он уже ломался —
  leptos#4870). После обновления wasm-bindgen, Leptos или этого крейта —
  `trunk build --release` и формы в браузере: хук сверяет части с главным
  модулем и упадёт, если они не сходятся, но то, что часть делает после
  загрузки, видно только в окне. Разрезу нужны связи компоновщика —
  `-Clink-args=--emit-relocs` в `.cargo/config.toml`; переменная окружения
  `RUSTFLAGS` перебивает эту строку целиком, и сайт с ней не соберётся. Клей
  wasm-bindgen хук дописывает запуском после строки
  `export { initSync, __wbg_init as default };` (0.2.129): сменит
  wasm-bindgen последнюю строку — хук скажет это и остановит сборку.
- **На продолжении после `await` держится транзакция IndexedDB.** Хранилище
  окна ждёт свои запросы внутри транзакции и ставит следующий после
  `await`: это работает, только пока `js-sys` будит задачу Rust микрозадачей
  (`queueMicrotask`, `src/futures/queue.rs` в js-sys 0.3.106; с 0.4.79
  `wasm-bindgen-futures` — лишь оболочка над ним). Документацией
  wasm-bindgen это не обещано; сменится — сломаются браузерные тесты
  хранилища, и их зелёный прогон после обновления `js-sys` и обязателен.
- **Случайность на wasm32 берёт `uuid`, а не getrandom.** `uuid` 1.28 на
  `wasm32-unknown-unknown` getrandom не зовёт: `v4` берёт случайные байты из
  WebCrypto по своей фиче `js`, а без неё (или `rng-getrandom`, `rng-rand`)
  не собирается — `compile_error!` в `src/rng.rs` (по исходникам uuid
  1.28.0, 2026-10-10). Ядро передаёт её своей фичей `js`, по умолчанию
  выключенной: библиотека браузерный источник сама не выбирает (README
  getrandom 0.4.3). Включает `kvartik-core/js` каждый, кто собирает ядро под
  wasm32: окно, калькулятор лендинга, браузерный тест ядра — без неё сборка
  падает. У самого getrandom способ менялся между 0.2, 0.3 и 0.4; в 0.4 на
  wasm32 хватает фичи `wasm_js`, флага `--cfg getrandom_backend` больше не
  нужно (по исходникам 0.4.3). После обновления `uuid` сверить заново:
  источник случайности он может сменить в минорном выпуске (так пишет его
  документация 1.28.0).
- **ed25519-dalek:** 3.0.0 вышла 2026-07-06 (crates.io); примеры под 2.x
  могут не собраться.
- **Trunk — 0.22.0-rc.3** (2026-10-16), а не стабильная 0.21.14
  (2025-05-08); выбор по исходникам 0.21 и rc.2, 2026-10-10. В `src/` rc.3
  от rc.2 отличается только флагом `-C` и подпротоколами WebSocket у прокси
  `trunk serve` (по исходникам обеих, 2026-10-16): записи «по исходникам
  rc.2» здесь и в коде верны и для неё. Формат `Trunk.toml`
  у них один, а в 0.22 имена файлов с хэшем считаются от выходных байтов
  и не расходятся с SRI (trunk-rs/trunk#1028) — на этом держится кэш service
  worker; wasm-opt по умолчанию version_131 вместо version_123 (её же
  с 0.0.18 берёт хук сборки — `WASM_OPT_VERSION` в `crates/sw/src/wasm.rs`
  и `BINARYEN_VERSION` в `ci.yml`). Wasm окна Trunk не собирает:
  `data-trunk-skip` в `index.html`, а без него он нашёл бы `Cargo.toml`
  рядом со страницей и собрал бы свой, неразрезанный (по исходникам
  0.22.0-rc.2, `src/pipelines/html.rs`). Версия записана в трёх местах:
  `trunk-version` в `crates/app/Trunk.toml`, `TRUNK_VERSION` и сумма архива
  в `.github/workflows/ci.yml`. Выйдет 0.22.0 — перейти на неё. Ставится
  `cargo install trunk --locked --version 0.22.0-rc.3`: без `--version`
  cargo берёт стабильную 0.21.14, а с ней `Trunk.toml` собирать откажется.
- **Раскладку сайта, wasm окна и `sw.js` делает хук Trunk `post_build`**
  (`cargo run -p kvartik-sw -- --root public/root --wasm kvartik-app
  public/sw.js`): он идёт из папки `Trunk.toml`, когда `index.html` уже
  дописан, а папка сборки — в `TRUNK_STAGING_DIR`, профиль —
  в `TRUNK_PROFILE`, `public_url` — в `TRUNK_PUBLIC_URL` (по исходникам
  trunk 0.22.0-rc.2, `src/hooks.rs`, `src/pipelines/html.rs`). Trunk кладёт
  всё в корень папки сборки, а `public_url` меняет только адреса в странице;
  программу в `app/` перекладывает хук (с 0.0.34). Поэтому `trunk serve`
  отдаёт сайт от корня (`serve_base = "/"`): по умолчанию он отдавал бы
  папку сборки с адреса `public_url` (`src/serve/mod.rs`). Упал хук — упала
  сборка. После обновления Trunk сверить порядок этапов и посмотреть
  `dist/`: в `app/index.html` — `<script type=module
  src=/app/…kvartik-app-…js>` и ни одного встроенного скрипта, а строка
  `const BUILD = "dev"` в `app/sw.js` — обновления выключены у всех.
- **wrangler выкладки** (задачи 15 и 50) — 4.149.0 из npm,
  в `tools/deploy/package.json` с замком: `npm ci --ignore-scripts` в CI
  ставит всё его дерево по суммам замка, без скриптов установки, а в сайт он
  не попадает. Это самый ранний выпуск без известной уязвимости
  (`npm audit`: `sharp` в 4.146–4.148, 2026-10-15). Новая версия —
  `npm view wrangler version`; поднимать —
  `npm install --package-lock-only --ignore-scripts wrangler@<версия>`
  в `tools/deploy`, `npm audit` и выкладка предпросмотром (pull request): тип
  каждого файла ставит при загрузке wrangler (пакет `mime`), и проверка
  выкладки увидит, если он сменился. Под Claude Code (`CLAUDECODE=1`)
  wrangler считает себя в руках ИИ-агента и `pages project create`
  выкладывает текущую папку в Workers вместо создания проекта (по исходникам
  4.146.0, `detect-agent.ts`): проект и вход в Cloudflare — в терминале
  человека. Тот же wrangler из `tools/deploy/node_modules` — сервер
  глазного прогона (skill `browser-run`).
- **Браузерные API** — по MDN и caniuse с датой проверки, а не по памяти
  (Правило 6).

## Программы на машине

Инструменты: `cargo-outdated`, `cargo-edit` (даёт `cargo upgrade`), цель
`wasm32-unknown-unknown`, Trunk 0.22.0-rc.3 (с 2026-10-16) и cargo-deny
0.20.2 (тот же выпуск, что в `ci.yml`) на машине есть (на 2026-10-10). Базу RustSec
cargo-deny с этой машины скачивает: беды Trunk с DNS у него нет (проверено
2026-10-10, 0.20.2).

Для сборки сайта в `~/.cargo/bin` (он в PATH) лежат (2026-10-13)
`wasm-bindgen` 0.2.129 — из того же архива, что раннер ниже, — и `wasm-opt`
binaryen version_131 — из `binaryen-version_131-x86_64-windows.tar.gz`
(github.com/WebAssembly/binaryen); суммы sha256 обоих архивов сверены
с файлами выпуска и с полем `digest` в API GitHub. Без них `trunk build`
останавливается и говорит, какой программы не хватает.

Для браузерных тестов в `~/.cargo/bin` лежат (2026-10-12):
`wasm-bindgen-test-runner` 0.2.129 — из архива выпуска wasm-bindgen
`wasm-bindgen-0.2.129-x86_64-pc-windows-msvc.tar.gz` (github.com/
wasm-bindgen/wasm-bindgen), сумма sha256 сверена с файлом выпуска;
и chromedriver 155.0.8059.40 — из Chrome for Testing
(`storage.googleapis.com/chrome-for-testing-public/<версия>/win64/
chromedriver-win64.zip`; сумм CfT не публикует, md5 сверен по заголовку
`x-goog-hash`). Драйвер годится только к Chrome той же сборки
(major.minor.build): Chrome обновлялся до 155.0.8059.80 (2026-10-14)
и 155.0.8059.85 (2026-10-16), и драйвер .40 с ними работает. Сменится
сборка — тесты упадут с «session not created», и драйвер берётся заново той
же версии, что Chrome. Версия Chrome — имя папки рядом с `chrome.exe`:
`chrome.exe --version` на Windows открывает окно вместо ответа. Раннер ищет
драйвер в PATH и первым берёт geckodriver: появится он — переменная
`CHROMEDRIVER` выберет Chrome явно (так в CI).

Шрифты пересобирает `tools/fonts.py` — Python 3 и fontTools 4.60.1 в своём
окружении `.venv-fonts/` (как — в шапке скрипта); сборке и CI они не нужны:
готовые файлы лежат в репозитории. Так же таблицу стран
(`crates/core/src/model/country/names.rs`) пишет `tools/countries.py` из
CLDR — ему хватает стандартной библиотеки Python.

Hooks и скрипты skills — Node 24 без зависимостей (на машине 24.21.0,
2026-10-16).

## Trunk не может скачать программу

Trunk сам скачивает wasm-bindgen и wasm-opt своим резолвером DNS (hickory),
а роутер на машине автора отвечает ему `Form Error`, хотя curl и nslookup
работают (проверено 2026-10-10, trunk 0.22.0-rc.2). С 0.0.18 Trunk wasm не
собирает и их не качает, а PDF (0.0.25) обошёлся без воркера; вернётся это
с первым wasm, который соберёт Trunk. Обход: скачать архивы с тех же
адресов curl'ом и положить программы в кэш Trunk —
`%LOCALAPPDATA%\trunkrs\trunk\cache\wasm-bindgen-<версия>\wasm-bindgen.exe`
и `…\cache\wasm-opt-version_<N>\bin\wasm-opt.exe` — или в PATH: той же
версии Trunk берёт оттуда. В CI это не нужно.

## Скачанное

Архив программы — в свою пустую папку `C:\tmp\k<номер>\…`, сумма sha256 —
сверить с файлом выпуска до запуска; в CI — закрепить суммой, как Trunk
и cargo-deny в `ci.yml` (Правило 13). Убрать после задачи (Правило 11).
