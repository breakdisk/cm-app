import { combineLoad, dimsFromScan, formatDims, formatVolume, volumeCm3 } from '../scan';

describe('dimsFromScan', () => {
  test('rounds to whole centimetres and puts the longest edge first', () => {
    expect(dimsFromScan({ length: 74.6, width: 212.8, height: 90.2 })).toEqual({ lengthCm: 213, widthCm: 90, heightCm: 75 });
  });

  test('never returns a zero or broken edge', () => {
    expect(dimsFromScan({ length: 0.2, width: Number.NaN, height: 40 })).toEqual({ lengthCm: 40, widthCm: 1, heightCm: 1 });
  });
});

describe('formatting', () => {
  test('dimensions and volume read like the design', () => {
    const sofa = { lengthCm: 213, widthCm: 90, heightCm: 75 };
    expect(formatDims(sofa)).toBe('213 × 90 × 75 cm');
    expect(formatVolume(volumeCm3(sofa))).toBe('1.44 m³');
    expect(formatVolume(12_340_000)).toBe('12.3 m³');
  });
});

describe('combineLoad', () => {
  const sofa = { lengthCm: 213, widthCm: 90, heightCm: 75 };
  const box = { lengthCm: 50, widthCm: 40, heightCm: 40 };

  test('nothing scanned gives no load size', () => {
    expect(combineLoad([{ qty: 1 }, { qty: 3 }])).toBeNull();
  });

  test('a single item is its own box', () => {
    expect(combineLoad([{ qty: 1, dims: sofa }])).toEqual(sofa);
  });

  test('quantity multiplies the volume', () => {
    expect(combineLoad([{ qty: 2, dims: sofa }])).toEqual({ ...sofa, heightCm: 150 });
  });

  // Billing reads only the volume, so that is what must survive combining.
  test('several items keep their combined volume to within half a slice', () => {
    const load = combineLoad([{ qty: 1, dims: sofa }, { qty: 2, dims: box }, { qty: 4 }])!;
    const expected = volumeCm3(sofa) + 2 * volumeCm3(box);
    expect(load.lengthCm).toBe(213);
    expect(load.widthCm).toBe(90);
    expect(Math.abs(volumeCm3(load) - expected)).toBeLessThanOrEqual((load.lengthCm * load.widthCm) / 2);
  });

  test('a zero quantity counts nothing', () => {
    expect(combineLoad([{ qty: 0, dims: sofa }])).toBeNull();
  });
});
