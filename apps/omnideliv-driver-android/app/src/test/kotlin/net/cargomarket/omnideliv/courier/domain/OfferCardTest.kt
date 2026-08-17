package net.cargomarket.omnideliv.courier.domain

import kotlinx.serialization.json.Json
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertFalse
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

class OfferCardTest {

    private fun card(raw: String): OfferCard? = parseOfferCard(Json.parseToJsonElement(raw))

    private val full = """
        {"v":1,"stops":3,"pickups":2,"distance_m":4200,"deadline_hint_mins":38,
         "vendors":["Kuya's Lutong Bahay","Mercury Drug"],
         "verticals":["restaurant","pharmacy"],"temperature":["hot","chilled"]}
    """

    @Test
    fun `a v1 card reads every field`() {
        val c = card(full)!!
        assertEquals(3, c.stops)
        assertEquals(2, c.pickups)
        assertEquals(4_200, c.distanceM)
        assertEquals(38, c.deadlineHintMins)
        assertEquals(listOf("Kuya's Lutong Bahay", "Mercury Drug"), c.vendors)
        assertEquals(listOf("restaurant", "pharmacy"), c.verticals)
        assertEquals(listOf("hot", "chilled"), c.temperature)
        assertFalse(c.unknownVersion)
        assertFalse(c.isRetry)
    }

    @Test
    fun `the headline is what a courier reads at a glance`() {
        assertEquals("3 stops · 4.2 km", card(full)!!.headline())
    }

    @Test
    fun `one stop is not pluralised`() {
        assertEquals("1 stop · 0.9 km", card("""{"v":1,"stops":1,"distance_m":900}""")!!.headline())
    }

    /**
     * Distance rounds to the nearest hundred metres rather than truncating —
     * truncating would understate every distance a courier is offered, which is
     * the direction that matters when they are deciding whether it is worth it.
     *
     * Asserted on the whole headline, not a substring: an earlier version of
     * this test expected only "4.3 km" and failed because `headline()` leads
     * with the stop count. Checking the number without the string around it is
     * how a correct implementation reads as broken.
     */
    @Test
    fun `distance rounds rather than truncates`() {
        assertEquals(
            "2 stops · 4.3 km",
            card("""{"v":1,"stops":2,"distance_m":4250}""")!!.headline(),
        )
        assertEquals(
            "2 stops · 4.2 km",
            card("""{"v":1,"stops":2,"distance_m":4249}""")!!.headline(),
        )
    }

    /**
     * The version rule that matters. The backend can ship a v2 before this APK
     * is updated. Refusing the card entirely would blank a courier's inbox on a
     * backend deploy; pretending it is v1 would present a partial card as whole.
     * So it parses what it recognises and says the version was unknown.
     */
    @Test
    fun `an unknown version still parses but is flagged`() {
        val c = card("""{"v":2,"stops":4,"pickups":3,"distance_m":6000,"surge_cents":500}""")!!
        assertEquals(4, c.stops)
        assertEquals(3, c.pickups)
        assertTrue(c.unknownVersion, "the UI must be able to say some detail may be missing")
    }

    /** A card with no version did not come from a writer that agreed to this contract. */
    @Test
    fun `a missing version counts as unknown`() {
        assertTrue(card("""{"stops":2,"pickups":1}""")!!.unknownVersion)
    }

    @Test
    fun `an absent card is a legitimate state`() {
        assertNull(parseOfferCard(null))
    }

    /**
     * This runs while rendering a list. One malformed card must not take the
     * whole inbox down, so nothing here may throw.
     */
    @Test
    fun `malformed input yields null rather than throwing`() {
        assertNull(parseOfferCard(Json.parseToJsonElement("""[1,2,3]""")))
        assertNull(parseOfferCard(Json.parseToJsonElement(""""just a string"""")))
        assertNull(card("""{}"""))
        assertNull(card("""{"v":1}"""))
    }

    /** Wrong types in the right keys are a server bug, not a reason to crash. */
    @Test
    fun `wrong types degrade to absent fields`() {
        val c = card("""{"v":1,"stops":"three","pickups":2,"vendors":"Kuya's"}""")
        assertEquals(2, c!!.pickups)
        assertNull(c.stops, "a non-numeric stop count is dropped, not guessed")
        assertEquals(emptyList<String>(), c.vendors)
    }

    /**
     * The recovery sweep re-offers with a reduced card: no vendor names, because
     * that service holds none. It must still render as a real offer rather than
     * being discarded as empty.
     */
    @Test
    fun `a retry card with no vendors is still worth showing`() {
        val c = card("""{"v":1,"stops":3,"pickups":2,"vendors":[],"retry":true}""")!!
        assertTrue(c.isRetry)
        assertEquals(2, c.pickups)
        assertEquals("3 stops", c.headline(), "no distance means no distance in the headline")
    }

    /** Blank vendor names would render as empty chips. */
    @Test
    fun `blank strings are dropped from lists`() {
        val c = card("""{"v":1,"stops":2,"vendors":["Kuya's","  ",""]}""")!!
        assertEquals(listOf("Kuya's"), c.vendors)
    }

    /**
     * The privacy rule, asserted from the consumer's side. `offer_to_nearest`
     * fans out, so the card reaches couriers who decline — and this type has no
     * field that could hold a customer or an address even if the server sent one.
     */
    @Test
    fun `the type cannot carry a customer or an address`() {
        val c = card(
            """{"v":1,"stops":2,"pickups":1,"customer_name":"Maria Reyes",
                "lat":14.55,"lng":121.02,"address":"Unit 12B"}""",
        )!!
        val rendered = c.toString().lowercase()
        for (leaked in listOf("maria", "14.55", "121.02", "unit 12b")) {
            assertFalse(rendered.contains(leaked), "OfferCard must not surface `$leaked`")
        }
    }

    @Test
    fun `the declared version is the one the server writes`() {
        assertEquals(1, OFFER_CARD_VERSION)
    }
}
