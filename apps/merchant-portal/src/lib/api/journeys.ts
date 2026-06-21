import { createApiClient } from "./client";

// ── Types ──────────────────────────────────────────────────────────────────────

export type JourneyStatus = "draft" | "active" | "paused" | "archived";
export type JourneyStepType = "send_campaign" | "wait" | "condition";

export interface JourneyStep {
  id:                    string;
  journey_id:            string;
  step_order:            number;
  step_type:             JourneyStepType;
  campaign_id?:           string | null;
  wait_days?:             number | null;
  condition_type?:        string | null;
  condition_campaign_id?: string | null;
  yes_next_order?:        number | null;
  no_next_order?:         number | null;
}

export interface Journey {
  id:          string;
  tenant_id:   string;
  name:        string;
  description: string | null;
  trigger:     Record<string, unknown>;
  status:      JourneyStatus;
  steps:       JourneyStep[];
  created_at:  string;
  updated_at:  string;
}

export interface JourneyEnrollment {
  id:                 string;
  journey_id:         string;
  tenant_id:          string;
  customer_id:        string;
  current_step_order: number | null;
  status:             string;
  next_action_at:     string | null;
  enrolled_at:        string;
}

export interface CreateJourneyStepPayload {
  step_order:             number;
  step_type:              JourneyStepType;
  campaign_id?:           string | null;
  wait_days?:             number | null;
  condition_type?:        string | null;
  condition_campaign_id?: string | null;
  yes_next_order?:        number | null;
  no_next_order?:         number | null;
}

export interface CreateJourneyPayload {
  name:        string;
  description?: string;
  trigger:     Record<string, unknown>;
  steps:       CreateJourneyStepPayload[];
}

export interface ListJourneysResponse  { journeys: Journey[]; count: number }
export interface ListEnrollmentsResponse { enrollments: JourneyEnrollment[] }

// ── Client ─────────────────────────────────────────────────────────────────────

export function createJourneysApi() {
  const http = createApiClient();

  return {
    async list(): Promise<ListJourneysResponse> {
      const { data } = await http.get<ListJourneysResponse>("/v1/journeys");
      return data;
    },

    async get(id: string): Promise<Journey> {
      const { data } = await http.get<Journey>(`/v1/journeys/${id}`);
      return data;
    },

    async create(payload: CreateJourneyPayload): Promise<Journey> {
      const { data } = await http.post<Journey>("/v1/journeys", payload);
      return data;
    },

    async update(id: string, payload: CreateJourneyPayload): Promise<Journey> {
      const { data } = await http.put<Journey>(`/v1/journeys/${id}`, payload);
      return data;
    },

    async delete(id: string): Promise<void> {
      await http.delete(`/v1/journeys/${id}`);
    },

    async activate(id: string): Promise<Journey> {
      const { data } = await http.post<Journey>(`/v1/journeys/${id}/activate`);
      return data;
    },

    async pause(id: string): Promise<Journey> {
      const { data } = await http.post<Journey>(`/v1/journeys/${id}/pause`);
      return data;
    },

    async enroll(id: string, customerIds: string[]): Promise<{ enrolled: number }> {
      const { data } = await http.post<{ enrolled: number }>(`/v1/journeys/${id}/enroll`, {
        customer_ids: customerIds,
      });
      return data;
    },

    async listEnrollments(id: string): Promise<ListEnrollmentsResponse> {
      const { data } = await http.get<ListEnrollmentsResponse>(`/v1/journeys/${id}/enrollments`);
      return data;
    },
  };
}

export type JourneysApi = ReturnType<typeof createJourneysApi>;
