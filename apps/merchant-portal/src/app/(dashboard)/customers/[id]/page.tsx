"use client";
/**
 * Merchant Portal — Customer Detail Page
 * Fetches a single customer profile from the CDP service by external_customer_id.
 * Route: /customers/[id]  (id = external_customer_id)
 */
import { useEffect, useMemo, useState } from "react";
import { useParams, useRouter } from "next/navigation";
import { motion } from "framer-motion";
import { variants } from "@/lib/design-system/tokens";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import {
  createCdpApi, deliverySuccessRate,
  type CustomerProfile, type BehavioralEvent,
} from "@/lib/api/cdp";
import {
  ArrowLeft, User, Mail, Phone, MapPin, Package, TrendingUp,
  CheckCircle2, AlertCircle, Clock, Star, Activity,
} from "lucide-react";

// ── Helpers ────────────────────────────────────────────────────────────────────

function fmtPhp(cents: number): string {
  return `₱${(cents / 100).toLocaleString(undefined, { minimumFractionDigits: 0, maximumFractionDigits: 0 })}`;
}

function relativeTime(iso: string): string {
  const diff = Date.now() - new Date(iso).getTime();
  const m = Math.floor(diff / 60_000);
  if (m < 1) return "just now";
  if (m < 60) return `${m}m ago`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h ago`;
  return `${Math.floor(h / 24)}d ago`;
}

function clvTier(score: number): { label: string; variant: "green" | "cyan" | "purple" | "amber" | "muted" } {
  if (score >= 80) return { label: "Platinum", variant: "green" };
  if (score >= 60) return { label: "Gold",     variant: "amber" };
  if (score >= 40) return { label: "Silver",   variant: "cyan" };
  if (score >= 20) return { label: "Bronze",   variant: "purple" };
  return { label: "New", variant: "muted" };
}

const EVENT_CFG: Record<string, { label: string; color: string; Icon: React.ElementType }> = {
  booking_created:     { label: "Booking Created",     color: "#00E5FF", Icon: Package       },
  delivery_completed:  { label: "Delivered",           color: "#00FF88", Icon: CheckCircle2  },
  delivery_failed:     { label: "Delivery Failed",     color: "#FF3B5C", Icon: AlertCircle   },
  rating_given:        { label: "Rating Given",        color: "#A855F7", Icon: Star          },
  support_contacted:   { label: "Support Contacted",   color: "#FFAB00", Icon: Activity      },
};

// ── Page ───────────────────────────────────────────────────────────────────────

export default function CustomerDetailPage() {
  const { id }   = useParams<{ id: string }>();
  const router   = useRouter();
  const api      = useMemo(() => createCdpApi(), []);

  const [profile, setProfile] = useState<CustomerProfile | null>(null);
  const [events,  setEvents]  = useState<BehavioralEvent[]>([]);
  const [loading, setLoading] = useState(true);
  const [error,   setError]   = useState<string | null>(null);

  useEffect(() => {
    if (!id) return;
    setLoading(true);
    setError(null);
    Promise.all([api.get(id), api.events(id)])
      .then(([p, evts]) => {
        setProfile(p);
        setEvents(evts);
      })
      .catch(e => setError(e?.response?.data?.message ?? e?.message ?? "Failed to load customer"))
      .finally(() => setLoading(false));
  }, [api, id]);

  if (loading) {
    return (
      <div className="flex h-64 items-center justify-center">
        <p className="font-mono text-sm text-white/40">Loading customer…</p>
      </div>
    );
  }

  if (error || !profile) {
    return (
      <div className="flex h-64 flex-col items-center justify-center gap-3">
        <AlertCircle size={32} className="text-red-signal" />
        <p className="font-mono text-sm text-red-signal">{error ?? "Customer not found"}</p>
        <button onClick={() => router.push("/customers")} className="text-xs text-cyan-neon hover:underline">
          ← Back to Customers
        </button>
      </div>
    );
  }

  const tier        = clvTier(profile.clv_score);
  const successRate = deliverySuccessRate(profile);
  const allEvents   = [...(profile.recent_events ?? []), ...events]
    .filter((e, i, arr) => arr.findIndex(x => x.id === e.id) === i)
    .sort((a, b) => new Date(b.occurred_at).getTime() - new Date(a.occurred_at).getTime())
    .slice(0, 20);

  return (
    <motion.div
      variants={variants.staggerContainer}
      initial="hidden"
      animate="visible"
      className="flex flex-col gap-5 p-6"
    >
      {/* Back + header */}
      <motion.div variants={variants.fadeInUp} className="flex items-center gap-4">
        <button
          onClick={() => router.push("/customers")}
          className="flex h-9 w-9 items-center justify-center rounded-lg border border-glass-border bg-glass-100 text-white/50 transition-all hover:border-cyan-neon/30 hover:text-cyan-neon"
        >
          <ArrowLeft size={16} />
        </button>
        <div className="min-w-0">
          <h1 className="font-heading text-2xl font-bold text-white flex items-center gap-2">
            <User size={20} className="text-cyan-neon flex-shrink-0" />
            {profile.name ?? "Unnamed Customer"}
          </h1>
          <p className="text-xs font-mono text-white/30 mt-0.5">{profile.external_customer_id}</p>
        </div>
        <NeonBadge variant={tier.variant}>{tier.label}</NeonBadge>
      </motion.div>

      {/* KPI strip */}
      <motion.div variants={variants.fadeInUp} className="grid grid-cols-2 gap-3 sm:grid-cols-4">
        {[
          { label: "Total Shipments",  value: profile.total_shipments.toString(),            color: "#00E5FF" },
          { label: "Success Rate",     value: profile.total_shipments === 0 ? "—" : `${successRate.toFixed(0)}%`, color: successRate > 90 ? "#00FF88" : successRate > 70 ? "#00E5FF" : "#FFAB00" },
          { label: "CLV Score",        value: profile.clv_score.toFixed(0),                 color: "#A855F7" },
          { label: "COD Collected",    value: fmtPhp(profile.total_cod_collected_cents),     color: "#00FF88" },
        ].map(({ label, value, color }) => (
          <GlassCard key={label} size="sm">
            <p className="text-2xs font-mono text-white/40 uppercase tracking-wider">{label}</p>
            <p className="font-heading text-xl font-bold mt-1" style={{ color }}>{value}</p>
          </GlassCard>
        ))}
      </motion.div>

      <div className="grid grid-cols-1 gap-5 lg:grid-cols-3">
        {/* Profile info */}
        <motion.div variants={variants.fadeInUp} className="lg:col-span-1">
          <GlassCard>
            <p className="text-xs font-semibold text-white mb-4">Contact Info</p>
            <div className="space-y-3">
              {profile.email && (
                <div className="flex items-center gap-2 text-xs text-white/60">
                  <Mail size={13} className="text-cyan-neon flex-shrink-0" />
                  <span className="truncate font-mono">{profile.email}</span>
                </div>
              )}
              {profile.phone && (
                <div className="flex items-center gap-2 text-xs text-white/60">
                  <Phone size={13} className="text-cyan-neon flex-shrink-0" />
                  <span className="font-mono">{profile.phone}</span>
                </div>
              )}
              {profile.address_history.length > 0 && (
                <div>
                  <p className="text-2xs font-mono text-white/30 uppercase tracking-wider mb-2 mt-4">Address History</p>
                  <div className="space-y-2">
                    {profile.address_history.slice(0, 4).map((a, i) => (
                      <div key={i} className="flex items-start gap-2 text-xs text-white/50">
                        <MapPin size={11} className="text-purple-plasma mt-0.5 flex-shrink-0" />
                        <div className="min-w-0">
                          <p className="truncate text-white/70">{a.address}</p>
                          <p className="text-2xs text-white/30 font-mono">{a.use_count}× · last {relativeTime(a.last_used)}</p>
                        </div>
                      </div>
                    ))}
                  </div>
                </div>
              )}
              <div className="border-t border-glass-border pt-3 mt-3 grid grid-cols-2 gap-2 text-2xs font-mono text-white/30">
                {profile.first_shipment_at && (
                  <div><p className="text-white/20 mb-0.5">First shipment</p><p>{relativeTime(profile.first_shipment_at)}</p></div>
                )}
                {profile.last_shipment_at && (
                  <div><p className="text-white/20 mb-0.5">Last shipment</p><p>{relativeTime(profile.last_shipment_at)}</p></div>
                )}
              </div>
            </div>
          </GlassCard>

          {/* Delivery breakdown */}
          <GlassCard className="mt-4">
            <p className="text-xs font-semibold text-white mb-4">Delivery Breakdown</p>
            <div className="space-y-2">
              {[
                { label: "Successful", count: profile.successful_deliveries, color: "#00FF88", Icon: CheckCircle2 },
                { label: "Failed",     count: profile.failed_deliveries,     color: "#FF3B5C", Icon: AlertCircle  },
                { label: "Pending",    count: profile.total_shipments - profile.successful_deliveries - profile.failed_deliveries, color: "#FFAB00", Icon: Clock },
              ].map(({ label, count, color, Icon }) => (
                <div key={label} className="flex items-center justify-between">
                  <div className="flex items-center gap-2 text-xs text-white/60">
                    <Icon size={12} style={{ color }} />
                    {label}
                  </div>
                  <span className="font-mono text-xs font-semibold" style={{ color }}>{count}</span>
                </div>
              ))}
            </div>
          </GlassCard>
        </motion.div>

        {/* Activity timeline */}
        <motion.div variants={variants.fadeInUp} className="lg:col-span-2">
          <GlassCard padding="none" className="p-5">
            <div className="flex items-center gap-2 mb-4">
              <TrendingUp size={14} className="text-cyan-neon" />
              <p className="text-xs font-semibold text-white">Activity Timeline</p>
            </div>
            {allEvents.length === 0 ? (
              <p className="text-xs font-mono text-white/30 py-6 text-center">No events recorded yet</p>
            ) : (
              <div className="relative">
                <div className="absolute left-3.5 top-0 bottom-0 w-px bg-glass-border" />
                <div className="space-y-4">
                  {allEvents.map((evt) => {
                    const cfg = EVENT_CFG[evt.event_type] ?? { label: evt.event_type, color: "#A855F7", Icon: Activity };
                    return (
                      <div key={evt.id} className="flex items-start gap-4 pl-8 relative">
                        <div
                          className="absolute left-0 flex h-7 w-7 items-center justify-center rounded-full border border-glass-border bg-canvas-100"
                          style={{ boxShadow: `0 0 8px ${cfg.color}40` }}
                        >
                          <cfg.Icon size={12} style={{ color: cfg.color }} />
                        </div>
                        <div className="min-w-0 flex-1 pb-2">
                          <div className="flex items-baseline justify-between gap-2">
                            <p className="text-xs font-medium text-white">{cfg.label}</p>
                            <p className="text-2xs font-mono text-white/25 flex-shrink-0">{relativeTime(evt.occurred_at)}</p>
                          </div>
                          {evt.shipment_id && (
                            <p className="mt-0.5 text-2xs font-mono text-white/40">{evt.shipment_id}</p>
                          )}
                          {Object.keys(evt.metadata ?? {}).length > 0 && (
                            <p className="mt-0.5 text-2xs font-mono text-white/30 truncate">
                              {Object.entries(evt.metadata).map(([k, v]) => `${k}: ${v}`).join(" · ")}
                            </p>
                          )}
                        </div>
                      </div>
                    );
                  })}
                </div>
              </div>
            )}
          </GlassCard>
        </motion.div>
      </div>
    </motion.div>
  );
}
