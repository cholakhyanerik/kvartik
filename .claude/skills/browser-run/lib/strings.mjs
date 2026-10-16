// Строки окна и образцы копий — прямо из исходников, без сборки
// (skill browser-run): сценарий ищет кнопку по её тексту на языке окна.
//
// - `t(key, lang)` — таблица `crates/core/src/i18n.rs`: макрос `strings!`
//   принимает ровно три строки — русскую, английскую, армянскую.
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
/// Строка `key` (`"UpdateApply"`) на языке `lang` (`hy`, `ru`, `en`).
export function t(key, lang) {
  if (!table) {
    const src = fs.readFileSync(path.join(REPO, "crates/core/src/i18n.rs"), "utf8");
    const body = src.slice(src.indexOf("strings! {")).replace(/^\s*\/\/.*$/gm, "");
    table = new Map();
    const entry = new RegExp(String.raw`(\w+)\s*=\s*${LIT}\s*,\s*${LIT}\s*,\s*${LIT}\s*;`, "g");
    for (const m of body.matchAll(entry)) {
      table.set(m[1], { ru: unescape(m[2]), en: unescape(m[3]), hy: unescape(m[4]) });
    }
  }
  const row = table.get(key);
  if (!row) throw new Error(`нет строки ${key} в i18n.rs`);
  return row[lang];
}

/// Образцы копий по порядку версий: `{ file, password }`.
export function samples() {
  const dir = path.join(REPO, "crates/core/tests/backups");
  const src = fs.readFileSync(path.join(dir, "samples.rs"), "utf8");
  return [...src.matchAll(new RegExp(String.raw`path:\s*${LIT},[\s\S]*?password:\s*${LIT},`, "g"))]
    .map((m) => ({ file: path.join(dir, unescape(m[1])), password: unescape(m[2]) }));
}
