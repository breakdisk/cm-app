package io.logisticos.driver.core.location

import org.junit.jupiter.api.Test
import org.junit.jupiter.api.Assertions.*

class AdaptiveLocationManagerTest {

    @Test
    fun `driving interval is 5s`() {
        // Was 2 s, which with the service's setMinUpdateIntervalMillis(interval/2)
        // allowed a fix every second at PRIORITY_HIGH_ACCURACY — continuous GNSS
        // for a whole shift. 5 s still leaves ~80 m spacing at 60 km/h, which is
        // ample to reconstruct a route for chain-of-custody purposes.
        val interval = AdaptiveLocationManager.intervalForSpeed(speedMps = 2.0f) // ~7.2 km/h
        assertEquals(5_000L, interval)
    }

    @Test
    fun `slow interval applies between 0 and 5kmh`() {
        assertEquals(15_000L, AdaptiveLocationManager.intervalForSpeed(speedMps = 1.0f))
    }

    @Test
    fun `slow interval applies at exactly 0`() {
        assertEquals(15_000L, AdaptiveLocationManager.intervalForSpeed(speedMps = 0.0f))
    }

    @Test
    fun `threshold itself is not treated as driving`() {
        // Strict greater-than: exactly at the threshold stays on the slow interval.
        assertEquals(15_000L, AdaptiveLocationManager.intervalForSpeed(speedMps = 1.39f))
    }

    @Test
    fun `just above the threshold switches to driving`() {
        assertEquals(5_000L, AdaptiveLocationManager.intervalForSpeed(speedMps = 1.40f))
    }

    @Test
    fun `stationary threshold is 2 minutes`() {
        assertEquals(120_000L, AdaptiveLocationManager.STATIONARY_THRESHOLD_MS)
    }

    @Test
    fun `stationary interval is 30s`() {
        // Pinned because a HomeViewModel comment previously claimed ~60 s. The
        // constant and its documentation drifting apart is how a sampling policy
        // stops being reviewable.
        assertEquals(30_000L, AdaptiveLocationManager.INTERVAL_STATIONARY_MS)
    }

    @Test
    fun `sampling gets less frequent as the driver slows`() {
        val driving = AdaptiveLocationManager.intervalForSpeed(speedMps = 10.0f)
        val slow = AdaptiveLocationManager.intervalForSpeed(speedMps = 0.5f)
        assertTrue(driving < slow, "driving must sample more often than slow")
        assertTrue(
            slow < AdaptiveLocationManager.INTERVAL_STATIONARY_MS,
            "slow must sample more often than stationary",
        )
    }
}
