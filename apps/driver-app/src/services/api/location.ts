import { apiRequest } from "./client";

export interface LocationUpdate {
  lat: number;
  lng: number;
  heading?: number;
  speed_kmh?: number;
  accuracy_m?: number;
  timestamp: string;
}

export const locationApi = {
  /** Post a GPS location update (called every ~30s by background task) */
  update: (update: LocationUpdate, token: string) =>
    apiRequest<void>("/v1/location", {
      method: "POST",
      body: update,
      token,
    }),

  /** Go online (start accepting tasks) */
  goOnline: (token: string) =>
    apiRequest<{ status: "online" }>("/v1/drivers/me/online", {
      method: "PUT",
      token,
    }),

  /** Go offline (stop accepting new tasks) */
  goOffline: (token: string) =>
    apiRequest<{ status: "offline" }>("/v1/drivers/me/offline", {
      method: "PUT",
      token,
    }),
};
