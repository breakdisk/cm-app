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
  BarChart2, User, Calendar, Link2,
} from "lucide-react";
import {
  createCampaignsApi,
  createAbTestApi,
  type Campaign,
  type Channel,
  type CampaignStatus,
  type AbTestWithStats,
} from "@/lib/api/campaigns";

// ── Social channel SVG icons ───────────────────────────────────────────────────

const MessengerIcon = ({ size = 14 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="currentColor">
    <path d="M12 2C6.477 2 2 6.145 2 11.243c0 2.928 1.373 5.55 3.528 7.3V22l3.375-1.85c1.267.35 2.602.35 3.097.35 5.523 0 10-4.145 10-9.257C22 6.145 17.523 2 12 2zm.012 14.47L9.56 13.8l-4.9 2.67 5.395-5.73 2.463 2.67 4.9-2.67-5.406 5.73z"/>
  </svg>
);

const TelegramIcon = ({ size = 14 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="currentColor">
    <path d="M12 0C5.373 0 0 5.373 0 12s5.373 12 12 12 12-5.373 12-12S18.627 0 12 0zm5.894 8.221-1.97 9.28c-.145.658-.537.818-1.084.508l-3-2.21-1.447 1.394c-.16.16-.295.295-.605.295l.213-3.053 5.56-5.023c.242-.213-.054-.333-.373-.12l-6.871 4.326-2.962-.924c-.643-.204-.657-.643.136-.953l11.57-4.463c.535-.194 1.003.131.833.943z"/>
  </svg>
);

const XSocialIcon = ({ size = 14 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="currentColor">
    <path d="M18.244 2.25h3.308l-7.227 8.26 8.502 11.24H16.17l-5.214-6.817L4.99 21.75H1.68l7.73-8.835L1.254 2.25H8.08l4.713 6.231zm-1.161 17.52h1.833L7.084 4.126H5.117z"/>
  </svg>
);

const ViberIcon = ({ size = 14 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="currentColor">
    <path d="M11.4 0C5.3 0 .5 4.8.5 10.8c0 3.5 1.6 6.5 4.1 8.5v3.2l3-1.7c1.1.3 2.2.5 3.4.5 6.1 0 10.8-4.8 10.8-10.8.1-5.9-4.7-10.5-10.4-10.5zm.5 16.9c-1 0-1.9-.2-2.8-.5l-2.6 1.5v-2.6c-2-1.4-3.3-3.7-3.3-6.3 0-4.3 3.9-7.8 8.7-7.8s8.7 3.5 8.7 7.8c.1 4.4-3.8 7.9-8.7 7.9zm4-8.4c-.2-.1-1.5-.7-1.7-.8-.2-.1-.4-.1-.5.1-.2.2-.6.8-.8 1-.1.2-.3.2-.5.1-.7-.3-1.4-.7-2-1.2-.5-.5-.9-1-.8-1.5l.2-.5c-.2-.3-.5-1.6-.7-2.2-.2-.5-.4-.5-.5-.5h-.5c-.2 0-.5.1-.7.3-.2.2-.8.8-.8 1.9s.9 2.2 1 2.3c.1.1 1.7 2.6 4.2 3.5.6.2 1 .3 1.4.4.6.1 1.1.1 1.5.1.5-.1 1.4-.6 1.6-1.1.2-.5.2-1 .1-1.1-.1-.1-.3-.2-.4-.3z"/>
  </svg>
);

const WeChatIcon = ({ size = 14 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="currentColor">
    <path d="M8.691 2.188C3.891 2.188 0 5.476 0 9.53c0 2.212 1.17 4.203 3.002 5.55a.59.59 0 0 1 .213.665l-.39 1.48c-.019.07-.048.141-.048.213 0 .163.13.295.29.295a.326.326 0 0 0 .167-.054l1.903-1.114a.864.864 0 0 1 .717-.098 10.16 10.16 0 0 0 2.837.403c.276 0 .543-.027.811-.05-.857-2.578.157-4.972 1.932-6.446 1.703-1.415 3.882-1.98 5.853-1.838-.576-3.583-3.898-6.348-7.596-6.348zM5.785 5.991c.642 0 1.162.529 1.162 1.18a1.17 1.17 0 0 1-1.162 1.178A1.17 1.17 0 0 1 4.623 7.17c0-.651.52-1.18 1.162-1.18zm5.813 0c.642 0 1.162.529 1.162 1.18a1.17 1.17 0 0 1-1.162 1.178 1.17 1.17 0 0 1-1.162-1.178c0-.651.52-1.18 1.162-1.18zm3.34 2.867c-1.797-.052-3.746.512-5.28 1.786-1.72 1.428-2.687 3.72-1.78 6.22.942 2.453 3.666 4.229 6.884 4.229.826 0 1.622-.12 2.361-.336a.722.722 0 0 1 .598.082l1.584.926a.272.272 0 0 0 .14.047c.134 0 .24-.11.24-.247 0-.06-.024-.12-.04-.177l-.327-1.233a.49.49 0 0 1 .176-.554 5.77 5.77 0 0 0 2.5-4.627c0-3.545-3.136-6.116-6.056-6.116zm-2.35 3.495c.535 0 .97.44.97.982 0 .542-.435.982-.97.982s-.97-.44-.97-.982c0-.542.435-.982.97-.982zm4.696 0c.535 0 .97.44.97.982 0 .542-.435.982-.97.982s-.97-.44-.97-.982c0-.542.435-.982.97-.982z"/>
  </svg>
);

const LineIcon = ({ size = 14 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="currentColor">
    <path d="M19.365 9.863c.349 0 .63.285.63.631 0 .345-.281.63-.63.63H17.61v1.125h1.755c.349 0 .63.283.63.63 0 .344-.281.629-.63.629h-2.386c-.345 0-.627-.285-.627-.629V8.108c0-.345.282-.63.63-.63h2.386c.346 0 .627.285.627.63 0 .349-.281.63-.63.63H17.61v1.125h1.755zm-3.855 3.016c0 .27-.174.51-.432.596-.064.021-.133.031-.199.031-.211 0-.391-.09-.51-.25l-2.443-3.317v2.94c0 .344-.279.629-.631.629-.346 0-.626-.285-.626-.629V8.108c0-.27.173-.51.43-.595.06-.023.136-.033.194-.033.195 0 .375.104.495.254l2.462 3.33V8.108c0-.345.282-.63.63-.63.345 0 .63.285.63.63v4.771zm-5.741 0c0 .344-.282.629-.631.629-.345 0-.627-.285-.627-.629V8.108c0-.345.282-.63.63-.63.346 0 .628.285.628.63v4.771zm-2.466.629H4.917c-.345 0-.63-.285-.63-.629V8.108c0-.345.285-.63.63-.63.348 0 .63.285.63.63v4.141h1.756c.348 0 .629.283.629.63 0 .344-.281.629-.629.629M24 10.314C24 4.943 18.615.572 12 .572S0 4.943 0 10.314c0 4.811 4.27 8.842 10.035 9.608.391.082.923.258 1.058.59.12.301.079.766.038 1.08l-.164 1.02c-.045.301-.24 1.186 1.049.645 1.291-.539 6.916-4.078 9.436-6.975C23.176 14.393 24 12.458 24 10.314"/>
  </svg>
);

const SlackIcon = ({ size = 14 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="currentColor">
    <path d="M5.042 15.165a2.528 2.528 0 0 1-2.52 2.523A2.528 2.528 0 0 1 0 15.165a2.527 2.527 0 0 1 2.522-2.52h2.52v2.52zM6.313 15.165a2.527 2.527 0 0 1 2.521-2.52 2.527 2.527 0 0 1 2.521 2.52v6.313A2.528 2.528 0 0 1 8.834 24a2.528 2.528 0 0 1-2.521-2.522v-6.313zM8.834 5.042a2.528 2.528 0 0 1-2.521-2.52A2.528 2.528 0 0 1 8.834 0a2.528 2.528 0 0 1 2.521 2.522v2.52H8.834zM8.834 6.313a2.528 2.528 0 0 1 2.521 2.521 2.528 2.528 0 0 1-2.521 2.521H2.522A2.528 2.528 0 0 1 0 8.834a2.528 2.528 0 0 1 2.522-2.521h6.312zM18.956 8.834a2.528 2.528 0 0 1 2.522-2.521A2.528 2.528 0 0 1 24 8.834a2.528 2.528 0 0 1-2.522 2.521h-2.522V8.834zM17.688 8.834a2.528 2.528 0 0 1-2.523 2.521 2.527 2.527 0 0 1-2.52-2.521V2.522A2.527 2.527 0 0 1 15.165 0a2.528 2.528 0 0 1 2.523 2.522v6.312zM15.165 18.956a2.528 2.528 0 0 1 2.523 2.522A2.528 2.528 0 0 1 15.165 24a2.527 2.527 0 0 1-2.52-2.522v-2.522h2.52zM15.165 17.688a2.527 2.527 0 0 1-2.52-2.523 2.526 2.526 0 0 1 2.52-2.52h6.313A2.527 2.527 0 0 1 24 15.165a2.528 2.528 0 0 1-2.522 2.523h-6.313z"/>
  </svg>
);

// ── Helpers ────────────────────────────────────────────────────────────────────

const SOCIAL_CHANNELS: Channel[] = ["messenger", "telegram", "x", "viber", "wechat", "line", "slack"];

const CHANNEL_COLOR: Record<Channel, string> = {
  whatsapp:  "#00FF88",
  sms:       "#00E5FF",
  email:     "#A855F7",
  push:      "#FFAB00",
  messenger: "#0099FF",
  telegram:  "#26A5E4",
  x:         "#E7E9EA",
  viber:     "#7360F2",
  wechat:    "#07C160",
  line:      "#06C755",
  slack:     "#E01E5A",
};

const CHANNEL_ICON: Record<Channel, React.ReactNode> = {
  whatsapp:  <MessageSquare size={14} className="text-green-signal"  />,
  sms:       <Smartphone    size={14} className="text-cyan-neon"     />,
  email:     <Mail          size={14} className="text-purple-plasma" />,
  push:      <Zap           size={14} className="text-amber-signal"  />,
  messenger: <span className="text-[#0099FF]"><MessengerIcon size={14} /></span>,
  telegram:  <span className="text-[#26A5E4]"><TelegramIcon  size={14} /></span>,
  x:         <span className="text-[#E7E9EA]"><XSocialIcon   size={14} /></span>,
  viber:     <span className="text-[#7360F2]"><ViberIcon     size={14} /></span>,
  wechat:    <span className="text-[#07C160]"><WeChatIcon    size={14} /></span>,
  line:      <span className="text-[#06C755]"><LineIcon      size={14} /></span>,
  slack:     <span className="text-[#E01E5A]"><SlackIcon     size={14} /></span>,
};

const CHANNEL_LABEL: Record<Channel, string> = {
  whatsapp:  "WhatsApp",
  sms:       "SMS",
  email:     "Email",
  push:      "Push",
  messenger: "Messenger",
  telegram:  "Telegram",
  x:         "X",
  viber:     "Viber",
  wechat:    "WeChat",
  line:      "Line",
  slack:     "Slack",
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
  const [abData,     setAbData]     = useState<AbTestWithStats | null>(null);
  const [loading,    setLoading]    = useState(true);
  const [error,      setError]      = useState<string | null>(null);
  const [mutating,   setMutating]   = useState(false);
  const [actionDone, setActionDone] = useState<"activated" | "cancelled" | null>(null);
  const [selectingWinner, setSelectingWinner] = useState(false);

  const api     = createCampaignsApi();
  const abApi   = createAbTestApi();

  const load = useCallback(async () => {
    setError(null);
    try {
      const c = await api.get(id);
      setCampaign(c);
      // Try loading A/B test data — not all campaigns have one
      try {
        const ab = await abApi.get(id);
        setAbData(ab);
      } catch {
        setAbData(null);
      }
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

  async function handleSelectWinner(variant: string) {
    if (!campaign) return;
    setSelectingWinner(true);
    try {
      await abApi.selectWinner(campaign.id, variant);
      const ab = await abApi.get(campaign.id);
      setAbData(ab);
    } catch (e) {
      const err = e as { message?: string };
      setError(err?.message ?? "Failed to select winner");
    } finally {
      setSelectingWinner(false);
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
  const body    = campaign?.template?.variables?.body as string | undefined;
  const subject = campaign?.template?.subject;
  const deepLink = campaign?.template?.variables?.deep_link as string | undefined;
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
          <GlassCard>
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
                    <div className="flex items-center gap-2">
                      <p className="text-sm font-medium text-white">{CHANNEL_LABEL[campaign.channel]}</p>
                      {SOCIAL_CHANNELS.includes(campaign.channel) && (
                        <span
                          className="flex items-center gap-0.5 rounded px-1 py-0.5"
                          style={{ fontSize: 9, fontFamily: "JetBrains Mono, monospace", background: `${CHANNEL_COLOR[campaign.channel]}12`, color: `${CHANNEL_COLOR[campaign.channel]}90`, border: `1px solid ${CHANNEL_COLOR[campaign.channel]}28` }}
                        >
                          <Link2 size={8} />
                          CRM
                        </span>
                      )}
                    </div>
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

                {/* Subject (email) / Push Title */}
                {subject && (campaign.channel === "email" || campaign.channel === "push") && (
                  <div className="flex items-start gap-3">
                    <div className="mt-0.5 flex h-7 w-7 shrink-0 items-center justify-center rounded-lg border border-glass-border bg-glass-100">
                      {campaign.channel === "email"
                        ? <Mail size={13} className="text-purple-plasma" />
                        : <Zap  size={13} className="text-amber-signal" />}
                    </div>
                    <div>
                      <p className="text-2xs font-mono text-white/30 uppercase tracking-wider mb-0.5">
                        {campaign.channel === "email" ? "Subject Line" : "Push Title"}
                      </p>
                      <p className="text-sm font-medium text-white">{subject}</p>
                    </div>
                  </div>
                )}

                {/* Deep Link (push only) */}
                {deepLink && campaign.channel === "push" && (
                  <div className="flex items-start gap-3">
                    <div className="mt-0.5 flex h-7 w-7 shrink-0 items-center justify-center rounded-lg border border-glass-border bg-glass-100">
                      <Zap size={13} className="text-amber-signal/60" />
                    </div>
                    <div>
                      <p className="text-2xs font-mono text-white/30 uppercase tracking-wider mb-0.5">Deep Link</p>
                      <p className="text-sm font-mono text-white/70">{deepLink}</p>
                    </div>
                  </div>
                )}

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

          {/* A/B Test panel — only shown when a test exists for this campaign */}
          {abData && (
            <motion.div variants={variants.fadeInUp}>
              <GlassCard>
                <div className="flex items-center justify-between mb-4">
                  <h2 className="font-heading text-sm font-semibold text-white flex items-center gap-2">
                    <BarChart2 size={14} className="text-purple-plasma" />
                    A/B Test — {abData.ab_test.name}
                  </h2>
                  {abData.ab_test.winner_variant && (
                    <span className="flex items-center gap-1 rounded-full border border-green-signal/30 bg-green-signal/10 px-2 py-0.5 text-2xs font-mono text-green-signal">
                      <CheckCircle2 size={10} />
                      Winner: {abData.ab_test.winner_variant}
                    </span>
                  )}
                </div>

                {/* Variant performance table */}
                <div className="overflow-x-auto">
                  <table className="w-full text-xs font-mono">
                    <thead>
                      <tr className="text-left text-white/30 uppercase text-2xs tracking-wider">
                        <th className="pb-2 pr-4">Variant</th>
                        <th className="pb-2 pr-4">Sent</th>
                        <th className="pb-2 pr-4">Delivered</th>
                        <th className="pb-2 pr-4">Opened</th>
                        <th className="pb-2 pr-4">Clicked</th>
                        <th className="pb-2">Open Rate</th>
                        {!abData.ab_test.winner_variant && <th className="pb-2 pl-4">Action</th>}
                      </tr>
                    </thead>
                    <tbody>
                      {abData.ab_test.variants.map((v) => {
                        const stat = abData.stats.find((s) => s.variant === v.name);
                        const openRate = stat && stat.sent > 0
                          ? ((stat.opened / stat.sent) * 100).toFixed(1) + "%"
                          : "—";
                        const isWinner = abData.ab_test.winner_variant === v.name;
                        return (
                          <tr
                            key={v.name}
                            className="border-t border-glass-border/40"
                            style={isWinner ? { background: "rgba(0,255,136,0.04)" } : undefined}
                          >
                            <td className="py-2.5 pr-4">
                              <div className="flex items-center gap-1.5">
                                <span
                                  className="flex h-5 w-5 items-center justify-center rounded font-bold text-xs"
                                  style={{ background: "rgba(168,85,247,0.15)", color: "#A855F7" }}
                                >
                                  {v.name}
                                </span>
                                {isWinner && <CheckCircle2 size={11} className="text-green-signal" />}
                              </div>
                            </td>
                            <td className="py-2.5 pr-4 text-white/60">{stat?.sent?.toLocaleString() ?? "0"}</td>
                            <td className="py-2.5 pr-4 text-white/60">{stat?.delivered?.toLocaleString() ?? "0"}</td>
                            <td className="py-2.5 pr-4 text-white/60">{stat?.opened?.toLocaleString() ?? "0"}</td>
                            <td className="py-2.5 pr-4 text-white/60">{stat?.clicked?.toLocaleString() ?? "0"}</td>
                            <td className="py-2.5 text-cyan-neon/80">{openRate}</td>
                            {!abData.ab_test.winner_variant && (
                              <td className="py-2.5 pl-4">
                                <button
                                  onClick={() => handleSelectWinner(v.name)}
                                  disabled={selectingWinner}
                                  className="rounded-md border border-green-signal/25 bg-green-signal/10 px-2.5 py-1 text-2xs font-semibold text-green-signal hover:bg-green-signal/20 transition-colors disabled:opacity-40"
                                >
                                  Pick Winner
                                </button>
                              </td>
                            )}
                          </tr>
                        );
                      })}
                    </tbody>
                  </table>
                </div>

                {abData.ab_test.concluded_at && (
                  <p className="mt-3 text-2xs text-white/25 font-mono">
                    Concluded {fmtDate(abData.ab_test.concluded_at)}
                  </p>
                )}
              </GlassCard>
            </motion.div>
          )}
        </>
      )}
    </motion.div>
  );
}
