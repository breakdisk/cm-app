package io.logisticos.driver.core.location

object AdaptiveLocationManager {
    const val STATIONARY_THRESHOLD_MS = 120_000L  // 2 minutes
    private const val SPEED_THRESHOLD_MPS = 1.39f  // 5 km/h in m/s
    private const val INTERVAL_DRIVING_MS = 2_000L
    private const val INTERVAL_SLOW_MS = 15_000L
    const val INTERVAL_STATIONARY_MS = 30_000L

    fun intervalForSpeed(speedMps: Float): Long =
        if (speedMps > SPEED_THRESHOLD_MPS) INTERVAL_DRIVING_MS else INTERVAL_SLOW_MS
}
