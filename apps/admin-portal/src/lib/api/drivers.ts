import { createApiClient, ApiResponse, PaginatedApiResponse } from "./client";

export type DriverStatus = "online" | "idle" | "on_break" | "offline";

export interface Driver {
  id: string;
  name: string;
  phone: string;
  vehicle_type: string;
  vehicle_plate: string;
  status: DriverStatus;
  tasks_total: number;
  tasks_done: number;
  lat?: number;
  lng?: number;
  last_location?: string;
  last_seen_at?: string;
  performance_grade: "A" | "B" | "C" | "D";
  cod_collected: number;
}

export interface DriverSummary {
  online: number;
  idle: number;
  on_break: number;
  offline: number;
  total_tasks_assigned: number;
  total_tasks_completed: number;
  total_cod_collected: number;
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
  };
}
