package net.cargomarket.omnideliv.courier.domain

import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertInstanceOf
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

class ManifestTest {

    private fun stop(seq: Int, name: String, pickedUp: Boolean = false) = Stop(
        stopRef = "v$seq",
        seq = seq,
        vendorName = name,
        address = "$seq Kalayaan Ave",
        lat = 14.55,
        lng = 121.02,
        vertical = if (seq == 2) "pharmacy" else "restaurant",
        prepTimeMinutes = 10,
        pickedUp = pickedUp,
        lines = listOf(Line(2, "Chicken Adobo Bowl", listOf("Size: Large"))),
    )

    private fun manifest(
        status: String = "collecting",
        stops: List<Stop> = listOf(stop(1, "Kuya's"), stop(2, "Mercury Drug")),
    ) = Manifest(
        orderId = "o1",
        status = status,
        codAmountCents = 38_900,
        tripCents = 3_500,
        tipCents = 0,
        stops = stops,
        dropoff = Dropoff("o1", 14.56, 121.03, "Maria Reyes", "639170000123", null),
    )

    @Test
    fun `the next leg is the lowest unpicked stop`() {
        val leg = manifest().currentLeg()
        assertInstanceOf(Leg.ToPickup::class.java, leg)
        assertEquals("Kuya's", (leg as Leg.ToPickup).stop.vendorName)
        assertEquals(2, leg.remainingPickups)
    }

    /**
     * The server sequences stops by readiness. Trusting array order would
     * silently reorder a route the moment a serialiser stopped preserving it —
     * and the courier would drive to the wrong vendor first.
     */
    @Test
    fun `stops are taken in seq order, not list order`() {
        val shuffled = listOf(stop(3, "Third"), stop(1, "First"), stop(2, "Second"))
        val leg = manifest(stops = shuffled).currentLeg()
        assertEquals("First", (leg as Leg.ToPickup).stop.vendorName)
    }

    @Test
    fun `a collected stop is skipped`() {
        val m = manifest(stops = listOf(stop(1, "Kuya's", pickedUp = true), stop(2, "Mercury Drug")))
        val leg = m.currentLeg()
        assertEquals("Mercury Drug", (leg as Leg.ToPickup).stop.vendorName)
        assertEquals(1, leg.remainingPickups)
    }

    @Test
    fun `every pickup collected means the customer is next`() {
        val m = manifest(
            stops = listOf(stop(1, "Kuya's", pickedUp = true), stop(2, "Mercury Drug", pickedUp = true)),
        )
        val leg = m.currentLeg()
        assertInstanceOf(Leg.ToDropoff::class.java, leg)
        assertEquals("Maria Reyes", (leg as Leg.ToDropoff).dropoff.customerName)
    }

    @Test
    fun `a delivered order has nothing left to do`() {
        val m = manifest(
            status = "delivered",
            stops = listOf(stop(1, "Kuya's", pickedUp = true)),
        )
        assertEquals(Leg.Done, m.currentLeg())
    }

    /**
     * The rail is what makes a mid-route re-sequence survivable: it animates
     * while the focus card and its button stay put. So it has to include the
     * dropoff, or the courier cannot see how much of the job is left.
     */
    @Test
    fun `the rail is every pickup plus the dropoff, in order`() {
        val r = manifest().rail()
        assertEquals(3, r.size)
        assertEquals(listOf("Kuya's", "Mercury Drug", "Maria Reyes"), r.map { it.label })
        assertTrue(r.last().isDropoff)
        assertEquals(listOf(1, 2, 3), r.map { it.seq })
    }

    @Test
    fun `the rail marks collected stops done`() {
        val m = manifest(stops = listOf(stop(1, "Kuya's", pickedUp = true), stop(2, "Mercury Drug")))
        val r = m.rail()
        assertTrue(r[0].done)
        assertTrue(!r[1].done)
        assertTrue(!r[2].done, "the dropoff is not done until the order is delivered")
    }

    @Test
    fun `an order with no customer name still labels the dropoff`() {
        val m = manifest().copy(
            dropoff = Dropoff("o1", 14.56, 121.03, null, null, null),
        )
        assertEquals("Customer", m.rail().last().label)
    }

    /**
     * COD is the customer's money passing through the courier's hands, not
     * earnings. Adding it here is the same sign error the backend ledger exists
     * to prevent.
     */
    @Test
    fun `earnings are the trip and the tip, never the cash collected`() {
        val m = manifest().copy(tripCents = 3_500, tipCents = 1_000, codAmountCents = 38_900)
        assertEquals(4_500, m.courierEarningsCents())
    }
}
