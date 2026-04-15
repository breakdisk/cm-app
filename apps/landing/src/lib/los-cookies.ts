import type { NextResponse } from "next/server";

/**
 * Canonical cookie names for the Firebase → LogisticOS JWT bridge.
 *
 * - __session : Firebase ID token, used by this landing app's Next middleware
 *               and `verifySession` helper. Kept for backward compatibility and
 *               as the source-of-truth for "is this user still signed in with
 *               Firebase" checks (so we can force re-auth if Firebase revokes).
 * - __los_at  : LogisticOS access JWT (short-lived, ~15m). Carries tenant_id,
 *               permissions, and the `onboarding` flag.
 * - __los_rt  : LogisticOS refresh JWT (long-lived, ~30d). Used exclusively by
 *               the server-side `/api/auth/refresh` route to mint a fresh pair.
 *
 * All three are `httpOnly` and set at `path=/` on the parent landing domain so
 * portals at `/merchant`, `/admin`, `/customer`, `/partner` see them via the
 * rewrite proxy.
 */
export const COOKIE_SESSION = "__session";
export const COOKIE_LOS_ACCESS = "__los_at";
export const COOKIE_LOS_REFRESH = "__los_rt";

const SEVEN_DAYS = 60 * 60 * 24 * 7;
const THIRTY_DAYS = 60 * 60 * 24 * 30;

export interface LosTokenPair {
  access_token: string;
  refresh_token: string;
  expires_in: number;
}

export function setSessionCookie(res: NextResponse, idToken: string): void {
  res.cookies.set(COOKIE_SESSION, idToken, {
    httpOnly: true,
    secure:   process.env.NODE_ENV === "production",
    sameSite: "lax",
    maxAge:   SEVEN_DAYS,
    path:     "/",
  });
}

export function setLosCookies(res: NextResponse, tokens: LosTokenPair): void {
  const secure = process.env.NODE_ENV === "production";
  res.cookies.set(COOKIE_LOS_ACCESS, tokens.access_token, {
    httpOnly: true,
    secure,
    sameSite: "lax",
    maxAge:   tokens.expires_in,
    path:     "/",
  });
  res.cookies.set(COOKIE_LOS_REFRESH, tokens.refresh_token, {
    httpOnly: true,
    secure,
    sameSite: "lax",
    maxAge:   THIRTY_DAYS,
    path:     "/",
  });
}

export function clearAllAuthCookies(res: NextResponse): void {
  for (const name of [COOKIE_SESSION, COOKIE_LOS_ACCESS, COOKIE_LOS_REFRESH]) {
    res.cookies.set(name, "", { maxAge: 0, path: "/" });
  }
}
