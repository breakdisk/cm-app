import { chromium } from "../../apps/admin-portal/node_modules/playwright/index.mjs";
import path from "path";
import { fileURLToPath } from "url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

const VIEWPORTS = [
  { name: "mobile-375",   width: 375,  height: 812  },
  { name: "mobile-430",   width: 430,  height: 932  },
  { name: "tablet-768",   width: 768,  height: 1024 },
  { name: "desktop-1280", width: 1280, height: 800  },
  { name: "desktop-1440", width: 1440, height: 900  },
];

const PAGES = [
  { slug: "home",       url: "http://localhost:3002/" },
  { slug: "reschedule", url: "http://localhost:3002/reschedule" },
  { slug: "feedback",   url: "http://localhost:3002/feedback" },
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
