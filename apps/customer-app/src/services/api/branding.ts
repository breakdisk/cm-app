import * as SecureStore from 'expo-secure-store';
import { getIdentityClient } from './client';

/**
 * White-label branding for the customer app. Colour fields are nullable hex
 * strings ("#00E5FF"); null means "use the default neon palette".
 */
export interface BrandingDto {
  display_name: string;
  app_tagline: string | null;
  logo_url: string | null;
  primary_color: string | null;
  secondary_color: string | null;
  accent_color: string | null;
}

const TENANT_SLUG = process.env.EXPO_PUBLIC_TENANT_SLUG || '';

/**
 * Fetch the tenant brand. Uses the authenticated endpoint when a session token
 * is present, otherwise the public endpoint resolved by the build-time tenant
 * slug. Returns `null` on any failure so callers keep the cached/default brand.
 */
export async function fetchBrandingApi(): Promise<BrandingDto | null> {
  try {
    const token = await SecureStore.getItemAsync('auth_token');
    const client = getIdentityClient();
    const path = token
      ? '/v1/tenants/me/branding'
      : `/v1/public/branding?slug=${encodeURIComponent(TENANT_SLUG)}`;
    if (!token && !TENANT_SLUG) return null;
    const res = await client.get(path);
    const body = res.data;
    return (body?.data ?? body) as BrandingDto;
  } catch {
    return null;
  }
}
