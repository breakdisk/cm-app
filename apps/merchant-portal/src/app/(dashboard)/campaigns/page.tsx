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
  BarChart2, ChevronDown, CheckCircle2, RefreshCw, Users, Search, Check,
} from "lucide-react";
import {
  createCampaignsApi,
  type Campaign,
  type Channel,
  type CampaignStatus,
  type CampaignRecipient,
  type WeeklyStat,
  type CreateCampaignPayload,
} from "@/lib/api/campaigns";
import { createCdpApi, type CustomerProfile, profileIdOf } from "@/lib/api/cdp";

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
  completed: "green",
  cancelled: "amber",
  failed:    "red",
};

/** Short day labels used on the chart x-axis. */
const DAY_LABELS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/** Transform flat WeeklyStat rows into the shape Recharts expects. */
function buildChartData(stats: WeeklyStat[]) {
  // Generate last-7-days date strings (oldest first).
  const days: string[] = [];
  for (let i = 6; i >= 0; i--) {
    const d = new Date();
    d.setDate(d.getDate() - i);
    days.push(d.toISOString().slice(0, 10));
  }

  return days.map((iso) => {
    const label = DAY_LABELS[new Date(iso + "T12:00:00Z").getUTCDay()];
    const row: Record<string, string | number> = { day: label };
    for (const channel of ["whatsapp", "sms", "email", "push"] as const) {
      const match = stats.find((s) => s.day === iso && s.channel === channel);
      row[channel] = match ? match.count : 0;
    }
    return row;
  });
}

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

/** E.164 validation: must start with + followed by 8–15 digits. */
function isValidE164(phone: string): boolean {
  return /^\+[1-9]\d{7,14}$/.test(phone.trim());
}

/**
 * SMS segment calculator.
 * Any character outside the GSM-7 basic set (including emoji and most accented
 * letters) forces UCS-2 encoding, which halves the per-segment capacity.
 *   GSM-7  single: 160 chars   multi-part: 153 chars/segment
 *   Unicode single:  70 chars   multi-part:  67 chars/segment
 */
function getSmsSegments(text: string): {
  chars: number;
  segments: number;
  encoding: "GSM-7" | "Unicode";
} {
  const isUnicode = /[^\x00-\x7F£¥ÀÅÆÇÉØÜàäåæèéìñòöùü]/.test(text);
  const singleMax = isUnicode ? 70  : 160;
  const multiMax  = isUnicode ? 67  : 153;
  const chars     = text.length;
  const segments  = chars === 0 ? 0 : chars <= singleMax ? 1 : Math.ceil(chars / multiMax);
  return { chars, segments, encoding: isUnicode ? "Unicode" : "GSM-7" };
}

/**
 * Parse a recipients textarea into CampaignRecipient objects.
 * WhatsApp / SMS support a "Name|+phone" pipe format for per-recipient names
 * so {{customer_name}} substitutes correctly in the message body.
 */
function parseRecipients(raw: string, channel: Channel): CampaignRecipient[] {
  return raw
    .split(/[\n,]+/)
    .map((s) => s.trim())
    .filter(Boolean)
    .map((contact) => {
      if (channel === "email") return { email: contact };
      if (channel === "push")  return { customer_id: contact };
      // WhatsApp / SMS: support "Name|+63phone" or plain "+63phone"
      const pipeIdx = contact.indexOf("|");
      if (pipeIdx !== -1) {
        const name  = contact.slice(0, pipeIdx).trim() || undefined;
        const phone = contact.slice(pipeIdx + 1).trim();
        return { phone, name };
      }
      return { phone: contact };
    });
}

function NewCampaignModal({ onClose, onCreated }: { onClose: () => void; onCreated?: () => void }) {
  const [name,       setName]       = useState("");
  const [channel,    setChannel]    = useState<Channel>("whatsapp");
  const [trigger,    setTrigger]    = useState(TRIGGER_OPTIONS[0]);
  const [subject,    setSubject]    = useState("");
  const [deepLink,   setDeepLink]   = useState("");
  const [message,    setMessage]    = useState("");
  const [recipients, setRecipients] = useState("");
  const [saving,         setSaving]         = useState(false);
  const [done,           setDone]           = useState(false);
  const [error,          setError]          = useState<string | null>(null);
  const [scheduleEnabled, setScheduleEnabled] = useState(false);
  const [scheduleFor,     setScheduleFor]     = useState("");

  // Customer picker mode
  const [recipientMode,      setRecipientMode]      = useState<"manual" | "customers">("manual");
  const [customerSearch,     setCustomerSearch]     = useState("");
  const [customerList,       setCustomerList]       = useState<CustomerProfile[]>([]);
  const [customersLoading,   setCustomersLoading]   = useState(false);
  const [selectedCustomerIds, setSelectedCustomerIds] = useState<Set<string>>(new Set());

  // Load sender-type customers whenever the picker opens or search changes.
  useEffect(() => {
    if (recipientMode !== "customers") return;
    let cancelled = false;
    const timer = setTimeout(async () => {
      setCustomersLoading(true);
      try {
        const cdp = createCdpApi();
        const res = await cdp.list({
          name: customerSearch || undefined,
          profile_type: "sender",
          limit: 50,
        });
        if (!cancelled) setCustomerList(res.profiles ?? []);
      } catch {
        // non-fatal; list stays stale
      } finally {
        if (!cancelled) setCustomersLoading(false);
      }
    }, 300);
    return () => { cancelled = true; clearTimeout(timer); };
  }, [recipientMode, customerSearch]);

  function toggleCustomer(profile: CustomerProfile) {
    const id = profile.external_customer_id;
    setSelectedCustomerIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id); else next.add(id);
      return next;
    });
  }

  // Build CampaignRecipient[] from the selected sender profiles.
  function buildCustomerRecipients(): CampaignRecipient[] {
    return customerList
      .filter((p) => selectedCustomerIds.has(p.external_customer_id))
      .map((p) => ({
        customer_id: profileIdOf(p) || null,
        phone:       p.phone ?? null,
        email:       p.email ?? null,
        name:        p.name  ?? null,
      }));
  }

  // SMS multi-part messages are valid — 1000 chars is ~6 segments, practical limit.
  // The segment counter below the textarea handles billing transparency.
  const charMax = 1000;
  const needsSubject   = channel === "email" || channel === "push";
  const isPhoneChannel = channel === "whatsapp" || channel === "sms";

  // Derived — recomputed on every keystroke, cheap.
  const parsedList    = parseRecipients(recipients, channel);
  const invalidPhones = isPhoneChannel
    ? parsedList.filter((r) => r.phone && !isValidE164(r.phone))
    : [];
  const hasInvalidPhones = invalidPhones.length > 0;
  const smsInfo = channel === "sms" ? getSmsSegments(message) : null;

  const recipientPlaceholder =
    channel === "email"
      ? "juan@example.com\nmaria@example.com"
      : channel === "push"
      ? "customer-uuid-1\ncustomer-uuid-2"
      : "Juan Dela Cruz|+63912345678\nMaria Santos|+63917654321";

  async function handleCreate() {
    if (!name.trim() || !message.trim()) return;
    setSaving(true);
    setError(null);
    try {
      const finalRecipients: CampaignRecipient[] =
        recipientMode === "customers"
          ? buildCustomerRecipients()
          : parsedList;

      const payload: CreateCampaignPayload = {
        name: name.trim(),
        description: trigger,
        channel,
        template: {
          template_id: `inline_${Date.now()}`,
          subject: needsSubject ? subject.trim() : null,
          variables: {
            body: message.trim(),
            ...(channel === "push" && deepLink.trim() ? { deep_link: deepLink.trim() } : {}),
          },
        },
        targeting: {
          customer_ids: [],
          recipients: finalRecipients,
          estimated_reach: finalRecipients.length,
        },
      };
      const api = createCampaignsApi();
      const created = await api.create(payload);
      if (scheduleEnabled && scheduleFor) {
        await api.schedule(created.id, { scheduled_at: new Date(scheduleFor).toISOString() });
      }
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

            {/* WhatsApp sandbox notice */}
            {channel === "whatsapp" && (
              <div className="flex items-start gap-2.5 rounded-xl border border-green-signal/20 bg-green-signal/5 px-3.5 py-2.5">
                <MessageSquare size={13} className="mt-0.5 shrink-0 text-green-signal/70" />
                <p className="text-2xs text-white/40 leading-relaxed">
                  <span className="text-green-signal/80 font-medium">Twilio Sandbox:</span>{" "}
                  Recipients must opt-in by texting <span className="font-mono text-white/60">join &lt;keyword&gt;</span> to your sandbox number before they can receive messages.
                  Freeform messages only — Meta HSM templates are not yet supported.
                </p>
              </div>
            )}

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

            {/* Subject (email) / Title (push) */}
            {needsSubject && (
              <div>
                <label className="mb-1.5 block text-xs font-medium text-white/50">
                  {channel === "email" ? "Subject Line" : "Push Title"}
                  <span className="ml-1 text-red-signal/70">*</span>
                </label>
                <input
                  value={subject}
                  onChange={(e) => setSubject(e.target.value)}
                  placeholder={channel === "email" ? "e.g. Your shipment has arrived!" : "e.g. Package delivered!"}
                  className="w-full rounded-xl border border-glass-border bg-glass-100 px-3.5 py-2.5 text-sm text-white placeholder-white/20 outline-none focus:border-purple-plasma/50 transition-colors"
                />
                {channel === "email" && (
                  <p className="mt-1 text-2xs text-white/25">Shown as the email subject in the recipient&apos;s inbox.</p>
                )}
              </div>
            )}

            {/* Deep link (push only, optional) */}
            {channel === "push" && (
              <div>
                <label className="mb-1.5 block text-xs font-medium text-white/50">Deep Link <span className="text-white/30">(optional)</span></label>
                <input
                  value={deepLink}
                  onChange={(e) => setDeepLink(e.target.value)}
                  placeholder="/tracking"
                  className="w-full rounded-xl border border-glass-border bg-glass-100 px-3.5 py-2.5 text-sm text-white placeholder-white/20 outline-none focus:border-amber-signal/50 transition-colors font-mono"
                />
                <p className="mt-1 text-2xs text-white/25">App screen to open on tap, e.g. /tracking or /shipments.</p>
              </div>
            )}

            {/* Message */}
            <div>
              <div className="mb-1.5 flex items-center justify-between">
                <label className="text-xs font-medium text-white/50">Message</label>
                {/* SMS: show segment count; others: simple char counter */}
                {smsInfo ? (
                  <span className={`text-2xs font-mono ${message.length > charMax ? "text-red-signal" : "text-white/25"}`}>
                    {smsInfo.chars} chars · {smsInfo.segments} segment{smsInfo.segments !== 1 ? "s" : ""}
                    {smsInfo.encoding === "Unicode" && <span className="ml-1 text-amber-signal/70">(Unicode)</span>}
                  </span>
                ) : (
                  <span className={`text-2xs font-mono ${message.length > charMax ? "text-red-signal" : "text-white/25"}`}>
                    {message.length}/{charMax}
                  </span>
                )}
              </div>
              <textarea
                value={message}
                onChange={(e) => setMessage(e.target.value)}
                rows={4}
                placeholder={
                  isPhoneChannel
                    ? "Hi {{customer_name}}, your package is on its way!\n\nReply STOP to opt out."
                    : "Hi {{customer_name}}, your order has been delivered! 🎉\n\nBook your next shipment and get 10% off."
                }
                className="w-full resize-none rounded-xl border border-glass-border bg-glass-100 px-3.5 py-2.5 text-sm text-white placeholder-white/15 outline-none focus:border-purple-plasma/50 transition-colors font-mono"
              />
              <p className="mt-1 text-2xs text-white/25">
                {isPhoneChannel ? (
                  <>{'{{customer_name}}'} · {'{{name}}'} · {'{{phone}}'}</>
                ) : (
                  <>{'{{customer_name}}'} · {'{{name}}'}</>
                )}
                {channel === "email" && <span className="ml-2 text-purple-plasma/60">· Supports HTML</span>}
                {channel === "sms" && smsInfo && smsInfo.segments > 1 && (
                  <span className="ml-2 text-amber-signal/60">· {smsInfo.segments} segments = {smsInfo.segments}× billing</span>
                )}
              </p>
            </div>

            {/* Recipients — mode toggle + panel */}
            <div>
              <div className="mb-2 flex items-center justify-between">
                <label className="text-xs font-medium text-white/50">Recipients</label>
                {/* Mode tabs */}
                <div className="flex rounded-lg border border-glass-border overflow-hidden">
                  {(["manual", "customers"] as const).map((mode) => (
                    <button
                      key={mode}
                      type="button"
                      onClick={() => setRecipientMode(mode)}
                      className={`flex items-center gap-1 px-2.5 py-1 text-2xs font-medium transition-colors ${
                        recipientMode === mode
                          ? "bg-cyan-neon/15 text-cyan-neon"
                          : "text-white/40 hover:text-white/70"
                      } border-r border-glass-border last:border-r-0`}
                    >
                      {mode === "customers" ? <Users size={10} /> : null}
                      {mode === "manual" ? "Manual" : "From Customers"}
                    </button>
                  ))}
                </div>
              </div>

              {recipientMode === "manual" ? (
                <>
                  <div className="mb-1 flex items-center justify-end">
                    {recipients.trim() && (
                      <span className={`text-2xs font-mono ${hasInvalidPhones ? "text-red-signal" : "text-white/30"}`}>
                        {parsedList.length} recipient{parsedList.length !== 1 ? "s" : ""}
                        {hasInvalidPhones && ` · ${invalidPhones.length} invalid`}
                      </span>
                    )}
                  </div>
                  <textarea
                    value={recipients}
                    onChange={(e) => setRecipients(e.target.value)}
                    rows={3}
                    placeholder={recipientPlaceholder}
                    className={`w-full resize-none rounded-xl border bg-glass-100 px-3.5 py-2.5 text-sm text-white placeholder-white/15 outline-none transition-colors font-mono ${
                      hasInvalidPhones
                        ? "border-red-signal/40 focus:border-red-signal/70"
                        : "border-glass-border focus:border-cyan-neon/50"
                    }`}
                  />
                  <p className="mt-1 text-2xs text-white/25">
                    {channel === "email"
                      ? "One email address per line (or comma-separated)."
                      : channel === "push"
                      ? "One customer UUID per line."
                      : <><span className="text-white/40">Name|+63phone</span> or just <span className="text-white/40">+63phone</span> per line.</>}
                  </p>
                  {hasInvalidPhones && (
                    <p className="mt-1.5 rounded-lg border border-red-signal/25 bg-red-signal/8 px-2.5 py-1.5 text-2xs text-red-signal font-mono">
                      {invalidPhones.length} number{invalidPhones.length !== 1 ? "s" : ""} not in E.164 format — must start with + and country code.
                    </p>
                  )}
                </>
              ) : (
                /* Customer picker panel */
                <div className="rounded-xl border border-glass-border bg-glass-50/30 overflow-hidden">
                  {/* Search */}
                  <div className="relative border-b border-glass-border">
                    <Search size={13} className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-white/30" />
                    <input
                      value={customerSearch}
                      onChange={(e) => setCustomerSearch(e.target.value)}
                      placeholder="Search senders by name…"
                      className="w-full bg-transparent pl-8 pr-3 py-2.5 text-xs text-white placeholder-white/25 outline-none"
                    />
                  </div>
                  {/* List */}
                  <div className="max-h-48 overflow-y-auto">
                    {customersLoading ? (
                      <p className="px-3 py-4 text-center text-2xs text-white/30 font-mono">loading…</p>
                    ) : customerList.length === 0 ? (
                      <p className="px-3 py-4 text-center text-2xs text-white/30 font-mono">
                        No senders found. Book a shipment first to auto-create sender profiles.
                      </p>
                    ) : (
                      customerList.map((p) => {
                        const selected = selectedCustomerIds.has(p.external_customer_id);
                        const contact  = p.phone ?? p.email ?? p.external_customer_id.slice(0, 8);
                        return (
                          <button
                            key={p.external_customer_id}
                            type="button"
                            onClick={() => toggleCustomer(p)}
                            className="flex w-full items-center gap-3 px-3 py-2.5 text-left hover:bg-glass-100 transition-colors border-b border-glass-border/40 last:border-b-0"
                          >
                            <span className={`flex h-4 w-4 flex-shrink-0 items-center justify-center rounded border transition-colors ${
                              selected
                                ? "border-cyan-neon bg-cyan-neon/20 text-cyan-neon"
                                : "border-glass-border text-transparent"
                            }`}>
                              <Check size={10} />
                            </span>
                            <span className="flex-1 min-w-0">
                              <span className="block text-xs text-white truncate">{p.name ?? "Unnamed"}</span>
                              <span className="block text-2xs font-mono text-white/30 truncate">{contact}</span>
                            </span>
                          </button>
                        );
                      })
                    )}
                  </div>
                  {/* Selection summary */}
                  {selectedCustomerIds.size > 0 && (
                    <div className="flex items-center justify-between border-t border-glass-border px-3 py-2">
                      <span className="text-2xs font-mono text-cyan-neon">
                        {selectedCustomerIds.size} selected
                      </span>
                      <button
                        type="button"
                        onClick={() => setSelectedCustomerIds(new Set())}
                        className="text-2xs text-white/30 hover:text-white/60 transition-colors"
                      >
                        Clear
                      </button>
                    </div>
                  )}
                </div>
              )}
            </div>

            {/* Schedule for later */}
            <div className="rounded-xl border border-glass-border bg-glass-100 px-3.5 py-3">
              <div className="flex items-center justify-between">
                <div>
                  <p className="text-sm font-medium text-white/80">Schedule for later</p>
                  <p className="text-2xs text-white/30 mt-0.5">Send at a specific date &amp; time</p>
                </div>
                {/* Toggle */}
                <button
                  type="button"
                  onClick={() => { setScheduleEnabled((v) => !v); setScheduleFor(""); }}
                  className="relative h-5 w-9 rounded-full transition-colors duration-200 focus:outline-none"
                  style={{ background: scheduleEnabled ? "rgba(168,85,247,0.7)" : "rgba(255,255,255,0.12)" }}
                  aria-checked={scheduleEnabled}
                  role="switch"
                >
                  <span
                    className="absolute top-0.5 left-0.5 h-4 w-4 rounded-full bg-white shadow transition-transform duration-200"
                    style={{ transform: scheduleEnabled ? "translateX(16px)" : "translateX(0)" }}
                  />
                </button>
              </div>
              {scheduleEnabled && (
                <div className="mt-3">
                  <input
                    type="datetime-local"
                    value={scheduleFor}
                    min={new Date(Date.now() + 60_000).toISOString().slice(0, 16)}
                    onChange={(e) => setScheduleFor(e.target.value)}
                    className="w-full rounded-xl border border-glass-border bg-glass-100 px-3.5 py-2.5 text-sm text-white outline-none focus:border-purple-plasma/50 transition-colors font-mono [color-scheme:dark]"
                  />
                  {scheduleFor && (
                    <p className="mt-1 text-2xs text-white/30 font-mono">
                      Sends {new Date(scheduleFor).toLocaleString()}
                    </p>
                  )}
                </div>
              )}
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
                disabled={
                  !name.trim() || !message.trim() || message.length > charMax || saving ||
                  (scheduleEnabled && !scheduleFor) || (needsSubject && !subject.trim()) ||
                  (recipientMode === "manual" && hasInvalidPhones) ||
                  (recipientMode === "customers" && selectedCustomerIds.size === 0)
                }
                className="flex items-center gap-2 rounded-lg px-5 py-2 text-sm font-semibold text-white transition-all disabled:opacity-40"
                style={{ background: "linear-gradient(135deg, #A855F7, #00E5FF)" }}
              >
                {saving ? (
                  <><span className="h-3.5 w-3.5 animate-spin rounded-full border-2 border-white/30 border-t-white" /> {scheduleEnabled ? "Scheduling…" : "Creating…"}</>
                ) : (
                  <><Plus size={14} /> {scheduleEnabled ? "Schedule Campaign" : "Create Campaign"}</>
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
              <p className="text-sm text-white/40 mt-1">
                {scheduleEnabled && scheduleFor
                  ? `"${name}" is scheduled for ${new Date(scheduleFor).toLocaleString()}.`
                  : `"${name}" is now saved as a draft.`}
              </p>
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

  const [campaigns,  setCampaigns]  = useState<Campaign[]>([]);
  const [chartStats, setChartStats] = useState<WeeklyStat[]>([]);
  const [loading,    setLoading]    = useState(true);
  const [error,      setError]      = useState<string | null>(null);
  const [mutatingId, setMutatingId] = useState<string | null>(null);

  const api = useMemo(() => createCampaignsApi(), []);

  const load = useCallback(async () => {
    setError(null);
    try {
      const [listResp, statsResp] = await Promise.all([
        api.list(),
        api.weeklyStats().catch(() => [] as WeeklyStat[]), // non-fatal
      ]);
      setCampaigns(listResp.campaigns ?? []);
      setChartStats(statsResp);
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
              <p className="text-2xs font-mono text-white/30">WhatsApp · SMS · Email · Push</p>
            </div>
            <BarChart2 size={15} className="text-purple-plasma" />
          </div>
          <ResponsiveContainer width="100%" height={180}>
            <AreaChart data={buildChartData(chartStats)} margin={{ top: 0, right: 0, bottom: 0, left: -24 }}>
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
                <linearGradient id="grad-push" x1="0" y1="0" x2="0" y2="1">
                  <stop offset="5%"  stopColor="#FFAB00" stopOpacity={0.25} />
                  <stop offset="95%" stopColor="#FFAB00" stopOpacity={0}    />
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
              <Area type="monotone" dataKey="push"     stroke="#FFAB00" fill="url(#grad-push)"  strokeWidth={2} />
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
              <div key={c.id} onClick={() => router.push(`/campaigns/${c.id}`)} className="grid grid-cols-[2fr_80px_100px_80px_100px_1fr_80px] gap-3 items-center px-5 py-3.5 border-b border-glass-border/50 hover:bg-glass-100 transition-colors cursor-pointer">
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
                      onClick={(e) => { e.stopPropagation(); handleActivate(c.id); }}
                      disabled={busy}
                      className="rounded p-1.5 text-white/30 hover:text-green-signal hover:bg-glass-200 transition-colors disabled:opacity-40"
                      title="Activate (start sending)"
                    >
                      {busy ? <span className="block h-3 w-3 animate-spin rounded-full border-2 border-white/30 border-t-white" /> : <Play size={12} />}
                    </button>
                  )}
                  {(c.status === "draft" || c.status === "scheduled") && (
                    <button
                      onClick={(e) => { e.stopPropagation(); handleCancel(c.id); }}
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
