package io.logisticos.driver.core.network.service

import kotlinx.serialization.json.Json
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertFalse
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

/** The shapes the server sends, read the way NetworkModule's Json reads them. */
class HomeMoveModelsTest {
    private val json = Json { ignoreUnknownKeys = true; isLenient = true; encodeDefaults = true }

    // A home claim answers with no assignment. Before this, the non-null
    // assignment_id made every won reservation fail to parse — and read as an error.
    @Test
    fun `a home claim parses as a reservation`() {
        val won = json.decodeFromString(
            ClaimOfferResponse.serializer(),
            """{"data":{"reserved":true,"shipment_id":"s1","move_date":"2026-09-26","assignment_id":null}}""",
        ).data
        assertTrue(won.reserved)
        assertNull(won.assignmentId)
        assertEquals("2026-09-26", won.moveDate)
    }

    @Test
    fun `a parcel claim still parses as an assignment`() {
        val won = json.decodeFromString(
            ClaimOfferResponse.serializer(),
            """{"data":{"assignment_id":"a1","shipment_id":"s2","payout_cents":12000}}""",
        ).data
        assertEquals("a1", won.assignmentId)
        assertFalse(won.reserved)
    }

    @Test
    fun `the lead's reservations parse`() {
        val moves = json.decodeFromString(
            HomeReservationsResponse.serializer(),
            """{"data":[{"shipment_id":"s1","tracking_number":null,"customer_name":"Ana","pickup":"12 Acacia St, Makati",
               "dropoff":"4 Palm Ave, Taguig","move_at":"2026-09-26T00:00:00Z","move_date":"2026-09-26",
               "survey_at":"2026-09-22T01:00:00Z","trucks":2,"helpers":4,"crew_total":6,"large_estate":true,
               "international":false,"activated":false}]}""",
        ).data
        assertEquals(6, moves.single().crewTotal)
        assertTrue(moves.single().largeEstate)
    }

    @Test
    fun `the move detail parses with its property under type`() {
        val d = json.decodeFromString(
            HomeMoveDetailResponse.serializer(),
            """{"data":{"shipment_id":"s1","tenant_id":"t","account_id":"u",
               "property":{"type":"villa","size":"3 bedroom","pickup_floor":0,"pickup_has_lift":true,"dropoff_floor":3,"dropoff_has_lift":false,"long_carry":false},
               "items":[{"room":"living","item_key":"sofa_3","name":"3-seat sofa","qty":1,"volume_l":2000,"weight_kg":60,"dismantle":true,"packing":false}],
               "plan":"trucks","distance_centikm":1840,"survey_required":true,"survey_at":null,"move_at":"2026-09-26T00:00:00Z",
               "total_cents":1500000,"currency":"PHP","trucks":1,"helpers":4,"crew_total":5,"large_estate":false,
               "international":false,"survey_cents":4500,"survey_submitted_at":null},"lead_name":"Idris Kamal"}""",
        )
        assertEquals("villa", d.data.property.type)
        assertEquals(3, d.data.property.dropoffFloor)
        assertEquals("sofa_3", d.data.items.single().itemKey)
        assertEquals("Idris Kamal", d.leadName)
    }

    @Test
    fun `a survey request is written in the server's field names`() {
        val body = json.encodeToString(
            SurveyRequest.serializer(),
            SurveyRequest(
                items = listOf(SurveyItemDto("living", "sofa_3", 1, dismantle = true, packing = false)),
                extras = listOf(SurveyExtraDto("material", "Boxes", 4, 35_000)),
                note = "",
            ),
        )
        assertTrue(body.contains("\"item_key\":\"sofa_3\""))
        assertTrue(body.contains("\"unit_cents\":35000"))
    }
}
