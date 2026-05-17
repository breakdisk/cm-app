"use client";
/**
 * Admin Portal — Settings
 *
 * LIVE:   API Keys → identity /v1/api-keys (list/create/revoke)
 *         Roles & Permissions → identity /v1/users grouped by role
 *         General → identity /v1/tenants/me + PUT /v1/tenants/:id
 *         Webhooks → /v1/webhooks CRUD
 *         Audit Log → identity /v1/audit-log (100 most recent mutations)
 *         Feature Flags → identity /v1/tenants/me + PUT /v1/tenants/:id/tier
 */
import { useCallback, useEffect, useMemo, useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { variants } from "@/lib/design-system/tokens";
import {
  apiKeysApi, apiKeyIdOf,
  type ApiKey, type CreateApiKeyResult,
} from "@/lib/api/api-keys";
import {
  createIdentityApi, tenantIdOf,
  type TenantSnapshot, type TenantTier, type TenantUser,
} from "@/lib/api/identity";
import {
  channelsApi,
  type ChannelConfigView,
  type ChannelView,
  type UpdateChannelConfigRequest,
} from "@/lib/api/channels";
import { authFetch } from "@/lib/auth/auth-fetch";
import { usePermissions } from "@/hooks/usePermissions";

const identityApi = createIdentityApi();

const API_BASE = process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:8000";

// Friendly role descriptions paired with permission summaries from
// libs/auth/src/rbac.rs::default_permissions_for_role. Kept here as a UI
// concern — when identity ships GET /v1/roles we can lift this to runtime.
const ROLE_DESCRIPTIONS: Record<string, string> = {
  admin:      "Full access — shipments, dispatch, drivers, fleet, billing, users, carriers, customers, compliance",
  dispatcher: "Dispatch console + driver read · no settings or billing",
  merchant:   "Create/track shipments · analytics read · CDP read",
  driver:     "Read own tasks · COD-self · no admin surface",
  finance:    "Billing reconcile + export · analytics read",
  readonly:   "All dashboards read-only",
  customer:   "Create/track own shipments · cancel",
};

const ROLE_ORDER = ["admin", "dispatcher", "merchant", "driver", "finance", "readonly", "customer"];

const ALL_TABS = ["General", "API Keys", "Webhooks", "Roles & Permissions", "Feature Flags", "Audit Log"] as const;
type Tab = (typeof ALL_TABS)[number];

const TAB_PERMISSIONS: Record<Tab, string> = {
  "General":              "users:manage",
  "API Keys":             "api_keys:manage",
  "Webhooks":             "webhooks:manage",
  "Roles & Permissions":  "users:manage",
  "Feature Flags":        "users:manage",
  "Audit Log":            "users:manage",
};

interface AuditEntry {
  id:          string;
  tenant_id:   string;
  actor_id?:   string | null;
  actor_email?: string | null;
  action:      string;
  resource:    string;
  ip?:         string | null;
  created_at:  string;
}

const ACTION_COLOR: Record<string, string> = {
  "api_key.created":          "cyan",
  "api_key.revoked":          "red",
  "webhook.created":          "cyan",
  "webhook.disabled":         "amber",
  "role.user_assigned":       "purple",
  "billing.invoice_exported": "green",
  "shipment.manual_override": "amber",
  "tenant.updated":           "cyan",
  "tenant.settings_updated":  "cyan",
  "tenant.tier_updated":      "purple",
};

export default function SettingsPage() {
  const { hasPermission } = usePermissions();
  const tabs = ALL_TABS.filter((t) => hasPermission(TAB_PERMISSIONS[t]));
  const [activeTab, setActiveTab] = useState<Tab>("General");
  const effectiveTab: Tab = tabs.includes(activeTab) ? activeTab : (tabs[0] ?? "General");

  return (
    <motion.div
      variants={variants.staggerContainer}
      initial="hidden"
      animate="visible"
      className="p-6 space-y-6"
    >
      {/* Header */}
      <motion.div variants={variants.fadeInUp}>
        <h1 className="text-2xl font-bold text-white font-space-grotesk">Settings</h1>
        <p className="text-white/40 text-sm mt-1">Tenant configuration, access control, and audit trail</p>
      </motion.div>

      {/* Tab bar */}
      <motion.div variants={variants.fadeInUp}>
        {tabs.length === 0 ? (
          <p className="text-sm text-white/40 font-mono">
            You don&apos;t have permission to view any settings.
          </p>
        ) : (
          <div className="flex gap-1 bg-white/[0.03] border border-white/[0.08] rounded-xl p-1 w-fit">
            {tabs.map((tab) => (
              <button
                key={tab}
                onClick={() => setActiveTab(tab)}
                className={`px-4 py-2 rounded-lg text-sm font-medium transition-all ${
                  effectiveTab === tab
                    ? "bg-[#00E5FF]/10 text-[#00E5FF] border border-[#00E5FF]/20"
                    : "text-white/40 hover:text-white/70"
                }`}
              >
                {tab}
              </button>
            ))}
          </div>
        )}
      </motion.div>

      {/* General */}
      {effectiveTab === "General" && tabs.includes("General") && (
        <motion.div variants={variants.fadeInUp} className="grid grid-cols-1 lg:grid-cols-2 gap-6">
          <TenantProfileCard />
          <NotificationChannelsCard />
        </motion.div>
      )}

      {/* API Keys — live */}
      {effectiveTab === "API Keys" && tabs.includes("API Keys") && <ApiKeysTab />}

      {/* Webhooks — backed by /v1/webhooks (CRUD) on the new webhooks
          service. The signing secret is returned exactly once at create
          time; the modal surfaces it for copy-paste. */}
      {effectiveTab === "Webhooks" && tabs.includes("Webhooks") && <WebhooksTab />}

      {/* Roles — derived from identity /v1/users grouped by role. */}
      {effectiveTab === "Roles & Permissions" && tabs.includes("Roles & Permissions") && <RolesTab />}

      {/* Feature Flags — tier-driven entitlements + admin plan change. */}
      {effectiveTab === "Feature Flags" && tabs.includes("Feature Flags") && <FeatureFlagsTab />}

      {/* Audit Log — live from identity /v1/audit-log (100 most recent). */}
      {effectiveTab === "Audit Log" && tabs.includes("Audit Log") && <AuditLogTab />}
    </motion.div>
  );
}

// ── Notification Channels card (General tab) ────────────────────────────────
// Live: engagement /v1/channel-configs (GET/PUT) for per-tenant config,
// and /v1/channels/health for provider readiness signals.

type ChannelKey = "whatsapp" | "sms" | "email" | "push";

const CHANNEL_META: Array<{ key: ChannelKey; label: string; provider: string }> = [
  { key: "whatsapp", label: "WhatsApp", provider: "Twilio" },
  { key: "sms",      label: "SMS",      provider: "Twilio" },
  { key: "email",    label: "Email",    provider: "AWS SES" },
  { key: "push",     label: "Push",     provider: "Expo" },
];

// ── Config form for a single channel ────────────────────────────────────────

interface ChannelFormState {
  enabled: boolean;
  account_sid: string;
  auth_token: string;
  from_number: string;
  from_address: string;
  from_name: string;
}

function defaultForm(ch: ChannelKey, view: ChannelView): ChannelFormState {
  return {
    enabled:      view.enabled,
    account_sid:  view.account_sid_hint ? "" : "",  // never pre-fill; show hint only
    auth_token:   "",
    from_number:  view.from_number ?? "",
    from_address: view.from_address ?? "",
    from_name:    view.from_name ?? "",
  };
}

interface ChannelConfigDrawerProps {
  channelKey: ChannelKey;
  label: string;
  provider: string;
  view: ChannelView;
  onClose: () => void;
  onSaved: (updated: ChannelConfigView) => void;
}

function ChannelConfigDrawer({ channelKey, label, provider, view, onClose, onSaved }: ChannelConfigDrawerProps) {
  const [form, setForm] = useState<ChannelFormState>(() => defaultForm(channelKey, view));
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const set = (k: keyof ChannelFormState, v: string | boolean) =>
    setForm((f) => ({ ...f, [k]: v }));

  const handleSave = async () => {
    setSaving(true);
    setError(null);
    try {
      const req: UpdateChannelConfigRequest = {};
      if (channelKey === "whatsapp") {
        req.whatsapp = {
          enabled:     form.enabled,
          account_sid: form.account_sid || undefined,
          auth_token:  form.auth_token  || undefined,
          from_number: form.from_number || undefined,
        };
      } else if (channelKey === "sms") {
        req.sms = {
          enabled:     form.enabled,
          account_sid: form.account_sid || undefined,
          auth_token:  form.auth_token  || undefined,
          from_number: form.from_number || undefined,
        };
      } else if (channelKey === "email") {
        req.email = {
          enabled:      form.enabled,
          from_address: form.from_address || undefined,
          from_name:    form.from_name    || undefined,
        };
      } else {
        req.push = { enabled: form.enabled };
      }
      const updated = await channelsApi.updateConfig(req);
      onSaved(updated);
      onClose();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Save failed");
    } finally {
      setSaving(false);
    }
  };

  const showTwilio = channelKey === "whatsapp" || channelKey === "sms";
  const showEmail  = channelKey === "email";

  return (
    <motion.div
      initial={{ opacity: 0, x: 32 }}
      animate={{ opacity: 1, x: 0 }}
      exit={{ opacity: 0, x: 32 }}
      transition={{ duration: 0.22, ease: [0.16, 1, 0.3, 1] }}
      className="fixed inset-y-0 right-0 w-full max-w-sm z-50 flex flex-col bg-[#0a0e1a]/95 backdrop-blur-xl border-l border-white/[0.08] shadow-2xl"
    >
      {/* Header */}
      <div className="flex items-center justify-between px-6 py-5 border-b border-white/[0.08]">
        <div>
          <p className="text-xs text-white/40 font-mono">{provider}</p>
          <h3 className="text-base font-semibold text-white">{label} Channel</h3>
        </div>
        <button
          onClick={onClose}
          className="text-white/40 hover:text-white transition-colors text-lg font-mono"
        >
          ✕
        </button>
      </div>

      {/* Body */}
      <div className="flex-1 overflow-y-auto px-6 py-6 space-y-5">
        {error && (
          <p className="text-xs text-red-400 font-mono bg-red-400/10 border border-red-400/20 rounded-lg px-3 py-2">
            {error}
          </p>
        )}

        {/* Enable toggle */}
        <div className="flex items-center justify-between p-3 bg-white/[0.03] rounded-lg border border-white/[0.06]">
          <span className="text-sm text-white">Enable channel</span>
          <button
            onClick={() => set("enabled", !form.enabled)}
            className={`relative w-11 h-6 rounded-full transition-colors duration-200 ${
              form.enabled ? "bg-[#00E5FF]/30 border border-[#00E5FF]/40" : "bg-white/10 border border-white/20"
            }`}
          >
            <span
              className={`absolute top-0.5 left-0.5 w-5 h-5 rounded-full transition-transform duration-200 ${
                form.enabled
                  ? "translate-x-5 bg-[#00E5FF] shadow-[0_0_8px_rgba(0,229,255,0.6)]"
                  : "translate-x-0 bg-white/40"
              }`}
            />
          </button>
        </div>

        {/* Twilio credentials */}
        {showTwilio && (
          <>
            <div className="space-y-1">
              <label className="text-xs text-white/50 font-mono uppercase tracking-wide">
                Account SID{view.account_sid_hint && <span className="ml-2 text-white/30 normal-case">current: {view.account_sid_hint}</span>}
              </label>
              <input
                type="text"
                value={form.account_sid}
                onChange={(e) => set("account_sid", e.target.value)}
                placeholder="ACxxxxxxxxxxxxxxxxx"
                className="w-full bg-white/[0.04] border border-white/[0.1] rounded-lg px-3 py-2 text-sm text-white font-mono placeholder:text-white/20 focus:outline-none focus:border-[#00E5FF]/40"
              />
            </div>
            <div className="space-y-1">
              <label className="text-xs text-white/50 font-mono uppercase tracking-wide">
                Auth Token <span className="text-white/30 normal-case">(leave blank to keep current)</span>
              </label>
              <input
                type="password"
                value={form.auth_token}
                onChange={(e) => set("auth_token", e.target.value)}
                placeholder="••••••••••••••••••••••••••••••••"
                className="w-full bg-white/[0.04] border border-white/[0.1] rounded-lg px-3 py-2 text-sm text-white font-mono placeholder:text-white/20 focus:outline-none focus:border-[#00E5FF]/40"
              />
            </div>
            <div className="space-y-1">
              <label className="text-xs text-white/50 font-mono uppercase tracking-wide">From Number</label>
              <input
                type="text"
                value={form.from_number}
                onChange={(e) => set("from_number", e.target.value)}
                placeholder={channelKey === "whatsapp" ? "whatsapp:+15005550006" : "+15005550006"}
                className="w-full bg-white/[0.04] border border-white/[0.1] rounded-lg px-3 py-2 text-sm text-white font-mono placeholder:text-white/20 focus:outline-none focus:border-[#00E5FF]/40"
              />
            </div>
          </>
        )}

        {/* Email credentials */}
        {showEmail && (
          <>
            <div className="space-y-1">
              <label className="text-xs text-white/50 font-mono uppercase tracking-wide">From Address</label>
              <input
                type="email"
                value={form.from_address}
                onChange={(e) => set("from_address", e.target.value)}
                placeholder="noreply@yourcompany.com"
                className="w-full bg-white/[0.04] border border-white/[0.1] rounded-lg px-3 py-2 text-sm text-white font-mono placeholder:text-white/20 focus:outline-none focus:border-[#00E5FF]/40"
              />
            </div>
            <div className="space-y-1">
              <label className="text-xs text-white/50 font-mono uppercase tracking-wide">Sender Name</label>
              <input
                type="text"
                value={form.from_name}
                onChange={(e) => set("from_name", e.target.value)}
                placeholder="Your Company"
                className="w-full bg-white/[0.04] border border-white/[0.1] rounded-lg px-3 py-2 text-sm text-white font-mono placeholder:text-white/20 focus:outline-none focus:border-[#00E5FF]/40"
              />
              <p className="text-2xs font-mono text-white/30">
                AWS SES handles email delivery. IAM credentials come from server env vars.
              </p>
            </div>
          </>
        )}

        {/* Push — no extra credentials */}
        {channelKey === "push" && (
          <p className="text-xs text-white/40 font-mono bg-white/[0.03] border border-white/[0.06] rounded-lg px-3 py-4 text-center">
            Push notifications use Expo with tokens resolved via the identity service.
            No additional credentials required.
          </p>
        )}
      </div>

      {/* Footer */}
      <div className="px-6 py-4 border-t border-white/[0.08]">
        <button
          onClick={handleSave}
          disabled={saving}
          className="w-full py-2.5 rounded-lg text-sm font-semibold transition-all
            bg-[#00E5FF]/10 text-[#00E5FF] border border-[#00E5FF]/30
            hover:bg-[#00E5FF]/20 disabled:opacity-40 disabled:cursor-not-allowed"
        >
          {saving ? "Saving…" : "Save Changes"}
        </button>
      </div>
    </motion.div>
  );
}

// ── Main card ────────────────────────────────────────────────────────────────

function NotificationChannelsCard() {
  const [config, setConfig]   = useState<ChannelConfigView | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError]     = useState<string | null>(null);
  const [editing, setEditing] = useState<ChannelKey | null>(null);
  const [toggling, setToggling] = useState<ChannelKey | null>(null);

  useEffect(() => {
    channelsApi.getConfig()
      .then(setConfig)
      .catch((e) => setError(e instanceof Error ? e.message : "Failed to load channel config"))
      .finally(() => setLoading(false));
  }, []);

  const handleToggle = async (key: ChannelKey) => {
    if (!config || toggling) return;
    setToggling(key);
    try {
      const current = config[key];
      const req: UpdateChannelConfigRequest = { [key]: { enabled: !current.enabled } };
      const updated = await channelsApi.updateConfig(req);
      setConfig(updated);
    } catch {
      // silent — user can open drawer for details
    } finally {
      setToggling(null);
    }
  };

  const statusDot = (view: ChannelView) => {
    if (view.enabled && view.configured) return "bg-[#00FF88] shadow-[0_0_6px_rgba(0,255,136,0.5)]";
    if (view.enabled && !view.configured) return "bg-[#FFAB00] shadow-[0_0_6px_rgba(255,171,0,0.5)]";
    return "bg-white/20";
  };

  const statusLabel = (view: ChannelView) => {
    if (view.enabled && view.configured) return "active";
    if (view.enabled && !view.configured) return "not configured";
    return "disabled";
  };

  return (
    <>
      <GlassCard>
        <div className="flex items-center justify-between mb-4">
          <h3 className="text-sm font-semibold text-white">Notification Channels</h3>
          <NeonBadge variant="cyan">
            {loading ? "…" : `${config ? Object.values(config).filter((v) => (v as ChannelView).enabled).length : 0} active`}
          </NeonBadge>
        </div>
        {error && (
          <p className="text-xs text-red-400 font-mono mb-3">{error}</p>
        )}
        <div className="space-y-2">
          {loading ? (
            <p className="text-xs text-white/40 font-mono py-4 text-center">loading channels…</p>
          ) : (
            CHANNEL_META.map(({ key, label, provider }) => {
              const view = config?.[key] ?? { enabled: false, configured: false };
              const isToggling = toggling === key;
              return (
                <div
                  key={key}
                  className="flex items-center gap-3 p-3 bg-white/[0.03] rounded-lg border border-white/[0.06] group"
                >
                  {/* Status dot */}
                  <div className={`w-2 h-2 rounded-full flex-shrink-0 transition-all ${statusDot(view)}`} />

                  {/* Name + provider */}
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2">
                      <span className="text-sm text-white">{label}</span>
                      <span className="text-2xs text-white/30 font-mono">{provider}</span>
                    </div>
                    <span className="text-2xs font-mono text-white/30">{statusLabel(view)}</span>
                  </div>

                  {/* Toggle */}
                  <button
                    onClick={() => handleToggle(key)}
                    disabled={!!isToggling}
                    title={view.enabled ? "Disable" : "Enable"}
                    className={`relative w-9 h-5 rounded-full flex-shrink-0 transition-colors duration-200 disabled:opacity-50 ${
                      view.enabled
                        ? "bg-[#00E5FF]/30 border border-[#00E5FF]/40"
                        : "bg-white/10 border border-white/20"
                    }`}
                  >
                    <span
                      className={`absolute top-0.5 left-0.5 w-4 h-4 rounded-full transition-transform duration-200 ${
                        view.enabled
                          ? "translate-x-4 bg-[#00E5FF] shadow-[0_0_6px_rgba(0,229,255,0.5)]"
                          : "translate-x-0 bg-white/40"
                      }`}
                    />
                  </button>

                  {/* Configure button */}
                  <button
                    onClick={() => setEditing(key)}
                    className="text-xs text-white/30 hover:text-[#00E5FF] font-mono transition-colors flex-shrink-0"
                  >
                    configure
                  </button>
                </div>
              );
            })
          )}
        </div>
        <p className="text-2xs font-mono text-white/30 mt-3">
          Source: engagement <span className="text-[#00E5FF]">/v1/channel-configs</span> ·
          amber = enabled but missing credentials
        </p>
      </GlassCard>

      {/* Config drawer */}
      <AnimatePresence>
        {editing && config && (() => {
          const meta = CHANNEL_META.find((m) => m.key === editing)!;
          return (
            <ChannelConfigDrawer
              key={editing}
              channelKey={editing}
              label={meta.label}
              provider={meta.provider}
              view={config[editing]}
              onClose={() => setEditing(null)}
              onSaved={(updated) => setConfig(updated)}
            />
          );
        })()}
      </AnimatePresence>
    </>
  );
}

// ── Feature Flags tab ────────────────────────────────────────────────────────
// Driven by tenant.subscription_tier — backend serialises as snake_case:
// "starter" | "growth" | "business" | "enterprise".
// Tier changes call PUT /v1/tenants/:id/tier (admin only, audited).

interface TierFlag {
  flag:    string;
  /** Lowest tier that grants this feature. */
  minTier: TenantTier;
}

const TIER_RANK: Record<TenantTier, number> = {
  starter: 0, growth: 1, business: 2, enterprise: 3,
};

const TIER_LABELS: Record<TenantTier, string> = {
  starter:    "Starter",
  growth:     "Growth",
  business:   "Business",
  enterprise: "Enterprise",
};

// Thresholds aligned with libs/types/src/lib.rs::SubscriptionTier capability methods.
// allows_ai_features() → Business+; allows_white_label() → Enterprise.
const TIER_FLAGS: TierFlag[] = [
  { flag: "Real-Time Driver Tracking", minTier: "starter"    },
  { flag: "COD Auto-Reconciliation",   minTier: "starter"    },
  { flag: "Balikbayan Box Service",    minTier: "starter"    },
  { flag: "AI Dispatch Agent",         minTier: "growth"     },
  { flag: "Same-Day Delivery",         minTier: "growth"     },
  { flag: "AI Recovery Agent",         minTier: "business"   },
  { flag: "Loyalty Program",           minTier: "business"   },
  { flag: "Dynamic Pricing",           minTier: "business"   },
  { flag: "Enterprise MCP Extension",  minTier: "enterprise" },
];

const ALL_TIERS: TenantTier[] = ["starter", "growth", "business", "enterprise"];

function FeatureFlagsTab() {
  const { hasPermission } = usePermissions();
  const canManage = hasPermission("users:manage");

  const [tenant,        setTenant]        = useState<TenantSnapshot | null>(null);
  const [error,         setError]         = useState<string | null>(null);
  const [loading,       setLoading]       = useState(true);
  const [showUpgrade,   setShowUpgrade]   = useState(false);
  const [selectedTier,  setSelectedTier]  = useState<TenantTier>("starter");
  const [upgrading,     setUpgrading]     = useState(false);
  const [upgradeError,  setUpgradeError]  = useState<string | null>(null);
  const [upgradeSaved,  setUpgradeSaved]  = useState(false);

  const load = useCallback(async () => {
    setError(null);
    try {
      const t = await identityApi.getTenant();
      setTenant(t);
    } catch (e) {
      const err = e as { message?: string };
      setError(err?.message ?? "Failed to load tenant tier");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { load(); }, [load]);

  async function handleUpgrade() {
    if (!tenant) return;
    setUpgrading(true);
    setUpgradeError(null);
    try {
      await identityApi.upgradeTier(tenantIdOf(tenant), selectedTier);
      setUpgradeSaved(true);
      setShowUpgrade(false);
      await load();
      setTimeout(() => setUpgradeSaved(false), 2500);
    } catch (e) {
      const err = e as { message?: string };
      setUpgradeError(err?.message ?? "Upgrade failed");
    } finally {
      setUpgrading(false);
    }
  }

  const tier = tenant?.subscription_tier ?? null;

  return (
    <motion.div variants={variants.fadeInUp} className="space-y-4">
      {/* Header row */}
      <div className="flex items-center justify-between">
        <div>
          <p className="text-sm text-white/40">
            Tier-derived feature entitlements for this tenant.
            Admin plan changes are audited and take effect immediately.
          </p>
        </div>
        {canManage && !loading && (
          <button
            onClick={() => {
              setSelectedTier(tier ?? "starter");
              setUpgradeError(null);
              setShowUpgrade(true);
            }}
            className="shrink-0 px-4 py-2 text-sm font-medium text-[#050810] bg-[#A855F7] rounded-lg hover:bg-[#A855F7]/90 transition-colors"
          >
            Change Plan
          </button>
        )}
      </div>

      {error && <p className="text-xs text-red-signal font-mono">{error}</p>}
      {upgradeSaved && <p className="text-xs text-green-signal font-mono">✓ Plan updated</p>}

      {/* Effective tier badge */}
      <div className="flex items-center gap-2">
        <span className="text-xs text-white/40">Effective tier:</span>
        <NeonBadge variant="purple">
          {loading ? "loading…" : (tier ? TIER_LABELS[tier] : "unknown")}
        </NeonBadge>
      </div>

      {/* Flag grid */}
      <GlassCard padding="none">
        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3">
          {TIER_FLAGS.map((f, i) => {
            const enabled = tier !== null && TIER_RANK[tier] >= TIER_RANK[f.minTier];
            return (
              <div
                key={f.flag}
                className={`flex items-center justify-between px-5 py-4 border-white/[0.06] ${
                  i % 3 !== 2 ? "border-r" : ""
                } ${i < TIER_FLAGS.length - 3 ? "border-b" : ""}`}
              >
                <div className="flex flex-col">
                  <span className="text-sm text-white/80 font-medium">{f.flag}</span>
                  <span className="text-2xs font-mono text-white/30 mt-0.5">{TIER_LABELS[f.minTier]}+</span>
                </div>
                <NeonBadge variant={enabled ? "green" : "red"}>{enabled ? "ON" : "OFF"}</NeonBadge>
              </div>
            );
          })}
        </div>
      </GlassCard>

      <p className="text-2xs font-mono text-white/30">
        Source: identity <span className="text-[#00E5FF]">GET /v1/tenants/me</span> ·
        changes via <span className="text-[#00E5FF]">PUT /v1/tenants/:id/tier</span>
      </p>

      {/* Change Plan modal */}
      {showUpgrade && (
        <div className="fixed inset-0 bg-canvas/80 backdrop-blur-sm flex items-center justify-center z-50">
          <div className="bg-canvas border border-white/10 rounded-xl p-6 w-full max-w-sm space-y-4">
            <div className="flex items-center justify-between">
              <h3 className="text-base font-bold text-white">Change Plan</h3>
              <button
                onClick={() => setShowUpgrade(false)}
                className="text-white/40 hover:text-white"
              >
                ✕
              </button>
            </div>

            <div className="space-y-2">
              {ALL_TIERS.map((t) => (
                <button
                  key={t}
                  onClick={() => setSelectedTier(t)}
                  className={`w-full flex items-center justify-between px-4 py-3 rounded-lg border transition-colors ${
                    selectedTier === t
                      ? "border-[#A855F7]/60 bg-[#A855F7]/10 text-[#A855F7]"
                      : "border-white/10 bg-white/[0.02] text-white/60 hover:border-white/20"
                  }`}
                >
                  <span className="text-sm font-medium">{TIER_LABELS[t]}</span>
                  {tier === t && (
                    <span className="text-2xs font-mono text-white/40">current</span>
                  )}
                </button>
              ))}
            </div>

            {upgradeError && (
              <p className="text-xs text-red-signal font-mono">{upgradeError}</p>
            )}

            <div className="flex justify-end gap-2 pt-1">
              <button
                onClick={() => setShowUpgrade(false)}
                disabled={upgrading}
                className="px-3 py-1.5 text-xs text-white/60 hover:text-white"
              >
                Cancel
              </button>
              <button
                onClick={handleUpgrade}
                disabled={upgrading || selectedTier === tier}
                className="px-4 py-2 text-sm font-medium text-[#050810] bg-[#A855F7] rounded-lg hover:bg-[#A855F7]/90 transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
              >
                {upgrading ? "Saving…" : "Set Plan"}
              </button>
            </div>
          </div>
        </div>
      )}
    </motion.div>
  );
}

// ── API Keys tab ──────────────────────────────────────────────────────────────
// Live: identity /v1/api-keys list + create + revoke.

function ApiKeysTab() {
  const [keys, setKeys]               = useState<ApiKey[]>([]);
  const [loading, setLoading]         = useState(true);
  const [error, setError]             = useState<string | null>(null);
  const [creating, setCreating]       = useState(false);
  const [newName, setNewName]         = useState("");
  const [newScopes, setNewScopes]     = useState("shipments:read,shipments:write");
  const [justCreated, setJustCreated] = useState<CreateApiKeyResult | null>(null);
  const [revokingId, setRevokingId]   = useState<string | null>(null);

  const load = useCallback(async () => {
    setError(null);
    try {
      setKeys(await apiKeysApi.list());
    } catch (e) {
      const err = e as { message?: string };
      setError(err?.message ?? "Failed to load API keys");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { load(); }, [load]);

  async function handleCreate() {
    if (!newName.trim()) return;
    setCreating(true);
    setError(null);
    try {
      const result = await apiKeysApi.create({
        name:   newName.trim(),
        scopes: newScopes.split(",").map((s) => s.trim()).filter(Boolean),
      });
      setJustCreated(result);
      setNewName("");
      await load();
    } catch (e) {
      const err = e as { message?: string };
      setError(err?.message ?? "Create failed");
    } finally {
      setCreating(false);
    }
  }

  async function handleRevoke(id: string) {
    setRevokingId(id);
    try {
      await apiKeysApi.revoke(id);
      await load();
    } catch (e) {
      const err = e as { message?: string };
      setError(err?.message ?? "Revoke failed");
    } finally {
      setRevokingId(null);
    }
  }

  return (
    <motion.div variants={variants.fadeInUp} className="space-y-4">
      {error && (
        <GlassCard>
          <p className="text-xs text-[#FF3B5C] font-mono">{error}</p>
        </GlassCard>
      )}

      {justCreated && (
        <GlassCard>
          <h3 className="text-sm font-semibold text-white mb-3">New API key — copy it now, you won't see it again</h3>
          <div className="space-y-3">
            <div className="flex items-center gap-3 bg-black/50 border border-[#00FF88]/30 rounded-lg p-4">
              <span className="flex-1 font-mono text-[#00FF88] text-sm break-all">{justCreated.raw_key}</span>
              <button
                onClick={() => navigator.clipboard?.writeText(justCreated.raw_key)}
                className="text-xs text-white/60 hover:text-white border border-white/10 rounded px-3 py-1.5"
              >
                Copy
              </button>
            </div>
            <p className="text-xs text-white/40">
              Key prefix <span className="font-mono text-white/60">{justCreated.key_prefix}</span>
              {justCreated.expires_at ? ` · expires ${new Date(justCreated.expires_at).toLocaleDateString()}` : " · no expiry"}
            </p>
            <button
              onClick={() => setJustCreated(null)}
              className="px-3 py-1.5 text-xs text-white/60 border border-white/10 rounded"
            >
              I've saved it
            </button>
          </div>
        </GlassCard>
      )}

      {/* Create form */}
      <GlassCard>
        <h3 className="text-sm font-semibold text-white mb-3">Generate new API key</h3>
        <div className="grid grid-cols-1 md:grid-cols-[2fr_3fr_auto] gap-3">
          <input
            value={newName}
            onChange={(e) => setNewName(e.target.value)}
            placeholder="Key name — e.g. Production API Key"
            maxLength={100}
            className="rounded-lg border border-white/10 bg-white/[0.03] px-3 py-2 text-sm text-white placeholder-white/25 outline-none focus:border-[#00E5FF]/40"
          />
          <input
            value={newScopes}
            onChange={(e) => setNewScopes(e.target.value)}
            placeholder="Scopes (comma-separated)"
            className="rounded-lg border border-white/10 bg-white/[0.03] px-3 py-2 text-sm font-mono text-white placeholder-white/25 outline-none focus:border-[#00E5FF]/40"
          />
          <button
            onClick={handleCreate}
            disabled={creating || !newName.trim()}
            className="rounded-lg bg-[#00E5FF] px-4 py-2 text-xs font-semibold text-[#050810] disabled:opacity-40"
          >
            {creating ? "Creating…" : "Create"}
          </button>
        </div>
      </GlassCard>

      {/* Existing keys */}
      <GlassCard padding="none">
        <div className="flex items-center justify-between px-5 py-4 border-b border-white/[0.08]">
          <h2 className="font-heading text-sm font-semibold text-white">Active API Keys</h2>
          <span className="text-2xs font-mono text-white/30">
            {loading ? "loading…" : `${keys.length} key${keys.length === 1 ? "" : "s"}`}
          </span>
        </div>
        <table className="w-full text-sm">
          <thead>
            <tr className="border-b border-white/[0.08]">
              {["Name", "Prefix", "Scopes", "Last Used", "Status", ""].map((h) => (
                <th key={h} className="text-left px-4 py-3 text-xs text-white/30 uppercase tracking-widest font-mono">{h}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {!loading && keys.length === 0 ? (
              <tr>
                <td colSpan={6} className="px-5 py-10 text-center text-xs text-white/40 font-mono">
                  No API keys yet. Generate one above.
                </td>
              </tr>
            ) : (
              keys.map((k) => {
                const id = apiKeyIdOf(k);
                const busy = revokingId === id;
                return (
                  <tr key={id} className="border-b border-white/[0.04] hover:bg-white/[0.02]">
                    <td className="px-4 py-3 text-white font-medium">{k.name}</td>
                    <td className="px-4 py-3 font-mono text-[#00E5FF] text-xs">{k.key_prefix}…</td>
                    <td className="px-4 py-3">
                      <div className="flex flex-wrap gap-1">
                        {k.scopes.length === 0 ? (
                          <span className="text-2xs font-mono text-white/30">no scopes</span>
                        ) : (
                          k.scopes.map((s) => (
                            <span key={s} className="text-[10px] px-2 py-0.5 rounded-full bg-[#A855F7]/10 text-[#A855F7] border border-[#A855F7]/20 font-mono">{s}</span>
                          ))
                        )}
                      </div>
                    </td>
                    <td className="px-4 py-3 text-white/40 text-xs font-mono">
                      {k.last_used_at ? new Date(k.last_used_at).toLocaleDateString() : "never"}
                    </td>
                    <td className="px-4 py-3">
                      <NeonBadge variant={k.is_active ? "green" : "red"} dot>
                        {k.is_active ? "active" : "revoked"}
                      </NeonBadge>
                    </td>
                    <td className="px-4 py-3">
                      {k.is_active && (
                        <button
                          onClick={() => handleRevoke(id)}
                          disabled={busy}
                          className="text-xs text-[#FF3B5C] hover:text-[#FF3B5C]/70 disabled:opacity-40"
                        >
                          {busy ? "…" : "Revoke"}
                        </button>
                      )}
                    </td>
                  </tr>
                );
              })
            )}
          </tbody>
        </table>
      </GlassCard>
    </motion.div>
  );
}

// ── Roles tab — live from /v1/users grouped by role ──────────────────────────

function RolesTab() {
  const [users, setUsers]     = useState<TenantUser[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError]     = useState<string | null>(null);

  const load = useCallback(async () => {
    setError(null);
    try {
      const result = await identityApi.listUsers();
      setUsers(Array.isArray(result.data) ? result.data : []);
    } catch (e) {
      const err = e as { message?: string };
      setError(err?.message ?? "Failed to load users");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { load(); }, [load]);

  // Bucket users by role. A user can hold multiple roles, so they count
  // toward each one — matches how the JWT permission union works.
  const roleCounts = useMemo(() => {
    const buckets = new Map<string, number>();
    for (const u of users) {
      if (!Array.isArray(u.roles)) continue;
      for (const r of u.roles) {
        buckets.set(r, (buckets.get(r) ?? 0) + 1);
      }
    }
    // Stable display order: known roles first per ROLE_ORDER, then any
    // unknown roles alphabetically so nothing gets hidden.
    const known = ROLE_ORDER.filter((r) => buckets.has(r));
    const unknown = Array.from(buckets.keys())
      .filter((r) => !ROLE_ORDER.includes(r))
      .sort();
    return [...known, ...unknown].map((r) => ({
      role:        r,
      users:       buckets.get(r) ?? 0,
      description: ROLE_DESCRIPTIONS[r] ?? "Custom role — see libs/auth/src/rbac.rs",
    }));
  }, [users]);

  return (
    <motion.div variants={variants.fadeInUp} className="space-y-4">
      {error && (
        <p className="text-xs text-red-signal font-mono">{error}</p>
      )}
      <GlassCard padding="none">
        <table className="w-full text-sm">
          <thead>
            <tr className="border-b border-white/[0.08]">
              {["Role", "Users", "Permissions Summary"].map((h) => (
                <th key={h} className="text-left px-4 py-3 text-xs text-white/30 uppercase tracking-widest font-mono">{h}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {loading ? (
              <tr><td colSpan={3} className="px-4 py-6 text-center text-xs text-white/30 font-mono">loading roles…</td></tr>
            ) : roleCounts.length === 0 ? (
              <tr><td colSpan={3} className="px-4 py-6 text-center text-xs text-white/30 font-mono">No users found in this tenant</td></tr>
            ) : (
              roleCounts.map((r) => (
                <tr key={r.role} className="border-b border-white/[0.04] hover:bg-white/[0.02]">
                  <td className="px-4 py-3 text-white font-semibold capitalize">{r.role}</td>
                  <td className="px-4 py-3">
                    <span className="px-2 py-0.5 rounded-full bg-[#A855F7]/10 text-[#A855F7] text-xs border border-[#A855F7]/20">
                      {r.users}
                    </span>
                  </td>
                  <td className="px-4 py-3 text-white/50 text-xs">{r.description}</td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </GlassCard>
      <p className="text-2xs font-mono text-white/30">
        Source: identity <span className="text-[#00E5FF]">/v1/users</span> · grouped by user.roles[]
        · descriptions mirror libs/auth/src/rbac.rs::default_permissions_for_role.
      </p>
    </motion.div>
  );
}

// ── Tenant Profile (General tab) ─────────────────────────────────────────────
// Backed by identityApi.getTenant() (read) + identityApi.updateTenant() (write).
// Slug + tier + status are intentionally read-only — those have first-class
// endpoints with cross-service side-effects.

function TenantProfileCard() {
  const [tenant,  setTenant]  = useState<TenantSnapshot | null>(null);
  const [loading, setLoading] = useState(true);
  const [error,   setError]   = useState<string | null>(null);
  const [saving,  setSaving]  = useState(false);
  const [saved,   setSaved]   = useState(false);
  const [form, setForm] = useState<{ name: string; owner_email: string } | null>(null);

  const load = useCallback(async () => {
    setError(null);
    try {
      const t = await identityApi.getTenant();
      setTenant(t);
      setForm({ name: t.name, owner_email: t.owner_email });
    } catch (e) {
      const err = e as { message?: string };
      setError(err?.message ?? "Failed to load tenant");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { load(); }, [load]);

  async function handleSave() {
    if (!tenant || !form) return;
    setSaving(true);
    setError(null);
    try {
      const updated = await identityApi.updateTenant(tenantIdOf(tenant), {
        name: form.name,
        owner_email: form.owner_email,
      });
      setTenant(updated);
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
    } catch (e) {
      const err = e as { message?: string };
      setError(err?.message ?? "Save failed");
    } finally {
      setSaving(false);
    }
  }

  return (
    <GlassCard>
      <h3 className="text-sm font-semibold text-white mb-3">Tenant Profile</h3>
      <div className="space-y-3">
        {loading && !tenant ? (
          <p className="text-xs text-white/40 font-mono py-4 text-center">loading tenant…</p>
        ) : tenant && form ? (
          <>
            <label className="block">
              <span className="text-xs text-white/40 uppercase tracking-widest font-mono block mb-1">Tenant Name</span>
              <input
                value={form.name}
                onChange={(e) => setForm({ ...form, name: e.target.value })}
                className="w-full rounded-md border border-white/10 bg-white/[0.03] px-3 py-1.5 text-sm text-white focus:border-cyan-neon/50 focus:outline-none"
              />
            </label>
            <label className="block">
              <span className="text-xs text-white/40 uppercase tracking-widest font-mono block mb-1">Owner Email</span>
              <input
                type="email"
                value={form.owner_email}
                onChange={(e) => setForm({ ...form, owner_email: e.target.value })}
                className="w-full rounded-md border border-white/10 bg-white/[0.03] px-3 py-1.5 text-sm text-white focus:border-cyan-neon/50 focus:outline-none"
              />
            </label>
            {/* Read-only metadata. Slug is immutable by design (cross-service
                key); tier + status flow through dedicated billing endpoints. */}
            <div className="pt-2 space-y-2">
              <ReadRow label="Tenant ID" value={tenantIdOf(tenant)} mono />
              <ReadRow label="Slug"      value={tenant.slug}        mono />
              <ReadRow label="Plan"      value={tenant.subscription_tier} />
              <ReadRow label="Status"    value={tenant.status}            />
              <ReadRow label="Active"    value={tenant.is_active ? "yes" : "no"} />
              <ReadRow label="Created"   value={new Date(tenant.created_at).toLocaleDateString()} />
            </div>
            {error && <p className="text-xs text-red-signal font-mono">{error}</p>}
            <div className="flex items-center justify-end gap-2 pt-2">
              {saved && <span className="text-xs text-green-signal font-mono">✓ Saved</span>}
              <button
                onClick={handleSave}
                disabled={saving}
                className="px-3 py-1.5 text-xs font-medium text-green-signal border border-green-signal/30 bg-green-signal/10 rounded-lg hover:border-green-signal/60 transition-colors disabled:opacity-40"
              >
                {saving ? "Saving…" : "Save Changes"}
              </button>
            </div>
          </>
        ) : (
          <p className="text-xs text-red-signal font-mono">{error ?? "Tenant unavailable"}</p>
        )}
      </div>
    </GlassCard>
  );
}

function ReadRow({ label, value, mono = false }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className="flex justify-between items-center py-1.5 border-b border-white/[0.06]">
      <span className="text-xs text-white/40 uppercase tracking-widest font-mono">{label}</span>
      <span className={`text-sm text-white ${mono ? "font-mono text-white/70" : ""} truncate max-w-[220px]`}>{value}</span>
    </div>
  );
}

// ── Webhooks tab — live from /v1/webhooks (CRUD on webhooks service) ────────

interface WebhookRow {
  id:                 string;
  url:                string;
  events:             string[];
  status:             string;
  description?:       string | null;
  success_count:      number;
  fail_count:         number;
  last_delivery_at?:  string | null;
  last_status_code?:  number | null;
  created_at:         string;
  updated_at:         string;
}

const KNOWN_EVENT_TYPES = [
  "*",
  "shipment.created",
  "shipment.confirmed",
  "shipment.cancelled",
  "driver.assigned",
  "pickup.completed",
  "delivery.completed",
  "delivery.failed",
  "invoice.finalized",
  "cod.remittance_ready",
];

function WebhooksTab() {
  const [webhooks, setWebhooks] = useState<WebhookRow[]>([]);
  const [loading,  setLoading]  = useState(true);
  const [error,    setError]    = useState<string | null>(null);
  const [busyId,   setBusyId]   = useState<string | null>(null);

  // New-webhook modal state — opens on +Add and on success surfaces the
  // one-time signing secret.
  const [showCreate, setShowCreate]   = useState(false);
  const [newUrl,     setNewUrl]       = useState("");
  const [newEvents,  setNewEvents]    = useState<string[]>([]);
  const [newDesc,    setNewDesc]      = useState("");
  const [creating,   setCreating]     = useState(false);
  const [revealedSecret, setRevealedSecret] = useState<string | null>(null);

  const load = useCallback(async () => {
    setError(null);
    try {
      const res = await authFetch(`${API_BASE}/v1/webhooks`);
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const json = await res.json();
      setWebhooks(json.data ?? []);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to load webhooks");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { load(); }, [load]);

  function toggleEvent(ev: string) {
    setNewEvents((prev) => prev.includes(ev) ? prev.filter((x) => x !== ev) : [...prev, ev]);
  }

  async function handleCreate() {
    if (!newUrl.trim() || newEvents.length === 0) return;
    setCreating(true);
    setError(null);
    try {
      const res = await authFetch(`${API_BASE}/v1/webhooks`, {
        method: "POST",
        body: JSON.stringify({
          url:         newUrl.trim(),
          events:      newEvents,
          description: newDesc.trim() || undefined,
        }),
      });
      if (!res.ok) {
        const j = await res.json().catch(() => ({}));
        throw new Error(j.error?.message ?? `HTTP ${res.status}`);
      }
      const j = await res.json();
      setRevealedSecret(j.secret);
      setNewUrl(""); setNewEvents([]); setNewDesc("");
      await load();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Create failed");
    } finally {
      setCreating(false);
    }
  }

  async function handleToggleStatus(w: WebhookRow) {
    setBusyId(w.id);
    try {
      const next = w.status === "active" ? "disabled" : "active";
      const res = await authFetch(`${API_BASE}/v1/webhooks/${w.id}`, {
        method: "PUT",
        body: JSON.stringify({ status: next }),
      });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      await load();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Toggle failed");
    } finally {
      setBusyId(null);
    }
  }

  async function handleDelete(w: WebhookRow) {
    if (!confirm(`Delete webhook to ${w.url}?`)) return;
    setBusyId(w.id);
    try {
      const res = await authFetch(`${API_BASE}/v1/webhooks/${w.id}`, { method: "DELETE" });
      if (!res.ok && res.status !== 204) throw new Error(`HTTP ${res.status}`);
      await load();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Delete failed");
    } finally {
      setBusyId(null);
    }
  }

  return (
    <motion.div variants={variants.fadeInUp} className="space-y-4">
      <div className="flex justify-between items-center gap-3">
        <p className="text-sm text-white/40">
          Webhooks deliver real-time platform events to your systems.
          Each request is signed with HMAC-SHA256 — verify the
          <span className="font-mono text-cyan-neon mx-1">x-logisticos-signature</span>
          header against your stored secret.
        </p>
        <button
          onClick={() => setShowCreate(true)}
          className="px-4 py-2 text-sm font-medium text-[#050810] bg-[#00FF88] rounded-lg hover:bg-[#00FF88]/90 transition-colors"
        >
          + Add Webhook
        </button>
      </div>

      {error && <p className="text-xs text-red-signal font-mono">{error}</p>}

      <div className="space-y-3">
        {loading && webhooks.length === 0 ? (
          <p className="text-xs text-white/40 font-mono py-4 text-center">loading webhooks…</p>
        ) : webhooks.length === 0 ? (
          <p className="text-xs text-white/40 font-mono py-4 text-center">
            No webhooks yet. Tap + Add Webhook to subscribe to platform events.
          </p>
        ) : webhooks.map((wh) => {
          const total = wh.success_count + wh.fail_count;
          const rate  = total > 0 ? (wh.success_count / total) * 100 : null;
          return (
            <GlassCard key={wh.id}>
              <div className="flex items-start justify-between gap-4">
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-3 mb-2">
                    <NeonBadge variant={wh.status === "active" ? "green" : "red"}>{wh.status}</NeonBadge>
                    <span className="font-mono text-sm text-white truncate">{wh.url}</span>
                  </div>
                  <div className="flex flex-wrap gap-1 mb-2">
                    {wh.events.map((e) => (
                      <span key={e} className="text-[10px] px-2 py-0.5 rounded-full bg-[#00E5FF]/10 text-[#00E5FF] border border-[#00E5FF]/20 font-mono">{e}</span>
                    ))}
                  </div>
                  <div className="flex gap-6 text-xs text-white/40">
                    <span>Last delivery: {wh.last_delivery_at ? new Date(wh.last_delivery_at).toLocaleString() : "never"}</span>
                    <span>
                      Success rate:{" "}
                      <span className={rate === null ? "text-white/30" : rate > 95 ? "text-[#00FF88]" : "text-[#FFAB00]"}>
                        {rate === null ? "—" : `${rate.toFixed(1)}% (${wh.success_count}/${total})`}
                      </span>
                    </span>
                    {wh.last_status_code != null && (
                      <span>Last HTTP: <span className="font-mono">{wh.last_status_code}</span></span>
                    )}
                  </div>
                </div>
                <div className="flex gap-3 shrink-0">
                  <button
                    onClick={() => handleToggleStatus(wh)}
                    disabled={busyId === wh.id}
                    className="text-xs text-[#FFAB00] hover:text-[#FFAB00]/70 disabled:opacity-40"
                  >
                    {wh.status === "active" ? "Disable" : "Enable"}
                  </button>
                  <button
                    onClick={() => handleDelete(wh)}
                    disabled={busyId === wh.id}
                    className="text-xs text-[#FF3B5C] hover:text-[#FF3B5C]/70 disabled:opacity-40"
                  >
                    Delete
                  </button>
                </div>
              </div>
            </GlassCard>
          );
        })}
      </div>

      {/* Create modal — minimal: URL + event chips + optional description.
          Server returns the signing secret exactly once on success. */}
      {showCreate && (
        <div className="fixed inset-0 bg-canvas/80 backdrop-blur-sm flex items-center justify-center z-50">
          <div className="bg-canvas border border-white/10 rounded-xl p-6 w-full max-w-lg space-y-4">
            <div className="flex items-center justify-between">
              <h3 className="text-lg font-bold text-white">Add Webhook</h3>
              <button
                onClick={() => { setShowCreate(false); setRevealedSecret(null); }}
                className="text-white/40 hover:text-white"
              >
                ✕
              </button>
            </div>

            {revealedSecret ? (
              <>
                <div className="rounded-md border border-amber-signal/30 bg-amber-signal/5 p-3">
                  <p className="text-xs text-amber-signal font-mono">
                    Save this signing secret now — you won&apos;t see it again.
                    Use it to verify the
                    <span className="text-cyan-neon mx-1">x-logisticos-signature</span>
                    header on every delivery.
                  </p>
                </div>
                <div className="rounded-md bg-white/[0.03] border border-white/10 p-3 break-all font-mono text-xs text-white">
                  {revealedSecret}
                </div>
                <div className="flex justify-end gap-2">
                  <button
                    onClick={() => navigator.clipboard.writeText(revealedSecret)}
                    className="text-xs text-cyan-neon hover:text-cyan-neon/70"
                  >
                    Copy
                  </button>
                  <button
                    onClick={() => { setShowCreate(false); setRevealedSecret(null); }}
                    className="px-4 py-2 text-sm font-medium text-[#050810] bg-[#00FF88] rounded-lg hover:bg-[#00FF88]/90 transition-colors"
                  >
                    Done
                  </button>
                </div>
              </>
            ) : (
              <>
                <label className="block">
                  <span className="text-xs text-white/40 uppercase tracking-widest font-mono block mb-1">URL</span>
                  <input
                    type="url"
                    value={newUrl}
                    onChange={(e) => setNewUrl(e.target.value)}
                    placeholder="https://your-app.example.com/webhooks/logisticos"
                    className="w-full rounded-md border border-white/10 bg-white/[0.03] px-3 py-1.5 text-sm text-white font-mono focus:border-cyan-neon/50 focus:outline-none"
                  />
                </label>

                <div>
                  <span className="text-xs text-white/40 uppercase tracking-widest font-mono block mb-1">Events</span>
                  <div className="flex flex-wrap gap-1.5">
                    {KNOWN_EVENT_TYPES.map((ev) => (
                      <button
                        key={ev}
                        onClick={() => toggleEvent(ev)}
                        className={`text-[10px] px-2 py-0.5 rounded-full font-mono transition-colors ${
                          newEvents.includes(ev)
                            ? "bg-cyan-neon/20 text-cyan-neon border border-cyan-neon/40"
                            : "bg-white/[0.03] text-white/40 border border-white/10"
                        }`}
                      >
                        {ev}
                      </button>
                    ))}
                  </div>
                  <p className="text-2xs font-mono text-white/30 mt-1">
                    {newEvents.length === 0 ? "Select at least one." : `${newEvents.length} subscribed`}
                  </p>
                </div>

                <label className="block">
                  <span className="text-xs text-white/40 uppercase tracking-widest font-mono block mb-1">Description (optional)</span>
                  <input
                    type="text"
                    value={newDesc}
                    onChange={(e) => setNewDesc(e.target.value)}
                    placeholder="e.g. Production billing system"
                    className="w-full rounded-md border border-white/10 bg-white/[0.03] px-3 py-1.5 text-sm text-white focus:border-cyan-neon/50 focus:outline-none"
                  />
                </label>

                <div className="flex justify-end gap-2">
                  <button
                    onClick={() => setShowCreate(false)}
                    disabled={creating}
                    className="px-3 py-1.5 text-xs text-white/60 hover:text-white"
                  >
                    Cancel
                  </button>
                  <button
                    onClick={handleCreate}
                    disabled={creating || !newUrl.trim() || newEvents.length === 0}
                    className="px-4 py-2 text-sm font-medium text-[#050810] bg-[#00FF88] rounded-lg hover:bg-[#00FF88]/90 transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
                  >
                    {creating ? "Creating…" : "Create webhook"}
                  </button>
                </div>
              </>
            )}
          </div>
        </div>
      )}
    </motion.div>
  );
}

// ── Audit Log tab — live from identity /v1/audit-log ─────────────────────────

function AuditLogTab() {
  const [entries,  setEntries]  = useState<AuditEntry[]>([]);
  const [loading,  setLoading]  = useState(true);
  const [error,    setError]    = useState<string | null>(null);

  const load = useCallback(async () => {
    setError(null);
    try {
      const res = await authFetch(`${API_BASE}/v1/audit-log`);
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const json = await res.json();
      setEntries(Array.isArray(json.data) ? json.data : []);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to load audit log");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { load(); }, [load]);

  return (
    <motion.div variants={variants.fadeInUp} className="space-y-4">
      <div className="flex justify-between items-center">
        <p className="text-sm text-white/40">All mutations — actor, action, resource. Immutable. Retained 90 days.</p>
        <div className="flex items-center gap-2">
          {error && <span className="text-2xs font-mono text-amber-signal">{error}</span>}
          <button
            onClick={() => downloadAuditCsv(entries)}
            disabled={entries.length === 0}
            className="px-4 py-2 text-sm font-medium text-white/70 border border-white/[0.08] rounded-lg hover:bg-white/[0.05] transition-colors disabled:opacity-40"
          >
            Export CSV
          </button>
        </div>
      </div>
      <GlassCard padding="none">
        <table className="w-full text-sm">
          <thead>
            <tr className="border-b border-white/[0.08]">
              {["Timestamp", "Actor", "Action", "Resource"].map((h) => (
                <th key={h} className="text-left px-4 py-3 text-xs text-white/30 uppercase tracking-widest font-mono">{h}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {loading ? (
              <tr><td colSpan={4} className="px-4 py-8 text-center text-xs text-white/30 font-mono">loading audit log…</td></tr>
            ) : entries.length === 0 ? (
              <tr><td colSpan={4} className="px-4 py-8 text-center text-xs text-white/30 font-mono">No audit events yet. Actions like API key creation will appear here.</td></tr>
            ) : entries.map((entry) => (
              <tr key={entry.id} className="border-b border-white/[0.04] hover:bg-white/[0.02]">
                <td className="px-4 py-3 font-mono text-xs text-white/40">
                  {new Date(entry.created_at).toLocaleString(undefined, { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit", second: "2-digit" })}
                </td>
                <td className="px-4 py-3 text-xs text-[#00E5FF] font-mono">{entry.actor_email ?? entry.actor_id ?? "system"}</td>
                <td className="px-4 py-3">
                  <NeonBadge variant={(ACTION_COLOR[entry.action] ?? "cyan") as Parameters<typeof NeonBadge>[0]["variant"]}>
                    {entry.action}
                  </NeonBadge>
                </td>
                <td className="px-4 py-3 text-xs text-white/60">{entry.resource}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </GlassCard>
    </motion.div>
  );
}

// ── Audit log CSV export ─────────────────────────────────────────────────────

function downloadAuditCsv(entries: readonly AuditEntry[]) {
  const header = ["timestamp", "actor", "action", "resource"];
  const rows = entries.map((e) => [
    new Date(e.created_at).toISOString(),
    e.actor_email ?? e.actor_id ?? "system",
    e.action,
    e.resource,
  ]);
  const csv = [header, ...rows]
    .map((row) => row.map((cell) => `"${String(cell).replace(/"/g, '""')}"`).join(","))
    .join("\n");
  const blob = new Blob([csv], { type: "text/csv;charset=utf-8" });
  const url  = URL.createObjectURL(blob);
  const a    = document.createElement("a");
  a.href     = url;
  a.download = `audit-log-${new Date().toISOString().slice(0, 10)}.csv`;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  URL.revokeObjectURL(url);
}
