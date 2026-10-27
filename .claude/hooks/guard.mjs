// Запреты, которые держит оболочка Claude Code, а не память агента (задача
// 66). Оболочка зовёт этот скрипт перед каждым вызовом Read, Edit, Write,
// NotebookEdit, Grep, Glob, Bash и PowerShell (`.claude/settings.json`),
// у главного агента и у субагентов, и передаёт вызов JSON на stdin. Отказ —
// ответ JSON с `permissionDecision: "deny"`: вызов не исполняется, а причину
// видит агент. Скрипт, который упал или не запустился, — тоже отказ
// (`onFailure: "block"` в настройках): сломанный запрет не должен
// превращаться в разрешение. Формат — по документации Claude Code
// (code.claude.com/docs/en/hooks, сверено 2026-10-16, Claude Code 2.1.296).
//
// Три запрета:
// 1. Ключи. Пути `*.key`, `*.pem` и папки `keys/` и `kvartik-license/` — те
//    же шаблоны, что в `.gitignore`. Закрытый ключ лицензий агент не читает,
//    не создаёт и не передаёт в команды (CLAUDE.md, Правила 9 и 13).
//    В `kvartik-license` домашнего каталога автора ключ и лежит, рядом
//    с журналом выданных ключей (задача 26, `SECRET_DIRS`).
// 2. `kvartik-keygen issue` без `--key`. Закрытый ключ программа берёт из
//    `kvartik-license` сама, и пути в команде нет, а подписанный им ключ —
//    настоящая лицензия. Ключи выпускает человек; с `--key` путь в команде
//    есть, и его проверяет первый запрет.
// 3. Текст через PowerShell 5.1. `Set-Content`, `Add-Content`, `Out-File`,
//    `Tee-Object`, `[IO.File]::Write…` и перенаправление `>`/`>>` рядом
//    с текстовым файлом проекта. PowerShell 5.1 читает файл без BOM как CP1252
//    и пишет его обратно с BOM: армянский и кириллица становятся кракозябрами
//    (CLAUDE.md, Правило 2).
//
// Скрипт идёт на каждый вызов инструмента, поэтому он без зависимостей и не
// зовёт других программ: только разбор строк (Правило 1). Проверки —
// `node .claude/hooks/test.mjs`.

import path from "node:path";

const PROJECT = norm(process.env.CLAUDE_PROJECT_DIR ?? process.cwd());

// Папки, всё в которых — ключ, где бы они ни лежали. По имени, а не полным
// путём: `~/kvartik-license`, `$HOME\kvartik-license`, `cd ~; cat
// kvartik-license/…` и `Join-Path $HOME kvartik-license` — одна папка, а
// полный путь поймал бы только первый. Имя редкое, и отказ — даже голому
// слову, в отличие от `keys` (`jq keys`).
const SECRET_DIRS = ["kvartik-license"];

// Исходники крейтов из реестра читать можно и нужно (Правило 3), а в них
// лежат тестовые `*.pem` и `*.key` чужих библиотек — это не наш ключ.
const READABLE = /\/\.cargo\/registry\/src\//;

// Текстовые файлы проекта: всё, что лежит в репозитории не двоичным.
const TEXT_EXT = new Set(["rs", "toml", "md", "json", "css", "html", "js", "mjs", "cjs", "ts", "py", "sh",
  "ps1", "yml", "yaml", "txt", "lock", "webmanifest", "svg", "csv", "xml", "ics"]);
const TEXT_NAMES = new Set(["_headers", "_redirects", ".gitignore", ".gitattributes"]);
// Сборка и чужое: файл туда пишет кто угодно, исходников там нет.
const BUILT = new Set(["target", "dist", "node_modules", ".git"]);

const WRITERS = /(^|[\s|;&({])(set-content|add-content|out-file|tee-object|sc|ac|tee)(?=$|[\s|;&)}])|\[(system\.)?io\.file\]::(write|append)all(text|lines|bytes)/i;
// `>`/`>>`, но не `2>&1` и не `> $null`. Ищется в команде без строк
// в кавычках: `"a > b"` — не перенаправление.
const REDIRECT = /(?<![>&])>>?(?![>&])\s*(?!\$null\b)\S/;

function norm(p) {
  let s = String(p).replace(/\\/g, "/");
  const bash = /^\/([a-z])(\/.*)?$/i.exec(s); // Git Bash: /c/dev → c:/dev
  if (bash) s = `${bash[1]}:${bash[2] ?? "/"}`;
  return s.replace(/\/+$/, "").toLowerCase();
}

function secret(p) {
  const s = norm(p);
  if (s.split("/").some((seg) => SECRET_DIRS.includes(seg))) return true;
  const base = s.slice(s.lastIndexOf("/") + 1);
  if (/\.(key|pem)$/.test(base) || /\.\{[^}]*\b(key|pem)\b/.test(base)) return true;
  // Голое слово «keys» (`jq keys`) — не путь: папка — только со слэшем.
  return s.includes("/") && /(^|\/)keys(\/|$)/.test(s);
}

// Слова команды оболочки: кавычки снимаются, разделители — пробелы
// и `| ; & < > ( )` и обратная кавычка, у PowerShell ещё `, { }` (у bash
// это часть шаблона `*.{key,pem}`). Обратная косая черта — буквой: в
// PowerShell и в путях Windows это разделитель папок, а не экранирование.
// Слово из кавычек с пробелами внутри — это текст (сообщение коммита,
// `echo`), и путь в нём ищется только по словам со слэшем — или это весь
// путь целиком, когда он и начинается как путь (`"C:/Мои документы/a.key"`).
function words(cmd, tool) {
  const seps = tool === "PowerShell" ? "|;&<>()`,{}" : "|;&<>()`";
  const out = [];
  let cur = "", quoted = false, spaced = false, q = null;
  const push = () => { if (cur || quoted) out.push({ text: cur, spaced }); cur = ""; quoted = false; spaced = false; };
  for (let i = 0; i < cmd.length; i++) {
    const c = cmd[i];
    if (q) {
      if (c === q) q = null;
      else if (c === "\\" && q === '"' && cmd[i + 1] === '"') cur += cmd[++i];
      else { if (/\s/.test(c)) spaced = true; cur += c; }
    } else if (c === "'" || c === '"') { q = c; quoted = true; }
    else if (/\s/.test(c) || seps.includes(c)) push();
    else cur += c;
  }
  push();
  return out.flatMap((w) => w.spaced
    ? [...(/^([a-z]:[\\/]|~[\\/]|\.{0,2}[\\/])[^\n]*$/i.test(w.text) ? [w.text] : []),
       ...w.text.split(/[\s|;&<>()`$]+/).filter((s) => /[\\/]/.test(s))]
    : [w.text]);
}

function unquoted(cmd) {
  return cmd.replace(/'[^']*'|"(?:[^"\\]|\\.)*"/g, '""');
}

function inProject(word, cwd) {
  if (!word || /^[-$@]|:\/\/|^\d+$/.test(word)) return null;
  const w = word.replace(/\\/g, "/");
  const abs = /^([a-z]:\/|\/[a-z]\/)/i.test(w) ? w : path.posix.join(norm(cwd ?? PROJECT), w);
  const s = norm(abs);
  if (s !== PROJECT && !s.startsWith(PROJECT + "/")) return null;
  const rel = s.slice(PROJECT.length + 1);
  if (rel.split("/").some((seg) => BUILT.has(seg))) return null;
  const base = rel.slice(rel.lastIndexOf("/") + 1);
  const ext = base.includes(".") ? base.slice(base.lastIndexOf(".") + 1) : "";
  return TEXT_NAMES.has(base) || TEXT_EXT.has(ext) ? word : null;
}

function check(input) {
  const tool = input.tool_name;
  const args = input.tool_input ?? {};
  const shell = tool === "Bash" || tool === "PowerShell";
  const paths = shell ? words(String(args.command ?? ""), tool)
    : [args.file_path, args.notebook_path, args.path, args.pattern && tool === "Glob" ? args.pattern : null, args.glob]
      .filter((p) => typeof p === "string" && p);
  const readOnly = tool === "Read" || tool === "Grep" || tool === "Glob";
  const key = paths.find((p) => secret(p) && !(readOnly && READABLE.test(norm(p))));
  if (key) {
    return `Отказ hook'а .claude/hooks/guard.mjs: «${key}» похоже на путь к ключу (*.key, *.pem, keys/, kvartik-license/). `
      + "Закрытый ключ лицензий агент не читает, не создаёт и не передаёт в команды (CLAUDE.md, Правила 9 и 13). "
      + "Если это не ключ, а слово в тексте, сделайте то же другим путём: строку ищите инструментом Grep "
      + "(его образец поиска путём не считается), текст пишите инструментами Write и Edit, а не командой; "
      + "сообщение коммита — файлом: git commit -F <файл вне проекта>. Если нужен именно такой путь — скажите человеку.";
  }
  if (shell && paths.some((w) => /(^|[\\/])kvartik-keygen(\.exe)?$/i.test(w)) && paths.includes("issue")
    && !paths.includes("--key")) {
    return "Отказ hook'а .claude/hooks/guard.mjs: kvartik-keygen issue без --key подписывает ключ закрытым ключом автора "
      + "из папки kvartik-license домашнего каталога — это настоящая лицензия. Ключи выпускает человек в своём терминале "
      + "(CLAUDE.md, Правила 9 и 13). Проверить программу — cargo test -p kvartik-keygen или issue --key с ключом проб "
      + "во временной папке задачи.";
  }
  if (tool === "PowerShell") {
    const cmd = String(args.command ?? "");
    const bare = unquoted(cmd);
    if (WRITERS.test(bare) || REDIRECT.test(bare)) {
      const file = words(cmd, tool).map((w) => inProject(w, input.cwd)).find(Boolean);
      if (file) {
        return `Отказ hook'а .claude/hooks/guard.mjs: команда PowerShell пишет текст (Set-Content, Add-Content, Out-File, `
          + `Tee-Object, [IO.File]::Write…, > или >>) рядом с файлом проекта «${file}». PowerShell 5.1 читает файл без BOM `
          + "как CP1252 и пишет его обратно с BOM — армянский и кириллица становятся кракозябрами (CLAUDE.md, Правило 2). "
          + "Правьте файлы инструментами Edit и Write, массовую замену делайте Python с явным encoding=\"utf-8\" по байтам, "
          + "а вывод команды пишите в файл вне проекта (C:\\tmp\\k<номер задачи>\\…) или в target/.";
      }
    }
  }
  return null;
}

let raw = "";
process.stdin.setEncoding("utf8");
for await (const chunk of process.stdin) raw += chunk;
const reason = check(JSON.parse(raw));
if (reason) {
  process.stdout.write(JSON.stringify({
    hookSpecificOutput: { hookEventName: "PreToolUse", permissionDecision: "deny", permissionDecisionReason: reason },
  }));
}
