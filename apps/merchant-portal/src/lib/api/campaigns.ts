import { createApiClient } from "./client";

// ── Types ──────────────────────────────────────────────────────────────────────
// Shapes match services/marketing — Channel/CampaignStatus are snake_case on the
// wire per `#[serde(rename_all = "snake_case")]` in domain/entities/mod.rs.

export type Channel =
  | "whatsapp" | "sms" | "email" | "push"
  | "messenger" | "telegram" | "x" | "viber" | "wechat" | "line" | "slack";

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
  subject?: string | null;   // email subject line / push notification title
  variables: {
    body?: string;
    deep_link?: string;      // push only — app screen to open on tap
    [key: string]: unknown;
  };
}

/** A single campaign recipient with embedded contact details. */
export interface CampaignRecipient {
  customer_id?: string | null;
  phone?: string | null;
  email?: string | null;
  name?: string | null;
  /** Platform-specific user/chat ID for social channels
   * (Facebook PSID, Telegram chat_id, Slack user_id, etc.) */
  platform_id?: string | null;
}

/** CDP-driven recipient filter — resolved server-side at activation time. */
export interface TargetingRule {
  min_clv_score?: number | null;
  last_active_days?: number | null;
  customer_ids: string[];
  /** Explicit recipient list with contact details — used for direct-address sends. */
  recipients?: CampaignRecipient[];
  estimated_reach: number;
}

/** Daily send-volume row returned by GET /v1/campaigns/weekly-stats */
export interface WeeklyStat {
  day:     string;   // ISO date, e.g. "2026-05-13"
  channel: Channel;
  count:   number;
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

    /** Daily send-volume for the last 7 days, by channel. Powers the chart. */
    async weeklyStats(): Promise<WeeklyStat[]> {
      const { data } = await http.get<{ stats: WeeklyStat[] }>("/v1/campaigns/weekly-stats");
      return data.stats ?? [];
    },
  };
}

export type CampaignsApi = ReturnType<typeof createCampaignsApi>;
