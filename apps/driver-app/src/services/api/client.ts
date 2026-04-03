/**
 * Lightweight fetch-based API client for the Driver Super App.
 * Uses native fetch (no axios) for React Native compatibility.
 * Offline-first: all mutating calls are queued via DeliveryQueue when offline.
 */

export const IDENTITY_URL    = process.env.EXPO_PUBLIC_API_URL        ?? "http://localhost:8001";
export const DRIVER_OPS_URL  = process.env.EXPO_PUBLIC_DRIVER_OPS_URL  ?? "http://localhost:8006";
export const POD_URL         = process.env.EXPO_PUBLIC_POD_URL          ?? "http://localhost:8011";

interface RequestOptions {
  method?: "GET" | "POST" | "PUT" | "PATCH" | "DELETE";
  body?: unknown;
  token?: string;
  baseUrl?: string;
}

export async function apiRequest<T>(
  path: string,
  options: RequestOptions = {}
): Promise<T> {
  const { method = "GET", body, token, baseUrl = DRIVER_OPS_URL } = options;

  const headers: Record<string, string> = {
    "Content-Type": "application/json",
  };
  if (token) headers["Authorization"] = `Bearer ${token}`;

  const response = await fetch(`${baseUrl}${path}`, {
    method,
    headers,
    body: body ? JSON.stringify(body) : undefined,
  });

  if (!response.ok) {
    let errorData: { error?: { message?: string; code?: string } } = {};
    try {
      errorData = await response.json();
    } catch {
      // ignore parse error
    }
    throw {
      message: errorData?.error?.message ?? `HTTP ${response.status}`,
      code: errorData?.error?.code ?? "API_ERROR",
      status: response.status,
    };
  }

  if (response.status === 204) return undefined as unknown as T;
  return response.json() as Promise<T>;
}
