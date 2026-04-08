/**
 * LogisticOS Loyalty Program — Business Logic
 *
 * EARNING:
 *   Local shipment            →  50 pts  (₱1 = 1 pt roughly at base ₱50 rate)
 *   International / Balikbayan → 150 pts
 *   COD shipment bonus        →  +20 pts (extra for trusting us with cash)
 *   First booking ever        → +100 pts welcome bonus
 *   Referral (referred user books) → +200 pts
 *
 * TIERS (lifetime points — never expire):
 *   Bronze   0–199 pts    no perks baseline
 *   Silver   200–499 pts  5% discount on local bookings
 *   Gold     500–999 pts  10% discount on all bookings + priority support
 *   Platinum 1000+ pts    15% discount + free fragile handling + VIP support
 *
 * REDEMPTION (spend points for cash discount at checkout):
 *   Minimum to redeem: 100 pts
 *   Rate: 1 pt = ₱0.10  (100 pts = ₱10 off)
 *   Max redemption per booking: 50% of booking total (can't pay full with points)
 *   Points deducted immediately on booking confirmation
 *
 * EXPIRY:
 *   Points do NOT expire as long as account is active
 *   Account inactive > 12 months: points freeze (not lost, just locked)
 */

export interface LoyaltyTier {
  label:     string;
  min:       number;
  max:       number | null;
  color:     string;
  icon:      string;
  discount:  number;       // percentage discount 0–15
  perks:     string[];
}

export const LOYALTY_TIERS: LoyaltyTier[] = [
  {
    label:    "Bronze",
    min:      0,
    max:      199,
    color:    "#CD7F32",
    icon:     "ribbon-outline",
    discount: 0,
    perks:    ["Earn 50 pts per local booking", "Earn 150 pts per international booking"],
  },
  {
    label:    "Silver",
    min:      200,
    max:      499,
    color:    "#C0C0C0",
    icon:     "ribbon-outline",
    discount: 5,
    perks:    ["5% off all local bookings", "Earn 60 pts per local booking", "Priority email support"],
  },
  {
    label:    "Gold",
    min:      500,
    max:      999,
    color:    "#FFAB00",
    icon:     "star-outline",
    discount: 10,
    perks:    ["10% off all bookings", "Earn 75 pts per local booking", "Free fragile surcharge waiver", "Priority phone support"],
  },
  {
    label:    "Platinum",
    min:      1000,
    max:      null,
    color:    "#00E5FF",
    icon:     "diamond-outline",
    discount: 15,
    perks:    ["15% off all bookings", "Earn 100 pts per local booking", "Free fragile surcharge", "Dedicated VIP support line", "Free re-delivery on failed attempts"],
  },
];

// Points awarded per booking type
export const EARN_RATES = {
  local:         50,
  international: 150,
  cod_bonus:     20,
  first_booking: 100,
} as const;

// Redemption config
export const REDEMPTION_RATE   = 0.10;  // 1 pt = ₱0.10
export const REDEMPTION_MIN    = 100;   // minimum pts to redeem
export const REDEMPTION_MAX_PCT = 0.50; // max 50% of total can be paid with points

/** Get the current tier for a points balance */
export function getTier(pts: number): LoyaltyTier {
  return LOYALTY_TIERS.find(t => pts >= t.min && (t.max === null || pts <= t.max)) ?? LOYALTY_TIERS[0];
}

/** Get the next tier, or null if already Platinum */
export function getNextTier(pts: number): LoyaltyTier | null {
  const idx = LOYALTY_TIERS.findIndex(t => pts >= t.min && (t.max === null || pts <= t.max));
  return idx < LOYALTY_TIERS.length - 1 ? LOYALTY_TIERS[idx + 1] : null;
}

/** Points needed to reach the next tier */
export function ptsToNextTier(pts: number): number {
  const next = getNextTier(pts);
  return next ? Math.max(0, next.min - pts) : 0;
}

/** Progress 0–1 within the current tier band */
export function tierProgress(pts: number): number {
  const tier = getTier(pts);
  if (tier.max === null) return 1;
  const range = tier.max - tier.min + 1;
  return Math.min(1, (pts - tier.min) / range);
}

/** Points to earn for a given booking */
export function calcEarnedPoints(params: {
  type: 'local' | 'international';
  isCOD: boolean;
  isFirstBooking: boolean;
  currentPts: number;
}): number {
  const tier    = getTier(params.currentPts);
  const baseMap = { local: EARN_RATES.local, international: EARN_RATES.international };
  // Gold+ earns more per booking
  let base = tier.label === 'Platinum' ? 100
           : tier.label === 'Gold'     ? 75
           : tier.label === 'Silver'   ? 60
           : baseMap[params.type];
  if (params.type === 'international') base = EARN_RATES.international; // always 150 for intl
  const cod   = params.isCOD            ? EARN_RATES.cod_bonus     : 0;
  const first = params.isFirstBooking   ? EARN_RATES.first_booking : 0;
  return base + cod + first;
}

/** Max PHP discount from points redemption for a given booking total */
export function maxRedemptionValue(pts: number, bookingTotal: number): number {
  if (pts < REDEMPTION_MIN) return 0;
  const maxFromPts  = pts * REDEMPTION_RATE;
  const maxFromPct  = bookingTotal * REDEMPTION_MAX_PCT;
  return Math.min(maxFromPts, maxFromPct);
}

/** Points required to get a given PHP discount */
export function ptsForDiscount(phpAmount: number): number {
  return Math.ceil(phpAmount / REDEMPTION_RATE);
}

/** Apply tier discount to a booking total */
export function applyTierDiscount(total: number, pts: number): number {
  const discount = getTier(pts).discount / 100;
  return Math.round(total * (1 - discount));
}

/** Format points as currency value */
export function ptsToPhp(pts: number): string {
  return `₱${(pts * REDEMPTION_RATE).toFixed(2)}`;
}
