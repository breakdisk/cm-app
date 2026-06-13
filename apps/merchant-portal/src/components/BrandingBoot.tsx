"use client";

import { BrandingProvider } from "@/lib/branding";
import { identityApi } from "@/lib/api/identity";

/**
 * Client wrapper that boots white-label branding for the Merchant Portal.
 * Resolves the authenticated tenant's branding once on mount; falls back to the
 * default LogisticOS brand on any failure (e.g. logged-out auth pages).
 */
export function BrandingBoot({ children }: { children: React.ReactNode }) {
  return (
    <BrandingProvider resolve={() => identityApi.getBranding().catch(() => null)}>
      {children}
    </BrandingProvider>
  );
}
