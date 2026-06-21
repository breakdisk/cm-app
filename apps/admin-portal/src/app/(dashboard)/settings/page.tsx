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
import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import { motion } from "framer-motion";
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
  type Branding, type UpdateBrandingPayload, type PricingFeature,
} from "@/lib/api/identity";
import { useBranding } from "@/lib/branding";
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

const ALL_TABS = ["General", "Branding", "API Keys", "Webhooks", "Roles & Permissions", "Feature Flags", "Audit Log"] as const;
type Tab = (typeof ALL_TABS)[number];

const TAB_PERMISSIONS: Record<Tab, string> = {
  "General":              "users:manage",
  "Branding":             "tenants:manage",
  "API Keys":             "api_keys:manage",
  "Webhooks":             "webhooks:manage",
  "Roles & Permissions":  "users:manage",
  "Feature Flags":        "tenants:manage",
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
  "api_key.created":                "cyan",
  "api_key.revoked":                "red",
  "webhook.created":                "cyan",
  "webhook.disabled":               "amber",
  "role.user_assigned":             "purple",
  "billing.invoice_exported":       "green",
  "shipment.manual_override":       "amber",
  "tenant.updated":                 "cyan",
  "tenant.settings_updated":        "cyan",
  "tenant.tier_updated":            "purple",
  "pricing_feature.tiers_updated":  "amber",
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

      {/* Branding — white-label self-serve (Enterprise-gated). */}
      {effectiveTab === "Branding" && tabs.includes("Branding") && <BrandingTab />}

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
// Live: engagement /v1/templates — counts active templates per channel as the
// "configured" signal. A channel with zero active templates can't dispatch
// anything, so it's the right gating signal in the absence of a dedicated
// per-channel health endpoint. When engagement ships /v1/channels/health
// (delivery rates), swap the rate column to that.

const ENGAGEMENT_URL = process.env.NEXT_PUBLIC_ENGAGEMENT_URL ?? "http://localhost:8003";

interface TemplateRow {
  id:         string;
  channel:    string;   // "WhatsApp" | "Sms" | "Email" | "Push"
  is_active:  boolean;
  language:   string;
  template_id: string;
}

const KNOWN_CHANNELS: Array<{ key: string; label: string }> = [
  { key: "WhatsApp", label: "WhatsApp" },
  { key: "Sms",      label: "SMS"      },
  { key: "Email",    label: "Email"    },
  { key: "Push",     label: "Push"     },
];

function NotificationChannelsCard() {
  const [rows, setRows]       = useState<TemplateRow[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError]     = useState<string | null>(null);

  useEffect(() => {
    (async () => {
      try {
        const res = await authFetch(`${ENGAGEMENT_URL}/v1/templates`);
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        const json = await res.json() as { templates?: TemplateRow[] };
        setRows(json.templates ?? []);
      } catch (e) {
        setError(e instanceof Error ? e.message : "Failed to load templates");
      } finally {
        setLoading(false);
      }
    })();
  }, []);

  const byChannel = useMemo(() => {
    const m = new Map<string, { active: number; total: number }>();
    for (const t of rows) {
      const cur = m.get(t.channel) ?? { active: 0, total: 0 };
      cur.total += 1;
      if (t.is_active) cur.active += 1;
      m.set(t.channel, cur);
    }
    return m;
  }, [rows]);

  return (
    <GlassCard>
      <h3 className="text-sm font-semibold text-white mb-3">Notification Channels</h3>
      {error && <p className="text-xs text-red-signal font-mono mb-2">{error}</p>}
      <div className="space-y-3">
        {loading ? (
          <p className="text-xs text-white/40 font-mono py-4 text-center">loading channels…</p>
        ) : (
          KNOWN_CHANNELS.map(({ key, label }) => {
            const stats = byChannel.get(key);
            const enabled = !!stats && stats.active > 0;
            return (
              <div key={key} className="flex items-center justify-between p-3 bg-white/[0.03] rounded-lg border border-white/[0.06]">
                <div className="flex items-center gap-3">
                  <div className={`w-2 h-2 rounded-full ${enabled ? "bg-[#00FF88]" : "bg-white/20"}`} />
                  <span className="text-sm text-white">{label}</span>
                </div>
                <span className="text-xs text-white/40 font-mono">
                  {stats
                    ? `${stats.active} active · ${stats.total} template${stats.total === 1 ? "" : "s"}`
                    : "No templates"}
                </span>
              </div>
            );
          })
        )}
      </div>
      <p className="text-2xs font-mono text-white/30 mt-3">
        Source: engagement <span className="text-[#00E5FF]">/v1/templates</span> ·
        a channel is &quot;enabled&quot; when ≥1 active template exists.
      </p>
    </GlassCard>
  );
}

// ── Feature Flags tab ────────────────────────────────────────────────────────
// Loads the feature matrix from GET /v1/pricing/features (DB-driven, not
// hardcoded). Admins with tenants:manage can toggle which tiers include each
// feature via PUT /v1/pricing/features/:key/tiers. Tier changes remain via
// PUT /v1/tenants/:id/tier (audited).

const TIER_LABELS: Record<TenantTier, string> = {
  starter:    "Starter",
  growth:     "Growth",
  business:   "Business",
  enterprise: "Enterprise",
};

const ALL_TIERS: TenantTier[] = ["starter", "growth", "business", "enterprise"];

const CATEGORY_LABELS: Record<string, string> = {
  logistics:  "Logistics",
  ai:         "AI",
  engagement: "Engagement",
  platform:   "Platform",
};

const CATEGORY_COLOR: Record<string, string> = {
  logistics:  "text-[#00E5FF]",
  ai:         "text-[#A855F7]",
  engagement: "text-[#00FF88]",
  platform:   "text-[#FFAB00]",
};

function FeatureFlagsTab() {
  const { hasPermission } = usePermissions();
  const canManage = hasPermission("tenants:manage");

  const [tenant,       setTenant]      = useState<TenantSnapshot | null>(null);
  const [features,     setFeatures]    = useState<PricingFeature[]>([]);
  const [error,        setError]       = useState<string | null>(null);
  const [loading,      setLoading]     = useState(true);
  const [showUpgrade,  setShowUpgrade] = useState(false);
  const [selectedTier, setSelectedTier] = useState<TenantTier>("starter");
  const [upgrading,    setUpgrading]   = useState(false);
  const [upgradeError, setUpgradeError] = useState<string | null>(null);
  const [upgradeSaved, setUpgradeSaved] = useState(false);
  // Track which feature keys are currently being saved
  const [savingKeys,   setSavingKeys]  = useState<Set<string>>(new Set());
  const [savedKeys,    setSavedKeys]   = useState<Set<string>>(new Set());

  const load = useCallback(async () => {
    setError(null);
    try {
      const [t, f] = await Promise.all([
        identityApi.getTenant(),
        identityApi.listPricingFeatures(),
      ]);
      setTenant(t);
      setFeatures(f);
    } catch (e) {
      const err = e as { message?: string };
      setError(err?.message ?? "Failed to load feature matrix");
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

  async function handleTierToggle(feature: PricingFeature, tier: TenantTier, checked: boolean) {
    const next = checked
      ? [...new Set([...feature.enabled_tiers, tier])]
      : feature.enabled_tiers.filter((t) => t !== tier);

    // Optimistic update
    setFeatures((prev) =>
      prev.map((f) => f.feature_key === feature.feature_key ? { ...f, enabled_tiers: next } : f)
    );

    setSavingKeys((s) => new Set(s).add(feature.feature_key));
    try {
      await identityApi.setFeatureTiers(feature.feature_key, next);
      setSavedKeys((s) => { const n = new Set(s); n.add(feature.feature_key); return n; });
      setTimeout(() => setSavedKeys((s) => { const n = new Set(s); n.delete(feature.feature_key); return n; }), 1500);
    } catch (e) {
      // Revert on failure
      setFeatures((prev) =>
        prev.map((f) => f.feature_key === feature.feature_key ? { ...f, enabled_tiers: feature.enabled_tiers } : f)
      );
      const err = e as { message?: string };
      setError(err?.message ?? `Failed to update ${feature.feature_name}`);
    } finally {
      setSavingKeys((s) => { const n = new Set(s); n.delete(feature.feature_key); return n; });
    }
  }

  const tier = tenant?.subscription_tier ?? null;

  // Group features by category, preserving sort_order within each group
  const byCategory = useMemo(() => {
    const groups: Record<string, PricingFeature[]> = {};
    for (const f of features) {
      (groups[f.feature_category] ??= []).push(f);
    }
    return groups;
  }, [features]);

  const categoryOrder = ["logistics", "ai", "engagement", "platform"];

  return (
    <motion.div variants={variants.fadeInUp} className="space-y-6">
      {/* Header row */}
      <div className="flex items-start justify-between gap-4">
        <div className="space-y-1">
          <p className="text-sm text-white/40">
            Platform feature matrix — controls which capabilities are included in each
            pricing tier. Toggle cells to change tier access; changes are audited and
            take effect on next token refresh.
          </p>
          {canManage && (
            <p className="text-2xs font-mono text-[#A855F7]/70">
              Admin: editing enabled — click tier checkboxes to change access
            </p>
          )}
        </div>
        <div className="flex items-center gap-2 shrink-0">
          {tier && (
            <div className="flex items-center gap-2">
              <span className="text-xs text-white/40">This tenant:</span>
              <NeonBadge variant="purple">
                {loading ? "…" : TIER_LABELS[tier]}
              </NeonBadge>
            </div>
          )}
          {canManage && !loading && (
            <button
              onClick={() => {
                setSelectedTier(tier ?? "starter");
                setUpgradeError(null);
                setShowUpgrade(true);
              }}
              className="px-4 py-2 text-sm font-medium text-[#050810] bg-[#A855F7] rounded-lg hover:bg-[#A855F7]/90 transition-colors"
            >
              Change Plan
            </button>
          )}
        </div>
      </div>

      {error && <p className="text-xs text-red-signal font-mono">{error}</p>}
      {upgradeSaved && <p className="text-xs text-green-signal font-mono">Plan updated</p>}

      {loading ? (
        <GlassCard>
          <p className="text-sm text-white/30 font-mono">Loading feature matrix…</p>
        </GlassCard>
      ) : (
        <div className="space-y-4">
          {categoryOrder
            .filter((cat) => byCategory[cat]?.length)
            .map((cat) => (
              <GlassCard key={cat} padding="none">
                {/* Category header */}
                <div className="flex items-center gap-2 px-5 py-3 border-b border-white/[0.06]">
                  <span className={`text-xs font-mono font-semibold uppercase tracking-widest ${CATEGORY_COLOR[cat] ?? "text-white/60"}`}>
                    {CATEGORY_LABELS[cat] ?? cat}
                  </span>
                  <span className="text-2xs text-white/20 font-mono">
                    ({byCategory[cat].length} features)
                  </span>
                </div>

                {/* Tier matrix header */}
                <div className="grid grid-cols-[1fr_repeat(4,56px)] gap-0 px-5 py-2 border-b border-white/[0.04]">
                  <span className="text-2xs text-white/20 font-mono uppercase tracking-wider">Feature</span>
                  {ALL_TIERS.map((t) => (
                    <span key={t} className={`text-2xs font-mono text-center uppercase tracking-wide ${t === tier ? "text-[#A855F7]" : "text-white/30"}`}>
                      {TIER_LABELS[t].slice(0, 3)}
                    </span>
                  ))}
                </div>

                {/* Feature rows */}
                {byCategory[cat].map((feature, idx) => {
                  const isSaving = savingKeys.has(feature.feature_key);
                  const justSaved = savedKeys.has(feature.feature_key);
                  return (
                    <div
                      key={feature.feature_key}
                      className={`grid grid-cols-[1fr_repeat(4,56px)] gap-0 items-center px-5 py-3 transition-colors ${
                        idx < byCategory[cat].length - 1 ? "border-b border-white/[0.04]" : ""
                      } ${isSaving ? "opacity-60" : ""}`}
                    >
                      {/* Feature name + description */}
                      <div className="flex flex-col min-w-0 pr-4">
                        <div className="flex items-center gap-2">
                          <span className="text-sm text-white/80 font-medium truncate">
                            {feature.feature_name}
                          </span>
                          {feature.is_system && (
                            <span className="text-2xs font-mono text-white/20 shrink-0">core</span>
                          )}
                          {justSaved && (
                            <span className="text-2xs font-mono text-[#00FF88] shrink-0">saved</span>
                          )}
                        </div>
                        {feature.description && (
                          <span className="text-2xs text-white/30 mt-0.5 truncate">{feature.description}</span>
                        )}
                      </div>

                      {/* Tier checkboxes */}
                      {ALL_TIERS.map((t) => {
                        const checked = feature.enabled_tiers.includes(t);
                        return (
                          <div key={t} className="flex items-center justify-center">
                            {canManage ? (
                              <button
                                onClick={() => handleTierToggle(feature, t, !checked)}
                                disabled={isSaving}
                                title={`${checked ? "Remove" : "Add"} ${feature.feature_name} from ${TIER_LABELS[t]}`}
                                className={`w-5 h-5 rounded border transition-all ${
                                  checked
                                    ? "bg-[#00E5FF]/20 border-[#00E5FF]/60 shadow-[0_0_6px_rgba(0,229,255,0.3)]"
                                    : "bg-white/[0.03] border-white/10 hover:border-white/30"
                                } disabled:cursor-not-allowed`}
                              >
                                {checked && (
                                  <span className="block w-full text-center text-[10px] text-[#00E5FF] leading-5">✓</span>
                                )}
                              </button>
                            ) : (
                              <div className={`w-5 h-5 rounded border ${
                                checked
                                  ? "bg-[#00E5FF]/10 border-[#00E5FF]/40"
                                  : "bg-transparent border-white/[0.08]"
                              }`}>
                                {checked && (
                                  <span className="block w-full text-center text-[10px] text-[#00E5FF] leading-5">✓</span>
                                )}
                              </div>
                            )}
                          </div>
                        );
                      })}
                    </div>
                  );
                })}
              </GlassCard>
            ))}
        </div>
      )}

      <p className="text-2xs font-mono text-white/30">
        Matrix: identity <span className="text-[#00E5FF]">GET /v1/pricing/features</span> ·
        edit via <span className="text-[#00E5FF]">PUT /v1/pricing/features/:key/tiers</span> ·
        tier via <span className="text-[#00E5FF]">PUT /v1/tenants/:id/tier</span>
      </p>

      {/* Change Plan modal */}
      {showUpgrade && (
        <div className="fixed inset-0 bg-canvas/80 backdrop-blur-sm flex items-center justify-center z-50">
          <div className="bg-canvas border border-white/10 rounded-xl p-6 w-full max-w-sm space-y-4">
            <div className="flex items-center justify-between">
              <h3 className="text-base font-bold text-white">Change Plan</h3>
              <button onClick={() => setShowUpgrade(false)} className="text-white/40 hover:text-white">✕</button>
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
                  <div className="flex items-center gap-2">
                    <span className="text-2xs font-mono text-white/30">
                      {features.filter((f) => f.enabled_tiers.includes(t)).length} features
                    </span>
                    {tier === t && <span className="text-2xs font-mono text-white/40">current</span>}
                  </div>
                </button>
              ))}
            </div>

            {upgradeError && <p className="text-xs text-red-signal font-mono">{upgradeError}</p>}

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

// ── Branding tab — white-label self-serve (Enterprise-gated) ─────────────────
// Reads identity GET /v1/tenants/me/branding + GET /v1/tenants/me (for tier),
// writes PUT /v1/tenants/me/branding. Non-Enterprise tenants see an upsell.
// Changes apply on next session/app load (near-live), per the chosen model.

const DEFAULT_COLORS = { primary: "#00E5FF", secondary: "#A855F7", accent: "#00FF88" };

function BrandingTab() {
  const { branding: liveBranding } = useBranding();
  const [tenant,   setTenant]   = useState<TenantSnapshot | null>(null);
  const [form,     setForm]     = useState<Branding | null>(null);
  const [loading,  setLoading]  = useState(true);
  const [saving,   setSaving]   = useState(false);
  const [saved,    setSaved]    = useState(false);
  const [error,    setError]    = useState<string | null>(null);

  const load = useCallback(async () => {
    setError(null);
    try {
      const [t, b] = await Promise.all([identityApi.getTenant(), identityApi.getBranding()]);
      setTenant(t);
      setForm(b);
    } catch (e) {
      const err = e as { message?: string };
      setError(err?.message ?? "Failed to load branding");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { load(); }, [load]);

  const isEnterprise = tenant?.subscription_tier === "enterprise";

  function set<K extends keyof Branding>(key: K, value: Branding[K]) {
    setForm((f) => (f ? { ...f, [key]: value } : f));
  }

  function setLegal(key: "terms" | "privacy" | "splash", value: string) {
    setForm((f) => (f ? { ...f, legal_text: { ...(f.legal_text ?? {}), [key]: value } } : f));
  }

  async function handleSave() {
    if (!form) return;
    setSaving(true);
    setError(null);
    try {
      const payload: UpdateBrandingPayload = {
        display_name:    form.display_name,
        app_tagline:     form.app_tagline ?? undefined,
        logo_url:        form.logo_url ?? undefined,
        favicon_url:     form.favicon_url ?? undefined,
        primary_color:   form.primary_color ?? undefined,
        secondary_color: form.secondary_color ?? undefined,
        accent_color:    form.accent_color ?? undefined,
        support_email:   form.support_email ?? undefined,
        support_phone:   form.support_phone ?? undefined,
        legal_text:      form.legal_text,
      };
      const updated = await identityApi.updateBranding(payload);
      setForm(updated);
      setSaved(true);
      setTimeout(() => setSaved(false), 2500);
    } catch (e) {
      const err = e as { message?: string };
      setError(err?.message ?? "Save failed");
    } finally {
      setSaving(false);
    }
  }

  if (loading) {
    return <p className="text-xs text-white/40 font-mono py-6 text-center">loading branding…</p>;
  }

  if (!isEnterprise) {
    return (
      <motion.div variants={variants.fadeInUp}>
        <GlassCard>
          <div className="flex items-start gap-4">
            <div
              className="h-10 w-10 shrink-0 rounded-lg"
              style={{ background: "linear-gradient(135deg, var(--brand-primary), var(--brand-secondary))" }}
            />
            <div className="space-y-2">
              <h3 className="text-sm font-semibold text-white">White-label branding is an Enterprise feature</h3>
              <p className="text-sm text-white/50">
                Upgrade to the Enterprise plan to set your own logo, colours, and legal copy across
                the merchant, admin, partner, and customer portals — plus the driver and customer apps.
              </p>
              <p className="text-2xs font-mono text-white/30">
                Current plan: <span className="text-[#A855F7]">{tenant?.subscription_tier ?? "unknown"}</span> ·
                change it in <span className="text-[#00E5FF]">Settings → Feature Flags</span>.
              </p>
            </div>
          </div>
        </GlassCard>
      </motion.div>
    );
  }

  if (!form) {
    return <p className="text-xs text-red-signal font-mono">{error ?? "Branding unavailable"}</p>;
  }

  const legal = (form.legal_text ?? {}) as Record<string, string>;
  const preview = {
    primary:   form.primary_color   || DEFAULT_COLORS.primary,
    secondary: form.secondary_color || DEFAULT_COLORS.secondary,
    accent:    form.accent_color    || DEFAULT_COLORS.accent,
  };

  return (
    <motion.div variants={variants.fadeInUp} className="grid grid-cols-1 lg:grid-cols-[3fr_2fr] gap-6">
      {/* ── Editor ── */}
      <GlassCard>
        <h3 className="text-sm font-semibold text-white mb-4">Brand Identity</h3>
        <div className="space-y-4">
          <BrandField label="Display Name">
            <input
              value={form.display_name}
              onChange={(e) => set("display_name", e.target.value)}
              maxLength={100}
              className="w-full rounded-md border border-white/10 bg-white/[0.03] px-3 py-1.5 text-sm text-white focus:border-cyan-neon/50 focus:outline-none"
            />
          </BrandField>

          <BrandField label="Tagline">
            <input
              value={form.app_tagline ?? ""}
              onChange={(e) => set("app_tagline", e.target.value)}
              className="w-full rounded-md border border-white/10 bg-white/[0.03] px-3 py-1.5 text-sm text-white focus:border-cyan-neon/50 focus:outline-none"
            />
          </BrandField>

          <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
            <BrandField label="Logo URL">
              <input
                value={form.logo_url ?? ""}
                onChange={(e) => set("logo_url", e.target.value || null)}
                placeholder="https://…/logo.png"
                className="w-full rounded-md border border-white/10 bg-white/[0.03] px-3 py-1.5 text-sm font-mono text-white placeholder-white/25 focus:border-cyan-neon/50 focus:outline-none"
              />
            </BrandField>
            <BrandField label="Favicon URL">
              <input
                value={form.favicon_url ?? ""}
                onChange={(e) => set("favicon_url", e.target.value || null)}
                placeholder="https://…/favicon.ico"
                className="w-full rounded-md border border-white/10 bg-white/[0.03] px-3 py-1.5 text-sm font-mono text-white placeholder-white/25 focus:border-cyan-neon/50 focus:outline-none"
              />
            </BrandField>
          </div>

          <div className="grid grid-cols-3 gap-3">
            <ColorField label="Primary"   value={preview.primary}   onChange={(v) => set("primary_color", v)} />
            <ColorField label="Secondary" value={preview.secondary} onChange={(v) => set("secondary_color", v)} />
            <ColorField label="Accent"    value={preview.accent}    onChange={(v) => set("accent_color", v)} />
          </div>

          <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
            <BrandField label="Support Email">
              <input
                type="email"
                value={form.support_email ?? ""}
                onChange={(e) => set("support_email", e.target.value || null)}
                className="w-full rounded-md border border-white/10 bg-white/[0.03] px-3 py-1.5 text-sm text-white focus:border-cyan-neon/50 focus:outline-none"
              />
            </BrandField>
            <BrandField label="Support Phone">
              <input
                value={form.support_phone ?? ""}
                onChange={(e) => set("support_phone", e.target.value || null)}
                className="w-full rounded-md border border-white/10 bg-white/[0.03] px-3 py-1.5 text-sm text-white focus:border-cyan-neon/50 focus:outline-none"
              />
            </BrandField>
          </div>

          <BrandField label="Splash / Welcome Copy">
            <textarea
              value={legal.splash ?? ""}
              onChange={(e) => setLegal("splash", e.target.value)}
              rows={2}
              className="w-full rounded-md border border-white/10 bg-white/[0.03] px-3 py-1.5 text-sm text-white focus:border-cyan-neon/50 focus:outline-none resize-none"
            />
          </BrandField>

          {error && <p className="text-xs text-red-signal font-mono">{error}</p>}

          <div className="flex items-center justify-end gap-2 pt-1">
            {saved && <span className="text-xs text-green-signal font-mono">✓ Saved — reloads on next sign-in</span>}
            <button
              onClick={handleSave}
              disabled={saving}
              className="px-4 py-2 text-sm font-medium text-[#050810] bg-[#00FF88] rounded-lg hover:bg-[#00FF88]/90 transition-colors disabled:opacity-40"
            >
              {saving ? "Saving…" : "Save Branding"}
            </button>
          </div>
        </div>
      </GlassCard>

      {/* ── Live preview ── */}
      <GlassCard>
        <h3 className="text-sm font-semibold text-white mb-4">Preview</h3>
        <div
          className="rounded-xl border border-white/10 p-4 space-y-4"
          style={{ background: "#050810" }}
        >
          <div className="flex items-center gap-2.5">
            {form.logo_url ? (
              // eslint-disable-next-line @next/next/no-img-element
              <img src={form.logo_url} alt={form.display_name} className="h-8 w-8 rounded-lg object-contain" />
            ) : (
              <div
                className="h-8 w-8 rounded-lg"
                style={{ background: `linear-gradient(135deg, ${preview.primary}, ${preview.secondary})` }}
              />
            )}
            <div>
              <p className="text-sm font-bold text-white">{form.display_name}</p>
              <p className="text-2xs font-mono uppercase tracking-widest text-white/30">
                {form.app_tagline ?? ""}
              </p>
            </div>
          </div>
          <div className="flex gap-2">
            <button className="px-3 py-1.5 rounded-lg text-xs font-semibold" style={{ background: preview.primary, color: "#050810" }}>
              Primary
            </button>
            <button className="px-3 py-1.5 rounded-lg text-xs font-semibold" style={{ background: preview.secondary, color: "#050810" }}>
              Secondary
            </button>
            <button className="px-3 py-1.5 rounded-lg text-xs font-semibold" style={{ background: preview.accent, color: "#050810" }}>
              Accent
            </button>
          </div>
          <div className="flex gap-3">
            {[preview.primary, preview.secondary, preview.accent].map((c) => (
              <div key={c} className="flex-1">
                <div className="h-10 rounded-lg" style={{ background: c, boxShadow: `0 0 18px ${c}55` }} />
                <p className="mt-1 text-2xs font-mono text-white/40 text-center">{c}</p>
              </div>
            ))}
          </div>
        </div>
        <p className="text-2xs font-mono text-white/30 mt-3">
          Currently applied: <span className="text-[#00E5FF]">{liveBranding.display_name}</span> ·
          changes go live on next sign-in / app reload.
        </p>
      </GlassCard>
    </motion.div>
  );
}

function BrandField({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="block">
      <span className="text-xs text-white/40 uppercase tracking-widest font-mono block mb-1">{label}</span>
      {children}
    </label>
  );
}

function ColorField({ label, value, onChange }: { label: string; value: string; onChange: (v: string) => void }) {
  return (
    <label className="block">
      <span className="text-xs text-white/40 uppercase tracking-widest font-mono block mb-1">{label}</span>
      <div className="flex items-center gap-2 rounded-md border border-white/10 bg-white/[0.03] px-2 py-1">
        <input
          type="color"
          value={value}
          onChange={(e) => onChange(e.target.value.toUpperCase())}
          className="h-7 w-7 cursor-pointer rounded bg-transparent border-0"
          aria-label={`${label} color`}
        />
        <input
          value={value}
          onChange={(e) => onChange(e.target.value)}
          className="w-full bg-transparent text-xs font-mono text-white outline-none"
        />
      </div>
    </label>
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
