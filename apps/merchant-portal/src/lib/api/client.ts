import axios, { AxiosInstance } from "axios";
import { getAccessToken } from "@/lib/auth/auth-fetch";

const API_BASE = process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:8000";

/**
 * Axios client for LogisticOS services.
 *
 * The request interceptor calls `getAccessToken()` on every outbound request
 * so the `Authorization` header always carries the freshest JWT from the
 * `__los_at` cookie (refreshed transparently via `/api/auth/refresh` if
 * expired). The `X-LogisticOS-Client: web` header is stamped in the same
 * place to satisfy the CSRF middleware.
 *
 * The optional `token` parameter is retained only for legacy callers — it is
 * ignored; the interceptor is the single source of truth.
 */
export function createApiClient(_legacyToken?: string): AxiosInstance {
  const client = axios.create({
    baseURL: API_BASE,
    headers: { "Content-Type": "application/json" },
    timeout: 15_000,
  });

  client.interceptors.request.use(async (config) => {
    const token = await getAccessToken();
    if (token) {
      config.headers.set("Authorization", `Bearer ${token}`);
    }
    config.headers.set("X-LogisticOS-Client", "web");
    return config;
  });

  client.interceptors.response.use(
    (response) => response,
    async (error) => {
      // Single retry with a forced refresh when we hit 401.
      if (error.response?.status === 401 && !error.config?._retried) {
        const fresh = await getAccessToken(true);
        if (fresh) {
          error.config._retried = true;
          error.config.headers.Authorization = `Bearer ${fresh}`;
          return client.request(error.config);
        }
      }
      const message =
        error.response?.data?.error?.message ?? error.message ?? "An unexpected error occurred";
      const code = error.response?.data?.error?.code ?? "UNKNOWN_ERROR";
      return Promise.reject({ message, code, status: error.response?.status });
    },
  );

  return client;
}

export interface ApiResponse<T> {
  data: T;
}

export interface PaginatedApiResponse<T> {
  data: T[];
  total: number;
  page: number;
  per_page: number;
  total_pages: number;
}

export interface ApiError {
  message: string;
  code: string;
  status: number;
}
