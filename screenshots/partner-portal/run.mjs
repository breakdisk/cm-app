import { chromium } from "../../apps/admin-portal/node_modules/playwright/index.mjs";
import path from "path";
import { fileURLToPath } from "url";
import { mkdirSync } from "fs";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
mkdirSync(__dirname, { recursive: true });

const VIEWPORTS = [
  { name: "mobile-375",   width: 375,  height: 812  },
  { name: "tablet-768",   width: 768,  height: 1024 },
  { name: "desktop-1280", width: 1280, height: 800  },
];

const PAGES = [
  { slug: "overview",  url: "http://localhost:3003/" },
  { slug: "sla",       url: "http://localhost:3003/sla" },
  { slug: "payouts",   url: "http://localhost:3003/payouts" },
  { slug: "manifests", url: "http://localhost:3003/manifests" },
];

const browser = await chromium.launch();

for (const vp of VIEWPORTS) {
  for (const pg of PAGES) {
    const ctx  = await browser.newContext({ viewport: { width: vp.width, height: vp.height } });
    const page = await ctx.newPage();
    try {
      await page.goto(pg.url, { waitUntil: "domcontentloaded", timeout: 15000 });
      await page.waitForTimeout(2000);
      const file = path.join(__dirname, `${vp.name}--${pg.slug}.png`);
      await page.screenshot({ path: file, fullPage: false });
      console.log(`saved ${vp.name}--${pg.slug}.png`);
    } catch(e) {
      console.log(`failed ${vp.name}--${pg.slug}: ${e.message}`);
    }
    await ctx.close();
  }
}

await browser.close();
console.log("done");
