import { test, expect } from "@playwright/test";
import { seedLosSession, testTokensFromEnv, COOKIE_LOS_ACCESS, COOKIE_LOS_REFRESH } from "../fixtures/cookies";

/**
 * /api/auth/refresh: given a valid refresh cookie, mint a new access+refresh
 * pair and rewrite both cookies. This is the silent-refresh path authFetch
 * relies on after a 401.
 */

const LANDING_URL  = process.env.LANDING_URL  ?? "http://localhost:3000";
const MERCHANT_URL = process.env.MERCHANT_URL ?? "http://localhost:3002";

test.describe("auth refresh", () => {
  test("401 with no refresh cookie clears session", async ({ request }) => {
    const res = await request.post(`${LANDING_URL}/api/auth/refresh`, {
      headers: { "X-LogisticOS-Client": "web" },
    });
    expect(res.status()).toBe(401);
  });

  test("200 rewrites both cookies with a valid refresh token", async ({ browser }) => {
    const context = await browser.newContext();
    const seeded = testTokensFromEnv();
    await seedLosSession(context, LANDING_URL, seeded);

    const res = await context.request.post(`${LANDING_URL}/api/auth/refresh`, {
      headers: { "X-LogisticOS-Client": "web" },
    });
    expect(res.status()).toBe(200);

    const cookies = await context.cookies(LANDING_URL);
    const access  = cookies.find((c) => c.name === COOKIE_LOS_ACCESS);
    const refresh = cookies.find((c) => c.name === COOKIE_LOS_REFRESH);

    expect(access?.value).toBeTruthy();
    expect(refresh?.value).toBeTruthy();
    // Refresh rotation: the new refresh should differ from the seeded one.
    expect(refresh?.value).not.toBe(seeded.refresh);
    expect(access?.httpOnly).toBe(true);
    expect(refresh?.httpOnly).toBe(true);

    await context.close();
  });

  test("portal refresh proxy also works", async ({ browser }) => {
    const context = await browser.newContext();
    await seedLosSession(context, MERCHANT_URL, testTokensFromEnv());

    const res = await context.request.post(`${MERCHANT_URL}/merchant/api/auth/refresh`, {
      headers: { "X-LogisticOS-Client": "web" },
    });
    expect(res.status()).toBe(200);
    await context.close();
  });
});
