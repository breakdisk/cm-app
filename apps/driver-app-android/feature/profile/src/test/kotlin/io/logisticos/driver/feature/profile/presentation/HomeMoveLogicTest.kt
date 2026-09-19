package io.logisticos.driver.feature.profile.presentation

import io.logisticos.driver.core.network.service.AddendumDto
import io.logisticos.driver.core.network.service.HomeCatalogueItemDto
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertNotNull
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test
import java.time.LocalDate
import java.time.ZoneOffset

class HomeMoveLogicTest {

    private val sofa = HomeCatalogueItemDto(key = "sofa_3", group = "living", name = "3-seat sofa", volumeL = 2000, weightKg = 60, assembly = true)
    private val box = HomeCatalogueItemDto(key = "box_m", group = "living", name = "Medium box", volumeL = 60, weightKg = 12)

    @Test
    fun `crew line matches the customer app`() {
        assertEquals("1 truck & 3-person crew", crewLine(1, 3))
        assertEquals("2 trucks & 5-person crew", crewLine(2, 5))
    }

    @Test
    fun `money shows pesos like the rest of the app and other currencies by code`() {
        assertEquals("₱1,250", money(125_000, "PHP"))
        assertEquals("₱12.50", money(1_250, "PHP"))
        assertEquals("AED 99.99", money(9_999, "AED"))
    }

    @Test
    fun `prices parse to minor units and nothing else does`() {
        assertEquals(25_000L, centsFromText("250"))
        assertEquals(25_050L, centsFromText("250.5"))
        assertEquals(125_050L, centsFromText("1,250.50"))
        assertNull(centsFromText(""))
        assertNull(centsFromText("12.345"))
        assertNull(centsFromText("-5"))
        assertNull(centsFromText("abc"))
    }

    @Test
    fun `adding an item takes the catalogue defaults, then counts up`() {
        var lines = addItem(emptyList(), "living", sofa)
        assertEquals(listOf(SurveyLine("living", "sofa_3", "3-seat sofa", 1, dismantle = true, packing = false)), lines)
        lines = addItem(lines, "living", sofa)
        assertEquals(2, qtyOf(lines, "living", "sofa_3"))
        // The same item in another room is its own line.
        lines = addItem(lines, "storage", sofa)
        assertEquals(2, lines.size)
    }

    @Test
    fun `quantity stops at the server's limit`() {
        var lines = emptyList<SurveyLine>()
        repeat(SURVEY_MAX_QTY + 5) { lines = addItem(lines, "living", box) }
        assertEquals(SURVEY_MAX_QTY, qtyOf(lines, "living", "box_m"))
    }

    @Test
    fun `removing counts down and drops the line at zero`() {
        var lines = addItem(addItem(emptyList(), "living", box), "living", box)
        lines = removeItem(lines, "living", "box_m")
        assertEquals(1, qtyOf(lines, "living", "box_m"))
        lines = removeItem(lines, "living", "box_m")
        assertTrue(lines.isEmpty())
        // Removing what isn't there changes nothing.
        assertTrue(removeItem(lines, "living", "box_m").isEmpty())
    }

    @Test
    fun `extras are held to the server's rules`() {
        val ok = ExtraDraft("material", "Wardrobe boxes", 4, 35_000)
        assertNull(extraProblem(ok))
        assertNotNull(extraProblem(ok.copy(kind = "discount")))
        assertNotNull(extraProblem(ok.copy(name = "  ")))
        assertNotNull(extraProblem(ok.copy(name = "x".repeat(81))))
        assertNotNull(extraProblem(ok.copy(qty = 0)))
        assertNotNull(extraProblem(ok.copy(qty = EXTRA_MAX_QTY + 1)))
        assertNotNull(extraProblem(ok.copy(unitCents = 0)))
        assertNotNull(extraProblem(ok.copy(unitCents = EXTRA_MAX_UNIT_CENTS + 1)))
    }

    @Test
    fun `an empty survey is allowed and too many extras is not`() {
        assertNull(surveyProblem(emptyList(), emptyList()))
        val many = List(SURVEY_MAX_EXTRAS + 1) { ExtraDraft("material", "Box $it", 1, 100) }
        assertNotNull(surveyProblem(emptyList(), many))
    }

    @Test
    fun `the request carries every line, trimmed`() {
        val req = surveyRequest(
            addItem(emptyList(), "living", sofa),
            listOf(ExtraDraft("resource", "  Hoist ", 1, 500_000)),
            "  Piano in the garage  ",
        )
        assertEquals("sofa_3", req.items.single().itemKey)
        assertTrue(req.items.single().dismantle)
        assertEquals("Hoist", req.extras.single().name)
        assertEquals(500_000L, req.extras.single().unitCents)
        assertEquals("Piano in the garage", req.note)
        assertEquals(700_000L, extrasTotal(listOf(ExtraDraft("material", "Box", 2, 350_000))))
    }

    @Test
    fun `the addendum reads from the lead's side`() {
        val a = AddendumDto(id = "a1", totalCents = 450_000, currency = "PHP", trucks = 2, crewTotal = 6, status = "pending")
        assertEquals("Waiting on the customer: +₱4,500", addendumLine(a))
        assertTrue(addendumLine(a.copy(status = "declined")).startsWith("Declined"))
        assertTrue(addendumLine(a.copy(status = "paid")).contains("2 trucks & 6-person crew"))
    }

    @Test
    fun `working days toggle and stay in order`() {
        assertEquals(listOf(0, 2, 5), toggleDay(listOf(5, 0), 2))
        assertEquals(listOf(0), toggleDay(listOf(0, 5), 5))
        assertEquals(listOf(1), toggleDay(listOf(1), 9))
    }

    @Test
    fun `days off are today onwards, a year at most, once each`() {
        val today = LocalDate.of(2026, 9, 19)
        assertNotNull(offDayProblem(today.minusDays(1), today))
        assertNull(offDayProblem(today, today))
        assertNull(offDayProblem(today.plusDays(OFF_DAY_HORIZON_DAYS), today))
        assertNotNull(offDayProblem(today.plusDays(OFF_DAY_HORIZON_DAYS + 1), today))
        assertEquals(
            listOf("2026-09-20", "2026-10-01"),
            addOffDay(addOffDay(listOf("2026-10-01"), LocalDate.of(2026, 9, 20)), LocalDate.of(2026, 10, 1)),
        )
    }

    @Test
    fun `dates read as days and times`() {
        assertEquals("Sat 26 Sep", dayLabel("2026-09-26"))
        assertEquals("not a date", dayLabel("not a date"))
        assertEquals("Sat 26 Sep · 8:00 AM", whenLabel("2026-09-26T00:00:00Z", ZoneOffset.ofHours(8)))
    }

    @Test
    fun `earnings lines are named by kind, and an unknown kind still reads`() {
        assertEquals("Waiting pay", adjustmentTitle("waiting_fee"))
        assertEquals("Dropped job fee", adjustmentTitle("drop_fee"))
        assertEquals("Survey fee", adjustmentTitle("survey_fee"))
        assertEquals("Support lead share", adjustmentTitle("support_share"))
        // Before this, anything that was not waiting pay read as a dropped-job fee.
        assertEquals("Loading bonus", adjustmentTitle("loading_bonus"))
        assertEquals("Adjustment", adjustmentTitle(""))
    }

    @Test
    fun `server messages are read out of the error body`() {
        assertEquals(
            "An addendum is already waiting on the customer for this move",
            serverMessage("""{"error":{"code":"CONFLICT","message":"An addendum is already waiting on the customer for this move"}}""", "x"),
        )
        assertEquals("fallback", serverMessage(null, "fallback"))
        assertEquals("fallback", serverMessage("<html>", "fallback"))
    }
}
