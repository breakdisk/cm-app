import { NextResponse, type NextRequest } from "next/server";

// TEMPORARY: Firebase Auth enforcement is disabled in middleware.
// The Edge runtime cannot import `firebase-admin` (transitive node:net / node:path).
// Proper fix: verify Firebase ID tokens with `jose` + Google JWKS.
// Tracked as tech debt — see memory/project_firebase_auth_plan.md.

/**
 * Hosts this app serves as itself.
 *
 * Anything else arriving here is assumed to be a vendor who pointed a CNAME at
 * us, and its root is rewritten to that vendor's public storefront.
 *
 * The suffix list mirrors `RESERVED_DOMAIN_SUFFIXES` in
 * `services/omnideliv/src/domain/entities/vendor.rs`, which refuses to let a
 * vendor *claim* one of these. The two must agree: if a host is claimable there
 * but treated as ours here, the vendor's domain silently never resolves.
 *
 * `PLATFORM_HOSTS` (comma-separated suffixes) extends it without a code change,
 * which is what a new platform domain needs.
 */
const PLATFORM_SUFFIXES = (
  process.env.PLATFORM_HOSTS ?? "cargomarket.net,logisticos.io,localhost"
)
  .split(",")
  .map((h) => h.trim().toLowerCase())
  .filter(Boolean);

function isPlatformHost(host: string): boolean {
  return PLATFORM_SUFFIXES.some((s) => host === s || host.endsWith(`.${s}`));
}

export function middleware(req: NextRequest) {
  const host = (req.headers.get("host") ?? "").split(":")[0].toLowerCase();

  /**
   * Custom vendor domains.
   *
   * **Only the root path is rewritten**, deliberately. If the host check is
   * ever wrong — a new platform domain nobody added to the list — the blast
   * radius is one page showing a 404 instead of the marketing site, rather
   * than every route on the origin disappearing.
   *
   * The rewrite passes the HOST as the handle. The API resolves a slug and a
   * custom domain through the same lookup, so `menu.kanto.ph` is simply
   * another public name for the same storefront.
   */
  if (host && !isPlatformHost(host) && req.nextUrl.pathname === "/") {
    const url = req.nextUrl.clone();
    url.pathname = `/s/${host}`;
    return NextResponse.rewrite(url);
  }

  return NextResponse.next();
}

export const config = {
  // The portal prefixes keep their existing (currently no-op) pass-through.
  // `/` is added so a custom domain's root can be rewritten; everything under
  // _next, static assets and the API is excluded so this never sits in front of
  // a bundle or a data request.
  matcher: [
    "/",
    "/merchant/:path*",
    "/admin/:path*",
    "/partner/:path*",
    "/customer/:path*",
  ],
};
