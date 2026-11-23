// Строки окна и образцы копий — прямо из исходников, без сборки (skill
// browser-run и `check.mjs` рядом): сценарий ищет кнопку по её тексту на
// языке окна.
//
// - `t(key, lang)` — таблица строк ядра: у каждого ключа — строка в макросе
//   своего языка, `(Key) => { "…" };` в `crates/core/src/i18n/{hy,ru,en}.rs`
//   (задача 119). `keys()` — все ключи по порядку таблицы.
// - `samples()` — образцы файла копии из `crates/core/tests/backups/samples.rs`
//   с их паролями: восстановленный образец даёт экранам данные.

import fs from "node:fs";
import path from "node:path";
import { REPO } from "./serve.mjs";

const LIT = String.raw`"((?:[^"\\]|\\[\s\S])*)"`;

function unescape(s) {
  return s.replace(/\\\r?\n\s*/g, "").replace(/\\u\{([0-9a-fA-F]+)\}|\\(.)/g, (_, hex, c) =>
    hex ? String.fromCodePoint(parseInt(hex, 16)) : ({ n: "\n", t: "\t", r: "\r", 0: "\0" }[c] ?? c));
}

let table;
function load() {
  if (!table) {
    table = new Map();
    const entry = new RegExp(String.raw`\((\w+)\) => \{\s*${LIT}\s*\};`, "g");
    for (const lang of ["ru", "en", "hy"]) {
      const src = fs.readFileSync(path.join(REPO, `crates/core/src/i18n/${lang}.rs`), "utf8");
      for (const m of src.matchAll(entry)) {
        if (!table.has(m[1])) table.set(m[1], {});
        table.get(m[1])[lang] = unescape(m[2]);
      }
    }
  }
  return table;
}

/// Строка `key` (`"UpdateApply"`) на языке `lang` (`hy`, `ru`, `en`).
export function t(key, lang) {
  const text = load().get(key)?.[lang];
  if (text === undefined) throw new Error(`нет строки ${key} (${lang}) в crates/core/src/i18n/${lang}.rs`);
  return text;
}

/// Все ключи таблицы по порядку.
export function keys() {
  return [...load().keys()];
}

/// Образцы копий по порядку версий: `{ file, password }`.
export function samples() {
  const dir = path.join(REPO, "crates/core/tests/backups");
  const src = fs.readFileSync(path.join(dir, "samples.rs"), "utf8");
  return [...src.matchAll(new RegExp(String.raw`path:\s*${LIT},[\s\S]*?password:\s*${LIT},`, "g"))]
    .map((m) => ({ file: path.join(dir, unescape(m[1])), password: unescape(m[2]) }));
}
