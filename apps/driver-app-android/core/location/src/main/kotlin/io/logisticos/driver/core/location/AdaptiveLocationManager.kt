package io.logisticos.driver.core.location

/**
 * Chooses the GPS sampling interval from the driver's speed.
 *
 * The intervals balance two things that pull against each other: battery over a
 * full shift, and breadcrumb density — which is not merely telemetry but the
 * chain-of-custody trail between POP and POD.
 *
 * [INTERVAL_DRIVING_MS] was 2 s, which with the service's
 * `setMinUpdateIntervalMillis(interval / 2)` permitted fixes as often as once per
 * second at PRIORITY_HIGH_ACCURACY — continuous GNSS at full duty cycle for the
 * length of a shift. At 5 s a vehicle at 60 km/h still leaves a breadcrumb roughly
 * every 80 m, which is ample to reconstruct a route, while roughly halving the
 * sampling rate. Raise it back only if route reconstruction proves too coarse;
 * lower it further only with battery measurements to justify the loss.
 */
object AdaptiveLocationManager {
    const val STATIONARY_THRESHOLD_MS = 120_000L  // 2 minutes
    private const val SPEED_THRESHOLD_MPS = 1.39f  // 5 km/h in m/s

    /** Moving faster than [SPEED_THRESHOLD_MPS]. ~80 m spacing at 60 km/h. */
    private const val INTERVAL_DRIVING_MS = 5_000L

    /** Walking or crawling in traffic — position changes slowly. */
    private const val INTERVAL_SLOW_MS = 15_000L

    /** Parked. Kept at 30 s so a driver who resumes is picked up promptly. */
    const val INTERVAL_STATIONARY_MS = 30_000L

    fun intervalForSpeed(speedMps: Float): Long =
        if (speedMps > SPEED_THRESHOLD_MPS) INTERVAL_DRIVING_MS else INTERVAL_SLOW_MS
}
