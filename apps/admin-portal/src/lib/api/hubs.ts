import { createApiClient } from "./client";

// ── Types ──────────────────────────────────────────────────────────────────────
// Matches services/hub-ops/src/domain/entities/mod.rs::Hub (id is a
// tuple-struct newtype over Uuid; serde emits either string or {0: "<uuid>"}).

export interface Hub {
  id: string | { 0: string };
  tenant_id: string | { 0: string };
  name: string;
  address: string;
  lat: number;
  lng: number;
  capacity: number;
  current_load: number;
  serving_zones: string[];
  is_active: boolean;
  created_at: string;
  updated_at: string;
}

export interface HubCapacity {
  hub_id: string;
  hub_name: string;
  capacity: number;
  current_load: number;
  capacity_pct: number;
  is_over_capacity: boolean;
}

export interface HubManifest {
  hub_id: string;
  parcels: unknown[];     // Parcel shape TBD; hub-ops returns raw entities
  count: number;
}

export interface ListHubsResponse {
  hubs: Hub[];
  count: number;
}

// ── Helpers ────────────────────────────────────────────────────────────────────

export function hubIdOf(h: Hub): string {
  const raw = h.id as unknown;
  if (typeof raw === "string") return raw;
  if (raw && typeof raw === "object" && "0" in raw) return String((raw as { 0: string })[0]);
  return "";
}

/** Capacity utilization as a 0-100 percentage. */
export function hubUtilization(h: Hub): number {
  if (h.capacity === 0) return 0;
  return (h.current_load / h.capacity) * 100;
}

/** Derived status tier for UI: critical >= 95%, high >= 80%, normal otherwise. */
export type HubStatusTier = "normal" | "high" | "critical";
export function hubStatusTier(h: Hub): HubStatusTier {
  const pct = hubUtilization(h);
  if (pct >= 95) return "critical";
  if (pct >= 80) return "high";
  return "normal";
}

// ── Client ─────────────────────────────────────────────────────────────────────

export function createHubsApi() {
  const http = createApiClient();

  return {
    async list(): Promise<Hub[]> {
      const { data } = await http.get<ListHubsResponse>("/v1/hubs");
      return data.hubs ?? [];
    },

    async get(hubId: string): Promise<Hub> {
      const { data } = await http.get<Hub>(`/v1/hubs/${hubId}`);
      return data;
    },

    async capacity(hubId: string): Promise<HubCapacity> {
      const { data } = await http.get<HubCapacity>(`/v1/hubs/${hubId}/capacity`);
      return data;
    },

    async manifest(hubId: string): Promise<HubManifest> {
      const { data } = await http.get<HubManifest>(`/v1/hubs/${hubId}/manifest`);
      return data;
    },
  };
}
