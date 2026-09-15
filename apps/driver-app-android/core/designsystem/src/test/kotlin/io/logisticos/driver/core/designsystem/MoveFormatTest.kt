package io.logisticos.driver.core.designsystem

import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Test

class MoveFormatTest {

    @Test
    fun `whole pesos drop the centavos and group thousands`() {
        assertEquals("₱214", pesos(21_400))
        assertEquals("₱1,470", pesos(147_000))
        assertEquals("₱214.50", pesos(21_450))
    }

    @Test
    fun `kilograms read naturally at both ends`() {
        assertEquals("1.5 kg", kilograms(1_500))
        assertEquals("1,470 kg", kilograms(1_470_000))
    }

    @Test
    fun `hours and minutes never go negative`() {
        assertEquals("6h 12m", hoursMinutes(372))
        assertEquals("45m", hoursMinutes(45))
        assertEquals("0m", hoursMinutes(-5))
    }
}
