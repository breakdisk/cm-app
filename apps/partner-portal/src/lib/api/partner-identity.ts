/**
 * Partner Portal — Acting-Partner Identity (pre-backend).
 *
 * In production, `partner_id` is pulled from the JWT `pid` claim once the
 * carrier's user logs in (ADR-0013 §Auth). Pre-backend, every portal session
 * runs as an anonymous demo user, so we persist a switchable "acting as"
 * partner in localStorage. That lets a single browser demo receive a
 * merchant booking targeted at any partner without code changes.
 *
 * When the real auth lands, these helpers become a thin passthrough that
 * reads from the JWT claim and the switcher UI collapses to a read-only
 * badge. The four IDs here mirror the merchant-portal & admin-portal seeds
 * so cross-portal propagation via marketplace-bus lines up by partner_id.
 */

export interface KnownPartner {
  id:   string;
  name: string;
  type: "alliance" | "marketplace";
}

export const KNOWN_PARTNERS: readonly KnownPartner[] = [
  { id: "a1b2c3d4-0000-0000-0000-000000000001", name: "FastShip Co.",         type: "alliance"    },
  { id: "a1b2c3d4-0000-0000-0000-000000000002", name: "NorthLink Logistics", type: "alliance"    },
  { id: "a1b2c3d4-0000-0000-0000-000000000003", name: "Manila MoveIt",        type: "marketplace" },
  { id: "a1b2c3d4-0000-0000-0000-000000000004", name: "Cebu Carriers Co-op", type: "marketplace" },
] as const;

export const DEFAULT_PARTNER_ID = KNOWN_PARTNERS[0].id;

const STORAGE_KEY = "cm:partner-portal:acting-as:v1";

export function listKnownPartners(): readonly KnownPartner[] {
  return KNOWN_PARTNERS;
}

export function getCurrentPartnerId(): string {
  if (typeof window === "undefined") return DEFAULT_PARTNER_ID;
  const stored = window.localStorage.getItem(STORAGE_KEY);
  if (stored && KNOWN_PARTNERS.some((p) => p.id === stored)) return stored;
  return DEFAULT_PARTNER_ID;
}

export function getCurrentPartner(): KnownPartner {
  const id = getCurrentPartnerId();
  return KNOWN_PARTNERS.find((p) => p.id === id) ?? KNOWN_PARTNERS[0];
}

export function setCurrentPartnerId(id: string): void {
  if (typeof window === "undefined") return;
  if (!KNOWN_PARTNERS.some((p) => p.id === id)) return;
  window.localStorage.setItem(STORAGE_KEY, id);
}

const IDENTITY_EVENT = "cm:partner-portal:identity-changed";

export function emitIdentityChanged(): void {
  if (typeof window === "undefined") return;
  window.dispatchEvent(new Event(IDENTITY_EVENT));
}

export function subscribeToIdentityChanges(cb: () => void): () => void {
  if (typeof window === "undefined") return () => {};
  const handler = () => cb();
  window.addEventListener(IDENTITY_EVENT, handler);
  return () => window.removeEventListener(IDENTITY_EVENT, handler);
}
