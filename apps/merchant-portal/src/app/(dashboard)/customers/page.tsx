"use client";
/**
 * Merchant Portal — Customers Page
 * Surfaces the Customer Data Platform (cdp service) for the logged-in tenant.
 *   GET /v1/customers         → paginated list (name/email/phone/CLV filters)
 *   GET /v1/customers/top-clv → top-N customers by CLV score
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { motion } from "framer-motion";
import { variants } from "@/lib/design-system/tokens";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { LiveMetric } from "@/components/ui/live-metric";
import {
  Users, Search, RefreshCw, TrendingUp, Package,
  Mail, Phone, SlidersHorizontal, X,
} from "lucide-react";
import {
  createCdpApi,
  deliverySuccessRate,
  type CustomerProfile,
  type ListProfilesQuery,
  type ProfileType,
} from "@/lib/api/cdp";

function clvTier(score: number): { label: string; variant: "green" | "cyan" | "purple" | "amber" | "muted" } {
  if (score >= 80) return { label: "Platinum", variant: "green" };
  if (score >= 60) return { label: "Gold",     variant: "amber" };
  if (score >= 40) return { label: "Silver",   variant: "cyan" };
  if (score >= 20) return { label: "Bronze",   variant: "purple" };
  return { label: "New", variant: "muted" };
}

function fmtPhp(cents: number): string {
  return `₱${(cents / 100).toLocaleString(undefined, { minimumFractionDigits: 0, maximumFractionDigits: 0 })}`;
}

const PAGE_SIZE = 50;

export default function CustomersPage() {
  const api    = useMemo(() => createCdpApi(), []);
  const router = useRouter();

  const [profiles, setProfiles] = useState<CustomerProfile[]>([]);
  const [top,      setTop]      = useState<CustomerProfile[]>([]);
  const [loading,  setLoading]  = useState(true);
  const [error,    setError]    = useState<string | null>(null);

  // Profile type toggle — default to "sender" so merchants see their shipping clients.
  const [profileTypeFilter, setProfileTypeFilter] = useState<ProfileType | undefined>("sender");

  // Raw filter inputs (update instantly on keystroke)
  const [nameInput,   setNameInput]   = useState("");
  const [emailInput,  setEmailInput]  = useState("");
  const [phoneInput,  setPhoneInput]  = useState("");
  const [minClvInput, setMinClvInput] = useState("");
  const [showFilters, setShowFilters] = useState(false);

  // Debounced values used for the actual fetch
  const [debouncedName,   setDebouncedName]   = useState("");
  const [debouncedEmail,  setDebouncedEmail]  = useState("");
  const [debouncedPhone,  setDebouncedPhone]  = useState("");
  const [debouncedMinClv, setDebouncedMinClv] = useState<number | undefined>(undefined);

  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    if (debounceRef.current) clearTimeout(debounceRef.current);
    debounceRef.current = setTimeout(() => {
      setDebouncedName(nameInput);
      setDebouncedEmail(emailInput);
      setDebouncedPhone(phoneInput);
      const parsed = parseFloat(minClvInput);
      setDebouncedMinClv(isNaN(parsed) ? undefined : parsed);
    }, 350);
    return () => {
      if (debounceRef.current) clearTimeout(debounceRef.current);
    };
  }, [nameInput, emailInput, phoneInput, minClvInput]);

  const activeFilterCount = [debouncedEmail, debouncedPhone, debouncedMinClv !== undefined ? "1" : ""].filter(Boolean).length;

  const load = useCallback(async () => {
    setError(null);
    setLoading(true);
    try {
      const query: ListProfilesQuery = {
        name:         debouncedName   || undefined,
        email:        debouncedEmail  || undefined,
        phone:        debouncedPhone  || undefined,
        min_clv:      debouncedMinClv,
        profile_type: profileTypeFilter,
        limit:        PAGE_SIZE,
        offset:       0,
      };
      const [list, topClv] = await Promise.all([
        api.list(query),
        api.topByClv(5),
      ]);
      setProfiles(list.profiles ?? []);
      setTop(topClv.profiles ?? []);
    } catch (e) {
      const err = e as { message?: string };
      setError(err?.message ?? "Failed to load customers");
    } finally {
      setLoading(false);
    }
  }, [api, debouncedName, debouncedEmail, debouncedPhone, debouncedMinClv, profileTypeFilter]);

  useEffect(() => { load(); }, [load]);

  const kpis = useMemo(() => {
    const totalShipments = profiles.reduce((n, p) => n + p.total_shipments, 0);
    const avgClv = profiles.length === 0 ? 0
      : profiles.reduce((n, p) => n + p.clv_score, 0) / profiles.length;
    const successRate = totalShipments === 0 ? 0
      : (profiles.reduce((n, p) => n + p.successful_deliveries, 0) / totalShipments) * 100;
    const label = profileTypeFilter === "sender" ? "Senders" : profileTypeFilter === "receiver" ? "Receivers" : "All Profiles";
    return [
      { label: label, value: profiles.length, trend: 0, color: "cyan"   as const, format: "number"  as const },
      { label: "Avg CLV Score",   value: avgClv,          trend: 0, color: "purple" as const, format: "number"  as const },
      { label: "Total Shipments", value: totalShipments,  trend: 0, color: "amber"  as const, format: "number"  as const },
      { label: "Success Rate",    value: successRate,     trend: 0, color: "green"  as const, format: "percent" as const },
    ];
  }, [profiles, profileTypeFilter]);

  function clearFilters() {
    setNameInput("");
    setEmailInput("");
    setPhoneInput("");
    setMinClvInput("");
  }

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
            <Users size={22} className="text-cyan-neon" />
            Customers
          </h1>
          <p className="text-sm text-white/40 font-mono mt-0.5">
            Customer Data Platform · {profiles.length} {profileTypeFilter === "sender" ? "senders" : "profiles"} loaded
          </p>
        </div>
        <div className="flex items-center gap-2">
          {/* Sender / All toggle */}
          <div className="flex rounded-lg border border-glass-border overflow-hidden">
            {(["sender", undefined] as const).map((type) => {
              const label = type === "sender" ? "Senders" : "All";
              const active = profileTypeFilter === type;
              return (
                <button
                  key={label}
                  onClick={() => setProfileTypeFilter(type)}
                  className={`px-3 py-1.5 text-xs font-medium transition-colors ${
                    active
                      ? "bg-cyan-neon/15 text-cyan-neon border-r border-glass-border last:border-r-0"
                      : "text-white/40 hover:text-white/70 border-r border-glass-border last:border-r-0"
                  }`}
                >
                  {label}
                </button>
              );
            })}
          </div>
          <button
            onClick={load}
            className="flex items-center gap-1.5 rounded-lg border border-glass-border px-3 py-2 text-xs text-white/60 hover:text-white transition-colors"
            title="Refresh"
          >
            <RefreshCw size={13} />
          </button>
        </div>
      </motion.div>

      {error && (
        <motion.div variants={variants.fadeInUp}>
          <GlassCard padding="sm">
            <p className="text-xs text-red-signal font-mono">{error}</p>
          </GlassCard>
        </motion.div>
      )}

      {/* KPIs */}
      <motion.div variants={variants.fadeInUp} className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        {kpis.map((m) => (
          <GlassCard key={m.label} size="sm" glow={m.color} accent>
            <LiveMetric label={m.label} value={m.value} trend={m.trend} color={m.color} format={m.format} />
          </GlassCard>
        ))}
      </motion.div>

      {/* Top CLV leaderboard + list */}
      <motion.div variants={variants.fadeInUp} className="grid gap-4 lg:grid-cols-[1fr_320px]">
        {/* Search + filters + list */}
        <GlassCard padding="none">
          {/* Toolbar */}
          <div className="flex items-center justify-between px-5 py-4 border-b border-glass-border gap-3 flex-wrap">
            <h2 className="font-heading text-sm font-semibold text-white whitespace-nowrap">
              {profileTypeFilter === "sender" ? "Senders" : "All Customers"}
            </h2>
            <div className="flex items-center gap-2 flex-1">
              {/* Name search */}
              <div className="relative flex-1 max-w-xs">
                <Search size={14} className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-white/30" />
                <input
                  value={nameInput}
                  onChange={(e) => setNameInput(e.target.value)}
                  placeholder="Search by name…"
                  className="w-full rounded-lg border border-glass-border bg-glass-100 pl-9 pr-3 py-2 text-xs text-white placeholder-white/25 outline-none focus:border-cyan-neon/40 transition-colors"
                />
              </div>
              {/* Filters toggle */}
              <button
                onClick={() => setShowFilters((v) => !v)}
                className={`relative flex items-center gap-1.5 rounded-lg border px-3 py-2 text-xs transition-colors ${
                  showFilters
                    ? "border-cyan-neon/40 text-cyan-neon bg-cyan-neon/5"
                    : "border-glass-border text-white/50 hover:text-white"
                }`}
              >
                <SlidersHorizontal size={13} />
                Filters
                {activeFilterCount > 0 && (
                  <span className="absolute -top-1.5 -right-1.5 flex h-4 w-4 items-center justify-center rounded-full bg-cyan-neon text-[9px] font-bold text-canvas-100">
                    {activeFilterCount}
                  </span>
                )}
              </button>
            </div>
          </div>

          {/* Expanded filter row */}
          {showFilters && (
            <div className="flex items-end gap-3 flex-wrap px-5 py-3 border-b border-glass-border bg-glass-50/30">
              <FilterField label="Email" value={emailInput} onChange={setEmailInput} placeholder="customer@example.com" icon={<Mail size={12} />} />
              <FilterField label="Phone" value={phoneInput} onChange={setPhoneInput} placeholder="+63 912…" icon={<Phone size={12} />} />
              <FilterField label="Min CLV" value={minClvInput} onChange={setMinClvInput} placeholder="e.g. 40" type="number" />
              {(emailInput || phoneInput || minClvInput) && (
                <button
                  onClick={clearFilters}
                  className="flex items-center gap-1 text-xs text-white/40 hover:text-white/70 transition-colors mb-0.5"
                >
                  <X size={12} /> Clear
                </button>
              )}
            </div>
          )}

          {/* Column headers */}
          <div className="grid grid-cols-[2fr_70px_90px_90px_90px_1fr] gap-3 px-5 py-2.5 border-b border-glass-border">
            {["Customer", "Type", "CLV", "Shipments", "Success", "Recent Address"].map((h) => (
              <span key={h} className="text-2xs font-mono text-white/30 uppercase tracking-wider">{h}</span>
            ))}
          </div>

          {loading ? (
            <div className="px-5 py-10 text-center text-xs text-white/40 font-mono">loading…</div>
          ) : profiles.length === 0 ? (
            <div className="px-5 py-10 text-center">
              <p className="text-xs text-white/40 font-mono">
                {nameInput || emailInput || phoneInput || minClvInput
                  ? "No customers match the current filters"
                  : profileTypeFilter === "sender"
                  ? "No senders yet — book a shipment to auto-create sender profiles"
                  : "No customers yet"}
              </p>
            </div>
          ) : (
            profiles.map((p) => {
              const tier = clvTier(p.clv_score);
              const successRate = deliverySuccessRate(p);
              const topAddr = (p.address_history ?? [])[0]?.address ?? "—";
              const ptVariant = p.profile_type === "sender" ? "cyan" : p.profile_type === "receiver" ? "purple" : "muted";
              const ptLabel   = p.profile_type === "sender" ? "Sender" : p.profile_type === "receiver" ? "Receiver" : "—";
              return (
                <div
                  key={p.external_customer_id}
                  onClick={() => router.push(`/customers/${p.external_customer_id}`)}
                  className="grid grid-cols-[2fr_70px_90px_90px_90px_1fr] gap-3 items-center px-5 py-3 border-b border-glass-border/50 hover:bg-glass-100 transition-colors cursor-pointer"
                >
                  <div className="min-w-0">
                    <p className="text-xs font-medium text-white truncate">{p.name ?? "Unnamed customer"}</p>
                    <p className="text-2xs font-mono text-white/30 mt-0.5 flex items-center gap-2 truncate">
                      {p.email && (<span className="flex items-center gap-1"><Mail size={10} />{p.email}</span>)}
                      {p.phone && (<span className="flex items-center gap-1"><Phone size={10} />{p.phone}</span>)}
                      {!p.email && !p.phone && <span>{p.external_customer_id.slice(0, 8)}…</span>}
                    </p>
                  </div>
                  <NeonBadge variant={ptVariant}>{ptLabel}</NeonBadge>
                  <NeonBadge variant={tier.variant} dot>{tier.label}</NeonBadge>
                  <span className="text-xs font-mono text-white/60">{p.total_shipments}</span>
                  <span className={`text-xs font-mono font-semibold ${
                    successRate > 90 ? "text-green-signal" :
                    successRate > 70 ? "text-cyan-neon" :
                    p.total_shipments === 0 ? "text-white/30" : "text-amber-signal"
                  }`}>
                    {p.total_shipments === 0 ? "—" : `${successRate.toFixed(0)}%`}
                  </span>
                  <span className="text-xs text-white/40 font-mono truncate" title={topAddr}>{topAddr}</span>
                </div>
              );
            })
          )}
        </GlassCard>

        {/* Top CLV */}
        <GlassCard>
          <div className="flex items-center justify-between mb-4">
            <h2 className="font-heading text-sm font-semibold text-white flex items-center gap-2">
              <TrendingUp size={14} className="text-green-signal" />
              Top Value Customers
            </h2>
          </div>

          <div className="flex flex-col gap-3">
            {top.length === 0 ? (
              <p className="text-xs text-white/30 font-mono">No data yet.</p>
            ) : top.map((p, i) => {
              const tier = clvTier(p.clv_score);
              return (
                <div
                  key={p.external_customer_id}
                  onClick={() => router.push(`/customers/${p.external_customer_id}`)}
                  className="flex items-center gap-3 cursor-pointer hover:opacity-80 transition-opacity"
                >
                  <div className="flex h-7 w-7 items-center justify-center rounded-full bg-glass-100 border border-glass-border text-xs font-mono text-white/60 flex-shrink-0">
                    {i + 1}
                  </div>
                  <div className="min-w-0 flex-1">
                    <p className="text-xs font-medium text-white truncate">{p.name ?? p.external_customer_id.slice(0, 8)}</p>
                    <p className="text-2xs font-mono text-white/30 mt-0.5 flex items-center gap-2">
                      <span>CLV {p.clv_score.toFixed(0)}</span>
                      <span className="flex items-center gap-0.5"><Package size={9} />{p.total_shipments}</span>
                      {p.total_cod_collected_cents > 0 && (
                        <span className="text-green-signal">{fmtPhp(p.total_cod_collected_cents)}</span>
                      )}
                    </p>
                  </div>
                  <NeonBadge variant={tier.variant}>{tier.label}</NeonBadge>
                </div>
              );
            })}
          </div>
        </GlassCard>
      </motion.div>
    </motion.div>
  );
}

// ── FilterField ────────────────────────────────────────────────────────────────

function FilterField({
  label, value, onChange, placeholder, type = "text", icon,
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  type?: string;
  icon?: React.ReactNode;
}) {
  return (
    <div className="flex flex-col gap-1">
      <span className="text-2xs font-mono text-white/30 uppercase tracking-wider">{label}</span>
      <div className="relative">
        {icon && (
          <span className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-white/30">
            {icon}
          </span>
        )}
        <input
          type={type}
          value={value}
          onChange={(e) => onChange(e.target.value)}
          placeholder={placeholder}
          className={`rounded-lg border border-glass-border bg-glass-100 py-1.5 pr-3 text-xs text-white placeholder-white/20 outline-none focus:border-cyan-neon/40 transition-colors ${icon ? "pl-7" : "pl-2.5"}`}
        />
      </div>
    </div>
  );
}
