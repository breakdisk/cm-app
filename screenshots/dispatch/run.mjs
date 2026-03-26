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

const browser = await chromium.launch();

for (const vp of VIEWPORTS) {
  const ctx  = await browser.newContext({ viewport: { width: vp.width, height: vp.height } });
  const page = await ctx.newPage();
  await page.goto("http://localhost:3001/dispatch", { waitUntil: "domcontentloaded", timeout: 20000 });
  await page.waitForTimeout(2500);
  const file = path.join(__dirname, `${vp.name}.png`);
  await page.screenshot({ path: file, fullPage: false });
  console.log(`saved ${vp.name}.png`);
  await ctx.close();
}

await browser.close();
console.log("done");
