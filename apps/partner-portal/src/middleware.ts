import { NextRequest, NextResponse } from "next/server";

const PUBLIC_PATHS = ["/login", "/api/", "/verify-email", "/reset-password"];

function isPublic(pathname: string): boolean {
  return PUBLIC_PATHS.some((p) => pathname.startsWith(p));
}

export function middleware(req: NextRequest) {
  const { pathname } = req.nextUrl;
  if (isPublic(pathname)) return NextResponse.next();

  const token = req.cookies.get("__los_at");
  if (!token) {
    const loginUrl = new URL("/login", req.url);
    loginUrl.searchParams.set("role", "partner");
    loginUrl.searchParams.set("returnTo", pathname);
    return NextResponse.redirect(loginUrl);
  }

  return NextResponse.next();
}

export const config = {
  matcher: ["/((?!_next/static|_next/image|favicon.ico).*)"],
};
