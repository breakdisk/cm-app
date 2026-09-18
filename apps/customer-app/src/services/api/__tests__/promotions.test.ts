const mockGet = jest.fn();
const mockPost = jest.fn();

jest.mock('../client', () => ({
  getPromotionsClient: jest.fn(() => ({ get: mockGet, post: mockPost })),
}));

import { getOffers, monthTitle, monthWeeks, offerHeadline, validateCode, type Offer, type WindowDay } from '../promotions';

const money = (c: number) => `₱${c / 100}`;

const offer = (over: Partial<Offer> = {}): Offer => ({
  code: 'MOVE20',
  title: 'Autumn move-in',
  body: '',
  discount: { kind: 'percent_carriage', bps: 2000 },
  cap_cents: 2500,
  windowed: true,
  state: 'available',
  ...over,
});

describe('offerHeadline', () => {
  test('a percentage is of carriage, with the code’s own cap', () => {
    expect(offerHeadline(offer(), money)).toBe('20% off carriage, up to ₱25');
  });
  test('a flat code is its amount', () => {
    expect(offerHeadline(offer({ discount: { kind: 'flat', cents: 1400 }, cap_cents: null }), money)).toBe('₱14 off');
  });
  test('a fractional percentage keeps only the digits it needs', () => {
    expect(offerHeadline(offer({ discount: { kind: 'percent_carriage', bps: 1250 }, cap_cents: null }), money)).toBe('12.5% off carriage');
  });
});

describe('monthWeeks', () => {
  // September 2026 starts on a Tuesday (weekday 1).
  const september: WindowDay[] = Array.from({ length: 30 }, (_, i) => ({
    day: i + 1,
    weekday: (i + 1) % 7,
    eligible: false,
  }));

  test('pads before the 1st so it lands under Tuesday', () => {
    const weeks = monthWeeks(september);
    expect(weeks[0][0]).toBeNull();
    expect(weeks[0][1]?.day).toBe(1);
  });

  test('every row is seven cells', () => {
    expect(monthWeeks(september).every((w) => w.length === 7)).toBe(true);
    expect(monthWeeks(september)).toHaveLength(5);
  });

  test('no days, no weeks', () => {
    expect(monthWeeks([])).toEqual([]);
  });
});

test('monthTitle reads the server’s month', () => {
  expect(monthTitle('2026-09')).toBe('September 2026');
});

describe('getOffers', () => {
  beforeEach(() => mockGet.mockReset());

  test('unwraps the feed', async () => {
    mockGet.mockResolvedValue({ data: { data: { window: { month: '2026-09' }, offers: [] } } });
    expect((await getOffers())?.window.month).toBe('2026-09');
  });

  test('promotions not deployed is null, not an empty feed', async () => {
    mockGet.mockRejectedValue({ status: 503 });
    expect(await getOffers()).toBeNull();
  });

  test('other failures throw', async () => {
    mockGet.mockRejectedValue({ status: 500 });
    await expect(getOffers()).rejects.toEqual({ status: 500 });
  });
});

test('validateCode sends the code the way the server stores it', async () => {
  mockPost.mockResolvedValue({ data: { data: { ok: false, code: 'MOVE20', refusal: 'OUTSIDE_WINDOW_WEEKEND' } } });
  const r = await validateCode(' move20 ');
  expect(mockPost).toHaveBeenCalledWith('/v1/promotions/codes/MOVE20/validate');
  expect(r.refusal).toBe('OUTSIDE_WINDOW_WEEKEND');
});
