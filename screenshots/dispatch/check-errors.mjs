import { chromium } from "../../apps/admin-portal/node_modules/playwright/index.mjs";
import path from "path";
import { fileURLToPath } from "url";
const __dirname = path.dirname(fileURLToPath(import.meta.url));

const browser = await chromium.launch();
const ctx = await browser.newContext({ viewport: { width: 375, height: 812 } });
const page = await ctx.newPage();
const errors = [];
page.on("console", m => { if (m.type() === "error") errors.push(m.text()); });
page.on("pageerror", e => errors.push(e.message));
await page.goto("http://localhost:3001/dispatch", { waitUntil: "domcontentloaded", timeout: 20000 });
await page.waitForTimeout(3000);
console.log("Console errors:", JSON.stringify(errors, null, 2));
await browser.close();
