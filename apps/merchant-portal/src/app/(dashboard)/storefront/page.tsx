"use client";
/**
 * Merchant Portal — Storefront (OmniDeliv vendor console)
 *
 * Why this page exists, beyond "vendors need a screen":
 *
 * OmniDeliv's agent decides whether to line up a substitute from the freshness
 * of an availability declaration, not just its value. An item marked available
 * but not confirmed inside the freshness window (STOCK_FRESHNESS_MINS, 30 by
 * default) reads as *uncertain*, and the Nutritionist proposes a substitute for
 * it. With no console, nobody ever confirms anything — so half an hour after a
 * deploy every item in every store is being quietly swapped out, and the mesh
 * looks like it is behaving badly when it is doing exactly what it was told.
 *
 * So the primary control here is not "edit my menu". It is "confirm this is
 * true", and the page leads with what has gone stale.
 *
 * The catalog can arrive two ways — typed here, or pushed through the ingest
 * port by a Shopify/Woo/POS adapter — and the confirmation loop is identical
 * either way. A synced item lands with `confirmed_at: null` and stays uncertain
 * until a person says otherwise, because a nightly-reconciled stock count is
 * precisely the old evidence the confidence model exists to distrust. That is
 * why "Never confirmed" is its own state on this screen rather than being
 * rendered as "confirmed a long time ago".
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { motion } from "framer-motion";
import {
  AlertTriangle,
  Check,
  CheckCheck,
  DownloadCloud,
  Upload,
  PackageX,
  Pencil,
  Plus,
  RefreshCw,
  ShieldAlert,
  Store,
  Trash2,
  X,
} from "lucide-react";

import { GlassCard } from "@/components/ui/glass-card";
import { variants } from "@/lib/design-system/tokens";
import { authFetch } from "@/lib/auth/auth-fetch";
import { API_BASE } from "@/lib/api/endpoints";
import {
  storefrontApi,
  VERTICALS,
  type Availability,
  type Catalog,
  type CatalogSource,
  type CsvRowError,
  type Item,
} from "@/lib/api/storefront";

interface Earnings {
  period: string;
  balance_cents: number;
}

/**
 * The set a vendor can state in one tap. Not exhaustive and not meant to be —
 * it covers the common cases, and anything unusual is why the storefront needs
 * a fuller editor later. Listing a wrong-but-quick set would be worse than
 * offering fewer accurate ones.
 */
const COMMON_ALLERGENS = ["peanuts", "dairy", "eggs", "shellfish", "gluten", "soy"];

const peso = (cents: number) =>
  `₱${(cents / 100).toLocaleString("en-PH", { minimumFractionDigits: 2 })}`;

/** "12m ago" — the number that decides whether the agent trusts this row. */
function since(iso: string): string {
  const mins = Math.max(0, Math.round((Date.now() - new Date(iso).getTime()) / 60000));
  if (mins < 1) return "just now";
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.round(mins / 60);
  if (hours < 24) return `${hours}h ago`;
  return `${Math.round(hours / 24)}d ago`;
}

const SOURCE_LABEL: Record<CatalogSource, string> = {
  manual: "typed here",
  shopify: "Shopify",
  woocommerce: "WooCommerce",
  csv: "CSV import",
  pos: "POS",
};

export default function Storefront() {
  const [catalog, setCatalog] = useState<Catalog | null>(null);
  const [earnings, setEarnings] = useState<Earnings | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [noStore, setNoStore] = useState(false);
  const [editing, setEditing] = useState<Item | null>(null);
  const [adding, setAdding] = useState(false);
  const [rowErrors, setRowErrors] = useState<CsvRowError[]>([]);
  const fileInput = useRef<HTMLInputElement>(null);

  const load = useCallback(async () => {
    try {
      const c = await storefrontApi.catalog();
      if (c === null) {
        // Distinct from "no items": this login runs no store at all.
        setNoStore(true);
        return;
      }
      setCatalog(c);
      setNoStore(false);

      const e = await authFetch(`${API_BASE}/v1/omnideliv/vendors/me/earnings`);
      if (e.ok) setEarnings(await e.json());
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : "could not load the storefront");
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  /** Run a mutation, then reload — server-set stamps must not be guessed here. */
  const run = useCallback(
    async (key: string, fn: () => Promise<void>, ok?: string) => {
      setBusy(key);
      setNotice(null);
      if (key !== "import") setRowErrors([]);
      try {
        await fn();
        await load();
        setError(null);
        if (ok) setNotice(ok);
      } catch (err) {
        setError(err instanceof Error ? err.message : "that did not work");
      } finally {
        setBusy(null);
      }
    },
    [load],
  );

  const setAvailability = useCallback(
    (itemId: string, state: Availability) =>
      run(itemId, () => storefrontApi.setAvailability(itemId, state)),
    [run],
  );

  /**
   * Declare an item's contents.
   *
   * `[]` is a real statement — "I confirm it contains none of these" — and is
   * precisely what an undeclared item cannot say. That is why the control is
   * two explicit buttons rather than a text field left blank: a blank field is
   * how the item got into this state.
   */
  const declare = useCallback(
    (itemId: string, allergens: string[]) =>
      run(itemId, () => storefrontApi.declareAllergens(itemId, allergens)),
    [run],
  );

  const confirmAll = useCallback(
    () =>
      run("confirm-all", async () => {
        const n = await storefrontApi.confirmAll();
        setNotice(`Confirmed ${n} item${n === 1 ? "" : "s"}.`);
      }),
    [run],
  );

  const importCsv = useCallback(
    (file: File) =>
      run("import", async () => {
        const r = await storefrontApi.importCsv(file);
        setRowErrors(r.row_errors);
        setNotice(
          `Imported ${r.created} new and ${r.updated} updated from ${file.name}` +
            (r.row_errors.length > 0
              ? ` — ${r.row_errors.length} row${r.row_errors.length === 1 ? "" : "s"} could not be read`
              : "") +
            `. ${r.next_step}`,
        );
      }),
    [run],
  );

  const syncCatalog = useCallback(
    () =>
      run("sync", async () => {
        const r = await storefrontApi.syncCatalog();
        // The counts and the caveat together. "Synced 43 items" alone reads as
        // "you are selling 43 items", and the merchant would wait for orders
        // that cannot come until someone confirms stock.
        //
        // Anything the sync could not bring over is named here rather than left
        // in a server log — a partial import that reads as a complete one is
        // how a merchant discovers a missing dish from a customer complaint.
        const dropped = [
          r.rejected > 0 ? `${r.rejected} rejected` : null,
          r.unpriced > 0 ? `${r.unpriced} with no price` : null,
          r.deferred > 0 ? `${r.deferred} not fetched` : null,
        ].filter(Boolean);

        setNotice(
          `Synced ${r.fetched} from ${SOURCE_LABEL[r.platform as CatalogSource] ?? r.platform}` +
            ` — ${r.created} new, ${r.updated} updated` +
            (dropped.length > 0 ? ` (${dropped.join(", ")})` : "") +
            `. ${r.next_step}`,
        );
      }),
    [run],
  );

  const items = useMemo(() => catalog?.items ?? [], [catalog]);
  const stale = items.filter((i) => i.warrants_substitute);
  const undeclared = items.filter((i) => !i.allergens_declared);
  const neverConfirmed = items.filter((i) => i.confirmed_at === null);
  const synced = items.filter((i) => i.source !== "manual");

  if (noStore) {
    return (
      <ApplyToSell
        onApplied={() => {
          setNoStore(false);
          void load();
        }}
      />
    );
  }

  return (
    // `variants={}` rather than a spread: fadeIn is a Framer variants map
    // ({hidden, visible}), and spreading it set a `hidden` DOM prop instead of
    // driving the animation.
    <motion.div
      variants={variants.fadeIn}
      initial="hidden"
      animate="visible"
      className="space-y-5 p-4 sm:p-6"
    >
      <div className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
        <div className="min-w-0">
          <h1 className="truncate text-xl font-bold text-white sm:text-2xl">
            {catalog?.vendor_name ?? "Storefront"}
          </h1>
          <p className="text-sm text-white/50">
            Confirm what you have. Anything unconfirmed gets substituted.
          </p>
        </div>

        <div className="flex flex-wrap items-center gap-2">
          <button
            onClick={() => setAdding(true)}
            className="flex items-center gap-2 rounded-lg border border-[#00E5FF]/40 bg-[#00E5FF]/10 px-3 py-2 text-sm text-[#00E5FF] hover:bg-[#00E5FF]/20"
          >
            <Plus className="h-4 w-4" /> Add item
          </button>
          <input
            ref={fileInput}
            type="file"
            accept=".csv,text/csv"
            className="hidden"
            onChange={(e) => {
              const f = e.target.files?.[0];
              // Reset first: picking the same file twice must re-fire change,
              // which it will not if the value still holds that filename.
              e.target.value = "";
              if (f) void importCsv(f);
            }}
          />
          <button
            onClick={() => fileInput.current?.click()}
            disabled={busy === "import"}
            className="flex items-center gap-2 rounded-lg border border-white/10 px-3 py-2 text-sm text-white/70 hover:bg-white/5 disabled:opacity-40"
          >
            <Upload className="h-4 w-4" />
            {busy === "import" ? "Importing…" : "Import CSV"}
          </button>
          <button
            onClick={() => void syncCatalog()}
            disabled={busy === "sync"}
            className="flex items-center gap-2 rounded-lg border border-white/10 px-3 py-2 text-sm text-white/70 hover:bg-white/5 disabled:opacity-40"
          >
            <DownloadCloud className="h-4 w-4" />
            {busy === "sync" ? "Syncing…" : "Sync from shop"}
          </button>
          <button
            onClick={() => void confirmAll()}
            disabled={busy === "confirm-all" || items.length === 0}
            className="flex items-center gap-2 rounded-lg border border-[#00FF88]/40 bg-[#00FF88]/10 px-3 py-2 text-sm text-[#00FF88] hover:bg-[#00FF88]/20 disabled:opacity-40"
          >
            <CheckCheck className="h-4 w-4" /> Confirm all
          </button>
          <button
            onClick={() => void load()}
            aria-label="Refresh"
            className="flex items-center gap-2 rounded-lg border border-white/10 px-3 py-2 text-sm text-white/70 hover:bg-white/5"
          >
            <RefreshCw className="h-4 w-4" />
            <span className="hidden sm:inline">Refresh</span>
          </button>
        </div>
      </div>

      {error && (
        <GlassCard className="border-l-2 border-l-[#FF3B5C] p-4">
          <p role="alert" className="text-sm text-[#FF3B5C]">
            {error}
          </p>
        </GlassCard>
      )}
      {notice && (
        <GlassCard className="border-l-2 border-l-[#00FF88] p-4">
          <p role="status" className="text-sm text-[#00FF88]">
            {notice}
          </p>
        </GlassCard>
      )}

      {rowErrors.length > 0 && (
        <GlassCard className="border-l-2 border-l-[#FFAB00] p-4">
          <p className="text-sm font-semibold text-[#FFAB00]">
            {rowErrors.length} row{rowErrors.length === 1 ? "" : "s"} in that file could not be
            imported
          </p>
          <p className="mt-1 text-xs text-white/60">
            Everything else went in. Fix these lines and upload again — re-importing is safe,
            rows are matched on SKU.
          </p>
          <ul className="mt-2 max-h-40 space-y-1 overflow-y-auto font-mono text-xs text-white/70">
            {rowErrors.map((e) => (
              <li key={e.line}>
                <span className="text-white/40">line {e.line}:</span> {e.reason}
              </li>
            ))}
          </ul>
        </GlassCard>
      )}

      {/* Above staleness on purpose. A stale item still gets sold with a
          substitute lined up; an undeclared one is refused outright to anyone
          with an allergy, and the vendor has no other way to learn that. */}
      {undeclared.length > 0 && (
        <GlassCard className="border-l-2 border-l-[#FF3B5C] p-4">
          <div className="flex items-start gap-3">
            <ShieldAlert className="mt-0.5 h-5 w-5 shrink-0 text-[#FF3B5C]" />
            <div>
              <p className="text-sm font-semibold text-[#FF3B5C]">
                {undeclared.length} item{undeclared.length === 1 ? "" : "s"} won&apos;t be
                offered to customers with allergies
              </p>
              <p className="mt-1 text-xs text-white/60">
                Nobody has stated what&apos;s in them. We won&apos;t guess on a
                customer&apos;s behalf, so we leave them out rather than risk it. Say what
                each one contains — &ldquo;none of these&rdquo; is a valid answer, and
                it&apos;s the one an undeclared item can&apos;t make.
                {synced.length > 0 && (
                  <>
                    {" "}
                    Items your shop pushed to us stay in this list on purpose: a product
                    tag is data, not a statement that someone checked the recipe.
                  </>
                )}
              </p>
            </div>
          </div>
        </GlassCard>
      )}

      {/* Lead with the consequence, not the inventory. */}
      {stale.length > 0 && (
        <GlassCard className="border-l-2 border-l-[#FFAB00] p-4">
          <div className="flex items-start gap-3">
            <AlertTriangle className="mt-0.5 h-5 w-5 shrink-0 text-[#FFAB00]" />
            <div>
              <p className="text-sm font-semibold text-[#FFAB00]">
                {stale.length} item{stale.length === 1 ? "" : "s"} will be substituted
              </p>
              <p className="mt-1 text-xs text-white/60">
                Either they are out of stock or limited, or nobody has confirmed them
                recently enough for the assistant to rely on. Confirming an item as
                available resets that clock.
                {neverConfirmed.length > 0 && (
                  <>
                    {" "}
                    <span className="text-white/80">
                      {neverConfirmed.length} of them have never been confirmed by anyone
                    </span>{" "}
                    — newly added or imported. &ldquo;Confirm all&rdquo; clears them in one
                    go.
                  </>
                )}
              </p>
            </div>
          </div>
        </GlassCard>
      )}

      {earnings && (
        <GlassCard className="p-4">
          <div className="flex flex-wrap items-baseline justify-between gap-2">
            <span className="text-xs uppercase tracking-wider text-white/40">
              Payouts · {earnings.period}
            </span>
            <span className="text-2xl font-bold text-[#00FF88]">
              {peso(earnings.balance_cents)}
            </span>
          </div>
        </GlassCard>
      )}

      {(adding || editing) && (
        <ItemForm
          item={editing}
          onClose={() => {
            setAdding(false);
            setEditing(null);
          }}
          onSaved={async () => {
            setAdding(false);
            setEditing(null);
            await load();
          }}
          onError={setError}
        />
      )}

      <GlassCard className="overflow-hidden p-0">
        <div className="divide-y divide-white/5">
          {items.length === 0 && (
            <div className="p-6 text-center">
              <p className="text-sm text-white/50">This store has no items yet.</p>
              <button
                onClick={() => setAdding(true)}
                className="mt-3 inline-flex items-center gap-2 rounded-lg border border-[#00E5FF]/40 px-3 py-2 text-sm text-[#00E5FF] hover:bg-[#00E5FF]/10"
              >
                <Plus className="h-4 w-4" /> Add your first item
              </button>
            </div>
          )}

          {items.map((item) => (
            <div
              key={item.id}
              className="flex flex-col gap-3 p-4 lg:flex-row lg:items-center lg:justify-between"
            >
              <div className="min-w-0 flex-1">
                <div className="flex flex-wrap items-center gap-2">
                  <span className="truncate font-semibold text-white">{item.name}</span>
                  {item.warrants_substitute && (
                    <span className="rounded bg-[#FFAB00]/10 px-1.5 py-0.5 text-[10px] uppercase tracking-wider text-[#FFAB00]">
                      substituting
                    </span>
                  )}
                  {item.source !== "manual" && (
                    <span className="rounded bg-white/5 px-1.5 py-0.5 text-[10px] uppercase tracking-wider text-white/40">
                      {SOURCE_LABEL[item.source]}
                    </span>
                  )}
                </div>

                <p className="mt-0.5 text-xs text-white/40">
                  <span className="font-mono">{item.sku}</span> · {peso(item.price_cents)} ·{" "}
                  {item.confirmed_at === null ? (
                    // Deliberately not "confirmed never ago". An item nobody has
                    // ever attested to is a different state from a stale one, and
                    // collapsing them is what let imports look verified.
                    <span className="text-[#FFAB00]">never confirmed</span>
                  ) : (
                    <>confirmed {since(item.confirmed_at)}</>
                  )}
                  {item.synced_at && <> · synced {since(item.synced_at)}</>}
                  {item.allergens_declared
                    ? item.allergens.length > 0
                      ? ` · contains ${item.allergens.join(", ")}`
                      : " · declared allergen-free"
                    : ""}
                </p>

                {!item.allergens_declared && (
                  <div className="mt-2 flex flex-wrap items-center gap-2">
                    <span className="text-[11px] text-[#FF3B5C]">Contents not stated:</span>
                    {COMMON_ALLERGENS.map((a) => (
                      <button
                        key={a}
                        disabled={busy === item.id}
                        onClick={() => void declare(item.id, [a])}
                        className="rounded border border-white/10 px-2 py-0.5 text-[11px] text-white/60 hover:bg-white/5 disabled:opacity-40"
                      >
                        contains {a}
                      </button>
                    ))}
                    <button
                      disabled={busy === item.id}
                      onClick={() => void declare(item.id, [])}
                      className="rounded border border-[#00FF88]/40 px-2 py-0.5 text-[11px] text-[#00FF88] hover:bg-[#00FF88]/10 disabled:opacity-40"
                    >
                      none of these
                    </button>
                  </div>
                )}
              </div>

              <div className="flex shrink-0 flex-wrap items-center gap-2">
                {(
                  [
                    ["available", Check, "#00FF88"],
                    ["limited", AlertTriangle, "#FFAB00"],
                    ["out_of_stock", PackageX, "#FF3B5C"],
                  ] as const
                ).map(([state, Icon, colour]) => {
                  const active = item.availability === state;
                  return (
                    <button
                      key={state}
                      disabled={busy === item.id}
                      onClick={() => void setAvailability(item.id, state)}
                      aria-label={`Mark ${item.name} ${state.replace("_", " ")}`}
                      className="flex items-center gap-1.5 rounded-lg border px-2.5 py-1.5 text-xs transition disabled:opacity-40"
                      style={{
                        borderColor: active ? colour : "rgba(255,255,255,0.10)",
                        backgroundColor: active ? `${colour}1A` : "transparent",
                        color: active ? colour : "rgba(255,255,255,0.55)",
                      }}
                    >
                      <Icon className="h-3.5 w-3.5" />
                      <span className="hidden sm:inline">
                        {state === "out_of_stock" ? "Out" : state === "limited" ? "Low" : "In stock"}
                      </span>
                    </button>
                  );
                })}

                <button
                  onClick={() => setEditing(item)}
                  aria-label={`Edit ${item.name}`}
                  className="rounded-lg border border-white/10 p-1.5 text-white/50 hover:bg-white/5 hover:text-white/80"
                >
                  <Pencil className="h-3.5 w-3.5" />
                </button>
                <button
                  disabled={busy === item.id}
                  onClick={() =>
                    void run(item.id, () => storefrontApi.delistItem(item.id), `Removed ${item.name}.`)
                  }
                  aria-label={`Remove ${item.name} from the menu`}
                  className="rounded-lg border border-white/10 p-1.5 text-white/50 hover:bg-[#FF3B5C]/10 hover:text-[#FF3B5C] disabled:opacity-40"
                >
                  <Trash2 className="h-3.5 w-3.5" />
                </button>
              </div>
            </div>
          ))}
        </div>
      </GlassCard>

      <p className="text-xs text-white/30">
        Re-confirming an item as available resets its freshness clock, even if nothing
        changed — that is the point of the control. Removing an item takes it off the menu
        and keeps its order history.
      </p>
    </motion.div>
  );
}

/**
 * Add or edit one item.
 *
 * Allergens are absent from this form on purpose. Declaring contents is a
 * separate, deliberate act with its own control on the row — folding it into a
 * create form would mean every new item carried an attestation made while
 * someone was typing a price.
 */
function ItemForm({
  item,
  onClose,
  onSaved,
  onError,
}: {
  item: Item | null;
  onClose: () => void;
  onSaved: () => Promise<void>;
  onError: (msg: string) => void;
}) {
  const [sku, setSku] = useState(item?.sku ?? "");
  const [name, setName] = useState(item?.name ?? "");
  const [description, setDescription] = useState(item?.description ?? "");
  const [price, setPrice] = useState(item ? (item.price_cents / 100).toFixed(2) : "");
  const [saving, setSaving] = useState(false);

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    setSaving(true);
    try {
      // Pesos in the form, cents on the wire. Rounding here rather than
      // trusting a float through JSON: money is integer cents everywhere in
      // this platform and the conversion belongs at the boundary.
      const price_cents = Math.round(parseFloat(price || "0") * 100);
      if (item) {
        await storefrontApi.updateItem(item.id, {
          name,
          description: description.trim() === "" ? null : description,
          price_cents,
        });
      } else {
        await storefrontApi.createItem({
          sku,
          name,
          description: description.trim() === "" ? null : description,
          price_cents,
        });
      }
      await onSaved();
    } catch (err) {
      onError(err instanceof Error ? err.message : "could not save that item");
    } finally {
      setSaving(false);
    }
  };

  return (
    <GlassCard className="p-4">
      <form onSubmit={submit} className="space-y-3">
        <div className="flex items-center justify-between">
          <h2 className="text-sm font-semibold text-white">
            {item ? `Edit ${item.name}` : "New item"}
          </h2>
          <button type="button" onClick={onClose} aria-label="Close" className="text-white/40 hover:text-white">
            <X className="h-4 w-4" />
          </button>
        </div>

        <div className="grid gap-3 sm:grid-cols-2">
          <label className="block">
            <span className="mb-1 block text-xs text-white/50">SKU</span>
            <input
              value={sku}
              onChange={(e) => setSku(e.target.value)}
              required
              // A SKU is the ingest port's fallback match key, so changing it
              // later would make a re-sync create a duplicate rather than
              // update this row. Fixed after creation.
              disabled={item !== null}
              placeholder="ADOBO-REG"
              className="w-full rounded-lg border border-white/10 bg-white/5 px-3 py-2 font-mono text-sm text-white placeholder:text-white/20 disabled:opacity-40"
            />
          </label>

          <label className="block">
            <span className="mb-1 block text-xs text-white/50">Price (₱)</span>
            <input
              value={price}
              onChange={(e) => setPrice(e.target.value)}
              required
              type="number"
              min="0"
              step="0.01"
              placeholder="180.00"
              className="w-full rounded-lg border border-white/10 bg-white/5 px-3 py-2 text-sm text-white placeholder:text-white/20"
            />
          </label>
        </div>

        <label className="block">
          <span className="mb-1 block text-xs text-white/50">Name</span>
          <input
            value={name}
            onChange={(e) => setName(e.target.value)}
            required
            placeholder="Chicken Adobo"
            className="w-full rounded-lg border border-white/10 bg-white/5 px-3 py-2 text-sm text-white placeholder:text-white/20"
          />
        </label>

        <label className="block">
          <span className="mb-1 block text-xs text-white/50">Description (optional)</span>
          <input
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            placeholder="with garlic rice"
            className="w-full rounded-lg border border-white/10 bg-white/5 px-3 py-2 text-sm text-white placeholder:text-white/20"
          />
        </label>

        <div className="flex flex-wrap items-center gap-2 pt-1">
          <button
            type="submit"
            disabled={saving}
            className="rounded-lg border border-[#00E5FF]/40 bg-[#00E5FF]/10 px-4 py-2 text-sm text-[#00E5FF] hover:bg-[#00E5FF]/20 disabled:opacity-40"
          >
            {saving ? "Saving…" : item ? "Save changes" : "Add item"}
          </button>
          <button
            type="button"
            onClick={onClose}
            className="rounded-lg border border-white/10 px-4 py-2 text-sm text-white/60 hover:bg-white/5"
          >
            Cancel
          </button>
          {!item && (
            <span className="text-xs text-white/35">
              New items start unconfirmed — confirm stock once it&apos;s on the shelf.
            </span>
          )}
        </div>
      </form>
    </GlassCard>
  );
}

/**
 * The only way to become an OmniDeliv vendor.
 *
 * This used to be a dead end: it told the merchant their `user_id` was not set
 * and to "ask an operator", while no operator UI could set it either. Every
 * vendor that existed had been written by hand in SQL, so in practice nobody
 * could start selling — the Storefront nav tab hides until you have a store,
 * and the only route to having one was behind that hidden tab.
 *
 * Coordinates are asked for rather than defaulted. `find_near` is what puts a
 * store in front of a customer, so a wrong or zeroed position does not fail
 * loudly — it just means nobody is ever shown the shop.
 */
function ApplyToSell({ onApplied }: { onApplied: () => void }) {
  const [name, setName] = useState("");
  const [vertical, setVertical] = useState<string>(VERTICALS[0].value);
  const [address, setAddress] = useState("");
  const [lat, setLat] = useState<string>("");
  const [lng, setLng] = useState<string>("");
  const [locating, setLocating] = useState(false);
  const [saving, setSaving] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  const latNum = Number(lat);
  const lngNum = Number(lng);
  const coordsValid =
    lat.trim() !== "" && lng.trim() !== "" &&
    Number.isFinite(latNum) && Number.isFinite(lngNum) &&
    Math.abs(latNum) <= 90 && Math.abs(lngNum) <= 180;
  const ready = name.trim() !== "" && address.trim() !== "" && coordsValid && !saving;

  function useMyLocation() {
    if (typeof navigator === "undefined" || !navigator.geolocation) {
      setErr("This browser cannot report a location — enter the coordinates instead.");
      return;
    }
    setLocating(true);
    setErr(null);
    navigator.geolocation.getCurrentPosition(
      (pos) => {
        setLat(pos.coords.latitude.toFixed(6));
        setLng(pos.coords.longitude.toFixed(6));
        setLocating(false);
      },
      (geoErr) => {
        setLocating(false);
        setErr(
          geoErr.code === geoErr.PERMISSION_DENIED
            ? "Location permission was declined — enter the coordinates instead."
            : "Could not read this device's location — enter the coordinates instead.",
        );
      },
      { enableHighAccuracy: true, timeout: 10_000 },
    );
  }

  async function submit() {
    setSaving(true);
    setErr(null);
    try {
      await storefrontApi.apply({
        vertical,
        name: name.trim(),
        address: address.trim(),
        lat: latNum,
        lng: lngNum,
      });
      onApplied();
    } catch (e) {
      setErr(e instanceof Error ? e.message : "could not submit the application");
      setSaving(false);
    }
  }

  const field =
    "w-full rounded-lg border border-white/10 bg-white/5 px-3 py-2 text-sm text-white " +
    "placeholder:text-white/30 focus:border-cyan-400/50 focus:outline-none";

  return (
    <div className="p-4 sm:p-6">
      <GlassCard className="mx-auto max-w-xl p-6 sm:p-8">
        <Store className="mb-3 h-8 w-8 text-cyan-400/80" />
        <h1 className="text-lg font-semibold text-white">Sell on OmniDeliv</h1>
        <p className="mt-1 text-sm text-white/50">
          This login does not run a store yet. Apply here and you can build your
          catalog straight away — an operator reviews the shop before customers
          can order from it.
        </p>

        <div className="mt-6 space-y-4">
          <div>
            <label htmlFor="v-name" className="mb-1 block text-xs text-white/60">Store name</label>
            <input id="v-name" className={field} value={name} placeholder="Kuya&apos;s Silog House"
                   onChange={(e) => setName(e.target.value)} />
          </div>

          <div>
            <label htmlFor="v-vertical" className="mb-1 block text-xs text-white/60">What do you sell?</label>
            <select id="v-vertical" className={field} value={vertical}
                    onChange={(e) => setVertical(e.target.value)}>
              {VERTICALS.map((v) => (
                <option key={v.value} value={v.value} className="bg-[#0a0f1c]">{v.label}</option>
              ))}
            </select>
          </div>

          <div>
            <label htmlFor="v-address" className="mb-1 block text-xs text-white/60">Address</label>
            <input id="v-address" className={field} value={address} placeholder="12 Mabini St, Ermita, Manila"
                   onChange={(e) => setAddress(e.target.value)} />
          </div>

          <div>
            <span className="mb-1 block text-xs text-white/60">
              Where the shop is — this is what decides which customers see it
            </span>
            <div className="flex flex-col gap-2 sm:flex-row">
              <input aria-label="Latitude" className={field} value={lat} placeholder="Latitude"
                     inputMode="decimal" onChange={(e) => setLat(e.target.value)} />
              <input aria-label="Longitude" className={field} value={lng} placeholder="Longitude"
                     inputMode="decimal" onChange={(e) => setLng(e.target.value)} />
            </div>
            <button
              type="button"
              onClick={useMyLocation}
              disabled={locating}
              className="mt-2 rounded-lg border border-white/10 px-3 py-1.5 text-xs text-white/70 hover:bg-white/5 disabled:opacity-40"
            >
              {locating ? "Locating…" : "Use my current location"}
            </button>
            {lat.trim() !== "" && lng.trim() !== "" && !coordsValid && (
              <p className="mt-1 text-xs text-amber-300/80">
                That is not a valid latitude/longitude.
              </p>
            )}
          </div>

          {err && (
            <p className="rounded-lg border border-rose-400/20 bg-rose-400/5 px-3 py-2 text-xs text-rose-300">
              {err}
            </p>
          )}

          <button
            type="button"
            onClick={() => void submit()}
            disabled={!ready}
            className="w-full rounded-lg bg-cyan-500/90 px-4 py-2.5 text-sm font-medium text-[#04121a] hover:bg-cyan-400 disabled:cursor-not-allowed disabled:opacity-40"
          >
            {saving ? "Submitting…" : "Apply to sell"}
          </button>
        </div>
      </GlassCard>
    </div>
  );
}
