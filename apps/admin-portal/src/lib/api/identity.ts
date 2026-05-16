import { createApiClient, ApiResponse, PaginatedApiResponse } from "./client";

// ── User types ─────────────────────────────────────────────────────────────────

export interface TenantUser {
  id: string;
  email: string;
  first_name: string;
  last_name: string;
  roles: string[];
  email_verified: boolean;
  is_active: boolean;
  created_at: string;
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

    listUsers: (params?: { page?: number; per_page?: number }) =>
      client
        .get<PaginatedApiResponse<TenantUser>>("/v1/users", { params })
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
