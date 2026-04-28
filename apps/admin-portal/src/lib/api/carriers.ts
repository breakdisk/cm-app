import { createApiClient, ApiResponse, PaginatedApiResponse } from "./client";

export type CarrierStatus = "pending_verification" | "active" | "suspended" | "deactivated";

export interface SlaCommitment {
  on_time_target_pct: number;
  max_delivery_days: number;
  penalty_per_breach: number;
}

export interface RateCard {
  service_type: string;
  base_rate_cents: number;
  per_kg_cents: number;
  max_weight_kg: number;
  coverage_zones: string[];
}

export interface Carrier {
  id: string;
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
  performance_grade: "excellent" | "good" | "fair" | "poor";
  onboarded_at: string;
  updated_at: string;
}

export interface OnboardCarrierPayload {
  name: string;
  code: string;
  contact_email: string;
  sla_target: number;
  max_delivery_days: number;
}

export interface UpdateCarrierPayload {
  name?: string;
  contact_email?: string;
  contact_phone?: string;
  api_endpoint?: string;
  sla?: SlaCommitment;
  rate_cards?: RateCard[];
}

export interface ZoneSlaRow {
  zone: string;
  total: number;
  on_time: number;
  failed: number;
  on_time_rate: number;
}

export function createCarriersApi() {
  const client = createApiClient();

  return {
    listCarriers: (params?: { status?: CarrierStatus; page?: number; per_page?: number }) =>
      client
        .get<{ carriers?: Carrier[]; data?: Carrier[] }>("/v1/carriers", { params })
        .then((r) => r.data),

    getCarrier: (carrierId: string) =>
      client
        .get<ApiResponse<Carrier>>(`/v1/carriers/${carrierId}`)
        .then((r) => r.data),

    onboardCarrier: (payload: OnboardCarrierPayload) =>
      client
        .post<ApiResponse<Carrier>>("/v1/carriers", payload)
        .then((r) => r.data),

    updateCarrier: (carrierId: string, payload: UpdateCarrierPayload) =>
      client
        .put<ApiResponse<Carrier>>(`/v1/carriers/${carrierId}`, payload)
        .then((r) => r.data),

    activateCarrier: (carrierId: string) =>
      client
        .post<ApiResponse<Carrier>>(`/v1/carriers/${carrierId}/activate`, {})
        .then((r) => r.data),

    suspendCarrier: (carrierId: string, reason: string) =>
      client
        .post<ApiResponse<Carrier>>(`/v1/carriers/${carrierId}/suspend`, { reason })
        .then((r) => r.data),

    getSlaZoneSummary: (carrierId: string, from: string, to: string) =>
      client
        .get<ApiResponse<ZoneSlaRow[]>>(`/v1/carriers/${carrierId}/sla-summary`, { params: { from, to } })
        .then((r) => r.data),
  };
}
