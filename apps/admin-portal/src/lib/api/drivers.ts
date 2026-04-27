import { createApiClient, ApiResponse, PaginatedApiResponse } from "./client";

/** Mirrors driver-ops DriverDto exactly. */
export type DriverStatus =
  | "offline"
  | "available"
  | "en_route"
  | "delivering"
  | "returning"
  | "on_break";

export interface Driver {
  id: string;
  user_id: string;
  first_name: string;
  last_name: string;
  phone: string;
  status: DriverStatus;
  is_online: boolean;
  driver_type: string;
  per_delivery_rate_cents: number;
  cod_commission_rate_bps: number;
  zone: string | null;
  vehicle_type: string;
  lat: number | null;
  lng: number | null;
  last_location_at: string | null;
  active_route_id: string | null;
  is_active: boolean;
}

/** Convenience helper — assembles a display name from backend fields. */
export function driverFullName(d: Driver): string {
  return `${d.first_name} ${d.last_name}`.trim();
}

export function createDriversApi() {
  const client = createApiClient();

  return {
    listDrivers: (params?: {
      status?: DriverStatus;
      search?: string;
      page?: number;
      per_page?: number;
    }) =>
      client
        .get<PaginatedApiResponse<Driver>>("/v1/drivers", { params })
        .then((r) => r.data),

    getDriver: (driverId: string) =>
      client
        .get<ApiResponse<Driver>>(`/v1/drivers/${driverId}`)
        .then((r) => r.data),
  };
}
