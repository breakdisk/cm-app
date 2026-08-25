import { complianceView } from "./compliance-view";
import {
  LEGACY_OFF_DUTY,
  CURRENT_NOT_ONBOARDED,
  CURRENT_COMPLIANT,
  CURRENT_OBSERVE_ONLY,
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
