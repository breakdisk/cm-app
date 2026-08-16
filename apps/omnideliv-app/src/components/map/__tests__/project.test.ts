import { markerIndices, projectAll } from "../project";

const BOX = { width: 300, height: 200, padding: 20 };

describe("projectAll", () => {
  it("places every point inside the padded box", () => {
    const pts = [
      { lat: 14.60, lng: 120.98 },
      { lat: 14.62, lng: 121.02 },
      { lat: 14.58, lng: 120.95 },
    ];
    for (const p of projectAll(pts, BOX)) {
      expect(p.x).toBeGreaterThanOrEqual(BOX.padding);
      expect(p.x).toBeLessThanOrEqual(BOX.width - BOX.padding);
      expect(p.y).toBeGreaterThanOrEqual(BOX.padding);
      expect(p.y).toBeLessThanOrEqual(BOX.height - BOX.padding);
    }
  });

  /** Screen y grows downwards; latitude grows northwards. */
  it("puts the northern point above the southern one", () => {
    const [north, south] = projectAll(
      [{ lat: 14.70, lng: 120.98 }, { lat: 14.50, lng: 120.98 }],
      BOX,
    );
    expect(north.y).toBeLessThan(south.y);
  });

  it("puts the eastern point right of the western one", () => {
    const [west, east] = projectAll(
      [{ lat: 14.60, lng: 120.90 }, { lat: 14.60, lng: 121.10 }],
      BOX,
    );
    expect(east.x).toBeGreaterThan(west.x);
  });

  /** One point, or several identical ones, must not divide by a zero span. */
  it("centres a degenerate set instead of dividing by zero", () => {
    for (const pts of [
      [{ lat: 14.6, lng: 120.98 }],
      [{ lat: 14.6, lng: 120.98 }, { lat: 14.6, lng: 120.98 }],
    ]) {
      for (const p of projectAll(pts, BOX)) {
        expect(Number.isFinite(p.x)).toBe(true);
        expect(Number.isFinite(p.y)).toBe(true);
        expect(p.x).toBeCloseTo(BOX.width / 2, 5);
        expect(p.y).toBeCloseTo(BOX.height / 2, 5);
      }
    }
  });

  it("returns nothing for no points", () => {
    expect(projectAll([], BOX)).toEqual([]);
  });
});

/**
 * `@testing-library/react-native` was deliberately dropped from this app (it
 * pulled an incompatible `react-test-renderer`), so CanvasPlot's marker
 * selection lives here as a pure function instead of being covered through a
 * rendered component tree.
 */
describe("markerIndices", () => {
  it("has no courier marker when there is no fix", () => {
    expect(markerIndices(3, false).courierIndex).toBeNull();
  });

  it("places the courier one past the destination when a fix is present", () => {
    expect(markerIndices(3, true).courierIndex).toBe(4);
  });

  it("places the destination right after the stops", () => {
    expect(markerIndices(3, true).destIndex).toBe(3);
    expect(markerIndices(0, true).destIndex).toBe(0);
  });

  it("holds even with no stops at all", () => {
    expect(markerIndices(0, false)).toEqual({
      stopCount: 0,
      destIndex: 0,
      courierIndex: null,
    });
    expect(markerIndices(0, true)).toEqual({
      stopCount: 0,
      destIndex: 0,
      courierIndex: 1,
    });
  });
});
