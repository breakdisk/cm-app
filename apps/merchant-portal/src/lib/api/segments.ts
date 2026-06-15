import { createApiClient } from "./client";

// ── Types ──────────────────────────────────────────────────────────────────────

export interface SegmentFilter {
  min_clv?: number | null;
  max_clv?: number | null;
  min_engagement?: number | null;
  churn_tier?: "high" | "medium" | "low" | "healthy" | null;
  max_churn_score?: number | null;
  inactive_days?: number | null;
  profile_type?: "sender" | "receiver" | "unknown" | null;
  min_shipments?: number | null;
}

export interface Segment {
  id: string;
  tenant_id: string;
  name: string;
  description: string;
  filter: SegmentFilter;
  customer_count: number;
  is_dynamic: boolean;
  last_computed: string | null;
  created_at: string;
  updated_at: string;
}

export interface SegmentMember {
  external_customer_id: string;
  name: string | null;
  email: string | null;
  phone: string | null;
  clv_score: number;
  engagement_score: number;
  last_shipment_at: string | null;
}

export interface SegmentPreview {
  estimated_count: number;
  sample_members: SegmentMember[];
}

export interface CreateSegmentPayload {
  name: string;
  description?: string;
  filter: SegmentFilter;
}

export interface UpdateSegmentPayload {
  name?: string;
  description?: string;
  filter?: SegmentFilter;
}

export interface ListSegmentsResponse {
  segments: Segment[];
  count: number;
}

export interface ListMembersResponse {
  members: SegmentMember[];
  count: number;
}

// ── Client ──────────────────────────────────────────────────────────────────────

export function createSegmentsApi() {
  const http = createApiClient();

  return {
    async list(): Promise<ListSegmentsResponse> {
      const { data } = await http.get<ListSegmentsResponse>("/v1/segments");
      return data;
    },

    async get(id: string): Promise<Segment> {
      const { data } = await http.get<Segment>(`/v1/segments/${id}`);
      return data;
    },

    async create(payload: CreateSegmentPayload): Promise<Segment> {
      const { data } = await http.post<Segment>("/v1/segments", payload);
      return data;
    },

    async update(id: string, payload: UpdateSegmentPayload): Promise<Segment> {
      const { data } = await http.put<Segment>(`/v1/segments/${id}`, payload);
      return data;
    },

    async delete(id: string): Promise<void> {
      await http.delete(`/v1/segments/${id}`);
    },

    /** Live preview for an existing saved segment */
    async preview(id: string): Promise<SegmentPreview> {
      const { data } = await http.get<SegmentPreview>(`/v1/segments/${id}/preview`);
      return data;
    },

    /** Ad-hoc preview for a filter being built (before saving) */
    async previewFilter(filter: SegmentFilter): Promise<SegmentPreview> {
      const { data } = await http.post<SegmentPreview>("/v1/segments/preview", filter);
      return data;
    },

    async members(id: string, limit = 50, offset = 0): Promise<ListMembersResponse> {
      const { data } = await http.get<ListMembersResponse>(`/v1/segments/${id}/members`, {
        params: { limit, offset },
      });
      return data;
    },

    async seedDefaults(): Promise<{ created: Segment[]; count: number }> {
      const { data } = await http.post<{ created: Segment[]; count: number }>("/v1/segments/seed-defaults");
      return data;
    },
  };
}

export type SegmentsApi = ReturnType<typeof createSegmentsApi>;
