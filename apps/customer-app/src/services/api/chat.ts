/**
 * Job chat — the thread with the driver on one move
 * (`/v1/engagement/jobs/:shipment_id/messages`, engagement through the gateway).
 *
 * Who may read or post is decided server-side, against order-intake ("this is
 * my shipment") and driver-ops ("I am the driver on it"). This client only
 * asks; a 403 here means the job is not yours, not that something broke.
 */
import { getEngagementClient } from './client';

/** How often an open thread asks for anything new. */
export const CHAT_POLL_MS = 4000;

export type ChatRole = 'customer' | 'driver';

export interface JobMessage {
  id: string;
  shipment_id: string;
  sender_id: string;
  sender_role: ChatRole;
  body: string;
  created_at: string;
}

export interface JobThread {
  role: ChatRole;
  /** False once the move is finished: the thread stays readable, and closed. */
  can_send: boolean;
  shipment_status: string | null;
  unread: number;
  messages: JobMessage[];
}

function unwrap<T>(body: any): T {
  return (body?.data ?? body) as T;
}

export async function listMessages(shipmentId: string, since?: string): Promise<JobThread> {
  const res = await getEngagementClient().get(`/v1/engagement/jobs/${shipmentId}/messages`, {
    params: since ? { since } : undefined,
  });
  const thread = unwrap<JobThread>(res.data);
  return { ...thread, messages: thread.messages ?? [] };
}

export async function sendMessage(shipmentId: string, body: string, clientMessageId: string): Promise<JobMessage> {
  const res = await getEngagementClient().post(`/v1/engagement/jobs/${shipmentId}/messages`, {
    body,
    client_message_id: clientMessageId,
  });
  return unwrap<JobMessage>(res.data);
}

export async function markThreadRead(shipmentId: string): Promise<void> {
  await getEngagementClient().post(`/v1/engagement/jobs/${shipmentId}/messages/read`, {});
}

export async function unreadCount(shipmentId: string): Promise<number> {
  const res = await getEngagementClient().get(`/v1/engagement/jobs/${shipmentId}/messages/unread`);
  return unwrap<{ unread: number }>(res.data)?.unread ?? 0;
}

/**
 * Folds a poll's messages into what is already on screen: each id once, oldest
 * first. A message sent from this phone is already there when the poll returns
 * it, and must not appear twice.
 */
export function mergeMessages(current: JobMessage[], incoming: JobMessage[]): JobMessage[] {
  const byId = new Map<string, JobMessage>();
  for (const message of [...current, ...incoming]) byId.set(message.id, message);
  return [...byId.values()].sort((a, b) => {
    const at = Date.parse(a.created_at);
    const bt = Date.parse(b.created_at);
    if (Number.isNaN(at) || Number.isNaN(bt) || at === bt) return a.id < b.id ? -1 : a.id > b.id ? 1 : 0;
    return at - bt;
  });
}

/** The sender's own id for a message, so a retry cannot post it twice. */
export function makeClientMessageId(): string {
  const hex = (n: number) => Math.floor(Math.random() * 16 ** n).toString(16).padStart(n, '0');
  return `${hex(8)}-${hex(4)}-4${hex(3)}-a${hex(3)}-${hex(12)}`;
}

export interface CallAttempt {
  bridged: boolean;
  reason?: 'not_configured' | 'no_number';
  call_sid?: string;
  masked_number?: string | null;
}

/**
 * Asks the platform to ring you, and the driver when you answer. The driver's
 * number never comes back here: if the bridge is off, the app says so rather
 * than offering a dial it cannot make.
 */
export async function startCall(shipmentId: string): Promise<CallAttempt> {
  const res = await getEngagementClient().post(`/v1/engagement/jobs/${shipmentId}/call`, {});
  return unwrap<CallAttempt>(res.data);
}

/** What to tell the customer about a call attempt. */
export function callMessage(attempt: CallAttempt): string {
  if (attempt.bridged) {
    return attempt.masked_number
      ? `Your phone will ring from ${attempt.masked_number}. Answer it and your driver is dialled.`
      : 'Your phone will ring. Answer it and your driver is dialled.';
  }
  if (attempt.reason === 'no_number') {
    return "There's no number for your driver yet. Send a message instead.";
  }
  return 'Calling through the app is not switched on here yet. Send a message instead.';
}
