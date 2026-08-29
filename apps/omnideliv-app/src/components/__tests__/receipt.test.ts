import { settlementLine } from "../Receipt";

/**
 * One sentence decides whether someone answers the door holding cash. Getting
 * it wrong in either direction is a real-world failure: cash they didn't need,
 * or a courier they can't pay.
 */
describe("settlementLine", () => {
  it("asks for cash on a cash order", () => {
    const l = settlementLine({
      cod_amount_cents: 4200,
      payment_status: "pending",
      settled: false,
    });
    expect(l.owed).toBe(true);
    expect(l.text).toContain("₱42.00");
  });

  /** The amount asked for is the COD remainder, never the grand total. */
  it("asks only for the unpaid remainder of a partly prepaid order", () => {
    const l = settlementLine({
      cod_amount_cents: 200,
      payment_status: "captured",
      settled: false,
    });
    expect(l.text).toContain("₱2.00");
    expect(l.owed).toBe(true);
  });

  /**
   * The regression this file exists for: a fully prepaid order must never
   * produce a line that sends someone to find cash.
   */
  it("never asks a fully prepaid customer for cash, in any payment state", () => {
    for (const payment_status of ["pending", "authorized", "captured"] as const) {
      const l = settlementLine({ cod_amount_cents: 0, payment_status, settled: false });
      expect(l.owed).toBe(false);
      expect(l.text).not.toMatch(/cash/i);
    }
  });

  /** A held authorization is not money taken, and must not read as if it were. */
  it("distinguishes a hold from a completed charge", () => {
    const held = settlementLine({
      cod_amount_cents: 0,
      payment_status: "authorized",
      settled: false,
    });
    const taken = settlementLine({
      cod_amount_cents: 0,
      payment_status: "captured",
      settled: false,
    });
    expect(held.text).not.toBe(taken.text);
    expect(taken.text).toMatch(/paid/i);
  });

  it("says cash was handed over once a cash order is delivered", () => {
    const l = settlementLine({
      cod_amount_cents: 4200,
      payment_status: "pending",
      settled: true,
    });
    expect(l.owed).toBe(false);
  });
});
