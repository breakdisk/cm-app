"use client";
/**
 * Admin Portal — Finance Oversight
 * Tenant-wide merchant invoice state from `GET /v1/invoices/tenant`
 * (payments service via api-gateway, BILLING_MANAGE-gated).
 *
 * This is the ops-tier counterpart to the merchant-portal billing page.
 * Shows every merchant invoice in the tenant so ops can spot overdue
 * accounts, disputed invoices, and billing-run output.
 */
import { useCallback, useEffect, useMemo, useState } from "react";
import { motion } from "framer-motion";
import { variants } from "@/lib/design-system/tokens";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { LiveMetric } from "@/components/ui/live-metric";
import {
  Receipt, RefreshCw, Search, CheckCircle2, Clock, AlertCircle, FileText,
} from "lucide-react";
import { authFetch } from "@/lib/auth/auth-fetch";

// Route through the api-gateway — same pattern as every other admin-portal
// data page. `NEXT_PUBLIC_API_URL` is the only backend URL baked into the
// build; per-service URLs like NEXT_PUBLIC_PAYMENTS_URL are not wired up
// on the Dokploy side, so hard-coding localhost here would break prod.
const API_BASE = process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:8000";

interface InvoiceSummary {
  invoice_id:     string;
  invoice_number: string;
  invoice_type:   string;
  status:         string;
  awb_count:      number;
  subtotal_cents: number;
  vat_cents:      number;
  total_cents:    number;
  billing_period: string;   // "YYYY-MM"
  due_at:         string;
  issued_at:      string;
}

type StatusKey = "all" | "issued" | "overdue" | "paid" | "disputed" | "draft" | "cancelled";

type StatusBadge = { label: string; variant: "green" | "cyan" | "amber" | "red" | "purple"; icon: React.ReactNode };

function statusBadge(status: string): StatusBadge {
  switch (status) {
    case "paid":      return { label: "Paid",     variant: "green",  icon: <CheckCircle2 size={11} /> };
    case "issued":    return { label: "Issued",   variant: "amber",  icon: <Clock size={11} />        };
    case "overdue":   return { label: "Overdue",  variant: "red",    icon: <AlertCircle size={11} /> };
    case "draft":     return { label: "Draft",    variant: "cyan",   icon: <FileText size={11} />    };
    case "disputed":  return { label: "Disputed", variant: "purple", icon: <AlertCircle size={11} /> };
    case "cancelled": return { label: "Void",     variant: "red",    icon: <AlertCircle size={11} /> };
    default:          return { label: status,     variant: "cyan",   icon: <Clock size={11} />        };
  }
}

function formatPeso(cents: number): string {
  return `₱${Math.round(cents / 100).toLocaleString()}`;
}

function formatBillingPeriod(period: string): string {
  const [y, m] = period.split("-").map(Number);
  if (!y || !m) return period;
  return new Date(Date.UTC(y, m - 1, 1)).toLocaleString("en-US", { year: "numeric", month: "long", timeZone: "UTC" });
}

function formatDate(iso: string): string {
  try { return new Date(iso).toLocaleDateString("en-US", { year: "numeric", month: "short", day: "numeric" }); }
  catch { return iso; }
}

function isOverdueByDate(iso: string): boolean {
  try { return new Date(iso).getTime() < Date.now(); }
  catch { return false; }
}

const FILTER_TABS: Array<{ key: StatusKey; label: string }> = [
  { key: "all",      label: "All" },
  { key: "issued",   label: "Issued" },
  { key: "overdue",  label: "Overdue" },
  { key: "paid",     label: "Paid" },
  { key: "disputed", label: "Disputed" },
  { key: "draft",    label: "Draft" },
];

export default function FinancePage() {
  const [invoices,  setInvoices]  = useState<InvoiceSummary[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [statusFilter, setStatusFilter] = useState<StatusKey>("all");
  const [query, setQuery] = useState("");

  const fetchInvoices = useCallback(async () => {
    setIsLoading(true);
    setLoadError(null);
    try {
      const res = await authFetch(`${API_BASE}/v1/invoices/tenant`);
      if (!res.ok) {
        const body = await res.text().catch(() => "");
        setLoadError(`Failed to load invoices (HTTP ${res.status})${body ? `: ${body.slice(0, 200)}` : ""}`);
        setInvoices([]);
        return;
      }
      const json = await res.json();
      setInvoices(Array.isArray(json.data) ? json.data : []);
    } catch (e) {
      setLoadError(e instanceof Error ? e.message : "Network error");
      setInvoices([]);
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => { fetchInvoices(); }, [fetchInvoices]);

  // Finance data moves on billing-run completion (monthly/weekly cron). A 5-min
  // poll is plenty — no Kafka channel exposed to the admin portal yet.
  useEffect(() => {
    const id = setInterval(fetchInvoices, 5 * 60_000);
    return () => clearInterval(id);
  }, [fetchInvoices]);

  // Derive "effective status" — backend marks issued invoices overdue via a
  // scheduled task, but a fresh read may include issued invoices whose due_at
  // has just passed. Show them as overdue in the UI so the number matches the
  // billing team's mental model.
  const withEffectiveStatus = useMemo(() => invoices.map((i) => {
    const effective = i.status === "issued" && isOverdueByDate(i.due_at) ? "overdue" : i.status;
    return { ...i, effective_status: effective };
  }), [invoices]);

  const kpi = useMemo(() => {
    const now = new Date();
    const thisMonthKey = `${now.getUTCFullYear()}-${String(now.getUTCMonth() + 1).padStart(2, "0")}`;

    const outstanding = withEffectiveStatus.filter((i) => i.effective_status === "issued" || i.effective_status === "overdue");
    const overdue     = withEffectiveStatus.filter((i) => i.effective_status === "overdue");
    const paidMtd     = withEffectiveStatus.filter((i) => i.status === "paid" && i.billing_period === thisMonthKey);

    return {
      outstanding:      Math.round(outstanding.reduce((a, i) => a + i.total_cents, 0) / 100),
      outstandingCount: outstanding.length,
      overdue:          Math.round(overdue.reduce((a, i) => a + i.total_cents, 0) / 100),
      overdueCount:     overdue.length,
      paidMtd:          Math.round(paidMtd.reduce((a, i) => a + i.total_cents, 0) / 100),
      shipmentsBilled:  withEffectiveStatus.reduce((a, i) => a + (i.awb_count ?? 0), 0),
    };
  }, [withEffectiveStatus]);

  const visible = useMemo(() => {
    const q = query.trim().toLowerCase();
    return withEffectiveStatus.filter((i) => {
      if (statusFilter !== "all" && i.effective_status !== statusFilter) return false;
      if (q && !i.invoice_number.toLowerCase().includes(q) && !i.billing_period.includes(q)) return false;
      return true;
    });
  }, [withEffectiveStatus, statusFilter, query]);

  return (
    <motion.div
      variants={variants.staggerContainer}
      initial="hidden"
      animate="visible"
      className="flex flex-col gap-5"
    >
      {/* Header */}
      <motion.div variants={variants.fadeInUp} className="flex items-center justify-between">
        <div>
          <h1 className="font-heading text-2xl font-bold text-white flex items-center gap-2">
            <Receipt size={22} className="text-amber-signal" />
            Finance Oversight
          </h1>
          <p className="text-sm text-white/40 font-mono mt-0.5">
            Tenant-wide merchant billing · {invoices.length} invoice{invoices.length === 1 ? "" : "s"}
          </p>
        </div>
        <button
          onClick={fetchInvoices}
          disabled={isLoading}
          className="flex items-center gap-1.5 rounded-lg border border-glass-border bg-glass-100 px-3 py-2 text-xs text-white/60 hover:text-white transition-colors disabled:opacity-50"
        >
          <RefreshCw size={12} className={isLoading ? "animate-spin" : ""} /> Refresh
        </button>
      </motion.div>

      {loadError && (
        <motion.div variants={variants.fadeInUp}>
          <GlassCard size="sm" glow="red">
            <p className="text-xs font-mono text-red-signal">{loadError}</p>
          </GlassCard>
        </motion.div>
      )}

      {/* KPI row */}
      <motion.div variants={variants.fadeInUp} className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        <GlassCard size="sm" glow="amber" accent>
          <LiveMetric label="Outstanding" value={kpi.outstanding} color="amber" format="currency" />
        </GlassCard>
        <GlassCard size="sm" glow="red" accent>
          <LiveMetric label="Overdue"     value={kpi.overdue}     color="red"   format="currency" />
        </GlassCard>
        <GlassCard size="sm" glow="green" accent>
          <LiveMetric label="Paid MTD"    value={kpi.paidMtd}     color="green" format="currency" />
        </GlassCard>
        <GlassCard size="sm" glow="cyan" accent>
          <LiveMetric label="Shipments Billed" value={kpi.shipmentsBilled} color="cyan" format="number" />
        </GlassCard>
      </motion.div>

      {/* Filter bar */}
      <motion.div variants={variants.fadeInUp}>
        <GlassCard size="sm">
          <div className="flex flex-wrap items-center gap-3">
            <div className="flex items-center gap-1 flex-wrap">
              {FILTER_TABS.map((t) => {
                const isActive = statusFilter === t.key;
                const count = t.key === "all"
                  ? withEffectiveStatus.length
                  : withEffectiveStatus.filter((i) => i.effective_status === t.key).length;
                return (
                  <button
                    key={t.key}
                    onClick={() => setStatusFilter(t.key)}
                    className={`px-3 py-1.5 rounded-md text-xs font-mono transition-colors ${
                      isActive
                        ? "bg-purple-surface text-purple-plasma"
                        : "text-white/50 hover:text-white/80 hover:bg-glass-200"
                    }`}
                  >
                    {t.label} <span className="ml-1 text-white/30">{count}</span>
                  </button>
                );
              })}
            </div>
            <div className="flex-1 min-w-[200px] relative">
              <Search size={12} className="absolute left-2.5 top-1/2 -translate-y-1/2 text-white/30" />
              <input
                type="search"
                placeholder="Search invoice number or period (YYYY-MM)…"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                className="w-full pl-7 pr-3 py-1.5 bg-glass-100 border border-glass-border rounded-md text-xs font-mono text-white/80 placeholder:text-white/30 focus:outline-none focus:border-purple-plasma/40"
              />
            </div>
          </div>
        </GlassCard>
      </motion.div>

      {/* Alerts strip */}
      {kpi.overdueCount > 0 && (
        <motion.div variants={variants.fadeInUp}>
          <GlassCard size="sm" glow="red" accent>
            <div className="flex items-center gap-2.5">
              <AlertCircle size={14} className="text-red-signal" />
              <span className="text-xs font-mono text-white/80">
                <span className="text-red-signal font-semibold">{kpi.overdueCount}</span> invoice{kpi.overdueCount === 1 ? " is" : "s are"} overdue
                {" — "}total {formatPeso(kpi.overdue * 100)} past due.
              </span>
            </div>
          </GlassCard>
        </motion.div>
      )}

      {/* Invoice table */}
      <motion.div variants={variants.fadeInUp}>
        <GlassCard padding="none">
          <div className="flex items-center justify-between px-5 py-4 border-b border-glass-border">
            <h2 className="font-heading text-sm font-semibold text-white">Invoices</h2>
            <span className="text-2xs font-mono text-white/30">
              Showing {visible.length} of {invoices.length}
            </span>
          </div>

          <div className="grid grid-cols-[2fr_110px_100px_110px_110px_110px_100px] gap-3 px-5 py-2.5 border-b border-glass-border">
            {["Invoice", "AWBs", "Subtotal", "VAT", "Total", "Due Date", "Status"].map((h) => (
              <span key={h} className="text-2xs font-mono text-white/30 uppercase tracking-wider">{h}</span>
            ))}
          </div>

          {isLoading && (
            <div className="px-5 py-12 text-center text-xs font-mono text-white/40">Loading invoices…</div>
          )}

          {!isLoading && !loadError && visible.length === 0 && (
            <div className="px-5 py-12 text-center text-xs font-mono text-white/40">
              {invoices.length === 0
                ? "No invoices yet — waiting for the first billing run."
                : "No invoices match the current filter."}
            </div>
          )}

          {!isLoading && visible.map((inv) => {
            const badge = statusBadge(inv.effective_status);
            return (
              <div
                key={inv.invoice_id}
                className="grid grid-cols-[2fr_110px_100px_110px_110px_110px_100px] gap-3 items-center px-5 py-3.5 border-b border-glass-border/50 hover:bg-glass-100 transition-colors"
              >
                <div>
                  <p className="text-xs font-mono text-cyan-neon">{inv.invoice_number}</p>
                  <p className="text-2xs font-mono text-white/30 mt-0.5">{formatBillingPeriod(inv.billing_period)}</p>
                </div>
                <span className="text-xs font-mono text-white/60">{inv.awb_count.toLocaleString()}</span>
                <span className="text-xs font-mono text-white/60">{formatPeso(inv.subtotal_cents)}</span>
                <span className="text-xs font-mono text-amber-signal">{formatPeso(inv.vat_cents)}</span>
                <span className="text-sm font-bold font-heading text-white">{formatPeso(inv.total_cents)}</span>
                <span className={`text-xs font-mono ${inv.effective_status === "overdue" ? "text-red-signal" : "text-white/50"}`}>
                  {formatDate(inv.due_at)}
                </span>
                <NeonBadge variant={badge.variant}>
                  <span className="flex items-center gap-1">{badge.icon}{badge.label}</span>
                </NeonBadge>
              </div>
            );
          })}
        </GlassCard>
      </motion.div>
    </motion.div>
  );
}
