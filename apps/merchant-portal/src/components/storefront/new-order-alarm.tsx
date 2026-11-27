"use client";
/**
 * Repeating audible alert for unanswered orders.
 *
 * Synthesised with WebAudio rather than shipped as an asset: no file to 404,
 * no bundle weight, and nothing for a blocked CDN to silence.
 *
 * It repeats until acknowledged on purpose. A single chime is missed over an
 * extractor fan, and a store that misses the chime is a customer waiting on
 * food nobody started. This is Tier 0 of ADR-0017's transport, and the reason
 * it is Tier 0 is that the screen is already open on the counter — it needs no
 * credentials, no phone number, and nobody to have enabled anything.
 */
import { useEffect, useRef } from "react";

/** How often the alert repeats while at least one order is unanswered. */
const REPEAT_MS = 15_000;

export function NewOrderAlarm({ active }: { active: boolean }) {
  const ctxRef = useRef<AudioContext | null>(null);

  useEffect(() => {
    if (!active) return;

    const beep = () => {
      try {
        // Created lazily and reused. Browsers refuse an AudioContext built
        // before a user gesture, and building one per beep leaks them until
        // the tab is closed — a console left open all service would end up
        // with hundreds.
        const Ctor =
          window.AudioContext ??
          (window as unknown as { webkitAudioContext?: typeof AudioContext })
            .webkitAudioContext;
        if (!Ctor) return;
        const ctx = (ctxRef.current ??= new Ctor());
        if (ctx.state === "suspended") void ctx.resume();

        const osc = ctx.createOscillator();
        const gain = ctx.createGain();
        osc.type = "sine";
        osc.frequency.value = 880;
        // Ramped rather than gated: an abrupt start or stop on a sine wave
        // clicks audibly, which over a shift is worse than the alert itself.
        gain.gain.setValueAtTime(0.0001, ctx.currentTime);
        gain.gain.exponentialRampToValueAtTime(0.25, ctx.currentTime + 0.02);
        gain.gain.exponentialRampToValueAtTime(0.0001, ctx.currentTime + 0.45);
        osc.connect(gain).connect(ctx.destination);
        osc.start();
        osc.stop(ctx.currentTime + 0.5);
      } catch {
        // Audio is an enhancement. A browser that refuses it must not take the
        // queue down with it — the list on screen is still the record.
      }
    };

    beep();
    const id = setInterval(beep, REPEAT_MS);
    return () => clearInterval(id);
  }, [active]);

  // Closed on unmount only, not when `active` flips — the context is reused
  // across quiet spells, and closing it would need a fresh user gesture to
  // rebuild.
  useEffect(
    () => () => {
      void ctxRef.current?.close();
    },
    [],
  );

  return null;
}
