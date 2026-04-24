import { createApiClient } from "./client";

// ── Types ──────────────────────────────────────────────────────────────────────
// Mirrors services/cdp/src/domain/entities/mod.rs. CustomerId is a tuple-struct
// over Uuid on the backend; accept either a plain string or {0: "<uuid>"}.

export type EventType =
  | "booking_created"
  | "delivery_completed"
  | "delivery_failed"
  | "rating_given"
  | "support_contacted"
  | string; // open-ended for forward compatibility

export interface BehavioralEvent {
  id: string;
  event_type: EventType;
  shipment_id?: string | null;
  metadata: Record<string, unknown>;
  occurred_at: string;
}

export interface AddressUsage {
  address: string;
  use_count: number;
  last_used: string;
}

export interface CustomerProfile {
  id: string | { 0: string };
  tenant_id: string | { 0: string };
  external_customer_id: string;

  name?: string | null;
  email?: string | null;
  phone?: string | null;

  total_shipments: number;
  successful_deliveries: number;
  failed_deliveries: number;
  first_shipment_at?: string | null;
  last_shipment_at?: string | null;

  total_cod_collected_cents: number;

  address_history: AddressUsage[];
  recent_events: BehavioralEvent[];

  clv_score: number;
  engagement_score: number;

  created_at: string;
  updated_at: string;
}

export interface ListProfilesResponse {
  profiles: CustomerProfile[];
  count: number;
}

export interface TopClvResponse {
  profiles: CustomerProfile[];
}

export interface ListProfilesQuery {
  name?: string;
  email?: string;
  phone?: string;
  min_clv?: number;
  limit?: number;
  offset?: number;
}

export interface UpsertProfilePayload {
  name?: string;
  email?: string;
  phone?: string;
}

// ── Helpers ────────────────────────────────────────────────────────────────────

export function profileIdOf(p: CustomerProfile): string {
  const raw = p.id as unknown;
  if (typeof raw === "string") return raw;
  if (raw && typeof raw === "object" && "0" in raw) return String((raw as { 0: string })[0]);
  return "";
}

// Delivery success rate as a percentage (0-100). Safe against 0 shipments.
export function deliverySuccessRate(p: CustomerProfile): number {
  if (p.total_shipments === 0) return 0;
  return (p.successful_deliveries / p.total_shipments) * 100;
}

// ── Client ─────────────────────────────────────────────────────────────────────
// Cookie-JWT flow — axios interceptor attaches Authorization on every request.
// Gateway routes /v1/customers + /v1/profiles → cdp service.

export function createCdpApi() {
  const http = createApiClient();

  return {
    async list(query: ListProfilesQuery = {}): Promise<ListProfilesResponse> {
      const { data } = await http.get<ListProfilesResponse>("/v1/customers", {
        params: {
          name: query.name,
          email: query.email,
          phone: query.phone,
          min_clv: query.min_clv,
          limit: query.limit ?? 50,
          offset: query.offset ?? 0,
        },
      });
      return data;
    },

    async topByClv(limit = 20): Promise<TopClvResponse> {
      const { data } = await http.get<TopClvResponse>("/v1/customers/top-clv", {
        params: { limit },
      });
      return data;
    },

    async get(externalId: string): Promise<CustomerProfile> {
      const { data } = await http.get<CustomerProfile>(`/v1/customers/${externalId}`);
      return data;
    },

    async upsert(externalId: string, payload: UpsertProfilePayload): Promise<CustomerProfile> {
      const { data } = await http.put<CustomerProfile>(`/v1/customers/${externalId}`, payload);
      return data;
    },

    async events(externalId: string): Promise<BehavioralEvent[]> {
      const { data } = await http.get<{ events: BehavioralEvent[] }>(`/v1/customers/${externalId}/events`);
      return data.events ?? [];
    },
  };
}

export type CdpApi = ReturnType<typeof createCdpApi>;
