"use client";
/**
 * Partner Portal — Rate Cards + Rate Shop
 * Surfaces the carrier service for the acting partner:
 *   GET /v1/carriers/:id        → partner's own rate_cards
 *   PUT /v1/carriers/:id        → save edited rate_cards array (CARRIERS_MANAGE)
 *   GET /v1/carriers/rate-shop  → calculator showing all carriers' quotes
 *
 * Edit-mode invariants enforced client-side: service_type unique per
 * carrier (rate engine picks the first match), base_rate >= 0, per_kg >= 0,
 * max_weight_kg > 0. Server-side validation is light today; treat the
 * client as the canonical guard until carrier service grows a domain check.
 */
import { useCallback, useEffect, useMemo, useState } from "react";
import { motion } from "framer-motion";
import { variants } from "@/lib/design-system/tokens";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { GitBranch, RefreshCw, Calculator, Download, Pencil, Plus, Trash2, Save, X } from "lucide-react";
import {
  carriersApi, fmtPhp,
  type Carrier, type RateCard, type RateQuote,
} from "@/lib/api/carriers";
import { getCurrentPartnerId } from "@/lib/api/partner-identity";

const SERVICE_TYPES = ["standard", "next_day", "same_day"] as const;
type ServiceType = typeof SERVICE_TYPES[number];

function emptyRateCard(): RateCard {
  return { service_type: "standard", base_rate_cents: 0, per_kg_cents: 0, max_weight_kg: 25, coverage_zones: [] };
}

export default function RateCardsPage() {
  const [carrier, setCarrier] = useState<Carrier | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError]     = useState<string | null>(null);

  // Edit-mode state. `editing` carries the local working copy; the page
  // commits to `carrier.rate_cards` only after a successful save. Cancel
  // discards the working copy and snaps back to the server state.
  const [editing, setEditing] = useState<RateCard[] | null>(null);
  const [saving, setSaving]   = useState(false);
  const [saved,  setSaved]    = useState(false);

  const load = useCallback(async () => {
    setError(null);
    try {
      const id = getCurrentPartnerId();
      const c = await carriersApi.get(id);
      setCarrier(c);
    } catch (e) {
      const err = e as { message?: string };
      setError(err?.message ?? "Failed to load rate card");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { load(); }, [load]);

  const rateCards: RateCard[] = editing ?? carrier?.rate_cards ?? [];

  // Detect duplicate service types in the working copy. The rate engine
  // matches by first-found, so duplicates are silently ignored — flag in
  // the UI so the partner notices before saving.
  const duplicateServiceTypes = useMemo(() => {
    if (!editing) return new Set<string>();
    const counts = new Map<string, number>();
    for (const r of editing) counts.set(r.service_type, (counts.get(r.service_type) ?? 0) + 1);
    return new Set(Array.from(counts.entries()).filter(([, n]) => n > 1).map(([s]) => s));
  }, [editing]);

  const editValid = !editing || (
    duplicateServiceTypes.size === 0 &&
    editing.every((r) =>
      r.base_rate_cents >= 0 &&
      r.per_kg_cents    >= 0 &&
      r.max_weight_kg   >  0
    )
  );

  function startEdit() {
    setEditing(carrier ? carrier.rate_cards.map((r) => ({ ...r, coverage_zones: [...r.coverage_zones] })) : []);
    setSaved(false);
    setError(null);
  }

  function cancelEdit() {
    setEditing(null);
    setError(null);
  }

  function patchRow(idx: number, patch: Partial<RateCard>) {
    if (!editing) return;
    const next = editing.slice();
    next[idx] = { ...next[idx], ...patch };
    setEditing(next);
  }

  function removeRow(idx: number) {
    if (!editing) return;
    setEditing(editing.filter((_, i) => i !== idx));
  }

  function addRow() {
    if (!editing) return;
    setEditing([...editing, emptyRateCard()]);
  }

  async function handleSave() {
    if (!editing || !carrier || !editValid) return;
    setSaving(true);
    setError(null);
    try {
      const updated = await carriersApi.update(getCurrentPartnerId(), { rate_cards: editing });
      setCarrier(updated);
      setEditing(null);
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
            <GitBranch size={20} className="text-cyan-neon" />
            Rate Cards
          </h1>
          <p className="text-sm text-white/40 font-mono mt-0.5">
            {carrier ? `${carrier.name} (${carrier.code})` : "Partner rate card"}
          </p>
        </div>
        <div className="flex items-center gap-2">
          {carrier && <NeonBadge variant={carrier.status === "active" ? "green" : "amber"} dot>{carrier.status}</NeonBadge>}
          <button
            onClick={load}
            className="flex items-center gap-1.5 rounded-lg border border-glass-border px-3 py-2 text-xs text-white/60 hover:text-white transition-colors"
            title="Refresh"
          >
            <RefreshCw size={12} />
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

      {/* SLA summary */}
      <motion.div variants={variants.fadeInUp} className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        {loading && !carrier ? (
          Array.from({ length: 4 }).map((_, i) => (
            <GlassCard key={i} size="sm">
              <div className="h-10 animate-pulse rounded bg-glass-200" />
            </GlassCard>
          ))
        ) : carrier ? [
          { label: "Service Types", value: String(rateCards.length),                         color: "text-cyan-neon"    },
          { label: "On-Time Target", value: `${carrier.sla.on_time_target_pct.toFixed(0)}%`, color: "text-green-signal" },
          { label: "Max Days",       value: `${carrier.sla.max_delivery_days}d`,             color: "text-white"        },
          { label: "Breach Penalty", value: carrier.sla.penalty_per_breach > 0 ? fmtPhp(carrier.sla.penalty_per_breach) : "—", color: "text-amber-signal" },
        ].map((m) => (
          <GlassCard key={m.label} size="sm">
            <p className="text-2xs font-mono text-white/30 uppercase tracking-wider">{m.label}</p>
            <p className={`text-sm font-bold font-mono mt-1 ${m.color}`}>{m.value}</p>
          </GlassCard>
        )) : null}
      </motion.div>

      {/* Rate card table — inline edit when `editing` is non-null. */}
      <motion.div variants={variants.fadeInUp}>
        <GlassCard padding="none">
          <div className="flex items-center justify-between px-5 py-4 border-b border-glass-border">
            <div>
              <h2 className="font-heading text-sm font-semibold text-white">Rate Cards</h2>
              <p className="text-2xs font-mono text-white/30 mt-0.5">
                {editing
                  ? "Editing — changes are local until you tap Save."
                  : "Self-serve pricing. Tap Edit to update base + per-kg rates and coverage zones."}
              </p>
            </div>
            <div className="flex items-center gap-2">
              {saved && <span className="text-xs text-green-signal font-mono">✓ Saved</span>}
              {editing ? (
                <>
                  <button
                    onClick={handleSave}
                    disabled={saving || !editValid}
                    className="flex items-center gap-1.5 rounded-lg border border-green-signal/30 bg-green-surface px-3 py-2 text-xs font-medium text-green-signal hover:border-green-signal/60 transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
                  >
                    <Save size={12} />
                    {saving ? "Saving…" : "Save"}
                  </button>
                  <button
                    onClick={cancelEdit}
                    disabled={saving}
                    className="flex items-center gap-1.5 rounded-lg border border-glass-border px-3 py-2 text-xs text-white/60 hover:text-white transition-colors disabled:opacity-40"
                  >
                    <X size={12} /> Cancel
                  </button>
                </>
              ) : (
                <>
                  <button
                    disabled
                    title="PDF export coming soon"
                    className="flex items-center gap-1.5 rounded-lg border border-glass-border px-3 py-2 text-xs text-white/30 cursor-not-allowed"
                  >
                    <Download size={12} /> Export
                  </button>
                  <button
                    onClick={startEdit}
                    disabled={loading || !carrier}
                    className="flex items-center gap-1.5 rounded-lg border border-cyan-neon/30 bg-cyan-neon/10 px-3 py-2 text-xs font-medium text-cyan-neon hover:border-cyan-neon/60 transition-colors disabled:opacity-40"
                  >
                    <Pencil size={12} /> Edit
                  </button>
                </>
              )}
            </div>
          </div>

          {/* Header row — column widths differ in edit mode (extra trash column). */}
          {editing ? (
            <div className="grid grid-cols-[140px_120px_140px_90px_1fr_36px] gap-3 px-5 py-2.5 border-b border-glass-border">
              {["Service", "Base ₱", "Per kg ₱", "Max kg", "Coverage Zones (comma-separated)", ""].map((h, i) => (
                <span key={i} className="text-2xs font-mono text-white/30 uppercase tracking-wider">{h}</span>
              ))}
            </div>
          ) : (
            <div className="grid grid-cols-[2fr_100px_120px_80px_1fr] gap-3 px-5 py-2.5 border-b border-glass-border">
              {["Service", "Base", "Per kg", "Max kg", "Coverage Zones"].map((h) => (
                <span key={h} className="text-2xs font-mono text-white/30 uppercase tracking-wider">{h}</span>
              ))}
            </div>
          )}

          {loading ? (
            <div className="px-5 py-10 text-center text-xs text-white/40 font-mono">loading…</div>
          ) : rateCards.length === 0 && !editing ? (
            <div className="px-5 py-10 text-center">
              <p className="text-xs text-white/40 font-mono">
                No rate cards configured. Tap Edit to add your first pricing tier.
              </p>
            </div>
          ) : editing ? (
            <>
              {editing.map((r, idx) => {
                const isDup = duplicateServiceTypes.has(r.service_type);
                return (
                  <div
                    key={idx}
                    className={`grid grid-cols-[140px_120px_140px_90px_1fr_36px] gap-3 items-center px-5 py-3 border-b border-glass-border/50 ${isDup ? "bg-amber-signal/5" : ""}`}
                  >
                    <select
                      value={r.service_type}
                      onChange={(e) => patchRow(idx, { service_type: e.target.value })}
                      className={`rounded-md border bg-glass-100 px-2 py-1.5 text-sm text-white outline-none focus:border-cyan-neon/40 ${isDup ? "border-amber-signal/60" : "border-glass-border"}`}
                    >
                      {SERVICE_TYPES.map((t) => (
                        <option key={t} value={t} style={{ background: "#0d1422" }}>{t.replace(/_/g, " ")}</option>
                      ))}
                    </select>
                    <input
                      type="number"
                      min={0}
                      step={1}
                      value={r.base_rate_cents / 100}
                      onChange={(e) => patchRow(idx, { base_rate_cents: Math.round(parseFloat(e.target.value || "0") * 100) })}
                      className="rounded-md border border-glass-border bg-glass-100 px-2 py-1.5 text-sm text-white font-mono outline-none focus:border-cyan-neon/40"
                    />
                    <input
                      type="number"
                      min={0}
                      step={1}
                      value={r.per_kg_cents / 100}
                      onChange={(e) => patchRow(idx, { per_kg_cents: Math.round(parseFloat(e.target.value || "0") * 100) })}
                      className="rounded-md border border-glass-border bg-glass-100 px-2 py-1.5 text-sm text-white font-mono outline-none focus:border-cyan-neon/40"
                    />
                    <input
                      type="number"
                      min={0.1}
                      step={0.5}
                      value={r.max_weight_kg}
                      onChange={(e) => patchRow(idx, { max_weight_kg: parseFloat(e.target.value || "0") })}
                      className="rounded-md border border-glass-border bg-glass-100 px-2 py-1.5 text-sm text-white font-mono outline-none focus:border-cyan-neon/40"
                    />
                    <input
                      type="text"
                      placeholder="Metro Manila, Cavite, Laguna"
                      value={r.coverage_zones.join(", ")}
                      onChange={(e) => patchRow(idx, {
                        coverage_zones: e.target.value
                          .split(",")
                          .map((z) => z.trim())
                          .filter((z) => z.length > 0),
                      })}
                      className="rounded-md border border-glass-border bg-glass-100 px-2 py-1.5 text-sm text-white font-mono outline-none focus:border-cyan-neon/40"
                    />
                    <button
                      onClick={() => removeRow(idx)}
                      title="Remove row"
                      className="flex h-8 w-8 items-center justify-center rounded-md border border-red-signal/30 text-red-signal hover:bg-red-signal/10 transition-colors"
                    >
                      <Trash2 size={12} />
                    </button>
                  </div>
                );
              })}

              {duplicateServiceTypes.size > 0 && (
                <p className="px-5 py-2 text-2xs font-mono text-amber-signal">
                  ⚠ Duplicate service types: {Array.from(duplicateServiceTypes).join(", ")}.
                  The rate engine will only honour the first row of each — rename or delete duplicates before saving.
                </p>
              )}

              <div className="px-5 py-3 border-b border-glass-border/50">
                <button
                  onClick={addRow}
                  className="flex items-center gap-1.5 rounded-lg border border-cyan-neon/30 bg-cyan-neon/5 px-3 py-1.5 text-xs font-medium text-cyan-neon hover:border-cyan-neon/60 transition-colors"
                >
                  <Plus size={12} /> Add rate card
                </button>
              </div>
            </>
          ) : (
            rateCards.map((r) => (
              <div key={r.service_type} className="grid grid-cols-[2fr_100px_120px_80px_1fr] gap-3 items-center px-5 py-3.5 border-b border-glass-border/50 hover:bg-glass-100 transition-colors">
                <span className="text-sm font-medium text-white capitalize">{r.service_type.replace(/_/g, " ")}</span>
                <span className="text-sm font-bold font-mono text-green-signal">{fmtPhp(r.base_rate_cents)}</span>
                <span className="text-xs font-mono text-white/60">{fmtPhp(r.per_kg_cents)} / kg</span>
                <span className="text-xs font-mono text-white/60">{r.max_weight_kg} kg</span>
                <span className="text-xs text-white/40 font-mono truncate" title={r.coverage_zones.join(", ")}>
                  {r.coverage_zones.length === 0 ? "—" : r.coverage_zones.join(", ")}
                </span>
              </div>
            ))
          )}
        </GlassCard>
      </motion.div>

      {/* Rate-shop calculator */}
      <motion.div variants={variants.fadeInUp}>
        <RateShopCalculator />
      </motion.div>
    </motion.div>
  );
}

// ── Rate shop calculator ───────────────────────────────────────────────────────

function RateShopCalculator() {
  const [serviceType, setServiceType] = useState<ServiceType>("standard");
  const [weightKg, setWeightKg]       = useState<string>("5");
  const [quotes, setQuotes]           = useState<RateQuote[] | null>(null);
  const [loading, setLoading]         = useState(false);
  const [error, setError]             = useState<string | null>(null);

  const weightNumber = useMemo(() => {
    const w = parseFloat(weightKg);
    return Number.isFinite(w) && w > 0 ? w : null;
  }, [weightKg]);

  async function handleShop() {
    if (weightNumber === null) return;
    setLoading(true);
    setError(null);
    try {
      const q = await carriersApi.rateShop({ service_type: serviceType, weight_kg: weightNumber });
      setQuotes(q);
    } catch (e) {
      const err = e as { message?: string };
      setError(err?.message ?? "Rate shop failed");
      setQuotes(null);
    } finally {
      setLoading(false);
    }
  }

  return (
    <GlassCard padding="none">
      <div className="flex items-center justify-between px-5 py-4 border-b border-glass-border">
        <div>
          <h2 className="font-heading text-sm font-semibold text-white flex items-center gap-2">
            <Calculator size={14} className="text-purple-plasma" />
            Rate Shop Calculator
          </h2>
          <p className="text-2xs font-mono text-white/30 mt-0.5">Compare carrier quotes for a hypothetical shipment.</p>
        </div>
      </div>

      <div className="grid grid-cols-[1fr_1fr_120px] gap-3 px-5 py-4 border-b border-glass-border">
        <div>
          <label className="mb-1 block text-2xs font-mono text-white/40 uppercase tracking-wider">Service</label>
          <select
            value={serviceType}
            onChange={(e) => setServiceType(e.target.value as ServiceType)}
            className="w-full rounded-lg border border-glass-border bg-glass-100 px-3 py-2 text-sm text-white outline-none focus:border-cyan-neon/40"
          >
            {SERVICE_TYPES.map((t) => (
              <option key={t} value={t} style={{ background: "#0d1422" }}>{t.replace(/_/g, " ")}</option>
            ))}
          </select>
        </div>
        <div>
          <label className="mb-1 block text-2xs font-mono text-white/40 uppercase tracking-wider">Weight (kg)</label>
          <input
            type="number"
            inputMode="decimal"
            min={0.1}
            step={0.1}
            value={weightKg}
            onChange={(e) => setWeightKg(e.target.value)}
            className="w-full rounded-lg border border-glass-border bg-glass-100 px-3 py-2 text-sm text-white outline-none focus:border-cyan-neon/40"
          />
        </div>
        <div className="flex items-end">
          <button
            onClick={handleShop}
            disabled={loading || weightNumber === null}
            className="w-full rounded-lg bg-gradient-to-r from-purple-plasma to-cyan-neon px-4 py-2 text-xs font-semibold text-white disabled:opacity-40 transition-opacity"
          >
            {loading ? "Shopping…" : "Get Quotes"}
          </button>
        </div>
      </div>

      {error && (
        <p className="px-5 py-3 text-xs text-red-signal font-mono">{error}</p>
      )}

      {quotes && (
        <>
          <div className="grid grid-cols-[2fr_100px_120px_1fr] gap-3 px-5 py-2.5 border-b border-glass-border">
            {["Carrier", "Quote", "Eligible", "Reason"].map((h) => (
              <span key={h} className="text-2xs font-mono text-white/30 uppercase tracking-wider">{h}</span>
            ))}
          </div>

          {quotes.length === 0 ? (
            <p className="px-5 py-10 text-center text-xs text-white/40 font-mono">
              No carriers match this service + weight. Adjust inputs or onboard more carriers.
            </p>
          ) : (
            quotes.map((q) => (
              <div key={q.carrier_id} className="grid grid-cols-[2fr_100px_120px_1fr] gap-3 items-center px-5 py-3 border-b border-glass-border/50">
                <span className="text-xs font-medium text-white">{q.carrier_name}</span>
                <span className={`text-sm font-bold font-mono ${q.eligible ? "text-green-signal" : "text-white/40"}`}>
                  {fmtPhp(q.total_cost_cents)}
                </span>
                <NeonBadge variant={q.eligible ? "green" : "muted"} dot>
                  {q.eligible ? "eligible" : "ineligible"}
                </NeonBadge>
                <span className="text-2xs text-white/40 font-mono truncate" title={q.ineligibility_reason ?? ""}>
                  {q.eligible ? "—" : q.ineligibility_reason ?? "—"}
                </span>
              </div>
            ))
          )}
        </>
      )}
    </GlassCard>
  );
}
