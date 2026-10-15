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
[ -f "$dist/index.html" ] || { echo "в $dist нет index.html" >&2; exit 2; }
[ -f "$headers_file" ] || { echo "в $dist нет _headers" >&2; exit 2; }

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
failures=()
fail() { failures+=("$1"); }

# Заголовок ответа без учёта регистра имени: последнее значение, без `\r`.
header() {
  grep -i "^$1:" "$2" | tail -n 1 | cut -d: -f2- | sed 's/^ *//' | tr -d '\r' || true
}

# Ответ на GET: код, заголовки — в файл, тело — в файл. `Accept-Encoding`
# — как у браузера; тело не разжимается, чтобы видеть, сжал ли хостинг.
# Сразу после выкладки край Cloudflare бывает отвечает 5xx: у первой
# выкладки 16 файлов из 79 пришли с 522 («не дождался источника»), а через
# минуту все — с 200 и верными заголовками (2026-10-15). Поэтому 5xx
# и обрыв связи (000) повторяются до пяти раз, и провал — только
# устойчивый.
fetch() {
  local code attempt
  for attempt in 1 2 3 4 5; do
    code="$(curl -sS -o "$work/body" -D "$work/head" -w '%{http_code}' \
      -H 'Accept-Encoding: br, gzip' "$site/$1" || true)"
    case "$code" in
      5?? | 000) [ "$attempt" = 5 ] || sleep 3 ;;
      *) break ;;
    esac
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
page_hash="$(sha256sum "$dist/index.html" | cut -d' ' -f1)"
for attempt in $(seq 1 12); do
  code="$(curl -sS --compressed -o "$work/page" -D "$work/page-head" -w '%{http_code}' \
    -H 'Accept: text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8' "$site/" || true)"
  if [ "$code" = 200 ] && [ "$(sha256sum "$work/page" | cut -d' ' -f1)" = "$page_hash" ]; then
    break
  fi
  if [ "$attempt" = 12 ]; then
    echo "$site/: $code, страница не этой сборки" >&2
    grep -o '<script[^>]*src="https\?://[^"]*"' "$work/page" | sed 's/^/  чужой скрипт: /' >&2 || true
    exit 1
  fi
  sleep 5
done

# Страница — байт в байт та, что собрана: хостинг ничего в неё не вписал
# (счётчик Web Analytics, включённый в панели, вписал бы скрипт).
check_page() {
  local got
  got="$(header content-security-policy "$work/page-head")"
  [ "$got" = "$csp" ] || fail "/: CSP «$got», а в _headers «$csp»"
  for expected in 'x-content-type-options: nosniff' 'referrer-policy: no-referrer' \
    'cross-origin-opener-policy: same-origin'; do
    got="$(header "${expected%%:*}" "$work/page-head")"
    [ "$got" = "${expected#*: }" ] || fail "/: ${expected%%:*} «$got», а ждали «${expected#*: }»"
  done
  got="$(header cache-control "$work/page-head")"
  case "$got" in
    *immutable* | *max-age=[1-9]*) fail "/: Cache-Control «$got» — страница без хэша в имени" ;;
  esac
}
check_page

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
# страницей с `immutable` на год (шапка 404.html).
check_missing() {
  local code got
  code="$(curl -sS --compressed -o "$work/missing" -D "$work/missing-head" -w '%{http_code}' \
    "$site/kvartik-no-such-file-0000000000000000.js" || true)"
  [ "$code" = 404 ] || fail "неизвестный адрес: $code, а ждали 404"
  cmp -s "$work/missing" "$dist/404.html" || fail "неизвестный адрес: тело — не 404.html этой сборки"
  got="$(header cache-control "$work/missing-head")"
  [[ "$got" == *no-store* ]] || fail "неизвестный адрес: Cache-Control «$got», а ждали no-store"
}
check_missing

# Тип по расширению — как его ждут браузер и service worker.
expected_type() {
  case "$1" in
    *.wasm) echo application/wasm ;;
    *.js) echo text/javascript application/javascript ;;
    *.css) echo text/css ;;
    *.ttf) echo font/ttf ;;
    *.png) echo image/png ;;
    *.webmanifest) echo application/manifest+json ;;
    *.txt) echo text/plain ;;
    *) echo "?" ;;
  esac
}

# Каждый файл сборки. С хэшем в имени (Trunk и хук сборки пишут его
# шестнадцатью знаками перед расширением) — `immutable`; без хэша — без
# долгого кэша. Тексты и wasm — сжатыми. Страницы HTML — выше: на
# `/404.html` Pages отвечает переадресацией на `/404`. Скрытые файлы
# (`.trunk-lock` Trunk) в выкладку не идут: их не берёт артефакт CI.
checked=0
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
  case "$path" in
    *.wasm | *.js | *.css | *.ttf | *.webmanifest)
      encoding="$(header content-encoding "$work/head")"
      [ -n "$encoding" ] || fail "$path: не сжат"
      ;;
  esac
  # Тело `sw.js` — этой сборки: по нему браузер узнаёт новую версию.
  if [ "$path" = sw.js ]; then
    if curl -sS --compressed -o "$work/sw" "$site/sw.js"; then
      cmp -s "$work/sw" "$dist/sw.js" || fail "sw.js: не этой сборки"
    else
      fail "sw.js: не скачан"
    fi
  fi
done < <(cd "$dist" && find . -type f ! -name '_*' ! -name '.*' ! -name '*.html' | sed 's|^\./||' | sort)

# Проверка, не проверившая ни одного файла, — тоже провал.
[ "$checked" -gt 0 ] || fail "$dist: ни одного файла сборки"
if [ ${#failures[@]} -gt 0 ]; then
  printf '%s\n' "Выкладка $site — провалов: ${#failures[@]}" "${failures[@]}" >&2
  exit 1
fi
echo "Выкладка $site: страница, CSP и $checked файлов — как задумано"
