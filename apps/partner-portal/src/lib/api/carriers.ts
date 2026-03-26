import { createApiClient, ApiResponse, PaginatedApiResponse } from "./client";

export interface CarrierProfile {
  id: string;
  name: string;
  code: string;
  status: "pending_verification" | "active" | "suspended" | "deactivated";
  performance_grade: "excellent" | "good" | "fair" | "poor";
  on_time_rate_pct: number;
  total_deliveries: number;
  created_at: string;
}

export interface SlaReport {
  carrier_id: string;
  period_from: string;
  period_to: string;
  total_shipments: number;
  on_time: number;
  late: number;
  failed: number;
  on_time_rate_pct: number;
  sla_breaches: number;
  avg_delivery_hours: number;
}

export interface RateCard {
  id: string;
  carrier_id: string;
  service_type: string;
  base_rate_php: number;
  per_kg_rate_php: number;
  max_weight_kg: number;
  zones: string[];
}

export interface Payout {
  id: string;
  carrier_id: string;
  period_from: string;
  period_to: string;
  total_deliveries: number;
  gross_amount_php: number;
  deductions_php: number;
  net_amount_php: number;
  status: "pending" | "processing" | "paid";
  paid_at?: string;
}

export interface Manifest {
  id: string;
  carrier_id: string;
  created_at: string;
  shipment_count: number;
  status: "open" | "dispatched" | "completed";
  download_url?: string;
}

export const carriersApi = {
  /** Get the authenticated carrier's own profile */
  getProfile: (token: string) =>
    createApiClient(token)
      .get<ApiResponse<CarrierProfile>>("/v1/carriers/me")
      .then((r) => r.data.data),

  /** Get SLA performance report */
  getSlaReport: (
    params: { from: string; to: string },
    token: string
  ) =>
    createApiClient(token)
      .get<ApiResponse<SlaReport>>("/v1/carriers/me/sla", { params })
      .then((r) => r.data.data),

  /** Get rate cards for the carrier */
  getRateCards: (token: string) =>
    createApiClient(token)
      .get<ApiResponse<RateCard[]>>("/v1/carriers/me/rates")
      .then((r) => r.data.data),

  /** Update a rate card */
  updateRateCard: (
    rateCardId: string,
    payload: Partial<Pick<RateCard, "base_rate_php" | "per_kg_rate_php">>,
    token: string
  ) =>
    createApiClient(token)
      .put<ApiResponse<RateCard>>(`/v1/carriers/me/rates/${rateCardId}`, payload)
      .then((r) => r.data.data),

  /** Get payout history */
  getPayouts: (
    params: { page?: number; per_page?: number; status?: string },
    token: string
  ) =>
    createApiClient(token)
      .get<PaginatedApiResponse<Payout>>("/v1/carriers/me/payouts", { params })
      .then((r) => r.data),

  /** Get dispatch manifests */
  getManifests: (
    params: { page?: number; per_page?: number; status?: string },
    token: string
  ) =>
    createApiClient(token)
      .get<PaginatedApiResponse<Manifest>>("/v1/carriers/me/manifests", {
        params,
      })
      .then((r) => r.data),
};
