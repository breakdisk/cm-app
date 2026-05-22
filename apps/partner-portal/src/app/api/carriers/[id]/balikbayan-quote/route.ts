/**
 * POST /api/carriers/[id]/balikbayan-quote
 *
 * Public quote endpoint — callable by the customer portal and AI agents.
 * Fetches the carrier's rate card from the backend (falls back to defaults),
 * runs the quote computation, and returns an itemized JSON result.
 *
 * Request body:
 *   mode             "sea" | "air"
 *   origin           Sea: exact origin string. Air: exact zone_name string.
 *   items            [{ size_id, quantity }]  — required for sea
 *   weight_kg        number  — required for air
 *   dim_l/w/h        number  — optional cm dimensions for volumetric (air)
 *   recipient        string  — province or city name, matched against province_zone_map
 *   addon_ids        string[]  — optional add-on IDs to include
 *
 * Response:
 *   line_items       array of { label, amount, currency, note?, component }
 *   totals           { [currency]: number }
 *   resolved_zone    { zone_code, zone_name } | null
 *   warnings         string[]
 */

import { NextRequest, NextResponse } from "next/server";
import type {
  BalikbayanRates,
  PhProvinceZoneEntry,
  AddOnService,
} from "@/lib/api/balikbayan-rates";
import { buildDefaultRates } from "@/lib/data/balikbayan-defaults";

// ── Types ─────────────────────────────────────────────────────────────────────

interface QuoteRequest {
  mode: "sea" | "air";
  origin: string;
  items?: { size_id: string; quantity: number }[];
  weight_kg?: number;
  dim_l?: number;
  dim_w?: number;
  dim_h?: number;
  recipient: string;
  addon_ids?: string[];
}

interface QuoteLine {
  label: string;
  amount: number;
  currency: string;
  note?: string;
  component: "sea" | "air" | "ph_delivery" | "addon";
}

// ── Quote engine (server-side, no browser deps) ───────────────────────────────

function resolveProvince(province: string, map: PhProvinceZoneEntry[]): PhProvinceZoneEntry | null {
  if (!province.trim()) return null;
  const q = province.toLowerCase().trim();
  const exact = map.find((e) => e.province.toLowerCase() === q);
  if (exact) return exact;
  const sw = map.find(
    (e) => q.startsWith(e.province.toLowerCase()) || e.province.toLowerCase().startsWith(q),
  );
  return sw ?? null;
}

function computeQuote(rates: BalikbayanRates, req: QuoteRequest): {
  lines: QuoteLine[];
  warnings: string[];
} {
  const lines: QuoteLine[] = [];
  const warnings: string[] = [];
  const items = req.items ?? [];
  const weightKg = req.weight_kg ?? 0;
  const dimL = req.dim_l ?? 0;
  const dimW = req.dim_w ?? 0;
  const dimH = req.dim_h ?? 0;

  // ── Sea ───────────────────────────────────────────────────────────────────────
  if (req.mode === "sea") {
    const row = rates.sea_cargo.find((r) => r.origin === req.origin);
    if (!row) {
      warnings.push(`Sea cargo origin not found: "${req.origin}"`);
    } else {
      for (const item of items) {
        if (!item.size_id || item.quantity <= 0) continue;
        const price = (row.prices[item.size_id] ?? 0) * item.quantity;
        const sizeName = rates.box_sizes.find((s) => s.id === item.size_id)?.name ?? item.size_id;
        if (price === 0) warnings.push(`No sea price for size "${item.size_id}" on origin "${req.origin}"`);
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

  // ── Air ───────────────────────────────────────────────────────────────────────
  if (req.mode === "air") {
    const zone  = rates.air_cargo_zones.find((z) => z.zone_name === req.origin);
    const fixed = rates.air_cargo_fixed;

    if (!zone) {
      warnings.push(`Air cargo zone not found: "${req.origin}"`);
    } else if (!fixed) {
      warnings.push("Air cargo fixed charges not configured");
    } else {
      const volumetricKg = dimL > 0 && dimW > 0 && dimH > 0
        ? (dimL * dimW * dimH) / (fixed.volumetric_divisor || 5000)
        : 0;
      const chargeableKg = Math.max(fixed.min_weight_kg ?? 0, weightKg, volumetricKg);
      const zoneCur  = zone.currency  ?? "USD";
      const fixedCur = fixed.currency ?? "USD";

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

      const fsc = zoneCharge * (fixed.fuel_surcharge_pct ?? 0);
      if (fsc > 0) {
        lines.push({
          label: `Fuel Surcharge (${((fixed.fuel_surcharge_pct ?? 0) * 100).toFixed(0)}%)`,
          amount: fsc,
          currency: zoneCur,
          component: "air",
        });
      }

      if ((fixed.awb_fee_usd ?? 0) > 0) {
        lines.push({ label: "AWB Fee",                   amount: fixed.awb_fee_usd,          currency: fixedCur, component: "air" });
      }
      if ((fixed.thc_usd ?? 0) > 0) {
        lines.push({ label: "Terminal Handling Charge",  amount: fixed.thc_usd,              currency: fixedCur, component: "air" });
      }
      if ((fixed.customs_clearance_usd ?? 0) > 0) {
        lines.push({ label: "PH Customs Clearance",      amount: fixed.customs_clearance_usd, currency: fixedCur, component: "air" });
      }
    }
  }

  // ── PH local delivery ─────────────────────────────────────────────────────────
  const resolved = resolveProvince(req.recipient, rates.province_zone_map ?? []);
  if (req.recipient.trim() && !resolved) {
    warnings.push(`Province not found in zone map: "${req.recipient}". Add it via the Province Map tab.`);
  }
  if (resolved) {
    const zone    = rates.ph_delivery_zones.find((z) => z.zone_code === resolved.zone_code);
    const phCur   = rates.ph_delivery_currency ?? "PHP";
    if (!zone) {
      warnings.push(`Zone code "${resolved.zone_code}" from province map has no matching delivery zone.`);
    } else {
      for (const item of items.length > 0 ? items : [{ size_id: "", quantity: 1 }]) {
        if (req.mode === "sea" && (!item.size_id || item.quantity <= 0)) continue;
        // Air mode: use first box size as reference for PH delivery (if no items provided)
        const sizeId  = item.size_id || rates.box_sizes[0]?.id || "";
        const qty     = item.quantity || 1;
        const price   = (zone.prices[sizeId] ?? 0) * qty;
        const sizeName = rates.box_sizes.find((s) => s.id === sizeId)?.name ?? sizeId;
        lines.push({
          label: `PH Delivery (${resolved.zone_code}) — ${sizeName} × ${qty}`,
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
  const baseForPct = seaTotal > 0
    ? seaTotal
    : lines.filter((l) => l.component === "air").reduce((s, l) => s + l.amount, 0);

  for (const id of req.addon_ids ?? []) {
    const addon: AddOnService | undefined = rates.addons?.find((a) => a.id === id);
    if (!addon) { warnings.push(`Add-on not found: "${id}"`); continue; }
    const cur = addon.currency ?? "USD";
    let amount = 0;
    let note   = "";

    switch (addon.rate_type) {
      case "fixed":
        amount = addon.rate;
        break;
      case "percent":
        amount = Math.max(addon.min_charge ?? 0, baseForPct * addon.rate);
        note   = `${(addon.rate * 100).toFixed(1)}% of freight`;
        break;
      case "per_kg":
        amount = Math.max(addon.min_charge ?? 0, weightKg * addon.rate);
        note   = `${weightKg} kg`;
        break;
      case "per_day":
        amount = addon.rate;
        note   = "per day";
        break;
    }

    if (amount > 0) {
      lines.push({ label: addon.name, amount, currency: cur, note, component: "addon" });
    }
  }

  return { lines, warnings };
}

// ── Route handler ─────────────────────────────────────────────────────────────

async function fetchRates(carrierId: string): Promise<BalikbayanRates> {
  const backendUrl = process.env.BACKEND_URL ?? process.env.NEXT_PUBLIC_API_BASE_URL ?? "http://localhost:8080";
  try {
    const res = await fetch(`${backendUrl}/v1/carriers/${carrierId}/balikbayan-rates`, {
      next: { revalidate: 60 }, // cache 60 s
    });
    if (res.ok) return (await res.json()) as BalikbayanRates;
  } catch {
    // backend not reachable — fall through to defaults
  }
  return buildDefaultRates(carrierId);
}

export async function POST(req: NextRequest, { params }: { params: { id: string } }) {
  const carrierId = params.id;

  let body: QuoteRequest;
  try {
    body = (await req.json()) as QuoteRequest;
  } catch {
    return NextResponse.json({ error: "Invalid JSON body" }, { status: 400 });
  }

  // Basic validation
  if (!body.mode || !["sea", "air"].includes(body.mode)) {
    return NextResponse.json({ error: "mode must be 'sea' or 'air'" }, { status: 400 });
  }
  if (!body.origin) {
    return NextResponse.json({ error: "origin is required" }, { status: 400 });
  }

  const rates  = await fetchRates(carrierId);
  const { lines, warnings } = computeQuote(rates, body);

  // Build totals map: { USD: 468.00, PHP: 1350.00 }
  const totals: Record<string, number> = {};
  for (const line of lines) {
    totals[line.currency] = (totals[line.currency] ?? 0) + line.amount;
  }

  // Resolved zone info
  const resolved = resolveProvince(body.recipient ?? "", rates.province_zone_map ?? []);
  const zone     = resolved ? rates.ph_delivery_zones.find((z) => z.zone_code === resolved.zone_code) : null;

  return NextResponse.json({
    carrier_id:     carrierId,
    mode:           body.mode,
    origin:         body.origin,
    recipient:      body.recipient ?? "",
    resolved_zone:  zone ? { zone_code: zone.zone_code, zone_name: zone.zone_name, transit_days: zone.transit_days } : null,
    line_items:     lines,
    totals,
    currencies:     Object.keys(totals),
    warnings,
    computed_at:    new Date().toISOString(),
  });
}

// Also expose GET for health / schema discovery
export async function GET() {
  return NextResponse.json({
    endpoint: "POST /api/carriers/[id]/balikbayan-quote",
    description: "Compute an itemized balikbayan box shipping quote",
    body_schema: {
      mode:      "'sea' | 'air'",
      origin:    "string — sea cargo origin or air cargo zone_name",
      items:     "[{ size_id: string, quantity: number }]",
      weight_kg: "number (required for air)",
      dim_l:     "number cm (optional, for volumetric)",
      dim_w:     "number cm (optional, for volumetric)",
      dim_h:     "number cm (optional, for volumetric)",
      recipient: "string — province or city (matched against province_zone_map)",
      addon_ids: "string[] (optional)",
    },
  });
}
