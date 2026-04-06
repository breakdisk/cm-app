import axios, { AxiosInstance, AxiosError } from 'axios';
import * as SecureStore from 'expo-secure-store';

export interface ApiErrorResponse {
  status: number;
  message: string;
  data?: any;
}

export class ApiError extends Error {
  constructor(public status: number, message: string, public data?: any) {
    super(message);
    this.name = 'ApiError';
  }
}

export function createApiClient(baseURL: string): AxiosInstance {
  const client = axios.create({
    baseURL,
    timeout: 30000,
    headers: {
      'Content-Type': 'application/json',
    },
  });

  // Request interceptor: Add JWT token
  client.interceptors.request.use(
    async config => {
      try {
        const token = await SecureStore.getItemAsync('auth_token');
        if (token) {
          config.headers.Authorization = `Bearer ${token}`;
        }
      } catch (error) {
        console.warn('Failed to read auth token:', error);
      }
      return config;
    },
    error => Promise.reject(error)
  );

  // Response interceptor: Handle errors and retry
  let retryCount = 0;
  const maxRetries = 3;

  client.interceptors.response.use(
    response => {
      retryCount = 0;
      return response;
    },
    async (error: AxiosError) => {
      const config = error.config as any;

      // Retry on network errors (except 4xx/5xx)
      if (!error.response && retryCount < maxRetries) {
        retryCount++;
        const delay = Math.pow(2, retryCount) * 1000; // Exponential backoff
        await new Promise(resolve => setTimeout(resolve, delay));
        return client(config);
      }

      // Handle 401: Refresh token
      if (error.response?.status === 401 && !config._retry) {
        config._retry = true;
        try {
          const refreshToken = await SecureStore.getItemAsync('refresh_token');
          if (refreshToken) {
            const response = await client.post('/v1/auth/refresh', { refreshToken });
            const { token } = response.data;
            await SecureStore.setItemAsync('auth_token', token);
            config.headers.Authorization = `Bearer ${token}`;
            return client(config);
          }
        } catch (refreshError) {
          console.error('Token refresh failed:', refreshError);
          // Clear stored tokens and redirect to login
          await SecureStore.deleteItemAsync('auth_token');
          await SecureStore.deleteItemAsync('refresh_token');
        }
      }

      // Transform error response
      const status = error.response?.status || 0;
      const message = (error.response?.data as any)?.message || error.message;
      throw new ApiError(status, message, error.response?.data);
    }
  );

  return client;
}

// Lazy-loaded singleton instances for each service
let cachedIdentityClient: AxiosInstance | null = null;
let cachedOrderClient: AxiosInstance | null = null;
let cachedTrackingClient: AxiosInstance | null = null;

export function getIdentityClient(): AxiosInstance {
  if (!cachedIdentityClient) {
    cachedIdentityClient = createApiClient(process.env.EXPO_PUBLIC_IDENTITY_URL || 'http://localhost:8001');
  }
  return cachedIdentityClient;
}

export function getOrderClient(): AxiosInstance {
  if (!cachedOrderClient) {
    cachedOrderClient = createApiClient(process.env.EXPO_PUBLIC_ORDER_URL || 'http://localhost:8004');
  }
  return cachedOrderClient;
}

export function getTrackingClient(): AxiosInstance {
  if (!cachedTrackingClient) {
    cachedTrackingClient = createApiClient(process.env.EXPO_PUBLIC_TRACKING_URL || 'http://localhost:8007');
  }
  return cachedTrackingClient;
}

/**
 * Legacy fetch-based API request function for backward compatibility
 * with existing shipments and tracking modules
 */
const API_BASE = process.env.EXPO_PUBLIC_API_URL ?? 'http://localhost:8000';

interface RequestOptions {
  method?: 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE';
  body?: unknown;
  token?: string;
  params?: Record<string, string | number | boolean | undefined>;
}

function buildUrl(path: string, params?: RequestOptions['params']): string {
  if (!params) return `${API_BASE}${path}`;
  const q = Object.entries(params)
    .filter(([, v]) => v !== undefined)
    .map(([k, v]) => `${encodeURIComponent(k)}=${encodeURIComponent(String(v))}`)
    .join('&');
  return q ? `${API_BASE}${path}?${q}` : `${API_BASE}${path}`;
}

export async function apiRequest<T>(
  path: string,
  options: RequestOptions = {}
): Promise<T> {
  const { method = 'GET', body, token, params } = options;

  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
  };
  if (token) headers['Authorization'] = `Bearer ${token}`;

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
      code: errorData?.error?.code ?? 'API_ERROR',
      status: response.status,
    };
  }

  if (response.status === 204) return undefined as unknown as T;
  return response.json() as Promise<T>;
}
