// Imports the pure module, not the hook: the hook pulls expo/fetch, which
// fails to construct under jest-expo before any assertion runs.
import { parseSseChunk, reconnectDelayMs } from "../../api/sse";

describe("parseSseChunk", () => {
  it("extracts one complete event and keeps the remainder buffered", () => {
    const { events, rest } = parseSseChunk('data: {"event":"intent_parsed","sub_intent_count":2}\n\ndata: {"eve');
    expect(events).toHaveLength(1);
    expect(events[0]).toEqual({ event: "intent_parsed", sub_intent_count: 2 });
    expect(rest).toBe('data: {"eve');
  });

  it("extracts several events from one chunk", () => {
    const chunk =
      'data: {"event":"intent_parsed","sub_intent_count":2}\n\n' +
      'data: {"event":"specialist_started","sub_intent_id":"a","role":"nutritionist","vertical":"grocery","label":"Checking grocery"}\n\n';
    const { events, rest } = parseSseChunk(chunk);
    expect(events).toHaveLength(2);
    expect(rest).toBe("");
  });

  /**
   * A truncated frame must stay buffered, not be dropped. Dropping it loses a
   * specialist card and the screen shows fewer agents than are actually running.
   */
  it("buffers a partial frame rather than dropping it", () => {
    const { events, rest } = parseSseChunk('data: {"event":"spec');
    expect(events).toHaveLength(0);
    expect(rest).toBe('data: {"event":"spec');
  });

  /** Keep-alive comments must not be parsed as events. */
  it("ignores comment frames", () => {
    const { events } = parseSseChunk(": keep-alive\n\n");
    expect(events).toHaveLength(0);
  });

  /** A malformed frame is skipped, not fatal — one bad event must not kill the stream. */
  it("skips an unparseable frame and keeps going", () => {
    const { events } = parseSseChunk('data: not-json\n\ndata: {"event":"failed","reason":"x"}\n\n');
    expect(events).toHaveLength(1);
    expect(events[0]).toMatchObject({ event: "failed" });
  });

  /**
   * The real failure mode this parser exists for: a frame split across two
   * network reads must reassemble into exactly one event, not zero and not two.
   */
  it("reassembles an event split across two chunks", () => {
    const first = parseSseChunk('data: {"event":"completed","basket_id":"b1","nee');
    expect(first.events).toHaveLength(0);

    const second = parseSseChunk(first.rest + 'ds_review":2}\n\n');
    expect(second.events).toHaveLength(1);
    expect(second.events[0]).toEqual({ event: "completed", basket_id: "b1", needs_review: 2 });
    expect(second.rest).toBe("");
  });
});

describe("reconnectDelayMs", () => {
  it("backs off exponentially with a ceiling", () => {
    expect(reconnectDelayMs(0)).toBe(500);
    expect(reconnectDelayMs(1)).toBe(1000);
    expect(reconnectDelayMs(2)).toBe(2000);
    expect(reconnectDelayMs(10)).toBe(8000);
  });

  /**
   * Backoff is a pure function of the attempt number precisely so it can be
   * tested without fake timers. `jest.useFakeTimers()` against a loop that
   * schedules its own next tick shares one virtual clock and hangs instead of
   * failing — that trap burned two six-hour CI runs on the Android app.
   */
  it("is pure, so no timer mocking is needed to test it", () => {
    expect(reconnectDelayMs(3)).toBe(reconnectDelayMs(3));
  });
});
