/**
 * Quote Engine — Landing Page
 *
 * Self-contained: all rate data and computation logic lives here.
 * No external API dependency — suitable for unauthenticated public pages.
 *
 * Currency policy: ALL amounts (sea freight, air freight, PH delivery) are
 * expressed in the origin currency selected by the user. PH delivery prices
 * are stored in the rate card as PHP and converted to origin currency using
 * a static FX table. Treat results as indicative estimates, not final invoices.
 */

// ── Types ──────────────────────────────────────────────────────────────────────

export interface BoxSize {
  id: string;
  name: string;
  dimensions: string;
  dims_cm: [number, number, number]; // [L, W, H]
  max_weight_kg: number;
  cbm: number; // pre-computed
  emoji: string;
}

export interface SeaCargoRate {
  origin: string;
  currency: string;
  transit_days: string;
  prices: Record<string, number>; // size_id → price
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
  coverage: string;
  prices: Record<string, number>; // size_id → price in PHP
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
  component: "sea" | "air" | "ph_delivery";
}

export interface QuoteResult {
  lines: QuoteLine[];
  total_origin_currency: number;
  origin_currency: string;
  cbm: number;
  transit_days: string;
}

// ── CBM helper ─────────────────────────────────────────────────────────────────

export function computeCbm(l: number, w: number, h: number): number {
  if (l <= 0 || w <= 0 || h <= 0) return 0;
  return parseFloat(((l * w * h) / 1_000_000).toFixed(4));
}

/** Returns the box size whose CBM is closest to the measured CBM. */
export function matchToStandardSize(l: number, w: number, h: number): BoxSize | null {
  const measured = computeCbm(l, w, h);
  if (measured <= 0) return null;
  return BOX_SIZES.reduce((best, s) =>
    Math.abs(s.cbm - measured) < Math.abs(best.cbm - measured) ? s : best
  );
}

// ── Static PHP → origin-currency FX table ─────────────────────────────────────
// PHP amount ÷ PHP_PER[currency] = amount in origin currency
// Rates are indicative (mid-market, ±5%). Not for final invoicing.

const PHP_PER: Record<string, number> = {
  USD: 57.0,
  CAD: 42.0,
  GBP: 72.0,
  EUR: 61.5,
  AUD: 37.0,
  NZD: 34.0,
  JPY: 0.38,
  KRW: 0.042,
  HKD: 7.3,
  SGD: 43.0,
  AED: 15.5,
  SAR: 15.2,
  QAR: 15.7,
  NOK: 5.3,
  SEK: 5.2,
};

function phpToOrigin(phpAmount: number, originCurrency: string): number {
  const rate = PHP_PER[originCurrency] ?? PHP_PER["USD"];
  return parseFloat((phpAmount / rate).toFixed(2));
}

// ── Province → zone lookup ─────────────────────────────────────────────────────

export function resolveProvince(province: string): ProvinceEntry | null {
  const q = province.toLowerCase().trim();
  if (!q) return null;
  const exact = PROVINCE_MAP.find(e => e.province.toLowerCase() === q);
  if (exact) return exact;
  return PROVINCE_MAP.find(e =>
    q.startsWith(e.province.toLowerCase()) ||
    e.province.toLowerCase().startsWith(q)
  ) ?? null;
}

// ── Main quote function ────────────────────────────────────────────────────────

export function computeQuote(
  mode: "sea" | "air",
  originKey: string,
  sizeId: string,
  qty: number,
  weightKg: number,
  dimsCm: [number, number, number],
  province: string,
): QuoteResult {
  const lines: QuoteLine[] = [];
  const [dimL, dimW, dimH] = dimsCm;
  let originCurrency = "USD";
  let transitDays = "";
  const cbm = computeCbm(dimL, dimW, dimH);

  // ── Sea freight ──────────────────────────────────────────────────────────────
  if (mode === "sea") {
    const row = SEA_CARGO.find(r => r.origin === originKey);
    if (row && qty > 0) {
      originCurrency = row.currency;
      transitDays = row.transit_days;
      const unitPrice = row.prices[sizeId] ?? 0;
      const total = unitPrice * qty;
      const sizeName = BOX_SIZES.find(s => s.id === sizeId)?.name ?? sizeId;
      lines.push({
        label: `Sea Freight — ${sizeName} × ${qty}`,
        amount: total,
        currency: originCurrency,
        note: row.origin,
        component: "sea",
      });
    }
  }

  // ── Air freight ───────────────────────────────────────────────────────────────
  if (mode === "air") {
    const zone = AIR_ZONES.find(z => z.zone_name === originKey);
    if (zone) {
      originCurrency = zone.currency;
      transitDays = zone.transit_days;
      const volKg = (dimL > 0 && dimW > 0 && dimH > 0)
        ? (dimL * dimW * dimH) / zone.volumetric_divisor
        : 0;
      const chargeKg = Math.max(zone.min_weight_kg, weightKg, volKg);

      const freight = chargeKg * zone.rate_per_kg;
      lines.push({
        label: `Air Freight — ${chargeKg.toFixed(1)} kg chargeable`,
        amount: parseFloat(freight.toFixed(2)),
        currency: originCurrency,
        note: volKg > weightKg
          ? `volumetric ${volKg.toFixed(1)} kg > actual ${weightKg} kg`
          : `actual weight ${weightKg} kg`,
        component: "air",
      });

      const fsc = parseFloat((freight * zone.fuel_surcharge_pct).toFixed(2));
      if (fsc > 0) {
        lines.push({
          label: `Fuel Surcharge (${Math.round(zone.fuel_surcharge_pct * 100)}%)`,
          amount: fsc, currency: originCurrency, component: "air",
        });
      }
      if (zone.awb_fee > 0) lines.push({ label: "AWB Fee", amount: zone.awb_fee, currency: originCurrency, component: "air" });
      if (zone.thc > 0) lines.push({ label: "Terminal Handling Charge", amount: zone.thc, currency: originCurrency, component: "air" });
      if (zone.customs > 0) lines.push({ label: "PH Customs Clearance", amount: zone.customs, currency: originCurrency, component: "air" });
    }
  }

  // ── PH local delivery ────────────────────────────────────────────────────────
  const resolved = resolveProvince(province);
  if (resolved) {
    const zone = PH_ZONES.find(z => z.zone_code === resolved.zone_code);
    if (zone && qty > 0) {
      const phpPrice = (zone.prices[sizeId] ?? 0) * qty;
      const converted = phpToOrigin(phpPrice, originCurrency);
      const sizeName = BOX_SIZES.find(s => s.id === sizeId)?.name ?? sizeId;
      lines.push({
        label: `PH Delivery (${zone.zone_code}) — ${sizeName} × ${qty}`,
        amount: converted,
        currency: originCurrency,
        note: `${zone.zone_name} · ${zone.transit_days} · ₱${phpPrice.toLocaleString()} converted`,
        component: "ph_delivery",
      });
      if (!transitDays) transitDays = zone.transit_days;
    }
  }

  const total = parseFloat(lines.reduce((s, l) => s + l.amount, 0).toFixed(2));

  return { lines, total_origin_currency: total, origin_currency: originCurrency, cbm, transit_days: transitDays };
}

// ── Rate data ──────────────────────────────────────────────────────────────────

export const BOX_SIZES: BoxSize[] = [
  { id: "bulilit", name: "Bulilit", dimensions: "30×25×25 cm", dims_cm: [30, 25, 25], max_weight_kg: 10, cbm: computeCbm(30, 25, 25), emoji: "📦" },
  { id: "small",   name: "Small",   dimensions: "41×30×30 cm", dims_cm: [41, 30, 30], max_weight_kg: 15, cbm: computeCbm(41, 30, 30), emoji: "📦" },
  { id: "medium",  name: "Medium",  dimensions: "46×36×36 cm", dims_cm: [46, 36, 36], max_weight_kg: 18, cbm: computeCbm(46, 36, 36), emoji: "📦" },
  { id: "large",   name: "Large",   dimensions: "51×41×41 cm", dims_cm: [51, 41, 41], max_weight_kg: 20, cbm: computeCbm(51, 41, 41), emoji: "📦" },
  { id: "xl",      name: "XL",      dimensions: "61×46×46 cm", dims_cm: [61, 46, 46], max_weight_kg: 25, cbm: computeCbm(61, 46, 46), emoji: "📦" },
  { id: "jumbo",   name: "Jumbo",   dimensions: "61×61×61 cm", dims_cm: [61, 61, 61], max_weight_kg: 30, cbm: computeCbm(61, 61, 61), emoji: "📦" },
];

function p(xl: number, jumbo: number, large: number, small: number): Record<string, number> {
  return {
    xl, jumbo, large, small,
    medium:  Math.round((large + small) / 2 * 0.97),
    bulilit: Math.round(small * 0.79),
  };
}

export const SEA_CARGO: SeaCargoRate[] = [
  { origin: "USA West Coast (CA, WA, OR)",          currency: "USD", transit_days: "30–45 days", prices: p(120, 150, 95,  70)  },
  { origin: "USA East Coast (NY, NJ, CT, MA)",      currency: "USD", transit_days: "40–55 days", prices: p(150, 185, 118, 88)  },
  { origin: "USA South & Central (TX, FL, IL, GA)", currency: "USD", transit_days: "40–55 days", prices: p(140, 175, 110, 82)  },
  { origin: "Hawaii",                               currency: "USD", transit_days: "25–35 days", prices: p(110, 138, 88,  65)  },
  { origin: "Alaska",                               currency: "USD", transit_days: "45–60 days", prices: p(170, 210, 135, 100) },
  { origin: "Guam / Saipan (CNMI)",                 currency: "USD", transit_days: "14–21 days", prices: p(86,  108, 68,  51)  },
  { origin: "Canada — British Columbia (Vancouver)", currency: "CAD", transit_days: "35–48 days", prices: p(165, 205, 130, 96)  },
  { origin: "Canada — Alberta (Calgary / Edmonton)", currency: "CAD", transit_days: "38–52 days", prices: p(170, 212, 135, 100) },
  { origin: "Canada — Ontario / Quebec",             currency: "CAD", transit_days: "42–56 days", prices: p(180, 225, 143, 106) },
  { origin: "United Kingdom — London",              currency: "GBP", transit_days: "38–55 days", prices: p(110, 138, 87,  65)  },
  { origin: "Ireland — Dublin",                     currency: "EUR", transit_days: "40–55 days", prices: p(130, 163, 103, 76)  },
  { origin: "Italy — Rome / Milan",                 currency: "EUR", transit_days: "38–52 days", prices: p(128, 160, 101, 75)  },
  { origin: "Spain — Madrid / Barcelona",           currency: "EUR", transit_days: "38–52 days", prices: p(126, 158, 100, 74)  },
  { origin: "Germany — Frankfurt / Munich",         currency: "EUR", transit_days: "40–55 days", prices: p(129, 161, 102, 75)  },
  { origin: "France — Paris",                       currency: "EUR", transit_days: "40–55 days", prices: p(128, 160, 101, 75)  },
  { origin: "Netherlands / Belgium — Amsterdam",    currency: "EUR", transit_days: "40–55 days", prices: p(128, 160, 101, 75)  },
  { origin: "Norway — Oslo",                        currency: "NOK", transit_days: "42–58 days", prices: p(1550, 1940, 1230, 910) },
  { origin: "Sweden / Denmark",                     currency: "SEK", transit_days: "42–58 days", prices: p(1650, 2060, 1300, 960) },
  { origin: "Switzerland — Zurich / Geneva",        currency: "EUR", transit_days: "42–58 days", prices: p(138, 173, 110, 81)  },
  { origin: "Greece / Cyprus",                      currency: "EUR", transit_days: "45–60 days", prices: p(133, 166, 105, 78)  },
  { origin: "UAE — Dubai / Abu Dhabi",              currency: "AED", transit_days: "18–28 days", prices: p(368, 460, 294, 213) },
  { origin: "Saudi Arabia — Riyadh",                currency: "SAR", transit_days: "20–30 days", prices: p(383, 480, 308, 225) },
  { origin: "Saudi Arabia — Jeddah / Dammam",       currency: "SAR", transit_days: "20–30 days", prices: p(390, 488, 311, 229) },
  { origin: "Qatar — Doha",                         currency: "QAR", transit_days: "20–30 days", prices: p(357, 444, 284, 208) },
  { origin: "Kuwait",                               currency: "USD", transit_days: "20–30 days", prices: p(99,  124, 79,  58)  },
  { origin: "Bahrain",                              currency: "USD", transit_days: "20–30 days", prices: p(96,  120, 76,  56)  },
  { origin: "Oman — Muscat",                        currency: "USD", transit_days: "20–30 days", prices: p(96,  120, 76,  56)  },
  { origin: "Israel",                               currency: "USD", transit_days: "28–40 days", prices: p(116, 145, 92,  68)  },
  { origin: "Australia — Sydney / Melbourne",       currency: "AUD", transit_days: "22–32 days", prices: p(163, 204, 130, 95)  },
  { origin: "Australia — Perth",                    currency: "AUD", transit_days: "20–28 days", prices: p(154, 193, 124, 91)  },
  { origin: "Australia — Brisbane",                 currency: "AUD", transit_days: "22–32 days", prices: p(160, 200, 127, 94)  },
  { origin: "New Zealand — Auckland",               currency: "NZD", transit_days: "25–35 days", prices: p(190, 237, 150, 110) },
  { origin: "Japan — Tokyo / Osaka",                currency: "JPY", transit_days: "10–18 days", prices: p(11900, 14900, 9500, 7000) },
  { origin: "South Korea — Seoul / Busan",          currency: "KRW", transit_days: "10–18 days", prices: p(100000, 125000, 79000, 58000) },
  { origin: "Hong Kong",                            currency: "HKD", transit_days: "7–12 days",  prices: p(500, 625, 398, 297)  },
  { origin: "Singapore",                            currency: "SGD", transit_days: "7–14 days",  prices: p(92,  115, 73,  54)   },
  { origin: "Taiwan — Taipei",                      currency: "USD", transit_days: "8–14 days",  prices: p(70,  88,  56,  41)   },
  { origin: "Macau",                                currency: "HKD", transit_days: "7–12 days",  prices: p(484, 606, 390, 289)  },
];

export const AIR_ZONES: AirCargoZone[] = [
  { zone_name: "Zone 1 — ASEAN / NE Asia",          currency: "USD", origins: "HK, Macau, Singapore, Japan, South Korea, Taiwan, Guam", rate_per_kg: 5.50,  fuel_surcharge_pct: 0.18, awb_fee: 25, thc: 15, customs: 20, min_weight_kg: 5, volumetric_divisor: 5000, transit_days: "2–5 days"  },
  { zone_name: "Zone 2 — Australia / NZ",            currency: "USD", origins: "Australia (all cities), New Zealand",                   rate_per_kg: 7.00,  fuel_surcharge_pct: 0.18, awb_fee: 25, thc: 15, customs: 20, min_weight_kg: 5, volumetric_divisor: 5000, transit_days: "4–8 days"  },
  { zone_name: "Zone 3 — Middle East",               currency: "USD", origins: "UAE, Saudi Arabia, Qatar, Kuwait, Bahrain, Oman, Israel", rate_per_kg: 8.00, fuel_surcharge_pct: 0.18, awb_fee: 25, thc: 15, customs: 20, min_weight_kg: 5, volumetric_divisor: 5000, transit_days: "5–9 days"  },
  { zone_name: "Zone 4 — Europe",                    currency: "USD", origins: "UK, Ireland, Germany, France, Netherlands, Norway, Sweden, Switzerland", rate_per_kg: 10.50, fuel_surcharge_pct: 0.18, awb_fee: 25, thc: 15, customs: 20, min_weight_kg: 5, volumetric_divisor: 5000, transit_days: "7–12 days" },
  { zone_name: "Zone 5 — N. America West",           currency: "USD", origins: "USA West Coast (CA, WA, OR), Hawaii",                   rate_per_kg: 12.00, fuel_surcharge_pct: 0.18, awb_fee: 25, thc: 15, customs: 20, min_weight_kg: 5, volumetric_divisor: 5000, transit_days: "7–12 days" },
  { zone_name: "Zone 6 — N. America East / Central", currency: "USD", origins: "USA East, South & Central, Alaska, Canada",             rate_per_kg: 13.50, fuel_surcharge_pct: 0.18, awb_fee: 25, thc: 15, customs: 20, min_weight_kg: 5, volumetric_divisor: 5000, transit_days: "8–14 days" },
  { zone_name: "Zone 7 — Others",                    currency: "USD", origins: "South Africa, Papua New Guinea, Latin America, others", rate_per_kg: 15.00, fuel_surcharge_pct: 0.18, awb_fee: 25, thc: 15, customs: 20, min_weight_kg: 5, volumetric_divisor: 5000, transit_days: "10–21 days" },
];

function ph(xl: number, jumbo: number, large: number, small: number): Record<string, number> {
  return {
    xl, jumbo, large, small,
    medium:  Math.max(100, Math.round((large + small) / 2 * 0.95)),
    bulilit: Math.max(100, Math.round(small * 0.75)),
  };
}

export const PH_ZONES: PhDeliveryZone[] = [
  { zone_code: "Zone 1A", zone_name: "Metro Manila / NCR",              coverage: "Makati, QC, Manila, Pasig, Taguig, Pasay, Parañaque, Caloocan, Marikina…", prices: ph(850,  1000, 650,  450),  transit_days: "1–2 days" },
  { zone_code: "Zone 2A", zone_name: "Region III — Bulacan / Pampanga", coverage: "Bulacan, Pampanga, Tarlac, Nueva Ecija",                                   prices: ph(1100, 1300, 850,  620),  transit_days: "2–3 days" },
  { zone_code: "Zone 2B", zone_name: "CALABARZON",                      coverage: "Cavite, Laguna, Batangas, Rizal",                                          prices: ph(1100, 1300, 850,  620),  transit_days: "2–3 days" },
  { zone_code: "Zone 2C", zone_name: "Quezon Province / Lucena",        coverage: "Quezon Province, Lucena City",                                             prices: ph(1250, 1550, 1000, 750),  transit_days: "3–4 days" },
  { zone_code: "Zone 3A", zone_name: "Ilocos / La Union / Pangasinan",  coverage: "Pangasinan, La Union, Zambales, Ilocos Norte, Ilocos Sur",                 prices: ph(1450, 1800, 1100, 850),  transit_days: "3–5 days" },
  { zone_code: "Zone 3B", zone_name: "Cagayan Valley",                  coverage: "Cagayan, Isabela, Nueva Vizcaya, Quirino",                                 prices: ph(1550, 2000, 1250, 900),  transit_days: "4–6 days" },
  { zone_code: "Zone 3C", zone_name: "CAR — Baguio / Cordillera",       coverage: "Benguet (Baguio), Mountain Province, Ifugao, Kalinga, Abra",               prices: ph(1700, 2100, 1350, 1000), transit_days: "4–6 days" },
  { zone_code: "Zone 4A", zone_name: "Bicol — Camarines / Naga",        coverage: "Camarines Norte, Camarines Sur, Naga City",                                prices: ph(1550, 2000, 1250, 900),  transit_days: "4–6 days" },
  { zone_code: "Zone 4B", zone_name: "Bicol — Albay / Sorsogon",        coverage: "Albay (Legazpi), Sorsogon, Catanduanes, Masbate",                          prices: ph(1800, 2250, 1400, 1000), transit_days: "5–7 days" },
  { zone_code: "Zone 4C", zone_name: "MIMAROPA",                        coverage: "Marinduque, Occ. Mindoro, Or. Mindoro, Romblon",                           prices: ph(1900, 2350, 1500, 1100), transit_days: "5–7 days" },
  { zone_code: "Zone 4D", zone_name: "Palawan — Puerto Princesa",       coverage: "Puerto Princesa City and Palawan main island",                             prices: ph(2450, 3100, 1950, 1450), transit_days: "7–10 days" },
  { zone_code: "Zone 5A", zone_name: "Metro Cebu",                      coverage: "Cebu City, Mandaue, Lapu-Lapu, Talisay",                                   prices: ph(1350, 1700, 1050, 800),  transit_days: "3–5 days" },
  { zone_code: "Zone 5B", zone_name: "Cebu Province",                   coverage: "Cebu province municipalities",                                             prices: ph(1700, 2100, 1350, 1000), transit_days: "5–7 days" },
  { zone_code: "Zone 5C", zone_name: "Iloilo / Bacolod City",           coverage: "Iloilo City, Bacolod City",                                               prices: ph(1550, 2000, 1250, 900),  transit_days: "4–6 days" },
  { zone_code: "Zone 5D", zone_name: "Western Visayas — Provinces",     coverage: "Iloilo Province, Negros Occidental",                                      prices: ph(1900, 2350, 1500, 1100), transit_days: "5–7 days" },
  { zone_code: "Zone 5E", zone_name: "Negros Oriental / Bohol",         coverage: "Negros Oriental, Bohol (Tagbilaran), Siquijor",                            prices: ph(2000, 2500, 1550, 1200), transit_days: "5–8 days" },
  { zone_code: "Zone 5F", zone_name: "Eastern Visayas — Leyte / Samar", coverage: "Leyte (Tacloban), Samar, Eastern Samar",                                  prices: ph(2100, 2700, 1700, 1250), transit_days: "6–9 days" },
  { zone_code: "Zone 5G", zone_name: "Aklan / Antique / Capiz",         coverage: "Capiz, Aklan (Boracay), Antique",                                         prices: ph(2000, 2500, 1550, 1200), transit_days: "5–8 days" },
  { zone_code: "Zone 6A", zone_name: "Metro Davao",                     coverage: "Davao City and Metro Davao",                                              prices: ph(1800, 2250, 1400, 1000), transit_days: "5–7 days" },
  { zone_code: "Zone 6B", zone_name: "Davao Region — Provinces",        coverage: "Davao del Norte, Davao Oriental, ComVal",                                 prices: ph(2100, 2700, 1700, 1250), transit_days: "6–9 days" },
  { zone_code: "Zone 6C", zone_name: "Cagayan de Oro / Bukidnon",       coverage: "Cagayan de Oro, Misamis Oriental, Bukidnon",                              prices: ph(1900, 2350, 1500, 1100), transit_days: "5–8 days" },
  { zone_code: "Zone 6D", zone_name: "GenSan / Sarangani / S. Cotabato",coverage: "General Santos City, Sarangani, South Cotabato",                          prices: ph(2000, 2500, 1550, 1200), transit_days: "6–9 days" },
  { zone_code: "Zone 6E", zone_name: "Zamboanga",                       coverage: "Zamboanga City, Zamboanga del Sur, Zamboanga del Norte",                  prices: ph(2350, 2900, 1850, 1350), transit_days: "7–10 days" },
  { zone_code: "Zone 6F", zone_name: "Lanao / Iligan — BARMM",          coverage: "Lanao del Norte, Lanao del Sur, Iligan City",                             prices: ph(2350, 2900, 1850, 1350), transit_days: "7–10 days" },
  { zone_code: "Zone 6G", zone_name: "Cotabato / Maguindanao — BARMM",  coverage: "Cotabato City, Sultan Kudarat, Maguindanao",                              prices: ph(2450, 3100, 1950, 1450), transit_days: "7–11 days" },
  { zone_code: "Zone 6H", zone_name: "Caraga — Surigao / Agusan",       coverage: "Surigao del Norte, Surigao del Sur, Agusan del Norte, Agusan del Sur",    prices: ph(2450, 3100, 1950, 1450), transit_days: "8–12 days" },
  { zone_code: "Zone 7A", zone_name: "Sulu / Tawi-Tawi / Basilan",      coverage: "Sulu, Tawi-Tawi, Basilan",                                               prices: ph(3600, 4500, 2850, 2150), transit_days: "12–18 days" },
  { zone_code: "Zone 7B", zone_name: "Batanes",                         coverage: "Batan Island and Batanes group",                                          prices: ph(4050, 5050, 3200, 2350), transit_days: "14–21 days" },
];

export const PROVINCE_MAP: ProvinceEntry[] = [
  // NCR
  { province: "Metro Manila",   zone_code: "Zone 1A" }, { province: "Manila",        zone_code: "Zone 1A" },
  { province: "Quezon City",    zone_code: "Zone 1A" }, { province: "Makati",        zone_code: "Zone 1A" },
  { province: "Pasig",          zone_code: "Zone 1A" }, { province: "Taguig",        zone_code: "Zone 1A" },
  { province: "Pasay",          zone_code: "Zone 1A" }, { province: "Parañaque",     zone_code: "Zone 1A" },
  { province: "Caloocan",       zone_code: "Zone 1A" }, { province: "Marikina",      zone_code: "Zone 1A" },
  { province: "Malabon",        zone_code: "Zone 1A" }, { province: "Valenzuela",    zone_code: "Zone 1A" },
  { province: "Muntinlupa",     zone_code: "Zone 1A" }, { province: "Las Pinas",     zone_code: "Zone 1A" },
  { province: "NCR",            zone_code: "Zone 1A" },
  // Luzon
  { province: "Bulacan",        zone_code: "Zone 2A" }, { province: "Pampanga",      zone_code: "Zone 2A" },
  { province: "Tarlac",         zone_code: "Zone 2A" }, { province: "Nueva Ecija",   zone_code: "Zone 2A" },
  { province: "Cavite",         zone_code: "Zone 2B" }, { province: "Laguna",        zone_code: "Zone 2B" },
  { province: "Batangas",       zone_code: "Zone 2B" }, { province: "Rizal",         zone_code: "Zone 2B" },
  { province: "Quezon Province",zone_code: "Zone 2C" }, { province: "Lucena",        zone_code: "Zone 2C" },
  { province: "Pangasinan",     zone_code: "Zone 3A" }, { province: "La Union",      zone_code: "Zone 3A" },
  { province: "Zambales",       zone_code: "Zone 3A" }, { province: "Ilocos Norte",  zone_code: "Zone 3A" },
  { province: "Ilocos Sur",     zone_code: "Zone 3A" }, { province: "Olongapo",      zone_code: "Zone 3A" },
  { province: "Cagayan",        zone_code: "Zone 3B" }, { province: "Isabela",       zone_code: "Zone 3B" },
  { province: "Nueva Vizcaya",  zone_code: "Zone 3B" }, { province: "Quirino",       zone_code: "Zone 3B" },
  { province: "Baguio",         zone_code: "Zone 3C" }, { province: "Benguet",       zone_code: "Zone 3C" },
  { province: "Ifugao",         zone_code: "Zone 3C" }, { province: "Kalinga",       zone_code: "Zone 3C" },
  { province: "Abra",           zone_code: "Zone 3C" }, { province: "Mountain Province", zone_code: "Zone 3C" },
  { province: "Camarines Norte",zone_code: "Zone 4A" }, { province: "Camarines Sur", zone_code: "Zone 4A" },
  { province: "Naga",           zone_code: "Zone 4A" }, { province: "Albay",         zone_code: "Zone 4B" },
  { province: "Sorsogon",       zone_code: "Zone 4B" }, { province: "Catanduanes",   zone_code: "Zone 4B" },
  { province: "Masbate",        zone_code: "Zone 4B" }, { province: "Legazpi",       zone_code: "Zone 4B" },
  { province: "Marinduque",     zone_code: "Zone 4C" }, { province: "Mindoro",       zone_code: "Zone 4C" },
  { province: "Romblon",        zone_code: "Zone 4C" }, { province: "Palawan",       zone_code: "Zone 4D" },
  { province: "Puerto Princesa",zone_code: "Zone 4D" },
  // Visayas
  { province: "Cebu",           zone_code: "Zone 5A" }, { province: "Cebu City",     zone_code: "Zone 5A" },
  { province: "Mandaue",        zone_code: "Zone 5A" }, { province: "Lapu-Lapu",     zone_code: "Zone 5A" },
  { province: "Iloilo",         zone_code: "Zone 5C" }, { province: "Iloilo City",   zone_code: "Zone 5C" },
  { province: "Bacolod",        zone_code: "Zone 5C" }, { province: "Negros Occidental", zone_code: "Zone 5D" },
  { province: "Negros Oriental",zone_code: "Zone 5E" }, { province: "Bohol",         zone_code: "Zone 5E" },
  { province: "Tagbilaran",     zone_code: "Zone 5E" }, { province: "Siquijor",      zone_code: "Zone 5E" },
  { province: "Leyte",          zone_code: "Zone 5F" }, { province: "Tacloban",      zone_code: "Zone 5F" },
  { province: "Samar",          zone_code: "Zone 5F" }, { province: "Aklan",         zone_code: "Zone 5G" },
  { province: "Boracay",        zone_code: "Zone 5G" }, { province: "Capiz",         zone_code: "Zone 5G" },
  { province: "Antique",        zone_code: "Zone 5G" },
  // Mindanao
  { province: "Davao",          zone_code: "Zone 6A" }, { province: "Davao City",    zone_code: "Zone 6A" },
  { province: "Davao del Norte",zone_code: "Zone 6B" }, { province: "Davao Oriental",zone_code: "Zone 6B" },
  { province: "Cagayan de Oro", zone_code: "Zone 6C" }, { province: "Bukidnon",      zone_code: "Zone 6C" },
  { province: "Misamis Oriental",zone_code:"Zone 6C" }, { province: "General Santos", zone_code: "Zone 6D" },
  { province: "GenSan",         zone_code: "Zone 6D" }, { province: "Sarangani",     zone_code: "Zone 6D" },
  { province: "South Cotabato", zone_code: "Zone 6D" }, { province: "Zamboanga",     zone_code: "Zone 6E" },
  { province: "Lanao del Norte",zone_code: "Zone 6F" }, { province: "Lanao del Sur", zone_code: "Zone 6F" },
  { province: "Iligan",         zone_code: "Zone 6F" }, { province: "Cotabato",      zone_code: "Zone 6G" },
  { province: "Maguindanao",    zone_code: "Zone 6G" }, { province: "Surigao",       zone_code: "Zone 6H" },
  { province: "Agusan del Norte",zone_code:"Zone 6H" }, { province: "Agusan del Sur",zone_code: "Zone 6H" },
  { province: "Sulu",           zone_code: "Zone 7A" }, { province: "Tawi-Tawi",     zone_code: "Zone 7A" },
  { province: "Basilan",        zone_code: "Zone 7A" }, { province: "Batanes",       zone_code: "Zone 7B" },
];

/** Currency display helpers */
export const CURRENCY_SYMBOL: Record<string, string> = {
  USD: "$", CAD: "CA$", GBP: "£", EUR: "€", AUD: "A$", NZD: "NZ$",
  JPY: "¥", KRW: "₩",  HKD: "HK$", SGD: "S$", AED: "د.إ", SAR: "﷼",
  QAR: "QR", NOK: "kr", SEK: "kr",
};

export function fmtAmount(amount: number, currency: string): string {
  const sym = CURRENCY_SYMBOL[currency] ?? currency + " ";
  if (currency === "JPY" || currency === "KRW") {
    return `${sym}${Math.round(amount).toLocaleString()}`;
  }
  return `${sym}${amount.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 })}`;
}
