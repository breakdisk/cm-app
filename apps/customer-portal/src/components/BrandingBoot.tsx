"use client";

import { BrandingProvider } from "@/lib/branding";
import { resolvePublicBranding } from "@/lib/api/branding";

/**
 * Boots white-label branding for the public customer portal. Resolves by
 * hostname (subdomain / custom domain) via the public branding endpoint; falls
 * back to the default LogisticOS brand.
 */
export function BrandingBoot({ children }: { children: React.ReactNode }) {
  return (
    <BrandingProvider resolve={() => resolvePublicBranding()}>
      {children}
    </BrandingProvider>
  );
}
