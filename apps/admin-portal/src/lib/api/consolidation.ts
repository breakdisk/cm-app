import type { AxiosInstance } from 'axios';

// ── Domain types ──────────────────────────────────────────────────────────────

export interface TruckSpec {
  id:                 string;
  tenant_id:          string;
  name:               string;
  transport_mode:     'road' | 'sea' | 'air';
  size_class:         string;
  interior_length_cm: number;
  interior_width_cm:  number;
  interior_height_cm: number;
  max_payload_kg:     number;
  is_active:          boolean;
  created_at:         string;
  updated_at:         string;
}

export interface BoxItem {
  awb:       string;
  weight_g:  number;
  length_cm: number;
  width_cm:  number;
  height_cm: number;
  estimated: boolean;
}

export interface Placement {
  awb:       string;
  x:         number;
  y:         number;
  z:         number;
  length_cm: number;
  width_cm:  number;
  height_cm: number;
  weight_g:  number;
  estimated: boolean;
  rotated:   boolean;
}

export interface UnplacedItem {
  awb:      string;
  weight_g: number;
  reason:   'no_space' | 'weight_limit' | 'zero_dimension';
}

export interface ConsolidationPlan {
  id:               string;
  tenant_id:        string;
  hub_id:           string;
  truck_spec_id:    string;
  container_id:     string | null;
  items:            BoxItem[];
  placements:       Placement[];
  unplaced:         UnplacedItem[];
  total_weight_kg:  number;
  volume_used_cm3:  number;
  volume_total_cm3: number;
  piece_count:      number;
  status:           'draft' | 'confirmed' | 'loaded';
  loaded_awbs:      string[];
  computed_at:      string;
  created_at:       string;
  updated_at:       string;
}

// ── Request bodies ────────────────────────────────────────────────────────────

export interface CreateSpecBody {
  name:               string;
  transport_mode:     'road' | 'sea' | 'air';
  size_class:         string;
  interior_length_cm: number;
  interior_width_cm:  number;
  interior_height_cm: number;
  max_payload_kg:     number;
}

export interface UpdateSpecBody {
  name?:               string;
  interior_length_cm?: number;
  interior_width_cm?:  number;
  interior_height_cm?: number;
  max_payload_kg?:     number;
  is_active?:          boolean;
}

export interface ComputePlanBody {
  hub_id:        string;
  truck_spec_id: string;
  items:         Array<{
    awb:       string;
    weight_g:  number;
    length_cm: number;
    width_cm:  number;
    height_cm: number;
  }>;
}

export interface ConfirmPlanBody {
  destination_hub_id: string;
  transport_mode:     'road' | 'sea' | 'air';
}

export interface ScanPieceResult {
  awb:          string;
  loaded_count: number;
  total_count:  number;
  plan_status:  'confirmed' | 'loaded';
}

// ── API factory ───────────────────────────────────────────────────────────────

export function createConsolidationApi(client: AxiosInstance) {
  return {
    /** List all active truck specs for the current tenant. */
    async listSpecs(): Promise<TruckSpec[]> {
      const res = await client.get<{ specs: TruckSpec[] }>('/v1/consolidation/specs');
      return res.data.specs;
    },

    async createSpec(body: CreateSpecBody): Promise<TruckSpec> {
      const res = await client.post<TruckSpec>('/v1/consolidation/specs', body);
      return res.data;
    },

    async updateSpec(id: string, body: UpdateSpecBody): Promise<TruckSpec> {
      const res = await client.put<TruckSpec>(`/v1/consolidation/specs/${id}`, body);
      return res.data;
    },

    /** Run the 3D bin-packing optimiser and persist the plan. */
    async computePlan(body: ComputePlanBody): Promise<ConsolidationPlan> {
      const res = await client.post<ConsolidationPlan>('/v1/consolidation/plans', body);
      return res.data;
    },

    async getPlan(id: string): Promise<ConsolidationPlan> {
      const res = await client.get<ConsolidationPlan>(`/v1/consolidation/plans/${id}`);
      return res.data;
    },

    /** List recent plans for a hub (last 20, newest first). */
    async listPlans(hubId: string): Promise<ConsolidationPlan[]> {
      const res = await client.get<{ plans: ConsolidationPlan[] }>(
        `/v1/hubs/${hubId}/consolidation/plans`
      );
      return res.data.plans;
    },

    /** Persist drag-and-drop placement overrides. */
    async updatePlacements(
      planId: string,
      placements: Placement[]
    ): Promise<ConsolidationPlan> {
      const res = await client.put<ConsolidationPlan>(
        `/v1/consolidation/plans/${planId}/placements`,
        { placements }
      );
      return res.data;
    },

    /** Confirm a draft plan — creates a container and links it. */
    async confirmPlan(planId: string, body: ConfirmPlanBody): Promise<ConsolidationPlan> {
      const res = await client.post<ConsolidationPlan>(
        `/v1/consolidation/plans/${planId}/confirm`,
        body,
      );
      return res.data;
    },

    /** Scan a piece AWB against a confirmed plan. */
    async scanPiece(planId: string, awb: string): Promise<ScanPieceResult> {
      const res = await client.post<ScanPieceResult>(
        `/v1/consolidation/plans/${planId}/scan`,
        { awb },
      );
      return res.data;
    },
  };
}
