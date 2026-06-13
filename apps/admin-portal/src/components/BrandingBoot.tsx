"use client";

import { BrandingProvider } from "@/lib/branding";
import { createIdentityApi } from "@/lib/api/identity";

/**
 * Client wrapper that boots white-label branding for the Admin/Ops Portal.
 * Resolves the authenticated tenant's branding once on mount; falls back to the
 * default LogisticOS brand on any failure.
 */
export function BrandingBoot({ children }: { children: React.ReactNode }) {
  return (
    <BrandingProvider resolve={() => createIdentityApi().getBranding().catch(() => null)}>
      {children}
    </BrandingProvider>
  );
}
