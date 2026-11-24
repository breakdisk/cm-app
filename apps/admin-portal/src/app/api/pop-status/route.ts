import { NextRequest, NextResponse } from "next/server";
import { COOKIE_LOS_ACCESS } from "@/lib/auth/los-cookies";

/**
 * Server-side proxy for batch Proof of Pickup completion status.
 *
 * Why this route exists
 * ─────────────────────
 * The pod service exposes GET /v1/internal/pop-status — a no-JWT
 * service-to-service endpoint protected by Istio mTLS in production.
 * Browser code cannot call it directly (wrong network, no mTLS). This BFF
 * runs inside the Docker/Istio network and can reach the pod container
 * directly without going through the public gateway.
 *
 * Request shape
 * ─────────────
 * GET /api/pop-status?shipment_ids=<uuid>&shipment_ids=<uuid>...
 * (the tenant is taken from the session cookie and passed to the pod service)
 *
 * Response shape (mirrors pod service)
 * ─────────────────────────────────────
 * {
 *   statuses: [{ shipment_id: string; has_completed_pop: boolean | null }];
 *   all_completed: boolean | null;
 * }
 *
 * On error / degradation: all statuses are returned as null (unknown),
 * so the caller can render a neutral "unknown" indicator rather than an error.
 */

const POD_INTERNAL_URL =
  process.env.POD_INTERNAL_URL ?? "http://logisticos-pod:8011";

const INTERNAL_GATEWAY_URL =
  process.env.INTERNAL_GATEWAY_URL ?? "http://logisticos-api-gateway:8000";

/**
 * The tenant the signed-in user belongs to, from the access cookie. The pod
 * service requires it and answers only for that tenant's shipments. The
 * payload is read, not verified, here (same as /api/auth/me): this route
 * returns completion booleans only, and the pod service never widens beyond
 * the tenant it is told.
 */
function tenantFromCookie(request: NextRequest): string | null {
  const token = request.cookies.get(COOKIE_LOS_ACCESS)?.value;
  const part = token?.split(".")[1];
  if (!part) return null;
  try {
    const json = Buffer.from(part.replace(/-/g, "+").replace(/_/g, "/"), "base64").toString("utf-8");
    const tenant = (JSON.parse(json) as { tenant_id?: unknown }).tenant_id;
    return typeof tenant === "string" ? tenant : null;
  } catch {
    return null;
  }
}

async function tryFetch(url: string): Promise<Response | null> {
  try {
    return await fetch(url, { cache: "no-store" });
  } catch {
    return null;
  }
}

export async function GET(request: NextRequest) {
  const ids = request.nextUrl.searchParams.getAll("shipment_ids");

  if (ids.length === 0) {
    return NextResponse.json({ statuses: [], all_completed: true });
  }

  const tenantId = tenantFromCookie(request);
  if (!tenantId) {
    return NextResponse.json({ error: "Not authenticated" }, { status: 401 });
  }

  const qs = `tenant_id=${encodeURIComponent(tenantId)}&` + ids.map((id) => `shipment_ids=${encodeURIComponent(id)}`).join("&");

  // Try pod service directly first (fastest; no gateway hop)
  let res = await tryFetch(`${POD_INTERNAL_URL}/v1/internal/pop-status?${qs}`);

  // Fall through to gateway if pod direct failed or returned non-2xx
  if (!res || !res.ok) {
    res = await tryFetch(`${INTERNAL_GATEWAY_URL}/v1/internal/pop-status?${qs}`);
  }

  if (res && res.ok) {
    const body = await res.json();
    return NextResponse.json(body);
  }

  // Graceful degradation — pod service unreachable or pre-dates this endpoint.
  // Return unknown (null) for every id so the caller shows a neutral indicator.
  return NextResponse.json({
    statuses: ids.map((id) => ({ shipment_id: id, has_completed_pop: null })),
    all_completed: null,
  });
}
