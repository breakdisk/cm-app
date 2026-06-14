import { createApiClient, ApiResponse } from "./client";
import type { Branding } from "@/lib/branding";

export type { Branding };

/** Partial branding update payload — mirrors UpdateBrandingCommand on the
 *  identity service. All fields optional. */
export interface UpdateBrandingPayload {
  display_name?: string;
  app_tagline?: string;
  logo_url?: string;
  logo_dark_url?: string;
  favicon_url?: string;
  primary_color?: string;
  secondary_color?: string;
  accent_color?: string;
  support_email?: string;
  support_phone?: string;
  legal_text?: Record<string, unknown>;
}

export interface Me {
  id: string;
  tenant_id: string;
  email: string;
  first_name: string;
  last_name: string;
  roles: string[];
  is_active: boolean;
  email_verified: boolean;
  phone_number: string | null;
  last_login_at: string | null;
  created_at: string;
  updated_at: string;
}

export interface Tenant {
  id: string;
  name: string;
  slug: string;
  owner_email: string;
  is_active: boolean;
  subscription_tier: string;
  /** ISO 4217 currency code set during onboarding (e.g. "PHP"). Null for legacy tenants. */
  currency: string | null;
  /** ISO 3166-1 alpha-2 region code set during onboarding (e.g. "PH"). Null for legacy tenants. */
  region: string | null;
  created_at: string;
  updated_at: string;
}

export interface UpdateMePayload {
  first_name?: string;
  last_name?: string;
  phone_number?: string;
}

export const identityApi = {
  async getMe(): Promise<Me> {
    const client = createApiClient();
    const res = await client.get<ApiResponse<Me>>("/v1/users/me");
    return res.data.data;
  },

  async updateMe(payload: UpdateMePayload): Promise<Me> {
    const client = createApiClient();
    const res = await client.put<ApiResponse<Me>>("/v1/users/me", payload);
    return res.data.data;
  },

  async getTenant(): Promise<Tenant> {
    const client = createApiClient();
    const res = await client.get<ApiResponse<Tenant>>("/v1/tenants/me");
    return res.data.data;
  },

  async getBranding(): Promise<Branding> {
    const client = createApiClient();
    const res = await client.get<ApiResponse<Branding>>("/v1/tenants/me/branding");
    return res.data.data;
  },

  async updateBranding(payload: UpdateBrandingPayload): Promise<Branding> {
    const client = createApiClient();
    const res = await client.put<ApiResponse<Branding>>("/v1/tenants/me/branding", payload);
    return res.data.data;
  },
};
