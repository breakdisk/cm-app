import { NextRequest, NextResponse } from "next/server";
import { refreshLosToken, IdentityError } from "@/lib/identity-client";
import {
  COOKIE_LOS_REFRESH,
  clearAllAuthCookies,
  setLosCookies,
} from "@/lib/los-cookies";

/**
 * Server-side refresh for the LogisticOS JWT pair.
 *
 * Portals call this route (via their own `/api/token` proxy) when the
 * access cookie is near-expiry. We read the httpOnly refresh cookie, call
 * identity's public `/v1/auth/refresh`, and rewrite both LogisticOS cookies.
 *
 * Any failure clears all auth cookies and returns 401 so the client drops
 * back to the login page — a stale refresh token is unrecoverable.
 */
export async function POST(_req: NextRequest) {
  const refreshToken = _req.cookies.get(COOKIE_LOS_REFRESH)?.value;
  if (!refreshToken) {
    return NextResponse.json(
      { error: "No refresh token present" },
      { status: 401 },
    );
  }

  try {
    const refreshed = await refreshLosToken(refreshToken);
    const res = NextResponse.json({
      ok: true,
      expires_in: refreshed.expires_in,
    });
    setLosCookies(res, refreshed);
    return res;
  } catch (err) {
    const status = err instanceof IdentityError ? err.status : 502;
    const code   = err instanceof IdentityError ? err.code   : "REFRESH_FAILED";
    console.warn("[refresh] identity refresh failed", status, code);

    const res = NextResponse.json(
      { error: "Refresh failed", code },
      { status: status === 500 ? 502 : status },
    );
    // Stale / revoked refresh token — wipe the slate so the next navigation
    // forces a fresh Firebase sign-in.
    if (status === 401 || status === 403) {
      clearAllAuthCookies(res);
    }
    return res;
  }
}
