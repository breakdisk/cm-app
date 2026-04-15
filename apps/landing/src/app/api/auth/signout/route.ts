import { NextResponse } from "next/server";
import { clearAllAuthCookies } from "@/lib/los-cookies";

export async function POST() {
  const baseUrl =
    process.env.NODE_ENV === "production"
      ? "https://os.cargomarket.net"
      : "http://localhost:3004";

  const res = NextResponse.redirect(new URL("/", baseUrl));
  clearAllAuthCookies(res);
  return res;
}
