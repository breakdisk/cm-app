"use client";

/**
 * Partner Portal — Marketplace Discovery.
 *
 * Lets a Carrier list idle vehicles for consumer on-demand booking
 * (ADR-0013, Marketplace Discovery addendum). Scope = partner — this page
 * only ever sees this partner's own listings and bookings; RLS enforces that
 * server-side.
 *
 * Zero-loss note: bookings created against these listings still pass through
 * order-intake — this UI never short-circuits the shipment pipeline.
 */

import { useCallback, useEffect, useMemo, useState } from "react";
import { useSearchParams } from "next/navigation";
import { motion, AnimatePresence } from "framer-motion";
import { useRosterEvents } from "@/hooks/useRosterEvents";
import {
  Truck,
  Package,
  DollarSign,
  Activity,
  Plus,
  Search,
  X,
  Pencil,
  Trash2,
  Clock,
  MapPin,
  Gauge,
  Pause,
  Play,
  ExternalLink,
  Check,
  XCircle,
} from "lucide-react";

import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge, type BadgeVariant } from "@/components/ui/neon-badge";
import { cn } from "@/lib/design-system/cn";
import {
  fetchBookings,
  fetchListings,
  createListing,
  updateListing,
  deleteListing,
  acceptBooking,
  rejectBooking,
  subscribeToMarketplaceUpdates,
  formatCentsPhp,
  SIZE_CLASS_CAPACITY_HINT,
  SIZE_CLASS_LABEL,
  type BookingStatus,
  type ListingStatus,
  type MarketplaceBooking,
  type SizeClass,
  type VehicleListing,
} from "@/lib/api/marketplace";

// ── Status styling ────────────────────────────────────────────────────────────

const LISTING_STATUS_VARIANT: Record<ListingStatus, BadgeVariant> = {
  active: "green",
  booked: "cyan",
  paused: "muted",
  expired: "red",
};

const BOOKING_STATUS_VARIANT: Record<BookingStatus, BadgeVariant> = {
  pending:    "amber",
  accepted:   "cyan",
  rejected:   "red",
  in_transit: "cyan",
  delivered:  "green",
  cancelled:  "muted",
  disputed:   "red",
};

// ── Small helpers ─────────────────────────────────────────────────────────────

function fmtWindow(fromISO: string, untilISO: string): string {
  const from  = new Date(fromISO);
  const until = new Date(untilISO);
  const now   = new Date();
  const endedMs  = until.getTime() - now.getTime();
  const startMs  = from.getTime()  - now.getTime();
  if (endedMs < 0) return "Expired";
  if (startMs > 0) {
    const h = Math.round(startMs / 3_600_000);
    return `Starts in ${h}h`;
  }
  const h = Math.round(endedMs / 3_600_000);
  return h < 1 ? "< 1h left" : `${h}h left`;
}

function fmtRelative(iso: string): string {
  const d = new Date(iso);
  const diff = d.getTime() - Date.now();
  const absMin = Math.round(Math.abs(diff) / 60_000);
  if (absMin < 1)  return "just now";
  if (absMin < 60) return diff < 0 ? `${absMin}m ago` : `in ${absMin}m`;
  const h = Math.round(absMin / 60);
  if (h < 24) return diff < 0 ? `${h}h ago` : `in ${h}h`;
  const days = Math.round(h / 24);
  return diff < 0 ? `${days}d ago` : `in ${days}d`;
}

// ── KPI card ──────────────────────────────────────────────────────────────────

function Kpi({
  label,
  value,
  icon: Icon,
  glow,
  hint,
}: {
  label: string;
  value: string;
  icon: React.ElementType;
  glow: "green" | "cyan" | "amber" | "purple";
  hint?: string;
}) {
  return (
    <GlassCard glow={glow} accent size="sm">
      <div className="flex items-start justify-between">
        <div className="min-w-0">
          <p className="text-2xs font-medium uppercase tracking-wider text-white/40">
            {label}
          </p>
          <p
            className="mt-2 font-mono text-2xl font-bold text-white"
            style={{
              textShadow:
                glow === "green"
                  ? "0 0 12px rgba(0,255,136,0.3)"
                  : glow === "cyan"
                  ? "0 0 12px rgba(0,229,255,0.3)"
                  : glow === "amber"
                  ? "0 0 12px rgba(255,171,0,0.3)"
                  : "0 0 12px rgba(168,85,247,0.3)",
            }}
          >
            {value}
          </p>
          {hint && (
            <p className="mt-1 text-xs text-white/40">{hint}</p>
          )}
        </div>
        <div
          className={cn(
            "flex h-9 w-9 items-center justify-center rounded-lg border",
            glow === "green" && "border-green-signal/30 bg-green-surface text-green-signal",
            glow === "cyan"  && "border-cyan-neon/30   bg-cyan-surface  text-cyan-neon",
            glow === "amber" && "border-amber-signal/30 bg-amber-surface text-amber-signal",
            glow === "purple" && "border-purple-plasma/30 bg-purple-surface text-purple-plasma",
          )}
        >
          <Icon className="h-4 w-4" />
        </div>
      </div>
    </GlassCard>
  );
}

// ── Listing drawer (create + edit) ────────────────────────────────────────────

type DrawerMode =
  | { kind: "closed" }
  | { kind: "create" }
  | { kind: "edit"; listing: VehicleListing };

interface DrawerFormState {
  vehicle_plate: string;
  size_class: SizeClass;
  max_weight_kg: number;
  max_volume_m3: number | null;
  base_price_cents: number;
  per_km_cents: number;
  per_kg_cents: number | null;
  service_area_label: string;
  idle_from: string;
  idle_until: string;
  status: ListingStatus;
  carrier_response_window_mins: number;
}

function defaultForm(): DrawerFormState {
  const now = new Date();
  const end = new Date(now.getTime() + 6 * 3_600_000);
  return {
    vehicle_plate: "",
    size_class: "van",
    max_weight_kg: 800,
    max_volume_m3: 5,
    base_price_cents: 90_000,
    per_km_cents: 1_800,
    per_kg_cents: null,
    service_area_label: "Metro Manila",
    idle_from: now.toISOString().slice(0, 16),
    idle_until: end.toISOString().slice(0, 16),
    status: "active",
    carrier_response_window_mins: 15,
  };
}

function fromListing(l: VehicleListing): DrawerFormState {
  return {
    vehicle_plate: l.vehicle_plate,
    size_class:    l.size_class,
    max_weight_kg: l.max_weight_kg,
    max_volume_m3: l.max_volume_m3,
    base_price_cents: l.base_price_cents,
    per_km_cents:     l.per_km_cents,
    per_kg_cents:     l.per_kg_cents,
    service_area_label: l.service_area_label,
    idle_from:  l.idle_from.slice(0, 16),
    idle_until: l.idle_until.slice(0, 16),
    status:     l.status,
    carrier_response_window_mins: l.carrier_response_window_mins,
  };
}

function ListingDrawer({
  mode,
  onClose,
  onSaved,
}: {
  mode: DrawerMode;
  onClose: () => void;
  onSaved: () => void;
}) {
  const isEdit = mode.kind === "edit";
  const [form, setForm] = useState<DrawerFormState>(defaultForm());
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (mode.kind === "edit")   setForm(fromListing(mode.listing));
    if (mode.kind === "create") setForm(defaultForm());
    setError(null);
  }, [mode]);

  async function handleSave() {
    setSaving(true);
    setError(null);
    try {
      if (mode.kind === "edit") {
        await updateListing(mode.listing.id, {
          status:                       form.status,
          base_price_cents:             form.base_price_cents,
          per_km_cents:                 form.per_km_cents,
          per_kg_cents:                 form.per_kg_cents,
          max_weight_kg:                form.max_weight_kg,
          max_volume_m3:                form.max_volume_m3,
          service_area_label:           form.service_area_label,
          idle_until:                   new Date(form.idle_until).toISOString(),
          carrier_response_window_mins: form.carrier_response_window_mins,
        });
      } else {
        await createListing({
          vehicle_plate:    form.vehicle_plate,
          size_class:       form.size_class,
          max_weight_kg:    form.max_weight_kg,
          max_volume_m3:    form.max_volume_m3,
          base_price_cents: form.base_price_cents,
          per_km_cents:     form.per_km_cents,
          per_kg_cents:     form.per_kg_cents,
          service_area_label: form.service_area_label,
          idle_from:  new Date(form.idle_from).toISOString(),
          idle_until: new Date(form.idle_until).toISOString(),
          status:     form.status,
          carrier_response_window_mins: form.carrier_response_window_mins,
        });
      }
      onSaved();
      onClose();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Something went wrong");
    } finally {
      setSaving(false);
    }
  }

  return (
    <AnimatePresence>
      {mode.kind !== "closed" && (
        <>
          <motion.div
            key="backdrop"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.2 }}
            className="fixed inset-0 z-50 bg-black/60 backdrop-blur-sm"
            onClick={onClose}
          />
          <motion.aside
            key="panel"
            initial={{ x: "100%" }}
            animate={{ x: 0 }}
            exit={{ x: "100%" }}
            transition={{ duration: 0.35, ease: [0.16, 1, 0.3, 1] }}
            className="fixed inset-y-0 right-0 z-50 flex w-full max-w-xl flex-col overflow-hidden border-l border-glass-border bg-canvas"
          >
            <header className="flex h-16 flex-shrink-0 items-center justify-between border-b border-glass-border px-6">
              <div>
                <p className="text-2xs font-mono uppercase tracking-wider text-white/40">
                  {isEdit ? "Edit Listing" : "New Listing"}
                </p>
                <h2 className="mt-0.5 font-heading text-lg font-semibold text-white">
                  {isEdit
                    ? (mode as { kind: "edit"; listing: VehicleListing }).listing.vehicle_plate
                    : "List an idle vehicle"}
                </h2>
              </div>
              <button
                onClick={onClose}
                className="flex h-9 w-9 items-center justify-center rounded-lg border border-glass-border text-white/60 transition-colors hover:bg-glass-200 hover:text-white"
                aria-label="Close drawer"
              >
                <X className="h-4 w-4" />
              </button>
            </header>

            <div className="flex-1 overflow-y-auto px-6 py-5">
              <div className="space-y-5">
                {!isEdit && (
                  <Field label="Vehicle Plate">
                    <input
                      value={form.vehicle_plate}
                      onChange={(e) => setForm((f) => ({ ...f, vehicle_plate: e.target.value.toUpperCase() }))}
                      placeholder="ABC-1234"
                      className="input"
                    />
                  </Field>
                )}

                <Field label="Size class">
                  <select
                    value={form.size_class}
                    onChange={(e) => setForm((f) => ({ ...f, size_class: e.target.value as SizeClass }))}
                    className="input"
                    disabled={isEdit}
                  >
                    {(Object.keys(SIZE_CLASS_LABEL) as SizeClass[]).map((sc) => (
                      <option key={sc} value={sc} className="bg-canvas-100">
                        {SIZE_CLASS_LABEL[sc]}
                      </option>
                    ))}
                  </select>
                  <p className="mt-1 text-2xs text-white/40">
                    Guide: {SIZE_CLASS_CAPACITY_HINT[form.size_class]}
                  </p>
                </Field>

                <div className="grid grid-cols-2 gap-4">
                  <Field label="Max weight (kg)">
                    <input
                      type="number"
                      value={form.max_weight_kg}
                      onChange={(e) => setForm((f) => ({ ...f, max_weight_kg: Number(e.target.value) }))}
                      className="input"
                    />
                  </Field>
                  <Field label="Max volume (m³)">
                    <input
                      type="number"
                      step="0.1"
                      value={form.max_volume_m3 ?? ""}
                      onChange={(e) =>
                        setForm((f) => ({ ...f, max_volume_m3: e.target.value === "" ? null : Number(e.target.value) }))
                      }
                      className="input"
                    />
                  </Field>
                </div>

                <div className="grid grid-cols-3 gap-4">
                  <Field label="Base price (₱)">
                    <input
                      type="number"
                      value={form.base_price_cents / 100}
                      onChange={(e) => setForm((f) => ({ ...f, base_price_cents: Number(e.target.value) * 100 }))}
                      className="input"
                    />
                  </Field>
                  <Field label="Per km (₱)">
                    <input
                      type="number"
                      step="0.01"
                      value={form.per_km_cents / 100}
                      onChange={(e) => setForm((f) => ({ ...f, per_km_cents: Math.round(Number(e.target.value) * 100) }))}
                      className="input"
                    />
                  </Field>
                  <Field label="Per kg (₱)">
                    <input
                      type="number"
                      step="0.01"
                      value={form.per_kg_cents === null ? "" : form.per_kg_cents / 100}
                      onChange={(e) =>
                        setForm((f) => ({
                          ...f,
                          per_kg_cents: e.target.value === "" ? null : Math.round(Number(e.target.value) * 100),
                        }))
                      }
                      className="input"
                    />
                  </Field>
                </div>

                <Field label="Service area">
                  <input
                    value={form.service_area_label}
                    onChange={(e) => setForm((f) => ({ ...f, service_area_label: e.target.value }))}
                    placeholder="e.g. Metro Manila + Cavite"
                    className="input"
                  />
                  <p className="mt-1 text-2xs text-white/40">
                    Display label for now. Polygon drawing ships with PostGIS integration.
                  </p>
                </Field>

                <div className="grid grid-cols-2 gap-4">
                  <Field label="Idle from">
                    <input
                      type="datetime-local"
                      value={form.idle_from}
                      onChange={(e) => setForm((f) => ({ ...f, idle_from: e.target.value }))}
                      className="input"
                      disabled={isEdit}
                    />
                  </Field>
                  <Field label="Idle until">
                    <input
                      type="datetime-local"
                      value={form.idle_until}
                      onChange={(e) => setForm((f) => ({ ...f, idle_until: e.target.value }))}
                      className="input"
                    />
                  </Field>
                </div>

                <div className="grid grid-cols-2 gap-4">
                  <Field label="Response window (min)">
                    <input
                      type="number"
                      value={form.carrier_response_window_mins}
                      onChange={(e) =>
                        setForm((f) => ({ ...f, carrier_response_window_mins: Number(e.target.value) }))
                      }
                      className="input"
                    />
                  </Field>
                  <Field label="Status">
                    <select
                      value={form.status}
                      onChange={(e) => setForm((f) => ({ ...f, status: e.target.value as ListingStatus }))}
                      className="input"
                    >
                      <option value="active" className="bg-canvas-100">Active</option>
                      <option value="paused" className="bg-canvas-100">Paused</option>
                    </select>
                  </Field>
                </div>

                {error && (
                  <div className="rounded-lg border border-red-signal/30 bg-red-surface px-3 py-2 text-xs text-red-signal">
                    {error}
                  </div>
                )}
              </div>
            </div>

            <footer className="flex flex-shrink-0 items-center justify-end gap-2 border-t border-glass-border px-6 py-4">
              <button
                onClick={onClose}
                className="rounded-lg border border-glass-border bg-glass-100 px-4 py-2 text-sm text-white/70 transition-colors hover:bg-glass-200 hover:text-white"
              >
                Cancel
              </button>
              <button
                onClick={handleSave}
                disabled={saving}
                className={cn(
                  "rounded-lg border border-green-signal/40 bg-green-surface px-4 py-2 text-sm font-medium text-green-signal",
                  "transition-all hover:shadow-[0_0_12px_rgba(0,255,136,0.35)]",
                  saving && "opacity-50"
                )}
              >
                {saving ? "Saving…" : isEdit ? "Save changes" : "Publish listing"}
              </button>
            </footer>
          </motion.aside>
        </>
      )}

      <style jsx>{`
        .input {
          width: 100%;
          border-radius: 0.5rem;
          border: 1px solid rgba(255, 255, 255, 0.08);
          background: rgba(255, 255, 255, 0.03);
          padding: 0.5rem 0.75rem;
          font-size: 0.875rem;
          color: rgba(255, 255, 255, 0.9);
          outline: none;
        }
        .input:focus {
          border-color: rgba(0, 255, 136, 0.5);
          box-shadow: 0 0 0 3px rgba(0, 255, 136, 0.1);
        }
        .input:disabled {
          opacity: 0.5;
        }
      `}</style>
    </AnimatePresence>
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <label className="block">
      <span className="mb-1.5 block text-2xs font-mono uppercase tracking-wider text-white/50">
        {label}
      </span>
      {children}
    </label>
  );
}

// ── Main page ─────────────────────────────────────────────────────────────────

export default function MarketplacePage() {
  const searchParams = useSearchParams();
  // Deep-link params (from /admin/marketplace, /partner/orders reverse link):
  //   ?awb=<code>   highlight a specific booking row
  //   ?new=1        auto-open the New Listing drawer
  //   ?status=<s>   pre-filter listings by status
  //   ?partner=<id> ignored here — RLS already scopes to this partner; we
  //                 only honor it defensively on the clear-banner text.
  const qpAwb    = searchParams.get("awb");
  const qpNew    = searchParams.get("new");
  const qpStatus = searchParams.get("status") as ListingStatus | null;

  const [listings, setListings] = useState<VehicleListing[]>([]);
  const [bookings, setBookings] = useState<MarketplaceBooking[]>([]);
  const [loading, setLoading]   = useState(true);
  const [search, setSearch]     = useState("");
  const [statusFilter, setStatusFilter] = useState<ListingStatus | "all">(qpStatus ?? "all");
  const [drawer, setDrawer] = useState<DrawerMode>(qpNew ? { kind: "create" } : { kind: "closed" });

  const refresh = useCallback(async () => {
    const [l, b] = await Promise.all([fetchListings(), fetchBookings()]);
    setListings(l);
    setBookings(b);
    setLoading(false);
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  // Live-ish refresh: a booking's pending→accepted→in_transit flow correlates
  // tightly with the carrier driver flipping status on the roster channel
  // (goes online to accept, starts route to transition to in_transit). Poll
  // backstop at 30s catches cancellations and idle-window expirations.
  useRosterEvents((event) => {
    if (event.type === "status_changed") refresh();
  });
  useEffect(() => {
    const id = setInterval(refresh, 30_000);
    // Cross-portal live refresh: merchant booking creation publishes to the
    // bus (ADR-0013 §Booking flow — stand-in for `marketplace.booking_created`
    // on Kafka); the storage event refreshes our table immediately.
    const unsubscribe = subscribeToMarketplaceUpdates(() => refresh());
    return () => { clearInterval(id); unsubscribe(); };
  }, [refresh]);

  const [respondingTo, setRespondingTo] = useState<string | null>(null);

  async function handleAccept(b: MarketplaceBooking) {
    setRespondingTo(b.id);
    try {
      await acceptBooking(b.id);
      await refresh();
    } finally {
      setRespondingTo(null);
    }
  }

  async function handleReject(b: MarketplaceBooking) {
    setRespondingTo(b.id);
    try {
      await rejectBooking(b.id);
      await refresh();
    } finally {
      setRespondingTo(null);
    }
  }

  // KPIs
  const kpis = useMemo(() => {
    const active = listings.filter((l) => l.status === "active" || l.status === "booked").length;
    const todayBookings = listings.reduce((s, l) => s + l.bookings_today, 0);
    const todayRevenue = listings.reduce((s, l) => s + l.revenue_today_cents, 0);
    const idle6h = listings.filter((l) => {
      const untilMs = new Date(l.idle_until).getTime() - Date.now();
      return untilMs > 0 && untilMs < 6 * 3_600_000 && l.status === "active";
    }).length;
    return { active, todayBookings, todayRevenue, idle6h };
  }, [listings]);

  // Filters
  const filtered = useMemo(() => {
    return listings.filter((l) => {
      if (statusFilter !== "all" && l.status !== statusFilter) return false;
      if (!search) return true;
      const q = search.toLowerCase();
      return (
        l.vehicle_plate.toLowerCase().includes(q) ||
        SIZE_CLASS_LABEL[l.size_class].toLowerCase().includes(q) ||
        l.service_area_label.toLowerCase().includes(q)
      );
    });
  }, [listings, search, statusFilter]);

  async function handleTogglePause(l: VehicleListing) {
    const next: ListingStatus = l.status === "active" ? "paused" : "active";
    await updateListing(l.id, { status: next });
    refresh();
  }

  async function handleDelete(l: VehicleListing) {
    if (!confirm(`Remove listing for ${l.vehicle_plate}? Active bookings remain valid.`)) return;
    await deleteListing(l.id);
    refresh();
  }

  return (
    <div className="space-y-6">
      {/* ── Header row ────────────────────────────────────────────────────── */}
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div>
          <h1 className="font-heading text-2xl font-bold text-white">
            Marketplace Discovery
          </h1>
          <p className="mt-1 max-w-2xl text-sm text-white/50">
            List idle vehicles for on-demand consumer booking. Match happens by size,
            weight, and distance — accepted bookings enter order-intake like any shipment.
          </p>
        </div>
        <button
          onClick={() => setDrawer({ kind: "create" })}
          className={cn(
            "flex items-center gap-2 rounded-lg border border-green-signal/40 bg-green-surface px-4 py-2",
            "text-sm font-medium text-green-signal transition-all",
            "hover:shadow-[0_0_14px_rgba(0,255,136,0.4)]"
          )}
        >
          <Plus className="h-4 w-4" />
          New Listing
        </button>
      </div>

      {/* ── Deep-link banner ─────────────────────────────────────────────── */}
      {(qpAwb || qpStatus) && (
        <div className="flex flex-wrap items-center gap-2 rounded-lg border border-green-signal/25 bg-green-signal/5 px-3 py-2">
          <ExternalLink className="h-3 w-3 text-green-signal" />
          <span className="font-mono text-xs text-white/70">
            {qpAwb && (
              <>Focused booking <span className="font-bold text-green-signal">{qpAwb}</span></>
            )}
            {qpAwb && qpStatus && <span className="text-white/30"> · </span>}
            {qpStatus && (
              <>Status filter <span className="font-bold text-green-signal">{qpStatus}</span></>
            )}
          </span>
          <a
            href="/partner/marketplace"
            title="Clear filter"
            className="ml-auto inline-flex h-5 w-5 items-center justify-center rounded-md text-white/40 transition-colors hover:text-white"
          >
            <X className="h-3 w-3" />
          </a>
        </div>
      )}

      {/* ── KPI strip ─────────────────────────────────────────────────────── */}
      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
        <Kpi
          label="Live Listings"
          value={kpis.active.toString()}
          icon={Truck}
          glow="green"
          hint={`${listings.length} total`}
        />
        <Kpi
          label="Idle < 6h"
          value={kpis.idle6h.toString()}
          icon={Clock}
          glow="amber"
          hint="Sell before window closes"
        />
        <Kpi
          label="Bookings Today"
          value={kpis.todayBookings.toString()}
          icon={Package}
          glow="cyan"
        />
        <Kpi
          label="Revenue Today"
          value={formatCentsPhp(kpis.todayRevenue)}
          icon={DollarSign}
          glow="purple"
        />
      </div>

      {/* ── Listings table ────────────────────────────────────────────────── */}
      <GlassCard size="sm" padding="none" accent glow="green">
        <div className="flex flex-wrap items-center gap-3 border-b border-glass-border px-5 py-3">
          <div className="flex items-center gap-2 font-mono text-2xs uppercase tracking-wider text-white/50">
            <Activity className="h-3 w-3 text-green-signal" />
            Vehicle Listings
          </div>

          <div className="relative ml-auto">
            <Search className="pointer-events-none absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-white/40" />
            <input
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder="Search plate, size, area…"
              className="w-56 rounded-lg border border-glass-border bg-glass-100 py-1.5 pl-9 pr-3 text-xs text-white/80 outline-none transition-colors focus:border-green-signal/40 focus:bg-glass-200"
            />
          </div>

          <select
            value={statusFilter}
            onChange={(e) => setStatusFilter(e.target.value as ListingStatus | "all")}
            className="rounded-lg border border-glass-border bg-glass-100 px-3 py-1.5 text-xs text-white/80 outline-none focus:border-green-signal/40"
          >
            <option value="all" className="bg-canvas-100">All statuses</option>
            <option value="active" className="bg-canvas-100">Active</option>
            <option value="booked" className="bg-canvas-100">Booked</option>
            <option value="paused" className="bg-canvas-100">Paused</option>
            <option value="expired" className="bg-canvas-100">Expired</option>
          </select>
        </div>

        <div className="overflow-x-auto">
          <table className="w-full min-w-[980px] text-left text-sm">
            <thead>
              <tr className="border-b border-glass-border text-2xs font-mono uppercase tracking-wider text-white/40">
                <th className="px-5 py-3 font-medium">Vehicle</th>
                <th className="px-5 py-3 font-medium">Capacity</th>
                <th className="px-5 py-3 font-medium">Pricing</th>
                <th className="px-5 py-3 font-medium">Availability</th>
                <th className="px-5 py-3 font-medium">Today</th>
                <th className="px-5 py-3 font-medium">Status</th>
                <th className="px-5 py-3 font-medium text-right">Actions</th>
              </tr>
            </thead>
            <tbody>
              {loading ? (
                <tr>
                  <td colSpan={7} className="px-5 py-10 text-center text-xs text-white/40">
                    Loading listings…
                  </td>
                </tr>
              ) : filtered.length === 0 ? (
                <tr>
                  <td colSpan={7} className="px-5 py-10 text-center text-xs text-white/40">
                    No listings match your filters.
                  </td>
                </tr>
              ) : (
                filtered.map((l) => (
                  <tr
                    key={l.id}
                    className="border-b border-glass-border/50 last:border-0 transition-colors hover:bg-glass-100"
                  >
                    <td className="px-5 py-3">
                      <div className="flex items-center gap-3">
                        <div className="flex h-8 w-8 items-center justify-center rounded-lg border border-glass-border bg-glass-100">
                          <Truck className="h-3.5 w-3.5 text-white/60" />
                        </div>
                        <div className="min-w-0">
                          <p className="font-mono text-xs font-medium text-white">
                            {l.vehicle_plate}
                          </p>
                          <p className="mt-0.5 text-2xs text-white/50">
                            {SIZE_CLASS_LABEL[l.size_class]}
                          </p>
                        </div>
                      </div>
                    </td>
                    <td className="px-5 py-3">
                      <div className="flex items-center gap-1.5 text-xs text-white/70">
                        <Gauge className="h-3 w-3 text-white/40" />
                        {l.max_weight_kg.toLocaleString()} kg
                        {l.max_volume_m3 !== null && (
                          <span className="text-white/40"> · {l.max_volume_m3} m³</span>
                        )}
                      </div>
                    </td>
                    <td className="px-5 py-3">
                      <div className="text-xs text-white/80">
                        <span className="font-mono">{formatCentsPhp(l.base_price_cents)}</span>
                        <span className="text-white/40"> base</span>
                      </div>
                      <div className="mt-0.5 text-2xs text-white/50">
                        + {formatCentsPhp(l.per_km_cents)}/km
                        {l.per_kg_cents !== null && ` · ${formatCentsPhp(l.per_kg_cents)}/kg`}
                      </div>
                    </td>
                    <td className="px-5 py-3">
                      <div className="flex items-center gap-1.5 text-xs text-white/70">
                        <Clock className="h-3 w-3 text-white/40" />
                        {fmtWindow(l.idle_from, l.idle_until)}
                      </div>
                      <div className="mt-0.5 flex items-center gap-1 text-2xs text-white/40">
                        <MapPin className="h-2.5 w-2.5" />
                        {l.service_area_label}
                      </div>
                    </td>
                    <td className="px-5 py-3">
                      <div className="text-xs text-white/80">
                        <span className="font-mono">{l.bookings_today}</span>
                        <span className="text-white/40"> bk</span>
                      </div>
                      <div className="mt-0.5 font-mono text-2xs text-green-signal">
                        {formatCentsPhp(l.revenue_today_cents)}
                      </div>
                    </td>
                    <td className="px-5 py-3">
                      <NeonBadge variant={LISTING_STATUS_VARIANT[l.status]} dot>
                        {l.status}
                      </NeonBadge>
                    </td>
                    <td className="px-5 py-3">
                      <div className="flex items-center justify-end gap-1">
                        <button
                          onClick={() => handleTogglePause(l)}
                          disabled={l.status === "booked" || l.status === "expired"}
                          className="flex h-7 w-7 items-center justify-center rounded-md border border-glass-border text-white/50 transition-colors hover:border-amber-signal/40 hover:bg-amber-surface hover:text-amber-signal disabled:opacity-30 disabled:hover:border-glass-border disabled:hover:bg-transparent disabled:hover:text-white/50"
                          title={l.status === "paused" ? "Resume" : "Pause"}
                        >
                          {l.status === "paused" ? (
                            <Play className="h-3 w-3" />
                          ) : (
                            <Pause className="h-3 w-3" />
                          )}
                        </button>
                        <button
                          onClick={() => setDrawer({ kind: "edit", listing: l })}
                          className="flex h-7 w-7 items-center justify-center rounded-md border border-glass-border text-white/50 transition-colors hover:border-cyan-neon/40 hover:bg-cyan-surface hover:text-cyan-neon"
                          title="Edit"
                        >
                          <Pencil className="h-3 w-3" />
                        </button>
                        <button
                          onClick={() => handleDelete(l)}
                          disabled={l.status === "booked"}
                          className="flex h-7 w-7 items-center justify-center rounded-md border border-glass-border text-white/50 transition-colors hover:border-red-signal/40 hover:bg-red-surface hover:text-red-signal disabled:opacity-30 disabled:hover:border-glass-border disabled:hover:bg-transparent disabled:hover:text-white/50"
                          title="Remove"
                        >
                          <Trash2 className="h-3 w-3" />
                        </button>
                      </div>
                    </td>
                  </tr>
                ))
              )}
            </tbody>
          </table>
        </div>

        <div className="flex items-center justify-between border-t border-glass-border px-5 py-2.5 text-2xs text-white/40">
          <span>
            Showing {filtered.length} of {listings.length} listings
          </span>
          <span className="font-mono">
            Scope = partner · RLS enforces tenant &amp; partner isolation
          </span>
        </div>
      </GlassCard>

      {/* ── Recent bookings ───────────────────────────────────────────────── */}
      <GlassCard size="sm" padding="none" accent glow="cyan">
        <div className="flex items-center gap-2 border-b border-glass-border px-5 py-3 font-mono text-2xs uppercase tracking-wider text-white/50">
          <Package className="h-3 w-3 text-cyan-neon" />
          Recent Bookings
        </div>

        <div className="overflow-x-auto">
          <table className="w-full min-w-[760px] text-left text-sm">
            <thead>
              <tr className="border-b border-glass-border text-2xs font-mono uppercase tracking-wider text-white/40">
                <th className="px-5 py-3 font-medium">AWB</th>
                <th className="px-5 py-3 font-medium">Consumer</th>
                <th className="px-5 py-3 font-medium">Route</th>
                <th className="px-5 py-3 font-medium">Cargo</th>
                <th className="px-5 py-3 font-medium">Quoted</th>
                <th className="px-5 py-3 font-medium">Pickup</th>
                <th className="px-5 py-3 font-medium">Status</th>
              </tr>
            </thead>
            <tbody>
              {bookings.length === 0 ? (
                <tr>
                  <td colSpan={7} className="px-5 py-10 text-center text-xs text-white/40">
                    No bookings yet. Listings appear in consumer discovery once published.
                  </td>
                </tr>
              ) : (
                bookings.map((b) => (
                  <tr
                    key={b.id}
                    className={cn(
                      "border-b border-glass-border/50 last:border-0 transition-colors hover:bg-glass-100",
                      qpAwb === b.awb && "bg-green-signal/10",
                    )}
                  >
                    <td className="px-5 py-3 font-mono text-xs text-white">{b.awb}</td>
                    <td className="px-5 py-3 text-xs text-white/80">
                      {b.consumer_name}
                      <div className="mt-0.5 font-mono text-2xs text-white/40">
                        {b.consumer_phone ?? "—"}
                      </div>
                    </td>
                    <td className="px-5 py-3 text-xs text-white/70">
                      {b.pickup_label}
                      <div className="mt-0.5 text-2xs text-white/40">→ {b.dropoff_label}</div>
                    </td>
                    <td className="px-5 py-3 text-xs text-white/70">
                      {b.cargo_weight_kg.toLocaleString()} kg
                      {b.cargo_volume_m3 !== null && (
                        <span className="text-white/40"> · {b.cargo_volume_m3} m³</span>
                      )}
                    </td>
                    <td className="px-5 py-3 font-mono text-xs text-green-signal">
                      {formatCentsPhp(b.quoted_price_cents)}
                    </td>
                    <td className="px-5 py-3 text-xs text-white/70">
                      {fmtRelative(b.pickup_at)}
                    </td>
                    <td className="px-5 py-3">
                      <div className="flex items-center gap-2">
                        <NeonBadge variant={BOOKING_STATUS_VARIANT[b.status]} dot>
                          {b.status.replace("_", " ")}
                        </NeonBadge>
                        {b.status === "pending" && (
                          // Carrier response — accept flips status + fires
                          // downstream dispatch enqueue (ADR-0013 §Booking flow).
                          <div className="flex items-center gap-1">
                            <button
                              onClick={() => handleAccept(b)}
                              disabled={respondingTo === b.id}
                              className="flex h-7 items-center gap-1 rounded-md border border-green-signal/40 bg-green-surface px-2 text-2xs font-mono text-green-signal transition-all hover:shadow-[0_0_8px_rgba(0,255,136,0.4)] disabled:opacity-50"
                              title="Accept booking"
                            >
                              <Check className="h-3 w-3" />
                              Accept
                            </button>
                            <button
                              onClick={() => handleReject(b)}
                              disabled={respondingTo === b.id}
                              className="flex h-7 items-center gap-1 rounded-md border border-red-signal/40 bg-red-surface px-2 text-2xs font-mono text-red-signal transition-all hover:bg-red-signal/15 disabled:opacity-50"
                              title="Reject booking"
                            >
                              <XCircle className="h-3 w-3" />
                              Reject
                            </button>
                          </div>
                        )}
                      </div>
                    </td>
                  </tr>
                ))
              )}
            </tbody>
          </table>
        </div>
      </GlassCard>

      {/* ── Drawer ────────────────────────────────────────────────────────── */}
      <ListingDrawer
        mode={drawer}
        onClose={() => setDrawer({ kind: "closed" })}
        onSaved={refresh}
      />
    </div>
  );
}
