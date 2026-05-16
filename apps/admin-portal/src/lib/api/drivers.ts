import { createApiClient, ApiResponse, PaginatedApiResponse } from "./client";

export type DriverStatus = "online" | "idle" | "on_break" | "offline";
export type DriverType = "full_time" | "part_time";

export interface Driver {
  id: string;
  user_id: string;
  first_name: string;
  last_name: string;
  name: string;
  phone: string;
  vehicle_type?: string | null;
  vehicle_plate?: string | null;
  status: string;                         // backend emits snake_case: available / en_route / …
  is_online: boolean;
  driver_type: DriverType;
  zone?: string | null;
  per_delivery_rate_cents: number;
  cod_commission_rate_bps: number;
  is_active: boolean;
  tasks_total?: number | null;
  tasks_done?: number | null;
  lat?: number | null;
  lng?: number | null;
  last_location?: string | null;          // human-readable (not in DTO — kept for compat)
  last_location_at?: string | null;       // ISO timestamp from backend DriverDto
  last_seen_at?: string | null;           // alias used in older roster patches
  active_route_id?: string | null;        // present in DriverDto — gates Cancel Tasks
  performance_grade?: "A" | "B" | "C" | "D" | null;
  cod_collected?: number | null;
  created_at?: string;
  updated_at?: string;
}

export interface DriverSummary {
  online: number;
  idle: number;
  on_break: number;
  offline: number;
  total_tasks_assigned: number;
  total_tasks_completed: number;
  total_tasks_failed: number;
  total_cod_collected: number;
}

export interface RegisterDriverPayload {
  user_id: string;
  first_name: string;
  last_name: string;
  phone: string;
  vehicle_id?: string;
}

export interface UpdateDriverPayload {
  first_name?: string;
  last_name?: string;
  phone?: string;
  driver_type?: DriverType;
  per_delivery_rate_cents?: number;
  cod_commission_rate_bps?: number;
  zone?: string;
  vehicle_type?: string;
  is_active?: boolean;
}

/** Shape returned by GET /v1/drivers/:id/tasks/history */
export interface DriverTaskHistory {
  id: string;
  route_id: string;
  shipment_id: string;
  /** "Pickup" | "Delivery" — capitalised by Rust serde default */
  task_type: "Pickup" | "Delivery";
  sequence: number;
  /** "Completed" | "Failed" | "Cancelled" — capitalised */
  status: "Completed" | "Failed" | "Cancelled";
  customer_name: string;
  customer_phone: string;
  tracking_number?: string | null;
  cod_amount_cents?: number | null;
  special_instructions?: string | null;
  failed_reason?: string | null;
  pod_id?: string | null;
  started_at?: string | null;
  completed_at?: string | null;
  address: {
    line1: string;
    line2?: string | null;
    city: string;
    province?: string | null;
    postal_code?: string | null;
    country?: string | null;
  };
}

export function createDriversApi() {
  const client = createApiClient();

  return {
    listDrivers: (params?: { status?: string; search?: string; page?: number; per_page?: number }) =>
      client
        .get<PaginatedApiResponse<Driver>>("/v1/drivers", { params })
        .then((r) => r.data),

    getDriver: (driverId: string) =>
      client
        .get<ApiResponse<Driver>>(`/v1/drivers/${driverId}`)
        .then((r) => r.data),

    getSummary: () =>
      client
        .get<ApiResponse<DriverSummary>>("/v1/drivers/summary")
        .then((r) => r.data),

    registerDriver: (payload: RegisterDriverPayload) =>
      client
        .post<ApiResponse<{ driver_id: string }>>("/v1/drivers", payload)
        .then((r) => r.data),

    updateDriver: (driverId: string, payload: UpdateDriverPayload) =>
      client
        .patch<ApiResponse<Driver>>(`/v1/drivers/${driverId}`, payload)
        .then((r) => r.data),

    setDriverStatus: (driverId: string, status: "available" | "offline" | "on_break") =>
      client
        .put<ApiResponse<Driver>>(`/v1/drivers/${driverId}/status`, { status })
        .then((r) => r.data),

    cancelTasks: (driverId: string) =>
      client
        .post<ApiResponse<{ tasks_cancelled: number }>>(`/v1/drivers/${driverId}/cancel-tasks`, {})
        .then((r) => r.data),

    /**
     * Backend requires both `instruction_type` and `message`.
     * instructionType defaults to "custom" for free-form ops messages.
     */
    sendInstruction: (driverId: string, message: string, instructionType = "custom") =>
      client
        .post<ApiResponse<void>>(`/v1/drivers/${driverId}/instructions`, {
          instruction_type: instructionType,
          message,
        })
        .then((r) => r.data),

    getTaskHistory: (driverId: string, params?: { page?: number; per_page?: number }) =>
      client
        .get<{ data: DriverTaskHistory[]; count: number }>(
          `/v1/drivers/${driverId}/tasks/history`,
          { params },
        )
        .then((r) => r.data),
  };
}
