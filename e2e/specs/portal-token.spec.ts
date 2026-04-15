import { test, expect } from "@playwright/test";
import { seedLosSession, testTokensFromEnv } from "../fixtures/cookies";

/**
 * /api/token is the bridge between the httpOnly __los_at cookie and the
 * browser-side authFetch helper. It MUST return 401 when no cookie is present
 * and 200 with the token value when the cookie is valid. Breakage here
 * cascades into every portal call site.
 */

const MERCHANT_URL = process.env.MERCHANT_URL ?? "http://localhost:3002";
const ADMIN_URL    = process.env.ADMIN_URL    ?? "http://localhost:3001";
const PARTNER_URL  = process.env.PARTNER_URL  ?? "http://localhost:3004";
const CUSTOMER_URL = process.env.CUSTOMER_URL ?? "http://localhost:3003";

const PORTALS = [
  { name: "merchant", url: MERCHANT_URL, prefix: "/merchant" },
  { name: "admin",    url: ADMIN_URL,    prefix: "/admin"    },
  { name: "partner",  url: PARTNER_URL,  prefix: "/partner"  },
  { name: "customer", url: CUSTOMER_URL, prefix: "/customer" },
];

for (const portal of PORTALS) {
  test.describe(`${portal.name} portal /api/token`, () => {
    test("401 without auth cookie", async ({ request }) => {
      const res = await request.get(`${portal.url}${portal.prefix}/api/token`, {
        headers: { "X-LogisticOS-Client": "web" },
      });
      expect(res.status()).toBe(401);
    });

    test("200 with seeded auth cookie", async ({ browser }) => {
      const context = await browser.newContext();
      await seedLosSession(context, portal.url, testTokensFromEnv());
      const res = await context.request.get(`${portal.url}${portal.prefix}/api/token`, {
        headers: { "X-LogisticOS-Client": "web" },
      });
      expect(res.status()).toBe(200);
      const body = await res.json() as { access_token: string };
      expect(body.access_token).toMatch(/^eyJ/); // JWT header prefix
      await context.close();
    });
  });
}
