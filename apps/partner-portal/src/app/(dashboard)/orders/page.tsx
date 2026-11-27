"use client";

import { useState, useEffect, useCallback, useMemo, Suspense } from "react";
import { motion } from "framer-motion";
import { Package, Globe, Clock, AlertCircle, Banknote, Weight } from "lucide-react";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { LiveMetric } from "@/components/ui/live-metric";
import { variants } from "@/lib/design-system/tokens";
import { cn } from "@/lib/design-system/cn";
import { shipmentsApi, type Shipment } from "@/lib/api/shipments";

// Read-only. Partner self-assignment returns once there is a partner-scoped assignment endpoint
// (dispatch refuses partners its queue and roster by design in Phase 0).

// ── Types ─────────────────────────────────────────────────────────────────────

type OrderStatus =
  | "unassigned" | "assigned" | "picked_up" | "in_transit" | "delivered" | "exception" | "unknown";
type OrderType = "local" | "balikbayan";

interface IncomingOrder {
  id:           string;
  awb:          string;
  type:         OrderType;
  status:       OrderStatus;
  senderName:   string;
  pickupCity:   string;
  receiverName: string;
  destCity:     string;
  destCountry?: string;
  weight:       string;
  isCOD:        boolean;
  codAmount?:   number;
  bookedAt:     string;
}

// ── API fetch functions ───────────────────────────────────────────────────────

// Orders come from order-intake's partner-scoped `GET /v1/shipments`. The
// dispatch queue (`/v1/queue`) is a tenant operation that partners are refused.
// The server narrows the list to the partner the token is bound to; nothing
// here names a partner.
const ORDERS_PAGE_SIZE = 100;  // the server's per_page ceiling
const ORDERS_MAX_PAGES = 5;    // 500 orders is far past what a partner acts on

// Throws when the list cannot be read, so the page can say so instead of
// showing an empty list that looks like "no orders".
async function fetchOrders(): Promise<IncomingOrder[]> {
  const rows: Shipment[] = [];
  for (let page = 1; page <= ORDERS_MAX_PAGES; page++) {
    const { shipments, total } = await shipmentsApi.list({ page, perPage: ORDERS_PAGE_SIZE });
    rows.push(...shipments);
    if (shipments.length < ORDERS_PAGE_SIZE || rows.length >= total) break;
  }
  return rows.filter(s => s.status !== "cancelled").map(mapShipmentRow);
}

function orderStatusOf(status: Shipment["status"]): OrderStatus {
  switch (status) {
    case "pending": case "confirmed":
      return "unassigned";
    case "pickup_assigned":
      return "assigned";
    case "picked_up":
      return "picked_up";
    case "at_hub": case "in_transit": case "customs_hold": case "out_for_delivery":
      return "in_transit";
    case "delivered":
      return "delivered";
    case "failed": case "returned": case "delivery_attempted": case "partial_delivery": case "piece_exception":
      return "exception";
    default:
      // An unrecognised status must not pass for a real state.
      return "unknown";
  }
}

function mapShipmentRow(s: Shipment): IncomingOrder {
  const codCents = s.cod_amount?.amount ?? 0;
  return {
    id:           s.id,
    awb:          s.awb,
    type:         s.service_type === "Balikbayan" || s.service_type === "International" ? "balikbayan" : "local",
    status:       orderStatusOf(s.status),
    senderName:   "—",
    pickupCity:   s.origin?.city ?? "—",
    receiverName: s.customer_name || "—",
    destCity:     s.destination?.city ?? "—",
    destCountry:  s.destination?.country_code || undefined,
    weight:       s.weight ? String(s.weight.grams / 1000) : "—",
    isCOD:        codCents > 0,
    codAmount:    codCents > 0 ? codCents / 100 : undefined,
    bookedAt:     s.created_at ? new Date(s.created_at).toLocaleString() : "—",
  };
}

// ── Status config ─────────────────────────────────────────────────────────────

const STATUS_CONFIG: Record<OrderStatus, {
  label: string; badge: "cyan" | "amber" | "green" | "purple" | "red" | "muted"; dot?: boolean; pulse?: boolean;
}> = {
  unassigned: { label: "Pending",    badge: "cyan",   dot: true, pulse: true },
  assigned:   { label: "Assigned",   badge: "green",  dot: true },
  picked_up:  { label: "Picked Up",  badge: "purple", dot: true },
  in_transit: { label: "In Transit", badge: "amber",  dot: true },
  delivered:  { label: "Delivered",  badge: "green",  dot: true },
  exception:  { label: "Exception",  badge: "red",    dot: true },
  unknown:    { label: "Unknown",    badge: "muted" },
};

// ── Order Row ─────────────────────────────────────────────────────────────────

function OrderRow({ order }: { order: IncomingOrder }) {
  const cfg    = STATUS_CONFIG[order.status];
  const isIntl = order.type === "balikbayan";

  return (
    <motion.tr
      variants={variants.fadeInUp}
      className="group border-b border-glass-border last:border-0 hover:bg-glass-100 transition-colors"
    >
      {/* AWB + Type */}
      <td className="px-4 py-3">
        <div className="flex items-center gap-2.5">
          <div className={cn("flex h-8 w-8 flex-shrink-0 items-center justify-center rounded-lg",
            isIntl ? "bg-purple-surface border border-purple-glow/25" : "bg-cyan-surface border border-cyan-glow/25"
          )}>
            {isIntl ? <Globe className="h-3.5 w-3.5 text-purple-plasma" /> : <Package className="h-3.5 w-3.5 text-cyan-neon" />}
          </div>
          <div>
            <p className="font-mono text-xs font-semibold text-white">{order.awb}</p>
            <p className={cn("text-2xs font-mono", isIntl ? "text-purple-plasma" : "text-white/40")}>
              {isIntl ? "Balikbayan" : "Local"}
            </p>
          </div>
        </div>
      </td>

      {/* Sender → Receiver */}
      <td className="px-4 py-3">
        <div className="text-xs">
          <div className="flex items-center gap-1.5 text-white/70">
            <div className="h-1.5 w-1.5 rounded-full bg-cyan-neon flex-shrink-0" />
            <span className="font-medium truncate max-w-[140px]">{order.senderName}</span>
            <span className="text-white/30 truncate">· {order.pickupCity}</span>
          </div>
          <div className="flex items-center gap-1.5 mt-1 text-white/50">
            <div className="h-1.5 w-1.5 rounded-full bg-white/20 flex-shrink-0" />
            <span className="truncate max-w-[140px]">{order.receiverName}</span>
            <span className="text-white/30 truncate">· {order.destCity}{isIntl && order.destCountry ? `, ${order.destCountry}` : ""}</span>
          </div>
        </div>
      </td>

      {/* Package info */}
      <td className="px-4 py-3 hidden lg:table-cell">
        <div className="text-2xs font-mono space-y-1">
          <div className="flex items-center gap-1.5 text-white/50">
            <Weight className="h-3 w-3" />
            {order.weight} kg
          </div>
          {order.isCOD && (
            <div className="flex items-center gap-1.5 text-amber-signal">
              <Banknote className="h-3 w-3" />
              COD ₱{order.codAmount?.toLocaleString()}
            </div>
          )}
          <div className="flex items-center gap-1.5 text-white/30">
            <Clock className="h-3 w-3" />
            {order.bookedAt}
          </div>
        </div>
      </td>

      {/* Status */}
      <td className="px-4 py-3">
        <NeonBadge variant={cfg.badge} dot={cfg.dot} pulse={cfg.pulse}>
          {cfg.label}
        </NeonBadge>
      </td>
    </motion.tr>
  );
}

// ── Main page ─────────────────────────────────────────────────────────────────

type OrderFilter = "all" | OrderStatus;

const FILTER_TABS: { key: OrderFilter; label: string }[] = [
  { key: "all",        label: "All" },
  { key: "unassigned", label: "Pending" },
  { key: "assigned",   label: "Assigned" },
  { key: "picked_up",  label: "Picked Up" },
  { key: "in_transit", label: "In Transit" },
  { key: "delivered",  label: "Delivered" },
  { key: "exception",  label: "Exception" },
];

function OrdersPageInner() {
  const [orders,    setOrders]    = useState<IncomingOrder[]>([]);
  const [filter,    setFilter]    = useState<OrderFilter>("all");
  const [loadError, setLoadError] = useState<string | null>(null);

  const loadData = useCallback(async () => {
    try {
      setOrders(await fetchOrders());
      setLoadError(null);
    } catch (e) {
      // A failed refresh keeps the last good list on screen and says why.
      setLoadError((e as { message?: string } | null)?.message ?? "Could not load orders");
    }
  }, []);

  useEffect(() => { loadData(); }, [loadData]);
  useEffect(() => {
    const id = setInterval(loadData, 20_000);
    return () => clearInterval(id);
  }, [loadData]);

  const kpis = useMemo(() => {
    const n = (st: OrderStatus) => orders.filter(o => o.status === st).length;
    return [
      { label: "New Orders",   value: n("unassigned"),                                   color: "cyan"   as const, live: true },
      { label: "In Progress",  value: n("assigned") + n("picked_up") + n("in_transit"),  color: "amber"  as const, live: true },
      { label: "Delivered",    value: n("delivered"),                                    color: "green"  as const, live: false },
      { label: "Total Orders", value: orders.length,                                     color: "purple" as const, live: false },
    ];
  }, [orders]);

  const filtered = orders.filter(o => filter === "all" || o.status === filter);

  return (
    <motion.div
      variants={variants.staggerContainer}
      initial="hidden"
      animate="visible"
      className="space-y-6"
    >
      {/* ── KPI strip ────────────────────────────────────────────────── */}
      <motion.div variants={variants.staggerContainer} className="grid grid-cols-2 gap-4 xl:grid-cols-4">
        {kpis.map((kpi) => (
          <motion.div key={kpi.label} variants={variants.fadeInUp}>
            <GlassCard glow={kpi.color}>
              <LiveMetric label={kpi.label} value={kpi.value} trend={0} color={kpi.color} format="number" live={kpi.live} />
            </GlassCard>
          </motion.div>
        ))}
      </motion.div>

      {/* ── Orders table ──────────────────────────────────────────── */}
      <motion.div variants={variants.fadeInUp}>
        <GlassCard padding="none" glow="cyan">
          <div className="flex flex-wrap items-center justify-between gap-3 border-b border-glass-border px-4 py-3">
            <div className="flex items-center gap-3">
              <p className="font-heading text-sm font-semibold text-white">Orders</p>
              <NeonBadge variant="cyan" dot pulse>{orders.length} total</NeonBadge>
            </div>
            {/* Filter tabs */}
            <div className="flex max-w-full items-center gap-1 overflow-x-auto rounded-lg border border-glass-border bg-glass-100 p-1">
              {FILTER_TABS.map((tab) => (
                <button
                  key={tab.key}
                  onClick={() => setFilter(tab.key)}
                  className={cn(
                    "whitespace-nowrap rounded-md px-3 py-1.5 text-xs font-medium transition-all",
                    filter === tab.key
                      ? "bg-cyan-surface text-cyan-neon border border-cyan-glow/30"
                      : "text-white/40 hover:text-white/70"
                  )}
                >
                  {tab.label}
                </button>
              ))}
            </div>
          </div>

          {loadError && (
            <div role="alert" className="flex items-center gap-2 border-b border-glass-border px-4 py-3 text-xs text-red-400">
              <AlertCircle className="h-4 w-4 shrink-0" />
              <span className="min-w-0 flex-1">Could not load orders: {loadError}</span>
              <button onClick={loadData} className="shrink-0 underline underline-offset-2 hover:text-white">Retry</button>
            </div>
          )}

          <div className="overflow-x-auto">
            <table className="w-full">
              <thead>
                <tr className="border-b border-glass-border">
                  {["AWB / Type", "Sender → Receiver", "Package", "Status"].map((h) => (
                    <th key={h} className={cn(
                      "px-4 py-2.5 text-left text-2xs font-medium uppercase tracking-widest text-white/30",
                      h === "Package" && "hidden lg:table-cell"
                    )}>
                      {h}
                    </th>
                  ))}
                </tr>
              </thead>
              <motion.tbody variants={variants.staggerContainer} initial="hidden" animate="visible">
                {filtered.map((order) => <OrderRow key={order.id} order={order} />)}
              </motion.tbody>
            </table>

            {filtered.length === 0 && (
              <div className="flex flex-col items-center gap-2 py-14 text-white/20">
                <Package className="h-8 w-8" />
                <p className="text-sm">{orders.length === 0 ? "No orders yet" : "No orders in this filter"}</p>
              </div>
            )}
          </div>

          <div className="flex items-center justify-between border-t border-glass-border px-4 py-3">
            <p className="text-2xs text-white/30 font-mono">{filtered.length} orders shown</p>
            <div className="flex items-center gap-2 text-2xs text-white/30">
              <div className="h-1.5 w-1.5 rounded-full bg-cyan-neon animate-pulse" />
              Updates every 20s
            </div>
          </div>
        </GlassCard>
      </motion.div>
    </motion.div>
  );
}

export default function OrdersPage() {
  return (
    <Suspense fallback={null}>
      <OrdersPageInner />
    </Suspense>
  );
}
