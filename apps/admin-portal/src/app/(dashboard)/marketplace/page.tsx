"use client";

/**
 * Admin Portal — Marketplace Oversight.
 *
 * Tenant-scoped view across every partner's marketplace listings and bookings
 * (ADR-0013 Marketplace Discovery addendum). The tenant_admin sees all rows
 * via `scope=tenant` in the session GUC — no partner filter applied server-side.
 *
 * Purpose: catch anomalies (disputed bookings, underpriced listings, idle
 * fleet clusters), cross-partner pricing visibility, and marketplace GMV
 * tracking. Read-only — mutation flows are owned by the partner portal.
 */

import { useCallback, useEffect, useMemo, useState, Suspense } from "react";
import { useSearchParams } from "next/navigation";
import { useRosterEvents } from "@/hooks/useRosterEvents";
import {
  Truck,
  Package,
  DollarSign,
  Activity,
  Search,
  Clock,
  MapPin,
  Gauge,
  Star,
  Users,
  Zap,
  ExternalLink,
  Map as MapIcon,
  Building2,
  User as UserIcon,
  X,
  Receipt as ReceiptIcon,
} from "lucide-react";

import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge, type BadgeVariant } from "@/components/ui/neon-badge";
import { cn } from "@/lib/design-system/cn";
import {
  fetchAllListings,
  fetchAllBookings,
  fetchMarketplaceStats,
  formatCentsPhp,
  SIZE_CLASS_LABEL,
  subscribeToMarketplaceUpdates,
  type AdminListing,
  type AdminBooking,
  type BookingStatus,
  type ListingStatus,
  type MarketplaceStats,
  type PartnerType,
} from "@/lib/api/marketplace";
import { findReceiptByBookingId, type BusReceipt } from "@/lib/api/marketplace-bus";
import { ReceiptModal, type ReceiptModalBooking } from "@/components/marketplace/ReceiptModal";
import { authFetch } from "@/lib/auth/auth-fetch";

// ── Status styling ────────────────────────────────────────────────────────────

const LISTING_STATUS_VARIANT: Record<ListingStatus, BadgeVariant> = {
  active:  "green",
  booked:  "cyan",
  paused:  "muted",
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

const PARTNER_TYPE_VARIANT: Record<PartnerType, BadgeVariant> = {
  alliance:    "purple",
  marketplace: "cyan",
};

// ── Helpers ───────────────────────────────────────────────────────────────────

function fmtRelative(iso: string): string {
  const diff = new Date(iso).getTime() - Date.now();
  const absMin = Math.round(Math.abs(diff) / 60_000);
  if (absMin < 1)  return "just now";
  if (absMin < 60) return diff < 0 ? `${absMin}m ago` : `in ${absMin}m`;
  const h = Math.round(absMin / 60);
  if (h < 24) return diff < 0 ? `${h}h ago` : `in ${h}h`;
  return diff < 0 ? `${Math.round(h / 24)}d ago` : `in ${Math.round(h / 24)}d`;
}

function fmtIdleWindow(untilISO: string): string {
  const ms = new Date(untilISO).getTime() - Date.now();
  if (ms < 0) return "Expired";
  const h = Math.round(ms / 3_600_000);
  return h < 1 ? "< 1h left" : `${h}h left`;
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
  glow: "purple" | "cyan" | "green" | "amber";
  hint?: string;
}) {
  const glowShadow = {
    purple: "0 0 12px rgba(168,85,247,0.3)",
    cyan:   "0 0 12px rgba(0,229,255,0.3)",
    green:  "0 0 12px rgba(0,255,136,0.3)",
    amber:  "0 0 12px rgba(255,171,0,0.3)",
  }[glow];

  return (
    <GlassCard glow={glow} accent size="sm">
      <div className="flex items-start justify-between">
        <div className="min-w-0">
          <p className="text-2xs font-medium uppercase tracking-wider text-white/40">
            {label}
          </p>
          <p
            className="mt-2 font-mono text-2xl font-bold text-white"
            style={{ textShadow: glowShadow }}
          >
            {value}
          </p>
          {hint && <p className="mt-1 text-xs text-white/40">{hint}</p>}
        </div>
        <div
          className={cn(
            "flex h-9 w-9 items-center justify-center rounded-lg border",
            glow === "purple" && "border-purple-plasma/30 bg-purple-surface text-purple-plasma",
            glow === "cyan"   && "border-cyan-neon/30     bg-cyan-surface    text-cyan-neon",
            glow === "green"  && "border-green-signal/30  bg-green-surface   text-green-signal",
            glow === "amber"  && "border-amber-signal/30  bg-amber-surface   text-amber-signal",
          )}
        >
          <Icon className="h-4 w-4" />
        </div>
      </div>
    </GlassCard>
  );
}

// ── Tabs ──────────────────────────────────────────────────────────────────────

type Tab = "listings" | "bookings";

// ── Main page ─────────────────────────────────────────────────────────────────

function AdminMarketplacePageInner() {
  const searchParams = useSearchParams();
  // Deep-link params (from /admin/carriers, /admin/dispatch, reverse links):
  //   ?partner=<id>   filter listings + bookings to one partner
  //   ?awb=<code>     jump to bookings tab, filter by AWB
  //   ?status=disputed  jump to bookings tab pre-filtered
  const qpPartner = searchParams.get("partner");
  const qpAwb     = searchParams.get("awb");
  const qpStatus  = searchParams.get("status");

  const [listings, setListings] = useState<AdminListing[]>([]);
  const [bookings, setBookings] = useState<AdminBooking[]>([]);
  const [stats, setStats]       = useState<MarketplaceStats | null>(null);
  const [loading, setLoading]   = useState(true);

  // Oversight view — receipts issued by partners on tenant bookings. Read-only.
  const [receiptsByBookingId, setReceiptsByBookingId] = useState<Record<string, BusReceipt>>({});
  const [receiptModal, setReceiptModal] = useState<
    { open: boolean; booking: ReceiptModalBooking | null; receipt: BusReceipt | null }
  >({ open: false, booking: null, receipt: null });
  const [tab, setTab]           = useState<Tab>(qpAwb || qpStatus ? "bookings" : "listings");
  const [search, setSearch]     = useState(qpAwb ?? "");
  const [partnerFilter, setPartnerFilter]     = useState<string>(qpPartner ?? "all");
  const [partnerTypeFilter, setPartnerTypeFilter] = useState<PartnerType | "all">("all");
  const [listingStatusFilter, setListingStatusFilter] = useState<ListingStatus | "all">("all");
  const [bookingStatusFilter, setBookingStatusFilter] = useState<BookingStatus | "all">(
    (qpStatus as BookingStatus) || "all",
  );

  const refresh = useCallback(async () => {
    const [l, b, s] = await Promise.all([
      fetchAllListings(),
      fetchAllBookings(),
      fetchMarketplaceStats(),
    ]);
    setListings(l);
    setBookings(b);
    setStats(s);
    setLoading(false);
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  // Live-ish refresh: bookings advance through pending→accepted→in_transit
  // driven by carrier driver status flips. The roster channel gives us the
  // high-signal nudge; the 20s poll catches cross-partner transitions + new
  // bookings that don't originate from a status event. Shorter than the
  // partner portal's 30s because tenant-wide oversight has more moving parts.
  useRosterEvents((event) => {
    if (event.type === "status_changed") refresh();
  });
  useEffect(() => {
    const id = setInterval(refresh, 20_000);
    const unsubscribe = subscribeToMarketplaceUpdates(() => refresh());
    return () => {
      clearInterval(id);
      unsubscribe();
    };
  }, [refresh]);

  // Hydrate receipts for tenant-wide visible bookings.
  useEffect(() => {
    if (bookings.length === 0) {
      setReceiptsByBookingId({});
      return;
    }
    const next: Record<string, BusReceipt> = {};
    for (const b of bookings) {
      const r = findReceiptByBookingId(b.id);
      if (r) next[b.id] = r;
    }
    setReceiptsByBookingId(next);
  }, [bookings]);

  function openReceiptFor(b: AdminBooking) {
    const receipt = receiptsByBookingId[b.id] ?? null;
    if (!receipt) return;
    const modalBooking: ReceiptModalBooking = {
      id:                    b.id,
      awb:                   b.awb,
      partner_display_name:  b.partner_display_name,
      merchant_display:      b.consumer_display,
      consumer_display:      b.consumer_display,
      pickup_label:          b.pickup_label,
      dropoff_label:         b.dropoff_label,
      pickup_at:             b.pickup_at,
      cargo_weight_kg:       b.cargo_weight_kg,
      quoted_price_cents:    b.quoted_price_cents,
      status:                b.status,
    };
    setReceiptModal({ open: true, booking: modalBooking, receipt });
  }

  // Distinct partners for filter dropdown
  const partnerOptions = useMemo(() => {
    const m = new Map<string, { id: string; name: string; type: PartnerType }>();
    listings.forEach((l) =>
      m.set(l.partner_id, {
        id: l.partner_id,
        name: l.partner_display_name,
        type: l.partner_type,
      }),
    );
    return Array.from(m.values()).sort((a, b) => a.name.localeCompare(b.name));
  }, [listings]);

  const filteredListings = useMemo(() => {
    return listings.filter((l) => {
      if (partnerFilter !== "all" && l.partner_id !== partnerFilter) return false;
      if (partnerTypeFilter !== "all" && l.partner_type !== partnerTypeFilter) return false;
      if (listingStatusFilter !== "all" && l.status !== listingStatusFilter) return false;
      if (!search) return true;
      const q = search.toLowerCase();
      return (
        l.vehicle_plate.toLowerCase().includes(q) ||
        l.partner_display_name.toLowerCase().includes(q) ||
        l.service_area_label.toLowerCase().includes(q) ||
        SIZE_CLASS_LABEL[l.size_class].toLowerCase().includes(q)
      );
    });
  }, [listings, search, partnerFilter, partnerTypeFilter, listingStatusFilter]);

  const filteredBookings = useMemo(() => {
    return bookings.filter((b) => {
      if (partnerFilter !== "all" && b.partner_id !== partnerFilter) return false;
      if (bookingStatusFilter !== "all" && b.status !== bookingStatusFilter) return false;
      if (!search) return true;
      const q = search.toLowerCase();
      return (
        b.awb.toLowerCase().includes(q) ||
        b.partner_display_name.toLowerCase().includes(q) ||
        b.consumer_display.toLowerCase().includes(q) ||
        b.pickup_label.toLowerCase().includes(q) ||
        b.dropoff_label.toLowerCase().includes(q)
      );
    });
  }, [bookings, search, partnerFilter, bookingStatusFilter]);

  const disputedCount = useMemo(
    () => bookings.filter((b) => b.status === "disputed").length,
    [bookings],
  );

  const partnerFilterLabel = useMemo(() => {
    if (partnerFilter === "all") return null;
    return partnerOptions.find((p) => p.id === partnerFilter)?.name ?? partnerFilter;
  }, [partnerFilter, partnerOptions]);

  const hasDeepLink = qpPartner || qpAwb || qpStatus;

  return (
    <div className="space-y-6">
      {/* ── Header row ────────────────────────────────────────────────────── */}
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div>
          <h1 className="font-heading text-2xl font-bold text-white">
            Marketplace Oversight
          </h1>
          <p className="mt-1 max-w-2xl text-sm text-white/50">
            Tenant-wide view of every partner&apos;s idle vehicle listings and consumer
            bookings. Read-only — listing mutations flow through partner portals.
          </p>
        </div>
        <div className="flex items-center gap-2 rounded-lg border border-purple-plasma/30 bg-purple-surface px-3 py-1.5">
          <Zap className="h-3 w-3 text-purple-plasma" />
          <span className="font-mono text-2xs uppercase tracking-wider text-purple-plasma">
            Scope · Tenant
          </span>
        </div>
      </div>

      {/* ── Deep-link banner ─────────────────────────────────────────────── */}
      {hasDeepLink && (
        <div className="flex flex-wrap items-center gap-2 rounded-lg border border-purple-plasma/25 bg-purple-plasma/5 px-3 py-2">
          <ExternalLink className="h-3 w-3 text-purple-plasma" />
          <span className="font-mono text-xs text-white/70">
            Filtered via deep-link:
            {qpPartner && partnerFilterLabel && (
              <>
                {" "}partner <span className="font-bold text-purple-plasma">{partnerFilterLabel}</span>
              </>
            )}
            {qpAwb && (
              <>
                {" "}AWB <span className="font-bold text-purple-plasma">{qpAwb}</span>
              </>
            )}
            {qpStatus && (
              <>
                {" "}status <span className="font-bold text-purple-plasma">{qpStatus}</span>
              </>
            )}
          </span>
          <a
            href="/admin/marketplace"
            title="Clear filter"
            className="ml-auto inline-flex h-5 w-5 items-center justify-center rounded-md text-white/40 transition-colors hover:text-white"
          >
            <X className="h-3 w-3" />
          </a>
        </div>
      )}

      {/* ── KPI strip ─────────────────────────────────────────────────────── */}
      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4 xl:grid-cols-6">
        <Kpi
          label="Live Listings"
          value={(stats?.active_listings ?? 0).toString()}
          icon={Truck}
          glow="green"
          hint={`${stats?.partners_participating ?? 0} partners`}
        />
        <Kpi
          label="Idle < 6h"
          value={(stats?.idle_vehicles_next_6h ?? 0).toString()}
          icon={Clock}
          glow="amber"
          hint="Network-wide"
        />
        <Kpi
          label="Bookings Today"
          value={(stats?.bookings_today ?? 0).toString()}
          icon={Package}
          glow="cyan"
        />
        <Kpi
          label="GMV Today"
          value={formatCentsPhp(stats?.gmv_today_cents ?? 0)}
          icon={DollarSign}
          glow="purple"
        />
        <Kpi
          label="Avg Match"
          value={`${stats?.avg_match_seconds ?? 0}s`}
          icon={Activity}
          glow="cyan"
          hint="Intent → accepted"
        />
        <Kpi
          label="Disputed"
          value={disputedCount.toString()}
          icon={Users}
          glow="amber"
          hint={disputedCount > 0 ? "Needs ops review" : "All clear"}
        />
      </div>

      {/* ── Tab switcher ──────────────────────────────────────────────────── */}
      <div className="flex items-center gap-1 border-b border-glass-border">
        <TabButton active={tab === "listings"} onClick={() => setTab("listings")}>
          Listings ({listings.length})
        </TabButton>
        <TabButton active={tab === "bookings"} onClick={() => setTab("bookings")}>
          Bookings ({bookings.length})
          {disputedCount > 0 && (
            <span className="ml-2 inline-flex h-4 min-w-[1rem] items-center justify-center rounded-full bg-red-signal px-1 font-mono text-2xs text-canvas">
              {disputedCount}
            </span>
          )}
        </TabButton>
      </div>

      {/* ── Shared filters row ────────────────────────────────────────────── */}
      <GlassCard size="sm" padding="none">
        <div className="flex flex-wrap items-center gap-3 px-5 py-3">
          <div className="relative">
            <Search className="pointer-events-none absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-white/40" />
            <input
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder={tab === "listings" ? "Search plate, partner, area…" : "Search AWB, partner, consumer…"}
              className="w-64 rounded-lg border border-glass-border bg-glass-100 py-1.5 pl-9 pr-3 text-xs text-white/80 outline-none transition-colors focus:border-purple-plasma/40 focus:bg-glass-200"
            />
          </div>

          <select
            value={partnerFilter}
            onChange={(e) => setPartnerFilter(e.target.value)}
            className="rounded-lg border border-glass-border bg-glass-100 px-3 py-1.5 text-xs text-white/80 outline-none focus:border-purple-plasma/40"
          >
            <option value="all" className="bg-canvas-100">All partners</option>
            {partnerOptions.map((p) => (
              <option key={p.id} value={p.id} className="bg-canvas-100">
                {p.name}
              </option>
            ))}
          </select>

          {tab === "listings" && (
            <>
              <select
                value={partnerTypeFilter}
                onChange={(e) => setPartnerTypeFilter(e.target.value as PartnerType | "all")}
                className="rounded-lg border border-glass-border bg-glass-100 px-3 py-1.5 text-xs text-white/80 outline-none focus:border-purple-plasma/40"
              >
                <option value="all" className="bg-canvas-100">All partner types</option>
                <option value="alliance" className="bg-canvas-100">Alliance</option>
                <option value="marketplace" className="bg-canvas-100">Marketplace</option>
              </select>
              <select
                value={listingStatusFilter}
                onChange={(e) => setListingStatusFilter(e.target.value as ListingStatus | "all")}
                className="rounded-lg border border-glass-border bg-glass-100 px-3 py-1.5 text-xs text-white/80 outline-none focus:border-purple-plasma/40"
              >
                <option value="all" className="bg-canvas-100">All statuses</option>
                <option value="active" className="bg-canvas-100">Active</option>
                <option value="booked" className="bg-canvas-100">Booked</option>
                <option value="paused" className="bg-canvas-100">Paused</option>
                <option value="expired" className="bg-canvas-100">Expired</option>
              </select>
            </>
          )}

          {tab === "bookings" && (
            <select
              value={bookingStatusFilter}
              onChange={(e) => setBookingStatusFilter(e.target.value as BookingStatus | "all")}
              className="rounded-lg border border-glass-border bg-glass-100 px-3 py-1.5 text-xs text-white/80 outline-none focus:border-purple-plasma/40"
            >
              <option value="all" className="bg-canvas-100">All statuses</option>
              <option value="pending" className="bg-canvas-100">Pending</option>
              <option value="accepted" className="bg-canvas-100">Accepted</option>
              <option value="in_transit" className="bg-canvas-100">In transit</option>
              <option value="delivered" className="bg-canvas-100">Delivered</option>
              <option value="disputed" className="bg-canvas-100">Disputed</option>
              <option value="cancelled" className="bg-canvas-100">Cancelled</option>
              <option value="rejected" className="bg-canvas-100">Rejected</option>
            </select>
          )}
        </div>
      </GlassCard>

      {/* Shadow Marketplace — pre-service capacity oversight.
          Cross-references each partner's active listings with their actual
          available drivers from driver-ops. Flags "phantom" partners with
          listings but no online drivers. See ADR-0014 — this view is what
          the real marketplace service will replace in Phase 2. */}
      {tab === "listings" && <ShadowMarketplacePanel listings={listings} />}

      {/* ── Content ───────────────────────────────────────────────────────── */}
      {tab === "listings" ? (
        <ListingsTable rows={filteredListings} total={listings.length} loading={loading} />
      ) : (
        <BookingsTable
          rows={filteredBookings}
          total={bookings.length}
          loading={loading}
          receipts={receiptsByBookingId}
          onOpenReceipt={openReceiptFor}
        />
      )}

      {/* Shipment receipt (view-only — tenant oversight) */}
      <ReceiptModal
        open={receiptModal.open}
        onClose={() => setReceiptModal({ open: false, booking: null, receipt: null })}
        booking={receiptModal.booking}
        receipt={receiptModal.receipt}
      />
    </div>
  );
}

export default function AdminMarketplacePage() {
  return (
    <Suspense fallback={null}>
      <AdminMarketplacePageInner />
    </Suspense>
  );
}

function partnerDeepLink(partnerId: string): string {
  // Cross-portal — partner-portal owns listing CRUD. Plain <a> preserves
  // the /partner basePath after the jump.
  return `/partner/marketplace?partner=${encodeURIComponent(partnerId)}`;
}

function dispatchDeepLink(shipmentId: string): string {
  // Dispatch console keys deep-links on ?order=<shipment_id>.
  return `/admin/dispatch?order=${encodeURIComponent(shipmentId)}`;
}

function merchantPortalDeepLink(awb: string): string {
  // Cross-portal — jumps into the merchant's own marketplace view with the
  // AWB surfaced for support/escalation context. Plain <a> preserves /merchant basePath.
  return `/merchant/marketplace?awb=${encodeURIComponent(awb)}`;
}

// ── Tab button ────────────────────────────────────────────────────────────────

function TabButton({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      onClick={onClick}
      className={cn(
        "relative px-4 py-2.5 font-mono text-2xs uppercase tracking-wider transition-colors",
        active ? "text-purple-plasma" : "text-white/40 hover:text-white/70",
      )}
    >
      {children}
      {active && (
        <span
          className="absolute inset-x-0 -bottom-px h-px"
          style={{
            background: "linear-gradient(90deg, transparent, #A855F7, transparent)",
            boxShadow: "0 0 8px rgba(168,85,247,0.5)",
          }}
        />
      )}
    </button>
  );
}

// ── Listings table ────────────────────────────────────────────────────────────

function ListingsTable({
  rows,
  total,
  loading,
}: {
  rows: AdminListing[];
  total: number;
  loading: boolean;
}) {
  return (
    <GlassCard size="sm" padding="none" accent glow="purple">
      <div className="overflow-x-auto">
        <table className="w-full min-w-[1120px] text-left text-sm">
          <thead>
            <tr className="border-b border-glass-border text-2xs font-mono uppercase tracking-wider text-white/40">
              <th className="px-5 py-3 font-medium">Partner</th>
              <th className="px-5 py-3 font-medium">Vehicle</th>
              <th className="px-5 py-3 font-medium">Capacity</th>
              <th className="px-5 py-3 font-medium">Pricing</th>
              <th className="px-5 py-3 font-medium">Area · Idle window</th>
              <th className="px-5 py-3 font-medium">Today</th>
              <th className="px-5 py-3 font-medium">Rating</th>
              <th className="px-5 py-3 font-medium">Status</th>
            </tr>
          </thead>
          <tbody>
            {loading ? (
              <tr>
                <td colSpan={8} className="px-5 py-10 text-center text-xs text-white/40">
                  Loading listings…
                </td>
              </tr>
            ) : rows.length === 0 ? (
              <tr>
                <td colSpan={8} className="px-5 py-10 text-center text-xs text-white/40">
                  No listings match your filters.
                </td>
              </tr>
            ) : (
              rows.map((l) => (
                <tr
                  key={l.id}
                  className="border-b border-glass-border/50 last:border-0 transition-colors hover:bg-glass-100"
                >
                  <td className="px-5 py-3">
                    <a
                      href={partnerDeepLink(l.partner_id)}
                      title="Open this partner's marketplace in Partner Portal"
                      className="group inline-flex items-center gap-1 text-xs font-medium text-white transition-colors hover:text-purple-plasma"
                    >
                      {l.partner_display_name}
                      <ExternalLink className="h-2.5 w-2.5 opacity-0 transition-opacity group-hover:opacity-100" />
                    </a>
                    <div className="mt-1">
                      <NeonBadge variant={PARTNER_TYPE_VARIANT[l.partner_type]}>
                        {l.partner_type}
                      </NeonBadge>
                    </div>
                  </td>
                  <td className="px-5 py-3">
                    <div className="flex items-center gap-3">
                      <div className="flex h-8 w-8 items-center justify-center rounded-lg border border-glass-border bg-glass-100">
                        <Truck className="h-3.5 w-3.5 text-white/60" />
                      </div>
                      <div>
                        <p className="font-mono text-xs font-medium text-white">
                          {l.vehicle_plate}
                        </p>
                        <p className="mt-0.5 text-2xs text-white/50">
                          {SIZE_CLASS_LABEL[l.size_class]}
                        </p>
                      </div>
                    </div>
                  </td>
                  <td className="px-5 py-3 text-xs text-white/70">
                    <div className="flex items-center gap-1.5">
                      <Gauge className="h-3 w-3 text-white/40" />
                      {l.max_weight_kg.toLocaleString()} kg
                    </div>
                  </td>
                  <td className="px-5 py-3">
                    <div className="font-mono text-xs text-white/80">
                      {formatCentsPhp(l.base_price_cents)}
                      <span className="text-white/40"> base</span>
                    </div>
                    <div className="mt-0.5 text-2xs text-white/50">
                      + {formatCentsPhp(l.per_km_cents)}/km
                    </div>
                  </td>
                  <td className="px-5 py-3">
                    <div className="flex items-center gap-1.5 text-xs text-white/70">
                      <MapPin className="h-3 w-3 text-white/40" />
                      {l.service_area_label}
                    </div>
                    <div className="mt-0.5 flex items-center gap-1 text-2xs text-white/40">
                      <Clock className="h-2.5 w-2.5" />
                      {fmtIdleWindow(l.idle_until)}
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
                    <div className="flex items-center gap-1 text-xs">
                      <Star className="h-3 w-3 fill-amber-signal text-amber-signal" />
                      <span className="font-mono text-white/80">{l.rating.toFixed(1)}</span>
                    </div>
                  </td>
                  <td className="px-5 py-3">
                    <NeonBadge variant={LISTING_STATUS_VARIANT[l.status]} dot>
                      {l.status}
                    </NeonBadge>
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>

      <div className="flex items-center justify-between border-t border-glass-border px-5 py-2.5 text-2xs text-white/40">
        <span>
          Showing {rows.length} of {total} listings across the network
        </span>
        <span className="font-mono">
          RLS · scope=tenant · cross-partner visibility enabled
        </span>
      </div>
    </GlassCard>
  );
}

// ── Bookings table ────────────────────────────────────────────────────────────

function BookingsTable({
  rows,
  total,
  loading,
  receipts,
  onOpenReceipt,
}: {
  rows: AdminBooking[];
  total: number;
  loading: boolean;
  receipts: Record<string, BusReceipt>;
  onOpenReceipt: (b: AdminBooking) => void;
}) {
  return (
    <GlassCard size="sm" padding="none" accent glow="cyan">
      <div className="overflow-x-auto">
        <table className="w-full min-w-[1120px] text-left text-sm">
          <thead>
            <tr className="border-b border-glass-border text-2xs font-mono uppercase tracking-wider text-white/40">
              <th className="px-5 py-3 font-medium">AWB</th>
              <th className="px-5 py-3 font-medium">Partner</th>
              <th className="px-5 py-3 font-medium">Booked by</th>
              <th className="px-5 py-3 font-medium">Route</th>
              <th className="px-5 py-3 font-medium">Cargo</th>
              <th className="px-5 py-3 font-medium">Quoted</th>
              <th className="px-5 py-3 font-medium">Pickup</th>
              <th className="px-5 py-3 font-medium">Status</th>
            </tr>
          </thead>
          <tbody>
            {loading ? (
              <tr>
                <td colSpan={8} className="px-5 py-10 text-center text-xs text-white/40">
                  Loading bookings…
                </td>
              </tr>
            ) : rows.length === 0 ? (
              <tr>
                <td colSpan={8} className="px-5 py-10 text-center text-xs text-white/40">
                  No bookings match your filters.
                </td>
              </tr>
            ) : (
              rows.map((b) => (
                <tr
                  key={b.id}
                  className={cn(
                    "border-b border-glass-border/50 last:border-0 transition-colors hover:bg-glass-100",
                    b.status === "disputed" && "bg-red-surface/30",
                  )}
                >
                  <td className="px-5 py-3">
                    <a
                      href={dispatchDeepLink(b.shipment_id)}
                      title="Open in Dispatch Console"
                      className="group inline-flex items-center gap-1 font-mono text-xs text-white transition-colors hover:text-cyan-neon"
                    >
                      {b.awb}
                      <MapIcon className="h-2.5 w-2.5 opacity-0 transition-opacity group-hover:opacity-100" />
                    </a>
                  </td>
                  <td className="px-5 py-3">
                    <a
                      href={partnerDeepLink(b.partner_id)}
                      title="Open this partner's marketplace in Partner Portal"
                      className="text-xs text-white/80 transition-colors hover:text-purple-plasma"
                    >
                      {b.partner_display_name}
                    </a>
                  </td>
                  <td className="px-5 py-3">
                    {b.merchant_type === "business" ? (
                      <a
                        href={merchantPortalDeepLink(b.awb)}
                        title="Open merchant view in Merchant Portal"
                        className="group inline-flex items-center gap-1.5 text-xs text-white/80 transition-colors hover:text-cyan-neon"
                      >
                        <Building2 className="h-3 w-3 text-purple-plasma flex-shrink-0" />
                        <span>{b.consumer_display}</span>
                        <MapIcon className="h-2.5 w-2.5 opacity-0 transition-opacity group-hover:opacity-100" />
                      </a>
                    ) : (
                      <span className="inline-flex items-center gap-1.5 text-xs text-white/80">
                        <UserIcon className="h-3 w-3 text-white/30 flex-shrink-0" />
                        <span>{b.consumer_display}</span>
                        <span className="text-2xs font-mono uppercase tracking-wider text-white/25">· walk-up</span>
                      </span>
                    )}
                  </td>
                  <td className="px-5 py-3 text-xs text-white/70">
                    {b.pickup_label}
                    <div className="mt-0.5 text-2xs text-white/40">→ {b.dropoff_label}</div>
                  </td>
                  <td className="px-5 py-3 text-xs text-white/70">
                    {SIZE_CLASS_LABEL[b.size_class]}
                    <div className="mt-0.5 text-2xs text-white/40">
                      {b.cargo_weight_kg.toLocaleString()} kg
                    </div>
                  </td>
                  <td className="px-5 py-3 font-mono text-xs text-green-signal">
                    {formatCentsPhp(b.quoted_price_cents)}
                  </td>
                  <td className="px-5 py-3 text-xs text-white/70">
                    {b.picked_up_at ? (
                      <>
                        <span className="text-green-signal">Picked up {fmtRelative(b.picked_up_at)}</span>
                        {b.picked_up_by && (
                          <div className="mt-0.5 font-mono text-2xs text-white/40">{b.picked_up_by}</div>
                        )}
                      </>
                    ) : (
                      fmtRelative(b.pickup_at)
                    )}
                  </td>
                  <td className="px-5 py-3">
                    <div className="flex items-center gap-2">
                      <NeonBadge variant={BOOKING_STATUS_VARIANT[b.status]} dot>
                        {b.status.replace("_", " ")}
                      </NeonBadge>
                      {receipts[b.id] && (
                        <button
                          onClick={() => onOpenReceipt(b)}
                          className="flex h-6 items-center gap-1 rounded-md border border-cyan-neon/40 bg-cyan-surface px-1.5 text-2xs font-mono text-cyan-neon transition-all hover:shadow-[0_0_8px_rgba(0,229,255,0.4)]"
                          title="View shipment receipt"
                        >
                          <ReceiptIcon className="h-3 w-3" />
                          Receipt
                        </button>
                      )}
                    </div>
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>

      <div className="flex items-center justify-between border-t border-glass-border px-5 py-2.5 text-2xs text-white/40">
        <span>Showing {rows.length} of {total} bookings</span>
        <span className="font-mono">Consumer identity masked until carrier accepts</span>
      </div>
    </GlassCard>
  );
}

// ── Shadow Marketplace panel ─────────────────────────────────────────────────
// Pre-service capacity oversight (ADR-0014 Phase 0). Cross-references each
// partner's active listings with live driver availability from driver-ops.
// This is the aggregation pattern the real marketplace service will embody.

type DriverOpsDriver = {
  id: string;
  user_id: string;
  first_name?: string;
  last_name?: string;
  status: string;                          // 'offline' | 'available' | 'en_route' | 'delivering' | 'returning' | 'on_break'
  is_active: boolean;
  carrier_id?: string | null;              // migration 0007; may be NULL for tenant-own drivers
  last_location_at?: string | null;
};

const API_BASE = process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:8000";
const HEARTBEAT_WINDOW_MIN = 5;            // Dead-Man's Switch threshold per ADR-0014

interface PartnerCapacity {
  partner_id: string;
  partner_display: string;
  active_listings: number;
  drivers_total: number;     // Drivers matched to this carrier (any status)
  drivers_online: number;    // status='available' AND last ping within HEARTBEAT_WINDOW_MIN
  risk: "ok" | "warn" | "critical";
}

function withinHeartbeat(last: string | null | undefined): boolean {
  if (!last) return false;
  const age = (Date.now() - new Date(last).getTime()) / 60_000;
  return age <= HEARTBEAT_WINDOW_MIN;
}

function ShadowMarketplacePanel({ listings }: { listings: AdminListing[] }) {
  const [drivers, setDrivers] = useState<DriverOpsDriver[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError]     = useState<string | null>(null);

  const loadDrivers = useCallback(async () => {
    setError(null);
    try {
      const res = await authFetch(`${API_BASE}/v1/drivers`);
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const json = await res.json() as { drivers?: DriverOpsDriver[] } | DriverOpsDriver[];
      // driver-ops wraps in { drivers: [...] } but some endpoints return a bare array
      const list = Array.isArray(json) ? json : (json.drivers ?? []);
      setDrivers(list);
    } catch (e) {
      const err = e as { message?: string };
      setError(err?.message ?? "Failed to load drivers");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { loadDrivers(); }, [loadDrivers]);

  // 60s poll — matches DMS tick cadence described in ADR-0014.
  useEffect(() => {
    const id = setInterval(loadDrivers, 60_000);
    return () => clearInterval(id);
  }, [loadDrivers]);

  const capacity: PartnerCapacity[] = useMemo(() => {
    // Group drivers by carrier_id once for O(1) lookup per partner.
    const byCarrier = new Map<string, DriverOpsDriver[]>();
    for (const d of drivers) {
      const k = d.carrier_id ?? "_unlinked";
      const arr = byCarrier.get(k) ?? [];
      arr.push(d);
      byCarrier.set(k, arr);
    }

    // Build a per-partner row for every partner that appears in listings.
    // (Partners with zero listings aren't shown — this is a capacity sanity
    // check, not a full partner roster.)
    const byPartner = new Map<string, { display: string; active_listings: number }>();
    for (const l of listings) {
      if (l.status !== "available") continue;
      const existing = byPartner.get(l.partner_id);
      if (existing) existing.active_listings += 1;
      else byPartner.set(l.partner_id, { display: l.partner_display_name, active_listings: 1 });
    }

    const rows: PartnerCapacity[] = [];
    byPartner.forEach((v, partner_id) => {
      const ds = byCarrier.get(partner_id) ?? [];
      const online = ds.filter((d) => d.is_active && d.status === "available" && withinHeartbeat(d.last_location_at)).length;
      let risk: PartnerCapacity["risk"] = "ok";
      if (online === 0 && v.active_listings > 0)          risk = "critical";
      else if (online < v.active_listings)                risk = "warn";
      rows.push({
        partner_id,
        partner_display: v.display,
        active_listings: v.active_listings,
        drivers_total:   ds.length,
        drivers_online:  online,
        risk,
      });
    });

    // Criticals first, then warns, then OK. Same severity ordered by name.
    const severity = { critical: 0, warn: 1, ok: 2 } as const;
    rows.sort((a, b) => severity[a.risk] - severity[b.risk] || a.partner_display.localeCompare(b.partner_display));
    return rows;
  }, [drivers, listings]);

  const unlinkedOnline = useMemo(() => {
    return drivers.filter((d) => !d.carrier_id && d.is_active && d.status === "available" && withinHeartbeat(d.last_location_at)).length;
  }, [drivers]);

  if (loading && drivers.length === 0) {
    return null; // Silent first load — the main table shows its own skeleton
  }

  return (
    <GlassCard padding="none" glow={capacity.some((c) => c.risk === "critical") ? "red" : undefined}>
      <div className="flex items-center justify-between px-5 py-3 border-b border-glass-border">
        <div>
          <h3 className="font-heading text-sm font-semibold text-white flex items-center gap-2">
            <Activity size={14} className="text-cyan-neon" />
            Capacity Oversight
            <span className="text-2xs font-mono text-white/30">(ADR-0014 pre-service shadow)</span>
          </h3>
          <p className="text-2xs font-mono text-white/40 mt-0.5">
            Listings vs live driver availability · {HEARTBEAT_WINDOW_MIN}-minute heartbeat window
          </p>
        </div>
        {unlinkedOnline > 0 && (
          <NeonBadge variant="amber">
            {unlinkedOnline} online driver{unlinkedOnline === 1 ? "" : "s"} unlinked to a partner
          </NeonBadge>
        )}
      </div>

      {error && (
        <p className="px-5 py-3 text-xs text-red-signal font-mono">{error}</p>
      )}

      {capacity.length === 0 ? (
        <p className="px-5 py-6 text-center text-xs text-white/40 font-mono">
          No partners currently listing vehicles.
        </p>
      ) : (
        <>
          <div className="grid grid-cols-[2fr_120px_120px_120px_120px] gap-3 px-5 py-2 border-b border-glass-border">
            {["Partner", "Active Listings", "Online Drivers", "Coverage", "Signal"].map((h) => (
              <span key={h} className="text-2xs font-mono text-white/30 uppercase tracking-wider">{h}</span>
            ))}
          </div>
          {capacity.map((c) => {
            const coverageRatio = c.active_listings === 0 ? 1 : c.drivers_online / c.active_listings;
            const coveragePct   = Math.min(100, Math.round(coverageRatio * 100));
            const riskVariant: BadgeVariant = c.risk === "critical" ? "red" : c.risk === "warn" ? "amber" : "green";
            const riskLabel    = c.risk === "critical" ? "Phantom" : c.risk === "warn" ? "Under-staffed" : "Ready";
            return (
              <div
                key={c.partner_id}
                className={cn(
                  "grid grid-cols-[2fr_120px_120px_120px_120px] gap-3 items-center px-5 py-3 border-b border-glass-border/50 transition-colors",
                  c.risk === "critical" && "bg-red-signal/5",
                )}
              >
                <div>
                  <p className="text-xs font-medium text-white truncate">{c.partner_display}</p>
                  <p className="text-2xs font-mono text-white/30 mt-0.5">{c.partner_id.slice(0, 8)}…</p>
                </div>
                <span className="text-sm font-mono text-white">{c.active_listings}</span>
                <span className={cn(
                  "text-sm font-mono font-semibold",
                  c.drivers_online === 0 ? "text-red-signal" :
                  c.drivers_online < c.active_listings ? "text-amber-signal" : "text-green-signal",
                )}>
                  {c.drivers_online}
                  <span className="text-white/30 font-normal"> / {c.drivers_total}</span>
                </span>
                <div className="flex items-center gap-2">
                  <div className="flex-1 h-1.5 rounded-full bg-glass-300 overflow-hidden">
                    <div
                      className="h-full rounded-full transition-all"
                      style={{
                        width: `${coveragePct}%`,
                        background: c.risk === "critical" ? "#FF3B5C" : c.risk === "warn" ? "#FFAB00" : "#00FF88",
                      }}
                    />
                  </div>
                  <span className="text-2xs font-mono text-white/40">{coveragePct}%</span>
                </div>
                <NeonBadge variant={riskVariant} dot={c.risk !== "ok"}>{riskLabel}</NeonBadge>
              </div>
            );
          })}
        </>
      )}

      <p className="px-5 py-2.5 text-2xs font-mono text-white/30 border-t border-glass-border">
        Phantom = listings published but no driver available now · Under-staffed = drivers &lt; listings.
        When the marketplace service (ADR-0014) ships, phantom listings will be auto-suspended by the Dead-Man&apos;s Switch.
      </p>
    </GlassCard>
  );
}
