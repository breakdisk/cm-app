import { NextRequest, NextResponse } from "next/server";
import {
  finalizeTenantSelf,
  refreshLosToken,
  IdentityError,
} from "@/lib/identity-client";
import {
  COOKIE_LOS_ACCESS,
  COOKIE_LOS_REFRESH,
  setLosCookies,
} from "@/lib/los-cookies";

/**
 * Draft-tenant onboarding exit.
 *
 * Promotes the caller's tenant from `draft` → `active`, then immediately
 * re-refreshes the LogisticOS JWT pair so the new cookies carry the full
 * role-based permission set (and `onboarding=false`). Portals can then use
 * those cookies directly without a second round trip.
 */
export async function POST(req: NextRequest) {
  const access  = req.cookies.get(COOKIE_LOS_ACCESS)?.value;
  const refresh = req.cookies.get(COOKIE_LOS_REFRESH)?.value;

  if (!access || !refresh) {
    return NextResponse.json(
      { error: "Not authenticated", code: "NO_SESSION" },
      { status: 401 },
    );
  }

  let body: unknown;
  try {
    body = await req.json();
  } catch {
    return NextResponse.json(
      { error: "Invalid JSON body", code: "BAD_REQUEST" },
      { status: 400 },
    );
  }

  const { business_name, currency, region } = (body ?? {}) as Record<
    string,
    string | undefined
  >;
  if (!business_name || !currency || !region) {
    return NextResponse.json(
      { error: "business_name, currency, region are required", code: "BAD_REQUEST" },
      { status: 400 },
    );
  }

  try {
    const tenant = await finalizeTenantSelf(access, { business_name, currency, region });
    const refreshed = await refreshLosToken(refresh);

    const res = NextResponse.json({
      ok:     true,
      tenant: { id: tenant.tenant_id, slug: tenant.slug, status: tenant.status },
    });
    setLosCookies(res, refreshed);
    return res;
  } catch (err) {
    const status = err instanceof IdentityError ? err.status : 502;
    const code   = err instanceof IdentityError ? err.code   : "FINALIZE_FAILED";
    const message =
      err instanceof IdentityError ? err.message : "Tenant finalize failed";
    console.warn("[tenants/finalize] identity call failed", status, code);
    return NextResponse.json(
      { error: message, code },
      { status: status === 500 ? 502 : status },
    );
  }
}
