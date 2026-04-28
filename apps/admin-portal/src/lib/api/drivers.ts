import { createApiClient, ApiResponse, PaginatedApiResponse } from "./client";

export type DriverStatus = "online" | "idle" | "on_break" | "offline";
export type DriverType = "full_time" | "part_time";

export interface Driver {
  id: string;
  user_id: string;
  first_name: string;
  last_name: string;
  /** Not returned by backend DTO — callers must compute from first_name + last_name */
  name?: string;
  phone: string;
  vehicle_type: string;
  /** Not in current DriverDto — may be absent */
  vehicle_plate?: string;
  status: DriverStatus;
  driver_type: DriverType;
  zone?: string;
  per_delivery_rate_cents: number;
  cod_commission_rate_bps: number;
  is_active: boolean;
  /** Not in current DriverDto — absent from list responses */
  tasks_total?: number;
  tasks_done?: number;
  lat?: number;
  lng?: number;
  last_location?: string;
  last_seen_at?: string;
  /** Not in current DriverDto — absent from list responses */
  performance_grade?: "A" | "B" | "C" | "D";
  cod_collected?: number;
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

export function createDriversApi() {
  const client = createApiClient();

  return {
    listDrivers: (params?: { status?: DriverStatus; search?: string; page?: number; per_page?: number }) =>
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
  };
}
