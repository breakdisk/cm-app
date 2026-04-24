"use client";
/**
 * Merchant Portal — Settings
 * Merchant profile, pickup addresses, notification preferences, API access.
 *
 * Status per tab:
 *   Profile          — display-only; backend has no PUT /v1/users/:id yet
 *   Pickup Addresses — UI-local; no backend `saved_addresses` table yet
 *   Notifications    — placeholder; no backend `notification_prefs` store yet
 *   API Access       — fully wired to identity /v1/api-keys (list + create + revoke)
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

const TABS = ["Profile", "Pickup Addresses", "Notifications", "API Access"] as const;
type Tab = (typeof TABS)[number];

const ADDRESSES = [
  { id: "addr_1", label: "Main Warehouse",  address: "123 Industrial Blvd, Pasig City, Metro Manila 1605", default: true  },
  { id: "addr_2", label: "Cebu Branch",     address: "45 Colon St, Cebu City, Cebu 6000",                 default: false },
  { id: "addr_3", label: "Davao Depot",     address: "Buhangin Rd, Davao City, Davao del Sur 8000",        default: false },
];

export default function MerchantSettingsPage() {
  const [activeTab, setActiveTab] = useState<Tab>("Profile");

  return (
    <motion.div
      variants={variants.staggerContainer}
      initial="hidden"
      animate="visible"
      className="p-6 space-y-6"
    >
      <motion.div variants={variants.fadeInUp}>
        <h1 className="text-2xl font-bold text-white font-space-grotesk">Settings</h1>
        <p className="text-white/40 text-sm mt-1">Manage your merchant account and preferences</p>
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
                  ? "bg-[#00FF88]/10 text-[#00FF88] border border-[#00FF88]/20"
                  : "text-white/40 hover:text-white/70"
              }`}
            >
              {tab}
            </button>
          ))}
        </div>
      </motion.div>

      {/* Profile */}
      {activeTab === "Profile" && (
        <motion.div variants={variants.fadeInUp} className="grid grid-cols-1 lg:grid-cols-2 gap-6">
          <GlassCard title="Business Information">
            <div className="space-y-4">
              {[
                { label: "Business Name",   value: "Acme Trading Corp."          },
                { label: "Merchant ID",     value: "merch-a1b2c3d4", mono: true  },
                { label: "Contact Person",  value: "Juan dela Cruz"               },
                { label: "Email",           value: "ops@acmetrading.ph"           },
                { label: "Phone",           value: "+63 917 123 4567"             },
                { label: "TIN",             value: "123-456-789-000", mono: true  },
              ].map((row) => (
                <div key={row.label} className="flex justify-between items-center py-2 border-b border-white/[0.06]">
                  <span className="text-xs text-white/40 uppercase tracking-widest font-mono">{row.label}</span>
                  <span className={`text-sm text-white font-medium ${row.mono ? "font-mono text-[#00FF88]" : ""}`}>{row.value}</span>
                </div>
              ))}
            </div>
          </GlassCard>

          <GlassCard title="Billing & Plan">
            <div className="space-y-4">
              {[
                { label: "Plan",             value: "Growth",       badge: "green"  },
                { label: "Billing Cycle",    value: "Monthly",      badge: null     },
                { label: "Shipments / mo",   value: "≤ 5,000",      badge: null     },
                { label: "COD Rate",         value: "1.5% + ₱15",   badge: null     },
                { label: "Fragile Surcharge",value: "₱30 / parcel", badge: null     },
                { label: "Status",           value: "Active",       badge: "green"  },
              ].map((row) => (
                <div key={row.label} className="flex justify-between items-center py-2 border-b border-white/[0.06]">
                  <span className="text-xs text-white/40 uppercase tracking-widest font-mono">{row.label}</span>
                  {row.badge ? (
                    <NeonBadge variant={row.badge as any}>{row.value}</NeonBadge>
                  ) : (
                    <span className="text-sm text-white font-medium">{row.value}</span>
                  )}
                </div>
              ))}
            </div>
          </GlassCard>
        </motion.div>
      )}

      {/* Pickup Addresses */}
      {activeTab === "Pickup Addresses" && (
        <motion.div variants={variants.fadeInUp} className="space-y-4">
          <div className="flex justify-end">
            <button className="px-4 py-2 text-sm font-medium text-[#050810] bg-[#00FF88] rounded-lg hover:bg-[#00FF88]/90 transition-colors">
              + Add Address
            </button>
          </div>
          {ADDRESSES.map((addr) => (
            <GlassCard key={addr.id}>
              <div className="flex items-start justify-between gap-4">
                <div>
                  <div className="flex items-center gap-3 mb-1">
                    <span className="text-white font-semibold">{addr.label}</span>
                    {addr.default && <NeonBadge variant="cyan">Default</NeonBadge>}
                  </div>
                  <p className="text-sm text-white/50">{addr.address}</p>
                </div>
                <div className="flex gap-2 shrink-0">
                  {!addr.default && (
                    <button className="text-xs text-[#00E5FF] hover:text-[#00E5FF]/70">Set Default</button>
                  )}
                  <button className="text-xs text-[#A855F7] hover:text-[#A855F7]/70">Edit</button>
                  {!addr.default && (
                    <button className="text-xs text-[#FF3B5C] hover:text-[#FF3B5C]/70">Remove</button>
                  )}
                </div>
              </div>
            </GlassCard>
          ))}
        </motion.div>
      )}

      {/* Notifications — honest placeholder until the engagement service
          gains a per-merchant notification_prefs store. The UI previously
          rendered 21 toggles that did nothing server-side. */}
      {activeTab === "Notifications" && (
        <motion.div variants={variants.fadeInUp}>
          <GlassCard>
            <div className="py-6 text-center space-y-2">
              <p className="text-sm text-white/70 font-semibold">Notification preferences — coming soon</p>
              <p className="text-xs text-white/40 max-w-md mx-auto">
                Per-event channel preferences (WhatsApp, SMS, Email, Push) will land with the next
                engagement-service release. Today the platform sends all shipment-lifecycle
                notifications by default; contact ops to disable specific events.
              </p>
            </div>
          </GlassCard>
        </motion.div>
      )}

      {/* API Access — fully wired */}
      {activeTab === "API Access" && <ApiKeysTab />}
    </motion.div>
  );
}

// ── API Keys tab ──────────────────────────────────────────────────────────────

function ApiKeysTab() {
  const [keys, setKeys]           = useState<ApiKey[]>([]);
  const [loading, setLoading]     = useState(true);
  const [error, setError]         = useState<string | null>(null);
  const [creating, setCreating]   = useState(false);
  const [newName, setNewName]     = useState("");
  const [newScopes, setNewScopes] = useState("shipments:read,shipments:create");
  const [justCreated, setJustCreated] = useState<CreateApiKeyResult | null>(null);
  const [revokingId, setRevokingId] = useState<string | null>(null);

  const load = useCallback(async () => {
    setError(null);
    try {
      const list = await apiKeysApi.list();
      setKeys(list);
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
        scopes: newScopes.split(",").map(s => s.trim()).filter(Boolean),
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
          <p className="text-xs text-red-signal font-mono">{error}</p>
        </GlassCard>
      )}

      {/* One-time display of a newly-created key */}
      {justCreated && (
        <GlassCard title="New API key — copy it now, you won't see it again">
          <div className="space-y-3">
            <div className="flex items-center gap-3 bg-black/50 border border-green-signal/30 rounded-lg p-4">
              <span className="flex-1 font-mono text-green-signal text-sm break-all">{justCreated.raw_key}</span>
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
      <GlassCard title="Create new key">
        <div className="grid grid-cols-1 md:grid-cols-[2fr_3fr_auto] gap-3">
          <input
            value={newName}
            onChange={(e) => setNewName(e.target.value)}
            placeholder="Key name — e.g. Shopify Integration"
            maxLength={100}
            className="rounded-lg border border-white/10 bg-white/[0.03] px-3 py-2 text-sm text-white placeholder-white/25 outline-none focus:border-cyan-neon/40"
          />
          <input
            value={newScopes}
            onChange={(e) => setNewScopes(e.target.value)}
            placeholder="Scopes (comma-separated)"
            className="rounded-lg border border-white/10 bg-white/[0.03] px-3 py-2 text-sm font-mono text-white placeholder-white/25 outline-none focus:border-cyan-neon/40"
          />
          <button
            onClick={handleCreate}
            disabled={creating || !newName.trim()}
            className="rounded-lg bg-gradient-to-r from-cyan-neon to-green-signal px-4 py-2 text-xs font-semibold text-black disabled:opacity-40"
          >
            {creating ? "Creating…" : "Create"}
          </button>
        </div>
      </GlassCard>

      {/* Existing keys list */}
      <GlassCard padding="none">
        <div className="flex items-center justify-between px-5 py-4 border-b border-white/[0.08]">
          <h2 className="font-heading text-sm font-semibold text-white">Your API Keys</h2>
          <span className="text-2xs font-mono text-white/30">
            {loading ? "loading…" : `${keys.length} key${keys.length === 1 ? "" : "s"}`}
          </span>
        </div>
        <div className="grid grid-cols-[2fr_1fr_90px_100px_80px] gap-3 px-5 py-2.5 border-b border-white/[0.08]">
          {["Name", "Prefix", "Status", "Last Used", ""].map((h) => (
            <span key={h} className="text-2xs font-mono text-white/30 uppercase tracking-wider">{h}</span>
          ))}
        </div>
        {!loading && keys.length === 0 ? (
          <div className="px-5 py-10 text-center text-xs text-white/40 font-mono">No API keys yet.</div>
        ) : (
          keys.map((k) => {
            const id = apiKeyIdOf(k);
            const busy = revokingId === id;
            return (
              <div key={id} className="grid grid-cols-[2fr_1fr_90px_100px_80px] gap-3 items-center px-5 py-3 border-b border-white/[0.04]">
                <div>
                  <p className="text-xs font-medium text-white">{k.name}</p>
                  <p className="text-2xs font-mono text-white/30 mt-0.5">
                    {k.scopes.length > 0 ? k.scopes.join(", ") : "no scopes"}
                  </p>
                </div>
                <span className="text-2xs font-mono text-white/50">{k.key_prefix}…</span>
                <NeonBadge variant={k.is_active ? "green" : "muted"} dot>
                  {k.is_active ? "active" : "revoked"}
                </NeonBadge>
                <span className="text-2xs font-mono text-white/40">
                  {k.last_used_at ? new Date(k.last_used_at).toLocaleDateString() : "never"}
                </span>
                <div className="text-right">
                  {k.is_active && (
                    <button
                      onClick={() => handleRevoke(id)}
                      disabled={busy}
                      className="rounded px-2 py-1 text-2xs text-red-signal hover:bg-red-signal/10 disabled:opacity-40 transition-colors"
                    >
                      {busy ? "…" : "Revoke"}
                    </button>
                  )}
                </div>
              </div>
            );
          })
        )}
      </GlassCard>
    </motion.div>
  );
}
