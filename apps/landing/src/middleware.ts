import { NextResponse } from "next/server";

// TEMPORARY: Firebase Auth enforcement is disabled in middleware.
// The Edge runtime cannot import `firebase-admin` (transitive node:net / node:path).
// Proper fix: verify Firebase ID tokens with `jose` + Google JWKS.
// Tracked as tech debt — see memory/project_firebase_auth_plan.md.
export function middleware() {
  return NextResponse.next();
}

export const config = {
  matcher: ["/merchant/:path*", "/admin/:path*", "/partner/:path*", "/customer/:path*"],
};
