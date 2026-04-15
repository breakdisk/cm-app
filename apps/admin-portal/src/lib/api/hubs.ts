import { createApiClient, ApiResponse, PaginatedApiResponse } from "./client";

export type HubStatus = "operational" | "degraded" | "offline";

export interface Hub {
  id: string;
  name: string;
  code: string;
  city: string;
  status: HubStatus;
  capacity_total: number;
  capacity_used: number;
  capacity_pct: number;
  inbound_today: number;
  outbound_today: number;
  pending_sort: number;
  docks_total: number;
  docks_occupied: number;
}

export interface HubSummary {
  total_hubs: number;
  operational: number;
  avg_capacity_pct: number;
  total_inbound_today: number;
  total_outbound_today: number;
}

export function createHubsApi() {
  const client = createApiClient();

  return {
    listHubs: (params?: { page?: number; per_page?: number }) =>
      client
        .get<PaginatedApiResponse<Hub>>("/v1/hubs", { params })
        .then((r) => r.data),

    getHub: (hubId: string) =>
      client
        .get<ApiResponse<Hub>>(`/v1/hubs/${hubId}`)
        .then((r) => r.data),

    getSummary: () =>
      client
        .get<ApiResponse<HubSummary>>("/v1/hubs/summary")
        .then((r) => r.data),
  };
}
