// Секрет сервера пробного периода (задача 106): ключ HMAC номеров ՀՎՀՀ —
// `TRIAL_KEY` функции сайта (`crates/app/public/root/_worker.js`). Свой
// у выкладки всем и у предпросмотров.
//
//   node tools/deploy/trial-key.mjs                  — поставить ключи в Cloudflare
//   node tools/deploy/trial-key.mjs --forget <ИНН>   — снять запись этого ՀՎՀՀ
//
// Запускает человек в своём терминале, из корня репозитория: агент секретов
// не видит и в Cloudflare не входит (CLAUDE.md, Правила 9 и 13). Не входил
// wrangler в Cloudflare на этой машине — он сам откроет вход в браузере.
// Ключ ложится в папку kvartik-license домашнего каталога, рядом с закрытым
// ключом лицензий, и уходит в Cloudflare; на экран он не печатается никогда.
// По нему продавец находит запись ՀՎՀՀ, когда человек просит её снять
// (политика конфиденциальности, п. 6; условия, п. 3).
//
// wrangler получает ключ файлом (`pages secret bulk`), а не вводом:
// с перенаправленным вводом он считает запуск неинтерактивным и без
// CLOUDFLARE_API_TOKEN не входит (`isTtyInteractive`, по исходникам wrangler
// 4.149.0, 2026-11-18). Файл — в той же папке, и после отправки его нет;
// оставшийся от прерванного запуска убирает следующий.
//
// Ключ не меняют: новый ключ — новые отпечатки, и пробный период у всех ՀՎՀՀ
// начался бы заново. Поэтому файл, который уже есть, не перезаписывается —
// скрипт ставит в Cloudflare тот же ключ, и повтор безвреден.
import { spawnSync } from "node:child_process";
import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

const WRANGLER = path.join(import.meta.dirname, "node_modules/wrangler/bin/wrangler.js");
const CONFIG = path.join(import.meta.dirname, "wrangler.toml");
const FOLDER = path.join(os.homedir(), "kvartik-license");
const PROJECT = "kvartik";
const ENVS = { production: "kvartik-trial", preview: "kvartik-trial-preview" };

/// Ключ среды `env`: из файла, а нет файла — новый, 32 случайных байта
/// шестнадцатеричной строкой, записанный только для владельца.
function key(env, create) {
  const file = path.join(FOLDER, `trial-key.${env}`);
  if (fs.existsSync(file)) return fs.readFileSync(file, "utf8").trim();
  if (!create) throw new Error(`нет ${file}: ключи ещё не заводили`);
  fs.mkdirSync(FOLDER, { recursive: true });
  const fresh = crypto.randomBytes(32).toString("hex");
  fs.writeFileSync(file, `${fresh}\n`, { mode: 0o600, flag: "wx" });
  return fresh;
}

/// wrangler с вводом и выводом этого терминала: так он сам войдёт
/// и спросит, что ему нужно. Успех — код 0.
function wrangler(args) {
  const env = { ...process.env, WRANGLER_SEND_METRICS: "false" };
  return spawnSync(process.execPath, [WRANGLER, ...args], { stdio: "inherit", env }).status === 0;
}

function failed() {
  console.error("wrangler не справился — что не так, он написал выше. Запустите скрипт ещё раз: повтор безвреден.");
  process.exit(1);
}

/// Ставит ключ среды `env` секретом `TRIAL_KEY` проекта.
function upload(env) {
  const secret = key(env, true);
  const file = path.join(FOLDER, `trial-key.${env}.json`);
  fs.writeFileSync(file, JSON.stringify({ TRIAL_KEY: secret }), { mode: 0o600 });
  let done = false;
  try {
    done = wrangler(["pages", "secret", "bulk", file, "--project-name", PROJECT, "--env", env]);
  } finally {
    fs.rmSync(file, { force: true });
  }
  if (!done) failed();
}

if (!fs.existsSync(WRANGLER)) {
  console.error("нет wrangler: поставьте его из замка — npm ci --ignore-scripts в tools/deploy");
  process.exit(2);
}

const [flag, tin] = process.argv.slice(2);
if (flag === undefined) {
  for (const env of Object.keys(ENVS)) upload(env);
  console.log("Готово: ключи стоят у выкладки всем и у предпросмотров, а копии — в папке kvartik-license.");
} else if (flag === "--forget" && /^\d{8}$/.test(tin ?? "")) {
  const id = crypto.createHmac("sha256", key("production", false)).update(tin).digest("hex");
  const sql = `DELETE FROM trials WHERE tin_mac = '${id}'`;
  if (!wrangler(["d1", "execute", ENVS.production, "--remote", "--config", CONFIG, "--command", sql])) failed();
  console.log(`Запись ՀՎՀՀ ${tin} снята: пробный период на нём начнётся с ближайшего вопроса программы.`);
} else {
  console.error("использование: node tools/deploy/trial-key.mjs [--forget <ИНН из 8 цифр>]");
  process.exit(2);
}
