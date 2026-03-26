import { createApiClient, ApiResponse, PaginatedApiResponse } from "./client";

export type CarrierStatus = "active" | "suspended" | "pending_review";

export interface Carrier {
  id: string;
  name: string;
  code: string;
  status: CarrierStatus;
  shipments_today: number;
  success_rate: number;
  avg_delivery_days: number;
  sla_target_days: number;
  sla_compliance_pct: number;
  active_routes: number;
  rating: number; // 1-5
}

export interface CarrierSummary {
  total_carriers: number;
  active: number;
  avg_sla_compliance: number;
  total_shipments_today: number;
}

export function createCarriersApi(token: string) {
  const client = createApiClient(token);

  return {
    listCarriers: (params?: { status?: CarrierStatus; page?: number; per_page?: number }) =>
      client
        .get<PaginatedApiResponse<Carrier>>("/v1/carriers", { params })
        .then((r) => r.data),

    getCarrier: (carrierId: string) =>
      client
        .get<ApiResponse<Carrier>>(`/v1/carriers/${carrierId}`)
        .then((r) => r.data),

    getSummary: () =>
      client
        .get<ApiResponse<CarrierSummary>>("/v1/carriers/summary")
        .then((r) => r.data),

    suspendCarrier: (carrierId: string, reason: string) =>
      client
        .post<ApiResponse<Carrier>>(`/v1/carriers/${carrierId}/suspend`, { reason })
        .then((r) => r.data),
  };
}
