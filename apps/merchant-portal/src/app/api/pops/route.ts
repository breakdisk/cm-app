import { NextRequest, NextResponse } from "next/server";
import { COOKIE_LOS_ACCESS } from "@/lib/auth/los-cookies";

/**
 * Merchant Portal — Server-side BFF proxy for Proof of Pickup (POP) data.
 *
 * Why this exists
 * ───────────────
 * The merchant portal's primary POP source is the enriched delivery-experience
 * tracking response (`GET /v1/tracking/:id`). This BFF is a resilience fallback:
 * if the delivery-experience service is unavailable or running a stale image
 * without the evidence enrichment, the merchant portal can still fetch POP
 * evidence directly from the pod service via this server-side proxy.
 *
 * Called by: DeliveryReceiptModal on fetch error from delivery-experience.
 *
 * Fallback chain (same pattern as admin-portal /api/pops)
 * ─────────────────────────────────────────────────────────
 * 1. Pod service direct    → POD_INTERNAL_URL
 * 2. Internal API gateway  → INTERNAL_GATEWAY_URL/v1/pops
 * 3. Internal gateway alias → INTERNAL_GATEWAY_URL/v1/pod/pops
 * 4. Public gateway        → NEXT_PUBLIC_API_URL/v1/pod/pops
 */

const POD_INTERNAL_URL    = process.env.POD_INTERNAL_URL    ?? "http://logisticos-pod:8011";
const INTERNAL_GATEWAY_URL = process.env.INTERNAL_GATEWAY_URL ?? "http://logisticos-api-gateway:8000";
const PUBLIC_GATEWAY_URL  = process.env.NEXT_PUBLIC_API_URL  ?? "http://localhost:8000";

async function tryFetch(url: string, headers: HeadersInit): Promise<Response | null> {
  try {
    return await fetch(url, { headers, cache: "no-store" });
  } catch {
    return null;
  }
}

export async function GET(request: NextRequest) {
  const shipmentId = request.nextUrl.searchParams.get("shipment_id");
  if (!shipmentId) {
    return NextResponse.json({ error: "shipment_id query parameter is required" }, { status: 400 });
  }

  const token = request.cookies.get(COOKIE_LOS_ACCESS)?.value;
  const headers: HeadersInit = {
    "Content-Type": "application/json",
    ...(token ? { Authorization: `Bearer ${token}` } : {}),
  };
  const query = `shipment_id=${shipmentId}`;

  // Each leg tries the next upstream when the current one is unreachable
  // (null) or returns any non-2xx status. This ensures a 500 or 503 from a
  // degraded upstream doesn't stop the chain before all paths are exhausted.
  let res = await tryFetch(`${POD_INTERNAL_URL}/v1/pops?${query}`, headers);

  if (!res || !res.ok) {
    const gw = await tryFetch(`${INTERNAL_GATEWAY_URL}/v1/pops?${query}`, headers);
    if (gw?.ok) res = gw;
  }

  if (!res || !res.ok) {
    const alias = await tryFetch(`${INTERNAL_GATEWAY_URL}/v1/pod/pops?${query}`, headers);
    if (alias?.ok) res = alias;
  }

  if (!res || !res.ok) {
    const pub = await tryFetch(`${PUBLIC_GATEWAY_URL}/v1/pod/pops?${query}`, headers);
    if (pub?.ok) res = pub;
  }

  // All legs failed with network errors.
  if (!res) {
    return NextResponse.json(
      { error: "POP service unreachable — all upstream paths failed" },
      { status: 502 },
    );
  }

  // All reachable upstreams returned non-2xx. The best response we have is a
  // route-missing 404 (stale image) → return 503 to distinguish from Next.js
  // file-not-found. Any other error status is passed through as-is.
  if (!res.ok) {
    const hint = res.status === 404
      ? "Pod service has no route for /v1/pops — redeploy pod image"
      : `Upstream error: ${res.status}`;
    return NextResponse.json({ error: hint }, { status: res.status === 404 ? 503 : res.status });
  }

  let body: unknown;
  try { body = await res.json(); } catch { body = {}; }
  return NextResponse.json(body, { status: res.status });
}
