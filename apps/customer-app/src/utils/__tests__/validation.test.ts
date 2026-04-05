import { validatePhone, validateEmail, validateWeight, validateCOD } from '../validation';

describe('Validation Utils', () => {
  test('validatePhone accepts 11-digit PH format', () => {
    expect(validatePhone('09123456789')).toBe(true);
    expect(validatePhone('+639123456789')).toBe(true);
  });

  test('validatePhone rejects invalid formats', () => {
    expect(validatePhone('123')).toBe(false);
    expect(validatePhone('hello')).toBe(false);
  });

  test('validateEmail accepts valid emails', () => {
    expect(validateEmail('user@example.com')).toBe(true);
  });

  test('validateEmail rejects invalid emails', () => {
    expect(validateEmail('notanemail')).toBe(false);
    expect(validateEmail('@example.com')).toBe(false);
  });

  test('validateWeight accepts positive numbers within limits', () => {
    expect(validateWeight(10, 'standard')).toBe(true);
    expect(validateWeight(50, 'standard')).toBe(true);
    expect(validateWeight(51, 'standard')).toBe(false);
  });

  test('validateCOD requires positive amount', () => {
    expect(validateCOD(100)).toBe(true);
    expect(validateCOD(0)).toBe(false);
    expect(validateCOD(-50)).toBe(false);
  });
});
