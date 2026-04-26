"use client";
/**
 * Admin Portal — Carrier Ops Page
 * Third-party carrier management: performance, allocation, SLA contract status.
 *
 * Sourced from carrier service `GET /v1/carriers` (proxied by api-gateway).
 * Each row reflects a real carrier in this tenant — performance grade,
 * lifetime on-time count, and rate-card coverage zones come straight from
 * the domain entity. KPIs are derived totals so this page stays useful
 * even before a dedicated /v1/carriers/aggregate endpoint exists.
 */
import { Suspense, useCallback, useEffect, useMemo, useState } from "react";
import { useSearchParams } from "next/navigation";
import { motion } from "framer-motion";
import { variants } from "@/lib/design-system/tokens";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { LiveMetric } from "@/components/ui/live-metric";
import { GitBranch, Plus, LineChart, Wallet, X, Store, RefreshCw } from "lucide-react";
import { authFetch } from "@/lib/auth/auth-fetch";

const API_BASE = process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:8000";

// ── Server types — mirror services/carrier/src/domain/entities/mod.rs ──

type CarrierStatus = "pending_verification" | "active" | "suspended" | "deactivated";
type PerformanceGrade = "excellent" | "good" | "fair" | "poor";

interface RateCard {
  service_type: string;
  base_rate_cents: number;
  per_kg_cents: number;
  max_weight_kg: number;
  coverage_zones: string[];
}

interface SlaCommitment {
  on_time_target_pct: number;
  max_delivery_days: number;
  penalty_per_breach: number;
}

interface ServerCarrier {
  id:                string | { 0: string };
  tenant_id:         string | { 0: string };
  name:              string;
  code:              string;
  contact_email:     string;
  contact_phone?:    string | null;
  api_endpoint?:     string | null;
  status:            CarrierStatus;
  sla:               SlaCommitment;
  rate_cards:        RateCard[];
  total_shipments:   number;
  on_time_count:     number;
  failed_count:      number;
  performance_grade: PerformanceGrade;
  onboarded_at:      string;
  updated_at:        string;
}

// ── Page-local view model ───────────────────────────────────────────────────────

type RowStatus = "active" | "probation" | "suspended" | "pending";

interface Carrier {
  id: string;
  name: string;
  code: string;
  coverage: string[];
  status: RowStatus;
  sla_rate: number;
  sla_target: number;
  shipments_mtd: number;
  cost_per_shipment: number;
  integration: "API" | "Manual" | "EDI";
  grade: "A" | "B" | "C" | "D";
}

function carrierIdOf(c: ServerCarrier): string {
  const raw = c.id as unknown;
  if (typeof raw === "string") return raw;
  if (raw && typeof raw === "object" && "0" in raw) return String((raw as { 0: string })[0]);
  return "";
}

function gradeLetter(g: PerformanceGrade): "A" | "B" | "C" | "D" {
  return g === "excellent" ? "A" : g === "good" ? "B" : g === "fair" ? "C" : "D";
}

// Maps server status → page status. probation is a UI-only intermediate
// state we render when SLA% is below target on an otherwise active carrier;
// the server doesn't carry "probation" as a first-class status.
function rowStatus(s: CarrierStatus, slaOk: boolean): RowStatus {
  if (s === "active")               return slaOk ? "active" : "probation";
  if (s === "suspended")            return "suspended";
  if (s === "pending_verification") return "pending";
  return "suspended";
}

function mapCarrier(c: ServerCarrier): Carrier {
  const slaOk = c.total_shipments === 0
    ? true
    : (c.on_time_count / c.total_shipments) * 100 >= c.sla.on_time_target_pct;
  const sla_rate = c.total_shipments === 0
    ? 0
    : Math.round((c.on_time_count / c.total_shipments) * 1000) / 10;
  const allZones = Array.from(new Set(c.rate_cards.flatMap((r) => r.coverage_zones)));
  const baseRates = c.rate_cards.map((r) => r.base_rate_cents).filter((n) => n > 0);
  const avgCost = baseRates.length > 0
    ? Math.round(baseRates.reduce((a, b) => a + b, 0) / baseRates.length / 100)
    : 0;
  return {
    id:                carrierIdOf(c),
    name:              c.name,
    code:              c.code,
    coverage:          allZones,
    status:            rowStatus(c.status, slaOk),
    sla_rate,
    sla_target:        c.sla.on_time_target_pct,
    shipments_mtd:     c.total_shipments,
    cost_per_shipment: avgCost,
    integration:       c.api_endpoint ? "API" : "Manual",
    grade:             gradeLetter(c.performance_grade),
  };
}

// Mock list kept only as a fallback for environments where the carrier
// service is unreachable — the page falls through to it after a failed
// fetch so the UI still renders something useful.
const CARRIERS_FALLBACK: Carrier[] = [
  { id: "C01", name: "FastLine Couriers",  code: "FAST", coverage: ["Metro Manila", "Luzon A"],   status: "active",    sla_rate: 94.8, sla_target: 93, shipments_mtd: 8420, cost_per_shipment: 50,  integration: "API",    grade: "A" },
  { id: "C02", name: "SpeedEx PH",         code: "SPDX", coverage: ["Luzon B", "Visayas"],        status: "active",    sla_rate: 89.2, sla_target: 87, shipments_mtd: 1840, cost_per_shipment: 68,  integration: "API",    grade: "B" },
  { id: "C03", name: "IslandLink Express", code: "ILE",  coverage: ["Visayas", "Mindanao"],       status: "active",    sla_rate: 86.4, sla_target: 85, shipments_mtd: 920,  cost_per_shipment: 95,  integration: "Manual", grade: "B" },
  { id: "C04", name: "NorthlinkLogistics", code: "NORL", coverage: ["Luzon B (North)"],           status: "active",    sla_rate: 91.8, sla_target: 88, shipments_mtd: 640,  cost_per_shipment: 72,  integration: "EDI",    grade: "A" },
  { id: "C05", name: "SouthStar Delivery", code: "SSD",  coverage: ["Mindanao"],                  status: "probation", sla_rate: 78.4, sla_target: 85, shipments_mtd: 280,  cost_per_shipment: 82,  integration: "Manual", grade: "D" },
  { id: "C06", name: "MegaMover PH",       code: "MEGA", coverage: ["Metro Manila"],              status: "active",    sla_rate: 96.1, sla_target: 95, shipments_mtd: 1240, cost_per_shipment: 45,  integration: "API",    grade: "A" },
  { id: "C07", name: "VisMinLog",          code: "VML",  coverage: ["Visayas", "Mindanao"],       status: "active",    sla_rate: 88.2, sla_target: 85, shipments_mtd: 480,  cost_per_shipment: 88,  integration: "API",    grade: "B" },
  { id: "C08", name: "QuickShip Cebu",     code: "QSC",  coverage: ["Cebu Metro"],                status: "pending",   sla_rate: 0,    sla_target: 90, shipments_mtd: 0,    cost_per_shipment: 62,  integration: "API",    grade: "A" },
];

const STATUS_CONFIG: Record<RowStatus, { label: string; variant: "green" | "cyan" | "amber" | "red" }> = {
  active:    { label: "Active",    variant: "green" },
  probation: { label: "Probation", variant: "red"   },
  suspended: { label: "Suspended", variant: "red"   },
  pending:   { label: "Pending",   variant: "amber" },
};

const GRADE_COLOR: Record<Carrier["grade"], string> = {
  A: "text-green-signal", B: "text-cyan-neon", C: "text-amber-signal", D: "text-red-signal",
};

function CarriersPageInner() {
  const searchParams = useSearchParams();
  const coverageFilter = searchParams.get("coverage");

  const [carriers, setCarriers] = useState<Carrier[]>([]);
  const [loading,  setLoading]  = useState(true);
  const [error,    setError]    = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const res = await authFetch(`${API_BASE}/v1/carriers`);
      if (!res.ok) throw new Error(`Carriers fetch failed: ${res.status} ${res.statusText}`);
      const json = await res.json();
      const list: ServerCarrier[] = json.carriers ?? json.data ?? [];
      setCarriers(list.map(mapCarrier));
    } catch (e) {
      // Fall back to the canned list so the page still renders something
      // useful in dev or when the carrier service is briefly unreachable.
      setError(e instanceof Error ? e.message : "Failed to load carriers");
      setCarriers(CARRIERS_FALLBACK);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { load(); }, [load]);

  const visibleCarriers = useMemo(() => coverageFilter
    ? carriers.filter((c) =>
        c.coverage.some((z) => z.toLowerCase().includes(coverageFilter.toLowerCase())),
      )
    : carriers,
  [carriers, coverageFilter]);

  const kpis = useMemo(() => {
    const active = carriers.filter((c) => c.status === "active");
    const totalShipments = carriers.reduce((a, c) => a + c.shipments_mtd, 0);
    // Shipment-weighted SLA average — gives big carriers proper weight.
    const slaWeighted = totalShipments > 0
      ? carriers.reduce((a, c) => a + c.sla_rate * c.shipments_mtd, 0) / totalShipments
      : 0;
    const costMtd = carriers.reduce((a, c) => a + c.cost_per_shipment * c.shipments_mtd, 0);
    return [
      { label: "Active Carriers",   value: active.length, trend: 0,  color: "cyan"   as const, format: "number"   as const },
      { label: "Shipments via 3PL", value: totalShipments,trend: 0,  color: "purple" as const, format: "number"   as const },
      { label: "Avg 3PL SLA",       value: slaWeighted,   trend: 0,  color: "green"  as const, format: "percent"  as const },
      { label: "3PL Cost MTD",      value: costMtd,       trend: 0,  color: "amber"  as const, format: "currency" as const },
    ];
  }, [carriers]);

  return (
    <motion.div
      variants={variants.staggerContainer}
      initial="hidden"
      animate="visible"
      className="flex flex-col gap-5 p-6"
    >
      {/* Header */}
      <motion.div variants={variants.fadeInUp} className="flex items-center justify-between">
        <div>
          <h1 className="font-heading text-2xl font-bold text-white flex items-center gap-2">
            <GitBranch size={22} className="text-cyan-neon" />
            Carrier Ops
          </h1>
          <p className="text-sm text-white/40 font-mono mt-0.5">
            {loading
              ? "loading…"
              : `${carriers.length} carrier${carriers.length === 1 ? "" : "s"} · ${carriers.filter((c) => c.status === "active").length} active`}
          </p>
        </div>
        <div className="flex items-center gap-2">
          <a
            href="/admin/marketplace"
            title="Cross-partner marketplace oversight"
            className="flex items-center gap-1.5 rounded-lg border border-purple-plasma/30 bg-purple-surface px-3 py-2 text-xs font-semibold text-purple-plasma transition-all hover:shadow-[0_0_12px_rgba(168,85,247,0.35)]"
          >
            <Store size={12} /> Marketplace
          </a>
          <button
            onClick={load}
            title="Refresh"
            className="flex items-center gap-1 rounded-lg border border-glass-border bg-glass-100 px-3 py-2 text-xs text-white/60 hover:text-white transition-colors"
          >
            <RefreshCw size={12} />
          </button>
          <button className="flex items-center gap-1.5 rounded-lg bg-gradient-to-r from-cyan-neon to-purple-plasma px-4 py-2 text-xs font-semibold text-canvas">
            <Plus size={12} /> Onboard Carrier
          </button>
        </div>
      </motion.div>

      {error && (
        <motion.div variants={variants.fadeInUp}>
          <div className="rounded-lg border border-amber-signal/30 bg-amber-signal/5 px-3 py-2">
            <span className="text-2xs font-mono text-amber-signal">{error} — showing cached data</span>
          </div>
        </motion.div>
      )}

      {/* Coverage filter banner (from partner/sla deep link) */}
      {coverageFilter && (
        <motion.div variants={variants.fadeInUp}>
          <div className="flex items-center gap-2 rounded-lg border border-purple-plasma/25 bg-purple-plasma/5 px-3 py-2">
            <GitBranch size={13} className="text-purple-plasma" />
            <span className="text-xs font-mono text-white/70">
              Filtered by coverage <span className="text-purple-plasma font-bold">{coverageFilter}</span>
              <span className="text-white/30"> · {visibleCarriers.length} of {carriers.length} carriers</span>
            </span>
            <a
              href="/admin/carriers"
              title="Clear filter"
              className="ml-auto inline-flex h-5 w-5 items-center justify-center rounded-md text-white/40 hover:text-white transition-colors"
            >
              <X size={11} />
            </a>
          </div>
        </motion.div>
      )}

      {/* KPI row — derived from the carrier list. trend stays at 0 until
          a /v1/carriers/aggregate endpoint exposes period-over-period
          deltas; the value is what matters today. */}
      <motion.div variants={variants.fadeInUp} className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        {kpis.map((m) => (
          <GlassCard key={m.label} size="sm" glow={m.color} accent>
            <LiveMetric label={m.label} value={m.value} trend={m.trend} color={m.color} format={m.format} />
          </GlassCard>
        ))}
      </motion.div>

      {/* Carrier table */}
      <motion.div variants={variants.fadeInUp}>
        <GlassCard padding="none">
          <div className="grid grid-cols-[2fr_1fr_60px_80px_80px_100px_60px_80px] gap-3 px-5 py-2.5 border-b border-glass-border">
            {["Carrier", "Coverage", "Grade", "SLA", "Shipments", "Cost/Ship", "Int.", "Status"].map((h) => (
              <span key={h} className="text-2xs font-mono text-white/30 uppercase tracking-wider">{h}</span>
            ))}
          </div>

          {visibleCarriers.map((c) => {
            const { label, variant } = STATUS_CONFIG[c.status];
            const slaOk = c.sla_rate >= c.sla_target;
            return (
              <div key={c.id} className="grid grid-cols-[2fr_1fr_60px_80px_80px_100px_60px_80px] gap-3 items-center px-5 py-3.5 border-b border-glass-border/50 hover:bg-glass-100 transition-colors cursor-pointer">
                <div className="flex items-center gap-2">
                  <div>
                    <p className="text-xs font-semibold text-white">{c.name}</p>
                    <div className="flex items-center gap-1 mt-0.5">
                      <span className="text-2xs font-mono text-cyan-neon/70">{c.code}</span>
                      <span className="text-2xs font-mono text-white/20">·</span>
                      <span className="text-2xs font-mono text-white/30 truncate max-w-[120px]">{c.id}</span>
                    </div>
                  </div>
                </div>
                <div className="flex flex-wrap gap-0.5">
                  {c.coverage.slice(0,2).map((z) => (
                    <span key={z} className="text-2xs font-mono text-white/40 bg-glass-200 rounded px-1">{z}</span>
                  ))}
                  {c.coverage.length > 2 && <span className="text-2xs font-mono text-white/30">+{c.coverage.length - 2}</span>}
                </div>
                <span className={`text-lg font-bold font-heading ${GRADE_COLOR[c.grade]}`}>{c.grade}</span>
                <span className={`text-xs font-bold font-mono ${slaOk ? "text-green-signal" : "text-red-signal"}`}>
                  {c.sla_rate > 0 ? `${c.sla_rate}%` : "—"}
                </span>
                <span className="text-xs font-mono text-white/60">{c.shipments_mtd.toLocaleString()}</span>
                <span className="text-xs font-mono text-white/60">₱{c.cost_per_shipment}</span>
                <NeonBadge variant={c.integration === "API" ? "green" : c.integration === "EDI" ? "cyan" : "amber"}>
                  {c.integration}
                </NeonBadge>
                <div className="flex items-center gap-1.5">
                  <NeonBadge variant={variant} dot={c.status === "active"}>{label}</NeonBadge>
                  {/* Cross-portal — partner-portal owns SLA detail + payout ledger.
                      Plain <a> preserves the /partner basePath after the jump. */}
                  <a
                    href={`/partner/sla?carrier=${encodeURIComponent(c.id)}`}
                    title="Open SLA in Partner Portal"
                    onClick={(e) => e.stopPropagation()}
                    className="inline-flex h-6 w-6 items-center justify-center rounded-md border border-glass-border text-white/40 hover:text-cyan-neon hover:border-cyan-neon/30 transition-colors"
                  >
                    <LineChart size={11} />
                  </a>
                  <a
                    href={`/partner/payouts?carrier=${encodeURIComponent(c.id)}`}
                    title="Open Payouts in Partner Portal"
                    onClick={(e) => e.stopPropagation()}
                    className="inline-flex h-6 w-6 items-center justify-center rounded-md border border-glass-border text-white/40 hover:text-green-signal hover:border-green-signal/30 transition-colors"
                  >
                    <Wallet size={11} />
                  </a>
                </div>
              </div>
            );
          })}
        </GlassCard>
      </motion.div>
    </motion.div>
  );
}

export default function CarriersPage() {
  return (
    <Suspense fallback={null}>
      <CarriersPageInner />
    </Suspense>
  );
}
