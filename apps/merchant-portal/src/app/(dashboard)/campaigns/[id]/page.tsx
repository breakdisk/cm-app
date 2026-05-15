"use client";
/**
 * Merchant Portal — Campaign Detail Page
 * Route: /campaigns/[id]
 *
 * Data flow:
 *   GET /v1/campaigns/:id  → marketing::get  (single campaign with full stats)
 *
 * Shows: status badge, channel, trigger/description, message body, delivery
 * metrics tiles (sent / delivered / failed / delivery-rate), and timestamps.
 * Action buttons mirror the list page: Activate, Cancel.
 */
import { useEffect, useState, useCallback } from "react";
import { useParams, useRouter } from "next/navigation";
import { motion } from "framer-motion";
import { variants } from "@/lib/design-system/tokens";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import {
  ArrowLeft, Megaphone, MessageSquare, Mail, Smartphone, Zap,
  Play, X, RefreshCw, CheckCircle2, Clock, Send, AlertTriangle,
  BarChart2, User, Calendar,
} from "lucide-react";
import {
  createCampaignsApi,
  type Campaign,
  type Channel,
  type CampaignStatus,
} from "@/lib/api/campaigns";

// ── Helpers ────────────────────────────────────────────────────────────────────

const CHANNEL_ICON: Record<Channel, React.ReactNode> = {
  whatsapp: <MessageSquare size={14} className="text-green-signal" />,
  sms:      <Smartphone    size={14} className="text-cyan-neon"    />,
  email:    <Mail          size={14} className="text-purple-plasma" />,
  push:     <Zap           size={14} className="text-amber-signal" />,
};

const CHANNEL_LABEL: Record<Channel, string> = {
  whatsapp: "WhatsApp",
  sms:      "SMS",
  email:    "Email",
  push:     "Push",
};

const STATUS_VARIANT: Record<CampaignStatus, "green" | "amber" | "purple" | "red" | "cyan"> = {
  draft:     "purple",
  scheduled: "cyan",
  sending:   "green",
  completed: "green",
  cancelled: "amber",
  failed:    "red",
};

function fmt(n: number) { return n.toLocaleString(); }
function fmtDate(iso?: string | null) {
  if (!iso) return "—";
  return new Date(iso).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
}

// ── Metric tile ────────────────────────────────────────────────────────────────

interface MetricTileProps {
  label:   string;
  value:   string;
  icon:    React.ReactNode;
  color:   "cyan" | "green" | "purple" | "amber" | "red";
}

const GLOW: Record<MetricTileProps["color"], string> = {
  cyan:   "rgba(0,229,255,0.08)",
  green:  "rgba(0,255,136,0.08)",
  purple: "rgba(168,85,247,0.08)",
  amber:  "rgba(255,171,0,0.08)",
  red:    "rgba(255,77,77,0.08)",
};

const TEXT: Record<MetricTileProps["color"], string> = {
  cyan:   "text-cyan-neon",
  green:  "text-green-signal",
  purple: "text-purple-plasma",
  amber:  "text-amber-signal",
  red:    "text-red-signal",
};

function MetricTile({ label, value, icon, color }: MetricTileProps) {
  return (
    <div
      className="flex flex-col gap-2 rounded-xl border border-glass-border p-4"
      style={{ background: GLOW[color] }}
    >
      <div className="flex items-center gap-1.5 text-white/40">
        {icon}
        <span className="text-2xs font-mono uppercase tracking-wider">{label}</span>
      </div>
      <p className={`font-heading text-2xl font-bold ${TEXT[color]}`}>{value}</p>
    </div>
  );
}

// ── Detail page ────────────────────────────────────────────────────────────────

export default function CampaignDetailPage() {
  const { id } = useParams<{ id: string }>();
  const router  = useRouter();

  const [campaign,   setCampaign]   = useState<Campaign | null>(null);
  const [loading,    setLoading]    = useState(true);
  const [error,      setError]      = useState<string | null>(null);
  const [mutating,   setMutating]   = useState(false);
  const [actionDone, setActionDone] = useState<"activated" | "cancelled" | null>(null);

  const api = createCampaignsApi();

  const load = useCallback(async () => {
    setError(null);
    try {
      const c = await api.get(id);
      setCampaign(c);
    } catch (e) {
      const err = e as { message?: string };
      setError(err?.message ?? "Failed to load campaign");
    } finally {
      setLoading(false);
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [id]);

  useEffect(() => { load(); }, [load]);

  async function handleActivate() {
    if (!campaign) return;
    setMutating(true);
    try {
      await api.activate(campaign.id);
      setActionDone("activated");
      await load();
    } catch (e) {
      const err = e as { message?: string };
      setError(err?.message ?? "Failed to activate campaign");
    } finally {
      setMutating(false);
    }
  }

  async function handleCancel() {
    if (!campaign) return;
    setMutating(true);
    try {
      await api.cancel(campaign.id);
      setActionDone("cancelled");
      await load();
    } catch (e) {
      const err = e as { message?: string };
      setError(err?.message ?? "Failed to cancel campaign");
    } finally {
      setMutating(false);
    }
  }

  // ── Derived values ───────────────────────────────────────────────────────────
  const deliveryRate = campaign && campaign.total_sent > 0
    ? (campaign.total_delivered / campaign.total_sent) * 100
    : 0;
  const body = campaign?.template?.variables?.body as string | undefined;
  const trigger = campaign?.description?.trim() || "Manual / Scheduled";

  // ── Render ───────────────────────────────────────────────────────────────────
  return (
    <motion.div
      variants={variants.staggerContainer}
      initial="hidden"
      animate="visible"
      className="flex flex-col gap-5 p-6 max-w-4xl mx-auto"
    >
      {/* Back + header */}
      <motion.div variants={variants.fadeInUp} className="flex items-start gap-3">
        <button
          onClick={() => router.push("/campaigns")}
          className="mt-0.5 flex h-8 w-8 shrink-0 items-center justify-center rounded-lg border border-glass-border text-white/40 hover:text-white transition-colors"
        >
          <ArrowLeft size={15} />
        </button>
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2 flex-wrap">
            <Megaphone size={18} className="text-purple-plasma shrink-0" />
            <h1 className="font-heading text-xl font-bold text-white truncate">
              {loading ? "Loading…" : campaign?.name ?? "Campaign"}
            </h1>
            {campaign && (
              <NeonBadge variant={STATUS_VARIANT[campaign.status]} dot>
                {campaign.status}
              </NeonBadge>
            )}
          </div>
          {campaign && (
            <p className="text-xs text-white/40 font-mono mt-1">
              ID: {campaign.id}
            </p>
          )}
        </div>

        {/* Actions */}
        {campaign && (
          <div className="flex items-center gap-2 shrink-0">
            <button
              onClick={load}
              className="flex items-center gap-1.5 rounded-lg border border-glass-border px-3 py-2 text-xs text-white/60 hover:text-white transition-colors"
              title="Refresh"
            >
              <RefreshCw size={13} />
            </button>
            {(campaign.status === "draft" || campaign.status === "scheduled") && (
              <>
                <button
                  onClick={handleActivate}
                  disabled={mutating}
                  className="flex items-center gap-1.5 rounded-lg border border-green-signal/30 bg-green-signal/10 px-3 py-2 text-xs font-semibold text-green-signal hover:bg-green-signal/20 transition-colors disabled:opacity-40"
                >
                  {mutating
                    ? <span className="block h-3 w-3 animate-spin rounded-full border-2 border-green-signal/30 border-t-green-signal" />
                    : <Play size={12} />}
                  Activate
                </button>
                <button
                  onClick={handleCancel}
                  disabled={mutating}
                  className="flex items-center gap-1.5 rounded-lg border border-red-signal/30 bg-red-signal/10 px-3 py-2 text-xs font-semibold text-red-signal hover:bg-red-signal/20 transition-colors disabled:opacity-40"
                >
                  <X size={12} />
                  Cancel
                </button>
              </>
            )}
          </div>
        )}
      </motion.div>

      {/* Error / action toast */}
      {error && (
        <motion.div variants={variants.fadeInUp}>
          <GlassCard padding="sm">
            <p className="text-xs text-red-signal font-mono">{error}</p>
          </GlassCard>
        </motion.div>
      )}
      {actionDone && (
        <motion.div
          variants={variants.fadeInUp}
          initial={{ opacity: 0, y: -8 }}
          animate={{ opacity: 1, y: 0 }}
        >
          <div className="flex items-center gap-2 rounded-xl border border-green-signal/30 bg-green-signal/10 px-4 py-3">
            <CheckCircle2 size={14} className="text-green-signal" />
            <p className="text-xs font-medium text-green-signal font-mono">
              Campaign {actionDone === "activated" ? "activated — sending in progress" : "cancelled"}.
            </p>
          </div>
        </motion.div>
      )}

      {loading && (
        <motion.div variants={variants.fadeInUp} className="flex justify-center py-16">
          <span className="h-6 w-6 animate-spin rounded-full border-2 border-white/10 border-t-purple-plasma" />
        </motion.div>
      )}

      {campaign && (
        <>
          {/* Metrics */}
          <motion.div variants={variants.fadeInUp} className="grid grid-cols-2 gap-3 sm:grid-cols-4">
            <MetricTile label="Sent"          value={fmt(campaign.total_sent)}      icon={<Send size={12} />}         color="cyan"   />
            <MetricTile label="Delivered"     value={fmt(campaign.total_delivered)} icon={<CheckCircle2 size={12} />} color="green"  />
            <MetricTile label="Failed"        value={fmt(campaign.total_failed)}    icon={<AlertTriangle size={12} />} color={campaign.total_failed > 0 ? "red" : "amber"} />
            <MetricTile
              label="Delivery Rate"
              value={campaign.total_sent > 0 ? `${deliveryRate.toFixed(1)}%` : "—"}
              icon={<BarChart2 size={12} />}
              color={deliveryRate > 80 ? "green" : deliveryRate > 40 ? "cyan" : "amber"}
            />
          </motion.div>

          {/* Overview card */}
          <motion.div variants={variants.fadeInUp}>
            <GlassCard>
              <h2 className="font-heading text-sm font-semibold text-white mb-4">Campaign Details</h2>
              <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">

                {/* Channel */}
                <div className="flex items-start gap-3">
                  <div className="mt-0.5 flex h-7 w-7 shrink-0 items-center justify-center rounded-lg border border-glass-border bg-glass-100">
                    {CHANNEL_ICON[campaign.channel]}
                  </div>
                  <div>
                    <p className="text-2xs font-mono text-white/30 uppercase tracking-wider mb-0.5">Channel</p>
                    <p className="text-sm font-medium text-white">{CHANNEL_LABEL[campaign.channel]}</p>
                  </div>
                </div>

                {/* Trigger */}
                <div className="flex items-start gap-3">
                  <div className="mt-0.5 flex h-7 w-7 shrink-0 items-center justify-center rounded-lg border border-glass-border bg-glass-100">
                    <Zap size={13} className="text-amber-signal" />
                  </div>
                  <div>
                    <p className="text-2xs font-mono text-white/30 uppercase tracking-wider mb-0.5">Trigger</p>
                    <p className="text-sm font-medium text-white">{trigger}</p>
                  </div>
                </div>

                {/* Recipients */}
                <div className="flex items-start gap-3">
                  <div className="mt-0.5 flex h-7 w-7 shrink-0 items-center justify-center rounded-lg border border-glass-border bg-glass-100">
                    <User size={13} className="text-cyan-neon" />
                  </div>
                  <div>
                    <p className="text-2xs font-mono text-white/30 uppercase tracking-wider mb-0.5">Estimated Reach</p>
                    <p className="text-sm font-medium text-white">
                      {fmt(campaign.targeting?.estimated_reach ?? 0)} recipient{(campaign.targeting?.estimated_reach ?? 0) !== 1 ? "s" : ""}
                    </p>
                  </div>
                </div>

                {/* Created by */}
                <div className="flex items-start gap-3">
                  <div className="mt-0.5 flex h-7 w-7 shrink-0 items-center justify-center rounded-lg border border-glass-border bg-glass-100">
                    <User size={13} className="text-purple-plasma" />
                  </div>
                  <div>
                    <p className="text-2xs font-mono text-white/30 uppercase tracking-wider mb-0.5">Created By</p>
                    <p className="text-sm font-mono text-white/70 truncate">{campaign.created_by}</p>
                  </div>
                </div>
              </div>
            </GlassCard>
          </motion.div>

          {/* Message body */}
          {body && (
            <motion.div variants={variants.fadeInUp}>
              <GlassCard>
                <h2 className="font-heading text-sm font-semibold text-white mb-3">Message</h2>
                <pre
                  className="whitespace-pre-wrap rounded-xl border border-glass-border bg-glass-100 px-4 py-3 font-mono text-xs text-white/70 leading-relaxed"
                >
                  {body}
                </pre>
                <p className="mt-2 text-2xs text-white/25 font-mono">
                  Template ID: {campaign.template.template_id}
                </p>
              </GlassCard>
            </motion.div>
          )}

          {/* Timeline */}
          <motion.div variants={variants.fadeInUp}>
            <GlassCard>
              <h2 className="font-heading text-sm font-semibold text-white mb-4">Timeline</h2>
              <div className="flex flex-col gap-3">
                {[
                  { label: "Created",    value: fmtDate(campaign.created_at),   icon: <Calendar   size={12} />, show: true },
                  { label: "Scheduled",  value: fmtDate(campaign.scheduled_at), icon: <Clock      size={12} />, show: !!campaign.scheduled_at },
                  { label: "Sent",       value: fmtDate(campaign.sent_at),      icon: <Send       size={12} />, show: !!campaign.sent_at },
                  { label: "Completed",  value: fmtDate(campaign.completed_at), icon: <CheckCircle2 size={12} />, show: !!campaign.completed_at },
                ].filter((r) => r.show).map((row) => (
                  <div key={row.label} className="flex items-center gap-3">
                    <div className="flex h-6 w-6 shrink-0 items-center justify-center rounded-md border border-glass-border bg-glass-100 text-white/40">
                      {row.icon}
                    </div>
                    <span className="w-20 text-2xs font-mono text-white/30 uppercase tracking-wider shrink-0">{row.label}</span>
                    <span className="text-xs font-mono text-white/60">{row.value}</span>
                  </div>
                ))}
              </div>
            </GlassCard>
          </motion.div>
        </>
      )}
    </motion.div>
  );
}
