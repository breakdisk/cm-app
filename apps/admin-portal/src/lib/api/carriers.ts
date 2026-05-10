import { createApiClient } from "./client";

export type CarrierStatus = "pending_verification" | "active" | "suspended" | "deactivated";
export type ComplianceStatus =
  | "pending_submission"
  | "under_review"
  | "compliant"
  | "expiring_soon"
  | "expired"
  | "rejected"
  | "suspended";

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
  has_api_key: boolean;
  status: CarrierStatus;
  compliance_status: ComplianceStatus;
  sla: SlaCommitment;
  rate_cards: RateCard[];
  total_shipments: number;
  on_time_count: number;
  failed_count: number;
  performance_grade: "excellent" | "good" | "fair" | "poor";
  onboarded_at: string;
  updated_at: string;
}

export interface SlaHistoryRecord {
  id:             string;
  shipment_id:    string;
  zone:           string;
  service_level:  string;
  promised_by:    string;
  delivered_at:   string | null;
  status:         "in_transit" | "delivered" | "failed";
  on_time:        boolean | null;
  failure_reason: string | null;
  created_at:     string;
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
  compliance_status?: ComplianceStatus;
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
    // Returns { carriers: Carrier[], count: number } envelope from carrier service.
    listCarriers: (params?: { status?: CarrierStatus; page?: number; per_page?: number }) =>
      client
        .get<{ carriers?: Carrier[]; data?: Carrier[] }>("/v1/carriers", { params })
        .then((r) => r.data),

    // Carrier service returns raw Carrier bodies (no { data: } wrapper).
    getCarrier: (carrierId: string) =>
      client.get<Carrier>(`/v1/carriers/${carrierId}`).then((r) => r.data),

    onboardCarrier: (payload: OnboardCarrierPayload) =>
      client.post<Carrier>("/v1/carriers", payload).then((r) => r.data),

    updateCarrier: (carrierId: string, payload: UpdateCarrierPayload) =>
      client.put<Carrier>(`/v1/carriers/${carrierId}`, payload).then((r) => r.data),

    activateCarrier: (carrierId: string) =>
      client.post<Carrier>(`/v1/carriers/${carrierId}/activate`, {}).then((r) => r.data),

    suspendCarrier: (carrierId: string, reason: string) =>
      client.post<Carrier>(`/v1/carriers/${carrierId}/suspend`, { reason }).then((r) => r.data),

    // Returns { zones: ZoneSlaRow[] } envelope.
    getSlaZoneSummary: (carrierId: string, from: string, to: string) =>
      client
        .get<{ zones: ZoneSlaRow[] }>(`/v1/carriers/${carrierId}/sla-summary`, { params: { from, to } })
        .then((r) => r.data),

    // Generate (or rotate) the carrier's integration API key.
    generateApiKey: (carrierId: string) =>
      client.post<{ api_key: string; has_api_key: boolean }>(`/v1/carriers/${carrierId}/api-key`, {}).then((r) => r.data),

    // Paginated per-shipment SLA history.
    getSlaHistory: (carrierId: string, limit = 20, offset = 0) =>
      client
        .get<{ records: SlaHistoryRecord[]; count: number }>(`/v1/carriers/${carrierId}/sla-history`, { params: { limit, offset } })
        .then((r) => r.data),
  };
}
