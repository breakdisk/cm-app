import { createApiClient } from "./client";

// ── Types ──────────────────────────────────────────────────────────────────────
// Mirrors services/carrier/src/domain/entities/mod.rs. Backend uses tuple-struct
// CarrierId newtypes over Uuid which serialize as either bare strings or `{0:
// "<uuid>"}` depending on serde config; accept both.

export type CarrierStatus = "pending_verification" | "active" | "suspended" | "deactivated";
export type PerformanceGrade = "excellent" | "good" | "fair" | "poor";

export interface RateCard {
  service_type: string;          // "same_day" | "next_day" | "standard"
  base_rate_cents: number;
  per_kg_cents: number;
  max_weight_kg: number;
  coverage_zones: string[];
}

export interface SlaCommitment {
  on_time_target_pct: number;
  max_delivery_days: number;
  penalty_per_breach: number;    // cents
}

export interface Carrier {
  id: string | { 0: string };
  tenant_id: string | { 0: string };
  name: string;
  code: string;
  contact_email: string;
  contact_phone?: string | null;
  api_endpoint?: string | null;
  status: CarrierStatus;
  sla: SlaCommitment;
  rate_cards: RateCard[];
  total_shipments: number;
  on_time_count: number;
  failed_count: number;
  performance_grade: PerformanceGrade;
  onboarded_at: string;
  updated_at: string;
}

export interface RateQuote {
  carrier_id: string;
  carrier_name: string;
  service_type: string;
  total_cost_cents: number;
  eligible: boolean;
  ineligibility_reason?: string | null;
}

export interface RateShopResponse {
  quotes: RateQuote[];
}

export interface RateShopQuery {
  service_type: string;          // "same_day" | "next_day" | "standard"
  weight_kg: number;
}

/** Zone-level SLA aggregate row — returned by GET /v1/carriers/:id/sla-summary */
export interface ZoneSlaRow {
  zone:         string;
  total:        number;
  on_time:      number;
  failed:       number;
  on_time_rate: number;   // 0–100
}

export interface SlaSummaryResponse {
  zones: ZoneSlaRow[];
}

/** Aggregated manifest row — one per (driver, task_type) for a given date.
 *  Served by driver-ops /v1/tasks/manifest. */
export interface ManifestEntry {
  driver_id: string;
  driver_name: string;
  task_type: "pickup" | "delivery";
  total: number;
  completed: number;
  failed: number;
  in_progress: number;
  pending: number;
}

export interface ManifestResponse {
  data: ManifestEntry[];
  date: string;
  carrier_id: string | null;
}

// ── Client ─────────────────────────────────────────────────────────────────────
// Cookie-JWT flow; axios interceptor stamps Authorization automatically.

/** Partial update payload for PUT /v1/carriers/:id. */
export interface UpdateCarrierBody {
  name?:           string;
  contact_email?:  string;
  contact_phone?:  string;
  api_endpoint?:   string;
  sla?:            SlaCommitment;
  rate_cards?:     RateCard[];
}

export const carriersApi = {
  /**
   * Returns the carrier that matches the authenticated user's email.
   * Use this in the partner portal instead of a hardcoded carrier ID.
   */
  async me(): Promise<Carrier> {
    const { data } = await createApiClient().get<Carrier>("/v1/carriers/me");
    return data;
  },

  /** Fetch a single carrier's full record including embedded rate_cards. */
  async get(carrierId: string): Promise<Carrier> {
    const { data } = await createApiClient().get<Carrier>(`/v1/carriers/${carrierId}`);
    return data;
  },

  /** Apply a partial update to the carrier — name/contact/sla/rate_cards.
   *  Server clamps SLA target to [0, 100] and floors max_delivery_days at 1. */
  async update(carrierId: string, body: UpdateCarrierBody): Promise<Carrier> {
    const { data } = await createApiClient().put<Carrier>(`/v1/carriers/${carrierId}`, body);
    return data;
  },

  /** List carriers for the logged-in tenant (ops view). */
  async list(): Promise<Carrier[]> {
    const { data } = await createApiClient().get<{ carriers: Carrier[] }>("/v1/carriers");
    return data.carriers ?? [];
  },

  /**
   * Rate-shop the tenant's active carriers for a given service + weight.
   * Returns a quote per eligible carrier, sorted by total cost ascending
   * server-side.
   */
  async rateShop(q: RateShopQuery): Promise<RateQuote[]> {
    const { data } = await createApiClient().get<RateShopResponse>("/v1/carriers/rate-shop", {
      params: { service_type: q.service_type, weight_kg: q.weight_kg },
    });
    return data.quotes ?? [];
  },

  /**
   * Zone-level SLA aggregate for a carrier over a time window.
   * Backed by `GET /v1/carriers/:id/sla-summary?from=&to=`.
   */
  async slaSummary(carrierId: string, from: string, to: string): Promise<ZoneSlaRow[]> {
    const { data } = await createApiClient().get<SlaSummaryResponse>(
      `/v1/carriers/${carrierId}/sla-summary`,
      { params: { from, to } },
    );
    return data.zones ?? [];
  },

  /**
   * Daily manifest aggregation from driver-ops. Passing `carrierId` filters
   * to that partner's drivers (via drivers.carrier_id); passing null returns
   * the whole-tenant view (useful for admin-scoped callers).
   */
  async manifest(date: string, carrierId?: string | null): Promise<ManifestResponse> {
    const { data } = await createApiClient().get<ManifestResponse>("/v1/tasks/manifest", {
      params: { date, carrier_id: carrierId ?? undefined },
    });
    return data;
  },
};

// ── Helpers ────────────────────────────────────────────────────────────────────

/** Unwrap a CarrierId that may be `string` or `{0: string}`. */
export function carrierIdOf(c: Carrier): string {
  const raw = c.id as unknown;
  if (typeof raw === "string") return raw;
  if (raw && typeof raw === "object" && "0" in raw) return String((raw as { 0: string })[0]);
  return "";
}

/** Format a cent-denominated number as ₱N,NNN (no decimals). */
export function fmtPhp(cents: number): string {
  return `₱${Math.round(cents / 100).toLocaleString()}`;
}
