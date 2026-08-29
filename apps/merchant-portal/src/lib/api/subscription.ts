import { createApiClient, ApiResponse } from "./client";

export type BillingInterval = "monthly" | "annual";

export type SubscriptionStatus =
  | "pending_payment"
  | "active"
  | "past_due"
  | "cancelled"
  | "lapsed";

export interface SubscriptionPlan {
  id: string;
  /** `growth` or `business`. Starter is free and Enterprise is quoted by
   *  hand, so neither has a plan row — see migration 0019. */
  tier: string;
  interval: BillingInterval;
  currency: string;
  /** The whole charge for one period, not a monthly rate. An annual plan is
   *  one charge covering twelve discounted months. */
  amount_cents: number;
  period_days: number;
}

export interface CurrentSubscription {
  id: string;
  /** What was bought. */
  tier: string;
  /**
   * What the tenant is actually entitled to right now. Differs from `tier`
   * exactly when it matters: a lapsed subscription still records what was
   * bought while entitling the tenant to nothing.
   */
  effective_tier: string;
  status: SubscriptionStatus;
  currency: string;
  amount_cents: number;
  current_period_start: string | null;
  current_period_end: string | null;
  cancelled_at: string | null;
  /**
   * Whether the tier has actually reached the identity service. `false` after
   * a captured payment means the money moved and the entitlement did not —
   * payments retries on a sweep, but support needs to be able to see it.
   */
  entitlement_synced: boolean;
}

export interface SubscriptionCheckout {
  subscription_id: string;
  tier: string;
  amount_cents: number;
  currency: string;
  /** The hosted card page. Card details are entered on the gateway's own
   *  page, never in this app. */
  checkout_url: string;
}

export const subscriptionApi = {
  async listPlans(currency?: string): Promise<SubscriptionPlan[]> {
    const client = createApiClient();
    const { data } = await client.get<
      ApiResponse<{ plans: SubscriptionPlan[]; currency: string }>
    >("/v1/subscriptions/plans", { params: currency ? { currency } : undefined });
    return data.data?.plans ?? [];
  },

  /** `null` for a tenant that has never subscribed — the Starter case, not an error. */
  async current(): Promise<CurrentSubscription | null> {
    const client = createApiClient();
    const { data } = await client.get<ApiResponse<CurrentSubscription | null>>(
      "/v1/subscriptions/me",
    );
    return data.data ?? null;
  },

  /**
   * Buy or change a plan. Returns a hosted card page and changes nothing about
   * the tenant's entitlement — abandoning the page leaves them exactly as they
   * were. The tier only moves when the payment is captured.
   */
  async checkout(
    tier: string,
    interval: BillingInterval,
    currency?: string,
  ): Promise<SubscriptionCheckout> {
    const client = createApiClient();
    const { data } = await client.post<ApiResponse<SubscriptionCheckout>>(
      "/v1/subscriptions/checkout",
      { tier, interval, currency },
    );
    return data.data as SubscriptionCheckout;
  },

  /** Stop at the end of the paid period. Not a refund and not an immediate downgrade. */
  async cancel(): Promise<void> {
    const client = createApiClient();
    await client.post("/v1/subscriptions/me/cancel");
  },
};

/** `$149.00`, from cents and an ISO currency code. */
export function formatMoney(cents: number, currency: string): string {
  return new Intl.NumberFormat("en-US", {
    style: "currency",
    currency,
    minimumFractionDigits: 2,
  }).format(cents / 100);
}

/**
 * What a plan works out to per month, for comparing an annual charge against a
 * monthly one. Presentational only — the charge is `amount_cents`.
 */
export function perMonthCents(plan: SubscriptionPlan): number {
  const months = Math.max(1, Math.round(plan.period_days / 30));
  return Math.round(plan.amount_cents / months);
}
