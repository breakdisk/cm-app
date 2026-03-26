import { createApiClient, ApiResponse, PaginatedApiResponse } from "./client";

export interface ActiveShipment {
  id: string;
  tracking_number: string;
  status: string;
  driver_id?: string;
  driver_name?: string;
  origin: { city: string; lat: number; lng: number };
  destination: { city: string; lat: number; lng: number };
  eta?: string;
  service_type: string;
  merchant_name: string;
}

export interface DriverLocation {
  driver_id: string;
  driver_name: string;
  lat: number;
  lng: number;
  heading?: number;
  speed_kmh?: number;
  status: "online" | "offline" | "busy";
  active_tasks: number;
  updated_at: string;
}

export interface Assignment {
  id: string;
  shipment_id: string;
  driver_id: string;
  assigned_at: string;
  status: "pending" | "accepted" | "in_progress" | "completed" | "cancelled";
}

export interface RouteOptimizationResult {
  driver_id: string;
  stops_reordered: number;
  estimated_time_saved_min: number;
  new_route: Array<{ shipment_id: string; sequence: number; eta: string }>;
}

export const dispatchApi = {
  /** Get all active shipments for the live dispatch map */
  getActiveShipments: (token: string) =>
    createApiClient(token)
      .get<ApiResponse<ActiveShipment[]>>("/v1/dispatch/active")
      .then((r) => r.data.data),

  /** Get live driver locations for the map */
  getDriverLocations: (token: string) =>
    createApiClient(token)
      .get<ApiResponse<DriverLocation[]>>("/v1/dispatch/driver-locations")
      .then((r) => r.data.data),

  /** Manually assign a driver to a shipment */
  assignDriver: (
    shipmentId: string,
    driverId: string,
    token: string
  ) =>
    createApiClient(token)
      .post<ApiResponse<Assignment>>(`/v1/assignments/${shipmentId}/assign`, {
        driver_id: driverId,
      })
      .then((r) => r.data.data),

  /** Auto-assign the optimal driver to a shipment */
  autoAssign: (shipmentId: string, token: string) =>
    createApiClient(token)
      .post<ApiResponse<Assignment>>(
        `/v1/assignments/${shipmentId}/auto-assign`
      )
      .then((r) => r.data.data),

  /** Re-optimize a driver's route */
  optimizeRoute: (driverId: string, token: string) =>
    createApiClient(token)
      .post<ApiResponse<RouteOptimizationResult>>(
        `/v1/dispatch/optimize-route`,
        { driver_id: driverId }
      )
      .then((r) => r.data.data),

  /** Get pending unassigned shipments */
  getPendingAssignment: (
    params: { zone_id?: string; page?: number; per_page?: number },
    token: string
  ) =>
    createApiClient(token)
      .get<PaginatedApiResponse<ActiveShipment>>(
        "/v1/dispatch/pending-assignment",
        { params }
      )
      .then((r) => r.data),
};
