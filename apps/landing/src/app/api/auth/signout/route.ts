import { NextResponse } from "next/server";

export async function POST() {
  const landingUrl = process.env.LANDING_URL ?? "http://localhost:3004";
  const res = NextResponse.redirect(new URL("/", landingUrl));
  res.cookies.set("__session", "", {
    maxAge:   0,
    path:     "/",
    httpOnly: true,
    secure:   process.env.NODE_ENV === "production",
    sameSite: "lax",
  });
  return res;
}
