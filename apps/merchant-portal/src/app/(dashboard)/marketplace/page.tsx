"use client";
/**
 * Merchant Portal — Marketplace Discovery
 *
 * Merchant-side view of carrier vehicle listings (ADR-0013). The merchant
 * browses idle capacity, books a vehicle for one of their own shipments, and
 * tracks / cancels bookings. All data is live from the carrier service
 * (`/v1/marketplace/*`); the page polls my-bookings every 30 s.
 *
 * Deep links: ?awb highlights a booking, ?listing opens the booking dialog,
 * ?partner pre-fills the search box.
 */

import { useCallback, useEffect, useMemo, useRef, useState, Suspense } from "react";
import { usePathname, useRouter, useSearchParams } from "next/navigation";
import Link from "next/link";
import { motion } from "framer-motion";
import {
  Store, Truck, Clock, Search, X, Check, ExternalLink,
  Gauge, Package, MapPin, AlertTriangle,
  Receipt as ReceiptIcon,
} from "lucide-react";
import { variants } from "@/lib/design-system/tokens";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge, type BadgeVariant } from "@/components/ui/neon-badge";
import {
  fetchAvailableListings,
  fetchMyBookings,
  fetchBookableShipments,
  computeMarketplaceStats,
  createBooking,
  cancelBooking,
  SIZE_CLASS_LABEL,
  formatCentsPhp,
  type VehicleListing,
  type MarketplaceBooking,
  type BookableShipment,
  type BookingPaymentMethod,
  type SizeClass,
  type ListingStatus,
  type BookingStatus,
} from "@/lib/api/marketplace";
import { safeCheckoutUrl } from "@/lib/api/checkout-url";
import { useDialogA11y } from "@/lib/hooks/use-dialog-a11y";
import { ReceiptModal } from "@/components/marketplace/ReceiptModal";

// ── Status → badge mapping ────────────────────────────────────────────────────

const LISTING_STATUS: Record<ListingStatus, { label: string; variant: BadgeVariant }> = {
  active:  { label: "Available", variant: "green" },
  booked:  { label: "Booked",    variant: "amber" },
  paused:  { label: "Paused",    variant: "muted" },
  expired: { label: "Expired",   variant: "muted" },
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

const SIZE_FILTERS: Array<{ label: string; value: SizeClass | "all" }> = [
  { label: "All",        value: "all"       },
  { label: "Motorcycle", value: "motorcycle" },
  { label: "Sedan",      value: "sedan"     },
  { label: "Cargo van",  value: "van"       },
  { label: "L300",       value: "l300"      },
  { label: "6-wheeler",  value: "6wheeler"  },
  { label: "10-wheeler", value: "10wheeler" },
  { label: "Trailer",    value: "trailer"   },
];

type Tab = "browse" | "bookings";

const POLL_MS = 30_000;

function idleCountdown(idleUntil: string): string {
  const diffMs = new Date(idleUntil).getTime() - Date.now();
  if (diffMs <= 0) return "Expired";
  const hours = Math.floor(diffMs / 3_600_000);
  const mins  = Math.floor((diffMs % 3_600_000) / 60_000);
  if (hours > 0) return `${hours}h ${mins}m`;
  return `${mins}m`;
}

function errMsg(e: unknown, fallback: string): string {
  return e instanceof Error && e.message ? e.message : fallback;
}

const fmtShort = (iso: string) =>
  new Date(iso).toLocaleString("en-PH", { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });

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
  const router = useRouter();
  const pathname = usePathname();
  const qpAwb     = qp?.get("awb");
  const qpListing = qp?.get("listing");
  const qpPartner = qp?.get("partner");

  const [listings, setListings] = useState<VehicleListing[]>([]);
  const [bookings, setBookings] = useState<MarketplaceBooking[]>([]);
  const [loading,  setLoading]  = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);

  const [tab,        setTab]        = useState<Tab>(qpAwb ? "bookings" : "browse");
  const [sizeFilter, setSizeFilter] = useState<SizeClass | "all">("all");
  const [search,     setSearch]     = useState(qpPartner ? qpPartner : "");
  const [bookingFor, setBookingFor] = useState<VehicleListing | null>(null);
  const [cancelling, setCancelling] = useState<string | null>(null);   // booking id mid-request
  const [confirmCancel, setConfirmCancel] = useState<string | null>(null);
  const [receiptFor, setReceiptFor] = useState<MarketplaceBooking | null>(null);

  const stats = useMemo(() => computeMarketplaceStats(listings, bookings), [listings, bookings]);

  const refresh = useCallback(async () => {
    try {
      const [l, b] = await Promise.all([fetchAvailableListings(), fetchMyBookings()]);
      setListings(l);
      setBookings(b);
      setLoadError(null);
    } catch (e) {
      setLoadError(errMsg(e, "Could not load the marketplace."));
    } finally {
      setLoading(false);
    }
  }, []);

  // Initial load + 30 s poll so a carrier's accept / reject shows up without a reload.
  useEffect(() => {
    refresh();
    const id = setInterval(refresh, POLL_MS);
    return () => clearInterval(id);
  }, [refresh]);

  // Auto-open the booking dialog for ?listing=<id>, at most once per distinct
  // id: the 30 s poll replaces `listings`, and closing the dialog must not
  // bring it back. The param is dropped from the URL once consumed.
  const consumedListing = useRef<string | null>(null);
  useEffect(() => {
    if (!qpListing || listings.length === 0) return;
    if (consumedListing.current === qpListing) return;
    consumedListing.current = qpListing;
    const l = listings.find((x) => x.id === qpListing);
    if (l && l.status === "active") setBookingFor(l);
    const rest = new URLSearchParams(qp?.toString() ?? "");
    rest.delete("listing");
    const q = rest.toString();
    router.replace(q ? `${pathname}?${q}` : pathname);
  }, [qpListing, listings, qp, router, pathname]);

  const listingById = useMemo(() => new Map(listings.map((l) => [l.id, l])), [listings]);

  const visibleListings = useMemo(() => {
    const q = search.trim().toLowerCase();
    return listings.filter((l) => {
      if (sizeFilter !== "all" && l.size_class !== sizeFilter) return false;
      if (q) {
        const hay = `${l.service_area_label} ${l.vehicle_plate} ${SIZE_CLASS_LABEL[l.size_class]}`.toLowerCase();
        if (!hay.includes(q)) return false;
      }
      return true;
    });
  }, [listings, sizeFilter, search]);

  async function handleBook(input: Parameters<typeof createBooking>[0]) {
    // Throws on failure; the dialog shows the server's message and stays open.
    const { checkout_url } = await createBooking(input);
    setBookingFor(null);
    setTab("bookings");
    setActionError(null);
    refresh();
    if (checkout_url) {
      // Online payment: the booking is only shown to a carrier after the hold
      // lands. The URL comes from the server, so only follow a valid https one.
      const safe = safeCheckoutUrl(checkout_url);
      if (safe) {
        window.location.assign(safe);
      } else {
        setActionError(
          "Payment link is invalid. Your booking was created but is not paid; contact support.",
        );
      }
    }
  }

  async function handleCancel(id: string) {
    setCancelling(id);
    setActionError(null);
    try {
      await cancelBooking(id);
      setConfirmCancel(null);
      await refresh();
    } catch (e) {
      setActionError(errMsg(e, "Could not cancel the booking."));
      setConfirmCancel(null);
    } finally {
      setCancelling(null);
    }
  }

  const activeCount = bookings.filter(
    (b) => b.status === "pending" || b.status === "accepted" || b.status === "in_transit",
  ).length;

  return (
    <motion.div
      variants={variants.staggerContainer}
      initial="hidden"
      animate="visible"
      className="flex flex-col gap-5 p-4 sm:p-6"
    >
      {/* Deep-link banner */}
      {qpAwb && (
        <motion.div variants={variants.fadeInUp}>
          <div className="flex items-center justify-between rounded-xl border border-cyan-neon/30 bg-cyan-surface px-4 py-2.5">
            <div className="flex items-center gap-2.5 min-w-0">
              <ExternalLink size={14} className="flex-shrink-0 text-cyan-neon" aria-hidden="true" />
              <p className="text-xs text-white/80 truncate">
                Linked from tracking · <span className="font-mono text-cyan-neon">{qpAwb}</span>
              </p>
            </div>
            <Link href="/marketplace" aria-label="Dismiss" className="text-xs text-white/40 hover:text-white">
              <X size={14} aria-hidden="true" />
            </Link>
          </div>
        </motion.div>
      )}

      {/* Header */}
      <motion.div variants={variants.fadeInUp} className="flex items-center justify-between">
        <div>
          <h1 className="font-heading text-xl sm:text-2xl font-bold text-white flex items-center gap-2">
            <Store size={22} className="text-purple-plasma" aria-hidden="true" />
            Marketplace
          </h1>
          <p className="text-sm text-white/40 font-mono mt-0.5">
            Browse idle capacity · book a vehicle for a shipment · track bookings
          </p>
        </div>
      </motion.div>

      {loadError && (
        <div role="alert" className="flex items-start gap-2 rounded-xl border border-red-signal/40 bg-red-signal/10 px-4 py-2.5 text-xs text-red-signal">
          <AlertTriangle size={14} className="mt-0.5 flex-shrink-0" aria-hidden="true" />
          <span className="min-w-0 break-words">{loadError}</span>
        </div>
      )}
      {actionError && (
        <div role="alert" className="flex items-start gap-2 rounded-xl border border-red-signal/40 bg-red-signal/10 px-4 py-2.5 text-xs text-red-signal">
          <AlertTriangle size={14} className="mt-0.5 flex-shrink-0" aria-hidden="true" />
          <span className="min-w-0 break-words">{actionError}</span>
        </div>
      )}

      {/* KPI strip */}
      <motion.div variants={variants.fadeInUp} className="grid grid-cols-2 md:grid-cols-4 gap-3">
        {[
          { label: "Available Now",      value: loading ? "—" : stats.available_now,                       icon: Truck,   tint: "text-green-signal"  },
          { label: "Avg Rate / km",      value: loading ? "—" : formatCentsPhp(stats.avg_rate_per_km),      icon: Gauge,   tint: "text-cyan-neon"     },
          { label: "Carriers Reachable", value: loading ? "—" : stats.carriers_reachable,                   icon: Store,   tint: "text-purple-plasma" },
          { label: "My Active Bookings", value: loading ? "—" : stats.my_bookings_active,                   icon: Package, tint: "text-amber-signal"  },
        ].map((k) => {
          const Icon = k.icon;
          return (
            <div key={k.label} className="rounded-xl border border-glass-border bg-glass-100 px-4 py-3">
              <div className="flex items-start justify-between gap-2">
                <p className="text-xs text-white/40 font-mono">{k.label}</p>
                <Icon size={14} className={`flex-shrink-0 ${k.tint}`} aria-hidden="true" />
              </div>
              <p className={`font-heading text-xl sm:text-2xl font-bold ${k.tint} mt-1`}>{k.value}</p>
            </div>
          );
        })}
      </motion.div>

      {/* Tab switcher */}
      <motion.div variants={variants.fadeInUp}>
        <div className="flex gap-1 border-b border-glass-border overflow-x-auto" role="tablist">
          <TabButton active={tab === "browse"} onClick={() => setTab("browse")}>
            Browse Listings
          </TabButton>
          <TabButton active={tab === "bookings"} onClick={() => setTab("bookings")}>
            My Bookings ({activeCount})
          </TabButton>
        </div>
      </motion.div>

      {/* Filter + search */}
      {tab === "browse" && (
        <motion.div variants={variants.fadeInUp}>
          <GlassCard>
            <div className="flex flex-col md:flex-row md:items-center gap-3">
              <div className="relative flex-1">
                <Search size={13} className="absolute left-3 top-1/2 -translate-y-1/2 text-white/30" aria-hidden="true" />
                <input
                  type="text"
                  value={search}
                  onChange={(e) => setSearch(e.target.value)}
                  aria-label="Search listings"
                  placeholder="Service area, plate, or vehicle class..."
                  className="w-full rounded-lg border border-glass-border bg-canvas-100 pl-9 pr-3 py-2 text-sm text-white placeholder-white/30 focus:border-purple-plasma/50 focus:outline-none"
                />
              </div>
              <div className="flex gap-1.5 flex-wrap">
                {SIZE_FILTERS.map((f) => (
                  <button
                    key={f.value}
                    onClick={() => setSizeFilter(f.value)}
                    aria-pressed={sizeFilter === f.value}
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
                <table className="w-full min-w-[640px] text-sm">
                  <thead className="text-xs text-white/40 uppercase tracking-wide border-b border-glass-border">
                    <tr>
                      <th className="text-left font-medium py-2.5 pr-4">Vehicle</th>
                      <th className="text-left font-medium py-2.5 pr-4">Service Area</th>
                      <th className="text-left font-medium py-2.5 pr-4">Base + /km</th>
                      <th className="text-left font-medium py-2.5 pr-4">Idle Left</th>
                      <th className="text-left font-medium py-2.5 pr-4">Status</th>
                      <th className="text-right font-medium py-2.5">Action</th>
                    </tr>
                  </thead>
                  <tbody>
                    {visibleListings.map((l) => {
                      const { label: statusLabel, variant } = LISTING_STATUS[l.status];
                      const canBook = l.status === "active";
                      return (
                        <tr key={l.id} className="border-b border-glass-border/50 hover:bg-glass-100/40 transition-colors">
                          <td className="py-3 pr-4">
                            <div className="flex flex-col gap-0.5">
                              <span className="text-white text-sm">{SIZE_CLASS_LABEL[l.size_class]}</span>
                              <span className="text-2xs font-mono text-white/30">
                                {l.vehicle_plate} · {l.max_weight_kg.toLocaleString()} kg
                                {l.max_volume_m3 != null ? ` · ${l.max_volume_m3} m³` : ""}
                              </span>
                            </div>
                          </td>
                          <td className="py-3 pr-4 text-xs text-white/70">{l.service_area_label}</td>
                          <td className="py-3 pr-4 font-mono text-xs text-white/80 whitespace-nowrap">
                            {formatCentsPhp(l.base_price_cents)}
                            <span className="text-white/30"> + </span>
                            {formatCentsPhp(l.per_km_cents)}/km
                          </td>
                          <td className="py-3 pr-4 font-mono text-xs text-amber-signal whitespace-nowrap">
                            <span className="inline-flex items-center gap-1">
                              <Clock size={11} aria-hidden="true" />
                              {idleCountdown(l.idle_until)}
                            </span>
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
                <table className="w-full min-w-[820px] text-sm">
                  <thead className="text-xs text-white/40 uppercase tracking-wide border-b border-glass-border">
                    <tr>
                      <th className="text-left font-medium py-2.5 pr-4">AWB</th>
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
                      const listing = listingById.get(b.listing_id);
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
                          <td className="py-3 pr-4 text-xs text-white/70">
                            <div className="flex items-center gap-1">
                              <MapPin size={10} className="flex-shrink-0 text-white/30" aria-hidden="true" />
                              <span>
                                {b.pickup_label}
                                <span className="text-white/20"> → </span>
                                {b.dropoff_label}
                              </span>
                            </div>
                          </td>
                          <td className="py-3 pr-4 text-xs text-white/70">
                            {listing ? `${SIZE_CLASS_LABEL[listing.size_class]} · ` : ""}{b.cargo_weight_kg} kg
                          </td>
                          <td className="py-3 pr-4 font-mono text-xs text-white/80">
                            {formatCentsPhp(b.quoted_price_cents)}
                            <div className="text-2xs text-white/40">
                              {b.payment_method === "online" ? "Online" : "Invoice"} · {b.payment_status}
                            </div>
                          </td>
                          <td className="py-3 pr-4 font-mono text-xs text-white/60">
                            {b.picked_up_at ? (
                              <>
                                <span className="text-green-signal">Picked up {fmtShort(b.picked_up_at)}</span>
                                {b.picked_up_by && (
                                  <div className="mt-0.5 text-2xs text-white/40">{b.picked_up_by}</div>
                                )}
                              </>
                            ) : (
                              fmtShort(b.pickup_at)
                            )}
                          </td>
                          <td className="py-3 pr-4">
                            <div className="flex flex-wrap items-center gap-2">
                              <NeonBadge variant={variant}>{label}</NeonBadge>
                              <button
                                onClick={() => setReceiptFor(b)}
                                className="inline-flex items-center gap-1 rounded-md border border-cyan-neon/40 bg-cyan-surface px-2 py-0.5 text-2xs font-mono text-cyan-neon transition-all hover:shadow-[0_0_8px_rgba(0,229,255,0.4)]"
                                aria-label={`View receipt for ${b.awb}`}
                              >
                                <ReceiptIcon size={10} aria-hidden="true" />
                                Receipt
                              </button>
                              {b.status === "pending" && (
                                confirmCancel === b.id ? (
                                  <span className="inline-flex items-center gap-1">
                                    <button
                                      onClick={() => handleCancel(b.id)}
                                      disabled={cancelling === b.id}
                                      className="rounded-md border border-red-signal/50 bg-red-signal/10 px-2 py-0.5 text-2xs font-mono text-red-signal hover:bg-red-signal/20 disabled:opacity-50"
                                    >
                                      {cancelling === b.id ? "Cancelling…" : "Confirm cancel"}
                                    </button>
                                    <button
                                      onClick={() => setConfirmCancel(null)}
                                      disabled={cancelling === b.id}
                                      className="rounded-md border border-glass-border px-2 py-0.5 text-2xs font-mono text-white/60 hover:text-white"
                                    >
                                      Keep
                                    </button>
                                  </span>
                                ) : (
                                  <button
                                    onClick={() => setConfirmCancel(b.id)}
                                    className="rounded-md border border-red-signal/40 px-2 py-0.5 text-2xs font-mono text-red-signal hover:bg-red-signal/10"
                                    aria-label={`Cancel booking ${b.awb}`}
                                  >
                                    Cancel
                                  </button>
                                )
                              )}
                              {b.status === "disputed" && (
                                // Plain <a> crosses the /merchant → /admin basePath boundary.
                                // Ops-escalation path: tenant-admin session required to load.
                                <a
                                  href={`/admin/marketplace?awb=${encodeURIComponent(b.awb)}&status=disputed`}
                                  className="inline-flex items-center gap-1 text-2xs font-mono text-red-signal hover:underline"
                                  title="Escalate to tenant ops"
                                >
                                  Escalate <ExternalLink size={10} aria-hidden="true" />
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

      {/* Booking dialog */}
      {bookingFor && (
        <BookingDialog
          listing={bookingFor}
          onCancel={() => setBookingFor(null)}
          onSubmit={handleBook}
        />
      )}

      {/* Booking receipt (read-only summary of the booking) */}
      <ReceiptModal
        open={receiptFor !== null}
        onClose={() => setReceiptFor(null)}
        booking={receiptFor}
      />
    </motion.div>
  );
}

// ── Tab button ────────────────────────────────────────────────────────────────

function TabButton({ active, onClick, children }: { active: boolean; onClick: () => void; children: React.ReactNode }) {
  return (
    <button
      role="tab"
      aria-selected={active}
      onClick={onClick}
      className={`relative whitespace-nowrap px-4 py-2 text-sm font-medium transition-colors ${
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

// ── Booking dialog ────────────────────────────────────────────────────────────

const DIALOG_TITLE_ID = "book-vehicle-title";

function BookingDialog({
  listing,
  onCancel,
  onSubmit,
}: {
  listing: VehicleListing;
  onCancel: () => void;
  onSubmit: (input: Parameters<typeof createBooking>[0]) => Promise<void>;
}) {
  const dialogRef = useDialogA11y<HTMLDivElement>(true, onCancel);

  const [shipments,   setShipments]   = useState<BookableShipment[]>([]);
  const [shipLoading, setShipLoading] = useState(true);
  const [shipError,   setShipError]   = useState<string | null>(null);
  const [shipmentId,  setShipmentId]  = useState("");

  const [consumer, setConsumer] = useState("");
  const [phone,    setPhone]    = useState("");
  const [pickup,   setPickup]   = useState("");
  const [dropoff,  setDropoff]  = useState("");
  const [weight,   setWeight]   = useState<number | "">("");
  const [distance, setDistance] = useState<number | "">(0);
  const [when,     setWhen]     = useState("");
  const [payment,  setPayment]  = useState<BookingPaymentMethod>("invoice");
  const [busy,     setBusy]     = useState(false);
  const [error,    setError]    = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    fetchBookableShipments()
      .then((rows) => { if (!cancelled) setShipments(rows); })
      .catch((e) => { if (!cancelled) setShipError(errMsg(e, "Could not load your shipments.")); })
      .finally(() => { if (!cancelled) setShipLoading(false); });
    return () => { cancelled = true; };
  }, []);

  const shipment = shipments.find((s) => s.id === shipmentId) ?? null;

  function pickShipment(id: string) {
    setShipmentId(id);
    const s = shipments.find((x) => x.id === id);
    if (!s) return;
    // Prefill from the shipment where it has the value; the merchant can still edit.
    setConsumer(s.customer_name);
    setPhone(s.customer_phone);
    setPickup(s.pickup_label);
    setDropoff(s.dropoff_label);
    if (s.weight_kg !== null) setWeight(s.weight_kg);
  }

  const maxWeight = listing.max_weight_kg;
  const weightOk = typeof weight === "number" && weight > 0 && weight <= maxWeight;
  const canSubmit =
    !!shipment &&
    consumer.trim() !== "" &&
    pickup.trim() !== "" &&
    dropoff.trim() !== "" &&
    weightOk &&
    typeof distance === "number" && distance >= 0 &&
    when !== "";

  async function submit() {
    if (!canSubmit || !shipment) return;
    setBusy(true);
    setError(null);
    try {
      await onSubmit({
        listing_id:      listing.id,
        shipment_id:     shipment.id,
        awb:             shipment.awb,
        consumer_name:   consumer.trim(),
        ...(phone.trim() ? { consumer_phone: phone.trim() } : {}),
        pickup_label:    pickup.trim(),
        dropoff_label:   dropoff.trim(),
        cargo_weight_kg: weight as number,
        distance_km:     distance as number,
        pickup_at:       new Date(when).toISOString(),
        payment_method:  payment,
      });
    } catch (e) {
      setError(errMsg(e, "Booking failed. Please try again."));
    } finally {
      setBusy(false);
    }
  }

  const inputCls = "rounded-md border border-glass-border bg-canvas px-3 py-2 text-sm text-white placeholder-white/30 focus:border-purple-plasma/50 focus:outline-none w-full";

  return (
    <div className="fixed inset-0 z-50 flex items-end md:items-center justify-center bg-black/70 backdrop-blur-sm">
      <motion.div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby={DIALOG_TITLE_ID}
        tabIndex={-1}
        initial={{ y: 40, opacity: 0 }}
        animate={{ y: 0,  opacity: 1 }}
        transition={{ duration: 0.25, ease: [0.16, 1, 0.3, 1] }}
        className="w-full md:w-[560px] rounded-t-2xl md:rounded-2xl border border-glass-border bg-canvas-100 p-4 sm:p-6 max-h-[92vh] overflow-y-auto outline-none"
      >
        <div className="flex items-start justify-between mb-4 gap-3">
          <div className="min-w-0">
            <h2 id={DIALOG_TITLE_ID} className="font-heading text-lg font-bold text-white">Book Vehicle</h2>
            <p className="text-xs text-white/50 font-mono mt-0.5 break-words">
              {SIZE_CLASS_LABEL[listing.size_class]} · {listing.vehicle_plate} · {listing.service_area_label}
            </p>
          </div>
          <button onClick={onCancel} aria-label="Close" className="flex-shrink-0 text-white/40 hover:text-white">
            <X size={16} aria-hidden="true" />
          </button>
        </div>

        <div className="rounded-lg border border-glass-border bg-glass-100 p-3 mb-4 text-xs text-white/60 font-mono">
          <p>
            Up to {maxWeight.toLocaleString()} kg
            {listing.max_volume_m3 != null ? ` · ${listing.max_volume_m3} m³` : ""}
          </p>
          <p className="mt-1">
            {formatCentsPhp(listing.base_price_cents)} base
            <span className="text-white/30"> + </span>
            {formatCentsPhp(listing.per_km_cents)}/km
            <span className="text-white/30"> · </span>
            carrier replies within {listing.carrier_response_window_mins} min
          </p>
        </div>

        <div className="flex flex-col gap-3">
          <label className="flex flex-col gap-1">
            <span className="text-xs text-white/50">Shipment</span>
            <select
              value={shipmentId}
              onChange={(e) => pickShipment(e.target.value)}
              disabled={shipLoading}
              className={inputCls}
            >
              <option value="">
                {shipLoading ? "Loading your shipments…" : shipments.length === 0 ? "No open shipments" : "Select a shipment…"}
              </option>
              {shipments.map((s) => (
                <option key={s.id} value={s.id}>
                  {s.awb}{s.customer_name ? ` · ${s.customer_name}` : ""}
                </option>
              ))}
            </select>
            {shipError && <span role="alert" className="text-2xs text-red-signal">{shipError}</span>}
            {!shipLoading && !shipError && shipments.length === 0 && (
              <span className="text-2xs text-white/40">
                Create a shipment first, then come back to book a vehicle for it.
              </span>
            )}
          </label>

          <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
            <label className="flex flex-col gap-1">
              <span className="text-xs text-white/50">Consignee name</span>
              <input value={consumer} onChange={(e) => setConsumer(e.target.value)} className={inputCls} />
            </label>
            <label className="flex flex-col gap-1">
              <span className="text-xs text-white/50">Consignee phone <span className="text-white/30">(optional)</span></span>
              <input value={phone} onChange={(e) => setPhone(e.target.value)} inputMode="tel" className={inputCls} />
            </label>
          </div>

          <label className="flex flex-col gap-1">
            <span className="text-xs text-white/50">Pickup address</span>
            <input
              value={pickup}
              onChange={(e) => setPickup(e.target.value)}
              placeholder="e.g., Pasig Warehouse, Ortigas Ave"
              className={inputCls}
            />
          </label>
          <label className="flex flex-col gap-1">
            <span className="text-xs text-white/50">Drop-off address</span>
            <input
              value={dropoff}
              onChange={(e) => setDropoff(e.target.value)}
              placeholder="e.g., Batangas Industrial Park"
              className={inputCls}
            />
          </label>

          <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
            <label className="flex flex-col gap-1">
              <span className="text-xs text-white/50">Cargo weight (kg)</span>
              <input
                type="number"
                min={1}
                max={maxWeight}
                value={weight}
                onChange={(e) => setWeight(e.target.value === "" ? "" : Number(e.target.value))}
                placeholder={`max ${maxWeight}`}
                aria-invalid={typeof weight === "number" && weight > maxWeight}
                className={inputCls}
              />
              {typeof weight === "number" && weight > maxWeight && (
                <span role="alert" className="text-2xs text-red-signal">Over this vehicle&apos;s {maxWeight} kg limit.</span>
              )}
            </label>
            <label className="flex flex-col gap-1">
              <span className="text-xs text-white/50">Distance (km) <span className="text-white/30">(prices the trip)</span></span>
              <input
                type="number"
                min={0}
                step="0.1"
                value={distance}
                onChange={(e) => setDistance(e.target.value === "" ? "" : Number(e.target.value))}
                className={inputCls}
              />
            </label>
          </div>

          <label className="flex flex-col gap-1">
            <span className="text-xs text-white/50">Pickup time</span>
            <input type="datetime-local" value={when} onChange={(e) => setWhen(e.target.value)} className={inputCls} />
          </label>

          <fieldset className="flex flex-col gap-1.5">
            <legend className="text-xs text-white/50 mb-1">Payment</legend>
            <div className="grid grid-cols-1 sm:grid-cols-2 gap-2">
              {([
                { value: "invoice", title: "Bill my invoice", hint: "Charged on your normal invoice run" },
                { value: "online",  title: "Pay online now",  hint: "You'll be sent to a secure checkout" },
              ] as const).map((o) => (
                <label
                  key={o.value}
                  className={`flex cursor-pointer items-start gap-2 rounded-lg border px-3 py-2 text-xs ${
                    payment === o.value ? "border-purple-plasma/60 bg-purple-surface" : "border-glass-border"
                  }`}
                >
                  <input
                    type="radio"
                    name="payment_method"
                    value={o.value}
                    checked={payment === o.value}
                    onChange={() => setPayment(o.value)}
                    className="mt-0.5"
                  />
                  <span>
                    <span className="block text-white">{o.title}</span>
                    <span className="block text-2xs text-white/40">{o.hint}</span>
                  </span>
                </label>
              ))}
            </div>
          </fieldset>
        </div>

        <div className="rounded-md border border-cyan-neon/20 bg-cyan-surface/50 px-3 py-2 mt-4 text-2xs font-mono text-cyan-neon/80 flex items-start gap-2">
          <Package size={12} className="flex-shrink-0 mt-0.5" aria-hidden="true" />
          <span>
            The price is calculated by the carrier service from the rate card and your distance.
            The carrier has {listing.carrier_response_window_mins} minutes to accept.
          </span>
        </div>

        {error && (
          <div role="alert" className="mt-3 flex items-start gap-2 rounded-md border border-red-signal/40 bg-red-signal/10 px-3 py-2 text-xs text-red-signal">
            <AlertTriangle size={13} className="mt-0.5 flex-shrink-0" aria-hidden="true" />
            <span className="min-w-0 break-words">{error}</span>
          </div>
        )}

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
            <Check size={12} aria-hidden="true" />
            {busy ? "Booking…" : payment === "online" ? "Book & pay" : "Confirm Booking"}
          </button>
        </div>
      </motion.div>
    </div>
  );
}
