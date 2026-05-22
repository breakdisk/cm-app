"use client";
import { createApiClient } from "./client";
import { carrierIdOf, type Carrier } from "./carriers";

// ── Box size config ────────────────────────────────────────────────────────────

export interface BoxSize {
  id: string;           // stable slug, unique per carrier — e.g. "jumbo", "bulilit"
  name: string;         // display label — e.g. "Jumbo", "Bulilit"
  dimensions: string;   // e.g. '61×61×61 cm'
  max_weight_kg: number;
  sort_order: number;
}

// ── Bundle config ──────────────────────────────────────────────────────────────

export interface BoxBundleItem {
  size_id: string;
  quantity: number;
}

export type BundleScope = "sea" | "air" | "both";

export interface BoxBundle {
  id: string;
  name: string;
  description: string;
  items: BoxBundleItem[];
  currency: string;     // ISO 4217 — currency the bundle price is quoted in
  price_usd: number;    // amount in `currency` (field name kept for compat)
  valid_for: BundleScope;
  notes: string;
}

// ── Rate table types ───────────────────────────────────────────────────────────

// prices is keyed by BoxSize.id — columns are fully dynamic, driven by box_sizes
export interface SeaCargoRate {
  origin: string;
  transit_days: string;
  currency: string;     // ISO 4217 — native currency for this origin
  prices: Record<string, number>;
}

export interface AirCargoZone {
  zone_name: string;
  origins: string;
  currency: string;     // ISO 4217 — currency rate_per_kg and dist. surcharge are quoted in
  rate_per_kg_usd: number;   // amount in `currency` (field name kept for compat)
  distance_surcharge_per_km: number;
  transit_days: string;
}

export interface AirCargoFixed {
  currency: string;     // ISO 4217 — currency for awb_fee, thc, customs_clearance
  awb_fee_usd: number;          // amount in `currency`
  fuel_surcharge_pct: number;   // percentage — currency-agnostic
  thc_usd: number;              // amount in `currency`
  customs_clearance_usd: number;// amount in `currency`
  min_weight_kg: number;
  volumetric_divisor: number;
}

// prices keyed by BoxSize.id — currency set at rate-card level via ph_delivery_currency
export interface PhDeliveryZone {
  zone_code: string;
  zone_name: string;
  coverage: string;
  prices: Record<string, number>;
  transit_days: string;
}

// Maps a recipient province or city name → PH delivery zone code
// Used by the quote calculator and the public /balikbayan-quote API
export interface PhProvinceZoneEntry {
  province: string;   // province or city name — matched case-insensitively
  zone_code: string;  // must match a PhDeliveryZone.zone_code
}

export type VolumetricGroupName = "Manila" | "Luzon" | "Visayas" | "Mindanao" | "Islands";

export interface VolumetricGroup {
  group: VolumetricGroupName;
  currency: string;     // ISO 4217 — currency for base_rate, rate_per_cbm, min_charge
  divisor: number;
  base_rate_usd: number;       // amount in `currency`
  rate_per_cbm_usd: number;    // amount in `currency`
  min_charge_usd: number;      // amount in `currency`
  surcharge_pct: number;
}

export type AddOnCategory = "insurance" | "crate" | "packing" | "surcharge";
export type AddOnRateType = "fixed" | "percent" | "per_kg" | "per_day";

export interface AddOnService {
  id: string;
  name: string;
  category: AddOnCategory;
  currency: string;     // ISO 4217 — currency for rate and min_charge (ignored for percent type)
  rate: number;
  rate_type: AddOnRateType;
  min_charge: number;
  description: string;
}

// ── Root document ──────────────────────────────────────────────────────────────

export interface BalikbayanRates {
  carrier_id: string;
  box_sizes: BoxSize[];
  sea_cargo: SeaCargoRate[];
  air_cargo_zones: AirCargoZone[];
  air_cargo_fixed: AirCargoFixed;
  ph_delivery_zones: PhDeliveryZone[];
  ph_delivery_currency: string;          // ISO 4217 — currency for all ph_delivery_zones prices (default: "PHP")
  province_zone_map: PhProvinceZoneEntry[];  // province / city → zone_code lookup table
  volumetric_groups: VolumetricGroup[];
  bundles: BoxBundle[];
  addons: AddOnService[];
  updated_at: string;
}

// ── API ────────────────────────────────────────────────────────────────────────

export const balikbayanRatesApi = {
  async get(carrier: Carrier): Promise<BalikbayanRates | null> {
    try {
      const { data } = await createApiClient().get<BalikbayanRates>(
        `/v1/carriers/${carrierIdOf(carrier)}/balikbayan-rates`,
      );
      return data;
    } catch (e) {
      const err = e as { status?: number; response?: { status?: number } };
      const status = err?.response?.status ?? err?.status;
      if (status === 404 || status === undefined) return null;
      throw e;
    }
  },

  async save(
    carrier: Carrier,
    rates: Omit<BalikbayanRates, "carrier_id" | "updated_at">,
  ): Promise<BalikbayanRates> {
    const { data } = await createApiClient().put<BalikbayanRates>(
      `/v1/carriers/${carrierIdOf(carrier)}/balikbayan-rates`,
      rates,
    );
    return data;
  },
};
