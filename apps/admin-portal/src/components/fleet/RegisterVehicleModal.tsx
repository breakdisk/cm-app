"use client";

import { useState, useRef, useEffect } from "react";
import { motion, AnimatePresence } from "framer-motion";
import {
  Truck, Store, CheckCircle2, ChevronRight, Loader2, X, ChevronDown,
} from "lucide-react";
import { GlassCard } from "@/components/ui/glass-card";
import { createFleetApi, type RegisterVehiclePayload, type VehicleType } from "@/lib/api/fleet";
import {
  createVehicleListing,
  type SizeClass,
  type VehicleFeature,
  type CreateListingPayload,
} from "@/lib/api/marketplace";

// ── Vehicle type config (1-to-1 with marketplace SizeClass) ──────────────────

interface TypeConfig {
  label:         string;
  size_class:    SizeClass;
  icon:          string;
  def_features:  VehicleFeature[];
  def_weight_kg: number;
  def_base_php:  number;
  def_km_php:    number;
}

const TYPE_CONFIG: Record<VehicleType, TypeConfig> = {
  Scooter:            { label: "Scooter / Bicycle", size_class: "scooter_bicycle",   icon: "🛵", def_features: [],             def_weight_kg: 30,    def_base_php: 150,  def_km_php: 8  },
  Motorcycle:         { label: "Motorcycle",         size_class: "motorcycle",         icon: "🏍", def_features: [],             def_weight_kg: 80,    def_base_php: 250,  def_km_php: 12 },
  Sedan:              { label: "Sedan / Car",         size_class: "sedan",              icon: "🚗", def_features: [],             def_weight_kg: 300,   def_base_php: 500,  def_km_php: 15 },
  Van:                { label: "Van",                 size_class: "van",                icon: "🚐", def_features: [],             def_weight_kg: 800,   def_base_php: 900,  def_km_php: 18 },
  Truck_1T:           { label: "Truck — 1 Ton",       size_class: "1ton",               icon: "🚛", def_features: [],             def_weight_kg: 1000,  def_base_php: 1200, def_km_php: 22 },
  Truck_3T:           { label: "Truck — 3 Ton",       size_class: "3ton",               icon: "🚛", def_features: ["tail_lift"],   def_weight_kg: 3000,  def_base_php: 2200, def_km_php: 28 },
  Truck_7T:           { label: "Truck — 7 Ton",       size_class: "7ton",               icon: "🚛", def_features: ["tail_lift"],   def_weight_kg: 7000,  def_base_php: 3800, def_km_php: 35 },
  Truck_10T:          { label: "Truck — 10 Ton",      size_class: "10ton",              icon: "🚛", def_features: ["tail_lift"],   def_weight_kg: 10000, def_base_php: 5500, def_km_php: 42 },
  Trailer:            { label: "Trailer",             size_class: "trailer",            icon: "🚚", def_features: ["tail_lift"],   def_weight_kg: 20000, def_base_php: 9000, def_km_php: 55 },
  Refrigerated_Truck: { label: "Refrigerated Truck",  size_class: "refrigerated_truck", icon: "❄️", def_features: ["chiller"],     def_weight_kg: 5000,  def_base_php: 4500, def_km_php: 40 },
  Recovery_Truck:     { label: "Recovery Truck",      size_class: "recovery_truck",     icon: "🔧", def_features: [],             def_weight_kg: 5000,  def_base_php: 3000, def_km_php: 35 },
};

const ALL_TYPES = Object.keys(TYPE_CONFIG) as VehicleType[];

// ── Make / Model data (Philippines logistics market) ──────────────────────────

const MAKES_BY_TYPE: Record<VehicleType, string[]> = {
  Scooter:            ["Honda", "Yamaha", "Suzuki", "Kymco", "SYM", "TVS", "Rusi"],
  Motorcycle:         ["Honda", "Yamaha", "Suzuki", "Kawasaki", "Bajaj", "KTM", "TVS", "Rusi", "Royal Enfield"],
  Sedan:              ["Toyota", "Honda", "Mitsubishi", "Nissan", "Hyundai", "Kia", "Suzuki", "Ford"],
  Van:                ["Toyota", "Nissan", "Mitsubishi", "Ford", "Hyundai", "Kia"],
  Truck_1T:           ["Isuzu", "Mitsubishi", "Toyota", "Foton", "JAC", "Hino", "FAW"],
  Truck_3T:           ["Isuzu", "Mitsubishi", "Hino", "Foton", "JAC", "UD Trucks"],
  Truck_7T:           ["Isuzu", "Mitsubishi Fuso", "Hino", "JAC", "Foton", "UD Trucks"],
  Truck_10T:          ["Isuzu", "Mitsubishi Fuso", "Hino", "Scania", "UD Trucks", "JAC"],
  Trailer:            ["Isuzu", "Mitsubishi Fuso", "Hino", "Scania", "Volvo", "Mack", "UD Trucks"],
  Refrigerated_Truck: ["Isuzu", "Mitsubishi Fuso", "Hino", "JAC", "Foton"],
  Recovery_Truck:     ["Isuzu", "Ford", "Toyota", "Mitsubishi"],
};

const MODELS_BY_MAKE: Record<string, Partial<Record<VehicleType, string[]>>> = {
  Toyota:          { Sedan: ["Vios", "Corolla Altis", "Corolla Cross", "Camry", "Yaris"], Van: ["Hiace Commuter", "Hiace GL Grandia", "Hiace Super Grandia"], Truck_1T: ["Dyna 100", "Dyna 150"], Recovery_Truck: ["Land Cruiser"] },
  Nissan:          { Sedan: ["Almera", "Sylphy", "Sentra"], Van: ["Urvan", "NV350 Urvan"] },
  Honda:           { Scooter: ["Click 125i", "ADV 160", "PCX 160", "BeAT 125"], Motorcycle: ["XRM 125", "TMX 125 Alpha", "CB300R", "CRF300L"], Sedan: ["Brio", "City", "Civic", "Accord", "Jazz"], Van: ["Odyssey"] },
  Yamaha:          { Scooter: ["Mio i 125", "NMAX 155", "Fino 125", "Aerox 155"], Motorcycle: ["MT-15", "R15", "XSR 155", "Tenere 700"] },
  Suzuki:          { Scooter: ["Skydrive 125", "Address 115"], Motorcycle: ["Raider R150", "GSX-R150"], Sedan: ["Dzire", "Ciaz"], Van: ["APV"] },
  Kawasaki:        { Motorcycle: ["Barako II", "Rouser NS 125", "Ninja 400", "Z400"] },
  Bajaj:           { Motorcycle: ["Pulsar NS160", "Pulsar NS200", "Boxer 150"] },
  KTM:             { Motorcycle: ["Duke 250", "Duke 390", "RC 200"] },
  TVS:             { Scooter: ["Dazz 125i", "Jupiter"], Motorcycle: ["Apache RTR 160 4V", "Apache RTR 200 4V"] },
  Rusi:            { Scooter: ["Maxi 150"], Motorcycle: ["RC150", "GT125"] },
  Kymco:           { Scooter: ["Like 150i", "People S 150"] },
  SYM:             { Scooter: ["Jet X 150", "Symphony SR 150"] },
  "Royal Enfield": { Motorcycle: ["Meteor 350", "Classic 350", "Hunter 350"] },
  Mitsubishi:      { Sedan: ["Mirage", "Mirage G4", "Lancer"], Van: ["L300", "L300 FB", "Delica"], Truck_1T: ["L300 Cab", "Minicab"], Truck_3T: ["Canter FE71", "Canter FE84"], Truck_7T: ["Fuso Fighter FK"], Truck_10T: ["Fuso Super Great FV"], Refrigerated_Truck: ["Canter Reefer", "Fuso Fighter Reefer"], Recovery_Truck: ["Fuso Fighter"] },
  Isuzu:           { Truck_1T: ["D-Max Cab", "Crosswind"], Truck_3T: ["ELF NHR 55", "ELF NKR 55"], Truck_7T: ["NPR 4HK1", "NQR 75"], Truck_10T: ["FRR 90", "FTR 33", "FVM 34"], Trailer: ["GVR", "FVZ 360"], Refrigerated_Truck: ["ELF Reefer", "NPR Reefer", "NQR Reefer"], Recovery_Truck: ["ELF Recovery"] },
  Hino:            { Truck_3T: ["300 WU300", "300 WU710"], Truck_7T: ["500 FC9J", "500 FG8J"], Truck_10T: ["700 GH8J", "700 GT8J"], Trailer: ["700 FL8J"], Refrigerated_Truck: ["300 Reefer", "500 Reefer"] },
  Ford:            { Sedan: ["Fiesta", "Focus"], Van: ["Transit", "Transit Custom"], Recovery_Truck: ["Ranger Recovery"] },
  Hyundai:         { Sedan: ["Accent", "Elantra", "Sonata"], Van: ["Starex", "H-1 Van"] },
  Kia:             { Sedan: ["Soluto", "Stinger"], Van: ["Carnival", "Grand Carnival"] },
  Foton:           { Truck_1T: ["Gratour TM", "View Transvan"], Truck_3T: ["Aumark S", "Aumark ES"], Truck_7T: ["Auman GT", "Auman EST"], Refrigerated_Truck: ["Aumark Reefer"] },
  JAC:             { Truck_1T: ["X200 Cab", "X350"], Truck_3T: ["X500", "N75"], Truck_7T: ["N120", "N160"], Truck_10T: ["N160", "N190"], Refrigerated_Truck: ["N75 Reefer", "N120 Reefer"] },
  Scania:          { Truck_10T: ["P 320", "R 410"], Trailer: ["R 450", "S 500"] },
  Volvo:           { Trailer: ["FH 460", "FH 500"] },
  Mack:            { Trailer: ["Anthem", "Pinnacle"] },
  FAW:             { Truck_1T: ["Sirius CA1032", "Tiger CA1040"] },
  "Mitsubishi Fuso": { Truck_7T: ["Fighter FK 61F", "Fighter FK 65F"], Truck_10T: ["Super Great FV 50JMX"], Trailer: ["Super Great FV 54JHX"], Refrigerated_Truck: ["Canter Reefer", "Fighter Reefer"] },
  "UD Trucks":     { Truck_3T: ["Condor MK11E"], Truck_7T: ["Condor PK9E"], Truck_10T: ["Quester CWE"], Trailer: ["Quester CGE"] },
};

// ── Feature metadata ──────────────────────────────────────────────────────────

const FEATURE_META: Record<VehicleFeature, { label: string; activeClass: string; inactiveClass: string }> = {
  tail_lift: {
    label:        "Tail-lift Gate",
    activeClass:  "border-amber-400/50 bg-amber-400/15 text-amber-300",
    inactiveClass:"border-glass-border  bg-glass-100   text-white/40",
  },
  chiller: {
    label:        "Chiller (0–8 °C)",
    activeClass:  "border-cyan-neon/50  bg-cyan-neon/15  text-cyan-neon",
    inactiveClass:"border-glass-border  bg-glass-100     text-white/40",
  },
  freezer: {
    label:        "Freezer (−18 °C)",
    activeClass:  "border-blue-400/50  bg-blue-400/15  text-blue-300",
    inactiveClass:"border-glass-border bg-glass-100   text-white/40",
  },
};

const FEATURE_ICONS: Record<VehicleFeature, string> = {
  tail_lift: "⬆",
  chiller:   "🌡",
  freezer:   "❄️",
};

// ── Types ─────────────────────────────────────────────────────────────────────

interface Step1Fields {
  plate_number:  string;
  vehicle_type:  VehicleType;
  make:          string;
  model:         string;
  year:          string;
  color:         string;
}

interface Step2Fields {
  list_on_marketplace: boolean;
  features:            VehicleFeature[];
  max_weight_kg:       string;
  base_price_php:      string;
  per_km_php:          string;
  service_area_label:  string;
  idle_from:           string;
  idle_until:          string;
}

interface RegisterResult {
  vehicle_id: string;
  plate:      string;
  listed:     boolean;
}

interface Props {
  open:      boolean;
  onClose:   () => void;
  onSuccess: () => void;
}

// ── Constants ─────────────────────────────────────────────────────────────────

const STEP_LABELS = ["Vehicle Identity", "Marketplace Listing"];
const THIS_YEAR   = new Date().getFullYear();

function defaultStep2(vtype: VehicleType = "Van"): Step2Fields {
  const cfg = TYPE_CONFIG[vtype];
  const now = new Date();
  const end = new Date(now.getTime() + 6 * 3_600_000);
  return {
    list_on_marketplace: true,
    features:            [...cfg.def_features],
    max_weight_kg:       String(cfg.def_weight_kg),
    base_price_php:      String(cfg.def_base_php),
    per_km_php:          String(cfg.def_km_php),
    service_area_label:  "Metro Manila",
    idle_from:           now.toISOString().slice(0, 16),
    idle_until:          end.toISOString().slice(0, 16),
  };
}

// ── Shared styles ─────────────────────────────────────────────────────────────

const inputCls =
  "w-full rounded-lg border border-glass-border bg-glass-100 px-3 py-2.5 text-sm text-white " +
  "placeholder:text-white/25 outline-none focus:border-cyan-neon/50 focus:ring-1 focus:ring-cyan-neon/20 " +
  "transition-all font-mono";

const labelCls = "block text-xs text-white/50 mb-1.5 font-medium";

// ── FeatureBadge (small inline tag) ──────────────────────────────────────────

function FeatureBadge({ feature }: { feature: VehicleFeature }) {
  const palette: Record<VehicleFeature, string> = {
    tail_lift: "text-amber-400 bg-amber-400/10 border-amber-400/20",
    chiller:   "text-cyan-400  bg-cyan-400/10  border-cyan-400/20",
    freezer:   "text-blue-400  bg-blue-400/10  border-blue-400/20",
  };
  return (
    <span className={`inline-flex items-center gap-0.5 text-[10px] font-mono px-1.5 py-0.5 rounded border ${palette[feature]}`}>
      {FEATURE_ICONS[feature]} {FEATURE_META[feature].label.split(" ")[0]}
    </span>
  );
}

// ── VehicleTypePicker ─────────────────────────────────────────────────────────

function VehicleTypePicker({ value, onChange }: { value: VehicleType; onChange: (v: VehicleType) => void }) {
  const [open, setOpen] = useState(false);
  const ref             = useRef<HTMLDivElement>(null);
  const cfg             = TYPE_CONFIG[value];

  useEffect(() => {
    function handler(e: MouseEvent) {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    }
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, []);

  return (
    <div ref={ref} className="relative">
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        className={`${inputCls} flex items-center justify-between gap-2 cursor-pointer`}
      >
        <span className="flex items-center gap-2 min-w-0">
          <span className="text-base leading-none flex-shrink-0">{cfg.icon}</span>
          <span className="truncate">{cfg.label}</span>
          {cfg.def_features.length > 0 && (
            <span className="hidden sm:flex gap-1 flex-shrink-0">
              {cfg.def_features.map((f) => <FeatureBadge key={f} feature={f} />)}
            </span>
          )}
        </span>
        <ChevronDown
          size={12}
          className={`text-white/30 flex-shrink-0 transition-transform duration-200 ${open ? "rotate-180" : ""}`}
        />
      </button>

      <AnimatePresence>
        {open && (
          <motion.div
            initial={{ opacity: 0, y: -6, scaleY: 0.94 }}
            animate={{ opacity: 1, y: 0,  scaleY: 1    }}
            exit={{    opacity: 0, y: -6, scaleY: 0.94 }}
            transition={{ duration: 0.13 }}
            style={{ transformOrigin: "top" }}
            className="absolute z-50 top-full mt-1 w-full rounded-lg border border-glass-border bg-[#080c1a] shadow-2xl shadow-black/60 overflow-hidden"
          >
            <div className="max-h-64 overflow-y-auto divide-y divide-glass-border/50">
              {ALL_TYPES.map((type) => {
                const c = TYPE_CONFIG[type];
                const active = value === type;
                return (
                  <button
                    key={type}
                    type="button"
                    onClick={() => { onChange(type); setOpen(false); }}
                    className={`w-full text-left px-3 py-2.5 flex items-center justify-between gap-2 transition-colors
                      ${active ? "bg-cyan-neon/10 text-white" : "hover:bg-glass-100 text-white/70 hover:text-white"}`}
                  >
                    <span className="flex items-center gap-2.5">
                      <span className="text-sm leading-none flex-shrink-0">{c.icon}</span>
                      <span className="text-sm font-mono">{c.label}</span>
                    </span>
                    <span className="flex items-center gap-1 flex-shrink-0">
                      {c.def_features.map((f) => <FeatureBadge key={f} feature={f} />)}
                      {active && (
                        <span className="w-1.5 h-1.5 rounded-full bg-cyan-neon ml-1" />
                      )}
                    </span>
                  </button>
                );
              })}
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}

// ── ComboboxInput ─────────────────────────────────────────────────────────────

interface ComboboxProps {
  value:       string;
  onChange:    (v: string) => void;
  options:     string[];
  placeholder: string;
  disabled?:   boolean;
}

function ComboboxInput({ value, onChange, options, placeholder, disabled }: ComboboxProps) {
  const [open,  setOpen]  = useState(false);
  const [query, setQuery] = useState(value);
  const ref               = useRef<HTMLDivElement>(null);

  useEffect(() => { setQuery(value); }, [value]);

  useEffect(() => {
    function handler(e: MouseEvent) {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    }
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, []);

  const filtered = options.filter((o) => o.toLowerCase().includes(query.toLowerCase()));

  function select(opt: string) {
    onChange(opt);
    setQuery(opt);
    setOpen(false);
  }

  return (
    <div ref={ref} className="relative">
      <div className="relative">
        <input
          className={`${inputCls} pr-8`}
          value={query}
          disabled={disabled}
          placeholder={placeholder}
          onChange={(e) => {
            setQuery(e.target.value);
            onChange(e.target.value);
            if (!open) setOpen(true);
          }}
          onFocus={() => setOpen(true)}
        />
        <button
          type="button"
          tabIndex={-1}
          onClick={() => setOpen((o) => !o)}
          className="absolute right-2.5 top-1/2 -translate-y-1/2 text-white/25 hover:text-white/60 transition-colors"
        >
          <ChevronDown size={11} className={`transition-transform duration-200 ${open ? "rotate-180" : ""}`} />
        </button>
      </div>

      <AnimatePresence>
        {open && (
          <motion.div
            initial={{ opacity: 0, y: -4, scaleY: 0.95 }}
            animate={{ opacity: 1, y: 0,  scaleY: 1    }}
            exit={{    opacity: 0, y: -4, scaleY: 0.95 }}
            transition={{ duration: 0.12 }}
            style={{ transformOrigin: "top" }}
            className="absolute z-50 top-full mt-1 w-full rounded-lg border border-glass-border bg-[#080c1a] shadow-xl shadow-black/50 overflow-hidden"
          >
            <div className="max-h-44 overflow-y-auto">
              {filtered.length === 0 ? (
                <div className="px-3 py-2.5 text-xs text-white/30 font-mono italic">
                  Custom — will be saved as typed
                </div>
              ) : (
                filtered.map((opt) => (
                  <button
                    key={opt}
                    type="button"
                    onMouseDown={(e) => { e.preventDefault(); select(opt); }}
                    className={`w-full text-left px-3 py-2 text-sm font-mono transition-colors
                      ${value === opt
                        ? "bg-cyan-neon/10 text-cyan-neon"
                        : "hover:bg-glass-100 text-white/65 hover:text-white"}`}
                  >
                    {opt}
                  </button>
                ))
              )}
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}

// ── Component ──────────────────────────────────────────────────────────────────

export function RegisterVehicleModal({ open, onClose, onSuccess }: Props) {
  const [step,    setStep]    = useState<1 | 2 | 3>(1);
  const [loading, setLoading] = useState(false);
  const [error,   setError]   = useState<string | null>(null);

  const [s1, setS1] = useState<Step1Fields>({
    plate_number: "", vehicle_type: "Van",
    make: "", model: "", year: String(THIS_YEAR), color: "",
  });
  const [s2,     setS2]     = useState<Step2Fields>(() => defaultStep2("Van"));
  const [result, setResult] = useState<RegisterResult | null>(null);

  function reset() {
    setStep(1); setLoading(false); setError(null); setResult(null);
    setS1({ plate_number: "", vehicle_type: "Van", make: "", model: "", year: String(THIS_YEAR), color: "" });
    setS2(defaultStep2("Van"));
  }

  function handleClose() { reset(); onClose(); }

  function handleTypeChange(type: VehicleType) {
    setS1((p) => ({ ...p, vehicle_type: type, make: "", model: "" }));
    const cfg = TYPE_CONFIG[type];
    setS2((p) => ({
      ...p,
      features:      [...cfg.def_features],
      max_weight_kg: String(cfg.def_weight_kg),
      base_price_php:String(cfg.def_base_php),
      per_km_php:    String(cfg.def_km_php),
    }));
  }

  function handleMakeChange(make: string) {
    setS1((p) => ({ ...p, make, model: "" }));
  }

  function toggleFeature(f: VehicleFeature) {
    setS2((p) => ({
      ...p,
      features: p.features.includes(f)
        ? p.features.filter((x) => x !== f)
        : [...p.features, f],
    }));
  }

  // ── Step 1: Register vehicle in fleet service ────────────────────────────────
  async function handleStep1(e: React.FormEvent) {
    e.preventDefault();
    const year = parseInt(s1.year, 10);
    if (!s1.plate_number.trim())                           { setError("Plate number is required.");                       return; }
    if (!s1.make.trim() || !s1.model.trim())               { setError("Make and model are required.");                    return; }
    if (isNaN(year) || year < 1990 || year > THIS_YEAR + 1){ setError(`Year must be between 1990 and ${THIS_YEAR + 1}.`); return; }
    if (!s1.color.trim())                                  { setError("Color is required.");                              return; }

    setError(null); setLoading(true);
    try {
      const api     = createFleetApi();
      const payload: RegisterVehiclePayload = {
        plate_number: s1.plate_number.toUpperCase().trim(),
        vehicle_type: s1.vehicle_type,
        make:         s1.make.trim(),
        model:        s1.model.trim(),
        year,
        color:        s1.color.trim(),
      };
      const res = await api.registerVehicle(payload);
      setResult({ vehicle_id: res.data.id, plate: res.data.plate, listed: false });
      setStep(2);
    } catch (err: unknown) {
      const e = err as { message?: string };
      setError(e.message ?? "Failed to register vehicle. Check plate uniqueness.");
    } finally {
      setLoading(false);
    }
  }

  // ── Step 2: Optionally list on marketplace ───────────────────────────────────
  async function handleStep2(e: React.FormEvent) {
    e.preventDefault();
    if (!result) return;

    if (!s2.list_on_marketplace) { setStep(3); onSuccess(); return; }

    const weightKg  = parseFloat(s2.max_weight_kg);
    const baseCents = Math.round(parseFloat(s2.base_price_php)  * 100);
    const kmCents   = Math.round(parseFloat(s2.per_km_php)      * 100);

    if (isNaN(weightKg)  || weightKg  <= 0) { setError("Max weight must be a positive number.");  return; }
    if (isNaN(baseCents) || baseCents <= 0) { setError("Base price must be a positive number.");   return; }
    if (isNaN(kmCents)   || kmCents   <= 0) { setError("Per-km rate must be a positive number."); return; }
    if (!s2.service_area_label.trim())      { setError("Service area is required.");               return; }
    if (new Date(s2.idle_until) <= new Date(s2.idle_from)) {
      setError("'Available until' must be after 'Available from'."); return;
    }

    setError(null); setLoading(true);
    try {
      const listing: CreateListingPayload = {
        vehicle_plate:                result.plate,
        size_class:                   TYPE_CONFIG[s1.vehicle_type].size_class,
        features:                     s2.features,
        max_weight_kg:                weightKg,
        max_volume_m3:                null,
        base_price_cents:             baseCents,
        per_km_cents:                 kmCents,
        per_kg_cents:                 null,
        service_area_label:           s2.service_area_label.trim(),
        idle_from:                    new Date(s2.idle_from).toISOString(),
        idle_until:                   new Date(s2.idle_until).toISOString(),
        status:                       "active",
        carrier_response_window_mins: 15,
      };
      await createVehicleListing(listing);
      setResult((p) => p ? { ...p, listed: true } : null);
      setStep(3);
      onSuccess();
    } catch (err: unknown) {
      const e = err as { message?: string };
      setError(
        e.message?.includes("carrier") || e.message?.includes("401") || e.message?.includes("403")
          ? "Vehicle registered. Marketplace listing requires carrier portal access — list it from the Partner Portal."
          : (e.message ?? "Marketplace listing failed. The vehicle was registered successfully."),
      );
    } finally {
      setLoading(false);
    }
  }

  if (!open) return null;

  const vCfg          = TYPE_CONFIG[s1.vehicle_type];
  const makeOptions   = MAKES_BY_TYPE[s1.vehicle_type] ?? [];
  const modelOptions  = (MODELS_BY_MAKE[s1.make]?.[s1.vehicle_type] ?? []);

  return (
    <AnimatePresence>
      {open && (
        <motion.div
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          className="fixed inset-0 z-50 flex items-center justify-center p-4"
          style={{ background: "rgba(5,8,16,0.85)", backdropFilter: "blur(8px)" }}
          onClick={(e) => { if (e.target === e.currentTarget) handleClose(); }}
        >
          <motion.div
            initial={{ scale: 0.95, opacity: 0, y: 16 }}
            animate={{ scale: 1, opacity: 1, y: 0 }}
            exit={{ scale: 0.95, opacity: 0, y: 16 }}
            transition={{ type: "spring", duration: 0.4 }}
            className="w-full max-w-lg max-h-[calc(100vh-32px)] overflow-y-auto"
          >
            <GlassCard glow="cyan" className="relative">

              {/* ── Header ───────────────────────────────────────────────── */}
              <div className="flex items-center justify-between mb-6">
                <div>
                  <h2 className="font-heading text-lg font-bold text-white">Register Vehicle</h2>
                  <p className="text-xs text-white/40 mt-0.5">
                    {step < 3
                      ? `Step ${step} of 2 — ${STEP_LABELS[step - 1]}`
                      : "Vehicle registered"}
                  </p>
                </div>
                <button
                  onClick={handleClose}
                  className="rounded-lg border border-glass-border bg-glass-100 p-1.5 text-white/40 hover:text-white transition-colors"
                >
                  <X size={14} />
                </button>
              </div>

              {/* ── Step progress ─────────────────────────────────────────── */}
              {step < 3 && (
                <div className="flex gap-1.5 mb-6">
                  {[1, 2].map((s) => (
                    <div
                      key={s}
                      className={`h-1 flex-1 rounded-full transition-all duration-500 ${
                        s < step ? "bg-cyan-neon" : s === step ? "bg-cyan-neon/60" : "bg-glass-300"
                      }`}
                    />
                  ))}
                </div>
              )}

              {/* ── Step 1: Vehicle Identity ──────────────────────────────── */}
              {step === 1 && (
                <form onSubmit={handleStep1} className="space-y-4">
                  <div className="flex items-center gap-2 mb-2">
                    <Truck size={14} className="text-cyan-neon" />
                    <span className="text-xs font-semibold text-cyan-neon uppercase tracking-wider">Vehicle Details</span>
                  </div>

                  {/* Plate + Type */}
                  <div className="grid grid-cols-2 gap-3">
                    <div>
                      <label className={labelCls}>Plate Number</label>
                      <input
                        className={inputCls}
                        value={s1.plate_number}
                        onChange={(e) => setS1((p) => ({ ...p, plate_number: e.target.value.toUpperCase() }))}
                        placeholder="ABC-1234"
                        required
                      />
                    </div>
                    <div>
                      <label className={labelCls}>Year</label>
                      <input
                        className={inputCls}
                        type="number"
                        min="1990"
                        max={THIS_YEAR + 1}
                        value={s1.year}
                        onChange={(e) => setS1((p) => ({ ...p, year: e.target.value }))}
                        placeholder={String(THIS_YEAR)}
                        required
                      />
                    </div>
                  </div>

                  {/* Vehicle Type — full SizeClass list with feature badges */}
                  <div>
                    <label className={labelCls}>Vehicle Type</label>
                    <VehicleTypePicker value={s1.vehicle_type} onChange={handleTypeChange} />
                  </div>

                  {/* Make + Model — combobox with brand pre-fill */}
                  <div className="grid grid-cols-2 gap-3">
                    <div>
                      <label className={labelCls}>Make</label>
                      <ComboboxInput
                        value={s1.make}
                        onChange={handleMakeChange}
                        options={makeOptions}
                        placeholder={makeOptions[0] ?? "e.g. Toyota"}
                      />
                    </div>
                    <div>
                      <label className={labelCls}>Model</label>
                      <ComboboxInput
                        value={s1.model}
                        onChange={(v) => setS1((p) => ({ ...p, model: v }))}
                        options={modelOptions}
                        placeholder={modelOptions[0] ?? "e.g. Hiace"}
                        disabled={!s1.make.trim()}
                      />
                    </div>
                  </div>

                  {/* Color */}
                  <div>
                    <label className={labelCls}>Color</label>
                    <input
                      className={inputCls}
                      value={s1.color}
                      onChange={(e) => setS1((p) => ({ ...p, color: e.target.value }))}
                      placeholder="White"
                      required
                    />
                  </div>

                  {error && (
                    <p className="rounded-lg bg-red-500/10 border border-red-500/20 px-3 py-2 text-xs text-red-400 font-mono">{error}</p>
                  )}

                  <button
                    type="submit"
                    disabled={loading}
                    className="w-full flex items-center justify-center gap-2 rounded-lg bg-cyan-neon/10 border border-cyan-neon/30 px-4 py-2.5 text-sm font-semibold text-cyan-neon hover:bg-cyan-neon/20 transition-all disabled:opacity-50"
                  >
                    {loading ? <Loader2 size={14} className="animate-spin" /> : <ChevronRight size={14} />}
                    {loading ? "Registering…" : "Next — Marketplace Listing"}
                  </button>
                </form>
              )}

              {/* ── Step 2: Marketplace Listing ───────────────────────────── */}
              {step === 2 && result && (
                <form onSubmit={handleStep2} className="space-y-4">
                  <div className="flex items-center gap-2 mb-2">
                    <Store size={14} className="text-purple-plasma" />
                    <span className="text-xs font-semibold text-purple-plasma uppercase tracking-wider">Marketplace Listing</span>
                  </div>

                  {/* Vehicle confirmation pill */}
                  <div className="rounded-lg border border-cyan-neon/20 bg-cyan-neon/5 px-3 py-2 flex items-center justify-between gap-2">
                    <div className="flex items-center gap-2 min-w-0">
                      <span className="text-sm flex-shrink-0">{vCfg.icon}</span>
                      <span className="text-xs font-mono text-white/60 truncate">
                        <span className="text-cyan-neon font-semibold">{result.plate}</span>
                        {s1.make && s1.model && (
                          <span className="text-white/40"> · {s1.make} {s1.model}</span>
                        )}
                      </span>
                    </div>
                    <span className="text-[10px] font-mono px-1.5 py-0.5 rounded border border-cyan-neon/20 bg-cyan-neon/5 text-cyan-neon/70 flex-shrink-0">
                      {vCfg.label}
                    </span>
                  </div>

                  {/* Toggle */}
                  <label className="flex items-center gap-3 cursor-pointer group">
                    <button
                      type="button"
                      role="switch"
                      aria-checked={s2.list_on_marketplace}
                      onClick={() => setS2((p) => ({ ...p, list_on_marketplace: !p.list_on_marketplace }))}
                      className={`relative w-10 h-5 rounded-full flex-shrink-0 transition-colors ${
                        s2.list_on_marketplace ? "bg-purple-plasma" : "bg-glass-300"
                      }`}
                    >
                      <span
                        className={`absolute top-0.5 left-0.5 w-4 h-4 rounded-full bg-white transition-transform ${
                          s2.list_on_marketplace ? "translate-x-5" : "translate-x-0"
                        }`}
                      />
                    </button>
                    <span className="text-sm text-white/70 group-hover:text-white transition-colors">
                      List this vehicle on the Marketplace now
                    </span>
                  </label>

                  {/* Listing fields */}
                  {s2.list_on_marketplace && (
                    <div className="space-y-3 pt-1">
                      <div className="rounded-lg border border-purple-plasma/20 bg-purple-plasma/5 px-3 py-2 text-[11px] font-mono text-white/40">
                        🏢 Listed under: LogisticOS Platform Fleet · auto-assigned carrier
                      </div>

                      {/* Vehicle Features */}
                      <div>
                        <label className={labelCls}>Vehicle Features</label>
                        <div className="flex gap-2 flex-wrap">
                          {(["tail_lift", "chiller", "freezer"] as VehicleFeature[]).map((f) => {
                            const meta    = FEATURE_META[f];
                            const checked = s2.features.includes(f);
                            return (
                              <button
                                key={f}
                                type="button"
                                onClick={() => toggleFeature(f)}
                                className={`flex items-center gap-1.5 px-3 py-1.5 rounded-lg border text-xs font-mono transition-all ${
                                  checked ? meta.activeClass : meta.inactiveClass
                                }`}
                              >
                                <span>{FEATURE_ICONS[f]}</span>
                                <span>{meta.label}</span>
                                {checked && (
                                  <span className="ml-0.5 opacity-70">✓</span>
                                )}
                              </button>
                            );
                          })}
                        </div>
                      </div>

                      {/* Weight + Pricing */}
                      <div className="grid grid-cols-3 gap-3">
                        <div>
                          <label className={labelCls}>Max Weight (kg)</label>
                          <input
                            className={inputCls}
                            type="number"
                            min="1"
                            value={s2.max_weight_kg}
                            onChange={(e) => setS2((p) => ({ ...p, max_weight_kg: e.target.value }))}
                            placeholder="800"
                          />
                        </div>
                        <div>
                          <label className={labelCls}>Base Price (₱)</label>
                          <input
                            className={inputCls}
                            type="number"
                            min="1"
                            step="0.01"
                            value={s2.base_price_php}
                            onChange={(e) => setS2((p) => ({ ...p, base_price_php: e.target.value }))}
                            placeholder="900.00"
                          />
                        </div>
                        <div>
                          <label className={labelCls}>Per-km (₱)</label>
                          <input
                            className={inputCls}
                            type="number"
                            min="0.01"
                            step="0.01"
                            value={s2.per_km_php}
                            onChange={(e) => setS2((p) => ({ ...p, per_km_php: e.target.value }))}
                            placeholder="18.00"
                          />
                        </div>
                      </div>

                      <div>
                        <label className={labelCls}>Service Area</label>
                        <input
                          className={inputCls}
                          value={s2.service_area_label}
                          onChange={(e) => setS2((p) => ({ ...p, service_area_label: e.target.value }))}
                          placeholder="Metro Manila"
                        />
                      </div>

                      <div className="grid grid-cols-2 gap-3">
                        <div>
                          <label className={labelCls}>Available From</label>
                          <input
                            className={inputCls}
                            type="datetime-local"
                            value={s2.idle_from}
                            onChange={(e) => setS2((p) => ({ ...p, idle_from: e.target.value }))}
                          />
                        </div>
                        <div>
                          <label className={labelCls}>Available Until</label>
                          <input
                            className={inputCls}
                            type="datetime-local"
                            value={s2.idle_until}
                            onChange={(e) => setS2((p) => ({ ...p, idle_until: e.target.value }))}
                          />
                        </div>
                      </div>
                    </div>
                  )}

                  {error && (
                    <p className="rounded-lg bg-red-500/10 border border-red-500/20 px-3 py-2 text-xs text-red-400 font-mono">{error}</p>
                  )}

                  <div className="flex gap-2">
                    <button
                      type="button"
                      onClick={() => { setStep(1); setError(null); }}
                      className="flex-1 rounded-lg border border-glass-border bg-glass-100 px-4 py-2.5 text-sm text-white/60 hover:text-white transition-colors"
                    >
                      Back
                    </button>
                    <button
                      type="submit"
                      disabled={loading}
                      className="flex-[2] flex items-center justify-center gap-2 rounded-lg bg-purple-plasma/10 border border-purple-plasma/30 px-4 py-2.5 text-sm font-semibold text-purple-plasma hover:bg-purple-plasma/20 transition-all disabled:opacity-50"
                    >
                      {loading ? (
                        <Loader2 size={14} className="animate-spin" />
                      ) : (
                        <ChevronRight size={14} />
                      )}
                      {loading
                        ? (s2.list_on_marketplace ? "Publishing…" : "Saving…")
                        : (s2.list_on_marketplace ? "Register & List" : "Register Only")}
                    </button>
                  </div>
                </form>
              )}

              {/* ── Step 3: Success ───────────────────────────────────────── */}
              {step === 3 && result && (
                <div className="space-y-4">
                  <div className="flex flex-col items-center text-center gap-2 py-2">
                    <div
                      className={`h-12 w-12 rounded-full flex items-center justify-center border ${
                        result.listed
                          ? "bg-green-signal/10 border-green-signal/30"
                          : "bg-cyan-neon/10   border-cyan-neon/30"
                      }`}
                    >
                      <CheckCircle2
                        size={24}
                        className={result.listed ? "text-green-signal" : "text-cyan-neon"}
                      />
                    </div>
                    <p className="font-heading font-bold text-white">
                      {result.listed ? "Vehicle Registered & Listed!" : "Vehicle Registered"}
                    </p>
                    <p className="text-xs text-white/40">
                      {result.listed
                        ? `${result.plate} is now in your fleet and live on the marketplace.`
                        : `${result.plate} is in your fleet. You can list it on the Marketplace later.`}
                    </p>
                  </div>

                  <div className="rounded-lg border border-glass-border bg-glass-100 p-3 space-y-2">
                    <div className="flex justify-between">
                      <span className="text-xs text-white/40 font-mono">Plate</span>
                      <span className="text-xs text-cyan-neon font-mono font-bold">{result.plate}</span>
                    </div>
                    <div className="flex justify-between">
                      <span className="text-xs text-white/40 font-mono">Type</span>
                      <span className="text-xs text-white/70 font-mono">{vCfg.icon} {vCfg.label}</span>
                    </div>
                    {s1.make && (
                      <div className="flex justify-between">
                        <span className="text-xs text-white/40 font-mono">Vehicle</span>
                        <span className="text-xs text-white/70 font-mono">{s1.make} {s1.model}</span>
                      </div>
                    )}
                    <div className="flex justify-between">
                      <span className="text-xs text-white/40 font-mono">Fleet Status</span>
                      <span className="text-xs text-green-signal font-mono font-semibold">Active</span>
                    </div>
                    {result.listed && (
                      <>
                        <div className="flex justify-between">
                          <span className="text-xs text-white/40 font-mono">Marketplace</span>
                          <span className="text-xs text-purple-plasma font-mono font-semibold">Listed · Active</span>
                        </div>
                        {s2.features.length > 0 && (
                          <div className="flex justify-between items-center">
                            <span className="text-xs text-white/40 font-mono">Features</span>
                            <span className="flex gap-1">
                              {s2.features.map((f) => <FeatureBadge key={f} feature={f} />)}
                            </span>
                          </div>
                        )}
                      </>
                    )}
                  </div>

                  <div className="flex gap-2">
                    {result.listed && (
                      <a
                        href="/admin/marketplace"
                        className="flex-1 flex items-center justify-center rounded-lg border border-purple-plasma/30 bg-purple-plasma/10 px-4 py-2.5 text-xs font-semibold text-purple-plasma hover:bg-purple-plasma/20 transition-all"
                      >
                        View in Marketplace
                      </a>
                    )}
                    <button
                      onClick={handleClose}
                      className={`flex-1 rounded-lg border px-4 py-2.5 text-sm font-semibold transition-all ${
                        result.listed
                          ? "border-green-signal/30 bg-green-signal/10 text-green-signal hover:bg-green-signal/20"
                          : "border-cyan-neon/30   bg-cyan-neon/10   text-cyan-neon   hover:bg-cyan-neon/20"
                      }`}
                    >
                      Done
                    </button>
                  </div>
                </div>
              )}

            </GlassCard>
          </motion.div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}
