import { describe, it, expect } from "@jest/globals";
import { MAX_UPLOAD_BYTES, nextFocusIndex, stripDataUrlPrefix, validateUploadFile } from "./upload";

describe("stripDataUrlPrefix", () => {
  it("removes the data: header and keeps the payload", () => {
    expect(stripDataUrlPrefix("data:image/png;base64,iVBORw0KGgo=")).toBe("iVBORw0KGgo=");
    expect(stripDataUrlPrefix("data:application/pdf;base64,JVBERi0x")).toBe("JVBERi0x");
  });

  it("only cuts at the first marker, so payload content is untouched", () => {
    expect(stripDataUrlPrefix("data:image/png;base64,AA;base64,BB")).toBe("AA;base64,BB");
  });

  it("leaves a string with no prefix alone", () => {
    expect(stripDataUrlPrefix("iVBORw0KGgo=")).toBe("iVBORw0KGgo=");
  });
});

describe("validateUploadFile", () => {
  it("accepts a file at exactly the limit", () => {
    expect(validateUploadFile({ size: MAX_UPLOAD_BYTES, type: "application/pdf" })).toBeNull();
  });

  it("rejects a file one byte over 10 MB with a message naming the limit", () => {
    const msg = validateUploadFile({ size: MAX_UPLOAD_BYTES + 1, type: "image/png" });
    expect(msg).toContain("10 MB");
  });

  it("rejects an empty file", () => {
    expect(validateUploadFile({ size: 0, type: "image/png" })).not.toBeNull();
  });

  it("rejects types outside the accept list", () => {
    expect(validateUploadFile({ size: 10, type: "image/gif" })).not.toBeNull();
    expect(validateUploadFile({ size: 10, type: "" })).not.toBeNull();
  });

  it.each(["image/jpeg", "image/png", "image/webp", "application/pdf"])("accepts %s", (type) => {
    expect(validateUploadFile({ size: 10, type })).toBeNull();
  });
});

describe("nextFocusIndex", () => {
  it("moves forward and wraps from last to first", () => {
    expect(nextFocusIndex(0, 3, false)).toBe(1);
    expect(nextFocusIndex(2, 3, false)).toBe(0);
  });

  it("moves backward and wraps from first to last", () => {
    expect(nextFocusIndex(2, 3, true)).toBe(1);
    expect(nextFocusIndex(0, 3, true)).toBe(2);
  });

  it("pulls focus in from outside the trap", () => {
    expect(nextFocusIndex(-1, 3, false)).toBe(0);
    expect(nextFocusIndex(-1, 3, true)).toBe(2);
  });

  it("returns null when nothing is focusable", () => {
    expect(nextFocusIndex(-1, 0, false)).toBeNull();
  });
});
