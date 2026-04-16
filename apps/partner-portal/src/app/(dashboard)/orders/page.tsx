"use client";

import { useState, useEffect, useCallback } from "react";
import { motion, AnimatePresence } from "framer-motion";
import {
  Package, Globe, MapPin, Clock, Zap, User, Truck,
  CheckCircle2, AlertCircle, RefreshCw, X, ChevronRight,
  Bot, Navigation, Phone, ArrowRight, Banknote, Weight, Car,
} from "lucide-react";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { LiveMetric } from "@/components/ui/live-metric";
import { variants } from "@/lib/design-system/tokens";
import { cn } from "@/lib/design-system/cn";
import { authFetch } from "@/lib/auth/auth-fetch";

// ── API helpers ───────────────────────────────────────────────────────────────

const ORDER_INTAKE_URL = process.env.NEXT_PUBLIC_ORDER_INTAKE_URL ?? "http://localhost:8004";
const DISPATCH_URL     = process.env.NEXT_PUBLIC_DISPATCH_URL     ?? "http://localhost:8005";

// ── Types ─────────────────────────────────────────────────────────────────────

type OrderStatus   = "unassigned" | "assigning" | "assigned" | "picked_up";
type OrderType     = "local" | "balikbayan";
type FreightMode   = "sea" | "air";
type DriverStatus  = "available" | "on_route" | "at_pickup";
type VehicleClass  = "motorcycle" | "sedan" | "van";

interface IncomingOrder {
  id:           string;
  awb:          string;
  type:         OrderType;
  freightMode?: FreightMode;
  status:       OrderStatus;
  assignedTo?:  string;
  // Sender
  senderName:   string;
  pickupAddr:   string;
  pickupCity:   string;
  // Receiver
  receiverName: string;
  destAddr:     string;
  destCity:     string;
  destCountry?: string;
  // Package
  description:  string;
  weight:       string;
  isCOD:        boolean;
  codAmount?:   number;
  totalFee:     number;
  bookedAt:     string;
  etaPickup?:   string;
}

interface Driver {
  id:           string;
  name:         string;
  phone:        string;
  vehicle:      string;
  vehicleClass: VehicleClass;
  status:       DriverStatus;
  tasksToday:   number;
  distanceKm:   number;
  etaMinutes:   number;
  isAiPick:     boolean;
}

// ── Mock data ─────────────────────────────────────────────────────────────────

const MOCK_ORDERS: IncomingOrder[] = [
  {
    id: "o1", awb: "CM-PH1-S0000010A", type: "local", status: "unassigned",
    senderName: "Fatima Al-Rashid",
    pickupAddr: "Bldg 7, Al Quoz Industrial Area", pickupCity: "Dubai",
    receiverName: "Maria Santos",
    destAddr: "123 Kalayaan Ave", destCity: "Pasig City",
    description: "Clothes & personal items", weight: "3.5",
    isCOD: false, totalFee: 165,
    bookedAt: "2 min ago",
  },
  {
    id: "o2", awb: "CM-PH1-B0000011B", type: "balikbayan", freightMode: "sea", status: "unassigned",
    senderName: "Ahmed Hassan",
    pickupAddr: "Villa 12, Jumeirah 2", pickupCity: "Dubai",
    receiverName: "Lourdes Hassan",
    destAddr: "45 Magsaysay Blvd", destCity: "Quezon City", destCountry: "PH",
    description: "Balikbayan Box — food, clothes, gadgets", weight: "22",
    isCOD: false, totalFee: 720,
    bookedAt: "8 min ago",
  },
  {
    id: "o3", awb: "CM-PH1-S0000012C", type: "local", status: "assigning",
    senderName: "Grace Villanueva",
    pickupAddr: "Unit 3A, Silicon Oasis", pickupCity: "Dubai",
    receiverName: "Pedro Reyes",
    destAddr: "789 EDSA", destCity: "Mandaluyong",
    description: "Electronics — laptop", weight: "2.1",
    isCOD: true, codAmount: 45000, totalFee: 185,
    bookedAt: "14 min ago",
  },
  {
    id: "o4", awb: "CM-PH1-N0000013D", type: "balikbayan", freightMode: "air", status: "assigned",
    assignedTo: "Rodel Bautista",
    senderName: "John Mendoza",
    pickupAddr: "Flat 401, Deira Twin Towers", pickupCity: "Deira",
    receiverName: "Elena Mendoza",
    destAddr: "32 Session Rd", destCity: "Baguio City", destCountry: "PH",
    description: "Urgent — medicines & documents", weight: "8",
    isCOD: false, totalFee: 1450,
    bookedAt: "21 min ago", etaPickup: "10:45 AM",
  },
  {
    id: "o5", awb: "CM-PH1-S0000014E", type: "local", status: "assigned",
    assignedTo: "Mark Cruz",
    senderName: "Aisha Morales",
    pickupAddr: "Shop 5, Karama Centre", pickupCity: "Al Karama",
    receiverName: "Jose Morales",
    destAddr: "10 Bonifacio St", destCity: "Makati City",
    description: "Accessories & shoes", weight: "1.8",
    isCOD: true, codAmount: 3200, totalFee: 125,
    bookedAt: "35 min ago", etaPickup: "11:00 AM",
  },
  {
    id: "o6", awb: "CM-PH1-S0000015F", type: "local", status: "picked_up",
    assignedTo: "Danny Soriano",
    senderName: "Sandra Lee",
    pickupAddr: "Office 22, Dafza Free Zone", pickupCity: "Dubai",
    receiverName: "Ana Dela Cruz",
    destAddr: "55 Timog Ave", destCity: "Quezon City",
    description: "Documents", weight: "0.5",
    isCOD: false, totalFee: 95,
    bookedAt: "1 hr ago", etaPickup: "Done",
  },
];

const MOCK_DRIVERS: Driver[] = [
  {
    id: "d1", name: "Rodel Bautista",   phone: "+971-55-001-2345",
    vehicle: "Toyota Hiace Van",  vehicleClass: "van",  status: "available",
    tasksToday: 4, distanceKm: 1.2, etaMinutes: 6, isAiPick: true,
  },
  {
    id: "d2", name: "Mark Cruz",        phone: "+971-55-002-3456",
    vehicle: "Toyota Land Cruiser", vehicleClass: "van", status: "available",
    tasksToday: 6, distanceKm: 2.8, etaMinutes: 12, isAiPick: false,
  },
  {
    id: "d3", name: "Danny Soriano",    phone: "+971-55-003-4567",
    vehicle: "Ford Transit",       vehicleClass: "van",  status: "on_route",
    tasksToday: 8, distanceKm: 4.1, etaMinutes: 18, isAiPick: false,
  },
  {
    id: "d4", name: "Rico Evangelista", phone: "+971-55-004-5678",
    vehicle: "Toyota Hiace Van",   vehicleClass: "van",  status: "available",
    tasksToday: 3, distanceKm: 5.5, etaMinutes: 22, isAiPick: false,
  },
  {
    id: "d5", name: "Carlo Reyes",      phone: "+971-55-005-6789",
    vehicle: "Toyota Vios",        vehicleClass: "sedan", status: "available",
    tasksToday: 5, distanceKm: 2.1, etaMinutes: 9, isAiPick: false,
  },
  {
    id: "d6", name: "Jessa Mariano",    phone: "+971-55-006-7890",
    vehicle: "Honda City",         vehicleClass: "sedan", status: "available",
    tasksToday: 3, distanceKm: 3.4, etaMinutes: 14, isAiPick: false,
  },
];

// ── API fetch functions ───────────────────────────────────────────────────────

async function fetchOrders(): Promise<IncomingOrder[]> {
  try {
    const res = await authFetch(`${ORDER_INTAKE_URL}/v1/shipments`);
    if (!res.ok) return MOCK_ORDERS;
    const json = await res.json();
    const items = json.data?.items ?? json.data ?? [];
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    return items.map((s: any) => ({
      id:           s.id,
      awb:          s.awb ?? s.tracking_number ?? s.id,
      type:         s.shipment_type === "international" ? "balikbayan" : "local",
      freightMode:  s.freight_mode ?? undefined,
      status:       s.status === "pending" ? "unassigned" : s.status,
      assignedTo:   s.assigned_driver_name ?? undefined,
      senderName:   s.sender_name ?? s.sender?.name ?? "—",
      pickupAddr:   s.pickup_address?.line1 ?? s.pickup_address ?? "—",
      pickupCity:   s.pickup_address?.city  ?? "—",
      receiverName: s.recipient_name ?? s.recipient?.name ?? "—",
      destAddr:     s.delivery_address?.line1 ?? s.delivery_address ?? "—",
      destCity:     s.delivery_address?.city  ?? "—",
      destCountry:  s.delivery_address?.country ?? undefined,
      description:  s.description ?? "—",
      weight:       String(s.weight_kg ?? s.weight ?? "—"),
      isCOD:        !!s.cod_amount,
      codAmount:    s.cod_amount ?? undefined,
      totalFee:     s.total_fee ?? s.base_rate ?? 0,
      bookedAt:     s.created_at ? new Date(s.created_at).toLocaleString() : "—",
    }));
  } catch {
    return MOCK_ORDERS;
  }
}

async function fetchAvailableDrivers(): Promise<Driver[]> {
  try {
    const res = await authFetch(`${DISPATCH_URL}/v1/drivers`);
    if (!res.ok) return MOCK_DRIVERS;
    const json = await res.json();
    const items = json.data ?? json.drivers ?? [];
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    return items.map((d: any) => ({
      id:           d.id,
      name:         d.name ?? `${d.first_name ?? ""} ${d.last_name ?? ""}`.trim(),
      phone:        d.phone ?? "—",
      vehicle:      d.vehicle_plate ?? d.vehicle ?? "—",
      vehicleClass: d.vehicle_type ?? "motorcycle",
      status:       d.is_available ? "available" : "on_route",
      tasksToday:   d.tasks_today ?? 0,
      distanceKm:   d.distance_km ?? 0,
      etaMinutes:   d.eta_minutes ?? 0,
      isAiPick:     false,
    }));
  } catch {
    return MOCK_DRIVERS;
  }
}

async function dispatchOrder(shipmentId: string, driverId: string): Promise<boolean> {
  try {
    const res = await authFetch(`${DISPATCH_URL}/v1/queue/${shipmentId}/dispatch`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ driver_id: driverId }),
    });
    return res.ok;
  } catch {
    return false;
  }
}

// ── Vehicle class helpers ──────────────────────────────────────────────────────

const VEHICLE_CLASS_CONFIG: Record<VehicleClass, {
  label: string; icon: React.ReactNode; color: string; bg: string; border: string;
}> = {
  motorcycle: { label: "Motorcycle", icon: <Zap className="h-3 w-3" />,  color: "text-green-signal",  bg: "bg-green-surface",  border: "border-green-glow/30"  },
  sedan:      { label: "Sedan",      icon: <Car className="h-3 w-3" />,   color: "text-cyan-neon",     bg: "bg-cyan-surface",   border: "border-cyan-glow/30"   },
  van:        { label: "Van",        icon: <Truck className="h-3 w-3" />, color: "text-purple-plasma", bg: "bg-purple-surface", border: "border-purple-glow/30" },
};

function recommendVehicle(order: IncomingOrder): VehicleClass {
  const weight = parseFloat(order.weight);
  if (order.type === "balikbayan") return "van";
  if (weight > 25) return "van";
  // High-COD or fragile electronics → sedan even if light
  if (weight >= 5 || (order.isCOD && order.codAmount && order.codAmount >= 20000)) return "sedan";
  return "motorcycle";
}

function vehicleSuitability(driverClass: VehicleClass, required: VehicleClass): "match" | "upgrade" | "mismatch" {
  const rank: Record<VehicleClass, number> = { motorcycle: 0, sedan: 1, van: 2 };
  const diff = rank[driverClass] - rank[required];
  if (diff === 0) return "match";
  if (diff > 0)  return "upgrade";    // can carry but overkill / more expensive
  return "mismatch";                  // cannot carry
}

// ── Status config ─────────────────────────────────────────────────────────────

const STATUS_CONFIG: Record<OrderStatus, { label: string; badge: "cyan" | "amber" | "green" | "purple"; dot?: boolean; pulse?: boolean }> = {
  unassigned: { label: "Unassigned",  badge: "cyan",   dot: true, pulse: true  },
  assigning:  { label: "Auto-Assigning…", badge: "amber", dot: true, pulse: true  },
  assigned:   { label: "Assigned",    badge: "green",  dot: true  },
  picked_up:  { label: "Picked Up",   badge: "purple", dot: true  },
};

// ── KPI data ──────────────────────────────────────────────────────────────────

const KPIS = [
  { label: "New Orders",      value: 2,    trend: +5,   color: "cyan"   as const, format: "number" as const, live: true  },
  { label: "Awaiting Driver", value: 2,    trend: 0,    color: "amber"  as const, format: "number" as const, live: true  },
  { label: "Assigned Today",  value: 2,    trend: +12,  color: "green"  as const, format: "number" as const             },
  { label: "Avg Assign Time", value: 3.4,  trend: -0.8, color: "purple" as const, format: "number" as const, unit: "min" },
];

// ── Assign Driver Modal ───────────────────────────────────────────────────────

function AssignModal({
  order,
  drivers,
  onClose,
  onAssign,
}: {
  order: IncomingOrder;
  drivers: Driver[];
  onClose: () => void;
  onAssign: (orderId: string, driverId: string, driverName: string) => void;
}) {
  const required = recommendVehicle(order);
  const bestDriver = drivers
    .filter(d => d.status === "available" && vehicleSuitability(d.vehicleClass, required) !== "mismatch")
    .sort((a, b) => a.distanceKm - b.distanceKm)[0];
  const [selected, setSelected] = useState<string | null>(bestDriver?.id ?? drivers[0]?.id ?? null);
  const isIntl = order.type === "balikbayan";

  return (
    <div className="fixed inset-0 z-50 flex items-end justify-center sm:items-center p-4">
      <motion.div
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        exit={{ opacity: 0 }}
        className="absolute inset-0 bg-black/70 backdrop-blur-sm"
        onClick={onClose}
      />
      <motion.div
        initial={{ opacity: 0, y: 24, scale: 0.97 }}
        animate={{ opacity: 1, y: 0, scale: 1 }}
        exit={{ opacity: 0, y: 12, scale: 0.97 }}
        transition={{ type: "spring", stiffness: 350, damping: 28 }}
        className="relative z-10 w-full max-w-lg rounded-2xl border border-glass-border bg-canvas-100 shadow-2xl overflow-hidden"
      >
        {/* Header */}
        <div className="flex items-center justify-between border-b border-glass-border px-5 py-4">
          <div>
            <p className="font-heading text-base font-semibold text-white">Assign Driver</p>
            <p className="mt-0.5 font-mono text-xs text-white/40">{order.awb}</p>
          </div>
          <button onClick={onClose} className="flex h-8 w-8 items-center justify-center rounded-lg border border-glass-border bg-glass-100 text-white/50 hover:text-white transition-colors">
            <X className="h-4 w-4" />
          </button>
        </div>

        {/* Order summary */}
        <div className={cn("mx-5 mt-4 rounded-xl border p-3 text-xs", isIntl ? "border-purple-glow/30 bg-purple-surface" : "border-cyan-glow/30 bg-cyan-surface")}>
          <div className="flex items-center gap-2 mb-2">
            {isIntl ? <Globe className="h-3.5 w-3.5 text-purple-plasma" /> : <Package className="h-3.5 w-3.5 text-cyan-neon" />}
            <span className={cn("font-semibold", isIntl ? "text-purple-plasma" : "text-cyan-neon")}>
              {isIntl ? `Balikbayan Box · ${order.freightMode === "sea" ? "Sea Freight" : "Air Freight"}` : "Local Delivery"}
            </span>
            {order.isCOD && <span className="ml-auto font-mono text-amber-signal">COD ₱{order.codAmount?.toLocaleString()}</span>}
          </div>
          <div className="flex items-center gap-2 text-white/50">
            <MapPin className="h-3 w-3 flex-shrink-0" />
            <span className="flex-1 truncate">{order.pickupAddr}, {order.pickupCity}</span>
            <ArrowRight className="h-3 w-3 flex-shrink-0" />
            <span className="flex-1 truncate">{order.destCity}{isIntl ? `, ${order.destCountry}` : ""}</span>
          </div>
          <div className="mt-1.5 flex items-center gap-4 text-white/40">
            <span className="flex items-center gap-1"><Weight className="h-3 w-3" />{order.weight} kg</span>
            <span className="flex items-center gap-1"><Banknote className="h-3 w-3" />₱{order.totalFee}</span>
            <span className="flex items-center gap-1"><User className="h-3 w-3" />{order.senderName}</span>
          </div>
        </div>

        {/* AI recommendation notice */}
        {(() => {
          const required = recommendVehicle(order);
          const vc = VEHICLE_CLASS_CONFIG[required];
          const bestMatch = drivers
            .filter(d => d.status === "available" && vehicleSuitability(d.vehicleClass, required) !== "mismatch")
            .sort((a, b) => a.distanceKm - b.distanceKm)[0];
          return (
            <div className="mx-5 mt-3 space-y-2">
              <div className={cn("flex items-center gap-2 rounded-lg border px-3 py-2", vc.bg, vc.border)}>
                <div className={vc.color}>{vc.icon}</div>
                <p className="text-xs text-white/60">
                  This order needs a <span className={cn("font-semibold", vc.color)}>{vc.label}</span>
                  {" "}— {required === "sedan" ? "5–25 kg or high-COD" : required === "van" ? "bulk / balikbayan" : "< 5 kg, small parcel"}.
                </p>
              </div>
              {bestMatch && (
                <div className="flex items-center gap-2 rounded-lg border border-green-glow/25 bg-green-surface px-3 py-2">
                  <Bot className="h-3.5 w-3.5 text-green-signal flex-shrink-0" />
                  <p className="text-xs text-white/60">
                    AI recommends <span className="font-semibold text-green-signal">{bestMatch.name}</span>
                    {" "}— nearest compatible {bestMatch.vehicleClass}, {bestMatch.distanceKm} km away, ETA {bestMatch.etaMinutes} min.
                  </p>
                </div>
              )}
            </div>
          );
        })()}

        {/* Driver list */}
        <div className="mt-3 max-h-64 overflow-y-auto px-5 space-y-2">
          {(() => {
            const required = recommendVehicle(order);
            return drivers
              .slice()
              .sort((a, b) => {
                const sa = vehicleSuitability(a.vehicleClass, required);
                const sb = vehicleSuitability(b.vehicleClass, required);
                const rank = { match: 0, upgrade: 1, mismatch: 2 };
                return rank[sa] - rank[sb] || a.distanceKm - b.distanceKm;
              })
              .map((driver) => {
                const suitability = vehicleSuitability(driver.vehicleClass, required);
                const vc = VEHICLE_CLASS_CONFIG[driver.vehicleClass];
                return (
                  <button
                    key={driver.id}
                    onClick={() => suitability !== "mismatch" && setSelected(driver.id)}
                    disabled={suitability === "mismatch"}
                    className={cn(
                      "w-full rounded-xl border p-3 text-left transition-all duration-150",
                      suitability === "mismatch"
                        ? "border-glass-border bg-glass-100 opacity-40 cursor-not-allowed"
                        : selected === driver.id
                          ? "border-cyan-glow/50 bg-cyan-surface shadow-[0_0_12px_rgba(0,229,255,0.08)]"
                          : "border-glass-border bg-glass-100 hover:bg-glass-200"
                    )}
                  >
                    <div className="flex items-center gap-3">
                      <div className={cn(
                        "flex h-8 w-8 flex-shrink-0 items-center justify-center rounded-full text-xs font-bold text-canvas",
                        driver.isAiPick
                          ? "bg-gradient-to-br from-green-signal to-cyan-neon"
                          : "bg-gradient-to-br from-white/20 to-white/10 text-white/70"
                      )}>
                        {driver.name.split(" ").map(n => n[0]).join("")}
                      </div>
                      <div className="flex-1 min-w-0">
                        <div className="flex items-center gap-2 flex-wrap">
                          <span className="text-sm font-medium text-white truncate">{driver.name}</span>
                          {driver.isAiPick && (
                            <span className="flex items-center gap-1 rounded px-1.5 py-0.5 bg-green-surface border border-green-glow/30 text-2xs font-mono text-green-signal">
                              <Bot className="h-2.5 w-2.5" /> AI Pick
                            </span>
                          )}
                          {/* Vehicle class badge */}
                          <span className={cn("flex items-center gap-1 rounded px-1.5 py-0.5 border text-2xs font-mono", vc.bg, vc.border, vc.color)}>
                            {vc.icon}{vc.label}
                          </span>
                        </div>
                        <div className="mt-0.5 flex items-center gap-3 text-2xs text-white/40 font-mono">
                          <span className="truncate max-w-[90px]">{driver.vehicle}</span>
                          <span className="flex items-center gap-1"><Navigation className="h-2.5 w-2.5" />{driver.distanceKm} km</span>
                          <span className="flex items-center gap-1"><Clock className="h-2.5 w-2.5" />{driver.etaMinutes} min</span>
                        </div>
                      </div>
                      <div className="text-right text-2xs flex-shrink-0">
                        {/* Suitability indicator */}
                        {suitability === "match" && (
                          <div className="flex items-center gap-1 text-green-signal font-mono font-semibold mb-0.5">
                            <CheckCircle2 className="h-3 w-3" /> Match
                          </div>
                        )}
                        {suitability === "upgrade" && (
                          <div className="flex items-center gap-1 text-amber-signal font-mono font-semibold mb-0.5">
                            <AlertCircle className="h-3 w-3" /> Oversize
                          </div>
                        )}
                        {suitability === "mismatch" && (
                          <div className="flex items-center gap-1 text-red-400 font-mono font-semibold mb-0.5">
                            <X className="h-3 w-3" /> Too small
                          </div>
                        )}
                        <div className={cn("font-mono font-semibold", driver.status === "available" ? "text-green-signal" : "text-amber-signal")}>
                          {driver.status === "available" ? "Available" : "On Route"}
                        </div>
                        <div className="text-white/30">{driver.tasksToday} tasks</div>
                      </div>
                    </div>
                  </button>
                );
              });
          })()}
        </div>

        {/* Actions */}
        <div className="flex items-center gap-3 border-t border-glass-border px-5 py-4 mt-3">
          <button onClick={onClose} className="flex-1 rounded-xl border border-glass-border bg-glass-100 px-4 py-2.5 text-sm text-white/60 transition hover:bg-glass-200">
            Cancel
          </button>
          <button
            onClick={() => {
              const driver = drivers.find(d => d.id === selected);
              if (driver) onAssign(order.id, driver.id, driver.name);
            }}
            disabled={!selected}
            className={cn(
              "flex-1 flex items-center justify-center gap-2 rounded-xl px-4 py-2.5 text-sm font-semibold text-canvas transition",
              selected
                ? "bg-gradient-to-r from-cyan-neon to-green-signal hover:opacity-90"
                : "bg-glass-200 text-white/30 cursor-not-allowed"
            )}
          >
            <CheckCircle2 className="h-4 w-4" />
            Confirm Assignment
          </button>
        </div>
      </motion.div>
    </div>
  );
}

// ── Order Row ─────────────────────────────────────────────────────────────────

function OrderRow({
  order,
  onAssign,
  onAutoAssign,
}: {
  order: IncomingOrder;
  onAssign: (order: IncomingOrder) => void;
  onAutoAssign: (orderId: string) => void;
}) {
  const cfg    = STATUS_CONFIG[order.status];
  const isIntl = order.type === "balikbayan";

  return (
    <motion.tr
      variants={variants.fadeInUp}
      className="group border-b border-glass-border last:border-0 hover:bg-glass-100 transition-colors"
    >
      {/* AWB + Type */}
      <td className="px-4 py-3">
        <div className="flex items-center gap-2.5">
          <div className={cn("flex h-8 w-8 flex-shrink-0 items-center justify-center rounded-lg",
            isIntl ? "bg-purple-surface border border-purple-glow/25" : "bg-cyan-surface border border-cyan-glow/25"
          )}>
            {isIntl ? <Globe className="h-3.5 w-3.5 text-purple-plasma" /> : <Package className="h-3.5 w-3.5 text-cyan-neon" />}
          </div>
          <div>
            <p className="font-mono text-xs font-semibold text-white">{order.awb}</p>
            <p className={cn("text-2xs font-mono", isIntl ? "text-purple-plasma" : "text-white/40")}>
              {isIntl ? `Balikbayan · ${order.freightMode === "sea" ? "Sea" : "Air"}` : "Local"}
            </p>
          </div>
        </div>
      </td>

      {/* Sender → Receiver */}
      <td className="px-4 py-3">
        <div className="text-xs">
          <div className="flex items-center gap-1.5 text-white/70">
            <div className="h-1.5 w-1.5 rounded-full bg-cyan-neon flex-shrink-0" />
            <span className="font-medium truncate max-w-[140px]">{order.senderName}</span>
            <span className="text-white/30 truncate">· {order.pickupCity}</span>
          </div>
          <div className="flex items-center gap-1.5 mt-1 text-white/50">
            <div className="h-1.5 w-1.5 rounded-full bg-white/20 flex-shrink-0" />
            <span className="truncate max-w-[140px]">{order.receiverName}</span>
            <span className="text-white/30 truncate">· {order.destCity}{isIntl ? `, ${order.destCountry}` : ""}</span>
          </div>
        </div>
      </td>

      {/* Package info */}
      <td className="px-4 py-3 hidden lg:table-cell">
        <div className="text-2xs font-mono space-y-1">
          <div className="flex items-center gap-1.5 text-white/50">
            <Weight className="h-3 w-3" />
            {order.weight} kg
          </div>
          {order.isCOD && (
            <div className="flex items-center gap-1.5 text-amber-signal">
              <Banknote className="h-3 w-3" />
              COD ₱{order.codAmount?.toLocaleString()}
            </div>
          )}
          {/* Vehicle recommendation */}
          {(() => {
            const rec = recommendVehicle(order);
            const vc  = VEHICLE_CLASS_CONFIG[rec];
            return (
              <div className={cn("inline-flex items-center gap-1 rounded px-1.5 py-0.5 border", vc.bg, vc.border, vc.color)}>
                {vc.icon}<span>{vc.label}</span>
              </div>
            );
          })()}
          <div className="flex items-center gap-1.5 text-white/30">
            <Clock className="h-3 w-3" />
            {order.bookedAt}
          </div>
        </div>
      </td>

      {/* Status */}
      <td className="px-4 py-3">
        <NeonBadge variant={cfg.badge} dot={cfg.dot} pulse={cfg.pulse}>
          {cfg.label}
        </NeonBadge>
        {order.assignedTo && (
          <p className="mt-1 text-2xs text-white/40 font-mono truncate max-w-[110px]">→ {order.assignedTo}</p>
        )}
        {order.etaPickup && (
          <p className="mt-0.5 text-2xs text-green-signal font-mono">ETA {order.etaPickup}</p>
        )}
      </td>

      {/* Actions */}
      <td className="px-4 py-3">
        {order.status === "unassigned" && (
          <div className="flex items-center gap-2">
            <button
              onClick={() => onAutoAssign(order.id)}
              className="flex items-center gap-1.5 rounded-lg border border-green-glow/30 bg-green-surface px-2.5 py-1.5 text-2xs font-semibold text-green-signal transition hover:border-green-glow/60 hover:bg-green-surface/80"
            >
              <Zap className="h-3 w-3" />
              Auto
            </button>
            <button
              onClick={() => onAssign(order)}
              className="flex items-center gap-1.5 rounded-lg border border-cyan-glow/30 bg-cyan-surface px-2.5 py-1.5 text-2xs font-semibold text-cyan-neon transition hover:border-cyan-glow/60"
            >
              <User className="h-3 w-3" />
              Manual
            </button>
          </div>
        )}
        {order.status === "assigning" && (
          <div className="flex items-center gap-1.5 text-amber-signal text-2xs font-mono">
            <RefreshCw className="h-3 w-3 animate-spin" />
            Assigning…
          </div>
        )}
        {(order.status === "assigned" || order.status === "picked_up") && (
          <div className="flex items-center gap-1.5 text-white/30 text-2xs font-mono">
            <CheckCircle2 className="h-3 w-3 text-green-signal" />
            Done
          </div>
        )}
      </td>
    </motion.tr>
  );
}

// ── Main page ─────────────────────────────────────────────────────────────────

export default function OrdersPage() {
  const [orders,       setOrders]       = useState<IncomingOrder[]>(MOCK_ORDERS);
  const [drivers,      setDrivers]      = useState<Driver[]>(MOCK_DRIVERS);
  const [assignTarget, setAssignTarget] = useState<IncomingOrder | null>(null);
  const [filter,       setFilter]       = useState<"all" | "unassigned" | "assigned" | "picked_up">("all");

  const loadData = useCallback(async () => {
    const [apiOrders, apiDrivers] = await Promise.all([fetchOrders(), fetchAvailableDrivers()]);
    setOrders(apiOrders);
    setDrivers(apiDrivers);
  }, []);

  useEffect(() => { loadData(); }, [loadData]);

  const unassignedCount = orders.filter(o => o.status === "unassigned").length;
  const assigningCount  = orders.filter(o => o.status === "assigning").length;

  function handleAutoAssign(orderId: string) {
    // Simulate AI auto-assignment: assigning → assigned (with AI-recommended driver)
    setOrders(prev => prev.map(o =>
      o.id === orderId ? { ...o, status: "assigning" } : o
    ));
    setTimeout(() => {
      setOrders(prev => prev.map(o =>
        o.id === orderId ? { ...o, status: "assigned", assignedTo: "Rodel Bautista", etaPickup: "~6 min" } : o
      ));
    }, 2000);
  }

  function handleManualAssign(orderId: string, driverId: string, driverName: string) {
    setOrders(prev => prev.map(o =>
      o.id === orderId ? { ...o, status: "assigned", assignedTo: driverName, etaPickup: "~12 min" } : o
    ));
    setAssignTarget(null);
    dispatchOrder(orderId, driverId);
  }

  const filtered = orders.filter(o => {
    if (filter === "unassigned") return o.status === "unassigned" || o.status === "assigning";
    if (filter === "assigned")   return o.status === "assigned";
    if (filter === "picked_up")  return o.status === "picked_up";
    return true;
  });

  return (
    <>
      <motion.div
        variants={variants.staggerContainer}
        initial="hidden"
        animate="visible"
        className="space-y-6"
      >
        {/* ── KPI strip ────────────────────────────────────────────────── */}
        <motion.div
          variants={variants.staggerContainer}
          className="grid grid-cols-2 gap-4 xl:grid-cols-4"
        >
          {KPIS.map((kpi) => (
            <motion.div key={kpi.label} variants={variants.fadeInUp}>
              <GlassCard glow={kpi.color}>
                <LiveMetric
                  label={kpi.label}
                  value={kpi.value}
                  unit={kpi.unit}
                  trend={kpi.trend}
                  color={kpi.color}
                  format={kpi.format}
                  live={kpi.live}
                />
              </GlassCard>
            </motion.div>
          ))}
        </motion.div>

        {/* ── Alert banner when unassigned orders exist ─────────────── */}
        {(unassignedCount > 0 || assigningCount > 0) && (
          <motion.div variants={variants.fadeInUp}>
            <div className="flex items-center gap-3 rounded-xl border border-cyan-glow/30 bg-cyan-surface px-4 py-3">
              <AlertCircle className="h-4 w-4 text-cyan-neon flex-shrink-0" />
              <p className="text-sm text-white/80">
                <span className="font-semibold text-cyan-neon">{unassignedCount + assigningCount} order{unassignedCount + assigningCount !== 1 ? "s" : ""}</span>
                {" "}need{unassignedCount + assigningCount === 1 ? "s" : ""} driver assignment.
                {" "}Use <span className="font-semibold text-green-signal">Auto</span> for AI-optimised dispatch or
                {" "}<span className="font-semibold text-cyan-neon">Manual</span> to choose a driver yourself.
              </p>
              <button
                onClick={() => orders.filter(o => o.status === "unassigned").forEach(o => handleAutoAssign(o.id))}
                className="ml-auto flex items-center gap-1.5 whitespace-nowrap rounded-lg border border-green-glow/30 bg-green-surface px-3 py-1.5 text-xs font-semibold text-green-signal transition hover:border-green-glow/60"
              >
                <Zap className="h-3.5 w-3.5" />
                Auto-Assign All
              </button>
            </div>
          </motion.div>
        )}

        {/* ── Orders table ──────────────────────────────────────────── */}
        <motion.div variants={variants.fadeInUp}>
          <GlassCard padding="none" glow="cyan">
            {/* Table header */}
            <div className="flex items-center justify-between border-b border-glass-border px-4 py-3">
              <div className="flex items-center gap-3">
                <p className="font-heading text-sm font-semibold text-white">Incoming Orders</p>
                <NeonBadge variant="cyan" dot pulse>{orders.length} total</NeonBadge>
              </div>
              {/* Filter tabs */}
              <div className="flex items-center gap-1 rounded-lg border border-glass-border bg-glass-100 p-1">
                {([
                  { key: "all",        label: "All" },
                  { key: "unassigned", label: "Pending" },
                  { key: "assigned",   label: "Assigned" },
                  { key: "picked_up",  label: "Picked Up" },
                ] as const).map((tab) => (
                  <button
                    key={tab.key}
                    onClick={() => setFilter(tab.key)}
                    className={cn(
                      "rounded-md px-3 py-1.5 text-xs font-medium transition-all",
                      filter === tab.key
                        ? "bg-cyan-surface text-cyan-neon border border-cyan-glow/30"
                        : "text-white/40 hover:text-white/70"
                    )}
                  >
                    {tab.label}
                  </button>
                ))}
              </div>
            </div>

            {/* Table */}
            <div className="overflow-x-auto">
              <table className="w-full">
                <thead>
                  <tr className="border-b border-glass-border">
                    {["AWB / Type", "Sender → Receiver", "Package", "Status", "Actions"].map((h) => (
                      <th key={h} className={cn(
                        "px-4 py-2.5 text-left text-2xs font-medium uppercase tracking-widest text-white/30",
                        h === "Package" && "hidden lg:table-cell"
                      )}>
                        {h}
                      </th>
                    ))}
                  </tr>
                </thead>
                <motion.tbody variants={variants.staggerContainer} initial="hidden" animate="visible">
                  {filtered.map((order) => (
                    <OrderRow
                      key={order.id}
                      order={order}
                      onAssign={setAssignTarget}
                      onAutoAssign={handleAutoAssign}
                    />
                  ))}
                </motion.tbody>
              </table>

              {filtered.length === 0 && (
                <div className="flex flex-col items-center gap-2 py-14 text-white/20">
                  <Package className="h-8 w-8" />
                  <p className="text-sm">No orders in this filter</p>
                </div>
              )}
            </div>

            {/* Footer */}
            <div className="flex items-center justify-between border-t border-glass-border px-4 py-3">
              <p className="text-2xs text-white/30 font-mono">{filtered.length} orders shown</p>
              <div className="flex items-center gap-2 text-2xs text-white/30">
                <div className="h-1.5 w-1.5 rounded-full bg-cyan-neon animate-pulse" />
                Live feed · updates every 30s
              </div>
            </div>
          </GlassCard>
        </motion.div>

        {/* ── Assignment flow legend ────────────────────────────────── */}
        <motion.div variants={variants.fadeInUp}>
          <GlassCard>
            <p className="mb-3 text-xs font-semibold text-white/60 uppercase tracking-widest">Assignment Flow</p>
            <div className="flex flex-wrap items-center gap-2 text-xs text-white/50">
              {[
                { icon: <Package className="h-3.5 w-3.5 text-cyan-neon" />,    label: "Customer Books",       color: "text-cyan-neon"    },
                { icon: <ChevronRight className="h-3.5 w-3.5" />,              label: "",                     color: ""                  },
                { icon: <AlertCircle className="h-3.5 w-3.5 text-amber-signal" />, label: "Unassigned Queue", color: "text-amber-signal" },
                { icon: <ChevronRight className="h-3.5 w-3.5" />,              label: "",                     color: ""                  },
                { icon: <Zap className="h-3.5 w-3.5 text-green-signal" />,     label: "AI Auto-Assigns",      color: "text-green-signal" },
                { icon: <ChevronRight className="h-3.5 w-3.5" />,              label: "",                     color: ""                  },
                { icon: <Truck className="h-3.5 w-3.5 text-purple-plasma" />,  label: "Driver Notified",      color: "text-purple-plasma"},
                { icon: <ChevronRight className="h-3.5 w-3.5" />,              label: "",                     color: ""                  },
                { icon: <Navigation className="h-3.5 w-3.5 text-cyan-neon" />, label: "Pickup Confirmed",     color: "text-cyan-neon"    },
              ].map((step, i) => (
                <span key={i} className={cn("flex items-center gap-1", step.color)}>{step.icon}{step.label}</span>
              ))}
            </div>
          </GlassCard>
        </motion.div>
      </motion.div>

      {/* ── Assign Modal ───────────────────────────────────────────── */}
      <AnimatePresence>
        {assignTarget && (
          <AssignModal
            order={assignTarget}
            drivers={drivers}
            onClose={() => setAssignTarget(null)}
            onAssign={handleManualAssign}
          />
        )}
      </AnimatePresence>
    </>
  );
}
