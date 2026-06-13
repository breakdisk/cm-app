import { useAppSelector } from '../store/hooks';
import type { BrandingState } from '../store/slices/branding';

/**
 * Resolved white-label brand for the customer app. Components read brand colours
 * from here (falling back to the default neon palette) instead of the static
 * COLORS constants when they need to honour the tenant's branding.
 */
export function useBranding(): BrandingState {
  return useAppSelector((s) => s.branding);
}
