'use client';

import { useState, useEffect } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { X, Plus, Pencil, Trash2, Truck, Save, Loader2 } from 'lucide-react';
import { cn } from '@/lib/utils';
import type { TruckSpec, CreateSpecBody, UpdateSpecBody } from '@/lib/api/consolidation';

// ── Constants ─────────────────────────────────────────────────────────────────

const TRANSPORT_MODES = [
  { value: 'road', label: 'Road' },
  { value: 'sea',  label: 'Sea' },
  { value: 'air',  label: 'Air' },
] as const;

const SIZE_CLASSES = [
  'Van', 'L300', '6Wheeler', '10Wheeler', 'Trailer', 'FCL20', 'FCL40',
] as const;

// Standard PH freight specs — used to pre-fill the form for quick setup.
const PRESETS: Record<string, Omit<CreateSpecBody, 'name'>> = {
  'L300 Van':     { transport_mode: 'road', size_class: 'L300',      interior_length_cm: 280,  interior_width_cm: 170,  interior_height_cm: 140,  max_payload_kg: 1500  },
  '6-Wheeler':    { transport_mode: 'road', size_class: '6Wheeler',   interior_length_cm: 520,  interior_width_cm: 220,  interior_height_cm: 200,  max_payload_kg: 5000  },
  '10-Wheeler':   { transport_mode: 'road', size_class: '10Wheeler',  interior_length_cm: 780,  interior_width_cm: 240,  interior_height_cm: 230,  max_payload_kg: 10000 },
  'Trailer':      { transport_mode: 'road', size_class: 'Trailer',    interior_length_cm: 1200, interior_width_cm: 240,  interior_height_cm: 260,  max_payload_kg: 22000 },
  'FCL 20-ft':    { transport_mode: 'sea',  size_class: 'FCL20',      interior_length_cm: 589,  interior_width_cm: 235,  interior_height_cm: 239,  max_payload_kg: 21700 },
  'FCL 40-ft':    { transport_mode: 'sea',  size_class: 'FCL40',      interior_length_cm: 1203, interior_width_cm: 235,  interior_height_cm: 239,  max_payload_kg: 26480 },
};

// ── Form ──────────────────────────────────────────────────────────────────────

interface FormState {
  name:               string;
  transport_mode:     'road' | 'sea' | 'air';
  size_class:         string;
  interior_length_cm: string;
  interior_width_cm:  string;
  interior_height_cm: string;
  max_payload_kg:     string;
}

const BLANK_FORM: FormState = {
  name: '', transport_mode: 'road', size_class: 'Van',
  interior_length_cm: '', interior_width_cm: '', interior_height_cm: '', max_payload_kg: '',
};

function specToForm(s: TruckSpec): FormState {
  return {
    name:               s.name,
    transport_mode:     s.transport_mode as 'road' | 'sea' | 'air',
    size_class:         s.size_class,
    interior_length_cm: String(s.interior_length_cm),
    interior_width_cm:  String(s.interior_width_cm),
    interior_height_cm: String(s.interior_height_cm),
    max_payload_kg:     String(s.max_payload_kg),
  };
}

function formToCreate(f: FormState): CreateSpecBody {
  return {
    name:               f.name.trim(),
    transport_mode:     f.transport_mode,
    size_class:         f.size_class,
    interior_length_cm: parseInt(f.interior_length_cm, 10),
    interior_width_cm:  parseInt(f.interior_width_cm, 10),
    interior_height_cm: parseInt(f.interior_height_cm, 10),
    max_payload_kg:     parseFloat(f.max_payload_kg),
  };
}

function Field({
  label, value, onChange, type = 'text', suffix,
}: {
  label: string; value: string;
  onChange: (v: string) => void;
  type?: string; suffix?: string;
}) {
  return (
    <div className="flex flex-col gap-1">
      <label className="text-[11px] uppercase tracking-wider text-white/40">{label}</label>
      <div className="relative flex items-center">
        <input
          type={type}
          value={value}
          onChange={e => onChange(e.target.value)}
          className="w-full rounded-lg border border-white/10 bg-white/5 px-3 py-2 text-sm text-white placeholder-white/20 focus:border-cyan-500/60 focus:outline-none"
        />
        {suffix && (
          <span className="absolute right-3 text-xs text-white/30 pointer-events-none">{suffix}</span>
        )}
      </div>
    </div>
  );
}

// ── Spec row ──────────────────────────────────────────────────────────────────

function SpecRow({
  spec, onEdit, onToggle,
}: {
  spec: TruckSpec;
  onEdit: () => void;
  onToggle: () => void;
}) {
  const vol = Math.round(
    spec.interior_length_cm * spec.interior_width_cm * spec.interior_height_cm / 1_000_000
  );
  return (
    <div className={cn(
      'flex items-center justify-between gap-3 rounded-xl border px-4 py-3 transition-all',
      spec.is_active
        ? 'border-white/10 bg-white/5'
        : 'border-white/5 bg-white/[0.02] opacity-50',
    )}>
      <div className="min-w-0">
        <div className="flex items-center gap-2">
          <Truck size={12} className="text-cyan-500 shrink-0" />
          <span className="text-sm font-semibold text-white truncate">{spec.name}</span>
          <span className={cn(
            'rounded-sm px-1.5 py-0.5 text-[10px] font-mono uppercase',
            spec.transport_mode === 'sea' ? 'bg-blue-500/20 text-blue-400' :
            spec.transport_mode === 'air' ? 'bg-purple-500/20 text-purple-400' :
            'bg-green-500/20 text-green-400',
          )}>
            {spec.transport_mode}
          </span>
        </div>
        <div className="mt-1 text-[11px] font-mono text-white/40">
          {spec.interior_length_cm}×{spec.interior_width_cm}×{spec.interior_height_cm} cm
          &nbsp;·&nbsp;{vol} m³
          &nbsp;·&nbsp;{spec.max_payload_kg.toLocaleString()} kg max
        </div>
      </div>
      <div className="flex shrink-0 items-center gap-1">
        <button
          onClick={onEdit}
          className="rounded-lg p-1.5 text-white/40 hover:bg-white/10 hover:text-white transition-colors"
          title="Edit"
        >
          <Pencil size={13} />
        </button>
        <button
          onClick={onToggle}
          className={cn(
            'rounded-lg px-2 py-1 text-[11px] font-mono transition-colors',
            spec.is_active
              ? 'text-white/30 hover:text-red-400 hover:bg-red-500/10'
              : 'text-green-400 hover:bg-green-500/10',
          )}
        >
          {spec.is_active ? 'Disable' : 'Enable'}
        </button>
      </div>
    </div>
  );
}

// ── Main modal ────────────────────────────────────────────────────────────────

export interface TruckSpecModalProps {
  specs:         TruckSpec[];
  onClose:       () => void;
  onCreate:      (body: CreateSpecBody)          => Promise<void>;
  onUpdate:      (id: string, body: UpdateSpecBody) => Promise<void>;
}

export default function TruckSpecModal({
  specs, onClose, onCreate, onUpdate,
}: TruckSpecModalProps) {
  const [editing,  setEditing]  = useState<TruckSpec | null>(null);
  const [adding,   setAdding]   = useState(false);
  const [form,     setForm]     = useState<FormState>(BLANK_FORM);
  const [saving,   setSaving]   = useState(false);
  const [error,    setError]    = useState<string | null>(null);

  function openAdd() {
    setEditing(null);
    setForm(BLANK_FORM);
    setError(null);
    setAdding(true);
  }

  function openEdit(spec: TruckSpec) {
    setAdding(false);
    setForm(specToForm(spec));
    setError(null);
    setEditing(spec);
  }

  function applyPreset(key: string) {
    const preset = PRESETS[key];
    if (!preset) return;
    setForm(f => ({
      ...f,
      name:               f.name || key,
      transport_mode:     preset.transport_mode,
      size_class:         preset.size_class,
      interior_length_cm: String(preset.interior_length_cm),
      interior_width_cm:  String(preset.interior_width_cm),
      interior_height_cm: String(preset.interior_height_cm),
      max_payload_kg:     String(preset.max_payload_kg),
    }));
  }

  function setField(key: keyof FormState) {
    return (v: string) => setForm(f => ({ ...f, [key]: v }));
  }

  async function handleSave() {
    setError(null);
    if (!form.name.trim()) { setError('Name is required.'); return; }
    const l = parseInt(form.interior_length_cm, 10);
    const w = parseInt(form.interior_width_cm,  10);
    const h = parseInt(form.interior_height_cm, 10);
    const kg = parseFloat(form.max_payload_kg);
    if ([l, w, h].some(v => isNaN(v) || v <= 0)) { setError('All dimensions must be positive integers.'); return; }
    if (isNaN(kg) || kg <= 0) { setError('Max payload must be a positive number.'); return; }

    setSaving(true);
    try {
      if (editing) {
        await onUpdate(editing.id, {
          name: form.name.trim(),
          interior_length_cm: l, interior_width_cm: w, interior_height_cm: h,
          max_payload_kg: kg,
        });
      } else {
        await onCreate(formToCreate(form));
      }
      setAdding(false);
      setEditing(null);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : 'Save failed — try again.');
    } finally {
      setSaving(false);
    }
  }

  const showForm = adding || editing !== null;

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center p-4"
      onClick={onClose}
    >
      <div className="absolute inset-0 bg-black/60 backdrop-blur-sm" />

      <motion.div
        initial={{ opacity: 0, scale: 0.95, y: 12 }}
        animate={{ opacity: 1, scale: 1,    y: 0 }}
        exit={{ opacity: 0, scale: 0.95, y: 12 }}
        transition={{ type: 'spring', damping: 26, stiffness: 300 }}
        className="relative z-10 w-full max-w-xl max-h-[85vh] flex flex-col rounded-2xl border border-white/10 bg-[#0d1422] shadow-2xl overflow-hidden"
        onClick={e => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-center justify-between border-b border-white/10 px-6 py-4">
          <div className="flex items-center gap-2">
            <Truck size={16} className="text-cyan-400" />
            <h2 className="text-base font-bold text-white">Vehicle Specs</h2>
            <span className="rounded-full bg-white/10 px-2 py-0.5 text-xs font-mono text-white/50">
              {specs.length}
            </span>
          </div>
          <div className="flex items-center gap-2">
            {!showForm && (
              <button
                onClick={openAdd}
                className="flex items-center gap-1.5 rounded-lg border border-cyan-500/40 bg-cyan-500/10 px-3 py-1.5 text-xs font-semibold text-cyan-300 hover:bg-cyan-500/20 transition-colors"
              >
                <Plus size={12} />
                Add Spec
              </button>
            )}
            <button
              onClick={onClose}
              className="rounded-lg p-1.5 text-white/40 hover:bg-white/10 hover:text-white transition-colors"
            >
              <X size={14} />
            </button>
          </div>
        </div>

        {/* Body */}
        <div className="flex-1 overflow-y-auto">
          <AnimatePresence mode="wait">
            {showForm ? (
              <motion.div
                key="form"
                initial={{ opacity: 0, x: 16 }}
                animate={{ opacity: 1, x: 0 }}
                exit={{ opacity: 0, x: -16 }}
                className="p-6 space-y-4"
              >
                {/* Preset picker */}
                <div>
                  <label className="text-[11px] uppercase tracking-wider text-white/40 mb-2 block">
                    Quick-fill from standard spec
                  </label>
                  <div className="flex flex-wrap gap-2">
                    {Object.keys(PRESETS).map(k => (
                      <button
                        key={k}
                        onClick={() => applyPreset(k)}
                        className="rounded-lg border border-white/10 bg-white/5 px-2.5 py-1 text-xs text-white/60 hover:border-cyan-500/40 hover:text-cyan-300 transition-colors"
                      >
                        {k}
                      </button>
                    ))}
                  </div>
                </div>

                <Field label="Spec name" value={form.name} onChange={setField('name')} />

                <div className="grid grid-cols-2 gap-3">
                  <div className="flex flex-col gap-1">
                    <label className="text-[11px] uppercase tracking-wider text-white/40">
                      Transport mode
                    </label>
                    <select
                      value={form.transport_mode}
                      onChange={e => setField('transport_mode')(e.target.value)}
                      className="rounded-lg border border-white/10 bg-white/5 px-3 py-2 text-sm text-white focus:border-cyan-500/60 focus:outline-none"
                    >
                      {TRANSPORT_MODES.map(m => (
                        <option key={m.value} value={m.value} className="bg-gray-900">{m.label}</option>
                      ))}
                    </select>
                  </div>

                  <div className="flex flex-col gap-1">
                    <label className="text-[11px] uppercase tracking-wider text-white/40">
                      Size class
                    </label>
                    <select
                      value={form.size_class}
                      onChange={e => setField('size_class')(e.target.value)}
                      className="rounded-lg border border-white/10 bg-white/5 px-3 py-2 text-sm text-white focus:border-cyan-500/60 focus:outline-none"
                    >
                      {SIZE_CLASSES.map(s => (
                        <option key={s} value={s} className="bg-gray-900">{s}</option>
                      ))}
                    </select>
                  </div>
                </div>

                <div className="grid grid-cols-3 gap-3">
                  <Field label="Length (cm)" value={form.interior_length_cm}
                    onChange={setField('interior_length_cm')} type="number" suffix="cm" />
                  <Field label="Width (cm)"  value={form.interior_width_cm}
                    onChange={setField('interior_width_cm')}  type="number" suffix="cm" />
                  <Field label="Height (cm)" value={form.interior_height_cm}
                    onChange={setField('interior_height_cm')} type="number" suffix="cm" />
                </div>

                <Field label="Max payload" value={form.max_payload_kg}
                  onChange={setField('max_payload_kg')} type="number" suffix="kg" />

                {error && (
                  <p className="text-xs text-red-400 bg-red-500/10 rounded-lg px-3 py-2">{error}</p>
                )}

                <div className="flex justify-end gap-2 pt-2">
                  <button
                    onClick={() => { setAdding(false); setEditing(null); }}
                    className="rounded-lg border border-white/10 px-4 py-2 text-sm text-white/50 hover:bg-white/5 transition-colors"
                  >
                    Cancel
                  </button>
                  <button
                    onClick={handleSave}
                    disabled={saving}
                    className="flex items-center gap-2 rounded-lg border border-cyan-500/50 bg-cyan-500/10 px-4 py-2 text-sm font-semibold text-cyan-300 hover:bg-cyan-500/20 disabled:opacity-40 transition-colors"
                  >
                    {saving ? <Loader2 size={14} className="animate-spin" /> : <Save size={14} />}
                    {editing ? 'Save Changes' : 'Create Spec'}
                  </button>
                </div>
              </motion.div>
            ) : (
              <motion.div
                key="list"
                initial={{ opacity: 0 }}
                animate={{ opacity: 1 }}
                className="p-6 space-y-3"
              >
                {specs.length === 0 && (
                  <div className="flex flex-col items-center gap-3 py-8 text-center">
                    <Truck size={32} className="text-white/20" />
                    <p className="text-sm text-white/30">No vehicle specs yet.</p>
                    <button
                      onClick={openAdd}
                      className="text-xs text-cyan-400 hover:underline"
                    >
                      Add your first spec →
                    </button>
                  </div>
                )}
                {specs.map(s => (
                  <SpecRow
                    key={s.id}
                    spec={s}
                    onEdit={() => openEdit(s)}
                    onToggle={() => onUpdate(s.id, { is_active: !s.is_active })}
                  />
                ))}
              </motion.div>
            )}
          </AnimatePresence>
        </div>
      </motion.div>
    </div>
  );
}
