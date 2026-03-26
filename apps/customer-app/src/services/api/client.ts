/**
 * Lightweight fetch-based API client for the Customer App.
 * Uses native fetch for React Native compatibility.
 */

const API_BASE = process.env.EXPO_PUBLIC_API_URL ?? "http://localhost:8000";

interface RequestOptions {
  method?: "GET" | "POST" | "PUT" | "PATCH" | "DELETE";
  body?: unknown;
  token?: string;
  params?: Record<string, string | number | boolean | undefined>;
}

function buildUrl(path: string, params?: RequestOptions["params"]): string {
  if (!params) return `${API_BASE}${path}`;
  const q = Object.entries(params)
    .filter(([, v]) => v !== undefined)
    .map(([k, v]) => `${encodeURIComponent(k)}=${encodeURIComponent(String(v))}`)
    .join("&");
  return q ? `${API_BASE}${path}?${q}` : `${API_BASE}${path}`;
}

export async function apiRequest<T>(
  path: string,
  options: RequestOptions = {}
): Promise<T> {
  const { method = "GET", body, token, params } = options;

  const headers: Record<string, string> = {
    "Content-Type": "application/json",
  };
  if (token) headers["Authorization"] = `Bearer ${token}`;

  const response = await fetch(buildUrl(path, params), {
    method,
    headers,
    body: body ? JSON.stringify(body) : undefined,
  });

  if (!response.ok) {
    let errorData: { error?: { message?: string; code?: string } } = {};
    try {
      errorData = await response.json();
    } catch {
      // ignore
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
