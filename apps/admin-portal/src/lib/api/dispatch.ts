/**
 * Dispatch API client — wraps all dispatch console endpoints.
 *
 * All paths route through the API gateway:
 *   /v1/drivers   → driver-ops service  (full DriverDto with GPS + status)
 *   /v1/queue     → dispatch service    (dispatch queue)
 *   /v1/routes    → dispatch service    (route management)
 *
 * Uses the shared Axios client which auto-attaches the freshest JWT from the
 * __los_at cookie via the request interceptor.
 */
import { createApiClient } from "./client";

// ── Driver types (driver-ops DriverDto) ────────────────────────────────────────

export interface DriverDto {
  id: string;
  user_id: string;
  first_name: string;
  last_name: string;
  phone: string;
  status: "offline" | "available" | "en_route" | "delivering" | "returning" | "on_break";
  is_online: boolean;
  driver_type: string;
  per_delivery_rate_cents: number;
  cod_commission_rate_bps: number;
  zone?: string | null;
  vehicle_type?: string | null;
  lat?: number | null;
  lng?: number | null;
  last_location_at?: string | null;
  active_route_id?: string | null;
  is_active: boolean;
  created_at: string;
  updated_at: string;
}

export interface DriverLocation {
  driver_id: string;
  lat: number;
  lng: number;
  last_updated_at?: string | null;
  status: string;
  is_online: boolean;
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

// ── Queue / Dispatch types (dispatch service) ──────────────────────────────────

export interface QueueItem {
  id: string;
  shipment_id: string;
  customer_name: string;
  customer_phone: string;
  tracking_number?: string | null;
  dest_address_line1: string;
  dest_city: string;
  dest_province: string;
  dest_postal_code: string;
  dest_lat?: number | null;
  dest_lng?: number | null;
  origin_address_line1: string;
  origin_city: string;
  cod_amount_cents?: number | null;
  service_type: string;
  status: "pending" | "dispatched";
  auto_dispatch_attempts?: number | null;
  last_dispatch_error?: string | null;
  dispatched_at?: string | null;
  queued_at?: string | null;
}

export interface DispatchResult {
  assignment_id: string;
  driver_id: string;
  status: string;
}

// ── Route types (dispatch service) ────────────────────────────────────────────

export interface RouteView {
  route_id: string;
  driver_id: string;
  vehicle_id: string;
  status: string;
  stop_count: number;
  total_distance_km: number;
  estimated_duration_minutes: number;
  created_at: string;
}

// ── Driver operations ─────────────────────────────────────────────────────────

/** List all drivers for the authenticated tenant (with GPS + status). */
export const listDrivers = () =>
  createApiClient()
    .get<{ data: DriverDto[] }>("/v1/drivers")
    .then((r) => r.data.data);

/** Get aggregated driver KPIs for the tenant (online count, task throughput). */
export const getDriverSummary = () =>
  createApiClient()
    .get<{ data: DriverSummary }>("/v1/drivers/summary")
    .then((r) => r.data.data);

/** Get live GPS location for a specific driver. */
export const getDriverLocation = (driverId: string) =>
  createApiClient()
    .get<{ data: DriverLocation }>(`/v1/drivers/${driverId}/location`)
    .then((r) => r.data.data);

/** Force-set a driver's status (available / offline / on_break). Requires FLEET_MANAGE. */
export const setDriverStatus = (driverId: string, status: "available" | "offline" | "on_break") =>
  createApiClient()
    .put<{ data: DriverDto }>(`/v1/drivers/${driverId}/status`, { status })
    .then((r) => r.data.data);

/** Send an operational instruction to a driver via FCM push + WebSocket. */
export const sendDriverInstruction = (
  driverId: string,
  instructionType: string,
  message: string,
) =>
  createApiClient()
    .post<{ data: { driver_id: string; instruction_type: string; delivered: boolean } }>(
      `/v1/drivers/${driverId}/instructions`,
      { instruction_type: instructionType, message },
    )
    .then((r) => r.data.data);

/** Cancel all pending/in-progress tasks for a driver. Requires FLEET_MANAGE. */
export const cancelDriverTasks = (driverId: string) =>
  createApiClient()
    .post<{ data: { tasks_cancelled: number } }>(`/v1/drivers/${driverId}/cancel-tasks`)
    .then((r) => r.data.data);

// ── Dispatch queue operations ─────────────────────────────────────────────────

/** List the dispatch queue. Filter: "pending" | "dispatched" | "all". */
export const listQueue = (filter: "pending" | "dispatched" | "all" = "pending") =>
  createApiClient()
    .get<{ data: QueueItem[]; count: number }>(`/v1/queue?status=${filter}`)
    .then((r) => r.data);

/** Quick-dispatch a shipment to a driver. Pass preferredDriverId to override auto-select. */
export const quickDispatch = (shipmentId: string, preferredDriverId?: string) =>
  createApiClient()
    .post<{ data: DispatchResult }>(`/v1/queue/${shipmentId}/dispatch`, {
      ...(preferredDriverId ? { preferred_driver_id: preferredDriverId } : {}),
    })
    .then((r) => r.data.data);

/**
 * Broadcast a pending shipment to the gig pool — fans the offer out to the
 * nearest part-time drivers simultaneously; first atomic claim wins. Waves
 * widen automatically (3→6→10 km); unclaimed offers expire back onto this
 * console with an attempt-counter flag.
 */
export const broadcastToGig = (shipmentId: string) =>
  createApiClient()
    .post<{ data: { offer_id: string; wave: number; expires_at: string } }>(
      `/v1/queue/${shipmentId}/broadcast`,
    )
    .then((r) => r.data.data);

/** Cancel a dispatched shipment — returns it to 'pending' for reassignment. */
export const cancelDispatch = (shipmentId: string) =>
  createApiClient()
    .post<{ data: { shipment_id: string; status: string; message: string } }>(
      `/v1/queue/${shipmentId}/cancel-dispatch`,
    )
    .then((r) => r.data.data);

// ── Route management ──────────────────────────────────────────────────────────

/** List all routes for the tenant. */
export const listRoutes = () =>
  createApiClient()
    .get<{ data: RouteView[] }>("/v1/routes")
    .then((r) => r.data.data);

/** Cancel a planned or in-progress route by ID. Requires DISPATCH_ASSIGN. */
export const cancelRoute = (routeId: string) =>
  createApiClient()
    .put<{ data: { route_id: string; status: string } }>(`/v1/routes/${routeId}/cancel`)
    .then((r) => r.data.data);

/** Cancel a driver's stale active assignment (re-enters them into auto-dispatch pool). */
export const cancelDriverAssignment = (driverId: string) =>
  createApiClient()
    .post<{ data: { driver_id: string; assignment_cancelled: boolean } }>(
      `/v1/drivers/${driverId}/cancel-assignment`,
    )
    .then((r) => r.data.data);
