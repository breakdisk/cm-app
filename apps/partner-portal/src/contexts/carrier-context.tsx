"use client";
/**
 * CarrierContext — single source of truth for the authenticated partner's
 * carrier record within the partner portal dashboard.
 *
 * Fetches once via GET /v1/carriers/me (which resolves the carrier by
 * matching the JWT email against carrier.contact_email — the email-based
 * workaround until ADR-0013 adds a `pid` JWT claim).
 *
 * Consumed by:
 *  • (dashboard)/layout.tsx  — sidebar name, status badge, status gate (D)
 *  • (dashboard)/settings    — already calls carriersApi.me() directly; will
 *                              migrate to context on next refactor pass
 *  • (dashboard)/manifests   — passes carrierId to manifest API filter
 */
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useState,
  type ReactNode,
} from "react";
import { carriersApi, carrierIdOf, type Carrier } from "@/lib/api/carriers";

// ── Context shape ─────────────────────────────────────────────────────────────

export interface CarrierContextValue {
  /** Resolved carrier for the logged-in partner. Null while loading or on error. */
  carrier: Carrier | null;
  /** UUID string extracted from carrier.id. Null when carrier is not yet loaded. */
  carrierId: string | null;
  loading: boolean;
  error: string | null;
  /** Re-fetch carrier from the backend — use after mutations (e.g. API key rotation). */
  reload: () => Promise<void>;
}

// Null-init + guard matches the admin portal convention (driver-roster-context.tsx).
// Calling useCarrier() outside a CarrierProvider throws rather than silently
// returning stale defaults.
const CarrierContext = createContext<CarrierContextValue | null>(null);

// ── Provider ──────────────────────────────────────────────────────────────────

export function CarrierProvider({ children }: { children: ReactNode }) {
  const [carrier, setCarrier] = useState<Carrier | null>(null);
  const [loading, setLoading] = useState(true);
  const [error,   setError]   = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const c = await carriersApi.me();
      setCarrier(c);
    } catch (e) {
      const err = e as { message?: string };
      setError(err?.message ?? "Failed to load carrier profile");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { load(); }, [load]);

  const carrierId = carrier ? carrierIdOf(carrier) : null;

  return (
    <CarrierContext.Provider value={{ carrier, carrierId, loading, error, reload: load }}>
      {children}
    </CarrierContext.Provider>
  );
}

// ── Hook ──────────────────────────────────────────────────────────────────────

export function useCarrier(): CarrierContextValue {
  const ctx = useContext(CarrierContext);
  if (!ctx) throw new Error("useCarrier must be used within a CarrierProvider");
  return ctx;
}
