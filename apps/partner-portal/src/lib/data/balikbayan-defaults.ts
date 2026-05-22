/**
 * Default Balikbayan rate tables.  Loaded when a carrier has not yet
 * customised their rates (backend returns 404).
 *
 * Sea cargo: each row carries its native origin currency (USD, AED, SAR, etc.)
 * PH Delivery: currency set via ph_delivery_currency (default: "PHP")
 * All other sections: currency field defaults to USD; partners update as needed
 * Province zone map: comprehensive PH province → zone_code lookup (editable by partner)
 */
import type {
  AirCargoFixed,
  AirCargoZone,
  AddOnService,
  BalikbayanRates,
  BoxBundle,
  BoxSize,
  PhDeliveryZone,
  PhProvinceZoneEntry,
  SeaCargoRate,
  VolumetricGroup,
} from "@/lib/api/balikbayan-rates";

// ── Box sizes ──────────────────────────────────────────────────────────────────

export const DEFAULT_BOX_SIZES: BoxSize[] = [
  { id: "xl",      name: "XL",      dimensions: "61×46×46 cm", max_weight_kg: 25, sort_order: 0 },
  { id: "jumbo",   name: "Jumbo",   dimensions: "61×61×61 cm", max_weight_kg: 30, sort_order: 1 },
  { id: "large",   name: "Large",   dimensions: "51×41×41 cm", max_weight_kg: 20, sort_order: 2 },
  { id: "medium",  name: "Medium",  dimensions: "46×36×36 cm", max_weight_kg: 18, sort_order: 3 },
  { id: "small",   name: "Small",   dimensions: "41×30×30 cm", max_weight_kg: 15, sort_order: 4 },
  { id: "bulilit", name: "Bulilit", dimensions: "30×25×25 cm", max_weight_kg: 10, sort_order: 5 },
];

// Helper — build a prices map for all 6 default sizes.
function p(
  xl: number, jumbo: number, large: number, small: number,
  medium = Math.round((large + small) / 2 * 0.97),
  bulilit = Math.round(small * 0.79),
): Record<string, number> {
  return { xl, jumbo, large, medium, small, bulilit };
}

// ── Sea cargo — per-origin native currency ─────────────────────────────────────

export const DEFAULT_SEA_CARGO: SeaCargoRate[] = [
  // USA (USD)
  { origin: "USA West Coast (CA, WA, OR)",          currency: "USD", transit_days: "30–45", prices: p(120, 150, 95,  70)  },
  { origin: "USA East Coast (NY, NJ, CT, MA)",      currency: "USD", transit_days: "40–55", prices: p(150, 185, 118, 88)  },
  { origin: "USA South & Central (TX, FL, IL, GA)", currency: "USD", transit_days: "40–55", prices: p(140, 175, 110, 82)  },
  { origin: "Hawaii",                               currency: "USD", transit_days: "25–35", prices: p(110, 138, 88,  65)  },
  { origin: "Alaska",                               currency: "USD", transit_days: "45–60", prices: p(170, 210, 135, 100) },
  { origin: "Guam / Saipan (CNMI)",                 currency: "USD", transit_days: "14–21", prices: p(86,  108, 68,  51)  },
  // Canada (CAD)
  { origin: "Canada — British Columbia (Vancouver)", currency: "CAD", transit_days: "35–48", prices: p(165, 205, 130, 96)  },
  { origin: "Canada — Alberta (Calgary / Edmonton)", currency: "CAD", transit_days: "38–52", prices: p(170, 212, 135, 100) },
  { origin: "Canada — Ontario / Quebec",             currency: "CAD", transit_days: "42–56", prices: p(180, 225, 143, 106) },
  // Europe
  { origin: "United Kingdom — London",              currency: "GBP", transit_days: "38–55", prices: p(110, 138, 87,  65)  },
  { origin: "Ireland — Dublin",                     currency: "EUR", transit_days: "40–55", prices: p(130, 163, 103, 76)  },
  { origin: "Italy — Rome / Milan",                 currency: "EUR", transit_days: "38–52", prices: p(128, 160, 101, 75)  },
  { origin: "Spain — Madrid / Barcelona",           currency: "EUR", transit_days: "38–52", prices: p(126, 158, 100, 74)  },
  { origin: "Germany — Frankfurt / Munich",         currency: "EUR", transit_days: "40–55", prices: p(129, 161, 102, 75)  },
  { origin: "France — Paris",                       currency: "EUR", transit_days: "40–55", prices: p(128, 160, 101, 75)  },
  { origin: "Netherlands / Belgium — Amsterdam",    currency: "EUR", transit_days: "40–55", prices: p(128, 160, 101, 75)  },
  { origin: "Norway — Oslo",                        currency: "NOK", transit_days: "42–58", prices: p(1550, 1940, 1230, 910) },
  { origin: "Sweden / Denmark",                     currency: "SEK", transit_days: "42–58", prices: p(1650, 2060, 1300, 960) },
  { origin: "Switzerland — Zurich / Geneva",        currency: "EUR", transit_days: "42–58", prices: p(138, 173, 110, 81)  },
  { origin: "Greece / Cyprus",                      currency: "EUR", transit_days: "45–60", prices: p(133, 166, 105, 78)  },
  // Middle East
  { origin: "UAE — Dubai / Abu Dhabi",              currency: "AED", transit_days: "18–28", prices: p(368, 460, 294, 213) },
  { origin: "Saudi Arabia — Riyadh",                currency: "SAR", transit_days: "20–30", prices: p(383, 480, 308, 225) },
  { origin: "Saudi Arabia — Jeddah / Dammam",       currency: "SAR", transit_days: "20–30", prices: p(390, 488, 311, 229) },
  { origin: "Qatar — Doha",                         currency: "QAR", transit_days: "20–30", prices: p(357, 444, 284, 208) },
  { origin: "Kuwait",                               currency: "USD", transit_days: "20–30", prices: p(99,  124, 79,  58)  },
  { origin: "Bahrain",                              currency: "USD", transit_days: "20–30", prices: p(96,  120, 76,  56)  },
  { origin: "Oman — Muscat",                        currency: "USD", transit_days: "20–30", prices: p(96,  120, 76,  56)  },
  { origin: "Israel",                               currency: "USD", transit_days: "28–40", prices: p(116, 145, 92,  68)  },
  // Australia & NZ
  { origin: "Australia — Sydney / Melbourne",       currency: "AUD", transit_days: "22–32", prices: p(163, 204, 130, 95)  },
  { origin: "Australia — Perth",                    currency: "AUD", transit_days: "20–28", prices: p(154, 193, 124, 91)  },
  { origin: "Australia — Brisbane",                 currency: "AUD", transit_days: "22–32", prices: p(160, 200, 127, 94)  },
  { origin: "New Zealand — Auckland",               currency: "NZD", transit_days: "25–35", prices: p(190, 237, 150, 110) },
  // Asia
  { origin: "Japan — Tokyo / Osaka",                currency: "JPY", transit_days: "10–18", prices: p(11900, 14900, 9500, 7000) },
  { origin: "South Korea — Seoul / Busan",          currency: "KRW", transit_days: "10–18", prices: p(100000, 125000, 79000, 58000) },
  { origin: "Hong Kong",                            currency: "HKD", transit_days: "7–12",  prices: p(500, 625, 398, 297)  },
  { origin: "Singapore",                            currency: "SGD", transit_days: "7–14",  prices: p(92,  115, 73,  54)   },
  { origin: "Taiwan — Taipei",                      currency: "USD", transit_days: "8–14",  prices: p(70,  88,  56,  41)   },
  { origin: "Macau",                                currency: "HKD", transit_days: "7–12",  prices: p(484, 606, 390, 289)  },
];

// ── Air cargo — zones default to USD (internationally quoted) ──────────────────

export const DEFAULT_AIR_ZONES: AirCargoZone[] = [
  { zone_name: "Zone 1 — ASEAN / NE Asia",          currency: "USD", origins: "Hong Kong, Macau, Singapore, Japan, South Korea, Taiwan, Guam",               rate_per_kg_usd: 5.50,  distance_surcharge_per_km: 0.018, transit_days: "2–5"   },
  { zone_name: "Zone 2 — Australia / NZ",            currency: "USD", origins: "Australia (all cities), New Zealand",                                          rate_per_kg_usd: 7.00,  distance_surcharge_per_km: 0.022, transit_days: "4–8"   },
  { zone_name: "Zone 3 — Middle East",               currency: "USD", origins: "UAE, Saudi Arabia, Qatar, Kuwait, Bahrain, Oman, Israel",                      rate_per_kg_usd: 8.00,  distance_surcharge_per_km: 0.025, transit_days: "5–9"   },
  { zone_name: "Zone 4 — Europe",                    currency: "USD", origins: "UK, Ireland, Italy, Spain, Germany, France, Netherlands, Norway, Sweden, Switzerland", rate_per_kg_usd: 10.50, distance_surcharge_per_km: 0.030, transit_days: "7–12"  },
  { zone_name: "Zone 5 — N. America West",           currency: "USD", origins: "USA West Coast (CA, WA, OR), Hawaii",                                          rate_per_kg_usd: 12.00, distance_surcharge_per_km: 0.032, transit_days: "7–12"  },
  { zone_name: "Zone 6 — N. America East / Central", currency: "USD", origins: "USA East, South & Central, Alaska, Canada (all provinces)",                    rate_per_kg_usd: 13.50, distance_surcharge_per_km: 0.035, transit_days: "8–14"  },
  { zone_name: "Zone 7 — Others",                    currency: "USD", origins: "South Africa, Papua New Guinea, Latin America, other unlisted countries",       rate_per_kg_usd: 15.00, distance_surcharge_per_km: 0.040, transit_days: "10–21" },
];

export const DEFAULT_AIR_FIXED: AirCargoFixed = {
  currency: "USD",
  awb_fee_usd: 25,
  fuel_surcharge_pct: 0.18,
  thc_usd: 15,
  customs_clearance_usd: 20,
  min_weight_kg: 5,
  volumetric_divisor: 5000,
};

// ── PH delivery zones — always Philippine Peso (₱) ────────────────────────────

// Prices are in PHP. medium/bulilit auto-derived; floor at ₱100.
function ph(
  xl: number, jumbo: number, large: number, small: number,
  medium = Math.max(100, Math.round((large + small) / 2 * 0.95)),
  bulilit = Math.max(100, Math.round(small * 0.75)),
): Record<string, number> {
  return { xl, jumbo, large, medium, small, bulilit };
}

export const DEFAULT_PH_DELIVERY: PhDeliveryZone[] = [
  // NCR
  { zone_code: "Zone 1A", zone_name: "Metro Manila / NCR",              coverage: "All NCR cities & municipalities (Makati, QC, Manila, Pasig, Taguig, Pasay, Parañaque, Caloocan, Marikina, etc.)", prices: ph(850,  1000, 650,  450),  transit_days: "1–2 days"  },
  // Nearby Luzon
  { zone_code: "Zone 2A", zone_name: "Region III — Bulacan / Pampanga", coverage: "Bulacan, Pampanga, Tarlac, Nueva Ecija",                                                                            prices: ph(1100, 1300, 850,  620),  transit_days: "2–3 days"  },
  { zone_code: "Zone 2B", zone_name: "CALABARZON",                      coverage: "Cavite, Laguna, Batangas, Rizal (Region IV-A)",                                                                      prices: ph(1100, 1300, 850,  620),  transit_days: "2–3 days"  },
  { zone_code: "Zone 2C", zone_name: "Quezon Province / Lucena",        coverage: "Quezon Province, Lucena City",                                                                                       prices: ph(1250, 1550, 1000, 750),  transit_days: "3–4 days"  },
  // Northern Luzon
  { zone_code: "Zone 3A", zone_name: "Ilocos / La Union / Pangasinan",  coverage: "Pangasinan, La Union, Zambales, Olongapo, Ilocos Norte, Ilocos Sur",                                                prices: ph(1450, 1800, 1100, 850),  transit_days: "3–5 days"  },
  { zone_code: "Zone 3B", zone_name: "Cagayan Valley",                   coverage: "Cagayan, Isabela, Nueva Vizcaya, Quirino",                                                                          prices: ph(1550, 2000, 1250, 900),  transit_days: "4–6 days"  },
  { zone_code: "Zone 3C", zone_name: "CAR — Baguio / Cordillera",       coverage: "Benguet (Baguio), Mountain Province, Ifugao, Kalinga, Abra",                                                         prices: ph(1700, 2100, 1350, 1000), transit_days: "4–6 days"  },
  // Bicol / Southern Luzon
  { zone_code: "Zone 4A", zone_name: "Bicol — Camarines / Naga",        coverage: "Camarines Norte, Camarines Sur, Naga City",                                                                          prices: ph(1550, 2000, 1250, 900),  transit_days: "4–6 days"  },
  { zone_code: "Zone 4B", zone_name: "Bicol — Albay / Sorsogon",        coverage: "Albay (Legazpi), Sorsogon, Catanduanes, Masbate",                                                                    prices: ph(1800, 2250, 1400, 1000), transit_days: "5–7 days"  },
  { zone_code: "Zone 4C", zone_name: "MIMAROPA",                         coverage: "Marinduque, Occ. Mindoro, Or. Mindoro, Romblon",                                                                    prices: ph(1900, 2350, 1500, 1100), transit_days: "5–7 days"  },
  { zone_code: "Zone 4D", zone_name: "Palawan — Puerto Princesa",        coverage: "Puerto Princesa City and Palawan main island",                                                                       prices: ph(2450, 3100, 1950, 1450), transit_days: "7–10 days" },
  { zone_code: "Zone 4E", zone_name: "Palawan — Remote Islands",         coverage: "Coron, El Nido, Southern Palawan islands",                                                                          prices: ph(3150, 3900, 2500, 1850), transit_days: "10–15 days"},
  // Visayas
  { zone_code: "Zone 5A", zone_name: "Metro Cebu",                       coverage: "Cebu City, Mandaue, Lapu-Lapu, Talisay",                                                                            prices: ph(1350, 1700, 1050, 800),  transit_days: "3–5 days"  },
  { zone_code: "Zone 5B", zone_name: "Cebu Province",                    coverage: "Cebu province municipalities outside Metro Cebu",                                                                    prices: ph(1700, 2100, 1350, 1000), transit_days: "5–7 days"  },
  { zone_code: "Zone 5C", zone_name: "Iloilo / Bacolod City",            coverage: "Iloilo City, Bacolod City (metro areas)",                                                                           prices: ph(1550, 2000, 1250, 900),  transit_days: "4–6 days"  },
  { zone_code: "Zone 5D", zone_name: "Western Visayas — Provinces",      coverage: "Iloilo Province, Negros Occidental municipalities",                                                                  prices: ph(1900, 2350, 1500, 1100), transit_days: "5–7 days"  },
  { zone_code: "Zone 5E", zone_name: "Negros Oriental / Bohol",          coverage: "Negros Oriental, Bohol (Tagbilaran), Siquijor",                                                                      prices: ph(2000, 2500, 1550, 1200), transit_days: "5–8 days"  },
  { zone_code: "Zone 5F", zone_name: "Eastern Visayas — Leyte / Samar",  coverage: "Leyte (Tacloban), Samar, Eastern Samar, Northern Samar",                                                           prices: ph(2100, 2700, 1700, 1250), transit_days: "6–9 days"  },
  { zone_code: "Zone 5G", zone_name: "Aklan / Antique / Capiz",          coverage: "Capiz, Aklan (Boracay), Antique",                                                                                   prices: ph(2000, 2500, 1550, 1200), transit_days: "5–8 days"  },
  // Mindanao
  { zone_code: "Zone 6A", zone_name: "Metro Davao",                      coverage: "Davao City and Metro Davao (Davao del Sur)",                                                                         prices: ph(1800, 2250, 1400, 1000), transit_days: "5–7 days"  },
  { zone_code: "Zone 6B", zone_name: "Davao Region — Provinces",         coverage: "Davao del Norte, Davao Oriental, ComVal",                                                                           prices: ph(2100, 2700, 1700, 1250), transit_days: "6–9 days"  },
  { zone_code: "Zone 6C", zone_name: "Cagayan de Oro / Bukidnon",        coverage: "Cagayan de Oro, Misamis Oriental, Bukidnon",                                                                         prices: ph(1900, 2350, 1500, 1100), transit_days: "5–8 days"  },
  { zone_code: "Zone 6D", zone_name: "GenSan / Sarangani / S. Cotabato", coverage: "General Santos City, Sarangani, South Cotabato",                                                                    prices: ph(2000, 2500, 1550, 1200), transit_days: "6–9 days"  },
  { zone_code: "Zone 6E", zone_name: "Zamboanga",                         coverage: "Zamboanga City, Zamboanga del Sur, Zamboanga del Norte",                                                            prices: ph(2350, 2900, 1850, 1350), transit_days: "7–10 days" },
  { zone_code: "Zone 6F", zone_name: "Lanao / Iligan — BARMM",           coverage: "Lanao del Norte, Lanao del Sur, Iligan City",                                                                       prices: ph(2350, 2900, 1850, 1350), transit_days: "7–10 days" },
  { zone_code: "Zone 6G", zone_name: "Cotabato / Maguindanao — BARMM",   coverage: "Cotabato City, Sultan Kudarat, Maguindanao",                                                                        prices: ph(2450, 3100, 1950, 1450), transit_days: "7–11 days" },
  { zone_code: "Zone 6H", zone_name: "Caraga — Surigao / Agusan",        coverage: "Surigao del Norte, Surigao del Sur, Agusan del Norte, Agusan del Sur",                                              prices: ph(2450, 3100, 1950, 1450), transit_days: "8–12 days" },
  { zone_code: "Zone 6I", zone_name: "Camiguin / Dinagat",               coverage: "Camiguin Island, Dinagat Islands",                                                                                  prices: ph(2800, 3500, 2250, 1600), transit_days: "9–14 days" },
  // Remote Islands
  { zone_code: "Zone 7A", zone_name: "Sulu / Tawi-Tawi / Basilan",       coverage: "Sulu, Tawi-Tawi, Basilan (BARMM far south)",                                                                        prices: ph(3600, 4500, 2850, 2150), transit_days: "12–18 days"},
  { zone_code: "Zone 7B", zone_name: "Batanes",                           coverage: "Batan Island and Batanes group (northernmost PH)",                                                                  prices: ph(4050, 5050, 3200, 2350), transit_days: "14–21 days"},
];

// ── Volumetric groups — default USD; partners update to their billing currency ─

export const DEFAULT_VOLUMETRIC: VolumetricGroup[] = [
  { group: "Manila",   currency: "USD", divisor: 5000, base_rate_usd: 12, rate_per_cbm_usd: 85,  min_charge_usd: 12, surcharge_pct: 0    },
  { group: "Luzon",    currency: "USD", divisor: 5000, base_rate_usd: 15, rate_per_cbm_usd: 95,  min_charge_usd: 15, surcharge_pct: 0.05 },
  { group: "Visayas",  currency: "USD", divisor: 4500, base_rate_usd: 18, rate_per_cbm_usd: 110, min_charge_usd: 18, surcharge_pct: 0.08 },
  { group: "Mindanao", currency: "USD", divisor: 4500, base_rate_usd: 20, rate_per_cbm_usd: 120, min_charge_usd: 20, surcharge_pct: 0.10 },
  { group: "Islands",  currency: "USD", divisor: 4000, base_rate_usd: 28, rate_per_cbm_usd: 150, min_charge_usd: 28, surcharge_pct: 0.15 },
];

// ── Bundles ────────────────────────────────────────────────────────────────────

export const DEFAULT_BUNDLES: BoxBundle[] = [
  {
    id: "bundle-ofw-starter",
    name: "OFW Starter Bundle",
    description: "Perfect for first-time senders — covers the family essentials.",
    items: [{ size_id: "jumbo", quantity: 1 }, { size_id: "small", quantity: 1 }],
    currency: "USD",
    price_usd: 195,
    valid_for: "sea",
    notes: "1 Jumbo + 1 Small box. Sea freight only. PH local delivery billed separately per zone.",
  },
  {
    id: "bundle-family",
    name: "Family Balikbayan Bundle",
    description: "Two Jumbo boxes + one Large — enough for the whole barangay.",
    items: [{ size_id: "jumbo", quantity: 2 }, { size_id: "large", quantity: 1 }],
    currency: "USD",
    price_usd: 375,
    valid_for: "sea",
    notes: "2 Jumbo + 1 Large. Combined sea freight. Schedule one pickup for all three.",
  },
  {
    id: "bundle-padala-pamilya",
    name: "Padala Pamilya Value Pack",
    description: "Mix of sizes — great for sending gifts and everyday goods together.",
    items: [
      { size_id: "xl",     quantity: 1 },
      { size_id: "medium", quantity: 1 },
      { size_id: "small",  quantity: 2 },
    ],
    currency: "USD",
    price_usd: 290,
    valid_for: "both",
    notes: "1 XL + 1 Medium + 2 Small. Valid for sea and air cargo routes.",
  },
  {
    id: "bundle-pasalubong",
    name: "Pasalubong Bundle",
    description: "5 Bulilit boxes — ideal for pasalubong, chocolates, and small gifts.",
    items: [{ size_id: "bulilit", quantity: 5 }],
    currency: "USD",
    price_usd: 240,
    valid_for: "sea",
    notes: "5 Bulilit boxes shipped together. Maximum 10 kg per box.",
  },
];

// ── Add-ons ────────────────────────────────────────────────────────────────────

export const DEFAULT_ADDONS: AddOnService[] = [
  // Insurance
  { id: "ins-sea-basic",   name: "Basic All-Risk Sea",       category: "insurance", currency: "USD", rate: 0.015, rate_type: "percent", min_charge: 15, description: "Loss & damage in transit; excludes inherent vice, war" },
  { id: "ins-sea-ext",     name: "Extended All-Risk Sea",    category: "insurance", currency: "USD", rate: 0.025, rate_type: "percent", min_charge: 20, description: "Adds natural disaster, port flooding, pilferage" },
  { id: "ins-sea-prem",    name: "Premium All-Risk Sea",     category: "insurance", currency: "USD", rate: 0.030, rate_type: "percent", min_charge: 25, description: "Total + partial loss + delay compensation up to 10%" },
  { id: "ins-air-basic",   name: "Basic All-Risk Air",       category: "insurance", currency: "USD", rate: 0.010, rate_type: "percent", min_charge: 15, description: "Standard IATA air cargo conditions" },
  { id: "ins-air-ext",     name: "Extended All-Risk Air",    category: "insurance", currency: "USD", rate: 0.0175,rate_type: "percent", min_charge: 18, description: "Adds rough handling, delay reimbursement up to $200" },
  { id: "ins-high-value",  name: "High-Value Goods Rider",   category: "insurance", currency: "USD", rate: 0.005, rate_type: "percent", min_charge: 10, description: "Electronics, jewelry, branded goods" },
  // Crate
  { id: "crate-mini",      name: "Mini Crate (51×41×41 cm)",     category: "crate", currency: "USD", rate: 55,  rate_type: "fixed", min_charge: 55,  description: "Fits 1 × Small or Bulilit box" },
  { id: "crate-std",       name: "Standard Crate (61×51×51 cm)", category: "crate", currency: "USD", rate: 75,  rate_type: "fixed", min_charge: 75,  description: "Fits 1 × Medium or Large box" },
  { id: "crate-large",     name: "Large Crate (71×56×56 cm)",    category: "crate", currency: "USD", rate: 95,  rate_type: "fixed", min_charge: 95,  description: "Fits 1 × XL box" },
  { id: "crate-jumbo",     name: "Jumbo Crate (76×71×71 cm)",    category: "crate", currency: "USD", rate: 125, rate_type: "fixed", min_charge: 125, description: "Fits 1 × Jumbo box" },
  { id: "crate-dbl",       name: "Double-Wall Reinforced",        category: "crate", currency: "USD", rate: 30,  rate_type: "fixed", min_charge: 30,  description: "Add-on to any crate — extra fragile items, antiques" },
  // Packing
  { id: "pack-basic",      name: "Basic Seal",                category: "packing", currency: "USD", rate: 15, rate_type: "fixed", min_charge: 15, description: "Tape reinforcement, box sealing, label" },
  { id: "pack-std",        name: "Standard Pack",             category: "packing", currency: "USD", rate: 30, rate_type: "fixed", min_charge: 30, description: "Bubble wrap + kraft paper fill + tape seal" },
  { id: "pack-full",       name: "Full Professional Pack",    category: "packing", currency: "USD", rate: 55, rate_type: "fixed", min_charge: 55, description: "Individual wrap, foam padding, corner protectors" },
  { id: "pack-prem",       name: "Premium Fragile Pack",      category: "packing", currency: "USD", rate: 90, rate_type: "fixed", min_charge: 90, description: "Foam-in-place, custom inserts, moisture barrier" },
  { id: "pack-vacuum",     name: "Vacuum / Compression Bags", category: "packing", currency: "USD", rate: 20, rate_type: "fixed", min_charge: 20, description: "Space-saving vacuum seal for textiles and linens" },
  { id: "pack-rebox",      name: "Re-boxing Service",         category: "packing", currency: "USD", rate: 40, rate_type: "fixed", min_charge: 40, description: "Unpack & repack into standard Balikbayan box" },
  { id: "pack-inventory",  name: "Inventory & Photo Docs",    category: "packing", currency: "USD", rate: 25, rate_type: "fixed", min_charge: 25, description: "Full item-by-item photo record + itemized list" },
  // Surcharges
  { id: "sur-peak",        name: "Peak Season Surcharge",     category: "surcharge", currency: "USD", rate: 0.175,rate_type: "percent", min_charge: 0, description: "Oct 1 – Jan 31 (Christmas / Pasko). 15–20% of sea freight" },
  { id: "sur-overweight",  name: "Overweight (per kg)",       category: "surcharge", currency: "USD", rate: 3,   rate_type: "per_kg",  min_charge: 3, description: "Per kg over declared box weight limit" },
  { id: "sur-oversize",    name: "Oversize Box",              category: "surcharge", currency: "USD", rate: 35,  rate_type: "fixed",   min_charge: 35, description: "Any side exceeds 69 cm" },
  { id: "sur-storage",     name: "Storage / Demurrage",       category: "surcharge", currency: "USD", rate: 5,   rate_type: "per_day", min_charge: 5,  description: "Per box/day after 7 free days at origin warehouse" },
  { id: "sur-redelivery",  name: "Re-delivery Fee (PH)",      category: "surcharge", currency: "USD", rate: 8,   rate_type: "fixed",   min_charge: 8,  description: "Per failed delivery attempt after first (up to 3 attempts)" },
  { id: "sur-customs",     name: "Customs Clearance Assist",  category: "surcharge", currency: "USD", rate: 30,  rate_type: "fixed",   min_charge: 30, description: "PH BOC query support; excludes duties & taxes" },
];

// ── Province → Zone map ────────────────────────────────────────────────────────
// Comprehensive mapping of PH provinces and cities to delivery zone codes.
// Partners can add city-level overrides; lookup is case-insensitive, exact match
// first then prefix/substring fallback.

export const DEFAULT_PROVINCE_ZONE_MAP: PhProvinceZoneEntry[] = [
  // Zone 1A — Metro Manila / NCR
  { province: "Metro Manila",         zone_code: "Zone 1A" },
  { province: "Makati",               zone_code: "Zone 1A" },
  { province: "Makati City",          zone_code: "Zone 1A" },
  { province: "Quezon City",          zone_code: "Zone 1A" },
  { province: "Manila",               zone_code: "Zone 1A" },
  { province: "Pasig",                zone_code: "Zone 1A" },
  { province: "Taguig",               zone_code: "Zone 1A" },
  { province: "Parañaque",            zone_code: "Zone 1A" },
  { province: "Paranaque",            zone_code: "Zone 1A" },
  { province: "Caloocan",             zone_code: "Zone 1A" },
  { province: "Mandaluyong",          zone_code: "Zone 1A" },
  { province: "Marikina",             zone_code: "Zone 1A" },
  { province: "Muntinlupa",           zone_code: "Zone 1A" },
  { province: "Las Piñas",            zone_code: "Zone 1A" },
  { province: "Las Pinas",            zone_code: "Zone 1A" },
  { province: "Navotas",              zone_code: "Zone 1A" },
  { province: "Pasay",                zone_code: "Zone 1A" },
  { province: "San Juan",             zone_code: "Zone 1A" },
  { province: "Valenzuela",           zone_code: "Zone 1A" },
  { province: "Malabon",              zone_code: "Zone 1A" },
  { province: "Pateros",              zone_code: "Zone 1A" },
  // Zone 2A — Region III
  { province: "Bulacan",              zone_code: "Zone 2A" },
  { province: "Pampanga",             zone_code: "Zone 2A" },
  { province: "Angeles",              zone_code: "Zone 2A" },
  { province: "Angeles City",         zone_code: "Zone 2A" },
  { province: "Tarlac",               zone_code: "Zone 2A" },
  { province: "Nueva Ecija",          zone_code: "Zone 2A" },
  { province: "Cabanatuan",           zone_code: "Zone 2A" },
  { province: "Bataan",               zone_code: "Zone 2A" },
  { province: "Zambales",             zone_code: "Zone 2A" },
  { province: "Olongapo",             zone_code: "Zone 2A" },
  // Zone 2B — CALABARZON
  { province: "Cavite",               zone_code: "Zone 2B" },
  { province: "Laguna",               zone_code: "Zone 2B" },
  { province: "Batangas",             zone_code: "Zone 2B" },
  { province: "Rizal",                zone_code: "Zone 2B" },
  { province: "Antipolo",             zone_code: "Zone 2B" },
  // Zone 2C — Quezon Province
  { province: "Quezon",               zone_code: "Zone 2C" },
  { province: "Quezon Province",      zone_code: "Zone 2C" },
  { province: "Lucena",               zone_code: "Zone 2C" },
  { province: "Lucena City",          zone_code: "Zone 2C" },
  // Zone 3A — Ilocos / Pangasinan
  { province: "Pangasinan",           zone_code: "Zone 3A" },
  { province: "Dagupan",              zone_code: "Zone 3A" },
  { province: "La Union",             zone_code: "Zone 3A" },
  { province: "San Fernando",         zone_code: "Zone 3A" },
  { province: "Ilocos Norte",         zone_code: "Zone 3A" },
  { province: "Laoag",                zone_code: "Zone 3A" },
  { province: "Ilocos Sur",           zone_code: "Zone 3A" },
  { province: "Vigan",                zone_code: "Zone 3A" },
  // Zone 3B — Cagayan Valley
  { province: "Cagayan",              zone_code: "Zone 3B" },
  { province: "Tuguegarao",           zone_code: "Zone 3B" },
  { province: "Isabela",              zone_code: "Zone 3B" },
  { province: "Ilagan",               zone_code: "Zone 3B" },
  { province: "Nueva Vizcaya",        zone_code: "Zone 3B" },
  { province: "Quirino",              zone_code: "Zone 3B" },
  { province: "Aurora",               zone_code: "Zone 3B" },
  // Zone 3C — CAR / Cordillera
  { province: "Benguet",              zone_code: "Zone 3C" },
  { province: "Baguio",               zone_code: "Zone 3C" },
  { province: "Baguio City",          zone_code: "Zone 3C" },
  { province: "Mountain Province",    zone_code: "Zone 3C" },
  { province: "Ifugao",               zone_code: "Zone 3C" },
  { province: "Kalinga",              zone_code: "Zone 3C" },
  { province: "Abra",                 zone_code: "Zone 3C" },
  { province: "Apayao",               zone_code: "Zone 3C" },
  // Zone 4A — Camarines / Naga
  { province: "Camarines Norte",      zone_code: "Zone 4A" },
  { province: "Camarines Sur",        zone_code: "Zone 4A" },
  { province: "Naga",                 zone_code: "Zone 4A" },
  { province: "Naga City",            zone_code: "Zone 4A" },
  // Zone 4B — Albay / Sorsogon / Islands
  { province: "Albay",                zone_code: "Zone 4B" },
  { province: "Legazpi",              zone_code: "Zone 4B" },
  { province: "Legazpi City",         zone_code: "Zone 4B" },
  { province: "Sorsogon",             zone_code: "Zone 4B" },
  { province: "Catanduanes",          zone_code: "Zone 4B" },
  { province: "Masbate",              zone_code: "Zone 4B" },
  // Zone 4C — MIMAROPA
  { province: "Marinduque",           zone_code: "Zone 4C" },
  { province: "Occidental Mindoro",   zone_code: "Zone 4C" },
  { province: "Oriental Mindoro",     zone_code: "Zone 4C" },
  { province: "Calapan",              zone_code: "Zone 4C" },
  { province: "Romblon",              zone_code: "Zone 4C" },
  // Zone 4D — Palawan (main)
  { province: "Palawan",              zone_code: "Zone 4D" },
  { province: "Puerto Princesa",      zone_code: "Zone 4D" },
  // Zone 4E — Palawan remote
  { province: "Coron",                zone_code: "Zone 4E" },
  { province: "El Nido",              zone_code: "Zone 4E" },
  // Zone 5A — Metro Cebu
  { province: "Cebu City",            zone_code: "Zone 5A" },
  { province: "Mandaue",              zone_code: "Zone 5A" },
  { province: "Mandaue City",         zone_code: "Zone 5A" },
  { province: "Lapu-Lapu",            zone_code: "Zone 5A" },
  { province: "Lapu-Lapu City",       zone_code: "Zone 5A" },
  { province: "Talisay",              zone_code: "Zone 5A" },
  // Zone 5B — Cebu Province
  { province: "Cebu",                 zone_code: "Zone 5B" },
  // Zone 5C — Iloilo City / Bacolod City
  { province: "Iloilo City",          zone_code: "Zone 5C" },
  { province: "Bacolod",              zone_code: "Zone 5C" },
  { province: "Bacolod City",         zone_code: "Zone 5C" },
  // Zone 5D — Western Visayas Provinces
  { province: "Iloilo",               zone_code: "Zone 5D" },
  { province: "Negros Occidental",    zone_code: "Zone 5D" },
  // Zone 5E — Negros Oriental / Bohol / Siquijor
  { province: "Negros Oriental",      zone_code: "Zone 5E" },
  { province: "Dumaguete",            zone_code: "Zone 5E" },
  { province: "Dumaguete City",       zone_code: "Zone 5E" },
  { province: "Bohol",                zone_code: "Zone 5E" },
  { province: "Tagbilaran",           zone_code: "Zone 5E" },
  { province: "Siquijor",             zone_code: "Zone 5E" },
  // Zone 5F — Eastern Visayas
  { province: "Leyte",                zone_code: "Zone 5F" },
  { province: "Tacloban",             zone_code: "Zone 5F" },
  { province: "Tacloban City",        zone_code: "Zone 5F" },
  { province: "Samar",                zone_code: "Zone 5F" },
  { province: "Eastern Samar",        zone_code: "Zone 5F" },
  { province: "Northern Samar",       zone_code: "Zone 5F" },
  { province: "Southern Leyte",       zone_code: "Zone 5F" },
  { province: "Biliran",              zone_code: "Zone 5F" },
  // Zone 5G — Aklan / Antique / Capiz / Guimaras
  { province: "Aklan",                zone_code: "Zone 5G" },
  { province: "Kalibo",               zone_code: "Zone 5G" },
  { province: "Boracay",              zone_code: "Zone 5G" },
  { province: "Antique",              zone_code: "Zone 5G" },
  { province: "Capiz",                zone_code: "Zone 5G" },
  { province: "Roxas",                zone_code: "Zone 5G" },
  { province: "Roxas City",           zone_code: "Zone 5G" },
  { province: "Guimaras",             zone_code: "Zone 5G" },
  // Zone 6A — Metro Davao
  { province: "Davao City",           zone_code: "Zone 6A" },
  { province: "Davao del Sur",        zone_code: "Zone 6A" },
  // Zone 6B — Davao Region Provinces
  { province: "Davao del Norte",      zone_code: "Zone 6B" },
  { province: "Tagum",                zone_code: "Zone 6B" },
  { province: "Tagum City",           zone_code: "Zone 6B" },
  { province: "Davao Oriental",       zone_code: "Zone 6B" },
  { province: "Mati",                 zone_code: "Zone 6B" },
  { province: "Davao de Oro",         zone_code: "Zone 6B" },
  { province: "Compostela Valley",    zone_code: "Zone 6B" },
  { province: "Davao Occidental",     zone_code: "Zone 6B" },
  // Zone 6C — CDO / Bukidnon
  { province: "Cagayan de Oro",       zone_code: "Zone 6C" },
  { province: "Misamis Oriental",     zone_code: "Zone 6C" },
  { province: "Bukidnon",             zone_code: "Zone 6C" },
  { province: "Malaybalay",           zone_code: "Zone 6C" },
  { province: "Misamis Occidental",   zone_code: "Zone 6C" },
  { province: "Ozamiz",               zone_code: "Zone 6C" },
  // Zone 6D — GenSan / Sarangani / South Cotabato
  { province: "General Santos",       zone_code: "Zone 6D" },
  { province: "General Santos City",  zone_code: "Zone 6D" },
  { province: "Sarangani",            zone_code: "Zone 6D" },
  { province: "South Cotabato",       zone_code: "Zone 6D" },
  { province: "Koronadal",            zone_code: "Zone 6D" },
  // Zone 6E — Zamboanga
  { province: "Zamboanga City",       zone_code: "Zone 6E" },
  { province: "Zamboanga del Sur",    zone_code: "Zone 6E" },
  { province: "Zamboanga del Norte",  zone_code: "Zone 6E" },
  { province: "Dipolog",              zone_code: "Zone 6E" },
  { province: "Zamboanga Sibugay",    zone_code: "Zone 6E" },
  { province: "Ipil",                 zone_code: "Zone 6E" },
  // Zone 6F — Lanao / Iligan
  { province: "Lanao del Norte",      zone_code: "Zone 6F" },
  { province: "Iligan",               zone_code: "Zone 6F" },
  { province: "Iligan City",          zone_code: "Zone 6F" },
  { province: "Lanao del Sur",        zone_code: "Zone 6F" },
  { province: "Marawi",               zone_code: "Zone 6F" },
  // Zone 6G — Cotabato / Maguindanao / BARMM
  { province: "Cotabato City",        zone_code: "Zone 6G" },
  { province: "North Cotabato",       zone_code: "Zone 6G" },
  { province: "Sultan Kudarat",       zone_code: "Zone 6G" },
  { province: "Maguindanao",          zone_code: "Zone 6G" },
  { province: "Maguindanao del Norte",zone_code: "Zone 6G" },
  { province: "Maguindanao del Sur",  zone_code: "Zone 6G" },
  // Zone 6H — Caraga
  { province: "Surigao del Norte",    zone_code: "Zone 6H" },
  { province: "Surigao City",         zone_code: "Zone 6H" },
  { province: "Surigao del Sur",      zone_code: "Zone 6H" },
  { province: "Agusan del Norte",     zone_code: "Zone 6H" },
  { province: "Butuan",               zone_code: "Zone 6H" },
  { province: "Butuan City",          zone_code: "Zone 6H" },
  { province: "Agusan del Sur",       zone_code: "Zone 6H" },
  // Zone 6I — Camiguin / Dinagat
  { province: "Camiguin",             zone_code: "Zone 6I" },
  { province: "Dinagat Islands",      zone_code: "Zone 6I" },
  // Zone 7A — Sulu / Tawi-Tawi / Basilan
  { province: "Sulu",                 zone_code: "Zone 7A" },
  { province: "Jolo",                 zone_code: "Zone 7A" },
  { province: "Tawi-Tawi",            zone_code: "Zone 7A" },
  { province: "Bongao",               zone_code: "Zone 7A" },
  { province: "Basilan",              zone_code: "Zone 7A" },
  { province: "Isabela City",         zone_code: "Zone 7A" },
  // Zone 7B — Batanes (northernmost)
  { province: "Batanes",              zone_code: "Zone 7B" },
  { province: "Basco",                zone_code: "Zone 7B" },
];

// ── Root builder ───────────────────────────────────────────────────────────────

export function buildDefaultRates(carrierId: string): BalikbayanRates {
  return {
    carrier_id:           carrierId,
    box_sizes:            DEFAULT_BOX_SIZES,
    sea_cargo:            DEFAULT_SEA_CARGO,
    air_cargo_zones:      DEFAULT_AIR_ZONES,
    air_cargo_fixed:      DEFAULT_AIR_FIXED,
    ph_delivery_zones:    DEFAULT_PH_DELIVERY,
    ph_delivery_currency: "PHP",
    province_zone_map:    DEFAULT_PROVINCE_ZONE_MAP,
    volumetric_groups:    DEFAULT_VOLUMETRIC,
    bundles:              DEFAULT_BUNDLES,
    addons:               DEFAULT_ADDONS,
    updated_at:           new Date().toISOString(),
  };
}
