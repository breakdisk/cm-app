import { createApiClient, ApiResponse } from "./client";

// ── User types ─────────────────────────────────────────────────────────────────

/**
 * Canonical user shape returned by identity /v1/users and /v1/users/:id.
 * Mirrors services/identity/src/domain/entities/user.rs.
 * password_hash is absent — skipped server-side via #[serde(skip_serializing)].
 */
export interface TenantUser {
  id: string | { 0: string };
  tenant_id: string | { 0: string };
  email: string;
  first_name: string;
  last_name: string;
  roles: string[];
  is_active: boolean;
  email_verified: boolean;
  phone_number?: string | null;
  last_login_at?: string | null;
  created_at: string;
  updated_at: string;
}

export interface InviteUserPayload {
  email: string;
  first_name: string;
  last_name: string;
  roles: string[];
  /** E.164 phone number — required for drivers so OTP login resolves to the
   *  pre-registered user rather than creating a duplicate ghost account. */
  phone_number?: string;
}

export interface InviteUserResult {
  user_id: string;
  email: string;
  temp_password: string;
}

// ── Tenant types ───────────────────────────────────────────────────────────────
// Mirrors services/identity/src/domain/entities/tenant.rs.
// subscription_tier is snake_case since SubscriptionTier now has
// #[serde(rename_all = "snake_case")].

export type TenantTier = "starter" | "growth" | "business" | "enterprise";

export interface TenantSnapshot {
  id: string | { 0: string };
  name: string;
  slug: string;
  subscription_tier: TenantTier;
  status: string;
  is_active: boolean;
  owner_email: string;
  created_at: string;
  updated_at: string;
}

export interface UpdateTenantPayload {
  name?: string;
  owner_email?: string;
}

/** Normalise the polymorphic id field the backend may return. */
export function tenantIdOf(t: Pick<TenantSnapshot, "id">): string {
  const raw: unknown = t.id;
  if (typeof raw === "string") return raw;
  if (raw && typeof raw === "object" && "0" in raw) return String((raw as { 0: string })[0]);
  return "";
}

// ── Client ─────────────────────────────────────────────────────────────────────

export function createIdentityApi() {
  const client = createApiClient();

  return {
    // ── Users ──────────────────────────────────────────────────────────────────

    inviteUser: (payload: InviteUserPayload) =>
      client
        .post<ApiResponse<InviteUserResult>>("/v1/users", payload)
        .then((r) => r.data),

    // Backend returns { "data": User[] } — not paginated at this endpoint.
    listUsers: () =>
      client
        .get<ApiResponse<TenantUser[]>>("/v1/users")
        .then((r) => r.data),

    getUser: (userId: string) =>
      client
        .get<ApiResponse<TenantUser>>(`/v1/users/${userId}`)
        .then((r) => r.data),

    // ── Tenant ─────────────────────────────────────────────────────────────────

    /** GET /v1/tenants/me — returns the caller's own tenant. */
    getTenant: () =>
      client
        .get<ApiResponse<TenantSnapshot>>("/v1/tenants/me")
        .then((r) => r.data.data),

    /** PUT /v1/tenants/:id — partial profile update (name, owner_email). */
    updateTenant: (id: string, payload: UpdateTenantPayload) =>
      client
        .put<ApiResponse<TenantSnapshot>>(`/v1/tenants/${id}`, payload)
        .then((r) => r.data.data),

    /** PUT /v1/tenants/:id/tier — set subscription tier (admin only). */
    upgradeTier: (id: string, tier: TenantTier) =>
      client
        .put<ApiResponse<{ subscription_tier: string }>>(`/v1/tenants/${id}/tier`, { tier })
        .then((r) => r.data.data),
  };
}
