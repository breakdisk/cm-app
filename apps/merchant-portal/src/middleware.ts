import { NextResponse } from "next/server";

// Portal middleware is intentionally a pass-through. Authentication is enforced
// at call time by `authFetch` (which reads the __los_at cookie via the portal's
// own /api/token route) and by each backend service's JWT validation. The
// matcher is retained so the edge runtime continues to build /api/token and
// /api/auth/refresh routes as part of the middleware manifest.
export function middleware() {
  return NextResponse.next();
}

export const config = {
  matcher: ["/((?!_next/static|_next/image|favicon.ico).*)"],
};
