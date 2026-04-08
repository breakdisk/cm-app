import { NextRequest, NextResponse } from "next/server";
import { verifySession, createFirebaseSessionCookie } from "@/lib/firebase/admin";

const SEVEN_DAYS_MS  = 60 * 60 * 24 * 7 * 1000;
const SEVEN_DAYS_SEC = 60 * 60 * 24 * 7;

export async function POST(req: NextRequest) {
  let body: { idToken?: unknown; role?: unknown };
  try {
    body = await req.json();
  } catch {
    return NextResponse.json({ error: "Invalid JSON body" }, { status: 400 });
  }

  const { idToken, role } = body;

  if (typeof idToken !== "string" || !idToken || typeof role !== "string" || !role) {
    return NextResponse.json({ error: "Missing idToken or role" }, { status: 400 });
  }

  // Verify the short-lived ID token first
  const session = await verifySession(idToken);
  if (!session) {
    return NextResponse.json({ error: "Invalid token" }, { status: 401 });
  }

  if (session.expired) {
    return NextResponse.json({ error: "Token expired" }, { status: 401 });
  }

  // Enforce role claim matches the requested portal
  if (session.role && session.role !== role) {
    return NextResponse.json({ error: "Unauthorized role" }, { status: 403 });
  }

  // Exchange the short-lived ID token for a long-lived Firebase session cookie
  let sessionCookie: string;
  try {
    sessionCookie = await createFirebaseSessionCookie(idToken, SEVEN_DAYS_MS);
  } catch {
    return NextResponse.json({ error: "Failed to create session" }, { status: 500 });
  }

  const res = NextResponse.json({ ok: true });
  res.cookies.set("__session", sessionCookie, {
    httpOnly: true,
    secure:   process.env.NODE_ENV === "production",
    sameSite: "lax",
    maxAge:   SEVEN_DAYS_SEC,
    path:     "/",
  });
  return res;
}
