"use client";
/**
 * Merchant Portal — Marketplace Discovery
 *
 * Merchant-side view of marketplace vehicle listings (ADR-0013). The merchant
 * browses idle capacity across all alliance + marketplace partners in the
 * tenant, books a vehicle, and the booking creates a shipment via
 * order-intake (zero-loss invariant preserved).
 *
 * Cross-portal deep-links:
 *   - Each booking row links to its shipment detail (intra-portal)
 *   - Each partner name links to a tenant-public partner overview (planned)
 *   - Banner respects ?awb, ?partner, ?listing, ?new query params
 */

import { useCallback, useEffect, useMemo, useState, Suspense } from "react";
import { useSearchParams } from "next/navigation";
import Link from "next/link";
import { motion } from "framer-motion";
import {
  Store, Truck, Clock, Search, Star, X, Check, ExternalLink,
  Gauge, Package, MapPin, Calendar,
} from "lucide-react";
import { variants } from "@/lib/design-system/tokens";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge, type BadgeVariant } from "@/components/ui/neon-badge";
import {
  fetchAvailableListings,
  fetchMyBookings,
  fetchMarketplaceStats,
  createBooking,
  subscribeToMarketplaceUpdates,
  SIZE_CLASS_LABEL,
  SIZE_CLASS_CAPACITY_HINT,
  formatCentsPhp,
  type MerchantListing,
  type MerchantBooking,
  type MerchantMarketplaceStats,
  type SizeClass,
  type ListingStatus,
  type BookingStatus,
  type PartnerType,
} from "@/lib/api/marketplace";

// ── Status → badge mapping ────────────────────────────────────────────────────

const LISTING_STATUS: Record<ListingStatus, { label: string; variant: BadgeVariant }> = {
  active: { label: "Available", variant: "green"  },
  booked: { label: "Booked",    variant: "amber"  },
};

const BOOKING_STATUS: Record<BookingStatus, { label: string; variant: BadgeVariant }> = {
  pending:    { label: "Pending",    variant: "amber"  },
  accepted:   { label: "Accepted",   variant: "cyan"   },
  rejected:   { label: "Rejected",   variant: "red"    },
  in_transit: { label: "In Transit", variant: "purple" },
  delivered:  { label: "Delivered",  variant: "green"  },
  cancelled:  { label: "Cancelled",  variant: "muted"  },
  disputed:   { label: "Disputed",   variant: "red"    },
};

const PARTNER_TYPE: Record<PartnerType, { label: string; variant: BadgeVariant }> = {
  alliance:    { label: "Alliance",    variant: "cyan"   },
  marketplace: { label: "Marketplace", variant: "purple" },
};

const SIZE_FILTERS: Array<{ label: string; value: SizeClass | "all" }> = [
  { label: "All",        value: "all"        },
  { label: "Motorcycle", value: "motorcycle" },
  { label: "Sedan",      value: "sedan"      },
  { label: "Van",        value: "van"        },
  { label: "L300",       value: "l300"       },
  { label: "6-Wheeler",  value: "6wheeler"   },
  { label: "10-Wheeler", value: "10wheeler"  },
];

type Tab = "browse" | "bookings";

function idleCountdown(idleUntil: string): string {
  const diffMs = new Date(idleUntil).getTime() - Date.now();
  if (diffMs <= 0) return "Expired";
  const hours = Math.floor(diffMs / 3_600_000);
  const mins  = Math.floor((diffMs % 3_600_000) / 60_000);
  if (hours > 0) return `${hours}h ${mins}m`;
  return `${mins}m`;
}

// ── Page entry ────────────────────────────────────────────────────────────────

export default function MerchantMarketplacePage() {
  return (
    <Suspense fallback={null}>
      <MarketplacePageInner />
    </Suspense>
  );
}

function MarketplacePageInner() {
  const qp = useSearchParams();
  const qpAwb     = qp?.get("awb");
  const qpListing = qp?.get("listing");
  const qpPartner = qp?.get("partner");

  const [stats,    setStats]    = useState<MerchantMarketplaceStats | null>(null);
  const [listings, setListings] = useState<MerchantListing[]>([]);
  const [bookings, setBookings] = useState<MerchantBooking[]>([]);
  const [loading,  setLoading]  = useState(true);

  const [tab,           setTab]           = useState<Tab>(qpAwb ? "bookings" : "browse");
  const [sizeFilter,    setSizeFilter]    = useState<SizeClass | "all">("all");
  const [search,        setSearch]        = useState(qpPartner ? qpPartner : "");
  const [bookingFor,    setBookingFor]    = useState<MerchantListing | null>(null);

  const refresh = useCallback(async () => {
    const [l, b, s] = await Promise.all([
      fetchAvailableListings(),
      fetchMyBookings(),
      fetchMarketplaceStats(),
    ]);
    setListings(l);
    setBookings(b);
    setStats(s);
    setLoading(false);
  }, []);

  useEffect(() => {
    refresh();
    const id = setInterval(refresh, 30_000);
    // Cross-portal live refresh: partner accepts/rejects fire a `storage`
    // event in other tabs (ADR-0013 §Booking flow notification channel is
    // Kafka in prod; localStorage stands in pre-backend).
    const unsubscribe = subscribeToMarketplaceUpdates(() => refresh());
    return () => { clearInterval(id); unsubscribe(); };
  }, [refresh]);

  // Auto-open booking drawer when arriving with ?listing=<id>
  useEffect(() => {
    if (!qpListing || listings.length === 0) return;
    const l = listings.find((x) => x.id === qpListing);
    if (l && l.status === "active") setBookingFor(l);
  }, [qpListing, listings]);

  const visibleListings = useMemo(() => {
    const q = search.trim().toLowerCase();
    return listings.filter((l) => {
      if (sizeFilter !== "all" && l.size_class !== sizeFilter) return false;
      if (q) {
        const hay = `${l.partner_display_name} ${l.service_area_label} ${l.size_class}`.toLowerCase();
        if (!hay.includes(q)) return false;
      }
      return true;
    });
  }, [listings, sizeFilter, search]);

  async function handleBook(input: { pickup_label: string; dropoff_label: string; cargo_weight_kg: number; pickup_at: string }) {
    if (!bookingFor) return;
    await createBooking({ listing_id: bookingFor.id, ...input });
    setBookingFor(null);
    setTab("bookings");
    refresh();
  }

  return (
    <motion.div
      variants={variants.staggerContainer}
      initial="hidden"
      animate="visible"
      className="flex flex-col gap-5 p-6"
    >
      {/* Deep-link banner */}
      {qpAwb && (
        <motion.div variants={variants.fadeInUp}>
          <div className="flex items-center justify-between rounded-xl border border-cyan-neon/30 bg-cyan-surface px-4 py-2.5">
            <div className="flex items-center gap-2.5">
              <ExternalLink size={14} className="text-cyan-neon" />
              <p className="text-xs text-white/80">
                Linked from tracking · <span className="font-mono text-cyan-neon">{qpAwb}</span>
              </p>
            </div>
            <Link href="/marketplace" className="text-xs text-white/40 hover:text-white">
              <X size={14} />
            </Link>
          </div>
        </motion.div>
      )}

      {/* Header */}
      <motion.div variants={variants.fadeInUp} className="flex items-center justify-between">
        <div>
          <h1 className="font-heading text-2xl font-bold text-white flex items-center gap-2">
            <Store size={22} className="text-purple-plasma" />
            Marketplace
          </h1>
          <p className="text-sm text-white/40 font-mono mt-0.5">
            Browse idle capacity across partners · book a vehicle · track bookings
          </p>
        </div>
      </motion.div>

      {/* KPI strip */}
      <motion.div variants={variants.fadeInUp} className="grid grid-cols-2 md:grid-cols-4 gap-3">
        {[
          { label: "Available Now",      value: stats?.available_now ?? "—",                     icon: Truck,   tint: "text-green-signal"  },
          { label: "Avg Rate / km",      value: stats ? formatCentsPhp(stats.avg_rate_per_km)    : "—", icon: Gauge,   tint: "text-cyan-neon"     },
          { label: "Partners Reachable", value: stats?.partners_reachable ?? "—",                icon: Store,   tint: "text-purple-plasma" },
          { label: "My Active Bookings", value: stats?.my_bookings_active ?? "—",                icon: Package, tint: "text-amber-signal"  },
        ].map((k) => {
          const Icon = k.icon;
          return (
            <div key={k.label} className="rounded-xl border border-glass-border bg-glass-100 px-4 py-3">
              <div className="flex items-start justify-between">
                <p className="text-xs text-white/40 font-mono">{k.label}</p>
                <Icon size={14} className={k.tint} />
              </div>
              <p className={`font-heading text-2xl font-bold ${k.tint} mt-1`}>{k.value}</p>
            </div>
          );
        })}
      </motion.div>

      {/* Tab switcher */}
      <motion.div variants={variants.fadeInUp}>
        <div className="flex gap-1 border-b border-glass-border">
          <TabButton active={tab === "browse"}   onClick={() => setTab("browse")}>
            Browse Listings
          </TabButton>
          <TabButton active={tab === "bookings"} onClick={() => setTab("bookings")}>
            My Bookings ({bookings.filter((b) => b.status === "pending" || b.status === "accepted" || b.status === "in_transit").length})
          </TabButton>
        </div>
      </motion.div>

      {/* Filter + search */}
      {tab === "browse" && (
        <motion.div variants={variants.fadeInUp}>
          <GlassCard>
            <div className="flex flex-col md:flex-row md:items-center gap-3">
              <div className="relative flex-1">
                <Search size={13} className="absolute left-3 top-1/2 -translate-y-1/2 text-white/30" />
                <input
                  type="text"
                  value={search}
                  onChange={(e) => setSearch(e.target.value)}
                  placeholder="Partner, area, or vehicle class..."
                  className="w-full rounded-lg border border-glass-border bg-canvas-100 pl-9 pr-3 py-2 text-sm text-white placeholder-white/30 focus:border-purple-plasma/50 focus:outline-none"
                />
              </div>
              <div className="flex gap-1.5 flex-wrap">
                {SIZE_FILTERS.map((f) => (
                  <button
                    key={f.value}
                    onClick={() => setSizeFilter(f.value)}
                    className={`rounded-full px-3 py-1 text-xs font-medium transition-all ${
                      sizeFilter === f.value
                        ? "bg-canvas border border-glass-border-bright text-white"
                        : "text-white/40 border border-glass-border hover:text-white"
                    }`}
                  >
                    {f.label}
                  </button>
                ))}
              </div>
            </div>
          </GlassCard>
        </motion.div>
      )}

      {/* Browse listings */}
      {tab === "browse" && (
        <motion.div variants={variants.fadeInUp}>
          <GlassCard>
            {loading ? (
              <p className="text-sm text-white/40 py-6 text-center">Loading listings…</p>
            ) : visibleListings.length === 0 ? (
              <p className="text-sm text-white/40 py-6 text-center">No listings match your filter.</p>
            ) : (
              <div className="overflow-x-auto">
                <table className="w-full text-sm">
                  <thead className="text-xs text-white/40 uppercase tracking-wide border-b border-glass-border">
                    <tr>
                      <th className="text-left font-medium py-2.5 pr-4">Partner</th>
                      <th className="text-left font-medium py-2.5 pr-4">Vehicle</th>
                      <th className="text-left font-medium py-2.5 pr-4">Service Area</th>
                      <th className="text-left font-medium py-2.5 pr-4">Base + /km</th>
                      <th className="text-left font-medium py-2.5 pr-4">Idle Left</th>
                      <th className="text-left font-medium py-2.5 pr-4">Rating</th>
                      <th className="text-left font-medium py-2.5 pr-4">Status</th>
                      <th className="text-right font-medium py-2.5">Action</th>
                    </tr>
                  </thead>
                  <tbody>
                    {visibleListings.map((l) => {
                      const { label: statusLabel, variant } = LISTING_STATUS[l.status];
                      const { label: ptypeLabel, variant: ptypeVariant } = PARTNER_TYPE[l.partner_type];
                      const canBook = l.status === "active";
                      return (
                        <tr key={l.id} className="border-b border-glass-border/50 hover:bg-glass-100/40 transition-colors">
                          <td className="py-3 pr-4">
                            <div className="flex flex-col gap-0.5">
                              <span className="text-white text-sm">{l.partner_display_name}</span>
                              <NeonBadge variant={ptypeVariant}>{ptypeLabel}</NeonBadge>
                            </div>
                          </td>
                          <td className="py-3 pr-4">
                            <div className="flex flex-col gap-0.5">
                              <span className="text-white text-sm">{SIZE_CLASS_LABEL[l.size_class]}</span>
                              <span className="text-2xs font-mono text-white/30">
                                {l.max_weight_kg.toLocaleString()} kg · {l.max_volume_m3 ?? "—"} m³
                              </span>
                            </div>
                          </td>
                          <td className="py-3 pr-4 text-xs text-white/70">{l.service_area_label}</td>
                          <td className="py-3 pr-4 font-mono text-xs text-white/80">
                            {formatCentsPhp(l.base_price_cents)}
                            <span className="text-white/30"> + </span>
                            {formatCentsPhp(l.per_km_cents)}/km
                          </td>
                          <td className="py-3 pr-4 font-mono text-xs text-amber-signal flex items-center gap-1">
                            <Clock size={11} />
                            {idleCountdown(l.idle_until)}
                          </td>
                          <td className="py-3 pr-4 font-mono text-xs text-white/80 flex items-center gap-1">
                            <Star size={11} className="text-amber-signal fill-amber-signal" />
                            {l.rating.toFixed(1)}
                          </td>
                          <td className="py-3 pr-4">
                            <NeonBadge variant={variant}>{statusLabel}</NeonBadge>
                          </td>
                          <td className="py-3 text-right">
                            <button
                              disabled={!canBook}
                              onClick={() => canBook && setBookingFor(l)}
                              className={`rounded-md border px-3 py-1 text-xs transition-colors ${
                                canBook
                                  ? "border-purple-plasma/40 bg-purple-surface text-purple-plasma hover:border-purple-plasma hover:bg-purple-plasma/15"
                                  : "border-glass-border text-white/20 cursor-not-allowed"
                              }`}
                            >
                              {canBook ? "Book" : "Unavailable"}
                            </button>
                          </td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </div>
            )}
          </GlassCard>
        </motion.div>
      )}

      {/* My bookings */}
      {tab === "bookings" && (
        <motion.div variants={variants.fadeInUp}>
          <GlassCard>
            {loading ? (
              <p className="text-sm text-white/40 py-6 text-center">Loading bookings…</p>
            ) : bookings.length === 0 ? (
              <p className="text-sm text-white/40 py-6 text-center">
                No bookings yet · browse listings to create one.
              </p>
            ) : (
              <div className="overflow-x-auto">
                <table className="w-full text-sm">
                  <thead className="text-xs text-white/40 uppercase tracking-wide border-b border-glass-border">
                    <tr>
                      <th className="text-left font-medium py-2.5 pr-4">AWB</th>
                      <th className="text-left font-medium py-2.5 pr-4">Partner</th>
                      <th className="text-left font-medium py-2.5 pr-4">Route</th>
                      <th className="text-left font-medium py-2.5 pr-4">Vehicle</th>
                      <th className="text-left font-medium py-2.5 pr-4">Quoted</th>
                      <th className="text-left font-medium py-2.5 pr-4">Pickup</th>
                      <th className="text-left font-medium py-2.5 pr-4">Status</th>
                    </tr>
                  </thead>
                  <tbody>
                    {bookings.map((b) => {
                      const { label, variant } = BOOKING_STATUS[b.status];
                      const isHighlighted = qpAwb && qpAwb === b.awb;
                      return (
                        <tr
                          key={b.id}
                          className={`border-b border-glass-border/50 transition-colors ${
                            isHighlighted ? "bg-purple-plasma/10" : "hover:bg-glass-100/40"
                          }`}
                        >
                          <td className="py-3 pr-4">
                            <Link
                              href={`/shipments?awb=${encodeURIComponent(b.awb)}`}
                              className="font-mono text-xs text-cyan-neon hover:underline"
                            >
                              {b.awb}
                            </Link>
                          </td>
                          <td className="py-3 pr-4 text-white text-sm">{b.partner_display_name}</td>
                          <td className="py-3 pr-4 text-xs text-white/70">
                            <div className="flex items-center gap-1">
                              <MapPin size={10} className="text-white/30" />
                              {b.pickup_label}
                              <span className="text-white/20"> → </span>
                              {b.dropoff_label}
                            </div>
                          </td>
                          <td className="py-3 pr-4 text-xs text-white/70">
                            {SIZE_CLASS_LABEL[b.size_class]} · {b.cargo_weight_kg} kg
                          </td>
                          <td className="py-3 pr-4 font-mono text-xs text-white/80">
                            {formatCentsPhp(b.quoted_price_cents)}
                          </td>
                          <td className="py-3 pr-4 font-mono text-xs text-white/60">
                            {new Date(b.pickup_at).toLocaleString("en-PH", { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" })}
                          </td>
                          <td className="py-3 pr-4">
                            <div className="flex items-center gap-2">
                              <NeonBadge variant={variant}>{label}</NeonBadge>
                              {b.status === "disputed" && (
                                // Plain <a> crosses the /merchant → /admin basePath boundary.
                                // Ops-escalation path — tenant-admin session required to load.
                                <a
                                  href={`/admin/marketplace?awb=${encodeURIComponent(b.awb)}&status=disputed`}
                                  className="inline-flex items-center gap-1 text-2xs font-mono text-red-signal hover:underline"
                                  title="Escalate to tenant ops"
                                >
                                  Escalate <ExternalLink size={10} />
                                </a>
                              )}
                            </div>
                          </td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </div>
            )}
          </GlassCard>
        </motion.div>
      )}

      {/* Booking drawer */}
      {bookingFor && (
        <BookingDrawer
          listing={bookingFor}
          onCancel={() => setBookingFor(null)}
          onSubmit={handleBook}
        />
      )}
    </motion.div>
  );
}

// ── Tab button ────────────────────────────────────────────────────────────────

function TabButton({ active, onClick, children }: { active: boolean; onClick: () => void; children: React.ReactNode }) {
  return (
    <button
      onClick={onClick}
      className={`relative px-4 py-2 text-sm font-medium transition-colors ${
        active ? "text-white" : "text-white/40 hover:text-white/70"
      }`}
    >
      {children}
      {active && (
        <motion.div
          layoutId="merchant-marketplace-tab-underline"
          className="absolute inset-x-0 -bottom-px h-0.5 bg-purple-plasma"
          style={{ boxShadow: "0 0 8px rgba(168, 85, 247, 0.6)" }}
        />
      )}
    </button>
  );
}

// ── Booking drawer ────────────────────────────────────────────────────────────

function BookingDrawer({
  listing,
  onCancel,
  onSubmit,
}: {
  listing: MerchantListing;
  onCancel: () => void;
  onSubmit: (input: { pickup_label: string; dropoff_label: string; cargo_weight_kg: number; pickup_at: string }) => Promise<void>;
}) {
  const [pickup,  setPickup]  = useState("");
  const [dropoff, setDropoff] = useState("");
  const [weight,  setWeight]  = useState<number | "">("");
  const [when,    setWhen]    = useState("");
  const [busy,    setBusy]    = useState(false);

  const canSubmit = pickup.trim() && dropoff.trim() && typeof weight === "number" && weight > 0 && weight <= listing.max_weight_kg && when;

  async function submit() {
    if (!canSubmit) return;
    setBusy(true);
    try {
      await onSubmit({
        pickup_label:    pickup.trim(),
        dropoff_label:   dropoff.trim(),
        cargo_weight_kg: weight as number,
        pickup_at:       new Date(when).toISOString(),
      });
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-end md:items-center justify-center bg-black/70 backdrop-blur-sm">
      <motion.div
        initial={{ y: 40, opacity: 0 }}
        animate={{ y: 0,  opacity: 1 }}
        transition={{ duration: 0.25, ease: [0.16, 1, 0.3, 1] }}
        className="w-full md:w-[520px] rounded-t-2xl md:rounded-2xl border border-glass-border bg-canvas-100 p-6 max-h-[90vh] overflow-y-auto"
      >
        <div className="flex items-start justify-between mb-4">
          <div>
            <h2 className="font-heading text-lg font-bold text-white">Book Vehicle</h2>
            <p className="text-xs text-white/50 font-mono mt-0.5">
              {listing.partner_display_name} · {SIZE_CLASS_LABEL[listing.size_class]}
            </p>
          </div>
          <button onClick={onCancel} className="text-white/40 hover:text-white"><X size={16} /></button>
        </div>

        <div className="rounded-lg border border-glass-border bg-glass-100 p-3 mb-4 text-xs text-white/60 font-mono">
          <p>{SIZE_CLASS_CAPACITY_HINT[listing.size_class]}</p>
          <p className="mt-1">
            {formatCentsPhp(listing.base_price_cents)} base
            <span className="text-white/30"> + </span>
            {formatCentsPhp(listing.per_km_cents)}/km
            <span className="text-white/30"> · </span>
            carrier replies within {listing.response_window_mins} min
          </p>
        </div>

        <div className="flex flex-col gap-3">
          <label className="flex flex-col gap-1">
            <span className="text-xs text-white/50">Pickup address</span>
            <input
              value={pickup}
              onChange={(e) => setPickup(e.target.value)}
              placeholder="e.g., Pasig Warehouse, Ortigas Ave"
              className="rounded-md border border-glass-border bg-canvas px-3 py-2 text-sm text-white placeholder-white/30 focus:border-purple-plasma/50 focus:outline-none"
            />
          </label>
          <label className="flex flex-col gap-1">
            <span className="text-xs text-white/50">Drop-off address</span>
            <input
              value={dropoff}
              onChange={(e) => setDropoff(e.target.value)}
              placeholder="e.g., Batangas Industrial Park"
              className="rounded-md border border-glass-border bg-canvas px-3 py-2 text-sm text-white placeholder-white/30 focus:border-purple-plasma/50 focus:outline-none"
            />
          </label>
          <div className="grid grid-cols-2 gap-3">
            <label className="flex flex-col gap-1">
              <span className="text-xs text-white/50">Cargo weight (kg)</span>
              <input
                type="number"
                min={1}
                max={listing.max_weight_kg}
                value={weight}
                onChange={(e) => setWeight(e.target.value === "" ? "" : Number(e.target.value))}
                placeholder={`max ${listing.max_weight_kg}`}
                className="rounded-md border border-glass-border bg-canvas px-3 py-2 text-sm text-white placeholder-white/30 focus:border-purple-plasma/50 focus:outline-none"
              />
            </label>
            <label className="flex flex-col gap-1">
              <span className="text-xs text-white/50">Pickup time</span>
              <input
                type="datetime-local"
                value={when}
                onChange={(e) => setWhen(e.target.value)}
                className="rounded-md border border-glass-border bg-canvas px-3 py-2 text-sm text-white focus:border-purple-plasma/50 focus:outline-none"
              />
            </label>
          </div>
        </div>

        <div className="rounded-md border border-cyan-neon/20 bg-cyan-surface/50 px-3 py-2 mt-4 text-2xs font-mono text-cyan-neon/80 flex items-start gap-2">
          <Package size={12} className="flex-shrink-0 mt-0.5" />
          <span>Booking creates a shipment in your orders list with an AWB. The carrier has {listing.response_window_mins} minutes to accept.</span>
        </div>

        <div className="flex items-center justify-end gap-2 mt-5">
          <button
            onClick={onCancel}
            className="rounded-md border border-glass-border px-4 py-2 text-xs text-white/60 hover:text-white hover:border-glass-border-bright transition-colors"
          >
            Cancel
          </button>
          <button
            disabled={!canSubmit || busy}
            onClick={submit}
            className={`flex items-center gap-1.5 rounded-md border px-4 py-2 text-xs transition-all ${
              canSubmit && !busy
                ? "border-purple-plasma/50 bg-purple-surface text-purple-plasma hover:bg-purple-plasma/20 hover:border-purple-plasma"
                : "border-glass-border text-white/20 cursor-not-allowed"
            }`}
          >
            <Check size={12} />
            {busy ? "Booking…" : "Confirm Booking"}
          </button>
        </div>
      </motion.div>
    </div>
  );
}
