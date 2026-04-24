/**
 * Notifications API
 *
 * Talks to the engagement service.
 *   GET /v1/notifications?customer_id=&status=&page=&limit=
 *   GET /v1/notifications/:id
 * The customer_id filter is used here so each user sees only their own history.
 */
import { createApiClient } from './client';

let cachedEngagementClient: ReturnType<typeof createApiClient> | null = null;

function getEngagementClient() {
  if (!cachedEngagementClient) {
    cachedEngagementClient = createApiClient(
      process.env.EXPO_PUBLIC_ENGAGEMENT_URL ||
      process.env.EXPO_PUBLIC_API_URL ||
      'http://localhost:8003'
    );
  }
  return cachedEngagementClient;
}

// Matches services/engagement/src/domain/entities/notification.rs exactly.
// Channel/status/priority enums come through as PascalCase strings.
export type NotificationChannel = 'WhatsApp' | 'Sms' | 'Email' | 'Push';
export type NotificationStatus = 'Queued' | 'Sending' | 'Sent' | 'Delivered' | 'Failed' | 'Bounced';
export type NotificationPriority = 'Low' | 'Normal' | 'High';

export interface Notification {
  id: string;
  tenant_id: string;
  customer_id: string;
  channel: NotificationChannel;
  recipient: string;
  template_id: string;
  rendered_body: string;
  subject: string | null;
  status: NotificationStatus;
  priority: NotificationPriority;
  provider_message_id: string | null;
  error_message: string | null;
  queued_at: string;
  sent_at: string | null;
  delivered_at: string | null;
  retry_count: number;
}

export interface NotificationListResponse {
  notifications: Notification[];
  page: number;
  limit: number;
  count: number;
}

export const notificationsApi = {
  async list(opts: { customerId?: string; status?: string; page?: number; limit?: number } = {}): Promise<NotificationListResponse> {
    const params: Record<string, string | number> = {
      page: opts.page ?? 1,
      limit: opts.limit ?? 20,
    };
    if (opts.customerId) params.customer_id = opts.customerId;
    if (opts.status)     params.status      = opts.status;

    const { data } = await getEngagementClient().get<NotificationListResponse>('/v1/notifications', { params });
    return data;
  },

  async get(id: string): Promise<Notification> {
    const { data } = await getEngagementClient().get<Notification>(`/v1/notifications/${id}`);
    return data;
  },
};
