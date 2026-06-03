import { createApiClient } from "./client";

/**
 * Cross-border hub transfer API (hub-ops service via the gateway).
 * Backs the Ops Portal hub-transfer board: container customs lifecycle,
 * customs queue, routing config, and inventory map.
 */

// ── Types ───────────────────────────────────────────────────────────────────────

/** Lowercase DB status from GET /v1/containers (container board). */
export type ContainerStatus =
  | "planning" | "manifested" | "loading" | "sealed" | "in_transit"
  | "arrived_at_port" | "customs" | "released" | "deconsolidated" | "delivered";

export interface ContainerSummary {
  id:                 string;
  transport_mode:     string;
  origin_hub_id:      string;
  destination_hub_id: string;
  status:             ContainerStatus;
  awb_count:          number | null;
  departed_at:        string | null;
  estimated_arrival:  string | null;
  arrived_at:         string | null;
  created_at:         string;
  updated_at:         string;
}

export type CustomsStatus = "not_required" | "pending" | "hold" | "cleared";

export interface TransferManifest {
  id:                  string;
  tenant_id:           string;
  container_id:        string;
  origin_hub_id:       string;
  destination_hub_id:  string;
  transport_mode:      string;
  carrier_booking_ref: string | null;
  customs_filing_ref:  string | null;
  customs_status:      CustomsStatus;
  duties_total_cents:  number | null;
  cleared_by:          string | null;
  cleared_at:          string | null;
  created_at:          string;
  updated_at:          string;
}

export type RoutingType = "own_driver" | "carrier" | "auto";

export interface RoutingConfig {
  id:                        string;
  tenant_id:                 string;
  hub_id:                    string;
  destination_zone:          string;
  routing_type:              RoutingType;
  carrier_id:                string | null;
  auto_fallback_window_mins: number;
  created_at:                string;
  updated_at:                string;
}

export interface HubLocation {
  id:           string;
  hub_id:       string;
  zone_id:      string;
  aisle:        string | null;
  rack:         string | null;
  shelf:        string | null;
  bin:          string | null;
  location_tag: string;
  capacity_cbm: number | null;
  is_active:    boolean;
  created_at:   string;
}

export interface InventoryUnit { kind: "piece" | "consolidated"; id: string; }

export interface HubInventory {
  id:             string;
  hub_id:         string;
  location_id:    string;
  inventory_unit: InventoryUnit;
  batch_number:   string | null;
  checked_in_at:  string;
  updated_at:     string;
}

export interface ClearCustomsBody {
  duties_total_cents?: number;
  customs_filing_ref?: string;
  tenant_code?:        string;
}

export interface CreateContainerBody {
  transport_mode:     string;
  origin_hub_id:      string;
  destination_hub_id: string;
  carrier_ref?:       string;
}

// ── Client ────────────────────────────────────────────────────────────────────

export function createHubTransferApi() {
  const http = createApiClient();

  return {
    async listContainers(params?: { hubId?: string; status?: ContainerStatus }): Promise<ContainerSummary[]> {
      const { data } = await http.get<{ containers: ContainerSummary[] }>("/v1/containers", {
        params: { hub_id: params?.hubId, status: params?.status },
      });
      return data.containers ?? [];
    },

    async getManifest(containerId: string): Promise<TransferManifest> {
      const { data } = await http.get<TransferManifest>(`/v1/containers/${containerId}/manifest`);
      return data;
    },

    async customsQueue(hubId: string): Promise<TransferManifest[]> {
      const { data } = await http.get<{ containers: TransferManifest[] }>(`/v1/hubs/${hubId}/customs-queue`);
      return data.containers ?? [];
    },

    // ── Container customs lifecycle transitions ──
    createContainer: (body: CreateContainerBody) =>
      http.post<ContainerSummary>("/v1/containers", body),

    arriveAtPort:    (id: string) => http.post(`/v1/containers/${id}/arrive-at-port`, { details: [] }),
    enterCustoms:    (id: string) => http.post(`/v1/containers/${id}/enter-customs`, { details: [] }),
    releaseDomestic: (id: string) => http.post(`/v1/containers/${id}/release-domestic`),
    clearCustoms:    (id: string, body: ClearCustomsBody = {}) =>
      http.post(`/v1/containers/${id}/clear-customs`, body),
    deconsolidate:   (id: string, destinationZone: string) =>
      http.post(`/v1/containers/${id}/deconsolidate`, { destination_zone: destinationZone }),

    // ── Routing config ──
    async listRouting(hubId: string): Promise<RoutingConfig[]> {
      const { data } = await http.get<RoutingConfig[]>("/v1/hub-transfer/routing-config", { params: { hub_id: hubId } });
      return data ?? [];
    },
    upsertRouting: (body: {
      hub_id: string; destination_zone: string; routing_type: RoutingType;
      carrier_id?: string; auto_fallback_window_mins?: number;
    }) => http.post("/v1/hub-transfer/routing-config", body),

    // ── Locations + inventory ──
    async listLocations(hubId: string, zoneId?: string): Promise<HubLocation[]> {
      const { data } = await http.get<HubLocation[]>("/v1/hub-transfer/locations", {
        params: { hub_id: hubId, zone_id: zoneId },
      });
      return data ?? [];
    },
    async listInventory(locationId: string): Promise<HubInventory[]> {
      const { data } = await http.get<HubInventory[]>("/v1/hub-transfer/inventory", { params: { location_id: locationId } });
      return data ?? [];
    },
  };
}

// ── UI helpers ──────────────────────────────────────────────────────────────────

export const TRANSPORT_MODES = [
  { value: "road",      label: "Road" },
  { value: "sea_fcl",   label: "Sea — FCL" },
  { value: "sea_lcl",   label: "Sea — LCL" },
  { value: "air_uld",   label: "Air — ULD" },
  { value: "air_loose", label: "Air — Loose" },
] as const;

export const CONTAINER_BOARD_COLUMNS: { status: ContainerStatus; label: string }[] = [
  { status: "manifested",      label: "Manifested" },
  { status: "in_transit",      label: "In Transit" },
  { status: "arrived_at_port", label: "At Port" },
  { status: "customs",         label: "Customs" },
  { status: "released",        label: "Released" },
  { status: "deconsolidated",  label: "Deconsolidated" },
];

export function customsTone(s: CustomsStatus): "green" | "amber" | "red" | "muted" {
  switch (s) {
    case "cleared":  return "green";
    case "pending":  return "amber";
    case "hold":     return "red";
    default:         return "muted";
  }
}
