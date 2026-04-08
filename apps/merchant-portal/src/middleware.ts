import { NextRequest, NextResponse } from "next/server";

const LANDING_URL = process.env.LANDING_URL ?? "http://localhost:3004";

// Edge-safe: only check cookie presence.
// Full Firebase session cookie verification is done in server components (Node.js runtime).
export function middleware(req: NextRequest) {
  const token = req.cookies.get("__session")?.value;
  if (!token) {
    const url = new URL(`${LANDING_URL}/login`);
    url.searchParams.set("role", "merchant");
    return NextResponse.redirect(url);
  }
  return NextResponse.next();
}

export const config = {
  matcher: ["/((?!_next/static|_next/image|favicon.ico).*)"],
};
