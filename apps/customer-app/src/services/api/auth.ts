/**
 * Auth service — uses the identity service's OTP endpoints.
 * POST /v1/auth/otp/send  → sends a 6-digit OTP to the phone
 * POST /v1/auth/otp/verify → verifies OTP, auto-registers if needed, returns JWT
 *
 * The customer selects a logistics provider (tenant slug) before signing in —
 * every shipment they create is scoped to that provider's tenant. The merchant
 * managing that tenant sees the customer's bookings in their portal.
 */
import * as SecureStore from 'expo-secure-store';
import { getIdentityClient } from './client';

const PROVIDER_SLUG_KEY = 'provider_slug';
const DEFAULT_PROVIDER_SLUG = process.env.EXPO_PUBLIC_TENANT_SLUG ?? '';

export interface AuthResponse {
  token: string;
  refreshToken: string;
  customerId: string;
  name: string;
  email: string;
}

function normalizeSlug(slug: string): string {
  return slug.trim().toLowerCase();
}

/**
 * Request an OTP for the given phone number against a specific logistics
 * provider's tenant. The slug is what the merchant gave the customer (a code
 * on their invoice, a tracking page, a deep link, etc.).
 */
export async function verifyPhone(phone: string, providerSlug: string): Promise<void> {
  const slug = normalizeSlug(providerSlug);
  if (!slug) throw new Error('Provider code is required.');

  const client = getIdentityClient();
  await client.post('/v1/auth/otp/send', {
    phone_number: phone,
    tenant_slug:  slug,
    role:         'customer',
  });
}

/**
 * Verify the OTP and obtain JWT tokens for the selected provider.
 * The identity service auto-registers the customer on that tenant if needed.
 */
export async function verifyOTP(phone: string, otp: string, providerSlug: string): Promise<AuthResponse> {
  const slug = normalizeSlug(providerSlug);
  if (!slug) throw new Error('Provider code is required.');

  const client = getIdentityClient();

  const response = await client.post<{ data: {
    access_token:  string;
    refresh_token: string;
    driver_id:     string;   // actually the user ID — shared field name with driver app
    tenant_id:     string;
    expires_in:    number;
    token_type:    string;
  } }>('/v1/auth/otp/verify', {
    phone_number: phone,
    otp_code:     otp,
    tenant_slug:  slug,
    role:         'customer',
  });

  const { access_token, refresh_token, driver_id } = response.data.data;

  await SecureStore.setItemAsync('auth_token',      access_token);
  await SecureStore.setItemAsync('refresh_token',   refresh_token);
  await SecureStore.setItemAsync('customer_phone', phone);
  await SecureStore.setItemAsync('customer_id',    driver_id);
  await SecureStore.setItemAsync(PROVIDER_SLUG_KEY, slug);

  const digits = phone.replace(/\D/g, '');

  return {
    token:        access_token,
    refreshToken: refresh_token,
    customerId:   driver_id,
    name:         '',
    email:        `${digits}@customer.logisticos.app`,
  };
}

/**
 * Load the previously-selected provider slug so the sign-in screen can
 * prefill it on relaunch. Returns the `EXPO_PUBLIC_TENANT_SLUG` build-time
 * default when nothing has been persisted yet (useful for dev builds).
 */
export async function getStoredProviderSlug(): Promise<string> {
  try {
    const stored = await SecureStore.getItemAsync(PROVIDER_SLUG_KEY);
    if (stored) return stored;
  } catch {/* fall through */}
  return DEFAULT_PROVIDER_SLUG;
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
    await SecureStore.deleteItemAsync(PROVIDER_SLUG_KEY);
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
