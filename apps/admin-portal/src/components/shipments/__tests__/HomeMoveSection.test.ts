import { describe, expect, test } from "@jest/globals";
import { payLines } from "../HomeMoveSection";

describe("payLines", () => {
  const pay = { lead_gross_cents: 145_500, commission_cents: 29_100, lead_payout_cents: 116_400, survey_payout_cents: 3_600, survey_commission_cents: 900 };

  test("shows what the customer paid, the lead's pay and the platform's commission", () => {
    const lines = Object.fromEntries(payLines({ total_cents: 150_000, currency: "PHP" }, pay));
    expect(lines["Customer paid"]).toContain("1,500.00");
    expect(lines["Lead's pay"]).toContain("1,164.00");
    expect(lines["Platform commission"]).toContain("291.00");
    expect(lines["Survey fee to lead"]).toContain("36.00");
  });

  test("names a surge only when there was one", () => {
    expect(payLines({ total_cents: 150_000, surge_bps: 10_000 }, null).map(([k]) => k)).toEqual(["Customer paid"]);
    const surged = Object.fromEntries(payLines({ total_cents: 180_000, surge_bps: 12_000, surge_cents: 30_000 }, null));
    expect(surged["High-demand surge"]).toContain("×1.20");
  });

  test("without the pay block (a customer's view) no pay line is drawn", () => {
    expect(payLines({ total_cents: 150_000 }, null)).toHaveLength(1);
  });
});
