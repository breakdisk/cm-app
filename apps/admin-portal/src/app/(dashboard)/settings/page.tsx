"use client";
/**
 * Admin Portal — Settings
 *
 * LIVE:   API Keys tab → identity /v1/api-keys (list/create/revoke)
 * STATIC: General, Webhooks, Roles & Permissions, Audit Log tabs.
 *         Backend endpoints for those don't exist yet:
 *           - General: no PUT /v1/tenants/:id endpoint
 *           - Webhooks: no webhook management service
 *           - Roles: identity has /v1/users but no role-assignment endpoint
 *           - Audit Log: no audit-log service exposed to admin portal
 *         When those ship, wire each tab analogous to the API Keys pattern.
 */
import { useCallback, useEffect, useState } from "react";
import { motion } from "framer-motion";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { variants } from "@/lib/design-system/tokens";
import {
  apiKeysApi, apiKeyIdOf,
  type ApiKey, type CreateApiKeyResult,
} from "@/lib/api/api-keys";

const TABS = ["General", "API Keys", "Webhooks", "Roles & Permissions", "Audit Log"] as const;
type Tab = (typeof TABS)[number];

const WEBHOOKS = [
  { id: "wh_1", url: "https://merchant.example.com/hooks/logisticos", events: ["shipment.delivered", "shipment.failed"], status: "active",   last_delivery: "1 min ago",  success_rate: 99.2 },
  { id: "wh_2", url: "https://erp.acme.co/api/logistics-events",      events: ["shipment.created", "shipment.picked_up"], status: "active",   last_delivery: "5 min ago",  success_rate: 98.7 },
  { id: "wh_3", url: "https://old.system.internal/callback",           events: ["shipment.delivered"],                    status: "disabled", last_delivery: "2 days ago", success_rate: 71.0 },
];

const ROLES = [
  { name: "Super Admin",     users: 2,  permissions: "Full access — all tenants and settings" },
  { name: "Ops Manager",     users: 8,  permissions: "Dispatch, drivers, hubs, analytics (read/write)" },
  { name: "Dispatcher",      users: 24, permissions: "Dispatch console, driver comms (no settings)" },
  { name: "Finance Analyst", users: 5,  permissions: "Billing, COD, analytics read-only" },
  { name: "Viewer",          users: 12, permissions: "All dashboards read-only" },
];

const AUDIT_LOG = [
  { ts: "2026-03-17 14:32:11", actor: "admin@logisticos.io",   action: "api_key.created",        resource: "Production API Key v2",      ip: "118.177.32.1"  },
  { ts: "2026-03-17 13:15:44", actor: "ops@logisticos.io",     action: "webhook.disabled",        resource: "wh_3 old.system.internal",   ip: "112.200.5.88"  },
  { ts: "2026-03-17 11:00:02", actor: "admin@logisticos.io",   action: "role.user_assigned",      resource: "Dispatcher → jdelacruz",     ip: "118.177.32.1"  },
  { ts: "2026-03-16 18:44:59", actor: "finance@logisticos.io", action: "billing.invoice_exported",resource: "INV-2026-02-0045",           ip: "203.177.91.22" },
  { ts: "2026-03-16 16:20:33", actor: "ops@logisticos.io",     action: "shipment.manual_override",resource: "CM-PH1-S0000001A → delivered",    ip: "112.200.5.88"  },
  { ts: "2026-03-15 09:05:17", actor: "admin@logisticos.io",   action: "tenant.settings_updated", resource: "SLA policy D+1 → D+2",       ip: "118.177.32.1"  },
];

const ACTION_COLOR: Record<string, string> = {
  "api_key.created":          "cyan",
  "webhook.disabled":         "amber",
  "role.user_assigned":       "purple",
  "billing.invoice_exported": "green",
  "shipment.manual_override": "amber",
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
          <GlassCard title="Tenant Profile">
            <div className="space-y-4">
              {[
                { label: "Tenant Name",   value: "LogisticOS Demo Tenant" },
                { label: "Tenant ID",     value: "tenant-a1b2c3d4",      mono: true },
                { label: "Plan",          value: "Enterprise"             },
                { label: "Region",        value: "ap-southeast-1 (PH)"   },
                { label: "SLA Policy",    value: "Standard (D+2)"         },
              ].map((row) => (
                <div key={row.label} className="flex justify-between items-center py-2 border-b border-white/[0.06]">
                  <span className="text-xs text-white/40 uppercase tracking-widest font-mono">{row.label}</span>
                  <span className={`text-sm text-white font-medium ${row.mono ? "font-mono text-[#00E5FF]" : ""}`}>{row.value}</span>
                </div>
              ))}
            </div>
          </GlassCard>

          <GlassCard title="Notification Channels">
            <div className="space-y-3">
              {[
                { ch: "WhatsApp",  enabled: true,  rate: "98.4%"  },
                { ch: "SMS",       enabled: true,  rate: "99.1%"  },
                { ch: "Email",     enabled: true,  rate: "97.8%"  },
                { ch: "Push",      enabled: true,  rate: "94.2%"  },
                { ch: "Viber",     enabled: false, rate: "—"      },
              ].map((row) => (
                <div key={row.ch} className="flex items-center justify-between p-3 bg-white/[0.03] rounded-lg border border-white/[0.06]">
                  <div className="flex items-center gap-3">
                    <div className={`w-2 h-2 rounded-full ${row.enabled ? "bg-[#00FF88]" : "bg-white/20"}`} />
                    <span className="text-sm text-white">{row.ch}</span>
                  </div>
                  <span className="text-xs text-white/40 font-mono">{row.enabled ? `Delivery rate: ${row.rate}` : "Disabled"}</span>
                </div>
              ))}
            </div>
          </GlassCard>

          <GlassCard title="Feature Flags" className="lg:col-span-2">
            <div className="grid grid-cols-2 md:grid-cols-3 gap-3">
              {[
                { flag: "AI Dispatch Agent",          enabled: true  },
                { flag: "AI Support Agent",           enabled: true  },
                { flag: "COD Auto-Reconciliation",    enabled: true  },
                { flag: "Balikbayan Box Service",     enabled: true  },
                { flag: "Same-Day Delivery",          enabled: false },
                { flag: "Real-Time Driver Tracking",  enabled: true  },
                { flag: "Loyalty Program",            enabled: true  },
                { flag: "Dynamic Pricing",            enabled: false },
                { flag: "Enterprise MCP Extension",   enabled: false },
              ].map((f) => (
                <div key={f.flag} className="flex items-center justify-between p-3 bg-white/[0.03] border border-white/[0.06] rounded-lg">
                  <span className="text-xs text-white/70">{f.flag}</span>
                  <NeonBadge variant={f.enabled ? "green" : "red"}>{f.enabled ? "ON" : "OFF"}</NeonBadge>
                </div>
              ))}
            </div>
          </GlassCard>
        </motion.div>
      )}

      {/* API Keys — live */}
      {activeTab === "API Keys" && <ApiKeysTab />}

      {/* Webhooks */}
      {activeTab === "Webhooks" && (
        <motion.div variants={variants.fadeInUp} className="space-y-4">
          <div className="flex justify-between items-center">
            <p className="text-sm text-white/40">Webhooks deliver real-time events to your systems. Signed with HMAC-SHA256.</p>
            <button className="px-4 py-2 text-sm font-medium text-[#050810] bg-[#00FF88] rounded-lg hover:bg-[#00FF88]/90 transition-colors">
              + Add Webhook
            </button>
          </div>
          <div className="space-y-3">
            {WEBHOOKS.map((wh) => (
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
                      <span>Last delivery: {wh.last_delivery}</span>
                      <span>Success rate: <span className={wh.success_rate > 95 ? "text-[#00FF88]" : "text-[#FFAB00]"}>{wh.success_rate}%</span></span>
                    </div>
                  </div>
                  <div className="flex gap-2 shrink-0">
                    <button className="text-xs text-[#A855F7] hover:text-[#A855F7]/70">Edit</button>
                    <button className="text-xs text-[#FFAB00] hover:text-[#FFAB00]/70">Test</button>
                    <button className="text-xs text-[#FF3B5C] hover:text-[#FF3B5C]/70">Delete</button>
                  </div>
                </div>
              </GlassCard>
            ))}
          </div>
        </motion.div>
      )}

      {/* Roles */}
      {activeTab === "Roles & Permissions" && (
        <motion.div variants={variants.fadeInUp} className="space-y-4">
          <GlassCard padding="none">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-white/[0.08]">
                  {["Role", "Users", "Permissions"].map((h) => (
                    <th key={h} className="text-left px-4 py-3 text-xs text-white/30 uppercase tracking-widest font-mono">{h}</th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {ROLES.map((r) => (
                  <tr key={r.name} className="border-b border-white/[0.04] hover:bg-white/[0.02]">
                    <td className="px-4 py-3 text-white font-semibold">{r.name}</td>
                    <td className="px-4 py-3">
                      <span className="px-2 py-0.5 rounded-full bg-[#A855F7]/10 text-[#A855F7] text-xs border border-[#A855F7]/20">{r.users}</span>
                    </td>
                    <td className="px-4 py-3 text-white/50 text-xs">{r.permissions}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </GlassCard>
        </motion.div>
      )}

      {/* Audit Log */}
      {activeTab === "Audit Log" && (
        <motion.div variants={variants.fadeInUp} className="space-y-4">
          <div className="flex justify-between items-center">
            <p className="text-sm text-white/40">All mutations — actor, action, resource, IP. Immutable. Retained 90 days.</p>
            <button className="px-4 py-2 text-sm font-medium text-white/70 border border-white/[0.08] rounded-lg hover:bg-white/[0.05] transition-colors">
              Export CSV
            </button>
          </div>
          <GlassCard padding="none">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-white/[0.08]">
                  {["Timestamp", "Actor", "Action", "Resource", "IP"].map((h) => (
                    <th key={h} className="text-left px-4 py-3 text-xs text-white/30 uppercase tracking-widest font-mono">{h}</th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {AUDIT_LOG.map((entry, i) => (
                  <tr key={i} className="border-b border-white/[0.04] hover:bg-white/[0.02]">
                    <td className="px-4 py-3 font-mono text-xs text-white/40">{entry.ts}</td>
                    <td className="px-4 py-3 text-xs text-[#00E5FF] font-mono">{entry.actor}</td>
                    <td className="px-4 py-3">
                      <NeonBadge variant={ACTION_COLOR[entry.action] as any ?? "cyan"}>
                        {entry.action}
                      </NeonBadge>
                    </td>
                    <td className="px-4 py-3 text-xs text-white/60">{entry.resource}</td>
                    <td className="px-4 py-3 font-mono text-xs text-white/30">{entry.ip}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </GlassCard>
        </motion.div>
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
        <GlassCard padding="sm">
          <p className="text-xs text-[#FF3B5C] font-mono">{error}</p>
        </GlassCard>
      )}

      {justCreated && (
        <GlassCard title="New API key — copy it now, you won't see it again">
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
      <GlassCard title="Generate new API key">
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
