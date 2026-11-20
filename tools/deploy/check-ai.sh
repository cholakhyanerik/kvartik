#!/usr/bin/env bash
# Проверка выкладки рамки ИИ-помощника (задача 111): отдаёт ли хостинг ровно
# эту сборку рамки и так, как она задумана, — своим проектом Pages
# `kvartik-ai` на своём домене `ai.kvartik.com`. Зовёт её CI сразу после
# выкладки рамки (`.github/workflows/ci.yml`), а можно и руками — с адресом
# и собранной папкой рамки:
#
#   tools/deploy/check-ai.sh https://ai.kvartik.com crates/app/dist-ai
#
# Зачем: CSP рамки — единственный, что пускает к поставщикам ИИ, а встроить
# рамку может только окно программы (`frame-ancestors`): чужой CSP хостинга
# открыл бы дверь или закрыл бы помощника, и ни сборка, ни тесты этого не
# видят (CLAUDE.md, Правила 6 и 13). Провалы копятся и выводятся все
# в конце (Правило 2). Приёмы — те же, что у `check.sh` сайта программы.
set -euo pipefail

if [ $# -ne 2 ]; then
  echo "использование: $0 <адрес рамки> <папка сборки рамки>" >&2
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

header() {
  grep -i "^$1:" "$2" | tail -n 1 | cut -d: -f2- | sed 's/^ *//' | tr -d '\r' || true
}

# Выкладка доходит до края сети не вся разом (Правило 6; `check.sh`):
# ответ прежней выкладки повторяется до трёх минут от первой страницы этой
# сборки, и провал — только устойчивый.
settle_until=0
settling() {
  [ "$(date +%s)" -lt "$settle_until" ] || return 1
  sleep 3
}

# Ответ на GET: код, заголовки и тело — разжатое (`--compressed`): тело
# сверяется с файлом сборки байт в байт. 5xx и обрыв — до пяти раз, 404 —
# пока выкладка доходит.
fetch() {
  local code attempt=1
  while :; do
    code="$(curl -sS --compressed -o "$work/body" -D "$work/head" -w '%{http_code}' "$site/$1" || true)"
    case "$code" in
      5?? | 000) [ "$attempt" -lt 5 ] || break; sleep 3 ;;
      404) settling || break ;;
      *) break ;;
    esac
    attempt=$((attempt + 1))
  done
  echo "$code"
}

csp="$(sed -n 's/^ *Content-Security-Policy: *//p' "$headers_file" | head -n 1 | tr -d '\r')"
[ -n "$csp" ] || { echo "в _headers рамки нет Content-Security-Policy" >&2; exit 2; }

# Страница рамки — этой сборки: первая же страница ставит окно ожидания.
deadline=$(( $(date +%s) + 180 ))
while :; do
  code="$(fetch "")"
  if [ "$code" = 200 ] && cmp -s "$work/body" "$dist/index.html"; then
    break
  fi
  [ "$(date +%s)" -lt "$deadline" ] || break
  sleep 3
done
settle_until=$(( $(date +%s) + 180 ))
if [ "$code" != 200 ] || ! cmp -s "$work/body" "$dist/index.html"; then
  fail "/: код $code, а страница — не этой сборки"
fi
got="$(header content-security-policy "$work/head")"
[ "$got" = "$csp" ] || fail "/: CSP «$got», а в _headers рамки «$csp»"
[ "$(header referrer-policy "$work/head")" = "no-referrer" ] || fail "/: Referrer-Policy — «$(header referrer-policy "$work/head")»"
[ "$(header x-content-type-options "$work/head")" = "nosniff" ] || fail "/: нет X-Content-Type-Options: nosniff"
[ -z "$(header x-frame-options "$work/head")" ] || fail "/: X-Frame-Options запретил бы рамку окну"
case "$(header cache-control "$work/head")" in
  *immutable*) fail "/: страница рамки — immutable" ;;
esac
# На своём домене отчётов о сбоях нет: их снимает правило зоны `no-nel`
# (CLAUDE.md, Правило 13); на `*.pages.dev` их ставит сама Pages.
case "$site" in
  https://ai.kvartik.com)
    [ -z "$(header nel "$work/head")" ] || fail "/: Nel — правило зоны no-nel не снимает его у ai.kvartik.com"
    [ -z "$(header report-to "$work/head")" ] || fail "/: Report-To — правило зоны no-nel не снимает его у ai.kvartik.com"
    ;;
esac

# Каждый файл рамки — байт в байт; на год — только файлы с хэшем в имени.
files=0
while IFS= read -r -d '' file; do
  path="${file#"$dist"/}"
  case "$path" in
    _headers | index.html | 404.html) continue ;;
  esac
  files=$((files + 1))
  code="$(fetch "$path")"
  if [ "$code" != 200 ]; then
    fail "/$path: код $code"
    continue
  fi
  cmp -s "$work/body" "$file" || fail "/$path: тело не то, что в сборке"
  cache="$(header cache-control "$work/head")"
  case "$path" in
    ai-*) case "$cache" in *immutable*) ;; *) fail "/$path: Cache-Control «$cache», а ждали immutable" ;; esac ;;
    *) case "$cache" in *immutable*) fail "/$path: immutable у файла без хэша в имени" ;; esac ;;
  esac
done < <(find "$dist" -type f -print0)

# Неизвестный адрес — `404.html` с кодом 404, а не страница рамки с 200.
code="$(curl -sS --compressed -o "$work/body" -D "$work/head" -w '%{http_code}' "$site/kvartik-check-missing" || true)"
[ "$code" = 404 ] || fail "/kvartik-check-missing: код $code, а ждали 404"
cmp -s "$work/body" "$dist/404.html" || fail "/kvartik-check-missing: тело — не 404.html рамки"

if [ ${#failures[@]} -gt 0 ]; then
  printf 'Выкладка рамки %s — провалы:\n' "$site" >&2
  printf '  %s\n' "${failures[@]}" >&2
  exit 1
fi
echo "Выкладка рамки $site: страница, CSP, заголовки и $files файлов — как задумано"
