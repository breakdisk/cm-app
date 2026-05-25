'use client';

import { useState, useEffect, useCallback, useMemo } from 'react';
import dynamic from 'next/dynamic';
import { AnimatePresence, motion } from 'framer-motion';
import {
  Boxes, Truck, Weight, Package, AlertTriangle,
  Wifi, WifiOff, RefreshCw, ChevronRight, Settings2,
  Download, ArrowRight, ArrowLeft, ArrowUp, ArrowDown,
  Info,
} from 'lucide-react';
import { createApiClient } from '@/lib/api/client';
import { createConsolidationApi } from '@/lib/api/consolidation';
import type {
  TruckSpec, ConsolidationPlan, Placement, UnplacedItem,
  CreateSpecBody, UpdateSpecBody,
} from '@/lib/api/consolidation';
import { createHubsApi } from '@/lib/api/hubs';
import { useHubEvents } from '@/hooks/useHubEvents';
import { cn } from '@/lib/utils';

// R3F Canvas — SSR-disabled (WebGL needs browser).
const PackingCanvas = dynamic(() => import('./PackingCanvas'), { ssr: false });
const TruckSpecModal = dynamic(() => import('./TruckSpecModal'), { ssr: false });

// ── Client-side collision / bounds helpers ────────────────────────────────────

function overlaps(a: Placement, b: Placement): boolean {
  return (
    a.x < b.x + b.length_cm && a.x + a.length_cm > b.x &&
    a.y < b.y + b.width_cm  && a.y + a.width_cm  > b.y &&
    a.z < b.z + b.height_cm && a.z + a.height_cm > b.z
  );
}

function canPlace(p: Placement, spec: TruckSpec, others: Placement[]): boolean {
  if (p.x + p.length_cm > spec.interior_length_cm) return false;
  if (p.y + p.width_cm  > spec.interior_width_cm)  return false;
  if (p.z + p.height_cm > spec.interior_height_cm) return false;
  if (p.x < 0 || p.y < 0 || p.z < 0)              return false;
  return !others.some(o => o.awb !== p.awb && overlaps(p, o));
}

// ── Helpers ───────────────────────────────────────────────────────────────────

function pct(used: number, total: number): number {
  if (total === 0) return 0;
  return Math.round((used / total) * 100);
}

function loadLabel(volumePct: number): { label: string; color: string } {
  if (volumePct >= 90) return { label: 'FULL',      color: '#FF3B5C' };
  if (volumePct >= 50) return { label: 'LTL',       color: '#FFAB00' };
  return                      { label: 'REMAINING', color: '#00FF88' };
}

function kgStr(grams: number): string {
  return (grams / 1000).toFixed(1);
}

// ── Sub-components ────────────────────────────────────────────────────────────

function StatCard({
  label, value, unit, color, icon: Icon,
}: {
  label: string; value: string | number; unit?: string;
  color: string; icon: React.ElementType;
}) {
  return (
    <div className="flex flex-col gap-1 rounded-xl border border-white/10 bg-white/5 p-4 backdrop-blur-sm">
      <div className="flex items-center gap-2 text-xs text-white/50">
        <Icon size={13} />
        {label}
      </div>
      <div className="flex items-baseline gap-1">
        <span className="text-2xl font-bold font-mono" style={{ color }}>
          {value}
        </span>
        {unit && <span className="text-xs text-white/40">{unit}</span>}
      </div>
    </div>
  );
}

function ItemRow({
  awb, weightG, estimated, selected, onClick,
}: {
  awb: string; weightG: number; estimated: boolean;
  selected: boolean; onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      className={cn(
        'flex w-full items-center justify-between rounded-lg px-3 py-2 text-left text-sm transition-all',
        selected
          ? 'bg-cyan-500/20 border border-cyan-500/40 text-cyan-300'
          : 'bg-white/5 hover:bg-white/10 text-white/70 border border-transparent',
      )}
    >
      <span className="font-mono text-xs truncate">{awb}</span>
      <div className="flex items-center gap-2 shrink-0">
        {estimated && (
          <span className="rounded-sm bg-amber-500/20 px-1 text-[10px] text-amber-400">EST</span>
        )}
        <span className="text-white/40 text-xs">{kgStr(weightG)} kg</span>
        {selected && <ChevronRight size={12} className="text-cyan-400" />}
      </div>
    </button>
  );
}

function UnplacedRow({ item }: { item: UnplacedItem }) {
  const reasonColor = {
    no_space:       '#FF3B5C',
    weight_limit:   '#FFAB00',
    zero_dimension: '#A855F7',
  }[item.reason] ?? '#FF3B5C';

  return (
    <div className="flex items-center justify-between rounded-lg bg-white/5 px-3 py-2 border border-red-500/20">
      <span className="font-mono text-xs text-white/60 truncate">{item.awb}</span>
      <span className="text-[10px] font-medium ml-2 shrink-0" style={{ color: reasonColor }}>
        {item.reason.replace(/_/g, ' ')}
      </span>
    </div>
  );
}

// ── Nudge button ──────────────────────────────────────────────────────────────

function NudgeBtn({
  icon: Icon, label, onClick, disabled,
}: { icon: React.ElementType; label: string; onClick: () => void; disabled?: boolean }) {
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      title={label}
      className="flex items-center justify-center rounded-lg border border-white/10 bg-white/5 p-1.5 text-white/50 hover:bg-white/10 hover:text-white disabled:opacity-20 transition-colors"
    >
      <Icon size={12} />
    </button>
  );
}

// ── Main component ────────────────────────────────────────────────────────────

interface Props {
  hubId:       string;
  token:       string;
  /** Override root height class. Defaults to h-[calc(100vh-4rem)] for the
   *  standalone full-page route. Pass a smaller value when embedding inside
   *  a tabbed layout (e.g. "h-[calc(100vh-10rem)]"). */
  heightClass?: string;
}

export default function ConsolidationPageClient({ hubId, token, heightClass }: Props) {
  // API clients
  const consolidationApi = useMemo(() => createConsolidationApi(createApiClient()), []);
  const hubsApi          = useMemo(() => createHubsApi(), []);

  // State
  const [specs,          setSpecs]          = useState<TruckSpec[]>([]);
  const [selectedSpecId, setSelectedSpecId] = useState<string>('');
  const [currentPlan,    setCurrentPlan]    = useState<ConsolidationPlan | null>(null);
  const [placements,     setPlacements]     = useState<Placement[]>([]);
  const [loading,        setLoading]        = useState(true);
  const [initError,      setInitError]      = useState<string | null>(null);
  const [retryKey,       setRetryKey]       = useState(0);
  const [optimizing,     setOptimizing]     = useState(false);
  const [loadingManifest, setLoadingManifest] = useState(false);
  const [manifestCount,  setManifestCount]  = useState<number | null>(null);
  const [selectedAwb,    setSelectedAwb]    = useState<string | null>(null);
  const [wsConnected,    setWsConnected]    = useState(false);
  const [specsModalOpen, setSpecsModalOpen] = useState(false);
  const [scanFeed,       setScanFeed]       = useState<string[]>([]);

  // Initialise — load specs and latest plan together.
  // `retryKey` increments on manual retry to re-trigger this effect.
  useEffect(() => {
    let cancelled = false;
    async function init() {
      setLoading(true);
      setInitError(null);
      try {
        const [specsData, plans] = await Promise.all([
          consolidationApi.listSpecs(),
          consolidationApi.listPlans(hubId),
        ]);
        if (cancelled) return;
        setSpecs(specsData);
        if (specsData.length > 0) setSelectedSpecId(specsData[0].id);
        if (plans.length > 0) {
          setCurrentPlan(plans[0]);
          setPlacements(plans[0].placements as Placement[]);
        }
      } catch (e) {
        if (cancelled) return;
        console.error('consolidation init error', e);
        const msg = (e as { message?: string })?.message;
        setInitError(msg ?? 'Failed to load plan data — check your connection and retry.');
      } finally {
        if (!cancelled) setLoading(false);
      }
    }
    init();
    return () => { cancelled = true; };
  }, [hubId, consolidationApi, retryKey]);

  // WebSocket — live plan_computed and box_scanned events.
  useHubEvents({
    hubId,
    token,
    onConnect:    () => setWsConnected(true),
    onDisconnect: () => setWsConnected(false),
    onEvent: useCallback(async (event) => {
      if (event.type === 'plan_computed' && event.plan_id) {
        try {
          const plan = await consolidationApi.getPlan(event.plan_id);
          setCurrentPlan(plan);
          setPlacements(plan.placements as Placement[]);
        } catch {}
      }
      if (event.type === 'box_scanned' && event.plan_id) {
        setScanFeed(prev => [event.plan_id!, ...prev].slice(0, 20));
      }
    }, [consolidationApi]),
  });

  // Derived values
  const selectedSpec = useMemo(
    () => specs.find(s => s.id === selectedSpecId) ?? null,
    [specs, selectedSpecId],
  );
  const unplaced: UnplacedItem[] = (currentPlan?.unplaced ?? []) as UnplacedItem[];
  const volumePct = currentPlan
    ? pct(currentPlan.volume_used_cm3, currentPlan.volume_total_cm3)
    : 0;
  const { label: loadLbl, color: loadColor } = loadLabel(volumePct);

  // ── Load from Hub Manifest ──────────────────────────────────────────────────
  async function handleLoadManifest() {
    setLoadingManifest(true);
    try {
      const manifest = await hubsApi.manifest(hubId);
      setManifestCount(manifest.count);
      // Build BoxItem list from inductions.
      // Dims are unknown here — server will estimate from weight_g via DIM factor.
      // We use 10 kg as a placeholder; ops can see the "EST" flag in the viewer.
      const items = manifest.parcels.map(p => ({
        awb:       p.tracking_number,
        weight_g:  10_000,   // 10 kg placeholder — server estimates dims
        length_cm: 0,
        width_cm:  0,
        height_cm: 0,
      }));

      if (items.length === 0) return;

      // Immediately run the optimiser with the manifest items.
      if (!selectedSpecId) return;
      setOptimizing(true);
      try {
        const plan = await consolidationApi.computePlan({
          hub_id:        hubId,
          truck_spec_id: selectedSpecId,
          items,
        });
        setCurrentPlan(plan);
        setPlacements(plan.placements as Placement[]);
      } finally {
        setOptimizing(false);
      }
    } catch (e) {
      console.error('manifest load error', e);
    } finally {
      setLoadingManifest(false);
    }
  }

  // ── Re-optimise ─────────────────────────────────────────────────────────────
  async function handleOptimize() {
    if (!selectedSpecId) return;
    setOptimizing(true);
    try {
      // Use items from the current plan if one exists, otherwise the manifest fallback.
      const items = currentPlan
        ? (currentPlan.items as Array<{ awb: string; weight_g: number; length_cm: number; width_cm: number; height_cm: number }>)
        : [];

      if (items.length === 0) {
        // Auto-load manifest if no items available yet.
        await handleLoadManifest();
        return;
      }

      const plan = await consolidationApi.computePlan({
        hub_id:        hubId,
        truck_spec_id: selectedSpecId,
        items,
      });
      setCurrentPlan(plan);
      setPlacements(plan.placements as Placement[]);
    } catch (e) {
      console.error('compute_plan error', e);
    } finally {
      setOptimizing(false);
    }
  }

  // ── Nudge (keyboard + buttons) ───────────────────────────────────────────────
  const handleNudge = useCallback(async (awb: string, axis: 'x' | 'y' | 'z', delta: number) => {
    if (!currentPlan || !selectedSpec) return;
    const idx = placements.findIndex(p => p.awb === awb);
    if (idx < 0) return;

    const old = placements[idx];
    const updated: Placement = {
      ...old,
      x: axis === 'x' ? old.x + delta : old.x,
      y: axis === 'y' ? old.y + delta : old.y,
      z: axis === 'z' ? old.z + delta : old.z,
    };

    const others = placements.filter((_, i) => i !== idx);
    if (!canPlace(updated, selectedSpec, others)) return;  // collision — ignore

    const newPlacements = [...placements];
    newPlacements[idx] = updated;
    setPlacements(newPlacements);

    try {
      await consolidationApi.updatePlacements(currentPlan.id, newPlacements);
    } catch (e) {
      // Revert on server error
      setPlacements(placements);
    }
  }, [placements, currentPlan, selectedSpec, consolidationApi]);

  // ── Spec management ──────────────────────────────────────────────────────────
  async function handleCreateSpec(body: CreateSpecBody) {
    const spec = await consolidationApi.createSpec(body);
    setSpecs(prev => [...prev, spec]);
    if (!selectedSpecId) setSelectedSpecId(spec.id);
  }

  async function handleUpdateSpec(id: string, body: UpdateSpecBody) {
    const updated = await consolidationApi.updateSpec(id, body);
    setSpecs(prev => prev.map(s => s.id === id ? updated : s));
  }

  // ── Selected box info ────────────────────────────────────────────────────────
  const selectedPlacement = placements.find(p => p.awb === selectedAwb) ?? null;

  if (loading) {
    return (
      <div className="flex h-full items-center justify-center">
        <div className="flex flex-col items-center gap-3">
          <div className="h-8 w-8 animate-spin rounded-full border-2 border-cyan-500 border-t-transparent" />
          <span className="text-sm text-white/40">Loading consolidation plan…</span>
        </div>
      </div>
    );
  }

  if (initError) {
    return (
      <div className="flex h-full items-center justify-center">
        <div className="flex flex-col items-center gap-4 text-center max-w-xs">
          <AlertTriangle size={32} className="text-amber-400" />
          <p className="text-sm font-mono text-white/60">{initError}</p>
          <button
            onClick={() => setRetryKey(k => k + 1)}
            className="flex items-center gap-2 rounded-lg border border-cyan-500/40 bg-cyan-500/10 px-4 py-2 text-xs font-semibold text-cyan-300 hover:bg-cyan-500/20 transition-all"
          >
            <RefreshCw size={12} /> Retry
          </button>
        </div>
      </div>
    );
  }

  return (
    <>
      <div className={cn('flex gap-4 p-4 overflow-hidden', heightClass ?? 'h-[calc(100vh-4rem)]')}>

        {/* ── Left panel ─────────────────────────────────────────────── */}
        <div className="flex w-64 shrink-0 flex-col gap-3 overflow-y-auto">

          {/* WS status */}
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-2 text-xs">
              {wsConnected
                ? <><Wifi size={12} className="text-green-400" /><span className="text-green-400">Live</span></>
                : <><WifiOff size={12} className="text-white/30" /><span className="text-white/30">Offline</span></>}
            </div>
            <button
              onClick={() => setSpecsModalOpen(true)}
              className="flex items-center gap-1 text-xs text-white/40 hover:text-white/70 transition-colors"
            >
              <Settings2 size={12} />
              Manage Specs
            </button>
          </div>

          {/* Load status badge */}
          {currentPlan && (
            <div
              className="flex items-center justify-between rounded-xl border px-4 py-3"
              style={{ borderColor: loadColor + '40', background: loadColor + '10' }}
            >
              <span className="text-sm font-bold tracking-widest" style={{ color: loadColor }}>
                {loadLbl}
              </span>
              <span className="text-2xl font-mono font-black" style={{ color: loadColor }}>
                {volumePct}%
              </span>
            </div>
          )}

          {/* KPI cards */}
          {currentPlan && <>
            <StatCard label="Volume used"    value={volumePct}                                 unit="%" color="#00E5FF" icon={Boxes}   />
            <StatCard label="Total weight"   value={currentPlan.total_weight_kg.toFixed(1)}    unit="kg" color="#A855F7" icon={Weight}  />
            <StatCard label="Pieces placed"  value={currentPlan.piece_count}                              color="#00FF88" icon={Package} />
            {unplaced.length > 0 && (
              <StatCard label="Unplaced" value={unplaced.length} color="#FF3B5C" icon={AlertTriangle} />
            )}
          </>}

          {/* Vehicle selector */}
          <div className="flex flex-col gap-1">
            <label className="text-[11px] text-white/40 uppercase tracking-wider">Vehicle</label>
            <select
              value={selectedSpecId}
              onChange={e => setSelectedSpecId(e.target.value)}
              className="rounded-lg border border-white/10 bg-white/5 px-3 py-2 text-sm text-white backdrop-blur-sm focus:border-cyan-500 focus:outline-none"
            >
              {specs.map(s => (
                <option key={s.id} value={s.id} className="bg-gray-900">{s.name}</option>
              ))}
              {specs.length === 0 && <option disabled>No specs — click Manage Specs</option>}
            </select>
          </div>

          {/* Actions */}
          <div className="flex flex-col gap-2">
            <button
              onClick={handleOptimize}
              disabled={optimizing || !selectedSpecId}
              className={cn(
                'flex items-center justify-center gap-2 rounded-xl py-3 text-sm font-semibold transition-all',
                'border border-cyan-500/50 bg-cyan-500/10 text-cyan-300',
                'hover:bg-cyan-500/20 hover:border-cyan-400 disabled:opacity-40 disabled:cursor-not-allowed',
              )}
            >
              <RefreshCw size={14} className={optimizing ? 'animate-spin' : ''} />
              {optimizing ? 'Optimising…' : 'Re-optimise Load'}
            </button>

            <button
              onClick={handleLoadManifest}
              disabled={loadingManifest || optimizing || !selectedSpecId}
              className={cn(
                'flex items-center justify-center gap-2 rounded-xl py-2.5 text-sm font-semibold transition-all',
                'border border-purple-500/40 bg-purple-500/10 text-purple-300',
                'hover:bg-purple-500/20 disabled:opacity-40 disabled:cursor-not-allowed',
              )}
            >
              <Download size={14} className={loadingManifest ? 'animate-bounce' : ''} />
              {loadingManifest ? 'Loading…' : manifestCount !== null
                ? `Reload Manifest (${manifestCount})`
                : 'Load from Manifest'}
            </button>
          </div>

          {/* Selected box info + nudge controls */}
          <AnimatePresence>
            {selectedPlacement && (
              <motion.div
                key="box-detail"
                initial={{ opacity: 0, y: 8 }}
                animate={{ opacity: 1, y: 0 }}
                exit={{ opacity: 0, y: 8 }}
                className="rounded-xl border border-purple-500/30 bg-purple-500/10 p-3 text-xs space-y-3"
              >
                <div className="font-mono font-semibold text-purple-300 truncate">
                  {selectedPlacement.awb}
                </div>

                {/* Dimensions & weight */}
                <div className="grid grid-cols-2 gap-x-3 gap-y-1 text-white/60">
                  <span>L×W×H</span>
                  <span className="font-mono text-white/80 text-right">
                    {selectedPlacement.length_cm}×{selectedPlacement.width_cm}×{selectedPlacement.height_cm} cm
                  </span>
                  <span>Weight</span>
                  <span className="font-mono text-white/80 text-right">{kgStr(selectedPlacement.weight_g)} kg</span>
                  <span>Position</span>
                  <span className="font-mono text-white/80 text-right">
                    ({selectedPlacement.x}, {selectedPlacement.y}, {selectedPlacement.z})
                  </span>
                  {selectedPlacement.estimated && (
                    <span className="col-span-2 text-amber-400 flex items-center gap-1">
                      <Info size={10} /> Dims estimated from weight
                    </span>
                  )}
                </div>

                {/* Nudge controls */}
                <div className="space-y-1.5">
                  <p className="text-[10px] uppercase tracking-wider text-white/30">Nudge (10 cm)</p>

                  {/* X axis — length (forward/back) */}
                  <div className="flex items-center gap-1.5">
                    <span className="w-4 text-[10px] font-mono text-white/30">X</span>
                    <NudgeBtn icon={ArrowLeft}  label="X −10 cm" onClick={() => handleNudge(selectedPlacement.awb, 'x', -10)} />
                    <NudgeBtn icon={ArrowRight} label="X +10 cm" onClick={() => handleNudge(selectedPlacement.awb, 'x',  10)} />
                    <span className="ml-auto font-mono text-[10px] text-white/40">{selectedPlacement.x} cm</span>
                  </div>

                  {/* Y axis — width (left/right) */}
                  <div className="flex items-center gap-1.5">
                    <span className="w-4 text-[10px] font-mono text-white/30">Y</span>
                    <NudgeBtn icon={ArrowLeft}  label="Y −10 cm" onClick={() => handleNudge(selectedPlacement.awb, 'y', -10)} />
                    <NudgeBtn icon={ArrowRight} label="Y +10 cm" onClick={() => handleNudge(selectedPlacement.awb, 'y',  10)} />
                    <span className="ml-auto font-mono text-[10px] text-white/40">{selectedPlacement.y} cm</span>
                  </div>

                  {/* Z axis — height (up/down) */}
                  <div className="flex items-center gap-1.5">
                    <span className="w-4 text-[10px] font-mono text-white/30">Z</span>
                    <NudgeBtn icon={ArrowDown} label="Z −10 cm" onClick={() => handleNudge(selectedPlacement.awb, 'z', -10)} />
                    <NudgeBtn icon={ArrowUp}   label="Z +10 cm" onClick={() => handleNudge(selectedPlacement.awb, 'z',  10)} />
                    <span className="ml-auto font-mono text-[10px] text-white/40">{selectedPlacement.z} cm</span>
                  </div>

                  {/* Keyboard hint */}
                  <p className="text-[10px] text-white/20 pt-1">
                    ← → ↑ ↓ move X/Y · Shift+↑↓ move Z
                  </p>
                </div>
              </motion.div>
            )}
          </AnimatePresence>
        </div>

        {/* ── Centre — 3D viewer ────────────────────────────────────── */}
        <div className="flex-1 overflow-hidden rounded-2xl border border-white/10">
          {selectedSpec ? (
            <PackingCanvas
              spec={selectedSpec}
              placements={placements}
              selectedAwb={selectedAwb}
              onSelect={setSelectedAwb}
              onNudge={handleNudge}
            />
          ) : (
            <div className="flex h-full items-center justify-center">
              <div className="text-center text-white/30">
                <Truck size={48} className="mx-auto mb-3 opacity-30" />
                <p className="text-sm">Select a vehicle spec to view the 3D layout</p>
                <button
                  onClick={() => setSpecsModalOpen(true)}
                  className="mt-3 text-xs text-cyan-400 hover:underline"
                >
                  Add vehicle specs →
                </button>
              </div>
            </div>
          )}
        </div>

        {/* ── Right panel — items + unplaced + scan feed ────────────── */}
        <div className="flex w-64 shrink-0 flex-col gap-3 overflow-y-auto">

          {/* Placed items */}
          <div>
            <div className="mb-2 text-[11px] uppercase tracking-wider text-white/40">
              Placed ({placements.length})
            </div>
            <div className="flex flex-col gap-1">
              {placements.map(p => (
                <ItemRow
                  key={p.awb}
                  awb={p.awb}
                  weightG={p.weight_g}
                  estimated={p.estimated}
                  selected={selectedAwb === p.awb}
                  onClick={() => setSelectedAwb(prev => prev === p.awb ? null : p.awb)}
                />
              ))}
              {placements.length === 0 && (
                <p className="text-xs text-white/30 px-2">
                  No items — click <span className="text-purple-400">Load from Manifest</span> to begin
                </p>
              )}
            </div>
          </div>

          {/* Unplaced */}
          {unplaced.length > 0 && (
            <div>
              <div className="mb-2 text-[11px] uppercase tracking-wider text-red-400/70">
                Unplaced ({unplaced.length})
              </div>
              <div className="flex flex-col gap-1">
                {unplaced.map(u => <UnplacedRow key={u.awb} item={u} />)}
              </div>
            </div>
          )}

          {/* Scan feed */}
          {scanFeed.length > 0 && (
            <div>
              <div className="mb-2 text-[11px] uppercase tracking-wider text-white/40">
                Scan feed
              </div>
              <div className="flex flex-col gap-1">
                {scanFeed.map((id, i) => (
                  <div key={i} className="rounded-lg bg-white/5 px-3 py-1.5 text-xs font-mono text-green-400">
                    {id}
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>
      </div>

      {/* Truck spec modal */}
      <AnimatePresence>
        {specsModalOpen && (
          <TruckSpecModal
            specs={specs}
            onClose={() => setSpecsModalOpen(false)}
            onCreate={handleCreateSpec}
            onUpdate={handleUpdateSpec}
          />
        )}
      </AnimatePresence>
    </>
  );
}
