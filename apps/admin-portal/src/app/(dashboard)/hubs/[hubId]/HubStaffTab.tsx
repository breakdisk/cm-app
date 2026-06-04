'use client';

import { useCallback, useEffect, useState } from 'react';
import { motion } from 'framer-motion';
import { UserCheck, UserMinus, Search, X } from 'lucide-react';
import { GlassCard } from '@/components/ui/glass-card';
import { variants } from '@/lib/design-system/tokens';
import { createHubStaffApi, type HubDriver } from '@/lib/api/hub-staff';

interface Props {
  hubId: string;
}

export default function HubStaffTab({ hubId }: Props) {
  const [scanners,    setScanners]    = useState<HubDriver[]>([]);
  const [loading,     setLoading]     = useState(true);
  const [actionError, setActionError] = useState<string | null>(null);

  // Search modal state
  const [showSearch,   setShowSearch]   = useState(false);
  const [searchQuery,  setSearchQuery]  = useState('');
  const [searchResult, setSearchResult] = useState<HubDriver[]>([]);
  const [searching,    setSearching]    = useState(false);
  const [assigning,    setAssigning]    = useState<string | null>(null);
  const [removing,     setRemoving]     = useState<string | null>(null);

  const api = createHubStaffApi();

  const load = useCallback(async () => {
    setLoading(true);
    try {
      setScanners(await api.listHubScanners(hubId));
    } catch {
      setScanners([]);
    } finally {
      setLoading(false);
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [hubId]);

  useEffect(() => { load(); }, [load]);

  async function handleSearch(q: string) {
    setSearchQuery(q);
    if (q.length < 2) { setSearchResult([]); return; }
    setSearching(true);
    try {
      const results = await api.searchDrivers(q);
      const assignedIds = new Set(scanners.map(s => s.id));
      setSearchResult(results.filter(d => !assignedIds.has(d.id)));
    } catch {
      setSearchResult([]);
    } finally {
      setSearching(false);
    }
  }

  async function handleAssign(driver: HubDriver) {
    setAssigning(driver.id);
    setActionError(null);
    try {
      await api.assignHubScanner(driver.id, driver.user_id, hubId);
      setShowSearch(false);
      setSearchQuery('');
      setSearchResult([]);
      await load();
    } catch (err: unknown) {
      setActionError((err as { message?: string })?.message ?? 'Assignment failed');
    } finally {
      setAssigning(null);
    }
  }

  async function handleRemove(driver: HubDriver) {
    setRemoving(driver.id);
    setActionError(null);
    try {
      await api.removeHubScanner(driver.id, driver.user_id);
      await load();
    } catch (err: unknown) {
      setActionError((err as { message?: string })?.message ?? 'Removal failed');
    } finally {
      setRemoving(null);
    }
  }

  return (
    <motion.div
      key="hub-staff"
      variants={variants.staggerContainer}
      initial="hidden"
      animate="visible"
      className="h-full overflow-y-auto space-y-4 pb-6"
    >
      {/* Header row */}
      <motion.div variants={variants.fadeInUp} className="flex items-center justify-between">
        <div>
          <h2 className="font-heading text-base font-semibold text-white">Hub Scanners</h2>
          <p className="text-xs font-mono text-white/40 mt-0.5">
            Drivers assigned to this hub with hub_scanner role
          </p>
        </div>
        <button
          onClick={() => setShowSearch(true)}
          className="flex items-center gap-2 rounded-lg border border-cyan-neon/30 bg-cyan-neon/5 px-3 py-1.5 text-xs font-mono text-cyan-neon hover:bg-cyan-neon/10 transition-all"
        >
          <UserCheck size={12} /> Assign Driver
        </button>
      </motion.div>

      {/* Error banner */}
      {actionError && (
        <div className="rounded-lg border border-red-signal/30 bg-red-signal/10 px-4 py-2 text-xs font-mono text-red-signal flex items-center justify-between">
          {actionError}
          <button onClick={() => setActionError(null)}><X size={12} /></button>
        </div>
      )}

      {/* Scanners table */}
      <motion.div variants={variants.fadeInUp}>
        <GlassCard>
          {loading ? (
            <div className="flex items-center justify-center py-12">
              <div className="h-6 w-6 animate-spin rounded-full border-2 border-cyan-neon border-t-transparent" />
            </div>
          ) : scanners.length === 0 ? (
            <p className="py-10 text-center text-sm font-mono text-white/30">
              No hub scanners assigned yet
            </p>
          ) : (
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-white/[0.06] text-left">
                  <th className="pb-2 pr-4 text-xs font-mono font-medium text-white/40">Name</th>
                  <th className="pb-2 pr-4 text-xs font-mono font-medium text-white/40">Phone</th>
                  <th className="pb-2 pr-4 text-xs font-mono font-medium text-white/40">Status</th>
                  <th className="pb-2 text-xs font-mono font-medium text-white/40" />
                </tr>
              </thead>
              <tbody>
                {scanners.map((d) => (
                  <tr key={d.id} className="border-b border-white/[0.03] last:border-0">
                    <td className="py-2.5 pr-4 font-medium text-white">
                      {d.first_name} {d.last_name}
                    </td>
                    <td className="py-2.5 pr-4 font-mono text-xs text-white/60">{d.phone}</td>
                    <td className="py-2.5 pr-4">
                      <span className={`rounded-full px-2 py-0.5 text-xs font-mono font-medium ${
                        d.status === 'available' ? 'bg-green-signal/15 text-green-signal' :
                        d.status === 'offline'   ? 'bg-white/5 text-white/40' :
                                                   'bg-cyan-neon/10 text-cyan-neon'
                      }`}>
                        {d.status}
                      </span>
                    </td>
                    <td className="py-2.5 text-right">
                      <button
                        onClick={() => handleRemove(d)}
                        disabled={removing === d.id}
                        className="flex items-center gap-1.5 rounded-md border border-red-signal/20 bg-red-signal/5 px-2.5 py-1 text-xs font-mono text-red-signal hover:bg-red-signal/10 disabled:opacity-40 transition-all ml-auto"
                      >
                        {removing === d.id
                          ? <span className="h-3 w-3 animate-spin rounded-full border border-red-signal border-t-transparent" />
                          : <UserMinus size={11} />}
                        Remove
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </GlassCard>
      </motion.div>

      {/* Search / Assign modal */}
      {showSearch && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
          <div className="w-full max-w-md rounded-2xl border border-white/[0.08] bg-[#0A0E1A] p-5 shadow-2xl">
            <div className="mb-4 flex items-center justify-between">
              <h3 className="font-heading text-sm font-semibold text-white">Assign Hub Scanner</h3>
              <button
                onClick={() => { setShowSearch(false); setSearchQuery(''); setSearchResult([]); }}
                className="text-white/40 hover:text-white/70"
              >
                <X size={16} />
              </button>
            </div>
            <div className="relative mb-3">
              <Search size={13} className="absolute left-3 top-1/2 -translate-y-1/2 text-white/30" />
              <input
                autoFocus
                type="text"
                placeholder="Search by name or phone…"
                value={searchQuery}
                onChange={(e) => handleSearch(e.target.value)}
                className="w-full rounded-lg border border-white/[0.1] bg-white/[0.05] pl-8 pr-3 py-2 text-xs font-mono text-white placeholder:text-white/30 focus:border-cyan-neon/40 focus:outline-none"
              />
            </div>
            <div className="max-h-56 overflow-y-auto space-y-1">
              {searching && (
                <p className="py-4 text-center text-xs font-mono text-white/40">Searching…</p>
              )}
              {!searching && searchResult.length === 0 && searchQuery.length >= 2 && (
                <p className="py-4 text-center text-xs font-mono text-white/40">No results</p>
              )}
              {searchResult.map((d) => (
                <button
                  key={d.id}
                  onClick={() => handleAssign(d)}
                  disabled={assigning === d.id}
                  className="w-full flex items-center justify-between rounded-lg border border-white/[0.05] bg-white/[0.02] px-3 py-2 text-left hover:bg-white/[0.05] disabled:opacity-40 transition-all"
                >
                  <div>
                    <p className="text-xs font-medium text-white">{d.first_name} {d.last_name}</p>
                    <p className="text-xs font-mono text-white/40">{d.phone}</p>
                  </div>
                  {assigning === d.id
                    ? <span className="h-4 w-4 animate-spin rounded-full border border-cyan-neon border-t-transparent" />
                    : <UserCheck size={13} className="text-cyan-neon" />}
                </button>
              ))}
            </div>
          </div>
        </div>
      )}
    </motion.div>
  );
}
