import { formatDate, formatCurrency, formatPhone, formatAWB } from '../formatting';

describe('Formatting Utils', () => {
  test('formatDate returns readable date format', () => {
    const date = new Date('2026-04-05T10:30:00');
    expect(formatDate(date)).toMatch(/Apr 5, 2026|April 5, 2026/);
  });

  test('formatDate with time flag includes time', () => {
    const date = new Date('2026-04-05T10:30:00');
    const result = formatDate(date, { time: true });
    expect(result).toMatch(/10:30/);
  });

  test('formatCurrency formats PHP with correct symbol', () => {
    expect(formatCurrency(1500, 'PHP')).toBe('₱1,500.00');
  });

  test('formatCurrency formats USD with correct symbol', () => {
    expect(formatCurrency(50.5, 'USD')).toBe('$50.50');
  });

  test('formatPhone removes non-digits and formats E.164', () => {
    expect(formatPhone('09123456789')).toBe('+639123456789');
    expect(formatPhone('+1 (202) 555-0123')).toBe('+12025550123');
  });

  test('formatAWB returns uppercase 10-char format', () => {
    expect(formatAWB('awb123456')).toBe('AWB123456');
  });
});
