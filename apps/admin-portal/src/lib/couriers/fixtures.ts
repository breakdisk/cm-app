/**
 * Payload shapes this screen has to survive.
 *
 * `LEGACY_*` is not invented. It is the exact object
 * `GET /v1/field-ops/admin/couriers` returned from the production API on
 * 2026-08-25, when the running field-ops image predated the compliance gate:
 * thirteen keys, with `block_reason`, `compliance_status` and
 * `compliance_assignable` **absent** rather than null. Keep it byte-faithful —
 * its whole value is that nobody wrote it to suit the code.
 *
 * The `as AdminCourier` casts on the legacy fixtures are deliberate and are the
 * point: those objects genuinely lack keys the interface declares, and pretending
 * otherwise would defeat the test.
 */
import type { AdminCourier } from "@/lib/api/couriers";

/** Live, 2026-08-25. Off duty, and the server said nothing about why. */
export const LEGACY_OFF_DUTY = {
  id:           "10ae4c3d-3dd3-4480-95d3-051ba3b20d36",
  user_id:      "10ae4c3d-3dd3-4480-95d3-051ba3b20d36",
  first_name:   "Courier",
  last_name:    "",
  phone:        "63581208617",
  status:       "offline",
  is_active:    true,
  vehicle_type: null,
  zone:         null,
  last_lat:     24.5000267,
  last_lng:     54.372825,
  last_seen_at: "2026-08-18T11:10:58.339168Z",
  dispatchable: false,
} as AdminCourier;

/** Live, 2026-08-25. On duty, with a fix recent as of `NOW_MS`. */
export const LEGACY_AVAILABLE = {
  id:           "761d071d-81e8-414b-88b8-c2c02caad198",
  user_id:      "761d071d-81e8-414b-88b8-c2c02caad198",
  first_name:   "Courier",
  last_name:    "",
  phone:        "971581206817",
  status:       "available",
  is_active:    true,
  vehicle_type: null,
  zone:         null,
  last_lat:     24.5018547,
  last_lng:     54.3737363,
  last_seen_at: "2026-08-23T09:10:09.602427Z",
  dispatchable: true,
} as AdminCourier;

/** Legacy shape, suspended by ops. */
export const LEGACY_SUSPENDED = {
  ...LEGACY_AVAILABLE,
  id:           "8a1f0000-0000-0000-0000-00000000000a",
  user_id:      "8a1f0000-0000-0000-0000-00000000000a",
  is_active:    false,
  dispatchable: false,
} as AdminCourier;

/** Post-#138: compliance has never seen this courier. */
export const CURRENT_NOT_ONBOARDED: AdminCourier = {
  ...LEGACY_AVAILABLE,
  id:                    "b0000000-0000-0000-0000-00000000000b",
  user_id:               "b0000000-0000-0000-0000-00000000000b",
  block_reason:          null,
  compliance_status:     null,
  compliance_assignable: true,
};

/** Post-#138: compliant, working. */
export const CURRENT_COMPLIANT: AdminCourier = {
  ...LEGACY_AVAILABLE,
  id:                    "c0000000-0000-0000-0000-00000000000c",
  user_id:               "c0000000-0000-0000-0000-00000000000c",
  block_reason:          null,
  compliance_status:     "compliant",
  compliance_assignable: true,
};

/**
 * Post-#138, observe-only: compliance has refused them and they are still being
 * offered work because `ENFORCE_COMPLIANCE` is false. `dispatchable` and
 * `compliance_assignable` disagree on purpose — that disagreement is the
 * rollout, and this screen is where it is visible before the flag flips.
 */
export const CURRENT_OBSERVE_ONLY: AdminCourier = {
  ...LEGACY_AVAILABLE,
  id:                    "d0000000-0000-0000-0000-00000000000d",
  user_id:               "d0000000-0000-0000-0000-00000000000d",
  dispatchable:          true,
  block_reason:          null,
  compliance_status:     "rejected",
  compliance_assignable: false,
};

/** Post-#138 with enforcement on: the server itself names compliance. */
export const CURRENT_ENFORCED_BLOCK: AdminCourier = {
  ...LEGACY_AVAILABLE,
  id:                    "e0000000-0000-0000-0000-00000000000e",
  user_id:               "e0000000-0000-0000-0000-00000000000e",
  dispatchable:          false,
  block_reason:          "compliance",
  compliance_status:     "expired",
  compliance_assignable: false,
};

/**
 * Fixed clock for the GPS-age branch: five minutes after `LEGACY_AVAILABLE`'s
 * last fix, so that courier is fresh and anything older than ten minutes is not.
 * Passed in rather than read, so the suite does not drift into failing with age.
 */
export const NOW_MS = Date.parse("2026-08-23T09:15:00.000Z");
