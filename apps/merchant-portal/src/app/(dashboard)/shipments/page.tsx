"use client";
/**
 * Merchant Portal — Shipments Page
 * Full shipment list with filters, status badges, bulk actions.
 * Includes New Shipment modal with Local / International (Balikbayan) toggle.
 */
import { useState, useRef, useEffect, useCallback, Suspense } from "react";
import { useSearchParams, useRouter } from "next/navigation";
import { motion, AnimatePresence } from "framer-motion";
import { variants } from "@/lib/design-system/tokens";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge, BadgeVariant } from "@/components/ui/neon-badge";
import {
  Search, Download, Plus, Upload, X, Globe, Home,
  Ship, PlaneTakeoff, ArrowUpDown, ChevronLeft, ChevronRight, Check,
  FileText, CheckCircle2, AlertCircle, ExternalLink, Copy, Share2, QrCode, Phone, User,
} from "lucide-react";
import { cn } from "@/lib/design-system/cn";
import { authFetch } from "@/lib/auth/auth-fetch";

// ── Types ──────────────────────────────────────────────────────────────────────

type ShipmentStatus =
  | "pending" | "confirmed" | "pickup_assigned" | "picked_up"
  | "in_transit" | "at_hub" | "out_for_delivery" | "delivery_attempted"
  | "delivered" | "partial_delivery" | "piece_exception" | "customs_hold"
  | "failed" | "cancelled" | "returned";

interface Shipment {
  id: string;
  tracking_number: string;
  recipient_name: string;
  destination: string;
  status: ShipmentStatus;
  cod_amount?: number;
  created_at: string;
  eta?: string;
  // Enriched client-side from dispatch queue + driver roster.
  driver_name?: string;
  driver_phone?: string;
}


const STATUS_MAP: Partial<Record<ShipmentStatus, { label: string; variant: BadgeVariant }>> = {
  pending:            { label: "Pending",           variant: "amber"  },
  confirmed:          { label: "Confirmed",         variant: "amber"  },
  pickup_assigned:    { label: "Pickup Assigned",   variant: "cyan"   },
  picked_up:          { label: "Picked Up",         variant: "cyan"   },
  in_transit:         { label: "In Transit",        variant: "purple" },
  at_hub:             { label: "At Hub",            variant: "purple" },
  out_for_delivery:   { label: "Out for Delivery",  variant: "green"  },
  delivery_attempted: { label: "Attempted",         variant: "amber"  },
  delivered:          { label: "Delivered",          variant: "green"  },
  partial_delivery:   { label: "Partial Delivery",  variant: "amber"  },
  piece_exception:    { label: "Exception",         variant: "red"    },
  customs_hold:       { label: "Customs Hold",      variant: "purple" },
  failed:             { label: "Failed",            variant: "red"    },
  cancelled:          { label: "Cancelled",         variant: "red"    },
  returned:           { label: "Returned",          variant: "red"    },
};

const STATUS_FILTERS: Array<{ label: string; value: ShipmentStatus | "all" }> = [
  { label: "All",           value: "all" },
  { label: "Active",        value: "in_transit" },
  { label: "Out Today",     value: "out_for_delivery" },
  { label: "Delivered",     value: "delivered" },
  { label: "Failed",        value: "failed" },
];

// ── Summary stats ──────────────────────────────────────────────────────────────

const DELIVERED_STATUSES: ReadonlySet<ShipmentStatus> = new Set(["delivered", "partial_delivery"]);
const FAILED_STATUSES:    ReadonlySet<ShipmentStatus> = new Set(["failed", "cancelled", "returned"]);

function computeStats(shipments: Shipment[]) {
  const total     = shipments.length;
  const delivered = shipments.filter(s => DELIVERED_STATUSES.has(s.status)).length;
  const failed    = shipments.filter(s => FAILED_STATUSES.has(s.status)).length;
  const active    = total - delivered - failed;
  return [
    { label: "Total",     value: total,     color: "cyan"   as const },
    { label: "Active",    value: active,    color: "purple" as const },
    { label: "Delivered", value: delivered, color: "green"  as const },
    { label: "Failed",    value: failed,    color: "red"    as const },
  ];
}

// ── API helpers ───────────────────────────────────────────────────────────────

const ORDER_INTAKE_URL        = process.env.NEXT_PUBLIC_API_URL                ?? "http://localhost:8000";
const DELIVERY_EXPERIENCE_URL = process.env.NEXT_PUBLIC_DELIVERY_EXPERIENCE_URL ?? "http://localhost:8007";

// ── New Shipment Modal ────────────────────────────────────────────────────────

type FreightMode = "sea" | "air";

interface Country { code: string; label: string; flag: string; popular?: boolean; }

const POPULAR_COUNTRIES: Country[] = [
  { code: "PH", label: "Philippines",    flag: "🇵🇭", popular: true },
  { code: "US", label: "United States",  flag: "🇺🇸", popular: true },
  { code: "CA", label: "Canada",         flag: "🇨🇦", popular: true },
  { code: "GB", label: "United Kingdom", flag: "🇬🇧", popular: true },
  { code: "IN", label: "India",          flag: "🇮🇳", popular: true },
  { code: "SA", label: "Saudi Arabia",   flag: "🇸🇦", popular: true },
  { code: "QA", label: "Qatar",          flag: "🇶🇦", popular: true },
  { code: "OM", label: "Oman",           flag: "🇴🇲", popular: true },
  { code: "KW", label: "Kuwait",         flag: "🇰🇼", popular: true },
  { code: "BH", label: "Bahrain",        flag: "🇧🇭", popular: true },
];

const ALL_COUNTRIES: Country[] = [
  { code: "AE", label: "United Arab Emirates", flag: "🇦🇪" },
  { code: "AU", label: "Australia",            flag: "🇦🇺" },
  { code: "AT", label: "Austria",              flag: "🇦🇹" },
  { code: "BE", label: "Belgium",              flag: "🇧🇪" },
  { code: "BR", label: "Brazil",               flag: "🇧🇷" },
  { code: "CN", label: "China",                flag: "🇨🇳" },
  { code: "DK", label: "Denmark",              flag: "🇩🇰" },
  { code: "EG", label: "Egypt",                flag: "🇪🇬" },
  { code: "FI", label: "Finland",              flag: "🇫🇮" },
  { code: "FR", label: "France",               flag: "🇫🇷" },
  { code: "DE", label: "Germany",              flag: "🇩🇪" },
  { code: "GR", label: "Greece",               flag: "🇬🇷" },
  { code: "HK", label: "Hong Kong",            flag: "🇭🇰" },
  { code: "ID", label: "Indonesia",            flag: "🇮🇩" },
  { code: "IE", label: "Ireland",              flag: "🇮🇪" },
  { code: "IL", label: "Israel",               flag: "🇮🇱" },
  { code: "IT", label: "Italy",                flag: "🇮🇹" },
  { code: "JP", label: "Japan",                flag: "🇯🇵" },
  { code: "JO", label: "Jordan",               flag: "🇯🇴" },
  { code: "KR", label: "South Korea",          flag: "🇰🇷" },
  { code: "LB", label: "Lebanon",              flag: "🇱🇧" },
  { code: "MY", label: "Malaysia",             flag: "🇲🇾" },
  { code: "MX", label: "Mexico",               flag: "🇲🇽" },
  { code: "NL", label: "Netherlands",          flag: "🇳🇱" },
  { code: "NZ", label: "New Zealand",          flag: "🇳🇿" },
  { code: "NG", label: "Nigeria",              flag: "🇳🇬" },
  { code: "NO", label: "Norway",               flag: "🇳🇴" },
  { code: "PK", label: "Pakistan",             flag: "🇵🇰" },
  { code: "PT", label: "Portugal",             flag: "🇵🇹" },
  { code: "RU", label: "Russia",               flag: "🇷🇺" },
  { code: "SG", label: "Singapore",            flag: "🇸🇬" },
  { code: "ZA", label: "South Africa",         flag: "🇿🇦" },
  { code: "ES", label: "Spain",                flag: "🇪🇸" },
  { code: "SE", label: "Sweden",               flag: "🇸🇪" },
  { code: "CH", label: "Switzerland",          flag: "🇨🇭" },
  { code: "TW", label: "Taiwan",               flag: "🇹🇼" },
  { code: "TH", label: "Thailand",             flag: "🇹🇭" },
  { code: "TR", label: "Turkey",               flag: "🇹🇷" },
  { code: "VN", label: "Vietnam",              flag: "🇻🇳" },
].sort((a, b) => a.label.localeCompare(b.label));

const DEST_COUNTRIES: Country[] = [...POPULAR_COUNTRIES, ...ALL_COUNTRIES];

// ── Searchable Country Select ─────────────────────────────────────────────────

function CountrySelect({ value, onChange }: { value: string; onChange: (code: string) => void }) {
  const [open,   setOpen]   = useState(false);
  const [search, setSearch] = useState("");
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    function handler(e: MouseEvent) {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    }
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, []);

  const selected = DEST_COUNTRIES.find((c) => c.code === value);
  const q = search.toLowerCase();
  const filteredPopular = POPULAR_COUNTRIES.filter(
    (c) => !q || c.label.toLowerCase().includes(q) || c.code.toLowerCase().includes(q)
  );
  const filteredOthers = ALL_COUNTRIES.filter(
    (c) => !q || c.label.toLowerCase().includes(q) || c.code.toLowerCase().includes(q)
  );

  function pick(code: string) { onChange(code); setOpen(false); setSearch(""); }

  return (
    <div ref={ref} className="relative">
      {/* Trigger */}
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        className={cn(
          "w-full flex items-center gap-3 rounded-xl border bg-glass-100 px-4 py-2.5 text-sm text-left transition-all",
          open ? "border-purple-plasma/50 bg-glass-200" : "border-glass-border hover:border-glass-border-bright"
        )}
      >
        <span className="text-lg leading-none">{selected?.flag}</span>
        <span className="flex-1 text-white font-mono text-sm">{selected?.label ?? "Select country"}</span>
        <span className="text-white/30 font-mono text-xs">{selected?.code}</span>
        <svg className={cn("h-4 w-4 text-white/30 transition-transform", open && "rotate-180")} viewBox="0 0 20 20" fill="currentColor">
          <path fillRule="evenodd" d="M5.23 7.21a.75.75 0 011.06.02L10 11.168l3.71-3.938a.75.75 0 111.08 1.04l-4.25 4.5a.75.75 0 01-1.08 0l-4.25-4.5a.75.75 0 01.02-1.06z" clipRule="evenodd" />
        </svg>
      </button>

      {/* Dropdown */}
      {open && (
        <div className="absolute left-0 right-0 top-[calc(100%+4px)] z-50 rounded-xl border border-glass-border bg-canvas shadow-2xl overflow-hidden"
          style={{ boxShadow: "0 16px 40px rgba(0,0,0,0.6), 0 0 0 1px rgba(168,85,247,0.15)" }}>

          {/* Search */}
          <div className="flex items-center gap-2 border-b border-glass-border px-3 py-2.5">
            <Search className="h-3.5 w-3.5 flex-shrink-0 text-white/30" />
            <input
              autoFocus
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder="Search country..."
              className="flex-1 bg-transparent text-xs text-white placeholder-white/25 font-mono outline-none"
            />
            {search && (
              <button onClick={() => setSearch("")} className="text-white/30 hover:text-white/60">
                <X className="h-3 w-3" />
              </button>
            )}
          </div>

          {/* List */}
          <div className="max-h-56 overflow-y-auto overscroll-contain">
            {/* Popular */}
            {filteredPopular.length > 0 && (
              <>
                {!search && (
                  <p className="px-3 pt-2.5 pb-1 text-2xs font-mono text-white/25 uppercase tracking-wider">
                    Common Destinations
                  </p>
                )}
                {filteredPopular.map((c) => (
                  <button key={c.code} onClick={() => pick(c.code)}
                    className={cn(
                      "w-full flex items-center gap-3 px-3 py-2 text-left text-sm transition-colors hover:bg-glass-200",
                      value === c.code && "bg-purple-plasma/10"
                    )}>
                    <span className="text-base leading-none w-6 text-center">{c.flag}</span>
                    <span className={cn("flex-1 font-mono text-xs", value === c.code ? "text-purple-plasma" : "text-white/80")}>
                      {c.label}
                    </span>
                    <span className="font-mono text-2xs text-white/25">{c.code}</span>
                    {value === c.code && <Check className="h-3 w-3 text-purple-plasma flex-shrink-0" />}
                  </button>
                ))}
              </>
            )}

            {/* Divider */}
            {!search && filteredOthers.length > 0 && (
              <>
                <div className="mx-3 my-1 border-t border-glass-border" />
                <p className="px-3 pt-1 pb-1 text-2xs font-mono text-white/25 uppercase tracking-wider">
                  All Countries
                </p>
              </>
            )}

            {/* Others */}
            {filteredOthers.map((c) => (
              <button key={c.code} onClick={() => pick(c.code)}
                className={cn(
                  "w-full flex items-center gap-3 px-3 py-2 text-left text-sm transition-colors hover:bg-glass-200",
                  value === c.code && "bg-purple-plasma/10"
                )}>
                <span className="text-base leading-none w-6 text-center">{c.flag}</span>
                <span className={cn("flex-1 font-mono text-xs", value === c.code ? "text-purple-plasma" : "text-white/80")}>
                  {c.label}
                </span>
                <span className="font-mono text-2xs text-white/25">{c.code}</span>
                {value === c.code && <Check className="h-3 w-3 text-purple-plasma flex-shrink-0" />}
              </button>
            ))}

            {filteredPopular.length === 0 && filteredOthers.length === 0 && (
              <p className="px-4 py-6 text-center text-xs text-white/25 font-mono">No countries found</p>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

// ── BulkUploadModal ───────────────────────────────────────────────────────────

type BulkRow = { row: number; tracking?: string; status: "ok" | "error"; message?: string };

function BulkUploadModal({ onClose, onDone }: { onClose: () => void; onDone?: () => void }) {
  const fileRef = useRef<HTMLInputElement>(null);
  const [file,      setFile]      = useState<File | null>(null);
  const [rows,      setRows]      = useState<BulkRow[]>([]);
  const [uploading, setUploading] = useState(false);
  const [done,      setDone]      = useState(false);

  function handleFile(f: File) {
    setFile(f);
    setRows([]);
    setDone(false);
    const reader = new FileReader();
    reader.onload = (e) => {
      const text = e.target?.result as string;
      const lines = text.split("\n").filter((l) => l.trim());
      // Skip header row
      const parsed: BulkRow[] = lines.slice(1).map((line, i) => {
        const cols = line.split(",");
        if (cols.length < 4) return { row: i + 2, status: "error" as const, message: "Too few columns" };
        return { row: i + 2, tracking: cols[0]?.trim(), status: "ok" as const };
      });
      setRows(parsed);
    };
    reader.readAsText(f);
  }

  async function handleUpload() {
    if (!file || rows.length === 0) return;
    setUploading(true);
    // Simulate upload — wire to POST /v1/shipments/bulk in production
    await new Promise((r) => setTimeout(r, 1400));
    setUploading(false);
    setDone(true);
    onDone?.();
  }

  const okCount  = rows.filter((r) => r.status === "ok").length;
  const errCount = rows.filter((r) => r.status === "error").length;

  return (
    <motion.div
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      className="fixed inset-0 z-50 flex items-center justify-center p-4"
      style={{ background: "rgba(0,0,0,0.75)", backdropFilter: "blur(6px)" }}
      onClick={(e) => e.target === e.currentTarget && onClose()}
    >
      <motion.div
        initial={{ opacity: 0, scale: 0.96, y: 16 }}
        animate={{ opacity: 1, scale: 1, y: 0 }}
        exit={{ opacity: 0, scale: 0.96, y: 8 }}
        transition={{ ease: [0.16, 1, 0.3, 1], duration: 0.3 }}
        className="relative w-full max-w-lg rounded-2xl border border-glass-border p-6 shadow-glass"
        style={{ background: "rgba(8,12,28,0.98)" }}
      >
        {/* Header */}
        <div className="flex items-center justify-between mb-5">
          <div>
            <h2 className="font-heading text-lg font-bold text-white">Bulk Upload CSV</h2>
            <p className="text-xs text-white/35 mt-0.5 font-mono">Upload multiple shipments at once</p>
          </div>
          <button onClick={onClose} className="flex h-8 w-8 items-center justify-center rounded-lg border border-glass-border text-white/40 hover:text-white/80 transition-all">
            <X size={15} />
          </button>
        </div>

        {!done ? (
          <>
            {/* Drop zone */}
            <div
              onClick={() => fileRef.current?.click()}
              onDragOver={(e) => e.preventDefault()}
              onDrop={(e) => { e.preventDefault(); const f = e.dataTransfer.files[0]; if (f) handleFile(f); }}
              className="flex flex-col items-center justify-center gap-3 rounded-xl border-2 border-dashed p-8 cursor-pointer transition-colors hover:border-cyan-neon/40"
              style={{ borderColor: file ? "rgba(0,229,255,0.3)" : "rgba(255,255,255,0.08)" }}
            >
              <div className="flex h-12 w-12 items-center justify-center rounded-xl" style={{ background: "rgba(0,229,255,0.08)" }}>
                <Upload className="h-5 w-5 text-cyan-neon" />
              </div>
              {file ? (
                <div className="text-center">
                  <p className="text-sm font-semibold text-white">{file.name}</p>
                  <p className="text-xs text-white/35 mt-0.5">{(file.size / 1024).toFixed(1)} KB · {rows.length} rows detected</p>
                </div>
              ) : (
                <div className="text-center">
                  <p className="text-sm font-medium text-white/70">Drop CSV here or <span className="text-cyan-neon">browse</span></p>
                  <p className="text-xs text-white/30 mt-1">recipient_name, phone, address, city, weight, cod_amount</p>
                </div>
              )}
              <input ref={fileRef} type="file" accept=".csv" className="hidden" onChange={(e) => { const f = e.target.files?.[0]; if (f) handleFile(f); }} />
            </div>

            {/* Download template */}
            <button className="mt-2 flex items-center gap-1.5 text-xs text-white/35 hover:text-cyan-neon transition-colors">
              <FileText size={12} /> Download CSV template
            </button>

            {/* Preview */}
            {rows.length > 0 && (
              <div className="mt-4 rounded-xl border border-glass-border overflow-hidden">
                <div className="flex items-center justify-between px-3 py-2 border-b border-glass-border bg-glass-100">
                  <span className="text-xs font-mono text-white/40">{rows.length} rows</span>
                  <div className="flex items-center gap-3">
                    {okCount  > 0 && <span className="flex items-center gap-1 text-xs text-green-signal"><CheckCircle2 size={11} />{okCount} valid</span>}
                    {errCount > 0 && <span className="flex items-center gap-1 text-xs text-red-signal"><AlertCircle size={11} />{errCount} errors</span>}
                  </div>
                </div>
                <div className="max-h-36 overflow-y-auto divide-y divide-glass-border">
                  {rows.slice(0, 8).map((r) => (
                    <div key={r.row} className="flex items-center gap-2 px-3 py-1.5">
                      <span className="text-2xs font-mono text-white/25 w-6">{r.row}</span>
                      {r.status === "ok"
                        ? <CheckCircle2 size={11} className="text-green-signal shrink-0" />
                        : <AlertCircle  size={11} className="text-red-signal shrink-0" />}
                      <span className={`text-xs truncate ${r.status === "ok" ? "text-white/60" : "text-red-signal"}`}>
                        {r.status === "ok" ? (r.tracking ?? `Row ${r.row}`) : r.message}
                      </span>
                    </div>
                  ))}
                  {rows.length > 8 && (
                    <div className="px-3 py-1.5 text-2xs text-white/25 font-mono">+{rows.length - 8} more rows…</div>
                  )}
                </div>
              </div>
            )}

            {/* Footer */}
            <div className="mt-5 flex justify-end gap-2">
              <button onClick={onClose} className="rounded-lg border border-glass-border px-4 py-2 text-sm text-white/50 hover:text-white transition-colors">Cancel</button>
              <button
                onClick={handleUpload}
                disabled={!file || okCount === 0 || uploading}
                className="flex items-center gap-2 rounded-lg px-5 py-2 text-sm font-semibold text-canvas transition-all disabled:opacity-40"
                style={{ background: "linear-gradient(135deg, #00E5FF, #A855F7)" }}
              >
                {uploading ? (
                  <><span className="h-3.5 w-3.5 animate-spin rounded-full border-2 border-canvas/30 border-t-canvas" /> Uploading…</>
                ) : (
                  <><Upload size={14} /> Upload {okCount > 0 ? `${okCount} shipments` : ""}</>
                )}
              </button>
            </div>
          </>
        ) : (
          <div className="flex flex-col items-center gap-4 py-6 text-center">
            <div className="flex h-14 w-14 items-center justify-center rounded-2xl" style={{ background: "rgba(0,255,136,0.1)" }}>
              <CheckCircle2 className="h-7 w-7 text-green-signal" />
            </div>
            <div>
              <p className="font-heading text-lg font-bold text-white">Upload Complete</p>
              <p className="text-sm text-white/40 mt-1">{okCount} shipments created successfully.</p>
            </div>
            <button
              onClick={onClose}
              className="rounded-lg px-6 py-2 text-sm font-semibold text-canvas"
              style={{ background: "linear-gradient(135deg, #00E5FF, #A855F7)" }}
            >
              Done
            </button>
          </div>
        )}
      </motion.div>
    </motion.div>
  );
}

// ── Delivery receipt modal ────────────────────────────────────────────────────

function DeliveryReceiptModal({ shipment, onClose }: { shipment: Shipment; onClose: () => void }) {
  const [deliveredAt, setDeliveredAt] = useState<string | null>(null);
  const [loading,     setLoading]     = useState(true);
  const [copied,      setCopied]      = useState(false);

  useEffect(() => {
    authFetch(`${DELIVERY_EXPERIENCE_URL}/v1/tracking/${shipment.id}`)
      .then(r => r.ok ? r.json() : null)
      .then(json => {
        if (json?.delivered_at) setDeliveredAt(json.delivered_at as string);
      })
      .catch(() => {})
      .finally(() => setLoading(false));
  }, [shipment.id]);

  function copyAwb() {
    navigator.clipboard.writeText(shipment.tracking_number).catch(() => {});
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  }

  const deliveryDate = deliveredAt
    ? new Date(deliveredAt).toLocaleString("en-PH", { dateStyle: "medium", timeStyle: "short" })
    : "—";

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/70 backdrop-blur-sm"
      onClick={(e) => e.target === e.currentTarget && onClose()}
    >
      <motion.div
        initial={{ opacity: 0, scale: 0.96, y: 16 }}
        animate={{ opacity: 1, scale: 1, y: 0 }}
        exit={{ opacity: 0, scale: 0.96, y: 16 }}
        transition={{ duration: 0.25, ease: [0.16, 1, 0.3, 1] }}
        className="w-full max-w-md rounded-2xl border border-glass-border bg-canvas shadow-2xl overflow-hidden"
        style={{ boxShadow: "0 0 60px rgba(0,255,136,0.08), 0 32px 64px rgba(0,0,0,0.7)" }}
      >
        {/* Header */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-glass-border bg-green-signal/5">
          <div>
            <h2 className="font-heading text-base font-semibold text-white flex items-center gap-2">
              <CheckCircle2 size={16} className="text-green-signal" />
              Delivery Receipt
            </h2>
            <p className="text-xs text-white/40 font-mono mt-0.5">{shipment.tracking_number}</p>
          </div>
          <button onClick={onClose} className="rounded-lg p-1.5 text-white/40 hover:text-white hover:bg-glass-200 transition-colors">
            <X size={16} />
          </button>
        </div>

        {/* Body */}
        <div className="flex flex-col gap-4 px-6 py-6">
          {/* AWB block */}
          <div className="flex flex-col items-center gap-3 rounded-2xl border border-green-signal/20 bg-glass-100 p-5">
            <div className="flex h-16 w-16 items-center justify-center rounded-xl border border-white/10 bg-white/5">
              <QrCode size={32} className="text-green-signal" />
            </div>
            <p className="font-mono text-lg font-bold text-green-signal tracking-wider">{shipment.tracking_number}</p>
            <button
              onClick={copyAwb}
              className="flex items-center gap-1.5 rounded-lg border border-white/10 bg-white/5 px-4 py-2 text-xs font-mono text-white/60 hover:text-white hover:border-white/20 transition-colors"
            >
              <Copy size={12} />
              {copied ? "Copied!" : "Copy AWB"}
            </button>
          </div>

          {/* Details */}
          <div className="rounded-xl border border-glass-border bg-glass-100 divide-y divide-glass-border">
            {[
              { label: "Recipient",    value: shipment.recipient_name },
              { label: "Destination",  value: shipment.destination    },
              { label: "Status",       value: "Delivered",   highlight: "text-green-signal" },
              { label: "Delivered At", value: loading ? "Loading…" : deliveryDate },
              ...(shipment.cod_amount ? [{ label: "COD Collected", value: `₱${shipment.cod_amount.toLocaleString()}`, highlight: "text-amber-signal" }] : []),
            ].map(({ label, value, highlight }) => (
              <div key={label} className="flex items-center justify-between px-4 py-3">
                <span className="text-2xs font-mono text-white/35 uppercase tracking-wider">{label}</span>
                <span className={`text-xs font-mono ${highlight ?? "text-white/80"}`}>{value}</span>
              </div>
            ))}
          </div>

          <p className="text-2xs font-mono text-center text-white/25">
            POD captured by driver · verified via LogisticOS
          </p>

          <button
            onClick={onClose}
            className="w-full rounded-xl py-2.5 text-sm font-semibold text-canvas"
            style={{ background: "linear-gradient(135deg, #00FF88, #00E5FF)" }}
          >
            Close
          </button>
        </div>
      </motion.div>
    </div>
  );
}

// ── Booking receipt types ─────────────────────────────────────────────────────

interface BookedResult {
  awb:          string;
  serviceType:  string;
  isIntl:       boolean;
  senderCity:   string;
  receiverCity: string;
  receiverName: string;
  fee:          number;
}

// ── Booking receipt view ──────────────────────────────────────────────────────

function BookingReceiptView({
  result,
  onDone,
}: {
  result:  BookedResult;
  onDone:  () => void;
}) {
  const [copied, setCopied] = useState(false);

  function copyAwb() {
    navigator.clipboard.writeText(result.awb).catch(() => {});
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  }

  function share() {
    const url = `${window.location.origin}/track?awb=${result.awb}`;
    if (navigator.share) {
      navigator.share({ title: "Track your shipment", url }).catch(() => {});
    } else {
      navigator.clipboard.writeText(url).catch(() => {});
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    }
  }

  const serviceLabel = result.isIntl ? "Balikbayan / International" : "Standard Local";

  return (
    <div className="flex flex-col items-center gap-5 px-6 py-8 text-center">
      {/* Success icon */}
      <div className="flex h-16 w-16 items-center justify-center rounded-full border border-green-signal/30 bg-green-signal/10">
        <CheckCircle2 size={32} className="text-green-signal" />
      </div>

      <div>
        <h2 className="font-heading text-xl font-bold text-white">Booking Confirmed!</h2>
        <p className="mt-1 text-sm text-white/40 font-mono">
          {result.senderCity} → {result.receiverCity}
        </p>
      </div>

      {/* AWB display */}
      <div className="w-full rounded-2xl border border-cyan-signal/20 bg-glass-100 p-5 flex flex-col items-center gap-4">
        <div className="flex h-20 w-20 items-center justify-center rounded-xl border border-white/10 bg-white/5">
          <QrCode size={40} className="text-cyan-signal" />
        </div>
        <div>
          <p className="text-2xs font-mono text-white/30 uppercase tracking-widest mb-1">Tracking Number / AWB</p>
          <p className="font-mono text-lg font-bold text-cyan-signal tracking-wider">{result.awb}</p>
        </div>
        <button
          onClick={copyAwb}
          className="flex items-center gap-1.5 rounded-lg border border-white/10 bg-white/5 px-4 py-2 text-xs font-mono text-white/60 hover:text-white hover:border-white/20 transition-colors"
        >
          <Copy size={12} />
          {copied ? "Copied!" : "Copy AWB"}
        </button>
      </div>

      {/* Summary rows */}
      <div className="w-full rounded-xl border border-glass-border bg-glass-100 divide-y divide-glass-border text-left">
        {[
          { label: "Service",   value: serviceLabel },
          { label: "Recipient", value: result.receiverName },
          { label: "Est. Fee",  value: `₱${result.fee.toLocaleString()}`, highlight: "text-amber-signal" },
        ].map(({ label, value, highlight }) => (
          <div key={label} className="flex items-center justify-between px-4 py-3">
            <span className="text-2xs font-mono text-white/35 uppercase tracking-wider">{label}</span>
            <span className={`text-xs font-mono ${highlight ?? "text-white/80"}`}>{value}</span>
          </div>
        ))}
      </div>

      {/* Actions */}
      <div className="flex w-full gap-3">
        <button
          onClick={share}
          className="flex flex-1 items-center justify-center gap-2 rounded-xl border border-glass-border bg-glass-100 py-2.5 text-sm text-white/60 hover:text-white transition-colors"
        >
          <Share2 size={14} />
          Share Tracking
        </button>
        <button
          onClick={onDone}
          className="flex-1 rounded-xl py-2.5 text-sm font-semibold text-canvas"
          style={{ background: "linear-gradient(135deg, #00E5FF, #A855F7)" }}
        >
          Done
        </button>
      </div>

      <p className="text-2xs font-mono text-white/20">
        Your driver will be assigned shortly. Check the shipments list for live status.
      </p>
    </div>
  );
}

/**
 * NewShipmentModal — simplified booking form.
 *
 * - No manual Local/International toggle.
 * - Sender (Step 1) and Receiver (Step 2) each have: Name, Phone, Address, City, ZIP, Country.
 * - isIntl = senderCountry !== receiverCountry (auto-derived).
 * - Step 3: Package (weight, description, COD for local; box dims + declared value + freight for intl).
 * - Step 4 (intl only): freight mode selection surfaced here for clarity on web.
 * - Step 5 (local=4, intl=5): Review & Confirm.
 */
function NewShipmentModal({ onClose, onBooked }: { onClose: () => void; onBooked?: () => void }) {
  const [step, setStep] = useState(1);

  // Step 1 — Sender
  const [senderName,    setSenderName]    = useState("");
  const [senderPhone,   setSenderPhone]   = useState("");
  const [senderAddress, setSenderAddress] = useState("");
  const [senderCity,    setSenderCity]    = useState("");
  const [senderZip,     setSenderZip]     = useState("");
  const [senderCountry, setSenderCountry] = useState("PH");

  // Step 2 — Receiver
  const [receiverName,    setReceiverName]    = useState("");
  const [receiverPhone,   setReceiverPhone]   = useState("");
  const [receiverAddress, setReceiverAddress] = useState("");
  const [receiverCity,    setReceiverCity]    = useState("");
  const [receiverZip,     setReceiverZip]     = useState("");
  const [receiverCountry, setReceiverCountry] = useState("PH");

  // Step 3 — Package (shared)
  const [weight,        setWeight]        = useState("");
  const [description,   setDescription]   = useState("");
  const [codAmount,     setCodAmount]      = useState("");
  // International extras
  const [boxL, setBoxL] = useState(""); const [boxW, setBoxW] = useState(""); const [boxH, setBoxH] = useState("");
  const [declaredValue, setDeclaredValue] = useState("");
  const [contents,      setContents]      = useState("");
  const [freightMode,   setFreightMode]   = useState<FreightMode>("sea");

  const [booking,      setBooking]      = useState(false);
  const [bookError,    setBookError]    = useState<string | null>(null);
  const [bookedResult, setBookedResult] = useState<BookedResult | null>(null);

  // Auto-detect shipment type from countries
  const isIntl     = senderCountry !== receiverCountry;
  const totalSteps = isIntl ? 4 : 3;
  const reviewStep = totalSteps;

  const senderCountryInfo   = DEST_COUNTRIES.find(c => c.code === senderCountry);
  const receiverCountryInfo = DEST_COUNTRIES.find(c => c.code === receiverCountry);

  function calcTotal() {
    const w = parseFloat(weight || "0");
    const base = isIntl ? 500 : 85;
    const surcharge = w > 1 ? Math.ceil((w - 1) / 0.5) * 10 : 0;
    const airPremium = isIntl && freightMode === "air" ? 800 : 0;
    return base + surcharge + airPremium;
  }

  async function handleBook() {
    setBooking(true); setBookError(null);
    try {
      const weightGrams = Math.round(parseFloat(weight || "0") * 1000);
      const body = {
        customer_name:  receiverName,
        customer_phone: receiverPhone,
        origin: {
          line1: senderAddress, city: senderCity,
          province: senderCity, postal_code: senderZip,
          country_code: senderCountry,
        },
        destination: {
          line1: receiverAddress, city: receiverCity,
          province: receiverCity, postal_code: receiverZip,
          country_code: receiverCountry,
        },
        service_type: isIntl ? "international" : "standard",
        weight_grams: weightGrams > 0 ? weightGrams : 500,
        ...(isIntl && boxL ? { length_cm: parseInt(boxL), width_cm: parseInt(boxW), height_cm: parseInt(boxH) } : {}),
        ...(declaredValue ? { declared_value_cents: Math.round(parseFloat(declaredValue) * 100) } : {}),
        ...(codAmount ? { cod_amount_cents: Math.round(parseFloat(codAmount) * 100) } : {}),
        ...(description || contents ? { special_instructions: description || contents } : {}),
        ...(isIntl ? { freight_mode: freightMode } : {}),
      };
      const res = await authFetch(`${ORDER_INTAKE_URL}/v1/shipments`, {
        method: "POST",
        body: JSON.stringify(body),
      });
      const json = await res.json();
      if (!res.ok) throw new Error(json.error?.message ?? "Booking failed");
      const awb = json.awb ?? json.tracking_number ?? json.data?.awb ?? "";
      setBookedResult({
        awb,
        serviceType:  isIntl ? "international" : "standard",
        isIntl,
        senderCity,
        receiverCity,
        receiverName,
        fee: calcTotal(),
      });
      onBooked?.();
    } catch (err: unknown) {
      setBookError(err instanceof Error ? err.message : "Booking failed");
    } finally {
      setBooking(false);
    }
  }

  const canStep1 = senderName.trim() && senderPhone.trim() && senderAddress.trim() && senderCity.trim() && senderZip.trim();
  const canStep2 = receiverName.trim() && receiverPhone.trim() && receiverAddress.trim() && receiverCity.trim() && receiverZip.trim();
  const canStep3 = isIntl
    ? boxL && boxW && boxH && weight && declaredValue
    : weight.trim();

  const inputCls = (accent: "cyan" | "purple" | "green" = "cyan") => cn(
    "w-full rounded-xl border bg-glass-100 px-4 py-2.5 text-sm text-white placeholder-white/25 font-mono focus:outline-none transition-all",
    accent === "cyan"   ? "border-glass-border focus:border-cyan-signal/40"   :
    accent === "purple" ? "border-glass-border focus:border-purple-plasma/40" :
                          "border-glass-border focus:border-green-signal/40"
  );

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/70 backdrop-blur-sm"
      onClick={(e) => e.target === e.currentTarget && onClose()}
    >
      <motion.div
        initial={{ opacity: 0, scale: 0.96, y: 16 }}
        animate={{ opacity: 1, scale: 1, y: 0 }}
        exit={{ opacity: 0, scale: 0.96, y: 16 }}
        transition={{ duration: 0.25, ease: [0.16, 1, 0.3, 1] }}
        className="w-full max-w-xl rounded-2xl border border-glass-border bg-canvas shadow-2xl overflow-hidden"
        style={{ boxShadow: "0 0 60px rgba(0,229,255,0.08), 0 32px 64px rgba(0,0,0,0.7)" }}
      >
        {bookedResult ? (
          <BookingReceiptView result={bookedResult} onDone={onClose} />
        ) : (
        <>
        {/* Header */}
        <div className={cn(
          "flex items-center justify-between px-6 py-4 border-b border-glass-border",
          isIntl ? "bg-purple-plasma/5" : "bg-cyan-signal/5"
        )}>
          <div>
            <h2 className="font-heading text-base font-semibold text-white">New Shipment</h2>
            <p className="text-xs text-white/40 font-mono mt-0.5">Step {step} of {totalSteps}</p>
          </div>
          <div className="flex items-center gap-3">
            {/* Auto-detected type badge */}
            <div className={cn(
              "hidden sm:flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-xs font-medium",
              isIntl ? "border-purple-plasma/30 bg-purple-plasma/10 text-purple-plasma" : "border-green-signal/30 bg-green-signal/10 text-green-signal"
            )}>
              {isIntl ? <Globe className="h-3 w-3" /> : <Home className="h-3 w-3" />}
              {isIntl
                ? `${senderCountryInfo?.flag ?? "🌐"} → ${receiverCountryInfo?.flag ?? "🌐"} International`
                : "Local Delivery"}
            </div>
            <button onClick={onClose} className="flex h-8 w-8 items-center justify-center rounded-lg border border-glass-border text-white/40 hover:text-white/80 hover:bg-glass-200 transition-all">
              <X className="h-3.5 w-3.5" />
            </button>
          </div>
        </div>

        {/* Step progress */}
        <div className="flex gap-1 px-6 pt-4">
          {Array.from({ length: totalSteps }, (_, i) => i + 1).map((n) => (
            <div key={n} className="h-0.5 flex-1 rounded-full transition-all duration-300"
              style={{ backgroundColor: n < step ? (isIntl ? "#A855F7" : "#00FF88") : n === step ? (isIntl ? "#A855F7" : "#00E5FF") : "rgba(255,255,255,0.08)" }} />
          ))}
        </div>

        <div className="p-6 space-y-4 max-h-[75vh] overflow-y-auto">

          {/* ── Step 1 — Sender ── */}
          {step === 1 && (
            <div className="space-y-3">
              <p className="text-xs font-semibold text-cyan-signal flex items-center gap-1.5">
                <ArrowUpDown className="h-3.5 w-3.5" /> Sender / Pickup
              </p>
              <input value={senderName} onChange={(e) => setSenderName(e.target.value)}
                placeholder="Sender's Full Name *" className={inputCls("cyan")} />
              <input value={senderPhone} onChange={(e) => setSenderPhone(e.target.value)}
                placeholder="Sender's Phone Number *" type="tel" className={inputCls("cyan")} />
              <input value={senderAddress} onChange={(e) => setSenderAddress(e.target.value)}
                placeholder="Street Address *" className={inputCls("cyan")} />
              <div className="grid grid-cols-2 gap-2">
                <input value={senderCity} onChange={(e) => setSenderCity(e.target.value)}
                  placeholder="City *" className={inputCls("cyan")} />
                <input value={senderZip} onChange={(e) => setSenderZip(e.target.value)}
                  placeholder="ZIP Code *" maxLength={10} className={inputCls("cyan")} />
              </div>
              <div>
                <label className="block text-2xs font-mono text-white/40 uppercase tracking-wider mb-1.5">Country</label>
                <CountrySelect value={senderCountry} onChange={setSenderCountry} />
              </div>
            </div>
          )}

          {/* ── Step 2 — Receiver ── */}
          {step === 2 && (
            <div className="space-y-3">
              <p className={cn("text-xs font-semibold flex items-center gap-1.5", isIntl ? "text-purple-plasma" : "text-green-signal")}>
                {isIntl ? <Globe className="h-3.5 w-3.5" /> : <Home className="h-3.5 w-3.5" />}
                Receiver / Delivery
              </p>
              <input value={receiverName} onChange={(e) => setReceiverName(e.target.value)}
                placeholder="Receiver's Full Name *" className={inputCls(isIntl ? "purple" : "green")} />
              <input value={receiverPhone} onChange={(e) => setReceiverPhone(e.target.value)}
                placeholder="Receiver's Phone Number *" type="tel" className={inputCls(isIntl ? "purple" : "green")} />
              <input value={receiverAddress} onChange={(e) => setReceiverAddress(e.target.value)}
                placeholder="Street Address *" className={inputCls(isIntl ? "purple" : "green")} />
              <div className="grid grid-cols-2 gap-2">
                <input value={receiverCity} onChange={(e) => setReceiverCity(e.target.value)}
                  placeholder="City *" className={inputCls(isIntl ? "purple" : "green")} />
                <input value={receiverZip} onChange={(e) => setReceiverZip(e.target.value)}
                  placeholder="ZIP Code *" maxLength={10} className={inputCls(isIntl ? "purple" : "green")} />
              </div>
              <div>
                <label className="block text-2xs font-mono text-white/40 uppercase tracking-wider mb-1.5">Country</label>
                <CountrySelect value={receiverCountry} onChange={setReceiverCountry} />
              </div>
              {/* International auto-detect hint */}
              {isIntl && (
                <div className="rounded-xl border border-purple-plasma/20 bg-purple-plasma/6 px-4 py-3 flex gap-3">
                  <Globe className="h-4 w-4 text-purple-plasma/70 flex-shrink-0 mt-0.5" />
                  <div>
                    <p className="text-xs font-semibold text-purple-plasma">International Shipment Detected</p>
                    <p className="text-xs text-purple-plasma/55 mt-0.5">
                      {senderCountryInfo?.flag} {senderCountryInfo?.label} → {receiverCountryInfo?.flag} {receiverCountryInfo?.label} · AI selects optimal carrier · Customs docs auto-generated
                    </p>
                  </div>
                </div>
              )}
            </div>
          )}

          {/* ── Step 3 — Package (local) ── */}
          {step === 3 && !isIntl && (
            <div className="space-y-3">
              <p className="text-xs font-semibold text-cyan-signal">Package Details</p>
              <div>
                <label className="block text-2xs font-mono text-white/40 uppercase tracking-wider mb-1.5">Weight (kg)</label>
                <input value={weight} onChange={(e) => setWeight(e.target.value)} type="number" min="0" step="0.1" placeholder="e.g. 1.5" className={inputCls("cyan")} />
              </div>
              <div>
                <label className="block text-2xs font-mono text-white/40 uppercase tracking-wider mb-1.5">Package Description</label>
                <input value={description} onChange={(e) => setDescription(e.target.value)} placeholder="e.g. Electronics, Clothes" className={inputCls("cyan")} />
              </div>
              <div>
                <label className="block text-2xs font-mono text-white/40 uppercase tracking-wider mb-1.5">COD Amount (leave blank if prepaid)</label>
                <div className="relative">
                  <span className="absolute left-3 top-1/2 -translate-y-1/2 font-mono text-sm text-white/40">₱</span>
                  <input value={codAmount} onChange={(e) => setCodAmount(e.target.value)} type="number" min="0" placeholder="0.00"
                    className="w-full rounded-xl border border-glass-border bg-glass-100 pl-7 pr-4 py-2.5 text-sm text-amber-400 placeholder-white/20 font-mono focus:outline-none focus:border-amber-400/40 transition-all" />
                </div>
              </div>
            </div>
          )}

          {/* ── Step 3 — Package (international) ── */}
          {step === 3 && isIntl && (
            <div className="space-y-3">
              <p className="text-xs font-semibold text-purple-plasma">Box Details</p>
              <div>
                <label className="block text-2xs font-mono text-white/40 uppercase tracking-wider mb-1.5">Box Dimensions (cm) — L × W × H</label>
                <div className="grid grid-cols-3 gap-2">
                  {([{v:boxL,s:setBoxL,p:"Length"},{v:boxW,s:setBoxW,p:"Width"},{v:boxH,s:setBoxH,p:"Height"}] as const).map(({v,s,p}) => (
                    <input key={p} value={v} onChange={(e) => s(e.target.value)} type="number" min="0" placeholder={p}
                      className="rounded-xl border border-glass-border bg-glass-100 px-3 py-2.5 text-sm text-center text-white placeholder-white/20 font-mono focus:outline-none focus:border-purple-plasma/40 transition-all" />
                  ))}
                </div>
              </div>
              <div>
                <label className="block text-2xs font-mono text-white/40 uppercase tracking-wider mb-1.5">Actual Weight (kg)</label>
                <input value={weight} onChange={(e) => setWeight(e.target.value)} type="number" min="0" step="0.5" placeholder="e.g. 20.5" className={inputCls("purple")} />
              </div>
              <div>
                <label className="block text-2xs font-mono text-white/40 uppercase tracking-wider mb-1.5">Contents</label>
                <input value={contents} onChange={(e) => setContents(e.target.value)} placeholder="e.g. Clothes, canned goods, electronics" className={inputCls("purple")} />
              </div>
              <div>
                <label className="block text-2xs font-mono text-white/40 uppercase tracking-wider mb-1.5">Declared Value (PHP) — for customs</label>
                <div className="relative">
                  <span className="absolute left-3 top-1/2 -translate-y-1/2 font-mono text-sm text-amber-400/60">₱</span>
                  <input value={declaredValue} onChange={(e) => setDeclaredValue(e.target.value)} type="number" min="0" placeholder="e.g. 15000"
                    className="w-full rounded-xl border border-glass-border bg-glass-100 pl-7 pr-4 py-2.5 text-sm text-amber-400 placeholder-white/20 font-mono focus:outline-none focus:border-amber-400/40 transition-all" />
                </div>
              </div>
              <div>
                <label className="block text-2xs font-mono text-white/40 uppercase tracking-wider mb-1.5">Freight Mode</label>
                <div className="grid grid-cols-2 gap-2">
                  {([
                    { val: "sea" as FreightMode, label: "Sea Freight", sub: "30–45 days · Most economical", icon: <Ship className="h-4 w-4" /> },
                    { val: "air" as FreightMode, label: "Air Freight",  sub: "5–10 days · +₱800 premium",   icon: <PlaneTakeoff className="h-4 w-4" /> },
                  ] as const).map((opt) => (
                    <button key={opt.val} onClick={() => setFreightMode(opt.val)}
                      className={cn(
                        "flex items-center gap-3 rounded-xl border px-4 py-3 text-left transition-all",
                        freightMode === opt.val
                          ? "border-cyan-signal/40 bg-cyan-signal/8 text-cyan-signal"
                          : "border-glass-border bg-glass-100 text-white/40 hover:text-white/60"
                      )}>
                      {opt.icon}
                      <div>
                        <p className="text-xs font-semibold">{opt.label}</p>
                        <p className="text-2xs font-mono mt-0.5 opacity-60">{opt.sub}</p>
                      </div>
                      {freightMode === opt.val && <Check className="h-3.5 w-3.5 ml-auto flex-shrink-0" />}
                    </button>
                  ))}
                </div>
              </div>
            </div>
          )}

          {/* ── Review step ── */}
          {step === reviewStep && (
            <div className="space-y-1">
              <div className={cn(
                "inline-flex items-center gap-2 rounded-full border px-3 py-1 text-xs font-semibold mb-3",
                isIntl ? "border-purple-plasma/30 bg-purple-plasma/10 text-purple-plasma" : "border-green-signal/30 bg-green-signal/10 text-green-signal"
              )}>
                {isIntl ? <Globe className="h-3 w-3" /> : <Home className="h-3 w-3" />}
                {isIntl
                  ? `${senderCountryInfo?.flag} → ${receiverCountryInfo?.flag} · ${freightMode === "sea" ? "Sea Freight (30-45d)" : "Air Freight (5-10d)"}`
                  : "Local Delivery"}
              </div>
              {(isIntl ? [
                ["Sender",          `${senderName}`],
                ["Sender Phone",    senderPhone],
                ["Sender Address",  `${senderAddress}, ${senderCity} ${senderZip}, ${senderCountryInfo?.flag} ${senderCountryInfo?.label ?? senderCountry}`],
                ["Receiver",        `${receiverName}`],
                ["Receiver Phone",  receiverPhone],
                ["Receiver Address",`${receiverAddress}, ${receiverCity} ${receiverZip}, ${receiverCountryInfo?.flag} ${receiverCountryInfo?.label ?? receiverCountry}`],
                ["Box Dims",        `${boxL} × ${boxW} × ${boxH} cm`],
                ["Weight",          `${weight} kg`],
                ["Contents",        contents || "—"],
                ["Declared Value",  `₱${declaredValue}`],
                ["Freight",         freightMode === "sea" ? "Sea Freight (30-45 days)" : "Air Freight (5-10 days)"],
              ] : [
                ["Sender",          `${senderName}`],
                ["Sender Phone",    senderPhone],
                ["Sender Address",  `${senderAddress}, ${senderCity} ${senderZip}, ${senderCountryInfo?.flag} ${senderCountryInfo?.label ?? senderCountry}`],
                ["Receiver",        `${receiverName}`],
                ["Receiver Phone",  receiverPhone],
                ["Receiver Address",`${receiverAddress}, ${receiverCity} ${receiverZip}`],
                ["Weight",          `${weight} kg`],
                ["Description",     description || "—"],
                ["COD",             codAmount ? `₱${codAmount}` : "Prepaid"],
              ]).map(([label, value]) => (
                <div key={label} className="flex justify-between py-2 border-b border-glass-border">
                  <span className="text-xs text-white/40 font-mono">{label}</span>
                  <span className="text-xs text-white/80 text-right max-w-[55%]">{value}</span>
                </div>
              ))}
              <div className="flex justify-between pt-3">
                <span className="text-sm font-semibold text-white/60">Est. Total</span>
                <span className={cn("text-xl font-bold font-heading", isIntl ? "text-purple-plasma" : "text-green-signal")}>
                  ₱{calcTotal()}
                </span>
              </div>
            </div>
          )}

          {/* Error + Nav */}
          {bookError && (
            <div className="rounded-lg border border-red-signal/30 bg-red-surface px-3 py-2 text-xs text-red-signal font-mono">
              {bookError}
            </div>
          )}
          <div className="flex gap-3 pt-2">
            {step > 1 && (
              <button onClick={() => setStep(s => s - 1)}
                className="flex-[0.4] rounded-xl border border-glass-border bg-glass-100 py-2.5 text-sm text-white/50 hover:text-white/70 transition-all">
                ← Back
              </button>
            )}
            {step < reviewStep ? (
              <button
                onClick={() => setStep(s => s + 1)}
                disabled={step === 1 ? !canStep1 : step === 2 ? !canStep2 : !canStep3}
                className={cn(
                  "flex-1 rounded-xl py-2.5 text-sm font-semibold text-canvas transition-all",
                  isIntl
                    ? "bg-gradient-to-r from-purple-plasma to-[#6B21D8] hover:opacity-90 disabled:opacity-40"
                    : "bg-gradient-to-r from-cyan-neon to-purple-plasma hover:opacity-90 disabled:opacity-40"
                )}>
                Next →
              </button>
            ) : (
              <button onClick={handleBook} disabled={booking}
                className={cn(
                  "flex-1 rounded-xl py-2.5 text-sm font-semibold text-canvas transition-all disabled:opacity-50",
                  isIntl
                    ? "bg-gradient-to-r from-purple-plasma to-[#6B21D8] hover:opacity-90"
                    : "bg-gradient-to-r from-green-signal to-cyan-neon hover:opacity-90"
                )}>
                {booking ? "Booking…" : isIntl ? "Book Balikbayan Box" : "Confirm Booking"}
              </button>
            )}
          </div>
        </div>
        </>
        )}
      </motion.div>
    </div>
  );
}

// ── Main page ──────────────────────────────────────────────────────────────────

function ShipmentsContent() {
  const searchParams = useSearchParams();
  const router = useRouter();
  // Cross-portal deep-link: /merchant/shipments?awb=<awb> (admin portal escalation path).
  const qpAwb = searchParams.get("awb");
  const [search,      setSearch]      = useState(qpAwb ?? "");
  const [statusFilter,setStatusFilter]= useState<ShipmentStatus | "all">("all");
  const [selected,    setSelected]    = useState<Set<string>>(new Set());
  const [showNewShipment,  setShowNewShipment]  = useState(false);
  const [showBulkUpload,   setShowBulkUpload]   = useState(false);
  const [receiptShipment,  setReceiptShipment]  = useState<Shipment | null>(null);
  const [shipments,   setShipments]   = useState<Shipment[]>([]);
  const [loadError,   setLoadError]   = useState<string | null>(null);
  const [isLoading,   setIsLoading]   = useState(true);
  const [page,        setPage]        = useState(1);
  const [bulkMsg,     setBulkMsg]     = useState<string | null>(null);
  const PAGE_SIZE = 20;

  // Auto-open modals from dashboard CTAs
  useEffect(() => {
    if (searchParams.get("new") === "1") {
      setShowNewShipment(true);
      router.replace("/shipments");
    }
    if (searchParams.get("bulk") === "1") {
      setShowBulkUpload(true);
      router.replace("/shipments");
    }
  }, [searchParams, router]);

  const fetchShipments = useCallback(async () => {
    setIsLoading(true);
    try {
      // Batch: shipments + dispatch queue (for assigned_driver_id) + drivers (for name/phone).
      const [shipmentsRes, queueRes, driversRes] = await Promise.allSettled([
        authFetch(`${ORDER_INTAKE_URL}/v1/shipments`),
        authFetch(`${ORDER_INTAKE_URL}/v1/queue?status=all`),
        authFetch(`${ORDER_INTAKE_URL}/v1/drivers`),
      ]);

      if (shipmentsRes.status === "rejected" || !shipmentsRes.value.ok) {
        const body = shipmentsRes.status === "fulfilled"
          ? await shipmentsRes.value.text().catch(() => "")
          : String(shipmentsRes.reason);
        const code = shipmentsRes.status === "fulfilled" ? shipmentsRes.value.status : 0;
        setLoadError(`Failed to load shipments (HTTP ${code})${body ? `: ${body.slice(0, 200)}` : ""}`);
        setShipments([]);
        return;
      }

      const json = await shipmentsRes.value.json();
      const rows: Shipment[] = (json.shipments ?? []).map((s: {
        id: string;
        awb?: string;
        tracking_number?: string;
        customer_name: string;
        destination?: { city?: string };
        status: string;
        cod_amount?: { amount?: number } | null;
        cod_amount_cents?: number | null;
        created_at: string;
      }) => ({
        id:              s.id,
        tracking_number: s.awb ?? s.tracking_number ?? "",
        recipient_name:  s.customer_name,
        destination:     s.destination?.city ?? "",
        status:          s.status as ShipmentStatus,
        cod_amount:      s.cod_amount?.amount ? s.cod_amount.amount / 100 : s.cod_amount_cents ? s.cod_amount_cents / 100 : undefined,
        created_at:      s.created_at,
      }));

      // Build shipment_id → assigned_driver_id map from the dispatch queue.
      const shipmentToDriver = new Map<string, string>();
      if (queueRes.status === "fulfilled" && queueRes.value.ok) {
        const qj = await queueRes.value.json().catch(() => ({ data: [] }));
        for (const q of (qj.data ?? [])) {
          if (q.shipment_id && q.assigned_driver_id) {
            shipmentToDriver.set(q.shipment_id, q.assigned_driver_id);
          }
        }
      }

      // Build driver_id → { name, phone } map from the driver list.
      const driverInfo = new Map<string, { name: string; phone: string }>();
      if (driversRes.status === "fulfilled" && driversRes.value.ok) {
        const dj = await driversRes.value.json().catch(() => ({ data: [] }));
        for (const d of (dj.data ?? [])) {
          const name = `${d.first_name ?? ""} ${d.last_name ?? ""}`.trim() || "—";
          driverInfo.set(d.id, { name, phone: d.phone ?? "" });
        }
      }

      // Enrich rows with driver name + phone for statuses that have an active driver.
      const DRIVER_VISIBLE: ReadonlySet<string> = new Set([
        "pickup_assigned", "picked_up", "in_transit", "out_for_delivery", "delivery_attempted",
      ]);
      const enriched = rows.map((s) => {
        if (!DRIVER_VISIBLE.has(s.status)) return s;
        const driverId = shipmentToDriver.get(s.id);
        if (!driverId) return s;
        const info = driverInfo.get(driverId);
        if (!info) return s;
        return { ...s, driver_name: info.name, driver_phone: info.phone };
      });

      setShipments(enriched);
      setLoadError(null);
    } catch (e) {
      setLoadError(e instanceof Error ? e.message : "Network error loading shipments");
      setShipments([]);
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => { fetchShipments(); }, [fetchShipments]);

  const filtered = shipments.filter((s) => {
    const matchStatus = statusFilter === "all" || s.status === statusFilter;
    const matchSearch = !search ||
      s.tracking_number.toLowerCase().includes(search.toLowerCase()) ||
      s.recipient_name.toLowerCase().includes(search.toLowerCase()) ||
      s.destination.toLowerCase().includes(search.toLowerCase());
    return matchStatus && matchSearch;
  });

  const totalPages = Math.max(1, Math.ceil(filtered.length / PAGE_SIZE));
  const paginated  = filtered.slice((page - 1) * PAGE_SIZE, page * PAGE_SIZE);

  // Reset to page 1 whenever the filter or search changes
  useEffect(() => { setPage(1); }, [statusFilter, search]);

  function handleExport() {
    const rows = filtered.map(s =>
      [s.tracking_number, s.recipient_name, s.destination, s.status,
       s.cod_amount ? `PHP ${s.cod_amount}` : "", s.eta ?? "", s.created_at].join(",")
    );
    const csv = ["Tracking #,Recipient,Destination,Status,COD,ETA,Created At", ...rows].join("\n");
    const blob = new Blob([csv], { type: "text/csv" });
    const url  = URL.createObjectURL(blob);
    const a    = document.createElement("a");
    a.href = url; a.download = `shipments-${new Date().toISOString().slice(0, 10)}.csv`;
    a.click(); URL.revokeObjectURL(url);
  }

  function handleBulkReschedule() {
    setBulkMsg(`${selected.size} shipment${selected.size > 1 ? "s" : ""} queued for rescheduling`);
    setSelected(new Set());
    setTimeout(() => setBulkMsg(null), 3000);
  }

  function handleBulkCancel() {
    setBulkMsg(`${selected.size} shipment${selected.size > 1 ? "s" : ""} cancellation requested`);
    setSelected(new Set());
    setTimeout(() => setBulkMsg(null), 3000);
  }

  function toggleSelect(id: string) {
    setSelected((prev) => {
      const next = new Set(prev);
      next.has(id) ? next.delete(id) : next.add(id);
      return next;
    });
  }

  function toggleAll() {
    setSelected(selected.size === filtered.length ? new Set() : new Set(filtered.map((s) => s.id)));
  }

  return (
    <>
    <motion.div
      variants={variants.staggerContainer}
      initial="hidden"
      animate="visible"
      className="flex flex-col gap-5 p-6"
    >
      {/* Deep-link banner (from ops/admin) */}
      {qpAwb && (
        <motion.div variants={variants.fadeInUp}>
          <div className="flex items-center justify-between rounded-xl border border-cyan-neon/30 bg-cyan-surface px-4 py-2.5">
            <div className="flex items-center gap-2.5">
              <ExternalLink size={14} className="text-cyan-neon" />
              <p className="text-xs text-white/80">
                Linked from ops · <span className="font-mono text-cyan-neon">{qpAwb}</span>
              </p>
            </div>
            <button
              onClick={() => { setSearch(""); router.replace("/shipments"); }}
              className="text-white/40 hover:text-white"
              aria-label="Clear deep-link filter"
            >
              <X size={14} />
            </button>
          </div>
        </motion.div>
      )}

      {/* Header */}
      <motion.div variants={variants.fadeInUp} className="flex items-center justify-between">
        <div>
          <h1 className="font-heading text-2xl font-bold text-white">Shipments</h1>
          <p className="text-sm text-white/40 font-mono mt-0.5">
            {shipments.length.toLocaleString()} {shipments.length === 1 ? "shipment" : "shipments"}
          </p>
        </div>
        <div className="flex items-center gap-2">
          <button onClick={handleExport} className="flex items-center gap-1.5 rounded-lg border border-glass-border bg-glass-100 px-3 py-2 text-xs text-white/60 hover:text-white transition-colors">
            <Download size={13} /> Export
          </button>
          <button
            onClick={() => setShowBulkUpload(true)}
            className="flex items-center gap-1.5 rounded-lg border px-3 py-2 text-xs font-medium transition-all hover:scale-[1.02]"
            style={{ borderColor: "rgba(168,85,247,0.25)", background: "rgba(168,85,247,0.07)", color: "#A855F7" }}
          >
            <Upload size={13} /> Bulk Upload
          </button>
          <button
            onClick={() => setShowNewShipment(true)}
            className="flex items-center gap-1.5 rounded-lg bg-gradient-to-r from-cyan-neon to-purple-plasma px-4 py-2 text-xs font-semibold text-canvas hover:opacity-90 transition-opacity"
          >
            <Plus size={13} /> New Shipment
          </button>
        </div>
      </motion.div>

      {/* Summary stats */}
      <motion.div variants={variants.fadeInUp} className="grid grid-cols-4 gap-3">
        {computeStats(shipments).map((s) => (
          <GlassCard key={s.label} size="sm" glow={s.color}>
            <p className="text-2xs font-mono text-white/40 uppercase tracking-wider">{s.label}</p>
            <p className={`font-heading text-2xl font-bold mt-1 ${
              s.color === "cyan" ? "text-cyan-neon" :
              s.color === "purple" ? "text-purple-plasma" :
              s.color === "green" ? "text-green-signal" : "text-red-signal"
            }`}>{s.value.toLocaleString()}</p>
          </GlassCard>
        ))}
      </motion.div>

      {/* Filters + search */}
      <motion.div variants={variants.fadeInUp}>
        <GlassCard>
          <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
            {/* Status filter pills */}
            <div className="flex items-center gap-1.5 flex-wrap">
              {STATUS_FILTERS.map((f) => (
                <button
                  key={f.value}
                  onClick={() => setStatusFilter(f.value)}
                  className={`rounded-full px-3 py-1 text-xs font-medium transition-all ${
                    statusFilter === f.value
                      ? "bg-cyan-surface text-cyan-neon border border-cyan-neon/30"
                      : "text-white/40 border border-glass-border hover:text-white"
                  }`}
                >
                  {f.label}
                </button>
              ))}
            </div>
            {/* Search */}
            <div className="flex items-center gap-2 rounded-lg border border-glass-border bg-glass-100 px-3 py-2 min-w-[220px]">
              <Search size={13} className="text-white/30 flex-shrink-0" />
              <input
                value={search}
                onChange={(e) => setSearch(e.target.value)}
                placeholder="Search AWB, recipient…"
                className="flex-1 bg-transparent text-xs text-white placeholder:text-white/25 outline-none font-mono"
              />
            </div>
          </div>
        </GlassCard>
      </motion.div>

      {/* Table */}
      <motion.div variants={variants.fadeInUp}>
        <GlassCard padding="none">
          {/* Bulk action bar */}
          {selected.size > 0 && (
            <div className="flex items-center gap-3 border-b border-glass-border px-4 py-2.5 bg-purple-surface">
              <span className="text-xs font-mono text-white/60">{selected.size} selected</span>
              <button onClick={handleBulkReschedule} className="text-xs text-cyan-neon hover:underline">Reschedule</button>
              <button onClick={handleBulkCancel} className="text-xs text-amber-signal hover:underline">Cancel</button>
              <button className="text-xs text-white/40 hover:underline ml-auto" onClick={() => setSelected(new Set())}>Clear</button>
            </div>
          )}

          {/* Table header */}
          <div className="grid grid-cols-[24px_1fr_1fr_1fr_100px_80px_110px] gap-3 px-4 py-3 border-b border-glass-border">
            <input
              type="checkbox"
              checked={selected.size === filtered.length && filtered.length > 0}
              onChange={toggleAll}
              className="accent-cyan-neon"
            />
            {["Tracking #", "Recipient", "Destination", "Status", "COD", "ETA / Action"].map((h) => (
              <button key={h} className="flex items-center gap-1 text-2xs font-mono text-white/30 uppercase tracking-wider hover:text-white/60 transition-colors text-left">
                {h} <ArrowUpDown size={10} />
              </button>
            ))}
          </div>

          {/* Load error */}
          {loadError && (
            <div className="mx-4 my-3 rounded-lg border border-red-500/40 bg-red-500/10 px-3 py-2 text-xs text-red-300 flex items-start gap-2">
              <AlertCircle size={14} className="mt-0.5 flex-shrink-0" />
              <span className="font-mono">{loadError}</span>
            </div>
          )}

          {/* Loading state */}
          {isLoading && !loadError && shipments.length === 0 && (
            <div className="px-4 py-8 text-center text-xs font-mono text-white/40">
              Loading shipments…
            </div>
          )}

          {/* Empty state */}
          {!isLoading && !loadError && filtered.length === 0 && (
            <div className="px-4 py-8 text-center text-xs font-mono text-white/40">
              {shipments.length === 0
                ? "No shipments yet. Customer bookings and merchant shipments will appear here."
                : "No shipments match the current filter."}
            </div>
          )}

          {/* Bulk feedback toast */}
          {bulkMsg && (
            <div className="mx-4 my-2 rounded-lg border border-green-signal/30 bg-green-signal/10 px-3 py-2 text-xs text-green-signal font-mono flex items-center gap-2">
              <CheckCircle2 size={13} /> {bulkMsg}
            </div>
          )}

          {/* Rows */}
          {paginated.map((shipment) => {
            const { label, variant } = STATUS_MAP[shipment.status] ?? { label: shipment.status, variant: "cyan" as BadgeVariant };
            const isDeepLinked = qpAwb && shipment.tracking_number === qpAwb;
            return (
              <div
                key={shipment.id}
                className={`grid grid-cols-[24px_1fr_1fr_1fr_100px_80px_110px] gap-3 px-4 py-3.5 border-b border-glass-border/50 hover:bg-glass-100 transition-colors cursor-pointer ${
                  isDeepLinked ? "bg-cyan-neon/10 ring-1 ring-inset ring-cyan-neon/30" :
                  selected.has(shipment.id) ? "bg-cyan-surface/30" : ""
                }`}
                onClick={() => toggleSelect(shipment.id)}
              >
                <input
                  type="checkbox"
                  checked={selected.has(shipment.id)}
                  onChange={() => toggleSelect(shipment.id)}
                  onClick={(e) => e.stopPropagation()}
                  className="accent-cyan-neon"
                />
                <span className="font-mono text-xs text-cyan-neon">{shipment.tracking_number}</span>
                <div className="flex flex-col gap-0.5 min-w-0">
                  <span className="text-xs text-white truncate">{shipment.recipient_name}</span>
                  {shipment.driver_name && (
                    <span className="flex items-center gap-1 text-2xs font-mono text-cyan-neon/60 truncate" title={`Driver: ${shipment.driver_name}${shipment.driver_phone ? ` · ${shipment.driver_phone}` : ""}`}>
                      <User size={8} className="shrink-0" />
                      {shipment.driver_name}
                      {shipment.driver_phone && (
                        <><Phone size={8} className="shrink-0 ml-0.5" />{shipment.driver_phone}</>
                      )}
                    </span>
                  )}
                </div>
                <span className="text-xs text-white/60 truncate">{shipment.destination}</span>
                <NeonBadge variant={variant}>{label}</NeonBadge>
                <span className="text-xs text-white/60 font-mono">
                  {shipment.cod_amount ? `₱${shipment.cod_amount.toLocaleString()}` : "—"}
                </span>
                {shipment.status === "delivered" ? (
                  <button
                    onClick={(e) => { e.stopPropagation(); setReceiptShipment(shipment); }}
                    className="flex items-center gap-1 rounded-md border border-green-signal/30 bg-green-signal/5 px-2 py-1 text-2xs font-mono text-green-signal hover:bg-green-signal/10 hover:border-green-signal/50 transition-colors"
                  >
                    <FileText size={10} /> Receipt
                  </button>
                ) : (
                  <span className="text-xs text-white/40 font-mono">{shipment.eta ?? "—"}</span>
                )}
              </div>
            );
          })}

          {/* Pagination */}
          {totalPages > 1 && (
            <div className="flex items-center justify-between px-4 py-3">
              <span className="text-2xs font-mono text-white/30">
                {(page - 1) * PAGE_SIZE + 1}–{Math.min(page * PAGE_SIZE, filtered.length)} of {filtered.length}
              </span>
              <div className="flex items-center gap-1">
                <button
                  onClick={() => setPage(p => Math.max(1, p - 1))}
                  disabled={page === 1}
                  className="rounded p-1 text-white/30 hover:text-white hover:bg-glass-200 transition-colors disabled:opacity-30"
                ><ChevronLeft size={14} /></button>
                {Array.from({ length: totalPages }, (_, i) => i + 1)
                  .filter(p => p === 1 || p === totalPages || Math.abs(p - page) <= 1)
                  .reduce<(number | "…")[]>((acc, p, i, arr) => {
                    if (i > 0 && (p as number) - (arr[i - 1] as number) > 1) acc.push("…");
                    acc.push(p);
                    return acc;
                  }, [])
                  .map((p, i) => p === "…"
                    ? <span key={`e${i}`} className="px-1.5 text-xs text-white/20">…</span>
                    : <button key={p} onClick={() => setPage(p as number)} className={`rounded px-2.5 py-1 text-xs transition-colors ${
                        p === page ? "bg-cyan-surface text-cyan-neon" : "text-white/40 hover:text-white hover:bg-glass-200"
                      }`}>{p}</button>
                  )}
                <button
                  onClick={() => setPage(p => Math.min(totalPages, p + 1))}
                  disabled={page === totalPages}
                  className="rounded p-1 text-white/30 hover:text-white hover:bg-glass-200 transition-colors disabled:opacity-30"
                ><ChevronRight size={14} /></button>
              </div>
            </div>
          )}
        </GlassCard>
      </motion.div>
    </motion.div>

    {/* New Shipment Modal */}
    <AnimatePresence>
      {showNewShipment && (
        <NewShipmentModal onClose={() => setShowNewShipment(false)} onBooked={fetchShipments} />
      )}
    </AnimatePresence>

    {/* Bulk Upload Modal */}
    <AnimatePresence>
      {showBulkUpload && (
        <BulkUploadModal onClose={() => setShowBulkUpload(false)} onDone={fetchShipments} />
      )}
    </AnimatePresence>

    {/* Delivery Receipt Modal */}
    <AnimatePresence>
      {receiptShipment && (
        <DeliveryReceiptModal shipment={receiptShipment} onClose={() => setReceiptShipment(null)} />
      )}
    </AnimatePresence>
    </>
  );
}

export default function ShipmentsPage() {
  return (
    <Suspense>
      <ShipmentsContent />
    </Suspense>
  );
}
