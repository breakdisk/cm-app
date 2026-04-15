import { test, expect } from "@playwright/test";
import { seedLosSession, testTokensFromEnv } from "../fixtures/cookies";

/**
 * Draft-tenant onboarding. Requires a TEST_DRAFT_* token pair (access token
 * must carry onboarding=true with tenants:update-self). The CI seed script
 * produces these alongside the regular TEST_LOS_* pair.
 */

const LANDING_URL = process.env.LANDING_URL ?? "http://localhost:3000";

function draftTokensFromEnv() {
  const access  = process.env.TEST_DRAFT_ACCESS_TOKEN;
  const refresh = process.env.TEST_DRAFT_REFRESH_TOKEN;
  if (!access || !refresh) {
    throw new Error(
      "TEST_DRAFT_ACCESS_TOKEN / TEST_DRAFT_REFRESH_TOKEN not set. " +
      "scripts/e2e-seed.sh should create a draft tenant and export these.",
    );
  }
  return { access, refresh };
}

test.describe("/setup onboarding flow", () => {
  test("page loads and renders the form", async ({ page }) => {
    await page.goto(`${LANDING_URL}/setup?role=merchant`);
    await expect(page.getByRole("heading", { name: /Finish setting up/i })).toBeVisible();
    await expect(page.getByPlaceholder(/Cargo Market/i)).toBeVisible();
  });

  test("finalize endpoint rejects missing fields", async ({ browser }) => {
    const context = await browser.newContext();
    await seedLosSession(context, LANDING_URL, draftTokensFromEnv());

    const res = await context.request.post(`${LANDING_URL}/api/tenants/finalize`, {
      headers: { "Content-Type": "application/json", "X-LogisticOS-Client": "web" },
      data:    { business_name: "" },
    });
    expect(res.status()).toBe(400);
    await context.close();
  });

  test("finalize endpoint promotes tenant and refreshes cookies", async ({ browser }) => {
    const context = await browser.newContext();
    await seedLosSession(context, LANDING_URL, draftTokensFromEnv());

    const res = await context.request.post(`${LANDING_URL}/api/tenants/finalize`, {
      headers: { "Content-Type": "application/json", "X-LogisticOS-Client": "web" },
      data:    {
        business_name: `Playwright E2E ${Date.now()}`,
        currency:      "USD",
        region:        "US",
      },
    });
    expect(res.status()).toBe(200);
    const body = await res.json() as { ok: boolean; tenant: { status: string } };
    expect(body.ok).toBe(true);
    expect(body.tenant.status).toBe("active");
    await context.close();
  });

  test("unauthenticated finalize is rejected", async ({ request }) => {
    const res = await request.post(`${LANDING_URL}/api/tenants/finalize`, {
      headers: { "Content-Type": "application/json", "X-LogisticOS-Client": "web" },
      data:    { business_name: "Nope", currency: "USD", region: "US" },
    });
    expect(res.status()).toBe(401);
  });
});
