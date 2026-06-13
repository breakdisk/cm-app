import { createApiClient, ApiResponse } from "./client";
import type { Branding } from "@/lib/branding";

export type { Branding };

export const brandingApi = {
  /** GET /v1/tenants/me/branding — the partner tenant's brand (or default). */
  async getBranding(): Promise<Branding> {
    const client = createApiClient();
    const res = await client.get<ApiResponse<Branding>>("/v1/tenants/me/branding");
    return res.data.data;
  },
};
