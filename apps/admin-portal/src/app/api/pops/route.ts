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
 * This Next.js API route runs *server-side* inside the Docker network and
 * can reach backend services directly without going through the public URL.
 * The JWT is forwarded from the httpOnly access cookie.
 *
 * Three-level fallback chain
 * ──────────────────────────
 * 1. Pod service direct    → POD_INTERNAL_URL (default: http://logisticos-pod:8011)
 *    Best path. Requires pod container on dokploy-network (see docker-compose.yml).
 *
 * 2. Internal API gateway  → INTERNAL_GATEWAY_URL (default: http://logisticos-api-gateway:8000)
 *    Works once the gateway image is updated with /v1/pops routing.
 *    api-gateway is already on dokploy-network — no docker-compose change needed.
 *
 * 3. Public API gateway    → NEXT_PUBLIC_API_URL (fallback for local dev)
 *    Used when neither Docker hostname resolves (e.g. local dev without Compose).
 *
 * Error shape contract
 * ────────────────────
 * • 200  — POP data (or empty {data:[]})
 * • 400  — missing shipment_id query param
 * • 502  — all three upstream paths threw network errors (unreachable)
 * • 503  — an upstream returned 404, meaning the gateway has no route for
 *          /v1/pops (stale image). Distinct from HTTP 404 which Next.js itself
 *          emits when THIS route file doesn't exist in the deployed build.
 */

/** Pod service directly on the shared Docker network. */
const POD_INTERNAL_URL =
  process.env.POD_INTERNAL_URL ?? "http://logisticos-pod:8011";

/** API gateway on the shared Docker network (api-gateway is on dokploy-network). */
const INTERNAL_GATEWAY_URL =
  process.env.INTERNAL_GATEWAY_URL ?? "http://logisticos-api-gateway:8000";

/** Public gateway URL — last resort for local dev or when Docker hostnames fail. */
const PUBLIC_GATEWAY_URL =
  process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:8000";

/** Try a single upstream fetch; return null on network error (don't throw). */
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
    return NextResponse.json(
      { error: "shipment_id query parameter is required" },
      { status: 400 }
    );
  }

  // Forward the JWT from the httpOnly cookie — the browser can't read it,
  // but the Next.js server can forward it to the backend.
  const token = request.cookies.get(COOKIE_LOS_ACCESS)?.value;
  const headers: HeadersInit = {
    "Content-Type": "application/json",
    ...(token ? { Authorization: `Bearer ${token}` } : {}),
  };

  const query = `shipment_id=${shipmentId}`;

  // ── Level 1: pod service directly (fastest, no gateway hop) ──────────────
  let res =
    await tryFetch(`${POD_INTERNAL_URL}/v1/pops?${query}`, headers);

  // ── Level 2: internal gateway /v1/pops (updated image required) ─────────
  if (!res) {
    res = await tryFetch(`${INTERNAL_GATEWAY_URL}/v1/pops?${query}`, headers);
  }

  // ── Level 3: internal gateway /v1/pod/pops alias (old-image compat) ──────
  // Old gateway images route /v1/pod* → pod service, so /v1/pod/pops works
  // even before the gateway is redeployed with explicit /v1/pops routing.
  if (!res || res.status === 404) {
    const aliasRes = await tryFetch(`${INTERNAL_GATEWAY_URL}/v1/pod/pops?${query}`, headers);
    if (aliasRes && aliasRes.status !== 404) res = aliasRes;
  }

  // ── Level 4: public gateway URL (local dev fallback) ─────────────────────
  if (!res) {
    res = await tryFetch(`${PUBLIC_GATEWAY_URL}/v1/pod/pops?${query}`, headers);
  }

  // All three paths threw network errors — nothing is reachable.
  if (!res) {
    return NextResponse.json(
      { error: "POP service unreachable — all upstream paths failed (pod direct, internal gateway, public gateway)" },
      { status: 502 }
    );
  }

  // A 404 from any upstream means the gateway has no route for /v1/pops
  // (stale image pre-dating the routing fix). Return 503 so the client can
  // distinguish "proxy route missing" (real 404 from Next.js) from
  // "gateway routing outdated" (503 from this proxy).
  if (res.status === 404) {
    return NextResponse.json(
      { error: "API gateway has no route for /v1/pops — redeploy with: docker compose pull api-gateway && docker compose up -d --no-deps api-gateway" },
      { status: 503 }
    );
  }

  let body: unknown;
  try {
    body = await res.json();
  } catch {
    body = {};
  }

  return NextResponse.json(body, { status: res.status });
}
