import { NextRequest, NextResponse } from "next/server";
import {
  COOKIE_LOS_REFRESH,
  clearAllAuthCookies,
  setLosCookies,
} from "@/lib/auth/los-cookies";

/**
 * Portal-side refresh proxy. Reads the `__los_rt` cookie, calls the identity
 * service's public `/v1/auth/refresh`, rewrites both LogisticOS cookies, and
 * returns the new access token so the caller can retry its pending request
 * without a second round-trip.
 *
 * Each portal owns its own copy of this route (rather than proxying to
 * landing) because Next rewrites on landing don't forward arbitrary API
 * paths into portal origins — rewrites only cover `/${role}/*` subtrees.
 */
const IDENTITY_URL =
  process.env.IDENTITY_SERVICE_URL ??
  process.env.IDENTITY_URL ??
  "http://localhost:8001";

export async function POST(req: NextRequest) {
  const refreshToken = req.cookies.get(COOKIE_LOS_REFRESH)?.value;
  if (!refreshToken) {
    return NextResponse.json({ error: "No refresh token present" }, { status: 401 });
  }

  let upstream: Response;
  try {
    upstream = await fetch(`${IDENTITY_URL}/v1/auth/refresh`, {
      method:  "POST",
      headers: { "Content-Type": "application/json" },
      body:    JSON.stringify({ refresh_token: refreshToken }),
      cache:   "no-store",
    });
  } catch (err) {
    console.error("[portal/refresh] identity unreachable", err);
    return NextResponse.json({ error: "Identity unreachable" }, { status: 502 });
  }

  if (!upstream.ok) {
    const res = NextResponse.json(
      { error: "Refresh failed" },
      { status: upstream.status === 500 ? 502 : upstream.status },
    );
    if (upstream.status === 401 || upstream.status === 403) {
      clearAllAuthCookies(res);
    }
    return res;
  }

  const refreshed = (await upstream.json()) as {
    access_token: string;
    refresh_token: string;
    expires_in: number;
  };

  const res = NextResponse.json({
    ok: true,
    access_token: refreshed.access_token,
    expires_in:   refreshed.expires_in,
  });
  setLosCookies(res, refreshed);
  return res;
}
