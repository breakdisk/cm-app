import { describe, expect, test } from "@jest/globals";

import { parseLabels } from "@/lib/venues/parse-labels";

describe("parseLabels", () => {
  test("expands numeric ranges and keeps plain labels in order", () => {
    expect(parseLabels("1-3, patio, 5")).toEqual(["1", "2", "3", "patio", "5"]);
  });

  test("trims whitespace around each comma-separated part", () => {
    expect(parseLabels("  a ,  b  ,c")).toEqual(["a", "b", "c"]);
  });

  test("allows whitespace around the range dash", () => {
    expect(parseLabels("1 - 3")).toEqual(["1", "2", "3"]);
  });

  test("empty input and blank parts produce no labels", () => {
    expect(parseLabels("")).toEqual([]);
    expect(parseLabels(" , ,  ,")).toEqual([]);
  });

  test("dedupes while preserving first-seen order", () => {
    expect(parseLabels("3, 1-3, a, 3, a")).toEqual(["3", "1", "2", "a"]);
  });

  test("a reversed range is kept as a literal label, not expanded", () => {
    expect(parseLabels("5-3")).toEqual(["5-3"]);
  });

  test("a range of 200 or more is kept as a literal label", () => {
    expect(parseLabels("1-201")).toEqual(["1-201"]);
  });

  test("a range of exactly 200 values is still expanded", () => {
    const out = parseLabels("1-200");
    expect(out).toHaveLength(200);
    expect(out[0]).toBe("1");
    expect(out[199]).toBe("200");
  });

  test("non-numeric ranges are kept as literal labels", () => {
    expect(parseLabels("a-b")).toEqual(["a-b"]);
  });
});
