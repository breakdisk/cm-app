/**
 * Lightweight fetch-based API client for the Driver Super App.
 * Uses native fetch (no axios) for React Native compatibility.
 * Offline-first: all mutating calls are queued via DeliveryQueue when offline.
 */

const API_BASE = process.env.EXPO_PUBLIC_API_URL ?? "http://localhost:8000";

interface RequestOptions {
  method?: "GET" | "POST" | "PUT" | "PATCH" | "DELETE";
  body?: unknown;
  token?: string;
}

export async function apiRequest<T>(
  path: string,
  options: RequestOptions = {}
): Promise<T> {
  const { method = "GET", body, token } = options;

  const headers: Record<string, string> = {
    "Content-Type": "application/json",
  };
  if (token) headers["Authorization"] = `Bearer ${token}`;

  const response = await fetch(`${API_BASE}${path}`, {
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
