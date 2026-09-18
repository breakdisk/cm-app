/**
 * Rewards — loyalty tier, account credit, referrals and the corporate rate
 * (`/v1/promotions/...`).
 *
 * As with codes, every rule is the server's: which tier this account is on,
 * what a referral pays, whether a friend's code can be claimed, which firm an
 * email belongs to. The app shows what it is told and explains a refusal in
 * the server's words.
 */
import { getPromotionsClient } from './client';

export interface Tier {
  name: string;
  min_moves: number;
  perk: string;
  /** Off accessorials, in basis points. */
  accessorial_bps: number;
  cap_cents: number;
  referral_multiplier: number;
}

export interface Loyalty {
  /** False when the plan has no loyalty programme, or no ladder is set up. */
  enabled: boolean;
  /** Completed moves in the last 12 months. */
  moves: number;
  tier?: Tier | null;
  next?: Tier | null;
  moves_to_next?: number | null;
  ladder: Tier[];
}

export interface CreditEntry {
  /** 'referral_reward' | 'grant' | 'applied' | 'returned'. */
  kind: string;
  /** Positive adds, negative spends. */
  amount_cents: number;
  currency: string;
  note: string;
  created_at: string;
}

export interface Credit {
  balance_cents: number;
  currency: string;
  entries: CreditEntry[];
}

export interface Invitee {
  joined_at: string;
  rewarded: boolean;
  reward_cents?: number | null;
}

export interface Referrals {
  code: string;
  /** What a referral pays now, tier included. 0: rewards are off. */
  reward_cents: number;
  currency: string;
  invitees: Invitee[];
}

export interface ClaimResult {
  ok: boolean;
  refusal?: string | null;
  message?: string | null;
}

export interface CorporateRate {
  code: string;
  firm_name: string;
  percent_bps: number;
}

export interface Corporate {
  linked?: CorporateRate | null;
  /** A firm matching this account's work email, offered for one-tap linking. */
  domain_match?: string | null;
}

/** Null where promotions is not deployed, so a section can hide itself
 *  rather than show an error for a feature that is simply not there. */
async function optional<T>(path: string): Promise<T | null> {
  try {
    const response = await getPromotionsClient().get<{ data: T }>(path);
    return response.data.data;
  } catch (err: any) {
    if (err?.status === 404 || err?.status === 503) return null;
    throw err;
  }
}

export const getLoyalty = () => optional<Loyalty>('/v1/promotions/loyalty/me');
export const getCredit = () => optional<Credit>('/v1/promotions/credits/me');
export const getReferrals = () => optional<Referrals>('/v1/promotions/referrals/me');
export const getCorporate = () => optional<Corporate>('/v1/promotions/corporate/me');

export async function claimReferral(code: string): Promise<ClaimResult> {
  const response = await getPromotionsClient().post<{ data: ClaimResult }>('/v1/promotions/referrals/claim', {
    code: code.trim().toUpperCase(),
  });
  return response.data.data;
}

/** With a code, links that firm; without one, the firm matching the account's email. */
export async function linkCorporate(code?: string): Promise<Corporate> {
  const clean = code?.trim().toUpperCase();
  const response = await getPromotionsClient().post<{ data: Corporate }>(
    '/v1/promotions/corporate/me',
    clean ? { code: clean } : {},
  );
  return response.data.data;
}

export async function unlinkCorporate(): Promise<void> {
  await getPromotionsClient().delete('/v1/promotions/corporate/me');
}

export function percent(bps: number): string {
  const n = bps / 100;
  return `${Number.isInteger(n) ? n : n.toFixed(2).replace(/0+$/, '').replace(/\.$/, '')}%`;
}

/** 0–1 of the way from this tier's threshold to the next one's. */
export function tierProgress(l: Loyalty): number {
  if (!l.next) return 1;
  const from = l.tier?.min_moves ?? 0;
  const span = l.next.min_moves - from;
  if (span <= 0) return 1;
  return Math.min(1, Math.max(0, (l.moves - from) / span));
}

/** "10% off extras, up to ₱500 a move" — what a tier takes off. */
export function tierPerk(t: Tier, money: (cents: number) => string): string {
  if (t.accessorial_bps <= 0) return t.perk || 'Member';
  const cap = t.cap_cents > 0 ? `, up to ${money(t.cap_cents)} a move` : '';
  return `${percent(t.accessorial_bps)} off extras${cap}`;
}

/** Title for one line of credit history. */
export function creditEntryTitle(e: CreditEntry): string {
  switch (e.kind) {
    case 'referral_reward': return 'Referral reward';
    case 'grant': return e.note || 'Credit added';
    case 'applied': return 'Used on a move';
    case 'returned': return 'Returned — move cancelled';
    default: return e.note || 'Credit';
  }
}

/** The message a Share sheet sends: the code and what it is for. */
export function referralShareMessage(code: string): string {
  return `I move with LogisticOS. Use my code ${code} when you sign up, and I get credit once your first move is done.`;
}
