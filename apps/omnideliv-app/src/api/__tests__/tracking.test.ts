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

  /**
   * The one moment the customer is actively doing something: they are on the
   * card page in another app, and the authorization webhook is what turns this
   * screen from "pay for this" into "finding a courier". At the ordinary 15s
   * that transition reads as the app having missed the payment.
   */
  it("polls faster while an online payment is still unfinished", () => {
    const paying = pollIntervalMs("placed", {
      payment_method: "online",
      payment_status: "pending",
    })!;
    expect(paying).toBeLessThan(pollIntervalMs("placed")!);
  });

  /** COD never has a pending gateway payment to wait on. */
  it("does not speed up for a COD order", () => {
    expect(
      pollIntervalMs("placed", { payment_method: "cod", payment_status: "pending" }),
    ).toBe(pollIntervalMs("placed"));
  });

  /** A terminal order stops polling whatever its payment status says. */
  it("still stops on a terminal order with an unfinished payment", () => {
    expect(
      pollIntervalMs("cancelled", { payment_method: "online", payment_status: "pending" }),
    ).toBeNull();
  });
});
