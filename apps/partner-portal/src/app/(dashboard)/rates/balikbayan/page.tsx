"use client";
/**
 * /rates/balikbayan — Balikbayan Box Rate Cards
 *
 * Per-carrier editable rate tables covering:
 *   Box Sizes   — partner-configurable size catalogue (add / rename / remove)
 *   Sea Cargo   — origin country × box size → USD (dynamic columns)
 *   Air Cargo   — zone rate/kg + distance surcharge + fixed charges
 *   PH Delivery — province delivery zone surcharge (on top of sea/air)
 *   Volumetric  — per-group (Manila/Luzon/Visayas/Mindanao/Islands) pricing
 *   Add-Ons     — insurance, crate, packing, surcharges
 *   Bundles     — pre-packaged multi-box deals with fixed price
 *
 * Data source: GET /v1/carriers/:id/balikbayan-rates
 * Falls back to built-in defaults on 404 (backend endpoint pending).
 * Saves via:  PUT /v1/carriers/:id/balikbayan-rates
 */
import { useCallback, useEffect, useState } from "react";
import Link from "next/link";
import { motion } from "framer-motion";
import * as Tabs from "@radix-ui/react-tabs";
import {
  ArrowLeft, Package, Plane, MapPin, Box, Layers, Gift,
  Pencil, Save, X, RefreshCw, Download,
} from "lucide-react";
import { variants } from "@/lib/design-system/tokens";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { carriersApi } from "@/lib/api/carriers";
import {
  balikbayanRatesApi,
  type BalikbayanRates,
  type BoxSize,
} from "@/lib/api/balikbayan-rates";
import { buildDefaultRates } from "@/lib/data/balikbayan-defaults";
import { BoxSizesManager } from "@/components/balikbayan/BoxSizesManager";
import { SeaCargoTab }    from "@/components/balikbayan/SeaCargoTab";
import { AirCargoTab }    from "@/components/balikbayan/AirCargoTab";
import { PhDeliveryTab }  from "@/components/balikbayan/PhDeliveryTab";
import { VolumetricTab }  from "@/components/balikbayan/VolumetricTab";
import { AddOnsTab }      from "@/components/balikbayan/AddOnsTab";
import { BundlesTab }     from "@/components/balikbayan/BundlesTab";

const TABS = [
  { id: "sea",        label: "Sea Cargo",        icon: Package },
  { id: "air",        label: "Air Cargo",         icon: Plane   },
  { id: "ph",         label: "PH Delivery Zones", icon: MapPin  },
  { id: "volumetric", label: "Volumetric",         icon: Box     },
  { id: "addons",     label: "Add-Ons",            icon: Layers  },
  { id: "bundles",    label: "Bundles",            icon: Gift    },
] as const;

type TabId = typeof TABS[number]["id"];

function exportAllCsv(rates: BalikbayanRates) {
  const sizes = rates.box_sizes;
  const sections: string[] = [
    "=== BOX SIZES ===",
    "id,name,dimensions,max_weight_kg,sort_order",
    ...sizes.map((s) => [s.id, `"${s.name}"`, `"${s.dimensions}"`, s.max_weight_kg, s.sort_order].join(",")),
    "",
    "=== SEA CARGO RATES ===",
    ["origin", "transit_days", ...sizes.map((s) => s.name)].join(","),
    ...rates.sea_cargo.map((r) =>
      [`"${r.origin}"`, r.transit_days, ...sizes.map((s) => r.prices[s.id] ?? 0)].join(",")
    ),
    "",
    "=== AIR CARGO ZONES ===",
    "zone_name,origins,rate_per_kg_usd,distance_surcharge_per_km,transit_days",
    ...rates.air_cargo_zones.map((z) =>
      [`"${z.zone_name}"`, `"${z.origins}"`, z.rate_per_kg_usd, z.distance_surcharge_per_km, z.transit_days].join(",")
    ),
    "",
    "=== PH DELIVERY ZONES ===",
    ["zone_code", "zone_name", "coverage", ...sizes.map((s) => s.name), "transit_days"].join(","),
    ...rates.ph_delivery_zones.map((z) =>
      [`"${z.zone_code}"`, `"${z.zone_name}"`, `"${z.coverage}"`, ...sizes.map((s) => z.prices[s.id] ?? 0), `"${z.transit_days}"`].join(",")
    ),
    "",
    "=== VOLUMETRIC GROUPS ===",
    "group,divisor,base_rate_usd,rate_per_cbm_usd,min_charge_usd,surcharge_pct",
    ...rates.volumetric_groups.map((g) =>
      [g.group, g.divisor, g.base_rate_usd, g.rate_per_cbm_usd, g.min_charge_usd, g.surcharge_pct].join(",")
    ),
    "",
    "=== BUNDLES ===",
    "id,name,description,items,price_usd,valid_for,notes",
    ...rates.bundles.map((b) =>
      [b.id, `"${b.name}"`, `"${b.description}"`, `"${b.items.map((i) => `${i.size_id}×${i.quantity}`).join("; ")}"`, b.price_usd, b.valid_for, `"${b.notes}"`].join(",")
    ),
    "",
    "=== ADD-ONS ===",
    "id,name,category,rate,rate_type,min_charge,description",
    ...rates.addons.map((a) =>
      [a.id, `"${a.name}"`, a.category, a.rate, a.rate_type, a.min_charge, `"${a.description}"`].join(",")
    ),
  ];
  const blob = new Blob([sections.join("\n")], { type: "text/csv;charset=utf-8" });
  const url  = URL.createObjectURL(blob);
  const anchor = Object.assign(document.createElement("a"), {
    href: url,
    download: `balikbayan-rates-${new Date().toISOString().slice(0, 10)}.csv`,
  });
  document.body.appendChild(anchor); anchor.click(); document.body.removeChild(anchor); URL.revokeObjectURL(url);
}

export default function BalikbayanRatesPage() {
  const [activeTab, setActiveTab] = useState<TabId>("sea");

  // Server-committed state
  const [rates, setRates]     = useState<BalikbayanRates | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError]     = useState<string | null>(null);

  // Edit-mode working copy — null = read mode
  const [editing, setEditing] = useState<BalikbayanRates | null>(null);
  const [saving, setSaving]   = useState(false);
  const [saved, setSaved]     = useState(false);

  const load = useCallback(async () => {
    setError(null);
    setLoading(true);
    try {
      const carrier = await carriersApi.me();
      const data    = await balikbayanRatesApi.get(carrier);
      setRates(data ?? buildDefaultRates(
        typeof carrier.id === "string" ? carrier.id : carrier.id[0]
      ));
    } catch (e) {
      const err = e as { message?: string };
      setError(err?.message ?? "Failed to load rates");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { load(); }, [load]);

  function startEdit() {
    if (!rates) return;
    setEditing(JSON.parse(JSON.stringify(rates)) as BalikbayanRates);
    setSaved(false);
    setError(null);
  }

  function cancelEdit() { setEditing(null); setError(null); }

  function handleSizesChange(newSizes: BoxSize[], deletedIds: string[]) {
    if (!editing) return;
    const seaCargo = editing.sea_cargo.map((row) => {
      if (!deletedIds.length) return row;
      const prices = { ...row.prices };
      deletedIds.forEach((id) => delete prices[id]);
      return { ...row, prices };
    });
    const phZones = editing.ph_delivery_zones.map((zone) => {
      if (!deletedIds.length) return zone;
      const prices = { ...zone.prices };
      deletedIds.forEach((id) => delete prices[id]);
      return { ...zone, prices };
    });
    setEditing({ ...editing, box_sizes: newSizes, sea_cargo: seaCargo, ph_delivery_zones: phZones });
  }

  async function handleSave() {
    if (!editing) return;
    setSaving(true);
    setError(null);
    try {
      const carrier = await carriersApi.me();
      const saved_rates = await balikbayanRatesApi.save(carrier, {
        box_sizes:         editing.box_sizes,
        sea_cargo:         editing.sea_cargo,
        air_cargo_zones:   editing.air_cargo_zones,
        air_cargo_fixed:   editing.air_cargo_fixed,
        ph_delivery_zones: editing.ph_delivery_zones,
        volumetric_groups: editing.volumetric_groups,
        bundles:           editing.bundles,
        addons:            editing.addons,
      });
      setRates(saved_rates);
      setEditing(null);
      setSaved(true);
      setTimeout(() => setSaved(false), 2500);
    } catch (e) {
      // Backend endpoint may not exist yet — persist locally and show warning
      const err = e as { response?: { status?: number }; message?: string };
      if (err?.response?.status === 404 || err?.response?.status === 405) {
        setRates({ ...editing, updated_at: new Date().toISOString() });
        setEditing(null);
        setSaved(true);
        setTimeout(() => setSaved(false), 2500);
        setError("Saved locally — backend endpoint /v1/carriers/:id/balikbayan-rates not yet deployed.");
      } else {
        setError(err?.message ?? "Save failed");
      }
    } finally {
      setSaving(false);
    }
  }

  const display   = editing ?? rates;
  const isEditing = editing !== null;

  return (
    <motion.div
      variants={variants.staggerContainer}
      initial="hidden"
      animate="visible"
      className="flex flex-col gap-5 p-6"
    >
      {/* ── Header ─────────────────────────────────────────────────────────── */}
      <motion.div variants={variants.fadeInUp} className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex items-center gap-3">
          <Link
            href="/rates"
            className="flex items-center gap-1.5 rounded-lg border border-glass-border px-2.5 py-1.5 text-xs text-white/50 hover:text-white transition-colors"
          >
            <ArrowLeft size={12} /> Rate Cards
          </Link>
          <div>
            <h1 className="font-heading text-2xl font-bold text-white flex items-center gap-2">
              <Package size={20} className="text-cyan-neon" />
              Balikbayan Box Rates
            </h1>
            <p className="text-xs font-mono text-white/40 mt-0.5">
              From any country worldwide → to anywhere in the Philippines
            </p>
          </div>
        </div>

        <div className="flex items-center gap-2">
          {saved && <span className="text-xs text-green-signal font-mono">✓ Saved</span>}

          {isEditing ? (
            <>
              <button
                onClick={handleSave}
                disabled={saving}
                className="flex items-center gap-1.5 rounded-lg border border-green-signal/30 bg-green-surface px-3 py-2 text-xs font-medium text-green-signal hover:border-green-signal/60 transition-colors disabled:opacity-40"
              >
                <Save size={12} /> {saving ? "Saving…" : "Save All"}
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
              {display && (
                <button
                  onClick={() => exportAllCsv(display)}
                  className="flex items-center gap-1.5 rounded-lg border border-glass-border px-3 py-2 text-xs text-white/60 hover:text-white transition-colors"
                >
                  <Download size={12} /> Export All
                </button>
              )}
              <button
                onClick={load}
                className="flex items-center gap-1.5 rounded-lg border border-glass-border px-3 py-2 text-xs text-white/60 hover:text-white transition-colors"
                title="Refresh"
              >
                <RefreshCw size={12} />
              </button>
              <button
                onClick={startEdit}
                disabled={loading || !display}
                className="flex items-center gap-1.5 rounded-lg border border-cyan-neon/30 bg-cyan-neon/10 px-3 py-2 text-xs font-medium text-cyan-neon hover:border-cyan-neon/60 transition-colors disabled:opacity-40"
              >
                <Pencil size={12} /> Edit Rates
              </button>
            </>
          )}
        </div>
      </motion.div>

      {/* ── Edit-mode banner ───────────────────────────────────────────────── */}
      {isEditing && (
        <motion.div variants={variants.fadeInUp}>
          <GlassCard glow="amber" accent size="sm">
            <p className="text-xs font-mono text-amber-signal">
              ✎ Editing mode — changes are local until you tap Save All. All tabs share one save action.
              Box size changes cascade to Sea Cargo and PH Delivery pricing columns.
            </p>
          </GlassCard>
        </motion.div>
      )}

      {/* ── Error ─────────────────────────────────────────────────────────── */}
      {error && (
        <motion.div variants={variants.fadeInUp}>
          <GlassCard size="sm">
            <p className="text-xs font-mono text-amber-signal">{error}</p>
          </GlassCard>
        </motion.div>
      )}

      {/* ── KPI strip ─────────────────────────────────────────────────────── */}
      {display && (
        <motion.div variants={variants.fadeInUp} className="grid grid-cols-3 gap-3 sm:grid-cols-6">
          {[
            { label: "Box Sizes",    value: display.box_sizes.length,          color: "text-purple-plasma" },
            { label: "Sea Origins",  value: display.sea_cargo.length,           color: "text-cyan-neon"    },
            { label: "Air Zones",    value: display.air_cargo_zones.length,     color: "text-purple-plasma"},
            { label: "PH Zones",     value: display.ph_delivery_zones.length,   color: "text-green-signal" },
            { label: "Vol. Groups",  value: display.volumetric_groups.length,   color: "text-amber-signal" },
            { label: "Bundles",      value: display.bundles.length,             color: "text-cyan-neon"    },
          ].map((m) => (
            <GlassCard key={m.label} size="sm">
              <p className="text-2xs font-mono text-white/30 uppercase tracking-wider">{m.label}</p>
              <p className={`text-2xl font-bold font-mono mt-1 ${m.color}`}>{m.value}</p>
            </GlassCard>
          ))}
        </motion.div>
      )}

      {/* ── Loading skeleton ───────────────────────────────────────────────── */}
      {loading && (
        <motion.div variants={variants.fadeInUp}>
          <GlassCard>
            <div className="flex flex-col gap-3">
              {Array.from({ length: 5 }).map((_, i) => (
                <div key={i} className="h-8 animate-pulse rounded bg-glass-200" />
              ))}
            </div>
          </GlassCard>
        </motion.div>
      )}

      {/* ── Box sizes manager (edit mode only) ────────────────────────────── */}
      {isEditing && editing && (
        <motion.div variants={variants.fadeInUp}>
          <BoxSizesManager
            sizes={editing.box_sizes}
            onSizesChange={handleSizesChange}
          />
        </motion.div>
      )}

      {/* ── Tabs ──────────────────────────────────────────────────────────── */}
      {display && !loading && (
        <motion.div variants={variants.fadeInUp}>
          <Tabs.Root
            value={activeTab}
            onValueChange={(v) => setActiveTab(v as TabId)}
          >
            {/* Tab list */}
            <Tabs.List className="flex gap-1 overflow-x-auto border-b border-glass-border pb-0 mb-5">
              {TABS.map(({ id, label, icon: Icon }) => (
                <Tabs.Trigger
                  key={id}
                  value={id}
                  className={[
                    "flex items-center gap-1.5 px-4 py-2.5 text-xs font-medium font-mono whitespace-nowrap",
                    "border-b-2 -mb-px transition-colors outline-none",
                    activeTab === id
                      ? "border-cyan-neon text-cyan-neon"
                      : "border-transparent text-white/40 hover:text-white/70",
                  ].join(" ")}
                >
                  <Icon size={13} />
                  {label}
                  {isEditing && (
                    <NeonBadge variant="amber" className="ml-1">edit</NeonBadge>
                  )}
                </Tabs.Trigger>
              ))}
            </Tabs.List>

            <Tabs.Content value="sea" className="outline-none">
              <SeaCargoTab
                rows={display.sea_cargo}
                boxSizes={display.box_sizes}
                editing={isEditing}
                onChange={(rows) => editing && setEditing({ ...editing, sea_cargo: rows })}
              />
            </Tabs.Content>

            <Tabs.Content value="air" className="outline-none">
              <AirCargoTab
                zones={display.air_cargo_zones}
                fixed={display.air_cargo_fixed}
                editing={isEditing}
                onZonesChange={(zones) => editing && setEditing({ ...editing, air_cargo_zones: zones })}
                onFixedChange={(fixed) => editing && setEditing({ ...editing, air_cargo_fixed: fixed })}
              />
            </Tabs.Content>

            <Tabs.Content value="ph" className="outline-none">
              <PhDeliveryTab
                zones={display.ph_delivery_zones}
                boxSizes={display.box_sizes}
                editing={isEditing}
                onChange={(zones) => editing && setEditing({ ...editing, ph_delivery_zones: zones })}
              />
            </Tabs.Content>

            <Tabs.Content value="volumetric" className="outline-none">
              <VolumetricTab
                groups={display.volumetric_groups}
                editing={isEditing}
                onChange={(groups) => editing && setEditing({ ...editing, volumetric_groups: groups })}
              />
            </Tabs.Content>

            <Tabs.Content value="addons" className="outline-none">
              <AddOnsTab
                addons={display.addons}
                editing={isEditing}
                onChange={(addons) => editing && setEditing({ ...editing, addons })}
              />
            </Tabs.Content>

            <Tabs.Content value="bundles" className="outline-none">
              <BundlesTab
                bundles={display.bundles}
                boxSizes={display.box_sizes}
                editing={isEditing}
                onChange={(bundles) => editing && setEditing({ ...editing, bundles })}
              />
            </Tabs.Content>
          </Tabs.Root>
        </motion.div>
      )}

      {/* ── Last updated ──────────────────────────────────────────────────── */}
      {display?.updated_at && !loading && (
        <motion.div variants={variants.fadeInUp}>
          <p className="text-2xs font-mono text-white/20 text-right">
            Last updated: {new Date(display.updated_at).toLocaleString()}
          </p>
        </motion.div>
      )}
    </motion.div>
  );
}
