"use client";
/**
 * Merchant Portal — Campaigns Page
 * Marketing automation: active campaigns, performance, campaign builder CTA.
 */
import { useState, useEffect, Suspense } from "react";
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
  Megaphone, Plus, Zap, MessageSquare, Mail, Smartphone, Play, Pause,
  BarChart2, X, ChevronDown, CheckCircle2,
} from "lucide-react";

// ── Mock data ──────────────────────────────────────────────────────────────────

const KPI_METRICS = [
  { label: "Active Campaigns",  value: 7,      trend: +2,    color: "cyan"   as const, format: "number"  as const },
  { label: "Messages Sent MTD", value: 84200,  trend: +31.4, color: "purple" as const, format: "number"  as const },
  { label: "Avg Open Rate",     value: 38.7,   trend: +4.2,  color: "green"  as const, format: "percent" as const },
  { label: "Conversions MTD",   value: 1840,   trend: +22.1, color: "amber"  as const, format: "number"  as const },
];

const SEND_TREND = [
  { day: "Mon", whatsapp: 1400, sms: 820,  email: 340 },
  { day: "Tue", whatsapp: 1620, sms: 910,  email: 410 },
  { day: "Wed", whatsapp: 1380, sms: 780,  email: 370 },
  { day: "Thu", whatsapp: 1750, sms: 960,  email: 490 },
  { day: "Fri", whatsapp: 2100, sms: 1140, email: 580 },
  { day: "Sat", whatsapp: 1560, sms: 870,  email: 310 },
  { day: "Sun", whatsapp: 1200, sms: 640,  email: 220 },
];

type CampaignStatus = "active" | "paused" | "draft" | "completed";

interface Campaign {
  id: string;
  name: string;
  type: "whatsapp" | "sms" | "email" | "push";
  status: CampaignStatus;
  sent: number;
  open_rate: number;
  conversions: number;
  trigger: string;
}

const CAMPAIGNS: Campaign[] = [
  { id: "1", name: "Post-Delivery Upsell",    type: "whatsapp", status: "active",    sent: 12400, open_rate: 44.2, conversions: 380, trigger: "On: delivered"      },
  { id: "2", name: "Delivery ETA Reminder",   type: "sms",      status: "active",    sent: 8200,  open_rate: 71.3, conversions: 0,   trigger: "4h before ETA"      },
  { id: "3", name: "Failed Delivery Rescue",  type: "whatsapp", status: "active",    sent: 1840,  open_rate: 58.4, conversions: 290, trigger: "On: failed delivery" },
  { id: "4", name: "Win-back Lapsed Senders", type: "email",    status: "active",    sent: 4100,  open_rate: 22.8, conversions: 64,  trigger: "30-day inactive"     },
  { id: "5", name: "Balikbayan Box Promo",    type: "push",     status: "paused",    sent: 6300,  open_rate: 31.0, conversions: 210, trigger: "Manual / Scheduled"  },
  { id: "6", name: "Loyalty Points Reminder", type: "sms",      status: "draft",     sent: 0,     open_rate: 0,    conversions: 0,   trigger: "On: 500pts reached"  },
  { id: "7", name: "Merchant Re-engagement",  type: "email",    status: "completed", sent: 3800,  open_rate: 19.4, conversions: 41,  trigger: "Manual blast"        },
];

const CHANNEL_ICON: Record<Campaign["type"], React.ReactNode> = {
  whatsapp: <MessageSquare size={12} className="text-green-signal" />,
  sms:      <Smartphone    size={12} className="text-cyan-neon"    />,
  email:    <Mail          size={12} className="text-purple-plasma" />,
  push:     <Zap           size={12} className="text-amber-signal" />,
};

const STATUS_VARIANT: Record<CampaignStatus, "green" | "amber" | "purple" | "red"> = {
  active:    "green",
  paused:    "amber",
  draft:     "purple",
  completed: "red",
};

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
  const [channel, setChannel] = useState<"whatsapp" | "sms" | "email" | "push">("whatsapp");
  const [trigger, setTrigger] = useState(TRIGGER_OPTIONS[0]);
  const [message, setMessage] = useState("");
  const [saving,  setSaving]  = useState(false);
  const [done,    setDone]    = useState(false);

  const charMax = channel === "sms" ? 160 : 1000;

  async function handleCreate() {
    if (!name.trim() || !message.trim()) return;
    setSaving(true);
    // Wire to POST /v1/campaigns in production
    await new Promise((r) => setTimeout(r, 1000));
    setSaving(false);
    setDone(true);
    onCreated?.();
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

  // Auto-open from dashboard CTA
  useEffect(() => {
    if (searchParams.get("new") === "1") {
      setShowNew(true);
      router.replace("/campaigns");
    }
  }, [searchParams, router]);

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
          <p className="text-sm text-white/40 font-mono mt-0.5">Engagement Engine · 7 active automations</p>
        </div>
        <button
          onClick={() => setShowNew(true)}
          className="flex items-center gap-1.5 rounded-lg bg-gradient-to-r from-purple-plasma to-cyan-neon px-4 py-2 text-xs font-semibold text-white hover:opacity-90 transition-opacity"
        >
          <Plus size={13} /> New Campaign
        </button>
      </motion.div>

      {/* KPI row */}
      <motion.div variants={variants.fadeInUp} className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        {KPI_METRICS.map((m) => (
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
            <span className="text-2xs font-mono text-white/30">{CAMPAIGNS.length} campaigns</span>
          </div>

          {/* Header row */}
          <div className="grid grid-cols-[2fr_80px_80px_80px_80px_1fr_80px] gap-3 px-5 py-2.5 border-b border-glass-border">
            {["Name", "Channel", "Status", "Sent", "Open %", "Trigger", ""].map((h) => (
              <span key={h} className="text-2xs font-mono text-white/30 uppercase tracking-wider">{h}</span>
            ))}
          </div>

          {CAMPAIGNS.map((c) => (
            <div key={c.id} className="grid grid-cols-[2fr_80px_80px_80px_80px_1fr_80px] gap-3 items-center px-5 py-3.5 border-b border-glass-border/50 hover:bg-glass-100 transition-colors">
              <div>
                <p className="text-xs font-medium text-white">{c.name}</p>
                <p className="text-2xs font-mono text-white/30 mt-0.5">{c.conversions > 0 ? `${c.conversions} conversions` : "No conversions yet"}</p>
              </div>
              <div className="flex items-center gap-1.5">
                {CHANNEL_ICON[c.type]}
                <span className="text-xs text-white/60 capitalize">{c.type}</span>
              </div>
              <NeonBadge variant={STATUS_VARIANT[c.status]} dot>{c.status}</NeonBadge>
              <span className="text-xs font-mono text-white/60">{c.sent > 0 ? c.sent.toLocaleString() : "—"}</span>
              <span className={`text-xs font-mono font-semibold ${c.open_rate > 40 ? "text-green-signal" : c.open_rate > 20 ? "text-cyan-neon" : "text-white/40"}`}>
                {c.open_rate > 0 ? `${c.open_rate}%` : "—"}
              </span>
              <span className="text-xs text-white/40 font-mono">{c.trigger}</span>
              <div className="flex items-center gap-1">
                {c.status === "active" && (
                  <button className="rounded p-1.5 text-white/30 hover:text-amber-signal hover:bg-glass-200 transition-colors" title="Pause">
                    <Pause size={12} />
                  </button>
                )}
                {c.status === "paused" && (
                  <button className="rounded p-1.5 text-white/30 hover:text-green-signal hover:bg-glass-200 transition-colors" title="Resume">
                    <Play size={12} />
                  </button>
                )}
              </div>
            </div>
          ))}
        </GlassCard>
      </motion.div>
    </motion.div>

    {/* New Campaign Modal */}
    <AnimatePresence>
      {showNew && (
        <NewCampaignModal onClose={() => setShowNew(false)} />
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
