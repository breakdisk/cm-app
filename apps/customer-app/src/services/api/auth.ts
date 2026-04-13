/**
 * Auth service — uses the identity service's OTP endpoints.
 * POST /v1/auth/otp/send  → sends a 6-digit OTP to the phone
 * POST /v1/auth/otp/verify → verifies OTP, auto-registers if needed, returns JWT
 */
import * as SecureStore from 'expo-secure-store';
import { getIdentityClient } from './client';

const TENANT_SLUG = process.env.EXPO_PUBLIC_TENANT_SLUG ?? 'demo';

export interface AuthResponse {
  token: string;
  refreshToken: string;
  customerId: string;
  name: string;
  email: string;
}

/**
 * Request an OTP for the given phone number.
 */
export async function verifyPhone(phone: string): Promise<void> {
  const client = getIdentityClient();
  await client.post('/v1/auth/otp/send', {
    phone_number: phone,
    tenant_slug: TENANT_SLUG,
    role: 'customer',
  });
}

/**
 * Verify the OTP and obtain JWT tokens.
 * The identity service auto-registers the user if they don't exist.
 */
export async function verifyOTP(phone: string, otp: string): Promise<AuthResponse> {
  const client = getIdentityClient();

  const response = await client.post<{ data: {
    access_token: string;
    refresh_token: string;
    driver_id: string;   // actually the user ID — shared field name with driver app
    tenant_id: string;
    expires_in: number;
    token_type: string;
  } }>('/v1/auth/otp/verify', {
    phone_number: phone,
    otp_code: otp,
    tenant_slug: TENANT_SLUG,
    role: 'customer',
  });

  const { access_token, refresh_token, driver_id } = response.data.data;

  await SecureStore.setItemAsync('auth_token', access_token);
  await SecureStore.setItemAsync('refresh_token', refresh_token);
  await SecureStore.setItemAsync('customer_phone', phone);
  await SecureStore.setItemAsync('customer_id', driver_id);

  const digits = phone.replace(/\D/g, '');

  return {
    token: access_token,
    refreshToken: refresh_token,
    customerId: driver_id,
    name: '',
    email: `${digits}@customer.logisticos.app`,
  };
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
