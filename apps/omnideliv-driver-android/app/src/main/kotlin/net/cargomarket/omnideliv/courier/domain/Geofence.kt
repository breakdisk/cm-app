package net.cargomarket.omnideliv.courier.domain

import kotlin.math.asin
import kotlin.math.cos
import kotlin.math.min
import kotlin.math.sin
import kotlin.math.sqrt

/** Within this, the courier is treated as at the stop. */
const val AT_STOP_METERS = 50.0

/**
 * A fix older than this is not a position.
 *
 * Matches `FIX_STALE_AFTER_SECS` in field-ops and omnideliv. Duplicated rather
 * than shared: the three do not depend on each other, and a crate for one
 * integer would couple them. This comment is the tripwire.
 */
const val FIX_STALE_AFTER_SECS = 120L

/**
 * What the UI says about the courier's distance from the stop.
 *
 * Never whether the button works. The button always works — see [GeofenceAdvice].
 */
sealed interface GeofenceAdvice {
    /** Close enough that no comment is needed beyond the distance. */
    data class AtStop(val meters: Int) : GeofenceAdvice

    /** Further away than expected. Advisory: the commit is still allowed. */
    data class Away(val meters: Int) : GeofenceAdvice

    /** No usable fix. Also advisory — a basement is not a reason to strand. */
    data object NoFix : GeofenceAdvice
}

/**
 * Advise, never block.
 *
 * A hard gate strands a courier standing at the door in an urban canyon, a lift
 * lobby or a basement. With cash on delivery the money is already in their hand
 * when the door closes, so refusing the button cannot un-collect it — it only
 * prevents the system recording what already happened, which is strictly worse
 * than recording it with a flag.
 *
 * This deliberately differs from the POD service's hard 200 m gate and follows
 * the platform's `OUT_OF_BOUNDS_HANDOVER` soft-flag precedent instead.
 */
fun adviseGeofence(
    courierLat: Double?,
    courierLng: Double?,
    fixAgeSeconds: Long?,
    stopLat: Double,
    stopLng: Double,
): GeofenceAdvice {
    if (courierLat == null || courierLng == null) return GeofenceAdvice.NoFix
    if (fixAgeSeconds == null || fixAgeSeconds > FIX_STALE_AFTER_SECS) return GeofenceAdvice.NoFix

    val meters = haversineMeters(courierLat, courierLng, stopLat, stopLng)
    return if (meters <= AT_STOP_METERS) {
        GeofenceAdvice.AtStop(meters.toInt())
    } else {
        GeofenceAdvice.Away(meters.toInt())
    }
}

/**
 * Whether this commit should carry an out-of-bounds flag for ops.
 *
 * Separate from [adviseGeofence] so the UI hint and the audit annotation cannot
 * drift apart — they are one rule with two consumers. No fix is *not* an
 * exception: the courier cannot be blamed for a dead GPS, and flagging every
 * basement would make the flag meaningless.
 */
fun isOutOfBounds(advice: GeofenceAdvice): Boolean = advice is GeofenceAdvice.Away

/**
 * Great-circle distance in metres.
 *
 * `min(1.0, ...)` guards the square root: for two identical points floating
 * point can put the argument a hair above 1, and `asin` of that is NaN — which
 * would render as a blank distance at exactly the moment the courier is
 * standing on the pin.
 */
fun haversineMeters(lat1: Double, lng1: Double, lat2: Double, lng2: Double): Double {
    val earthRadiusM = 6_371_000.0
    val dLat = Math.toRadians(lat2 - lat1)
    val dLng = Math.toRadians(lng2 - lng1)
    val a = sin(dLat / 2) * sin(dLat / 2) +
        cos(Math.toRadians(lat1)) * cos(Math.toRadians(lat2)) *
        sin(dLng / 2) * sin(dLng / 2)
    return 2 * earthRadiusM * asin(min(1.0, sqrt(a)))
}
