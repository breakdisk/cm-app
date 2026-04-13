import { NextRequest, NextResponse } from "next/server";
import { verifySession } from "@/lib/firebase/admin";

const LANDING_URL = process.env.LANDING_URL ?? "http://localhost:3004";

export async function middleware(req: NextRequest) {
  const token = req.cookies.get("__session")?.value;
  if (!token) return redirectToLogin(req);

  const session = await verifySession(token);
  if (!session) return redirectToLogin(req);

  if (session.role && session.role !== "admin") {
    return redirectToLogin(req, "unauthorized");
  }

  return NextResponse.next();
}

function redirectToLogin(req: NextRequest, error?: string) {
  const url = new URL(`${LANDING_URL}/login`);
  url.searchParams.set("role", "admin");
  if (error) url.searchParams.set("error", error);
  return NextResponse.redirect(url);
}

export const config = {
  matcher: ["/((?!_next/static|_next/image|favicon.ico).*)"],
};
