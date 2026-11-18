#!/usr/bin/env bash
# Выкладка собранного сайта на Cloudflare Pages одной веткой (задачи 15
# и 50). Печатает адрес этой выкладки — по нему её проверяет check.sh.
# Зовёт её CI (`.github/workflows/ci.yml`), а в окружении ждёт
# CLOUDFLARE_API_TOKEN, CLOUDFLARE_ACCOUNT_ID и PAGES_PROJECT:
#
#   tools/deploy/deploy.sh <ветка> <папка сборки> <коммит>
#
# Ветка `main` — сайт всем, любая другая — предпросмотр на своём адресе.
# wrangler — из замка рядом (`npm ci --ignore-scripts` в этой папке).
# Без вопросов: stdin в CI не терминал (по исходникам wrangler
# 4.146–4.149). Ветку и коммит — явно: в pull request git стоит на
# сливающем коммите без ветки. Адрес wrangler пишет в файл (ND-JSON,
# запись `pages-deploy-detailed`), а свой вывод — в stderr: в stdout
# остаётся только адрес. Отчёты wrangler о себе выключены.
set -euo pipefail

if [ $# -ne 3 ]; then
  echo "использование: $0 <ветка> <папка сборки> <коммит>" >&2
  exit 2
fi
branch="$1"
dist="$(cd "$2" && pwd)"
commit="$3"
out="$(mktemp)"
trap 'rm -f "$out"' EXIT

# Настройки проекта — базы D1 функции сайта (задача 106) — из файла рядом,
# `wrangler.toml`. `pages deploy` берёт его только из папки запуска и выше
# по дереву, а `--config` не принимает вовсе («Pages does not support custom
# paths», по исходникам wrangler 4.149.0; выкладка 0.0.152 в CI, 2026-11-18).
# Не нашёл файла — шлёт выкладку без привязок, и молча (Правило 6). Поэтому
# wrangler идёт из этой папки, а папка сборки — полным путём.
cd "$(dirname "$0")"
WRANGLER_OUTPUT_FILE_PATH="$out" WRANGLER_SEND_METRICS=false WRANGLER_SEND_ERROR_REPORTS=false \
  ./node_modules/.bin/wrangler pages deploy "$dist" \
  --project-name="$PAGES_PROJECT" --branch="$branch" \
  --commit-hash="$commit" --commit-dirty=true >&2
url="$(jq -r 'select(.type == "pages-deploy-detailed") | .url' "$out")"
if [ -z "$url" ] || [ "$url" = null ]; then
  echo "wrangler не назвал адрес выкладки" >&2
  exit 1
fi
echo "$url"
