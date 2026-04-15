import { createApiClient, ApiResponse, PaginatedApiResponse } from "./client";

export type VehicleStatus = "active" | "idle" | "maintenance" | "offline";

export interface Vehicle {
  id: string;
  plate: string;
  type: "Motorcycle" | "Van" | "Truck";
  driver_id?: string;
  driver_name?: string;
  status: VehicleStatus;
  fuel_pct: number;
  km_today: number;
  location?: string;
  lat?: number;
  lng?: number;
  next_service_km: number;
  alerts: string[];
}

export interface FleetSummary {
  active: number;
  idle: number;
  maintenance: number;
  offline: number;
  avg_fuel_pct: number;
  total_km_today: number;
}

export function createFleetApi() {
  const client = createApiClient();

  return {
    listVehicles: (params?: { status?: VehicleStatus; page?: number; per_page?: number }) =>
      client
        .get<PaginatedApiResponse<Vehicle>>("/v1/fleet/vehicles", { params })
        .then((r) => r.data),

    getVehicle: (vehicleId: string) =>
      client
        .get<ApiResponse<Vehicle>>(`/v1/fleet/vehicles/${vehicleId}`)
        .then((r) => r.data),

    getSummary: () =>
      client
        .get<ApiResponse<FleetSummary>>("/v1/fleet/summary")
        .then((r) => r.data),
  };
}
