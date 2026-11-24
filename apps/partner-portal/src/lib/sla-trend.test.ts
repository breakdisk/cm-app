// Imported rather than ambient: @jest/globals ships with jest and carries its own
// types, so the app needs no @types/jest.
import { describe, expect, it } from "@jest/globals";
import { dailyOnTimeRate, totalsFromZones } from "./sla-trend";
import type { SlaRecord, ZoneSlaRow } from "@/lib/api/carriers";

function rec(over: Partial<SlaRecord>): SlaRecord {
  return {
    id: "r", shipment_id: "s", zone: "AUH", service_level: "standard",
    promised_by: "2026-11-02T10:00:00Z", delivered_at: "2026-11-02T09:00:00Z",
    status: "delivered", on_time: true, failure_reason: null, created_at: "2026-11-01T08:00:00Z",
    ...over,
  };
}

describe("dailyOnTimeRate", () => {
  it("groups by the day of the verdict and rounds to one decimal", () => {
    const out = dailyOnTimeRate([
      rec({ delivered_at: "2026-11-02T09:00:00Z", on_time: true }),
      rec({ delivered_at: "2026-11-02T15:00:00Z", on_time: true }),
      rec({ delivered_at: "2026-11-02T18:00:00Z", on_time: false, status: "failed" }),
      rec({ delivered_at: "2026-11-03T09:00:00Z", on_time: true }),
    ]);
    expect(out).toEqual([
      { date: "2026-11-02", rate: 66.7 },
      { date: "2026-11-03", rate: 100 },
    ]);
  });

  it("leaves out a shipment still in transit instead of counting it as late", () => {
    const out = dailyOnTimeRate([
      rec({ on_time: null, delivered_at: null, status: "in_transit" }),
      rec({ delivered_at: "2026-11-02T09:00:00Z", on_time: true }),
    ]);
    expect(out).toEqual([{ date: "2026-11-02", rate: 100 }]);
  });

  it("omits a day that has only undecided shipments", () => {
    expect(dailyOnTimeRate([rec({ on_time: null, delivered_at: null, status: "in_transit" })])).toEqual([]);
  });

  it("dates a failure with no delivery time by when it was recorded", () => {
    const out = dailyOnTimeRate([
      rec({ on_time: false, delivered_at: null, status: "failed", created_at: "2026-11-05T07:00:00Z" }),
    ]);
    expect(out).toEqual([{ date: "2026-11-05", rate: 0 }]);
  });

  it("returns days oldest first whatever order the history arrives in", () => {
    const out = dailyOnTimeRate([
      rec({ delivered_at: "2026-11-09T09:00:00Z" }),
      rec({ delivered_at: "2026-11-02T09:00:00Z" }),
      rec({ delivered_at: "2026-11-05T09:00:00Z" }),
    ]);
    expect(out.map((p) => p.date)).toEqual(["2026-11-02", "2026-11-05", "2026-11-09"]);
  });

  it("returns nothing for no history, not a fabricated trend", () => {
    expect(dailyOnTimeRate([])).toEqual([]);
  });
});

describe("totalsFromZones", () => {
  const zones: ZoneSlaRow[] = [
    { zone: "A", total: 10, on_time: 8, failed: 1, on_time_rate: 88.9 },
    { zone: "B", total: 5, on_time: 2, failed: 2, on_time_rate: 50 },
  ];

  it("adds the zones up and rates on-time over decided shipments", () => {
    const t = totalsFromZones(zones);
    expect(t).toMatchObject({ total: 15, onTime: 10, failed: 3 });
    expect(t.ratePct).toBeCloseTo((10 / 13) * 100, 5);
  });

  it("has no rate, not 0 or 100, when nothing is decided yet", () => {
    const t = totalsFromZones([{ zone: "A", total: 4, on_time: 0, failed: 0, on_time_rate: 0 }]);
    expect(t.total).toBe(4);
    expect(t.ratePct).toBeNull();
  });

  it("handles no zones", () => {
    expect(totalsFromZones([])).toEqual({ total: 0, onTime: 0, failed: 0, ratePct: null });
  });
});
