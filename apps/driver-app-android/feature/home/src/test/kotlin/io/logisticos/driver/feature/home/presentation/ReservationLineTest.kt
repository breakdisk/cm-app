package io.logisticos.driver.feature.home.presentation

import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

class ReservationLineTest {

    @Test
    fun `names the move's day`() {
        assertTrue(reservationLine("2026-09-26").startsWith("The move on Sat 26 Sep is yours."))
    }

    @Test
    fun `a captain is told a second truck is being found`() {
        assertTrue(reservationLine("2026-09-26", "captain").contains("Mission Captain"))
        assertTrue(reservationLine("2026-09-26", "support").contains("paid your share on delivery"))
    }

    @Test
    fun `reads generic without a day, and as sent when unparseable`() {
        assertTrue(reservationLine(null).startsWith("The move is yours."))
        assertTrue(reservationLine("").startsWith("The move is yours."))
        assertTrue(reservationLine("soon").startsWith("The move on soon is yours."))
    }
}
