import type { Branding } from "@/lib/branding";
import { DEFAULT_BRANDING } from "@/lib/branding";

const API_BASE = process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:8000";

/**
 * Resolve white-label branding for the public customer portal. Tries the
 * current hostname first (custom-domain / subdomain routing), then falls back
 * to an explicit `slug`. Returns `null` on failure so the caller keeps the
 * default brand. Unauthenticated — hits `GET /v1/public/branding`.
 */
export async function resolvePublicBranding(slug?: string): Promise<Branding | null> {
  const host = typeof window !== "undefined" ? window.location.hostname : "";
  if (!host && !slug) return null;
  const params = new URLSearchParams();
  if (host) params.set("host", host);
  if (slug) params.set("slug", slug);

  try {
    const res = await fetch(`${API_BASE}/v1/public/branding?${params.toString()}`, {
      headers: { Accept: "application/json" },
    });
    if (!res.ok) return null;
    const json = (await res.json()) as { data?: Branding };
    return json.data ?? null;
  } catch {
    return null;
  }
}

export { DEFAULT_BRANDING };
export type { Branding };
