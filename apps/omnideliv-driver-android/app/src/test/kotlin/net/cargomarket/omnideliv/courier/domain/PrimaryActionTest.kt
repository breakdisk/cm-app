package net.cargomarket.omnideliv.courier.domain

import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

/**
 * What the one button at the bottom of the manifest does.
 *
 * This exists because the app shipped with that button **disabled** on the last
 * leg: the label said "Done", `enabled` was `currentLeg() != Leg.Done`, and the
 * shift screen had been popped off the back stack when the job was claimed. So
 * a courier who finished a delivery was left on a dead screen with a grey
 * button and no way out of it — at the end of every single job. Reported from a
 * real device on 2026-08-23.
 *
 * The rule is small enough to state as a function, and stating it here is what
 * stops it regressing behind a Compose preview nobody runs.
 */
class PrimaryActionTest {

    private fun pickup() = Leg.ToPickup(
        stop = Stop(
            stopRef = "vendor-1",
            seq = 1,
            vendorName = "Kuya's",
            address = "12 Mabini St",
            lat = 14.6,
            lng = 120.98,
            vertical = "food",
            prepTimeMinutes = 10,
            pickedUp = false,
            lines = emptyList(),
        ),
        remainingPickups = 1,
    )

    private fun dropoff() = Leg.ToDropoff(
        dropoff = Dropoff(
            stopRef = "order-1",
            lat = 14.61,
            lng = 120.99,
            customerName = "Ana",
            customerPhone = null,
            notes = null,
        ),
    )

    @Test
    fun `a pickup leg advances`() {
        val action = primaryAction(pickup())
        assertEquals("Picked up", action.label)
        assertEquals(PrimaryAction.Kind.Advance, action.kind)
        assertTrue(action.enabled)
    }

    @Test
    fun `a dropoff leg advances`() {
        val action = primaryAction(dropoff())
        assertEquals("Delivered", action.label)
        assertEquals(PrimaryAction.Kind.Advance, action.kind)
        assertTrue(action.enabled)
    }

    /**
     * The bug, stated as a rule: on the last leg the button is the way *out*,
     * so it must be enabled and it must mean "finish", not "advance".
     */
    @Test
    fun `a finished job offers a way back to the shift screen`() {
        val action = primaryAction(Leg.Done)

        assertTrue(action.enabled, "the only exit from a finished job must not be disabled")
        assertEquals(PrimaryAction.Kind.Finish, action.kind)
    }

    /** Nothing in the app may advance a milestone once the job is over. */
    @Test
    fun `a finished job never advances a milestone`() {
        assertEquals(PrimaryAction.Kind.Finish, primaryAction(Leg.Done).kind)
        listOf(pickup(), dropoff()).forEach {
            assertEquals(
                PrimaryAction.Kind.Advance,
                primaryAction(it).kind,
                "an unfinished leg must still advance",
            )
        }
    }
}
