"use client";
import { useState, useMemo, useRef, useCallback } from "react";
import { Calculator, Plus, Trash2, AlertTriangle } from "lucide-react";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { fmtCurrency } from "@/lib/data/currencies";
import type {
  BalikbayanRates,
  PhProvinceZoneEntry,
  AddOnService,
} from "@/lib/api/balikbayan-rates";
import { addressLookupApi, type AddressCode } from "@/lib/api/addresses";

interface Props {
  rates: BalikbayanRates;
}

interface BoxItem {
  size_id: string;
  quantity: number;
}

interface QuoteLine {
  label: string;
  amount: number;
  currency: string;
  note?: string;
  component: "sea" | "air" | "ph_delivery" | "addon";
}

// ── Quote engine ──────────────────────────────────────────────────────────────

function resolveProvince(province: string, map: PhProvinceZoneEntry[]): PhProvinceZoneEntry | null {
  if (!province.trim()) return null;
  const q = province.toLowerCase().trim();
  // Exact match
  const exact = map.find((e) => e.province.toLowerCase() === q);
  if (exact) return exact;
  // Starts-with (city within province string)
  const sw = map.find((e) => q.startsWith(e.province.toLowerCase()) || e.province.toLowerCase().startsWith(q));
  return sw ?? null;
}

function computeQuote(
  rates: BalikbayanRates,
  mode: "sea" | "air",
  originKey: string,         // sea: origin string; air: zone_name
  items: BoxItem[],
  weightKg: number,
  dimL: number, dimW: number, dimH: number,
  province: string,
  addonIds: string[],
): QuoteLine[] {
  const lines: QuoteLine[] = [];

  // ── Sea cargo ────────────────────────────────────────────────────────────────
  if (mode === "sea") {
    const row = rates.sea_cargo.find((r) => r.origin === originKey);
    if (row) {
      for (const item of items) {
        if (!item.size_id || item.quantity <= 0) continue;
        const price = (row.prices[item.size_id] ?? 0) * item.quantity;
        const sizeName = rates.box_sizes.find((s) => s.id === item.size_id)?.name ?? item.size_id;
        lines.push({
          label: `Sea Freight — ${sizeName} × ${item.quantity}`,
          amount: price,
          currency: row.currency ?? "USD",
          note: row.origin,
          component: "sea",
        });
      }
    }
  }

  // ── Air cargo ─────────────────────────────────────────────────────────────────
  if (mode === "air") {
    const zone = rates.air_cargo_zones.find((z) => z.zone_name === originKey);
    const fixed = rates.air_cargo_fixed;
    if (zone && fixed) {
      const volumetricKg = (dimL > 0 && dimW > 0 && dimH > 0)
        ? (dimL * dimW * dimH) / fixed.volumetric_divisor
        : 0;
      const chargeableKg = Math.max(
        fixed.min_weight_kg,
        weightKg,
        volumetricKg,
      );
      const zoneCur = zone.currency ?? "USD";
      const fixedCur = fixed.currency ?? "USD";

      // Zone rate
      const zoneCharge = chargeableKg * zone.rate_per_kg_usd;
      lines.push({
        label: `Air Freight — ${chargeableKg.toFixed(1)} kg chargeable`,
        amount: zoneCharge,
        currency: zoneCur,
        note: volumetricKg > weightKg
          ? `vol ${volumetricKg.toFixed(1)} kg > actual ${weightKg} kg`
          : `actual ${weightKg} kg`,
        component: "air",
      });

      // FSC
      const fsc = zoneCharge * fixed.fuel_surcharge_pct;
      if (fsc > 0) {
        lines.push({
          label: `Fuel Surcharge (${(fixed.fuel_surcharge_pct * 100).toFixed(0)}% FSC)`,
          amount: fsc,
          currency: zoneCur,
          component: "air",
        });
      }

      // AWB + THC + customs (in fixed.currency)
      if (fixed.awb_fee_usd > 0) {
        lines.push({ label: "AWB Fee", amount: fixed.awb_fee_usd, currency: fixedCur, component: "air" });
      }
      if (fixed.thc_usd > 0) {
        lines.push({ label: "Terminal Handling Charge (THC)", amount: fixed.thc_usd, currency: fixedCur, component: "air" });
      }
      if (fixed.customs_clearance_usd > 0) {
        lines.push({ label: "PH Customs Clearance", amount: fixed.customs_clearance_usd, currency: fixedCur, component: "air" });
      }
    }
  }

  // ── PH local delivery ────────────────────────────────────────────────────────
  const resolved = resolveProvince(province, rates.province_zone_map);
  if (resolved) {
    const zone = rates.ph_delivery_zones.find((z) => z.zone_code === resolved.zone_code);
    if (zone) {
      const phCur = rates.ph_delivery_currency ?? "PHP";
      for (const item of items) {
        if (!item.size_id || item.quantity <= 0) continue;
        const price = (zone.prices[item.size_id] ?? 0) * item.quantity;
        const sizeName = rates.box_sizes.find((s) => s.id === item.size_id)?.name ?? item.size_id;
        lines.push({
          label: `PH Delivery (${zone.zone_code}) — ${sizeName} × ${item.quantity}`,
          amount: price,
          currency: phCur,
          note: zone.zone_name,
          component: "ph_delivery",
        });
      }
    }
  }

  // ── Add-ons ───────────────────────────────────────────────────────────────────
  const seaTotal = lines.filter((l) => l.component === "sea").reduce((s, l) => s + l.amount, 0);
  const baseForPct = seaTotal > 0 ? seaTotal : lines.filter((l) => l.component === "air").reduce((s, l) => s + l.amount, 0);

  for (const id of addonIds) {
    const addon: AddOnService | undefined = rates.addons.find((a) => a.id === id);
    if (!addon) continue;
    const cur = addon.currency ?? "USD";
    let amount = 0;
    let note = "";

    switch (addon.rate_type) {
      case "fixed":
        amount = addon.rate;
        break;
      case "percent":
        amount = Math.max(addon.min_charge, baseForPct * addon.rate);
        note = `${(addon.rate * 100).toFixed(1)}% of freight`;
        break;
      case "per_kg":
        amount = Math.max(addon.min_charge, weightKg * addon.rate);
        note = `${weightKg} kg`;
        break;
      case "per_day":
        amount = addon.rate;
        note = "per day";
        break;
    }

    if (amount > 0) {
      lines.push({ label: addon.name, amount, currency: cur, note, component: "addon" });
    }
  }

  return lines;
}

// ── Component ─────────────────────────────────────────────────────────────────

const COMPONENT_COLOR: Record<string, string> = {
  sea:         "text-cyan-neon",
  air:         "text-purple-plasma",
  ph_delivery: "text-green-signal",
  addon:       "text-amber-signal",
};

const inputCls  = "rounded-md border border-glass-border bg-glass-100 px-2 py-1.5 text-xs text-white font-mono outline-none focus:border-cyan-neon/40 w-full";
const selectCls = inputCls;

export function QuoteCalculatorTab({ rates }: Props) {
  const [mode, setMode] = useState<"sea" | "air">("sea");

  // Sea: select from sea_cargo origins; Air: select from air_cargo_zones
  const [originKey, setOriginKey] = useState<string>(rates.sea_cargo[0]?.origin ?? "");

  // Box items (sea mode only)
  const [items, setItems] = useState<BoxItem[]>([
    { size_id: rates.box_sizes[0]?.id ?? "", quantity: 1 },
  ]);

  // Air weight & dims
  const [weightKg, setWeightKg] = useState(20);
  const [dimL, setDimL] = useState(0);
  const [dimW, setDimW] = useState(0);
  const [dimH, setDimH] = useState(0);

  // Recipient province
  const [province, setProvince] = useState("");

  // Add-ons
  const [addonIds, setAddonIds] = useState<string[]>([]);

  // Province type-ahead
  const [provinceSuggestions, setProvinceSuggestions] = useState<AddressCode[]>([]);
  const provinceTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const onProvinceChange = useCallback((value: string) => {
    setProvince(value);
    if (provinceTimerRef.current) clearTimeout(provinceTimerRef.current);
    if (value.length < 2) { setProvinceSuggestions([]); return; }
    provinceTimerRef.current = setTimeout(async () => {
      const hits = await addressLookupApi.byQuery("PH", value, 8);
      const seen = new Set<string>();
      const unique = hits.filter(h => {
        const key = h.state_province ?? h.city;
        if (seen.has(key)) return false;
        seen.add(key);
        return true;
      });
      setProvinceSuggestions(unique.slice(0, 6));
    }, 400);
  }, []);

  // Derived: resolved zone
  const resolvedEntry = useMemo(
    () => resolveProvince(province, rates.province_zone_map),
    [province, rates.province_zone_map],
  );
  const resolvedZone = resolvedEntry
    ? rates.ph_delivery_zones.find((z) => z.zone_code === resolvedEntry.zone_code)
    : null;

  // Compute quote
  const lines = useMemo(
    () => computeQuote(rates, mode, originKey, items, weightKg, dimL, dimW, dimH, province, addonIds),
    [rates, mode, originKey, items, weightKg, dimL, dimW, dimH, province, addonIds],
  );

  // Currencies used
  const currencies = [...new Set(lines.map((l) => l.currency))];
  const currencyTotals = currencies.map((cur) => ({
    currency: cur,
    total: lines.filter((l) => l.currency === cur).reduce((s, l) => s + l.amount, 0),
  }));

  function toggleAddon(id: string) {
    setAddonIds((prev) => prev.includes(id) ? prev.filter((x) => x !== id) : [...prev, id]);
  }

  function switchMode(m: "sea" | "air") {
    setMode(m);
    if (m === "sea") setOriginKey(rates.sea_cargo[0]?.origin ?? "");
    else             setOriginKey(rates.air_cargo_zones[0]?.zone_name ?? "");
  }

  function reset() {
    setMode("sea");
    setOriginKey(rates.sea_cargo[0]?.origin ?? "");
    setItems([{ size_id: rates.box_sizes[0]?.id ?? "", quantity: 1 }]);
    setWeightKg(20);
    setDimL(0); setDimW(0); setDimH(0);
    setProvince("");
    setProvinceSuggestions([]);
    setAddonIds([]);
  }

  // Add-ons filtered by mode
  const relevantAddons = rates.addons.filter((a) =>
    mode === "sea"
      ? !a.id.includes("-air-")
      : !a.id.includes("-sea-"),
  );

  return (
    <div className="flex flex-col gap-4">
      {/* Info banner */}
      <GlassCard glow="cyan" accent size="sm">
        <div className="flex items-center gap-2">
          <Calculator size={14} className="text-cyan-neon flex-shrink-0" />
          <p className="text-xs font-mono text-cyan-neon">
            QUOTE SIMULATOR — preview what a customer would be quoted based on your current rate card
          </p>
        </div>
      </GlassCard>

      <div className="grid grid-cols-1 lg:grid-cols-[1fr_380px] gap-4">
        {/* ── Left: inputs ──────────────────────────────────────────────────────── */}
        <GlassCard padding="none">
          {/* Mode selector */}
          <div className="flex gap-1 p-4 border-b border-glass-border">
            {(["sea", "air"] as const).map((m) => (
              <button
                key={m}
                onClick={() => switchMode(m)}
                className={[
                  "flex-1 rounded-lg py-2 text-xs font-medium font-mono transition-colors",
                  mode === m
                    ? "bg-cyan-neon/10 border border-cyan-neon/40 text-cyan-neon"
                    : "border border-glass-border text-white/40 hover:text-white/70",
                ].join(" ")}
              >
                {m === "sea" ? "🚢 Sea Freight" : "✈️ Air Freight"}
              </button>
            ))}
          </div>

          <div className="flex flex-col gap-5 p-5">
            {/* Origin */}
            <div>
              <label className="text-2xs font-mono text-white/40 uppercase tracking-wider block mb-1.5">
                {mode === "sea" ? "Origin Country / Region" : "Air Cargo Zone"}
              </label>
              <select
                value={originKey}
                onChange={(e) => setOriginKey(e.target.value)}
                className={selectCls}
              >
                {mode === "sea"
                  ? rates.sea_cargo.map((r) => (
                      <option key={r.origin} value={r.origin} style={{ background: "#0d1422" }}>
                        {r.origin}
                      </option>
                    ))
                  : rates.air_cargo_zones.map((z) => (
                      <option key={z.zone_name} value={z.zone_name} style={{ background: "#0d1422" }}>
                        {z.zone_name}
                      </option>
                    ))
                }
              </select>
            </div>

            {/* Box items (sea) / Weight + dims (air) */}
            {mode === "sea" ? (
              <div>
                <label className="text-2xs font-mono text-white/40 uppercase tracking-wider block mb-1.5">
                  Box Sizes
                </label>
                <div className="flex flex-col gap-2">
                  {items.map((item, idx) => (
                    <div key={idx} className="flex items-center gap-2">
                      <select
                        value={item.size_id}
                        onChange={(e) => {
                          const next = items.slice();
                          next[idx] = { ...next[idx], size_id: e.target.value };
                          setItems(next);
                        }}
                        className={selectCls + " flex-1"}
                      >
                        {rates.box_sizes.map((s) => (
                          <option key={s.id} value={s.id} style={{ background: "#0d1422" }}>
                            {s.name} ({s.dimensions})
                          </option>
                        ))}
                      </select>
                      <span className="text-2xs font-mono text-white/30">×</span>
                      <input
                        type="number" min={1} step={1} value={item.quantity}
                        onChange={(e) => {
                          const next = items.slice();
                          next[idx] = { ...next[idx], quantity: parseInt(e.target.value || "1") };
                          setItems(next);
                        }}
                        className="rounded-md border border-glass-border bg-glass-100 px-2 py-1.5 text-xs text-white font-mono outline-none focus:border-cyan-neon/40 w-16 text-center"
                      />
                      {items.length > 1 && (
                        <button onClick={() => setItems(items.filter((_, i) => i !== idx))}
                          className="flex h-7 w-7 items-center justify-center rounded text-red-signal/50 hover:text-red-signal transition-colors">
                          <Trash2 size={10} />
                        </button>
                      )}
                    </div>
                  ))}
                  {rates.box_sizes.length > 0 && (
                    <button
                      onClick={() => setItems([...items, { size_id: rates.box_sizes[0].id, quantity: 1 }])}
                      className="flex items-center gap-1 text-2xs font-mono text-cyan-neon/60 hover:text-cyan-neon transition-colors mt-1"
                    >
                      <Plus size={10} /> add another size
                    </button>
                  )}
                </div>
              </div>
            ) : (
              <div className="flex flex-col gap-3">
                <div>
                  <label className="text-2xs font-mono text-white/40 uppercase tracking-wider block mb-1.5">
                    Actual Weight (kg)
                  </label>
                  <input
                    type="number" min={1} step={0.5} value={weightKg}
                    onChange={(e) => setWeightKg(parseFloat(e.target.value || "1"))}
                    className={inputCls + " w-28"}
                  />
                </div>
                <div>
                  <label className="text-2xs font-mono text-white/40 uppercase tracking-wider block mb-1.5">
                    Dimensions — L × W × H cm <span className="text-white/20">(optional, for volumetric)</span>
                  </label>
                  <div className="flex items-center gap-2">
                    {[
                      { val: dimL, set: setDimL },
                      { val: dimW, set: setDimW },
                      { val: dimH, set: setDimH },
                    ].map(({ val, set }, i) => (
                      <input
                        key={i}
                        type="number" min={0} step={1} value={val || ""}
                        onChange={(e) => set(parseFloat(e.target.value || "0"))}
                        placeholder={["L", "W", "H"][i]}
                        className="rounded-md border border-glass-border bg-glass-100 px-2 py-1.5 text-xs text-white font-mono outline-none focus:border-cyan-neon/40 w-20 text-center"
                      />
                    ))}
                    <span className="text-2xs font-mono text-white/30">cm</span>
                  </div>
                  {dimL > 0 && dimW > 0 && dimH > 0 && (
                    <p className="text-2xs font-mono text-white/40 mt-1">
                      Volumetric: {((dimL * dimW * dimH) / (rates.air_cargo_fixed?.volumetric_divisor ?? 5000)).toFixed(2)} kg
                      {" "}(÷ {rates.air_cargo_fixed?.volumetric_divisor ?? 5000})
                    </p>
                  )}
                </div>
              </div>
            )}

            {/* Recipient province */}
            <div className="relative">
              <label className="text-2xs font-mono text-white/40 uppercase tracking-wider block mb-1.5">
                Recipient Province / City
              </label>
              <input
                value={province}
                onChange={(e) => onProvinceChange(e.target.value)}
                placeholder="e.g. Cebu City, Davao City, Bulacan…"
                className={inputCls}
              />
              {provinceSuggestions.length > 0 && (
                <ul className="absolute z-10 mt-1 w-full rounded-md border border-glass-border bg-[#0d1422] py-1 shadow-lg">
                  {provinceSuggestions.map((s, i) => (
                    <li key={i}>
                      <button
                        type="button"
                        onMouseDown={(e) => e.preventDefault()}
                        onClick={() => { setProvince(s.state_province ?? s.city); setProvinceSuggestions([]); }}
                        className="w-full px-3 py-1.5 text-left text-xs font-mono text-white/80 hover:bg-cyan-neon/10 hover:text-cyan-neon transition-colors"
                      >
                        {s.state_province ?? s.city}
                        {s.state_province && <span className="ml-2 text-white/30">{s.city}</span>}
                      </button>
                    </li>
                  ))}
                </ul>
              )}
              {province.trim() && provinceSuggestions.length === 0 && (
                <p className={[
                  "text-2xs font-mono mt-1",
                  resolvedZone ? "text-green-signal" : "text-amber-signal",
                ].join(" ")}>
                  {resolvedZone
                    ? `✓ ${resolvedEntry!.zone_code} — ${resolvedZone.zone_name} · ${resolvedZone.transit_days}`
                    : "⚠ Province not found in zone map — add it in the Province Map tab"
                  }
                </p>
              )}
            </div>

            {/* Add-ons */}
            {relevantAddons.length > 0 && (
              <div>
                <label className="text-2xs font-mono text-white/40 uppercase tracking-wider block mb-1.5">
                  Add-Ons <span className="text-white/20">(optional)</span>
                </label>
                <div className="flex flex-wrap gap-1.5">
                  {relevantAddons.map((a) => (
                    <button
                      key={a.id}
                      onClick={() => toggleAddon(a.id)}
                      className={[
                        "rounded-full border px-2.5 py-0.5 text-2xs font-mono transition-colors",
                        addonIds.includes(a.id)
                          ? "border-amber-signal/60 bg-amber-signal/10 text-amber-signal"
                          : "border-glass-border text-white/40 hover:text-white/60",
                      ].join(" ")}
                    >
                      {a.name}
                    </button>
                  ))}
                </div>
              </div>
            )}

            <button
              onClick={reset}
              className="self-start text-2xs font-mono text-white/30 hover:text-white/60 transition-colors"
            >
              Clear / Reset
            </button>
          </div>
        </GlassCard>

        {/* ── Right: quote output ───────────────────────────────────────────────── */}
        <div className="flex flex-col gap-3">
          <GlassCard padding="none" glow="cyan">
            <div className="px-5 py-4 border-b border-glass-border">
              <p className="text-2xs font-mono text-white/40 uppercase tracking-wider">Itemized Quote</p>
            </div>

            {lines.length === 0 ? (
              <div className="px-5 py-8 text-center">
                <Calculator size={28} className="text-white/10 mx-auto mb-2" />
                <p className="text-xs font-mono text-white/30">Select origin and recipient to generate a quote</p>
              </div>
            ) : (
              <>
                <div className="divide-y divide-glass-border/30">
                  {lines.map((line, i) => (
                    <div key={i} className="flex items-start justify-between gap-3 px-5 py-3">
                      <div className="min-w-0">
                        <p className={`text-xs font-medium ${COMPONENT_COLOR[line.component]}`}>
                          {line.label}
                        </p>
                        {line.note && (
                          <p className="text-2xs font-mono text-white/30 mt-0.5">{line.note}</p>
                        )}
                      </div>
                      <p className={`text-sm font-bold font-mono flex-shrink-0 ${COMPONENT_COLOR[line.component]}`}>
                        {fmtCurrency(line.amount, line.currency)}
                      </p>
                    </div>
                  ))}
                </div>

                {/* Totals per currency */}
                <div className="px-5 py-4 border-t border-glass-border bg-glass-100/20">
                  <p className="text-2xs font-mono text-white/30 uppercase tracking-wider mb-2">
                    {currencyTotals.length === 1 ? "Total" : "Totals by currency"}
                  </p>
                  {currencyTotals.map(({ currency, total }) => (
                    <div key={currency} className="flex items-center justify-between gap-2">
                      <span className="text-2xs font-mono text-white/40">{currency}</span>
                      <span className="text-lg font-bold font-mono text-white">
                        {fmtCurrency(total, currency)}
                      </span>
                    </div>
                  ))}
                </div>
              </>
            )}
          </GlassCard>

          {/* Multi-currency warning */}
          {currencies.length > 1 && (
            <GlassCard size="sm">
              <div className="flex items-start gap-2">
                <AlertTriangle size={13} className="text-amber-signal flex-shrink-0 mt-0.5" />
                <p className="text-2xs font-mono text-white/50">
                  This quote spans <span className="text-amber-signal font-bold">{currencies.join(" + ")}</span>.
                  Totals cannot be combined without FX conversion. Consider setting all rate sections to a single
                  operating currency.
                </p>
              </div>
            </GlassCard>
          )}

          {/* Legend */}
          <div className="flex flex-wrap gap-3 px-1">
            {[
              { label: "Sea Freight",  color: "text-cyan-neon"    },
              { label: "Air Freight",  color: "text-purple-plasma"},
              { label: "PH Delivery",  color: "text-green-signal" },
              { label: "Add-Ons",      color: "text-amber-signal" },
            ].map(({ label, color }) => (
              <div key={label} className="flex items-center gap-1">
                <span className={`text-2xs font-bold font-mono ${color}`}>●</span>
                <span className="text-2xs font-mono text-white/30">{label}</span>
              </div>
            ))}
          </div>

          {/* API hint */}
          <GlassCard size="sm">
            <p className="text-2xs font-mono text-white/30">
              <span className="text-purple-plasma font-bold">POST</span>{" "}
              <span className="text-white/50">/api/carriers/[id]/balikbayan-quote</span>
              {" "}— same computation, callable by customer portal and AI agent.
            </p>
          </GlassCard>
        </div>
      </div>
    </div>
  );
}
