"use client";
import { createApiClient } from "./client";
import { carrierIdOf, type Carrier } from "./carriers";

// ── Domain types ───────────────────────────────────────────────────────────────

export interface SeaCargoRate {
  origin: string;
  transit_days: string;
  jumbo_usd: number;
  xl_usd: number;
  large_usd: number;
  small_usd: number;
}

export interface AirCargoZone {
  zone_name: string;
  origins: string;
  rate_per_kg_usd: number;
  distance_surcharge_per_km: number;
  transit_days: string;
}

export interface AirCargoFixed {
  awb_fee_usd: number;
  fuel_surcharge_pct: number;
  thc_usd: number;
  customs_clearance_usd: number;
  min_weight_kg: number;
  volumetric_divisor: number;
}

export interface PhDeliveryZone {
  zone_code: string;
  zone_name: string;
  coverage: string;
  jumbo_usd: number;
  xl_usd: number;
  large_usd: number;
  small_usd: number;
  transit_days: string;
}

export type VolumetricGroupName = "Manila" | "Luzon" | "Visayas" | "Mindanao" | "Islands";

export interface VolumetricGroup {
  group: VolumetricGroupName;
  divisor: number;
  base_rate_usd: number;
  rate_per_cbm_usd: number;
  min_charge_usd: number;
  surcharge_pct: number;
}

export type AddOnCategory = "insurance" | "crate" | "packing" | "surcharge";
export type AddOnRateType = "fixed" | "percent" | "per_kg" | "per_day";

export interface AddOnService {
  id: string;
  name: string;
  category: AddOnCategory;
  rate: number;
  rate_type: AddOnRateType;
  min_charge: number;
  description: string;
}

export interface BalikbayanRates {
  carrier_id: string;
  sea_cargo: SeaCargoRate[];
  air_cargo_zones: AirCargoZone[];
  air_cargo_fixed: AirCargoFixed;
  ph_delivery_zones: PhDeliveryZone[];
  volumetric_groups: VolumetricGroup[];
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

  async save(carrier: Carrier, rates: Omit<BalikbayanRates, "carrier_id" | "updated_at">): Promise<BalikbayanRates> {
    const { data } = await createApiClient().put<BalikbayanRates>(
      `/v1/carriers/${carrierIdOf(carrier)}/balikbayan-rates`,
      rates,
    );
    return data;
  },
};
