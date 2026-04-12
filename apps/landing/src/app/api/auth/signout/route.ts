import { NextResponse } from "next/server";

export async function POST() {
  const baseUrl =
    process.env.NODE_ENV === "production"
      ? "https://os.cargomarket.net"
      : "http://localhost:3004";

  const res = NextResponse.redirect(new URL("/", baseUrl));
  res.cookies.set("__session", "", { maxAge: 0, path: "/" });
  return res;
}
