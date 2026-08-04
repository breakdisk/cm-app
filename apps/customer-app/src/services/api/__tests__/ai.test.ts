/**
 * `../client` is mocked wholesale rather than via `jest.mock('axios')` — the
 * axios automock makes jest load the real axios, whose fetch-adapter probe
 * crashes under this Expo/Node test environment (the same crash the existing
 * client.test.ts and sync.test.ts suites hit).
 *
 * `ApiError` is re-declared inside the factory so both `ai.ts` and this file
 * resolve to the same class and `instanceof` stays meaningful.
 */
const mockPost = jest.fn();
const mockGet = jest.fn();

jest.mock('expo-secure-store', () => ({
  getItemAsync: jest.fn(() => Promise.resolve(null)),
  setItemAsync: jest.fn(() => Promise.resolve()),
  deleteItemAsync: jest.fn(() => Promise.resolve()),
}));

jest.mock('../client', () => {
  // Plain field assignment, not TS parameter properties — babel's jest-hoist
  // plugin rejects `constructor(public status: ...)` inside a mock factory.
  class ApiError extends Error {
    status: number;
    data?: unknown;
    constructor(status: number, message: string, data?: unknown) {
      super(message);
      this.name = 'ApiError';
      this.status = status;
      this.data = data;
    }
  }
  return {
    ApiError,
    createApiClient: jest.fn(() => ({ post: mockPost, get: mockGet })),
  };
});

import { ApiError } from '../client';
import { aiApi, AiUnavailableError } from '../ai';

beforeEach(() => {
  mockPost.mockReset();
  mockGet.mockReset();
});

describe('aiApi.chat', () => {
  test('sends the message, session id and shipment context to /v1/agents/chat', async () => {
    mockPost.mockResolvedValue({
      data: { session_id: 'sess-2', reply: 'On its way.', escalated: false, status: 'completed' },
    });

    await aiApi.chat({
      sessionId: 'sess-1',
      message: 'Where is my parcel?',
      shipments: [{ id: 'ship-1', awb: 'CM-DEM-N123', status: 'in_transit' }],
    });

    expect(mockPost).toHaveBeenCalledWith('/v1/agents/chat', {
      session_id: 'sess-1',
      message: 'Where is my parcel?',
      shipments: [{ id: 'ship-1', awb: 'CM-DEM-N123', status: 'in_transit' }],
    });
  });

  test('omits the session id on the first turn and defaults shipments to an empty list', async () => {
    mockPost.mockResolvedValue({
      data: { session_id: 'sess-1', reply: 'Hi!', escalated: false, status: 'completed' },
    });

    const res = await aiApi.chat({ message: 'hello' });

    expect(mockPost).toHaveBeenCalledWith('/v1/agents/chat', {
      session_id: undefined,
      message: 'hello',
      shipments: [],
    });
    // The returned session id is what keeps the next turn in the same conversation.
    expect(res.session_id).toBe('sess-1');
    expect(res.reply).toBe('Hi!');
  });

  test('surfaces an escalation flag so the UI can mark the hand-off', async () => {
    mockPost.mockResolvedValue({
      data: {
        session_id: 'sess-3',
        reply: 'Passing you to a colleague.',
        escalated: true,
        status: 'human_escalated',
      },
    });

    const res = await aiApi.chat({ message: 'I want a human' });

    expect(res.escalated).toBe(true);
    expect(res.status).toBe('human_escalated');
  });

  test('maps a 403 to AiUnavailableError so the caller can fall back to offline answers', async () => {
    mockPost.mockRejectedValue(new ApiError(403, 'ai_features not enabled'));

    await expect(aiApi.chat({ message: 'hi' })).rejects.toBeInstanceOf(AiUnavailableError);
  });

  test('propagates other failures unchanged', async () => {
    mockPost.mockRejectedValue(new ApiError(500, 'upstream exploded'));

    const err = await aiApi.chat({ message: 'hi' }).catch((e: unknown) => e);
    expect(err).toBeInstanceOf(ApiError);
    expect(err).not.toBeInstanceOf(AiUnavailableError);
  });
});

describe('aiApi.getChat', () => {
  test('reads conversation state for the given session', async () => {
    mockGet.mockResolvedValue({
      data: {
        session_id: 'sess-9',
        status: 'human_escalated',
        escalated: true,
        resolved_by_human: false,
        latest_reply: null,
      },
    });

    const state = await aiApi.getChat('sess-9');

    expect(mockGet).toHaveBeenCalledWith('/v1/agents/chat/sess-9');
    expect(state.escalated).toBe(true);
    expect(state.resolved_by_human).toBe(false);
  });

  test("carries a human operator's resolution back as the latest reply", async () => {
    mockGet.mockResolvedValue({
      data: {
        session_id: 'sess-9',
        status: 'completed',
        escalated: false,
        resolved_by_human: true,
        latest_reply: 'Refund processed, sorry about that.',
      },
    });

    const state = await aiApi.getChat('sess-9');

    // This pair is what tells SupportScreen to post an operator bubble and
    // retire the session.
    expect(state.resolved_by_human).toBe(true);
    expect(state.latest_reply).toBe('Refund processed, sorry about that.');
  });
});
