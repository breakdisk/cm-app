import { chromium } from "../../apps/admin-portal/node_modules/playwright/index.mjs";
import path from "path";
import { fileURLToPath } from "url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

const VIEWPORTS = [
  { name: "iphone-14",     width: 390, height: 844 },
  { name: "android-large", width: 412, height: 915 },
];

const PAGES = [
  { slug: "tasks",   url: "http://localhost:8081/" },
  { slug: "map",     url: "http://localhost:8081/map" },
  { slug: "scanner", url: "http://localhost:8081/scanner" },
  { slug: "profile", url: "http://localhost:8081/profile" },
];

const browser = await chromium.launch();

// Warm-up: load once to trigger full bundle compilation
{
  const ctx = await browser.newContext({ viewport: { width: 390, height: 844 } });
  const page = await ctx.newPage();
  console.log("warming up bundle...");
  await page.goto("http://localhost:8081/", { waitUntil: "networkidle", timeout: 60000 }).catch(() => {});
  await page.waitForTimeout(3000);
  await ctx.close();
  console.log("bundle ready");
}

for (const vp of VIEWPORTS) {
  for (const pg of PAGES) {
    const ctx = await browser.newContext({
      viewport: { width: vp.width, height: vp.height },
      userAgent: "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15",
    });
    const page = await ctx.newPage();
    try {
      await page.goto(pg.url, { waitUntil: "domcontentloaded", timeout: 30000 });
      await page.waitForTimeout(3500);
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
