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
