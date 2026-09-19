import { describe, expect, jest, test } from "@jest/globals";
jest.mock("@/lib/auth/auth-fetch", () => ({ authFetch: jest.fn() }));

import { amountToCents, bpsToPercent, ladderProblem, offerSummary, percentToBps, type TierRung } from "@/lib/api/promotions";

const rung = (over: Partial<TierRung> = {}): TierRung => ({
  name: "Gold", min_moves: 7, perk: "", accessorial_bps: 1000, cap_cents: 50000, referral_multiplier: 2, ...over,
});

describe("units", () => {
  test("percent and basis points round-trip", () => {
    expect(percentToBps("12.5")).toBe(1250);
    expect(bpsToPercent(1250)).toBe("12.5");
    expect(bpsToPercent(2000)).toBe("20");
    expect(percentToBps("")).toBeNaN();
    expect(percentToBps("101")).toBeNaN();
  });

  test("an amount becomes minor units, and junk does not", () => {
    expect(amountToCents("1,500.50")).toBe(150050);
    expect(amountToCents("200")).toBe(20000);
    expect(amountToCents("-5")).toBeNaN();
    expect(amountToCents("1.234")).toBeNaN();
  });

  test("an offer reads as what it takes off", () => {
    expect(offerSummary({ discount: { kind: "percent_carriage", bps: 2000 }, cap_cents: 25000 })).toMatch(/^20% off carriage, up to 250/);
    expect(offerSummary({ discount: { kind: "flat", cents: 14000 }, cap_cents: null })).toMatch(/^140(\.00)? off$/);
  });
});

describe("ladderProblem matches the server's rules", () => {
  test("a good ladder passes", () => {
    expect(ladderProblem([rung({ name: "Silver", min_moves: 3 }), rung()])).toBeNull();
  });
  test("two tiers at one threshold", () => {
    expect(ladderProblem([rung({ name: "A" }), rung({ name: "B" })])).toMatch(/Two tiers start at 7/);
  });
  test("names are unique regardless of case", () => {
    expect(ladderProblem([rung(), rung({ name: "gold", min_moves: 9 })])).toMatch(/Two tiers are called/);
  });
  test("a discounting tier needs a cap", () => {
    expect(ladderProblem([rung({ cap_cents: 0 })])).toMatch(/needs a cap/);
  });
  test("zero moves is allowed, as on the server", () => {
    expect(ladderProblem([rung({ min_moves: 0 })])).toBeNull();
  });
  test("the referral multiplier is 1–5", () => {
    expect(ladderProblem([rung({ referral_multiplier: 6 })])).toMatch(/1–5/);
  });
});
