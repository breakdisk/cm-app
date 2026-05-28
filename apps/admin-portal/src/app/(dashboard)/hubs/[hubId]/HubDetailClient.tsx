'use client';

import { useEffect, useMemo, useState } from 'react';
import Link from 'next/link';
import { useRouter } from 'next/navigation';
import { motion, AnimatePresence } from 'framer-motion';
import {
  Building2, ChevronLeft, MapPin, Layers,
  Package, Boxes, AlertTriangle, RefreshCw,
  FileText, ArrowRight,
} from 'lucide-react';
import { GlassCard } from '@/components/ui/glass-card';
import { NeonBadge } from '@/components/ui/neon-badge';
import { LiveMetric } from '@/components/ui/live-metric';
import { variants } from '@/lib/design-system/tokens';
import { cn } from '@/lib/design-system/cn';
import {
  createHubsApi, hubIdOf, hubUtilization, hubStatusTier,
  type Hub, type HubStatusTier, type HubManifest,
} from '@/lib/api/hubs';
import ConsolidationPageClient from './consolidation/ConsolidationPageClient';

// ── Tab types ─────────────────────────────────────────────────────────────────

const HUB_TABS = ['overview', 'plan-load'] as const;
type HubTab = (typeof HUB_TABS)[number];

const TAB_LABELS: Record<HubTab, string> = {
  'overview':   'Overview',
  'plan-load':  'Plan Load (3D)',
};

// ── Status config ─────────────────────────────────────────────────────────────

const STATUS_CONFIG: Record<HubStatusTier, {
  label: string; variant: 'green' | 'amber' | 'red'; color: string;
}> = {
  normal:   { label: 'Normal',   variant: 'green', color: '#00FF88' },
  high:     { label: 'High',     variant: 'amber', color: '#FFAB00' },
  critical: { label: 'Critical', variant: 'red',   color: '#FF3B5C' },
};

// ── Overview tab ──────────────────────────────────────────────────────────────

function OverviewTab({ hub, manifest, manifestLoading, onGoToPlanLoad }: {
  hub:             Hub;
  manifest:        HubManifest | null;
  manifestLoading: boolean;
  onGoToPlanLoad:  () => void;
}) {
  const id         = hubIdOf(hub);
  const tier       = hubStatusTier(hub);
  const { label, variant, color } = STATUS_CONFIG[tier];
  const capacityPct = Math.round(hubUtilization(hub));

  return (
    <motion.div
      key="overview"
      variants={variants.staggerContainer}
      initial="hidden"
      animate="visible"
      className="h-full overflow-y-auto space-y-5 pb-6"
    >
      {/* Top KPI row */}
      <motion.div variants={variants.fadeInUp} className="grid grid-cols-2 gap-3 md:grid-cols-4">
        <GlassCard size="sm" glow="cyan" accent>
          <LiveMetric label="Parcels In Hub" value={hub.current_load} trend={0} color="cyan" format="number" />
        </GlassCard>
        <GlassCard size="sm" glow="green" accent>
          <LiveMetric label="Total Capacity" value={hub.capacity} trend={0} color="green" format="number" />
        </GlassCard>
        <GlassCard size="sm" glow="purple" accent>
          <LiveMetric label="Utilization" value={capacityPct} trend={0} color="purple" format="number" />
        </GlassCard>
        <GlassCard size="sm" glow={tier === 'critical' ? 'red' : 'green'} accent>
          <LiveMetric
            label="Status"
            value={hub.is_active ? 1 : 0}
            trend={0}
            color={hub.is_active ? 'green' : 'red'}
            format="number"
          />
        </GlassCard>
      </motion.div>

      <div className="grid grid-cols-1 gap-5 md:grid-cols-2">
        {/* Capacity card */}
        <motion.div variants={variants.fadeInUp}>
          <GlassCard glow={tier === 'critical' ? 'red' : tier === 'high' ? 'amber' : undefined}>
            <div className="flex items-center justify-between mb-4">
              <p className="text-sm font-semibold text-white">Capacity</p>
              <NeonBadge variant={variant} dot={tier !== 'normal'}>{label}</NeonBadge>
            </div>

            <div className="flex items-end justify-between mb-3">
              <span className="text-3xl font-bold font-mono text-white">
                {hub.current_load.toLocaleString()}
                <span className="text-base text-white/30 ml-1">/ {hub.capacity.toLocaleString()}</span>
              </span>
              <span className="text-xl font-mono font-black" style={{ color }}>{capacityPct}%</span>
            </div>

            <div className="h-3 rounded-full bg-glass-300 overflow-hidden mb-3">
              <motion.div
                className="h-full rounded-full"
                initial={{ width: 0 }}
                animate={{ width: `${Math.min(capacityPct, 100)}%` }}
                transition={{ duration: 0.7, ease: 'easeOut' }}
                style={{ background: color }}
              />
            </div>

            {tier === 'critical' && (
              <p className="text-xs font-mono text-red-signal flex items-center gap-1.5 mt-2">
                <AlertTriangle size={11} /> Near capacity — consider redistributing parcels
              </p>
            )}

            <div className="grid grid-cols-2 gap-3 mt-4 pt-4 border-t border-glass-border">
              <div>
                <p className="text-2xs font-mono text-white/30 mb-0.5">Available</p>
                <p className="text-sm font-mono font-semibold text-white">
                  {(hub.capacity - hub.current_load).toLocaleString()}
                </p>
              </div>
              <div>
                <p className="text-2xs font-mono text-white/30 mb-0.5">Operational</p>
                <p className={`text-sm font-mono font-semibold ${hub.is_active ? 'text-green-signal' : 'text-white/40'}`}>
                  {hub.is_active ? 'Active' : 'Inactive'}
                </p>
              </div>
            </div>
          </GlassCard>
        </motion.div>

        {/* Hub info + location */}
        <motion.div variants={variants.fadeInUp} className="space-y-4">
          <GlassCard>
            <p className="text-2xs font-mono text-white/40 uppercase tracking-widest mb-3">Location</p>
            <div className="flex items-start gap-2 text-sm text-white/70">
              <MapPin size={13} className="text-white/30 mt-0.5 shrink-0" />
              <span className="font-mono text-xs leading-relaxed">{hub.address}</span>
            </div>
            <div className="mt-3 pt-3 border-t border-glass-border">
              <p className="text-2xs font-mono text-white/30">Hub ID</p>
              <p className="text-xs font-mono text-white/50 mt-0.5 break-all">{id}</p>
            </div>
          </GlassCard>

          {/* Manifest quick summary */}
          <GlassCard>
            <div className="flex items-center justify-between mb-3">
              <div className="flex items-center gap-2">
                <FileText size={13} className="text-white/40" />
                <p className="text-2xs font-mono text-white/40 uppercase tracking-widest">
                  Today&apos;s Manifest
                </p>
              </div>
              {manifestLoading && (
                <RefreshCw size={11} className="text-white/30 animate-spin" />
              )}
            </div>
            <p className="text-3xl font-bold font-mono text-white">
              {manifest ? manifest.count : '—'}
              <span className="text-sm text-white/30 ml-1">parcels</span>
            </p>
            <button
              onClick={onGoToPlanLoad}
              className="mt-4 flex w-full items-center justify-between rounded-xl border border-cyan-neon/30 bg-cyan-neon/5 px-4 py-3 text-sm font-semibold text-cyan-neon hover:border-cyan-neon/60 hover:bg-cyan-neon/10 transition-all"
            >
              <span className="flex items-center gap-2">
                <Boxes size={14} /> Plan Load (3D)
              </span>
              <div className="flex items-center gap-2">
                {manifest && manifest.count > 0 && (
                  <span className="font-mono text-xs text-cyan-neon/60">
                    {manifest.count} parcels
                  </span>
                )}
                <ArrowRight size={14} />
              </div>
            </button>
          </GlassCard>
        </motion.div>
      </div>

      {/* Serving zones */}
      {hub.serving_zones.length > 0 && (
        <motion.div variants={variants.fadeInUp}>
          <GlassCard>
            <div className="flex items-center gap-2 mb-4">
              <Layers size={13} className="text-white/40" />
              <p className="text-2xs font-mono text-white/40 uppercase tracking-widest">
                Serving Zones ({hub.serving_zones.length})
              </p>
            </div>
            <div className="flex flex-wrap gap-2">
              {hub.serving_zones.map((zone) => (
                <span
                  key={zone}
                  className="rounded-lg border border-glass-border bg-glass-200 px-3 py-1 text-xs font-mono text-white/70"
                >
                  {zone}
                </span>
              ))}
            </div>
          </GlassCard>
        </motion.div>
      )}
    </motion.div>
  );
}

// ── Main client component ─────────────────────────────────────────────────────

interface Props {
  hubId:      string;
  token:      string;
  initialTab: HubTab;
}

export default function HubDetailClient({ hubId, token, initialTab }: Props) {
  const router = useRouter();

  const [activeTab,       setActiveTab]       = useState<HubTab>(initialTab);
  const [hub,             setHub]             = useState<Hub | null>(null);
  const [manifest,        setManifest]        = useState<HubManifest | null>(null);
  const [loading,         setLoading]         = useState(true);
  const [loadError,       setLoadError]       = useState<string | null>(null);
  const [manifestLoading, setManifestLoading] = useState(false);

  const hubsApi = useMemo(() => createHubsApi(), []);

  // Load hub data + manifest in parallel.
  useEffect(() => {
    let cancelled = false;
    async function load() {
      setLoading(true);
      setLoadError(null);
      try {
        if (!cancelled) setManifestLoading(true);
        const [hubData] = await Promise.all([
          hubsApi.get(hubId),
          hubsApi.manifest(hubId)
            .then((m) => { if (!cancelled) setManifest(m); })
            .catch(() => {/* manifest may be 404 if hub has no active parcels */})
            .finally(() => { if (!cancelled) setManifestLoading(false); }),
        ]);
        if (!cancelled) setHub(hubData);
      } catch (err: unknown) {
        if (cancelled) return;
        // The api client interceptor rejects with a flat { message, code, status }
        // shape — NOT Axios's default nested { response: { status } } shape.
        const status = (err as { status?: number })?.status;
        if (status === 404) {
          router.replace('/hubs');
        } else {
          const msg = (err as { message?: string })?.message;
          setLoadError(msg ?? 'Failed to load hub data. Please try again.');
        }
      } finally {
        if (!cancelled) setLoading(false);
      }
    }
    load();
    return () => { cancelled = true; };
  }, [hubId, hubsApi, router]);

  // Keep URL in sync with active tab without creating browser history entries.
  function switchTab(tab: HubTab) {
    setActiveTab(tab);
    const url = tab === 'overview' ? `/hubs/${hubId}` : `/hubs/${hubId}?tab=plan-load`;
    router.replace(url, { scroll: false });
  }

  if (loading) {
    return (
      <div className="flex h-[calc(100vh-8rem)] items-center justify-center">
        <div className="flex flex-col items-center gap-3">
          <div className="h-8 w-8 animate-spin rounded-full border-2 border-cyan-500 border-t-transparent" />
          <span className="text-sm text-white/40 font-mono">Loading hub…</span>
        </div>
      </div>
    );
  }

  if (loadError) {
    return (
      <div className="flex h-[calc(100vh-8rem)] items-center justify-center">
        <div className="flex flex-col items-center gap-4 text-center">
          <AlertTriangle size={32} className="text-red-signal" />
          <p className="text-sm text-white/60 font-mono">{loadError}</p>
          <button
            onClick={() => window.location.reload()}
            className="flex items-center gap-2 rounded-lg border border-cyan-neon/30 bg-cyan-neon/5 px-4 py-2 text-xs font-mono text-cyan-neon hover:bg-cyan-neon/10 transition-all"
          >
            <RefreshCw size={12} /> Retry
          </button>
        </div>
      </div>
    );
  }

  if (!hub) return null;

  const tier    = hubStatusTier(hub);
  const { label, variant } = STATUS_CONFIG[tier];

  return (
    // Outer container mirrors the dashboard header height (h-16 = 4rem).
    // We use flex-col so the tab bar is fixed-height and the content grows.
    <div className="flex h-[calc(100vh-6rem)] md:h-[calc(100vh-7rem)] flex-col">

      {/* ── Page header ──────────────────────────────────────────────── */}
      <div className="flex-shrink-0 pb-4">
        {/* Breadcrumb */}
        <div className="flex items-center gap-1.5 mb-3 text-xs text-white/40 font-mono">
          <Link href="/hubs" className="flex items-center gap-1 hover:text-white/70 transition-colors">
            <ChevronLeft size={12} />
            Hub Operations
          </Link>
          <span>/</span>
          <span className="text-white/60">{hub.name}</span>
        </div>

        {/* Hub title + status */}
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-3 min-w-0">
            <Building2 size={20} className="text-purple-plasma shrink-0" />
            <div className="min-w-0">
              <h1 className="font-heading text-xl font-bold text-white truncate">{hub.name}</h1>
              <p className="text-2xs font-mono text-white/30 mt-0.5">{hubIdOf(hub).slice(0, 16)}…</p>
            </div>
          </div>
          <NeonBadge variant={variant} dot={tier !== 'normal'}>{label}</NeonBadge>
        </div>
      </div>

      {/* ── Tab bar ───────────────────────────────────────────────────── */}
      <div className="flex-shrink-0 pb-4">
        <div className="flex gap-1 bg-white/[0.03] border border-white/[0.08] rounded-xl p-1 w-fit">
          {HUB_TABS.map((tab) => (
            <button
              key={tab}
              onClick={() => switchTab(tab)}
              className={cn(
                'flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-medium transition-all',
                activeTab === tab
                  ? 'bg-[#00E5FF]/10 text-[#00E5FF] border border-[#00E5FF]/20'
                  : 'text-white/40 hover:text-white/70',
              )}
            >
              {tab === 'plan-load' && <Boxes size={13} />}
              {tab === 'overview'  && <Package size={13} />}
              {TAB_LABELS[tab]}
            </button>
          ))}
        </div>
      </div>

      {/* ── Tab content ───────────────────────────────────────────────── */}
      {/* flex-1 + min-h-0 lets the tab area shrink/grow correctly inside
          the parent flex-col without overflowing the viewport.              */}
      <div className="flex-1 min-h-0">
        <AnimatePresence mode="wait">

          {activeTab === 'overview' && (
            <motion.div
              key="overview-tab"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              transition={{ duration: 0.15 }}
              className="h-full"
            >
              <OverviewTab
                hub={hub}
                manifest={manifest}
                manifestLoading={manifestLoading}
                onGoToPlanLoad={() => switchTab('plan-load')}
              />
            </motion.div>
          )}

          {activeTab === 'plan-load' && (
            <motion.div
              key="plan-load-tab"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              transition={{ duration: 0.15 }}
              className="h-full overflow-hidden rounded-2xl border border-white/[0.06]"
            >
              {/* ConsolidationPageClient fills this container via h-full */}
              <ConsolidationPageClient
                hubId={hubId}
                token={token}
                heightClass="h-full"
              />
            </motion.div>
          )}

        </AnimatePresence>
      </div>
    </div>
  );
}
