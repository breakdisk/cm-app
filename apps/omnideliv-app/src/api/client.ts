/**
 * Fetch wrapper. Mirrors apps/customer-app/src/services/api/client.ts — the
 * token lives in SecureStore and is attached per request.
 */
import * as SecureStore from "expo-secure-store";

export class ApiError extends Error {
  constructor(public status: number, message: string) {
    super(message);
    this.name = "ApiError";
  }
}

const BASE = process.env.EXPO_PUBLIC_OMNIDELIV_API ?? "http://localhost:8091";

export async function authHeaders(): Promise<Record<string, string>> {
  const token = await SecureStore.getItemAsync("auth_token");
  return {
    "Content-Type": "application/json",
    ...(token ? { Authorization: `Bearer ${token}` } : {}),
  };
}

export async function apiFetch<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`${BASE}${path}`, {
    ...init,
    headers: { ...(await authHeaders()), ...(init?.headers ?? {}) },
  });

  if (!res.ok) {
    const body = await res.text().catch(() => "");
    throw new ApiError(res.status, body || res.statusText);
  }

  // 204 has no body — parsing it throws.
  if (res.status === 204) return undefined as T;
  return res.json() as Promise<T>;
}

export { BASE as API_BASE };
