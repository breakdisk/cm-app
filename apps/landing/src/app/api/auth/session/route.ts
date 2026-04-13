import { NextRequest, NextResponse } from "next/server";
import { verifySession } from "@/lib/firebase/admin";

const SEVEN_DAYS = 60 * 60 * 24 * 7;

export async function POST(req: NextRequest) {
  const { idToken, role } = await req.json() as { idToken: string; role: string };

  if (!idToken || !role) {
    return NextResponse.json({ error: "Missing idToken or role" }, { status: 400 });
  }

  const session = await verifySession(idToken);
  if (!session) {
    return NextResponse.json({ error: "Invalid token" }, { status: 401 });
  }

  if (session.expired) {
    return NextResponse.json({ error: "Token expired" }, { status: 401 });
  }

  // Enforce role claim matches requested portal (only if claim is already set)
  if (session.role && session.role !== role) {
    return NextResponse.json({ error: "Unauthorized role" }, { status: 403 });
  }

  const res = NextResponse.json({ ok: true });
  res.cookies.set("__session", idToken, {
    httpOnly: true,
    secure:   process.env.NODE_ENV === "production",
    sameSite: "lax",
    maxAge:   SEVEN_DAYS,
    path:     "/",
  });
  return res;
}
