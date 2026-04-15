import { defineConfig, devices } from "@playwright/test";

/**
 * Targets a running local stack: landing on 3000, portals on 3001-3004.
 * See e2e/README.md for bring-up instructions. CI points LANDING_URL etc. at
 * the staging deployment so the same specs run against real infrastructure.
 */
const LANDING_URL  = process.env.LANDING_URL  ?? "http://localhost:3000";
const MERCHANT_URL = process.env.MERCHANT_URL ?? "http://localhost:3002";
const ADMIN_URL    = process.env.ADMIN_URL    ?? "http://localhost:3001";
const PARTNER_URL  = process.env.PARTNER_URL  ?? "http://localhost:3004";
const CUSTOMER_URL = process.env.CUSTOMER_URL ?? "http://localhost:3003";

export default defineConfig({
  testDir:       "./specs",
  fullyParallel: true,
  forbidOnly:   !!process.env.CI,
  retries:       process.env.CI ? 2 : 0,
  workers:       process.env.CI ? 1 : undefined,
  reporter:      process.env.CI ? [["github"], ["html", { open: "never" }]] : "list",
  timeout:       30_000,

  use: {
    baseURL:           LANDING_URL,
    trace:             "on-first-retry",
    screenshot:        "only-on-failure",
    actionTimeout:     10_000,
    navigationTimeout: 15_000,
    extraHTTPHeaders: {
      "X-LogisticOS-Client": "web",
    },
  },

  projects: [
    {
      name: "chromium",
      use:  { ...devices["Desktop Chrome"] },
    },
  ],

  metadata: {
    urls: {
      landing:  LANDING_URL,
      merchant: MERCHANT_URL,
      admin:    ADMIN_URL,
      partner:  PARTNER_URL,
      customer: CUSTOMER_URL,
    },
  },
});
