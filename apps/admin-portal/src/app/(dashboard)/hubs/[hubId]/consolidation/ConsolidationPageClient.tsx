'use client';

import { useState, useEffect, useCallback, useRef, useMemo } from 'react';
import dynamic from 'next/dynamic';
import { motion, AnimatePresence } from 'framer-motion';
import {
  Boxes, Truck, Weight, Package, AlertTriangle,
  Wifi, WifiOff, RefreshCw, ChevronRight,
} from 'lucide-react';
import { createApiClient } from '@/lib/api/client';
import { createConsolidationApi } from '@/lib/api/consolidation';
import type { TruckSpec, ConsolidationPlan, Placement, UnplacedItem } from '@/lib/api/consolidation';
import { useHubEvents } from '@/hooks/useHubEvents';
import { cn } from '@/lib/utils';

// R3F Canvas — must be SSR-disabled (WebGL requires browser).
const PackingCanvas = dynamic(() => import('./PackingCanvas'), { ssr: false });

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

function kg(grams: number): string {
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
    <div className="relative flex flex-col gap-1 rounded-xl border border-white/10 bg-white/5 p-4 backdrop-blur-sm">
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
        <span className="text-white/40 text-xs">{kg(weightG)} kg</span>
        {selected && <ChevronRight size={12} className="text-cyan-400" />}
      </div>
    </button>
  );
}

function UnplacedRow({ item }: { item: UnplacedItem }) {
  const reasonColor = {
    no_space:      '#FF3B5C',
    weight_limit:  '#FFAB00',
    zero_dimension: '#A855F7',
  }[item.reason] ?? '#FF3B5C';

  return (
    <div className="flex w-full items-center justify-between rounded-lg bg-white/5 px-3 py-2 border border-red-500/20">
      <span className="font-mono text-xs text-white/60 truncate">{item.awb}</span>
      <span className="text-[10px] font-medium ml-2 shrink-0" style={{ color: reasonColor }}>
        {item.reason.replace('_', ' ')}
      </span>
    </div>
  );
}

// ── Main component ────────────────────────────────────────────────────────────

interface Props {
  hubId: string;
  token: string;
}

export default function ConsolidationPageClient({ hubId, token }: Props) {
  const api = useMemo(() => {
    const client = createApiClient();
    return createConsolidationApi(client);
  }, []);

  const [specs, setSpecs]               = useState<TruckSpec[]>([]);
  const [selectedSpecId, setSelectedSpecId] = useState<string>('');
  const [currentPlan, setCurrentPlan]   = useState<ConsolidationPlan | null>(null);
  const [loading, setLoading]           = useState(true);
  const [optimizing, setOptimizing]     = useState(false);
  const [selectedAwb, setSelectedAwb]   = useState<string | null>(null);
  const [wsConnected, setWsConnected]   = useState(false);
  const [scanFeed, setScanFeed]         = useState<string[]>([]);

  // Load specs + latest plan on mount.
  useEffect(() => {
    async function init() {
      setLoading(true);
      try {
        const [specsData, plans] = await Promise.all([
          api.listSpecs(),
          api.listPlans(hubId),
        ]);
        setSpecs(specsData);
        if (specsData.length > 0) setSelectedSpecId(specsData[0].id);
        if (plans.length > 0) setCurrentPlan(plans[0]);
      } catch (e) {
        console.error('consolidation init error', e);
      } finally {
        setLoading(false);
      }
    }
    init();
  }, [hubId, api]);

  // WebSocket — real-time plan updates.
  useHubEvents({
    hubId,
    token,
    onConnect:    () => setWsConnected(true),
    onDisconnect: () => setWsConnected(false),
    onEvent: useCallback(async (event) => {
      if (event.type === 'plan_computed' && event.plan_id) {
        try {
          const plan = await api.getPlan(event.plan_id);
          setCurrentPlan(plan);
        } catch {}
      }
      if (event.type === 'box_scanned' && event.plan_id) {
        setScanFeed(prev => [event.plan_id!, ...prev].slice(0, 20));
      }
    }, [api]),
  });

  const selectedSpec = useMemo(
    () => specs.find(s => s.id === selectedSpecId) ?? null,
    [specs, selectedSpecId]
  );

  const placements: Placement[]      = (currentPlan?.placements ?? []) as Placement[];
  const unplaced:   UnplacedItem[]   = (currentPlan?.unplaced   ?? []) as UnplacedItem[];
  const volumePct = currentPlan
    ? pct(currentPlan.volume_used_cm3, currentPlan.volume_total_cm3)
    : 0;
  const { label: loadLbl, color: loadColor } = loadLabel(volumePct);

  async function handleOptimize() {
    if (!selectedSpecId) return;
    // Build items from the parcel induction list.
    // For now we send the existing items from the latest plan, or a placeholder.
    const items = placements.length > 0
      ? placements.map(p => ({
          awb: p.awb, weight_g: p.weight_g,
          length_cm: p.length_cm, width_cm: p.width_cm, height_cm: p.height_cm,
        }))
      : [];

    setOptimizing(true);
    try {
      const plan = await api.computePlan({
        hub_id:        hubId,
        truck_spec_id: selectedSpecId,
        items,
      });
      setCurrentPlan(plan);
    } catch (e) {
      console.error('compute_plan error', e);
    } finally {
      setOptimizing(false);
    }
  }

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

  return (
    <div className="flex h-[calc(100vh-4rem)] gap-4 p-4 overflow-hidden">

      {/* ── Left panel — stats + controls ─────────────────────────────── */}
      <div className="flex w-64 shrink-0 flex-col gap-3 overflow-y-auto">
        {/* WS status */}
        <div className="flex items-center gap-2 text-xs">
          {wsConnected
            ? <><Wifi size={12} className="text-green-400" /><span className="text-green-400">Live</span></>
            : <><WifiOff size={12} className="text-white/30" /><span className="text-white/30">Offline</span></>}
        </div>

        {/* Load status badge */}
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

        {/* KPI cards */}
        <StatCard
          label="Volume used"
          value={volumePct}
          unit="%"
          color="#00E5FF"
          icon={Boxes}
        />
        <StatCard
          label="Total weight"
          value={currentPlan ? currentPlan.total_weight_kg.toFixed(1) : '—'}
          unit="kg"
          color="#A855F7"
          icon={Weight}
        />
        <StatCard
          label="Pieces placed"
          value={currentPlan?.piece_count ?? '—'}
          color="#00FF88"
          icon={Package}
        />
        {unplaced.length > 0 && (
          <StatCard
            label="Unplaced"
            value={unplaced.length}
            color="#FF3B5C"
            icon={AlertTriangle}
          />
        )}

        {/* Truck spec selector */}
        <div className="flex flex-col gap-1">
          <label className="text-[11px] text-white/40 uppercase tracking-wider">Vehicle</label>
          <select
            value={selectedSpecId}
            onChange={e => setSelectedSpecId(e.target.value)}
            className="rounded-lg border border-white/10 bg-white/5 px-3 py-2 text-sm text-white backdrop-blur-sm focus:border-cyan-500 focus:outline-none"
          >
            {specs.map(s => (
              <option key={s.id} value={s.id} className="bg-gray-900">
                {s.name}
              </option>
            ))}
            {specs.length === 0 && (
              <option disabled>No specs configured</option>
            )}
          </select>
        </div>

        {/* Optimise button */}
        <button
          onClick={handleOptimize}
          disabled={optimizing || !selectedSpecId}
          className={cn(
            'flex items-center justify-center gap-2 rounded-xl py-3 text-sm font-semibold transition-all',
            'border border-cyan-500/50 bg-cyan-500/10 text-cyan-300',
            'hover:bg-cyan-500/20 hover:border-cyan-400',
            'disabled:opacity-40 disabled:cursor-not-allowed',
          )}
        >
          <RefreshCw size={14} className={optimizing ? 'animate-spin' : ''} />
          {optimizing ? 'Optimising…' : 'Re-optimise Load'}
        </button>

        {/* Selected box info */}
        <AnimatePresence>
          {selectedAwb && (() => {
            const p = placements.find(x => x.awb === selectedAwb);
            if (!p) return null;
            return (
              <motion.div
                key="box-info"
                initial={{ opacity: 0, y: 8 }}
                animate={{ opacity: 1, y: 0 }}
                exit={{ opacity: 0, y: 8 }}
                className="rounded-xl border border-purple-500/30 bg-purple-500/10 p-3 text-xs"
              >
                <div className="font-mono font-semibold text-purple-300 mb-2 truncate">{p.awb}</div>
                <div className="grid grid-cols-2 gap-1 text-white/60">
                  <span>L×W×H</span>
                  <span className="font-mono text-white/80">
                    {p.length_cm}×{p.width_cm}×{p.height_cm} cm
                  </span>
                  <span>Weight</span>
                  <span className="font-mono text-white/80">{kg(p.weight_g)} kg</span>
                  <span>Position</span>
                  <span className="font-mono text-white/80">
                    ({p.x}, {p.y}, {p.z})
                  </span>
                  <span>Rotated</span>
                  <span className="font-mono text-white/80">{p.rotated ? 'Yes' : 'No'}</span>
                  {p.estimated && (
                    <span className="col-span-2 text-amber-400">⚠ Dims estimated from weight</span>
                  )}
                </div>
              </motion.div>
            );
          })()}
        </AnimatePresence>
      </div>

      {/* ── Centre — 3D viewer ─────────────────────────────────────────── */}
      <div className="flex-1 overflow-hidden rounded-2xl border border-white/10">
        {selectedSpec ? (
          <PackingCanvas
            spec={selectedSpec}
            placements={placements}
            selectedAwb={selectedAwb}
            onSelect={setSelectedAwb}
          />
        ) : (
          <div className="flex h-full items-center justify-center">
            <div className="text-center text-white/30">
              <Truck size={48} className="mx-auto mb-3 opacity-30" />
              <p className="text-sm">Select a vehicle spec to view the 3D layout</p>
            </div>
          </div>
        )}
      </div>

      {/* ── Right panel — items list + unplaced + scan feed ───────────── */}
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
              <p className="text-xs text-white/30 px-2">No items placed yet</p>
            )}
          </div>
        </div>

        {/* Unplaced items */}
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

        {/* Live scan feed */}
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
  );
}
