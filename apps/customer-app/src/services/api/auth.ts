/**
 * Auth service — maps the phone-based mobile UX to the identity service's
 * email+password API. Phone number is used to derive a deterministic email
 * (phone@customer.logisticos.app) until a proper OTP endpoint is added.
 */
import * as SecureStore from 'expo-secure-store';
import { getIdentityClient } from './client';

const TENANT_SLUG = process.env.EXPO_PUBLIC_TENANT_SLUG ?? 'demo';

// Derive a stable email from a phone number for the identity service
function phoneToEmail(phone: string): string {
  const digits = phone.replace(/\D/g, '');
  return `${digits}@customer.logisticos.app`;
}

// Default password derived from phone — replace with real OTP in production
function phoneToPassword(phone: string): string {
  const digits = phone.replace(/\D/g, '');
  return `Cust${digits}!Lgx`;
}

export interface AuthResponse {
  token: string;
  refreshToken: string;
  customerId: string;
  name: string;
  email: string;
}

/**
 * Register a new customer account using their phone number.
 * Called on first-ever login when the account doesn't exist yet.
 */
export async function registerWithPhone(
  phone: string,
  firstName: string,
  lastName: string,
): Promise<AuthResponse> {
  const client = getIdentityClient();
  const email    = phoneToEmail(phone);
  const password = phoneToPassword(phone);

  await client.post('/v1/auth/register', {
    tenant_slug: TENANT_SLUG,
    email,
    password,
    first_name: firstName,
    last_name: lastName,
  });

  // Register doesn't return a token — login immediately after
  return loginWithPhone(phone);
}

/**
 * Login with phone number (mapped to email+password).
 */
export async function loginWithPhone(phone: string): Promise<AuthResponse> {
  const client = getIdentityClient();
  const email    = phoneToEmail(phone);
  const password = phoneToPassword(phone);

  const response = await client.post<{ data: {
    access_token: string;
    refresh_token: string;
    expires_in: number;
    token_type: string;
  } }>('/v1/auth/login', {
    tenant_slug: TENANT_SLUG,
    email,
    password,
  });

  const { access_token, refresh_token } = response.data.data;

  await SecureStore.setItemAsync('auth_token', access_token);
  await SecureStore.setItemAsync('refresh_token', refresh_token);
  await SecureStore.setItemAsync('customer_phone', phone);

  // Derive a customer ID from the phone for offline use
  const customerId = `cust-${phone.replace(/\D/g, '')}`;
  await SecureStore.setItemAsync('customer_id', customerId);

  return {
    token: access_token,
    refreshToken: refresh_token,
    customerId,
    name: '',
    email,
  };
}

/**
 * Try login; if 401 (not registered), register then login.
 * This is the single entry point for the OTP-verify step.
 */
export async function verifyOTP(phone: string, _otp: string): Promise<AuthResponse> {
  try {
    return await loginWithPhone(phone);
  } catch (error: any) {
    // 401 or 404 = account doesn't exist yet → register first
    const status = error?.status ?? error?.response?.status;
    if (status === 401 || status === 404 || status === 422) {
      return registerWithPhone(phone, 'Customer', phone.replace(/\D/g, '').slice(-4));
    }
    throw error;
  }
}

/** Legacy: send OTP (demo — no-op in backend, always succeeds) */
export async function verifyPhone(_phone: string): Promise<void> {
  // In demo mode this is a no-op — the OTP is always 123456
  // In production: call an SMS gateway here
}

export async function logout(): Promise<void> {
  try {
    const client = getIdentityClient();
    await client.post('/v1/auth/logout').catch(() => {/* best-effort */});
  } finally {
    await SecureStore.deleteItemAsync('auth_token');
    await SecureStore.deleteItemAsync('refresh_token');
    await SecureStore.deleteItemAsync('customer_id');
    await SecureStore.deleteItemAsync('customer_phone');
  }
}

export async function getStoredToken(): Promise<string | null> {
  try {
    return await SecureStore.getItemAsync('auth_token');
  } catch {
    return null;
  }
}

export async function getStoredCustomerId(): Promise<string | null> {
  try {
    return await SecureStore.getItemAsync('customer_id');
  } catch {
    return null;
  }
}
