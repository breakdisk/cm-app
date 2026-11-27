const mockGet = jest.fn();
const mockPost = jest.fn();
const mockDelete = jest.fn();

jest.mock('../client', () => ({
  getPromotionsClient: jest.fn(() => ({ get: mockGet, post: mockPost, delete: mockDelete })),
}));

import {
  claimReferral, creditEntryTitle, getCredit, getLoyalty, linkCorporate, percent, referralShareMessage,
  tierPerk, tierProgress, type Loyalty, type Tier,
} from '../rewards';

const money = (c: number) => `₱${c / 100}`;
const tier = (over: Partial<Tier> = {}): Tier => ({
  name: 'Silver', min_moves: 3, perk: '', accessorial_bps: 500, cap_cents: 20000, referral_multiplier: 1, ...over,
});

beforeEach(() => jest.clearAllMocks());

describe('tierProgress', () => {
  test('is the share of the way from this tier to the next', () => {
    const l: Loyalty = { enabled: true, moves: 5, tier: tier(), next: tier({ name: 'Gold', min_moves: 7 }), ladder: [] };
    expect(tierProgress(l)).toBe(0.5);
  });
  test('starts from zero below the first tier', () => {
    const l: Loyalty = { enabled: true, moves: 1, tier: null, next: tier({ min_moves: 4 }), ladder: [] };
    expect(tierProgress(l)).toBe(0.25);
  });
  test('is full at the top of the ladder', () => {
    expect(tierProgress({ enabled: true, moves: 40, tier: tier(), next: null, ladder: [] })).toBe(1);
  });
});

describe('tierPerk', () => {
  test('says what comes off extras, with the cap', () => {
    expect(tierPerk(tier(), money)).toBe('5% off extras, up to ₱200 a move');
  });
  test('without a discount, the tier’s own words', () => {
    expect(tierPerk(tier({ accessorial_bps: 0, perk: 'Priority support' }), money)).toBe('Priority support');
  });
});

test('percent keeps only the digits it needs', () => {
  expect(percent(1250)).toBe('12.5%');
  expect(percent(1000)).toBe('10%');
});

test('credit history lines are named by what happened', () => {
  const e = { amount_cents: 100, currency: 'PHP', note: '', created_at: '' };
  expect(creditEntryTitle({ ...e, kind: 'referral_reward' })).toBe('Referral reward');
  expect(creditEntryTitle({ ...e, kind: 'applied' })).toBe('Used on a move');
  expect(creditEntryTitle({ ...e, kind: 'grant', note: 'Sorry about the delay' })).toBe('Sorry about the delay');
});

test('the share message carries the code', () => {
  expect(referralShareMessage('K7M2QP9X')).toContain('K7M2QP9X');
});

describe('network', () => {
  test('an undeployed promotions reads as absent, not as an error', async () => {
    mockGet.mockRejectedValueOnce({ status: 404 });
    await expect(getLoyalty()).resolves.toBeNull();
  });
  test('any other failure is thrown', async () => {
    mockGet.mockRejectedValueOnce({ status: 500 });
    await expect(getCredit()).rejects.toEqual({ status: 500 });
  });
  test('a referral code is sent normalised', async () => {
    mockPost.mockResolvedValueOnce({ data: { data: { ok: true } } });
    await claimReferral('  k7m2qp9x ');
    expect(mockPost).toHaveBeenCalledWith('/v1/promotions/referrals/claim', { code: 'K7M2QP9X' });
  });
  test('linking without a code sends no code, so the server matches the email', async () => {
    mockPost.mockResolvedValueOnce({ data: { data: { linked: null } } });
    await linkCorporate();
    expect(mockPost).toHaveBeenCalledWith('/v1/promotions/corporate/me', {});
  });
});
