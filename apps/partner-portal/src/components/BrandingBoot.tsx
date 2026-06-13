"use client";

import { BrandingProvider } from "@/lib/branding";
import { brandingApi } from "@/lib/api/branding";

/**
 * Client wrapper that boots white-label branding for the Partner Portal.
 * Resolves the authenticated tenant's branding once on mount; falls back to the
 * default LogisticOS brand on any failure (e.g. logged-out pages).
 */
export function BrandingBoot({ children }: { children: React.ReactNode }) {
  return (
    <BrandingProvider resolve={() => brandingApi.getBranding().catch(() => null)}>
      {children}
    </BrandingProvider>
  );
}
