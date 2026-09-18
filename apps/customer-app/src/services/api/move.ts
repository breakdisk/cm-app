/**
 * Move pricing — order-intake's rate-card quote and accessorials (PR #161).
 *
 * `POST /v1/shipments/quote` prices a move off the carrier rate card when it is
 * given an origin and a destination, and returns the carriage fee as rows that
 * add up (base, distance, weight) plus the priced accessorials. The app renders
 * those rows as given. It never adds anything up itself — `linesReconcile` only
 * checks that the server's rows agree with the server's total before a total is
 * shown.
 *
 * A backend without #161 prices only AED parcels and refuses this request;
 * callers show the server's message.
 */
import { getOrderClient } from './client';
import type { AddressInput } from './shipments';

export interface AccessorialOffer {
  code: string;
  amount_cents: number;
  basis: 'booking' | 'stair_flight';
  max_units?: number;
}

export interface AccessorialCatalog {
  currency: string;
  items: AccessorialOffer[];
}

export interface PriceRow {
  label: string;
  note: string;
  amount_cents: number;
}

export interface PricedAccessorial {
  code: string;
  units: number;
  amount_cents: number;
}

export interface QuoteBreakdown {
  currency: string;
  carriage_cents: number;
  accessorial_cents: number;
  total_cents: number;
  carriage_rows: PriceRow[];
  items: PricedAccessorial[];
}

export interface MoveQuoteRequest {
  service_type: 'standard';
  weight_grams: number;
  length_cm?: number;
  width_cm?: number;
  height_cm?: number;
  origin: AddressInput;
  destination: AddressInput;
  accessorials: { code: string; units?: number }[];
  /** Priced in by the server when it applies; refused with a reason when not. */
  promo_code?: string;
}

/** One discount, as the server priced it. */
export interface QuoteDiscount {
  kind: 'code' | 'corporate' | 'tier' | 'credit' | string;
  label: string;
  amount_cents: number;
  /** The discount ceiling cut this line short. */
  clipped: boolean;
}

export interface MoveQuote {
  amount_cents: number;
  currency: string;
  quote_token: string;
  expires_at: string;
  pricing_mode?: string;
  breakdown?: QuoteBreakdown | null;
  billable_grams?: number | null;
  billable_basis?: string | null;
  distance_km?: number | null;
  vehicle_label?: string | null;
  /** Before discounts. Absent on servers without promotions. */
  gross_cents?: number;
  discount_cents?: number;
  discounts?: QuoteDiscount[];
  ceiling_binds?: boolean;
  /** The code that took something off. */
  promo_code?: string | null;
  /** Why a requested code was not applied, e.g. "OUTSIDE_WINDOW_WEEKEND". */
  promo_refusal?: string | null;
  promo_message?: string | null;
  /** The code was valid, but the linked company rate took off more. */
  code_lost_to_corporate?: boolean;
}

/** How a discount line reads on the plan: "Code MOVE20", "Acme Corp rate". */
export function discountLabel(d: QuoteDiscount): string {
  switch (d.kind) {
    case 'code': return `Code ${d.label}`;
    case 'corporate': return `${d.label} rate`;
    case 'tier': return `${d.label} member`;
    default: return d.label;
  }
}

/** What the tenant offers. Null when the server has no accessorials endpoint. */
export async function listAccessorials(): Promise<AccessorialCatalog | null> {
  try {
    const response = await getOrderClient().get<AccessorialCatalog>('/v1/accessorials');
    return response.data;
  } catch (err: any) {
    if (err?.status === 404) return null;
    throw err;
  }
}

export async function quoteMove(request: MoveQuoteRequest): Promise<MoveQuote> {
  const response = await getOrderClient().post<MoveQuote>('/v1/shipments/quote', request);
  return response.data;
}

const LABELS: Record<string, string> = {
  helper: 'Helper',
  assembly: 'Assembly',
  haul_away: 'Haul-away',
  carbon_offset: 'Carbon offset',
};

export function accessorialLabel(code: string): string {
  return LABELS[code] ?? code.replace(/_/g, ' ').replace(/^./, (c) => c.toUpperCase());
}

export interface QuoteLine {
  label: string;
  note: string;
  amount_cents: number;
}

/** The quote as rows: the carriage rows, then each accessorial. */
export function quoteLines(q: MoveQuote): QuoteLine[] {
  if (!q.breakdown) {
    return [{ label: 'Carriage', note: '', amount_cents: q.amount_cents }];
  }
  return [
    ...q.breakdown.carriage_rows,
    ...q.breakdown.items.map((i) => ({
      label: accessorialLabel(i.code),
      note: i.units > 1 ? `${i.units} units` : '',
      amount_cents: i.amount_cents,
    })),
  ];
}

/** What the rows add up to, before any discount. */
export function quoteGross(q: MoveQuote): { cents: number; currency: string } {
  return q.breakdown
    ? { cents: q.breakdown.total_cents, currency: q.breakdown.currency }
    : { cents: q.amount_cents, currency: q.currency };
}

export function discountTotal(q: MoveQuote): number {
  return (q.discounts ?? []).reduce((n, d) => n + d.amount_cents, 0);
}

/** What the customer pays: the rows, less the server's discount lines. */
export function quoteTotal(q: MoveQuote): { cents: number; currency: string } {
  const gross = quoteGross(q);
  return { cents: gross.cents - discountTotal(q), currency: gross.currency };
}

/**
 * Every visible row is in the total and the total is every visible row — the
 * invariant the handoff says broke twice in review. Discounts are rows too: the
 * price rows less the discount lines must be exactly what the server will
 * charge. A total that does not reconcile is not shown.
 */
export function linesReconcile(q: MoveQuote): boolean {
  const sum = quoteLines(q).reduce((n, l) => n + l.amount_cents, 0);
  if (sum !== quoteGross(q).cents) return false;
  return quoteTotal(q).cents === q.amount_cents;
}

/** Pickup country from the tenant's billing currency — the fallback when the
 *  customer has not said. */
export const COUNTRY_FOR_CURRENCY: Record<string, string> = {
  PHP: 'PH', AED: 'AE', SAR: 'SA', USD: 'US', GBP: 'GB', AUD: 'AU', CAD: 'CA', SGD: 'SG',
};
