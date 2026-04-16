"use client";
/**
 * Admin Portal — Settings
 * Tenant configuration, API keys, webhook endpoints, role management, audit log.
 */
import { useState } from "react";
import { motion } from "framer-motion";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { variants } from "@/lib/design-system/tokens";

const TABS = ["General", "API Keys", "Webhooks", "Roles & Permissions", "Audit Log"] as const;
type Tab = (typeof TABS)[number];

const API_KEYS = [
  { id: "key_1", name: "Production API Key",  prefix: "lsk_prod_****", created: "2024-01-15", last_used: "2 minutes ago",  scopes: ["shipments:write", "dispatch:read", "webhooks:manage"] },
  { id: "key_2", name: "Staging API Key",     prefix: "lsk_stg_****",  created: "2024-02-01", last_used: "1 hour ago",     scopes: ["shipments:write", "dispatch:read"] },
  { id: "key_3", name: "Analytics Read-Only", prefix: "lsk_ro_****",   created: "2024-02-20", last_used: "3 days ago",     scopes: ["analytics:read"] },
];

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

      {/* API Keys */}
      {activeTab === "API Keys" && (
        <motion.div variants={variants.fadeInUp} className="space-y-4">
          <div className="flex justify-between items-center">
            <p className="text-sm text-white/40">API keys grant programmatic access. Rotate regularly. Store in Vault — never in code.</p>
            <button className="px-4 py-2 text-sm font-medium text-[#050810] bg-[#00E5FF] rounded-lg hover:bg-[#00E5FF]/90 transition-colors">
              + Generate Key
            </button>
          </div>
          <GlassCard padding="none">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-white/[0.08]">
                  {["Name", "Key Prefix", "Scopes", "Created", "Last Used", ""].map((h) => (
                    <th key={h} className="text-left px-4 py-3 text-xs text-white/30 uppercase tracking-widest font-mono">{h}</th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {API_KEYS.map((k) => (
                  <tr key={k.id} className="border-b border-white/[0.04] hover:bg-white/[0.02]">
                    <td className="px-4 py-3 text-white font-medium">{k.name}</td>
                    <td className="px-4 py-3 font-mono text-[#00E5FF] text-xs">{k.prefix}</td>
                    <td className="px-4 py-3">
                      <div className="flex flex-wrap gap-1">
                        {k.scopes.map((s) => (
                          <span key={s} className="text-[10px] px-2 py-0.5 rounded-full bg-[#A855F7]/10 text-[#A855F7] border border-[#A855F7]/20 font-mono">{s}</span>
                        ))}
                      </div>
                    </td>
                    <td className="px-4 py-3 text-white/40 text-xs font-mono">{k.created}</td>
                    <td className="px-4 py-3 text-white/40 text-xs">{k.last_used}</td>
                    <td className="px-4 py-3">
                      <div className="flex gap-2">
                        <button className="text-xs text-[#FFAB00] hover:text-[#FFAB00]/70">Rotate</button>
                        <button className="text-xs text-[#FF3B5C] hover:text-[#FF3B5C]/70">Revoke</button>
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </GlassCard>
        </motion.div>
      )}

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
