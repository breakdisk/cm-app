"use client";
/**
 * Merchant Portal — Campaigns Page
 * Marketing automation: active campaigns, performance, campaign builder CTA.
 *
 * Data flow:
 *   GET  /v1/campaigns            → marketing::list
 *   POST /v1/campaigns            → marketing::create
 *   POST /v1/campaigns/:id/activate → emits CAMPAIGN_TRIGGERED → engagement
 *   POST /v1/campaigns/:id/cancel → marketing::cancel
 * The page polls every 30s while active, and reloads after any mutation.
 */
import { useCallback, useState, useEffect, useMemo, Suspense } from "react";
import { useSearchParams, useRouter } from "next/navigation";
import { motion, AnimatePresence } from "framer-motion";
import { variants } from "@/lib/design-system/tokens";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { LiveMetric } from "@/components/ui/live-metric";
import {
  AreaChart, Area, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer,
} from "recharts";
import {
  Megaphone, Plus, Zap, MessageSquare, Mail, Smartphone, Play, X,
  BarChart2, ChevronDown, CheckCircle2, RefreshCw,
} from "lucide-react";
import {
  createCampaignsApi,
  type Campaign,
  type Channel,
  type CampaignStatus,
  type CreateCampaignPayload,
} from "@/lib/api/campaigns";

const CHANNEL_ICON: Record<Channel, React.ReactNode> = {
  whatsapp: <MessageSquare size={12} className="text-green-signal" />,
  sms:      <Smartphone    size={12} className="text-cyan-neon"    />,
  email:    <Mail          size={12} className="text-purple-plasma" />,
  push:     <Zap           size={12} className="text-amber-signal" />,
};

const STATUS_VARIANT: Record<CampaignStatus, "green" | "amber" | "purple" | "red" | "cyan"> = {
  draft:     "purple",
  scheduled: "cyan",
  sending:   "green",
  completed: "red",
  cancelled: "amber",
  failed:    "red",
};

/**
 * The backend does not yet expose a per-day message-volume timeseries, so the
 * chart is left as a visual placeholder until an analytics endpoint exists.
 * When analytics/engagement ship `/v1/analytics/campaign-sends?range=week`,
 * swap this constant for a `useQuery`.
 */
const SEND_TREND = [
  { day: "Mon", whatsapp: 0, sms: 0, email: 0 },
  { day: "Tue", whatsapp: 0, sms: 0, email: 0 },
  { day: "Wed", whatsapp: 0, sms: 0, email: 0 },
  { day: "Thu", whatsapp: 0, sms: 0, email: 0 },
  { day: "Fri", whatsapp: 0, sms: 0, email: 0 },
  { day: "Sat", whatsapp: 0, sms: 0, email: 0 },
  { day: "Sun", whatsapp: 0, sms: 0, email: 0 },
];

// ── NewCampaignModal ───────────────────────────────────────────────────────────

const CHANNEL_OPTIONS = [
  { value: "whatsapp", label: "WhatsApp",  icon: MessageSquare, color: "#00FF88" },
  { value: "sms",      label: "SMS",       icon: Smartphone,    color: "#00E5FF" },
  { value: "email",    label: "Email",     icon: Mail,          color: "#A855F7" },
  { value: "push",     label: "Push",      icon: Zap,           color: "#FFAB00" },
] as const;

const TRIGGER_OPTIONS = [
  "On: delivered",
  "On: failed delivery",
  "On: out_for_delivery",
  "4h before ETA",
  "30-day inactive",
  "On: 500pts reached",
  "Manual / Scheduled",
];

function NewCampaignModal({ onClose, onCreated }: { onClose: () => void; onCreated?: () => void }) {
  const [name,    setName]    = useState("");
  const [channel, setChannel] = useState<Channel>("whatsapp");
  const [trigger, setTrigger] = useState(TRIGGER_OPTIONS[0]);
  const [message, setMessage] = useState("");
  const [saving,  setSaving]  = useState(false);
  const [done,    setDone]    = useState(false);
  const [error,   setError]   = useState<string | null>(null);

  const charMax = channel === "sms" ? 160 : 1000;

  async function handleCreate() {
    if (!name.trim() || !message.trim()) return;
    setSaving(true);
    setError(null);
    try {
      // Marketing service's CreateCampaignCommand shape: name, description,
      // channel, template{template_id, subject?, variables}, targeting.
      // The `trigger` drop-down is a UX label and is stored as description
      // until the engagement service exposes a distinct trigger catalog.
      const payload: CreateCampaignPayload = {
        name: name.trim(),
        description: trigger,
        channel,
        template: {
          template_id: `inline_${Date.now()}`,
          subject: channel === "email" ? name.trim() : null,
          variables: { body: message.trim() },
        },
        targeting: {
          customer_ids: [],
          estimated_reach: 0,
        },
      };
      await createCampaignsApi().create(payload);
      setDone(true);
      onCreated?.();
    } catch (e) {
      const err = e as { message?: string };
      setError(err?.message ?? "Failed to create campaign");
    } finally {
      setSaving(false);
    }
  }

  return (
    <motion.div
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      className="fixed inset-0 z-50 flex items-center justify-center p-4"
      style={{ background: "rgba(0,0,0,0.75)", backdropFilter: "blur(6px)" }}
      onClick={(e) => e.target === e.currentTarget && onClose()}
    >
      <motion.div
        initial={{ opacity: 0, scale: 0.96, y: 16 }}
        animate={{ opacity: 1, scale: 1, y: 0 }}
        exit={{ opacity: 0, scale: 0.96, y: 8 }}
        transition={{ ease: [0.16, 1, 0.3, 1], duration: 0.3 }}
        className="relative w-full max-w-lg rounded-2xl border border-glass-border p-6 shadow-glass"
        style={{ background: "rgba(8,12,28,0.98)" }}
      >
        {/* Header */}
        <div className="flex items-center justify-between mb-5">
          <div>
            <h2 className="font-heading text-lg font-bold text-white">New Campaign</h2>
            <p className="text-xs text-white/35 mt-0.5 font-mono">Engagement Engine · AI-powered targeting</p>
          </div>
          <button onClick={onClose} className="flex h-8 w-8 items-center justify-center rounded-lg border border-glass-border text-white/40 hover:text-white/80 transition-all">
            <X size={15} />
          </button>
        </div>

        {!done ? (
          <div className="flex flex-col gap-4">
            {/* Name */}
            <div>
              <label className="mb-1.5 block text-xs font-medium text-white/50">Campaign Name</label>
              <input
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="e.g. Post-Delivery Upsell"
                className="w-full rounded-xl border border-glass-border bg-glass-100 px-3.5 py-2.5 text-sm text-white placeholder-white/20 outline-none focus:border-purple-plasma/50 transition-colors"
              />
            </div>

            {/* Channel */}
            <div>
              <label className="mb-1.5 block text-xs font-medium text-white/50">Channel</label>
              <div className="grid grid-cols-4 gap-2">
                {CHANNEL_OPTIONS.map(({ value, label, icon: Icon, color }) => (
                  <button
                    key={value}
                    onClick={() => setChannel(value)}
                    className="flex flex-col items-center gap-1.5 rounded-xl border py-3 text-xs font-medium transition-all"
                    style={{
                      borderColor: channel === value ? `${color}40` : "rgba(255,255,255,0.08)",
                      background:  channel === value ? `${color}0e` : "transparent",
                      color:       channel === value ? color         : "rgba(255,255,255,0.4)",
                    }}
                  >
                    <Icon size={14} />
                    {label}
                  </button>
                ))}
              </div>
            </div>

            {/* Trigger */}
            <div>
              <label className="mb-1.5 block text-xs font-medium text-white/50">Trigger</label>
              <div className="relative">
                <select
                  value={trigger}
                  onChange={(e) => setTrigger(e.target.value)}
                  className="w-full appearance-none rounded-xl border border-glass-border bg-glass-100 px-3.5 py-2.5 pr-9 text-sm text-white outline-none focus:border-purple-plasma/50 transition-colors"
                >
                  {TRIGGER_OPTIONS.map((t) => (
                    <option key={t} value={t} style={{ background: "#0d1422" }}>{t}</option>
                  ))}
                </select>
                <ChevronDown size={13} className="pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 text-white/30" />
              </div>
            </div>

            {/* Message */}
            <div>
              <div className="mb-1.5 flex items-center justify-between">
                <label className="text-xs font-medium text-white/50">Message</label>
                <span className={`text-2xs font-mono ${message.length > charMax ? "text-red-signal" : "text-white/25"}`}>
                  {message.length}/{charMax}
                </span>
              </div>
              <textarea
                value={message}
                onChange={(e) => setMessage(e.target.value)}
                rows={4}
                placeholder={`Hi {{name}}, your order {{awb}} has been delivered! 🎉\n\nBook your next shipment and get 10% off.`}
                className="w-full resize-none rounded-xl border border-glass-border bg-glass-100 px-3.5 py-2.5 text-sm text-white placeholder-white/15 outline-none focus:border-purple-plasma/50 transition-colors font-mono"
              />
              <p className="mt-1 text-2xs text-white/25">Variables: {'{{name}}'}, {'{{awb}}'}, {'{{eta}}'}, {'{{cod_amount}}'}</p>
            </div>

            {error && (
              <p className="rounded-lg border border-red-signal/30 bg-red-signal/10 px-3 py-2 text-xs text-red-signal">
                {error}
              </p>
            )}

            {/* Footer */}
            <div className="flex justify-end gap-2 pt-1">
              <button onClick={onClose} className="rounded-lg border border-glass-border px-4 py-2 text-sm text-white/50 hover:text-white transition-colors">
                Cancel
              </button>
              <button
                onClick={handleCreate}
                disabled={!name.trim() || !message.trim() || message.length > charMax || saving}
                className="flex items-center gap-2 rounded-lg px-5 py-2 text-sm font-semibold text-white transition-all disabled:opacity-40"
                style={{ background: "linear-gradient(135deg, #A855F7, #00E5FF)" }}
              >
                {saving ? (
                  <><span className="h-3.5 w-3.5 animate-spin rounded-full border-2 border-white/30 border-t-white" /> Creating…</>
                ) : (
                  <><Plus size={14} /> Create Campaign</>
                )}
              </button>
            </div>
          </div>
        ) : (
          <div className="flex flex-col items-center gap-4 py-6 text-center">
            <div className="flex h-14 w-14 items-center justify-center rounded-2xl" style={{ background: "rgba(168,85,247,0.12)" }}>
              <CheckCircle2 className="h-7 w-7 text-purple-plasma" />
            </div>
            <div>
              <p className="font-heading text-lg font-bold text-white">Campaign Created</p>
              <p className="text-sm text-white/40 mt-1">"{name}" is now saved as a draft.</p>
            </div>
            <button
              onClick={onClose}
              className="rounded-lg px-6 py-2 text-sm font-semibold text-white"
              style={{ background: "linear-gradient(135deg, #A855F7, #00E5FF)" }}
            >
              Done
            </button>
          </div>
        )}
      </motion.div>
    </motion.div>
  );
}

// ── Page ───────────────────────────────────────────────────────────────────────

function CampaignsContent() {
  const searchParams = useSearchParams();
  const router = useRouter();
  const [showNew, setShowNew] = useState(false);

  const [campaigns, setCampaigns] = useState<Campaign[]>([]);
  const [loading, setLoading]     = useState(true);
  const [error, setError]         = useState<string | null>(null);
  const [mutatingId, setMutatingId] = useState<string | null>(null);

  const api = useMemo(() => createCampaignsApi(), []);

  const load = useCallback(async () => {
    setError(null);
    try {
      const resp = await api.list();
      setCampaigns(resp.campaigns ?? []);
    } catch (e) {
      const err = e as { message?: string };
      setError(err?.message ?? "Failed to load campaigns");
    } finally {
      setLoading(false);
    }
  }, [api]);

  useEffect(() => { load(); }, [load]);

  // Poll while the tab is active — campaigns change status as the engagement
  // service processes sends. 30s is coarse enough to avoid load spikes.
  useEffect(() => {
    const id = setInterval(load, 30_000);
    return () => clearInterval(id);
  }, [load]);

  // Auto-open from dashboard CTA
  useEffect(() => {
    if (searchParams.get("new") === "1") {
      setShowNew(true);
      router.replace("/campaigns");
    }
  }, [searchParams, router]);

  async function handleActivate(id: string) {
    setMutatingId(id);
    try {
      await api.activate(id);
      await load();
    } catch (e) {
      const err = e as { message?: string };
      setError(err?.message ?? "Failed to activate campaign");
    } finally {
      setMutatingId(null);
    }
  }

  async function handleCancel(id: string) {
    setMutatingId(id);
    try {
      await api.cancel(id);
      await load();
    } catch (e) {
      const err = e as { message?: string };
      setError(err?.message ?? "Failed to cancel campaign");
    } finally {
      setMutatingId(null);
    }
  }

  // KPIs derived from live list. Open/conversion rates require per-send analytics
  // (engagement service `/v1/notifications` aggregation) — shown as — until wired.
  const kpis = useMemo(() => {
    const active = campaigns.filter(c => c.status === "sending" || c.status === "scheduled").length;
    const sent   = campaigns.reduce((n, c) => n + (c.total_sent ?? 0), 0);
    const delivered = campaigns.reduce((n, c) => n + (c.total_delivered ?? 0), 0);
    const deliveryRate = sent > 0 ? (delivered / sent) * 100 : 0;
    return [
      { label: "Active Campaigns", value: active,       trend: 0, color: "cyan"   as const, format: "number"  as const },
      { label: "Messages Sent",    value: sent,         trend: 0, color: "purple" as const, format: "number"  as const },
      { label: "Delivery Rate",    value: deliveryRate, trend: 0, color: "green"  as const, format: "percent" as const },
      { label: "Total Campaigns",  value: campaigns.length, trend: 0, color: "amber" as const, format: "number" as const },
    ];
  }, [campaigns]);

  return (
    <>
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
            <Megaphone size={22} className="text-purple-plasma" />
            Campaigns
          </h1>
          <p className="text-sm text-white/40 font-mono mt-0.5">
            Engagement Engine · {kpis[0].value} active, {campaigns.length} total
          </p>
        </div>
        <div className="flex items-center gap-2">
          <button
            onClick={load}
            className="flex items-center gap-1.5 rounded-lg border border-glass-border px-3 py-2 text-xs text-white/60 hover:text-white transition-colors"
            title="Refresh"
          >
            <RefreshCw size={13} />
          </button>
          <button
            onClick={() => setShowNew(true)}
            className="flex items-center gap-1.5 rounded-lg bg-gradient-to-r from-purple-plasma to-cyan-neon px-4 py-2 text-xs font-semibold text-white hover:opacity-90 transition-opacity"
          >
            <Plus size={13} /> New Campaign
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

      {/* KPI row */}
      <motion.div variants={variants.fadeInUp} className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        {kpis.map((m) => (
          <GlassCard key={m.label} size="sm" glow={m.color} accent>
            <LiveMetric label={m.label} value={m.value} trend={m.trend} color={m.color} format={m.format} />
          </GlassCard>
        ))}
      </motion.div>

      {/* Send volume chart */}
      <motion.div variants={variants.fadeInUp}>
        <GlassCard glow="purple">
          <div className="flex items-center justify-between mb-5">
            <div>
              <h2 className="font-heading text-sm font-semibold text-white">Message Volume — This Week</h2>
              <p className="text-2xs font-mono text-white/30">WhatsApp · SMS · Email</p>
            </div>
            <BarChart2 size={15} className="text-purple-plasma" />
          </div>
          <ResponsiveContainer width="100%" height={180}>
            <AreaChart data={SEND_TREND} margin={{ top: 0, right: 0, bottom: 0, left: -24 }}>
              <defs>
                <linearGradient id="grad-wa" x1="0" y1="0" x2="0" y2="1">
                  <stop offset="5%"  stopColor="#00FF88" stopOpacity={0.3} />
                  <stop offset="95%" stopColor="#00FF88" stopOpacity={0}   />
                </linearGradient>
                <linearGradient id="grad-sms" x1="0" y1="0" x2="0" y2="1">
                  <stop offset="5%"  stopColor="#00E5FF" stopOpacity={0.25} />
                  <stop offset="95%" stopColor="#00E5FF" stopOpacity={0}    />
                </linearGradient>
                <linearGradient id="grad-email" x1="0" y1="0" x2="0" y2="1">
                  <stop offset="5%"  stopColor="#A855F7" stopOpacity={0.25} />
                  <stop offset="95%" stopColor="#A855F7" stopOpacity={0}    />
                </linearGradient>
              </defs>
              <CartesianGrid stroke="rgba(255,255,255,0.04)" strokeDasharray="4 4" vertical={false} />
              <XAxis dataKey="day" tick={{ fill: "rgba(255,255,255,0.3)", fontSize: 11, fontFamily: "JetBrains Mono" }} axisLine={false} tickLine={false} />
              <YAxis tick={{ fill: "rgba(255,255,255,0.3)", fontSize: 11, fontFamily: "JetBrains Mono" }} axisLine={false} tickLine={false} />
              <Tooltip
                contentStyle={{ background: "rgba(13,20,34,0.95)", border: "1px solid rgba(255,255,255,0.08)", borderRadius: 8, fontFamily: "JetBrains Mono", fontSize: 11 }}
                labelStyle={{ color: "rgba(255,255,255,0.4)" }}
              />
              <Area type="monotone" dataKey="whatsapp" stroke="#00FF88" fill="url(#grad-wa)"    strokeWidth={2} />
              <Area type="monotone" dataKey="sms"      stroke="#00E5FF" fill="url(#grad-sms)"   strokeWidth={2} />
              <Area type="monotone" dataKey="email"    stroke="#A855F7" fill="url(#grad-email)" strokeWidth={2} />
            </AreaChart>
          </ResponsiveContainer>
        </GlassCard>
      </motion.div>

      {/* Campaign list */}
      <motion.div variants={variants.fadeInUp}>
        <GlassCard padding="none">
          <div className="flex items-center justify-between px-5 py-4 border-b border-glass-border">
            <h2 className="font-heading text-sm font-semibold text-white">All Campaigns</h2>
            <span className="text-2xs font-mono text-white/30">
              {loading ? "loading…" : `${campaigns.length} campaign${campaigns.length === 1 ? "" : "s"}`}
            </span>
          </div>

          {/* Header row */}
          <div className="grid grid-cols-[2fr_80px_100px_80px_100px_1fr_80px] gap-3 px-5 py-2.5 border-b border-glass-border">
            {["Name", "Channel", "Status", "Sent", "Delivered %", "Trigger", ""].map((h) => (
              <span key={h} className="text-2xs font-mono text-white/30 uppercase tracking-wider">{h}</span>
            ))}
          </div>

          {!loading && campaigns.length === 0 && (
            <div className="px-5 py-10 text-center">
              <p className="text-xs text-white/40 font-mono">
                No campaigns yet. Click <span className="text-purple-plasma">New Campaign</span> to create one.
              </p>
            </div>
          )}

          {campaigns.map((c) => {
            const deliveryRate = c.total_sent > 0 ? (c.total_delivered / c.total_sent) * 100 : 0;
            const trigger = c.description?.trim() || "Manual / Scheduled";
            const busy = mutatingId === c.id;
            return (
              <div key={c.id} className="grid grid-cols-[2fr_80px_100px_80px_100px_1fr_80px] gap-3 items-center px-5 py-3.5 border-b border-glass-border/50 hover:bg-glass-100 transition-colors">
                <div>
                  <p className="text-xs font-medium text-white">{c.name}</p>
                  <p className="text-2xs font-mono text-white/30 mt-0.5">
                    {c.total_delivered > 0 ? `${c.total_delivered.toLocaleString()} delivered` : "No sends yet"}
                  </p>
                </div>
                <div className="flex items-center gap-1.5">
                  {CHANNEL_ICON[c.channel]}
                  <span className="text-xs text-white/60 capitalize">{c.channel}</span>
                </div>
                <NeonBadge variant={STATUS_VARIANT[c.status]} dot>{c.status}</NeonBadge>
                <span className="text-xs font-mono text-white/60">
                  {c.total_sent > 0 ? c.total_sent.toLocaleString() : "—"}
                </span>
                <span className={`text-xs font-mono font-semibold ${
                  deliveryRate > 80 ? "text-green-signal" :
                  deliveryRate > 40 ? "text-cyan-neon" :
                  "text-white/40"
                }`}>
                  {c.total_sent > 0 ? `${deliveryRate.toFixed(1)}%` : "—"}
                </span>
                <span className="text-xs text-white/40 font-mono truncate" title={trigger}>{trigger}</span>
                <div className="flex items-center gap-1">
                  {(c.status === "draft" || c.status === "scheduled") && (
                    <button
                      onClick={() => handleActivate(c.id)}
                      disabled={busy}
                      className="rounded p-1.5 text-white/30 hover:text-green-signal hover:bg-glass-200 transition-colors disabled:opacity-40"
                      title="Activate (start sending)"
                    >
                      {busy ? <span className="block h-3 w-3 animate-spin rounded-full border-2 border-white/30 border-t-white" /> : <Play size={12} />}
                    </button>
                  )}
                  {(c.status === "draft" || c.status === "scheduled") && (
                    <button
                      onClick={() => handleCancel(c.id)}
                      disabled={busy}
                      className="rounded p-1.5 text-white/30 hover:text-red-signal hover:bg-glass-200 transition-colors disabled:opacity-40"
                      title="Cancel"
                    >
                      <X size={12} />
                    </button>
                  )}
                </div>
              </div>
            );
          })}
        </GlassCard>
      </motion.div>
    </motion.div>

    {/* New Campaign Modal */}
    <AnimatePresence>
      {showNew && (
        <NewCampaignModal
          onClose={() => setShowNew(false)}
          onCreated={load}
        />
      )}
    </AnimatePresence>
    </>
  );
}

export default function CampaignsPage() {
  return (
    <Suspense>
      <CampaignsContent />
    </Suspense>
  );
}
