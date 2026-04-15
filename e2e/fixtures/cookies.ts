import type { APIRequestContext, BrowserContext } from "@playwright/test";

export const COOKIE_SESSION       = "__session";
export const COOKIE_LOS_ACCESS    = "__los_at";
export const COOKIE_LOS_REFRESH   = "__los_rt";

/**
 * Seed LogisticOS auth cookies directly, bypassing Firebase sign-in.
 *
 * The full happy path (Firebase popup → /api/auth/session) is covered by the
 * interactive \`login\` spec. For every other spec we want to exercise portal
 * behavior under a known-good session without re-running the whole dance, so
 * we inject cookies minted by the identity service's test fixture. See
 * \`e2e/README.md\` for how to obtain fresh tokens via \`scripts/e2e-seed.sh\`.
 */
export async function seedLosSession(
  context: BrowserContext,
  url: string,
  tokens: { access: string; refresh: string; firebaseId?: string },
) {
  const parsed = new URL(url);
  await context.addCookies([
    {
      name:     COOKIE_LOS_ACCESS,
      value:    tokens.access,
      domain:   parsed.hostname,
      path:     "/",
      httpOnly: true,
      sameSite: "Lax",
      secure:   parsed.protocol === "https:",
    },
    {
      name:     COOKIE_LOS_REFRESH,
      value:    tokens.refresh,
      domain:   parsed.hostname,
      path:     "/",
      httpOnly: true,
      sameSite: "Lax",
      secure:   parsed.protocol === "https:",
    },
    ...(tokens.firebaseId
      ? [{
          name:     COOKIE_SESSION,
          value:    tokens.firebaseId,
          domain:   parsed.hostname,
          path:     "/",
          httpOnly: true,
          sameSite: "Lax" as const,
          secure:   parsed.protocol === "https:",
        }]
      : []),
  ]);
}

/**
 * Reads LOS_* tokens from env vars. Tests fail loudly if they are missing so
 * CI surfaces "seed step didn't run" rather than "test silently passed".
 */
export function testTokensFromEnv() {
  const access  = process.env.TEST_LOS_ACCESS_TOKEN;
  const refresh = process.env.TEST_LOS_REFRESH_TOKEN;
  if (!access || !refresh) {
    throw new Error(
      "TEST_LOS_ACCESS_TOKEN and TEST_LOS_REFRESH_TOKEN must be set. " +
      "Run scripts/e2e-seed.sh against your identity service to mint a pair.",
    );
  }
  return { access, refresh, firebaseId: process.env.TEST_FIREBASE_ID_TOKEN };
}

export async function apiGet(request: APIRequestContext, url: string, token?: string) {
  return request.get(url, {
    headers: {
      "X-LogisticOS-Client": "web",
      ...(token ? { Authorization: `Bearer ${token}` } : {}),
    },
  });
}
