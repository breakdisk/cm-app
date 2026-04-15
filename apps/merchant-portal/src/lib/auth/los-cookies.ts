import type { NextResponse } from "next/server";

/**
 * Mirror of landing's cookie names — portals read & rewrite the same three
 * cookies via their own `/api/token` and `/api/auth/refresh` routes.
 */
export const COOKIE_SESSION     = "__session";
export const COOKIE_LOS_ACCESS  = "__los_at";
export const COOKIE_LOS_REFRESH = "__los_rt";

const THIRTY_DAYS = 60 * 60 * 24 * 30;

export interface LosTokenPair {
  access_token: string;
  refresh_token: string;
  expires_in: number;
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
