/**
 * Default Balikbayan rate tables — seeded from the published Atlas Cargo
 * rate card.  Loaded when a carrier has not yet customised their rates
 * (backend returns 404 on GET /v1/carriers/:id/balikbayan-rates).
 *
 * Box sizes are fully dynamic — partners can add / rename / remove sizes.
 * Default set: XL · Jumbo · Large · Medium · Small · Bulilit
 */
import type {
  AirCargoFixed,
  AirCargoZone,
  AddOnService,
  BalikbayanRates,
  BoxBundle,
  BoxSize,
  PhDeliveryZone,
  SeaCargoRate,
  VolumetricGroup,
} from "@/lib/api/balikbayan-rates";

// ── Box sizes ──────────────────────────────────────────────────────────────────

export const DEFAULT_BOX_SIZES: BoxSize[] = [
  { id: "xl",      name: "XL",      dimensions: '24"×18"×18"', max_weight_kg: 25, sort_order: 0 },
  { id: "jumbo",   name: "Jumbo",   dimensions: '24"×24"×24"', max_weight_kg: 30, sort_order: 1 },
  { id: "large",   name: "Large",   dimensions: '20"×16"×16"', max_weight_kg: 20, sort_order: 2 },
  { id: "medium",  name: "Medium",  dimensions: '18"×14"×14"', max_weight_kg: 18, sort_order: 3 },
  { id: "small",   name: "Small",   dimensions: '16"×12"×12"', max_weight_kg: 15, sort_order: 4 },
  { id: "bulilit", name: "Bulilit", dimensions: '12"×10"×10"', max_weight_kg: 10, sort_order: 5 },
];

// Helper — build a prices map for all 6 default sizes.
// medium and bulilit are derived from large/small if not supplied explicitly.
function p(
  xl: number, jumbo: number, large: number, small: number,
  medium = Math.round((large + small) / 2 * 0.97),
  bulilit = Math.round(small * 0.79),
): Record<string, number> {
  return { xl, jumbo, large, medium, small, bulilit };
}

// ── Sea cargo ──────────────────────────────────────────────────────────────────

export const DEFAULT_SEA_CARGO: SeaCargoRate[] = [
  // USA
  { origin: "USA West Coast (CA, WA, OR)",          transit_days: "30–45", prices: p(120, 150, 95,  70)  },
  { origin: "USA East Coast (NY, NJ, CT, MA)",      transit_days: "40–55", prices: p(150, 185, 118, 88)  },
  { origin: "USA South & Central (TX, FL, IL, GA)", transit_days: "40–55", prices: p(140, 175, 110, 82)  },
  { origin: "Hawaii",                               transit_days: "25–35", prices: p(110, 138, 88,  65)  },
  { origin: "Alaska",                               transit_days: "45–60", prices: p(170, 210, 135, 100) },
  // Canada
  { origin: "Canada — British Columbia (Vancouver)", transit_days: "35–48", prices: p(130, 162, 103, 76) },
  { origin: "Canada — Alberta (Calgary / Edmonton)", transit_days: "38–52", prices: p(135, 168, 107, 79) },
  { origin: "Canada — Ontario / Quebec",             transit_days: "42–56", prices: p(143, 178, 113, 84) },
  // Europe
  { origin: "United Kingdom — London",              transit_days: "38–55", prices: p(140, 175, 111, 82)  },
  { origin: "Ireland — Dublin",                     transit_days: "40–55", prices: p(143, 178, 113, 84)  },
  { origin: "Italy — Rome / Milan",                 transit_days: "38–52", prices: p(138, 172, 109, 81)  },
  { origin: "Spain — Madrid / Barcelona",           transit_days: "38–52", prices: p(136, 170, 108, 80)  },
  { origin: "Germany — Frankfurt / Munich",         transit_days: "40–55", prices: p(139, 173, 110, 81)  },
  { origin: "France — Paris",                       transit_days: "40–55", prices: p(138, 172, 109, 80)  },
  { origin: "Netherlands / Belgium — Amsterdam",    transit_days: "40–55", prices: p(138, 172, 109, 80)  },
  { origin: "Norway — Oslo",                        transit_days: "42–58", prices: p(146, 182, 116, 86)  },
  { origin: "Sweden / Denmark",                     transit_days: "42–58", prices: p(144, 180, 114, 85)  },
  { origin: "Switzerland — Zurich / Geneva",        transit_days: "42–58", prices: p(148, 185, 118, 87)  },
  { origin: "Greece / Cyprus",                      transit_days: "45–60", prices: p(143, 178, 113, 84)  },
  // Middle East
  { origin: "UAE — Dubai / Abu Dhabi",              transit_days: "18–28", prices: p(100, 125, 80,  58)  },
  { origin: "Saudi Arabia — Riyadh",                transit_days: "20–30", prices: p(102, 128, 82,  60)  },
  { origin: "Saudi Arabia — Jeddah / Dammam",       transit_days: "20–30", prices: p(104, 130, 83,  61)  },
  { origin: "Qatar — Doha",                         transit_days: "20–30", prices: p(98,  122, 78,  57)  },
  { origin: "Kuwait",                               transit_days: "20–30", prices: p(99,  124, 79,  58)  },
  { origin: "Bahrain",                              transit_days: "20–30", prices: p(96,  120, 76,  56)  },
  { origin: "Oman — Muscat",                        transit_days: "20–30", prices: p(96,  120, 76,  56)  },
  { origin: "Israel",                               transit_days: "28–40", prices: p(116, 145, 92,  68)  },
  // Australia & NZ
  { origin: "Australia — Sydney / Melbourne",       transit_days: "22–32", prices: p(108, 135, 86,  63)  },
  { origin: "Australia — Perth",                    transit_days: "20–28", prices: p(102, 128, 82,  60)  },
  { origin: "Australia — Brisbane",                 transit_days: "22–32", prices: p(106, 132, 84,  62)  },
  { origin: "New Zealand — Auckland",               transit_days: "25–35", prices: p(114, 142, 90,  66)  },
  // Asia
  { origin: "Japan — Tokyo / Osaka",                transit_days: "10–18", prices: p(78,  98,  62,  46)  },
  { origin: "South Korea — Seoul / Busan",          transit_days: "10–18", prices: p(76,  95,  60,  44)  },
  { origin: "Hong Kong",                            transit_days: "7–12",  prices: p(64,  80,  51,  38)  },
  { origin: "Singapore",                            transit_days: "7–14",  prices: p(68,  85,  54,  40)  },
  { origin: "Taiwan — Taipei",                      transit_days: "8–14",  prices: p(70,  88,  56,  41)  },
  { origin: "Macau",                                transit_days: "7–12",  prices: p(62,  78,  50,  37)  },
  { origin: "Guam / Saipan (CNMI)",                 transit_days: "14–21", prices: p(86,  108, 68,  51)  },
];

// ── Air cargo ──────────────────────────────────────────────────────────────────

export const DEFAULT_AIR_ZONES: AirCargoZone[] = [
  { zone_name: "Zone 1 — ASEAN / NE Asia",        origins: "Hong Kong, Macau, Singapore, Japan, South Korea, Taiwan, Guam",               rate_per_kg_usd: 5.50,  distance_surcharge_per_km: 0.018, transit_days: "2–5"   },
  { zone_name: "Zone 2 — Australia / NZ",          origins: "Australia (all cities), New Zealand",                                          rate_per_kg_usd: 7.00,  distance_surcharge_per_km: 0.022, transit_days: "4–8"   },
  { zone_name: "Zone 3 — Middle East",             origins: "UAE, Saudi Arabia, Qatar, Kuwait, Bahrain, Oman, Israel",                      rate_per_kg_usd: 8.00,  distance_surcharge_per_km: 0.025, transit_days: "5–9"   },
  { zone_name: "Zone 4 — Europe",                  origins: "UK, Ireland, Italy, Spain, Germany, France, Netherlands, Norway, Sweden, Switzerland", rate_per_kg_usd: 10.50, distance_surcharge_per_km: 0.030, transit_days: "7–12"  },
  { zone_name: "Zone 5 — N. America West",         origins: "USA West Coast (CA, WA, OR), Hawaii",                                          rate_per_kg_usd: 12.00, distance_surcharge_per_km: 0.032, transit_days: "7–12"  },
  { zone_name: "Zone 6 — N. America East / Central", origins: "USA East, South & Central, Alaska, Canada (all provinces)",                 rate_per_kg_usd: 13.50, distance_surcharge_per_km: 0.035, transit_days: "8–14"  },
  { zone_name: "Zone 7 — Others",                  origins: "South Africa, Papua New Guinea, Latin America, other unlisted countries",       rate_per_kg_usd: 15.00, distance_surcharge_per_km: 0.040, transit_days: "10–21" },
];

export const DEFAULT_AIR_FIXED: AirCargoFixed = {
  awb_fee_usd: 25,
  fuel_surcharge_pct: 0.18,
  thc_usd: 15,
  customs_clearance_usd: 20,
  min_weight_kg: 5,
  volumetric_divisor: 5000,
};

// ── PH delivery zones ──────────────────────────────────────────────────────────

// Same price-helper for PH local delivery (smaller numbers, floor at 3)
function ph(
  xl: number, jumbo: number, large: number, small: number,
  medium = Math.max(3, Math.round((large + small) / 2 * 0.95)),
  bulilit = Math.max(3, Math.round(small * 0.75)),
): Record<string, number> {
  return { xl, jumbo, large, medium, small, bulilit };
}

export const DEFAULT_PH_DELIVERY: PhDeliveryZone[] = [
  // NCR
  { zone_code: "Zone 1A", zone_name: "Metro Manila / NCR",              coverage: "All NCR cities & municipalities (Makati, QC, Manila, Pasig, Taguig, Pasay, Parañaque, Caloocan, Marikina, etc.)", prices: ph(14, 18, 11, 8),  transit_days: "1–2 days"  },
  // Nearby Luzon
  { zone_code: "Zone 2A", zone_name: "Region III — Bulacan / Pampanga", coverage: "Bulacan, Pampanga, Tarlac, Nueva Ecija",                                                                            prices: ph(18, 22, 14, 10), transit_days: "2–3 days"  },
  { zone_code: "Zone 2B", zone_name: "CALABARZON",                      coverage: "Cavite, Laguna, Batangas, Rizal (Region IV-A)",                                                                      prices: ph(18, 22, 14, 10), transit_days: "2–3 days"  },
  { zone_code: "Zone 2C", zone_name: "Quezon Province / Lucena",        coverage: "Quezon Province, Lucena City",                                                                                       prices: ph(22, 28, 18, 13), transit_days: "3–4 days"  },
  // Northern Luzon
  { zone_code: "Zone 3A", zone_name: "Ilocos / La Union / Pangasinan",  coverage: "Pangasinan, La Union, Zambales, Olongapo, Ilocos Norte, Ilocos Sur",                                                prices: ph(26, 32, 20, 15), transit_days: "3–5 days"  },
  { zone_code: "Zone 3B", zone_name: "Cagayan Valley",                   coverage: "Cagayan, Isabela, Nueva Vizcaya, Quirino",                                                                          prices: ph(28, 35, 22, 16), transit_days: "4–6 days"  },
  { zone_code: "Zone 3C", zone_name: "CAR — Baguio / Cordillera",       coverage: "Benguet (Baguio), Mountain Province, Ifugao, Kalinga, Abra",                                                         prices: ph(30, 38, 24, 18), transit_days: "4–6 days"  },
  // Bicol / Southern Luzon
  { zone_code: "Zone 4A", zone_name: "Bicol — Camarines / Naga",        coverage: "Camarines Norte, Camarines Sur, Naga City",                                                                          prices: ph(28, 35, 22, 16), transit_days: "4–6 days"  },
  { zone_code: "Zone 4B", zone_name: "Bicol — Albay / Sorsogon",        coverage: "Albay (Legazpi), Sorsogon, Catanduanes, Masbate",                                                                    prices: ph(32, 40, 25, 18), transit_days: "5–7 days"  },
  { zone_code: "Zone 4C", zone_name: "MIMAROPA",                         coverage: "Marinduque, Occ. Mindoro, Or. Mindoro, Romblon",                                                                    prices: ph(34, 42, 27, 20), transit_days: "5–7 days"  },
  { zone_code: "Zone 4D", zone_name: "Palawan — Puerto Princesa",        coverage: "Puerto Princesa City and Palawan main island",                                                                       prices: ph(44, 55, 35, 26), transit_days: "7–10 days" },
  { zone_code: "Zone 4E", zone_name: "Palawan — Remote Islands",         coverage: "Coron, El Nido, Southern Palawan islands",                                                                          prices: ph(56, 70, 45, 33), transit_days: "10–15 days"},
  // Visayas
  { zone_code: "Zone 5A", zone_name: "Metro Cebu",                       coverage: "Cebu City, Mandaue, Lapu-Lapu, Talisay",                                                                            prices: ph(24, 30, 19, 14), transit_days: "3–5 days"  },
  { zone_code: "Zone 5B", zone_name: "Cebu Province",                    coverage: "Cebu province municipalities outside Metro Cebu",                                                                    prices: ph(30, 38, 24, 18), transit_days: "5–7 days"  },
  { zone_code: "Zone 5C", zone_name: "Iloilo / Bacolod City",            coverage: "Iloilo City, Bacolod City (metro areas)",                                                                           prices: ph(28, 35, 22, 16), transit_days: "4–6 days"  },
  { zone_code: "Zone 5D", zone_name: "Western Visayas — Provinces",      coverage: "Iloilo Province, Negros Occidental municipalities",                                                                  prices: ph(34, 42, 27, 20), transit_days: "5–7 days"  },
  { zone_code: "Zone 5E", zone_name: "Negros Oriental / Bohol",          coverage: "Negros Oriental, Bohol (Tagbilaran), Siquijor",                                                                      prices: ph(36, 45, 28, 21), transit_days: "5–8 days"  },
  { zone_code: "Zone 5F", zone_name: "Eastern Visayas — Leyte / Samar",  coverage: "Leyte (Tacloban), Samar, Eastern Samar, Northern Samar",                                                           prices: ph(38, 48, 30, 22), transit_days: "6–9 days"  },
  { zone_code: "Zone 5G", zone_name: "Aklan / Antique / Capiz",          coverage: "Capiz, Aklan (Boracay), Antique",                                                                                   prices: ph(36, 45, 29, 21), transit_days: "5–8 days"  },
  // Mindanao
  { zone_code: "Zone 6A", zone_name: "Metro Davao",                      coverage: "Davao City and Metro Davao (Davao del Sur)",                                                                         prices: ph(32, 40, 25, 18), transit_days: "5–7 days"  },
  { zone_code: "Zone 6B", zone_name: "Davao Region — Provinces",         coverage: "Davao del Norte, Davao Oriental, ComVal",                                                                           prices: ph(38, 48, 30, 22), transit_days: "6–9 days"  },
  { zone_code: "Zone 6C", zone_name: "Cagayan de Oro / Bukidnon",        coverage: "Cagayan de Oro, Misamis Oriental, Bukidnon",                                                                         prices: ph(34, 42, 27, 20), transit_days: "5–8 days"  },
  { zone_code: "Zone 6D", zone_name: "GenSan / Sarangani / S. Cotabato", coverage: "General Santos City, Sarangani, South Cotabato",                                                                    prices: ph(36, 45, 28, 21), transit_days: "6–9 days"  },
  { zone_code: "Zone 6E", zone_name: "Zamboanga",                         coverage: "Zamboanga City, Zamboanga del Sur, Zamboanga del Norte",                                                            prices: ph(42, 52, 33, 24), transit_days: "7–10 days" },
  { zone_code: "Zone 6F", zone_name: "Lanao / Iligan — BARMM",           coverage: "Lanao del Norte, Lanao del Sur, Iligan City",                                                                       prices: ph(42, 52, 33, 24), transit_days: "7–10 days" },
  { zone_code: "Zone 6G", zone_name: "Cotabato / Maguindanao — BARMM",   coverage: "Cotabato City, Sultan Kudarat, Maguindanao",                                                                        prices: ph(44, 55, 35, 26), transit_days: "7–11 days" },
  { zone_code: "Zone 6H", zone_name: "Caraga — Surigao / Agusan",        coverage: "Surigao del Norte, Surigao del Sur, Agusan del Norte, Agusan del Sur",                                              prices: ph(44, 55, 35, 26), transit_days: "8–12 days" },
  { zone_code: "Zone 6I", zone_name: "Camiguin / Dinagat",               coverage: "Camiguin Island, Dinagat Islands",                                                                                  prices: ph(50, 62, 40, 29), transit_days: "9–14 days" },
  // Remote Islands
  { zone_code: "Zone 7A", zone_name: "Sulu / Tawi-Tawi / Basilan",       coverage: "Sulu, Tawi-Tawi, Basilan (BARMM far south)",                                                                        prices: ph(64, 80, 51, 38), transit_days: "12–18 days"},
  { zone_code: "Zone 7B", zone_name: "Batanes",                           coverage: "Batan Island and Batanes group (northernmost PH)",                                                                  prices: ph(72, 90, 57, 42), transit_days: "14–21 days"},
];

// ── Volumetric groups ──────────────────────────────────────────────────────────

export const DEFAULT_VOLUMETRIC: VolumetricGroup[] = [
  { group: "Manila",   divisor: 5000, base_rate_usd: 12, rate_per_cbm_usd: 85,  min_charge_usd: 12, surcharge_pct: 0    },
  { group: "Luzon",    divisor: 5000, base_rate_usd: 15, rate_per_cbm_usd: 95,  min_charge_usd: 15, surcharge_pct: 0.05 },
  { group: "Visayas",  divisor: 4500, base_rate_usd: 18, rate_per_cbm_usd: 110, min_charge_usd: 18, surcharge_pct: 0.08 },
  { group: "Mindanao", divisor: 4500, base_rate_usd: 20, rate_per_cbm_usd: 120, min_charge_usd: 20, surcharge_pct: 0.10 },
  { group: "Islands",  divisor: 4000, base_rate_usd: 28, rate_per_cbm_usd: 150, min_charge_usd: 28, surcharge_pct: 0.15 },
];

// ── Bundles ────────────────────────────────────────────────────────────────────

export const DEFAULT_BUNDLES: BoxBundle[] = [
  {
    id: "bundle-ofw-starter",
    name: "OFW Starter Bundle",
    description: "Perfect for first-time senders — covers the family essentials.",
    items: [{ size_id: "jumbo", quantity: 1 }, { size_id: "small", quantity: 1 }],
    price_usd: 195,
    valid_for: "sea",
    notes: "1 Jumbo + 1 Small box. Sea freight only. PH local delivery billed separately per zone.",
  },
  {
    id: "bundle-family",
    name: "Family Balikbayan Bundle",
    description: "Two Jumbo boxes + one Large — enough for the whole barangay.",
    items: [{ size_id: "jumbo", quantity: 2 }, { size_id: "large", quantity: 1 }],
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
    price_usd: 290,
    valid_for: "both",
    notes: "1 XL + 1 Medium + 2 Small. Valid for sea and air cargo routes.",
  },
  {
    id: "bundle-pasalubong",
    name: "Pasalubong Bundle",
    description: "5 Bulilit boxes — ideal for pasalubong, chocolates, and small gifts.",
    items: [{ size_id: "bulilit", quantity: 5 }],
    price_usd: 240,
    valid_for: "sea",
    notes: "5 Bulilit boxes shipped together. Maximum 10 kg per box.",
  },
];

// ── Add-ons ────────────────────────────────────────────────────────────────────

export const DEFAULT_ADDONS: AddOnService[] = [
  // Insurance
  { id: "ins-sea-basic",   name: "Basic All-Risk Sea",       category: "insurance", rate: 0.015, rate_type: "percent", min_charge: 15, description: "Loss & damage in transit; excludes inherent vice, war" },
  { id: "ins-sea-ext",     name: "Extended All-Risk Sea",    category: "insurance", rate: 0.025, rate_type: "percent", min_charge: 20, description: "Adds natural disaster, port flooding, pilferage" },
  { id: "ins-sea-prem",    name: "Premium All-Risk Sea",     category: "insurance", rate: 0.030, rate_type: "percent", min_charge: 25, description: "Total + partial loss + delay compensation up to 10%" },
  { id: "ins-air-basic",   name: "Basic All-Risk Air",       category: "insurance", rate: 0.010, rate_type: "percent", min_charge: 15, description: "Standard IATA air cargo conditions" },
  { id: "ins-air-ext",     name: "Extended All-Risk Air",    category: "insurance", rate: 0.0175,rate_type: "percent", min_charge: 18, description: "Adds rough handling, delay reimbursement up to $200" },
  { id: "ins-high-value",  name: "High-Value Goods Rider",   category: "insurance", rate: 0.005, rate_type: "percent", min_charge: 10, description: "Electronics, jewelry, branded goods" },
  // Crate
  { id: "crate-mini",      name: 'Mini Crate (20"×16"×16")',    category: "crate", rate: 55,  rate_type: "fixed", min_charge: 55,  description: "Fits 1 × Small or Bulilit box" },
  { id: "crate-std",       name: 'Standard Crate (24"×20"×20")',category: "crate", rate: 75,  rate_type: "fixed", min_charge: 75,  description: "Fits 1 × Medium or Large box" },
  { id: "crate-large",     name: 'Large Crate (28"×22"×22")',   category: "crate", rate: 95,  rate_type: "fixed", min_charge: 95,  description: "Fits 1 × XL box" },
  { id: "crate-jumbo",     name: 'Jumbo Crate (30"×28"×28")',   category: "crate", rate: 125, rate_type: "fixed", min_charge: 125, description: "Fits 1 × Jumbo box" },
  { id: "crate-dbl",       name: "Double-Wall Reinforced",       category: "crate", rate: 30,  rate_type: "fixed", min_charge: 30,  description: "Add-on to any crate — extra fragile items, antiques" },
  // Packing
  { id: "pack-basic",      name: "Basic Seal",                category: "packing", rate: 15, rate_type: "fixed", min_charge: 15, description: "Tape reinforcement, box sealing, label" },
  { id: "pack-std",        name: "Standard Pack",             category: "packing", rate: 30, rate_type: "fixed", min_charge: 30, description: "Bubble wrap + kraft paper fill + tape seal" },
  { id: "pack-full",       name: "Full Professional Pack",    category: "packing", rate: 55, rate_type: "fixed", min_charge: 55, description: "Individual wrap, foam padding, corner protectors" },
  { id: "pack-prem",       name: "Premium Fragile Pack",      category: "packing", rate: 90, rate_type: "fixed", min_charge: 90, description: "Foam-in-place, custom inserts, moisture barrier" },
  { id: "pack-vacuum",     name: "Vacuum / Compression Bags", category: "packing", rate: 20, rate_type: "fixed", min_charge: 20, description: "Space-saving vacuum seal for textiles and linens" },
  { id: "pack-rebox",      name: "Re-boxing Service",         category: "packing", rate: 40, rate_type: "fixed", min_charge: 40, description: "Unpack & repack into standard Balikbayan box" },
  { id: "pack-inventory",  name: "Inventory & Photo Docs",    category: "packing", rate: 25, rate_type: "fixed", min_charge: 25, description: "Full item-by-item photo record + itemized list" },
  // Surcharges
  { id: "sur-peak",        name: "Peak Season Surcharge",     category: "surcharge", rate: 0.175,rate_type: "percent", min_charge: 0, description: "Oct 1 – Jan 31 (Christmas / Pasko). 15–20% of sea freight" },
  { id: "sur-overweight",  name: "Overweight (per kg)",       category: "surcharge", rate: 3,   rate_type: "per_kg",  min_charge: 3, description: "Per kg over declared box weight limit" },
  { id: "sur-oversize",    name: "Oversize Box",              category: "surcharge", rate: 35,  rate_type: "fixed",   min_charge: 35, description: "Any side exceeds 27 inches" },
  { id: "sur-storage",     name: "Storage / Demurrage",       category: "surcharge", rate: 5,   rate_type: "per_day", min_charge: 5,  description: "Per box/day after 7 free days at origin warehouse" },
  { id: "sur-redelivery",  name: "Re-delivery Fee (PH)",      category: "surcharge", rate: 8,   rate_type: "fixed",   min_charge: 8,  description: "Per failed delivery attempt after first (up to 3 attempts)" },
  { id: "sur-customs",     name: "Customs Clearance Assist",  category: "surcharge", rate: 30,  rate_type: "fixed",   min_charge: 30, description: "PH BOC query support; excludes duties & taxes" },
];

// ── Root builder ───────────────────────────────────────────────────────────────

export function buildDefaultRates(carrierId: string): BalikbayanRates {
  return {
    carrier_id:       carrierId,
    box_sizes:        DEFAULT_BOX_SIZES,
    sea_cargo:        DEFAULT_SEA_CARGO,
    air_cargo_zones:  DEFAULT_AIR_ZONES,
    air_cargo_fixed:  DEFAULT_AIR_FIXED,
    ph_delivery_zones:DEFAULT_PH_DELIVERY,
    volumetric_groups:DEFAULT_VOLUMETRIC,
    bundles:          DEFAULT_BUNDLES,
    addons:           DEFAULT_ADDONS,
    updated_at:       new Date().toISOString(),
  };
}
