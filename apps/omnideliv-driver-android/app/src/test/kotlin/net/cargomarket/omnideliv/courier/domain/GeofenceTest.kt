package net.cargomarket.omnideliv.courier.domain

import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertFalse
import org.junit.jupiter.api.Assertions.assertInstanceOf
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

class GeofenceTest {

    // A real Manila pair, ~30 m apart.
    private val stopLat = 14.5547
    private val stopLng = 121.0244

    @Test
    fun `standing on the pin reads as at the stop`() {
        val a = adviseGeofence(stopLat, stopLng, 5, stopLat, stopLng)
        assertInstanceOf(GeofenceAdvice.AtStop::class.java, a)
        assertEquals(0, (a as GeofenceAdvice.AtStop).meters)
    }

    /**
     * Two identical points can put the haversine argument a hair above 1 in
     * floating point, and `asin` of that is NaN — a blank distance at exactly
     * the moment the courier is standing on the pin.
     */
    @Test
    fun `an identical point does not produce NaN`() {
        val m = haversineMeters(stopLat, stopLng, stopLat, stopLng)
        assertFalse(m.isNaN(), "haversine must not return NaN for a zero distance")
        assertEquals(0.0, m, 0.001)
    }

    @Test
    fun `a courier down the street is advised, not blocked`() {
        // ~0.01 degrees of latitude is roughly 1.1 km.
        val a = adviseGeofence(stopLat + 0.01, stopLng, 5, stopLat, stopLng)
        assertInstanceOf(GeofenceAdvice.Away::class.java, a)
        assertTrue((a as GeofenceAdvice.Away).meters > 1000)
    }

    /**
     * The decision this app is built on. A hard gate strands a courier at the
     * door in a lift lobby, and with cash on delivery the money is already in
     * their hand — refusing the button cannot un-collect it.
     */
    @Test
    fun `no fix is advisory, never a refusal`() {
        assertInstanceOf(
            GeofenceAdvice.NoFix::class.java,
            adviseGeofence(null, null, null, stopLat, stopLng),
        )
        // There is deliberately no "blocked" variant to assert against: the
        // type cannot express refusing the commit.
    }

    /**
     * field-ops answers with the newest fix it holds however old that is, so
     * freshness is this layer's job. A courier whose phone lost GPS twenty
     * minutes ago is not "at the stop" because they were once.
     */
    @Test
    fun `a stale fix is not a position`() {
        val fresh = adviseGeofence(stopLat, stopLng, FIX_STALE_AFTER_SECS, stopLat, stopLng)
        assertInstanceOf(GeofenceAdvice.AtStop::class.java, fresh)

        val stale = adviseGeofence(stopLat, stopLng, FIX_STALE_AFTER_SECS + 1, stopLat, stopLng)
        assertInstanceOf(GeofenceAdvice.NoFix::class.java, stale)
    }

    @Test
    fun `only a known distance beyond the radius is flagged for ops`() {
        assertTrue(isOutOfBounds(GeofenceAdvice.Away(840)))
        assertFalse(isOutOfBounds(GeofenceAdvice.AtStop(12)))
        // A dead GPS is not the courier's fault, and flagging every basement
        // would make the flag mean nothing.
        assertFalse(isOutOfBounds(GeofenceAdvice.NoFix))
    }

    @Test
    fun `the radius boundary is inclusive`() {
        // ~0.00045 degrees of latitude is about 50 m.
        val justInside = adviseGeofence(stopLat + 0.00040, stopLng, 5, stopLat, stopLng)
        assertInstanceOf(GeofenceAdvice.AtStop::class.java, justInside)

        val justOutside = adviseGeofence(stopLat + 0.00060, stopLng, 5, stopLat, stopLng)
        assertInstanceOf(GeofenceAdvice.Away::class.java, justOutside)
    }
}
