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
