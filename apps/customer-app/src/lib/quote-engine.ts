/**
 * Quote Engine — Customer App (React Native)
 *
 * Same logic as the landing page quote engine but structured as a plain TS module
 * for use in React Native screens. Kept in sync manually with
 * apps/landing/src/lib/quote-engine.ts.
 *
 * No network dependency — all computation is local.
 */

export interface BoxSize {
  id: string;
  name: string;
  dimensions: string;
  dims_cm: [number, number, number];
  max_weight_kg: number;
  cbm: number;
}

export interface SeaCargoRate {
  origin: string;
  currency: string;
  transit_days: string;
  prices: Record<string, number>;
}

export interface AirCargoZone {
  zone_name: string;
  currency: string;
  origins: string;
  rate_per_kg: number;
  fuel_surcharge_pct: number;
  awb_fee: number;
  thc: number;
  customs: number;
  min_weight_kg: number;
  volumetric_divisor: number;
  transit_days: string;
}

export interface PhDeliveryZone {
  zone_code: string;
  zone_name: string;
  prices: Record<string, number>;
  transit_days: string;
}

export interface ProvinceEntry {
  province: string;
  zone_code: string;
}

export interface QuoteLine {
  label: string;
  amount: number;
  currency: string;
  note?: string;
  component: 'sea' | 'air' | 'ph_delivery';
}

export interface QuoteResult {
  lines: QuoteLine[];
  total_origin_currency: number;
  origin_currency: string;
  cbm: number;
  transit_days: string;
}

// ── CBM ────────────────────────────────────────────────────────────────────────

export function computeCbm(l: number, w: number, h: number): number {
  if (l <= 0 || w <= 0 || h <= 0) return 0;
  return parseFloat(((l * w * h) / 1_000_000).toFixed(4));
}

export function matchToStandardSize(l: number, w: number, h: number): BoxSize | null {
  const measured = computeCbm(l, w, h);
  if (measured <= 0) return null;
  return BOX_SIZES.reduce((best, s) =>
    Math.abs(s.cbm - measured) < Math.abs(best.cbm - measured) ? s : best
  );
}

// ── FX (PHP → origin currency) ────────────────────────────────────────────────
const PHP_PER: Record<string, number> = {
  USD: 57.0, CAD: 42.0, GBP: 72.0, EUR: 61.5, AUD: 37.0, NZD: 34.0,
  JPY: 0.38, KRW: 0.042, HKD: 7.3, SGD: 43.0, AED: 15.5, SAR: 15.2,
  QAR: 15.7, NOK: 5.3, SEK: 5.2,
};

function phpToOrigin(phpAmount: number, currency: string): number {
  const rate = PHP_PER[currency] ?? PHP_PER['USD'];
  return parseFloat((phpAmount / rate).toFixed(2));
}

// ── Province lookup ────────────────────────────────────────────────────────────

export function resolveProvince(province: string): ProvinceEntry | null {
  const q = province.toLowerCase().trim();
  if (!q) return null;
  return (
    PROVINCE_MAP.find(e => e.province.toLowerCase() === q) ??
    PROVINCE_MAP.find(e =>
      q.startsWith(e.province.toLowerCase()) ||
      e.province.toLowerCase().startsWith(q)
    ) ??
    null
  );
}

// ── Quote computation ─────────────────────────────────────────────────────────

export function computeQuote(
  mode: 'sea' | 'air',
  originKey: string,
  sizeId: string,
  qty: number,
  weightKg: number,
  dimsCm: [number, number, number],
  province: string,
): QuoteResult {
  const lines: QuoteLine[] = [];
  const [dimL, dimW, dimH] = dimsCm;
  let originCurrency = 'USD';
  let transitDays = '';
  const cbm = computeCbm(dimL, dimW, dimH);

  if (mode === 'sea') {
    const row = SEA_CARGO.find(r => r.origin === originKey);
    if (row && qty > 0) {
      originCurrency = row.currency;
      transitDays = row.transit_days;
      const unitPrice = row.prices[sizeId] ?? 0;
      const sizeName = BOX_SIZES.find(s => s.id === sizeId)?.name ?? sizeId;
      lines.push({
        label: `Sea Freight — ${sizeName} × ${qty}`,
        amount: unitPrice * qty,
        currency: originCurrency,
        note: row.origin,
        component: 'sea',
      });
    }
  }

  if (mode === 'air') {
    const zone = AIR_ZONES.find(z => z.zone_name === originKey);
    if (zone) {
      originCurrency = zone.currency;
      transitDays = zone.transit_days;
      const volKg = (dimL > 0 && dimW > 0 && dimH > 0)
        ? (dimL * dimW * dimH) / zone.volumetric_divisor : 0;
      const chargeKg = Math.max(zone.min_weight_kg, weightKg, volKg);
      const freight = parseFloat((chargeKg * zone.rate_per_kg).toFixed(2));
      lines.push({
        label: `Air Freight — ${chargeKg.toFixed(1)} kg`,
        amount: freight, currency: originCurrency,
        note: volKg > weightKg ? `vol ${volKg.toFixed(1)} kg > actual ${weightKg} kg` : `actual ${weightKg} kg`,
        component: 'air',
      });
      const fsc = parseFloat((freight * zone.fuel_surcharge_pct).toFixed(2));
      if (fsc > 0) lines.push({ label: `FSC (${Math.round(zone.fuel_surcharge_pct * 100)}%)`, amount: fsc, currency: originCurrency, component: 'air' });
      if (zone.awb_fee > 0) lines.push({ label: 'AWB Fee', amount: zone.awb_fee, currency: originCurrency, component: 'air' });
      if (zone.thc > 0) lines.push({ label: 'THC', amount: zone.thc, currency: originCurrency, component: 'air' });
      if (zone.customs > 0) lines.push({ label: 'PH Customs', amount: zone.customs, currency: originCurrency, component: 'air' });
    }
  }

  const resolved = resolveProvince(province);
  if (resolved) {
    const zone = PH_ZONES.find(z => z.zone_code === resolved.zone_code);
    if (zone && qty > 0) {
      const phpPrice = (zone.prices[sizeId] ?? 0) * qty;
      const converted = phpToOrigin(phpPrice, originCurrency);
      const sizeName = BOX_SIZES.find(s => s.id === sizeId)?.name ?? sizeId;
      lines.push({
        label: `PH Delivery (${zone.zone_code}) — ${sizeName} × ${qty}`,
        amount: converted, currency: originCurrency,
        note: `${zone.zone_name} · ${zone.transit_days} · ₱${phpPrice.toLocaleString()} est.`,
        component: 'ph_delivery',
      });
    }
  }

  const total = parseFloat(lines.reduce((s, l) => s + l.amount, 0).toFixed(2));
  return { lines, total_origin_currency: total, origin_currency: originCurrency, cbm, transit_days: transitDays };
}

// ── Display helpers ────────────────────────────────────────────────────────────

export const CURRENCY_SYMBOL: Record<string, string> = {
  USD: '$', CAD: 'CA$', GBP: '£', EUR: '€', AUD: 'A$', NZD: 'NZ$',
  JPY: '¥', KRW: '₩',  HKD: 'HK$', SGD: 'S$', AED: 'AED ', SAR: 'SAR ',
  QAR: 'QR ', NOK: 'kr ', SEK: 'kr ',
};

export function fmtAmount(amount: number, currency: string): string {
  const sym = CURRENCY_SYMBOL[currency] ?? `${currency} `;
  if (currency === 'JPY' || currency === 'KRW') {
    return `${sym}${Math.round(amount).toLocaleString()}`;
  }
  return `${sym}${amount.toFixed(2)}`;
}

// ── Rate data (mirrors landing/src/lib/quote-engine.ts) ───────────────────────

function p(xl: number, jumbo: number, large: number, small: number): Record<string, number> {
  return { xl, jumbo, large, small, medium: Math.round((large + small) / 2 * 0.97), bulilit: Math.round(small * 0.79) };
}

export const BOX_SIZES: BoxSize[] = [
  { id: 'bulilit', name: 'Bulilit', dimensions: '30×25×25 cm', dims_cm: [30, 25, 25], max_weight_kg: 10, cbm: computeCbm(30, 25, 25) },
  { id: 'small',   name: 'Small',   dimensions: '41×30×30 cm', dims_cm: [41, 30, 30], max_weight_kg: 15, cbm: computeCbm(41, 30, 30) },
  { id: 'medium',  name: 'Medium',  dimensions: '46×36×36 cm', dims_cm: [46, 36, 36], max_weight_kg: 18, cbm: computeCbm(46, 36, 36) },
  { id: 'large',   name: 'Large',   dimensions: '51×41×41 cm', dims_cm: [51, 41, 41], max_weight_kg: 20, cbm: computeCbm(51, 41, 41) },
  { id: 'xl',      name: 'XL',      dimensions: '61×46×46 cm', dims_cm: [61, 46, 46], max_weight_kg: 25, cbm: computeCbm(61, 46, 46) },
  { id: 'jumbo',   name: 'Jumbo',   dimensions: '61×61×61 cm', dims_cm: [61, 61, 61], max_weight_kg: 30, cbm: computeCbm(61, 61, 61) },
];

export const SEA_CARGO: SeaCargoRate[] = [
  { origin: 'USA West Coast (CA, WA, OR)',          currency: 'USD', transit_days: '30–45 days', prices: p(120, 150, 95,  70)  },
  { origin: 'USA East Coast (NY, NJ, CT, MA)',      currency: 'USD', transit_days: '40–55 days', prices: p(150, 185, 118, 88)  },
  { origin: 'USA South & Central (TX, FL, IL, GA)', currency: 'USD', transit_days: '40–55 days', prices: p(140, 175, 110, 82)  },
  { origin: 'Hawaii',                               currency: 'USD', transit_days: '25–35 days', prices: p(110, 138, 88,  65)  },
  { origin: 'Alaska',                               currency: 'USD', transit_days: '45–60 days', prices: p(170, 210, 135, 100) },
  { origin: 'Guam / Saipan (CNMI)',                 currency: 'USD', transit_days: '14–21 days', prices: p(86,  108, 68,  51)  },
  { origin: 'Canada — British Columbia (Vancouver)', currency: 'CAD', transit_days: '35–48 days', prices: p(165, 205, 130, 96)  },
  { origin: 'Canada — Alberta (Calgary / Edmonton)', currency: 'CAD', transit_days: '38–52 days', prices: p(170, 212, 135, 100) },
  { origin: 'Canada — Ontario / Quebec',             currency: 'CAD', transit_days: '42–56 days', prices: p(180, 225, 143, 106) },
  { origin: 'United Kingdom — London',              currency: 'GBP', transit_days: '38–55 days', prices: p(110, 138, 87,  65)  },
  { origin: 'Ireland — Dublin',                     currency: 'EUR', transit_days: '40–55 days', prices: p(130, 163, 103, 76)  },
  { origin: 'Italy — Rome / Milan',                 currency: 'EUR', transit_days: '38–52 days', prices: p(128, 160, 101, 75)  },
  { origin: 'Spain — Madrid / Barcelona',           currency: 'EUR', transit_days: '38–52 days', prices: p(126, 158, 100, 74)  },
  { origin: 'Germany — Frankfurt / Munich',         currency: 'EUR', transit_days: '40–55 days', prices: p(129, 161, 102, 75)  },
  { origin: 'France — Paris',                       currency: 'EUR', transit_days: '40–55 days', prices: p(128, 160, 101, 75)  },
  { origin: 'Netherlands / Belgium — Amsterdam',    currency: 'EUR', transit_days: '40–55 days', prices: p(128, 160, 101, 75)  },
  { origin: 'UAE — Dubai / Abu Dhabi',              currency: 'AED', transit_days: '18–28 days', prices: p(368, 460, 294, 213) },
  { origin: 'Saudi Arabia — Riyadh',                currency: 'SAR', transit_days: '20–30 days', prices: p(383, 480, 308, 225) },
  { origin: 'Qatar — Doha',                         currency: 'QAR', transit_days: '20–30 days', prices: p(357, 444, 284, 208) },
  { origin: 'Kuwait',                               currency: 'USD', transit_days: '20–30 days', prices: p(99,  124, 79,  58)  },
  { origin: 'Australia — Sydney / Melbourne',       currency: 'AUD', transit_days: '22–32 days', prices: p(163, 204, 130, 95)  },
  { origin: 'Australia — Perth',                    currency: 'AUD', transit_days: '20–28 days', prices: p(154, 193, 124, 91)  },
  { origin: 'Japan — Tokyo / Osaka',                currency: 'JPY', transit_days: '10–18 days', prices: p(11900, 14900, 9500, 7000) },
  { origin: 'South Korea — Seoul / Busan',          currency: 'KRW', transit_days: '10–18 days', prices: p(100000, 125000, 79000, 58000) },
  { origin: 'Hong Kong',                            currency: 'HKD', transit_days: '7–12 days',  prices: p(500, 625, 398, 297)  },
  { origin: 'Singapore',                            currency: 'SGD', transit_days: '7–14 days',  prices: p(92,  115, 73,  54)   },
];

export const AIR_ZONES: AirCargoZone[] = [
  { zone_name: 'Zone 1 — ASEAN / NE Asia',          currency: 'USD', origins: 'HK, Singapore, Japan, South Korea, Taiwan, Guam', rate_per_kg: 5.50,  fuel_surcharge_pct: 0.18, awb_fee: 25, thc: 15, customs: 20, min_weight_kg: 5, volumetric_divisor: 5000, transit_days: '2–5 days'  },
  { zone_name: 'Zone 2 — Australia / NZ',            currency: 'USD', origins: 'Australia, New Zealand',                         rate_per_kg: 7.00,  fuel_surcharge_pct: 0.18, awb_fee: 25, thc: 15, customs: 20, min_weight_kg: 5, volumetric_divisor: 5000, transit_days: '4–8 days'  },
  { zone_name: 'Zone 3 — Middle East',               currency: 'USD', origins: 'UAE, Saudi Arabia, Qatar, Kuwait, Bahrain, Oman', rate_per_kg: 8.00, fuel_surcharge_pct: 0.18, awb_fee: 25, thc: 15, customs: 20, min_weight_kg: 5, volumetric_divisor: 5000, transit_days: '5–9 days'  },
  { zone_name: 'Zone 4 — Europe',                    currency: 'USD', origins: 'UK, Ireland, Germany, France, Netherlands',       rate_per_kg: 10.50, fuel_surcharge_pct: 0.18, awb_fee: 25, thc: 15, customs: 20, min_weight_kg: 5, volumetric_divisor: 5000, transit_days: '7–12 days' },
  { zone_name: 'Zone 5 — N. America West',           currency: 'USD', origins: 'USA West Coast, Hawaii',                          rate_per_kg: 12.00, fuel_surcharge_pct: 0.18, awb_fee: 25, thc: 15, customs: 20, min_weight_kg: 5, volumetric_divisor: 5000, transit_days: '7–12 days' },
  { zone_name: 'Zone 6 — N. America East / Central', currency: 'USD', origins: 'USA East, South & Central, Alaska, Canada',       rate_per_kg: 13.50, fuel_surcharge_pct: 0.18, awb_fee: 25, thc: 15, customs: 20, min_weight_kg: 5, volumetric_divisor: 5000, transit_days: '8–14 days' },
];

function ph(xl: number, jumbo: number, large: number, small: number): Record<string, number> {
  return { xl, jumbo, large, small, medium: Math.max(100, Math.round((large + small) / 2 * 0.95)), bulilit: Math.max(100, Math.round(small * 0.75)) };
}

export const PH_ZONES: PhDeliveryZone[] = [
  { zone_code: 'Zone 1A', zone_name: 'Metro Manila / NCR',              prices: ph(850,  1000, 650,  450),  transit_days: '1–2 days'  },
  { zone_code: 'Zone 2A', zone_name: 'Region III — Bulacan / Pampanga', prices: ph(1100, 1300, 850,  620),  transit_days: '2–3 days'  },
  { zone_code: 'Zone 2B', zone_name: 'CALABARZON',                      prices: ph(1100, 1300, 850,  620),  transit_days: '2–3 days'  },
  { zone_code: 'Zone 3A', zone_name: 'Ilocos / La Union / Pangasinan',  prices: ph(1450, 1800, 1100, 850),  transit_days: '3–5 days'  },
  { zone_code: 'Zone 3C', zone_name: 'CAR — Baguio / Cordillera',       prices: ph(1700, 2100, 1350, 1000), transit_days: '4–6 days'  },
  { zone_code: 'Zone 4A', zone_name: 'Bicol — Camarines / Naga',        prices: ph(1550, 2000, 1250, 900),  transit_days: '4–6 days'  },
  { zone_code: 'Zone 5A', zone_name: 'Metro Cebu',                      prices: ph(1350, 1700, 1050, 800),  transit_days: '3–5 days'  },
  { zone_code: 'Zone 5C', zone_name: 'Iloilo / Bacolod City',           prices: ph(1550, 2000, 1250, 900),  transit_days: '4–6 days'  },
  { zone_code: 'Zone 5E', zone_name: 'Negros Oriental / Bohol',         prices: ph(2000, 2500, 1550, 1200), transit_days: '5–8 days'  },
  { zone_code: 'Zone 6A', zone_name: 'Metro Davao',                     prices: ph(1800, 2250, 1400, 1000), transit_days: '5–7 days'  },
  { zone_code: 'Zone 6C', zone_name: 'Cagayan de Oro / Bukidnon',       prices: ph(1900, 2350, 1500, 1100), transit_days: '5–8 days'  },
  { zone_code: 'Zone 6E', zone_name: 'Zamboanga',                       prices: ph(2350, 2900, 1850, 1350), transit_days: '7–10 days' },
  { zone_code: 'Zone 7B', zone_name: 'Batanes',                         prices: ph(4050, 5050, 3200, 2350), transit_days: '14–21 days'},
];

export const PROVINCE_MAP: ProvinceEntry[] = [
  { province: 'Metro Manila', zone_code: 'Zone 1A' }, { province: 'Manila', zone_code: 'Zone 1A' },
  { province: 'Quezon City',  zone_code: 'Zone 1A' }, { province: 'Makati', zone_code: 'Zone 1A' },
  { province: 'Pasig',        zone_code: 'Zone 1A' }, { province: 'Taguig', zone_code: 'Zone 1A' },
  { province: 'NCR',          zone_code: 'Zone 1A' }, { province: 'Bulacan', zone_code: 'Zone 2A' },
  { province: 'Pampanga',     zone_code: 'Zone 2A' }, { province: 'Cavite',  zone_code: 'Zone 2B' },
  { province: 'Laguna',       zone_code: 'Zone 2B' }, { province: 'Batangas',zone_code: 'Zone 2B' },
  { province: 'Pangasinan',   zone_code: 'Zone 3A' }, { province: 'La Union',zone_code: 'Zone 3A' },
  { province: 'Baguio',       zone_code: 'Zone 3C' }, { province: 'Benguet', zone_code: 'Zone 3C' },
  { province: 'Naga',         zone_code: 'Zone 4A' }, { province: 'Cebu',    zone_code: 'Zone 5A' },
  { province: 'Cebu City',    zone_code: 'Zone 5A' }, { province: 'Iloilo',  zone_code: 'Zone 5C' },
  { province: 'Bacolod',      zone_code: 'Zone 5C' }, { province: 'Bohol',   zone_code: 'Zone 5E' },
  { province: 'Davao',        zone_code: 'Zone 6A' }, { province: 'Davao City', zone_code: 'Zone 6A' },
  { province: 'Cagayan de Oro', zone_code: 'Zone 6C' }, { province: 'Zamboanga', zone_code: 'Zone 6E' },
  { province: 'Batanes',      zone_code: 'Zone 7B' },
];
