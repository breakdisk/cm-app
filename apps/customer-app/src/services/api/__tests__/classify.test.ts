const mockPost = jest.fn();

jest.mock('../ai', () => ({
  getAiClient: jest.fn(() => ({ post: mockPost })),
}));

import { classifyPrompt, route, type Classification } from '../classify';
import type { ParsedMove } from '../../../screens/move/parsePrompt';

const regex: ParsedMove = { items: [{ name: 'sofa', qty: 1 }], from: '', to: 'Makati', when: '' };

const ai = (over: Partial<Classification> = {}): Classification => ({
  intent: 'book',
  confidence: 0.9,
  extracted: { items: [{ name: 'sofa', qty: 1 }, { name: 'boxes', qty: 6 }], from: 'BGC', to: 'Makati', when: 'tomorrow 9am' },
  ...over,
});

beforeEach(() => jest.clearAllMocks());

describe('route', () => {
  test('a confident server reading is planned from, and recorded as the AI’s', () => {
    const r = route(ai(), regex, 'book', 'prompt');
    expect(r.intent).toBe('book');
    expect(r.parsed.items).toHaveLength(2);
    expect(r.parsed.from).toBe('BGC');
    expect(r.intake).toMatchObject({ source: 'prompt_ai', intent: 'book', confidence: 0.9 });
  });

  test('an unsure server loses to the regex', () => {
    const r = route(ai({ confidence: 0.3 }), regex, 'book', 'voice');
    expect(r.parsed).toBe(regex);
    expect(r.intake.source).toBe('voice_offline');
  });

  test('no answer at all is the offline path', () => {
    const r = route(null, regex, 'support', 'prompt');
    expect(r.intent).toBe('support');
    expect(r.intake).toMatchObject({ source: 'prompt_offline', intent: 'support' });
  });

  test('a booking the server read nothing from keeps the regex parse', () => {
    const empty = ai({ extracted: { items: [], from: '', to: '', when: '' } });
    expect(route(empty, regex, 'book', 'prompt').parsed).toBe(regex);
  });

  test('a whole-home move is routed even when the server stated nothing, with the regex parse', () => {
    const home = ai({ intent: 'home_move', extracted: { items: [], from: '', to: '', when: '' } });
    const r = route(home, regex, 'book', 'prompt');
    expect(r.intent).toBe('home_move');
    expect(r.parsed).toBe(regex);
  });

  test('quantities are whole and at least one', () => {
    const odd = ai({ extracted: { items: [{ name: 'chair', qty: 0 }, { name: 'box', qty: 2.6 }], from: 'A', to: 'B', when: '' } });
    expect(route(odd, regex, 'book', 'prompt').parsed.items.map((i) => i.qty)).toEqual([1, 3]);
  });
});

describe('classifyPrompt', () => {
  test('sends the text and whether a job is running, with a timeout', async () => {
    mockPost.mockResolvedValueOnce({ data: { data: ai() } });
    await classifyPrompt('move my sofa', true, 1234);
    expect(mockPost).toHaveBeenCalledWith('/v1/agents/classify', { text: 'move my sofa', has_active_job: true }, { timeout: 1234 });
  });

  test('any failure — refusal, timeout, offline — is null, never a throw', async () => {
    mockPost.mockRejectedValueOnce({ status: 403 });
    await expect(classifyPrompt('move my sofa', false)).resolves.toBeNull();
  });
});
