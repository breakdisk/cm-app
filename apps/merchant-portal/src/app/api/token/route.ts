import { NextRequest, NextResponse } from "next/server";
import { COOKIE_LOS_ACCESS } from "@/lib/auth/los-cookies";

/**
 * Return the current LogisticOS access JWT to the browser.
 *
 * The cookie is httpOnly and unreadable from client JS, so the browser calls
 * this route to obtain a bearer token to stamp on outbound API requests.
 * If the cookie is missing, the client should POST `/api/auth/refresh` to
 * mint a new pair from the refresh cookie — handled by `authFetch`.
 */
export async function GET(req: NextRequest) {
  const token = req.cookies.get(COOKIE_LOS_ACCESS)?.value;
  if (!token) {
    return NextResponse.json({ error: "No access token" }, { status: 401 });
  }
  return NextResponse.json({ access_token: token });
}
