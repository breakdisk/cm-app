import { describeCancellation, LEGACY_CANCEL_COPY } from '../cancellation';
import type { CancellationPreview } from '../../services/api/shipments';

const preview = (over: Partial<CancellationPreview> = {}): CancellationPreview => ({
  cancellable: true,
  policy_applies: false,
  tier: 'unscheduled',
  hours_to_pickup: null,
  fee_bps: 0,
  fee_cents: null,
  refund_cents: null,
  currency: null,
  policy_version: null,
  ...over,
});

describe('describeCancellation', () => {
  // A backend without the endpoint must get exactly the old sheet.
  test('without a preview the old unpriced confirm stands', () => {
    expect(describeCancellation(null)).toEqual({ canCancel: true, headline: LEGACY_CANCEL_COPY });
  });

  test('the server refusing hides the confirm', () => {
    const t = describeCancellation(preview({ cancellable: false }));
    expect(t.canCancel).toBe(false);
    expect(t.headline).toMatch(/can no longer be cancelled/);
  });

  test('an unscheduled shipment is free', () => {
    const t = describeCancellation(preview());
    expect(t).toEqual({ canCancel: true, headline: 'No cancellation fee applies.', detail: undefined });
  });

  // The shipped policy is 0 bps everywhere; a scheduled job must still read free.
  test('a scheduled job at a 0% rate reads free, with the refund', () => {
    const t = describeCancellation(preview({
      policy_applies: true, tier: 'late', hours_to_pickup: 19, fee_bps: 0,
      fee_cents: 0, refund_cents: 20_000, currency: 'PHP',
    }));
    expect(t.headline).toBe('No cancellation fee applies.');
    expect(t.detail).toBe('Estimated refund PHP 200.00.');
  });

  test('a late cancellation shows the server fee and refund, never its own arithmetic', () => {
    const t = describeCancellation(preview({
      policy_applies: true, tier: 'late', hours_to_pickup: 19.6, fee_bps: 1_500,
      fee_cents: 3_000, refund_cents: 17_000, currency: 'PHP',
    }));
    expect(t.headline).toBe('Cancelling now costs PHP 30.00.');
    expect(t.detail).toBe('Pickup is 19 hours away. 15% cancellation fee. Estimated refund PHP 170.00.');
  });

  test('a cash booking has a rate but no amount', () => {
    const t = describeCancellation(preview({ policy_applies: true, tier: 'same_day', fee_bps: 1_250 }));
    expect(t.headline).toBe('A 12.5% cancellation fee applies.');
    expect(t.detail).toBe('The scheduled pickup time has passed.');
  });
});
