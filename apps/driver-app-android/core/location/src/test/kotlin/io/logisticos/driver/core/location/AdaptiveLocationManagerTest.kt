package io.logisticos.driver.core.location

import org.junit.jupiter.api.Test
import org.junit.jupiter.api.Assertions.*

class AdaptiveLocationManagerTest {

    @Test
    fun `returns 2000ms interval when speed above 5kmh`() {
        val interval = AdaptiveLocationManager.intervalForSpeed(speedMps = 2.0f) // ~7.2 km/h
        assertEquals(2000L, interval)
    }

    @Test
    fun `returns 15000ms interval when speed between 0 and 5kmh`() {
        val interval = AdaptiveLocationManager.intervalForSpeed(speedMps = 1.0f) // ~3.6 km/h
        assertEquals(15000L, interval)
    }

    @Test
    fun `returns 15000ms interval when speed is exactly 0`() {
        val interval = AdaptiveLocationManager.intervalForSpeed(speedMps = 0.0f)
        assertEquals(15000L, interval)
    }

    @Test
    fun `stationary threshold is 2 minutes`() {
        assertEquals(120_000L, AdaptiveLocationManager.STATIONARY_THRESHOLD_MS)
    }
}
