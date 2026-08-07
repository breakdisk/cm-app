/**
 * SSE frame parsing and reconnect backoff.
 *
 * Deliberately free of React and Expo imports. `useMeshRun` pulls `expo/fetch`,
 * which cannot be constructed under jest-expo's environment — importing the hook
 * from a test fails at module load before a single assertion runs. Keeping the
 * pure logic here means the parts most worth testing are testable without a
 * React Native runtime at all.
 */
import type { MeshEvent } from "./mesh";

/** Split a buffer into complete SSE events plus the unconsumed remainder. */
export function parseSseChunk(buffer: string): { events: MeshEvent[]; rest: string } {
  const events: MeshEvent[] = [];
  const frames = buffer.split("\n\n");
  // The last element is either "" (buffer ended on a boundary) or a partial
  // frame. Either way it stays buffered — dropping it would lose an event.
  const rest = frames.pop() ?? "";

  for (const frame of frames) {
    const line = frame.split("\n").find((l) => l.startsWith("data:"));
    if (!line) continue; // comment / keep-alive
    const payload = line.slice(5).trim();
    if (!payload) continue;
    try {
      events.push(JSON.parse(payload) as MeshEvent);
    } catch {
      // One malformed frame must not kill the stream.
      continue;
    }
  }

  return { events, rest };
}

const BASE_DELAY = 500;
const MAX_DELAY = 8000;

/**
 * Exponential backoff with a ceiling.
 *
 * A pure function of the attempt number precisely so it can be tested without
 * fake timers: `jest.useFakeTimers()` against a loop that schedules its own
 * next tick shares one virtual clock and hangs instead of failing, which cost
 * this repo two six-hour CI runs on the Android app.
 */
export function reconnectDelayMs(attempt: number): number {
  return Math.min(BASE_DELAY * 2 ** attempt, MAX_DELAY);
}
