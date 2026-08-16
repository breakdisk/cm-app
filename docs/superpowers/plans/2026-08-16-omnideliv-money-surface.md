# OmniDeliv Read-Only Money Surface Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give an OmniDeliv customer an honest view of their money — what is owed at the door, what they have spent, and what each order was made of — without inventing a balance.

**Architecture:** Every number already exists on `omnideliv.orders`; only the grand total is currently serialized. This plan widens two existing reads by three columns each, adds a receipt component and a home-canvas panel, and adds a guard test that keeps a wallet from being "restored" later. **No new endpoints.**

**Tech Stack:** Rust (axum, sqlx), React Native 0.81 + Expo 54 + expo-router, TypeScript, jest-expo.

**Spec:** `docs/superpowers/specs/2026-08-16-omnideliv-telemetry-and-money-surface-design.md` (decision D1)

**Companion plan:** `docs/superpowers/plans/2026-08-16-omnideliv-live-telemetry.md` — independent, either order.

---

## Read this first — the thing this plan is protecting

On 2026-08-11 the LogisticOS customer app's Wallet screen was **deleted rather than repointed**. It was pointed at the *merchant settlement wallet*: `GET /v1/wallet` returns the tenant's balance and `POST /v1/wallet/withdraw` reserves against the tenant's funds. The screen rendered that as "WALLET BALANCE" with a **Request Withdrawal** button. The only thing between an end customer and the merchant's settlement money was `BILLING_MANAGE` being absent from the `customer` role.

The danger recorded at the time: **a broken customer-facing money screen invites the wrong repair.** The natural fix for "my wallet screen 403s" is to grant the permission — and that grant moves real money.

This plan therefore does two things at once: it ships the money surface a customer legitimately needs, and it makes the absence of a wallet *deliberate and tested* on this second surface rather than merely accidental.

**If you find yourself adding a balance, a top-up, or a withdraw button, stop.** That is a regulated product (BSP e-money, PH market) and needs its own spec and ADR, not a task in this plan.

**Build commands.** Always set `CARGO_INCREMENTAL=0` — the incremental cache fills the C: drive on this machine, and `link.exe` exit code 1318 is a disk-full error, not a code error.

---

## File structure

| File | Responsibility |
|---|---|
| `services/omnideliv/src/domain/repositories/mod.rs` *(modify)* | Widen `OrderSummary` with the breakdown |
| `services/omnideliv/src/infrastructure/db/order_repo.rs` *(modify)* | Select the three columns that already exist |
| `services/omnideliv/src/api/http/tracking.rs` *(modify)* | Serialize the breakdown on both reads |
| `apps/omnideliv-app/src/api/orders.ts` *(modify)* | Types for the widened list |
| `apps/omnideliv-app/src/components/Receipt.tsx` *(create)* | One receipt, used by two screens |
| `apps/omnideliv-app/src/money.ts` *(create)* | Pure: peso formatting and the month rollup |
| `apps/omnideliv-app/src/components/MoneyPanel.tsx` *(create)* | The home-canvas panel |
| `apps/omnideliv-app/app/index.tsx` *(modify)* | Mount the panel |
| `apps/omnideliv-app/app/track/[id].tsx` *(modify)* | Show the receipt |
| `apps/omnideliv-app/src/api/__tests__/no-wallet.test.ts` *(create)* | The guard |

---

## Task 1: Widen the two backend reads

**Files:**
- Modify: `services/omnideliv/src/domain/repositories/mod.rs`
- Modify: `services/omnideliv/src/infrastructure/db/order_repo.rs`
- Modify: `services/omnideliv/src/api/http/tracking.rs`

- [ ] **Step 1: Widen `OrderSummary`**

Find `pub struct OrderSummary` in `services/omnideliv/src/domain/repositories/mod.rs` and add three fields:

```rust
    /// The breakdown a receipt needs. Already columns on `omnideliv.orders` —
    /// the list simply never selected them, so a customer could see what they
    /// owed and never what for.
    pub goods_total_cents:  i64,
    pub delivery_fee_cents: i64,
    pub tip_cents:          i64,
```

- [ ] **Step 2: Run the build to verify it fails**

```bash
CARGO_INCREMENTAL=0 cargo check -p logisticos-omnideliv
```

Expected: FAIL — `missing fields goods_total_cents, delivery_fee_cents, tip_cents in initializer of OrderSummary`, pointing at `order_repo.rs`.

- [ ] **Step 3: Select the columns**

In `services/omnideliv/src/infrastructure/db/order_repo.rs`, inside `list_summaries_for_customer`, add to the `SELECT` list alongside `o.grand_total_cents`:

```sql
                   o.goods_total_cents  AS goods_total_cents,
                   o.delivery_fee_cents AS delivery_fee_cents,
                   o.tip_cents          AS tip_cents,
```

**Qualify every column with `o.`,** as the existing comment in that function demands: `orders`, `order_vendor_legs` and `vendors` all carry overlapping names, and an unqualified name across that three-way join is rejected outright as ambiguous.

The query already has `GROUP BY o.id`. These three columns are functionally dependent on the primary key, so Postgres accepts them without adding them to the `GROUP BY`. Do **not** add them to the `GROUP BY` clause — that would work but obscures why it is legal.

Then populate them in the row mapper below the query, following exactly how `grand_total_cents` is read:

```rust
            goods_total_cents:  r.get("goods_total_cents"),
            delivery_fee_cents: r.get("delivery_fee_cents"),
            tip_cents:          r.get("tip_cents"),
```

**Check the mapper's existing style before writing this.** If it uses `try_get` or a `FromRow` derive rather than `.get()`, match that. A column a mapper reads that no `SELECT` names is a known failure mode in this repo, and so is the reverse.

- [ ] **Step 4: Serialize on both HTTP reads**

In `services/omnideliv/src/api/http/tracking.rs`, add the same three fields to **both** `OrderListItem` and `TrackResponse`:

```rust
    pub goods_total_cents:  i64,
    pub delivery_fee_cents: i64,
    pub tip_cents:          i64,
```

Populate them in `my_orders` from the summary (`s.goods_total_cents`, and so on) and in `track` from the order (`order.goods_total_cents`, and so on).

- [ ] **Step 5: Build**

```bash
CARGO_INCREMENTAL=0 cargo check -p logisticos-omnideliv
```

Expected: clean.

- [ ] **Step 6: Verify the arithmetic holds on real data**

Against a running stack, confirm the parts sum to the whole. If they do not, the receipt would show a total that contradicts its own lines, which is worse than showing no receipt.

```bash
curl -s -H "Authorization: Bearer $TOKEN" http://localhost:8000/v1/omnideliv/orders \
  | python -c "import json,sys; [print(o['goods_total_cents']+o['delivery_fee_cents']+o['tip_cents'], o['grand_total_cents']) for o in json.load(sys.stdin)]"
```

Expected: the two numbers match on every row. If they do not, stop and find out what else is in the grand total before building a receipt on it — report the discrepancy rather than adding a rounding line to hide it.

- [ ] **Step 7: Commit**

```bash
git add services/omnideliv/src/domain/repositories/mod.rs services/omnideliv/src/infrastructure/db/order_repo.rs services/omnideliv/src/api/http/tracking.rs
git commit -m "feat(omnideliv): serialize the order money breakdown on both customer reads"
```

---

## Task 2: Pure money helpers

Formatting and the month rollup are pure and get real tests. The components that use them do not need to be tested for arithmetic.

**Files:**
- Create: `apps/omnideliv-app/src/money.ts`
- Create: `apps/omnideliv-app/src/__tests__/money.test.ts`

- [ ] **Step 1: Write the failing tests**

`apps/omnideliv-app/src/__tests__/money.test.ts`:

```ts
import { peso, monthToDate, cashDue } from "../money";
import type { OrderListItem } from "../api/orders";

function order(p: Partial<OrderListItem>): OrderListItem {
  return {
    order_id: "id",
    status: "delivered",
    grand_total_cents: 1000,
    goods_total_cents: 800,
    delivery_fee_cents: 150,
    tip_cents: 50,
    stops_total: 1,
    vendor_names: "Kuya's",
    placed_at: new Date().toISOString(),
    delivered_at: null,
    ...p,
  };
}

describe("peso", () => {
  it("formats cents as pesos with two decimals", () => {
    expect(peso(41200)).toBe("₱412.00");
    expect(peso(0)).toBe("₱0.00");
    expect(peso(5)).toBe("₱0.05");
  });
});

describe("monthToDate", () => {
  it("counts only delivered orders", () => {
    const rollup = monthToDate([
      order({ status: "delivered", grand_total_cents: 1000 }),
      order({ status: "cancelled", grand_total_cents: 9999 }),
      order({ status: "delivering", grand_total_cents: 5000 }),
    ]);
    expect(rollup.cents).toBe(1000);
    expect(rollup.count).toBe(1);
  });

  it("excludes orders from previous months", () => {
    const old = new Date();
    old.setMonth(old.getMonth() - 2);
    const rollup = monthToDate([
      order({ status: "delivered", grand_total_cents: 1000 }),
      order({ status: "delivered", grand_total_cents: 7777, placed_at: old.toISOString() }),
    ]);
    expect(rollup.cents).toBe(1000);
  });

  it("is zero for no orders rather than throwing", () => {
    expect(monthToDate([])).toEqual({ cents: 0, count: 0 });
  });
});

describe("cashDue", () => {
  it("finds the order still in flight", () => {
    const inflight = order({ status: "delivering", order_id: "live", grand_total_cents: 4200 });
    expect(cashDue([order({}), inflight])?.order_id).toBe("live");
  });

  it("is null when everything is finished", () => {
    expect(cashDue([order({ status: "delivered" }), order({ status: "cancelled" })])).toBeNull();
  });

  /** Newest first is how the list arrives; the newest live one is the answer. */
  it("picks the most recent when several are in flight", () => {
    const newer = order({ status: "collecting", order_id: "newer" });
    const older = order({ status: "delivering", order_id: "older" });
    expect(cashDue([newer, older])?.order_id).toBe("newer");
  });
});
```

- [ ] **Step 2: Run to verify it fails**

```bash
cd apps/omnideliv-app && npx jest src/__tests__/money.test.ts
```

Expected: FAIL — `Cannot find module '../money'`.

- [ ] **Step 3: Write the implementation**

`apps/omnideliv-app/src/money.ts`:

```ts
/**
 * The customer's money, as arithmetic.
 *
 * There is no balance here and there is not meant to be. Every OmniDeliv order
 * is cash on delivery: the only real numbers are what is owed at the door and
 * what has already been handed over. See decision D1 in the spec — a customer
 * stored-value wallet is a regulated product, not a screen.
 */
import type { OrderListItem } from "./api/orders";

/** Statuses where the courier has not yet been paid at the door. */
const IN_FLIGHT = ["placed", "awaiting_courier", "collecting", "delivering"];

export function peso(cents: number): string {
  return `₱${(cents / 100).toFixed(2)}`;
}

export interface Rollup {
  cents: number;
  count: number;
}

/**
 * What has actually been spent this calendar month.
 *
 * Delivered only. Counting an in-flight order would tell someone they have
 * spent money they are still holding, and counting a cancelled one would be
 * simply false.
 */
export function monthToDate(orders: OrderListItem[]): Rollup {
  const now = new Date();
  return orders.reduce<Rollup>(
    (acc, o) => {
      if (o.status !== "delivered") return acc;
      const at = new Date(o.placed_at);
      if (at.getMonth() !== now.getMonth() || at.getFullYear() !== now.getFullYear()) return acc;
      return { cents: acc.cents + o.grand_total_cents, count: acc.count + 1 };
    },
    { cents: 0, count: 0 },
  );
}

/**
 * The order whose cash is still owed, if any.
 *
 * The list arrives newest first, so the first match is the most recent — which
 * is the one someone is about to answer the door for.
 */
export function cashDue(orders: OrderListItem[]): OrderListItem | null {
  return orders.find((o) => IN_FLIGHT.includes(o.status)) ?? null;
}
```

- [ ] **Step 4: Add the types the tests assume**

In `apps/omnideliv-app/src/api/orders.ts`, add the three fields to `OrderListItem`:

```ts
  goods_total_cents: number;
  delivery_fee_cents: number;
  tip_cents: number;
```

Read the existing interface first and place them beside `grand_total_cents`.

- [ ] **Step 5: Run to verify it passes**

```bash
cd apps/omnideliv-app && npx tsc --noEmit && npx jest src/__tests__/money.test.ts
```

Expected: PASS, 7 tests.

- [ ] **Step 6: Mutation-check the delivered filter**

Change `if (o.status !== "delivered") return acc;` to `if (o.status === "cancelled") return acc;` and re-run.

Expected: `counts only delivered orders` FAILS. Revert and confirm green.

- [ ] **Step 7: Commit**

```bash
git add apps/omnideliv-app/src/money.ts apps/omnideliv-app/src/__tests__/money.test.ts apps/omnideliv-app/src/api/orders.ts
git commit -m "feat(omnideliv-app): pure money helpers for spend and cash due"
```

---

## Task 3: The receipt

**Files:**
- Create: `apps/omnideliv-app/src/components/Receipt.tsx`
- Modify: `apps/omnideliv-app/app/track/[id].tsx`

- [ ] **Step 1: Write the component**

`apps/omnideliv-app/src/components/Receipt.tsx`:

```tsx
/**
 * What an order was made of.
 *
 * The grand total alone told a customer what they owed and never what for —
 * which reads as a mistake the moment a modifier has folded a large-size delta
 * into a line price and the number no longer matches the menu.
 *
 * The rail is named out loud on the last line. A money panel that does not say
 * how it is paid is what invites the assumption that a balance sits behind it.
 */
import { Text, View } from "react-native";

import { peso } from "@/money";
import { theme } from "@/theme";

export interface ReceiptProps {
  goods_total_cents: number;
  delivery_fee_cents: number;
  tip_cents: number;
  grand_total_cents: number;
  /** Cash is only still owed while the order is in flight. */
  settled: boolean;
}

export function Receipt(p: ReceiptProps) {
  return (
    <View
      style={{
        backgroundColor: theme.surface,
        borderColor: theme.border,
        borderWidth: 1,
        borderRadius: theme.radius.md,
        padding: 14,
        gap: 8,
      }}
    >
      <Line label="Goods" value={peso(p.goods_total_cents)} />
      <Line label="Delivery fee" value={peso(p.delivery_fee_cents)} />
      {/* Zero tip is shown rather than hidden: a missing line reads as a
          number that was rolled into something else. */}
      <Line label="Tip" value={peso(p.tip_cents)} />

      <View style={{ height: 1, backgroundColor: theme.border, marginVertical: 2 }} />

      <View style={{ flexDirection: "row", justifyContent: "space-between" }}>
        <Text style={{ color: theme.text, fontSize: 14, fontWeight: "800" }}>Total</Text>
        <Text style={{ color: theme.text, fontSize: 14, fontWeight: "800" }}>
          {peso(p.grand_total_cents)}
        </Text>
      </View>

      <Text style={{ color: p.settled ? theme.faint : theme.amber, fontSize: 12 }}>
        {p.settled
          ? "Paid in cash on delivery"
          : `Please have ${peso(p.grand_total_cents)} in cash ready.`}
      </Text>
    </View>
  );
}

function Line({ label, value }: { label: string; value: string }) {
  return (
    <View style={{ flexDirection: "row", justifyContent: "space-between" }}>
      <Text style={{ color: theme.muted, fontSize: 13 }}>{label}</Text>
      <Text style={{ color: theme.text, fontSize: 13, fontWeight: "700" }}>{value}</Text>
    </View>
  );
}
```

Check `apps/omnideliv-app/src/theme.ts` for the real token names (`surface`, `border`, `text`, `muted`, `faint`, `amber`, `radius.md`) and use those — do not invent one.

- [ ] **Step 2: Replace the totals card on the track screen**

In `apps/omnideliv-app/app/track/[id].tsx`, delete the existing totals `View` — the one containing the `Row label="Total"` and the amber "Please have … in cash ready" text — and replace it with:

```tsx
        <Receipt
          goods_total_cents={order.goods_total_cents}
          delivery_fee_cents={order.delivery_fee_cents}
          tip_cents={order.tip_cents}
          grand_total_cents={order.grand_total_cents}
          settled={order.status === "delivered"}
        />
```

Keep the "Stops collected" row — move it above the receipt as its own line, since it is progress rather than money:

```tsx
        <Text style={{ color: theme.muted, fontSize: 13 }}>
          {order.stops_collected} of {order.stops_total} stops collected
        </Text>
```

Import the component and add the three fields to the `TrackResponse` interface in `src/api/tracking.ts`:

```ts
  goods_total_cents: number;
  delivery_fee_cents: number;
  tip_cents: number;
```

If the local `Row` helper at the bottom of the track screen is now unused, delete it. Leaving a dead helper behind is how a file grows into something nobody can read.

- [ ] **Step 3: Typecheck and test**

```bash
cd apps/omnideliv-app && npx tsc --noEmit && npx jest
```

Expected: both clean.

- [ ] **Step 4: Commit**

```bash
git add apps/omnideliv-app/src/components/Receipt.tsx apps/omnideliv-app/app/track/\[id\].tsx apps/omnideliv-app/src/api/tracking.ts
git commit -m "feat(omnideliv-app): a receipt that says what an order was made of"
```

---

## Task 4: The home canvas money panel

**Files:**
- Create: `apps/omnideliv-app/src/components/MoneyPanel.tsx`
- Modify: `apps/omnideliv-app/app/index.tsx`

- [ ] **Step 1: Write the panel**

`apps/omnideliv-app/src/components/MoneyPanel.tsx`:

```tsx
/**
 * The customer's money on the home canvas.
 *
 * Deliberately not a wallet. There is no balance, no top-up and no withdraw,
 * because OmniDeliv has no rail behind any of them — every order is cash on
 * delivery, payment capture is deferred and refunds do not exist. See spec
 * decision D1. The nearest thing to a balance a customer has is the cash they
 * are about to hand over, so that is what this shows.
 */
import { useCallback, useEffect, useState } from "react";
import { Pressable, Text, View } from "react-native";
import { router } from "expo-router";

import { listMyOrders, type OrderListItem } from "@/api/orders";
import { cashDue, monthToDate, peso } from "@/money";
import { theme } from "@/theme";

export function MoneyPanel() {
  const [orders, setOrders] = useState<OrderListItem[] | null>(null);

  const load = useCallback(async () => {
    try {
      setOrders(await listMyOrders());
    } catch {
      // Silent. This panel is context on a screen whose job is taking an
      // order; a failed rollup must not put an error in front of that.
      setOrders([]);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  if (orders === null) return null;

  const due = cashDue(orders);
  const month = monthToDate(orders);

  // Nothing owed and nothing spent yet — say nothing rather than showing a
  // ₱0.00 that looks like an empty balance.
  if (!due && month.count === 0) return null;

  return (
    <View
      style={{
        borderWidth: 1,
        borderColor: "rgba(255,255,255,0.08)",
        borderRadius: theme.radius.md,
        overflow: "hidden",
      }}
    >
      {due && (
        <Pressable
          onPress={() => router.push(`/track/${due.order_id}`)}
          accessibilityRole="button"
          accessibilityLabel={`Cash due at the door, ${peso(due.grand_total_cents)}`}
          style={{ padding: 14, gap: 2 }}
        >
          <Text style={{ color: theme.amber, fontSize: 10, letterSpacing: 1.2 }}>
            CASH DUE AT THE DOOR
          </Text>
          <Text style={{ color: theme.text, fontSize: 24, fontWeight: "800" }}>
            {peso(due.grand_total_cents)}
          </Text>
          <Text numberOfLines={1} style={{ color: theme.muted, fontSize: 12 }}>
            {due.vendor_names || "Order"} · on the way
          </Text>
        </Pressable>
      )}

      {month.count > 0 && (
        <View
          style={{
            padding: 14,
            borderTopWidth: due ? 1 : 0,
            borderTopColor: "rgba(255,255,255,0.08)",
            flexDirection: "row",
            justifyContent: "space-between",
            alignItems: "center",
          }}
        >
          <Text style={{ color: theme.muted, fontSize: 12 }}>
            This month · {month.count} {month.count === 1 ? "order" : "orders"}
          </Text>
          <Text style={{ color: theme.text, fontSize: 14, fontWeight: "700" }}>
            {peso(month.cents)}
          </Text>
        </View>
      )}
    </View>
  );
}
```

- [ ] **Step 2: Mount it on the home canvas**

In `apps/omnideliv-app/app/index.tsx`, add the import and place `<MoneyPanel />` directly **above** the existing "Your orders →" pressable — money is context for the order history that follows it, and both sit below the intent input which is the screen's primary job.

```tsx
import { MoneyPanel } from "@/components/MoneyPanel";
```

```tsx
        <MoneyPanel />
```

- [ ] **Step 3: Typecheck and test**

```bash
cd apps/omnideliv-app && npx tsc --noEmit && npx jest
```

Expected: both clean.

- [ ] **Step 4: Audit responsiveness**

Required by CLAUDE.md. Check the home canvas at a small viewport (≤360 dp wide) and a large one:

- the vendor line truncates (`numberOfLines={1}`) rather than wrapping the panel to three lines
- the month row keeps its label and amount on one line without the amount clipping
- the panel does not push the intent input off-screen when both rows are present
- the whole canvas remains reachable — if it now overflows, the container needs to scroll

Fix anything that breaks before committing.

- [ ] **Step 5: Commit**

```bash
git add apps/omnideliv-app/src/components/MoneyPanel.tsx apps/omnideliv-app/app/index.tsx
git commit -m "feat(omnideliv-app): cash due and monthly spend on the home canvas"
```

---

## Task 5: The guard

This test is the point of the plan as much as the panel is. Its job is to fail in six months when someone reads "unified digital wallet" in a backlog and starts wiring one.

**Files:**
- Create: `apps/omnideliv-app/src/api/__tests__/no-wallet.test.ts`

- [ ] **Step 1: Write the test**

```ts
/**
 * The OmniDeliv customer surface has no wallet, and that is a decision.
 *
 * On 2026-08-11 the LogisticOS customer app's Wallet screen was deleted rather
 * than repointed: it was showing the *merchant settlement wallet* — tenant
 * balance, with a working Request Withdrawal button — and the only thing
 * standing between a customer and that money was a permission the customer role
 * happens not to hold.
 *
 * That is guarded on the identity side by a role test. This is the second
 * surface where the same wrong repair is tempting, so it is guarded here too.
 *
 * If this test fails because someone is building a customer wallet: that is a
 * regulated stored-value product (BSP e-money in the PH market) needing a top-up
 * rail, refund-to-balance semantics and its own ADR. Reopen spec decision D1
 * rather than deleting this test.
 */
import fs from "fs";
import path from "path";

const SRC = path.resolve(__dirname, "../..");
const APP = path.resolve(__dirname, "../../../app");

function sourceFiles(dir: string): string[] {
  return fs.readdirSync(dir, { withFileTypes: true }).flatMap((e) => {
    const full = path.join(dir, e.name);
    if (e.isDirectory()) return e.name === "__tests__" ? [] : sourceFiles(full);
    return /\.tsx?$/.test(e.name) ? [full] : [];
  });
}

const sources = [...sourceFiles(SRC), ...sourceFiles(APP)].map((f) => ({
  file: f,
  text: fs.readFileSync(f, "utf8"),
}));

describe("the customer app has no wallet", () => {
  it("calls no wallet, balance, top-up or withdraw endpoint", () => {
    const offenders = sources.filter((s) =>
      /["'`][^"'`]*\/v1\/(wallet|balance|top-?up|withdraw)/i.test(s.text),
    );
    expect(offenders.map((o) => o.file)).toEqual([]);
  });

  it("renders no withdraw or top-up control", () => {
    const offenders = sources.filter((s) => /withdraw|top[\s-]?up/i.test(s.text));
    expect(offenders.map((o) => o.file)).toEqual([]);
  });

  /** The panel that legitimately shows money must not call itself a balance. */
  it("does not present a balance", () => {
    const offenders = sources.filter((s) => /wallet balance|available balance/i.test(s.text));
    expect(offenders.map((o) => o.file)).toEqual([]);
  });
});
```

- [ ] **Step 2: Run it**

```bash
cd apps/omnideliv-app && npx jest src/api/__tests__/no-wallet.test.ts
```

Expected: PASS, 3 tests.

If it fails, read the offending file paths it prints. If the only hits are this plan's own doc comments, adjust the exclusion so comments in `__tests__` are skipped — the existing `sourceFiles` already skips `__tests__` directories, so a hit means real source.

- [ ] **Step 3: Mutation-check all three**

The guard must be seen to fail. Temporarily add to `apps/omnideliv-app/src/api/orders.ts`:

```ts
export const TEMP = "/v1/wallet/withdraw"; // Wallet balance
```

Re-run.

Expected: **all three tests FAIL**, each naming `orders.ts`. If any one passes, its pattern is wrong — fix it before removing the line.

Remove the line and confirm green.

- [ ] **Step 4: Commit**

```bash
git add apps/omnideliv-app/src/api/__tests__/no-wallet.test.ts
git commit -m "test(omnideliv-app): guard the decision that customers have no wallet"
```

---

## Task 6: End-to-end verification

No new code. A green build has repeatedly not meant working software in this repo.

- [ ] **Step 1: Full test run**

```bash
CARGO_INCREMENTAL=0 cargo test -p logisticos-omnideliv
```

```bash
cd apps/omnideliv-app && npx tsc --noEmit && npx jest
```

Expected: all green. Record the actual counts — do not claim a pass you have not read.

- [ ] **Step 2: Clippy**

```bash
CARGO_INCREMENTAL=0 cargo clippy -p logisticos-omnideliv -- -D warnings
```

Expected: clean.

- [ ] **Step 3: Confirm both reads carry the breakdown**

```bash
curl -s -H "Authorization: Bearer $TOKEN" http://localhost:8000/v1/omnideliv/orders | head -c 400
curl -s -H "Authorization: Bearer $TOKEN" http://localhost:8000/v1/omnideliv/orders/$ORDER_ID/track | head -c 400
```

Expected: `goods_total_cents`, `delivery_fee_cents` and `tip_cents` present in both, and summing to `grand_total_cents`.

- [ ] **Step 4: Confirm the app bundle resolves the new imports**

`tsc --noEmit` has passed through three separate states that made this app unbuildable, and `expo export` cannot run on this Windows machine — `hermesc` rejects `#private` fields in a dependency regardless of your code, so a failure there is not yours.

```bash
cd apps/omnideliv-app && npx expo-doctor
```

Expected: no new failures versus before this branch. Then start Metro and compare the module count against a `git stash`ed baseline — a count that did not rise means an import was silently dropped rather than resolved.

- [ ] **Step 5: Stop and hand back**

Do not merge. Report the test counts, the curl output, and the module-count delta.

---

## Notes for the implementer

- **If the live-telemetry plan has already run**, the track screen has a map above the totals card and `TrackResponse` already carries `courier`, `eta`, `destination` and `stops`. Task 3 replaces the totals card only — leave the map and the milestone strip alone. The two plans extend the same interface with different fields and do not collide in either order.
- **`grand_total_cents` stays the source of truth for what is owed.** The three new fields are for explanation. If they ever disagree with the total, show the total and report the discrepancy — never silently reconcile them with a rounding line.
- **The month rollup is client-side on the existing 50-order list.** That is correct at this scale and wrong at some larger one. When it becomes wrong the fix is a server-side aggregate, not a bigger `LIMIT`.
- **Do not add a "Payment method" row.** There is exactly one rail and the receipt names it. A row implying a choice would be the first step towards a wallet nobody decided to build.
- **Carbon offset and consolidation bonuses are explicitly out of scope**, with reasons recorded in the spec's Not Building section. Consolidation is a margin lever by deliberate design, with a test asserting a three-stop plan never costs more than a one-stop one — a customer-facing bonus needs that decision reopened first.
