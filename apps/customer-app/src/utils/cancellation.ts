/**
 * What the cancel sheet says, from `GET /v1/shipments/:id/cancellation-preview`.
 *
 * The server prices the cancellation; this only words it. No arithmetic on the
 * device clock or the booking total happens here — the sheet shows the server's
 * numbers or none.
 *
 * `null` preview means the server predates the endpoint. The sheet then says
 * what it always said, so this build works against a backend that has not
 * deployed the priced cancel yet.
 */
import type { CancellationPreview } from "../services/api/shipments";

export interface CancellationTerms {
  /** False only when the server says so. */
  canCancel: boolean;
  headline:  string;
  detail?:   string;
}

export const LEGACY_CANCEL_COPY =
  "This cannot be undone. The shipment will be cancelled and a driver will not be dispatched.";

function money(cents: number, currency: string | null | undefined): string {
  const amount = (cents / 100).toFixed(2);
  return currency ? `${currency} ${amount}` : amount;
}

/** 1500 → "15%", 1250 → "12.5%". */
function percent(bps: number): string {
  return `${+(bps / 100).toFixed(2)}%`;
}

export function describeCancellation(preview: CancellationPreview | null): CancellationTerms {
  if (!preview) {
    return { canCancel: true, headline: LEGACY_CANCEL_COPY };
  }

  if (!preview.cancellable) {
    return {
      canCancel: false,
      headline: "This shipment can no longer be cancelled.",
      detail: "It has moved past the point where a cancellation is possible. Contact support if something has changed.",
    };
  }

  const refund = preview.refund_cents != null && preview.refund_cents > 0
    ? `Estimated refund ${money(preview.refund_cents, preview.currency)}.`
    : undefined;

  if (!preview.policy_applies || preview.fee_bps <= 0) {
    return { canCancel: true, headline: "No cancellation fee applies.", detail: refund };
  }

  const when = preview.tier === "same_day"
    ? "The scheduled pickup time has passed."
    : preview.hours_to_pickup != null
      ? `Pickup is ${Math.max(0, Math.floor(preview.hours_to_pickup))} hours away.`
      : undefined;

  if (preview.fee_cents != null) {
    return {
      canCancel: true,
      headline: `Cancelling now costs ${money(preview.fee_cents, preview.currency)}.`,
      detail: [when, `${percent(preview.fee_bps)} cancellation fee.`, refund].filter(Boolean).join(" "),
    };
  }

  // A rate but no amount: nothing was paid online, so there is no total to apply it to.
  return {
    canCancel: true,
    headline: `A ${percent(preview.fee_bps)} cancellation fee applies.`,
    detail: when,
  };
}
