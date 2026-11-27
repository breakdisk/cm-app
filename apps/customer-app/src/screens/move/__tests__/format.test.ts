import { formatKg, formatMoney, initials } from '../format';

describe('formatMoney', () => {
  test('groups thousands and keeps cents', () => {
    expect(formatMoney(17200, 'USD')).toBe('$172.00');
    expect(formatMoney(123456789, 'PHP')).toBe('₱1,234,567.89');
  });

  test('whole amounts can drop the cents', () => {
    expect(formatMoney(17200, 'USD', { whole: true })).toBe('$172');
    expect(formatMoney(17250, 'USD', { whole: true })).toBe('$172.50');
  });

  // The handoff: the riyal glyph renders as a box, so SAR reads "SR".
  test('currencies without a safe glyph use a code', () => {
    expect(formatMoney(5000, 'SAR', { whole: true })).toBe('SR 50');
    expect(formatMoney(5000, 'AED', { whole: true })).toBe('AED 50');
    expect(formatMoney(5000, 'JPY', { whole: true })).toBe('JPY 50');
  });

  test('no currency still formats, and negatives keep their sign', () => {
    expect(formatMoney(99, null)).toBe('0.99');
    expect(formatMoney(-4300, 'USD', { whole: true })).toBe('-$43');
  });
});

describe('formatKg and initials', () => {
  test('kilograms read naturally', () => {
    expect(formatKg(187000)).toBe('187 kg');
    expect(formatKg(1500)).toBe('1.5 kg');
    expect(formatKg(1470000)).toBe('1,470 kg');
  });

  test('initials from a name, with a placeholder for none', () => {
    expect(initials('Dee Whitlock')).toBe('DW');
    expect(initials('  ana ')).toBe('A');
    expect(initials(null)).toBe('·');
  });
});
