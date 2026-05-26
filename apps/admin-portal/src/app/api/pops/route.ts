import { NextRequest, NextResponse } from "next/server";
import { COOKIE_LOS_ACCESS } from "@/lib/auth/los-cookies";

/**
 * Server-side proxy for Proof of Pickup (POP) data.
 *
 * Why this route exists
 * ─────────────────────
 * The admin portal browser code cannot call the pod service directly — it must
 * go through the API gateway. If the gateway is running a stale Docker image
 * that pre-dates the /v1/pops routing fix, every POP request returns 404.
 *
 * This Next.js API route runs *server-side*, inside the same Docker network as
 * the backend services. It reaches the pod service at `http://pod:8011`
 * directly — no gateway required. The JWT is read from the httpOnly access
 * cookie (same source as /api/token) and forwarded as `Authorization: Bearer`.
 *
 * Fallback chain
 * ──────────────
 * 1. Pod service directly  → POD_INTERNAL_URL  (default: http://pod:8011)
 * 2. API gateway           → INTERNAL_GATEWAY_URL or NEXT_PUBLIC_API_URL
 *    (handles local-dev without Docker where 'pod' hostname is not resolvable)
 *
 * Once the API gateway is redeployed with the /v1/pops routing, the gateway
 * fallback also works — but the direct path is always preferred for latency.
 */

const POD_INTERNAL_URL =
  process.env.POD_INTERNAL_URL ?? "http://pod:8011";

const GATEWAY_URL =
  process.env.INTERNAL_GATEWAY_URL ??
  process.env.NEXT_PUBLIC_API_URL ??
  "http://localhost:8000";

export async function GET(request: NextRequest) {
  const shipmentId = request.nextUrl.searchParams.get("shipment_id");
  if (!shipmentId) {
    return NextResponse.json(
      { error: "shipment_id query parameter is required" },
      { status: 400 }
    );
  }

  // Read the JWT from the httpOnly cookie — the browser cannot read it
  // directly, but the Next.js server can forward it to the pod service.
  const token = request.cookies.get(COOKIE_LOS_ACCESS)?.value;
  const authHeader = token ? `Bearer ${token}` : "";

  const requestHeaders: HeadersInit = {
    "Content-Type": "application/json",
    ...(authHeader ? { Authorization: authHeader } : {}),
  };

  // ── 1. Try pod service directly (bypasses stale API gateway) ──────────────
  let res: Response | null = null;
  try {
    res = await fetch(
      `${POD_INTERNAL_URL}/v1/pops?shipment_id=${shipmentId}`,
      { headers: requestHeaders, cache: "no-store" }
    );
  } catch {
    // Pod service not directly reachable — fall through to gateway.
    // Expected in local dev environments that don't use Docker Compose.
  }

  // ── 2. Fall back to API gateway (local dev, or direct path failed) ────────
  if (!res) {
    try {
      res = await fetch(
        `${GATEWAY_URL}/v1/pops?shipment_id=${shipmentId}`,
        { headers: requestHeaders, cache: "no-store" }
      );
    } catch {
      return NextResponse.json(
        { error: "POP service unreachable via both direct and gateway paths" },
        { status: 502 }
      );
    }
  }

  let body: unknown;
  try {
    body = await res.json();
  } catch {
    body = {};
  }

  return NextResponse.json(body, { status: res.status });
}
