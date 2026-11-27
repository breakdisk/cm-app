/**
 * Promotions admin — offers, the loyalty ladder, company rates and account
 * credit (`/v1/promotions/admin/...`).
 *
 * Every rule is enforced server-side: code format, the ladder's shape, the
 * plan gate on loyalty, who may grant credit. This client only shapes
 * requests and reads answers; the helpers below exist so the page can show
 * amounts the way the server stores them (minor units, basis points).
 */
import { authFetch } from "@/lib/auth/auth-fetch";

const BASE = process.env.NEXT_PUBLIC_API_BASE ?? "";

async function okJson(r: Response) {
  if (r.status === 204) return {};
  const j = await r.json().catch(() => ({}));
  if (!r.ok) throw new Error(j?.error?.message ?? j?.message ?? `HTTP ${r.status}`);
  return j;
}

function send(path: string, method: string, body?: unknown) {
  return authFetch(`${BASE}${path}`, {
    method,
    headers: body === undefined ? undefined : { "Content-Type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  }).then(okJson);
}

// ── Offers ──────────────────────────────────────────────────────────────────

export type OfferDiscount = { kind: "percent_carriage"; bps: number } | { kind: "flat"; cents: number };

export interface AdminOffer {
  id: string;
  code: string;
  title: string;
  body: string;
  discount: OfferDiscount;
  cap_cents: number | null;
  windowed: boolean;
  once_per_account: boolean;
  starts_at: string | null;
  ends_at: string | null;
  active: boolean;
  budget_tag: string;
}

export interface NewOffer {
  code: string;
  title: string;
  body?: string;
  discount_kind: "percent_carriage" | "flat";
  percent_bps?: number;
  flat_cents?: number;
  cap_cents?: number | null;
  windowed: boolean;
  once_per_account: boolean;
  ends_at?: string | null;
  budget_tag: string;
}

export const listOffers = (): Promise<AdminOffer[]> => send("/v1/promotions/admin/offers", "GET").then((j) => j.data ?? []);
export const createOffer = (o: NewOffer): Promise<AdminOffer> => send("/v1/promotions/admin/offers", "POST", o).then((j) => j.data);
export const setOfferActive = (id: string, active: boolean) =>
  send(`/v1/promotions/admin/offers/${encodeURIComponent(id)}/${active ? "activate" : "deactivate"}`, "POST");

// ── Loyalty ladder ──────────────────────────────────────────────────────────

export interface TierRung {
  name: string;
  min_moves: number;
  perk: string;
  accessorial_bps: number;
  cap_cents: number;
  referral_multiplier: number;
}

export const getLadder = (): Promise<TierRung[]> => send("/v1/promotions/admin/tiers", "GET").then((j) => j.data ?? []);
/** Replaces the whole ladder. An empty list switches tiers off. */
export const saveLadder = (rungs: TierRung[]): Promise<TierRung[]> =>
  send("/v1/promotions/admin/tiers", "PUT", rungs).then((j) => j.data ?? []);

// ── Company rates ───────────────────────────────────────────────────────────

export interface CompanyRate {
  id: string;
  code: string;
  firm_name: string;
  percent_bps: number;
  email_domain: string | null;
  active: boolean;
}

export const listCompanies = (): Promise<CompanyRate[]> =>
  send("/v1/promotions/admin/corporate-accounts", "GET").then((j) => j.data ?? []);
export const createCompany = (c: { code: string; firm_name: string; percent_bps: number; email_domain?: string | null }): Promise<CompanyRate> =>
  send("/v1/promotions/admin/corporate-accounts", "POST", c).then((j) => j.data);
export const setCompanyActive = (id: string, active: boolean) =>
  send(`/v1/promotions/admin/corporate-accounts/${encodeURIComponent(id)}/${active ? "activate" : "deactivate"}`, "POST");

// ── Account credit ──────────────────────────────────────────────────────────

export interface CustomerUser {
  id: string;
  email: string;
  first_name: string;
  last_name: string;
  phone_number?: string | null;
}

export interface CreditEntry {
  kind: string;
  amount_cents: number;
  currency: string;
  note: string;
  created_at: string;
}

export interface AccountCredit {
  balance_cents: number;
  currency: string;
  entries: CreditEntry[];
}

/** One customer, found exactly by what they told support. */
export async function findCustomer(query: string): Promise<CustomerUser> {
  const q = query.trim();
  const param = q.includes("@") ? `email=${encodeURIComponent(q)}` : `phone=${encodeURIComponent(q)}`;
  const j = await send(`/v1/users/lookup?${param}`, "GET");
  return j.data;
}

export const getAccountCredit = (accountId: string): Promise<AccountCredit> =>
  send(`/v1/promotions/admin/credits/${encodeURIComponent(accountId)}`, "GET").then((j) => j.data);

export const grantCredit = (g: { account_id: string; amount_cents: number; currency: string; note: string }) =>
  send("/v1/promotions/admin/credits", "POST", g);

// ── Units ───────────────────────────────────────────────────────────────────

/** "12.5" from 1250 bps. */
export function bpsToPercent(bps: number): string {
  const n = bps / 100;
  return Number.isInteger(n) ? String(n) : n.toFixed(2).replace(/0+$/, "").replace(/\.$/, "");
}

/** 1250 from "12.5". NaN for anything that is not a number between 0 and 100. */
export function percentToBps(input: string): number {
  const n = Number(input.trim());
  if (input.trim() === "" || !Number.isFinite(n) || n < 0 || n > 100) return NaN;
  return Math.round(n * 100);
}

/** 150050 from "1500.50". NaN for anything that is not a non-negative amount. */
export function amountToCents(input: string): number {
  const clean = input.trim().replace(/,/g, "");
  if (!/^\d+(\.\d{1,2})?$/.test(clean)) return NaN;
  return Math.round(Number(clean) * 100);
}

export function centsToAmount(cents: number): string {
  return (cents / 100).toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 });
}

/** "20% off carriage, up to 250.00" / "140.00 off". */
export function offerSummary(o: Pick<AdminOffer, "discount" | "cap_cents">): string {
  const what = o.discount.kind === "percent_carriage"
    ? `${bpsToPercent(o.discount.bps)}% off carriage`
    : `${centsToAmount(o.discount.cents)} off`;
  return o.cap_cents != null && o.discount.kind === "percent_carriage" ? `${what}, up to ${centsToAmount(o.cap_cents)}` : what;
}

/**
 * What would stop a ladder saving, said before the round trip. The server
 * checks the same things; this only saves the admin a failed save.
 */
export function ladderProblem(rungs: TierRung[]): string | null {
  if (rungs.length > 10) return "A ladder has at most 10 tiers.";
  const names = new Set<string>();
  const thresholds = new Set<number>();
  for (const r of rungs) {
    const name = r.name.trim();
    if (!name || name.length > 32) return "Every tier needs a name of 1–32 characters.";
    if (names.has(name.toLowerCase())) return `Two tiers are called ${name}.`;
    names.add(name.toLowerCase());
    if (!Number.isInteger(r.min_moves) || r.min_moves < 0) return `${name}: moves to reach must be a whole number, 0 or more.`;
    if (thresholds.has(r.min_moves)) return `Two tiers start at ${r.min_moves} moves.`;
    thresholds.add(r.min_moves);
    if (Number.isNaN(r.accessorial_bps) || r.accessorial_bps < 0 || r.accessorial_bps > 10_000) return `${name}: the discount is 0–100%.`;
    if (Number.isNaN(r.cap_cents) || r.cap_cents < 0) return `${name}: the cap can't be negative.`;
    if (r.accessorial_bps > 0 && r.cap_cents === 0) return `${name}: a tier that discounts needs a cap per move.`;
    if (r.referral_multiplier < 1 || r.referral_multiplier > 5) return `${r.name}: the referral multiplier is 1–5.`;
  }
  return null;
}
