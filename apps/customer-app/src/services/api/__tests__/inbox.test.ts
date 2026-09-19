const mockGet = jest.fn();
const mockPost = jest.fn();

jest.mock('../client', () => ({
  getEngagementClient: jest.fn(() => ({ get: mockGet, post: mockPost })),
}));

import { getInbox, markRead, unreadCount, whenSent } from '../inbox';

beforeEach(() => jest.clearAllMocks());

describe('whenSent', () => {
  const now = new Date(2026, 8, 18, 16, 0);
  test('today shows the time', () => {
    expect(whenSent(new Date(2026, 8, 18, 9, 5).toISOString(), now)).toBe('Today 09:05');
  });
  test('yesterday is named', () => {
    expect(whenSent(new Date(2026, 8, 17, 23, 0).toISOString(), now)).toBe('Yesterday');
  });
  test('older is a date, with the year only when it is not this one', () => {
    expect(whenSent(new Date(2026, 8, 2).toISOString(), now)).toBe('2 Sep');
    expect(whenSent(new Date(2025, 11, 30).toISOString(), now)).toBe('30 Dec 2025');
  });
});

test('pages back with before', async () => {
  mockGet.mockResolvedValueOnce({ data: { data: { unread: 0, items: [] } } });
  await getInbox('2026-09-01T00:00:00Z');
  expect(mockGet).toHaveBeenCalledWith('/v1/engagement/inbox', { params: { before: '2026-09-01T00:00:00Z' } });
});

test('the badge is zero when the inbox cannot be read', async () => {
  mockGet.mockRejectedValueOnce(new Error('offline'));
  await expect(unreadCount()).resolves.toBe(0);
});

test('marking read addresses the message by id', async () => {
  mockPost.mockResolvedValueOnce({});
  await markRead('abc');
  expect(mockPost).toHaveBeenCalledWith('/v1/engagement/inbox/abc/read');
});

test('a campaign link opens only screens the app knows', () => {
  const { linkTarget } = require('../inbox');
  expect(linkTarget('logisticos://offers')).toBe('MoveOffers');
  expect(linkTarget('/payments?from=push')).toBe('MovePayments');
  expect(linkTarget('https://evil.example/offers')).toBeNull();
  expect(linkTarget('logisticos://admin')).toBeNull();
  expect(linkTarget('logisticos://inbox')).toBe('MoveInbox');
  expect(linkTarget(null)).toBeNull();
});
