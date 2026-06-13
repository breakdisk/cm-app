"use client";

/**
 * White-label branding — runtime theming for this portal.
 *
 * Local to the app (mirrors the per-app `lib/design-system` convention). Shape
 * mirrors the identity service `Branding` entity and the payloads from
 * `GET /v1/tenants/me/branding` and `GET /v1/public/branding`.
 */
import React, { createContext, useContext, useEffect, useState } from "react";

export interface Branding {
  tenant_id: string;
  display_name: string;
  app_tagline: string | null;
  logo_url: string | null;
  logo_dark_url: string | null;
  favicon_url: string | null;
  primary_color: string | null;
  secondary_color: string | null;
  accent_color: string | null;
  support_email: string | null;
  support_phone: string | null;
  legal_text: Record<string, unknown>;
  custom_domain: string | null;
  created_at: string;
  updated_at: string;
}

export const DEFAULT_BRANDING: Branding = {
  tenant_id: "",
  display_name: "LogisticOS",
  app_tagline: "AI Agentic Last-Mile Delivery",
  logo_url: null,
  logo_dark_url: null,
  favicon_url: null,
  primary_color: "#00E5FF",
  secondary_color: "#A855F7",
  accent_color: "#00FF88",
  support_email: null,
  support_phone: null,
  legal_text: {},
  custom_domain: null,
  created_at: "",
  updated_at: "",
};

/** Hex (`#RGB`/`#RRGGBB`) to shadcn HSL triplet `"H S% L%"`. `null` if invalid. */
export function hexToHslTriplet(hex: string): string | null {
  const m = hex.trim().match(/^#?([a-f\d]{3}|[a-f\d]{6})$/i);
  if (!m) return null;
  let raw = m[1];
  if (raw.length === 3) raw = raw.split("").map((c) => c + c).join("");
  const r = parseInt(raw.slice(0, 2), 16) / 255;
  const g = parseInt(raw.slice(2, 4), 16) / 255;
  const b = parseInt(raw.slice(4, 6), 16) / 255;
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const lightness = (max + min) / 2;
  let hue = 0;
  let sat = 0;
  if (max !== min) {
    const d = max - min;
    sat = lightness > 0.5 ? d / (2 - max - min) : d / (max + min);
    if (max === r) hue = (g - b) / d + (g < b ? 6 : 0);
    else if (max === g) hue = (b - r) / d + 2;
    else hue = (r - g) / d + 4;
    hue /= 6;
  }
  return `${Math.round(hue * 360)} ${Math.round(sat * 100)}% ${Math.round(lightness * 100)}%`;
}

function applyBrandingToDocument(b: Branding) {
  if (typeof document === "undefined") return;
  const root = document.documentElement;
  const primary = b.primary_color ?? DEFAULT_BRANDING.primary_color!;
  const secondary = b.secondary_color ?? DEFAULT_BRANDING.secondary_color!;
  const accent = b.accent_color ?? DEFAULT_BRANDING.accent_color!;
  root.style.setProperty("--brand-primary", primary);
  root.style.setProperty("--brand-secondary", secondary);
  root.style.setProperty("--brand-accent", accent);
  const pHsl = hexToHslTriplet(primary);
  const sHsl = hexToHslTriplet(secondary);
  const aHsl = hexToHslTriplet(accent);
  if (pHsl) { root.style.setProperty("--primary", pHsl); root.style.setProperty("--ring", pHsl); }
  if (sHsl) root.style.setProperty("--secondary", sHsl);
  if (aHsl) root.style.setProperty("--accent", aHsl);
  if (b.favicon_url) {
    let link = document.querySelector<HTMLLinkElement>("link[rel~='icon']");
    if (!link) { link = document.createElement("link"); link.rel = "icon"; document.head.appendChild(link); }
    link.href = b.favicon_url;
  }
}

interface BrandingContextValue {
  branding: Branding;
  loading: boolean;
}

const BrandingContext = createContext<BrandingContextValue>({
  branding: DEFAULT_BRANDING,
  loading: false,
});

export interface BrandingProviderProps {
  children: React.ReactNode;
  initialBranding?: Branding;
  /** Async resolver run on mount. Return `null` to keep the default brand. */
  resolve?: () => Promise<Branding | null>;
}

export function BrandingProvider({ children, initialBranding, resolve }: BrandingProviderProps) {
  const [branding, setBranding] = useState<Branding>(initialBranding ?? DEFAULT_BRANDING);
  const [loading, setLoading] = useState<boolean>(!!resolve);

  useEffect(() => {
    applyBrandingToDocument(branding);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    let cancelled = false;
    if (!resolve) return;
    (async () => {
      try {
        const next = await resolve();
        if (!cancelled && next) { setBranding(next); applyBrandingToDocument(next); }
      } catch {
        /* keep default brand */
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();
    return () => { cancelled = true; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <BrandingContext.Provider value={{ branding, loading }}>
      {children}
    </BrandingContext.Provider>
  );
}

export function useBranding(): BrandingContextValue {
  return useContext(BrandingContext);
}
