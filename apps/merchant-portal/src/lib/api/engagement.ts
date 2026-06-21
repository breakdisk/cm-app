import { createApiClient } from "./client";

// ── Types ──────────────────────────────────────────────────────────────────────
// Mirrors services/engagement/src/domain/entities — serde defaults to PascalCase
// for enum variants (no rename_all attribute on the Rust side).

export type NotificationStatus =
  | "Queued" | "Sending" | "Sent" | "Delivered" | "Failed" | "Bounced";

export type NotificationPriority = "Low" | "Normal" | "High";

export type NotificationChannel =
  | "WhatsApp" | "Sms" | "Email" | "Push"
  | "Messenger" | "Telegram" | "X" | "Viber" | "WeChat" | "Line" | "Slack";

export interface Notification {
  id: string;
  tenant_id: string;
  customer_id: string;
  channel: NotificationChannel;
  recipient: string;
  template_id: string;
  rendered_body: string;
  subject?: string | null;
  status: NotificationStatus;
  priority: NotificationPriority;
  provider_message_id?: string | null;
  error_message?: string | null;
  queued_at: string;
  sent_at?: string | null;
  delivered_at?: string | null;
  retry_count: number;
}

export interface ListNotificationsResponse {
  notifications: Notification[];
  page: number;
  limit: number;
  count: number;
}

export interface ListNotificationsParams {
  customer_id?: string;
  status?: string;
  page?: number;
  limit?: number;
}

export interface NotificationTemplate {
  id: string;
  tenant_id?: string | null;
  template_id: string;
  channel: NotificationChannel;
  language: string;
  subject?: string | null;
  body: string;
  variables: string[];
  is_active: boolean;
}

export interface ListTemplatesResponse {
  templates: NotificationTemplate[];
  count: number;
}

export interface SendNotificationPayload {
  customer_id: string;
  channel: string;
  template_id: string;
  template_vars: Record<string, string>;
  priority?: "high" | "normal" | "low";
}

export interface SendNotificationResponse {
  id: string;
  status: NotificationStatus;
}

export interface CreateTemplatePayload {
  name: string;
  channel: string;
  subject?: string;
  body_template: string;
  variables: string[];
}

export interface UpdateTemplatePayload {
  name?: string;
  subject?: string;
  body_template?: string;
  variables?: string[];
  is_active?: boolean;
}

// ── Client sends history for a specific customer ───────────────────────────────

export interface CustomerSend {
  id: string;
  channel: NotificationChannel;
  status: NotificationStatus;
  subject?: string | null;
  rendered_body: string;
  queued_at: string;
  delivered_at?: string | null;
}

// ── Client ─────────────────────────────────────────────────────────────────────

export function createEngagementApi() {
  const http = createApiClient();

  return {
    async listNotifications(
      params: ListNotificationsParams = {},
    ): Promise<ListNotificationsResponse> {
      const { data } = await http.get<ListNotificationsResponse>("/v1/notifications", {
        params: {
          customer_id: params.customer_id,
          status: params.status,
          page: params.page ?? 1,
          limit: params.limit ?? 20,
        },
      });
      return data;
    },

    async getNotification(id: string): Promise<Notification> {
      const { data } = await http.get<Notification>(`/v1/notifications/${id}`);
      return data;
    },

    async sendNotification(payload: SendNotificationPayload): Promise<SendNotificationResponse> {
      const { data } = await http.post<SendNotificationResponse>(
        "/v1/notifications/send",
        payload,
      );
      return data;
    },

    async listTemplates(): Promise<ListTemplatesResponse> {
      const { data } = await http.get<ListTemplatesResponse>("/v1/templates");
      return data;
    },

    async getTemplate(id: string): Promise<NotificationTemplate> {
      const { data } = await http.get<NotificationTemplate>(`/v1/templates/${id}`);
      return data;
    },

    async createTemplate(payload: CreateTemplatePayload): Promise<NotificationTemplate> {
      const { data } = await http.post<NotificationTemplate>("/v1/templates", payload);
      return data;
    },

    async updateTemplate(
      id: string,
      payload: UpdateTemplatePayload,
    ): Promise<NotificationTemplate> {
      const { data } = await http.put<NotificationTemplate>(`/v1/templates/${id}`, payload);
      return data;
    },

    async customerSends(customerId: string): Promise<CustomerSend[]> {
      const { data } = await http.get<{ sends: CustomerSend[] }>(
        `/v1/customers/${customerId}/sends`,
      );
      return data.sends ?? [];
    },
  };
}

export type EngagementApi = ReturnType<typeof createEngagementApi>;
