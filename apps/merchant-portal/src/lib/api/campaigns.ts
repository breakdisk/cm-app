import { createApiClient, ApiResponse, PaginatedApiResponse } from "./client";

export interface Campaign {
  id: string;
  tenant_id: string;
  name: string;
  description: string;
  status: CampaignStatus;
  channel: CampaignChannel;
  template_id: string;
  audience: CampaignAudience;
  schedule?: CampaignSchedule;
  stats?: CampaignStats;
  ab_test?: AbTestConfig;
  created_at: string;
  started_at?: string;
  completed_at?: string;
}

export type CampaignStatus =
  | "draft"
  | "scheduled"
  | "running"
  | "paused"
  | "completed"
  | "cancelled";

export type CampaignChannel = "whatsapp" | "sms" | "email" | "push";

export interface CampaignAudience {
  type: "segment" | "all_customers" | "manual_list";
  segment_id?: string;
  customer_ids?: string[];
  estimated_reach?: number;
}

export interface CampaignSchedule {
  send_at?: string;
  timezone: string;
  optimal_send_time?: boolean;
}

export interface CampaignStats {
  total_sent: number;
  delivered: number;
  opened: number;
  clicked: number;
  converted: number;
  failed: number;
  delivery_rate_pct: number;
  open_rate_pct: number;
  conversion_rate_pct: number;
}

export interface AbTestConfig {
  variant_a_template_id: string;
  variant_b_template_id: string;
  split_pct: number;
  winner_metric: "open_rate" | "click_rate" | "conversion_rate";
  winner_declared?: "a" | "b";
}

export interface MessageTemplate {
  id: string;
  tenant_id: string;
  name: string;
  channel: CampaignChannel;
  body: string;
  variables: string[];
  preview_url?: string;
  whatsapp_template_name?: string;
  created_at: string;
}

export interface CreateCampaignPayload {
  name: string;
  description?: string;
  channel: CampaignChannel;
  template_id: string;
  audience: CampaignAudience;
  schedule?: CampaignSchedule;
  ab_test?: Pick<
    AbTestConfig,
    "variant_a_template_id" | "variant_b_template_id" | "split_pct" | "winner_metric"
  >;
}

export const campaignsApi = {
  // ── Campaigns ─────────────────────────────────────────────────

  /** List campaigns with optional status filter */
  list: (
    params: {
      status?: CampaignStatus;
      channel?: CampaignChannel;
      page?: number;
      per_page?: number;
    },
    token: string
  ) =>
    createApiClient(token)
      .get<PaginatedApiResponse<Campaign>>("/v1/campaigns", { params })
      .then((r) => r.data),

  /** Get a single campaign with full stats */
  get: (campaignId: string, token: string) =>
    createApiClient(token)
      .get<ApiResponse<Campaign>>(`/v1/campaigns/${campaignId}`)
      .then((r) => r.data.data),

  /** Create a new campaign (starts in draft) */
  create: (payload: CreateCampaignPayload, token: string) =>
    createApiClient(token)
      .post<ApiResponse<Campaign>>("/v1/campaigns", payload)
      .then((r) => r.data.data),

  /** Update a draft campaign */
  update: (
    campaignId: string,
    payload: Partial<CreateCampaignPayload>,
    token: string
  ) =>
    createApiClient(token)
      .put<ApiResponse<Campaign>>(`/v1/campaigns/${campaignId}`, payload)
      .then((r) => r.data.data),

  /** Launch a draft or scheduled campaign immediately */
  launch: (campaignId: string, token: string) =>
    createApiClient(token)
      .post<ApiResponse<Campaign>>(`/v1/campaigns/${campaignId}/launch`)
      .then((r) => r.data.data),

  /** Pause a running campaign */
  pause: (campaignId: string, token: string) =>
    createApiClient(token)
      .post<ApiResponse<Campaign>>(`/v1/campaigns/${campaignId}/pause`)
      .then((r) => r.data.data),

  /** Cancel a campaign (draft, scheduled, or running) */
  cancel: (campaignId: string, token: string) =>
    createApiClient(token)
      .post<void>(`/v1/campaigns/${campaignId}/cancel`)
      .then((r) => r.data),

  /** Get real-time campaign stats */
  getStats: (campaignId: string, token: string) =>
    createApiClient(token)
      .get<ApiResponse<CampaignStats>>(`/v1/campaigns/${campaignId}/stats`)
      .then((r) => r.data.data),

  // ── Templates ─────────────────────────────────────────────────

  /** List message templates */
  listTemplates: (
    params: { channel?: CampaignChannel; page?: number; per_page?: number },
    token: string
  ) =>
    createApiClient(token)
      .get<PaginatedApiResponse<MessageTemplate>>("/v1/campaigns/templates", {
        params,
      })
      .then((r) => r.data),

  /** Get a single template */
  getTemplate: (templateId: string, token: string) =>
    createApiClient(token)
      .get<ApiResponse<MessageTemplate>>(
        `/v1/campaigns/templates/${templateId}`
      )
      .then((r) => r.data.data),

  /** Create a new message template */
  createTemplate: (
    payload: Pick<
      MessageTemplate,
      "name" | "channel" | "body" | "whatsapp_template_name"
    >,
    token: string
  ) =>
    createApiClient(token)
      .post<ApiResponse<MessageTemplate>>("/v1/campaigns/templates", payload)
      .then((r) => r.data.data),

  /** Update an existing template */
  updateTemplate: (
    templateId: string,
    payload: Partial<Pick<MessageTemplate, "name" | "body">>,
    token: string
  ) =>
    createApiClient(token)
      .put<ApiResponse<MessageTemplate>>(
        `/v1/campaigns/templates/${templateId}`,
        payload
      )
      .then((r) => r.data.data),
};
