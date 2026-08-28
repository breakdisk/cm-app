/**
 * Tenant self-service API — wraps identity's `GET /v1/tenants/me`.
 *
 * The app has no other client-side source for the tenant's billing currency
 * (no JWT decode anywhere, no `currency` field on the auth or branding
 * response), so any screen that needs to know whether the signed-in
 * customer's tenant bills in AED reads it from here instead of inferring it
 * from a probe request against an unrelated endpoint.
 */
import { getIdentityClient } from './client';

/** Mirrors `Tenant` in services/identity/src/domain/entities/tenant.rs —
 *  no `#[serde(rename_all)]` on that struct, so field names pass through
 *  as-is (snake_case, matching the Rust field names verbatim). */
export interface TenantSelf {
  id: string;
  name: string;
  slug: string;
  subscription_tier: string;
  is_active: boolean;
  status: string;
  owner_email: string;
  /** ISO 4217 currency code (e.g. "AED", "PHP"). Null for tenants created
   *  before the currency/region migration. */
  currency: string | null;
  region: string | null;
  created_at: string;
  updated_at: string;
}

// Cached for the app session — the signed-in customer's tenant doesn't
// change mid-session, and this is consulted on every render of screens that
// gate a feature on tenant currency, so avoid re-fetching on each mount.
let cachedTenant: TenantSelf | null = null;
let inFlight: Promise<TenantSelf> | null = null;

/**
 * Fetch (and cache) the authenticated user's tenant. Requires a valid JWT —
 * the endpoint is gated on authentication only, no permission check.
 * Rejects on failure (network error, 401, etc.) — callers that only care
 * about a soft feature gate should catch and fall back to "unavailable"
 * rather than surfacing an error to the user.
 */
export async function getMyTenant(): Promise<TenantSelf> {
  if (cachedTenant) return cachedTenant;
  if (!inFlight) {
    const client = getIdentityClient();
    inFlight = client
      .get<{ data: TenantSelf }>('/v1/tenants/me')
      .then(response => {
        cachedTenant = response.data.data;
        return cachedTenant;
      })
      .finally(() => {
        inFlight = null;
      });
  }
  return inFlight;
}

/**
 * Drop the cached tenant. Call this on logout so that if a different tenant
 * signs in during the same app session (e.g. a shared device), stale
 * currency/region data from the previous tenant is never reused.
 */
export function clearCachedTenant(): void {
  cachedTenant = null;
  inFlight = null;
}
