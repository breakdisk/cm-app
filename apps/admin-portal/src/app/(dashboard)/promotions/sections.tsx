"use client";
/**
 * The four promotions sections. Each loads its own data and saves through the
 * promotions API; the server enforces every rule, and a refusal is shown in
 * its words.
 */
import { useCallback, useEffect, useState, type ReactNode } from "react";
import { Plus, Power, RefreshCw, Save, Search, Trash2 } from "lucide-react";
import { toast } from "sonner";

import { GlassCard } from "@/components/ui/glass-card";
import {
  amountToCents, bpsToPercent, centsToAmount, createCompany, createOffer, findCustomer, getAccountCredit,
  getLadder, grantCredit, ladderProblem, listCompanies, listOffers, offerSummary, percentToBps, saveLadder,
  setCompanyActive, setOfferActive,
  type AccountCredit, type AdminOffer, type CompanyRate, type CustomerUser, type TierRung,
} from "@/lib/api/promotions";

const input =
  "w-full min-w-0 rounded-lg border border-white/10 bg-white/[0.03] px-3 py-2 text-sm text-white placeholder:text-white/25 focus:border-cyan-400/50 focus:outline-none";
const primary =
  "inline-flex items-center justify-center gap-2 rounded-lg bg-cyan-400/90 px-3 py-2 text-xs font-medium text-[#041a1f] hover:bg-cyan-300 disabled:opacity-40";
const ghost =
  "inline-flex items-center justify-center gap-2 rounded-lg border border-white/10 px-3 py-1.5 text-xs text-white/70 hover:bg-white/5 disabled:opacity-40";

function message(e: unknown): string {
  return e instanceof Error ? e.message : "Something went wrong";
}

function Field({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  return (
    <label className="block min-w-0">
      <span className="mb-1 block text-[11px] uppercase tracking-wide text-white/40">{label}</span>
      {children}
      {hint && <span className="mt-1 block text-[11px] text-white/35">{hint}</span>}
    </label>
  );
}

function Pill({ on, children }: { on: boolean; children: ReactNode }) {
  return (
    <span className={`rounded-full px-2 py-0.5 text-[11px] font-medium ${on ? "bg-emerald-400/10 text-emerald-300" : "bg-white/5 text-white/40"}`}>
      {children}
    </span>
  );
}

function useLoad<T>(load: () => Promise<T>) {
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<string | null>(null);
  const reload = useCallback(async () => {
    try {
      setData(await load());
      setError(null);
    } catch (e) {
      setError(message(e));
    }
  }, [load]);
  useEffect(() => { void reload(); }, [reload]);
  return { data, error, reload, setData };
}

function LoadState({ error, loading }: { error: string | null; loading: boolean }) {
  if (error) return <GlassCard className="p-4"><p className="text-sm text-rose-300">{error}</p></GlassCard>;
  if (loading) return <GlassCard className="p-6 text-center"><p className="text-sm text-white/40">Loading…</p></GlassCard>;
  return null;
}

// ── Offers ───────────────────────────────────────────────────────────────────

export function OffersSection() {
  const { data: offers, error, reload } = useLoad(listOffers);
  const [busy, setBusy] = useState<string | null>(null);
  const [form, setForm] = useState({
    code: "", title: "", body: "", kind: "percent_carriage" as "percent_carriage" | "flat",
    percent: "20", flat: "", cap: "", windowed: true, once: false, ends: "", budget: "marketing",
  });
  const [saving, setSaving] = useState(false);

  async function toggle(o: AdminOffer) {
    setBusy(o.id);
    try {
      await setOfferActive(o.id, !o.active);
      toast.success(`${o.code} ${o.active ? "switched off" : "switched on"}`);
      await reload();
    } catch (e) {
      toast.error(message(e));
    } finally {
      setBusy(null);
    }
  }

  async function create() {
    const percent_bps = form.kind === "percent_carriage" ? percentToBps(form.percent) : undefined;
    const flat_cents = form.kind === "flat" ? amountToCents(form.flat) : undefined;
    const cap_cents = form.cap.trim() ? amountToCents(form.cap) : null;
    if (percent_bps !== undefined && (Number.isNaN(percent_bps) || percent_bps <= 0)) return toast.error("The percentage is 0–100.");
    if (flat_cents !== undefined && (Number.isNaN(flat_cents) || flat_cents <= 0)) return toast.error("The amount off must be above zero.");
    if (cap_cents !== null && Number.isNaN(cap_cents)) return toast.error("The cap is an amount, like 250 or 250.00.");
    setSaving(true);
    try {
      const o = await createOffer({
        code: form.code, title: form.title, body: form.body, discount_kind: form.kind,
        percent_bps, flat_cents, cap_cents, windowed: form.windowed, once_per_account: form.once,
        ends_at: form.ends ? new Date(`${form.ends}T23:59:59`).toISOString() : null, budget_tag: form.budget,
      });
      toast.success(`${o.code} is live`);
      setForm((f) => ({ ...f, code: "", title: "", body: "" }));
      await reload();
    } catch (e) {
      toast.error(message(e));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="space-y-4">
      <GlassCard className="p-4 sm:p-5">
        <h2 className="font-heading text-sm font-semibold text-white">New promo code</h2>
        <p className="mt-1 text-xs text-white/45">
          Customers enter it on a move. Windowed codes follow the monthly window — weekdays in the middle of the month, one code a month per customer.
        </p>
        <div className="mt-4 grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
          <Field label="Code" hint="3–32 letters, digits or hyphens">
            <input className={input} value={form.code} onChange={(e) => setForm({ ...form, code: e.target.value.toUpperCase() })} placeholder="MOVE20" />
          </Field>
          <Field label="Title">
            <input className={input} value={form.title} onChange={(e) => setForm({ ...form, title: e.target.value })} placeholder="Autumn move-in" />
          </Field>
          <Field label="Budget">
            <select className={input} value={form.budget} onChange={(e) => setForm({ ...form, budget: e.target.value })}>
              <option value="marketing">Marketing</option>
              <option value="acquisition">Acquisition</option>
              <option value="loyalty">Loyalty</option>
            </select>
          </Field>
          <Field label="Discount">
            <select className={input} value={form.kind} onChange={(e) => setForm({ ...form, kind: e.target.value as typeof form.kind })}>
              <option value="percent_carriage">% off carriage</option>
              <option value="flat">Flat amount off</option>
            </select>
          </Field>
          {form.kind === "percent_carriage" ? (
            <Field label="Percent off carriage">
              <input className={input} inputMode="decimal" value={form.percent} onChange={(e) => setForm({ ...form, percent: e.target.value })} />
            </Field>
          ) : (
            <Field label="Amount off">
              <input className={input} inputMode="decimal" value={form.flat} onChange={(e) => setForm({ ...form, flat: e.target.value })} placeholder="150.00" />
            </Field>
          )}
          <Field label="Cap per move" hint="Blank: only the platform ceiling limits it">
            <input className={input} inputMode="decimal" value={form.cap} onChange={(e) => setForm({ ...form, cap: e.target.value })} placeholder="250.00" />
          </Field>
          <Field label="Ends (optional)">
            <input className={input} type="date" value={form.ends} onChange={(e) => setForm({ ...form, ends: e.target.value })} />
          </Field>
          <Field label="Shown to customers">
            <input className={input} value={form.body} onChange={(e) => setForm({ ...form, body: e.target.value })} placeholder="20% off your van, weekdays only" />
          </Field>
          <div className="flex flex-col justify-end gap-2 text-sm text-white/70">
            <label className="flex items-center gap-2">
              <input type="checkbox" checked={form.windowed} onChange={(e) => setForm({ ...form, windowed: e.target.checked })} />
              Follows the monthly window
            </label>
            <label className="flex items-center gap-2">
              <input type="checkbox" checked={form.once} onChange={(e) => setForm({ ...form, once: e.target.checked })} />
              Once per customer, ever
            </label>
          </div>
        </div>
        <div className="mt-4 flex justify-end">
          <button type="button" className={primary} disabled={saving || !form.code.trim() || !form.title.trim()} onClick={() => void create()}>
            <Plus className="h-3.5 w-3.5" />
            {saving ? "Creating…" : "Create code"}
          </button>
        </div>
      </GlassCard>

      <LoadState error={error} loading={offers === null && !error} />
      {offers && (
        <GlassCard padding="none">
          {offers.length === 0 ? (
            <p className="p-6 text-center text-sm text-white/45">No codes yet.</p>
          ) : (
            <div className="divide-y divide-white/5">
              {offers.map((o) => (
                <div key={o.id} className="flex flex-col gap-3 p-4 sm:flex-row sm:items-center sm:justify-between">
                  <div className="min-w-0">
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="font-mono text-sm font-semibold tracking-wide text-cyan-300">{o.code}</span>
                      <Pill on={o.active}>{o.active ? "live" : "off"}</Pill>
                      <span className="text-[11px] uppercase tracking-wide text-white/30">{o.budget_tag}</span>
                    </div>
                    <p className="mt-1 truncate text-sm text-white/80">{o.title}</p>
                    <p className="mt-0.5 text-xs text-white/45">
                      {offerSummary(o)}
                      {o.windowed ? " · monthly window" : " · any day"}
                      {o.once_per_account ? " · once per customer" : ""}
                      {o.ends_at ? ` · ends ${new Date(o.ends_at).toLocaleDateString()}` : ""}
                    </p>
                  </div>
                  <button type="button" className={ghost} disabled={busy === o.id} onClick={() => void toggle(o)}>
                    <Power className="h-3.5 w-3.5" />
                    {o.active ? "Switch off" : "Switch on"}
                  </button>
                </div>
              ))}
            </div>
          )}
        </GlassCard>
      )}
    </div>
  );
}

// ── Member tiers ─────────────────────────────────────────────────────────────

interface RungDraft { name: string; min_moves: string; perk: string; percent: string; cap: string; multiplier: string }

function toDraft(r: TierRung): RungDraft {
  return {
    name: r.name, min_moves: String(r.min_moves), perk: r.perk, percent: bpsToPercent(r.accessorial_bps),
    cap: r.cap_cents ? (r.cap_cents / 100).toFixed(2) : "", multiplier: String(r.referral_multiplier),
  };
}

function fromDraft(d: RungDraft): TierRung {
  return {
    name: d.name.trim(), min_moves: Number(d.min_moves), perk: d.perk.trim(),
    accessorial_bps: d.percent.trim() ? percentToBps(d.percent) : 0,
    cap_cents: d.cap.trim() ? amountToCents(d.cap) : 0,
    referral_multiplier: Number(d.multiplier) || 1,
  };
}

export function TiersSection() {
  const { data: ladder, error, reload } = useLoad(getLadder);
  const [drafts, setDrafts] = useState<RungDraft[] | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (ladder) setDrafts(ladder.map(toDraft));
  }, [ladder]);

  const rungs = (drafts ?? []).map(fromDraft);
  const problem = drafts ? ladderProblem(rungs) : null;

  function update(i: number, patch: Partial<RungDraft>) {
    setDrafts((ds) => ds?.map((d, j) => (j === i ? { ...d, ...patch } : d)) ?? null);
  }

  async function save() {
    if (problem) return toast.error(problem);
    setSaving(true);
    try {
      const saved = await saveLadder([...rungs].sort((a, b) => a.min_moves - b.min_moves));
      setDrafts(saved.map(toDraft));
      toast.success(saved.length === 0 ? "Tiers switched off" : "Ladder saved — it applies from the next quote");
    } catch (e) {
      toast.error(message(e));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="space-y-4">
      <GlassCard className="p-4 sm:p-5">
        <h2 className="font-heading text-sm font-semibold text-white">The ladder</h2>
        <p className="mt-1 text-xs text-white/45">
          Customers climb by completed moves in the last 12 months. A tier&apos;s discount comes off extras (helpers, packing), up to its cap per move, with no code needed. Moving up sends the customer a push.
        </p>
      </GlassCard>

      <LoadState error={error} loading={drafts === null && !error} />
      {drafts && (
        <GlassCard className="p-4 sm:p-5">
          {drafts.length === 0 && <p className="pb-4 text-sm text-white/45">No tiers. Add one to start a loyalty programme.</p>}
          <div className="space-y-3">
            {drafts.map((d, i) => (
              <div key={i} className="grid grid-cols-2 gap-3 rounded-xl border border-white/5 p-3 sm:grid-cols-3 lg:grid-cols-7">
                <Field label="Name"><input className={input} value={d.name} onChange={(e) => update(i, { name: e.target.value })} placeholder="Gold" /></Field>
                <Field label="From moves"><input className={input} inputMode="numeric" value={d.min_moves} onChange={(e) => update(i, { min_moves: e.target.value })} /></Field>
                <Field label="% off extras"><input className={input} inputMode="decimal" value={d.percent} onChange={(e) => update(i, { percent: e.target.value })} placeholder="10" /></Field>
                <Field label="Cap per move"><input className={input} inputMode="decimal" value={d.cap} onChange={(e) => update(i, { cap: e.target.value })} placeholder="500.00" /></Field>
                <Field label="Referral ×"><input className={input} inputMode="numeric" value={d.multiplier} onChange={(e) => update(i, { multiplier: e.target.value })} /></Field>
                <Field label="Perk text"><input className={input} value={d.perk} onChange={(e) => update(i, { perk: e.target.value })} placeholder="Priority support" /></Field>
                <div className="col-span-2 flex items-end sm:col-span-1">
                  <button type="button" className={ghost} onClick={() => setDrafts(drafts.filter((_, j) => j !== i))} aria-label={`Remove ${d.name || "tier"}`}>
                    <Trash2 className="h-3.5 w-3.5" />
                    Remove
                  </button>
                </div>
              </div>
            ))}
          </div>
          {problem && <p className="mt-3 text-xs text-amber-300">{problem}</p>}
          <div className="mt-4 flex flex-wrap justify-between gap-2">
            <button
              type="button"
              className={ghost}
              disabled={drafts.length >= 10}
              onClick={() => setDrafts([...drafts, { name: "", min_moves: String((rungs.at(-1)?.min_moves ?? 0) + 3), perk: "", percent: "", cap: "", multiplier: "1" }])}
            >
              <Plus className="h-3.5 w-3.5" />
              Add tier
            </button>
            <div className="flex gap-2">
              <button type="button" className={ghost} onClick={() => void reload()}><RefreshCw className="h-3.5 w-3.5" />Discard</button>
              <button type="button" className={primary} disabled={saving || !!problem} onClick={() => void save()}>
                <Save className="h-3.5 w-3.5" />
                {saving ? "Saving…" : "Save ladder"}
              </button>
            </div>
          </div>
        </GlassCard>
      )}
    </div>
  );
}

// ── Company rates ────────────────────────────────────────────────────────────

export function CompaniesSection() {
  const { data: companies, error, reload } = useLoad(listCompanies);
  const [form, setForm] = useState({ code: "", firm: "", percent: "10", domain: "" });
  const [saving, setSaving] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);

  async function create() {
    const percent_bps = percentToBps(form.percent);
    if (Number.isNaN(percent_bps) || percent_bps <= 0) return toast.error("The rate is 0–100% off carriage.");
    setSaving(true);
    try {
      const c = await createCompany({
        code: form.code, firm_name: form.firm, percent_bps,
        email_domain: form.domain.trim().replace(/^@/, "").toLowerCase() || null,
      });
      toast.success(`${c.firm_name} added`);
      setForm({ code: "", firm: "", percent: "10", domain: "" });
      await reload();
    } catch (e) {
      toast.error(message(e));
    } finally {
      setSaving(false);
    }
  }

  async function toggle(c: CompanyRate) {
    setBusy(c.id);
    try {
      await setCompanyActive(c.id, !c.active);
      toast.success(c.active ? `${c.firm_name} switched off — linked customers pay the normal rate from their next quote` : `${c.firm_name} switched on`);
      await reload();
    } catch (e) {
      toast.error(message(e));
    } finally {
      setBusy(null);
    }
  }

  return (
    <div className="space-y-4">
      <GlassCard className="p-4 sm:p-5">
        <h2 className="font-heading text-sm font-semibold text-white">New company rate</h2>
        <p className="mt-1 text-xs text-white/45">
          Staff link their account with the company code, or in one tap when their email is on the company&apos;s domain. The rate comes off carriage on every move; a promo code is used instead only when it takes off more.
        </p>
        <div className="mt-4 grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-4">
          <Field label="Company"><input className={input} value={form.firm} onChange={(e) => setForm({ ...form, firm: e.target.value })} placeholder="Acme Logistics" /></Field>
          <Field label="Company code" hint="Staff enter this in the app"><input className={input} value={form.code} onChange={(e) => setForm({ ...form, code: e.target.value.toUpperCase() })} placeholder="ACME-CORP" /></Field>
          <Field label="% off carriage"><input className={input} inputMode="decimal" value={form.percent} onChange={(e) => setForm({ ...form, percent: e.target.value })} /></Field>
          <Field label="Email domain (optional)" hint="Offers one-tap linking"><input className={input} value={form.domain} onChange={(e) => setForm({ ...form, domain: e.target.value })} placeholder="acme.com" /></Field>
        </div>
        <div className="mt-4 flex justify-end">
          <button type="button" className={primary} disabled={saving || !form.code.trim() || !form.firm.trim()} onClick={() => void create()}>
            <Plus className="h-3.5 w-3.5" />
            {saving ? "Adding…" : "Add company"}
          </button>
        </div>
      </GlassCard>

      <LoadState error={error} loading={companies === null && !error} />
      {companies && (
        <GlassCard padding="none">
          {companies.length === 0 ? (
            <p className="p-6 text-center text-sm text-white/45">No company rates yet.</p>
          ) : (
            <div className="divide-y divide-white/5">
              {companies.map((c) => (
                <div key={c.id} className="flex flex-col gap-3 p-4 sm:flex-row sm:items-center sm:justify-between">
                  <div className="min-w-0">
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="truncate font-medium text-white">{c.firm_name}</span>
                      <Pill on={c.active}>{c.active ? "live" : "off"}</Pill>
                    </div>
                    <p className="mt-1 text-xs text-white/45">
                      <span className="font-mono text-cyan-300/80">{c.code}</span> · {bpsToPercent(c.percent_bps)}% off carriage
                      {c.email_domain ? ` · @${c.email_domain}` : ""}
                    </p>
                  </div>
                  <button type="button" className={ghost} disabled={busy === c.id} onClick={() => void toggle(c)}>
                    <Power className="h-3.5 w-3.5" />
                    {c.active ? "Switch off" : "Switch on"}
                  </button>
                </div>
              ))}
            </div>
          )}
        </GlassCard>
      )}
    </div>
  );
}

// ── Account credit ───────────────────────────────────────────────────────────

const ENTRY_LABEL: Record<string, string> = {
  referral_reward: "Referral reward",
  grant: "Granted",
  applied: "Used on a move",
  returned: "Returned — move cancelled",
};

export function CreditSection() {
  const [query, setQuery] = useState("");
  const [finding, setFinding] = useState(false);
  const [customer, setCustomer] = useState<CustomerUser | null>(null);
  const [credit, setCredit] = useState<AccountCredit | null>(null);
  const [lookupError, setLookupError] = useState<string | null>(null);
  const [amount, setAmount] = useState("");
  const [note, setNote] = useState("");
  const [granting, setGranting] = useState(false);

  async function find() {
    if (!query.trim()) return;
    setFinding(true);
    setLookupError(null);
    setCustomer(null);
    setCredit(null);
    try {
      const c = await findCustomer(query);
      setCustomer(c);
      setCredit(await getAccountCredit(c.id));
    } catch (e) {
      setLookupError(message(e).includes("not found") || message(e).includes("404") ? "No customer with that email or phone in this tenant." : message(e));
    } finally {
      setFinding(false);
    }
  }

  async function grant() {
    if (!customer || !credit) return;
    const cents = amountToCents(amount);
    if (Number.isNaN(cents) || cents <= 0) return toast.error("Grant an amount above zero, like 200 or 200.00.");
    if (!note.trim()) return toast.error("Say why — the customer sees it in their history.");
    setGranting(true);
    try {
      await grantCredit({ account_id: customer.id, amount_cents: cents, currency: credit.currency, note: note.trim() });
      toast.success(`${credit.currency} ${centsToAmount(cents)} granted`);
      setAmount("");
      setNote("");
      setCredit(await getAccountCredit(customer.id));
    } catch (e) {
      toast.error(message(e));
    } finally {
      setGranting(false);
    }
  }

  return (
    <div className="space-y-4">
      <GlassCard className="p-4 sm:p-5">
        <h2 className="font-heading text-sm font-semibold text-white">Find a customer</h2>
        <p className="mt-1 text-xs text-white/45">
          Goodwill credit comes off the customer&apos;s next move by itself. It can&apos;t be withdrawn as cash, and every grant is logged with who made it.
        </p>
        <form className="mt-4 flex flex-col gap-2 sm:flex-row" onSubmit={(e) => { e.preventDefault(); void find(); }}>
          <input className={input} value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Email or phone, e.g. ana@example.com or +63 917 555 0123" />
          <button type="submit" className={primary} disabled={finding || !query.trim()}>
            <Search className="h-3.5 w-3.5" />
            {finding ? "Finding…" : "Find"}
          </button>
        </form>
        {lookupError && <p className="mt-3 text-sm text-amber-300">{lookupError}</p>}
      </GlassCard>

      {customer && credit && (
        <GlassCard className="p-4 sm:p-5">
          <div className="flex flex-col gap-1 sm:flex-row sm:items-baseline sm:justify-between">
            <div className="min-w-0">
              <p className="truncate font-medium text-white">{[customer.first_name, customer.last_name].filter(Boolean).join(" ") || customer.email}</p>
              <p className="truncate text-xs text-white/45">{customer.email}{customer.phone_number ? ` · ${customer.phone_number}` : ""}</p>
            </div>
            <p className="font-heading text-2xl font-semibold text-cyan-300">{credit.currency} {centsToAmount(credit.balance_cents)}</p>
          </div>

          <div className="mt-4 grid grid-cols-1 gap-3 sm:grid-cols-[160px_1fr_auto] sm:items-end">
            <Field label={`Amount (${credit.currency})`}>
              <input className={input} inputMode="decimal" value={amount} onChange={(e) => setAmount(e.target.value)} placeholder="200.00" />
            </Field>
            <Field label="Why">
              <input className={input} value={note} maxLength={200} onChange={(e) => setNote(e.target.value)} placeholder="Sorry about the late pickup on 12 Sep" />
            </Field>
            <button type="button" className={primary} disabled={granting} onClick={() => void grant()}>
              <Plus className="h-3.5 w-3.5" />
              {granting ? "Granting…" : "Grant credit"}
            </button>
          </div>

          <div className="mt-5 divide-y divide-white/5 border-t border-white/5">
            {credit.entries.length === 0 ? (
              <p className="pt-4 text-sm text-white/40">No credit history.</p>
            ) : (
              credit.entries.map((e, i) => (
                <div key={i} className="flex items-center justify-between gap-3 py-2.5">
                  <div className="min-w-0">
                    <p className="truncate text-sm text-white/80">{e.kind === "grant" && e.note ? e.note : ENTRY_LABEL[e.kind] ?? e.kind}</p>
                    <p className="text-[11px] text-white/35">{new Date(e.created_at).toLocaleString()}</p>
                  </div>
                  <span className={`shrink-0 font-mono text-sm ${e.amount_cents > 0 ? "text-emerald-300" : "text-white/55"}`}>
                    {e.amount_cents > 0 ? "+" : "−"}{centsToAmount(Math.abs(e.amount_cents))}
                  </span>
                </div>
              ))
            )}
          </div>
        </GlassCard>
      )}
    </div>
  );
}
