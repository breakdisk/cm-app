import { pollIntervalMs } from "../tracking";

describe("pollIntervalMs", () => {
  it("stops polling once nothing more can change", () => {
    expect(pollIntervalMs("delivered")).toBeNull();
    expect(pollIntervalMs("cancelled")).toBeNull();
  });

  it("polls fastest while the courier is moving", () => {
    const moving = pollIntervalMs("delivering")!;
    const waiting = pollIntervalMs("awaiting_courier")!;
    expect(moving).toBeLessThan(waiting);
  });

  it("never returns a zero or negative interval", () => {
    for (const s of ["placed", "awaiting_courier", "collecting", "delivering"] as const) {
      expect(pollIntervalMs(s)!).toBeGreaterThan(0);
    }
  });
});
