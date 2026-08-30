"use client";
/**
 * OmniDeliv vendor order console.
 *
 * Tier 0 of ADR-0017's notification design, and the tier that actually works in
 * a kitchen: the screen is already open on the counter, so a poll and a sound
 * beat a push notification nobody has enabled on a device nobody is holding.
 *
 * It polls unconditionally. The queue endpoint is the record and every other
 * channel is a hint, so this screen must never depend on having received one.
 */
import { useCallback, useEffect, useRef, useState } from "react";

import { NewOrderAlarm } from "@/components/storefront/new-order-alarm";
import { OrderQueue } from "@/components/storefront/order-queue";
import { vendorOrdersApi, type VendorLegRow } from "@/lib/api/vendor-orders";

/** Fast enough that a customer is not waiting on a screen refresh. */
const POLL_MS = 10_000;

export default function VendorOrdersPage() {
  const [legs, setLegs] = useState<VendorLegRow[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [muted, setMuted] = useState(false);
  const inFlight = useRef(false);

  const refresh = useCallback(async () => {
    // A slow response must not stack requests behind it. The next tick is
    // never far away, so skipping is cheaper than queueing.
    if (inFlight.current) return;
    inFlight.current = true;
    try {
      setLegs(await vendorOrdersApi.queue());
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not load the order queue");
    } finally {
      inFlight.current = false;
      setLoaded(true);
    }
  }, []);

  useEffect(() => {
    void refresh();
    const id = setInterval(() => void refresh(), POLL_MS);
    return () => clearInterval(id);
  }, [refresh]);

  const unanswered = legs.filter((l) => l.status === "pending").length;

  // Muting silences the orders on screen right now; a genuinely NEW unanswered
  // order re-arms it. A permanent mute is how a store quietly stops hearing
  // about orders at all, which is the failure this whole tier exists to
  // prevent — so the button cannot produce one.
  const mutedAt = useRef(0);
  useEffect(() => {
    if (muted) mutedAt.current = unanswered;
  }, [muted, unanswered]);

  const alarmActive = unanswered > 0 && (!muted || unanswered > mutedAt.current);

  return (
    <div className="space-y-6">
      <NewOrderAlarm active={alarmActive} />
      <OrderQueue
        legs={legs}
        loaded={loaded}
        error={error}
        unanswered={unanswered}
        muted={muted}
        onToggleMute={() => setMuted((m) => !m)}
        onChanged={refresh}
      />
    </div>
  );
}
