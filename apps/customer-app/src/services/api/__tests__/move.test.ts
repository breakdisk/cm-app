const mockGet = jest.fn();
const mockPost = jest.fn();

jest.mock('../client', () => ({
  getOrderClient: jest.fn(() => ({ get: mockGet, post: mockPost })),
}));

import { accessorialLabel, linesReconcile, listAccessorials, quoteLines, quoteTotal, type MoveQuote } from '../move';

const quote = (over: Partial<MoveQuote> = {}): MoveQuote => ({
  amount_cents: 20_000,
  currency: 'USD',
  quote_token: 't',
  expires_at: '2026-09-15T12:00:00Z',
  breakdown: {
    currency: 'USD',
    carriage_cents: 18_000,
    accessorial_cents: 2_000,
    total_cents: 20_000,
    carriage_rows: [
      { label: 'Cargo van callout', note: 'base rate', amount_cents: 9_500 },
      { label: 'Distance', note: '7.4 km at $2.40 per km', amount_cents: 1_800 },
      { label: 'Weight', note: '320 kg at $0.22 per kg', amount_cents: 6_700 },
    ],
    items: [{ code: 'helper', units: 2, amount_cents: 2_000 }],
  },
  ...over,
});

describe('quoteLines', () => {
  test('carriage rows then accessorials, as the server sent them', () => {
    const lines = quoteLines(quote());
    expect(lines.map((l) => l.label)).toEqual(['Cargo van callout', 'Distance', 'Weight', 'Helper']);
    expect(lines[3].note).toBe('2 units');
  });

  test('a quote without a breakdown is one carriage line', () => {
    expect(quoteLines(quote({ breakdown: null }))).toEqual([{ label: 'Carriage', note: '', amount_cents: 20_000 }]);
    expect(quoteTotal(quote({ breakdown: null }))).toEqual({ cents: 20_000, currency: 'USD' });
  });
});

// The handoff: the total broke twice in review — once itemising something not
// charged, once charging something not itemised.
describe('linesReconcile', () => {
  test('rows that add up to the total reconcile', () => {
    expect(linesReconcile(quote())).toBe(true);
  });

  test('a charge with no row does not reconcile', () => {
    const q = quote();
    q.breakdown!.total_cents = 21_000;
    expect(linesReconcile(q)).toBe(false);
  });

  test('a row with no charge does not reconcile', () => {
    const q = quote();
    q.breakdown!.items.push({ code: 'carbon_offset', units: 1, amount_cents: 300 });
    expect(linesReconcile(q)).toBe(false);
  });
});

// Discounts are rows the server sends; the app subtracts nothing it wasn't told.
describe('discounts', () => {
  const discounted = () => quote({
    amount_cents: 17_500,
    gross_cents: 20_000,
    discount_cents: 2_500,
    discounts: [{ kind: 'code', label: 'MOVE20', amount_cents: 2_500, clipped: false }],
    promo_code: 'MOVE20',
  });

  test('the total is the rows less the discount lines', () => {
    expect(quoteTotal(discounted())).toEqual({ cents: 17_500, currency: 'USD' });
  });

  test('a discount that adds up reconciles', () => {
    expect(linesReconcile(discounted())).toBe(true);
  });

  test('a discount the charge does not match does not reconcile', () => {
    expect(linesReconcile({ ...discounted(), amount_cents: 18_000 })).toBe(false);
  });

  test('a quote from a server without promotions is unchanged', () => {
    expect(quoteTotal(quote())).toEqual({ cents: 20_000, currency: 'USD' });
    expect(linesReconcile(quote())).toBe(true);
  });
});

describe('listAccessorials', () => {
  beforeEach(() => mockGet.mockReset());

  test('returns the catalog', async () => {
    mockGet.mockResolvedValue({ data: { currency: 'USD', items: [{ code: 'helper', amount_cents: 1000, basis: 'stair_flight', max_units: 8 }] } });
    expect((await listAccessorials())?.items[0].code).toBe('helper');
  });

  test('a server without the endpoint is null, not an error', async () => {
    mockGet.mockRejectedValue({ status: 404 });
    expect(await listAccessorials()).toBeNull();
  });

  test('other failures throw', async () => {
    mockGet.mockRejectedValue({ status: 500 });
    await expect(listAccessorials()).rejects.toEqual({ status: 500 });
  });
});

test('accessorialLabel reads unknown codes as words', () => {
  expect(accessorialLabel('haul_away')).toBe('Haul-away');
  expect(accessorialLabel('white_glove')).toBe('White glove');
});

describe('discountLabel', () => {
  const { discountLabel } = require('../move');
  const line = (kind: string, label: string) => ({ kind, label, amount_cents: 100, clipped: false });
  test('each kind reads as what it is', () => {
    expect(discountLabel(line('code', 'MOVE20'))).toBe('Code MOVE20');
    expect(discountLabel(line('corporate', 'Acme Corp'))).toBe('Acme Corp rate');
    expect(discountLabel(line('tier', 'Gold'))).toBe('Gold member');
    expect(discountLabel(line('credit', 'Account credit'))).toBe('Account credit');
  });
});
