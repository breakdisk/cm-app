/**
 * `../client` is mocked wholesale, as in ai.test.ts — the axios automock loads
 * real axios, whose fetch-adapter probe crashes under this test environment.
 */
const mockPost = jest.fn();
const mockStore = new Map<string, string>();

jest.mock('expo-secure-store', () => ({
  getItemAsync: jest.fn((k: string) => Promise.resolve(mockStore.get(k) ?? null)),
  setItemAsync: jest.fn((k: string, v: string) => {
    mockStore.set(k, v);
    return Promise.resolve();
  }),
  deleteItemAsync: jest.fn(() => Promise.resolve()),
}));

jest.mock('../client', () => ({
  getPodClient: jest.fn(() => ({ post: mockPost })),
}));

import {
  DELIVERY_PIN_TTL_MS,
  getStoredDeliveryPin,
  issueDeliveryPin,
  onDeliveryPinIssued,
  parseIssuedPin,
} from '../pod';

const SHIPMENT = '0b9f3c1e-2f4a-4d7b-9a51-3c2e1f0a9b8c';

beforeEach(() => {
  mockPost.mockReset();
  mockStore.clear();
});

describe('issueDeliveryPin', () => {
  test('asks pod for the shipment, sends the recipient phone, and keeps the code', async () => {
    mockPost.mockResolvedValue({ data: { data: { otp_id: 'o1', code: '042917' } } });

    const pin = await issueDeliveryPin(SHIPMENT, '+639171234567');

    expect(mockPost).toHaveBeenCalledWith('/v1/otps/generate', {
      shipment_id: SHIPMENT,
      recipient_phone: '+639171234567',
    });
    expect(pin).toMatchObject({ kind: 'code', code: '042917' });
    expect(await getStoredDeliveryPin(SHIPMENT)).toMatchObject({ kind: 'code', code: '042917' });
  });

  // PR #161 withholds the code from roles that must not read it.
  test('a withheld code is kept as sent, so the app does not re-issue it', async () => {
    mockPost.mockResolvedValue({ data: { data: { otp_id: 'o1', sent: true } } });

    const pin = await issueDeliveryPin(SHIPMENT, '');

    expect(pin.kind).toBe('sent');
    expect(await getStoredDeliveryPin(SHIPMENT)).toMatchObject({ kind: 'sent' });
  });

  // Each call replaces the recipient's code. Two taps must be one request.
  test('a second tap while the first request is open does not issue again', async () => {
    let resolve: (v: unknown) => void = () => {};
    mockPost.mockReturnValue(new Promise(r => { resolve = r; }));

    const a = issueDeliveryPin(SHIPMENT, '');
    const b = issueDeliveryPin(SHIPMENT, '');
    resolve({ data: { data: { code: '111222' } } });

    expect(await a).toEqual(await b);
    expect(mockPost).toHaveBeenCalledTimes(1);
  });

  test('every mounted card hears about the new PIN', async () => {
    mockPost.mockResolvedValue({ data: { data: { code: '333444' } } });
    const heard: string[] = [];
    const off = onDeliveryPinIssued((id, pin) => heard.push(`${id}:${pin.kind}`));

    await issueDeliveryPin(SHIPMENT, '');
    off();

    expect(heard).toEqual([`${SHIPMENT}:code`]);
  });

  test('a malformed response throws and stores nothing', async () => {
    mockPost.mockResolvedValue({ data: { data: { otp_id: 'o1' } } });

    await expect(issueDeliveryPin(SHIPMENT, '')).rejects.toThrow(/neither a code nor/);
    expect(await getStoredDeliveryPin(SHIPMENT)).toBeNull();
  });
});

describe('getStoredDeliveryPin', () => {
  // pod expires a PIN after 15 minutes. Showing a dead one would fail at the door.
  test('an expired PIN is not shown', async () => {
    const issuedAt = Date.now() - DELIVERY_PIN_TTL_MS - 1;
    mockStore.set(`delivery_pin_${SHIPMENT}`, JSON.stringify({ kind: 'code', code: '123456', issuedAt }));

    expect(await getStoredDeliveryPin(SHIPMENT)).toBeNull();
  });

  test('a corrupt entry reads as no PIN', async () => {
    mockStore.set(`delivery_pin_${SHIPMENT}`, '{not json');
    expect(await getStoredDeliveryPin(SHIPMENT)).toBeNull();
  });
});

describe('parseIssuedPin', () => {
  test('accepts the unenveloped shape too', () => {
    expect(parseIssuedPin({ code: '987654' }, 1)).toEqual({ kind: 'code', code: '987654', issuedAt: 1 });
  });

  test('rejects a code that is not digits', () => {
    expect(() => parseIssuedPin({ data: { code: 'abc' } }, 1)).toThrow();
  });
});
