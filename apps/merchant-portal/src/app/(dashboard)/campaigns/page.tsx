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
 *
 * Channels: WhatsApp · SMS · Email · Push (live)
 *           Messenger · Telegram · X · Viber · WeChat · Line · Slack (CRM integration — pending)
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
  Megaphone, Plus, Zap, MessageSquare, Mail, Smartphone, Play, X as XClose,
  BarChart2, ChevronDown, CheckCircle2, RefreshCw, Users, Search, Check, Link2,
  Users2,
} from "lucide-react";
import {
  createCampaignsApi,
  createAbTestApi,
  type Campaign,
  type Channel,
  type CampaignStatus,
  type CampaignRecipient,
  type WeeklyStat,
  type CreateCampaignPayload,
} from "@/lib/api/campaigns";
import { createCdpApi, type CustomerProfile, profileIdOf } from "@/lib/api/cdp";
import { createSegmentsApi, type Segment } from "@/lib/api/segments";

// ── Social channel SVG icons ───────────────────────────────────────────────────

const MessengerIcon = ({ size = 14, className = "" }: { size?: number; className?: string }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="currentColor" className={className}>
    <path d="M12 2C6.477 2 2 6.145 2 11.243c0 2.928 1.373 5.55 3.528 7.3V22l3.375-1.85c1.267.35 2.602.35 3.097.35 5.523 0 10-4.145 10-9.257C22 6.145 17.523 2 12 2zm.012 14.47L9.56 13.8l-4.9 2.67 5.395-5.73 2.463 2.67 4.9-2.67-5.406 5.73z"/>
  </svg>
);

const TelegramIcon = ({ size = 14, className = "" }: { size?: number; className?: string }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="currentColor" className={className}>
    <path d="M12 0C5.373 0 0 5.373 0 12s5.373 12 12 12 12-5.373 12-12S18.627 0 12 0zm5.894 8.221-1.97 9.28c-.145.658-.537.818-1.084.508l-3-2.21-1.447 1.394c-.16.16-.295.295-.605.295l.213-3.053 5.56-5.023c.242-.213-.054-.333-.373-.12l-6.871 4.326-2.962-.924c-.643-.204-.657-.643.136-.953l11.57-4.463c.535-.194 1.003.131.833.943z"/>
  </svg>
);

const XIcon = ({ size = 14, className = "" }: { size?: number; className?: string }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="currentColor" className={className}>
    <path d="M18.244 2.25h3.308l-7.227 8.26 8.502 11.24H16.17l-5.214-6.817L4.99 21.75H1.68l7.73-8.835L1.254 2.25H8.08l4.713 6.231zm-1.161 17.52h1.833L7.084 4.126H5.117z"/>
  </svg>
);

const ViberIcon = ({ size = 14, className = "" }: { size?: number; className?: string }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="currentColor" className={className}>
    <path d="M11.4 0C5.3 0 .5 4.8.5 10.8c0 3.5 1.6 6.5 4.1 8.5v3.2l3-1.7c1.1.3 2.2.5 3.4.5 6.1 0 10.8-4.8 10.8-10.8.1-5.9-4.7-10.5-10.4-10.5zm.5 16.9c-1 0-1.9-.2-2.8-.5l-2.6 1.5v-2.6c-2-1.4-3.3-3.7-3.3-6.3 0-4.3 3.9-7.8 8.7-7.8s8.7 3.5 8.7 7.8c.1 4.4-3.8 7.9-8.7 7.9zm4-8.4c-.2-.1-1.5-.7-1.7-.8-.2-.1-.4-.1-.5.1-.2.2-.6.8-.8 1-.1.2-.3.2-.5.1-.7-.3-1.4-.7-2-1.2-.5-.5-.9-1-.8-1.5l.2-.5c-.2-.3-.5-1.6-.7-2.2-.2-.5-.4-.5-.5-.5h-.5c-.2 0-.5.1-.7.3-.2.2-.8.8-.8 1.9s.9 2.2 1 2.3c.1.1 1.7 2.6 4.2 3.5.6.2 1 .3 1.4.4.6.1 1.1.1 1.5.1.5-.1 1.4-.6 1.6-1.1.2-.5.2-1 .1-1.1-.1-.1-.3-.2-.4-.3z"/>
  </svg>
);

const WeChatIcon = ({ size = 14, className = "" }: { size?: number; className?: string }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="currentColor" className={className}>
    <path d="M8.691 2.188C3.891 2.188 0 5.476 0 9.53c0 2.212 1.17 4.203 3.002 5.55a.59.59 0 0 1 .213.665l-.39 1.48c-.019.07-.048.141-.048.213 0 .163.13.295.29.295a.326.326 0 0 0 .167-.054l1.903-1.114a.864.864 0 0 1 .717-.098 10.16 10.16 0 0 0 2.837.403c.276 0 .543-.027.811-.05-.857-2.578.157-4.972 1.932-6.446 1.703-1.415 3.882-1.98 5.853-1.838-.576-3.583-3.898-6.348-7.596-6.348zM5.785 5.991c.642 0 1.162.529 1.162 1.18a1.17 1.17 0 0 1-1.162 1.178A1.17 1.17 0 0 1 4.623 7.17c0-.651.52-1.18 1.162-1.18zm5.813 0c.642 0 1.162.529 1.162 1.18a1.17 1.17 0 0 1-1.162 1.178 1.17 1.17 0 0 1-1.162-1.178c0-.651.52-1.18 1.162-1.18zm3.34 2.867c-1.797-.052-3.746.512-5.28 1.786-1.72 1.428-2.687 3.72-1.78 6.22.942 2.453 3.666 4.229 6.884 4.229.826 0 1.622-.12 2.361-.336a.722.722 0 0 1 .598.082l1.584.926a.272.272 0 0 0 .14.047c.134 0 .24-.11.24-.247 0-.06-.024-.12-.04-.177l-.327-1.233a.49.49 0 0 1 .176-.554 5.77 5.77 0 0 0 2.5-4.627c0-3.545-3.136-6.116-6.056-6.116zm-2.35 3.495c.535 0 .97.44.97.982 0 .542-.435.982-.97.982s-.97-.44-.97-.982c0-.542.435-.982.97-.982zm4.696 0c.535 0 .97.44.97.982 0 .542-.435.982-.97.982s-.97-.44-.97-.982c0-.542.435-.982.97-.982z"/>
  </svg>
);

const LineIcon = ({ size = 14, className = "" }: { size?: number; className?: string }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="currentColor" className={className}>
    <path d="M19.365 9.863c.349 0 .63.285.63.631 0 .345-.281.63-.63.63H17.61v1.125h1.755c.349 0 .63.283.63.63 0 .344-.281.629-.63.629h-2.386c-.345 0-.627-.285-.627-.629V8.108c0-.345.282-.63.63-.63h2.386c.346 0 .627.285.627.63 0 .349-.281.63-.63.63H17.61v1.125h1.755zm-3.855 3.016c0 .27-.174.51-.432.596-.064.021-.133.031-.199.031-.211 0-.391-.09-.51-.25l-2.443-3.317v2.94c0 .344-.279.629-.631.629-.346 0-.626-.285-.626-.629V8.108c0-.27.173-.51.43-.595.06-.023.136-.033.194-.033.195 0 .375.104.495.254l2.462 3.33V8.108c0-.345.282-.63.63-.63.345 0 .63.285.63.63v4.771zm-5.741 0c0 .344-.282.629-.631.629-.345 0-.627-.285-.627-.629V8.108c0-.345.282-.63.63-.63.346 0 .628.285.628.63v4.771zm-2.466.629H4.917c-.345 0-.63-.285-.63-.629V8.108c0-.345.285-.63.63-.63.348 0 .63.285.63.63v4.141h1.756c.348 0 .629.283.629.63 0 .344-.281.629-.629.629M24 10.314C24 4.943 18.615.572 12 .572S0 4.943 0 10.314c0 4.811 4.27 8.842 10.035 9.608.391.082.923.258 1.058.59.12.301.079.766.038 1.08l-.164 1.02c-.045.301-.24 1.186 1.049.645 1.291-.539 6.916-4.078 9.436-6.975C23.176 14.393 24 12.458 24 10.314"/>
  </svg>
);

const SlackIcon = ({ size = 14, className = "" }: { size?: number; className?: string }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="currentColor" className={className}>
    <path d="M5.042 15.165a2.528 2.528 0 0 1-2.52 2.523A2.528 2.528 0 0 1 0 15.165a2.527 2.527 0 0 1 2.522-2.52h2.52v2.52zM6.313 15.165a2.527 2.527 0 0 1 2.521-2.52 2.527 2.527 0 0 1 2.521 2.52v6.313A2.528 2.528 0 0 1 8.834 24a2.528 2.528 0 0 1-2.521-2.522v-6.313zM8.834 5.042a2.528 2.528 0 0 1-2.521-2.52A2.528 2.528 0 0 1 8.834 0a2.528 2.528 0 0 1 2.521 2.522v2.52H8.834zM8.834 6.313a2.528 2.528 0 0 1 2.521 2.521 2.528 2.528 0 0 1-2.521 2.521H2.522A2.528 2.528 0 0 1 0 8.834a2.528 2.528 0 0 1 2.522-2.521h6.312zM18.956 8.834a2.528 2.528 0 0 1 2.522-2.521A2.528 2.528 0 0 1 24 8.834a2.528 2.528 0 0 1-2.522 2.521h-2.522V8.834zM17.688 8.834a2.528 2.528 0 0 1-2.523 2.521 2.527 2.527 0 0 1-2.52-2.521V2.522A2.527 2.527 0 0 1 15.165 0a2.528 2.528 0 0 1 2.523 2.522v6.312zM15.165 18.956a2.528 2.528 0 0 1 2.523 2.522A2.528 2.528 0 0 1 15.165 24a2.527 2.527 0 0 1-2.52-2.522v-2.522h2.52zM15.165 17.688a2.527 2.527 0 0 1-2.52-2.523 2.526 2.526 0 0 1 2.52-2.52h6.313A2.527 2.527 0 0 1 24 15.165a2.528 2.528 0 0 1-2.522 2.523h-6.313z"/>
  </svg>
);

// ── Channel metadata ───────────────────────────────────────────────────────────

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

const CHANNEL_LABEL: Record<Channel, string> = {
  whatsapp:  "WhatsApp",
  sms:       "SMS",
  email:     "Email",
  push:       "Push",
  messenger: "Messenger",
  telegram:  "Telegram",
  x:         "X",
  viber:     "Viber",
  wechat:    "WeChat",
  line:      "Line",
  slack:     "Slack",
};

const CHANNEL_ICON: Record<Channel, React.ReactNode> = {
  whatsapp:  <MessageSquare size={12} className="text-green-signal"    />,
  sms:       <Smartphone    size={12} className="text-cyan-neon"       />,
  email:     <Mail          size={12} className="text-purple-plasma"   />,
  push:      <Zap           size={12} className="text-amber-signal"    />,
  messenger: <MessengerIcon size={12} className="text-[#0099FF]"       />,
  telegram:  <TelegramIcon  size={12} className="text-[#26A5E4]"       />,
  x:         <XIcon         size={12} className="text-[#E7E9EA]"       />,
  viber:     <ViberIcon     size={12} className="text-[#7360F2]"       />,
  wechat:    <WeChatIcon    size={12} className="text-[#07C160]"       />,
  line:      <LineIcon      size={12} className="text-[#06C755]"       />,
  slack:     <SlackIcon     size={12} className="text-[#E01E5A]"       />,
};

/** CRM connector description shown when a social channel is selected in the builder. */
const SOCIAL_CHANNEL_NOTE: Partial<Record<Channel, string>> = {
  messenger: "Requires a connected Facebook Page via Meta Business Suite. Recipients identified by Page-Scoped IDs (PSIDs) — users must have messaged your Page at least once.",
  telegram:  "Requires a Telegram Bot token configured in the CRM connector. Recipients via @username or numeric chat_id.",
  x:         "Requires Twitter API v2 OAuth with Direct Message permissions. Recipients addressed by @handle or numeric user ID.",
  viber:     "Requires an approved Viber Business Profile. Recipients identified by E.164 phone numbers — they must have Viber installed.",
  wechat:    "Requires a WeChat Official Account (Subscription or Service Account). Recipients identified by OpenID — users must follow your account first.",
  line:      "Requires a verified LINE Official Account with Messaging API access. Recipients identified by Line userId.",
  slack:     "Connects to your Slack workspace via a configured Slack App. Target users via @username or Slack user IDs (e.g. U01234ABCDE).",
};

// Live channels (fully wired to engagement engine)
const LIVE_CHANNELS: Channel[] = ["whatsapp", "sms", "email", "push"];
// Social channels — UI ready, backend connector pending CRM integration
const SOCIAL_CHANNELS: Channel[] = ["messenger", "telegram", "x", "viber", "wechat", "line", "slack"];
// Channels that use E.164 phone numbers for addressing
const PHONE_CHANNELS: Channel[] = ["whatsapp", "sms", "viber"];

type ChannelOption = {
  value: Channel;
  label: string;
  // size accepts string | number to be compatible with both Lucide icons and custom SVGs
  icon: React.ComponentType<{ size?: number | string; className?: string }>;
  color: string;
};

const CORE_CHANNEL_OPTIONS: ChannelOption[] = [
  { value: "whatsapp", label: "WhatsApp", icon: MessageSquare, color: "#00FF88" },
  { value: "sms",      label: "SMS",      icon: Smartphone,    color: "#00E5FF" },
  { value: "email",    label: "Email",    icon: Mail,          color: "#A855F7" },
  { value: "push",     label: "Push",     icon: Zap,           color: "#FFAB00" },
];

const SOCIAL_CHANNEL_OPTIONS: ChannelOption[] = [
  { value: "messenger", label: "Messenger", icon: MessengerIcon, color: "#0099FF" },
  { value: "telegram",  label: "Telegram",  icon: TelegramIcon,  color: "#26A5E4" },
  { value: "x",         label: "X",         icon: XIcon,         color: "#E7E9EA" },
  { value: "viber",     label: "Viber",     icon: ViberIcon,     color: "#7360F2" },
  { value: "wechat",    label: "WeChat",    icon: WeChatIcon,    color: "#07C160" },
  { value: "line",      label: "Line",      icon: LineIcon,      color: "#06C755" },
  { value: "slack",     label: "Slack",     icon: SlackIcon,     color: "#E01E5A" },
];

const STATUS_VARIANT: Record<CampaignStatus, "green" | "amber" | "purple" | "red" | "cyan"> = {
  draft:     "purple",
  scheduled: "cyan",
  sending:   "green",
  completed: "green",
  cancelled: "amber",
  failed:    "red",
};

const DAY_LABELS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/** Transform flat WeeklyStat rows into the shape Recharts expects.
 *  Includes an aggregated "social" key for all social-channel sends. */
function buildChartData(stats: WeeklyStat[]) {
  const days: string[] = [];
  for (let i = 6; i >= 0; i--) {
    const d = new Date();
    d.setDate(d.getDate() - i);
    days.push(d.toISOString().slice(0, 10));
  }

  return days.map((iso) => {
    const label = DAY_LABELS[new Date(iso + "T12:00:00Z").getUTCDay()];
    const row: Record<string, string | number> = { day: label };
    for (const ch of LIVE_CHANNELS) {
      const match = stats.find((s) => s.day === iso && s.channel === ch);
      row[ch] = match ? match.count : 0;
    }
    // Aggregate all social channels into one line for the chart
    row["social"] = SOCIAL_CHANNELS.reduce((sum, ch) => {
      const match = stats.find((s) => s.day === iso && s.channel === ch);
      return sum + (match ? match.count : 0);
    }, 0);
    return row;
  });
}

// ── Channel picker button ─────────────────────────────────────────────────────

function ChannelButton({
  opt,
  selected,
  onSelect,
  showCrmBadge = false,
  compact = false,
}: {
  opt: ChannelOption;
  selected: boolean;
  onSelect: (v: Channel) => void;
  showCrmBadge?: boolean;
  compact?: boolean;
}) {
  const { value, label, icon: Icon, color } = opt;
  return (
    <button
      key={value}
      onClick={() => onSelect(value)}
      className={`relative flex flex-col items-center gap-1 rounded-xl border transition-all font-medium ${compact ? "py-2 text-2xs" : "py-3 text-xs"}`}
      style={{
        borderColor: selected ? `${color}40` : "rgba(255,255,255,0.07)",
        background:  selected ? `${color}10` : "transparent",
        color:       selected ? color         : "rgba(255,255,255,0.38)",
      }}
    >
      <Icon size={compact ? 12 : 14} />
      <span className="truncate w-full text-center px-0.5 leading-tight">{label}</span>
      {showCrmBadge && (
        <span
          className="absolute -top-1.5 -right-1 rounded px-0.5 leading-none"
          style={{
            fontSize: 7,
            fontFamily: "JetBrains Mono, monospace",
            background: "rgba(0,229,255,0.10)",
            color: "rgba(0,229,255,0.55)",
            border: "1px solid rgba(0,229,255,0.18)",
            paddingTop: 1,
            paddingBottom: 1,
          }}
        >
          CRM
        </span>
      )}
    </button>
  );
}

// ── Helpers ────────────────────────────────────────────────────────────────────

function isValidE164(phone: string): boolean {
  return /^\+[1-9]\d{7,14}$/.test(phone.trim());
}

function getSmsSegments(text: string): {
  chars: number; segments: number; encoding: "GSM-7" | "Unicode";
} {
  const isUnicode = /[^\x00-\x7F£¥ÀÅÆÇÉØÜàäåæèéìñòöùü]/.test(text);
  const singleMax = isUnicode ? 70  : 160;
  const multiMax  = isUnicode ? 67  : 153;
  const chars     = text.length;
  const segments  = chars === 0 ? 0 : chars <= singleMax ? 1 : Math.ceil(chars / multiMax);
  return { chars, segments, encoding: isUnicode ? "Unicode" : "GSM-7" };
}

function parseRecipients(raw: string, channel: Channel): CampaignRecipient[] {
  return raw
    .split(/[\n,]+/)
    .map((s) => s.trim())
    .filter(Boolean)
    .map((contact) => {
      if (channel === "email") return { email: contact };
      // Push and social channels: store identifier in customer_id
      if (channel === "push" || SOCIAL_CHANNELS.includes(channel)) return { customer_id: contact };
      // Phone-based channels (WhatsApp, SMS, Viber): support "Name|+phone" or plain "+phone"
      const pipeIdx = contact.indexOf("|");
      if (pipeIdx !== -1) {
        const name  = contact.slice(0, pipeIdx).trim() || undefined;
        const phone = contact.slice(pipeIdx + 1).trim();
        return { phone, name };
      }
      return { phone: contact };
    });
}

const TRIGGER_OPTIONS = [
  "On: delivered",
  "On: failed delivery",
  "On: out_for_delivery",
  "4h before ETA",
  "30-day inactive",
  "On: 500pts reached",
  "Manual / Scheduled",
];

// ── NewCampaignModal ───────────────────────────────────────────────────────────

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
  const [scheduleEnabled,   setScheduleEnabled]   = useState(false);
  const [scheduleFor,       setScheduleFor]       = useState("");
  const [abTestEnabled,     setAbTestEnabled]     = useState(false);
  const [variantBTemplateId, setVariantBTemplateId] = useState("");
  const [createdId,      setCreatedId]      = useState<string | null>(null);
  const [activating,     setActivating]     = useState(false);
  const [activated,      setActivated]      = useState(false);

  const [recipientMode,       setRecipientMode]       = useState<"manual" | "customers" | "segment">("manual");
  const [customerSearch,      setCustomerSearch]      = useState("");
  const [customerList,        setCustomerList]        = useState<CustomerProfile[]>([]);
  const [customersLoading,    setCustomersLoading]    = useState(false);
  const [selectedCustomerIds, setSelectedCustomerIds] = useState<Set<string>>(new Set());
  const [segments,            setSegments]            = useState<Segment[]>([]);
  const [segmentsLoading,     setSegmentsLoading]     = useState(false);
  const [selectedSegmentId,   setSelectedSegmentId]   = useState<string>("");

  useEffect(() => {
    if (recipientMode !== "segment") return;
    setSegmentsLoading(true);
    createSegmentsApi().list()
      .then((res) => setSegments(res.segments))
      .catch(() => {})
      .finally(() => setSegmentsLoading(false));
  }, [recipientMode]);

  useEffect(() => {
    if (recipientMode !== "customers") return;
    let cancelled = false;
    const timer = setTimeout(async () => {
      setCustomersLoading(true);
      try {
        const cdp = createCdpApi();
        const res = await cdp.list({ name: customerSearch || undefined, limit: 50 });
        if (!cancelled) setCustomerList(res.profiles ?? []);
      } catch { /* non-fatal */ } finally {
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

  async function handleActivateNow() {
    if (!createdId) return;
    setActivating(true);
    setError(null);
    try {
      await createCampaignsApi().activate(createdId);
      setActivated(true);
      onCreated?.();
    } catch (e) {
      setError((e as { message?: string })?.message ?? "Failed to activate campaign");
    } finally {
      setActivating(false);
    }
  }

  const charMax        = 1000;
  const needsSubject   = channel === "email" || channel === "push";
  const isPhoneChannel = PHONE_CHANNELS.includes(channel);
  const isSocialChannel = SOCIAL_CHANNELS.includes(channel);

  const parsedList    = parseRecipients(recipients, channel);
  const invalidPhones = isPhoneChannel ? parsedList.filter((r) => r.phone && !isValidE164(r.phone)) : [];
  const hasInvalidPhones = invalidPhones.length > 0;
  const smsInfo = channel === "sms" ? getSmsSegments(message) : null;

  const recipientPlaceholder =
    channel === "email"     ? "juan@example.com\nmaria@example.com"
    : channel === "push"    ? "customer-uuid-1\ncustomer-uuid-2"
    : channel === "messenger" ? "107834567890123\n209845678901234 (Page-Scoped IDs)"
    : channel === "telegram"  ? "@juan_delacruz\n987654321 (chat_id)"
    : channel === "x"         ? "@juan_delacruz\n@maria_santos"
    : channel === "wechat"    ? "oAbc1xyzABCDEF123456 (WeChat OpenID)"
    : channel === "line"      ? "Uabc123def456ghi789 (Line userId)"
    : channel === "slack"     ? "@john.doe\nU01234ABCDE (Slack ID)"
    : "Juan Dela Cruz|+63912345678\nMaria Santos|+63917654321";

  const recipientHint =
    channel === "email" ? "One email address per line (or comma-separated)."
    : channel === "push"    ? "One customer UUID per line."
    : channel === "messenger" ? "One Facebook Page-Scoped ID (PSID) per line."
    : channel === "telegram"  ? "@username or numeric chat_id, one per line."
    : channel === "x"         ? "@handle per line."
    : channel === "wechat"    ? "WeChat OpenID per line."
    : channel === "line"      ? "Line userId per line."
    : channel === "slack"     ? "@username or Slack user ID (U…) per line."
    : <><span className="text-white/40">Name|+63phone</span> or just <span className="text-white/40">+63phone</span> per line.</>;

  async function handleCreate() {
    if (!name.trim() || !message.trim()) return;
    setSaving(true);
    setError(null);
    try {
      const finalRecipients: CampaignRecipient[] =
        recipientMode === "customers" ? buildCustomerRecipients()
        : recipientMode === "segment" ? []
        : parsedList;
      const selectedSeg = recipientMode === "segment" ? segments.find((s) => s.id === selectedSegmentId) : undefined;
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
          estimated_reach: recipientMode === "segment" ? (selectedSeg?.customer_count ?? 0) : finalRecipients.length,
          ...(recipientMode === "segment" && selectedSegmentId ? { segment_id: selectedSegmentId } : {}),
        },
      };
      const api = createCampaignsApi();
      const created = await api.create(payload);
      setCreatedId(created.id);
      if (scheduleEnabled && scheduleFor) {
        await api.schedule(created.id, { scheduled_at: new Date(scheduleFor).toISOString() });
      }
      if (abTestEnabled && variantBTemplateId.trim()) {
        await createAbTestApi().create(created.id, {
          name: `${name.trim()} — A/B Test`,
          variants: [
            { name: "A", template_id: `inline_${created.id}`, weight_pct: 50 },
            { name: "B", template_id: variantBTemplateId.trim(), weight_pct: 50 },
          ],
        }).catch(() => { /* non-fatal — campaign still created */ });
      }
      setDone(true);
      onCreated?.();
    } catch (e) {
      setError((e as { message?: string })?.message ?? "Failed to create campaign");
    } finally {
      setSaving(false);
    }
  }

  const activeChannelColor = CHANNEL_COLOR[channel];

  return (
    <motion.div
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      className="fixed inset-0 z-50 flex items-center justify-center p-4"
      style={{ background: "rgba(0,0,0,0.78)", backdropFilter: "blur(6px)" }}
      onClick={(e) => e.target === e.currentTarget && onClose()}
    >
      <motion.div
        initial={{ opacity: 0, scale: 0.96, y: 16 }}
        animate={{ opacity: 1, scale: 1, y: 0 }}
        exit={{ opacity: 0, scale: 0.96, y: 8 }}
        transition={{ ease: [0.16, 1, 0.3, 1], duration: 0.3 }}
        className="relative w-full max-w-lg rounded-2xl border border-glass-border shadow-glass flex flex-col"
        style={{ background: "rgba(8,12,28,0.98)", maxHeight: "92vh" }}
      >
        {/* Header */}
        <div className="flex items-center justify-between px-6 pt-6 pb-4 shrink-0 border-b border-glass-border/50">
          <div>
            <h2 className="font-heading text-lg font-bold text-white">New Campaign</h2>
            <p className="text-xs text-white/35 mt-0.5 font-mono">Engagement Engine · AI-powered targeting</p>
          </div>
          <button onClick={onClose} className="flex h-8 w-8 items-center justify-center rounded-lg border border-glass-border text-white/40 hover:text-white/80 transition-all">
            <XClose size={15} />
          </button>
        </div>

        {/* Scrollable content */}
        <div className="overflow-y-auto flex-1 px-6 py-4">
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

                {/* Core channels */}
                <div className="grid grid-cols-4 gap-2">
                  {CORE_CHANNEL_OPTIONS.map((opt) => (
                    <ChannelButton key={opt.value} opt={opt} selected={channel === opt.value} onSelect={setChannel} />
                  ))}
                </div>

                {/* Social channels */}
                <div className="mt-3">
                  <div className="flex items-center gap-2 mb-2">
                    <div className="h-px flex-1" style={{ background: "rgba(255,255,255,0.06)" }} />
                    <span className="text-[10px] font-mono text-white/25 uppercase tracking-widest px-1 flex items-center gap-1">
                      <Link2 size={9} className="opacity-60" />
                      Social · CRM Integration
                    </span>
                    <div className="h-px flex-1" style={{ background: "rgba(255,255,255,0.06)" }} />
                  </div>
                  <div className="grid grid-cols-4 gap-1.5">
                    {SOCIAL_CHANNEL_OPTIONS.map((opt) => (
                      <ChannelButton
                        key={opt.value}
                        opt={opt}
                        selected={channel === opt.value}
                        onSelect={setChannel}
                        showCrmBadge
                        compact
                      />
                    ))}
                  </div>
                </div>
              </div>

              {/* WhatsApp sandbox notice */}
              {channel === "whatsapp" && (
                <div className="flex items-start gap-2.5 rounded-xl border border-green-signal/20 bg-green-signal/5 px-3.5 py-2.5">
                  <MessageSquare size={12} className="mt-0.5 shrink-0 text-green-signal/70" />
                  <p className="text-2xs text-white/40 leading-relaxed">
                    <span className="text-green-signal/80 font-medium">Twilio Sandbox:</span>{" "}
                    Recipients must opt-in by texting <span className="font-mono text-white/60">join &lt;keyword&gt;</span> to your sandbox number.
                    Freeform messages only — Meta HSM templates not yet supported.
                  </p>
                </div>
              )}

              {/* Social CRM connector notice */}
              {isSocialChannel && SOCIAL_CHANNEL_NOTE[channel] && (
                <div
                  className="flex items-start gap-2.5 rounded-xl border px-3.5 py-3"
                  style={{
                    borderColor: `${activeChannelColor}28`,
                    background:  `${activeChannelColor}09`,
                  }}
                >
                  <Link2 size={12} className="mt-0.5 shrink-0" style={{ color: `${activeChannelColor}90` }} />
                  <div>
                    <p className="text-2xs font-semibold mb-0.5" style={{ color: `${activeChannelColor}CC` }}>
                      CRM Connector Required
                    </p>
                    <p className="text-2xs text-white/38 leading-relaxed">
                      {SOCIAL_CHANNEL_NOTE[channel]}
                    </p>
                  </div>
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

              {/* Deep link (push only) */}
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
                    isPhoneChannel || isSocialChannel
                      ? "Hi {{customer_name}}, your package is on its way!\n\nReply STOP to opt out."
                      : "Hi {{customer_name}}, your order has been delivered! 🎉\n\nBook your next shipment and get 10% off."
                  }
                  className="w-full resize-none rounded-xl border border-glass-border bg-glass-100 px-3.5 py-2.5 text-sm text-white placeholder-white/15 outline-none focus:border-purple-plasma/50 transition-colors font-mono"
                />
                <p className="mt-1 text-2xs text-white/25">
                  {"{{"}<span className="text-white/40">customer_name</span>{"}}"} · {"{{"}<span className="text-white/40">name</span>{"}}"}
                  {isPhoneChannel && <> · {"{{"}<span className="text-white/40">phone</span>{"}}"}</>}
                  {channel === "email" && <span className="ml-2 text-purple-plasma/60">· Supports HTML</span>}
                  {channel === "sms" && smsInfo && smsInfo.segments > 1 && (
                    <span className="ml-2 text-amber-signal/60">· {smsInfo.segments} segments = {smsInfo.segments}× billing</span>
                  )}
                </p>
              </div>

              {/* Recipients */}
              <div>
                <div className="mb-2 flex items-center justify-between">
                  <label className="text-xs font-medium text-white/50">Recipients</label>
                  <div className="flex rounded-lg border border-glass-border overflow-hidden">
                    {(["manual", "customers", "segment"] as const).map((mode) => (
                      <button
                        key={mode}
                        type="button"
                        onClick={() => setRecipientMode(mode)}
                        className={`flex items-center gap-1 px-2.5 py-1 text-2xs font-medium transition-colors ${
                          recipientMode === mode ? "bg-cyan-neon/15 text-cyan-neon" : "text-white/40 hover:text-white/70"
                        } border-r border-glass-border last:border-r-0`}
                      >
                        {mode === "customers" ? <Users size={10} /> : null}
                        {mode === "segment" ? <Users2 size={10} /> : null}
                        {mode === "manual" ? "Manual" : mode === "customers" ? "From Customers" : "Segment"}
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
                    <p className="mt-1 text-2xs text-white/25">{recipientHint}</p>
                    {hasInvalidPhones && (
                      <p className="mt-1.5 rounded-lg border border-red-signal/25 bg-red-signal/8 px-2.5 py-1.5 text-2xs text-red-signal font-mono">
                        {invalidPhones.length} number{invalidPhones.length !== 1 ? "s" : ""} not in E.164 format — must start with + and country code.
                      </p>
                    )}
                  </>
                ) : recipientMode === "customers" ? (
                  <div className="rounded-xl border border-glass-border bg-glass-50/30 overflow-hidden">
                    <div className="relative border-b border-glass-border">
                      <Search size={13} className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-white/30" />
                      <input
                        value={customerSearch}
                        onChange={(e) => setCustomerSearch(e.target.value)}
                        placeholder="Search customers by name…"
                        className="w-full bg-transparent pl-8 pr-3 py-2.5 text-xs text-white placeholder-white/25 outline-none"
                      />
                    </div>
                    <div className="max-h-48 overflow-y-auto">
                      {customersLoading ? (
                        <p className="px-3 py-4 text-center text-2xs text-white/30 font-mono">loading…</p>
                      ) : customerList.length === 0 ? (
                        <p className="px-3 py-4 text-center text-2xs text-white/30 font-mono">
                          No customers found. Book a shipment first to auto-create customer profiles.
                        </p>
                      ) : (
                        customerList.map((p) => {
                          const selected = selectedCustomerIds.has(p.external_customer_id);
                          const contact  = p.phone ?? p.email ?? p.external_customer_id.slice(0, 8);
                          const typeLabel = p.profile_type === "sender" ? "Sender" : p.profile_type === "receiver" ? "Receiver" : null;
                          const typeColor = p.profile_type === "sender" ? "text-cyan-neon" : "text-purple-plasma";
                          return (
                            <button
                              key={p.external_customer_id}
                              type="button"
                              onClick={() => toggleCustomer(p)}
                              className="flex w-full items-center gap-3 px-3 py-2.5 text-left hover:bg-glass-100 transition-colors border-b border-glass-border/40 last:border-b-0"
                            >
                              <span className={`flex h-4 w-4 flex-shrink-0 items-center justify-center rounded border transition-colors ${
                                selected ? "border-cyan-neon bg-cyan-neon/20 text-cyan-neon" : "border-glass-border text-transparent"
                              }`}>
                                <Check size={10} />
                              </span>
                              <span className="flex-1 min-w-0">
                                <span className="block text-xs text-white truncate">{p.name ?? "Unnamed"}</span>
                                <span className="block text-2xs font-mono text-white/30 truncate">{contact}</span>
                              </span>
                              {typeLabel && (
                                <span className={`text-2xs font-mono flex-shrink-0 ${typeColor}`}>{typeLabel}</span>
                              )}
                            </button>
                          );
                        })
                      )}
                    </div>
                    {selectedCustomerIds.size > 0 && (
                      <div className="flex items-center justify-between border-t border-glass-border px-3 py-2">
                        <span className="text-2xs font-mono text-cyan-neon">{selectedCustomerIds.size} selected</span>
                        <button type="button" onClick={() => setSelectedCustomerIds(new Set())} className="text-2xs text-white/30 hover:text-white/60 transition-colors">
                          Clear
                        </button>
                      </div>
                    )}
                  </div>
                ) : (
                  /* Segment picker */
                  <div className="rounded-xl border border-glass-border bg-glass-50/30 overflow-hidden">
                    {segmentsLoading ? (
                      <p className="px-3 py-4 text-center text-2xs text-white/30 font-mono">loading segments…</p>
                    ) : segments.length === 0 ? (
                      <p className="px-3 py-4 text-center text-2xs text-white/30 font-mono">
                        No segments yet. Create one in the CRM → Segments page.
                      </p>
                    ) : (
                      <div className="max-h-48 overflow-y-auto">
                        {segments.map((seg) => {
                          const chosen = selectedSegmentId === seg.id;
                          return (
                            <button
                              key={seg.id}
                              type="button"
                              onClick={() => setSelectedSegmentId(chosen ? "" : seg.id)}
                              className="flex w-full items-center gap-3 px-3 py-2.5 text-left hover:bg-glass-100 transition-colors border-b border-glass-border/40 last:border-b-0"
                            >
                              <span className={`flex h-4 w-4 flex-shrink-0 items-center justify-center rounded-full border transition-colors ${
                                chosen ? "border-purple-plasma bg-purple-plasma/20 text-purple-plasma" : "border-glass-border text-transparent"
                              }`}>
                                <Check size={10} />
                              </span>
                              <span className="flex-1 min-w-0">
                                <span className="block text-xs text-white truncate">{seg.name}</span>
                                <span className="block text-2xs font-mono text-white/30">
                                  {seg.customer_count != null ? `${seg.customer_count.toLocaleString()} members` : seg.description ?? ""}
                                </span>
                              </span>
                              {chosen && <NeonBadge variant="purple">Selected</NeonBadge>}
                            </button>
                          );
                        })}
                      </div>
                    )}
                    {selectedSegmentId && (() => {
                      const seg = segments.find((s) => s.id === selectedSegmentId);
                      return seg ? (
                        <div className="flex items-center justify-between border-t border-glass-border px-3 py-2">
                          <span className="text-2xs font-mono text-purple-plasma">
                            {seg.name}{seg.customer_count != null ? ` · ${seg.customer_count.toLocaleString()} members` : ""}
                          </span>
                          <button type="button" onClick={() => setSelectedSegmentId("")} className="text-2xs text-white/30 hover:text-white/60 transition-colors">
                            Clear
                          </button>
                        </div>
                      ) : null;
                    })()}
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

              {/* A/B Test */}
              <div className="rounded-xl border border-glass-border bg-glass-100 px-3.5 py-3">
                <div className="flex items-center justify-between">
                  <div>
                    <p className="text-sm font-medium text-white/80">A/B Test</p>
                    <p className="text-2xs text-white/30 mt-0.5">Split audience 50/50 between two templates</p>
                  </div>
                  <button
                    type="button"
                    onClick={() => setAbTestEnabled((v) => !v)}
                    className="relative h-5 w-9 rounded-full transition-colors duration-200 focus:outline-none"
                    style={{ background: abTestEnabled ? "rgba(0,229,255,0.7)" : "rgba(255,255,255,0.12)" }}
                    aria-checked={abTestEnabled}
                    role="switch"
                  >
                    <span
                      className="absolute top-0.5 left-0.5 h-4 w-4 rounded-full bg-white shadow transition-transform duration-200"
                      style={{ transform: abTestEnabled ? "translateX(16px)" : "translateX(0)" }}
                    />
                  </button>
                </div>
                {abTestEnabled && (
                  <div className="mt-3">
                    <label className="mb-1.5 block text-2xs font-medium text-white/45">
                      Variant B — Template ID
                    </label>
                    <input
                      value={variantBTemplateId}
                      onChange={(e) => setVariantBTemplateId(e.target.value)}
                      placeholder="engagement_template_uuid_or_key"
                      className="w-full rounded-xl border border-glass-border bg-glass-50/40 px-3 py-2 text-xs text-white placeholder-white/20 outline-none focus:border-cyan-neon/50 transition-colors font-mono"
                    />
                    <p className="mt-1 text-2xs text-white/25">
                      Variant A uses this campaign&apos;s template. Variant B uses the ID above.
                      Results visible in campaign detail after activation.
                    </p>
                  </div>
                )}
              </div>

              {error && (
                <p className="rounded-lg border border-red-signal/30 bg-red-signal/10 px-3 py-2 text-xs text-red-signal">
                  {error}
                </p>
              )}
            </div>
          ) : (
            <div className="flex flex-col items-center gap-4 py-6 text-center">
              <div
                className="flex h-14 w-14 items-center justify-center rounded-2xl"
                style={{ background: activated ? "rgba(0,255,136,0.12)" : "rgba(168,85,247,0.12)" }}
              >
                <CheckCircle2 className={`h-7 w-7 ${activated ? "text-green-signal" : "text-purple-plasma"}`} />
              </div>
              <div>
                <p className="font-heading text-lg font-bold text-white">
                  {activated ? "Campaign Sending!" : "Campaign Created"}
                </p>
                <p className="text-sm text-white/40 mt-1">
                  {activated
                    ? `"${name}" is now sending to recipients.`
                    : scheduleEnabled && scheduleFor
                    ? `"${name}" is scheduled for ${new Date(scheduleFor).toLocaleString()}.`
                    : `"${name}" saved as draft — send it now or activate later from the list.`}
                </p>
              </div>
              {error && (
                <p className="rounded-lg border border-red-signal/30 bg-red-signal/10 px-3 py-2 text-xs text-red-signal w-full">
                  {error}
                </p>
              )}
              {!scheduleEnabled && !activated ? (
                <div className="flex items-center gap-3">
                  <button
                    onClick={handleActivateNow}
                    disabled={activating}
                    className="flex items-center gap-1.5 rounded-lg px-5 py-2 text-sm font-semibold text-white transition-all disabled:opacity-40"
                    style={{ background: "linear-gradient(135deg, #00FF88, #00E5FF)" }}
                  >
                    {activating ? (
                      <><span className="h-3.5 w-3.5 animate-spin rounded-full border-2 border-white/30 border-t-white" /> Sending…</>
                    ) : (
                      <><Play size={13} /> Send Now</>
                    )}
                  </button>
                  <button onClick={onClose} className="rounded-lg border border-glass-border px-4 py-2 text-sm text-white/50 hover:text-white transition-colors">
                    Later
                  </button>
                </div>
              ) : (
                <button
                  onClick={onClose}
                  className="rounded-lg px-6 py-2 text-sm font-semibold text-white"
                  style={{ background: "linear-gradient(135deg, #A855F7, #00E5FF)" }}
                >
                  Done
                </button>
              )}
            </div>
          )}
        </div>

        {/* Sticky footer with CTA */}
        {!done && (
          <div className="shrink-0 border-t border-glass-border/50 px-6 py-4 flex justify-end gap-2">
            <button onClick={onClose} className="rounded-lg border border-glass-border px-4 py-2 text-sm text-white/50 hover:text-white transition-colors">
              Cancel
            </button>
            <button
              onClick={handleCreate}
              disabled={
                !name.trim() || !message.trim() || message.length > charMax || saving ||
                (scheduleEnabled && !scheduleFor) || (needsSubject && !subject.trim()) ||
                (recipientMode === "manual" && hasInvalidPhones) ||
                (recipientMode === "customers" && selectedCustomerIds.size === 0) ||
                (recipientMode === "segment" && !selectedSegmentId)
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
        api.weeklyStats().catch(() => [] as WeeklyStat[]),
      ]);
      setCampaigns(listResp.campaigns ?? []);
      setChartStats(statsResp);
    } catch (e) {
      setError((e as { message?: string })?.message ?? "Failed to load campaigns");
    } finally {
      setLoading(false);
    }
  }, [api]);

  useEffect(() => { load(); }, [load]);

  useEffect(() => {
    const id = setInterval(load, 30_000);
    return () => clearInterval(id);
  }, [load]);

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
      setError((e as { message?: string })?.message ?? "Failed to activate campaign");
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
      setError((e as { message?: string })?.message ?? "Failed to cancel campaign");
    } finally {
      setMutatingId(null);
    }
  }

  const kpis = useMemo(() => {
    const active = campaigns.filter(c => c.status === "sending" || c.status === "scheduled").length;
    const sent   = campaigns.reduce((n, c) => n + (c.total_sent ?? 0), 0);
    const delivered = campaigns.reduce((n, c) => n + (c.total_delivered ?? 0), 0);
    const deliveryRate = sent > 0 ? (delivered / sent) * 100 : 0;
    return [
      { label: "Active Campaigns", value: active,           trend: 0, color: "cyan"   as const, format: "number"  as const },
      { label: "Messages Sent",    value: sent,             trend: 0, color: "purple" as const, format: "number"  as const },
      { label: "Delivery Rate",    value: deliveryRate,     trend: 0, color: "green"  as const, format: "percent" as const },
      { label: "Total Campaigns",  value: campaigns.length, trend: 0, color: "amber"  as const, format: "number"  as const },
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
            Engagement Engine · {kpis[0].value} active, {campaigns.length} total · 11 channels
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
          <GlassCard>
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
          <div className="flex items-center justify-between mb-4">
            <div>
              <h2 className="font-heading text-sm font-semibold text-white">Message Volume — This Week</h2>
              <p className="text-2xs font-mono text-white/30 mt-0.5">
                WhatsApp · SMS · Email · Push
                <span className="ml-1.5 text-white/18">· Social (CRM)</span>
              </p>
            </div>
            <BarChart2 size={15} className="text-purple-plasma" />
          </div>

          {/* Channel legend */}
          <div className="flex flex-wrap gap-3 mb-4">
            {[
              { key: "whatsapp", color: "#00FF88", label: "WhatsApp" },
              { key: "sms",      color: "#00E5FF", label: "SMS"      },
              { key: "email",    color: "#A855F7", label: "Email"    },
              { key: "push",     color: "#FFAB00", label: "Push"     },
              { key: "social",   color: "#E01E5A", label: "Social",  dashed: true },
            ].map(({ key, color, label, dashed }) => (
              <span key={key} className="flex items-center gap-1.5 text-2xs font-mono text-white/40">
                <span
                  className="inline-block h-2 rounded-full"
                  style={{
                    width: dashed ? 18 : 12,
                    background: color,
                    opacity: dashed ? 0.6 : 1,
                    backgroundImage: dashed ? `repeating-linear-gradient(90deg, ${color} 0 4px, transparent 4px 6px)` : undefined,
                  }}
                />
                {label}
                {dashed && <span className="text-white/25">(pending)</span>}
              </span>
            ))}
          </div>

          <ResponsiveContainer width="100%" height={160}>
            <AreaChart data={buildChartData(chartStats)} margin={{ top: 0, right: 0, bottom: 0, left: -24 }}>
              <defs>
                <linearGradient id="grad-wa"     x1="0" y1="0" x2="0" y2="1">
                  <stop offset="5%"  stopColor="#00FF88" stopOpacity={0.28} />
                  <stop offset="95%" stopColor="#00FF88" stopOpacity={0}    />
                </linearGradient>
                <linearGradient id="grad-sms"    x1="0" y1="0" x2="0" y2="1">
                  <stop offset="5%"  stopColor="#00E5FF" stopOpacity={0.22} />
                  <stop offset="95%" stopColor="#00E5FF" stopOpacity={0}    />
                </linearGradient>
                <linearGradient id="grad-email"  x1="0" y1="0" x2="0" y2="1">
                  <stop offset="5%"  stopColor="#A855F7" stopOpacity={0.22} />
                  <stop offset="95%" stopColor="#A855F7" stopOpacity={0}    />
                </linearGradient>
                <linearGradient id="grad-push"   x1="0" y1="0" x2="0" y2="1">
                  <stop offset="5%"  stopColor="#FFAB00" stopOpacity={0.22} />
                  <stop offset="95%" stopColor="#FFAB00" stopOpacity={0}    />
                </linearGradient>
                <linearGradient id="grad-social" x1="0" y1="0" x2="0" y2="1">
                  <stop offset="5%"  stopColor="#E01E5A" stopOpacity={0.18} />
                  <stop offset="95%" stopColor="#E01E5A" stopOpacity={0}    />
                </linearGradient>
              </defs>
              <CartesianGrid stroke="rgba(255,255,255,0.04)" strokeDasharray="4 4" vertical={false} />
              <XAxis dataKey="day" tick={{ fill: "rgba(255,255,255,0.3)", fontSize: 11, fontFamily: "JetBrains Mono" }} axisLine={false} tickLine={false} />
              <YAxis tick={{ fill: "rgba(255,255,255,0.3)", fontSize: 11, fontFamily: "JetBrains Mono" }} axisLine={false} tickLine={false} />
              <Tooltip
                contentStyle={{ background: "rgba(13,20,34,0.95)", border: "1px solid rgba(255,255,255,0.08)", borderRadius: 8, fontFamily: "JetBrains Mono", fontSize: 11 }}
                labelStyle={{ color: "rgba(255,255,255,0.4)" }}
              />
              <Area type="monotone" dataKey="whatsapp" stroke="#00FF88" fill="url(#grad-wa)"     strokeWidth={2} />
              <Area type="monotone" dataKey="sms"      stroke="#00E5FF" fill="url(#grad-sms)"    strokeWidth={2} />
              <Area type="monotone" dataKey="email"    stroke="#A855F7" fill="url(#grad-email)"  strokeWidth={2} />
              <Area type="monotone" dataKey="push"     stroke="#FFAB00" fill="url(#grad-push)"   strokeWidth={2} />
              <Area type="monotone" dataKey="social"   stroke="#E01E5A" fill="url(#grad-social)" strokeWidth={1.5} strokeDasharray="4 2" />
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

          <div className="grid grid-cols-[2fr_100px_100px_80px_100px_1fr_80px] gap-3 px-5 py-2.5 border-b border-glass-border">
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
            const isSocial = SOCIAL_CHANNELS.includes(c.channel);
            return (
              <div
                key={c.id}
                onClick={() => router.push(`/campaigns/${c.id}`)}
                className="grid grid-cols-[2fr_100px_100px_80px_100px_1fr_80px] gap-3 items-center px-5 py-3.5 border-b border-glass-border/50 hover:bg-glass-100 transition-colors cursor-pointer"
              >
                <div>
                  <p className="text-xs font-medium text-white">{c.name}</p>
                  <p className="text-2xs font-mono text-white/30 mt-0.5">
                    {c.total_delivered > 0 ? `${c.total_delivered.toLocaleString()} delivered` : "No sends yet"}
                  </p>
                </div>
                <div className="flex items-center gap-1.5">
                  {CHANNEL_ICON[c.channel]}
                  <span className="text-xs text-white/60">
                    {CHANNEL_LABEL[c.channel]}
                  </span>
                  {isSocial && (
                    <span
                      className="rounded px-0.5 text-white/30"
                      style={{ fontSize: 8, fontFamily: "JetBrains Mono, monospace", background: "rgba(0,229,255,0.08)", border: "1px solid rgba(0,229,255,0.15)" }}
                    >
                      CRM
                    </span>
                  )}
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
                      <XClose size={12} />
                    </button>
                  )}
                </div>
              </div>
            );
          })}
        </GlassCard>
      </motion.div>
    </motion.div>

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
