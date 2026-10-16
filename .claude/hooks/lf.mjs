// Файл, записанный инструментом Write, — с концами строк LF (задача 66).
// Оболочка Claude Code зовёт этот скрипт после каждого Write
// (`.claude/settings.json`) и передаёт вызов JSON на stdin.
//
// Репозиторий хранит текст в LF, а bash читает `\r` частью команды: скрипт
// `.sh` с CRLF падает на первой строке. Write создавал файлы с CRLF (замечено
// 2026-10-13), и агент каждый раз правил их sed'ом; Claude Code 2.1.296 пишет LF
// и при создании, и поверх файла с CRLF (проверено 2026-10-16). Этот скрипт —
// страховка на случай, если прежнее поведение вернётся: он ничего не делает,
// пока в файле нет `\r\n`, и тогда не зовёт даже git.
//
// Не трогает файлы, которые git хранит байт в байт (`-text` и `binary`
// в `.gitattributes` — образцы копий и дизайн), и файлы с `eol=crlf`.
// Двоичный файл (с нулевым байтом) тоже не трогает.

import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";

let raw = "";
process.stdin.setEncoding("utf8");
for await (const chunk of process.stdin) raw += chunk;
const input = JSON.parse(raw);
const file = input.tool_input?.file_path ?? input.tool_response?.filePath;
const bytes = file ? fs.readFileSync(file) : null;
if (bytes && bytes.indexOf(0) < 0 && bytes.indexOf("\r\n") >= 0) {
  const attrs = spawnSync("git", ["check-attr", "text", "eol", "--", path.basename(file)], {
    cwd: path.dirname(file), encoding: "utf8",
  });
  // Вне репозитория git отвечает ошибкой: правил `.gitattributes` там нет.
  const kept = attrs.status === 0 && /: text: unset$|: eol: crlf$/m.test(attrs.stdout);
  if (!kept) {
    const out = Buffer.from(bytes.toString("latin1").replace(/\r\n/g, "\n"), "latin1");
    fs.writeFileSync(file, out);
    process.stdout.write(JSON.stringify({
      hookSpecificOutput: {
        hookEventName: "PostToolUse",
        additionalContext: `hook .claude/hooks/lf.mjs перевёл концы строк ${file} из CRLF в LF. `
          + "Перед правкой этого файла инструментом Edit прочитайте его заново.",
      },
    }));
  }
}
