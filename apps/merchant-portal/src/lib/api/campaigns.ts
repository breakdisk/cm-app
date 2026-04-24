import { createApiClient } from "./client";

// ── Types ──────────────────────────────────────────────────────────────────────
// Shapes match services/marketing — Channel/CampaignStatus are snake_case on the
// wire per `#[serde(rename_all = "snake_case")]` in domain/entities/mod.rs.

export type Channel = "whatsapp" | "sms" | "email" | "push";

export type CampaignStatus =
  | "draft"      // not yet scheduled / activated
  | "scheduled"  // queued for future send
  | "sending"    // send in progress (no pause on backend; only cancel)
  | "completed"  // all sends dispatched
  | "cancelled"
  | "failed";

/** Per-channel message payload. template_id refers to the engagement-service template registry. */
export interface MessageTemplate {
  template_id: string;
  subject?: string | null; // email only
  variables: Record<string, unknown>;
}

/** CDP-driven recipient filter — resolved server-side at activation time. */
export interface TargetingRule {
  min_clv_score?: number | null;
  last_active_days?: number | null;
  customer_ids: string[];
  estimated_reach: number;
}

export interface Campaign {
  id: string;
  tenant_id: string;
  name: string;
  description?: string | null;

  channel: Channel;
  template: MessageTemplate;
  targeting: TargetingRule;

  status: CampaignStatus;
  scheduled_at?: string | null;
  sent_at?: string | null;
  completed_at?: string | null;

  total_sent: number;
  total_delivered: number;
  total_failed: number;

  created_by: string;
  created_at: string;
  updated_at: string;
}

export interface CreateCampaignPayload {
  name: string;
  description?: string;
  channel: Channel;
  template: MessageTemplate;
  targeting: TargetingRule;
}

export interface ScheduleCampaignPayload {
  scheduled_at: string; // ISO 8601
}

export interface ListCampaignsResponse {
  campaigns: Campaign[];
  count: number;
}

// ── Client ─────────────────────────────────────────────────────────────────────
// Cookie-JWT flow — the axios interceptor in client.ts injects the Authorization
// header on every request, so callers don't pass tokens explicitly.

export function createCampaignsApi() {
  const http = createApiClient();

  return {
    async list(limit = 50, offset = 0): Promise<ListCampaignsResponse> {
      const { data } = await http.get<ListCampaignsResponse>("/v1/campaigns", {
        params: { limit, offset },
      });
      return data;
    },

    async get(id: string): Promise<Campaign> {
      const { data } = await http.get<Campaign>(`/v1/campaigns/${id}`);
      return data;
    },

    async create(payload: CreateCampaignPayload): Promise<Campaign> {
      const { data } = await http.post<Campaign>("/v1/campaigns", payload);
      return data;
    },

    async schedule(id: string, payload: ScheduleCampaignPayload): Promise<Campaign> {
      const { data } = await http.post<Campaign>(`/v1/campaigns/${id}/schedule`, payload);
      return data;
    },

    /** Start send immediately. Publishes CAMPAIGN_TRIGGERED → engagement service. */
    async activate(id: string): Promise<Campaign> {
      const { data } = await http.post<Campaign>(`/v1/campaigns/${id}/activate`);
      return data;
    },

    /** Cancel draft/scheduled. Backend rejects cancel on `sending` status. */
    async cancel(id: string): Promise<Campaign> {
      const { data } = await http.post<Campaign>(`/v1/campaigns/${id}/cancel`);
      return data;
    },
  };
}

export type CampaignsApi = ReturnType<typeof createCampaignsApi>;
