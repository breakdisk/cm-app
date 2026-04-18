import { NextResponse } from "next/server";
import { clearAllAuthCookies } from "@/lib/auth/los-cookies";

/**
 * Portal-side signout. Clears the three auth cookies (`__session`, `__los_at`,
 * `__los_rt`) on the shared domain so the next page load lands on the landing
 * app's login. Mirrors `apps/landing/src/app/api/auth/signout` but runs on the
 * portal subtree — Next rewrites don't forward `/api/*` into the landing
 * origin, so each portal owns its own copy.
 */
export async function POST() {
  const res = NextResponse.json({ ok: true });
  clearAllAuthCookies(res);
  return res;
}
