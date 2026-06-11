"use client";
/**
 * Admin Portal — Cross-Border Hub Transfer
 *
 * LIVE backend (hub-ops service via gateway):
 *  - Container Board  : GET /v1/containers (+ lifecycle transition actions + POST create)
 *  - Customs Queue    : GET /v1/hubs/:id/customs-queue, POST .../clear-customs
 *  - Routing Config   : GET/POST /v1/hub-transfer/routing-config
 *  - Inventory Map    : GET /v1/hub-transfer/locations, /v1/hub-transfer/inventory
 *
 * Hub scope is chosen from the hub selector (GET /v1/hubs). All actions are
 * tenant-scoped server-side via the JWT; this page never sends tenant_id.
 */
import { useCallback, useEffect, useMemo, useState } from "react";
import { AnimatePresence, motion } from "framer-motion";
import { toast } from "sonner";
import { variants } from "@/lib/design-system/tokens";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import {
  Container, ShieldCheck, Boxes, Route, Loader2, ChevronRight,
  PackageCheck, MapPin, AlertTriangle, ArrowRight, Plus, X,
} from "lucide-react";
import { createHubsApi, hubIdOf, type Hub } from "@/lib/api/hubs";
import {
  createHubTransferApi, CONTAINER_BOARD_COLUMNS, TRANSPORT_MODES,
  customsTone, transportModeLabel, isCustomsMode,
  type ContainerSummary, type TransferManifest, type RoutingConfig,
  type HubLocation, type HubInventory, type RoutingType,
} from "@/lib/api/hub-transfer";
import { cn } from "@/lib/utils";

type Tab = "board" | "customs" | "routing" | "inventory";

const TABS: { key: Tab; label: string; icon: typeof Container }[] = [
  { key: "board",     label: "Container Board", icon: Container },
  { key: "customs",   label: "Customs Queue",   icon: ShieldCheck },
  { key: "routing",   label: "Routing Config",  icon: Route },
  { key: "inventory", label: "Inventory Map",   icon: Boxes },
];

const STATUS_TONE: Record<string, "green" | "amber" | "red" | "muted" | "cyan"> = {
  released: "green", deconsolidated: "green", delivered: "green",
  customs: "amber", arrived_at_port: "cyan", in_transit: "cyan",
  manifested: "muted",
};

function shortId(id: string) { return id.slice(0, 8); }

// ── Create Container Modal ────────────────────────────────────────────────────
function CreateContainerModal({
  hubs, onClose, onCreate,
}: {
  hubs: Hub[];
  onClose: () => void;
  onCreate: () => void;
}) {
  const api = useMemo(() => createHubTransferApi(), []);
  const [form, setForm] = useState({
    transport_mode: "sea",
    origin_hub_id: "",
    destination_hub_id: "",
    carrier_ref: "",
  });
  const [saving, setSaving] = useState(false);

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    if (!form.origin_hub_id || !form.destination_hub_id) {
      toast.error("Origin and destination hubs are required");
      return;
    }
    setSaving(true);
    try {
      await api.createContainer({
        transport_mode:     form.transport_mode,
        origin_hub_id:      form.origin_hub_id,
        destination_hub_id: form.destination_hub_id,
        carrier_ref:        form.carrier_ref || undefined,
      });
      toast.success("Container created");
      onCreate();
    } catch (e) {
      toast.error((e as { message?: string })?.message ?? "Failed to create container");
    } finally {
      setSaving(false);
    }
  }

  return (
    <motion.div initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }}
      className="fixed inset-0 z-50 flex items-center justify-center p-4"
      onClick={onClose}>
      <div className="absolute inset-0 bg-black/60 backdrop-blur-sm" />
      <motion.div initial={{ opacity: 0, y: 14, scale: 0.97 }} animate={{ opacity: 1, y: 0, scale: 1 }}
        exit={{ opacity: 0, y: 14, scale: 0.97 }}
        transition={{ type: "spring", damping: 28, stiffness: 260 }}
        className="relative w-full max-w-md rounded-2xl border border-glass-border bg-[#0d1422] shadow-2xl overflow-hidden"
        onClick={(e) => e.stopPropagation()}>
        <div className="flex items-center justify-between border-b border-glass-border px-5 py-4">
          <div className="flex items-center gap-2">
            <Container size={15} className="text-cyan-neon" />
            <h2 className="font-heading text-sm font-bold text-white">New Container</h2>
          </div>
          <button onClick={onClose} className="rounded-lg p-1.5 text-white/40 transition-colors hover:bg-glass-200 hover:text-white">
            <X size={14} />
          </button>
        </div>
        <form onSubmit={submit} className="space-y-4 p-5">
          <Field label="Transport Mode">
            <select value={form.transport_mode} onChange={(e) => setForm((f) => ({ ...f, transport_mode: e.target.value }))}
              className="w-full rounded-xl border border-white/10 bg-white/5 px-3 py-2.5 text-sm text-white focus:border-cyan-500 focus:outline-none">
              {TRANSPORT_MODES.map((m) => <option key={m.value} value={m.value}>{m.label}</option>)}
            </select>
          </Field>
          <Field label="Origin Hub *">
            <select value={form.origin_hub_id} onChange={(e) => setForm((f) => ({ ...f, origin_hub_id: e.target.value }))}
              className="w-full rounded-xl border border-white/10 bg-white/5 px-3 py-2.5 text-sm text-white focus:border-cyan-500 focus:outline-none">
              <option value="">— select hub —</option>
              {hubs.map((h) => <option key={hubIdOf(h)} value={hubIdOf(h)}>{h.name}</option>)}
            </select>
          </Field>
          <Field label="Destination Hub *">
            <select value={form.destination_hub_id} onChange={(e) => setForm((f) => ({ ...f, destination_hub_id: e.target.value }))}
              className="w-full rounded-xl border border-white/10 bg-white/5 px-3 py-2.5 text-sm text-white focus:border-cyan-500 focus:outline-none">
              <option value="">— select hub —</option>
              {hubs.map((h) => <option key={hubIdOf(h)} value={hubIdOf(h)}>{h.name}</option>)}
            </select>
          </Field>
          <Field label="Carrier Ref (optional)">
            <input value={form.carrier_ref} onChange={(e) => setForm((f) => ({ ...f, carrier_ref: e.target.value }))}
              placeholder="MAWB / Bill of Lading"
              className="w-full rounded-xl border border-white/10 bg-white/5 px-3 py-2.5 text-sm text-white placeholder-white/20 focus:border-cyan-500 focus:outline-none" />
          </Field>
          <div className="flex items-center justify-end gap-3 pt-1">
            <button type="button" onClick={onClose}
              className="rounded-xl border border-white/10 bg-white/5 px-4 py-2 text-sm text-white/60 transition-colors hover:text-white">
              Cancel
            </button>
            <button type="submit" disabled={saving}
              className="flex items-center gap-1.5 rounded-xl border border-cyan-500/50 bg-cyan-500/10 px-5 py-2 text-sm font-semibold text-cyan-300 transition-all hover:bg-cyan-500/20 disabled:opacity-40">
              {saving ? <Loader2 size={13} className="animate-spin" /> : <Plus size={13} />}
              {saving ? "Creating…" : "Create"}
            </button>
          </div>
        </form>
      </motion.div>
    </motion.div>
  );
}

// ── Deconsolidate Zone Modal ──────────────────────────────────────────────────
function DeconsolidateModal({
  containerId, onClose, onDone, api,
}: {
  containerId: string;
  onClose: () => void;
  onDone: () => void;
  api: ReturnType<typeof createHubTransferApi>;
}) {
  const [zone, setZone] = useState("");
  const [busy, setBusy] = useState(false);

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    setBusy(true);
    try {
      await api.deconsolidate(containerId, zone.trim());
      toast.success("Deconsolidation triggered — routing fan-out in progress");
      onDone();
    } catch (e) {
      toast.error((e as { message?: string })?.message ?? "Deconsolidation failed");
    } finally {
      setBusy(false);
    }
  }

  return (
    <motion.div initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }}
      className="fixed inset-0 z-50 flex items-center justify-center p-4"
      onClick={onClose}>
      <div className="absolute inset-0 bg-black/60 backdrop-blur-sm" />
      <motion.div initial={{ opacity: 0, y: 14, scale: 0.97 }} animate={{ opacity: 1, y: 0, scale: 1 }}
        exit={{ opacity: 0, y: 14, scale: 0.97 }}
        transition={{ type: "spring", damping: 28, stiffness: 260 }}
        className="relative w-full max-w-sm rounded-2xl border border-glass-border bg-[#0d1422] shadow-2xl overflow-hidden"
        onClick={(e) => e.stopPropagation()}>
        <div className="flex items-center justify-between border-b border-glass-border px-5 py-4">
          <h2 className="font-heading text-sm font-bold text-white">Deconsolidate Container</h2>
          <button onClick={onClose} className="rounded-lg p-1.5 text-white/40 transition-colors hover:bg-glass-200 hover:text-white"><X size={14} /></button>
        </div>
        <form onSubmit={submit} className="space-y-4 p-5">
          <p className="font-mono text-xs text-white/50">
            Enter the destination zone for routing rule lookup. Leave blank to use
            the default own-driver routing.
          </p>
          <Field label="Destination Zone">
            <input value={zone} onChange={(e) => setZone(e.target.value)}
              placeholder="e.g. Metro Manila, Visayas"
              className="w-full rounded-xl border border-white/10 bg-white/5 px-3 py-2.5 text-sm text-white placeholder-white/20 focus:border-cyan-500 focus:outline-none" />
          </Field>
          <div className="flex items-center justify-end gap-3 pt-1">
            <button type="button" onClick={onClose}
              className="rounded-xl border border-white/10 bg-white/5 px-4 py-2 text-sm text-white/60 transition-colors hover:text-white">
              Cancel
            </button>
            <button type="submit" disabled={busy}
              className="flex items-center gap-1.5 rounded-xl border border-purple-plasma/50 bg-purple-plasma/10 px-5 py-2 text-sm font-semibold text-purple-plasma transition-all hover:bg-purple-plasma/20 disabled:opacity-40">
              {busy ? <Loader2 size={13} className="animate-spin" /> : <ArrowRight size={13} />}
              {busy ? "Processing…" : "Deconsolidate"}
            </button>
          </div>
        </form>
      </motion.div>
    </motion.div>
  );
}

// ── Container Board ───────────────────────────────────────────────────────────
function ContainerBoard({ hubId, hubs }: { hubId: string; hubs: Hub[] }) {
  const api = useMemo(() => createHubTransferApi(), []);
  const [containers, setContainers] = useState<ContainerSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [createOpen, setCreateOpen] = useState(false);
  const [deconsolidateId, setDeconsolidateId] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    try { setContainers(await api.listContainers({ hubId })); }
    catch (e) { toast.error((e as { message?: string })?.message ?? "Failed to load containers"); }
    finally { setLoading(false); }
  }, [api, hubId]);

  useEffect(() => { load(); }, [load]);

  async function act(id: string, fn: () => Promise<unknown>, label: string) {
    setBusy(id);
    try { await fn(); toast.success(label); await load(); }
    catch (e) { toast.error((e as { message?: string })?.message ?? "Action failed"); }
    finally { setBusy(null); }
  }

  // Next transition — transport-mode-aware.
  // Road containers skip port/customs: in_transit → release-domestic → deconsolidate.
  // Sea/air containers go through port → customs → release → deconsolidate.
  function nextAction(c: ContainerSummary): { label: string; run: () => void } | null {
    const customs = isCustomsMode(c.transport_mode);
    switch (c.status) {
      case "in_transit":
        return customs
          ? { label: "Arrive at Port",    run: () => act(c.id, () => api.arriveAtPort(c.id),    "Arrived at port ✓") }
          : { label: "Release Domestic",  run: () => act(c.id, () => api.releaseDomestic(c.id), "Released domestic ✓") };
      case "arrived_at_port":
        return { label: "Enter Customs",  run: () => act(c.id, () => api.enterCustoms(c.id),    "Entered customs ✓") };
      case "customs":
        return { label: "Clear Customs",  run: () => act(c.id, () => api.clearCustoms(c.id),    "Customs cleared ✓") };
      case "released":
        return { label: "Deconsolidate",  run: () => setDeconsolidateId(c.id) };
      default:
        return null;
    }
  }

  const byStatus = (s: string) => containers.filter((c) => c.status === s);

  return (
    <>
      {/* Toolbar */}
      <div className="mb-3 flex items-center justify-end">
        <button onClick={() => setCreateOpen(true)}
          className="flex items-center gap-1.5 rounded-lg border border-cyan-500/40 bg-cyan-500/10 px-3 py-2 text-xs font-semibold text-cyan-300 transition-colors hover:bg-cyan-500/20">
          <Plus size={12} /> New Container
        </button>
      </div>

      {loading ? <CardLoader label="loading containers…" /> : containers.length === 0 ? (
        <EmptyState icon={Container} title="No containers in scope" hint="Create a container or wait for manifests to appear." />
      ) : (
        <div className="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">
          {CONTAINER_BOARD_COLUMNS.map((col) => {
            const items = byStatus(col.status);
            return (
              <GlassCard key={col.status} size="sm" className="flex flex-col">
                <div className="mb-3 flex items-center justify-between">
                  <p className="text-2xs font-mono uppercase tracking-widest text-white/40">{col.label}</p>
                  <NeonBadge variant={STATUS_TONE[col.status] ?? "muted"}>{items.length}</NeonBadge>
                </div>
                <div className="flex flex-col gap-2">
                  {items.length === 0 && <p className="py-3 text-center text-2xs font-mono text-white/20">—</p>}
                  {items.map((c) => {
                    const next = nextAction(c);
                    return (
                      <div key={c.id} className="rounded-xl border border-glass-border bg-glass-100 p-3">
                        <div className="flex items-center justify-between">
                          <span className="font-mono text-xs text-white">{shortId(c.id)}</span>
                          <span className="text-2xs font-mono text-white/30 uppercase">
                            {transportModeLabel(c.transport_mode)}
                          </span>
                        </div>
                        <p className="mt-1 text-2xs font-mono text-white/40">
                          {c.awb_count ?? 0} AWB · {shortId(c.origin_hub_id)} → {shortId(c.destination_hub_id)}
                        </p>
                        {next && (
                          <button
                            onClick={next.run}
                            disabled={busy === c.id}
                            className="mt-2 flex w-full items-center justify-center gap-1.5 rounded-lg border border-cyan-500/40 bg-cyan-500/10 px-2 py-1.5 text-2xs font-semibold text-cyan-300 transition-colors hover:bg-cyan-500/20 disabled:opacity-40"
                          >
                            {busy === c.id ? <Loader2 size={11} className="animate-spin" /> : <ChevronRight size={11} />}
                            {next.label}
                          </button>
                        )}
                      </div>
                    );
                  })}
                </div>
              </GlassCard>
            );
          })}
        </div>
      )}

      <AnimatePresence>
        {createOpen && (
          <CreateContainerModal
            hubs={hubs}
            onClose={() => setCreateOpen(false)}
            onCreate={() => { setCreateOpen(false); load(); }}
          />
        )}
        {deconsolidateId && (
          <DeconsolidateModal
            containerId={deconsolidateId}
            api={api}
            onClose={() => setDeconsolidateId(null)}
            onDone={() => { setDeconsolidateId(null); load(); }}
          />
        )}
      </AnimatePresence>
    </>
  );
}

// ── Customs Queue ─────────────────────────────────────────────────────────────
function CustomsQueue({ hubId }: { hubId: string }) {
  const api = useMemo(() => createHubTransferApi(), []);
  const [rows, setRows] = useState<TransferManifest[]>([]);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    try { setRows(await api.customsQueue(hubId)); }
    catch (e) { toast.error((e as { message?: string })?.message ?? "Failed to load customs queue"); }
    finally { setLoading(false); }
  }, [api, hubId]);

  useEffect(() => { load(); }, [load]);

  async function clear(m: TransferManifest) {
    setBusy(m.container_id);
    try {
      await api.clearCustoms(m.container_id, { customs_filing_ref: m.customs_filing_ref ?? undefined });
      toast.success(`Container ${shortId(m.container_id)} cleared`);
      await load();
    } catch (e) { toast.error((e as { message?: string })?.message ?? "Clearance failed"); }
    finally { setBusy(null); }
  }

  if (loading) return <CardLoader label="loading customs queue…" />;
  if (rows.length === 0) return <EmptyState icon={ShieldCheck} title="Customs queue is clear" hint="No containers are currently under customs hold at this hub." />;

  return (
    <div className="flex flex-col gap-2">
      {rows.map((m) => (
        <GlassCard key={m.id} size="sm" glow={m.customs_status === "hold" ? "red" : undefined}>
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div className="min-w-0">
              <div className="flex items-center gap-2">
                <Container size={14} className="text-cyan-neon shrink-0" />
                <span className="font-mono text-sm text-white">{shortId(m.container_id)}</span>
                <NeonBadge variant={customsTone(m.customs_status)} dot={m.customs_status === "hold"}>
                  {m.customs_status}
                </NeonBadge>
              </div>
              <p className="mt-1 text-2xs font-mono text-white/40">
                {transportModeLabel(m.transport_mode)} · filing {m.customs_filing_ref ?? "—"} ·
                {m.duties_total_cents != null ? ` duties ₱${(m.duties_total_cents / 100).toLocaleString()}` : " no duties"}
              </p>
            </div>
            {m.customs_status !== "cleared" && (
              <button
                onClick={() => clear(m)}
                disabled={busy === m.container_id}
                className="flex items-center gap-1.5 rounded-xl border border-green-signal/40 bg-green-signal/10 px-4 py-2 text-xs font-semibold text-green-signal transition-colors hover:bg-green-signal/20 disabled:opacity-40"
              >
                {busy === m.container_id ? <Loader2 size={12} className="animate-spin" /> : <PackageCheck size={12} />}
                Clear Customs
              </button>
            )}
          </div>
        </GlassCard>
      ))}
    </div>
  );
}

// ── Routing Config ────────────────────────────────────────────────────────────
const ROUTING_TONE: Record<RoutingType, "cyan" | "purple" | "amber"> = {
  own_driver: "cyan", carrier: "purple", auto: "amber",
};

function RoutingConfigView({ hubId }: { hubId: string }) {
  const api = useMemo(() => createHubTransferApi(), []);
  const [rows, setRows] = useState<RoutingConfig[]>([]);
  const [loading, setLoading] = useState(true);
  const [form, setForm] = useState({ destination_zone: "", routing_type: "auto" as RoutingType, window: "30" });
  const [saving, setSaving] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    try { setRows(await api.listRouting(hubId)); }
    catch (e) { toast.error((e as { message?: string })?.message ?? "Failed to load routing config"); }
    finally { setLoading(false); }
  }, [api, hubId]);

  useEffect(() => { load(); }, [load]);

  async function save(e: React.FormEvent) {
    e.preventDefault();
    if (!form.destination_zone.trim()) { toast.error("Destination zone is required"); return; }
    setSaving(true);
    try {
      await api.upsertRouting({
        hub_id: hubId,
        destination_zone: form.destination_zone.trim(),
        routing_type: form.routing_type,
        auto_fallback_window_mins: parseInt(form.window, 10) || 30,
      });
      toast.success("Routing rule saved");
      setForm({ destination_zone: "", routing_type: "auto", window: "30" });
      await load();
    } catch (e) { toast.error((e as { message?: string })?.message ?? "Save failed"); }
    finally { setSaving(false); }
  }

  return (
    <div className="grid grid-cols-1 gap-3 lg:grid-cols-3">
      {/* Existing rules */}
      <div className="lg:col-span-2">
        {loading ? <CardLoader label="loading routing rules…" />
          : rows.length === 0 ? <EmptyState icon={Route} title="No routing rules" hint="Add a rule to control how deconsolidated pieces are dispatched." />
          : (
            <div className="flex flex-col gap-2">
              {rows.map((r) => (
                <GlassCard key={r.id} size="sm">
                  <div className="flex items-center justify-between">
                    <div className="flex items-center gap-2">
                      <MapPin size={13} className="text-white/40" />
                      <span className="font-mono text-sm text-white">{r.destination_zone}</span>
                    </div>
                    <div className="flex items-center gap-2">
                      <NeonBadge variant={ROUTING_TONE[r.routing_type]}>{r.routing_type.replace("_", " ")}</NeonBadge>
                      {r.routing_type === "auto" && (
                        <span className="text-2xs font-mono text-white/30">{r.auto_fallback_window_mins}m fallback</span>
                      )}
                    </div>
                  </div>
                </GlassCard>
              ))}
            </div>
          )}
      </div>

      {/* Add rule */}
      <GlassCard size="sm" className="h-fit">
        <p className="mb-3 text-2xs font-mono uppercase tracking-widest text-white/40">New Routing Rule</p>
        <form onSubmit={save} className="space-y-3">
          <Field label="Destination Zone">
            <input value={form.destination_zone} onChange={(e) => setForm((f) => ({ ...f, destination_zone: e.target.value }))}
              placeholder="e.g. Makati"
              className="w-full rounded-xl border border-white/10 bg-white/5 px-3 py-2 text-sm text-white placeholder-white/20 focus:border-cyan-500 focus:outline-none" />
          </Field>
          <Field label="Routing Type">
            <select value={form.routing_type} onChange={(e) => setForm((f) => ({ ...f, routing_type: e.target.value as RoutingType }))}
              className="w-full rounded-xl border border-white/10 bg-white/5 px-3 py-2 text-sm text-white focus:border-cyan-500 focus:outline-none">
              <option value="auto">Auto (driver → carrier fallback)</option>
              <option value="own_driver">Own Driver</option>
              <option value="carrier">Carrier</option>
            </select>
          </Field>
          {form.routing_type === "auto" && (
            <Field label="Fallback Window (mins)">
              <input type="number" min={1} value={form.window} onChange={(e) => setForm((f) => ({ ...f, window: e.target.value }))}
                className="w-full rounded-xl border border-white/10 bg-white/5 px-3 py-2 text-sm text-white focus:border-cyan-500 focus:outline-none" />
            </Field>
          )}
          <button type="submit" disabled={saving}
            className="flex w-full items-center justify-center gap-1.5 rounded-xl border border-cyan-500/50 bg-cyan-500/10 px-4 py-2 text-sm font-semibold text-cyan-300 transition-all hover:bg-cyan-500/20 disabled:opacity-40">
            {saving ? <Loader2 size={13} className="animate-spin" /> : <ArrowRight size={13} />} Save Rule
          </button>
        </form>
      </GlassCard>
    </div>
  );
}

// ── Inventory Map ─────────────────────────────────────────────────────────────
function InventoryMap({ hubId }: { hubId: string }) {
  const api = useMemo(() => createHubTransferApi(), []);
  const [locations, setLocations] = useState<HubLocation[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [inventory, setInventory] = useState<HubInventory[]>([]);
  const [loading, setLoading] = useState(true);
  const [invLoading, setInvLoading] = useState(false);

  useEffect(() => {
    setLoading(true);
    api.listLocations(hubId)
      .then((locs) => { setLocations(locs); setSelected(locs[0]?.id ?? null); })
      .catch((e) => toast.error((e as { message?: string })?.message ?? "Failed to load locations"))
      .finally(() => setLoading(false));
  }, [api, hubId]);

  useEffect(() => {
    if (!selected) { setInventory([]); return; }
    setInvLoading(true);
    api.listInventory(selected)
      .then(setInventory)
      .catch((e) => toast.error((e as { message?: string })?.message ?? "Failed to load inventory"))
      .finally(() => setInvLoading(false));
  }, [api, selected]);

  if (loading) return <CardLoader label="loading locations…" />;
  if (locations.length === 0) return <EmptyState icon={Boxes} title="No storage locations" hint="Define hub locations to map inventory positions." />;

  return (
    <div className="grid grid-cols-1 gap-3 lg:grid-cols-3">
      {/* Location list */}
      <GlassCard size="sm" className="lg:col-span-1">
        <p className="mb-3 text-2xs font-mono uppercase tracking-widest text-white/40">Locations ({locations.length})</p>
        <div className="flex max-h-[28rem] flex-col gap-1.5 overflow-y-auto">
          {locations.map((l) => (
            <button key={l.id} onClick={() => setSelected(l.id)}
              className={cn(
                "flex items-center justify-between rounded-lg border px-3 py-2 text-left transition-colors",
                selected === l.id ? "border-cyan-neon/50 bg-cyan-neon/10" : "border-glass-border bg-glass-100 hover:border-glass-border-bright",
              )}>
              <span className="font-mono text-xs text-white">{l.location_tag}</span>
              {!l.is_active && <span className="text-2xs font-mono text-white/30">inactive</span>}
            </button>
          ))}
        </div>
      </GlassCard>

      {/* Inventory at selected location */}
      <GlassCard size="sm" className="lg:col-span-2">
        <p className="mb-3 text-2xs font-mono uppercase tracking-widest text-white/40">
          Inventory{selected ? ` @ ${locations.find((l) => l.id === selected)?.location_tag ?? ""}` : ""}
        </p>
        {invLoading ? <CardLoader label="loading inventory…" />
          : inventory.length === 0 ? <p className="py-6 text-center text-xs font-mono text-white/30">Location is empty</p>
          : (
            <div className="grid grid-cols-1 gap-1.5 sm:grid-cols-2">
              {inventory.map((i) => (
                <div key={i.id} className="flex items-center justify-between rounded-lg border border-glass-border bg-glass-100 px-3 py-2">
                  <span className="truncate font-mono text-xs text-white">{i.inventory_unit.id}</span>
                  <NeonBadge variant={i.inventory_unit.kind === "consolidated" ? "purple" : "cyan"}>
                    {i.inventory_unit.kind === "consolidated" ? "pallet" : "piece"}
                  </NeonBadge>
                </div>
              ))}
            </div>
          )}
      </GlassCard>
    </div>
  );
}

// ── Shared bits ─────────────────────────────────────────────────────────────
function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div>
      <label className="mb-1.5 block text-[11px] font-mono uppercase tracking-wider text-white/40">{label}</label>
      {children}
    </div>
  );
}
function CardLoader({ label }: { label: string }) {
  return <GlassCard className="py-10 text-center"><p className="text-xs font-mono text-white/40">{label}</p></GlassCard>;
}
function EmptyState({ icon: Icon, title, hint }: { icon: typeof Container; title: string; hint: string }) {
  return (
    <GlassCard className="py-12 text-center">
      <Icon size={34} className="mx-auto mb-3 text-white/20" />
      <p className="text-sm font-mono text-white/60">{title}</p>
      <p className="mt-1 text-xs font-mono text-white/30">{hint}</p>
    </GlassCard>
  );
}

// ── Page ──────────────────────────────────────────────────────────────────────
export default function HubTransferPage() {
  const hubsApi = useMemo(() => createHubsApi(), []);
  const [hubs, setHubs] = useState<Hub[]>([]);
  const [hubId, setHubId] = useState<string>("");
  const [tab, setTab] = useState<Tab>("board");
  const [hubsLoading, setHubsLoading] = useState(true);

  useEffect(() => {
    hubsApi.list()
      .then((list) => { setHubs(list); setHubId(list[0] ? hubIdOf(list[0]) : ""); })
      .catch(() => {})
      .finally(() => setHubsLoading(false));
  }, [hubsApi]);

  return (
    <motion.div variants={variants.staggerContainer} initial="hidden" animate="visible" className="flex flex-col gap-5 p-6">
      {/* Header */}
      <motion.div variants={variants.fadeInUp} className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="flex items-center gap-2 font-heading text-2xl font-bold text-white">
            <Container size={22} className="text-purple-plasma" />
            Cross-Border Hub Transfer
          </h1>
          <p className="mt-0.5 font-mono text-sm text-white/40">Container customs lifecycle · deconsolidation · routing</p>
        </div>
        <div className="flex items-center gap-2">
          {hubsLoading ? (
            <span className="font-mono text-xs text-white/30">loading hubs…</span>
          ) : hubs.length > 0 ? (
            <select value={hubId} onChange={(e) => setHubId(e.target.value)}
              className="rounded-lg border border-glass-border bg-[#0d1422] px-3 py-2 text-xs font-mono text-white focus:border-cyan-500 focus:outline-none">
              {hubs.map((h) => <option key={hubIdOf(h)} value={hubIdOf(h)}>{h.name}</option>)}
            </select>
          ) : (
            <span className="flex items-center gap-1.5 font-mono text-xs text-amber-signal">
              <AlertTriangle size={12} /> No hubs configured
            </span>
          )}
          <NeonBadge variant="green" dot>Live</NeonBadge>
        </div>
      </motion.div>

      {/* Tabs */}
      <motion.div variants={variants.fadeInUp} className="flex flex-wrap gap-1.5 border-b border-glass-border pb-px">
        {TABS.map((t) => {
          const Icon = t.icon;
          const active = tab === t.key;
          return (
            <button key={t.key} onClick={() => setTab(t.key)}
              className={cn(
                "flex items-center gap-1.5 rounded-t-lg border-b-2 px-4 py-2 text-xs font-semibold transition-colors",
                active ? "border-cyan-neon text-cyan-neon" : "border-transparent text-white/40 hover:text-white/70",
              )}>
              <Icon size={13} /> {t.label}
            </button>
          );
        })}
      </motion.div>

      {/* Content */}
      <motion.div variants={variants.fadeInUp}>
        {!hubId ? (
          <EmptyState icon={Container} title="Select a hub" hint="Choose a hub from the selector to view its transfer operations." />
        ) : tab === "board" ? <ContainerBoard hubId={hubId} hubs={hubs} />
          : tab === "customs" ? <CustomsQueue hubId={hubId} />
          : tab === "routing" ? <RoutingConfigView hubId={hubId} />
          : <InventoryMap hubId={hubId} />}
      </motion.div>
    </motion.div>
  );
}
