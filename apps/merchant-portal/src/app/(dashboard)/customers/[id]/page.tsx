"use client";
/**
 * Merchant Portal — Customer Detail Page
 * Route: /customers/[id]  (id = external_customer_id)
 *
 * Wired signals:
 *  - GET /v1/customers/:id            → profile
 *  - GET /v1/customers/:id/events     → activity timeline
 *  - GET /v1/customers/:id/churn-score
 *  - GET /v1/customers/:id/preferences
 *  - PUT /v1/customers/:id            → edit profile
 *  - POST /v1/customers/:id/support-tickets
 *  - POST /v1/customers/:id/support-tickets/:ticket_id/close
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
  type ChurnScore, type CustomerPreferences,
} from "@/lib/api/cdp";
import { createApiClient } from "@/lib/api/client";
import {
  ArrowLeft, User, Mail, Phone, MapPin, Package, TrendingUp,
  CheckCircle2, AlertCircle, Clock, Star, Activity, Edit2, X,
  TicketCheck, MessageSquare, Zap, Wifi, WifiOff, Send, Bot,
  Eye, MousePointerClick,
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

const CHANNEL_LABEL: Record<string, string> = {
  whatsapp: "WhatsApp",
  sms: "SMS",
  email: "Email",
  push: "Push",
};

interface CampaignSend {
  id: string;
  campaign_id: string;
  campaign_name: string;
  channel: string;
  status: string;
  triggered_by_rule_id: string | null;
  triggered_by_rule_name: string | null;
  queued_at: string;
  sent_at: string | null;
  delivered_at: string | null;
  read_at: string | null;
  failed_at: string | null;
  opened_at: string | null;
  clicked_at: string | null;
  clicked_url: string | null;
}

const SEND_STATUS_COLOR: Record<string, string> = {
  sent:      "#00FF88",
  delivered: "#00FF88",
  read:      "#00FF88",
  queued:    "#FFAB00",
  sending:   "#00E5FF",
  failed:    "#FF3B5C",
  bounced:   "#FF3B5C",
  skipped:   "#A855F7",
};

const EVENT_CFG: Record<string, { label: string; color: string; Icon: React.ElementType }> = {
  booking_created:    { label: "Booking Created",   color: "#00E5FF", Icon: Package      },
  delivery_completed: { label: "Delivered",         color: "#00FF88", Icon: CheckCircle2 },
  delivery_failed:    { label: "Delivery Failed",   color: "#FF3B5C", Icon: AlertCircle  },
  rating_given:       { label: "Rating Given",      color: "#A855F7", Icon: Star         },
  support_contacted:  { label: "Support Contacted", color: "#FFAB00", Icon: Activity     },
};

// ── Page ───────────────────────────────────────────────────────────────────────

export default function CustomerDetailPage() {
  const { id } = useParams<{ id: string }>();
  const router = useRouter();
  const api    = useMemo(() => createCdpApi(), []);

  const [profile,    setProfile]    = useState<CustomerProfile | null>(null);
  const [events,     setEvents]     = useState<BehavioralEvent[]>([]);
  const [churn,      setChurn]      = useState<ChurnScore | null>(null);
  const [prefs,      setPrefs]      = useState<CustomerPreferences | null>(null);
  const [sends,      setSends]      = useState<CampaignSend[]>([]);
  const [loading,    setLoading]    = useState(true);
  const [error,      setError]      = useState<string | null>(null);
  const [activeTab,  setActiveTab]  = useState<"timeline" | "comms">("timeline");

  // Edit profile
  const [editOpen,    setEditOpen]    = useState(false);
  const [editName,    setEditName]    = useState("");
  const [editEmail,   setEditEmail]   = useState("");
  const [editPhone,   setEditPhone]   = useState("");
  const [editSaving,  setEditSaving]  = useState(false);
  const [editError,   setEditError]   = useState<string | null>(null);

  // Support ticket
  const [ticketOpen,    setTicketOpen]    = useState(false);
  const [ticketSubject, setTicketSubject] = useState("");
  const [ticketMessage, setTicketMessage] = useState("");
  const [ticketSaving,  setTicketSaving]  = useState(false);
  const [ticketError,   setTicketError]   = useState<string | null>(null);
  const [openTicketId,  setOpenTicketId]  = useState<string | null>(null);
  const [ticketSuccess, setTicketSuccess] = useState(false);

  useEffect(() => {
    if (!id) return;
    setLoading(true);
    setError(null);
    // Profile + events are critical — failure blocks the page.
    // Churn score + preferences are non-critical — failure just leaves panels empty.
    Promise.all([api.get(id), api.events(id)])
      .then(([p, evts]) => {
        setProfile(p);
        setEvents(evts);
        setEditName(p.name ?? "");
        setEditEmail(p.email ?? "");
        setEditPhone(p.phone ?? "");
        api.churnScore(id).then(setChurn).catch(() => {});
        api.preferences(id).then(setPrefs).catch(() => {});
        // Load communication history from engagement service (non-critical).
        const http = createApiClient();
        http.get<{ sends: CampaignSend[] }>(`/v1/customers/${id}/sends`, {
          params: { limit: 50 },
        }).then((res) => setSends(res.data.sends ?? [])).catch(() => {});
      })
      .catch(e => setError(e?.response?.data?.message ?? e?.message ?? "Failed to load customer"))
      .finally(() => setLoading(false));
  }, [api, id]);

  async function saveEdit() {
    if (!profile) return;
    setEditSaving(true);
    setEditError(null);
    try {
      const updated = await api.upsert(profile.external_customer_id, {
        name:  editName  || undefined,
        email: editEmail || undefined,
        phone: editPhone || undefined,
      });
      setProfile(updated);
      setEditOpen(false);
    } catch (e) {
      const err = e as { message?: string };
      setEditError(err?.message ?? "Failed to save changes");
    } finally {
      setEditSaving(false);
    }
  }

  async function submitTicket() {
    if (!profile) return;
    setTicketSaving(true);
    setTicketError(null);
    try {
      const result = await api.openSupportTicket(profile.external_customer_id, {
        subject: ticketSubject || undefined,
        message: ticketMessage || undefined,
      });
      setOpenTicketId(result.ticket_id);
      setTicketSuccess(true);
      setTicketOpen(false);
      setTicketSubject("");
      setTicketMessage("");
    } catch (e) {
      const err = e as { message?: string };
      setTicketError(err?.message ?? "Failed to open ticket");
    } finally {
      setTicketSaving(false);
    }
  }

  async function closeTicket() {
    if (!profile || !openTicketId) return;
    try {
      await api.closeSupportTicket(profile.external_customer_id, openTicketId);
      setOpenTicketId(null);
      setTicketSuccess(false);
    } catch {
      // silent — ticket close is best-effort from the UI
    }
  }

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

  const churnColor = churn?.risk_tier === "high" ? "#FF3B5C" : churn?.risk_tier === "medium" ? "#FFAB00" : "#00FF88";

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
        <div className="min-w-0 flex-1">
          <h1 className="font-heading text-2xl font-bold text-white flex items-center gap-2">
            <User size={20} className="text-cyan-neon flex-shrink-0" />
            {profile.name ?? "Unnamed Customer"}
          </h1>
          <p className="text-xs font-mono text-white/30 mt-0.5">{profile.external_customer_id}</p>
        </div>
        <NeonBadge variant={tier.variant}>{tier.label}</NeonBadge>

        {/* Action buttons */}
        <div className="flex items-center gap-2">
          <button
            onClick={() => setEditOpen(true)}
            className="flex items-center gap-1.5 rounded-lg border border-glass-border bg-glass-100 px-3 py-2 text-xs text-white/60 hover:border-cyan-neon/30 hover:text-cyan-neon transition-all"
          >
            <Edit2 size={13} />
            Edit
          </button>
          {openTicketId ? (
            <button
              onClick={closeTicket}
              className="flex items-center gap-1.5 rounded-lg border border-amber-signal/40 bg-amber-signal/10 px-3 py-2 text-xs text-amber-signal hover:bg-amber-signal/20 transition-all"
            >
              <TicketCheck size={13} />
              Close Ticket
            </button>
          ) : (
            <button
              onClick={() => setTicketOpen(true)}
              className="flex items-center gap-1.5 rounded-lg border border-glass-border bg-glass-100 px-3 py-2 text-xs text-white/60 hover:border-amber-signal/30 hover:text-amber-signal transition-all"
            >
              <MessageSquare size={13} />
              Support Ticket
            </button>
          )}
        </div>
      </motion.div>

      {/* Ticket success banner */}
      {ticketSuccess && openTicketId && (
        <motion.div variants={variants.fadeInUp}>
          <GlassCard className="border-amber-signal/20">
            <div className="flex items-center justify-between gap-3">
              <div className="flex items-center gap-2 text-xs text-amber-signal">
                <TicketCheck size={14} />
                <span className="font-mono">Support ticket opened · ID: <span className="font-bold">{openTicketId.slice(0, 8)}…</span></span>
              </div>
              <button onClick={closeTicket} className="text-xs text-white/40 hover:text-white/70">Close ticket</button>
            </div>
          </GlassCard>
        </motion.div>
      )}

      {/* KPI strip — 5 metrics */}
      <motion.div variants={variants.fadeInUp} className="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-5">
        {[
          { label: "Total Shipments",   value: profile.total_shipments.toString(),            color: "#00E5FF" },
          { label: "Success Rate",      value: profile.total_shipments === 0 ? "—" : `${successRate.toFixed(0)}%`, color: successRate > 90 ? "#00FF88" : successRate > 70 ? "#00E5FF" : "#FFAB00" },
          { label: "CLV Score",         value: profile.clv_score.toFixed(0),                 color: "#A855F7" },
          { label: "Engagement Score",  value: profile.engagement_score?.toFixed(0) ?? "—",  color: "#00E5FF" },
          { label: "COD Collected",     value: fmtPhp(profile.total_cod_collected_cents),    color: "#00FF88" },
        ].map(({ label, value, color }) => (
          <GlassCard key={label} size="sm">
            <p className="text-2xs font-mono text-white/40 uppercase tracking-wider">{label}</p>
            <p className="font-heading text-xl font-bold mt-1" style={{ color }}>{value}</p>
          </GlassCard>
        ))}
      </motion.div>

      <div className="grid grid-cols-1 gap-5 lg:grid-cols-3">
        {/* Left column */}
        <motion.div variants={variants.fadeInUp} className="lg:col-span-1 flex flex-col gap-4">
          {/* Contact info */}
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
              {(profile.address_history ?? []).length > 0 && (
                <div>
                  <p className="text-2xs font-mono text-white/30 uppercase tracking-wider mb-2 mt-4">Address History</p>
                  <div className="space-y-2">
                    {(profile.address_history ?? []).slice(0, 4).map((a, i) => (
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
          <GlassCard>
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

          {/* Churn score */}
          {churn && (
            <GlassCard>
              <div className="flex items-center justify-between mb-3">
                <p className="text-xs font-semibold text-white flex items-center gap-2">
                  <Zap size={13} className="text-amber-signal" />
                  Churn Risk
                </p>
                <NeonBadge variant={churn.risk_tier === "high" ? "muted" : churn.risk_tier === "medium" ? "amber" : "green"}>
                  {churn.risk_tier.toUpperCase()}
                </NeonBadge>
              </div>
              <div className="w-full h-1.5 rounded-full bg-glass-100 overflow-hidden mb-3">
                <div
                  className="h-full rounded-full transition-all duration-700"
                  style={{ width: `${Math.min(churn.churn_score * 100, 100).toFixed(0)}%`, background: churnColor }}
                />
              </div>
              <p className="font-heading text-2xl font-bold mb-3" style={{ color: churnColor }}>
                {Math.min(churn.churn_score * 100, 100).toFixed(0)}%
              </p>
              <div className="grid grid-cols-2 gap-2 text-2xs font-mono text-white/30">
                <div><p className="text-white/20 mb-0.5">CLV signal</p><p>{churn.signals.clv_score.toFixed(0)}</p></div>
                <div><p className="text-white/20 mb-0.5">Engagement</p><p>{churn.signals.engagement_score.toFixed(0)}</p></div>
                <div><p className="text-white/20 mb-0.5">Shipments</p><p>{churn.signals.total_shipments}</p></div>
                {churn.signals.last_shipment_at && (
                  <div><p className="text-white/20 mb-0.5">Last active</p><p>{relativeTime(churn.signals.last_shipment_at)}</p></div>
                )}
              </div>
            </GlassCard>
          )}

          {/* Communication preferences */}
          {prefs && (
            <GlassCard>
              <p className="text-xs font-semibold text-white mb-3">Communication Preferences</p>
              <div className="space-y-2 mb-3">
                {(["whatsapp", "sms", "email", "push"] as const).map((ch) => {
                  const on = prefs.opt_in[ch];
                  const isPrimary = prefs.preferred_channel === ch;
                  return (
                    <div key={ch} className="flex items-center justify-between">
                      <div className="flex items-center gap-2 text-xs text-white/60">
                        {on ? <Wifi size={12} className="text-green-signal" /> : <WifiOff size={12} className="text-white/20" />}
                        {CHANNEL_LABEL[ch]}
                        {isPrimary && <NeonBadge variant="cyan" className="text-[9px] py-0 px-1.5">Primary</NeonBadge>}
                      </div>
                      <span className={`text-2xs font-mono ${on ? "text-green-signal" : "text-white/20"}`}>
                        {on ? "opted in" : "opted out"}
                      </span>
                    </div>
                  );
                })}
              </div>
              {prefs.delivery_time_window && (
                <div className="border-t border-glass-border pt-2 text-2xs font-mono text-white/30">
                  <p className="text-white/20 mb-0.5">Preferred delivery window</p>
                  <p>{prefs.delivery_time_window.preferred_from_hour}:00 – {prefs.delivery_time_window.preferred_to_hour}:00</p>
                </div>
              )}
            </GlassCard>
          )}
        </motion.div>

        {/* Activity timeline + Communication history (tabbed) */}
        <motion.div variants={variants.fadeInUp} className="lg:col-span-2">
          <GlassCard padding="none" className="p-5">
            {/* Tab bar */}
            <div className="flex items-center gap-1 mb-4 border-b border-glass-border pb-3">
              <button
                onClick={() => setActiveTab("timeline")}
                className={`flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-xs font-mono transition-all ${
                  activeTab === "timeline"
                    ? "bg-cyan-neon/10 text-cyan-neon border border-cyan-neon/30"
                    : "text-white/40 hover:text-white/70"
                }`}
              >
                <TrendingUp size={11} />
                Activity
              </button>
              <button
                onClick={() => setActiveTab("comms")}
                className={`flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-xs font-mono transition-all ${
                  activeTab === "comms"
                    ? "bg-cyan-neon/10 text-cyan-neon border border-cyan-neon/30"
                    : "text-white/40 hover:text-white/70"
                }`}
              >
                <Send size={11} />
                Messages
                {sends.length > 0 && (
                  <span className="ml-1 rounded-full bg-cyan-neon/20 px-1.5 py-0 text-cyan-neon text-[9px]">
                    {sends.length}
                  </span>
                )}
              </button>
            </div>

            {activeTab === "timeline" && (
              <>
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
              </>
            )}

            {activeTab === "comms" && (
              <>
                {sends.length === 0 ? (
                  <div className="flex flex-col items-center justify-center py-8 gap-2">
                    <Send size={20} className="text-white/20" />
                    <p className="text-xs font-mono text-white/30">No messages sent yet</p>
                    <p className="text-2xs font-mono text-white/20 text-center max-w-xs">
                      Campaign sends and automation-triggered messages will appear here.
                    </p>
                  </div>
                ) : (
                  <div className="space-y-2">
                    {sends.map((send) => {
                      const color = SEND_STATUS_COLOR[send.status] ?? "#A855F7";
                      const isAuto = !!send.triggered_by_rule_id;
                      return (
                        <div
                          key={send.id}
                          className="flex items-start gap-3 rounded-xl border border-glass-border bg-glass-100/30 p-3"
                        >
                          <div
                            className="mt-0.5 flex h-7 w-7 flex-shrink-0 items-center justify-center rounded-full"
                            style={{ background: `${color}15`, border: `1px solid ${color}30` }}
                          >
                            {isAuto
                              ? <Bot size={12} style={{ color }} />
                              : <Send size={12} style={{ color }} />}
                          </div>
                          <div className="min-w-0 flex-1">
                            <div className="flex items-start justify-between gap-2">
                              <p className="text-xs font-medium text-white line-clamp-1">{send.campaign_name}</p>
                              <span
                                className="flex-shrink-0 rounded-full px-1.5 py-0 text-[9px] font-mono font-semibold"
                                style={{ color, background: `${color}15` }}
                              >
                                {send.status}
                              </span>
                            </div>
                            <div className="mt-1 flex flex-wrap items-center gap-1.5 text-2xs font-mono text-white/40">
                              <span>{CHANNEL_LABEL[send.channel] ?? send.channel}</span>
                              <span>·</span>
                              <span>{relativeTime(send.queued_at)}</span>
                              {isAuto && (
                                <>
                                  <span>·</span>
                                  <span className="flex items-center gap-1 text-cyan-neon/70">
                                    <Bot size={9} />
                                    Auto: {send.triggered_by_rule_name ?? "rule"}
                                  </span>
                                </>
                              )}
                            </div>
                            {/* Engagement signals */}
                            {(send.read_at || send.opened_at || send.clicked_at) && (
                              <div className="mt-1.5 flex items-center gap-2">
                                {(send.read_at || send.opened_at) && (
                                  <span className="flex items-center gap-1 text-[10px] font-mono text-green-signal/70">
                                    <Eye size={9} />
                                    {send.read_at ? "Read" : "Opened"} {relativeTime(send.read_at ?? send.opened_at!)}
                                  </span>
                                )}
                                {send.clicked_at && (
                                  <span className="flex items-center gap-1 text-[10px] font-mono text-cyan-neon/70">
                                    <MousePointerClick size={9} />
                                    Clicked {relativeTime(send.clicked_at)}
                                  </span>
                                )}
                              </div>
                            )}
                          </div>
                        </div>
                      );
                    })}
                  </div>
                )}
              </>
            )}
          </GlassCard>
        </motion.div>
      </div>

      {/* Edit Profile Modal */}
      {editOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm p-4">
          <motion.div
            initial={{ opacity: 0, scale: 0.95 }}
            animate={{ opacity: 1, scale: 1 }}
            className="w-full max-w-md"
          >
            <GlassCard>
              <div className="flex items-center justify-between mb-5">
                <h2 className="font-heading text-sm font-semibold text-white flex items-center gap-2">
                  <Edit2 size={14} className="text-cyan-neon" />
                  Edit Customer Profile
                </h2>
                <button onClick={() => setEditOpen(false)} className="text-white/40 hover:text-white/70">
                  <X size={16} />
                </button>
              </div>

              <div className="space-y-4">
                <ModalField label="Name" value={editName} onChange={setEditName} placeholder="Full name" />
                <ModalField label="Email" value={editEmail} onChange={setEditEmail} placeholder="customer@example.com" type="email" />
                <ModalField label="Phone" value={editPhone} onChange={setEditPhone} placeholder="+63 912 345 6789" type="tel" />
              </div>

              {editError && (
                <p className="mt-3 text-xs text-red-signal font-mono">{editError}</p>
              )}

              <div className="flex gap-3 mt-5">
                <button
                  onClick={() => setEditOpen(false)}
                  className="flex-1 rounded-lg border border-glass-border py-2 text-xs text-white/50 hover:text-white transition-colors"
                >
                  Cancel
                </button>
                <button
                  onClick={saveEdit}
                  disabled={editSaving}
                  className="flex-1 rounded-lg bg-cyan-neon/10 border border-cyan-neon/30 py-2 text-xs text-cyan-neon hover:bg-cyan-neon/20 transition-colors disabled:opacity-50"
                >
                  {editSaving ? "Saving…" : "Save Changes"}
                </button>
              </div>
            </GlassCard>
          </motion.div>
        </div>
      )}

      {/* Support Ticket Modal */}
      {ticketOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm p-4">
          <motion.div
            initial={{ opacity: 0, scale: 0.95 }}
            animate={{ opacity: 1, scale: 1 }}
            className="w-full max-w-md"
          >
            <GlassCard>
              <div className="flex items-center justify-between mb-5">
                <h2 className="font-heading text-sm font-semibold text-white flex items-center gap-2">
                  <MessageSquare size={14} className="text-amber-signal" />
                  Open Support Ticket
                </h2>
                <button onClick={() => setTicketOpen(false)} className="text-white/40 hover:text-white/70">
                  <X size={16} />
                </button>
              </div>

              <p className="text-2xs font-mono text-white/30 mb-4">
                Opens a ticket for <span className="text-white/60">{profile.name ?? profile.external_customer_id.slice(0, 8)}</span>.
                Campaign messages for this customer will be suppressed until the ticket is closed.
              </p>

              <div className="space-y-4">
                <ModalField label="Subject" value={ticketSubject} onChange={setTicketSubject} placeholder="e.g. Delivery complaint" />
                <div className="flex flex-col gap-1">
                  <span className="text-2xs font-mono text-white/30 uppercase tracking-wider">Message</span>
                  <textarea
                    value={ticketMessage}
                    onChange={(e) => setTicketMessage(e.target.value)}
                    placeholder="Describe the issue…"
                    rows={3}
                    className="rounded-lg border border-glass-border bg-glass-100 px-3 py-2 text-xs text-white placeholder-white/20 outline-none focus:border-amber-signal/40 transition-colors resize-none"
                  />
                </div>
              </div>

              {ticketError && (
                <p className="mt-3 text-xs text-red-signal font-mono">{ticketError}</p>
              )}

              <div className="flex gap-3 mt-5">
                <button
                  onClick={() => setTicketOpen(false)}
                  className="flex-1 rounded-lg border border-glass-border py-2 text-xs text-white/50 hover:text-white transition-colors"
                >
                  Cancel
                </button>
                <button
                  onClick={submitTicket}
                  disabled={ticketSaving}
                  className="flex-1 rounded-lg bg-amber-signal/10 border border-amber-signal/30 py-2 text-xs text-amber-signal hover:bg-amber-signal/20 transition-colors disabled:opacity-50"
                >
                  {ticketSaving ? "Opening…" : "Open Ticket"}
                </button>
              </div>
            </GlassCard>
          </motion.div>
        </div>
      )}
    </motion.div>
  );
}

// ── ModalField ─────────────────────────────────────────────────────────────────

function ModalField({
  label, value, onChange, placeholder, type = "text",
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  type?: string;
}) {
  return (
    <div className="flex flex-col gap-1">
      <span className="text-2xs font-mono text-white/30 uppercase tracking-wider">{label}</span>
      <input
        type={type}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        className="rounded-lg border border-glass-border bg-glass-100 px-3 py-2 text-xs text-white placeholder-white/20 outline-none focus:border-cyan-neon/40 transition-colors"
      />
    </div>
  );
}
