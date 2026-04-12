import { NextRequest, NextResponse } from "next/server";
import { verifySession } from "@/lib/firebase/admin";

const PORTAL_ROLES: Record<string, string> = {
  merchant: "merchant",
  admin:    "admin",
  partner:  "partner",
  customer: "customer",
};

export async function middleware(req: NextRequest) {
  const { pathname } = req.nextUrl;

  // Extract portal prefix (e.g. "merchant" from "/merchant/dashboard")
  const prefix = pathname.split("/")[1];
  const requiredRole = PORTAL_ROLES[prefix];

  // Not a protected path — allow through
  if (!requiredRole) return NextResponse.next();

  const token = req.cookies.get("__session")?.value;
  if (!token) return redirectToLogin(req, prefix);

  const session = await verifySession(token);
  if (!session) return redirectToLogin(req, prefix);

  // Expired token — redirect with expired flag so login page shows message
  if (session.expired) return redirectToLogin(req, prefix, "expired");

  // If user has no role claim yet, allow access (claim set after first login)
  if (session.role && session.role !== requiredRole) {
    return redirectToLogin(req, prefix, "unauthorized");
  }

  return NextResponse.next();
}

function redirectToLogin(req: NextRequest, role: string, error?: string) {
  const url = req.nextUrl.clone();
  url.pathname = "/login";
  url.searchParams.set("role", role);
  if (error) url.searchParams.set("error", error);
  return NextResponse.redirect(url);
}

export const config = {
  matcher: ["/merchant/:path*", "/admin/:path*", "/partner/:path*", "/customer/:path*"],
};
