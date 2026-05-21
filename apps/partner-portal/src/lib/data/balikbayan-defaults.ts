/**
 * Default Balikbayan rate tables — seeded from the published Atlas Cargo
 * rate card.  Loaded when a carrier has not yet customised their rates
 * (backend returns 404 on GET /v1/carriers/:id/balikbayan-rates).
 */
import type {
  AirCargoFixed,
  AirCargoZone,
  AddOnService,
  BalikbayanRates,
  PhDeliveryZone,
  SeaCargoRate,
  VolumetricGroup,
} from "@/lib/api/balikbayan-rates";

export const DEFAULT_SEA_CARGO: SeaCargoRate[] = [
  // ── USA ────────────────────────────────────────────────────────────────────
  { origin: "USA West Coast (CA, WA, OR)",          transit_days: "30–45", jumbo_usd: 150, xl_usd: 120, large_usd: 95,  small_usd: 70  },
  { origin: "USA East Coast (NY, NJ, CT, MA)",      transit_days: "40–55", jumbo_usd: 185, xl_usd: 150, large_usd: 118, small_usd: 88  },
  { origin: "USA South & Central (TX, FL, IL, GA)", transit_days: "40–55", jumbo_usd: 175, xl_usd: 140, large_usd: 110, small_usd: 82  },
  { origin: "Hawaii",                               transit_days: "25–35", jumbo_usd: 138, xl_usd: 110, large_usd: 88,  small_usd: 65  },
  { origin: "Alaska",                               transit_days: "45–60", jumbo_usd: 210, xl_usd: 170, large_usd: 135, small_usd: 100 },
  // ── Canada ─────────────────────────────────────────────────────────────────
  { origin: "Canada — British Columbia (Vancouver)", transit_days: "35–48", jumbo_usd: 162, xl_usd: 130, large_usd: 103, small_usd: 76 },
  { origin: "Canada — Alberta (Calgary/Edmonton)",  transit_days: "38–52", jumbo_usd: 168, xl_usd: 135, large_usd: 107, small_usd: 79  },
  { origin: "Canada — Ontario/Quebec (Toronto/Montreal)", transit_days: "42–56", jumbo_usd: 178, xl_usd: 143, large_usd: 113, small_usd: 84 },
  // ── Europe ─────────────────────────────────────────────────────────────────
  { origin: "United Kingdom — London",              transit_days: "38–55", jumbo_usd: 175, xl_usd: 140, large_usd: 111, small_usd: 82  },
  { origin: "Ireland — Dublin",                     transit_days: "40–55", jumbo_usd: 178, xl_usd: 143, large_usd: 113, small_usd: 84  },
  { origin: "Italy — Rome / Milan",                 transit_days: "38–52", jumbo_usd: 172, xl_usd: 138, large_usd: 109, small_usd: 81  },
  { origin: "Spain — Madrid / Barcelona",           transit_days: "38–52", jumbo_usd: 170, xl_usd: 136, large_usd: 108, small_usd: 80  },
  { origin: "Germany — Frankfurt / Munich",         transit_days: "40–55", jumbo_usd: 173, xl_usd: 139, large_usd: 110, small_usd: 81  },
  { origin: "France — Paris",                       transit_days: "40–55", jumbo_usd: 172, xl_usd: 138, large_usd: 109, small_usd: 80  },
  { origin: "Netherlands / Belgium — Amsterdam",    transit_days: "40–55", jumbo_usd: 172, xl_usd: 138, large_usd: 109, small_usd: 80  },
  { origin: "Norway — Oslo",                        transit_days: "42–58", jumbo_usd: 182, xl_usd: 146, large_usd: 116, small_usd: 86  },
  { origin: "Sweden / Denmark — Stockholm",         transit_days: "42–58", jumbo_usd: 180, xl_usd: 144, large_usd: 114, small_usd: 85  },
  { origin: "Switzerland — Zurich / Geneva",        transit_days: "42–58", jumbo_usd: 185, xl_usd: 148, large_usd: 118, small_usd: 87  },
  { origin: "Greece / Cyprus",                      transit_days: "45–60", jumbo_usd: 178, xl_usd: 143, large_usd: 113, small_usd: 84  },
  // ── Middle East ─────────────────────────────────────────────────────────────
  { origin: "UAE — Dubai / Abu Dhabi",              transit_days: "18–28", jumbo_usd: 125, xl_usd: 100, large_usd: 80,  small_usd: 58  },
  { origin: "Saudi Arabia — Riyadh",                transit_days: "20–30", jumbo_usd: 128, xl_usd: 102, large_usd: 82,  small_usd: 60  },
  { origin: "Saudi Arabia — Jeddah / Dammam",       transit_days: "20–30", jumbo_usd: 130, xl_usd: 104, large_usd: 83,  small_usd: 61  },
  { origin: "Qatar — Doha",                         transit_days: "20–30", jumbo_usd: 122, xl_usd: 98,  large_usd: 78,  small_usd: 57  },
  { origin: "Kuwait",                               transit_days: "20–30", jumbo_usd: 124, xl_usd: 99,  large_usd: 79,  small_usd: 58  },
  { origin: "Bahrain",                              transit_days: "20–30", jumbo_usd: 120, xl_usd: 96,  large_usd: 76,  small_usd: 56  },
  { origin: "Oman — Muscat",                        transit_days: "20–30", jumbo_usd: 120, xl_usd: 96,  large_usd: 76,  small_usd: 56  },
  { origin: "Israel",                               transit_days: "28–40", jumbo_usd: 145, xl_usd: 116, large_usd: 92,  small_usd: 68  },
  // ── Australia & NZ ──────────────────────────────────────────────────────────
  { origin: "Australia — Sydney / Melbourne",       transit_days: "22–32", jumbo_usd: 135, xl_usd: 108, large_usd: 86,  small_usd: 63  },
  { origin: "Australia — Perth",                    transit_days: "20–28", jumbo_usd: 128, xl_usd: 102, large_usd: 82,  small_usd: 60  },
  { origin: "Australia — Brisbane",                 transit_days: "22–32", jumbo_usd: 132, xl_usd: 106, large_usd: 84,  small_usd: 62  },
  { origin: "New Zealand — Auckland",               transit_days: "25–35", jumbo_usd: 142, xl_usd: 114, large_usd: 90,  small_usd: 66  },
  // ── Asia ───────────────────────────────────────────────────────────────────
  { origin: "Japan — Tokyo / Osaka",                transit_days: "10–18", jumbo_usd: 98,  xl_usd: 78,  large_usd: 62,  small_usd: 46  },
  { origin: "South Korea — Seoul / Busan",          transit_days: "10–18", jumbo_usd: 95,  xl_usd: 76,  large_usd: 60,  small_usd: 44  },
  { origin: "Hong Kong",                            transit_days: "7–12",  jumbo_usd: 80,  xl_usd: 64,  large_usd: 51,  small_usd: 38  },
  { origin: "Singapore",                            transit_days: "7–14",  jumbo_usd: 85,  xl_usd: 68,  large_usd: 54,  small_usd: 40  },
  { origin: "Taiwan — Taipei",                      transit_days: "8–14",  jumbo_usd: 88,  xl_usd: 70,  large_usd: 56,  small_usd: 41  },
  { origin: "Macau",                                transit_days: "7–12",  jumbo_usd: 78,  xl_usd: 62,  large_usd: 50,  small_usd: 37  },
  { origin: "Guam / Saipan (CNMI)",                 transit_days: "14–21", jumbo_usd: 108, xl_usd: 86,  large_usd: 68,  small_usd: 51  },
];

export const DEFAULT_AIR_ZONES: AirCargoZone[] = [
  { zone_name: "Zone 1 — ASEAN / NE Asia",      origins: "Hong Kong, Macau, Singapore, Japan, South Korea, Taiwan, Guam",               rate_per_kg_usd: 5.50,  distance_surcharge_per_km: 0.018, transit_days: "2–5"   },
  { zone_name: "Zone 2 — Australia / NZ",        origins: "Australia (all cities), New Zealand",                                          rate_per_kg_usd: 7.00,  distance_surcharge_per_km: 0.022, transit_days: "4–8"   },
  { zone_name: "Zone 3 — Middle East",           origins: "UAE, Saudi Arabia, Qatar, Kuwait, Bahrain, Oman, Israel",                      rate_per_kg_usd: 8.00,  distance_surcharge_per_km: 0.025, transit_days: "5–9"   },
  { zone_name: "Zone 4 — Europe",                origins: "UK, Ireland, Italy, Spain, Germany, France, Netherlands, Norway, Sweden, Switzerland", rate_per_kg_usd: 10.50, distance_surcharge_per_km: 0.030, transit_days: "7–12"  },
  { zone_name: "Zone 5 — N. America West",       origins: "USA West Coast (CA, WA, OR), Hawaii",                                          rate_per_kg_usd: 12.00, distance_surcharge_per_km: 0.032, transit_days: "7–12"  },
  { zone_name: "Zone 6 — N. America East/Central", origins: "USA East, South & Central, Alaska, Canada (all provinces)",                 rate_per_kg_usd: 13.50, distance_surcharge_per_km: 0.035, transit_days: "8–14"  },
  { zone_name: "Zone 7 — Others",                origins: "South Africa, Papua New Guinea, Latin America, other unlisted countries",       rate_per_kg_usd: 15.00, distance_surcharge_per_km: 0.040, transit_days: "10–21" },
];

export const DEFAULT_AIR_FIXED: AirCargoFixed = {
  awb_fee_usd: 25,
  fuel_surcharge_pct: 0.18,
  thc_usd: 15,
  customs_clearance_usd: 20,
  min_weight_kg: 5,
  volumetric_divisor: 5000,
};

export const DEFAULT_PH_DELIVERY: PhDeliveryZone[] = [
  // ── NCR ────────────────────────────────────────────────────────────────────
  { zone_code: "Zone 1A", zone_name: "Metro Manila / NCR",             coverage: "All NCR cities & municipalities (Makati, QC, Manila, Pasig, Taguig, Pasay, Parañaque, Caloocan, Marikina, etc.)", jumbo_usd: 18, xl_usd: 14, large_usd: 11, small_usd: 8,  transit_days: "1–2 days" },
  // ── Nearby Luzon ────────────────────────────────────────────────────────────
  { zone_code: "Zone 2A", zone_name: "Region III — Bulacan / Pampanga",coverage: "Bulacan, Pampanga, Tarlac, Nueva Ecija",                                                                           jumbo_usd: 22, xl_usd: 18, large_usd: 14, small_usd: 10, transit_days: "2–3 days" },
  { zone_code: "Zone 2B", zone_name: "CALABARZON",                     coverage: "Cavite, Laguna, Batangas, Rizal (Region IV-A)",                                                                     jumbo_usd: 22, xl_usd: 18, large_usd: 14, small_usd: 10, transit_days: "2–3 days" },
  { zone_code: "Zone 2C", zone_name: "Quezon Province / Lucena",       coverage: "Quezon Province, Lucena City",                                                                                      jumbo_usd: 28, xl_usd: 22, large_usd: 18, small_usd: 13, transit_days: "3–4 days" },
  // ── Northern Luzon ──────────────────────────────────────────────────────────
  { zone_code: "Zone 3A", zone_name: "Ilocos / La Union / Pangasinan", coverage: "Pangasinan, La Union, Zambales, Olongapo, Ilocos Norte, Ilocos Sur",                                               jumbo_usd: 32, xl_usd: 26, large_usd: 20, small_usd: 15, transit_days: "3–5 days" },
  { zone_code: "Zone 3B", zone_name: "Cagayan Valley",                  coverage: "Cagayan, Isabela, Nueva Vizcaya, Quirino",                                                                         jumbo_usd: 35, xl_usd: 28, large_usd: 22, small_usd: 16, transit_days: "4–6 days" },
  { zone_code: "Zone 3C", zone_name: "CAR — Baguio / Cordillera",      coverage: "Benguet (Baguio), Mountain Province, Ifugao, Kalinga, Abra",                                                        jumbo_usd: 38, xl_usd: 30, large_usd: 24, small_usd: 18, transit_days: "4–6 days" },
  // ── Bicol / Southern Luzon ──────────────────────────────────────────────────
  { zone_code: "Zone 4A", zone_name: "Bicol — Camarines / Naga",       coverage: "Camarines Norte, Camarines Sur, Naga City",                                                                         jumbo_usd: 35, xl_usd: 28, large_usd: 22, small_usd: 16, transit_days: "4–6 days" },
  { zone_code: "Zone 4B", zone_name: "Bicol — Albay / Sorsogon",       coverage: "Albay (Legazpi), Sorsogon, Catanduanes, Masbate",                                                                   jumbo_usd: 40, xl_usd: 32, large_usd: 25, small_usd: 18, transit_days: "5–7 days" },
  { zone_code: "Zone 4C", zone_name: "MIMAROPA",                        coverage: "Marinduque, Occ. Mindoro, Or. Mindoro, Romblon",                                                                   jumbo_usd: 42, xl_usd: 34, large_usd: 27, small_usd: 20, transit_days: "5–7 days" },
  { zone_code: "Zone 4D", zone_name: "Palawan — Puerto Princesa",       coverage: "Puerto Princesa City and Palawan main island",                                                                      jumbo_usd: 55, xl_usd: 44, large_usd: 35, small_usd: 26, transit_days: "7–10 days" },
  { zone_code: "Zone 4E", zone_name: "Palawan — Remote Islands",        coverage: "Coron, El Nido, Southern Palawan islands",                                                                         jumbo_usd: 70, xl_usd: 56, large_usd: 45, small_usd: 33, transit_days: "10–15 days" },
  // ── Visayas ─────────────────────────────────────────────────────────────────
  { zone_code: "Zone 5A", zone_name: "Metro Cebu",                      coverage: "Cebu City, Mandaue, Lapu-Lapu, Talisay",                                                                           jumbo_usd: 30, xl_usd: 24, large_usd: 19, small_usd: 14, transit_days: "3–5 days" },
  { zone_code: "Zone 5B", zone_name: "Cebu Province",                   coverage: "Cebu province municipalities outside Metro Cebu",                                                                   jumbo_usd: 38, xl_usd: 30, large_usd: 24, small_usd: 18, transit_days: "5–7 days" },
  { zone_code: "Zone 5C", zone_name: "Iloilo / Bacolod City",           coverage: "Iloilo City, Bacolod City (metro areas)",                                                                          jumbo_usd: 35, xl_usd: 28, large_usd: 22, small_usd: 16, transit_days: "4–6 days" },
  { zone_code: "Zone 5D", zone_name: "Western Visayas — Provinces",     coverage: "Iloilo Province, Negros Occidental municipalities",                                                                 jumbo_usd: 42, xl_usd: 34, large_usd: 27, small_usd: 20, transit_days: "5–7 days" },
  { zone_code: "Zone 5E", zone_name: "Negros Oriental / Bohol",         coverage: "Negros Oriental, Bohol (Tagbilaran), Siquijor",                                                                     jumbo_usd: 45, xl_usd: 36, large_usd: 28, small_usd: 21, transit_days: "5–8 days" },
  { zone_code: "Zone 5F", zone_name: "Eastern Visayas — Leyte / Samar", coverage: "Leyte (Tacloban), Samar, Eastern Samar, Northern Samar",                                                          jumbo_usd: 48, xl_usd: 38, large_usd: 30, small_usd: 22, transit_days: "6–9 days" },
  { zone_code: "Zone 5G", zone_name: "Aklan / Antique / Capiz",         coverage: "Capiz, Aklan (Boracay), Antique",                                                                                  jumbo_usd: 45, xl_usd: 36, large_usd: 29, small_usd: 21, transit_days: "5–8 days" },
  // ── Mindanao ────────────────────────────────────────────────────────────────
  { zone_code: "Zone 6A", zone_name: "Metro Davao",                     coverage: "Davao City and Metro Davao (Davao del Sur)",                                                                        jumbo_usd: 40, xl_usd: 32, large_usd: 25, small_usd: 18, transit_days: "5–7 days" },
  { zone_code: "Zone 6B", zone_name: "Davao Region — Provinces",        coverage: "Davao del Norte, Davao Oriental, ComVal",                                                                          jumbo_usd: 48, xl_usd: 38, large_usd: 30, small_usd: 22, transit_days: "6–9 days" },
  { zone_code: "Zone 6C", zone_name: "Cagayan de Oro / Bukidnon",       coverage: "Cagayan de Oro, Misamis Oriental, Bukidnon",                                                                        jumbo_usd: 42, xl_usd: 34, large_usd: 27, small_usd: 20, transit_days: "5–8 days" },
  { zone_code: "Zone 6D", zone_name: "GenSan / Sarangani / S. Cotabato",coverage: "General Santos City, Sarangani, South Cotabato",                                                                   jumbo_usd: 45, xl_usd: 36, large_usd: 28, small_usd: 21, transit_days: "6–9 days" },
  { zone_code: "Zone 6E", zone_name: "Zamboanga",                        coverage: "Zamboanga City, Zamboanga del Sur, Zamboanga del Norte",                                                           jumbo_usd: 52, xl_usd: 42, large_usd: 33, small_usd: 24, transit_days: "7–10 days" },
  { zone_code: "Zone 6F", zone_name: "Lanao / Iligan — BARMM",          coverage: "Lanao del Norte, Lanao del Sur, Iligan City",                                                                      jumbo_usd: 52, xl_usd: 42, large_usd: 33, small_usd: 24, transit_days: "7–10 days" },
  { zone_code: "Zone 6G", zone_name: "Cotabato / Maguindanao — BARMM",  coverage: "Cotabato City, Sultan Kudarat, Maguindanao",                                                                       jumbo_usd: 55, xl_usd: 44, large_usd: 35, small_usd: 26, transit_days: "7–11 days" },
  { zone_code: "Zone 6H", zone_name: "Caraga — Surigao / Agusan",       coverage: "Surigao del Norte, Surigao del Sur, Agusan del Norte, Agusan del Sur",                                             jumbo_usd: 55, xl_usd: 44, large_usd: 35, small_usd: 26, transit_days: "8–12 days" },
  { zone_code: "Zone 6I", zone_name: "Camiguin / Dinagat",              coverage: "Camiguin Island, Dinagat Islands",                                                                                 jumbo_usd: 62, xl_usd: 50, large_usd: 40, small_usd: 29, transit_days: "9–14 days" },
  // ── Remote Islands ──────────────────────────────────────────────────────────
  { zone_code: "Zone 7A", zone_name: "Sulu / Tawi-Tawi / Basilan",      coverage: "Sulu, Tawi-Tawi, Basilan (BARMM far south)",                                                                       jumbo_usd: 80, xl_usd: 64, large_usd: 51, small_usd: 38, transit_days: "12–18 days" },
  { zone_code: "Zone 7B", zone_name: "Batanes",                          coverage: "Batan Island and Batanes group (northernmost PH)",                                                                 jumbo_usd: 90, xl_usd: 72, large_usd: 57, small_usd: 42, transit_days: "14–21 days" },
];

export const DEFAULT_VOLUMETRIC: VolumetricGroup[] = [
  { group: "Manila",   divisor: 5000, base_rate_usd: 12, rate_per_cbm_usd: 85,  min_charge_usd: 12, surcharge_pct: 0     },
  { group: "Luzon",    divisor: 5000, base_rate_usd: 15, rate_per_cbm_usd: 95,  min_charge_usd: 15, surcharge_pct: 0.05  },
  { group: "Visayas",  divisor: 4500, base_rate_usd: 18, rate_per_cbm_usd: 110, min_charge_usd: 18, surcharge_pct: 0.08  },
  { group: "Mindanao", divisor: 4500, base_rate_usd: 20, rate_per_cbm_usd: 120, min_charge_usd: 20, surcharge_pct: 0.10  },
  { group: "Islands",  divisor: 4000, base_rate_usd: 28, rate_per_cbm_usd: 150, min_charge_usd: 28, surcharge_pct: 0.15  },
];

export const DEFAULT_ADDONS: AddOnService[] = [
  // Insurance
  { id: "ins-sea-basic",    name: "Basic All-Risk Sea",       category: "insurance", rate: 0.015, rate_type: "percent", min_charge: 15, description: "Loss & damage in transit; excludes inherent vice, war" },
  { id: "ins-sea-ext",      name: "Extended All-Risk Sea",    category: "insurance", rate: 0.025, rate_type: "percent", min_charge: 20, description: "Adds natural disaster, port flooding, pilferage" },
  { id: "ins-sea-prem",     name: "Premium All-Risk Sea",     category: "insurance", rate: 0.030, rate_type: "percent", min_charge: 25, description: "Total + partial loss + delay compensation up to 10%" },
  { id: "ins-air-basic",    name: "Basic All-Risk Air",       category: "insurance", rate: 0.010, rate_type: "percent", min_charge: 15, description: "Standard IATA air cargo conditions" },
  { id: "ins-air-ext",      name: "Extended All-Risk Air",    category: "insurance", rate: 0.0175,rate_type: "percent", min_charge: 18, description: "Adds rough handling, delay reimbursement up to $200" },
  { id: "ins-high-value",   name: "High-Value Goods Rider",   category: "insurance", rate: 0.005, rate_type: "percent", min_charge: 10, description: "Electronics, jewelry, branded goods — attach packing list" },
  // Wooden Crate
  { id: "crate-mini",       name: 'Mini Crate (20"×16"×16")',    category: "crate",     rate: 55,  rate_type: "fixed",   min_charge: 55,  description: "Fits 1 × Small box. Electronics, small appliances" },
  { id: "crate-std",        name: 'Standard Crate (24"×20"×20")',category: "crate",     rate: 75,  rate_type: "fixed",   min_charge: 75,  description: "Fits 1 × Large box. Household goods, kitchenware" },
  { id: "crate-large",      name: 'Large Crate (28"×22"×22")',   category: "crate",     rate: 95,  rate_type: "fixed",   min_charge: 95,  description: "Fits 1 × XL box. Large appliances, medical equipment" },
  { id: "crate-jumbo",      name: 'Jumbo Crate (30"×28"×28")',   category: "crate",     rate: 125, rate_type: "fixed",   min_charge: 125, description: "Fits 1 × Jumbo box. Oversized items, machinery" },
  { id: "crate-dbl",        name: "Double-Wall Reinforced",       category: "crate",     rate: 30,  rate_type: "fixed",   min_charge: 30,  description: "Add-on to any crate. Extra fragile items, antiques" },
  // Packing
  { id: "pack-basic",       name: "Basic Seal",               category: "packing",   rate: 15, rate_type: "fixed",   min_charge: 15, description: "Tape reinforcement, box sealing, label" },
  { id: "pack-std",         name: "Standard Pack",            category: "packing",   rate: 30, rate_type: "fixed",   min_charge: 30, description: "Bubble wrap + kraft paper fill + tape seal" },
  { id: "pack-full",        name: "Full Professional Pack",   category: "packing",   rate: 55, rate_type: "fixed",   min_charge: 55, description: "Individual wrap, foam padding, corner protectors" },
  { id: "pack-prem",        name: "Premium Fragile Pack",     category: "packing",   rate: 90, rate_type: "fixed",   min_charge: 90, description: "Foam-in-place, custom inserts, moisture barrier" },
  { id: "pack-vacuum",      name: "Vacuum / Compression Bags",category: "packing",   rate: 20, rate_type: "fixed",   min_charge: 20, description: "Space-saving vacuum seal for textiles and linens" },
  { id: "pack-rebox",       name: "Re-boxing Service",        category: "packing",   rate: 40, rate_type: "fixed",   min_charge: 40, description: "Unpack & repack into standard Balikbayan box" },
  { id: "pack-inventory",   name: "Inventory & Photo Docs",   category: "packing",   rate: 25, rate_type: "fixed",   min_charge: 25, description: "Full item-by-item photo record + itemized list" },
  // Surcharges
  { id: "sur-peak",         name: "Peak Season Surcharge",    category: "surcharge", rate: 0.175,rate_type: "percent", min_charge: 0,  description: "Oct 1 – Jan 31 (Christmas / Pasko season). 15–20% of sea freight" },
  { id: "sur-overweight",   name: "Overweight (per kg)",      category: "surcharge", rate: 3,   rate_type: "per_kg",  min_charge: 3,  description: "Per kg over declared box weight limit" },
  { id: "sur-oversize",     name: "Oversize Box",             category: "surcharge", rate: 35,  rate_type: "fixed",   min_charge: 35, description: "Any side exceeds 27 inches" },
  { id: "sur-storage",      name: "Storage / Demurrage",      category: "surcharge", rate: 5,   rate_type: "per_day", min_charge: 5,  description: "Per box/day after 7 free days at origin warehouse" },
  { id: "sur-redelivery",   name: "Re-delivery Fee (PH)",     category: "surcharge", rate: 8,   rate_type: "fixed",   min_charge: 8,  description: "Per failed delivery attempt after first (up to 3 attempts)" },
  { id: "sur-customs",      name: "Customs Clearance Assist", category: "surcharge", rate: 30,  rate_type: "fixed",   min_charge: 30, description: "PH BOC query support; excludes duties & taxes" },
];

export function buildDefaultRates(carrierId: string): BalikbayanRates {
  return {
    carrier_id: carrierId,
    sea_cargo: DEFAULT_SEA_CARGO,
    air_cargo_zones: DEFAULT_AIR_ZONES,
    air_cargo_fixed: DEFAULT_AIR_FIXED,
    ph_delivery_zones: DEFAULT_PH_DELIVERY,
    volumetric_groups: DEFAULT_VOLUMETRIC,
    addons: DEFAULT_ADDONS,
    updated_at: new Date().toISOString(),
  };
}
