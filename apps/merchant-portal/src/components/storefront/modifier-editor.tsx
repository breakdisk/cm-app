"use client";
/**
 * Modifier groups on a catalog item — "Size", "Extras", "Spice level".
 *
 * The `modifiers` column has existed since the first catalog migration, and
 * until now nothing read it: the API round-tripped whatever JSON it was handed,
 * no price used it, and no screen showed it. A form on its own would have been
 * the same thing again, which is why this ships with the pricing path and the
 * customer-facing picker rather than ahead of them.
 *
 * The rules enforced here are the server's rules, checked early. A group the
 * customer cannot satisfy — two required picks over one option, a maximum of
 * zero — makes *every* add-to-basket for the item fail, and fail with a message
 * about the customer's choices. Catching it at the point someone types it is the
 * difference between a form error and an unorderable item.
 *
 * Prices are entered in pesos and carried as integer cents, the same conversion
 * the item's own price field makes. Deltas are signed on purpose: "no cheese"
 * legitimately takes money off.
 */
import { Plus, Trash2, TriangleAlert } from "lucide-react";

import type { ModifierGroup, ModifierOption } from "@/lib/api/storefront";

/** A stable id for a newly typed group or option. */
function newId(): string {
  // `crypto.randomUUID` needs a secure context, which the portal always is in
  // deployment but is not on a plain-http preview host. The fallback keeps the
  // editor usable there rather than throwing while a vendor is mid-form.
  if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
    return crypto.randomUUID();
  }
  return "xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx".replace(/[xy]/g, (c) => {
    const r = (Math.random() * 16) | 0;
    return (c === "x" ? r : (r & 0x3) | 0x8).toString(16);
  });
}

/**
 * Why a group would be refused, or null when it is fine.
 *
 * Mirrors `ModifierGroup::is_coherent` and `validate_modifiers` on the server.
 * Duplicated deliberately: the server is the authority and still checks, but a
 * vendor should not have to press Save to find out they typed something
 * impossible.
 */
export function groupProblem(g: ModifierGroup): string | null {
  if (g.name.trim() === "") return "This group needs a name.";
  if (g.options.length === 0) return "Add at least one option.";
  if (g.options.some((o) => o.name.trim() === "")) return "Every option needs a name.";
  if (g.max_select < 1) return "“At most” must be at least 1, or nobody can choose anything.";
  if (g.min_select > g.max_select) return "“At least” cannot exceed “at most”.";
  if (g.min_select > g.options.length) {
    return `Requires ${g.min_select} choices but only offers ${g.options.length}.`;
  }
  return null;
}

/** Every problem across a set of groups. Empty means safe to save. */
export function modifierProblems(groups: ModifierGroup[]): string[] {
  return groups
    .map((g) => {
      const p = groupProblem(g);
      return p ? `${g.name.trim() || "Untitled group"}: ${p}` : null;
    })
    .filter((p): p is string => p !== null);
}

const pesos = (cents: number) => (cents / 100).toFixed(2);

export function ModifierEditor({
  groups,
  onChange,
}: {
  groups: ModifierGroup[];
  onChange: (next: ModifierGroup[]) => void;
}) {
  const replace = (i: number, g: ModifierGroup) =>
    onChange(groups.map((x, idx) => (idx === i ? g : x)));

  const addGroup = () =>
    onChange([
      ...groups,
      {
        id: newId(),
        name: "",
        // Optional and single-choice: the least surprising thing a new group can
        // be, and the only combination that cannot make the item unorderable
        // while it is still half-typed.
        min_select: 0,
        max_select: 1,
        options: [{ id: newId(), name: "", price_delta_cents: 0 }],
      },
    ]);

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between">
        <div>
          <span className="block text-xs text-white/50">
            Options <span className="text-white/25">— optional</span>
          </span>
          <span className="block text-[11px] text-white/30">
            Choices the customer makes when ordering, like size or add-ons.
          </span>
        </div>
        <button
          type="button"
          onClick={addGroup}
          className="inline-flex items-center gap-1 rounded-lg border border-white/10 bg-white/5 px-2.5 py-1.5 text-xs text-white/70 hover:border-cyan-400/40 hover:text-white"
        >
          <Plus className="h-3.5 w-3.5" /> Add group
        </button>
      </div>

      {groups.length === 0 && (
        <p className="rounded-lg border border-dashed border-white/10 px-3 py-4 text-center text-xs text-white/30">
          No options. Most items need none.
        </p>
      )}

      {groups.map((g, gi) => {
        const problem = groupProblem(g);
        return (
          <div
            key={g.id}
            className={`rounded-lg border p-3 ${
              problem ? "border-amber-400/40 bg-amber-400/5" : "border-white/10 bg-white/[0.03]"
            }`}
          >
            <div className="flex items-start gap-2">
              <input
                value={g.name}
                onChange={(e) => replace(gi, { ...g, name: e.target.value })}
                placeholder="Size"
                aria-label="Group name"
                className="min-w-0 flex-1 rounded-lg border border-white/10 bg-white/5 px-3 py-2 text-sm text-white placeholder:text-white/25 focus:border-cyan-400/50 focus:outline-none"
              />
              <button
                type="button"
                onClick={() => onChange(groups.filter((_, idx) => idx !== gi))}
                aria-label={`Remove ${g.name || "group"}`}
                className="rounded-lg border border-white/10 p-2 text-white/40 hover:border-rose-400/40 hover:text-rose-300"
              >
                <Trash2 className="h-3.5 w-3.5" />
              </button>
            </div>

            <div className="mt-2 flex flex-wrap items-center gap-3 text-[11px] text-white/40">
              <label className="flex items-center gap-1.5">
                Pick at least
                <input
                  type="number"
                  min={0}
                  value={g.min_select}
                  onChange={(e) =>
                    replace(gi, { ...g, min_select: Math.max(0, Number(e.target.value) || 0) })
                  }
                  aria-label="Minimum selections"
                  className="w-14 rounded border border-white/10 bg-white/5 px-2 py-1 text-center text-white focus:border-cyan-400/50 focus:outline-none"
                />
              </label>
              <label className="flex items-center gap-1.5">
                at most
                <input
                  type="number"
                  min={1}
                  value={g.max_select}
                  onChange={(e) =>
                    replace(gi, { ...g, max_select: Math.max(0, Number(e.target.value) || 0) })
                  }
                  aria-label="Maximum selections"
                  className="w-14 rounded border border-white/10 bg-white/5 px-2 py-1 text-center text-white focus:border-cyan-400/50 focus:outline-none"
                />
              </label>
              <span className="text-white/25">
                {g.min_select === 0 ? "optional" : "required"} ·{" "}
                {g.max_select === 1 ? "one choice" : `up to ${g.max_select}`}
              </span>
            </div>

            <div className="mt-2 space-y-1.5">
              {g.options.map((o, oi) => (
                <div key={o.id} className="flex items-center gap-2">
                  <input
                    value={o.name}
                    onChange={(e) =>
                      replace(gi, {
                        ...g,
                        options: g.options.map((x, idx): ModifierOption =>
                          idx === oi ? { ...x, name: e.target.value } : x,
                        ),
                      })
                    }
                    placeholder="Large"
                    aria-label="Option name"
                    className="min-w-0 flex-1 rounded-lg border border-white/10 bg-white/5 px-3 py-1.5 text-sm text-white placeholder:text-white/25 focus:border-cyan-400/50 focus:outline-none"
                  />
                  <div className="flex items-center gap-1 text-xs text-white/40">
                    <span aria-hidden>₱</span>
                    <input
                      type="number"
                      step="0.01"
                      value={pesos(o.price_delta_cents)}
                      onChange={(e) =>
                        replace(gi, {
                          ...g,
                          options: g.options.map((x, idx): ModifierOption =>
                            idx === oi
                              ? {
                                  ...x,
                                  // Pesos in, cents on the wire — the same
                                  // rounding the item price field does.
                                  price_delta_cents: Math.round(
                                    parseFloat(e.target.value || "0") * 100,
                                  ),
                                }
                              : x,
                          ),
                        })
                      }
                      aria-label={`Extra cost for ${o.name || "this option"}`}
                      className="w-20 rounded border border-white/10 bg-white/5 px-2 py-1.5 text-right text-white focus:border-cyan-400/50 focus:outline-none"
                    />
                  </div>
                  <button
                    type="button"
                    onClick={() =>
                      replace(gi, { ...g, options: g.options.filter((_, idx) => idx !== oi) })
                    }
                    aria-label={`Remove ${o.name || "option"}`}
                    className="rounded-lg border border-white/10 p-1.5 text-white/30 hover:border-rose-400/40 hover:text-rose-300"
                  >
                    <Trash2 className="h-3 w-3" />
                  </button>
                </div>
              ))}
              <button
                type="button"
                onClick={() =>
                  replace(gi, {
                    ...g,
                    options: [...g.options, { id: newId(), name: "", price_delta_cents: 0 }],
                  })
                }
                className="inline-flex items-center gap-1 text-[11px] text-cyan-300/70 hover:text-cyan-200"
              >
                <Plus className="h-3 w-3" /> Add option
              </button>
            </div>

            {problem && (
              <p className="mt-2 flex items-start gap-1.5 text-[11px] text-amber-300/90">
                <TriangleAlert className="mt-px h-3 w-3 shrink-0" />
                {problem}
              </p>
            )}
          </div>
        );
      })}
    </div>
  );
}
