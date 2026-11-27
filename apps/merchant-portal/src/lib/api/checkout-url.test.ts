import { describe, it, expect } from "@jest/globals";
import { safeCheckoutUrl } from "./checkout-url";

describe("safeCheckoutUrl", () => {
  it("returns an https URL", () => {
    expect(safeCheckoutUrl("https://pay.example.com/checkout/abc?x=1")).toBe(
      "https://pay.example.com/checkout/abc?x=1",
    );
  });

  it.each([
    ["http", "http://pay.example.com/checkout"],
    ["javascript", "javascript:alert(1)"],
    ["JavaScript mixed case", "JaVaScRiPt:alert(1)"],
    ["data", "data:text/html,<script>alert(1)</script>"],
    ["relative path", "/checkout/abc"],
    ["protocol-relative", "//evil.example.com/x"],
    ["malformed", "not a url"],
    ["empty", ""],
  ])("rejects %s", (_name, raw) => {
    expect(safeCheckoutUrl(raw)).toBeNull();
  });

  it("rejects null and undefined", () => {
    expect(safeCheckoutUrl(null)).toBeNull();
    expect(safeCheckoutUrl(undefined)).toBeNull();
  });
});
