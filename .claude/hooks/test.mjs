// Проверка hooks (задача 66): каждый запрет должен отказать запрещённому
// и пропустить разрешённое. `node .claude/hooks/test.mjs` — после любой
// правки `guard.mjs` или `lf.mjs`. Провалы собираются и выдаются в конце
// (Правило 2), а не обрывают прогон на первом.

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const HOOKS = path.dirname(fileURLToPath(import.meta.url));
const PROJECT = path.resolve(HOOKS, "../..");
const failures = [];
let passed = 0;

function run(script, input, env = {}) {
  const started = process.hrtime.bigint();
  const r = spawnSync(process.execPath, [path.join(HOOKS, script)], {
    input: typeof input === "string" ? input : JSON.stringify(input),
    encoding: "utf8", env: { ...process.env, CLAUDE_PROJECT_DIR: PROJECT, ...env },
  });
  return { ...r, ms: Number(process.hrtime.bigint() - started) / 1e6 };
}

function expect(name, ok, detail) {
  if (ok) passed++;
  else failures.push(`${name}${detail ? ` — ${detail}` : ""}`);
}

function guard(tool, toolInput, want, cwd = PROJECT) {
  const r = run("guard.mjs", { tool_name: tool, tool_input: toolInput, cwd, hook_event_name: "PreToolUse" });
  const out = r.stdout.trim() ? JSON.parse(r.stdout) : null;
  const denied = out?.hookSpecificOutput?.permissionDecision === "deny";
  const name = `${want}: ${tool} ${JSON.stringify(toolInput)}`;
  expect(name, r.status === 0 && denied === (want === "deny"), `код ${r.status}, ${denied ? "отказ" : "пропуск"} ${r.stderr.trim()}`);
  return r.ms;
}

const P = PROJECT.replace(/\\/g, "/");
const cases = [
  // Ключи: инструменты с путём.
  ["deny", "Read", { file_path: `${P}/license.key` }],
  ["deny", "Read", { file_path: "C:\\Users\\chola\\keys\\kvartik.txt" }],
  ["deny", "Write", { file_path: "C:\\tmp\\k66\\signing.pem", content: "x" }],
  ["deny", "Edit", { file_path: `${P}/keys/notes.md`, old_string: "a", new_string: "b" }],
  ["deny", "NotebookEdit", { notebook_path: "C:/tmp/a.KEY", new_source: "" }],
  ["deny", "Glob", { pattern: "**/*.key" }],
  ["deny", "Glob", { pattern: "*", path: "C:/Users/chola/keys" }],
  ["deny", "Grep", { pattern: "BEGIN", glob: "*.{rs,pem}" }],
  ["deny", "Grep", { pattern: "BEGIN", path: "/c/Users/chola/keys" }],
  ["allow", "Read", { file_path: `${P}/CLAUDE.md` }],
  ["allow", "Read", { file_path: "C:\\Users\\chola\\.cargo\\registry\\src\\index.crates.io-1949cf8c6b5b557f\\ed25519-dalek-3.0.0\\tests\\examples\\pkcs8-v1.pem" }],
  ["allow", "Grep", { pattern: "\\.key", path: `${P}/.gitignore` }],
  ["allow", "Glob", { pattern: "**/*.keystore.rs" }],
  ["allow", "Edit", { file_path: `${P}/.gitignore`, old_string: "*.key", new_string: "*.key" }],
  // Ключи: команды оболочки.
  ["deny", "Bash", { command: "cat ~/keys/license.key" }],
  ["deny", "Bash", { command: "cp \"C:/Users/chola/Documents/kvartik signing.key\" /c/tmp/" }],
  ["deny", "Bash", { command: "echo \"$(cat /c/Users/chola/keys/private)\"" }],
  ["deny", "Bash", { command: "openssl pkey -in signing.pem -noout" }],
  ["deny", "PowerShell", { command: "Get-Content C:\\Users\\chola\\kvartik.pem" }],
  ["deny", "PowerShell", { command: "cargo run -p kvartik-keygen -- --key C:\\secrets\\license.key" }],
  ["allow", "Bash", { command: "git commit -m \"hooks: запрет на *.key и *.pem\"" }],
  ["allow", "Bash", { command: "jq keys crates/core/tests/backups/v5/sample.json" }],
  ["allow", "Bash", { command: "cargo test -p kvartik-core license" }],
  ["allow", "PowerShell", { command: "git status --short" }],
  // Папка закрытого ключа и журнала у автора — по имени, в любом месте пути,
  // и выпуск ключа без `--key` (задача 26).
  ["deny", "Read", { file_path: "C:\\Users\\chola\\kvartik-license\\issued.tsv" }],
  ["deny", "Glob", { pattern: "*", path: "C:/Users/chola/kvartik-license" }],
  ["deny", "Grep", { pattern: "00054321", path: "/c/Users/chola/Kvartik-License" }],
  ["deny", "Bash", { command: "cat ~/kvartik-license/issued.tsv" }],
  ["deny", "Bash", { command: "cd ~ && ls kvartik-license" }],
  ["deny", "PowerShell", { command: "Get-ChildItem (Join-Path $HOME kvartik-license)" }],
  ["deny", "PowerShell", { command: "Get-Content %USERPROFILE%\\kvartik-license\\issued.tsv" }],
  ["deny", "Bash", { command: "cargo run -q -p kvartik-keygen --release -- issue --tin 00054321 --plan standard --until 2027-10-10" }],
  ["deny", "PowerShell", { command: "& .\\target\\release\\kvartik-keygen.exe issue --tin 00054321 --plan pro --until 2027-10-10" }],
  ["deny", "Bash", { command: "cargo run -p kvartik-keygen -- issue --key ~/kvartik-license/license.txt --tin 00054321 --plan standard --until 2027-10-10" }],
  ["deny", "Bash", { command: "cargo run -q -p kvartik-keygen --release -- issue --tin 00054321 --plan standard --renew" }],
  ["allow", "Read", { file_path: "C:/tmp/k26/kvartik-licenses/notes.txt" }],
  ["allow", "Grep", { pattern: "kvartik-license", path: `${P}/crates/keygen` }],
  ["allow", "Bash", { command: "git commit -m \"0.0.61: журнал рядом с ключом в kvartik-license\"" }],
  ["allow", "Bash", { command: "cargo run -p kvartik-keygen -- issue --key C:/tmp/k26/seed.txt --tin 00054321 --plan standard --until 2027-10-10" }],
  ["allow", "Bash", { command: "cargo run -q -p kvartik-keygen -- issue --key C:/tmp/k97/probe/seed.txt --tin 00054321 --plan standard --renew" }],
  ["allow", "Bash", { command: "cargo clippy -p kvartik-keygen --all-targets -- -D warnings" }],
  // PowerShell 5.1 и текстовые файлы проекта.
  ["deny", "PowerShell", { command: "Set-Content -Path CLAUDE.md -Value $text" }],
  ["deny", "PowerShell", { command: "(Get-Content crates\\core\\src\\i18n.rs) -replace 'a','b' | Set-Content crates\\core\\src\\i18n.rs" }],
  ["deny", "PowerShell", { command: "\"x\" | Out-File -Encoding utf8 C:\\dev\\kvartik\\README.md" }],
  ["deny", "PowerShell", { command: "echo hi > notes.md" }],
  ["deny", "PowerShell", { command: "git log -1 >> crates/app/public/root/_headers" }],
  ["deny", "PowerShell", { command: "Add-Content .gitignore 'x'" }],
  ["deny", "PowerShell", { command: "[IO.File]::WriteAllText(\"C:\\dev\\kvartik\\Cargo.toml\", $t)" }],
  ["deny", "PowerShell", { command: "Get-ChildItem *.rs | ForEach-Object { sc $_ ((gc $_) -replace 'a','b') }" }],
  ["deny", "PowerShell", { command: "Get-Content CLAUDE.md | Tee-Object -FilePath C:\\tmp\\k66\\copy.md" }],
  ["allow", "PowerShell", { command: "cargo test 2>&1 | Out-File C:\\tmp\\k66\\log.txt" }],
  ["allow", "PowerShell", { command: "cargo build > C:\\tmp\\k66\\build.txt 2>&1" }],
  ["allow", "PowerShell", { command: "cargo build >> target\\build.log" }],
  ["allow", "PowerShell", { command: "Get-Content CLAUDE.md | Select-String 'Правило 2'" }],
  ["allow", "PowerShell", { command: "git fetch > $null; cargo test *> $null; git gc >>$null" }],
  ["allow", "PowerShell", { command: "git commit -m \"0.0.36: a > b в CLAUDE.md\"" }],
  ["allow", "PowerShell", { command: "trunk build --release 2>&1 | Tee-Object -FilePath C:\\tmp\\k66\\trunk.log" }],
  ["allow", "Bash", { command: "echo x > CLAUDE.md" }],
];
const times = cases.map(([want, tool, input]) => guard(tool, input, want));
guard("PowerShell", { command: "Set-Content notes.md 'x'" }, "allow", "C:\\tmp\\k66");

// Сломанный вход — не пропуск: скрипт падает, а `onFailure: "block"` делает
// из падения отказ.
const broken = run("guard.mjs", "{не json");
expect("сломанный вход: guard.mjs падает с кодом не 0", broken.status !== 0, `код ${broken.status}`);

// lf.mjs — на своих файлах во временной папке, в том числе в своём
// репозитории git с правилами `.gitattributes`.
const dir = fs.mkdtempSync(path.join(os.tmpdir(), "k66-lf-"));
try {
  const lf = (name, content, want) => {
    const file = path.join(dir, name);
    fs.writeFileSync(file, content);
    const r = run("lf.mjs", { tool_name: "Write", tool_input: { file_path: file }, tool_response: { filePath: file, type: "create" } });
    const got = fs.readFileSync(file);
    expect(`lf.mjs: ${name}`, r.status === 0 && got.equals(Buffer.from(want)), `код ${r.status}, ${JSON.stringify(got.toString("latin1"))} ${r.stderr.trim()}`);
    return r;
  };
  const changed = lf("crlf.sh", "echo 1\r\necho 2\r\n", "echo 1\necho 2\n");
  expect("lf.mjs: перевод назван агенту", /прочитайте его заново/.test(changed.stdout), changed.stdout);
  const same = lf("lf.md", "строка\nещё\n", "строка\nещё\n");
  expect("lf.mjs: без CRLF — молча", same.stdout === "", same.stdout);
  lf("zero.bin", "a\r\n\0", "a\r\n\0");
  spawnSync("git", ["init", "-q"], { cwd: dir });
  fs.writeFileSync(path.join(dir, ".gitattributes"), "keep.json -text\n*.ttf binary\n*.bat eol=crlf\n");
  lf("keep.json", "{\r\n}\r\n", "{\r\n}\r\n");
  lf("font.ttf", "a\r\nb", "a\r\nb");
  lf("run.bat", "@echo off\r\n", "@echo off\r\n");
  lf("plain.toml", "a = 1\r\n", "a = 1\n");
} finally {
  fs.rmSync(dir, { recursive: true, force: true });
}

const sorted = [...times].sort((a, b) => a - b);
console.log(`guard.mjs: ${cases.length + 1} вызовов, медиана ${sorted[sorted.length >> 1].toFixed(0)} мс, худший ${sorted.at(-1).toFixed(0)} мс`);
for (const f of failures) console.log(`ПРОВАЛ ${f}`);
console.log(`итог: ${passed} из ${passed + failures.length}`);
process.exit(failures.length ? 1 : 0);
