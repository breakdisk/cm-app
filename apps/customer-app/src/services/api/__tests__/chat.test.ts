import { makeClientMessageId, mergeMessages, type JobMessage } from '../chat';

function message(id: string, minute: number, role: 'customer' | 'driver' = 'driver'): JobMessage {
  return {
    id,
    shipment_id: 'ship-1',
    sender_id: role === 'driver' ? 'driver-1' : 'me',
    sender_role: role,
    body: `message ${id}`,
    created_at: `2026-09-16T09:${String(minute).padStart(2, '0')}:00Z`,
  };
}

describe('mergeMessages', () => {
  test('keeps the thread in order, oldest first', () => {
    const merged = mergeMessages([message('b', 2)], [message('a', 1), message('c', 3)]);
    expect(merged.map((m) => m.id)).toEqual(['a', 'b', 'c']);
  });

  // The phone adds its own message as soon as the server accepts it; the next
  // poll returns the same message, and it must not show up twice.
  test('a message already on screen is not added again', () => {
    const mine = message('a', 1, 'customer');
    const merged = mergeMessages([mine], [mine, message('b', 2)]);
    expect(merged.map((m) => m.id)).toEqual(['a', 'b']);
  });

  test('nothing new leaves the thread as it was', () => {
    const current = [message('a', 1), message('b', 2)];
    expect(mergeMessages(current, [])).toHaveLength(2);
  });

  test('messages sent in the same second still have a stable order', () => {
    const first = { ...message('a', 5), created_at: '2026-09-16T09:05:00Z' };
    const second = { ...message('b', 5), created_at: '2026-09-16T09:05:00Z' };
    expect(mergeMessages([second], [first]).map((m) => m.id)).toEqual(['a', 'b']);
  });

  test('an unreadable timestamp does not drop the message', () => {
    const broken = { ...message('z', 1), created_at: 'not a date' };
    expect(mergeMessages([broken], [message('a', 2)]).map((m) => m.id).sort()).toEqual(['a', 'z']);
  });
});

describe('makeClientMessageId', () => {
  test('looks like a uuid and differs each time', () => {
    const a = makeClientMessageId();
    const b = makeClientMessageId();
    expect(a).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-a[0-9a-f]{3}-[0-9a-f]{12}$/);
    expect(a).not.toBe(b);
  });
});
