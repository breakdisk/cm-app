"use client";

import { useState, useEffect, useCallback } from "react";
import { motion, AnimatePresence } from "framer-motion";
import {
  X, Edit2, Save, Loader2, MapPin, Truck, Zap, ZapOff,
  Coffee, AlertTriangle, MessageSquare, Send, Clock, Package,
  ChevronDown, ChevronUp, CheckCircle2, XCircle,
} from "lucide-react";
import {
  createDriversApi,
  Driver as ApiDriver,
  DriverTaskHistory,
  UpdateDriverPayload,
} from "@/lib/api/drivers";
import { usePermissions } from "@/hooks/usePermissions";
import { VEHICLE_TYPES, ZONES } from "@/lib/constants/driver-options";

// ── Status display config ─────────────────────────────────────────────────────

interface StatusCfg { label: string; dot: string; text: string }

const STATUS_CFG: Record<string, StatusCfg> = {
  offline:    { label: "Offline",    dot: "bg-white/20",   text: "text-white/50"        },
  available:  { label: "Online",     dot: "bg-green-400",  text: "text-green-400"       },
  en_route:   { label: "En Route",   dot: "bg-cyan-400",   text: "text-cyan-400"        },
  delivering: { label: "Delivering", dot: "bg-green-400",  text: "text-green-400"       },
  returning:  { label: "Returning",  dot: "bg-purple-400", text: "text-purple-400"      },
  on_break:   { label: "On Break",   dot: "bg-amber-400",  text: "text-amber-400"       },
};

type SettableStatus = "available" | "offline" | "on_break";

const SETTABLE_STATUSES: Array<{ value: SettableStatus; label: string; icon: React.ReactNode }> = [
  { value: "available", label: "Online",   icon: <Zap    size={12} /> },
  { value: "on_break",  label: "On Break", icon: <Coffee size={12} /> },
  { value: "offline",   label: "Offline",  icon: <ZapOff size={12} /> },
];

// ── Shared styles ─────────────────────────────────────────────────────────────

const inputCls =
  "w-full rounded-lg border border-white/10 bg-white/[0.04] px-3 py-2 text-sm text-white " +
  "placeholder:text-white/25 outline-none focus:border-cyan-neon/50 focus:ring-1 " +
  "focus:ring-cyan-neon/20 transition-all font-mono";

const labelCls = "block text-[10px] font-mono uppercase tracking-widest text-white/30 mb-1";

// ── Edit form state ───────────────────────────────────────────────────────────

interface EditFields {
  first_name: string;
  last_name: string;
  phone: string;
  driver_type: "full_time" | "part_time";
  vehicle_type: string;
  zone: string;
  per_delivery_rate: string;
  cod_commission_bps: string;
}

// ── Task history helpers ──────────────────────────────────────────────────────

function taskStatusIcon(status: string) {
  if (status === "Completed") return <CheckCircle2 size={12} className="text-green-signal flex-shrink-0" />;
  if (status === "Failed")    return <XCircle      size={12} className="text-red-400 flex-shrink-0" />;
  return                             <XCircle      size={12} className="text-white/30 flex-shrink-0" />;
}

function fmtTime(iso: string | null | undefined): string {
  if (!iso) return "—";
  return new Date(iso).toLocaleString("en-PH", { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });
}

function fmtCod(cents: number | null | undefined): string | null {
  if (!cents) return null;
  return `₱${(cents / 100).toLocaleString("en-PH", { minimumFractionDigits: 0 })}`;
}

// ── Props ─────────────────────────────────────────────────────────────────────

interface Props {
  driverId: string | null;
  driverName: string;
  onClose: () => void;
  onUpdated: () => void;
}

// ── Component ─────────────────────────────────────────────────────────────────

export function DriverDetailDrawer({ driverId, driverName, onClose, onUpdated }: Props) {
  // ── core profile ──────────────────────────────────────────────────────────
  const [driver,  setDriver]  = useState<ApiDriver | null>(null);
  const [loading, setLoading] = useState(false);
  const [error,   setError]   = useState<string | null>(null);

  // ── edit form ─────────────────────────────────────────────────────────────
  const [editing,    setEditing]    = useState(false);
  const [editFields, setEditFields] = useState<EditFields | null>(null);
  const [saving,     setSaving]     = useState(false);
  const [saveError,  setSaveError]  = useState<string | null>(null);

  // ── status / activate ─────────────────────────────────────────────────────
  const [togglingStatus, setTogglingStatus] = useState<string | null>(null);
  const [togglingActive, setTogglingActive] = useState(false);
  const [cancellingTasks, setCancellingTasks] = useState(false);

  // ── instruction ───────────────────────────────────────────────────────────
  const [instruction,       setInstruction]       = useState("");
  const [sendingInstruction,setSendingInstruction] = useState(false);
  const [instructionSent,   setInstructionSent]   = useState(false);

  // ── task history ──────────────────────────────────────────────────────────
  const [historyOpen,    setHistoryOpen]    = useState(false);
  const [history,        setHistory]        = useState<DriverTaskHistory[]>([]);
  const [historyLoading, setHistoryLoading] = useState(false);
  const [historyError,   setHistoryError]   = useState<string | null>(null);

  const { hasPermission } = usePermissions();
  const canManage = hasPermission("drivers:manage");

  // ── fetch full driver profile ─────────────────────────────────────────────

  const fetchDriver = useCallback(async () => {
    if (!driverId) return;
    setLoading(true);
    setError(null);
    try {
      const res = await createDriversApi().getDriver(driverId);
      setDriver(res.data);
    } catch {
      setError("Failed to load driver profile.");
    } finally {
      setLoading(false);
    }
  }, [driverId]);

  useEffect(() => {
    if (driverId) {
      // Reset per-drawer state when a new driver is opened
      setEditing(false);
      setEditFields(null);
      setSaveError(null);
      setError(null);
      setHistoryOpen(false);
      setHistory([]);
      setHistoryError(null);
      setInstruction("");
      fetchDriver();
    }
  }, [driverId, fetchDriver]);

  // ── task history (lazy) ───────────────────────────────────────────────────

  const fetchHistory = useCallback(async () => {
    if (!driverId) return;
    setHistoryLoading(true);
    setHistoryError(null);
    try {
      const res = await createDriversApi().getTaskHistory(driverId);
      setHistory(res.data ?? []);
    } catch {
      setHistoryError("Failed to load task history.");
    } finally {
      setHistoryLoading(false);
    }
  }, [driverId]);

  function toggleHistory() {
    const next = !historyOpen;
    setHistoryOpen(next);
    if (next && history.length === 0 && !historyLoading) fetchHistory();
  }

  // ── edit helpers ──────────────────────────────────────────────────────────

  function startEdit() {
    if (!driver) return;
    setEditFields({
      first_name:         driver.first_name,
      last_name:          driver.last_name,
      phone:              driver.phone,
      driver_type:        driver.driver_type ?? "full_time",
      vehicle_type:       driver.vehicle_type ?? "",
      zone:               driver.zone ?? "",
      per_delivery_rate:  String((driver.per_delivery_rate_cents ?? 0) / 100),
      cod_commission_bps: String(driver.cod_commission_rate_bps ?? 0),
    });
    setSaveError(null);
    setEditing(true);
  }

  function cancelEdit() {
    setEditing(false);
    setEditFields(null);
    setSaveError(null);
  }

  async function handleSave() {
    if (!driver || !editFields) return;
    setSaving(true);
    setSaveError(null);
    try {
      const payload: UpdateDriverPayload = {
        first_name:              editFields.first_name  || undefined,
        last_name:               editFields.last_name   || undefined,
        phone:                   editFields.phone       || undefined,
        driver_type:             editFields.driver_type,
        vehicle_type:            editFields.vehicle_type || undefined,
        zone:                    editFields.zone         || undefined,
        per_delivery_rate_cents: Math.round(parseFloat(editFields.per_delivery_rate || "0") * 100),
        cod_commission_rate_bps: parseInt(editFields.cod_commission_bps || "0", 10),
      };
      const res = await createDriversApi().updateDriver(driver.id, payload);
      setDriver(res.data);
      setEditing(false);
      setEditFields(null);
      onUpdated();
    } catch (err: unknown) {
      setSaveError((err as { message?: string }).message ?? "Failed to save changes.");
    } finally {
      setSaving(false);
    }
  }

  // ── actions ───────────────────────────────────────────────────────────────

  async function handleSetStatus(status: SettableStatus) {
    if (!driver) return;
    setTogglingStatus(status);
    setError(null);
    try {
      const res = await createDriversApi().setDriverStatus(driver.id, status);
      setDriver(res.data);
      onUpdated();
    } catch (err: unknown) {
      setError((err as { message?: string }).message ?? "Failed to update status.");
    } finally {
      setTogglingStatus(null);
    }
  }

  async function handleToggleActive() {
    if (!driver) return;
    setTogglingActive(true);
    setError(null);
    try {
      const res = await createDriversApi().updateDriver(driver.id, { is_active: !driver.is_active });
      setDriver(res.data);
      onUpdated();
    } catch (err: unknown) {
      setError((err as { message?: string }).message ?? "Failed to update activation.");
    } finally {
      setTogglingActive(false);
    }
  }

  async function handleCancelTasks() {
    if (!driver) return;
    setCancellingTasks(true);
    setError(null);
    try {
      await createDriversApi().cancelTasks(driver.id);
      await fetchDriver();
      // Refresh history too if it was open
      if (historyOpen) fetchHistory();
      onUpdated();
    } catch (err: unknown) {
      setError((err as { message?: string }).message ?? "Failed to cancel tasks.");
    } finally {
      setCancellingTasks(false);
    }
  }

  async function handleSendInstruction() {
    if (!driver || !instruction.trim()) return;
    setSendingInstruction(true);
    try {
      // Backend requires instruction_type + message; "custom" covers free-form ops messages.
      await createDriversApi().sendInstruction(driver.id, instruction.trim(), "custom");
      setInstruction("");
      setInstructionSent(true);
      setTimeout(() => setInstructionSent(false), 3000);
    } catch (err: unknown) {
      setError((err as { message?: string }).message ?? "Failed to send instruction.");
    } finally {
      setSendingInstruction(false);
    }
  }

  // ── derived ───────────────────────────────────────────────────────────────

  const statusCfg    = driver ? (STATUS_CFG[driver.status ?? "offline"] ?? STATUS_CFG.offline) : null;
  // active_route_id is now in the Driver interface — this correctly reflects backend state
  const hasActiveRoute = Boolean(driver?.active_route_id);

  // ── render ────────────────────────────────────────────────────────────────

  return (
    <AnimatePresence>
      {driverId && (
        <motion.div
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          className="fixed inset-0 z-50 flex items-end justify-end bg-black/40 backdrop-blur-sm"
          onClick={(e) => { if (e.target === e.currentTarget) onClose(); }}
        >
          <motion.div
            initial={{ x: "100%" }}
            animate={{ x: 0 }}
            exit={{ x: "100%" }}
            transition={{ type: "spring", stiffness: 320, damping: 30 }}
            className="h-full w-full max-w-sm border-l border-white/10 bg-[#050810]/95 backdrop-blur-xl flex flex-col"
          >
            {/* ── Header ──────────────────────────────────────────────────── */}
            <div className="flex items-center justify-between border-b border-white/10 px-5 py-4 flex-shrink-0">
              <div>
                <h2 className="font-heading text-base font-semibold text-white">
                  {driver ? `${driver.first_name} ${driver.last_name}` : driverName}
                </h2>
                <p className="text-xs font-mono text-white/40">{driver?.phone ?? "Loading…"}</p>
              </div>
              <div className="flex items-center gap-2">
                {canManage && driver && !editing && (
                  <button
                    onClick={startEdit}
                    className="flex items-center gap-1 rounded-lg border border-white/10 bg-white/[0.04] px-2.5 py-1.5 text-xs text-white/50 hover:text-white transition-colors"
                  >
                    <Edit2 size={11} /> Edit
                  </button>
                )}
                <button
                  onClick={onClose}
                  className="text-white/40 hover:text-white transition-colors ml-1"
                >
                  <X size={18} />
                </button>
              </div>
            </div>

            {/* ── Status row ──────────────────────────────────────────────── */}
            {driver && statusCfg && (
              <div className="border-b border-white/10 px-5 py-3 flex items-center gap-3 flex-shrink-0">
                <span className={`h-2.5 w-2.5 rounded-full flex-shrink-0 ${statusCfg.dot}`} />
                <span className={`text-sm font-mono capitalize ${statusCfg.text}`}>{statusCfg.label}</span>
                {driver.vehicle_type && (
                  <span className="ml-auto text-xs font-mono text-white/30 flex items-center gap-1">
                    <Truck size={11} /> {driver.vehicle_type}
                  </span>
                )}
                {!driver.is_active && (
                  <span className="ml-auto rounded-full border border-red-500/30 bg-red-500/10 px-2 py-0.5 text-[10px] font-mono text-red-400">
                    Suspended
                  </span>
                )}
              </div>
            )}

            {/* ── Loading / Error banner ───────────────────────────────────── */}
            {loading && (
              <div className="flex items-center justify-center py-12">
                <Loader2 size={20} className="animate-spin text-white/30" />
              </div>
            )}
            {error && (
              <div className="mx-5 mt-4 rounded-lg border border-red-500/20 bg-red-500/10 px-3 py-2 text-xs text-red-400 flex-shrink-0">
                {error}
              </div>
            )}

            {/* ── Scrollable body ──────────────────────────────────────────── */}
            {driver && !loading && (
              <div className="flex-1 overflow-y-auto">

                {/* ── Profile (read-only) ────────────────────────────────── */}
                {!editing && (
                  <div className="px-5 py-4 flex flex-col gap-3">
                    <h3 className={labelCls}>Profile</h3>
                    <div className="grid grid-cols-2 gap-2.5">
                      <InfoCell label="Zone">{driver.zone ?? "—"}</InfoCell>
                      <InfoCell label="Type">{(driver.driver_type ?? "—").replace("_", " ")}</InfoCell>
                      <InfoCell label="Rate / Delivery">
                        {driver.per_delivery_rate_cents != null
                          ? `₱${(driver.per_delivery_rate_cents / 100).toFixed(0)}`
                          : "—"}
                      </InfoCell>
                      <InfoCell label="COD Commission">
                        {driver.cod_commission_rate_bps != null
                          ? `${driver.cod_commission_rate_bps} bps`
                          : "—"}
                      </InfoCell>
                      <InfoCell label="Active Route" span2 mono>
                        {driver.active_route_id
                          ? driver.active_route_id.slice(0, 8) + "…"
                          : "None"}
                      </InfoCell>
                    </div>

                    {/* GPS */}
                    {driver.lat != null && driver.lng != null ? (
                      <div className="rounded-lg border border-white/5 bg-white/[0.02] px-3 py-2">
                        <p className={`${labelCls} flex items-center gap-1`}>
                          <MapPin size={9} /> Last Location
                        </p>
                        <p className="text-sm font-mono text-cyan-400/80">
                          {driver.lat.toFixed(5)}, {driver.lng.toFixed(5)}
                        </p>
                        {/* last_location_at is the canonical timestamp from DriverDto */}
                        {(driver.last_location_at ?? driver.last_seen_at) && (
                          <p className="text-[10px] font-mono text-white/25 mt-0.5">
                            {new Date((driver.last_location_at ?? driver.last_seen_at)!).toLocaleTimeString("en-PH")}
                          </p>
                        )}
                      </div>
                    ) : (
                      <p className="text-xs font-mono text-white/25 italic px-1">No GPS signal yet</p>
                    )}
                  </div>
                )}

                {/* ── Edit Form ──────────────────────────────────────────── */}
                {editing && editFields && (
                  <div className="px-5 py-4 flex flex-col gap-4">
                    <h3 className={labelCls}>Edit Profile</h3>

                    <div className="grid grid-cols-2 gap-3">
                      <div>
                        <label className={labelCls}>First Name</label>
                        <input className={inputCls} value={editFields.first_name}
                          onChange={(e) => setEditFields((p) => p && ({ ...p, first_name: e.target.value }))} />
                      </div>
                      <div>
                        <label className={labelCls}>Last Name</label>
                        <input className={inputCls} value={editFields.last_name}
                          onChange={(e) => setEditFields((p) => p && ({ ...p, last_name: e.target.value }))} />
                      </div>
                    </div>

                    <div>
                      <label className={labelCls}>Phone</label>
                      <input className={inputCls} value={editFields.phone}
                        onChange={(e) => setEditFields((p) => p && ({ ...p, phone: e.target.value }))} />
                    </div>

                    <div className="grid grid-cols-2 gap-3">
                      <div>
                        <label className={labelCls}>Driver Type</label>
                        <select className={inputCls} value={editFields.driver_type}
                          onChange={(e) => setEditFields((p) => p && ({ ...p, driver_type: e.target.value as "full_time" | "part_time" }))}>
                          <option value="full_time">Full Time</option>
                          <option value="part_time">Part Time</option>
                        </select>
                      </div>
                      <div>
                        <label className={labelCls}>Vehicle Type</label>
                        <select className={inputCls} value={editFields.vehicle_type}
                          onChange={(e) => setEditFields((p) => p && ({ ...p, vehicle_type: e.target.value }))}>
                          {VEHICLE_TYPES.map((v) => <option key={v} value={v}>{v}</option>)}
                        </select>
                      </div>
                    </div>

                    <div>
                      <label className={labelCls}>Zone / Territory</label>
                      <select className={inputCls} value={editFields.zone}
                        onChange={(e) => setEditFields((p) => p && ({ ...p, zone: e.target.value }))}>
                        <option value="">— Unassigned —</option>
                        {ZONES.map((z) => <option key={z} value={z}>{z}</option>)}
                      </select>
                    </div>

                    <div className="grid grid-cols-2 gap-3">
                      <div>
                        <label className={labelCls}>Rate / Delivery (₱)</label>
                        <input className={inputCls} type="number" min="0" step="0.01"
                          value={editFields.per_delivery_rate}
                          onChange={(e) => setEditFields((p) => p && ({ ...p, per_delivery_rate: e.target.value }))} />
                      </div>
                      <div>
                        <label className={labelCls}>COD Commission (bps)</label>
                        <input className={inputCls} type="number" min="0" max="10000"
                          value={editFields.cod_commission_bps}
                          onChange={(e) => setEditFields((p) => p && ({ ...p, cod_commission_bps: e.target.value }))} />
                        <p className="text-[10px] font-mono text-white/25 mt-1">100 bps = 1%</p>
                      </div>
                    </div>

                    {saveError && (
                      <p className="rounded-lg bg-red-500/10 border border-red-500/20 px-3 py-2 text-xs text-red-400">
                        {saveError}
                      </p>
                    )}

                    <div className="flex gap-2">
                      <button onClick={cancelEdit}
                        className="flex-1 rounded-lg border border-white/10 bg-white/[0.03] px-3 py-2 text-sm text-white/50 hover:text-white transition-colors">
                        Cancel
                      </button>
                      <button onClick={handleSave} disabled={saving}
                        className="flex-[2] flex items-center justify-center gap-1.5 rounded-lg border border-cyan-neon/30 bg-cyan-neon/10 px-3 py-2 text-sm font-semibold text-cyan-neon hover:bg-cyan-neon/20 transition-all disabled:opacity-50">
                        {saving ? <Loader2 size={13} className="animate-spin" /> : <Save size={13} />}
                        {saving ? "Saving…" : "Save Changes"}
                      </button>
                    </div>
                  </div>
                )}

                {/* ── Set Status ────────────────────────────────────────── */}
                {canManage && !editing && (
                  <div className="border-t border-white/5 px-5 py-4 flex flex-col gap-3">
                    <h3 className={labelCls}>Set Status</h3>
                    {hasActiveRoute && (
                      <p className="flex items-center gap-1.5 text-xs font-mono text-amber-400/70">
                        <AlertTriangle size={11} /> Cancel active route before changing status
                      </p>
                    )}
                    <div className="flex gap-2">
                      {SETTABLE_STATUSES.map(({ value, label, icon }) => {
                        const isCurrent = driver.status === value;
                        const isLoading = togglingStatus === value;
                        return (
                          <button
                            key={value}
                            onClick={() => handleSetStatus(value)}
                            disabled={isCurrent || hasActiveRoute || Boolean(togglingStatus)}
                            className={`flex flex-1 items-center justify-center gap-1.5 rounded-lg border px-2 py-2 text-xs font-mono transition-all disabled:opacity-40 ${
                              isCurrent
                                ? "border-cyan-neon/40 bg-cyan-neon/10 text-cyan-neon"
                                : "border-white/10 bg-white/[0.03] text-white/50 hover:text-white hover:border-white/20"
                            }`}
                          >
                            {isLoading ? <Loader2 size={11} className="animate-spin" /> : icon}
                            {label}
                          </button>
                        );
                      })}
                    </div>
                  </div>
                )}

                {/* ── Send Instruction ──────────────────────────────────── */}
                {canManage && !editing && (
                  <div className="border-t border-white/5 px-5 py-4 flex flex-col gap-2">
                    <h3 className={`${labelCls} flex items-center gap-1.5`}>
                      <MessageSquare size={10} /> Send Instruction
                    </h3>
                    <div className="flex gap-2">
                      <input
                        className={`${inputCls} flex-1`}
                        placeholder="Type a message to this driver…"
                        value={instruction}
                        onChange={(e) => setInstruction(e.target.value)}
                        onKeyDown={(e) => {
                          if (e.key === "Enter" && !e.shiftKey) {
                            e.preventDefault();
                            handleSendInstruction();
                          }
                        }}
                      />
                      <button
                        onClick={handleSendInstruction}
                        disabled={!instruction.trim() || sendingInstruction}
                        className="flex items-center gap-1.5 rounded-lg border border-purple-plasma/30 bg-purple-plasma/10 px-3 py-2 text-xs font-mono text-purple-plasma hover:bg-purple-plasma/20 transition-all disabled:opacity-40"
                      >
                        {sendingInstruction
                          ? <Loader2 size={12} className="animate-spin" />
                          : <Send size={12} />}
                      </button>
                    </div>
                    {instructionSent && (
                      <p className="text-[10px] font-mono text-green-signal/70">Instruction sent.</p>
                    )}
                  </div>
                )}

                {/* ── Task History ──────────────────────────────────────── */}
                {!editing && (
                  <div className="border-t border-white/5">
                    {/* Collapsible header */}
                    <button
                      onClick={toggleHistory}
                      className="w-full flex items-center justify-between px-5 py-3.5 text-left hover:bg-white/[0.02] transition-colors"
                    >
                      <span className={`${labelCls} mb-0 flex items-center gap-1.5`}>
                        <Clock size={10} /> Task History
                        {history.length > 0 && (
                          <span className="ml-1 rounded-full bg-white/10 px-1.5 py-0.5 text-[9px] font-mono text-white/50">
                            {history.length}
                          </span>
                        )}
                      </span>
                      {historyOpen
                        ? <ChevronUp size={13} className="text-white/30" />
                        : <ChevronDown size={13} className="text-white/30" />}
                    </button>

                    {historyOpen && (
                      <div className="px-5 pb-4">
                        {historyLoading && (
                          <div className="flex items-center justify-center py-6">
                            <Loader2 size={16} className="animate-spin text-white/30" />
                          </div>
                        )}

                        {historyError && (
                          <p className="rounded-lg bg-red-500/10 border border-red-500/20 px-3 py-2 text-xs text-red-400">
                            {historyError}
                          </p>
                        )}

                        {!historyLoading && !historyError && history.length === 0 && (
                          <p className="text-xs font-mono text-white/25 italic py-2">
                            No completed tasks yet.
                          </p>
                        )}

                        {!historyLoading && history.length > 0 && (
                          <div className="flex flex-col divide-y divide-white/5">
                            {history.map((t) => (
                              <div key={t.id} className="py-2.5">
                                <div className="flex items-start gap-2">
                                  {taskStatusIcon(t.status)}
                                  <div className="flex-1 min-w-0">
                                    <div className="flex items-center gap-1.5 flex-wrap">
                                      <span className="text-xs font-mono text-white/80 truncate">
                                        {t.customer_name}
                                      </span>
                                      <span className={`text-[10px] font-mono flex items-center gap-0.5 ${
                                        t.task_type === "Pickup" ? "text-purple-400/70" : "text-cyan-400/70"
                                      }`}>
                                        <Package size={9} />
                                        {t.task_type}
                                      </span>
                                    </div>

                                    {t.tracking_number && (
                                      <p className="text-[10px] font-mono text-white/30 truncate mt-0.5">
                                        {t.tracking_number}
                                      </p>
                                    )}

                                    <div className="flex items-center gap-2 mt-0.5 flex-wrap">
                                      <span className="text-[10px] font-mono text-white/25">
                                        {fmtTime(t.completed_at)}
                                      </span>
                                      {fmtCod(t.cod_amount_cents) && (
                                        <span className="text-[10px] font-mono text-amber-signal/70">
                                          {fmtCod(t.cod_amount_cents)}
                                        </span>
                                      )}
                                    </div>

                                    {t.failed_reason && (
                                      <p className="text-[10px] font-mono text-red-400/60 mt-0.5 italic truncate">
                                        {t.failed_reason}
                                      </p>
                                    )}
                                  </div>
                                </div>
                              </div>
                            ))}
                          </div>
                        )}
                      </div>
                    )}
                  </div>
                )}

              </div>
            )}

            {/* ── Footer Actions ──────────────────────────────────────────── */}
            {driver && !loading && canManage && !editing && (
              <div className="border-t border-white/10 px-5 py-4 flex flex-col gap-2 flex-shrink-0">
                {/* Activate / Suspend */}
                <button
                  onClick={handleToggleActive}
                  disabled={togglingActive}
                  className={`w-full flex items-center justify-center gap-1.5 rounded-lg border px-4 py-2 text-sm font-mono transition-colors disabled:opacity-40 ${
                    driver.is_active
                      ? "border-red-500/30 bg-red-500/10 text-red-400 hover:bg-red-500/20"
                      : "border-green-signal/30 bg-green-signal/10 text-green-signal hover:bg-green-signal/20"
                  }`}
                >
                  {togglingActive && <Loader2 size={13} className="animate-spin" />}
                  {driver.is_active ? "Suspend Driver" : "Activate Driver"}
                </button>

                {/* Cancel Active Tasks — only enabled when driver has an active route */}
                <button
                  onClick={handleCancelTasks}
                  disabled={cancellingTasks || !hasActiveRoute}
                  className="w-full rounded-lg border border-white/10 bg-white/[0.03] px-4 py-2 text-sm font-mono text-white/50 hover:text-red-400 hover:border-red-500/30 transition-colors disabled:opacity-30"
                >
                  {cancellingTasks ? "Cancelling tasks…" : "Cancel Active Tasks"}
                </button>
              </div>
            )}
          </motion.div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}

// ── Small helper component ────────────────────────────────────────────────────

function InfoCell({
  label, children, span2, mono,
}: {
  label: string;
  children: React.ReactNode;
  span2?: boolean;
  mono?: boolean;
}) {
  return (
    <div className={`rounded-lg border border-white/5 bg-white/[0.02] px-3 py-2 ${span2 ? "col-span-2" : ""}`}>
      <p className={labelCls}>{label}</p>
      <p className={`text-sm text-white/80 ${mono ? "font-mono truncate" : "capitalize"}`}>{children}</p>
    </div>
  );
}
