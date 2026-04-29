"use client";
/**
 * Partner Portal — Drivers Management Page
 * Set driver type (part-time / full-time) and commission rates per driver.
 */
import { useState, useEffect, Suspense } from "react";
import { useRouter, useSearchParams } from "next/navigation";
import { motion, AnimatePresence } from "framer-motion";
import { variants } from "@/lib/design-system/tokens";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { Users, Clock, Briefcase, Search, Check, Pencil, X, UserPlus, Loader2, MapPin, Truck } from "lucide-react";
import { cn } from "@/lib/design-system/cn";
import { ComplianceBadge, canAssign } from "@/components/compliance/compliance-badge";
import { authFetch } from "@/lib/auth/auth-fetch";
import { useRosterEvents } from "@/hooks/useRosterEvents";

// ── API helpers ────────────────────────────────────────────────────────────────

const DRIVER_OPS_URL = process.env.NEXT_PUBLIC_DRIVER_OPS_URL ?? "http://localhost:8006";
const API_BASE       = process.env.NEXT_PUBLIC_API_URL        ?? "http://localhost:8000";

// ── Types ──────────────────────────────────────────────────────────────────────

type DriverType = "full_time" | "part_time";

interface Driver {
  id:                string;
  /** Identity user_id — used to match live RosterEvent frames which key on user_id. */
  userId:            string;
  name:              string;
  phone:             string;
  zone:              string;
  driverType:        DriverType;
  commissionRate:    number;   // PHP per delivery (part-time only)
  codCommissionRate: number;   // fraction, e.g. 0.02 = 2%
  deliveriesToday:   number;
  deliveriesWeek:    number;
  earningsToday:     number;
  status:             "active" | "offline" | "on_delivery";
  compliance_status:  "compliant" | "expiring_soon" | "expired" | "suspended" | "under_review" | "pending_submission" | "rejected";
  compliance_detail?: string;   // e.g. "License · 18d left"
}

// ── API fetch ──────────────────────────────────────────────────────────────────

// Maps driver-ops DriverDto → UI row. per_delivery_rate_cents → PHP, bps → fraction.
// eslint-disable-next-line @typescript-eslint/no-explicit-any
function dtoToDriver(d: any): Driver {
  const activeRoute = d.active_route_id ?? null;
  const onlineStatus: Driver["status"] =
    activeRoute ? "on_delivery" : d.is_online ? "active" : "offline";
  return {
    id:                d.id,
    userId:            d.user_id ?? d.id,
    name:              `${d.first_name ?? ""} ${d.last_name ?? ""}`.trim() || "—",
    phone:             d.phone ?? "—",
    zone:              d.zone ?? "—",
    driverType:        d.driver_type === "part_time" ? "part_time" : "full_time",
    commissionRate:    (d.per_delivery_rate_cents ?? 0) / 100,
    codCommissionRate: (d.cod_commission_rate_bps ?? 0) / 10_000,
    deliveriesToday:   d.deliveries_today ?? 0,
    deliveriesWeek:    d.deliveries_week ?? 0,
    earningsToday:     d.earnings_today ?? 0,
    status:            onlineStatus,
    compliance_status: d.compliance_status ?? "compliant",
    compliance_detail: d.compliance_detail ?? undefined,
  };
}

async function fetchDriversFromApi(): Promise<Driver[] | null> {
  try {
    const res = await authFetch(`${DRIVER_OPS_URL}/v1/drivers`);
    if (!res.ok) return null;
    const json = await res.json();
    const items = json.data ?? [];
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    return items.map((d: any) => dtoToDriver(d));
  } catch {
    return null;
  }
}

async function patchDriver(id: string, patch: Record<string, unknown>): Promise<Driver | null> {
  try {
    const res = await authFetch(`${DRIVER_OPS_URL}/v1/drivers/${id}`, {
      method: "PATCH",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(patch),
    });
    if (!res.ok) return null;
    const json = await res.json();
    return json.data ? dtoToDriver(json.data) : null;
  } catch {
    return null;
  }
}

/**
 * Two-step driver registration: invite user in identity (returns user_id +
 * temp password), then register the driver profile in driver-ops under that
 * user_id. The temp password is surfaced to the admin so they can hand it
 * off until out-of-band email delivery is wired up.
 */
interface RegisterResult {
  driverId:     string;
  tempPassword: string | null;
  error?:       string;
}

async function registerDriverApi(input: {
  email:      string;
  firstName:  string;
  lastName:   string;
  phone:      string;
}): Promise<RegisterResult> {
  try {
    const inviteRes = await authFetch(`${API_BASE}/v1/users`, {
      method: "POST",
      body: JSON.stringify({
        email:        input.email,
        first_name:   input.firstName,
        last_name:    input.lastName,
        roles:        ["driver"],
        // phone_number is stored on the identity user so the Driver App OTP
        // login can resolve this record by phone rather than creating an orphan.
        phone_number: input.phone,
      }),
    });
    if (!inviteRes.ok) {
      const body = await inviteRes.text();
      return { driverId: "", tempPassword: null, error: `Identity error: ${body || inviteRes.status}` };
    }
    const inviteJson = await inviteRes.json();
    const userId       = inviteJson?.data?.user_id;
    const tempPassword = inviteJson?.data?.temp_password ?? null;
    if (!userId) return { driverId: "", tempPassword: null, error: "Identity did not return a user_id" };

    const regRes = await authFetch(`${DRIVER_OPS_URL}/v1/drivers`, {
      method: "POST",
      body: JSON.stringify({
        user_id:    userId,
        first_name: input.firstName,
        last_name:  input.lastName,
        phone:      input.phone,
      }),
    });
    if (!regRes.ok) {
      const body = await regRes.text();
      return { driverId: "", tempPassword, error: `Driver profile error: ${body || regRes.status}` };
    }
    const regJson = await regRes.json();
    return { driverId: regJson?.data?.driver_id ?? userId, tempPassword };
  } catch (e) {
    return { driverId: "", tempPassword: null, error: (e as Error).message };
  }
}

const fmt = (n: number) =>
  `₱${n.toFixed(0).replace(/\B(?=(\d{3})+(?!\d))/g, ",")}`;

const STATUS_CONFIG = {
  active:      { label: "Active",      variant: "green" as const },
  offline:     { label: "Offline",     variant: "red"   as const },
  on_delivery: { label: "On Delivery", variant: "cyan"  as const },
};

// ── Edit commission drawer ──────────────────────────────────────────────────────

function EditDrawer({
  driver,
  onSave,
  onClose,
}: {
  driver: Driver;
  onSave: (updates: Partial<Driver>) => void;
  onClose: () => void;
}) {
  const [driverType, setDriverType]               = useState<DriverType>(driver.driverType);
  const [commissionRate, setCommissionRate]        = useState(String(driver.commissionRate));
  const [codCommissionRate, setCodCommissionRate]  = useState(String((driver.codCommissionRate * 100).toFixed(1)));

  function handleSave() {
    onSave({
      driverType,
      commissionRate:    driverType === "part_time" ? parseFloat(commissionRate) || 0 : 0,
      codCommissionRate: driverType === "part_time" ? (parseFloat(codCommissionRate) || 0) / 100 : 0,
    });
  }

  return (
    <motion.div
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/60 backdrop-blur-sm"
      onClick={(e) => e.target === e.currentTarget && onClose()}
    >
      <motion.div
        initial={{ opacity: 0, scale: 0.95, y: 12 }}
        animate={{ opacity: 1, scale: 1, y: 0 }}
        exit={{ opacity: 0, scale: 0.95, y: 12 }}
        transition={{ duration: 0.25, ease: [0.16, 1, 0.3, 1] }}
        className="w-full max-w-md rounded-2xl border border-glass-border bg-canvas-100 p-6 shadow-2xl"
        style={{ boxShadow: "0 0 40px rgba(0,229,255,0.08), 0 24px 48px rgba(0,0,0,0.6)" }}
      >
        {/* Header */}
        <div className="flex items-start justify-between mb-6">
          <div>
            <h2 className="font-heading text-base font-semibold text-white">{driver.name}</h2>
            <p className="text-xs text-white/40 font-mono mt-0.5">{driver.id} · {driver.zone}</p>
          </div>
          <button
            onClick={onClose}
            className="flex h-8 w-8 items-center justify-center rounded-lg border border-glass-border text-white/40 hover:text-white/70 hover:bg-glass-200 transition-all"
          >
            <X className="h-3.5 w-3.5" />
          </button>
        </div>

        {/* Driver type toggle */}
        <div className="mb-5">
          <label className="block text-xs font-mono text-white/40 uppercase tracking-wider mb-2">Driver Type</label>
          <div className="grid grid-cols-2 gap-2">
            {(["full_time", "part_time"] as DriverType[]).map((type) => (
              <button
                key={type}
                onClick={() => setDriverType(type)}
                className={cn(
                  "flex items-center gap-2.5 rounded-xl border px-4 py-3 text-sm font-medium transition-all",
                  driverType === type
                    ? type === "part_time"
                      ? "border-amber-400/40 bg-amber-400/10 text-amber-400"
                      : "border-cyan-signal/40 bg-cyan-signal/10 text-cyan-signal"
                    : "border-glass-border bg-glass-100 text-white/40 hover:text-white/60 hover:bg-glass-200"
                )}
              >
                {type === "part_time" ? <Clock className="h-4 w-4" /> : <Briefcase className="h-4 w-4" />}
                {type === "part_time" ? "Part-Time" : "Full-Time"}
                {driverType === type && <Check className="h-3.5 w-3.5 ml-auto" />}
              </button>
            ))}
          </div>
        </div>

        {/* Commission fields — only for part-time */}
        {driverType === "part_time" && (
          <motion.div
            initial={{ opacity: 0, height: 0 }}
            animate={{ opacity: 1, height: "auto" }}
            exit={{ opacity: 0, height: 0 }}
            transition={{ duration: 0.2 }}
            className="space-y-4 mb-6"
          >
            <div>
              <label className="block text-xs font-mono text-white/40 uppercase tracking-wider mb-2">
                Base Rate per Delivery (PHP)
              </label>
              <div className="relative">
                <span className="absolute left-3 top-1/2 -translate-y-1/2 font-mono text-sm text-white/40">₱</span>
                <input
                  type="number"
                  min="0"
                  step="5"
                  value={commissionRate}
                  onChange={(e) => setCommissionRate(e.target.value)}
                  className={cn(
                    "w-full rounded-xl border border-glass-border bg-glass-100 pl-7 pr-4 py-2.5",
                    "font-mono text-sm text-white placeholder-white/20",
                    "focus:outline-none focus:border-cyan-signal/50 focus:bg-glass-200 transition-all"
                  )}
                  placeholder="50"
                />
              </div>
            </div>
            <div>
              <label className="block text-xs font-mono text-white/40 uppercase tracking-wider mb-2">
                COD Commission Rate (%)
              </label>
              <div className="relative">
                <input
                  type="number"
                  min="0"
                  max="10"
                  step="0.5"
                  value={codCommissionRate}
                  onChange={(e) => setCodCommissionRate(e.target.value)}
                  className={cn(
                    "w-full rounded-xl border border-glass-border bg-glass-100 px-4 py-2.5 pr-8",
                    "font-mono text-sm text-white placeholder-white/20",
                    "focus:outline-none focus:border-amber-400/50 focus:bg-glass-200 transition-all"
                  )}
                  placeholder="2.0"
                />
                <span className="absolute right-3 top-1/2 -translate-y-1/2 font-mono text-sm text-white/40">%</span>
              </div>
              <p className="mt-1.5 text-xs text-white/30 font-mono">Applied on top of base rate for COD deliveries</p>
            </div>
          </motion.div>
        )}

        {driverType === "full_time" && (
          <div className="mb-6 rounded-xl border border-cyan-signal/15 bg-cyan-signal/5 px-4 py-3">
            <p className="text-xs text-cyan-signal/70">
              Full-time drivers receive a fixed salary. Commission settings are not applicable.
            </p>
          </div>
        )}

        {/* Actions */}
        <div className="flex gap-3">
          <button
            onClick={onClose}
            className="flex-1 rounded-xl border border-glass-border bg-glass-100 py-2.5 text-sm text-white/50 hover:text-white/70 hover:bg-glass-200 transition-all"
          >
            Cancel
          </button>
          <button
            onClick={handleSave}
            className={cn(
              "flex-1 rounded-xl py-2.5 text-sm font-semibold transition-all",
              "bg-green-signal/15 border border-green-signal/40 text-green-signal",
              "hover:bg-green-signal/25 hover:shadow-[0_0_12px_rgba(0,255,136,0.25)]"
            )}
          >
            Save Changes
          </button>
        </div>
      </motion.div>
    </motion.div>
  );
}

// ── Driver row ─────────────────────────────────────────────────────────────────

function DriverRow({ driver, onEdit, onAssign }: { driver: Driver; onEdit: () => void; onAssign: () => void }) {
  const isPartTime  = driver.driverType === "part_time";
  const statusCfg   = STATUS_CONFIG[driver.status];

  return (
    <tr className="group border-b border-glass-border transition-colors hover:bg-glass-100/60">
      <td className="py-3.5 pl-4 pr-3">
        <div className="flex items-center gap-3">
          <div
            className="flex h-9 w-9 flex-shrink-0 items-center justify-center rounded-full text-xs font-bold text-canvas"
            style={{
              background: isPartTime
                ? "linear-gradient(135deg, #FFAB00 0%, #FF6B00 100%)"
                : "linear-gradient(135deg, #00E5FF 0%, #A855F7 100%)",
            }}
          >
            {driver.name.split(" ").map((n) => n[0]).slice(0, 2).join("")}
          </div>
          <div>
            <p className="text-sm font-medium text-white/90">{driver.name}</p>
            <p className="text-xs font-mono text-white/30">{driver.id}</p>
          </div>
        </div>
      </td>
      <td className="py-3.5 px-3 text-xs text-white/50">{driver.zone}</td>
      <td className="py-3.5 px-3">
        <span
          className={cn(
            "inline-flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-xs font-medium",
            isPartTime
              ? "border-amber-400/30 bg-amber-400/10 text-amber-400"
              : "border-cyan-signal/30 bg-cyan-signal/10 text-cyan-signal"
          )}
        >
          {isPartTime ? <Clock className="h-3 w-3" /> : <Briefcase className="h-3 w-3" />}
          {isPartTime ? "Part-Time" : "Full-Time"}
        </span>
      </td>
      <td className="py-3.5 px-3">
        {isPartTime ? (
          <div className="space-y-0.5">
            <p className="text-sm font-mono text-white/80">{fmt(driver.commissionRate)}<span className="text-white/30 text-xs">/delivery</span></p>
            <p className="text-xs font-mono text-amber-400/60">{(driver.codCommissionRate * 100).toFixed(1)}% COD bonus</p>
          </div>
        ) : (
          <span className="text-xs text-white/25 font-mono">Fixed salary</span>
        )}
      </td>
      <td className="py-3.5 px-3 text-center">
        <p className="text-sm font-mono text-white/80">{driver.deliveriesToday}</p>
        <p className="text-xs text-white/30">{driver.deliveriesWeek} / wk</p>
      </td>
      <td className="py-3.5 px-3">
        {isPartTime ? (
          <span className="font-mono text-sm text-green-signal">{fmt(driver.earningsToday)}</span>
        ) : (
          <span className="text-xs text-white/25 font-mono">—</span>
        )}
      </td>
      <td className="py-3.5 px-3">
        <NeonBadge variant={statusCfg.variant} dot={driver.status !== "offline"} pulse={driver.status === "on_delivery"}>
          {statusCfg.label}
        </NeonBadge>
      </td>
      <td className="py-3.5 px-3">
        <ComplianceBadge status={driver.compliance_status} expiryDetail={driver.compliance_detail} />
      </td>
      <td className="py-3.5 pl-3 pr-4 text-right">
        <div className="flex items-center justify-end gap-2 opacity-0 transition-all group-hover:opacity-100">
          <button
            disabled={!canAssign(driver.compliance_status)}
            onClick={onAssign}
            className={cn(
              "inline-flex items-center gap-1 rounded-lg border px-2.5 py-1.5 text-xs font-medium transition-all",
              canAssign(driver.compliance_status)
                ? "border-purple-plasma/30 bg-purple-plasma/10 text-purple-plasma hover:bg-purple-plasma/20"
                : "border-glass-border bg-glass-100 text-white/20 cursor-not-allowed opacity-40"
            )}
          >
            Assign Task
          </button>
          {/* Cross-portal deep link — opens the admin Live Map focused on this driver.
              Plain <a> (not next/link) so the basePath prefix stays as /admin/... */}
          <a
            href={`/admin/map?driver=${encodeURIComponent(driver.id)}`}
            title="View on Ops Live Map"
            className="inline-flex items-center gap-1 rounded-lg border border-glass-border bg-glass-100 px-2.5 py-1.5 text-xs text-white/50 transition-all hover:border-cyan-neon/30 hover:text-cyan-neon"
          >
            <MapPin className="h-3 w-3" />
            Map
          </a>
          <a
            href={`/admin/fleet?driver=${encodeURIComponent(driver.userId)}`}
            title="Find vehicle on Fleet page"
            className="inline-flex items-center gap-1 rounded-lg border border-glass-border bg-glass-100 px-2.5 py-1.5 text-xs text-white/50 transition-all hover:border-amber-signal/30 hover:text-amber-signal"
          >
            <Truck className="h-3 w-3" />
            Fleet
          </a>
          <button
            onClick={onEdit}
            className="inline-flex items-center gap-1.5 rounded-lg border border-glass-border bg-glass-100 px-2.5 py-1.5 text-xs text-white/50 transition-all hover:border-cyan-signal/30 hover:bg-glass-200 hover:text-white/80"
          >
            <Pencil className="h-3 w-3" />
            Edit
          </button>
        </div>
      </td>
    </tr>
  );
}

// ── Register-driver modal ──────────────────────────────────────────────────────

function RegisterModal({ onClose, onRegistered }: { onClose: () => void; onRegistered: () => void }) {
  const [firstName, setFirstName] = useState("");
  const [lastName,  setLastName]  = useState("");
  const [email,     setEmail]     = useState("");
  const [phone,     setPhone]     = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [result,    setResult]    = useState<RegisterResult | null>(null);

  const canSubmit = firstName.trim() && lastName.trim() && email.trim() && phone.trim() && !submitting;

  async function handleSubmit() {
    if (!canSubmit) return;
    setSubmitting(true);
    const res = await registerDriverApi({ firstName, lastName, email, phone });
    setSubmitting(false);
    setResult(res);
    if (!res.error) onRegistered();
  }

  return (
    <motion.div
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/60 backdrop-blur-sm"
      onClick={(e) => e.target === e.currentTarget && onClose()}
    >
      <motion.div
        initial={{ opacity: 0, scale: 0.95, y: 12 }}
        animate={{ opacity: 1, scale: 1, y: 0 }}
        exit={{ opacity: 0, scale: 0.95, y: 12 }}
        transition={{ duration: 0.25, ease: [0.16, 1, 0.3, 1] }}
        className="w-full max-w-md rounded-2xl border border-glass-border bg-canvas-100 p-6 shadow-2xl"
        style={{ boxShadow: "0 0 40px rgba(0,229,255,0.08), 0 24px 48px rgba(0,0,0,0.6)" }}
      >
        <div className="flex items-start justify-between mb-6">
          <div>
            <h2 className="font-heading text-base font-semibold text-white">Register New Driver</h2>
            <p className="text-xs text-white/40 mt-0.5">Creates an identity user and driver profile.</p>
          </div>
          <button
            onClick={onClose}
            className="flex h-8 w-8 items-center justify-center rounded-lg border border-glass-border text-white/40 hover:text-white/70 hover:bg-glass-200 transition-all"
          >
            <X className="h-3.5 w-3.5" />
          </button>
        </div>

        {!result?.tempPassword && (
          <div className="space-y-3 mb-6">
            <div className="grid grid-cols-2 gap-3">
              <LabeledInput label="First Name"  value={firstName} onChange={setFirstName} placeholder="Juan" />
              <LabeledInput label="Last Name"   value={lastName}  onChange={setLastName}  placeholder="dela Cruz" />
            </div>
            <LabeledInput label="Email"          value={email} onChange={setEmail} placeholder="driver@example.com" type="email" />
            <LabeledInput label="Phone"          value={phone} onChange={setPhone} placeholder="+63 917 123 4567" />
            {result?.error && (
              <p className="text-xs text-red-400 font-mono">{result.error}</p>
            )}
          </div>
        )}

        {result?.tempPassword && !result.error && (
          <div className="mb-6 space-y-3">
            {/* Driver App login instructions */}
            <div className="rounded-xl border border-green-signal/30 bg-green-signal/10 p-4 space-y-2">
              <p className="text-xs font-semibold text-green-signal">✓ Driver registered successfully</p>
              <p className="text-xs text-white/60">
                Tell the driver to download the <span className="text-cyan-signal font-mono">LogisticOS Driver</span> app
                and log in with their phone number:
              </p>
              <div className="rounded-lg border border-glass-border bg-glass-100 px-3 py-2 font-mono text-sm text-white">
                {phone}
              </div>
              <p className="text-xs text-white/40">
                They will receive a one-time code via SMS each time they log in — no password needed.
              </p>
            </div>
            {/* Admin-portal email credentials (for portal access if ever needed) */}
            <div className="rounded-xl border border-glass-border bg-glass-100 p-3 space-y-1">
              <p className="text-xs text-white/30 font-mono uppercase tracking-wider">Portal credentials (admin use only)</p>
              <div className="text-xs font-mono text-white/50">
                <div>Email: <span className="text-white/70">{email}</span></div>
                <div>Temp password: <span className="text-amber-400">{result.tempPassword}</span></div>
              </div>
            </div>
          </div>
        )}

        <div className="flex gap-3">
          <button
            onClick={onClose}
            className="flex-1 rounded-xl border border-glass-border bg-glass-100 py-2.5 text-sm text-white/50 hover:text-white/70 hover:bg-glass-200 transition-all"
          >
            {result?.tempPassword ? "Close" : "Cancel"}
          </button>
          {!result?.tempPassword && (
            <button
              onClick={handleSubmit}
              disabled={!canSubmit}
              className={cn(
                "flex-1 rounded-xl py-2.5 text-sm font-semibold transition-all flex items-center justify-center gap-2",
                canSubmit
                  ? "bg-green-signal/15 border border-green-signal/40 text-green-signal hover:bg-green-signal/25 hover:shadow-[0_0_12px_rgba(0,255,136,0.25)]"
                  : "bg-glass-100 border border-glass-border text-white/30 cursor-not-allowed"
              )}
            >
              {submitting && <Loader2 className="h-3.5 w-3.5 animate-spin" />}
              {submitting ? "Creating..." : "Create Driver"}
            </button>
          )}
        </div>
      </motion.div>
    </motion.div>
  );
}

function LabeledInput({ label, value, onChange, placeholder, type = "text" }: {
  label:       string;
  value:       string;
  onChange:    (v: string) => void;
  placeholder: string;
  type?:       string;
}) {
  return (
    <div>
      <label className="block text-xs font-mono text-white/40 uppercase tracking-wider mb-2">{label}</label>
      <input
        type={type}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        className={cn(
          "w-full rounded-xl border border-glass-border bg-glass-100 px-4 py-2.5",
          "font-mono text-sm text-white placeholder-white/20",
          "focus:outline-none focus:border-cyan-signal/50 focus:bg-glass-200 transition-all",
        )}
      />
    </div>
  );
}

// ── Page ───────────────────────────────────────────────────────────────────────

function DriversPageInner() {
  const router                      = useRouter();
  const searchParams                = useSearchParams();
  // Deep-link from admin-portal: /partner/drivers?focus=<driver_id> pre-populates
  // the search box so the row is visible immediately on load.
  const focusParam                  = searchParams.get("focus") ?? "";
  const [drivers, setDrivers]       = useState<Driver[]>([]);
  const [loading, setLoading]       = useState(true);
  const [fetchError, setFetchError] = useState<string | null>(null);
  const [editingDriver, setEditing] = useState<Driver | null>(null);
  const [registerOpen, setRegisterOpen] = useState(false);
  const [search, setSearch]         = useState(focusParam);
  const [filter, setFilter]         = useState<"all" | DriverType>("all");

  async function loadDrivers() {
    setLoading(true);
    setFetchError(null);
    const data = await fetchDriversFromApi();
    if (data === null) {
      setFetchError("Could not reach driver-ops. Check your connection and try again.");
      setDrivers([]);
    } else {
      setDrivers(data);
    }
    setLoading(false);
  }

  useEffect(() => {
    loadDrivers();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // ── Live roster WS ──────────────────────────────────────────────────────────
  // Patch status in-place from driver-ops RosterEvent frames. Matched by user_id
  // because that's what the server emits (driver_id in the event == identity user_id).
  // Location updates are ignored here — this page doesn't surface coordinates;
  // the Live Map deep-link is where lat/lng matters.
  useRosterEvents((event) => {
    if (event.type !== "status_changed") return;
    setDrivers((prev) => {
      const idx = prev.findIndex((d) => d.userId === event.driver_id);
      if (idx === -1) return prev;
      const uiStatus: Driver["status"] = event.active_route_id
        ? "on_delivery"
        : event.is_online
        ? "active"
        : "offline";
      const next = [...prev];
      next[idx] = { ...next[idx], status: uiStatus };
      return next;
    });
  });

  function handleAssign(driverId: string) {
    router.push(`/orders?assignTo=${encodeURIComponent(driverId)}`);
  }

  const partTimeCount = drivers.filter((d) => d.driverType === "part_time").length;
  const fullTimeCount = drivers.filter((d) => d.driverType === "full_time").length;
  const activeCount   = drivers.filter((d) => d.status !== "offline").length;

  const filtered = drivers.filter((d) => {
    const matchSearch = d.name.toLowerCase().includes(search.toLowerCase()) ||
                        d.id.toLowerCase().includes(search.toLowerCase()) ||
                        d.zone.toLowerCase().includes(search.toLowerCase());
    const matchFilter = filter === "all" || d.driverType === filter;
    return matchSearch && matchFilter;
  });

  async function handleSave(updates: Partial<Driver>) {
    if (!editingDriver) return;
    const patch: Record<string, unknown> = {};
    if (updates.driverType       !== undefined) patch.driver_type             = updates.driverType;
    if (updates.commissionRate    !== undefined) patch.per_delivery_rate_cents = Math.round(updates.commissionRate * 100);
    if (updates.codCommissionRate !== undefined) patch.cod_commission_rate_bps = Math.round(updates.codCommissionRate * 10_000);
    if (updates.zone              !== undefined) patch.zone                    = updates.zone;

    const saved = await patchDriver(editingDriver.id, patch);
    setDrivers((prev) =>
      prev.map((d) =>
        d.id === editingDriver.id ? (saved ?? { ...d, ...updates }) : d
      )
    );
    setEditing(null);
  }

  return (
    <div className="space-y-6">

      {/* KPI strip */}
      <motion.div
        variants={variants.staggerContainer}
        initial="initial"
        animate="animate"
        className="grid grid-cols-2 gap-3 sm:grid-cols-4"
      >
        {[
          { label: "Total Drivers",   value: drivers.length, color: "cyan",   icon: <Users className="h-4 w-4" /> },
          { label: "Active Now",      value: activeCount,    color: "green",  icon: <div className="h-2 w-2 rounded-full bg-green-signal animate-pulse" /> },
          { label: "Full-Time",       value: fullTimeCount,  color: "cyan",   icon: <Briefcase className="h-4 w-4" /> },
          { label: "Part-Time",       value: partTimeCount,  color: "amber",  icon: <Clock className="h-4 w-4" /> },
        ].map(({ label, value, color, icon }, i) => (
          <motion.div key={label} variants={variants.fadeInUp}>
            <GlassCard
              className="flex items-center gap-4 p-4"
            >
              <div
                className={cn(
                  "flex h-10 w-10 flex-shrink-0 items-center justify-center rounded-xl",
                  color === "green" ? "bg-green-signal/10 text-green-signal" :
                  color === "amber" ? "bg-amber-400/10 text-amber-400" :
                  "bg-cyan-signal/10 text-cyan-signal"
                )}
              >
                {icon}
              </div>
              <div>
                <p className="text-2xl font-bold font-heading text-white">{value}</p>
                <p className="text-xs text-white/40 font-mono uppercase tracking-wider">{label}</p>
              </div>
            </GlassCard>
          </motion.div>
        ))}
      </motion.div>

      {/* Table card */}
      <motion.div variants={variants.fadeInUp} initial="initial" animate="animate">
        <GlassCard className="overflow-hidden p-0">

          {/* Table header row — search + filters */}
          <div className="flex flex-col gap-3 border-b border-glass-border p-4 sm:flex-row sm:items-center sm:justify-between">
            <div>
              <h2 className="font-heading text-sm font-semibold text-white">Driver Roster</h2>
              <p className="text-xs text-white/40 mt-0.5">
                Set driver type and commission rates. Changes apply to the driver's next shift.
              </p>
            </div>
            <div className="flex items-center gap-2">
              {/* Search */}
              <div className="relative">
                <Search className="absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-white/30" />
                <input
                  type="text"
                  placeholder="Search drivers..."
                  value={search}
                  onChange={(e) => setSearch(e.target.value)}
                  className={cn(
                    "w-44 rounded-lg border border-glass-border bg-glass-100 py-1.5 pl-8 pr-3",
                    "text-xs text-white placeholder-white/25 font-mono",
                    "focus:outline-none focus:border-cyan-signal/40 focus:bg-glass-200 transition-all"
                  )}
                />
              </div>
              {/* Filter */}
              <div className="flex rounded-lg border border-glass-border overflow-hidden">
                {(["all", "full_time", "part_time"] as const).map((f) => (
                  <button
                    key={f}
                    onClick={() => setFilter(f)}
                    className={cn(
                      "px-3 py-1.5 text-xs font-mono transition-all",
                      filter === f
                        ? "bg-glass-300 text-white/90"
                        : "text-white/40 hover:text-white/60 hover:bg-glass-200"
                    )}
                  >
                    {f === "all" ? "All" : f === "full_time" ? "Full-Time" : "Part-Time"}
                  </button>
                ))}
              </div>
              {/* Register */}
              <button
                onClick={() => setRegisterOpen(true)}
                className={cn(
                  "inline-flex items-center gap-1.5 rounded-lg border px-3 py-1.5 text-xs font-medium transition-all",
                  "border-green-signal/40 bg-green-signal/10 text-green-signal",
                  "hover:bg-green-signal/20 hover:shadow-[0_0_12px_rgba(0,255,136,0.25)]"
                )}
              >
                <UserPlus className="h-3.5 w-3.5" />
                Register Driver
              </button>
            </div>
          </div>

          {/* Table */}
          <div className="overflow-x-auto">
            <table className="w-full text-left">
              <thead>
                <tr className="border-b border-glass-border">
                  {["Driver", "Zone", "Type", "Commission", "Deliveries", "Today's Earnings", "Status", "Compliance", ""].map((h) => (
                    <th
                      key={h}
                      className={cn(
                        "py-3 text-xs font-mono font-medium uppercase tracking-wider text-white/30",
                        h === "" ? "pl-3 pr-4 text-right" : h === "Driver" ? "pl-4 pr-3" : "px-3"
                      )}
                      style={h === "Compliance" ? { minWidth: 130 } : undefined}
                    >
                      {h}
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {!loading && !fetchError && filtered.map((driver) => (
                  <DriverRow
                    key={driver.id}
                    driver={driver}
                    onEdit={() => setEditing(driver)}
                    onAssign={() => handleAssign(driver.id)}
                  />
                ))}
                {loading && (
                  <tr>
                    <td colSpan={9} className="py-12 text-center">
                      <div className="flex items-center justify-center gap-2 text-sm text-white/40 font-mono">
                        <Loader2 className="h-4 w-4 animate-spin" />
                        Loading drivers...
                      </div>
                    </td>
                  </tr>
                )}
                {!loading && fetchError && (
                  <tr>
                    <td colSpan={9} className="py-12 text-center">
                      <div className="space-y-2">
                        <p className="text-sm text-amber-400 font-mono">{fetchError}</p>
                        <button
                          onClick={loadDrivers}
                          className="text-xs text-cyan-signal hover:text-cyan-signal/70 font-mono underline"
                        >
                          Retry
                        </button>
                      </div>
                    </td>
                  </tr>
                )}
                {!loading && !fetchError && drivers.length === 0 && (
                  <tr>
                    <td colSpan={9} className="py-12 text-center">
                      <div className="space-y-3">
                        <p className="text-sm text-white/30 font-mono">No drivers registered yet.</p>
                        <button
                          onClick={() => setRegisterOpen(true)}
                          className={cn(
                            "inline-flex items-center gap-1.5 rounded-lg border px-3 py-1.5 text-xs font-medium transition-all",
                            "border-green-signal/40 bg-green-signal/10 text-green-signal",
                            "hover:bg-green-signal/20"
                          )}
                        >
                          <UserPlus className="h-3.5 w-3.5" />
                          Register your first driver
                        </button>
                      </div>
                    </td>
                  </tr>
                )}
                {!loading && !fetchError && drivers.length > 0 && filtered.length === 0 && (
                  <tr>
                    <td colSpan={9} className="py-12 text-center text-sm text-white/25 font-mono">
                      No drivers match your search
                    </td>
                  </tr>
                )}
              </tbody>
            </table>
          </div>

          {/* Footer */}
          <div className="border-t border-glass-border px-4 py-3 flex items-center justify-between">
            <p className="text-xs text-white/30 font-mono">{filtered.length} of {drivers.length} drivers shown</p>
            <p className="text-xs text-white/25 font-mono">Part-time commission is paid per completed delivery, settled weekly</p>
          </div>
        </GlassCard>
      </motion.div>

      {/* Modals */}
      <AnimatePresence>
        {editingDriver && (
          <EditDrawer
            key="edit-drawer"
            driver={editingDriver}
            onSave={handleSave}
            onClose={() => setEditing(null)}
          />
        )}
        {registerOpen && (
          <RegisterModal
            key="register-modal"
            onClose={() => setRegisterOpen(false)}
            onRegistered={() => {
              loadDrivers();
            }}
          />
        )}
      </AnimatePresence>
    </div>
  );
}

export default function DriversPage() {
  return (
    <Suspense fallback={null}>
      <DriversPageInner />
    </Suspense>
  );
}
