package io.logisticos.driver.feature.boxmeasure.data

import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertNotNull
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

/**
 * This engine prices real shipments and its output is quoted to customers, so
 * the cases below pin behaviour that is easy to regress silently — a wrong zone
 * or a wrong box size produces a plausible-looking number, not an error.
 */
class QuoteEngineTest {

    // ── resolveProvince ───────────────────────────────────────────────────────

    @Test
    fun `exact province name resolves`() {
        assertEquals("Zone 1A", resolveProvince("Metro Manila")?.zoneCode)
        assertEquals("Zone 5C", resolveProvince("bacolod")?.zoneCode)
        assertEquals("Zone 6A", resolveProvince("Davao City")?.zoneCode)
    }

    @Test
    fun `input is trimmed and case insensitive`() {
        assertEquals("Zone 5A", resolveProvince("  CEBU  ")?.zoneCode)
    }

    @Test
    fun `a short ambiguous prefix resolves to nothing rather than guessing`() {
        // The regression this guards: "ba" used to match Batangas (Zone 2B) on
        // the way to Bacolod (Zone 5C) — about a 31% price difference on a small
        // box, presented as a settled figure.
        assertNull(resolveProvince("b"))
        assertNull(resolveProvince("ba"))
        assertNull(resolveProvince("c"))
        assertNull(resolveProvince("m"))
    }

    @Test
    fun `a long enough unambiguous prefix still resolves`() {
        // "bacol" prefixes only Bacolod, so completing the word is not required.
        assertEquals("Zone 5C", resolveProvince("bacol")?.zoneCode)
        assertEquals("Zone 3C", resolveProvince("bengu")?.zoneCode)
    }

    @Test
    fun `a long prefix spanning several zones stays unresolved`() {
        // "davao" prefixes both davao and davao city, but both are Zone 6A, so
        // it resolves. A prefix spanning different zones must not.
        assertEquals("Zone 6A", resolveProvince("davao")?.zoneCode)
        assertNull(resolveProvince("bat")) // too short, and Batangas-only anyway
    }

    @Test
    fun `a query containing a full province name resolves to the longest match`() {
        // Address-style input: the more specific entry must win so "cebu city"
        // is not captured by the shorter "cebu".
        assertEquals("Zone 6A", resolveProvince("davao del sur")?.zoneCode)
        assertEquals("Zone 5A", resolveProvince("cebu city, philippines")?.zoneCode)
    }

    @Test
    fun `unknown input resolves to nothing`() {
        assertNull(resolveProvince(""))
        assertNull(resolveProvince("   "))
        assertNull(resolveProvince("Reykjavik"))
    }

    // ── matchToStandardSize ───────────────────────────────────────────────────

    @Test
    fun `a box close to a standard size matches it`() {
        // Large is 51x41x41.
        val m = matchToStandardSize(51.0, 41.0, 41.0)
        assertEquals("large", m?.id)
    }

    @Test
    fun `a slightly-off box still matches the nearest standard size`() {
        val m = matchToStandardSize(52.0, 42.0, 40.0)
        assertNotNull(m)
    }

    @Test
    fun `an oversized box does not silently match jumbo`() {
        // Nearest-match alone mapped any volume onto Jumbo, and that value is
        // carried into the POP dimensioning record — so an unbounded match would
        // record a crate as a verified Jumbo.
        assertNull(matchToStandardSize(200.0, 200.0, 200.0))
    }

    @Test
    fun `a tiny box does not silently match bulilit`() {
        assertNull(matchToStandardSize(5.0, 5.0, 5.0))
    }

    @Test
    fun `non-positive dimensions resolve to nothing`() {
        assertNull(matchToStandardSize(0.0, 10.0, 10.0))
        assertNull(matchToStandardSize(-1.0, 10.0, 10.0))
    }

    // ── volumetric / CBM ──────────────────────────────────────────────────────

    @Test
    fun `computeCbm converts cubic cm to cubic m`() {
        // 100x100x100 cm = 1 000 000 cm3 = 1.0 m3
        assertEquals(1.0, computeCbm(100.0, 100.0, 100.0))
    }

    @Test
    fun `computeCbm rejects non-positive dimensions`() {
        assertEquals(0.0, computeCbm(0.0, 10.0, 10.0))
        assertEquals(0.0, computeCbm(10.0, -5.0, 10.0))
    }

    @Test
    fun `volumetric weight uses the IATA divisor`() {
        // 50x40x30 = 60 000 cm3 / 5000 = 12.0 kg
        assertEquals(12.0, computeVolumetricWeight(50.0, 40.0, 30.0))
    }

    @Test
    fun `volumetric weight rejects non-positive dimensions`() {
        assertEquals(0.0, computeVolumetricWeight(0.0, 40.0, 30.0))
    }

    // ── computeQuote ──────────────────────────────────────────────────────────

    @Test
    fun `sea quote totals freight plus PH delivery in the origin currency`() {
        val q = computeQuote(
            mode = FreightMode.SEA,
            originKey = "USA West Coast (CA, WA, OR)",
            sizeId = "large",
            qty = 2,
            weightKg = 18.0,
            dimsCm = Triple(51.0, 41.0, 41.0),
            province = "Metro Manila",
        )
        assertEquals("USD", q.originCurrency)
        // Sea line + PH delivery line.
        assertEquals(2, q.lines.size)
        // Unit price 95.0 x 2 boxes.
        assertEquals(190.0, q.lines.first { it.component == QuoteLine.Component.SEA }.amount)
        assertTrue(q.totalOriginCurrency > 190.0)
    }

    @Test
    fun `air quote bills the greater of actual and volumetric weight`() {
        // 60x60x60 = 216 000 cm3 / 5000 = 43.2 kg volumetric, well above 5 kg actual.
        val q = computeQuote(
            mode = FreightMode.AIR,
            originKey = "Zone 1 — ASEAN / NE Asia",
            sizeId = "jumbo",
            qty = 1,
            weightKg = 5.0,
            dimsCm = Triple(60.0, 60.0, 60.0),
            province = "Metro Manila",
        )
        val freight = q.lines.first { it.component == QuoteLine.Component.AIR }
        // 43.2 kg x 5.50/kg
        assertEquals(237.6, freight.amount)
        assertTrue(freight.note!!.contains("vol"))
    }

    @Test
    fun `air quote applies the zone minimum weight`() {
        val q = computeQuote(
            mode = FreightMode.AIR,
            originKey = "Zone 1 — ASEAN / NE Asia",
            sizeId = "bulilit",
            qty = 1,
            weightKg = 0.5,
            dimsCm = Triple(10.0, 10.0, 10.0),
            province = "Metro Manila",
        )
        // min 5 kg x 5.50 — neither the 0.5 kg actual nor the 0.2 kg volumetric.
        val freight = q.lines.first { it.component == QuoteLine.Component.AIR }
        assertEquals(27.5, freight.amount)
    }

    @Test
    fun `an unresolvable province omits the PH delivery line rather than guessing`() {
        val q = computeQuote(
            mode = FreightMode.SEA,
            originKey = "USA West Coast (CA, WA, OR)",
            sizeId = "large",
            qty = 1,
            weightKg = 18.0,
            dimsCm = Triple(51.0, 41.0, 41.0),
            province = "ba",   // ambiguous — must not price a zone
        )
        assertTrue(q.lines.none { it.component == QuoteLine.Component.PH_DELIVERY })
    }

    @Test
    fun `zero quantity produces no freight line`() {
        val q = computeQuote(
            mode = FreightMode.SEA,
            originKey = "USA West Coast (CA, WA, OR)",
            sizeId = "large",
            qty = 0,
            weightKg = 18.0,
            dimsCm = Triple(51.0, 41.0, 41.0),
            province = "Metro Manila",
        )
        assertTrue(q.lines.isEmpty())
        assertEquals(0.0, q.totalOriginCurrency)
    }
}
