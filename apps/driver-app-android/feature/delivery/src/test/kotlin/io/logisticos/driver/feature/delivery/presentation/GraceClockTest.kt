package io.logisticos.driver.feature.delivery.presentation

import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Test

class GraceClockTest {

    @Test
    fun `the countdown is anchored on the server clock, not the phone's`() {
        // The server says it is 10:00 and grace ends 10:45. Whatever the phone
        // thinks the time is, 45 minutes are left.
        val clock = GraceClock.from("2026-09-18T10:45:00Z", "2026-09-18T10:00:00Z", fetchedAtElapsedMs = 5_000)!!
        assertEquals(45 * 60_000L, clock.remainingMs(5_000))
        assertEquals(44 * 60_000L, clock.remainingMs(65_000))
    }

    @Test
    fun `the server's fractional seconds parse`() {
        val clock = GraceClock.from("2026-09-18T10:45:00.250000Z", "2026-09-18T10:00:00.250000Z", 0)!!
        assertEquals(45 * 60_000L, clock.remainingMs(0))
    }

    @Test
    fun `no deadline means no clock`() {
        assertNull(GraceClock.from(null, "2026-09-18T10:00:00Z", 0))
        assertNull(GraceClock.from("not a time", "2026-09-18T10:00:00Z", 0))
    }

    @Test
    fun `the clock turns amber in the last ten minutes and red past the deadline`() {
        assertEquals(GraceTone.Running, graceTone(CLOSING_WINDOW_MS + 1))
        assertEquals(GraceTone.Closing, graceTone(CLOSING_WINDOW_MS))
        assertEquals(GraceTone.Closing, graceTone(1))
        assertEquals(GraceTone.Over, graceTone(0))
        assertEquals(GraceTone.Over, graceTone(-60_000))
    }

    @Test
    fun `it reads as the design shows it`() {
        assertEquals("45:00", formatGrace(45 * 60_000L))
        assertEquals("44:59", formatGrace(44 * 60_000L + 59_000))
        assertEquals("0:01", formatGrace(1))
        assertEquals("+3:10", formatGrace(-(3 * 60_000L + 10_000)))
        assertEquals("1:02:03", formatGrace(3_723_000))
    }
}
