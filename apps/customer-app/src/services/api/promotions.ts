/**
 * Promotions — the offers feed and promo codes (`/v1/promotions/...`).
 *
 * Every rule lives on the server: which offers this account sees, whether a
 * code can be used today, the one-code-a-month window, the stacking and the
 * ceiling. The app draws what it is told — the month grid included — and
 * explains a refusal in the server's words. It never decides who gets what.
 */
import { getPromotionsClient } from './client';

export interface WindowDay {
  day: number;
  /** 0 = Monday. */
  weekday: number;
  eligible: boolean;
}

export interface PromoWindow {
  /** "2026-09". */
  month: string;
  today: number;
  open_today: boolean;
  from_day: number;
  to_day: number;
  days: WindowDay[];
  used_this_month: boolean;
  message?: string | null;
}

export type OfferDiscount =
  | { kind: 'percent_carriage'; bps: number }
  | { kind: 'flat'; cents: number };

export interface Offer {
  code: string;
  title: string;
  body: string;
  discount: OfferDiscount;
  cap_cents?: number | null;
  windowed: boolean;
  ends_at?: string | null;
  state: 'available' | 'used' | 'not_now';
  reason?: string | null;
}

export interface OffersFeed {
  window: PromoWindow;
  offers: Offer[];
}

export interface CodeCheck {
  ok: boolean;
  code: string;
  title?: string | null;
  refusal?: string | null;
  message?: string | null;
}

/** The feed, or null where promotions is not deployed — the screen says so
 *  rather than showing an empty page that looks like "no offers". */
export async function getOffers(): Promise<OffersFeed | null> {
  try {
    const response = await getPromotionsClient().get<{ data: OffersFeed }>('/v1/promotions/offers');
    return response.data.data;
  } catch (err: any) {
    if (err?.status === 404 || err?.status === 503) return null;
    throw err;
  }
}

export async function validateCode(code: string): Promise<CodeCheck> {
  const clean = code.trim().toUpperCase();
  const response = await getPromotionsClient().post<{ data: CodeCheck }>(
    `/v1/promotions/codes/${encodeURIComponent(clean)}/validate`,
  );
  return response.data.data;
}

/** "20% off carriage", "₱14 off" — with the code's own cap when it has one. */
export function offerHeadline(o: Offer, money: (cents: number) => string): string {
  const what = o.discount.kind === 'percent_carriage'
    ? `${trimZeros(o.discount.bps / 100)}% off carriage`
    : `${money(o.discount.cents)} off`;
  return o.cap_cents != null && o.discount.kind === 'percent_carriage' ? `${what}, up to ${money(o.cap_cents)}` : what;
}

function trimZeros(n: number): string {
  return Number.isInteger(n) ? String(n) : n.toFixed(2).replace(/0+$/, '').replace(/\.$/, '');
}

/**
 * The server's days as calendar weeks, Monday first, with blanks before the
 * 1st and after the last day so every row has seven cells.
 */
export function monthWeeks(days: WindowDay[]): (WindowDay | null)[][] {
  if (days.length === 0) return [];
  const cells: (WindowDay | null)[] = [...Array(days[0].weekday).fill(null), ...days];
  while (cells.length % 7 !== 0) cells.push(null);
  const weeks: (WindowDay | null)[][] = [];
  for (let i = 0; i < cells.length; i += 7) weeks.push(cells.slice(i, i + 7));
  return weeks;
}

/** "September 2026" from "2026-09". */
export function monthTitle(month: string): string {
  const [y, m] = month.split('-').map(Number);
  const names = ['January', 'February', 'March', 'April', 'May', 'June', 'July', 'August', 'September', 'October', 'November', 'December'];
  return names[m - 1] ? `${names[m - 1]} ${y}` : month;
}
