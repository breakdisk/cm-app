"use client";
/**
 * Admin Portal — Settings
 *
 * LIVE:   API Keys → identity /v1/api-keys (list/create/revoke)
 *         Roles & Permissions → identity /v1/users grouped by role
 *         General → identity /v1/tenants/me + PUT /v1/tenants/:id
 *         Webhooks → /v1/webhooks CRUD
 *         Audit Log → identity /v1/audit-log (100 most recent mutations)
 */
import { useCallback, useEffect, useMemo, useState } from "react";
import { motion } from "framer-motion";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { variants } from "@/lib/design-system/tokens";
import {
  apiKeysApi, apiKeyIdOf,
  type ApiKey, type CreateApiKeyResult,
} from "@/lib/api/api-keys";
import { authFetch } from "@/lib/auth/auth-fetch";

const API_BASE = process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:8000";

interface IdentityUser {
  id: string | { 0: string };
  email: string;
  first_name?: string;
  last_name?: string;
  roles: string[];
  is_active?: boolean;
}

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

const TABS = ["General", "API Keys", "Webhooks", "Roles & Permissions", "Audit Log"] as const;
type Tab = (typeof TABS)[number];

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
};

export default function SettingsPage() {
  const [activeTab, setActiveTab] = useState<Tab>("General");

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
        <div className="flex gap-1 bg-white/[0.03] border border-white/[0.08] rounded-xl p-1 w-fit">
          {TABS.map((tab) => (
            <button
              key={tab}
              onClick={() => setActiveTab(tab)}
              className={`px-4 py-2 rounded-lg text-sm font-medium transition-all ${
                activeTab === tab
                  ? "bg-[#00E5FF]/10 text-[#00E5FF] border border-[#00E5FF]/20"
                  : "text-white/40 hover:text-white/70"
              }`}
            >
              {tab}
            </button>
          ))}
        </div>
      </motion.div>

      {/* General */}
      {activeTab === "General" && (
        <motion.div variants={variants.fadeInUp} className="grid grid-cols-1 lg:grid-cols-2 gap-6">
          <TenantProfileCard />

          <NotificationChannelsCard />

          <FeatureFlagsCard />
        </motion.div>
      )}

      {/* API Keys — live */}
      {activeTab === "API Keys" && <ApiKeysTab />}

      {/* Webhooks — backed by /v1/webhooks (CRUD) on the new webhooks
          service. The signing secret is returned exactly once at create
          time; the modal surfaces it for copy-paste. */}
      {activeTab === "Webhooks" && <WebhooksTab />}

      {/* Roles — derived from identity /v1/users grouped by role. */}
      {activeTab === "Roles & Permissions" && <RolesTab />}

      {/* Audit Log — live from identity /v1/audit-log (100 most recent). */}
      {activeTab === "Audit Log" && <AuditLogTab />}
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

// ── Feature Flags card (General tab) ────────────────────────────────────────
// Driven by tenant.subscription_tier (free | starter | business | enterprise).
// The platform doesn't yet ship a dedicated feature-flags service; gating
// today is tier-based at the API layer (see services/identity middleware).
// This panel makes the effective tier-derived flag set visible so ops can
// confirm a tenant has the entitlements they expect.

type TenantTier = "free" | "starter" | "business" | "enterprise";

interface TierFlag {
  flag:    string;
  /** Lowest tier that grants this feature. */
  minTier: TenantTier;
}

const TIER_RANK: Record<TenantTier, number> = {
  free: 0, starter: 1, business: 2, enterprise: 3,
};

const TIER_FLAGS: TierFlag[] = [
  { flag: "AI Dispatch Agent",         minTier: "starter"    },
  { flag: "AI Recovery Agent",         minTier: "business"   },
  { flag: "COD Auto-Reconciliation",   minTier: "starter"    },
  { flag: "Balikbayan Box Service",    minTier: "starter"    },
  { flag: "Same-Day Delivery",         minTier: "business"   },
  { flag: "Real-Time Driver Tracking", minTier: "starter"    },
  { flag: "Loyalty Program",           minTier: "business"   },
  { flag: "Dynamic Pricing",           minTier: "business"   },
  { flag: "Enterprise MCP Extension",  minTier: "enterprise" },
];

function FeatureFlagsCard() {
  const [tier, setTier]       = useState<TenantTier | null>(null);
  const [error, setError]     = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    (async () => {
      try {
        const res = await authFetch(`${API_BASE}/v1/tenants/me`);
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        const json = await res.json();
        const t = json?.data?.subscription_tier;
        setTier((t as TenantTier) ?? "starter");
      } catch (e) {
        setError(e instanceof Error ? e.message : "Failed to load tenant tier");
      } finally {
        setLoading(false);
      }
    })();
  }, []);

  return (
    <GlassCard className="lg:col-span-2">
      <h3 className="text-sm font-semibold text-white mb-3">Feature Flags</h3>
      {error && <p className="text-xs text-red-signal font-mono mb-2">{error}</p>}
      <div className="flex items-center gap-2 mb-3">
        <span className="text-xs text-white/40">Effective tier:</span>
        <NeonBadge variant="purple">
          {loading ? "loading…" : (tier ?? "unknown")}
        </NeonBadge>
      </div>
      <div className="grid grid-cols-2 md:grid-cols-3 gap-3">
        {TIER_FLAGS.map((f) => {
          const enabled = tier !== null && TIER_RANK[tier] >= TIER_RANK[f.minTier];
          return (
            <div key={f.flag} className="flex items-center justify-between p-3 bg-white/[0.03] border border-white/[0.06] rounded-lg">
              <div className="flex flex-col">
                <span className="text-xs text-white/70">{f.flag}</span>
                <span className="text-2xs font-mono text-white/30">requires {f.minTier}+</span>
              </div>
              <NeonBadge variant={enabled ? "green" : "red"}>{enabled ? "ON" : "OFF"}</NeonBadge>
            </div>
          );
        })}
      </div>
      <p className="text-2xs font-mono text-white/30 mt-3">
        Driven by <span className="text-[#00E5FF]">tenant.subscription_tier</span>.
        Tier upgrades flow through the billing service.
      </p>
    </GlassCard>
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
  const [users, setUsers]     = useState<IdentityUser[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError]     = useState<string | null>(null);

  const load = useCallback(async () => {
    setError(null);
    try {
      const res = await authFetch(`${API_BASE}/v1/users`);
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const json = await res.json();
      setUsers(Array.isArray(json.data) ? json.data : []);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to load users");
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
// Backed by GET /v1/tenants/me (read) + PUT /v1/tenants/:id (write).
// Slug + tier + status are intentionally read-only — those have first-class
// endpoints with cross-service side-effects.

interface TenantSnapshot {
  id:                string | { 0: string };
  name:              string;
  slug:              string;
  subscription_tier: string;
  status:            string;
  is_active:         boolean;
  owner_email:       string;
  created_at:        string;
  updated_at:        string;
}

function tenantIdOf(t: TenantSnapshot): string {
  const raw = t.id as unknown;
  if (typeof raw === "string") return raw;
  if (raw && typeof raw === "object" && "0" in raw) return String((raw as { 0: string })[0]);
  return "";
}

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
      const res = await authFetch(`${API_BASE}/v1/tenants/me`);
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const json = await res.json();
      const t: TenantSnapshot = json.data;
      setTenant(t);
      setForm({ name: t.name, owner_email: t.owner_email });
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to load tenant");
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
      const res = await authFetch(`${API_BASE}/v1/tenants/${tenantIdOf(tenant)}`, {
        method: "PUT",
        body: JSON.stringify({ name: form.name, owner_email: form.owner_email }),
      });
      if (!res.ok) {
        const j = await res.json().catch(() => ({}));
        throw new Error(j.error?.message ?? `HTTP ${res.status}`);
      }
      const j = await res.json();
      setTenant(j.data);
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Save failed");
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
