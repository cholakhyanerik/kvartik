#!/usr/bin/env bash
# Проверка выкладки (задача 15): отдаёт ли хостинг ровно эту сборку и так,
# как она задумана. Зовёт её CI сразу после выкладки (`.github/workflows/
# ci.yml`), а можно и руками — с адресом и собранной папкой:
#
#   tools/deploy/check.sh https://kvartik.com crates/app/dist
#
# Папка — та, что выложена: с живым сайтом сверяется артефакт `dist`
# прогона CI (`gh run download <прогон>`). Сборка на Windows байт в байт
# с ним не совпадает: в wasm записаны пути исходников (2026-10-15).
#
# Зачем: заголовки решает хостинг, а не сборка, и молча (CLAUDE.md,
# Правило 6). Без CSP «ничего не уходит с устройства» держится только на
# честности кода (Правило 13); `immutable` у файла без хэша в имени оставил
# бы людям старый файл; wasm без сжатия — первый заход за целью в 3 с
# (Правило 1). Ни сборка, ни тесты этого не видят — только запрос к живому
# сайту. Провалы копятся и выводятся все в конце (Правило 2).
set -euo pipefail

if [ $# -ne 2 ]; then
  echo "использование: $0 <адрес сайта> <папка сборки>" >&2
  exit 2
fi
site="${1%/}"
dist="$2"
headers_file="$dist/_headers"
# Программа живёт в папке сайта, корень — лендингу (задача 65).
app=app
[ -f "$dist/$app/index.html" ] || { echo "в $dist нет $app/index.html" >&2; exit 2; }
[ -f "$headers_file" ] || { echo "в $dist нет _headers" >&2; exit 2; }

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
failures=()
fail() { failures+=("$1"); }

# Заголовок ответа без учёта регистра имени: последнее значение, без `\r`.
header() {
  grep -i "^$1:" "$2" | tail -n 1 | cut -d: -f2- | sed 's/^ *//' | tr -d '\r' || true
}

# Выкладка доходит до края сети не вся разом: на kvartik.com страница /app/
# была уже новой, а часть файлов, `/app` и `404.html` ещё отвечали
# прежней выкладкой — 404 и её телами; через пять минут всё было новым
# (2026-10-16). Поэтому ответ прежней выкладки повторяется, пока не выйдут
# три минуты от первой страницы этой сборки (`settle_until` ставится, когда
# она пришла), и провал — только устойчивый. `settling` ждёт три секунды
# и отвечает, есть ли ещё время.
settle_until=0
settling() {
  [ "$(date +%s)" -lt "$settle_until" ] || return 1
  sleep 3
}

# Ответ на GET: код, заголовки — в файл, тело — в файл. `Accept-Encoding`
# — как у браузера; тело не разжимается, чтобы видеть, сжал ли хостинг.
# Сразу после выкладки край Cloudflare бывает отвечает 5xx: у первой
# выкладки 16 файлов из 79 пришли с 522 («не дождался источника»), а через
# минуту все — с 200 и верными заголовками (2026-10-15). Поэтому 5xx
# и обрыв связи (000) повторяются до пяти раз, а 404 — пока выкладка
# доходит (выше): файл из сборки есть, и 404 на него — ответ прежней.
fetch() {
  local code attempt=1
  while :; do
    code="$(curl -sS -o "$work/body" -D "$work/head" -w '%{http_code}' \
      -H 'Accept-Encoding: br, gzip' "$site/$1" || true)"
    case "$code" in
      5?? | 000) [ "$attempt" -lt 5 ] || break; sleep 3 ;;
      404) settling || break ;;
      *) break ;;
    esac
    attempt=$((attempt + 1))
  done
  echo "$code"
}

# CSP — та строка, что в `_headers` у правила `/*`: проверяется, что
# хостинг отдаёт её целиком, а не своё.
csp="$(sed -n 's/^ *Content-Security-Policy: *//p' "$headers_file" | head -n 1 | tr -d '\r')"
[ -n "$csp" ] || { echo "в _headers нет Content-Security-Policy" >&2; exit 2; }

# Выкладка доходит до края сети не мгновенно: первый запрос ждёт страницу
# этой сборки до минуты. Страница — с `Accept` браузера: зона Cloudflare
# вписывает свой скрипт (счётчик Web Analytics) только в ответ на
# `text/html`, а на `*/*` curl'а отдаёт страницу нетронутой (проверено
# 2026-10-15 на kvartik.com).
page_hash="$(sha256sum "$dist/$app/index.html" | cut -d' ' -f1)"
for attempt in $(seq 1 12); do
  code="$(curl -sS --compressed -o "$work/page" -D "$work/page-head" -w '%{http_code}' \
    -H 'Accept: text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8' "$site/$app/" || true)"
  if [ "$code" = 200 ] && [ "$(sha256sum "$work/page" | cut -d' ' -f1)" = "$page_hash" ]; then
    break
  fi
  if [ "$attempt" = 12 ]; then
    echo "$site/$app/: $code, страница не этой сборки" >&2
    grep -o '<script[^>]*src="https\?://[^"]*"' "$work/page" | sed 's/^/  чужой скрипт: /' >&2 || true
    exit 1
  fi
  sleep 5
done
settle_until=$(($(date +%s) + 180))

# Страница — байт в байт та, что собрана: хостинг ничего в неё не вписал
# (счётчик Web Analytics, включённый в панели, вписал бы скрипт).
check_page() {
  local got
  got="$(header content-security-policy "$work/page-head")"
  [ "$got" = "$csp" ] || fail "/$app/: CSP «$got», а в _headers «$csp»"
  for expected in 'x-content-type-options: nosniff' 'referrer-policy: no-referrer' \
    'cross-origin-opener-policy: same-origin'; do
    got="$(header "${expected%%:*}" "$work/page-head")"
    [ "$got" = "${expected#*: }" ] || fail "/$app/: ${expected%%:*} «$got», а ждали «${expected#*: }»"
  done
  got="$(header cache-control "$work/page-head")"
  case "$got" in
    *immutable* | *max-age=[1-9]*) fail "/$app/: Cache-Control «$got» — страница без хэша в имени" ;;
  esac
}
check_page

# Корень и страницы языков — лендинг (задача 34, с 0.0.82), юридические
# тексты к каждой (задача 32, с 0.0.94), страница «Купить» (задача 35,
# с 0.0.96), страница «Отзывы и предложения» (задача 114, с 0.0.164)
# и прежние редакции текстов по своим адресам (задача 93,
# с 0.0.123; условия, п. 11) — какие собраны, а у условий одна есть
# точно: 200 и байт в байт та
# страница, что собрана, — с CSP и без долгого кэша. Зона вписывает свой
# скрипт в любой ответ `text/html` (выше), и на лендинге тоже. До 0.0.82
# корень вёл в программу переадресацией 302.
check_landing() {
  local path code got earlier
  earlier="$(cd "$dist" && find . \( -path './*terms/????-??-??/index.html' -o -path './*privacy/????-??-??/index.html' \) \
    | sed 's|^\./||; s|index\.html$||' | sort)"
  [ -n "$earlier" ] || fail "в сборке нет прежних редакций условий (terms/<день>/)"
  # shellcheck disable=SC2086 # адреса без пробелов: делить их по строкам и нужно
  for path in "" ru/ en/ buy/ ru/buy/ en/buy/ feedback/ ru/feedback/ en/feedback/ terms/ ru/terms/ en/terms/ privacy/ ru/privacy/ en/privacy/ $earlier; do
    while :; do
      code="$(curl -sS --compressed -o "$work/landing" -D "$work/landing-head" -w '%{http_code}' \
        -H 'Accept: text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8' "$site/$path" || true)"
      if [ "$code" = 200 ] && cmp -s "$work/landing" "$dist/${path}index.html"; then
        break
      fi
      settling || break
    done
    [ "$code" = 200 ] || fail "/$path: $code, а ждали 200"
    cmp -s "$work/landing" "$dist/${path}index.html" || fail "/$path: страница не этой сборки"
    got="$(header content-security-policy "$work/landing-head")"
    [ "$got" = "$csp" ] || fail "/$path: CSP «$got», а в _headers «$csp»"
    got="$(header cache-control "$work/landing-head")"
    case "$got" in
      *immutable* | *max-age=[1-9]*) fail "/$path: Cache-Control «$got» — страница без хэша в имени" ;;
    esac
  done
}
check_landing

# Адрес программы без косой черты — на неё же, 308: относительные ссылки
# страницы с `/app` ушли бы в корень. Строка запроса — с собой.
check_redirect() {
  local got location
  while :; do
    got="$(curl -sS -o /dev/null -D "$work/redirect-head" -w '%{http_code}' "$site/$app?q=1" || true)"
    location="$(header location "$work/redirect-head")"
    location="${location#"$site"}"
    [ "$got $location" != "308 /$app/?q=1" ] || break
    settling || { fail "/$app?q=1: «$got $location», а ждали 308 на /$app/?q=1"; break; }
  done
}
check_redirect

# Свой домен (задача 16). Его зона Cloudflare стоит перед Pages и меняет
# ответы, а зона `*.pages.dev` — не наша, и там этого не проверить.
# Отчёты о сбоях (`Nel`, `Report-To`) браузер слал бы на
# a.nel.cloudflare.com (CLAUDE.md, Правило 13), а второй CSP «только
# отчёт» (Client-side security, бывший Page Shield) — список скриптов
# страницы в Cloudflare; `Speculation-Rules` (Speed Brain) загружал бы
# страницы заранее. HSTS — сайт только по https. А http
# и `www` — переадресацией сюда же, с путём и строкой запроса: программа
# на втором адресе завела бы человеку данные второго происхождения.
host="${site#*://}"
host="${host%%/*}"
check_own_domain() {
  local got name from
  for name in nel report-to content-security-policy-report-only speculation-rules; do
    got="$(header "$name" "$work/page-head")"
    [ -z "$got" ] || fail "/: заголовок $name «$got» — зона его ставить не должна"
  done
  got="$(header strict-transport-security "$work/page-head")"
  [[ "$got" == *max-age=[1-9]* ]] || fail "/: Strict-Transport-Security «$got», а ждали max-age"
  for from in "http://$host" "https://www.$host"; do
    got="$(curl -sS -o /dev/null -w '%{http_code} %{redirect_url}' "$from/kvartik-path?q=1" || true)"
    [ "$got" = "301 https://$host/kvartik-path?q=1" ] \
      || fail "$from: «$got», а ждали 301 на https://$host/kvartik-path?q=1"
  done
}
case "$host" in
  *.pages.dev) ;;
  *) check_own_domain ;;
esac

# Неизвестный адрес — 404.html с кодом 404 и без кэша, а не страница
# программы с 200: иначе пропавший файл с хэшем лёг бы в кэш браузера
# страницей с `immutable` на год (шапка 404.html). И в корне, и в папке
# программы: 404.html один, в корне, и Pages ищет его вверх по папкам.
check_missing() {
  local code got path
  for path in "kvartik-no-such-file-0000000000000000.js" "$app/kvartik-no-such-file-0000000000000000.js"; do
    while :; do
      code="$(curl -sS --compressed -o "$work/missing" -D "$work/missing-head" -w '%{http_code}' \
        "$site/$path" || true)"
      if [ "$code" = 404 ] && cmp -s "$work/missing" "$dist/404.html"; then
        break
      fi
      settling || break
    done
    [ "$code" = 404 ] || fail "/$path: $code, а ждали 404"
    cmp -s "$work/missing" "$dist/404.html" || fail "/$path: тело — не 404.html этой сборки"
    got="$(header cache-control "$work/missing-head")"
    [[ "$got" == *no-store* ]] || fail "/$path: Cache-Control «$got», а ждали no-store"
  done
}
check_missing

# Сервера пробного периода больше нет (задача 113): `POST /api/trial` функция
# сайта не принимает (`_routes.json` её на этот адрес не зовёт), и хостинг
# отвечает на него сам — 404 или 405, а не днём пробного периода. Прежняя
# версия программы из кэша ещё спросит этот адрес: без ответа 2xx она считает
# срок по устройству, как без сети. Сразу после выкладки край сети минуты
# отдаёт прежнюю функцию (Правило 6, «Хостинг отвечает успехом не тем»), а без
# её базы D1 (её удаляют после выкладки, задача 113) прежняя отвечает 503, —
# поэтому 2xx и 5xx повторяются до трёх минут, обрыв — до пяти раз, а провал —
# только устойчивый. ՀՎՀՀ вопроса — восемь нулей: такого у ИП нет (`license::NO_TIN`).
check_no_trial() {
  local code errors=1 until=$(($(date +%s) + 180))
  while :; do
    code="$(curl -sS -o "$work/trial" -w '%{http_code}' -X POST \
      --data-binary '{"tin":"00000000","consent":"2026-11-18"}' "$site/api/trial" || true)"
    case "$code" in
      2?? | 5??) [ "$(date +%s)" -lt "$until" ] || break; sleep 3 ;;
      000) [ "$errors" -lt 5 ] || break; errors=$((errors + 1)); sleep 3 ;;
      *) break ;;
    esac
  done
  case "$code" in
    404 | 405) ;;
    *) fail "/api/trial: $code «$(head -c 200 "$work/trial")», а ждали 404 или 405: сервера пробного периода нет" ;;
  esac
}
check_no_trial

# Тип по расширению — как его ждут браузер и service worker.
expected_type() {
  case "$1" in
    *.wasm) echo application/wasm ;;
    *.js) echo text/javascript application/javascript ;;
    *.css) echo text/css ;;
    *.ttf) echo font/ttf ;;
    *.png) echo image/png ;;
    *.webmanifest) echo application/manifest+json ;;
    # Заметки «Что нового» (задача 67): тип ставит wrangler при выкладке по
    # таблице `mime` (по исходникам wrangler 4.149.0, 2026-10-18).
    *.json) echo application/json ;;
    *.txt) echo text/plain ;;
    # Общий календарь сроков (задача 22): тип iCalendar — RFC 5545, 8.1;
    # ставит его wrangler по той же таблице `mime`.
    *.ics) echo text/calendar ;;
    # Карта сайта лендинга (задача 34): тип XML — RFC 7303; какой из двух
    # ставит wrangler по таблице `mime`, сверяет первая выкладка.
    *.xml) echo application/xml text/xml ;;
    *) echo "?" ;;
  esac
}

# Каждый файл сборки — и корня, и программы. С хэшем в имени (Trunk и хук
# сборки пишут его шестнадцатью знаками перед расширением) — `immutable`;
# без хэша — без долгого кэша. Тексты и wasm — сжатыми, а wasm программы —
# байт в байт своей копией уровнем brotli 11 (задача 109): её отдаёт функция
# сайта, а сжатие хостинга — около уровня 4 и на 49 КБ больше у главного
# wasm. Сами копии `*.wasm.br` отдельно не перебираются: их тело и есть этот
# ответ. Страницы HTML — выше: на `/404.html` Pages отвечает переадресацией
# на `/404`. Скрытые файлы (`.trunk-lock` Trunk) в выкладку не идут: их не
# берёт артефакт CI.
checked=0
packed=0
while IFS= read -r path; do
  checked=$((checked + 1))
  code="$(fetch "$path" || true)"
  if [ "$code" != 200 ]; then
    fail "$path: $code"
    continue
  fi
  type="$(header content-type "$work/head")"
  type="${type%%;*}"
  ok=0
  for want in $(expected_type "$path"); do
    [ "$type" = "$want" ] && ok=1
  done
  [ "$ok" = 1 ] || fail "$path: Content-Type «$type», а ждали $(expected_type "$path")"
  cache="$(header cache-control "$work/head")"
  if [[ "$path" =~ -[0-9a-f]{16}(_bg)?\.(wasm|js|css)$ ]]; then
    [[ "$cache" == *immutable* ]] || fail "$path: Cache-Control «$cache» — у файла с хэшем ждали immutable"
  else
    case "$cache" in
      *immutable* | *max-age=[1-9]*) fail "$path: Cache-Control «$cache» — файл без хэша в имени" ;;
    esac
  fi
  # `application/json` (заметки «Что нового») — в списке типов, которые
  # Cloudflare сжимает (docs Cloudflare, «Content compression», обновлено
  # 2026-04-17, сверено 2026-10-18); меньше 50 байт он не сжимает ничего.
  case "$path" in
    *.wasm | *.js | *.css | *.ttf | *.webmanifest | *.json)
      encoding="$(header content-encoding "$work/head")"
      [ -n "$encoding" ] || fail "$path: не сжат"
      ;;
  esac
  if [[ "$path" == "$app/"*.wasm ]] && [ -f "$dist/$path.br" ]; then
    packed=$((packed + 1))
    if [ "$(header content-encoding "$work/head")" != br ] || ! cmp -s "$work/body" "$dist/$path.br"; then
      fail "$path: не копия уровнем 11 — по проводу $(wc -c < "$work/body") Б, а копия — $(wc -c < "$dist/$path.br") Б"
    fi
  fi
  # Тело обоих `sw.js` — этой сборки: по `sw.js` программы браузер
  # узнаёт новую версию, а по `sw.js` корня — что прежний service worker
  # корня пора снять. Тело файла курсов ЦБ (задача 43) — этой выкладки: его
  # кладёт выкладка, а не сборка, и выкладка курсов по расписанию меняет
  # только его.
  if [ "${path##*/}" = sw.js ] || [ "$path" = app/rates.json ]; then
    while :; do
      if curl -sS --compressed -o "$work/sw" "$site/$path" && cmp -s "$work/sw" "$dist/$path"; then
        break
      fi
      settling || { fail "$path: не этой сборки или не скачан"; break; }
    done
  fi
done < <(cd "$dist" && find . -type f ! -name '_*' ! -name '.*' ! -name '*.html' ! -name '*.wasm.br' | sed 's|^\./||' | sort)

# Проверка, не проверившая ни одного файла, — тоже провал.
[ "$checked" -gt 0 ] || fail "$dist: ни одного файла сборки"
if [ ${#failures[@]} -gt 0 ]; then
  printf '%s\n' "Выкладка $site — провалов: ${#failures[@]}" "${failures[@]}" >&2
  exit 1
fi
echo "Выкладка $site: страница программы, переадресации, CSP, сервер пробного периода и $checked файлов, из них wasm копией уровнем 11 — $packed, — как задумано"
