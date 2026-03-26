import axios, { AxiosInstance } from "axios";

const API_BASE = process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:8000";

export function createApiClient(token?: string): AxiosInstance {
  const client = axios.create({
    baseURL: API_BASE,
    headers: {
      "Content-Type": "application/json",
      ...(token && { Authorization: `Bearer ${token}` }),
    },
    timeout: 15_000,
  });

  client.interceptors.request.use((config) => {
    if (typeof window !== "undefined") {
      const tenantSlug = localStorage.getItem("tenant_slug");
      if (tenantSlug) config.headers["X-Tenant"] = tenantSlug;
    }
    return config;
  });

  client.interceptors.response.use(
    (response) => response,
    (error) => {
      const message =
        error.response?.data?.error?.message ?? error.message ?? "An unexpected error occurred";
      const code = error.response?.data?.error?.code ?? "UNKNOWN_ERROR";
      return Promise.reject({ message, code, status: error.response?.status });
    }
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
