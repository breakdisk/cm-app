import { chromium } from "../../apps/admin-portal/node_modules/playwright/index.mjs";
import path from "path";
import { fileURLToPath } from "url";
const __dirname = path.dirname(fileURLToPath(import.meta.url));

const browser = await chromium.launch();
const ctx = await browser.newContext({ viewport: { width: 390, height: 844 } });
const page = await ctx.newPage();

const errors = [];
const logs = [];
page.on("console", m => { if (["error","warn"].includes(m.type())) logs.push(`[${m.type()}] ${m.text()}`); });
page.on("pageerror", e => errors.push(e.message));

await page.goto("http://localhost:8081/", { waitUntil: "domcontentloaded", timeout: 20000 });
await page.waitForTimeout(6000);

console.log("=== PAGE ERRORS ===");
errors.slice(0, 5).forEach(e => console.log(e.slice(0, 200)));
console.log("\n=== CONSOLE ERRORS ===");
logs.slice(0, 10).forEach(l => console.log(l.slice(0, 200)));

const bodyText = await page.evaluate(() => document.body.innerText);
console.log("\n=== BODY TEXT ===");
console.log(bodyText.slice(0, 400));

await browser.close();
