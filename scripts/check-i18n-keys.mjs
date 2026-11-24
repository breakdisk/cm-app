#!/usr/bin/env node
// Fails (exit 1) when English and Arabic translation keys differ.
//   - Web: apps/<app>/messages/{en,ar}.json, flattened to dotted keys.
//   - Android: apps/driver-app-android values/strings.xml vs values-ar/strings.xml
//     (strings marked translatable="false" are skipped).
// No dependencies. Run from anywhere: node scripts/check-i18n-keys.mjs
import { readFileSync, existsSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const WEB_APPS = ["partner-portal", "admin-portal", "customer-portal", "landing"];
const ANDROID_RES = "apps/driver-app-android/app/src/main/res";

const problems = [];

function flatten(obj, prefix = "", out = new Set()) {
  for (const [k, v] of Object.entries(obj)) {
    const key = prefix ? `${prefix}.${k}` : k;
    if (v !== null && typeof v === "object" && !Array.isArray(v)) flatten(v, key, out);
    else out.add(key);
  }
  return out;
}

function compare(label, enKeys, arKeys) {
  for (const k of [...enKeys].sort()) {
    if (!arKeys.has(k)) problems.push(`${label}: missing in Arabic: ${k}`);
  }
  for (const k of [...arKeys].sort()) {
    if (!enKeys.has(k)) problems.push(`${label}: missing in English: ${k}`);
  }
}

function readJson(path) {
  try {
    return JSON.parse(readFileSync(path, "utf8"));
  } catch (e) {
    problems.push(`${path}: cannot read/parse (${e.message})`);
    return null;
  }
}

for (const app of WEB_APPS) {
  const dir = join(root, "apps", app, "messages");
  const en = readJson(join(dir, "en.json"));
  const ar = readJson(join(dir, "ar.json"));
  if (en && ar) compare(`apps/${app}`, flatten(en), flatten(ar));
}

function androidNames(path) {
  const xml = readFileSync(path, "utf8");
  const names = new Set();
  for (const m of xml.matchAll(/<(string|plurals|string-array)\s+([^>]*)>/g)) {
    const attrs = m[2];
    if (/translatable\s*=\s*"false"/.test(attrs)) continue;
    const name = /name\s*=\s*"([^"]+)"/.exec(attrs);
    if (name) names.add(name[1]);
  }
  return names;
}

const enXml = join(root, ANDROID_RES, "values", "strings.xml");
const arXml = join(root, ANDROID_RES, "values-ar", "strings.xml");
if (!existsSync(enXml)) problems.push(`${enXml}: not found`);
else if (!existsSync(arXml)) problems.push(`${arXml}: not found`);
else compare("android/driver-app", androidNames(enXml), androidNames(arXml));

if (problems.length) {
  console.error(`i18n key check FAILED (${problems.length}):`);
  for (const p of problems) console.error(`  - ${p}`);
  process.exit(1);
}
console.log(`i18n key check passed: ${WEB_APPS.length} web apps + Android driver app.`);
