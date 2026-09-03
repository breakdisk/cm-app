/**
 * A vendor's menu, grouped by category.
 *
 * Extracted from the table-QR diner page so the public storefront can render
 * the same menu the same way. The two pages differ in how you *arrive* — a
 * printed code at a table versus a link someone shared — and in what you can do
 * once there, but the menu itself is one thing and should look like one thing.
 *
 * `onAdd` is what separates them. With it, this is an ordering surface; without
 * it, a read-only menu. The public storefront passes nothing, because ordering
 * there needs a customer principal that does not exist yet — see the note in
 * `app/s/[slug]/page.tsx`.
 *
 * Deliberately not a client component. It renders from props and holds no
 * state, so the public storefront can render it on the server (which is what
 * lets a social crawler see the menu) while the diner page uses the identical
 * markup inside its own client tree.
 */
import { Plus } from "lucide-react";

/** The subset both callers actually have. */
export interface MenuItem {
  item_id: string;
  name: string;
  price_cents: number;
  category: string | null;
}

/**
 * Prices are stored in the smallest unit and rendered in pesos.
 *
 * Shared so the diner page and the public page cannot disagree about what a
 * price looks like — the same number on the sticker and on the shared link.
 */
export function money(cents: number): string {
  return `₱${(cents / 100).toLocaleString(undefined, {
    minimumFractionDigits: 2,
  })}`;
}

/** Uncategorised items sort last under "More", rather than vanishing. */
export function groupByCategory(items: MenuItem[]): [string, MenuItem[]][] {
  const by = new Map<string, MenuItem[]>();
  for (const i of items) {
    const k = i.category ?? "More";
    if (!by.has(k)) by.set(k, []);
    by.get(k)!.push(i);
  }
  return Array.from(by.entries());
}

export function MenuList({
  items,
  onAdd,
  busyItemId,
}: {
  items: MenuItem[];
  /** Omit for a read-only menu. */
  onAdd?: (item: MenuItem) => void;
  busyItemId?: string | null;
}) {
  if (items.length === 0) {
    return (
      <p className="py-16 text-center text-sm text-white/40">
        Nothing on the menu right now.
      </p>
    );
  }

  return (
    <div className="space-y-6">
      {groupByCategory(items).map(([category, group]) => (
        <section key={category}>
          <h2 className="mb-2 text-xs font-medium uppercase tracking-wide text-white/40">
            {category}
          </h2>
          <ul className="space-y-2">
            {group.map((item) => (
              <li
                key={item.item_id}
                className="flex items-center gap-3 rounded-xl border border-white/10 bg-white/[0.03] p-3"
              >
                <div className="min-w-0 flex-1">
                  <p className="truncate text-sm font-medium text-white">{item.name}</p>
                  <p className="text-sm text-white/50">{money(item.price_cents)}</p>
                </div>
                {onAdd && (
                  <button
                    onClick={() => onAdd(item)}
                    disabled={busyItemId === item.item_id}
                    aria-label={`Add ${item.name}`}
                    className="shrink-0 rounded-lg border border-cyan-400/40 bg-cyan-400/10 p-2 text-cyan-300 transition active:scale-95 disabled:opacity-40"
                  >
                    <Plus className="h-4 w-4" />
                  </button>
                )}
              </li>
            ))}
          </ul>
        </section>
      ))}
    </div>
  );
}
