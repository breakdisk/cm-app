/**
 * The campaign inbox (`/v1/engagement/inbox`): every campaign message this
 * account was sent, on any channel. Always the caller's own — the server
 * pins it to the token, so there is no id to pass.
 */
import { getEngagementClient } from './client';

export interface InboxItem {
  id: string;
  campaign_id: string;
  channel: string;
  title: string;
  body: string;
  deep_link?: string | null;
  sent_at: string;
  read_at?: string | null;
}

export interface Inbox {
  unread: number;
  items: InboxItem[];
}

export async function getInbox(before?: string): Promise<Inbox> {
  const response = await getEngagementClient().get<{ data: Inbox }>('/v1/engagement/inbox', {
    params: before ? { before } : undefined,
  });
  return response.data.data;
}

/** The home badge. Zero rather than an error: a badge is never worth a failure. */
export async function unreadCount(): Promise<number> {
  try {
    return (await getEngagementClient().get<{ data: Inbox }>('/v1/engagement/inbox', { params: { limit: 1 } })).data.data.unread;
  } catch {
    return 0;
  }
}

export async function markRead(id: string): Promise<void> {
  await getEngagementClient().post(`/v1/engagement/inbox/${encodeURIComponent(id)}/read`);
}

export async function markAllRead(): Promise<number> {
  const response = await getEngagementClient().post<{ data: { marked: number } }>('/v1/engagement/inbox/read-all');
  return response.data.data.marked;
}

/** "Today 14:05", "Yesterday", "12 Sep". */
export function whenSent(iso: string, now: Date = new Date()): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return '';
  const startOf = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const days = Math.round((startOf(now) - startOf(d)) / 86_400_000);
  if (days === 0) return `Today ${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`;
  if (days === 1) return 'Yesterday';
  const months = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];
  return `${d.getDate()} ${months[d.getMonth()]}${d.getFullYear() === now.getFullYear() ? '' : ` ${d.getFullYear()}`}`;
}

/** In-app routes a campaign may link to; anything else is shown, not followed. */
const LINKS: Record<string, string> = {
  offers: 'MoveOffers',
  payments: 'MovePayments',
  support: 'Support',
  rules: 'MoveRules',
};

export function linkTarget(deepLink?: string | null): string | null {
  if (!deepLink) return null;
  const key = deepLink.replace(/^[a-z]+:\/\//i, '').replace(/^\/+/, '').split(/[/?#]/)[0].toLowerCase();
  return LINKS[key] ?? null;
}
