import type { AdminCourier } from "@/lib/api/couriers";
import { complianceView, dispatchView, courierCounts } from "./compliance-view";
import {
  LEGACY_OFF_DUTY,
  LEGACY_AVAILABLE,
  LEGACY_SUSPENDED,
  CURRENT_NOT_ONBOARDED,
  CURRENT_COMPLIANT,
  CURRENT_OBSERVE_ONLY,
  CURRENT_ENFORCED_BLOCK,
  NOW_MS,
} from "./fixtures";

describe("complianceView", () => {
  it("does not throw on a payload with no compliance fields", () => {
    expect(() => complianceView(LEGACY_OFF_DUTY)).not.toThrow();
  });

  it("reports an absent field as unsupported, not as onboarded or blocked", () => {
    const v = complianceView(LEGACY_OFF_DUTY);
    expect(v.kind).toBe("unsupported");
    expect(v.label).toBe("—");
  });

  it("distinguishes an absent field from an explicit null", () => {
    expect(complianceView(LEGACY_OFF_DUTY).kind).toBe("unsupported");
    expect(complianceView(CURRENT_NOT_ONBOARDED).kind).toBe("not-onboarded");
  });

  it("labels a null status as not onboarded", () => {
    expect(complianceView(CURRENT_NOT_ONBOARDED).label).toBe("not onboarded");
  });

  it("renders a known status with underscores turned into spaces", () => {
    expect(complianceView(CURRENT_OBSERVE_ONLY).label).toBe("rejected");
    expect(
      complianceView({ ...CURRENT_COMPLIANT, compliance_status: "pending_submission" }).label,
    ).toBe("pending submission");
  });

  it("tones a refused courier differently from a compliant one", () => {
    expect(complianceView(CURRENT_COMPLIANT).tone).not.toBe(
      complianceView(CURRENT_OBSERVE_ONLY).tone,
    );
  });
});

describe("dispatchView", () => {
  it("does not throw on a payload with no block_reason", () => {
    expect(() => dispatchView(LEGACY_OFF_DUTY, NOW_MS)).not.toThrow();
  });

  it("says an offline courier is off duty rather than guessing at their GPS", () => {
    const v = dispatchView(LEGACY_OFF_DUTY, NOW_MS);
    expect(v.label).toBe("not on duty");
    expect(v.label).not.toMatch(/last fix|never sent/);
  });

  it("derives suspension from is_active when the server did not say", () => {
    expect(dispatchView(LEGACY_SUSPENDED, NOW_MS).label).toBe("suspended by ops");
  });

  it("ranks suspension above duty, as the server does", () => {
    const both = { ...LEGACY_SUSPENDED, status: "offline" } as AdminCourier;
    expect(dispatchView(both, NOW_MS).label).toBe("suspended by ops");
  });

  it("prefers the server's block_reason over anything it could derive", () => {
    expect(dispatchView(CURRENT_ENFORCED_BLOCK, NOW_MS).label).toBe("blocked · expired");
  });

  it("never claims compliance blocks on a payload that cannot know", () => {
    for (const c of [LEGACY_OFF_DUTY, LEGACY_AVAILABLE, LEGACY_SUSPENDED]) {
      expect(dispatchView(c, NOW_MS).label).not.toMatch(/compliance/i);
    }
  });

  it("still flags a stale fix for an on-duty courier", () => {
    const stale = { ...LEGACY_AVAILABLE, last_seen_at: "2026-08-23T08:00:00.000Z" } as AdminCourier;
    expect(dispatchView(stale, NOW_MS).label).toMatch(/stale/);
  });

  it("flags a courier who has never sent a position", () => {
    const never = { ...LEGACY_AVAILABLE, last_seen_at: null } as AdminCourier;
    expect(dispatchView(never, NOW_MS).label).toMatch(/never sent a position/);
  });

  it("says a healthy legacy courier is receiving offers", () => {
    expect(dispatchView(LEGACY_AVAILABLE, NOW_MS).label).toBe("receiving offers");
  });

  it("shows the observe-only disagreement when compliance is known", () => {
    expect(dispatchView(CURRENT_OBSERVE_ONLY, NOW_MS).label)
      .toBe("receiving offers · compliance would block");
  });
});

describe("courierCounts", () => {
  const legacy = [LEGACY_OFF_DUTY, LEGACY_AVAILABLE, LEGACY_SUSPENDED];

  it("counts what a legacy payload does support", () => {
    const n = courierCounts(legacy);
    expect(n.total).toBe(3);
    expect(n.dispatchable).toBe(1);
    expect(n.suspended).toBe(1);
  });

  it("returns null, not zero, for counts a legacy payload cannot support", () => {
    const n = courierCounts(legacy);
    expect(n.complianceBlocked).toBeNull();
    expect(n.notOnboarded).toBeNull();
  });

  it("counts compliance once the server reports it", () => {
    const n = courierCounts([CURRENT_COMPLIANT, CURRENT_NOT_ONBOARDED, CURRENT_OBSERVE_ONLY]);
    expect(n.complianceBlocked).toBe(1);
    expect(n.notOnboarded).toBe(1);
  });

  it("ignores couriers the server said nothing about when mixing payloads", () => {
    const n = courierCounts([LEGACY_AVAILABLE, CURRENT_OBSERVE_ONLY]);
    expect(n.complianceBlocked).toBe(1);
    expect(n.notOnboarded).toBe(0);
  });

  it("is empty-safe", () => {
    const n = courierCounts([]);
    expect(n.total).toBe(0);
    expect(n.complianceBlocked).toBeNull();
  });
});
