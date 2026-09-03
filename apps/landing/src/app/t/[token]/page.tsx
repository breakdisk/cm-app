"use client";
/**
 * `/t/:token` — what a printed table QR actually opens.
 *
 * This route is the reason the rest of QR table ordering existed but did not
 * work. The codes encoded `{TABLE_SCAN_BASE_URL}/t/{token}` and **nothing
 * served that path**: a diner scanning a sticker got a 404 from a feature that
 * was otherwise complete and deployed.
 *
 * It lives in the landing app because that app owns the public routes on the
 * public host — the same reason `/track` is here — and because the whole point
 * of table ordering is that a diner needs no app install. The native app has
 * scheme `omnideliv`; a stranger at a restaurant does not have it.
 *
 * ## Trust
 *
 * The scan mints a **table-session** principal: no roles, no permissions, and
 * bounded at the gateway by a deny-by-default allowlist. Everything this page
 * calls — catalog search, baskets, checkout, track — is on that allowlist by
 * design, and nothing else is reachable with the token even if this code asked.
 *
 * The token is held in `sessionStorage`, not `localStorage`: it is a bearer
 * credential for a table someone is currently sitting at, and it should die
 * with the tab rather than linger on a phone that has left the building.
 */
import { useCallback, useEffect, useState } from "react";
import { useParams } from "next/navigation";
import {
  AlertCircle,
  CheckCircle2,
  ChefHat,
  Loader2,
  Minus,
  ShoppingBag,
  UtensilsCrossed,
} from "lucide-react";

import { API_BASE } from "@/lib/api-base";
import { MenuList, money, type MenuItem } from "@/components/menu-list";

interface VendorBrief {
  vendor_id: string;
  name: string;
}

interface ScanResponse {
  session_id: string;
  access_token: string;
  expires_at: string;
  venue_id: string;
  venue_name: string;
  table_label: string;
  vendors: VendorBrief[];
}

/** The menu fields plus what only this page needs. */
interface SearchHit extends MenuItem {
  availability: string;
}

interface BasketLine {
  id: string;
  item_id: string;
  name: string;
  qty: number;
  subtotal_cents: number;
}

interface Basket {
  id: string;
  goods_total_cents: number;
  lines: BasketLine[];
}

interface PlacedOrder {
  order_id: string;
  grand_total_cents: number;
}

export default function TableOrderPage() {
  const params = useParams<{ token: string }>();
  const token = params.token;

  const [session, setSession] = useState<ScanResponse | null>(null);
  const [scanError, setScanError] = useState<string | null>(null);
  const [scanning, setScanning] = useState(true);

  const [vendorId, setVendorId] = useState<string | null>(null);
  const [items, setItems] = useState<SearchHit[]>([]);
  const [menuLoading, setMenuLoading] = useState(false);

  const [basket, setBasket] = useState<Basket | null>(null);
  const [busyItem, setBusyItem] = useState<string | null>(null);
  const [placing, setPlacing] = useState(false);
  const [placed, setPlaced] = useState<PlacedOrder | null>(null);
  const [error, setError] = useState<string | null>(null);

  const storageKey = `omnideliv.table.${token}`;

  /** Authenticated call as the diner. */
  const call = useCallback(
    async <T,>(path: string, init?: RequestInit): Promise<T> => {
      if (!session) throw new Error("No table session.");
      const res = await fetch(`${API_BASE}${path}`, {
        ...init,
        headers: {
          "Content-Type": "application/json",
          Authorization: `Bearer ${session.access_token}`,
          ...(init?.headers ?? {}),
        },
      });
      if (res.status === 401) {
        // The session row is the record, not the JWT — an ended or expired
        // session stops ordering even while the token still verifies.
        sessionStorage.removeItem(storageKey);
        throw new Error("This table session has ended. Scan the code again.");
      }
      if (!res.ok) {
        throw new Error((await res.text().catch(() => "")) || "Something went wrong.");
      }
      return res.status === 204 ? (undefined as T) : ((await res.json()) as T);
    },
    [session, storageKey],
  );

  // --- 1. Open the session ------------------------------------------------
  useEffect(() => {
    let cancelled = false;

    const cached = (() => {
      try {
        const raw = sessionStorage.getItem(storageKey);
        if (!raw) return null;
        const parsed = JSON.parse(raw) as ScanResponse;
        // A cached token past its expiry is worse than none: it produces a 401
        // on the first real action instead of a clean rescan.
        return new Date(parsed.expires_at) > new Date() ? parsed : null;
      } catch {
        return null;
      }
    })();

    if (cached) {
      setSession(cached);
      setVendorId(cached.vendors[0]?.vendor_id ?? null);
      setScanning(false);
      return;
    }

    // Not a GET: opening a session writes a row, and the cap that bounds this
    // endpoint counts rows.
    fetch(`${API_BASE}/v1/omnideliv/tables/${encodeURIComponent(token)}/session`, {
      method: "POST",
    })
      .then(async (res) => {
        if (cancelled) return;
        if (res.status === 429) {
          setScanError(
            "This code has been scanned a lot in the last minute. Wait a moment and try again.",
          );
          return;
        }
        if (!res.ok) {
          // Every refusal is one indistinguishable 404 on purpose — unknown
          // code, closed table, paused venue, outside hours. So the message
          // has to cover all of them without guessing which.
          setScanError(
            "This code is not taking orders right now. It may be outside opening hours, or the code may have been replaced. Ask a member of staff.",
          );
          return;
        }
        const s = (await res.json()) as ScanResponse;
        try {
          sessionStorage.setItem(storageKey, JSON.stringify(s));
        } catch {
          // Private browsing. The session still works for this page load.
        }
        setSession(s);
        setVendorId(s.vendors[0]?.vendor_id ?? null);
      })
      .catch(() => {
        if (!cancelled) setScanError("Could not reach the restaurant. Check your signal.");
      })
      .finally(() => {
        if (!cancelled) setScanning(false);
      });

    return () => {
      cancelled = true;
    };
  }, [token, storageKey]);

  // --- 2. Menu for the selected vendor ------------------------------------
  useEffect(() => {
    if (!session || !vendorId) return;
    setMenuLoading(true);
    call<SearchHit[]>(
      `/v1/omnideliv/catalog/search?vendor_id=${vendorId}&q=&limit=100`,
    )
      .then((hits) => setItems(hits.filter((h) => h.availability !== "out_of_stock")))
      .catch((e) => setError(e instanceof Error ? e.message : "Could not load the menu"))
      .finally(() => setMenuLoading(false));
  }, [session, vendorId, call]);

  // --- 3. Basket ----------------------------------------------------------
  // Takes the shared shape, not this page's: adding a line needs only the id.
  const addItem = async (item: MenuItem) => {
    if (!vendorId) return;
    setBusyItem(item.item_id);
    setError(null);
    try {
      // Created lazily on the first add, so a diner who only browses never
      // leaves an empty basket behind.
      let id = basket?.id;
      if (!id) {
        id = (await call<Basket>("/v1/omnideliv/baskets", { method: "POST" })).id;
      }
      const updated = await call<Basket>(`/v1/omnideliv/baskets/${id}/lines`, {
        method: "POST",
        body: JSON.stringify({ vendor_id: vendorId, item_id: item.item_id, qty: 1 }),
      });
      setBasket(updated);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not add that");
    } finally {
      setBusyItem(null);
    }
  };

  const removeLine = async (line: BasketLine) => {
    if (!basket) return;
    setBusyItem(line.item_id);
    setError(null);
    try {
      // The delete returns the updated basket, so re-fetching would be both a
      // wasted round trip and a race against another diner at the same table.
      setBasket(
        await call<Basket>(`/v1/omnideliv/baskets/${basket.id}/lines/${line.id}`, {
          method: "DELETE",
        }),
      );
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not remove that");
    } finally {
      setBusyItem(null);
    }
  };

  // --- 4. Order -----------------------------------------------------------
  const placeOrder = async () => {
    if (!basket) return;
    setPlacing(true);
    setError(null);
    try {
      // No coordinates: a diner has none, and the server derives dine-in from
      // the principal rather than anything sent here. No delivery fee is
      // charged and no courier is dispatched.
      const order = await call<PlacedOrder>("/v1/omnideliv/orders/checkout", {
        method: "POST",
        body: JSON.stringify({ basket_id: basket.id, payment_method: "cod" }),
      });
      setPlaced(order);
      setBasket(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not place the order");
    } finally {
      setPlacing(false);
    }
  };

  // --- Render -------------------------------------------------------------
  if (scanning) {
    return (
      <Shell>
        <div className="flex flex-col items-center gap-3 py-24 text-center">
          <Loader2 className="h-7 w-7 animate-spin text-cyan-400" />
          <p className="text-sm text-white/50">Opening your table…</p>
        </div>
      </Shell>
    );
  }

  if (scanError || !session) {
    return (
      <Shell>
        <div className="flex flex-col items-center gap-4 py-20 text-center">
          <AlertCircle className="h-10 w-10 text-amber-400" />
          <h1 className="font-heading text-xl font-semibold text-white">
            Can&apos;t open this table
          </h1>
          <p className="max-w-sm text-sm text-white/50">{scanError}</p>
        </div>
      </Shell>
    );
  }

  if (placed) {
    return (
      <Shell>
        <div className="flex flex-col items-center gap-4 py-20 text-center">
          <CheckCircle2 className="h-12 w-12 text-emerald-400" />
          <h1 className="font-heading text-2xl font-semibold text-white">
            Order sent to the kitchen
          </h1>
          <p className="text-sm text-white/50">
            {session.venue_name} · {session.table_label}
          </p>
          <p className="text-lg font-semibold text-white">
            {money(placed.grand_total_cents)}
          </p>
          <p className="max-w-sm text-sm text-white/40">
            Pay at the table when your food arrives. Staff can see this order now.
          </p>
        </div>
      </Shell>
    );
  }

  const lineCount = basket?.lines.reduce((n, l) => n + l.qty, 0) ?? 0;

  return (
    <Shell>
      <header className="flex items-center gap-3 border-b border-white/10 pb-4">
        <span className="rounded-xl border border-white/10 bg-white/5 p-2.5 text-cyan-400">
          <UtensilsCrossed className="h-5 w-5" />
        </span>
        <div className="min-w-0">
          <h1 className="truncate font-heading text-xl font-semibold text-white">
            {session.venue_name}
          </h1>
          <p className="text-sm text-white/50">Table {session.table_label}</p>
        </div>
      </header>

      {session.vendors.length === 0 ? (
        <div className="flex flex-col items-center gap-3 py-20 text-center">
          <ChefHat className="h-9 w-9 text-white/25" />
          <p className="text-sm text-white/50">
            Nothing is available to order at this venue yet.
          </p>
          <p className="text-xs text-white/30">Please ask a member of staff.</p>
        </div>
      ) : (
        <>
          {session.vendors.length > 1 && (
            <nav className="-mx-4 flex gap-2 overflow-x-auto px-4 pb-1">
              {session.vendors.map((v) => (
                <button
                  key={v.vendor_id}
                  onClick={() => setVendorId(v.vendor_id)}
                  className={`shrink-0 rounded-full border px-4 py-1.5 text-sm transition ${
                    v.vendor_id === vendorId
                      ? "border-cyan-400/50 bg-cyan-400/15 text-cyan-300"
                      : "border-white/10 bg-white/5 text-white/60"
                  }`}
                >
                  {v.name}
                </button>
              ))}
            </nav>
          )}

          {error && (
            <p className="rounded-lg border border-amber-400/30 bg-amber-400/10 px-3 py-2 text-sm text-amber-300">
              {error}
            </p>
          )}

          {menuLoading ? (
            <div className="flex justify-center py-16">
              <Loader2 className="h-6 w-6 animate-spin text-white/30" />
            </div>
          ) : (
            // Same component the public storefront renders, so a menu looks
            // the same whether you reached it from a table or from a link
            // somebody forwarded. `pb-40` clears the fixed basket bar, which
            // only exists on this page.
            <div className="pb-40">
              <MenuList items={items} onAdd={addItem} busyItemId={busyItem} />
            </div>
          )}
        </>
      )}

      {basket && basket.lines.length > 0 && (
        // Fixed to the bottom: a phone held at a table is the only viewport
        // this page ever gets, and the order button must never be scrolled off.
        <div className="fixed inset-x-0 bottom-0 border-t border-white/10 bg-[#050810]/95 backdrop-blur-xl">
          <div className="mx-auto max-w-lg space-y-2 p-4">
            <ul className="max-h-40 space-y-1 overflow-y-auto">
              {basket.lines.map((l) => (
                <li
                  key={l.id}
                  className="flex items-center gap-2 text-sm text-white/70"
                >
                  <span className="text-white/40">{l.qty}×</span>
                  <span className="min-w-0 flex-1 truncate">{l.name}</span>
                  <span>{money(l.subtotal_cents)}</span>
                  <button
                    onClick={() => removeLine(l)}
                    disabled={busyItem === l.item_id}
                    aria-label={`Remove ${l.name}`}
                    className="rounded p-1 text-white/30 transition hover:text-red-400 disabled:opacity-40"
                  >
                    <Minus className="h-3.5 w-3.5" />
                  </button>
                </li>
              ))}
            </ul>
            <button
              onClick={placeOrder}
              disabled={placing}
              className="flex w-full items-center justify-center gap-2 rounded-xl bg-cyan-400 py-3 font-semibold text-[#050810] transition active:scale-[0.99] disabled:opacity-50"
            >
              {placing ? (
                <Loader2 className="h-5 w-5 animate-spin" />
              ) : (
                <ShoppingBag className="h-5 w-5" />
              )}
              Order {lineCount} {lineCount === 1 ? "item" : "items"} ·{" "}
              {money(basket.goods_total_cents)}
            </button>
            <p className="text-center text-xs text-white/30">
              Pay at the table. No delivery fee.
            </p>
          </div>
        </div>
      )}
    </Shell>
  );
}

/** Phone-first: this page is only ever opened by a camera at a table. */
function Shell({ children }: { children: React.ReactNode }) {
  return (
    <main className="min-h-screen bg-[#050810]">
      <div className="mx-auto max-w-lg space-y-4 px-4 py-6">{children}</div>
    </main>
  );
}
