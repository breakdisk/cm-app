"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { motion } from "framer-motion";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { variants } from "@/lib/design-system/tokens";
import {
  apiKeysApi, apiKeyIdOf,
  type ApiKey, type CreateApiKeyResult,
} from "@/lib/api/api-keys";
import { identityApi, type Me, type Tenant } from "@/lib/api/identity";
import { addressesApi, addressLookupApi, type AddressCode, type PickupAddress, type CreatePickupAddressPayload } from "@/lib/api/addresses";

const TABS = ["Profile", "Pickup Addresses", "Notifications", "API Access"] as const;
type Tab = (typeof TABS)[number];

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

      {activeTab === "Profile" && <ProfileTab />}
      {activeTab === "Pickup Addresses" && <PickupAddressesTab />}

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

      {activeTab === "API Access" && <ApiKeysTab />}
    </motion.div>
  );
}

// ── Profile tab ───────────────────────────────────────────────────────────────

function ProfileTab() {
  const [me, setMe]           = useState<Me | null>(null);
  const [tenant, setTenant]   = useState<Tenant | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError]     = useState<string | null>(null);
  const [editing, setEditing] = useState(false);
  const [saving, setSaving]   = useState(false);
  const [form, setForm]       = useState({ first_name: "", last_name: "", phone_number: "" });

  useEffect(() => {
    Promise.all([identityApi.getMe(), identityApi.getTenant()])
      .then(([meData, tenantData]) => {
        setMe(meData);
        setTenant(tenantData);
        setForm({
          first_name:   meData.first_name,
          last_name:    meData.last_name,
          phone_number: meData.phone_number ?? "",
        });
      })
      .catch((e) => setError(e?.message ?? "Failed to load profile"))
      .finally(() => setLoading(false));
  }, []);

  async function handleSave() {
    if (!me) return;
    setSaving(true);
    setError(null);
    try {
      const updated = await identityApi.updateMe({
        first_name:   form.first_name || undefined,
        last_name:    form.last_name  || undefined,
        phone_number: form.phone_number || undefined,
      });
      setMe(updated);
      setEditing(false);
    } catch (e) {
      const err = e as { message?: string };
      setError(err?.message ?? "Save failed");
    } finally {
      setSaving(false);
    }
  }

  if (loading) {
    return (
      <motion.div variants={variants.fadeInUp} className="text-white/40 text-sm font-mono py-8 text-center">
        Loading profile…
      </motion.div>
    );
  }

  return (
    <motion.div variants={variants.fadeInUp} className="grid grid-cols-1 lg:grid-cols-2 gap-6">
      <GlassCard title="Your Profile">
        {error && <p className="text-xs text-red-400 font-mono mb-4">{error}</p>}
        {editing ? (
          <div className="space-y-4">
            {(["first_name", "last_name", "phone_number"] as const).map((field) => (
              <div key={field} className="space-y-1">
                <label className="text-xs text-white/40 uppercase tracking-widest font-mono">
                  {field.replace(/_/g, " ")}
                </label>
                <input
                  value={form[field]}
                  onChange={(e) => setForm((f) => ({ ...f, [field]: e.target.value }))}
                  className="w-full rounded-lg border border-white/10 bg-white/[0.03] px-3 py-2 text-sm text-white placeholder-white/25 outline-none focus:border-cyan-neon/40"
                />
              </div>
            ))}
            <div className="flex gap-3 pt-2">
              <button
                onClick={handleSave}
                disabled={saving}
                className="px-4 py-2 text-sm font-semibold text-black bg-[#00FF88] rounded-lg disabled:opacity-40"
              >
                {saving ? "Saving…" : "Save"}
              </button>
              <button
                onClick={() => { setEditing(false); setError(null); }}
                className="px-4 py-2 text-sm text-white/60 border border-white/10 rounded-lg"
              >
                Cancel
              </button>
            </div>
          </div>
        ) : (
          <div className="space-y-4">
            {[
              { label: "Name",   value: me ? `${me.first_name} ${me.last_name}` : "—" },
              { label: "Email",  value: me?.email ?? "—" },
              { label: "Phone",  value: me?.phone_number ?? "—" },
              { label: "Roles",  value: me?.roles.join(", ") ?? "—" },
              { label: "User ID",value: me?.id ?? "—", mono: true },
            ].map((row) => (
              <div key={row.label} className="flex justify-between items-center py-2 border-b border-white/[0.06]">
                <span className="text-xs text-white/40 uppercase tracking-widest font-mono">{row.label}</span>
                <span className={`text-sm text-white font-medium ${row.mono ? "font-mono text-[#00FF88]" : ""}`}>
                  {row.value}
                </span>
              </div>
            ))}
            <div className="pt-2">
              <button
                onClick={() => setEditing(true)}
                className="px-4 py-2 text-sm font-medium text-[#00E5FF] border border-[#00E5FF]/20 rounded-lg hover:bg-[#00E5FF]/5"
              >
                Edit Profile
              </button>
            </div>
          </div>
        )}
      </GlassCard>

      <GlassCard title="Tenant & Plan">
        <div className="space-y-4">
          {[
            { label: "Business Name",     value: tenant?.name ?? "—" },
            { label: "Tenant Slug",       value: tenant?.slug ?? "—", mono: true },
            { label: "Plan",              value: tenant?.subscription_tier ?? "—", badge: "green" },
            { label: "Status",            value: tenant?.is_active ? "Active" : "Suspended", badge: tenant?.is_active ? "green" : "muted" },
            { label: "Tenant ID",         value: tenant?.id ?? "—", mono: true },
          ].map((row) => (
            <div key={row.label} className="flex justify-between items-center py-2 border-b border-white/[0.06]">
              <span className="text-xs text-white/40 uppercase tracking-widest font-mono">{row.label}</span>
              {row.badge ? (
                <NeonBadge variant={row.badge as "green" | "muted"}>{row.value}</NeonBadge>
              ) : (
                <span className={`text-sm text-white font-medium ${row.mono ? "font-mono text-[#00FF88] text-xs" : ""}`}>
                  {row.value}
                </span>
              )}
            </div>
          ))}
        </div>
      </GlassCard>
    </motion.div>
  );
}

// ── Pickup Addresses tab ──────────────────────────────────────────────────────

function PickupAddressesTab() {
  const [addresses, setAddresses] = useState<PickupAddress[]>([]);
  const [loading, setLoading]     = useState(true);
  const [error, setError]         = useState<string | null>(null);
  const [adding, setAdding]       = useState(false);
  const [saving, setSaving]       = useState(false);
  const [form, setForm]           = useState<CreatePickupAddressPayload>({ label: "", address: "", city: "", province: "", postal_code: "", country: "PH" });
  const [busyId, setBusyId]       = useState<string | null>(null);
  const [citySuggestions, setCitySuggestions] = useState<AddressCode[]>([]);
  const cityTimerRef   = useRef<ReturnType<typeof setTimeout> | null>(null);
  const postalTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  function handlePostalChange(value: string) {
    setForm((f) => ({ ...f, postal_code: value }));
    if (postalTimerRef.current) clearTimeout(postalTimerRef.current);
    if (value.length >= 3) {
      const country = form.country || "PH";
      postalTimerRef.current = setTimeout(async () => {
        const hits = await addressLookupApi.byPostalCode(country, value);
        if (hits[0]) {
          setForm((f) => ({ ...f, city: hits[0].city, province: hits[0].state_province ?? f.province ?? "" }));
          setCitySuggestions([]);
        }
      }, 500);
    }
  }

  function handleCityChange(value: string) {
    setForm((f) => ({ ...f, city: value }));
    if (cityTimerRef.current) clearTimeout(cityTimerRef.current);
    if (value.length >= 2) {
      cityTimerRef.current = setTimeout(async () => {
        setCitySuggestions(await addressLookupApi.byCity(form.country || "PH", value));
      }, 400);
    } else {
      setCitySuggestions([]);
    }
  }

  const load = useCallback(async () => {
    setError(null);
    try {
      setAddresses(await addressesApi.list());
    } catch (e) {
      const err = e as { message?: string };
      setError(err?.message ?? "Failed to load addresses");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { load(); }, [load]);

  async function handleCreate() {
    if (!form.label.trim() || !form.address.trim()) return;
    setSaving(true);
    setError(null);
    try {
      await addressesApi.create(form);
      setForm({ label: "", address: "", city: "", province: "", postal_code: "", country: "PH" });
      setAdding(false);
      await load();
    } catch (e) {
      const err = e as { message?: string };
      setError(err?.message ?? "Create failed");
    } finally {
      setSaving(false);
    }
  }

  async function handleSetDefault(id: string) {
    setBusyId(id);
    try {
      await addressesApi.setDefault(id);
      await load();
    } catch (e) {
      const err = e as { message?: string };
      setError(err?.message ?? "Failed to set default");
    } finally {
      setBusyId(null);
    }
  }

  async function handleRemove(id: string) {
    setBusyId(id);
    try {
      await addressesApi.remove(id);
      await load();
    } catch (e) {
      const err = e as { message?: string };
      setError(err?.message ?? "Failed to remove");
    } finally {
      setBusyId(null);
    }
  }

  return (
    <motion.div variants={variants.fadeInUp} className="space-y-4">
      {error && (
        <GlassCard>
          <p className="text-xs text-red-400 font-mono">{error}</p>
        </GlassCard>
      )}

      <div className="flex justify-end">
        <button
          onClick={() => setAdding((v) => !v)}
          className="px-4 py-2 text-sm font-medium text-[#050810] bg-[#00FF88] rounded-lg hover:bg-[#00FF88]/90 transition-colors"
        >
          {adding ? "Cancel" : "+ Add Address"}
        </button>
      </div>

      {adding && (
        <GlassCard title="New pickup address">
          <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
            {([
              ["label",       "Label",       "Main Warehouse"],
              ["address",     "Street Address", "123 Industrial Blvd"],
              ["city",        "City",        "Pasig City"],
              ["province",    "Province",    "Metro Manila"],
              ["postal_code", "Postal Code", "1600"],
              ["country",     "Country",     "PH"],
            ] as [keyof CreatePickupAddressPayload, string, string][]).map(([field, label, placeholder]) => (
              <div key={field} className="space-y-1">
                <label className="text-xs text-white/40 uppercase tracking-widest font-mono">{label}</label>
                <input
                  value={(form[field] as string) ?? ""}
                  onChange={(e) => {
                    if (field === "city")        handleCityChange(e.target.value);
                    else if (field === "postal_code") handlePostalChange(e.target.value);
                    else setForm((f) => ({ ...f, [field]: e.target.value }));
                  }}
                  placeholder={placeholder}
                  className="w-full rounded-lg border border-white/10 bg-white/[0.03] px-3 py-2 text-sm text-white placeholder-white/25 outline-none focus:border-cyan-neon/40"
                />
                {field === "city" && citySuggestions.length > 0 && (
                  <div className="rounded-lg border border-white/10 bg-[#050810]/95 divide-y divide-white/[0.06] overflow-hidden shadow-lg">
                    {citySuggestions.slice(0, 5).map((s) => (
                      <button key={`${s.postal_code}-${s.city}`} type="button"
                        onClick={() => {
                          setForm((f) => ({ ...f, city: s.city, province: s.state_province ?? f.province ?? "", postal_code: s.postal_code }));
                          setCitySuggestions([]);
                        }}
                        className="w-full flex items-center gap-3 px-3 py-2 text-left hover:bg-white/[0.04] transition-colors">
                        <span className="flex-1 font-mono text-sm text-white/80">{s.city}</span>
                        {s.state_province && <span className="font-mono text-xs text-white/30">{s.state_province}</span>}
                        <span className="font-mono text-xs text-cyan-neon/60">{s.postal_code}</span>
                      </button>
                    ))}
                  </div>
                )}
              </div>
            ))}
          </div>
          <div className="flex gap-3 mt-4">
            <button
              onClick={handleCreate}
              disabled={saving || !form.label.trim() || !form.address.trim()}
              className="px-4 py-2 text-sm font-semibold text-black bg-[#00FF88] rounded-lg disabled:opacity-40"
            >
              {saving ? "Saving…" : "Save Address"}
            </button>
          </div>
        </GlassCard>
      )}

      {loading ? (
        <div className="text-white/40 text-sm font-mono py-8 text-center">Loading addresses…</div>
      ) : addresses.length === 0 ? (
        <GlassCard>
          <div className="py-8 text-center text-xs text-white/40 font-mono">No pickup addresses yet. Add one above.</div>
        </GlassCard>
      ) : (
        addresses.map((addr) => {
          const busy = busyId === addr.id;
          return (
            <GlassCard key={addr.id}>
              <div className="flex items-start justify-between gap-4">
                <div>
                  <div className="flex items-center gap-3 mb-1">
                    <span className="text-white font-semibold">{addr.label}</span>
                    {addr.is_default && <NeonBadge variant="cyan">Default</NeonBadge>}
                  </div>
                  <p className="text-sm text-white/50">
                    {[addr.address, addr.city, addr.province, addr.postal_code, addr.country]
                      .filter(Boolean)
                      .join(", ")}
                  </p>
                </div>
                <div className="flex gap-2 shrink-0 items-center">
                  {!addr.is_default && (
                    <button
                      onClick={() => handleSetDefault(addr.id)}
                      disabled={busy}
                      className="text-xs text-[#00E5FF] hover:text-[#00E5FF]/70 disabled:opacity-40"
                    >
                      {busy ? "…" : "Set Default"}
                    </button>
                  )}
                  {!addr.is_default && (
                    <button
                      onClick={() => handleRemove(addr.id)}
                      disabled={busy}
                      className="text-xs text-[#FF3B5C] hover:text-[#FF3B5C]/70 disabled:opacity-40"
                    >
                      {busy ? "…" : "Remove"}
                    </button>
                  )}
                </div>
              </div>
            </GlassCard>
          );
        })
      )}
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
        <GlassCard>
          <p className="text-xs text-red-signal font-mono">{error}</p>
        </GlassCard>
      )}

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
              I&apos;ve saved it
            </button>
          </div>
        </GlassCard>
      )}

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
